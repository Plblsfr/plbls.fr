use super::*;

pub async fn purger(
    pool: &PgPool,
    disque: &Disque,
    age: Duration,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut n = 0;
    loop {
        let orphelins: Vec<(String, String)> = sqlx::query_as(
            "SELECT id::text, key FROM files WHERE message_id IS NULL AND report_id IS NULL AND created_at < now() - make_interval(secs => $1) ORDER BY created_at LIMIT 100",
        )
        .bind(age.as_secs_f64())
        .fetch_all(pool)
        .await?;
        if orphelins.is_empty() {
            return Ok(n);
        }
        for (id, cle) in orphelins {
            disque
                .supprimer(&cle)
                .await
                .map_err(|e| format!("{cle} : {e:?}"))?;
            supprimer_ligne(pool, &id).await?;
            n += 1;
        }
    }
}

pub fn lancer_purge(
    state: AppState,
    intervalle: Duration,
    age: Duration,
    mut arret: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tic =
            tokio::time::interval_at(tokio::time::Instant::now() + intervalle, intervalle);
        loop {
            tokio::select! {
                _ = arret.changed() => return,
                _ = tic.tick() => {}
            }
            let Ok(pool) = state.pool() else {
                continue;
            };
            let disque = Disque::new(&state.config().data_dir);
            match purger(pool, &disque, age).await {
                Err(e) => tracing::error!("purge des fichiers : {e}"),
                Ok(n) if n > 0 => tracing::info!("purge : {n} fichiers supprimés"),
                Ok(_) => {}
            }
        }
    })
}
