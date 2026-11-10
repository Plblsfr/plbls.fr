#![allow(clippy::result_large_err)]

mod filtre;
mod flux;
mod jellyfin;
mod sous_titres;

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    Extension,
    body::Body,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use estompe_core::{
    administration::Admin,
    api::ActiveUser,
    http::{champ, decode, internal, text_error, write_err, write_json},
    identity::cred_of,
    openapi,
    servers::{horodatage, rfc3339},
    state::{AppState, FilmEtSuite, FilmResolu, Films, RefusFilm},
    users::{self, User},
};
use flux::{Acces, Flux, Lecture, Limites, Mode, Ouverture, Refus, maintenant_unix};
use futures_util::stream;
use jellyfin::{Element, Jellyfin, Liste, Panne, envoyer, id_valide, lire_borne};
use reqwest::Url;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const BIBLIOTHEQUE_MAX: usize = 120;
const IMAGES_MAX: usize = 1200;
const FENETRE: Duration = Duration::from_secs(60);
const LARGEURS: [u32; 3] = [240, 480, 1280];
const PLAYLIST_MAX: usize = 2 << 20;
const DEBIT_VIDEO_MAX: u32 = 120_000_000;
const POSITION_MAX: f64 = 24.0 * 3600.0;
const REPRISE_MAX: i64 = 20;
const VUS_MAX: i64 = 100;
const SUITE_MAX: usize = 50;

struct Contexte {
    flux: Arc<Flux>,
    jellyfin: Jellyfin,
}

impl Contexte {
    fn nouveau() -> Arc<Self> {
        Arc::new(Contexte {
            flux: Arc::default(),
            jellyfin: Jellyfin::nouveau(),
        })
    }
}

fn partage() -> Arc<Contexte> {
    static PARTAGE: OnceLock<Arc<Contexte>> = OnceLock::new();
    PARTAGE.get_or_init(Contexte::nouveau).clone()
}

pub fn mount(routeur: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    routeur.merge(routes(partage()))
}

pub fn brancher(state: &AppState) {
    if state.config().entracte_actif() {
        return;
    }
    state.definir_films(Arc::new(Resolveur {
        state: state.clone(),
        ctx: partage(),
    }));
}

fn panne(_: Panne) -> Response {
    write_err(
        StatusCode::BAD_GATEWAY,
        "jellyfin_unavailable",
        "la bibliothèque est injoignable",
    )
}

fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("films : base : {e}");
    internal()
}

fn introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "titre introuvable")
}

fn mauvais(message: &str) -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", message)
}

fn jeton_refuse() -> Response {
    write_err(
        StatusCode::UNAUTHORIZED,
        "stream_expired",
        "flux expiré ou invalide",
    )
}

fn pool(state: &AppState) -> Result<&PgPool, Response> {
    state.pool().map_err(IntoResponse::into_response)
}

fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

async fn autoriser(state: &AppState, sub: &str) -> Result<(), Response> {
    if !state.config().jellyfin_actif() {
        return Err(text_error(StatusCode::NOT_FOUND, "404 page not found"));
    }
    match state.droit(sub, estompe_core::catalogue::FILMS).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "accès aux films et séries non accordé",
        )),
        Err(_) => Err(write_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "permission_unavailable",
            "service de permission injoignable",
        )),
    }
}

struct Spectateur(User);

impl FromRequestParts<AppState> for Spectateur {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Response> {
        let ActiveUser(user) = ActiveUser::from_request_parts(parts, state).await?;
        autoriser(state, &user.sub).await?;
        Ok(Spectateur(user))
    }
}

fn limiter(state: &AppState, user: &User, famille: &str, max: usize) -> Result<(), Response> {
    let cle = format!("{famille}:{}", user.id);
    if state.limiteurs().films.allow(&cle, max, FENETRE) {
        return Ok(());
    }
    let mut r = write_err(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "trop de requêtes, patientez un instant",
    );
    r.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    Err(r)
}

async fn publics(pool: &PgPool, ids: &[String]) -> Result<HashMap<String, String>, Response> {
    sqlx::query(
        "INSERT INTO films_items (jellyfin_id) SELECT unnest($1::text[]) ON CONFLICT (jellyfin_id) DO NOTHING",
    )
    .bind(ids)
    .execute(pool)
    .await
    .map_err(bd)?;
    let lignes = sqlx::query(
        "SELECT jellyfin_id, id::text FROM films_items WHERE jellyfin_id = ANY($1::text[])",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    Ok(lignes
        .iter()
        .map(|l| (l.get::<String, _>(0), l.get::<String, _>(1)))
        .collect())
}

async fn jellyfin_id(pool: &PgPool, id: &str) -> Result<String, Response> {
    if !uuid_valide(id) {
        return Err(introuvable());
    }
    sqlx::query_scalar::<_, String>("SELECT jellyfin_id FROM films_items WHERE id=$1::uuid")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(bd)?
        .filter(|j| id_valide(j))
        .ok_or_else(introuvable)
}

async fn retirer_rejets(pool: &PgPool, liste: &mut Liste) -> Result<(), Response> {
    let rejets: Vec<(String, String, &'static str)> = liste
        .items
        .iter()
        .filter_map(|e| {
            e.rejet().map(|r| {
                (
                    e.id.clone(),
                    filtre::nom_de_fichier(e.chemin()).to_string(),
                    r.code(),
                )
            })
        })
        .collect();
    if rejets.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = rejets.iter().map(|(id, _, _)| id.clone()).collect();
    let correspondance = publics(pool, &ids).await?;
    for (id, nom, raison) in &rejets {
        let Some(public) = correspondance.get(id) else {
            continue;
        };
        tracing::warn!("films : fichier écarté ({raison}) : {nom}");
        sqlx::query(
            "INSERT INTO films_rejets (item_id, nom, raison) VALUES ($1::uuid,$2,$3)
             ON CONFLICT (item_id, raison) DO UPDATE SET nom=EXCLUDED.nom, vu_le=now()",
        )
        .bind(public)
        .bind(nom)
        .bind(raison)
        .execute(pool)
        .await
        .map_err(bd)?;
    }
    liste.items.retain(|e| e.rejet().is_none());
    liste.total = (liste.total - rejets.len() as i64).max(0);
    Ok(())
}

async fn lire_frais(
    state: &AppState,
    ctx: &Contexte,
    parametres: &[(&str, String)],
) -> Result<Liste, Response> {
    let mut liste = ctx
        .jellyfin
        .liste(state.config(), parametres)
        .await
        .map_err(panne)?;
    retirer_rejets(pool(state)?, &mut liste).await?;
    Ok(liste)
}

async fn lire(
    state: &AppState,
    ctx: &Contexte,
    parametres: &[(&str, String)],
) -> Result<Arc<Liste>, Response> {
    let cle = parametres
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let duree = Duration::from_secs(state.config().films_cache_secondes as u64);
    if let Some(l) = ctx.jellyfin.liste_en_cache(&cle, duree) {
        return Ok(l);
    }
    let liste = Arc::new(lire_frais(state, ctx, parametres).await?);
    ctx.jellyfin.garder_liste(cle, liste.clone(), duree);
    Ok(liste)
}

fn seul(id: &str) -> Vec<(&'static str, String)> {
    vec![("Ids", id.to_string())]
}

struct Progression {
    position: f64,
    duree: f64,
    vu: bool,
}

async fn progressions(
    pool: &PgPool,
    utilisateur: &str,
    ids: &[String],
) -> Result<HashMap<String, Progression>, Response> {
    let lignes = sqlx::query(
        "SELECT item_id::text, position, duree, vu FROM films_progression
         WHERE user_id=$1::uuid AND item_id = ANY($2::text[]::uuid[])",
    )
    .bind(utilisateur)
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    Ok(lignes
        .iter()
        .map(|l| {
            (
                l.get::<String, _>(0),
                Progression {
                    position: l.get(1),
                    duree: l.get(2),
                    vu: l.get(3),
                },
            )
        })
        .collect())
}

fn genre_public(genre: Option<&str>) -> &'static str {
    match genre {
        Some("Movie") => "movie",
        Some("Series") => "series",
        Some("Season") => "season",
        Some("Episode") => "episode",
        _ => "other",
    }
}

fn a_image(e: &Element, genre: &str) -> bool {
    if genre == "Backdrop" {
        return e
            .backdrop_image_tags
            .as_ref()
            .is_some_and(|t| !t.is_empty());
    }
    e.image_tags.as_ref().is_some_and(|t| t.contains_key(genre))
}

fn public(
    e: &Element,
    id: &str,
    serie: Option<&String>,
    progression: Option<&Progression>,
) -> Value {
    json!({
        "id": id,
        "kind": genre_public(e.genre.as_deref()),
        "title": e.titre(),
        "year": e.production_year,
        "overview": e.overview,
        "duration": e.duree(),
        "genres": e.genres.clone().unwrap_or_default(),
        "season": e.parent_index_number,
        "episode": e.index_number,
        "series_id": serie,
        "series_title": e.series_name,
        "poster": a_image(e, "Primary"),
        "backdrop": a_image(e, "Backdrop"),
        "progress": progression.map(|p| json!({
            "position": p.position,
            "duration": p.duree,
            "seen": p.vu,
        })),
    })
}

async fn publier_elements(
    pool: &PgPool,
    utilisateur: &str,
    items: &[Element],
) -> Result<Vec<Value>, Response> {
    let mut ids: Vec<String> = items.iter().map(|e| e.id.clone()).collect();
    ids.extend(
        items
            .iter()
            .filter_map(|e| e.series_id.clone())
            .filter(|s| id_valide(s)),
    );
    let correspondance = publics(pool, &ids).await?;
    let uuids: Vec<String> = correspondance.values().cloned().collect();
    let suivis = progressions(pool, utilisateur, &uuids).await?;
    Ok(items
        .iter()
        .filter_map(|e| {
            let id = correspondance.get(&e.id)?;
            let serie = e.series_id.as_ref().and_then(|s| correspondance.get(s));
            Some(public(e, id, serie, suivis.get(id)))
        })
        .collect())
}

fn annees(texte: &str) -> Option<String> {
    let (debut, fin) = texte.split_once('-').unwrap_or((texte, texte));
    let (debut, fin) = (debut.parse::<u16>().ok()?, fin.parse::<u16>().ok()?);
    let valide = |a: u16| (1880..=2100).contains(&a);
    (valide(debut) && valide(fin) && debut <= fin).then(|| {
        (debut..=fin)
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",")
    })
}

fn parametres_bibliotheque(
    q: &HashMap<String, String>,
) -> Result<Vec<(&'static str, String)>, Response> {
    let mut p: Vec<(&'static str, String)> = vec![("Recursive", "true".into())];
    let genres = match q.get("kind").map(String::as_str) {
        None | Some("") => "Movie,Series",
        Some("movie") => "Movie",
        Some("series") => "Series",
        Some(_) => return Err(mauvais("type inconnu")),
    };
    p.push(("IncludeItemTypes", genres.into()));
    if let Some(t) = q.get("q").filter(|t| !t.is_empty()) {
        if t.chars().count() > 100 {
            return Err(mauvais("recherche trop longue"));
        }
        p.push(("SearchTerm", t.clone()));
    }
    if let Some(g) = q.get("genre").filter(|g| !g.is_empty()) {
        if g.chars().count() > 64 {
            return Err(mauvais("genre trop long"));
        }
        p.push(("Genres", g.clone()));
    }
    if let Some(a) = q.get("year").filter(|a| !a.is_empty()) {
        p.push(("Years", annees(a).ok_or_else(|| mauvais("année invalide"))?));
    }
    let (tri, sens) = match q.get("sort").map(String::as_str) {
        None | Some("") | Some("recent") => ("DateCreated,SortName", "Descending"),
        Some("title") => ("SortName", "Ascending"),
        Some("year") => ("ProductionYear,SortName", "Descending"),
        Some(_) => return Err(mauvais("tri inconnu")),
    };
    p.push(("SortBy", tri.into()));
    p.push(("SortOrder", sens.into()));
    let entier = |nom: &str, def: u32, max: u32| match q.get(nom).filter(|v| !v.is_empty()) {
        None => Ok(def),
        Some(v) => v
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= max)
            .ok_or_else(|| mauvais("pagination invalide")),
    };
    p.push(("StartIndex", entier("start", 0, 100_000)?.to_string()));
    p.push(("Limit", entier("limit", 40, 100)?.max(1).to_string()));
    Ok(p)
}

async fn bibliotheque(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    limiter(&state, &user, "bibliotheque", BIBLIOTHEQUE_MAX)?;
    let p = parametres_bibliotheque(&q)?;
    let liste = lire(&state, &ctx, &p).await?;
    let items = publier_elements(pool(&state)?, &user.id, &liste.items).await?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "items": items, "total": liste.total }),
    ))
}

async fn genres(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
) -> Result<Response, Response> {
    limiter(&state, &user, "bibliotheque", BIBLIOTHEQUE_MAX)?;
    let noms = ctx.jellyfin.genres(state.config()).await.map_err(panne)?;
    Ok(write_json(StatusCode::OK, &json!({ "genres": noms })))
}

fn pistes(e: &Element) -> Value {
    let decrire = |n: usize, p: &jellyfin::Piste| {
        json!({
            "n": n,
            "label": p.display_title,
            "language": p.language,
        })
    };
    json!({
        "audio": e.audio().iter().enumerate().map(|(n, p)| decrire(n, p)).collect::<Vec<_>>(),
        "subtitles": e.sous_titres().iter().enumerate().map(|(n, p)| decrire(n, p)).collect::<Vec<_>>(),
    })
}

async fn detail(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    limiter(&state, &user, "bibliotheque", BIBLIOTHEQUE_MAX)?;
    let pool = pool(&state)?;
    let jid = jellyfin_id(pool, &id).await?;
    let liste = lire(&state, &ctx, &seul(&jid)).await?;
    let e = liste.items.first().ok_or_else(introuvable)?;
    let mut v = publier_elements(pool, &user.id, std::slice::from_ref(e))
        .await?
        .pop()
        .ok_or_else(introuvable)?;
    if let (Some(o), Value::Object(p)) = (v.as_object_mut(), pistes(e)) {
        o.extend(p);
    }
    Ok(write_json(StatusCode::OK, &v))
}

async fn enfants(
    state: &AppState,
    ctx: &Contexte,
    user: &User,
    id: &str,
    genre: &'static str,
) -> Result<Response, Response> {
    limiter(state, user, "bibliotheque", BIBLIOTHEQUE_MAX)?;
    let pool = pool(state)?;
    let jid = jellyfin_id(pool, id).await?;
    let p = vec![
        ("ParentId", jid),
        ("IncludeItemTypes", genre.to_string()),
        ("SortBy", "IndexNumber".to_string()),
        ("SortOrder", "Ascending".to_string()),
        ("Limit", "500".to_string()),
    ];
    let liste = lire(state, ctx, &p).await?;
    let items = publier_elements(pool, &user.id, &liste.items).await?;
    Ok(write_json(StatusCode::OK, &json!({ "items": items })))
}

async fn saisons(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    enfants(&state, &ctx, &user, &id, "Season").await
}

async fn episodes(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    enfants(&state, &ctx, &user, &id, "Episode").await
}

async fn image(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    limiter(&state, &user, "images", IMAGES_MAX)?;
    let genre = match q.get("type").map(String::as_str) {
        None | Some("poster") => "Primary",
        Some("fond") => "Backdrop",
        Some(_) => return Err(mauvais("type d'image inconnu")),
    };
    let largeur = match q.get("w") {
        None => 480,
        Some(w) => w
            .parse::<u32>()
            .ok()
            .filter(|w| LARGEURS.contains(w))
            .ok_or_else(|| mauvais("largeur non proposée"))?,
    };
    let jid = jellyfin_id(pool(&state)?, &id).await?;
    let image = ctx
        .jellyfin
        .image(state.config(), &jid, genre, largeur)
        .await
        .map_err(panne)?
        .ok_or_else(introuvable)?;
    let mut r = Response::new(Body::from(image.octets.clone()));
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, image.mime.clone());
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(r)
}

async fn suivis(
    state: &AppState,
    ctx: &Contexte,
    user: &User,
    vus: bool,
    max: i64,
) -> Result<Response, Response> {
    limiter(state, user, "bibliotheque", BIBLIOTHEQUE_MAX)?;
    let pool = pool(state)?;
    let ordre: Vec<String> = sqlx::query_scalar(
        "SELECT i.jellyfin_id FROM films_progression p JOIN films_items i ON i.id = p.item_id
         WHERE p.user_id=$1::uuid AND p.vu=$2 AND p.position > 0 ORDER BY p.maj DESC LIMIT $3",
    )
    .bind(&user.id)
    .bind(vus)
    .bind(max)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    if ordre.is_empty() {
        return Ok(write_json(StatusCode::OK, &json!({ "items": [] })));
    }
    let liste = lire_frais(state, ctx, &seul(&ordre.join(","))).await?;
    let tries: Vec<Element> = ordre
        .iter()
        .filter_map(|j| liste.items.iter().find(|e| e.id == *j).cloned())
        .collect();
    let items = publier_elements(pool, &user.id, &tries).await?;
    Ok(write_json(StatusCode::OK, &json!({ "items": items })))
}

async fn reprise(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
) -> Result<Response, Response> {
    suivis(&state, &ctx, &user, false, REPRISE_MAX).await
}

async fn deja_vus(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
) -> Result<Response, Response> {
    suivis(&state, &ctx, &user, true, VUS_MAX).await
}

async fn progression(
    State(state): State<AppState>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let corps = decode(corps).await?;
    let lire_nombre = |nom: &str| champ(&corps, nom).and_then(Value::as_f64);
    let (Some(position), Some(duree)) = (lire_nombre("position"), lire_nombre("duration")) else {
        return Err(mauvais("position et durée requises"));
    };
    if !(0.0..=POSITION_MAX).contains(&position) || !(duree > 0.0 && duree <= POSITION_MAX) {
        return Err(mauvais("position ou durée invalide"));
    }
    let pool = pool(&state)?;
    jellyfin_id(pool, &id).await?;
    sqlx::query(
        "INSERT INTO films_progression (user_id, item_id, position, duree, vu)
         VALUES ($1::uuid,$2::uuid,$3,$4,$5)
         ON CONFLICT (user_id, item_id) DO UPDATE SET position=EXCLUDED.position, duree=EXCLUDED.duree,
           vu=films_progression.vu OR EXCLUDED.vu, maj=now()",
    )
    .bind(&user.id)
    .bind(&id)
    .bind(position)
    .bind(duree)
    .bind(position >= duree * 0.9)
    .execute(pool)
    .await
    .map_err(bd)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn limites(state: &AppState) -> Limites {
    let c = state.config();
    Limites {
        flux: c.films_max_flux as usize,
        transcodages: c.films_max_transcodages as usize,
    }
}

async fn lecture(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let corps = decode(corps).await?;
    let direct = champ(&corps, "direct")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let audio = match champ(&corps, "audio") {
        None | Some(Value::Null) => 0,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| mauvais("piste audio invalide"))?,
    };
    let pool = pool(&state)?;
    let jid = jellyfin_id(pool, &id).await?;
    let liste = lire_frais(&state, &ctx, &seul(&jid)).await?;
    let e = liste
        .items
        .first()
        .filter(|e| e.jouable())
        .ok_or_else(introuvable)?;
    let pistes_audio = e.audio();
    if audio > 0 && audio >= pistes_audio.len() {
        return Err(mauvais("piste audio invalide"));
    }
    let sous_titres = e.sous_titres();
    let mode = if direct && e.direct_possible(audio) {
        Mode::Direct
    } else {
        Mode::Hls
    };
    let ouverture = Ouverture {
        utilisateur: user.id.clone(),
        sub: user.sub.clone(),
        jellyfin_id: e.id.clone(),
        source_id: e.source_id().to_string(),
        mode,
        audio: pistes_audio.get(audio).and_then(|p| p.index),
        sous_titres: sous_titres.iter().filter_map(|p| p.index).collect(),
    };
    let ouvert = ctx.flux.ouvrir(
        ouverture,
        limites(&state),
        &state.config().stream_secret,
        Instant::now(),
        maintenant_unix(),
    );
    let (flux_id, signe, remplaces) = match ouvert {
        Ok(o) => o,
        Err(Refus::TropDeFlux) => {
            return Err(write_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_streams",
                "trop de lectures en cours sur ce compte",
            ));
        }
        Err(Refus::ServeurCharge) => {
            return Err(write_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "serveur_charge",
                "le serveur est trop chargé, réessayez plus tard",
            ));
        }
    };
    for ancien in &remplaces {
        ctx.jellyfin
            .arreter_transcodage(state.config(), ancien)
            .await;
    }
    let base = format!("/films/stream/{signe}");
    let (nom_mode, url) = match mode {
        Mode::Direct => ("direct", format!("{base}/direct")),
        Mode::Hls => ("hls", format!("{base}/hls.m3u8")),
    };
    let sous_titres: Vec<Value> = sous_titres
        .iter()
        .enumerate()
        .map(|(n, p)| {
            json!({
                "n": n,
                "label": p.display_title,
                "language": p.language,
                "url": format!("{base}/sub/{n}"),
            })
        })
        .collect();
    Ok(write_json(
        StatusCode::OK,
        &json!({
            "stream_id": flux_id,
            "mode": nom_mode,
            "url": url,
            "duration": e.duree(),
            "audio": pistes(e)["audio"],
            "subtitles": sous_titres,
        }),
    ))
}

async fn fermer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Spectateur(user): Spectateur,
    Path(flux): Path<String>,
) -> Result<Response, Response> {
    match ctx.flux.fermer(&flux, &user.id) {
        None => Err(write_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "flux introuvable",
        )),
        Some(mode) => {
            if mode == Mode::Hls {
                ctx.jellyfin
                    .arreter_transcodage(state.config(), &flux)
                    .await;
            }
            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}

async fn acces(
    state: &AppState,
    ctx: &Contexte,
    jeton: &str,
    entetes: &HeaderMap,
) -> Result<Acces, Response> {
    let acces = ctx
        .flux
        .valider(
            jeton,
            &state.config().stream_secret,
            Instant::now(),
            maintenant_unix(),
        )
        .ok_or_else(jeton_refuse)?;
    let sub = &acces.ouverture.sub;
    if cred_of(entetes).is_some()
        && let Ok(idn) = state.identity().resolve(cred_of(entetes).as_ref()).await
        && idn.sub != *sub
    {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce flux appartient à un autre utilisateur",
        ));
    }
    autoriser(state, sub).await?;
    match users::user_by_sub(pool(state)?, sub).await {
        Ok(Some(u)) if !u.suspended => Ok(acces),
        Ok(_) => Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "compte suspendu",
        )),
        Err(_) => Err(internal()),
    }
}

fn entetes_de_flux(r: &mut Response, mime: &'static str) {
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

fn plage(entetes: &HeaderMap) -> Option<HeaderValue> {
    let v = entetes.get(header::RANGE)?;
    let texte = v.to_str().ok()?;
    let (debut, fin) = texte.strip_prefix("bytes=")?.split_once('-')?;
    (debut.bytes().all(|b| b.is_ascii_digit())
        && fin.bytes().all(|b| b.is_ascii_digit())
        && !(debut.is_empty() && fin.is_empty())
        && debut.len() <= 20
        && fin.len() <= 20)
        .then(|| v.clone())
}

fn relayer(
    amont: reqwest::Response,
    garde: Lecture,
    mime: &'static str,
) -> Result<Response, Response> {
    let statut = amont.status();
    if statut != reqwest::StatusCode::OK && statut != reqwest::StatusCode::PARTIAL_CONTENT {
        tracing::warn!("films : flux Jellyfin → {}", statut.as_u16());
        return Err(panne(Panne));
    }
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::from_u16(statut.as_u16()).map_err(|_| internal())?;
    for nom in [
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(v) = amont.headers().get(nom.as_str())
            && let Ok(v) = HeaderValue::from_bytes(v.as_bytes())
        {
            r.headers_mut().insert(nom, v);
        }
    }
    entetes_de_flux(&mut r, mime);
    *r.body_mut() = Body::from_stream(stream::unfold(
        (amont, garde),
        |(mut amont, garde)| async move {
            match amont.chunk().await {
                Ok(Some(morceau)) => Some((Ok::<_, reqwest::Error>(morceau), (amont, garde))),
                Ok(None) => None,
                Err(e) => Some((Err(e.without_url()), (amont, garde))),
            }
        },
    ));
    Ok(r)
}

fn requete_amont(
    state: &AppState,
    ctx: &Contexte,
    url: Url,
    entetes: &HeaderMap,
) -> reqwest::RequestBuilder {
    let mut r = ctx.jellyfin.requete(state.config(), url);
    if let Some(p) = plage(entetes)
        && let Ok(valeur) = p.to_str()
    {
        r = r.header("Range", valeur);
    }
    r
}

async fn direct(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Path(jeton): Path<String>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &ctx, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Direct {
        return Err(jeton_refuse());
    }
    let mut url = ctx
        .jellyfin
        .url(
            state.config(),
            &format!("/Videos/{}/stream", a.ouverture.jellyfin_id),
        )
        .map_err(panne)?;
    url.query_pairs_mut()
        .append_pair("static", "true")
        .append_pair("mediaSourceId", &a.ouverture.source_id);
    let amont = envoyer(requete_amont(&state, &ctx, url, &entetes))
        .await
        .map_err(|e| {
            tracing::warn!("films : flux direct : {}", e.without_url());
            panne(Panne)
        })?;
    let mime = match amont
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
    {
        Some(c) if c.starts_with("video/webm") => "video/webm",
        _ => "video/mp4",
    };
    relayer(amont, ctx.flux.lecture(&a.flux), mime)
}

async fn repondre_playlist(
    ctx: &Contexte,
    a: &Acces,
    amont: reqwest::Response,
    base: Url,
    prefixe: &str,
) -> Result<Response, Response> {
    if !amont.status().is_success() {
        tracing::warn!("films : playlist Jellyfin → {}", amont.status().as_u16());
        return Err(panne(Panne));
    }
    let octets = lire_borne(amont, PLAYLIST_MAX).await.map_err(panne)?;
    let texte = String::from_utf8(octets).map_err(|_| panne(Panne))?;
    let sortie = flux::reecrire_playlist(&texte, &base, prefixe, |uri| {
        ctx.flux.enregistrer_uri(&a.flux, uri)
    })
    .ok_or_else(|| {
        tracing::warn!("films : playlist Jellyfin refusée");
        panne(Panne)
    })?;
    let mut r = Response::new(Body::from(sortie));
    entetes_de_flux(&mut r, "application/vnd.apple.mpegurl");
    Ok(r)
}

async fn hls(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Path(jeton): Path<String>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &ctx, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Hls {
        return Err(jeton_refuse());
    }
    let mut url = ctx
        .jellyfin
        .url(
            state.config(),
            &format!("/Videos/{}/master.m3u8", a.ouverture.jellyfin_id),
        )
        .map_err(panne)?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("MediaSourceId", &a.ouverture.source_id)
            .append_pair("DeviceId", "estompe")
            .append_pair("PlaySessionId", &a.flux)
            .append_pair("VideoCodec", "h264")
            .append_pair("VideoBitrate", &DEBIT_VIDEO_MAX.to_string())
            .append_pair("AudioCodec", "aac")
            .append_pair("SegmentContainer", "ts")
            .append_pair("TranscodingMaxAudioChannels", "2");
        if let Some(n) = a.ouverture.audio {
            q.append_pair("AudioStreamIndex", &n.to_string());
        }
    }
    let amont = envoyer(ctx.jellyfin.requete(state.config(), url.clone()))
        .await
        .map_err(|e| {
            tracing::warn!("films : playlist : {}", e.without_url());
            panne(Panne)
        })?;
    repondre_playlist(&ctx, &a, amont, url, "u/").await
}

async fn morceau(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Path((jeton, n)): Path<(String, usize)>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &ctx, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Hls {
        return Err(jeton_refuse());
    }
    let uri = ctx.flux.uri(&a.flux, n).ok_or_else(introuvable)?;
    let url = Url::parse(&uri).map_err(|_| panne(Panne))?;
    let amont = envoyer(requete_amont(&state, &ctx, url.clone(), &entetes))
        .await
        .map_err(|e| {
            tracing::warn!("films : segment : {}", e.without_url());
            panne(Panne)
        })?;
    let type_amont = amont
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if type_amont.contains("mpegurl") || url.path().ends_with(".m3u8") {
        return repondre_playlist(&ctx, &a, amont, url, "").await;
    }
    let mime = if type_amont.starts_with("video/mp4") {
        "video/mp4"
    } else if type_amont.starts_with("audio/aac") {
        "audio/aac"
    } else {
        "video/mp2t"
    };
    relayer(amont, ctx.flux.lecture(&a.flux), mime)
}

async fn sous_titre(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    Path((jeton, n)): Path<(String, usize)>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &ctx, &jeton, &entetes).await?;
    let index = *a.ouverture.sous_titres.get(n).ok_or_else(introuvable)?;
    let url = ctx
        .jellyfin
        .url(
            state.config(),
            &format!(
                "/Videos/{}/{}/Subtitles/{index}/0/Stream.vtt",
                a.ouverture.jellyfin_id, a.ouverture.source_id
            ),
        )
        .map_err(panne)?;
    let amont = envoyer(ctx.jellyfin.requete(state.config(), url))
        .await
        .map_err(|e| {
            tracing::warn!("films : sous-titres : {}", e.without_url());
            panne(Panne)
        })?;
    if !amont.status().is_success() {
        return Err(panne(Panne));
    }
    let octets = lire_borne(amont, sous_titres::TAILLE_MAX)
        .await
        .map_err(panne)?;
    let mut r = Response::new(Body::from(sous_titres::nettoyer(&String::from_utf8_lossy(
        &octets,
    ))));
    entetes_de_flux(&mut r, "text/vtt; charset=utf-8");
    Ok(r)
}

async fn rejets(State(state): State<AppState>, _: Admin) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT nom, raison, {} FROM films_rejets ORDER BY vu_le DESC, nom LIMIT 500",
        horodatage("vu_le")
    )))
    .fetch_all(pool(&state)?)
    .await
    .map_err(bd)?;
    let items: Vec<Value> = lignes
        .iter()
        .map(|l| {
            json!({
                "name": l.get::<String, _>(0),
                "reason": l.get::<String, _>(1),
                "seen_at": rfc3339(&l.get::<String, _>(2)),
            })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &json!({ "items": items })))
}

struct Resolveur {
    state: AppState,
    ctx: Arc<Contexte>,
}

fn resolu(e: &Element, id: String) -> FilmResolu {
    let titre = match (
        e.series_name.as_deref(),
        e.parent_index_number,
        e.index_number,
    ) {
        (Some(serie), Some(s), Some(n)) => format!("{serie} · S{s}E{n} · {}", e.titre()),
        _ => e.titre().to_string(),
    };
    FilmResolu {
        id,
        titre,
        duree: e.duree(),
    }
}

impl Resolveur {
    async fn chercher(&self, sub: &str, id: &str) -> FilmEtSuite {
        let refus_droit = |r: Response| match r.status() {
            StatusCode::FORBIDDEN => RefusFilm::Permission,
            StatusCode::NOT_FOUND => RefusFilm::Introuvable,
            _ => RefusFilm::Indisponible,
        };
        autoriser(&self.state, sub).await.map_err(refus_droit)?;
        let pool = pool(&self.state).map_err(|_| RefusFilm::Indisponible)?;
        let jid = jellyfin_id(pool, id).await.map_err(refus_droit)?;
        let liste = lire_frais(&self.state, &self.ctx, &seul(&jid))
            .await
            .map_err(|_| RefusFilm::Indisponible)?;
        let e = liste
            .items
            .first()
            .filter(|e| e.jouable())
            .ok_or(RefusFilm::Introuvable)?;
        let premier = resolu(e, id.to_string());
        let Some(serie) = e.series_id.as_ref().filter(|s| id_valide(s)) else {
            return Ok((premier, Vec::new()));
        };
        let p = vec![
            ("ParentId", serie.clone()),
            ("Recursive", "true".to_string()),
            ("IncludeItemTypes", "Episode".to_string()),
            ("SortBy", "ParentIndexNumber,IndexNumber".to_string()),
            ("SortOrder", "Ascending".to_string()),
            ("Limit", "1000".to_string()),
        ];
        let tous = lire(&self.state, &self.ctx, &p)
            .await
            .map_err(|_| RefusFilm::Indisponible)?;
        let suivants: Vec<&Element> = tous
            .items
            .iter()
            .skip_while(|x| x.id != jid)
            .skip(1)
            .take(SUITE_MAX)
            .collect();
        let ids: Vec<String> = suivants.iter().map(|x| x.id.clone()).collect();
        let correspondance = publics(pool, &ids)
            .await
            .map_err(|_| RefusFilm::Indisponible)?;
        let suite = suivants
            .iter()
            .filter_map(|x| Some(resolu(x, correspondance.get(&x.id)?.clone())))
            .collect();
        Ok((premier, suite))
    }
}

impl Films for Resolveur {
    fn resoudre<'a>(
        &'a self,
        sub: &'a str,
        id: &'a str,
    ) -> Pin<Box<dyn Future<Output = FilmEtSuite> + Send + 'a>> {
        Box::pin(self.chercher(sub, id))
    }
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const AUTH: (&str, &str) = ("401", "Identité absente ou invalide");
const DROITS: (&str, &str) = ("403", "`forbidden` : permission `estompe:movies` requise");

const DOCS: [Doc; 16] = [
    (
        HttpMethod::Get,
        "/films/library",
        "Bibliothèque : recherche, filtres, tri et pagination",
        &[
            ("200", "ok"),
            ("400", "paramètre invalide"),
            AUTH,
            DROITS,
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/genres",
        "Genres proposés par la bibliothèque",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/continue",
        "Lectures commencées et non vues",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/seen",
        "Titres déjà vus, les plus récents d'abord",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/items/{id}",
        "Fiche d'un film, d'une série ou d'un épisode",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("404", "not_found"),
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/series/{id}/seasons",
        "Saisons d'une série",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("404", "not_found"),
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/seasons/{id}/episodes",
        "Épisodes d'une saison",
        &[
            ("200", "ok"),
            AUTH,
            DROITS,
            ("404", "not_found"),
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/items/{id}/image",
        "Affiche ou image de fond (`type=poster|fond`, `w=240|480|1280`)",
        &[
            ("200", "image"),
            ("400", "paramètre invalide"),
            AUTH,
            DROITS,
            ("404", "not_found"),
            ("429", "rate_limited"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Put,
        "/films/items/{id}/progress",
        "Enregistrer sa position de lecture",
        &[
            ("204", "ok"),
            ("400", "position ou durée invalide"),
            AUTH,
            DROITS,
            ("404", "not_found"),
        ],
    ),
    (
        HttpMethod::Post,
        "/films/items/{id}/playback",
        "Ouvrir un flux de lecture signé",
        &[
            ("200", "ok"),
            ("400", "piste audio invalide"),
            AUTH,
            DROITS,
            ("404", "not_found"),
            ("429", "`too_many_streams`"),
            ("502", "jellyfin_unavailable"),
            ("503", "`serveur_charge`"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/films/streams/{flux}",
        "Fermer son flux de lecture",
        &[("204", "ok"), AUTH, DROITS, ("404", "not_found")],
    ),
    (
        HttpMethod::Get,
        "/films/stream/{jeton}/direct",
        "Lecture directe (Range accepté)",
        &[
            ("200", "flux"),
            ("206", "plage"),
            ("401", "`stream_expired`"),
            DROITS,
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/stream/{jeton}/hls.m3u8",
        "Playlist HLS réécrite",
        &[
            ("200", "playlist"),
            ("401", "`stream_expired`"),
            DROITS,
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/stream/{jeton}/u/{n}",
        "Playlist ou segment HLS",
        &[
            ("200", "flux"),
            ("206", "plage"),
            ("401", "`stream_expired`"),
            DROITS,
            ("404", "not_found"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/films/stream/{jeton}/sub/{n}",
        "Sous-titres WebVTT nettoyés",
        &[
            ("200", "WebVTT"),
            ("401", "`stream_expired`"),
            DROITS,
            ("404", "not_found"),
            ("502", "jellyfin_unavailable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/admin/films/rejets",
        "Fichiers écartés (administrateurs)",
        &[
            ("200", "ok"),
            AUTH,
            ("403", "`forbidden` : réservé aux administrateurs"),
        ],
    ),
];

fn routes(ctx: Arc<Contexte>) -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::new()
        .route("/films/library", get(bibliotheque))
        .route("/films/genres", get(genres))
        .route("/films/continue", get(reprise))
        .route("/films/seen", get(deja_vus))
        .route("/films/items/{id}", get(detail))
        .route("/films/series/{id}/seasons", get(saisons))
        .route("/films/seasons/{id}/episodes", get(episodes))
        .route("/films/items/{id}/image", get(image))
        .route("/films/items/{id}/progress", put(progression))
        .route("/films/items/{id}/playback", post(lecture))
        .route("/films/streams/{flux}", delete(fermer))
        .route("/films/stream/{jeton}/direct", get(direct))
        .route("/films/stream/{jeton}/hls.m3u8", get(hls))
        .route("/films/stream/{jeton}/u/{n}", get(morceau))
        .route("/films/stream/{jeton}/sub/{n}", get(sous_titre))
        .route("/admin/films/rejets", get(rejets));
    for (methode, chemin, resume, reponses) in DOCS.iter() {
        let mut op = openapi::operation(
            if chemin.starts_with("/admin") {
                "administration"
            } else {
                "films"
            },
            resume,
            reponses,
            chemin,
        );
        if chemin.starts_with("/films/stream/") {
            op.security = Some(Vec::new());
        }
        r = openapi::documenter(r, chemin, methode.clone(), op);
    }
    r.layer(Extension(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::Request};
    use estompe_core::{
        db::migrate_embarquees,
        middleware, testutil,
        testutil::{BasePostgres, Faux, Requete, faux, faux_avec_entetes},
    };
    use tower::ServiceExt;

    const CLE: &str = "CLE-API-SECRETE";
    const SECRET: &str = "secret-de-flux-de-test-assez-long!";
    const CHEMIN: &str = "/medias/secret/Le.Film.2024.mp4";
    const JF_FILM: &str = "jf0film";
    const JF_MKV: &str = "jf1mkv";
    const JF_MAUVAIS: &str = "jf2mauvais";
    const JF_SERIE: &str = "jf3serie";
    const JF_SAISON: &str = "jf4saison";
    const JF_E1: &str = "jf5ep1";
    const JF_E2: &str = "jf6ep2";

    fn pistes_film(conteneur: &str, video: &str) -> Value {
        json!([
            { "Type": "Video", "Index": 0, "Codec": video, "Path": CHEMIN },
            { "Type": "Audio", "Index": 1, "Codec": "aac", "Language": "fra", "DisplayTitle": "Français" },
            { "Type": "Audio", "Index": 2, "Codec": "ac3", "Language": "eng", "DisplayTitle": conteneur },
            { "Type": "Subtitle", "Index": 3, "Codec": "subrip", "Language": "fra", "DisplayTitle": "Français" },
            { "Type": "Subtitle", "Index": 4, "Codec": "pgssub", "Language": "eng", "DisplayTitle": "Image" }
        ])
    }

    fn fichier(
        id: &str,
        genre: &str,
        nom: &str,
        chemin: &str,
        conteneur: &str,
        codec: &str,
    ) -> Value {
        json!({
            "Id": id,
            "Name": nom,
            "Type": genre,
            "Path": chemin,
            "Container": conteneur,
            "ProductionYear": 2024,
            "Overview": "Résumé",
            "RunTimeTicks": 72_000_000_000_i64,
            "Genres": ["Drame"],
            "ImageTags": { "Primary": "tag-secret" },
            "BackdropImageTags": ["tag-fond-secret"],
            "MediaSources": [{
                "Id": id,
                "Path": chemin,
                "Container": conteneur,
                "MediaStreams": pistes_film(conteneur, codec),
            }],
        })
    }

    fn film() -> Value {
        fichier(JF_FILM, "Movie", "Le Film", CHEMIN, "mov,mp4", "h264")
    }

    fn film_mkv() -> Value {
        fichier(JF_MKV, "Movie", "Autre", "/medias/Autre.mkv", "mkv", "hevc")
    }

    fn mauvais_fichier() -> Value {
        fichier(
            JF_MAUVAIS,
            "Movie",
            "Piege",
            "/medias/Piege.mkv.exe",
            "mkv",
            "h264",
        )
    }

    fn serie() -> Value {
        json!({ "Id": JF_SERIE, "Name": "La Série", "Type": "Series", "Path": "/medias/serie" })
    }

    fn episode(id: &str, n: i64) -> Value {
        let mut e = fichier(
            id,
            "Episode",
            &format!("Ep{n}"),
            &format!("/medias/serie/E{n}.mp4"),
            "mp4",
            "h264",
        );
        e["SeriesId"] = JF_SERIE.into();
        e["SeriesName"] = "La Série".into();
        e["ParentIndexNumber"] = 1.into();
        e["IndexNumber"] = n.into();
        e
    }

    fn items(v: Vec<Value>) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
        let total = v.len();
        (
            200,
            vec![("content-type", "application/json".to_string())],
            json!({ "Items": v, "TotalRecordCount": total })
                .to_string()
                .into_bytes(),
        )
    }

    fn param<'a>(r: &'a Requete, nom: &str) -> Option<&'a str> {
        r.query.split('&').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            (k == nom).then_some(v)
        })
    }

    fn jellyfin_faux(r: &Requete) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
        assert_eq!(
            r.entete("authorization"),
            format!("MediaBrowser Token=\"{CLE}\""),
            "clé d'API en en-tête"
        );
        if !r.chemin.ends_with("main.m3u8") && !r.chemin.ends_with("seg0.ts") {
            assert!(!r.query.contains(CLE), "clé d'API hors de l'URL");
        }
        let ligne = format!("{}{}", r.chemin, "");
        if ligne == "/Items" {
            if let Some(ids) = param(r, "Ids") {
                let tous = vec![
                    film(),
                    film_mkv(),
                    mauvais_fichier(),
                    serie(),
                    episode(JF_E1, 1),
                    episode(JF_E2, 2),
                ];
                return items(
                    tous.into_iter()
                        .filter(|e| ids.split("%2C").any(|i| e["Id"] == i))
                        .collect(),
                );
            }
            return match (param(r, "ParentId"), param(r, "IncludeItemTypes")) {
                (Some(JF_SERIE), Some("Season")) => items(vec![json!({
                    "Id": JF_SAISON, "Name": "Saison 1", "Type": "Season", "IndexNumber": 1, "SeriesId": JF_SERIE,
                })]),
                (Some(JF_SAISON | JF_SERIE), Some("Episode")) => {
                    items(vec![episode(JF_E1, 1), episode(JF_E2, 2)])
                }
                _ => items(vec![film(), film_mkv(), mauvais_fichier(), serie()]),
            };
        }
        if ligne == "/Genres" {
            return (
                200,
                vec![],
                json!({ "Items": [{ "Name": "Drame" }] })
                    .to_string()
                    .into_bytes(),
            );
        }
        if ligne.starts_with("/Items/") && ligne.contains("/Images/") {
            return (
                200,
                vec![("content-type", "image/jpeg".to_string())],
                b"octets-image".to_vec(),
            );
        }
        if ligne == format!("/Videos/{JF_FILM}/stream") {
            return if r.entete("range").is_empty() {
                (
                    200,
                    vec![
                        ("content-type", "video/mp4".to_string()),
                        (
                            "content-disposition",
                            "attachment; filename=secret.mp4".to_string(),
                        ),
                    ],
                    b"video-complete".to_vec(),
                )
            } else {
                (
                    206,
                    vec![
                        ("content-type", "video/mp4".to_string()),
                        ("content-range", "bytes 0-1/14".to_string()),
                    ],
                    b"vi".to_vec(),
                )
            };
        }
        if ligne.ends_with("/master.m3u8") {
            return (
                200,
                vec![("content-type", "application/vnd.apple.mpegurl".to_string())],
                format!("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nmain.m3u8?ApiKey={CLE}&MediaSourceId={JF_MKV}\n").into_bytes(),
            );
        }
        if ligne.ends_with("/main.m3u8") {
            return (
                200,
                vec![("content-type", "application/vnd.apple.mpegurl".to_string())],
                format!("#EXTM3U\n#EXTINF:6,\nhls1/{JF_MKV}/seg0.ts?ApiKey={CLE}\n").into_bytes(),
            );
        }
        if ligne.ends_with("/seg0.ts") {
            return (
                200,
                vec![("content-type", "video/mp2t".to_string())],
                b"segment".to_vec(),
            );
        }
        if ligne.ends_with("/Stream.vtt") {
            return (
                200,
                vec![("content-type", "text/vtt".to_string())],
                b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<script>alert(1)</script>Bonjour <i>toi</i>\n".to_vec(),
            );
        }
        (404, vec![], vec![])
    }

    struct Env {
        app: Router,
        pool: PgPool,
        jellyfin: Faux,
        ctx: Arc<Contexte>,
        state: AppState,
        _services: (Faux, Faux, BasePostgres),
    }

    async fn env() -> Option<Env> {
        env_avec(&[]).await
    }

    async fn env_avec(extra: &[(&str, &str)]) -> Option<Env> {
        let g = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            if nom.is_empty() {
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
        let p = testutil::faux_permission(|sub, perm| match perm {
            "estompe:movies" => sub != "sub-sans",
            "estompe:admin" => sub == "sub-admin",
            _ => true,
        })
        .await;
        let jellyfin = faux_avec_entetes(jellyfin_faux).await;
        let mut config = extra.to_vec();
        config.extend([
            ("GATEWAY_URL", g.url.as_str()),
            ("PERMISSION_URL", p.url.as_str()),
            ("JELLYFIN_URL", jellyfin.url.as_str()),
            ("JELLYFIN_API_KEY", CLE),
            ("STREAM_SECRET", SECRET),
        ]);
        let state = AppState::new(testutil::config_avec(&config), vec!["films".into()]);
        let (pg, url) = testutil::postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob", "sans", "admin"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        state.set_pool(pool.clone());
        let ctx = Contexte::nouveau();
        let app = middleware::envelopper(Router::from(routes(ctx.clone())), state.clone());
        Some(Env {
            app,
            pool,
            jellyfin,
            ctx,
            state,
            _services: (g, p, pg),
        })
    }

    struct Rep {
        statut: StatusCode,
        entetes: HeaderMap,
        octets: Vec<u8>,
    }

    impl Rep {
        fn json(&self) -> Value {
            serde_json::from_slice(&self.octets).unwrap_or(Value::Null)
        }

        fn texte(&self) -> String {
            String::from_utf8_lossy(&self.octets).into_owned()
        }
    }

    async fn appel(
        app: &Router,
        methode: &str,
        chemin: &str,
        qui: &str,
        corps: Option<Value>,
        range: Option<&str>,
    ) -> Rep {
        let mut r = Request::builder().method(methode).uri(chemin);
        if !qui.is_empty() {
            r = r.header(header::AUTHORIZATION, format!("Bearer {qui}"));
        }
        if let Some(p) = range {
            r = r.header(header::RANGE, p);
        }
        let corps = match corps {
            Some(v) => {
                r = r.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let rep = app.clone().oneshot(r.body(corps).unwrap()).await.unwrap();
        let (statut, entetes) = (rep.status(), rep.headers().clone());
        let octets = axum::body::to_bytes(rep.into_body(), 1 << 24)
            .await
            .unwrap()
            .to_vec();
        Rep {
            statut,
            entetes,
            octets,
        }
    }

    async fn get_de(e: &Env, chemin: &str, qui: &str) -> Rep {
        appel(&e.app, "GET", chemin, qui, None, None).await
    }

    async fn public_de(e: &Env, jid: &str) -> String {
        publics(&e.pool, &[jid.to_string()]).await.unwrap()[jid].clone()
    }

    async fn ouvrir(e: &Env, jid: &str, qui: &str, direct: bool) -> Rep {
        let id = public_de(e, jid).await;
        appel(
            &e.app,
            "POST",
            &format!("/films/items/{id}/playback"),
            qui,
            Some(json!({ "direct": direct })),
            None,
        )
        .await
    }

    fn sans_fuite(texte: &str, route: &str) {
        for interdit in [
            CLE,
            "/medias",
            "secret",
            "tag-",
            JF_FILM,
            JF_MKV,
            JF_MAUVAIS,
            JF_SERIE,
            JF_SAISON,
            JF_E1,
            JF_E2,
            "Piege",
            "ApiKey",
            "127.0.0.1",
        ] {
            assert!(
                !texte.contains(interdit),
                "{route} : {interdit} dans {texte}"
            );
        }
    }

    #[tokio::test]
    async fn refus_sans_la_permission_sur_chaque_route() {
        let Some(e) = env().await else { return };
        let id = public_de(&e, JF_FILM).await;
        let ouvert = ouvrir(&e, JF_FILM, "alice", true).await.json();
        let flux_id = ouvert["stream_id"].as_str().unwrap().to_string();
        let url = ouvert["url"].as_str().unwrap().to_string();
        let racine = url.trim_end_matches("/direct").to_string();
        for (m, c) in [
            ("GET", "/films/library".to_string()),
            ("GET", "/films/genres".to_string()),
            ("GET", "/films/continue".to_string()),
            ("GET", "/films/seen".to_string()),
            ("GET", format!("/films/items/{id}")),
            ("GET", format!("/films/series/{id}/seasons")),
            ("GET", format!("/films/seasons/{id}/episodes")),
            ("GET", format!("/films/items/{id}/image")),
            ("PUT", format!("/films/items/{id}/progress")),
            ("POST", format!("/films/items/{id}/playback")),
            ("DELETE", format!("/films/streams/{flux_id}")),
        ] {
            let corps =
                (m != "GET" && m != "DELETE").then(|| json!({ "position": 1, "duration": 2 }));
            let r = appel(&e.app, m, &c, "sans", corps, None).await;
            assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {c}");
            assert_eq!(r.json()["error"], "forbidden", "{m} {c}");
        }
        for chemin in ["/direct", "/hls.m3u8", "/u/0", "/sub/0"] {
            let c = format!("{racine}{chemin}");
            assert_eq!(
                get_de(&e, &c, "bob").await.statut,
                StatusCode::FORBIDDEN,
                "{c}"
            );
        }
    }

    #[tokio::test]
    async fn bloc_inactif_sans_jellyfin() {
        let Some(e) = env_avec(&[("JELLYFIN_URL", ""), ("JELLYFIN_API_KEY", "")]).await else {
            return;
        };
        assert_eq!(
            get_de(&e, "/films/library", "alice").await.statut,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn bibliotheque_filtre_et_ne_fuit_rien() {
        let Some(e) = env().await else { return };
        let r = get_de(
            &e,
            "/films/library?q=le&genre=Drame&year=2024&sort=title&limit=10&kind=movie",
            "alice",
        )
        .await;
        assert_eq!(r.statut, StatusCode::OK);
        let v = r.json();
        let titres: Vec<&str> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap())
            .collect();
        assert_eq!(titres, ["Le Film", "Autre", "La Série"]);
        assert_eq!(v["total"], 3);
        assert_eq!(v["items"][0]["duration"], 7200);
        assert_eq!(v["items"][0]["poster"], true);
        sans_fuite(&r.texte(), "library");
        let vue = &e.jellyfin.requetes()[0];
        assert_eq!(vue.chemin, "/Items");
        for attendu in [
            "SearchTerm=le",
            "Genres=Drame",
            "Years=2024",
            "SortBy=SortName",
            "Limit=10",
            "IncludeItemTypes=Movie",
        ] {
            assert!(vue.query.contains(attendu), "{attendu} : {}", vue.query);
        }
        let r = get_de(&e, "/films/library?year=2010-2012", "alice").await;
        assert_eq!(r.statut, StatusCode::OK);
        let vue = e.jellyfin.requetes().into_iter().last().unwrap();
        assert!(
            vue.query.contains("Years=2010%2C2011%2C2012"),
            "{}",
            vue.query
        );
        for mauvaise in [
            "year=1",
            "year=2012-2010",
            "year=1-99999",
            "sort=x",
            "limit=9999",
            "kind=x",
            "start=-1",
        ] {
            let r = get_de(&e, &format!("/films/library?{mauvaise}"), "alice").await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{mauvaise}");
        }
    }

    #[tokio::test]
    async fn cache_de_liste() {
        let Some(e) = env().await else { return };
        get_de(&e, "/films/library", "alice").await;
        let apres = e.jellyfin.appels();
        get_de(&e, "/films/library", "bob").await;
        assert_eq!(e.jellyfin.appels(), apres);
        get_de(&e, "/films/library?sort=title", "bob").await;
        assert_eq!(e.jellyfin.appels(), apres + 1);
    }

    #[tokio::test]
    async fn fiches_saisons_et_episodes_sans_fuite() {
        let Some(e) = env().await else { return };
        let id = public_de(&e, JF_FILM).await;
        let r = get_de(&e, &format!("/films/items/{id}"), "alice").await;
        assert_eq!(r.statut, StatusCode::OK);
        let v = r.json();
        assert_eq!(v["id"], id.as_str());
        assert_eq!(v["audio"].as_array().unwrap().len(), 2);
        assert_eq!(
            v["subtitles"].as_array().unwrap().len(),
            1,
            "sous-titres image écartés"
        );
        sans_fuite(&r.texte(), "item");

        let serie_id = public_de(&e, JF_SERIE).await;
        let r = get_de(&e, &format!("/films/series/{serie_id}/seasons"), "alice").await;
        assert_eq!(r.json()["items"][0]["title"], "Saison 1");
        sans_fuite(&r.texte(), "seasons");
        let saison = r.json()["items"][0]["id"].as_str().unwrap().to_string();
        let r = get_de(&e, &format!("/films/seasons/{saison}/episodes"), "alice").await;
        let v = r.json();
        assert_eq!(v["items"][1]["episode"], 2);
        assert_eq!(v["items"][1]["series_id"], serie_id.as_str());
        sans_fuite(&r.texte(), "episodes");

        let inconnu = "00000000-0000-0000-0000-000000000000";
        for id in [inconnu, "pas-un-uuid"] {
            assert_eq!(
                get_de(&e, &format!("/films/items/{id}"), "alice")
                    .await
                    .statut,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn images_relayees_avec_cache() {
        let Some(e) = env().await else { return };
        let id = public_de(&e, JF_FILM).await;
        let chemin = format!("/films/items/{id}/image?type=poster&w=240");
        let r = get_de(&e, &chemin, "alice").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.octets, b"octets-image");
        assert_eq!(r.entetes[header::CACHE_CONTROL], "private, max-age=3600");
        assert_eq!(r.entetes[header::CONTENT_TYPE], "image/jpeg");
        let avant = e.jellyfin.appels();
        get_de(&e, &chemin, "bob").await;
        assert_eq!(e.jellyfin.appels(), avant);
        let vue = e.jellyfin.requetes().into_iter().last().unwrap();
        assert!(vue.chemin.ends_with("/Images/Primary") && vue.query.contains("maxWidth=240"));
        for q in ["w=9999", "type=x"] {
            let r = get_de(&e, &format!("/films/items/{id}/image?{q}"), "alice").await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{q}");
        }
    }

    #[tokio::test]
    async fn progression_par_utilisateur() {
        let Some(e) = env().await else { return };
        let id = public_de(&e, JF_FILM).await;
        let chemin = format!("/films/items/{id}/progress");
        let envoi = |qui: &'static str, position: f64| {
            let (app, chemin) = (e.app.clone(), chemin.clone());
            async move {
                appel(
                    &app,
                    "PUT",
                    &chemin,
                    qui,
                    Some(json!({ "position": position, "duration": 7200 })),
                    None,
                )
                .await
                .statut
            }
        };
        assert_eq!(envoi("alice", 600.0).await, StatusCode::NO_CONTENT);
        let v = get_de(&e, "/films/continue", "alice").await.json();
        assert_eq!(v["items"][0]["id"], id.as_str());
        assert_eq!(v["items"][0]["progress"]["position"], 600.0);
        assert_eq!(v["items"][0]["progress"]["seen"], false);
        assert_eq!(
            get_de(&e, "/films/continue", "bob").await.json()["items"],
            json!([])
        );
        let fiche = get_de(&e, &format!("/films/items/{id}"), "bob")
            .await
            .json();
        assert_eq!(fiche["progress"], Value::Null);

        envoi("alice", 6479.0).await;
        assert_eq!(
            get_de(&e, "/films/continue", "alice").await.json()["items"][0]["progress"]["seen"],
            false
        );
        envoi("alice", 6480.0).await;
        assert_eq!(
            get_de(&e, "/films/continue", "alice").await.json()["items"],
            json!([])
        );
        let fiche = get_de(&e, &format!("/films/items/{id}"), "alice")
            .await
            .json();
        assert_eq!(fiche["progress"]["seen"], true);
        envoi("alice", 10.0).await;
        let fiche = get_de(&e, &format!("/films/items/{id}"), "alice")
            .await
            .json();
        assert_eq!(
            fiche["progress"]["seen"], true,
            "reste vu après un retour en arrière"
        );

        for corps in [
            json!({ "position": -1, "duration": 10 }),
            json!({ "position": 1, "duration": 0 }),
            json!({ "position": 1 }),
            json!({ "position": 90000, "duration": 10 }),
        ] {
            let r = appel(&e.app, "PUT", &chemin, "alice", Some(corps.clone()), None).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
        }
    }

    #[tokio::test]
    async fn rejets_journalises_une_fois_et_lisibles_par_admin() {
        let Some(e) = env_avec(&[("FILMS_CACHE_SECONDES", "1")]).await else {
            return;
        };
        get_de(&e, "/films/library", "alice").await;
        get_de(&e, "/films/library?sort=title", "alice").await;
        let nb: i64 = sqlx::query_scalar("SELECT count(*) FROM films_rejets")
            .fetch_one(&e.pool)
            .await
            .unwrap();
        assert_eq!(nb, 1);
        let r = get_de(&e, "/admin/films/rejets", "admin").await;
        assert_eq!(r.statut, StatusCode::OK);
        let v = r.json();
        assert_eq!(v["items"][0]["name"], "Piege.mkv.exe");
        assert_eq!(v["items"][0]["reason"], "double_extension");
        assert!(v["items"][0]["seen_at"].is_string());
        assert!(!r.texte().contains("/medias"), "nom seul, sans chemin");
        assert_eq!(
            get_de(&e, "/admin/films/rejets", "alice").await.statut,
            StatusCode::FORBIDDEN
        );

        let id = public_de(&e, JF_MAUVAIS).await;
        assert_eq!(
            ouvrir(&e, JF_MAUVAIS, "alice", true).await.statut,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get_de(&e, &format!("/films/items/{id}"), "alice")
                .await
                .statut,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn lecture_directe_avec_plage_sans_telechargement() {
        let Some(e) = env().await else { return };
        let o = ouvrir(&e, JF_FILM, "alice", true).await;
        assert_eq!(o.statut, StatusCode::OK);
        let v = o.json();
        assert_eq!(v["mode"], "direct");
        assert_eq!(v["audio"].as_array().unwrap().len(), 2);
        sans_fuite(&o.texte(), "playback");
        let url = v["url"].as_str().unwrap().to_string();

        let r = appel(&e.app, "GET", &url, "", None, None).await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.octets, b"video-complete");
        assert_eq!(r.entetes[header::CACHE_CONTROL], "no-store");
        assert!(r.entetes.get(header::CONTENT_DISPOSITION).is_none());

        let r = appel(&e.app, "GET", &url, "alice", None, Some("bytes=0-1")).await;
        assert_eq!(r.statut, StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.entetes[header::CONTENT_RANGE], "bytes 0-1/14");

        let vu = e.jellyfin.requetes().into_iter().last().unwrap();
        assert_eq!(vu.entete("range"), "bytes=0-1");
        let r = appel(&e.app, "GET", &url, "alice", None, Some("n'importe quoi")).await;
        assert_eq!(r.statut, StatusCode::OK, "plage invalide non relayée");

        assert_eq!(
            appel(&e.app, "GET", &url, "bob", None, None).await.statut,
            StatusCode::FORBIDDEN
        );
        let hls = url.replace("/direct", "/hls.m3u8");
        assert_eq!(
            get_de(&e, &hls, "alice").await.statut,
            StatusCode::UNAUTHORIZED
        );

        let flux_id = v["stream_id"].as_str().unwrap();
        let c = format!("/films/streams/{flux_id}");
        assert_eq!(
            appel(&e.app, "DELETE", &c, "bob", None, None).await.statut,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            appel(&e.app, "DELETE", &c, "alice", None, None)
                .await
                .statut,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            get_de(&e, &url, "alice").await.statut,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn jeton_forge_refuse() {
        let Some(e) = env().await else { return };
        let v = ouvrir(&e, JF_FILM, "alice", true).await.json();
        let url = v["url"].as_str().unwrap();
        let forge = url.replacen(".", ".9", 1);
        assert_eq!(
            get_de(&e, &forge, "").await.statut,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            get_de(&e, "/films/stream/rien/direct", "").await.statut,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn hls_reecrit_sans_fuite_et_relaye_les_segments() {
        let Some(e) = env().await else { return };
        let o = ouvrir(&e, JF_MKV, "alice", true).await;
        let v = o.json();
        assert_eq!(v["mode"], "hls", "hevc en mkv : pas de lecture directe");
        let url = v["url"].as_str().unwrap().to_string();
        let racine = url.trim_end_matches("/hls.m3u8").to_string();

        let r = get_de(&e, &url, "alice").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(
            r.entetes[header::CONTENT_TYPE],
            "application/vnd.apple.mpegurl"
        );
        let maitre = r.texte();
        sans_fuite(&maitre, "master");
        assert!(maitre.contains("\nu/0\n"), "{maitre}");

        let r = get_de(&e, &format!("{racine}/u/0"), "alice").await;
        let variante = r.texte();
        sans_fuite(&variante, "main");
        assert!(
            variante.contains("\n1\n"),
            "relatif à la même table : {variante}"
        );

        let r = get_de(&e, &format!("{racine}/u/1"), "alice").await;
        assert_eq!(r.octets, b"segment");
        assert_eq!(r.entetes[header::CONTENT_TYPE], "video/mp2t");
        assert_eq!(
            get_de(&e, &format!("{racine}/u/99"), "alice").await.statut,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get_de(&e, &format!("{racine}/direct"), "alice")
                .await
                .statut,
            StatusCode::UNAUTHORIZED
        );

        let requetes = e.jellyfin.requetes();
        let maitre_vu = requetes
            .iter()
            .find(|q| q.chemin.ends_with("master.m3u8"))
            .unwrap();
        assert!(
            maitre_vu.query.contains("AudioStreamIndex=1"),
            "{}",
            maitre_vu.query
        );
        assert!(
            maitre_vu.query.contains("VideoBitrate=120000000"),
            "sans plafond de débit, Jellyfin réduit la vidéo à 416 px : {}",
            maitre_vu.query
        );
        for interdit in [
            "MaxWidth",
            "MaxHeight",
            "EnableAdaptiveBitrateStreaming=true",
        ] {
            assert!(!maitre_vu.query.contains(interdit), "{}", maitre_vu.query);
        }
    }

    #[tokio::test]
    async fn piste_audio_autre_que_la_premiere_passe_en_hls() {
        let Some(e) = env().await else { return };
        let id = public_de(&e, JF_FILM).await;
        let chemin = format!("/films/items/{id}/playback");
        let r = appel(
            &e.app,
            "POST",
            &chemin,
            "alice",
            Some(json!({ "direct": true, "audio": 1 })),
            None,
        )
        .await;
        assert_eq!(r.json()["mode"], "hls");
        let url = r.json()["url"].as_str().unwrap().to_string();
        get_de(&e, &url, "alice").await;
        let vu = e
            .jellyfin
            .requetes()
            .into_iter()
            .find(|q| q.chemin.ends_with("master.m3u8"))
            .unwrap();
        assert!(vu.query.contains("AudioStreamIndex=2"), "{}", vu.query);
        let r = appel(
            &e.app,
            "POST",
            &chemin,
            "alice",
            Some(json!({ "audio": 9 })),
            None,
        )
        .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
        let r = appel(
            &e.app,
            "POST",
            &chemin,
            "bob",
            Some(json!({ "direct": false })),
            None,
        )
        .await;
        assert_eq!(r.json()["mode"], "hls", "le client refuse le direct");
    }

    #[tokio::test]
    async fn sous_titres_nettoyes() {
        let Some(e) = env().await else { return };
        let v = ouvrir(&e, JF_FILM, "alice", true).await.json();
        let sub = v["subtitles"][0]["url"].as_str().unwrap().to_string();
        let r = get_de(&e, &sub, "alice").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert!(
            r.entetes[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/vtt")
        );
        let t = r.texte();
        assert!(
            t.starts_with("WEBVTT") && t.contains("Bonjour <i>toi</i>") && !t.contains("alert"),
            "{t}"
        );
        let vu = e.jellyfin.requetes().into_iter().last().unwrap();
        assert!(vu.chemin.contains("/Subtitles/3/0/"), "{}", vu.chemin);
        let autre = sub.replace("/sub/0", "/sub/1");
        assert_eq!(
            get_de(&e, &autre, "alice").await.statut,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn limites_de_flux_et_de_transcodages() {
        let Some(e) = env().await else { return };
        assert_eq!(
            ouvrir(&e, JF_FILM, "alice", true).await.statut,
            StatusCode::OK
        );
        assert_eq!(
            ouvrir(&e, JF_E1, "alice", true).await.statut,
            StatusCode::OK
        );
        let r = ouvrir(&e, JF_E2, "alice", true).await;
        assert_eq!(r.statut, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(r.json()["error"], "too_many_streams");

        assert_eq!(
            ouvrir(&e, JF_MKV, "bob", false).await.statut,
            StatusCode::OK
        );
        assert_eq!(ouvrir(&e, JF_E1, "bob", false).await.statut, StatusCode::OK);
        sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ('sub-carole', 'carole')")
            .execute(&e.pool)
            .await
            .unwrap();
        let r = ouvrir(&e, JF_MKV, "carole", false).await;
        assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(r.json()["error"], "serveur_charge");
        assert_eq!(
            ouvrir(&e, JF_FILM, "carole", true).await.statut,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn resolution_pour_la_session_vocale() {
        let Some(e) = env().await else { return };
        let r = Resolveur {
            state: e.state.clone(),
            ctx: e.ctx.clone(),
        };
        let ep1 = public_de(&e, JF_E1).await;
        let (premier, suite) = r.resoudre("sub-alice", &ep1).await.unwrap();
        assert_eq!(premier.titre, "La Série · S1E1 · Ep1");
        assert_eq!(premier.duree, 7200);
        assert_eq!(suite.len(), 1);
        assert_eq!(suite[0].titre, "La Série · S1E2 · Ep2");
        assert_eq!(suite[0].id, public_de(&e, JF_E2).await);

        let (film_seul, suite) = r
            .resoudre("sub-alice", &public_de(&e, JF_FILM).await)
            .await
            .unwrap();
        assert_eq!(film_seul.titre, "Le Film");
        assert!(suite.is_empty());

        assert_eq!(
            r.resoudre("sub-sans", &ep1).await.unwrap_err(),
            RefusFilm::Permission
        );
        let serie = public_de(&e, JF_SERIE).await;
        assert_eq!(
            r.resoudre("sub-alice", &serie).await.unwrap_err(),
            RefusFilm::Introuvable
        );
        let mauvais = public_de(&e, JF_MAUVAIS).await;
        assert_eq!(
            r.resoudre("sub-alice", &mauvais).await.unwrap_err(),
            RefusFilm::Introuvable
        );
        assert_eq!(
            r.resoudre("sub-alice", "inconnu").await.unwrap_err(),
            RefusFilm::Introuvable
        );
    }
}
