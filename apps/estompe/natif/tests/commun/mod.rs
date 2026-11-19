#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use estompe_client::auth::{ErreurAuth, Parcours};
use estompe_client::http::ErreurApi;
use estompe_client::types::{GetMeReponse, Person, Server};
use estompe_natif::entree::{ApiEntree, Authentifieur, Futur, Services};
use estompe_plateforme::{OuvertureUrl, Resultat};
use tokio::sync::oneshot;

pub const DELAI: Duration = Duration::from_secs(10);

pub fn attendre(mut condition: impl FnMut() -> bool, quoi: &str) {
    let limite = Instant::now() + DELAI;
    while !condition() {
        assert!(
            Instant::now() < limite,
            "condition non atteinte en 10 s : {quoi}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[derive(Default)]
pub struct FausseAuth {
    pub session: Mutex<bool>,
    pub reprises: AtomicUsize,
    pub connexions: Mutex<Vec<Parcours>>,
    pub annulations: AtomicUsize,
    pub deconnexions: AtomicUsize,
    attente: Mutex<Option<oneshot::Sender<Result<(), ErreurAuth>>>>,
}

impl FausseAuth {
    pub fn avec_session(session: bool) -> Arc<Self> {
        let auth = Self::default();
        *auth.session.lock().unwrap() = session;
        Arc::new(auth)
    }

    pub fn connexions(&self) -> usize {
        self.connexions.lock().unwrap().len()
    }

    pub fn en_attente(&self) -> bool {
        self.attente
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|envoi| !envoi.is_closed())
    }

    pub fn terminer(&self, resultat: Result<(), ErreurAuth>) {
        let envoi = self
            .attente
            .lock()
            .unwrap()
            .take()
            .expect("une connexion en attente");
        envoi.send(resultat).expect("connexion toujours attendue");
    }
}

impl Authentifieur for FausseAuth {
    fn reprendre(&self) -> Futur<Result<bool, ErreurAuth>> {
        self.reprises.fetch_add(1, Ordering::SeqCst);
        let session = *self.session.lock().unwrap();
        Box::pin(async move { Ok(session) })
    }

    fn connecter(&self, parcours: Parcours) -> Futur<Result<(), ErreurAuth>> {
        let (envoi, reception) = oneshot::channel();
        self.connexions.lock().unwrap().push(parcours);
        *self.attente.lock().unwrap() = Some(envoi);
        Box::pin(async move { reception.await.unwrap_or(Err(ErreurAuth::Annulee)) })
    }

    fn annuler(&self) {
        self.annulations.fetch_add(1, Ordering::SeqCst);
        if let Some(envoi) = self.attente.lock().unwrap().take() {
            let _ = envoi.send(Err(ErreurAuth::Annulee));
        }
    }

    fn deconnecter(&self) -> Futur<()> {
        self.deconnexions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}

pub struct FausseApi {
    pub moi: Mutex<GetMeReponse>,
    pub appels: AtomicUsize,
}

impl FausseApi {
    pub fn compte_actif() -> Arc<Self> {
        Arc::new(Self {
            moi: Mutex::new(GetMeReponse {
                allowed: Some(true),
                activated: Some(true),
                id: Some("u1".into()),
                pseudo: Some("bob".into()),
                ..GetMeReponse::default()
            }),
            appels: AtomicUsize::new(0),
        })
    }
}

impl ApiEntree for FausseApi {
    fn moi(&self) -> Futur<Result<GetMeReponse, ErreurApi>> {
        self.appels.fetch_add(1, Ordering::SeqCst);
        let moi = self.moi.lock().unwrap().clone();
        Box::pin(async move { Ok(moi) })
    }

    fn activer(&self, _pseudo: String) -> Futur<Result<Person, ErreurApi>> {
        self.appels.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Person::default()) })
    }

    fn rejoindre(&self, _code: String) -> Futur<Result<Server, ErreurApi>> {
        self.appels.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Server::default()) })
    }
}

#[derive(Default)]
pub struct FauxNavigateur {
    pub ouvertes: Mutex<Vec<String>>,
}

impl OuvertureUrl for FauxNavigateur {
    fn disponible(&self) -> bool {
        true
    }

    fn ouvrir(&self, url: &str) -> Resultat<()> {
        self.ouvertes.lock().unwrap().push(url.to_string());
        Ok(())
    }
}

pub fn services(
    auth: Arc<FausseAuth>,
    api: Arc<dyn ApiEntree>,
    navigateur: Arc<FauxNavigateur>,
) -> Services {
    Services {
        auth,
        api,
        navigateur,
        lien_telechargement: "https://chat.example/".into(),
        journal: None,
    }
}
