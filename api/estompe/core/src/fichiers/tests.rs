use super::*;
use crate::{
    mp4::tests::mp4_valide,
    testutil::{self, BasePostgres, Faux, faux},
    users,
};
use axum::{Router, body::to_bytes};
use tower::ServiceExt;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
const WEBM: &[u8] = b"\x1aE\xdf\xa3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

struct Env {
    app: Router,
    state: AppState,
    _dossier: tempfile::TempDir,
    _g: Faux,
    _p: Faux,
    _pg: BasePostgres,
}

fn gateway(r: &testutil::Requete) -> (u16, Vec<u8>) {
    match r.entete("authorization").strip_prefix("Bearer ") {
        Some(nom) => (
            200,
            json!({ "id": format!("sub-{nom}"), "name": nom })
                .to_string()
                .into_bytes(),
        ),
        None => (401, vec![]),
    }
}

fn permission(r: &testutil::Requete) -> (u16, Vec<u8>) {
    testutil::reponse_droits(r, |sub, p| match p {
        "estompe:access" => true,
        p if testutil::est_admin(p) => sub == "sub-admin",
        p => testutil::ouverte(p),
    })
}

async fn env(extra: &[(&str, &str)], regler: impl FnOnce(&mut Config)) -> Option<Env> {
    let (pg, url) = testutil::postgres().await?;
    let g = faux(gateway).await;
    let p = faux(permission).await;
    let dossier = tempfile::tempdir().unwrap();
    let data = dossier.path().join("data");
    let mut vars = vec![
        ("GATEWAY_URL", g.url.as_str()),
        ("AEGIS_URL", p.url.as_str()),
        ("DATA_DIR", data.to_str().unwrap()),
    ];
    vars.extend_from_slice(extra);
    let mut config = testutil::config_avec(&vars);
    regler(&mut config);
    let state = AppState::new(config, vec![]);
    let pool = PgPool::connect(&url).await.unwrap();
    db::migrate_embarquees(&pool).await.unwrap();
    state.set_pool(pool);
    Some(Env {
        app: Router::from(router()).with_state(state.clone()),
        state,
        _dossier: dossier,
        _g: g,
        _p: p,
        _pg: pg,
    })
}

impl Env {
    fn pool(&self) -> &PgPool {
        self.state.pool().ok().unwrap()
    }

    async fn user(&self, nom: &str) -> String {
        users::activate(self.pool(), &format!("sub-{nom}"), nom)
            .await
            .ok()
            .unwrap()
            .id
    }

    async fn requete(
        &self,
        methode: &str,
        chemin: &str,
        nom: Option<&str>,
        entetes: &[(&str, &str)],
        corps: Vec<u8>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut r = Request::builder().method(methode).uri(chemin);
        if let Some(n) = nom {
            r = r.header(header::AUTHORIZATION, format!("Bearer {n}"));
        }
        for (k, v) in entetes {
            r = r.header(*k, *v);
        }
        let rep = self
            .app
            .clone()
            .oneshot(r.body(Body::from(corps)).unwrap())
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        (
            s,
            h,
            to_bytes(rep.into_body(), 64 << 20).await.unwrap().to_vec(),
        )
    }

    async fn envoi(
        &self,
        nom: &str,
        champs: &[(&str, &str)],
        fichier: Option<(&str, &[u8])>,
        miniature: Option<&[u8]>,
    ) -> (StatusCode, Value) {
        let (ct, corps) = multipart(champs, fichier, miniature);
        let (s, h, c) = self
            .requete("POST", "/files", Some(nom), &[("content-type", &ct)], corps)
            .await;
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        (s, serde_json::from_slice(&c).unwrap())
    }

    async fn envoi_ok(&self, nom: &str, fichier: &str, donnees: &[u8]) -> String {
        let (s, v) = self.envoi(nom, &[], Some((fichier, donnees)), None).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    async fn lire(&self, nom: Option<&str>, chemin: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        self.requete("GET", chemin, nom, &[], vec![]).await
    }

    async fn sql(&self, requete: &str, args: &[&str]) -> String {
        let mut q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(requete));
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_one(self.pool()).await.unwrap()
    }

    async fn serveur(&self, proprietaire: &str, membres: &[&str]) -> (String, String) {
        let serveur = self
            .sql(
                "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
                &[proprietaire],
            )
            .await;
        for m in std::iter::once(&proprietaire).chain(membres) {
            self.sql(
                    "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid) RETURNING user_id::text",
                    &[&serveur, m],
                )
                .await;
        }
        self.sql(
                "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid, '@tout le monde', 0, 25677568, true) RETURNING id::text",
                &[&serveur],
            )
            .await;
        let salon = self
                .sql(
                    "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid, 'general', 'text') RETURNING id::text",
                    &[&serveur],
                )
                .await;
        (serveur, salon)
    }

    async fn joindre(&self, salon: &str, auteur: &str, fichier: &str) {
        let message = self
                .sql(
                    "INSERT INTO messages (channel_id, author_id, content) VALUES ($1::uuid, $2::uuid, 'x') RETURNING id::text",
                    &[salon, auteur],
                )
                .await;
        sqlx::query("UPDATE files SET message_id=$1::bigint WHERE id=$2::uuid")
            .bind(message)
            .bind(fichier)
            .execute(self.pool())
            .await
            .unwrap();
    }
}

fn multipart(
    champs: &[(&str, &str)],
    fichier: Option<(&str, &[u8])>,
    miniature: Option<&[u8]>,
) -> (String, Vec<u8>) {
    let limite = "----limite";
    let mut c = Vec::new();
    for (k, v) in champs {
        c.extend_from_slice(
            format!("--{limite}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    if let Some(m) = miniature {
        c.extend_from_slice(
                format!("--{limite}\r\nContent-Disposition: form-data; name=\"thumb\"; filename=\"t.jpg\"\r\n\r\n")
                    .as_bytes(),
            );
        c.extend_from_slice(m);
        c.extend_from_slice(b"\r\n");
    }
    if let Some((nom, donnees)) = fichier {
        c.extend_from_slice(
                format!("--{limite}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{nom}\"\r\n\r\n")
                    .as_bytes(),
            );
        c.extend_from_slice(donnees);
        c.extend_from_slice(b"\r\n");
    }
    c.extend_from_slice(format!("--{limite}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={limite}"), c)
}

fn texte(c: &[u8]) -> String {
    String::from_utf8_lossy(c).into_owned()
}

#[tokio::test]
async fn cycle_de_vie_et_visibilite() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let (alice, bob) = (e.user("alice").await, e.user("bob").await);
    e.user("carol").await;
    let (s, v) = e.envoi("alice", &[], Some(("photo.png", PNG)), None).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(v["name"], "photo.png");
    assert_eq!(v["mime"], "image/png");
    assert_eq!(v["size"], PNG.len());
    assert_eq!(v["kind"], "file");
    assert_eq!(v["expired"], false);
    assert!(
        v.get("duration_ms").is_none() && v.get("waveform").is_none() && v.get("thumb").is_none()
    );
    let id = v["id"].as_str().unwrap().to_string();
    let chemin = format!("/files/{id}");

    let (s, h, c) = e.lire(Some("alice"), &chemin).await;
    assert_eq!((s, c.as_slice()), (StatusCode::OK, PNG));
    assert_eq!(h[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        h[header::CONTENT_DISPOSITION],
        "inline; filename*=UTF-8''photo.png; filename=\"photo.png\""
    );
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(
        h[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
    assert_eq!(h["cross-origin-resource-policy"], "same-site");
    assert_eq!(h[header::CACHE_CONTROL], "private, max-age=3600");
    assert_eq!(h[header::ACCEPT_RANGES], "bytes");
    assert_eq!(h[header::CONTENT_LENGTH], PNG.len().to_string());
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
    assert_eq!(e.lire(None, &chemin).await.0, StatusCode::UNAUTHORIZED);

    let (serveur, salon) = e.serveur(&alice, &[&bob]).await;
    e.joindre(&salon, &alice, &id).await;
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);
    assert_eq!(
        e.lire(Some("carol"), &chemin).await.0,
        StatusCode::NOT_FOUND
    );

    let defaut = e
        .sql(
            "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
            &[&serveur],
        )
        .await;
    sqlx::query("INSERT INTO channel_overrides (channel_id, role_id, allow, deny) VALUES ($1::uuid, $2::uuid, 0, 256)")
            .bind(&salon)
            .bind(&defaut)
            .execute(e.pool())
            .await
            .unwrap();
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
    assert_eq!(e.lire(Some("alice"), &chemin).await.0, StatusCode::OK);
    sqlx::query("INSERT INTO channel_member_overrides (channel_id, user_id, allow, deny) VALUES ($1::uuid, $2::uuid, 256, 0)")
            .bind(&salon)
            .bind(&bob)
            .execute(e.pool())
            .await
            .unwrap();
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);

    for mauvais in [
        "/files/00000000-0000-0000-0000-000000000000",
        "/files/pas-un-uuid",
    ] {
        assert_eq!(
            e.lire(Some("alice"), mauvais).await.0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn messages_prives() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let (a, b) = (e.user("alice").await, e.user("bob").await);
    e.user("carol").await;
    let (u1, u2) = if a < b { (&a, &b) } else { (&b, &a) };
    let dm = e
            .sql(
                "INSERT INTO dm_conversations (user_a, user_b) VALUES ($1::uuid, $2::uuid) RETURNING id::text",
                &[u1, u2],
            )
            .await;
    let id = e.envoi_ok("alice", "a.txt", b"bonjour").await;
    let message = e
            .sql(
                "INSERT INTO messages (dm_id, author_id, content) VALUES ($1::uuid, $2::uuid, 'x') RETURNING id::text",
                &[&dm, &a],
            )
            .await;
    sqlx::query("UPDATE files SET message_id=$1::bigint WHERE id=$2::uuid")
        .bind(message)
        .bind(&id)
        .execute(e.pool())
        .await
        .unwrap();
    let chemin = format!("/files/{id}");
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::OK);
    assert_eq!(
        e.lire(Some("carol"), &chemin).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn liens_signes() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let alice = e.user("alice").await;
    let bob = e.user("bob").await;
    let id = e.envoi_ok("alice", "photo.png", PNG).await;
    let lien = format!("/files/{id}/link");
    let (s, _, _) = e.requete("POST", &lien, Some("bob"), &[], vec![]).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = e.requete("POST", &lien, None, &[], vec![]).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, h, c) = e.requete("POST", &lien, Some("alice"), &[], vec![]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/json");
    let jeton = serde_json::from_slice::<Value>(&c).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();

    let (s, h, c) = e.lire(None, &format!("/files/{id}?t={jeton}")).await;
    assert_eq!((s, c.as_slice()), (StatusCode::OK, PNG));
    assert_eq!(h["cross-origin-resource-policy"], "cross-origin");
    let (s, _, _) = e.lire(None, &format!("/files/{id}/thumb?t={jeton}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    for mauvais in [
        format!("/files/{id}?t=abc"),
        format!("/files/{id}?t=9999999999.{}", "0".repeat(64)),
        format!("/files/00000000-0000-0000-0000-000000000000?t={jeton}"),
        format!("/files/pas-un-uuid?t={jeton}"),
    ] {
        let (s, _, c) = e.lire(None, &mauvais).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{mauvais}");
        assert_eq!(
            serde_json::from_slice::<Value>(&c).unwrap()["error"],
            "invalid_link"
        );
    }
    let perime = signer(e.state.cle_liens(), &id, &alice, maintenant() - 1);
    assert_eq!(
        e.lire(None, &format!("/files/{id}?t={perime}")).await.0,
        StatusCode::FORBIDDEN
    );

    let (serveur, salon) = e.serveur(&alice, &[&bob]).await;
    e.joindre(&salon, &alice, &id).await;
    let (s, _, c) = e.requete("POST", &lien, Some("bob"), &[], vec![]).await;
    assert_eq!(s, StatusCode::OK);
    let de_bob = serde_json::from_slice::<Value>(&c).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let chemin = format!("/files/{id}?t={de_bob}");
    assert_eq!(e.lire(None, &chemin).await.0, StatusCode::OK);
    sqlx::query("DELETE FROM server_members WHERE server_id=$1::uuid AND user_id=$2::uuid")
        .bind(&serveur)
        .bind(&bob)
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(e.lire(None, &chemin).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        e.lire(None, &format!("/files/{id}?t={jeton}")).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn lecture_par_plages() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let id = e.envoi_ok("alice", "a.txt", b"0123456789").await;
    let chemin = format!("/files/{id}");
    let avec = |range: &'static str| {
        let e = &e;
        let chemin = &chemin;
        async move {
            e.requete("GET", chemin, Some("alice"), &[("range", range)], vec![])
                .await
        }
    };
    let (s, h, c) = avec("bytes=2-5").await;
    assert_eq!(
        (s, texte(&c).as_str()),
        (StatusCode::PARTIAL_CONTENT, "2345")
    );
    assert_eq!(h[header::CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(h[header::CONTENT_LENGTH], "4");
    assert_eq!(h[header::CONTENT_TYPE], "application/octet-stream");
    let (s, h, c) = avec("bytes=7-").await;
    assert_eq!(
        (s, texte(&c).as_str()),
        (StatusCode::PARTIAL_CONTENT, "789")
    );
    assert_eq!(h[header::CONTENT_RANGE], "bytes 7-9/10");
    let (_, h, c) = avec("bytes=-3").await;
    assert_eq!(
        (
            texte(&c).as_str(),
            h[header::CONTENT_RANGE].to_str().unwrap()
        ),
        ("789", "bytes 7-9/10")
    );
    let (_, _, c) = avec("bytes=8-100").await;
    assert_eq!(texte(&c), "89");
    let (s, h, _) = avec("bytes=50-60").await;
    assert_eq!(s, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(h[header::CONTENT_RANGE], "bytes */10");
    assert!(h.get(header::ACCEPT_RANGES).is_none() && h.get(header::CACHE_CONTROL).is_none());
    let (s, h, _) = avec("bytes=-0").await;
    assert_eq!(s, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(h[header::CONTENT_RANGE], "bytes */10");
    let (s, h, c) = avec("octets=1-2").await;
    assert_eq!(
        (s, texte(&c).as_str()),
        (StatusCode::RANGE_NOT_SATISFIABLE, "invalid range\n")
    );
    assert!(h.get(header::CONTENT_RANGE).is_none());
    let (s, _, c) = e
        .requete(
            "GET",
            &chemin,
            Some("alice"),
            &[("range", "bytes=2-5"), ("if-range", "\"x\"")],
            vec![],
        )
        .await;
    assert_eq!((s, c.len()), (StatusCode::OK, 10));
    let (s, h, c) = e.requete("HEAD", &chemin, Some("alice"), &[], vec![]).await;
    assert_eq!((s, c.len()), (StatusCode::OK, 0));
    assert_eq!(h[header::CONTENT_LENGTH], "10");
}

#[tokio::test]
async fn controles_de_type() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let (s, v) = e
        .envoi(
            "alice",
            &[],
            Some(("faux.png", b"<script>alert(1)</script>")),
            None,
        )
        .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::UNSUPPORTED_MEDIA_TYPE, Some("type_mismatch"))
    );
    let (s, _) = e.envoi("alice", &[], Some(("vrai.jpg", PNG)), None).await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (s, _) = e.envoi("alice", &[], Some(("MAJ.PNG", PNG)), None).await;
    assert_eq!(s, StatusCode::CREATED);

    let page = e
        .envoi_ok(
            "alice",
            "page.html",
            b"<html><script>alert(1)</script></html>",
        )
        .await;
    let (_, h, _) = e.lire(Some("alice"), &format!("/files/{page}")).await;
    assert_eq!(h[header::CONTENT_TYPE], "application/octet-stream");
    assert!(
        h[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    assert!(
        h[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );

    let (s, v) = e
        .envoi("alice", &[], Some(("../../etc/pass\\wd", b"x")), None)
        .await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::CREATED, Some("wd")));

    let (s, v) = e.envoi("alice", &[], Some(("vide.txt", b"")), None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("bad_request"))
    );
    let (s, v) = e.envoi("alice", &[("kind", "voice")], None, None).await;
    assert_eq!(
        (s, v["message"].as_str()),
        (StatusCode::BAD_REQUEST, Some("champ « file » manquant"))
    );
    let (s, h, _) = e
        .requete(
            "POST",
            "/files",
            Some("alice"),
            &[("content-type", "text/plain")],
            b"x".to_vec(),
        )
        .await;
    assert_eq!(
        (s, h[header::CONTENT_TYPE].to_str().unwrap()),
        (StatusCode::BAD_REQUEST, "application/json")
    );
}

#[tokio::test]
async fn messages_vocaux() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let vocal = |duree: &'static str, onde: &'static str| {
        [
            ("kind", "voice"),
            ("duration_ms", duree),
            ("waveform", onde),
        ]
    };
    let (s, v) = e
        .envoi(
            "alice",
            &vocal("12000", "0,40,100"),
            Some(("m.webm", WEBM)),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["kind"], "voice");
    assert_eq!(v["mime"], "video/webm");
    assert_eq!(v["duration_ms"], 12000);
    assert_eq!(v["waveform"], json!([0, 40, 100]));
    for (duree, onde, code) in [
        ("0", "1", 400),
        ("300001", "1", 400),
        ("abc", "1", 400),
        ("1000", "101", 400),
        ("1000", "-1", 400),
        ("1000", "", 400),
        ("1000", "1,x", 400),
    ] {
        let (s, v) = e
            .envoi("alice", &vocal(duree, onde), Some(("m.webm", WEBM)), None)
            .await;
        assert_eq!(
            (s.as_u16(), v["error"].as_str()),
            (code, Some("invalid_voice")),
            "{duree} {onde}"
        );
    }
    let onde = vec!["1"; 101].join(",");
    let (s, _) = e
        .envoi(
            "alice",
            &[
                ("kind", "voice"),
                ("duration_ms", "1000"),
                ("waveform", &onde),
            ],
            Some(("m.webm", WEBM)),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = e
        .envoi(
            "alice",
            &vocal("1000", "1"),
            Some(("m.webm", b"<script>")),
            None,
        )
        .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Some("unsupported_voice")
        )
    );
    let gros = [WEBM, &vec![0; TAILLE_VOCAL_MAX]].concat();
    let (s, v) = e
        .envoi("alice", &vocal("1000", "1"), Some(("m.webm", &gros)), None)
        .await;
    assert_eq!(
        (s, v["message"].as_str()),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Some("5 Mo maximum pour un message vocal")
        )
    );
}

#[tokio::test]
async fn limites_de_taille() {
    let Some(e) = env(&[("MAX_VIDEO_SIZE_MB", "11")], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let juste = vec![b'a'; TAILLE_MAX as usize];
    let (s, _) = e.envoi("alice", &[], Some(("a.txt", &juste)), None).await;
    assert_eq!(s, StatusCode::CREATED);
    let trop = vec![b'a'; TAILLE_MAX as usize + 1];
    let (s, v) = e.envoi("alice", &[], Some(("b.txt", &trop)), None).await;
    assert_eq!(
        (s, v["error"].as_str(), v["message"].as_str()),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Some("file_too_large"),
            Some("10 Mo maximum")
        )
    );
    let video = vec![0u8; (11 << 20) + 1];
    let (s, v) = e
        .envoi("alice", &[("kind", "video")], Some(("v.mp4", &video)), None)
        .await;
    assert_eq!(
        (s, v["message"].as_str()),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Some("11 Mo maximum pour une vidéo")
        )
    );
    assert!(e.state.reserver_envoi_video(ENVOIS_VIDEO_MAX));
    e.state.liberer_envoi_video();
}

#[tokio::test]
async fn videos_et_miniatures() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let mp4 = mp4_valide(&[b"avc1", b"mp4a"], 1000, 4500);
    let jpeg = b"\xFF\xD8\xFF\xE0miniature";
    let (s, v) = e
        .envoi(
            "alice",
            &[("kind", "video")],
            Some(("clip.mp4", &mp4)),
            Some(jpeg),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(
        (
            v["kind"].as_str(),
            v["duration_ms"].as_i64(),
            v["thumb"].as_bool()
        ),
        (Some("video"), Some(4500), Some(true))
    );
    let id = v["id"].as_str().unwrap();
    let (s, h, c) = e.lire(Some("alice"), &format!("/files/{id}/thumb")).await;
    assert_eq!((s, c.as_slice()), (StatusCode::OK, &jpeg[..]));
    assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(h[header::CACHE_CONTROL], "private, max-age=3600");
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(
        e.lire(Some("alice"), &format!("/files/{id}/thumb?t=x"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let (s, v) = e
        .envoi(
            "alice",
            &[("kind", "video")],
            Some(("clip.mp4", b"pas une video")),
            None,
        )
        .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Some("unsupported_video")
        )
    );
    for mauvaise in [&b"GIF89a-pas-jpeg"[..], &vec![0xFF; MINIATURE_MAX + 1]] {
        let mut m = b"\xFF\xD8\xFF".to_vec();
        m.extend_from_slice(if mauvaise.len() > 100 { mauvaise } else { b"" });
        let miniature = if mauvaise.len() > 100 {
            m.as_slice()
        } else {
            mauvaise
        };
        let (s, v) = e
            .envoi(
                "alice",
                &[("kind", "video")],
                Some(("clip.mp4", &mp4)),
                Some(miniature),
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_thumb"))
        );
    }
    let img = e.envoi_ok("alice", "photo.png", PNG).await;
    assert_eq!(
        e.lire(Some("alice"), &format!("/files/{img}/thumb"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn envois_de_videos_simultanes() {
    let e = AppState::new(testutil::config(), vec![]);
    assert!((0..ENVOIS_VIDEO_MAX).all(|_| e.reserver_envoi_video(ENVOIS_VIDEO_MAX)));
    assert!(!e.reserver_envoi_video(ENVOIS_VIDEO_MAX));
    e.liberer_envoi_video();
    assert!(e.reserver_envoi_video(ENVOIS_VIDEO_MAX));
}

#[tokio::test]
async fn quotas_de_stockage() {
    let Some(e) = env(&[], |c| {
        c.max_storage_bytes = 40;
        c.max_user_storage_bytes = 30;
    })
    .await
    else {
        return;
    };
    e.user("alice").await;
    e.user("bob").await;
    let (s, v) = e
        .envoi("alice", &[], Some(("a.txt", &[b'a'; 20])), None)
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (s, v) = e
        .envoi("alice", &[], Some(("b.txt", &[b'a'; 11])), None)
        .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::PAYLOAD_TOO_LARGE, Some("quota_exceeded"))
    );
    let (s, _) = e
        .envoi("bob", &[], Some(("c.txt", &[b'a'; 20])), None)
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, v) = e.envoi("bob", &[], Some(("d.txt", &[b'a'; 1])), None).await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::INSUFFICIENT_STORAGE, Some("storage_full"))
    );
    let restants = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM files")
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(restants, 2);
}

#[tokio::test]
async fn debit_par_heure_et_par_jour() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let alice = e.user("alice").await;
    e.envoi_ok("alice", "a.txt", &[b'a'; 20]).await;
    let config = e.state.config();
    let code = |r: Result<(), Response>| r.err().map(|r| r.status());
    assert_eq!(
        code(controle_quota(e.pool(), config, &alice, 5, 25, 1000).await),
        None
    );
    assert_eq!(
        code(controle_quota(e.pool(), config, &alice, 6, 25, 1000).await),
        Some(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(
        code(controle_quota(e.pool(), config, &alice, 6, 1000, 25).await),
        Some(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(
        code(controle_quota(e.pool(), config, &alice, 6, 1000, 26).await),
        None
    );
}

#[tokio::test]
async fn fichiers_en_attente_et_purge() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let alice = e.user("alice").await;
    let mut ids = vec![];
    for i in 0..FICHIERS_EN_ATTENTE_MAX {
        ids.push(e.envoi_ok("alice", &format!("{i}.txt"), b"x").await);
    }
    let (s, v) = e
        .envoi("alice", &[], Some(("de-trop.txt", b"x")), None)
        .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (
            StatusCode::TOO_MANY_REQUESTS,
            Some("too_many_pending_files")
        )
    );
    let (_, salon) = e.serveur(&alice, &[]).await;
    e.joindre(&salon, &alice, &ids[0]).await;
    let signalement = e
            .sql("INSERT INTO reports (author_id, title, description) VALUES ($1::uuid, 't', 'd') RETURNING id::text", &[&alice])
            .await;
    sqlx::query("UPDATE files SET report_id=$1::uuid WHERE id=$2::uuid")
        .bind(&signalement)
        .bind(&ids[1])
        .execute(e.pool())
        .await
        .unwrap();
    let disque = Disque::new(&e.state.config().data_dir);
    assert_eq!(purger(e.pool(), &disque, AGE_ORPHELINS).await.unwrap(), 0);
    let cle: String = e
        .sql("SELECT key FROM files WHERE id=$1::uuid", &[&ids[5]])
        .await;
    disque.ouvrir(&cle).await.unwrap();
    assert_eq!(
        purger(e.pool(), &disque, Duration::ZERO).await.unwrap(),
        FICHIERS_EN_ATTENTE_MAX as usize - 2
    );
    assert!(matches!(
        disque.ouvrir(&cle).await,
        Err(ErreurBlob::Introuvable)
    ));
    let restants: Vec<String> =
        sqlx::query_scalar("SELECT id::text FROM files ORDER BY position, id")
            .fetch_all(e.pool())
            .await
            .unwrap();
    assert_eq!(restants.len(), 2);
    assert!(restants.contains(&ids[0]) && restants.contains(&ids[1]));
    assert_eq!(purger(e.pool(), &disque, Duration::ZERO).await.unwrap(), 0);
}

#[tokio::test]
async fn purge_periodique_et_arret_propre() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    e.envoi_ok("alice", "a.txt", b"x").await;
    let (stop, recu) = tokio::sync::watch::channel(false);
    let tache = lancer_purge(
        e.state.clone(),
        Duration::from_millis(20),
        Duration::ZERO,
        recu,
    );
    let limite = tokio::time::Instant::now() + Duration::from_secs(10);
    while sqlx::query_scalar::<_, i64>("SELECT count(*) FROM files")
        .fetch_one(e.pool())
        .await
        .unwrap()
        > 0
    {
        assert!(tokio::time::Instant::now() < limite, "purge non exécutée");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), tache)
        .await
        .expect("arrêt propre")
        .unwrap();
}

#[tokio::test]
async fn fichier_supprime_ou_absent() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    e.user("alice").await;
    let id = e.envoi_ok("alice", "a.txt", b"x").await;
    let chemin = format!("/files/{id}");
    let cle = e
        .sql("SELECT key FROM files WHERE id=$1::uuid", &[&id])
        .await;
    Disque::new(&e.state.config().data_dir)
        .supprimer(&cle)
        .await
        .unwrap();
    let (s, _, c) = e.lire(Some("alice"), &chemin).await;
    assert_eq!(s, StatusCode::GONE);
    assert!(texte(&c).contains("fichier introuvable sur le serveur"));
    sqlx::query("UPDATE files SET purged_at=now() WHERE id=$1::uuid")
        .bind(&id)
        .execute(e.pool())
        .await
        .unwrap();
    let (s, _, c) = e.lire(Some("alice"), &chemin).await;
    assert_eq!(s, StatusCode::GONE);
    assert!(texte(&c).contains("fichier supprimé du serveur"));
}

#[tokio::test]
async fn capture_de_signalement_pour_les_admins() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let alice = e.user("alice").await;
    e.user("admin").await;
    let id = e.envoi_ok("alice", "a.txt", b"capture").await;
    let chemin = format!("/files/{id}");
    assert_eq!(
        e.lire(Some("admin"), &chemin).await.0,
        StatusCode::NOT_FOUND
    );
    let signalement = e
            .sql("INSERT INTO reports (author_id, title, description) VALUES ($1::uuid, 't', 'd') RETURNING id::text", &[&alice])
            .await;
    sqlx::query("UPDATE files SET report_id=$1::uuid WHERE id=$2::uuid")
        .bind(&signalement)
        .bind(&id)
        .execute(e.pool())
        .await
        .unwrap();
    e.user("bob").await;
    assert_eq!(e.lire(Some("bob"), &chemin).await.0, StatusCode::NOT_FOUND);
    let (s, _, c) = e.lire(Some("admin"), &chemin).await;
    assert_eq!((s, texte(&c).as_str()), (StatusCode::OK, "capture"));
}

#[test]
fn noms_nettoyes() {
    for (entree, attendu) in [
        ("photo.png", "photo.png"),
        ("../../etc/passwd", "passwd"),
        ("C:\\dossier\\fichier.txt", "fichier.txt"),
        ("a/b/", "b"),
        ("a\nb\u{7f}c", "abc"),
        ("", "fichier"),
        ("/", "fichier"),
        ("///", "fichier"),
        (".", "fichier"),
        ("..", ".."),
        ("été.txt", "été.txt"),
    ] {
        assert_eq!(nom_propre(entree), attendu, "{entree:?}");
    }
    let long = format!("{}fin", "a".repeat(200));
    let propre = nom_propre(&long);
    assert_eq!(propre.chars().count(), 100);
    assert!(propre.ends_with("fin"));
    assert_eq!(nom_ascii("été \"q\" \\ ok"), "_t_ _q_ _ ok");
    assert_eq!(
        echappe_chemin("été (1) a+b.txt"),
        "%C3%A9t%C3%A9%20%281%29%20a+b.txt"
    );
    assert_eq!(extension("a.PNG"), ".png");
    assert_eq!(extension("sans"), "");
    assert_eq!(extension("a.b.jpeg"), ".jpeg");
}

#[test]
fn plages_comme_le_go() {
    use Plage::*;
    let lire = |h: &str| match analyser_plage(h, 10) {
        Aucune => "aucune".to_string(),
        Une(d, l) => format!("{d}+{l}"),
        NonSatisfaisante => "hors".into(),
        Invalide => "invalide".into(),
    };
    for (entete, attendu) in [
        ("", "aucune"),
        ("bytes=0-0", "0+1"),
        ("bytes=0-", "0+10"),
        ("bytes=0-99", "0+10"),
        ("bytes=-4", "6+4"),
        ("bytes=-99", "0+10"),
        ("bytes=-0", "hors"),
        ("bytes=-0,2-3", "2+2"),
        ("bytes= 2 - 3 ", "2+2"),
        ("bytes=10-", "hors"),
        ("bytes=10-12", "hors"),
        ("bytes=5-2", "invalide"),
        ("bytes=a-b", "invalide"),
        ("bytes=-", "invalide"),
        ("bytes=--1", "invalide"),
        ("bytes=3", "invalide"),
        ("items=0-1", "invalide"),
        ("bytes=0-1,4-5", "aucune"),
        ("bytes=0-1,20-30", "0+2"),
        ("bytes=", "aucune"),
        ("bytes=,", "aucune"),
    ] {
        assert_eq!(lire(entete), attendu, "{entete:?}");
    }
    for entete in ["bytes=0-", "bytes=-1", "bytes=-0", "bytes=0-0"] {
        assert!(
            matches!(analyser_plage(entete, 0), NonSatisfaisante),
            "{entete:?}"
        );
    }
}

fn cle_aleatoire() -> [u8; 32] {
    use std::hash::BuildHasher;
    let graine = std::collections::hash_map::RandomState::new();
    std::array::from_fn(|i| graine.hash_one(i) as u8)
}

#[test]
fn liens_et_identifiants() {
    let cle = cle_aleatoire();
    let id = "123e4567-e89b-12d3-a456-426614174000";
    let user = "00000000-0000-0000-0000-0000000000aa";
    let jeton = signer(&cle, id, user, 2000);
    assert_eq!(lien_valide(&cle, id, &jeton, 1999).as_deref(), Some(user));
    assert_eq!(lien_valide(&cle, id, &jeton, 2000), None);
    assert_eq!(lien_valide(&cle, "autre", &jeton, 1999), None);
    assert_eq!(lien_valide(&cle_aleatoire(), id, &jeton, 1999), None);
    let autre = jeton.replace(user, "00000000-0000-0000-0000-0000000000bb");
    let ancien = format!("2000.{}", jeton.rsplit('.').next().unwrap());
    for mauvais in [
        "",
        "x",
        "2000.",
        "+2000.ab",
        "-1.ab",
        &format!("{jeton}0"),
        &autre,
        &ancien,
    ] {
        assert_eq!(lien_valide(&cle, id, mauvais, 1999), None, "{mauvais}");
    }
    assert!(uuid_valide(id) && uuid_valide(&id.to_uppercase()));
    for mauvais in [
        "",
        "pas-un-uuid",
        &id[1..],
        &format!("{id}0"),
        &id.replace('-', "_"),
    ] {
        assert!(!uuid_valide(mauvais), "{mauvais}");
    }
    assert_eq!(parametre(Some("a=1&t=x%2By+z&t=2"), "t"), "x+y z");
    assert_eq!(parametre(Some("download=1"), "t"), "");
    assert_eq!(parametre(None, "t"), "");
    assert_eq!(parametre(Some("t"), "t"), "");
}

#[tokio::test]
async fn importer_fichier_cree_un_fichier_en_attente() {
    let Some(e) = env(&[], |_| {}).await else {
        return;
    };
    let u = users::activate(e.pool(), "sub-alice", "alice")
        .await
        .ok()
        .unwrap();
    let rep = importer_fichier(
        &e.state,
        &u,
        "gif.gif".into(),
        b"GIF89a\x01\0\x01\0\0\0\0;".to_vec(),
    )
    .await;
    assert_eq!(rep.status(), StatusCode::CREATED);
    let v: Value =
        serde_json::from_slice(&to_bytes(rep.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(v["mime"], "image/gif");
    assert_eq!(v["name"], "gif.gif");
    assert_eq!(v["kind"], "file");
    let rep = importer_fichier(&e.state, &u, "gif.gif".into(), b"pas un gif".to_vec()).await;
    assert_eq!(rep.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}
