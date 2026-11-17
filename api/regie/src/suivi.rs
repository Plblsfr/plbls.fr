use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::sync::{Notify, watch};

use crate::{arr::Service, etat::AppState, qbittorrent, sante};

const PERIODE_TITRES: Duration = Duration::from_secs(10);
const TOURS_SANS_LECTEUR: u32 = 20;

#[derive(Default)]
pub struct Instantane {
    pub version: u64,
    pub corps: Value,
}

pub struct Suivi {
    tx: watch::Sender<Arc<Instantane>>,
    actif: AtomicBool,
    reveil: Notify,
}

impl Default for Suivi {
    fn default() -> Self {
        Self {
            tx: watch::channel(Arc::default()).0,
            actif: AtomicBool::new(false),
            reveil: Notify::new(),
        }
    }
}

impl Suivi {
    pub fn abonner(&self) -> watch::Receiver<Arc<Instantane>> {
        self.tx.subscribe()
    }

    pub fn courant(&self) -> Arc<Instantane> {
        self.tx.borrow().clone()
    }

    pub fn relire(&self) {
        self.reveil.notify_one();
    }
}

pub fn assurer(state: &AppState) {
    if !state.suivi().actif.swap(true, Ordering::SeqCst) {
        let etat = state.clone();
        tokio::spawn(async move { boucle(etat).await });
    }
}

async fn titres(state: &AppState) -> HashMap<String, Value> {
    let mut tous = HashMap::new();
    for service in Service::BIBLIOTHEQUES {
        let Some(c) = sante::client(state, service) else {
            continue;
        };
        match c.rattachements().await {
            Ok(liens) => tous.extend(liens),
            Err(e) => tracing::debug!("{} : file illisible ({e})", service.nom()),
        }
    }
    tous
}

async fn relever(state: &AppState, liens: &HashMap<String, Value>) -> Value {
    let Some(cfg) = state.config().qbittorrent.as_ref() else {
        return json!({ "error": "qBittorrent n'est pas configuré" });
    };
    let c = qbittorrent::Client::new(cfg, state.http(), state.session_qbittorrent());
    match c.etat().await {
        Ok(mut v) => {
            for t in v["torrents"].as_array_mut().into_iter().flatten() {
                let lien = t["hash"].as_str().and_then(|h| liens.get(h)).cloned();
                t["title"] = lien.unwrap_or(Value::Null);
            }
            v["error"] = Value::Null;
            v
        }
        Err(e) => {
            let mut v = state.suivi().courant().corps.clone();
            if !v.is_object() {
                v = json!({});
            }
            v["error"] = json!(e);
            v
        }
    }
}

async fn boucle(state: AppState) {
    let suivi = state.suivi();
    let mut liens = HashMap::new();
    let mut lus: Option<Instant> = None;
    let mut sans_lecteur = 0;
    loop {
        if suivi.tx.receiver_count() == 0 {
            sans_lecteur += 1;
            if sans_lecteur > TOURS_SANS_LECTEUR {
                suivi.actif.store(false, Ordering::SeqCst);
                if suivi.tx.receiver_count() == 0 || suivi.actif.swap(true, Ordering::SeqCst) {
                    return;
                }
            }
        } else {
            sans_lecteur = 0;
        }
        if lus.is_none_or(|t| t.elapsed() >= PERIODE_TITRES) {
            liens = titres(&state).await;
            lus = Some(Instant::now());
        }
        let corps = relever(&state, &liens).await;
        suivi.tx.send_if_modified(|e| {
            if e.corps == corps {
                return false;
            }
            *e = Arc::new(Instantane {
                version: e.version + 1,
                corps,
            });
            true
        });
        tokio::select! {
            () = tokio::time::sleep(state.config().periode_suivi) => {}
            () = suivi.reveil.notified() => {}
        }
    }
}
