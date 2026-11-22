use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::super::galerie::{self, Annexe, Horodatage};
use super::super::types::Codec;
use super::edition::pics;
use super::plan::{
    analyser, arguments_export, debit_export, dimensions_png, planifier, progression, sources,
    verifier, Entree, Infos, AUDIO_KBPS,
};
use super::{Montage, LOGICIELS};

const ARGUMENTS_MAX: usize = 30_000;

pub type Enfant = Arc<Mutex<Option<Child>>>;
pub type Ffmpeg<'a> = &'a dyn Fn() -> Result<Command, String>;

pub fn nom_montage(dossier: &Path, nom: &str) -> PathBuf {
    (1u32..)
        .map(|n| {
            dossier.join(if n == 1 {
                format!("{nom} (montage).mp4")
            } else {
                format!("{nom} (montage {n}).mp4")
            })
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dossier.join(format!("{nom} (montage).mp4")))
}

pub fn disque_plein(erreur: &str) -> bool {
    [
        "No space left on device",
        "not enough space on the disk",
        "Disk quota exceeded",
    ]
    .iter()
    .any(|m| erreur.contains(m))
}

fn derniere_ligne(erreur: &str) -> String {
    erreur
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("échec de ffmpeg")
        .trim()
        .to_string()
}

pub fn message_disque(erreur: String, quoi: &str) -> String {
    if disque_plein(&erreur) {
        format!("{quoi} impossible : le disque est plein.")
    } else {
        format!("{quoi} impossible : {}", derniere_ligne(&erreur))
    }
}

pub fn purger_partiels(dossier: &Path) {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES))
        .into_iter()
        .flatten()
        .flatten();
    for e in entrees.filter(|e| {
        e.file_name().to_string_lossy().starts_with("export-")
            && e.path().extension().is_some_and(|x| x == galerie::PARTIEL)
    }) {
        let _ = fs::remove_file(e.path());
    }
}

pub fn encodeurs_export(liste: &str) -> Vec<String> {
    let mut encodeurs = crate::video::encodeurs_materiels(liste);
    encodeurs.extend(
        LOGICIELS
            .into_iter()
            .rev()
            .filter(|e| liste.split_whitespace().any(|mot| mot == *e))
            .map(String::from),
    );
    encodeurs
}

fn executer(
    mut commande: Command,
    duree_s: f64,
    enfant: &Enfant,
    annule: &AtomicBool,
    progres: &mut dyn FnMut(u8),
) -> Result<(), String> {
    let mut processus = commande
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
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
    for ligne in sortie
        .into_iter()
        .flat_map(|s| BufReader::new(s).lines())
        .map_while(Result::ok)
    {
        if let Some(p) = progression(&ligne, duree_s) {
            progres(p);
        }
    }
    let texte = lecteur.join().unwrap_or_default();
    let processus = enfant.lock().map_err(|e| e.to_string())?.take();
    let statut = processus
        .ok_or("processus ffmpeg perdu")?
        .wait()
        .map_err(|e| e.to_string())?;
    if statut.success() {
        Ok(())
    } else {
        Err(if texte.trim().is_empty() {
            "échec de ffmpeg".into()
        } else {
            texte
        })
    }
}

pub fn arreter(enfant: &Enfant) {
    if let Ok(mut e) = enfant.lock() {
        if let Some(processus) = e.as_mut() {
            let _ = processus.kill();
        }
    }
}

fn lancer(
    ffmpeg: Ffmpeg,
    arguments: Vec<String>,
    duree_s: f64,
    enfant: &Enfant,
    annule: &AtomicBool,
    progres: &mut dyn FnMut(u8),
) -> Result<(), String> {
    let mut commande = ffmpeg()?;
    commande.args(arguments);
    executer(commande, duree_s, enfant, annule, progres)
}

pub fn sonder(ffmpeg: Ffmpeg, clip: &Path) -> Result<Infos, String> {
    fs::File::open(clip).map_err(|e| format!("Clip illisible : {e}."))?;
    let sortie = ffmpeg()?
        .arg("-i")
        .arg(clip)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    analyser(&String::from_utf8_lossy(&sortie.stderr)).ok_or_else(|| {
        "Clip illisible : ce fichier n'est pas une vidéo que l'éditeur sait ouvrir.".to_string()
    })
}

pub fn onde(ffmpeg: Ffmpeg, clip: &Path, piste: usize, barres: usize) -> Result<Vec<f32>, String> {
    let sortie = ffmpeg()?
        .args(["-v", "error", "-i"])
        .arg(clip)
        .args(["-map", &format!("0:a:{piste}"), "-ac", "1", "-ar", "4000"])
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !sortie.status.success() {
        return Err("piste audio illisible".into());
    }
    let echantillons: Vec<f32> = sortie
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|o| f32::from_le_bytes(*o))
        .collect();
    Ok(pics(&[&echantillons], barres))
}

pub struct Demande<'a> {
    pub id: u64,
    pub dossier: &'a Path,
    pub montage: &'a Montage,
    pub images: &'a [Vec<u8>],
    pub quota: u64,
    pub encodeurs: &'a [String],
    pub date: Horodatage,
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

fn dossier_clip(dossier: &Path, nom: &str) -> Result<PathBuf, String> {
    let clip = dossier.join(nom);
    if !clip.is_file() {
        return Err(format!(
            "Export impossible : le clip « {nom} » est introuvable. Retire-le du montage ; le montage est conservé."
        ));
    }
    galerie::dans_dossier(dossier, &clip)
}

fn nom_de(clip: &Path) -> String {
    clip.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn exporter(
    ffmpeg: Ffmpeg,
    d: &Demande,
    annule: &AtomicBool,
    enfant: &Enfant,
    progres: &mut dyn FnMut(u8, bool),
) -> Result<Exporte, Echec> {
    verifier(d.montage)?;
    let mut infos = BTreeMap::new();
    for nom in sources(d.montage) {
        let clip = dossier_clip(d.dossier, nom)?;
        infos.insert(nom.to_string(), sonder(ffmpeg, &clip)?);
    }
    let plan = planifier(d.montage, &infos)?;
    if d.images.len() != plan.textes.len()
        || d.images
            .iter()
            .any(|i| dimensions_png(i) != Some((plan.largeur, plan.hauteur)))
    {
        return Err(Echec::Erreur(
            "Export impossible : les blocs de texte ne correspondent pas au cadre de sortie."
                .into(),
        ));
    }
    let annexes = galerie::creer_annexes(d.dossier)
        .map_err(|e| format!("Export impossible : dossier des clips inaccessible ({e})."))?;
    let utilise = galerie::utilise(d.dossier);
    if utilise >= d.quota {
        return Err(Echec::Erreur("Export impossible : le quota du dossier des clips est atteint. Supprime d'anciens clips puis relance l'export ; le montage est conservé.".into()));
    }
    let surface = f64::from(plan.largeur * plan.hauteur) / f64::from(plan.toile.0 * plan.toile.1);
    let debit = |e: &Entree| {
        debit_export(
            fs::metadata(d.dossier.join(&e.source)).map_or(0, |m| m.len()),
            infos[&e.source].duree_s,
            infos[&e.source].codec,
        )
    };
    let kbps = (f64::from(plan.entrees.iter().map(debit).max().unwrap_or(2000)) * surface.min(1.0))
        .max(2000.0) as u32;
    let estimation = (u64::from(kbps + AUDIO_KBPS) * 125) as f64 * plan.duree_s();
    if estompe_plateforme::espace_libre(d.dossier).is_some_and(|libre| (libre as f64) < estimation)
    {
        return Err(Echec::Erreur("Export impossible : il ne reste pas assez de place sur le disque. Le montage est conservé.".into()));
    }
    let base = format!("export-{}-{}", std::process::id(), d.id);
    let partiel = annexes.join(format!("{base}.{}", galerie::PARTIEL));
    let images: Vec<PathBuf> = (0..plan.textes.len())
        .filter(|j| plan.textes[*j].is_some())
        .map(|j| annexes.join(format!("{base}-t{j}.{}", galerie::PARTIEL)))
        .collect();
    let abandon = |echec: Echec| {
        for f in images.iter().chain([&partiel]) {
            let _ = fs::remove_file(f);
        }
        Err(echec)
    };
    let a_ecrire = d
        .images
        .iter()
        .zip(&plan.textes)
        .filter(|(_, t)| t.is_some())
        .map(|(i, _)| i);
    for (fichier, octets) in images.iter().zip(a_ecrire) {
        if let Err(e) = fs::write(fichier, octets) {
            return abandon(Echec::Erreur(format!(
                "{} Le montage est conservé.",
                message_disque(e.to_string(), "Export")
            )));
        }
    }
    let arguments =
        |encodeur: &str| arguments_export(d.dossier, &images, &partiel, &plan, encodeur, kbps);
    if arguments("libopenh264")
        .iter()
        .map(|a| a.len() + 3)
        .sum::<usize>()
        > ARGUMENTS_MAX
    {
        return abandon(Echec::Erreur("Export impossible : le montage compte trop de passages pour un seul export. Réunis ou retire des passages ; le montage est conservé.".into()));
    }
    let mut dernier = "aucun encodeur H.264 disponible".to_string();
    let mut reussi = None;
    for encodeur in d.encodeurs {
        if annule.load(Ordering::SeqCst) {
            break;
        }
        let logiciel = LOGICIELS.contains(&encodeur.as_str());
        progres(0, logiciel);
        match lancer(
            ffmpeg,
            arguments(encodeur),
            plan.duree_s(),
            enfant,
            annule,
            &mut |p| progres(p, logiciel),
        ) {
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
        return abandon(Echec::Erreur(format!(
            "{} Le montage est conservé.",
            message_disque(dernier, "Export")
        )));
    };
    let premier = d.dossier.join(&d.montage.segments[0].source);
    let nom = Some(galerie::nettoyer(&d.montage.nom))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| nom_de(&premier));
    let chemin = nom_montage(d.dossier, &nom);
    if let Err(e) = fs::rename(&partiel, &chemin) {
        return abandon(Echec::Erreur(format!(
            "Export impossible : {e}. Le montage est conservé."
        )));
    }
    for f in &images {
        let _ = fs::remove_file(f);
    }
    let annexe = Annexe {
        jeu: galerie::lire_annexe(d.dossier, &premier)
            .map(|a| a.jeu)
            .unwrap_or_default(),
        date: d.date.iso(),
        duree_s: plan.duree_s(),
        codec: Codec::H264,
        largeur: plan.largeur,
        hauteur: plan.hauteur,
        ips: plan.ips,
        pistes_audio: u32::from(plan.son()),
        origine: Some(nom_de(&premier)),
    };
    let _ = fs::write(
        galerie::annexe(d.dossier, &chemin, "json"),
        serde_json::to_vec(&annexe).unwrap_or_default(),
    );
    let _ = ffmpeg().and_then(|mut c| {
        c.args(galerie::arguments_vignette(
            &chemin,
            &galerie::annexe(d.dossier, &chemin, "jpg"),
        ))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())
    });
    let taille = fs::metadata(&chemin).map_or(0, |m| m.len());
    Ok(Exporte {
        chemin,
        logiciel,
        quota_atteint: utilise + taille >= d.quota,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::edition::initial;
    use super::super::plan::duree_finale;
    use super::super::plan::tests::{de, montage, segment, texte, CLIP};
    use super::super::{
        enregistrer, lire_projet, projets, Cadre, Segment, Sortie, Source, Volume, VERSION,
    };
    use super::*;

    pub fn ffmpeg_local() -> Result<Command, String> {
        let mut c = Command::new("ffmpeg");
        c.args(["-hide_banner", "-nostdin"]);
        Ok(c)
    }

    fn encodeurs_locaux() -> String {
        ffmpeg_local()
            .ok()
            .and_then(|mut c| c.arg("-encoders").output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    pub fn encodeur_local() -> Option<String> {
        let liste = encodeurs_locaux();
        LOGICIELS
            .into_iter()
            .find(|e| liste.contains(e))
            .map(String::from)
    }

    pub fn fabriquer(chemin: &Path, image: &str, tons: &[u32], encodeur: &str, ips: u32) {
        let mut c = ffmpeg_local().unwrap();
        c.args(["-v", "error", "-f", "lavfi", "-i", image]);
        for t in tons {
            c.args([
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency={t}:sample_rate=48000:duration=4"),
            ]);
        }
        c.args(["-map", "0:v"]);
        for i in 0..tons.len() {
            c.args(["-map", &format!("{}:a", i + 1)]);
        }
        c.args([
            "-c:v",
            encodeur,
            "-b:v",
            "500k",
            "-g",
            &ips.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-ac:a:0",
            "2",
            "-ac:a:1",
            "1",
            "-f",
            "mp4",
            "-y",
        ]);
        let resultat = c.arg(chemin).output().unwrap();
        assert!(
            resultat.status.success(),
            "{}",
            String::from_utf8_lossy(&resultat.stderr)
        );
    }

    pub fn clip_synthetique(chemin: &Path, encodeur: &str) {
        fabriquer(
            chemin,
            "testsrc=size=320x240:rate=30:duration=4",
            &[440, 1500],
            encodeur,
            30,
        );
    }

    fn son(chemin: &Path) -> Vec<i16> {
        let sortie = ffmpeg_local()
            .unwrap()
            .args(["-v", "error", "-i"])
            .arg(chemin)
            .args([
                "-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "s16le", "-",
            ])
            .output()
            .unwrap();
        sortie
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|o| i16::from_le_bytes(*o).saturating_abs())
            .collect()
    }

    fn crete(chemin: &Path) -> i16 {
        son(chemin).into_iter().skip(4800).max().unwrap_or(0)
    }

    fn gris(chemin: &Path) -> Vec<u8> {
        ffmpeg_local()
            .unwrap()
            .args(["-v", "error", "-i"])
            .arg(chemin)
            .args(["-map", "0:v:0", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
            .output()
            .unwrap()
            .stdout
    }

    fn png(dessin: &str) -> Vec<u8> {
        let source = format!("color=c=black@0:s=126x224,format=rgba,{dessin}");
        ffmpeg_local()
            .unwrap()
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &source,
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "png",
                "-",
            ])
            .output()
            .unwrap()
            .stdout
    }

    fn date() -> Horodatage {
        Horodatage::depuis_epoque(1_700_000_000)
    }

    fn partiels(d: &Path) -> usize {
        fs::read_dir(d.join(galerie::ANNEXES))
            .unwrap()
            .flatten()
            .filter(|f| f.path().extension().is_some_and(|x| x == "part"))
            .count()
    }

    #[test]
    fn nom_du_fichier_exporte() {
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        assert_eq!(nom_montage(d, "Jeu 1"), d.join("Jeu 1 (montage).mp4"));
        fs::write(d.join("Jeu 1 (montage).mp4"), b"x").unwrap();
        assert_eq!(nom_montage(d, "Jeu 1"), d.join("Jeu 1 (montage 2).mp4"));
        fs::write(d.join("Jeu 1 (montage 2).mp4"), b"x").unwrap();
        assert_eq!(nom_montage(d, "Jeu 1"), d.join("Jeu 1 (montage 3).mp4"));
    }

    #[test]
    fn disque_plein_et_restes() {
        assert!(disque_plein(
            "[out#0/mp4] Error writing trailer: No space left on device"
        ));
        assert!(disque_plein(
            "av_interleaved_write_frame(): There is not enough space on the disk"
        ));
        assert!(!disque_plein("Unknown encoder 'h264_nvenc'"));
        assert_eq!(derniere_ligne("a\nb\n\n"), "b");
        assert_eq!(
            message_disque("x: No space left on device".into(), "Export"),
            "Export impossible : le disque est plein."
        );
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let annexes = galerie::creer_annexes(d).unwrap();
        for nom in ["export-1-2.part", "Jeu.part", "Jeu.mp4.json"] {
            fs::write(annexes.join(nom), b"x").unwrap();
        }
        purger_partiels(d);
        assert!(
            !annexes.join("export-1-2.part").exists()
                && annexes.join("Jeu.part").exists()
                && annexes.join("Jeu.mp4.json").exists()
        );
    }

    #[test]
    fn encodeurs_materiels_puis_logiciels() {
        let liste = " V....D h264_nvenc NVIDIA\n V....D libopenh264 OpenH264\n V....D libx264 x264\n V....D h264_mf MediaFoundation\n";
        assert_eq!(
            encodeurs_export(liste),
            ["h264_nvenc", "h264_mf", "libopenh264", "libx264"]
        );
        assert!(encodeurs_export("").is_empty());
    }

    #[test]
    fn export_reel_avec_ffmpeg() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let clip = d.join(CLIP);
        clip_synthetique(&clip, &encodeur);
        let original = fs::read(&clip).unwrap();
        let infos = sonder(&ffmpeg_local, &clip).unwrap();
        assert_eq!(
            (
                infos.ips,
                infos.largeur,
                infos.hauteur,
                infos.codec,
                infos.pistes_audio
            ),
            (30, 320, 240, Some(Codec::H264), 2)
        );
        let ondes = [
            onde(&ffmpeg_local, &clip, 0, 50).unwrap(),
            onde(&ffmpeg_local, &clip, 1, 50).unwrap(),
        ];
        assert!(ondes
            .iter()
            .all(|o| o.len() == 50 && o.iter().all(|v| *v > 0.1)));
        assert!(onde(&ffmpeg_local, &clip, 3, 50).is_err());

        let m = montage(
            vec![
                segment(0.5, 1.0, true),
                segment(1.0, 1.5, false),
                segment(1.5, 2.5, true),
                segment(2.5, 3.0, false),
                segment(3.0, 3.5, true),
            ],
            &[(0.5, false), (1.0, true)],
        );
        enregistrer(d, "p1", &m).unwrap();
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let mut suivi = Vec::new();
        let encodeurs = ["encodeur_absent".to_string(), encodeur.clone()];
        let demande = Demande {
            id: 1,
            dossier: d,
            montage: &m,
            images: &[],
            quota: 1 << 30,
            encodeurs: &encodeurs,
            date: date(),
        };
        let e = exporter(
            &ffmpeg_local,
            &demande,
            &annule,
            &enfant,
            &mut |p, logiciel| suivi.push((p, logiciel)),
        )
        .unwrap();
        assert_eq!(
            (e.chemin.clone(), e.logiciel, e.quota_atteint),
            (d.join("Jeu (montage).mp4"), true, false)
        );
        assert_eq!(
            (suivi[0], suivi.iter().filter(|s| s.0 == 0).count() >= 2),
            ((0, false), true)
        );
        assert!(
            suivi
                .windows(2)
                .all(|w| w[0].1 != w[1].1 || w[0].0 <= w[1].0)
                && suivi.iter().all(|s| s.0 <= 99)
        );
        let sortie = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!(
            (
                sortie.codec,
                sortie.largeur,
                sortie.hauteur,
                sortie.ips,
                sortie.pistes_audio
            ),
            (Some(Codec::H264), 320, 240, 30, 1)
        );
        assert!(
            (sortie.duree_s - 2.0).abs() <= 1.0 / 30.0 + 0.011,
            "durée {}",
            sortie.duree_s
        );
        assert_eq!(gris(&e.chemin).len(), 320 * 240 * 60);
        let niveau = crete(&e.chemin);
        assert!((1500..2700).contains(&niveau), "crête {niveau}");
        assert_eq!(fs::read(&clip).unwrap(), original);
        let annexe = galerie::lire_annexe(d, &e.chemin).unwrap();
        assert_eq!(
            (
                annexe.origine.as_deref(),
                annexe.pistes_audio,
                annexe.codec,
                annexe.duree_s,
                annexe.date.as_str()
            ),
            (Some("Jeu"), 1, Codec::H264, 2.0, "2023-11-14T22:13:20")
        );
        assert!(galerie::annexe(d, &e.chemin, "jpg").is_file());
        assert_eq!(
            galerie::lister(d)
                .iter()
                .find(|c| c.nom == "Jeu (montage)")
                .unwrap()
                .origine
                .as_deref(),
            Some("Jeu")
        );

        let tout = montage(vec![segment(0.0, 4.0, true)], &[(1.0, true), (2.0, true)]);
        let seul = [encodeur.clone()];
        let muet = exporter(
            &ffmpeg_local,
            &Demande {
                id: 2,
                montage: &tout,
                encodeurs: &seul,
                ..demande
            },
            &annule,
            &enfant,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(
            (muet.chemin.clone(), muet.logiciel),
            (d.join("Jeu (montage 2).mp4"), true)
        );
        let infos = sonder(&ffmpeg_local, &muet.chemin).unwrap();
        assert_eq!(infos.pistes_audio, 0);
        assert!(
            (infos.duree_s - 4.0).abs() <= 1.0 / 30.0 + 0.011,
            "durée {}",
            infos.duree_s
        );

        let fichiers = galerie::lister(d).len();
        annule.store(true, Ordering::SeqCst);
        assert_eq!(
            exporter(
                &ffmpeg_local,
                &Demande { id: 3, ..demande },
                &annule,
                &enfant,
                &mut |_, _| {}
            ),
            Err(Echec::Annule)
        );
        annule.store(false, Ordering::SeqCst);
        let quota = exporter(
            &ffmpeg_local,
            &Demande {
                id: 4,
                quota: 1,
                ..demande
            },
            &annule,
            &enfant,
            &mut |_, _| {},
        );
        assert!(
            matches!(quota, Err(Echec::Erreur(ref m)) if m.contains("quota") && m.contains("conservé")),
            "{quota:?}"
        );
        let absent = ["encodeur_absent".to_string()];
        let echec = exporter(
            &ffmpeg_local,
            &Demande {
                id: 5,
                encodeurs: &absent,
                ..demande
            },
            &annule,
            &enfant,
            &mut |_, _| {},
        );
        assert!(
            matches!(echec, Err(Echec::Erreur(ref m)) if m.starts_with("Export impossible") && m.ends_with("Le montage est conservé.")),
            "{echec:?}"
        );
        let texte_faux = Montage {
            textes: vec![texte(0.5, 1.0)],
            ..m.clone()
        };
        for images in [&[][..], &[png("null")][..], &[b"pas un png".to_vec()][..]] {
            let refus = exporter(
                &ffmpeg_local,
                &Demande {
                    id: 6,
                    montage: &texte_faux,
                    images,
                    ..demande
                },
                &annule,
                &enfant,
                &mut |_, _| {},
            );
            assert!(
                matches!(refus, Err(Echec::Erreur(ref m)) if m.contains("blocs de texte")),
                "{refus:?}"
            );
        }
        assert_eq!((partiels(d), galerie::lister(d).len()), (0, fichiers));
        assert_eq!(projets(d).len(), 1);

        fs::remove_file(&clip).unwrap();
        assert!(sonder(&ffmpeg_local, &clip)
            .unwrap_err()
            .contains("Clip illisible"));
        let disparu = exporter(
            &ffmpeg_local,
            &Demande { id: 7, ..demande },
            &annule,
            &enfant,
            &mut |_, _| {},
        );
        assert!(
            matches!(disparu, Err(Echec::Erreur(ref m)) if m.contains("« Jeu.mp4 » est introuvable")),
            "{disparu:?}"
        );
        fs::write(&clip, b"pas une video").unwrap();
        assert!(sonder(&ffmpeg_local, &clip)
            .unwrap_err()
            .contains("Clip illisible"));
    }

    #[test]
    fn export_coupe_a_des_instants_donnes_a_une_image_pres() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let clip = d.join(CLIP);
        fabriquer(
            &clip,
            "testsrc=size=320x240:rate=60:duration=4",
            &[440, 1500],
            &encodeur,
            60,
        );
        let infos = sonder(&ffmpeg_local, &clip).unwrap();
        let s = Source::de(CLIP, &infos);
        let m = super::super::rogner(&initial(&s, None), &s, super::super::Cote::Debut, 0.37, 0);
        let m = super::super::rogner(&m, &s, super::super::Cote::Fin, 3.21, 0);
        let m = super::super::couper(&m, &s, 1.13, None);
        let m = super::super::couper(&m, &s, 2.05, None);
        let m = super::super::retirer(&m, 1);
        let attendu = duree_finale(&m);
        assert!((attendu - ((1.13 - 0.37) + (3.21 - 2.05))).abs() < 1.0 / 60.0);
        let encodeurs = [encodeur];
        let demande = Demande {
            id: 1,
            dossier: d,
            montage: &m,
            images: &[],
            quota: 1 << 30,
            encodeurs: &encodeurs,
            date: date(),
        };
        let e = exporter(
            &ffmpeg_local,
            &demande,
            &AtomicBool::new(false),
            &Enfant::default(),
            &mut |_, _| {},
        )
        .unwrap();
        let sortie = sonder(&ffmpeg_local, &e.chemin).unwrap();
        let images = gris(&e.chemin).len() / (320 * 240);
        assert!(
            (images as f64 - attendu * 60.0).abs() <= 1.0,
            "{images} images pour {attendu} s"
        );
        assert!(
            (sortie.duree_s - attendu).abs() <= 1.0 / 60.0 + 0.011,
            "durée {} pour {attendu}",
            sortie.duree_s
        );
    }

    #[test]
    fn export_reel_de_plusieurs_clips_recadres_avec_vitesses_et_textes() {
        let liste = encodeurs_locaux();
        let Some(encodeur) = encodeur_local().filter(|_| liste.contains("libx265")) else {
            eprintln!(
                "ignoré : ffmpeg, libx265 ou encodeur H.264 logiciel absent de cette machine"
            );
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let (rouge, vert, bleu) = (d.join("Rouge.mp4"), d.join("Vert.mp4"), d.join("Bleu.mp4"));
        fabriquer(&rouge, "color=c=red:size=320x240:rate=30:duration=4,drawbox=x=140:y=100:w=40:h=40:color=white:t=fill", &[440, 1500], &encodeur, 30);
        fabriquer(
            &vert,
            "color=c=lime:size=640x360:rate=60:duration=3",
            &[],
            "libx265",
            60,
        );
        fabriquer(
            &bleu,
            "color=c=blue:size=320x240:rate=30:duration=4",
            &[880],
            &encodeur,
            30,
        );
        let originaux: Vec<Vec<u8>> = [&rouge, &vert, &bleu]
            .iter()
            .map(|f| fs::read(f).unwrap())
            .collect();
        let m = Montage {
            version: VERSION,
            nom: "Mon montage".into(),
            segments: vec![
                de("Bleu.mp4", 0.0, 1.0, 1.0, false),
                de("Rouge.mp4", 1.0, 2.0, 0.5, false),
                Segment {
                    garde: false,
                    ..de("Vert.mp4", 1.0, 3.0, 1.0, false)
                },
                de("Vert.mp4", 0.0, 1.0, 2.0, false),
                de("Rouge.mp4", 0.0, 0.5, 1.0, true),
            ],
            volumes: BTreeMap::from([(
                "Rouge.mp4".to_string(),
                vec![
                    Volume {
                        volume: 1.0,
                        muet: false,
                    },
                    Volume {
                        volume: 1.0,
                        muet: true,
                    },
                ],
            )]),
            textes: vec![texte(1.0, 2.0), texte(1.5, 3.9), texte(50.0, 60.0)],
            cadre: Cadre {
                x: 0.2875,
                y: 0.0,
                l: 0.421_875,
                h: 1.0,
                rapport: "9:16".into(),
            },
            sortie: Sortie {
                hauteur: None,
                ips: Some(30),
            },
        };
        enregistrer(d, "avance", &m).unwrap();
        assert_eq!(
            lire_projet(
                &fs::read(d.join(".estompe/projets/avance.json")).unwrap(),
                None
            )
            .as_ref(),
            Some(&m)
        );
        let images = [
            png("drawbox=x=10:y=180:w=30:h=20:color=white:t=fill:replace=1"),
            png("drawbox=x=80:y=20:w=30:h=20:color=white:t=fill:replace=1"),
            png("null"),
        ];
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let encodeurs = [encodeur];
        let demande = Demande {
            id: 1,
            dossier: d,
            montage: &m,
            images: &images,
            quota: 1 << 30,
            encodeurs: &encodeurs,
            date: date(),
        };
        let e = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |_, _| {}).unwrap();
        assert_eq!(e.chemin, d.join("Mon montage (montage).mp4"));
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!(
            (
                infos.codec,
                infos.largeur,
                infos.hauteur,
                infos.ips,
                infos.pistes_audio
            ),
            (Some(Codec::H264), 126, 224, 30, 1)
        );
        let pixels = gris(&e.chemin);
        assert_eq!(pixels.len(), 126 * 224 * 120, "30 + 60 + 15 + 15 images");
        assert!(
            (infos.duree_s - duree_finale(&m)).abs() <= 1.0 / 30.0 + 0.011,
            "durée {}",
            infos.duree_s
        );
        let ton = |image: usize, x: usize, y: usize| match pixels[image * 126 * 224 + y * 126 + x] {
            0..=12 => "noir",
            20..=48 => "bleu",
            60..=100 => "rouge",
            125..=175 => "vert",
            215..=255 => "blanc",
            autre => panic!("ton inattendu {autre} à l'image {image} ({x}, {y})"),
        };
        let milieu = |image: usize| ton(image, 20, 112);
        assert_eq!(
            [0, 29, 30, 89, 90, 104, 105, 119].map(milieu),
            ["bleu", "bleu", "rouge", "rouge", "vert", "vert", "rouge", "rouge"]
        );
        assert_eq!(
            (ton(60, 63, 112), ton(60, 63, 60), ton(110, 63, 112)),
            ("blanc", "rouge", "blanc")
        );
        assert_eq!(
            (
                ton(95, 63, 8),
                ton(95, 63, 216),
                ton(95, 63, 40),
                ton(95, 4, 112),
                ton(95, 121, 112)
            ),
            ("noir", "noir", "vert", "vert", "vert")
        );
        let premier = |image: usize| ton(image, 25, 190);
        let second = |image: usize| ton(image, 95, 30);
        assert_eq!(
            [29, 30, 45, 59, 60].map(premier),
            ["bleu", "blanc", "blanc", "blanc", "rouge"]
        );
        assert_eq!(
            [44, 45, 59, 100, 116, 117].map(second),
            ["rouge", "blanc", "blanc", "blanc", "blanc", "rouge"]
        );
        let piste = son(&e.chemin);
        let fort = |debut_s: f64, fin_s: f64| {
            piste[(debut_s * 48000.0) as usize..(fin_s * 48000.0) as usize]
                .iter()
                .copied()
                .max()
                .unwrap_or(0)
        };
        assert!(
            fort(0.2, 0.9) > 2000 && fort(1.2, 2.9) > 2000 && fort(2.9, 2.96) > 2000,
            "son avant le raccord"
        );
        assert!(
            fort(3.04, 3.5) < 200 && fort(3.5, 3.9) < 200,
            "silence après le raccord : {}",
            fort(3.04, 3.5)
        );
        assert!(
            (piste.len() as f64 / 48000.0 - 4.0).abs() <= 0.04,
            "son {} s",
            piste.len() as f64 / 48000.0
        );
        for (fichier, original) in [&rouge, &vert, &bleu].iter().zip(&originaux) {
            assert_eq!(&fs::read(fichier).unwrap(), original);
        }
        let annexe = galerie::lire_annexe(d, &e.chemin).unwrap();
        assert_eq!(
            (
                annexe.largeur,
                annexe.hauteur,
                annexe.ips,
                annexe.duree_s,
                annexe.origine.as_deref()
            ),
            (126, 224, 30, 4.0, Some("Bleu"))
        );
        assert_eq!(partiels(d), 0);

        let libre = Montage {
            segments: vec![
                de("Vert.mp4", 0.0, 1.0, 1.0, false),
                de("Rouge.mp4", 0.0, 1.0, 4.0, false),
            ],
            textes: Vec::new(),
            cadre: Cadre::default(),
            sortie: Sortie {
                hauteur: Some(180),
                ips: Some(60),
            },
            ..m.clone()
        };
        let e = exporter(
            &ffmpeg_local,
            &Demande {
                id: 2,
                montage: &libre,
                images: &[],
                ..demande
            },
            &annule,
            &enfant,
            &mut |_, _| {},
        )
        .unwrap();
        let infos = sonder(&ffmpeg_local, &e.chemin).unwrap();
        assert_eq!(
            (
                infos.codec,
                infos.largeur,
                infos.hauteur,
                infos.ips,
                gris(&e.chemin).len()
            ),
            (Some(Codec::H264), 320, 180, 60, 320 * 180 * 75)
        );

        fs::remove_file(&vert).unwrap();
        assert_eq!(
            projets(d)
                .iter()
                .find(|p| p.id == "avance")
                .unwrap()
                .manquants,
            ["Vert.mp4"]
        );
    }

    #[test]
    fn annulation_reelle_pendant_l_export() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let clip = d.join(CLIP);
        clip_synthetique(&clip, &encodeur);
        let m = Montage {
            textes: vec![texte(0.0, 1.0)],
            ..montage(vec![segment(0.0, 4.0, true)], &[])
        };
        let image = ffmpeg_local()
            .unwrap()
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black@0:s=320x240,format=rgba",
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "png",
                "-",
            ])
            .output()
            .unwrap()
            .stdout;
        let (annule, enfant) = (AtomicBool::new(false), Enfant::default());
        let encodeurs = [encodeur];
        let images = [image];
        let demande = Demande {
            id: 1,
            dossier: d,
            montage: &m,
            images: &images,
            quota: 1 << 30,
            encodeurs: &encodeurs,
            date: date(),
        };
        let resultat = exporter(&ffmpeg_local, &demande, &annule, &enfant, &mut |_, _| {
            annule.store(true, Ordering::SeqCst);
            arreter(&enfant);
        });
        assert_eq!(resultat, Err(Echec::Annule));
        assert!(partiels(d) == 0 && galerie::lister(d).len() == 1);
    }
}
