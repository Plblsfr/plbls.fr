use estompe_client::http::ErreurApi;
use estompe_client::nouveau_prive::{
    message_d_erreur, personnes_connues, proposer, PersonneConnue,
};
use estompe_client::route::Route;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::super::projection::{initiales, teinte};
use super::super::recherche::voisine;
use super::{presence_slint, Pont};
use crate::{DonneesCoquille, DonneesNouveauPrive, PersonneProposee, Principale};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatNouveauPrive {
    pub connues: Vec<PersonneConnue>,
    pub saisie: String,
    pub proposes: Vec<PersonneConnue>,
    pub active: Option<usize>,
    pub en_cours: bool,
    pub erreur: String,
}

impl Pont {
    pub fn nouveau_prive(&self) -> Option<EtatNouveauPrive> {
        self.verrou().nouveau_prive.clone()
    }

    fn modifier_nouveau_prive<R>(
        &self,
        modification: impl FnOnce(&mut EtatNouveauPrive) -> R,
    ) -> Option<R> {
        self.verrou().nouveau_prive.as_mut().map(modification)
    }

    pub fn ouvrir_nouveau_prive(&self, f: &Principale) {
        if self.verrou().nouveau_prive.is_some() {
            return;
        }
        let connues = self.store.lire(personnes_connues);
        self.verrou().nouveau_prive = Some(EtatNouveauPrive {
            connues,
            ..EtatNouveauPrive::default()
        });
        let g = f.global::<DonneesNouveauPrive>();
        g.set_saisie("".into());
        self.afficher_nouveau_prive(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_nouveau_prive(&self, f: &Principale) {
        if self.verrou().nouveau_prive.take().is_none() {
            return;
        }
        f.global::<DonneesNouveauPrive>().set_ouvert(false);
        if self.messagerie.peut_ecrire() {
            self.messagerie.saisie().demander_focus(f);
        } else {
            let g = f.global::<DonneesCoquille>();
            g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
        }
    }

    pub fn nouveau_prive_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_nouveau_prive(|e| {
            e.saisie = texte.to_string();
            e.proposes = proposer(&e.connues, texte);
            e.active = None;
        });
        self.afficher_nouveau_prive(f);
    }

    pub fn naviguer_nouveau_prive(&self, f: &Principale, pas: i32) {
        self.modifier_nouveau_prive(|e| e.active = voisine(e.active, pas, e.proposes.len()));
        self.afficher_nouveau_prive(f);
    }

    pub fn fermer_liste_nouveau_prive(&self, f: &Principale) {
        self.modifier_nouveau_prive(|e| {
            e.proposes.clear();
            e.active = None;
        });
        self.afficher_nouveau_prive(f);
    }

    pub fn choisir_personne(&self, f: &Principale, index: i32) {
        let pseudo = self
            .modifier_nouveau_prive(|e| {
                let pseudo = e.proposes.get(usize::try_from(index).ok()?)?.pseudo.clone();
                e.saisie = pseudo.clone();
                e.proposes.clear();
                e.active = None;
                Some(pseudo)
            })
            .flatten();
        let Some(pseudo) = pseudo else {
            return;
        };
        let g = f.global::<DonneesNouveauPrive>();
        g.set_saisie(pseudo.as_str().into());
        g.set_curseur(pseudo.len() as i32);
        g.set_demande_curseur(g.get_demande_curseur().wrapping_add(1));
        self.afficher_nouveau_prive(f);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn valider_nouveau_prive(&self, f: &Principale) {
        if let Some(i) = self.modifier_nouveau_prive(|e| e.active).flatten() {
            self.choisir_personne(f, i as i32);
            return;
        }
        let pseudo = self
            .modifier_nouveau_prive(|e| {
                let pseudo = e.saisie.trim().to_string();
                if pseudo.is_empty() || e.en_cours {
                    return None;
                }
                e.en_cours = true;
                e.erreur.clear();
                e.proposes.clear();
                Some(pseudo)
            })
            .flatten();
        let Some(pseudo) = pseudo else {
            return;
        };
        self.afficher_nouveau_prive(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont.store.ouvrir_message_prive(&pseudo).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.nouveau_prive_recu(&f, resultat));
        });
    }

    pub fn nouveau_prive_recu(&self, f: &Principale, resultat: Result<String, ErreurApi>) {
        match resultat {
            Ok(id) => {
                if self.verrou().nouveau_prive.is_none() {
                    return;
                }
                self.naviguer(f, Route::MessagePrive { id, message: None });
                self.fermer_nouveau_prive(f);
            }
            Err(erreur) => {
                self.modifier_nouveau_prive(|e| {
                    e.en_cours = false;
                    e.erreur = message_d_erreur(&erreur);
                });
                self.afficher_nouveau_prive(f);
            }
        }
    }

    fn afficher_nouveau_prive(&self, f: &Principale) {
        let Some(e) = self.nouveau_prive() else {
            return;
        };
        let proposes: Vec<PersonneProposee> = self.store.lire(|etat| {
            e.proposes
                .iter()
                .map(|p| PersonneProposee {
                    pseudo: p.pseudo.as_str().into(),
                    ou: p.ou.as_str().into(),
                    initiales: initiales(&p.pseudo).into(),
                    teinte: teinte(if p.id.is_empty() { &p.pseudo } else { &p.id }),
                    presence: presence_slint(etat.presence_de(&p.id)),
                })
                .collect()
        });
        let g = f.global::<DonneesNouveauPrive>();
        g.set_proposes(ModelRc::new(VecModel::from(proposes)));
        g.set_active(e.active.map_or(-1, |i| i as i32));
        g.set_peut_valider(!e.saisie.trim().is_empty() && !e.en_cours);
        g.set_en_cours(e.en_cours);
        g.set_erreur(e.erreur.as_str().into());
    }
}
