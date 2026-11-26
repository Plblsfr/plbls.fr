use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    bilan::Nature,
    classement,
    etat::AppState,
    garde::Personne,
    http::{bd, identifiant, introuvable, invalide, ok},
    releves::{OPERATION, OPERATIONS},
    stats::{Calcul, Categorie, Donnees, Mode, Operation, Releve},
};

#[derive(Debug, Default, Deserialize)]
pub struct Demande {
    pub mode: Option<String>,
    pub period: Option<String>,
}

type LigneOperation = (
    i64,
    i64,
    NaiveDate,
    Option<NaiveDate>,
    Option<i64>,
    String,
    i64,
    Option<i64>,
);

pub async fn charger(pool: &PgPool, sub: &str) -> Result<Donnees, sqlx::Error> {
    let releves: Vec<(i64, NaiveDate, NaiveDate)> =
        sqlx::query_as("SELECT id, debut, date_arrete FROM releves WHERE sub = $1 ORDER BY id")
            .bind(sub)
            .fetch_all(pool)
            .await?;
    let operations: Vec<LigneOperation> = sqlx::query_as(
        "SELECT id, releve_id, date_operation, date_achat, categorie_id, sens, montant, commercant_id
         FROM operations WHERE sub = $1 ORDER BY id",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    let categories: Vec<(i64, String, String, Option<i64>, i32, bool)> = sqlx::query_as(
        "SELECT id, nom, nature, plafond, couleur, archivee FROM categories WHERE sub = $1 ORDER BY rang, id",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    let commercants: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, nom FROM commercants WHERE sub = $1")
            .bind(sub)
            .fetch_all(pool)
            .await?;
    Ok(Donnees {
        releves: releves
            .into_iter()
            .map(|(id, debut, fin)| Releve { id, debut, fin })
            .collect(),
        operations: operations
            .into_iter()
            .map(
                |(id, releve, jour, achat, categorie, sens, montant, commercant)| Operation {
                    id,
                    releve,
                    jour,
                    achat,
                    categorie,
                    credit: sens == "credit",
                    montant,
                    commercant,
                },
            )
            .collect(),
        categories: categories
            .into_iter()
            .filter_map(|(id, nom, nature, plafond, couleur, archivee)| {
                Some(Categorie {
                    id,
                    nom,
                    nature: Nature::lire(&nature)?,
                    plafond,
                    couleur,
                    archivee,
                })
            })
            .collect(),
        commercants: commercants.into_iter().collect::<BTreeMap<_, _>>(),
    })
}

fn mode(d: &Demande) -> Result<Mode, Response> {
    Mode::lire(d.mode.as_deref()).ok_or_else(|| invalide("mode : calendar ou statement"))
}

fn cle(d: &Demande, mode: Mode) -> Result<Option<&str>, Response> {
    let Some(cle) = d.period.as_deref() else {
        return Ok(None);
    };
    let valide = match mode {
        Mode::Calendaire => NaiveDate::parse_from_str(&format!("{cle}-01"), "%Y-%m-%d").is_ok(),
        Mode::Releve => NaiveDate::parse_from_str(cle, "%Y-%m-%d").is_ok(),
    };
    if valide && cle.is_ascii() {
        Ok(Some(cle))
    } else {
        Err(invalide(
            "period : AAAA-MM en mode calendar, date d'arrêté AAAA-MM-JJ en mode statement",
        ))
    }
}

pub async fn lister(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    Ok(ok(
        &json!({ "mode": mode.cle(), "periods": calcul.liste() }),
    ))
}

pub async fn lire(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    Ok(ok(&calcul.mois(i)))
}

pub async fn categorie(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    let mut detail = calcul.detail(i, id).ok_or_else(introuvable)?;
    let lues: Vec<(i64, Value)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT o.id, {OPERATION} FROM {OPERATIONS} WHERE o.sub = $1 AND o.id = ANY($2)"
    )))
    .bind(p.sub())
    .bind(&detail.dernieres)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut par_id: BTreeMap<i64, Value> = lues.into_iter().collect();
    detail.operations = detail
        .dernieres
        .iter()
        .filter_map(|id| par_id.remove(id))
        .collect();
    Ok(ok(&detail))
}
