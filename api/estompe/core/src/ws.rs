use std::{collections::HashMap, time::Duration};

use axum::{
    extract::{
        Query, State,
        ws::{
            CloseFrame, Message, WebSocket, WebSocketUpgrade, rejection::WebSocketUpgradeRejection,
        },
    },
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout, timeout_at};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    api::ActiveUser,
    http::{text_error, write_err, write_json},
    hub::{Conn, enveloppe},
    identity::{Erreur, Identity, cred_of},
    state::AppState,
    users,
};

const LECTURE_MAX: usize = 1 << 10;
const ECRITURE_MAX: Duration = Duration::from_secs(10);
const FERMETURE_NORMALE: u16 = 1000;
const TROP_GRAND: u16 = 1009;
const POLITIQUE: u16 = 1008;
const SESSION_EXPIREE: u16 = 4401;
const ACCES_RETIRE: u16 = 4403;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(ws_ticket))
        .routes(routes!(ws))
}

#[derive(Serialize, ToSchema)]
#[allow(dead_code)]
struct TicketReponse {
    ticket: String,
}

#[utoipa::path(post, path = "/ws-ticket", tag = "temps réel",
    summary = "Ticket à usage unique (30 s) pour ouvrir le WebSocket",
    responses((status = 200, description = "ok", body = TicketReponse)))]
async fn ws_ticket(
    State(state): State<AppState>,
    ActiveUser(user): ActiveUser,
    headers: HeaderMap,
) -> Response {
    let Some(cred) = cred_of(&headers) else {
        return text_error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let ticket = state.hub().issue_ticket(&user.id, cred);
    write_json(StatusCode::OK, &json!({ "ticket": ticket }))
}

pub async fn on_presence(state: AppState, user_id: String, online: bool) {
    let Ok(pool) = state.pool() else { return };
    if let Ok(co) = users::co_members(pool, &user_id).await {
        state.publier(
            &co,
            "presence",
            &json!({ "user_id": user_id, "online": online }),
        );
    }
}

fn origine_autorisee(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origine) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(hote) = origine
        .to_str()
        .ok()
        .and_then(|o| o.parse::<Uri>().ok())
        .and_then(|u| u.authority().map(|a| a.as_str().to_string()))
    else {
        return false;
    };
    let propre = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.eq_ignore_ascii_case(&hote));
    propre
        || state.config().allowed_origins.iter().any(|o| {
            o.parse::<Uri>()
                .ok()
                .and_then(|u| {
                    u.authority()
                        .map(|a| a.as_str().eq_ignore_ascii_case(&hote))
                })
                .unwrap_or(false)
        })
}

fn clip(s: &str, n: usize) -> &str {
    let mut fin = n.min(s.len());
    while !s.is_char_boundary(fin) {
        fin -= 1;
    }
    &s[..fin]
}

#[utoipa::path(get, path = "/ws", tag = "temps réel", security(),
    summary = "WebSocket d'évènements (voir docs/realtime.md)",
    params(("ticket" = String, Query, description = "Ticket obtenu par POST /ws-ticket")),
    responses(
        (status = 101, description = "Switching Protocols"),
        (status = 401, description = "bad_ticket", body = crate::openapi::Error),
    ))]
async fn ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    requete: Result<Query<HashMap<String, String>>, axum::extract::rejection::QueryRejection>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let q = requete.map(|Query(q)| q).unwrap_or_default();
    let champ = |nom: &str| q.get(nom).map(String::as_str).unwrap_or_default();
    let Some((user_id, cred)) = state.hub().redeem_ticket(champ("ticket")) else {
        return write_err(
            StatusCode::UNAUTHORIZED,
            "bad_ticket",
            "ticket invalide ou expiré",
        );
    };
    let upgrade = match upgrade {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    if !origine_autorisee(&state, &headers) {
        return text_error(StatusCode::FORBIDDEN, "origine non autorisée");
    }
    let plateforme = clip(champ("platform"), 16).to_string();
    let version = clip(champ("v"), 24).to_string();
    upgrade
        .max_message_size(LECTURE_MAX)
        .max_frame_size(LECTURE_MAX)
        .on_upgrade(move |socket| connexion(state, socket, user_id, cred, plateforme, version))
}

struct Garde {
    state: AppState,
    conn: std::sync::Arc<Conn>,
}

impl Drop for Garde {
    fn drop(&mut self) {
        if self.state.hub().unregister(&self.conn) {
            tokio::spawn(on_presence(
                self.state.clone(),
                self.conn.user_id.clone(),
                false,
            ));
        }
    }
}

fn texte(message: String) -> Message {
    Message::Text(message.into())
}

fn fermeture(code: u16, raison: &str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: raison.into(),
    }))
}

fn est_ping(octets: &[u8]) -> bool {
    serde_json::from_slice::<Value>(octets)
        .ok()
        .is_some_and(|v| v["type"] == "ping")
}

async fn connexion(
    state: AppState,
    socket: WebSocket,
    user_id: String,
    cred: crate::identity::Cred,
    plateforme: String,
    version: String,
) {
    let (conn, mut file, premiere) = state.hub().register(&user_id, cred.clone());
    conn.tag(&plateforme, &version);
    tracing::info!("ws: client {plateforme} {version}");
    if premiere {
        tokio::spawn(on_presence(state.clone(), user_id.clone(), true));
    }
    let _garde = Garde {
        state: state.clone(),
        conn: conn.clone(),
    };

    conn.enqueue(enveloppe("ready", &json!({ "user_id": user_id })));
    if let Ok(pool) = state.pool()
        && let Ok(co) = users::co_members(pool, &user_id).await
    {
        conn.enqueue(enveloppe(
            "presence.snapshot",
            &json!({
                "online": state.hub().online_among(&co),
                "activities": state.hub().activities_among(&co),
            }),
        ));
    }

    let durees = state.durees_ws();
    let (mut envoi, mut reception) = socket.split();
    let mut lecture = tokio::spawn({
        let conn = conn.clone();
        async move {
            let mut echeance = Instant::now() + durees.idle;
            loop {
                let message = match timeout_at(echeance, reception.next()).await {
                    Ok(Some(Ok(message))) => message,
                    Ok(Some(Err(_))) => return (TROP_GRAND, "message trop grand"),
                    _ => return (FERMETURE_NORMALE, ""),
                };
                match message {
                    Message::Text(t) => {
                        if est_ping(t.as_bytes()) {
                            conn.enqueue(enveloppe("pong", &Value::Null));
                        }
                    }
                    Message::Binary(b) => {
                        if est_ping(&b) {
                            conn.enqueue(enveloppe("pong", &Value::Null));
                        }
                    }
                    Message::Close(_) => return (FERMETURE_NORMALE, ""),
                    Message::Ping(_) | Message::Pong(_) => continue,
                }
                echeance = Instant::now() + durees.idle;
            }
        }
    });

    let mut tick = interval_at(Instant::now() + durees.recheck, durees.recheck);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut sub = String::new();
    let mut permission_hs: Option<Instant> = None;
    let fin = loop {
        tokio::select! {
            Some(message) = file.recv() => {
                match timeout(ECRITURE_MAX, envoi.send(texte(message))).await {
                    Ok(Ok(())) => {}
                    _ => break None,
                }
            }
            _ = tick.tick() => {
                match state.identity().revalidate(Some(&cred)).await {
                    Err(Erreur::Invalid) => break Some((SESSION_EXPIREE, "session expirée")),
                    Ok(Identity { sub: s, .. }) => sub = s,
                    Err(Erreur::Unavailable) => {}
                }
                if sub.is_empty() {
                    continue;
                }
                match state.acces(&sub).await {
                    Err(_) => {
                        let debut = *permission_hs.get_or_insert_with(Instant::now);
                        if debut.elapsed() > durees.perm_grace {
                            break Some((ACCES_RETIRE, "service de permission indisponible"));
                        }
                    }
                    Ok(true) => permission_hs = None,
                    Ok(false) => {
                        state.acces_retire(&user_id).await;
                        break Some((ACCES_RETIRE, "accès à Estompe retiré"));
                    }
                }
            }
            _ = conn.closed() => break Some((POLITIQUE, "client trop lent ou serveur arrêté")),
            Ok(fin) = &mut lecture => break Some(fin),
        }
    };
    if let Some((code, raison)) = fin {
        let _ = timeout(ECRITURE_MAX, envoi.send(fermeture(code, raison))).await;
    }
    lecture.abort();
}

#[cfg(test)]
mod tests {
    use super::{clip, router};
    use crate::{
        api,
        middleware::envelopper,
        state::{AppState, DureesWs},
        testutil::{self, BasePostgres, Faux, faux},
        updates::{self, Policy},
    };
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{Value, json};
    use std::{
        collections::HashSet,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
    use tokio_tungstenite::{
        MaybeTlsStream, WebSocketStream, connect_async,
        tungstenite::{Error, Message, client::IntoClientRequest},
    };

    const ORIGINE: &str = "https://app.test";
    const DELAI: Duration = Duration::from_secs(5);

    type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

    struct Env {
        hote: String,
        state: AppState,
        http: reqwest::Client,
        decalage: Arc<Mutex<Duration>>,
        revoques: Arc<Mutex<HashSet<String>>>,
        refuses: Arc<Mutex<HashSet<String>>>,
        permission_hs: Arc<AtomicBool>,
        serveur: JoinHandle<()>,
        _gateway: Faux,
        permission: Faux,
        _pg: BasePostgres,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            self.serveur.abort();
        }
    }

    fn durees_par_defaut() -> DureesWs {
        DureesWs::default()
    }

    async fn env(durees: DureesWs) -> Option<Env> {
        env_avec(durees, updates::parse(b"{}")).await
    }

    async fn env_avec(durees: DureesWs, politique: Policy) -> Option<Env> {
        let revoques: Arc<Mutex<HashSet<String>>> = Arc::default();
        let refuses: Arc<Mutex<HashSet<String>>> = Arc::default();
        let permission_hs = Arc::new(AtomicBool::new(false));
        let gateway = faux({
            let revoques = revoques.clone();
            move |r| {
                let auth = r.entete("authorization");
                let nom = auth.strip_prefix("Bearer ").unwrap_or_default();
                if nom.is_empty() || revoques.lock().unwrap().contains(nom) {
                    return (401, vec![]);
                }
                let corps = json!({ "id": format!("sub-{nom}"), "name": nom, "email": "" });
                (200, corps.to_string().into_bytes())
            }
        })
        .await;
        let permission = faux({
            let refuses = refuses.clone();
            let hs = permission_hs.clone();
            move |r| {
                if hs.load(Ordering::SeqCst) {
                    return (500, vec![]);
                }
                let refuses = refuses.lock().unwrap().clone();
                testutil::reponse_droits(r, |sub, _| !refuses.contains(sub))
            }
        })
        .await;
        let (pg, url) = testutil::postgres().await?;
        let config = testutil::config_avec(&[
            ("GATEWAY_URL", gateway.url.as_str()),
            ("PERMISSION_URL", permission.url.as_str()),
            ("ALLOWED_ORIGINS", ORIGINE),
        ]);
        let debut = Instant::now();
        let decalage: Arc<Mutex<Duration>> = Arc::default();
        let horloge = {
            let decalage = decalage.clone();
            Arc::new(move || debut + *decalage.lock().unwrap())
        };
        let state = AppState::new_avec_ws(config, vec![], politique, durees, horloge);
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        crate::db::migrate_embarquees(&pool).await.unwrap();
        state.set_pool(pool);
        let app = envelopper(api::routes(), state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hote = listener.local_addr().unwrap().to_string();
        let serveur = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        Some(Env {
            hote,
            state,
            http: reqwest::Client::new(),
            decalage,
            revoques,
            refuses,
            permission_hs,
            serveur,
            _gateway: gateway,
            permission,
            _pg: pg,
        })
    }

    impl Env {
        async fn connect_ready(&self, nom: &str) -> Client {
            let mut c = self.connecter(nom).await;
            assert_eq!(suivant(&mut c).await.0, "ready");
            attendre(&mut c, "presence.snapshot").await;
            c
        }

        async fn utilisateur(&self, nom: &str) -> String {
            sqlx::query_scalar(
                "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
            )
            .bind(format!("sub-{nom}"))
            .bind(nom)
            .fetch_one(self.state.pool().ok().unwrap())
            .await
            .unwrap()
        }

        async fn partager(&self, a: &str, b: &str) {
            let pool = self.state.pool().ok().unwrap();
            let serveur: String = sqlx::query_scalar(
                "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
            )
            .bind(a)
            .fetch_one(pool)
            .await
            .unwrap();
            for u in [a, b] {
                sqlx::query(
                    "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid)",
                )
                .bind(&serveur)
                .bind(u)
                .execute(pool)
                .await
                .unwrap();
            }
        }

        fn avancer(&self, d: Duration) {
            *self.decalage.lock().unwrap() += d;
        }

        async fn ticket(&self, nom: &str) -> (u16, String) {
            let r = self
                .http
                .post(format!("http://{}/ws-ticket", self.hote))
                .bearer_auth(nom)
                .send()
                .await
                .unwrap();
            (r.status().as_u16(), r.text().await.unwrap())
        }

        async fn ticket_valide(&self, nom: &str) -> String {
            let (statut, corps) = self.ticket(nom).await;
            assert_eq!(statut, 200, "{corps}");
            let v: Value = serde_json::from_str(&corps).unwrap();
            let spec = router().split_for_parts().1;
            crate::testutil::conforme_openapi(&spec, "POST", "/ws-ticket", 200, &v).unwrap();
            v["ticket"].as_str().unwrap().to_string()
        }

        async fn tenter(
            &self,
            ticket: &str,
            requete: &str,
            origine: Option<&str>,
        ) -> Result<Client, Error> {
            let mut r = format!("ws://{}/ws?ticket={ticket}{requete}", self.hote)
                .into_client_request()
                .unwrap();
            if let Some(o) = origine {
                r.headers_mut().insert("Origin", o.parse().unwrap());
            }
            Ok(connect_async(r).await?.0)
        }

        async fn connecter(&self, nom: &str) -> Client {
            let ticket = self.ticket_valide(nom).await;
            self.tenter(&ticket, "", None).await.unwrap()
        }
    }

    async fn suivant(c: &mut Client) -> (String, Value) {
        loop {
            match timeout(DELAI, c.next()).await.expect("délai de lecture") {
                Some(Ok(Message::Text(t))) => {
                    let v: Value = serde_json::from_str(t.as_str()).unwrap();
                    return (v["type"].as_str().unwrap().to_string(), v["data"].clone());
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                autre => panic!("lecture : {autre:?}"),
            }
        }
    }

    async fn attendre(c: &mut Client, typ: &str) -> Value {
        for _ in 0..20 {
            let (t, data) = suivant(c).await;
            if t == typ {
                return data;
            }
        }
        panic!("évènement {typ:?} jamais reçu");
    }

    async fn fermeture(c: &mut Client) -> Option<(u16, String)> {
        loop {
            match timeout(DELAI, c.next()).await.expect("délai de fermeture") {
                Some(Ok(Message::Close(Some(f)))) => {
                    return Some((u16::from(f.code), f.reason.to_string()));
                }
                Some(Ok(Message::Close(None))) | Some(Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        }
    }

    async fn jusqua(condition: impl Fn() -> bool) {
        let fin = Instant::now() + DELAI;
        while !condition() {
            assert!(Instant::now() < fin, "condition jamais atteinte");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn durees_rapides(grace: Duration) -> DureesWs {
        DureesWs {
            recheck: Duration::from_millis(50),
            perm_grace: grace,
            idle: durees_par_defaut().idle,
        }
    }

    #[test]
    fn clip_borne_en_octets_sans_couper_un_caractere() {
        assert_eq!(clip("abcdef", 3), "abc");
        assert_eq!(clip("abc", 16), "abc");
        assert_eq!(clip("ééé", 3), "é");
        assert_eq!(clip("éé", 4), "éé");
        assert_eq!(clip("", 4), "");
    }

    #[tokio::test]
    async fn ticket_comme_les_routes_protegees_et_usage_unique() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        assert_eq!(e.ticket("").await.0, 401);
        assert_eq!(e.ticket("jamais-active").await.0, 403);
        e.utilisateur("nope").await;
        e.refuses.lock().unwrap().insert("sub-nope".into());
        let (s, c) = e.ticket("nope").await;
        assert_eq!(s, 403);
        assert!(c.contains("\"forbidden\""), "{c}");
        e.utilisateur("boom").await;
        e.permission_hs.store(true, Ordering::SeqCst);
        let (s, c) = e.ticket("boom").await;
        assert_eq!(s, 503);
        assert!(c.contains("\"permission_unavailable\""), "{c}");
        e.permission_hs.store(false, Ordering::SeqCst);

        e.utilisateur("alice").await;
        let faux_ticket = e
            .http
            .get(format!("http://{}/ws?ticket=faux", e.hote))
            .send()
            .await
            .unwrap();
        assert_eq!(faux_ticket.status().as_u16(), 401);
        assert!(faux_ticket.text().await.unwrap().contains("\"bad_ticket\""));

        let ticket = e.ticket_valide("alice").await;
        assert_eq!(ticket.len(), 48);
        let mut c = e.tenter(&ticket, "", None).await.unwrap();
        assert_eq!(suivant(&mut c).await.0, "ready");
        let Err(Error::Http(r)) = e.tenter(&ticket, "", None).await else {
            panic!("ticket réutilisable");
        };
        assert_eq!(r.status().as_u16(), 401);

        let expire = e.ticket_valide("alice").await;
        e.avancer(Duration::from_secs(31));
        let Err(Error::Http(r)) = e.tenter(&expire, "", None).await else {
            panic!("ticket expiré accepté");
        };
        assert_eq!(r.status().as_u16(), 401);
    }

    #[tokio::test]
    async fn controle_d_origine() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        e.utilisateur("alice").await;
        let ticket = e.ticket_valide("alice").await;
        let Err(Error::Http(r)) = e.tenter(&ticket, "", Some("https://evil.example")).await else {
            panic!("origine étrangère acceptée");
        };
        assert_eq!(r.status().as_u16(), 403);
        for origine in [Some(ORIGINE), Some("HTTPS://APP.TEST"), None] {
            let ticket = e.ticket_valide("alice").await;
            let mut c = e
                .tenter(&ticket, "", origine)
                .await
                .unwrap_or_else(|x| panic!("{origine:?} : {x}"));
            assert_eq!(suivant(&mut c).await.0, "ready");
        }
        let ticket = e.ticket_valide("alice").await;
        let propre = format!("http://{}", e.hote);
        let mut c = e.tenter(&ticket, "", Some(&propre)).await.unwrap();
        assert_eq!(suivant(&mut c).await.0, "ready");
    }

    #[tokio::test]
    async fn version_sous_le_minimum_refusee_en_426() {
        let politique = updates::parse(br#"{"minVersion":"0.5.0"}"#);
        let Some(e) = env_avec(durees_par_defaut(), politique).await else {
            return;
        };
        e.utilisateur("alice").await;
        let ticket = e.ticket_valide("alice").await;
        let Err(Error::Http(r)) = e.tenter(&ticket, "&platform=windows&v=0.1.0", None).await else {
            panic!("version trop ancienne acceptée");
        };
        assert_eq!(r.status().as_u16(), 426);
        let mut c = e
            .tenter(&ticket, "&platform=windows&v=0.6.0", None)
            .await
            .unwrap();
        assert_eq!(suivant(&mut c).await.0, "ready");
    }

    #[tokio::test]
    async fn ready_snapshot_des_co_membres_et_ping() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let (a, b, c) = (
            e.utilisateur("alice").await,
            e.utilisateur("bob").await,
            e.utilisateur("carl").await,
        );
        e.partager(&a, &b).await;
        let _wb = e.connect_ready("bob").await;
        let _wc = e.connect_ready("carl").await;
        let mut wa = e.connecter("alice").await;
        let (typ, data) = suivant(&mut wa).await;
        assert_eq!((typ.as_str(), data), ("ready", json!({ "user_id": a })));
        let (typ, data) = suivant(&mut wa).await;
        assert_eq!(typ, "presence.snapshot");
        assert_eq!(data, json!({ "online": [b], "activities": {} }));
        assert!(!data.to_string().contains(&c));
        wa.send(Message::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();
        assert_eq!(attendre(&mut wa, "pong").await, Value::Null);
        wa.send(Message::Text(r#"{"type":"autre"}"#.into()))
            .await
            .unwrap();
        wa.send(Message::Text("pas du json".into())).await.unwrap();
        wa.send(Message::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();
        attendre(&mut wa, "pong").await;
    }

    #[tokio::test]
    async fn publish_arrive_avec_la_meme_enveloppe() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let (a, b) = (e.utilisateur("alice").await, e.utilisateur("bob").await);
        let mut wa = e.connect_ready("alice").await;
        let mut wb = e.connect_ready("bob").await;
        let mut wa2 = e.connect_ready("alice").await;
        e.state
            .publier(&[a], "typing", &json!({ "channel_id": "c1", "user_id": b }));
        for w in [&mut wa, &mut wa2] {
            let Some(Ok(Message::Text(t))) = timeout(DELAI, w.next()).await.unwrap() else {
                panic!("pas de texte");
            };
            assert_eq!(
                t.as_str(),
                format!(r#"{{"type":"typing","data":{{"channel_id":"c1","user_id":"{b}"}}}}"#)
            );
        }
        e.state.publier(std::slice::from_ref(&b), "x", &json!(1));
        assert_eq!(attendre(&mut wb, "x").await, json!(1));
    }

    #[tokio::test]
    async fn presence_premiere_et_derniere_connexion_seulement() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let (a, b) = (e.utilisateur("alice").await, e.utilisateur("bob").await);
        e.partager(&a, &b).await;
        let mut wa = e.connect_ready("alice").await;
        let mut wb1 = e.connect_ready("bob").await;
        assert_eq!(
            attendre(&mut wa, "presence").await,
            json!({ "user_id": b, "online": true })
        );
        let mut wb2 = e.connect_ready("bob").await;
        wb1.close(None).await.unwrap();
        jusqua(|| e.state.hub().counts() == (2, 2)).await;
        wb2.close(None).await.unwrap();
        assert_eq!(
            attendre(&mut wa, "presence").await,
            json!({ "user_id": b, "online": false })
        );
        e.state.publier(&[a], "fin", &Value::Null);
        loop {
            let (typ, _) = suivant(&mut wa).await;
            assert_ne!(typ, "presence", "présence en trop");
            if typ == "fin" {
                break;
            }
        }
    }

    #[tokio::test]
    async fn presence_des_nouvelles_relations() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let (a, b) = (e.utilisateur("alice").await, e.utilisateur("bob").await);
        let mut wb = e.connect_ready("bob").await;
        let mut wa = e.connecter("alice").await;
        attendre(&mut wa, "ready").await;
        let snapshot = attendre(&mut wa, "presence.snapshot").await;
        assert_eq!(snapshot["online"], json!([]));

        e.partager(&a, &b).await;
        let mut wb2 = e.connect_ready("bob").await;
        wb.close(None).await.unwrap();
        wb2.close(None).await.unwrap();
        assert_eq!(
            attendre(&mut wa, "presence").await,
            json!({ "user_id": b, "online": false })
        );
        let mut wa2 = e.connecter("alice").await;
        attendre(&mut wa2, "ready").await;
        let snapshot = attendre(&mut wa2, "presence.snapshot").await;
        assert_eq!(snapshot["online"], json!([]));
        let mut wb3 = e.connecter("bob").await;
        attendre(&mut wb3, "ready").await;
        let snapshot = attendre(&mut wb3, "presence.snapshot").await;
        assert_eq!(snapshot["online"], json!([a]));
    }

    #[tokio::test]
    async fn activite_dans_le_snapshot_puis_effacee_a_la_deconnexion() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let (a, b) = (e.utilisateur("alice").await, e.utilisateur("bob").await);
        e.partager(&a, &b).await;
        let mut wb = e.connect_ready("bob").await;
        assert!(e.state.hub().set_activity(&b, "PokeMMO"));
        let mut wa = e.connecter("alice").await;
        attendre(&mut wa, "ready").await;
        let snapshot = attendre(&mut wa, "presence.snapshot").await;
        assert_eq!(snapshot["activities"], json!({ b.clone(): "PokeMMO" }));
        wb.close(None).await.unwrap();
        jusqua(|| !e.state.hub().online(&b)).await;
        let mut wa2 = e.connecter("alice").await;
        attendre(&mut wa2, "ready").await;
        let snapshot = attendre(&mut wa2, "presence.snapshot").await;
        assert_eq!(snapshot["activities"], json!({}));
        assert!(e.state.hub().activities_among(&[b]).is_empty());
    }

    #[tokio::test]
    async fn connexion_muette_fermee_apres_idle() {
        let durees = DureesWs {
            idle: Duration::from_millis(100),
            ..durees_par_defaut()
        };
        let Some(e) = env(durees).await else {
            return;
        };
        let a = e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        assert_eq!(fermeture(&mut w).await.map(|f| f.0), Some(1000));
        jusqua(|| !e.state.hub().online(&a)).await;
    }

    #[tokio::test]
    async fn lecture_au_dela_de_1_kio_refusee() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let a = e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        w.send(Message::Text("x".repeat(2000).into()))
            .await
            .unwrap();
        let fin = fermeture(&mut w).await;
        assert_eq!(fin.map(|f| f.0), Some(1009));
        jusqua(|| !e.state.hub().online(&a)).await;
    }

    #[tokio::test]
    async fn session_expiree_fermee_en_4401() {
        let Some(e) = env(durees_rapides(Duration::from_secs(600))).await else {
            return;
        };
        e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        e.revoques.lock().unwrap().insert("alice".into());
        assert_eq!(
            fermeture(&mut w).await,
            Some((4401, "session expirée".to_string()))
        );
    }

    #[tokio::test]
    async fn acces_retire_ferme_en_4403_apres_le_rappel() {
        let Some(e) = env(durees_rapides(Duration::from_secs(600))).await else {
            return;
        };
        let a = e.utilisateur("alice").await;
        let appels: Arc<Mutex<Vec<String>>> = Arc::default();
        e.state.sur_acces_retire({
            let appels = appels.clone();
            move |_, id| {
                let appels = appels.clone();
                async move { appels.lock().unwrap().push(id) }
            }
        });
        let mut w = e.connect_ready("alice").await;
        e.refuses.lock().unwrap().insert("sub-alice".into());
        e.avancer(Duration::from_secs(31));
        assert_eq!(
            fermeture(&mut w).await,
            Some((4403, "accès à Estompe retiré".to_string()))
        );
        assert_eq!(*appels.lock().unwrap(), [a]);
    }

    #[tokio::test]
    async fn panne_de_permission_toleree_sous_la_grace_puis_4403() {
        let grace = Duration::from_millis(300);
        let Some(e) = env(durees_rapides(grace)).await else {
            return;
        };
        e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        e.permission_hs.store(true, Ordering::SeqCst);
        e.avancer(Duration::from_secs(31));
        let debut = Instant::now();
        assert_eq!(
            fermeture(&mut w).await,
            Some((4403, "service de permission indisponible".to_string()))
        );
        assert!(debut.elapsed() >= grace, "{:?}", debut.elapsed());
    }

    #[tokio::test]
    async fn panne_de_permission_courte_ne_ferme_pas() {
        let Some(e) = env(durees_rapides(Duration::from_secs(600))).await else {
            return;
        };
        let a = e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        e.permission_hs.store(true, Ordering::SeqCst);
        e.avancer(Duration::from_secs(31));
        let avant = e.permission.appels();
        jusqua(|| e.permission.appels() >= avant + 2).await;
        assert!(e.state.hub().online(&a));
        e.permission_hs.store(false, Ordering::SeqCst);
        w.send(Message::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();
        attendre(&mut w, "pong").await;
    }

    #[tokio::test]
    async fn plateforme_et_version_bornees_dans_clients() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        e.utilisateur("alice").await;
        let ticket = e.ticket_valide("alice").await;
        let plateforme = "é".repeat(10);
        let version = "9".repeat(30);
        let mut w = e
            .tenter(
                &ticket,
                &format!("&platform={plateforme}&v={version}"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(suivant(&mut w).await.0, "ready");
        let clients = e.state.hub().clients();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].platform, "é".repeat(8));
        assert_eq!(clients[0].version, "9".repeat(24));
    }

    #[tokio::test]
    async fn close_all_ferme_proprement_en_1008() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        let a = e.utilisateur("alice").await;
        let mut w = e.connect_ready("alice").await;
        e.state.hub().close_all();
        assert_eq!(
            fermeture(&mut w).await,
            Some((1008, "client trop lent ou serveur arrêté".to_string()))
        );
        jusqua(|| !e.state.hub().online(&a)).await;
    }

    #[tokio::test]
    async fn metrics_compte_les_connexions() {
        let Some(e) = env(durees_par_defaut()).await else {
            return;
        };
        e.utilisateur("alice").await;
        let _w1 = e.connect_ready("alice").await;
        let _w2 = e.connect_ready("alice").await;
        let r = e.state.metrics().rendu(&crate::metrics::Instantane {
            uptime: 0,
            base_up: true,
            totaux: None,
            stockage: None,
            ws: e.state.hub().counts(),
        });
        assert!(r.contains("estompe_ws_users 1\n"), "{r}");
        assert!(r.contains("estompe_ws_connections 2\n"), "{r}");
    }
}
