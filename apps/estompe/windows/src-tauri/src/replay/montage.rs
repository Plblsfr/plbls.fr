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

pub const VERSION: u32 = 2;
pub const CLIPS_MAX: usize = 20;
pub const DUREE_MAX_S: f64 = 1800.0;
const SEGMENTS_MAX: usize = 500;
const TEXTES_MAX: usize = 50;
const TEXTE_MAX: usize = 200;
const NOM_MAX: usize = 80;
const IMAGE_MAX: usize = 16 << 20;
const ARGUMENTS_MAX: usize = 30_000;
const VOLUME_MAX: f64 = 2.0;
const AUDIO_KBPS: u32 = 160;
const VITESSES: [f64; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 4.0];
const RAPPORTS: [&str; 5] = ["libre", "16:9", "1:1", "9:16", "4:5"];
const POLICES: [&str; 3] = ["Bricolage Grotesque", "Instrument Sans", "JetBrains Mono"];
const HABILLAGES: [&str; 3] = ["aucun", "contour", "fond"];
const FORMAT_SON: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";
const LOGICIELS: [&str; 2] = ["libx264", "libopenh264"];
const NOMS_PISTES: [&str; 2] = ["Jeu", "Micro"];
const PROJETS: &str = "projets";
const ANCIEN: &str = ".montage.json";
const EVENEMENT_EXPORT: &str = "montage://export";
const EVENEMENT_PREPARATION: &str = "montage://preparation";

fn un() -> f64 {
    1.0
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub source: String,
    pub debut: f64,
    pub fin: f64,
    pub garde: bool,
    #[serde(default = "un")]
    pub vitesse: f64,
    #[serde(default)]
    pub muet: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub volume: f64,
    pub muet: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Texte {
    pub contenu: String,
    pub police: String,
    pub taille: f64,
    pub couleur: String,
    pub habillage: String,
    pub x: f64,
    pub y: f64,
    pub debut: f64,
    pub fin: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cadre {
    pub x: f64,
    pub y: f64,
    pub l: f64,
    pub h: f64,
    pub rapport: String,
}

impl Default for Cadre {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, l: 1.0, h: 1.0, rapport: RAPPORTS[0].into() }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sortie {
    pub hauteur: Option<u32>,
    pub ips: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Montage {
    pub version: u32,
    #[serde(default)]
    pub nom: String,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub volumes: BTreeMap<String, Vec<Volume>>,
    #[serde(default)]
    pub textes: Vec<Texte>,
    #[serde(default)]
    pub cadre: Cadre,
    #[serde(default)]
    pub sortie: Sortie,
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

type Boite = (u32, u32, u32, u32);

#[derive(Clone, Debug, PartialEq)]
pub struct Entree {
    pub source: String,
    pub ips: u32,
    pub echelle: Option<(u32, u32)>,
    pub marge: Option<(u32, u32)>,
    pub gains: Vec<(u32, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Morceau {
    pub entree: usize,
    pub images: (u64, u64),
    pub vitesse: f64,
    pub son: bool,
    pub sortie: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub ips: u32,
    pub largeur: u32,
    pub hauteur: u32,
    pub toile: (u32, u32),
    pub recadre: Option<Boite>,
    pub entrees: Vec<Entree>,
    pub morceaux: Vec<Morceau>,
    pub textes: Vec<Option<(u64, u64)>>,
}

impl Plan {
    pub fn images(&self) -> u64 {
        self.morceaux.iter().map(|m| m.sortie).sum()
    }

    pub fn duree_s(&self) -> f64 {
        self.images() as f64 / self.ips as f64
    }
}

fn pair(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

pub fn toile(largeur: u32, hauteur: u32) -> (u32, u32) {
    ((largeur & !1).max(2), (hauteur & !1).max(2))
}

pub fn inscrire(largeur: u32, hauteur: u32, toile: (u32, u32)) -> Boite {
    let echelle = (toile.0 as f64 / largeur.max(1) as f64).min(toile.1 as f64 / hauteur.max(1) as f64);
    let (l, h) = (pair(largeur as f64 * echelle).min(toile.0), pair(hauteur as f64 * echelle).min(toile.1));
    (l, h, (toile.0 - l) / 4 * 2, (toile.1 - h) / 4 * 2)
}

pub fn cadre_pixels(c: &Cadre, toile: (u32, u32)) -> Boite {
    let (l, h) = (pair(c.l * toile.0 as f64).min(toile.0), pair(c.h * toile.1 as f64).min(toile.1));
    let coin = |f: f64, total: u32, taille: u32| ((f * total as f64 / 2.0).round().max(0.0) as u32 * 2).min(total - taille);
    (l, h, coin(c.x, toile.0, l), coin(c.y, toile.1, h))
}

fn rapport(r: &str) -> Option<(f64, f64)> {
    let (a, b) = r.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

pub fn dimensions_sortie(cadre: (u32, u32), forme: &str, hauteur: Option<u32>) -> (u32, u32) {
    let (l, h) = (cadre.0 as f64, cadre.1 as f64);
    let voulue = hauteur.map_or(h, |x| (x as f64).min(h));
    match rapport(forme) {
        Some((a, b)) => {
            let k = (((l / a).min(h / b).min(voulue / b) / 2.0).floor() * 2.0).max(2.0);
            ((a * k) as u32, (b * k) as u32)
        }
        None => {
            let haut = pair(voulue).min(cadre.1);
            (if haut == cadre.1 { cadre.0 } else { pair(l * haut as f64 / h).min(cadre.0) }, haut)
        }
    }
}

fn source_valide(nom: &str) -> bool {
    nom.len() > 4 && nom.to_ascii_lowercase().ends_with(".mp4") && Path::new(nom).file_name().is_some_and(|n| n == nom) && !nom.contains(['/', '\\', ':'])
}

fn fraction(x: f64) -> bool {
    x.is_finite() && (0.0..=1.0).contains(&x)
}

fn texte_valide(t: &Texte) -> bool {
    let couleur = t.couleur.len() == 7 && t.couleur.starts_with('#') && t.couleur[1..].chars().all(|c| c.is_ascii_hexdigit());
    !t.contenu.trim().is_empty() && t.contenu.chars().count() <= TEXTE_MAX && POLICES.contains(&t.police.as_str()) && HABILLAGES.contains(&t.habillage.as_str()) && couleur
        && fraction(t.x) && fraction(t.y) && t.taille.is_finite() && (0.01..=0.5).contains(&t.taille) && t.debut.is_finite() && t.fin.is_finite() && t.debut >= 0.0 && t.fin > t.debut
}

pub fn sources(m: &Montage) -> Vec<&str> {
    let mut vues: Vec<&str> = Vec::new();
    for s in &m.segments {
        if !vues.contains(&s.source.as_str()) {
            vues.push(&s.source);
        }
    }
    vues
}

pub fn duree_finale(m: &Montage) -> f64 {
    m.segments.iter().filter(|s| s.garde).map(|s| (s.fin - s.debut) / s.vitesse).sum()
}

pub fn verifier(m: &Montage) -> Result<(), String> {
    if m.version != VERSION {
        return Err(format!("version de montage inconnue ({})", m.version));
    }
    if m.segments.is_empty() || m.segments.len() > SEGMENTS_MAX {
        return Err("montage invalide : nombre de segments hors bornes".into());
    }
    if m.nom.chars().count() > NOM_MAX || m.nom.chars().any(char::is_control) {
        return Err("montage invalide : nom du projet".into());
    }
    for s in &m.segments {
        if !source_valide(&s.source) {
            return Err("montage invalide : un segment désigne un fichier hors du dossier des clips".into());
        }
        if !(s.debut.is_finite() && s.fin.is_finite()) || s.debut < 0.0 || s.fin <= s.debut {
            return Err("montage invalide : segment hors bornes".into());
        }
        if !VITESSES.contains(&s.vitesse) {
            return Err("montage invalide : vitesse inconnue".into());
        }
    }
    let clips = sources(m);
    if clips.len() > CLIPS_MAX {
        return Err(format!("Un montage réunit {CLIPS_MAX} clips au plus."));
    }
    if duree_finale(m) > DUREE_MAX_S + 0.5 {
        return Err(format!("Un montage dure {} minutes au plus.", DUREE_MAX_S as u32 / 60));
    }
    if m.volumes.keys().any(|k| !clips.contains(&k.as_str())) || m.volumes.values().any(|v| v.len() > PISTES_MAX) {
        return Err("montage invalide : volumes d'un fichier absent du montage".into());
    }
    if m.volumes.values().flatten().any(|v| !v.volume.is_finite() || !(0.0..=VOLUME_MAX).contains(&v.volume)) {
        return Err("montage invalide : volume hors de 0 à 200 %".into());
    }
    if m.textes.len() > TEXTES_MAX || !m.textes.iter().all(texte_valide) {
        return Err("montage invalide : bloc de texte".into());
    }
    let c = &m.cadre;
    if ![c.x, c.y, c.l, c.h].into_iter().all(fraction) || c.l <= 0.0 || c.h <= 0.0 || c.x + c.l > 1.0001 || c.y + c.h > 1.0001 || !RAPPORTS.contains(&c.rapport.as_str()) {
        return Err("montage invalide : cadre hors de l'image".into());
    }
    if m.sortie.hauteur.is_some_and(|h| !(2..=4320).contains(&h)) || m.sortie.ips.is_some_and(|i| !(1..=240).contains(&i)) {
        return Err("montage invalide : résolution ou cadence de sortie".into());
    }
    Ok(())
}

pub fn planifier(m: &Montage, infos: &BTreeMap<String, Infos>) -> Result<Plan, String> {
    verifier(m)?;
    let de = |nom: &str| infos.get(nom).filter(|i| i.ips > 0 && i.duree_s > 0.0).ok_or_else(|| format!("cadence ou durée du clip « {nom} » inconnue"));
    let premier = de(&m.segments[0].source)?;
    let toile = toile(premier.largeur, premier.hauteur);
    let cadre = cadre_pixels(&m.cadre, toile);
    let (largeur, hauteur) = dimensions_sortie((cadre.0, cadre.1), &m.cadre.rapport, m.sortie.hauteur);
    let mut maximum = 0;
    for nom in sources(m) {
        maximum = maximum.max(de(nom)?.ips);
    }
    let ips = m.sortie.ips.unwrap_or(premier.ips).min(maximum);
    let mut entrees: Vec<Entree> = Vec::new();
    let mut atteint: Vec<u64> = Vec::new();
    let mut morceaux: Vec<Morceau> = Vec::new();
    let (mut temps, mut faites) = (0.0, 0u64);
    for s in m.segments.iter().filter(|s| s.garde) {
        let i = de(&s.source)?;
        let cadence = i.ips as f64;
        if s.fin > i.duree_s + 1.0 / cadence {
            return Err("montage invalide : un segment dépasse la fin du clip".into());
        }
        let (a, b) = ((s.debut * cadence).round() as u64, ((s.fin * cadence).round() as u64).min((i.duree_s * cadence).round() as u64));
        if b <= a {
            continue;
        }
        temps += (b - a) as f64 / cadence / s.vitesse;
        let sortie = ((temps * ips as f64).round() as u64).saturating_sub(faites);
        faites += sortie;
        if sortie == 0 {
            continue;
        }
        let volumes = m.volumes.get(&s.source);
        let gain = |p: u32| match volumes.and_then(|v| v.get(p as usize)) {
            Some(v) if v.muet || v.volume == 0.0 => None,
            Some(v) => Some((p, v.volume)),
            None => Some((p, 1.0)),
        };
        let gains: Vec<(u32, f64)> = (0..i.pistes_audio.min(PISTES_MAX as u32)).filter_map(gain).collect();
        let son = !s.muet && !gains.is_empty();
        if let Some(dernier) = morceaux.last_mut().filter(|d| entrees[d.entree].source == s.source && d.images.1 == a && d.vitesse == s.vitesse && d.son == son) {
            dernier.images.1 = b;
            dernier.sortie += sortie;
            atteint[dernier.entree] = b;
            continue;
        }
        let entree = match (0..entrees.len()).find(|e| entrees[*e].source == s.source && atteint[*e] <= a) {
            Some(e) => e,
            None => {
                let (l, h, x, y) = inscrire(i.largeur, i.hauteur, toile);
                let echelle = Some((l, h)).filter(|e| *e != (i.largeur, i.hauteur));
                entrees.push(Entree { source: s.source.clone(), ips: i.ips, echelle, marge: ((l, h) != toile).then_some((x, y)), gains });
                atteint.push(0);
                entrees.len() - 1
            }
        };
        atteint[entree] = b;
        morceaux.push(Morceau { entree, images: (a, b), vitesse: s.vitesse, son, sortie });
    }
    if morceaux.is_empty() {
        return Err("rien à exporter : tous les segments sont retirés".into());
    }
    let total: u64 = morceaux.iter().map(|x| x.sortie).sum();
    let image = |t: f64| ((t * ips as f64).round() as u64).min(total);
    let textes = m.textes.iter().map(|t| Some((image(t.debut), image(t.fin))).filter(|(a, b)| b > a)).collect();
    Ok(Plan { ips, largeur, hauteur, toile, recadre: Some(cadre).filter(|c| (c.0, c.1) != toile), entrees, morceaux, textes })
}

fn tempo(vitesse: f64) -> String {
    if vitesse == 1.0 {
        String::new()
    } else if vitesse < 0.5 {
        format!(",atempo=0.5,atempo={}", vitesse / 0.5)
    } else if vitesse > 2.0 {
        format!(",atempo=2,atempo={}", vitesse / 2.0)
    } else {
        format!(",atempo={vitesse}")
    }
}

pub fn graphe(p: &Plan) -> String {
    let n = p.morceaux.len();
    let de = |e: usize| p.morceaux.iter().enumerate().filter(move |(_, m)| m.entree == e);
    let etiquettes = |e: usize, prefixe: &str, son: bool| de(e).filter(|(_, m)| !son || m.son).map(|(i, _)| format!("[{prefixe}{i}]")).collect::<String>();
    let mut g: Vec<String> = Vec::new();
    for (e, entree) in p.entrees.iter().enumerate() {
        g.push(format!("[{e}:v:0]fps={}:start_time=0,split={}{}", entree.ips, de(e).count(), etiquettes(e, "e", false)));
    }
    for (i, m) in p.morceaux.iter().enumerate() {
        let entree = &p.entrees[m.entree];
        let mut f = format!("[e{i}]trim=start_frame={}:end_frame={},setpts=(PTS-STARTPTS)/{},fps={},tpad=stop_mode=clone:stop=-1,trim=end_frame={}", m.images.0, m.images.1, m.vitesse, p.ips, m.sortie);
        if let Some((l, h)) = entree.echelle {
            f += &format!(",scale={l}:{h}");
        }
        if let Some((x, y)) = entree.marge {
            f += &format!(",pad={}:{}:{x}:{y}", p.toile.0, p.toile.1);
        }
        let avant = p.recadre.map_or(p.toile, |(l, h, _, _)| (l, h));
        if let Some((l, h, x, y)) = p.recadre {
            f += &format!(",crop={l}:{h}:{x}:{y}");
        }
        if avant != (p.largeur, p.hauteur) {
            f += &format!(",scale={}:{}", p.largeur, p.hauteur);
        }
        g.push(format!("{f},setsar=1[v{i}]"));
    }
    let son = p.morceaux.iter().any(|m| m.son);
    for (e, entree) in p.entrees.iter().enumerate().filter(|(e, _)| son && de(*e).any(|(_, m)| m.son)) {
        g.extend(entree.gains.iter().map(|(i, v)| format!("[{e}:a:{i}]volume={v:.3}[p{e}x{i}]")));
        let pistes: String = entree.gains.iter().map(|(i, _)| format!("[p{e}x{i}]")).collect();
        let mixe = if entree.gains.len() == 1 {
            pistes
        } else {
            g.push(format!("{pistes}amix=inputs={}:normalize=0:duration=longest[m{e}]", entree.gains.len()));
            format!("[m{e}]")
        };
        g.push(format!("{mixe}asplit={}{}", de(e).filter(|(_, m)| m.son).count(), etiquettes(e, "b", true)));
    }
    for (i, m) in p.morceaux.iter().enumerate().filter(|_| son) {
        let (cadence, duree) = (p.entrees[m.entree].ips as f64, m.sortie as f64 / p.ips as f64);
        g.push(if m.son {
            format!("[b{i}]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS{},{FORMAT_SON},apad=whole_dur={duree:.6},atrim=end={duree:.6}[a{i}]", m.images.0 as f64 / cadence, m.images.1 as f64 / cadence, tempo(m.vitesse))
        } else {
            format!("aevalsrc=0|0:s=48000:d={duree:.6},{FORMAT_SON}[a{i}]")
        });
    }
    let entrees: String = (0..n).map(|i| if son { format!("[v{i}][a{i}]") } else { format!("[v{i}]") }).collect();
    let incrustes: Vec<(u64, u64)> = p.textes.iter().flatten().copied().collect();
    let apres = |j: usize| if j == incrustes.len() { "[v]".to_string() } else { format!("[t{j}]") };
    g.push(format!("{entrees}concat=n={n}:v=1:a={}{}{}", u8::from(son), apres(0), if son { "[a]" } else { "" }));
    for (j, (a, b)) in incrustes.iter().enumerate() {
        g.push(format!("[t{j}][{}:v]overlay=0:0:enable='between(n,{a},{})'{}", p.entrees.len() + j, b - 1, apres(j + 1)));
    }
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

pub fn arguments_export(dossier: &Path, images: &[PathBuf], sortie: &Path, p: &Plan, encodeur: &str, kbps: u32) -> Vec<String> {
    let mut a = entree(&dossier.join(&p.entrees[0].source));
    for e in &p.entrees[1..] {
        a.extend(["-i".into(), dossier.join(&e.source).to_string_lossy().into_owned()]);
    }
    for image in images {
        a.extend(["-f".into(), "png_pipe".into(), "-i".into(), image.to_string_lossy().into_owned()]);
    }
    a.extend(["-filter_complex".into(), graphe(p), "-map".into(), "[v]".into()]);
    if p.morceaux.iter().any(|m| m.son) {
        a.extend(["-map", "[a]", "-c:a", "aac", "-ac", "2"].map(String::from));
        a.extend(["-b:a".into(), format!("{AUDIO_KBPS}k")]);
    }
    a.extend(options_video(encodeur, kbps, p.ips));
    sortie_mp4(a, sortie)
}

pub fn dimensions_png(octets: &[u8]) -> Option<(u32, u32)> {
    let lire = |i: usize| Some(u32::from_be_bytes(octets.get(i..i + 4)?.try_into().ok()?));
    (octets.len() <= IMAGE_MAX && octets.starts_with(b"\x89PNG\r\n\x1a\n") && octets.get(12..16)? == b"IHDR").then_some(())?;
    Some((lire(16)?, lire(20)?))
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
pub struct ClipOuvert {
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
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Ouverture {
    pub id: String,
    pub projet: Option<Montage>,
    pub clips: Vec<ClipOuvert>,
    pub manquants: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub id: String,
    pub nom: String,
    pub date: String,
    pub duree_s: f64,
    pub clips: Vec<String>,
    pub manquants: Vec<String>,
}

pub enum Cible<'a> {
    Clip(&'a Path),
    Projet(&'a str),
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

pub fn preparer(ffmpeg: Ffmpeg, encodeurs: &[&str], dossier: &Path, clip: &Path, progres: &mut dyn FnMut(u8)) -> Result<ClipOuvert, String> {
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
    Ok(ClipOuvert {
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
    })
}

fn id_valide(id: &str) -> bool {
    (1..=48).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn fichier_projet(dossier: &Path, id: &str) -> Result<PathBuf, String> {
    if !id_valide(id) {
        return Err("projet inconnu".into());
    }
    Ok(dossier.join(galerie::ANNEXES).join(PROJETS).join(format!("{id}.json")))
}

fn nouvel_id(dossier: &Path) -> String {
    let date = Horodatage::maintenant().pour_nom().replace(' ', "-");
    (1u32..).map(|n| format!("{date}-{n}")).find(|id| fichier_projet(dossier, id).map_or(true, |f| !f.exists())).unwrap_or(date)
}

pub fn lire_projet(octets: &[u8], source: Option<&str>) -> Option<Montage> {
    let mut brut: serde_json::Value = serde_json::from_slice(octets).ok()?;
    let ancien = brut.get("version")?.as_u64()? == 1;
    if ancien {
        brut["version"] = VERSION.into();
    }
    let mut m: Montage = serde_json::from_value(brut).ok()?;
    if let Some(source) = source.filter(|_| ancien) {
        m.segments.iter_mut().for_each(|s| s.source = source.to_string());
        m.volumes = m.volumes.into_values().next().map(|v| BTreeMap::from([(source.to_string(), v)])).unwrap_or_default();
        m.nom = source.strip_suffix(".mp4").unwrap_or(source).chars().take(NOM_MAX).collect();
    }
    verifier(&m).ok().map(|_| m)
}

fn ecrire_projet(dossier: &Path, id: &str, montage: &Montage) -> Result<(), String> {
    let fichier = fichier_projet(dossier, id)?;
    galerie::creer_annexes(dossier).and_then(|a| fs::create_dir_all(a.join(PROJETS))).map_err(|e| format!("dossier des clips inaccessible : {e}"))?;
    let partiel = fichier.with_extension("part");
    let octets = serde_json::to_vec(montage).map_err(|e| e.to_string())?;
    fs::write(&partiel, octets).and_then(|_| fs::rename(&partiel, &fichier)).map_err(|e| format!("montage non enregistré : {e}"))
}

pub fn enregistrer(dossier: &Path, id: &str, montage: &Montage) -> Result<(), String> {
    verifier(montage)?;
    ecrire_projet(dossier, id, montage)
}

pub fn migrer(dossier: &Path) {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES)).into_iter().flatten().flatten();
    for e in entrees {
        let nom = e.file_name().to_string_lossy().into_owned();
        let Some(source) = nom.strip_suffix(ANCIEN).filter(|s| source_valide(s)) else { continue };
        let lu = fs::read(e.path()).ok().and_then(|o| lire_projet(&o, Some(source)));
        if lu.map_or(true, |m| ecrire_projet(dossier, &nouvel_id(dossier), &m).is_ok()) {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn fichiers_projets(dossier: &Path) -> Vec<(String, PathBuf, i64)> {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES).join(PROJETS)).into_iter().flatten().flatten();
    let mut liste: Vec<(String, PathBuf, i64)> = entrees
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let id = nom_de(&e.path(), false);
            let modifie = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
            id_valide(&id).then(|| (id, e.path(), modifie))
        })
        .collect();
    liste.sort_by(|a, b| (b.2, &b.0).cmp(&(a.2, &a.0)));
    liste
}

pub fn projets(dossier: &Path) -> Vec<Resume> {
    migrer(dossier);
    let lus = fichiers_projets(dossier).into_iter().filter_map(|(id, fichier, modifie)| Some((id, lire_projet(&fs::read(fichier).ok()?, None)?, modifie)));
    lus.map(|(id, m, modifie)| {
        let clips: Vec<String> = sources(&m).into_iter().map(String::from).collect();
        let manquants = clips.iter().filter(|c| !dossier.join(c).is_file()).cloned().collect();
        Resume { id, date: format!("{}Z", Horodatage::depuis_epoque(modifie).iso()), duree_s: duree_finale(&m), nom: m.nom, clips, manquants }
    })
    .collect()
}

pub fn renommer_projet(dossier: &Path, id: &str, nom: &str) -> Result<(), String> {
    let fichier = fichier_projet(dossier, id)?;
    let mut m = fs::read(&fichier).ok().and_then(|o| lire_projet(&o, None)).ok_or("projet introuvable ou illisible")?;
    m.nom = galerie::nettoyer(nom);
    if m.nom.is_empty() {
        return Err("nom de projet invalide".into());
    }
    ecrire_projet(dossier, id, &m)
}

pub fn supprimer_projet(dossier: &Path, id: &str) -> Result<(), String> {
    fs::remove_file(fichier_projet(dossier, id)?).map_err(|e| format!("suppression impossible : {e}"))
}

pub fn suivre_renommage(dossier: &Path, ancien: &str, nouveau: &str) {
    migrer(dossier);
    for (id, fichier, _) in fichiers_projets(dossier) {
        let Some(mut m) = fs::read(fichier).ok().and_then(|o| lire_projet(&o, None)).filter(|m| sources(m).contains(&ancien)) else { continue };
        m.segments.iter_mut().filter(|s| s.source == ancien).for_each(|s| s.source = nouveau.to_string());
        if let Some(v) = m.volumes.remove(ancien) {
            m.volumes.insert(nouveau.to_string(), v);
        }
        let _ = ecrire_projet(dossier, &id, &m);
    }
}

pub fn ouvrir(ffmpeg: Ffmpeg, encodeurs: &[&str], dossier: &Path, cible: &Cible, progres: &mut dyn FnMut(&Path, u8)) -> Result<Ouverture, String> {
    migrer(dossier);
    let lire = |id: &str| fichier_projet(dossier, id).ok().and_then(|f| fs::read(f).ok()).and_then(|o| lire_projet(&o, None));
    let (id, projet) = match cible {
        Cible::Projet(id) => (id.to_string(), Some(lire(id).ok_or("Ce projet est introuvable ou illisible.")?)),
        Cible::Clip(clip) => {
            let source = nom_de(clip, true);
            let existant = fichiers_projets(dossier).into_iter().find_map(|(id, _, _)| lire(&id).filter(|m| sources(m) == [source.as_str()]).map(|m| (id, m)));
            existant.map_or_else(|| (nouvel_id(dossier), None), |(id, m)| (id, Some(m)))
        }
    };
    let noms: Vec<String> = match (&projet, cible) {
        (Some(m), _) => sources(m).into_iter().map(String::from).collect(),
        (None, Cible::Clip(clip)) => vec![nom_de(clip, true)],
        (None, Cible::Projet(_)) => Vec::new(),
    };
    let (mut clips, mut manquants) = (Vec::new(), Vec::new());
    for nom in noms {
        let clip = dossier.join(&nom);
        match preparer(ffmpeg, encodeurs, dossier, &clip, &mut |pct| progres(&clip, pct)) {
            Ok(c) => clips.push(c),
            Err(e) if e.starts_with("Clip illisible") && projet.is_some() => manquants.push(nom),
            Err(e) => return Err(e),
        }
    }
    if clips.is_empty() {
        return Err("Tous les clips de ce projet sont introuvables : ils ont été déplacés ou supprimés hors d'Estompe.".into());
    }
    Ok(Ouverture { id, projet, clips, manquants })
}

pub struct Demande<'a> {
    pub id: u64,
    pub dossier: &'a Path,
    pub montage: &'a Montage,
    pub images: &'a [Vec<u8>],
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
    verifier(d.montage)?;
    let mut infos = BTreeMap::new();
    for nom in sources(d.montage) {
        let clip = dossier_clip(d.dossier, nom)?;
        infos.insert(nom.to_string(), sonder(ffmpeg, &clip)?);
    }
    let plan = planifier(d.montage, &infos)?;
    if d.images.len() != plan.textes.len() || d.images.iter().any(|i| dimensions_png(i) != Some((plan.largeur, plan.hauteur))) {
        return Err(Echec::Erreur("Export impossible : les blocs de texte ne correspondent pas au cadre de sortie.".into()));
    }
    let annexes = galerie::creer_annexes(d.dossier).map_err(|e| format!("Export impossible : dossier des clips inaccessible ({e})."))?;
    let utilise = galerie::utilise(d.dossier);
    if utilise >= d.quota {
        return Err(Echec::Erreur("Export impossible : le quota du dossier des clips est atteint. Supprime d'anciens clips puis relance l'export ; le montage est conservé.".into()));
    }
    let surface = (plan.largeur * plan.hauteur) as f64 / (plan.toile.0 * plan.toile.1) as f64;
    let debit = |e: &Entree| debit_export(fs::metadata(d.dossier.join(&e.source)).map_or(0, |m| m.len()), infos[&e.source].duree_s, infos[&e.source].codec);
    let kbps = (plan.entrees.iter().map(debit).max().unwrap_or(2000) as f64 * surface.min(1.0)).max(2000.0) as u32;
    let estimation = (u64::from(kbps + AUDIO_KBPS) * 125) as f64 * plan.duree_s();
    if espace_libre(d.dossier).is_some_and(|libre| (libre as f64) < estimation) {
        return Err(Echec::Erreur("Export impossible : il ne reste pas assez de place sur le disque. Le montage est conservé.".into()));
    }
    let base = format!("export-{}-{}", std::process::id(), d.id);
    let partiel = annexes.join(format!("{base}.part"));
    let images: Vec<PathBuf> = (0..plan.textes.len()).filter(|j| plan.textes[*j].is_some()).map(|j| annexes.join(format!("{base}-t{j}.part"))).collect();
    let abandon = |echec: Echec| {
        for f in images.iter().chain([&partiel]) {
            let _ = fs::remove_file(f);
        }
        Err(echec)
    };
    for (fichier, octets) in images.iter().zip(d.images.iter().zip(&plan.textes).filter(|(_, t)| t.is_some()).map(|(i, _)| i)) {
        if let Err(e) = fs::write(fichier, octets) {
            return abandon(Echec::Erreur(format!("{} Le montage est conservé.", message_disque(e.to_string(), "Export"))));
        }
    }
    let arguments = |encodeur: &str| arguments_export(d.dossier, &images, &partiel, &plan, encodeur, kbps);
    if arguments("libopenh264").iter().map(|a| a.len() + 3).sum::<usize>() > ARGUMENTS_MAX {
        return abandon(Echec::Erreur("Export impossible : le montage compte trop de passages pour un seul export. Réunis ou retire des passages ; le montage est conservé.".into()));
    }
    let mut dernier = "aucun encodeur H.264 disponible".to_string();
    let mut reussi = None;
    for encodeur in d.encodeurs {
        if annule.load(Ordering::SeqCst) {
            break;
        }
        let logiciel = LOGICIELS.contains(encodeur);
        progres(0, logiciel);
        match lancer(ffmpeg, arguments(encodeur), plan.duree_s(), enfant, annule, &mut |p| progres(p, logiciel)) {
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
    if annule.load(Ordering::SeqCst) {
        return abandon(Echec::Annule);
    }
    let Some(logiciel) = reussi else {
        return abandon(Echec::Erreur(format!("{} Le montage est conservé.", message_disque(dernier, "Export"))));
    };
    let premier = d.dossier.join(&d.montage.segments[0].source);
    let nom = Some(galerie::nettoyer(&d.montage.nom)).filter(|n| !n.is_empty()).unwrap_or_else(|| nom_de(&premier, false));
    let chemin = nom_montage(d.dossier, &nom);
    if let Err(e) = fs::rename(&partiel, &chemin) {
        return abandon(Echec::Erreur(format!("Export impossible : {e}. Le montage est conservé.")));
    }
    for f in &images {
        let _ = fs::remove_file(f);
    }
    let annexe = Annexe {
        jeu: galerie::lire_annexe(d.dossier, &premier).map(|a| a.jeu).unwrap_or_default(),
        date: Horodatage::maintenant().iso(),
        duree_s: plan.duree_s(),
        codec: Codec::H264,
        largeur: plan.largeur,
        hauteur: plan.hauteur,
        ips: plan.ips,
        pistes_audio: u32::from(plan.morceaux.iter().any(|m| m.son)),
        origine: Some(nom_de(&premier, false)),
    };
    let _ = fs::write(galerie::annexe(d.dossier, &chemin, "json"), serde_json::to_vec(&annexe).unwrap_or_default());
    let _ = ffmpeg().and_then(|mut c| c.args(galerie::arguments_vignette(&chemin, &galerie::annexe(d.dossier, &chemin, "jpg"))).output().map_err(|e| e.to_string()));
    let taille = fs::metadata(&chemin).map_or(0, |m| m.len());
    Ok(Exporte { chemin, logiciel, quota_atteint: utilise + taille >= d.quota })
}

fn dossier_clip(dossier: &Path, nom: &str) -> Result<PathBuf, String> {
    let clip = dossier.join(nom);
    if !clip.is_file() {
        return Err(format!("Export impossible : le clip « {nom} » est introuvable. Retire-le du montage ; le montage est conservé."));
    }
    galerie::dans_dossier(dossier, &clip.to_string_lossy())
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EtatExport {
    pub id: u64,
    pub projet: String,
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

fn emettre_preparation(app: &AppHandle) -> impl FnMut(&Path, u8) + '_ {
    let mut dernier = None;
    move |clip: &Path, pct: u8| {
        if dernier != Some(pct) {
            dernier = Some(pct);
            let _ = app.emit(EVENEMENT_PREPARATION, serde_json::json!({ "nom": nom_de(clip, false), "pct": pct }));
        }
    }
}

#[tauri::command]
pub async fn montage_ouvrir(app: AppHandle, etat: State<'_, Replay>, chemin: Option<String>, projet: Option<String>) -> Result<Ouverture, String> {
    let (dossier, source) = match &chemin {
        Some(c) => clip(&app, &etat, c).map(|(d, s)| (d, Some(s)))?,
        None => (dossier(&app, &etat)?, None),
    };
    let tache = tauri::async_runtime::spawn_blocking(move || {
        let cible = match (&source, &projet) {
            (Some(s), _) => Cible::Clip(s),
            (None, Some(id)) => Cible::Projet(id),
            (None, None) => return Err("projet inconnu".to_string()),
        };
        ouvrir(&|| crate::video::command(&app), &encodeurs(&app), &dossier, &cible, &mut emettre_preparation(&app))
    });
    tache.await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn montage_clip(app: AppHandle, etat: State<'_, Replay>, chemin: String) -> Result<ClipOuvert, String> {
    let (dossier, source) = clip(&app, &etat, &chemin)?;
    let tache = tauri::async_runtime::spawn_blocking(move || {
        let mut progres = emettre_preparation(&app);
        preparer(&|| crate::video::command(&app), &encodeurs(&app), &dossier, &source, &mut |pct| progres(&source, pct))
    });
    tache.await.map_err(|e| e.to_string())?
}

#[tauri::command(async)]
pub fn montage_projets(app: AppHandle, etat: State<Replay>) -> Result<Vec<Resume>, String> {
    Ok(projets(&dossier(&app, &etat)?))
}

#[tauri::command(async)]
pub fn montage_renommer(app: AppHandle, etat: State<Replay>, projet: String, nom: String) -> Result<(), String> {
    renommer_projet(&dossier(&app, &etat)?, &projet, &nom)
}

#[tauri::command(async)]
pub fn montage_supprimer(app: AppHandle, etat: State<Replay>, projet: String) -> Result<(), String> {
    supprimer_projet(&dossier(&app, &etat)?, &projet)
}

#[tauri::command(async)]
pub fn montage_enregistrer(app: AppHandle, etat: State<Replay>, projet: String, montage: Montage) -> Result<(), String> {
    enregistrer(&dossier(&app, &etat)?, &projet, &montage)
}

#[tauri::command(async)]
pub fn montage_exporter(app: AppHandle, etat: State<Replay>, projet: String, montage: Montage, images: Vec<Vec<u8>>) -> Result<EtatExport, String> {
    let dossier = dossier(&app, &etat)?;
    enregistrer(&dossier, &projet, &montage)?;
    let quota = etat.reglages().quota_octets;
    let id = etat.exports.numero.fetch_add(1, Ordering::Relaxed) + 1;
    let initial = EtatExport { id, projet, nom: montage.nom.clone(), pct: 0, etat: "encours", message: None, chemin: None, logiciel: false };
    let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
    let tache = Tache { etat: initial.clone(), annule: annule.clone(), enfant: enfant.clone() };
    etat.exports.taches.lock().map_err(|e| e.to_string())?.insert(id, tache);
    let _ = app.emit(EVENEMENT_EXPORT, &initial);
    let debut = initial.clone();
    std::thread::spawn(move || {
        let replay = app.state::<Replay>();
        let demande = Demande { id, dossier: &dossier, montage: &montage, images: &images, quota, encodeurs: &encodeurs(&app) };
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
        Segment { source: CLIP.into(), debut, fin, garde, vitesse: 1.0, muet: false }
    }

    fn de(source: &str, debut: f64, fin: f64, vitesse: f64, muet: bool) -> Segment {
        Segment { source: source.into(), debut, fin, garde: true, vitesse, muet }
    }

    fn montage(segments: Vec<Segment>, volumes: &[(f64, bool)]) -> Montage {
        let volumes: Vec<Volume> = volumes.iter().map(|(volume, muet)| Volume { volume: *volume, muet: *muet }).collect();
        let volumes = if volumes.is_empty() { BTreeMap::new() } else { BTreeMap::from([(CLIP.to_string(), volumes)]) };
        Montage { version: VERSION, nom: "Jeu".into(), segments, volumes, textes: Vec::new(), cadre: Cadre::default(), sortie: Sortie::default() }
    }

    fn texte(debut: f64, fin: f64) -> Texte {
        Texte { contenu: "Bien joué 🎉".into(), police: POLICES[0].into(), taille: 0.08, couleur: "#ffffff".into(), habillage: "contour".into(), x: 0.5, y: 0.8, debut, fin }
    }

    fn infos(pistes_audio: u32) -> Infos {
        Infos { duree_s: 10.0, codec: Some(Codec::H264), largeur: 1280, hauteur: 720, ips: 60, pistes_audio }
    }

    fn connus(pistes_audio: u32) -> BTreeMap<String, Infos> {
        let hevc = Infos { duree_s: 8.0, codec: Some(Codec::Hevc), largeur: 2560, hauteur: 1440, ips: 144, pistes_audio: 1 };
        let carre = Infos { duree_s: 5.0, codec: Some(Codec::H264), largeur: 1000, hauteur: 1000, ips: 30, pistes_audio: 0 };
        BTreeMap::from([(CLIP.to_string(), infos(pistes_audio)), ("Hevc.mp4".to_string(), hevc), ("Carre.mp4".to_string(), carre)])
    }

    fn trois_segments() -> Montage {
        montage(vec![segment(0.5, 2.0, true), segment(2.0, 3.0, false), segment(3.0, 5.0, true), segment(5.0, 7.0, false), segment(7.0, 9.5, true)], &[(0.5, false), (1.0, true)])
    }

    fn images(p: &Plan) -> Vec<(u64, u64)> {
        p.morceaux.iter().map(|m| m.images).collect()
    }

    #[test]
    fn montage_recu_jamais_cru_sur_parole() {
        assert!(verifier(&trois_segments()).is_ok());
        assert!(verifier(&Montage { version: 1, ..trois_segments() }).is_err());
        assert!(verifier(&montage(Vec::new(), &[])).is_err());
        assert!(verifier(&montage(vec![segment(0.0, 1.0, true); SEGMENTS_MAX + 1], &[])).is_err());
        for (debut, fin) in [(2.0, 1.0), (1.0, 1.0), (-1.0, 1.0), (f64::NAN, 1.0), (0.0, f64::INFINITY)] {
            assert!(verifier(&montage(vec![segment(debut, fin, true)], &[])).is_err(), "{debut} {fin}");
        }
        assert!(verifier(&montage(vec![segment(2.0, 3.0, true), segment(0.0, 1.0, true), segment(0.5, 2.5, true)], &[])).is_ok());
        for volume in [2.01, -0.1, f64::NAN] {
            assert!(verifier(&montage(vec![segment(0.0, 1.0, true)], &[(volume, false)])).is_err());
        }
        for source in ["..\\dehors.mp4", "C:\\dehors.mp4", "sous/dossier.mp4", "../dehors.mp4", "Jeu.txt", ".mp4", ""] {
            assert!(verifier(&montage(vec![de(source, 0.0, 1.0, 1.0, false)], &[])).is_err(), "{source}");
        }
        let mut volumes = trois_segments();
        volumes.volumes.insert("Autre.mp4".into(), Vec::new());
        assert!(verifier(&volumes).is_err());
        for vitesse in [0.0, 3.0, -1.0, f64::NAN] {
            assert!(verifier(&montage(vec![de(CLIP, 0.0, 1.0, vitesse, false)], &[])).is_err(), "{vitesse}");
        }
        let clips = |n: usize| montage((0..n).map(|i| de(&format!("c{i}.mp4"), 0.0, 1.0, 1.0, false)).collect(), &[]);
        assert!(verifier(&clips(CLIPS_MAX)).is_ok() && verifier(&clips(CLIPS_MAX + 1)).unwrap_err().contains("20 clips"));
        assert!(verifier(&montage(vec![de(CLIP, 0.0, 450.0, 0.25, false)], &[])).is_ok());
        assert!(verifier(&montage(vec![de(CLIP, 0.0, 451.0, 0.25, false)], &[])).unwrap_err().contains("30 minutes"));
        assert!(verifier(&montage(vec![de(CLIP, 0.0, 1801.0, 1.0, false), segment(0.0, 5000.0, false)], &[])).is_err());
        let cadre = |x, y, l, h, rapport: &str| verifier(&Montage { cadre: Cadre { x, y, l, h, rapport: rapport.into() }, ..trois_segments() });
        assert!(cadre(0.25, 0.0, 0.5, 1.0, "9:16").is_ok() && cadre(0.0, 0.0, 1.0, 1.0, "libre").is_ok());
        assert!(cadre(0.6, 0.0, 0.5, 1.0, "libre").is_err() && cadre(0.0, 0.0, 0.0, 1.0, "libre").is_err() && cadre(0.0, -0.1, 1.0, 1.0, "libre").is_err());
        assert!(cadre(0.0, 0.0, f64::NAN, 1.0, "libre").is_err() && cadre(0.0, 0.0, 1.0, 1.0, "21:9").is_err());
        let avec = |t: Texte| verifier(&Montage { textes: vec![t], ..trois_segments() });
        assert!(avec(texte(2.0, 5.0)).is_ok() && avec(Texte { contenu: "é".repeat(TEXTE_MAX), ..texte(0.0, 1.0) }).is_ok());
        assert!(avec(texte(5.0, 2.0)).is_err() && avec(Texte { contenu: " ".into(), ..texte(0.0, 1.0) }).is_err() && avec(Texte { contenu: "é".repeat(TEXTE_MAX + 1), ..texte(0.0, 1.0) }).is_err());
        assert!(avec(Texte { police: "Comic Sans MS".into(), ..texte(0.0, 1.0) }).is_err() && avec(Texte { couleur: "rouge".into(), ..texte(0.0, 1.0) }).is_err());
        assert!(avec(Texte { x: 1.5, ..texte(0.0, 1.0) }).is_err() && avec(Texte { taille: 0.9, ..texte(0.0, 1.0) }).is_err() && avec(Texte { habillage: "ombre".into(), ..texte(0.0, 1.0) }).is_err());
        assert!(verifier(&Montage { textes: vec![texte(0.0, 1.0); TEXTES_MAX + 1], ..trois_segments() }).is_err());
        assert!(verifier(&Montage { sortie: Sortie { hauteur: Some(0), ips: None }, ..trois_segments() }).is_err());
        assert!(verifier(&Montage { sortie: Sortie { hauteur: None, ips: Some(1000) }, ..trois_segments() }).is_err());
        assert!(verifier(&Montage { nom: "x".repeat(NOM_MAX + 1), ..trois_segments() }).is_err());
        assert!(planifier(&montage(vec![segment(0.0, 11.0, true)], &[]), &connus(1)).is_err());
        assert!(planifier(&montage(vec![segment(0.0, 5.0, false)], &[]), &connus(1)).unwrap_err().contains("rien à exporter"));
        assert!(planifier(&trois_segments(), &BTreeMap::from([(CLIP.to_string(), Infos { ips: 0, ..infos(2) })])).is_err());
        assert!(planifier(&montage(vec![de("Absent.mp4", 0.0, 1.0, 1.0, false)], &[]), &connus(1)).unwrap_err().contains("Absent.mp4"));
        assert_eq!(planifier(&Montage { sortie: Sortie { hauteur: None, ips: Some(120) }, ..trois_segments() }, &connus(1)).unwrap().ips, 60);
    }

    #[test]
    fn projet_serialise_et_repris_de_la_version_1() {
        let complet = Montage { textes: vec![texte(2.0, 5.0)], cadre: Cadre { x: 0.25, y: 0.0, l: 0.5, h: 1.0, rapport: "9:16".into() }, sortie: Sortie { hauteur: Some(720), ips: Some(30) }, ..trois_segments() };
        let json = serde_json::to_value(&complet).unwrap();
        assert_eq!(json["segments"][1], serde_json::json!({ "source": CLIP, "debut": 2.0, "fin": 3.0, "garde": false, "vitesse": 1.0, "muet": false }));
        assert_eq!(json["volumes"][CLIP][1], serde_json::json!({ "volume": 1.0, "muet": true }));
        assert_eq!(json["textes"][0], serde_json::json!({ "contenu": "Bien joué 🎉", "police": "Bricolage Grotesque", "taille": 0.08, "couleur": "#ffffff", "habillage": "contour", "x": 0.5, "y": 0.8, "debut": 2.0, "fin": 5.0 }));
        assert_eq!((&json["version"], &json["nom"], &json["cadre"], &json["sortie"]), (&serde_json::json!(2), &serde_json::json!("Jeu"), &serde_json::json!({ "x": 0.25, "y": 0.0, "l": 0.5, "h": 1.0, "rapport": "9:16" }), &serde_json::json!({ "hauteur": 720, "ips": 30 })));
        assert_eq!(serde_json::from_value::<Montage>(json.clone()).unwrap(), complet);
        assert_eq!(lire_projet(&serde_json::to_vec(&json).unwrap(), None), Some(complet));
        let v1 = br#"{"version":1,"segments":[{"source":"Ancien nom.mp4","debut":0.5,"fin":2,"garde":true},{"source":"Ancien nom.mp4","debut":2,"fin":3,"garde":false}],"volumes":{"Ancien nom.mp4":[{"volume":0.5,"muet":false},{"volume":1,"muet":true}]}}"#;
        let repris = lire_projet(v1, Some(CLIP)).unwrap();
        assert_eq!(repris, Montage { segments: vec![segment(0.5, 2.0, true), segment(2.0, 3.0, false)], ..trois_segments() });
        assert_eq!((repris.version, repris.textes.len(), repris.cadre.clone(), repris.sortie.clone()), (2, 0, Cadre::default(), Sortie::default()));
        for illisible in [&b"pas du json"[..], b"{}", br#"{"version":3,"segments":[]}"#, br#"{"version":2,"segments":[]}"#, br#"{"version":1,"segments":[{"source":"a.mp4","debut":2,"fin":1,"garde":true}]}"#] {
            assert_eq!(lire_projet(illisible, Some(CLIP)), None);
        }
    }

    #[test]
    fn projets_a_cote_des_clips() {
        let d = dossier_test("projets");
        for nom in [CLIP, "Autre.mp4"] {
            fs::write(d.join(nom), b"x").unwrap();
        }
        let annexes = galerie::creer_annexes(&d).unwrap();
        fs::write(annexes.join("Jeu.mp4.montage.json"), br#"{"version":1,"segments":[{"source":"Jeu.mp4","debut":0.5,"fin":2,"garde":true}],"volumes":{}}"#).unwrap();
        fs::write(annexes.join("Autre.mp4.montage.json"), b"illisible").unwrap();
        let liste = projets(&d);
        assert_eq!(liste.len(), 1);
        let id = liste[0].id.clone();
        assert_eq!((liste[0].nom.as_str(), liste[0].clips.clone(), liste[0].manquants.len(), liste[0].duree_s), ("Jeu", vec![CLIP.to_string()], 0, 1.5));
        assert!(!annexes.join("Jeu.mp4.montage.json").exists() && !annexes.join("Autre.mp4.montage.json").exists() && annexes.join("projets").join(format!("{id}.json")).is_file());
        let deux = Montage { segments: vec![segment(0.0, 1.0, true), de("Autre.mp4", 0.0, 2.0, 0.5, false)], volumes: BTreeMap::from([("Autre.mp4".to_string(), vec![Volume { volume: 2.0, muet: false }])]), nom: "Deux clips".into(), ..trois_segments() };
        enregistrer(&d, "deux", &deux).unwrap();
        assert!(enregistrer(&d, "../dehors", &deux).is_err() && enregistrer(&d, "", &deux).is_err() && enregistrer(&d, "deux", &Montage { version: 1, ..deux.clone() }).is_err());
        assert_eq!(serde_json::from_slice::<Montage>(&fs::read(annexes.join("projets").join("deux.json")).unwrap()).unwrap(), deux);
        let trouve = |id: &str| projets(&d).into_iter().find(|p| p.id == id).unwrap();
        assert_eq!((projets(&d).len(), trouve("deux").duree_s, trouve("deux").clips.len()), (2, 5.0, 2));
        renommer_projet(&d, "deux", " Mon: montage ").unwrap();
        assert!(renommer_projet(&d, "deux", " ").is_err() && renommer_projet(&d, "absent", "x").is_err());
        assert_eq!(trouve("deux").nom, "Mon montage");
        fs::rename(d.join("Autre.mp4"), d.join("Renomme.mp4")).unwrap();
        suivre_renommage(&d, "Autre.mp4", "Renomme.mp4");
        let suivi = lire_projet(&fs::read(annexes.join("projets").join("deux.json")).unwrap(), None).unwrap();
        assert_eq!((suivi.segments[1].source.as_str(), suivi.volumes.keys().next().map(String::as_str), trouve("deux").manquants.len()), ("Renomme.mp4", Some("Renomme.mp4"), 0));
        fs::remove_file(d.join("Renomme.mp4")).unwrap();
        assert_eq!((trouve("deux").manquants, trouve(&id).manquants.len()), (vec!["Renomme.mp4".to_string()], 0));
        supprimer_projet(&d, "deux").unwrap();
        assert!(supprimer_projet(&d, "deux").is_err() && supprimer_projet(&d, "../../Jeu").is_err());
        assert_eq!((projets(&d).len(), fs::read(d.join(CLIP)).unwrap()), (1, b"x".to_vec()));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn plan_a_l_image_pres() {
        let p = planifier(&trois_segments(), &connus(2)).unwrap();
        assert_eq!(images(&p), [(30, 120), (180, 300), (420, 570)]);
        assert_eq!((p.entrees.len(), p.entrees[0].gains.clone(), p.entrees[0].echelle, p.entrees[0].marge), (1, vec![(0, 0.5)], None, None));
        assert_eq!((p.duree_s(), p.ips, p.largeur, p.hauteur, p.recadre, p.morceaux.iter().all(|m| m.son)), (6.0, 60, 1280, 720, None, true));
        let colles = planifier(&montage(vec![segment(0.0, 1.0, true), segment(1.0, 2.0, true), segment(2.0, 2.004, true), segment(4.0, 10.01, true)], &[]), &connus(2)).unwrap();
        assert_eq!((images(&colles), colles.entrees[0].gains.clone()), (vec![(0, 120), (240, 600)], vec![(0, 1.0), (1, 1.0)]));
        assert!(planifier(&trois_segments(), &connus(0)).unwrap().morceaux.iter().all(|m| !m.son));
        assert!(planifier(&montage(vec![segment(0.0, 1.0, true)], &[(0.0, false), (1.5, true)]), &connus(2)).unwrap().morceaux.iter().all(|m| !m.son));
        assert_eq!(planifier(&montage(vec![segment(0.0, 1.0, true)], &[(2.0, false)]), &connus(2)).unwrap().entrees[0].gains, [(0, 2.0), (1, 1.0)]);
    }

    #[test]
    fn plan_vitesses_ordre_et_textes() {
        let vitesses = montage(vec![de(CLIP, 0.0, 1.0, 0.5, false), de(CLIP, 1.0, 2.0, 2.0, false), de(CLIP, 2.0, 3.0, 0.25, true), de(CLIP, 3.0, 4.0, 4.0, false), de(CLIP, 4.0, 5.0, 1.5, false)], &[]);
        let p = planifier(&vitesses, &connus(1)).unwrap();
        assert_eq!(p.morceaux.iter().map(|m| m.sortie).collect::<Vec<_>>(), [120, 30, 240, 15, 40]);
        assert_eq!((p.images(), p.duree_s(), duree_finale(&vitesses)), (445, 445.0 / 60.0, 2.0 + 0.5 + 4.0 + 0.25 + 1.0 / 1.5));
        assert_eq!(p.morceaux.iter().map(|m| m.son).collect::<Vec<_>>(), [true, true, false, true, true]);
        let trente = planifier(&Montage { sortie: Sortie { hauteur: None, ips: Some(30) }, ..montage(vec![de(CLIP, 0.0, 0.05, 1.0, false), de(CLIP, 1.0, 1.05, 1.5, false), de(CLIP, 2.0, 2.05, 1.0, false)], &[]) }, &connus(1)).unwrap();
        assert_eq!((trente.ips, trente.morceaux.iter().map(|m| m.sortie).collect::<Vec<_>>(), trente.images()), (30, vec![2, 1, 1], 4));
        let melange = montage(vec![de("Hevc.mp4", 1.0, 2.0, 1.0, false), de(CLIP, 2.0, 3.0, 1.0, false), de("Carre.mp4", 0.0, 1.0, 1.0, false), de(CLIP, 0.0, 1.0, 1.0, false), de(CLIP, 5.0, 6.0, 1.0, false), de("Hevc.mp4", 4.0, 5.0, 2.0, false)], &[]);
        let p = planifier(&melange, &connus(2)).unwrap();
        assert_eq!(p.entrees.iter().map(|e| (e.source.as_str(), e.ips)).collect::<Vec<_>>(), [("Hevc.mp4", 144), (CLIP, 60), ("Carre.mp4", 30), (CLIP, 60)]);
        assert_eq!(p.morceaux.iter().map(|m| m.entree).collect::<Vec<_>>(), [0, 1, 2, 3, 1, 0]);
        assert_eq!((images(&p)[0], images(&p)[2], images(&p)[5]), ((144, 288), (0, 30), (576, 720)));
        assert_eq!((p.toile, p.ips, p.largeur, p.hauteur, p.images()), ((2560, 1440), 144, 2560, 1440, 144 * 5 + 72));
        assert_eq!((p.entrees[0].echelle, p.entrees[0].marge, p.entrees[1].echelle, p.entrees[1].marge), (None, None, Some((2560, 1440)), None));
        assert_eq!((p.entrees[2].echelle, p.entrees[2].marge, p.morceaux[2].son), (Some((1440, 1440)), Some((560, 0)), false));
        let textes = Montage { textes: vec![texte(2.0, 5.0), texte(4.0, 99.0), texte(7.0, 8.0), texte(0.0, 0.004)], ..trois_segments() };
        assert_eq!(planifier(&textes, &connus(1)).unwrap().textes, [Some((120, 300)), Some((240, 360)), None, None]);
        let vertical = Montage { cadre: Cadre { x: 0.341_796_875, y: 0.0, l: 0.316_406_25, h: 1.0, rapport: "9:16".into() }, sortie: Sortie { hauteur: Some(1080), ips: Some(60) }, ..melange };
        let p = planifier(&vertical, &connus(1)).unwrap();
        assert_eq!((p.recadre, p.largeur, p.hauteur, p.ips), (Some((810, 1440, 876, 0)), 594, 1056, 60));
    }

    #[test]
    fn geometrie_du_cadre_et_de_la_sortie() {
        assert_eq!((toile(1920, 1080), toile(1921, 1081), toile(1, 1)), ((1920, 1080), (1920, 1080), (2, 2)));
        assert_eq!(inscrire(1920, 1080, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(2560, 1440, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(1280, 720, (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(inscrire(1000, 1000, (1920, 1080)), (1080, 1080, 420, 0));
        assert_eq!(inscrire(640, 360, (320, 240)), (320, 180, 0, 30));
        assert_eq!(inscrire(1080, 1920, (1920, 1080)), (608, 1080, 656, 0));
        assert_eq!(inscrire(2560, 1080, (1920, 1080)), (1920, 810, 0, 134));
        let cadre = |x, y, l, h| Cadre { x, y, l, h, rapport: "libre".into() };
        assert_eq!(cadre_pixels(&Cadre::default(), (1920, 1080)), (1920, 1080, 0, 0));
        assert_eq!(cadre_pixels(&cadre(0.341_796_875, 0.0, 0.316_406_25, 1.0), (1920, 1080)), (608, 1080, 656, 0));
        assert_eq!(cadre_pixels(&cadre(0.25, 0.25, 0.5, 0.5), (1920, 1080)), (960, 540, 480, 270));
        assert_eq!(cadre_pixels(&cadre(0.9, 0.9, 0.5, 0.5), (1920, 1080)), (960, 540, 960, 540));
        assert_eq!(cadre_pixels(&cadre(0.1, 0.2, 0.333, 0.333), (1280, 720)), (426, 240, 128, 144));
        assert_eq!(cadre_pixels(&cadre(0.0, 0.0, 0.0001, 0.0001), (1280, 720)), (2, 2, 0, 0));
        assert_eq!(dimensions_sortie((1920, 1080), "libre", None), (1920, 1080));
        assert_eq!(dimensions_sortie((1920, 1080), "libre", Some(720)), (1280, 720));
        assert_eq!(dimensions_sortie((1920, 1080), "libre", Some(2160)), (1920, 1080));
        assert_eq!(dimensions_sortie((426, 240), "libre", Some(121)), (216, 122));
        assert_eq!(dimensions_sortie((1920, 1080), "16:9", None), (1920, 1080));
        assert_eq!(dimensions_sortie((1920, 1080), "16:9", Some(720)), (1280, 720));
        assert_eq!(dimensions_sortie((608, 1080), "9:16", None), (594, 1056));
        assert_eq!(dimensions_sortie((608, 1080), "9:16", Some(720)), (396, 704));
        assert_eq!(dimensions_sortie((1080, 1080), "1:1", None), (1080, 1080));
        assert_eq!(dimensions_sortie((864, 1080), "4:5", None), (864, 1080));
        assert_eq!(dimensions_sortie((136, 240), "9:16", None), (126, 224));
        let png = |l: u32, h: u32| [&b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..], &l.to_be_bytes(), &h.to_be_bytes()].concat();
        assert_eq!((dimensions_png(&png(594, 1056)), dimensions_png(b"GIF89a"), dimensions_png(&png(1, 1)[..20])), (Some((594, 1056)), None, None));
    }

    #[test]
    fn arguments_ffmpeg_de_l_export() {
        let p = planifier(&trois_segments(), &connus(2)).unwrap();
        let g = graphe(&p);
        assert!(g.starts_with("[0:v:0]fps=60:start_time=0,split=3[e0][e1][e2];[e0]trim=start_frame=30:end_frame=120,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=90,setsar=1[v0];"), "{g}");
        assert!(g.contains("[e2]trim=start_frame=420:end_frame=570,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=150,setsar=1[v2]"));
        assert!(g.contains("[0:a:0]volume=0.500[p0x0];[p0x0]asplit=3[b0][b1][b2]") && !g.contains("0:a:1") && !g.contains("amix") && !g.contains("scale") && !g.contains("crop") && !g.contains("overlay"));
        assert!(g.contains(&format!("[b1]atrim=start=3.000000:end=5.000000,asetpts=PTS-STARTPTS,{FORMAT_SON},apad=whole_dur=2.000000,atrim=end=2.000000[a1]")));
        assert!(g.ends_with("[v0][a0][v1][a1][v2][a2]concat=n=3:v=1:a=1[v][a]"));
        let mut deux = p.clone();
        deux.entrees[0].gains = vec![(0, 0.5), (1, 2.0)];
        assert!(graphe(&deux).contains("[0:a:0]volume=0.500[p0x0];[0:a:1]volume=2.000[p0x1];[p0x0][p0x1]amix=inputs=2:normalize=0:duration=longest[m0];[m0]asplit=3"));
        let muet = graphe(&planifier(&montage(vec![segment(0.0, 1.0, true)], &[]), &connus(0)).unwrap());
        assert_eq!(muet, "[0:v:0]fps=60:start_time=0,split=1[e0];[e0]trim=start_frame=0:end_frame=60,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=60,setsar=1[v0];[v0]concat=n=1:v=1:a=0[v]");
        let avance = Montage {
            segments: vec![de("Hevc.mp4", 0.0, 1.0, 0.25, false), de(CLIP, 2.0, 3.0, 4.0, true), de("Carre.mp4", 0.0, 1.0, 1.5, false), de(CLIP, 0.0, 1.0, 1.0, false)],
            textes: vec![texte(2.0, 5.0), texte(90.0, 91.0), texte(4.0, 4.5)],
            cadre: Cadre { x: 0.341_796_875, y: 0.0, l: 0.316_406_25, h: 1.0, rapport: "9:16".into() },
            sortie: Sortie { hauteur: Some(1080), ips: Some(60) },
            ..montage(Vec::new(), &[(0.5, false)])
        };
        let p = planifier(&avance, &connus(1)).unwrap();
        let g = graphe(&p);
        assert!(g.contains("[0:v:0]fps=144:start_time=0,split=1[e0];[1:v:0]fps=60:start_time=0,split=1[e1];[2:v:0]fps=30:start_time=0,split=1[e2];[3:v:0]fps=60:start_time=0,split=1[e3];"), "{g}");
        assert!(g.contains("[e0]trim=start_frame=0:end_frame=144,setpts=(PTS-STARTPTS)/0.25,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=240,crop=810:1440:876:0,scale=594:1056,setsar=1[v0]"), "{g}");
        assert!(g.contains("[e1]trim=start_frame=120:end_frame=180,setpts=(PTS-STARTPTS)/4,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=15,scale=2560:1440,crop=810:1440:876:0,scale=594:1056,setsar=1[v1]"));
        assert!(g.contains("[e2]trim=start_frame=0:end_frame=30,setpts=(PTS-STARTPTS)/1.5,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=40,scale=1440:1440,pad=2560:1440:560:0,crop=810:1440:876:0,scale=594:1056,setsar=1[v2]"));
        assert!(g.contains(&format!("[0:a:0]volume=1.000[p0x0];[p0x0]asplit=1[b0];[3:a:0]volume=0.500[p3x0];[p3x0]asplit=1[b3];[b0]atrim=start=0.000000:end=1.000000,asetpts=PTS-STARTPTS,atempo=0.5,atempo=0.5,{FORMAT_SON},apad=whole_dur=4.000000,atrim=end=4.000000[a0]")));
        assert!(g.contains(&format!("aevalsrc=0|0:s=48000:d=0.250000,{FORMAT_SON}[a1];aevalsrc=0|0:s=48000:d=0.666667,{FORMAT_SON}[a2];[b3]atrim=start=0.000000:end=1.000000,asetpts=PTS-STARTPTS,{FORMAT_SON},apad=whole_dur=1.000000,atrim=end=1.000000[a3]")));
        assert!(g.ends_with("[v0][a0][v1][a1][v2][a2][v3][a3]concat=n=4:v=1:a=1[t0][a];[t0][4:v]overlay=0:0:enable='between(n,120,299)'[t1];[t1][5:v]overlay=0:0:enable='between(n,240,269)'[v]"));
        assert_eq!((tempo(2.0), tempo(1.5), tempo(4.0), tempo(0.5)), (",atempo=2".to_string(), ",atempo=1.5".to_string(), ",atempo=2,atempo=2".to_string(), ",atempo=0.5".to_string()));
        let a = arguments_export(Path::new("d"), &[PathBuf::from("t0.part"), PathBuf::from("t2.part")], Path::new("out.part"), &p, "h264_amf", 12_000);
        let paire = |x: &str, y: &str| a.windows(2).any(|w| w == [x, y]);
        let clip = |nom: &str| Path::new("d").join(nom).to_string_lossy().into_owned();
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 6);
        assert!(paire("-progress", "pipe:1") && paire("-i", &clip("Hevc.mp4")) && paire("-i", &clip("Carre.mp4")) && paire("-filter_complex", &g) && paire("-map", "[v]") && paire("-map", "[a]"));
        assert!(a.windows(4).any(|w| w == ["-f", "png_pipe", "-i", "t2.part"]) && a.iter().position(|x| x == "t0.part") > a.iter().rposition(|x| *x == clip(CLIP)));
        assert!(paire("-c:v", "h264_amf") && paire("-b:v", "12000k") && paire("-c:a", "aac") && paire("-r", "60") && paire("-fps_mode", "cfr") && paire("-pix_fmt", "yuv420p"));
        assert!(paire("-movflags", "+faststart") && paire("-f", "mp4") && a.last().unwrap() == "out.part");
        let sans_son = arguments_export(Path::new("d"), &[], Path::new("o"), &planifier(&trois_segments(), &connus(0)).unwrap(), "libopenh264", 2000);
        assert!(!sans_son.contains(&"[a]".to_string()) && !sans_son.contains(&"-c:a".to_string()) && !sans_son.contains(&"png_pipe".to_string()));
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
        let ralenti = planifier(&montage(vec![de(CLIP, 0.0, 2.0, 0.25, false), de(CLIP, 2.0, 6.0, 2.0, false)], &[]), &connus(1)).unwrap();
        assert_eq!((ralenti.duree_s(), progression("out_time_us=5000000", ralenti.duree_s())), (10.0, Some(50)));
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
    fn suivi_des_exports() {
        let exports = Exports::default();
        let etat = EtatExport { id: 1, projet: "p".into(), nom: "n".into(), pct: 0, etat: "encours", message: None, chemin: None, logiciel: false };
        let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
        exports.taches.lock().unwrap().insert(1, Tache { etat, annule: annule.clone(), enfant });
        assert_eq!(exports.avancer(1, 0, false), None);
        assert_eq!(exports.avancer(1, 40, false).unwrap().pct, 40);
        assert_eq!(exports.avancer(1, 40, false), None);
        assert!(exports.avancer(1, 0, true).unwrap().logiciel);
        assert_eq!(exports.en_cours()[0].pct, 0);
        assert_eq!((&serde_json::to_value(&exports.en_cours()[0]).unwrap()["etat"], &serde_json::to_value(&exports.en_cours()[0]).unwrap()["projet"]), (&serde_json::json!("encours"), &serde_json::json!("p")));
        assert!(exports.annuler(2).is_err());
        exports.annuler(1).unwrap();
        assert!(annule.load(Ordering::SeqCst));
    }

    fn ffmpeg_local() -> Result<Command, String> {
        let mut c = Command::new("ffmpeg");
        c.args(["-hide_banner", "-nostdin"]);
        Ok(c)
    }

    fn encodeurs_locaux() -> String {
        ffmpeg_local().ok().and_then(|mut c| c.arg("-encoders").output().ok()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    }

    fn encodeur_local() -> Option<&'static str> {
        let liste = encodeurs_locaux();
        LOGICIELS.into_iter().find(|e| liste.contains(e))
    }

    fn fabriquer(chemin: &Path, image: &str, tons: &[u32], encodeur: &str, ips: u32) {
        let mut c = ffmpeg_local().unwrap();
        c.args(["-v", "error", "-f", "lavfi", "-i", image]);
        for t in tons {
            c.args(["-f", "lavfi", "-i", &format!("sine=frequency={t}:sample_rate=48000:duration=4")]);
        }
        c.args(["-map", "0:v"]);
        for i in 0..tons.len() {
            c.args(["-map", &format!("{}:a", i + 1)]);
        }
        c.args(["-c:v", encodeur, "-b:v", "500k", "-g", &ips.to_string(), "-pix_fmt", "yuv420p", "-c:a", "aac", "-ac:a:0", "2", "-ac:a:1", "1", "-f", "mp4", "-y"]);
        let resultat = c.arg(chemin).output().unwrap();
        assert!(resultat.status.success(), "{}", String::from_utf8_lossy(&resultat.stderr));
    }

    fn clip_synthetique(chemin: &Path, encodeur: &str) {
        fabriquer(chemin, "testsrc=size=320x240:rate=30:duration=4", &[440, 1500], encodeur, 30);
    }

    fn son(chemin: &Path) -> Vec<i16> {
        let sortie = ffmpeg_local().unwrap().args(["-v", "error", "-i"]).arg(chemin).args(["-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "s16le", "-"]).output().unwrap();
        sortie.stdout.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]).saturating_abs()).collect()
    }

    fn crete(chemin: &Path) -> i16 {
        son(chemin).into_iter().skip(4800).max().unwrap_or(0)
    }

    fn gris(chemin: &Path) -> Vec<u8> {
        ffmpeg_local().unwrap().args(["-v", "error", "-i"]).arg(chemin).args(["-map", "0:v:0", "-f", "rawvideo", "-pix_fmt", "gray", "-"]).output().unwrap().stdout
    }

    fn png(dessin: &str) -> Vec<u8> {
        let source = format!("color=c=black@0:s=126x224,format=rgba,{dessin}");
        ffmpeg_local().unwrap().args(["-v", "error", "-f", "lavfi", "-i", &source, "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"]).output().unwrap().stdout
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
        assert_eq!((ouverture.apercu.as_str(), ouverture.nom.as_str(), ouverture.source.as_str()), (clip.to_str().unwrap(), "Jeu", CLIP));
        assert_eq!((ouverture.pistes[0].nom.as_str(), ouverture.pistes[1].nom.as_str()), ("Jeu", "Micro"));
        let piste = sonder(&ffmpeg_local, Path::new(&ouverture.pistes[1].chemin));
        assert!(piste.is_err() && Path::new(&ouverture.pistes[1].chemin).is_file());
        let date = modifie(Path::new(&ouverture.pistes[0].chemin));
        assert_eq!(preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |_| {}).map(|o| modifie(Path::new(&o.pistes[0].chemin))), Ok(date));

        let m = montage(vec![segment(0.5, 1.0, true), segment(1.0, 1.5, false), segment(1.5, 2.5, true), segment(2.5, 3.0, false), segment(3.0, 3.5, true)], &[(0.5, false), (1.0, true)]);
        let neuf = ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Clip(&clip), &mut |_, _| {}).unwrap();
        assert_eq!((neuf.projet.as_ref(), neuf.clips.len(), neuf.manquants.len(), projets(&d).len()), (None, 1, 0, 0));
        enregistrer(&d, &neuf.id, &m).unwrap();
        let rouvert = ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Clip(&clip), &mut |_, _| {}).unwrap();
        assert_eq!((rouvert.id.as_str(), rouvert.projet.as_ref(), &rouvert.clips[0]), (neuf.id.as_str(), Some(&m), &ouverture));
        assert_eq!(ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Projet(&neuf.id), &mut |_, _| {}), Ok(rouvert));
        assert!(ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Projet("absent"), &mut |_, _| {}).unwrap_err().contains("introuvable"));
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let mut suivi = Vec::new();
        let demande = Demande { id: 1, dossier: &d, montage: &m, images: &[], quota: 1 << 30, encodeurs: &["encodeur_absent", encodeur] };
        let e = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |p, logiciel| suivi.push((p, logiciel))).unwrap();
        assert_eq!((e.chemin.clone(), e.logiciel, e.quota_atteint), (d.join("Jeu (montage).mp4"), true, false));
        assert_eq!((suivi[0], suivi.iter().filter(|s| s.0 == 0).count() >= 2), ((0, false), true));
        assert!(suivi.windows(2).all(|w| w[0].1 != w[1].1 || w[0].0 <= w[1].0) && suivi.iter().all(|s| s.0 <= 99));
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!((infos.codec, infos.largeur, infos.hauteur, infos.ips, infos.pistes_audio), (Some(Codec::H264), 320, 240, 30, 1));
        assert!((infos.duree_s - 2.0).abs() <= 1.0 / 30.0 + 0.011, "durée {}", infos.duree_s);
        assert_eq!(gris(&e.chemin).len(), 320 * 240 * 60);
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
        let texte_faux = Montage { textes: vec![texte(0.5, 1.0)], ..m.clone() };
        for images in [&[][..], &[png("null")][..], &[b"pas un png".to_vec()][..]] {
            let refus = exporter(&ffmpeg_local, &Demande { id: 6, montage: &texte_faux, images, ..demande }, &annule, &enfant, &mut |_, _| {});
            assert!(matches!(refus, Err(Echec::Erreur(ref m)) if m.contains("blocs de texte")), "{refus:?}");
        }
        assert_eq!((partiels(), galerie::lister(&d).len()), (0, fichiers));
        assert_eq!(projets(&d).len(), 1);

        fs::remove_file(&clip).unwrap();
        assert!(sonder(&ffmpeg_local, &clip).unwrap_err().contains("Clip illisible"));
        let disparu = exporter(&ffmpeg_local, &Demande { id: 7, ..demande }, &annule, &enfant, &mut |_, _| {});
        assert!(matches!(disparu, Err(Echec::Erreur(ref m)) if m.contains("« Jeu.mp4 » est introuvable")), "{disparu:?}");
        fs::write(&clip, b"pas une video").unwrap();
        assert!(preparer(&ffmpeg_local, &[encodeur], &d, &clip, &mut |_| {}).unwrap_err().contains("Clip illisible"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn export_reel_de_plusieurs_clips_recadres_avec_vitesses_et_textes() {
        let liste = encodeurs_locaux();
        let Some(encodeur) = encodeur_local().filter(|_| liste.contains("libx265")) else {
            eprintln!("ignoré : ffmpeg, libx265 ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let d = dossier_test("export-avance");
        let (rouge, vert, bleu) = (d.join("Rouge.mp4"), d.join("Vert.mp4"), d.join("Bleu.mp4"));
        fabriquer(&rouge, "color=c=red:size=320x240:rate=30:duration=4,drawbox=x=140:y=100:w=40:h=40:color=white:t=fill", &[440, 1500], encodeur, 30);
        fabriquer(&vert, "color=c=lime:size=640x360:rate=60:duration=3", &[], "libx265", 60);
        fabriquer(&bleu, "color=c=blue:size=320x240:rate=30:duration=4", &[880], encodeur, 30);
        let originaux: Vec<Vec<u8>> = [&rouge, &vert, &bleu].iter().map(|f| fs::read(f).unwrap()).collect();
        let m = Montage {
            version: VERSION,
            nom: "Mon montage".into(),
            segments: vec![de("Bleu.mp4", 0.0, 1.0, 1.0, false), de("Rouge.mp4", 1.0, 2.0, 0.5, false), Segment { garde: false, ..de("Vert.mp4", 1.0, 3.0, 1.0, false) }, de("Vert.mp4", 0.0, 1.0, 2.0, false), de("Rouge.mp4", 0.0, 0.5, 1.0, true)],
            volumes: BTreeMap::from([("Rouge.mp4".to_string(), vec![Volume { volume: 1.0, muet: false }, Volume { volume: 1.0, muet: true }])]),
            textes: vec![texte(1.0, 2.0), texte(1.5, 3.9), texte(50.0, 60.0)],
            cadre: Cadre { x: 0.2875, y: 0.0, l: 0.421_875, h: 1.0, rapport: "9:16".into() },
            sortie: Sortie { hauteur: None, ips: Some(30) },
        };
        enregistrer(&d, "avance", &m).unwrap();
        let ouverture = ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Projet("avance"), &mut |_, _| {}).unwrap();
        assert_eq!(ouverture.clips.iter().map(|c| (c.source.as_str(), c.codec, c.ips, c.pistes.len())).collect::<Vec<_>>(), [("Bleu.mp4", Some(Codec::H264), 30, 1), ("Rouge.mp4", Some(Codec::H264), 30, 2), ("Vert.mp4", Some(Codec::Hevc), 60, 0)]);
        assert!(ouverture.clips[2].apercu.ends_with("apercu.mp4") && ouverture.manquants.is_empty() && ouverture.projet.as_ref() == Some(&m));
        let images = [png("drawbox=x=10:y=180:w=30:h=20:color=white:t=fill:replace=1"), png("drawbox=x=80:y=20:w=30:h=20:color=white:t=fill:replace=1"), png("null")];
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let demande = Demande { id: 1, dossier: &d, montage: &m, images: &images, quota: 1 << 30, encodeurs: &[encodeur] };
        let e = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |_, _| {}).unwrap();
        assert_eq!(e.chemin, d.join("Mon montage (montage).mp4"));
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!((infos.codec, infos.largeur, infos.hauteur, infos.ips, infos.pistes_audio), (Some(Codec::H264), 126, 224, 30, 1));
        assert_eq!(infos.largeur * 16, infos.hauteur * 9);
        let pixels = gris(&e.chemin);
        assert_eq!(pixels.len(), 126 * 224 * 120, "30 + 60 + 15 + 15 images");
        assert!((infos.duree_s - duree_finale(&m)).abs() <= 1.0 / 30.0 + 0.011, "durée {}", infos.duree_s);
        let ton = |image: usize, x: usize, y: usize| match pixels[image * 126 * 224 + y * 126 + x] {
            0..=12 => "noir",
            20..=48 => "bleu",
            60..=100 => "rouge",
            125..=175 => "vert",
            215..=255 => "blanc",
            autre => panic!("ton inattendu {autre} à l'image {image} ({x}, {y})"),
        };
        let milieu = |image: usize| ton(image, 20, 112);
        assert_eq!([0, 29, 30, 89, 90, 104, 105, 119].map(milieu), ["bleu", "bleu", "rouge", "rouge", "vert", "vert", "rouge", "rouge"]);
        assert_eq!((ton(60, 63, 112), ton(60, 63, 60), ton(110, 63, 112)), ("blanc", "rouge", "blanc"));
        assert_eq!((ton(95, 63, 8), ton(95, 63, 216), ton(95, 63, 40), ton(95, 4, 112), ton(95, 121, 112)), ("noir", "noir", "vert", "vert", "vert"));
        let premier = |image: usize| ton(image, 25, 190);
        let second = |image: usize| ton(image, 95, 30);
        assert_eq!([29, 30, 45, 59, 60].map(premier), ["bleu", "blanc", "blanc", "blanc", "rouge"]);
        assert_eq!([44, 45, 59, 100, 116, 117].map(second), ["rouge", "blanc", "blanc", "blanc", "blanc", "rouge"]);
        let piste = son(&e.chemin);
        let fort = |debut_s: f64, fin_s: f64| piste[(debut_s * 48000.0) as usize..(fin_s * 48000.0) as usize].iter().copied().max().unwrap_or(0);
        assert!(fort(0.2, 0.9) > 2000 && fort(1.2, 2.9) > 2000 && fort(2.9, 2.96) > 2000, "son avant le raccord");
        assert!(fort(3.04, 3.5) < 200 && fort(3.5, 3.9) < 200, "silence après le raccord : {}", fort(3.04, 3.5));
        assert!((piste.len() as f64 / 48000.0 - 4.0).abs() <= 0.04, "son {} s", piste.len() as f64 / 48000.0);
        for (fichier, original) in [&rouge, &vert, &bleu].iter().zip(&originaux) {
            assert_eq!(&fs::read(fichier).unwrap(), original);
        }
        let annexe = galerie::lire_annexe(&d, &e.chemin).unwrap();
        assert_eq!((annexe.largeur, annexe.hauteur, annexe.ips, annexe.duree_s, annexe.origine.as_deref()), (126, 224, 30, 4.0, Some("Bleu")));
        assert_eq!(fs::read_dir(d.join(galerie::ANNEXES)).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|x| x == "part")).count(), 0);

        let libre = Montage { segments: vec![de("Vert.mp4", 0.0, 1.0, 1.0, false), de("Rouge.mp4", 0.0, 1.0, 4.0, false)], textes: Vec::new(), cadre: Cadre::default(), sortie: Sortie { hauteur: Some(180), ips: Some(60) }, ..m.clone() };
        let e = exporter(&ffmpeg_local, &Demande { id: 2, montage: &libre, images: &[], ..demande }, &annule, &enfant, &mut |_, _| {}).unwrap();
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!((infos.codec, infos.largeur, infos.hauteur, infos.ips, gris(&e.chemin).len()), (Some(Codec::H264), 320, 180, 60, 320 * 180 * 75));

        fs::remove_file(&vert).unwrap();
        let ouverture = ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Projet("avance"), &mut |_, _| {}).unwrap();
        assert_eq!((ouverture.manquants.clone(), ouverture.clips.len(), ouverture.projet.as_ref()), (vec!["Vert.mp4".to_string()], 2, Some(&m)));
        assert_eq!(projets(&d).iter().find(|p| p.id == "avance").unwrap().manquants, ["Vert.mp4"]);
        fs::remove_file(&rouge).unwrap();
        fs::remove_file(&bleu).unwrap();
        assert!(ouvrir(&ffmpeg_local, &[encodeur], &d, &Cible::Projet("avance"), &mut |_, _| {}).unwrap_err().contains("introuvables"));
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
        let m = Montage { textes: vec![texte(0.0, 1.0)], ..montage(vec![segment(0.0, 4.0, true)], &[]) };
        let image = ffmpeg_local().unwrap().args(["-v", "error", "-f", "lavfi", "-i", "color=c=black@0:s=320x240,format=rgba", "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"]).output().unwrap().stdout;
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let demande = Demande { id: 1, dossier: &d, montage: &m, images: &[image], quota: 1 << 30, encodeurs: &[encodeur] };
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
        let liste = encodeurs_locaux();
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
