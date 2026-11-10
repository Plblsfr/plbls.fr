use std::collections::HashSet;

use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use serde_json::{Map, Value, json};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    http::{decode, write_err, write_json},
    openapi,
    recherche::salons_visibles,
    servers::{bd, horodatage, mauvais, pool, rfc3339, sans_contenu},
    state::AppState,
};

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/notifications",
        "Mentions non lues (200 max, récentes d'abord)",
        &[("200", "ok"), ("403", "Compte non activé")],
    ),
    (
        HttpMethod::Post,
        "/notifications/read",
        "Marquer des mentions comme lues",
        &[("204", "ok")],
    ),
];

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/notifications", get(lister))
        .route("/notifications/read", post(marquer_lues));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

type Ligne = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    String,
    bool,
);

async fn lister(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let lignes = sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(format!(
        "SELECT m.id::text, m.channel_id::text, ch.server_id::text, m.dm_id::text, u.id::text, u.pseudo, m.content, {}, mm.groupe
         FROM message_mentions mm
         JOIN messages m ON m.id = mm.message_id
         JOIN users u ON u.id = m.author_id
         LEFT JOIN channels ch ON ch.id = m.channel_id
         WHERE mm.user_id=$1::uuid AND mm.read_at IS NULL ORDER BY m.id DESC LIMIT 200",
        horodatage("m.created_at"),
    )))
    .bind(&u.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let visibles: HashSet<String> = if lignes.iter().any(|l| l.1.is_some()) {
        salons_visibles(pool, &u.id, "")
            .await
            .map_err(bd)?
            .into_iter()
            .collect()
    } else {
        HashSet::new()
    };
    let sortie: Vec<Value> = lignes
        .into_iter()
        .filter(|l| l.1.as_ref().is_none_or(|s| visibles.contains(s)))
        .map(
            |(id, salon, serveur, dm, auteur, pseudo, contenu, date, groupe)| {
                let mut m = Map::new();
                m.insert("message_id".into(), id.into());
                if let Some(s) = salon {
                    m.insert("channel_id".into(), s.into());
                }
                if let Some(s) = serveur {
                    m.insert("server_id".into(), s.into());
                }
                if let Some(d) = dm {
                    m.insert("dm_id".into(), d.into());
                }
                m.insert("author".into(), json!({ "id": auteur, "pseudo": pseudo }));
                m.insert("content".into(), contenu.into());
                m.insert("created_at".into(), rfc3339(&date).into());
                if groupe {
                    m.insert("group".into(), true.into());
                }
                Value::Object(m)
            },
        )
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(sortie)))
}

async fn marquer_lues(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let e = decode(body).await?;
    let ids = crate::messages::liste_stricte(&e, "message_ids")
        .map_err(|()| mauvais())?
        .unwrap_or_default();
    let Ok(numeriques) = ids
        .iter()
        .map(|i| i.parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "identifiant invalide",
        ));
    };
    if numeriques.is_empty() {
        sqlx::query(
            "UPDATE message_mentions SET read_at=now() WHERE user_id=$1::uuid AND read_at IS NULL",
        )
        .bind(&u.id)
        .execute(pool)
        .await
        .map_err(bd)?;
    } else {
        sqlx::query(
            "UPDATE message_mentions SET read_at=now() WHERE user_id=$1::uuid AND read_at IS NULL AND message_id = ANY($2::bigint[])",
        )
        .bind(&u.id)
        .bind(&numeriques)
        .execute(pool)
        .await
        .map_err(bd)?;
    }
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::messages::tests::cadre;

    #[tokio::test]
    async fn notifications_filtrees_et_purgees_au_depart() {
        let Some(c) = cadre().await else { return };
        let notifs = |qui: &'static str| {
            let c = &c;
            async move {
                c.api(qui, "GET", "/notifications", Value::Null)
                    .await
                    .1
                    .as_array()
                    .unwrap()
                    .len()
            }
        };
        let en_base = || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM message_mentions WHERE user_id=$1::uuid",
            )
            .bind(&c.bob)
            .fetch_one(c.pool())
            .await
            .unwrap()
        };
        c.poster("alice", "@bob salon").await;
        let dm = c.ouvrir_dm("alice", "bob").await;
        let (s, _) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "@bob privé" }),
            )
            .await;
        assert_eq!(s, 201);
        assert_eq!(notifs("bob").await, 2);

        let defaut: String = sqlx::query_scalar(
            "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
        )
        .bind(&c.sid)
        .fetch_one(c.pool())
        .await
        .unwrap();
        let surcharge = format!("/servers/{}/channels/{}/overrides/{defaut}", c.sid, c.salon);
        assert_eq!(
            c.api(
                "alice",
                "PUT",
                &surcharge,
                json!({ "deny": ["view_channel"] })
            )
            .await
            .0,
            204
        );
        let (_, l) = c.api("bob", "GET", "/notifications", Value::Null).await;
        assert_eq!(l.as_array().unwrap().len(), 1);
        assert_eq!(l[0]["dm_id"], dm);
        assert_eq!(
            c.api("alice", "PUT", &surcharge, json!({ "deny": [] }))
                .await
                .0,
            204
        );
        assert_eq!(notifs("bob").await, 2);

        let sid = c.sid.clone();
        let rejoindre = || async {
            let (_, v) = c
                .api(
                    "alice",
                    "POST",
                    &format!("/servers/{sid}/invites"),
                    json!({}),
                )
                .await;
            let code = v["code"].as_str().unwrap().to_string();
            assert_eq!(
                c.api(
                    "bob",
                    "POST",
                    &format!("/server-invites/{code}/join"),
                    Value::Null
                )
                .await
                .0,
                200
            );
            c.poster("alice", "@bob encore").await;
        };
        assert_eq!(
            c.api("bob", "POST", &format!("/servers/{sid}/leave"), Value::Null)
                .await
                .0,
            204
        );
        assert_eq!((en_base().await, notifs("bob").await), (1, 1));
        rejoindre().await;
        assert_eq!(en_base().await, 2);
        let kick = format!("/servers/{sid}/members/{}/kick", c.bob);
        assert_eq!(c.api("alice", "POST", &kick, json!({})).await.0, 204);
        assert_eq!(en_base().await, 1);
        rejoindre().await;
        let ban = format!("/servers/{sid}/members/{}/ban", c.bob);
        let (s, v) = c
            .api(
                "alice",
                "POST",
                &ban,
                json!({ "reason": "", "duration": 0, "delete_messages": 0 }),
            )
            .await;
        assert_eq!(s, 204, "{v}");
        assert_eq!(en_base().await, 1);
    }

    #[tokio::test]
    async fn notifications_internes() {
        let Some(c) = cadre().await else { return };
        let mut ids = Vec::new();
        for i in 0..3 {
            let m = c.poster("bob", &format!("{i} @alice")).await;
            ids.push(m["id"].as_str().unwrap().to_string());
        }
        let dm = c.ouvrir_dm("bob", "alice").await;
        let (s, m) = c
            .api(
                "bob",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "@alice en privé" }),
            )
            .await;
        assert_eq!(s, 201);
        let (s, l) = c.api("alice", "GET", "/notifications", Value::Null).await;
        assert_eq!((s, l.as_array().unwrap().len()), (200, 4));
        assert_eq!(l[0]["message_id"], m["id"]);
        assert_eq!(l[0]["dm_id"], dm);
        assert!(l[0].get("channel_id").is_none() && l[0].get("server_id").is_none());
        assert_eq!(l[1]["message_id"], json!(ids[2]));
        assert_eq!(l[1]["channel_id"], c.salon);
        assert_eq!(l[1]["server_id"], c.sid);
        assert!(l[1].get("dm_id").is_none());
        assert_eq!(l[1]["author"], json!({ "id": c.bob, "pseudo": "bob" }));
        assert_eq!(l[1]["content"], "2 @alice");
        assert!(l[1]["created_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(
            c.api("bob", "GET", "/notifications", Value::Null).await.1,
            json!([])
        );

        let (s, v) = c
            .api(
                "alice",
                "POST",
                "/notifications/read",
                json!({ "message_ids": ["abc"] }),
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str(), v["message"].as_str()),
            (400, Some("bad_request"), Some("identifiant invalide"))
        );
        assert_eq!(
            c.api(
                "alice",
                "POST",
                "/notifications/read",
                json!({ "message_ids": [ids[0], ids[1]] })
            )
            .await
            .0,
            204
        );
        let (_, l) = c.api("alice", "GET", "/notifications", Value::Null).await;
        assert_eq!(l.as_array().unwrap().len(), 2);
        assert_eq!(
            c.api("alice", "POST", "/notifications/read", json!({}))
                .await
                .0,
            204
        );
        assert_eq!(
            c.api("alice", "GET", "/notifications", Value::Null).await.1,
            json!([])
        );

        c.sql(&format!(
            "WITH m AS (INSERT INTO messages (channel_id, author_id, content) SELECT '{}'::uuid, '{}'::uuid, 'x' FROM generate_series(1, 205) RETURNING id)
             INSERT INTO message_mentions (message_id, user_id) SELECT id, '{}'::uuid FROM m",
            c.salon, c.bob, c.alice
        ))
        .await;
        let (_, l) = c.api("alice", "GET", "/notifications", Value::Null).await;
        assert_eq!(l.as_array().unwrap().len(), 200);
        let a = l[0]["message_id"].as_str().unwrap().parse::<i64>().unwrap();
        let b = l[199]["message_id"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap();
        assert!(a > b);
    }
}
