use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{Router, http::Method, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::{
    config::{Lidarr, correspondances},
    droits::Droit,
    etat::AppState,
    garde::{Garde, Personne},
    http::ok,
    routes::{self, Entree, entree},
    testutil::{Env, Faux, Rep, Requete, appel_sur, attendre, env, env_avec, faux_avec_entetes},
};

async fn sonde(p: Personne) -> axum::response::Response {
    ok(&json!({ "sub": p.sub() }))
}

fn sondes() -> Vec<Entree> {
    Droit::TOUS
        .iter()
        .map(|d| {
            let chemin: &'static str = Box::leak(format!("/sondes/{}", d.cle()).into_boxed_str());
            entree(Method::GET, chemin, Garde::Droit(*d), get(sonde))
        })
        .collect()
}

fn avec_sondes(e: &Env) -> (Router, Vec<(Method, &'static str, Garde)>) {
    let entrees: Vec<Entree> = routes::table().into_iter().chain(sondes()).collect();
    let routes = entrees
        .iter()
        .map(|(r, _)| (r.methode.clone(), r.chemin, r.garde))
        .collect();
    (routes::construire(e.state.clone(), entrees), routes)
}

fn exemple(chemin: &str) -> String {
    chemin
        .split('/')
        .map(|s| {
            if s.starts_with('{') {
                "00000000-0000-0000-0000-000000000001"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let Some(e) = env().await else { return };
    e.droits
        .lock()
        .unwrap()
        .insert("sub-sansacces".into(), vec!["add", "admin"]);
    let (app, routes) = avec_sondes(&e);
    let mut droits_vus = Vec::new();
    for (methode, chemin, garde) in routes {
        let chemin = exemple(chemin);
        let m = methode.as_str();
        let corps = (m != "GET" && m != "DELETE").then(|| json!({}));
        let appel =
            |qui: Option<&'static str>| appel_sur(&app, qui, m, &chemin, corps.clone(), &[]);
        match garde {
            Garde::Droit(d) => {
                droits_vus.push(d);
                let r = appel(None).await;
                assert_eq!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} sans session"
                );
                let r = appel(Some("invalide")).await;
                assert_eq!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} session invalide"
                );
                let r = appel(Some("personne")).await;
                assert_eq!(
                    r.statut,
                    StatusCode::FORBIDDEN,
                    "{m} {chemin} sans permission"
                );
                assert!(r.texte().contains(&d.nom()), "{m} {chemin} : {}", r.texte());
                let sauf = match d {
                    Droit::Acces => "sansacces",
                    Droit::Ajout => "alice",
                    Droit::Admin => "bruno",
                };
                let r = appel(Some(sauf)).await;
                assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour {sauf}");
                assert!(r.texte().contains(&d.nom()));
                let r = appel(Some("admin")).await;
                assert_ne!(
                    r.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin} pour admin"
                );
                assert_ne!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin} pour admin");
            }
            Garde::Session(_) => {
                assert_eq!(
                    appel(None).await.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
                assert_eq!(
                    appel(Some("invalide")).await.statut,
                    StatusCode::UNAUTHORIZED,
                    "{m} {chemin}"
                );
                assert_eq!(
                    appel(Some("personne")).await.statut,
                    StatusCode::OK,
                    "{m} {chemin}"
                );
            }
            Garde::Webhook(_) => {
                for qui in [None, Some("admin"), Some("jeton-faux")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
            }
            Garde::Public(_) => {
                assert!(appel(None).await.statut.is_success(), "{m} {chemin}");
            }
            Garde::Service(_) => {
                for qui in [None, Some("admin"), Some("jeton-faux")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
            }
            Garde::Jeton(_) => {
                for qui in [None, Some("admin")] {
                    let r = appel(qui).await;
                    assert_eq!(
                        r.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                    assert_eq!(r.json()["error"], "stream_expired");
                }
            }
        }
    }
    assert!(Droit::TOUS.iter().all(|d| droits_vus.contains(d)));
}

#[tokio::test]
async fn me_sans_permission_sillon_annonce_ce_qu_on_peut_ouvrir() {
    let Some(e) = env().await else { return };
    let r = e.get("personne", "/me").await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json(),
        json!({
            "sub": "sub-personne",
            "name": "personne",
            "email": "personne@x.fr",
            "permissions": {"access": false, "add": false, "admin": false},
            "sections": [],
        })
    );
    let r = e.get("alice", "/me").await.json();
    assert_eq!(
        r["permissions"],
        json!({"access": true, "add": false, "admin": false})
    );
    assert_eq!(r["sections"], json!(["library"]));
    assert_eq!(
        e.get("bruno", "/me").await.json()["sections"],
        json!(["library", "add"])
    );
    assert_eq!(
        e.get("admin", "/me").await.json()["sections"],
        json!(["library", "add", "admin"])
    );
    let r = e
        .appel_avec(
            None,
            "GET",
            "/me",
            None,
            &[("cookie", "a=b; __Secure-plbls_sid=alice")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.json()["sub"], "sub-alice");
    assert_eq!(r.entetes["cache-control"], "no-store");
    assert_eq!(r.entetes["x-content-type-options"], "nosniff");
    let r = e.get("panne", "/me").await;
    assert_eq!(r.statut, StatusCode::BAD_GATEWAY);
    assert_eq!(r.json()["error"], "gateway_unavailable");
}

#[tokio::test]
async fn aegis_injoignable_me_repond_et_les_permissions_sont_refusees() {
    let Some(e) = env_avec(|c| c.aegis = Some(("http://127.0.0.1:9".into(), "jeton".into()))).await
    else {
        return;
    };
    let r = e.get("admin", "/me").await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json()["permissions"],
        json!({"access": false, "add": false, "admin": false})
    );
    assert_eq!(r.json()["sections"], json!([]));
    let (app, _) = avec_sondes(&e);
    let r = appel_sur(&app, Some("admin"), "GET", "/sondes/access", None, &[]).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "aegis_unavailable");
}

#[tokio::test]
async fn sans_aegis_configure_tout_est_ferme() {
    let Some(e) = env_avec(|c| c.aegis = None).await else {
        return;
    };
    assert_eq!(
        e.get("admin", "/me").await.json()["permissions"],
        json!({"access": false, "add": false, "admin": false})
    );
    let (app, _) = avec_sondes(&e);
    let r = appel_sur(&app, Some("admin"), "GET", "/sondes/admin", None, &[]).await;
    assert_eq!(r.statut, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn manifeste_envoye_a_aegis_au_demarrage() {
    let Some(e) = env().await else { return };
    e.state.demarrer_taches();
    attendre(
        || {
            e.aegis
                .requetes()
                .iter()
                .any(|r| r.chemin == "/v1/manifest")
        },
        "manifeste reçu par aegis",
    )
    .await;
    let r = e
        .aegis
        .requetes()
        .into_iter()
        .find(|r| r.chemin == "/v1/manifest")
        .unwrap();
    assert_eq!(r.methode, "PUT");
    assert_eq!(r.entete("authorization"), "Bearer jeton-aegis");
    let m = r.json();
    assert_eq!(m["app"], "sillon");
    let p = m["permissions"].as_array().unwrap();
    let cles: Vec<&str> = p.iter().map(|x| x["key"].as_str().unwrap()).collect();
    assert_eq!(cles, ["access", "add", "admin"]);
    assert!(p.iter().all(|x| x["open_default"] == json!(false)), "{m}");
}

#[tokio::test]
async fn origine_et_cors() {
    let Some(e) = env().await else { return };
    let r = e
        .appel_avec(
            None,
            "POST",
            "/me",
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
    let r = e
        .appel_avec(
            None,
            "OPTIONS",
            "/me",
            None,
            &[("origin", "https://estompe.plbls.fr")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    assert_eq!(
        r.entetes["access-control-allow-origin"],
        "https://estompe.plbls.fr"
    );
    assert!(
        r.entetes["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .contains("Authorization")
    );
    let r = e
        .appel_avec(
            None,
            "GET",
            "/me",
            None,
            &[
                ("cookie", "__Secure-plbls_sid=alice"),
                ("origin", "https://estompe.plbls.fr"),
            ],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.entetes["access-control-allow-credentials"], "true");
    let r = e
        .appel_avec(
            Some("alice"),
            "GET",
            "/me",
            None,
            &[("origin", "https://ailleurs.fr")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert!(!r.entetes.contains_key("access-control-allow-origin"));
    let r = e.appel(None, "GET", "/inconnue", None).await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
    assert_eq!(r.json()["error"], "not_found");
}

#[tokio::test]
async fn sante_disponibilite_et_migrations() {
    let Some(e) = env().await else { return };
    let r = e.appel(None, "GET", "/healthz", None).await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(
        r.json(),
        json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")})
    );
    let r = e.appel(None, "GET", "/readyz", None).await;
    assert_eq!(
        (r.statut, r.json()),
        (StatusCode::OK, json!({"status": "ready"}))
    );
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(e.state.pool())
            .await
            .unwrap();
    let attendues: Vec<i64> = crate::MIGRATIONS.iter().map(|m| m.version).collect();
    assert_eq!(versions, attendues);
    assert_eq!(attendues[..2], [1, 2]);
    crate::MIGRATIONS.run(e.state.pool()).await.unwrap();
    e.state.pool().close().await;
    let r = e.appel(None, "GET", "/readyz", None).await;
    assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json()["error"], "not_ready");
}

pub(crate) const LIDARR: i64 = 987_000;

pub(crate) fn ffmpeg_present() -> bool {
    ["ffmpeg", "ffprobe"].iter().all(|o| {
        std::process::Command::new(o)
            .arg("-version")
            .output()
            .is_ok_and(|s| s.status.success())
    })
}

pub(crate) fn generer(dest: &Path, codec: &[&str]) {
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let ok = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
        .args(codec)
        .arg(dest)
        .status()
        .unwrap()
        .success();
    assert!(ok, "génération de {}", dest.display());
}

pub(crate) const FLAC: &[&str] = &["-c:a", "flac"];
pub(crate) const MP3: &[&str] = &["-c:a", "libmp3lame", "-b:a", "128k"];
const OPUS: &[&str] = &["-c:a", "libopus", "-b:a", "64k"];

#[derive(Default)]
pub(crate) struct Monde {
    pub(crate) artistes: Vec<Value>,
    pub(crate) albums: Vec<Value>,
    pub(crate) pistes: Vec<Value>,
    pub(crate) fichiers: Vec<Value>,
    pub(crate) en_panne: bool,
    pub(crate) mb_artistes: Vec<Value>,
    pub(crate) mb_albums: Vec<Value>,
    pub(crate) file: Vec<Value>,
    pub(crate) historique: Vec<Value>,
    pub(crate) dossiers: Vec<Value>,
    pub(crate) qualites: Vec<Value>,
    pub(crate) metas: Vec<Value>,
}

impl Monde {
    fn piste(
        &mut self,
        id: i64,
        album: i64,
        titre: &str,
        numero: i64,
        fichier: Option<(i64, &str)>,
    ) {
        self.pistes.retain(|p| p["id"] != id);
        self.pistes.push(json!({
            "id": id, "albumId": album, "artistId": artiste_de(album), "foreignRecordingId": format!("rec-{titre}"),
            "foreignTrackId": format!("trk-{titre}"), "title": titre, "trackNumber": numero.to_string(),
            "absoluteTrackNumber": numero, "mediumNumber": 1, "duration": 2000, "explicit": false,
            "hasFile": fichier.is_some(), "trackFileId": fichier.map_or(0, |f| f.0),
        }));
        if let Some((f, chemin)) = fichier {
            self.fichiers.retain(|x| x["id"] != f);
            self.fichiers.push(
                json!({"id": f, "artistId": artiste_de(album), "albumId": album,
                "path": format!("/music/{chemin}"), "size": 1234}),
            );
        }
    }
}

fn artiste_de(album: i64) -> i64 {
    if album == LIDARR + 30 {
        LIDARR + 3
    } else {
        LIDARR + 1
    }
}

fn monde() -> Monde {
    let mut m = Monde {
        artistes: vec![
            json!({"id": LIDARR + 1, "artistName": "Beyoncé", "sortName": "Beyonce", "foreignArtistId": "mb-beyonce",
                   "overview": "Chanteuse.", "genres": ["R&B", "Pop"],
                   "images": [{"coverType": "poster", "url": "/MediaCover/987001/poster.jpg?lastWrite=1"}],
                   "statistics": {"trackFileCount": 8}}),
            json!({"id": LIDARR + 2, "artistName": "Sans fichiers", "foreignArtistId": "mb-vide",
                   "statistics": {"trackFileCount": 0}}),
            json!({"id": LIDARR + 3, "artistName": "Sigur Rós", "foreignArtistId": "mb-sigur", "genres": ["Post-rock"],
                   "statistics": {"trackFileCount": 1}}),
        ],
        albums: vec![
            json!({"id": LIDARR + 10, "artistId": LIDARR + 1, "foreignAlbumId": "mb-lemonade", "title": "Lemonade",
                   "albumType": "Album", "releaseDate": "2016-04-23T00:00:00Z", "genres": ["R&B"],
                   "images": [{"coverType": "cover", "url": "/MediaCover/Albums/987010/cover.jpg?lastWrite=2"}]}),
            json!({"id": LIDARR + 11, "artistId": LIDARR + 1, "foreignAlbumId": "mb-beyonce-album", "title": "Beyoncé",
                   "albumType": "Album", "releaseDate": "2013-12-13T00:00:00Z"}),
            json!({"id": LIDARR + 30, "artistId": LIDARR + 3, "foreignAlbumId": "mb-takk", "title": "Takk…",
                   "albumType": "Album", "releaseDate": "2005-09-12T00:00:00Z"}),
        ],
        dossiers: vec![
            json!({"name": "music", "path": "/data/music", "defaultMetadataProfileId": 3,
            "defaultQualityProfileId": 4, "defaultMonitorOption": "all", "defaultNewItemMonitorOption": "all",
            "defaultTags": [7], "accessible": true, "freeSpace": 81014300672i64, "totalSpace": 207071854592i64, "id": 2}),
        ],
        qualites: vec![
            json!({"id": 1, "name": "Any", "items": [{"quality": {"name": "MP3-320"}, "allowed": true}]}),
            json!({"id": 4, "name": "Lossless", "items": [
                {"quality": {"name": "MP3-320"}, "allowed": true},
                {"quality": {"name": "FLAC"}, "allowed": true},
                {"quality": {"name": "FLAC 24bit"}, "allowed": false}]}),
        ],
        metas: vec![
            json!({"id": 1, "name": "Standard"}),
            json!({"id": 3, "name": "Albums studio"}),
        ],
        ..Monde::default()
    };
    let lemonade = LIDARR + 10;
    m.piste(
        LIDARR + 100,
        lemonade,
        "Formation",
        1,
        Some((LIDARR + 200, "Beyonce/Lemonade/01 Formation.flac")),
    );
    m.piste(
        LIDARR + 101,
        lemonade,
        "Hold Up",
        2,
        Some((LIDARR + 201, "Beyonce/Lemonade/02 Hold Up.mp3")),
    );
    m.piste(
        LIDARR + 102,
        lemonade,
        "Sorry",
        3,
        Some((LIDARR + 202, "Beyonce/Lemonade/03 Sorry.opus")),
    );
    m.piste(
        LIDARR + 103,
        lemonade,
        "Daddy Lessons",
        4,
        Some((LIDARR + 203, "Beyonce/Lemonade/lien.flac")),
    );
    m.piste(
        LIDARR + 104,
        lemonade,
        "Love Drought",
        5,
        Some((LIDARR + 204, "Beyonce/Lemonade/Love.exe.flac")),
    );
    m.piste(
        LIDARR + 105,
        lemonade,
        "Sandcastles",
        6,
        Some((LIDARR + 205, "Beyonce/Lemonade/faux.flac")),
    );
    m.piste(LIDARR + 106, lemonade, "Forward", 7, None);
    m.pistes.push(json!({"id": LIDARR + 107, "albumId": lemonade, "artistId": LIDARR + 1, "foreignRecordingId": "rec-Freedom", "title": "Freedom",
        "trackNumber": "8", "mediumNumber": 1, "hasFile": true, "trackFileId": LIDARR + 207}));
    m.fichiers.push(json!({"id": LIDARR + 207, "artistId": LIDARR + 1, "path": "/autre/Freedom.flac", "size": 1}));
    m.piste(LIDARR + 110, LIDARR + 11, "Beyoncé Medley", 1, None);
    m.piste(
        LIDARR + 300,
        LIDARR + 30,
        "Glósóli",
        1,
        Some((LIDARR + 400, "Sigur Ros/Takk/01 Glosoli.flac")),
    );
    m
}

fn reponse_lidarr(r: &Requete, m: &Mutex<Monde>) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
    let mut m = m.lock().unwrap();
    if r.entete("x-api-key") != "cle-lidarr" {
        return (401, vec![], vec![]);
    }
    if m.en_panne {
        return (500, vec![], b"panne".to_vec());
    }
    if let Some(rep) = crate::ajouts::tests::lidarr_ajouts(r, &mut m) {
        return rep;
    }
    let corps = |v: Value| {
        (
            200,
            vec![("content-type", "application/json".to_string())],
            v.to_string().into_bytes(),
        )
    };
    let de_l_artiste = |l: &[Value]| {
        let id: i64 = r
            .query
            .strip_prefix("artistId=")
            .and_then(|i| i.parse().ok())
            .unwrap_or(-1);
        corps(json!(
            l.iter().filter(|x| x["artistId"] == id).collect::<Vec<_>>()
        ))
    };
    match r.chemin.as_str() {
        "/api/v1/system/status" => corps(json!({"version": "2.9.6.4552"})),
        "/api/v1/artist" => corps(json!(m.artistes)),
        "/api/v1/album" => de_l_artiste(&m.albums),
        "/api/v1/track" => de_l_artiste(&m.pistes),
        "/api/v1/trackfile" => de_l_artiste(&m.fichiers),
        c if c.starts_with("/api/v1/mediacover/") => (
            200,
            vec![("content-type", "image/jpeg".to_string())],
            format!("IMG-{}", c.rsplit('/').next().unwrap_or_default()).into_bytes(),
        ),
        _ => (404, vec![], vec![]),
    }
}

pub(crate) struct Bibliotheque {
    pub(crate) e: Env,
    pub(crate) lidarr: Faux,
    pub(crate) monde: Arc<Mutex<Monde>>,
    pub(crate) medias: tempfile::TempDir,
    _dehors: tempfile::TempDir,
}

impl Bibliotheque {
    pub(crate) fn chemin(&self, relatif: &str) -> std::path::PathBuf {
        self.medias.path().join(relatif)
    }

    pub(crate) async fn synchro(&self) {
        crate::catalogue::synchroniser(&self.e.state).await;
    }

    pub(crate) async fn get(&self, qui: &str, chemin: &str) -> Rep {
        self.e.get(qui, chemin).await
    }

    pub(crate) fn sans_fuite(&self, texte: &str, contexte: &str) {
        let medias = self.medias.path().to_string_lossy().to_string();
        for interdit in [
            medias.as_str(),
            "/music",
            "/autre",
            "MediaCover",
            "remoteUrl",
            "exemple.invalid",
            "lastWrite",
            self.lidarr.url.as_str(),
            "cle-lidarr",
        ] {
            assert!(
                !texte.contains(interdit),
                "{contexte} : « {interdit} » dans {texte}"
            );
        }
        for id in ["9870", "9871", "9872", "9873", "9874"] {
            assert!(
                !commence_un_mot(texte, id),
                "{contexte} : id Lidarr « {id}… » dans {texte}"
            );
        }
    }

    pub(crate) async fn album(&self, titre: &str) -> Value {
        let s = self
            .get("alice", &format!("/search?q={}", encode(titre)))
            .await
            .json();
        let id = s["albums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["title"] == titre)
            .unwrap_or_else(|| panic!("{titre} : {s}"))["id"]
            .as_str()
            .unwrap()
            .to_string();
        self.get("alice", &format!("/albums/{id}")).await.json()
    }
}

fn commence_un_mot(texte: &str, motif: &str) -> bool {
    texte.match_indices(motif).any(|(i, _)| {
        !texte[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

#[test]
fn un_id_lidarr_ne_se_confond_pas_avec_une_empreinte() {
    assert!(!commence_un_mot(
        r#"{"image":"5544a9ad456987274bafa9bf08e49f4c"}"#,
        "9872"
    ));
    assert!(commence_un_mot(r#"{"id":987201}"#, "9872"));
    assert!(commence_un_mot("/MediaCover/987201/poster.jpg", "9872"));
    assert!(commence_un_mot("987201", "9872"));
}

pub(crate) fn encode(t: &str) -> String {
    t.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub(crate) fn piste<'a>(album: &'a Value, titre: &str) -> &'a Value {
    album["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == titre)
        .unwrap_or_else(|| panic!("{titre} : {album}"))
}

pub(crate) async fn bibliotheque() -> Option<Bibliotheque> {
    bibliotheque_avec(|_| {}).await
}

pub(crate) async fn bibliotheque_avec(
    regler: impl FnOnce(&mut crate::config::Config) + Send + 'static,
) -> Option<Bibliotheque> {
    if !ffmpeg_present() {
        eprintln!("ignoré : ffmpeg ou ffprobe absent");
        return None;
    }
    let medias = tempfile::tempdir().unwrap();
    let dehors = tempfile::tempdir().unwrap();
    let r = medias.path();
    generer(&r.join("Beyonce/Lemonade/01 Formation.flac"), FLAC);
    generer(&r.join("Beyonce/Lemonade/02 Hold Up.mp3"), MP3);
    generer(&r.join("Beyonce/Lemonade/03 Sorry.opus"), OPUS);
    generer(&r.join("Beyonce/Lemonade/Love.exe.flac"), FLAC);
    generer(&r.join("Beyonce/Lemonade/faux.mp3"), MP3);
    std::fs::rename(
        r.join("Beyonce/Lemonade/faux.mp3"),
        r.join("Beyonce/Lemonade/faux.flac"),
    )
    .unwrap();
    generer(&dehors.path().join("secret.flac"), FLAC);
    std::os::unix::fs::symlink(
        dehors.path().join("secret.flac"),
        r.join("Beyonce/Lemonade/lien.flac"),
    )
    .unwrap();
    generer(&r.join("Sigur Ros/Takk/01 Glosoli.flac"), FLAC);
    let monde = Arc::new(Mutex::new(monde()));
    let m = monde.clone();
    let lidarr = faux_avec_entetes(move |r| reponse_lidarr(r, &m)).await;
    let (url, local) = (lidarr.url.clone(), r.to_path_buf());
    let e = env_avec(move |c| {
        c.lidarr = Some(Lidarr {
            url,
            cle: "cle-lidarr".into(),
        });
        c.chemins = correspondances(&format!("/music={}", local.display())).unwrap();
        c.medias = local;
        regler(c);
    })
    .await?;
    Some(Bibliotheque {
        e,
        lidarr,
        monde,
        medias,
        _dehors: dehors,
    })
}

#[tokio::test]
async fn synchronisation_cree_artistes_albums_et_pistes_avec_formats() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let etat = b.get("admin", "/admin/status").await;
    let ej = etat.json();
    assert_eq!(ej["sync"][0]["source"], "lidarr", "{ej}");
    assert!(ej["sync"][0]["error"].is_null(), "{ej}");
    assert_eq!(ej["library"]["artists"], 2, "{ej}");
    assert_eq!(ej["library"]["albums"], 2, "{ej}");
    assert_eq!(ej["library"]["tracks"], 4, "{ej}");
    b.sans_fuite(&etat.texte(), "/admin/status");

    let artistes = b.get("alice", "/artists").await;
    assert_eq!(artistes.statut, StatusCode::OK);
    let aj = artistes.json();
    let noms: Vec<&str> = aj["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(noms, ["Beyoncé", "Sigur Rós"]);
    assert_eq!(aj["items"][0]["genres"], json!(["Pop", "R&B"]));
    assert_eq!(aj["items"][0]["album_count"], 1);
    b.sans_fuite(&artistes.texte(), "/artists");
    let p1 = b.get("alice", "/artists?limit=1").await.json();
    assert_eq!(p1["items"][0]["name"], "Beyoncé");
    let suite = p1["next_cursor"].as_str().unwrap();
    let p2 = b
        .get("alice", &format!("/artists?limit=1&cursor={suite}"))
        .await
        .json();
    assert_eq!(p2["items"][0]["name"], "Sigur Rós");
    assert!(p2["next_cursor"].is_null());
    assert_eq!(
        b.get("alice", "/artists?genre=Post-rock").await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        b.get("alice", "/artists?limit=0").await.statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        b.get("alice", "/artists?cursor=xyz").await.statut,
        StatusCode::BAD_REQUEST
    );

    let beyonce = aj["items"][0]["id"].as_str().unwrap().to_string();
    let fiche = b.get("alice", &format!("/artists/{beyonce}")).await;
    let fj = fiche.json();
    assert_eq!(fj["albums"][0]["title"], "Lemonade");
    assert_eq!(fj["albums"][0]["year"], 2016);
    assert_eq!(fj["albums"][0]["type"], "Album");
    assert_eq!(fj["albums"][0]["track_count"], 3);
    b.sans_fuite(&fiche.texte(), "/artists/{id}");

    let lemonade = b.album("Lemonade").await;
    let titres: Vec<&str> = lemonade["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["title"].as_str().unwrap())
        .collect();
    assert_eq!(titres, ["Formation", "Hold Up", "Sorry"]);
    for (titre, codec, conteneur) in [
        ("Formation", "flac", "flac"),
        ("Hold Up", "mp3", "mp3"),
        ("Sorry", "opus", "ogg"),
    ] {
        let p = piste(&lemonade, titre);
        assert_eq!(
            (
                p["format"]["codec"].as_str(),
                p["format"]["container"].as_str()
            ),
            (Some(codec), Some(conteneur)),
            "{p}"
        );
        assert!(
            p["format"]["sample_rate"]
                .as_i64()
                .is_some_and(|f| f >= 44_100),
            "{p}"
        );
        assert_eq!(p["format"]["channels"], 1, "{p}");
        assert!((p["duration"].as_f64().unwrap() - 2.0).abs() < 0.2, "{p}");
        assert_eq!(p["available"], true);
    }
    assert_eq!(piste(&lemonade, "Hold Up")["number"], 2);
    let id = piste(&lemonade, "Sorry")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = b.get("alice", &format!("/tracks/{id}")).await;
    assert_eq!(p.json()["album"]["title"], "Lemonade");
    assert_eq!(p.json()["artist"]["name"], "Beyoncé");
    b.sans_fuite(&p.texte(), "/tracks/{id}");
    assert_eq!(
        b.get("alice", "/tracks/pas-un-uuid").await.statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        b.get("alice", &format!("/tracks/{}", uuid::Uuid::new_v4()))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );

    let recents = b.get("alice", "/albums/recent").await;
    assert_eq!(recents.json()["items"].as_array().unwrap().len(), 2);
    b.sans_fuite(&recents.texte(), "/albums/recent");
    let genres = b.get("alice", "/genres").await.json();
    let noms: Vec<&str> = genres["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap())
        .collect();
    assert_eq!(noms, ["Pop", "Post-rock", "R&B"]);

    let image = b.get("alice", &format!("/artists/{beyonce}/image")).await;
    assert_eq!(
        (image.statut, image.texte().as_str()),
        (StatusCode::OK, "IMG-poster-250.jpg")
    );
    assert_eq!(image.entetes["content-type"], "image/jpeg");
    let grande = b
        .get("alice", &format!("/artists/{beyonce}/image?size=large"))
        .await;
    assert_eq!(grande.texte(), "IMG-poster.jpg");
    assert_eq!(
        b.get("alice", &format!("/artists/{beyonce}/image?type=fanart"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        b.get("alice", &format!("/artists/{beyonce}/image?size=huge"))
            .await
            .statut,
        StatusCode::BAD_REQUEST
    );
    let album = lemonade["id"].as_str().unwrap();
    let cover = b
        .get("alice", &format!("/albums/{album}/cover?size=medium"))
        .await;
    assert_eq!(
        (cover.statut, cover.texte().as_str()),
        (StatusCode::OK, "IMG-cover-500.jpg")
    );
    b.monde.lock().unwrap().en_panne = true;
    let cache = b
        .get("alice", &format!("/albums/{album}/cover?size=medium"))
        .await;
    assert_eq!(
        (cache.statut, cache.texte().as_str()),
        (StatusCode::OK, "IMG-cover-500.jpg"),
        "servie depuis le cache"
    );
    assert_eq!(
        b.get("alice", &format!("/albums/{album}/cover?size=large"))
            .await
            .statut,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn fichiers_hors_medias_ou_refuses_ecartes_avec_leur_raison() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let r = b.get("admin", "/admin/rejected").await;
    b.sans_fuite(&r.texte(), "/admin/rejected");
    let mut ecartes: Vec<(String, String)> = r.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_string(),
                f["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    ecartes.sort();
    assert_eq!(
        ecartes,
        [
            ("Freedom.flac".to_string(), "hors_medias".to_string()),
            ("Love.exe.flac".into(), "double_extension".into()),
            ("faux.flac".into(), "conteneur_incoherent".into()),
            ("lien.flac".into(), "hors_medias".into()),
        ]
    );
    let lemonade = b.album("Lemonade").await;
    for titre in [
        "Daddy Lessons",
        "Love Drought",
        "Sandcastles",
        "Freedom",
        "Forward",
    ] {
        assert!(
            lemonade["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["title"] != titre),
            "{titre}"
        );
    }
    assert_eq!(
        b.get("admin", "/admin/status").await.json()["library"]["rejected"],
        4
    );
    assert_eq!(
        b.get("alice", "/admin/rejected").await.statut,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn fichier_supprime_piste_absente_puis_retablie_avec_le_meme_identifiant() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let avant = b.album("Lemonade").await;
    let id = piste(&avant, "Formation")["id"].clone();
    let fichier = b.chemin("Beyonce/Lemonade/01 Formation.flac");
    std::fs::remove_file(&fichier).unwrap();
    b.synchro().await;
    let absent = b.album("Lemonade").await;
    let p = piste(&absent, "Formation");
    assert_eq!(
        (p["id"].clone(), p["available"].clone(), p["format"].clone()),
        (id.clone(), json!(false), Value::Null)
    );
    assert_eq!(absent["track_count"], 2);
    let id_str = id.as_str().unwrap();
    assert_eq!(
        b.get("alice", &format!("/tracks/{id_str}")).await.json()["available"],
        false
    );
    assert_eq!(
        b.get("admin", "/admin/status").await.json()["library"]["absent_tracks"],
        1
    );

    generer(&fichier, FLAC);
    {
        let mut m = b.monde.lock().unwrap();
        m.pistes.retain(|p| p["title"] != "Formation");
        m.fichiers.retain(|f| f["id"] != LIDARR + 200);
        m.piste(
            LIDARR + 120,
            LIDARR + 10,
            "Formation",
            1,
            Some((LIDARR + 220, "Beyonce/Lemonade/01 Formation.flac")),
        );
    }
    b.synchro().await;
    let retour = b.album("Lemonade").await;
    let p = piste(&retour, "Formation");
    assert_eq!((p["id"].clone(), p["available"].clone()), (id, json!(true)));
    assert_eq!(p["format"]["codec"], "flac");
    assert_eq!(
        b.get("admin", "/admin/status").await.json()["library"]["absent_tracks"],
        0
    );
}

#[tokio::test]
async fn webhook_d_import_ajoute_la_piste_et_jeton_exige() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    assert!(
        b.get("alice", "/search?q=medley").await.json()["tracks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    generer(&b.chemin("Beyonce/Beyonce/01 Medley.flac"), FLAC);
    b.monde.lock().unwrap().piste(
        LIDARR + 110,
        LIDARR + 11,
        "Beyoncé Medley",
        1,
        Some((LIDARR + 210, "Beyonce/Beyonce/01 Medley.flac")),
    );
    let corps = Some(json!({"eventType": "Download", "artist": {"id": LIDARR + 1}}));
    for chemin in ["/webhooks/lidarr", "/webhooks/lidarr?token=faux"] {
        let r = b.e.appel(None, "POST", chemin, corps.clone()).await;
        assert_eq!(r.statut, StatusCode::UNAUTHORIZED, "{chemin}");
    }
    let r =
        b.e.appel_avec(
            None,
            "POST",
            "/webhooks/lidarr",
            corps.clone(),
            &[("authorization", "Bearer faux")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::UNAUTHORIZED);
    assert!(
        b.get("alice", "/search?q=medley").await.json()["tracks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let r =
        b.e.appel(None, "POST", "/webhooks/lidarr?token=jeton-webhook", corps)
            .await;
    assert_eq!(r.statut, StatusCode::NO_CONTENT);
    let limite = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while b.get("alice", "/search?q=medley").await.json()["tracks"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        assert!(std::time::Instant::now() < limite, "piste importée absente");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let sansjeton = env_avec(|c| c.jeton_webhook = String::new()).await.unwrap();
    let r = sansjeton
        .appel(None, "POST", "/webhooks/lidarr?token=", None)
        .await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn recherche_sans_accents_par_categorie() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    generer(&b.chemin("Beyonce/Beyonce/01 Medley.flac"), FLAC);
    b.monde.lock().unwrap().piste(
        LIDARR + 110,
        LIDARR + 11,
        "Beyoncé Medley",
        1,
        Some((LIDARR + 210, "Beyonce/Beyonce/01 Medley.flac")),
    );
    b.synchro().await;
    let r = b.get("alice", "/search?q=beyonce").await;
    assert_eq!(r.statut, StatusCode::OK);
    let s = r.json();
    let noms = |cat: &str, champ: &str| -> Vec<String> {
        s[cat]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x[champ].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(noms("artists", "name"), ["Beyoncé"]);
    assert_eq!(noms("albums", "title"), ["Beyoncé"]);
    assert_eq!(noms("tracks", "title"), ["Beyoncé Medley"]);
    b.sans_fuite(&r.texte(), "/search");
    for q in ["BEYONCÉ", "glosoli", "sigur%20ros", "hold", "takk"] {
        let s = b.get("alice", &format!("/search?q={q}")).await.json();
        let total: usize = ["artists", "albums", "tracks"]
            .iter()
            .map(|c| s[c].as_array().unwrap().len())
            .sum();
        assert!(total > 0, "{q} : {s}");
    }
    let s = b.get("alice", "/search?q=%25").await.json();
    assert!(
        ["artists", "albums", "tracks"]
            .iter()
            .all(|c| s[c].as_array().unwrap().is_empty()),
        "« % » échappé : {s}"
    );
    assert_eq!(
        b.get("alice", "/search?q=").await.statut,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        b.get("alice", &format!("/search?q={}", "a".repeat(101)))
            .await
            .statut,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn lidarr_en_panne_ou_dossier_inaccessible_rien_n_est_retire() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let compte = |e: &Value| {
        (
            e["library"]["tracks"].clone(),
            e["library"]["absent_tracks"].clone(),
        )
    };
    assert_eq!(
        compte(&b.get("admin", "/admin/status").await.json()),
        (json!(4), json!(0))
    );
    b.monde.lock().unwrap().en_panne = true;
    b.synchro().await;
    let etat = b.get("admin", "/admin/status").await;
    let ej = etat.json();
    assert_eq!(compte(&ej), (json!(4), json!(0)));
    assert_eq!(ej["sync"][0]["error"], "Lidarr : HTTP 500", "{ej}");
    assert!(ej["sync"][0]["succeeded_at"].is_string());
    b.sans_fuite(&etat.texte(), "/admin/status");
    assert_eq!(b.album("Lemonade").await["track_count"], 3);

    b.monde.lock().unwrap().en_panne = false;
    let mut cfg = b.e.state.config().clone();
    let absent = b.medias.path().join("demonte");
    cfg.chemins = correspondances(&format!("/music={}", absent.display())).unwrap();
    cfg.medias = absent;
    crate::catalogue::synchroniser(&AppState::new(cfg, b.e.state.pool().clone())).await;
    let ej = b.get("admin", "/admin/status").await.json();
    assert_eq!(compte(&ej), (json!(4), json!(0)));
    assert!(
        ej["sync"][0]["error"]
            .as_str()
            .unwrap()
            .contains("inaccessible"),
        "{ej}"
    );
    b.synchro().await;
    assert!(b.get("admin", "/admin/status").await.json()["sync"][0]["error"].is_null());
}

#[tokio::test]
async fn synchronisation_demandee_pendant_une_autre_relancee() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    let verrou = b.e.state.verrou_synchro().lock().await;
    let r = b.get("admin", "/admin/sync").await;
    assert_eq!(r.statut, StatusCode::METHOD_NOT_ALLOWED);
    let r = b.e.appel(Some("admin"), "POST", "/admin/sync", None).await;
    assert_eq!(r.statut, StatusCode::ACCEPTED);
    crate::catalogue::synchroniser(&b.e.state).await;
    assert!(b.get("admin", "/admin/status").await.json()["running"] == true);
    assert_eq!(b.lidarr.appels(), 0);
    drop(verrou);
    b.e.state.synchro_a_relancer();
    crate::catalogue::synchroniser(&b.e.state).await;
    assert!(!b.e.state.synchro_relancee());
    assert_eq!(
        b.get("admin", "/admin/status").await.json()["library"]["tracks"],
        4
    );
}

#[tokio::test]
async fn albums_jamais_ecoutes_tries_par_taille() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let lemonade = b.album("Lemonade").await;
    let takk = b.album("Takk…").await;
    let titres = |r: Rep| -> Vec<String> {
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let j = r.json();
        let items = j["items"].as_array().unwrap();
        let tailles: Vec<i64> = items.iter().map(|a| a["size"].as_i64().unwrap()).collect();
        assert!(tailles.windows(2).all(|t| t[0] >= t[1]), "{j}");
        items
            .iter()
            .map(|a| a["title"].as_str().unwrap().to_string())
            .collect()
    };
    let tous = titres(b.get("admin", "/admin/albums").await);
    assert!(tous.contains(&"Lemonade".to_string()) && tous.contains(&"Takk…".to_string()));
    assert_eq!(
        titres(b.get("admin", "/admin/albums?never_played=true").await),
        tous
    );
    let ecouter = |album: &Value, titre: &str, secondes: f64, fin: &str| json!({"track": piste(album, titre)["id"], "played": secondes, "ended_by": fin});
    let r =
        b.e.appel(
            Some("bruno"),
            "POST",
            "/history",
            Some(ecouter(&takk, "Glósóli", 0.2, "skip")),
        )
        .await;
    assert_eq!(r.json()["counted"], false, "{}", r.texte());
    let r =
        b.e.appel(
            Some("alice"),
            "POST",
            "/history",
            Some(ecouter(&lemonade, "Formation", 2.0, "end")),
        )
        .await;
    assert_eq!(r.json()["counted"], true, "{}", r.texte());
    let jamais = titres(b.get("admin", "/admin/albums?never_played=true").await);
    assert!(jamais.contains(&"Takk…".to_string()), "{jamais:?}");
    assert!(!jamais.contains(&"Lemonade".to_string()), "{jamais:?}");
    assert_eq!(
        titres(b.get("admin", "/admin/albums?never_played=false").await),
        tous
    );
    let r = b.get("admin", "/admin/albums?never_played=oui").await;
    assert_eq!(r.statut, StatusCode::BAD_REQUEST);
    assert_eq!(
        b.get("alice", "/admin/albums?never_played=true")
            .await
            .statut,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn apercu_interne_reserve_au_jeton_de_service_et_a_sillon_access() {
    let Some(b) = bibliotheque().await else {
        return;
    };
    b.synchro().await;
    let album = b.album("Lemonade").await;
    let (album_id, artiste_id) = (
        album["id"].as_str().unwrap().to_string(),
        album["artist"]["id"].as_str().unwrap().to_string(),
    );
    let piste_id = piste(&album, "Formation")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let apercu = |jeton: &'static str, requete: String| {
        let e = &b.e;
        async move {
            e.appel_avec(
                None,
                "GET",
                &format!("/internal/preview?{requete}"),
                None,
                &[("authorization", jeton)],
            )
            .await
        }
    };
    let ok = "Bearer jeton-service";
    let r = apercu(ok, format!("type=album&id={album_id}&sub=sub-alice")).await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let pochette = if album["cover"] == true {
        json!({"kind": "album", "id": album_id})
    } else {
        Value::Null
    };
    assert_eq!(
        r.json(),
        json!({"type": "album", "id": album_id, "title": "Lemonade", "artist": album["artist"]["name"], "album": album_id, "cover": pochette})
    );
    b.sans_fuite(&r.texte(), "aperçu d'album");
    let r = apercu(ok, format!("type=track&id={piste_id}&sub=sub-alice")).await;
    assert_eq!(r.json()["title"], "Formation");
    assert_eq!(r.json()["cover"], pochette);
    assert_eq!(r.json()["album"], album_id.as_str());
    let r = apercu(ok, format!("type=artist&id={artiste_id}&sub=sub-alice")).await;
    assert_eq!(r.json()["type"], "artist");
    assert_eq!(r.json()["title"], album["artist"]["name"]);
    assert_eq!(r.json()["album"], Value::Null);
    for qui in ["sub-personne", "sub-inconnu"] {
        let r = apercu(ok, format!("type=album&id={album_id}&sub={qui}")).await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN, "{qui}");
        assert!(!r.texte().contains("Lemonade"));
    }
    for jeton in ["", "Bearer faux", "Bearer jeton-webhook"] {
        let r = apercu(jeton, format!("type=album&id={album_id}&sub=sub-alice")).await;
        assert_eq!(r.statut, StatusCode::UNAUTHORIZED, "{jeton}");
    }
    let liste =
        b.e.appel(
            Some("alice"),
            "POST",
            "/playlists",
            Some(json!({"name": "Secrète"})),
        )
        .await
        .json()["id"]
            .as_str()
            .unwrap()
            .to_string();
    let existante = apercu(ok, format!("type=playlist&id={liste}&sub=sub-alice")).await;
    let absente = apercu(
        ok,
        "type=playlist&id=00000000-0000-0000-0000-000000000009&sub=sub-alice".into(),
    )
    .await;
    assert_eq!(existante.statut, StatusCode::NOT_FOUND);
    assert_eq!(
        (existante.statut, existante.corps.clone()),
        (absente.statut, absente.corps)
    );
    assert!(!existante.texte().contains("Secrète"));
    let r = apercu(
        ok,
        "type=album&id=00000000-0000-0000-0000-000000000009&sub=sub-alice".into(),
    )
    .await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
    for requete in [
        format!("type=genre&id={album_id}&sub=sub-alice"),
        format!("type=album&id={album_id}"),
    ] {
        let r = apercu(ok, requete.clone()).await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{requete}");
    }
    let sans = env_avec(|c| c.jeton_service = String::new()).await.unwrap();
    let r = sans
        .appel_avec(
            None,
            "GET",
            "/internal/preview?type=album&id=x&sub=sub-alice",
            None,
            &[("authorization", "Bearer ")],
        )
        .await;
    assert_eq!(r.statut, StatusCode::NOT_FOUND);
}
