use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ordre_serveurs::{ordonner_serveurs, voisin, Sens};
use crate::regarder_ensemble::RegistreSeances;
use crate::roles::{date_en_millisecondes, Detail, Membre};
use crate::temps_reel::evenements::StatutPresence;
use crate::types::{
    Channel, ChannelKind, GetMeReponse, GetServersIdReponseChannelsElement,
    GetServersIdReponseChannelsElementKind, Mention, NotifySettings, NotifySettingsTypes, Server,
    DM,
};
use crate::videos::TableauVideos;

pub type Salon = GetServersIdReponseChannelsElement;
pub type GenreSalon = GetServersIdReponseChannelsElementKind;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CleConversation {
    Salon(String),
    Prive(String),
}

impl CleConversation {
    pub fn analyser(texte: &str) -> Option<Self> {
        if let Some(id) = texte.strip_prefix("c:") {
            return Some(Self::Salon(id.to_string()));
        }
        texte
            .strip_prefix("d:")
            .map(|id| Self::Prive(id.to_string()))
    }

    pub fn de(salon: Option<&str>, prive: Option<&str>) -> Option<Self> {
        match (salon.filter(|s| !s.is_empty()), prive) {
            (Some(salon), _) => Some(Self::Salon(salon.to_string())),
            (None, Some(prive)) => Some(Self::Prive(prive.to_string())),
            (None, None) => None,
        }
    }

    pub fn de_mention(mention: &Mention) -> Option<Self> {
        Self::de(mention.channel_id.as_deref(), mention.dm_id.as_deref())
    }

    pub fn salon(&self) -> Option<&str> {
        match self {
            Self::Salon(id) => Some(id),
            Self::Prive(_) => None,
        }
    }
}

impl fmt::Display for CleConversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Salon(id) => write!(f, "c:{id}"),
            Self::Prive(id) => write!(f, "d:{id}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParticipantVocal {
    pub utilisateur: String,
    pub pseudo: String,
    pub depuis: Option<String>,
    pub ecran: bool,
    pub camera: bool,
    pub micro_coupe: bool,
    pub son_coupe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    HorsLigne,
    EnLigne,
    Absent,
    NePasDeranger,
}

pub fn reglages_par_defaut() -> NotifySettings {
    NotifySettings {
        levels: Some(BTreeMap::new()),
        types: Some(NotifySettingsTypes {
            dm: Some(true),
            mention: Some(true),
            channel: Some(true),
            voice: Some(true),
        }),
        preview: Some(true),
        sound: Some(true),
        ignore_group: None,
        general: None,
        mutes: None,
    }
}

pub fn texte_du_genre(genre: Option<GenreSalon>) -> &'static str {
    match genre {
        Some(GenreSalon::Text) => "text",
        Some(GenreSalon::Voice) => "voice",
        Some(GenreSalon::Videos) => "videos",
        Some(GenreSalon::Music) => "music",
        Some(GenreSalon::Cinema) => "cinema",
        Some(GenreSalon::Inconnu) | None => "",
    }
}

pub fn est_vocal(genre: Option<GenreSalon>) -> bool {
    matches!(genre, Some(GenreSalon::Voice | GenreSalon::Cinema))
}

pub fn salon_depuis(salon: &Channel) -> Salon {
    let genre = salon.kind.map(|k| match k {
        ChannelKind::Text => GenreSalon::Text,
        ChannelKind::Voice => GenreSalon::Voice,
        ChannelKind::Videos => GenreSalon::Videos,
        ChannelKind::Music => GenreSalon::Music,
        ChannelKind::Cinema => GenreSalon::Cinema,
        ChannelKind::Inconnu => GenreSalon::Inconnu,
    });
    Salon {
        id: salon.id.clone(),
        name: salon.name.clone(),
        kind: genre,
        position: salon.position,
        topic: salon.topic.clone(),
        slowmode: salon.slowmode,
        locked: salon.locked,
        user_limit: salon.user_limit,
        permissions: None,
    }
}

pub fn trier_salons(salons: &mut [Salon]) {
    salons.sort_by(|a, b| {
        let (ga, gb) = (texte_du_genre(a.kind), texte_du_genre(b.kind));
        gb.cmp(ga)
            .then(a.position.unwrap_or(0).cmp(&b.position.unwrap_or(0)))
    });
}

pub(crate) fn id_de(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Etat {
    pub compte: GetMeReponse,
    pub serveurs: Vec<Server>,
    pub details: BTreeMap<String, Detail>,
    pub messages_prives: Vec<DM>,
    pub en_ligne: BTreeSet<String>,
    pub statuts: BTreeMap<String, StatutPresence>,
    pub activites: BTreeMap<String, String>,
    pub mentions: Vec<Mention>,
    pub non_lus: BTreeMap<CleConversation, i64>,
    pub reglages_notification: NotifySettings,
    pub vocal: BTreeMap<String, Vec<ParticipantVocal>>,
    pub avatars: BTreeMap<String, i64>,
    pub avatars_serveur: BTreeMap<String, BTreeMap<String, i64>>,
    pub courante: Option<CleConversation>,
    pub focus: bool,
    pub en_bas: bool,
    pub activite_locale: String,
    pub ordre_serveurs: Vec<String>,
    pub seances: RegistreSeances,
    pub videos: BTreeMap<String, TableauVideos>,
}

impl Default for Etat {
    fn default() -> Self {
        Self {
            compte: GetMeReponse::default(),
            serveurs: Vec::new(),
            details: BTreeMap::new(),
            messages_prives: Vec::new(),
            en_ligne: BTreeSet::new(),
            statuts: BTreeMap::new(),
            activites: BTreeMap::new(),
            mentions: Vec::new(),
            non_lus: BTreeMap::new(),
            reglages_notification: reglages_par_defaut(),
            vocal: BTreeMap::new(),
            avatars: BTreeMap::new(),
            avatars_serveur: BTreeMap::new(),
            courante: None,
            focus: true,
            en_bas: true,
            activite_locale: String::new(),
            ordre_serveurs: Vec::new(),
            seances: RegistreSeances::default(),
            videos: BTreeMap::new(),
        }
    }
}

impl Etat {
    pub fn avec_ordre_serveurs(ordre: &[String]) -> Self {
        Self {
            ordre_serveurs: ordre.to_vec(),
            ..Self::default()
        }
    }

    pub fn moi(&self) -> &str {
        id_de(&self.compte.id)
    }

    pub fn pseudo(&self) -> &str {
        id_de(&self.compte.pseudo)
    }

    pub fn bloc_actif(&self, nom: &str) -> bool {
        self.compte
            .blocks
            .as_ref()
            .is_none_or(|blocs| blocs.iter().any(|b| b == nom))
    }

    pub fn capacite(&self, nom: &str) -> bool {
        let Some(capacites) = self.compte.capabilities.as_ref() else {
            return true;
        };
        let Some(c) = capacites.get(&format!("estompe:{nom}")) else {
            return false;
        };
        let contient = |liste: &Option<Vec<String>>| {
            liste
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|r| r == "*")
        };
        !contient(&c.deny) && contient(&c.allow)
    }

    pub fn serveur(&self, id: &str) -> Option<&Server> {
        self.serveurs.iter().find(|s| id_de(&s.id) == id)
    }

    pub fn serveurs_ordonnes(&self) -> Vec<&Server> {
        ordonner_serveurs(self.serveurs.iter().collect(), &self.ordre_serveurs, |s| {
            id_de(&s.id)
        })
    }

    pub fn ids_serveurs_ordonnes(&self) -> Vec<String> {
        self.serveurs_ordonnes()
            .into_iter()
            .map(|s| id_de(&s.id).to_string())
            .collect()
    }

    pub fn serveur_voisin(&self, courant: Option<&str>, sens: Sens) -> Option<String> {
        let ids: Vec<String> = std::iter::once(String::new())
            .chain(self.ids_serveurs_ordonnes())
            .collect();
        voisin(&ids, courant.unwrap_or_default(), sens).map(str::to_string)
    }

    pub fn salons(&self, serveur: &str) -> &[Salon] {
        self.details
            .get(serveur)
            .and_then(|d| d.channels.as_deref())
            .unwrap_or_default()
    }

    pub fn membres(&self, serveur: &str) -> &[Membre] {
        self.details
            .get(serveur)
            .and_then(|d| d.members.as_deref())
            .unwrap_or_default()
    }

    pub fn membre(&self, serveur: &str, utilisateur: &str) -> Option<&Membre> {
        self.membres(serveur)
            .iter()
            .find(|m| id_de(&m.id) == utilisateur)
    }

    pub fn salon(&self, salon: &str) -> Option<(&Detail, &Salon)> {
        self.details.values().find_map(|d| {
            d.channels
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|c| id_de(&c.id) == salon)
                .map(|c| (d, c))
        })
    }

    pub fn serveur_du_salon(&self, salon: &str) -> Option<String> {
        self.details.iter().find_map(|(id, d)| {
            d.channels
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|c| id_de(&c.id) == salon)
                .then(|| id.clone())
        })
    }

    pub fn serveur_de(&self, conversation: &CleConversation) -> Option<String> {
        conversation
            .salon()
            .and_then(|salon| self.serveur_du_salon(salon))
    }

    pub fn presence_de(&self, utilisateur: &str) -> Presence {
        if utilisateur != self.moi() && !self.en_ligne.contains(utilisateur) {
            return Presence::HorsLigne;
        }
        match self.statuts.get(utilisateur) {
            Some(StatutPresence::Idle) => Presence::Absent,
            Some(StatutPresence::Dnd) => Presence::NePasDeranger,
            _ => Presence::EnLigne,
        }
    }

    pub fn activite_de(&self, utilisateur: &str) -> &str {
        if utilisateur == self.moi() {
            return &self.activite_locale;
        }
        if !self.en_ligne.contains(utilisateur) {
            return "";
        }
        self.activites
            .get(utilisateur)
            .map(String::as_str)
            .unwrap_or_default()
    }

    pub fn mentions_comptees(&self) -> impl Iterator<Item = &Mention> {
        let ignores = self
            .reglages_notification
            .ignore_group
            .as_deref()
            .unwrap_or_default();
        self.mentions.iter().filter(move |m| {
            let serveur = m.server_id.as_deref().filter(|s| !s.is_empty());
            !(m.group == Some(true) && serveur.is_some_and(|s| ignores.iter().any(|i| i == s)))
        })
    }

    pub fn nombre_mentions(&self, conversation: &CleConversation) -> usize {
        self.mentions_comptees()
            .filter(|m| CleConversation::de_mention(m).as_ref() == Some(conversation))
            .count()
    }

    pub fn mentions_du_serveur(&self, serveur: &str) -> usize {
        self.mentions_comptees()
            .filter(|m| m.server_id.as_deref() == Some(serveur))
            .count()
    }

    pub fn total_mentions(&self) -> usize {
        self.mentions_comptees().count()
    }

    pub fn non_lus_de(&self, conversation: &CleConversation) -> i64 {
        self.non_lus.get(conversation).copied().unwrap_or(0)
    }

    pub fn non_lus_du_serveur(&self, serveur: &str) -> i64 {
        self.salons(serveur)
            .iter()
            .map(|c| self.non_lus_de(&CleConversation::Salon(id_de(&c.id).to_string())))
            .sum()
    }

    pub fn peut(&self, serveur: Option<&str>, permission: &str, salon: Option<&str>) -> bool {
        let Some(detail) = serveur.and_then(|s| self.details.get(s)) else {
            return true;
        };
        let salon = salon.and_then(|id| {
            detail
                .channels
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|c| id_de(&c.id) == id)
        });
        salon
            .and_then(|c| c.permissions.as_ref())
            .or(detail.permissions.as_ref())
            .is_none_or(|p| p.iter().any(|x| x == permission))
    }

    pub fn peut_dans(&self, conversation: Option<&CleConversation>, permission: &str) -> bool {
        let Some(salon) = conversation.and_then(CleConversation::salon) else {
            return true;
        };
        match self.serveur_du_salon(salon) {
            Some(serveur) => self.peut(Some(&serveur), permission, Some(salon)),
            None => true,
        }
    }

    fn detail_de(&self, conversation: Option<&CleConversation>) -> Option<&Detail> {
        let salon = conversation.and_then(CleConversation::salon)?;
        self.salon(salon).map(|(detail, _)| detail)
    }

    pub fn mentions_de_groupe(&self, conversation: Option<&CleConversation>) -> Vec<String> {
        let Some(detail) = self.detail_de(conversation) else {
            return Vec::new();
        };
        let roles = detail
            .roles
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|r| r.is_default != Some(true))
            .map(|r| id_de(&r.name).to_string());
        ["everyone".to_string(), "here".to_string()]
            .into_iter()
            .chain(roles)
            .collect()
    }

    pub fn suggestions_de_groupe(&self, conversation: Option<&CleConversation>) -> Vec<String> {
        if self.peut_dans(conversation, "mention_everyone") {
            self.mentions_de_groupe(conversation)
        } else {
            Vec::new()
        }
    }

    pub fn touches_par_groupe(
        &self,
        conversation: Option<&CleConversation>,
        nom: &str,
    ) -> Option<usize> {
        let detail = self.detail_de(conversation)?;
        if nom != "everyone" && nom != "here" {
            return None;
        }
        let autres = detail
            .members
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|m| id_de(&m.id) != self.moi());
        Some(if nom == "everyone" {
            autres.count()
        } else {
            autres.filter(|m| m.online == Some(true)).count()
        })
    }

    pub fn pseudos_de(&self, conversation: Option<&CleConversation>) -> Vec<String> {
        match conversation {
            None => Vec::new(),
            Some(CleConversation::Prive(id)) => {
                let autre = self
                    .messages_prives
                    .iter()
                    .find(|d| id_de(&d.id) == id)
                    .and_then(|d| d.other.as_ref())
                    .map(|p| id_de(&p.pseudo).to_string());
                std::iter::once(self.pseudo().to_string())
                    .chain(autre)
                    .collect()
            }
            Some(CleConversation::Salon(salon)) => self
                .salon(salon)
                .map(|(detail, _)| {
                    detail
                        .members
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|m| id_de(&m.pseudo).to_string())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn participants(&self, salon: &str) -> &[ParticipantVocal] {
        self.vocal.get(salon).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn arrivee_la_plus_ancienne(&self, salon: &str) -> Option<i64> {
        self.participants(salon)
            .iter()
            .filter_map(|p| p.depuis.as_deref().and_then(date_en_millisecondes))
            .min()
    }

    pub fn arrivee_vocale(&self, serveur: &str, utilisateur: &str) -> Option<i64> {
        self.salons(serveur).iter().find_map(|c| {
            self.participants(id_de(&c.id))
                .iter()
                .find(|p| p.utilisateur == utilisateur)
                .and_then(|p| p.depuis.as_deref())
                .and_then(date_en_millisecondes)
        })
    }

    pub fn version_avatar(&self, utilisateur: &str) -> i64 {
        self.avatars.get(utilisateur).copied().unwrap_or(0)
    }

    pub fn version_avatar_du_serveur(&self, utilisateur: &str, serveur: Option<&str>) -> i64 {
        serveur
            .and_then(|s| self.avatars_serveur.get(s))
            .and_then(|v| v.get(utilisateur))
            .copied()
            .unwrap_or(0)
    }
}
