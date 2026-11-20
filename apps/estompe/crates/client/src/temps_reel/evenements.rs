use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::types::{
    Channel, ChannelKind, GetServersIdReponseMembersElement, Message, NotifySettings,
    PutMeStatusRequeteStatus,
};

pub type StatutPresence = PutMeStatusRequeteStatus;
pub type MembreArrivant = GetServersIdReponseMembersElement;

macro_rules! charge {
    ($nom:ident { $($champ:ident : $type:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $nom {
            $(pub $champ: $type,)*
        }
    };
}

macro_rules! valeurs {
    ($nom:ident { $($variante:ident = $texte:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $nom {
            $(#[serde(rename = $texte)] $variante,)*
            #[serde(other)]
            Inconnu,
        }
    };
}

charge!(Pret { user_id: String });
charge!(InstantanePresence {
    online: Vec<String>,
    activities: BTreeMap<String, String>,
});
charge!(AvatarModifie {
    user_id: String,
    version: i64,
});
charge!(Saisie {
    channel_id: Option<String>,
    dm_id: Option<String>,
    user_id: String,
    pseudo: String,
});
charge!(Lecture {
    channel_id: Option<String>,
    dm_id: Option<String>,
    user_id: String,
    pseudo: String,
    last_id: String,
    read_at: String,
});
charge!(StatutModifie {
    user_id: String,
    status: Option<StatutPresence>,
});
charge!(ActiviteModifiee {
    user_id: String,
    name: Option<String>,
});
valeurs!(GenreNotification {
    MessagePrive = "dm",
    Mention = "mention",
    Salon = "channel",
    Vocal = "voice",
});
valeurs!(PieceNotifiee {
    Image = "image",
    Fichier = "file",
});
charge!(NotificationServeur {
    title: String,
    body: String,
    tag: String,
    url: String,
    conv: Option<String>,
    msg: Option<String>,
    kind: Option<GenreNotification>,
    silent: bool,
    read: Option<String>,
    sender: Option<String>,
    room: Option<String>,
    text: Option<String>,
    attach: Option<PieceNotifiee>,
});
charge!(EmojisModifies { server_id: String });
charge!(Presence {
    user_id: String,
    online: bool,
});
charge!(MessageSupprime {
    id: String,
    channel_id: Option<String>,
    dm_id: Option<String>,
});
charge!(SondageVote {
    message_id: String,
    user_id: String,
    options: Vec<String>,
    counts: BTreeMap<String, i64>,
});
charge!(SondageClos {
    message_id: String,
    closes_at: String,
});
charge!(ReactionModifiee {
    message_id: String,
    emoji: String,
    user_id: String,
    pseudo: Option<String>,
    added: bool,
});

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MentionRecue {
    #[serde(flatten)]
    pub message: Message,
    pub group_mention: bool,
}

charge!(SalonCree {
    server_id: String,
    channel: Channel,
    after: Option<String>,
});
charge!(SalonModifie {
    server_id: String,
    id: String,
    name: String,
});
charge!(SalonsReordonnes {
    server_id: String,
    kind: Option<ChannelKind>,
    ids: Vec<String>,
});
charge!(BanniereModifiee {
    user_id: String,
    color: Option<String>,
});
charge!(SalonSupprime {
    server_id: String,
    id: String,
});
charge!(ServeurModifie {
    id: String,
    name: String,
});
charge!(IconeServeurModifiee {
    id: String,
    version: i64,
});
charge!(ServeurSupprime { id: String });
charge!(MembreArrive {
    server_id: String,
    member: MembreArrivant,
});
valeurs!(GenreDepart {
    Expulsion = "kick",
    Bannissement = "ban",
});
charge!(MembreParti {
    server_id: String,
    user_id: String,
    kind: Option<GenreDepart>,
    reason: Option<String>,
    until: Option<String>,
});
charge!(MembreExclu {
    server_id: String,
    user_id: String,
    until: Option<String>,
    reason: Option<String>,
});
charge!(MembreAverti {
    server_id: String,
    reason: Option<String>,
});
charge!(PseudoModifie {
    server_id: String,
    user_id: String,
    nickname: Option<String>,
});
charge!(MessagesSupprimes {
    channel_id: String,
    ids: Vec<String>,
});
charge!(VocalDeplace {
    server_id: String,
    from: String,
    to: String,
    by: String,
    to_name: String,
});
charge!(VocalDeconnecte {
    server_id: String,
    channel_id: String,
});
charge!(AvatarServeurModifie {
    server_id: String,
    user_id: String,
    version: i64,
});
charge!(RolesModifies {
    server_id: String,
    user_id: Option<String>,
});
charge!(EtatVocal {
    server_id: String,
    channel_id: String,
    user_id: String,
    pseudo: String,
    joined: bool,
    at: Option<String>,
});
charge!(ModerationVocale {
    server_id: String,
    channel_id: String,
    user_id: String,
    mic_off: bool,
    sound_off: bool,
});
valeurs!(SourcePartage {
    Ecran = "screen",
    Camera = "camera",
});
charge!(PartageVocal {
    server_id: String,
    channel_id: String,
    user_id: String,
    pseudo: Option<String>,
    source: Option<SourcePartage>,
    on: bool,
});
charge!(MetaMorceau {
    title: String,
    artist: String,
    album: String,
    duration: i64,
    cover: String,
});
charge!(ElementFile {
    id: String,
    provider: String,
    video: String,
    by: String,
    by_id: String,
    meta: Option<MetaMorceau>,
});
charge!(EtatSeance {
    rev: i64,
    provider: String,
    video: String,
    playing: bool,
    pos: f64,
    rate: f64,
    at: String,
    host: String,
    host_only: bool,
    by_id: String,
    by: String,
    act: String,
    queue: Vec<ElementFile>,
    ready: BTreeMap<String, bool>,
    waiting: Vec<String>,
    meta: Option<MetaMorceau>,
    repeat: String,
    history: Vec<ElementFile>,
});
charge!(Seance {
    server_id: String,
    channel_id: String,
    state: Option<EtatSeance>,
    now: Option<String>,
});
valeurs!(PhaseDessin {
    Salle = "lobby",
    Choix = "choosing",
    Dessin = "drawing",
    FinDeTour = "turn_end",
    Terminee = "ended",
});
charge!(ReglagesDessin {
    rounds: i64,
    turn_secs: i64,
    word_choices: i64,
    max_players: i64,
    lang: String,
    custom_only: bool,
    custom_count: i64,
});
charge!(JoueurDessin {
    id: String,
    pseudo: String,
    points: i64,
    turn_points: i64,
    connected: bool,
    spectator: bool,
    found: bool,
});
charge!(EtatPartieDessin {
    rev: i64,
    server_id: String,
    host: String,
    phase: Option<PhaseDessin>,
    settings: ReglagesDessin,
    players: Vec<JoueurDessin>,
    round: i64,
    drawer: String,
    deadline: Option<i64>,
    mask: String,
    word_len: i64,
    reveal: Option<String>,
    reason: String,
    draw_seq: i64,
    reports: i64,
    skip_votes: i64,
    winners: Vec<String>,
});
charge!(PartieDessin {
    server_id: String,
    channel_id: String,
    state: Option<EtatPartieDessin>,
    now: i64,
});
charge!(DessinPrive {
    server_id: String,
    channel_id: String,
    rev: i64,
    choices: Option<Vec<String>>,
    word: Option<String>,
});
valeurs!(GenreChatDessin {
    Proposition = "guess",
    Discussion = "chat",
    Trouve = "found",
    Proche = "close",
    DiscussionPrivee = "chat_private",
});
charge!(ChatDessin {
    server_id: String,
    channel_id: String,
    kind: Option<GenreChatDessin>,
    from: String,
    pseudo: String,
    text: String,
    at: i64,
});
charge!(TraitDessin {
    id: String,
    tool: String,
    color: String,
    size: i64,
    pts: Vec<[f64; 2]>,
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum OperationDessin {
    #[serde(rename = "stroke")]
    Trait(TraitDessin),
    #[serde(rename = "undo")]
    Annuler,
    #[serde(rename = "clear")]
    Effacer,
    #[serde(other)]
    Inconnue,
}

charge!(OperationsDessin {
    server_id: String,
    channel_id: String,
    seq: i64,
    ops: Vec<OperationDessin>,
});
charge!(VideosModifiees {
    server_id: String,
    channel_id: String,
});

macro_rules! evenements {
    ($($texte:literal => $variante:ident($charge:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum Evenement {
            $($variante($charge),)*
            Pong,
            Renouvele,
        }

        impl Evenement {
            pub const TYPES: &'static [&'static str] = &[$($texte,)* "pong", "renewed"];

            pub fn type_evenement(&self) -> &'static str {
                match self {
                    $(Self::$variante(_) => $texte,)*
                    Self::Pong => "pong",
                    Self::Renouvele => "renewed",
                }
            }

            fn depuis(type_evenement: &str, data: Value) -> Option<Result<Self, serde_json::Error>> {
                let data = if data.is_null() { Value::Object(Map::new()) } else { data };
                match type_evenement {
                    $($texte => Some(serde_json::from_value::<$charge>(data).map(Self::$variante)),)*
                    "pong" => Some(Ok(Self::Pong)),
                    "renewed" => Some(Ok(Self::Renouvele)),
                    _ => None,
                }
            }
        }
    };
}

evenements! {
    "ready" => Pret(Pret),
    "presence.snapshot" => InstantanePresence(InstantanePresence),
    "avatar.updated" => AvatarModifie(AvatarModifie),
    "typing" => Saisie(Saisie),
    "read.updated" => LectureModifiee(Lecture),
    "status.updated" => StatutModifie(StatutModifie),
    "activity.updated" => ActiviteModifiee(ActiviteModifiee),
    "notify_settings.updated" => ReglagesNotificationModifies(NotifySettings),
    "notification" => Notification(NotificationServeur),
    "emoji.updated" => EmojisModifies(EmojisModifies),
    "presence" => Presence(Presence),
    "message.created" => MessageCree(Message),
    "message.updated" => MessageModifie(Message),
    "message.deleted" => MessageSupprime(MessageSupprime),
    "poll.voted" => SondageVote(SondageVote),
    "poll.closed" => SondageClos(SondageClos),
    "reaction.updated" => ReactionModifiee(ReactionModifiee),
    "mention" => Mention(MentionRecue),
    "channel.created" => SalonCree(SalonCree),
    "channel.updated" => SalonModifie(SalonModifie),
    "channel.reordered" => SalonsReordonnes(SalonsReordonnes),
    "banner.updated" => BanniereModifiee(BanniereModifiee),
    "channel.deleted" => SalonSupprime(SalonSupprime),
    "server.updated" => ServeurModifie(ServeurModifie),
    "server_icon.updated" => IconeServeurModifiee(IconeServeurModifiee),
    "server.deleted" => ServeurSupprime(ServeurSupprime),
    "member.joined" => MembreArrive(MembreArrive),
    "member.left" => MembreParti(MembreParti),
    "member.timeout" => MembreExclu(MembreExclu),
    "member.warned" => MembreAverti(MembreAverti),
    "member.nickname" => PseudoModifie(PseudoModifie),
    "messages.deleted" => MessagesSupprimes(MessagesSupprimes),
    "voice.moved" => VocalDeplace(VocalDeplace),
    "voice.disconnected" => VocalDeconnecte(VocalDeconnecte),
    "server_avatar.updated" => AvatarServeurModifie(AvatarServeurModifie),
    "roles.updated" => RolesModifies(RolesModifies),
    "voice.state" => EtatVocal(EtatVocal),
    "voice.mod" => ModerationVocale(ModerationVocale),
    "voice.share" => PartageVocal(PartageVocal),
    "watch.state" => SeanceRegarder(Seance),
    "cinema.state" => SeanceCinema(Seance),
    "music.state" => SeanceMusique(Seance),
    "draw.state" => PartieDessin(PartieDessin),
    "draw.private" => DessinPrive(DessinPrive),
    "draw.chat" => ChatDessin(ChatDessin),
    "draw.ops" => OperationsDessin(OperationsDessin),
    "videos.changed" => VideosModifiees(VideosModifiees),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Trame {
    Evenement(Box<Evenement>),
    Inconnue(String),
    Illisible(String),
}

#[derive(Deserialize)]
struct Enveloppe {
    #[serde(rename = "type")]
    type_evenement: String,
    #[serde(default)]
    data: Value,
}

pub fn decoder(texte: &str) -> Trame {
    let enveloppe = match serde_json::from_str::<Enveloppe>(texte) {
        Ok(enveloppe) => enveloppe,
        Err(erreur) => return Trame::Illisible(erreur.to_string()),
    };
    match Evenement::depuis(&enveloppe.type_evenement, enveloppe.data) {
        Some(Ok(evenement)) => Trame::Evenement(Box::new(evenement)),
        Some(Err(erreur)) => Trame::Illisible(format!("{} : {erreur}", enveloppe.type_evenement)),
        None => Trame::Inconnue(enveloppe.type_evenement),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn evenement(trame: Value) -> Evenement {
        match decoder(&trame.to_string()) {
            Trame::Evenement(e) => *e,
            autre => panic!("trame non décodée : {autre:?}"),
        }
    }

    fn exemples() -> Vec<Value> {
        vec![
            json!({"type": "ready", "data": {"user_id": "u1"}}),
            json!({"type": "presence.snapshot", "data": {"online": ["u2"], "activities": {"u2": "Tetris"}}}),
            json!({"type": "avatar.updated", "data": {"user_id": "u2", "version": 3}}),
            json!({"type": "typing", "data": {"channel_id": "c1", "dm_id": null, "user_id": "u2", "pseudo": "bob"}}),
            json!({"type": "read.updated", "data": {"dm_id": "d1", "user_id": "u2", "pseudo": "bob", "last_id": "42", "read_at": "2026-01-01T00:00:00Z"}}),
            json!({"type": "status.updated", "data": {"user_id": "u2", "status": "dnd"}}),
            json!({"type": "activity.updated", "data": {"user_id": "u2", "name": null}}),
            json!({"type": "notify_settings.updated", "data": {"levels": {"s1": "mentions"}, "types": {"dm": true}, "preview": false, "sound": true}}),
            json!({"type": "notification", "data": {"title": "t", "body": "b", "tag": "msg-7", "url": "/#/s/s1/c1/7", "conv": "c:c1", "msg": "7", "kind": "mention", "silent": false, "sender": "alice", "room": "général", "text": "salut", "attach": "image"}}),
            json!({"type": "emoji.updated", "data": {"server_id": "s1"}}),
            json!({"type": "presence", "data": {"user_id": "u2", "online": true}}),
            json!({"type": "message.created", "data": {"id": "7", "channel_id": "c1", "content": "salut", "author": {"id": "u2", "pseudo": "bob"}}}),
            json!({"type": "message.updated", "data": {"id": "7", "channel_id": "c1", "content": "salut !", "edited_at": "2026-01-01T00:00:00Z"}}),
            json!({"type": "message.deleted", "data": {"id": "7", "channel_id": "c1", "dm_id": null}}),
            json!({"type": "poll.voted", "data": {"message_id": "7", "user_id": "u2", "options": ["o1"], "counts": {"o1": 1, "o2": 0}}}),
            json!({"type": "poll.closed", "data": {"message_id": "7", "closes_at": "2026-01-01T00:00:00Z"}}),
            json!({"type": "reaction.updated", "data": {"message_id": "7", "emoji": "👍", "user_id": "u2", "pseudo": "bob", "added": true}}),
            json!({"type": "mention", "data": {"id": "7", "channel_id": "c1", "content": "@everyone", "group_mention": true}}),
            json!({"type": "channel.created", "data": {"server_id": "s1", "channel": {"id": "c2", "name": "jeux", "kind": "voice"}, "after": "c1"}}),
            json!({"type": "channel.updated", "data": {"server_id": "s1", "id": "c2", "name": "jeux-video"}}),
            json!({"type": "channel.reordered", "data": {"server_id": "s1", "kind": "text", "ids": ["c2", "c1"]}}),
            json!({"type": "banner.updated", "data": {"user_id": "u2", "color": "#aabbcc"}}),
            json!({"type": "channel.deleted", "data": {"server_id": "s1", "id": "c2"}}),
            json!({"type": "server.updated", "data": {"id": "s1", "name": "Club"}}),
            json!({"type": "server_icon.updated", "data": {"id": "s1", "version": 0}}),
            json!({"type": "server.deleted", "data": {"id": "s1"}}),
            json!({"type": "member.joined", "data": {"server_id": "s1", "member": {"id": "u3", "pseudo": "carla", "online": true}}}),
            json!({"type": "member.left", "data": {"server_id": "s1", "user_id": "u3", "kind": "ban", "reason": "spam", "until": null}}),
            json!({"type": "member.timeout", "data": {"server_id": "s1", "user_id": "u3", "until": "2026-01-01T00:00:00Z", "reason": "calme"}}),
            json!({"type": "member.warned", "data": {"server_id": "s1", "reason": "attention"}}),
            json!({"type": "member.nickname", "data": {"server_id": "s1", "user_id": "u3", "nickname": ""}}),
            json!({"type": "messages.deleted", "data": {"channel_id": "c1", "ids": ["1", "2"]}}),
            json!({"type": "voice.moved", "data": {"server_id": "s1", "from": "v1", "to": "v2", "by": "modo", "to_name": "Salle 2"}}),
            json!({"type": "voice.disconnected", "data": {"server_id": "s1", "channel_id": "v1"}}),
            json!({"type": "server_avatar.updated", "data": {"server_id": "s1", "user_id": "u2", "version": 2}}),
            json!({"type": "roles.updated", "data": {"server_id": "s1", "user_id": "u2"}}),
            json!({"type": "voice.state", "data": {"server_id": "s1", "channel_id": "v1", "user_id": "u2", "pseudo": "bob", "joined": true, "at": "2026-01-01T00:00:00Z"}}),
            json!({"type": "voice.mod", "data": {"server_id": "s1", "channel_id": "v1", "user_id": "u2", "mic_off": true, "sound_off": false}}),
            json!({"type": "voice.share", "data": {"server_id": "s1", "channel_id": "v1", "user_id": "u2", "pseudo": "bob", "source": "screen", "on": true}}),
            json!({"type": "watch.state", "data": {"server_id": "s1", "channel_id": "v1", "now": "2026-01-01T00:00:01Z", "state": {
                "rev": 4, "provider": "youtube", "video": "abc", "playing": true, "pos": 12.5, "rate": 1.0, "at": "2026-01-01T00:00:00Z",
                "host": "u1", "host_only": false, "by_id": "u1", "by": "alice", "act": "play",
                "queue": [{"id": "q1", "provider": "youtube", "video": "def", "by": "bob", "by_id": "u2"}],
                "ready": {"u1": true}, "waiting": [], "repeat": "", "history": []}}}),
            json!({"type": "cinema.state", "data": {"server_id": "s1", "channel_id": "v3", "now": "2026-01-01T00:00:01Z", "state": {"rev": 1, "provider": "film", "video": "t1", "host_only": true, "meta": {"title": "Film", "duration": 5400}}}}),
            json!({"type": "music.state", "data": {"server_id": "s1", "channel_id": "v1", "now": "2026-01-01T00:00:01Z", "state": null}}),
            json!({"type": "draw.state", "data": {"server_id": "s1", "channel_id": "v1", "now": 1767225600000_i64, "state": {
                "rev": 2, "server_id": "s1", "host": "u1", "phase": "drawing", "settings": {"rounds": 3, "turn_secs": 80, "word_choices": 3, "max_players": 12, "lang": "fr", "custom_only": false, "custom_count": 0},
                "players": [{"id": "u1", "pseudo": "alice", "points": 10, "turn_points": 0, "connected": true, "spectator": false, "found": false}],
                "round": 1, "drawer": "u1", "deadline": 1767225680000_i64, "mask": "_ _ _", "word_len": 3, "reveal": null, "reason": "", "draw_seq": 5, "reports": 0, "skip_votes": 0, "winners": []}}}),
            json!({"type": "draw.private", "data": {"server_id": "s1", "channel_id": "v1", "rev": 2, "choices": ["chat", "chien"]}}),
            json!({"type": "draw.chat", "data": {"server_id": "s1", "channel_id": "v1", "kind": "found", "from": "u2", "pseudo": "bob", "text": "", "at": 1767225600000_i64}}),
            json!({"type": "draw.ops", "data": {"server_id": "s1", "channel_id": "v1", "seq": 6, "ops": [
                {"op": "stroke", "id": "t1", "tool": "pen", "color": "#000000", "size": 4, "pts": [[0.0, 0.0], [1.5, 2.5]]}, {"op": "undo"}, {"op": "clear"}, {"op": "laser"}]}}),
            json!({"type": "videos.changed", "data": {"server_id": "s1", "channel_id": "c4"}}),
            json!({"type": "pong", "data": null}),
            json!({"type": "renewed", "data": null}),
        ]
    }

    #[test]
    fn chaque_evenement_documente_se_decode_en_type_rust() {
        let exemples = exemples();
        let types: Vec<&str> = exemples
            .iter()
            .map(|e| evenement(e.clone()).type_evenement())
            .collect();
        assert_eq!(types, Evenement::TYPES);
        assert_eq!(
            exemples
                .iter()
                .map(|e| e["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            types
        );
    }

    #[test]
    fn chaque_evenement_de_realtime_md_a_sa_variante() {
        let doc = include_str!("../../../../../../docs/estompe/realtime.md");
        let section = doc.split("## Évènements").nth(1).unwrap();
        let documentes: Vec<&str> = section
            .lines()
            .filter_map(|l| l.strip_prefix("| `"))
            .filter_map(|l| l.split('`').next())
            .collect();
        assert!(documentes.len() > 40, "{documentes:?}");
        for type_evenement in documentes {
            assert!(
                Evenement::TYPES.contains(&type_evenement),
                "évènement documenté sans variante : {type_evenement}"
            );
        }
    }

    #[test]
    fn charges_typees() {
        let Evenement::Saisie(saisie) = evenement(exemples()[3].clone()) else {
            panic!()
        };
        assert_eq!(saisie.channel_id.as_deref(), Some("c1"));
        assert_eq!(saisie.dm_id, None);
        let Evenement::StatutModifie(statut) = evenement(exemples()[5].clone()) else {
            panic!()
        };
        assert_eq!(statut.status, Some(StatutPresence::Dnd));
        let Evenement::Mention(mention) = evenement(exemples()[17].clone()) else {
            panic!()
        };
        assert!(mention.group_mention);
        assert_eq!(mention.message.id.as_deref(), Some("7"));
        let Evenement::MembreParti(parti) = evenement(exemples()[27].clone()) else {
            panic!()
        };
        assert_eq!(parti.kind, Some(GenreDepart::Bannissement));
        let Evenement::SeanceRegarder(seance) = evenement(exemples()[39].clone()) else {
            panic!()
        };
        let etat = seance.state.unwrap();
        assert_eq!((etat.rev, etat.pos, etat.queue.len()), (4, 12.5, 1));
        let Evenement::SeanceMusique(musique) = evenement(exemples()[41].clone()) else {
            panic!()
        };
        assert_eq!(musique.state, None);
        let Evenement::PartieDessin(partie) = evenement(exemples()[42].clone()) else {
            panic!()
        };
        assert_eq!(partie.state.unwrap().phase, Some(PhaseDessin::Dessin));
        let Evenement::OperationsDessin(ops) = evenement(exemples()[45].clone()) else {
            panic!()
        };
        assert!(matches!(&ops.ops[0], OperationDessin::Trait(t) if t.pts[1] == [1.5, 2.5]));
        assert_eq!(
            ops.ops[1..],
            [
                OperationDessin::Annuler,
                OperationDessin::Effacer,
                OperationDessin::Inconnue
            ]
        );
    }

    #[test]
    fn valeur_inconnue_d_un_champ_ferme_ne_rend_pas_la_trame_illisible() {
        let Evenement::StatutModifie(statut) = evenement(
            json!({"type": "status.updated", "data": {"user_id": "u", "status": "invisible"}}),
        ) else {
            panic!()
        };
        assert_eq!(statut.status, Some(StatutPresence::Inconnu));
        let Evenement::Pret(pret) =
            evenement(json!({"type": "ready", "data": {"user_id": "u", "nouveau": 1}}))
        else {
            panic!()
        };
        assert_eq!(pret.user_id, "u");
    }

    #[test]
    fn type_inconnu_et_trame_illisible() {
        assert_eq!(
            decoder(r#"{"type":"evenement.futur","data":{"x":1}}"#),
            Trame::Inconnue("evenement.futur".into())
        );
        assert!(matches!(decoder("pas du json"), Trame::Illisible(_)));
        assert!(matches!(decoder(r#"{"data":{}}"#), Trame::Illisible(_)));
        assert!(matches!(
            decoder(r#"{"type":"presence","data":{"user_id":5}}"#),
            Trame::Illisible(_)
        ));
    }
}
