use estompe_plateforme::instance::{acquerir, Instance, MessageInstance, NomInstance};
use estompe_plateforme::{Dossiers, VARIABLE_DOSSIER};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BORNE: Duration = Duration::from_secs(20);

#[test]
fn c1_un_second_lancement_passe_ses_arguments_et_sort_sans_fenetre() {
    let dossier = tempfile::tempdir().unwrap();
    let nom = NomInstance::selon(&Dossiers::sous(dossier.path()), Some(dossier.path()));
    let Instance::Premiere(ecoute) = acquerir(&nom, &MessageInstance::default()).unwrap() else {
        panic!("le test doit tenir la première instance");
    };
    let (envoi, recus) = mpsc::channel();
    ecoute
        .ecouter(Box::new(move |message| {
            let _ = envoi.send(message);
        }))
        .unwrap();
    let mut second = Command::new(env!("CARGO_BIN_EXE_estompe-natif"))
        .args(["--invitation", "ABCDE-23456"])
        .env(VARIABLE_DOSSIER, dossier.path())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .spawn()
        .unwrap();
    let limite = Instant::now() + BORNE;
    let statut = loop {
        if let Some(statut) = second.try_wait().unwrap() {
            break statut;
        }
        if Instant::now() > limite {
            let _ = second.kill();
            panic!("le second lancement tourne encore après {BORNE:?}");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(statut.success(), "sortie du second lancement : {statut}");
    let message = recus.recv_timeout(BORNE).unwrap();
    assert_eq!(message.arguments, ["--invitation", "ABCDE-23456"]);
}
