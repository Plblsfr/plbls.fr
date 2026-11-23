use std::collections::HashSet;
use std::sync::Arc;

use estompe_client::profil::{
    banniere_valide, deplacer, ma_banniere, photos_par_serveur, zoom_lisible, zoomer, Recadrage,
    COTE_ENVOYE, NUANCES_BANNIERE,
};
use estompe_client::store::Changement;
use estompe_plateforme::FiltreFichiers;
use image::RgbaImage;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedPixelBuffer, VecModel};

use super::super::avatars::CleAvatar;
use super::super::profil::{
    avatar_recadre, decoupe, encoder, lire_image, recadrer, EXTENSIONS_IMAGES,
};
use super::{couleur_slint, Pont};
use crate::{DonneesCoquille, DonneesProfil, NuanceBanniere, PhotoServeurProfil, Principale};

pub const MOI: &str = "moi";
pub const TITRE_DIALOGUE: &str = "Choisir une image";
pub const FILTRE_IMAGES: FiltreFichiers = FiltreFichiers {
    nom: "Images",
    extensions: EXTENSIONS_IMAGES,
};
pub const ERREUR_AVATAR: &str = "Image impossible à utiliser comme avatar.";
pub const ERREUR_PHOTO: &str = "Image impossible à utiliser comme photo de serveur.";
pub const ERREUR_SUPPRESSION: &str = "Photo non supprimée.";
pub const ERREUR_BANNIERE: &str = "Couleur non enregistrée.";
pub const TITRE_ICONE: &str = "Recadrer l'icône du serveur";

#[derive(Clone)]
struct EnCours {
    cible: Option<String>,
    icone: bool,
    image: Arc<RgbaImage>,
    recadrage: Recadrage,
}

#[derive(Default)]
pub(super) struct EtatProfil {
    recadrage: Option<EnCours>,
    occupe: Option<String>,
    erreur: String,
    demandes: HashSet<CleAvatar>,
    echecs: HashSet<CleAvatar>,
}

pub fn touche_le_profil(changements: &[Changement], moi: &str) -> bool {
    changements.iter().any(|c| match c {
        Changement::Compte
        | Changement::ServeurAjoute { .. }
        | Changement::ServeurRetire { .. }
        | Changement::Serveur { .. }
        | Changement::OrdreServeurs => true,
        Changement::Avatar { utilisateur, .. } | Changement::Membre { utilisateur, .. } => {
            utilisateur == moi
        }
        _ => false,
    })
}

fn erreur_d_image(cible: &Option<String>) -> &'static str {
    match cible {
        Some(_) => ERREUR_PHOTO,
        None => ERREUR_AVATAR,
    }
}

fn poser<T: Clone + PartialEq + 'static>(
    modele: ModelRc<T>,
    lignes: Vec<T>,
    poser: impl Fn(ModelRc<T>),
) {
    let Some(v) = modele.as_any().downcast_ref::<VecModel<T>>() else {
        poser(ModelRc::new(VecModel::from(lignes)));
        return;
    };
    if v.row_count() != lignes.len() {
        v.set_vec(lignes);
        return;
    }
    for (i, l) in lignes.into_iter().enumerate() {
        if v.row_data(i).as_ref() != Some(&l) {
            v.set_row_data(i, l);
        }
    }
}

pub fn image_slint(image: &RgbaImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

impl Pont {
    pub fn projeter_profil(&self, f: &Principale) {
        let (moi, version, photos, banniere) = self.store.lire(|etat| {
            (
                etat.moi().to_string(),
                etat.version_avatar(etat.moi()),
                photos_par_serveur(etat),
                ma_banniere(etat),
            )
        });
        let globale = (version > 0).then(|| CleAvatar {
            utilisateur: moi.clone(),
            version,
            serveur: None,
        });
        let mut a_demander = Vec::new();
        let mut portrait_de = |cle: &Option<CleAvatar>,
                               demandes: &mut HashSet<CleAvatar>,
                               echecs: &HashSet<CleAvatar>| {
            let cle = cle.as_ref()?;
            let image = self.profils.avatar(cle);
            if image.is_none() && !echecs.contains(cle) && demandes.insert(cle.clone()) {
                a_demander.push(cle.clone());
            }
            image.map(Image::from_rgba8)
        };
        let (portrait, serveurs, occupe, erreur) = {
            let mut guard = self.verrou();
            let EtatProfil {
                demandes,
                echecs,
                occupe,
                erreur,
                ..
            } = &mut guard.profil;
            let portrait = portrait_de(&globale, demandes, echecs);
            let serveurs: Vec<PhotoServeurProfil> = photos
                .iter()
                .map(|p| {
                    let cle = if p.version > 0 {
                        Some(CleAvatar {
                            utilisateur: moi.clone(),
                            version: p.version,
                            serveur: Some(p.serveur.clone()),
                        })
                    } else {
                        globale.clone()
                    };
                    PhotoServeurProfil {
                        id: p.serveur.as_str().into(),
                        nom: p.nom.as_str().into(),
                        portrait: portrait_de(&cle, demandes, echecs).unwrap_or_default(),
                        propre: p.version > 0,
                    }
                })
                .collect();
            (portrait, serveurs, occupe.clone(), erreur.clone())
        };
        let nuances: Vec<NuanceBanniere> = NUANCES_BANNIERE
            .iter()
            .map(|code| NuanceBanniere {
                code: (*code).into(),
                couleur: couleur_slint(Some(code)).unwrap_or_default(),
                choisie: banniere == *code,
            })
            .collect();
        let portrait = portrait.unwrap_or_default();
        f.global::<DonneesCoquille>().set_portrait(portrait.clone());
        let g = f.global::<DonneesProfil>();
        g.set_portrait(portrait);
        g.set_a_avatar(version > 0);
        poser(g.get_serveurs(), serveurs, |m| g.set_serveurs(m));
        poser(g.get_nuances(), nuances, |m| g.set_nuances(m));
        g.set_banniere_du_role(banniere.is_empty());
        g.set_occupe(occupe.unwrap_or_default().into());
        g.set_erreur(erreur.into());
        for cle in a_demander {
            self.telecharger_portrait(cle);
        }
    }

    fn telecharger_portrait(&self, cle: CleAvatar) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let recu = pont.profils.charger_avatar(cle.clone()).await.is_some();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut guard = p.verrou();
                    guard.profil.demandes.remove(&cle);
                    if !recu {
                        guard.profil.echecs.insert(cle);
                    }
                }
                p.projeter_profil(&f);
            });
        });
    }

    fn terminer(&self, erreur: Option<&str>) {
        {
            let mut guard = self.verrou();
            guard.profil.occupe = None;
            if let Some(erreur) = erreur {
                guard.profil.erreur = erreur.to_string();
            }
        }
        let p = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| p.projeter_profil(&f));
    }

    fn commencer(&self, f: &Principale, cible: &Option<String>) -> bool {
        {
            let mut guard = self.verrou();
            if guard.profil.occupe.is_some() || guard.profil.recadrage.is_some() {
                return false;
            }
            guard.profil.occupe = Some(cible.clone().unwrap_or_else(|| MOI.to_string()));
            guard.profil.erreur.clear();
        }
        self.projeter_profil(f);
        true
    }

    pub fn changer_avatar(&self, f: &Principale, cible: &str) {
        let cible = (!cible.is_empty()).then(|| cible.to_string());
        let fichiers = self.messagerie.fichiers();
        if !fichiers.disponible() || !self.commencer(f, &cible) {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-avatar".into())
            .spawn(move || {
                let chemin = match fichiers.choisir_fichier(TITRE_DIALOGUE, &FILTRE_IMAGES, None) {
                    Ok(Some(chemin)) => chemin,
                    _ => return pont.terminer(None),
                };
                let Some(image) = lire_image(&chemin) else {
                    return pont.terminer(Some(erreur_d_image(&cible)));
                };
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.verrou().profil.occupe = None;
                    p.ouvrir_recadrage(&f, cible, image);
                });
            });
    }

    pub fn ouvrir_recadrage(&self, f: &Principale, cible: Option<String>, image: RgbaImage) {
        let titre = match &cible {
            Some(serveur) => {
                let nom = self.store.lire(|etat| {
                    etat.serveur(serveur)
                        .and_then(|s| s.name.clone())
                        .unwrap_or_default()
                });
                format!("Recadrer la photo pour {nom}")
            }
            None => "Recadrer l'avatar".to_string(),
        };
        self.montrer_recadrage(f, &titre, cible, false, image);
    }

    pub fn ouvrir_recadrage_icone(&self, f: &Principale, image: RgbaImage) {
        if self.verrou().profil.recadrage.is_some() {
            return;
        }
        self.montrer_recadrage(f, TITRE_ICONE, None, true, image);
    }

    fn montrer_recadrage(
        &self,
        f: &Principale,
        titre: &str,
        cible: Option<String>,
        icone: bool,
        image: RgbaImage,
    ) {
        let g = f.global::<DonneesProfil>();
        g.set_source(image_slint(&image));
        g.set_recadrage_titre(titre.into());
        g.set_envoi(false);
        self.verrou().profil.recadrage = Some(EnCours {
            cible,
            icone,
            image: Arc::new(image),
            recadrage: Recadrage::default(),
        });
        self.projeter_recadrage(f);
        self.projeter_profil(f);
        g.set_recadrage_ouvert(true);
    }

    fn projeter_recadrage(&self, f: &Principale) {
        let Some(en_cours) = self.verrou().profil.recadrage.clone() else {
            return;
        };
        let (largeur, hauteur) = en_cours.image.dimensions();
        let (x, y, cote) = decoupe(largeur, hauteur, en_cours.recadrage);
        let g = f.global::<DonneesProfil>();
        g.set_clip_x(x as i32);
        g.set_clip_y(y as i32);
        g.set_clip_cote(cote as i32);
        g.set_zoom(en_cours.recadrage.zoom);
        g.set_zoom_lisible(zoom_lisible(en_cours.recadrage.zoom).into());
    }

    fn modifier_recadrage(&self, f: &Principale, modifier: impl FnOnce(&EnCours) -> Recadrage) {
        {
            let mut guard = self.verrou();
            let Some(en_cours) = guard.profil.recadrage.as_mut() else {
                return;
            };
            en_cours.recadrage = modifier(en_cours);
        }
        self.projeter_recadrage(f);
    }

    pub fn deplacer_recadrage(&self, f: &Principale, dx: f32, dy: f32) {
        self.modifier_recadrage(f, |e| {
            let (l, h) = e.image.dimensions();
            deplacer(e.recadrage, l, h, dx, dy)
        });
    }

    pub fn zoomer_recadrage(&self, f: &Principale, zoom: f32) {
        self.modifier_recadrage(f, |e| zoomer(e.recadrage, zoom));
    }

    pub fn recadrage_en_cours(&self) -> Option<Recadrage> {
        self.verrou().profil.recadrage.as_ref().map(|e| e.recadrage)
    }

    pub fn fermer_recadrage(&self, f: &Principale) {
        self.verrou().profil.recadrage = None;
        let g = f.global::<DonneesProfil>();
        g.set_recadrage_ouvert(false);
        g.set_envoi(false);
        g.set_source(Image::default());
        self.projeter_profil(f);
    }

    pub fn valider_recadrage(&self, f: &Principale) {
        let icone = self
            .verrou()
            .profil
            .recadrage
            .as_ref()
            .filter(|e| e.icone)
            .cloned();
        if let Some(en_cours) = icone {
            let carre = recadrer(&en_cours.image, en_cours.recadrage, COTE_ENVOYE);
            let encode = encoder(&carre).ok();
            self.fermer_recadrage(f);
            self.icone_recadree(f, encode.map(|(mime, octets)| (mime, octets, carre)));
            return;
        }
        let en_cours = {
            let mut guard = self.verrou();
            let Some(en_cours) = guard.profil.recadrage.clone() else {
                return;
            };
            if guard.profil.occupe.is_some() {
                return;
            }
            guard.profil.occupe = Some(en_cours.cible.clone().unwrap_or_else(|| MOI.to_string()));
            guard.profil.erreur.clear();
            en_cours
        };
        f.global::<DonneesProfil>().set_envoi(true);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let client = pont.store.client().clone();
            let envoye = match avatar_recadre(&en_cours.image, en_cours.recadrage) {
                Err(_) => false,
                Ok((mime, octets)) => match &en_cours.cible {
                    Some(serveur) => client
                        .definir_avatar_de_serveur(serveur, mime, octets)
                        .await
                        .is_ok(),
                    None => client.definir_avatar(mime, octets).await.is_ok(),
                },
            };
            {
                let mut guard = pont.verrou();
                guard.profil.occupe = None;
                guard.profil.recadrage = None;
                if !envoye {
                    guard.profil.erreur = erreur_d_image(&en_cours.cible).to_string();
                }
            }
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.fermer_recadrage(&f));
        });
    }

    pub fn retirer_avatar(&self, f: &Principale, cible: &str) {
        let cible = (!cible.is_empty()).then(|| cible.to_string());
        if !self.commencer(f, &cible) {
            return;
        }
        let pont = self.clone();
        self.runtime.spawn(async move {
            let client = pont.store.client().clone();
            let fait = match &cible {
                Some(serveur) => client.supprimer_avatar_de_serveur(serveur).await,
                None => client.supprimer_avatar().await,
            };
            pont.terminer(fait.is_err().then_some(ERREUR_SUPPRESSION));
        });
    }

    pub fn choisir_banniere(&self, f: &Principale, couleur: &str) {
        let couleur = couleur.to_lowercase();
        if !banniere_valide(&couleur) || !self.commencer(f, &None) {
            return;
        }
        let pont = self.clone();
        self.runtime.spawn(async move {
            let fait = pont.store.client().definir_banniere(&couleur).await;
            pont.terminer(fait.is_err().then_some(ERREUR_BANNIERE));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_profil_suit_mes_avatars_mes_serveurs_et_ma_banniere() {
        let avatar = |utilisateur: &str, serveur: Option<&str>| Changement::Avatar {
            utilisateur: utilisateur.into(),
            serveur: serveur.map(str::to_string),
        };
        assert!(touche_le_profil(&[avatar("me", None)], "me"));
        assert!(touche_le_profil(&[avatar("me", Some("s1"))], "me"));
        assert!(!touche_le_profil(&[avatar("bob", None)], "me"));
        assert!(touche_le_profil(
            &[Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "me".into()
            }],
            "me"
        ));
        assert!(touche_le_profil(&[Changement::OrdreServeurs], "me"));
        assert!(!touche_le_profil(&[Changement::MessagesPrives], "me"));
        assert_eq!(erreur_d_image(&None), ERREUR_AVATAR);
        assert_eq!(erreur_d_image(&Some("s1".into())), ERREUR_PHOTO);
    }
}
