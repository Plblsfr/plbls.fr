#![allow(clippy::result_large_err)]

use std::{sync::Arc, time::Duration};

use axum::{
    Extension,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::post,
};
use estompe_core::{
    api::ActiveUser,
    blob::{Disque, nouvelle_cle},
    fichiers::{TAILLE_VOCAL_MAX, est_uuid},
    http::{champ_texte, decode, internal, write_err, write_json},
    messages::{conv_du_message, message_complet, publier},
    openapi, salons,
    state::AppState,
};
use reqwest::Client;
use serde_json::json;
use sqlx::{PgPool, Row};
use tokio::{io::AsyncReadExt, sync::Semaphore};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const TRANSCRIPTION_MAX: usize = 4000;
const PAR_HEURE: usize = 20;
const SIMULTANEES: usize = 2;
const DELAI: Duration = Duration::from_secs(5 * 60);
const REPONSE_MAX: usize = 1 << 20;

pub struct Service {
    client: Client,
    places: Semaphore,
}

impl Service {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(DELAI)
                .build()
                .expect("client HTTP"),
            places: Semaphore::new(SIMULTANEES),
        }
    }

    async fn transcrire(
        &self,
        url: &str,
        modele: &str,
        langue: &str,
        nom: &str,
        audio: &[u8],
    ) -> Result<String, String> {
        let frontiere = format!("estompe{}", nouvelle_cle());
        let mut corps = Vec::with_capacity(audio.len() + 512);
        let mut champ = |nom: &str, valeur: &str| {
            corps.extend_from_slice(
                format!(
                    "--{frontiere}\r\nContent-Disposition: form-data; name=\"{nom}\"\r\n\r\n{valeur}\r\n"
                )
                .as_bytes(),
            );
        };
        champ("model", modele);
        champ("response_format", "json");
        if !langue.is_empty() {
            champ("language", langue);
        }
        let nom = nom
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(['\r', '\n'], "");
        corps.extend_from_slice(
            format!(
                "--{frontiere}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{nom}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        corps.extend_from_slice(audio);
        corps.extend_from_slice(format!("\r\n--{frontiere}--\r\n").as_bytes());
        let mut reponse = self
            .client
            .post(format!(
                "{}/v1/audio/transcriptions",
                url.trim_end_matches('/')
            ))
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={frontiere}"),
            )
            .body(corps)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let mut brut = Vec::new();
        while let Some(morceau) = reponse.chunk().await.map_err(|e| e.to_string())? {
            brut.extend_from_slice(&morceau);
            if brut.len() > REPONSE_MAX {
                brut.truncate(REPONSE_MAX);
                break;
            }
        }
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(format!(
                "transcription : statut {}",
                reponse.status().as_u16()
            ));
        }
        let sortie: serde_json::Value =
            serde_json::from_slice(&brut).map_err(|_| "transcription : réponse illisible")?;
        Ok(sortie["text"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string())
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

pub fn routes(service: Arc<Service>) -> OpenApiRouter<AppState> {
    let chemin = "/files/{id}/transcript";
    let transcrire = openapi::operation(
        "fichiers",
        "Transcrire un message vocal visible par l'appelant",
        &[
            ("200", "ok"),
            ("404", "introuvable ou pas un message vocal"),
            ("410", "file_expired"),
            ("422", "no_speech"),
            ("429", "rate_limited ou transcription_busy"),
            ("501", "transcription_disabled"),
            ("502", "transcription_failed"),
        ],
        chemin,
    );
    let corriger = openapi::operation(
        "fichiers",
        "Corriger ou retirer (texte vide) la transcription d'un message vocal (auteur seul)",
        &[
            ("200", "ok"),
            ("400", "invalid_transcript ou bad_request"),
            ("403", "forbidden"),
            ("404", "introuvable ou pas un message vocal"),
        ],
        chemin,
    );
    let r = OpenApiRouter::new().route(chemin, post(transcrire_fichier).put(corriger_fichier));
    let r = openapi::documenter(r, chemin, HttpMethod::Post, transcrire);
    openapi::documenter(r, chemin, HttpMethod::Put, corriger).layer(Extension(service))
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes(Arc::new(Service::new())))
}

fn non_trouve() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

struct Vocal {
    nom: String,
    cle: String,
    expire: bool,
    message: Option<i64>,
}

async fn vocal_visible(pool: &PgPool, id: &str, user: &str) -> Result<Vocal, Response> {
    let ligne = sqlx::query(
        "SELECT name, key, purged_at IS NOT NULL, kind, message_id, uploader_id::text FROM files WHERE id=$1::uuid",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| internal())?
    .ok_or_else(non_trouve)?;
    if ligne.get::<String, _>(3) != "voice" {
        return Err(non_trouve());
    }
    let message: Option<i64> = ligne.get(4);
    let visible = match message {
        None => ligne.get::<String, _>(5) == user,
        Some(m) => salons::voit_message(pool, m, user)
            .await
            .map_err(|_| internal())?,
    };
    if !visible {
        return Err(non_trouve());
    }
    Ok(Vocal {
        nom: ligne.get(0),
        cle: ligne.get(1),
        expire: ligne.get(2),
        message,
    })
}

fn vue(texte: &str, auto: bool) -> Response {
    write_json(
        StatusCode::OK,
        &json!({ "transcript": texte, "transcript_auto": auto }),
    )
}

async fn diffuser(state: &AppState, pool: &PgPool, message: Option<i64>, user: &str) {
    let Some(id) = message else { return };
    let Ok((conv, _)) = conv_du_message(pool, id, user).await else {
        return;
    };
    if let Ok(m) = message_complet(pool, id, user).await {
        publier(state, pool, &conv, "message.updated", &m).await;
    }
}

async fn transcrire_fichier(
    State(state): State<AppState>,
    Extension(service): Extension<Arc<Service>>,
    Path(id): Path<String>,
    ActiveUser(user): ActiveUser,
) -> Response {
    let config = state.config();
    if config.transcribe_url.is_empty() {
        return write_err(
            StatusCode::NOT_IMPLEMENTED,
            "transcription_disabled",
            "la transcription n'est pas activée sur ce serveur",
        );
    }
    if !est_uuid(&id) {
        return non_trouve();
    }
    let Ok(pool) = state.pool() else {
        return internal();
    };
    let fichier = match vocal_visible(pool, &id, &user.id).await {
        Ok(f) => f,
        Err(r) => return r,
    };
    if fichier.expire {
        return write_err(
            StatusCode::GONE,
            "file_expired",
            "fichier supprimé du serveur",
        );
    }
    let existant: Result<String, _> =
        sqlx::query_scalar("SELECT transcript FROM files WHERE id=$1::uuid")
            .bind(&id)
            .fetch_one(pool)
            .await;
    if let Ok(texte) = existant
        && !texte.is_empty()
    {
        return vue(&texte, true);
    }
    if !state
        .limiteurs()
        .transcription
        .allow(&user.id, PAR_HEURE, Duration::from_secs(3600))
    {
        return write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "trop de transcriptions, réessayez plus tard",
        );
    }
    let Ok(_place) = service.places.try_acquire() else {
        return write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "transcription_busy",
            "transcription occupée, réessayez dans un instant",
        );
    };
    let Ok(mut source) = Disque::new(&config.data_dir).ouvrir(&fichier.cle).await else {
        return write_err(
            StatusCode::GONE,
            "file_expired",
            "fichier introuvable sur le serveur",
        );
    };
    let mut audio = Vec::new();
    if (&mut source)
        .take(TAILLE_VOCAL_MAX as u64 + 1)
        .read_to_end(&mut audio)
        .await
        .is_err()
    {
        return internal();
    }
    let texte = match service
        .transcrire(
            &config.transcribe_url,
            &config.transcribe_model,
            &config.transcribe_language,
            &fichier.nom,
            &audio,
        )
        .await
    {
        Ok(t) => t.chars().take(TRANSCRIPTION_MAX).collect::<String>(),
        Err(e) => {
            tracing::warn!("transcription : {e}");
            return write_err(
                StatusCode::BAD_GATEWAY,
                "transcription_failed",
                "la transcription a échoué",
            );
        }
    };
    if texte.is_empty() {
        return write_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "no_speech",
            "aucune parole détectée",
        );
    }
    let change = sqlx::query(
        "UPDATE files SET transcript=$2, transcript_auto=true WHERE id=$1::uuid AND kind='voice' AND purged_at IS NULL AND transcript = ''",
    )
    .bind(&id)
    .bind(&texte)
    .execute(pool)
    .await;
    match change {
        Err(_) => return internal(),
        Ok(r) if r.rows_affected() > 0 => diffuser(&state, pool, fichier.message, &user.id).await,
        Ok(_) => {}
    }
    vue(&texte, true)
}

async fn corriger_fichier(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ActiveUser(user): ActiveUser,
    corps: Body,
) -> Response {
    let objet = match decode(corps).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let Some(texte) = champ_texte(&objet, "text") else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide");
    };
    let texte = texte.trim();
    if texte.chars().count() > TRANSCRIPTION_MAX || !est_uuid(&id) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_transcript",
            "transcription : 4000 caractères au plus",
        );
    }
    let Ok(pool) = state.pool() else {
        return internal();
    };
    let fichier = match vocal_visible(pool, &id, &user.id).await {
        Ok(f) => f,
        Err(r) => return r,
    };
    let change = sqlx::query(
        "UPDATE files SET transcript=$2, transcript_auto=false WHERE id=$1::uuid AND kind='voice' AND purged_at IS NULL AND uploader_id=$3::uuid",
    )
    .bind(&id)
    .bind(texte)
    .bind(&user.id)
    .execute(pool)
    .await;
    match change {
        Err(_) => return internal(),
        Ok(r) if r.rows_affected() == 0 => {
            return write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "seul l'auteur peut modifier la transcription",
            );
        }
        Ok(_) => {}
    }
    diffuser(&state, pool, fichier.message, &user.id).await;
    vue(texte, false)
}

#[cfg(test)]
mod tests {
    use super::{Service, TRANSCRIPTION_MAX, routes};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use estompe_core::{
        blob::Disque,
        db::migrate_embarquees,
        identity::Cred,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux, faux_permission},
    };
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tower::ServiceExt;

    struct Env {
        app: Router,
        state: AppState,
        alice: String,
        bob: String,
        _dossier: tempfile::TempDir,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(url: &str, extra: &[(&str, &str)]) -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_permission(|_, _| true).await;
        let (pg, base) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&base).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        let mut ids = Vec::new();
        for nom in ["alice", "bob", "carol"] {
            ids.push(
                sqlx::query_scalar::<_, String>(
                    "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
                )
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .fetch_one(&pool)
                .await
                .unwrap(),
            );
        }
        let dossier = tempfile::tempdir().unwrap();
        let data = dossier.path().join("data");
        let mut reglages = vec![
            ("GATEWAY_URL", gateway.url.as_str()),
            ("PERMISSION_URL", permission.url.as_str()),
            ("DATA_DIR", data.to_str().unwrap()),
            ("TRANSCRIBE_URL", url),
            ("TRANSCRIBE_MODEL", "modele"),
            ("TRANSCRIBE_LANGUAGE", "fr"),
        ];
        reglages.extend_from_slice(extra);
        let state = AppState::new(testutil::config_avec(&reglages), vec![]);
        state.set_pool(pool);
        let app = Router::from(routes(Arc::new(Service::new()))).with_state(state.clone());
        Some(Env {
            app,
            state,
            alice: ids[0].clone(),
            bob: ids[1].clone(),
            _dossier: dossier,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    impl Env {
        fn pool(&self) -> &sqlx::PgPool {
            self.state.pool().ok().unwrap()
        }

        async fn vocal(&self, kind: &str) -> String {
            let cle = estompe_core::blob::nouvelle_cle();
            Disque::new(&self.state.config().data_dir)
                .put(&cle, b"audio")
                .await
                .unwrap();
            sqlx::query_scalar(
                "INSERT INTO files (uploader_id, name, mime, size, key, kind) VALUES ($1::uuid,'v.webm','audio/webm',5,$2,$3) RETURNING id::text",
            )
            .bind(&self.alice)
            .bind(cle)
            .bind(kind)
            .fetch_one(self.pool())
            .await
            .unwrap()
        }

        async fn envoyer(&self, fichier: &str) {
            let dm: String = sqlx::query_scalar(
                "INSERT INTO dm_conversations (user_a, user_b) VALUES (LEAST($1::uuid, $2::uuid), GREATEST($1::uuid, $2::uuid)) RETURNING id::text",
            )
            .bind(&self.alice)
            .bind(&self.bob)
            .fetch_one(self.pool())
            .await
            .unwrap();
            let message: i64 = sqlx::query_scalar(
                "INSERT INTO messages (dm_id, author_id, content) VALUES ($1::uuid,$2::uuid,'') RETURNING id",
            )
            .bind(dm)
            .bind(&self.alice)
            .fetch_one(self.pool())
            .await
            .unwrap();
            sqlx::query("UPDATE files SET message_id=$1 WHERE id=$2::uuid")
                .bind(message)
                .bind(fichier)
                .execute(self.pool())
                .await
                .unwrap();
        }

        async fn appel(
            &self,
            nom: &str,
            methode: &str,
            chemin: &str,
            corps: &str,
        ) -> (StatusCode, Value) {
            let rep = self
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(methode)
                        .uri(chemin)
                        .header(header::AUTHORIZATION, format!("Bearer {nom}"))
                        .body(Body::from(corps.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let s = rep.status();
            let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
            (s, serde_json::from_slice(&c).unwrap_or(Value::Null))
        }
    }

    fn code(r: &(StatusCode, Value)) -> (u16, Option<&str>) {
        (r.0.as_u16(), r.1["error"].as_str())
    }

    #[tokio::test]
    async fn sans_service_501_et_fichiers_inconnus() {
        let Some(e) = env("", &[]).await else { return };
        let id = e.vocal("voice").await;
        let r = e
            .appel("alice", "POST", &format!("/files/{id}/transcript"), "")
            .await;
        assert_eq!(code(&r), (501, Some("transcription_disabled")));
        assert_eq!(
            r.1["message"],
            "la transcription n'est pas activée sur ce serveur"
        );
    }

    #[tokio::test]
    async fn transcription_une_fois_correction_et_visibilite() {
        let service = faux(|r| {
            let corps = String::from_utf8_lossy(&r.corps).to_string();
            if r.chemin != "/v1/audio/transcriptions"
                || !corps.contains("name=\"model\"\r\n\r\nmodele")
                || !corps.contains("name=\"response_format\"\r\n\r\njson")
                || !corps.contains("name=\"language\"\r\n\r\nfr")
                || !corps.contains("filename=\"v.webm\"")
                || !corps.contains("audio")
            {
                return (400, vec![]);
            }
            (
                200,
                json!({ "text": "  Rendez-vous demain matin " })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let Some(e) = env(&service.url, &[]).await else {
            return;
        };
        let (_, mut rx) = {
            let (_, rx, _) = e.state.hub().register(
                &e.bob,
                Cred {
                    value: "x".into(),
                    cookie: false,
                },
            );
            ((), rx)
        };
        let id = e.vocal("voice").await;
        let chemin = format!("/files/{id}/transcript");

        let r = e.appel("bob", "POST", &chemin, "").await;
        assert_eq!(code(&r), (404, Some("not_found")));
        let r = e
            .appel("alice", "POST", "/files/pas-un-uuid/transcript", "")
            .await;
        assert_eq!(r.0, 404);
        let texte = e.vocal("file").await;
        let r = e
            .appel("alice", "POST", &format!("/files/{texte}/transcript"), "")
            .await;
        assert_eq!(r.0, 404);

        let r = e.appel("alice", "POST", &chemin, "").await;
        assert_eq!(r.0, 200, "{:?}", r.1);
        assert_eq!(
            r.1,
            json!({ "transcript": "Rendez-vous demain matin", "transcript_auto": true })
        );
        let r = e.appel("alice", "POST", &chemin, "").await;
        assert_eq!(r.0, 200);
        assert_eq!(service.appels(), 1);

        let r = e
            .appel(
                "alice",
                "PUT",
                &chemin,
                r#"{"text":"  Rendez-vous demain à 9h "}"#,
            )
            .await;
        assert_eq!(
            r.1,
            json!({ "transcript": "Rendez-vous demain à 9h", "transcript_auto": false })
        );
        let long = "a".repeat(TRANSCRIPTION_MAX + 1);
        let r = e
            .appel(
                "alice",
                "PUT",
                &chemin,
                &json!({ "text": long }).to_string(),
            )
            .await;
        assert_eq!(code(&r), (400, Some("invalid_transcript")));
        let r = e.appel("alice", "PUT", &chemin, "pas du json").await;
        assert_eq!(code(&r), (400, Some("bad_request")));

        e.envoyer(&id).await;
        while rx.try_recv().is_ok() {}
        let r = e.appel("bob", "POST", &chemin, "").await;
        assert_eq!(r.1["transcript"], "Rendez-vous demain à 9h");
        assert_eq!(service.appels(), 1);
        let r = e.appel("bob", "PUT", &chemin, r#"{"text":"pirate"}"#).await;
        assert_eq!(code(&r), (403, Some("forbidden")));
        let r = e.appel("carol", "PUT", &chemin, r#"{"text":"x"}"#).await;
        assert_eq!(r.0, 404);

        let r = e.appel("alice", "PUT", &chemin, r#"{"text":""}"#).await;
        assert_eq!(r.1, json!({ "transcript": "", "transcript_auto": false }));
        let m: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(m["type"], "message.updated");
        assert_eq!(m["data"]["attachments"][0]["id"], id);
        let stocke: String = sqlx::query_scalar("SELECT transcript FROM files WHERE id=$1::uuid")
            .bind(&id)
            .fetch_one(e.pool())
            .await
            .unwrap();
        assert_eq!(stocke, "");
    }

    #[tokio::test]
    async fn echecs_du_service_expiration_et_limites() {
        let service = faux(|r| {
            let corps = String::from_utf8_lossy(&r.corps).to_string();
            if corps.contains("echec") {
                (500, vec![])
            } else if corps.contains("silence") {
                (200, br#"{"text":"  "}"#.to_vec())
            } else if corps.contains("illisible") {
                (200, b"pas du json".to_vec())
            } else {
                (
                    200,
                    json!({ "text": "é".repeat(TRANSCRIPTION_MAX + 10) })
                        .to_string()
                        .into_bytes(),
                )
            }
        })
        .await;
        let Some(e) = env(&service.url, &[]).await else {
            return;
        };
        let disque = Disque::new(&e.state.config().data_dir);
        let mut ids = Vec::new();
        for contenu in ["echec", "silence", "illisible", "long"] {
            let id = e.vocal("voice").await;
            let cle: String = sqlx::query_scalar("SELECT key FROM files WHERE id=$1::uuid")
                .bind(&id)
                .fetch_one(e.pool())
                .await
                .unwrap();
            disque.put(&cle, contenu.as_bytes()).await.unwrap();
            ids.push(id);
        }
        let post = |i: usize| format!("/files/{}/transcript", ids[i]);
        assert_eq!(
            code(&e.appel("alice", "POST", &post(0), "").await),
            (502, Some("transcription_failed"))
        );
        assert_eq!(
            code(&e.appel("alice", "POST", &post(1), "").await),
            (422, Some("no_speech"))
        );
        assert_eq!(
            code(&e.appel("alice", "POST", &post(2), "").await),
            (502, Some("transcription_failed"))
        );
        let r = e.appel("alice", "POST", &post(3), "").await;
        assert_eq!(
            r.1["transcript"].as_str().unwrap().chars().count(),
            TRANSCRIPTION_MAX
        );

        sqlx::query("UPDATE files SET purged_at = now() WHERE id=$1::uuid")
            .bind(&ids[0])
            .execute(e.pool())
            .await
            .unwrap();
        assert_eq!(
            code(&e.appel("alice", "POST", &post(0), "").await),
            (410, Some("file_expired"))
        );

        let manquant = e.vocal("voice").await;
        sqlx::query("UPDATE files SET key = 'cle-absente-0123456789abcdef' WHERE id=$1::uuid")
            .bind(&manquant)
            .execute(e.pool())
            .await
            .unwrap();
        assert_eq!(
            code(
                &e.appel(
                    "alice",
                    "POST",
                    &format!("/files/{manquant}/transcript"),
                    ""
                )
                .await
            ),
            (410, Some("file_expired"))
        );

        for _ in 0..20 {
            e.state.limiteurs().transcription.allow(
                &e.alice,
                20,
                std::time::Duration::from_secs(3600),
            );
        }
        let r = e.appel("alice", "POST", &post(1), "").await;
        assert_eq!(code(&r), (429, Some("rate_limited")));
    }

    #[tokio::test]
    async fn deux_transcriptions_simultanees_au_plus() {
        let service = faux(|_| (200, br#"{"text":"ok"}"#.to_vec())).await;
        let Some(e) = env(&service.url, &[]).await else {
            return;
        };
        let service_blocs = Arc::new(Service::new());
        let _a = service_blocs.places.try_acquire().unwrap();
        let _b = service_blocs.places.try_acquire().unwrap();
        let app = Router::from(routes(service_blocs.clone())).with_state(e.state.clone());
        let id = e.vocal("voice").await;
        let rep = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/files/{id}/transcript"))
                    .header(header::AUTHORIZATION, "Bearer alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rep.status(), StatusCode::TOO_MANY_REQUESTS);
        let c: Value =
            serde_json::from_slice(&to_bytes(rep.into_body(), 1 << 20).await.unwrap()).unwrap();
        assert_eq!(c["error"], "transcription_busy");
        assert_eq!(service.appels(), 0);
    }
}
