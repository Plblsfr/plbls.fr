use std::collections::{BTreeSet, HashMap};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    arr,
    bibliotheque::{COLONNES, jointures, public, uuid},
    etat::AppState,
    garde::Personne,
    http::{self, bd, erreur, horodatage, introuvable, invalide, objet, ok},
    images,
    tmdb::{Fiche, Tmdb, chemin_image_valide},
};

const COLONNES_DEMANDE: &str =
    "d.id, d.genre, d.tmdb_id, d.saisons, d.titre, d.annee, d.affiche, d.etat, d.motif,
    d.envoyee, d.erreur_envoi, d.cree_le, d.maj";

fn url_affiche(chemin: Option<&str>) -> Option<String> {
    chemin
        .filter(|c| chemin_image_valide(c))
        .map(|c| format!("/images/tmdb{c}"))
}

fn genre_demande(kind: &str) -> Option<&'static str> {
    match kind {
        "movie" => Some("film"),
        "series" => Some("serie"),
        _ => None,
    }
}

fn kind(genre: &str) -> &'static str {
    if genre == "film" { "movie" } else { "series" }
}

fn demande_publique(l: &PgRow, admin: bool) -> Value {
    let mut v = json!({
        "id": l.get::<Uuid, _>("id"),
        "kind": kind(l.get::<String, _>("genre").as_str()),
        "tmdb_id": l.get::<i64, _>("tmdb_id"),
        "seasons": l.get::<Option<Vec<i32>>, _>("saisons"),
        "title": l.get::<String, _>("titre"),
        "year": l.get::<Option<i32>, _>("annee"),
        "poster_url": url_affiche(l.get::<Option<String>, _>("affiche").as_deref()),
        "state": l.get::<String, _>("etat"),
        "reason": l.get::<Option<String>, _>("motif"),
        "created_at": horodatage(&l.get::<DateTime<Utc>, _>("cree_le")),
        "updated_at": horodatage(&l.get::<DateTime<Utc>, _>("maj")),
    });
    if admin {
        v["sent"] = json!(l.get::<bool, _>("envoyee"));
        v["send_error"] = json!(l.get::<Option<String>, _>("erreur_envoi"));
    }
    v
}

pub async fn image_tmdb(
    State(state): State<AppState>,
    Path(nom): Path<String>,
) -> Result<Response, Response> {
    let chemin = format!("/{nom}");
    if !chemin_image_valide(&chemin) {
        return Err(introuvable());
    }
    let (mime, octets) = images::lire(&state, &chemin, images::LARGEURS[0])
        .await?
        .ok_or_else(introuvable)?;
    Ok(images::reponse(mime, octets))
}

async fn demandes_par_titre(
    state: &AppState,
    cles: &[(String, i64)],
) -> Result<HashMap<(String, i64), Value>, Response> {
    if cles.is_empty() {
        return Ok(HashMap::new());
    }
    let genres: Vec<String> = cles.iter().map(|(g, _)| g.clone()).collect();
    let ids: Vec<i64> = cles.iter().map(|(_, i)| *i).collect();
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT DISTINCT ON (d.genre, d.tmdb_id) {COLONNES_DEMANDE} FROM demandes d
         JOIN unnest($1::text[], $2::bigint[]) AS c(genre, tmdb_id) ON c.genre = d.genre AND c.tmdb_id = d.tmdb_id
         ORDER BY d.genre, d.tmdb_id, (d.etat IN ('en_attente', 'acceptee')) DESC, d.maj DESC"
    )))
    .bind(&genres)
    .bind(&ids)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(lignes
        .iter()
        .map(|l| {
            (
                (l.get::<String, _>("genre"), l.get::<i64, _>("tmdb_id")),
                demande_publique(l, false),
            )
        })
        .collect())
}

pub async fn rechercher(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let texte = q.get("q").map(|t| t.trim()).unwrap_or_default();
    if texte.is_empty() || texte.chars().count() > 100 {
        return Err(invalide("recherche vide ou trop longue"));
    }
    let motif = commun::sql::contient(texte);
    let locaux = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES}, t.tmdb_id {} WHERE t.genre <> 'episode' AND t.present AND (t.titre ILIKE $2 OR t.titre_original ILIKE $2)
         ORDER BY lower(t.titre) LIMIT 20",
        jointures("$1")
    )))
    .bind(p.sub())
    .bind(&motif)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut items: Vec<Value> = Vec::new();
    let mut vus: BTreeSet<(String, i64)> = BTreeSet::new();
    for l in &locaux {
        let genre: String = l.get("genre");
        vus.insert((genre.clone(), l.get("tmdb_id")));
        let mut v = public(l);
        v["present"] = json!(true);
        v["tmdb_id"] = json!(l.get::<i64, _>("tmdb_id"));
        v["requestable"] = json!(genre == "serie");
        items.push(v);
    }
    let tmdb = Tmdb::new(state.config(), state.http());
    let mut erreur_tmdb = None;
    let resultats = if tmdb.configure() {
        match tmdb.rechercher(texte).await {
            Ok(r) => r,
            Err(e) => {
                erreur_tmdb = Some(e.to_string());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let cles: Vec<(String, i64)> = resultats
        .iter()
        .map(|r| (r.genre.to_string(), r.tmdb_id))
        .collect();
    let presents: HashMap<(String, i64), PgRow> = if cles.is_empty() {
        HashMap::new()
    } else {
        let genres: Vec<String> = cles.iter().map(|(g, _)| g.clone()).collect();
        let ids: Vec<i64> = cles.iter().map(|(_, i)| *i).collect();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLONNES}, t.tmdb_id {} JOIN unnest($2::text[], $3::bigint[]) AS c(genre, tmdb_id)
             ON c.genre = t.genre AND c.tmdb_id = t.tmdb_id",
            jointures("$1")
        )))
        .bind(p.sub())
        .bind(&genres)
        .bind(&ids)
        .fetch_all(state.pool())
        .await
        .map_err(bd)?
        .into_iter()
        .map(|l| ((l.get::<String, _>("genre"), l.get::<i64, _>("tmdb_id")), l))
        .collect()
    };
    let demandes = demandes_par_titre(&state, &cles).await?;
    for r in resultats {
        let cle = (r.genre.to_string(), r.tmdb_id);
        if vus.contains(&cle) {
            continue;
        }
        vus.insert(cle.clone());
        let local = presents.get(&cle);
        let present = local.is_some_and(|l| l.get::<bool, _>("present"));
        let mut v = match local {
            Some(l) => public(l),
            None => json!({
                "id": null,
                "kind": kind(r.genre),
                "title": r.titre,
                "year": r.annee,
                "overview": r.resume,
                "poster": false,
                "available": false,
            }),
        };
        v["present"] = json!(present);
        v["tmdb_id"] = json!(r.tmdb_id);
        v["poster_url"] = json!(url_affiche(r.affiche.as_deref()));
        v["requestable"] = json!(!present || r.genre == "serie");
        v["request"] = demandes.get(&cle).cloned().unwrap_or(Value::Null);
        items.push(v);
    }
    Ok(ok(
        &json!({ "items": items, "tmdb_error": erreur_tmdb.is_some() }),
    ))
}

struct EtatSaison {
    numero: i32,
    titre: String,
    episodes: i32,
    disponibles: i64,
}

impl EtatSaison {
    fn complete(&self) -> bool {
        self.episodes > 0 && self.disponibles >= i64::from(self.episodes)
    }
}

async fn saisons_tmdb(
    state: &AppState,
    tmdb_id: i64,
) -> Result<(Fiche, Vec<EtatSaison>), Response> {
    let fiche = Tmdb::new(state.config(), state.http())
        .serie(tmdb_id)
        .await
        .map_err(|e| match e {
            crate::tmdb::Erreur::Introuvable => introuvable(),
            e => {
                tracing::warn!("{e}");
                http::indisponible("tmdb_unavailable", "TMDB injoignable")
            }
        })?;
    let disponibles: HashMap<i32, i64> = sqlx::query(
        "SELECT e.saison, count(*) FROM titres s JOIN titres e ON e.serie_id = s.id AND e.present
         WHERE s.genre = 'serie' AND s.tmdb_id = $1 GROUP BY e.saison",
    )
    .bind(tmdb_id)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?
    .iter()
    .map(|l| (l.get::<i32, _>(0), l.get::<i64, _>(1)))
    .collect();
    let saisons = fiche
        .saisons
        .iter()
        .filter(|s| s.numero > 0)
        .map(|s| EtatSaison {
            numero: s.numero,
            titre: s.titre.clone(),
            episodes: s.episodes,
            disponibles: disponibles.get(&s.numero).copied().unwrap_or(0),
        })
        .collect();
    Ok((fiche, saisons))
}

pub async fn saisons_demandables(
    State(state): State<AppState>,
    Path(tmdb_id): Path<i64>,
) -> Result<Response, Response> {
    let (fiche, saisons) = saisons_tmdb(&state, tmdb_id).await?;
    let en_cours: Vec<(Option<Vec<i32>>, String)> = sqlx::query(
        "SELECT saisons, etat FROM demandes WHERE genre = 'serie' AND tmdb_id = $1 AND etat IN ('en_attente', 'acceptee')",
    )
    .bind(tmdb_id)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?
    .iter()
    .map(|l| (l.get(0), l.get(1)))
    .collect();
    let demandee = |n: i32| {
        en_cours
            .iter()
            .find(|(s, _)| s.as_ref().is_none_or(|s| s.contains(&n)))
            .map(|(_, e)| e.clone())
    };
    Ok(ok(&json!({
        "tmdb_id": tmdb_id,
        "title": fiche.titre,
        "year": fiche.annee,
        "poster_url": url_affiche(fiche.affiche.as_deref()),
        "seasons": saisons.iter().map(|s| json!({
            "number": s.numero,
            "title": s.titre,
            "episode_count": s.episodes,
            "available_count": s.disponibles,
            "complete": s.complete(),
            "request_state": demandee(s.numero),
        })).collect::<Vec<_>>(),
    })))
}

async fn lire_demande(state: &AppState, id: Uuid) -> Result<PgRow, Response> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_DEMANDE} FROM demandes d WHERE d.id = $1"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)
}

fn deja_disponible() -> Response {
    erreur(
        StatusCode::CONFLICT,
        "already_available",
        "ce titre est déjà disponible",
    )
}

pub async fn creer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let genre = corps
        .get("kind")
        .and_then(Value::as_str)
        .and_then(genre_demande)
        .ok_or_else(|| invalide("type attendu : movie ou series"))?;
    let tmdb_id = corps
        .get("tmdb_id")
        .and_then(Value::as_i64)
        .filter(|i| *i > 0)
        .ok_or_else(|| invalide("tmdb_id invalide"))?;
    let saisons: Option<Vec<i32>> = match corps.get("seasons") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) if genre == "serie" && !a.is_empty() && a.len() <= 100 => {
            let mut s: Vec<i32> = a
                .iter()
                .map(|x| {
                    x.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .filter(|n| *n > 0)
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalide("saisons invalides"))?;
            s.sort_unstable();
            s.dedup();
            Some(s)
        }
        Some(_) => return Err(invalide("saisons invalides")),
    };
    let (titre, annee, affiche) = if genre == "film" {
        let present: Option<bool> =
            sqlx::query_scalar("SELECT present FROM titres WHERE genre = 'film' AND tmdb_id = $1")
                .bind(tmdb_id)
                .fetch_optional(state.pool())
                .await
                .map_err(bd)?;
        if present == Some(true) {
            return Err(deja_disponible());
        }
        let f = Tmdb::new(state.config(), state.http())
            .film(tmdb_id)
            .await
            .map_err(|e| match e {
                crate::tmdb::Erreur::Introuvable => introuvable(),
                _ => http::indisponible("tmdb_unavailable", "TMDB injoignable"),
            })?;
        (f.titre, f.annee, f.affiche)
    } else {
        let (f, etats) = saisons_tmdb(&state, tmdb_id).await?;
        match &saisons {
            None => {
                if !etats.is_empty()
                    && etats
                        .iter()
                        .filter(|s| s.episodes > 0)
                        .all(EtatSaison::complete)
                {
                    return Err(deja_disponible());
                }
            }
            Some(liste) => {
                for n in liste {
                    match etats.iter().find(|s| s.numero == *n) {
                        None => return Err(invalide(&format!("la saison {n} n'existe pas"))),
                        Some(s) if s.complete() => return Err(deja_disponible()),
                        Some(_) => {}
                    }
                }
            }
        }
        (f.titre, f.annee, f.affiche)
    };
    let cree: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO demandes (genre, tmdb_id, saisons, titre, annee, affiche) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (genre, tmdb_id, coalesce(saisons, '{}')) WHERE etat IN ('en_attente', 'acceptee') DO NOTHING
         RETURNING id",
    )
    .bind(genre)
    .bind(tmdb_id)
    .bind(&saisons)
    .bind(&titre)
    .bind(annee)
    .bind(&affiche)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?;
    let (id, nouvelle) = match cree {
        Some(id) => (id, true),
        None => (
            sqlx::query_scalar(
                "SELECT id FROM demandes WHERE genre = $1 AND tmdb_id = $2 AND coalesce(saisons, '{}') = coalesce($3, '{}')
                 AND etat IN ('en_attente', 'acceptee')",
            )
            .bind(genre)
            .bind(tmdb_id)
            .bind(&saisons)
            .fetch_one(state.pool())
            .await
            .map_err(bd)?,
            false,
        ),
    };
    sqlx::query(
        "INSERT INTO demandeurs (demande_id, sub, nom) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(p.sub())
    .bind(&p.identite.nom)
    .execute(state.pool())
    .await
    .map_err(bd)?;
    let l = lire_demande(&state, id).await?;
    let mut v = demande_publique(&l, false);
    v["joined_existing"] = json!(!nouvelle);
    Ok(http::json(
        if nouvelle {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        &v,
    ))
}

pub async fn miennes(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_DEMANDE}, (SELECT t.id FROM titres t WHERE t.genre = d.genre AND t.tmdb_id = d.tmdb_id) AS titre_id
         FROM demandes d JOIN demandeurs r ON r.demande_id = d.id AND r.sub = $1 ORDER BY r.rattache_le DESC LIMIT 200"
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| {
            let mut v = demande_publique(l, false);
            v["title_id"] = json!(l.get::<Option<Uuid>, _>("titre_id"));
            v
        }).collect::<Vec<_>>(),
    })))
}

pub async fn liste_admin(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let etat = q.get("state").filter(|e| !e.is_empty());
    if etat.is_some_and(|e| {
        !matches!(
            e.as_str(),
            "en_attente" | "acceptee" | "refusee" | "disponible"
        )
    }) {
        return Err(invalide("état inconnu"));
    }
    let depuis = match q.get("since").filter(|d| !d.is_empty()) {
        None => None,
        Some(d) => Some(
            DateTime::parse_from_rfc3339(d)
                .map(|d| d.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .map(|n| n.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc())
                })
                .map_err(|_| invalide("date invalide"))?,
        ),
    };
    let demandeur = q
        .get("requester")
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .map(|r| format!("%{r}%"));
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_DEMANDE},
           (SELECT json_agg(json_build_object('name', r.nom, 'joined_at', r.rattache_le) ORDER BY r.rattache_le)
            FROM demandeurs r WHERE r.demande_id = d.id) AS demandeurs
         FROM demandes d
         WHERE ($1::text IS NULL OR d.etat = $1) AND ($2::timestamptz IS NULL OR d.cree_le >= $2)
           AND ($3::text IS NULL OR EXISTS (SELECT 1 FROM demandeurs r WHERE r.demande_id = d.id AND r.nom ILIKE $3))
         ORDER BY (d.etat = 'en_attente') DESC, d.cree_le DESC LIMIT 500"
    )))
    .bind(etat)
    .bind(depuis)
    .bind(&demandeur)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| {
            let mut v = demande_publique(l, true);
            v["requesters"] = l.get::<Option<Value>, _>("demandeurs").unwrap_or(json!([]));
            v
        }).collect::<Vec<_>>(),
    })))
}

pub async fn envoyer(state: &AppState, id: Uuid) -> Result<(), Response> {
    let l = lire_demande(state, id).await?;
    let genre: String = l.get("genre");
    let tmdb_id: i64 = l.get("tmdb_id");
    let saisons: Option<Vec<i32>> = l.get("saisons");
    let resultat = if genre == "film" {
        match &state.config().radarr {
            None => Err("Radarr non configuré".to_string()),
            Some(c) => {
                arr::Client::new(c, state.http())
                    .ajouter_film(tmdb_id)
                    .await
            }
        }
    } else {
        match &state.config().sonarr {
            None => Err("Sonarr non configuré".to_string()),
            Some(c) => {
                let tvdb = Tmdb::new(state.config(), state.http())
                    .serie(tmdb_id)
                    .await
                    .ok()
                    .and_then(|f| f.tvdb_id);
                arr::Client::new(c, state.http())
                    .ajouter_serie(tmdb_id, tvdb, saisons.as_deref())
                    .await
            }
        }
    };
    let erreur = resultat.err();
    if let Some(e) = &erreur {
        tracing::warn!("envoi de la demande {id} : {e}");
    }
    sqlx::query("UPDATE demandes SET envoyee = $2, erreur_envoi = $3, maj = now() WHERE id = $1")
        .bind(id)
        .bind(erreur.is_none())
        .bind(&erreur)
        .execute(state.pool())
        .await
        .map_err(bd)?;
    Ok(())
}

pub async fn accepter(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let l = lire_demande(&state, id).await?;
    let etat: String = l.get("etat");
    match etat.as_str() {
        "en_attente" => {
            let modifiees = sqlx::query("UPDATE demandes SET etat = 'acceptee', traitee_par = $2, maj = now() WHERE id = $1 AND etat = 'en_attente'")
                .bind(id)
                .bind(p.sub())
                .execute(state.pool())
                .await
                .map_err(bd)?
                .rows_affected();
            if modifiees == 0 {
                return Err(erreur(
                    StatusCode::CONFLICT,
                    "invalid_state",
                    "demande déjà traitée",
                ));
            }
        }
        "acceptee" if !l.get::<bool, _>("envoyee") => {}
        _ => {
            return Err(erreur(
                StatusCode::CONFLICT,
                "invalid_state",
                "demande déjà traitée",
            ));
        }
    }
    envoyer(&state, id).await?;
    marquer_disponibles(&state).await;
    Ok(ok(&demande_publique(
        &lire_demande(&state, id).await?,
        true,
    )))
}

pub async fn refuser(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let motif = corps
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .ok_or_else(|| invalide("motif obligatoire"))?;
    if motif.chars().count() > 500 {
        return Err(invalide("motif trop long"));
    }
    let modifiee = sqlx::query(
        "UPDATE demandes SET etat = 'refusee', motif = $2, traitee_par = $3, maj = now()
         WHERE id = $1 AND etat IN ('en_attente', 'acceptee')",
    )
    .bind(id)
    .bind(motif)
    .bind(p.sub())
    .execute(state.pool())
    .await
    .map_err(bd)?
    .rows_affected();
    if modifiee == 0 {
        lire_demande(&state, id).await?;
        return Err(erreur(
            StatusCode::CONFLICT,
            "invalid_state",
            "demande déjà traitée",
        ));
    }
    let l = lire_demande(&state, id).await?;
    let titre: String = l.get("titre");
    crate::push::prevenir(
        &state,
        id,
        "refusee",
        &format!("Demande refusée : {titre}"),
        motif,
    )
    .await;
    Ok(ok(&demande_publique(&l, true)))
}

pub async fn marquer_disponibles(state: &AppState) {
    let r = sqlx::query(
        "WITH saisons_dispo AS (
            SELECT s.tmdb_id, sa.numero, sa.episodes,
                   (SELECT count(*) FROM titres e WHERE e.serie_id = s.id AND e.saison = sa.numero AND e.present) AS presents
            FROM titres s JOIN saisons sa ON sa.serie_id = s.id WHERE s.genre = 'serie' AND sa.numero > 0
         )
         UPDATE demandes d SET etat = 'disponible', maj = now()
         WHERE d.etat IN ('en_attente', 'acceptee') AND (
           (d.genre = 'film' AND EXISTS (SELECT 1 FROM titres t WHERE t.genre = 'film' AND t.tmdb_id = d.tmdb_id AND t.present))
           OR (d.genre = 'serie' AND d.saisons IS NOT NULL AND NOT EXISTS (
                 SELECT 1 FROM unnest(d.saisons) n
                 WHERE NOT EXISTS (SELECT 1 FROM saisons_dispo x WHERE x.tmdb_id = d.tmdb_id AND x.numero = n
                                   AND x.episodes > 0 AND x.presents >= x.episodes)))
           OR (d.genre = 'serie' AND d.saisons IS NULL
               AND EXISTS (SELECT 1 FROM saisons_dispo x WHERE x.tmdb_id = d.tmdb_id AND x.episodes > 0)
               AND NOT EXISTS (SELECT 1 FROM saisons_dispo x WHERE x.tmdb_id = d.tmdb_id AND x.episodes > 0 AND x.presents < x.episodes))
         )
         RETURNING d.id, d.titre",
    )
    .fetch_all(state.pool())
    .await;
    match r {
        Ok(lignes) => {
            for l in lignes {
                let titre: String = l.get(1);
                crate::push::prevenir(
                    state,
                    l.get(0),
                    "disponible",
                    &format!("{titre} est disponible"),
                    "Votre demande est disponible dans Entracte",
                )
                .await;
            }
        }
        Err(e) => tracing::warn!("demandes disponibles : {e}"),
    }
}
