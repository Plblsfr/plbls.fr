use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row;

use crate::{
    ajouts::{self, COLONNES_AJOUT, ajout_public, etat_interne, non_configure, reponse_erreur},
    bibliotheque::{COLONNES_ALBUM, album_public, uuid},
    catalogue,
    etat::AppState,
    garde::Personne,
    http::{bd, erreur, indisponible, invalide, ok, sans_contenu},
    lidarr, quota,
};

type Params = Query<HashMap<String, String>>;

pub async fn disque(State(state): State<AppState>) -> Result<Response, Response> {
    let o = quota::occupation(&state).await.map_err(bd)?;
    let mut v = o.public();
    v["daily_limit"] = json!(state.config().ajouts.par_jour);
    v["configured"] = json!(state.config().lidarr.is_some() && state.config().ajouts.configure());
    Ok(ok(&v))
}

pub async fn liste(State(state): State<AppState>, Query(q): Params) -> Result<Response, Response> {
    let etat = match q.get("state").filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => Some(etat_interne(s).ok_or_else(|| invalide("état inconnu"))?),
    };
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_AJOUT} FROM ajouts a WHERE ($1::text IS NULL OR a.etat = $1)
         ORDER BY a.cree_le DESC, a.id LIMIT 500"
    )))
    .bind(etat)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| ajout_public(l, true)).collect::<Vec<_>>(),
    })))
}

pub async fn relancer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let r = ajouts::relancer(&state, id).await.map_err(reponse_erreur)?;
    ajouts::reponse_resultat(&state, r, true).await
}

async fn supprimer_dans_lidarr(state: &AppState, chemin: &str) -> Result<(), Response> {
    let l = state.config().lidarr.as_ref().ok_or_else(non_configure)?;
    let c = lidarr::Client::new(l, state.http());
    let rep = c
        .requete(Method::DELETE, chemin)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("Lidarr : {}", e.without_url());
            indisponible("lidarr_unavailable", "Lidarr injoignable")
        })?;
    let s = rep.status();
    if s.is_success() || s == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    tracing::warn!("Lidarr : suppression HTTP {}", s.as_u16());
    Err(indisponible(
        "lidarr_unavailable",
        &format!("Lidarr : HTTP {}", s.as_u16()),
    ))
}

async fn supprimer(
    state: AppState,
    p: Personne,
    id: String,
    genre: ajouts::Genre,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let (table, chemin, colonne) = match genre {
        ajouts::Genre::Album => ("albums", "album", "titre"),
        ajouts::Genre::Artiste => ("artistes", "artist", "nom"),
    };
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT mbid, lidarr_id, {colonne} FROM {table} WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(crate::http::introuvable)?;
    let (mbid, lidarr_id, titre): (String, Option<i64>, String) = (l.get(0), l.get(1), l.get(2));
    let lidarr_id = lidarr_id
        .ok_or_else(|| erreur(StatusCode::CONFLICT, "not_in_lidarr", "inconnu de Lidarr"))?;
    supprimer_dans_lidarr(
        &state,
        &format!("/{chemin}/{lidarr_id}?deleteFiles=true&addImportListExclusion=true"),
    )
    .await?;
    sqlx::query(
        "INSERT INTO exclusions (mbid, type, titre, par) VALUES ($1, $2, $3, $4)
         ON CONFLICT (mbid) DO UPDATE SET par = EXCLUDED.par, le = now()",
    )
    .bind(&mbid)
    .bind(genre.code())
    .bind(&titre)
    .bind(p.sub())
    .execute(state.pool())
    .await
    .map_err(bd)?;
    let cond = match genre {
        ajouts::Genre::Album => "type = 'album' AND mbid = $1",
        ajouts::Genre::Artiste => "artiste_mbid = $1",
    };
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE ajouts SET etat = 'supprime', maj = now() WHERE etat <> 'supprime' AND {cond}"
    )))
    .bind(&mbid)
    .execute(state.pool())
    .await
    .map_err(bd)?;
    catalogue::synchroniser(&state).await;
    Ok(sans_contenu())
}

pub async fn supprimer_album(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    supprimer(state, p, id, ajouts::Genre::Album).await
}

pub async fn supprimer_artiste(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    supprimer(state, p, id, ajouts::Genre::Artiste).await
}

pub async fn albums(State(state): State<AppState>, Query(q): Params) -> Result<Response, Response> {
    let n: i64 = match q.get("limit").filter(|v| !v.is_empty()) {
        None => 100,
        Some(v) => v
            .parse()
            .ok()
            .filter(|n| (1..=500).contains(n))
            .ok_or_else(|| invalide("limite invalide"))?,
    };
    let jamais_ecoutes = match q.get("never_played").map(String::as_str) {
        None | Some("" | "false") => false,
        Some("true") => true,
        Some(_) => return Err(invalide("never_played : true ou false")),
    };
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ALBUM}, t.taille,
           (SELECT a.nom FROM ajouts a WHERE al.mbid = ANY(a.albums) ORDER BY a.cree_le DESC LIMIT 1) AS ajoute_par
         FROM albums al JOIN artistes ar ON ar.id = al.artiste_id
         JOIN LATERAL (SELECT coalesce(sum(f.taille), 0)::bigint AS taille FROM fichiers f JOIN pistes p ON p.id = f.piste_id
                       WHERE p.album_id = al.id) t ON true
         WHERE al.present
           AND NOT ($2 AND EXISTS (SELECT 1 FROM ecoutes e JOIN pistes p ON p.id = e.piste_id WHERE p.album_id = al.id AND e.compte))
         ORDER BY t.taille DESC, al.id LIMIT $1"
    )))
    .bind(n)
    .bind(jamais_ecoutes)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let items: Vec<Value> = lignes
        .iter()
        .map(|l| {
            let mut v = album_public(l);
            v["size"] = json!(l.get::<i64, _>("taille"));
            v["added_by"] = json!(l.get::<Option<String>, _>("ajoute_par"));
            v
        })
        .collect();
    Ok(ok(&json!({ "items": items })))
}
