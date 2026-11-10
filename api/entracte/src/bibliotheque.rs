use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    etat::AppState,
    ffprobe::Analyse,
    garde::Personne,
    http::{self, bd, horodatage, introuvable, invalide, ok},
    images,
};

pub const PALIERS: [(&str, i64); 3] = [("1080p", 1080), ("720p", 720), ("480p", 480)];

pub const COLONNES: &str = "t.id, t.genre, t.titre, t.titre_original, t.annee, t.resume, t.genres, t.saison, t.episode,
    t.serie_id, s.titre AS serie_titre, (t.affiche IS NOT NULL OR s.affiche IS NOT NULL) AS a_affiche,
    (t.fond IS NOT NULL OR s.fond IS NOT NULL) AS a_fond, t.duree, t.present,
    p.position, p.duree AS p_duree, v.vu_le";

pub fn jointures(sub_param: &str) -> String {
    format!(
        "FROM titres t LEFT JOIN titres s ON s.id = t.serie_id
         LEFT JOIN progression p ON p.titre_id = t.id AND p.sub = {sub_param}
         LEFT JOIN vus v ON v.titre_id = t.id AND v.sub = {sub_param}"
    )
}

pub fn genre_public(g: &str) -> &'static str {
    match g {
        "film" => "movie",
        "serie" => "series",
        _ => "episode",
    }
}

pub fn public(l: &PgRow) -> Value {
    let position: Option<f64> = l.get("position");
    let vu: Option<DateTime<Utc>> = l.get("vu_le");
    json!({
        "id": l.get::<Uuid, _>("id"),
        "kind": genre_public(l.get::<String, _>("genre").as_str()),
        "title": l.get::<String, _>("titre"),
        "original_title": l.get::<Option<String>, _>("titre_original"),
        "year": l.get::<Option<i32>, _>("annee"),
        "overview": l.get::<Option<String>, _>("resume"),
        "genres": l.get::<Vec<String>, _>("genres"),
        "season": l.get::<Option<i32>, _>("saison"),
        "episode": l.get::<Option<i32>, _>("episode"),
        "series_id": l.get::<Option<Uuid>, _>("serie_id"),
        "series_title": l.get::<Option<String>, _>("serie_titre"),
        "poster": l.get::<bool, _>("a_affiche"),
        "backdrop": l.get::<bool, _>("a_fond"),
        "duration": l.get::<Option<f64>, _>("duree"),
        "available": l.get::<bool, _>("present"),
        "progress": position.map(|p| json!({
            "position": p,
            "duration": l.get::<Option<f64>, _>("p_duree"),
            "seen": vu.is_some(),
        })),
        "seen_at": vu.as_ref().map(horodatage),
    })
}

pub fn uuid(id: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(id).map_err(|_| introuvable())
}

fn annees(texte: &str) -> Option<(i32, i32)> {
    let (a, b) = texte.split_once('-').unwrap_or((texte, texte));
    let (a, b) = (a.trim().parse::<i32>().ok()?, b.trim().parse::<i32>().ok()?);
    let ok = |x: i32| (1880..=2100).contains(&x);
    (ok(a) && ok(b) && a <= b).then_some((a, b))
}

fn entier(q: &HashMap<String, String>, nom: &str, defaut: i64, max: i64) -> Result<i64, Response> {
    match q.get(nom).filter(|v| !v.is_empty()) {
        None => Ok(defaut),
        Some(v) => v
            .parse::<i64>()
            .ok()
            .filter(|n| (0..=max).contains(n))
            .ok_or_else(|| invalide("pagination invalide")),
    }
}

pub async fn liste(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let genres: &[&str] = match q.get("kind").map(String::as_str) {
        None | Some("") => &["film", "serie"],
        Some("movie") => &["film"],
        Some("series") => &["serie"],
        Some(_) => return Err(invalide("type inconnu")),
    };
    let recherche = q.get("q").map(|t| t.trim()).filter(|t| !t.is_empty());
    if recherche.is_some_and(|t| t.chars().count() > 100) {
        return Err(invalide("recherche trop longue"));
    }
    let genre = q.get("genre").filter(|g| !g.is_empty());
    if genre.is_some_and(|g| g.chars().count() > 64) {
        return Err(invalide("genre trop long"));
    }
    let (de, a) = match q.get("year").filter(|a| !a.is_empty()) {
        None => (0, 9999),
        Some(t) => annees(t).ok_or_else(|| invalide("année invalide"))?,
    };
    let tri = match q.get("sort").map(String::as_str) {
        None | Some("") | Some("recent") => "t.present_le DESC NULLS LAST, t.titre",
        Some("title") => "lower(t.titre), t.annee",
        Some("year") => "t.annee DESC NULLS LAST, lower(t.titre)",
        Some(_) => return Err(invalide("tri inconnu")),
    };
    let debut = entier(&q, "start", 0, 100_000)?;
    let limite = entier(&q, "limit", 40, 100)?.max(1);
    let motif = recherche.map(commun::sql::contient);
    let filtre = "WHERE t.present AND t.genre = ANY($2) AND ($3::text IS NULL OR t.titre ILIKE $3 OR t.titre_original ILIKE $3)
        AND ($4::text IS NULL OR $4 = ANY(t.genres)) AND coalesce(t.annee, 0) BETWEEN $5 AND $6";
    let genres: Vec<String> = genres.iter().map(|g| g.to_string()).collect();
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES}, count(*) OVER () AS total {} {filtre} ORDER BY {tri} OFFSET $7 LIMIT $8",
        jointures("$1")
    )))
    .bind(p.sub())
    .bind(&genres)
    .bind(&motif)
    .bind(genre)
    .bind(de)
    .bind(a)
    .bind(debut)
    .bind(limite)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let total = match lignes.first() {
        Some(l) => l.get::<i64, _>("total"),
        None => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) {} {filtre}",
            jointures("$1")
        )))
        .bind(p.sub())
        .bind(&genres)
        .bind(&motif)
        .bind(genre)
        .bind(de)
        .bind(a)
        .fetch_one(state.pool())
        .await
        .map_err(bd)?,
    };
    Ok(ok(&json!({
        "items": lignes.iter().map(public).collect::<Vec<_>>(),
        "total": total,
    })))
}

pub async fn genres(State(state): State<AppState>) -> Result<Response, Response> {
    let noms: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT g FROM titres, unnest(genres) g WHERE present AND genre <> 'episode' ORDER BY g",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({ "genres": noms })))
}

pub async fn ligne(state: &AppState, sub: &str, id: Uuid) -> Result<PgRow, Response> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES} {} WHERE t.id = $2",
        jointures("$1")
    )))
    .bind(sub)
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)
}

pub async fn fichier_de(
    state: &AppState,
    titre: Uuid,
) -> Result<Option<(Uuid, String, Analyse)>, Response> {
    let l = sqlx::query(
        "SELECT id, chemin, infos FROM fichiers WHERE titre_id = $1 ORDER BY taille DESC LIMIT 1",
    )
    .bind(titre)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?;
    Ok(l.map(|l| {
        let a: sqlx::types::Json<Analyse> = l.get(2);
        (l.get(0), l.get(1), a.0)
    }))
}

pub fn libelle_audio(a: &crate::ffprobe::Audio) -> String {
    let canaux = match a.canaux {
        1 => "mono".to_string(),
        2 => "stéréo".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{n} canaux"),
    };
    let base = a
        .titre
        .clone()
        .or_else(|| a.langue.clone())
        .unwrap_or_else(|| "Piste".into());
    format!("{base} ({}, {canaux})", a.codec.to_uppercase())
}

pub fn libelle_sous_titre(s: &crate::ffprobe::SousTitre) -> String {
    let base = s
        .titre
        .clone()
        .or_else(|| s.langue.clone())
        .unwrap_or_else(|| "Sous-titres".into());
    if s.externe.is_some() {
        format!("{base} (externe)")
    } else {
        base
    }
}

pub fn paliers(a: &Analyse) -> Vec<&'static str> {
    let hauteur = a.video.as_ref().map_or(0, |v| v.hauteur);
    std::iter::once("original")
        .chain(
            PALIERS
                .iter()
                .filter(|(_, h)| *h < hauteur)
                .map(|(n, _)| *n),
        )
        .collect()
}

pub fn pistes(a: &Analyse) -> Value {
    json!({
        "audio": a.audio.iter().enumerate().map(|(n, p)| json!({
            "n": n, "label": libelle_audio(p), "language": p.langue,
        })).collect::<Vec<_>>(),
        "subtitles": a.sous_titres.iter().enumerate().map(|(n, s)| json!({
            "n": n, "label": libelle_sous_titre(s), "language": s.langue,
        })).collect::<Vec<_>>(),
        "qualities": paliers(a),
    })
}

pub async fn detail(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let l = ligne(&state, p.sub(), id).await?;
    let mut v = public(&l);
    if let Some((_, _, a)) = fichier_de(&state, id).await?
        && let (Some(o), Value::Object(extra)) = (v.as_object_mut(), pistes(&a))
    {
        o.extend(extra);
    }
    Ok(ok(&v))
}

pub async fn saisons(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let genre: Option<String> = sqlx::query_scalar("SELECT genre FROM titres WHERE id = $1")
        .bind(id)
        .fetch_optional(state.pool())
        .await
        .map_err(bd)?;
    if genre.as_deref() != Some("serie") {
        return Err(introuvable());
    }
    let lignes = sqlx::query(
        "SELECT n.numero, coalesce(s.titre, 'Saison ' || n.numero) AS titre, coalesce(s.episodes, 0) AS episodes,
                s.affiche IS NOT NULL AS affiche,
                (SELECT count(*) FROM titres e WHERE e.serie_id = $1 AND e.saison = n.numero AND e.present) AS disponibles
         FROM (SELECT numero FROM saisons WHERE serie_id = $1 AND numero > 0
               UNION SELECT DISTINCT saison FROM titres WHERE serie_id = $1) n
         LEFT JOIN saisons s ON s.serie_id = $1 AND s.numero = n.numero
         ORDER BY n.numero",
    )
    .bind(id)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let items: Vec<Value> = lignes
        .iter()
        .map(|l| {
            let disponibles: i64 = l.get("disponibles");
            json!({
                "number": l.get::<i32, _>("numero"),
                "title": l.get::<String, _>("titre"),
                "episode_count": l.get::<i32, _>("episodes"),
                "available_count": disponibles,
                "available": disponibles > 0,
                "poster": l.get::<bool, _>("affiche"),
            })
        })
        .collect();
    Ok(ok(&json!({ "items": items })))
}

pub async fn episodes(
    State(state): State<AppState>,
    p: Personne,
    Path((id, numero)): Path<(String, i32)>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES} {} WHERE t.serie_id = $2 AND t.saison = $3 ORDER BY t.episode",
        jointures("$1")
    )))
    .bind(p.sub())
    .bind(id)
    .bind(numero)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(
        &json!({ "items": lignes.iter().map(public).collect::<Vec<_>>() }),
    ))
}

pub async fn image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let fond = match q.get("type").map(String::as_str) {
        None | Some("poster") => false,
        Some("backdrop") => true,
        Some(_) => return Err(invalide("type d'image inconnu")),
    };
    let largeur = match q.get("w") {
        None => images::LARGEURS[0],
        Some(w) => w
            .parse::<u32>()
            .ok()
            .filter(|w| images::LARGEURS.contains(w))
            .ok_or_else(|| invalide("largeur non proposée"))?,
    };
    let chemin: Option<String> = sqlx::query_scalar(if fond {
        "SELECT coalesce(t.fond, s.fond) FROM titres t LEFT JOIN titres s ON s.id = t.serie_id WHERE t.id = $1"
    } else {
        "SELECT coalesce(t.affiche, s.affiche) FROM titres t LEFT JOIN titres s ON s.id = t.serie_id WHERE t.id = $1"
    })
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .flatten();
    let chemin = chemin.ok_or_else(introuvable)?;
    let (mime, octets) = images::lire(&state, &chemin, largeur)
        .await?
        .ok_or_else(introuvable)?;
    Ok(images::reponse(mime, octets))
}

pub async fn image_saison(
    State(state): State<AppState>,
    Path((id, numero)): Path<(String, i32)>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let chemin: Option<String> = sqlx::query_scalar(
        "SELECT coalesce(s.affiche, t.affiche) FROM titres t LEFT JOIN saisons s ON s.serie_id = t.id AND s.numero = $2 WHERE t.id = $1",
    )
    .bind(id)
    .bind(numero)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .flatten();
    let chemin = chemin.ok_or_else(introuvable)?;
    let (mime, octets) = images::lire(&state, &chemin, images::LARGEURS[0])
        .await?
        .ok_or_else(introuvable)?;
    Ok(images::reponse(mime, octets))
}

pub async fn ecartes(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT nom, raison, vu_le FROM fichiers_ecartes ORDER BY vu_le DESC, nom LIMIT 500",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| json!({
            "name": l.get::<String, _>(0),
            "reason": l.get::<String, _>(1),
            "seen_at": horodatage(&l.get::<DateTime<Utc>, _>(2)),
        })).collect::<Vec<_>>(),
    })))
}

pub async fn etat(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT source, reussie_le, essai_le, erreur FROM synchronisations ORDER BY source",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let materiel = state.materiel().etat();
    Ok(ok(&json!({
        "sync": lignes.iter().map(|l| json!({
            "source": l.get::<String, _>(0),
            "succeeded_at": l.get::<Option<DateTime<Utc>>, _>(1).as_ref().map(horodatage),
            "attempted_at": l.get::<Option<DateTime<Utc>>, _>(2).as_ref().map(horodatage),
            "error": l.get::<Option<String>, _>(3),
        })).collect::<Vec<_>>(),
        "transcoding": {
            "requested": materiel.demande.code(),
            "active": materiel.effectif.code(),
            "message": materiel.message,
            "running": state.transcodeur().nombre(),
        },
        "configured": {
            "radarr": state.config().radarr.is_some(),
            "sonarr": state.config().sonarr.is_some(),
            "tmdb": !state.config().tmdb_cle.is_empty(),
            "push": state.push().web_actif() || state.push().fcm_actif(),
        },
    })))
}

pub async fn lancer_synchro(State(state): State<AppState>) -> Response {
    tokio::spawn(async move { crate::catalogue::synchroniser(&state).await });
    http::json(StatusCode::ACCEPTED, &json!({ "started": true }))
}

pub async fn webhook(State(state): State<AppState>) -> Response {
    tokio::spawn(async move { crate::catalogue::synchroniser(&state).await });
    http::sans_contenu()
}
