use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::sync::Mutex as MutexAsync;

use crate::{
    config::Config,
    droits::{Aegis, Droits, Injoignable},
    identite::{Identite, Resolveur},
};

pub const PERIODE_MANIFESTE: Duration = Duration::from_secs(600);

pub struct Inner {
    cfg: Config,
    pool: PgPool,
    http: reqwest::Client,
    identites: Resolveur,
    aegis: Option<Aegis>,
    synchro: MutexAsync<()>,
    relance: std::sync::atomic::AtomicBool,
    lecture: crate::lecture::Lecture,
    analyse: crate::analyse::Analyse,
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
        let lecture = crate::lecture::Lecture::nouveau(&cfg);
        Self(Arc::new(Inner {
            identites: Resolveur::new(&cfg.gateway_url, http.clone()),
            cfg,
            pool,
            http,
            aegis,
            synchro: MutexAsync::new(()),
            relance: std::sync::atomic::AtomicBool::new(false),
            lecture,
            analyse: crate::analyse::Analyse::default(),
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

    pub fn verrou_synchro(&self) -> &MutexAsync<()> {
        &self.0.synchro
    }

    pub fn synchro_en_cours(&self) -> bool {
        self.0.synchro.try_lock().is_err()
    }

    pub fn synchro_a_relancer(&self) {
        self.0
            .relance
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn synchro_relancee(&self) -> bool {
        self.0
            .relance
            .swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    pub fn lecture(&self) -> &crate::lecture::Lecture {
        &self.0.lecture
    }

    pub fn analyse(&self) -> &crate::analyse::Analyse {
        &self.0.analyse
    }

    pub async fn droits_de(&self, id: &Identite) -> Result<Droits, Injoignable> {
        match &self.0.aegis {
            Some(a) => a.droits(id).await,
            None => Ok(Droits::default()),
        }
    }

    pub fn demarrer_taches(&self) {
        let transcodeur = self.lecture().transcodeur().clone();
        tokio::spawn(async move { transcodeur.preparer().await });
        crate::analyse::demarrer(self);
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
        let periode = self.config().periode_synchro;
        if self.config().lidarr.is_some() && !periode.is_zero() {
            let etat = self.clone();
            tokio::spawn(async move {
                let mut t = tokio::time::interval(periode);
                loop {
                    t.tick().await;
                    crate::catalogue::synchroniser(&etat).await;
                }
            });
        }
        crate::ajouts::demarrer(self);
    }
}
