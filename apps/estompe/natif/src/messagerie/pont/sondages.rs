use estompe_client::sondage::{
    valider, Cloture, Formulaire, CLOTURES, FORMAT_DATE, NOMBRE_DE_CHOIX_MAX,
};
use estompe_client::store::messages::Brouillon;
use estompe_client::store::CleConversation;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::super::lignes::Votants;
use super::super::saisie::Action;
use super::PontMessagerie;
use crate::confirmation::{self, Demande};
use crate::{DonneesCreationSondage, Principale};

pub struct CreationEnCours {
    cle: CleConversation,
    formulaire: Formulaire,
    tente: bool,
}

fn modele_de(choix: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        choix
            .iter()
            .map(|c| SharedString::from(c.as_str()))
            .collect::<Vec<_>>(),
    ))
}

impl PontMessagerie {
    pub fn sondage_possible(&self) -> bool {
        let Some(cle) = self.conversation() else {
            return false;
        };
        self.raison_lecture_seule().is_none()
            && self
                .store
                .lire(|etat| etat.peut_dans(Some(&cle), "create_polls"))
    }

    pub fn ouvrir_creation_sondage(&self, f: &Principale) {
        let Some(cle) = self.conversation().filter(|_| self.sondage_possible()) else {
            return;
        };
        let formulaire = Formulaire::default();
        let g = f.global::<DonneesCreationSondage>();
        g.set_question(SharedString::new());
        g.set_choix(modele_de(&formulaire.choix));
        g.set_choix_max(NOMBRE_DE_CHOIX_MAX as i32);
        g.set_multiple(false);
        g.set_clotures(ModelRc::new(VecModel::from(
            CLOTURES
                .iter()
                .map(|(libelle, _)| SharedString::from(*libelle))
                .collect::<Vec<_>>(),
        )));
        g.set_date(SharedString::new());
        g.set_format_date(FORMAT_DATE.into());
        g.set_erreur(SharedString::new());
        self.verrou().creation_sondage = Some(CreationEnCours {
            cle,
            formulaire,
            tente: false,
        });
        self.afficher_creation(f);
        g.set_ouvert(true);
    }

    fn afficher_creation(&self, f: &Principale) {
        let g = f.global::<DonneesCreationSondage>();
        let (cloture, erreur) = {
            let i = self.verrou();
            let Some(c) = i.creation_sondage.as_ref() else {
                return;
            };
            let erreur = c
                .tente
                .then(|| self.valider_creation(&c.formulaire).err())
                .flatten()
                .unwrap_or_default();
            (c.formulaire.cloture, erreur)
        };
        let index = CLOTURES
            .iter()
            .position(|(_, c)| *c == cloture)
            .unwrap_or(0);
        g.set_cloture(index as i32);
        g.set_personnalisee(cloture == Cloture::Personnalisee);
        g.set_erreur(erreur.into());
    }

    fn valider_creation(
        &self,
        formulaire: &Formulaire,
    ) -> Result<(String, estompe_client::appels::NouveauSondage), String> {
        let maintenant = self.maintenant();
        let decalage = self
            .fuseau
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()(maintenant);
        valider(formulaire, maintenant, decalage).map_err(|e| e.libelle())
    }

    fn modifier_creation(&self, f: &Principale, modification: impl FnOnce(&mut Formulaire)) {
        let g = f.global::<DonneesCreationSondage>();
        let (question, date, multiple) = (g.get_question(), g.get_date(), g.get_multiple());
        if let Some(c) = self.verrou().creation_sondage.as_mut() {
            c.formulaire.question = question.to_string();
            c.formulaire.date = date.to_string();
            c.formulaire.multiple = multiple;
            modification(&mut c.formulaire);
        }
        self.afficher_creation(f);
    }

    pub fn creation_modifiee(&self, f: &Principale) {
        self.modifier_creation(f, |_| {});
    }

    pub fn choix_modifie(&self, f: &Principale, index: i32, texte: &str) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.modifier_creation(f, |formulaire| {
            if let Some(choix) = formulaire.choix.get_mut(index) {
                *choix = texte.to_string();
            }
        });
        let modele = f.global::<DonneesCreationSondage>().get_choix();
        if modele.row_data(index).is_some_and(|c| c != texte) {
            modele.set_row_data(index, texte.into());
        }
    }

    pub fn ajouter_choix(&self, f: &Principale) {
        let mut ajoute = false;
        self.modifier_creation(f, |formulaire| {
            if formulaire.choix.len() < NOMBRE_DE_CHOIX_MAX {
                formulaire.choix.push(String::new());
                ajoute = true;
            }
        });
        if ajoute {
            let modele = f.global::<DonneesCreationSondage>().get_choix();
            if let Some(v) = modele.as_any().downcast_ref::<VecModel<SharedString>>() {
                v.push(SharedString::new());
            }
        }
    }

    pub fn retirer_choix(&self, f: &Principale, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut retire = false;
        self.modifier_creation(f, |formulaire| {
            if formulaire.choix.len() > 2 && index < formulaire.choix.len() {
                formulaire.choix.remove(index);
                retire = true;
            }
        });
        if retire {
            let modele = f.global::<DonneesCreationSondage>().get_choix();
            if let Some(v) = modele.as_any().downcast_ref::<VecModel<SharedString>>() {
                if index < v.row_count() {
                    v.remove(index);
                }
            }
        }
    }

    pub fn choisir_cloture(&self, f: &Principale, index: i32) {
        if let Some((_, cloture)) = usize::try_from(index).ok().and_then(|i| CLOTURES.get(i)) {
            self.modifier_creation(f, |formulaire| formulaire.cloture = *cloture);
        }
    }

    pub fn fermer_creation_sondage(&self, f: &Principale) {
        self.verrou().creation_sondage = None;
        f.global::<DonneesCreationSondage>().set_ouvert(false);
        self.saisie.demander_focus(f);
    }

    pub fn publier_sondage(&self, f: &Principale) {
        self.creation_modifiee(f);
        let validation = {
            let mut i = self.verrou();
            let Some(c) = i.creation_sondage.as_mut() else {
                drop(i);
                self.fermer_creation_sondage(f);
                return;
            };
            c.tente = true;
            let formulaire = c.formulaire.clone();
            let cle = c.cle.clone();
            drop(i);
            self.valider_creation(&formulaire).map(|v| (cle, v))
        };
        match validation {
            Err(erreur) => {
                f.global::<DonneesCreationSondage>()
                    .set_erreur(erreur.into());
            }
            Ok((cle, (question, sondage))) => {
                self.fermer_creation_sondage(f);
                {
                    let mut i = self.verrou();
                    i.nouveau_depuis = None;
                    i.defilement.aller_en_bas();
                }
                self.lancer(Action::Envoyer(
                    cle,
                    Brouillon {
                        sondage: Some(sondage),
                        ..Brouillon::texte(&question)
                    },
                ));
            }
        }
    }

    fn signaler_plus_tard(&self, texte: String) {
        let saisie = self.saisie.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| saisie.signaler_erreur(&f, &texte));
    }

    pub fn voter(&self, id: &str, option: &str) {
        let pont = self.clone();
        let id = id.to_string();
        let option = option.to_string();
        self.runtime.spawn(async move {
            if let Err(erreur) = pont.store.voter(&id, &option).await {
                pont.signaler_plus_tard(format!("Vote impossible : {}", erreur.message()));
            }
        });
    }

    pub fn clore_sondage(&self, id: &str) {
        let pont = self.clone();
        let id = id.to_string();
        self.runtime.spawn(async move {
            if let Err(erreur) = pont.store.clore_sondage(&id).await {
                pont.signaler_plus_tard(format!("Clôture impossible : {}", erreur.message()));
            }
        });
    }

    pub fn supprimer_sondage(&self, f: &Principale, id: &str) {
        let pont = self.clone();
        let id = id.to_string();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer ce sondage ?".into(),
                message: "Le sondage et ses votes seront supprimés pour tout le monde.".into(),
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
    }

    pub fn afficher_votants(&self, f: &Principale, id: &str, votants: Votants) {
        let Some(cle) = self.conversation() else {
            return;
        };
        self.verrou().votants.insert(id.to_string(), votants);
        self.modifier_ligne(&cle, id);
        self.rafraichir(f);
    }

    pub fn basculer_votants(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        if self.verrou().votants.remove(id).is_some() {
            self.modifier_ligne(&cle, id);
            self.rafraichir(f);
            return;
        }
        let pont = self.clone();
        let id = id.to_string();
        self.runtime.spawn(async move {
            match pont.store.votants(&id).await {
                Ok(votants) => {
                    let _ = pont
                        .fenetre
                        .clone()
                        .upgrade_in_event_loop(move |f| pont.afficher_votants(&f, &id, votants));
                }
                Err(erreur) => {
                    pont.signaler_plus_tard(format!("Votants indisponibles : {}", erreur.message()))
                }
            }
        });
    }
}
