use std::{
    io::SeekFrom,
    path::{Path as CheminFs, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    bibliotheque::{self, PALIERS, paliers, uuid},
    config::ModeMateriel,
    droits::Droit,
    etat::AppState,
    ffprobe::Analyse,
    flux::{Acces, Limites, Mode, Ouverture, Refus, maintenant_unix},
    garde::Personne,
    http::{self, bd, erreur, introuvable, invalide, objet, ok},
    identite::{Identite, justificatif},
    sous_titres::{self, Echec},
    transcodage::{self, Etat, PLAYLIST, Parametres},
};

const ATTENTE_PLAYLIST: Duration = Duration::from_secs(40);
const ATTENTE_SEGMENT: Duration = Duration::from_secs(30);
const PAS: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Support {
    pub conteneurs: Vec<String>,
    pub video: Vec<String>,
    pub audio: Vec<String>,
}

impl Support {
    pub fn lire(v: Option<&Value>) -> Self {
        let liste = |k: &str| -> Vec<String> {
            v.and_then(|v| v[k].as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .take(32)
                        .map(|s| s.to_ascii_lowercase())
                        .collect()
                })
                .unwrap_or_default()
        };
        Support {
            conteneurs: liste("containers"),
            video: liste("video"),
            audio: liste("audio"),
        }
    }
}

pub fn famille_conteneur(conteneur: &str) -> &'static str {
    let c = conteneur.to_ascii_lowercase();
    if c.contains("mp4") || c.contains("mov") {
        "mp4"
    } else if c.contains("matroska") || c.contains("webm") {
        "mkv"
    } else if c.contains("avi") {
        "avi"
    } else {
        "autre"
    }
}

pub fn lisible_tel_quel(a: &Analyse, s: &Support, audio: usize, palier: &str) -> bool {
    let Some(v) = &a.video else { return false };
    let famille = famille_conteneur(&a.conteneur);
    let webm = famille == "mkv"
        && matches!(v.codec.as_str(), "vp8" | "vp9" | "av1")
        && a.audio
            .iter()
            .all(|x| matches!(x.codec.as_str(), "opus" | "vorbis"));
    let conteneur_ok = s.conteneurs.iter().any(|c| c == famille)
        || (webm && s.conteneurs.iter().any(|c| c == "webm"));
    let audio_ok = match a.audio.get(audio) {
        None => a.audio.is_empty(),
        Some(p) => audio == 0 && s.audio.contains(&p.codec),
    };
    palier == "original" && conteneur_ok && s.video.contains(&v.codec) && audio_ok
}

fn hauteur_palier(a: &Analyse, palier: &str) -> Result<Option<i64>, ()> {
    if palier == "original" {
        return Ok(None);
    }
    let h = PALIERS
        .iter()
        .find(|(n, _)| *n == palier)
        .map(|(_, h)| *h)
        .ok_or(())?;
    if paliers(a).contains(&palier) {
        Ok(Some(h))
    } else {
        Err(())
    }
}

fn mime_direct(chemin: &str) -> &'static str {
    match chemin
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        _ => "video/mp4",
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

fn jeton_refuse() -> Response {
    erreur(
        StatusCode::UNAUTHORIZED,
        "stream_expired",
        "flux expiré ou invalide",
    )
}

fn limites(state: &AppState) -> Limites {
    let c = state.config();
    Limites {
        flux: c.max_flux,
        transcodages: c.max_transcodages,
        inactivite: c.inactivite,
    }
}

fn charge() -> Response {
    erreur(
        StatusCode::SERVICE_UNAVAILABLE,
        "serveur_charge",
        "le serveur est trop chargé, réessayez plus tard",
    )
}

pub async fn arreter_flux(state: &AppState, ids: &[String]) {
    for id in ids {
        state.transcodeur().arreter(id).await;
    }
}

pub async fn ouvrir(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps: Map<String, Value> = objet(corps).await?;
    let audio = match corps.get("audio") {
        None | Some(Value::Null) => 0,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalide("piste audio invalide"))?,
    };
    let palier = corps
        .get("quality")
        .and_then(Value::as_str)
        .unwrap_or("original")
        .to_string();
    let debut = match corps.get("start") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v
            .as_f64()
            .filter(|d| d.is_finite() && (0.0..=86_400.0).contains(d))
            .ok_or_else(|| invalide("position de départ invalide"))?,
    };
    let support = Support::lire(corps.get("support"));
    let l = bibliotheque::ligne(&state, p.sub(), id).await?;
    let (fichier, chemin, analyse) = bibliotheque::fichier_de(&state, id)
        .await?
        .ok_or_else(introuvable)?;
    let video = analyse.video.clone().ok_or_else(introuvable)?;
    if audio > 0 && audio >= analyse.audio.len() {
        return Err(invalide("piste audio invalide"));
    }
    let hauteur = hauteur_palier(&analyse, &palier)
        .map_err(|_| invalide("qualité non proposée pour ce titre"))?;
    let mode = if lisible_tel_quel(&analyse, &support, audio, &palier) {
        Mode::Direct
    } else {
        Mode::Hls
    };
    if mode == Mode::Hls
        && state.transcodeur().taille().await > state.config().transcodage_max_octets
    {
        return Err(charge());
    }
    let ouverture = Ouverture {
        sub: p.sub().to_string(),
        titre: id,
        fichier,
        mode,
    };
    let (flux_id, signe, morts) = match state.flux().ouvrir(
        ouverture,
        limites(&state),
        &state.config().secret_flux,
        Instant::now(),
        maintenant_unix(),
    ) {
        Ok(o) => o,
        Err(Refus::TropDeFlux) => {
            return Err(erreur(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_streams",
                "trop de lectures en cours sur ce compte",
            ));
        }
        Err(Refus::ServeurCharge) => return Err(charge()),
    };
    arreter_flux(&state, &morts).await;
    let base = format!("/stream/{signe}");
    let url = if mode == Mode::Hls {
        let params = Parametres {
            source: PathBuf::from(&chemin),
            index_video: video.index,
            index_audio: analyse.audio.get(audio).map(|a| a.index),
            hauteur,
            copie_video: hauteur.is_none()
                && video.codec == "h264"
                && support.video.iter().any(|c| c == "h264"),
            debut,
            mode: state.materiel().mode(),
            peripherique: state.config().peripherique.clone(),
        };
        if let Err(e) = state
            .transcodeur()
            .demarrer(&state.config().ffmpeg, &flux_id, &params)
            .await
        {
            tracing::error!("transcodage : {e}");
            state.flux().fermer(&flux_id, p.sub());
            return Err(http::indisponible(
                "transcode_failed",
                "transcodage impossible",
            ));
        }
        format!("{base}/hls.m3u8")
    } else {
        format!("{base}/direct")
    };
    let pistes = bibliotheque::pistes(&analyse);
    let sous_titres: Vec<Value> = pistes["subtitles"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut s| {
            s["url"] = json!(format!("{base}/sub/{}", s["n"]));
            s
        })
        .collect();
    Ok(ok(&json!({
        "stream_id": flux_id,
        "mode": if mode == Mode::Hls { "hls" } else { "direct" },
        "url": url,
        "start": debut,
        "offset": if mode == Mode::Hls { debut } else { 0.0 },
        "duration": l.get::<Option<f64>, _>("duree").or(analyse.duree),
        "quality": palier,
        "qualities": pistes["qualities"],
        "audio": pistes["audio"],
        "audio_n": audio,
        "subtitles": sous_titres,
    })))
}

use sqlx::Row;

pub async fn fermer(
    State(state): State<AppState>,
    p: Personne,
    Path(flux): Path<String>,
) -> Result<Response, Response> {
    match state.flux().fermer(&flux, p.sub()) {
        None => Err(introuvable()),
        Some(_) => {
            state.transcodeur().arreter(&flux).await;
            Ok(http::sans_contenu())
        }
    }
}

async fn acces(state: &AppState, jeton: &str, entetes: &HeaderMap) -> Result<Acces, Response> {
    let a = state
        .flux()
        .valider(
            jeton,
            &state.config().secret_flux,
            Instant::now(),
            maintenant_unix(),
        )
        .ok_or_else(jeton_refuse)?;
    if let Some(j) = justificatif(entetes)
        && let Ok(id) = state.identites().resoudre(Some(&j)).await
        && id.sub != a.ouverture.sub
    {
        return Err(erreur(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce flux appartient à une autre personne",
        ));
    }
    let identite = Identite {
        sub: a.ouverture.sub.clone(),
        nom: String::new(),
        email: String::new(),
    };
    match state.droits_de(&identite).await {
        Ok(d) if d.a(Droit::Acces) => Ok(a),
        Ok(_) => Err(crate::garde::interdit(Droit::Acces)),
        Err(_) => Err(erreur(
            StatusCode::SERVICE_UNAVAILABLE,
            "aegis_unavailable",
            "service de droits injoignable",
        )),
    }
}

async fn fichier(state: &AppState, id: Uuid) -> Result<(String, Analyse), Response> {
    let l = sqlx::query("SELECT chemin, infos FROM fichiers WHERE id = $1")
        .bind(id)
        .fetch_optional(state.pool())
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let a: sqlx::types::Json<Analyse> = l.get(1);
    Ok((l.get(0), a.0))
}

fn plage(entetes: &HeaderMap, taille: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(v) = entetes.get(header::RANGE) else {
        return Ok(None);
    };
    let texte = v.to_str().map_err(|_| ())?;
    let (debut, fin) = texte
        .strip_prefix("bytes=")
        .ok_or(())?
        .split_once('-')
        .ok_or(())?;
    if debut.contains(',') || fin.contains(',') || taille == 0 {
        return Err(());
    }
    let nombre = |s: &str| s.trim().parse::<u64>().map_err(|_| ());
    let (a, b) = match (debut.trim().is_empty(), fin.trim().is_empty()) {
        (true, true) => return Err(()),
        (true, false) => {
            let n = nombre(fin)?.min(taille);
            if n == 0 {
                return Err(());
            }
            (taille - n, taille - 1)
        }
        (false, true) => (nombre(debut)?, taille - 1),
        (false, false) => (nombre(debut)?, nombre(fin)?.min(taille - 1)),
    };
    if a > b || a >= taille {
        return Err(());
    }
    Ok(Some((a, b)))
}

pub async fn direct(
    State(state): State<AppState>,
    Path(jeton): Path<String>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Direct {
        return Err(jeton_refuse());
    }
    let (chemin, _) = fichier(&state, a.ouverture.fichier).await?;
    let mut f = tokio::fs::File::open(&chemin).await.map_err(|e| {
        tracing::warn!("lecture directe : {e}");
        introuvable()
    })?;
    let taille = f.metadata().await.map_err(|_| introuvable())?.len();
    let mut r = Response::new(Body::empty());
    let (debut, longueur) = match plage(&entetes, taille) {
        Err(()) => {
            let mut r = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            if let Ok(v) = HeaderValue::from_str(&format!("bytes */{taille}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            return Ok(r);
        }
        Ok(None) => (0, taille),
        Ok(Some((a, b))) => {
            *r.status_mut() = StatusCode::PARTIAL_CONTENT;
            if let Ok(v) = HeaderValue::from_str(&format!("bytes {a}-{b}/{taille}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            (a, b - a + 1)
        }
    };
    f.seek(SeekFrom::Start(debut))
        .await
        .map_err(|_| http::interne())?;
    entetes_de_flux(&mut r, mime_direct(&chemin));
    let h = r.headers_mut();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(longueur));
    let garde = state.flux().lecture(&a.flux);
    let flux = ReaderStream::new(f.take(longueur));
    *r.body_mut() = Body::from_stream(futures_util::StreamExt::map(flux, move |m| {
        let _ = &garde;
        m
    }));
    Ok(r)
}

async fn relancer_sur_cpu(state: &AppState, flux: &str) -> bool {
    let Some(mut p) = state.transcodeur().parametres(flux) else {
        return false;
    };
    if p.mode == ModeMateriel::Cpu || p.copie_video {
        return false;
    }
    state
        .materiel()
        .signaler("ffmpeg a échoué au démarrage d'un transcodage, relancé sur le CPU");
    p.mode = ModeMateriel::Cpu;
    state
        .transcodeur()
        .demarrer(&state.config().ffmpeg, flux, &p)
        .await
        .is_ok()
}

pub fn playlist_propre(brut: &str) -> String {
    let mut sortie = String::new();
    for ligne in brut.lines() {
        let l = ligne.trim_end_matches('\r');
        if l.starts_with('#') {
            if l.starts_with("#EXT") {
                sortie.push_str(l);
                sortie.push('\n');
            }
            continue;
        }
        let nom = l.rsplit(['/', '\\']).next().unwrap_or_default();
        if transcodage::segment_valide(nom) {
            sortie.push_str(nom);
            sortie.push('\n');
        }
    }
    sortie
}

pub async fn hls(
    State(state): State<AppState>,
    Path(jeton): Path<String>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Hls {
        return Err(jeton_refuse());
    }
    let _garde = state.flux().lecture(&a.flux);
    let limite = Instant::now() + ATTENTE_PLAYLIST;
    let mut relance = false;
    loop {
        if let Some(d) = state.transcodeur().dossier(&a.flux)
            && let Ok(texte) = tokio::fs::read_to_string(d.join(PLAYLIST)).await
            && texte.contains("#EXTINF")
        {
            let mut r = Response::new(Body::from(playlist_propre(&texte)));
            entetes_de_flux(&mut r, "application/vnd.apple.mpegurl");
            return Ok(r);
        }
        match state.transcodeur().etat(&a.flux) {
            Etat::EnCours => {}
            Etat::Echec if !relance && relancer_sur_cpu(&state, &a.flux).await => relance = true,
            _ => {
                return Err(http::indisponible(
                    "transcode_failed",
                    "le transcodage a échoué",
                ));
            }
        }
        if Instant::now() > limite {
            return Err(http::indisponible(
                "transcode_timeout",
                "le transcodage ne démarre pas",
            ));
        }
        tokio::time::sleep(PAS).await;
    }
}

pub async fn segment(
    State(state): State<AppState>,
    Path((jeton, nom)): Path<(String, String)>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    if !transcodage::segment_valide(&nom) {
        return Err(introuvable());
    }
    let a = acces(&state, &jeton, &entetes).await?;
    if a.ouverture.mode != Mode::Hls {
        return Err(jeton_refuse());
    }
    let _garde = state.flux().lecture(&a.flux);
    let limite = Instant::now() + ATTENTE_SEGMENT;
    loop {
        let dossier = state
            .transcodeur()
            .dossier(&a.flux)
            .ok_or_else(introuvable)?;
        if let Ok(octets) = tokio::fs::read(dossier.join(&nom)).await {
            let mut r = Response::new(Body::from(octets));
            entetes_de_flux(&mut r, "video/mp2t");
            return Ok(r);
        }
        if state.transcodeur().etat(&a.flux) != Etat::EnCours || Instant::now() > limite {
            return Err(introuvable());
        }
        tokio::time::sleep(PAS).await;
    }
}

async fn extraire_vtt(
    ffmpeg: &str,
    source: &CheminFs,
    index: Option<i64>,
) -> Result<String, String> {
    let mut c = tokio::process::Command::new(ffmpeg);
    c.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(source);
    if let Some(i) = index {
        c.args(["-map", &format!("0:{i}")]);
    }
    c.args(["-f", "webvtt", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut enfant = c.spawn().map_err(|e| format!("ffmpeg : {e}"))?;
    let mut sortie = enfant.stdout.take().ok_or("ffmpeg sans sortie")?;
    let lecture = async {
        let mut tampon = Vec::new();
        let mut morceau = [0u8; 65536];
        loop {
            let n = sortie.read(&mut morceau).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            tampon.extend_from_slice(&morceau[..n]);
            if tampon.len() > sous_titres::TAILLE_MAX {
                return Err("sous-titres trop gros".to_string());
            }
        }
        Ok(tampon)
    };
    let tampon = tokio::time::timeout(Duration::from_secs(600), lecture)
        .await
        .map_err(|_| "extraction trop longue".to_string())??;
    let statut = enfant.wait().await.map_err(|e| e.to_string())?;
    if !statut.success() {
        return Err("extraction refusée par ffmpeg".into());
    }
    Ok(String::from_utf8_lossy(&tampon).into_owned())
}

pub async fn sous_titre(
    State(state): State<AppState>,
    Path((jeton, n)): Path<(String, usize)>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let a = acces(&state, &jeton, &entetes).await?;
    let fichier_id = a.ouverture.fichier;
    let cle = n.to_string();
    let en_cache: Option<String> =
        sqlx::query_scalar("SELECT vtt FROM sous_titres WHERE fichier_id = $1 AND piste = $2")
            .bind(fichier_id)
            .bind(&cle)
            .fetch_optional(state.pool())
            .await
            .map_err(bd)?;
    let vtt = match en_cache {
        Some(v) => v,
        None => {
            let (chemin, analyse) = fichier(&state, fichier_id).await?;
            let piste = analyse.sous_titres.get(n).ok_or_else(introuvable)?;
            let (source, index) = match &piste.externe {
                Some(e) => (PathBuf::from(e), None),
                None => (PathBuf::from(chemin), piste.index),
            };
            let (etat, piste_cle) = (&state, cle.clone());
            state
                .extractions()
                .obtenir(fichier_id, &piste_cle, || async move {
                    let requete = "SELECT vtt FROM sous_titres WHERE fichier_id = $1 AND piste = $2";
                    let deja: Option<String> = sqlx::query_scalar(requete)
                        .bind(fichier_id)
                        .bind(&cle)
                        .fetch_optional(etat.pool())
                        .await
                        .map_err(|e| Echec::Base(e.to_string()))?;
                    if let Some(v) = deja {
                        return Ok(v);
                    }
                    let brut = extraire_vtt(&etat.config().ffmpeg, &source, index)
                        .await
                        .map_err(Echec::Extraction)?;
                    let propre = sous_titres::nettoyer(&brut);
                    sqlx::query("INSERT INTO sous_titres (fichier_id, piste, vtt) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                        .bind(fichier_id)
                        .bind(&cle)
                        .bind(&propre)
                        .execute(etat.pool())
                        .await
                        .map_err(|e| Echec::Base(e.to_string()))?;
                    Ok(propre)
                })
                .await
                .map_err(|e| match e {
                    Echec::Extraction(e) => {
                        tracing::warn!("sous-titres : {e}");
                        http::indisponible("subtitles_failed", "sous-titres illisibles")
                    }
                    Echec::Base(e) => {
                        tracing::warn!("base : {e}");
                        http::interne()
                    }
                })?
        }
    };
    let mut r = Response::new(Body::from(vtt));
    entetes_de_flux(&mut r, "text/vtt; charset=utf-8");
    Ok(r)
}

pub async fn menage(state: &AppState) {
    let mortes: Vec<String> = state
        .flux()
        .purger(Instant::now())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    arreter_flux(state, &mortes).await;
    for id in state.transcodeur().identifiants() {
        if !state.flux().existe(&id) {
            state.transcodeur().arreter(&id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffprobe::{Audio, Video};

    fn analyse(conteneur: &str, video: &str, audios: &[&str], hauteur: i64) -> Analyse {
        Analyse {
            conteneur: conteneur.into(),
            duree: Some(100.0),
            video: Some(Video {
                index: 0,
                codec: video.into(),
                largeur: hauteur * 16 / 9,
                hauteur,
            }),
            audio: audios
                .iter()
                .enumerate()
                .map(|(i, c)| Audio {
                    index: i as i64 + 1,
                    codec: c.to_string(),
                    langue: None,
                    titre: None,
                    canaux: 2,
                })
                .collect(),
            sous_titres: vec![],
        }
    }

    fn navigateur() -> Support {
        Support {
            conteneurs: vec!["mp4".into(), "webm".into()],
            video: vec!["h264".into(), "vp9".into(), "av1".into()],
            audio: vec!["aac".into(), "mp3".into(), "opus".into()],
        }
    }

    #[test]
    fn decision_directe_ou_hls() {
        let s = navigateur();
        let mp4 = analyse("mov,mp4,m4a,3gp,3g2,mj2", "h264", &["aac", "ac3"], 1080);
        assert!(lisible_tel_quel(&mp4, &s, 0, "original"));
        assert!(!lisible_tel_quel(&mp4, &s, 1, "original"));
        assert!(!lisible_tel_quel(&mp4, &s, 0, "720p"));
        assert!(!lisible_tel_quel(&mp4, &Support::default(), 0, "original"));
        assert!(!lisible_tel_quel(
            &analyse("matroska,webm", "hevc", &["aac"], 2160),
            &s,
            0,
            "original"
        ));
        assert!(!lisible_tel_quel(
            &analyse("matroska,webm", "h264", &["aac"], 1080),
            &s,
            0,
            "original"
        ));
        assert!(lisible_tel_quel(
            &analyse("matroska,webm", "vp9", &["opus"], 1080),
            &s,
            0,
            "original"
        ));
        assert!(!lisible_tel_quel(
            &analyse("mov,mp4", "h264", &["eac3"], 1080),
            &s,
            0,
            "original"
        ));
    }

    #[test]
    fn paliers_bornes_par_la_source() {
        let a = analyse("mp4", "h264", &["aac"], 720);
        assert_eq!(paliers(&a), ["original", "480p"]);
        assert_eq!(hauteur_palier(&a, "original"), Ok(None));
        assert_eq!(hauteur_palier(&a, "480p"), Ok(Some(480)));
        assert!(hauteur_palier(&a, "720p").is_err());
        assert!(hauteur_palier(&a, "1080p").is_err());
        assert!(hauteur_palier(&a, "4k").is_err());
        assert_eq!(
            paliers(&analyse("mp4", "h264", &[], 2160)),
            ["original", "1080p", "720p", "480p"]
        );
    }

    #[test]
    fn plages() {
        let h = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::RANGE, v.parse().unwrap());
            h
        };
        assert_eq!(plage(&HeaderMap::new(), 100), Ok(None));
        assert_eq!(plage(&h("bytes=0-9"), 100), Ok(Some((0, 9))));
        assert_eq!(plage(&h("bytes=90-"), 100), Ok(Some((90, 99))));
        assert_eq!(plage(&h("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(plage(&h("bytes=50-500"), 100), Ok(Some((50, 99))));
        for faux in [
            "bytes=100-",
            "bytes=5-1",
            "bytes=-0",
            "octets=0-1",
            "bytes=0-1,3-4",
            "bytes=-",
        ] {
            assert_eq!(plage(&h(faux), 100), Err(()), "{faux}");
        }
    }

    #[test]
    fn playlist_sans_chemin() {
        let p = playlist_propre(
            "#EXTM3U\n# commentaire /secret\n#EXTINF:4.0,\n/tmp/entracte/abc/seg00000.ts\n#EXTINF:4.0,\nseg00001.ts\n../../etc/passwd\n",
        );
        assert_eq!(
            p,
            "#EXTM3U\n#EXTINF:4.0,\nseg00000.ts\n#EXTINF:4.0,\nseg00001.ts\n"
        );
    }
}
