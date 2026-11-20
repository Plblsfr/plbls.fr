use estompe_natif::{creer_fenetre, Page, Principale};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};
use std::time::Duration;

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

fn tabulations(fenetre: &Principale, nombre: usize) {
    for _ in 0..nombre {
        touche(fenetre, Key::Tab);
    }
}

fn demonstration() -> Principale {
    i_slint_backend_testing::init_no_event_loop();
    let fenetre = creer_fenetre().unwrap();
    fenetre.set_page(Page::Composants);
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    fenetre.show().unwrap();
    fenetre
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    fenetre.invoke_focus_demo();
    fenetre
}

#[test]
fn entree_et_espace_activent_le_bouton_qui_a_le_focus() {
    let f = demonstration();
    touche(&f, Key::Return);
    assert_eq!(f.get_clics_demo(), 1);
    touche(&f, " ");
    assert_eq!(f.get_clics_demo(), 2);
}

#[test]
fn la_tabulation_passe_au_bouton_suivant_et_saute_le_bouton_inactif() {
    let f = demonstration();
    tabulations(&f, 1);
    touche(&f, Key::Return);
    assert_eq!(f.get_clics_demo(), 1, "le deuxième bouton a reçu Entrée");
    touche(&f, Key::Backtab);
    touche(&f, Key::Return);
    assert_eq!(f.get_clics_demo(), 2, "Maj+Tab revient au premier bouton");
    tabulations(&f, 5);
    assert!(!f.get_modal_demo_ouvert());
    touche(&f, Key::Return);
    assert!(
        f.get_modal_demo_ouvert(),
        "cinq tabulations mènent à « Ouvrir un modal » : le bouton inactif est sauté"
    );
}

#[test]
fn la_tabulation_atteint_le_champ_la_case_et_l_interrupteur() {
    let f = demonstration();
    tabulations(&f, 6);
    touche(&f, "!");
    assert_eq!(
        f.get_saisie_demo(),
        "!",
        "le champ atteint au clavier a tout son texte sélectionné : la frappe le remplace"
    );
    tabulations(&f, 2);
    assert!(f.get_case_demo());
    touche(&f, " ");
    assert!(!f.get_case_demo(), "Espace décoche la case");
    tabulations(&f, 1);
    assert!(f.get_interrupteur_demo());
    touche(&f, Key::Return);
    assert!(!f.get_interrupteur_demo(), "Entrée bascule l'interrupteur");
}

#[test]
fn echap_ferme_le_modal() {
    let f = demonstration();
    f.set_modal_demo_ouvert(true);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    touche(&f, Key::Escape);
    assert!(!f.get_modal_demo_ouvert());
    assert_eq!(f.get_confirmations_demo(), 0);
}

#[test]
fn dans_un_modal_le_focus_part_de_la_croix_et_reste_dans_le_modal() {
    let f = demonstration();
    f.set_modal_demo_ouvert(true);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    tabulations(&f, 2);
    touche(&f, Key::Return);
    assert_eq!(
        f.get_confirmations_demo(),
        1,
        "croix, Annuler, puis Supprimer"
    );
    assert!(!f.get_modal_demo_ouvert());

    f.set_modal_demo_ouvert(true);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    for _ in 0..12 {
        touche(&f, Key::Tab);
        assert!(f.get_modal_demo_ouvert());
    }
    assert_eq!(
        f.get_clics_demo(),
        0,
        "aucun bouton sous le modal n'a été atteint"
    );
    touche(&f, Key::Return);
    assert_eq!(f.get_clics_demo(), 0);
}

#[test]
fn entree_sur_la_croix_ferme_le_modal() {
    let f = demonstration();
    f.set_modal_demo_ouvert(true);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    touche(&f, Key::Return);
    assert!(!f.get_modal_demo_ouvert());
    assert_eq!(f.get_confirmations_demo(), 0);
}
