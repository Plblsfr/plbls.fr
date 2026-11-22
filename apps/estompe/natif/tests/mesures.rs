use std::sync::Arc;
use std::time::Duration;

use estompe_natif::ecran_mesures::{brancher, EcranMesures};
use estompe_natif::{creer_fenetre, DonneesMesures};
use estompe_plateforme::{CompteursProcessus, InfosMachine, MesureProcessus, Resultat};
use slint::{ComponentHandle, Model};

struct Source(bool);

impl MesureProcessus for Source {
    fn disponible(&self) -> bool {
        self.0
    }
    fn compteurs(&self) -> Resultat<CompteursProcessus> {
        Ok(CompteursProcessus {
            prive: 60 << 20,
            ws: 90 << 20,
            pic_ws: 95 << 20,
            cpu_ms: 3,
        })
    }
    fn machine(&self) -> InfosMachine {
        InfosMachine {
            os: Some("Windows 11".into()),
            coeurs: 4,
            ..InfosMachine::default()
        }
    }
}

#[test]
fn l_ecran_mene_une_session_de_bout_en_bout_et_exporte() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let f = creer_fenetre().unwrap();
    let ecran = EcranMesures::nouveau(Arc::new(Source(true)), dossier.path().join("mesures"));
    let _minuterie = brancher(&f, ecran);
    let g = f.global::<DonneesMesures>();

    g.invoke_ouvrir();
    assert!(g.get_ouvert());
    assert!(!g.get_en_session());
    assert_eq!(g.get_scenarios().row_count(), 6);
    assert!(!g.get_peut_exporter());

    g.invoke_lancer("jeu".into());
    assert_eq!(g.get_scenario_en_cours(), "");

    g.invoke_demarrer();
    assert!(g.get_en_session());
    g.invoke_lancer("jeu".into());
    assert_eq!(g.get_scenario_en_cours(), "Partie de jeu avec overlays");
    assert_eq!(g.get_reste(), 300);
    g.invoke_lancer("repos".into());
    assert_ne!(g.get_erreur(), "");
    std::thread::sleep(Duration::from_millis(1100));
    g.invoke_terminer_scenario();
    assert_eq!(g.get_scenario_en_cours(), "");
    assert!(g.get_scenarios().iter().any(|s| s.id == "jeu" && s.fait));

    g.invoke_arreter();
    assert!(!g.get_en_session());
    assert!(g.get_peut_exporter());
    g.invoke_exporter();
    assert!(
        g.get_message().contains("Mesures exportées"),
        "{}",
        g.get_message()
    );
    let fichiers: Vec<_> = std::fs::read_dir(dossier.path().join("mesures"))
        .unwrap()
        .collect();
    assert_eq!(fichiers.len(), 1);
    g.invoke_fermer();
    assert!(!g.get_ouvert());
    sans_mesure_du_processus_la_session_ne_demarre_pas();
}

fn sans_mesure_du_processus_la_session_ne_demarre_pas() {
    let dossier = tempfile::tempdir().unwrap();
    let f = creer_fenetre().unwrap();
    let ecran = EcranMesures::nouveau(Arc::new(Source(false)), dossier.path().into());
    let _minuterie = brancher(&f, ecran);
    let g = f.global::<DonneesMesures>();
    g.invoke_ouvrir();
    assert!(!g.get_disponible());
    g.invoke_demarrer();
    assert!(!g.get_en_session());
    assert_ne!(g.get_erreur(), "");
}
