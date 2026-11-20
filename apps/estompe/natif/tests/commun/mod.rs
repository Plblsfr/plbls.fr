#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
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

pub fn sur_l_interface<R: Send + 'static>(
    faible: &slint::Weak<estompe_natif::Principale>,
    action: impl FnOnce(&estompe_natif::Principale) -> R + Send + 'static,
) -> R {
    let (envoi, reception) = mpsc::channel();
    faible
        .upgrade_in_event_loop(move |f| {
            let _ = envoi.send(action(&f));
        })
        .expect("boucle d'évènements");
    reception
        .recv_timeout(DELAI)
        .expect("réponse de l'interface")
}

pub fn attendre_l_interface(
    faible: &slint::Weak<estompe_natif::Principale>,
    condition: impl Fn(&estompe_natif::Principale) -> bool + Send + Sync + Clone + 'static,
    quoi: &str,
) {
    attendre(
        || {
            let condition = condition.clone();
            sur_l_interface(faible, move |f| condition(f))
        },
        quoi,
    );
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
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use estompe_client::appels::Badge;
    use estompe_client::http::{Client, ErreurApi};
    use estompe_client::sections::Section;
    use estompe_client::store::{CleConversation, Etat, ParticipantVocal, Store};
    use estompe_client::temps_reel::{decoder, EtatReseau, FluxTempsReel, Signal, Trame};
    use estompe_natif::coquille::{
        CleAvatar, EchecOuverture, Fabrique, Ouverture, Profils, SourceProfils,
    };
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

    pub fn message_de(valeur: Value) -> estompe_client::types::Message {
        depuis(valeur)
    }

    fn ecrit(id: &str, lieu: (&str, &str), auteur: (&str, &str), quand: &str, contenu: &str) -> Value {
        let mut valeur = json!({"id": id, "author": {"id": auteur.0, "pseudo": auteur.1}, "content": contenu, "created_at": quand, "attachments": [], "reactions": [], "mentions": []});
        valeur[lieu.0] = json!(lieu.1);
        valeur
    }

    pub const ESTOMPE: &str = "00000000-0000-0000-0000-000000000001";

    pub fn conversation_variantes() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let mut v = vec![
            ecrit("100", c1, ("eve", "Ève"), "2026-11-18T21:40:00Z", "Bonne nuit tout le monde, à demain !"),
            ecrit("101", c1, (ESTOMPE, "Estompe"), "2026-11-19T07:58:00Z", "Inès a rejoint le serveur."),
            ecrit("102", c1, ("carol", "carol"), "2026-11-19T08:00:00Z", "Salut ! Quelqu'un a vu le **planning** de demain ?"),
            ecrit("103", c1, ("carol", "carol"), "2026-11-19T08:01:00Z", "Je l'avais posé ici mais je ne le retrouve plus."),
            ecrit("104", c1, ("dave", "dave"), "2026-11-19T08:02:00Z", "Oui, il est dans le dossier partagé, je l'ai mis à jour."),
            ecrit("105", c1, ("elise", "Élise"), "2026-11-19T08:03:00Z", "Je ne vois plus le message auquel je répondais."),
            ecrit("106", c1, (ESTOMPE, "Estompe"), "2026-11-19T08:05:00Z", "Nouvelle version disponible : 0.42 apporte la messagerie native."),
            ecrit("107", c1, ("eve", "Ève"), "2026-11-19T08:06:00Z", "@bob tu peux valider le planning avant midi ?"),
            ecrit("108", c1, ("ines", "Inès"), "2026-11-19T08:07:00Z", "Les photos du week-end :"),
            ecrit("109", c1, ("carol", "carol"), "2026-11-19T08:09:00Z", "Et ce message est mis en évidence après un saut."),
        ];
        v[1]["system"] = json!(true);
        v[2]["pinned_at"] = json!("2026-11-19T08:10:00Z");
        v[4]["reply_to"] = json!({"id": "102", "author": {"id": "carol", "pseudo": "carol"}, "content": "Salut ! Quelqu'un a vu le **planning** de demain ?"});
        v[4]["edited_at"] = json!("2026-11-19T08:04:00Z");
        v[5]["reply_to"] = json!({"id": "99", "author": {"id": "hugo", "pseudo": "hugo"}, "content": "message retiré"});
        v[7]["mentions"] = json!(["u1"]);
        v[8]["attachments"] = json!([{"id": "a1", "name": "plage.jpg"}, {"id": "a2", "name": "dune.jpg"}]);
        v[8]["reactions"] = json!([{"emoji": "👍", "count": 3, "me": true}, {"emoji": "🎉", "count": 1, "me": false}]);
        v.into_iter().map(depuis).collect()
    }

    pub const GALERIE: &str = "# Titre de niveau 1\n**Gras**, *italique*, ~~barré~~, `code en ligne`, [un lien](https://plbls.fr) et une mention de @carol.\nUn passage ||masqué|| à révéler d'un clic.\n> Une citation\n> > et une citation imbriquée\n- premier point\n- second point\n  - sous-point\n1. étape une\n2. étape deux\n```\nfn main() {\n    println!(\"Bonjour\");\n}\n```\n## Titre de niveau 2\nUn texte assez long pour passer à la ligne : la liste conserve la position du message sous le curseur quand l'historique arrive au-dessus.";

    pub fn galerie_markdown() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        vec![
            depuis(ecrit("200", c1, ("eve", "Ève"), "2026-11-19T08:30:00Z", GALERIE)),
            depuis(ecrit("201", c1, ("dave", "dave"), "2026-11-19T08:31:00Z", "Révélé : ||la fin du film|| et ### pas un titre en ligne")),
        ]
    }

    pub fn conversation_privee() -> Vec<estompe_client::types::Message> {
        let d1 = ("dm_id", "d1");
        [
            ecrit("300", d1, ("u1", "bob"), "2026-11-19T07:50:00Z", "On se voit toujours ce soir ?"),
            ecrit("301", d1, ("carol", "carol"), "2026-11-19T07:52:00Z", "Oui ! 19 h devant le cinéma."),
            ecrit("302", d1, ("carol", "carol"), "2026-11-19T08:20:00Z", "Petit changement : on se retrouve plutôt au café d'à côté."),
            ecrit("303", d1, ("carol", "carol"), "2026-11-19T08:21:00Z", "Ça te va ?"),
        ]
        .into_iter()
        .map(depuis)
        .collect()
    }

    pub fn conversation_courte(salon: &str) -> Vec<estompe_client::types::Message> {
        let lieu = ("channel_id", salon);
        [
            ecrit("400", lieu, ("carol", "carol"), "2026-11-19T08:00:00Z", "Bonjour à toutes et à tous !"),
            ecrit("401", lieu, ("dave", "dave"), "2026-11-19T08:05:00Z", "Salut, prêt pour la réunion de **10 h** ?"),
            ecrit("402", lieu, ("dave", "dave"), "2026-11-19T08:06:00Z", "L'ordre du jour : [plbls.fr/agenda](https://plbls.fr/agenda)"),
        ]
        .into_iter()
        .map(depuis)
        .collect()
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

    pub fn png(couleur: [u8; 3]) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(48, 48, |x, y| {
            if (x / 12 + y / 12) % 2 == 0 {
                image::Rgba([couleur[0], couleur[1], couleur[2], 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        });
        let mut octets = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        octets.into_inner()
    }

    #[derive(Default)]
    pub struct SourceFactice {
        pub avatars: Mutex<Vec<CleAvatar>>,
        pub badges: Mutex<Vec<String>>,
        pub badges_par_utilisateur: Mutex<BTreeMap<String, Vec<Badge>>>,
    }

    impl SourceFactice {
        pub fn requetes_avatars(&self) -> Vec<CleAvatar> {
            self.avatars.lock().unwrap().clone()
        }

        pub fn requetes_badges(&self) -> Vec<String> {
            self.badges.lock().unwrap().clone()
        }
    }

    impl SourceProfils for SourceFactice {
        fn avatar(&self, cle: &CleAvatar) -> Futur<Result<Vec<u8>, ErreurApi>> {
            self.avatars.lock().unwrap().push(cle.clone());
            Box::pin(async { Ok(png([208, 104, 138])) })
        }

        fn badges(&self, utilisateur: &str) -> Futur<Result<Vec<Badge>, ErreurApi>> {
            self.badges.lock().unwrap().push(utilisateur.to_string());
            let badges = self
                .badges_par_utilisateur
                .lock()
                .unwrap()
                .get(utilisateur)
                .cloned()
                .unwrap_or_default();
            Box::pin(async move { Ok(badges) })
        }
    }

    pub fn badge(nom: &str, couleur: &str) -> Badge {
        Badge {
            id: nom.to_lowercase(),
            name: nom.to_string(),
            icon: "🏅".into(),
            color: couleur.to_string(),
            ..Badge::default()
        }
    }

    pub fn etat_avec_membres() -> Etat {
        let mut etat = etat();
        let detail = etat.details.get_mut("s1").unwrap();
        if let Some(serveur) = detail.server.as_mut() {
            serveur.owner_id = Some("u1".into());
        }
        detail.roles = Some(depuis(json!([
            {"id": "r0", "name": "everyone", "is_default": true, "position": 0, "hoist": false, "color": "", "icon": ""},
            {"id": "rm", "name": "Modération", "position": 5, "hoist": true, "color": "#2f5d8a", "icon": ""},
            {"id": "ra", "name": "Artistes", "position": 2, "hoist": false, "color": "#b0413e", "icon": ""}
        ])));
        detail.members = Some(depuis(json!([
            {"id": "u1", "pseudo": "bob", "roles": ["rm"], "joined_at": "2025-06-01T12:00:00Z", "created_at": "2025-05-20T12:00:00Z"},
            {"id": "eve", "pseudo": "Ève", "roles": ["rm"]},
            {"id": "carol", "pseudo": "carol", "nickname": "Caro", "roles": ["ra"], "joined_at": "2026-03-04T12:00:00Z", "created_at": "2025-01-02T12:00:00Z", "banner_color": "#f2a65a"},
            {"id": "dave", "pseudo": "dave", "roles": []},
            {"id": "elise", "pseudo": "Élise", "roles": []},
            {"id": "00000000-0000-0000-0000-000000000001", "pseudo": "Estompe", "roles": []},
            {"id": "ines", "pseudo": "Inès", "roles": ["ra"]},
            {"id": "frank", "pseudo": "frank", "roles": [], "timeout_until": "2099-01-01T00:00:00Z"},
            {"id": "gaelle", "pseudo": "Gaëlle", "roles": []},
            {"id": "hugo", "pseudo": "hugo", "roles": []}
        ])));
        for present in [
            "eve",
            "dave",
            "ines",
            "00000000-0000-0000-0000-000000000001",
        ] {
            etat.en_ligne.insert(present.into());
        }
        etat.statuts.insert("carol".into(), depuis(json!("dnd")));
        etat.statuts.insert("dave".into(), depuis(json!("idle")));
        etat.activites.insert("carol".into(), "Tetris".into());
        etat.activites.insert("eve".into(), "Hades II".into());
        for (utilisateur, version) in [("eve", 2), ("carol", 1), ("gaelle", 4)] {
            etat.avatars.insert(utilisateur.into(), version);
        }
        etat
    }

    pub fn etat_nombreux(nombre: usize) -> Etat {
        let mut etat = etat();
        let membres: Vec<serde_json::Value> = (0..nombre)
            .map(|i| json!({"id": format!("m{i:05}"), "pseudo": format!("membre {i:05}"), "roles": []}))
            .collect();
        etat.details.get_mut("s1").unwrap().members = Some(depuis(json!(membres)));
        for i in 0..nombre {
            let id = format!("m{i:05}");
            if i % 3 == 0 {
                etat.en_ligne.insert(id.clone());
            }
            etat.avatars.insert(id, 1);
        }
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
        pub source: Arc<SourceFactice>,
        pub profils: Arc<Profils>,
    }

    impl FausseFabrique {
        pub fn nouvelle(etat: Etat, sections: Vec<Section>) -> Arc<Self> {
            let source = Arc::new(SourceFactice::default());
            Arc::new(Self {
                etat,
                sections,
                ouvertures: AtomicUsize::new(0),
                signaux: Mutex::new(None),
                store: Mutex::new(None),
                reseau: watch::channel(EtatReseau::Reconnexion).0,
                profils: Profils::nouveaux(source.clone(), None),
                source,
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

        fn profils(&self) -> Arc<Profils> {
            self.profils.clone()
        }
    }
}
