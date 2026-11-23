mod commun;

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use commun::attendre;
use commun::coquille::{store_sur, ServeurMessages, SourceFactice};
use estompe_client::niveaux_notification::{sourdine, DureeSourdine};
use estompe_client::route::Route;
use estompe_client::types::NotifySettingsGeneral;
use estompe_natif::coquille::{Navigation, Pont, Profils};
use estompe_natif::menu::{self, Ancre};
use estompe_natif::DonneesMenu;

use estompe_client::journal::{Journal, Niveau};
use estompe_client::preferences::{self, Preferences};
use estompe_natif::parametres::{brancher, OutilsParametres};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, DonneesCoquille, DonneesParametres, ModeTheme, Mouvement, Page, Principale,
    Theme,
};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

const MAINTENANT_MS: i64 = 1_795_077_000_000;

struct Banc {
    f: Principale,
    memoire: Memoire,
    journal: Arc<Journal>,
    dossier: tempfile::TempDir,
}

fn rendre(f: &Principale) {
    let _ = f.window().take_snapshot();
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
}

fn touche(f: &Principale, texte: impl Into<SharedString>) {
    let text: SharedString = texte.into();
    f.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    f.window().dispatch_event(WindowEvent::KeyReleased { text });
    rendre(f);
}

fn touches(f: &Principale, texte: impl Into<SharedString> + Clone, nombre: usize) {
    for _ in 0..nombre {
        touche(f, texte.clone());
    }
}

fn controle(f: &Principale, lettre: &str) {
    let ctrl: SharedString = Key::Control.into();
    f.window()
        .dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    touche(f, lettre.to_string());
    f.window()
        .dispatch_event(WindowEvent::KeyReleased { text: ctrl });
    rendre(f);
}

fn ecrire(f: &Principale, texte: &str) {
    for c in texte.chars() {
        touche(f, c.to_string());
    }
}

fn banc() -> Banc {
    i_slint_backend_testing::init_no_event_loop();
    let dossier = tempfile::tempdir().unwrap();
    let journal = Arc::new(Journal::ouvrir(&dossier.path().join("journaux")));
    let memoire = Memoire::nouvelle(Preferences::default(), dossier.path().join("donnees"), None);
    let f = creer_fenetre().unwrap();
    f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
    f.set_page(Page::Coquille);
    f.global::<DonneesCoquille>().set_pret(true);
    f.global::<DonneesCoquille>().set_pseudo("bob".into());
    f.show().unwrap();
    f.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    brancher(
        &f,
        OutilsParametres {
            memoire: memoire.clone(),
            journal: journal.clone(),
            fuseau: Arc::new(|_| 60),
            horloge: Arc::new(|| MAINTENANT_MS),
            dossier_export: dossier.path().join("exports"),
        },
    );
    rendre(&f);
    ouvrir(&f);
    Banc {
        f,
        memoire,
        journal,
        dossier,
    }
}

fn ouvrir(f: &Principale) {
    f.global::<DonneesParametres>().invoke_ouvrir("".into());
    rendre(f);
}

fn onglet(f: &Principale) -> String {
    f.global::<DonneesParametres>().get_onglet().to_string()
}

fn onglets(f: &Principale) -> Vec<String> {
    f.global::<DonneesParametres>()
        .get_onglets()
        .iter()
        .map(|o| o.id.to_string())
        .collect()
}

#[test]
fn les_onglets_suivent_l_ordre_du_front_et_se_parcourent_aux_fleches() {
    let b = banc();
    let f = &b.f;
    assert!(f.global::<DonneesParametres>().get_ouverte());
    assert_eq!(
        onglets(f),
        [
            "compte",
            "notifications",
            "voix",
            "apparence",
            "application",
            "debogage",
            "signalements",
            "raccourcis"
        ],
        "même liste et même ordre que le front, puis les onglets propres à l'app ; l'instant replay n'apparaît que disponible"
    );
    assert_eq!(onglet(f), "compte");
    touche(f, Key::DownArrow);
    assert_eq!(onglet(f), "notifications");
    touche(f, Key::UpArrow);
    touche(f, Key::UpArrow);
    assert_eq!(onglet(f), "raccourcis", "les flèches bouclent");
    touche(f, Key::Home);
    assert_eq!(onglet(f), "compte");
    touche(f, Key::End);
    assert_eq!(onglet(f), "raccourcis");
    assert_eq!(f.global::<DonneesParametres>().get_titre(), "Raccourcis");
    assert_eq!(
        b.memoire.lire(|p| p.onglet_parametres.clone()),
        "raccourcis"
    );
    assert!(f.global::<DonneesParametres>().get_raccourcis().row_count() >= 8);
    let globales: Vec<String> = f
        .global::<DonneesParametres>()
        .get_touches_globales()
        .iter()
        .map(|r| format!("{} {}", r.action, r.touches))
        .collect();
    assert_eq!(globales, ["Appuyer pour parler désactivé"]);
}

#[test]
fn le_theme_et_les_animations_se_reglent_au_clavier_et_echap_ferme() {
    let b = banc();
    let f = &b.f;
    touches(f, Key::DownArrow, 3);
    assert_eq!(onglet(f), "apparence");
    touches(f, Key::Tab, 4 + 1 + 3);
    touche(f, Key::Return);
    assert_eq!(f.global::<Theme>().get_mode(), ModeTheme::Sombre);
    assert_eq!(b.memoire.lire(|p| p.theme), preferences::Theme::Sombre);
    touche(f, Key::Backtab);
    touche(f, " ");
    assert_eq!(f.global::<Theme>().get_mode(), ModeTheme::Clair);
    touches(f, Key::Tab, 3);
    touche(f, " ");
    assert_eq!(
        b.memoire.lire(|p| p.densite),
        preferences::Densite::Compacte
    );
    assert!(f.global::<estompe_natif::Affichage>().get_compacte());
    touches(f, Key::Tab, 2);
    touche(f, Key::Return);
    assert!(f.global::<Mouvement>().get_reduit());
    assert!(b.memoire.lire(|p| p.reduire_animations));
    let demandes = f.global::<DonneesCoquille>().get_demande_focus();
    touche(f, Key::Escape);
    assert!(!f.global::<DonneesParametres>().get_ouverte());
    assert_eq!(
        f.global::<DonneesCoquille>().get_demande_focus(),
        demandes + 1,
        "le focus revient à la coquille"
    );
    ouvrir(f);
    assert_eq!(onglet(f), "apparence", "le dernier onglet est rouvert");
}

#[test]
fn la_recherche_filtre_les_onglets_et_echap_l_efface_avant_de_fermer() {
    let b = banc();
    let f = &b.f;
    controle(f, "f");
    ecrire(f, "Theme");
    assert_eq!(onglets(f), ["apparence"]);
    assert_eq!(
        f.global::<DonneesParametres>()
            .get_onglets()
            .row_data(0)
            .unwrap()
            .trouves,
        "Thème"
    );
    touche(f, Key::Escape);
    assert!(f.global::<DonneesParametres>().get_ouverte());
    assert_eq!(onglets(f).len(), 8);
    touche(f, Key::Escape);
    assert!(!f.global::<DonneesParametres>().get_ouverte());
}

#[test]
fn le_journal_se_consulte_s_exporte_et_s_efface_au_clavier() {
    let b = banc();
    let f = &b.f;
    touches(f, Key::DownArrow, 5);
    assert_eq!(onglet(f), "debogage");
    touches(f, Key::Tab, 2 + 1 + 1);
    touche(f, " ");
    assert!(b.journal.est_actif());
    assert!(b.memoire.lire(|p| p.journal_debogage));
    b.journal
        .ecrire(Niveau::Avertissement, "réseau", "reconnexion lente");
    b.journal.ecrire(Niveau::Erreur, "vocal", "micro absent");
    f.global::<DonneesParametres>().invoke_rafraichir_journal();
    rendre(f);
    let g = f.global::<DonneesParametres>();
    assert_eq!(g.get_total_lignes(), 3);
    assert!(g
        .get_lignes()
        .iter()
        .any(|l| l.niveau == "warn" && l.texte.ends_with("[WARN] réseau: reconnexion lente")));
    touches(f, Key::Tab, 1 + 4);
    touche(f, Key::Return);
    assert_eq!(g.get_niveau(), 3);
    assert_eq!(g.get_lignes().row_count(), 1);
    touche(f, Key::Tab);
    ecrire(f, "zz");
    assert_eq!(g.get_lignes().row_count(), 0);
    touche(f, Key::Backtab);
    touches(f, Key::Backtab, 3);
    touche(f, Key::Return);
    assert_eq!(g.get_niveau(), 0);
    touches(f, Key::Tab, 4);
    touche(f, Key::Backspace);
    touche(f, Key::Backspace);
    assert_eq!(g.get_lignes().row_count(), 3);
    touches(f, Key::Tab, 2);
    touche(f, Key::Return);
    let exports: Vec<_> = std::fs::read_dir(b.dossier.path().join("exports"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(exports.len(), 1);
    assert!(exports[0].ends_with("estompe-debug-2026-11-19-09-30-00.log"));
    assert!(std::fs::read_to_string(&exports[0])
        .unwrap()
        .contains("micro absent"));
    assert!(g.get_message_journal().starts_with("Journal exporté"));
    touche(f, Key::Tab);
    touche(f, Key::Return);
    assert_eq!(b.journal.lire(), "");
    assert_eq!(g.get_total_lignes(), 0);
}

#[test]
fn sept_clics_sur_la_version_debloquent_les_fonctions_alpha() {
    let b = banc();
    let f = &b.f;
    let g = f.global::<DonneesParametres>();
    g.invoke_choisir("application".into());
    for _ in 0..6 {
        g.invoke_version_cliquee();
    }
    assert!(!b.memoire.lire(|p| p.alpha));
    g.invoke_version_cliquee();
    assert!(b.memoire.lire(|p| p.alpha));
    assert_eq!(g.get_annonce(), "Fonctions alpha débloquées.");
}

fn pont_de_notifications(f: &Principale, adresse: &str, minutes: Arc<AtomicI64>) -> Pont {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let execution = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    let matin = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap();
    let pont = Pont::nouveau(
        store_sur(commun::coquille::etat(), adresse),
        f.as_weak(),
        execution,
        None,
        Arc::new(move || matin + chrono::Duration::minutes(minutes.load(Ordering::SeqCst))),
        Navigation::nouvelle(Route::Salon {
            serveur: "s1".into(),
            salon: Some("c1".into()),
            message: None,
        }),
        Profils::nouveaux(Arc::new(SourceFactice::default()), None),
    );
    pont.demarrer(f);
    pont
}

fn entrees_du_menu(f: &Principale) -> Vec<(String, String)> {
    f.global::<DonneesMenu>()
        .get_entrees()
        .iter()
        .map(|e| (e.id.to_string(), e.libelle.to_string()))
        .collect()
}

fn ids_du_menu(f: &Principale) -> Vec<String> {
    entrees_du_menu(f).into_iter().map(|(id, _)| id).collect()
}

fn salons_regles(f: &Principale) -> Vec<(String, i32, String, String)> {
    f.global::<DonneesParametres>()
        .get_salons_notif()
        .iter()
        .map(|l| {
            (
                l.nom.to_string(),
                l.rang,
                l.effectif.to_string(),
                l.sourdine.to_string(),
            )
        })
        .collect()
}

fn enregistre(serveur: &ServeurMessages, motif: &str) -> bool {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .any(|(requete, corps)| requete == "PUT /me/notify-settings" && corps.contains(motif))
}

#[test]
fn les_niveaux_et_les_sourdines_s_enregistrent_et_une_sourdine_se_leve_seule() {
    i_slint_backend_testing::init_no_event_loop();
    let serveur = ServeurMessages::demarrer(10);
    let f = creer_fenetre().unwrap();
    f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
    f.show().unwrap();
    let dossier = tempfile::tempdir().unwrap();
    brancher(
        &f,
        OutilsParametres {
            memoire: Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None),
            journal: Arc::new(Journal::ouvrir(&dossier.path().join("journaux"))),
            fuseau: Arc::new(|_| 60),
            horloge: Arc::new(|| MAINTENANT_MS),
            dossier_export: dossier.path().join("exports"),
        },
    );
    let minutes = Arc::new(AtomicI64::new(0));
    let pont = pont_de_notifications(&f, &serveur.adresse, minutes.clone());
    let g = f.global::<DonneesParametres>();
    let reglages = || pont.store().lire(|e| e.reglages_notification.clone());

    assert_eq!(
        (
            g.get_serveur_notif().as_str(),
            g.get_nom_serveur_notif().as_str()
        ),
        ("s1", "Les amis"),
        "le serveur ouvert est réglé d'abord"
    );
    assert_eq!(g.get_niveau_serveur(), 0);
    assert_eq!(
        g.get_effectif_serveur(),
        "Actuellement : mentions seulement (par défaut)"
    );
    assert!(g.get_plusieurs_serveurs() && g.get_autres_salons());
    assert!(salons_regles(&f).is_empty());

    let ancre = Ancre::Point { x: 300.0, y: 300.0 };
    assert!(pont.menu_salon(&f, "c4", ancre));
    assert_eq!(ids_du_menu(&f), ["lu", "sourdine", "lien", "creer-salon"]);
    menu::choisir(&f, "sourdine");
    assert!(
        f.global::<DonneesMenu>().get_ouvert(),
        "le menu des durées s'ouvre à la place"
    );
    assert_eq!(f.global::<DonneesMenu>().get_titre(), "Sourdine : #photos");
    assert_eq!(
        entrees_du_menu(&f)
            .into_iter()
            .map(|(_, l)| l)
            .collect::<Vec<_>>(),
        [
            "Pendant 15 minutes",
            "Pendant 1 heure",
            "Pendant 8 heures",
            "Pendant 24 heures",
            "Jusqu'à réactivation"
        ]
    );
    menu::choisir(&f, DureeSourdine::QuinzeMinutes.id());
    assert!(!f.global::<DonneesMenu>().get_ouvert());
    attendre(
        || enregistre(&serveur, "\"mutes\":{\"c4\":"),
        "sourdine envoyée au serveur",
    );
    pont.projeter_parametres(&f);
    assert_eq!(
        salons_regles(&f),
        [(
            "#photos".to_string(),
            0,
            "En sourdine : aucune notification".to_string(),
            "En sourdine jusqu'à 09:45".to_string()
        )]
    );
    assert!(g.get_a_sourdines());
    pont.menu_salon(&f, "c4", ancre);
    assert_eq!(
        entrees_du_menu(&f)[1],
        (
            "lever-sourdine".to_string(),
            "Réactiver les notifications".to_string()
        )
    );
    menu::fermer(&f);

    minutes.store(14, Ordering::SeqCst);
    pont.projeter_parametres(&f);
    assert_eq!(salons_regles(&f).len(), 1, "toujours en sourdine à 09:44");
    minutes.store(15, Ordering::SeqCst);
    pont.projeter_parametres(&f);
    assert!(
        salons_regles(&f).is_empty(),
        "la sourdine de 15 minutes s'est levée seule"
    );
    pont.menu_salon(&f, "c4", ancre);
    assert_eq!(ids_du_menu(&f), ["lu", "sourdine", "lien", "creer-salon"]);
    menu::fermer(&f);

    pont.definir_niveau_general(&f, 1);
    attendre(
        || reglages().general == Some(NotifySettingsGeneral::All),
        "niveau général enregistré",
    );
    pont.projeter_parametres(&f);
    assert_eq!(
        (g.get_niveau_general(), g.get_effectif_serveur().to_string()),
        (
            1,
            "Actuellement : tous les messages (réglage général)".to_string()
        )
    );
    pont.definir_niveau(&f, "s1", 3);
    pont.definir_niveau(&f, "c1", 2);
    attendre(
        || enregistre(&serveur, "\"c1\":\"mentions\""),
        "niveau du salon enregistré",
    );
    pont.projeter_parametres(&f);
    assert_eq!(
        (g.get_niveau_serveur(), g.get_effectif_serveur().to_string()),
        (3, "Actuellement : rien (réglage du serveur)".to_string())
    );
    assert_eq!(
        salons_regles(&f),
        [(
            "#général".to_string(),
            2,
            "Actuellement : mentions seulement (réglage du salon)".to_string(),
            String::new()
        )]
    );

    assert!(pont.menu_serveur(&f, "s2", ancre));
    assert_eq!(
        ids_du_menu(&f),
        [
            "creer-salon",
            "sourdine",
            "notifications",
            "lien",
            "quitter"
        ]
    );
    menu::choisir(&f, "sourdine");
    assert_eq!(f.global::<DonneesMenu>().get_titre(), "Sourdine : Travail");
    menu::choisir(&f, DureeSourdine::Toujours.id());
    attendre(
        || sourdine(&reglages(), "s2", i64::MAX - 1) == Some(0),
        "serveur en sourdine jusqu'à réactivation",
    );
    pont.menu_serveur(&f, "s2", ancre);
    menu::choisir(&f, "notifications");
    assert!(g.get_ouverte() && g.get_onglet() == "notifications");
    assert_eq!(
        g.get_serveur_notif(),
        "s2",
        "les réglages s'ouvrent sur ce serveur"
    );
    assert_eq!(g.get_sourdine_serveur(), "En sourdine jusqu'à réactivation");
    pont.mettre_en_sourdine(&f, "s2", None);
    attendre(
        || sourdine(&reglages(), "s2", 0).is_none(),
        "sourdine du serveur levée",
    );

    pont.definir_apercu_notifications(&f, false);
    pont.definir_son_notifications(&f, false);
    pont.definir_ignore_groupe(&f, true);
    attendre(
        || {
            let r = reglages();
            r.preview == Some(false)
                && r.sound == Some(false)
                && r.ignore_group == Some(vec!["s2".to_string()])
        },
        "aperçu, son et @everyone enregistrés",
    );
    pont.projeter_parametres(&f);
    assert!(!g.get_apercu_notifications() && !g.get_son_notifications() && g.get_ignore_groupe());
}

fn execution() -> tokio::runtime::Handle {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let execution = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    execution
}

fn fenetre_de_test() -> Principale {
    let f = creer_fenetre().unwrap();
    f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
    f.set_page(Page::Coquille);
    f.show().unwrap();
    f.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    f
}

fn client_de(store: &estompe_client::store::Store) -> Arc<Profils> {
    Profils::nouveaux(Arc::new(store.client().clone()), None)
}

fn pont_profil(
    f: &Principale,
    etat: estompe_client::store::Etat,
    adresse: &str,
    route: Route,
) -> Pont {
    let store = store_sur(etat, adresse);
    let profils = client_de(&store);
    let matin = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap();
    let pont = Pont::nouveau(
        store,
        f.as_weak(),
        execution(),
        None,
        Arc::new(move || matin),
        Navigation::nouvelle(route),
        profils,
    );
    pont.demarrer(f);
    pont.tout_projeter(f);
    rendre(f);
    pont
}

fn quadrants() -> image::RgbaImage {
    image::RgbaImage::from_fn(800, 400, |x, y| {
        image::Rgba(match (x < 400, y < 200) {
            (true, true) => [220, 30, 40, 255],
            (false, true) => [30, 200, 60, 255],
            (true, false) => [30, 60, 220, 255],
            (false, false) => [240, 210, 20, 255],
        })
    })
}

fn pixel_central(image: &slint::Image) -> [u8; 3] {
    let tampon = image.to_rgba8().expect("image en pixels");
    let p = tampon.as_slice()[(tampon.height() / 2 * tampon.width() + tampon.width() / 2) as usize];
    [p.r, p.g, p.b]
}

fn vert(c: [u8; 3]) -> bool {
    c[0].abs_diff(30) < 12 && c[1].abs_diff(200) < 12 && c[2].abs_diff(60) < 12
}

fn diffuser(pont: &Pont, f: &Principale, type_evenement: &str, data: serde_json::Value) {
    let mut abonne = pont.store().abonner();
    let estompe_client::temps_reel::Signal::Evenement(e) =
        commun::coquille::evenement(type_evenement, data)
    else {
        unreachable!()
    };
    pont.store().appliquer(&e);
    while let Ok(changements) = abonne.try_recv() {
        pont.appliquer(f, &changements);
    }
    rendre(f);
}

fn requete_vue(serveur: &ServeurMessages, ligne: &str) -> bool {
    serveur.requetes.lock().unwrap().iter().any(|r| r == ligne)
        || serveur
            .corps
            .lock()
            .unwrap()
            .iter()
            .any(|(r, _)| r == ligne)
}

#[test]
fn un_avatar_recadre_est_envoye_et_vu_en_temps_reel_par_un_autre_client() {
    use estompe_natif::coquille::CleAvatar;
    use estompe_natif::DonneesProfil;
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
    i_slint_backend_testing::init_no_event_loop();
    let serveur = ServeurMessages::demarrer(10);
    let a = fenetre_de_test();
    let dossier = tempfile::tempdir().unwrap();
    brancher(
        &a,
        OutilsParametres {
            memoire: Memoire::nouvelle(Preferences::default(), dossier.path().to_path_buf(), None),
            journal: Arc::new(Journal::ouvrir(&dossier.path().join("journaux"))),
            fuseau: Arc::new(|_| 60),
            horloge: Arc::new(|| MAINTENANT_MS),
            dossier_export: dossier.path().join("exports"),
        },
    );
    let pont_a = pont_profil(
        &a,
        commun::coquille::etat_avec_membres(),
        &serveur.adresse,
        Route::Accueil,
    );
    a.global::<DonneesParametres>()
        .invoke_ouvrir("compte".into());
    rendre(&a);
    let g = a.global::<DonneesProfil>();
    let serveurs: Vec<(String, bool)> = g
        .get_serveurs()
        .iter()
        .map(|s| (s.nom.to_string(), s.propre))
        .collect();
    assert_eq!(serveurs.len(), 4);
    assert_eq!(serveurs[0], ("Les amis".to_string(), false));
    assert_eq!(g.get_nuances().row_count(), 10);
    assert!(g.get_banniere_du_role() && !g.get_a_avatar());
    assert!(
        ElementHandle::find_by_accessible_label(&a, "Changer l'avatar")
            .any(|e| e.accessible_role() == Some(AccessibleRole::Button))
    );

    let p = pont_a.clone();
    let faible = a.as_weak();
    g.on_deplacer(move |dx, dy| p.deplacer_recadrage(&faible.upgrade().unwrap(), dx, dy));
    let p = pont_a.clone();
    let faible = a.as_weak();
    g.on_zoomer(move |z| p.zoomer_recadrage(&faible.upgrade().unwrap(), z));
    let p = pont_a.clone();
    let faible = a.as_weak();
    g.on_valider_recadrage(move || p.valider_recadrage(&faible.upgrade().unwrap()));

    pont_a.ouvrir_recadrage(&a, None, quadrants());
    rendre(&a);
    assert!(g.get_recadrage_ouvert());
    assert_eq!(g.get_recadrage_titre(), "Recadrer l'avatar");
    assert_eq!(
        (g.get_clip_x(), g.get_clip_y(), g.get_clip_cote()),
        (200, 0, 400)
    );
    touches(&a, "+", 4);
    assert_eq!(
        pont_a.recadrage_en_cours().unwrap().zoom,
        2.0,
        "+ zoome au clavier"
    );
    assert_eq!(g.get_zoom_lisible(), "2,0×");
    assert_eq!(g.get_clip_cote(), 200);
    touches(&a, SharedString::from(Key::UpArrow), 12);
    assert_eq!(
        pont_a.recadrage_en_cours().unwrap().y,
        -1.0,
        "les flèches déplacent"
    );
    let apercu = ElementHandle::find_by_accessible_label(
        &a,
        "Aperçu du recadrage : faire glisser ou utiliser les flèches pour déplacer, + et - pour zoomer",
    )
    .next()
    .expect("aperçu du recadrage");
    let position = apercu.absolute_position();
    let taille = apercu.size();
    let centre = slint::LogicalPosition::new(
        position.x + taille.width / 2.0,
        position.y + taille.height / 2.0,
    );
    a.window().dispatch_event(WindowEvent::PointerPressed {
        position: centre,
        button: slint::platform::PointerEventButton::Left,
    });
    for pas in 1..=6 {
        a.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(centre.x - 100.0 * pas as f32, centre.y),
        });
    }
    a.window().dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(centre.x - 600.0, centre.y),
        button: slint::platform::PointerEventButton::Left,
    });
    rendre(&a);
    assert_eq!(
        pont_a.recadrage_en_cours().unwrap().x,
        1.0,
        "glisser vers la gauche montre la droite"
    );
    assert_eq!(
        (g.get_clip_x(), g.get_clip_y(), g.get_clip_cote()),
        (600, 0, 200)
    );

    ElementHandle::find_by_accessible_label(&a, "Valider")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("bouton Valider")
        .mock_single_click(slint::platform::PointerEventButton::Left);
    attendre(|| requete_vue(&serveur, "PUT /me/avatar"), "avatar envoyé");
    attendre(|| pont_a.recadrage_en_cours().is_none(), "envoi terminé");

    let b = fenetre_de_test();
    b.global::<DonneesCoquille>().set_marge_ouverte(true);
    let mut etat_b = commun::coquille::etat_avec_membres();
    etat_b.compte = serde_json::from_value(
        serde_json::json!({"activated": true, "allowed": true, "id": "carol", "pseudo": "carol"}),
    )
    .unwrap();
    let pont_b = pont_profil(
        &b,
        etat_b,
        &serveur.adresse,
        Route::Salon {
            serveur: "s1".into(),
            salon: Some("c1".into()),
            message: None,
        },
    );
    pont_b.membres_defiles(&b, 0.0, 800.0);
    let portrait_de_bob = |f: &Principale| {
        f.global::<DonneesCoquille>()
            .get_membres()
            .iter()
            .find(|l| l.utilisateur == "u1")
            .map(|l| l.portrait)
            .unwrap()
    };
    assert_eq!(portrait_de_bob(&b).size().width, 0, "pas encore d'avatar");

    let cle = CleAvatar {
        utilisateur: "u1".into(),
        version: 1,
        serveur: None,
    };
    diffuser(
        &pont_b,
        &b,
        "avatar.updated",
        serde_json::json!({"user_id": "u1", "version": 1}),
    );
    attendre(
        || pont_b.profils().avatar(&cle).is_some(),
        "nouvel avatar téléchargé par l'autre client",
    );
    pont_b.membres_defiles(&b, 0.0, 800.0);
    let portrait = portrait_de_bob(&b);
    assert_eq!(portrait.size().width, 96);
    assert!(
        vert(pixel_central(&portrait)),
        "{:?}",
        pixel_central(&portrait)
    );

    brancher(
        &b,
        OutilsParametres {
            memoire: Memoire::nouvelle(Preferences::default(), dossier.path().join("b"), None),
            journal: Arc::new(Journal::ouvrir(&dossier.path().join("journaux-b"))),
            fuseau: Arc::new(|_| 60),
            horloge: Arc::new(|| MAINTENANT_MS),
            dossier_export: dossier.path().join("exports-b"),
        },
    );
    let p = pont_b.clone();
    let faible = b.as_weak();
    b.global::<DonneesCoquille>()
        .on_fermer_carte(move || p.fermer_carte(&faible.upgrade().unwrap()));
    pont_b.ouvrir_membre(&b, "carol");
    rendre(&b);
    ElementHandle::find_by_accessible_label(&b, "Modifier mon profil")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .expect("« Modifier mon profil » sur ma carte")
        .mock_single_click(slint::platform::PointerEventButton::Left);
    rendre(&b);
    let pb = b.global::<DonneesParametres>();
    assert!(
        pb.get_ouverte() && pb.get_onglet() == "compte",
        "la carte ouvre le compte"
    );
    assert!(!b.global::<DonneesCoquille>().get_carte_ouverte());

    diffuser(
        &pont_a,
        &a,
        "avatar.updated",
        serde_json::json!({"user_id": "u1", "version": 1}),
    );
    attendre(
        || pont_a.profils().avatar(&cle).is_some(),
        "mon avatar rechargé",
    );
    pont_a.projeter_profil(&a);
    assert!(g.get_a_avatar());
    assert!(vert(pixel_central(&g.get_portrait())));
    assert!(
        vert(pixel_central(&a.global::<DonneesCoquille>().get_portrait())),
        "barre de l'utilisateur"
    );

    pont_a.choisir_banniere(&a, "#2F5D8A");
    attendre(
        || {
            serveur
                .corps
                .lock()
                .unwrap()
                .iter()
                .any(|(r, c)| r == "PUT /me/banner" && c == r##"{"color":"#2f5d8a"}"##)
        },
        "bannière envoyée",
    );
    diffuser(
        &pont_a,
        &a,
        "banner.updated",
        serde_json::json!({"user_id": "u1", "color": "#2f5d8a"}),
    );
    attendre(
        || {
            pont_a.projeter_profil(&a);
            !g.get_banniere_du_role()
        },
        "bannière reçue",
    );
    assert!(g
        .get_nuances()
        .iter()
        .any(|n| n.code == "#2f5d8a" && n.choisie));
    attendre(
        || {
            pont_a.retirer_avatar(&a, "s2");
            requete_vue(&serveur, "DELETE /servers/s2/me/avatar")
        },
        "photo du serveur retirée",
    );
}
