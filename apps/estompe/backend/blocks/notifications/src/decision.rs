use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Types {
    pub dm: bool,
    pub mention: bool,
    pub channel: bool,
    pub voice: bool,
}

impl Default for Types {
    fn default() -> Self {
        Types {
            dm: true,
            mention: true,
            channel: true,
            voice: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Reglages {
    pub levels: BTreeMap<String, String>,
    pub types: Types,
    pub preview: bool,
    pub sound: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignore_group: Vec<String>,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            levels: BTreeMap::new(),
            types: Types::default(),
            preview: true,
            sound: true,
            ignore_group: Vec::new(),
        }
    }
}

impl Reglages {
    pub fn depuis_objet(
        mut objet: serde_json::Map<String, Value>,
    ) -> Result<Self, serde_json::Error> {
        objet.retain(|_, v| !v.is_null());
        if let Some(Value::Object(t)) = objet.get_mut("types") {
            t.retain(|_, v| !v.is_null());
        }
        serde_json::from_value(Value::Object(objet))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Lieu {
    pub salon: Option<String>,
    pub serveur: Option<String>,
    pub dm: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Evenement {
    pub lieu: Lieu,
    pub message: Value,
    pub mentionnes: Vec<String>,
    pub groupe: Vec<String>,
    pub a: Option<Vec<String>>,
    pub vocal: String,
    pub rappel: String,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub tag: String,
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub conv: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub msg: String,
    pub kind: String,
    pub silent: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub read: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sender: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub room: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub attach: String,
}

impl Evenement {
    pub fn auteur_id(&self) -> &str {
        self.message["author"]["id"].as_str().unwrap_or_default()
    }
}

pub fn niveau(reglages: &Reglages, lieu: &Lieu) -> String {
    for id in [&lieu.salon, &lieu.dm, &lieu.serveur].into_iter().flatten() {
        if let Some(n) = reglages.levels.get(id) {
            return n.clone();
        }
    }
    if lieu.dm.is_some() { "all" } else { "mentions" }.to_string()
}

fn tronquer(texte: &str, max: usize) -> String {
    let mut caracteres = texte.chars();
    let debut: String = caracteres.by_ref().take(max).collect();
    if caracteres.next().is_some() {
        format!("{debut}…")
    } else {
        debut
    }
}

fn texte<'a>(v: &'a Value, cle: &str) -> &'a str {
    v[cle].as_str().unwrap_or_default()
}

pub fn decider(reglages: &Reglages, e: &Evenement, user: &str, ou: &str) -> Option<Notification> {
    let lieu = &e.lieu;
    let niveau = niveau(reglages, lieu);
    let salon = lieu.salon.clone().unwrap_or_default();
    let serveur = lieu.serveur.clone().unwrap_or_default();
    if !e.rappel.is_empty() {
        if niveau == "none" {
            return None;
        }
        return Some(Notification {
            title: if ou.is_empty() { "Estompe" } else { ou }.to_string(),
            body: e.rappel.clone(),
            kind: "voice".into(),
            silent: !reglages.sound,
            url: format!("/#/s/{serveur}/{salon}"),
            conv: format!("c:{salon}"),
            tag: format!("watch-{salon}"),
            ..Default::default()
        });
    }
    if niveau == "none" {
        return None;
    }
    let auteur = texte(&e.message["author"], "pseudo");
    let mut n = Notification {
        silent: !reglages.sound,
        url: "/#/".into(),
        ..Default::default()
    };
    if let Some(dm) = &lieu.dm {
        n.url = format!("/#/dm/{dm}");
        n.conv = format!("d:{dm}");
    } else if lieu.serveur.is_some() && lieu.salon.is_some() {
        n.url = format!("/#/s/{serveur}/{salon}");
        n.conv = format!("c:{salon}");
    }
    if !e.vocal.is_empty() {
        if !reglages.types.voice {
            return None;
        }
        n.title = "Estompe".into();
        n.tag = format!("voice-{salon}-{}", e.auteur_id());
        n.kind = "voice".into();
        n.body = format!("{auteur} {}", e.vocal);
        if !ou.is_empty() {
            n.body.push_str(&format!(" dans {ou}"));
        }
        return Some(n);
    }
    let ignore_groupe = lieu
        .serveur
        .as_ref()
        .is_some_and(|s| reglages.ignore_group.contains(s))
        && e.groupe.iter().any(|g| g == user);
    let mention = !ignore_groupe && e.mentionnes.iter().any(|m| m == user);
    if niveau == "mentions" && !mention {
        return None;
    }
    if lieu.dm.is_some() {
        if !reglages.types.dm && !(mention && reglages.types.mention) {
            return None;
        }
    } else if mention {
        if !reglages.types.mention {
            return None;
        }
    } else if !reglages.types.channel {
        return None;
    }
    let id = texte(&e.message, "id");
    n.tag = format!("msg-{id}");
    n.msg = id.to_string();
    if !n.conv.is_empty() {
        n.url.push_str(&format!("/{id}"));
    }
    n.kind = if mention {
        "mention"
    } else if lieu.dm.is_some() {
        "dm"
    } else {
        "channel"
    }
    .into();
    if !reglages.preview {
        n.title = format!("Nouveau message de {auteur}");
        return Some(n);
    }
    n.title = if mention {
        format!("{auteur} vous a mentionné")
    } else if !ou.is_empty() && lieu.dm.is_none() {
        format!("{auteur} · {ou}")
    } else {
        auteur.to_string()
    };
    let contenu = texte(&e.message, "content").trim();
    let pieces = e.message["attachments"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    n.body = tronquer(contenu, 140);
    if n.body.is_empty()
        && let Some(p) = pieces.first()
    {
        n.body = "Fichier joint".into();
        if texte(p, "kind") == "voice" {
            let duree = p["duration_ms"].as_i64().unwrap_or(0);
            n.body = format!("Message vocal ({}:{:02})", duree / 60000, duree / 1000 % 60);
            let transcription = texte(p, "transcript");
            if !transcription.is_empty() {
                n.body = tronquer(transcription, 140);
            }
        }
    }
    n.sender = auteur.to_string();
    n.text = tronquer(contenu, 500);
    if lieu.dm.is_none() {
        n.room = ou.to_string();
    }
    if let Some(p) = pieces.first() {
        n.attach = if texte(p, "kind") == "voice" {
            "voice"
        } else if texte(p, "mime").starts_with("image/") {
            "image"
        } else {
            "file"
        }
        .into();
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn salon() -> Evenement {
        Evenement {
            lieu: Lieu {
                salon: Some("c1".into()),
                serveur: Some("s1".into()),
                dm: None,
            },
            message: msg(),
            ..Default::default()
        }
    }

    fn mp() -> Evenement {
        Evenement {
            lieu: Lieu {
                salon: None,
                serveur: None,
                dm: Some("d1".into()),
            },
            message: msg(),
            ..Default::default()
        }
    }

    fn msg() -> Value {
        json!({ "id": "7", "author": { "id": "a", "pseudo": "alice" }, "content": "salut", "attachments": [] })
    }

    fn avec(f: impl FnOnce(&mut Reglages)) -> Reglages {
        let mut r = Reglages::default();
        f(&mut r);
        r
    }

    fn mentionne(mut e: Evenement) -> Evenement {
        e.mentionnes = vec!["u".into()];
        e
    }

    fn vocal(mut e: Evenement) -> Evenement {
        e.vocal = "a rejoint".into();
        e
    }

    #[test]
    fn table_de_decision() {
        let defaut = Reglages::default;
        let cas: Vec<(&str, Reglages, Evenement, Option<&str>)> = vec![
            (
                "salon par défaut : mentions seulement",
                defaut(),
                salon(),
                None,
            ),
            (
                "salon mentionné",
                defaut(),
                mentionne(salon()),
                Some("alice vous a mentionné"),
            ),
            (
                "salon en tout",
                avec(|s| {
                    s.levels.insert("s1".into(), "all".into());
                }),
                salon(),
                Some("alice · général"),
            ),
            (
                "salon coupé malgré mention",
                avec(|s| {
                    s.levels.insert("c1".into(), "none".into());
                }),
                mentionne(salon()),
                None,
            ),
            (
                "le salon l'emporte sur le serveur",
                avec(|s| {
                    s.levels.insert("s1".into(), "none".into());
                    s.levels.insert("c1".into(), "all".into());
                }),
                salon(),
                Some("alice · général"),
            ),
            (
                "type mention coupé",
                avec(|s| s.types.mention = false),
                mentionne(salon()),
                None,
            ),
            (
                "type salon coupé",
                avec(|s| {
                    s.levels.insert("s1".into(), "all".into());
                    s.types.channel = false;
                }),
                salon(),
                None,
            ),
            ("message privé", defaut(), mp(), Some("alice")),
            (
                "message privé coupé",
                avec(|s| s.types.dm = false),
                mp(),
                None,
            ),
            (
                "message privé coupé mais mentionné",
                avec(|s| s.types.dm = false),
                mentionne(mp()),
                Some("alice vous a mentionné"),
            ),
            (
                "aperçu masqué",
                avec(|s| s.preview = false),
                mp(),
                Some("Nouveau message de alice"),
            ),
            ("vocal", defaut(), vocal(salon()), Some("Estompe")),
            (
                "vocal coupé",
                avec(|s| s.types.voice = false),
                vocal(salon()),
                None,
            ),
            (
                "vocal sur serveur coupé",
                avec(|s| {
                    s.levels.insert("s1".into(), "none".into());
                }),
                vocal(salon()),
                None,
            ),
        ];
        for (nom, reglages, e, titre) in cas {
            let n = decider(&reglages, &e, "u", "général");
            assert_eq!(n.as_ref().map(|n| n.title.as_str()), titre, "{nom}");
        }
        let n = decider(&avec(|s| s.sound = false), &mp(), "u", "").unwrap();
        assert!(n.silent);
        assert_eq!(n.body, "salut");
        assert_eq!(n.url, "/#/dm/d1/7");
        assert_eq!(
            (n.tag.as_str(), n.msg.as_str(), n.conv.as_str()),
            ("msg-7", "7", "d:d1")
        );
    }

    #[test]
    fn mention_de_groupe_ignoree() {
        let mut e = mentionne(salon());
        e.groupe = vec!["u".into()];
        let ignore = avec(|s| s.ignore_group = vec!["s1".into()]);
        assert!(decider(&Reglages::default(), &e, "u", "").is_some());
        assert!(decider(&ignore, &e, "u", "").is_none());
        e.groupe.clear();
        assert!(decider(&ignore, &e, "u", "").is_some());
    }

    #[test]
    fn liens() {
        let d = Reglages::default();
        assert_eq!(
            decider(&d, &mentionne(salon()), "u", "").unwrap().url,
            "/#/s/s1/c1/7"
        );
        let v = decider(&d, &vocal(salon()), "u", "").unwrap();
        assert_eq!(v.url, "/#/s/s1/c1");
        assert_eq!(v.tag, "voice-c1-a");
        assert_eq!(v.body, "alice a rejoint");
        assert_eq!(
            decider(&d, &vocal(salon()), "u", "vocal").unwrap().body,
            "alice a rejoint dans vocal"
        );
    }

    #[test]
    fn message_vocal() {
        let mut e = mentionne(salon());
        e.message["content"] = json!("");
        e.message["attachments"] =
            json!([{ "kind": "voice", "mime": "video/webm", "duration_ms": 12400 }]);
        let n = decider(&Reglages::default(), &e, "u", "").unwrap();
        assert_eq!(
            (n.attach.as_str(), n.body.as_str()),
            ("voice", "Message vocal (0:12)")
        );
        e.message["attachments"][0]["transcript"] = json!("Rendez-vous demain à 9h");
        assert_eq!(
            decider(&Reglages::default(), &e, "u", "").unwrap().body,
            "Rendez-vous demain à 9h"
        );
        e.message["attachments"] = json!([{ "kind": "file", "mime": "text/plain" }]);
        let n = decider(&Reglages::default(), &e, "u", "").unwrap();
        assert_eq!(
            (n.attach.as_str(), n.body.as_str()),
            ("file", "Fichier joint")
        );
    }

    #[test]
    fn detail_du_message() {
        let mut e = mentionne(salon());
        e.message["attachments"] = json!([{ "mime": "image/png", "kind": "file" }]);
        let n = decider(&Reglages::default(), &e, "u", "général").unwrap();
        assert_eq!(
            (
                n.sender.as_str(),
                n.room.as_str(),
                n.text.as_str(),
                n.attach.as_str()
            ),
            ("alice", "général", "salut", "image")
        );
        let n = decider(&avec(|s| s.preview = false), &e, "u", "général").unwrap();
        assert_eq!(
            (
                n.sender.as_str(),
                n.room.as_str(),
                n.text.as_str(),
                n.attach.as_str()
            ),
            ("", "", "", "")
        );
        assert_eq!(n.body, "");
    }

    #[test]
    fn troncatures() {
        let mut e = mentionne(salon());
        e.message["content"] = json!(format!("  {}  ", "é".repeat(600)));
        let n = decider(&Reglages::default(), &e, "u", "").unwrap();
        assert_eq!(n.body.chars().count(), 141);
        assert!(n.body.ends_with('…'));
        assert_eq!(n.text.chars().count(), 501);
    }

    #[test]
    fn genres_de_notification() {
        let tout = avec(|s| {
            s.levels.insert("s1".into(), "all".into());
        });
        let genre = |e: &Evenement| decider(&tout, e, "u", "").unwrap().kind;
        assert_eq!(genre(&mp()), "dm");
        assert_eq!(genre(&salon()), "channel");
        assert_eq!(genre(&mentionne(salon())), "mention");
        assert_eq!(genre(&vocal(salon())), "voice");
    }

    #[test]
    fn rappel_de_seance() {
        let e = Evenement {
            lieu: Lieu {
                salon: Some("c1".into()),
                serveur: Some("s1".into()),
                dm: None,
            },
            rappel: "Séance « Soirée » dans 10 min".into(),
            ..Default::default()
        };
        let n = decider(&Reglages::default(), &e, "u", "clips").unwrap();
        assert_eq!(
            (
                n.title.as_str(),
                n.body.as_str(),
                n.url.as_str(),
                n.tag.as_str()
            ),
            ("clips", e.rappel.as_str(), "/#/s/s1/c1", "watch-c1")
        );
        assert_eq!((n.kind.as_str(), n.conv.as_str()), ("voice", "c:c1"));
        assert!(
            decider(
                &avec(|s| {
                    s.levels.insert("c1".into(), "none".into());
                }),
                &e,
                "u",
                "clips"
            )
            .is_none()
        );
    }

    #[test]
    fn json_comme_le_go() {
        let n = decider(&Reglages::default(), &mentionne(salon()), "u", "général").unwrap();
        assert_eq!(
            serde_json::to_string(&n).unwrap(),
            r##"{"title":"alice vous a mentionné","body":"salut","tag":"msg-7","url":"/#/s/s1/c1/7","conv":"c:c1","msg":"7","kind":"mention","silent":false,"sender":"alice","room":"général","text":"salut"}"##
        );
    }

    #[test]
    fn reglages_partiels_et_nuls() {
        let objet = |v: Value| Reglages::depuis_objet(v.as_object().unwrap().clone());
        assert_eq!(objet(json!({})).unwrap(), Reglages::default());
        let r = objet(json!({ "types": { "voice": false, "dm": null }, "preview": null, "levels": { "x": "all" } })).unwrap();
        assert!(!r.types.voice && r.types.dm && r.preview);
        assert_eq!(r.levels["x"], "all");
        assert!(objet(json!({ "sound": "oui" })).is_err());
    }
}
