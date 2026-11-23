use std::collections::HashSet;
use std::future::Future;

use estompe_client::appels::EmojiPersonnalise;
use estompe_client::creation::{message_d_erreur, nom_de_serveur, MAX_NOM_SERVEUR};
use estompe_client::http::ErreurApi;
use estompe_client::reglages_serveur::{
    auteur_d_emoji, compte_membres, detail_ban, detail_invitation, droits, emojis_du_serveur,
    lien_d_invitation, membres_trouves, nom_d_emoji_valide, nom_propose, onglet_retenu,
    onglets_visibles, peut_modifier_emoji, proprietaire_de, verifier_emoji, Droits, Onglet,
    EXPIRATIONS, MAX_EMOJIS, NOM_INVALIDE, OCTETS_EMOJI_MAX, UTILISATIONS,
};
use estompe_client::store::Changement;
use estompe_client::types::{GetServersIdBansReponseElement, ServerInvite};
use estompe_plateforme::FiltreFichiers;
use image::RgbaImage;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use super::super::profil::{lire_image, ouvrir_image};
use super::super::projection::{initiales, serveur_courant, teinte};
use super::creation::{IconeChoisie, ERREUR_ICONE};
use super::profil::{image_slint, FILTRE_IMAGES, TITRE_DIALOGUE};
use super::{couleur_slint, image_de, Pont};
use crate::confirmation::{self, Demande, Saisie};
use crate::messagerie::medias::{CleImage, Format, Source};
use crate::{
    BanReglage, DonneesCoquille, DonneesReglagesServeur, EmojiReglage, InvitationReglage,
    MembreReglage, OngletServeur, Principale,
};

pub const FILTRE_EMOJIS: FiltreFichiers = FiltreFichiers {
    nom: "Images PNG, JPEG ou GIF",
    extensions: &["png", "jpg", "jpeg", "gif"],
};
pub const LIEN_COPIE: &str = "Lien copié.";
pub const ENREGISTRE: &str = "Modifications enregistrées.";

#[derive(Clone)]
pub struct EmojiEnAttente {
    pub type_mime: &'static str,
    pub octets: Vec<u8>,
    pub apercu: RgbaImage,
    pub nom: String,
}

#[derive(Clone)]
pub struct EtatReglages {
    pub serveur: String,
    pub onglet: Onglet,
    pub nom: String,
    pub icone: Option<IconeChoisie>,
    pub icone_retiree: bool,
    pub choix_en_cours: bool,
    pub en_cours: bool,
    pub erreur: String,
    pub annonce: String,
    pub expiration: usize,
    pub utilisation: usize,
    pub invitations: Vec<ServerInvite>,
    pub emoji: Option<EmojiEnAttente>,
    pub emojis: Vec<EmojiPersonnalise>,
    pub recherche: String,
    pub bannis: Vec<GetServersIdBansReponseElement>,
    pub roles: super::roles::EtatRoles,
    images: HashSet<String>,
}

impl EtatReglages {
    fn nouveau(serveur: &str, nom: &str, onglet: Onglet) -> Self {
        Self {
            serveur: serveur.to_string(),
            onglet,
            nom: nom.to_string(),
            icone: None,
            icone_retiree: false,
            choix_en_cours: false,
            en_cours: false,
            erreur: String::new(),
            annonce: String::new(),
            expiration: 0,
            utilisation: 0,
            invitations: Vec::new(),
            emoji: None,
            emojis: Vec::new(),
            recherche: String::new(),
            bannis: Vec::new(),
            roles: super::roles::EtatRoles::default(),
            images: HashSet::new(),
        }
    }
}

pub fn touche_les_reglages(changements: &[Changement], serveur: &str) -> bool {
    changements.iter().any(|c| match c {
        Changement::Compte => true,
        Changement::Serveur { serveur: s }
        | Changement::ServeurRetire { serveur: s }
        | Changement::Roles { serveur: s }
        | Changement::Emojis { serveur: s }
        | Changement::MembreAjoute { serveur: s, .. }
        | Changement::MembreRetire { serveur: s, .. }
        | Changement::Membre { serveur: s, .. } => s == serveur,
        _ => false,
    })
}

fn modifie(e: &EtatReglages, nom_actuel: &str, d: &Droits) -> bool {
    (d.serveur && e.nom.trim() != nom_actuel) || e.icone.is_some() || e.icone_retiree
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

impl Pont {
    pub fn reglages_serveur(&self) -> Option<EtatReglages> {
        self.verrou().reglages.clone()
    }

    pub(super) fn modifier_reglages<R>(
        &self,
        modification: impl FnOnce(&mut EtatReglages) -> R,
    ) -> Option<R> {
        self.verrou().reglages.as_mut().map(modification)
    }

    pub(super) fn serveur_des_reglages(&self) -> Option<String> {
        self.verrou().reglages.as_ref().map(|e| e.serveur.clone())
    }

    pub fn ouvrir_reglages_serveur(&self, f: &Principale, serveur: &str) {
        let serveur = if serveur.is_empty() {
            match serveur_courant(&self.route()) {
                Some(s) => s.to_string(),
                None => return,
            }
        } else {
            serveur.to_string()
        };
        let Some(nom) = self.store.lire(|etat| {
            etat.details
                .contains_key(&serveur)
                .then(|| etat.serveur(&serveur).and_then(|s| s.name.clone()))
                .flatten()
        }) else {
            return;
        };
        f.global::<crate::DonneesParametres>().set_ouverte(false);
        self.verrou().reglages = Some(EtatReglages::nouveau(&serveur, &nom, Onglet::VueEnsemble));
        let g = f.global::<DonneesReglagesServeur>();
        g.set_recherche(SharedString::new());
        let roles = f.global::<crate::DonneesRoles>();
        roles.set_filtre(SharedString::new());
        roles.set_recherche(SharedString::new());
        g.set_nom(nom.as_str().into());
        g.set_nom_emoji(SharedString::new());
        g.set_expirations(modele(
            EXPIRATIONS
                .iter()
                .map(|(l, _)| SharedString::from(*l))
                .collect(),
        ));
        g.set_utilisations(modele(
            UTILISATIONS
                .iter()
                .map(|(l, _)| SharedString::from(*l))
                .collect(),
        ));
        self.projeter_reglages(f);
        g.set_focus_initial(true);
        g.set_ouverte(true);
        self.charger_emojis_des_reglages(f);
        if self.store.lire(|etat| droits(etat, &serveur).invitations) {
            self.charger_invitations(f);
        }
    }

    pub fn fermer_reglages_serveur(&self, f: &Principale) {
        let ouverte = self.verrou().reglages.take().is_some();
        let g = f.global::<DonneesReglagesServeur>();
        if !ouverte && !g.get_ouverte() {
            return;
        }
        g.set_ouverte(false);
        g.set_apercu(Image::default());
        g.set_apercu_emoji(Image::default());
        let c = f.global::<DonneesCoquille>();
        c.set_demande_focus(c.get_demande_focus().wrapping_add(1));
    }

    pub(super) fn oublier_reglages_serveur(&self, f: &Principale) {
        self.verrou().reglages = None;
        f.global::<DonneesReglagesServeur>().set_ouverte(false);
    }

    pub fn choisir_onglet_serveur(&self, f: &Principale, id: &str) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let visibles = self
            .store
            .lire(|etat| onglets_visibles(&droits(etat, &serveur)));
        let onglet = onglet_retenu(Onglet::depuis_id(id), &visibles);
        self.modifier_reglages(|e| {
            e.onglet = onglet;
            e.erreur.clear();
            e.annonce.clear();
        });
        self.projeter_reglages(f);
        match onglet {
            Onglet::Invitations => self.charger_invitations(f),
            Onglet::Bannis => self.charger_bannis(f),
            Onglet::Emojis => self.charger_emojis_des_reglages(f),
            _ => {}
        }
    }

    pub fn deplacer_onglet_serveur(&self, f: &Principale, pas: i32) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let visibles = self
            .store
            .lire(|etat| onglets_visibles(&droits(etat, &e.serveur)));
        let Some(i) = visibles.iter().position(|o| *o == e.onglet) else {
            return;
        };
        let n = visibles.len() as i32;
        let cible = match pas {
            p if p <= -n => 0,
            p if p >= n => n - 1,
            p => (i as i32 + p).rem_euclid(n),
        };
        self.choisir_onglet_serveur(f, visibles[cible as usize].id());
        let g = f.global::<DonneesReglagesServeur>();
        g.set_retour_focus(g.get_retour_focus().wrapping_add(1));
    }

    pub fn nom_des_reglages_modifie(&self, f: &Principale, texte: &str) {
        let nom = nom_de_serveur(texte);
        if nom != texte {
            f.global::<DonneesReglagesServeur>()
                .set_nom(nom.as_str().into());
        }
        self.modifier_reglages(|e| {
            e.nom = nom;
            e.annonce.clear();
        });
        self.projeter_reglages(f);
    }

    pub fn choisir_icone_des_reglages(&self, _f: &Principale) {
        let fichiers = self.messagerie.fichiers();
        let libre = self
            .modifier_reglages(|e| {
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
            self.modifier_reglages(|e| e.choix_en_cours = false);
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
                    p.modifier_reglages(|e| e.choix_en_cours = false);
                    match image {
                        Ok(image) => p.ouvrir_recadrage_icone(&f, image),
                        Err(erreur) => {
                            p.modifier_reglages(|e| e.erreur = erreur.to_string());
                            p.projeter_reglages(&f);
                        }
                    }
                });
            });
    }

    pub fn icone_des_reglages_recadree(
        &self,
        f: &Principale,
        icone: Option<(&'static str, Vec<u8>, RgbaImage)>,
    ) {
        self.modifier_reglages(|e| match icone {
            Some((type_mime, octets, apercu)) => {
                e.icone = Some(IconeChoisie {
                    type_mime,
                    octets,
                    apercu,
                });
                e.icone_retiree = false;
                e.erreur.clear();
                e.annonce.clear();
            }
            None => e.erreur = ERREUR_ICONE.to_string(),
        });
        self.projeter_reglages(f);
    }

    pub fn supprimer_icone_des_reglages(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let version = self.store.lire(|etat| {
            etat.serveur(&serveur)
                .and_then(|s| s.icon_version)
                .unwrap_or(0)
        });
        self.modifier_reglages(|e| {
            e.icone = None;
            e.icone_retiree = version > 0;
            e.annonce.clear();
        });
        self.projeter_reglages(f);
    }

    pub fn annuler_reglages(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let nom = self.store.lire(|etat| {
            etat.serveur(&serveur)
                .and_then(|s| s.name.clone())
                .unwrap_or_default()
        });
        f.global::<DonneesReglagesServeur>()
            .set_nom(nom.as_str().into());
        self.modifier_reglages(|e| {
            e.nom = nom;
            e.icone = None;
            e.icone_retiree = false;
            e.erreur.clear();
        });
        self.projeter_reglages(f);
    }

    pub(super) fn executer<T: Send + 'static>(
        &self,
        f: &Principale,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&Pont, &Principale, T) + Send + 'static,
    ) {
        let libre = self
            .modifier_reglages(|e| {
                if e.en_cours {
                    return false;
                }
                e.en_cours = true;
                e.erreur.clear();
                e.annonce.clear();
                true
            })
            .unwrap_or(false);
        if !libre {
            return;
        }
        self.projeter_reglages(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.modifier_reglages(|e| e.en_cours = false).is_none() {
                    return;
                }
                match resultat {
                    Ok(valeur) => suite(&p, &f, valeur),
                    Err(erreur) => {
                        p.modifier_reglages(|e| e.erreur = message_d_erreur(&erreur));
                    }
                }
                p.projeter_reglages(&f);
            });
        });
    }

    pub fn enregistrer_reglages(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let (d, actuel) = self.store.lire(|etat| {
            (
                droits(etat, &e.serveur),
                etat.serveur(&e.serveur)
                    .and_then(|s| s.name.clone())
                    .unwrap_or_default(),
            )
        });
        let nom = e.nom.trim().to_string();
        if !modifie(&e, &actuel, &d) || nom.is_empty() {
            return;
        }
        let renommer = d.serveur && nom != actuel;
        let store = self.store.clone();
        let serveur = e.serveur.clone();
        let icone = e.icone.clone();
        let retiree = e.icone_retiree;
        self.executer(
            f,
            async move {
                if renommer {
                    store.renommer_serveur(&serveur, &nom).await?;
                }
                if let Some(i) = icone {
                    store
                        .client()
                        .definir_icone_de_serveur(&serveur, i.type_mime, i.octets)
                        .await?;
                } else if retiree {
                    store.client().supprimer_icone_de_serveur(&serveur).await?;
                }
                Ok(nom)
            },
            |p, f, nom| {
                f.global::<DonneesReglagesServeur>()
                    .set_nom(nom.as_str().into());
                p.modifier_reglages(|e| {
                    e.nom = nom;
                    e.icone = None;
                    e.icone_retiree = false;
                    e.annonce = ENREGISTRE.to_string();
                });
            },
        );
    }

    pub fn quitter_depuis_reglages(&self, f: &Principale) {
        if let Some(serveur) = self.serveur_des_reglages() {
            self.demander_quitter_serveur(f, &serveur);
        }
    }

    pub fn demander_suppression_serveur(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let Some(nom) = self.store.lire(|etat| {
            (proprietaire_de(etat, &serveur).as_deref() == Some(etat.moi()))
                .then(|| etat.serveur(&serveur).and_then(|s| s.name.clone()))
                .flatten()
        }) else {
            return;
        };
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le serveur".into(),
                message: format!(
                    "Supprimer « {nom} », ses salons et tous leurs messages ? Cette action est définitive."
                ),
                action: "Supprimer le serveur".into(),
                destructif: true,
                saisie: Some(Saisie {
                    libelle: format!("Saisis « {nom} » pour confirmer la suppression"),
                    valeur: String::new(),
                    exiger: Some(nom.clone()),
                }),
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let store = pont.store.clone();
                let s = serveur.clone();
                pont.executer(
                    f,
                    async move { store.supprimer_serveur(&s).await },
                    |p, f, ()| p.fermer_reglages_serveur(f),
                );
            },
        );
    }

    fn charger_invitations(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let _ = f;
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(liste) = pont.store.client().invitations_serveur(&serveur).await else {
                return;
            };
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.poser_invitations(&f, &serveur, liste));
        });
    }

    pub fn poser_invitations(&self, f: &Principale, serveur: &str, liste: Vec<ServerInvite>) {
        let pose = self
            .modifier_reglages(|e| {
                if e.serveur != serveur {
                    return false;
                }
                e.invitations = liste;
                true
            })
            .unwrap_or(false);
        if pose {
            self.projeter_reglages(f);
        }
    }

    pub fn choisir_expiration(&self, f: &Principale, index: i32) {
        self.modifier_reglages(|e| {
            e.expiration = (index.max(0) as usize).min(EXPIRATIONS.len() - 1)
        });
        self.projeter_reglages(f);
    }

    pub fn choisir_utilisation(&self, f: &Principale, index: i32) {
        self.modifier_reglages(|e| {
            e.utilisation = (index.max(0) as usize).min(UTILISATIONS.len() - 1)
        });
        self.projeter_reglages(f);
    }

    pub fn creer_invitation(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let client = self.store.client().clone();
        let serveur = e.serveur.clone();
        let utilisations = UTILISATIONS[e.utilisation].1;
        let expiration = EXPIRATIONS[e.expiration].1;
        self.executer(
            f,
            async move {
                client
                    .creer_invitation_serveur(&serveur, utilisations, expiration)
                    .await?;
                client.invitations_serveur(&serveur).await
            },
            |p, f, liste| {
                if let Some(s) = p.serveur_des_reglages() {
                    p.poser_invitations(f, &s, liste);
                }
            },
        );
    }

    pub fn copier_invitation(&self, f: &Principale, index: i32) {
        let Some(code) = self.modifier_reglages(|e| {
            e.invitations
                .get(index.max(0) as usize)
                .and_then(|i| i.code.clone())
        }) else {
            return;
        };
        let Some(code) = code else {
            return;
        };
        crate::menu::copier_texte(f, &lien_d_invitation(self.store.client().adresse(), &code));
        self.modifier_reglages(|e| {
            e.erreur.clear();
            e.annonce = LIEN_COPIE.to_string();
        });
        self.projeter_reglages(f);
    }

    pub fn revoquer_invitation(&self, f: &Principale, index: i32) {
        let Some(Some((serveur, code))) = self.modifier_reglages(|e| {
            e.invitations
                .get(index.max(0) as usize)
                .and_then(|i| i.code.clone())
                .map(|c| (e.serveur.clone(), c))
        }) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(
            f,
            async move {
                client.revoquer_invitation_serveur(&serveur, &code).await?;
                client.invitations_serveur(&serveur).await
            },
            |p, f, liste| {
                if let Some(s) = p.serveur_des_reglages() {
                    p.poser_invitations(f, &s, liste);
                }
            },
        );
    }

    fn charger_emojis_des_reglages(&self, f: &Principale) {
        let _ = f;
        let medias = self.messagerie.medias();
        let pont = self.clone();
        self.runtime.spawn(async move {
            medias.charger_emojis().await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.projeter_reglages(&f));
        });
    }

    pub fn choisir_emoji_a_ajouter(&self, _f: &Principale) {
        let fichiers = self.messagerie.fichiers();
        let libre = self
            .modifier_reglages(|e| {
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
            self.modifier_reglages(|e| e.choix_en_cours = false);
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-emoji".into())
            .spawn(move || {
                let choix = match fichiers.choisir_fichier(TITRE_DIALOGUE, &FILTRE_EMOJIS, None) {
                    Ok(Some(chemin)) => Some(lire_emoji(&chemin)),
                    _ => None,
                };
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.modifier_reglages(|e| e.choix_en_cours = false);
                    if let Some(choix) = choix {
                        p.emoji_choisi(&f, choix);
                    }
                });
            });
    }

    pub fn emoji_choisi(&self, f: &Principale, choix: Result<EmojiEnAttente, &'static str>) {
        let nom = choix.as_ref().map(|e| e.nom.clone()).unwrap_or_default();
        self.modifier_reglages(|e| match choix {
            Ok(emoji) => {
                e.emoji = Some(emoji);
                e.erreur.clear();
                e.annonce.clear();
            }
            Err(erreur) => {
                e.emoji = None;
                e.erreur = erreur.to_string();
            }
        });
        f.global::<DonneesReglagesServeur>()
            .set_nom_emoji(nom.as_str().into());
        self.projeter_reglages(f);
    }

    pub fn nom_d_emoji_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_reglages(|e| {
            if let Some(emoji) = e.emoji.as_mut() {
                emoji.nom = texte.to_string();
            }
        });
        self.projeter_reglages(f);
    }

    pub fn annuler_emoji(&self, f: &Principale) {
        self.modifier_reglages(|e| e.emoji = None);
        f.global::<DonneesReglagesServeur>()
            .set_apercu_emoji(Image::default());
        self.projeter_reglages(f);
    }

    pub fn ajouter_emoji(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let Some(emoji) = e.emoji.filter(|x| nom_d_emoji_valide(&x.nom)) else {
            return;
        };
        let client = self.store.client().clone();
        let medias = self.messagerie.medias();
        let serveur = e.serveur.clone();
        self.executer(
            f,
            async move {
                client
                    .ajouter_emoji(&serveur, &emoji.nom, emoji.type_mime, emoji.octets)
                    .await?;
                medias.charger_emojis().await;
                Ok(())
            },
            |p, f, ()| {
                p.modifier_reglages(|e| e.emoji = None);
                f.global::<DonneesReglagesServeur>()
                    .set_apercu_emoji(Image::default());
            },
        );
    }

    fn emoji_a(&self, index: i32) -> Option<(String, EmojiPersonnalise)> {
        self.modifier_reglages(|e| {
            e.emojis
                .get(index.max(0) as usize)
                .cloned()
                .map(|x| (e.serveur.clone(), x))
        })
        .flatten()
    }

    pub fn renommer_emoji(&self, f: &Principale, index: i32) {
        let Some((_, emoji)) = self.emoji_a(index) else {
            return;
        };
        let pont = self.clone();
        confirmation::saisir(
            f,
            Demande {
                titre: format!("Renommer :{}:", emoji.name),
                message: NOM_INVALIDE.into(),
                action: "Renommer".into(),
                saisie: Some(Saisie {
                    libelle: "Nouveau nom".into(),
                    valeur: emoji.name.clone(),
                    exiger: None,
                }),
                ..Demande::default()
            },
            move |f, valeur| {
                if let Some(valeur) = valeur {
                    pont.renommer_emoji_en(f, &emoji, &valeur);
                }
            },
        );
    }

    pub fn renommer_emoji_en(&self, f: &Principale, emoji: &EmojiPersonnalise, nom: &str) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let nom = nom.trim().to_lowercase();
        if nom == emoji.name {
            return;
        }
        if !nom_d_emoji_valide(&nom) {
            self.modifier_reglages(|e| e.erreur = NOM_INVALIDE.to_string());
            self.projeter_reglages(f);
            return;
        }
        let client = self.store.client().clone();
        let medias = self.messagerie.medias();
        let id = emoji.id.clone();
        self.executer(
            f,
            async move {
                client.renommer_emoji(&serveur, &id, &nom).await?;
                medias.charger_emojis().await;
                Ok(())
            },
            |_, _, ()| {},
        );
    }

    pub fn supprimer_emoji(&self, f: &Principale, index: i32) {
        let Some((serveur, emoji)) = self.emoji_a(index) else {
            return;
        };
        let client = self.store.client().clone();
        let medias = self.messagerie.medias();
        self.executer(
            f,
            async move {
                client.supprimer_emoji(&serveur, &emoji.id).await?;
                medias.charger_emojis().await;
                Ok(())
            },
            |_, _, ()| {},
        );
    }

    pub fn chercher_membres_des_reglages(&self, f: &Principale, texte: &str) {
        self.modifier_reglages(|e| e.recherche = texte.to_string());
        self.projeter_reglages(f);
    }

    fn charger_bannis(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let _ = f;
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(liste) = pont.store.client().bannis(&serveur).await else {
                return;
            };
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.poser_bannis(&f, &serveur, liste));
        });
    }

    pub fn poser_bannis(
        &self,
        f: &Principale,
        serveur: &str,
        liste: Vec<GetServersIdBansReponseElement>,
    ) {
        let pose = self
            .modifier_reglages(|e| {
                if e.serveur != serveur {
                    return false;
                }
                e.bannis = liste;
                true
            })
            .unwrap_or(false);
        if pose {
            self.projeter_reglages(f);
        }
    }

    pub fn lever_ban(&self, f: &Principale, index: i32) {
        let Some(Some((serveur, utilisateur))) = self.modifier_reglages(|e| {
            e.bannis
                .get(index.max(0) as usize)
                .and_then(|b| b.user_id.clone())
                .map(|u| (e.serveur.clone(), u))
        }) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(
            f,
            async move {
                client.lever_ban(&serveur, &utilisateur).await?;
                client.bannis(&serveur).await
            },
            |p, f, liste| {
                if let Some(s) = p.serveur_des_reglages() {
                    p.poser_bannis(f, &s, liste);
                }
                p.modifier_reglages(|e| e.annonce = "Bannissement levé.".to_string());
            },
        );
    }

    pub fn changements_des_reglages(&self, f: &Principale, changements: &[Changement]) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        if !touche_les_reglages(changements, &serveur) {
            return;
        }
        let existe = self.store.lire(|etat| etat.details.contains_key(&serveur));
        if !existe {
            self.oublier_reglages_serveur(f);
            return;
        }
        if changements
            .iter()
            .any(|c| matches!(c, Changement::Emojis { serveur: s } if *s == serveur))
        {
            self.charger_emojis_des_reglages(f);
        }
        if changements
            .iter()
            .any(|c| matches!(c, Changement::Roles { serveur: s } if *s == serveur))
        {
            self.charger_surcharges(f);
        }
        self.projeter_reglages(f);
    }

    pub fn projeter_reglages(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let g = f.global::<DonneesReglagesServeur>();
        let medias = self.messagerie.medias();
        let decalage = self.messagerie.decalage_minutes();
        let tous = medias.emojis().unwrap_or_default();
        let projection = self.store.lire(|etat| {
            let d = droits(etat, &e.serveur);
            let serveur = etat.serveur(&e.serveur).cloned().unwrap_or_default();
            let nom = serveur.name.clone().unwrap_or_default();
            let createur = proprietaire_de(etat, &e.serveur)
                .and_then(|id| {
                    etat.details
                        .get(&e.serveur)
                        .and_then(|det| det.members.as_deref())
                        .unwrap_or_default()
                        .iter()
                        .find(|m| m.id.as_deref() == Some(id.as_str()))
                        .and_then(|m| m.pseudo.clone())
                })
                .unwrap_or_else(|| "un membre".to_string());
            let emojis: Vec<(EmojiPersonnalise, String, bool)> =
                emojis_du_serveur(&tous, &e.serveur)
                    .into_iter()
                    .map(|x| {
                        (
                            x.clone(),
                            auteur_d_emoji(etat, &e.serveur, x),
                            peut_modifier_emoji(etat, &e.serveur, x),
                        )
                    })
                    .collect();
            let (membres, total) = membres_trouves(etat, &e.serveur, &e.recherche, decalage);
            (d, serveur, nom, createur, emojis, membres, total)
        });
        let (d, serveur, nom, createur, emojis, membres, total) = projection;
        let visibles = onglets_visibles(&d);
        let onglet = onglet_retenu(Some(e.onglet), &visibles);
        if onglet != e.onglet {
            self.modifier_reglages(|x| x.onglet = onglet);
        }
        let version = serveur.icon_version.unwrap_or(0);
        let icone = image_de(&self.verrou().icones, &e.serveur, version);
        g.set_nom_serveur(nom.as_str().into());
        g.set_initiales(initiales(&nom).as_str().into());
        g.set_a_icone(icone.is_some());
        g.set_icone(icone.clone().unwrap_or_default());
        g.set_gestionnaire(d.gestionnaire());
        g.set_role(d.role().into());
        g.set_onglets(modele(
            visibles
                .iter()
                .map(|o| OngletServeur {
                    id: o.id().into(),
                    libelle: o.libelle().into(),
                    icone: o.icone().into(),
                    compte: if *o == Onglet::Emojis {
                        emojis.len().to_string().into()
                    } else {
                        SharedString::new()
                    },
                    bientot: o.bientot(),
                })
                .collect(),
        ));
        g.set_onglet(onglet.id().into());
        g.set_titre(onglet.libelle().into());
        g.set_erreur(e.erreur.as_str().into());
        g.set_annonce(e.annonce.as_str().into());
        g.set_en_cours(e.en_cours);
        g.set_peut_renommer(d.serveur);
        g.set_peut_icone(d.icone);
        g.set_compte_nom(format!("{} / {MAX_NOM_SERVEUR}", e.nom.chars().count()).into());
        let apercu = match (&e.icone, e.icone_retiree) {
            (Some(i), _) => Some(image_slint(&i.apercu)),
            (None, true) => None,
            (None, false) => icone,
        };
        g.set_a_apercu(apercu.is_some());
        g.set_apercu(apercu.unwrap_or_default());
        let nom_apercu = if e.nom.trim().is_empty() {
            nom.clone()
        } else {
            e.nom.trim().to_string()
        };
        g.set_initiales_apercu(initiales(&nom_apercu).as_str().into());
        g.set_peut_supprimer_icone(
            d.icone && (e.icone.is_some() || (version > 0 && !e.icone_retiree)),
        );
        g.set_modifie(modifie(&e, &nom, &d));
        g.set_proprietaire(d.proprietaire);
        g.set_createur(createur.as_str().into());
        g.set_expiration(e.expiration as i32);
        g.set_utilisation(e.utilisation as i32);
        let adresse = self.store.client().adresse().to_string();
        g.set_invitations(modele(
            e.invitations
                .iter()
                .map(|i| {
                    let code = i.code.clone().unwrap_or_default();
                    InvitationReglage {
                        lien: lien_d_invitation(&adresse, &code).into(),
                        code: code.into(),
                        detail: detail_invitation(i, decalage).into(),
                    }
                })
                .collect(),
        ));
        let mut a_charger = Vec::new();
        let lignes: Vec<EmojiReglage> = emojis
            .iter()
            .map(|(x, auteur, modifiable)| {
                let cle = CleImage {
                    source: Source::Emoji(x.id.clone()),
                    format: Format::Emoji,
                };
                let image = medias
                    .image(&cle)
                    .map(|d| Image::from_rgba8(d.premiere().clone()));
                if image.is_none() && !e.images.contains(&x.id) && !medias.en_echec(&cle) {
                    a_charger.push((x.id.clone(), cle));
                }
                EmojiReglage {
                    id: x.id.as_str().into(),
                    nom: x.name.as_str().into(),
                    auteur: auteur.as_str().into(),
                    a_image: image.is_some(),
                    image: image.unwrap_or_default(),
                    modifiable: *modifiable,
                }
            })
            .collect();
        g.set_compte_emojis(format!("{} / {MAX_EMOJIS} emojis", lignes.len()).into());
        g.set_peut_ajouter_emoji(lignes.len() < MAX_EMOJIS);
        g.set_emojis(modele(lignes));
        match &e.emoji {
            Some(x) => {
                g.set_emoji_en_attente(true);
                g.set_apercu_emoji(image_slint(&x.apercu));
                g.set_nom_emoji_valide(nom_d_emoji_valide(&x.nom) && !e.en_cours);
            }
            None => {
                g.set_emoji_en_attente(false);
                g.set_nom_emoji_valide(false);
            }
        }
        g.set_compte_membres(compte_membres(membres.len(), total, &e.recherche).into());
        g.set_membres(modele(
            membres
                .iter()
                .map(|m| {
                    let couleur = couleur_slint(m.couleur.as_deref());
                    MembreReglage {
                        id: m.id.as_str().into(),
                        nom: m.nom.as_str().into(),
                        pseudo: m.pseudo.as_str().into(),
                        initiales: initiales(&m.nom).as_str().into(),
                        teinte: teinte(&m.id),
                        role: m.role.as_str().into(),
                        a_couleur: couleur.is_some(),
                        couleur: couleur.unwrap_or_default(),
                        createur: m.createur,
                        arrivee: m.arrivee.as_str().into(),
                    }
                })
                .collect(),
        ));
        g.set_bannis(modele(
            e.bannis
                .iter()
                .map(|b| {
                    let pseudo = b.pseudo.clone().unwrap_or_default();
                    let id = b.user_id.clone().unwrap_or_default();
                    BanReglage {
                        initiales: initiales(&pseudo).as_str().into(),
                        teinte: teinte(&id),
                        id: id.into(),
                        pseudo: pseudo.into(),
                        detail: detail_ban(b, decalage).into(),
                        raison: b.reason.clone().unwrap_or_default().into(),
                    }
                })
                .collect(),
        ));
        self.modifier_reglages(|x| {
            x.emojis = emojis.into_iter().map(|(e, _, _)| e).collect();
            for (id, _) in &a_charger {
                x.images.insert(id.clone());
            }
        });
        self.projeter_roles(f);
        for (_, cle) in a_charger {
            let medias = medias.clone();
            let pont = self.clone();
            self.runtime.spawn(async move {
                if medias.charger_image(cle).await.is_some() {
                    let p = pont.clone();
                    let _ = pont
                        .fenetre
                        .upgrade_in_event_loop(move |f| p.projeter_reglages(&f));
                }
            });
        }
    }
}

pub fn lire_emoji(chemin: &std::path::Path) -> Result<EmojiEnAttente, &'static str> {
    let taille = std::fs::metadata(chemin)
        .map(|m| m.len())
        .map_err(|_| estompe_client::reglages_serveur::ILLISIBLE)?;
    if taille > (OCTETS_EMOJI_MAX as u64) * 4 {
        return Err(estompe_client::reglages_serveur::TROP_LOURDE);
    }
    let octets = std::fs::read(chemin).map_err(|_| estompe_client::reglages_serveur::ILLISIBLE)?;
    emoji_depuis(&octets, &chemin.to_string_lossy())
}

pub fn emoji_depuis(octets: &[u8], nom_fichier: &str) -> Result<EmojiEnAttente, &'static str> {
    let type_mime = verifier_emoji(octets)?;
    let apercu = ouvrir_image(octets).ok_or(estompe_client::reglages_serveur::ILLISIBLE)?;
    Ok(EmojiEnAttente {
        type_mime,
        octets: octets.to_vec(),
        apercu,
        nom: nom_propose(nom_fichier),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_page_suit_son_serveur() {
        let serveur = |s: &str| Changement::Serveur { serveur: s.into() };
        assert!(touche_les_reglages(&[serveur("s1")], "s1"));
        assert!(!touche_les_reglages(&[serveur("s2")], "s1"));
        assert!(touche_les_reglages(
            &[Changement::Emojis {
                serveur: "s1".into()
            }],
            "s1"
        ));
        assert!(touche_les_reglages(
            &[Changement::MembreRetire {
                serveur: "s1".into(),
                utilisateur: "u2".into()
            }],
            "s1"
        ));
        assert!(!touche_les_reglages(&[Changement::MessagesPrives], "s1"));
    }

    #[test]
    fn un_emoji_trop_grand_n_est_pas_mis_en_attente() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(300, 20, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(
            emoji_depuis(&png, "grand.png").err(),
            Some(estompe_client::reglages_serveur::TROP_GRANDE)
        );
        let mut petit = Vec::new();
        image::RgbaImage::from_pixel(64, 64, image::Rgba([1, 2, 3, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut petit),
                image::ImageFormat::Png,
            )
            .unwrap();
        let e = emoji_depuis(&petit, "/tmp/Mon Chat.png").unwrap();
        assert_eq!((e.type_mime, e.nom.as_str()), ("image/png", "mon_chat"));
        assert_eq!(e.apercu.dimensions(), (64, 64));
    }
}
