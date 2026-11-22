use std::collections::BTreeMap;
use std::path::Path;

use super::edition::{caler, image_de, images, Lieu};
use super::plan::{graphe_partiel, planifier, sources, Infos, Parties, Plan};
use super::{Montage, Segment, Sortie, Sources};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluxApercu {
    Video,
    Son,
}

fn avec_segments(m: &Montage, segments: Vec<Segment>, sortie: Sortie) -> Montage {
    let mut extrait = Montage {
        segments,
        textes: Vec::new(),
        sortie,
        ..m.clone()
    };
    let gardees: Vec<String> = sources(&extrait).into_iter().map(String::from).collect();
    extrait.volumes.retain(|k, _| gardees.contains(k));
    extrait
}

pub fn extrait(m: &Montage, l: Lieu, sources: &Sources) -> Option<Montage> {
    let mut segments = Vec::new();
    for (k, s) in m
        .segments
        .iter()
        .enumerate()
        .skip(l.i)
        .filter(|(_, s)| s.garde)
    {
        let debut = match sources.get(&s.source) {
            Some(source) if k == l.i => caler(l.t, source).max(s.debut),
            _ => s.debut,
        };
        if debut < s.fin {
            segments.push(Segment { debut, ..s.clone() });
        }
    }
    (!segments.is_empty()).then(|| avec_segments(m, segments, Sortie::default()))
}

pub fn image_seule(m: &Montage, l: Lieu, sources: &Sources) -> Option<Montage> {
    let segment = m.segments.get(l.i)?;
    let source = sources.get(&segment.source)?;
    let ips = source.cadence();
    let derniere = images(source).saturating_sub(1) as i64;
    let image = image_de(l.t, ips).clamp(0, derniere);
    let segments = vec![Segment {
        source: segment.source.clone(),
        debut: image as f64 / ips,
        fin: (image + 1) as f64 / ips,
        garde: true,
        vitesse: 1.0,
        muet: true,
    }];
    Some(avec_segments(
        m,
        segments,
        Sortie {
            hauteur: m.sortie.hauteur,
            ips: Some(source.ips),
        },
    ))
}

pub fn plan_apercu(
    m: &Montage,
    infos: &BTreeMap<String, Infos>,
    hauteur: u32,
    ips: u32,
) -> Result<Plan, String> {
    planifier(
        &Montage {
            sortie: Sortie {
                hauteur: Some(hauteur),
                ips: Some(m.sortie.ips.unwrap_or(ips)),
            },
            ..m.clone()
        },
        infos,
    )
}

pub fn arguments_apercu(dossier: &Path, p: &Plan, flux: FluxApercu) -> Vec<String> {
    let decalages: Vec<u64> = (0..p.entrees.len())
        .map(|e| {
            p.morceaux
                .iter()
                .filter(|m| m.entree == e)
                .map(|m| m.images.0)
                .min()
                .unwrap_or(0)
        })
        .collect();
    let mut a: Vec<String> = ["-v", "error", "-nostats"].map(String::from).into();
    for (e, entree) in p.entrees.iter().enumerate() {
        if decalages[e] > 0 {
            a.extend([
                "-ss".into(),
                format!("{:.6}", decalages[e] as f64 / f64::from(entree.ips)),
            ]);
        }
        a.extend([
            "-i".into(),
            dossier.join(&entree.source).to_string_lossy().into_owned(),
        ]);
    }
    let video = flux == FluxApercu::Video;
    a.extend([
        "-filter_complex".into(),
        graphe_partiel(
            p,
            Parties {
                video,
                son: !video,
                decalages: &decalages,
            },
        ),
    ]);
    if video {
        a.extend(
            [
                "-map", "[v]", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
            ]
            .map(String::from),
        );
    } else {
        a.extend(
            [
                "-map", "[a]", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1",
            ]
            .map(String::from),
        );
    }
    a
}

#[cfg(test)]
mod tests {
    use super::super::essais::*;
    use super::super::export::sonder;
    use super::super::export::tests::{encodeur_local, fabriquer, ffmpeg_local};
    use super::super::{initial, retirer, sources_de, Cadre, Source};
    use super::*;

    #[test]
    fn extrait_depuis_la_tete_de_lecture() {
        let m = trois();
        let src = src();
        let e = extrait(&m, Lieu { i: 0, t: 1.004 }, &src).unwrap();
        assert_eq!(
            plages(&e),
            [(1.0, 2.0, true), (3.0, 5.0, true), (7.0, 9.5, true)]
        );
        assert!(e.textes.is_empty() && e.volumes.contains_key("Jeu.mp4"));
        let e = extrait(&m, Lieu { i: 1, t: 2.5 }, &src).unwrap();
        assert_eq!(plages(&e), [(3.0, 5.0, true), (7.0, 9.5, true)]);
        assert_eq!(extrait(&m, Lieu { i: 4, t: 9.5 }, &src), None);
        let deux = super::super::ajouter_clip(&retirer(&initial(&s(), None), 0), &h());
        let e = extrait(&deux, Lieu { i: 0, t: 0.0 }, &src).unwrap();
        assert_eq!(e.volumes.keys().collect::<Vec<_>>(), ["Hevc.mp4"]);
    }

    #[test]
    fn une_seule_image_a_la_cadence_du_clip() {
        let m = Montage {
            sortie: Sortie {
                hauteur: Some(720),
                ips: Some(30),
            },
            ..trois()
        };
        let src = src();
        let i = image_seule(&m, Lieu { i: 1, t: 2.51 }, &src).unwrap();
        assert_eq!(plages(&i), [(150.0 / 60.0, 151.0 / 60.0, true)]);
        assert_eq!(
            (i.segments[0].muet, i.sortie),
            (
                true,
                Sortie {
                    hauteur: Some(720),
                    ips: Some(60)
                }
            )
        );
        let fin = image_seule(&m, Lieu { i: 4, t: 99.0 }, &src).unwrap();
        assert_eq!(plages(&fin), [(599.0 / 60.0, 10.0, true)]);
        assert_eq!(image_seule(&m, Lieu { i: 9, t: 0.0 }, &src), None);
    }

    #[test]
    fn arguments_de_l_apercu() {
        let infos = BTreeMap::from([(
            "Jeu.mp4".to_string(),
            Infos {
                duree_s: 10.0,
                codec: None,
                largeur: 1920,
                hauteur: 1080,
                ips: 60,
                pistes_audio: 2,
            },
        )]);
        let m = extrait(&trois(), Lieu { i: 0, t: 1.0 }, &src()).unwrap();
        let p = plan_apercu(&m, &infos, 360, 30).unwrap();
        assert_eq!((p.largeur, p.hauteur, p.ips), (640, 360, 30));
        let a = arguments_apercu(Path::new("d"), &p, FluxApercu::Video);
        let paire = |x: &str, y: &str| a.windows(2).any(|w| w == [x, y]);
        assert!(
            paire("-ss", "1.000000")
                && paire("-map", "[v]")
                && paire("-pix_fmt", "rgba")
                && a.last().unwrap() == "pipe:1"
        );
        assert!(
            a.iter()
                .any(|x| x.contains("trim=start_frame=0:end_frame=60,")),
            "{a:?}"
        );
        let s = arguments_apercu(Path::new("d"), &p, FluxApercu::Son);
        assert!(
            s.windows(2).any(|w| w == ["-map", "[a]"])
                && s.windows(2).any(|w| w == ["-f", "f32le"])
        );
        let haute = plan_apercu(
            &Montage {
                sortie: Sortie {
                    hauteur: None,
                    ips: Some(60),
                },
                ..m
            },
            &infos,
            360,
            30,
        )
        .unwrap();
        assert_eq!(haute.ips, 60);
    }

    fn rgba(arguments: &[String]) -> Vec<u8> {
        let sortie = ffmpeg_local().unwrap().args(arguments).output().unwrap();
        assert!(
            sortie.status.success(),
            "{}",
            String::from_utf8_lossy(&sortie.stderr)
        );
        sortie.stdout
    }

    #[test]
    fn apercu_reel_a_l_image_pres() {
        let Some(encodeur) = encodeur_local() else {
            eprintln!("ignoré : ffmpeg ou encodeur H.264 logiciel absent de cette machine");
            return;
        };
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        let clip = d.join("Jeu.mp4");
        fabriquer(
            &clip,
            "color=c=black:s=64x48:r=30:d=4,geq=lum='16+N*1.8':cb=128:cr=128",
            &[440],
            &encodeur,
            30,
        );
        let infos =
            BTreeMap::from([("Jeu.mp4".to_string(), sonder(&ffmpeg_local, &clip).unwrap())]);
        let source = Source::de("Jeu.mp4", &infos["Jeu.mp4"]);
        let src = sources_de(std::slice::from_ref(&source));
        let m = initial(&source, None);
        let reference = rgba(
            &[
                "-v",
                "error",
                "-i",
                clip.to_str().unwrap(),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ]
            .map(String::from),
        );
        let rouges: Vec<u8> = reference.chunks(64 * 48 * 4).map(|i| i[0]).collect();
        assert_eq!(rouges.len(), 120);
        let numeros = |octets: &[u8], largeur: usize, image: usize| -> Vec<usize> {
            let rouge = octets[image * largeur * 48 * 4];
            (0..rouges.len()).filter(|n| rouges[*n] == rouge).collect()
        };
        for t in [0.0, 1.0, 2.04, 3.5] {
            let seule = image_seule(&m, Lieu { i: 0, t }, &src).unwrap();
            let p = plan_apercu(&seule, &infos, 48, 30).unwrap();
            let octets = rgba(&arguments_apercu(d, &p, FluxApercu::Video));
            assert_eq!(octets.len(), 64 * 48 * 4);
            let attendu = image_de(t, 30.0) as usize;
            assert!(
                numeros(&octets, 64, 0).contains(&attendu),
                "{t} : {:?} au lieu de {attendu}",
                numeros(&octets, 64, 0)
            );
        }
        let recadre = Montage {
            cadre: Cadre {
                x: 0.25,
                y: 0.0,
                l: 0.5,
                h: 1.0,
                rapport: "libre".into(),
            },
            ..m.clone()
        };
        let lecture = extrait(&recadre, Lieu { i: 0, t: 2.0 }, &src).unwrap();
        let p = plan_apercu(&lecture, &infos, 48, 30).unwrap();
        let octets = rgba(&arguments_apercu(d, &p, FluxApercu::Video));
        assert_eq!(octets.len(), 32 * 48 * 4 * 60);
        assert!(numeros(&octets, 32, 0).contains(&60) && numeros(&octets, 32, 59).contains(&119));
        let son = rgba(&arguments_apercu(d, &p, FluxApercu::Son));
        assert!(
            (son.len() as f64 / 4.0 / 48000.0 - 2.0).abs() < 0.05,
            "{} octets",
            son.len()
        );
    }
}
