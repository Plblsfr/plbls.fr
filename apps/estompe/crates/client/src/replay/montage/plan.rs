use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::galerie::PISTES_MAX;
use super::super::types::Codec;
use super::geometrie::{cadre_pixels, dimensions_sortie, inscrire, toile};
use super::{
    Montage, Texte, CLIPS_MAX, DUREE_MAX_S, HABILLAGES, NOM_MAX, POLICES, RAPPORTS, SEGMENTS_MAX,
    TEXTES_MAX, TEXTE_MAX, VERSION, VITESSES, VOLUME_MAX,
};

pub const AUDIO_KBPS: u32 = 160;
pub const FORMAT_SON: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";
const IMAGE_MAX: usize = 16 << 20;

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
    pub recadre: Option<(u32, u32, u32, u32)>,
    pub entrees: Vec<Entree>,
    pub morceaux: Vec<Morceau>,
    pub textes: Vec<Option<(u64, u64)>>,
}

impl Plan {
    pub fn images(&self) -> u64 {
        self.morceaux.iter().map(|m| m.sortie).sum()
    }

    pub fn duree_s(&self) -> f64 {
        self.images() as f64 / f64::from(self.ips)
    }

    pub fn son(&self) -> bool {
        self.morceaux.iter().any(|m| m.son)
    }
}

fn source_valide(nom: &str) -> bool {
    nom.len() > 4
        && nom.to_ascii_lowercase().ends_with(".mp4")
        && Path::new(nom).file_name().is_some_and(|n| n == nom)
        && !nom.contains(['/', '\\', ':'])
}

fn fraction(x: f64) -> bool {
    x.is_finite() && (0.0..=1.0).contains(&x)
}

fn texte_valide(t: &Texte) -> bool {
    let couleur = t.couleur.len() == 7
        && t.couleur.starts_with('#')
        && t.couleur[1..].chars().all(|c| c.is_ascii_hexdigit());
    !t.contenu.trim().is_empty()
        && t.contenu.chars().count() <= TEXTE_MAX
        && POLICES.contains(&t.police.as_str())
        && HABILLAGES.iter().any(|(h, _)| *h == t.habillage)
        && couleur
        && fraction(t.x)
        && fraction(t.y)
        && t.taille.is_finite()
        && (0.01..=0.5).contains(&t.taille)
        && t.debut.is_finite()
        && t.fin.is_finite()
        && t.debut >= 0.0
        && t.fin > t.debut
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
    m.segments
        .iter()
        .filter(|s| s.garde)
        .map(|s| (s.fin - s.debut) / s.vitesse)
        .sum()
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
            return Err(
                "montage invalide : un segment désigne un fichier hors du dossier des clips".into(),
            );
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
        return Err(format!(
            "Un montage dure {} minutes au plus.",
            DUREE_MAX_S as u32 / 60
        ));
    }
    if m.volumes.keys().any(|k| !clips.contains(&k.as_str()))
        || m.volumes.values().any(|v| v.len() > PISTES_MAX)
    {
        return Err("montage invalide : volumes d'un fichier absent du montage".into());
    }
    if m.volumes
        .values()
        .flatten()
        .any(|v| !v.volume.is_finite() || !(0.0..=VOLUME_MAX).contains(&v.volume))
    {
        return Err("montage invalide : volume hors de 0 à 200 %".into());
    }
    if m.textes.len() > TEXTES_MAX || !m.textes.iter().all(texte_valide) {
        return Err("montage invalide : bloc de texte".into());
    }
    let c = &m.cadre;
    if ![c.x, c.y, c.l, c.h].into_iter().all(fraction)
        || c.l <= 0.0
        || c.h <= 0.0
        || c.x + c.l > 1.0001
        || c.y + c.h > 1.0001
        || !RAPPORTS.contains(&c.rapport.as_str())
    {
        return Err("montage invalide : cadre hors de l'image".into());
    }
    if m.sortie.hauteur.is_some_and(|h| !(2..=4320).contains(&h))
        || m.sortie.ips.is_some_and(|i| !(1..=240).contains(&i))
    {
        return Err("montage invalide : résolution ou cadence de sortie".into());
    }
    Ok(())
}

pub fn planifier(m: &Montage, infos: &BTreeMap<String, Infos>) -> Result<Plan, String> {
    verifier(m)?;
    let de = |nom: &str| {
        infos
            .get(nom)
            .filter(|i| i.ips > 0 && i.duree_s > 0.0)
            .ok_or_else(|| format!("cadence ou durée du clip « {nom} » inconnue"))
    };
    let premier = de(&m.segments[0].source)?;
    let toile = toile(premier.largeur, premier.hauteur);
    let cadre = cadre_pixels(&m.cadre, toile);
    let (largeur, hauteur) =
        dimensions_sortie((cadre.0, cadre.1), &m.cadre.rapport, m.sortie.hauteur);
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
        let cadence = f64::from(i.ips);
        if s.fin > i.duree_s + 1.0 / cadence {
            return Err("montage invalide : un segment dépasse la fin du clip".into());
        }
        let (a, b) = (
            (s.debut * cadence).round() as u64,
            ((s.fin * cadence).round() as u64).min((i.duree_s * cadence).round() as u64),
        );
        if b <= a {
            continue;
        }
        temps += (b - a) as f64 / cadence / s.vitesse;
        let sortie = ((temps * f64::from(ips)).round() as u64).saturating_sub(faites);
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
        let gains: Vec<(u32, f64)> = (0..i.pistes_audio.min(PISTES_MAX as u32))
            .filter_map(gain)
            .collect();
        let son = !s.muet && !gains.is_empty();
        if let Some(dernier) = morceaux.last_mut().filter(|d| {
            entrees[d.entree].source == s.source
                && d.images.1 == a
                && d.vitesse == s.vitesse
                && d.son == son
        }) {
            dernier.images.1 = b;
            dernier.sortie += sortie;
            atteint[dernier.entree] = b;
            continue;
        }
        let entree =
            match (0..entrees.len()).find(|e| entrees[*e].source == s.source && atteint[*e] <= a) {
                Some(e) => e,
                None => {
                    let (l, h, x, y) = inscrire(i.largeur, i.hauteur, toile);
                    let echelle = Some((l, h)).filter(|e| *e != (i.largeur, i.hauteur));
                    entrees.push(Entree {
                        source: s.source.clone(),
                        ips: i.ips,
                        echelle,
                        marge: ((l, h) != toile).then_some((x, y)),
                        gains,
                    });
                    atteint.push(0);
                    entrees.len() - 1
                }
            };
        atteint[entree] = b;
        morceaux.push(Morceau {
            entree,
            images: (a, b),
            vitesse: s.vitesse,
            son,
            sortie,
        });
    }
    if morceaux.is_empty() {
        return Err("rien à exporter : tous les segments sont retirés".into());
    }
    let total: u64 = morceaux.iter().map(|x| x.sortie).sum();
    let image = |t: f64| ((t * f64::from(ips)).round() as u64).min(total);
    let textes = m
        .textes
        .iter()
        .map(|t| Some((image(t.debut), image(t.fin))).filter(|(a, b)| b > a))
        .collect();
    Ok(Plan {
        ips,
        largeur,
        hauteur,
        toile,
        recadre: Some(cadre).filter(|c| (c.0, c.1) != toile),
        entrees,
        morceaux,
        textes,
    })
}

pub fn tempo(vitesse: f64) -> String {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parties<'a> {
    pub video: bool,
    pub son: bool,
    pub decalages: &'a [u64],
}

pub fn graphe(p: &Plan) -> String {
    graphe_partiel(
        p,
        Parties {
            video: true,
            son: true,
            decalages: &[],
        },
    )
}

pub(crate) fn graphe_partiel(p: &Plan, parties: Parties) -> String {
    let n = p.morceaux.len();
    let decalage = |e: usize| parties.decalages.get(e).copied().unwrap_or(0);
    let de = |e: usize| {
        p.morceaux
            .iter()
            .enumerate()
            .filter(move |(_, m)| m.entree == e)
    };
    let etiquettes = |e: usize, prefixe: &str, son: bool| {
        de(e)
            .filter(|(_, m)| !son || m.son)
            .map(|(i, _)| format!("[{prefixe}{i}]"))
            .collect::<String>()
    };
    let mut g: Vec<String> = Vec::new();
    if parties.video {
        for (e, entree) in p.entrees.iter().enumerate() {
            g.push(format!(
                "[{e}:v:0]fps={}:start_time=0,split={}{}",
                entree.ips,
                de(e).count(),
                etiquettes(e, "e", false)
            ));
        }
        for (i, m) in p.morceaux.iter().enumerate() {
            let entree = &p.entrees[m.entree];
            let d = decalage(m.entree);
            let mut f = format!(
                "[e{i}]trim=start_frame={}:end_frame={},setpts=(PTS-STARTPTS)/{},fps={},tpad=stop_mode=clone:stop=-1,trim=end_frame={}",
                m.images.0 - d,
                m.images.1 - d,
                m.vitesse,
                p.ips,
                m.sortie
            );
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
    }
    let son = parties.son && p.son();
    for (e, entree) in p
        .entrees
        .iter()
        .enumerate()
        .filter(|(e, _)| son && de(*e).any(|(_, m)| m.son))
    {
        g.extend(
            entree
                .gains
                .iter()
                .map(|(i, v)| format!("[{e}:a:{i}]volume={v:.3}[p{e}x{i}]")),
        );
        let pistes: String = entree
            .gains
            .iter()
            .map(|(i, _)| format!("[p{e}x{i}]"))
            .collect();
        let mixe = if entree.gains.len() == 1 {
            pistes
        } else {
            g.push(format!(
                "{pistes}amix=inputs={}:normalize=0:duration=longest[m{e}]",
                entree.gains.len()
            ));
            format!("[m{e}]")
        };
        g.push(format!(
            "{mixe}asplit={}{}",
            de(e).filter(|(_, m)| m.son).count(),
            etiquettes(e, "b", true)
        ));
    }
    for (i, m) in p.morceaux.iter().enumerate().filter(|_| son) {
        let (cadence, duree) = (
            f64::from(p.entrees[m.entree].ips),
            m.sortie as f64 / f64::from(p.ips),
        );
        let d = decalage(m.entree);
        g.push(if m.son {
            format!(
                "[b{i}]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS{},{FORMAT_SON},apad=whole_dur={duree:.6},atrim=end={duree:.6}[a{i}]",
                (m.images.0 - d) as f64 / cadence,
                (m.images.1 - d) as f64 / cadence,
                tempo(m.vitesse)
            )
        } else {
            format!("aevalsrc=0|0:s=48000:d={duree:.6},{FORMAT_SON}[a{i}]")
        });
    }
    let entrees: String = (0..n)
        .map(|i| match (parties.video, son) {
            (true, true) => format!("[v{i}][a{i}]"),
            (true, false) => format!("[v{i}]"),
            (false, _) => format!("[a{i}]"),
        })
        .collect();
    if !parties.video {
        g.push(format!("{entrees}concat=n={n}:v=0:a=1[a]"));
        return g.join(";");
    }
    let incrustes: Vec<(u64, u64)> = p.textes.iter().flatten().copied().collect();
    let apres = |j: usize| {
        if j == incrustes.len() {
            "[v]".to_string()
        } else {
            format!("[t{j}]")
        }
    };
    g.push(format!(
        "{entrees}concat=n={n}:v=1:a={}{}{}",
        u8::from(son),
        apres(0),
        if son { "[a]" } else { "" }
    ));
    for (j, (a, b)) in incrustes.iter().enumerate() {
        g.push(format!(
            "[t{j}][{}:v]overlay=0:0:enable='between(n,{a},{})'{}",
            p.entrees.len() + j,
            b - 1,
            apres(j + 1)
        ));
    }
    g.join(";")
}

pub fn debit_export(taille: u64, duree_s: f64, codec: Option<Codec>) -> u32 {
    let source = taille as f64 * 8.0 / 1000.0 / duree_s.max(1.0);
    (source * if codec == Some(Codec::H264) { 1.0 } else { 1.5 }).clamp(2000.0, 50_000.0) as u32
}

pub(crate) fn options_video(encodeur: &str, kbps: u32, ips: u32) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-c:v".into(),
        encodeur.into(),
        "-b:v".into(),
        format!("{kbps}k"),
        "-maxrate".into(),
        format!("{kbps}k"),
        "-bufsize".into(),
        format!("{}k", kbps * 2),
    ];
    a.extend([
        "-g".into(),
        ips.to_string(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-r".into(),
        ips.to_string(),
        "-fps_mode".into(),
        "cfr".into(),
    ]);
    a
}

pub(crate) fn entree(source: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-nostats", "-progress", "pipe:1", "-i"]
        .map(String::from)
        .into();
    a.push(source.to_string_lossy().into_owned());
    a
}

pub(crate) fn sortie_mp4(mut a: Vec<String>, sortie: &Path) -> Vec<String> {
    a.extend(["-movflags", "+faststart", "-f", "mp4", "-y"].map(String::from));
    a.push(sortie.to_string_lossy().into_owned());
    a
}

pub fn arguments_export(
    dossier: &Path,
    images: &[PathBuf],
    sortie: &Path,
    p: &Plan,
    encodeur: &str,
    kbps: u32,
) -> Vec<String> {
    let mut a = entree(&dossier.join(&p.entrees[0].source));
    for e in &p.entrees[1..] {
        a.extend([
            "-i".into(),
            dossier.join(&e.source).to_string_lossy().into_owned(),
        ]);
    }
    for image in images {
        a.extend([
            "-f".into(),
            "png_pipe".into(),
            "-i".into(),
            image.to_string_lossy().into_owned(),
        ]);
    }
    a.extend([
        "-filter_complex".into(),
        graphe(p),
        "-map".into(),
        "[v]".into(),
    ]);
    if p.son() {
        a.extend(["-map", "[a]", "-c:a", "aac", "-ac", "2"].map(String::from));
        a.extend(["-b:a".into(), format!("{AUDIO_KBPS}k")]);
    }
    a.extend(options_video(encodeur, kbps, p.ips));
    sortie_mp4(a, sortie)
}

pub fn dimensions_png(octets: &[u8]) -> Option<(u32, u32)> {
    let lire = |i: usize| Some(u32::from_be_bytes(octets.get(i..i + 4)?.try_into().ok()?));
    (octets.len() <= IMAGE_MAX
        && octets.starts_with(b"\x89PNG\r\n\x1a\n")
        && octets.get(12..16)? == b"IHDR")
        .then_some(())?;
    Some((lire(16)?, lire(20)?))
}

pub fn analyser(texte: &str) -> Option<Infos> {
    let flux = |genre: &'static str| {
        texte
            .lines()
            .filter(move |l| l.contains("Stream #") && l.contains(genre))
    };
    let video = flux(": Video:").find(|l| !l.contains("attached pic"))?;
    let mots: Vec<&str> = video.split([' ', ',']).filter(|m| !m.is_empty()).collect();
    let apres = |cle: &str| {
        mots.iter()
            .position(|m| *m == cle)
            .and_then(|i| mots.get(i + 1))
            .copied()
    };
    let avant = |cle: &str| {
        mots.iter()
            .position(|m| *m == cle)
            .and_then(|i| mots.get(i.checked_sub(1)?))
            .and_then(|m| m.parse::<f64>().ok())
    };
    let (largeur, hauteur) = mots.iter().find_map(|m| {
        let (l, h) = m.split_once('x')?;
        Some((l.parse::<u32>().ok()?, h.parse::<u32>().ok()?)).filter(|(l, h)| *l >= 2 && *h >= 2)
    })?;
    let codec = match apres("Video:") {
        Some("h264") => Some(Codec::H264),
        Some("hevc") => Some(Codec::Hevc),
        _ => None,
    };
    let duree_s = crate::video::duree_ffmpeg(texte);
    let ips = avant("fps")
        .or_else(|| avant("tbr"))
        .map_or(0, |i| i.round() as u32);
    (duree_s > 0.0 && ips > 0).then_some(Infos {
        duree_s,
        codec,
        largeur,
        hauteur,
        ips,
        pistes_audio: flux(": Audio:").count() as u32,
    })
}

pub fn progression(ligne: &str, duree_s: f64) -> Option<u8> {
    let micros: f64 = ligne.strip_prefix("out_time_us=")?.trim().parse().ok()?;
    Some((micros / 1e6 / duree_s.max(0.001) * 100.0).clamp(0.0, 99.0) as u8)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{Cadre, Segment, Sortie, Volume};
    use super::*;

    pub const CLIP: &str = "Jeu.mp4";

    pub fn segment(debut: f64, fin: f64, garde: bool) -> Segment {
        Segment {
            source: CLIP.into(),
            debut,
            fin,
            garde,
            vitesse: 1.0,
            muet: false,
        }
    }

    pub fn de(source: &str, debut: f64, fin: f64, vitesse: f64, muet: bool) -> Segment {
        Segment {
            source: source.into(),
            debut,
            fin,
            garde: true,
            vitesse,
            muet,
        }
    }

    pub fn montage(segments: Vec<Segment>, volumes: &[(f64, bool)]) -> Montage {
        let volumes: Vec<Volume> = volumes
            .iter()
            .map(|(volume, muet)| Volume {
                volume: *volume,
                muet: *muet,
            })
            .collect();
        let volumes = if volumes.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([(CLIP.to_string(), volumes)])
        };
        Montage {
            version: VERSION,
            nom: "Jeu".into(),
            segments,
            volumes,
            textes: Vec::new(),
            cadre: Cadre::default(),
            sortie: Sortie::default(),
        }
    }

    pub fn texte(debut: f64, fin: f64) -> Texte {
        Texte {
            contenu: "Bien joué 🎉".into(),
            police: POLICES[0].into(),
            taille: 0.08,
            couleur: "#ffffff".into(),
            habillage: "contour".into(),
            x: 0.5,
            y: 0.8,
            debut,
            fin,
        }
    }

    fn infos(pistes_audio: u32) -> Infos {
        Infos {
            duree_s: 10.0,
            codec: Some(Codec::H264),
            largeur: 1280,
            hauteur: 720,
            ips: 60,
            pistes_audio,
        }
    }

    fn connus(pistes_audio: u32) -> BTreeMap<String, Infos> {
        let hevc = Infos {
            duree_s: 8.0,
            codec: Some(Codec::Hevc),
            largeur: 2560,
            hauteur: 1440,
            ips: 144,
            pistes_audio: 1,
        };
        let carre = Infos {
            duree_s: 5.0,
            codec: Some(Codec::H264),
            largeur: 1000,
            hauteur: 1000,
            ips: 30,
            pistes_audio: 0,
        };
        BTreeMap::from([
            (CLIP.to_string(), infos(pistes_audio)),
            ("Hevc.mp4".to_string(), hevc),
            ("Carre.mp4".to_string(), carre),
        ])
    }

    pub fn trois_segments() -> Montage {
        montage(
            vec![
                segment(0.5, 2.0, true),
                segment(2.0, 3.0, false),
                segment(3.0, 5.0, true),
                segment(5.0, 7.0, false),
                segment(7.0, 9.5, true),
            ],
            &[(0.5, false), (1.0, true)],
        )
    }

    fn images(p: &Plan) -> Vec<(u64, u64)> {
        p.morceaux.iter().map(|m| m.images).collect()
    }

    #[test]
    fn montage_recu_jamais_cru_sur_parole() {
        assert!(verifier(&trois_segments()).is_ok());
        assert!(verifier(&Montage {
            version: 1,
            ..trois_segments()
        })
        .is_err());
        assert!(verifier(&montage(Vec::new(), &[])).is_err());
        assert!(verifier(&montage(
            vec![segment(0.0, 1.0, true); SEGMENTS_MAX + 1],
            &[]
        ))
        .is_err());
        for (debut, fin) in [
            (2.0, 1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(
                verifier(&montage(vec![segment(debut, fin, true)], &[])).is_err(),
                "{debut} {fin}"
            );
        }
        assert!(verifier(&montage(
            vec![
                segment(2.0, 3.0, true),
                segment(0.0, 1.0, true),
                segment(0.5, 2.5, true)
            ],
            &[]
        ))
        .is_ok());
        for volume in [2.01, -0.1, f64::NAN] {
            assert!(verifier(&montage(vec![segment(0.0, 1.0, true)], &[(volume, false)])).is_err());
        }
        for source in [
            "..\\dehors.mp4",
            "C:\\dehors.mp4",
            "sous/dossier.mp4",
            "../dehors.mp4",
            "Jeu.txt",
            ".mp4",
            "",
        ] {
            assert!(
                verifier(&montage(vec![de(source, 0.0, 1.0, 1.0, false)], &[])).is_err(),
                "{source}"
            );
        }
        let mut volumes = trois_segments();
        volumes.volumes.insert("Autre.mp4".into(), Vec::new());
        assert!(verifier(&volumes).is_err());
        for vitesse in [0.0, 3.0, -1.0, f64::NAN] {
            assert!(
                verifier(&montage(vec![de(CLIP, 0.0, 1.0, vitesse, false)], &[])).is_err(),
                "{vitesse}"
            );
        }
        let clips = |n: usize| {
            montage(
                (0..n)
                    .map(|i| de(&format!("c{i}.mp4"), 0.0, 1.0, 1.0, false))
                    .collect(),
                &[],
            )
        };
        assert!(
            verifier(&clips(CLIPS_MAX)).is_ok()
                && verifier(&clips(CLIPS_MAX + 1))
                    .unwrap_err()
                    .contains("20 clips")
        );
        assert!(verifier(&montage(vec![de(CLIP, 0.0, 450.0, 0.25, false)], &[])).is_ok());
        assert!(
            verifier(&montage(vec![de(CLIP, 0.0, 451.0, 0.25, false)], &[]))
                .unwrap_err()
                .contains("30 minutes")
        );
        assert!(verifier(&montage(
            vec![
                de(CLIP, 0.0, 1801.0, 1.0, false),
                segment(0.0, 5000.0, false)
            ],
            &[]
        ))
        .is_err());
        let cadre = |x, y, l, h, rapport: &str| {
            verifier(&Montage {
                cadre: Cadre {
                    x,
                    y,
                    l,
                    h,
                    rapport: rapport.into(),
                },
                ..trois_segments()
            })
        };
        assert!(
            cadre(0.25, 0.0, 0.5, 1.0, "9:16").is_ok()
                && cadre(0.0, 0.0, 1.0, 1.0, "libre").is_ok()
        );
        assert!(
            cadre(0.6, 0.0, 0.5, 1.0, "libre").is_err()
                && cadre(0.0, 0.0, 0.0, 1.0, "libre").is_err()
                && cadre(0.0, -0.1, 1.0, 1.0, "libre").is_err()
        );
        assert!(
            cadre(0.0, 0.0, f64::NAN, 1.0, "libre").is_err()
                && cadre(0.0, 0.0, 1.0, 1.0, "21:9").is_err()
        );
        let avec = |t: Texte| {
            verifier(&Montage {
                textes: vec![t],
                ..trois_segments()
            })
        };
        assert!(
            avec(texte(2.0, 5.0)).is_ok()
                && avec(Texte {
                    contenu: "é".repeat(TEXTE_MAX),
                    ..texte(0.0, 1.0)
                })
                .is_ok()
        );
        assert!(
            avec(texte(5.0, 2.0)).is_err()
                && avec(Texte {
                    contenu: " ".into(),
                    ..texte(0.0, 1.0)
                })
                .is_err()
        );
        assert!(avec(Texte {
            contenu: "é".repeat(TEXTE_MAX + 1),
            ..texte(0.0, 1.0)
        })
        .is_err());
        assert!(
            avec(Texte {
                police: "Comic Sans MS".into(),
                ..texte(0.0, 1.0)
            })
            .is_err()
                && avec(Texte {
                    couleur: "rouge".into(),
                    ..texte(0.0, 1.0)
                })
                .is_err()
        );
        assert!(
            avec(Texte {
                x: 1.5,
                ..texte(0.0, 1.0)
            })
            .is_err()
                && avec(Texte {
                    taille: 0.9,
                    ..texte(0.0, 1.0)
                })
                .is_err()
        );
        assert!(avec(Texte {
            habillage: "ombre".into(),
            ..texte(0.0, 1.0)
        })
        .is_err());
        assert!(verifier(&Montage {
            textes: vec![texte(0.0, 1.0); TEXTES_MAX + 1],
            ..trois_segments()
        })
        .is_err());
        assert!(verifier(&Montage {
            sortie: Sortie {
                hauteur: Some(0),
                ips: None
            },
            ..trois_segments()
        })
        .is_err());
        assert!(verifier(&Montage {
            sortie: Sortie {
                hauteur: None,
                ips: Some(1000)
            },
            ..trois_segments()
        })
        .is_err());
        assert!(verifier(&Montage {
            nom: "x".repeat(NOM_MAX + 1),
            ..trois_segments()
        })
        .is_err());
        assert!(planifier(&montage(vec![segment(0.0, 11.0, true)], &[]), &connus(1)).is_err());
        assert!(
            planifier(&montage(vec![segment(0.0, 5.0, false)], &[]), &connus(1))
                .unwrap_err()
                .contains("rien à exporter")
        );
        assert!(planifier(
            &trois_segments(),
            &BTreeMap::from([(CLIP.to_string(), Infos { ips: 0, ..infos(2) })])
        )
        .is_err());
        assert!(planifier(
            &montage(vec![de("Absent.mp4", 0.0, 1.0, 1.0, false)], &[]),
            &connus(1)
        )
        .unwrap_err()
        .contains("Absent.mp4"));
        assert_eq!(
            planifier(
                &Montage {
                    sortie: Sortie {
                        hauteur: None,
                        ips: Some(120)
                    },
                    ..trois_segments()
                },
                &connus(1)
            )
            .unwrap()
            .ips,
            60
        );
    }

    #[test]
    fn plan_a_l_image_pres() {
        let p = planifier(&trois_segments(), &connus(2)).unwrap();
        assert_eq!(images(&p), [(30, 120), (180, 300), (420, 570)]);
        assert_eq!(
            (
                p.entrees.len(),
                p.entrees[0].gains.clone(),
                p.entrees[0].echelle,
                p.entrees[0].marge
            ),
            (1, vec![(0, 0.5)], None, None)
        );
        assert_eq!(
            (p.duree_s(), p.ips, p.largeur, p.hauteur, p.recadre, p.son()),
            (6.0, 60, 1280, 720, None, true)
        );
        let colles = planifier(
            &montage(
                vec![
                    segment(0.0, 1.0, true),
                    segment(1.0, 2.0, true),
                    segment(2.0, 2.004, true),
                    segment(4.0, 10.01, true),
                ],
                &[],
            ),
            &connus(2),
        )
        .unwrap();
        assert_eq!(
            (images(&colles), colles.entrees[0].gains.clone()),
            (vec![(0, 120), (240, 600)], vec![(0, 1.0), (1, 1.0)])
        );
        assert!(planifier(&trois_segments(), &connus(0))
            .unwrap()
            .morceaux
            .iter()
            .all(|m| !m.son));
        assert!(planifier(
            &montage(vec![segment(0.0, 1.0, true)], &[(0.0, false), (1.5, true)]),
            &connus(2)
        )
        .unwrap()
        .morceaux
        .iter()
        .all(|m| !m.son));
        assert_eq!(
            planifier(
                &montage(vec![segment(0.0, 1.0, true)], &[(2.0, false)]),
                &connus(2)
            )
            .unwrap()
            .entrees[0]
                .gains,
            [(0, 2.0), (1, 1.0)]
        );
    }

    #[test]
    fn plan_vitesses_ordre_et_textes() {
        let vitesses = montage(
            vec![
                de(CLIP, 0.0, 1.0, 0.5, false),
                de(CLIP, 1.0, 2.0, 2.0, false),
                de(CLIP, 2.0, 3.0, 0.25, true),
                de(CLIP, 3.0, 4.0, 4.0, false),
                de(CLIP, 4.0, 5.0, 1.5, false),
            ],
            &[],
        );
        let p = planifier(&vitesses, &connus(1)).unwrap();
        assert_eq!(
            p.morceaux.iter().map(|m| m.sortie).collect::<Vec<_>>(),
            [120, 30, 240, 15, 40]
        );
        assert_eq!(
            (p.images(), p.duree_s(), duree_finale(&vitesses)),
            (445, 445.0 / 60.0, 2.0 + 0.5 + 4.0 + 0.25 + 1.0 / 1.5)
        );
        assert_eq!(
            p.morceaux.iter().map(|m| m.son).collect::<Vec<_>>(),
            [true, true, false, true, true]
        );
        let trente = planifier(
            &Montage {
                sortie: Sortie {
                    hauteur: None,
                    ips: Some(30),
                },
                ..montage(
                    vec![
                        de(CLIP, 0.0, 0.05, 1.0, false),
                        de(CLIP, 1.0, 1.05, 1.5, false),
                        de(CLIP, 2.0, 2.05, 1.0, false),
                    ],
                    &[],
                )
            },
            &connus(1),
        )
        .unwrap();
        assert_eq!(
            (
                trente.ips,
                trente.morceaux.iter().map(|m| m.sortie).collect::<Vec<_>>(),
                trente.images()
            ),
            (30, vec![2, 1, 1], 4)
        );
        let melange = montage(
            vec![
                de("Hevc.mp4", 1.0, 2.0, 1.0, false),
                de(CLIP, 2.0, 3.0, 1.0, false),
                de("Carre.mp4", 0.0, 1.0, 1.0, false),
                de(CLIP, 0.0, 1.0, 1.0, false),
                de(CLIP, 5.0, 6.0, 1.0, false),
                de("Hevc.mp4", 4.0, 5.0, 2.0, false),
            ],
            &[],
        );
        let p = planifier(&melange, &connus(2)).unwrap();
        assert_eq!(
            p.entrees
                .iter()
                .map(|e| (e.source.as_str(), e.ips))
                .collect::<Vec<_>>(),
            [("Hevc.mp4", 144), (CLIP, 60), ("Carre.mp4", 30), (CLIP, 60)]
        );
        assert_eq!(
            p.morceaux.iter().map(|m| m.entree).collect::<Vec<_>>(),
            [0, 1, 2, 3, 1, 0]
        );
        assert_eq!(
            (images(&p)[0], images(&p)[2], images(&p)[5]),
            ((144, 288), (0, 30), (576, 720))
        );
        assert_eq!(
            (p.toile, p.ips, p.largeur, p.hauteur, p.images()),
            ((2560, 1440), 144, 2560, 1440, 144 * 5 + 72)
        );
        assert_eq!(
            (
                p.entrees[0].echelle,
                p.entrees[0].marge,
                p.entrees[1].echelle,
                p.entrees[1].marge
            ),
            (None, None, Some((2560, 1440)), None)
        );
        assert_eq!(
            (p.entrees[2].echelle, p.entrees[2].marge, p.morceaux[2].son),
            (Some((1440, 1440)), Some((560, 0)), false)
        );
        let textes = Montage {
            textes: vec![
                texte(2.0, 5.0),
                texte(4.0, 99.0),
                texte(7.0, 8.0),
                texte(0.0, 0.004),
            ],
            ..trois_segments()
        };
        assert_eq!(
            planifier(&textes, &connus(1)).unwrap().textes,
            [Some((120, 300)), Some((240, 360)), None, None]
        );
        let vertical = Montage {
            cadre: Cadre {
                x: 0.341_796_875,
                y: 0.0,
                l: 0.316_406_25,
                h: 1.0,
                rapport: "9:16".into(),
            },
            sortie: Sortie {
                hauteur: Some(1080),
                ips: Some(60),
            },
            ..melange
        };
        let p = planifier(&vertical, &connus(1)).unwrap();
        assert_eq!(
            (p.recadre, p.largeur, p.hauteur, p.ips),
            (Some((810, 1440, 876, 0)), 594, 1056, 60)
        );
    }

    #[test]
    fn arguments_ffmpeg_de_l_export() {
        let p = planifier(&trois_segments(), &connus(2)).unwrap();
        let g = graphe(&p);
        assert!(g.starts_with("[0:v:0]fps=60:start_time=0,split=3[e0][e1][e2];[e0]trim=start_frame=30:end_frame=120,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=90,setsar=1[v0];"), "{g}");
        assert!(g.contains("[e2]trim=start_frame=420:end_frame=570,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=150,setsar=1[v2]"));
        assert!(
            g.contains("[0:a:0]volume=0.500[p0x0];[p0x0]asplit=3[b0][b1][b2]")
                && !g.contains("0:a:1")
                && !g.contains("amix")
                && !g.contains("scale")
                && !g.contains("crop")
                && !g.contains("overlay")
        );
        assert!(g.contains(&format!("[b1]atrim=start=3.000000:end=5.000000,asetpts=PTS-STARTPTS,{FORMAT_SON},apad=whole_dur=2.000000,atrim=end=2.000000[a1]")));
        assert!(g.ends_with("[v0][a0][v1][a1][v2][a2]concat=n=3:v=1:a=1[v][a]"));
        let mut deux = p.clone();
        deux.entrees[0].gains = vec![(0, 0.5), (1, 2.0)];
        assert!(graphe(&deux).contains("[0:a:0]volume=0.500[p0x0];[0:a:1]volume=2.000[p0x1];[p0x0][p0x1]amix=inputs=2:normalize=0:duration=longest[m0];[m0]asplit=3"));
        let muet =
            graphe(&planifier(&montage(vec![segment(0.0, 1.0, true)], &[]), &connus(0)).unwrap());
        assert_eq!(muet, "[0:v:0]fps=60:start_time=0,split=1[e0];[e0]trim=start_frame=0:end_frame=60,setpts=(PTS-STARTPTS)/1,fps=60,tpad=stop_mode=clone:stop=-1,trim=end_frame=60,setsar=1[v0];[v0]concat=n=1:v=1:a=0[v]");
        let avance = Montage {
            segments: vec![
                de("Hevc.mp4", 0.0, 1.0, 0.25, false),
                de(CLIP, 2.0, 3.0, 4.0, true),
                de("Carre.mp4", 0.0, 1.0, 1.5, false),
                de(CLIP, 0.0, 1.0, 1.0, false),
            ],
            textes: vec![texte(2.0, 5.0), texte(90.0, 91.0), texte(4.0, 4.5)],
            cadre: Cadre {
                x: 0.341_796_875,
                y: 0.0,
                l: 0.316_406_25,
                h: 1.0,
                rapport: "9:16".into(),
            },
            sortie: Sortie {
                hauteur: Some(1080),
                ips: Some(60),
            },
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
        assert_eq!(
            (tempo(2.0), tempo(1.5), tempo(4.0), tempo(0.5)),
            (
                ",atempo=2".to_string(),
                ",atempo=1.5".to_string(),
                ",atempo=2,atempo=2".to_string(),
                ",atempo=0.5".to_string()
            )
        );
        let a = arguments_export(
            Path::new("d"),
            &[PathBuf::from("t0.part"), PathBuf::from("t2.part")],
            Path::new("out.part"),
            &p,
            "h264_amf",
            12_000,
        );
        let paire = |x: &str, y: &str| a.windows(2).any(|w| w == [x, y]);
        let clip = |nom: &str| Path::new("d").join(nom).to_string_lossy().into_owned();
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 6);
        assert!(
            paire("-progress", "pipe:1")
                && paire("-i", &clip("Hevc.mp4"))
                && paire("-i", &clip("Carre.mp4"))
                && paire("-filter_complex", &g)
                && paire("-map", "[v]")
                && paire("-map", "[a]")
        );
        assert!(
            a.windows(4)
                .any(|w| w == ["-f", "png_pipe", "-i", "t2.part"])
                && a.iter().position(|x| x == "t0.part") > a.iter().rposition(|x| *x == clip(CLIP))
        );
        assert!(
            paire("-c:v", "h264_amf")
                && paire("-b:v", "12000k")
                && paire("-c:a", "aac")
                && paire("-r", "60")
                && paire("-fps_mode", "cfr")
                && paire("-pix_fmt", "yuv420p")
        );
        assert!(
            paire("-movflags", "+faststart")
                && paire("-f", "mp4")
                && a.last().unwrap() == "out.part"
        );
        let sans_son = arguments_export(
            Path::new("d"),
            &[],
            Path::new("o"),
            &planifier(&trois_segments(), &connus(0)).unwrap(),
            "libopenh264",
            2000,
        );
        assert!(
            !sans_son.contains(&"[a]".to_string())
                && !sans_son.contains(&"-c:a".to_string())
                && !sans_son.contains(&"png_pipe".to_string())
        );
        assert_eq!(debit_export(75_000_000, 30.0, Some(Codec::H264)), 20_000);
        assert_eq!(debit_export(75_000_000, 30.0, Some(Codec::Hevc)), 30_000);
        assert_eq!(
            (
                debit_export(1000, 30.0, None),
                debit_export(u64::MAX / 16, 1.0, None)
            ),
            (2000, 50_000)
        );
    }

    #[test]
    fn graphe_partiel_de_l_apercu() {
        let p = planifier(&trois_segments(), &connus(2)).unwrap();
        let video = graphe_partiel(
            &p,
            Parties {
                video: true,
                son: false,
                decalages: &[30],
            },
        );
        assert!(
            video.contains("[e0]trim=start_frame=0:end_frame=90,")
                && video.contains("[e2]trim=start_frame=390:end_frame=540,"),
            "{video}"
        );
        assert!(video.ends_with("[v0][v1][v2]concat=n=3:v=1:a=0[v]") && !video.contains("atrim"));
        let son = graphe_partiel(
            &p,
            Parties {
                video: false,
                son: true,
                decalages: &[30],
            },
        );
        assert!(
            !son.contains("fps=")
                && son.contains("[b0]atrim=start=0.000000:end=1.500000,")
                && son.ends_with("[a0][a1][a2]concat=n=3:v=0:a=1[a]"),
            "{son}"
        );
    }

    #[test]
    fn dimensions_d_une_image_png() {
        let png = |l: u32, h: u32| {
            [
                &b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..],
                &l.to_be_bytes(),
                &h.to_be_bytes(),
            ]
            .concat()
        };
        assert_eq!(
            (
                dimensions_png(&png(594, 1056)),
                dimensions_png(b"GIF89a"),
                dimensions_png(&png(1, 1)[..20])
            ),
            (Some((594, 1056)), None, None)
        );
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
        let ralenti = planifier(
            &montage(
                vec![
                    de(CLIP, 0.0, 2.0, 0.25, false),
                    de(CLIP, 2.0, 6.0, 2.0, false),
                ],
                &[],
            ),
            &connus(1),
        )
        .unwrap();
        assert_eq!(
            (
                ralenti.duree_s(),
                progression("out_time_us=5000000", ralenti.duree_s())
            ),
            (10.0, Some(50))
        );
    }

    #[test]
    fn flux_lus_dans_la_sortie_de_ffmpeg() {
        let texte = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'a.mp4':\n  Duration: 00:00:10.00, start: 0.000000, bitrate: 304 kb/s\n  Stream #0:0[0x1](und): Video: hevc (Main) (hvc1 / 0x31766568), yuv420p(tv, progressive), 2560x1440 [SAR 1:1 DAR 16:9], 87 kb/s, 144 fps, 144 tbr, 15360 tbn (default)\n  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)\n  Stream #0:2[0x3](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, mono, fltp, 69 kb/s\n";
        assert_eq!(
            analyser(texte),
            Some(Infos {
                duree_s: 10.0,
                codec: Some(Codec::Hevc),
                largeur: 2560,
                hauteur: 1440,
                ips: 144,
                pistes_audio: 2
            })
        );
        let h264 = "  Duration: 00:01:02.50, start: 0.0\n  Stream #0:0: Video: h264 (High) (avc1 / 0x31637661), yuv420p, 1920x1080, 59.94 tbr, 90k tbn\n";
        assert_eq!(
            analyser(h264),
            Some(Infos {
                duree_s: 62.5,
                codec: Some(Codec::H264),
                largeur: 1920,
                hauteur: 1080,
                ips: 60,
                pistes_audio: 0
            })
        );
        assert_eq!(analyser("  Duration: 00:00:05.00\n  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv), 640x360, 30 fps\n").unwrap().codec, None);
        assert_eq!(
            analyser("a.mp4: Invalid data found when processing input"),
            None
        );
        assert_eq!(
            analyser("  Duration: 00:00:05.00\n  Stream #0:0: Audio: aac, 48000 Hz, stereo\n"),
            None
        );
    }
}
