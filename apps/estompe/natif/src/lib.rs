slint::include_modules!();

pub mod bureau;
pub mod coquille;
pub mod ecrans;
pub mod entree;
pub mod messagerie;
pub mod mise_a_jour;
pub mod session;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OPTION_INVITATION: &str = "--invitation";

pub fn creer_fenetre() -> Result<Principale, slint::PlatformError> {
    let fenetre = Principale::new()?;
    coquille::installer_modeles(&fenetre);
    fenetre.set_version(VERSION.into());
    fenetre.set_debogage(cfg!(debug_assertions));
    Ok(fenetre)
}

pub fn invitation_des_arguments(arguments: impl IntoIterator<Item = String>) -> Option<String> {
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == OPTION_INVITATION {
            return arguments.next().filter(|valeur| !valeur.trim().is_empty());
        }
        if let Some(valeur) = argument.strip_prefix("--invitation=") {
            return Some(valeur.to_string()).filter(|valeur| !valeur.trim().is_empty());
        }
    }
    None
}

pub struct Application {
    pub bureau: bureau::Bureau,
    pub coquille: coquille::Coquille,
    pub entree: entree::Entree,
    pub mise_a_jour: estompe_client::mise_a_jour::MiseAJour,
    pub client: estompe_client::http::Client,
    pub auth: std::sync::Arc<estompe_client::auth::Authentification>,
}

pub fn demarrer(
    session: &session::Session,
    fenetre: &Principale,
    invitation: Option<String>,
) -> Result<Application, String> {
    let canal = entree::canal();
    let canal_maj = estompe_client::mise_a_jour::canal();
    let reels = session.services(&canal.0)?;
    let navigateur = reels.services.navigateur.clone();
    ecrans::brancher(fenetre, &canal.0);
    let entree = entree::Entree::demarrer(
        entree::Machine::nouvelle(invitation),
        reels.services,
        canal,
        ecrans::publieur(fenetre),
    )
    .map_err(|e| format!("fil de l'entrée : {e}"))?;
    let jeton = reels.auth.fournisseur_jeton();
    let production = session.adresse.url == estompe_client::config::ADRESSE_PRODUCTION;
    let fabrique = coquille::FabriqueReelle {
        client: reels.client.clone(),
        jeton: std::sync::Arc::new(jeton),
        adresses_sections: estompe_client::sections::adresses_sections(
            |nom| std::env::var(nom).ok(),
            production,
        ),
        journal: Some(session.journal.clone()),
        navigateur,
        dossier_donnees: Some(session.dossiers.donnees.clone()),
        medias: messagerie::medias::Medias::nouveaux(
            std::sync::Arc::new(reels.client.clone()),
            estompe_client::cache::CacheDisque::ouvrir(
                session
                    .dossiers
                    .cache
                    .join(messagerie::medias::DOSSIER_MEDIAS),
                messagerie::medias::BORNE_DISQUE_MEDIAS,
            )
            .ok(),
        ),
        profils: coquille::Profils::nouveaux(
            std::sync::Arc::new(reels.client.clone()),
            estompe_client::cache::CacheDisque::ouvrir(
                session
                    .dossiers
                    .cache
                    .join(coquille::avatars::DOSSIER_AVATARS),
                coquille::avatars::BORNE_DISQUE,
            )
            .ok(),
        ),
    };
    let coquille = coquille::Coquille::brancher(
        fenetre,
        &entree,
        std::sync::Arc::new(fabrique),
        session.memoire.clone(),
        coquille::horloge_locale(),
    );
    let plateforme = estompe_plateforme::Plateforme::de_l_os();
    let dossier_maj = session
        .dossiers
        .cache
        .join(estompe_client::mise_a_jour::DOSSIER_MISES_A_JOUR);
    estompe_client::mise_a_jour::nettoyer(&dossier_maj, VERSION);
    let source_maj = mise_a_jour::source_reseau(
        plateforme.installation.as_ref(),
        dossier_maj,
        std::env::var(estompe_client::mise_a_jour::VARIABLE_MANIFESTE)
            .ok()
            .as_deref(),
    )?;
    let service_maj = mise_a_jour::demarrer_service(
        &entree.runtime(),
        mise_a_jour::OptionsService {
            canal: canal_maj,
            source: std::sync::Arc::new(source_maj),
            installation: plateforme.installation.clone(),
            fenetre: fenetre.as_weak(),
            entree: entree.emetteur(),
            redemarrer: mise_a_jour::redemarrage(),
            journal: Some(session.journal.clone()),
        },
    );
    let suivi = coquille.suivi_du_focus();
    let premier_plan = service_maj.emetteur();
    suivre_le_focus(fenetre, move |actif| {
        suivi(actif);
        if actif {
            premier_plan.envoyer(estompe_client::mise_a_jour::EvenementMaj::PremierPlan);
        }
    });
    let emetteur = entree.emetteur();
    let bureau = bureau::Bureau::installer(
        fenetre,
        bureau::OptionsBureau {
            zone: plateforme.zone,
            pastille: plateforme.pastille,
            demarrage: plateforme.demarrage,
            memoire: session.memoire.clone(),
            entree: std::sync::Arc::new(move |evenement| emetteur.envoyer(evenement)),
            journal: Some(session.journal.clone()),
            suivi_du_focus: Some(Box::new(coquille.suivi_du_focus())),
        },
    );
    let arret = coquille.arret();
    bureau
        .fermeture()
        .avant_de_quitter(bureau::EtapeFermeture::Coquille, move || {
            arret.arreter(bureau::DELAI_ARRET);
        });
    coquille.observer(bureau::notifications::Notifieur::nouveau(
        bureau::notifications::OptionsNotifieur {
            systeme: plateforme.notifications,
            memoire: session.memoire.clone(),
            fenetre_active: coquille.fenetre_active_partagee(),
            source: coquille.source_pont(),
            runtime: entree.runtime(),
            journal: Some(session.journal.clone()),
        },
    ));
    Ok(Application {
        bureau,
        coquille,
        entree,
        mise_a_jour: service_maj,
        client: reels.client,
        auth: reels.auth,
    })
}

fn suivre_le_focus(fenetre: &Principale, suivi: impl Fn(bool) + 'static) {
    use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
    fenetre.window().on_winit_window_event(move |_, evenement| {
        if let winit::event::WindowEvent::Focused(actif) = evenement {
            suivi(*actif);
        }
        EventResult::Propagate
    });
}
