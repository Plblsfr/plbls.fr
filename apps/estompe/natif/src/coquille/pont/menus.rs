use estompe_client::route::{adresse_web, lien_message_prive, lien_salon};
use estompe_client::store::{CleConversation, Presence};
use estompe_client::temps_reel::evenements::StatutPresence;

use super::super::membres::DonneesMembre;
use super::super::projection::{serveur_courant, DonneesPrive, DonneesSalon};
use super::super::recherche::STATUTS;
use super::parametres::REGLER_NOTIFICATIONS;
use super::Pont;
use crate::menu::{self, Ancre, Entree};
use crate::Principale;
use slint::ComponentHandle;

pub const MARQUER_LU: &str = "lu";
pub const COPIER_LIEN: &str = "lien";
pub const PROFIL: &str = "profil";
pub const MENTIONNER: &str = "mentionner";
pub const COPIER_PSEUDO: &str = "pseudo";
pub const VOIR_MON_PROFIL: &str = "mon-profil";
pub const MODIFIER_MON_PROFIL: &str = "modifier-profil";
pub const SIGNALER_UN_BUG: &str = "bug";
pub const JEUX_ET_ACTIVITE: &str = "jeux";
pub const PARAMETRES: &str = "parametres";

fn id_du_statut(statut: StatutPresence) -> &'static str {
    match statut {
        StatutPresence::Idle => "idle",
        StatutPresence::Dnd => "dnd",
        _ => "online",
    }
}

pub fn entrees_de_moi(actuel: Presence, jeux: bool) -> Vec<Entree> {
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
    ]);
    if jeux {
        entrees.push(Entree::nouvelle(
            JEUX_ET_ACTIVITE,
            "Jeux et activité",
            "trophee",
        ));
    }
    entrees.push(Entree::nouvelle(SIGNALER_UN_BUG, "Signaler un bug", "bug").bientot());
    entrees.push(Entree::nouvelle(PARAMETRES, "Paramètres", "reglages").separee());
    entrees
}

fn lien() -> Entree {
    Entree::nouvelle(COPIER_LIEN, "Copier le lien", "lien")
}

fn lu(non_lus: i64, mentions: usize) -> Option<Entree> {
    (non_lus > 0 || mentions > 0).then(|| Entree::nouvelle(MARQUER_LU, "Marquer comme lu", "ok"))
}

pub fn entrees_du_salon(salon: &DonneesSalon, sourdine: Vec<Entree>) -> Vec<Entree> {
    let lu = lu(salon.non_lus, salon.mentions).filter(|_| !salon.bientot);
    let sourdine = if salon.vocal || salon.bientot {
        Vec::new()
    } else {
        sourdine
    };
    lu.into_iter().chain(sourdine).chain([lien()]).collect()
}

pub fn entrees_du_serveur(sourdine: Vec<Entree>) -> Vec<Entree> {
    sourdine
        .into_iter()
        .chain([
            Entree::nouvelle(REGLER_NOTIFICATIONS, "Paramètres de notification", "cloche"),
            lien().separee(),
        ])
        .collect()
}

pub fn entrees_du_prive(prive: &DonneesPrive) -> Vec<Entree> {
    lu(prive.non_lus, prive.mentions)
        .into_iter()
        .chain([
            lien(),
            Entree::nouvelle(COPIER_PSEUDO, "Copier le pseudo", "copie"),
        ])
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
        let sourdine = self.entrees_de_sourdine_de(&salon.id);
        let titre = salon.nom.clone();
        menu::ouvrir(
            f,
            &titre,
            ancre,
            entrees_du_salon(&salon, sourdine),
            move |f, action| {
                match action {
                    MARQUER_LU => pont.lire(CleConversation::Salon(salon.id.clone())),
                    COPIER_LIEN => menu::copier_texte(
                        f,
                        &pont.lien_absolu(&lien_salon(&serveur, Some(&salon.id))),
                    ),
                    _ => {
                        pont.action_de_sourdine(
                            f,
                            &salon.id,
                            &format!("#{}", salon.nom),
                            ancre,
                            action,
                        );
                    }
                }
                false
            },
        )
    }

    pub fn menu_serveur(&self, f: &Principale, id: &str, ancre: Ancre) -> bool {
        let Some(nom) = self
            .store
            .lire(|e| e.serveur(id).map(|s| s.name.clone().unwrap_or_default()))
        else {
            return false;
        };
        let pont = self.clone();
        let id = id.to_string();
        let sourdine = self.entrees_de_sourdine_de(&id);
        menu::ouvrir(
            f,
            &nom.clone(),
            ancre,
            entrees_du_serveur(sourdine),
            move |f, action| match action {
                REGLER_NOTIFICATIONS => {
                    let g = f.global::<crate::DonneesParametres>();
                    g.set_serveur_notif(id.as_str().into());
                    pont.projeter_parametres(f);
                    g.invoke_ouvrir("notifications".into());
                    true
                }
                COPIER_LIEN => {
                    menu::copier_texte(f, &pont.lien_absolu(&lien_salon(&id, None)));
                    false
                }
                _ => {
                    pont.action_de_sourdine(f, &id, &nom, ancre, action);
                    false
                }
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
        let titre = prive.pseudo.clone();
        menu::ouvrir(
            f,
            &titre,
            ancre,
            entrees_du_prive(&prive),
            move |f, action| {
                match action {
                    MARQUER_LU => pont.lire(CleConversation::Prive(prive.id.clone())),
                    COPIER_LIEN => {
                        menu::copier_texte(f, &pont.lien_absolu(&lien_message_prive(&prive.id)))
                    }
                    COPIER_PSEUDO => menu::copier_texte(f, &prive.pseudo),
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
            entrees_de_moi(actuel, crate::jeux::reglages::disponible(f)),
            move |f, action| {
                if action == JEUX_ET_ACTIVITE {
                    crate::jeux::reglages::ouvrir(f);
                    return true;
                }
                if action == PARAMETRES {
                    f.global::<crate::DonneesParametres>()
                        .invoke_ouvrir("".into());
                    return true;
                }
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
            ids(&entrees_du_salon(&salon(3, false), Vec::new())),
            [MARQUER_LU, COPIER_LIEN]
        );
        assert_eq!(
            ids(&entrees_du_salon(&salon(0, false), Vec::new())),
            [COPIER_LIEN]
        );
        assert_eq!(
            ids(&entrees_du_salon(&salon(3, true), Vec::new())),
            [COPIER_LIEN]
        );
    }

    #[test]
    fn la_sourdine_se_propose_pour_un_salon_texte_et_pour_un_serveur() {
        let sourdine = || super::super::parametres::entrees_de_sourdine(None);
        assert_eq!(
            ids(&entrees_du_salon(&salon(3, false), sourdine())),
            [MARQUER_LU, super::super::parametres::SOURDINE, COPIER_LIEN]
        );
        let mut vocal = salon(0, false);
        vocal.vocal = true;
        assert_eq!(ids(&entrees_du_salon(&vocal, sourdine())), [COPIER_LIEN]);
        let serveur = entrees_du_serveur(sourdine());
        assert_eq!(
            ids(&serveur),
            [
                super::super::parametres::SOURDINE,
                REGLER_NOTIFICATIONS,
                COPIER_LIEN
            ]
        );
        assert!(serveur[2].separe);
    }

    #[test]
    fn le_menu_d_une_conversation_privee_copie_le_pseudo_comme_le_front() {
        let prive = |non_lus| DonneesPrive {
            id: "d1".into(),
            pseudo: "bob".into(),
            initiales: "B".into(),
            teinte: 0,
            actif: false,
            non_lus,
            mentions: 0,
            presence: Presence::EnLigne,
            brouillon: false,
        };
        assert_eq!(
            ids(&entrees_du_prive(&prive(2))),
            [MARQUER_LU, COPIER_LIEN, COPIER_PSEUDO]
        );
        assert_eq!(
            ids(&entrees_du_prive(&prive(0))),
            [COPIER_LIEN, COPIER_PSEUDO]
        );
    }

    #[test]
    fn le_menu_de_l_utilisateur_coche_son_statut() {
        let entrees = entrees_de_moi(Presence::Absent, false);
        assert_eq!(
            ids(&entrees),
            [
                "online",
                "idle",
                "dnd",
                VOIR_MON_PROFIL,
                MODIFIER_MON_PROFIL,
                SIGNALER_UN_BUG,
                PARAMETRES
            ]
        );
        let coches: Vec<bool> = entrees.iter().map(|e| e.coche).collect();
        assert_eq!(coches, [false, true, false, false, false, false, false]);
        assert!(entrees[3].separe && entrees[3].bientot);
        assert!(entrees[6].separe && !entrees[6].bientot);
        let avec_jeux = entrees_de_moi(Presence::Absent, true);
        assert_eq!(
            ids(&avec_jeux)[5..],
            [JEUX_ET_ACTIVITE, SIGNALER_UN_BUG, PARAMETRES]
        );
        assert!(!avec_jeux[5].bientot);
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
