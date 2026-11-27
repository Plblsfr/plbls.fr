use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::{
    categories, classement,
    etat::AppState,
    garde::Personne,
    http::{bd, corps, identifiant, introuvable, invalide, ok},
    releves::OPERATION,
};

pub const NOM_MAX: usize = 80;
pub const LISTE_MAX: i64 = 500;
pub const FUSION_MAX: usize = 50;

const COMMERCANT: &str = "jsonb_build_object(
    'id', c.id, 'name', c.nom, 'category_id', c.categorie_id,
    'operations', s.nombre, 'debits', s.debits, 'credits', s.credits, 'last_date', s.dernier,
    'variants', (SELECT count(*) FROM variantes v WHERE v.commercant_id = c.id))";

const SOMMES: &str = "LEFT JOIN LATERAL (
        SELECT count(*) AS nombre,
               COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'debit'), 0)::bigint AS debits,
               COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'credit'), 0)::bigint AS credits,
               max(o.date_operation) AS dernier
        FROM operations o WHERE o.commercant_id = c.id) s ON true";

async fn detail(c: &mut PgConnection, sub: &str, id: i64) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {COMMERCANT} || jsonb_build_object('variant_list', COALESCE((
            SELECT jsonb_agg(jsonb_build_object(
                'key', v.cle, 'manual', v.manuelle,
                'operations', (SELECT count(*) FROM operations o WHERE o.sub = v.sub AND o.commercant_cle = v.cle))
                ORDER BY v.cle)
            FROM variantes v WHERE v.commercant_id = c.id), '[]'::jsonb))
         FROM commercants c {SOMMES} WHERE c.sub = $1 AND c.id = $2"
    )))
    .bind(sub)
    .bind(id)
    .fetch_optional(&mut *c)
    .await
}

#[derive(Debug, Default, Deserialize)]
pub struct Recherche {
    pub q: Option<String>,
}

pub async fn lister(
    State(state): State<AppState>,
    p: Personne,
    Query(r): Query<Recherche>,
) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let motif =
        r.q.as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(commun::sql::contient);
    let commercants: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {COMMERCANT} FROM commercants c {SOMMES}
         WHERE c.sub = $1
           AND ($2::text IS NULL OR c.nom ILIKE $2
                OR EXISTS (SELECT 1 FROM variantes v WHERE v.commercant_id = c.id AND v.cle ILIKE $2))
           AND (s.nombre > 0 OR c.categorie_id IS NOT NULL
                OR (SELECT count(*) FROM variantes v WHERE v.commercant_id = c.id) > 1)
         ORDER BY s.nombre DESC, c.nom, c.id LIMIT $3"
    )))
    .bind(p.sub())
    .bind(motif)
    .bind(LISTE_MAX)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({ "merchants": commercants })))
}

pub async fn lire(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let mut c = state.pool().acquire().await.map_err(bd)?;
    let commercant = detail(&mut c, p.sub(), id)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    Ok(ok(&json!({ "merchant": commercant })))
}

async fn existe(c: &mut PgConnection, sub: &str, id: i64) -> Result<(), Response> {
    let trouve: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM commercants WHERE sub = $1 AND id = $2)")
            .bind(sub)
            .bind(id)
            .fetch_one(&mut *c)
            .await
            .map_err(bd)?;
    if trouve { Ok(()) } else { Err(introuvable()) }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Renommage {
    name: String,
}

pub async fn renommer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let r: Renommage = corps(&octets, "corps attendu : {name}")?;
    let nom = r.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if nom.is_empty() || nom.chars().count() > NOM_MAX || nom.chars().any(char::is_control) {
        return Err(invalide("name : 1 à 80 caractères"));
    }
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    existe(&mut tx, p.sub(), id).await?;
    sqlx::query("UPDATE commercants SET nom = $3 WHERE sub = $1 AND id = $2")
        .bind(p.sub())
        .bind(id)
        .bind(&nom)
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
    let commercant = detail(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "merchant": commercant })))
}

pub async fn classer_en(
    c: &mut PgConnection,
    sub: &str,
    id: i64,
    categorie: Option<i64>,
) -> Result<i64, Response> {
    existe(&mut *c, sub, id).await?;
    if let Some(categorie) = categorie {
        categories::exiger(&mut *c, sub, categorie).await?;
    }
    sqlx::query("UPDATE commercants SET categorie_id = $3 WHERE sub = $1 AND id = $2")
        .bind(sub)
        .bind(id)
        .bind(categorie)
        .execute(&mut *c)
        .await
        .map_err(bd)?;
    classement::rejouer(&mut *c, sub).await.map_err(bd)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Classement {
    category_id: Option<i64>,
}

pub async fn classer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let k: Classement = corps(&octets, "corps attendu : {category_id}")?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let reclassees = classer_en(&mut tx, p.sub(), id, k.category_id).await?;
    let commercant = detail(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(
        &json!({ "merchant": commercant, "reclassified": reclassees }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fusion {
    merchant_ids: Vec<i64>,
}

pub async fn fusionner(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let f: Fusion = corps(&octets, "corps attendu : {merchant_ids}")?;
    let mut autres = f.merchant_ids.clone();
    autres.sort_unstable();
    autres.dedup();
    if autres.is_empty() || autres.len() > FUSION_MAX || autres.contains(&id) {
        return Err(invalide(
            "merchant_ids : 1 à 50 autres commerçants à regrouper",
        ));
    }
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    existe(&mut tx, p.sub(), id).await?;
    let connus: i64 =
        sqlx::query_scalar("SELECT count(*) FROM commercants WHERE sub = $1 AND id = ANY($2)")
            .bind(p.sub())
            .bind(&autres)
            .fetch_one(&mut *tx)
            .await
            .map_err(bd)?;
    if usize::try_from(connus).ok() != Some(autres.len()) {
        return Err(invalide("merchant_ids : commerçant inconnu"));
    }
    sqlx::query(
        "UPDATE commercants c SET categorie_id = (
            SELECT a.categorie_id FROM commercants a
            WHERE a.sub = $1 AND a.id = ANY($3) AND a.categorie_id IS NOT NULL
            ORDER BY array_position($4::bigint[], a.id) LIMIT 1)
         WHERE c.sub = $1 AND c.id = $2 AND c.categorie_id IS NULL",
    )
    .bind(p.sub())
    .bind(id)
    .bind(&autres)
    .bind(&f.merchant_ids)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    sqlx::query(
        "UPDATE variantes SET commercant_id = $2, manuelle = true
         WHERE sub = $1 AND commercant_id = ANY($3)",
    )
    .bind(p.sub())
    .bind(id)
    .bind(&autres)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    sqlx::query("DELETE FROM commercants WHERE sub = $1 AND id = ANY($2)")
        .bind(p.sub())
        .bind(&autres)
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
    let reclassees = classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    let commercant = detail(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(
        &json!({ "merchant": commercant, "reclassified": reclassees }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Separation {
    keys: Vec<String>,
}

pub async fn separer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let s: Separation = corps(&octets, "corps attendu : {keys}")?;
    let mut cles = s.keys.clone();
    cles.sort_unstable();
    cles.dedup();
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    existe(&mut tx, p.sub(), id).await?;
    let variantes: Vec<String> =
        sqlx::query_scalar("SELECT cle FROM variantes WHERE sub = $1 AND commercant_id = $2")
            .bind(p.sub())
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(bd)?;
    if cles.is_empty() || cles.iter().any(|c| !variantes.contains(c)) {
        return Err(invalide("keys : variantes de ce commerçant"));
    }
    if cles.len() >= variantes.len() {
        return Err(invalide(
            "keys : le commerçant doit garder au moins une variante",
        ));
    }
    let mut crees = Vec::new();
    for cle in &cles {
        let nouveau: i64 = sqlx::query_scalar(
            "INSERT INTO commercants (sub, nom, categorie_id)
             SELECT $1, $3, categorie_id FROM commercants WHERE sub = $1 AND id = $2
             RETURNING id",
        )
        .bind(p.sub())
        .bind(id)
        .bind(crate::normalisation::nom(cle))
        .fetch_one(&mut *tx)
        .await
        .map_err(bd)?;
        sqlx::query(
            "UPDATE variantes SET commercant_id = $3, manuelle = true WHERE sub = $1 AND cle = $2",
        )
        .bind(p.sub())
        .bind(cle)
        .bind(nouveau)
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
        crees.push(nouveau);
    }
    classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
    let commercant = detail(&mut tx, p.sub(), id).await.map_err(bd)?;
    let mut nouveaux = Vec::new();
    for nouveau in crees {
        nouveaux.push(detail(&mut tx, p.sub(), nouveau).await.map_err(bd)?);
    }
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "merchant": commercant, "created": nouveaux })))
}

const A_CLASSER: &str = "o.sub = $1 AND o.categorie_id IS NULL";
pub const GROUPES_MAX: i64 = 200;

async fn compteur(c: &mut PgConnection, sub: &str) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_build_object(
            'operations', count(*),
            'merchants', count(DISTINCT COALESCE(o.commercant_id, -o.id)),
            'debits', COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'debit'), 0)::bigint,
            'credits', COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'credit'), 0)::bigint)
         FROM operations o WHERE {A_CLASSER}"
    )))
    .bind(sub)
    .fetch_one(&mut *c)
    .await
}

pub async fn compter(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let mut c = state.pool().acquire().await.map_err(bd)?;
    Ok(ok(&compteur(&mut c, p.sub()).await.map_err(bd)?))
}

pub async fn a_classer(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let mut c = state.pool().acquire().await.map_err(bd)?;
    let mut reponse = compteur(&mut c, p.sub()).await.map_err(bd)?;
    let groupes: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_build_object(
            'merchant_id', o.commercant_id, 'merchant_name', m.nom,
            'operations', count(*),
            'debits', COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'debit'), 0)::bigint,
            'credits', COALESCE(sum(o.montant) FILTER (WHERE o.sens = 'credit'), 0)::bigint,
            'first_date', min(o.date_operation), 'last_date', max(o.date_operation),
            'sample', to_jsonb((array_agg({OPERATION} ORDER BY o.date_operation DESC, o.id DESC))[1:3]))
         FROM operations o LEFT JOIN commercants m ON m.id = o.commercant_id
         WHERE {A_CLASSER}
         GROUP BY o.commercant_id, m.nom, CASE WHEN o.commercant_id IS NULL THEN o.id END
         ORDER BY count(*) DESC, max(o.date_operation) DESC, min(o.id)
         LIMIT $2"
    )))
    .bind(p.sub())
    .bind(GROUPES_MAX)
    .fetch_all(&mut *c)
    .await
    .map_err(bd)?;
    reponse["groups"] = Value::Array(groupes);
    Ok(ok(&reponse))
}
