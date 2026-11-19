use std::collections::BTreeSet;

use estompe_client::route::Route;
use estompe_client::store::{Changement, CleConversation};
use slint::{Model, VecModel};

use super::projection::serveur_courant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Retirer(usize),
    Inserer(usize),
    Remplacer(usize),
}

pub fn plan<T: PartialEq, K: PartialEq>(
    anciens: &[T],
    nouveaux: &[T],
    cle: impl Fn(&T) -> K,
) -> Vec<Operation> {
    let mut operations = Vec::new();
    let mut courants: Vec<&T> = anciens.iter().collect();
    let cles_nouvelles: Vec<K> = nouveaux.iter().map(&cle).collect();
    let mut i = courants.len();
    while i > 0 {
        i -= 1;
        if !cles_nouvelles.contains(&cle(courants[i])) {
            courants.remove(i);
            operations.push(Operation::Retirer(i));
        }
    }
    for (i, nouveau) in nouveaux.iter().enumerate() {
        let k = &cles_nouvelles[i];
        match courants.get(i) {
            Some(present) if cle(present) == *k => {
                if *present != nouveau {
                    operations.push(Operation::Remplacer(i));
                }
                courants[i] = nouveau;
            }
            _ => {
                if let Some(j) = (i + 1..courants.len()).find(|&j| cle(courants[j]) == *k) {
                    courants.remove(j);
                    operations.push(Operation::Retirer(j));
                }
                courants.insert(i, nouveau);
                operations.push(Operation::Inserer(i));
            }
        }
    }
    operations
}

pub fn synchroniser<T: PartialEq + Clone, K: PartialEq, S: Clone + 'static>(
    modele: &VecModel<S>,
    ombre: &mut Vec<T>,
    nouveaux: Vec<T>,
    cle: impl Fn(&T) -> K,
    convertir: impl Fn(&T) -> S,
) -> Vec<Operation> {
    let operations = plan(ombre, &nouveaux, cle);
    for operation in &operations {
        match *operation {
            Operation::Retirer(i) => {
                modele.remove(i);
            }
            Operation::Inserer(i) => modele.insert(i, convertir(&nouveaux[i])),
            Operation::Remplacer(i) => modele.set_row_data(i, convertir(&nouveaux[i])),
        }
    }
    *ombre = nouveaux;
    debug_assert_eq!(modele.row_count(), ombre.len());
    operations
}

pub fn remplacer_ligne<T: PartialEq, S: Clone + 'static>(
    modele: &VecModel<S>,
    ombre: &mut [T],
    indice: usize,
    nouvelle: T,
    convertir: impl Fn(&T) -> S,
) -> bool {
    if ombre.get(indice) == Some(&nouvelle) {
        return false;
    }
    modele.set_row_data(indice, convertir(&nouvelle));
    ombre[indice] = nouvelle;
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Ensemble {
    #[default]
    Aucun,
    Certains(BTreeSet<String>),
    Tous,
}

impl Ensemble {
    pub fn ajouter(&mut self, valeur: &str) {
        match self {
            Self::Tous => {}
            Self::Certains(valeurs) => {
                valeurs.insert(valeur.to_string());
            }
            Self::Aucun => *self = Self::Certains(BTreeSet::from([valeur.to_string()])),
        }
    }

    pub fn tous(&mut self) {
        *self = Self::Tous;
    }

    pub fn est_vide(&self) -> bool {
        *self == Self::Aucun
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cibles {
    pub serveurs: Ensemble,
    pub salons: Ensemble,
    pub prives: Ensemble,
    pub accueil: bool,
    pub panneau: bool,
    pub vue: bool,
    pub compte: bool,
    pub corriger: bool,
}

pub fn cibles(changements: &[Changement], route: &Route) -> Cibles {
    let courant = serveur_courant(route);
    let est_courant = |serveur: &str| courant == Some(serveur);
    let mut c = Cibles::default();
    for changement in changements {
        match changement {
            Changement::Compte => {
                c.compte = true;
                c.accueil = true;
            }
            Changement::ServeurAjoute { .. } | Changement::OrdreServeurs => {
                c.serveurs.tous();
                c.accueil = true;
            }
            Changement::ServeurRetire { serveur } => {
                c.serveurs.tous();
                c.accueil = true;
                c.corriger = true;
                if est_courant(serveur) {
                    c.salons.tous();
                    c.panneau = true;
                    c.vue = true;
                }
            }
            Changement::Serveur { serveur } => {
                c.serveurs.ajouter(serveur);
                c.accueil = true;
                if est_courant(serveur) {
                    c.panneau = true;
                    c.vue = true;
                }
            }
            Changement::SalonAjoute { serveur, .. }
            | Changement::SalonRetire { serveur, .. }
            | Changement::OrdreSalons { serveur } => {
                c.serveurs.ajouter(serveur);
                c.accueil = true;
                if est_courant(serveur) {
                    c.salons.tous();
                    c.vue = true;
                }
            }
            Changement::Salon { serveur, salon } => {
                c.accueil = true;
                if est_courant(serveur) {
                    c.salons.ajouter(salon);
                    c.vue = true;
                }
            }
            Changement::NonLus {
                conversation,
                serveur,
            }
            | Changement::Mentions {
                conversation,
                serveur,
            } => {
                c.accueil = true;
                match (conversation, serveur) {
                    (CleConversation::Prive(id), _) => c.prives.ajouter(id),
                    (CleConversation::Salon(salon), Some(serveur)) => {
                        c.serveurs.ajouter(serveur);
                        if est_courant(serveur) {
                            c.salons.ajouter(salon);
                        }
                    }
                    (CleConversation::Salon(_), None) => {
                        c.serveurs.tous();
                        c.salons.tous();
                    }
                }
            }
            Changement::ReglagesNotification | Changement::NotificationsLues { .. } => {
                c.serveurs.tous();
                c.salons.tous();
                c.prives.tous();
                c.accueil = true;
            }
            Changement::MessagesPrives => {
                c.prives.tous();
                c.accueil = true;
                c.corriger = true;
                c.vue = matches!(route, Route::MessagePrive { .. });
            }
            Changement::Presence { .. } => c.prives.tous(),
            Changement::Vocal { salon } => {
                c.accueil = true;
                if courant.is_some() {
                    c.salons.ajouter(salon);
                }
            }
            Changement::Roles { .. }
            | Changement::MembreAjoute { .. }
            | Changement::MembreRetire { .. }
            | Changement::Membre { .. }
            | Changement::Avatar { .. }
            | Changement::ConversationCourante
            | Changement::Notification(_)
            | Changement::Avis(_) => {}
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::Operation::*;
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Ligne(&'static str, i32);

    fn lignes(valeurs: &[(&'static str, i32)]) -> Vec<Ligne> {
        valeurs.iter().map(|(k, v)| Ligne(k, *v)).collect()
    }

    fn appliquer(anciens: &[Ligne], nouveaux: &[Ligne]) -> (Vec<Operation>, Vec<Ligne>) {
        let modele = VecModel::from(anciens.to_vec());
        let mut ombre = anciens.to_vec();
        let operations = synchroniser(
            &modele,
            &mut ombre,
            nouveaux.to_vec(),
            |l| l.0,
            Clone::clone,
        );
        (operations, modele.iter().collect())
    }

    #[test]
    fn un_element_modifie_ne_remplace_que_sa_ligne() {
        let avant = lignes(&[("a", 0), ("b", 0), ("c", 0)]);
        let apres = lignes(&[("a", 0), ("b", 5), ("c", 0)]);
        assert_eq!(
            appliquer(&avant, &apres),
            (vec![Remplacer(1)], apres.clone())
        );
        assert_eq!(appliquer(&apres, &apres), (vec![], apres));
    }

    #[test]
    fn ajout_retrait_et_deplacement_sans_reconstruire_la_liste() {
        let avant = lignes(&[("a", 0), ("b", 0), ("c", 0)]);
        let ajout = lignes(&[("a", 0), ("n", 1), ("b", 0), ("c", 0)]);
        assert_eq!(appliquer(&avant, &ajout), (vec![Inserer(1)], ajout));
        let retrait = lignes(&[("a", 0), ("c", 0)]);
        assert_eq!(appliquer(&avant, &retrait), (vec![Retirer(1)], retrait));
        let deplacement = lignes(&[("c", 0), ("a", 0), ("b", 0)]);
        assert_eq!(
            appliquer(&avant, &deplacement),
            (vec![Retirer(2), Inserer(0)], deplacement)
        );
        let tout = lignes(&[("z", 9), ("b", 1)]);
        let (operations, resultat) = appliquer(&avant, &tout);
        assert_eq!(resultat, tout);
        assert_eq!(
            operations,
            vec![Retirer(2), Retirer(0), Inserer(0), Remplacer(1)]
        );
    }

    #[test]
    fn remplacer_une_ligne_identique_ne_touche_pas_le_modele() {
        let modele = VecModel::from(lignes(&[("a", 0)]));
        let mut ombre = lignes(&[("a", 0)]);
        assert!(!remplacer_ligne(
            &modele,
            &mut ombre,
            0,
            Ligne("a", 0),
            Clone::clone
        ));
        assert!(remplacer_ligne(
            &modele,
            &mut ombre,
            0,
            Ligne("a", 2),
            Clone::clone
        ));
        assert_eq!(modele.row_data(0), Some(Ligne("a", 2)));
    }

    fn salon(serveur: &str) -> Route {
        Route::Salon {
            serveur: serveur.into(),
            salon: None,
            message: None,
        }
    }

    fn ensemble(valeurs: &[&str]) -> Ensemble {
        Ensemble::Certains(valeurs.iter().map(|v| v.to_string()).collect())
    }

    #[test]
    fn un_non_lu_ne_vise_que_son_serveur_et_son_salon() {
        let non_lu = [Changement::NonLus {
            conversation: CleConversation::Salon("c2".into()),
            serveur: Some("s1".into()),
        }];
        let ici = cibles(&non_lu, &salon("s1"));
        assert_eq!(
            ici,
            Cibles {
                serveurs: ensemble(&["s1"]),
                salons: ensemble(&["c2"]),
                accueil: true,
                ..Cibles::default()
            }
        );
        let ailleurs = cibles(&non_lu, &Route::Accueil);
        assert_eq!(
            (ailleurs.serveurs, ailleurs.salons),
            (ensemble(&["s1"]), Ensemble::Aucun)
        );
        let prive = cibles(
            &[Changement::Mentions {
                conversation: CleConversation::Prive("d1".into()),
                serveur: None,
            }],
            &Route::Accueil,
        );
        assert_eq!(
            (prive.prives, prive.serveurs),
            (ensemble(&["d1"]), Ensemble::Aucun)
        );
    }

    #[test]
    fn la_structure_des_serveurs_et_des_salons_vise_les_listes_entieres() {
        let retire = cibles(
            &[Changement::ServeurRetire {
                serveur: "s1".into(),
            }],
            &salon("s1"),
        );
        assert!(retire.corriger && retire.panneau && retire.vue);
        assert_eq!(
            (retire.serveurs, retire.salons),
            (Ensemble::Tous, Ensemble::Tous)
        );
        let ordre = cibles(&[Changement::OrdreServeurs], &Route::Accueil);
        assert_eq!(
            (ordre.serveurs, ordre.salons, ordre.corriger),
            (Ensemble::Tous, Ensemble::Aucun, false)
        );
        let ajout = cibles(
            &[Changement::SalonAjoute {
                serveur: "s2".into(),
                salon: "x".into(),
            }],
            &salon("s1"),
        );
        assert_eq!(
            (ajout.serveurs, ajout.salons, ajout.vue),
            (ensemble(&["s2"]), Ensemble::Aucun, false)
        );
        let renomme = cibles(
            &[Changement::Salon {
                serveur: "s1".into(),
                salon: "c1".into(),
            }],
            &salon("s1"),
        );
        assert_eq!((renomme.salons, renomme.vue), (ensemble(&["c1"]), true));
        let rien = cibles(
            &[Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "u".into(),
            }],
            &salon("s1"),
        );
        assert_eq!(rien, Cibles::default());
    }
}
