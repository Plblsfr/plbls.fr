use crate::{ContenuPressePapiers, Erreur, Fichiers, Image, PressePapiers, Resultat};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct FichiersSysteme;

impl Fichiers for FichiersSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn choisir_dossier(&self, depart: Option<&Path>) -> Resultat<Option<PathBuf>> {
        let mut dialogue = rfd::FileDialog::new().set_title("Enregistrer dans…");
        if let Some(depart) = depart.filter(|d| d.is_dir()) {
            dialogue = dialogue.set_directory(depart);
        }
        Ok(dialogue.pick_folder())
    }

    fn choisir_fichiers(&self, depart: Option<&Path>) -> Resultat<Vec<PathBuf>> {
        let mut dialogue = rfd::FileDialog::new().set_title("Joindre des fichiers");
        if let Some(depart) = depart.filter(|d| d.is_dir()) {
            dialogue = dialogue.set_directory(depart);
        }
        Ok(dialogue.pick_files().unwrap_or_default())
    }

    fn ouvrir(&self, chemin: &Path) -> Resultat<()> {
        if !chemin.is_file() {
            return Err(Erreur::Refuse(format!(
                "{} n'est pas un fichier",
                chemin.display()
            )));
        }
        lancer_ouverture(chemin)
    }
}

#[cfg(windows)]
fn lancer_ouverture(chemin: &Path) -> Resultat<()> {
    std::process::Command::new("explorer")
        .arg(chemin)
        .spawn()
        .map(|_| ())
        .map_err(|e| Erreur::Echec(e.to_string()))
}

#[cfg(not(windows))]
fn lancer_ouverture(chemin: &Path) -> Resultat<()> {
    std::process::Command::new("xdg-open")
        .arg(chemin)
        .spawn()
        .map(|_| ())
        .map_err(|e| Erreur::Echec(e.to_string()))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PressePapiersSysteme;

impl PressePapiers for PressePapiersSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn copier_image(&self, image: &Image) -> Resultat<()> {
        let attendu = image.largeur as usize * image.hauteur as usize * 4;
        if image.largeur == 0 || image.hauteur == 0 || image.rgba.len() != attendu {
            return Err(Erreur::Refuse("image vide ou mal formée".into()));
        }
        let mut presse_papiers =
            arboard::Clipboard::new().map_err(|e| Erreur::Echec(e.to_string()))?;
        presse_papiers
            .set_image(arboard::ImageData {
                width: image.largeur as usize,
                height: image.hauteur as usize,
                bytes: std::borrow::Cow::Borrowed(&image.rgba),
            })
            .map_err(|e| Erreur::Echec(e.to_string()))
    }

    fn lire_pieces(&self) -> Resultat<Option<ContenuPressePapiers>> {
        let mut presse_papiers =
            arboard::Clipboard::new().map_err(|e| Erreur::Echec(e.to_string()))?;
        let fichiers = presse_papiers.get().file_list().unwrap_or_default();
        if !fichiers.is_empty() {
            return Ok(Some(ContenuPressePapiers::Fichiers(fichiers)));
        }
        match presse_papiers.get_image() {
            Ok(image) => Ok(image_lue(
                image.width,
                image.height,
                image.bytes.into_owned(),
            )),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(Erreur::Echec(e.to_string())),
        }
    }
}

fn image_lue(largeur: usize, hauteur: usize, rgba: Vec<u8>) -> Option<ContenuPressePapiers> {
    let (largeur, hauteur) = (u32::try_from(largeur).ok()?, u32::try_from(hauteur).ok()?);
    let attendu = largeur as usize * hauteur as usize * 4;
    if largeur == 0 || hauteur == 0 || rgba.len() != attendu {
        return None;
    }
    Some(ContenuPressePapiers::Image(Image {
        largeur,
        hauteur,
        rgba,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ouvrir_refuse_ce_qui_n_est_pas_un_fichier() {
        let dossier = tempfile::tempdir().unwrap();
        assert!(matches!(
            FichiersSysteme.ouvrir(dossier.path()),
            Err(Erreur::Refuse(_))
        ));
        assert!(matches!(
            FichiersSysteme.ouvrir(&dossier.path().join("absent.txt")),
            Err(Erreur::Refuse(_))
        ));
    }

    #[test]
    fn une_image_mal_formee_n_atteint_pas_le_presse_papiers() {
        let image = Image {
            largeur: 2,
            hauteur: 2,
            rgba: vec![0; 3],
        };
        assert!(matches!(
            PressePapiersSysteme.copier_image(&image),
            Err(Erreur::Refuse(_))
        ));
    }

    #[test]
    fn une_image_lue_mal_formee_est_ignoree() {
        assert_eq!(image_lue(0, 2, Vec::new()), None);
        assert_eq!(image_lue(2, 2, vec![0; 15]), None);
        assert_eq!(
            image_lue(1, 1, vec![1, 2, 3, 4]),
            Some(ContenuPressePapiers::Image(Image {
                largeur: 1,
                hauteur: 1,
                rgba: vec![1, 2, 3, 4]
            }))
        );
    }
}
