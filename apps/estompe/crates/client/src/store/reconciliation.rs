use std::collections::{BTreeMap, BTreeSet};

use super::etat::{id_de, CleConversation, Etat};
use super::reducteur::Changement;
use crate::roles::Detail;
use crate::types::Mention;

struct Ecarts {
    ajoutes: Vec<String>,
    retires: Vec<String>,
    modifies: Vec<String>,
    ordre: bool,
}

fn ecarts<T: PartialEq>(ancien: &[T], nouveau: &[T], id: impl Fn(&T) -> &str) -> Ecarts {
    let trouver = |liste: &[T], cle: &str| liste.iter().position(|x| id(x) == cle);
    let ajoutes = nouveau
        .iter()
        .filter(|x| trouver(ancien, id(x)).is_none())
        .map(|x| id(x).to_string())
        .collect();
    let retires = ancien
        .iter()
        .filter(|x| trouver(nouveau, id(x)).is_none())
        .map(|x| id(x).to_string())
        .collect();
    let modifies = nouveau
        .iter()
        .filter(|x| trouver(ancien, id(x)).is_some_and(|i| ancien[i] != **x))
        .map(|x| id(x).to_string())
        .collect();
    let communs = |liste: &[T], autre: &[T]| -> Vec<String> {
        liste
            .iter()
            .filter(|x| trouver(autre, id(x)).is_some())
            .map(|x| id(x).to_string())
            .collect()
    };
    let ordre = communs(ancien, nouveau) != communs(nouveau, ancien);
    Ecarts {
        ajoutes,
        retires,
        modifies,
        ordre,
    }
}

fn cles_differentes<K: Ord + Clone, V: PartialEq>(
    ancien: &BTreeMap<K, V>,
    nouveau: &BTreeMap<K, V>,
) -> Vec<K> {
    ancien
        .keys()
        .chain(nouveau.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| ancien.get(*k) != nouveau.get(*k))
        .cloned()
        .collect()
}

pub(crate) fn differences_detail(
    serveur: &str,
    ancien: Option<&Detail>,
    nouveau: Option<&Detail>,
    sortie: &mut Vec<Changement>,
) {
    let vide = Detail::default();
    let (ancien, nouveau) = (ancien.unwrap_or(&vide), nouveau.unwrap_or(&vide));
    let s = || serveur.to_string();
    if ancien.server != nouveau.server {
        sortie.push(Changement::Serveur { serveur: s() });
    }
    if ancien.roles != nouveau.roles || ancien.permissions != nouveau.permissions {
        sortie.push(Changement::Roles { serveur: s() });
    }
    let salons = ecarts(
        ancien.channels.as_deref().unwrap_or_default(),
        nouveau.channels.as_deref().unwrap_or_default(),
        |c| id_de(&c.id),
    );
    sortie.extend(
        salons
            .retires
            .into_iter()
            .map(|salon| Changement::SalonRetire {
                serveur: s(),
                salon,
            }),
    );
    sortie.extend(
        salons
            .ajoutes
            .into_iter()
            .map(|salon| Changement::SalonAjoute {
                serveur: s(),
                salon,
            }),
    );
    sortie.extend(salons.modifies.into_iter().map(|salon| Changement::Salon {
        serveur: s(),
        salon,
    }));
    if salons.ordre {
        sortie.push(Changement::OrdreSalons { serveur: s() });
    }
    let membres = ecarts(
        ancien.members.as_deref().unwrap_or_default(),
        nouveau.members.as_deref().unwrap_or_default(),
        |m| id_de(&m.id),
    );
    sortie.extend(
        membres
            .retires
            .into_iter()
            .map(|utilisateur| Changement::MembreRetire {
                serveur: s(),
                utilisateur,
            }),
    );
    sortie.extend(
        membres
            .ajoutes
            .into_iter()
            .map(|utilisateur| Changement::MembreAjoute {
                serveur: s(),
                utilisateur,
            }),
    );
    sortie.extend(
        membres
            .modifies
            .into_iter()
            .map(|utilisateur| Changement::Membre {
                serveur: s(),
                utilisateur,
            }),
    );
}

fn mentions_par_conversation(etat: &Etat) -> BTreeMap<CleConversation, Vec<&Mention>> {
    let mut parties: BTreeMap<CleConversation, Vec<&Mention>> = BTreeMap::new();
    for m in &etat.mentions {
        if let Some(cle) = CleConversation::de_mention(m) {
            parties.entry(cle).or_default().push(m);
        }
    }
    parties
}

pub fn reconcilier(ancien: &Etat, nouveau: &Etat) -> Vec<Changement> {
    let mut sortie = Vec::new();
    if ancien.compte != nouveau.compte {
        sortie.push(Changement::Compte);
    }
    let serveurs = ecarts(&ancien.serveurs, &nouveau.serveurs, |s| id_de(&s.id));
    let ajoutes_ou_retires: BTreeSet<String> = serveurs
        .ajoutes
        .iter()
        .chain(&serveurs.retires)
        .cloned()
        .collect();
    sortie.extend(
        serveurs
            .retires
            .iter()
            .map(|serveur| Changement::ServeurRetire {
                serveur: serveur.clone(),
            }),
    );
    sortie.extend(
        serveurs
            .ajoutes
            .iter()
            .map(|serveur| Changement::ServeurAjoute {
                serveur: serveur.clone(),
            }),
    );
    let ordre = |etat: &Etat| -> Vec<String> {
        etat.ids_serveurs_ordonnes()
            .into_iter()
            .filter(|id| !ajoutes_ou_retires.contains(id))
            .collect()
    };
    if ordre(ancien) != ordre(nouveau) {
        sortie.push(Changement::OrdreServeurs);
    }
    let modifies: BTreeSet<&String> = serveurs.modifies.iter().collect();
    for serveur in cles_differentes(&ancien.details, &nouveau.details)
        .iter()
        .chain(serveurs.modifies.iter())
        .collect::<BTreeSet<_>>()
    {
        if ajoutes_ou_retires.contains(serveur) {
            continue;
        }
        let mut ecarts_detail = Vec::new();
        differences_detail(
            serveur,
            ancien.details.get(serveur),
            nouveau.details.get(serveur),
            &mut ecarts_detail,
        );
        if modifies.contains(serveur)
            && !ecarts_detail.contains(&Changement::Serveur {
                serveur: serveur.clone(),
            })
        {
            sortie.push(Changement::Serveur {
                serveur: serveur.clone(),
            });
        }
        sortie.extend(ecarts_detail);
    }
    if ancien.messages_prives != nouveau.messages_prives {
        sortie.push(Changement::MessagesPrives);
    }
    let utilisateurs: BTreeSet<&String> = [ancien, nouveau]
        .iter()
        .flat_map(|e| {
            e.en_ligne
                .iter()
                .chain(e.statuts.keys())
                .chain(e.activites.keys())
        })
        .collect();
    for utilisateur in utilisateurs {
        let trace = |e: &Etat| {
            (
                e.en_ligne.contains(utilisateur),
                e.statuts.get(utilisateur).copied(),
                e.activites.get(utilisateur).cloned(),
            )
        };
        if trace(ancien) != trace(nouveau) {
            sortie.push(Changement::Presence {
                utilisateur: utilisateur.clone(),
            });
        }
    }
    let serveur_de =
        |cle: &CleConversation| nouveau.serveur_de(cle).or_else(|| ancien.serveur_de(cle));
    for conversation in cles_differentes(&ancien.non_lus, &nouveau.non_lus) {
        sortie.push(Changement::NonLus {
            serveur: serveur_de(&conversation),
            conversation,
        });
    }
    let (mentions_avant, mentions_apres) = (
        mentions_par_conversation(ancien),
        mentions_par_conversation(nouveau),
    );
    for conversation in cles_differentes(&mentions_avant, &mentions_apres) {
        let serveur = mentions_apres
            .get(&conversation)
            .or(mentions_avant.get(&conversation))
            .and_then(|l| l.first())
            .and_then(|m| m.server_id.clone());
        sortie.push(Changement::Mentions {
            conversation,
            serveur,
        });
    }
    if ancien.reglages_notification != nouveau.reglages_notification {
        sortie.push(Changement::ReglagesNotification);
    }
    for salon in cles_differentes(&ancien.vocal, &nouveau.vocal) {
        sortie.push(Changement::Vocal { salon });
    }
    for utilisateur in cles_differentes(&ancien.avatars, &nouveau.avatars) {
        sortie.push(Changement::Avatar {
            utilisateur,
            serveur: None,
        });
    }
    let vide = BTreeMap::new();
    for serveur in cles_differentes(&ancien.avatars_serveur, &nouveau.avatars_serveur) {
        let (avant, apres) = (
            ancien.avatars_serveur.get(&serveur).unwrap_or(&vide),
            nouveau.avatars_serveur.get(&serveur).unwrap_or(&vide),
        );
        for utilisateur in cles_differentes(avant, apres) {
            sortie.push(Changement::Avatar {
                utilisateur,
                serveur: Some(serveur.clone()),
            });
        }
    }
    if ancien.courante != nouveau.courante {
        sortie.push(Changement::ConversationCourante);
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::essais::etat_de_depart;

    #[test]
    fn deux_etats_identiques_ne_changent_rien() {
        let etat = etat_de_depart();
        assert!(reconcilier(&etat, &etat.clone()).is_empty());
    }

    #[test]
    fn un_membre_modifie_ne_notifie_que_lui() {
        let ancien = etat_de_depart();
        let mut nouveau = ancien.clone();
        nouveau
            .details
            .get_mut("s1")
            .unwrap()
            .members
            .as_mut()
            .unwrap()[1]
            .nickname = Some("Bobby".into());
        nouveau.ordre_serveurs = vec!["s1".into()];
        assert_eq!(
            reconcilier(&ancien, &nouveau),
            [Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "bob".into()
            }]
        );
    }

    #[test]
    fn un_ordre_de_salons_change_est_signale() {
        let ancien = etat_de_depart();
        let mut nouveau = ancien.clone();
        let salons = nouveau
            .details
            .get_mut("s1")
            .unwrap()
            .channels
            .as_mut()
            .unwrap();
        let mut deux = salons[0].clone();
        deux.id = Some("c2".into());
        salons.insert(0, deux);
        let mut intermediaire = ancien.clone();
        intermediaire.details = nouveau.details.clone();
        assert_eq!(
            reconcilier(&ancien, &intermediaire),
            [Changement::SalonAjoute {
                serveur: "s1".into(),
                salon: "c2".into()
            }]
        );
        nouveau
            .details
            .get_mut("s1")
            .unwrap()
            .channels
            .as_mut()
            .unwrap()
            .reverse();
        assert_eq!(
            reconcilier(&intermediaire, &nouveau),
            [Changement::OrdreSalons {
                serveur: "s1".into()
            }]
        );
    }
}
