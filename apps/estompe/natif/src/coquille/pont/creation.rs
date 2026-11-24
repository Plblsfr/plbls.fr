use estompe_client::creation::{
    basculer, cinema_actif, code_saisi, erreur_de_rejoindre, erreur_nom_salon, est_vocal_ou_cinema,
    genre_de_depart, genres_proposes, libelle_limite, message_d_erreur, nom_de_serveur,
    peut_creer_salon, peut_creer_serveur, peut_quitter, peut_rejoindre_serveur, GenrePropose,
    NouveauSalon, LIMITES, MODES_LENTS,
};
use estompe_client::route::Route;
use estompe_client::store::GenreSalon;
use image::RgbaImage;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use super::super::profil::lire_image;
use super::super::projection::{initiales, serveur_courant};
use super::profil::{image_slint, FILTRE_IMAGES, TITRE_DIALOGUE};
use super::Pont;
use crate::confirmation::{self, Demande};
use crate::{
    ChoixAcces, DonneesCoquille, DonneesCreationSalon, DonneesCreerRejoindre, GenreSalonPropose,
    Principale,
};

pub const ERREUR_ICONE: &str = "Image impossible à utiliser comme icône.";
pub const ICONE_NON_ENVOYEE: &str = "Serveur créé, mais l'icône n'a pas été envoyée.";

#[derive(Clone)]
pub struct IconeChoisie {
    pub type_mime: &'static str,
    pub octets: Vec<u8>,
    pub apercu: RgbaImage,
}

#[derive(Default, Clone)]
pub struct EtatCreerRejoindre {
    pub nom: String,
    pub code: String,
    pub icone: Option<IconeChoisie>,
    pub en_cours: bool,
    pub choix_en_cours: bool,
    pub erreur: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtatCreationSalon {
    pub serveur: String,
    pub nouveau: NouveauSalon,
    pub genres: Vec<GenrePropose>,
    pub roles: Vec<(String, String)>,
    pub membres: Vec<(String, String)>,
    pub touche: bool,
    pub erreur_serveur: String,
    pub en_cours: bool,
}

#[derive(Default)]
pub(super) struct EtatCreation {
    creer_rejoindre: Option<EtatCreerRejoindre>,
    salon: Option<EtatCreationSalon>,
}

pub fn genre_depuis_texte(texte: &str) -> GenreSalon {
    match texte {
        "voice" => GenreSalon::Voice,
        "videos" => GenreSalon::Videos,
        "music" => GenreSalon::Music,
        "cinema" => GenreSalon::Cinema,
        _ => GenreSalon::Text,
    }
}

fn texte_du_genre(genre: GenreSalon) -> &'static str {
    match genre {
        GenreSalon::Voice => "voice",
        GenreSalon::Videos => "videos",
        GenreSalon::Music => "music",
        GenreSalon::Cinema => "cinema",
        GenreSalon::Text | GenreSalon::Inconnu => "text",
    }
}

impl EtatCreationSalon {
    pub fn erreur(&self, salons: &[estompe_client::store::Salon]) -> String {
        if !self.erreur_serveur.is_empty() {
            return self.erreur_serveur.clone();
        }
        erreur_nom_salon(&self.nouveau.nom, self.nouveau.genre(), salons).to_string()
    }

    pub fn valide(&self, salons: &[estompe_client::store::Salon]) -> bool {
        self.erreur_serveur.is_empty()
            && erreur_nom_salon(&self.nouveau.nom, self.nouveau.genre(), salons).is_empty()
    }
}

impl Pont {
    pub fn creer_rejoindre(&self) -> Option<EtatCreerRejoindre> {
        self.verrou().creation.creer_rejoindre.clone()
    }

    pub fn creation_salon(&self) -> Option<EtatCreationSalon> {
        self.verrou().creation.salon.clone()
    }

    fn modifier_creer_rejoindre<R>(
        &self,
        modification: impl FnOnce(&mut EtatCreerRejoindre) -> R,
    ) -> Option<R> {
        self.verrou()
            .creation
            .creer_rejoindre
            .as_mut()
            .map(modification)
    }

    fn modifier_creation_salon<R>(
        &self,
        modification: impl FnOnce(&mut EtatCreationSalon) -> R,
    ) -> Option<R> {
        self.verrou().creation.salon.as_mut().map(modification)
    }

    pub(super) fn rendre_le_focus(&self, f: &Principale) {
        if self.messagerie.peut_ecrire() {
            self.messagerie.saisie().demander_focus(f);
        } else {
            let g = f.global::<DonneesCoquille>();
            g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
        }
    }

    pub fn ouvrir_creer_rejoindre(&self, f: &Principale) {
        if self.verrou().creation.creer_rejoindre.is_some() {
            return;
        }
        self.verrou().creation.creer_rejoindre = Some(EtatCreerRejoindre::default());
        let (creer, rejoindre) = self
            .store
            .lire(|etat| (peut_creer_serveur(etat), peut_rejoindre_serveur(etat)));
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_peut_creer(creer);
        g.set_peut_rejoindre(rejoindre);
        g.set_nom("".into());
        g.set_code("".into());
        self.afficher_creer_rejoindre(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_creer_rejoindre(&self, f: &Principale) {
        if self.verrou().creation.creer_rejoindre.take().is_none() {
            return;
        }
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_ouvert(false);
        g.set_icone(Image::default());
        self.rendre_le_focus(f);
    }

    pub fn nom_de_serveur_modifie(&self, f: &Principale, texte: &str) {
        let nom = nom_de_serveur(texte);
        if nom != texte {
            f.global::<DonneesCreerRejoindre>()
                .set_nom(nom.as_str().into());
        }
        self.modifier_creer_rejoindre(|e| {
            e.nom = nom;
            e.erreur.clear();
        });
        self.afficher_creer_rejoindre(f);
    }

    pub fn code_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_creer_rejoindre(|e| {
            e.code = texte.to_string();
            e.erreur.clear();
        });
        self.afficher_creer_rejoindre(f);
    }

    pub fn choisir_icone_de_serveur(&self, _f: &Principale) {
        let fichiers = self.messagerie.fichiers();
        let libre = self
            .modifier_creer_rejoindre(|e| {
                if e.en_cours || e.choix_en_cours {
                    return false;
                }
                e.choix_en_cours = true;
                true
            })
            .unwrap_or(false);
        if !libre {
            return;
        }
        if !fichiers.disponible() {
            self.modifier_creer_rejoindre(|e| e.choix_en_cours = false);
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-icone".into())
            .spawn(move || {
                let image = match fichiers.choisir_fichier(TITRE_DIALOGUE, &FILTRE_IMAGES, None) {
                    Ok(Some(chemin)) => lire_image(&chemin).ok_or(ERREUR_ICONE),
                    _ => Err(""),
                };
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.modifier_creer_rejoindre(|e| e.choix_en_cours = false);
                    match image {
                        Ok(image) => p.recadrer_icone_de_serveur(&f, image),
                        Err(erreur) => {
                            p.modifier_creer_rejoindre(|e| e.erreur = erreur.to_string());
                            p.afficher_creer_rejoindre(&f);
                        }
                    }
                });
            });
    }

    pub fn recadrer_icone_de_serveur(&self, f: &Principale, image: RgbaImage) {
        if self.verrou().creation.creer_rejoindre.is_none() {
            return;
        }
        self.ouvrir_recadrage_icone(f, image);
    }

    pub fn icone_recadree(
        &self,
        f: &Principale,
        icone: Option<(&'static str, Vec<u8>, RgbaImage)>,
    ) {
        if self.verrou().creation.creer_rejoindre.is_none() {
            self.icone_des_reglages_recadree(f, icone);
            return;
        }
        self.modifier_creer_rejoindre(|e| match icone {
            Some((type_mime, octets, apercu)) => {
                e.icone = Some(IconeChoisie {
                    type_mime,
                    octets,
                    apercu,
                });
                e.erreur.clear();
            }
            None => e.erreur = ERREUR_ICONE.to_string(),
        });
        self.afficher_creer_rejoindre(f);
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn retirer_icone_de_serveur(&self, f: &Principale) {
        self.modifier_creer_rejoindre(|e| e.icone = None);
        self.afficher_creer_rejoindre(f);
    }

    pub fn creer_serveur(&self, f: &Principale) {
        let demande = self
            .modifier_creer_rejoindre(|e| {
                let nom = e.nom.trim().to_string();
                if nom.is_empty() || e.en_cours {
                    return None;
                }
                e.en_cours = true;
                e.erreur.clear();
                Some((nom, e.icone.clone()))
            })
            .flatten();
        let Some((nom, icone)) = demande else {
            return;
        };
        self.afficher_creer_rejoindre(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont
                .store
                .creer_serveur(&nom, icone.map(|i| (i.type_mime, i.octets)))
                .await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| match resultat {
                Ok((id, icone_posee)) => {
                    if p.verrou().creation.creer_rejoindre.is_none() {
                        return;
                    }
                    p.ouvrir_serveur(&f, &id);
                    if icone_posee {
                        p.fermer_creer_rejoindre(&f);
                    } else {
                        p.modifier_creer_rejoindre(|e| {
                            *e = EtatCreerRejoindre {
                                erreur: ICONE_NON_ENVOYEE.to_string(),
                                ..EtatCreerRejoindre::default()
                            }
                        });
                        f.global::<DonneesCreerRejoindre>().set_nom("".into());
                        p.afficher_creer_rejoindre(&f);
                    }
                }
                Err(erreur) => {
                    p.modifier_creer_rejoindre(|e| {
                        e.en_cours = false;
                        e.erreur = message_d_erreur(&erreur);
                    });
                    p.afficher_creer_rejoindre(&f);
                }
            });
        });
    }

    pub fn rejoindre_serveur(&self, f: &Principale) {
        let code = self
            .modifier_creer_rejoindre(|e| {
                let code = code_saisi(&e.code)?;
                if e.en_cours {
                    return None;
                }
                e.en_cours = true;
                e.erreur.clear();
                Some(code)
            })
            .flatten();
        let Some(code) = code else {
            return;
        };
        self.afficher_creer_rejoindre(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont.store.rejoindre_serveur(&code).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| match resultat {
                Ok(serveur) => {
                    if p.verrou().creation.creer_rejoindre.is_none() {
                        return;
                    }
                    p.fermer_creer_rejoindre(&f);
                    if let Some(id) = serveur.id.as_deref() {
                        p.ouvrir_serveur(&f, id);
                    }
                }
                Err(erreur) => {
                    p.modifier_creer_rejoindre(|e| {
                        e.en_cours = false;
                        e.erreur = erreur_de_rejoindre(&erreur);
                    });
                    p.afficher_creer_rejoindre(&f);
                }
            });
        });
    }

    fn afficher_creer_rejoindre(&self, f: &Principale) {
        let Some(e) = self.creer_rejoindre() else {
            return;
        };
        let g = f.global::<DonneesCreerRejoindre>();
        g.set_initiales(initiales(e.nom.trim()).as_str().into());
        match &e.icone {
            Some(icone) => {
                g.set_icone(image_slint(&icone.apercu));
                g.set_a_icone(true);
            }
            None => {
                g.set_icone(Image::default());
                g.set_a_icone(false);
            }
        }
        g.set_en_cours(e.en_cours);
        g.set_erreur(e.erreur.as_str().into());
    }

    pub fn demander_quitter_serveur(&self, f: &Principale, serveur: &str) {
        let Some(nom) = self.store.lire(|etat| {
            peut_quitter(etat, serveur)
                .then(|| etat.serveur(serveur).and_then(|s| s.name.clone()))
                .flatten()
        }) else {
            return;
        };
        let pont = self.clone();
        let serveur = serveur.to_string();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Quitter le serveur".into(),
                message: format!("Quitter « {nom} » ?"),
                action: "Quitter le serveur".into(),
                destructif: true,
                ..Demande::default()
            },
            move |_, ok| {
                if ok {
                    pont.quitter_serveur(serveur);
                }
            },
        );
    }

    fn quitter_serveur(&self, serveur: String) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            if let Err(erreur) = pont.store.quitter_serveur(&serveur).await {
                let texte = format!(
                    "Impossible de quitter le serveur : {}",
                    message_d_erreur(&erreur)
                );
                let saisie = pont.messagerie.saisie().clone();
                let ecrire = pont.messagerie.peut_ecrire();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    if ecrire {
                        saisie.signaler_erreur(&f, &texte);
                    }
                });
            }
        });
    }

    pub fn creer_salon_ici(&self, f: &Principale, genre: &str) {
        let Some(serveur) = serveur_courant(&self.route()).map(str::to_string) else {
            return;
        };
        self.ouvrir_creation_salon(f, &serveur, genre_depuis_texte(genre), None);
    }

    pub fn ouvrir_creation_salon(
        &self,
        f: &Principale,
        serveur: &str,
        genre: GenreSalon,
        apres: Option<String>,
    ) {
        if self.verrou().creation.salon.is_some() {
            return;
        }
        let Some((nom, cinema, roles, membres)) = self.store.lire(|etat| {
            if !peut_creer_salon(etat, serveur) {
                return None;
            }
            let detail = etat.details.get(serveur)?;
            let nom = etat
                .serveur(serveur)
                .and_then(|s| s.name.clone())
                .unwrap_or_default();
            let roles: Vec<(String, String)> = detail
                .roles
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|r| r.is_default != Some(true))
                .map(|r| {
                    (
                        r.id.clone().unwrap_or_default(),
                        r.name.clone().unwrap_or_default(),
                    )
                })
                .collect();
            let membres: Vec<(String, String)> = etat
                .membres(serveur)
                .iter()
                .filter(|m| m.id.as_deref() != Some(etat.moi()))
                .map(|m| {
                    let surnom = m.nickname.clone().unwrap_or_default();
                    (
                        m.id.clone().unwrap_or_default(),
                        if surnom.is_empty() {
                            m.pseudo.clone().unwrap_or_default()
                        } else {
                            surnom
                        },
                    )
                })
                .collect();
            Some((nom, cinema_actif(etat), roles, membres))
        }) else {
            return;
        };
        self.verrou().creation.salon = Some(EtatCreationSalon {
            serveur: serveur.to_string(),
            nouveau: NouveauSalon {
                genre: Some(genre_de_depart(genre, cinema)),
                apres,
                ..NouveauSalon::default()
            },
            genres: genres_proposes(cinema),
            roles,
            membres,
            touche: false,
            erreur_serveur: String::new(),
            en_cours: false,
        });
        let g = f.global::<DonneesCreationSalon>();
        g.set_serveur(nom.as_str().into());
        g.set_nom("".into());
        g.set_sujet("".into());
        g.set_avances(false);
        let modes: Vec<SharedString> = MODES_LENTS.iter().map(|(l, _)| (*l).into()).collect();
        g.set_modes_lents(ModelRc::new(VecModel::from(modes)));
        let limites: Vec<SharedString> =
            LIMITES.iter().map(|l| libelle_limite(*l).into()).collect();
        g.set_limites(ModelRc::new(VecModel::from(limites)));
        self.afficher_creation_salon(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_creation_salon(&self, f: &Principale) {
        if self.verrou().creation.salon.take().is_none() {
            return;
        }
        f.global::<DonneesCreationSalon>().set_ouvert(false);
        self.rendre_le_focus(f);
    }

    pub fn choisir_genre_de_salon(&self, f: &Principale, index: i32) {
        self.modifier_creation_salon(|e| {
            if let Some(g) = usize::try_from(index).ok().and_then(|i| e.genres.get(i)) {
                e.nouveau.genre = Some(g.genre);
                e.erreur_serveur.clear();
            }
        });
        self.afficher_creation_salon(f);
    }

    pub fn nom_de_salon_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_creation_salon(|e| {
            e.nouveau.nom = texte.to_string();
            e.touche = true;
            e.erreur_serveur.clear();
        });
        self.afficher_creation_salon(f);
    }

    pub fn basculer_salon_prive(&self, f: &Principale, prive: bool) {
        self.modifier_creation_salon(|e| e.nouveau.prive = prive);
        self.afficher_creation_salon(f);
    }

    pub fn basculer_role_du_salon(&self, f: &Principale, index: i32) {
        self.modifier_creation_salon(|e| {
            if let Some((id, _)) = usize::try_from(index).ok().and_then(|i| e.roles.get(i)) {
                basculer(&mut e.nouveau.roles, id);
            }
        });
        self.afficher_creation_salon(f);
    }

    pub fn basculer_membre_du_salon(&self, f: &Principale, index: i32) {
        self.modifier_creation_salon(|e| {
            if let Some((id, _)) = usize::try_from(index).ok().and_then(|i| e.membres.get(i)) {
                basculer(&mut e.nouveau.membres, id);
            }
        });
        self.afficher_creation_salon(f);
    }

    pub fn choisir_mode_lent(&self, f: &Principale, index: i32) {
        self.modifier_creation_salon(|e| {
            if let Some((_, v)) = usize::try_from(index).ok().and_then(|i| MODES_LENTS.get(i)) {
                e.nouveau.mode_lent = *v;
            }
        });
        self.afficher_creation_salon(f);
    }

    pub fn choisir_limite(&self, f: &Principale, index: i32) {
        self.modifier_creation_salon(|e| {
            if let Some(v) = usize::try_from(index).ok().and_then(|i| LIMITES.get(i)) {
                e.nouveau.limite = *v;
            }
        });
        self.afficher_creation_salon(f);
    }

    pub fn valider_creation_salon(&self, f: &Principale) {
        let sujet = f.global::<DonneesCreationSalon>().get_sujet().to_string();
        let demande = self.store.lire(|etat| {
            self.modifier_creation_salon(|e| {
                e.touche = true;
                e.nouveau.sujet = sujet;
                if e.en_cours || !e.valide(etat.salons(&e.serveur)) {
                    return None;
                }
                e.en_cours = true;
                Some((e.serveur.clone(), e.nouveau.clone()))
            })
            .flatten()
        });
        self.afficher_creation_salon(f);
        let Some((serveur, nouveau)) = demande else {
            return;
        };
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont.store.creer_salon(&serveur, &nouveau).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| match resultat {
                Ok(id) => {
                    if p.verrou().creation.salon.is_none() {
                        return;
                    }
                    p.naviguer(
                        &f,
                        Route::Salon {
                            serveur,
                            salon: Some(id),
                            message: None,
                        },
                    );
                    p.fermer_creation_salon(&f);
                }
                Err(erreur) => {
                    p.modifier_creation_salon(|e| {
                        e.en_cours = false;
                        e.erreur_serveur = message_d_erreur(&erreur);
                    });
                    p.afficher_creation_salon(&f);
                }
            });
        });
    }

    fn afficher_creation_salon(&self, f: &Principale) {
        let Some(e) = self.creation_salon() else {
            return;
        };
        let (erreur, valide) = self.store.lire(|etat| {
            let salons = etat.salons(&e.serveur);
            (e.erreur(salons), e.valide(salons))
        });
        let genre = e.nouveau.genre();
        let g = f.global::<DonneesCreationSalon>();
        let genres: Vec<GenreSalonPropose> = e
            .genres
            .iter()
            .map(|p| GenreSalonPropose {
                id: texte_du_genre(p.genre).into(),
                libelle: p.libelle.into(),
                indice: p.indice.into(),
                icone: p.icone.into(),
            })
            .collect();
        g.set_genres(ModelRc::new(VecModel::from(genres)));
        g.set_genre(
            e.genres
                .iter()
                .position(|p| p.genre == genre)
                .map_or(0, |i| i as i32),
        );
        g.set_icone(
            e.genres
                .iter()
                .find(|p| p.genre == genre)
                .map_or("hash", |p| p.icone)
                .into(),
        );
        let montrer = e.touche || !e.erreur_serveur.is_empty();
        g.set_erreur(if montrer { erreur.as_str() } else { "" }.into());
        g.set_en_faute(montrer && !erreur.is_empty());
        g.set_prive(e.nouveau.prive);
        let choix = |liste: &[(String, String)], choisis: &[String]| -> Vec<ChoixAcces> {
            liste
                .iter()
                .map(|(id, nom)| ChoixAcces {
                    id: id.as_str().into(),
                    nom: nom.as_str().into(),
                    choisi: choisis.contains(id),
                })
                .collect()
        };
        g.set_roles(ModelRc::new(VecModel::from(choix(
            &e.roles,
            &e.nouveau.roles,
        ))));
        g.set_membres(ModelRc::new(VecModel::from(choix(
            &e.membres,
            &e.nouveau.membres,
        ))));
        g.set_texte(genre == GenreSalon::Text);
        g.set_vocal(est_vocal_ou_cinema(genre));
        g.set_mode_lent(
            MODES_LENTS
                .iter()
                .position(|(_, v)| *v == e.nouveau.mode_lent)
                .map_or(0, |i| i as i32),
        );
        g.set_limite(
            LIMITES
                .iter()
                .position(|v| *v == e.nouveau.limite)
                .map_or(0, |i| i as i32),
        );
        g.set_peut_creer(valide && !e.en_cours);
        g.set_en_cours(e.en_cours);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_genres_se_lisent_dans_les_deux_sens() {
        for genre in [
            GenreSalon::Text,
            GenreSalon::Voice,
            GenreSalon::Videos,
            GenreSalon::Music,
            GenreSalon::Cinema,
        ] {
            assert_eq!(genre_depuis_texte(texte_du_genre(genre)), genre);
        }
        assert_eq!(genre_depuis_texte("inconnu"), GenreSalon::Text);
    }
}
