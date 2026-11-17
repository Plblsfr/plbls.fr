use axum::{body::Bytes, extract::State, response::Response};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, invalide, ok},
    transcodage::Qualite,
};

pub const QUALITE_PAR_DEFAUT: Qualite = Qualite::Originale;

pub async fn qualite_de(pool: &PgPool, sub: &str) -> Result<Qualite, sqlx::Error> {
    let q: Option<String> = sqlx::query_scalar("SELECT qualite FROM preferences WHERE sub = $1")
        .bind(sub)
        .fetch_optional(pool)
        .await?;
    Ok(q.as_deref()
        .and_then(Qualite::depuis)
        .unwrap_or(QUALITE_PAR_DEFAUT))
}

pub async fn decouverte_de(pool: &PgPool, sub: &str) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar("SELECT decouverte FROM preferences WHERE sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?
            .unwrap_or(true),
    )
}

async fn reponse(pool: &PgPool, sub: &str) -> Result<Response, sqlx::Error> {
    Ok(ok(&json!({
        "quality": qualite_de(pool, sub).await?.nom(),
        "qualities": Qualite::TOUTES.map(Qualite::nom),
        "discovery": decouverte_de(pool, sub).await?,
    })))
}

pub async fn lire(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    reponse(state.pool(), p.sub()).await.map_err(bd)
}

pub async fn ecrire(
    State(state): State<AppState>,
    p: Personne,
    corps: Bytes,
) -> Result<Response, Response> {
    let v: Value = serde_json::from_slice(&corps).map_err(|_| invalide("JSON attendu"))?;
    let q =
        match v.get("quality").filter(|q| !q.is_null()) {
            None => None,
            Some(q) => Some(q.as_str().and_then(Qualite::depuis).ok_or_else(|| {
                invalide("qualité invalide : originale, haute, normale ou basse")
            })?),
        };
    let d = match v.get("discovery").filter(|d| !d.is_null()) {
        None => None,
        Some(d) => Some(
            d.as_bool()
                .ok_or_else(|| invalide("discovery : booléen attendu"))?,
        ),
    };
    if q.is_none() && d.is_none() {
        return Err(invalide("quality ou discovery requis"));
    }
    sqlx::query(
        "INSERT INTO preferences (sub, qualite, decouverte) VALUES ($1, coalesce($2, $4), coalesce($3, true))
         ON CONFLICT (sub) DO UPDATE SET qualite = coalesce($2, preferences.qualite),
           decouverte = coalesce($3, preferences.decouverte), modifie_le = now()",
    )
    .bind(p.sub())
    .bind(q.map(Qualite::nom))
    .bind(d)
    .bind(QUALITE_PAR_DEFAUT.nom())
    .execute(state.pool())
    .await
    .map_err(bd)?;
    reponse(state.pool(), p.sub()).await.map_err(bd)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::testutil::env;

    #[tokio::test]
    async fn decouverte_vraie_par_defaut_et_ecriture_partielle() {
        let Some(e) = env().await else { return };
        let ecrire = |corps| e.appel(Some("alice"), "PUT", "/preferences", Some(corps));
        assert_eq!(
            e.get("alice", "/preferences").await.json()["discovery"],
            true
        );
        let r = ecrire(json!({"discovery": false})).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        assert_eq!(
            (r.json()["discovery"].clone(), r.json()["quality"].clone()),
            (json!(false), json!("originale"))
        );
        let r = ecrire(json!({"quality": "basse"})).await.json();
        assert_eq!(
            (r["discovery"].clone(), r["quality"].clone()),
            (json!(false), json!("basse"))
        );
        let r = ecrire(json!({"discovery": true})).await.json();
        assert_eq!(
            (r["discovery"].clone(), r["quality"].clone()),
            (json!(true), json!("basse"))
        );
        let r = ecrire(json!({"discovery": false, "quality": "haute"})).await;
        assert_eq!(
            e.get("alice", "/preferences").await.json(),
            json!({"quality": "haute", "qualities": ["originale", "haute", "normale", "basse"], "discovery": false})
        );
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(
            e.get("bruno", "/preferences").await.json()["discovery"],
            true
        );
        for faux in [
            json!({"discovery": "non"}),
            json!({"discovery": 0}),
            json!({"quality": "basse", "discovery": "oui"}),
            json!({"autre": 1}),
        ] {
            let r = ecrire(faux.clone()).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
        }
        assert_eq!(
            e.get("alice", "/preferences").await.json()["quality"],
            "haute",
            "refus sans effet"
        );
    }
}
