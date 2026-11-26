use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::{
    categories, classement,
    etat::AppState,
    garde::Personne,
    http::{bd, corps, cree, facultatif, identifiant, introuvable, invalide, ok},
    normalisation,
};

pub const MOTS_MAX: usize = 12;
pub const MOT_MAX: usize = 60;
pub const REGLES_MAX: i64 = 500;
pub const TYPES: [&str; 7] = [
    "carte",
    "prelevement",
    "virement",
    "virement_instantane",
    "echeance_pret",
    "cotisation",
    "autre",
];

const REGLE: &str = "jsonb_build_object(
    'id', r.id, 'key', r.cle, 'keywords', r.mots, 'type', r.type, 'section', r.section,
    'category_id', r.categorie_id,
    'position', (SELECT count(*) FROM regles a WHERE a.sub = r.sub AND (a.rang, a.id) < (r.rang, r.id)),
    'operations', (SELECT count(*) FROM operations o WHERE o.regle_id = r.id))";

async fn lire(c: &mut PgConnection, sub: &str, id: i64) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {REGLE} FROM regles r WHERE r.sub = $1 AND r.id = $2"
    )))
    .bind(sub)
    .bind(id)
    .fetch_optional(&mut *c)
    .await
}

pub async fn lister(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let regles: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {REGLE} FROM regles r WHERE r.sub = $1 ORDER BY r.rang, r.id"
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({ "rules": regles })))
}

pub fn mots_valides(mots: &[String]) -> Result<Vec<String>, Response> {
    let mut propres: Vec<String> = Vec::new();
    for mot in mots {
        let mot = normalisation::texte(mot);
        let longueur = mot.chars().count();
        if !(2..=MOT_MAX).contains(&longueur) {
            return Err(invalide("keywords : mots-clés de 2 à 60 caractères"));
        }
        if !propres.contains(&mot) {
            propres.push(mot);
        }
    }
    if propres.len() > MOTS_MAX {
        return Err(invalide("keywords : 12 mots-clés au plus"));
    }
    Ok(propres)
}

fn type_valide(genre: Option<String>) -> Result<Option<String>, Response> {
    match genre {
        Some(g) if !TYPES.contains(&g.as_str()) => Err(invalide("type : type d'opération inconnu")),
        g => Ok(g),
    }
}

fn section_valide(section: Option<String>) -> Result<Option<String>, Response> {
    let section = section.map(|s| normalisation::texte(&s));
    match section {
        Some(s) if s.is_empty() || s.chars().count() > MOT_MAX => Err(invalide(
            "section : titre de section de 60 caractères au plus",
        )),
        s => Ok(s),
    }
}

fn sans_condition() -> Response {
    invalide("une règle demande au moins un mot-clé, un type ou une section")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nouvelle {
    #[serde(default)]
    keywords: Vec<String>,
    category_id: i64,
    #[serde(default, rename = "type")]
    genre: Option<String>,
    #[serde(default)]
    section: Option<String>,
}

pub async fn creer(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let n: Nouvelle = corps(
        &octets,
        "corps attendu : {keywords, category_id, type?, section?}",
    )?;
    let mots = mots_valides(&n.keywords)?;
    let genre = type_valide(n.genre)?;
    let section = section_valide(n.section)?;
    if mots.is_empty() && genre.is_none() && section.is_none() {
        return Err(sans_condition());
    }
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    categories::exiger(&mut tx, p.sub(), n.category_id).await?;
    let nombre: i64 = sqlx::query_scalar("SELECT count(*) FROM regles WHERE sub = $1")
        .bind(p.sub())
        .fetch_one(&mut *tx)
        .await
        .map_err(bd)?;
    if nombre >= REGLES_MAX {
        return Err(invalide("trop de règles"));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO regles (sub, categorie_id, mots, type, section, rang)
         SELECT $1, $2, $3, $4, $5, COALESCE(min(rang), 0) - 1 FROM regles WHERE sub = $1
         RETURNING id",
    )
    .bind(p.sub())
    .bind(n.category_id)
    .bind(&mots)
    .bind(&genre)
    .bind(&section)
    .fetch_one(&mut *tx)
    .await
    .map_err(bd)?;
    let reclassees = classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    let regle = lire(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(cree(&json!({ "rule": regle, "reclassified": reclassees })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Modification {
    keywords: Option<Vec<String>>,
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "facultatif", rename = "type")]
    genre: Option<Option<String>>,
    #[serde(default, deserialize_with = "facultatif")]
    section: Option<Option<String>>,
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let m: Modification = corps(
        &octets,
        "corps attendu : {keywords?, category_id?, type?, section?}",
    )?;
    let mots = m.keywords.as_deref().map(mots_valides).transpose()?;
    let genre = m.genre.map(type_valide).transpose()?;
    let section = m.section.map(section_valide).transpose()?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    if let Some(c) = m.category_id {
        categories::exiger(&mut tx, p.sub(), c).await?;
    }
    let reste: Option<bool> = sqlx::query_scalar(
        "UPDATE regles SET
            mots = COALESCE($3, mots), categorie_id = COALESCE($4, categorie_id),
            type = CASE WHEN $5 THEN $6 ELSE type END,
            section = CASE WHEN $7 THEN $8 ELSE section END
         WHERE sub = $1 AND id = $2
         RETURNING cardinality(mots) > 0 OR type IS NOT NULL OR section IS NOT NULL",
    )
    .bind(p.sub())
    .bind(id)
    .bind(mots)
    .bind(m.category_id)
    .bind(genre.is_some())
    .bind(genre.flatten())
    .bind(section.is_some())
    .bind(section.flatten())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| match e.as_database_error() {
        Some(d) if d.is_check_violation() => sans_condition(),
        _ => bd(e),
    })?;
    if reste.is_none() {
        return Err(introuvable());
    }
    let reclassees = classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    let regle = lire(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "rule": regle, "reclassified": reclassees })))
}

pub async fn supprimer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let supprimees = sqlx::query("DELETE FROM regles WHERE sub = $1 AND id = $2")
        .bind(p.sub())
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(bd)?
        .rows_affected();
    if supprimees == 0 {
        return Err(introuvable());
    }
    let reclassees = classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "reclassified": reclassees })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ordre {
    ids: Vec<i64>,
}

pub async fn ordonner(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let o: Ordre = corps(&octets, "corps attendu : {ids}")?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let mut connues: Vec<i64> = sqlx::query_scalar("SELECT id FROM regles WHERE sub = $1")
        .bind(p.sub())
        .fetch_all(&mut *tx)
        .await
        .map_err(bd)?;
    let mut donnees = o.ids.clone();
    connues.sort_unstable();
    donnees.sort_unstable();
    if connues != donnees {
        return Err(invalide(
            "ids : toutes les règles, une fois chacune, dans l'ordre voulu",
        ));
    }
    sqlx::query(
        "UPDATE regles r SET rang = u.rang::integer
         FROM UNNEST($2::bigint[]) WITH ORDINALITY AS u(id, rang)
         WHERE r.sub = $1 AND r.id = u.id",
    )
    .bind(p.sub())
    .bind(&o.ids)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    let reclassees = classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    let regles: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {REGLE} FROM regles r WHERE r.sub = $1 ORDER BY r.rang, r.id"
    )))
    .bind(p.sub())
    .fetch_all(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "rules": regles, "reclassified": reclassees })))
}
