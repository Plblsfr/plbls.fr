use estompe_client::store::{CleConversation, Etat, ParticipantVocal};
use serde_json::{json, Value};

fn depuis<T: serde::de::DeserializeOwned>(valeur: Value) -> T {
    serde_json::from_value(valeur).unwrap()
}

pub fn etat() -> Etat {
    let mut etat = Etat {
        compte: depuis(json!({"activated": true, "allowed": true, "id": "me", "pseudo": "bob"})),
        serveurs: depuis(json!([
            {"id": "s1", "name": "Les amis", "owner_id": "me", "icon_version": 0},
            {"id": "s2", "name": "Travail", "owner_id": "eve", "icon_version": 3}
        ])),
        messages_prives: depuis(json!([
            {"id": "d1", "other": {"id": "carol", "pseudo": "carol"}},
            {"id": "d2", "other": {"id": "dave", "pseudo": "dave"}}
        ])),
        mentions: depuis(json!([
            {"message_id": "m9", "channel_id": "c1", "server_id": "s1", "author": {"id": "carol", "pseudo": "carol"}, "content": "@bob tu viens ?", "created_at": "2026-11-19T09:00:00Z", "group": false}
        ])),
        ..Etat::default()
    };
    etat.details.insert(
        "s1".into(),
        depuis(json!({
            "server": {"id": "s1", "name": "Les amis", "owner_id": "me", "icon_version": 0},
            "channels": [
                {"id": "v1", "name": "Salon vocal", "kind": "voice", "position": 1},
                {"id": "m1", "name": "Musique", "kind": "music", "position": 1},
                {"id": "c1", "name": "général", "kind": "text", "position": 1, "topic": "On parle de tout", "slowmode": 10},
                {"id": "c2", "name": "annonces", "kind": "text", "position": 2, "locked": true},
                {"id": "k1", "name": "Ciné", "kind": "cinema", "position": 2}
            ],
            "members": []
        })),
    );
    etat.details.insert(
        "s2".into(),
        depuis(json!({
            "server": {"id": "s2", "name": "Travail", "owner_id": "eve", "icon_version": 3},
            "channels": [{"id": "t1", "name": "équipe", "kind": "text", "position": 1}],
            "members": []
        })),
    );
    etat.non_lus.insert(CleConversation::Salon("c2".into()), 3);
    etat.non_lus.insert(CleConversation::Salon("c1".into()), 1);
    etat.non_lus.insert(CleConversation::Prive("d1".into()), 2);
    etat.en_ligne.insert("carol".into());
    etat.vocal.insert(
        "v1".into(),
        vec![ParticipantVocal {
            utilisateur: "carol".into(),
            pseudo: "carol".into(),
            ..ParticipantVocal::default()
        }],
    );
    etat
}
