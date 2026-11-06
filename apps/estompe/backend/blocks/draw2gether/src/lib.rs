#![allow(clippy::result_large_err)]

mod mots;
mod partie;

use std::{
    sync::{Arc, Once},
    time::{Duration, SystemTime},
};

use axum::{
    Extension,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use estompe_core::{
    api::ActiveUser,
    http::{champ_texte, decode, internal, write_err, write_json},
    messages::{self, Conv},
    openapi,
    ratelimit::RateLimiter,
    roles,
    state::AppState,
    users::User,
};
use partie::{Diffusion, Etat, Op, Parties, Phase, Refus, ReglagesMaj, Utilisateur, ms};
use serde_json::{Value, json};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const BOT: &str = "00000000-0000-0000-0000-000000000001";
const PROPOSITIONS_MAX: usize = 5;
const FENETRE_PROPOSITIONS: Duration = Duration::from_secs(5);
const TRACES_LOTS_MAX: usize = 120;
const ACTIONS_MAX: usize = 30;
const FENETRE: Duration = Duration::from_secs(10);
const TIC: Duration = Duration::from_secs(1);

struct Contexte {
    parties: Parties,
    propositions: RateLimiter,
    traces: RateLimiter,
    actions: RateLimiter,
    accroche: Once,
}

impl Contexte {
    fn nouveau() -> Self {
        Contexte {
            parties: Parties::default(),
            propositions: RateLimiter::default(),
            traces: RateLimiter::default(),
            actions: RateLimiter::default(),
            accroche: Once::new(),
        }
    }

    fn accrocher(self: &Arc<Self>, state: &AppState) {
        self.accroche.call_once(|| {
            let ctx = self.clone();
            state.sur_sortie_salon_vocal(move |etat, serveur, salon, membre| {
                let ctx = ctx.clone();
                async move { ctx.depart(&etat, &serveur, &salon, &membre).await }
            });
            let ctx = self.clone();
            let state = state.clone();
            tokio::spawn(async move {
                let mut cadence = tokio::time::interval(TIC);
                loop {
                    cadence.tick().await;
                    for (salon, etat) in ctx.parties.tic(ms(SystemTime::now())) {
                        let cible = Salon {
                            serveur: etat.server_id.clone(),
                            id: salon,
                        };
                        publier(&state, &cible, Some(&etat)).await;
                    }
                }
            });
        });
    }

    async fn depart(&self, state: &AppState, serveur: &str, salon: &str, membre: &str) {
        if let Some(suivant) = self.parties.quitter(salon, membre, ms(SystemTime::now())) {
            let cible = Salon {
                serveur: serveur.to_string(),
                id: salon.to_string(),
            };
            publier(state, &cible, suivant.as_ref()).await;
        }
    }
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

fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

fn erreur(r: Refus) -> Response {
    let code = if r.statut == StatusCode::NOT_FOUND {
        "no_draw"
    } else {
        "draw_refused"
    };
    write_err(r.statut, code, &r.message)
}

fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("draw2gether : base : {e}");
    internal()
}

fn utilisateur(u: &User) -> Utilisateur<'_> {
    Utilisateur {
        id: &u.id,
        pseudo: &u.pseudo,
    }
}

async fn salon_vocal(state: &AppState, cid: &str, user: &User) -> Result<Salon, Response> {
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
    let acces = roles::access_for(pool, &serveur, &user.id)
        .await
        .map_err(bd)?
        .ok_or_else(|| introuvable("introuvable"))?;
    if !roles::contient(acces.channel(cid), roles::VIEW_CHANNEL) {
        return Err(introuvable("introuvable"));
    }
    if kind != "voice" {
        return Err(write_err(
            StatusCode::BAD_REQUEST,
            "not_voice_channel",
            "ce salon n'est pas un salon vocal",
        ));
    }
    Ok(Salon {
        serveur,
        id: cid.to_string(),
    })
}

async fn salon_ecriture(
    state: &AppState,
    cid: &str,
    user: &User,
    limiteur: &RateLimiter,
    max: usize,
    fenetre: Duration,
) -> Result<Salon, Response> {
    let salon = salon_vocal(state, cid, user).await?;
    if !limiteur.allow(&user.id, max, fenetre) {
        let mut r = write_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "trop d'actions, patientez un instant",
        );
        r.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&fenetre.as_secs().to_string()).expect("valeur"),
        );
        return Err(r);
    }
    let presents = match state.participants_vocal(&salon.id).await {
        Some(Ok(p)) => p,
        Some(Err(_)) => {
            return Err(write_err(
                StatusCode::BAD_GATEWAY,
                "voice_unavailable",
                "serveur vocal injoignable",
            ));
        }
        None => Vec::new(),
    };
    if presents
        .iter()
        .any(|p| p["user_id"].as_str() == Some(user.id.as_str()) && p["user_id"] != BOT)
    {
        Ok(salon)
    } else {
        Err(write_err(
            StatusCode::FORBIDDEN,
            "not_in_voice",
            "rejoignez le salon vocal pour jouer",
        ))
    }
}

fn conv(salon: &Salon) -> Conv {
    Conv {
        channel_id: Some(salon.id.clone()),
        server_id: Some(salon.serveur.clone()),
        dm_id: None,
        kind: "voice".into(),
        slowmode: 0,
        user_limit: 0,
        perms: 0,
    }
}

async fn diffuser(state: &AppState, salon: &Salon, typ: &str, mut v: Value) {
    v["server_id"] = salon.serveur.clone().into();
    v["channel_id"] = salon.id.clone().into();
    if let Ok(pool) = state.pool() {
        messages::publier(state, pool, &conv(salon), typ, &v).await;
    }
}

fn vue(etat: Option<&Etat>) -> Value {
    json!({ "state": etat, "now": ms(SystemTime::now()) })
}

async fn publier(state: &AppState, salon: &Salon, etat: Option<&Etat>) {
    diffuser(state, salon, "draw.state", vue(etat)).await;
    if let Some(e) = etat
        && matches!(e.phase, Phase::Choosing | Phase::Drawing)
    {
        let mut v = e.prive(&e.drawer);
        v["rev"] = e.rev.into();
        v["server_id"] = salon.serveur.clone().into();
        v["channel_id"] = salon.id.clone().into();
        state.publier(std::slice::from_ref(&e.drawer), "draw.private", &v);
    }
}

async fn publier_chat(state: &AppState, salon: &Salon, d: Diffusion) {
    let mut v = json!({
        "kind": d.kind,
        "from": d.from,
        "pseudo": d.pseudo,
        "text": d.text,
        "at": ms(SystemTime::now()),
    });
    match d.to {
        None => diffuser(state, salon, "draw.chat", v).await,
        Some(ids) => {
            v["server_id"] = salon.serveur.clone().into();
            v["channel_id"] = salon.id.clone().into();
            state.publier(&ids, "draw.chat", &v);
        }
    }
}

fn corps_json<T: serde::de::DeserializeOwned>(
    corps: serde_json::Map<String, Value>,
) -> Result<T, Response> {
    serde_json::from_value(Value::Object(corps)).map_err(|_| mauvais())
}

async fn lire(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_vocal(&state, &cid, &user).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let etat = ctx.parties.etat(&salon.id);
    let mut v = vue(etat.as_ref());
    if let Some(e) = &etat {
        v["me"] = e.prive(&user.id);
        v["drawing"] = e.dessin();
    }
    write_json(StatusCode::OK, &v)
}

async fn rejoindre(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match ctx
        .parties
        .creer_ou_rejoindre(&salon.id, &salon.serveur, &utilisateur(&user))
    {
        Ok(e) => {
            publier(&state, &salon, Some(&e)).await;
            sans_contenu()
        }
        Err(r) => erreur(r),
    }
}

async fn arreter(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match ctx.parties.arreter(&salon.id, &user.id) {
        Ok(()) => {
            publier(&state, &salon, None).await;
            sans_contenu()
        }
        Err(r) => erreur(r),
    }
}

async fn appliquer<R>(
    state: &AppState,
    ctx: &Contexte,
    salon: &Salon,
    f: impl FnOnce(&mut Etat, i64) -> Result<R, Refus>,
) -> Result<R, Response> {
    match ctx.parties.agir(&salon.id, f) {
        Ok((etat, r)) => {
            publier(state, salon, Some(&etat)).await;
            Ok(r)
        }
        Err(r) => Err(erreur(r)),
    }
}

async fn reglages(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let m: ReglagesMaj = match decode(corps).await.and_then(corps_json) {
        Ok(m) => m,
        Err(r) => return r,
    };
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match appliquer(&state, &ctx, &salon, |e, _| e.regler(&user.id, m)).await {
        Ok(()) => sans_contenu(),
        Err(r) => r,
    }
}

async fn lancer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
) -> Response {
    ctx.accrocher(&state);
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match appliquer(&state, &ctx, &salon, |e, now| e.lancer(&user.id, now)).await {
        Ok(()) => sans_contenu(),
        Err(r) => r,
    }
}

async fn choisir(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(index) = corps.get("index").and_then(Value::as_u64) else {
        return mauvais();
    };
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match appliquer(&state, &ctx, &salon, |e, now| {
        e.choisir(&user.id, index as usize, now)
    })
    .await
    {
        Ok(()) => sans_contenu(),
        Err(r) => r,
    }
}

async fn proposer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(texte) = champ_texte(&corps, "text") else {
        return mauvais();
    };
    let salon = match salon_ecriture(
        &state,
        &cid,
        &user,
        &ctx.propositions,
        PROPOSITIONS_MAX,
        FENETRE_PROPOSITIONS,
    )
    .await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    let r = ctx.parties.agir(&salon.id, |e, now| {
        let (d, change) = e.proposer(&user.id, &texte, now)?;
        if !change {
            e.rev -= 1;
        }
        Ok((d, change))
    });
    match r {
        Ok((etat, (diffusions, change))) => {
            for d in diffusions {
                publier_chat(&state, &salon, d).await;
            }
            if change {
                publier(&state, &salon, Some(&etat)).await;
            }
            sans_contenu()
        }
        Err(r) => erreur(r),
    }
}

async fn dessiner(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let op: Op = match decode(corps).await.and_then(corps_json) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let salon = match salon_ecriture(
        &state,
        &cid,
        &user,
        &ctx.traces,
        TRACES_LOTS_MAX,
        FENETRE_PROPOSITIONS,
    )
    .await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match ctx.parties.dessiner(&salon.id, &user.id, op) {
        Ok((seq, op)) => {
            diffuser(
                &state,
                &salon,
                "draw.ops",
                json!({ "seq": seq, "ops": [op] }),
            )
            .await;
            sans_contenu()
        }
        Err(r) => erreur(r),
    }
}

async fn voter(
    State(state): State<AppState>,
    Extension(ctx): Extension<Arc<Contexte>>,
    ActiveUser(user): ActiveUser,
    Path(cid): Path<String>,
    corps: Body,
) -> Response {
    ctx.accrocher(&state);
    let corps = match decode(corps).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(genre) = champ_texte(&corps, "kind") else {
        return mauvais();
    };
    let cible = champ_texte(&corps, "target");
    let salon = match salon_ecriture(&state, &cid, &user, &ctx.actions, ACTIONS_MAX, FENETRE).await
    {
        Ok(s) => s,
        Err(r) => return r,
    };
    match appliquer(&state, &ctx, &salon, |e, now| {
        e.voter(&user.id, &genre, cible.as_deref(), now)
    })
    .await
    {
        Ok(()) => sans_contenu(),
        Err(r) => r,
    }
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOCS: [Doc; 9] = [
    (
        HttpMethod::Get,
        "/voice/{cid}/draw",
        "Partie Draw2Gether du salon",
        &[("200", "ok"), ("404", "salon introuvable")],
    ),
    (
        HttpMethod::Put,
        "/voice/{cid}/draw",
        "Créer la partie ou la rejoindre",
        &[
            ("204", "ok"),
            ("403", "`not_in_voice` ou exclu"),
            ("409", "partie pleine"),
            ("429", "rate_limited"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/voice/{cid}/draw",
        "Arrêter la partie",
        &[("204", "ok"), ("403", "forbidden"), ("404", "no_draw")],
    ),
    (
        HttpMethod::Put,
        "/voice/{cid}/draw/settings",
        "Régler la partie (salle d'attente)",
        &[
            ("204", "ok"),
            ("400", "réglage invalide"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("409", "la partie a commencé"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/draw/start",
        "Lancer la partie",
        &[
            ("204", "ok"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("409", "moins de 2 joueurs ou partie commencée"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/draw/choose",
        "Choisir le mot à dessiner",
        &[
            ("204", "ok"),
            ("400", "choix invalide"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("409", "pas de choix en cours"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/draw/guess",
        "Proposer un mot ou écrire dans le chat de la partie",
        &[
            ("204", "ok"),
            ("400", "message invalide"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("429", "rate_limited"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/draw/draw",
        "Transmettre un lot de points, annuler ou effacer",
        &[
            ("204", "ok"),
            ("400", "trait invalide"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("409", "dessin trop chargé"),
            ("429", "rate_limited"),
        ],
    ),
    (
        HttpMethod::Post,
        "/voice/{cid}/draw/vote",
        "Signaler le dessin, voter pour passer le tour ou exclure un joueur",
        &[
            ("204", "ok"),
            ("400", "vote inconnu ou cible invalide"),
            ("403", "forbidden"),
            ("404", "no_draw"),
            ("409", "aucun tour à passer"),
        ],
    ),
];

fn routes_partie() -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::new()
        .route(
            "/voice/{cid}/draw",
            get(lire).put(rejoindre).delete(arreter),
        )
        .route("/voice/{cid}/draw/settings", put(reglages))
        .route("/voice/{cid}/draw/start", post(lancer))
        .route("/voice/{cid}/draw/choose", post(choisir))
        .route("/voice/{cid}/draw/guess", post(proposer))
        .route("/voice/{cid}/draw/draw", post(dessiner))
        .route("/voice/{cid}/draw/vote", post(voter));
    for (methode, chemin, resume, reponses) in DOCS.iter() {
        r = openapi::documenter(
            r,
            chemin,
            methode.clone(),
            openapi::operation("vocal", resume, reponses, chemin),
        );
    }
    r
}

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.merge(routes_partie().layer(Extension(Arc::new(Contexte::nouveau()))))
}
