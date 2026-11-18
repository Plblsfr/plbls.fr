pub mod entracte;
pub mod messagerie;
pub mod sillon;

use chrono::NaiveDate;
use commun::evenements::{Evenement, Source};
use sqlx::PgConnection;

pub async fn appliquer(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    match e.donnees.source() {
        Source::Entracte => entracte::appliquer(tx, e, jour).await,
        Source::Sillon => sillon::appliquer(tx, e, jour).await,
        Source::Estompe => messagerie::appliquer(tx, e, jour).await,
    }
}
