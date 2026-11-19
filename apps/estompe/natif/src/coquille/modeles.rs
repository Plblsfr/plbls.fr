use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;

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

pub fn plan<T: PartialEq, K: Eq + Hash>(
    anciens: &[T],
    nouveaux: &[T],
    cle: impl Fn(&T) -> K,
) -> Vec<Operation> {
    let mut operations = Vec::new();
    let mut courants: Vec<&T> = anciens.iter().collect();
    let cles_nouvelles: Vec<K> = nouveaux.iter().map(&cle).collect();
    let positions: HashMap<&K, usize> = cles_nouvelles
        .iter()
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect();
    let mut i = courants.len();
    while i > 0 {
        i -= 1;
        if !positions.contains_key(&cle(courants[i])) {
            courants.remove(i);
            operations.push(Operation::Retirer(i));
        }
    }
    let mut i = 0;
    while i < nouveaux.len() {
        let k = &cles_nouvelles[i];
        match courants.get(i) {
            Some(present) if cle(present) == *k => {
                if *present != &nouveaux[i] {
                    operations.push(Operation::Remplacer(i));
                }
                courants[i] = &nouveaux[i];
            }
            Some(present)
                if positions.get(&cle(present)).is_some_and(|&p| p > i)
                    && courants
                        .get(i + 1)
                        .is_some_and(|suivant| cle(suivant) == *k) =>
            {
                courants.remove(i);
                operations.push(Operation::Retirer(i));
                continue;
            }
            _ => {
                if let Some(j) = (i + 1..courants.len()).find(|&j| cle(courants[j]) == *k) {
                    courants.remove(j);
                    operations.push(Operation::Retirer(j));
                }
                courants.insert(i, &nouveaux[i]);
                operations.push(Operation::Inserer(i));
            }
        }
        i += 1;
    }
    operations
}

pub fn synchroniser<T: PartialEq + Clone, K: Eq + Hash, S: Clone + 'static>(
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
    pub membres: bool,
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
                    c.membres = true;
                }
            }
            Changement::Serveur { serveur } => {
                c.serveurs.ajouter(serveur);
                c.accueil = true;
                if est_courant(serveur) {
                    c.panneau = true;
                    c.vue = true;
                    c.membres = true;
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
            Changement::Presence { .. } => {
                c.prives.tous();
                c.membres |= courant.is_some();
            }
            Changement::Roles { serveur }
            | Changement::MembreAjoute { serveur, .. }
            | Changement::MembreRetire { serveur, .. }
            | Changement::Membre { serveur, .. } => c.membres |= est_courant(serveur),
            Changement::Avatar { serveur, .. } => {
                c.membres |= match serveur {
                    Some(serveur) => est_courant(serveur),
                    None => courant.is_some(),
                }
            }
            Changement::Vocal { salon } => {
                c.accueil = true;
                if courant.is_some() {
                    c.salons.ajouter(salon);
                }
            }
            Changement::ConversationCourante
            | Changement::Notification(_)
            | Changement::Avis(_)
            | Changement::Messages(_) => {}
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
        let rien = cibles(&[Changement::ConversationCourante], &salon("s1"));
        assert_eq!(rien, Cibles::default());
    }

    #[test]
    fn les_membres_ne_visent_que_la_liste_du_serveur_courant() {
        let membres = Cibles {
            membres: true,
            ..Cibles::default()
        };
        for changement in [
            Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "u".into(),
            },
            Changement::MembreAjoute {
                serveur: "s1".into(),
                utilisateur: "u".into(),
            },
            Changement::MembreRetire {
                serveur: "s1".into(),
                utilisateur: "u".into(),
            },
            Changement::Roles {
                serveur: "s1".into(),
            },
            Changement::Avatar {
                utilisateur: "u".into(),
                serveur: Some("s1".into()),
            },
            Changement::Avatar {
                utilisateur: "u".into(),
                serveur: None,
            },
        ] {
            assert_eq!(
                cibles(std::slice::from_ref(&changement), &salon("s1")),
                membres,
                "{changement:?}"
            );
            assert_eq!(
                cibles(std::slice::from_ref(&changement), &salon("s2")).membres,
                matches!(changement, Changement::Avatar { serveur: None, .. }),
                "{changement:?}"
            );
            assert!(!cibles(&[changement], &Route::Accueil).membres);
        }
        let presence = cibles(
            &[Changement::Presence {
                utilisateur: "u".into(),
            }],
            &salon("s2"),
        );
        assert_eq!((presence.membres, presence.prives), (true, Ensemble::Tous));
    }

    #[test]
    fn un_membre_qui_change_de_groupe_ne_deplace_que_sa_ligne() {
        let avant = lignes(&[("h1", 3), ("a", 0), ("b", 0), ("c", 0), ("h2", 1), ("d", 0)]);
        let descend = lignes(&[("h1", 2), ("b", 0), ("c", 0), ("h2", 2), ("a", 1), ("d", 0)]);
        let (operations, resultat) = appliquer(&avant, &descend);
        assert_eq!(resultat, descend);
        assert_eq!(
            operations,
            vec![Remplacer(0), Retirer(1), Remplacer(3), Inserer(4)]
        );
        let (operations, resultat) = appliquer(&descend, &avant);
        assert_eq!(resultat, avant);
        assert_eq!(
            operations,
            vec![Remplacer(0), Retirer(4), Inserer(1), Remplacer(4)]
        );
        let grand: Vec<(String, i32)> = (0..5000).map(|i| (format!("m{i}"), 0)).collect();
        let mut deplace = grand.clone();
        let parti = deplace.remove(10);
        deplace.insert(4000, parti);
        assert_eq!(
            plan(&grand, &deplace, |l| l.0.clone()),
            vec![Retirer(10), Inserer(4000)]
        );
        let remonte = plan(&deplace, &grand, |l| l.0.clone());
        assert_eq!(remonte, vec![Retirer(4000), Inserer(10)]);
    }
}
