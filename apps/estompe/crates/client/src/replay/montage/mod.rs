mod apercu;
mod edition;
mod export;
mod geometrie;
mod plan;
mod projets;
mod texte;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use apercu::{arguments_apercu, extrait, image_seule, plan_apercu, FluxApercu};
pub use edition::*;
pub use export::{
    arreter, disque_plein, encodeurs_export, exporter, message_disque, nom_montage, onde,
    purger_partiels, sonder, Demande, Echec, Enfant, Exporte, Ffmpeg,
};
pub use geometrie::*;
pub use plan::{
    analyser, arguments_export, debit_export, dimensions_png, duree_finale, graphe, planifier,
    progression, sources, tempo, verifier, Entree, Infos, Morceau, Plan,
};
pub use projets::{
    dernier_pour, enregistrer, lire, lire_projet, migrer, nouvel_id, projets, renommer_projet,
    suivre_renommage, supprimer_projet, Resume,
};
pub use texte::*;

pub const VERSION: u32 = 2;
pub const CLIPS_MAX: usize = 20;
pub const DUREE_MAX_S: f64 = 1800.0;
pub const SEGMENTS_MAX: usize = 500;
pub const VOLUME_MAX: f64 = 2.0;
pub const HISTORIQUE_MAX: usize = 200;
pub const TEXTES_MAX: usize = 50;
pub const TEXTE_MAX: usize = 200;
pub const NOM_MAX: usize = 80;
pub const VITESSES: [f64; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 4.0];
pub const RAPPORTS: [&str; 5] = ["libre", "16:9", "1:1", "9:16", "4:5"];
pub const POLICES: [&str; 3] = ["Bricolage Grotesque", "Instrument Sans", "JetBrains Mono"];
pub const HABILLAGES: [(&str, &str); 3] =
    [("aucun", "Aucun"), ("contour", "Contour"), ("fond", "Fond")];
pub const HAUTEURS: [u32; 5] = [2160, 1440, 1080, 720, 480];
pub const CADENCES: [u32; 4] = [144, 120, 60, 30];
pub const NOMS_PISTES: [&str; 2] = ["Jeu", "Micro"];
pub const LOGICIELS: [&str; 2] = ["libx264", "libopenh264"];

fn un() -> f64 {
    1.0
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Volume {
    pub volume: f64,
    pub muet: bool,
}

impl Default for Volume {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muet: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
pub struct Cadre {
    pub x: f64,
    pub y: f64,
    pub l: f64,
    pub h: f64,
    pub rapport: String,
}

impl Default for Cadre {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            l: 1.0,
            h: 1.0,
            rapport: RAPPORTS[0].into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Sortie {
    pub hauteur: Option<u32>,
    pub ips: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
pub struct Source {
    pub nom: String,
    pub duree_s: f64,
    pub ips: u32,
    pub pistes: usize,
    pub largeur: u32,
    pub hauteur: u32,
}

pub type Sources = BTreeMap<String, Source>;

impl Source {
    pub fn de(nom: &str, infos: &Infos) -> Self {
        Self {
            nom: nom.to_string(),
            duree_s: infos.duree_s,
            ips: infos.ips,
            pistes: (infos.pistes_audio as usize).min(super::galerie::PISTES_MAX),
            largeur: infos.largeur,
            hauteur: infos.hauteur,
        }
    }

    pub fn cadence(&self) -> f64 {
        f64::from(self.ips)
    }
}

pub fn sources_de(liste: &[Source]) -> Sources {
    liste.iter().map(|s| (s.nom.clone(), s.clone())).collect()
}

pub fn nom_piste(i: usize) -> String {
    NOMS_PISTES
        .get(i)
        .map_or_else(|| format!("Piste {}", i + 1), |n| n.to_string())
}

pub fn nom_travail(nom: &str, logiciel: bool) -> String {
    format!(
        "Export de « {nom} »{}",
        if logiciel { " (encodeur logiciel)" } else { "" }
    )
}

pub(crate) fn arrondi(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub(crate) fn min_js(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

pub(crate) fn max_js(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

pub(crate) fn borner(x: f64, min: f64, max: f64, defaut: f64) -> f64 {
    if x.is_finite() {
        x.max(min).min(max)
    } else {
        defaut
    }
}

#[cfg(test)]
pub(crate) mod essais {
    use super::*;

    pub fn s() -> Source {
        Source {
            nom: "Jeu.mp4".into(),
            duree_s: 10.0,
            ips: 60,
            pistes: 2,
            largeur: 1920,
            hauteur: 1080,
        }
    }

    pub fn h() -> Source {
        Source {
            nom: "Hevc.mp4".into(),
            duree_s: 8.0,
            ips: 144,
            pistes: 1,
            largeur: 2560,
            hauteur: 1440,
        }
    }

    pub fn c() -> Source {
        Source {
            nom: "Carre.mp4".into(),
            duree_s: 5.0,
            ips: 30,
            pistes: 0,
            largeur: 1000,
            hauteur: 1000,
        }
    }

    pub fn src() -> Sources {
        sources_de(&[s(), h(), c()])
    }

    pub fn plages(m: &Montage) -> Vec<(f64, f64, bool)> {
        m.segments
            .iter()
            .map(|s| (s.debut, s.fin, s.garde))
            .collect()
    }

    pub fn ordre(m: &Montage) -> String {
        m.segments
            .iter()
            .map(|s| format!("{}{}", &s.source[..1], s.debut))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn coupes(m: Montage, t: &[f64]) -> Montage {
        t.iter().fold(m, |x, c| couper(&x, &s(), *c, None))
    }

    pub fn trois() -> Montage {
        let m = rogner(&initial(&s(), None), &s(), Cote::Debut, 0.5, 0);
        let m = rogner(&m, &s(), Cote::Fin, 9.5, 0);
        let m = coupes(m, &[2.0, 3.0, 5.0, 7.0]);
        retirer(&retirer(&m, 1), 3)
    }

    pub fn tous() -> Montage {
        ajouter_clip(&ajouter_clip(&initial(&s(), None), &h()), &c())
    }

    pub fn proche(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn nomme_un_export_et_les_pistes() {
        assert_eq!(nom_travail("Jeu", false), "Export de « Jeu »");
        assert_eq!(
            nom_travail("Jeu", true),
            "Export de « Jeu » (encodeur logiciel)"
        );
        assert_eq!(
            (nom_piste(0), nom_piste(1), nom_piste(2)),
            ("Jeu".into(), "Micro".into(), "Piste 3".into())
        );
    }

    #[test]
    fn source_tiree_des_infos_du_clip() {
        let infos = Infos {
            duree_s: 3.0,
            codec: None,
            largeur: 640,
            hauteur: 360,
            ips: 30,
            pistes_audio: 7,
        };
        assert_eq!(
            Source::de("a.mp4", &infos),
            Source {
                nom: "a.mp4".into(),
                duree_s: 3.0,
                ips: 30,
                pistes: 4,
                largeur: 640,
                hauteur: 360
            }
        );
        assert_eq!(sources_de(&[s()]).get("Jeu.mp4"), Some(&s()));
    }
}
