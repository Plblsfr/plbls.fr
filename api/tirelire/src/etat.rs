use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{
    config::Config,
    droits::{Aegis, Droits, Injoignable},
    identite::{Identite, Resolveur},
};

pub const PERIODE_MANIFESTE: Duration = Duration::from_secs(600);

pub struct Inner {
    cfg: Config,
    pool: PgPool,
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
            identites: Resolveur::new(&cfg.gateway_url, http),
            cfg,
            pool,
            aegis,
        }))
    }

    pub fn config(&self) -> &Config {
        &self.0.cfg
    }

    pub fn pool(&self) -> &PgPool {
        &self.0.pool
    }

    pub fn identites(&self) -> &Resolveur {
        &self.0.identites
    }

    pub fn aegis(&self) -> Option<&Aegis> {
        self.0.aegis.as_ref()
    }

    pub async fn droits_de(&self, id: &Identite) -> Result<Droits, Injoignable> {
        match &self.0.aegis {
            Some(a) => a.droits(id).await,
            None => Ok(Droits::default()),
        }
    }

    pub fn demarrer_taches(&self) {
        if self.aegis().is_none() {
            return;
        }
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
}
