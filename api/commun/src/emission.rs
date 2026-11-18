use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use redis::aio::MultiplexedConnection;
use tokio::{
    sync::{Mutex as MutexAsync, mpsc},
    time::{Instant, timeout},
};

use crate::evenements::Evenement;

pub const FLUX: &str = "sablier:evenements";
pub const CHAMP: &str = "e";
pub const PLAFOND: usize = 500_000;
pub const VARIABLE_URL: &str = "SABLIER_REDIS_URL";
pub const VARIABLE_PLAFOND: &str = "SABLIER_FILE_MAX";
pub const TAMPON: usize = 10_000;
const DELAI: Duration = Duration::from_secs(2);
const PAUSE_APRES_PANNE: Duration = Duration::from_secs(5);

struct Cible {
    client: redis::Client,
    cle: String,
    plafond: usize,
}

impl Cible {
    async fn ajouter(
        &self,
        connexion: &mut Option<MultiplexedConnection>,
        json: &str,
    ) -> Result<(), String> {
        let mut c = match connexion.take() {
            Some(c) => c,
            None => timeout(DELAI, self.client.get_multiplexed_async_connection())
                .await
                .map_err(|_| "connexion : délai dépassé".to_string())?
                .map_err(|e| format!("connexion : {e}"))?,
        };
        let mut commande = redis::cmd("XADD");
        commande
            .arg(&self.cle)
            .arg("MAXLEN")
            .arg("~")
            .arg(self.plafond)
            .arg("*")
            .arg(CHAMP)
            .arg(json);
        timeout(DELAI, commande.query_async::<String>(&mut c))
            .await
            .map_err(|_| "XADD : délai dépassé".to_string())?
            .map_err(|e| format!("XADD : {e}"))?;
        *connexion = Some(c);
        Ok(())
    }
}

struct Interne {
    cible: Arc<Cible>,
    envoi: mpsc::Sender<String>,
    reception: Mutex<Option<mpsc::Receiver<String>>>,
    lancee: AtomicBool,
    directe: MutexAsync<Option<MultiplexedConnection>>,
}

#[derive(Clone)]
pub struct Emetteur(Option<Arc<Interne>>);

async fn vider(cible: Arc<Cible>, mut reception: mpsc::Receiver<String>) {
    let mut connexion = None;
    let mut reprise: Option<Instant> = None;
    while let Some(json) = reception.recv().await {
        if reprise.is_some_and(|r| Instant::now() < r) {
            continue;
        }
        match cible.ajouter(&mut connexion, &json).await {
            Ok(()) => {
                if reprise.take().is_some() {
                    tracing::info!("file d'évènements : Redis de nouveau joignable");
                }
            }
            Err(e) => {
                if reprise.is_none() {
                    tracing::warn!("file d'évènements : évènements abandonnés ({e})");
                }
                reprise = Some(Instant::now() + PAUSE_APRES_PANNE);
            }
        }
    }
}

impl Emetteur {
    pub fn inactif() -> Self {
        Emetteur(None)
    }

    pub fn sur(url: &str, cle: &str, plafond: usize) -> Self {
        let url = url.trim();
        if url.is_empty() || cle.is_empty() {
            return Self::inactif();
        }
        let client = match redis::Client::open(url) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "file d'évènements : {VARIABLE_URL} invalide, émission coupée ({:?})",
                    e.kind()
                );
                return Self::inactif();
            }
        };
        let (envoi, reception) = mpsc::channel(TAMPON);
        Emetteur(Some(Arc::new(Interne {
            cible: Arc::new(Cible {
                client,
                cle: cle.to_string(),
                plafond: plafond.max(1),
            }),
            envoi,
            reception: Mutex::new(Some(reception)),
            lancee: AtomicBool::new(false),
            directe: MutexAsync::new(None),
        })))
    }

    pub fn depuis(url: Option<&str>, plafond: Option<usize>) -> Self {
        Self::sur(url.unwrap_or_default(), FLUX, plafond.unwrap_or(PLAFOND))
    }

    pub fn depuis_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let url = crate::env::url_file(&get);
        let plafond = get(VARIABLE_PLAFOND)
            .map(|v| crate::config::nettoie(&v))
            .filter(|v| !v.is_empty())
            .and_then(|v| match v.parse::<usize>() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    tracing::warn!("{VARIABLE_PLAFOND} invalide ({v}) : {PLAFOND} gardé");
                    None
                }
            });
        Self::depuis(Some(&url), plafond)
    }

    pub fn actif(&self) -> bool {
        self.0.is_some()
    }

    fn lancer(interne: &Interne) {
        if interne.lancee.load(Ordering::Acquire) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let Ok(mut garde) = interne.reception.lock() else {
            return;
        };
        if let Some(reception) = garde.take() {
            runtime.spawn(vider(interne.cible.clone(), reception));
        }
        interne.lancee.store(true, Ordering::Release);
    }

    pub fn emettre(&self, e: Evenement) {
        let Some(interne) = &self.0 else {
            return;
        };
        Self::lancer(interne);
        let _ = interne.envoi.try_send(e.json());
    }

    pub async fn publier(&self, e: &Evenement) -> Result<(), String> {
        let Some(interne) = &self.0 else {
            return Err(format!("émission inactive ({VARIABLE_URL} non renseignée)"));
        };
        let mut connexion = interne.directe.lock().await;
        interne.cible.ajouter(&mut connexion, &e.json()).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::evenements::Donnees;

    const PORT_FERME: &str = "redis://127.0.0.1:1";

    fn evenement() -> Evenement {
        Evenement::nouveau(
            "sub-alice",
            Donnees::SillonEcoute {
                piste: "p1".into(),
                secondes: 30,
                comptee: true,
            },
        )
    }

    fn mille_appels(e: &Emetteur) -> Duration {
        let debut = std::time::Instant::now();
        for _ in 0..1000 {
            e.emettre(evenement());
        }
        debut.elapsed()
    }

    #[test]
    fn inactif_sans_adresse_ou_avec_une_adresse_invalide() {
        assert!(!Emetteur::inactif().actif());
        assert!(!Emetteur::depuis(None, None).actif());
        assert!(!Emetteur::depuis(Some("  "), Some(10)).actif());
        assert!(!Emetteur::depuis(Some("pas une url"), None).actif());
        assert!(!Emetteur::depuis_env(|_| None).actif());
        assert!(Emetteur::depuis(Some(PORT_FERME), None).actif());
        let env = |k: &str| match k {
            VARIABLE_URL => Some(format!("\"{PORT_FERME}\"")),
            VARIABLE_PLAFOND => Some("x".to_string()),
            _ => None,
        };
        let e = Emetteur::depuis_env(env);
        assert_eq!(e.0.as_ref().unwrap().cible.plafond, PLAFOND);
        let env = |k: &str| match k {
            VARIABLE_URL => Some(PORT_FERME.to_string()),
            VARIABLE_PLAFOND => Some("1200".to_string()),
            _ => None,
        };
        let e = Emetteur::depuis_env(env);
        assert_eq!(e.0.as_ref().unwrap().cible.plafond, 1200);
        assert_eq!(e.0.as_ref().unwrap().cible.cle, FLUX);
    }

    #[test]
    fn emettre_hors_de_tokio_ne_panique_pas() {
        assert!(mille_appels(&Emetteur::inactif()) < Duration::from_millis(500));
        assert!(
            mille_appels(&Emetteur::depuis(Some(PORT_FERME), None)) < Duration::from_millis(500)
        );
    }

    #[tokio::test]
    async fn emettre_sans_redis_rend_la_main_aussitot() {
        let inactif = Emetteur::inactif();
        assert!(mille_appels(&inactif) < Duration::from_millis(500));
        assert!(
            inactif
                .publier(&evenement())
                .await
                .unwrap_err()
                .contains(VARIABLE_URL)
        );
        let arrete = Emetteur::depuis(Some(PORT_FERME), None);
        assert!(mille_appels(&arrete) < Duration::from_millis(500));
        tokio::task::yield_now().await;
        assert!(mille_appels(&arrete) < Duration::from_millis(500));
        for _ in 0..(2 * TAMPON / 1000) {
            assert!(mille_appels(&arrete) < Duration::from_millis(500));
        }
        let debut = std::time::Instant::now();
        let erreur = arrete.publier(&evenement()).await.unwrap_err();
        assert!(erreur.starts_with("connexion"), "{erreur}");
        assert!(debut.elapsed() < DELAI + Duration::from_secs(1));
    }

    static COMPTEUR: AtomicUsize = AtomicUsize::new(0);

    async fn redis() -> Option<(String, String, MultiplexedConnection)> {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("ignoré : TEST_REDIS_URL absent");
            return None;
        };
        let cle = format!(
            "sablier:test:emission:{}:{}",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        );
        let c = redis::Client::open(url.as_str())
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .expect("connexion à TEST_REDIS_URL");
        Some((url, cle, c))
    }

    async fn longueur(c: &mut MultiplexedConnection, cle: &str) -> usize {
        redis::cmd("XLEN").arg(cle).query_async(c).await.unwrap()
    }

    async fn dernier(c: &mut MultiplexedConnection, cle: &str) -> Option<Evenement> {
        let r: Vec<(String, Vec<String>)> = redis::cmd("XREVRANGE")
            .arg(cle)
            .arg("+")
            .arg("-")
            .arg("COUNT")
            .arg(1)
            .query_async(c)
            .await
            .unwrap();
        let (_, champs) = r.into_iter().next()?;
        assert_eq!(champs[0], CHAMP);
        Some(Evenement::lire(&champs[1]).unwrap())
    }

    async fn attendre_dernier(c: &mut MultiplexedConnection, cle: &str, attendu: &Evenement) {
        let limite = Instant::now() + Duration::from_secs(20);
        while dernier(c, cle).await.as_ref() != Some(attendu) {
            assert!(
                Instant::now() < limite,
                "délai dépassé : évènement dans le flux"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn effacer(c: &mut MultiplexedConnection, cle: &str) {
        let _: i64 = redis::cmd("DEL").arg(cle).query_async(c).await.unwrap();
    }

    #[tokio::test]
    async fn evenement_emis_arrive_dans_le_flux() {
        let Some((url, cle, mut c)) = redis().await else {
            return;
        };
        let emetteur = Emetteur::sur(&url, &cle, 1000);
        let (premier, second, troisieme) = (evenement(), evenement(), evenement());
        emetteur.emettre(premier.clone());
        emetteur.clone().emettre(second.clone());
        attendre_dernier(&mut c, &cle, &second).await;
        assert_eq!(longueur(&mut c, &cle).await, 2);
        emetteur.publier(&troisieme).await.unwrap();
        assert_eq!(dernier(&mut c, &cle).await, Some(troisieme));
        assert_eq!(longueur(&mut c, &cle).await, 3);
        effacer(&mut c, &cle).await;
    }

    #[tokio::test]
    async fn flux_plafonne_approximativement() {
        let Some((url, cle, mut c)) = redis().await else {
            return;
        };
        let emetteur = Emetteur::sur(&url, &cle, 100);
        let mut fin = evenement();
        for _ in 0..1500 {
            fin = evenement();
            emetteur.emettre(fin.clone());
        }
        attendre_dernier(&mut c, &cle, &fin).await;
        let n = longueur(&mut c, &cle).await;
        assert!(
            (100..=400).contains(&n),
            "{n} entrées pour un plafond de 100"
        );
        effacer(&mut c, &cle).await;
    }
}
