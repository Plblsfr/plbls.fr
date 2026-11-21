use std::collections::BTreeSet;

use super::etat::{id_de, salon_depuis, trier_salons, CleConversation, Etat, ParticipantVocal};
use super::messages::ChangementMessages;
use crate::temps_reel::evenements::{
    Evenement, GenreDepart, MentionRecue, NotificationServeur, SourcePartage, StatutPresence,
};
use crate::types::{Mention, Message};

#[derive(Debug, Clone, PartialEq)]
pub enum Changement {
    Compte,
    ServeurAjoute {
        serveur: String,
    },
    ServeurRetire {
        serveur: String,
    },
    Serveur {
        serveur: String,
    },
    OrdreServeurs,
    Roles {
        serveur: String,
    },
    SalonAjoute {
        serveur: String,
        salon: String,
    },
    SalonRetire {
        serveur: String,
        salon: String,
    },
    Salon {
        serveur: String,
        salon: String,
    },
    OrdreSalons {
        serveur: String,
    },
    MembreAjoute {
        serveur: String,
        utilisateur: String,
    },
    MembreRetire {
        serveur: String,
        utilisateur: String,
    },
    Membre {
        serveur: String,
        utilisateur: String,
    },
    Presence {
        utilisateur: String,
    },
    MessagesPrives,
    NonLus {
        conversation: CleConversation,
        serveur: Option<String>,
    },
    Mentions {
        conversation: CleConversation,
        serveur: Option<String>,
    },
    ReglagesNotification,
    Vocal {
        salon: String,
    },
    DeplacementVocal {
        de: String,
        vers: String,
        par: String,
        nom: String,
    },
    Avatar {
        utilisateur: String,
        serveur: Option<String>,
    },
    ConversationCourante,
    NotificationsLues {
        conversation: CleConversation,
    },
    Notification(Box<NotificationServeur>),
    Avis(Avis),
    Messages(ChangementMessages),
    Emojis {
        serveur: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Avis {
    Expulsion {
        serveur: Option<String>,
        raison: Option<String>,
    },
    Bannissement {
        serveur: Option<String>,
        raison: Option<String>,
    },
    Exclusion {
        raison: Option<String>,
    },
    ExclusionLevee,
    Avertissement {
        raison: Option<String>,
    },
    DeconnexionVocale,
}

fn avec_raison(texte: String, raison: &Option<String>) -> String {
    match raison.as_deref().filter(|r| !r.is_empty()) {
        Some(raison) => format!("{texte} : {raison}"),
        None => texte,
    }
}

impl Avis {
    pub fn texte(&self) -> String {
        let depart = |verbe: &str, serveur: &Option<String>, raison: &Option<String>| {
            let nom = serveur.as_deref().unwrap_or("ce serveur");
            avec_raison(format!("Vous avez été {verbe} de « {nom} »"), raison)
        };
        match self {
            Self::Expulsion { serveur, raison } => depart("expulsé", serveur, raison),
            Self::Bannissement { serveur, raison } => depart("banni", serveur, raison),
            Self::Exclusion { raison } => {
                avec_raison("Vous êtes exclu temporairement".to_string(), raison)
            }
            Self::ExclusionLevee => "Votre exclusion temporaire est levée".to_string(),
            Self::Avertissement { raison } => {
                avec_raison("Avertissement de la modération".to_string(), raison)
            }
            Self::DeconnexionVocale => {
                "Vous avez été déconnecté du vocal par la modération".to_string()
            }
        }
    }

    pub fn est_un_echec(&self) -> bool {
        !matches!(self, Self::ExclusionLevee | Self::DeconnexionVocale)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effet {
    RechargerServeurs,
    RechargerServeur(String),
    RechargerMessagesPrives,
    MarquerLu(CleConversation),
    LireNotifications(Vec<String>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Application {
    pub changements: Vec<Changement>,
    pub effets: Vec<Effet>,
}

impl Application {
    pub(crate) fn changer(&mut self, changement: Changement) {
        if !self.changements.contains(&changement) {
            self.changements.push(changement);
        }
    }

    pub(crate) fn effet(&mut self, effet: Effet) {
        if !self.effets.contains(&effet) {
            self.effets.push(effet);
        }
    }
}

pub fn appliquer(etat: &mut Etat, evenement: &Evenement) -> Vec<Changement> {
    appliquer_avec_effets(etat, evenement).changements
}

pub fn appliquer_avec_effets(etat: &mut Etat, evenement: &Evenement) -> Application {
    let mut sortie = Application::default();
    let a = &mut sortie;
    match evenement {
        Evenement::InstantanePresence(d) => {
            let avant: BTreeSet<String> = etat
                .en_ligne
                .iter()
                .chain(etat.activites.keys())
                .cloned()
                .collect();
            let anciennes = std::mem::take(&mut etat.activites);
            let anciens = std::mem::take(&mut etat.en_ligne);
            etat.en_ligne = d.online.iter().cloned().collect();
            etat.activites = d.activities.clone();
            let touches: BTreeSet<&String> = avant
                .iter()
                .chain(etat.en_ligne.iter())
                .chain(etat.activites.keys())
                .collect();
            for utilisateur in touches {
                if anciens.contains(utilisateur) != etat.en_ligne.contains(utilisateur)
                    || anciennes.get(utilisateur) != etat.activites.get(utilisateur)
                {
                    a.changer(Changement::Presence {
                        utilisateur: utilisateur.clone(),
                    });
                }
            }
        }
        Evenement::AvatarModifie(d) => {
            if mettre_version(&mut etat.avatars, &d.user_id, d.version) {
                a.changer(Changement::Avatar {
                    utilisateur: d.user_id.clone(),
                    serveur: None,
                });
            }
        }
        Evenement::AvatarServeurModifie(d) => {
            let versions = etat.avatars_serveur.entry(d.server_id.clone()).or_default();
            let change = mettre_version(versions, &d.user_id, d.version);
            if versions.is_empty() {
                etat.avatars_serveur.remove(&d.server_id);
            }
            if change {
                a.changer(Changement::Avatar {
                    utilisateur: d.user_id.clone(),
                    serveur: Some(d.server_id.clone()),
                });
            }
        }
        Evenement::LectureModifiee(d) => {
            if d.user_id == etat.moi() {
                if let Some(cle) = CleConversation::de(d.channel_id.as_deref(), d.dm_id.as_deref())
                {
                    retirer_non_lus(etat, &cle, a);
                    retirer_mentions(etat, a, |m| {
                        CleConversation::de_mention(m).as_ref() == Some(&cle)
                    });
                }
            }
        }
        Evenement::StatutModifie(d) => {
            let avant = etat.statuts.get(&d.user_id).copied();
            match d.status {
                None | Some(StatutPresence::Online) => {
                    etat.statuts.remove(&d.user_id);
                }
                Some(statut) => {
                    etat.statuts.insert(d.user_id.clone(), statut);
                }
            }
            if avant != etat.statuts.get(&d.user_id).copied() {
                a.changer(Changement::Presence {
                    utilisateur: d.user_id.clone(),
                });
            }
        }
        Evenement::ActiviteModifiee(d) => {
            let avant = match d.name.as_deref().filter(|n| !n.is_empty()) {
                Some(nom) => etat.activites.insert(d.user_id.clone(), nom.to_string()),
                None => etat.activites.remove(&d.user_id),
            };
            if avant.as_ref() != etat.activites.get(&d.user_id) {
                a.changer(Changement::Presence {
                    utilisateur: d.user_id.clone(),
                });
            }
        }
        Evenement::ReglagesNotificationModifies(reglages) => {
            if etat.reglages_notification != *reglages {
                etat.reglages_notification = reglages.clone();
                a.changer(Changement::ReglagesNotification);
            }
        }
        Evenement::Notification(d) => a.changer(Changement::Notification(Box::new(d.clone()))),
        Evenement::Presence(d) => {
            let change = if d.online {
                etat.en_ligne.insert(d.user_id.clone())
            } else {
                let parti = etat.en_ligne.remove(&d.user_id);
                etat.activites.remove(&d.user_id).is_some() || parti
            };
            if change {
                a.changer(Changement::Presence {
                    utilisateur: d.user_id.clone(),
                });
            }
        }
        Evenement::MessageCree(m) => message_cree(etat, m, a),
        Evenement::MessageSupprime(d) => {
            retirer_mentions(etat, a, |m| m.message_id.as_deref() == Some(d.id.as_str()));
        }
        Evenement::MessagesSupprimes(d) => {
            retirer_mentions(etat, a, |m| {
                m.message_id.as_ref().is_some_and(|id| d.ids.contains(id))
            });
        }
        Evenement::Mention(m) => mention(etat, m, a),
        Evenement::SalonCree(d) => {
            let Some(detail) = etat.details.get_mut(&d.server_id) else {
                return sortie;
            };
            let salons = detail.channels.get_or_insert_with(Vec::new);
            let id = id_de(&d.channel.id).to_string();
            if id.is_empty() || salons.iter().any(|c| id_de(&c.id) == id) {
                return sortie;
            }
            let mut salon = salon_depuis(&d.channel);
            let reference = d.after.as_deref().and_then(|apres| {
                salons
                    .iter()
                    .find(|c| id_de(&c.id) == apres && c.kind == salon.kind)
            });
            if let Some(reference) = reference {
                salon.position = reference.position;
            }
            salons.push(salon);
            trier_salons(salons);
            a.changer(Changement::SalonAjoute {
                serveur: d.server_id.clone(),
                salon: id,
            });
        }
        Evenement::SalonModifie(d) => {
            let salon = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.channels.as_mut())
                .and_then(|c| c.iter_mut().find(|c| id_de(&c.id) == d.id));
            if let Some(salon) = salon {
                if salon.name.as_deref() != Some(d.name.as_str()) {
                    salon.name = Some(d.name.clone());
                    a.changer(Changement::Salon {
                        serveur: d.server_id.clone(),
                        salon: d.id.clone(),
                    });
                }
            }
        }
        Evenement::SalonsReordonnes(d) => {
            let Some(salons) = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.channels.as_mut())
            else {
                return sortie;
            };
            let avant = salons.clone();
            for (i, id) in d.ids.iter().enumerate() {
                if let Some(c) = salons.iter_mut().find(|c| id_de(&c.id) == id) {
                    c.position = Some(i as i64 + 1);
                }
            }
            trier_salons(salons);
            if *salons != avant {
                a.changer(Changement::OrdreSalons {
                    serveur: d.server_id.clone(),
                });
            }
        }
        Evenement::BanniereModifiee(d) => {
            for (serveur, detail) in &mut etat.details {
                let membre = detail
                    .members
                    .as_mut()
                    .and_then(|m| m.iter_mut().find(|m| id_de(&m.id) == d.user_id));
                if let Some(membre) = membre {
                    if membre.banner_color != d.color {
                        membre.banner_color = d.color.clone();
                        a.changer(Changement::Membre {
                            serveur: serveur.clone(),
                            utilisateur: d.user_id.clone(),
                        });
                    }
                }
            }
        }
        Evenement::SalonSupprime(d) => {
            let salons = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.channels.as_mut());
            if let Some(salons) = salons {
                let avant = salons.len();
                salons.retain(|c| id_de(&c.id) != d.id);
                if salons.len() != avant {
                    a.changer(Changement::SalonRetire {
                        serveur: d.server_id.clone(),
                        salon: d.id.clone(),
                    });
                }
            }
            oublier_salon(etat, &d.id, Some(&d.server_id), a);
        }
        Evenement::ServeurModifie(d) => {
            let mut change = false;
            if let Some(s) = etat.serveurs.iter_mut().find(|s| id_de(&s.id) == d.id) {
                change |= s.name.as_deref() != Some(d.name.as_str());
                s.name = Some(d.name.clone());
            }
            if let Some(s) = etat
                .details
                .get_mut(&d.id)
                .and_then(|det| det.server.as_mut())
            {
                change |= s.name.as_deref() != Some(d.name.as_str());
                s.name = Some(d.name.clone());
            }
            if change {
                a.changer(Changement::Serveur {
                    serveur: d.id.clone(),
                });
            }
        }
        Evenement::IconeServeurModifiee(d) => {
            let mut change = false;
            if let Some(s) = etat.serveurs.iter_mut().find(|s| id_de(&s.id) == d.id) {
                change |= s.icon_version != Some(d.version);
                s.icon_version = Some(d.version);
            }
            if let Some(s) = etat
                .details
                .get_mut(&d.id)
                .and_then(|det| det.server.as_mut())
            {
                change |= s.icon_version != Some(d.version);
                s.icon_version = Some(d.version);
            }
            if change {
                a.changer(Changement::Serveur {
                    serveur: d.id.clone(),
                });
            }
        }
        Evenement::ServeurSupprime(d) => oublier_serveur(etat, &d.id, a),
        Evenement::MembreArrive(d) => {
            let id = id_de(&d.member.id).to_string();
            if d.member.online == Some(true) && etat.en_ligne.insert(id.clone()) {
                a.changer(Changement::Presence {
                    utilisateur: id.clone(),
                });
            }
            let en_ligne = etat.en_ligne.contains(&id);
            let moi = id == etat.moi();
            match etat.details.get_mut(&d.server_id) {
                Some(detail) => {
                    let membres = detail.members.get_or_insert_with(Vec::new);
                    if !membres.iter().any(|m| id_de(&m.id) == id) {
                        let mut membre = d.member.clone();
                        membre.online = Some(en_ligne);
                        membres.push(membre);
                        a.changer(Changement::MembreAjoute {
                            serveur: d.server_id.clone(),
                            utilisateur: id,
                        });
                    }
                }
                None if moi => a.effet(Effet::RechargerServeurs),
                None => {}
            }
        }
        Evenement::MembreParti(d) => {
            if d.user_id == etat.moi() {
                let serveur = etat.serveur(&d.server_id).and_then(|s| s.name.clone());
                let raison = d.reason.clone();
                match d.kind {
                    Some(GenreDepart::Bannissement) => {
                        a.changer(Changement::Avis(Avis::Bannissement { serveur, raison }))
                    }
                    Some(GenreDepart::Expulsion) => {
                        a.changer(Changement::Avis(Avis::Expulsion { serveur, raison }))
                    }
                    Some(GenreDepart::Inconnu) | None => {}
                }
                oublier_serveur(etat, &d.server_id, a);
            } else if let Some(membres) = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.members.as_mut())
            {
                let avant = membres.len();
                membres.retain(|m| id_de(&m.id) != d.user_id);
                if membres.len() != avant {
                    a.changer(Changement::MembreRetire {
                        serveur: d.server_id.clone(),
                        utilisateur: d.user_id.clone(),
                    });
                }
            }
        }
        Evenement::RolesModifies(d) => {
            if etat.details.contains_key(&d.server_id) {
                a.effet(Effet::RechargerServeur(d.server_id.clone()));
            }
        }
        Evenement::MembreExclu(d) => {
            let membre = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.members.as_mut())
                .and_then(|m| m.iter_mut().find(|m| id_de(&m.id) == d.user_id));
            if let Some(membre) = membre {
                if membre.timeout_until != d.until {
                    membre.timeout_until = d.until.clone();
                    a.changer(Changement::Membre {
                        serveur: d.server_id.clone(),
                        utilisateur: d.user_id.clone(),
                    });
                }
            }
            if d.user_id == etat.moi() {
                a.effet(Effet::RechargerServeur(d.server_id.clone()));
                a.changer(Changement::Avis(match d.until {
                    Some(_) => Avis::Exclusion {
                        raison: d.reason.clone(),
                    },
                    None => Avis::ExclusionLevee,
                }));
            }
        }
        Evenement::MembreAverti(d) => a.changer(Changement::Avis(Avis::Avertissement {
            raison: d.reason.clone(),
        })),
        Evenement::PseudoModifie(d) => {
            let membre = etat
                .details
                .get_mut(&d.server_id)
                .and_then(|det| det.members.as_mut())
                .and_then(|m| m.iter_mut().find(|m| id_de(&m.id) == d.user_id));
            if let Some(membre) = membre {
                if membre.nickname != d.nickname {
                    membre.nickname = d.nickname.clone();
                    a.changer(Changement::Membre {
                        serveur: d.server_id.clone(),
                        utilisateur: d.user_id.clone(),
                    });
                }
            }
        }
        Evenement::VocalDeconnecte(_) => a.changer(Changement::Avis(Avis::DeconnexionVocale)),
        Evenement::VocalDeplace(d) => a.changer(Changement::DeplacementVocal {
            de: d.from.clone(),
            vers: d.to.clone(),
            par: d.by.clone(),
            nom: d.to_name.clone(),
        }),
        Evenement::EtatVocal(d) => {
            let avant = etat.vocal.get(&d.channel_id).cloned().unwrap_or_default();
            let mut liste: Vec<ParticipantVocal> = avant
                .iter()
                .filter(|p| p.utilisateur != d.user_id)
                .cloned()
                .collect();
            if d.joined {
                liste.push(ParticipantVocal {
                    utilisateur: d.user_id.clone(),
                    pseudo: d.pseudo.clone(),
                    depuis: d.at.clone(),
                    ..ParticipantVocal::default()
                });
            }
            if liste != avant {
                if liste.is_empty() {
                    etat.vocal.remove(&d.channel_id);
                } else {
                    etat.vocal.insert(d.channel_id.clone(), liste);
                }
                a.changer(Changement::Vocal {
                    salon: d.channel_id.clone(),
                });
            }
        }
        Evenement::PartageVocal(d) => {
            let participant = etat
                .vocal
                .get_mut(&d.channel_id)
                .and_then(|l| l.iter_mut().find(|p| p.utilisateur == d.user_id));
            if let Some(p) = participant {
                let drapeau = match d.source {
                    Some(SourcePartage::Ecran) => &mut p.ecran,
                    Some(SourcePartage::Camera) => &mut p.camera,
                    Some(SourcePartage::Inconnu) | None => return sortie,
                };
                if *drapeau != d.on {
                    *drapeau = d.on;
                    a.changer(Changement::Vocal {
                        salon: d.channel_id.clone(),
                    });
                }
            }
        }
        Evenement::ModerationVocale(d) => {
            let participant = etat
                .vocal
                .get_mut(&d.channel_id)
                .and_then(|l| l.iter_mut().find(|p| p.utilisateur == d.user_id));
            if let Some(p) = participant {
                if (p.micro_coupe, p.son_coupe) != (d.mic_off, d.sound_off) {
                    p.micro_coupe = d.mic_off;
                    p.son_coupe = d.sound_off;
                    a.changer(Changement::Vocal {
                        salon: d.channel_id.clone(),
                    });
                }
            }
        }
        Evenement::EmojisModifies(e) => a.changer(Changement::Emojis {
            serveur: e.server_id.clone(),
        }),
        Evenement::Pret(_)
        | Evenement::Saisie(_)
        | Evenement::MessageModifie(_)
        | Evenement::SondageVote(_)
        | Evenement::SondageClos(_)
        | Evenement::ReactionModifiee(_)
        | Evenement::SeanceRegarder(_)
        | Evenement::SeanceCinema(_)
        | Evenement::SeanceMusique(_)
        | Evenement::PartieDessin(_)
        | Evenement::DessinPrive(_)
        | Evenement::ChatDessin(_)
        | Evenement::OperationsDessin(_)
        | Evenement::VideosModifiees(_)
        | Evenement::Pong
        | Evenement::Renouvele => {}
    }
    sortie
}

fn mettre_version(
    versions: &mut std::collections::BTreeMap<String, i64>,
    utilisateur: &str,
    version: i64,
) -> bool {
    let avant = if version > 0 {
        versions.insert(utilisateur.to_string(), version)
    } else {
        versions.remove(utilisateur)
    };
    avant != versions.get(utilisateur).copied()
}

pub fn ouvrir_conversation(etat: &mut Etat, conversation: Option<CleConversation>) -> Application {
    let mut a = Application::default();
    if etat.courante != conversation {
        etat.courante = conversation.clone();
        etat.en_bas = true;
        a.changer(Changement::ConversationCourante);
    }
    if let Some(cle) = conversation {
        lire_mentions_et_non_lus(etat, &cle, &mut a);
        if etat.focus {
            marquer_lu(&cle, &mut a);
        }
    }
    a
}

pub fn lire_conversation(etat: &mut Etat, conversation: &CleConversation) -> Application {
    let mut a = Application::default();
    lire_mentions_et_non_lus(etat, conversation, &mut a);
    marquer_lu(conversation, &mut a);
    a
}

pub fn definir_focus(etat: &mut Etat, focus: bool) -> Application {
    let mut a = Application::default();
    etat.focus = focus;
    if focus {
        lecture_effective(etat, &mut a);
    }
    a
}

pub fn definir_bas_de_liste(etat: &mut Etat, en_bas: bool) -> Application {
    let mut a = Application::default();
    let avant = etat.en_bas;
    etat.en_bas = en_bas;
    if en_bas && !avant {
        lecture_effective(etat, &mut a);
    }
    a
}

pub fn decision_lecture(etat: &Etat) -> Option<CleConversation> {
    etat.courante.clone().filter(|_| etat.focus && etat.en_bas)
}

fn lecture_effective(etat: &mut Etat, a: &mut Application) {
    if let Some(cle) = decision_lecture(etat) {
        lire_mentions_et_non_lus(etat, &cle, a);
        marquer_lu(&cle, a);
    }
}

fn marquer_lu(conversation: &CleConversation, a: &mut Application) {
    a.changer(Changement::NotificationsLues {
        conversation: conversation.clone(),
    });
    a.effet(Effet::MarquerLu(conversation.clone()));
}

fn lire_mentions_et_non_lus(etat: &mut Etat, cle: &CleConversation, a: &mut Application) {
    retirer_non_lus(etat, cle, a);
    let ids = retirer_mentions(etat, a, |m| {
        CleConversation::de_mention(m).as_ref() == Some(cle)
    });
    if !ids.is_empty() {
        a.effet(Effet::LireNotifications(ids));
    }
}

fn retirer_non_lus(etat: &mut Etat, cle: &CleConversation, a: &mut Application) {
    if etat.non_lus.remove(cle).is_some() {
        a.changer(Changement::NonLus {
            conversation: cle.clone(),
            serveur: etat.serveur_de(cle),
        });
    }
}

pub(crate) fn retirer_mentions(
    etat: &mut Etat,
    a: &mut Application,
    critere: impl Fn(&Mention) -> bool,
) -> Vec<String> {
    let (retirees, gardees): (Vec<Mention>, Vec<Mention>) = std::mem::take(&mut etat.mentions)
        .into_iter()
        .partition(|m| critere(m));
    etat.mentions = gardees;
    for m in &retirees {
        if let Some(conversation) = CleConversation::de_mention(m) {
            a.changer(Changement::Mentions {
                conversation,
                serveur: m.server_id.clone(),
            });
        }
    }
    retirees.into_iter().filter_map(|m| m.message_id).collect()
}

fn message_cree(etat: &mut Etat, m: &Message, a: &mut Application) {
    let Some(cle) = CleConversation::de(m.channel_id.as_deref(), m.dm_id.as_deref()) else {
        return;
    };
    if let CleConversation::Prive(id) = &cle {
        if !etat.messages_prives.iter().any(|d| id_de(&d.id) == id) {
            a.effet(Effet::RechargerMessagesPrives);
        }
    }
    let auteur = m.author.as_ref().map(|p| id_de(&p.id)).unwrap_or_default();
    if auteur == etat.moi() {
        return;
    }
    if decision_lecture(etat).as_ref() == Some(&cle) {
        marquer_lu(&cle, a);
    } else {
        *etat.non_lus.entry(cle.clone()).or_insert(0) += 1;
        a.changer(Changement::NonLus {
            serveur: etat.serveur_de(&cle),
            conversation: cle,
        });
    }
}

fn mention(etat: &mut Etat, m: &MentionRecue, a: &mut Application) {
    let message = &m.message;
    let Some(id) = message.id.clone() else {
        return;
    };
    if etat
        .mentions
        .iter()
        .any(|x| x.message_id.as_deref() == Some(id.as_str()))
    {
        return;
    }
    let Some(cle) = CleConversation::de(message.channel_id.as_deref(), message.dm_id.as_deref())
    else {
        return;
    };
    if etat.courante.as_ref() == Some(&cle) {
        a.effet(Effet::LireNotifications(vec![id]));
        return;
    }
    let serveur = etat.serveur_de(&cle);
    etat.mentions.insert(
        0,
        Mention {
            message_id: Some(id),
            channel_id: message.channel_id.clone(),
            server_id: serveur.clone(),
            dm_id: message.dm_id.clone(),
            author: message.author.clone(),
            content: message.content.clone(),
            created_at: message.created_at.clone(),
            group: Some(m.group_mention),
        },
    );
    a.changer(Changement::Mentions {
        conversation: cle,
        serveur,
    });
}

pub(crate) fn oublier_salon(
    etat: &mut Etat,
    salon: &str,
    serveur: Option<&str>,
    a: &mut Application,
) {
    let cle = CleConversation::Salon(salon.to_string());
    if etat.non_lus.remove(&cle).is_some() {
        a.changer(Changement::NonLus {
            conversation: cle,
            serveur: serveur.map(str::to_string),
        });
    }
    if etat.vocal.remove(salon).is_some() {
        a.changer(Changement::Vocal {
            salon: salon.to_string(),
        });
    }
}

pub(crate) fn oublier_serveur(etat: &mut Etat, serveur: &str, a: &mut Application) {
    let avant = etat.serveurs.len();
    etat.serveurs.retain(|s| id_de(&s.id) != serveur);
    let detail = etat.details.remove(serveur);
    etat.avatars_serveur.remove(serveur);
    if etat.serveurs.len() != avant || detail.is_some() {
        a.changer(Changement::ServeurRetire {
            serveur: serveur.to_string(),
        });
    }
    let salons = detail.and_then(|d| d.channels).unwrap_or_default();
    for salon in salons {
        oublier_salon(etat, id_de(&salon.id), Some(serveur), a);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::essais::{depuis_json, etat_de_depart, evenement, message};
    use crate::store::etat::{est_vocal, Presence};

    fn cle(texte: &str) -> CleConversation {
        CleConversation::analyser(texte).unwrap()
    }

    fn ev(etat: &mut Etat, type_evenement: &str, data: serde_json::Value) -> Application {
        appliquer_avec_effets(etat, &evenement(type_evenement, data))
    }

    fn ids_salons(etat: &Etat) -> Vec<String> {
        etat.salons("s1")
            .iter()
            .map(|c| id_de(&c.id).to_string())
            .collect()
    }

    fn ids_membres(etat: &Etat) -> Vec<String> {
        etat.membres("s1")
            .iter()
            .map(|m| id_de(&m.id).to_string())
            .collect()
    }

    #[test]
    fn pastille_d_une_conversation_jamais_ouverte() {
        let mut s = etat_de_depart();
        ev(&mut s, "message.created", message("1", "c2", "bob"));
        assert_eq!(s.non_lus_de(&cle("c:c2")), 1);
    }

    #[test]
    fn non_lu_seulement_hors_de_la_conversation_ouverte_et_jamais_ses_propres_messages() {
        let mut s = etat_de_depart();
        s.courante = Some(cle("c:c1"));
        let a = ev(&mut s, "message.created", message("1", "c1", "bob"));
        assert_eq!(s.non_lus.get(&cle("c:c1")), None);
        assert_eq!(a.effets, [Effet::MarquerLu(cle("c:c1"))]);
        s.courante = None;
        ev(&mut s, "message.created", message("2", "c1", "me"));
        assert_eq!(s.non_lus.get(&cle("c:c1")), None);
        let a = ev(&mut s, "message.created", message("3", "c1", "bob"));
        assert_eq!(s.non_lus_de(&cle("c:c1")), 1);
        assert_eq!(
            a.changements,
            [Changement::NonLus {
                conversation: cle("c:c1"),
                serveur: Some("s1".into())
            }]
        );
    }

    #[test]
    fn sans_le_focus_la_conversation_ouverte_n_est_pas_marquee_lue() {
        let mut s = etat_de_depart();
        s.courante = Some(cle("c:c1"));
        s.focus = false;
        let a = ev(&mut s, "message.created", message("1", "c1", "bob"));
        assert!(a.effets.is_empty());
        assert_eq!(s.non_lus_de(&cle("c:c1")), 1);
    }

    #[test]
    fn un_message_recu_fenetre_inactive_reste_non_lu_jusqu_au_retour_en_bas_de_liste() {
        let mut s = etat_de_depart();
        ouvrir_conversation(&mut s, Some(cle("c:c1")));
        definir_focus(&mut s, false);
        let a = ev(&mut s, "message.created", message("1", "c1", "bob"));
        assert!(a.effets.is_empty());
        assert_eq!(s.non_lus_de(&cle("c:c1")), 1);
        assert_eq!(decision_lecture(&s), None);
        definir_bas_de_liste(&mut s, false);
        assert!(definir_focus(&mut s, true).effets.is_empty());
        assert_eq!(s.non_lus_de(&cle("c:c1")), 1);
        let a = ev(&mut s, "message.created", message("2", "c1", "bob"));
        assert!(a.effets.is_empty());
        assert_eq!(s.non_lus_de(&cle("c:c1")), 2);
        let a = definir_bas_de_liste(&mut s, true);
        assert_eq!(a.effets, [Effet::MarquerLu(cle("c:c1"))]);
        assert!(a.changements.contains(&Changement::NonLus {
            conversation: cle("c:c1"),
            serveur: Some("s1".into())
        }));
        assert_eq!(s.non_lus_de(&cle("c:c1")), 0);
        assert_eq!(decision_lecture(&s), Some(cle("c:c1")));
        assert!(definir_bas_de_liste(&mut s, true).effets.is_empty());
        ouvrir_conversation(&mut s, None);
        assert_eq!(decision_lecture(&s), None);
    }

    #[test]
    fn message_d_une_conversation_privee_inconnue_recharge_la_liste() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "message.created",
            json!({"id": "4", "dm_id": "d9", "author": {"id": "bob", "pseudo": "bob"}}),
        );
        assert_eq!(a.effets, [Effet::RechargerMessagesPrives]);
        assert_eq!(s.non_lus_de(&cle("d:d9")), 1);
    }

    #[test]
    fn transmet_la_notification_decidee_par_le_serveur() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "notification",
            json!({"title": "a", "body": "", "tag": "t", "url": "/#/s/s1/c1", "conv": "c:c1", "msg": "1", "kind": "channel", "silent": false}),
        );
        let [Changement::Notification(n)] = a.changements.as_slice() else {
            panic!("{:?}", a.changements)
        };
        assert_eq!((n.tag.as_str(), n.conv.as_deref()), ("t", Some("c:c1")));
    }

    #[test]
    fn applique_les_reglages_de_notifications_recus_d_un_autre_appareil() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "notify_settings.updated",
            json!({"levels": {"s1": "none"}, "types": {"dm": true, "mention": true, "channel": false, "voice": true}, "preview": false, "sound": false}),
        );
        assert_eq!(
            s.reglages_notification.levels.as_ref().unwrap()["s1"],
            crate::types::NotifySettingsLevelsValeur::None
        );
        assert_eq!(s.reglages_notification.preview, Some(false));
        assert_eq!(a.changements, [Changement::ReglagesNotification]);
    }

    #[test]
    fn retire_les_mentions_d_un_message_supprime() {
        let mut s = etat_de_depart();
        s.mentions = vec![depuis_json(
            json!({"message_id": "1", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "", "created_at": ""}),
        )];
        let a = ev(
            &mut s,
            "message.deleted",
            json!({"id": "1", "channel_id": "c1"}),
        );
        assert!(s.mentions.is_empty());
        assert_eq!(
            a.changements,
            [Changement::Mentions {
                conversation: cle("c:c1"),
                serveur: None
            }]
        );
        s.mentions = vec![
            depuis_json(json!({"message_id": "1", "channel_id": "c1"})),
            depuis_json(json!({"message_id": "2", "channel_id": "c1"})),
            depuis_json(json!({"message_id": "3", "channel_id": "c1"})),
        ];
        ev(
            &mut s,
            "messages.deleted",
            json!({"channel_id": "c1", "ids": ["1", "3"]}),
        );
        assert_eq!(s.mentions.len(), 1);
        assert_eq!(s.mentions[0].message_id.as_deref(), Some("2"));
    }

    #[test]
    fn lecture_depuis_un_autre_appareil_efface_pastille_et_mentions() {
        let mut s = etat_de_depart();
        s.non_lus.insert(cle("c:c1"), 3);
        s.mentions = vec![depuis_json(
            json!({"message_id": "5", "channel_id": "c1", "server_id": "s1", "author": {"id": "bob", "pseudo": "bob"}, "content": "yo", "created_at": ""}),
        )];
        let lecture = |u: &str| json!({"channel_id": "c1", "user_id": u, "pseudo": u, "last_id": "5", "read_at": ""});
        let a = ev(&mut s, "read.updated", lecture("bob"));
        assert_eq!(s.non_lus_de(&cle("c:c1")), 3);
        assert!(a.changements.is_empty());
        let a = ev(&mut s, "read.updated", lecture("me"));
        assert_eq!(s.non_lus.get(&cle("c:c1")), None);
        assert_eq!(s.total_mentions(), 0);
        assert!(a.effets.is_empty());
        assert_eq!(
            a.changements,
            [
                Changement::NonLus {
                    conversation: cle("c:c1"),
                    serveur: Some("s1".into())
                },
                Changement::Mentions {
                    conversation: cle("c:c1"),
                    serveur: Some("s1".into())
                }
            ]
        );
    }

    #[test]
    fn lire_ou_ouvrir_retire_la_notification_de_la_plateforme() {
        let mut s = etat_de_depart();
        let a = lire_conversation(&mut s, &cle("c:c1"));
        assert!(a.changements.contains(&Changement::NotificationsLues {
            conversation: cle("c:c1")
        }));
        assert_eq!(a.effets, [Effet::MarquerLu(cle("c:c1"))]);
        let a = ouvrir_conversation(&mut s, Some(cle("c:c1")));
        assert!(a.changements.contains(&Changement::NotificationsLues {
            conversation: cle("c:c1")
        }));
        s.focus = false;
        let a = ouvrir_conversation(&mut s, Some(cle("c:c2")));
        assert_eq!(a.changements, [Changement::ConversationCourante]);
        assert_eq!(
            definir_focus(&mut s, true).effets,
            [Effet::MarquerLu(cle("c:c2"))]
        );
    }

    #[test]
    fn suit_la_presence() {
        let mut s = etat_de_depart();
        let a = ev(&mut s, "presence.snapshot", json!({"online": ["bob"]}));
        assert!(s.en_ligne.contains("bob"));
        assert_eq!(
            a.changements,
            [Changement::Presence {
                utilisateur: "bob".into()
            }]
        );
        ev(
            &mut s,
            "presence",
            json!({"user_id": "bob", "online": false}),
        );
        assert!(!s.en_ligne.contains("bob"));
        assert_eq!(s.presence_de("bob"), Presence::HorsLigne);
        assert_eq!(s.presence_de("me"), Presence::EnLigne);
        ev(
            &mut s,
            "presence",
            json!({"user_id": "bob", "online": true}),
        );
        ev(
            &mut s,
            "status.updated",
            json!({"user_id": "bob", "status": "dnd"}),
        );
        assert_eq!(s.presence_de("bob"), Presence::NePasDeranger);
        let a = ev(
            &mut s,
            "status.updated",
            json!({"user_id": "bob", "status": "online"}),
        );
        assert_eq!(s.presence_de("bob"), Presence::EnLigne);
        assert_eq!(a.changements.len(), 1);
        let a = ev(
            &mut s,
            "status.updated",
            json!({"user_id": "bob", "status": "online"}),
        );
        assert!(a.changements.is_empty());
    }

    #[test]
    fn suit_le_jeu_en_cours_des_autres() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "presence.snapshot",
            json!({"online": ["bob"], "activities": {"bob": "PokeMMO"}}),
        );
        assert_eq!(s.activite_de("bob"), "PokeMMO");
        ev(
            &mut s,
            "activity.updated",
            json!({"user_id": "bob", "name": null}),
        );
        assert_eq!(s.activite_de("bob"), "");
        ev(
            &mut s,
            "activity.updated",
            json!({"user_id": "bob", "name": "Factorio"}),
        );
        ev(
            &mut s,
            "presence",
            json!({"user_id": "bob", "online": false}),
        );
        assert_eq!(s.activite_de("bob"), "");
        s.activite_locale = "Tetris".into();
        assert_eq!(s.activite_de("me"), "Tetris");
    }

    #[test]
    fn affiche_un_arrivant_deja_en_ligne_comme_tel() {
        let mut s = etat_de_depart();
        let arrivee = json!({"server_id": "s1", "member": {"id": "carol", "pseudo": "carol", "online": true}});
        let a = ev(&mut s, "member.joined", arrivee.clone());
        assert_eq!(s.membre("s1", "carol").unwrap().online, Some(true));
        assert!(s.en_ligne.contains("carol"));
        assert_eq!(
            a.changements,
            [
                Changement::Presence {
                    utilisateur: "carol".into()
                },
                Changement::MembreAjoute {
                    serveur: "s1".into(),
                    utilisateur: "carol".into()
                }
            ]
        );
        let a = ev(&mut s, "member.joined", arrivee);
        assert_eq!(ids_membres(&s).iter().filter(|m| *m == "carol").count(), 1);
        assert!(a.changements.is_empty());
    }

    #[test]
    fn arrivee_sur_un_serveur_inconnu_recharge_la_liste_des_serveurs() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "member.joined",
            json!({"server_id": "s9", "member": {"id": "me", "pseudo": "moi"}}),
        );
        assert_eq!(a.effets, [Effet::RechargerServeurs]);
        let a = ev(
            &mut s,
            "member.joined",
            json!({"server_id": "s9", "member": {"id": "bob", "pseudo": "bob"}}),
        );
        assert!(a.effets.is_empty());
    }

    #[test]
    fn retire_un_membre_parti_et_le_serveur_quand_on_le_quitte() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "member.left",
            json!({"server_id": "s1", "user_id": "bob"}),
        );
        assert_eq!(ids_membres(&s), ["me"]);
        let a = ev(
            &mut s,
            "member.left",
            json!({"server_id": "s1", "user_id": "me"}),
        );
        assert!(!s.details.contains_key("s1"));
        assert!(s.serveurs.is_empty());
        assert_eq!(
            a.changements,
            [Changement::ServeurRetire {
                serveur: "s1".into()
            }]
        );
    }

    #[test]
    fn signale_une_expulsion_ou_un_bannissement_avec_la_raison() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "member.left",
            json!({"server_id": "s1", "user_id": "me", "kind": "ban", "reason": "spam", "until": null}),
        );
        let avis = a
            .changements
            .iter()
            .find_map(|c| match c {
                Changement::Avis(avis) => Some(avis.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(avis.texte(), "Vous avez été banni de « Amis » : spam");
        assert!(avis.est_un_echec());
        assert!(!s.details.contains_key("s1"));
        let expulsion = Avis::Expulsion {
            serveur: None,
            raison: None,
        };
        assert_eq!(expulsion.texte(), "Vous avez été expulsé de « ce serveur »");
    }

    #[test]
    fn suit_l_exclusion_temporaire_et_le_pseudo_de_serveur_d_un_membre() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "member.timeout",
            json!({"server_id": "s1", "user_id": "bob", "until": "2099-01-01T00:00:00Z", "reason": ""}),
        );
        assert_eq!(
            s.membre("s1", "bob").unwrap().timeout_until.as_deref(),
            Some("2099-01-01T00:00:00Z")
        );
        assert_eq!(
            a.changements,
            [Changement::Membre {
                serveur: "s1".into(),
                utilisateur: "bob".into()
            }]
        );
        assert!(a.effets.is_empty());
        ev(
            &mut s,
            "member.timeout",
            json!({"server_id": "s1", "user_id": "bob", "until": null, "reason": ""}),
        );
        assert_eq!(s.membre("s1", "bob").unwrap().timeout_until, None);
        ev(
            &mut s,
            "member.nickname",
            json!({"server_id": "s1", "user_id": "bob", "nickname": "Bobby"}),
        );
        assert_eq!(
            s.membre("s1", "bob").unwrap().nickname.as_deref(),
            Some("Bobby")
        );
    }

    #[test]
    fn sa_propre_exclusion_recharge_le_serveur_et_previent() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "member.timeout",
            json!({"server_id": "s1", "user_id": "me", "until": "2099-01-01T00:00:00Z", "reason": "calme"}),
        );
        assert_eq!(a.effets, [Effet::RechargerServeur("s1".into())]);
        assert!(a.changements.contains(&Changement::Avis(Avis::Exclusion {
            raison: Some("calme".into())
        })));
        assert_eq!(
            Avis::Exclusion {
                raison: Some("calme".into())
            }
            .texte(),
            "Vous êtes exclu temporairement : calme"
        );
        let a = ev(
            &mut s,
            "member.timeout",
            json!({"server_id": "s1", "user_id": "me", "until": null}),
        );
        assert!(a
            .changements
            .contains(&Changement::Avis(Avis::ExclusionLevee)));
        let a = ev(
            &mut s,
            "member.warned",
            json!({"server_id": "s1", "reason": "attention"}),
        );
        assert_eq!(
            a.changements,
            [Changement::Avis(Avis::Avertissement {
                raison: Some("attention".into())
            })]
        );
    }

    #[test]
    fn un_salon_cinema_est_un_salon_vocal() {
        use crate::store::etat::GenreSalon as G;
        let vocaux: Vec<G> = [G::Text, G::Voice, G::Videos, G::Music, G::Cinema]
            .into_iter()
            .filter(|g| est_vocal(Some(*g)))
            .collect();
        assert_eq!(vocaux, [G::Voice, G::Cinema]);
    }

    #[test]
    fn gere_les_salons() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "channel.created",
            json!({"server_id": "s1", "channel": {"id": "c9", "name": "nouveau", "kind": "text", "position": 3}}),
        );
        assert_eq!(
            a.changements,
            [Changement::SalonAjoute {
                serveur: "s1".into(),
                salon: "c9".into()
            }]
        );
        ev(
            &mut s,
            "channel.updated",
            json!({"server_id": "s1", "id": "c9", "name": "renommé"}),
        );
        assert_eq!(s.salon("c9").unwrap().1.name.as_deref(), Some("renommé"));
        ev(
            &mut s,
            "channel.deleted",
            json!({"server_id": "s1", "id": "c9"}),
        );
        assert!(s.salon("c9").is_none());
    }

    #[test]
    fn place_un_salon_cree_apres_celui_demande_et_ignore_un_doublon() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "channel.created",
            json!({"server_id": "s1", "channel": {"id": "c2", "name": "deux", "kind": "text", "position": 2}}),
        );
        let trois = json!({"server_id": "s1", "channel": {"id": "c3", "name": "trois", "kind": "text", "position": 3}, "after": "c1"});
        ev(&mut s, "channel.created", trois.clone());
        let a = ev(&mut s, "channel.created", trois);
        assert!(a.changements.is_empty());
        assert_eq!(ids_salons(&s), ["c1", "c3", "c2"]);
    }

    #[test]
    fn reordonne_les_salons_et_suit_la_couleur_de_banniere() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "channel.created",
            json!({"server_id": "s1", "channel": {"id": "c8", "name": "huit", "kind": "text", "position": 3}}),
        );
        let mut textes = ids_salons(&s);
        textes.reverse();
        let a = ev(
            &mut s,
            "channel.reordered",
            json!({"server_id": "s1", "kind": "text", "ids": textes}),
        );
        assert_eq!(ids_salons(&s), textes);
        assert_eq!(
            a.changements,
            [Changement::OrdreSalons {
                serveur: "s1".into()
            }]
        );
        ev(
            &mut s,
            "banner.updated",
            json!({"user_id": "bob", "color": "#aa00ff"}),
        );
        assert_eq!(
            s.membre("s1", "bob").unwrap().banner_color.as_deref(),
            Some("#aa00ff")
        );
    }

    #[test]
    fn transmet_le_deplacement_par_un_moderateur() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "voice.moved",
            json!({"server_id": "s1", "from": "v1", "to": "v2", "by": "eve", "to_name": "Réunion"}),
        );
        assert_eq!(
            a.changements,
            [Changement::DeplacementVocal {
                de: "v1".into(),
                vers: "v2".into(),
                par: "eve".into(),
                nom: "Réunion".into(),
            }]
        );
    }

    #[test]
    fn suit_les_participants_du_vocal() {
        let mut s = etat_de_depart();
        let etat_vocal = |joined: bool| json!({"server_id": "s1", "channel_id": "v1", "user_id": "bob", "pseudo": "bob", "joined": joined});
        ev(&mut s, "voice.state", etat_vocal(true));
        let a = ev(&mut s, "voice.state", etat_vocal(true));
        assert!(a.changements.is_empty());
        assert_eq!(s.participants("v1").len(), 1);
        let a = ev(&mut s, "voice.state", etat_vocal(false));
        assert!(s.participants("v1").is_empty());
        assert_eq!(a.changements, [Changement::Vocal { salon: "v1".into() }]);
    }

    #[test]
    fn retient_l_heure_d_arrivee_et_en_deduit_la_duree_de_presence_du_salon() {
        let mut s = etat_de_depart();
        assert_eq!(s.arrivee_la_plus_ancienne("v1"), None);
        let arrivee = |u: &str, joined: bool, at: &str| json!({"channel_id": "v1", "user_id": u, "pseudo": u, "joined": joined, "at": at});
        ev(
            &mut s,
            "voice.state",
            arrivee("bob", true, "2026-10-21T10:00:10Z"),
        );
        ev(
            &mut s,
            "voice.state",
            arrivee("me", true, "2026-10-21T10:00:30Z"),
        );
        let ms = |d: &str| crate::roles::date_en_millisecondes(d);
        assert_eq!(s.arrivee_la_plus_ancienne("v1"), ms("2026-10-21T10:00:10Z"));
        ev(&mut s, "voice.state", arrivee("bob", false, ""));
        assert_eq!(s.arrivee_la_plus_ancienne("v1"), ms("2026-10-21T10:00:30Z"));
        ev(&mut s, "voice.state", arrivee("me", false, ""));
        assert_eq!(s.arrivee_la_plus_ancienne("v1"), None);
    }

    #[test]
    fn retrouve_l_arrivee_d_un_membre_dans_le_vocal_d_un_serveur() {
        let mut s = etat_de_depart();
        s.details.insert(
            "s1".into(),
            depuis_json(json!({"server": {"id": "s1"}, "channels": [{"id": "v1", "kind": "voice"}], "members": []})),
        );
        assert_eq!(s.arrivee_vocale("s1", "bob"), None);
        ev(
            &mut s,
            "voice.state",
            json!({"server_id": "s1", "channel_id": "v1", "user_id": "bob", "pseudo": "bob", "joined": true, "at": "2026-10-21T10:00:10Z"}),
        );
        assert_eq!(
            s.arrivee_vocale("s1", "bob"),
            crate::roles::date_en_millisecondes("2026-10-21T10:00:10Z")
        );
        assert_eq!(s.arrivee_vocale("s1", "eve"), None);
    }

    fn partage(etat: &mut Etat, source: &str, on: bool) -> Application {
        ev(
            etat,
            "voice.share",
            json!({"server_id": "s1", "channel_id": "v1", "user_id": "bob", "pseudo": "bob", "source": source, "on": on}),
        )
    }

    fn rejoindre_v1(etat: &mut Etat) {
        ev(
            etat,
            "voice.state",
            json!({"channel_id": "v1", "user_id": "bob", "pseudo": "bob", "joined": true}),
        );
    }

    #[test]
    fn suit_les_partages_d_ecran_et_de_camera() {
        let mut s = etat_de_depart();
        rejoindre_v1(&mut s);
        partage(&mut s, "screen", true);
        assert!(s.participants("v1")[0].ecran);
        partage(&mut s, "screen", false);
        assert!(!s.participants("v1")[0].ecran);
    }

    #[test]
    fn retire_l_icone_camera_a_la_coupure_et_la_remet_a_la_reactivation() {
        let mut s = etat_de_depart();
        rejoindre_v1(&mut s);
        partage(&mut s, "camera", true);
        assert!(s.participants("v1")[0].camera);
        partage(&mut s, "camera", false);
        assert!(!s.participants("v1")[0].camera);
        let a = partage(&mut s, "camera", true);
        assert!(s.participants("v1")[0].camera);
        assert_eq!(a.changements, [Changement::Vocal { salon: "v1".into() }]);
    }

    #[test]
    fn affiche_la_coupure_du_micro_ou_du_son_posee_par_le_createur() {
        let mut s = etat_de_depart();
        rejoindre_v1(&mut s);
        let moderation = |micro: bool| json!({"server_id": "s1", "channel_id": "v1", "user_id": "bob", "mic_off": micro, "sound_off": false});
        ev(&mut s, "voice.mod", moderation(true));
        let p = &s.participants("v1")[0];
        assert_eq!((p.micro_coupe, p.son_coupe), (true, false));
        ev(&mut s, "voice.mod", moderation(false));
        let p = &s.participants("v1")[0];
        assert_eq!((p.micro_coupe, p.son_coupe), (false, false));
    }

    #[test]
    fn supprime_un_serveur_et_ses_conversations() {
        let mut s = etat_de_depart();
        s.non_lus.insert(cle("c:c1"), 2);
        let a = ev(&mut s, "server.deleted", json!({"id": "s1"}));
        assert!(s.serveurs.is_empty());
        assert!(s.non_lus.is_empty());
        assert_eq!(
            a.changements,
            [
                Changement::ServeurRetire {
                    serveur: "s1".into()
                },
                Changement::NonLus {
                    conversation: cle("c:c1"),
                    serveur: Some("s1".into())
                }
            ]
        );
    }

    #[test]
    fn compte_les_mentions_par_conversation_et_les_efface_a_l_ouverture() {
        let mut s = etat_de_depart();
        let mention = json!({"id": "7", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "content": "salut", "mentions": ["me"]});
        ev(&mut s, "mention", mention.clone());
        let a = ev(&mut s, "mention", mention);
        assert!(a.changements.is_empty());
        assert_eq!(s.nombre_mentions(&cle("c:c1")), 1);
        assert_eq!(s.mentions_du_serveur("s1"), 1);
        assert_eq!(s.total_mentions(), 1);
        let a = ouvrir_conversation(&mut s, Some(cle("c:c1")));
        assert_eq!(s.nombre_mentions(&cle("c:c1")), 0);
        assert!(a
            .effets
            .contains(&Effet::LireNotifications(vec!["7".into()])));
        let a = ev(
            &mut s,
            "mention",
            json!({"id": "8", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}}),
        );
        assert_eq!(s.nombre_mentions(&cle("c:c1")), 0);
        assert_eq!(a.effets, [Effet::LireNotifications(vec!["8".into()])]);
    }

    #[test]
    fn ne_compte_pas_une_mention_de_groupe_quand_le_serveur_est_ignore() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "mention",
            json!({"id": "8", "channel_id": "c1", "author": {"id": "bob", "pseudo": "bob"}, "mentions": ["me"], "group_mention": true}),
        );
        assert_eq!(s.nombre_mentions(&cle("c:c1")), 1);
        s.reglages_notification.ignore_group = Some(vec!["s1".into()]);
        assert_eq!(s.nombre_mentions(&cle("c:c1")), 0);
        assert_eq!(s.mentions_du_serveur("s1"), 0);
        assert_eq!(s.total_mentions(), 0);
    }

    #[test]
    fn liste_les_pseudos_d_un_salon_pour_l_autocompletion() {
        let s = etat_de_depart();
        let mut pseudos = s.pseudos_de(Some(&cle("c:c1")));
        pseudos.sort();
        assert_eq!(pseudos, ["bob", "moi"]);
        assert!(s.pseudos_de(None).is_empty());
    }

    #[test]
    fn suit_la_photo_propre_au_serveur_sans_toucher_aux_autres_serveurs() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "server_avatar.updated",
            json!({"server_id": "s1", "user_id": "bob", "version": 2}),
        );
        assert_eq!(s.version_avatar_du_serveur("bob", Some("s1")), 2);
        assert_eq!(s.version_avatar_du_serveur("bob", Some("s2")), 0);
        assert_eq!(s.version_avatar_du_serveur("bob", None), 0);
        assert_eq!(
            a.changements,
            [Changement::Avatar {
                utilisateur: "bob".into(),
                serveur: Some("s1".into())
            }]
        );
    }

    #[test]
    fn retombe_sur_la_photo_globale_quand_la_photo_du_serveur_est_retiree() {
        let mut s = etat_de_depart();
        ev(
            &mut s,
            "avatar.updated",
            json!({"user_id": "carl", "version": 1}),
        );
        ev(
            &mut s,
            "server_avatar.updated",
            json!({"server_id": "s1", "user_id": "carl", "version": 1}),
        );
        ev(
            &mut s,
            "server_avatar.updated",
            json!({"server_id": "s1", "user_id": "carl", "version": 0}),
        );
        assert_eq!(s.version_avatar_du_serveur("carl", Some("s1")), 0);
        assert_eq!(s.version_avatar("carl"), 1);
        assert!(s.avatars_serveur.is_empty());
    }

    #[test]
    fn suit_la_version_de_l_icone_dans_la_liste_et_le_detail() {
        let mut s = etat_de_depart();
        let a = ev(
            &mut s,
            "server_icon.updated",
            json!({"id": "s1", "version": 3}),
        );
        assert_eq!(s.serveurs[0].icon_version, Some(3));
        assert_eq!(
            s.details["s1"].server.as_ref().unwrap().icon_version,
            Some(3)
        );
        assert_eq!(
            a.changements,
            [Changement::Serveur {
                serveur: "s1".into()
            }]
        );
        ev(
            &mut s,
            "server_icon.updated",
            json!({"id": "s1", "version": 0}),
        );
        assert_eq!(s.serveurs[0].icon_version, Some(0));
        ev(
            &mut s,
            "server.updated",
            json!({"id": "s1", "name": "Club"}),
        );
        assert_eq!(s.serveurs[0].name.as_deref(), Some("Club"));
        assert_eq!(
            s.details["s1"].server.as_ref().unwrap().name.as_deref(),
            Some("Club")
        );
    }

    #[test]
    fn lit_les_permissions_du_salon_a_defaut_celles_du_serveur() {
        let mut s = etat_de_depart();
        let detail = s.details.get_mut("s1").unwrap();
        detail.permissions = Some(vec!["send_messages".into(), "create_invites".into()]);
        detail.channels.as_mut().unwrap()[0].permissions = Some(vec!["send_messages".into()]);
        assert!(s.peut(Some("s1"), "send_messages", Some("c1")));
        assert!(!s.peut(Some("s1"), "create_invites", Some("c1")));
        assert!(s.peut(Some("s1"), "create_invites", None));
        assert!(!s.peut_dans(Some(&cle("c:c1")), "create_polls"));
        assert!(s.peut_dans(Some(&cle("d:x")), "create_polls"));
        assert!(s.peut(None, "create_polls", None));
    }

    #[test]
    fn ne_propose_les_mentions_de_groupe_qu_avec_la_permission() {
        let mut s = etat_de_depart();
        let detail = s.details.get_mut("s1").unwrap();
        detail.roles = Some(vec![
            depuis_json(
                json!({"id": "r1", "name": "Modérateur", "position": 1, "is_default": false}),
            ),
            depuis_json(
                json!({"id": "e", "name": "@tout le monde", "position": 0, "is_default": true}),
            ),
        ]);
        detail.channels.as_mut().unwrap()[0].permissions = Some(vec![]);
        let c1 = cle("c:c1");
        assert_eq!(
            s.mentions_de_groupe(Some(&c1)),
            ["everyone", "here", "Modérateur"]
        );
        assert!(s.suggestions_de_groupe(Some(&c1)).is_empty());
        s.details.get_mut("s1").unwrap().channels.as_mut().unwrap()[0].permissions =
            Some(vec!["mention_everyone".into()]);
        assert_eq!(
            s.suggestions_de_groupe(Some(&c1)),
            ["everyone", "here", "Modérateur"]
        );
        assert_eq!(s.touches_par_groupe(Some(&c1), "everyone"), Some(1));
        assert_eq!(s.touches_par_groupe(Some(&c1), "here"), Some(0));
        assert_eq!(s.touches_par_groupe(Some(&c1), "Modérateur"), None);
    }

    #[test]
    fn roles_modifies_demandent_le_rechargement_du_serveur_connu() {
        let mut s = etat_de_depart();
        let a = ev(&mut s, "roles.updated", json!({"server_id": "s1"}));
        assert_eq!(
            (a.changements, a.effets),
            (vec![], vec![Effet::RechargerServeur("s1".into())])
        );
        let a = ev(&mut s, "roles.updated", json!({"server_id": "s9"}));
        assert!(a.effets.is_empty());
    }

    #[test]
    fn un_evenement_sur_un_salon_ne_notifie_que_ce_salon() {
        let mut s = etat_de_depart();
        for (id, position) in [("c2", 2), ("c3", 3)] {
            ev(
                &mut s,
                "channel.created",
                json!({"server_id": "s1", "channel": {"id": id, "name": id, "kind": "text", "position": position}}),
            );
        }
        let mut avant = s.clone();
        let a = ev(
            &mut s,
            "channel.updated",
            json!({"server_id": "s1", "id": "c2", "name": "jeux"}),
        );
        assert_eq!(
            a.changements,
            [Changement::Salon {
                serveur: "s1".into(),
                salon: "c2".into()
            }]
        );
        let salon = avant
            .details
            .get_mut("s1")
            .unwrap()
            .channels
            .as_mut()
            .unwrap();
        salon[1].name = Some("jeux".into());
        assert_eq!(s, avant);
        let a = ev(&mut s, "message.created", message("9", "c3", "bob"));
        assert_eq!(
            a.changements,
            [Changement::NonLus {
                conversation: cle("c:c3"),
                serveur: Some("s1".into())
            }]
        );
        let a = ev(
            &mut s,
            "channel.updated",
            json!({"server_id": "s1", "id": "c2", "name": "jeux"}),
        );
        assert!(a.changements.is_empty());
    }

    #[test]
    fn une_liste_d_emojis_modifiee_est_signalee_sans_changer_l_etat() {
        let mut s = etat_de_depart();
        let avant = s.clone();
        let a = ev(&mut s, "emoji.updated", json!({"server_id": "s1"}));
        assert_eq!(
            a.changements,
            [Changement::Emojis {
                serveur: "s1".into()
            }]
        );
        assert_eq!(s, avant);
    }

    #[test]
    fn ignore_sans_paniquer_les_evenements_hors_du_store() {
        let mut s = etat_de_depart();
        let avant = s.clone();
        for (type_evenement, data) in [
            (
                "typing",
                json!({"channel_id": "c1", "user_id": "bob", "pseudo": "bob"}),
            ),
            ("message.updated", message("1", "c1", "bob")),
            (
                "reaction.updated",
                json!({"message_id": "1", "emoji": "👍", "user_id": "bob", "added": true}),
            ),
            (
                "poll.voted",
                json!({"message_id": "1", "user_id": "bob", "options": [], "counts": {}}),
            ),
            ("poll.closed", json!({"message_id": "1", "closes_at": "x"})),
            (
                "watch.state",
                json!({"server_id": "s1", "channel_id": "v1", "state": null}),
            ),
            (
                "music.state",
                json!({"server_id": "s1", "channel_id": "v1", "state": null}),
            ),
            (
                "cinema.state",
                json!({"server_id": "s1", "channel_id": "v1", "state": null}),
            ),
            (
                "draw.state",
                json!({"server_id": "s1", "channel_id": "v1", "state": null, "now": 0}),
            ),
            (
                "draw.private",
                json!({"server_id": "s1", "channel_id": "v1", "rev": 1}),
            ),
            (
                "draw.chat",
                json!({"server_id": "s1", "channel_id": "v1", "from": "u", "pseudo": "u", "text": "", "at": 0}),
            ),
            (
                "draw.ops",
                json!({"server_id": "s1", "channel_id": "v1", "seq": 1, "ops": []}),
            ),
            (
                "videos.changed",
                json!({"server_id": "s1", "channel_id": "c1"}),
            ),
            ("ready", json!({"user_id": "me"})),
            ("pong", json!(null)),
            ("renewed", json!(null)),
        ] {
            let a = ev(&mut s, type_evenement, data);
            assert_eq!(
                (a.changements, a.effets),
                (vec![], vec![]),
                "{type_evenement}"
            );
        }
        assert_eq!(s, avant);
        let a = ev(
            &mut s,
            "voice.disconnected",
            json!({"server_id": "s1", "channel_id": "v1"}),
        );
        assert_eq!(a.changements, [Changement::Avis(Avis::DeconnexionVocale)]);
    }
}
