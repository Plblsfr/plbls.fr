use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{
    config::Config,
    droits::{Aegis, Droits, Injoignable},
    identite::{Identite, Resolveur},
};

pub const PERIODE_MANIFESTE: Duration = Duration::from_secs(600);
pub const PERIODE_PURGE: Duration = Duration::from_secs(24 * 3600);
pub const PERIODE_EVALUATION: Duration = Duration::from_secs(10);
pub const PERIODE_RECAPS: Duration = Duration::from_secs(3600);

pub struct Inner {
    cfg: Config,
    pool: PgPool,
    http: reqwest::Client,
    identites: Resolveur,
    aegis: Option<Aegis>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub fn client_http() -> reqwest::Client {
    commun::http::client(Duration::from_secs(15))
}

impl AppState {
    pub fn new(cfg: Config, pool: PgPool) -> Self {
        let http = client_http();
        let aegis = cfg
            .aegis
            .as_ref()
            .map(|(url, jeton)| Aegis::new(url, jeton, http.clone()));
        Self(Arc::new(Inner {
            identites: Resolveur::new(&cfg.gateway_url, http.clone()),
            cfg,
            pool,
            http,
            aegis,
        }))
    }

    pub fn config(&self) -> &Config {
        &self.0.cfg
    }

    pub fn pool(&self) -> &PgPool {
        &self.0.pool
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.0.http
    }

    pub fn identites(&self) -> &Resolveur {
        &self.0.identites
    }

    pub fn aegis(&self) -> Option<&Aegis> {
        self.0.aegis.as_ref()
    }

    pub async fn aujourd_hui(&self) -> Result<chrono::NaiveDate, sqlx::Error> {
        crate::agregats::jour_de(self.pool(), &self.config().fuseau, chrono::Utc::now()).await
    }

    pub async fn droits_de(&self, id: &Identite) -> Result<Droits, Injoignable> {
        match &self.0.aegis {
            Some(a) => a.droits(id).await,
            None => Ok(Droits::default()),
        }
    }

    pub fn demarrer_taches(&self) {
        if self.aegis().is_some() {
            let etat = self.clone();
            tokio::spawn(async move {
                let mut t = tokio::time::interval(PERIODE_MANIFESTE);
                loop {
                    t.tick().await;
                    if let Some(a) = etat.aegis()
                        && let Err(e) = a.envoyer_manifeste().await
                    {
                        tracing::warn!("manifeste non envoyé à aegis : {e}");
                    }
                }
            });
        }
        let etat = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(PERIODE_PURGE);
            loop {
                t.tick().await;
                let mois = etat.config().conservation_mois;
                match crate::purge::purger(etat.pool(), mois, chrono::Utc::now()).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("purge : {n} évènements bruts de plus de {mois} mois"),
                    Err(e) => tracing::warn!("purge : {e}"),
                }
            }
        });
        let etat = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(PERIODE_EVALUATION);
            t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                t.tick().await;
                let passe = async {
                    crate::trophees::evaluer_les_marques(etat.pool(), etat.aujourd_hui().await?)
                        .await
                };
                if let Err(e) = passe.await {
                    tracing::warn!("trophées : {e}");
                }
            }
        });
        let etat = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(PERIODE_RECAPS);
            loop {
                t.tick().await;
                let passe = async {
                    crate::recaps::creer_les_recaps(&etat, etat.aujourd_hui().await?).await
                };
                match passe.await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("récaps : {n} créés"),
                    Err(e) => tracing::warn!("récaps : {e}"),
                }
            }
        });
        match crate::file::Reglages::de(self.config()) {
            Some(reglages) => {
                let pool = self.pool().clone();
                let fuseau = self.config().fuseau.clone();
                tokio::spawn(async move { crate::file::consommer(pool, fuseau, reglages).await });
            }
            None => tracing::warn!(
                "SABLIER_REDIS_URL non renseignée : la file d'évènements n'est pas consommée"
            ),
        }
    }
}
