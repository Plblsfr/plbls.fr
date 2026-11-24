use estompe_client::route::{adresse_web, lien_message_de_salon, lien_message_en_prive};
use estompe_client::store::CleConversation;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::super::actions::{
    entrees_du_message, COPIER_LIEN, COPIER_TEXTE, DESEPINGLER, EPINGLER, MODIFIER, NON_LU,
    REACTIONS_RAPIDES, REAGIR, REPONDRE, SIGNALER, SUPPRIMER,
};
use super::super::emoji;
use super::super::lignes::DonneesMessage;
use super::super::selecteur::Cible as CibleSelecteur;
use super::PontMessagerie;
use crate::confirmation::{self, Demande};
use crate::menu::{self, Ancre};
use crate::{DonneesMessagerie, DonneesSignalements, Principale, ReactionMessage};

pub const TITRE_MENU_MESSAGE: &str = "Actions sur le message";

impl PontMessagerie {
    pub fn installer_reactions_rapides(&self, f: &Principale) {
        let reactions: Vec<ReactionMessage> = REACTIONS_RAPIDES
            .iter()
            .map(|e| ReactionMessage {
                emoji: (*e).into(),
                image: emoji::image(e).unwrap_or_default(),
                ..ReactionMessage::default()
            })
            .collect();
        f.global::<DonneesMessagerie>()
            .set_reactions_rapides(ModelRc::new(VecModel::from(reactions)));
    }

    fn message_affiche(&self, id: &str) -> Option<DonneesMessage> {
        let i = self.verrou();
        let n = *i.index.get(id)?;
        i.lignes.get(n)?.message.clone()
    }

    pub fn ouvrir_menu_message(&self, f: &Principale, id: &str, ancre: Ancre) -> bool {
        let Some(m) = self.message_affiche(id) else {
            return false;
        };
        let texte = !self.rendus_de(&m).texte_brut.is_empty();
        let entrees = entrees_du_message(m.droits, m.epingle, texte);
        let pont = self.clone();
        let id = id.to_string();
        menu::ouvrir(f, TITRE_MENU_MESSAGE, ancre, entrees, move |f, action| {
            pont.action_message(f, &id, action)
        })
    }

    pub fn lien_du_message(&self, id: &str) -> Option<String> {
        let (cle, serveur) = {
            let i = self.verrou();
            (i.cle.clone()?, i.serveur.clone())
        };
        let lien = match &cle {
            CleConversation::Salon(salon) => lien_message_de_salon(serveur.as_deref()?, salon, id),
            CleConversation::Prive(prive) => lien_message_en_prive(prive, id),
        };
        Some(adresse_web(self.store.client().adresse(), &lien))
    }

    pub fn action_message(&self, f: &Principale, id: &str, action: &str) -> bool {
        match action {
            REPONDRE => {
                self.repondre_a(f, id);
                true
            }
            MODIFIER => {
                self.modifier_message(f, id);
                true
            }
            EPINGLER | DESEPINGLER => {
                self.epingler(id, action == EPINGLER);
                false
            }
            REAGIR => {
                if self.selecteur().ouvert() {
                    self.fermer_selecteur(f);
                }
                self.ouvrir_selecteur(f, CibleSelecteur::Reaction(id.to_string()));
                true
            }
            COPIER_TEXTE => {
                if let Some(m) = self.message_affiche(id) {
                    menu::copier_texte(f, &self.rendus_de(&m).texte_brut);
                }
                false
            }
            COPIER_LIEN => {
                if let Some(lien) = self.lien_du_message(id) {
                    menu::copier_texte(f, &lien);
                }
                false
            }
            NON_LU => {
                self.marquer_non_lu(f, id);
                false
            }
            SIGNALER => {
                if let Some(m) = self.message_affiche(id) {
                    let extrait = self.rendus_de(&m).texte_brut.clone();
                    f.global::<DonneesSignalements>().invoke_signaler_message(
                        id.into(),
                        m.auteur.as_str().into(),
                        extrait.as_str().into(),
                    );
                }
                true
            }
            SUPPRIMER => {
                let pont = self.clone();
                let id = id.to_string();
                confirmation::confirmer(
                    f,
                    Demande {
                        titre: "Supprimer ce message ?".into(),
                        message: "Cette action est définitive.".into(),
                        action: "Supprimer".into(),
                        destructif: true,
                        ..Demande::default()
                    },
                    move |_, ok| {
                        if ok {
                            pont.supprimer_message(&id);
                        }
                    },
                );
                true
            }
            _ => false,
        }
    }

    fn epingler(&self, id: &str, epingle: bool) {
        let store = self.store.clone();
        let saisie = self.saisie.clone();
        let faible = self.fenetre.clone();
        let id = id.to_string();
        self.runtime.spawn(async move {
            if let Err(erreur) = store.epingler(&id, epingle).await {
                let texte = format!("Épinglage impossible : {erreur}");
                let _ = faible.upgrade_in_event_loop(move |f| saisie.signaler_erreur(&f, &texte));
            }
        });
    }

    pub fn reagir(&self, id: &str, emoji: &str) {
        let store = self.store.clone();
        let saisie = self.saisie.clone();
        let faible = self.fenetre.clone();
        let id = id.to_string();
        let emoji = emoji.to_string();
        self.runtime.spawn(async move {
            if let Err(erreur) = store.basculer_reaction(&id, &emoji).await {
                let texte = format!("Réaction impossible : {erreur}");
                let _ = faible.upgrade_in_event_loop(move |f| saisie.signaler_erreur(&f, &texte));
            }
        });
    }

    pub fn marquer_non_lu(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if self.store.marquer_non_lu(&cle, id) == 0 {
            return;
        }
        self.verrou().nouveau_depuis = Some(id.to_string());
        self.reprojeter(f);
    }
}
