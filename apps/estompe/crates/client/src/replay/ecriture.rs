use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};

use super::galerie::{self, Annexe, Horodatage};
use super::micro::FREQUENCE;
use super::mp4::{self, PisteAudio, PisteVideo};
use super::tampon::{Instantane, UNITE_TEMPS};
use super::types::{ClipEnregistre, ErreurReplay as Erreur};

pub struct Tache {
    pub instantane: Instantane,
    pub jeu: String,
    pub dossier: PathBuf,
    pub quota: u64,
    pub date: Horodatage,
}

#[derive(Debug)]
pub struct Ecrit {
    pub clip: ClipEnregistre,
    pub chemin: PathBuf,
    pub dossier: PathBuf,
    pub quota_atteint: bool,
}

fn erreur(e: &io::Error, dossier: &Path) -> Erreur {
    match e.kind() {
        _ if estompe_plateforme::disque_plein(e) => {
            Erreur::nouvelle("disque-plein", "Clip non enregistré : le disque est plein.")
        }
        io::ErrorKind::InvalidData => {
            Erreur::nouvelle("ecriture", format!("Clip non enregistré : {e}."))
        }
        _ => Erreur::nouvelle(
            "dossier-inaccessible",
            format!(
                "Clip non enregistré : dossier {} inaccessible ({e}).",
                dossier.display()
            ),
        ),
    }
}

fn ecrire_fichier(chemin: &Path, i: &Instantane) -> io::Result<u64> {
    let mut sortie = BufWriter::with_capacity(1 << 20, File::create(chemin)?);
    let video = PisteVideo {
        codec: i.format.codec,
        largeur: i.format.largeur,
        hauteur: i.format.hauteur,
        unites: &i.video,
    };
    let audios = [
        PisteAudio {
            nom: "Jeu",
            frequence: FREQUENCE,
            canaux: 2,
            unites: &i.jeu,
        },
        PisteAudio {
            nom: "Micro",
            frequence: FREQUENCE,
            canaux: 1,
            unites: &i.micro,
        },
    ];
    let taille = mp4::ecrire(&mut sortie, &video, &audios)?;
    sortie.flush()?;
    sortie.get_ref().sync_all()?;
    Ok(taille)
}

pub fn ecrire(t: &Tache) -> Result<Ecrit, Erreur> {
    if t.instantane.video.is_empty() {
        return Err(Erreur::nouvelle(
            "tampon-vide",
            "Rien à enregistrer : le tampon est vide.",
        ));
    }
    let annexes = galerie::creer_annexes(&t.dossier).map_err(|e| erreur(&e, &t.dossier))?;
    let utilise = galerie::utilise(&t.dossier);
    if utilise >= t.quota {
        return Err(Erreur::nouvelle("quota", "Clip non enregistré : le quota du dossier des clips est atteint. Supprime d'anciens clips pour continuer."));
    }
    let chemin = galerie::chemin_libre(&t.dossier, &galerie::nom_clip(&t.jeu, &t.date));
    let nom = chemin
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let partiel = annexes.join(format!("{nom}.{}", galerie::PARTIEL));
    let taille = ecrire_fichier(&partiel, &t.instantane)
        .and_then(|taille| fs::rename(&partiel, &chemin).map(|_| taille));
    let taille = taille.map_err(|e| {
        let _ = fs::remove_file(&partiel);
        erreur(&e, &t.dossier)
    })?;
    let f = t.instantane.format;
    let duree_s = t.instantane.duree() as f64 / UNITE_TEMPS as f64;
    let pistes_audio = [&t.instantane.jeu, &t.instantane.micro]
        .iter()
        .filter(|p| !p.is_empty())
        .count() as u32;
    let annexe = Annexe {
        jeu: t.jeu.clone(),
        date: t.date.iso(),
        duree_s,
        codec: f.codec,
        largeur: f.largeur,
        hauteur: f.hauteur,
        ips: f.ips,
        pistes_audio,
        origine: None,
    };
    let _ = fs::write(
        galerie::annexe(&t.dossier, &chemin, "json"),
        serde_json::to_vec(&annexe).unwrap_or_default(),
    );
    Ok(Ecrit {
        clip: ClipEnregistre {
            chemin: chemin.clone(),
            nom,
            jeu: t.jeu.clone(),
            duree_s,
            taille_octets: taille,
        },
        chemin,
        dossier: t.dossier.clone(),
        quota_atteint: utilise + taille >= t.quota,
    })
}

pub struct FileEcriture(Sender<Tache>);

impl FileEcriture {
    pub fn demarrer(
        avant: impl FnOnce() + Send + 'static,
        mut apres: impl FnMut(Result<Ecrit, Erreur>) + Send + 'static,
    ) -> Self {
        let (tx, rx) = channel::<Tache>();
        std::thread::spawn(move || {
            avant();
            for tache in rx {
                apres(ecrire(&tache));
            }
        });
        Self(tx)
    }

    pub fn demander(&self, tache: Tache) -> Result<(), Erreur> {
        self.0.send(tache).map_err(|_| {
            Erreur::nouvelle(
                "ecriture",
                "Clip non enregistré : l'écriture des clips est arrêtée.",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::mp4::tests::unites;
    use crate::replay::tampon::{Format, Unite};
    use crate::replay::types::Codec;

    fn tache(dossier: &Path, micro: bool, quota: u64) -> Tache {
        let image = UNITE_TEMPS / 30;
        let bloc = UNITE_TEMPS * 1024 / 48_000;
        let format = Format {
            codec: Codec::H264,
            largeur: 1280,
            hauteur: 720,
            ips: 30,
            micro,
        };
        let instantane = Instantane {
            format,
            video: unites(60, image, 200, true, 0),
            jeu: unites(93, bloc, 40, false, 0),
            micro: if micro {
                unites(93, bloc, 20, false, 0)
            } else {
                Vec::new()
            },
        };
        Tache {
            instantane,
            jeu: "Mon: Jeu".into(),
            dossier: dossier.to_path_buf(),
            quota,
            date: Horodatage::depuis_epoque(1_794_441_600),
        }
    }

    #[test]
    fn enregistre_le_clip_et_son_annexe() {
        let racine = tempfile::tempdir().unwrap();
        let d = racine.path().join("clips");
        let e = ecrire(&tache(&d, true, 1 << 30)).unwrap();
        assert_eq!(e.chemin, d.join("Mon Jeu 2026-11-12 00-00-00.mp4"));
        assert_eq!(
            (e.clip.taille_octets, e.quota_atteint),
            (fs::metadata(&e.chemin).unwrap().len(), false)
        );
        assert!((e.clip.duree_s - 2.0).abs() < 0.001);
        let a = galerie::lire_annexe(&d, &e.chemin).unwrap();
        assert_eq!(
            (a.jeu.as_str(), a.pistes_audio, a.ips, a.date.as_str()),
            ("Mon: Jeu", 2, 30, "2026-11-12T00:00:00")
        );
        assert_eq!(
            galerie::lire_annexe(&d, &ecrire(&tache(&d, false, 1 << 30)).unwrap().chemin)
                .unwrap()
                .pistes_audio,
            1
        );
        let restes: Vec<_> = fs::read_dir(d.join(galerie::ANNEXES))
            .unwrap()
            .flatten()
            .filter(|f| f.path().extension().is_some_and(|x| x == "part"))
            .collect();
        assert!(restes.is_empty());
        assert_eq!(galerie::lister(&d).len(), 2);
    }

    #[test]
    fn erreurs_sans_perdre_les_clips_existants() {
        let racine = tempfile::tempdir().unwrap();
        let d = racine.path().to_path_buf();
        let mut vide = tache(&d, false, 1 << 30);
        vide.instantane.video.clear();
        assert_eq!(ecrire(&vide).unwrap_err().code, "tampon-vide");
        let premier = ecrire(&tache(&d, false, 30_000)).unwrap();
        assert!(premier.quota_atteint || premier.clip.taille_octets < 30_000);
        let plein = ecrire(&tache(&d, false, 1000));
        assert_eq!(plein.unwrap_err().code, "quota");
        assert!(premier.chemin.exists());
        let fichier = d.join("fichier");
        fs::write(&fichier, b"x").unwrap();
        assert_eq!(
            ecrire(&tache(&fichier, false, 1 << 30)).unwrap_err().code,
            "dossier-inaccessible"
        );
        assert_eq!(
            erreur(
                &io::Error::from_raw_os_error(estompe_plateforme::ERREUR_DISQUE_PLEIN),
                &d
            )
            .code,
            "disque-plein"
        );
    }

    #[test]
    fn deux_demandes_rapprochees_donnent_deux_clips() {
        let racine = tempfile::tempdir().unwrap();
        let d = racine.path().to_path_buf();
        let (pret, attendre) = channel::<()>();
        let (fini, resultats) = channel();
        let file = FileEcriture::demarrer(
            move || attendre.recv().unwrap(),
            move |r| fini.send(r.map(|e| e.chemin)).unwrap(),
        );
        file.demander(tache(&d, false, 1 << 30)).unwrap();
        file.demander(tache(&d, true, 1 << 30)).unwrap();
        pret.send(()).unwrap();
        let (a, b) = (
            resultats.recv().unwrap().unwrap(),
            resultats.recv().unwrap().unwrap(),
        );
        assert_eq!(a, d.join("Mon Jeu 2026-11-12 00-00-00.mp4"));
        assert_eq!(b, d.join("Mon Jeu 2026-11-12 00-00-00 (2).mp4"));
        assert!(a.exists() && b.exists());
    }

    fn ffmpeg(programme: &str, arguments: &[&str]) -> Option<std::process::Output> {
        std::process::Command::new(programme)
            .arg("-hide_banner")
            .args((programme == "ffmpeg").then_some("-nostdin"))
            .args(arguments)
            .output()
            .ok()
    }

    fn images_h264(flux: &[u8], ips: i64) -> Vec<Unite> {
        let mut images: Vec<Vec<u8>> = Vec::new();
        for n in mp4::nal(flux) {
            if n[0] & 0x1F == 9 {
                images.push(Vec::new());
            } else if let Some(image) = images.last_mut() {
                image.extend([0, 0, 0, 1]);
                image.extend(n);
            }
        }
        let pas = UNITE_TEMPS / ips;
        images
            .into_iter()
            .enumerate()
            .map(|(i, image)| Unite {
                pts: i as i64 * pas,
                duree: pas,
                cle: mp4::nal(&image).iter().any(|n| n[0] & 0x1F == 5),
                donnees: mp4::en_longueurs(&image).into(),
            })
            .collect()
    }

    fn trames_adts(mut flux: &[u8]) -> Vec<Unite> {
        let mut trames = Vec::new();
        while flux.len() >= 7 && flux[0] == 0xFF && flux[1] & 0xF0 == 0xF0 {
            let entete = if flux[1] & 1 == 1 { 7 } else { 9 };
            let longueur = ((flux[3] as usize & 3) << 11)
                | ((flux[4] as usize) << 3)
                | (flux[5] as usize >> 5);
            let pts = trames.len() as i64 * 1024 * UNITE_TEMPS / 48_000;
            trames.push(Unite {
                pts,
                duree: 1024 * UNITE_TEMPS / 48_000,
                cle: true,
                donnees: flux[entete..longueur].to_vec().into(),
            });
            flux = &flux[longueur..];
        }
        trames
    }

    #[test]
    fn mp4_de_synthese_lu_par_ffmpeg() {
        let racine = tempfile::tempdir().unwrap();
        let chemin = |nom: &str| racine.path().join(nom).to_string_lossy().into_owned();
        let (h264, jeu, micro) = (chemin("v.h264"), chemin("jeu.aac"), chemin("micro.aac"));
        let video = ffmpeg(
            "ffmpeg",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30",
                "-t",
                "2",
                "-c:v",
                "libx264",
                "-g",
                "30",
                "-bf",
                "0",
                "-pix_fmt",
                "yuv420p",
                "-x264-params",
                "aud=1",
                "-f",
                "h264",
                &h264,
            ],
        );
        if !video.is_some_and(|o| o.status.success()) {
            eprintln!("ffmpeg avec libx264 absent : test du MP4 lu par ffmpeg non lancé");
            return;
        }
        for (sortie, canaux) in [(&jeu, "2"), (&micro, "1")] {
            let son = ffmpeg(
                "ffmpeg",
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-t",
                    "2",
                    "-ac",
                    canaux,
                    "-c:a",
                    "aac",
                    "-b:a",
                    "128k",
                    "-f",
                    "adts",
                    sortie,
                ],
            )
            .unwrap();
            assert!(
                son.status.success(),
                "{}",
                String::from_utf8_lossy(&son.stderr)
            );
        }
        let format = Format {
            codec: Codec::H264,
            largeur: 320,
            hauteur: 240,
            ips: 30,
            micro: true,
        };
        let instantane = Instantane {
            format,
            video: images_h264(&fs::read(&h264).unwrap(), 30),
            jeu: trames_adts(&fs::read(&jeu).unwrap()),
            micro: trames_adts(&fs::read(&micro).unwrap()),
        };
        assert_eq!(instantane.video.len(), 60);
        assert!(instantane.jeu.len() > 90 && instantane.micro.len() > 90);
        let d = racine.path().join("clips");
        let e = ecrire(&Tache {
            instantane,
            jeu: "Synthèse".into(),
            dossier: d,
            quota: 1 << 30,
            date: Horodatage::depuis_epoque(0),
        })
        .unwrap();
        let clip = e.chemin.to_string_lossy().into_owned();
        let lecture = ffmpeg(
            "ffmpeg",
            &["-v", "error", "-i", &clip, "-map", "0", "-f", "null", "-"],
        )
        .unwrap();
        assert!(
            lecture.status.success() && lecture.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&lecture.stderr)
        );
        let flux = ffmpeg(
            "ffprobe",
            &[
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,channels",
                "-of",
                "csv=p=0",
                &clip,
            ],
        )
        .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&flux.stdout)
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["h264", "aac,2", "aac,1"]
        );
        let duree = ffmpeg(
            "ffprobe",
            &[
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
                &clip,
            ],
        )
        .unwrap();
        let duree: f64 = String::from_utf8_lossy(&duree.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!((duree - 2.0).abs() < 0.1, "{duree}");
    }
}
