use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use estompe_natif::ecrans::afficher;
use estompe_natif::entree::Etat;
use estompe_natif::{creer_fenetre, Page, Principale};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};

fn touche(fenetre: &Principale, texte: impl Into<SharedString>) {
    let text: SharedString = texte.into();
    fenetre
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    fenetre
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
}

struct Compteurs {
    connexions: Rc<Cell<u32>>,
    inscriptions: Rc<Cell<u32>>,
    annulations: Rc<Cell<u32>>,
}

fn ecran_de_connexion() -> (Principale, Compteurs) {
    i_slint_backend_testing::init_no_event_loop();
    let f = creer_fenetre().unwrap();
    f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
    afficher(&f, &Etat::Deconnecte);
    let compteurs = Compteurs {
        connexions: Rc::default(),
        inscriptions: Rc::default(),
        annulations: Rc::default(),
    };
    let c = compteurs.connexions.clone();
    f.on_se_connecter(move || c.set(c.get() + 1));
    let c = compteurs.inscriptions.clone();
    f.on_creer_compte(move || c.set(c.get() + 1));
    let c = compteurs.annulations.clone();
    f.on_annuler_connexion(move || c.set(c.get() + 1));
    f.show().unwrap();
    f.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    assert_eq!(f.get_page(), Page::Connexion);
    (f, compteurs)
}

#[test]
fn c2_entree_sur_se_connecter_puis_tab_vers_creer_un_compte() {
    let (f, c) = ecran_de_connexion();
    f.invoke_focus_connexion();
    touche(&f, Key::Return);
    assert_eq!(c.connexions.get(), 1);
    touche(&f, Key::Tab);
    touche(&f, Key::Return);
    assert_eq!(c.inscriptions.get(), 1, "Tab mène à « Créer un compte »");
    touche(&f, Key::Backtab);
    touche(&f, " ");
    assert_eq!(c.connexions.get(), 2, "Maj+Tab revient à « Se connecter »");
}

#[test]
fn c2_pendant_l_attente_entree_sur_annuler() {
    let (f, c) = ecran_de_connexion();
    f.invoke_focus_connexion();
    afficher(&f, &Etat::AttenteNavigateur);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    touche(&f, Key::Tab);
    touche(&f, Key::Return);
    assert_eq!(
        c.annulations.get(),
        1,
        "Tab mène à « Annuler » pendant l'attente"
    );
    assert_eq!(c.connexions.get(), 0);
    assert_eq!(c.inscriptions.get(), 0, "« Créer un compte » est inactif");
}
