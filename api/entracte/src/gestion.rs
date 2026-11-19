use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use serde_json::{Map, Value, json};
use sqlx::Row;

use crate::{
    arr::Client,
    arr_gestion::{Cible, Modification, Service},
    config::Arr,
    demandes,
    etat::AppState,
    garde::Personne,
    http::{self, bd, erreur, introuvable, invalide, objet, ok, sans_contenu},
    tmdb::Tmdb,
};

const REFUS: [&str; 8] = [
    "profil de qualité inconnu",
    "emplacement inconnu",
    "tag inconnu",
    "film inconnu de Radarr",
    "série inconnue de Sonarr",
    "cible invalide pour Radarr",
    "cible invalide",
    "un seul épisode par recherche",
];

type Requete = Query<HashMap<String, String>>;

fn service(s: &str) -> Result<Service, Response> {
    Service::depuis(s).ok_or_else(introuvable)
}

fn configure(state: &AppState, s: Service) -> Option<&Arr> {
    match s {
        Service::Radarr => state.config().radarr.as_ref(),
        Service::Sonarr => state.config().sonarr.as_ref(),
    }
}

fn client(state: &AppState, s: Service) -> Result<Client<'_>, Response> {
    configure(state, s)
        .map(|c| Client::new(c, state.http()))
        .ok_or_else(|| http::indisponible("arr_unavailable", &format!("{} non configuré", s.nom())))
}

fn amont(s: Service, e: String) -> Response {
    if e == "HTTP 404" {
        return introuvable();
    }
    if REFUS.contains(&e.as_str()) {
        return invalide(&e);
    }
    tracing::warn!("{} : {e}", s.nom());
    http::indisponible("arr_unavailable", &format!("{} : {e}", s.nom()))
}

fn identifiant(s: &str) -> Result<i64, Response> {
    s.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalide("identifiant invalide"))
}

fn entier(corps: &Map<String, Value>, cle: &str) -> Result<Option<i64>, Response> {
    match corps.get(cle) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .filter(|n| *n > 0)
            .map(Some)
            .ok_or_else(|| invalide(&format!("{cle} invalide"))),
    }
}

fn booleen(corps: &Map<String, Value>, cle: &str) -> Result<Option<bool>, Response> {
    match corps.get(cle) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(invalide(&format!("{cle} : booléen attendu"))),
    }
}

fn identifiants(corps: &Map<String, Value>, cle: &str) -> Result<Option<Vec<i64>>, Response> {
    match corps.get(cle) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(l)) if !l.is_empty() && l.len() <= 500 => l
            .iter()
            .map(|x| x.as_i64().filter(|n| *n > 0))
            .collect::<Option<Vec<i64>>>()
            .map(Some)
            .ok_or_else(|| invalide(&format!("{cle} invalide"))),
        Some(_) => Err(invalide(&format!("{cle} invalide"))),
    }
}

fn saison(v: i64) -> Result<i32, Response> {
    i32::try_from(v)
        .ok()
        .filter(|n| (0..=1000).contains(n))
        .ok_or_else(|| invalide("saison invalide"))
}

fn vrai(q: &HashMap<String, String>, cle: &str) -> bool {
    q.get(cle).is_some_and(|v| v == "true" || v == "1")
}

async fn partout<'a, F, Fut>(state: &'a AppState, f: F) -> (Vec<Value>, Vec<Value>)
where
    F: Fn(Client<'a>, Service) -> Fut,
    Fut: Future<Output = Result<Vec<Value>, String>>,
{
    let mut elements = Vec::new();
    let mut erreurs = Vec::new();
    for s in Service::TOUS {
        let Some(c) = configure(state, s) else {
            continue;
        };
        match f(Client::new(c, state.http()), s).await {
            Ok(l) => elements.extend(l),
            Err(e) => {
                tracing::warn!("{} : {e}", s.nom());
                erreurs.push(json!({ "service": s.cle(), "message": e }));
            }
        }
    }
    (elements, erreurs)
}

fn services(state: &AppState) -> Value {
    json!({
        "radarr": state.config().radarr.is_some(),
        "sonarr": state.config().sonarr.is_some(),
    })
}

pub async fn titres(State(state): State<AppState>) -> Response {
    let (items, errors) = partout(&state, |c, s| async move { c.titres(s).await }).await;
    ok(&json!({ "items": items, "errors": errors, "services": services(&state) }))
}

pub async fn ajouter(
    State(state): State<AppState>,
    Path(s): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let corps = objet(corps).await?;
    let tmdb_id = entier(&corps, "tmdb_id")?.ok_or_else(|| invalide("tmdb_id obligatoire"))?;
    let choix = demandes::reglages(&corps)?;
    let c = client(&state, s)?;
    let resultat = match s {
        Service::Radarr => c.ajouter_film(tmdb_id, &choix).await,
        Service::Sonarr => {
            let saisons = match corps.get("seasons") {
                None | Some(Value::Null) => None,
                Some(Value::Array(l)) if !l.is_empty() && l.len() <= 100 => Some(
                    l.iter()
                        .map(|x| {
                            x.as_i64()
                                .ok_or_else(|| invalide("saison invalide"))
                                .and_then(saison)
                        })
                        .collect::<Result<Vec<i32>, Response>>()?,
                ),
                Some(_) => return Err(invalide("seasons invalide")),
            };
            let tvdb = Tmdb::new(state.config(), state.http())
                .serie(tmdb_id)
                .await
                .ok()
                .and_then(|f| f.tvdb_id);
            c.ajouter_serie(tmdb_id, tvdb, saisons.as_deref(), &choix)
                .await
        }
    };
    resultat.map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}

pub async fn fiche(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    let f = client(&state, s)?
        .fiche(s, id)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(ok(&f))
}

pub async fn modifier(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    let corps = objet(corps).await?;
    let saisons = match corps.get("seasons") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(l)) if l.len() <= 100 => l
            .iter()
            .map(|x| {
                let n = x["number"]
                    .as_i64()
                    .ok_or_else(|| invalide("saison invalide"))?;
                let m = x["monitored"]
                    .as_bool()
                    .ok_or_else(|| invalide("monitored : booléen attendu"))?;
                Ok((saison(n)?, m))
            })
            .collect::<Result<Vec<(i32, bool)>, Response>>()?,
        Some(_) => return Err(invalide("seasons invalide")),
    };
    let m = Modification {
        surveille: booleen(&corps, "monitored")?,
        profil: entier(&corps, "profile")?,
        saisons,
        nouvelles_saisons: booleen(&corps, "monitor_new")?,
    };
    if m == Modification::default() {
        return Err(invalide("aucune modification"));
    }
    let c = client(&state, s)?;
    c.modifier(s, id, &m).await.map_err(|e| amont(s, e))?;
    Ok(ok(&c.fiche(s, id).await.map_err(|e| amont(s, e))?))
}

pub async fn supprimer(
    State(state): State<AppState>,
    p: Personne,
    Path((s, id)): Path<(String, String)>,
    Query(q): Requete,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    let motif = q
        .get("reason")
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .unwrap_or("Titre retiré du catalogue");
    if motif.chars().count() > 500 {
        return Err(invalide("motif trop long"));
    }
    let tmdb = client(&state, s)?
        .supprimer(s, id, vrai(&q, "files"), vrai(&q, "exclude"))
        .await
        .map_err(|e| amont(s, e))?;
    if let Some(tmdb) = tmdb {
        let genre = if s == Service::Radarr {
            "film"
        } else {
            "serie"
        };
        let refusees = sqlx::query(
            "UPDATE demandes SET etat = 'refusee', motif = $3, traitee_par = $4, maj = now()
             WHERE genre = $1 AND tmdb_id = $2 AND etat IN ('en_attente', 'acceptee')
             RETURNING id, titre",
        )
        .bind(genre)
        .bind(tmdb)
        .bind(motif)
        .bind(p.sub())
        .fetch_all(state.pool())
        .await
        .map_err(bd)?;
        for l in refusees {
            let titre: String = l.get(1);
            crate::push::prevenir(
                &state,
                l.get(0),
                "refusee",
                &format!("Demande refusée : {titre}"),
                motif,
            )
            .await;
        }
    }
    Ok(sans_contenu())
}

pub async fn surveiller_episodes(
    State(state): State<AppState>,
    Path(s): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    if s != Service::Sonarr {
        return Err(invalide("seulement pour sonarr"));
    }
    let corps = objet(corps).await?;
    let ids = identifiants(&corps, "episodes")?.ok_or_else(|| invalide("episodes obligatoire"))?;
    let surveille =
        booleen(&corps, "monitored")?.ok_or_else(|| invalide("monitored obligatoire"))?;
    client(&state, s)?
        .surveiller_episodes(&ids, surveille)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}

fn cible(corps: &Map<String, Value>) -> Result<Cible, Response> {
    if booleen(corps, "missing")? == Some(true) {
        return Ok(Cible::Manquants);
    }
    if let Some(ids) = identifiants(corps, "episodes")? {
        return Ok(Cible::Episodes(ids));
    }
    let titre = entier(corps, "title")?.ok_or_else(|| invalide("title obligatoire"))?;
    Ok(match corps.get("season").and_then(Value::as_i64) {
        Some(n) => Cible::Saison(titre, saison(n)?),
        None => Cible::Titre(titre),
    })
}

pub async fn rechercher(
    State(state): State<AppState>,
    Path(s): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let cible = cible(&objet(corps).await?)?;
    client(&state, s)?
        .rechercher(s, &cible)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}

pub async fn releases(
    State(state): State<AppState>,
    Path(s): Path<String>,
    Query(q): Requete,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let lu = |cle: &str| q.get(cle).filter(|v| !v.is_empty());
    let cible = match (s, lu("title"), lu("season"), lu("episode")) {
        (Service::Radarr, Some(t), None, None) => Cible::Titre(identifiant(t)?),
        (Service::Sonarr, _, _, Some(e)) => Cible::Episodes(vec![identifiant(e)?]),
        (Service::Sonarr, Some(t), Some(n), None) => Cible::Saison(
            identifiant(t)?,
            saison(n.parse().map_err(|_| invalide("saison invalide"))?)?,
        ),
        _ => return Err(invalide("title, ou title et season, ou episode")),
    };
    let items = client(&state, s)?
        .releases(&cible)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(ok(&json!({ "items": items })))
}

pub async fn recuperer(
    State(state): State<AppState>,
    Path(s): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let corps = objet(corps).await?;
    let guid = corps
        .get("guid")
        .and_then(Value::as_str)
        .filter(|g| !g.is_empty() && g.chars().count() <= 2000)
        .ok_or_else(|| invalide("guid obligatoire"))?;
    let indexeur =
        entier(&corps, "indexer_id")?.ok_or_else(|| invalide("indexer_id obligatoire"))?;
    client(&state, s)?
        .recuperer(guid, indexeur)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}

pub async fn file(State(state): State<AppState>) -> Response {
    let (items, errors) = partout(&state, |c, s| async move { c.file(s).await }).await;
    ok(&json!({ "items": items, "errors": errors }))
}

pub async fn retirer(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
    Query(q): Requete,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    client(&state, s)?
        .retirer(id, vrai(&q, "blocklist"), vrai(&q, "search"))
        .await
        .map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}

pub async fn import_possible(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    let items = client(&state, s)?
        .import_possible(s, id)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(ok(&json!({ "items": items })))
}

pub async fn importer(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
    corps: Body,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    let corps = objet(corps).await?;
    let noms: Vec<String> = match corps.get("files") {
        Some(Value::Array(l)) if !l.is_empty() && l.len() <= 500 => l
            .iter()
            .map(|x| x.as_str().map(String::from))
            .collect::<Option<Vec<String>>>()
            .ok_or_else(|| invalide("files invalide"))?,
        _ => return Err(invalide("files obligatoire")),
    };
    let nombre = client(&state, s)?
        .importer(s, id, &noms)
        .await
        .map_err(|e| amont(s, e))?;
    if nombre == 0 {
        return Err(erreur(
            StatusCode::CONFLICT,
            "nothing_to_import",
            &format!("{} ne reconnaît aucun de ces fichiers", s.nom()),
        ));
    }
    Ok(ok(&json!({ "files": nombre })))
}

pub async fn manquants(State(state): State<AppState>) -> Response {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut totaux = json!({});
    for s in Service::TOUS {
        let Some(c) = configure(&state, s) else {
            continue;
        };
        match Client::new(c, state.http()).manquants(s).await {
            Ok((l, total)) => {
                totaux[s.cle()] = json!(total);
                items.extend(l);
            }
            Err(e) => {
                tracing::warn!("{} : {e}", s.nom());
                errors.push(json!({ "service": s.cle(), "message": e }));
            }
        }
    }
    ok(&json!({ "items": items, "totals": totaux, "errors": errors }))
}

pub async fn historique(
    State(state): State<AppState>,
    Query(q): Requete,
) -> Result<Response, Response> {
    let titre = q
        .get("title")
        .filter(|t| !t.is_empty())
        .map(|t| identifiant(t))
        .transpose()?;
    let voulu = q
        .get("service")
        .filter(|s| !s.is_empty())
        .map(|s| Service::depuis(s).ok_or_else(|| invalide("service inconnu")))
        .transpose()?;
    if titre.is_some() && voulu.is_none() {
        return Err(invalide("service obligatoire avec title"));
    }
    let (mut items, errors) = partout(&state, |c, s| async move {
        if voulu.is_some_and(|v| v != s) {
            return Ok(Vec::new());
        }
        c.historique(s, titre).await
    })
    .await;
    items.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    Ok(ok(&json!({ "items": items, "errors": errors })))
}

pub async fn liste_noire(State(state): State<AppState>) -> Response {
    let (mut items, errors) = partout(&state, |c, s| async move { c.liste_noire(s).await }).await;
    items.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    ok(&json!({ "items": items, "errors": errors }))
}

pub async fn retirer_de_la_liste_noire(
    State(state): State<AppState>,
    Path((s, id)): Path<(String, String)>,
) -> Result<Response, Response> {
    let s = service(&s)?;
    let id = identifiant(&id)?;
    client(&state, s)?
        .retirer_de_la_liste_noire(id)
        .await
        .map_err(|e| amont(s, e))?;
    Ok(sans_contenu())
}
