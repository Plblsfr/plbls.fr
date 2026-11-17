use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    path::{Path as Chemin, PathBuf},
    sync::Mutex,
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt},
    sync::{Mutex as MutexAsync, watch},
};

use crate::{
    arr::{self, Service},
    bencode::{Fichier, Torrent},
    config::Depots,
    etat::AppState,
    http::{
        conflit, indisponible, interne, introuvable, invalide, json as reponse, ok, sans_contenu,
    },
    qbittorrent::hash_valide,
    sante,
    telechargements::{amont, corps, qbittorrent},
};

pub const TORRENT_MAX: usize = 10 << 20;
pub const MORCEAU_MAX: usize = 32 << 20;
const ATTENTE: Duration = Duration::from_secs(20);
const TOURS_SANS_VERIFICATION: u32 = 10;
const ECHECS_MAX: u32 = 100;
const RESULTATS_MAX: usize = 30;

pub struct Etat {
    ecriture: MutexAsync<()>,
    surveilles: Mutex<HashSet<String>>,
    version: watch::Sender<u64>,
}

impl Default for Etat {
    fn default() -> Self {
        Self {
            ecriture: MutexAsync::new(()),
            surveilles: Mutex::default(),
            version: watch::channel(0).0,
        }
    }
}

struct Depot {
    torrent: Torrent,
    meta: Value,
}

impl Depot {
    fn etat(&self) -> &str {
        self.meta["state"].as_str().unwrap_or("upload")
    }
}

fn configuration(state: &AppState) -> Result<&Depots, Response> {
    state.config().depots.as_ref().ok_or_else(|| {
        indisponible(
            "not_configured",
            "le dossier des dépôts n'est pas configuré",
        )
    })
}

fn es(e: std::io::Error) -> Response {
    tracing::warn!("dépôts : {e}");
    interne()
}

fn dossier(d: &Depots, hash: &str) -> PathBuf {
    d.dossier.join(hash)
}

fn chemin_de(d: &Depots, hash: &str, f: &Fichier) -> PathBuf {
    let mut p = dossier(d, hash).join("contenu");
    p.extend(&f.chemin);
    p
}

async fn recu(chemin: &Chemin) -> u64 {
    tokio::fs::metadata(chemin).await.map_or(0, |m| m.len())
}

async fn charger(d: &Depots, hash: &str) -> Option<Depot> {
    let racine = dossier(d, hash);
    let source = tokio::fs::read(racine.join("source.torrent")).await.ok()?;
    let torrent = Torrent::lire(&source).ok().filter(|t| t.hash == hash)?;
    let meta = tokio::fs::read(racine.join("depot.json"))
        .await
        .ok()
        .and_then(|o| serde_json::from_slice::<Value>(&o).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    Some(Depot { torrent, meta })
}

async fn enregistrer(
    state: &AppState,
    d: &Depots,
    hash: &str,
    meta: &Value,
) -> std::io::Result<()> {
    let racine = dossier(d, hash);
    let provisoire = racine.join("depot.json.tmp");
    tokio::fs::write(&provisoire, meta.to_string()).await?;
    tokio::fs::rename(&provisoire, racine.join("depot.json")).await?;
    state.depots().version.send_modify(|v| *v += 1);
    Ok(())
}

async fn vue(d: &Depots, hash: &str, depot: &Depot) -> Value {
    let mut fichiers = Vec::with_capacity(depot.torrent.fichiers.len());
    let mut total_recu = 0;
    for (i, f) in depot.torrent.fichiers.iter().enumerate() {
        let r = recu(&chemin_de(d, hash, f)).await.min(f.taille);
        total_recu += r;
        fichiers.push(json!({
            "index": i,
            "path": f.chemin.join("/"),
            "size": f.taille,
            "received": r,
            "valid": depot.meta["checks"][i].as_f64(),
        }));
    }
    json!({
        "id": hash,
        "name": depot.torrent.nom,
        "state": depot.etat(),
        "created_at": depot.meta["created_at"].as_i64().unwrap_or(0),
        "tracker": depot.torrent.tracker,
        "private": depot.torrent.prive,
        "size": depot.torrent.total(),
        "received": total_recu,
        "files": fichiers,
        "imported": depot.meta["imported"].as_array().cloned().unwrap_or_default(),
        "error": depot.meta["error"].as_str(),
    })
}

async fn identifiants(d: &Depots) -> Vec<String> {
    let mut noms = Vec::new();
    let Ok(mut lecture) = tokio::fs::read_dir(&d.dossier).await else {
        return noms;
    };
    while let Ok(Some(e)) = lecture.next_entry().await {
        if let Some(nom) = e.file_name().to_str().filter(|n| hash_valide(n)) {
            noms.push(nom.to_string());
        }
    }
    noms
}

fn ouvrir_au_groupe(racine: PathBuf) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut a_voir = vec![racine];
    while let Some(chemin) = a_voir.pop() {
        let meta = std::fs::symlink_metadata(&chemin)?;
        if meta.is_dir() {
            std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o775))?;
            for e in std::fs::read_dir(&chemin)? {
                a_voir.push(e?.path());
            }
        } else if meta.is_file() {
            std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o664))?;
        }
    }
    Ok(())
}

fn maintenant() -> i64 {
    chrono::Utc::now().timestamp()
}

fn identifiant(hash: &str) -> Result<String, Response> {
    if hash_valide(hash) {
        Ok(hash.to_ascii_lowercase())
    } else {
        Err(introuvable())
    }
}

async fn trouver<'a>(
    state: &'a AppState,
    hash: &str,
) -> Result<(&'a Depots, String, Depot), Response> {
    let d = configuration(state)?;
    let hash = identifiant(hash)?;
    let depot = charger(d, &hash).await.ok_or_else(introuvable)?;
    Ok((d, hash, depot))
}

pub async fn lister(State(state): State<AppState>) -> Response {
    let d = match configuration(&state) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut depots = Vec::new();
    for hash in identifiants(d).await {
        if let Some(depot) = charger(d, &hash).await {
            reprendre_surveillance(&state, &hash, &depot);
            depots.push(vue(d, &hash, &depot).await);
        }
    }
    depots.sort_by_key(|v| std::cmp::Reverse(v["created_at"].as_i64()));
    let occupe: u64 = depots.iter().filter_map(|v| v["size"].as_u64()).sum();
    ok(&json!({ "items": depots, "used": occupe, "limit": d.plafond }))
}

pub async fn creer(State(state): State<AppState>, octets: Bytes) -> Response {
    let d = match configuration(&state) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let torrent = match Torrent::lire(&octets) {
        Ok(t) => t,
        Err(e) => return invalide(&e),
    };
    let _verrou = state.depots().ecriture.lock().await;
    if let Some(existant) = charger(d, &torrent.hash).await {
        return ok(&vue(d, &torrent.hash, &existant).await);
    }
    let mut occupe = 0u64;
    for hash in identifiants(d).await {
        if let Some(autre) = charger(d, &hash).await {
            occupe = occupe.saturating_add(autre.torrent.total());
        }
    }
    if occupe.saturating_add(torrent.total()) > d.plafond {
        return conflit(
            "storage_full",
            "le plafond du dossier des dépôts serait dépassé",
        );
    }
    let racine = dossier(d, &torrent.hash);
    let meta = json!({ "state": "upload", "created_at": maintenant(), "imported": [] });
    let ecrit = async {
        tokio::fs::create_dir_all(racine.join("contenu")).await?;
        tokio::fs::write(racine.join("source.torrent"), &octets).await?;
        enregistrer(&state, d, &torrent.hash, &meta).await
    };
    if let Err(e) = ecrit.await {
        let _ = tokio::fs::remove_dir_all(&racine).await;
        return es(e);
    }
    let depot = Depot { torrent, meta };
    reponse(
        StatusCode::CREATED,
        &vue(d, &depot.torrent.hash, &depot).await,
    )
}

pub async fn lire(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let (d, hash, mut depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    reprendre_surveillance(&state, &hash, &depot);
    if q.get("wait").is_some_and(|e| e == depot.etat()) {
        let mut rx = state.depots().version.subscribe();
        let attendu = depot.etat().to_string();
        let _ = tokio::time::timeout(ATTENTE, async {
            while rx.changed().await.is_ok() {
                match charger(d, &hash).await {
                    Some(nouveau) if nouveau.etat() == attendu => {}
                    _ => break,
                }
            }
        })
        .await;
        depot = match charger(d, &hash).await {
            Some(nouveau) => nouveau,
            None => return introuvable(),
        };
    }
    ok(&vue(d, &hash, &depot).await)
}

pub async fn recevoir(
    State(state): State<AppState>,
    Path((hash, index)): Path<(String, usize)>,
    Query(q): Query<HashMap<String, String>>,
    octets: Bytes,
) -> Response {
    let (d, hash, depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let Some(fichier) = depot.torrent.fichiers.get(index) else {
        return introuvable();
    };
    if !matches!(depot.etat(), "upload" | "mismatch") {
        return conflit("bad_state", "ce dépôt n'accepte plus de fichier");
    }
    let Some(position) = q.get("offset").and_then(|o| o.parse::<u64>().ok()) else {
        return invalide("offset : position du morceau attendue");
    };
    let fin = position.saturating_add(octets.len() as u64);
    if fin > fichier.taille {
        return invalide("le morceau dépasse la taille annoncée par le torrent");
    }
    let chemin = chemin_de(d, &hash, fichier);
    let _verrou = state.depots().ecriture.lock().await;
    let present = recu(&chemin).await;
    let remplacer = position == 0 && q.get("reset").is_some_and(|v| v == "true");
    if present != position && !remplacer {
        return reponse(
            StatusCode::CONFLICT,
            &json!({ "error": "bad_offset", "message": "position inattendue", "received": present.min(fichier.taille) }),
        );
    }
    let ecrit = async {
        if let Some(parent) = chemin.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&chemin)
            .await?;
        f.set_len(position).await?;
        f.seek(SeekFrom::Start(position)).await?;
        f.write_all(&octets).await?;
        f.flush().await
    };
    if let Err(e) = ecrit.await {
        return es(e);
    }
    if remplacer && depot.meta["checks"].is_array() {
        let mut meta = depot.meta.clone();
        meta["checks"][index] = Value::Null;
        if let Err(e) = enregistrer(&state, d, &hash, &meta).await {
            return es(e);
        }
    }
    ok(&json!({ "received": fin }))
}

async fn conclure(state: &AppState, d: &Depots, hash: &str, changer: impl FnOnce(&mut Value)) {
    let Some(depot) = charger(d, hash).await else {
        return;
    };
    let mut meta = depot.meta;
    changer(&mut meta);
    if let Err(e) = enregistrer(state, d, hash, &meta).await {
        tracing::warn!("dépôts : {e}");
    }
    state.suivi().relire();
}

async fn surveiller(state: AppState, hash: String) {
    let Some(d) = state.config().depots.as_ref() else {
        return;
    };
    let (mut vue_en_verification, mut tours, mut echecs) = (false, 0u32, 0u32);
    loop {
        tokio::time::sleep(state.config().periode_suivi).await;
        let Ok(c) = qbittorrent(&state) else {
            break;
        };
        let info = match c.info(&hash).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                conclure(&state, d, &hash, |m| {
                    m["state"] = json!("upload");
                    m["error"] = json!("le torrent n'est plus dans qBittorrent");
                })
                .await;
                break;
            }
            Err(e) => {
                echecs += 1;
                if echecs > ECHECS_MAX {
                    conclure(&state, d, &hash, |m| {
                        m["state"] = json!("mismatch");
                        m["error"] = json!(format!("qBittorrent : {e}"));
                    })
                    .await;
                    break;
                }
                continue;
            }
        };
        echecs = 0;
        tours += 1;
        let etat = info["state"].as_str().unwrap_or_default();
        let progression = info["progress"].as_f64().unwrap_or(0.0);
        if etat.starts_with("checking") || matches!(etat, "allocating" | "moving") {
            vue_en_verification = true;
            continue;
        }
        if !vue_en_verification && progression < 1.0 && tours < TOURS_SANS_VERIFICATION {
            continue;
        }
        if progression >= 1.0 {
            let demarre = c.demarrer(&hash).await;
            conclure(&state, d, &hash, |m| {
                m["state"] = json!("seeding");
                m["checks"] = Value::Null;
                m["error"] = demarre.err().map_or(Value::Null, |e| {
                    json!(format!("vérifié, mais le partage n'a pas démarré : {e}"))
                });
            })
            .await;
        } else {
            let valides: Vec<Value> = match c.fichiers(&hash).await {
                Ok(Some(f)) => f.iter().map(|x| x["progress"].clone()).collect(),
                _ => Vec::new(),
            };
            conclure(&state, d, &hash, |m| {
                m["state"] = json!("mismatch");
                m["checks"] = json!(valides);
                m["error"] = Value::Null;
            })
            .await;
        }
        break;
    }
    state.depots().surveilles.lock().unwrap().remove(&hash);
}

fn lancer_surveillance(state: &AppState, hash: &str) {
    if state
        .depots()
        .surveilles
        .lock()
        .unwrap()
        .insert(hash.to_string())
    {
        tokio::spawn(surveiller(state.clone(), hash.to_string()));
    }
}

fn reprendre_surveillance(state: &AppState, hash: &str, depot: &Depot) {
    if depot.etat() == "checking" {
        lancer_surveillance(state, hash);
    }
}

pub async fn partager(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let (d, hash, depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let c = match qbittorrent(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };
    if !matches!(depot.etat(), "upload" | "mismatch") {
        return conflit(
            "bad_state",
            "ce dépôt est déjà en vérification ou en partage",
        );
    }
    for f in &depot.torrent.fichiers {
        if recu(&chemin_de(d, &hash, f)).await != f.taille {
            return conflit("incomplete", "tous les fichiers ne sont pas encore reçus");
        }
    }
    let racine = dossier(d, &hash);
    match tokio::task::spawn_blocking(move || ouvrir_au_groupe(racine)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return es(e),
        Err(_) => return interne(),
    }
    let present = match c.info(&hash).await {
        Ok(i) => i.is_some(),
        Err(e) => return amont(e),
    };
    if !present {
        let source = match tokio::fs::read(dossier(d, &hash).join("source.torrent")).await {
            Ok(s) => s,
            Err(e) => return es(e),
        };
        let cible = format!("{}/{hash}/contenu", d.vu_par_qbittorrent);
        if let Err(e) = c.ajouter(&hash, &source, &cible).await {
            return amont(e);
        }
    }
    if let Err(e) = c.reverifier(&hash).await {
        return amont(e);
    }
    let mut meta = depot.meta.clone();
    meta["state"] = json!("checking");
    meta["checks"] = Value::Null;
    meta["error"] = Value::Null;
    if let Err(e) = enregistrer(&state, d, &hash, &meta).await {
        return es(e);
    }
    lancer_surveillance(&state, &hash);
    state.suivi().relire();
    let depot = Depot {
        torrent: depot.torrent,
        meta,
    };
    ok(&vue(d, &hash, &depot).await)
}

pub async fn supprimer(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let (d, hash, depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if depot.etat() != "upload" || state.config().qbittorrent.is_some() {
        let c = match qbittorrent(&state) {
            Ok(c) => c,
            Err(r) => return r,
        };
        match c.info(&hash).await {
            Ok(Some(_)) => {
                if let Err(e) = c.supprimer(&hash, false).await {
                    return amont(e);
                }
            }
            Ok(None) => {}
            Err(e) if depot.etat() == "upload" => tracing::debug!("qBittorrent : {e}"),
            Err(e) => return amont(e),
        }
    }
    let _verrou = state.depots().ecriture.lock().await;
    if let Err(e) = tokio::fs::remove_dir_all(dossier(d, &hash)).await {
        return es(e);
    }
    state.depots().version.send_modify(|v| *v += 1);
    state.suivi().relire();
    sans_contenu()
}

fn raisons(element: &Value) -> Vec<String> {
    element["rejections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["reason"].as_str())
        .map(String::from)
        .collect()
}

fn reconnu(service: Service, e: &Value) -> bool {
    let id = |v: &Value| v["id"].as_i64().is_some_and(|i| i > 0);
    let plusieurs = |cle: &str| {
        e[cle]
            .as_array()
            .is_some_and(|l| !l.is_empty() && l.iter().all(id))
    };
    match service {
        Service::Radarr => id(&e["movie"]),
        Service::Sonarr => id(&e["series"]) && plusieurs("episodes"),
        Service::Lidarr => id(&e["artist"]) && id(&e["album"]) && plusieurs("tracks"),
        Service::Prowlarr => false,
    }
}

fn titre_propose(service: Service, e: &Value) -> Value {
    let adapte = match service {
        Service::Sonarr => {
            let mut episode = e["episodes"][0].clone();
            if episode.is_object() && episode["seasonNumber"].is_null() {
                episode["seasonNumber"] = e["seasonNumber"].clone();
            }
            json!({ "series": e["series"], "episode": episode })
        }
        _ => e.clone(),
    };
    arr::titre_de(service, &adapte).unwrap_or(Value::Null)
}

fn fichier_a_importer(service: Service, e: &Value, film: Option<i64>) -> Value {
    let ids = |cle: &str| -> Vec<i64> {
        e[cle]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x["id"].as_i64())
            .collect()
    };
    let mut f = json!({
        "path": e["path"],
        "quality": e["quality"],
        "indexerFlags": e["indexerFlags"].as_i64().unwrap_or(0),
    });
    match service {
        Service::Radarr => {
            f["folderName"] = e["folderName"].clone();
            f["movieId"] = json!(film.or_else(|| e["movie"]["id"].as_i64()));
            f["languages"] = e["languages"].clone();
            f["releaseGroup"] = e["releaseGroup"].clone();
        }
        Service::Sonarr => {
            f["folderName"] = e["folderName"].clone();
            f["seriesId"] = e["series"]["id"].clone();
            f["episodeIds"] = json!(ids("episodes"));
            f["languages"] = e["languages"].clone();
            f["releaseGroup"] = e["releaseGroup"].clone();
            f["releaseType"] = e["releaseType"].clone();
        }
        Service::Lidarr => {
            f["artistId"] = e["artist"]["id"].clone();
            f["albumId"] = e["album"]["id"].clone();
            f["albumReleaseId"] = e["albumReleaseId"].clone();
            f["trackIds"] = json!(ids("tracks"));
            f["disableReleaseSwitching"] = json!(false);
        }
        Service::Prowlarr => {}
    }
    f
}

struct Import<'a> {
    service: Service,
    client: arr::Client<'a>,
    racine: String,
    elements: Vec<Value>,
}

async fn elements_a_importer<'a>(
    state: &'a AppState,
    d: &Depots,
    hash: &str,
    service: Option<&str>,
) -> Result<Import<'a>, Response> {
    let service = service
        .and_then(Service::bibliotheque)
        .ok_or_else(|| invalide("service : radarr, sonarr ou lidarr"))?;
    let client = sante::client(state, service).ok_or_else(|| {
        indisponible(
            "not_configured",
            &format!("{} n'est pas configuré", service.nom()),
        )
    })?;
    let racine = format!("{}/{hash}/contenu", d.vu_par_arr);
    let prefixe = format!("{racine}/");
    let elements = client
        .import_manuel(&racine)
        .await
        .map_err(|e| indisponible("upstream", &format!("{} : {e}", service.nom())))?
        .into_iter()
        .filter(|e| {
            e["path"]
                .as_str()
                .is_some_and(|p| p.starts_with(&prefixe) && !p.contains("/../"))
        })
        .collect();
    Ok(Import {
        service,
        client,
        racine,
        elements,
    })
}

pub async fn proposer_import(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let (d, hash, depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    if depot.etat() != "seeding" {
        return conflit("bad_state", "l'import attend la vérification du dépôt");
    }
    let import =
        match elements_a_importer(&state, d, &hash, q.get("service").map(String::as_str)).await {
            Ok(i) => i,
            Err(r) => return r,
        };
    let propositions: Vec<Value> = import
        .elements
        .iter()
        .map(|e| {
            let chemin = e["path"].as_str().unwrap_or_default();
            json!({
                "path": chemin.strip_prefix(&import.racine).unwrap_or(chemin).trim_start_matches('/'),
                "size": e["size"].as_i64(),
                "title": titre_propose(import.service, e),
                "recognized": reconnu(import.service, e),
                "reasons": raisons(e),
            })
        })
        .collect();
    ok(&json!({ "service": import.service.cle(), "items": propositions }))
}

pub async fn importer(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    octets: Bytes,
) -> Response {
    let (d, hash, depot) = match trouver(&state, &hash).await {
        Ok(t) => t,
        Err(r) => return r,
    };
    let v = match corps(&octets) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if depot.etat() != "seeding" {
        return conflit("bad_state", "l'import attend la vérification du dépôt");
    }
    let film = match &v["movie_id"] {
        Value::Null => None,
        autre => match autre.as_i64().filter(|i| *i > 0) {
            Some(i) => Some(i),
            None => return invalide("movie_id : identifiant de film attendu"),
        },
    };
    let import = match elements_a_importer(&state, d, &hash, v["service"].as_str()).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    if film.is_some() && import.service != Service::Radarr {
        return invalide("movie_id : seulement pour radarr");
    }
    let fichiers: Vec<Value> = import
        .elements
        .iter()
        .filter(|e| film.is_some() || reconnu(import.service, e))
        .map(|e| fichier_a_importer(import.service, e, film))
        .collect();
    if fichiers.is_empty() {
        return conflit(
            "nothing_to_import",
            &format!(
                "{} ne reconnaît aucun fichier de ce dépôt",
                import.service.nom()
            ),
        );
    }
    let nombre = fichiers.len();
    if let Err(e) = import.client.importer(fichiers).await {
        return indisponible("upstream", &format!("{} : {e}", import.service.nom()));
    }
    let mut meta = depot.meta.clone();
    let mut importes: Vec<Value> = meta["imported"].as_array().cloned().unwrap_or_default();
    if !importes.iter().any(|s| s == import.service.cle()) {
        importes.push(json!(import.service.cle()));
    }
    meta["imported"] = json!(importes);
    if let Err(e) = enregistrer(&state, d, &hash, &meta).await {
        return es(e);
    }
    ok(&json!({ "service": import.service.cle(), "files": nombre }))
}

pub async fn bibliotheque(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(service) = Service::bibliotheque(&service) else {
        return introuvable();
    };
    let Some(c) = sante::client(&state, service) else {
        return indisponible(
            "not_configured",
            &format!("{} n'est pas configuré", service.nom()),
        );
    };
    let mots: Vec<String> = q
        .get("q")
        .map(|t| t.to_lowercase())
        .unwrap_or_default()
        .split_whitespace()
        .take(8)
        .map(String::from)
        .collect();
    if mots.is_empty() {
        return ok(&json!({ "items": [] }));
    }
    match c.bibliotheque().await {
        Ok(titres) => {
            let trouves: Vec<Value> = titres
                .into_iter()
                .filter(|t| {
                    let texte = format!(
                        "{} {}",
                        t["title"].as_str().unwrap_or_default(),
                        t["detail"].as_str().unwrap_or_default()
                    )
                    .to_lowercase();
                    mots.iter().all(|m| texte.contains(m))
                })
                .take(RESULTATS_MAX)
                .collect();
            ok(&json!({ "items": trouves }))
        }
        Err(e) => indisponible("upstream", &format!("{} : {e}", service.nom())),
    }
}
