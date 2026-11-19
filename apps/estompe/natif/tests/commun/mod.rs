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

pub mod coquille {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use estompe_client::http::Client;
    use estompe_client::sections::Section;
    use estompe_client::store::{CleConversation, Etat, ParticipantVocal, Store};
    use estompe_client::temps_reel::{decoder, EtatReseau, FluxTempsReel, Signal, Trame};
    use estompe_natif::coquille::{EchecOuverture, Fabrique, Ouverture};
    use estompe_natif::entree::Futur;
    use serde_json::{json, Value};
    use tokio::sync::{mpsc, watch};

    fn depuis<T: serde::de::DeserializeOwned>(valeur: Value) -> T {
        serde_json::from_value(valeur).unwrap()
    }

    pub fn evenement(type_evenement: &str, data: Value) -> Signal {
        match decoder(&json!({"type": type_evenement, "data": data}).to_string()) {
            Trame::Evenement(e) => Signal::Evenement(e),
            autre => panic!("trame non décodée : {autre:?}"),
        }
    }

    pub fn message(id: &str, salon: &str, auteur: &str) -> Value {
        json!({"id": id, "channel_id": salon, "author": {"id": auteur, "pseudo": auteur}, "content": "salut", "created_at": "2026-11-19T10:00:00Z", "attachments": [], "reactions": [], "mentions": []})
    }

    pub fn etat() -> Etat {
        let mut etat = Etat {
            compte: depuis(
                json!({"activated": true, "allowed": true, "id": "u1", "pseudo": "bob"}),
            ),
            serveurs: depuis(json!([
                {"id": "s1", "name": "Les amis", "owner_id": "u1", "icon_version": 0},
                {"id": "s2", "name": "Travail", "owner_id": "eve", "icon_version": 0},
                {"id": "s3", "name": "Club de lecture", "owner_id": "eve", "icon_version": 2},
                {"id": "s4", "name": "Jeux du jeudi", "owner_id": "eve", "icon_version": 0}
            ])),
            messages_prives: depuis(json!([
                {"id": "d1", "other": {"id": "carol", "pseudo": "carol"}},
                {"id": "d2", "other": {"id": "dave", "pseudo": "dave"}},
                {"id": "d3", "other": {"id": "elise", "pseudo": "Élise"}}
            ])),
            mentions: depuis(json!([
                {"message_id": "m9", "channel_id": "c3", "server_id": "s1", "author": {"id": "carol", "pseudo": "carol"}, "content": "@bob tu as vu le planning ?", "created_at": "2026-11-19T09:12:00Z", "group": false}
            ])),
            ..Etat::default()
        };
        let detail = |id: &str, nom: &str, version: i64, salons: Value| -> Value {
            json!({"server": {"id": id, "name": nom, "owner_id": "eve", "icon_version": version}, "channels": salons, "members": []})
        };
        etat.details.insert(
            "s1".into(),
            depuis(detail("s1", "Les amis", 0, json!([
                {"id": "v1", "name": "Salon vocal", "kind": "voice", "position": 1},
                {"id": "k1", "name": "Ciné du dimanche", "kind": "cinema", "position": 2},
                {"id": "m1", "name": "Playlist commune", "kind": "music", "position": 1},
                {"id": "c1", "name": "général", "kind": "text", "position": 1, "topic": "On parle de tout et de rien"},
                {"id": "c2", "name": "annonces", "kind": "text", "position": 2, "locked": true},
                {"id": "c3", "name": "organisation-des-vacances-d-ete", "kind": "text", "position": 3},
                {"id": "c4", "name": "photos", "kind": "text", "position": 4}
            ]))),
        );
        etat.details.insert(
            "s2".into(),
            depuis(detail(
                "s2",
                "Travail",
                0,
                json!([
                    {"id": "t1", "name": "équipe", "kind": "text", "position": 1},
                    {"id": "t2", "name": "réunion", "kind": "voice", "position": 1}
                ]),
            )),
        );
        etat.details.insert(
            "s3".into(),
            depuis(detail(
                "s3",
                "Club de lecture",
                2,
                json!([
                    {"id": "l1", "name": "livres", "kind": "text", "position": 1}
                ]),
            )),
        );
        etat.details.insert(
            "s4".into(),
            depuis(detail(
                "s4",
                "Jeux du jeudi",
                0,
                json!([
                    {"id": "j1", "name": "parties", "kind": "text", "position": 1}
                ]),
            )),
        );
        etat.non_lus.insert(CleConversation::Salon("c4".into()), 3);
        etat.non_lus.insert(CleConversation::Salon("c3".into()), 2);
        etat.non_lus.insert(CleConversation::Salon("j1".into()), 12);
        etat.non_lus.insert(CleConversation::Prive("d1".into()), 2);
        etat.en_ligne.insert("carol".into());
        etat.en_ligne.insert("elise".into());
        etat.vocal.insert(
            "v1".into(),
            vec![
                ParticipantVocal {
                    utilisateur: "carol".into(),
                    pseudo: "carol".into(),
                    ..ParticipantVocal::default()
                },
                ParticipantVocal {
                    utilisateur: "dave".into(),
                    pseudo: "dave".into(),
                    ..ParticipantVocal::default()
                },
            ],
        );
        etat
    }

    pub fn store(etat: Etat) -> Store {
        let store = Store::nouveau(Client::nouveau("http://127.0.0.1:9").unwrap(), &[]);
        store.installer(etat);
        store
    }

    pub struct FausseFabrique {
        pub etat: Etat,
        pub sections: Vec<Section>,
        pub ouvertures: AtomicUsize,
        pub signaux: Mutex<Option<mpsc::UnboundedSender<Signal>>>,
        pub store: Mutex<Option<Store>>,
        pub reseau: watch::Sender<EtatReseau>,
    }

    impl FausseFabrique {
        pub fn nouvelle(etat: Etat, sections: Vec<Section>) -> Arc<Self> {
            Arc::new(Self {
                etat,
                sections,
                ouvertures: AtomicUsize::new(0),
                signaux: Mutex::new(None),
                store: Mutex::new(None),
                reseau: watch::channel(EtatReseau::Reconnexion).0,
            })
        }

        pub fn envoyer(&self, signal: Signal) {
            self.signaux
                .lock()
                .unwrap()
                .as_ref()
                .expect("store ouvert")
                .send(signal)
                .expect("flux suivi");
        }

        pub fn store(&self) -> Store {
            self.store.lock().unwrap().clone().expect("store ouvert")
        }
    }

    impl Fabrique for FausseFabrique {
        fn ouvrir(&self, ordre: Vec<String>) -> Futur<Result<Ouverture, EchecOuverture>> {
            self.ouvertures.fetch_add(1, Ordering::SeqCst);
            let mut etat = self.etat.clone();
            etat.ordre_serveurs = ordre;
            let store = store(etat);
            let (signaux, flux) = FluxTempsReel::factice();
            *self.signaux.lock().unwrap() = Some(signaux);
            *self.store.lock().unwrap() = Some(store.clone());
            let reseau = self.reseau.subscribe();
            Box::pin(async move {
                Ok(Ouverture {
                    store,
                    flux,
                    reseau,
                    temps_reel: None,
                })
            })
        }

        fn sections(&self) -> Futur<Vec<Section>> {
            let sections = self.sections.clone();
            Box::pin(async move { sections })
        }
    }
}
