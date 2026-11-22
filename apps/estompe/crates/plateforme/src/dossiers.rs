use crate::{Erreur, Resultat};
use std::path::{Path, PathBuf};

pub const IDENTIFIANT: &str = "fr.plbls.estompe.natif";
pub const VARIABLE_DOSSIER: &str = "ESTOMPE_NATIF_DOSSIER";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dossiers {
    pub donnees: PathBuf,
    pub cache: PathBuf,
    pub journaux: PathBuf,
}

impl Dossiers {
    pub fn de_l_os() -> Resultat<Self> {
        if let Some(racine) = std::env::var_os(VARIABLE_DOSSIER).filter(|v| !v.is_empty()) {
            return Ok(Self::sous(Path::new(&racine)));
        }
        let (donnees, locales, cache) =
            racines().ok_or(Erreur::Indisponible("dossiers de l'utilisateur"))?;
        Ok(Self::selon(&donnees, &locales, &cache, IDENTIFIANT))
    }

    pub fn sous(racine: &Path) -> Self {
        Self {
            donnees: racine.join("donnees"),
            cache: racine.join("cache"),
            journaux: racine.join("journaux"),
        }
    }

    fn selon(donnees: &Path, locales: &Path, cache: &Path, identifiant: &str) -> Self {
        Self {
            donnees: donnees.join(identifiant),
            cache: cache.join(identifiant).join("cache"),
            journaux: locales.join(identifiant).join("journaux"),
        }
    }

    pub fn creer(&self) -> Resultat<()> {
        for dossier in [&self.donnees, &self.cache, &self.journaux] {
            std::fs::create_dir_all(dossier)
                .map_err(|e| Erreur::Echec(format!("{} : {e}", dossier.display())))?;
        }
        Ok(())
    }
}

pub const DOSSIER_TELECHARGEMENTS_ESSAI: &str = "telechargements";

pub fn dossier_telechargements() -> Option<PathBuf> {
    if let Some(racine) = std::env::var_os(VARIABLE_DOSSIER).filter(|v| !v.is_empty()) {
        return Some(Path::new(&racine).join(DOSSIER_TELECHARGEMENTS_ESSAI));
    }
    telechargements_de_l_os()
}

pub const DOSSIER_VIDEOS_ESSAI: &str = "videos";

pub fn dossier_videos() -> Option<PathBuf> {
    if let Some(racine) = std::env::var_os(VARIABLE_DOSSIER).filter(|v| !v.is_empty()) {
        return Some(Path::new(&racine).join(DOSSIER_VIDEOS_ESSAI));
    }
    videos_de_l_os()
}

#[cfg(windows)]
fn videos_de_l_os() -> Option<PathBuf> {
    Some(variable("USERPROFILE")?.join("Videos"))
}

#[cfg(not(windows))]
fn videos_de_l_os() -> Option<PathBuf> {
    utilisateur("XDG_VIDEOS_DIR", "Videos")
}

#[cfg(windows)]
fn telechargements_de_l_os() -> Option<PathBuf> {
    Some(variable("USERPROFILE")?.join("Downloads"))
}

#[cfg(not(windows))]
fn telechargements_de_l_os() -> Option<PathBuf> {
    utilisateur("XDG_DOWNLOAD_DIR", "Downloads")
}

#[cfg(not(windows))]
fn utilisateur(cle: &str, defaut: &str) -> Option<PathBuf> {
    let maison = variable("HOME")?;
    let configuration = variable("XDG_CONFIG_HOME").unwrap_or_else(|| maison.join(".config"));
    let declare = std::fs::read_to_string(configuration.join("user-dirs.dirs"))
        .ok()
        .and_then(|texte| declaration(&texte, &maison, cle));
    Some(declare.unwrap_or_else(|| maison.join(defaut)))
}

#[cfg(not(windows))]
fn declaration(texte: &str, maison: &Path, cle: &str) -> Option<PathBuf> {
    let prefixe = format!("{cle}=");
    let valeur = texte
        .lines()
        .filter_map(|l| l.trim().strip_prefix(prefixe.as_str()))
        .next()?
        .trim()
        .trim_matches('"');
    let chemin = match valeur.strip_prefix("$HOME") {
        Some(reste) => maison.join(reste.trim_start_matches('/')),
        None => PathBuf::from(valeur),
    };
    (chemin.is_absolute() && chemin != maison).then_some(chemin)
}

fn variable(nom: &str) -> Option<PathBuf> {
    std::env::var_os(nom)
        .map(PathBuf::from)
        .filter(|chemin| chemin.is_absolute())
}

#[cfg(windows)]
fn racines() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let locales = variable("LOCALAPPDATA")?;
    Some((variable("APPDATA")?, locales.clone(), locales))
}

#[cfg(not(windows))]
fn racines() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let maison = variable("HOME");
    let ou = |nom: &str, defaut: &str| variable(nom).or_else(|| Some(maison.clone()?.join(defaut)));
    let donnees = ou("XDG_DATA_HOME", ".local/share")?;
    Some((donnees.clone(), donnees, ou("XDG_CACHE_HOME", ".cache")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAURI: [&str; 2] = ["fr.plbls.estompe", "fr.plbls.estompe.test"];

    #[test]
    fn aucun_dossier_n_est_partage_avec_l_app_tauri() {
        let (donnees, locales, cache) = (
            Path::new("/u/donnees"),
            Path::new("/u/locales"),
            Path::new("/u/cache"),
        );
        let natif = Dossiers::selon(donnees, locales, cache, IDENTIFIANT);
        for identifiant in TAURI {
            for racine in [donnees, locales, cache] {
                let tauri = racine.join(identifiant);
                for dossier in [&natif.donnees, &natif.cache, &natif.journaux] {
                    assert!(
                        !dossier.starts_with(&tauri) && !tauri.starts_with(dossier),
                        "{} et {} se recouvrent",
                        dossier.display(),
                        tauri.display()
                    );
                }
            }
        }
    }

    #[test]
    fn les_trois_dossiers_sont_distincts_meme_si_l_os_donne_la_meme_racine() {
        let racine = Path::new("/u/AppData/Local");
        let d = Dossiers::selon(racine, racine, racine, IDENTIFIANT);
        assert_ne!(d.donnees, d.cache);
        assert_ne!(d.donnees, d.journaux);
        assert_ne!(d.cache, d.journaux);
        assert!(d.cache.starts_with(&d.donnees) && d.journaux.starts_with(&d.donnees));
    }

    #[cfg(not(windows))]
    fn dossier_declare(texte: &str, maison: &Path) -> Option<PathBuf> {
        declaration(texte, maison, "XDG_DOWNLOAD_DIR")
    }

    #[cfg(not(windows))]
    #[test]
    fn le_dossier_des_telechargements_suit_user_dirs() {
        let maison = Path::new("/home/u");
        assert_eq!(
            dossier_declare("# x\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n", maison),
            Some(PathBuf::from("/home/u/Téléchargements"))
        );
        assert_eq!(
            dossier_declare("XDG_DOWNLOAD_DIR=\"/data/dl\"", maison),
            Some(PathBuf::from("/data/dl"))
        );
        assert_eq!(dossier_declare("XDG_DOWNLOAD_DIR=\"$HOME/\"", maison), None);
        assert_eq!(
            dossier_declare("XDG_DOWNLOAD_DIR=\"relatif\"", maison),
            None
        );
        assert_eq!(dossier_declare("XDG_MUSIC_DIR=\"$HOME/M\"", maison), None);
        assert_eq!(
            declaration("XDG_VIDEOS_DIR=\"$HOME/Vidéos\"", maison, "XDG_VIDEOS_DIR"),
            Some(PathBuf::from("/home/u/Vidéos"))
        );
    }

    #[test]
    fn creer_cree_les_trois_dossiers() {
        let tmp = tempfile::tempdir().unwrap();
        let d = Dossiers::sous(tmp.path());
        d.creer().unwrap();
        assert!(d.donnees.is_dir() && d.cache.is_dir() && d.journaux.is_dir());
    }
}
