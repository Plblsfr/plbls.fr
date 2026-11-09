#![allow(clippy::result_large_err)]

mod base;
mod meta;
mod seances;
mod source;

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, put},
};
use estompe_block_apercu_liens::Fetcher;
use estompe_block_musique::Musique;
use estompe_block_regarder_ensemble::Lecteurs;
use estompe_core::{
    api::ActiveUser,
    http::{champ, champ_texte, decode, write_err, write_json},
    messages::{Conv, conv_de_tout_salon, publier},
    openapi, roles,
    servers::{
        bd, booleen, entier, liste, nom_propre, non_trouve, pool, sans_contenu, uuid_valide,
    },
    state::{AppState, PostVideo},
};
use regex::Regex;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use base::Echec;
use meta::{Bases, Moteur};
pub use seances::Planificateur;
use source::{jouable, premier_lien, re_numero, titre_de_repli};

const VIDEOS_PAR_SEANCE: usize = 50;
const VIDEOS_PAR_LISTE: usize = 100;
const SEANCE_MAX_MINUTES: i64 = 24 * 60;
const DELAI_METADONNEES: Duration = Duration::from_secs(20);

type Params = Path<HashMap<String, String>>;

pub fn brancher(state: &AppState) {
    brancher_avec(
        state,
        Arc::new(moteur(
            Fetcher::new(false),
            Musique::new(),
            Bases::production(),
        )),
        Lecteurs::partage(),
    )
    .lancer();
}

pub fn moteur(fetcher: Fetcher, musique: Musique, bases: Bases) -> Moteur {
    Moteur {
        fetcher,
        musique,
        bases,
    }
}

pub fn brancher_avec(
    state: &AppState,
    moteur: Arc<Moteur>,
    lecteurs: Lecteurs,
) -> Arc<Planificateur> {
    state.definir_post_video(Arc::new(Crochet {
        state: state.clone(),
        moteur,
    }));
    Arc::new(Planificateur {
        state: state.clone(),
        lecteurs,
    })
}

struct Crochet {
    state: AppState,
    moteur: Arc<Moteur>,
}

impl PostVideo for Crochet {
    fn apres_message<'a>(
        &'a self,
        conv: &'a Conv,
        message: &'a Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.publication(conv, message))
    }
}

impl Crochet {
    async fn publication(&self, conv: &Conv, message: &Value) {
        let (Some(id), Some(contenu), Some(salon)) = (
            message["id"].as_str(),
            message["content"].as_str(),
            conv.channel_id.as_deref(),
        ) else {
            return;
        };
        let Some(src) = premier_lien(contenu) else {
            return;
        };
        if conv.kind == "music" && src.provider != "ytmusic" {
            return;
        }
        let Ok(pool) = self.state.pool() else {
            return;
        };
        if let Err(e) = base::creer_publication(
            pool,
            salon,
            id,
            &src.provider,
            &src.reference,
            &src.url,
            titre_de_repli(&src.provider),
        )
        .await
        {
            tracing::warn!("vidéos : carte du message {id} : {e}");
            return;
        }
        changement(&self.state, pool, conv).await;
        let (state, moteur, conv, id) = (
            self.state.clone(),
            self.moteur.clone(),
            conv.clone(),
            id.to_string(),
        );
        tokio::spawn(async move {
            let cle = state.config().youtube_key.clone();
            let Ok(m) = tokio::time::timeout(DELAI_METADONNEES, moteur.recuperer(&cle, &src)).await
            else {
                return;
            };
            if let Some(e) = &m.erreur {
                tracing::warn!("vidéos : métadonnées de {} : {e}", src.url);
            }
            if m.echec && m.miniature.is_empty() && m.titre == titre_de_repli(&m.provider) {
                return;
            }
            let Ok(pool) = state.pool() else {
                return;
            };
            if base::maj_metadonnees(
                pool,
                &id,
                &m.provider,
                &m.reference,
                &m.titre,
                &m.site,
                &m.miniature,
            )
            .await
            .is_ok()
            {
                changement(&state, pool, &conv).await;
            }
        });
    }
}

async fn changement(state: &AppState, pool: &PgPool, conv: &Conv) {
    let donnees = json!({ "server_id": conv.server_id, "channel_id": conv.channel_id });
    publier(state, pool, conv, "videos.changed", &donnees).await;
}

fn param<'a>(p: &'a HashMap<String, String>, nom: &str) -> &'a str {
    p.get(nom).map_or("", String::as_str)
}

async fn salon(
    state: &AppState,
    p: &HashMap<String, String>,
    utilisateur: &str,
) -> Result<Conv, Response> {
    if !estompe_core::servers::uuid_valide(param(p, "cid")) {
        return Err(estompe_core::servers::non_trouve("salon introuvable"));
    }
    let conv = conv_de_tout_salon(state, param(p, "cid"), utilisateur).await?;
    if conv.kind != "videos" && conv.kind != "music" {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_videos_channel",
            "ce salon n'est pas un salon vidéos ou musique",
        ));
    }
    Ok(conv)
}

fn salon_id(conv: &Conv) -> &str {
    conv.channel_id.as_deref().unwrap_or_default()
}

fn introuvable() -> Response {
    non_trouve("introuvable")
}

fn erreur_ecriture(e: Echec) -> Response {
    match e {
        Echec::Introuvable | Echec::PasUneVideo => write_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "introuvable dans ce salon",
        ),
        Echec::Plein => write_err(StatusCode::CONFLICT, "limit_reached", "limite atteinte"),
        Echec::Sql(e) => bd(e),
    }
}

async fn terminer(
    state: &AppState,
    pool: &PgPool,
    conv: &Conv,
    resultat: Result<(), Echec>,
    statut: StatusCode,
    corps: Option<Value>,
) -> Response {
    if let Err(e) = resultat {
        return erreur_ecriture(e);
    }
    changement(state, pool, conv).await;
    match corps {
        Some(c) => write_json(statut, &c),
        None => statut_vide(statut),
    }
}

fn statut_vide(statut: StatusCode) -> Response {
    if statut == StatusCode::NO_CONTENT {
        sans_contenu()
    } else {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = statut;
        r
    }
}

fn identifiant(p: &HashMap<String, String>, nom: &str) -> Result<String, Response> {
    let id = param(p, nom);
    if uuid_valide(id) {
        Ok(id.to_string())
    } else {
        Err(introuvable())
    }
}

fn invalide() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

fn forbidden() -> Response {
    write_err(
        StatusCode::FORBIDDEN,
        "forbidden",
        "permission manquante dans ce salon",
    )
}

fn nom_liste(entree: &Map<String, Value>) -> Result<(String, Option<bool>), Response> {
    let nom = champ_texte(entree, "name").ok_or_else(invalide)?;
    let collaborative = match champ(entree, "collaborative") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        _ => return Err(invalide()),
    };
    Ok((nom, collaborative))
}

fn nom_valide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_name",
        "nom : 1 à 60 caractères",
    )
}

fn numeros_distincts(ids: &[String]) -> bool {
    ids.iter()
        .enumerate()
        .all(|(i, id)| re_numero().is_match(id) && !ids[..i].contains(id))
}

fn liste_modifiable(conv: &Conv, l: &base::Liste, utilisateur: &str) -> bool {
    l.collaborative || l.created_by == utilisateur || conv.peut(roles::MANAGE_MESSAGES)
}

async fn contenu(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = salon_id(&conv);
    let (publications, listes, seances) = tokio::try_join!(
        base::publications(pool, id, &u.id),
        base::listes(pool, id),
        base::seances(pool, id),
    )
    .map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "posts": publications, "lists": listes, "sessions": seances }),
    ))
}

async fn miniature(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let mid = param(&p, "mid");
    if !re_numero().is_match(mid) {
        return Err(introuvable());
    }
    let miniature = base::miniature(pool, salon_id(&conv), mid)
        .await
        .map_err(erreur_lecture)?;
    let mut r = write_json(StatusCode::OK, &json!({ "thumb": miniature }));
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(r)
}

fn erreur_lecture(e: Echec) -> Response {
    match e {
        Echec::Sql(e) => bd(e),
        _ => introuvable(),
    }
}

async fn vue(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let entree = decode(corps).await?;
    let vue = booleen(&entree, "watched").ok_or_else(invalide)?;
    if !conv.peut(roles::ORGANIZE_WATCH) && !conv.peut(roles::MANAGE_MESSAGES) {
        return Err(forbidden());
    }
    let mid = param(&p, "mid");
    if !re_numero().is_match(mid) {
        return Err(introuvable());
    }
    let r = base::marquer_vue(pool, salon_id(&conv), mid, vue).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

async fn creer_liste(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let entree = decode(corps).await?;
    let (nom, collaborative) = nom_liste(&entree)?;
    if !conv.peut(roles::SEND_MESSAGES) {
        return Err(forbidden());
    }
    let nom = nom_propre(&nom, 60).ok_or_else(nom_valide)?;
    let r = base::creer_liste(
        pool,
        salon_id(&conv),
        &nom,
        &u.id,
        collaborative.unwrap_or(true),
    )
    .await;
    Ok(match r {
        Ok(l) => {
            terminer(
                &etat,
                pool,
                &conv,
                Ok(()),
                StatusCode::CREATED,
                Some(l.json()),
            )
            .await
        }
        Err(e) => erreur_ecriture(e),
    })
}

async fn modifier_liste(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = identifiant(&p, "lid")?;
    let entree = decode(corps).await?;
    let (nom, collaborative) = nom_liste(&entree)?;
    if !conv.peut(roles::SEND_MESSAGES) {
        return Err(forbidden());
    }
    let nom = nom_propre(&nom, 60).ok_or_else(nom_valide)?;
    let l = match base::liste(pool, salon_id(&conv), &id).await {
        Ok(l) => l,
        Err(e) => return Ok(erreur_ecriture(e)),
    };
    if !liste_modifiable(&conv, &l, &u.id)
        || collaborative.is_some() && l.created_by != u.id && !conv.peut(roles::MANAGE_MESSAGES)
    {
        return Err(forbidden());
    }
    let r = base::renommer_liste(pool, salon_id(&conv), &id, &nom, collaborative).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

async fn supprimer_liste(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = identifiant(&p, "lid")?;
    let l = match base::liste(pool, salon_id(&conv), &id).await {
        Ok(l) => l,
        Err(e) => return Ok(erreur_ecriture(e)),
    };
    if l.created_by != u.id && !conv.peut(roles::MANAGE_MESSAGES) {
        return Err(forbidden());
    }
    let r = base::supprimer_liste(pool, salon_id(&conv), &id).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

async fn elements_liste(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = identifiant(&p, "lid")?;
    let entree = decode(corps).await?;
    let ids = liste(&entree, "ids").ok_or_else(invalide)?;
    if !conv.peut(roles::SEND_MESSAGES) {
        return Err(forbidden());
    }
    if ids.len() > VIDEOS_PAR_LISTE || !numeros_distincts(&ids) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_items",
            "liste : 100 vidéos au plus, sans doublon",
        ));
    }
    let l = match base::liste(pool, salon_id(&conv), &id).await {
        Ok(l) => l,
        Err(e) => return Ok(erreur_ecriture(e)),
    };
    if !liste_modifiable(&conv, &l, &u.id) {
        return Err(forbidden());
    }
    let r = base::remplacer_elements(pool, salon_id(&conv), &id, &ids).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

fn seance_invalide(message: &str) -> Response {
    write_err(StatusCode::BAD_REQUEST, "invalid_session", message)
}

fn date_rfc3339() -> &'static Regex {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$")
            .expect("expression")
    })
}

async fn creer_seance(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let entree = decode(corps).await?;
    let titre = champ_texte(&entree, "title").ok_or_else(invalide)?;
    let debut = match champ(&entree, "starts_at") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if date_rfc3339().is_match(s) => Some(s.clone()),
        _ => return Err(invalide()),
    };
    let vocal = champ_texte(&entree, "voice_channel_id").ok_or_else(invalide)?;
    let rappel = entier(&entree, "remind_minutes").ok_or_else(invalide)?;
    let id_liste = champ_texte(&entree, "list_id").ok_or_else(invalide)?;
    let mut ids = liste(&entree, "message_ids").ok_or_else(invalide)?;
    if !conv.peut(roles::ORGANIZE_WATCH) {
        return Err(forbidden());
    }
    let message_titre = "séance : titre de 1 à 60 caractères, date dans l'année à venir, rappel de 0 à 1440 minutes";
    let titre = nom_propre(&titre, 60).ok_or_else(|| seance_invalide(message_titre))?;
    let debut = debut.ok_or_else(|| seance_invalide(message_titre))?;
    let (passe, trop_loin): (bool, bool) = sqlx::query_as(
        "SELECT $1::timestamptz < now() - interval '1 minute', $1::timestamptz > now() + interval '366 days'",
    )
    .bind(&debut)
    .fetch_one(pool)
    .await
    .map_err(|_| invalide())?;
    if passe || trop_loin || !(0..=SEANCE_MAX_MINUTES).contains(&rappel) {
        return Err(seance_invalide(message_titre));
    }
    if !id_liste.is_empty() {
        if !uuid_valide(&id_liste) {
            return Err(introuvable());
        }
        ids = match base::liste(pool, salon_id(&conv), &id_liste).await {
            Ok(l) => l.items,
            Err(e) => return Ok(erreur_ecriture(e)),
        };
    }
    let message_videos =
        "séance : 1 à 50 vidéos ou morceaux de ce salon, lisibles ensemble (pas de carte simple)";
    if ids.is_empty() || ids.len() > VIDEOS_PAR_SEANCE || !numeros_distincts(&ids) {
        return Err(seance_invalide(message_videos));
    }
    let publications = base::publications_par_id(pool, salon_id(&conv), &ids)
        .await
        .map_err(bd)?;
    if publications.len() != ids.len()
        || publications
            .iter()
            .any(|v| !jouable(&v.provider) || conv.kind == "music" && v.provider != "ytmusic")
    {
        return Err(seance_invalide(message_videos));
    }
    let destination = if uuid_valide(&vocal) {
        conv_de_tout_salon(&etat, &vocal, &u.id).await.ok()
    } else {
        None
    };
    if !destination.is_some_and(|d| {
        d.kind == "voice" && d.server_id == conv.server_id && d.peut(roles::CONNECT)
    }) {
        return Err(seance_invalide("salon vocal invalide"));
    }
    let nouvelle = base::NouvelleSeance {
        titre: &titre,
        debut: &debut,
        vocal: &vocal,
        rappel: rappel as i32,
        videos: &ids,
        createur: &u.id,
    };
    Ok(
        match base::creer_seance(pool, salon_id(&conv), &nouvelle).await {
            Ok(s) => {
                terminer(
                    &etat,
                    pool,
                    &conv,
                    Ok(()),
                    StatusCode::CREATED,
                    Some(s.json()),
                )
                .await
            }
            Err(e) => erreur_ecriture(e),
        },
    )
}

async fn annuler_seance(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = identifiant(&p, "sid")?;
    let s = match base::seance(pool, salon_id(&conv), &id).await {
        Ok(s) => s,
        Err(e) => return Ok(erreur_ecriture(e)),
    };
    if s.created_by != u.id && !conv.peut(roles::MANAGE_MESSAGES) {
        return Err(forbidden());
    }
    let r = base::supprimer_seance(pool, salon_id(&conv), &id).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

async fn repondre(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Result<Response, Response> {
    let pool = pool(&etat)?;
    let conv = salon(&etat, &p, &u.id).await?;
    let id = identifiant(&p, "sid")?;
    let entree = decode(corps).await?;
    let reponse = champ_texte(&entree, "answer").ok_or_else(invalide)?;
    if !["yes", "maybe", "no"].contains(&reponse.as_str()) {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_answer",
            "réponse : yes, maybe ou no",
        ));
    }
    let r = base::repondre(pool, salon_id(&conv), &id, &u.id, &reponse).await;
    Ok(terminer(&etat, pool, &conv, r, StatusCode::NO_CONTENT, None).await)
}

type Operation = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const OPERATIONS: [Operation; 10] = [
    (
        HttpMethod::Get,
        "/channels/{cid}/videos",
        "Contenu d'un salon « Vidéos » : cartes, listes, séances à venir",
        &[
            ("200", "ok"),
            ("400", "not_videos_channel"),
            ("404", "salon introuvable"),
        ],
    ),
    (
        HttpMethod::Get,
        "/channels/{cid}/videos/posts/{mid}/thumb",
        "Miniature d'une carte",
        &[("200", "ok"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Put,
        "/channels/{cid}/videos/posts/{mid}/watched",
        "Marquer une vidéo comme vue (ou non) par le groupe",
        &[("204", "ok"), ("403", "forbidden"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/videos/lists",
        "Créer une liste partagée (30 au plus par salon)",
        &[
            ("201", "ok"),
            ("400", "invalid_name"),
            ("403", "forbidden"),
            ("409", "limit_reached"),
        ],
    ),
    (
        HttpMethod::Put,
        "/channels/{cid}/videos/lists/{lid}",
        "Renommer une liste, ou changer son partage",
        &[
            ("204", "ok"),
            ("400", "invalid_name"),
            ("403", "forbidden"),
            ("404", "introuvable"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/channels/{cid}/videos/lists/{lid}",
        "Supprimer une liste (son auteur, ou `manage_messages`)",
        &[("204", "ok"), ("403", "forbidden"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Put,
        "/channels/{cid}/videos/lists/{lid}/items",
        "Remplacer le contenu ordonné d'une liste (ajouter, retirer, réordonner)",
        &[
            ("204", "ok"),
            ("400", "invalid_items"),
            ("403", "forbidden"),
            ("404", "introuvable"),
        ],
    ),
    (
        HttpMethod::Post,
        "/channels/{cid}/videos/sessions",
        "Planifier une séance « Regarder ensemble »",
        &[
            ("201", "ok"),
            ("400", "invalid_session"),
            ("403", "forbidden"),
            ("409", "limit_reached"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/channels/{cid}/videos/sessions/{sid}",
        "Annuler une séance (son organisateur, ou `manage_messages`)",
        &[("204", "ok"), ("403", "forbidden"), ("404", "introuvable")],
    ),
    (
        HttpMethod::Put,
        "/channels/{cid}/videos/sessions/{sid}/rsvp",
        "Répondre à une séance",
        &[
            ("204", "ok"),
            ("400", "invalid_answer"),
            ("404", "introuvable"),
        ],
    ),
];

pub fn routes() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/channels/{cid}/videos", get(contenu))
        .route("/channels/{cid}/videos/posts/{mid}/thumb", get(miniature))
        .route("/channels/{cid}/videos/posts/{mid}/watched", put(vue))
        .route(
            "/channels/{cid}/videos/lists",
            axum::routing::post(creer_liste),
        )
        .route(
            "/channels/{cid}/videos/lists/{lid}",
            put(modifier_liste).delete(supprimer_liste),
        )
        .route(
            "/channels/{cid}/videos/lists/{lid}/items",
            put(elements_liste),
        )
        .route(
            "/channels/{cid}/videos/sessions",
            axum::routing::post(creer_seance),
        )
        .route(
            "/channels/{cid}/videos/sessions/{sid}",
            axum::routing::delete(annuler_seance),
        )
        .route("/channels/{cid}/videos/sessions/{sid}/rsvp", put(repondre));
    for (methode, chemin, resume, reponses) in OPERATIONS {
        routeur = openapi::documenter(
            routeur,
            chemin,
            methode,
            openapi::operation("vidéos", resume, reponses, chemin),
        );
    }
    routeur
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use estompe_block_regarder_ensemble::Lecteurs;
    use estompe_core::{
        db::migrate_embarquees,
        testutil::{self, BasePostgres, Faux, faux, faux_avec_entetes, faux_permission},
    };
    use estompe_core::{
        messages::EvenementMessage,
        state::{Notifieur, ParticipantsVocal, Vocal},
    };
    use serde_json::{Value, json};
    use std::{future::Future, pin::Pin, sync::Mutex, time::Instant};
    use tower::ServiceExt;

    #[derive(Default)]
    struct Notes(Mutex<Vec<(Vec<String>, String)>>);

    impl Notifieur for Notes {
        fn publier<'a>(
            &'a self,
            e: &'a EvenementMessage,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.0.lock().unwrap().push((e.a.clone(), e.rappel.clone()));
            })
        }
    }

    struct Presents(Mutex<Vec<String>>);

    impl Vocal for Presents {
        fn sortir_du_serveur<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {})
        }

        fn participants<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = ParticipantsVocal> + Send + 'a>> {
            Box::pin(async move {
                Ok(self
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|id| json!({ "user_id": id }))
                    .collect())
            })
        }
    }

    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    struct Env {
        app: Router,
        state: AppState,
        planif: Arc<Planificateur>,
        pool: PgPool,
        oembed: Faux,
        _faux: (Faux, Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env() -> Option<Env> {
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
        let images = faux_avec_entetes(|_| {
            (
                200,
                vec![("content-type", "image/png".into())],
                PNG.to_vec(),
            )
        })
        .await;
        let url_images = images.url.clone();
        let oembed = faux(move |r| {
            if r.query.contains("absente") {
                return (404, vec![]);
            }
            (
                200,
                json!({ "title": "Ma vidéo", "author_name": "Ma chaîne", "thumbnail_url": format!("{url_images}/image.png") })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let images = faux_avec_entetes(|_| {
            (
                200,
                vec![("content-type", "image/png".into())],
                PNG.to_vec(),
            )
        })
        .await;
        let (pg, url) = testutil::postgres().await?;
        let pool = PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob", "carol"] {
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
                ("PERMISSION_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool.clone());
        let bases = Bases {
            oembed_youtube: format!("{}/youtube", oembed.url),
            oembed_vimeo: format!("{}/vimeo", oembed.url),
            oembed_dailymotion: format!("{}/dailymotion", oembed.url),
            miniatures: images.url.clone(),
        };
        let lecteurs = Lecteurs::nouveau(Arc::new(Musique::avec_bases(
            &oembed.url,
            &oembed.url,
            &images.url,
        )));
        let planif = brancher_avec(
            &state,
            Arc::new(super::moteur(
                Fetcher::new(true),
                Musique::avec_bases(&oembed.url, &oembed.url, &images.url),
                bases,
            )),
            lecteurs.clone(),
        );
        let app = Router::from(
            estompe_core::api::router()
                .merge(routes())
                .merge(lecteurs.routes()),
        )
        .with_state(state.clone());
        Some(Env {
            app,
            state,
            planif,
            pool,
            oembed,
            _faux: (gateway, permission, images),
            _pg: pg,
        })
    }

    async fn appel(
        e: &Env,
        qui: &str,
        methode: &str,
        chemin: &str,
        corps: Option<Value>,
    ) -> (StatusCode, Value) {
        let r = Request::builder()
            .method(methode)
            .uri(chemin)
            .header(header::AUTHORIZATION, format!("Bearer {qui}"))
            .body(corps.map_or_else(Body::empty, |c| Body::from(c.to_string())))
            .unwrap();
        let rep = e.app.clone().oneshot(r).await.unwrap();
        let statut = rep.status();
        let octets = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        let v = if octets.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&octets).unwrap_or(Value::Null)
        };
        (statut, v)
    }

    struct Decor {
        videos: String,
        musique: String,
        texte: String,
        vocal: String,
    }

    async fn decor(e: &Env) -> Decor {
        let (_, sv) = appel(e, "alice", "POST", "/servers", Some(json!({ "name": "S" }))).await;
        let serveur = sv["id"].as_str().unwrap().to_string();
        let mut ids = Vec::new();
        for (nom, kind) in [
            ("v", "videos"),
            ("m", "music"),
            ("t", "text"),
            ("o", "voice"),
        ] {
            let (s, c) = appel(
                e,
                "alice",
                "POST",
                &format!("/servers/{serveur}/channels"),
                Some(json!({ "name": nom, "kind": kind })),
            )
            .await;
            assert_eq!(s, StatusCode::CREATED, "{c}");
            ids.push(c["id"].as_str().unwrap().to_string());
        }
        let (_, inv) = appel(
            e,
            "alice",
            "POST",
            &format!("/servers/{serveur}/invites"),
            None,
        )
        .await;
        let code = inv["code"].as_str().unwrap().to_string();
        for nom in ["bob", "carol"] {
            let (s, _) = appel(
                e,
                nom,
                "POST",
                &format!("/server-invites/{code}/join"),
                None,
            )
            .await;
            assert!(s.is_success());
        }
        Decor {
            videos: ids[0].clone(),
            musique: ids[1].clone(),
            texte: ids[2].clone(),
            vocal: ids[3].clone(),
        }
    }

    async fn publier_lien(e: &Env, qui: &str, salon: &str, lien: &str) -> String {
        let (s, m) = appel(
            e,
            qui,
            "POST",
            &format!("/channels/{salon}/messages"),
            Some(json!({ "content": format!("regardez {lien}") })),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{m}");
        m["id"].as_str().unwrap().to_string()
    }

    async fn contenu(e: &Env, qui: &str, salon: &str) -> (StatusCode, Value) {
        appel(e, qui, "GET", &format!("/channels/{salon}/videos"), None).await
    }

    async fn attendre_titre(e: &Env, salon: &str, titre: &str) -> Value {
        let debut = Instant::now();
        loop {
            let (_, c) = contenu(e, "alice", salon).await;
            if c["posts"][0]["title"] == titre {
                return c;
            }
            assert!(
                debut.elapsed().as_secs() < 20,
                "titre {titre} jamais atteint : {c}"
            );
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn un_lien_devient_une_carte_enrichie_en_arriere_plan() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let mid = publier_lien(&e, "alice", &d.videos, "https://youtu.be/dQw4w9WgXcQ.").await;
        let c = attendre_titre(&e, &d.videos, "Ma vidéo").await;
        let p = &c["posts"][0];
        assert_eq!(p["message_id"], mid);
        assert_eq!(p["provider"], "youtube");
        assert_eq!(p["ref"], "dQw4w9WgXcQ");
        assert_eq!(p["site"], "Ma chaîne");
        assert_eq!(p["has_thumb"], true);
        assert_eq!(p["author"]["pseudo"], "alice");
        assert_eq!(p["reactions"], 0);
        assert_eq!(p["watched_at"], Value::Null);
        assert_eq!(c["lists"], json!([]));
        assert_eq!(c["sessions"], json!([]));
        let (s, t) = appel(
            &e,
            "bob",
            "GET",
            &format!("/channels/{}/videos/posts/{mid}/thumb", d.videos),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert!(
            t["thumb"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        let (s, _) = appel(
            &e,
            "bob",
            "GET",
            &format!("/channels/{}/videos/posts/abc/thumb", d.videos),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert!(e.oembed.appels() >= 1);
    }

    #[tokio::test]
    async fn sans_metadonnees_la_carte_garde_son_titre_de_repli() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        publier_lien(&e, "alice", &d.videos, "https://dai.ly/xabsente").await;
        let debut = Instant::now();
        while e.oembed.appels() == 0 {
            assert!(debut.elapsed().as_secs() < 20);
            tokio::task::yield_now().await;
        }
        let (_, c) = contenu(&e, "alice", &d.videos).await;
        assert_eq!(c["posts"][0]["provider"], "dailymotion");
        assert_eq!(c["posts"][0]["title"], "Vidéo Dailymotion");
        publier_lien(&e, "alice", &d.videos, "texte sans lien utile").await;
        let (_, c) = contenu(&e, "alice", &d.videos).await;
        assert_eq!(c["posts"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn le_salon_musique_ne_garde_que_youtube_music() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        publier_lien(&e, "alice", &d.musique, "https://youtu.be/dQw4w9WgXcQ").await;
        publier_lien(
            &e,
            "alice",
            &d.musique,
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
        )
        .await;
        let (s, c) = contenu(&e, "bob", &d.musique).await;
        assert_eq!(s, StatusCode::OK);
        let posts = c["posts"].as_array().unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0]["provider"], "ytmusic");
    }

    #[tokio::test]
    async fn salons_et_acces() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let (s, c) = contenu(&e, "alice", &d.texte).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("not_videos_channel"))
        );
        let (s, _) = contenu(&e, "alice", &d.vocal).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, _) = contenu(&e, "alice", "00000000-0000-4000-8000-000000000000").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = contenu(&e, "alice", "pas-un-uuid").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn vu_par_le_groupe() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let mid = publier_lien(&e, "alice", &d.videos, &format!("{}/page", e.oembed.url)).await;
        let chemin = format!("/channels/{}/videos/posts/{mid}/watched", d.videos);
        let (s, _) = appel(&e, "bob", "PUT", &chemin, Some(json!({ "watched": true }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, c) = contenu(&e, "alice", &d.videos).await;
        assert!(c["posts"][0]["watched_at"].is_string());
        let (s, _) = appel(&e, "bob", "PUT", &chemin, Some(json!({ "watched": false }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, c) = contenu(&e, "alice", &d.videos).await;
        assert_eq!(c["posts"][0]["watched_at"], Value::Null);
        let (s, _) = appel(&e, "bob", "PUT", &chemin, Some(json!({ "watched": "oui" }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let inconnu = format!("/channels/{}/videos/posts/999999/watched", d.videos);
        let (s, _) = appel(&e, "bob", "PUT", &inconnu, Some(json!({ "watched": true }))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = appel(
            &e,
            "bob",
            "PUT",
            &chemin.replace(&mid, "x"),
            Some(json!({ "watched": true })),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn listes_creation_droits_et_ordre() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let a = publier_lien(&e, "alice", &d.videos, &format!("{}/a", e.oembed.url)).await;
        let b = publier_lien(&e, "alice", &d.videos, &format!("{}/b", e.oembed.url)).await;
        let base = format!("/channels/{}/videos/lists", d.videos);
        let (s, l) = appel(
            &e,
            "alice",
            "POST",
            &base,
            Some(json!({ "name": "  Soirée  ", "collaborative": false })),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(l["name"], "Soirée");
        assert_eq!(l["collaborative"], false);
        assert_eq!(l["items"], json!([]));
        let lid = l["id"].as_str().unwrap().to_string();
        let (s, c) = appel(&e, "alice", "POST", &base, Some(json!({ "name": "" }))).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_name"))
        );
        let (s, _) = appel(&e, "alice", "POST", &base, Some(json!({ "name": 3 }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let items = format!("{base}/{lid}/items");
        let (s, c) = appel(&e, "bob", "PUT", &items, Some(json!({ "ids": [a] }))).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden"))
        );
        let (s, _) = appel(&e, "alice", "PUT", &items, Some(json!({ "ids": [b, a] }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, c) = contenu(&e, "bob", &d.videos).await;
        assert_eq!(c["lists"][0]["items"], json!([b, a]));
        let (s, c) = appel(&e, "alice", "PUT", &items, Some(json!({ "ids": [a, a] }))).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_items"))
        );
        let (s, _) = appel(
            &e,
            "alice",
            "PUT",
            &items,
            Some(json!({ "ids": ["999999"] })),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = appel(
            &e,
            "alice",
            "PUT",
            &format!("{base}/{lid}"),
            Some(json!({ "name": "Autre", "collaborative": true })),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = appel(&e, "bob", "PUT", &items, Some(json!({ "ids": [a] }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = appel(
            &e,
            "bob",
            "PUT",
            &format!("{base}/{lid}"),
            Some(json!({ "name": "Bob", "collaborative": false })),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = appel(&e, "bob", "DELETE", &format!("{base}/{lid}"), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/{lid}"), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/{lid}"), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/pas-un-uuid"), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trente_listes_au_plus() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let base = format!("/channels/{}/videos/lists", d.videos);
        for i in 0..30 {
            let (s, _) = appel(
                &e,
                "alice",
                "POST",
                &base,
                Some(json!({ "name": format!("l{i}") })),
            )
            .await;
            assert_eq!(s, StatusCode::CREATED);
        }
        let (s, c) = appel(
            &e,
            "alice",
            "POST",
            &base,
            Some(json!({ "name": "de trop" })),
        )
        .await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::CONFLICT, Some("limit_reached"))
        );
    }

    #[tokio::test]
    async fn seances_planification_reponses_et_annulation() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let a = publier_lien(&e, "alice", &d.videos, "https://youtu.be/dQw4w9WgXcQ").await;
        let simple = publier_lien(&e, "alice", &d.videos, &format!("{}/page", e.oembed.url)).await;
        let base = format!("/channels/{}/videos/sessions", d.videos);
        let (futur,): (String,) = sqlx::query_as("SELECT to_char((now() + interval '2 days') AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')")
            .fetch_one(&e.pool)
            .await
            .unwrap();
        let corps = |titre: &str, debut: &str, ids: Vec<&str>, vocal: &str| json!({ "title": titre, "starts_at": debut, "voice_channel_id": vocal, "remind_minutes": 15, "message_ids": ids });
        for mauvais in [
            corps("", &futur, vec![a.as_str()], &d.vocal),
            corps("Soirée", "2020-01-01T00:00:00Z", vec![a.as_str()], &d.vocal),
            corps("Soirée", "2999-01-01T00:00:00Z", vec![a.as_str()], &d.vocal),
            corps("Soirée", &futur, vec![], &d.vocal),
            corps("Soirée", &futur, vec![simple.as_str()], &d.vocal),
            corps("Soirée", &futur, vec!["999999"], &d.vocal),
            corps("Soirée", &futur, vec![a.as_str()], &d.videos),
            corps("Soirée", &futur, vec![a.as_str()], "pas-un-uuid"),
        ] {
            let (s, c) = appel(&e, "alice", "POST", &base, Some(mauvais.clone())).await;
            assert_eq!(
                (s, c["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_session")),
                "{mauvais}"
            );
        }
        let (s, _) = appel(
            &e,
            "alice",
            "POST",
            &base,
            Some(json!({ "title": "x", "starts_at": "demain" })),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let mut rappel = corps("Soirée", &futur, vec![a.as_str()], &d.vocal);
        rappel["remind_minutes"] = json!(1441);
        let (s, c) = appel(&e, "alice", "POST", &base, Some(rappel)).await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_session"))
        );
        let (s, sc) = appel(
            &e,
            "alice",
            "POST",
            &base,
            Some(corps(" Soirée ", &futur, vec![a.as_str()], &d.vocal)),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{sc}");
        assert_eq!(sc["title"], "Soirée");
        assert_eq!(sc["videos"], json!([a]));
        assert_eq!(sc["started_at"], Value::Null);
        assert_eq!(
            sc["rsvps"]
                .as_object()
                .unwrap()
                .values()
                .collect::<Vec<_>>(),
            vec!["yes"]
        );
        let sid = sc["id"].as_str().unwrap().to_string();
        let rsvp = format!("{base}/{sid}/rsvp");
        let (s, c) = appel(
            &e,
            "bob",
            "PUT",
            &rsvp,
            Some(json!({ "answer": "peut-etre" })),
        )
        .await;
        assert_eq!(
            (s, c["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_answer"))
        );
        let (s, _) = appel(&e, "bob", "PUT", &rsvp, Some(json!({ "answer": "maybe" }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = appel(&e, "bob", "PUT", &rsvp, Some(json!({ "answer": "no" }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, c) = contenu(&e, "carol", &d.videos).await;
        assert_eq!(c["sessions"][0]["rsvps"].as_object().unwrap().len(), 2);
        let (s, _) = appel(&e, "bob", "DELETE", &format!("{base}/{sid}"), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/{sid}"), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = appel(&e, "alice", "PUT", &rsvp, Some(json!({ "answer": "yes" }))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (_, c) = contenu(&e, "alice", &d.videos).await;
        assert_eq!(c["sessions"], json!([]));
    }

    #[tokio::test]
    async fn une_seance_depuis_une_liste() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let a = publier_lien(&e, "alice", &d.videos, "https://youtu.be/dQw4w9WgXcQ").await;
        let (_, l) = appel(
            &e,
            "alice",
            "POST",
            &format!("/channels/{}/videos/lists", d.videos),
            Some(json!({ "name": "L" })),
        )
        .await;
        let lid = l["id"].as_str().unwrap();
        appel(
            &e,
            "alice",
            "PUT",
            &format!("/channels/{}/videos/lists/{lid}/items", d.videos),
            Some(json!({ "ids": [a] })),
        )
        .await;
        let corps = json!({ "title": "Séance", "starts_at": "2999-01-01T00:00:00Z", "voice_channel_id": d.vocal, "list_id": lid });
        let (s, _) = appel(
            &e,
            "alice",
            "POST",
            &format!("/channels/{}/videos/sessions", d.videos),
            Some(corps.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let mut ok = corps;
        ok["starts_at"] = json!((dans_un_jour(&e).await));
        let (s, sc) = appel(
            &e,
            "alice",
            "POST",
            &format!("/channels/{}/videos/sessions", d.videos),
            Some(ok.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{sc}");
        assert_eq!(sc["videos"], json!([a]));
        ok["list_id"] = json!("00000000-0000-4000-8000-000000000000");
        let (s, _) = appel(
            &e,
            "alice",
            "POST",
            &format!("/channels/{}/videos/sessions", d.videos),
            Some(ok),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn le_planificateur_rappelle_puis_lance_la_seance() {
        let Some(e) = env().await else { return };
        let d = decor(&e).await;
        let a = publier_lien(&e, "alice", &d.videos, "https://youtu.be/dQw4w9WgXcQ").await;
        let (s, sc) = appel(
            &e,
            "alice",
            "POST",
            &format!("/channels/{}/videos/sessions", d.videos),
            Some(json!({ "title": "Soirée", "starts_at": dans_un_jour(&e).await, "voice_channel_id": d.vocal, "remind_minutes": 15, "message_ids": [a] })),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{sc}");
        let sid = sc["id"].as_str().unwrap().to_string();
        let notes = Arc::new(Notes::default());
        e.state.definir_notifieur(notes.clone());
        let presents = Arc::new(Presents(Mutex::default()));
        let (_, moi) = appel(&e, "alice", "GET", "/me", None).await;
        let alice = moi["id"].as_str().unwrap().to_string();

        e.planif.passage().await;
        assert!(notes.0.lock().unwrap().is_empty());
        sqlx::query("UPDATE watch_sessions SET starts_at = now() + interval '10 minutes'")
            .execute(&e.pool)
            .await
            .unwrap();
        e.planif.passage().await;
        e.planif.passage().await;
        assert_eq!(
            *notes.0.lock().unwrap(),
            [(
                vec![alice.clone()],
                "Séance « Soirée » dans 10 min".to_string()
            )]
        );

        sqlx::query("UPDATE watch_sessions SET starts_at = now() - interval '1 minute'")
            .execute(&e.pool)
            .await
            .unwrap();
        let lire = || async {
            appel(
                &e,
                "alice",
                "GET",
                &format!("/voice/{}/watch", d.vocal),
                None,
            )
            .await
            .1
        };
        e.planif.passage().await;
        e.state.definir_vocal(presents.clone());
        e.planif.passage().await;
        presents
            .0
            .lock()
            .unwrap()
            .push("00000000-0000-0000-0000-000000000001".into());
        e.planif.passage().await;
        assert!(lire().await["state"].is_null());
        let demarree = |e: &Env| {
            let pool = e.pool.clone();
            let sid = sid.clone();
            async move {
                sqlx::query_scalar::<_, bool>(
                    "SELECT started_at IS NOT NULL FROM watch_sessions WHERE id=$1::uuid",
                )
                .bind(sid)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        assert!(!demarree(&e).await);

        presents.0.lock().unwrap().push(alice.clone());
        e.planif.passage().await;
        assert!(demarree(&e).await);
        let etat = lire().await["state"].clone();
        assert_eq!(etat["video"], "dQw4w9WgXcQ");
        assert_eq!(etat["provider"], "youtube");
        assert_eq!(etat["playing"], true);
        assert_eq!(etat["by"], "Séance « Soirée »");
        assert_eq!(etat["host"], alice);
        let rev = etat["rev"].clone();
        e.planif.passage().await;
        assert_eq!(lire().await["state"]["rev"], rev);
    }

    async fn dans_un_jour(e: &Env) -> String {
        sqlx::query_scalar("SELECT to_char((now() + interval '1 day') AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')")
            .fetch_one(&e.pool)
            .await
            .unwrap()
    }
}
