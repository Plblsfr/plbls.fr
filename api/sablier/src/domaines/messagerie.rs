use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::NaiveDate;
use commun::evenements::{Donnees, Evenement, Sens};
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::{
    agregats,
    etat::AppState,
    garde::Personne,
    http::{bd, ok},
    periode::{Demande, Intervalle, Plage},
};

pub const MESSAGES: &str = "messagerie.messages";
pub const REACTIONS_DONNEES: &str = "messagerie.reactions_donnees";
pub const REACTIONS_RECUES: &str = "messagerie.reactions_recues";
pub const FICHIERS: &str = "messagerie.fichiers";
pub const VOCAL_SECONDES: &str = "messagerie.vocal_secondes";
pub const APPEL_MAX: &str = "messagerie.appel_max";
pub const PARTAGE_SECONDES: &str = "messagerie.partage_secondes";

pub const PRIVE: &str = "prive";

fn cle_salon(serveur: Option<&str>, salon: &str) -> String {
    format!("{}/{salon}", serveur.unwrap_or_default())
}

fn salon_de(cle: &str) -> (Option<String>, String) {
    let (serveur, salon) = cle.split_once('/').unwrap_or(("", cle));
    (
        (!serveur.is_empty()).then(|| serveur.to_string()),
        salon.to_string(),
    )
}

pub async fn appliquer(
    tx: &mut PgConnection,
    e: &Evenement,
    jour: NaiveDate,
) -> Result<(), sqlx::Error> {
    let sub = e.sub.as_str();
    match &e.donnees {
        Donnees::EstompeMessage {
            serveur,
            salon,
            prive,
        } => {
            let cle = match salon {
                Some(salon) if !prive => cle_salon(serveur.as_deref(), salon),
                _ => PRIVE.to_string(),
            };
            agregats::ajouter(tx, sub, jour, MESSAGES, &cle, 1).await
        }
        Donnees::EstompeFichier { nombre, .. } => {
            agregats::ajouter(tx, sub, jour, FICHIERS, "", i64::from(*nombre)).await
        }
        Donnees::EstompeReaction { sens, emoji } => {
            let (stat, cle) = match sens {
                Sens::Donnee => (REACTIONS_DONNEES, emoji.as_str()),
                Sens::Recue => (REACTIONS_RECUES, ""),
            };
            agregats::ajouter(tx, sub, jour, stat, cle, 1).await
        }
        Donnees::EstompeVocalTemps {
            serveur,
            salon,
            secondes,
        } => {
            let cle = cle_salon(serveur.as_deref(), salon);
            agregats::ajouter(tx, sub, jour, VOCAL_SECONDES, &cle, i64::from(*secondes)).await
        }
        Donnees::EstompeVocalAppel { secondes, .. } => {
            agregats::maximum(tx, sub, jour, APPEL_MAX, "", i64::from(*secondes)).await
        }
        Donnees::EstompePartageEcran { secondes } => {
            agregats::ajouter(tx, sub, jour, PARTAGE_SECONDES, "", i64::from(*secondes)).await
        }
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SalonMessages {
    pub server: Option<String>,
    pub channel: String,
    pub messages: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SalonVocal {
    pub server: Option<String>,
    pub channel: String,
    pub seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Journee {
    pub day: NaiveDate,
    pub messages: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Emoji {
    pub emoji: String,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Stats {
    #[serde(flatten)]
    pub intervalle: Intervalle,
    pub total: i64,
    pub private: i64,
    pub by_channel: Vec<SalonMessages>,
    pub top_channel: Option<SalonMessages>,
    pub busiest_day: Option<Journee>,
    pub reactions_given: i64,
    pub reactions_received: i64,
    pub top_emoji: Option<Emoji>,
    pub files: i64,
    pub voice_seconds: i64,
    pub longest_call: i64,
    pub top_voice_channel: Option<SalonVocal>,
    pub screen_share_seconds: i64,
}

pub async fn lire(pool: &PgPool, sub: &str, plage: &Plage) -> Result<Stats, sqlx::Error> {
    let intervalle = plage.intervalle_de(pool, sub).await?;
    let (du, au) = (intervalle.from, intervalle.to);
    let messages = agregats::par_cle(pool, sub, MESSAGES, du, au).await?;
    let by_channel: Vec<SalonMessages> = messages
        .iter()
        .filter(|(cle, _)| cle != PRIVE)
        .map(|(cle, messages)| {
            let (server, channel) = salon_de(cle);
            SalonMessages {
                server,
                channel,
                messages: *messages,
            }
        })
        .collect();
    let donnees = agregats::par_cle(pool, sub, REACTIONS_DONNEES, du, au).await?;
    let vocal = agregats::par_cle(pool, sub, VOCAL_SECONDES, du, au).await?;
    Ok(Stats {
        intervalle,
        total: messages.iter().map(|(_, n)| n).sum(),
        private: messages
            .iter()
            .filter(|(cle, _)| cle == PRIVE)
            .map(|(_, n)| n)
            .sum(),
        top_channel: by_channel.first().cloned(),
        by_channel,
        busiest_day: agregats::max_jour(pool, sub, MESSAGES, du, au)
            .await?
            .map(|(day, messages)| Journee { day, messages }),
        reactions_given: donnees.iter().map(|(_, n)| n).sum(),
        reactions_received: agregats::somme(pool, sub, REACTIONS_RECUES, du, au).await?,
        top_emoji: donnees.first().map(|(emoji, count)| Emoji {
            emoji: emoji.clone(),
            count: *count,
        }),
        files: agregats::somme(pool, sub, FICHIERS, du, au).await?,
        voice_seconds: vocal.iter().map(|(_, n)| n).sum(),
        longest_call: agregats::plus_grande(pool, sub, APPEL_MAX, du, au).await?,
        top_voice_channel: vocal.first().map(|(cle, seconds)| {
            let (server, channel) = salon_de(cle);
            SalonVocal {
                server,
                channel,
                seconds: *seconds,
            }
        }),
        screen_share_seconds: agregats::somme(pool, sub, PARTAGE_SECONDES, du, au).await?,
    })
}

pub async fn voir(
    State(state): State<AppState>,
    p: Personne,
    Query(demande): Query<Demande>,
) -> Result<Response, Response> {
    let plage = demande.plage(&state).await?;
    Ok(ok(&lire(state.pool(), p.sub(), &plage)
        .await
        .map_err(bd)?))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        file::Issue,
        testutil::{Env, env},
    };

    const SUB: &str = "sub-alice";
    const MARS: &str = "period=month&date=2026-03-10";

    fn message(salon: Option<(&str, &str)>) -> Donnees {
        Donnees::EstompeMessage {
            serveur: salon.map(|(serveur, _)| serveur.into()),
            salon: salon.map(|(_, salon)| salon.into()),
            prive: salon.is_none(),
        }
    }

    fn reaction(sens: Sens, emoji: &str) -> Donnees {
        Donnees::EstompeReaction {
            sens,
            emoji: emoji.into(),
        }
    }

    fn vocal(serveur: Option<&str>, salon: &str, secondes: u32) -> Donnees {
        Donnees::EstompeVocalTemps {
            serveur: serveur.map(Into::into),
            salon: salon.into(),
            secondes,
        }
    }

    fn appel(secondes: u32) -> Donnees {
        Donnees::EstompeVocalAppel {
            serveur: Some("srv".into()),
            salon: "voix".into(),
            secondes,
        }
    }

    async fn lue(e: &Env, requete: &str) -> Value {
        let r = e.get("alice", &format!("/messaging?{requete}")).await;
        assert_eq!(r.statut, StatusCode::OK);
        r.json()
    }

    async fn recevoir_tout(e: &Env, evenements: Vec<(&str, Donnees)>) {
        for (i, (date, donnees)) in evenements.into_iter().enumerate() {
            assert_eq!(
                e.recevoir(&format!("{date}-{i}"), SUB, date, false, donnees)
                    .await,
                Issue::Applique
            );
        }
    }

    #[tokio::test]
    async fn message_de_salon_et_message_prive() {
        let Some(e) = env().await else { return };
        let vide = lue(&e, MARS).await;
        assert_eq!(
            vide,
            json!({
                "period": "month",
                "from": "2026-03-01",
                "to": "2026-03-31",
                "total": 0,
                "private": 0,
                "by_channel": [],
                "top_channel": null,
                "busiest_day": null,
                "reactions_given": 0,
                "reactions_received": 0,
                "top_emoji": null,
                "files": 0,
                "voice_seconds": 0,
                "longest_call": 0,
                "top_voice_channel": null,
                "screen_share_seconds": 0,
            })
        );
        recevoir_tout(
            &e,
            vec![("2026-03-10T10:00:00Z", message(Some(("srv", "general"))))],
        )
        .await;
        let r = lue(&e, MARS).await;
        assert_eq!(
            (
                r["total"].clone(),
                r["private"].clone(),
                r["by_channel"].clone()
            ),
            (
                json!(1),
                json!(0),
                json!([{"server": "srv", "channel": "general", "messages": 1}])
            )
        );
        recevoir_tout(&e, vec![("2026-03-10T10:01:00Z", message(None))]).await;
        let r = lue(&e, MARS).await;
        assert_eq!(
            (
                r["total"].clone(),
                r["private"].clone(),
                r["by_channel"].clone()
            ),
            (
                json!(2),
                json!(1),
                json!([{"server": "srv", "channel": "general", "messages": 1}])
            )
        );
        e.recevoir(
            "autre",
            "sub-bruno",
            "2026-03-10T10:00:00Z",
            false,
            message(None),
        )
        .await;
        assert_eq!(lue(&e, MARS).await["total"], 2);
    }

    #[tokio::test]
    async fn toutes_les_stats_de_la_messagerie() {
        let Some(e) = env().await else { return };
        recevoir_tout(
            &e,
            vec![
                ("2026-03-10T10:00:00Z", message(Some(("srv", "general")))),
                ("2026-03-11T10:00:00Z", message(Some(("srv", "jeux")))),
                ("2026-03-11T10:01:00Z", message(Some(("srv", "jeux")))),
                ("2026-03-11T10:02:00Z", message(Some(("autre", "jeux")))),
                ("2026-03-11T10:03:00Z", message(None)),
                ("2026-03-11T10:04:00Z", message(None)),
                ("2026-03-11T10:05:00Z", message(None)),
                ("2026-04-02T10:00:00Z", message(Some(("srv", "general")))),
                ("2026-03-10T11:00:00Z", reaction(Sens::Donnee, "👍")),
                ("2026-03-10T11:01:00Z", reaction(Sens::Donnee, "🎉")),
                ("2026-03-12T11:00:00Z", reaction(Sens::Donnee, "🎉")),
                ("2026-03-12T11:01:00Z", reaction(Sens::Recue, "👍")),
                (
                    "2026-03-12T12:00:00Z",
                    Donnees::EstompeFichier {
                        serveur: Some("srv".into()),
                        salon: Some("general".into()),
                        prive: false,
                        nombre: 3,
                    },
                ),
                (
                    "2026-03-12T12:01:00Z",
                    Donnees::EstompeFichier {
                        serveur: None,
                        salon: None,
                        prive: true,
                        nombre: 1,
                    },
                ),
                ("2026-03-12T20:00:00Z", vocal(Some("srv"), "voix", 300)),
                ("2026-03-12T20:05:00Z", vocal(Some("srv"), "voix", 300)),
                ("2026-03-13T20:00:00Z", vocal(Some("srv"), "detente", 500)),
                ("2026-03-13T21:00:00Z", vocal(None, "conv1", 100)),
                ("2026-03-12T20:10:00Z", appel(600)),
                ("2026-03-13T20:10:00Z", appel(500)),
                ("2026-03-13T22:00:00Z", appel(450)),
                (
                    "2026-03-13T20:30:00Z",
                    Donnees::EstompePartageEcran { secondes: 240 },
                ),
                (
                    "2026-03-13T20:40:00Z",
                    Donnees::EstompePartageEcran { secondes: 60 },
                ),
            ],
        )
        .await;
        assert_eq!(
            lue(&e, MARS).await,
            json!({
                "period": "month",
                "from": "2026-03-01",
                "to": "2026-03-31",
                "total": 7,
                "private": 3,
                "by_channel": [
                    {"server": "srv", "channel": "jeux", "messages": 2},
                    {"server": "autre", "channel": "jeux", "messages": 1},
                    {"server": "srv", "channel": "general", "messages": 1},
                ],
                "top_channel": {"server": "srv", "channel": "jeux", "messages": 2},
                "busiest_day": {"day": "2026-03-11", "messages": 6},
                "reactions_given": 3,
                "reactions_received": 1,
                "top_emoji": {"emoji": "🎉", "count": 2},
                "files": 4,
                "voice_seconds": 1200,
                "longest_call": 600,
                "top_voice_channel": {"server": "srv", "channel": "voix", "seconds": 600},
                "screen_share_seconds": 300,
            })
        );
        let r = lue(&e, "period=day&date=2026-03-13").await;
        assert_eq!(
            (
                r["voice_seconds"].clone(),
                r["longest_call"].clone(),
                r["top_voice_channel"].clone()
            ),
            (
                json!(600),
                json!(500),
                json!({"server": "srv", "channel": "detente", "seconds": 500})
            )
        );
        let r = lue(&e, "period=all&date=2026-12-31").await;
        assert_eq!(
            (
                r["from"].clone(),
                r["total"].clone(),
                r["top_channel"].clone()
            ),
            (
                json!("2026-03-10"),
                json!(8),
                json!({"server": "srv", "channel": "general", "messages": 2})
            )
        );
        let r = e.get("alice", "/messaging?date=hier").await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn vocal_prive_sans_serveur() {
        let Some(e) = env().await else { return };
        recevoir_tout(
            &e,
            vec![("2026-03-13T21:00:00Z", vocal(None, "conv1", 100))],
        )
        .await;
        assert_eq!(
            lue(&e, MARS).await["top_voice_channel"],
            json!({"server": null, "channel": "conv1", "seconds": 100})
        );
    }

    #[tokio::test]
    async fn import_rejoue_deux_fois_memes_totaux() {
        let Some(e) = env().await else { return };
        let historique = [
            (
                "import:estompe:message:1",
                "2025-02-01T09:00:00Z",
                message(Some(("srv", "general"))),
            ),
            (
                "import:estompe:message:2",
                "2025-02-01T09:01:00Z",
                message(Some(("srv", "general"))),
            ),
            (
                "import:estompe:message:3",
                "2025-02-02T09:00:00Z",
                message(None),
            ),
            (
                "import:estompe:reaction:1",
                "2025-02-02T09:01:00Z",
                reaction(Sens::Donnee, "👍"),
            ),
            (
                "import:estompe:fichier:1",
                "2025-02-02T09:02:00Z",
                Donnees::EstompeFichier {
                    serveur: Some("srv".into()),
                    salon: Some("general".into()),
                    prive: false,
                    nombre: 2,
                },
            ),
        ];
        let mut lues = Vec::new();
        for attendue in [Issue::Applique, Issue::Doublon] {
            for (id, date, donnees) in historique.clone() {
                assert_eq!(e.recevoir(id, SUB, date, true, donnees).await, attendue);
            }
            lues.push(lue(&e, "period=year&date=2025-06-01").await);
        }
        assert_eq!(lues[0], lues[1]);
        assert_eq!(
            (
                lues[0]["total"].clone(),
                lues[0]["private"].clone(),
                lues[0]["by_channel"].clone(),
                lues[0]["reactions_given"].clone(),
                lues[0]["files"].clone(),
            ),
            (
                json!(3),
                json!(1),
                json!([{"server": "srv", "channel": "general", "messages": 2}]),
                json!(1),
                json!(2)
            )
        );
    }
}
