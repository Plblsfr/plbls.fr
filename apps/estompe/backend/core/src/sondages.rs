use std::{collections::HashMap, sync::OnceLock};

use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{get, post, put},
};
use regex::Regex;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    fichiers::est_admin,
    http::{decode, write_err, write_json},
    messages::{self, Conv},
    moderation::corps_facultatif,
    openapi,
    servers::{Params, bd, horodatage, mauvais, non_trouve, param, pool, rfc3339, sans_contenu},
    state::AppState,
    users::User,
};

const OPTIONS_MIN: usize = 2;
const OPTIONS_MAX: usize = 10;
const LIBELLE_MAX: usize = 100;

pub struct EntreeSondage {
    pub options: Vec<String>,
    pub multiple: bool,
    pub fin: Option<String>,
}

pub struct NouveauSondage {
    pub options: Vec<String>,
    pub multiple: bool,
    pub fin: Option<String>,
    pub lanceur: String,
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/messages/{id}/vote", put(voter).delete(voter))
        .route("/messages/{id}/poll/votes", get(votants))
        .route("/messages/{id}/poll/close", post(cloturer));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Put,
        "/messages/{id}/vote",
        "Voter pour un choix d'un sondage (en choix unique, il remplace le précédent)",
        &[
            ("204", "ok"),
            ("400", "invalid_option"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "poll_closed"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/messages/{id}/vote",
        "Retirer son vote (sans `option_id` : tous ses votes)",
        &[
            ("204", "ok"),
            ("400", "invalid_option"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "poll_closed"),
        ],
    ),
    (
        HttpMethod::Get,
        "/messages/{id}/poll/votes",
        "Votants de chaque choix d'un sondage (tout participant de la conversation)",
        &[
            ("200", "ok"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Post,
        "/messages/{id}/poll/close",
        "Clore un sondage sans attendre l'échéance (lanceur, modérateur ou admin)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
];

pub fn rfc3339_valide(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(?:\.[0-9]+)?(?:Z|[+-]([0-9]{2}):([0-9]{2}))$",
        )
        .expect("motif invalide")
    });
    let Some(c) = re.captures(s) else {
        return false;
    };
    let n = |i: usize| {
        c.get(i)
            .map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(99))
    };
    let (annee, mois, jour) = (n(1), n(2), n(3));
    let bissextile = annee % 4 == 0 && (annee % 100 != 0 || annee % 400 == 0);
    let jours = match mois {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if bissextile => 29,
        2 => 28,
        _ => return false,
    };
    annee >= 1
        && (1..=jours).contains(&jour)
        && n(4) < 24
        && n(5) < 60
        && n(6) < 60
        && (c.get(7).is_none() || (n(7) <= 15 && n(8) < 60))
}

pub fn lire_entree(o: &Map<String, Value>) -> Option<EntreeSondage> {
    let options = match o.get("options") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
    };
    let multiple = match o.get("multiple") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    let fin = match o.get("closes_at") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if rfc3339_valide(s) => Some(s.clone()),
        Some(_) => return None,
    };
    Some(EntreeSondage {
        options,
        multiple,
        fin,
    })
}

pub fn options_valides(options: &[String]) -> Option<Vec<String>> {
    let mut sortie = Vec::with_capacity(options.len());
    for o in options {
        let o = o.trim();
        if o.is_empty() || o.chars().count() > LIBELLE_MAX {
            return None;
        }
        sortie.push(o.to_string());
    }
    (OPTIONS_MIN..=OPTIONS_MAX)
        .contains(&sortie.len())
        .then_some(sortie)
}

pub fn sondage_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_poll",
        "sondage : une question, 2 à 10 choix de 100 caractères maximum et une clôture dans le futur",
    )
}

pub async fn dans_le_futur(pool: &PgPool, fin: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT $1::timestamptz > now()")
        .bind(fin)
        .fetch_one(pool)
        .await
}

pub async fn inserer(
    tx: &mut PgConnection,
    message: i64,
    s: &NouveauSondage,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO polls (message_id, multiple, closes_at, created_by) VALUES ($1,$2,$3::timestamptz,$4::uuid)",
    )
    .bind(message)
    .bind(s.multiple)
    .bind(&s.fin)
    .bind(&s.lanceur)
    .execute(&mut *tx)
    .await?;
    for (i, libelle) in s.options.iter().enumerate() {
        sqlx::query("INSERT INTO poll_options (message_id, label, position) VALUES ($1,$2,$3)")
            .bind(message)
            .bind(libelle)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

pub async fn hydrater(
    pool: &PgPool,
    messages: &mut [Map<String, Value>],
    index: &HashMap<String, usize>,
    ids: &[i64],
    viewer: &str,
) -> Result<(), sqlx::Error> {
    let meta = sqlx::query_as::<_, (String, bool, Option<String>, String, String)>(sqlx::AssertSqlSafe(format!(
        "SELECT p.message_id::text, p.multiple, {}, u.id::text, u.pseudo FROM polls p JOIN users u ON u.id = p.created_by WHERE p.message_id = ANY($1::bigint[])",
        horodatage("p.closes_at"),
    )))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    for (mid, multiple, fin, uid, pseudo) in meta {
        if let Some(&i) = index.get(&mid) {
            messages[i].insert(
                "poll".into(),
                json!({
                    "multiple": multiple,
                    "closes_at": fin.map_or(Value::Null, |f| Value::String(rfc3339(&f))),
                    "created_by": { "id": uid, "pseudo": pseudo },
                    "options": [],
                }),
            );
        }
    }
    let options = sqlx::query_as::<_, (String, String, String, i64, bool)>(
        "SELECT o.message_id::text, o.id::text, o.label, count(v.user_id), coalesce(bool_or(v.user_id=$2::uuid), false)
         FROM poll_options o LEFT JOIN poll_votes v ON v.option_id = o.id
         WHERE o.message_id = ANY($1::bigint[]) GROUP BY o.id ORDER BY o.message_id, o.position",
    )
    .bind(ids)
    .bind(viewer)
    .fetch_all(pool)
    .await?;
    for (mid, id, libelle, votes, moi) in options {
        if let Some(&i) = index.get(&mid)
            && let Some(l) = messages[i]
                .get_mut("poll")
                .and_then(|p| p.get_mut("options"))
                .and_then(Value::as_array_mut)
        {
            l.push(json!({ "id": id, "label": libelle, "votes": votes, "me": moi }));
        }
    }
    Ok(())
}

pub async fn peut_gerer(
    state: &AppState,
    pool: &PgPool,
    message: i64,
    u: &User,
) -> Result<bool, sqlx::Error> {
    let admin = est_admin(state, &u.sub).await;
    let ok: Option<bool> = sqlx::query_scalar(
        "SELECT $3 OR p.created_by = $2::uuid FROM polls p WHERE p.message_id = $1",
    )
    .bind(message)
    .bind(&u.id)
    .bind(admin)
    .fetch_optional(pool)
    .await?;
    Ok(ok.unwrap_or(false))
}

enum EchecVote {
    Option,
    Clos,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for EchecVote {
    fn from(e: sqlx::Error) -> Self {
        EchecVote::Base(e)
    }
}

async fn enregistrer_vote(
    pool: &PgPool,
    message: i64,
    user: &str,
    option: Option<i64>,
    retrait: bool,
) -> Result<Vec<String>, EchecVote> {
    let mut tx = pool.begin().await?;
    let sondage: Option<(bool, bool)> = sqlx::query_as(
        "SELECT multiple, coalesce(closes_at <= now(), false) FROM polls WHERE message_id=$1 FOR UPDATE",
    )
    .bind(message)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((multiple, clos)) = sondage else {
        return Err(EchecVote::Option);
    };
    if clos {
        return Err(EchecVote::Clos);
    }
    if retrait {
        sqlx::query(
            "DELETE FROM poll_votes WHERE message_id=$1 AND user_id=$2::uuid AND ($3::bigint IS NULL OR option_id = $3)",
        )
        .bind(message)
        .bind(user)
        .bind(option)
        .execute(&mut *tx)
        .await?;
    } else {
        let existe: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM poll_options WHERE id=$1 AND message_id=$2)",
        )
        .bind(option)
        .bind(message)
        .fetch_one(&mut *tx)
        .await?;
        if !existe {
            return Err(EchecVote::Option);
        }
        if !multiple {
            sqlx::query("DELETE FROM poll_votes WHERE message_id=$1 AND user_id=$2::uuid")
                .bind(message)
                .bind(user)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO poll_votes (message_id, user_id, option_id) VALUES ($1,$2::uuid,$3) ON CONFLICT DO NOTHING",
        )
        .bind(message)
        .bind(user)
        .bind(option)
        .execute(&mut *tx)
        .await?;
    }
    let miens: Vec<String> = sqlx::query_scalar(
        "SELECT option_id::text FROM poll_votes WHERE message_id=$1 AND user_id=$2::uuid",
    )
    .bind(message)
    .bind(user)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(miens)
}

fn option_invalide() -> Response {
    write_err(StatusCode::BAD_REQUEST, "invalid_option", "choix invalide")
}

async fn voter(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    methode: axum::http::Method,
    p: Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = messages::id_message(param(&p.0, "id"))?;
    let (conv, _) = messages::conv_du_message(pool, id, &u.id).await?;
    let retrait = methode == axum::http::Method::DELETE;
    let e = if retrait {
        corps_facultatif(body).await?
    } else {
        decode(body).await?
    };
    let brut = match e.get("option_id") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(mauvais()),
    };
    let option = brut.parse::<i64>().ok();
    if option.is_none() && (!retrait || !brut.is_empty()) {
        return Ok(option_invalide());
    }
    let miens = match enregistrer_vote(pool, id, &u.id, option, retrait).await {
        Ok(m) => m,
        Err(EchecVote::Option) => return Ok(option_invalide()),
        Err(EchecVote::Clos) => {
            return Ok(write_err(
                StatusCode::CONFLICT,
                "poll_closed",
                "le sondage est clos",
            ));
        }
        Err(EchecVote::Base(e)) => return Err(bd(e)),
    };
    let comptes: Vec<(String, i64)> = sqlx::query_as(
        "SELECT o.id::text, count(v.user_id) FROM poll_options o
         LEFT JOIN poll_votes v ON v.option_id = o.id WHERE o.message_id=$1 GROUP BY o.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let comptes: Map<String, Value> = comptes.into_iter().map(|(k, n)| (k, n.into())).collect();
    let charge = json!({ "message_id": param(&p.0, "id"), "user_id": u.id, "options": miens, "counts": comptes });
    messages::publier(&state, pool, &conv, "poll.voted", &charge).await;
    Ok(sans_contenu())
}

async fn votants(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = messages::id_message(param(&p.0, "id"))?;
    messages::conv_du_message(pool, id, &u.id).await?;
    let lignes: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT v.option_id::text, u.id::text, u.pseudo FROM poll_votes v JOIN users u ON u.id = v.user_id
         WHERE v.message_id=$1 ORDER BY u.pseudo",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let mut sortie: Map<String, Value> = Map::new();
    for (option, uid, pseudo) in lignes {
        if let Value::Array(l) = sortie
            .entry(option)
            .or_insert_with(|| Value::Array(Vec::new()))
        {
            l.push(json!({ "id": uid, "pseudo": pseudo }));
        }
    }
    Ok(write_json(StatusCode::OK, &Value::Object(sortie)))
}

async fn cloturer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = messages::id_message(param(&p.0, "id"))?;
    let (conv, _): (Conv, String) = messages::conv_du_message(pool, id, &u.id).await?;
    let peut = peut_gerer(&state, pool, id, &u).await.map_err(bd)?;
    if !peut && !conv.peut(crate::roles::MANAGE_MESSAGES) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "seuls le lanceur, les modérateurs et les admins peuvent clore le sondage",
        ));
    }
    let modifies = sqlx::query(
        "UPDATE polls SET closes_at = now() WHERE message_id=$1 AND (closes_at IS NULL OR closes_at > now())",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(bd)?
    .rows_affected();
    if modifies == 0 {
        let existe: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM polls WHERE message_id=$1)")
                .bind(id)
                .fetch_one(pool)
                .await
                .map_err(bd)?;
        if !existe {
            return Ok(non_trouve("introuvable"));
        }
    }
    let maintenant: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {}",
        horodatage("now()")
    )))
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    let charge = json!({ "message_id": param(&p.0, "id"), "closes_at": rfc3339(&maintenant) });
    messages::publier(&state, pool, &conv, "poll.closed", &charge).await;
    Ok(sans_contenu())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        messages::tests::{Cadre, cadre, recus},
        servers::tests::{appel, invitation_de},
    };

    async fn lancer(c: &Cadre, corps: Value) -> (u16, Value) {
        c.api("alice", "POST", &c.chemin(), corps).await
    }

    #[test]
    fn dates_rfc3339() {
        for bonne in [
            "2999-01-01T00:00:00Z",
            "2999-12-31T23:59:59.123456789+02:00",
            "2028-02-29T10:00:00-05:30",
        ] {
            assert!(rfc3339_valide(bonne), "{bonne}");
        }
        for mauvaise in [
            "demain",
            "2999-01-01",
            "2999-01-01 00:00:00Z",
            "2999-13-01T00:00:00Z",
            "2027-02-29T00:00:00Z",
            "2999-01-01T24:00:00Z",
            "2999-01-01T00:00:60Z",
            "2999-01-01T00:00:00",
            "2999-01-01t00:00:00z",
            "0000-01-01T00:00:00Z",
            "2999-01-01T00:00:00+24:60",
            "2999-01-01T00:00:00+16:00",
        ] {
            assert!(!rfc3339_valide(mauvaise), "{mauvaise}");
        }
    }

    #[tokio::test]
    async fn sondage_complet() {
        let Some(c) = cadre().await else { return };
        let code = invitation_de(&c.e, "alice", &c.sid, "").await;
        let rejoindre = format!("/server-invites/{code}/join");
        assert_eq!(
            appel(&c.e, "admin", "POST", &rejoindre, None)
                .await
                .0
                .as_u16(),
            200
        );
        let mut rx_bob = c.ecouter(&c.bob);

        for corps in [
            json!({ "content": "Q ?", "poll": { "options": ["seul"] } }),
            json!({ "content": "Q ?", "poll": { "options": ["a", "  "] } }),
            json!({ "content": "Q ?", "poll": { "options": ["a", "b".repeat(101)] } }),
            json!({ "content": "Q ?", "poll": { "options": (0..11).map(|i| i.to_string()).collect::<Vec<_>>() } }),
            json!({ "content": "Q ?", "poll": {} }),
            json!({ "content": "Q ?", "poll": { "options": ["a", "b"], "closes_at": "2000-01-01T00:00:00Z" } }),
        ] {
            let (s, v) = lancer(&c, corps.clone()).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (400, Some("invalid_poll")),
                "{corps}"
            );
        }
        let (s, v) = lancer(
            &c,
            json!({ "content": "", "poll": { "options": ["a", "b"] } }),
        )
        .await;
        assert_eq!((s, v["error"].as_str()), (400, Some("invalid_message")));
        let f = c.fichier("alice", "file").await;
        let (s, v) = lancer(
            &c,
            json!({ "content": "Q ?", "file_ids": [f], "poll": { "options": ["a", "b"] } }),
        )
        .await;
        assert_eq!((s, v["error"].as_str()), (400, Some("invalid_poll")));
        for corps in [
            json!({ "content": "Q ?", "poll": { "options": ["a", "b"], "closes_at": "demain" } }),
            json!({ "content": "Q ?", "poll": { "options": "a" } }),
            json!({ "content": "Q ?", "poll": { "options": ["a", "b"], "multiple": "oui" } }),
            json!({ "content": "Q ?", "poll": 3 }),
        ] {
            let (s, v) = lancer(&c, corps.clone()).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (400, Some("bad_request")),
                "{corps}"
            );
        }

        let (s, m) = lancer(&c, json!({ "content": " Q ? ", "poll": { "options": [" a ", "b"], "closes_at": "2999-01-01T00:00:00Z" } })).await;
        assert_eq!(s, 201, "{m}");
        assert_eq!(m["author"]["pseudo"], "Estompe");
        assert_eq!(
            m["poll"]["created_by"],
            json!({ "id": c.alice, "pseudo": "alice" })
        );
        assert_eq!(m["poll"]["closes_at"], "2999-01-01T00:00:00Z");
        assert_eq!(m["poll"]["multiple"], false);
        let (a, b) = (
            m["poll"]["options"][0]["id"].as_str().unwrap().to_string(),
            m["poll"]["options"][1]["id"].as_str().unwrap().to_string(),
        );
        assert_eq!(m["poll"]["options"][0]["label"], "a");
        let id = m["id"].as_str().unwrap();
        let vote = format!("/messages/{id}/vote");
        recus(&mut rx_bob);

        assert_eq!(
            c.api("bob", "PUT", &vote, json!({ "option_id": a }))
                .await
                .0,
            204
        );
        let evenements = recus(&mut rx_bob);
        assert_eq!(
            evenements,
            [(
                "poll.voted".to_string(),
                json!({ "message_id": id, "user_id": c.bob, "options": [a], "counts": { a.clone(): 1, b.clone(): 0 } })
            )]
        );
        assert_eq!(
            c.api("bob", "PUT", &vote, json!({ "option_id": b }))
                .await
                .0,
            204
        );
        let (_, p) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
        let options = &p["messages"][0]["poll"]["options"];
        assert_eq!(
            (options[0]["votes"].as_i64(), options[0]["me"].as_bool()),
            (Some(0), Some(false))
        );
        assert_eq!(
            (options[1]["votes"].as_i64(), options[1]["me"].as_bool()),
            (Some(1), Some(true))
        );
        let (_, v) = c
            .api(
                "alice",
                "GET",
                &format!("/messages/{id}/poll/votes"),
                Value::Null,
            )
            .await;
        assert_eq!(v, json!({ b.clone(): [{ "id": c.bob, "pseudo": "bob" }] }));
        for (corps, statut) in [
            (json!({ "option_id": "abc" }), 400),
            (json!({ "option_id": "99999999" }), 400),
            (json!({}), 400),
            (json!({ "option_id": 3 }), 400),
        ] {
            let (s, v) = c.api("bob", "PUT", &vote, corps.clone()).await;
            assert_eq!(
                (s, v["error"].as_str().map(str::len).is_some()),
                (statut, true),
                "{corps}"
            );
        }
        assert_eq!(c.api("bob", "PUT", &vote, Value::Null).await.0, 400);
        let simple = c.poster("alice", "pas un sondage").await;
        let (s, v) = c
            .api(
                "bob",
                "PUT",
                &format!("/messages/{}/vote", simple["id"].as_str().unwrap()),
                json!({ "option_id": a }),
            )
            .await;
        assert_eq!((s, v["error"].as_str()), (400, Some("invalid_option")));
        assert_eq!(
            c.api("bob", "DELETE", &vote, json!({ "option_id": b }))
                .await
                .0,
            204
        );
        assert_eq!(
            c.api("bob", "PUT", &vote, json!({ "option_id": a }))
                .await
                .0,
            204
        );
        assert_eq!(c.api("bob", "DELETE", &vote, Value::Null).await.0, 204);
        let (_, v) = c
            .api(
                "alice",
                "GET",
                &format!("/messages/{id}/poll/votes"),
                Value::Null,
            )
            .await;
        assert_eq!(v, json!({}));
        let (s, v) = c
            .api("bob", "DELETE", &vote, json!({ "option_id": "abc" }))
            .await;
        assert_eq!((s, v["error"].as_str()), (400, Some("invalid_option")));

        let (_, multiple) = lancer(&c, json!({ "content": "Plusieurs ?", "poll": { "options": ["x", "y"], "multiple": true } })).await;
        let (mid, x, y) = (
            multiple["id"].as_str().unwrap(),
            multiple["poll"]["options"][0]["id"].as_str().unwrap(),
            multiple["poll"]["options"][1]["id"].as_str().unwrap(),
        );
        for o in [x, y] {
            assert_eq!(
                c.api(
                    "bob",
                    "PUT",
                    &format!("/messages/{mid}/vote"),
                    json!({ "option_id": o })
                )
                .await
                .0,
                204
            );
        }
        let (_, v) = c
            .api(
                "alice",
                "GET",
                &format!("/messages/{mid}/poll/votes"),
                Value::Null,
            )
            .await;
        assert_eq!(
            (
                v[x].as_array().unwrap().len(),
                v[y].as_array().unwrap().len()
            ),
            (1, 1)
        );

        let cloture = format!("/messages/{id}/poll/close");
        let (s, v) = c.api("bob", "POST", &cloture, Value::Null).await;
        assert_eq!((s, v["error"].as_str()), (403, Some("forbidden")));
        assert_eq!(
            c.api(
                "bob",
                "POST",
                &format!("/messages/{}/poll/close", simple["id"].as_str().unwrap()),
                Value::Null
            )
            .await
            .0,
            403
        );
        assert_eq!(
            c.api(
                "alice",
                "POST",
                &format!("/messages/{}/poll/close", simple["id"].as_str().unwrap()),
                Value::Null
            )
            .await
            .0,
            404
        );
        recus(&mut rx_bob);
        assert_eq!(c.api("alice", "POST", &cloture, Value::Null).await.0, 204);
        let evenements = recus(&mut rx_bob);
        assert_eq!(evenements.len(), 1);
        assert_eq!(
            (evenements[0].0.as_str(), &evenements[0].1["message_id"]),
            ("poll.closed", &json!(id))
        );
        assert!(
            evenements[0].1["closes_at"]
                .as_str()
                .unwrap()
                .ends_with('Z')
        );
        let (s, v) = c.api("bob", "PUT", &vote, json!({ "option_id": a })).await;
        assert_eq!((s, v["error"].as_str()), (409, Some("poll_closed")));
        assert_eq!(c.api("bob", "DELETE", &vote, Value::Null).await.0, 409);
        assert_eq!(c.api("admin", "POST", &cloture, Value::Null).await.0, 204);
        assert_eq!(recus(&mut rx_bob).len(), 1);

        let cloture_multiple = format!("/messages/{mid}/poll/close");
        c.role("Modo", &["manage_messages"], &[("bob", &c.bob)])
            .await;
        assert_eq!(
            c.api("bob", "POST", &cloture_multiple, Value::Null).await.0,
            204
        );

        let resultat: i64 = sqlx::query_scalar(
            "INSERT INTO messages (channel_id, author_id, content, reply_to, system) VALUES ($1::uuid,$2::uuid,'résultat',$3::bigint,true) RETURNING id",
        )
        .bind(&c.salon)
        .bind(crate::messages::SYSTEME)
        .bind(id)
        .fetch_one(c.pool())
        .await
        .unwrap();
        recus(&mut rx_bob);
        let (_, p) = lancer(
            &c,
            json!({ "content": "autre ?", "poll": { "options": ["a", "b"] } }),
        )
        .await;
        let autre = format!("/messages/{}", p["id"].as_str().unwrap());
        c.sql(&format!(
            "DELETE FROM member_roles WHERE user_id='{}'::uuid",
            c.bob
        ))
        .await;
        assert_eq!(c.api("bob", "DELETE", &autre, Value::Null).await.0, 403);
        recus(&mut rx_bob);
        assert_eq!(
            c.api("alice", "DELETE", &format!("/messages/{id}"), Value::Null)
                .await
                .0,
            204
        );
        let supprimes: Vec<String> = recus(&mut rx_bob)
            .into_iter()
            .filter(|(t, _)| t == "message.deleted")
            .map(|(_, d)| d["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(supprimes, [resultat.to_string(), id.to_string()]);
        assert_eq!(c.api("admin", "DELETE", &autre, Value::Null).await.0, 204);
    }
}
