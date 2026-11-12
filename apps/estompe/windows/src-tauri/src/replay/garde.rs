use std::path::PathBuf;
use std::sync::Mutex;

const FICHIER: &str = "estompe-amf.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Etat {
    Libre,
    Essai,
    Valide,
    Interdit,
}

pub fn etat_lu(contenu: Option<&str>, version: &str) -> Etat {
    match contenu.map(str::trim).and_then(|c| c.split_once(' ')) {
        Some(("essai", v)) if v == version => Etat::Interdit,
        Some(("valide", v)) if v == version => Etat::Valide,
        _ => Etat::Libre,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Essai {
    pub h264: Option<(u32, u32)>,
    pub hevc: Option<(u32, u32)>,
}

pub const FIN_ESSAI: &str = "amf fin";

pub fn ligne_essai(codec: &str, taille: (u32, u32)) -> String {
    format!("amf {codec} {} {}", taille.0, taille.1)
}

pub fn lire_essai(sortie: &str) -> Option<Essai> {
    let mut essai = Essai::default();
    let mut fini = false;
    for ligne in sortie.lines().map(str::trim) {
        let mots: Vec<&str> = ligne.split_whitespace().collect();
        match mots.as_slice() {
            ["amf", "fin"] => fini = true,
            ["amf", codec, largeur, hauteur] => {
                let taille = largeur.parse().ok().zip(hauteur.parse().ok());
                match *codec {
                    "h264" => essai.h264 = taille,
                    "hevc" => essai.hevc = taille,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fini.then_some(essai)
}

pub struct Garde {
    fichier: Option<PathBuf>,
    version: String,
    etat: Mutex<Etat>,
}

impl Garde {
    pub fn charger(dossier: Option<PathBuf>, version: &str) -> Self {
        let fichier = dossier.map(|d| d.join(FICHIER));
        let contenu = fichier.as_ref().and_then(|f| std::fs::read_to_string(f).ok());
        Self { fichier, version: version.to_string(), etat: Mutex::new(etat_lu(contenu.as_deref(), version)) }
    }

    fn passer(&self, de: &[Etat], vers: Etat, mot: &str) {
        let Ok(mut etat) = self.etat.lock() else { return };
        if !de.contains(&etat) {
            return;
        }
        *etat = vers;
        if let Some(fichier) = &self.fichier {
            if let Some(dossier) = fichier.parent() {
                let _ = std::fs::create_dir_all(dossier);
            }
            let _ = std::fs::write(fichier, format!("{mot} {}", self.version));
        }
    }

    pub fn permis(&self) -> bool {
        self.etat.lock().map_or(true, |e| *e != Etat::Interdit)
    }

    pub fn debut(&self) {
        self.passer(&[Etat::Libre], Etat::Essai, "essai");
    }

    pub fn relacher(&self) {
        self.passer(&[Etat::Essai], Etat::Libre, "libre");
    }

    pub fn valider(&self) {
        self.passer(&[Etat::Essai], Etat::Valide, "valide");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essai_interrompu_interdit_jusqu_a_la_version_suivante() {
        assert_eq!(etat_lu(None, "1.0.0"), Etat::Libre);
        assert_eq!(etat_lu(Some("essai 1.0.0\n"), "1.0.0"), Etat::Interdit);
        assert_eq!(etat_lu(Some("essai 1.0.0"), "1.0.1"), Etat::Libre);
        assert_eq!(etat_lu(Some("valide 1.0.0"), "1.0.0"), Etat::Valide);
        assert_eq!(etat_lu(Some("valide 1.0.0"), "1.0.1"), Etat::Libre);
        assert_eq!(etat_lu(Some("libre 1.0.0"), "1.0.0"), Etat::Libre);
        assert_eq!(etat_lu(Some("n'importe quoi"), "1.0.0"), Etat::Libre);
    }

    #[test]
    fn essai_hors_processus_lu_seulement_s_il_est_complet() {
        let complet = format!("{}\r\namf refus hevc Init : code 1\n{}\n{FIN_ESSAI}\n", ligne_essai("h264", (4096, 2160)), ligne_essai("av1", (1, 1)));
        assert_eq!(lire_essai(&complet), Some(Essai { h264: Some((4096, 2160)), hevc: None }));
        assert_eq!(lire_essai(&ligne_essai("h264", (4096, 2160))), None);
        assert_eq!(lire_essai(""), None);
        assert_eq!(lire_essai(FIN_ESSAI), Some(Essai::default()));
        assert_eq!(lire_essai("amf hevc x 2160\namf fin"), Some(Essai::default()));
    }

    #[test]
    fn essai_ecrit_puis_valide_ou_relache() {
        let dossier = std::env::temp_dir().join(format!("estompe-garde-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        let lire = || std::fs::read_to_string(dossier.join(FICHIER)).unwrap_or_default();
        let garde = Garde::charger(Some(dossier.clone()), "1.0.0");
        assert!(garde.permis());
        garde.valider();
        assert_eq!(lire(), "");
        garde.debut();
        assert_eq!(lire(), "essai 1.0.0");
        assert!(!Garde::charger(Some(dossier.clone()), "1.0.0").permis());
        assert!(Garde::charger(Some(dossier.clone()), "1.0.1").permis());
        garde.relacher();
        assert_eq!(lire(), "libre 1.0.0");
        garde.debut();
        garde.valider();
        assert_eq!(lire(), "valide 1.0.0");
        garde.debut();
        garde.relacher();
        assert_eq!(lire(), "valide 1.0.0");
        assert!(Garde::charger(Some(dossier.clone()), "1.0.0").permis());
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
