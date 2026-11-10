#![allow(clippy::result_large_err)]

mod entracte;
mod lien;
mod session;

use std::{
    sync::{Arc, Once, OnceLock},
    time::{Duration, SystemTime},
};

use axum::{
    Extension,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use estompe_block_musique::{Musique, analyser_youtube_music};
use estompe_core::{
    api::ActiveUser,
    http::{champ, champ_texte, decode, internal, write_err, write_json},
    messages::{self, Conv},
    openapi, roles,
    state::{AppState, FilmResolu, RefusFilm},
    users::User,
};
use serde_json::{Map, Value};
pub use session::Utilisateur;
use session::{Action, Etat, Item, Meta, Refus, Resultat, Sessions, vue};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const ACTIONS_MAX: usize = 30;
const FENETRE: Duration = Duration::from_secs(10);
const BOT: &str = "00000000-0000-0000-0000-000000000001";
const VIDEO_INVALIDE: &str = "lien non reconnu : YouTube, YouTube Music, Twitch, Vimeo, Dailymotion ou fichier vidéo (MP4, WebM, HLS)";
const PISTE_INVALIDE: &str = "lien non reconnu : seuls les morceaux, albums et playlists de YouTube Music (music.youtube.com) sont acceptés";
const FILM_INVALIDE: &str =
    "seuls les films et séries de la bibliothèque se regardent dans un salon cinéma";
const FILM_HORS_CINEMA: &str = "les films et séries se regardent dans un salon cinéma";
const FILE_INVALIDE: &str = "file d'attente invalide : 50 liens reconnus au plus";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Genre {
    Watch,
    Music,
    Cinema,
}

impl Genre {
    fn chemin(self) -> &'static str {
        match self {
            Genre::Watch => "watch",
            Genre::Music => "music",
            Genre::Cinema => "cinema",
        }
    }

    fn salon(self) -> &'static str {
        match self {
            Genre::Watch | Genre::Music => "voice",
            Genre::Cinema => "cinema",
        }
    }

    fn evenement(self) -> &'static str {
        match self {
            Genre::Watch => "watch.state",
            Genre::Music => "music.state",
            Genre::Cinema => "cinema.state",
        }
    }
}

struct Contexte {
    watch: Sessions,
    tunes: Sessions,
    cinema: Sessions,
    musique: Arc<Musique>,
    accroche: Once,
}

impl Contexte {
    fn sessions(&self, genre: Genre) -> &Sessions {
        match genre {
            Genre::Watch => &self.watch,
            Genre::Music => &self.tunes,
            Genre::Cinema => &self.cinema,
        }
    }

    fn accrocher(self: &Arc<Self>, state: &AppState) {
        self.accroche.call_once(|| {
            let ctx = self.clone();
            state.sur_sortie_salon_vocal(move |etat, serveur, salon, membre| {
                let ctx = ctx.clone();
                async move { ctx.depart(&etat, &serveur, &salon, &membre).await }
            });
        });
    }

    async fn depart(&self, state: &AppState, serveur: &str, salon: &str, membre: &str) {
        for genre in [Genre::Watch, Genre::Music, Genre::Cinema] {
            let sessions = self.sessions(genre);
            if !sessions.existe(salon) {
                continue;
            }
            let Some(Ok(presents)) = state.participants_vocal(salon).await else {
                continue;
            };
            let restants: Vec<String> = presents
                .iter()
                .filter_map(|p| p["user_id"].as_str())
                .filter(|id| *id != BOT && *id != membre)
                .map(String::from)
                .collect();
            let film = sessions
                .etat(salon)
                .is_some_and(|e| e.provider == session::PROVIDER_FILM);
            let restants = if film {
                spectateurs(state, restants).await
            } else {
                restants
            };
            if let Some(suivant) = sessions.quitter(salon, membre, &restants) {
                let cible = Salon {
                    serveur: serveur.to_string(),
                    id: salon.to_string(),
                };
                publier(state, genre, &cible, suivant.as_ref()).await;
            }
        }
    }
}

async fn spectateurs(state: &AppState, ids: Vec<String>) -> Vec<String> {
    let Ok(pool) = state.pool() else {
        return Vec::new();
    };
    let Ok(lignes) = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, keycloak_sub FROM users WHERE id = ANY($1::text[]::uuid[])",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    else {
        return Vec::new();
    };
    let mut sortie = Vec::new();
    for id in ids {
        let Some((_, sub)) = lignes.iter().find(|(i, _)| *i == id) else {
            continue;
        };
        if matches!(
            state.droit(sub, estompe_core::catalogue::FILMS).await,
            Ok(true)
        ) {
            sortie.push(id);
        }
    }
    sortie
}

struct Salon {
    serveur: String,
    id: String,
}

fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn introuvable(message: &str) -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", message)
}

fn injoignable() -> Response {
    write_err(
        StatusCode::BAD_GATEWAY,
        "voice_unavailable",
        "serveur vocal injoignable",
    )
}

fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

fn erreur(r: Refus) -> Response {
    let code = if r.statut == StatusCode::NOT_FOUND {
        "no_watch"
    } else {
        "watch_refused"
    };
    write_err(r.statut, code, &r.message)
}

fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("regarder-ensemble : base : {e}");
    internal()
}

async fn salon_vocal(
    state: &AppState,
    genre: Genre,
    cid: &str,
    user: &User,
) -> Result<Salon, Response> {
    if !state.vocal_branche() {
        return Err(write_err(
            StatusCode::NOT_IMPLEMENTED,
            "no_voice",
            "vocal indisponible",
        ));
    }
    if !uuid_valide(cid) {
        return Err(introuvable("introuvable"));
    }
    let pool = state.pool().map_err(IntoResponse::into_response)?;
    let (serveur, kind) = sqlx::query_as::<_, (String, String)>(
        "SELECT c.server_id::text, c.kind FROM channels c
         JOIN server_members m ON m.server_id = c.server_id WHERE c.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(cid)
    .bind(&user.id)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(|| introuvable("introuvable"))?;
    let acces = state
        .acces_serveur(&serveur, &user.id)
        .await
        .map_err(bd)?
        .ok_or_else(|| introuvable("introuvable"))?;
    if !roles::contient(acces.channel(cid), roles::VIEW_CHANNEL) {
        return Err(introuvable("introuvable"));
    }
    if kind != genre.salon() {
        return Err(if genre == Genre::Cinema {
            write_err(
                StatusCode::BAD_REQUEST,
                "not_cinema_channel",
                "ce salon n'est pas un salon cinéma",
            )
        } else {
            write_err(
                StatusCode::BAD_REQUEST,
                "not_voice_channel",
                "ce salon n'est pas un salon vocal",
            )
        });
    }
    Ok(Salon {
        serveur,
        id: cid.to_string(),
    })
}

async fn salon_ecriture(
    state: &AppState,
    genre: Genre,
    cid: &str,
    user: &User,
) -> Result<Salon, Response> {
    let salon = salon_vocal(state, genre, cid, user).await?;
    if !state
        .limiteurs()
        .regarder_ensemble
        .allow(&user.id, ACTIONS_MAX, FENETRE)
    {
        let mut r = write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "trop d'actions, patientez un instant",
        );
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("10"));
        return Err(r);
    }
    let presents = match state.participants_vocal(&salon.id).await {
        Some(Ok(p)) => p,
        Some(Err(_)) => return Err(injoignable()),
        None => Vec::new(),
    };
    if presents
        .iter()
        .any(|p| p["user_id"].as_str() == Some(user.id.as_str()))
    {
        Ok(salon)
    } else {
        Err(write_err(
            StatusCode::FORBIDDEN,
            "not_in_voice",
            "rejoignez le salon vocal pour regarder ensemble",
        ))
    }
}

async fn publier(state: &AppState, genre: Genre, salon: &Salon, etat: Option<&Etat>) {
    let Ok(pool) = state.pool() else { return };
    let conv = Conv {
        channel_id: Some(salon.id.clone()),
        server_id: Some(salon.serveur.clone()),
        dm_id: None,
        kind: genre.salon().into(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    };
    let mut v = vue(etat, SystemTime::now());
    v["server_id"] = salon.serveur.clone().into();
    v["channel_id"] = salon.id.clone().into();
    messages::publier(state, pool, &conv, genre.evenement(), &v).await;
}

async fn appliquer(
    state: &AppState,
    ctx: &Contexte,
    genre: Genre,
    salon: &Salon,
    besoin: bool,
    f: impl FnOnce(Option<&Etat>, SystemTime) -> Resultat,
) -> Response {
    match ctx.sessions(genre).appliquer(&salon.id, besoin, f) {
        Ok(suivant) => {
            publier(state, genre, salon, suivant.as_ref()).await;
            sans_contenu()
        }
        Err(r) => erreur(r),
    }
}

fn meta(m: &estompe_block_musique::MusicMeta) -> Meta {
    Meta {
        title: m.title.clone(),
        artist: m.artist.clone(),
        album: m.album.clone(),
        duration: m.duration,
        cover: m.cover.clone(),
    }
}

fn item_film(f: FilmResolu, user: &Utilisateur<'_>) -> Item {
    Item {
        id: session::nouvel_id(),
        provider: session::PROVIDER_FILM.into(),
        video: f.id,
        by: user.pseudo.to_string(),
        by_id: user.id.to_string(),
        meta: Some(Meta {
            title: f.titre,
            artist: String::new(),
            album: String::new(),
            duration: f.duree,
            cover: String::new(),
        }),
    }
}

async fn films(
    state: &AppState,
    id: &str,
    sub: &str,
    avec_suite: bool,
    user: &Utilisateur<'_>,
) -> Result<Vec<Item>, String> {
    let films = state.films().ok_or_else(|| VIDEO_INVALIDE.to_string())?;
    let (premier, suite) = films.resoudre(sub, id).await.map_err(|r| match r {
        RefusFilm::Permission => "accès aux films et séries non accordé".to_string(),
        RefusFilm::Introuvable => "titre introuvable".to_string(),
        RefusFilm::Indisponible => "la bibliothèque est injoignable".to_string(),
    })?;
    let mut items = vec![item_film(premier, user)];
    if avec_suite {
        items.extend(suite.into_iter().map(|f| item_film(f, user)));
    }
    Ok(items)
}

async fn elements(
    state: &AppState,
    ctx: &Contexte,
    genre: Genre,
    liens: &[String],
    user: &Utilisateur<'_>,
    film: Option<(&str, bool)>,
) -> Result<Vec<Item>, String> {
    let mut sortie = Vec::with_capacity(liens.len());
    for l in liens {
        if genre == Genre::Music {
            let lien = analyser_youtube_music(l).ok_or_else(|| PISTE_INVALIDE.to_string())?;
            let (_, pistes) = ctx
                .musique
                .resoudre(&state.config().youtube_key, &lien)
                .await
                .map_err(|e| e.to_string())?;
            for p in pistes {
                sortie.push(Item {
                    id: session::nouvel_id(),
                    provider: "youtube".into(),
                    video: p.id,
                    by: user.pseudo.to_string(),
                    by_id: user.id.to_string(),
                    meta: Some(meta(&p.meta)),
                });
            }
            continue;
        }
        let film_demande = l.strip_prefix("film:");
        if genre == Genre::Cinema && film_demande.is_none() {
            return Err(FILM_INVALIDE.to_string());
        }
        if let Some(id) = film_demande {
            if genre != Genre::Cinema {
                return Err(FILM_HORS_CINEMA.to_string());
            }
            let (sub, avec_suite) = film.ok_or_else(|| VIDEO_INVALIDE.to_string())?;
            sortie.extend(films(state, id, sub, avec_suite, user).await?);
            continue;
        }
        let (provider, reference) =
            lien::lien_video(l).ok_or_else(|| VIDEO_INVALIDE.to_string())?;
        sortie.push(Item {
            id: session::nouvel_id(),
            provider: provider.into(),
            video: reference,
            by: user.pseudo.to_string(),
            by_id: user.id.to_string(),
            meta: None,
        });
    }
    Ok(sortie)
}

fn invalide(message: &str) -> Response {
    write_err(StatusCode::BAD_REQUEST, "invalid_video", message)
}

fn type_de<T>(
    corps: &Map<String, Value>,
    nom: &str,
    lire: impl Fn(&Value) -> Option<T>,
) -> Result<Option<T>, Response> {
    match champ(corps, nom) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => lire(v).map(Some).ok_or_else(mauvais),
    }
}

fn liens(corps: &Map<String, Value>, nom: &str) -> Result<Option<Vec<String>>, Response> {
    type_de(corps, nom, |v| {
        v.as_array()?
            .iter()
            .map(|e| match e {
                Value::String(s) => Some(s.clone()),
                Value::Null => Some(String::new()),
                _ => None,
            })
            .collect()
    })
}

fn utilisateur(u: &User) -> Utilisateur<'_> {
    Utilisateur {
        id: &u.id,
        pseudo: &u.pseudo,
    }
}

async fn serveur(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(id): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if !uuid_valide(&id) {
        return introuvable("serveur introuvable");
    }
    let acces = match state.acces_serveur(&id, &user.id).await {
        Ok(Some(a)) => a,
        Ok(None) => return introuvable("serveur introuvable"),
        Err(e) => return bd(e),
    };
    let salons = match sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND kind=$2",
    )
    .bind(&id)
    .bind(genre.salon())
    .fetch_all(pool)
    .await
    {
        Ok(s) => s,
        Err(e) => return bd(e),
    };
    let now = SystemTime::now();
    let mut sortie = Map::new();
    for (salon, etat) in ctx.sessions(genre).parmi(&salons) {
        if roles::contient(acces.channel(&salon), roles::VIEW_CHANNEL) {
            sortie.insert(salon, vue(Some(&etat), now));
        }
    }
    write_json(StatusCode::OK, &Value::Object(sortie))
}

async fn lire(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    match salon_vocal(&state, genre, &cid, &user).await {
        Ok(salon) => write_json(
            StatusCode::OK,
            &vue(
                ctx.sessions(genre).etat(&salon.id).as_ref(),
                SystemTime::now(),
            ),
        ),
        Err(r) => r,
    }
}

async fn demarrer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let (video, file) = match (champ_texte(&corps, "video"), liens(&corps, "queue")) {
        (Some(v), Ok(f)) => (v, f),
        (_, Err(r)) => return r,
        (None, _) => return mauvais(),
    };
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let film = Some((user.sub.as_str(), file.as_ref().is_none_or(Vec::is_empty)));
    let items = match elements(&state, &ctx, genre, &[video], &utilisateur(&user), film).await {
        Ok(i) => i,
        Err(m) => return invalide(&m),
    };
    let film = Some((user.sub.as_str(), false));
    let file = match file {
        None => None,
        Some(f) if f.len() > session::FILE_MAX => return invalide(FILE_INVALIDE),
        Some(f) => match elements(&state, &ctx, genre, &f, &utilisateur(&user), film).await {
            Ok(i) => Some(i),
            Err(m) => return invalide(&m),
        },
    };
    appliquer(&state, &ctx, genre, &salon, false, |s, now| {
        session::demarrer(s, &utilisateur(&user), items, file, now)
    })
    .await
}

async fn arreter(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::arreter(s.expect("session requise"), &user.id)
    })
    .await
}

async fn action(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let a = (|| {
        Ok::<_, Response>(Action {
            action: champ_texte(&corps, "action").ok_or_else(mauvais)?,
            position: type_de(&corps, "position", Value::as_f64)?.unwrap_or_default(),
            rate: type_de(&corps, "rate", Value::as_f64)?.unwrap_or_default(),
            rev: type_de(&corps, "rev", Value::as_i64)?.unwrap_or_default(),
            video: champ_texte(&corps, "video").ok_or_else(mauvais)?,
        })
    })();
    let a = match a {
        Ok(a) => a,
        Err(r) => return r,
    };
    if a.position < 0.0 || a.position > session::POSITION_MAX || a.position.is_nan() {
        return write_err(
            StatusCode::BAD_REQUEST,
            "invalid_position",
            "position invalide",
        );
    }
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, now| {
        session::agir(s.expect("session requise"), &utilisateur(&user), &a, now)
    })
    .await
}

async fn pret(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let pret = match type_de(&corps, "ready", Value::as_bool) {
        Ok(p) => p.unwrap_or_default(),
        Err(r) => return r,
    };
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::pret(s.expect("session requise"), &user.id, pret)
    })
    .await
}

async fn options(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let lues = (|| {
        Ok::<_, Response>((
            type_de(&corps, "host_only", Value::as_bool)?,
            type_de(&corps, "repeat", |v| v.as_str().map(String::from))?,
        ))
    })();
    let (host_only, repeat) = match lues {
        Ok(l) => l,
        Err(r) => return r,
    };
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::options(
            s.expect("session requise"),
            &user.id,
            host_only,
            repeat.as_deref(),
        )
    })
    .await
}

async fn ajouter(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(video) = champ_texte(&corps, "video") else {
        return mauvais();
    };
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let film = Some((user.sub.as_str(), false));
    let items = match elements(&state, &ctx, genre, &[video], &utilisateur(&user), film).await {
        Ok(i) => i,
        Err(m) => return invalide(&m),
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::ajouter(s.expect("session requise"), items)
    })
    .await
}

async fn reordonner(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let ids = match liens(&corps, "ids") {
        Ok(i) => i.unwrap_or_default(),
        Err(r) => return r,
    };
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::reordonner(s.expect("session requise"), &user.id, &ids)
    })
    .await
}

async fn retirer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Extension(genre): Extension<Genre>,
    ActiveUser(user): ActiveUser,
    Path((cid, id)): Path<(String, String)>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_ecriture(&state, genre, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    appliquer(&state, &ctx, genre, &salon, true, |s, _| {
        session::retirer(s.expect("session requise"), &user.id, &id)
    })
    .await
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOCS: [Doc; 10] = [
    (
        HttpMethod::Get,
        "/voice/{cid}/{k}",
        "Session du salon",
        &[("200", "ok"), ("404", "salon introuvable")],
    ),
    (
        HttpMethod::Put,
        "/voice/{cid}/{k}",
        "Lancer la session ou changer de vidéo",
        &[
            ("204", "ok"),
            ("400", "invalid_video"),
            ("403", "`not_in_voice` ou hôte seul"),
            ("429", "rate_limited"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/voice/{cid}/{k}",
        "Arrêter la session",
        &[
            ("204", "ok"),
            ("403", "`not_in_voice` ou hôte seul"),
            ("404", "no_watch"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/{k}/action",
        "Piloter la lecture",
        &[
            ("204", "ok"),
            ("400", "position, vitesse ou action invalide"),
            ("403", "`not_in_voice` ou hôte seul"),
            ("404", "no_watch"),
            ("409", "la vidéo a changé"),
            ("429", "rate_limited"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/{k}/ready",
        "Indiquer que son lecteur est prêt ou en chargement",
        &[("204", "ok"), ("403", "not_in_voice"), ("404", "no_watch")],
    ),
    (
        HttpMethod::Put,
        "/voice/{cid}/{k}/options",
        "Options de la session",
        &[
            ("204", "ok"),
            ("400", "mode de répétition inconnu"),
            ("403", "forbidden"),
            ("404", "no_watch"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/{k}/queue",
        "Ajouter à la file",
        &[
            ("204", "ok"),
            ("400", "invalid_video"),
            ("404", "no_watch"),
            ("409", "file pleine"),
        ],
    ),
    (
        HttpMethod::Put,
        "/voice/{cid}/{k}/queue",
        "Réordonner la file",
        &[
            ("204", "ok"),
            ("403", "forbidden"),
            ("409", "la file a changé"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/voice/{cid}/{k}/queue/{id}",
        "Retirer de la file",
        &[("204", "ok"), ("403", "forbidden"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/{k}",
        "Sessions en cours dans les salons vocaux visibles",
        &[("200", "ok"), ("404", "serveur introuvable")],
    ),
];

fn routes_du_genre(genre: Genre) -> OpenApiRouter<AppState> {
    let k = genre.chemin();
    let mut r = OpenApiRouter::new()
        .route(&format!("/servers/{{id}}/{k}"), get(serveur))
        .route(
            &format!("/voice/{{cid}}/{k}"),
            get(lire).put(demarrer).delete(arreter),
        )
        .route(
            &format!("/voice/{{cid}}/{k}/action"),
            axum::routing::post(action),
        )
        .route(
            &format!("/voice/{{cid}}/{k}/ready"),
            axum::routing::post(pret),
        )
        .route(
            &format!("/voice/{{cid}}/{k}/options"),
            axum::routing::put(options),
        )
        .route(
            &format!("/voice/{{cid}}/{k}/queue"),
            axum::routing::post(ajouter).put(reordonner),
        )
        .route(
            &format!("/voice/{{cid}}/{k}/queue/{{id}}"),
            axum::routing::delete(retirer),
        );
    for (methode, modele, resume, reponses) in DOCS.iter() {
        let chemin = modele.replace("{k}", k);
        r = openapi::documenter(
            r,
            &chemin,
            methode.clone(),
            openapi::operation("vocal", resume, reponses, &chemin),
        );
    }
    r.layer(Extension(genre))
}

#[derive(Clone)]
pub struct Lecteurs(Arc<Contexte>);

impl Lecteurs {
    pub fn nouveau(musique: Arc<Musique>) -> Self {
        Lecteurs(Arc::new(Contexte {
            watch: Sessions::default(),
            tunes: Sessions::default(),
            cinema: Sessions::default(),
            musique,
            accroche: Once::new(),
        }))
    }

    pub fn partage() -> Self {
        static PARTAGE: OnceLock<Lecteurs> = OnceLock::new();
        PARTAGE
            .get_or_init(|| Lecteurs::nouveau(Arc::new(Musique::new())))
            .clone()
    }

    pub fn routes(&self) -> OpenApiRouter<AppState> {
        routes_du_genre(Genre::Watch)
            .merge(routes_du_genre(Genre::Music))
            .layer(Extension(self.0.clone()))
    }

    pub fn routes_cinema(&self) -> OpenApiRouter<AppState> {
        routes_du_genre(Genre::Cinema).layer(Extension(self.0.clone()))
    }

    pub async fn lancer(
        &self,
        state: &AppState,
        salon: &SalonLance<'_>,
        par: &Utilisateur<'_>,
        videos: Vec<(String, String)>,
        liens_musique: Vec<String>,
    ) {
        let cible = Salon {
            serveur: salon.serveur.to_string(),
            id: salon.id.to_string(),
        };
        self.0.accrocher(state);
        if !liens_musique.is_empty() {
            match elements(state, &self.0, Genre::Music, &liens_musique, par, None).await {
                Ok(pistes) if !pistes.is_empty() => {
                    self.demarrer_file(state, Genre::Music, &cible, par, pistes)
                        .await;
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("séances : musique : {e}"),
            }
        }
        if !videos.is_empty() {
            let items = videos
                .into_iter()
                .map(|(provider, video)| Item {
                    id: session::nouvel_id(),
                    provider,
                    video,
                    by: par.pseudo.to_string(),
                    by_id: par.id.to_string(),
                    meta: None,
                })
                .collect();
            self.demarrer_file(state, Genre::Watch, &cible, par, items)
                .await;
        }
    }

    async fn demarrer_file(
        &self,
        state: &AppState,
        genre: Genre,
        salon: &Salon,
        par: &Utilisateur<'_>,
        items: Vec<Item>,
    ) {
        let r = self
            .0
            .sessions(genre)
            .appliquer(&salon.id, false, |s, now| {
                session::lancer_file(s, par, items, now)
            });
        if let Ok(suivant) = r {
            publier(state, genre, salon, suivant.as_ref()).await;
        }
    }
}

pub struct SalonLance<'a> {
    pub serveur: &'a str,
    pub id: &'a str,
}

pub fn routes(musique: Arc<Musique>) -> OpenApiRouter<AppState> {
    Lecteurs::nouveau(musique).routes()
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let lecteurs = Lecteurs::partage();
    router
        .merge(lecteurs.routes())
        .merge(lecteurs.routes_cinema())
}

pub fn brancher(state: &AppState) {
    entracte::brancher(state);
}

#[cfg(test)]
mod tests {
    use super::Lecteurs;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
    };
    use estompe_block_musique::Musique;
    use estompe_core::{
        db::migrate_embarquees,
        identity::Cred,
        state::{
            AppState, FilmEtSuite, FilmResolu, Films, ParticipantsVocal, RefusFilm, Vocal,
            VocalInjoignable,
        },
        testutil::{self, BasePostgres, Faux, faux, faux_permission},
    };
    use serde_json::{Value, json};
    use std::{
        collections::HashMap,
        future::Future,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tower::ServiceExt;

    const V1: &str = "dQw4w9WgXcQ";
    const V2: &str = "aaaaaaaaaaa";
    const V3: &str = "bbbbbbbbbbb";

    #[derive(Default)]
    struct FauxVocal(Mutex<HashMap<String, Vec<Value>>>, AtomicBool);

    impl FauxVocal {
        fn presents<S: AsRef<str>>(&self, salon: &str, ids: &[S]) {
            self.0.lock().unwrap().insert(
                salon.to_string(),
                ids.iter()
                    .map(|id| json!({ "user_id": id.as_ref(), "pseudo": "x" }))
                    .collect(),
            );
        }
    }

    impl Vocal for FauxVocal {
        fn sortir_du_serveur<'a>(
            &'a self,
            _serveur: &'a str,
            _membre: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {})
        }

        fn participants<'a>(
            &'a self,
            salon: &'a str,
        ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>> {
            Box::pin(async move {
                if self.1.load(Ordering::SeqCst) {
                    return Err(VocalInjoignable);
                }
                Ok(self
                    .0
                    .lock()
                    .unwrap()
                    .get(salon)
                    .cloned()
                    .unwrap_or_default())
            })
        }
    }

    struct Env {
        app: Router,
        state: AppState,
        vocal: Arc<FauxVocal>,
        alice: String,
        bob: String,
        serveur: String,
        salon: String,
        texte: String,
        cinema: String,
        _faux: (Faux, Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env(avec_vocal: bool) -> Option<Env> {
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
        let oembed = faux(|_| {
            (
                200,
                json!({ "title": "Titre oEmbed", "author_name": "Chaîne - Topic" })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        let mut ids = Vec::new();
        for nom in ["alice", "bob"] {
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
        let serveur: String = sqlx::query_scalar(
            "INSERT INTO servers (name, owner_id) VALUES ('s', $1::uuid) RETURNING id::text",
        )
        .bind(&ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
        for id in &ids {
            sqlx::query(
                "INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid,$2::uuid)",
            )
            .bind(&serveur)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO roles (server_id, name, position, permissions, is_default) VALUES ($1::uuid,'@tout le monde',0,$2,true)",
        )
        .bind(&serveur)
        .bind(estompe_core::roles::EVERYONE as i64)
        .execute(&pool)
        .await
        .unwrap();
        let mut salons = Vec::new();
        for (nom, kind) in [("vocal", "voice"), ("texte", "text"), ("ciné", "cinema")] {
            salons.push(
                sqlx::query_scalar::<_, String>(
                    "INSERT INTO channels (server_id, name, kind) VALUES ($1::uuid,$2,$3) RETURNING id::text",
                )
                .bind(&serveur)
                .bind(nom)
                .bind(kind)
                .fetch_one(&pool)
                .await
                .unwrap(),
            );
        }
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("PERMISSION_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let vocal = Arc::new(FauxVocal::default());
        if avec_vocal {
            state.definir_vocal(vocal.clone());
        }
        let musique = Musique::avec_bases(&oembed.url, &oembed.url, &oembed.url);
        let lecteurs = Lecteurs::nouveau(Arc::new(musique));
        let app = Router::from(lecteurs.routes().merge(lecteurs.routes_cinema()))
            .with_state(state.clone());
        Some(Env {
            app,
            state,
            vocal,
            alice: ids[0].clone(),
            bob: ids[1].clone(),
            serveur,
            salon: salons[0].clone(),
            texte: salons[1].clone(),
            cinema: salons[2].clone(),
            _faux: (gateway, permission, oembed),
            _pg: pg,
        })
    }

    impl Env {
        async fn appel(
            &self,
            qui: &str,
            methode: &str,
            chemin: &str,
            corps: &str,
        ) -> (StatusCode, HeaderMap, Value) {
            let rep = self
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(methode)
                        .uri(chemin)
                        .header(header::AUTHORIZATION, format!("Bearer {qui}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(corps.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let (statut, entetes) = (rep.status(), rep.headers().clone());
            let octets = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
            (
                statut,
                entetes,
                serde_json::from_slice(&octets).unwrap_or(Value::Null),
            )
        }

        async fn code(&self, qui: &str, methode: &str, chemin: &str, corps: &str) -> u16 {
            self.appel(qui, methode, chemin, corps).await.0.as_u16()
        }

        fn chemin(&self, genre: &str) -> String {
            let salon = if genre == "cinema" {
                &self.cinema
            } else {
                &self.salon
            };
            format!("/voice/{salon}/{genre}")
        }

        async fn etat(&self, genre: &str) -> Value {
            let (s, _, v) = self.appel("alice", "GET", &self.chemin(genre), "").await;
            assert_eq!(s, StatusCode::OK);
            v["state"].clone()
        }

        async fn listees(&self, genre: &str) -> usize {
            let chemin = format!("/servers/{}/{genre}", self.serveur);
            let (s, _, v) = self.appel("bob", "GET", &chemin, "").await;
            assert_eq!(s, StatusCode::OK);
            v.as_object().unwrap().len()
        }
    }

    struct FauxFilms;

    impl Films for FauxFilms {
        fn resoudre<'a>(
            &'a self,
            sub: &'a str,
            id: &'a str,
        ) -> Pin<Box<dyn Future<Output = FilmEtSuite> + Send + 'a>> {
            Box::pin(async move {
                if sub == "sub-bob" {
                    return Err(RefusFilm::Permission);
                }
                if id == "inconnu" {
                    return Err(RefusFilm::Introuvable);
                }
                let f = |id: &str, titre: &str| FilmResolu {
                    id: id.into(),
                    titre: titre.into(),
                    duree: 3000,
                };
                Ok((f(id, "S1E1"), vec![f("e2", "S1E2"), f("e3", "S1E3")]))
            })
        }
    }

    #[tokio::test]
    async fn session_de_film_pilotee_par_l_hote_avec_la_file_d_episodes() {
        let Some(e) = env(true).await else { return };
        let chemin = e.chemin("cinema");
        e.vocal.presents(&e.cinema, &[&e.alice, &e.bob]);
        let lien = r#"{"video":"film:e1","queue":[]}"#;
        assert_eq!(
            e.code("alice", "PUT", &chemin, lien).await,
            400,
            "lien film refusé sans le bloc"
        );
        e.state.definir_films(Arc::new(FauxFilms));
        let (s, _, v) = e.appel("bob", "PUT", &chemin, lien).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "rejet sans la permission");
        assert_eq!(v["message"], "accès aux films et séries non accordé");
        assert!(e.etat("cinema").await.is_null());
        let inconnu = r#"{"video":"film:inconnu"}"#;
        assert_eq!(e.code("alice", "PUT", &chemin, inconnu).await, 400);

        assert_eq!(e.code("alice", "PUT", &chemin, lien).await, 204);
        let s = e.etat("cinema").await;
        assert_eq!(s["provider"], "film");
        assert_eq!(s["video"], "e1");
        assert_eq!(s["host"], e.alice);
        assert_eq!(s["host_only"], true);
        assert_eq!(s["meta"]["title"], "S1E1");
        assert_eq!(s["meta"]["duration"], 3000);
        let file: Vec<&str> = s["queue"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["video"].as_str().unwrap())
            .collect();
        assert_eq!(file, ["e2", "e3"]);

        let action = format!("{chemin}/action");
        let pause = r#"{"action":"pause","position":5,"rev":1,"video":""}"#;
        assert_eq!(e.code("bob", "POST", &action, pause).await, 403);
        assert_eq!(e.code("alice", "POST", &action, pause).await, 204);
        let options = format!("{chemin}/options");
        assert_eq!(
            e.code("alice", "PUT", &options, r#"{"host_only":false}"#)
                .await,
            403
        );
        let (s, _, v) = e.appel("bob", "GET", &chemin, "").await;
        assert_eq!(
            s,
            StatusCode::OK,
            "un participant sans permission voit la session"
        );
        assert_eq!(v["state"]["meta"]["title"], "S1E1");
    }

    #[tokio::test]
    async fn salon_cinema_reserve_aux_films_et_regarder_ensemble_sans_films() {
        let Some(e) = env(true).await else { return };
        e.state.definir_films(Arc::new(FauxFilms));
        e.vocal.presents(&e.salon, &[&e.alice, &e.bob]);
        e.vocal.presents(&e.cinema, &[&e.alice, &e.bob]);
        let film = r#"{"video":"film:e1"}"#;
        let video = format!(r#"{{"video":"https://youtu.be/{V1}"}}"#);

        let (s, _, v) = e.appel("alice", "PUT", &e.chemin("watch"), film).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(
            v["message"],
            "les films et séries se regardent dans un salon cinéma"
        );
        assert_eq!(
            e.code("alice", "PUT", &e.chemin("watch"), &video).await,
            204
        );
        let (s, _, v) = e
            .appel(
                "alice",
                "POST",
                &format!("{}/queue", e.chemin("watch")),
                film,
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
        let file = r#"{"video":"https://youtu.be/dQw4w9WgXcQ","queue":["film:e2"]}"#;
        assert_eq!(e.code("alice", "PUT", &e.chemin("watch"), file).await, 400);

        let (s, _, v) = e.appel("alice", "PUT", &e.chemin("cinema"), &video).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(
            v["message"],
            "seuls les films et séries de la bibliothèque se regardent dans un salon cinéma"
        );
        let (s, _, v) = e
            .appel("alice", "PUT", &format!("/voice/{}/cinema", e.salon), film)
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("not_cinema_channel"))
        );
        let (s, _, v) = e
            .appel(
                "alice",
                "PUT",
                &format!("/voice/{}/watch", e.cinema),
                &video,
            )
            .await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("not_voice_channel"))
        );

        let (_c, mut rx, _) = e.state.hub().register(
            &e.bob,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        assert_eq!(e.code("alice", "PUT", &e.chemin("cinema"), film).await, 204);
        let evenement: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(evenement["type"], "cinema.state");
        assert_eq!(evenement["data"]["channel_id"], e.cinema);
        assert_eq!(evenement["data"]["state"]["provider"], "film");
        assert_eq!(e.listees("cinema").await, 1);
        assert_eq!(e.listees("watch").await, 1);
        assert_eq!(e.etat("watch").await["provider"], "youtube");

        let action = format!("{}/action", e.chemin("cinema"));
        let seek = r#"{"action":"seek","position":120,"rev":1,"video":"e1"}"#;
        assert_eq!(e.code("bob", "POST", &action, seek).await, 403);
        assert_eq!(e.code("alice", "POST", &action, seek).await, 204);
        let evenement: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(evenement["type"], "cinema.state");
        assert_eq!(evenement["data"]["state"]["pos"], 120.0);
        assert_eq!(evenement["data"]["state"]["act"], "seek");

        e.vocal.presents(&e.cinema, &[&e.bob]);
        e.state
            .sortie_salon_vocal(&e.serveur, &e.cinema, &e.alice)
            .await;
        let s = e.etat("cinema").await;
        assert_eq!(
            (s["host"].as_str(), s["host_only"].as_bool()),
            (Some(e.bob.as_str()), Some(true))
        );
        assert_eq!(e.etat("watch").await["host"], e.alice);
    }

    #[tokio::test]
    async fn vocal_injoignable_donne_502() {
        let Some(e) = env(true).await else { return };
        let chemin = e.chemin("watch");
        let lien = format!(r#"{{"video":"https://youtu.be/{V1}"}}"#);
        e.vocal.1.store(true, Ordering::SeqCst);
        assert_eq!(e.code("alice", "PUT", &chemin, &lien).await, 502);
        e.vocal.1.store(false, Ordering::SeqCst);
        assert_eq!(e.code("alice", "PUT", &chemin, &lien).await, 403);
    }

    #[tokio::test]
    async fn session_complete_comme_le_go() {
        let Some(e) = env(true).await else { return };
        let (_c, mut rx, _) = e.state.hub().register(
            &e.bob,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        let chemin = e.chemin("watch");
        assert_eq!(
            e.code("alice", "PUT", &chemin, &format!(r#"{{"video":"{V1}"}}"#))
                .await,
            403
        );
        assert!(e.etat("watch").await.is_null());
        e.vocal.presents(&e.salon, &[&e.alice, &e.bob]);
        assert_eq!(
            e.code("alice", "PUT", &chemin, r#"{"video":"pas une vidéo"}"#)
                .await,
            400
        );
        assert_eq!(
            e.code("alice", "PUT", &chemin, r#"{"video":"nul"#).await,
            400
        );
        let lien = format!(r#"{{"video":"https://youtu.be/{V1}"}}"#);
        assert_eq!(e.code("alice", "PUT", &chemin, &lien).await, 204);
        assert_eq!(e.listees("watch").await, 1);
        assert_eq!(e.listees("music").await, 0);
        let s = e.etat("watch").await;
        assert_eq!(s["video"], V1);
        assert_eq!(s["provider"], "youtube");
        assert_eq!(s["playing"], true);
        assert_eq!(s["host"], e.alice);
        assert_eq!(s["rate"], 1.0);
        let evenement: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(evenement["type"], "watch.state");
        assert_eq!(evenement["data"]["channel_id"], e.salon);
        assert_eq!(evenement["data"]["server_id"], e.serveur);
        assert_eq!(evenement["data"]["state"]["video"], V1);

        let action = format!("{chemin}/action");
        assert_eq!(
            e.code(
                "bob",
                "POST",
                &action,
                r#"{"action":"pause","position":42}"#
            )
            .await,
            204
        );
        let s = e.etat("watch").await;
        assert_eq!(
            (s["playing"].as_bool(), s["pos"].as_f64()),
            (Some(false), Some(42.0))
        );
        assert_eq!(s["by"], "bob");
        assert_eq!(s["act"], "pause");
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"play","position":42}"#)
                .await,
            204
        );
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"rate","rate":1.5}"#)
                .await,
            204
        );
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"rate","rate":9}"#)
                .await,
            400
        );
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"seek","position":-1}"#)
                .await,
            400
        );
        assert_eq!(
            e.code(
                "bob",
                "POST",
                &action,
                r#"{"action":"seek","position":"x"}"#
            )
            .await,
            400
        );
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"danse"}"#)
                .await,
            400
        );

        let options = format!("{chemin}/options");
        assert_eq!(
            e.code("bob", "PUT", &options, r#"{"host_only":true}"#)
                .await,
            403
        );
        assert_eq!(
            e.code("alice", "PUT", &options, r#"{"host_only":true}"#)
                .await,
            204
        );
        assert_eq!(
            e.code("bob", "POST", &action, r#"{"action":"pause","position":1}"#)
                .await,
            403
        );
        assert_eq!(
            e.code(
                "alice",
                "POST",
                &action,
                r#"{"action":"seek","position":100}"#
            )
            .await,
            204
        );
        assert_eq!(
            e.code("bob", "PUT", &chemin, &format!(r#"{{"video":"{V2}"}}"#))
                .await,
            403
        );

        let file = format!("{chemin}/queue");
        assert_eq!(
            e.code("bob", "POST", &file, &format!(r#"{{"video":"{V2}"}}"#))
                .await,
            204
        );
        assert_eq!(
            e.code("alice", "POST", &file, &format!(r#"{{"video":"{V3}"}}"#))
                .await,
            204
        );
        let q = e.etat("watch").await["queue"].clone();
        let (q0, q1) = (q[0]["id"].as_str().unwrap(), q[1]["id"].as_str().unwrap());
        assert_eq!(q[0]["video"], V2);
        assert_eq!(q[1]["video"], V3);
        let inverse = format!(r#"{{"ids":["{q1}","{q0}"]}}"#);
        assert_eq!(e.code("bob", "PUT", &file, &inverse).await, 403);
        assert_eq!(
            e.code("alice", "PUT", &file, &format!(r#"{{"ids":["{q1}"]}}"#))
                .await,
            409
        );
        assert_eq!(e.code("alice", "PUT", &file, &inverse).await, 204);
        assert_eq!(
            e.code("bob", "DELETE", &format!("{file}/{q0}"), "").await,
            204
        );
        assert_eq!(
            e.code("bob", "DELETE", &format!("{file}/{q1}"), "").await,
            403
        );
        assert_eq!(
            e.code("alice", "DELETE", &format!("{file}/inconnu"), "")
                .await,
            404
        );

        let rev = e.etat("watch").await["rev"].as_i64().unwrap();
        let fin = format!(r#"{{"action":"ended","rev":{rev}}}"#);
        assert_eq!(e.code("bob", "POST", &action, &fin).await, 204);
        assert_eq!(e.code("alice", "POST", &action, &fin).await, 204);
        let s = e.etat("watch").await;
        assert_eq!(s["video"], V3);
        assert_eq!(s["playing"], true);
        assert_eq!(s["pos"].as_f64(), Some(0.0));
        assert!(s["queue"].as_array().unwrap().is_empty());
        let rev = s["rev"].as_i64().unwrap();
        let fin = format!(r#"{{"action":"ended","rev":{rev}}}"#);
        assert_eq!(e.code("alice", "POST", &action, &fin).await, 204);
        let s = e.etat("watch").await;
        assert_eq!(
            (s["playing"].as_bool(), s["video"].as_str()),
            (Some(false), Some(V3))
        );

        assert_eq!(
            e.code(
                "bob",
                "POST",
                &format!("{chemin}/ready"),
                r#"{"ready":true}"#
            )
            .await,
            204
        );
        assert_eq!(e.etat("watch").await["ready"][&e.bob], true);

        e.vocal.presents(&e.salon, &[&e.bob]);
        e.state
            .sortie_salon_vocal(&e.serveur, &e.salon, &e.alice)
            .await;
        let s = e.etat("watch").await;
        assert_eq!(s["host"], e.bob);
        assert!(s["ready"].as_object().unwrap().contains_key(&e.bob));
        e.vocal.presents(&e.salon, &[] as &[&str]);
        e.state
            .sortie_salon_vocal(&e.serveur, &e.salon, &e.bob)
            .await;
        assert!(e.etat("watch").await.is_null());
        assert_eq!(e.listees("watch").await, 0);
        assert_eq!(
            e.code("alice", "POST", &action, r#"{"action":"pause"}"#)
                .await,
            403
        );
    }

    #[tokio::test]
    async fn arret_par_l_hote_et_sans_session() {
        let Some(e) = env(true).await else { return };
        e.vocal.presents(&e.salon, &[&e.alice, &e.bob]);
        let chemin = e.chemin("watch");
        assert_eq!(e.code("alice", "DELETE", &chemin, "").await, 404);
        assert_eq!(
            e.code(
                "alice",
                "POST",
                &format!("{chemin}/action"),
                r#"{"action":"pause"}"#
            )
            .await,
            404
        );
        let lien = format!(r#"{{"video":"{V1}"}}"#);
        assert_eq!(e.code("alice", "PUT", &chemin, &lien).await, 204);
        assert_eq!(
            e.code(
                "alice",
                "PUT",
                &format!("{chemin}/options"),
                r#"{"host_only":true}"#
            )
            .await,
            204
        );
        assert_eq!(e.code("bob", "DELETE", &chemin, "").await, 403);
        assert_eq!(e.code("alice", "DELETE", &chemin, "").await, 204);
        assert!(e.etat("watch").await.is_null());
    }

    #[tokio::test]
    async fn limite_de_debit() {
        let Some(e) = env(true).await else { return };
        e.vocal.presents(&e.salon, &[&e.alice]);
        let chemin = e.chemin("watch");
        assert_eq!(
            e.code("alice", "PUT", &chemin, &format!(r#"{{"video":"{V1}"}}"#))
                .await,
            204
        );
        let action = format!("{chemin}/action");
        for _ in 0..29 {
            e.code("alice", "POST", &action, r#"{"action":"pause"}"#)
                .await;
        }
        let (s, h, _) = e
            .appel("alice", "POST", &action, r#"{"action":"pause"}"#)
            .await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(h["retry-after"], "10");
    }

    #[tokio::test]
    async fn autres_sources() {
        let Some(e) = env(true).await else { return };
        e.vocal.presents(&e.salon, &[&e.alice]);
        let chemin = e.chemin("watch");
        let netflix = r#"{"video":"https://www.netflix.com/watch/81234567"}"#;
        assert_eq!(e.code("alice", "PUT", &chemin, netflix).await, 400);
        let corps = r#"{"video":"https://vimeo.com/76979871","queue":["https://exemple.fr/a.mp4","https://dai.ly/x8abc12"]}"#;
        assert_eq!(e.code("alice", "PUT", &chemin, corps).await, 204);
        let mauvais =
            r#"{"video":"https://vimeo.com/76979871","queue":["https://exemple.fr/a.txt"]}"#;
        assert_eq!(e.code("alice", "PUT", &chemin, mauvais).await, 400);
        let s = e.etat("watch").await;
        assert_eq!(
            (s["provider"].as_str(), s["video"].as_str()),
            (Some("vimeo"), Some("76979871"))
        );
        assert_eq!(s["queue"][0]["provider"], "file");
        assert_eq!(s["queue"][1]["provider"], "dailymotion");
        let trop: Vec<String> = (0..51).map(|_| V2.to_string()).collect();
        let corps = json!({ "video": V1, "queue": trop }).to_string();
        assert_eq!(e.code("alice", "PUT", &chemin, &corps).await, 400);
        let file = format!("{chemin}/queue");
        assert_eq!(
            e.code(
                "alice",
                "POST",
                &file,
                r#"{"video":"https://www.twitch.tv/videos/12"}"#
            )
            .await,
            204
        );
        let rev = e.etat("watch").await["rev"].as_i64().unwrap();
        let fin = format!(r#"{{"action":"ended","rev":{rev}}}"#);
        assert_eq!(
            e.code("alice", "POST", &format!("{chemin}/action"), &fin)
                .await,
            204
        );
        let s = e.etat("watch").await;
        assert_eq!(s["provider"], "file");
        assert_eq!(s["video"], "https://exemple.fr/a.mp4");
        assert_eq!(s["queue"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn commande_sur_une_autre_video() {
        let Some(e) = env(true).await else { return };
        e.vocal.presents(&e.salon, &[&e.alice]);
        let chemin = e.chemin("watch");
        assert_eq!(
            e.code("alice", "PUT", &chemin, &format!(r#"{{"video":"{V1}"}}"#))
                .await,
            204
        );
        let action = format!("{chemin}/action");
        assert_eq!(
            e.code(
                "alice",
                "POST",
                &action,
                r#"{"action":"seek","position":30,"video":"autre-video"}"#
            )
            .await,
            409
        );
        let ok = format!(r#"{{"action":"seek","position":30,"video":"{V1}"}}"#);
        assert_eq!(e.code("alice", "POST", &action, &ok).await, 204);
        assert_eq!(e.etat("watch").await["pos"].as_f64(), Some(30.0));
        assert_eq!(
            e.code(
                "alice",
                "POST",
                &action,
                r#"{"action":"pause","position":31}"#
            )
            .await,
            204
        );
    }

    #[tokio::test]
    async fn music2gether_separee_de_regarder_ensemble() {
        let Some(e) = env(true).await else { return };
        let (_c, mut rx, _) = e.state.hub().register(
            &e.bob,
            Cred {
                value: "x".into(),
                cookie: false,
            },
        );
        e.vocal.presents(&e.salon, &[&e.alice]);
        let musique = e.chemin("music");
        let piste = |id: &str| format!(r#"{{"video":"https://music.youtube.com/watch?v={id}"}}"#);
        assert_eq!(
            e.code("alice", "PUT", &musique, &piste("eeeeeeeeeee"))
                .await,
            204
        );
        let evenement: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(evenement["type"], "music.state");
        let s = e.etat("music").await;
        assert_eq!(s["video"], "eeeeeeeeeee");
        assert_eq!(s["meta"]["title"], "Titre oEmbed");
        assert_eq!(s["meta"]["artist"], "Chaîne");
        assert_eq!(s["meta"]["cover"], "/music/cover/eeeeeeeeeee");
        assert_eq!(s["repeat"], "");
        assert_eq!(e.listees("music").await, 1);
        assert_eq!(e.listees("watch").await, 0);

        let file = format!("{musique}/queue");
        assert_eq!(
            e.code("alice", "POST", &file, &piste("ddddddddddd")).await,
            204
        );
        assert_eq!(
            e.code("alice", "POST", &file, &piste("ccccccccccc")).await,
            204
        );
        let action = format!("{musique}/action");
        let rev = e.etat("music").await["rev"].as_i64().unwrap();
        let fin = format!(r#"{{"action":"ended","rev":{rev}}}"#);
        assert_eq!(e.code("alice", "POST", &action, &fin).await, 204);
        let s = e.etat("music").await;
        assert_eq!(s["video"], "ddddddddddd");
        assert_eq!(s["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            e.code("alice", "POST", &action, r#"{"action":"previous"}"#)
                .await,
            204
        );
        let s = e.etat("music").await;
        assert_eq!(s["video"], "eeeeeeeeeee");
        assert!(s["history"].as_array().unwrap().is_empty());
        assert_eq!(s["queue"].as_array().unwrap().len(), 2);
        assert_eq!(
            e.code("alice", "POST", &action, r#"{"action":"shuffle"}"#)
                .await,
            204
        );
        assert_eq!(e.etat("music").await["queue"].as_array().unwrap().len(), 2);

        let options = format!("{musique}/options");
        assert_eq!(
            e.code("alice", "PUT", &options, r#"{"repeat":"one"}"#)
                .await,
            204
        );
        let rev = e.etat("music").await["rev"].as_i64().unwrap();
        let fin = format!(r#"{{"action":"ended","rev":{rev}}}"#);
        assert_eq!(e.code("alice", "POST", &action, &fin).await, 204);
        let s = e.etat("music").await;
        assert_eq!(
            (s["video"].as_str(), s["act"].as_str()),
            (Some("eeeeeeeeeee"), Some("repeat"))
        );
        assert_eq!(
            e.code("alice", "PUT", &options, r#"{"repeat":"toujours"}"#)
                .await,
            400
        );
        assert_eq!(e.etat("music").await["host_only"], false);

        for mauvais in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://vimeo.com/123456789",
            "https://music.youtube.com/playlist?list=OLAK5uy_abcdefgh",
        ] {
            let corps = json!({ "video": mauvais }).to_string();
            assert_eq!(
                e.code("alice", "PUT", &musique, &corps).await,
                400,
                "{mauvais}"
            );
            assert_eq!(
                e.code("alice", "POST", &file, &corps).await,
                400,
                "{mauvais}"
            );
        }

        let video = e.chemin("watch");
        assert_eq!(
            e.code("alice", "PUT", &video, &format!(r#"{{"video":"{V1}"}}"#))
                .await,
            204
        );
        let liste = r#"{"video":"https://music.youtube.com/playlist?list=OLAK5uy_abcdefgh"}"#;
        assert_eq!(e.code("alice", "PUT", &video, liste).await, 400);
        assert_eq!(e.etat("music").await["video"], "eeeeeeeeeee");
        assert_eq!(e.code("alice", "DELETE", &video, "").await, 204);
        assert!(!e.etat("music").await.is_null());
        assert_eq!(e.code("alice", "DELETE", &musique, "").await, 204);
        assert!(e.etat("music").await.is_null());
    }

    #[tokio::test]
    async fn salons_et_acces_refuses() {
        let Some(e) = env(true).await else { return };
        e.vocal.presents(&e.salon, &[&e.alice]);
        let texte = format!("/voice/{}/watch", e.texte);
        assert_eq!(e.code("alice", "GET", &texte, "").await, 400);
        assert_eq!(
            e.code("alice", "GET", "/voice/pas-un-uuid/watch", "").await,
            404
        );
        let inconnu = "/voice/00000000-0000-0000-0000-000000000000/watch";
        assert_eq!(e.code("alice", "GET", inconnu, "").await, 404);
        assert_eq!(
            e.code("alice", "GET", "/servers/pas-un-uuid/watch", "")
                .await,
            404
        );
        let (s, _, v) = e.appel("alice", "GET", &e.chemin("watch"), "").await;
        assert_eq!(s, StatusCode::OK);
        assert!(v["state"].is_null());
        assert!(v["now"].as_str().unwrap().ends_with('Z'));
        assert_eq!(e.code("alice", "PUT", &e.chemin("watch"), "[1]").await, 400);
    }

    #[tokio::test]
    async fn sans_vocal_branche_501() {
        let Some(e) = env(false).await else { return };
        assert_eq!(e.code("alice", "GET", &e.chemin("watch"), "").await, 501);
        assert_eq!(e.code("alice", "GET", &e.chemin("music"), "").await, 501);
    }
}
