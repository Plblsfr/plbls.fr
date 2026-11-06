use std::{sync::Arc, time::Duration};

use estompe_block_regarder_ensemble::{Lecteurs, SalonLance, Utilisateur};
use estompe_core::{
    messages::{Conv, EvenementMessage, SYSTEME},
    state::AppState,
};
use sqlx::PgPool;

use crate::{base, changement};

pub const PERIODE: Duration = Duration::from_secs(20);

pub struct Planificateur {
    pub state: AppState,
    pub lecteurs: Lecteurs,
}

fn identifiant_youtube(s: &str) -> bool {
    s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Planificateur {
    pub fn lancer(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut t = tokio::time::interval_at(tokio::time::Instant::now() + PERIODE, PERIODE);
            loop {
                t.tick().await;
                self.passage().await;
            }
        })
    }

    pub async fn passage(&self) {
        let Ok(pool) = self.state.pool() else {
            return;
        };
        match base::rappels_dus(pool).await {
            Ok(dues) => {
                for d in dues {
                    self.rappeler(pool, &d).await;
                }
            }
            Err(e) => tracing::warn!("séances : rappels : {e}"),
        }
        match base::demarrages_dus(pool).await {
            Ok(dues) => {
                for d in dues {
                    self.demarrer(pool, &d).await;
                }
            }
            Err(e) => tracing::warn!("séances : démarrage : {e}"),
        }
    }

    async fn rappeler(&self, pool: &PgPool, d: &base::Due) {
        if d.presents.is_empty() {
            return;
        }
        let Ok(Some((serveur, _))) = base::serveur_et_genre(pool, &d.salon).await else {
            return;
        };
        let minutes = ((d.dans / 60.0).round() as i64).max(1);
        self.state
            .notifier(&EvenementMessage {
                conv: Conv {
                    server_id: Some(serveur),
                    channel_id: Some(d.salon.clone()),
                    ..Default::default()
                },
                a: d.presents.clone(),
                rappel: format!("Séance « {} » dans {minutes} min", d.titre),
                ..Default::default()
            })
            .await;
    }

    async fn demarrer(&self, pool: &PgPool, d: &base::Due) {
        let Some(Ok(presents)) = self.state.participants_vocal(&d.vocal).await else {
            return;
        };
        if !presents
            .iter()
            .any(|p| p["user_id"].as_str().is_some_and(|id| id != SYSTEME))
        {
            return;
        }
        let Ok(Some((serveur_vocal, _))) = base::serveur_et_genre(pool, &d.vocal).await else {
            return;
        };
        let publications = match base::publications_par_id(pool, &d.salon, &d.videos).await {
            Ok(p) if !p.is_empty() => p,
            _ => return,
        };
        if !matches!(base::reclamer_demarrage(pool, &d.id).await, Ok(true)) {
            return;
        }
        let Ok(Some((serveur, genre))) = base::serveur_et_genre(pool, &d.salon).await else {
            return;
        };
        if genre == "music"
            && let Err(e) = base::marquer_ecoutees(pool, &d.salon, &d.videos).await
        {
            tracing::warn!("séances : morceaux écoutés : {e}");
        }
        let pseudo = format!("Séance « {} »", d.titre);
        let hote = d.presents.first().cloned().unwrap_or_default();
        let mut videos = Vec::new();
        let mut musique = Vec::new();
        for p in publications {
            match p.provider.as_str() {
                "ytmusic" => musique.push(if identifiant_youtube(&p.reference) {
                    format!("https://music.youtube.com/watch?v={}", p.reference)
                } else {
                    format!("https://music.youtube.com/playlist?list={}", p.reference)
                }),
                "page" => videos.push(("file".to_string(), p.reference)),
                _ => videos.push((p.provider, p.reference)),
            }
        }
        self.lecteurs
            .lancer(
                &self.state,
                &SalonLance {
                    serveur: &serveur_vocal,
                    id: &d.vocal,
                },
                &Utilisateur {
                    id: &hote,
                    pseudo: &pseudo,
                },
                videos,
                musique,
            )
            .await;
        let conv = Conv {
            server_id: Some(serveur),
            channel_id: Some(d.salon.clone()),
            kind: genre,
            ..Default::default()
        };
        changement(&self.state, pool, &conv).await;
    }
}
