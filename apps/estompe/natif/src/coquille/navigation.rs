use std::collections::BTreeMap;

use estompe_client::derniere_vue::DerniereVue;
use estompe_client::route::{analyser_hash, lien_message_prive, lien_salon, Route};
use estompe_client::store::{CleConversation, Etat};

use super::projection::{salon_affiche, salon_de_la_route};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    route: Route,
    derniers_salons: BTreeMap<String, String>,
}

impl Default for Navigation {
    fn default() -> Self {
        Self::nouvelle(Route::Accueil)
    }
}

fn id(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

impl Navigation {
    pub fn nouvelle(route: Route) -> Self {
        Self {
            route,
            derniers_salons: BTreeMap::new(),
        }
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    pub fn aller(&mut self, route: Route, etat: &Etat) -> bool {
        if let Route::Salon { serveur, .. } = &route {
            if let Some(salon) = salon_de_la_route(etat, &route) {
                self.derniers_salons
                    .insert(serveur.clone(), id(&salon.id).to_string());
            }
        }
        if route == self.route {
            return false;
        }
        self.route = route;
        true
    }

    pub fn route_du_serveur(&self, serveur: &str, etat: &Etat) -> Route {
        let dernier = self
            .derniers_salons
            .get(serveur)
            .filter(|salon| etat.salons(serveur).iter().any(|c| id(&c.id) == *salon))
            .cloned();
        Route::Salon {
            serveur: serveur.to_string(),
            salon: dernier,
            message: None,
        }
    }

    pub fn route_de_la_conversation(etat: &Etat, cle: &CleConversation) -> Option<Route> {
        match cle {
            CleConversation::Prive(id) => Some(Route::MessagePrive {
                id: id.clone(),
                message: None,
            }),
            CleConversation::Salon(salon) => {
                etat.serveur_du_salon(salon).map(|serveur| Route::Salon {
                    serveur,
                    salon: Some(salon.clone()),
                    message: None,
                })
            }
        }
    }

    pub fn corriger(&mut self, etat: &Etat) -> bool {
        let valide = match &self.route {
            Route::Salon { serveur, .. } => etat.serveur(serveur).is_some(),
            Route::MessagePrive { id: prive, .. } => {
                etat.messages_prives.iter().any(|d| id(&d.id) == prive)
            }
            _ => true,
        };
        if valide {
            return false;
        }
        self.route = Route::Accueil;
        true
    }

    pub fn adresse_a_retenir(&self, etat: &Etat) -> Option<String> {
        match &self.route {
            Route::Salon { serveur, salon, .. } => {
                let affiche = salon_affiche(etat, serveur, salon.as_deref()).map(|s| id(&s.id));
                Some(lien_salon(serveur, affiche.or(salon.as_deref())))
            }
            Route::MessagePrive { id, .. } => Some(lien_message_prive(id)),
            _ => None,
        }
    }
}

pub fn route_de_demarrage(etat: &Etat, vue: Option<&DerniereVue>) -> Route {
    let serveurs: Vec<String> = etat
        .serveurs
        .iter()
        .map(|s| id(&s.id).to_string())
        .collect();
    let prives: Vec<String> = etat
        .messages_prives
        .iter()
        .map(|d| id(&d.id).to_string())
        .collect();
    estompe_client::derniere_vue::adresse_a_rouvrir(vue, etat.moi(), &serveurs, &prives)
        .map(|adresse| analyser_hash(adresse, &[]))
        .map(|route| match route {
            Route::Salon { serveur, salon, .. } => Route::Salon {
                serveur,
                salon,
                message: None,
            },
            Route::MessagePrive { id, .. } => Route::MessagePrive { id, message: None },
            autre => autre,
        })
        .unwrap_or(Route::Accueil)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;

    fn salon(serveur: &str, salon: Option<&str>) -> Route {
        Route::Salon {
            serveur: serveur.into(),
            salon: salon.map(String::from),
            message: None,
        }
    }

    #[test]
    fn revient_au_dernier_salon_vu_du_serveur() {
        let mut e = etat();
        let mut n = Navigation::default();
        assert_eq!(n.route_du_serveur("s1", &e), salon("s1", None));
        assert!(n.aller(salon("s1", Some("c2")), &e));
        assert!(n.aller(salon("s2", None), &e));
        assert!(!n.aller(salon("s2", None), &e));
        assert_eq!(n.route_du_serveur("s1", &e), salon("s1", Some("c2")));
        assert_eq!(n.route_du_serveur("s2", &e), salon("s2", Some("t1")));
        e.details
            .get_mut("s1")
            .unwrap()
            .channels
            .as_mut()
            .unwrap()
            .retain(|c| c.id.as_deref() != Some("c2"));
        assert_eq!(n.route_du_serveur("s1", &e), salon("s1", None));
    }

    #[test]
    fn retourne_a_l_accueil_quand_le_serveur_ou_le_message_prive_disparait() {
        let mut e = etat();
        let mut n = Navigation::nouvelle(salon("s2", None));
        assert!(!n.corriger(&e));
        e.serveurs.retain(|s| s.id.as_deref() != Some("s2"));
        assert!(n.corriger(&e));
        assert_eq!(*n.route(), Route::Accueil);
        let mut n = Navigation::nouvelle(Route::MessagePrive {
            id: "d9".into(),
            message: None,
        });
        assert!(n.corriger(&e));
    }

    #[test]
    fn retient_l_adresse_du_salon_affiche_ou_du_message_prive() {
        let e = etat();
        let adresse = |route| Navigation::nouvelle(route).adresse_a_retenir(&e);
        assert_eq!(adresse(salon("s1", None)).as_deref(), Some("#/s/s1/c1"));
        assert_eq!(
            adresse(salon("s1", Some("v1"))).as_deref(),
            Some("#/s/s1/v1")
        );
        assert_eq!(
            adresse(Route::MessagePrive {
                id: "d1".into(),
                message: None
            })
            .as_deref(),
            Some("#/dm/d1")
        );
        assert_eq!(adresse(Route::Accueil), None);
    }

    #[test]
    fn restaure_la_derniere_vue_du_compte_au_lancement() {
        let e = etat();
        let vue = |compte: &str, adresse: &str| DerniereVue::enregistrer(compte, adresse);
        assert_eq!(
            route_de_demarrage(&e, Some(&vue("me", "#/s/s1/c2"))),
            salon("s1", Some("c2"))
        );
        assert_eq!(
            route_de_demarrage(&e, Some(&vue("me", "#/dm/d2/55"))),
            Route::MessagePrive {
                id: "d2".into(),
                message: None
            }
        );
        assert_eq!(
            route_de_demarrage(&e, Some(&vue("autre", "#/s/s1/c2"))),
            Route::Accueil
        );
        assert_eq!(
            route_de_demarrage(&e, Some(&vue("me", "#/s/s9"))),
            Route::Accueil
        );
        assert_eq!(route_de_demarrage(&e, None), Route::Accueil);
    }

    #[test]
    fn une_conversation_mene_a_son_serveur_ou_au_message_prive() {
        let e = etat();
        assert_eq!(
            Navigation::route_de_la_conversation(&e, &CleConversation::Salon("t1".into())),
            Some(salon("s2", Some("t1")))
        );
        assert_eq!(
            Navigation::route_de_la_conversation(&e, &CleConversation::Salon("zz".into())),
            None
        );
        assert_eq!(
            Navigation::route_de_la_conversation(&e, &CleConversation::Prive("d1".into())),
            Some(Route::MessagePrive {
                id: "d1".into(),
                message: None
            })
        );
    }
}
