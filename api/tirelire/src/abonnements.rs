use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    classement,
    etat::AppState,
    garde::Personne,
    http::{bd, corps, introuvable, invalide, ok},
    mois::charger,
    recurrents::{Etat, Etats, Recurrents, recurrents},
};

pub async fn etats(pool: &PgPool, sub: &str) -> Result<Etats, sqlx::Error> {
    let lignes: Vec<(i64, String, bool, bool, Option<i64>)> = sqlx::query_as(
        "SELECT commercant_id, cle, ecarte, nouveau_vu, montant_vu FROM recurrents WHERE sub = $1",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|(commercant, cle, ecarte, nouveau_vu, montant_vu)| {
            (
                (commercant, cle),
                Etat {
                    ecarte,
                    nouveau_vu,
                    montant_vu,
                },
            )
        })
        .collect())
}

async fn calculer(pool: &PgPool, sub: &str) -> Result<Recurrents, sqlx::Error> {
    let donnees = charger(pool, sub).await?;
    Ok(recurrents(&donnees, &etats(pool, sub).await?))
}

pub async fn lire(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    Ok(ok(&calculer(state.pool(), p.sub()).await.map_err(bd)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Modification {
    dismissed: Option<bool>,
    seen: Option<bool>,
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let m: Modification = corps(&octets, "corps attendu : {dismissed} ou {seen}")?;
    if m.dismissed.is_none() && m.seen != Some(true) {
        return Err(invalide("dismissed : booléen, ou seen : true"));
    }
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let actuels = calculer(state.pool(), p.sub()).await.map_err(bd)?;
    let r = actuels
        .items
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(introuvable)?;
    let nouveau = m.seen == Some(true) && r.alerts.iter().any(|a| a.kind == "new");
    let montant = r
        .alerts
        .iter()
        .find(|a| m.seen == Some(true) && a.kind == "amount_changed")
        .map(|a| a.new_amount);
    sqlx::query(
        "INSERT INTO recurrents (sub, commercant_id, cle, ecarte, nouveau_vu, montant_vu)
         VALUES ($1, $2, $3, COALESCE($4, false), $5, $6)
         ON CONFLICT (sub, commercant_id, cle) DO UPDATE SET
            ecarte = COALESCE($4, recurrents.ecarte),
            nouveau_vu = recurrents.nouveau_vu OR $5,
            montant_vu = COALESCE($6, recurrents.montant_vu),
            modifie_le = now()",
    )
    .bind(p.sub())
    .bind(r.commercant)
    .bind(&r.cle)
    .bind(m.dismissed)
    .bind(nouveau)
    .bind(montant)
    .execute(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&calculer(state.pool(), p.sub()).await.map_err(bd)?))
}
