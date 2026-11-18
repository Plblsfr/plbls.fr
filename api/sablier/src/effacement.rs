pub fn tables_personnelles() -> &'static [&'static str] {
    &[
        "evenements",
        "agregats_jour",
        "battements",
        "sessions_entracte",
        "deblocages",
        "a_evaluer",
        "recaps",
        "partages_stats",
        "partages_trophees",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::env;

    #[tokio::test]
    async fn toute_table_avec_une_colonne_sub_est_une_table_personnelle() {
        let Some(e) = env().await else { return };
        let mut en_base: Vec<String> = sqlx::query_scalar(
            "SELECT table_name::text FROM information_schema.columns
            WHERE table_schema = 'public' AND column_name = 'sub'",
        )
        .fetch_all(e.state.pool())
        .await
        .unwrap();
        en_base.sort();
        let mut declarees: Vec<&str> = tables_personnelles().to_vec();
        declarees.sort_unstable();
        assert_eq!(en_base, declarees);
    }
}
