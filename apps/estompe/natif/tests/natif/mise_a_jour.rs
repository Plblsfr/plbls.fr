use crate::commun;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use commun::{attendre, FausseAuth, FauxNavigateur, DELAI};
use estompe_client::http::Reglages;
use estompe_natif::entree::{self, Entree, Etat, Evenement, Machine, PLATEFORME};

#[derive(Default)]
struct Journal {
    requetes: Mutex<Vec<Vec<String>>>,
}

fn serveur_qui_refuse_la_version(journal: Arc<Journal>) -> String {
    let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
    let adresse = format!("http://{}", ecoute.local_addr().unwrap());
    std::thread::spawn(move || {
        for flux in ecoute.incoming() {
            let Ok(mut flux) = flux else { return };
            let mut lignes = Vec::new();
            let mut lecteur = BufReader::new(flux.try_clone().unwrap());
            loop {
                let mut ligne = String::new();
                if lecteur.read_line(&mut ligne).unwrap_or(0) == 0 || ligne == "\r\n" {
                    break;
                }
                lignes.push(ligne.trim_end().to_string());
            }
            journal.requetes.lock().unwrap().push(lignes);
            let corps = r#"{"error":"upgrade_required","message":"Une faille a été corrigée.","min_version":"9.9.9"}"#;
            let _ = write!(
                flux,
                "HTTP/1.1 426 Upgrade Required\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{corps}",
                corps.len()
            );
        }
    });
    adresse
}

fn entete<'a>(requete: &'a [String], nom: &str) -> Option<&'a str> {
    requete.iter().find_map(|ligne| {
        let (cle, valeur) = ligne.split_once(':')?;
        cle.eq_ignore_ascii_case(nom).then(|| valeur.trim())
    })
}

#[test]
fn c2_version_trop_ancienne_ecran_bloquant_et_plus_aucune_requete() {
    let journal = Arc::new(Journal::default());
    let adresse = serveur_qui_refuse_la_version(journal.clone());
    let canal = entree::canal();
    let reglages = Reglages {
        mandataire_systeme: false,
        ..Reglages::default()
    };
    let client = entree::client_de_l_app(&adresse, reglages, &canal.0).unwrap();
    let auth = FausseAuth::avec_session(true);
    let navigateur = Arc::new(FauxNavigateur::default());
    let entree = Entree::demarrer(
        Machine::nouvelle(Some("ABCDE-23456".into())),
        commun::services(auth.clone(), Arc::new(client), navigateur.clone()),
        canal,
        Arc::new(|_: &Etat| {}),
    )
    .unwrap();

    let etat = entree
        .attendre(
            |e| matches!(e, Etat::MiseAJourRequise { version_minimale, .. } if !version_minimale.is_empty()),
            DELAI,
        )
        .expect("écran de mise à jour");
    assert_eq!(
        etat,
        Etat::MiseAJourRequise {
            message: "Une faille a été corrigée.".into(),
            version_minimale: "9.9.9".into(),
        }
    );
    {
        let requetes = journal.requetes.lock().unwrap();
        assert_eq!(requetes.len(), 1);
        assert_eq!(requetes[0][0], "GET /me HTTP/1.1");
        assert_eq!(entete(&requetes[0], "x-estompe-platform"), Some(PLATEFORME));
        assert_eq!(
            entete(&requetes[0], "x-estompe-version"),
            Some(estompe_natif::VERSION)
        );
    }

    for evenement in [
        Evenement::Reessayer,
        Evenement::Lancer,
        Evenement::SeConnecter,
        Evenement::Activer("bob".into()),
        Evenement::OuvrirInvitation,
        Evenement::Rejoindre("ABCDE-23456".into()),
        Evenement::SeDeconnecter,
        Evenement::RepriseTerminee(Ok(true)),
    ] {
        entree.envoyer(evenement);
    }
    entree.envoyer(Evenement::Telecharger);
    attendre(
        || !navigateur.ouvertes.lock().unwrap().is_empty(),
        "lien de téléchargement ouvert",
    );
    assert_eq!(
        *navigateur.ouvertes.lock().unwrap(),
        ["https://chat.example/"]
    );
    assert_eq!(entree.etat(), etat);
    assert_eq!(
        journal.requetes.lock().unwrap().len(),
        1,
        "aucune autre requête"
    );
    assert_eq!(auth.connexions(), 0);
    assert_eq!(
        auth.deconnexions.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(auth.reprises.load(std::sync::atomic::Ordering::SeqCst), 1);
}
