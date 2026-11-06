use axum::{
    body::Body,
    extract::{FromRequestParts, Query, State},
    http::{StatusCode, request::Parts},
    response::Response,
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::Auth,
    db,
    fichiers::{TAILLE_MAX, est_admin},
    http::{champ_texte, decode, write_err, write_json},
    identity::Identity,
    messages::{self, SYSTEME},
    notes_version, openapi,
    servers::{
        MAX_PARTICIPANTS_VOCAL, Params, bd, horodatage, mauvais, membres_ids, non_trouve, param,
        pool, rfc3339, sans_contenu, uuid_valide,
    },
    state::AppState,
};

const MAX_ECRAN_FPS: i64 = 60;
const MAX_ECRAN_HAUTEUR: i64 = 2160;

pub struct Admin(pub Identity);

impl FromRequestParts<AppState> for Admin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        let Auth(idn) = Auth::from_request_parts(parts, state).await?;
        if !est_admin(state, &idn.sub).await {
            return Err(write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "réservé aux administrateurs",
            ));
        }
        Ok(Admin(idn))
    }
}

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/admin/live", get(live))
        .route("/admin/overview", get(apercu))
        .route("/admin/users", get(utilisateurs))
        .route(
            "/admin/users/{id}/suspension",
            axum::routing::put(suspendre).delete(retablir),
        )
        .route("/admin/servers/{id}", delete(supprimer_serveur))
        .route("/admin/stats", get(statistiques))
        .route("/admin/log", get(journal))
        .route("/admin/errors", get(erreurs))
        .route("/admin/bot", get(bot))
        .route("/admin/bot/messages", post(message_du_bot));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("administration", resume, reponses, chemin);
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
        HttpMethod::Get,
        "/admin/live",
        "Connexions et utilisateurs en ligne (une ligne par connexion), salons vocaux actifs avec leurs participants",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/overview",
        "Version du backend, serveurs avec leurs effectifs et configuration non secrète",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/users",
        "Recherche de comptes par pseudo (`q`), 50 au plus, compte système exclu",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Put,
        "/admin/users/{id}/suspension",
        "Suspendre un compte ; sa connexion temps réel est coupée. Un administrateur ne peut pas être suspendu (409 `admin_account`)",
        &[
            ("204", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
            ("404", "Compte introuvable"),
            ("409", "`admin_account`"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/admin/users/{id}/suspension",
        "Rétablir un compte suspendu",
        &[
            ("204", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
            ("404", "Compte introuvable"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/admin/servers/{id}",
        "Supprimer un serveur ; ses membres reçoivent `server.deleted`",
        &[
            ("204", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
            ("404", "Serveur introuvable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/stats",
        "Compteurs : comptes, serveurs, messages, fichiers, stockage, erreurs des dernières 24 h",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/log",
        "Journal d'administration, 100 entrées au plus, les plus récentes d'abord",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/errors",
        "Réponses 5xx de l'API, 100 au plus, les plus récentes d'abord",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/bot",
        "Compte « Estompe » et versions dont la note a déjà été envoyée",
        &[
            ("200", "ok"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
        ],
    ),
    (
        HttpMethod::Post,
        "/admin/bot/messages",
        "Envoyer un message privé du compte « Estompe » (`content`, `to` = pseudo ou vide pour tous les comptes)",
        &[
            ("200", "ok"),
            ("400", "`invalid_message`"),
            ("401", "Non authentifié"),
            ("403", "Réservé aux administrateurs"),
            ("404", "Destinataire introuvable"),
        ],
    ),
];

pub async fn journaliser(pool: &PgPool, sub: &str, action: &str, cible: &str) {
    let acteur: Option<String> =
        sqlx::query_scalar("SELECT pseudo FROM users WHERE keycloak_sub=$1")
            .bind(sub)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let r = sqlx::query("INSERT INTO admin_log (actor, action, target) VALUES ($1,$2,$3)")
        .bind(acteur.as_deref().unwrap_or(sub))
        .bind(action)
        .bind(cible)
        .execute(pool)
        .await;
    if let Err(e) = r {
        tracing::warn!("journal d'administration : {e}");
    }
}

async fn live(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let clients = state.hub().clients();
    let ids: Vec<String> = clients.iter().map(|c| c.user_id.clone()).collect();
    let noms: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, pseudo FROM users WHERE id = ANY($1::uuid[])",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .collect();
    let mut en_ligne: Vec<Value> = clients
        .iter()
        .map(|c| {
            json!({
                "pseudo": noms.get(&c.user_id).cloned().unwrap_or_default(),
                "platform": c.platform,
                "version": c.version,
            })
        })
        .collect();
    en_ligne.sort_by(|a, b| a["pseudo"].as_str().cmp(&b["pseudo"].as_str()));
    let mut vocal = Vec::new();
    let salons = sqlx::query_as::<_, (String, String, String)>(
        "SELECT c.id::text, c.name, sv.name FROM channels c JOIN servers sv ON sv.id = c.server_id WHERE c.kind = ANY($1) ORDER BY sv.name, c.name",
    )
    .bind(&crate::servers::TYPES_VOCAUX[..])
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    for (id, nom, serveur) in salons {
        match state.participants_vocal(&id).await {
            Some(Ok(p)) if !p.is_empty() => {
                vocal.push(json!({ "server": serveur, "channel": nom, "participants": p }));
            }
            Some(_) => {}
            None => break,
        }
    }
    Ok(write_json(
        StatusCode::OK,
        &json!({ "connections": clients.len(), "online": en_ligne, "voice": vocal }),
    ))
}

fn version() -> String {
    option_env!("ESTOMPE_REVISION")
        .filter(|v| !v.is_empty())
        .map_or_else(|| "dev".to_string(), |v| v.chars().take(7).collect())
}

async fn apercu(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let derniere = horodatage("max(x.created_at)");
    let requete = format!(
        "SELECT sv.id::text, sv.name,
           (SELECT count(*) FROM server_members m WHERE m.server_id = sv.id),
           (SELECT count(*) FROM channels c WHERE c.server_id = sv.id),
           (SELECT count(*) FROM messages x JOIN channels c ON c.id = x.channel_id WHERE c.server_id = sv.id),
           (SELECT {derniere} FROM messages x JOIN channels c ON c.id = x.channel_id WHERE c.server_id = sv.id)
         FROM servers sv ORDER BY (SELECT max(x.created_at) FROM messages x JOIN channels c ON c.id = x.channel_id WHERE c.server_id = sv.id) DESC NULLS LAST, sv.name"
    );
    let serveurs: Vec<Value> =
        sqlx::query_as::<_, (String, String, i64, i64, i64, Option<String>)>(sqlx::AssertSqlSafe(
            requete,
        ))
        .fetch_all(pool)
        .await
        .map_err(bd)?
        .into_iter()
        .map(|(id, name, members, channels, messages, dernier)| {
            json!({
                "id": id,
                "name": name,
                "members": members,
                "channels": channels,
                "messages": messages,
                "last_message_at": dernier.map(|d| rfc3339(&d)),
            })
        })
        .collect();
    let c = state.config();
    let origines = if c.allowed_origins.is_empty() {
        Value::Null
    } else {
        json!(c.allowed_origins)
    };
    let positif = |v: i64, defaut: i64| if v <= 0 { defaut } else { v };
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "version": version(),
            "servers": serveurs,
            "config": {
                "settings": {
                    "push_web": !c.vapid_public.is_empty() && !c.vapid_private.is_empty(),
                    "push_android": !c.fcm_service_account.is_empty(),
                    "giphy": !c.giphy_key.is_empty(),
                    "metrics": !c.metrics_token.is_empty(),
                    "allow_video": c.livekit.allow_video,
                    "livekit_url": c.livekit.url,
                    "max_storage": c.max_storage_bytes,
                    "origins": origines,
                },
                "max_file_size": TAILLE_MAX,
                "max_participants": MAX_PARTICIPANTS_VOCAL,
                "screen_max_fps": positif(c.livekit.max_screen_fps, MAX_ECRAN_FPS),
                "screen_max_height": positif(c.livekit.max_screen_height, MAX_ECRAN_HAUTEUR),
            },
        }),
    ))
}

async fn utilisateurs(
    State(state): State<AppState>,
    _: Admin,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let requete = format!(
        "SELECT u.id::text, u.pseudo, {}, u.suspended_at IS NOT NULL,
           (SELECT count(*) FROM server_members m WHERE m.user_id = u.id),
           (SELECT count(*) FROM messages x WHERE x.author_id = u.id)
         FROM users u WHERE u.id <> $1::uuid AND position(lower($2) in lower(u.pseudo)) > 0
         ORDER BY u.pseudo LIMIT 50",
        horodatage("u.created_at")
    );
    let lignes =
        sqlx::query_as::<_, (String, String, String, bool, i64, i64)>(sqlx::AssertSqlSafe(requete))
            .bind(SYSTEME)
            .bind(q.get("q").map_or("", String::as_str))
            .fetch_all(pool)
            .await
            .map_err(bd)?;
    let l: Vec<Value> = lignes
        .into_iter()
        .map(|(id, pseudo, cree, suspendu, serveurs, messages)| {
            json!({
                "id": id,
                "pseudo": pseudo,
                "created_at": rfc3339(&cree),
                "suspended": suspendu,
                "servers": serveurs,
                "messages": messages,
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn suspendre(state: State<AppState>, admin: Admin, p: Params) -> Result<Response, Response> {
    suspension(state.0, admin, p, true).await
}

async fn retablir(state: State<AppState>, admin: Admin, p: Params) -> Result<Response, Response> {
    suspension(state.0, admin, p, false).await
}

fn introuvable() -> Response {
    non_trouve("introuvable")
}

async fn suspension(
    state: AppState,
    Admin(idn): Admin,
    p: Params,
    suspendre: bool,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = param(&p.0, "id");
    if !uuid_valide(id) {
        return Ok(introuvable());
    }
    if suspendre {
        let sub: Option<String> =
            sqlx::query_scalar("SELECT keycloak_sub FROM users WHERE id=$1::uuid")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(bd)?;
        if let Some(sub) = sub
            && est_admin(&state, &sub).await
        {
            return Ok(write_err(
                StatusCode::CONFLICT,
                "admin_account",
                "un administrateur ne peut pas être suspendu",
            ));
        }
    }
    let pseudo: Option<String> = sqlx::query_scalar(
        "UPDATE users SET suspended_at = CASE WHEN $2 THEN coalesce(suspended_at, now()) END
         WHERE id=$1::uuid AND id <> $3::uuid RETURNING pseudo",
    )
    .bind(id)
    .bind(suspendre)
    .bind(SYSTEME)
    .fetch_optional(pool)
    .await
    .map_err(bd)?;
    let Some(pseudo) = pseudo else {
        return Ok(introuvable());
    };
    let action = if suspendre {
        state.hub().kick(id);
        "utilisateur suspendu"
    } else {
        "utilisateur rétabli"
    };
    journaliser(pool, &idn.sub, action, &pseudo).await;
    Ok(sans_contenu())
}

async fn supprimer_serveur(
    State(state): State<AppState>,
    Admin(idn): Admin,
    p: Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let id = param(&p.0, "id");
    if !uuid_valide(id) {
        return Ok(introuvable());
    }
    let nom: Option<String> = sqlx::query_scalar("SELECT name FROM servers WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(bd)?;
    let Some(nom) = nom else {
        return Ok(introuvable());
    };
    let a = membres_ids(pool, id).await.unwrap_or_default();
    sqlx::query("DELETE FROM servers WHERE id=$1::uuid")
        .bind(id)
        .execute(pool)
        .await
        .map_err(bd)?;
    if !a.is_empty() {
        state.publier(&a, "server.deleted", &json!({ "id": id }));
    }
    journaliser(pool, &idn.sub, "serveur supprimé", &nom).await;
    Ok(sans_contenu())
}

async fn statistiques(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let s: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM users WHERE id <> $1::uuid),
           (SELECT count(*) FROM users WHERE id <> $1::uuid AND created_at > now() - interval '7 days'),
           (SELECT count(*) FROM users WHERE suspended_at IS NOT NULL),
           (SELECT count(*) FROM servers),
           (SELECT count(*) FROM messages),
           (SELECT count(*) FROM messages WHERE created_at > now() - interval '24 hours'),
           (SELECT count(*) FROM files WHERE purged_at IS NULL),
           (SELECT count(*) FROM server_errors WHERE at > now() - interval '24 hours')",
    )
    .bind(SYSTEME)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    let octets = db::octets_stockes(pool).await.map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "users": s.0,
            "new_users_7d": s.1,
            "suspended": s.2,
            "servers": s.3,
            "messages": s.4,
            "messages_24h": s.5,
            "files": s.6,
            "storage_bytes": octets,
            "errors_24h": s.7,
        }),
    ))
}

async fn journal(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let requete = format!(
        "SELECT {}, actor, action, target FROM admin_log ORDER BY id DESC LIMIT 100",
        horodatage("at")
    );
    let l: Vec<Value> = sqlx::query_as::<_, (String, String, String, String)>(sqlx::AssertSqlSafe(requete))
        .fetch_all(pool)
        .await
        .map_err(bd)?
        .into_iter()
        .map(|(at, actor, action, target)| {
            json!({ "at": rfc3339(&at), "actor": actor, "action": action, "target": target })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn erreurs(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let requete = format!(
        "SELECT {}, method, path, status FROM server_errors ORDER BY id DESC LIMIT 100",
        horodatage("at")
    );
    let l: Vec<Value> = sqlx::query_as::<_, (String, String, String, i32)>(sqlx::AssertSqlSafe(requete))
        .fetch_all(pool)
        .await
        .map_err(bd)?
        .into_iter()
        .map(|(at, method, path, status)| {
            json!({ "at": rfc3339(&at), "method": method, "path": path, "status": status })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn bot(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let versions = notes_version::envoyees(pool).await.map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "pseudo": "Estompe", "releases": versions }),
    ))
}

pub async fn ecrire_bot(
    state: &AppState,
    pool: &PgPool,
    destinataire: &str,
    contenu: &str,
) -> Result<(), sqlx::Error> {
    let dm: String = sqlx::query_scalar(
        "INSERT INTO dm_conversations (user_a, user_b)
         VALUES (LEAST($1::uuid, $2::uuid), GREATEST($1::uuid, $2::uuid))
         ON CONFLICT (user_a, user_b) DO UPDATE SET user_a = EXCLUDED.user_a RETURNING id::text",
    )
    .bind(SYSTEME)
    .bind(destinataire)
    .fetch_one(pool)
    .await?;
    messages::envoyer_systeme(state, pool, &dm, contenu).await
}

async fn message_du_bot(
    State(state): State<AppState>,
    Admin(idn): Admin,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let e = decode(body).await?;
    let contenu = champ_texte(&e, "content").ok_or_else(mauvais)?;
    let a = champ_texte(&e, "to").ok_or_else(mauvais)?;
    let Some(contenu) = messages::contenu_valide(&contenu, false) else {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_message",
            "message : 1 à 4000 caractères",
        ));
    };
    let ids: Vec<String> = if a.is_empty() {
        sqlx::query_scalar("SELECT id::text FROM users WHERE id <> $1::uuid")
            .bind(SYSTEME)
            .fetch_all(pool)
            .await
            .map_err(bd)?
    } else {
        let id: Option<String> =
            sqlx::query_scalar("SELECT id::text FROM users WHERE lower(pseudo)=lower($1)")
                .bind(&a)
                .fetch_optional(pool)
                .await
                .map_err(bd)?;
        match id {
            Some(id) if id != SYSTEME => vec![id],
            _ => return Ok(introuvable()),
        }
    };
    let echec = |n: usize, e: sqlx::Error| {
        tracing::warn!("message du bot : échec après {n} envois : {e}");
        bd(e)
    };
    for (n, id) in ids.iter().enumerate() {
        ecrire_bot(&state, pool, id, &contenu)
            .await
            .map_err(|e| echec(n, e))?;
    }
    let cible = if a.is_empty() { "tous les comptes" } else { &a };
    journaliser(pool, &idn.sub, "message du bot", cible).await;
    Ok(write_json(StatusCode::OK, &json!({ "sent": ids.len() })))
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, sync::Arc};

    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        identity::Cred,
        servers::tests::{Env, appel, env_avec, membre, serveur},
        state::Vocal,
        testutil::Requete,
    };

    fn droits(r: &Requete) -> (u16, Vec<u8>) {
        let v: Value = serde_json::from_slice(&r.corps).unwrap_or_default();
        let ok = v["permission"] != "estompe:admin" || v["user_id"] == "sub-root";
        (200, json!({ "allowed": ok }).to_string().into_bytes())
    }

    fn refus(_: &Requete) -> (u16, Vec<u8>) {
        (500, Vec::new())
    }

    async fn env() -> Option<Env> {
        let e = env_avec(droits).await?;
        membre(&e, "root").await;
        Some(e)
    }

    async fn admin(
        e: &Env,
        methode: &str,
        chemin: &str,
        corps: Option<&str>,
    ) -> (StatusCode, Value) {
        appel(e, "root", methode, chemin, corps).await
    }

    async fn sans_identite(e: &Env, methode: &str, chemin: &str) -> StatusCode {
        let r = Request::builder()
            .method(methode)
            .uri(chemin)
            .body(Body::empty())
            .unwrap();
        e.app.clone().oneshot(r).await.unwrap().status()
    }

    async fn sql(e: &Env, requete: &str) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(requete))
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
    }

    fn ecouter(e: &Env, id: &str) -> (Arc<crate::hub::Conn>, tokio::sync::mpsc::Receiver<String>) {
        let (c, rx, _) = e.state.hub().register(
            id,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        (c, rx)
    }

    fn recus(rx: &mut tokio::sync::mpsc::Receiver<String>) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        while let Ok(m) = rx.try_recv() {
            let v: Value = serde_json::from_str(&m).unwrap();
            out.push((v["type"].as_str().unwrap().to_string(), v["data"].clone()));
        }
        out
    }

    const ROUTES: [(&str, &str); 11] = [
        ("GET", "/admin/live"),
        ("GET", "/admin/overview"),
        ("GET", "/admin/users"),
        (
            "PUT",
            "/admin/users/00000000-0000-0000-0000-0000000000aa/suspension",
        ),
        (
            "DELETE",
            "/admin/users/00000000-0000-0000-0000-0000000000aa/suspension",
        ),
        (
            "DELETE",
            "/admin/servers/00000000-0000-0000-0000-0000000000aa",
        ),
        ("GET", "/admin/stats"),
        ("GET", "/admin/log"),
        ("GET", "/admin/errors"),
        ("GET", "/admin/bot"),
        ("POST", "/admin/bot/messages"),
    ];

    #[test]
    fn routes_testees_documentees() {
        let mut testees: Vec<String> = ROUTES.iter().map(|(m, c)| format!("{m} {c}")).collect();
        let mut documentees: Vec<String> = DOC
            .iter()
            .map(|(m, c, _, _)| {
                let c = c.replace("{id}", "00000000-0000-0000-0000-0000000000aa");
                format!(
                    "{} {c}",
                    match m {
                        HttpMethod::Get => "GET",
                        HttpMethod::Post => "POST",
                        HttpMethod::Put => "PUT",
                        HttpMethod::Delete => "DELETE",
                        _ => "AUTRE",
                    }
                )
            })
            .collect();
        testees.sort();
        documentees.sort();
        assert_eq!(testees, documentees);
    }

    #[tokio::test]
    async fn droits_sur_toutes_les_routes() {
        let Some(e) = env().await else { return };
        membre(&e, "simple").await;
        for (m, c) in ROUTES {
            assert_eq!(
                sans_identite(&e, m, c).await,
                StatusCode::UNAUTHORIZED,
                "{m} {c}"
            );
            let (s, v) = appel(&e, "simple", m, c, None).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{m} {c}");
            assert_eq!(v["error"], "forbidden");
            assert_eq!(v["message"], "réservé aux administrateurs");
            let (s, _) = admin(&e, m, c, Some("{}")).await;
            assert_ne!(s, StatusCode::FORBIDDEN, "{m} {c}");
            assert_ne!(s, StatusCode::UNAUTHORIZED, "{m} {c}");
        }
    }

    #[tokio::test]
    async fn ferme_quand_le_verificateur_refuse() {
        let Some(e) = env_avec(refus).await else {
            return;
        };
        for (m, c) in ROUTES {
            let (s, v) = appel(&e, "root", m, c, None).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{m} {c}");
            assert_eq!(v["error"], "forbidden");
        }
    }

    #[tokio::test]
    async fn statistiques_sur_des_donnees_connues() {
        let Some(e) = env().await else { return };
        let (_, avant) = admin(&e, "GET", "/admin/stats", None).await;
        assert_eq!(
            avant
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "users",
                "new_users_7d",
                "suspended",
                "servers",
                "messages",
                "messages_24h",
                "files",
                "storage_bytes",
                "errors_24h"
            ]
            .iter()
            .map(|s| s.to_string())
            .collect()
        );
        let alice = membre(&e, "alice").await;
        let vieux = membre(&e, "vieux").await;
        sql(
            &e,
            &format!("UPDATE users SET created_at = now() - interval '10 days', suspended_at = now() WHERE id = '{vieux}'"),
        )
        .await;
        let sid = serveur(&e, "S", "alice").await;
        let salon: String = sqlx::query_scalar(
            "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind='text' LIMIT 1",
        )
        .bind(&sid)
        .fetch_one(e.state.pool().ok().unwrap())
        .await
        .unwrap();
        sql(
            &e,
            &format!(
                "INSERT INTO messages (channel_id, author_id, content) VALUES ('{salon}','{alice}','a');
                 INSERT INTO messages (channel_id, author_id, content, created_at) VALUES ('{salon}','{alice}','b', now() - interval '3 days');
                 INSERT INTO files (uploader_id, name, mime, size, key) VALUES ('{alice}','a','text/plain',1000,'k1');
                 INSERT INTO files (uploader_id, name, mime, size, key, purged_at) VALUES ('{alice}','b','text/plain',500,'k2', now());
                 INSERT INTO server_errors (method, path, status) VALUES ('GET','/x',500);
                 INSERT INTO server_errors (at, method, path, status) VALUES (now() - interval '2 days','GET','/y',500);"
            ),
        )
        .await;
        let (s, apres) = admin(&e, "GET", "/admin/stats", None).await;
        assert_eq!(s, StatusCode::OK);
        let delta = |k: &str| apres[k].as_i64().unwrap() - avant[k].as_i64().unwrap();
        assert_eq!(delta("users"), 2);
        assert_eq!(delta("new_users_7d"), 1);
        assert_eq!(delta("suspended"), 1);
        assert_eq!(delta("servers"), 1);
        assert_eq!(delta("messages") - delta("messages_24h"), 1);
        assert_eq!(delta("files"), 1);
        assert_eq!(delta("storage_bytes"), 1000);
        assert_eq!(delta("errors_24h"), 1);
    }

    #[tokio::test]
    async fn utilisateurs_journal_et_erreurs() {
        let Some(e) = env().await else { return };
        for i in 0..52 {
            membre(&e, &format!("zz{i:02}")).await;
        }
        let (s, l) = admin(&e, "GET", "/admin/users", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(l.as_array().unwrap().len(), 50);
        assert!(
            l.as_array()
                .unwrap()
                .iter()
                .all(|u| u["pseudo"] != "Estompe")
        );
        let (_, l) = admin(&e, "GET", "/admin/users?q=ZZ5", None).await;
        let noms: Vec<_> = l
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["pseudo"].as_str().unwrap())
            .collect();
        assert_eq!(noms, ["zz50", "zz51"]);
        let u = &l[0];
        assert_eq!(u["suspended"], false);
        assert_eq!(u["servers"], 0);
        assert_eq!(u["messages"], 0);
        assert!(u["created_at"].as_str().unwrap().ends_with('Z'));
        let (_, l) = admin(&e, "GET", "/admin/users?q=%C3%A9inconnu", None).await;
        assert_eq!(l, json!([]));

        for i in 0..102 {
            sql(
                &e,
                &format!("INSERT INTO admin_log (actor, action, target) VALUES ('a','act','c{i:03}');
                          INSERT INTO server_errors (method, path, status) VALUES ('GET','/p{i:03}',503)"),
            )
            .await;
        }
        let (_, l) = admin(&e, "GET", "/admin/log", None).await;
        assert_eq!(l.as_array().unwrap().len(), 100);
        assert_eq!(l[0]["target"], "c101");
        assert_eq!(l[0]["actor"], "a");
        assert_eq!(l[0]["action"], "act");
        let (_, l) = admin(&e, "GET", "/admin/errors", None).await;
        assert_eq!(l.as_array().unwrap().len(), 100);
        assert_eq!(l[0]["path"], "/p101");
        assert_eq!(l[0]["status"], 503);
        assert_eq!(l[0]["method"], "GET");
        assert!(l[0]["at"].as_str().unwrap().ends_with('Z'));
    }

    #[tokio::test]
    async fn suspension_et_retablissement() {
        let Some(e) = env().await else { return };
        let victime = membre(&e, "victime").await;
        let lien = |id: &str| format!("/admin/users/{id}/suspension");
        let (conn, _rx) = ecouter(&e, &victime);

        let (s, me) = appel(&e, "victime", "GET", "/me", None).await;
        assert_eq!((s, &me["suspended"]), (StatusCode::OK, &json!(false)));

        let (s, _) = admin(&e, "PUT", &lien(&victime), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert!(conn.is_closed());
        let (_, me) = appel(&e, "victime", "GET", "/me", None).await;
        assert_eq!(me["suspended"], true);
        let (s, v) = appel(&e, "victime", "GET", "/servers", None).await;
        assert_eq!(
            (s, &v["error"]),
            (StatusCode::FORBIDDEN, &json!("suspended"))
        );

        let (s, _) = admin(&e, "DELETE", &lien(&victime), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, me) = appel(&e, "victime", "GET", "/me", None).await;
        assert_eq!(me["suspended"], false);

        let (_, journal) = admin(&e, "GET", "/admin/log", None).await;
        assert_eq!(journal[0]["action"], "utilisateur rétabli");
        assert_eq!(journal[0]["target"], "victime");
        assert_eq!(journal[0]["actor"], "root");
        assert_eq!(journal[1]["action"], "utilisateur suspendu");
        assert_eq!(journal[1]["target"], "victime");
    }

    #[tokio::test]
    async fn suspension_refusee_ou_introuvable() {
        let Some(e) = env().await else { return };
        let racine: String = sqlx::query_scalar("SELECT id::text FROM users WHERE pseudo='root'")
            .fetch_one(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        let lien = |id: &str| format!("/admin/users/{id}/suspension");
        let (s, v) = admin(&e, "PUT", &lien(&racine), None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["error"], "admin_account");
        for id in [
            "pas-un-uuid",
            "00000000-0000-0000-0000-0000000000aa",
            SYSTEME,
        ] {
            for m in ["PUT", "DELETE"] {
                let (s, v) = admin(&e, m, &lien(id), None).await;
                assert_eq!(s, StatusCode::NOT_FOUND, "{m} {id}");
                assert_eq!(v["error"], "not_found");
            }
        }
        let (_, l) = admin(&e, "GET", "/admin/log", None).await;
        assert!(l.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn suppression_de_serveur() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let tiers = membre(&e, "tiers").await;
        let sid = serveur(&e, "À supprimer", "alice").await;
        let (_a, mut rx_alice) = ecouter(&e, &alice);
        let (_t, mut rx_tiers) = ecouter(&e, &tiers);
        let (s, _) = admin(&e, "DELETE", &format!("/admin/servers/{sid}"), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(
            recus(&mut rx_alice),
            [("server.deleted".to_string(), json!({ "id": sid }))]
        );
        assert!(recus(&mut rx_tiers).is_empty());
        let (_, l) = appel(&e, "alice", "GET", "/servers", None).await;
        assert_eq!(l, json!([]));
        let (_, journal) = admin(&e, "GET", "/admin/log", None).await;
        assert_eq!(journal[0]["action"], "serveur supprimé");
        assert_eq!(journal[0]["target"], "À supprimer");
        for id in [sid.as_str(), "pas-un-uuid"] {
            let (s, _) = admin(&e, "DELETE", &format!("/admin/servers/{id}"), None).await;
            assert_eq!(s, StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn messages_du_bot() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        membre(&e, "bob").await;
        let (_a, mut rx_alice) = ecouter(&e, &alice);
        let corps = |c: &str, a: &str| json!({ "content": c, "to": a }).to_string();

        let (s, v) = admin(
            &e,
            "POST",
            "/admin/bot/messages",
            Some(&corps("  Salut  ", "ALICE")),
        )
        .await;
        assert_eq!((s, v), (StatusCode::OK, json!({ "sent": 1 })));
        let recu = recus(&mut rx_alice);
        assert_eq!(recu.len(), 1);
        assert_eq!(recu[0].0, "message.created");
        assert_eq!(recu[0].1["content"], "Salut");
        assert_eq!(recu[0].1["author"]["pseudo"], "Estompe");
        let (_, dms) = appel(&e, "alice", "GET", "/dms", None).await;
        let dm = dms
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["other"]["pseudo"] == "Estompe")
            .unwrap();
        let (_, l) = appel(
            &e,
            "alice",
            "GET",
            &format!("/dms/{}/messages", dm["id"].as_str().unwrap()),
            None,
        )
        .await;
        assert_eq!(l["messages"].as_array().unwrap().len(), 1);

        let (s, v) = admin(&e, "POST", "/admin/bot/messages", Some(&corps("Tous", ""))).await;
        assert_eq!((s, v), (StatusCode::OK, json!({ "sent": 3 })));
        let (_, journal) = admin(&e, "GET", "/admin/log", None).await;
        assert_eq!(journal[0]["action"], "message du bot");
        assert_eq!(journal[0]["target"], "tous les comptes");
        assert_eq!(journal[1]["target"], "ALICE");

        for (c, a) in [("", "alice"), ("   ", "alice")] {
            let (s, v) = admin(&e, "POST", "/admin/bot/messages", Some(&corps(c, a))).await;
            assert_eq!(
                (s, v["error"].clone()),
                (StatusCode::BAD_REQUEST, json!("invalid_message"))
            );
        }
        let long = "é".repeat(4001);
        let (s, v) = admin(
            &e,
            "POST",
            "/admin/bot/messages",
            Some(&corps(&long, "alice")),
        )
        .await;
        assert_eq!(
            (s, v["error"].clone()),
            (StatusCode::BAD_REQUEST, json!("invalid_message"))
        );
        let ok = "é".repeat(4000);
        let (s, _) = admin(
            &e,
            "POST",
            "/admin/bot/messages",
            Some(&corps(&ok, "alice")),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        for a in ["inconnu", "estompe"] {
            let (s, v) = admin(&e, "POST", "/admin/bot/messages", Some(&corps("x", a))).await;
            assert_eq!(
                (s, v["error"].clone()),
                (StatusCode::NOT_FOUND, json!("not_found")),
                "{a}"
            );
        }
        let (s, v) = admin(&e, "POST", "/admin/bot/messages", Some("[1]")).await;
        assert_eq!(
            (s, v["error"].clone()),
            (StatusCode::BAD_REQUEST, json!("bad_request"))
        );
        let (s, _) = admin(&e, "POST", "/admin/bot/messages", Some(r#"{"content":3}"#)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, v) = admin(
            &e,
            "POST",
            "/admin/bot/messages",
            Some(r#"{"content":"x"}"#),
        )
        .await;
        assert_eq!((s, v), (StatusCode::OK, json!({ "sent": 3 })));
    }

    #[tokio::test]
    async fn bot_liste_les_versions_envoyees() {
        let Some(e) = env().await else { return };
        let (s, v) = admin(&e, "GET", "/admin/bot", None).await;
        assert_eq!(
            (s, &v["pseudo"], &v["releases"]),
            (StatusCode::OK, &json!("Estompe"), &json!([]))
        );
        for i in 0..32 {
            sql(
                &e,
                &format!("INSERT INTO release_announced (version, sent_at) VALUES ('1.0.{i:02}', now() - make_interval(mins => {}))", 100 - i),
            )
            .await;
        }
        let (_, v) = admin(&e, "GET", "/admin/bot", None).await;
        let l = v["releases"].as_array().unwrap();
        assert_eq!(l.len(), 30);
        assert_eq!(l[0], "1.0.31");
        assert_eq!(l[29], "1.0.02");
    }

    struct Salons(String);

    impl Vocal for Salons {
        fn sortir_du_serveur<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {})
        }

        fn participants<'a>(
            &'a self,
            salon: &'a str,
        ) -> Pin<Box<dyn Future<Output = crate::state::ParticipantsVocal> + Send + 'a>> {
            Box::pin(async move {
                if salon == self.0 {
                    Ok(vec![json!({ "user_id": "u", "pseudo": "p" })])
                } else {
                    Ok(Vec::new())
                }
            })
        }
    }

    #[tokio::test]
    async fn en_direct() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let bob = membre(&e, "bob").await;
        let (_, v) = admin(&e, "GET", "/admin/live", None).await;
        assert_eq!(v, json!({ "connections": 0, "online": [], "voice": [] }));

        let (c1, _r1) = ecouter(&e, &bob);
        let (_c2, _r2) = ecouter(&e, &alice);
        let (_c3, _r3) = ecouter(&e, &alice);
        c1.tag("web", "1.2.3");
        let (_, v) = admin(&e, "GET", "/admin/live", None).await;
        assert_eq!(v["connections"], 3);
        let en_ligne = v["online"].as_array().unwrap();
        let noms: Vec<_> = en_ligne
            .iter()
            .map(|o| o["pseudo"].as_str().unwrap())
            .collect();
        assert_eq!(noms, ["alice", "alice", "bob"]);
        assert_eq!(
            en_ligne[2],
            json!({ "pseudo": "bob", "platform": "web", "version": "1.2.3" })
        );
        assert_eq!(v["voice"], json!([]));

        let sid = serveur(&e, "Vocal", "alice").await;
        let vocaux: Vec<String> = sqlx::query_scalar(
            "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind='voice' ORDER BY name",
        )
        .bind(&sid)
        .fetch_all(e.state.pool().ok().unwrap())
        .await
        .unwrap();
        assert!(!vocaux.is_empty());
        e.state.definir_vocal(Arc::new(Salons(vocaux[0].clone())));
        let (_, v) = admin(&e, "GET", "/admin/live", None).await;
        let nom: String = sqlx::query_scalar("SELECT name FROM channels WHERE id=$1::uuid")
            .bind(&vocaux[0])
            .fetch_one(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        assert_eq!(
            v["voice"],
            json!([{ "server": "Vocal", "channel": nom, "participants": [{ "user_id": "u", "pseudo": "p" }] }])
        );
    }

    #[tokio::test]
    async fn apercu_du_service() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let vide = serveur(&e, "Vide", "alice").await;
        let actif = serveur(&e, "Actif", "alice").await;
        let salon: String = sqlx::query_scalar(
            "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind='text' LIMIT 1",
        )
        .bind(&actif)
        .fetch_one(e.state.pool().ok().unwrap())
        .await
        .unwrap();
        sql(&e, &format!("INSERT INTO messages (channel_id, author_id, content) VALUES ('{salon}','{alice}','a')")).await;
        let (s, v) = admin(&e, "GET", "/admin/overview", None).await;
        assert_eq!(s, StatusCode::OK);
        assert!(!v["version"].as_str().unwrap().is_empty());
        assert!(v["version"].as_str().unwrap().len() <= 7);
        let serveurs = v["servers"].as_array().unwrap();
        assert_eq!(serveurs[0]["id"], actif);
        assert_eq!(serveurs[0]["messages"], 1);
        assert_eq!(serveurs[0]["members"], 1);
        assert!(
            serveurs[0]["last_message_at"]
                .as_str()
                .unwrap()
                .ends_with('Z')
        );
        assert_eq!(serveurs[1]["id"], vide);
        assert_eq!(serveurs[1]["last_message_at"], Value::Null);
        let config = &v["config"];
        assert_eq!(config["max_participants"], 10);
        assert_eq!(config["max_file_size"], 10 << 20);
        assert_eq!(config["screen_max_fps"], 60);
        assert_eq!(config["screen_max_height"], 2160);
        let reglages = &config["settings"];
        for k in [
            "push_web",
            "push_android",
            "giphy",
            "metrics",
            "allow_video",
        ] {
            assert!(reglages[k].is_boolean(), "{k}");
        }
        assert_eq!(reglages["livekit_url"], "wss://livekit.test");
        assert_eq!(reglages["max_storage"], e.state.config().max_storage_bytes);
        assert_eq!(
            reglages["origins"].is_null(),
            e.state.config().allowed_origins.is_empty()
        );
    }
}
