use std::collections::BTreeMap;
use std::fmt;

use futures_util::future::{join_all, try_join_all};

use super::etat::{id_de, CleConversation, Etat, ParticipantVocal};
use crate::http::{Client, ErreurApi};
use crate::roles::Detail;
use crate::temps_reel::evenements::StatutPresence;
use crate::types::{
    GetMeReponse, GetServersIdVoiceReponse, GetUnreadReponse, Mention, NotifySettings, Server, DM,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurStore {
    Api(ErreurApi),
    CompteInactif,
}

impl From<ErreurApi> for ErreurStore {
    fn from(erreur: ErreurApi) -> Self {
        Self::Api(erreur)
    }
}

impl fmt::Display for ErreurStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(erreur) => write!(f, "{erreur}"),
            Self::CompteInactif => write!(f, "compte non autorisé ou non activé"),
        }
    }
}

impl std::error::Error for ErreurStore {}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesServeur {
    pub detail: Detail,
    pub vocal: Option<GetServersIdVoiceReponse>,
    pub avatars: Option<BTreeMap<String, i64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chargement {
    pub compte: GetMeReponse,
    pub serveurs: Vec<Server>,
    pub donnees: BTreeMap<String, DonneesServeur>,
    pub messages_prives: Vec<DM>,
    pub mentions: Vec<Mention>,
    pub non_lus: GetUnreadReponse,
    pub statuts: Option<BTreeMap<String, StatutPresence>>,
    pub reglages: Option<NotifySettings>,
    pub avatars: Option<BTreeMap<String, i64>>,
}

pub async fn charger_serveur(client: &Client, id: &str) -> Result<DonneesServeur, ErreurApi> {
    let (detail, vocal, avatars) = tokio::join!(
        client.serveur(id),
        client.vocal_du_serveur(id),
        client.versions_avatars_du_serveur(id)
    );
    Ok(DonneesServeur {
        detail: detail?,
        vocal: vocal.ok(),
        avatars: avatars.ok(),
    })
}

pub async fn charger(client: &Client) -> Result<Chargement, ErreurStore> {
    let compte = client.moi().await?;
    if compte.allowed != Some(true) || compte.activated != Some(true) {
        return Err(ErreurStore::CompteInactif);
    }
    let (serveurs, messages_prives, mentions, non_lus) = tokio::try_join!(
        client.serveurs(),
        client.conversations_privees(),
        client.notifications(),
        client.non_lus()
    )?;
    let (statuts, reglages, avatars) = tokio::join!(
        client.statuts(),
        client.reglages_notification(),
        client.versions_avatars()
    );
    let ids: Vec<String> = serveurs
        .iter()
        .map(|s| id_de(&s.id).to_string())
        .filter(|id| !id.is_empty())
        .collect();
    let donnees = try_join_all(ids.iter().map(|id| charger_serveur(client, id))).await?;
    Ok(Chargement {
        compte,
        serveurs,
        donnees: ids.into_iter().zip(donnees).collect(),
        messages_prives,
        mentions,
        non_lus,
        statuts: statuts.ok(),
        reglages: reglages.ok(),
        avatars: avatars.ok(),
    })
}

pub async fn charger_les_serveurs(
    client: &Client,
    ids: &[String],
) -> Vec<(String, Result<DonneesServeur, ErreurApi>)> {
    let resultats = join_all(ids.iter().map(|id| charger_serveur(client, id))).await;
    ids.iter().cloned().zip(resultats).collect()
}

pub(crate) fn participants(
    liste: &[crate::types::GetServersIdVoiceReponseValeurElement],
) -> Vec<ParticipantVocal> {
    liste
        .iter()
        .map(|p| ParticipantVocal {
            utilisateur: id_de(&p.user_id).to_string(),
            pseudo: id_de(&p.pseudo).to_string(),
            depuis: p.since.clone(),
            ..ParticipantVocal::default()
        })
        .collect()
}

pub(crate) fn installer_serveur(
    etat: &mut Etat,
    precedent: &Etat,
    serveur: &str,
    donnees: DonneesServeur,
) {
    let salons: Vec<String> = donnees
        .detail
        .channels
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|c| id_de(&c.id).to_string())
        .collect();
    let anciens_salons: Vec<String> = precedent
        .salons(serveur)
        .iter()
        .map(|c| id_de(&c.id).to_string())
        .collect();
    for salon in &anciens_salons {
        etat.vocal.remove(salon);
    }
    match &donnees.vocal {
        Some(vocal) => {
            for (salon, liste) in vocal {
                if !liste.is_empty() && salons.contains(salon) {
                    etat.vocal.insert(salon.clone(), participants(liste));
                }
            }
        }
        None => {
            for salon in salons.iter().filter(|s| anciens_salons.contains(s)) {
                if let Some(liste) = precedent.vocal.get(salon) {
                    etat.vocal.insert(salon.clone(), liste.clone());
                }
            }
        }
    }
    match donnees.avatars {
        Some(versions) if !versions.is_empty() => {
            etat.avatars_serveur.insert(serveur.to_string(), versions);
        }
        Some(_) => {
            etat.avatars_serveur.remove(serveur);
        }
        None => match precedent.avatars_serveur.get(serveur) {
            Some(versions) => {
                etat.avatars_serveur
                    .insert(serveur.to_string(), versions.clone());
            }
            None => {
                etat.avatars_serveur.remove(serveur);
            }
        },
    }
    for membre in donnees.detail.members.as_deref().unwrap_or_default() {
        if membre.online == Some(true) {
            etat.en_ligne.insert(id_de(&membre.id).to_string());
        }
    }
    etat.details.insert(serveur.to_string(), donnees.detail);
}

pub fn etat_depuis(chargement: Chargement, precedent: &Etat) -> Etat {
    let mut etat = Etat {
        compte: chargement.compte,
        serveurs: chargement.serveurs,
        messages_prives: chargement.messages_prives,
        mentions: chargement.mentions,
        statuts: chargement
            .statuts
            .unwrap_or_else(|| precedent.statuts.clone()),
        activites: precedent.activites.clone(),
        reglages_notification: chargement
            .reglages
            .unwrap_or_else(|| precedent.reglages_notification.clone()),
        avatars: chargement
            .avatars
            .unwrap_or_else(|| precedent.avatars.clone()),
        courante: precedent.courante.clone(),
        focus: precedent.focus,
        en_bas: precedent.en_bas,
        activite_locale: precedent.activite_locale.clone(),
        ordre_serveurs: precedent.ordre_serveurs.clone(),
        seances: precedent.seances.clone(),
        ..Etat::default()
    };
    for dm in &etat.messages_prives {
        if dm.online == Some(true) {
            if let Some(autre) = dm.other.as_ref().and_then(|p| p.id.clone()) {
                etat.en_ligne.insert(autre);
            }
        }
    }
    let comptes = chargement.non_lus.counts.unwrap_or_default();
    let cles = chargement
        .non_lus
        .channels
        .unwrap_or_default()
        .into_iter()
        .map(CleConversation::Salon)
        .chain(
            chargement
                .non_lus
                .dms
                .unwrap_or_default()
                .into_iter()
                .map(CleConversation::Prive),
        );
    for cle in cles {
        if etat.courante.as_ref() == Some(&cle) {
            continue;
        }
        let id = match &cle {
            CleConversation::Salon(id) | CleConversation::Prive(id) => id.clone(),
        };
        etat.non_lus
            .insert(cle, comptes.get(&id).copied().unwrap_or(1));
    }
    for (serveur, donnees) in chargement.donnees {
        installer_serveur(&mut etat, precedent, &serveur, donnees);
    }
    etat
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::essais::{depuis_json, etat_de_depart};

    fn chargement(non_lus: serde_json::Value, vocal: Option<serde_json::Value>) -> Chargement {
        let depart = etat_de_depart();
        let mut donnees = BTreeMap::new();
        donnees.insert(
            "s1".to_string(),
            DonneesServeur {
                detail: depart.details["s1"].clone(),
                vocal: vocal.map(depuis_json),
                avatars: None,
            },
        );
        Chargement {
            compte: depart.compte,
            serveurs: depart.serveurs,
            donnees,
            messages_prives: vec![depuis_json(
                json!({"id": "d1", "other": {"id": "dave", "pseudo": "dave"}, "online": true}),
            )],
            mentions: Vec::new(),
            non_lus: depuis_json(non_lus),
            statuts: None,
            reglages: None,
            avatars: None,
        }
    }

    #[test]
    fn la_conversation_affichee_est_consideree_comme_lue() {
        let precedent = Etat {
            courante: Some(CleConversation::Salon("c1".into())),
            ..Etat::default()
        };
        let etat = etat_depuis(
            chargement(
                json!({"channels": ["c1", "c2"], "dms": ["d1"], "counts": {"c2": 4}}),
                None,
            ),
            &precedent,
        );
        let non_lus: Vec<(String, i64)> = etat
            .non_lus
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();
        assert_eq!(non_lus, [("c:c2".to_string(), 4), ("d:d1".to_string(), 1)]);
        assert_eq!(etat.courante, precedent.courante);
        assert!(etat.en_ligne.contains("dave") && etat.en_ligne.contains("me"));
    }

    #[test]
    fn garde_ce_qui_n_a_pas_pu_etre_relu() {
        let mut precedent = etat_de_depart();
        precedent.statuts.insert("bob".into(), StatutPresence::Idle);
        precedent.vocal.insert(
            "c1".into(),
            vec![ParticipantVocal {
                utilisateur: "bob".into(),
                ..ParticipantVocal::default()
            }],
        );
        precedent.activite_locale = "Tetris".into();
        let etat = etat_depuis(chargement(json!({}), None), &precedent);
        assert_eq!(etat.statuts, precedent.statuts);
        assert_eq!(etat.vocal, precedent.vocal);
        assert_eq!(etat.activite_locale, "Tetris");
        let etat = etat_depuis(chargement(json!({}), Some(json!({"c1": []}))), &precedent);
        assert!(etat.vocal.is_empty());
    }
}
