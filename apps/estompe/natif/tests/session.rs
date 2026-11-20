use estompe_client::preferences::{self, Fenetre, Preferences};
use estompe_natif::session::{self, Session};
use estompe_natif::{creer_fenetre, ModeTheme, Mouvement, Theme};
use estompe_plateforme::Dossiers;
use slint::ComponentHandle;

#[test]
fn les_preferences_sont_appliquees_a_la_fenetre_puis_retenues() {
    i_slint_backend_testing::init_no_event_loop();
    let tmp = tempfile::tempdir().unwrap();
    let dossiers = Dossiers::sous(tmp.path());
    dossiers.creer().unwrap();
    Preferences {
        theme: preferences::Theme::Sombre,
        reduire_animations: true,
        fenetre: Some(Fenetre {
            x: 40,
            y: 60,
            largeur: 900,
            hauteur: 700,
        }),
        ..Preferences::default()
    }
    .enregistrer(&dossiers.donnees)
    .unwrap();

    let s = Session::ouvrir(dossiers.clone());
    let f = creer_fenetre().unwrap();
    s.brancher(&f);
    f.show().unwrap();
    assert_eq!(f.global::<Theme>().get_mode(), ModeTheme::Sombre);
    assert!(f.global::<Theme>().get_sombre());
    assert!(f.global::<Mouvement>().get_reduit());
    let g = session::geometrie(&f);
    assert_eq!((g.largeur, g.hauteur), (900, 700));

    f.window().set_size(slint::LogicalSize::new(1000.0, 800.0));
    f.invoke_theme_change(ModeTheme::Clair);
    s.retenir_geometrie(&f);
    let relu = Preferences::charger(&dossiers.donnees).preferences;
    assert_eq!(relu.theme, preferences::Theme::Clair);
    let fenetre = relu.fenetre.unwrap();
    assert_eq!((fenetre.largeur, fenetre.hauteur), (1000, 800));
    assert!(relu.reduire_animations);
}

#[test]
fn une_geometrie_aberrante_est_ignoree() {
    let f = |largeur, hauteur| Fenetre {
        x: 0,
        y: 0,
        largeur,
        hauteur,
    };
    assert!(session::geometrie_valide(&f(1180, 760)));
    assert!(session::geometrie_valide(&f(
        session::LARGEUR_MIN,
        session::HAUTEUR_MIN
    )));
    assert!(!session::geometrie_valide(&f(0, 0)));
    assert!(!session::geometrie_valide(&f(100, 760)));
    assert!(!session::geometrie_valide(&f(1180, 1_000_000)));
}

#[test]
fn un_fichier_de_preferences_corrompu_n_empeche_pas_d_ouvrir_la_session() {
    let tmp = tempfile::tempdir().unwrap();
    let dossiers = Dossiers::sous(tmp.path());
    dossiers.creer().unwrap();
    std::fs::write(Preferences::chemin(&dossiers.donnees), b"{ pas du json").unwrap();
    let s = Session::ouvrir(dossiers.clone());
    assert_eq!(s.memoire.lire(Clone::clone), Preferences::default());
    assert_eq!(
        Preferences::charger(&dossiers.donnees).preferences,
        Preferences::default()
    );
}
