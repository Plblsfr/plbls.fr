use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    cmb::Releve,
    etat::AppState,
    garde::Personne,
    http::{bd, introuvable, invalide, ok, sans_contenu},
    import::BANQUE,
};

pub const LIMITE_DEFAUT: i64 = 50;
pub const LIMITE_MAX: i64 = 200;

const RELEVE: &str = "SELECT jsonb_build_object(
        'id', r.id, 'account_id', r.compte_id, 'number', r.numero, 'year', r.annee,
        'from', r.debut, 'to', r.date_arrete,
        'old_balance', r.ancien_solde, 'new_balance', r.nouveau_solde,
        'total_debits', r.total_debits, 'total_credits', r.total_credits,
        'operations', (SELECT count(*) FROM operations o WHERE o.releve_id = r.id),
        'pages', r.pages, 'sha256', r.empreinte, 'checks', r.controle, 'imported_at', r.importe_le)
    FROM releves r
    WHERE r.sub = $1 AND ($2::bigint IS NULL OR r.id = $2)
    ORDER BY r.date_arrete DESC, r.id DESC";

pub async fn lire(pool: &PgPool, sub: &str, id: i64) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(RELEVE)
        .bind(sub)
        .bind(Some(id))
        .fetch_one(pool)
        .await
}

pub async fn par_empreinte(
    pool: &PgPool,
    sub: &str,
    empreinte: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM releves WHERE sub = $1 AND empreinte = $2 LIMIT 1")
        .bind(sub)
        .bind(empreinte)
        .fetch_optional(pool)
        .await
}

pub async fn par_identite(
    pool: &PgPool,
    sub: &str,
    r: &Releve,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT r.id FROM releves r JOIN comptes c ON c.id = r.compte_id
         WHERE c.sub = $1 AND c.banque = $2 AND c.iban = $3 AND r.annee = $4 AND r.numero = $5 AND r.date_arrete = $6",
    )
    .bind(sub)
    .bind(BANQUE)
    .bind(&r.compte.iban)
    .bind(r.annee)
    .bind(r.numero)
    .bind(r.date_arrete)
    .fetch_optional(pool)
    .await
}

pub async fn lister(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let releves: Vec<Value> = sqlx::query_scalar(RELEVE)
        .bind(p.sub())
        .bind(None::<i64>)
        .fetch_all(state.pool())
        .await
        .map_err(bd)?;
    Ok(ok(&json!({ "statements": releves })))
}

pub async fn comptes(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let comptes: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'id', c.id, 'bank', c.banque, 'iban', c.iban, 'bic', c.bic, 'number', c.numero,
            'reference', c.reference, 'holder', c.titulaire,
            'statements', (SELECT count(*) FROM releves r WHERE r.compte_id = c.id))
         FROM comptes c WHERE c.sub = $1 ORDER BY c.id",
    )
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({ "accounts": comptes })))
}

pub async fn supprimer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id: i64 = id.parse().map_err(|_| introuvable())?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    let compte: Option<i64> =
        sqlx::query_scalar("DELETE FROM releves WHERE id = $1 AND sub = $2 RETURNING compte_id")
            .bind(id)
            .bind(p.sub())
            .fetch_optional(&mut *tx)
            .await
            .map_err(bd)?;
    let Some(compte) = compte else {
        return Err(introuvable());
    };
    sqlx::query(
        "DELETE FROM comptes c WHERE c.id = $1 AND c.sub = $2
         AND NOT EXISTS (SELECT 1 FROM releves r WHERE r.compte_id = c.id)",
    )
    .bind(compte)
    .bind(p.sub())
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(sans_contenu())
}

#[derive(Debug, Default, Deserialize)]
pub struct Filtre {
    pub statement: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

fn entier(valeur: &Option<String>, message: &str) -> Result<Option<i64>, Response> {
    valeur
        .as_deref()
        .map(|v| v.parse::<i64>().ok().filter(|n| *n >= 0))
        .map(|n| n.ok_or_else(|| invalide(message)))
        .transpose()
}

fn jour(valeur: &Option<String>, message: &str) -> Result<Option<NaiveDate>, Response> {
    valeur
        .as_deref()
        .map(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| invalide(message)))
        .transpose()
}

pub async fn operations(
    State(state): State<AppState>,
    p: Personne,
    Query(f): Query<Filtre>,
) -> Result<Response, Response> {
    let releve = entier(&f.statement, "statement : identifiant d'un relevé")?;
    let du = jour(&f.from, "from : date AAAA-MM-JJ")?;
    let au = jour(&f.to, "to : date AAAA-MM-JJ")?;
    let limite = entier(&f.limit, "limit : entier de 1 à 200")?.map_or(Ok(LIMITE_DEFAUT), |n| {
        (1..=LIMITE_MAX)
            .contains(&n)
            .then_some(n)
            .ok_or_else(|| invalide("limit : entier de 1 à 200"))
    })?;
    let debut = entier(&f.offset, "offset : entier positif")?.unwrap_or(0);
    const OU: &str = "o.sub = $1 AND ($2::bigint IS NULL OR o.releve_id = $2)
        AND ($3::date IS NULL OR o.date_operation >= $3) AND ($4::date IS NULL OR o.date_operation <= $4)";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM operations o WHERE {OU}"
    )))
    .bind(p.sub())
    .bind(releve)
    .bind(du)
    .bind(au)
    .fetch_one(state.pool())
    .await
    .map_err(bd)?;
    let lignes: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_build_object(
            'id', o.id, 'statement_id', o.releve_id, 'account_id', o.compte_id,
            'date', o.date_operation, 'value_date', o.date_valeur, 'purchase_date', o.date_achat,
            'section', o.section, 'type', o.type, 'label', o.libelle, 'details', o.details,
            'merchant', o.commercant_brut, 'amount', o.montant, 'direction', o.sens,
            'merchant_id', o.commercant_id, 'category_id', o.categorie_id)
         FROM operations o WHERE {OU}
         ORDER BY o.date_operation DESC, o.releve_id DESC, o.rang DESC
         LIMIT $5 OFFSET $6"
    )))
    .bind(p.sub())
    .bind(releve)
    .bind(du)
    .bind(au)
    .bind(limite)
    .bind(debut)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "operations": lignes,
        "total": total,
        "limit": limite,
        "offset": debut,
    })))
}
