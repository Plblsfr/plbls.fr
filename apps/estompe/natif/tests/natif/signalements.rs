use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use commun::coquille::{
    etat_administrateur, servir_les_signalements, FausseFabrique, ServeurMessages,
};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::preferences::Preferences;
use estompe_client::signalements::FICHIER_BROUILLON;
use estompe_natif::coquille::pont::TITRE_FENETRE;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, menu, DonneesAdmin, DonneesCoquille, DonneesSignalements,
    Principale,
};
use estompe_plateforme::{EcransFactices, SourceCapture, TypeSource};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};

fn source(
    id: &str,
    nom: &str,
    genre: TypeSource,
    taille: (u32, u32),
    estompe: bool,
) -> SourceCapture {
    SourceCapture {
        id: id.into(),
        nom: nom.into(),
        application: String::new(),
        genre,
        largeur: taille.0,
        hauteur: taille.1,
        principale: genre == TypeSource::Ecran,
        processus: None,
        estompe,
    }
}

fn corps(serveur: &ServeurMessages, ligne: &str) -> Vec<Value> {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .filter(|(r, _)| r == ligne)
        .map(|(_, c)| serde_json::from_str(c).unwrap_or_default())
        .collect()
}

fn infos(f: &Principale) -> Vec<(String, String, bool)> {
    f.global::<DonneesSignalements>()
        .get_informations()
        .iter()
        .map(|i| (i.cle.to_string(), i.valeur.to_string(), i.incluse))
        .collect()
}

fn avancer(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |_| {
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200))
    });
}

fn fenetre_prete(faible: &slint::Weak<Principale>, captures: usize, quoi: &str) {
    avancer(faible);
    attendre_l_interface(
        faible,
        move |f| {
            let g = f.global::<DonneesSignalements>();
            g.get_ouverte() && !g.get_masquee() && g.get_captures().row_count() == captures
        },
        quoi,
    );
}

fn signaler(faible: &slint::Weak<Principale>, serveur: &ServeurMessages, brouillon: PathBuf) {
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesCoquille>().get_pret(),
        "coquille prête",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>().invoke_signaler_bug()
    });
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert!(
            g.get_ouverte() && g.get_masquee(),
            "la fenêtre attend la capture avant de s'afficher"
        );
    });
    fenetre_prete(faible, 1, "capture de la fenêtre jointe à l'ouverture");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert_eq!(g.get_titre_fenetre(), "Signaler un bug");
        assert!(g.get_brouillon() && g.get_capture_disponible() && !g.get_envoyable());
        let i = infos(f);
        assert_eq!(i[0], ("Plateforme".into(), "desktop".into(), true));
        assert!(
            i.iter().any(|(k, v, _)| k == "Écran" && v == "accueil"),
            "{i:?}"
        );
        assert!(i.iter().any(|(k, v, _)| k == "Vocal" && v == "inactif"));
        g.set_titre("Le vocal coupe".into());
        g.set_description("après 5 min".into());
        g.invoke_choisir_categorie("vocal".into());
        g.invoke_basculer_information(i.iter().position(|(k, _, _)| k == "Vocal").unwrap() as i32);
        assert!(g.get_envoyable());
        assert_eq!(g.get_informations_incluses() as usize, i.len() - 1);
    });
    let garde: Value = serde_json::from_str(&std::fs::read_to_string(&brouillon).unwrap()).unwrap();
    assert_eq!(
        garde,
        json!({"title": "Le vocal coupe", "description": "après 5 min", "category": "vocal"})
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>().invoke_fermer()
    });
    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>().invoke_signaler_bug()
    });
    fenetre_prete(faible, 1, "brouillon repris à la réouverture");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert_eq!(
            (g.get_titre().as_str(), g.get_categorie().as_str()),
            ("Le vocal coupe", "vocal")
        );
        g.invoke_capturer();
        assert!(
            g.get_masquee(),
            "le formulaire disparaît le temps de la capture"
        );
    });
    fenetre_prete(faible, 2, "seconde capture");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        g.invoke_retirer_capture(1);
        assert_eq!(g.get_captures().row_count(), 1);
        let vocal = infos(f).iter().position(|(k, _, _)| k == "Vocal").unwrap();
        assert!(
            infos(f)[vocal].2,
            "les informations repartent cochées à la réouverture"
        );
        g.invoke_basculer_information(vocal as i32);
        g.invoke_envoyer();
    });
    attendre(
        || !corps(serveur, "POST /reports").is_empty(),
        "signalement envoyé",
    );
    let envoye = corps(serveur, "POST /reports").remove(0);
    assert_eq!(envoye["title"], "Le vocal coupe");
    assert_eq!(envoye["category"], "vocal");
    assert_eq!(envoye["platform"], "desktop");
    assert_eq!(envoye["file_ids"], json!(["f1"]));
    assert_eq!(envoye["logs"], "");
    assert!(envoye["tech"].get("Vocal").is_none() && envoye["tech"]["Version"].is_string());
    let capture = serveur.contenus.lock().unwrap().get("f1").cloned().unwrap();
    let image = image::load_from_memory(&capture).unwrap();
    assert_eq!((image.width(), image.height()), (1180, 900));
    assert_eq!(
        image::guess_format(&capture).unwrap(),
        image::ImageFormat::Jpeg
    );
    println!("367 C2 : seule la fenêtre d'Estompe est capturée (1180 × 900, ni l'écran ni l'autre fenêtre)");
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesSignalements>().get_ouverte(),
        "fenêtre refermée après l'envoi",
    );
    assert!(!brouillon.exists(), "brouillon vidé après l'envoi");
    sur_l_interface(faible, |f| {
        assert_eq!(
            f.global::<DonneesSignalements>().get_info_mes(),
            "Signalement envoyé, merci !"
        );
    });
    println!(
        "367 : bug signalé avec capture, informations décochables et brouillon gardé puis vidé"
    );

    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>().invoke_signaler_message(
            "5".into(),
            "carol".into(),
            "un message déplacé".into(),
        )
    });
    fenetre_prete(faible, 1, "fenêtre d'un message signalé");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert_eq!(g.get_titre_fenetre(), "Signaler un message");
        assert_eq!(g.get_titre(), "Message de carol");
        assert_eq!(g.get_categorie(), "messages");
        assert!(!g.get_brouillon());
        assert_eq!(infos(f)[0], ("Message".into(), "5".into(), true));
        g.invoke_retirer_capture(0);
        g.set_description("Insulte envers un membre".into());
        g.invoke_modifie();
        g.invoke_envoyer();
    });
    attendre(
        || corps(serveur, "POST /reports").len() == 2,
        "message signalé",
    );
    let message = corps(serveur, "POST /reports").remove(1);
    assert_eq!(message["file_ids"], json!([]));
    assert_eq!(message["tech"]["Extrait"], "un message déplacé");
    assert!(
        !brouillon.exists(),
        "un message signalé ne touche pas au brouillon de bug"
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>()
            .invoke_signaler_personne("u2".into(), "alice".into())
    });
    fenetre_prete(faible, 1, "fenêtre d'une personne signalée");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert_eq!(g.get_titre_fenetre(), "Signaler une personne");
        assert_eq!(infos(f)[0], ("Personne".into(), "alice (u2)".into(), true));
        g.invoke_fermer();
    });
    println!("367 : message et personne signalés avec leur contexte");

    sur_l_interface(faible, |f| {
        f.global::<DonneesSignalements>().invoke_charger_mes()
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSignalements>().get_mes().row_count() == 2,
        "mes signalements",
    );
    sur_l_interface(faible, |f| {
        let r = f
            .global::<DonneesSignalements>()
            .get_mes()
            .row_data(1)
            .unwrap();
        assert_eq!(
            (r.statut.as_str(), r.ton.as_str(), r.resume.as_str()),
            ("Résolu", "ok", "02/11/2026")
        );
    });
    println!("367 : mes signalements et leur statut");

    sur_l_interface(faible, |f| {
        let a = f.global::<DonneesAdmin>();
        a.invoke_ouvrir();
        a.invoke_choisir("signalements".into());
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSignalements>().get_liste().row_count() == 3,
        "signalements de l'administration",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        let r = g.get_liste().row_data(0).unwrap();
        assert!(r.nouveau && r.plateforme == "Application PC" && r.ton == "info");
        g.invoke_choisir_filtre("statut".into(), 2);
        g.set_filtre_auteur("ali ce".into());
        g.invoke_filtrer();
        g.invoke_ouvrir_signalement(0);
    });
    attendre(
        || {
            serveur
                .requetes
                .lock()
                .unwrap()
                .iter()
                .any(|r| r == "GET /admin/reports?status=in_progress&author=ali%20ce")
        },
        "filtres envoyés",
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSignalements>();
            g.get_detail_ouvert() && g.get_detail_captures().row_count() == 1
        },
        "détail avec sa capture",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert_eq!(g.get_detail_tech().row_count(), 5);
        assert!(g.get_detail_logs().contains("reconnexion"));
        assert!(g.get_liste().row_data(0).unwrap().actif);
        g.invoke_choisir_statut("duplicate".into());
        g.invoke_enregistrer();
        assert_eq!(g.get_erreur_admin(), "Choisis le signalement d'origine.");
        g.invoke_ouvrir_origines(10.0, 10.0, 30.0);
        menu::choisir(f, "r3");
        assert_eq!(g.get_origine(), "Partage d'écran noir (bob)");
        g.set_notes("Même cause que r3".into());
        g.invoke_enregistrer();
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSignalements>().get_info_admin() == "Signalement mis à jour.",
        "traitement enregistré",
    );
    assert_eq!(
        corps(serveur, "PATCH /admin/reports/r1"),
        [json!({"status": "duplicate", "duplicate_of": "r3", "notes": "Même cause que r3"})]
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesSignalements>();
        assert!(g.get_a_original());
        g.invoke_copier();
        assert_eq!(g.get_info_admin(), "Signalement copié.");
        g.invoke_supprimer();
    });
    attendre_l_interface(
        faible,
        |_| confirmation::ouverte().is_some(),
        "confirmation de suppression",
    );
    sur_l_interface(faible, |f| {
        assert_eq!(
            confirmation::ouverte().unwrap().message,
            "Supprimer définitivement le signalement « Le vocal coupe après cinq minutes » ?"
        );
        confirmation::repondre(f, true)
    });
    attendre(
        || {
            serveur
                .requetes
                .lock()
                .unwrap()
                .iter()
                .any(|r| r == "DELETE /admin/reports/r1")
        },
        "signalement supprimé",
    );
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesSignalements>().get_detail_ouvert(),
        "détail refermé après la suppression",
    );
    println!("367 : traitement par les administrateurs (filtres, détail, doublon, notes, copie, suppression)");
}

#[test]
fn signaler_et_traiter() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(20);
    servir_les_signalements(&serveur);
    let dossier = tempfile::tempdir().unwrap();
    let brouillon = dossier.path().join(FICHIER_BROUILLON);
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None);
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let auth = FausseAuth::avec_session(true);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(
            auth,
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let fabrique = FausseFabrique::sur(etat_administrateur(), Vec::new(), &serveur.adresse);
    let captures = EcransFactices::nouveaux();
    captures.brancher(source(
        "ecran:0",
        "Écran 1",
        TypeSource::Ecran,
        (2560, 1440),
        false,
    ));
    captures.brancher(source(
        "fenetre:1",
        "Navigateur",
        TypeSource::Fenetre,
        (1920, 1080),
        false,
    ));
    captures.brancher(source(
        "fenetre:2",
        TITRE_FENETRE,
        TypeSource::Fenetre,
        (1180, 900),
        true,
    ));
    *fabrique.ecrans.lock().unwrap() = Some(Arc::new(captures));
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Arc::new(Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire,
        Arc::new(move || midi),
    ));
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| signaler(&faible, &serveur, brouillon)));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    drop(coquille);
    drop(entree);
    drop(fabrique);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
