use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::avec::{dossier, Replay};
use super::galerie::{self, Annexe, Horodatage, PISTES_MAX};
use super::types::Codec;

pub const VERSION: u32 = 1;
const SEGMENTS_MAX: usize = 500;
const VOLUME_MAX: f64 = 2.0;
const AUDIO_KBPS: u32 = 160;
const LOGICIELS: [&str; 2] = ["libx264", "libopenh264"];
const NOMS_PISTES: [&str; 2] = ["Jeu", "Micro"];
const EVENEMENT_EXPORT: &str = "montage://export";
const EVENEMENT_PREPARATION: &str = "montage://preparation";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub source: String,
    pub debut: f64,
    pub fin: f64,
    pub garde: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub volume: f64,
    pub muet: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Montage {
    pub version: u32,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub volumes: BTreeMap<String, Vec<Volume>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Infos {
    pub duree_s: f64,
    pub codec: Option<Codec>,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub pistes_audio: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub ips: u32,
    pub images: Vec<(u64, u64)>,
    pub gains: Vec<(u32, f64)>,
}

impl Plan {
    pub fn duree_s(&self) -> f64 {
        self.images.iter().map(|(a, b)| b - a).sum::<u64>() as f64 / self.ips as f64
    }
}

pub fn verifier(m: &Montage, source: &str) -> Result<(), String> {
    if m.version != VERSION {
        return Err(format!("version de montage inconnue ({})", m.version));
    }
    if m.segments.is_empty() || m.segments.len() > SEGMENTS_MAX {
        return Err("montage invalide : nombre de segments hors bornes".into());
    }
    let mut precedent = 0.0;
    for s in &m.segments {
        if s.source != source {
            return Err("montage invalide : un segment vient d'un autre fichier que le clip ouvert".into());
        }
        if !(s.debut.is_finite() && s.fin.is_finite()) || s.debut < precedent || s.fin <= s.debut {
            return Err("montage invalide : segments hors bornes ou dans le désordre".into());
        }
        precedent = s.fin;
    }
    if m.volumes.keys().any(|k| k != source) || m.volumes.values().any(|v| v.len() > PISTES_MAX) {
        return Err("montage invalide : volumes d'un autre fichier que le clip ouvert".into());
    }
    if m.volumes.values().flatten().any(|v| !v.volume.is_finite() || !(0.0..=VOLUME_MAX).contains(&v.volume)) {
        return Err("montage invalide : volume hors de 0 à 200 %".into());
    }
    Ok(())
}

pub fn planifier(m: &Montage, source: &str, infos: &Infos) -> Result<Plan, String> {
    verifier(m, source)?;
    if infos.ips == 0 || infos.duree_s <= 0.0 {
        return Err("cadence ou durée du clip inconnue".into());
    }
    let ips = infos.ips as f64;
    let total = (infos.duree_s * ips).round() as u64;
    let mut images: Vec<(u64, u64)> = Vec::new();
    for s in m.segments.iter().filter(|s| s.garde) {
        if s.fin > infos.duree_s + 1.0 / ips {
            return Err("montage invalide : un segment dépasse la fin du clip".into());
        }
        let (a, b) = ((s.debut * ips).round() as u64, ((s.fin * ips).round() as u64).min(total));
        match images.last_mut() {
            _ if b <= a => {}
            Some(dernier) if dernier.1 == a => dernier.1 = b,
            _ => images.push((a, b)),
        }
    }
    if images.is_empty() {
        return Err("rien à exporter : tous les segments sont retirés".into());
    }
    let volumes = m.volumes.get(source);
    let gain = |i: u32| match volumes.and_then(|v| v.get(i as usize)) {
        Some(v) if v.muet || v.volume == 0.0 => None,
        Some(v) => Some((i, v.volume)),
        None => Some((i, 1.0)),
    };
    Ok(Plan { ips: infos.ips, images, gains: (0..infos.pistes_audio.min(PISTES_MAX as u32)).filter_map(gain).collect() })
}

pub fn graphe(p: &Plan) -> String {
    let n = p.images.len();
    let etiquettes = |prefixe: &str| (0..n).map(|i| format!("[{prefixe}{i}]")).collect::<String>();
    let mut g = vec![format!("[0:v:0]fps={}:start_time=0,split={n}{}", p.ips, etiquettes("e"))];
    g.extend(p.images.iter().enumerate().map(|(i, (a, b))| format!("[e{i}]trim=start_frame={a}:end_frame={b},setpts=PTS-STARTPTS[v{i}]")));
    let son = !p.gains.is_empty();
    if son {
        g.extend(p.gains.iter().map(|(i, v)| format!("[0:a:{i}]volume={v:.3}[p{i}]")));
        let mixe = match p.gains.as_slice() {
            [(i, _)] => format!("[p{i}]"),
            plusieurs => {
                let entrees: String = plusieurs.iter().map(|(i, _)| format!("[p{i}]")).collect();
                g.push(format!("{entrees}amix=inputs={}:normalize=0:duration=longest[mix]", plusieurs.len()));
                "[mix]".into()
            }
        };
        g.push(format!("{mixe}asplit={n}{}", etiquettes("b")));
        let ips = p.ips as f64;
        g.extend(p.images.iter().enumerate().map(|(i, (a, b))| format!("[b{i}]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS[a{i}]", *a as f64 / ips, *b as f64 / ips)));
    }
    let entrees: String = (0..n).map(|i| if son { format!("[v{i}][a{i}]") } else { format!("[v{i}]") }).collect();
    g.push(format!("{entrees}concat=n={n}:v=1:a={}[v]{}", u8::from(son), if son { "[a]" } else { "" }));
    g.join(";")
}

pub fn debit_export(taille: u64, duree_s: f64, codec: Option<Codec>) -> u32 {
    let source = taille as f64 * 8.0 / 1000.0 / duree_s.max(1.0);
    (source * if codec == Some(Codec::H264) { 1.0 } else { 1.5 }).clamp(2000.0, 50_000.0) as u32
}

fn options_video(encodeur: &str, kbps: u32, ips: u32) -> Vec<String> {
    let mut a: Vec<String> = vec!["-c:v".into(), encodeur.into(), "-b:v".into(), format!("{kbps}k"), "-maxrate".into(), format!("{kbps}k"), "-bufsize".into(), format!("{}k", kbps * 2)];
    a.extend(["-g".into(), ips.to_string(), "-pix_fmt".into(), "yuv420p".into(), "-r".into(), ips.to_string(), "-fps_mode".into(), "cfr".into()]);
    a
}

fn entree(source: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-nostats", "-progress", "pipe:1", "-i"].map(String::from).into();
    a.push(source.to_string_lossy().into_owned());
    a
}

fn sortie_mp4(mut a: Vec<String>, sortie: &Path) -> Vec<String> {
    a.extend(["-movflags", "+faststart", "-f", "mp4", "-y"].map(String::from));
    a.push(sortie.to_string_lossy().into_owned());
    a
}

pub fn arguments_export(source: &Path, sortie: &Path, p: &Plan, encodeur: &str, kbps: u32) -> Vec<String> {
    let mut a = entree(source);
    a.extend(["-filter_complex".into(), graphe(p), "-map".into(), "[v]".into()]);
    if !p.gains.is_empty() {
        a.extend(["-map", "[a]", "-c:a", "aac", "-ac", "2"].map(String::from));
        a.extend(["-b:a".into(), format!("{AUDIO_KBPS}k")]);
    }
    a.extend(options_video(encodeur, kbps, p.ips));
    sortie_mp4(a, sortie)
}

pub fn arguments_apercu(source: &Path, sortie: &Path, encodeur: &str, ips: u32) -> Vec<String> {
    let mut a = entree(source);
    a.extend(["-map", "0:v:0", "-an", "-vf"].map(String::from));
    a.push(format!("fps={ips}:start_time=0,scale=-2:min(720\\,ih)"));
    a.extend(options_video(encodeur, 4000, ips));
    sortie_mp4(a, sortie)
}

pub fn arguments_piste(source: &Path, sortie: &Path, piste: usize) -> Vec<String> {
    let mut a = entree(source);
    a.extend(["-map".into(), format!("0:a:{piste}"), "-vn".into(), "-c:a".into(), "copy".into()]);
    sortie_mp4(a, sortie)
}

pub fn analyser(texte: &str) -> Option<Infos> {
    let flux = |genre: &'static str| texte.lines().filter(move |l| l.contains("Stream #") && l.contains(genre));
    let video = flux(": Video:").find(|l| !l.contains("attached pic"))?;
    let mots: Vec<&str> = video.split([' ', ',']).filter(|m| !m.is_empty()).collect();
    let apres = |cle: &str| mots.iter().position(|m| *m == cle).and_then(|i| mots.get(i + 1)).copied();
    let avant = |cle: &str| mots.iter().position(|m| *m == cle).and_then(|i| mots.get(i.checked_sub(1)?)).and_then(|m| m.parse::<f64>().ok());
    let (largeur, hauteur) = mots.iter().find_map(|m| {
        let (l, h) = m.split_once('x')?;
        Some((l.parse::<u32>().ok()?, h.parse::<u32>().ok()?)).filter(|(l, h)| *l >= 2 && *h >= 2)
    })?;
    let codec = match apres("Video:") {
        Some("h264") => Some(Codec::H264),
        Some("hevc") => Some(Codec::Hevc),
        _ => None,
    };
    let duree_s = crate::video::parse_duration(texte);
    let ips = avant("fps").or_else(|| avant("tbr")).map_or(0, |i| i.round() as u32);
    (duree_s > 0.0 && ips > 0).then_some(Infos { duree_s, codec, largeur, hauteur, ips, pistes_audio: flux(": Audio:").count() as u32 })
}

pub fn progression(ligne: &str, duree_s: f64) -> Option<u8> {
    let micros: f64 = ligne.strip_prefix("out_time_us=")?.trim().parse().ok()?;
    Some((micros / 1e6 / duree_s.max(0.001) * 100.0).clamp(0.0, 99.0) as u8)
}

pub fn nom_montage(dossier: &Path, nom: &str) -> PathBuf {
    (1u32..)
        .map(|n| dossier.join(if n == 1 { format!("{nom} (montage).mp4") } else { format!("{nom} (montage {n}).mp4") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dossier.join(format!("{nom} (montage).mp4")))
}

pub fn disque_plein(erreur: &str) -> bool {
    ["No space left on device", "not enough space on the disk", "Disk quota exceeded"].iter().any(|m| erreur.contains(m))
}

fn derniere_ligne(erreur: &str) -> String {
    erreur.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("échec de ffmpeg").trim().to_string()
}

pub fn purger_partiels(dossier: &Path) {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES)).into_iter().flatten().flatten();
    for e in entrees.filter(|e| e.file_name().to_string_lossy().starts_with("export-") && e.path().extension().is_some_and(|x| x == "part")) {
        let _ = fs::remove_file(e.path());
    }
}

#[cfg(windows)]
fn espace_libre(dossier: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let large: Vec<u16> = dossier.as_os_str().encode_wide().chain([0]).collect();
    let mut libre = 0u64;
    let lu = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(large.as_ptr(), &mut libre, std::ptr::null_mut(), std::ptr::null_mut()) };
    (lu != 0).then_some(libre)
}

#[cfg(not(windows))]
fn espace_libre(_: &Path) -> Option<u64> {
    None
}

pub type Enfant = Arc<Mutex<Option<Child>>>;
type Ffmpeg<'a> = &'a dyn Fn() -> Result<Command, String>;

fn executer(mut commande: Command, duree_s: f64, enfant: &Enfant, annule: &AtomicBool, progres: &mut dyn FnMut(u8)) -> Result<(), String> {
    let mut processus = commande.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let (sortie, erreurs) = (processus.stdout.take(), processus.stderr.take());
    *enfant.lock().map_err(|e| e.to_string())? = Some(processus);
    if annule.load(Ordering::SeqCst) {
        arreter(enfant);
    }
    let lecteur = std::thread::spawn(move || {
        let mut octets = Vec::new();
        let _ = erreurs.map(|mut e| e.read_to_end(&mut octets));
        String::from_utf8_lossy(&octets).into_owned()
    });
    for ligne in sortie.into_iter().flat_map(|s| BufReader::new(s).lines()).map_while(Result::ok) {
        if let Some(p) = progression(&ligne, duree_s) {
            progres(p);
        }
    }
    let texte = lecteur.join().unwrap_or_default();
    let processus = enfant.lock().map_err(|e| e.to_string())?.take();
    let statut = processus.ok_or("processus ffmpeg perdu")?.wait().map_err(|e| e.to_string())?;
    if statut.success() {
        Ok(())
    } else {
        Err(if texte.trim().is_empty() { "échec de ffmpeg".into() } else { texte })
    }
}

fn arreter(enfant: &Enfant) {
    if let Ok(mut e) = enfant.lock() {
        if let Some(processus) = e.as_mut() {
            let _ = processus.kill();
        }
    }
}

fn lancer(ffmpeg: Ffmpeg, arguments: Vec<String>, duree_s: f64, enfant: &Enfant, annule: &AtomicBool, progres: &mut dyn FnMut(u8)) -> Result<(), String> {
    let mut commande = ffmpeg()?;
    commande.args(arguments);
    executer(commande, duree_s, enfant, annule, progres)
}

pub fn sonder(ffmpeg: Ffmpeg, clip: &Path) -> Result<Infos, String> {
    fs::File::open(clip).map_err(|e| format!("Clip illisible : {e}."))?;
    let sortie = ffmpeg()?.arg("-i").arg(clip).output().map_err(|e| e.to_string())?;
    analyser(&String::from_utf8_lossy(&sortie.stderr)).ok_or_else(|| "Clip illisible : ce fichier n'est pas une vidéo que l'éditeur sait ouvrir.".to_string())
}

fn modifie(chemin: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(chemin).ok().filter(|m| m.len() > 0)?.modified().ok()
}

fn a_jour(derive: &Path, source: &Path) -> bool {
    matches!((modifie(derive), modifie(source)), (Some(d), Some(s)) if d >= s)
}

fn deriver(cible: &Path, source: &Path, produire: &mut dyn FnMut(&Path) -> Result<(), String>) -> Result<(), String> {
    if a_jour(cible, source) {
        return Ok(());
    }
    let partiel = cible.with_extension("part");
    let resultat = produire(&partiel).and_then(|_| fs::rename(&partiel, cible).map_err(|e| e.to_string()));
    if resultat.is_err() {
        let _ = fs::remove_file(&partiel);
    }
    resultat
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Piste {
    pub nom: String,
    pub chemin: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ouverture {
    pub chemin: String,
    pub nom: String,
    pub source: String,
    pub duree_s: f64,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub codec: Option<Codec>,
    pub apercu: String,
    pub pistes: Vec<Piste>,
    pub projet: Option<Montage>,
}

fn nom_de(clip: &Path, extension: bool) -> String {
    if extension { clip.file_name() } else { clip.file_stem() }.map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn message_disque(erreur: String, quoi: &str) -> String {
    if disque_plein(&erreur) {
        format!("{quoi} impossible : le disque est plein.")
    } else {
        format!("{quoi} impossible : {}", derniere_ligne(&erreur))
    }
}

pub fn preparer(ffmpeg: Ffmpeg, encodeurs: &[&str], dossier: &Path, clip: &Path, progres: &mut dyn FnMut(u8)) -> Result<Ouverture, String> {
    let infos = sonder(ffmpeg, clip)?;
    galerie::creer_annexes(dossier).map_err(|e| format!("dossier des clips inaccessible : {e}"))?;
    let (enfant, jamais) = (Enfant::default(), AtomicBool::new(false));
    let mut pistes = Vec::new();
    for i in 0..(infos.pistes_audio as usize).min(PISTES_MAX) {
        let cible = galerie::annexe(dossier, clip, &format!("piste{i}.m4a"));
        deriver(&cible, clip, &mut |partiel| lancer(ffmpeg, arguments_piste(clip, partiel, i), infos.duree_s, &enfant, &jamais, &mut |_| {})).map_err(|e| message_disque(e, "Préparation du son"))?;
        pistes.push(Piste { nom: NOMS_PISTES.get(i).map_or_else(|| format!("Piste {}", i + 1), |n| n.to_string()), chemin: cible.to_string_lossy().into_owned() });
    }
    let apercu = if infos.codec == Some(Codec::H264) {
        clip.to_path_buf()
    } else {
        let cible = galerie::annexe(dossier, clip, "apercu.mp4");
        let mut produire = |partiel: &Path| {
            let mut dernier = "aucun encodeur H.264 disponible".to_string();
            for encodeur in encodeurs {
                match lancer(ffmpeg, arguments_apercu(clip, partiel, encodeur, infos.ips), infos.duree_s, &enfant, &jamais, progres) {
                    Ok(()) => return Ok(()),
                    Err(e) if disque_plein(&e) => return Err(e),
                    Err(e) => dernier = e,
                }
            }
            Err(dernier)
        };
        deriver(&cible, clip, &mut produire).map_err(|e| message_disque(e, "Aperçu"))?;
        cible
    };
    let projet = fs::read(galerie::annexe(dossier, clip, "montage.json")).ok().and_then(|o| serde_json::from_slice(&o).ok());
    Ok(Ouverture {
        chemin: clip.to_string_lossy().into_owned(),
        nom: nom_de(clip, false),
        source: nom_de(clip, true),
        duree_s: infos.duree_s,
        largeur: infos.largeur,
        hauteur: infos.hauteur,
        ips: infos.ips,
        codec: infos.codec,
        apercu: apercu.to_string_lossy().into_owned(),
        pistes,
        projet,
    })
}

pub fn enregistrer(dossier: &Path, clip: &Path, montage: &Montage) -> Result<(), String> {
    verifier(montage, &nom_de(clip, true))?;
    galerie::creer_annexes(dossier).map_err(|e| format!("dossier des clips inaccessible : {e}"))?;
    let octets = serde_json::to_vec(montage).map_err(|e| e.to_string())?;
    fs::write(galerie::annexe(dossier, clip, "montage.json"), octets).map_err(|e| format!("montage non enregistré : {e}"))
}

pub struct Demande<'a> {
    pub id: u64,
    pub dossier: &'a Path,
    pub source: &'a Path,
    pub montage: &'a Montage,
    pub quota: u64,
    pub encodeurs: &'a [&'a str],
}

#[derive(Debug, PartialEq)]
pub struct Exporte {
    pub chemin: PathBuf,
    pub logiciel: bool,
    pub quota_atteint: bool,
}

#[derive(Debug, PartialEq)]
pub enum Echec {
    Annule,
    Erreur(String),
}

impl From<String> for Echec {
    fn from(message: String) -> Self {
        Echec::Erreur(message)
    }
}

pub fn exporter(ffmpeg: Ffmpeg, d: &Demande, annule: &AtomicBool, enfant: &Enfant, progres: &mut dyn FnMut(u8, bool)) -> Result<Exporte, Echec> {
    let infos = sonder(ffmpeg, d.source)?;
    let plan = planifier(d.montage, &nom_de(d.source, true), &infos)?;
    let annexes = galerie::creer_annexes(d.dossier).map_err(|e| format!("Export impossible : dossier des clips inaccessible ({e})."))?;
    let utilise = galerie::utilise(d.dossier);
    if utilise >= d.quota {
        return Err(Echec::Erreur("Export impossible : le quota du dossier des clips est atteint. Supprime d'anciens clips puis relance l'export ; le montage est conservé.".into()));
    }
    let taille_source = fs::metadata(d.source).map_err(|e| format!("Clip illisible : {e}."))?.len();
    let kbps = debit_export(taille_source, infos.duree_s, infos.codec);
    let estimation = (u64::from(kbps + AUDIO_KBPS) * 125) as f64 * plan.duree_s();
    if espace_libre(d.dossier).is_some_and(|libre| (libre as f64) < estimation) {
        return Err(Echec::Erreur("Export impossible : il ne reste pas assez de place sur le disque. Le montage est conservé.".into()));
    }
    let partiel = annexes.join(format!("export-{}-{}.part", std::process::id(), d.id));
    let mut dernier = "aucun encodeur H.264 disponible".to_string();
    let mut reussi = None;
    for encodeur in d.encodeurs {
        if annule.load(Ordering::SeqCst) {
            break;
        }
        let logiciel = LOGICIELS.contains(encodeur);
        progres(0, logiciel);
        match lancer(ffmpeg, arguments_export(d.source, &partiel, &plan, encodeur, kbps), plan.duree_s(), enfant, annule, &mut |p| progres(p, logiciel)) {
            Ok(()) => {
                reussi = Some(logiciel);
                break;
            }
            Err(e) => {
                dernier = e;
                if disque_plein(&dernier) {
                    break;
                }
            }
        }
    }
    let abandon = |echec: Echec| {
        let _ = fs::remove_file(&partiel);
        Err(echec)
    };
    if annule.load(Ordering::SeqCst) {
        return abandon(Echec::Annule);
    }
    let Some(logiciel) = reussi else {
        return abandon(Echec::Erreur(format!("{} Le montage est conservé.", message_disque(dernier, "Export"))));
    };
    let chemin = nom_montage(d.dossier, &nom_de(d.source, false));
    if let Err(e) = fs::rename(&partiel, &chemin) {
        return abandon(Echec::Erreur(format!("Export impossible : {e}. Le montage est conservé.")));
    }
    let origine = galerie::lire_annexe(d.dossier, d.source);
    let annexe = Annexe {
        jeu: origine.map(|a| a.jeu).unwrap_or_default(),
        date: Horodatage::maintenant().iso(),
        duree_s: plan.duree_s(),
        codec: Codec::H264,
        largeur: infos.largeur,
        hauteur: infos.hauteur,
        ips: infos.ips,
        pistes_audio: u32::from(!plan.gains.is_empty()),
        origine: Some(nom_de(d.source, false)),
    };
    let _ = fs::write(galerie::annexe(d.dossier, &chemin, "json"), serde_json::to_vec(&annexe).unwrap_or_default());
    let _ = ffmpeg().and_then(|mut c| c.args(galerie::arguments_vignette(&chemin, &galerie::annexe(d.dossier, &chemin, "jpg"))).output().map_err(|e| e.to_string()));
    let taille = fs::metadata(&chemin).map_or(0, |m| m.len());
    Ok(Exporte { chemin, logiciel, quota_atteint: utilise + taille >= d.quota })
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EtatExport {
    pub id: u64,
    pub source: String,
    pub nom: String,
    pub pct: u8,
    pub etat: &'static str,
    pub message: Option<String>,
    pub chemin: Option<String>,
    pub logiciel: bool,
}

struct Tache {
    etat: EtatExport,
    annule: Arc<AtomicBool>,
    enfant: Enfant,
}

#[derive(Default)]
pub struct Exports {
    taches: Mutex<BTreeMap<u64, Tache>>,
    numero: AtomicU64,
}

impl Exports {
    pub fn en_cours(&self) -> Vec<EtatExport> {
        self.taches.lock().map(|t| t.values().map(|t| t.etat.clone()).collect()).unwrap_or_default()
    }

    fn avancer(&self, id: u64, pct: u8, logiciel: bool) -> Option<EtatExport> {
        let mut taches = self.taches.lock().ok()?;
        let etat = &mut taches.get_mut(&id)?.etat;
        if (etat.pct, etat.logiciel) == (pct, logiciel) {
            return None;
        }
        (etat.pct, etat.logiciel) = (pct, logiciel);
        Some(etat.clone())
    }

    fn annuler(&self, id: u64) -> Result<(), String> {
        let taches = self.taches.lock().map_err(|e| e.to_string())?;
        let tache = taches.get(&id).ok_or("export introuvable ou déjà terminé")?;
        tache.annule.store(true, Ordering::SeqCst);
        arreter(&tache.enfant);
        Ok(())
    }
}

fn encodeurs(app: &AppHandle) -> Vec<&'static str> {
    crate::video::encoders(app).iter().copied().chain(crate::video::software_encoders(app)).collect()
}

fn clip(app: &AppHandle, etat: &Replay, chemin: &str) -> Result<(PathBuf, PathBuf), String> {
    let dossier = dossier(app, etat)?;
    if !Path::new(chemin).exists() {
        return Err("Le fichier de ce clip est introuvable : il a été déplacé ou supprimé hors d'Estompe.".into());
    }
    let source = galerie::dans_dossier(&dossier, chemin)?;
    Ok((dossier, source))
}

#[tauri::command]
pub async fn montage_ouvrir(app: AppHandle, etat: State<'_, Replay>, chemin: String) -> Result<Ouverture, String> {
    let (dossier, source) = clip(&app, &etat, &chemin)?;
    let tache = tauri::async_runtime::spawn_blocking(move || {
        let mut dernier = None;
        let mut progres = |pct: u8| {
            if dernier != Some(pct) {
                dernier = Some(pct);
                let _ = app.emit(EVENEMENT_PREPARATION, serde_json::json!({ "chemin": chemin, "pct": pct }));
            }
        };
        preparer(&|| crate::video::command(&app), &encodeurs(&app), &dossier, &source, &mut progres)
    });
    tache.await.map_err(|e| e.to_string())?
}

#[tauri::command(async)]
pub fn montage_enregistrer(app: AppHandle, etat: State<Replay>, chemin: String, montage: Montage) -> Result<(), String> {
    let (dossier, source) = clip(&app, &etat, &chemin)?;
    enregistrer(&dossier, &source, &montage)
}

#[tauri::command(async)]
pub fn montage_exporter(app: AppHandle, etat: State<Replay>, chemin: String, montage: Montage) -> Result<EtatExport, String> {
    let (dossier, source) = clip(&app, &etat, &chemin)?;
    enregistrer(&dossier, &source, &montage)?;
    let quota = etat.reglages().quota_octets;
    let id = etat.exports.numero.fetch_add(1, Ordering::Relaxed) + 1;
    let initial = EtatExport { id, source: source.to_string_lossy().into_owned(), nom: nom_de(&source, false), pct: 0, etat: "encours", message: None, chemin: None, logiciel: false };
    let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
    let tache = Tache { etat: initial.clone(), annule: annule.clone(), enfant: enfant.clone() };
    etat.exports.taches.lock().map_err(|e| e.to_string())?.insert(id, tache);
    let _ = app.emit(EVENEMENT_EXPORT, &initial);
    let debut = initial.clone();
    std::thread::spawn(move || {
        let replay = app.state::<Replay>();
        let demande = Demande { id, dossier: &dossier, source: &source, montage: &montage, quota, encodeurs: &encodeurs(&app) };
        let mut progres = |pct: u8, logiciel: bool| {
            if let Some(e) = replay.exports.avancer(id, pct, logiciel) {
                let _ = app.emit(EVENEMENT_EXPORT, e);
            }
        };
        let resultat = exporter(&|| crate::video::command(&app), &demande, &annule, &enfant, &mut progres);
        let dernier = replay.exports.taches.lock().ok().and_then(|mut t| t.remove(&id)).map_or(debut, |t| t.etat);
        let fin = match resultat {
            Ok(e) => {
                let quota = e.quota_atteint.then_some("Quota du dossier des clips atteint : supprime d'anciens clips pour continuer à enregistrer.");
                let logiciel = e.logiciel.then_some("Encodeur matériel indisponible ou en échec : l'export a été fait par l'encodeur logiciel.");
                let message = [logiciel, quota].into_iter().flatten().collect::<Vec<_>>().join(" ");
                EtatExport { pct: 100, etat: "fini", chemin: Some(e.chemin.to_string_lossy().into_owned()), logiciel: e.logiciel, message: Some(message).filter(|m| !m.is_empty()), ..dernier }
            }
            Err(Echec::Annule) => EtatExport { etat: "annule", ..dernier },
            Err(Echec::Erreur(message)) => EtatExport { etat: "erreur", message: Some(message), ..dernier },
        };
        let _ = app.emit(EVENEMENT_EXPORT, fin);
    });
    Ok(initial)
}

#[tauri::command]
pub fn montage_annuler(etat: State<Replay>, id: u64) -> Result<(), String> {
    etat.exports.annuler(id)
}

#[tauri::command]
pub fn montage_exports(etat: State<Replay>) -> Vec<EtatExport> {
    etat.exports.en_cours()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::galerie::tests::dossier_test;

    const CLIP: &str = "Jeu.mp4";

    fn segment(debut: f64, fin: f64, garde: bool) -> Segment {
        Segment { source: CLIP.into(), debut, fin, garde }
    }

    fn montage(segments: Vec<Segment>, volumes: &[(f64, bool)]) -> Montage {
        let volumes: Vec<Volume> = volumes.iter().map(|(volume, muet)| Volume { volume: *volume, muet: *muet }).collect();
        Montage { version: VERSION, segments, volumes: if volumes.is_empty() { BTreeMap::new() } else { BTreeMap::from([(CLIP.to_string(), volumes)]) } }
    }

    fn infos(pistes_audio: u32) -> Infos {
        Infos { duree_s: 10.0, codec: Some(Codec::H264), largeur: 1280, hauteur: 720, ips: 60, pistes_audio }
    }

    fn trois_segments() -> Montage {
        montage(vec![segment(0.5, 2.0, true), segment(2.0, 3.0, false), segment(3.0, 5.0, true), segment(5.0, 7.0, false), segment(7.0, 9.5, true)], &[(0.5, false), (1.0, true)])
    }

    #[test]
    fn montage_recu_jamais_cru_sur_parole() {
        assert!(verifier(&trois_segments(), CLIP).is_ok());
        assert!(verifier(&trois_segments(), "Autre.mp4").is_err());
        assert!(verifier(&Montage { version: 2, ..trois_segments() }, CLIP).is_err());
        assert!(verifier(&montage(Vec::new(), &[]), CLIP).is_err());
        assert!(verifier(&montage(vec![segment(0.0, 1.0, true); SEGMENTS_MAX + 1], &[]), CLIP).is_err());
        for (debut, fin) in [(2.0, 1.0), (1.0, 1.0), (-1.0, 1.0), (f64::NAN, 1.0), (0.0, f64::INFINITY)] {
            assert!(verifier(&montage(vec![segment(debut, fin, true)], &[]), CLIP).is_err(), "{debut} {fin}");
        }
        assert!(verifier(&montage(vec![segment(2.0, 3.0, true), segment(0.0, 1.0, true)], &[]), CLIP).is_err());
        assert!(verifier(&montage(vec![segment(0.0, 2.0, true), segment(1.5, 3.0, true)], &[]), CLIP).is_err());
        for volume in [2.01, -0.1, f64::NAN] {
            assert!(verifier(&montage(vec![segment(0.0, 1.0, true)], &[(volume, false)]), CLIP).is_err());
        }
        let mut ailleurs = trois_segments();
        ailleurs.segments[2].source = "..\\dehors.mp4".into();
        assert!(verifier(&ailleurs, CLIP).is_err());
        let mut volumes = trois_segments();
        volumes.volumes.insert("C:\\dehors.mp4".into(), Vec::new());
        assert!(verifier(&volumes, CLIP).is_err());
        assert!(planifier(&montage(vec![segment(0.0, 11.0, true)], &[]), CLIP, &infos(1)).is_err());
        assert!(planifier(&montage(vec![segment(0.0, 5.0, false)], &[]), CLIP, &infos(1)).unwrap_err().contains("rien à exporter"));
        assert!(planifier(&trois_segments(), CLIP, &Infos { ips: 0, ..infos(2) }).is_err());
        let json = serde_json::to_value(trois_segments()).unwrap();
        assert_eq!(json["segments"][1], serde_json::json!({ "source": CLIP, "debut": 2.0, "fin": 3.0, "garde": false }));
        assert_eq!(json["volumes"][CLIP][1], serde_json::json!({ "volume": 1.0, "muet": true }));
        assert_eq!(serde_json::from_value::<Montage>(json).unwrap(), trois_segments());
    }

    #[test]
    fn plan_a_l_image_pres() {
        let p = planifier(&trois_segments(), CLIP, &infos(2)).unwrap();
        assert_eq!(p.images, [(30, 120), (180, 300), (420, 570)]);
        assert_eq!(p.gains, [(0, 0.5)]);
        assert_eq!(p.duree_s(), 6.0);
        let colles = planifier(&montage(vec![segment(0.0, 1.0, true), segment(1.0, 2.0, true), segment(2.0, 2.004, true), segment(4.0, 10.01, true)], &[]), CLIP, &infos(2)).unwrap();
        assert_eq!(colles.images, [(0, 120), (240, 600)]);
        assert_eq!(colles.gains, [(0, 1.0), (1, 1.0)]);
        assert!(planifier(&trois_segments(), CLIP, &infos(0)).unwrap().gains.is_empty());
        assert!(planifier(&montage(vec![segment(0.0, 1.0, true)], &[(0.0, false), (1.5, true)]), CLIP, &infos(2)).unwrap().gains.is_empty());
        assert_eq!(planifier(&montage(vec![segment(0.0, 1.0, true)], &[(2.0, false)]), CLIP, &infos(2)).unwrap().gains, [(0, 2.0), (1, 1.0)]);
    }

    #[test]
    fn arguments_ffmpeg_de_l_export() {
        let p = planifier(&trois_segments(), CLIP, &infos(2)).unwrap();
        let g = graphe(&p);
        assert!(g.starts_with("[0:v:0]fps=60:start_time=0,split=3[e0][e1][e2];[e0]trim=start_frame=30:end_frame=120,setpts=PTS-STARTPTS[v0];"));
        assert!(g.contains("[e2]trim=start_frame=420:end_frame=570,setpts=PTS-STARTPTS[v2]"));
        assert!(g.contains("[0:a:0]volume=0.500[p0];[p0]asplit=3[b0][b1][b2]") && !g.contains("0:a:1") && !g.contains("amix"));
        assert!(g.contains("[b1]atrim=start=3.000000:end=5.000000,asetpts=PTS-STARTPTS[a1]"));
        assert!(g.ends_with("[v0][a0][v1][a1][v2][a2]concat=n=3:v=1:a=1[v][a]"));
        let deux = graphe(&Plan { gains: vec![(0, 0.5), (1, 2.0)], ..p.clone() });
        assert!(deux.contains("[0:a:0]volume=0.500[p0];[0:a:1]volume=2.000[p1];[p0][p1]amix=inputs=2:normalize=0:duration=longest[mix];[mix]asplit=3"));
        let muet = graphe(&Plan { gains: Vec::new(), images: vec![(0, 60)], ips: 30 });
        assert_eq!(muet, "[0:v:0]fps=30:start_time=0,split=1[e0];[e0]trim=start_frame=0:end_frame=60,setpts=PTS-STARTPTS[v0];[v0]concat=n=1:v=1:a=0[v]");
        let a = arguments_export(Path::new("in.mp4"), Path::new("out.part"), &p, "h264_amf", 12_000);
        let paire = |x: &str, y: &str| a.windows(2).any(|w| w == [x, y]);
        assert!(paire("-progress", "pipe:1") && paire("-i", "in.mp4") && paire("-filter_complex", &g) && paire("-map", "[v]") && paire("-map", "[a]"));
        assert!(paire("-c:v", "h264_amf") && paire("-b:v", "12000k") && paire("-c:a", "aac") && paire("-r", "60") && paire("-fps_mode", "cfr") && paire("-pix_fmt", "yuv420p"));
        assert!(paire("-movflags", "+faststart") && paire("-f", "mp4") && a.last().unwrap() == "out.part");
        assert!(!a.iter().any(|x| x.contains("scale")));
        let sans_son = arguments_export(Path::new("i"), Path::new("o"), &Plan { gains: Vec::new(), ..p }, "libopenh264", 2000);
        assert!(!sans_son.contains(&"[a]".to_string()) && !sans_son.contains(&"-c:a".to_string()));
        assert_eq!(debit_export(75_000_000, 30.0, Some(Codec::H264)), 20_000);
        assert_eq!(debit_export(75_000_000, 30.0, Some(Codec::Hevc)), 30_000);
        assert_eq!((debit_export(1000, 30.0, None), debit_export(u64::MAX / 16, 1.0, None)), (2000, 50_000));
        let apercu = arguments_apercu(Path::new("i"), Path::new("o"), "h264_mf", 144);
        assert!(apercu.contains(&"-an".to_string()) && apercu.contains(&"fps=144:start_time=0,scale=-2:min(720\\,ih)".to_string()));
        assert!(arguments_piste(Path::new("i"), Path::new("o"), 1).windows(2).any(|w| w == ["-map", "0:a:1"]));
    }

    #[test]
    fn progression_reelle() {
        assert_eq!(progression("out_time_us=3000000", 6.0), Some(50));
        assert_eq!(progression("out_time_us=0", 6.0), Some(0));
        assert_eq!(progression("out_time_us=6000000", 6.0), Some(99));
        assert_eq!(progression("out_time_us=9000000", 6.0), Some(99));
        assert_eq!(progression("out_time_us=N/A", 6.0), None);
        assert_eq!(progression("progress=end", 6.0), None);
        assert_eq!(progression("out_time_ms=3000000", 6.0), None);
    }

    #[test]
    fn flux_lus_dans_la_sortie_de_ffmpeg() {
        let texte = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'a.mp4':\n  Duration: 00:00:10.00, start: 0.000000, bitrate: 304 kb/s\n  Stream #0:0[0x1](und): Video: hevc (Main) (hvc1 / 0x31766568), yuv420p(tv, progressive), 2560x1440 [SAR 1:1 DAR 16:9], 87 kb/s, 144 fps, 144 tbr, 15360 tbn (default)\n  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)\n  Stream #0:2[0x3](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, mono, fltp, 69 kb/s\n";
        assert_eq!(analyser(texte), Some(Infos { duree_s: 10.0, codec: Some(Codec::Hevc), largeur: 2560, hauteur: 1440, ips: 144, pistes_audio: 2 }));
        let h264 = "  Duration: 00:01:02.50, start: 0.0\n  Stream #0:0: Video: h264 (High) (avc1 / 0x31637661), yuv420p, 1920x1080, 59.94 tbr, 90k tbn\n";
        assert_eq!(analyser(h264), Some(Infos { duree_s: 62.5, codec: Some(Codec::H264), largeur: 1920, hauteur: 1080, ips: 60, pistes_audio: 0 }));
        assert_eq!(analyser("  Duration: 00:00:05.00\n  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv), 640x360, 30 fps\n").unwrap().codec, None);
        assert_eq!(analyser("a.mp4: Invalid data found when processing input"), None);
        assert_eq!(analyser("  Duration: 00:00:05.00\n  Stream #0:0: Audio: aac, 48000 Hz, stereo\n"), None);
    }

    #[test]
    fn nom_du_fichier_exporte() {
        let d = dossier_test("nom-montage");
        assert_eq!(nom_montage(&d, "Jeu 1"), d.join("Jeu 1 (montage).mp4"));
        fs::write(d.join("Jeu 1 (montage).mp4"), b"x").unwrap();
        assert_eq!(nom_montage(&d, "Jeu 1"), d.join("Jeu 1 (montage 2).mp4"));
        fs::write(d.join("Jeu 1 (montage 2).mp4"), b"x").unwrap();
        assert_eq!(nom_montage(&d, "Jeu 1"), d.join("Jeu 1 (montage 3).mp4"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn disque_plein_et_restes() {
        assert!(disque_plein("[out#0/mp4] Error writing trailer: No space left on device"));
        assert!(disque_plein("av_interleaved_write_frame(): There is not enough space on the disk"));
        assert!(!disque_plein("Unknown encoder 'h264_nvenc'"));
        assert_eq!(derniere_ligne("a\nb\n\n"), "b");
        assert_eq!(message_disque("x: No space left on device".into(), "Export"), "Export impossible : le disque est plein.");
        let d = dossier_test("partiels");
        let annexes = galerie::creer_annexes(&d).unwrap();
        for nom in ["export-1-2.part", "Jeu.part", "Jeu.mp4.json"] {
            fs::write(annexes.join(nom), b"x").unwrap();
        }
        purger_partiels(&d);
        assert!(!annexes.join("export-1-2.part").exists() && annexes.join("Jeu.part").exists() && annexes.join("Jeu.mp4.json").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn projet_enregistre_a_cote_du_clip() {
        let d = dossier_test("projet");
        let clip = d.join(CLIP);
        fs::write(&clip, b"x").unwrap();
        enregistrer(&d, &clip, &trois_segments()).unwrap();
        let lu: Montage = serde_json::from_slice(&fs::read(d.join(".estompe").join("Jeu.mp4.montage.json")).unwrap()).unwrap();
        assert_eq!(lu, trois_segments());
        assert!(enregistrer(&d, &d.join("Autre.mp4"), &trois_segments()).is_err());
        assert_eq!(fs::read(&clip).unwrap(), b"x");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suivi_des_exports() {
        let exports = Exports::default();
        let etat = EtatExport { id: 1, source: "c".into(), nom: "n".into(), pct: 0, etat: "encours", message: None, chemin: None, logiciel: false };
        let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
        exports.taches.lock().unwrap().insert(1, Tache { etat, annule: annule.clone(), enfant });
        assert_eq!(exports.avancer(1, 0, false), None);
        assert_eq!(exports.avancer(1, 40, false).unwrap().pct, 40);
        assert_eq!(exports.avancer(1, 40, false), None);
        assert!(exports.avancer(1, 0, true).unwrap().logiciel);
        assert_eq!(exports.en_cours()[0].pct, 0);
        assert_eq!(serde_json::to_value(&exports.en_cours()[0]).unwrap()["etat"], "encours");
        assert!(exports.annuler(2).is_err());
        exports.annuler(1).unwrap();
        assert!(annule.load(Ordering::SeqCst));
    }

    fn ffmpeg_local() -> Result<Command, String> {
        let mut c = Command::new("ffmpeg");
        c.args(["-hide_banner", "-nostdin"]);
        Ok(c)
    }

    fn encodeur_local() -> Option<&'static str> {
        let sortie = ffmpeg_local().ok()?.arg("-encoders").output().ok()?;
        let liste = String::from_utf8_lossy(&sortie.stdout).into_owned();
        LOGICIELS.into_iter().find(|e| liste.contains(e))
    }

    fn clip_synthetique(chemin: &Path, encodeur: &str) {
        let sources = ["testsrc=size=320x240:rate=30:duration=4", "sine=frequency=440:sample_rate=48000:duration=4", "sine=frequency=1500:sample_rate=48000:duration=4"];
        let mut c = ffmpeg_local().unwrap();
        c.args(["-v", "error"]);
        for s in sources {
            c.args(["-f", "lavfi", "-i", s]);
        }
        c.args(["-map", "0:v", "-map", "1:a", "-map", "2:a", "-c:v", encodeur, "-b:v", "500k", "-g", "30", "-pix_fmt", "yuv420p", "-c:a", "aac", "-ac:a:0", "2", "-ac:a:1", "1", "-f", "mp4", "-y"]);
        let resultat = c.arg(chemin).output().unwrap();
        assert!(resultat.status.success(), "{}", String::from_utf8_lossy(&resultat.stderr));
    }

    fn crete(chemin: &Path) -> i16 {
        let sortie = ffmpeg_local().unwrap().args(["-v", "error", "-i"]).arg(chemin).args(["-map", "0:a:0", "-ac", "1", "-f", "s16le", "-"]).output().unwrap();
        sortie.stdout.chunks_exact(2).skip(4800).map(|o| i16::from_le_bytes([o[0], o[1]]).saturating_abs()).max().unwrap_or(0)
    }

    #[test]
    fn export_reel_avec_ffmpeg() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let d = dossier_test("export-reel");
        let clip = d.join(CLIP);
        clip_synthetique(&clip, encodeur);
        let original = fs::read(&clip).unwrap();
        let mut etapes = Vec::new();
        let ouverture = preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |p| etapes.push(p)).unwrap();
        assert_eq!((ouverture.ips, ouverture.largeur, ouverture.hauteur, ouverture.codec, ouverture.pistes.len()), (30, 320, 240, Some(Codec::H264), 2));
        assert_eq!((ouverture.apercu.as_str(), ouverture.nom.as_str(), ouverture.source.as_str(), ouverture.projet.as_ref()), (clip.to_str().unwrap(), "Jeu", CLIP, None));
        assert_eq!((ouverture.pistes[0].nom.as_str(), ouverture.pistes[1].nom.as_str()), ("Jeu", "Micro"));
        let piste = sonder(&ffmpeg_local, Path::new(&ouverture.pistes[1].chemin));
        assert!(piste.is_err() && Path::new(&ouverture.pistes[1].chemin).is_file());
        let date = modifie(Path::new(&ouverture.pistes[0].chemin));
        assert_eq!(preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |_| {}).map(|o| modifie(Path::new(&o.pistes[0].chemin))), Ok(date));

        let m = montage(vec![segment(0.5, 1.0, true), segment(1.0, 1.5, false), segment(1.5, 2.5, true), segment(2.5, 3.0, false), segment(3.0, 3.5, true)], &[(0.5, false), (1.0, true)]);
        enregistrer(&d, &clip, &m).unwrap();
        assert_eq!(preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |_| {}).unwrap().projet, Some(m.clone()));
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let mut suivi = Vec::new();
        let demande = Demande { id: 1, dossier: &d, source: &clip, montage: &m, quota: 1 << 30, encodeurs: &["encodeur_absent", encodeur] };
        let e = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |p, logiciel| suivi.push((p, logiciel))).unwrap();
        assert_eq!((e.chemin.clone(), e.logiciel, e.quota_atteint), (d.join("Jeu (montage).mp4"), true, false));
        assert_eq!((suivi[0], suivi.iter().filter(|s| s.0 == 0).count() >= 2), ((0, false), true));
        assert!(suivi.windows(2).all(|w| w[0].1 != w[1].1 || w[0].0 <= w[1].0) && suivi.iter().all(|s| s.0 <= 99));
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!((infos.codec, infos.largeur, infos.hauteur, infos.ips, infos.pistes_audio), (Some(Codec::H264), 320, 240, 30, 1));
        assert!((infos.duree_s - 2.0).abs() <= 1.0 / 30.0 + 0.011, "durée {}", infos.duree_s);
        let niveau = crete(&e.chemin);
        assert!((1500..2700).contains(&niveau), "crête {niveau}");
        assert_eq!(fs::read(&clip).unwrap(), original);
        let annexe = galerie::lire_annexe(&d, &e.chemin).unwrap();
        assert_eq!((annexe.origine.as_deref(), annexe.pistes_audio, annexe.codec, annexe.duree_s), (Some("Jeu"), 1, Codec::H264, 2.0));
        assert!(galerie::annexe(&d, &e.chemin, "jpg").is_file());
        assert_eq!(galerie::lister(&d).iter().find(|c| c.nom == "Jeu (montage)").unwrap().origine.as_deref(), Some("Jeu"));

        let tout = montage(vec![segment(0.0, 4.0, true)], &[(1.0, true), (2.0, true)]);
        let muet = exporter(&ffmpeg_local, &Demande { id: 2, montage: &tout, encodeurs: &[encodeur], ..demande }, &annule, &enfant, &mut |_, _| {}).unwrap();
        assert_eq!((muet.chemin.clone(), muet.logiciel), (d.join("Jeu (montage 2).mp4"), true));
        let infos = sonder(&ffmpeg_local, &muet.chemin).unwrap();
        assert_eq!(infos.pistes_audio, 0);
        assert!((infos.duree_s - 4.0).abs() <= 1.0 / 30.0 + 0.011, "durée {}", infos.duree_s);

        let partiels = || fs::read_dir(d.join(galerie::ANNEXES)).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|x| x == "part")).count();
        let fichiers = galerie::lister(&d).len();
        annule.store(true, Ordering::SeqCst);
        assert_eq!(exporter(&ffmpeg_local, &Demande { id: 3, ..demande }, &annule, &enfant, &mut |_, _| {}), Err(Echec::Annule));
        annule.store(false, Ordering::SeqCst);
        let quota = exporter(&ffmpeg_local, &Demande { id: 4, quota: 1, ..demande }, &annule, &enfant, &mut |_, _| {});
        assert!(matches!(quota, Err(Echec::Erreur(ref m)) if m.contains("quota") && m.contains("conservé")), "{quota:?}");
        let echec = exporter(&ffmpeg_local, &Demande { id: 5, encodeurs: &["encodeur_absent"], ..demande }, &annule, &enfant, &mut |_, _| {});
        assert!(matches!(echec, Err(Echec::Erreur(ref m)) if m.starts_with("Export impossible") && m.ends_with("Le montage est conservé.")), "{echec:?}");
        assert_eq!((partiels(), galerie::lister(&d).len()), (0, fichiers));
        assert!(galerie::annexe(&d, &clip, "montage.json").is_file());

        fs::remove_file(&clip).unwrap();
        assert!(sonder(&ffmpeg_local, &clip).unwrap_err().contains("Clip illisible"));
        fs::write(&clip, b"pas une video").unwrap();
        assert!(preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |_| {}).unwrap_err().contains("Clip illisible"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn annulation_reelle_pendant_l_export() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let d = dossier_test("annulation");
        let clip = d.join(CLIP);
        clip_synthetique(&clip, encodeur);
        let m = montage(vec![segment(0.0, 4.0, true)], &[]);
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let demande = Demande { id: 1, dossier: &d, source: &clip, montage: &m, quota: 1 << 30, encodeurs: &[encodeur] };
        let resultat = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |_, _| {
            annule.store(true, Ordering::SeqCst);
            arreter(&enfant);
        });
        assert_eq!(resultat, Err(Echec::Annule));
        let restes: Vec<_> = fs::read_dir(d.join(galerie::ANNEXES)).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|x| x == "part")).collect();
        assert!(restes.is_empty() && galerie::lister(&d).len() == 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apercu_h264_d_un_clip_hevc() {
        let liste = ffmpeg_local().ok().and_then(|mut c| c.arg("-encoders").output().ok()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let Some(encodeur) = encodeur_local().filter(|_| liste.contains("libx265")) else {
            eprintln!("ignoré : ffmpeg, libx265 ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let d = dossier_test("apercu");
        let clip = d.join(CLIP);
        clip_synthetique(&clip, "libx265");
        let mut etapes = Vec::new();
        let ouverture = preparer(&ffmpeg_local, &["encodeur_absent", encodeur], &d, &clip, &mut |p| etapes.push(p)).unwrap();
        assert_eq!((ouverture.codec, ouverture.apercu.as_str()), (Some(Codec::Hevc), galerie::annexe(&d, &clip, "apercu.mp4").to_str().unwrap()));
        let apercu = sonder(&ffmpeg_local, Path::new(&ouverture.apercu)).unwrap();
        assert_eq!((apercu.codec, apercu.ips, apercu.pistes_audio, apercu.hauteur), (Some(Codec::H264), 30, 0, 240));
        assert!((apercu.duree_s - 4.0).abs() < 0.05 && !etapes.is_empty() && etapes.iter().all(|p| *p <= 99));
        etapes.clear();
        preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |p| etapes.push(p)).unwrap();
        assert!(etapes.is_empty());
        galerie::supprimer(&d, &[clip.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(fs::read_dir(d.join(galerie::ANNEXES)).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&d);
    }
}
