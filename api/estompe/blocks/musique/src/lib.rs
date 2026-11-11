mod analyse;
mod cache;
mod oembed;
mod youtube;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use estompe_core::{
    api::ActiveUser,
    http::{write_err, write_json},
    openapi, sniff,
    state::AppState,
};
use serde_json::{Value, json};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

pub use analyse::{Lien, analyser_url, analyser_youtube_music, est_id_video, id_video_canonique};
use cache::Cache;
pub use youtube::Liste;
use youtube::sans_topic;

const PAR_MINUTE: usize = 30;
const POCHETTE_MAX: usize = 512 << 10;
const TITRE_GENERIQUE: &str = "Morceau YouTube Music";
const LIEN_INVALIDE: &str = "lien non reconnu : seuls les morceaux, albums et playlists de YouTube Music (music.youtube.com) sont acceptés";
const SANS_API: &str =
    "les albums et playlists demandent la clé YouTube du serveur (YOUTUBE_API_KEY)";
const MINIATURES: &str = "https://i.ytimg.com";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MusicMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
    pub cover: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piste {
    pub id: String,
    pub meta: MusicMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erreur {
    SansApi,
    Autre(String),
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SansApi => f.write_str(SANS_API),
            Self::Autre(m) => f.write_str(m),
        }
    }
}

pub struct Musique {
    http: reqwest::Client,
    api: reqwest::Client,
    youtube: String,
    oembed: oembed::Oembed,
    miniatures: String,
    cache: Cache,
}

impl Default for Musique {
    fn default() -> Self {
        Self::new()
    }
}

fn chemin_pochette(id: &str) -> String {
    format!("/music/cover/{id}")
}

impl Musique {
    pub fn new() -> Self {
        Self::avec_bases(youtube::BASE, oembed::BASE, MINIATURES)
    }

    pub fn avec_bases(youtube: &str, oembed: &str, miniatures: &str) -> Self {
        let http = youtube::http();
        Self {
            oembed: oembed::Oembed::new(oembed, http.clone()),
            http,
            api: youtube::http_api(),
            youtube: youtube.to_string(),
            miniatures: miniatures.trim_end_matches('/').to_string(),
            cache: Cache::default(),
        }
    }

    fn client(&self, cle: &str) -> Option<youtube::Client> {
        (!cle.is_empty()).then(|| youtube::Client::new(cle, &self.youtube, self.api.clone()))
    }

    async fn repli(&self, id: &str) -> MusicMeta {
        let mut meta = MusicMeta {
            title: TITRE_GENERIQUE.into(),
            cover: chemin_pochette(id),
            ..MusicMeta::default()
        };
        match self.oembed.recuperer(id).await {
            Ok(info) if !info.titre.is_empty() => {
                meta.title = info.titre;
                meta.artist = sans_topic(&info.auteur);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("musique : oEmbed de {id} : {e}"),
        }
        meta
    }

    pub async fn pistes(&self, cle: &str, ids: &[String]) -> Vec<Piste> {
        let maintenant = Instant::now();
        let mut connus = HashMap::new();
        let mut manquants: Vec<String> = Vec::new();
        for id in ids {
            match self.cache.meta(id, maintenant) {
                Some(m) => {
                    connus.insert(id.clone(), m);
                }
                None => manquants.push(id.clone()),
            }
        }
        if !manquants.is_empty() {
            let mut frais = HashMap::new();
            if let Some(client) = self.client(cle) {
                match client.videos(&manquants).await {
                    Ok(titres) => {
                        for t in titres {
                            let meta = MusicMeta {
                                title: t.titre,
                                artist: t.artiste,
                                album: t.album,
                                duration: t.duree,
                                cover: chemin_pochette(&t.id),
                            };
                            frais.insert(t.id, meta);
                        }
                    }
                    Err(e) => tracing::warn!("musique : métadonnées : {e}"),
                }
            }
            for id in &manquants {
                if !frais.contains_key(id) {
                    let meta = self.repli(id).await;
                    frais.insert(id.clone(), meta);
                }
            }
            self.cache.ranger_meta(&frais, maintenant);
            connus.extend(frais);
        }
        ids.iter()
            .map(|id| Piste {
                id: id.clone(),
                meta: connus.get(id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    pub async fn resoudre(&self, cle: &str, lien: &Lien) -> Result<(String, Vec<Piste>), Erreur> {
        match lien {
            Lien::Piste(id) => Ok((
                String::new(),
                self.pistes(cle, std::slice::from_ref(id)).await,
            )),
            Lien::Liste(id) => self.resoudre_liste(cle, id).await,
        }
    }

    pub async fn carte_liste(&self, cle: &str, id: &str) -> Result<Liste, Erreur> {
        let client = self.client(cle).ok_or(Erreur::SansApi)?;
        client.liste(id).await.map_err(|e| {
            tracing::warn!("musique : liste {id} : {e}");
            Erreur::Autre("liste introuvable ou privée".into())
        })
    }

    async fn resoudre_liste(&self, cle: &str, id: &str) -> Result<(String, Vec<Piste>), Erreur> {
        let client = self.client(cle).ok_or(Erreur::SansApi)?;
        let liste = client.liste(id).await.map_err(|e| {
            tracing::warn!("musique : liste {id} : {e}");
            Erreur::Autre("liste introuvable ou privée".into())
        })?;
        if liste.ids.is_empty() {
            return Err(Erreur::Autre("liste vide".into()));
        }
        let mut pistes = self.pistes(cle, &liste.ids).await;
        if id.starts_with("OLAK5uy_") {
            for p in &mut pistes {
                if p.meta.album.is_empty() {
                    p.meta.album.clone_from(&liste.titre);
                }
            }
        }
        Ok((liste.titre, pistes))
    }

    async fn pochette(&self, id: &str) -> Response {
        if let Some(octets) = self.cache.pochette(id) {
            return pochette_jpeg(&octets);
        }
        let indisponible = || {
            write_err(
                StatusCode::BAD_GATEWAY,
                "cover_unavailable",
                "pochette indisponible",
            )
        };
        let Ok(reponse) = self
            .http
            .get(format!("{}/vi/{id}/hqdefault.jpg", self.miniatures))
            .send()
            .await
        else {
            return indisponible();
        };
        if reponse.status() != reqwest::StatusCode::OK {
            return introuvable();
        }
        let Ok(octets) = youtube::lire_limite(reponse, POCHETTE_MAX + 1).await else {
            return indisponible();
        };
        if octets.is_empty()
            || octets.len() > POCHETTE_MAX
            || sniff::detecter(&octets) != "image/jpeg"
        {
            return indisponible();
        }
        pochette_jpeg(&self.cache.ranger_pochette(id, octets))
    }
}

fn introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

fn pochette_jpeg(octets: &[u8]) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
        ],
        octets.to_vec(),
    )
        .into_response()
}

fn piste_json(p: &Piste) -> Value {
    json!({
        "id": p.id,
        "title": p.meta.title,
        "artist": p.meta.artist,
        "album": p.meta.album,
        "duration": p.meta.duration,
        "cover": p.meta.cover,
    })
}

fn trop_de_recherches() -> Response {
    let mut r = write_err(
        StatusCode::TOO_MANY_REQUESTS,
        "too_many_requests",
        "trop de recherches",
    );
    r.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    r
}

fn autorise(state: &AppState, utilisateur: &str) -> bool {
    state
        .limiteurs()
        .recherche
        .allow(utilisateur, PAR_MINUTE, Duration::from_secs(60))
}

fn sans_api() -> Response {
    write_err(StatusCode::SERVICE_UNAVAILABLE, "no_music_api", SANS_API)
}

fn param<'a>(params: &'a [(String, String)], nom: &str) -> &'a str {
    params
        .iter()
        .find(|(k, _)| k == nom)
        .map_or("", |(_, v)| v.as_str())
}

pub fn routes(musique: Arc<Musique>) -> OpenApiRouter<AppState> {
    let routeur = OpenApiRouter::new()
        .route("/music/search", {
            let musique = musique.clone();
            get(
                move |state: State<AppState>,
                      user: ActiveUser,
                      params: Query<Vec<(String, String)>>| {
                    rechercher(state, musique, user, params)
                },
            )
        })
        .route("/music/resolve", {
            let musique = musique.clone();
            get(
                move |state: State<AppState>,
                      user: ActiveUser,
                      params: Query<Vec<(String, String)>>| {
                    resoudre(state, musique, user, params)
                },
            )
        })
        .route(
            "/music/cover/{id}",
            get(move |id: Path<String>| pochette(musique, id)),
        );
    let routeur = openapi::documenter(
        routeur,
        "/music/search",
        HttpMethod::Get,
        openapi::operation(
            "vocal",
            "Recherche de morceaux (YouTube Music)",
            &[
                ("200", "ok"),
                ("400", "bad_request"),
                ("429", "too_many_requests"),
                ("502", "music_unavailable"),
                ("503", "no_music_api"),
            ],
            "/music/search",
        ),
    );
    let routeur = openapi::documenter(
        routeur,
        "/music/resolve",
        HttpMethod::Get,
        openapi::operation(
            "vocal",
            "Morceaux d'un lien YouTube Music (morceau, album ou playlist)",
            &[
                ("200", "ok"),
                ("400", "bad_request"),
                ("404", "not_found"),
                ("429", "too_many_requests"),
                ("503", "no_music_api"),
            ],
            "/music/resolve",
        ),
    );
    openapi::documenter(
        routeur,
        "/music/cover/{id}",
        HttpMethod::Get,
        openapi::operation(
            "vocal",
            "Pochette d'un morceau (image JPEG)",
            &[
                ("200", "ok"),
                ("404", "not_found"),
                ("502", "cover_unavailable"),
            ],
            "/music/cover/{id}",
        ),
    )
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes(Arc::new(Musique::new())))
}

async fn resoudre(
    State(state): State<AppState>,
    musique: Arc<Musique>,
    ActiveUser(user): ActiveUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    let Some(lien) = analyser_youtube_music(param(&params, "url")) else {
        return write_err(StatusCode::BAD_REQUEST, "bad_request", LIEN_INVALIDE);
    };
    if !autorise(&state, &user.id) {
        return trop_de_recherches();
    }
    match musique.resoudre(&state.config().youtube_key, &lien).await {
        Ok((titre, pistes)) => {
            let pistes: Vec<Value> = pistes.iter().map(piste_json).collect();
            write_json(StatusCode::OK, &json!({ "title": titre, "tracks": pistes }))
        }
        Err(Erreur::SansApi) => sans_api(),
        Err(e) => write_err(StatusCode::NOT_FOUND, "not_found", &e.to_string()),
    }
}

async fn rechercher(
    State(state): State<AppState>,
    musique: Arc<Musique>,
    ActiveUser(user): ActiveUser,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    let q = param(&params, "q").trim();
    let longueur = q.chars().count();
    if !(2..=100).contains(&longueur) {
        return write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "recherche : 2 à 100 caractères",
        );
    }
    let cle = state.config().youtube_key.as_str();
    let Some(client) = musique.client(cle) else {
        return sans_api();
    };
    if !autorise(&state, &user.id) {
        return trop_de_recherches();
    }
    let ids = match client.rechercher(q).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("musique : recherche : {e}");
            return write_err(
                StatusCode::BAD_GATEWAY,
                "music_unavailable",
                "recherche indisponible",
            );
        }
    };
    let pistes: Vec<Value> = musique
        .pistes(cle, &ids)
        .await
        .iter()
        .map(piste_json)
        .collect();
    write_json(StatusCode::OK, &Value::Array(pistes))
}

async fn pochette(musique: Arc<Musique>, Path(id): Path<String>) -> Response {
    let Some(id) = id_video_canonique(&id) else {
        return introuvable();
    };
    musique.pochette(&id).await
}

#[cfg(test)]
mod tests {
    use super::{Musique, PAR_MINUTE, routes};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
    };
    use estompe_core::testutil::BasePostgres;
    use estompe_core::{
        db::migrate_embarquees,
        state::AppState,
        testutil::{self, Faux, faux, faux_aegis},
    };
    use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tower::ServiceExt;

    const PISTE: &str = "https://music.youtube.com/watch?v=dQw4w9WgXcQ";
    const ALBUM: &str = "https://music.youtube.com/playlist?list=OLAK5uy_album123456";

    struct Env {
        app: Router,
        youtube: Faux,
        oembed: Faux,
        miniatures: Faux,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    fn jpeg(taille: usize) -> Vec<u8> {
        let mut octets = vec![0xFF, 0xD8, 0xFF, 0xE0];
        octets.resize(taille, 0);
        octets
    }

    fn parametre(query: &str, nom: &str) -> String {
        query
            .split('&')
            .find_map(|p| p.strip_prefix(&format!("{nom}=")))
            .map(|v| {
                percent_decode_str(&v.replace('+', " "))
                    .decode_utf8_lossy()
                    .into_owned()
            })
            .unwrap_or_default()
    }

    async fn env(cle: &str) -> Option<Env> {
        env_avec(cle, None).await
    }

    async fn env_avec(cle: &str, miniatures_ko: Option<&str>) -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            if nom == "anonyme" {
                return (401, vec![]);
            }
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_aegis(|_, _| true).await;
        let youtube = faux(|r| {
            if parametre(&r.query, "key") != "bonne" {
                return (403, vec![]);
            }
            let corps = match r.chemin.as_str() {
                "/videos" => {
                    let items: Vec<Value> = parametre(&r.query, "id")
                        .split(',')
                        .filter(|id| !id.starts_with("absent"))
                        .map(|id| {
                            json!({
                                "id": id,
                                "snippet": {
                                    "title": format!("Titre {id}"),
                                    "description": "",
                                    "channelTitle": "Artiste - Topic",
                                    "thumbnails": {}
                                },
                                "contentDetails": { "duration": "PT3M25S" }
                            })
                        })
                        .collect();
                    json!({ "items": items })
                }
                "/playlists" => match parametre(&r.query, "id").as_str() {
                    "OLAK5uy_album123456" | "PLvide0000000" => json!({ "items": [{
                        "snippet": { "title": "Mon album", "channelTitle": "Artiste - Topic", "thumbnails": {} },
                        "contentDetails": { "itemCount": 2 }
                    }] }),
                    _ => json!({ "items": [] }),
                },
                "/playlistItems" => {
                    if parametre(&r.query, "playlistId") == "PLvide0000000" {
                        json!({ "items": [] })
                    } else {
                        json!({ "items": [
                            { "contentDetails": { "videoId": "aaaaaaaaaaa" } },
                            { "contentDetails": { "videoId": "bbbbbbbbbbb" } }
                        ] })
                    }
                }
                "/search" => {
                    if parametre(&r.query, "q") == "casse" {
                        return (500, vec![]);
                    }
                    let items: Vec<Value> = (0..10)
                        .map(|i| json!({ "id": { "videoId": format!("recherche{i:02}") } }))
                        .collect();
                    json!({ "items": if parametre(&r.query, "q") == "rien" { vec![] } else { items } })
                }
                _ => return (404, vec![]),
            };
            (200, corps.to_string().into_bytes())
        })
        .await;
        let oembed = faux(|r| {
            if parametre(&r.query, "url").ends_with("zzzzzzzzzzz") {
                return (500, vec![]);
            }
            (
                200,
                json!({ "title": "Titre oEmbed", "author_name": "Chaîne - Topic" })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let miniatures = faux(|r| match r.chemin.as_str() {
            "/vi/aaaaaaaaaaa/hqdefault.jpg" => (200, jpeg(2000)),
            "/vi/bbbbbbbbbbb/hqdefault.jpg" => (404, vec![]),
            "/vi/ccccccccccc/hqdefault.jpg" => (200, b"<html>pas une image</html>".to_vec()),
            "/vi/ddddddddddd/hqdefault.jpg" => (200, vec![]),
            "/vi/eeeeeeeeeee/hqdefault.jpg" => (200, jpeg((512 << 10) + 1)),
            "/vi/fffffffffff/hqdefault.jpg" => (200, jpeg(512 << 10)),
            _ => (404, vec![]),
        })
        .await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("AEGIS_URL", permission.url.as_str()),
                ("YOUTUBE_API_KEY", cle),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let base_miniatures = miniatures_ko.unwrap_or(miniatures.url.as_str());
        let musique = Musique::avec_bases(&youtube.url, &oembed.url, base_miniatures);
        let app = Router::from(routes(Arc::new(musique))).with_state(state);
        Some(Env {
            app,
            youtube,
            oembed,
            miniatures,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn brut(
        app: &Router,
        bearer: Option<&str>,
        chemin: &str,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut r = Request::get(chemin);
        if let Some(b) = bearer {
            r = r.header(header::AUTHORIZATION, format!("Bearer {b}"));
        }
        let rep = app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let corps = to_bytes(rep.into_body(), 4 << 20).await.unwrap();
        (statut, entetes, corps.to_vec())
    }

    async fn appel(
        app: &Router,
        bearer: Option<&str>,
        chemin: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        let (s, h, c) = brut(app, bearer, chemin).await;
        (s, h, serde_json::from_slice(&c).unwrap_or(Value::Null))
    }

    fn resolve(url: &str) -> String {
        format!(
            "/music/resolve?url={}",
            utf8_percent_encode(url, NON_ALPHANUMERIC)
        )
    }

    fn search(q: &str) -> String {
        format!(
            "/music/search?q={}",
            utf8_percent_encode(q, NON_ALPHANUMERIC)
        )
    }

    #[tokio::test]
    async fn resolve_une_piste_sans_cle_par_oembed() {
        let Some(e) = env("").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(PISTE)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c["title"], "");
        assert_eq!(
            c["tracks"],
            json!([{
                "id": "dQw4w9WgXcQ",
                "title": "Titre oEmbed",
                "artist": "Chaîne",
                "album": "",
                "duration": 0,
                "cover": "/music/cover/dQw4w9WgXcQ"
            }])
        );
        assert_eq!(e.youtube.appels(), 0);
    }

    #[tokio::test]
    async fn resolve_repli_generique_quand_oembed_echoue() {
        let Some(e) = env("").await else { return };
        let url = "https://music.youtube.com/watch?v=zzzzzzzzzzz";
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(url)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c["tracks"][0]["title"], "Morceau YouTube Music");
        assert_eq!(c["tracks"][0]["artist"], "");
        assert_eq!(c["tracks"][0]["cover"], "/music/cover/zzzzzzzzzzz");
    }

    #[tokio::test]
    async fn resolve_une_piste_avec_cle_et_met_en_cache() {
        let Some(e) = env("bonne").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(PISTE)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c["tracks"][0]["title"], "Titre dQw4w9WgXcQ");
        assert_eq!(c["tracks"][0]["artist"], "Artiste");
        assert_eq!(c["tracks"][0]["duration"], 205);
        assert_eq!(e.youtube.appels(), 1);
        let (s, _, c2) = appel(&e.app, Some("bob"), &resolve(PISTE)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c2, c);
        assert_eq!(e.youtube.appels(), 1);
        assert_eq!(e.oembed.appels(), 0);
    }

    #[tokio::test]
    async fn resolve_une_cle_refusee_retombe_sur_oembed() {
        let Some(e) = env("mauvaise").await else {
            return;
        };
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(PISTE)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c["tracks"][0]["title"], "Titre oEmbed");
        assert_eq!(e.oembed.appels(), 1);
    }

    #[tokio::test]
    async fn resolve_un_album_remplit_l_album_et_garde_l_ordre() {
        let Some(e) = env("bonne").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(ALBUM)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c["title"], "Mon album");
        let ids: Vec<&str> = c["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["aaaaaaaaaaa", "bbbbbbbbbbb"]);
        assert!(
            c["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["album"] == "Mon album")
        );
    }

    #[tokio::test]
    async fn resolve_une_liste_sans_cle_donne_503() {
        let Some(e) = env("").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &resolve(ALBUM)).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::SERVICE_UNAVAILABLE, Some("no_music_api"))
        );
        assert_eq!(
            c["message"],
            "les albums et playlists demandent la clé YouTube du serveur (YOUTUBE_API_KEY)"
        );
    }

    #[tokio::test]
    async fn resolve_liste_introuvable_ou_vide_donne_404() {
        let Some(e) = env("bonne").await else { return };
        for (url, message) in [
            (
                "https://music.youtube.com/playlist?list=PLintrouvable0",
                "liste introuvable ou privée",
            ),
            (
                "https://music.youtube.com/playlist?list=PLvide0000000",
                "liste vide",
            ),
        ] {
            let (s, _, c) = appel(&e.app, Some("alice"), &resolve(url)).await;
            assert_eq!(
                (s, c["error"].as_str(), c["message"].as_str()),
                (StatusCode::NOT_FOUND, Some("not_found"), Some(message)),
                "{url}"
            );
        }
    }

    #[tokio::test]
    async fn resolve_refuse_les_liens_invalides_et_les_inconnus() {
        let Some(e) = env("bonne").await else { return };
        for url in [
            "",
            "n'importe quoi",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/playlist?list=OLAK5uy_album123456",
        ] {
            let (s, _, c) = appel(&e.app, Some("alice"), &resolve(url)).await;
            assert_eq!(
                (s, c["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("bad_request")),
                "{url}"
            );
            assert_eq!(
                c["message"],
                "lien non reconnu : seuls les morceaux, albums et playlists de YouTube Music (music.youtube.com) sont acceptés"
            );
        }
        assert_eq!(
            appel(&e.app, None, &resolve(PISTE)).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            appel(&e.app, Some("jamais-active"), &resolve(PISTE))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn search_valide_la_longueur_en_caracteres() {
        let Some(e) = env("bonne").await else { return };
        for q in ["", "a", " a ", &"a".repeat(101)] {
            let (s, _, c) = appel(&e.app, Some("alice"), &search(q)).await;
            assert_eq!(
                (s, c["error"].as_str(), c["message"].as_str()),
                (
                    StatusCode::BAD_REQUEST,
                    Some("bad_request"),
                    Some("recherche : 2 à 100 caractères")
                ),
                "{q:?}"
            );
        }
        let (s, _, _) = appel(&e.app, Some("alice"), &search(&"é".repeat(100))).await;
        assert_eq!(s, StatusCode::OK);
        let (s, _, _) = appel(&e.app, Some("alice"), &search("ab")).await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn search_sans_cle_ou_amont_en_erreur() {
        let Some(e) = env("").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &search("daft punk")).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::SERVICE_UNAVAILABLE, Some("no_music_api"))
        );
        let Some(e) = env("bonne").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &search("casse")).await;
        assert_eq!(
            (s, c["error"].as_str(), c["message"].as_str()),
            (
                StatusCode::BAD_GATEWAY,
                Some("music_unavailable"),
                Some("recherche indisponible")
            )
        );
        assert_eq!(
            appel(&e.app, None, &search("daft punk")).await.0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn search_renvoie_au_plus_huit_morceaux() {
        let Some(e) = env("bonne").await else { return };
        let (s, _, c) = appel(&e.app, Some("alice"), &search("daft punk")).await;
        assert_eq!(s, StatusCode::OK);
        let pistes = c.as_array().unwrap();
        assert_eq!(pistes.len(), 8);
        assert_eq!(pistes[0]["id"], "recherche00");
        assert_eq!(pistes[0]["title"], "Titre recherche00");
        let (s, _, c) = appel(&e.app, Some("alice"), &search("rien")).await;
        assert_eq!((s, c), (StatusCode::OK, json!([])));
    }

    #[tokio::test]
    async fn la_recherche_est_limitee_par_utilisateur() {
        let Some(e) = env("").await else { return };
        for _ in 0..PAR_MINUTE {
            assert_eq!(
                appel(&e.app, Some("alice"), &resolve(PISTE)).await.0,
                StatusCode::OK
            );
        }
        let (s, h, c) = appel(&e.app, Some("alice"), &resolve(PISTE)).await;
        assert_eq!(h[header::RETRY_AFTER], "60");
        assert_eq!(
            (s, c["error"].as_str(), c["message"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("too_many_requests"),
                Some("trop de recherches")
            )
        );
        assert_eq!(
            appel(&e.app, Some("bob"), &resolve(PISTE)).await.0,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn resolve_et_search_partagent_le_quota() {
        let Some(e) = env("bonne").await else { return };
        for _ in 0..PAR_MINUTE - 1 {
            assert_eq!(
                appel(&e.app, Some("alice"), &resolve(PISTE)).await.0,
                StatusCode::OK
            );
        }
        assert_eq!(
            appel(&e.app, Some("alice"), &search("daft punk")).await.0,
            StatusCode::OK
        );
        assert_eq!(
            appel(&e.app, Some("alice"), &search("daft punk")).await.0,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            appel(&e.app, Some("alice"), &resolve(PISTE)).await.0,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn cover_sert_le_jpeg_publiquement_et_le_met_en_cache() {
        let Some(e) = env("").await else { return };
        for _ in 0..2 {
            let (s, h, c) = brut(&e.app, None, "/music/cover/aaaaaaaaaaa").await;
            assert_eq!(s, StatusCode::OK);
            assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
            assert_eq!(h[header::CACHE_CONTROL], "public, max-age=86400");
            assert_eq!(c, jpeg(2000));
        }
        assert_eq!(e.miniatures.appels(), 1);
        let (s, _, _) = brut(&e.app, None, "/music/cover/fffffffffff").await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn cover_refuse_les_identifiants_et_les_images_invalides() {
        let Some(e) = env("").await else { return };
        for id in ["court", "dQw4w9WgXcQa", "dQw4w9WgXc%21"] {
            let (s, _, c) = appel(&e.app, None, &format!("/music/cover/{id}")).await;
            assert_eq!(
                (s, c["error"].as_str(), c["message"].as_str()),
                (
                    StatusCode::NOT_FOUND,
                    Some("not_found"),
                    Some("introuvable")
                ),
                "{id}"
            );
        }
        assert_eq!(e.miniatures.appels(), 0);
        let (s, _, _) = appel(&e.app, None, "/music/cover/bbbbbbbbbbb").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        for id in ["ccccccccccc", "ddddddddddd", "eeeeeeeeeee"] {
            let (s, _, c) = appel(&e.app, None, &format!("/music/cover/{id}")).await;
            assert_eq!(
                (s, c["error"].as_str(), c["message"].as_str()),
                (
                    StatusCode::BAD_GATEWAY,
                    Some("cover_unavailable"),
                    Some("pochette indisponible")
                ),
                "{id}"
            );
        }
    }

    #[tokio::test]
    async fn cover_amont_injoignable_donne_502() {
        let Some(e) = env_avec("", Some("http://127.0.0.1:1")).await else {
            return;
        };
        let (s, _, c) = appel(&e.app, None, "/music/cover/aaaaaaaaaaa").await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_GATEWAY, Some("cover_unavailable"))
        );
    }
}
