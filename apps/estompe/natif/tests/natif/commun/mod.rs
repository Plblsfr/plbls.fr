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
            let resultat = action(&f);
            i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
            let _ = envoi.send(resultat);
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
        installer: None,
        journal: None,
    }
}

pub fn video_hevc(dossier: &std::path::Path) -> Option<std::path::PathBuf> {
    let encodeurs = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    if !(encodeurs.contains("libx264") && encodeurs.contains("libx265")) {
        assert!(
            std::env::var_os("ESTOMPE_FFMPEG_EXIGE").is_none(),
            "ffmpeg avec libx264 et libx265 exigé (ESTOMPE_FFMPEG_EXIGE)"
        );
        println!("ignoré : ffmpeg avec libx264 et libx265 indisponible");
        return None;
    }
    let chemin = dossier.join("hevc.mp4");
    let statut = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
        .arg("testsrc=size=320x240:rate=25:duration=3")
        .args(["-c:v", "libx265", "-preset", "ultrafast", "-x265-params"])
        .args(["log-level=error", "-tag:v", "hvc1", "-y"])
        .arg(&chemin)
        .status()
        .ok()?;
    assert!(statut.success(), "vidéo HEVC de test");
    Some(chemin)
}

pub mod acces;

pub mod coquille {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use estompe_client::appels::Badge;
    use estompe_client::appels::EmojiPersonnalise;
    use estompe_client::http::{Client, ErreurApi};
    use estompe_client::sections::Section;
    use estompe_client::store::{CleConversation, Etat, ParticipantVocal, Store};
    use estompe_client::temps_reel::{decoder, EtatReseau, FluxTempsReel, Signal, Trame};
    use estompe_natif::coquille::{
        CleAvatar, EchecOuverture, Fabrique, Ouverture, Profils, SourceProfils,
    };
    use estompe_natif::entree::Futur;
    use estompe_natif::messagerie::medias::{Medias, Progres, Source, SourceMedias};
    use estompe_plateforme::{
        ContenuPressePapiers, Erreur, Fichiers, FluxSon, Micro, PressePapiers, ProducteurSon,
        RecepteurSon, SortieSon,
    };
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

    fn ecrit(
        id: &str,
        lieu: (&str, &str),
        auteur: (&str, &str),
        quand: &str,
        contenu: &str,
    ) -> Value {
        let mut valeur = json!({"id": id, "author": {"id": auteur.0, "pseudo": auteur.1}, "content": contenu, "created_at": quand, "attachments": [], "reactions": [], "mentions": []});
        valeur[lieu.0] = json!(lieu.1);
        valeur
    }

    pub const ESTOMPE: &str = "00000000-0000-0000-0000-000000000001";

    pub fn conversation_variantes() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let mut v = vec![
            ecrit(
                "100",
                c1,
                ("eve", "Ève"),
                "2026-11-18T21:40:00Z",
                "Bonne nuit tout le monde, à demain !",
            ),
            ecrit(
                "101",
                c1,
                (ESTOMPE, "Estompe"),
                "2026-11-19T07:58:00Z",
                "Inès a rejoint le serveur.",
            ),
            ecrit(
                "102",
                c1,
                ("carol", "carol"),
                "2026-11-19T08:00:00Z",
                "Salut ! Quelqu'un a vu le **planning** de demain ?",
            ),
            ecrit(
                "103",
                c1,
                ("carol", "carol"),
                "2026-11-19T08:01:00Z",
                "Je l'avais posé ici mais je ne le retrouve plus.",
            ),
            ecrit(
                "104",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:02:00Z",
                "Oui, il est dans le dossier partagé, je l'ai mis à jour.",
            ),
            ecrit(
                "105",
                c1,
                ("elise", "Élise"),
                "2026-11-19T08:03:00Z",
                "Je ne vois plus le message auquel je répondais.",
            ),
            ecrit(
                "106",
                c1,
                (ESTOMPE, "Estompe"),
                "2026-11-19T08:05:00Z",
                "Nouvelle version disponible : 0.42 apporte la messagerie native.",
            ),
            ecrit(
                "107",
                c1,
                ("eve", "Ève"),
                "2026-11-19T08:06:00Z",
                "@bob tu peux valider le planning avant midi ?",
            ),
            ecrit(
                "108",
                c1,
                ("ines", "Inès"),
                "2026-11-19T08:07:00Z",
                "Les photos du week-end :",
            ),
            ecrit(
                "109",
                c1,
                ("carol", "carol"),
                "2026-11-19T08:09:00Z",
                "Et ce message est mis en évidence après un saut.",
            ),
        ];
        v[1]["system"] = json!(true);
        v[2]["pinned_at"] = json!("2026-11-19T08:10:00Z");
        v[4]["reply_to"] = json!({"id": "102", "author": {"id": "carol", "pseudo": "carol"}, "content": "Salut ! Quelqu'un a vu le **planning** de demain ?"});
        v[4]["edited_at"] = json!("2026-11-19T08:04:00Z");
        v[5]["reply_to"] = json!({"id": "99", "author": {"id": "hugo", "pseudo": "hugo"}, "content": "message retiré"});
        v[7]["mentions"] = json!(["u1"]);
        v[8]["attachments"] = json!([{"id": "a1", "name": "plage.jpg", "mime": "image/jpeg", "size": 812345}, {"id": "a2", "name": "dune.jpg", "mime": "image/jpeg", "size": 645000}]);
        v[8]["reactions"] = json!([{"emoji": "👍", "count": 3, "me": true}, {"emoji": "🎉", "count": 1, "me": false}]);
        v.into_iter().map(depuis).collect()
    }

    pub const GALERIE: &str = "# Titre de niveau 1\n**Gras**, *italique*, ~~barré~~, `code en ligne`, [un lien](https://plbls.fr) et une mention de @carol.\nUn passage ||masqué|| à révéler d'un clic.\n> Une citation\n> > et une citation imbriquée\n- premier point\n- second point\n  - sous-point\n1. étape une\n2. étape deux\n```\nfn main() {\n    println!(\"Bonjour\");\n}\n```\n## Titre de niveau 2\nUn texte assez long pour passer à la ligne : la liste conserve la position du message sous le curseur quand l'historique arrive au-dessus.";

    pub fn galerie_markdown() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        vec![
            depuis(ecrit(
                "200",
                c1,
                ("eve", "Ève"),
                "2026-11-19T08:30:00Z",
                GALERIE,
            )),
            depuis(ecrit(
                "201",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:31:00Z",
                "Révélé : ||la fin du film|| et ### pas un titre en ligne",
            )),
        ]
    }

    pub fn conversation_emoji() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let mut v = vec![
            ecrit(
                "500",
                c1,
                ("carol", "carol"),
                "2026-11-19T08:10:00Z",
                "Bravo pour la démo 🎉🎉 ! On fête ça vendredi 🍕🍺 ?",
            ),
            ecrit(
                "501",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:11:00Z",
                "Drapeaux du tournoi : 🇫🇷 🇧🇪 🇨🇦 🇯🇵, et l'équipe 👩🏽‍💻👨🏿‍💻🧑🏻‍🎨 au complet 👋🏾",
            ),
            ecrit("502", c1, ("eve", "Ève"), "2026-11-19T08:12:00Z", "🥳🎂"),
            ecrit(
                "503",
                c1,
                ("eve", "Ève"),
                "2026-11-19T08:12:30Z",
                "Les emoji du serveur : :chaton: et :fete: dans une phrase.",
            ),
            ecrit(
                "504",
                c1,
                ("ines", "Inès"),
                "2026-11-19T08:13:00Z",
                ":chaton:",
            ),
            ecrit(
                "505",
                c1,
                ("ines", "Inès"),
                "2026-11-19T08:14:00Z",
                "Famille 👨‍👩‍👧‍👦, drapeau 🏳️‍🌈, `code` et © en texte simple.",
            ),
        ];
        v[0]["reactions"] = json!([
            {"emoji": "👍", "count": 4, "me": true, "users": [{"id": "u1", "pseudo": "bob"}, {"id": "dave", "pseudo": "dave"}, {"id": "eve", "pseudo": "Ève"}, {"id": "ines", "pseudo": "Inès"}]},
            {"emoji": "❤️", "count": 2, "me": false, "users": [{"id": "dave", "pseudo": "dave"}, {"id": "eve", "pseudo": "Ève"}]},
            {"emoji": "😂", "count": 1, "me": false, "users": [{"id": "ines", "pseudo": "Inès"}]}
        ]);
        v[5]["reactions"] = json!([
            {"emoji": "🇫🇷", "count": 1, "me": false, "users": [{"id": "dave", "pseudo": "dave"}]},
            {"emoji": "👍🏽", "count": 2, "me": true, "users": [{"id": "u1", "pseudo": "bob"}, {"id": "carol", "pseudo": "carol"}]}
        ]);
        v.into_iter().map(depuis).collect()
    }

    pub fn conversation_pieces() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let mut v = vec![
            ecrit(
                "600",
                c1,
                ("ines", "Inès"),
                "2026-11-19T08:20:00Z",
                "Les photos du week-end :",
            ),
            ecrit(
                "601",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:22:00Z",
                "Le compte rendu, l'enregistrement et l'ancienne archive :",
            ),
            ecrit("602", c1, ("carol", "carol"), "2026-11-19T08:23:00Z", ""),
        ];
        v[0]["attachments"] = json!([
            {"id": "a1", "name": "plage.jpg", "mime": "image/jpeg", "size": 812345},
            {"id": "a2", "name": "dune.jpg", "mime": "image/jpeg", "size": 645000},
            {"id": "a3", "name": "coucher.png", "mime": "image/png", "size": 1203000}
        ]);
        v[1]["attachments"] = json!([
            {"id": "f1", "name": "compte-rendu de la réunion.pdf", "mime": "application/pdf", "size": 245760},
            {"id": "f2", "name": "budget.ods", "mime": "application/vnd.oasis.opendocument.spreadsheet", "size": 18432},
            {"id": "v1", "name": "reunion.mp4", "mime": "video/mp4", "size": 52428800},
            {"id": "x1", "name": "vieille-archive.zip", "mime": "application/zip", "size": 1024, "expired": true}
        ]);
        v[2]["attachments"] = json!([
            {"id": "g1", "name": "danse.gif", "mime": "image/gif", "size": 32000},
            {"id": "m1", "name": "message vocal", "kind": "voice", "duration_ms": 4000}
        ]);
        v.into_iter().map(depuis).collect()
    }

    pub fn forme_de_test(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| (30 + (i * 37) % 60 + if i % 7 == 3 { 10 } else { 0 }) as u8)
            .collect()
    }

    pub fn conversation_vocaux() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let mut v = vec![
            ecrit("700", c1, ("ines", "Inès"), "2026-11-19T08:20:00Z", ""),
            ecrit(
                "701",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:22:00Z",
                "Je réponds en vocal :",
            ),
            ecrit("702", c1, ("carol", "carol"), "2026-11-19T08:23:00Z", ""),
        ];
        v[0]["attachments"] = json!([
            {"id": "m1", "name": "message-vocal.webm", "mime": "audio/webm", "size": 48000, "kind": "voice", "duration_ms": 12_400, "waveform": forme_de_test(40)}
        ]);
        v[1]["attachments"] = json!([
            {"id": "m2", "name": "message-vocal.ogg", "mime": "audio/ogg", "size": 9000, "kind": "voice", "duration_ms": 2_300, "waveform": forme_de_test(23)}
        ]);
        v[2]["attachments"] = json!([
            {"id": "m3", "name": "message-vocal.m4a", "mime": "audio/mp4", "size": 7000, "kind": "voice", "duration_ms": 1_800, "waveform": [50]},
            {"id": "m4", "name": "message-vocal.ogg", "mime": "audio/ogg", "size": 7000, "kind": "voice", "duration_ms": 1_800, "expired": true}
        ]);
        v.into_iter().map(depuis).collect()
    }

    pub fn conversation_transcriptions() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        let auteurs = [
            ("ines", "Inès"),
            ("dave", "dave"),
            ("carol", "carol"),
            ("u1", "bob"),
            ("eve", "Ève"),
        ];
        let mut v: Vec<Value> = auteurs
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let mut m = ecrit(
                    &format!("{}", 720 + i),
                    c1,
                    *a,
                    &format!("2026-11-19T08:{:02}:00Z", 20 + i * 3),
                    "",
                );
                m["attachments"] = json!([
                    {"id": format!("t{}", i + 1), "name": "message-vocal.ogg", "mime": "audio/ogg", "size": 9000, "kind": "voice", "duration_ms": (4_000 + i as i64 * 3_100), "waveform": forme_de_test(30 + i)}
                ]);
                m
            })
            .collect();
        v[2]["attachments"][0]["transcript"] = json!("Salut tout le monde, je confirme pour samedi : on se retrouve à 19 h devant le cinéma, j'apporte les billets. Si quelqu'un est en retard, prévenez-moi dans le salon, je garderai les places au fond de la salle.");
        v[2]["attachments"][0]["transcript_auto"] = json!(true);
        v[3]["attachments"][0]["transcript"] = json!("Parfait, je serai là à 19 h.");
        v[3]["attachments"][0]["transcript_auto"] = json!(false);
        v.into_iter().map(depuis).collect()
    }

    pub fn conversation_cartes() -> Vec<estompe_client::types::Message> {
        let c1 = ("channel_id", "c1");
        [
            ecrit(
                "700",
                c1,
                ("dave", "dave"),
                "2026-11-19T08:30:00Z",
                "L'article dont je parlais : https://plbls.fr/journal/estompe-natif",
            ),
            ecrit(
                "701",
                c1,
                ("eve", "Ève"),
                "2026-11-19T08:31:00Z",
                "Et sans image : https://exemple.fr/article",
            ),
            ecrit(
                "702",
                c1,
                ("carol", "carol"),
                "2026-11-19T08:32:00Z",
                "https://youtu.be/dQw4w9WgXcQ",
            ),
        ]
        .into_iter()
        .map(depuis)
        .collect()
    }

    pub fn sondage(id: &str, question: &str, clos_a: &str, options: Value) -> Value {
        let mut m = ecrit(
            id,
            ("channel_id", "c1"),
            ("ines", "Inès"),
            "2026-11-19T08:33:00Z",
            question,
        );
        m["poll"] = json!({"multiple": false, "closes_at": clos_a, "created_by": {"id": "ines", "pseudo": "Inès"}, "options": options});
        m
    }

    pub fn conversation_sondages() -> Vec<estompe_client::types::Message> {
        let mut clos = sondage(
            "703",
            "Film de dimanche ?",
            "2026-11-19T07:00:00Z",
            json!([{"id": "o1", "label": "Dune", "votes": 5}, {"id": "o2", "label": "Alien", "votes": 2, "me": true}]),
        );
        clos["created_at"] = json!("2026-11-18T20:00:00Z");
        vec![
            depuis(clos),
            depuis(sondage(
                "704",
                "On mange où **vendredi** ?",
                "2026-11-19T10:30:00Z",
                json!([{"id": "o1", "label": "Pizza", "votes": 3, "me": true}, {"id": "o2", "label": "Sushi", "votes": 1}, {"id": "o3", "label": "Crêpes", "votes": 2}]),
            )),
        ]
    }

    pub fn conversation_privee() -> Vec<estompe_client::types::Message> {
        let d1 = ("dm_id", "d1");
        [
            ecrit(
                "300",
                d1,
                ("u1", "bob"),
                "2026-11-19T07:50:00Z",
                "On se voit toujours ce soir ?",
            ),
            ecrit(
                "301",
                d1,
                ("carol", "carol"),
                "2026-11-19T07:52:00Z",
                "Oui ! 19 h devant le cinéma.",
            ),
            ecrit(
                "302",
                d1,
                ("carol", "carol"),
                "2026-11-19T08:20:00Z",
                "Petit changement : on se retrouve plutôt au café d'à côté.",
            ),
            ecrit(
                "303",
                d1,
                ("carol", "carol"),
                "2026-11-19T08:21:00Z",
                "Ça te va ?",
            ),
        ]
        .into_iter()
        .map(depuis)
        .collect()
    }

    pub fn conversation_courte(salon: &str) -> Vec<estompe_client::types::Message> {
        let lieu = ("channel_id", salon);
        [
            ecrit(
                "400",
                lieu,
                ("carol", "carol"),
                "2026-11-19T08:00:00Z",
                "Bonjour à toutes et à tous !",
            ),
            ecrit(
                "401",
                lieu,
                ("dave", "dave"),
                "2026-11-19T08:05:00Z",
                "Salut, prêt pour la réunion de **10 h** ?",
            ),
            ecrit(
                "402",
                lieu,
                ("dave", "dave"),
                "2026-11-19T08:06:00Z",
                "L'ordre du jour : [plbls.fr/agenda](https://plbls.fr/agenda)",
            ),
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

    pub fn gif_anime(images: usize) -> Vec<u8> {
        let mut octets = Vec::new();
        {
            let mut encodeur = image::codecs::gif::GifEncoder::new(&mut octets);
            for i in 0..images {
                let couleur = image::Rgba([(i * 50) as u8, 120, 200, 255]);
                encodeur
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(60, 40, couleur),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        octets
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

    pub fn reponses_admin() -> BTreeMap<&'static str, Value> {
        let mut reponses = BTreeMap::new();
        let mut json = |chemin: &'static str, valeur: Value| {
            reponses.insert(chemin, valeur);
        };
        json(
            "/admin/users",
            json!([
                {"id": "u2", "pseudo": "alice", "created_at": "2026-02-03T10:00:00Z", "suspended": false, "servers": 3, "messages": 1520},
                {"id": "u3", "pseudo": "marc", "created_at": "2026-05-14T10:00:00Z", "suspended": true, "servers": 1, "messages": 12}
            ]),
        );
        json(
            "/admin/overview",
            json!({
                "version": "0.9.2",
                "servers": [
                    {"id": "s1", "name": "Les amis", "members": 12, "channels": 5, "messages": 48211, "last_message_at": "2026-11-19T11:42:00Z"},
                    {"id": "s9", "name": "Atelier", "members": 3, "channels": 2, "messages": 0, "last_message_at": null}
                ],
                "config": {
                    "settings": {"push_web": true, "push_android": false, "giphy": true, "metrics": false, "allow_video": true, "livekit_url": "wss://voix.example", "max_storage": 21474836480u64, "origins": ["https://chat.example"]},
                    "max_file_size": 52428800, "max_participants": 25, "screen_max_fps": 30, "screen_max_height": 1080
                }
            }),
        );
        json(
            "/admin/badges",
            json!([
                {"id": "b1", "name": "Fondateur", "icon": "🏅", "color": "#d0688a", "description": "Présent depuis le premier jour", "icon_version": 0, "sort_order": 1, "created_at": "2026-01-01T00:00:00Z", "holders": [{"id": "u2", "pseudo": "alice"}, {"id": "u1", "pseudo": "bob"}]},
                {"id": "b2", "name": "Étoile", "icon": "⭐", "color": "#e0a030", "description": "", "icon_version": 0, "sort_order": 2, "created_at": "2026-03-01T00:00:00Z", "holders": []},
                {"id": "b3", "name": "Artiste", "icon": "🎨", "color": "#2f5d8a", "description": "Dessins et musique", "icon_version": 0, "sort_order": 3, "created_at": "2026-02-01T00:00:00Z", "holders": [{"id": "u3", "pseudo": "marc"}]}
            ]),
        );
        json(
            "/admin/bot",
            json!({"pseudo": "Estompe", "releases": ["0.9.0", "0.9.1"]}),
        );
        json(
            "/admin/stats",
            json!({"users": 1284, "new_users_7d": 31, "suspended": 2, "servers": 87, "messages": 412350, "messages_24h": 2311, "files": 5120, "storage_bytes": 7516192768u64, "errors_24h": 3}),
        );
        json(
            "/admin/live",
            json!({"connections": 42, "online": [{"pseudo": "alice", "platform": "windows", "version": "0.9.2"}, {"pseudo": "bob", "platform": "", "version": "0.9.1"}], "voice": [{"server": "Les amis", "channel": "Salon vocal", "participants": [{"user_id": "u2", "pseudo": "alice", "screen": true}, {"user_id": "u1", "pseudo": "bob", "camera": true}]}]}),
        );
        json(
            "/admin/log",
            json!([
                {"at": "2026-11-19T10:00:00Z", "actor": "eve", "action": "badge.grant", "target": "Fondateur"},
                {"at": "2026-11-18T09:30:00Z", "actor": "eve", "action": "user.suspend", "target": ""}
            ]),
        );
        json(
            "/admin/errors",
            json!([{"at": "2026-11-19T08:00:00Z", "method": "GET", "path": "/servers/s1", "status": 502}]),
        );
        reponses
    }

    pub fn reponses_signalements() -> BTreeMap<&'static str, Value> {
        let mut reponses = BTreeMap::new();
        let liste = json!([
            {"id": "r1", "author": "alice", "created_at": "2026-11-19T09:12:00Z", "title": "Le vocal coupe après cinq minutes", "category": "vocal", "platform": "desktop", "status": "new", "duplicate_of": null, "seen": false},
            {"id": "r2", "author": "marc", "created_at": "2026-11-18T17:40:00Z", "title": "Notifications en double", "category": "notifications", "platform": "android", "status": "in_progress", "duplicate_of": null, "seen": true},
            {"id": "r3", "author": "bob", "created_at": "2026-11-17T08:05:00Z", "title": "Partage d'écran noir", "category": "ecran", "platform": "web", "status": "resolved", "duplicate_of": null, "seen": true}
        ]);
        reponses.insert("/admin/reports", liste);
        reponses.insert(
            "/admin/reports/r1",
            json!({"id": "r1", "author": "alice", "created_at": "2026-11-19T09:12:00Z", "title": "Le vocal coupe après cinq minutes", "category": "vocal", "platform": "desktop", "status": "new", "duplicate_of": null, "seen": true,
                "description": "Dans le salon vocal des amis, le son se coupe au bout de cinq minutes. Je dois quitter puis revenir.",
                "tech": {"Plateforme": "desktop", "Version": "0.9.2", "Système": "windows x86_64", "Écran": "serveur « Les amis », salon « Salon vocal »", "Vocal": "connecte dans « Salon vocal »"},
                "logs": "2026-11-19 09:10:02.481 [INFO] vocal: connexion au salon v1\n2026-11-19 09:15:03.004 [WARN] vocal: reconnexion",
                "notes": "", "attachments": [{"id": "fr1", "expired": false}]}),
        );
        reponses.insert(
            "/reports",
            json!([
                {"id": "m1", "created_at": "2026-11-19T09:12:00Z", "title": "Le vocal coupe après cinq minutes", "category": "vocal", "platform": "desktop", "status": "new", "duplicate_of": null},
                {"id": "m2", "created_at": "2026-11-02T14:00:00Z", "title": "Emoji illisible en mode sombre", "category": "", "platform": "desktop", "status": "resolved", "duplicate_of": null}
            ]),
        );
        reponses
    }

    pub fn servir_les_signalements(serveur: &ServeurMessages) {
        for (chemin, valeur) in reponses_signalements() {
            serveur.servir_fichier(chemin, "application/json", valeur.to_string().into_bytes());
        }
        serveur.servir_fichier("/files/fr1", "image/png", png([210, 120, 150]));
    }

    pub fn servir_l_administration(serveur: &ServeurMessages) {
        for (chemin, valeur) in reponses_admin() {
            serveur.servir_fichier(chemin, "application/json", valeur.to_string().into_bytes());
        }
    }

    pub fn etat_administrateur() -> Etat {
        let mut etat = etat();
        etat.compte.admin = Some(true);
        etat
    }

    pub fn store(etat: Etat) -> Store {
        store_sur(etat, "http://127.0.0.1:9")
    }

    pub fn store_sur(etat: Etat, adresse: &str) -> Store {
        let reglages = estompe_client::http::Reglages {
            mandataire_systeme: false,
            ..estompe_client::http::Reglages::default()
        };
        let store = Store::nouveau(Client::avec_reglages(adresse, reglages).unwrap(), &[]);
        store.installer(etat);
        store
    }

    pub struct ServeurMessages {
        pub adresse: String,
        pub nombre: u64,
        pub requetes: Mutex<Vec<String>>,
        bloque: Mutex<bool>,
        reveil: std::sync::Condvar,
        fichiers: Mutex<BTreeMap<String, (String, Vec<u8>)>>,
        messages: Mutex<BTreeMap<u64, Value>>,
        pub corps: Mutex<Vec<(String, String)>>,
        pub refus_d_envoi: Mutex<Option<u16>>,
        pub refus_de_fichier: Mutex<Option<u16>>,
        pub fichiers_bloques: Mutex<bool>,
        pub refus_de_gifs: Mutex<Option<u16>>,
        pub refus_de_recherche: Mutex<Option<u16>>,
        pub refus_vocal: Mutex<Option<u16>>,
        pieces: Mutex<BTreeMap<String, Value>>,
        pub contenus: Mutex<BTreeMap<String, Vec<u8>>>,
        pub serveurs_ajoutes: Mutex<Vec<Value>>,
        pub emojis: Mutex<Vec<Value>>,
        pub invitations: Mutex<Vec<Value>>,
        pub bannis: Mutex<Vec<Value>>,
        pub detail_s1: Mutex<Option<Value>>,
        pub surcharges: Mutex<Vec<Value>>,
        pub repondeur: Mutex<Option<Repondeur>>,
        envoyes: AtomicUsize,
    }

    pub type Repondeur = Arc<dyn Fn(&str, &str, &str) -> Option<(u16, String)> + Send + Sync>;

    pub fn contenu_de(n: u64) -> String {
        if n.is_multiple_of(7) {
            format!("message n°{n}\nUne deuxième ligne pour varier la hauteur, avec assez de mots pour passer à la ligne dans une fenêtre étroite.\nEt une troisième.")
        } else {
            format!("message n°{n}")
        }
    }

    pub fn message_numero(n: u64, nombre: u64) -> Value {
        let auteur = if (n / 3).is_multiple_of(2) {
            "carol"
        } else {
            "dave"
        };
        let mut m = json!({
            "id": n.to_string(), "channel_id": "c1",
            "author": {"id": auteur, "pseudo": auteur},
            "content": contenu_de(n),
            "created_at": format!("2026-11-{:02}T{:02}:{:02}:{:02}Z", 1 + (n / 4000).min(17), (n / 240) % 24, (n / 4) % 60, (n * 15) % 60),
            "attachments": [], "reactions": [], "mentions": []
        });
        if n == nombre {
            m["reply_to"] = json!({"id": "3", "author": {"id": "carol", "pseudo": "carol"}, "content": contenu_de(3)});
        }
        m
    }

    fn parametre(requete: &str, nom: &str) -> Option<u64> {
        let (_, requete) = requete.split_once('?')?;
        requete.split('&').find_map(|p| {
            let (cle, valeur) = p.split_once('=')?;
            (cle == nom).then(|| valeur.parse().ok()).flatten()
        })
    }

    impl ServeurMessages {
        pub fn demarrer(nombre: u64) -> Arc<Self> {
            use std::io::{BufRead, BufReader, Write};
            let ecoute = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let serveur = Arc::new(Self {
                adresse: format!("http://{}", ecoute.local_addr().unwrap()),
                nombre,
                requetes: Mutex::new(Vec::new()),
                bloque: Mutex::new(false),
                reveil: std::sync::Condvar::new(),
                fichiers: Mutex::new(BTreeMap::new()),
                messages: Mutex::new(BTreeMap::new()),
                corps: Mutex::new(Vec::new()),
                refus_d_envoi: Mutex::new(None),
                refus_de_fichier: Mutex::new(None),
                fichiers_bloques: Mutex::new(false),
                refus_de_gifs: Mutex::new(None),
                refus_de_recherche: Mutex::new(None),
                refus_vocal: Mutex::new(None),
                pieces: Mutex::new(BTreeMap::new()),
                contenus: Mutex::new(BTreeMap::new()),
                serveurs_ajoutes: Mutex::new(Vec::new()),
                emojis: Mutex::new(Vec::new()),
                invitations: Mutex::new(Vec::new()),
                bannis: Mutex::new(Vec::new()),
                detail_s1: Mutex::new(None),
                surcharges: Mutex::new(Vec::new()),
                repondeur: Mutex::new(None),
                envoyes: AtomicUsize::new(0),
            });
            let s = serveur.clone();
            std::thread::spawn(move || {
                for flux in ecoute.incoming() {
                    let Ok(mut flux) = flux else { return };
                    let s = s.clone();
                    std::thread::spawn(move || {
                        let mut lecteur = BufReader::new(flux.try_clone().unwrap());
                        let mut premiere = String::new();
                        if lecteur.read_line(&mut premiere).unwrap_or(0) == 0 {
                            return;
                        }
                        let mut longueur = 0usize;
                        loop {
                            let mut ligne = String::new();
                            if lecteur.read_line(&mut ligne).unwrap_or(0) == 0 || ligne == "\r\n" {
                                break;
                            }
                            if let Some((nom, valeur)) = ligne.split_once(':') {
                                if nom.trim().eq_ignore_ascii_case("content-length") {
                                    longueur = valeur.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                        if premiere.starts_with("POST /files ")
                            && *s.fichiers_bloques.lock().unwrap()
                        {
                            use std::io::Read;
                            s.requetes.lock().unwrap().push("POST /files bloqué".into());
                            let mut tampon = vec![0u8; 64 * 1024];
                            while matches!(lecteur.read(&mut tampon), Ok(n) if n > 0) {}
                            s.requetes.lock().unwrap().push("FERMÉE /files".into());
                            return;
                        }
                        let mut octets = vec![0u8; longueur];
                        if longueur > 0 {
                            use std::io::Read;
                            if lecteur.read_exact(&mut octets).is_err() {
                                return;
                            }
                        }
                        let corps_recu = String::from_utf8_lossy(&octets).into_owned();
                        if premiere.starts_with("POST /files ") {
                            let (statut, corps) = s.recevoir_fichier(&octets);
                            let _ = write!(
                                flux,
                                "HTTP/1.1 {statut} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{corps}",
                                corps.len()
                            );
                            return;
                        }
                        let mut morceaux = premiere.split_whitespace();
                        let methode = morceaux.next().unwrap_or_default().to_string();
                        let chemin = morceaux.next().unwrap_or_default().to_string();
                        if longueur > 0 || methode != "GET" {
                            s.corps
                                .lock()
                                .unwrap()
                                .push((format!("{methode} {chemin}"), corps_recu.clone()));
                        }
                        if methode == "PUT" {
                            if let Some(prefixe) = chemin.strip_suffix("/me/avatar") {
                                s.servir_fichier(
                                    &format!("{prefixe}/users/u1/avatar"),
                                    "image/png",
                                    octets.clone(),
                                );
                            }
                        }
                        let servi = if methode == "GET" {
                            s.fichier(&chemin)
                        } else {
                            None
                        };
                        let repondeur = s.repondeur.lock().unwrap().clone();
                        let repondu = repondeur.and_then(|r| r(&methode, &chemin, &corps_recu));
                        if repondu.is_some() {
                            s.requetes
                                .lock()
                                .unwrap()
                                .push(format!("{methode} {chemin}"));
                        }
                        let (statut, type_mime, corps) = match (repondu, servi) {
                            (Some((statut, corps)), _) => {
                                (statut, "application/json".to_string(), corps.into_bytes())
                            }
                            (None, Some((type_mime, octets))) => (200, type_mime, octets),
                            (None, None) => {
                                let (statut, corps) =
                                    s.repondre_avec(&methode, &chemin, &corps_recu);
                                (statut, "application/json".to_string(), corps.into_bytes())
                            }
                        };
                        let _ = write!(
                            flux,
                            "HTTP/1.1 {statut} OK\r\ncontent-type: {type_mime}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            corps.len()
                        );
                        let _ = flux.write_all(&corps);
                    });
                }
            });
            serveur
        }

        pub fn servir_fichier(&self, chemin: &str, type_mime: &str, octets: Vec<u8>) {
            self.fichiers
                .lock()
                .unwrap()
                .insert(chemin.to_string(), (type_mime.to_string(), octets));
        }

        pub fn remplacer_message(&self, n: u64, message: Value) {
            self.messages.lock().unwrap().insert(n, message);
        }

        fn fichier(&self, chemin: &str) -> Option<(String, Vec<u8>)> {
            let sans_parametres = chemin.split('?').next().unwrap_or(chemin);
            let trouve = self.fichiers.lock().unwrap().get(sans_parametres).cloned();
            if trouve.is_some() {
                self.requetes.lock().unwrap().push(format!("GET {chemin}"));
            }
            trouve
        }

        pub fn bloquer(&self, bloque: bool) {
            *self.bloque.lock().unwrap() = bloque;
            self.reveil.notify_all();
        }

        pub fn nombre_de(&self, debut: &str) -> usize {
            self.requetes
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.starts_with(debut))
                .count()
        }

        pub fn envois(&self) -> Vec<String> {
            self.corps
                .lock()
                .unwrap()
                .iter()
                .filter(|(r, _)| r == "POST /channels/c1/messages")
                .map(|(_, c)| {
                    serde_json::from_str::<Value>(c).unwrap()["content"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        }

        fn recevoir_fichier(&self, octets: &[u8]) -> (u16, String) {
            let corps = String::from_utf8_lossy(octets);
            let position = |motif: &[u8], depuis: usize| {
                octets[depuis..]
                    .windows(motif.len())
                    .position(|f| f == motif)
                    .map(|p| p + depuis)
            };
            let debut = position(b"filename=", 0)
                .and_then(|p| position(b"\r\n\r\n", p))
                .map_or(0, |p| p + 4);
            let fin = octets
                .windows(4)
                .rposition(|f| f == b"\r\n--")
                .unwrap_or(octets.len());
            let longueur = fin.saturating_sub(debut);
            self.requetes.lock().unwrap().push("POST /files".into());
            if let Some(statut) = *self.refus_de_fichier.lock().unwrap() {
                return (
                    statut,
                    r#"{"error":"storage_full","message":"Stockage plein"}"#.into(),
                );
            }
            let champ = |debut: &str, fin: &str| {
                corps
                    .split_once(debut)
                    .and_then(|(_, reste)| reste.split_once(fin))
                    .map(|(v, _)| v.to_string())
                    .unwrap_or_default()
            };
            let mut pieces = self.pieces.lock().unwrap();
            let id = format!("f{}", pieces.len() + 1);
            let genre = if corps.contains("name=\"kind\"\r\n\r\nvideo") {
                "video"
            } else if corps.contains("name=\"kind\"\r\n\r\nvoice") {
                "voice"
            } else {
                "file"
            };
            let mut piece = json!({"id": id, "name": champ("filename=\"", "\""), "mime": champ("Content-Type: ", "\r\n"), "size": longueur, "kind": genre});
            if genre == "voice" {
                piece["duration_ms"] = json!(champ("name=\"duration_ms\"\r\n\r\n", "\r\n")
                    .parse::<u64>()
                    .unwrap_or(0));
                piece["waveform"] = champ("name=\"waveform\"\r\n\r\n", "\r\n")
                    .split(',')
                    .filter_map(|v| v.parse::<u8>().ok())
                    .collect::<Vec<_>>()
                    .into();
            }
            self.contenus
                .lock()
                .unwrap()
                .insert(id.clone(), octets[debut..debut + longueur].to_vec());
            pieces.insert(id, piece.clone());
            (201, piece.to_string())
        }

        pub fn identifiants_envoyes(&self, requete: &str) -> Vec<Vec<String>> {
            self.corps
                .lock()
                .unwrap()
                .iter()
                .filter(|(r, _)| r == requete)
                .map(|(_, c)| {
                    serde_json::from_str::<Value>(c).unwrap()["file_ids"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|v| v.as_str().unwrap_or_default().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        }

        pub fn piece(&self, id: &str) -> Value {
            self.pieces.lock().unwrap()[id].clone()
        }

        pub fn nom_de_piece(&self, id: &str) -> String {
            self.pieces.lock().unwrap()[id]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        }

        fn pieces_de(&self, recu: &Value) -> Value {
            let pieces = self.pieces.lock().unwrap();
            recu["file_ids"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| pieces.get(id.as_str()?).cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into()
        }

        fn repondre_avec(&self, methode: &str, chemin: &str, corps: &str) -> (u16, String) {
            if chemin.starts_with("/voice/") {
                if let Some(statut) = *self.refus_vocal.lock().unwrap() {
                    return (
                        statut,
                        r#"{"error":"forbidden","message":"Rang insuffisant"}"#.into(),
                    );
                }
            }
            if methode == "POST" && chemin == "/channels/c1/messages" {
                self.requetes
                    .lock()
                    .unwrap()
                    .push(format!("{methode} {chemin}"));
                if let Some(statut) = *self.refus_d_envoi.lock().unwrap() {
                    return (statut, r#"{"error":"refus","message":"refusé"}"#.into());
                }
                let n = self.nombre + 1 + self.envoyes.fetch_add(1, Ordering::SeqCst) as u64;
                let recu: Value = serde_json::from_str(corps).unwrap_or_default();
                let mut m = message_numero(n, 0);
                m["author"] = json!({"id": "u1", "pseudo": "moi"});
                m["content"] = recu["content"].clone();
                m["attachments"] = self.pieces_de(&recu);
                if let Some(r) = recu["reply_to"].as_str() {
                    m["reply_to"] = json!({"id": r, "author": {"id": "carol", "pseudo": "carol"}, "content": "cité"});
                }
                if let Some(options) = recu["poll"]["options"].as_array() {
                    let options: Vec<Value> = options
                        .iter()
                        .enumerate()
                        .map(|(i, o)| json!({"id": format!("o{}", i + 1), "label": o, "votes": 0, "me": false}))
                        .collect();
                    m["poll"] = json!({"multiple": recu["poll"]["multiple"], "closes_at": recu["poll"]["closes_at"], "created_by": {"id": "u1", "pseudo": "moi"}, "options": options});
                }
                return (201, m.to_string());
            }
            if methode == "PATCH" && chemin.starts_with("/admin/reports/") {
                self.requetes
                    .lock()
                    .unwrap()
                    .push(format!("{methode} {chemin}"));
                let recu: Value = serde_json::from_str(corps).unwrap_or_default();
                let id = chemin.trim_start_matches("/admin/reports/");
                let mut s = reponses_signalements()
                    .remove(chemin)
                    .unwrap_or_else(|| json!({"id": id}));
                s["status"] = recu["status"].clone();
                s["duplicate_of"] = recu["duplicate_of"].clone();
                s["notes"] = recu["notes"].clone();
                return (200, s.to_string());
            }
            if methode == "PATCH" && chemin.starts_with("/messages/") {
                self.requetes
                    .lock()
                    .unwrap()
                    .push(format!("{methode} {chemin}"));
                let n: u64 = chemin.trim_start_matches("/messages/").parse().unwrap_or(0);
                let recu: Value = serde_json::from_str(corps).unwrap_or_default();
                let mut m = message_numero(n, 0);
                m["author"] = json!({"id": "u1", "pseudo": "moi"});
                m["content"] = recu["content"].clone();
                m["edited_at"] = json!("2026-11-19T12:00:00Z");
                if recu.get("file_ids").is_some() {
                    m["attachments"] = self.pieces_de(&recu);
                }
                return (200, m.to_string());
            }
            if chemin.starts_with("/gifs/") {
                return self.repondre_gifs(methode, chemin, corps);
            }
            if methode == "POST" && chemin == "/dms" {
                self.requetes
                    .lock()
                    .unwrap()
                    .push(format!("{methode} {chemin}"));
                let recu: Value = serde_json::from_str(corps).unwrap_or_default();
                let pseudo = recu["pseudo"].as_str().unwrap_or_default();
                let (id, autre) = match pseudo {
                    "carol" => ("d1", "carol"),
                    "dave" => ("d2", "dave"),
                    "Élise" => ("d3", "elise"),
                    "zoe" => {
                        return (
                            404,
                            r#"{"error":"user_not_found","message":"introuvable"}"#.into(),
                        )
                    }
                    _ => ("d9", "nouveau"),
                };
                return (
                    200,
                    json!({"id": id, "other": {"id": autre, "pseudo": pseudo}, "online": false})
                        .to_string(),
                );
            }
            if chemin.starts_with("/search?") {
                return self.repondre_recherche(chemin);
            }
            if let Some(reponse) = self.repondre_espaces(methode, chemin, corps) {
                return reponse;
            }
            self.repondre(methode, chemin)
        }

        fn repondre_espaces(
            &self,
            methode: &str,
            chemin: &str,
            corps: &str,
        ) -> Option<(u16, String)> {
            let recu: Value = serde_json::from_str(corps).unwrap_or_default();
            let noter = || {
                self.requetes
                    .lock()
                    .unwrap()
                    .push(format!("{methode} {chemin}"))
            };
            let nouveaux = || self.serveurs_ajoutes.lock().unwrap().clone();
            if let Some(reponse) = self.repondre_reglages(methode, chemin, &recu) {
                noter();
                return Some(reponse);
            }
            if let Some(reponse) = self.repondre_roles(methode, chemin, &recu) {
                noter();
                return Some(reponse);
            }
            if let Some(reponse) = self.repondre_moderation(methode, chemin, &recu) {
                noter();
                return Some(reponse);
            }
            match (methode, chemin) {
                ("POST", "/servers") => {
                    noter();
                    let serveur = json!({"id": "s9", "name": recu["name"], "owner_id": "u1", "icon_version": 0});
                    self.serveurs_ajoutes.lock().unwrap().push(serveur.clone());
                    Some((201, serveur.to_string()))
                }
                ("POST", "/server-invites/ABCDE-23456/join") => {
                    noter();
                    let serveur = json!({"id": "s8", "name": "Club photo", "owner_id": "eve", "icon_version": 0});
                    self.serveurs_ajoutes.lock().unwrap().push(serveur.clone());
                    Some((200, serveur.to_string()))
                }
                ("POST", c) if c.starts_with("/server-invites/") => {
                    noter();
                    Some((
                        404,
                        r#"{"error":"not_found","message":"introuvable"}"#.into(),
                    ))
                }
                ("GET", "/servers") => {
                    noter();
                    let mut liste: Vec<Value> = serde_json::to_value(&etat().serveurs)
                        .unwrap()
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    liste.extend(nouveaux());
                    Some((200, Value::Array(liste).to_string()))
                }
                ("GET", c) => {
                    let id = c.strip_prefix("/servers/")?;
                    let serveur = nouveaux().into_iter().find(|s| s["id"] == id)?;
                    noter();
                    Some((
                        200,
                        json!({"server": serveur, "channels": [{"id": format!("{id}-c1"), "name": "accueil", "kind": "text", "position": 1}], "members": [], "roles": [], "permissions": ["manage_channels"]}).to_string(),
                    ))
                }
                ("POST", c) if c.starts_with("/servers/") && c.ends_with("/channels") => {
                    noter();
                    if recu["name"] == "refusé" {
                        return Some((
                            400,
                            r#"{"error":"invalid_name","message":"Nom refusé par le serveur"}"#
                                .into(),
                        ));
                    }
                    let mut salon = json!({"id": "c9", "name": recu["name"], "kind": recu["kind"], "position": 9});
                    if let Some(limite) = recu.get("user_limit") {
                        salon["user_limit"] = limite.clone();
                    }
                    Some((201, salon.to_string()))
                }
                _ => None,
            }
        }

        fn repondre_moderation(
            &self,
            methode: &str,
            chemin: &str,
            recu: &Value,
        ) -> Option<(u16, String)> {
            let vide = || Some((204, String::new()));
            if let Some(salon) = chemin
                .strip_prefix("/channels/")
                .and_then(|c| c.strip_suffix("/messages/bulk-delete"))
            {
                let n = recu["last"].as_u64().unwrap_or(0).min(3);
                let ids: Vec<String> = (0..n).map(|i| format!("{salon}-{i}")).collect();
                return Some((200, json!({ "deleted": ids }).to_string()));
            }
            if let Some(requete) = chemin.strip_prefix("/servers/s1/audit-log") {
                let filtre = requete.strip_prefix("?action=").unwrap_or_default();
                let journal = [
                    json!({"at": "2026-11-19T10:20:00Z", "actor": "bob", "actor_id": "u1", "action": "member.ban", "target": "hugo", "reason": "spam"}),
                    json!({"at": "2026-11-19T09:05:00Z", "actor": "bob", "actor_id": "u1", "action": "member.timeout", "target": "frank", "reason": ""}),
                    json!({"at": "2026-11-18T18:40:00Z", "actor": "Ève", "actor_id": "eve", "action": "role.create", "target": "Artistes", "reason": ""}),
                    json!({"at": "2026-11-18T17:00:00Z", "actor": "bob", "actor_id": "u1", "action": "channel.moderation", "target": "annonces", "reason": ""}),
                    json!({"at": "2026-11-17T08:30:00Z", "actor": "Ève", "actor_id": "eve", "action": "message.bulk_delete", "target": "général", "reason": ""}),
                ];
                let liste: Vec<&Value> = journal
                    .iter()
                    .filter(|e| e["action"].as_str().unwrap_or_default().starts_with(filtre))
                    .collect();
                return Some((200, json!(liste).to_string()));
            }
            let mut garde = self.detail_s1.lock().unwrap();
            let detail = garde.as_mut()?;
            if let Some(suite) = chemin.strip_prefix("/servers/s1/members/") {
                let (membre, action) = suite.split_once('/')?;
                let membres = detail["members"].as_array_mut()?;
                match (methode, action) {
                    ("POST", "kick" | "ban") => membres.retain(|m| m["id"] != membre),
                    ("POST", "warn") => {}
                    ("PUT", "timeout") => {
                        let m = membres.iter_mut().find(|m| m["id"] == membre)?;
                        m["timeout_until"] = json!("2099-01-01T00:00:00Z");
                    }
                    ("DELETE", "timeout") => {
                        let m = membres.iter_mut().find(|m| m["id"] == membre)?;
                        m["timeout_until"] = Value::Null;
                    }
                    ("PUT", "nickname") => {
                        let m = membres.iter_mut().find(|m| m["id"] == membre)?;
                        m["nickname"] = recu["nickname"].clone();
                    }
                    _ => return None,
                }
                return vide();
            }
            let suite = chemin.strip_prefix("/servers/s1/channels/")?;
            let (salon, reste) = suite.split_once('/').unwrap_or((suite, ""));
            let salons = detail["channels"].as_array_mut()?;
            match (methode, reste) {
                ("PUT", "moderation") => {
                    let c = salons.iter_mut().find(|c| c["id"] == salon)?;
                    c["slowmode"] = recu["slowmode"].clone();
                    c["locked"] = recu["locked"].clone();
                }
                ("PATCH", "") => {
                    let c = salons.iter_mut().find(|c| c["id"] == salon)?;
                    c["name"] = recu["name"].clone();
                }
                ("DELETE", "") => salons.retain(|c| c["id"] != salon),
                _ => return None,
            }
            vide()
        }

        fn repondre_roles(
            &self,
            methode: &str,
            chemin: &str,
            recu: &Value,
        ) -> Option<(u16, String)> {
            let mut garde = self.detail_s1.lock().unwrap();
            let detail = garde.as_mut()?;
            let vide = || Some((204, String::new()));
            let roles = detail["roles"].as_array_mut()?;
            if let Some(suite) = chemin.strip_prefix("/servers/s1/channels/") {
                let (salon, reste) = suite.split_once("/overrides")?;
                let mut surcharges = self.surcharges.lock().unwrap();
                return match (methode, reste.strip_prefix('/')) {
                    ("GET", None) => {
                        let liste: Vec<Value> = surcharges
                            .iter()
                            .filter(|o| o["channel_id"] == salon)
                            .map(|o| json!({"role_id": o["role_id"], "allow": o["allow"], "deny": o["deny"]}))
                            .collect();
                        Some((200, Value::Array(liste).to_string()))
                    }
                    ("PUT", Some(role)) => {
                        surcharges.retain(|o| !(o["channel_id"] == salon && o["role_id"] == role));
                        let vides = |v: &Value| v.as_array().is_none_or(|l| l.is_empty());
                        if !vides(&recu["allow"]) || !vides(&recu["deny"]) {
                            surcharges.push(json!({"channel_id": salon, "role_id": role, "allow": recu["allow"], "deny": recu["deny"]}));
                        }
                        vide()
                    }
                    _ => None,
                };
            }
            match (methode, chemin) {
                ("GET", "/servers/s1") => Some((200, detail.to_string())),
                ("POST", "/servers/s1/roles") => {
                    for r in roles.iter_mut() {
                        if r["is_default"] != true {
                            r["position"] = json!(r["position"].as_i64().unwrap_or(0) + 1);
                        }
                    }
                    let role = json!({"id": format!("rn{}", roles.len()), "name": recu["name"], "color": recu["color"], "icon": recu["icon"], "hoist": recu["hoist"], "permissions": recu["permissions"], "position": 1, "is_default": false});
                    roles.push(role.clone());
                    Some((201, role.to_string()))
                }
                ("PUT", "/servers/s1/roles-order") => {
                    let ids: Vec<Value> = recu["ids"].as_array().cloned().unwrap_or_default();
                    let n = ids.len() as i64;
                    for r in roles.iter_mut() {
                        if let Some(i) = ids.iter().position(|id| *id == r["id"]) {
                            r["position"] = json!(n - i as i64);
                        }
                    }
                    vide()
                }
                (_, c) if c.starts_with("/servers/s1/roles/") => {
                    let id = &c["/servers/s1/roles/".len()..];
                    if methode == "DELETE" {
                        roles.retain(|r| r["id"] != id);
                        for m in detail["members"].as_array_mut()?.iter_mut() {
                            if let Some(l) = m["roles"].as_array_mut() {
                                l.retain(|r| r != id);
                            }
                        }
                        return vide();
                    }
                    let role = roles.iter_mut().find(|r| r["id"] == id)?;
                    for champ in ["name", "color", "icon", "hoist", "permissions"] {
                        role[champ] = recu[champ].clone();
                    }
                    Some((200, role.to_string()))
                }
                (_, c) if c.starts_with("/servers/s1/members/") => {
                    let (membre, role) = c["/servers/s1/members/".len()..].split_once("/roles/")?;
                    let m = detail["members"]
                        .as_array_mut()?
                        .iter_mut()
                        .find(|m| m["id"] == membre)?;
                    let liste = m["roles"].as_array_mut()?;
                    liste.retain(|r| r != role);
                    if methode == "PUT" {
                        liste.push(json!(role));
                    }
                    vide()
                }
                _ => None,
            }
        }

        fn repondre_reglages(
            &self,
            methode: &str,
            chemin: &str,
            recu: &Value,
        ) -> Option<(u16, String)> {
            let liste = |m: &Mutex<Vec<Value>>| Value::Array(m.lock().unwrap().clone()).to_string();
            let retirer = |m: &Mutex<Vec<Value>>, champ: &str, valeur: &str| {
                m.lock().unwrap().retain(|v| v[champ] != valeur);
                Some((204, String::new()))
            };
            match (methode, chemin) {
                ("GET", "/emojis") => Some((200, liste(&self.emojis))),
                ("GET", "/servers/s1/invites") => Some((200, liste(&self.invitations))),
                ("POST", "/servers/s1/invites") => {
                    let mut invitations = self.invitations.lock().unwrap();
                    let mut invitation = json!({"code": format!("code{}", invitations.len() + 1), "created_by": "bob", "uses": 0});
                    if let Some(m) = recu.get("max_uses") {
                        invitation["max_uses"] = m.clone();
                    }
                    if recu.get("expires_in").is_some() {
                        invitation["expires_at"] = json!("2026-11-19T13:00:00Z");
                    }
                    invitations.push(invitation.clone());
                    Some((201, invitation.to_string()))
                }
                ("DELETE", c) if c.starts_with("/servers/s1/invites/") => retirer(
                    &self.invitations,
                    "code",
                    &c["/servers/s1/invites/".len()..],
                ),
                ("GET", "/servers/s1/bans") => Some((200, liste(&self.bannis))),
                ("DELETE", c) if c.starts_with("/servers/s1/bans/") => {
                    retirer(&self.bannis, "user_id", &c["/servers/s1/bans/".len()..])
                }
                ("POST", c) if c.starts_with("/servers/s1/emojis?name=") => {
                    let nom = &c["/servers/s1/emojis?name=".len()..];
                    let mut emojis = self.emojis.lock().unwrap();
                    if emojis.iter().any(|e| e["name"] == nom) {
                        return Some((
                            409,
                            r#"{"error":"emoji_taken","message":"Ce nom est déjà pris"}"#.into(),
                        ));
                    }
                    let emoji = json!({"id": format!("e{}", emojis.len() + 10), "server_id": "s1", "name": nom, "created_by": "u1"});
                    emojis.push(emoji.clone());
                    Some((201, emoji.to_string()))
                }
                ("PATCH", c) if c.starts_with("/servers/s1/emojis/") => {
                    let id = &c["/servers/s1/emojis/".len()..];
                    for e in self.emojis.lock().unwrap().iter_mut() {
                        if e["id"] == id {
                            e["name"] = recu["name"].clone();
                        }
                    }
                    Some((204, String::new()))
                }
                ("DELETE", c) if c.starts_with("/servers/s1/emojis/") => {
                    retirer(&self.emojis, "id", &c["/servers/s1/emojis/".len()..])
                }
                ("PATCH", c)
                    if c.starts_with("/servers/") && !c["/servers/".len()..].contains('/') =>
                {
                    Some((204, String::new()))
                }
                _ => None,
            }
        }

        fn repondre_recherche(&self, chemin: &str) -> (u16, String) {
            self.requetes.lock().unwrap().push(format!("GET {chemin}"));
            if let Some(statut) = *self.refus_de_recherche.lock().unwrap() {
                return (
                    statut,
                    r#"{"error":"invalid_query","message":"Recherche refusée"}"#.into(),
                );
            }
            let encode = chemin
                .split_once("q=")
                .map_or("", |(_, reste)| reste.split('&').next().unwrap_or_default());
            let mut octets = Vec::new();
            let mut i = 0;
            while i < encode.len() {
                if encode.as_bytes()[i] == b'%' {
                    octets.push(u8::from_str_radix(&encode[i + 1..i + 3], 16).unwrap());
                    i += 3;
                } else {
                    octets.push(encode.as_bytes()[i]);
                    i += 1;
                }
            }
            let terme = String::from_utf8(octets).unwrap().to_lowercase();
            let remplaces = self.messages.lock().unwrap().clone();
            let trouves: Vec<Value> = (1..=self.nombre)
                .rev()
                .map(|n| {
                    remplaces
                        .get(&n)
                        .cloned()
                        .unwrap_or_else(|| message_numero(n, self.nombre))
                })
                .filter(|m| {
                    m["content"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase()
                        .split_whitespace()
                        .any(|mot| mot.contains(&terme))
                })
                .collect();
            let decalage = parametre(chemin, "offset").unwrap_or(0) as usize;
            let page: Vec<Value> = trouves.iter().skip(decalage).take(25).cloned().collect();
            (
                200,
                json!({"messages": page, "has_more": decalage + 25 < trouves.len()}).to_string(),
            )
        }

        fn repondre_gifs(&self, methode: &str, chemin: &str, corps: &str) -> (u16, String) {
            self.requetes
                .lock()
                .unwrap()
                .push(format!("{methode} {chemin}"));
            if let Some(statut) = *self.refus_de_gifs.lock().unwrap() {
                return (
                    statut,
                    r#"{"error":"too_many_requests","message":"doucement"}"#.into(),
                );
            }
            if methode == "POST" {
                let recu: Value = serde_json::from_str(corps).unwrap_or_default();
                let mut pieces = self.pieces.lock().unwrap();
                let id = format!("f{}", pieces.len() + 1);
                let piece = json!({"id": id, "name": "giphy.gif", "mime": "image/gif", "size": 2000, "kind": "file", "url": recu["url"]});
                pieces.insert(id, piece.clone());
                return (201, piece.to_string());
            }
            let terme = chemin
                .split_once("q=")
                .map_or("tendances", |(_, reste)| {
                    reste.split('&').next().unwrap_or_default()
                })
                .to_string();
            let decalage = parametre(chemin, "offset").unwrap_or(0) as usize;
            let gifs: Vec<Value> = (decalage..decalage + 24)
                .map(|n| {
                    let (largeur, hauteur) = [(200, 150), (150, 200), (200, 100), (200, 200)][n % 4];
                    json!({"id": format!("{terme}-{n}"), "title": format!("{terme} {n}"), "preview": format!("{}/giphy/{}.gif", self.adresse, n % 4), "url": format!("https://media1.giphy.com/media/{terme}-{n}/giphy.gif"), "width": largeur, "height": hauteur})
                })
                .collect();
            (200, Value::from(gifs).to_string())
        }

        fn repondre(&self, methode: &str, chemin: &str) -> (u16, String) {
            self.requetes
                .lock()
                .unwrap()
                .push(format!("{methode} {chemin}"));
            if methode == "POST" {
                return (200, "{}".into());
            }
            if methode == "PUT" || methode == "DELETE" {
                return (204, String::new());
            }
            if chemin.ends_with("/reads") || chemin == "/emojis" {
                return (200, "[]".into());
            }
            if chemin.ends_with("/poll/votes") {
                return (
                    200,
                    json!({"o1": [{"id": "carol", "pseudo": "carol"}, {"id": "u1", "pseudo": "moi"}], "o2": [{"id": "dave", "pseudo": "dave"}]}).to_string(),
                );
            }
            if !chemin.starts_with("/channels/c1/messages") {
                return (404, r#"{"error":"introuvable"}"#.into());
            }
            let limite = parametre(chemin, "limit").unwrap_or(50);
            let (debut, fin, encore) =
                match (parametre(chemin, "before"), parametre(chemin, "after")) {
                    (Some(avant), _) => {
                        let mut bloque = self.bloque.lock().unwrap();
                        while *bloque {
                            bloque = self.reveil.wait(bloque).unwrap();
                        }
                        let fin = avant.saturating_sub(1).min(self.nombre);
                        let debut = fin.saturating_sub(limite) + 1;
                        (debut, fin, debut > 1)
                    }
                    (None, Some(apres)) => {
                        let debut = apres + 1;
                        let fin = (apres + limite).min(self.nombre);
                        (debut, fin, fin < self.nombre)
                    }
                    (None, None) => {
                        let debut = self.nombre.saturating_sub(limite) + 1;
                        (debut, self.nombre, debut > 1)
                    }
                };
            let remplaces = self.messages.lock().unwrap().clone();
            let messages: Vec<Value> = (debut..=fin)
                .filter(|n| *n >= 1)
                .map(|n| {
                    remplaces
                        .get(&n)
                        .cloned()
                        .unwrap_or_else(|| message_numero(n, self.nombre))
                })
                .collect();
            (
                200,
                json!({"messages": messages, "has_more": encore}).to_string(),
            )
        }
    }

    pub struct FausseFabrique {
        pub etat: Etat,
        pub sections: Vec<Section>,
        pub ouvertures: AtomicUsize,
        pub signaux: Mutex<Option<mpsc::UnboundedSender<Signal>>>,
        pub store: Mutex<Option<Store>>,
        pub reseau: watch::Sender<EtatReseau>,
        pub jeux: Mutex<Option<watch::Receiver<estompe_natif::jeux::EtatJeux>>>,
        pub source: Arc<SourceFactice>,
        pub profils: Arc<Profils>,
        pub adresse: String,
        pub navigateur: Arc<super::FauxNavigateur>,
        pub medias: Arc<Medias>,
        pub fichiers: Arc<FauxFichiers>,
        pub presse_papiers: Arc<FauxPressePapiers>,
        pub dossier_brouillons: Mutex<Option<std::path::PathBuf>>,
        pub vocal: Mutex<Option<estompe_natif::vocal::OutilsVocaux>>,
        pub ecrans: Mutex<Option<Arc<dyn estompe_plateforme::CaptureEcran>>>,
        pub conversions: Mutex<Option<Arc<estompe_client::video::Conversions>>>,
        pub micro: Arc<FauxMicro>,
        pub sortie: Arc<FausseSortie>,
    }

    type Emplacement<T> = Arc<Mutex<(u64, Option<T>)>>;

    struct GardeSon<T: Send>(Emplacement<T>, u64);

    impl<T: Send> Drop for GardeSon<T> {
        fn drop(&mut self) {
            let mut e = self.0.lock().unwrap();
            if e.0 == self.1 {
                e.1 = None;
            }
        }
    }

    fn installer<T: Send + 'static>(emplacement: &Emplacement<T>, valeur: T) -> FluxSon {
        let mut e = emplacement.lock().unwrap();
        e.0 += 1;
        e.1 = Some(valeur);
        FluxSon::nouveau(GardeSon(emplacement.clone(), e.0))
    }

    #[derive(Default)]
    pub struct FauxMicro {
        recepteur: Emplacement<RecepteurSon>,
        pub refus: Mutex<Option<Erreur>>,
    }

    impl FauxMicro {
        pub fn ouvert(&self) -> bool {
            self.recepteur.lock().unwrap().1.is_some()
        }

        pub fn parler(&self, echantillons: &[f32], taux: u32) -> bool {
            match self.recepteur.lock().unwrap().1.as_mut() {
                Some(recepteur) => {
                    recepteur(echantillons, taux);
                    true
                }
                None => false,
            }
        }
    }

    impl Micro for FauxMicro {
        fn disponible(&self) -> bool {
            true
        }

        fn ecouter(&self, recepteur: RecepteurSon) -> estompe_plateforme::Resultat<FluxSon> {
            if let Some(refus) = self.refus.lock().unwrap().clone() {
                return Err(refus);
            }
            Ok(installer(&self.recepteur, recepteur))
        }
    }

    #[derive(Default)]
    pub struct FausseSortie {
        producteur: Emplacement<ProducteurSon>,
        pub ouvertures: AtomicUsize,
    }

    impl FausseSortie {
        pub fn ouverte(&self) -> bool {
            self.producteur.lock().unwrap().1.is_some()
        }

        pub fn tirer(&self, nombre: usize, taux: u32) -> Option<Vec<f32>> {
            let mut tampon = vec![0f32; nombre];
            (self.producteur.lock().unwrap().1.as_mut()?)(&mut tampon, taux);
            Some(tampon)
        }
    }

    impl SortieSon for FausseSortie {
        fn disponible(&self) -> bool {
            true
        }

        fn jouer(&self, producteur: ProducteurSon) -> estompe_plateforme::Resultat<FluxSon> {
            self.ouvertures.fetch_add(1, Ordering::SeqCst);
            Ok(installer(&self.producteur, producteur))
        }
    }

    #[derive(Default)]
    pub struct FauxFichiers {
        pub dossier: Mutex<Option<std::path::PathBuf>>,
        pub ouverts: Mutex<Vec<std::path::PathBuf>>,
        pub choix: Mutex<Vec<std::path::PathBuf>>,
    }

    #[derive(Default)]
    pub struct FauxPressePapiers {
        pub contenu: Mutex<Option<ContenuPressePapiers>>,
    }

    impl PressePapiers for FauxPressePapiers {
        fn disponible(&self) -> bool {
            self.contenu.lock().unwrap().is_some()
        }

        fn copier_image(&self, _: &estompe_plateforme::Image) -> estompe_plateforme::Resultat<()> {
            Ok(())
        }

        fn lire_pieces(&self) -> estompe_plateforme::Resultat<Option<ContenuPressePapiers>> {
            Ok(self.contenu.lock().unwrap().clone())
        }
    }

    impl Fichiers for FauxFichiers {
        fn disponible(&self) -> bool {
            true
        }

        fn choisir_dossier(
            &self,
            _: Option<&std::path::Path>,
        ) -> estompe_plateforme::Resultat<Option<std::path::PathBuf>> {
            Ok(self.dossier.lock().unwrap().clone())
        }

        fn choisir_fichiers(
            &self,
            _: Option<&std::path::Path>,
        ) -> estompe_plateforme::Resultat<Vec<std::path::PathBuf>> {
            Ok(std::mem::take(&mut *self.choix.lock().unwrap()))
        }

        fn ouvrir(&self, chemin: &std::path::Path) -> estompe_plateforme::Resultat<()> {
            self.ouverts.lock().unwrap().push(chemin.to_path_buf());
            Ok(())
        }
    }

    pub struct SourceMediasFactice;

    impl SourceMedias for SourceMediasFactice {
        fn octets(&self, _: &Source) -> Futur<Result<Vec<u8>, ErreurApi>> {
            Box::pin(async { Err(ErreurApi::DelaiDepasse) })
        }
        fn emojis(&self) -> Futur<Result<Vec<EmojiPersonnalise>, ErreurApi>> {
            Box::pin(async { Err(ErreurApi::DelaiDepasse) })
        }
        fn apercu(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Futur<Result<Option<estompe_client::types::LinkPreview>, ErreurApi>> {
            Box::pin(async { Err(ErreurApi::DelaiDepasse) })
        }
        fn telecharger(
            &self,
            _: &str,
            _: std::path::PathBuf,
            _: Progres,
        ) -> Futur<Result<u64, ErreurApi>> {
            Box::pin(async { Err(ErreurApi::DelaiDepasse) })
        }
    }

    impl FausseFabrique {
        pub fn nouvelle(etat: Etat, sections: Vec<Section>) -> Arc<Self> {
            Self::sur(etat, sections, "http://127.0.0.1:9")
        }

        pub fn sur(etat: Etat, sections: Vec<Section>, adresse: &str) -> Arc<Self> {
            let source = Arc::new(SourceFactice::default());
            let reglages = estompe_client::http::Reglages {
                mandataire_systeme: false,
                ..estompe_client::http::Reglages::default()
            };
            Arc::new(Self {
                medias: Medias::nouveaux(
                    Arc::new(Client::avec_reglages(adresse, reglages).unwrap()),
                    None,
                ),
                fichiers: Arc::new(FauxFichiers::default()),
                presse_papiers: Arc::new(FauxPressePapiers::default()),
                dossier_brouillons: Mutex::new(None),
                vocal: Mutex::new(None),
                ecrans: Mutex::new(None),
                conversions: Mutex::new(None),
                micro: Arc::new(FauxMicro::default()),
                sortie: Arc::new(FausseSortie::default()),
                adresse: adresse.to_string(),
                navigateur: Arc::new(super::FauxNavigateur::default()),
                etat,
                sections,
                ouvertures: AtomicUsize::new(0),
                signaux: Mutex::new(None),
                store: Mutex::new(None),
                reseau: watch::channel(EtatReseau::Reconnexion).0,
                jeux: Mutex::new(None),
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
            let store = store_sur(etat, &self.adresse);
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

        fn navigateur(&self) -> estompe_natif::messagerie::pont::Navigateur {
            self.navigateur.clone()
        }

        fn medias(&self) -> Option<Arc<Medias>> {
            Some(self.medias.clone())
        }

        fn fichiers(&self) -> estompe_natif::messagerie::pont::SystemeFichiers {
            self.fichiers.clone()
        }

        fn presse_papiers(&self) -> estompe_natif::messagerie::pont::Presse {
            self.presse_papiers.clone()
        }

        fn conversions(&self) -> Arc<estompe_client::video::Conversions> {
            self.conversions
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| Arc::new(estompe_client::video::Conversions::aucune()))
        }

        fn micro(&self) -> estompe_natif::messagerie::vocal::MicroPartage {
            self.micro.clone()
        }

        fn sortie_son(&self) -> estompe_natif::messagerie::vocal::SortiePartagee {
            self.sortie.clone()
        }

        fn vocal(&self) -> Option<estompe_natif::vocal::OutilsVocaux> {
            self.vocal.lock().unwrap().clone()
        }

        fn jeux(&self) -> Option<watch::Receiver<estompe_natif::jeux::EtatJeux>> {
            self.jeux.lock().unwrap().clone()
        }

        fn ecrans(&self) -> Option<Arc<dyn estompe_plateforme::CaptureEcran>> {
            self.ecrans.lock().unwrap().clone()
        }

        fn client(&self) -> Option<Client> {
            Some(store_sur(Etat::default(), &self.adresse).client().clone())
        }

        fn brouillons(&self) -> estompe_client::brouillons::Brouillons {
            match self.dossier_brouillons.lock().unwrap().as_deref() {
                Some(dossier) => estompe_client::brouillons::Brouillons::charger(dossier),
                None => estompe_client::brouillons::Brouillons::en_memoire(),
            }
        }
    }
}

pub mod bureau {
    use estompe_plateforme::{EvenementZone, Image, RecepteurZone, Resultat, ZoneNotification};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct EtatZone {
        pub presente: AtomicBool,
        pub retiree: AtomicBool,
        pub icones: Mutex<Vec<Image>>,
        recepteur: Mutex<Option<RecepteurZone>>,
    }

    impl EtatZone {
        pub fn declencher(&self, evenement: EvenementZone) {
            let recepteur = self.recepteur.lock().unwrap();
            (recepteur.as_ref().expect("zone installée"))(evenement);
        }

        pub fn installee(&self) -> bool {
            self.recepteur.lock().unwrap().is_some()
        }
    }

    pub struct FausseZone(pub Arc<EtatZone>);

    impl FausseZone {
        pub fn nouvelle(presente: bool) -> (Box<Self>, Arc<EtatZone>) {
            let etat = Arc::new(EtatZone::default());
            etat.presente.store(presente, Ordering::SeqCst);
            (Box::new(Self(etat.clone())), etat)
        }
    }

    impl ZoneNotification for FausseZone {
        fn disponible(&self) -> bool {
            true
        }
        fn presente(&self) -> bool {
            self.0.presente.load(Ordering::SeqCst)
        }
        fn installer(&self, icone: &Image, _: &str, evenements: RecepteurZone) -> Resultat<()> {
            self.0.icones.lock().unwrap().push(icone.clone());
            *self.0.recepteur.lock().unwrap() = Some(evenements);
            Ok(())
        }
        fn definir_icone(&self, icone: &Image) -> Resultat<()> {
            self.0.icones.lock().unwrap().push(icone.clone());
            Ok(())
        }
        fn retirer(&self) {
            self.0.retiree.store(true, Ordering::SeqCst);
        }
    }
}

pub mod notifications {
    use estompe_plateforme::{
        ActionNotification, Image, Notification, Notifications, Pastille, RawWindowHandle,
        RecepteurActions, Resultat,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct NotificationsFactices {
        pub affichees: Mutex<Vec<Notification>>,
        pub retirees: Mutex<Vec<String>>,
        recepteur: Mutex<Option<RecepteurActions>>,
    }

    impl NotificationsFactices {
        pub fn declencher(&self, etiquette: &str, action: ActionNotification) {
            let recepteur = self.recepteur.lock().unwrap();
            (recepteur.as_ref().expect("actions écoutées"))(etiquette.to_string(), action);
        }

        pub fn nombre(&self) -> usize {
            self.affichees.lock().unwrap().len()
        }
    }

    impl Notifications for NotificationsFactices {
        fn disponible(&self) -> bool {
            true
        }
        fn accepte_les_actions(&self) -> bool {
            true
        }
        fn accepte_la_reponse(&self) -> bool {
            true
        }
        fn ecouter(&self, actions: RecepteurActions) {
            *self.recepteur.lock().unwrap() = Some(actions);
        }
        fn notifier(&self, notification: &Notification) -> Resultat<()> {
            self.affichees.lock().unwrap().push(notification.clone());
            Ok(())
        }
        fn retirer(&self, etiquette: &str) -> Resultat<()> {
            self.retirees.lock().unwrap().push(etiquette.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct EtatPastille {
        pub definitions: Mutex<Vec<(Option<Image>, String)>>,
    }

    impl EtatPastille {
        pub fn derniere(&self) -> Option<(Option<Image>, String)> {
            self.definitions.lock().unwrap().last().cloned()
        }
    }

    pub struct PastilleFactice(pub Arc<EtatPastille>);

    impl PastilleFactice {
        pub fn nouvelle() -> (Box<Self>, Arc<EtatPastille>) {
            let etat = Arc::new(EtatPastille::default());
            (Box::new(Self(etat.clone())), etat)
        }
    }

    impl Pastille for PastilleFactice {
        fn disponible(&self) -> bool {
            true
        }
        fn attacher(&self, _: RawWindowHandle) -> Resultat<()> {
            Ok(())
        }
        fn definir(&self, image: Option<&Image>, description: &str) -> Resultat<()> {
            self.0
                .definitions
                .lock()
                .unwrap()
                .push((image.cloned(), description.to_string()));
            Ok(())
        }
    }
}

pub mod references {
    use std::path::{Path, PathBuf};

    pub const ECART_PAR_CANAL: u8 = 24;
    pub const PART_TOLEREE: f64 = 0.001;
    pub const VARIABLE: &str = "ESTOMPE_CAPTURES";

    pub fn dossier() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/references")
    }

    pub fn sorties() -> PathBuf {
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("captures")
    }

    pub fn comparer(
        obtenu: &image::RgbaImage,
        reference: &image::RgbaImage,
    ) -> (f64, image::RgbaImage) {
        let mut difference = image::RgbaImage::new(obtenu.width(), obtenu.height());
        if obtenu.dimensions() != reference.dimensions() {
            return (1.0, difference);
        }
        let mut ecarts = 0u64;
        for ((o, r), d) in obtenu
            .pixels()
            .zip(reference.pixels())
            .zip(difference.pixels_mut())
        {
            let ecart =
                o.0.iter()
                    .zip(r.0)
                    .any(|(a, b)| a.abs_diff(b) > ECART_PAR_CANAL);
            let gris = ((o.0[0] as u32 + o.0[1] as u32 + o.0[2] as u32) / 3 / 4 + 190) as u8;
            *d = if ecart {
                ecarts += 1;
                image::Rgba([220, 0, 60, 255])
            } else {
                image::Rgba([gris, gris, gris, 255])
            };
        }
        let total = u64::from(obtenu.width()) * u64::from(obtenu.height());
        (ecarts as f64 / total as f64, difference)
    }

    pub fn controler(nom: &str, obtenu: &image::RgbaImage, regenerer: bool) -> Result<(), String> {
        let chemin = dossier().join(format!("{nom}.png"));
        if regenerer {
            std::fs::create_dir_all(dossier()).expect("dossier des références");
            obtenu.save(&chemin).expect("écriture de la référence");
            eprintln!("référence écrite : {}", chemin.display());
            return Ok(());
        }
        let Ok(reference) = image::open(&chemin).map(|i| i.into_rgba8()) else {
            let sorties = sorties();
            std::fs::create_dir_all(&sorties).expect("dossier des sorties");
            obtenu
                .save(sorties.join(format!("{nom}.obtenu.png")))
                .expect("écriture de la capture");
            return Err(format!(
                "{nom} : référence absente ({}), capture dans {}",
                chemin.display(),
                sorties.display()
            ));
        };
        let (part, difference) = comparer(obtenu, &reference);
        if part <= PART_TOLEREE {
            return Ok(());
        }
        let sorties = sorties();
        std::fs::create_dir_all(&sorties).expect("dossier des sorties");
        obtenu
            .save(sorties.join(format!("{nom}.obtenu.png")))
            .expect("écriture de la capture");
        reference
            .save(sorties.join(format!("{nom}.reference.png")))
            .expect("copie de la référence");
        difference
            .save(sorties.join(format!("{nom}.difference.png")))
            .expect("écriture de la différence");
        Err(format!(
            "{nom} : {:.3} % de pixels différents (seuil {:.3} %), images dans {}",
            part * 100.0,
            PART_TOLEREE * 100.0,
            sorties.display()
        ))
    }
}
