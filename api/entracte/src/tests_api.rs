use std::{
    collections::HashMap,
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use crate::{
    config::{Arr, Config, ModeMateriel, correspondances},
    droits::Droit,
    etat::AppState,
    garde::Garde,
    routes,
    testutil::{BasePostgres, Faux, Requete, faux_avec_entetes, postgres},
};

const RADARR_ID: i64 = 987_654;
const SONARR_ID: i64 = 876_543;

fn ffmpeg_present() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn generer(dest: &Path, args: &[&str]) {
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let ok = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(dest)
        .status()
        .unwrap()
        .success();
    assert!(ok, "génération de {}", dest.display());
}

fn medias() -> &'static Path {
    static DOSSIER: OnceLock<tempfile::TempDir> = OnceLock::new();
    DOSSIER
        .get_or_init(|| {
            let d = tempfile::tempdir().unwrap();
            let r = d.path();
            let srt = r.join("sous-titres.srt");
            std::fs::write(
                &srt,
                "1\n00:00:00,500 --> 00:00:03,000\nBonjour <script>x</script>tout le monde\n",
            )
            .unwrap();
            let srt = srt.to_string_lossy().to_string();
            let video = |taille: &str, duree: &str| {
                vec![
                    "-f".to_string(),
                    "lavfi".into(),
                    "-i".into(),
                    format!("testsrc=size={taille}:rate=24:duration={duree}"),
                    "-f".into(),
                    "lavfi".into(),
                    "-i".into(),
                    format!("sine=frequency=440:duration={duree}"),
                ]
            };
            let mut a = video("1280x720", "8");
            a.extend(
                [
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-c:a",
                    "aac",
                    "-shortest",
                ]
                .map(String::from),
            );
            generer(
                &r.join("films/Film A (2020)/Film.A.mp4"),
                &a.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            std::fs::write(
                r.join("films/Film A (2020)/Film.A.fr.srt"),
                "1\n00:00:01,000 --> 00:00:02,000\nExterne\n",
            )
            .unwrap();
            std::fs::create_dir_all(r.join("films/Film B")).unwrap();
            std::fs::write(r.join("films/Film B/Film.B.exe.mkv"), b"pas une video").unwrap();
            let mut c = video("640x360", "8");
            c.extend(
                [
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=880:duration=8",
                    "-i",
                    &srt,
                ]
                .map(String::from),
            );
            c.extend(
                [
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:a",
                    "-map",
                    "3:s",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-c:a",
                    "aac",
                    "-c:s",
                    "srt",
                    "-metadata:s:a:0",
                    "language=fre",
                    "-metadata:s:a:1",
                    "language=eng",
                    "-metadata:s:s:0",
                    "language=fre",
                    "-shortest",
                ]
                .map(String::from),
            );
            generer(
                &r.join("films/Film C/Film.C.mkv"),
                &c.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            for n in 1..=2 {
                let mut e = video("640x360", "4");
                e.extend(
                    [
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-c:a",
                        "aac",
                        "-shortest",
                    ]
                    .map(String::from),
                );
                generer(
                    &r.join(format!("series/La Serie/S01E0{n}.mkv")),
                    &e.iter().map(String::as_str).collect::<Vec<_>>(),
                );
            }
            d
        })
        .path()
}

#[derive(Default)]
struct Monde {
    films: Vec<Value>,
    radarr_en_panne: bool,
    radarr_ajouts_refuses: bool,
    episodes: Vec<(i64, i64, bool)>,
    sonarr_en_panne: bool,
}

struct Env {
    _base: BasePostgres,
    app: Router,
    state: AppState,
    monde: Arc<Mutex<Monde>>,
    droits: Arc<Mutex<HashMap<String, Vec<&'static str>>>>,
    tmdb_en_panne: Arc<AtomicBool>,
    radarr: Faux,
    sonarr: Faux,
    tmdb: Faux,
    _gateway: Faux,
    _aegis: Faux,
    _transcodage: tempfile::TempDir,
}

fn film_radarr(tmdb: i64, titre: &str, chemin: Option<&str>) -> Value {
    let mut v = json!({ "id": RADARR_ID + tmdb, "tmdbId": tmdb, "title": titre, "year": 2020 });
    if let Some(c) = chemin {
        v["movieFile"] = json!({ "id": 5, "path": c, "size": 1000 });
    }
    v
}

fn reponse_tmdb(r: &Requete, panne: &AtomicBool) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    if panne.load(Ordering::SeqCst) {
        return (503, vec![], vec![]);
    }
    let corps = |v: Value| {
        (
            200,
            vec![("content-type", "application/json".to_string())],
            v.to_string().into_bytes(),
        )
    };
    if let Some(nom) = r.chemin.strip_prefix("/t/p/w342/") {
        return (
            200,
            vec![("content-type", "image/jpeg".to_string())],
            format!("IMG-{nom}").into_bytes(),
        );
    }
    match r.chemin.as_str() {
        "/3/movie/101" => corps(
            json!({"title": "Film A", "release_date": "2020-01-01", "overview": "Un film", "genres": [{"name": "Drame"}], "poster_path": "/a101.jpg", "backdrop_path": "/f101.jpg", "runtime": 90}),
        ),
        "/3/movie/103" => corps(
            json!({"title": "Film C", "release_date": "2022-03-03", "genres": [{"name": "Comédie"}], "poster_path": "/a103.jpg"}),
        ),
        "/3/tv/201" => corps(
            json!({"name": "La Série", "first_air_date": "2019-01-01", "poster_path": "/s201.jpg", "genres": [{"name": "Drame"}],
            "seasons": [{"season_number": 0, "name": "Bonus", "episode_count": 1}, {"season_number": 1, "name": "Saison 1", "episode_count": 2}, {"season_number": 2, "name": "Saison 2", "episode_count": 3}],
            "external_ids": {"tvdb_id": 3201}}),
        ),
        "/3/tv/201/season/1" => corps(
            json!({"episodes": [{"episode_number": 1, "name": "Pilote", "runtime": 40}, {"episode_number": 2, "name": "Suite", "still_path": "/e2.jpg"}]}),
        ),
        "/search/shows" => corps(json!([{"score": 0.9, "show": {"name": "Daredevil"}}])),
        "/3/search/multi" if r.query.contains("query=dardevil") => corps(json!({"results": [
            {"media_type": "movie", "id": 900, "title": "Dark Devil", "popularity": 1.0}]})),
        "/3/search/multi" if r.query.contains("query=Daredevil") => corps(json!({"results": [
            {"media_type": "tv", "id": 61889, "name": "Marvel's Daredevil", "original_name": "Marvel's Daredevil", "popularity": 90.0},
            {"media_type": "movie", "id": 9480, "title": "Daredevil", "release_date": "2003-02-14", "popularity": 20.0},
            {"media_type": "movie", "id": 901, "title": "Sans rapport", "popularity": 99.0}]})),
        "/3/search/multi" => corps(json!({"results": [
            {"media_type": "movie", "id": 101, "title": "Film A", "release_date": "2020-01-01", "poster_path": "/a101.jpg"},
            {"media_type": "movie", "id": 103, "title": "Film C", "release_date": "2022-03-03", "poster_path": "/a103.jpg"},
            {"media_type": "tv", "id": 201, "name": "La Série", "first_air_date": "2019-01-01"},
            {"media_type": "person", "id": 9, "name": "Quelqu'un"}]})),
        _ => (404, vec![], vec![]),
    }
}

fn reponse_radarr(r: &Requete, m: &Mutex<Monde>) -> (u16, Vec<u8>) {
    let m = m.lock().unwrap();
    if m.radarr_en_panne {
        return (500, vec![]);
    }
    let corps = |v: Value| (200, v.to_string().into_bytes());
    match (r.methode.as_str(), r.chemin.as_str()) {
        ("GET", "/api/v3/movie") => corps(json!(m.films)),
        ("GET", "/api/v3/movie/lookup/tmdb") => {
            corps(json!({"title": "Film C", "tmdbId": 103, "year": 2022}))
        }
        ("GET", "/api/v3/qualityprofile") => {
            corps(json!([{"id": 7, "name": "HD-1080p"}, {"id": 9, "name": "Ultra-HD"}]))
        }
        ("GET", "/api/v3/rootfolder") => corps(
            json!([{"path": "/movies", "freeSpace": 5000}, {"path": "/movies-4k", "freeSpace": 90}]),
        ),
        ("GET", "/api/v3/tag") => corps(json!([{"id": 2, "label": "vf"}])),
        ("POST", "/api/v3/movie") if m.radarr_ajouts_refuses => (400, b"refus".to_vec()),
        ("POST", "/api/v3/movie") => (201, b"{}".to_vec()),
        ("GET", p) if p.starts_with("/api/v3/movie/") => match m
            .films
            .iter()
            .find(|f| format!("/api/v3/movie/{}", f["id"]) == p)
        {
            Some(f) => corps(f.clone()),
            None => (404, vec![]),
        },
        ("PUT", _) | ("POST", "/api/v3/command") => (200, b"{}".to_vec()),
        _ => (404, vec![]),
    }
}

fn reponse_sonarr(r: &Requete, m: &Mutex<Monde>) -> (u16, Vec<u8>) {
    let m = m.lock().unwrap();
    if m.sonarr_en_panne {
        return (500, vec![]);
    }
    let corps = |v: Value| (200, v.to_string().into_bytes());
    let serie = json!({"id": SONARR_ID, "tmdbId": 201, "tvdbId": 3201, "title": "La Serie", "year": 2019,
        "seasons": [{"seasonNumber": 1, "monitored": true}, {"seasonNumber": 2, "monitored": false}]});
    match (r.methode.as_str(), r.chemin.as_str()) {
        ("GET", "/api/v3/series") => corps(json!([serie])),
        ("GET", p) if p == format!("/api/v3/series/{SONARR_ID}") => corps(serie),
        ("PUT", _) | ("POST", "/api/v3/command") => (200, b"{}".to_vec()),
        ("GET", "/api/v3/episode") => corps(json!(m
            .episodes
            .iter()
            .map(|(s, e, f)| json!({"seasonNumber": s, "episodeNumber": e, "hasFile": f, "episodeFileId": s * 100 + e, "title": format!("Ep {e}")}))
            .collect::<Vec<_>>())),
        ("GET", "/api/v3/episodefile") => corps(json!(m
            .episodes
            .iter()
            .filter(|(_, _, f)| *f)
            .map(|(s, e, _)| json!({"id": s * 100 + e, "path": format!("/tv/La Serie/S0{s}E0{e}.mkv"), "size": 10}))
            .collect::<Vec<_>>())),
        _ => (404, vec![]),
    }
}

async fn env_avec(regler: impl FnOnce(&mut Config)) -> Option<Env> {
    if !ffmpeg_present() {
        eprintln!("ignoré : ffmpeg absent");
        return None;
    }
    let medias = medias();
    let (base, url) = postgres().await?;
    let monde = Arc::new(Mutex::new(Monde {
        films: vec![
            film_radarr(101, "Film A", Some("/movies/Film A (2020)/Film.A.mp4")),
            film_radarr(102, "Film B", Some("/movies/Film B/Film.B.exe.mkv")),
        ],
        episodes: vec![(1, 1, true), (1, 2, true), (2, 1, false)],
        ..Monde::default()
    }));
    let droits: Arc<Mutex<HashMap<String, Vec<&'static str>>>> =
        Arc::new(Mutex::new(HashMap::from([
            ("sub-alice".to_string(), vec!["access", "request"]),
            ("sub-bob".to_string(), vec!["access", "request"]),
            ("sub-carole".to_string(), vec!["access"]),
            ("sub-admin".to_string(), vec!["access", "request", "admin"]),
        ])));
    let gateway = faux_avec_entetes(|r| {
        let qui = r
            .entete("authorization")
            .strip_prefix("Bearer ")
            .unwrap_or_default()
            .to_string();
        let qui = if qui.is_empty() {
            r.entete("cookie")
                .split("plbls_sid=")
                .nth(1)
                .unwrap_or_default()
                .to_string()
        } else {
            qui
        };
        if qui.is_empty() || qui == "invalide" {
            return (401, vec![], vec![]);
        }
        (
            200,
            vec![],
            json!({"id": format!("sub-{qui}"), "name": qui, "email": format!("{qui}@x.fr")})
                .to_string()
                .into_bytes(),
        )
    })
    .await;
    let d = droits.clone();
    let aegis = faux_avec_entetes(move |r| {
        if r.chemin == "/v1/manifest" {
            return (200, vec![], b"{}".to_vec());
        }
        let sub = r.json()["sub"].as_str().unwrap_or_default().to_string();
        let perms: serde_json::Map<String, Value> = d
            .lock()
            .unwrap()
            .get(&sub)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|p| (format!("entracte:{p}"), json!({"allow": ["*"], "deny": []})))
            .collect();
        (
            200,
            vec![],
            json!({"sub": sub, "permissions": perms})
                .to_string()
                .into_bytes(),
        )
    })
    .await;
    let panne = Arc::new(AtomicBool::new(false));
    let p = panne.clone();
    let tmdb = faux_avec_entetes(move |r| reponse_tmdb(r, &p)).await;
    let m = monde.clone();
    let radarr = faux_avec_entetes(move |r| {
        let (s, c) = reponse_radarr(r, &m);
        (s, vec![], c)
    })
    .await;
    let m = monde.clone();
    let sonarr = faux_avec_entetes(move |r| {
        let (s, c) = reponse_sonarr(r, &m);
        (s, vec![], c)
    })
    .await;
    let transcodage = tempfile::tempdir().unwrap();
    let mut cfg = Config::pour_tests(&url);
    cfg.gateway_url = gateway.url.clone();
    cfg.aegis = Some((aegis.url.clone(), "jeton-aegis".into()));
    cfg.origines = vec!["https://estompe.plbls.fr".into()];
    cfg.radarr = Some(Arr {
        url: radarr.url.clone(),
        cle: "cle-radarr".into(),
        profil: None,
        racine: None,
    });
    cfg.sonarr = Some(Arr {
        url: sonarr.url.clone(),
        cle: "cle-sonarr".into(),
        profil: Some(3),
        racine: Some("/tv".into()),
    });
    cfg.tmdb_cle = "cle-tmdb".into();
    cfg.tmdb_url = tmdb.url.clone();
    cfg.tmdb_images = tmdb.url.clone();
    cfg.tvmaze_url = tmdb.url.clone();
    cfg.chemins = correspondances(&format!(
        "/movies={},/tv={}",
        medias.join("films").display(),
        medias.join("series").display()
    ))
    .unwrap();
    cfg.medias = medias.to_path_buf();
    cfg.dossier_transcodage = transcodage.path().join("t");
    cfg.jeton_service = "jeton-service-estompe".into();
    cfg.jeton_webhook = "jeton-webhook".into();
    regler(&mut cfg);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    crate::MIGRATIONS.run(&pool).await.unwrap();
    let state = AppState::new(cfg, pool);
    state.transcodeur().preparer().await;
    Some(Env {
        _base: base,
        app: routes::routeur(state.clone()),
        state,
        monde,
        droits,
        tmdb_en_panne: panne,
        radarr,
        sonarr,
        tmdb,
        _gateway: gateway,
        _aegis: aegis,
        _transcodage: transcodage,
    })
}

async fn env() -> Option<Env> {
    env_avec(|_| {}).await
}

struct Rep {
    statut: StatusCode,
    entetes: HeaderMap,
    corps: Vec<u8>,
}

impl Rep {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.corps).unwrap_or(Value::Null)
    }

    fn texte(&self) -> String {
        String::from_utf8_lossy(&self.corps).into_owned()
    }
}

async fn requete(app: &Router, req: Request<Body>) -> Rep {
    let r = app.clone().oneshot(req).await.unwrap();
    let statut = r.status();
    let entetes = r.headers().clone();
    let corps = to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec();
    Rep {
        statut,
        entetes,
        corps,
    }
}

async fn appel(
    e: &Env,
    qui: Option<&str>,
    methode: &str,
    chemin: &str,
    corps: Option<Value>,
) -> Rep {
    appel_avec(e, qui, methode, chemin, corps, &[]).await
}

async fn appel_avec(
    e: &Env,
    qui: Option<&str>,
    methode: &str,
    chemin: &str,
    corps: Option<Value>,
    entetes: &[(&str, &str)],
) -> Rep {
    let mut b = Request::builder().method(methode).uri(chemin);
    if let Some(q) = qui {
        b = b.header("authorization", format!("Bearer {q}"));
    }
    for (k, v) in entetes {
        b = b.header(*k, *v);
    }
    let corps = match corps {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    requete(&e.app, b.body(corps).unwrap()).await
}

async fn get(e: &Env, qui: &str, chemin: &str) -> Rep {
    appel(e, Some(qui), "GET", chemin, None).await
}

async fn synchro(e: &Env) {
    crate::catalogue::synchroniser(&e.state).await;
}

fn sans_fuite(e: &Env, texte: &str, contexte: &str) {
    let medias = medias().to_string_lossy().to_string();
    for interdit in [
        medias.as_str(),
        "/movies/",
        "/tv/",
        &RADARR_ID.to_string(),
        &(RADARR_ID + 101).to_string(),
        &SONARR_ID.to_string(),
        "3201",
        e.tmdb.url.as_str(),
        "image.tmdb.org",
        "themoviedb",
        e.radarr.url.as_str(),
        e.sonarr.url.as_str(),
        "cle-tmdb",
    ] {
        assert!(
            !texte.contains(interdit),
            "{contexte} : « {interdit} » dans {texte}"
        );
    }
}

async fn id_de(e: &Env, titre: &str) -> String {
    let l = get(e, "alice", "/library?limit=100").await.json();
    l["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == titre)
        .unwrap_or_else(|| panic!("{titre} absent de {l}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn attendre(mut condition: impl FnMut() -> bool, quoi: &str) {
    let limite = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < limite, "délai dépassé : {quoi}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn exemple(chemin: &str) -> String {
    let id = "00000000-0000-0000-0000-000000000001";
    chemin
        .replace("{id}", id)
        .replace("{n}", "1")
        .replace("{tmdb_id}", "201")
        .replace("{nom}", "a.jpg")
        .replace("{jeton}", "x.1.y")
        .replace("{segment}", "seg00000.ts")
}

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let Some(e) = env().await else { return };
    e.droits
        .lock()
        .unwrap()
        .insert("sub-personne".into(), vec![]);
    for (route, _) in routes::table() {
        let chemin = exemple(route.chemin);
        let m = route.methode.as_str();
        let corps = (m != "GET" && m != "DELETE").then(|| json!({}));
        match route.garde {
            Garde::Droit(d) => {
                let r = appel(&e, None, m, &chemin, corps.clone()).await;
                assert_eq!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} sans session"
                );
                let r = appel(&e, Some("personne"), m, &chemin, corps.clone()).await;
                assert_eq!(
                    r.statut,
                    StatusCode::FORBIDDEN,
                    "{m} {chemin} sans permission"
                );
                assert!(r.texte().contains(&d.nom()), "{m} {chemin} : {}", r.texte());
                let sous = match d {
                    Droit::Acces => None,
                    Droit::Demande => Some("carole"),
                    Droit::Admin => Some("alice"),
                };
                if let Some(q) = sous {
                    let r = appel(&e, Some(q), m, &chemin, corps.clone()).await;
                    assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour {q}");
                }
            }
            Garde::Session(_) => {
                assert_eq!(
                    appel(&e, None, m, &chemin, None).await.statut,
                    StatusCode::UNAUTHORIZED
                );
                let r = appel(&e, Some("personne"), m, &chemin, None).await;
                assert_eq!(r.statut, StatusCode::OK);
                assert_eq!(
                    r.json()["permissions"],
                    json!({"access": false, "request": false, "admin": false})
                );
            }
            Garde::Service(_) | Garde::Webhook(_) => {
                assert_eq!(
                    appel(&e, Some("admin"), m, &chemin, corps.clone())
                        .await
                        .statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
            }
            Garde::Jeton(_) => {
                assert_eq!(
                    appel(&e, Some("admin"), m, &chemin, None).await.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
            }
            Garde::Public(_) => {
                assert!(
                    appel(&e, None, m, &chemin, None).await.statut.is_success(),
                    "{m} {chemin}"
                );
            }
        }
    }
    let r = get(&e, "invalide", "/library").await;
    assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
    let r = appel(&e, Some("alice"), "GET", "/me", None).await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": true, "request": true, "admin": false})
    );
}

#[tokio::test]
async fn origine_et_cors() {
    let Some(e) = env().await else { return };
    let r = appel_avec(
        &e,
        None,
        "PUT",
        "/titles/x/progress",
        Some(json!({})),
        &[
            ("cookie", "__Secure-plbls_sid=alice"),
            ("origin", "https://ailleurs.fr"),
        ],
    )
    .await;
    assert_eq!(
        (r.statut, r.json()["error"].clone()),
        (StatusCode::FORBIDDEN, json!("bad_origin"))
    );
    let r = appel_avec(
        &e,
        None,
        "OPTIONS",
        "/library",
        None,
        &[("origin", "https://estompe.plbls.fr")],
    )
    .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    assert_eq!(
        r.entetes["access-control-allow-origin"],
        "https://estompe.plbls.fr"
    );
    let r = appel_avec(
        &e,
        None,
        "GET",
        "/library",
        None,
        &[
            ("cookie", "__Secure-plbls_sid=alice"),
            ("origin", "https://estompe.plbls.fr"),
        ],
    )
    .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.entetes["access-control-allow-credentials"], "true");
    let tele = [("origin", "https://www.gstatic.com")];
    let r = appel_avec(&e, None, "GET", "/stream/x/hls.m3u8", None, &tele).await;
    assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
    assert_eq!(r.entetes["access-control-allow-origin"], "*");
    assert!(!r.entetes.contains_key("access-control-allow-credentials"));
    let r = appel_avec(&e, None, "OPTIONS", "/stream/x/direct", None, &tele).await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    assert_eq!(r.entetes["access-control-allow-origin"], "*");
    assert_eq!(r.entetes["access-control-allow-headers"], "Range");
    let r = appel_avec(&e, Some("alice"), "GET", "/library", None, &tele).await;
    assert_eq!(r.statut, StatusCode::OK);
    assert!(!r.entetes.contains_key("access-control-allow-origin"));
    let r = appel_avec(&e, None, "DELETE", "/streams/x", None, &tele).await;
    assert!(!r.entetes.contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn catalogue_synchronise_filtre_et_garde_les_titres_absents() {
    let Some(e) = env().await else { return };
    synchro(&e).await;
    let l = get(&e, "alice", "/library").await;
    assert_eq!(l.statut, StatusCode::OK);
    let v = l.json();
    let titres_possedes: Vec<String> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_string())
        .collect();
    assert!(
        titres_possedes.contains(&"Film A".to_string())
            && titres_possedes.contains(&"La Série".to_string()),
        "{v}"
    );
    assert!(!titres_possedes.contains(&"Film B".to_string()), "{v}");
    assert_eq!(v["total"], 2);
    sans_fuite(&e, &l.texte(), "/library");
    assert_eq!(
        get(&e, "alice", "/library?kind=movie").await.json()["total"],
        1
    );
    assert_eq!(
        get(&e, "alice", "/library?q=s%C3%A9rie").await.json()["total"],
        1
    );
    assert_eq!(
        get(&e, "alice", "/library?genre=Com%C3%A9die").await.json()["total"],
        0
    );
    assert_eq!(
        get(&e, "alice", "/library?year=2019-2019").await.json()["total"],
        1
    );
    assert_eq!(
        get(&e, "alice", "/library?sort=nope").await.statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(&e, "alice", "/genres").await.json()["genres"],
        json!(["Drame"])
    );
    let ecartes = get(&e, "admin", "/admin/rejected").await;
    assert_eq!(ecartes.json()["items"][0]["name"], "Film.B.exe.mkv");
    assert_eq!(ecartes.json()["items"][0]["reason"], "double_extension");
    sans_fuite(&e, &ecartes.texte(), "/admin/rejected");

    let film = id_de(&e, "Film A").await;
    let serie = id_de(&e, "La Série").await;
    let d = get(&e, "alice", &format!("/titles/{film}")).await;
    let dj = d.json();
    assert_eq!(dj["duration"].as_f64().map(|x| x.round()), Some(8.0));
    assert_eq!(dj["qualities"], json!(["original", "480p"]));
    assert_eq!(dj["subtitles"][0]["label"], "fr (externe)");
    sans_fuite(&e, &d.texte(), "/titles/{id}");
    let saisons = get(&e, "alice", &format!("/titles/{serie}/seasons")).await;
    let sj = saisons.json();
    assert_eq!(sj["items"].as_array().unwrap().len(), 2, "{sj}");
    assert_eq!(sj["items"][0]["available_count"], 2);
    assert_eq!(sj["items"][1]["available"], false);
    let ep = get(&e, "alice", &format!("/titles/{serie}/seasons/1/episodes")).await;
    assert_eq!(ep.json()["items"][0]["title"], "Pilote");
    assert_eq!(ep.json()["items"][1]["title"], "Suite");
    assert_eq!(ep.json()["items"][0]["series_title"], "La Série");
    sans_fuite(&e, &ep.texte(), "episodes");
    let image = get(&e, "alice", &format!("/titles/{film}/image")).await;
    assert_eq!(
        (image.statut, image.texte()),
        (StatusCode::OK, "IMG-a101.jpg".to_string())
    );

    e.monde.lock().unwrap().films[0] = film_radarr(101, "Film A", None);
    synchro(&e).await;
    assert_eq!(get(&e, "alice", "/library").await.json()["total"], 1);
    let absent = get(&e, "alice", &format!("/titles/{film}")).await.json();
    assert_eq!(
        (
            absent["title"].clone(),
            absent["available"].clone(),
            absent["poster"].clone()
        ),
        (json!("Film A"), json!(false), json!(true))
    );
    e.tmdb_en_panne.store(true, Ordering::SeqCst);
    let image = get(&e, "alice", &format!("/titles/{film}/image")).await;
    assert_eq!(
        image.statut,
        StatusCode::OK,
        "affiche servie depuis le cache"
    );

    e.monde.lock().unwrap().radarr_en_panne = true;
    synchro(&e).await;
    assert_eq!(get(&e, "alice", "/library").await.json()["total"], 1);
    let etat = get(&e, "admin", "/admin/status").await;
    let sync = etat.json()["sync"].clone();
    let radarr = sync
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source"] == "radarr")
        .unwrap()
        .clone();
    assert!(radarr["error"].as_str().unwrap().contains("500"), "{sync}");
    assert!(radarr["succeeded_at"].is_string());
    sans_fuite(&e, &etat.texte(), "/admin/status");
}

#[tokio::test]
async fn medias_inaccessibles_rien_n_est_retire() {
    let Some(e) = env().await else { return };
    synchro(&e).await;
    assert_eq!(get(&e, "alice", "/library").await.json()["total"], 2);
    let mut cfg = e.state.config().clone();
    cfg.chemins =
        correspondances("/movies=/medias/absent/films,/tv=/medias/absent/series").unwrap();
    let demonte = AppState::new(cfg, e.state.pool().clone());
    crate::catalogue::synchroniser(&demonte).await;
    assert_eq!(get(&e, "alice", "/library").await.json()["total"], 2);
    let etat = get(&e, "admin", "/admin/status").await.json();
    for source in ["radarr", "sonarr"] {
        let s = etat["sync"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["source"] == source)
            .unwrap()
            .clone();
        assert!(
            s["error"].as_str().unwrap().contains("inaccessible"),
            "{source} : {etat}"
        );
    }
    assert!(
        get(&e, "admin", "/admin/rejected").await.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["reason"] != "fichier_inaccessible")
    );
}

#[tokio::test]
async fn synchronisation_demandee_pendant_une_autre_relancee() {
    let Some(e) = env().await else { return };
    let verrou = e.state.verrou_synchro().lock().await;
    crate::catalogue::synchroniser(&e.state).await;
    assert_eq!(get(&e, "alice", "/library").await.json()["total"], 0);
    assert!(e.state.synchro_relancee(), "la demande est retenue");
    drop(verrou);
}

async fn ouvrir(e: &Env, qui: &str, id: &str, corps: Value) -> Rep {
    appel(
        e,
        Some(qui),
        "POST",
        &format!("/titles/{id}/playback"),
        Some(corps),
    )
    .await
}

fn navigateur() -> Value {
    json!({"containers": ["mp4", "webm"], "video": ["h264", "vp9"], "audio": ["aac", "opus"]})
}

#[tokio::test]
async fn lecture_directe_hls_jetons_et_limites() {
    let Some(e) = env_avec(|c| {
        c.max_transcodages = 1;
        c.inactivite = Duration::from_secs(1);
    })
    .await
    else {
        return;
    };
    e.monde.lock().unwrap().films.push(film_radarr(
        103,
        "Film C",
        Some("/movies/Film C/Film.C.mkv"),
    ));
    synchro(&e).await;
    let a = id_de(&e, "Film A").await;
    let c = id_de(&e, "Film C").await;

    let r = ouvrir(&e, "alice", &a, json!({"support": navigateur()})).await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let o = r.json();
    assert_eq!(o["mode"], "direct");
    sans_fuite(&e, &r.texte(), "playback");
    let url = o["url"].as_str().unwrap().to_string();
    let plage = appel_avec(&e, None, "GET", &url, None, &[("range", "bytes=0-99")]).await;
    assert_eq!(plage.statut, StatusCode::PARTIAL_CONTENT);
    assert_eq!(plage.corps.len(), 100);
    assert!(
        plage.entetes["content-range"]
            .to_str()
            .unwrap()
            .starts_with("bytes 0-99/")
    );
    let tout = appel(&e, None, "GET", &url, None).await;
    assert_eq!(
        (tout.statut, tout.entetes["content-type"].to_str().unwrap()),
        (StatusCode::OK, "video/mp4")
    );
    assert_eq!(
        appel_avec(
            &e,
            None,
            "GET",
            &url,
            None,
            &[("range", "bytes=999999999-")]
        )
        .await
        .statut,
        StatusCode::RANGE_NOT_SATISFIABLE
    );
    assert_eq!(
        appel(&e, Some("bob"), "GET", &url, None).await.statut,
        StatusCode::FORBIDDEN
    );
    let falsifie = format!("{}x", url.trim_end_matches("/direct"));
    assert_eq!(
        appel(&e, None, "GET", &format!("{falsifie}/direct"), None)
            .await
            .statut,
        StatusCode::UNAUTHORIZED
    );
    let st = get(&e, "alice", o["subtitles"][0]["url"].as_str().unwrap()).await;
    assert!(
        st.texte().starts_with("WEBVTT") && st.texte().contains("Externe"),
        "{}",
        st.texte()
    );

    let r = ouvrir(
        &e,
        "alice",
        &c,
        json!({"support": navigateur(), "audio": 1}),
    )
    .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let h = r.json();
    assert_eq!(h["mode"], "hls");
    assert_eq!(h["audio"].as_array().unwrap().len(), 2);
    let flux_a = h["stream_id"].as_str().unwrap().to_string();
    let playlist = appel(&e, None, "GET", h["url"].as_str().unwrap(), None).await;
    assert_eq!(playlist.statut, StatusCode::OK, "{}", playlist.texte());
    assert!(
        playlist.texte().contains("seg00000.ts"),
        "{}",
        playlist.texte()
    );
    sans_fuite(&e, &playlist.texte(), "playlist");
    assert!(
        !playlist.texte().contains(
            &e.state
                .config()
                .dossier_transcodage
                .to_string_lossy()
                .to_string()
        )
    );
    let base = h["url"]
        .as_str()
        .unwrap()
        .trim_end_matches("hls.m3u8")
        .to_string();
    let seg = appel(&e, None, "GET", &format!("{base}seg00000.ts"), None).await;
    assert_eq!(
        (seg.statut, seg.entetes["content-type"].to_str().unwrap()),
        (StatusCode::OK, "video/mp2t")
    );
    assert!(seg.corps.len() > 1000);
    assert_eq!(
        appel(&e, None, "GET", &format!("{base}..%2Fx"), None)
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    let st = appel(
        &e,
        None,
        "GET",
        h["subtitles"][0]["url"].as_str().unwrap(),
        None,
    )
    .await;
    assert!(
        st.texte().contains("Bonjour") && !st.texte().contains("script"),
        "{}",
        st.texte()
    );

    let charge = ouvrir(&e, "bob", &c, json!({})).await;
    assert_eq!(
        (charge.statut, charge.json()["error"].clone()),
        (StatusCode::SERVICE_UNAVAILABLE, json!("serveur_charge"))
    );
    let pid = e.state.transcodeur().pid(&flux_a).unwrap();
    assert_eq!(
        appel(
            &e,
            Some("bob"),
            "DELETE",
            &format!("/streams/{flux_a}"),
            None
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        appel(
            &e,
            Some("alice"),
            "DELETE",
            &format!("/streams/{flux_a}"),
            None
        )
        .await
        .statut,
        StatusCode::NO_CONTENT
    );
    let dossier = e.state.config().dossier_transcodage.join(&flux_a);
    attendre(
        || {
            !Path::new(&format!("/proc/{pid}")).exists()
                || std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .is_ok_and(|s| s.contains(") Z "))
        },
        "ffmpeg arrêté",
    )
    .await;
    assert!(!dossier.exists());
    assert_eq!(
        appel(&e, None, "GET", h["url"].as_str().unwrap(), None)
            .await
            .statut,
        StatusCode::UNAUTHORIZED
    );

    let r = ouvrir(&e, "bob", &c, json!({"quality": "original"})).await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let flux_b = r.json()["stream_id"].as_str().unwrap().to_string();
    assert!(e.state.transcodeur().pid(&flux_b).is_some());
    let dossier_b = e.state.config().dossier_transcodage.join(&flux_b);
    let etat = e.state.clone();
    let limite = Instant::now() + Duration::from_secs(20);
    while etat.transcodeur().nombre() > 0 {
        assert!(
            Instant::now() < limite,
            "transcodage sans spectateur non arrêté"
        );
        crate::lecture::menage(&etat).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!dossier_b.exists());
    assert_eq!(
        appel(
            &e,
            Some("bob"),
            "DELETE",
            &format!("/streams/{flux_b}"),
            None
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn changer_de_palier_ou_de_piste_reprend_a_la_meme_position() {
    let Some(e) = env().await else { return };
    synchro(&e).await;
    let a = id_de(&e, "Film A").await;
    let r = ouvrir(
        &e,
        "alice",
        &a,
        json!({"support": navigateur(), "start": 2.5}),
    )
    .await
    .json();
    assert_eq!(
        (r["mode"].clone(), r["start"].clone(), r["offset"].clone()),
        (json!("direct"), json!(2.5), json!(0.0))
    );
    let ancien = r["url"].as_str().unwrap().to_string();
    let r2 = ouvrir(
        &e,
        "alice",
        &a,
        json!({"support": navigateur(), "start": 3.0, "quality": "480p"}),
    )
    .await;
    assert_eq!(r2.statut, StatusCode::OK, "{}", r2.texte());
    let r2 = r2.json();
    assert_eq!(
        (
            r2["mode"].clone(),
            r2["start"].clone(),
            r2["offset"].clone(),
            r2["quality"].clone()
        ),
        (json!("hls"), json!(3.0), json!(3.0), json!("480p"))
    );
    assert_eq!(
        appel(&e, None, "GET", &ancien, None).await.statut,
        StatusCode::UNAUTHORIZED,
        "l'ancien flux est remplacé"
    );
    assert_eq!(
        appel(&e, None, "GET", r2["url"].as_str().unwrap(), None)
            .await
            .statut,
        StatusCode::OK
    );
    let p = e
        .state
        .transcodeur()
        .parametres(r2["stream_id"].as_str().unwrap())
        .unwrap();
    assert_eq!((p.debut, p.hauteur, p.copie_video), (3.0, Some(480), false));
    assert_eq!(
        ouvrir(&e, "alice", &a, json!({"quality": "1080p"}))
            .await
            .statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        ouvrir(&e, "alice", &a, json!({"audio": 4})).await.statut,
        StatusCode::BAD_REQUEST
    );
    let r3 = ouvrir(
        &e,
        "alice",
        &a,
        json!({"support": {"containers": [], "video": ["h264"], "audio": []}, "start": 4.0}),
    )
    .await
    .json();
    let p = e
        .state
        .transcodeur()
        .parametres(r3["stream_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        (r3["mode"].clone(), p.copie_video, p.debut),
        (json!("hls"), true, 0.0),
        "une copie de la vidéo part de l'image clé qui précède la position demandée"
    );
    assert_eq!(
        (r3["start"].clone(), r3["offset"].clone()),
        (json!(4.0), json!(0.0))
    );
    assert!(
        e.state
            .transcodeur()
            .pid(r2["stream_id"].as_str().unwrap())
            .is_none()
    );
}

#[tokio::test]
async fn plafond_disque_depasse_pendant_le_transcodage() {
    let Some(e) = env_avec(|c| c.transcodage_max_octets = 1).await else {
        return;
    };
    synchro(&e).await;
    let a = id_de(&e, "Film A").await;
    let r = ouvrir(&e, "alice", &a, json!({"quality": "480p"})).await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let h = r.json();
    let flux = h["stream_id"].as_str().unwrap().to_string();
    let dossier = e.state.config().dossier_transcodage.join(&flux);
    attendre(
        || dossier.join("seg00000.ts").exists(),
        "premier segment écrit",
    )
    .await;
    assert!(e.state.transcodeur().taille().await > 1);
    crate::lecture::menage(&e.state).await;
    assert_eq!(e.state.transcodeur().nombre(), 0);
    assert!(!e.state.flux().existe(&flux));
    assert!(!dossier.exists());
    assert_eq!(
        appel(&e, None, "GET", h["url"].as_str().unwrap(), None)
            .await
            .statut,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn materiel_inutilisable_lecture_sur_le_cpu() {
    let Some(e) = env_avec(|c| {
        c.mode_materiel = ModeMateriel::Vaapi;
        c.peripherique = "/dev/absent-entracte".into();
    })
    .await
    else {
        return;
    };
    e.state.materiel().verifier(e.state.config()).await;
    let s = get(&e, "admin", "/admin/status").await.json();
    assert_eq!(
        (
            s["transcoding"]["requested"].clone(),
            s["transcoding"]["active"].clone()
        ),
        (json!("vaapi"), json!("cpu"))
    );
    assert!(
        s["transcoding"]["message"]
            .as_str()
            .unwrap()
            .contains("repli sur le CPU")
    );
    synchro(&e).await;
    let a = id_de(&e, "Film A").await;
    let r = ouvrir(&e, "alice", &a, json!({"quality": "480p"}))
        .await
        .json();
    assert_eq!(
        appel(&e, None, "GET", r["url"].as_str().unwrap(), None)
            .await
            .statut,
        StatusCode::OK
    );
}

#[tokio::test]
async fn progression_reprise_et_historique() {
    let Some(e) = env().await else { return };
    synchro(&e).await;
    let a = id_de(&e, "Film A").await;
    let serie = id_de(&e, "La Série").await;
    let episodes = get(&e, "alice", &format!("/titles/{serie}/seasons/1/episodes"))
        .await
        .json();
    let (e1, e2) = (
        episodes["items"][0]["id"].as_str().unwrap().to_string(),
        episodes["items"][1]["id"].as_str().unwrap().to_string(),
    );
    let progres = |qui: &'static str, id: String, p: f64, d: f64| {
        let e = &e;
        async move {
            appel(
                e,
                Some(qui),
                "PUT",
                &format!("/titles/{id}/progress"),
                Some(json!({"position": p, "duration": d})),
            )
            .await
            .statut
        }
    };
    assert_eq!(
        progres("alice", a.clone(), 10.0, 100.0).await,
        StatusCode::NO_CONTENT
    );
    let reprise = get(&e, "alice", "/continue").await.json();
    assert_eq!(reprise["items"][0]["id"], json!(a));
    assert_eq!(reprise["items"][0]["progress"]["position"], 10.0);
    assert!(
        get(&e, "alice", "/history").await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        get(&e, "bob", "/continue").await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        progres("bob", a.clone(), 50.0, 100.0).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get(&e, "alice", "/continue").await.json()["items"][0]["progress"]["position"],
        10.0
    );

    assert_eq!(
        progres("alice", a.clone(), 95.0, 100.0).await,
        StatusCode::NO_CONTENT
    );
    assert!(
        get(&e, "alice", "/continue").await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let h = get(&e, "alice", "/history").await.json();
    assert_eq!(
        (h["total"].clone(), h["items"][0]["id"].clone()),
        (json!(1), json!(a))
    );
    let premier = h["items"][0]["seen_at"].as_str().unwrap().to_string();
    assert!(
        get(&e, "bob", "/history").await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        progres("alice", e1.clone(), 39.0, 40.0).await,
        StatusCode::NO_CONTENT
    );
    let reprise = get(&e, "alice", "/continue").await.json();
    assert_eq!(
        reprise["items"][0]["id"],
        json!(e2),
        "épisode suivant : {reprise}"
    );
    let h = get(&e, "alice", "/history").await.json();
    assert_eq!(h["items"][0]["id"], json!(e1));
    assert_eq!(h["items"][0]["series_title"], "La Série");

    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(
        progres("alice", a.clone(), 99.0, 100.0).await,
        StatusCode::NO_CONTENT
    );
    let h = get(&e, "alice", "/history").await.json();
    assert_eq!(h["total"], 2, "une entrée par titre");
    assert_eq!(h["items"][0]["id"], json!(a));
    assert_ne!(h["items"][0]["seen_at"].as_str().unwrap(), premier);

    for corps in [
        json!({"position": -1, "duration": 10}),
        json!({"position": 1, "duration": 0}),
        json!({"position": 1}),
        json!({"position": 90000, "duration": 10}),
    ] {
        let r = appel(
            &e,
            Some("alice"),
            "PUT",
            &format!("/titles/{a}/progress"),
            Some(corps.clone()),
        )
        .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
    }
    assert_eq!(
        progres("alice", serie.clone(), 1.0, 10.0).await,
        StatusCode::NOT_FOUND
    );

    e.monde.lock().unwrap().films[0] = film_radarr(101, "Film A", None);
    synchro(&e).await;
    let h = get(&e, "alice", "/history").await;
    let hj = h.json();
    let film = hj["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == json!(a))
        .unwrap()
        .clone();
    assert_eq!(
        (
            film["title"].clone(),
            film["available"].clone(),
            film["poster"].clone()
        ),
        (json!("Film A"), json!(false), json!(true))
    );
    assert!(film["seen_at"].is_string());
    sans_fuite(&e, &h.texte(), "/history");
}

fn cles_navigateur() -> (String, String) {
    use base64::Engine;
    use ring::rand::SystemRandom;
    let rng = SystemRandom::new();
    let k =
        ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::ECDH_P256, &rng).unwrap();
    let p = k.compute_public_key().unwrap();
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    (b64.encode(p.as_ref()), b64.encode([5u8; 16]))
}

fn cles_vapid() -> (String, String) {
    use base64::Engine;
    use ring::{
        rand::SystemRandom,
        signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
    };
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    let paire =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    let fin = pkcs8.as_ref().len() - 65 - 5;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    (
        b64.encode(&pkcs8.as_ref()[fin - 32..fin]),
        b64.encode(paire.public_key().as_ref()),
    )
}

#[tokio::test]
async fn recherche_demandes_administration_et_notifications() {
    let (privee, publique) = cles_vapid();
    let Some(e) = env_avec(move |c| {
        c.vapid_privee = privee;
        c.vapid_publique = publique;
    })
    .await
    else {
        return;
    };
    synchro(&e).await;
    let push = faux_avec_entetes(|r| {
        if r.chemin == "/mort" {
            (410, vec![], vec![])
        } else {
            (201, vec![], vec![])
        }
    })
    .await;
    let (p256dh, auth) = cles_navigateur();
    for (qui, chemin) in [
        ("alice", "/alice"),
        ("alice", "/mort"),
        ("bob", "/bob"),
        ("carole", "/carole"),
    ] {
        let r = appel(&e, Some(qui), "PUT", "/push/subscription", Some(json!({"endpoint": format!("{}{chemin}", push.url), "keys": {"p256dh": p256dh, "auth": auth}}))).await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT, "{}", r.texte());
    }
    assert_eq!(
        appel(
            &e,
            Some("alice"),
            "PUT",
            "/push/subscription",
            Some(json!({"endpoint": "ftp://x", "keys": {}}))
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
    assert!(get(&e, "alice", "/push/key").await.json()["key"].is_string());

    let s = get(&e, "alice", "/search?q=film").await;
    assert_eq!(s.statut, StatusCode::OK);
    sans_fuite(&e, &s.texte(), "/search");
    let sj = s.json();
    let items = sj["items"].as_array().unwrap();
    let a = items.iter().find(|i| i["title"] == "Film A").unwrap();
    let c = items.iter().find(|i| i["title"] == "Film C").unwrap();
    assert_eq!(
        (a["present"].clone(), a["requestable"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        (
            c["present"].clone(),
            c["requestable"].clone(),
            c["id"].clone()
        ),
        (json!(false), json!(true), Value::Null)
    );
    assert_eq!(c["poster_url"], "/images/tmdb/a103.jpg");
    assert_eq!(
        get(&e, "alice", "/images/tmdb/a103.jpg").await.texte(),
        "IMG-a103.jpg"
    );
    assert!(!items.iter().any(|i| i["title"] == "Quelqu'un"));
    assert_eq!(items.iter().filter(|i| i["title"] == "La Série").count(), 1);

    let demande = |qui: &'static str, corps: Value| {
        let e = &e;
        async move { appel(e, Some(qui), "POST", "/requests", Some(corps)).await }
    };
    assert_eq!(
        demande("alice", json!({"kind": "movie", "tmdb_id": 101}))
            .await
            .statut,
        StatusCode::CONFLICT
    );
    assert_eq!(
        demande("carole", json!({"kind": "movie", "tmdb_id": 103}))
            .await
            .statut,
        StatusCode::FORBIDDEN
    );
    let r = demande("alice", json!({"kind": "movie", "tmdb_id": 103})).await;
    assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
    let id_c = r.json()["id"].as_str().unwrap().to_string();
    let r = demande("bob", json!({"kind": "movie", "tmdb_id": 103})).await;
    assert_eq!(
        (
            r.statut,
            r.json()["id"].clone(),
            r.json()["joined_existing"].clone()
        ),
        (StatusCode::OK, json!(id_c), json!(true))
    );
    let serie = demande(
        "alice",
        json!({"kind": "series", "tmdb_id": 201, "seasons": [2]}),
    )
    .await;
    assert_eq!(serie.statut, StatusCode::CREATED, "{}", serie.texte());
    let id_s = serie.json()["id"].as_str().unwrap().to_string();
    assert_eq!(serie.json()["seasons"], json!([2]));
    assert_eq!(
        demande(
            "alice",
            json!({"kind": "series", "tmdb_id": 201, "seasons": [1]})
        )
        .await
        .statut,
        StatusCode::CONFLICT
    );
    assert_eq!(
        demande(
            "alice",
            json!({"kind": "series", "tmdb_id": 201, "seasons": [9]})
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        demande("alice", json!({"kind": "series", "tmdb_id": 201}))
            .await
            .statut,
        StatusCode::CREATED
    );
    let saisons = get(&e, "alice", "/search/series/201").await.json();
    assert_eq!(saisons["seasons"][0]["complete"], true);
    assert_eq!(saisons["seasons"][1]["request_state"], "en_attente");

    let s = get(&e, "alice", "/search?q=film").await.json();
    let c = s["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == "Film C")
        .unwrap()
        .clone();
    assert_eq!(c["request"]["state"], "en_attente");

    let corrigee = get(&e, "alice", "/search?q=dardevil").await;
    assert_eq!(corrigee.statut, StatusCode::OK);
    sans_fuite(&e, &corrigee.texte(), "/search");
    let titres: Vec<Value> = corrigee.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].clone())
        .collect();
    assert_eq!(
        titres,
        [
            json!("Marvel's Daredevil"),
            json!("Daredevil"),
            json!("Dark Devil")
        ]
    );

    let mes = get(&e, "bob", "/requests/mine").await;
    assert_eq!(mes.json()["items"].as_array().unwrap().len(), 1);
    assert!(mes.json()["items"][0].get("sent").is_none());
    assert_eq!(
        get(&e, "alice", "/admin/requests").await.statut,
        StatusCode::FORBIDDEN
    );
    let liste = get(&e, "admin", "/admin/requests?state=en_attente").await;
    sans_fuite(&e, &liste.texte(), "/admin/requests");
    let lj = liste.json();
    let dc = lj["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == json!(id_c))
        .unwrap()
        .clone();
    assert_eq!(dc["requesters"].as_array().unwrap().len(), 2);
    assert_eq!(
        get(&e, "admin", "/admin/requests?requester=bob")
            .await
            .json()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        get(&e, "admin", "/admin/requests?state=nope").await.statut,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        appel(
            &e,
            Some("admin"),
            "POST",
            &format!("/admin/requests/{id_s}/refuse"),
            Some(json!({"reason": " "}))
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );

    e.monde.lock().unwrap().radarr_ajouts_refuses = true;
    let r = appel(
        &e,
        Some("admin"),
        "POST",
        &format!("/admin/requests/{id_c}/accept"),
        None,
    )
    .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        (r.json()["state"].clone(), r.json()["sent"].clone()),
        (json!("acceptee"), json!(false))
    );
    assert!(r.json()["send_error"].as_str().unwrap().contains("400"));
    e.monde.lock().unwrap().radarr_ajouts_refuses = false;
    assert_eq!(
        get(&e, "alice", "/admin/requests/options?kind=movie")
            .await
            .statut,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&e, "admin", "/admin/requests/options").await.statut,
        StatusCode::BAD_REQUEST
    );
    let options = get(&e, "admin", "/admin/requests/options?kind=movie")
        .await
        .json();
    assert_eq!(
        options,
        json!({
            "folders": [{"path": "/movies", "free": 5000}, {"path": "/movies-4k", "free": 90}],
            "profiles": [{"id": 7, "name": "HD-1080p"}, {"id": 9, "name": "Ultra-HD"}],
            "tags": [{"id": 2, "label": "vf"}],
            "defaults": {"folder": "/movies", "profile": 7, "tags": [], "availability": null},
        })
    );
    for mauvais in [
        json!({"folder": 3}),
        json!({"profile": "hd"}),
        json!({"tags": [0]}),
    ] {
        let r = appel(
            &e,
            Some("admin"),
            "POST",
            &format!("/admin/requests/{id_c}/accept"),
            Some(mauvais),
        )
        .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{}", r.texte());
    }
    let r = appel(
        &e,
        Some("admin"),
        "POST",
        &format!("/admin/requests/{id_c}/accept"),
        Some(json!({"folder": "/ailleurs"})),
    )
    .await;
    assert_eq!(
        (r.json()["sent"].clone(), r.json()["send_error"].clone()),
        (json!(false), json!("emplacement inconnu"))
    );
    let r = appel(
        &e,
        Some("admin"),
        "POST",
        &format!("/admin/requests/{id_c}/accept"),
        Some(json!({"folder": "/movies-4k", "profile": 9, "tags": [2]})),
    )
    .await;
    assert_eq!(
        (r.json()["sent"].clone(), r.json()["send_error"].clone()),
        (json!(true), Value::Null)
    );
    let ajout = e
        .radarr
        .requetes()
        .into_iter()
        .rfind(|r| r.methode == "POST" && r.chemin == "/api/v3/movie")
        .unwrap()
        .json();
    assert_eq!(
        (
            ajout["tmdbId"].clone(),
            ajout["qualityProfileId"].clone(),
            ajout["rootFolderPath"].clone(),
            ajout["tags"].clone()
        ),
        (json!(103), json!(9), json!("/movies-4k"), json!([2]))
    );
    assert_eq!(ajout["addOptions"]["searchForMovie"], true);
    assert_eq!(
        appel(
            &e,
            Some("admin"),
            "POST",
            &format!("/admin/requests/{id_c}/accept"),
            None
        )
        .await
        .statut,
        StatusCode::CONFLICT
    );

    let r = appel(
        &e,
        Some("admin"),
        "POST",
        &format!("/admin/requests/{id_s}/accept"),
        None,
    )
    .await;
    assert_eq!(r.json()["sent"], true, "{}", r.texte());
    let req = e.sonarr.requetes();
    let maj = req.iter().find(|r| r.methode == "PUT").unwrap().json();
    assert_eq!(
        maj["seasons"],
        json!([{"seasonNumber": 1, "monitored": true}, {"seasonNumber": 2, "monitored": true}])
    );
    let cmd = req
        .iter()
        .find(|r| r.chemin == "/api/v3/command")
        .unwrap()
        .json();
    assert_eq!(
        cmd,
        json!({"name": "SeasonSearch", "seriesId": SONARR_ID, "seasonNumber": 2})
    );

    let entiere = get(&e, "admin", "/admin/requests?state=en_attente")
        .await
        .json()["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = appel(
        &e,
        Some("admin"),
        "POST",
        &format!("/admin/requests/{entiere}/refuse"),
        Some(json!({"reason": "Saisons déjà demandées"})),
    )
    .await;
    assert_eq!(
        (r.statut, r.json()["state"].clone()),
        (StatusCode::OK, json!("refusee"))
    );
    let mes = get(&e, "alice", "/requests/mine").await.json();
    let refusee = mes["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == json!(entiere))
        .unwrap()
        .clone();
    assert_eq!(refusee["reason"], "Saisons déjà demandées");
    attendre(
        || push.requetes().iter().any(|r| r.chemin == "/alice"),
        "push de refus",
    )
    .await;
    assert!(
        !push
            .requetes()
            .iter()
            .any(|r| r.chemin == "/bob" || r.chemin == "/carole")
    );
    let limite = Instant::now() + Duration::from_secs(20);
    let restants = loop {
        let restants: Vec<String> =
            sqlx::query_scalar("SELECT endpoint FROM abonnements_push ORDER BY endpoint")
                .fetch_all(e.state.pool())
                .await
                .unwrap();
        if !restants.iter().any(|x| x.ends_with("/mort")) {
            break restants;
        }
        assert!(
            Instant::now() < limite,
            "abonnement refusé retiré : {restants:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(restants.len(), 3);
    let notifs = get(&e, "alice", "/notifications").await.json();
    assert_eq!(notifs["items"][0]["kind"], "refusee");
    assert_eq!(
        get(
            &e,
            "alice",
            &format!("/notifications?after={}", notifs["last_id"])
        )
        .await
        .json()["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let avant = push.appels();
    e.monde.lock().unwrap().films.push(film_radarr(
        103,
        "Film C",
        Some("/movies/Film C/Film.C.mkv"),
    ));
    e.monde.lock().unwrap().episodes = vec![
        (1, 1, true),
        (1, 2, true),
        (2, 1, true),
        (2, 2, true),
        (2, 3, true),
    ];
    std::fs::create_dir_all(medias().join("series/La Serie")).unwrap();
    for n in 1..=3 {
        let dest = medias().join(format!("series/La Serie/S02E0{n}.mkv"));
        if !dest.exists() {
            std::fs::copy(medias().join("series/La Serie/S01E01.mkv"), dest).unwrap();
        }
    }
    synchro(&e).await;
    for qui in ["alice", "bob"] {
        let mes = get(&e, qui, "/requests/mine").await.json();
        let d = mes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == json!(id_c))
            .unwrap()
            .clone();
        assert_eq!(d["state"], "disponible", "{qui} : {mes}");
        assert!(d["title_id"].is_string());
    }
    let mes = get(&e, "alice", "/requests/mine").await.json();
    assert_eq!(
        mes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == json!(id_s))
            .unwrap()["state"],
        "disponible"
    );
    attendre(
        || {
            push.requetes()
                .iter()
                .skip(avant)
                .any(|r| r.chemin == "/bob")
        },
        "push de disponibilité",
    )
    .await;
    assert!(
        !push
            .requetes()
            .iter()
            .skip(avant)
            .any(|r| r.chemin == "/carole")
    );
    sans_fuite(
        &e,
        &get(&e, "alice", "/requests/mine").await.texte(),
        "/requests/mine",
    );
    for chemin in ["/alice", "/bob"] {
        let r = push
            .requetes()
            .into_iter()
            .rfind(|r| r.chemin == chemin)
            .unwrap();
        assert_eq!(r.entete("content-encoding"), "aes128gcm");
    }
}

#[tokio::test]
async fn titre_pour_le_salon_cinema_d_estompe() {
    let Some(e) = env().await else { return };
    synchro(&e).await;
    let serie = id_de(&e, "La Série").await;
    let episodes = get(&e, "alice", &format!("/titles/{serie}/seasons/1/episodes"))
        .await
        .json();
    let e1 = episodes["items"][0]["id"].as_str().unwrap().to_string();
    let service = [("authorization", "Bearer jeton-service-estompe")];
    let r = appel_avec(
        &e,
        None,
        "GET",
        &format!("/internal/titles/{e1}?sub=sub-alice"),
        None,
        &service,
    )
    .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let v = r.json();
    assert_eq!(v["title"], "La Série · S1E1 · Pilote");
    assert_eq!(v["allowed"], true);
    assert_eq!(v["next"].as_array().unwrap().len(), 1);
    assert_eq!(v["next"][0]["title"], "La Série · S1E2 · Suite");
    sans_fuite(&e, &r.texte(), "/internal/titles");
    let r = appel_avec(
        &e,
        None,
        "GET",
        &format!("/internal/titles/{e1}?sub=sub-inconnu"),
        None,
        &service,
    )
    .await;
    assert_eq!(r.json()["allowed"], false);
    let r = appel_avec(
        &e,
        None,
        "GET",
        "/internal/titles/00000000-0000-0000-0000-000000000009?sub=sub-alice",
        None,
        &service,
    )
    .await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
    assert_eq!(
        appel_avec(
            &e,
            None,
            "GET",
            &format!("/internal/titles/{serie}?sub=sub-alice"),
            None,
            &service
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        appel_avec(
            &e,
            None,
            "GET",
            &format!("/internal/titles/{e1}"),
            None,
            &service
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        appel_avec(
            &e,
            None,
            "GET",
            &format!("/internal/titles/{e1}?sub=x"),
            None,
            &[("authorization", "Bearer faux")]
        )
        .await
        .statut,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn webhook_declenche_la_synchronisation() {
    let Some(e) = env().await else { return };
    assert_eq!(
        appel(&e, None, "POST", "/webhooks/radarr?token=faux", None)
            .await
            .statut,
        StatusCode::UNAUTHORIZED
    );
    use base64::Engine;
    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("radarr:jeton-webhook")
    );
    for (chemin, entetes) in [
        ("/webhooks/radarr?token=jeton-webhook", vec![]),
        ("/webhooks/sonarr", vec![("authorization", basic.as_str())]),
        (
            "/webhooks/radarr",
            vec![("authorization", "Bearer jeton-webhook")],
        ),
    ] {
        assert_eq!(
            appel_avec(&e, None, "POST", chemin, None, &entetes)
                .await
                .statut,
            StatusCode::NO_CONTENT,
            "{chemin}"
        );
    }
    let limite = Instant::now() + Duration::from_secs(30);
    loop {
        if get(&e, "alice", "/library").await.json()["total"] == 2 {
            break;
        }
        assert!(Instant::now() < limite, "synchronisation non lancée");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
