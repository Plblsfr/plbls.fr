use std::sync::Arc;
use std::time::Duration;

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
