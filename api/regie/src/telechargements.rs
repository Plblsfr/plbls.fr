use std::{collections::HashMap, time::Duration};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::Response,
};
use serde_json::{Value, json};

use crate::{
    arr::Service,
    etat::AppState,
    http::{indisponible, introuvable, invalide, ok, sans_contenu},
    qbittorrent::{Client, hash_valide},
    sante, suivi,
};

const ATTENTE: Duration = Duration::from_secs(20);
const HASHES_MAX: usize = 200;
const TAGS_MAX: usize = 20;
const DEBIT_MAX: i64 = 10_000_000_000;
const PRIORITES: [i64; 4] = [0, 1, 6, 7];

fn qbittorrent(state: &AppState) -> Result<Client<'_>, Response> {
    state
        .config()
        .qbittorrent
        .as_ref()
        .map(|cfg| Client::new(cfg, state.http(), state.session_qbittorrent()))
        .ok_or_else(|| indisponible("not_configured", "qBittorrent n'est pas configuré"))
}

fn amont(e: String) -> Response {
    indisponible("upstream", &format!("qBittorrent : {e}"))
}

fn corps(octets: &Bytes) -> Result<Value, Response> {
    serde_json::from_slice::<Value>(octets)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| invalide("corps JSON attendu"))
}

fn hashes(v: &Value) -> Result<String, Response> {
    let liste: Vec<&str> = v["hashes"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let complet = v["hashes"]
        .as_array()
        .is_some_and(|a| a.len() == liste.len());
    if !complet
        || liste.is_empty()
        || liste.len() > HASHES_MAX
        || !liste.iter().all(|h| hash_valide(h))
    {
        return Err(invalide("hashes : 1 à 200 identifiants de torrent"));
    }
    Ok(liste.join("|").to_ascii_lowercase())
}

fn debit(v: &Value, cle: &str) -> Result<i64, Response> {
    v[cle]
        .as_i64()
        .filter(|d| (0..=DEBIT_MAX).contains(d))
        .ok_or_else(|| {
            invalide(&format!(
                "{cle} : débit en octets par seconde, 0 = sans limite"
            ))
        })
}

fn tags(v: &Value, cle: &str) -> Result<Vec<String>, Response> {
    let Some(liste) = v.get(cle) else {
        return Ok(Vec::new());
    };
    let lus: Vec<String> = liste
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|t| t.trim().to_string())
        .collect();
    let valides = liste.as_array().is_some_and(|a| a.len() == lus.len())
        && lus.len() <= TAGS_MAX
        && lus.iter().all(|t| {
            !t.is_empty()
                && t.chars().count() <= 50
                && !t.contains(',')
                && !t.chars().any(char::is_control)
        });
    if !valides {
        return Err(invalide(&format!(
            "{cle} : 20 tags au plus, 50 caractères au plus, sans virgule"
        )));
    }
    Ok(lus)
}

fn fait(state: &AppState, r: Result<(), String>) -> Response {
    match r {
        Ok(()) => {
            state.suivi().relire();
            sans_contenu()
        }
        Err(e) => amont(e),
    }
}

pub async fn lister(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Err(r) = qbittorrent(&state) {
        return r;
    }
    let depuis = q.get("since").and_then(|v| v.parse::<u64>().ok());
    let mut rx = state.suivi().abonner();
    suivi::assurer(&state);
    let _ = tokio::time::timeout(
        ATTENTE,
        rx.wait_for(|e| e.version != 0 && Some(e.version) != depuis),
    )
    .await;
    let e = rx.borrow().clone();
    let mut v = e.corps.clone();
    if !v.is_object() {
        v = json!({ "error": "qBittorrent ne répond pas encore" });
    }
    v["version"] = json!(e.version);
    ok(&v)
}

pub async fn suspendre(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, h) = match (qbittorrent(&state), corps(&octets).and_then(|v| hashes(&v))) {
        (Ok(c), Ok(h)) => (c, h),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    fait(&state, c.arreter(&h).await)
}

pub async fn reprendre(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, h) = match (qbittorrent(&state), corps(&octets).and_then(|v| hashes(&v))) {
        (Ok(c), Ok(h)) => (c, h),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    fait(&state, c.demarrer(&h).await)
}

pub async fn reverifier(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, h) = match (qbittorrent(&state), corps(&octets).and_then(|v| hashes(&v))) {
        (Ok(c), Ok(h)) => (c, h),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    fait(&state, c.reverifier(&h).await)
}

pub async fn supprimer(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let h = match hashes(&v) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let Some(fichiers) = v["delete_files"].as_bool() else {
        return invalide("delete_files : booléen attendu");
    };
    fait(&state, c.supprimer(&h, fichiers).await)
}

pub async fn categoriser(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let h = match hashes(&v) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let Some(categorie) = v["category"].as_str() else {
        return invalide("category : texte attendu, vide pour retirer la catégorie");
    };
    if !categorie.is_empty() {
        let connues = match c.etat().await {
            Ok(e) => e["categories"].clone(),
            Err(e) => return amont(e),
        };
        if !connues
            .as_array()
            .is_some_and(|l| l.iter().any(|x| x == categorie))
        {
            return invalide("category : catégorie inconnue de qBittorrent");
        }
    }
    fait(&state, c.categoriser(&h, categorie).await)
}

pub async fn etiqueter(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let (h, ajout, retrait) = match (hashes(&v), tags(&v, "add"), tags(&v, "remove")) {
        (Ok(h), Ok(a), Ok(r)) => (h, a, r),
        (Err(r), _, _) | (_, Err(r), _) | (_, _, Err(r)) => return r,
    };
    if ajout.is_empty() && retrait.is_empty() {
        return invalide("add ou remove : au moins un tag");
    }
    let mut r = Ok(());
    if !retrait.is_empty() {
        r = c.etiqueter(&h, &retrait.join(","), false).await;
    }
    if r.is_ok() && !ajout.is_empty() {
        r = c.etiqueter(&h, &ajout.join(","), true).await;
    }
    fait(&state, r)
}

pub async fn limiter(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let (h, reception, envoi) = match (hashes(&v), debit(&v, "download"), debit(&v, "upload")) {
        (Ok(h), Ok(d), Ok(u)) => (h, d, u),
        (Err(r), _, _) | (_, Err(r), _) | (_, _, Err(r)) => return r,
    };
    fait(&state, c.limiter(&h, reception, envoi).await)
}

pub async fn limiter_tout(State(state): State<AppState>, octets: Bytes) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let (reception, envoi) = match (debit(&v, "download"), debit(&v, "upload")) {
        (Ok(d), Ok(u)) => (d, u),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let Some(alternatif) = v["alternative"].as_bool() else {
        return invalide("alternative : booléen attendu");
    };
    let mut r = c.limiter_tout(reception, envoi).await;
    if r.is_ok() {
        r = c.mode_alternatif(alternatif).await;
    }
    fait(&state, r)
}

async fn titre(state: &AppState, hash: &str) -> Value {
    let courant = state.suivi().courant();
    let connu = courant.corps["torrents"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["hash"] == hash)
        .map(|t| t["title"].clone())
        .filter(|t| !t.is_null());
    if let Some(t) = connu {
        return t;
    }
    for service in Service::BIBLIOTHEQUES {
        let Some(c) = sante::client(state, service) else {
            continue;
        };
        if let Ok(Some(t)) = c.titre_par_historique(hash).await {
            return t;
        }
    }
    Value::Null
}

pub async fn detail(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let c = match qbittorrent(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };
    if !hash_valide(&hash) {
        return introuvable();
    }
    let hash = hash.to_ascii_lowercase();
    match c.fichiers(&hash).await {
        Ok(Some(fichiers)) => ok(&json!({
            "hash": hash,
            "files": fichiers,
            "title": titre(&state, &hash).await,
        })),
        Ok(None) => introuvable(),
        Err(e) => amont(e),
    }
}

pub async fn prioriser(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    octets: Bytes,
) -> Response {
    let (c, v) = match (qbittorrent(&state), corps(&octets)) {
        (Ok(c), Ok(v)) => (c, v),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    if !hash_valide(&hash) {
        return introuvable();
    }
    let ids: Vec<i64> = v["ids"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let complets = v["ids"].as_array().is_some_and(|a| a.len() == ids.len());
    if !complets || ids.is_empty() || ids.len() > 5000 || ids.iter().any(|i| *i < 0) {
        return invalide("ids : index des fichiers attendus");
    }
    let Some(priorite) = v["priority"].as_i64().filter(|p| PRIORITES.contains(p)) else {
        return invalide("priority : 0 (ignoré), 1 (normale), 6 (haute) ou 7 (maximale)");
    };
    let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
    match c
        .prioriser(&hash.to_ascii_lowercase(), &ids.join("|"), priorite)
        .await
    {
        Ok(true) => {
            state.suivi().relire();
            sans_contenu()
        }
        Ok(false) => introuvable(),
        Err(e) => amont(e),
    }
}
