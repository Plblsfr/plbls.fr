#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use estompe_natif::session::Session;
use estompe_plateforme::Dossiers;
use slint::ComponentHandle;
use std::cell::Cell;

const ESSAI_DE_LANCEMENT: &str = "--essai-de-lancement";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let essai = std::env::args().any(|a| a == ESSAI_DE_LANCEMENT);
    let dossiers = Dossiers::de_l_os()?;
    dossiers.creer()?;
    let session = Session::ouvrir(dossiers);
    let fenetre = estompe_natif::creer_fenetre()?;
    session.brancher(&fenetre);
    let invitation = estompe_natif::invitation_des_arguments(std::env::args().skip(1));
    let application = estompe_natif::demarrer(&session, &fenetre, invitation)?;
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
    fenetre.run()?;
    application
        .vocal
        .fermer_bloquant(estompe_client::vocal::DELAI_FERMETURE);
    session.retenir_geometrie(&fenetre);
    Ok(())
}
