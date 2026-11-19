use futures_util::future::join_all;
use serde_json::Value;

use crate::config::valider_adresse;
use crate::http::{Client, ErreurApi};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    Entracte,
    Sillon,
    Sablier,
    Regie,
}

impl Section {
    pub const TOUTES: [Section; 4] = [
        Section::Entracte,
        Section::Sillon,
        Section::Sablier,
        Section::Regie,
    ];

    pub fn variable(self) -> &'static str {
        match self {
            Self::Entracte => "ESTOMPE_ENTRACTE_URL",
            Self::Sillon => "ESTOMPE_SILLON_URL",
            Self::Sablier => "ESTOMPE_SABLIER_URL",
            Self::Regie => "ESTOMPE_REGIE_URL",
        }
    }

    pub fn adresse_de_production(self) -> Option<&'static str> {
        match self {
            Self::Entracte => Some("https://entracte-api.plbls.fr"),
            Self::Sillon => Some("https://sillon-api.plbls.fr"),
            Self::Sablier => Some("https://sablier-api.plbls.fr"),
            Self::Regie => None,
        }
    }

    pub fn ouverte_par(self, moi: &Value) -> bool {
        let droit = match self {
            Self::Regie => "admin",
            _ => "access",
        };
        moi.pointer(&format!("/permissions/{droit}"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

pub fn adresses_sections(
    lire: impl Fn(&str) -> Option<String>,
    production: bool,
) -> Vec<(Section, String)> {
    Section::TOUTES
        .iter()
        .filter_map(|&section| {
            let adresse = match lire(section.variable()) {
                Some(valeur) => valider_adresse(&valeur).ok()?,
                None if production => section.adresse_de_production()?.to_string(),
                None => return None,
            };
            Some((section, adresse))
        })
        .collect()
}

pub async fn sections_ouvertes(
    adresses: &[(Section, String)],
    client_de: impl Fn(&str) -> Result<Client, ErreurApi>,
) -> Vec<Section> {
    let appels = adresses.iter().map(|(section, adresse)| {
        let client = client_de(adresse);
        async move {
            let moi = client.ok()?.get::<Value>("/me").await.ok()?;
            section.ouverte_par(&moi).then_some(*section)
        }
    });
    join_all(appels).await.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    #[test]
    fn suit_les_permissions_annoncees_par_le_me_de_chaque_section() {
        let acces = json!({"permissions": {"access": true, "admin": false}});
        let admin = json!({"permissions": {"access": false, "admin": true}});
        assert!(Section::Entracte.ouverte_par(&acces));
        assert!(Section::Sablier.ouverte_par(&acces));
        assert!(!Section::Regie.ouverte_par(&acces));
        assert!(Section::Regie.ouverte_par(&admin));
        assert!(!Section::Sillon.ouverte_par(&admin));
        assert!(!Section::Sillon.ouverte_par(&json!({})));
        assert!(!Section::Sillon.ouverte_par(&json!({"permissions": {"access": "oui"}})));
    }

    #[test]
    fn prend_les_adresses_de_l_environnement_ou_celles_de_production() {
        let environnement = BTreeMap::from([
            ("ESTOMPE_SILLON_URL", "http://localhost:4010/"),
            ("ESTOMPE_SABLIER_URL", ""),
            ("ESTOMPE_REGIE_URL", "ftp://regie"),
        ]);
        let lire = |nom: &str| environnement.get(nom).map(|v| v.to_string());
        assert_eq!(
            adresses_sections(lire, true),
            [
                (
                    Section::Entracte,
                    "https://entracte-api.plbls.fr".to_string()
                ),
                (Section::Sillon, "http://localhost:4010".to_string()),
            ]
        );
        assert_eq!(
            adresses_sections(lire, false),
            [(Section::Sillon, "http://localhost:4010".to_string())]
        );
        assert_eq!(adresses_sections(|_| None, false), []);
    }

    #[tokio::test]
    async fn n_ouvre_que_les_sections_dont_le_me_l_autorise() {
        let ouverte = ServeurFactice::demarrer([Reponse::json(
            200,
            json!({"permissions": {"access": true, "admin": false}}),
        )])
        .await;
        let fermee = ServeurFactice::demarrer([Reponse::json(
            200,
            json!({"permissions": {"access": false, "admin": false}}),
        )])
        .await;
        let en_panne = ServeurFactice::demarrer([Reponse::vide(500)]).await;
        let adresses = [
            (Section::Entracte, ouverte.adresse().to_string()),
            (Section::Sillon, fermee.adresse().to_string()),
            (Section::Sablier, en_panne.adresse().to_string()),
            (Section::Regie, "http://127.0.0.1:1".to_string()),
        ];
        let ouvertes = sections_ouvertes(&adresses, |adresse| {
            Ok(Client::avec_reglages(adresse, ServeurFactice::reglages())?
                .avec_jeton(|| Some("jeton".to_string())))
        })
        .await;
        assert_eq!(ouvertes, [Section::Entracte]);
        let requete = &ouverte.requetes()[0];
        assert_eq!(requete.ligne(), ("/me", "GET"));
        assert_eq!(requete.entete("authorization"), Some("Bearer jeton"));
    }
}
