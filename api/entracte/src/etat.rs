use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::sync::Mutex as MutexAsync;

use crate::{
    config::Config,
    droits::{Aegis, Droits, Injoignable},
    flux::Flux,
    identite::{Identite, Resolveur},
    materiel::Materiel,
    push::Push,
    sous_titres::{self, Extractions},
    transcodage::Transcodeur,
};

pub struct Inner {
    cfg: Config,
    pool: PgPool,
    http: reqwest::Client,
    identites: Resolveur,
    aegis: Option<Aegis>,
    flux: Arc<Flux>,
    transcodeur: Arc<Transcodeur>,
    materiel: Materiel,
    synchro: MutexAsync<()>,
    relance: std::sync::atomic::AtomicBool,
    push: Push,
    extractions: Extractions,
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
        let materiel = Materiel::nouveau(cfg.mode_materiel);
        let push = Push::nouveau(&cfg, http.clone());
        Self(Arc::new(Inner {
            identites: Resolveur::new(&cfg.gateway_url, http.clone()),
            flux: Arc::new(Flux::default()),
            transcodeur: Arc::new(Transcodeur::nouveau(cfg.dossier_transcodage.clone())),
            cfg,
            pool,
            http,
            aegis,
            materiel,
            synchro: MutexAsync::new(()),
            relance: std::sync::atomic::AtomicBool::new(false),
            push,
            extractions: Extractions::nouveau(
                sous_titres::EXTRACTIONS_MAX,
                sous_titres::DUREE_ECHEC,
            ),
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

    pub fn flux(&self) -> &Arc<Flux> {
        &self.0.flux
    }

    pub fn transcodeur(&self) -> &Arc<Transcodeur> {
        &self.0.transcodeur
    }

    pub fn materiel(&self) -> &Materiel {
        &self.0.materiel
    }

    pub fn verrou_synchro(&self) -> &MutexAsync<()> {
        &self.0.synchro
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

    pub fn push(&self) -> &Push {
        &self.0.push
    }

    pub fn extractions(&self) -> &Extractions {
        &self.0.extractions
    }

    pub async fn droits_de(&self, id: &Identite) -> Result<Droits, Injoignable> {
        match &self.0.aegis {
            Some(a) => a.droits(id).await,
            None => Ok(Droits::default()),
        }
    }

    pub fn demarrer_taches(&self) {
        let etat = self.clone();
        if etat.aegis().is_some() {
            tokio::spawn(async move {
                let mut t = tokio::time::interval(Duration::from_secs(600));
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
            etat.materiel().verifier(etat.config()).await;
        });
        let etat = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(10));
            loop {
                t.tick().await;
                crate::lecture::menage(&etat).await;
            }
        });
        let etat = self.clone();
        let periode = self.config().periode_synchro;
        if !periode.is_zero() {
            tokio::spawn(async move {
                let mut t = tokio::time::interval(periode);
                loop {
                    t.tick().await;
                    crate::catalogue::synchroniser(&etat).await;
                }
            });
        }
    }
}
