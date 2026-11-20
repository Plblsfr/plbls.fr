use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::derniere_vue::DerniereVue;

pub const NOM_FICHIER: &str = "preferences.json";
const SUFFIXE_CORROMPU: &str = "corrompu";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Auto,
    Clair,
    Sombre,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Densite {
    #[default]
    Confortable,
    Compacte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fenetre {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preferences {
    pub theme: Theme,
    pub densite: Densite,
    pub reduire_animations: bool,
    pub notifications: bool,
    pub apercus: bool,
    pub journal_debogage: bool,
    pub ordre_serveurs: Vec<String>,
    pub fenetre: Option<Fenetre>,
    pub adresse_serveur: Option<String>,
    pub derniere_vue: Option<DerniereVue>,
    pub panneau_replie: bool,
    pub marge_ouverte: bool,
    pub emoji_recents: Vec<String>,
    pub recherches_recentes: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            densite: Densite::Confortable,
            reduire_animations: false,
            notifications: true,
            apercus: true,
            journal_debogage: false,
            ordre_serveurs: Vec::new(),
            fenetre: None,
            adresse_serveur: None,
            derniere_vue: None,
            panneau_replie: false,
            marge_ouverte: true,
            emoji_recents: Vec::new(),
            recherches_recentes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatFichier {
    Lu,
    Absent,
    Corrompu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chargement {
    pub preferences: Preferences,
    pub etat: EtatFichier,
}

fn champ<T: DeserializeOwned>(objet: &Map<String, Value>, cle: &str, defaut: T) -> T {
    objet
        .get(cle)
        .and_then(|valeur| T::deserialize(valeur).ok())
        .unwrap_or(defaut)
}

fn avec_suffixe(chemin: &Path, suffixe: &str) -> PathBuf {
    let mut nom = chemin.file_name().unwrap_or_default().to_os_string();
    nom.push(".");
    nom.push(suffixe);
    chemin.with_file_name(nom)
}

impl Preferences {
    pub fn chemin(dossier: &Path) -> PathBuf {
        dossier.join(NOM_FICHIER)
    }

    pub fn depuis_json(texte: &str) -> Option<Self> {
        let Ok(Value::Object(objet)) = serde_json::from_str::<Value>(texte) else {
            return None;
        };
        let d = Self::default();
        Some(Self {
            theme: champ(&objet, "theme", d.theme),
            densite: champ(&objet, "densite", d.densite),
            reduire_animations: champ(&objet, "reduire_animations", d.reduire_animations),
            notifications: champ(&objet, "notifications", d.notifications),
            apercus: champ(&objet, "apercus", d.apercus),
            journal_debogage: champ(&objet, "journal_debogage", d.journal_debogage),
            ordre_serveurs: champ(&objet, "ordre_serveurs", d.ordre_serveurs),
            fenetre: champ(&objet, "fenetre", d.fenetre)
                .filter(|f: &Fenetre| f.largeur > 0 && f.hauteur > 0),
            adresse_serveur: champ(&objet, "adresse_serveur", d.adresse_serveur)
                .filter(|a: &String| !a.trim().is_empty()),
            derniere_vue: champ(&objet, "derniere_vue", d.derniere_vue)
                .filter(|v: &DerniereVue| !v.compte.is_empty() && !v.adresse.is_empty()),
            panneau_replie: champ(&objet, "panneau_replie", d.panneau_replie),
            marge_ouverte: champ(&objet, "marge_ouverte", d.marge_ouverte),
            emoji_recents: objet
                .get("emoji_recents")
                .map(|v| crate::emoji::charger_recents(&v.to_string()))
                .unwrap_or_default(),
            recherches_recentes: objet
                .get("recherches_recentes")
                .map(|v| crate::emoji::charger_recents(&v.to_string()))
                .unwrap_or_default()
                .into_iter()
                .take(crate::recherche::RECENTES_MAX)
                .collect(),
        })
    }

    pub fn charger(dossier: &Path) -> Chargement {
        let chemin = Self::chemin(dossier);
        let octets = match fs::read(&chemin) {
            Ok(octets) => octets,
            Err(_) if !chemin.exists() => {
                return Chargement {
                    preferences: Self::default(),
                    etat: EtatFichier::Absent,
                }
            }
            Err(_) => Vec::new(),
        };
        if let Some(preferences) = std::str::from_utf8(&octets)
            .ok()
            .and_then(Self::depuis_json)
        {
            return Chargement {
                preferences,
                etat: EtatFichier::Lu,
            };
        }
        let preferences = Self::default();
        let _ = fs::rename(&chemin, avec_suffixe(&chemin, SUFFIXE_CORROMPU));
        let _ = preferences.enregistrer(dossier);
        Chargement {
            preferences,
            etat: EtatFichier::Corrompu,
        }
    }

    pub fn enregistrer(&self, dossier: &Path) -> io::Result<()> {
        fs::create_dir_all(dossier)?;
        let mut octets = serde_json::to_vec_pretty(self)?;
        octets.push(b'\n');
        ecrire_atomiquement(&Self::chemin(dossier), &octets)
    }
}

pub fn ecrire_atomiquement(chemin: &Path, octets: &[u8]) -> io::Result<()> {
    let provisoire = avec_suffixe(chemin, &format!("{}.tmp", std::process::id()));
    let ecriture = (|| {
        let mut fichier = File::create(&provisoire)?;
        fichier.write_all(octets)?;
        fichier.sync_all()?;
        drop(fichier);
        fs::rename(&provisoire, chemin)
    })();
    if ecriture.is_err() {
        let _ = fs::remove_file(&provisoire);
    }
    ecriture
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fichiers(dossier: &Path) -> Vec<String> {
        let mut noms: Vec<String> = fs::read_dir(dossier)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        noms.sort();
        noms
    }

    #[test]
    fn valeurs_par_defaut_sans_fichier() {
        let dossier = tempfile::tempdir().unwrap();
        let charge = Preferences::charger(dossier.path());
        assert_eq!(charge.etat, EtatFichier::Absent);
        assert_eq!(charge.preferences, Preferences::default());
        assert_eq!(charge.preferences.theme, Theme::Auto);
        assert_eq!(charge.preferences.densite, Densite::Confortable);
        assert!(charge.preferences.notifications && charge.preferences.apercus);
        assert!(!charge.preferences.reduire_animations && !charge.preferences.journal_debogage);
        assert_eq!(
            (
                charge.preferences.fenetre,
                charge.preferences.adresse_serveur
            ),
            (None, None)
        );
        assert!(fichiers(dossier.path()).is_empty());
    }

    #[test]
    fn un_fichier_corrompu_est_ignore_et_recree() {
        for corrompu in [
            &b"{\"theme\": \"som"[..],
            b"",
            b"[1, 2]",
            b"null",
            b"\xff\xfe\x00binaire",
            b"pas du json",
        ] {
            let dossier = tempfile::tempdir().unwrap();
            let chemin = Preferences::chemin(dossier.path());
            fs::write(&chemin, corrompu).unwrap();
            let charge = Preferences::charger(dossier.path());
            assert_eq!(charge.etat, EtatFichier::Corrompu);
            assert_eq!(charge.preferences, Preferences::default());
            let recree = fs::read_to_string(&chemin).unwrap();
            assert_eq!(
                Preferences::depuis_json(&recree),
                Some(Preferences::default())
            );
            assert_eq!(
                fs::read(avec_suffixe(&chemin, SUFFIXE_CORROMPU)).unwrap(),
                corrompu
            );
            assert_eq!(Preferences::charger(dossier.path()).etat, EtatFichier::Lu);
        }
    }

    #[test]
    fn un_dossier_a_la_place_du_fichier_ne_fait_pas_planter() {
        let dossier = tempfile::tempdir().unwrap();
        fs::create_dir(Preferences::chemin(dossier.path())).unwrap();
        let charge = Preferences::charger(dossier.path());
        assert_eq!(charge.etat, EtatFichier::Corrompu);
        assert_eq!(charge.preferences, Preferences::default());
        assert_eq!(Preferences::charger(dossier.path()).etat, EtatFichier::Lu);
    }

    #[test]
    fn enregistre_puis_relit_sans_laisser_de_fichier_provisoire() {
        let racine = tempfile::tempdir().unwrap();
        let dossier = racine.path().join("donnees");
        let preferences = Preferences {
            theme: Theme::Sombre,
            densite: Densite::Compacte,
            reduire_animations: true,
            notifications: false,
            apercus: false,
            journal_debogage: true,
            ordre_serveurs: vec!["s2".to_string(), "s1".to_string()],
            fenetre: Some(Fenetre {
                x: -1200,
                y: 40,
                largeur: 1280,
                hauteur: 720,
            }),
            adresse_serveur: Some("http://localhost:8080".to_string()),
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c9")),
            panneau_replie: true,
            marge_ouverte: false,
            emoji_recents: vec!["😂".to_string(), "👍".to_string()],
            recherches_recentes: vec!["de:carol vacances".to_string()],
        };
        preferences.enregistrer(&dossier).unwrap();
        assert_eq!(fichiers(&dossier), [NOM_FICHIER]);
        let charge = Preferences::charger(&dossier);
        assert_eq!(
            (charge.etat, &charge.preferences),
            (EtatFichier::Lu, &preferences)
        );
        Preferences::default().enregistrer(&dossier).unwrap();
        assert_eq!(fichiers(&dossier), [NOM_FICHIER]);
        assert_eq!(
            Preferences::charger(&dossier).preferences,
            Preferences::default()
        );
    }

    #[test]
    fn champs_inconnus_toleres_et_champs_manquants_par_defaut() {
        let lu = Preferences::depuis_json(r#"{"theme": "clair", "futur": {"a": 1}, "fenetre": {"x": 1, "y": 2, "largeur": 800, "hauteur": 600, "ecran": 2}}"#).unwrap();
        let attendu = Preferences {
            theme: Theme::Clair,
            fenetre: Some(Fenetre {
                x: 1,
                y: 2,
                largeur: 800,
                hauteur: 600,
            }),
            ..Preferences::default()
        };
        assert_eq!(lu, attendu);
        assert_eq!(Preferences::depuis_json("{}"), Some(Preferences::default()));
    }

    #[test]
    fn une_valeur_invalide_ne_perd_que_son_champ() {
        let lu = Preferences::depuis_json(
            r#"{"theme": "fluo", "densite": "compacte", "notifications": "oui", "apercus": false, "ordre_serveurs": [1], "fenetre": {"x": 0, "y": 0, "largeur": 0, "hauteur": 600}, "adresse_serveur": "  "}"#,
        )
        .unwrap();
        let attendu = Preferences {
            densite: Densite::Compacte,
            apercus: false,
            ..Preferences::default()
        };
        assert_eq!(lu, attendu);
        let negatif = Preferences::depuis_json(
            r#"{"fenetre": {"x": 0, "y": 0, "largeur": -5, "hauteur": 600}}"#,
        )
        .unwrap();
        assert_eq!(negatif.fenetre, None);
    }

    #[test]
    fn le_fichier_enregistre_est_du_json_lisible() {
        let dossier = tempfile::tempdir().unwrap();
        Preferences::default().enregistrer(dossier.path()).unwrap();
        let valeur: Value =
            serde_json::from_slice(&fs::read(Preferences::chemin(dossier.path())).unwrap())
                .unwrap();
        assert_eq!(valeur["theme"], "auto");
        assert_eq!(valeur["densite"], "confortable");
        assert_eq!(valeur["fenetre"], Value::Null);
        assert_eq!(valeur["adresse_serveur"], Value::Null);
        assert_eq!(valeur["derniere_vue"], Value::Null);
        assert_eq!(valeur["panneau_replie"], false);
        assert_eq!(valeur["marge_ouverte"], true);
    }

    #[test]
    fn la_derniere_vue_incomplete_est_ecartee() {
        let lu = Preferences::depuis_json(
            r##"{"derniere_vue": {"compte": "u1", "adresse": "#/dm/d1"}, "panneau_replie": true, "marge_ouverte": false}"##,
        )
        .unwrap();
        assert_eq!(
            (lu.derniere_vue, lu.panneau_replie, lu.marge_ouverte),
            (Some(DerniereVue::enregistrer("u1", "#/dm/d1")), true, false)
        );
        assert_eq!(
            Preferences::depuis_json(r#"{"emoji_recents": ["😂", 3, "👍"]}"#)
                .unwrap()
                .emoji_recents,
            ["😂", "👍"]
        );
        let recherches: Vec<String> = (0..12).map(|i| format!("q{i}")).collect();
        let lu = Preferences::depuis_json(
            &serde_json::json!({"recherches_recentes": recherches}).to_string(),
        )
        .unwrap();
        assert_eq!(lu.recherches_recentes, recherches[..10]);
        for incomplete in [
            r#"{"derniere_vue": {"compte": "", "adresse": "x"}}"#,
            r#"{"derniere_vue": {"compte": "u1"}}"#,
            r##"{"derniere_vue": "#/s/s1"}"##,
        ] {
            assert_eq!(
                Preferences::depuis_json(incomplete).unwrap().derniere_vue,
                None,
                "{incomplete}"
            );
        }
    }
}
