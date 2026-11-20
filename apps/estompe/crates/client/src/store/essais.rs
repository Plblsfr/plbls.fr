use serde_json::{json, Value};

use super::etat::Etat;
use crate::temps_reel::{decoder, Evenement, Trame};

pub fn evenement(type_evenement: &str, data: Value) -> Evenement {
    match decoder(&json!({"type": type_evenement, "data": data}).to_string()) {
        Trame::Evenement(e) => *e,
        autre => panic!("trame non décodée : {autre:?}"),
    }
}

pub fn depuis_json<T: serde::de::DeserializeOwned>(valeur: Value) -> T {
    serde_json::from_value(valeur).unwrap()
}

pub fn etat_de_depart() -> Etat {
    let mut etat = Etat {
        compte: depuis_json(
            json!({"activated": true, "allowed": true, "id": "me", "pseudo": "moi"}),
        ),
        serveurs: vec![depuis_json(
            json!({"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0}),
        )],
        ..Etat::default()
    };
    etat.details.insert(
        "s1".to_string(),
        depuis_json(json!({
            "server": {"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0},
            "channels": [{"id": "c1", "name": "général", "kind": "text", "position": 1}],
            "members": [{"id": "me", "pseudo": "moi", "online": true}, {"id": "bob", "pseudo": "bob", "online": false}]
        })),
    );
    etat
}

pub fn message(id: &str, salon: &str, auteur: &str) -> Value {
    json!({"id": id, "channel_id": salon, "author": {"id": auteur, "pseudo": auteur}, "content": "salut", "created_at": "2026-10-13T10:00:00Z", "attachments": [], "reactions": [], "mentions": []})
}

pub fn programmer(serveur: &crate::serveur_factice::ServeurFactice, routes: Vec<(&str, Value)>) {
    for (chemin, corps) in routes {
        serveur.router(
            "GET",
            chemin,
            crate::serveur_factice::Reponse::json(200, corps),
        );
    }
}

pub fn monde_a() -> Vec<(&'static str, Value)> {
    vec![
        (
            "/me",
            json!({"activated": true, "allowed": true, "id": "me", "pseudo": "moi", "blocks": ["vocal"]}),
        ),
        (
            "/servers",
            json!([{"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0}]),
        ),
        (
            "/servers/s1",
            json!({
                "server": {"id": "s1", "name": "Amis", "owner_id": "me", "icon_version": 0},
                "channels": [
                    {"id": "c1", "name": "général", "kind": "text", "position": 1, "permissions": ["view_channel", "send_messages"]},
                    {"id": "c3", "name": "jeux", "kind": "text", "position": 2, "permissions": ["view_channel"]},
                    {"id": "v1", "name": "vocal", "kind": "voice", "position": 1, "permissions": ["view_channel", "connect"]}
                ],
                "members": [
                    {"id": "me", "pseudo": "moi", "online": true, "roles": [], "nickname": "", "timeout_until": null, "banner_color": ""},
                    {"id": "bob", "pseudo": "bob", "online": false, "roles": ["r1"], "nickname": "", "timeout_until": null, "banner_color": ""},
                    {"id": "carol", "pseudo": "carol", "online": true, "roles": [], "nickname": "Caro", "timeout_until": null, "banner_color": "#aa00ff"}
                ],
                "roles": [
                    {"id": "r1", "name": "Modérateur", "color": "", "icon": "", "position": 1, "hoist": true, "permissions": ["kick_members"], "is_default": false},
                    {"id": "e1", "name": "@tout le monde", "color": "", "icon": "", "position": 0, "hoist": false, "permissions": [], "is_default": true}
                ],
                "permissions": ["view_channel", "send_messages"]
            }),
        ),
        (
            "/servers/s1/voice",
            json!({"v1": [{"user_id": "carol", "pseudo": "carol", "since": "2026-10-21T10:00:10Z"}]}),
        ),
        ("/servers/s1/avatars", json!({"bob": 2})),
        (
            "/dms",
            json!([{"id": "d1", "other": {"id": "dave", "pseudo": "dave"}, "online": true}]),
        ),
        (
            "/notifications",
            json!([{"message_id": "5", "channel_id": "c1", "server_id": "s1", "author": {"id": "bob", "pseudo": "bob"}, "content": "@moi", "created_at": "2026-10-13T10:00:00Z"}]),
        ),
        (
            "/unread",
            json!({"channels": ["c1"], "dms": [], "counts": {"c1": 3}}),
        ),
        ("/statuses", json!({"bob": "idle"})),
        (
            "/me/notify-settings",
            json!({"levels": {}, "types": {"dm": true, "mention": true, "channel": true, "voice": true}, "preview": true, "sound": true}),
        ),
        ("/avatars", json!({"me": 1, "bob": 1})),
    ]
}

pub fn monde_b() -> Vec<(&'static str, Value)> {
    vec![
        (
            "/servers",
            json!([
                {"id": "s1", "name": "Club", "owner_id": "me", "icon_version": 2},
                {"id": "s2", "name": "Travail", "owner_id": "eve", "icon_version": 0}
            ]),
        ),
        (
            "/servers/s1",
            json!({
                "server": {"id": "s1", "name": "Club", "owner_id": "me", "icon_version": 2},
                "channels": [
                    {"id": "c1", "name": "accueil", "kind": "text", "position": 1, "permissions": ["view_channel", "send_messages"]},
                    {"id": "c3", "name": "jeux", "kind": "text", "position": 2, "permissions": ["view_channel"]},
                    {"id": "c2", "name": "annonces", "kind": "text", "position": 3, "permissions": ["view_channel"]},
                    {"id": "v1", "name": "vocal", "kind": "voice", "position": 1, "permissions": ["view_channel", "connect"]}
                ],
                "members": [
                    {"id": "me", "pseudo": "moi", "online": true, "roles": [], "nickname": "", "timeout_until": null, "banner_color": ""},
                    {"id": "carol", "pseudo": "carol", "online": true, "roles": [], "nickname": "Carole", "timeout_until": null, "banner_color": "#aa00ff"}
                ],
                "roles": [
                    {"id": "r1", "name": "Modérateur", "color": "", "icon": "", "position": 1, "hoist": true, "permissions": ["kick_members"], "is_default": false},
                    {"id": "e1", "name": "@tout le monde", "color": "", "icon": "", "position": 0, "hoist": false, "permissions": [], "is_default": true}
                ],
                "permissions": ["view_channel", "send_messages"]
            }),
        ),
        ("/servers/s1/voice", json!({})),
        ("/servers/s1/avatars", json!({"bob": 2, "carol": 1})),
        (
            "/servers/s2",
            json!({
                "server": {"id": "s2", "name": "Travail", "owner_id": "eve", "icon_version": 0},
                "channels": [{"id": "t1", "name": "bureau", "kind": "text", "position": 1, "permissions": ["view_channel"]}],
                "members": [
                    {"id": "me", "pseudo": "moi", "online": true, "roles": []},
                    {"id": "eve", "pseudo": "eve", "online": true, "roles": []}
                ],
                "roles": [],
                "permissions": ["view_channel"]
            }),
        ),
        ("/servers/s2/voice", json!({})),
        ("/servers/s2/avatars", json!({})),
        (
            "/dms",
            json!([
                {"id": "d1", "other": {"id": "dave", "pseudo": "dave"}, "online": false},
                {"id": "d2", "other": {"id": "fred", "pseudo": "fred"}, "online": true}
            ]),
        ),
        ("/notifications", json!([])),
        (
            "/unread",
            json!({"channels": ["c2", "t1"], "dms": ["d2"], "counts": {"c2": 1, "d2": 2}}),
        ),
        ("/statuses", json!({"bob": "dnd", "eve": "idle"})),
        ("/avatars", json!({"me": 2, "bob": 1, "eve": 3})),
    ]
}
