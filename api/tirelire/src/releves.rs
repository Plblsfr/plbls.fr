use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::Response,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::{
    categories, classement,
    cmb::Releve,
    commercants,
    etat::AppState,
    garde::Personne,
    http::{bd, corps, identifiant, introuvable, invalide, ok, sans_contenu},
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

pub const OPERATION: &str = "jsonb_build_object(
    'id', o.id, 'statement_id', o.releve_id, 'account_id', o.compte_id,
    'date', o.date_operation, 'value_date', o.date_valeur, 'purchase_date', o.date_achat,
    'section', o.section, 'type', o.type, 'label', o.libelle, 'details', o.details,
    'merchant', o.commercant_brut, 'amount', o.montant, 'direction', o.sens,
    'merchant_id', o.commercant_id, 'merchant_name', m.nom,
    'category_id', o.categorie_id, 'category_source', o.origine, 'rule_id', o.regle_id)";

pub const OPERATIONS: &str = "operations o LEFT JOIN commercants m ON m.id = o.commercant_id";

#[derive(Debug, Default, Deserialize)]
pub struct Filtre {
    pub statement: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub category: Option<String>,
    pub merchant: Option<String>,
    pub unclassified: Option<String>,
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

fn booleen(valeur: &Option<String>, message: &str) -> Result<bool, Response> {
    match valeur.as_deref() {
        None | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(_) => Err(invalide(message)),
    }
}

pub async fn operations(
    State(state): State<AppState>,
    p: Personne,
    Query(f): Query<Filtre>,
) -> Result<Response, Response> {
    let releve = entier(&f.statement, "statement : identifiant d'un relevé")?;
    let du = jour(&f.from, "from : date AAAA-MM-JJ")?;
    let au = jour(&f.to, "to : date AAAA-MM-JJ")?;
    let categorie = entier(&f.category, "category : identifiant d'une catégorie")?;
    let commercant = entier(&f.merchant, "merchant : identifiant d'un commerçant")?;
    let a_classer = booleen(&f.unclassified, "unclassified : true ou false")?;
    let limite = entier(&f.limit, "limit : entier de 1 à 200")?.map_or(Ok(LIMITE_DEFAUT), |n| {
        (1..=LIMITE_MAX)
            .contains(&n)
            .then_some(n)
            .ok_or_else(|| invalide("limit : entier de 1 à 200"))
    })?;
    let debut = entier(&f.offset, "offset : entier positif")?.unwrap_or(0);
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    const OU: &str = "o.sub = $1 AND ($2::bigint IS NULL OR o.releve_id = $2)
        AND ($3::date IS NULL OR o.date_operation >= $3) AND ($4::date IS NULL OR o.date_operation <= $4)
        AND ($5::bigint IS NULL OR o.categorie_id = $5) AND ($6::bigint IS NULL OR o.commercant_id = $6)
        AND (NOT $7 OR o.categorie_id IS NULL)";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM operations o WHERE {OU}"
    )))
    .bind(p.sub())
    .bind(releve)
    .bind(du)
    .bind(au)
    .bind(categorie)
    .bind(commercant)
    .bind(a_classer)
    .fetch_one(state.pool())
    .await
    .map_err(bd)?;
    let lignes: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {OPERATION} FROM {OPERATIONS} WHERE {OU}
         ORDER BY o.date_operation DESC, o.releve_id DESC, o.rang DESC
         LIMIT $8 OFFSET $9"
    )))
    .bind(p.sub())
    .bind(releve)
    .bind(du)
    .bind(au)
    .bind(categorie)
    .bind(commercant)
    .bind(a_classer)
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

async fn une(c: &mut PgConnection, sub: &str, id: i64) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {OPERATION} FROM {OPERATIONS} WHERE o.sub = $1 AND o.id = $2"
    )))
    .bind(sub)
    .bind(id)
    .fetch_optional(&mut *c)
    .await
}

pub async fn operation(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let mut c = state.pool().acquire().await.map_err(bd)?;
    let operation = une(&mut c, p.sub(), id)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    Ok(ok(&json!({ "operation": operation })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reclassement {
    category_id: Option<i64>,
    scope: String,
}

pub async fn classer(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let r: Reclassement = corps(&octets, "corps attendu : {category_id, scope}")?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let commercant: Option<Option<i64>> =
        sqlx::query_scalar("SELECT commercant_id FROM operations WHERE sub = $1 AND id = $2")
            .bind(p.sub())
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(bd)?;
    let Some(commercant) = commercant else {
        return Err(introuvable());
    };
    let reclassees = match (r.scope.as_str(), commercant) {
        ("merchant", Some(commercant)) => {
            sqlx::query(
                "UPDATE operations SET categorie_id = NULL, origine = NULL, regle_id = NULL
                 WHERE sub = $1 AND id = $2 AND origine = 'operation'",
            )
            .bind(p.sub())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(bd)?;
            commercants::classer_en(&mut tx, p.sub(), commercant, r.category_id).await?
        }
        ("merchant", None) => {
            return Err(invalide(
                "scope : cette opération n'a pas de commerçant, seule « operation » convient",
            ));
        }
        ("operation", _) => {
            if let Some(categorie) = r.category_id {
                categories::exiger(&mut tx, p.sub(), categorie).await?;
            }
            sqlx::query(
                "UPDATE operations SET categorie_id = $3, regle_id = NULL,
                    origine = CASE WHEN $3::bigint IS NULL THEN NULL ELSE 'operation' END
                 WHERE sub = $1 AND id = $2",
            )
            .bind(p.sub())
            .bind(id)
            .bind(r.category_id)
            .execute(&mut *tx)
            .await
            .map_err(bd)?;
            classement::rejouer(&mut tx, p.sub()).await.map_err(bd)?;
            1
        }
        _ => return Err(invalide("scope : operation ou merchant")),
    };
    let operation = une(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(
        &json!({ "operation": operation, "reclassified": reclassees }),
    ))
}
