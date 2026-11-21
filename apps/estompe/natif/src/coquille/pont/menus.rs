use estompe_client::route::{adresse_web, lien_message_prive, lien_salon};
use estompe_client::store::{CleConversation, Presence};
use estompe_client::temps_reel::evenements::StatutPresence;

use super::super::membres::DonneesMembre;
use super::super::projection::{serveur_courant, DonneesPrive, DonneesSalon};
use super::super::recherche::STATUTS;
use super::Pont;
use crate::menu::{self, Ancre, Entree};
use crate::Principale;

pub const MARQUER_LU: &str = "lu";
pub const COPIER_LIEN: &str = "lien";
pub const PROFIL: &str = "profil";
pub const MENTIONNER: &str = "mentionner";
pub const COPIER_PSEUDO: &str = "pseudo";
pub const VOIR_MON_PROFIL: &str = "mon-profil";
pub const MODIFIER_MON_PROFIL: &str = "modifier-profil";
pub const SIGNALER_UN_BUG: &str = "bug";

fn id_du_statut(statut: StatutPresence) -> &'static str {
    match statut {
        StatutPresence::Idle => "idle",
        StatutPresence::Dnd => "dnd",
        _ => "online",
    }
}

pub fn entrees_de_moi(actuel: Presence) -> Vec<Entree> {
    let mut entrees: Vec<Entree> = STATUTS
        .iter()
        .map(|(statut, presence, libelle)| {
            Entree::nouvelle(id_du_statut(*statut), libelle, "perso").cochee(*presence == actuel)
        })
        .collect();
    entrees.extend([
        Entree::nouvelle(VOIR_MON_PROFIL, "Voir mon profil", "perso")
            .separee()
            .bientot(),
        Entree::nouvelle(MODIFIER_MON_PROFIL, "Modifier mon profil", "reglages").bientot(),
        Entree::nouvelle(SIGNALER_UN_BUG, "Signaler un bug", "bug").bientot(),
    ]);
    entrees
}

fn lien() -> Entree {
    Entree::nouvelle(COPIER_LIEN, "Copier le lien", "lien")
}

fn lu(non_lus: i64, mentions: usize) -> Option<Entree> {
    (non_lus > 0 || mentions > 0).then(|| Entree::nouvelle(MARQUER_LU, "Marquer comme lu", "ok"))
}

pub fn entrees_du_salon(salon: &DonneesSalon) -> Vec<Entree> {
    let lu = lu(salon.non_lus, salon.mentions).filter(|_| !salon.bientot);
    lu.into_iter().chain([lien()]).collect()
}

pub fn entrees_du_prive(prive: &DonneesPrive) -> Vec<Entree> {
    lu(prive.non_lus, prive.mentions)
        .into_iter()
        .chain([lien()])
        .collect()
}

pub fn entrees_du_membre(mentionner: bool) -> Vec<Entree> {
    let mut entrees = vec![Entree::nouvelle(PROFIL, "Profil", "perso")];
    if mentionner {
        entrees.push(Entree::nouvelle(MENTIONNER, "Mentionner", "arobase"));
    }
    entrees.push(Entree::nouvelle(COPIER_PSEUDO, "Copier le pseudo", "copie"));
    entrees
}

impl Pont {
    fn lire(&self, cle: CleConversation) {
        let store = self.store.clone();
        self.runtime
            .spawn(async move { store.lire_conversation(&cle).await });
    }

    pub fn tout_lire(&self) {
        let store = self.store.clone();
        self.runtime.spawn(async move { store.tout_lire().await });
    }

    fn lien_absolu(&self, lien: &str) -> String {
        adresse_web(self.store.client().adresse(), lien)
    }

    pub fn menu_salon(&self, f: &Principale, id: &str, ancre: Ancre) -> bool {
        let Some(serveur) = serveur_courant(&self.route()).map(str::to_string) else {
            return false;
        };
        let Some(salon) = self
            .verrou()
            .ombres
            .salons
            .iter()
            .find(|s| s.id == id)
            .cloned()
        else {
            return false;
        };
        let pont = self.clone();
        menu::ouvrir(
            f,
            &salon.nom,
            ancre,
            entrees_du_salon(&salon),
            move |f, action| {
                match action {
                    MARQUER_LU => pont.lire(CleConversation::Salon(salon.id.clone())),
                    COPIER_LIEN => menu::copier_texte(
                        f,
                        &pont.lien_absolu(&lien_salon(&serveur, Some(&salon.id))),
                    ),
                    _ => {}
                }
                false
            },
        )
    }

    pub fn menu_prive(&self, f: &Principale, id: &str, ancre: Ancre) -> bool {
        let Some(prive) = self
            .verrou()
            .ombres
            .prives
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return false;
        };
        let pont = self.clone();
        menu::ouvrir(
            f,
            &prive.pseudo,
            ancre,
            entrees_du_prive(&prive),
            move |f, action| {
                match action {
                    MARQUER_LU => pont.lire(CleConversation::Prive(prive.id.clone())),
                    COPIER_LIEN => {
                        menu::copier_texte(f, &pont.lien_absolu(&lien_message_prive(&prive.id)))
                    }
                    _ => {}
                }
                false
            },
        )
    }

    pub fn definir_statut(&self, statut: StatutPresence) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            if let Err(erreur) = pont.store.definir_statut(statut).await {
                let texte = format!("Changement de statut impossible : {}", erreur.message());
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

    pub fn menu_moi(&self, f: &Principale, ancre: Ancre) -> bool {
        let (pseudo, actuel) = self
            .store
            .lire(|e| (e.pseudo().to_string(), e.presence_de(e.moi())));
        let pont = self.clone();
        menu::ouvrir(
            f,
            &pseudo,
            ancre,
            entrees_de_moi(actuel),
            move |_, action| {
                if let Some((statut, _, _)) =
                    STATUTS.iter().find(|(s, _, _)| id_du_statut(*s) == action)
                {
                    pont.definir_statut(*statut);
                }
                false
            },
        )
    }

    fn mention_possible(&self) -> bool {
        let serveur = self.verrou().membres.serveur.clone();
        let salon = self
            .messagerie()
            .conversation()
            .and_then(|c| c.salon().map(str::to_string));
        let du_serveur = match (salon, serveur) {
            (Some(salon), Some(serveur)) => self
                .store
                .lire(|e| e.serveur_du_salon(&salon))
                .is_some_and(|s| s == serveur),
            _ => false,
        };
        du_serveur && self.messagerie().ecriture_permise()
    }

    pub fn menu_membre(&self, f: &Principale, utilisateur: &str, ancre: Ancre) -> bool {
        let membre: Option<DonneesMembre> = {
            let i = self.verrou();
            i.membres
                .ombre
                .iter()
                .find(|m| !m.en_tete && m.utilisateur == utilisateur)
                .cloned()
        };
        let Some(membre) = membre else {
            return false;
        };
        let pont = self.clone();
        let entrees = entrees_du_membre(self.mention_possible());
        menu::ouvrir(
            f,
            &membre.nom,
            ancre,
            entrees,
            move |f, action| match action {
                PROFIL => {
                    pont.ouvrir_membre(f, &membre.utilisateur);
                    true
                }
                MENTIONNER => {
                    pont.messagerie().inserer_dans_la_saisie(
                        f,
                        &format!("@{} ", membre.pseudo),
                        true,
                    );
                    pont.rafraichir_brouillons(f);
                    true
                }
                COPIER_PSEUDO => {
                    menu::copier_texte(f, &membre.pseudo);
                    false
                }
                _ => false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::projection::DonneesSalon;

    fn ids(entrees: &[Entree]) -> Vec<&str> {
        entrees.iter().map(|e| e.id.as_str()).collect()
    }

    fn salon(non_lus: i64, bientot: bool) -> DonneesSalon {
        DonneesSalon {
            id: "c1".into(),
            nom: "général".into(),
            icone: "hash",
            titre_groupe: "",
            actif: false,
            non_lus,
            mentions: 0,
            bientot,
            occupants: 0,
            brouillon: false,
            duree: String::new(),
            vocal: false,
        }
    }

    #[test]
    fn marquer_comme_lu_n_apparait_que_pour_un_salon_texte_avec_des_non_lus() {
        assert_eq!(
            ids(&entrees_du_salon(&salon(3, false))),
            [MARQUER_LU, COPIER_LIEN]
        );
        assert_eq!(ids(&entrees_du_salon(&salon(0, false))), [COPIER_LIEN]);
        assert_eq!(ids(&entrees_du_salon(&salon(3, true))), [COPIER_LIEN]);
    }

    #[test]
    fn le_menu_de_l_utilisateur_coche_son_statut() {
        let entrees = entrees_de_moi(Presence::Absent);
        assert_eq!(
            ids(&entrees),
            [
                "online",
                "idle",
                "dnd",
                VOIR_MON_PROFIL,
                MODIFIER_MON_PROFIL,
                SIGNALER_UN_BUG
            ]
        );
        let coches: Vec<bool> = entrees.iter().map(|e| e.coche).collect();
        assert_eq!(coches, [false, true, false, false, false, false]);
        assert!(entrees[3].separe && entrees[3].bientot);
    }

    #[test]
    fn mentionner_un_membre_demande_une_conversation_de_son_serveur_ouverte_en_ecriture() {
        assert_eq!(
            ids(&entrees_du_membre(true)),
            [PROFIL, MENTIONNER, COPIER_PSEUDO]
        );
        assert_eq!(ids(&entrees_du_membre(false)), [PROFIL, COPIER_PSEUDO]);
    }
}
