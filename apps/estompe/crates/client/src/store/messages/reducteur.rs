use std::collections::BTreeMap;

use tokio::time::Instant;

use super::etat::{
    auteur, comparer_ids, conversation_du_message, id_message, Conversation, EtatMessages,
    SaisieEnCours, DUREE_SAISIE, MESSAGES_PAR_CONVERSATION,
};
use crate::store::etat::{CleConversation, Etat};
use crate::store::reducteur::{Application, Changement};
use crate::temps_reel::evenements::{Lecture, ReactionModifiee, SondageVote};
use crate::temps_reel::Evenement;
use crate::types::{Message, Person, Reaction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangementMessages {
    AjoutesEnFin {
        conversation: CleConversation,
        nombre: usize,
    },
    AjoutesEnTete {
        conversation: CleConversation,
        nombre: usize,
    },
    Insere {
        conversation: CleConversation,
        message: String,
    },
    RetiresEnTete {
        conversation: CleConversation,
        nombre: usize,
    },
    RetiresEnFin {
        conversation: CleConversation,
        nombre: usize,
    },
    Modifie {
        conversation: CleConversation,
        message: String,
    },
    Supprime {
        conversation: CleConversation,
        message: String,
    },
    Bornes {
        conversation: CleConversation,
    },
    Rechargee {
        conversation: CleConversation,
    },
    MiseEnEvidence {
        conversation: CleConversation,
        message: String,
    },
    Saisie {
        conversation: CleConversation,
    },
    Lectures {
        conversation: CleConversation,
    },
    Epingles {
        conversation: CleConversation,
    },
    Envois {
        conversation: CleConversation,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contexte {
    pub moi: String,
    pub courante: Option<CleConversation>,
    pub en_bas: bool,
}

impl Contexte {
    pub fn de(etat: &Etat) -> Self {
        Self {
            moi: etat.moi().to_string(),
            courante: etat.courante.clone(),
            en_bas: etat.en_bas,
        }
    }

    pub fn lit_l_historique(&self, cle: &CleConversation) -> bool {
        self.courante.as_ref() == Some(cle) && !self.en_bas
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Installation {
    Dernieres {
        messages: Vec<Message>,
        plus_anciens: bool,
        rechargee: bool,
    },
    PlusAnciens {
        avant: String,
        messages: Vec<Message>,
        plus_anciens: bool,
    },
    PlusRecents {
        apres: String,
        messages: Vec<Message>,
        plus_recents: bool,
    },
    Rattrapage {
        apres: String,
        messages: Vec<Message>,
    },
    Relecture {
        de: String,
        jusqu_a: String,
        messages: Vec<Message>,
    },
    Autour {
        message: String,
        messages: Vec<Message>,
        plus_anciens: bool,
        plus_recents: bool,
        mise_en_evidence: bool,
    },
}

fn changer(a: &mut Application, changement: ChangementMessages) {
    a.changer(Changement::Messages(changement));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cote {
    Tete,
    Fin,
}

#[derive(Debug, Default)]
struct Fusion {
    en_tete: usize,
    en_fin: usize,
    inseres: Vec<String>,
}

fn fusionner(conversation: &mut Conversation, messages: Vec<Message>) -> Fusion {
    let mut fusion = Fusion::default();
    let (premier, dernier) = (
        conversation.premier_id().map(str::to_string),
        conversation.dernier_id().map(str::to_string),
    );
    for message in messages {
        let id = id_message(&message).to_string();
        if id.is_empty() {
            continue;
        }
        let Err(i) = conversation.position(&id) else {
            continue;
        };
        conversation.messages.insert(i, message);
        match (&premier, &dernier) {
            (None, _) => fusion.en_fin += 1,
            (Some(p), _) if comparer_ids(&id, p).is_lt() => fusion.en_tete += 1,
            (_, Some(d)) if comparer_ids(&id, d).is_gt() => fusion.en_fin += 1,
            _ => fusion.inseres.push(id),
        }
    }
    fusion
}

fn annoncer(a: &mut Application, cle: &CleConversation, fusion: Fusion) {
    if fusion.en_tete > 0 {
        changer(
            a,
            ChangementMessages::AjoutesEnTete {
                conversation: cle.clone(),
                nombre: fusion.en_tete,
            },
        );
    }
    for message in fusion.inseres {
        changer(
            a,
            ChangementMessages::Insere {
                conversation: cle.clone(),
                message,
            },
        );
    }
    if fusion.en_fin > 0 {
        changer(
            a,
            ChangementMessages::AjoutesEnFin {
                conversation: cle.clone(),
                nombre: fusion.en_fin,
            },
        );
    }
}

fn borner(conversation: &mut Conversation, cle: &CleConversation, cote: Cote, a: &mut Application) {
    let exces = conversation
        .messages
        .len()
        .saturating_sub(MESSAGES_PAR_CONVERSATION);
    if exces == 0 {
        return;
    }
    match cote {
        Cote::Tete => {
            conversation.messages.drain(..exces);
            conversation.debut_atteint = false;
            changer(
                a,
                ChangementMessages::RetiresEnTete {
                    conversation: cle.clone(),
                    nombre: exces,
                },
            );
        }
        Cote::Fin => {
            let garde = conversation.messages.len() - exces;
            conversation.messages.truncate(garde);
            conversation.fin_atteinte = false;
            changer(
                a,
                ChangementMessages::RetiresEnFin {
                    conversation: cle.clone(),
                    nombre: exces,
                },
            );
        }
    }
}

fn bornes(
    conversation: &mut Conversation,
    cle: &CleConversation,
    debut: bool,
    fin: bool,
    a: &mut Application,
) {
    if (conversation.debut_atteint, conversation.fin_atteinte) != (debut, fin) {
        conversation.debut_atteint = debut;
        conversation.fin_atteinte = fin;
        changer(
            a,
            ChangementMessages::Bornes {
                conversation: cle.clone(),
            },
        );
    }
}

fn ajouter_au_present(
    conversation: &mut Conversation,
    cle: &CleConversation,
    messages: Vec<Message>,
    contexte: &Contexte,
    a: &mut Application,
) {
    let lecture = contexte.lit_l_historique(cle);
    let place = MESSAGES_PAR_CONVERSATION.saturating_sub(conversation.messages.len());
    let mut nouveaux: Vec<Message> = messages
        .into_iter()
        .filter(|m| !conversation.contient(id_message(m)))
        .collect();
    nouveaux.sort_by(|x, y| comparer_ids(id_message(x), id_message(y)));
    nouveaux.dedup_by(|x, y| id_message(x) == id_message(y));
    if lecture && nouveaux.len() > place {
        let au_dela = nouveaux.split_off(place);
        let premier_exclu = id_message(&au_dela[0]).to_string();
        let annonces = fusionner(conversation, nouveaux);
        let coupe = conversation.position(&premier_exclu).unwrap_or_else(|i| i);
        let retires = conversation.messages.len() - coupe;
        conversation.messages.truncate(coupe);
        annoncer(a, cle, annonces);
        if retires > 0 {
            changer(
                a,
                ChangementMessages::RetiresEnFin {
                    conversation: cle.clone(),
                    nombre: retires,
                },
            );
        }
        let debut = conversation.debut_atteint;
        bornes(conversation, cle, debut, false, a);
        return;
    }
    let annonces = fusionner(conversation, nouveaux);
    annoncer(a, cle, annonces);
    borner(conversation, cle, Cote::Tete, a);
}

fn relire(
    conversation: &mut Conversation,
    cle: &CleConversation,
    de: &str,
    jusqu_a: &str,
    messages: Vec<Message>,
    a: &mut Application,
) {
    let dans = |id: &str| comparer_ids(id, de).is_ge() && comparer_ids(id, jusqu_a).is_le();
    let mut relus: BTreeMap<String, Message> = messages
        .into_iter()
        .filter(|m| dans(id_message(m)))
        .map(|m| (id_message(&m).to_string(), m))
        .collect();
    let mut supprimes: Vec<String> = Vec::new();
    let mut modifies: Vec<String> = Vec::new();
    conversation.messages.retain_mut(|m| {
        let id = id_message(m).to_string();
        if !dans(&id) {
            return true;
        }
        match relus.remove(&id) {
            Some(relu) => {
                if *m != relu {
                    *m = relu;
                    modifies.push(id);
                }
                true
            }
            None => {
                supprimes.push(id);
                false
            }
        }
    });
    for message in modifies {
        modifie(a, cle.clone(), &message);
    }
    for message in supprimes {
        changer(
            a,
            ChangementMessages::Supprime {
                conversation: cle.clone(),
                message,
            },
        );
    }
    let fusion = fusionner(conversation, relus.into_values().collect());
    annoncer(a, cle, fusion);
}

pub fn installer(
    etat: &mut EtatMessages,
    contexte: &Contexte,
    cle: &CleConversation,
    installation: Installation,
) -> Application {
    let mut a = Application::default();
    let Some(conversation) = etat.conversations.get_mut(cle) else {
        return a;
    };
    conversation.en_chargement = false;
    match installation {
        Installation::Dernieres {
            messages,
            plus_anciens,
            rechargee,
        } => {
            let attente = std::mem::take(&mut conversation.en_attente);
            conversation.messages.clear();
            fusionner(conversation, messages);
            fusionner(conversation, attente);
            conversation.debut_atteint = !plus_anciens;
            conversation.fin_atteinte = true;
            conversation.chargee = true;
            conversation.rechargee = rechargee;
            let mut ignore = Application::default();
            borner(conversation, cle, Cote::Tete, &mut ignore);
            changer(
                &mut a,
                ChangementMessages::Rechargee {
                    conversation: cle.clone(),
                },
            );
        }
        Installation::PlusAnciens {
            avant,
            messages,
            plus_anciens,
        } => {
            if !conversation.chargee || conversation.premier_id() != Some(avant.as_str()) {
                return a;
            }
            let anciens: Vec<Message> = messages
                .into_iter()
                .filter(|m| comparer_ids(id_message(m), &avant).is_lt())
                .collect();
            let fusion = fusionner(conversation, anciens);
            annoncer(&mut a, cle, fusion);
            let fin = conversation.fin_atteinte;
            bornes(conversation, cle, !plus_anciens, fin, &mut a);
            borner(conversation, cle, Cote::Fin, &mut a);
        }
        Installation::PlusRecents {
            apres,
            messages,
            plus_recents,
        } => {
            if !conversation.chargee || conversation.dernier_id() != Some(apres.as_str()) {
                return a;
            }
            let recents: Vec<Message> = messages
                .into_iter()
                .filter(|m| comparer_ids(id_message(m), &apres).is_gt())
                .collect();
            let fusion = fusionner(conversation, recents);
            annoncer(&mut a, cle, fusion);
            let debut = conversation.debut_atteint;
            bornes(conversation, cle, debut, !plus_recents, &mut a);
            borner(conversation, cle, Cote::Tete, &mut a);
        }
        Installation::Rattrapage { apres, messages } => {
            if !conversation.chargee || !conversation.fin_atteinte {
                return a;
            }
            let recents: Vec<Message> = messages
                .into_iter()
                .filter(|m| comparer_ids(id_message(m), &apres).is_gt())
                .collect();
            ajouter_au_present(conversation, cle, recents, contexte, &mut a);
        }
        Installation::Relecture {
            de,
            jusqu_a,
            messages,
        } => {
            if conversation.chargee {
                relire(conversation, cle, &de, &jusqu_a, messages, &mut a);
            }
        }
        Installation::Autour {
            message,
            messages,
            plus_anciens,
            plus_recents,
            mise_en_evidence,
        } => {
            let present: Vec<Message> = if !plus_recents && conversation.fin_atteinte {
                let dernier = messages.last().map(id_message).unwrap_or(&message);
                conversation
                    .messages
                    .iter()
                    .filter(|m| comparer_ids(id_message(m), dernier).is_gt())
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            conversation.messages.clear();
            conversation.en_attente.clear();
            fusionner(conversation, messages);
            fusionner(conversation, present);
            conversation.debut_atteint = !plus_anciens;
            conversation.fin_atteinte = !plus_recents;
            conversation.chargee = true;
            conversation.rechargee = true;
            conversation.ancre = Some(message.clone());
            let mut ignore = Application::default();
            borner(conversation, cle, Cote::Tete, &mut ignore);
            changer(
                &mut a,
                ChangementMessages::Rechargee {
                    conversation: cle.clone(),
                },
            );
            if mise_en_evidence && conversation.contient(&message) {
                changer(
                    &mut a,
                    ChangementMessages::MiseEnEvidence {
                        conversation: cle.clone(),
                        message,
                    },
                );
            }
        }
    }
    a
}

pub fn appliquer_evenement(
    etat: &mut EtatMessages,
    contexte: &Contexte,
    evenement: &Evenement,
    maintenant: Instant,
) -> Application {
    let mut a = Application::default();
    match evenement {
        Evenement::MessageCree(m) => message_cree(etat, contexte, m, &mut a),
        Evenement::MessageModifie(m) => {
            let Some(cle) = conversation_du_message(m) else {
                return a;
            };
            if let Some(conversation) = etat.conversations.get_mut(&cle) {
                modifier(conversation, &cle, m, &mut a);
            }
            epingle_modifiee(etat, &cle, m, &mut a);
        }
        Evenement::MessageSupprime(d) => {
            if let Some(cle) = CleConversation::de(d.channel_id.as_deref(), d.dm_id.as_deref()) {
                supprimer(etat, &cle, std::slice::from_ref(&d.id), &mut a);
            }
        }
        Evenement::MessagesSupprimes(d) => {
            let cle = CleConversation::Salon(d.channel_id.clone());
            supprimer(etat, &cle, &d.ids, &mut a);
        }
        Evenement::ReactionModifiee(d) => reaction(etat, &contexte.moi, d, &mut a),
        Evenement::SondageVote(d) => vote(etat, &contexte.moi, d, &mut a),
        Evenement::SondageClos(d) => {
            if let Some((cle, message)) = trouver(etat, &d.message_id) {
                if let Some(sondage) = message.poll.as_mut() {
                    if sondage.closes_at.as_deref() != Some(d.closes_at.as_str()) {
                        sondage.closes_at = Some(d.closes_at.clone());
                        modifie(&mut a, cle, &d.message_id);
                    }
                }
            }
        }
        Evenement::Saisie(d) => {
            let Some(cle) = CleConversation::de(d.channel_id.as_deref(), d.dm_id.as_deref()) else {
                return a;
            };
            if d.user_id == contexte.moi {
                return a;
            }
            let liste = etat.saisies.entry(cle.clone()).or_default();
            let expire = maintenant + DUREE_SAISIE;
            match liste.iter_mut().find(|s| s.utilisateur == d.user_id) {
                Some(saisie) => {
                    saisie.expire = expire;
                    if saisie.pseudo != d.pseudo {
                        saisie.pseudo = d.pseudo.clone();
                        changer(&mut a, ChangementMessages::Saisie { conversation: cle });
                    }
                }
                None => {
                    liste.push(SaisieEnCours {
                        utilisateur: d.user_id.clone(),
                        pseudo: d.pseudo.clone(),
                        expire,
                    });
                    changer(&mut a, ChangementMessages::Saisie { conversation: cle });
                }
            }
        }
        Evenement::LectureModifiee(d) => {
            let Some(cle) = CleConversation::de(d.channel_id.as_deref(), d.dm_id.as_deref()) else {
                return a;
            };
            if let Some(conversation) = etat.conversations.get_mut(&cle) {
                if conversation.lectures.get(&d.user_id) != Some(d) {
                    conversation.lectures.insert(d.user_id.clone(), d.clone());
                    changer(&mut a, ChangementMessages::Lectures { conversation: cle });
                }
            }
        }
        Evenement::SalonSupprime(d) => {
            let cle = CleConversation::Salon(d.id.clone());
            etat.oublier(&cle);
            if etat.epingles_de(&cle).is_some() {
                etat.epingles = None;
            }
        }
        _ => {}
    }
    a
}

pub fn installer_lectures(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    lectures: Vec<Lecture>,
) -> Application {
    let mut a = Application::default();
    if let Some(conversation) = etat.conversations.get_mut(cle) {
        let nouvelles = lectures
            .into_iter()
            .map(|l| (l.user_id.clone(), l))
            .collect();
        if conversation.lectures != nouvelles {
            conversation.lectures = nouvelles;
            changer(
                &mut a,
                ChangementMessages::Lectures {
                    conversation: cle.clone(),
                },
            );
        }
    }
    a
}

pub fn mettre_en_evidence(
    etat: &EtatMessages,
    cle: &CleConversation,
    message: &str,
) -> Application {
    let mut a = Application::default();
    if etat.conversation(cle).is_some_and(|c| c.contient(message)) {
        changer(
            &mut a,
            ChangementMessages::MiseEnEvidence {
                conversation: cle.clone(),
                message: message.to_string(),
            },
        );
    }
    a
}

pub fn expirer_saisies(etat: &mut EtatMessages, maintenant: Instant) -> Application {
    let mut a = Application::default();
    for (cle, liste) in etat.saisies.iter_mut() {
        let avant = liste.len();
        liste.retain(|s| s.expire > maintenant);
        if liste.len() != avant {
            changer(
                &mut a,
                ChangementMessages::Saisie {
                    conversation: cle.clone(),
                },
            );
        }
    }
    etat.saisies.retain(|_, l| !l.is_empty());
    a
}

fn arreter_saisie(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    utilisateur: &str,
    a: &mut Application,
) {
    let Some(liste) = etat.saisies.get_mut(cle) else {
        return;
    };
    let avant = liste.len();
    liste.retain(|s| s.utilisateur != utilisateur);
    if liste.len() != avant {
        changer(
            a,
            ChangementMessages::Saisie {
                conversation: cle.clone(),
            },
        );
    }
    if liste.is_empty() {
        etat.saisies.remove(cle);
    }
}

pub(super) fn message_cree(
    etat: &mut EtatMessages,
    contexte: &Contexte,
    m: &Message,
    a: &mut Application,
) {
    super::envois::rapprocher_du_temps_reel(etat, contexte, m, a);
    let Some(cle) = conversation_du_message(m) else {
        return;
    };
    if id_message(m).is_empty() {
        return;
    }
    arreter_saisie(etat, &cle, auteur(m), a);
    if let CleConversation::Prive(id) = &cle {
        if etat.remontees_privees.first() != Some(id) {
            etat.remontees_privees.retain(|r| r != id);
            etat.remontees_privees.insert(0, id.clone());
            a.changer(Changement::MessagesPrives);
        }
    }
    let Some(conversation) = etat.conversations.get_mut(&cle) else {
        return;
    };
    if !conversation.chargee {
        if !conversation
            .en_attente
            .iter()
            .any(|x| id_message(x) == id_message(m))
        {
            conversation.en_attente.push(m.clone());
        }
        return;
    }
    if !conversation.fin_atteinte {
        return;
    }
    ajouter_au_present(conversation, &cle, vec![m.clone()], contexte, a);
}

fn modifier(
    conversation: &mut Conversation,
    cle: &CleConversation,
    m: &Message,
    a: &mut Application,
) {
    let id = id_message(m);
    if let Some(attente) = conversation
        .en_attente
        .iter_mut()
        .find(|x| id_message(x) == id)
    {
        *attente = m.clone();
    }
    let Ok(i) = conversation.position(id) else {
        return;
    };
    let ancien = &conversation.messages[i];
    let mut nouveau = m.clone();
    nouveau.reactions = ancien.reactions.clone();
    if ancien.poll.is_some() {
        nouveau.poll = ancien.poll.clone();
    }
    if conversation.messages[i] != nouveau {
        conversation.messages[i] = nouveau;
        modifie(a, cle.clone(), id);
    }
}

fn modifie(a: &mut Application, conversation: CleConversation, message: &str) {
    changer(
        a,
        ChangementMessages::Modifie {
            conversation,
            message: message.to_string(),
        },
    );
}

pub fn installer_epingles(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    messages: Vec<Message>,
) -> Application {
    let mut a = Application::default();
    if etat.epingles_de(cle) != Some(messages.as_slice()) {
        etat.epingles = Some((cle.clone(), messages));
        epingles_changees(&mut a, cle);
    }
    a
}

fn epingles_changees(a: &mut Application, cle: &CleConversation) {
    changer(
        a,
        ChangementMessages::Epingles {
            conversation: cle.clone(),
        },
    );
}

fn epingle_modifiee(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    m: &Message,
    a: &mut Application,
) {
    let Some((c, liste)) = etat.epingles.as_mut() else {
        return;
    };
    if c != cle {
        return;
    }
    let id = id_message(m);
    let place = liste.iter().position(|x| id_message(x) == id);
    let avant = liste.clone();
    match (place, m.pinned_at.is_some()) {
        (Some(i), true) => liste[i] = m.clone(),
        (Some(i), false) => {
            liste.remove(i);
        }
        (None, true) => {
            let quand = m.pinned_at.as_deref();
            let i = liste
                .iter()
                .position(|x| x.pinned_at.as_deref() < quand)
                .unwrap_or(liste.len());
            liste.insert(i, m.clone());
        }
        (None, false) => {}
    }
    if *liste != avant {
        epingles_changees(a, cle);
    }
}

pub(super) fn supprimer(
    etat: &mut EtatMessages,
    cle: &CleConversation,
    ids: &[String],
    a: &mut Application,
) {
    if let Some((c, liste)) = etat.epingles.as_mut() {
        if c == cle {
            let avant = liste.len();
            liste.retain(|m| !ids.iter().any(|id| id == id_message(m)));
            if liste.len() != avant {
                epingles_changees(a, cle);
            }
        }
    }
    let Some(conversation) = etat.conversations.get_mut(cle) else {
        return;
    };
    conversation
        .en_attente
        .retain(|m| !ids.iter().any(|id| id == id_message(m)));
    for id in ids {
        if let Ok(i) = conversation.position(id) {
            conversation.messages.remove(i);
            changer(
                a,
                ChangementMessages::Supprime {
                    conversation: cle.clone(),
                    message: id.clone(),
                },
            );
        }
    }
}

fn trouver<'a>(etat: &'a mut EtatMessages, id: &str) -> Option<(CleConversation, &'a mut Message)> {
    etat.conversations.iter_mut().find_map(|(cle, c)| {
        let i = c.position(id).ok()?;
        Some((cle.clone(), &mut c.messages[i]))
    })
}

fn reaction(etat: &mut EtatMessages, moi: &str, d: &ReactionModifiee, a: &mut Application) {
    let Some((cle, message)) = trouver(etat, &d.message_id) else {
        return;
    };
    let reactions = message.reactions.get_or_insert_with(Vec::new);
    let mien = d.user_id == moi;
    let position = reactions
        .iter()
        .position(|r| r.emoji.as_deref() == Some(d.emoji.as_str()));
    if d.added {
        match position {
            Some(i) => {
                let r = &mut reactions[i];
                if mien && r.me == Some(true) {
                    return;
                }
                r.count = Some(r.count.unwrap_or(0) + 1);
                if mien {
                    r.me = Some(true);
                }
                let personnes = r.users.get_or_insert_with(Vec::new);
                if !personnes
                    .iter()
                    .any(|p| p.id.as_deref() == Some(d.user_id.as_str()))
                {
                    personnes.push(personne(d));
                }
            }
            None => reactions.push(Reaction {
                emoji: Some(d.emoji.clone()),
                count: Some(1),
                me: Some(mien),
                users: Some(vec![personne(d)]),
            }),
        }
    } else {
        let Some(i) = position else {
            return;
        };
        let r = &mut reactions[i];
        if mien && r.me != Some(true) {
            return;
        }
        r.count = Some(r.count.unwrap_or(0) - 1);
        if mien {
            r.me = Some(false);
        }
        if let Some(personnes) = r.users.as_mut() {
            personnes.retain(|p| p.id.as_deref() != Some(d.user_id.as_str()));
        }
        if r.count.unwrap_or(0) <= 0 {
            reactions.remove(i);
        }
    }
    modifie(a, cle, &d.message_id);
}

fn personne(d: &ReactionModifiee) -> Person {
    Person {
        id: Some(d.user_id.clone()),
        pseudo: Some(d.pseudo.clone().unwrap_or_default()),
    }
}

fn vote(etat: &mut EtatMessages, moi: &str, d: &SondageVote, a: &mut Application) {
    let Some((cle, message)) = trouver(etat, &d.message_id) else {
        return;
    };
    let Some(sondage) = message.poll.as_mut() else {
        return;
    };
    let avant = sondage.clone();
    for option in sondage.options.iter_mut().flatten() {
        let id = option.id.clone().unwrap_or_default();
        option.votes = Some(d.counts.get(&id).copied().unwrap_or(0));
        if d.user_id == moi {
            option.me = Some(d.options.contains(&id));
        }
    }
    if *sondage != avant {
        modifie(a, cle, &d.message_id);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::store::essais::{depuis_json, evenement, message};

    fn cle(texte: &str) -> CleConversation {
        CleConversation::analyser(texte).unwrap()
    }

    fn contexte() -> Contexte {
        Contexte {
            moi: "me".into(),
            courante: None,
            en_bas: true,
        }
    }

    fn msg(id: &str) -> Message {
        depuis_json(message(id, "c1", "bob"))
    }

    fn avec(ids: &[&str]) -> EtatMessages {
        let mut etat = EtatMessages::default();
        let c = etat.reserver(&cle("c:c1"), None);
        c.messages = ids.iter().map(|id| msg(id)).collect();
        c.chargee = true;
        c.fin_atteinte = true;
        etat
    }

    fn ids(etat: &EtatMessages, conversation: &str) -> Vec<String> {
        etat.conversation(&cle(conversation))
            .map(|c| {
                c.messages
                    .iter()
                    .map(|m| id_message(m).to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ev(etat: &mut EtatMessages, type_evenement: &str, data: Value) -> Application {
        appliquer_evenement(
            etat,
            &contexte(),
            &evenement(type_evenement, data),
            Instant::now(),
        )
    }

    fn premier(etat: &EtatMessages) -> &Message {
        &etat.conversation(&cle("c:c1")).unwrap().messages[0]
    }

    fn message_cle(id: &str) -> ChangementMessages {
        ChangementMessages::AjoutesEnFin {
            conversation: cle(id),
            nombre: 1,
        }
    }

    #[test]
    fn ajoute_un_message_recu_une_seule_fois() {
        let mut s = avec(&[]);
        let a = ev(&mut s, "message.created", message("1", "c1", "bob"));
        ev(&mut s, "message.created", message("1", "c1", "bob"));
        assert_eq!(ids(&s, "c:c1"), ["1"]);
        assert_eq!(a.changements, [Changement::Messages(message_cle("c:c1"))]);
    }

    #[test]
    fn ne_charge_rien_pour_une_conversation_jamais_ouverte() {
        let mut s = avec(&[]);
        let a = ev(&mut s, "message.created", message("1", "c2", "bob"));
        assert!(s.conversation(&cle("c:c2")).is_none());
        assert!(a.changements.is_empty());
    }

    #[test]
    fn conserve_ses_propres_reactions_lors_d_une_modification() {
        let mut s = avec(&[]);
        s.conversations.get_mut(&cle("c:c1")).unwrap().messages = vec![depuis_json(
            json!({"id": "1", "channel_id": "c1", "reactions": [{"emoji": "👍", "count": 2, "me": true, "users": []}]}),
        )];
        let a = ev(
            &mut s,
            "message.updated",
            json!({"id": "1", "channel_id": "c1", "content": "modifié", "edited_at": "x", "pinned_at": "2026-10-13T11:00:00Z", "reactions": [{"emoji": "👍", "count": 2, "me": false, "users": []}]}),
        );
        let m = premier(&s);
        assert_eq!(m.content.as_deref(), Some("modifié"));
        assert_eq!(m.pinned_at.as_deref(), Some("2026-10-13T11:00:00Z"));
        assert_eq!(m.reactions.as_ref().unwrap()[0].me, Some(true));
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Modifie {
                conversation: cle("c:c1"),
                message: "1".into()
            })]
        );
    }

    #[test]
    fn retire_un_message_supprime() {
        let mut s = avec(&["1", "2"]);
        let a = ev(
            &mut s,
            "message.deleted",
            json!({"id": "1", "channel_id": "c1"}),
        );
        assert_eq!(ids(&s, "c:c1"), ["2"]);
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Supprime {
                conversation: cle("c:c1"),
                message: "1".into()
            })]
        );
    }

    #[test]
    fn retire_les_messages_supprimes_en_masse() {
        let mut s = avec(&[]);
        for id in ["1", "2", "3"] {
            ev(&mut s, "message.created", message(id, "c1", "bob"));
        }
        let a = ev(
            &mut s,
            "messages.deleted",
            json!({"channel_id": "c1", "ids": ["1", "3"]}),
        );
        assert_eq!(ids(&s, "c:c1"), ["2"]);
        assert_eq!(a.changements.len(), 2);
    }

    #[test]
    fn compte_les_reactions_des_autres_et_distingue_les_siennes() {
        let mut s = avec(&["1"]);
        let r = |user: &str, added: bool, pseudo: Option<&str>| json!({"message_id": "1", "emoji": "👍", "user_id": user, "pseudo": pseudo, "added": added});
        ev(&mut s, "reaction.updated", r("bob", true, Some("bob")));
        ev(&mut s, "reaction.updated", r("me", true, None));
        let reactions = |s: &EtatMessages| -> Value {
            serde_json::to_value(premier(s).reactions.clone().unwrap_or_default()).unwrap()
        };
        assert_eq!(
            reactions(&s),
            json!([{"emoji": "👍", "count": 2, "me": true, "users": [{"id": "bob", "pseudo": "bob"}, {"id": "me", "pseudo": ""}]}])
        );
        ev(&mut s, "reaction.updated", r("bob", false, None));
        assert_eq!(
            reactions(&s),
            json!([{"emoji": "👍", "count": 1, "me": true, "users": [{"id": "me", "pseudo": ""}]}])
        );
        ev(&mut s, "reaction.updated", r("me", false, None));
        assert_eq!(reactions(&s), json!([]));
    }

    #[test]
    fn ne_compte_pas_deux_fois_sa_propre_reaction() {
        let mut s = avec(&["1"]);
        s.conversations.get_mut(&cle("c:c1")).unwrap().messages[0].reactions = Some(vec![
            depuis_json(
                json!({"emoji": "👍", "count": 1, "me": true, "users": [{"id": "me", "pseudo": "moi"}]}),
            ),
        ]);
        let a = ev(
            &mut s,
            "reaction.updated",
            json!({"message_id": "1", "emoji": "👍", "user_id": "me", "added": true}),
        );
        assert_eq!(premier(&s).reactions.as_ref().unwrap()[0].count, Some(1));
        assert!(a.changements.is_empty());
        s.conversations.get_mut(&cle("c:c1")).unwrap().messages[0].reactions = Some(vec![]);
        ev(
            &mut s,
            "reaction.updated",
            json!({"message_id": "1", "emoji": "❤️", "user_id": "me", "added": false}),
        );
        assert_eq!(premier(&s).reactions, Some(vec![]));
    }

    fn avec_sondage() -> EtatMessages {
        let mut s = avec(&[]);
        s.conversations.get_mut(&cle("c:c1")).unwrap().messages = vec![depuis_json(json!({
            "id": "1", "channel_id": "c1",
            "poll": {"multiple": false, "closes_at": null, "created_by": {"id": "bob", "pseudo": "bob"},
                     "options": [{"id": "5", "label": "a", "votes": 0, "me": false}, {"id": "6", "label": "b", "votes": 0, "me": false}]}
        }))];
        s
    }

    fn options(s: &EtatMessages) -> Vec<(i64, bool)> {
        premier(s)
            .poll
            .as_ref()
            .unwrap()
            .options
            .iter()
            .flatten()
            .map(|o| (o.votes.unwrap_or(0), o.me == Some(true)))
            .collect()
    }

    #[test]
    fn reprend_les_totaux_du_vote_et_ne_marque_me_que_pour_son_propre_vote() {
        let mut s = avec_sondage();
        ev(
            &mut s,
            "poll.voted",
            json!({"message_id": "1", "user_id": "bob", "options": ["5"], "counts": {"5": 1, "6": 0}}),
        );
        assert_eq!(options(&s), [(1, false), (0, false)]);
        ev(
            &mut s,
            "poll.voted",
            json!({"message_id": "1", "user_id": "me", "options": ["6"], "counts": {"5": 1, "6": 1}}),
        );
        assert_eq!(options(&s), [(1, false), (1, true)]);
    }

    #[test]
    fn marque_le_sondage_clos_et_garde_ses_votes_a_la_modification() {
        let mut s = avec_sondage();
        let a = ev(
            &mut s,
            "poll.closed",
            json!({"message_id": "1", "closes_at": "2026-01-01T00:00:00Z"}),
        );
        assert_eq!(
            premier(&s).poll.as_ref().unwrap().closes_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(a.changements.len(), 1);
        ev(
            &mut s,
            "poll.voted",
            json!({"message_id": "1", "user_id": "me", "options": ["5"], "counts": {"5": 1}}),
        );
        ev(
            &mut s,
            "message.updated",
            json!({"id": "1", "channel_id": "c1", "content": "Q ?", "poll": {"options": [{"id": "5", "votes": 1, "me": false}]}}),
        );
        assert_eq!(options(&s), [(1, true), (0, false)]);
        assert_eq!(premier(&s).content.as_deref(), Some("Q ?"));
    }

    #[test]
    fn applique_le_temps_reel_aux_conversations_en_cache_non_affichees() {
        let mut s = avec(&["1"]);
        let c2 = s.reserver(&cle("d:d1"), None);
        c2.chargee = true;
        c2.fin_atteinte = true;
        ev(
            &mut s,
            "message.created",
            json!({"id": "2", "dm_id": "d1", "author": {"id": "bob", "pseudo": "bob"}}),
        );
        ev(
            &mut s,
            "reaction.updated",
            json!({"message_id": "1", "emoji": "👍", "user_id": "bob", "added": true}),
        );
        assert_eq!(ids(&s, "d:d1"), ["2"]);
        assert_eq!(premier(&s).reactions.as_ref().unwrap().len(), 1);
        assert_eq!(s.remontees_privees, ["d1"]);
    }

    #[test]
    fn suit_qui_ecrit_et_l_oublie_a_l_expiration_ou_a_son_message() {
        let mut s = avec(&[]);
        let depart = Instant::now();
        let saisie = |u: &str| json!({"channel_id": "c1", "user_id": u, "pseudo": u});
        let a = appliquer_evenement(
            &mut s,
            &contexte(),
            &evenement("typing", saisie("bob")),
            depart,
        );
        appliquer_evenement(
            &mut s,
            &contexte(),
            &evenement("typing", saisie("me")),
            depart,
        );
        appliquer_evenement(
            &mut s,
            &contexte(),
            &evenement("typing", saisie("eve")),
            depart + DUREE_SAISIE / 2,
        );
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Saisie {
                conversation: cle("c:c1")
            })]
        );
        assert_eq!(s.saisies_de(&cle("c:c1")), ["bob", "eve"]);
        assert_eq!(s.prochaine_expiration(), Some(depart + DUREE_SAISIE));
        assert!(expirer_saisies(&mut s, depart + DUREE_SAISIE / 4)
            .changements
            .is_empty());
        let a = expirer_saisies(&mut s, depart + DUREE_SAISIE);
        assert_eq!(a.changements.len(), 1);
        assert_eq!(s.saisies_de(&cle("c:c1")), ["eve"]);
        ev(&mut s, "message.created", message("1", "c1", "eve"));
        assert!(s.saisies.is_empty());
        assert_eq!(s.prochaine_expiration(), None);
    }

    #[test]
    fn retient_les_accuses_de_lecture_des_conversations_en_cache() {
        let mut s = avec(&["9", "10"]);
        let lecture = json!({"channel_id": "c1", "user_id": "bob", "pseudo": "bob", "last_id": "10", "read_at": "x"});
        let a = ev(&mut s, "read.updated", lecture.clone());
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Lectures {
                conversation: cle("c:c1")
            })]
        );
        assert!(ev(&mut s, "read.updated", lecture).changements.is_empty());
        assert_eq!(
            s.lecteurs(&cle("c:c1"), "me")
                .into_keys()
                .collect::<Vec<_>>(),
            ["10"]
        );
        ev(
            &mut s,
            "read.updated",
            json!({"channel_id": "c9", "user_id": "bob", "pseudo": "bob", "last_id": "1", "read_at": "x"}),
        );
        assert!(s.conversation(&cle("c:c9")).is_none());
    }

    #[test]
    fn garde_les_messages_recus_pendant_le_chargement_de_la_premiere_page() {
        let mut s = EtatMessages::default();
        s.reserver(&cle("c:c1"), None).en_chargement = true;
        ev(&mut s, "message.created", message("52", "c1", "bob"));
        let a = installer(
            &mut s,
            &contexte(),
            &cle("c:c1"),
            Installation::Dernieres {
                messages: vec![msg("50"), msg("51")],
                plus_anciens: true,
                rechargee: false,
            },
        );
        assert_eq!(ids(&s, "c:c1"), ["50", "51", "52"]);
        let c = s.conversation(&cle("c:c1")).unwrap();
        assert!(c.fin_atteinte && !c.debut_atteint && !c.en_chargement);
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Rechargee {
                conversation: cle("c:c1")
            })]
        );
    }

    #[test]
    fn une_page_plus_ancienne_s_ajoute_en_tete_avec_son_nombre() {
        let mut s = avec(&["10", "11"]);
        let a = installer(
            &mut s,
            &contexte(),
            &cle("c:c1"),
            Installation::PlusAnciens {
                avant: "10".into(),
                messages: vec![msg("8"), msg("9"), msg("10")],
                plus_anciens: false,
            },
        );
        assert_eq!(ids(&s, "c:c1"), ["8", "9", "10", "11"]);
        assert_eq!(
            a.changements,
            [
                Changement::Messages(ChangementMessages::AjoutesEnTete {
                    conversation: cle("c:c1"),
                    nombre: 2
                }),
                Changement::Messages(ChangementMessages::Bornes {
                    conversation: cle("c:c1")
                })
            ]
        );
        assert!(s.conversation(&cle("c:c1")).unwrap().debut_atteint);
        let perimee = installer(
            &mut s,
            &contexte(),
            &cle("c:c1"),
            Installation::PlusAnciens {
                avant: "10".into(),
                messages: vec![msg("1")],
                plus_anciens: false,
            },
        );
        assert!(perimee.changements.is_empty());
        assert_eq!(ids(&s, "c:c1").len(), 4);
    }

    #[test]
    fn borne_les_messages_par_conversation_en_chargeant_de_nombreuses_pages() {
        let mut s = avec(&[]);
        let depart = 1_000_000u64;
        let page = |fin: u64| -> Vec<Message> {
            (fin - LIMITE as u64..fin)
                .map(|i| msg(&i.to_string()))
                .collect()
        };
        const LIMITE: usize = 50;
        installer(
            &mut s,
            &contexte(),
            &cle("c:c1"),
            Installation::Dernieres {
                messages: page(depart),
                plus_anciens: true,
                rechargee: false,
            },
        );
        let mut retires = 0;
        for _ in 0..40 {
            let avant = s
                .conversation(&cle("c:c1"))
                .unwrap()
                .premier_id()
                .unwrap()
                .to_string();
            let fin: u64 = avant.parse().unwrap();
            let a = installer(
                &mut s,
                &contexte(),
                &cle("c:c1"),
                Installation::PlusAnciens {
                    avant,
                    messages: page(fin),
                    plus_anciens: true,
                },
            );
            for c in a.changements {
                if let Changement::Messages(ChangementMessages::RetiresEnFin { nombre, .. }) = c {
                    retires += nombre;
                }
            }
            assert!(
                s.conversation(&cle("c:c1")).unwrap().messages.len() <= MESSAGES_PAR_CONVERSATION
            );
        }
        let c = s.conversation(&cle("c:c1")).unwrap();
        assert_eq!(c.messages.len(), MESSAGES_PAR_CONVERSATION);
        assert_eq!(retires, 41 * LIMITE - MESSAGES_PAR_CONVERSATION);
        assert!(!c.fin_atteinte);
        assert!(c
            .messages
            .windows(2)
            .all(|w| comparer_ids(id_message(&w[0]), id_message(&w[1])).is_lt()));
        let nouveau = ev(
            &mut s,
            "message.created",
            message(&depart.to_string(), "c1", "bob"),
        );
        assert!(nouveau.changements.is_empty());
    }

    #[test]
    fn un_message_recu_pendant_la_lecture_de_l_historique_s_ajoute_en_fin_sans_rien_deplacer() {
        let mut s = avec(&[]);
        let c = s.conversations.get_mut(&cle("c:c1")).unwrap();
        c.messages = (1..=50).map(|i| msg(&i.to_string())).collect();
        c.ancre = Some("12".into());
        let lecture = Contexte {
            moi: "me".into(),
            courante: Some(cle("c:c1")),
            en_bas: false,
        };
        let avant = s.conversation(&cle("c:c1")).unwrap().messages.clone();
        let a = appliquer_evenement(
            &mut s,
            &lecture,
            &evenement("message.created", message("51", "c1", "bob")),
            Instant::now(),
        );
        assert_eq!(a.changements, [Changement::Messages(message_cle("c:c1"))]);
        let c = s.conversation(&cle("c:c1")).unwrap();
        assert_eq!(c.messages[..50], avant[..]);
        assert_eq!(c.dernier_id(), Some("51"));
        assert_eq!(c.ancre.as_deref(), Some("12"));
    }

    #[test]
    fn plein_pendant_la_lecture_de_l_historique_le_present_n_est_plus_charge_au_lieu_de_couper_le_haut(
    ) {
        let mut s = avec(&[]);
        let c = s.conversations.get_mut(&cle("c:c1")).unwrap();
        c.messages = (1..=MESSAGES_PAR_CONVERSATION as u64)
            .map(|i| msg(&(i + 1000).to_string()))
            .collect();
        let lecture = Contexte {
            moi: "me".into(),
            courante: Some(cle("c:c1")),
            en_bas: false,
        };
        let avant = s.conversation(&cle("c:c1")).unwrap().messages.clone();
        let a = appliquer_evenement(
            &mut s,
            &lecture,
            &evenement("message.created", message("9999", "c1", "bob")),
            Instant::now(),
        );
        let c = s.conversation(&cle("c:c1")).unwrap();
        assert_eq!(c.messages, avant);
        assert!(!c.fin_atteinte);
        assert_eq!(
            a.changements,
            [Changement::Messages(ChangementMessages::Bornes {
                conversation: cle("c:c1")
            })]
        );
        let mut s2 = avec(&[]);
        s2.conversations.get_mut(&cle("c:c1")).unwrap().messages = avant;
        let a = ev(&mut s2, "message.created", message("9999", "c1", "bob"));
        assert_eq!(
            a.changements,
            [
                Changement::Messages(message_cle("c:c1")),
                Changement::Messages(ChangementMessages::RetiresEnTete {
                    conversation: cle("c:c1"),
                    nombre: 1
                })
            ]
        );
        assert_eq!(
            s2.conversation(&cle("c:c1")).unwrap().messages.len(),
            MESSAGES_PAR_CONVERSATION
        );
    }

    #[test]
    fn charger_autour_remplace_l_historique_et_garde_les_messages_arrives_entre_temps() {
        let mut s = avec(&["99", "101"]);
        let a = installer(
            &mut s,
            &contexte(),
            &cle("c:c1"),
            Installation::Autour {
                message: "5".into(),
                messages: vec![msg("5"), msg("6"), msg("7")],
                plus_anciens: true,
                plus_recents: false,
                mise_en_evidence: true,
            },
        );
        assert_eq!(ids(&s, "c:c1"), ["5", "6", "7", "99", "101"]);
        let c = s.conversation(&cle("c:c1")).unwrap();
        assert!(c.rechargee && c.fin_atteinte && !c.debut_atteint);
        assert_eq!(
            a.changements,
            [
                Changement::Messages(ChangementMessages::Rechargee {
                    conversation: cle("c:c1")
                }),
                Changement::Messages(ChangementMessages::MiseEnEvidence {
                    conversation: cle("c:c1"),
                    message: "5".into()
                })
            ]
        );
    }
}
