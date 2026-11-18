pub mod entracte;
pub mod messagerie;
pub mod sillon;

use axum::response::Response;
use chrono::NaiveDate;
use commun::evenements::{Donnees, Evenement, Source};
use serde_json::json;
use sqlx::PgConnection;

use crate::http::ok;

pub async fn appliquer(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    match (&e.donnees, e.donnees.source()) {
        (Donnees::EstompeCinemaTemps { .. } | Donnees::EstompeCinemaSeance {}, _)
        | (_, Source::Entracte) => entracte::appliquer(tx, e, jour).await,
        (_, Source::Sillon) => sillon::appliquer(tx, e, jour).await,
        (_, Source::Estompe) => messagerie::appliquer(tx, e, jour).await,
    }
}

pub fn sans_acces() -> Response {
    ok(&json!({ "access": false }))
}
