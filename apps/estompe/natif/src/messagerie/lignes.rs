use std::collections::{BTreeMap, HashMap, HashSet};

use estompe_client::emoji::titre_de_reaction;
use estompe_client::fichiers::{categorie, nom_de_fichier_sur, CategoriePiece};
use estompe_client::format::taille_lisible;
use estompe_client::format::{date_heure_complete, heure_de, lignes_de_messages, Ligne};
use estompe_client::mention::Mentionnables;
use estompe_client::roles::{couleur_du_pseudo, Membre};
use estompe_client::sondage::{est_clos, fin_en_ms, gagnants, libelle_du_pied, pourcentage, total};
use estompe_client::store::messages::{
    auteur, est_compte_estompe, est_local, id_message, EnvoiLocal, EtatEnvoi,
};
use estompe_client::store::{CleConversation, Etat};
use estompe_client::temps_reel::evenements::Lecture;
use estompe_client::types::{Attachment, Message, MessagePoll, Person};

pub const TITRE_REACTION_MAX: usize = 3;

use super::actions::{droits_du_message, DroitsConversation, DroitsMessage, Sujet};
use crate::coquille::avatars::CleAvatar;
use crate::coquille::membres::{analyser_couleur, cle_avatar};
use crate::coquille::projection::{initiales, teinte};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreLigne {
    Jour,
    Nouveau,
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesReponse {
    pub id: String,
    pub auteur: String,
    pub texte: String,
    pub supprimee: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesReaction {
    pub emoji: String,
    pub nombre: i64,
    pub moi: bool,
    pub titre: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenrePiece {
    Image,
    Fichier,
    Media,
    Expiree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesPiece {
    pub id: String,
    pub nom: String,
    pub nom_sur: String,
    pub genre: GenrePiece,
    pub taille: String,
    pub anime: bool,
    pub vocal: bool,
    pub duree_ms: u64,
    pub forme: Vec<u8>,
    pub transcription: Option<String>,
    pub transcription_auto: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesOptionSondage {
    pub id: String,
    pub libelle: String,
    pub votes: i64,
    pub pourcentage: i64,
    pub moi: bool,
    pub gagnant: bool,
    pub votants: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesSondage {
    pub auteur: String,
    pub auteur_bot: bool,
    pub clos: bool,
    pub options: Vec<DonneesOptionSondage>,
    pub pied: String,
    pub fin_ms: Option<i64>,
    pub total: i64,
    pub votable: bool,
    pub gerer: bool,
    pub votants_ouverts: bool,
}

pub type Votants = BTreeMap<String, Vec<Person>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesLecteur {
    pub utilisateur: String,
    pub pseudo: String,
    pub initiales: String,
    pub teinte: i32,
    pub avatar: Option<CleAvatar>,
    pub heure: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DonneesAnnexes {
    pub pieces: Vec<DonneesPiece>,
    pub sondage: Option<DonneesSondage>,
    pub reactions: Vec<DonneesReaction>,
    pub lien: Option<String>,
    pub lecteurs: Vec<DonneesLecteur>,
}

impl DonneesAnnexes {
    pub fn est_vide(&self) -> bool {
        self.pieces.is_empty()
            && self.sondage.is_none()
            && self.reactions.is_empty()
            && self.lien.is_none()
            && self.lecteurs.is_empty()
    }

    pub fn images(&self) -> impl Iterator<Item = &DonneesPiece> {
        self.pieces.iter().filter(|p| p.genre == GenrePiece::Image)
    }

    pub fn resume(&self) -> String {
        let mut parties = Vec::new();
        match self.pieces.len() {
            0 => {}
            1 => parties.push("1 pièce jointe".to_string()),
            n => parties.push(format!("{n} pièces jointes")),
        }
        if self.sondage.is_some() {
            parties.push("Sondage".to_string());
        }
        if !self.reactions.is_empty() {
            parties.push(
                self.reactions
                    .iter()
                    .map(|r| format!("{} {}", r.emoji, r.nombre))
                    .collect::<Vec<_>>()
                    .join("  "),
            );
        }
        if !self.lecteurs.is_empty() {
            parties.push(libelle_vu_par(&self.lecteurs));
        }
        parties.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesMessage {
    pub id: String,
    pub utilisateur: String,
    pub auteur: String,
    pub initiales: String,
    pub teinte: i32,
    pub couleur: Option<String>,
    pub avatar: Option<CleAvatar>,
    pub bot: bool,
    pub heure: String,
    pub date_complete: String,
    pub suite: bool,
    pub systeme: bool,
    pub epingle: bool,
    pub modifie: bool,
    pub date_modification: String,
    pub mention: bool,
    pub mis_en_evidence: bool,
    pub reponse: Option<DonneesReponse>,
    pub contenu: String,
    pub annexes: DonneesAnnexes,
    pub envoi: Option<EtatEnvoi>,
    pub droits: DroitsMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonneesLigne {
    pub cle: String,
    pub genre: GenreLigne,
    pub libelle: String,
    pub message: Option<DonneesMessage>,
}

pub struct ContexteLignes<'a> {
    pub etat: &'a Etat,
    pub serveur: Option<&'a str>,
    pub nouveau_depuis: Option<&'a str>,
    pub mis_en_evidence: Option<&'a str>,
    pub supprimes: &'a HashSet<String>,
    pub decalage_minutes: i32,
    pub maintenant_ms: i64,
    pub apercus: bool,
    pub lecteurs: &'a BTreeMap<String, Vec<Lecture>>,
    pub envois: &'a [EnvoiLocal],
    pub droits: DroitsConversation,
    pub votants: &'a HashMap<String, Votants>,
}

fn texte(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

fn membre<'a>(etat: &'a Etat, serveur: Option<&str>, utilisateur: &str) -> Option<&'a Membre> {
    etat.details
        .get(serveur?)?
        .members
        .as_deref()?
        .iter()
        .find(|m| texte(&m.id) == utilisateur)
}

fn cle_avatar_du_message(
    etat: &Etat,
    serveur: Option<&str>,
    utilisateur: &str,
) -> Option<CleAvatar> {
    match serveur {
        Some(serveur) => cle_avatar(etat, serveur, utilisateur),
        None => {
            let version = etat.version_avatar(utilisateur);
            (version > 0).then(|| CleAvatar {
                utilisateur: utilisateur.to_string(),
                version,
                serveur: None,
            })
        }
    }
}

fn une_ligne(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn piece(piece: &Attachment) -> DonneesPiece {
    let mime = texte(&piece.mime);
    let vocal = categorie(piece) == CategoriePiece::Vocal;
    let expiree = piece.expired == Some(true);
    let nom = if vocal && expiree {
        "Message vocal".to_string()
    } else {
        texte(&piece.name).to_string()
    };
    DonneesPiece {
        id: texte(&piece.id).to_string(),
        nom_sur: nom_de_fichier_sur(&nom),
        genre: match categorie(piece) {
            _ if expiree => GenrePiece::Expiree,
            CategoriePiece::Image => GenrePiece::Image,
            CategoriePiece::Fichier => GenrePiece::Fichier,
            CategoriePiece::Expiree => GenrePiece::Expiree,
            CategoriePiece::Vocal | CategoriePiece::Replay | CategoriePiece::Video => {
                GenrePiece::Media
            }
        },
        taille: taille_lisible(piece.size.unwrap_or(0).max(0) as u64),
        anime: matches!(mime, "image/gif" | "image/webp"),
        vocal,
        duree_ms: piece.duration_ms.unwrap_or(0).max(0) as u64,
        forme: piece
            .waveform
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|v| (*v).clamp(0, 100) as u8)
            .collect(),
        transcription: piece.transcript.clone().filter(|_| vocal),
        transcription_auto: piece.transcript_auto == Some(true),
        nom,
    }
}

pub fn sondage(sondage: &MessagePoll, maintenant_ms: i64) -> DonneesSondage {
    let options = sondage.options.as_deref().unwrap_or_default();
    let clos_a = sondage.closes_at.as_deref();
    let clos = est_clos(clos_a, maintenant_ms);
    let total = total(options);
    let gagnants = if clos { gagnants(options) } else { Vec::new() };
    let auteur = sondage.created_by.as_ref();
    DonneesSondage {
        auteur: auteur.and_then(|p| p.pseudo.clone()).unwrap_or_default(),
        auteur_bot: auteur
            .and_then(|p| p.id.as_deref())
            .is_some_and(est_compte_estompe),
        clos,
        options: options
            .iter()
            .map(|o| {
                let id = texte(&o.id).to_string();
                let votes = o.votes.unwrap_or(0);
                DonneesOptionSondage {
                    gagnant: gagnants.contains(&id),
                    libelle: texte(&o.label).to_string(),
                    pourcentage: pourcentage(votes, total),
                    moi: o.me == Some(true),
                    votes,
                    id,
                    votants: None,
                }
            })
            .collect(),
        pied: libelle_du_pied(total, sondage.multiple == Some(true), clos_a, maintenant_ms),
        fin_ms: fin_en_ms(clos_a).filter(|fin| *fin > maintenant_ms),
        total,
        votable: !clos,
        gerer: false,
        votants_ouverts: false,
    }
}

fn completer_sondage(
    sondage: &mut DonneesSondage,
    droits: DroitsMessage,
    local: bool,
    votants: Option<&Votants>,
) {
    sondage.votable &= !local;
    sondage.gerer = droits.supprimer;
    sondage.votants_ouverts = votants.is_some();
    for option in &mut sondage.options {
        option.votants = votants.map(|v| {
            v.get(&option.id)
                .into_iter()
                .flatten()
                .map(|p| texte(&p.pseudo))
                .collect::<Vec<_>>()
                .join(", ")
        });
    }
}

pub fn annexes(message: &Message, moi: &str, maintenant_ms: i64, apercus: bool) -> DonneesAnnexes {
    let sondage = message.poll.as_ref().map(|p| sondage(p, maintenant_ms));
    DonneesAnnexes {
        pieces: if sondage.is_some() {
            Vec::new()
        } else {
            message.attachments.iter().flatten().map(piece).collect()
        },
        reactions: message
            .reactions
            .iter()
            .flatten()
            .filter(|r| r.count.unwrap_or(0) > 0)
            .map(|r| DonneesReaction {
                emoji: texte(&r.emoji).to_string(),
                nombre: r.count.unwrap_or(0),
                moi: r.me == Some(true),
                titre: titre_de_reaction(r, moi, TITRE_REACTION_MAX),
            })
            .collect(),
        lien: if sondage.is_some() || !apercus {
            None
        } else {
            estompe_client::markdown::premier_lien(texte(&message.content))
        },
        sondage,
        lecteurs: Vec::new(),
    }
}

pub fn libelle_vu_par(lecteurs: &[DonneesLecteur]) -> String {
    if lecteurs.is_empty() {
        return String::new();
    }
    let noms: Vec<&str> = lecteurs.iter().map(|l| l.pseudo.as_str()).collect();
    format!("Vu par {}", noms.join(", "))
}

pub fn lecteurs(lectures: &[Lecture], c: &ContexteLignes) -> Vec<DonneesLecteur> {
    lectures
        .iter()
        .map(|l| DonneesLecteur {
            initiales: initiales(&l.pseudo),
            teinte: teinte(if l.user_id.is_empty() {
                &l.pseudo
            } else {
                &l.user_id
            }),
            avatar: cle_avatar_du_message(c.etat, c.serveur, &l.user_id),
            heure: heure_de(&l.read_at, c.decalage_minutes).unwrap_or_default(),
            pseudo: l.pseudo.clone(),
            utilisateur: l.user_id.clone(),
        })
        .collect()
}

pub fn donnees_message(message: &Message, suite: bool, c: &ContexteLignes) -> DonneesMessage {
    let id = id_message(message).to_string();
    let utilisateur = auteur(message).to_string();
    let pseudo = message
        .author
        .as_ref()
        .map(|p| texte(&p.pseudo).to_string())
        .unwrap_or_default();
    let detail = c.serveur.and_then(|s| c.etat.details.get(s));
    let couleur = couleur_du_pseudo(detail, membre(c.etat, c.serveur, &utilisateur))
        .filter(|c| analyser_couleur(c).is_some())
        .map(str::to_string);
    let systeme = message.system == Some(true);
    let moi = c.etat.moi();
    let cree = texte(&message.created_at);
    let local = est_local(&id);
    let droits = droits_du_message(
        &Sujet {
            auteur: &utilisateur,
            moi,
            systeme,
            local,
            sondage: message.poll.is_some(),
        },
        c.droits,
    );
    let mut annexes = DonneesAnnexes {
        lecteurs: c
            .lecteurs
            .get(&id)
            .map(|l| lecteurs(l, c))
            .unwrap_or_default(),
        ..annexes(message, moi, c.maintenant_ms, c.apercus)
    };
    if let Some(sondage) = annexes.sondage.as_mut() {
        completer_sondage(sondage, droits, local, c.votants.get(&id));
    }
    DonneesMessage {
        initiales: initiales(&pseudo),
        teinte: teinte(if utilisateur.is_empty() {
            &pseudo
        } else {
            &utilisateur
        }),
        couleur,
        avatar: cle_avatar_du_message(c.etat, c.serveur, &utilisateur),
        bot: est_compte_estompe(&utilisateur),
        heure: heure_de(cree, c.decalage_minutes).unwrap_or_default(),
        date_complete: date_heure_complete(cree, c.decalage_minutes).unwrap_or_default(),
        suite: suite && !systeme,
        systeme,
        epingle: message.pinned_at.is_some(),
        modifie: message.edited_at.is_some(),
        date_modification: message
            .edited_at
            .as_deref()
            .and_then(|d| date_heure_complete(d, c.decalage_minutes))
            .unwrap_or_default(),
        mention: utilisateur != moi
            && !systeme
            && !moi.is_empty()
            && message.mentions.iter().flatten().any(|m| m == moi),
        mis_en_evidence: c.mis_en_evidence == Some(id.as_str()),
        reponse: message.reply_to.as_ref().map(|r| {
            let cible = texte(&r.id).to_string();
            DonneesReponse {
                supprimee: c.supprimes.contains(&cible),
                auteur: r
                    .author
                    .as_ref()
                    .map(|p| texte(&p.pseudo).to_string())
                    .unwrap_or_default(),
                texte: une_ligne(&estompe_client::markdown::texte_brut(
                    &estompe_client::markdown::analyser(
                        texte(&r.content),
                        &Mentionnables::default(),
                    )
                    .blocs,
                )),
                id: cible,
            }
        }),
        contenu: texte(&message.content).to_string(),
        annexes,
        envoi: c
            .envois
            .iter()
            .find(|e| e.id_local() == id)
            .map(|e| e.etat.clone()),
        droits,
        auteur: pseudo,
        utilisateur,
        id,
    }
}

pub fn lignes_de_conversation(messages: &[Message], c: &ContexteLignes) -> Vec<DonneesLigne> {
    lignes_de_messages(messages, c.nouveau_depuis, c.decalage_minutes)
        .into_iter()
        .map(|ligne| match ligne {
            Ligne::Jour { cle, libelle } => DonneesLigne {
                cle,
                genre: GenreLigne::Jour,
                libelle: majuscule(&libelle),
                message: None,
            },
            Ligne::Nouveau { cle } => DonneesLigne {
                cle,
                genre: GenreLigne::Nouveau,
                libelle: "Nouveaux messages".to_string(),
                message: None,
            },
            Ligne::Message { cle, index, suite } => DonneesLigne {
                cle,
                genre: GenreLigne::Message,
                libelle: String::new(),
                message: Some(donnees_message(&messages[index], suite, c)),
            },
        })
        .collect()
}

fn majuscule(texte: &str) -> String {
    let mut caracteres = texte.chars();
    match caracteres.next() {
        Some(premier) => premier.to_uppercase().chain(caracteres).collect(),
        None => String::new(),
    }
}

pub fn premier_non_lu(messages: &[Message], non_lus: i64, moi: &str) -> Option<String> {
    if non_lus <= 0 {
        return None;
    }
    let mut restants = non_lus;
    let mut premier = None;
    for message in messages.iter().rev() {
        if auteur(message) == moi {
            continue;
        }
        premier = Some(id_message(message).to_string());
        restants -= 1;
        if restants == 0 {
            break;
        }
    }
    premier
}

pub fn mentionnables(etat: &Etat, cle: &CleConversation, emojis: &[String]) -> Mentionnables {
    let mut personnes = etat.pseudos_de(Some(cle));
    let mut roles = Vec::new();
    let mut groupes = Vec::new();
    if let Some((detail, _)) = cle.salon().and_then(|s| etat.salon(s)) {
        for m in detail.members.as_deref().unwrap_or_default() {
            if let Some(surnom) = m.nickname.as_deref().filter(|s| !s.is_empty()) {
                personnes.push(surnom.to_string());
            }
        }
        roles = detail
            .roles
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|r| r.is_default != Some(true))
            .map(|r| texte(&r.name).to_string())
            .collect();
        groupes = vec!["everyone".to_string(), "here".to_string()];
    }
    Mentionnables::nouveau(personnes, roles, groupes, emojis)
}

pub fn utilisateur_mentionne(etat: &Etat, cle: &CleConversation, nom: &str) -> Option<String> {
    let nom = nom.to_lowercase();
    match cle {
        CleConversation::Salon(salon) => etat.salon(salon).and_then(|(detail, _)| {
            detail
                .members
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|m| {
                    texte(&m.pseudo).to_lowercase() == nom
                        || m.nickname.as_deref().map(str::to_lowercase).as_deref()
                            == Some(nom.as_str())
                })
                .map(|m| texte(&m.id).to_string())
        }),
        CleConversation::Prive(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;
    use serde_json::json;

    fn message(id: &str, auteur: &str, heure: &str, contenu: &str) -> Message {
        serde_json::from_value(json!({
            "id": id, "channel_id": "c1",
            "author": {"id": auteur, "pseudo": auteur},
            "content": contenu,
            "created_at": format!("2026-11-19T{heure}:00Z"),
        }))
        .unwrap()
    }

    fn contexte<'a>(etat: &'a Etat, supprimes: &'a HashSet<String>) -> ContexteLignes<'a> {
        ContexteLignes {
            etat,
            serveur: Some("s1"),
            nouveau_depuis: None,
            mis_en_evidence: None,
            supprimes,
            decalage_minutes: 60,
            maintenant_ms: MAINTENANT,
            apercus: true,
            lecteurs: &AUCUN_LECTEUR,
            envois: &[],
            droits: DroitsConversation::default(),
            votants: &AUCUN_VOTANT,
        }
    }

    static AUCUN_VOTANT: std::sync::LazyLock<HashMap<String, Votants>> =
        std::sync::LazyLock::new(HashMap::new);

    static AUCUN_LECTEUR: BTreeMap<String, Vec<Lecture>> = BTreeMap::new();

    const MAINTENANT: i64 = 1_795_600_000_000;

    #[test]
    fn vu_par_s_attache_au_dernier_message_lu_avec_l_heure_de_lecture() {
        let e = etat();
        let supprimes = HashSet::new();
        let lecture = |u: &str, p: &str| Lecture {
            channel_id: Some("c1".into()),
            dm_id: None,
            user_id: u.into(),
            pseudo: p.into(),
            last_id: "2".into(),
            read_at: "2026-11-19T09:05:00Z".into(),
        };
        let lus = BTreeMap::from([(
            "2".to_string(),
            vec![lecture("dave", "dave"), lecture("eve", "Ève")],
        )]);
        let c = ContexteLignes {
            lecteurs: &lus,
            ..contexte(&e, &supprimes)
        };
        let messages = vec![
            message("1", "carol", "09:00", "a"),
            message("2", "carol", "09:01", "b"),
        ];
        let lignes = lignes_de_conversation(&messages, &c);
        let m = |id: &str| {
            lignes
                .iter()
                .filter_map(|l| l.message.as_ref())
                .find(|m| m.id == id)
                .unwrap()
                .clone()
        };
        assert!(m("1").annexes.lecteurs.is_empty());
        let vus = m("2").annexes.lecteurs;
        let resume: Vec<(&str, &str, &str)> = vus
            .iter()
            .map(|l| (l.pseudo.as_str(), l.initiales.as_str(), l.heure.as_str()))
            .collect();
        assert_eq!(resume, [("dave", "DA", "10:05"), ("Ève", "ÈV", "10:05")]);
        assert!(!m("2").annexes.est_vide());
        assert_eq!(m("2").annexes.resume(), "Vu par dave, Ève");
    }

    #[test]
    fn regroupe_les_messages_consecutifs_et_separe_les_jours() {
        let e = etat();
        let supprimes = HashSet::new();
        let mut hier = message("1", "carol", "09:00", "hier");
        hier.created_at = Some("2026-11-18T09:00:00Z".into());
        let messages = vec![
            hier,
            message("2", "carol", "09:00", "a"),
            message("3", "carol", "09:02", "b"),
            message("4", "dave", "09:03", "c"),
            message("5", "dave", "09:30", "d"),
        ];
        let lignes = lignes_de_conversation(&messages, &contexte(&e, &supprimes));
        let resume: Vec<(GenreLigne, bool)> = lignes
            .iter()
            .map(|l| (l.genre, l.message.as_ref().is_some_and(|m| m.suite)))
            .collect();
        assert_eq!(
            resume,
            [
                (GenreLigne::Jour, false),
                (GenreLigne::Message, false),
                (GenreLigne::Jour, false),
                (GenreLigne::Message, false),
                (GenreLigne::Message, true),
                (GenreLigne::Message, false),
                (GenreLigne::Message, false),
            ]
        );
        assert_eq!(lignes[0].libelle, "Mercredi 18 novembre 2026");
        assert_eq!(lignes[3].message.as_ref().unwrap().heure, "10:00");
    }

    #[test]
    fn la_ligne_nouveau_coupe_le_groupe_et_le_premier_non_lu_ignore_mes_messages() {
        let e = etat();
        let supprimes = HashSet::new();
        let messages = vec![
            message("1", "carol", "09:00", "a"),
            message("2", "carol", "09:01", "b"),
            message("3", "me", "09:02", "moi"),
            message("4", "carol", "09:03", "c"),
        ];
        assert_eq!(premier_non_lu(&messages, 2, "me").as_deref(), Some("2"));
        assert_eq!(premier_non_lu(&messages, 0, "me"), None);
        assert_eq!(premier_non_lu(&messages, 99, "me").as_deref(), Some("1"));
        let mut c = contexte(&e, &supprimes);
        c.nouveau_depuis = Some("2");
        let lignes = lignes_de_conversation(&messages, &c);
        let cles: Vec<&str> = lignes.iter().map(|l| l.cle.as_str()).collect();
        assert_eq!(cles, ["day-1", "1", "new-2", "2", "3", "4"]);
        assert!(!lignes[3].message.as_ref().unwrap().suite);
    }

    #[test]
    fn chaque_variante_du_message_est_projetee() {
        let mut e = etat();
        e.details.get_mut("s1").unwrap().roles = Some(
            serde_json::from_value(
                json!([{"id": "r1", "name": "Art", "color": "#b0413e", "position": 1}]),
            )
            .unwrap(),
        );
        e.details.get_mut("s1").unwrap().members = Some(
            serde_json::from_value(json!([{"id": "carol", "pseudo": "carol", "roles": ["r1"]}]))
                .unwrap(),
        );
        let supprimes = HashSet::from(["70".to_string()]);
        let mut m: Message = serde_json::from_value(json!({
            "id": "80", "channel_id": "c1",
            "author": {"id": "carol", "pseudo": "carol"},
            "content": "@bob regarde",
            "created_at": "2026-11-19T09:00:00Z",
            "edited_at": "2026-11-19T09:05:00Z",
            "pinned_at": "2026-11-19T09:06:00Z",
            "mentions": ["me"],
            "reply_to": {"id": "70", "author": {"id": "dave", "pseudo": "dave"}, "content": "texte\nsur deux lignes"},
            "attachments": [{"id": "a"}, {"id": "b"}],
            "reactions": [{"emoji": "👍", "count": 3, "me": true}, {"emoji": "🎉", "count": 0}]
        }))
        .unwrap();
        let c = contexte(&e, &supprimes);
        let d = donnees_message(&m, false, &c);
        assert_eq!(d.couleur.as_deref(), Some("#b0413e"));
        assert!(d.modifie && d.epingle && d.mention && !d.bot && !d.systeme);
        assert_eq!(d.date_modification, "19/11/2026 10:05:00");
        let reponse = d.reponse.unwrap();
        assert_eq!(
            (
                reponse.auteur.as_str(),
                reponse.texte.as_str(),
                reponse.supprimee
            ),
            ("dave", "texte sur deux lignes", true)
        );
        assert_eq!(d.annexes.resume(), "2 pièces jointes · 👍 3");
        m.system = Some(true);
        m.author = serde_json::from_value(
            json!({"id": estompe_client::store::messages::ID_COMPTE_ESTOMPE, "pseudo": "Estompe"}),
        )
        .unwrap();
        let systeme = donnees_message(&m, true, &c);
        assert!(systeme.systeme && systeme.bot && !systeme.suite && !systeme.mention);
        let mut evidence = contexte(&e, &supprimes);
        evidence.mis_en_evidence = Some("80");
        assert!(donnees_message(&m, false, &evidence).mis_en_evidence);
    }

    #[test]
    fn pieces_jointes_classees_avec_nom_sur_et_taille_lisible() {
        let m: Message = serde_json::from_value(json!({
            "id": "1", "channel_id": "c1", "author": {"id": "carol", "pseudo": "carol"},
            "content": "voir https://plbls.fr/page et https://autre.fr",
            "attachments": [
                {"id": "a", "name": "photo.png", "mime": "image/png", "size": 2048},
                {"id": "b", "name": "anim.gif", "mime": "image/gif", "size": 10},
                {"id": "c", "name": "..\\CON.txt", "mime": "text/plain", "size": 3_500_000},
                {"id": "d", "name": "vieux.zip", "mime": "application/zip", "expired": true},
                {"id": "e", "name": "voix.ogg", "kind": "voice", "transcript": "Bonjour", "transcript_auto": true},
                {"id": "f", "name": "clip.mp4", "mime": "video/mp4", "transcript": "ignoré"}
            ],
            "reactions": [{"emoji": "👍", "count": 2, "me": true, "users": [{"id": "me", "pseudo": "moi"}, {"id": "bob", "pseudo": "bob"}]}]
        }))
        .unwrap();
        let a = annexes(&m, "me", MAINTENANT, true);
        let resume: Vec<(&str, GenrePiece, &str, bool)> = a
            .pieces
            .iter()
            .map(|p| (p.id.as_str(), p.genre, p.taille.as_str(), p.anime))
            .collect();
        assert_eq!(
            resume,
            [
                ("a", GenrePiece::Image, "2.0 Ko", false),
                ("b", GenrePiece::Image, "10 o", true),
                ("c", GenrePiece::Fichier, "3.3 Mo", false),
                ("d", GenrePiece::Expiree, "0 o", false),
                ("e", GenrePiece::Media, "0 o", false),
                ("f", GenrePiece::Media, "0 o", false),
            ]
        );
        assert_eq!(a.pieces[2].nom_sur, "_CON.txt");
        assert_eq!(a.pieces[4].transcription.as_deref(), Some("Bonjour"));
        assert!(a.pieces[4].transcription_auto);
        assert_eq!(a.pieces[5].transcription, None);
        assert_eq!(a.images().count(), 2);
        assert_eq!(a.lien.as_deref(), Some("https://plbls.fr/page"));
        assert_eq!(a.reactions[0].titre, "Toi et bob ont réagi avec 👍");
        assert_eq!(annexes(&m, "me", MAINTENANT, false).lien, None);
    }

    #[test]
    fn un_sondage_remplace_pieces_et_apercu_et_passe_clos_a_l_echeance() {
        let fin = "2026-11-25T10:00:00Z";
        let fin_ms = estompe_client::roles::date_en_millisecondes(fin).unwrap();
        let m: Message = serde_json::from_value(json!({
            "id": "1", "channel_id": "c1", "author": {"id": "carol", "pseudo": "carol"},
            "content": "On mange où ? https://plbls.fr",
            "attachments": [{"id": "a", "name": "x.png", "mime": "image/png"}],
            "poll": {"multiple": false, "closes_at": fin, "created_by": {"id": "carol", "pseudo": "carol"},
                "options": [{"id": "o1", "label": "Pizza", "votes": 3, "me": true}, {"id": "o2", "label": "Sushi", "votes": 1}]}
        }))
        .unwrap();
        let ouvert = annexes(&m, "me", fin_ms - 2 * 3_600_000, true);
        assert!(ouvert.pieces.is_empty() && ouvert.lien.is_none());
        let s = ouvert.sondage.unwrap();
        assert!(!s.clos && s.fin_ms == Some(fin_ms));
        assert_eq!(
            s.options
                .iter()
                .map(|o| (o.pourcentage, o.moi, o.gagnant))
                .collect::<Vec<_>>(),
            [(75, true, false), (25, false, false)]
        );
        assert_eq!(s.pied, "4 votes · réponse unique · reste 2 h");
        let clos = annexes(&m, "me", fin_ms, true).sondage.unwrap();
        assert!(clos.clos && clos.fin_ms.is_none());
        assert!(clos.options[0].gagnant && !clos.options[1].gagnant);
        assert_eq!(clos.pied, "4 votes · réponse unique · Clos");
    }

    #[test]
    fn un_sondage_porte_le_vote_la_gestion_et_les_votants_affiches() {
        let e = etat();
        let supprimes = HashSet::new();
        let sondage = |id: &str, auteur: &str| -> Message {
            serde_json::from_value(json!({
                "id": id, "channel_id": "c1", "author": {"id": auteur, "pseudo": auteur}, "content": "Q ?",
                "poll": {"multiple": false, "closes_at": null, "created_by": {"id": auteur, "pseudo": auteur},
                    "options": [{"id": "o1", "label": "a", "votes": 2}, {"id": "o2", "label": "b", "votes": 0}]}
            }))
            .unwrap()
        };
        let votants = HashMap::from([(
            "1".to_string(),
            Votants::from([(
                "o1".to_string(),
                serde_json::from_value::<Vec<Person>>(
                    json!([{"id": "bob", "pseudo": "bob"}, {"id": "eve", "pseudo": "Ève"}]),
                )
                .unwrap(),
            )]),
        )]);
        let c = ContexteLignes {
            votants: &votants,
            ..contexte(&e, &supprimes)
        };
        let d = |m: &Message| donnees_message(m, false, &c).annexes.sondage.unwrap();
        let autre = d(&sondage("1", "carol"));
        assert!(autre.votable && !autre.gerer && autre.votants_ouverts && autre.total == 2);
        assert_eq!(autre.options[0].votants.as_deref(), Some("bob, Ève"));
        assert_eq!(autre.options[1].votants.as_deref(), Some(""));
        let mien = d(&sondage("2", e.moi()));
        assert!(mien.gerer && !mien.votants_ouverts && mien.options[0].votants.is_none());
        assert!(!d(&sondage("local:1", e.moi())).votable);
    }

    #[test]
    fn mentionnables_et_resolution_d_une_mention() {
        let mut e = etat();
        e.details.get_mut("s1").unwrap().members = Some(
            serde_json::from_value(json!([{"id": "carol", "pseudo": "carol", "nickname": "Caro"}]))
                .unwrap(),
        );
        let cle = CleConversation::Salon("c1".into());
        let contexte = mentionnables(&e, &cle, &["chat".to_string()]);
        let rendu = estompe_client::markdown::analyser("@Caro @everyone :chat:", &contexte);
        assert_eq!(rendu.blocs.len(), 1);
        let estompe_client::markdown::Bloc::Paragraphe(segments) = &rendu.blocs[0] else {
            panic!("paragraphe attendu");
        };
        assert!(segments
            .iter()
            .any(|s| s.contenu == estompe_client::markdown::Contenu::Emoji("chat".into())));
        assert_eq!(
            utilisateur_mentionne(&e, &cle, "caro").as_deref(),
            Some("carol")
        );
        assert_eq!(
            utilisateur_mentionne(&e, &cle, "CAROL").as_deref(),
            Some("carol")
        );
        assert_eq!(utilisateur_mentionne(&e, &cle, "personne"), None);
    }
}
