#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use estompe_client::journal::Niveau;
use estompe_natif::session::Session;
use estompe_plateforme::instance::{self, Instance, MessageInstance, NomInstance};
use estompe_plateforme::{Dossiers, OPTION_ARRIERE_PLAN};
use slint::ComponentHandle;
use std::cell::Cell;

const ESSAI_DE_LANCEMENT: &str = "--essai-de-lancement";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let essai = arguments.iter().any(|a| a == ESSAI_DE_LANCEMENT);
    let arriere_plan = arguments.iter().any(|a| a == OPTION_ARRIERE_PLAN);
    let dossiers = Dossiers::de_l_os()?;
    dossiers.creer()?;
    let acquisition = instance::acquerir(
        &NomInstance::de(&dossiers),
        &MessageInstance::de_l_environnement(arguments.clone()),
    );
    let ecoute = match acquisition {
        Ok(Instance::Seconde) => return Ok(()),
        Ok(Instance::Premiere(ecoute)) => Ok(ecoute),
        Err(e) => Err(e),
    };
    let session = Session::ouvrir(dossiers);
    let fenetre = estompe_natif::creer_fenetre()?;
    session.brancher(&fenetre);
    let invitation = estompe_natif::invitation_des_arguments(arguments);
    let application = estompe_natif::demarrer(&session, &fenetre, invitation)?;
    match ecoute.and_then(|e| e.ecouter(application.bureau.recepteur_instance())) {
        Ok(()) => {}
        Err(e) => session.journal.ecrire(
            Niveau::Avertissement,
            "instance",
            &format!("instance unique indisponible : {e}"),
        ),
    }
    if essai {
        let rendue = Cell::new(false);
        fenetre
            .window()
            .set_rendering_notifier(move |etat, _| {
                if matches!(etat, slint::RenderingState::AfterRendering) && !rendue.replace(true) {
                    println!("fenêtre affichée");
                    let _ = slint::quit_event_loop();
                }
            })
            .map_err(|e| format!("notification de rendu indisponible : {e:?}"))?;
    }
    if essai || !(arriere_plan && application.bureau.zone_presente()) {
        fenetre.show()?;
    }
    slint::run_event_loop_until_quit()?;
    application.bureau.fermeture().quitter();
    Ok(())
}
