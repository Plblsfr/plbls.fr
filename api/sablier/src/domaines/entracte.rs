use chrono::NaiveDate;
use commun::evenements::Evenement;
use sqlx::PgConnection;

pub async fn appliquer(
    _tx: &mut PgConnection,
    _e: &Evenement,
    _jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    Ok(())
}
