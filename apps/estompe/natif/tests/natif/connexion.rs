use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

use commun::{attendre, FausseApi, FausseAuth, FauxNavigateur};
use estompe_client::auth::{ErreurAuth, Parcours};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, ecrans, Page, Principale};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::PointerEventButton;
use slint::ComponentHandle;

fn sur_l_interface<R: Send + 'static>(
    faible: &slint::Weak<Principale>,
    action: impl FnOnce(&Principale) -> R + Send + 'static,
) -> R {
    let (envoi, reception) = mpsc::channel();
    faible
        .upgrade_in_event_loop(move |f| {
            let _ = envoi.send(action(&f));
        })
        .expect("boucle d'évènements");
    reception
        .recv_timeout(commun::DELAI)
        .expect("réponse de l'interface")
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        let bouton = ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("bouton « {libelle} » introuvable"));
        assert_eq!(bouton.accessible_enabled(), Some(true), "{libelle}");
        bouton.mock_single_click(PointerEventButton::Left);
    });
}

fn attendre_l_interface(
    faible: &slint::Weak<Principale>,
    condition: impl Fn(&Principale) -> bool + Send + Sync + Clone + 'static,
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

fn scenario(faible: slint::Weak<Principale>, auth: Arc<FausseAuth>, api: Arc<FausseApi>) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Connexion && !f.get_connexion_en_attente(),
        "écran de connexion après la reprise sans session",
    );
    assert_eq!(auth.reprises.load(Ordering::SeqCst), 1);

    cliquer(&faible, "Se connecter");
    attendre_l_interface(
        &faible,
        |f| f.get_connexion_en_attente(),
        "attente du navigateur",
    );
    attendre(|| auth.en_attente(), "connexion lancée");
    assert_eq!(*auth.connexions.lock().unwrap(), [Parcours::Connexion]);

    cliquer(&faible, "Annuler");
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Connexion && !f.get_connexion_en_attente(),
        "retour à la connexion après l'annulation",
    );
    assert_eq!(auth.annulations.load(Ordering::SeqCst), 1);
    assert_eq!(sur_l_interface(&faible, |f| f.get_connexion_erreur()), "");

    cliquer(&faible, "Se connecter");
    attendre(|| auth.en_attente(), "deuxième connexion lancée");
    auth.terminer(Err(ErreurAuth::DelaiDepasse));
    attendre_l_interface(
        &faible,
        |f| !f.get_connexion_en_attente() && f.get_connexion_erreur().contains("à temps"),
        "erreur de délai affichée",
    );

    cliquer(&faible, "Se connecter");
    attendre_l_interface(
        &faible,
        |f| f.get_connexion_en_attente() && f.get_connexion_erreur().is_empty(),
        "nouvel essai : l'erreur disparaît",
    );
    attendre(|| auth.en_attente(), "troisième connexion lancée");
    assert_eq!(auth.connexions(), 3);
    assert_eq!(api.appels.load(Ordering::SeqCst), 0);
    auth.terminer(Ok(()));
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.get_pseudo() == "bob"
                && f.global::<estompe_natif::DonneesCoquille>().get_pret()
        },
        "arrivée sur la coquille",
    );
    assert_eq!(api.appels.load(Ordering::SeqCst), 1, "un seul /me");

    cliquer(&faible, "Créer ou rejoindre un serveur");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<estompe_natif::DonneesCreerRejoindre>()
                .get_ouvert()
        },
        "fenêtre « Créer ou rejoindre un serveur »",
    );
    sur_l_interface(&faible, |f| {
        f.global::<estompe_natif::DonneesCreerRejoindre>()
            .invoke_fermer();
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::ZERO);
    });
    attendre_l_interface(
        &faible,
        |f| {
            !f.global::<estompe_natif::DonneesCreerRejoindre>()
                .get_ouvert()
        },
        "fenêtre refermée",
    );
    sur_l_interface(&faible, |f| f.invoke_ouvrir_invitation());
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Invitation,
        "écran d'invitation",
    );
    cliquer(&faible, "Retour à l'accueil");
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "retour à la coquille",
    );

    cliquer(&faible, "Se déconnecter");
    attendre_l_interface(&faible, |f| f.get_page() == Page::Connexion, "déconnexion");
    attendre(
        || auth.deconnexions.load(Ordering::SeqCst) == 1,
        "déconnexion demandée",
    );
}

#[test]
fn c2_connexion_annulation_erreur_puis_arrivee_sur_la_coquille() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let auth = FausseAuth::avec_session(false);
    let api = FausseApi::compte_actif();
    let navigateur = Arc::new(FauxNavigateur::default());
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(auth.clone(), api.clone(), navigateur),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let dossier = tempfile::tempdir().unwrap();
    let coquille = Coquille::brancher(
        &fenetre,
        &entree,
        commun::coquille::FausseFabrique::nouvelle(commun::coquille::etat(), Vec::new()),
        Memoire::nouvelle(Default::default(), dossier.path().to_path_buf(), None),
        estompe_natif::coquille::horloge_locale(),
    );
    fenetre.show().unwrap();

    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, auth, api)));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    drop(coquille);
    drop(entree);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
