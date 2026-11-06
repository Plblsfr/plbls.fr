use std::collections::HashSet;

use axum::{
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    http::write_json,
    messages::{Conv, conv_du_dm, conv_du_salon, publier},
    openapi,
    recherche::salons_visibles,
    servers::{Params, bd, horodatage, param, pool, rfc3339, sans_contenu},
    state::AppState,
};

const SQL_SALONS: &str = "SELECT id, n FROM (
        SELECT ch.id::text AS id, (
            SELECT count(*) FROM messages m
            WHERE m.channel_id = ch.id AND m.author_id <> $1::uuid AND m.created_at > sm.joined_at
              AND m.id > COALESCE((SELECT r.last_id FROM read_states r WHERE r.user_id = $1::uuid AND r.channel_id = ch.id), 0)) AS n
        FROM server_members sm
        JOIN channels ch ON ch.server_id = sm.server_id AND ch.kind = 'text'
        WHERE sm.user_id = $1::uuid) t WHERE n > 0";

const SQL_DMS: &str = "SELECT id, n FROM (
        SELECT d.id::text AS id, (
            SELECT count(*) FROM messages m
            WHERE m.dm_id = d.id AND m.author_id <> $1::uuid
              AND m.id > COALESCE((SELECT r.last_id FROM read_states r WHERE r.user_id = $1::uuid AND r.dm_id = d.id), 0)) AS n
        FROM dm_conversations d
        WHERE d.user_a = $1::uuid OR d.user_b = $1::uuid) t WHERE n > 0";

struct Lecture {
    user_id: String,
    pseudo: String,
    last_id: String,
    read_at: String,
}

impl Lecture {
    fn json(&self) -> Value {
        json!({
            "user_id": self.user_id,
            "pseudo": self.pseudo,
            "last_id": self.last_id,
            "read_at": rfc3339(&self.read_at),
        })
    }
}

async fn non_lus(pool: &PgPool, user: &str) -> Result<Value, sqlx::Error> {
    let mut comptes = Map::new();
    let salons = sqlx::query_as::<_, (String, i64)>(SQL_SALONS)
        .bind(user)
        .fetch_all(pool)
        .await?;
    let visibles: HashSet<String> = if salons.is_empty() {
        HashSet::new()
    } else {
        salons_visibles(pool, user, "").await?.into_iter().collect()
    };
    let mut channels = Vec::new();
    for (id, n) in salons {
        if visibles.contains(&id) {
            comptes.insert(id.clone(), json!(n));
            channels.push(id);
        }
    }
    let mut dms = Vec::new();
    for (id, n) in sqlx::query_as::<_, (String, i64)>(SQL_DMS)
        .bind(user)
        .fetch_all(pool)
        .await?
    {
        comptes.insert(id.clone(), json!(n));
        dms.push(id);
    }
    Ok(json!({ "channels": channels, "dms": dms, "counts": comptes }))
}

fn colonne(conv: &Conv) -> (&'static str, String) {
    match &conv.channel_id {
        Some(id) => ("channel_id", id.clone()),
        None => ("dm_id", conv.dm_id.clone().unwrap_or_default()),
    }
}

async fn marquer_lu(
    pool: &PgPool,
    user: &str,
    conv: &Conv,
) -> Result<Option<Lecture>, sqlx::Error> {
    let (col, id) = colonne(conv);
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE message_mentions mm SET read_at = now() FROM messages m
         WHERE mm.message_id = m.id AND mm.user_id = $1::uuid AND mm.read_at IS NULL AND m.{col} = $2::uuid"
    )))
    .bind(user)
    .bind(&id)
    .execute(pool)
    .await?;
    let ligne = sqlx::query_as::<_, (String, String, Option<String>)>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO read_states (user_id, {col}, last_id)
         SELECT $1::uuid, $2::uuid, max(id) FROM messages WHERE {col} = $2::uuid HAVING max(id) IS NOT NULL
         ON CONFLICT (user_id, {col}) WHERE {col} IS NOT NULL DO UPDATE SET last_id = EXCLUDED.last_id, read_at = now()
             WHERE EXCLUDED.last_id > read_states.last_id
         RETURNING last_id::text, {}, (SELECT pseudo FROM users WHERE id = $1::uuid)",
        horodatage("read_at")
    )))
    .bind(user)
    .bind(&id)
    .fetch_optional(pool)
    .await?;
    Ok(ligne.map(|(last_id, read_at, pseudo)| Lecture {
        user_id: user.to_string(),
        pseudo: pseudo.unwrap_or_default(),
        last_id,
        read_at,
    }))
}

async fn lectures_de(pool: &PgPool, conv: &Conv) -> Result<Vec<Lecture>, sqlx::Error> {
    let selection = format!(
        "SELECT r.user_id::text, u.pseudo, r.last_id::text, {} FROM read_states r JOIN users u ON u.id = r.user_id ",
        horodatage("r.read_at")
    );
    let lignes = match (&conv.channel_id, &conv.server_id) {
        (Some(cid), Some(sid)) => {
            sqlx::query_as::<_, (String, String, String, String)>(sqlx::AssertSqlSafe(format!(
                "{selection}JOIN server_members sm ON sm.user_id = r.user_id AND sm.server_id = $2::uuid WHERE r.channel_id = $1::uuid"
            )))
            .bind(cid)
            .bind(sid)
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as::<_, (String, String, String, String)>(sqlx::AssertSqlSafe(format!(
                "{selection}WHERE r.dm_id = $1::uuid"
            )))
            .bind(conv.dm_id.clone().unwrap_or_default())
            .fetch_all(pool)
            .await?
        }
    };
    Ok(lignes
        .into_iter()
        .map(|(user_id, pseudo, last_id, read_at)| Lecture {
            user_id,
            pseudo,
            last_id,
            read_at,
        })
        .collect())
}

async fn conversation(
    state: &AppState,
    p: &Params,
    user: &str,
    dm: bool,
) -> Result<Conv, Response> {
    let pool = pool(state)?;
    if dm {
        conv_du_dm(pool, param(&p.0, "id"), user).await
    } else {
        conv_du_salon(pool, param(&p.0, "cid"), user).await
    }
}

async fn marquer(state: &AppState, p: &Params, user: &str, dm: bool) -> Result<Response, Response> {
    let conv = conversation(state, p, user, dm).await?;
    let pool = pool(state)?;
    if let Some(lecture) = marquer_lu(pool, user, &conv).await.map_err(bd)? {
        let mut data = match lecture.json() {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let conv_notif = match (&conv.channel_id, &conv.dm_id) {
            (Some(id), _) => {
                data.insert("channel_id".into(), json!(id));
                format!("c:{id}")
            }
            (None, Some(id)) => {
                data.insert("dm_id".into(), json!(id));
                format!("d:{id}")
            }
            _ => String::new(),
        };
        publier(state, pool, &conv, "read.updated", &Value::Object(data)).await;
        state
            .retirer_notification(user, &conv_notif, &lecture.last_id)
            .await;
    }
    Ok(sans_contenu())
}

async fn lister(state: &AppState, p: &Params, user: &str, dm: bool) -> Result<Response, Response> {
    let conv = conversation(state, p, user, dm).await?;
    let lectures = lectures_de(pool(state)?, &conv).await.map_err(bd)?;
    let liste: Vec<Value> = lectures.iter().map(Lecture::json).collect();
    Ok(write_json(StatusCode::OK, &Value::Array(liste)))
}

async fn route_non_lus(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let v = non_lus(pool(&state)?, &u.id).await.map_err(bd)?;
    Ok(write_json(StatusCode::OK, &v))
}

async fn marquer_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    marquer(&state, &p, &u.id, false).await
}

async fn marquer_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    marquer(&state, &p, &u.id, true).await
}

async fn lectures_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    lister(&state, &p, &u.id, false).await
}

async fn lectures_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    lister(&state, &p, &u.id, true).await
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/unread",
        "Conversations contenant des messages non lus",
        &[("200", "ok — {channels, dms, counts}")],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/read",
        "Marquer un salon texte comme lu (ses mentions sont lues avec lui)",
        &[("204", "ok"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Post,
        "/dms/{id}/read",
        "Marquer une conversation privée comme lue (ses mentions sont lues avec elle)",
        &[("204", "ok"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Get,
        "/channels/{cid}/reads",
        "Dernier message vu par chaque membre d'un salon texte",
        &[
            ("200", "liste de {user_id, pseudo, last_id, read_at}"),
            ("404", "introuvable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/reads",
        "Dernier message vu par chaque personne d'une conversation privée",
        &[
            ("200", "liste de {user_id, pseudo, last_id, read_at}"),
            ("404", "introuvable"),
        ],
    ),
];

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/unread", get(route_non_lus))
        .route("/channels/{cid}/read", post(marquer_salon))
        .route("/dms/{id}/read", post(marquer_dm))
        .route("/channels/{cid}/reads", get(lectures_salon))
        .route("/dms/{id}/reads", get(lectures_dm));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::messages::tests::{cadre, recus};
    use crate::state::tests::Enregistreur;

    async fn non_lus_de(c: &crate::messages::tests::Cadre, qui: &str) -> Value {
        let (s, v) = c.api(qui, "GET", "/unread", Value::Null).await;
        assert_eq!(s, 200, "{v}");
        v
    }

    fn vide() -> Value {
        json!({ "channels": [], "dms": [], "counts": {} })
    }

    #[tokio::test]
    async fn non_lus_et_marquage() {
        let Some(c) = cadre().await else { return };
        assert_eq!(non_lus_de(&c, "bob").await, vide());
        c.poster("alice", "salut").await;
        assert_eq!(
            non_lus_de(&c, "bob").await,
            json!({ "channels": [c.salon], "dms": [], "counts": { c.salon.clone(): 1 } })
        );
        assert_eq!(non_lus_de(&c, "alice").await, vide());
        let lu = format!("/channels/{}/read", c.salon);
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert_eq!(non_lus_de(&c, "bob").await, vide());
        c.poster("alice", "un").await;
        c.poster("alice", "deux").await;
        assert_eq!(non_lus_de(&c, "bob").await["counts"][&c.salon], 2);

        c.poster("alice", "coucou @bob").await;
        let mentions = |c: &crate::messages::tests::Cadre| {
            let pool = c.pool().clone();
            let bob = c.bob.clone();
            async move {
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM message_mentions WHERE user_id=$1::uuid AND read_at IS NULL",
                )
                .bind(bob)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        assert_eq!(mentions(&c).await, 1);
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert_eq!(mentions(&c).await, 0);
        assert_eq!(non_lus_de(&c, "bob").await, vide());

        let dm = c.ouvrir_dm("alice", "bob").await;
        let (s, _) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "psst" }),
            )
            .await;
        assert_eq!(s, 201);
        assert_eq!(
            non_lus_de(&c, "bob").await,
            json!({ "channels": [], "dms": [dm], "counts": { dm.clone(): 1 } })
        );
        assert_eq!(non_lus_de(&c, "alice").await, vide());
        let lu_dm = format!("/dms/{dm}/read");
        assert_eq!(c.api("bob", "POST", &lu_dm, Value::Null).await.0, 204);
        assert_eq!(non_lus_de(&c, "bob").await, vide());

        let (s, v) = c
            .api(
                "bob",
                "POST",
                &format!("/channels/{}/read", c.vocal),
                Value::Null,
            )
            .await;
        assert_eq!((s, v["error"].as_str().unwrap()), (400, "not_text_channel"));
        assert_eq!(c.api("admin", "POST", &lu, Value::Null).await.0, 404);
        assert_eq!(c.api("admin", "POST", &lu_dm, Value::Null).await.0, 404);
        assert_eq!(
            c.api("bob", "POST", "/channels/pas-un-uuid/read", Value::Null)
                .await
                .0,
            404
        );
    }

    #[tokio::test]
    async fn non_lus_ignorent_l_avant_adhesion_et_les_salons_masques() {
        let Some(c) = cadre().await else { return };
        c.poster("alice", "avant").await;
        c.sql(&format!(
            "UPDATE server_members SET joined_at = now() + interval '1 hour' WHERE user_id='{}'::uuid",
            c.bob
        ))
        .await;
        assert_eq!(non_lus_de(&c, "bob").await, vide());
        c.sql(&format!(
            "UPDATE server_members SET joined_at = now() - interval '1 hour' WHERE user_id='{}'::uuid",
            c.bob
        ))
        .await;
        assert_eq!(non_lus_de(&c, "bob").await["counts"][&c.salon], 1);

        let (s, prive) = c
            .api(
                "alice",
                "POST",
                &format!("/servers/{}/channels", c.sid),
                json!({ "name": "secret", "kind": "text" }),
            )
            .await;
        assert_eq!(s, 201, "{prive}");
        let prive = prive["id"].as_str().unwrap().to_string();
        let (s, _) = c
            .api(
                "alice",
                "POST",
                &format!("/channels/{prive}/messages"),
                json!({ "content": "confidentiel" }),
            )
            .await;
        assert_eq!(s, 201);
        let v = non_lus_de(&c, "bob").await;
        assert_eq!(v["counts"][&prive], 1);
        assert_eq!(v["channels"].as_array().unwrap().len(), 2);

        let defaut: String = sqlx::query_scalar(
            "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
        )
        .bind(&c.sid)
        .fetch_one(c.pool())
        .await
        .unwrap();
        let (s, _) = c
            .api(
                "alice",
                "PUT",
                &format!("/servers/{}/channels/{prive}/overrides/{defaut}", c.sid),
                json!({ "deny": ["view_channel"] }),
            )
            .await;
        assert_eq!(s, 204);
        assert_eq!(
            non_lus_de(&c, "bob").await,
            json!({ "channels": [c.salon], "dms": [], "counts": { c.salon.clone(): 1 } })
        );
        assert_eq!(non_lus_de(&c, "alice").await["counts"][&prive], Value::Null);
        let v = non_lus_de(&c, "alice").await;
        assert_eq!(v, vide());
    }

    #[tokio::test]
    async fn accuses_de_lecture() {
        let Some(c) = cadre().await else { return };
        let lu = format!("/channels/{}/read", c.salon);
        let liste = format!("/channels/{}/reads", c.salon);
        let mut rx_alice = c.ecouter(&c.alice);
        let mut rx_bob = c.ecouter(&c.bob);

        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert!(recus(&mut rx_alice).is_empty());
        assert_eq!(
            c.api("bob", "GET", &liste, Value::Null).await,
            (200, json!([]))
        );

        let m1 = c.poster("alice", "un").await["id"].clone();
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        let trames = recus(&mut rx_alice);
        assert_eq!(
            trames.iter().filter(|(t, _)| t == "read.updated").count(),
            1
        );
        let (_, data) = trames.iter().find(|(t, _)| t == "read.updated").unwrap();
        assert_eq!(data["channel_id"], json!(c.salon));
        assert!(data.get("dm_id").is_none());
        assert_eq!(data["user_id"], json!(c.bob));
        assert_eq!(data["pseudo"], "bob");
        assert_eq!(data["last_id"], m1);
        assert!(data["read_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(
            recus(&mut rx_bob)
                .iter()
                .filter(|(t, _)| t == "read.updated")
                .count(),
            1
        );

        let m2 = c.poster("alice", "deux").await["id"].clone();
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        let trames = recus(&mut rx_alice);
        let lectures: Vec<_> = trames.iter().filter(|(t, _)| t == "read.updated").collect();
        assert_eq!(lectures.len(), 1);
        assert_eq!(lectures[0].1["last_id"], m2);

        let (s, v) = c.api("alice", "GET", &liste, Value::Null).await;
        assert_eq!(s, 200);
        let v = v.as_array().unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["user_id"], json!(c.bob));
        assert_eq!(v[0]["pseudo"], "bob");
        assert_eq!(v[0]["last_id"], m2);
        assert!(v[0]["read_at"].as_str().unwrap().ends_with('Z'));

        c.sql(&format!(
            "UPDATE read_states SET last_id = last_id + 1000 WHERE user_id='{}'::uuid",
            c.bob
        ))
        .await;
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert!(recus(&mut rx_alice).is_empty());
        let (_, v) = c.api("alice", "GET", &liste, Value::Null).await;
        assert_ne!(v[0]["last_id"], m2);

        assert_eq!(c.api("admin", "GET", &liste, Value::Null).await.0, 404);
        assert_eq!(
            c.api(
                "bob",
                "GET",
                &format!("/channels/{}/reads", c.vocal),
                Value::Null
            )
            .await
            .0,
            400
        );
    }

    #[tokio::test]
    async fn accuses_en_message_prive() {
        let Some(c) = cadre().await else { return };
        let dm = c.ouvrir_dm("alice", "bob").await;
        let mut rx_alice = c.ecouter(&c.alice);
        let (s, m) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "psst" }),
            )
            .await;
        assert_eq!(s, 201);
        recus(&mut rx_alice);
        assert_eq!(
            c.api("bob", "POST", &format!("/dms/{dm}/read"), Value::Null)
                .await
                .0,
            204
        );
        let trames = recus(&mut rx_alice);
        let (_, data) = trames.iter().find(|(t, _)| t == "read.updated").unwrap();
        assert_eq!(data["dm_id"], json!(dm));
        assert!(data.get("channel_id").is_none());
        assert_eq!(data["last_id"], m["id"]);
        assert_eq!(data["user_id"], json!(c.bob));

        let (s, v) = c
            .api("alice", "GET", &format!("/dms/{dm}/reads"), Value::Null)
            .await;
        assert_eq!(s, 200);
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["pseudo"], "bob");
        assert_eq!(
            c.api("admin", "GET", &format!("/dms/{dm}/reads"), Value::Null)
                .await
                .0,
            404
        );
    }

    #[tokio::test]
    async fn retire_la_notification_quand_la_lecture_avance() {
        let Some(c) = cadre().await else { return };
        let faux = Arc::new(Enregistreur(Mutex::default()));
        c.e.state.definir_notifieur(faux.clone());
        let retraits = || {
            faux.0
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.starts_with("retirer:"))
                .cloned()
                .collect::<Vec<_>>()
        };
        let lu = format!("/channels/{}/read", c.salon);
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert!(retraits().is_empty());

        let m = c.poster("alice", "un").await["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert_eq!(retraits(), [format!("retirer:{}:c:{}:{m}", c.bob, c.salon)]);
        assert_eq!(c.api("bob", "POST", &lu, Value::Null).await.0, 204);
        assert_eq!(retraits().len(), 1);

        let dm = c.ouvrir_dm("alice", "bob").await;
        let (_, m) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "psst" }),
            )
            .await;
        assert_eq!(
            c.api("bob", "POST", &format!("/dms/{dm}/read"), Value::Null)
                .await
                .0,
            204
        );
        assert_eq!(
            retraits()[1],
            format!("retirer:{}:d:{dm}:{}", c.bob, m["id"].as_str().unwrap())
        );
    }
}
