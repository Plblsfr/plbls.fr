slint::include_modules!();

pub mod confirmation;
pub mod coquille;
pub mod ecrans;
pub mod entree;
pub mod menu;
pub mod messagerie;
pub mod mesures;
pub mod session;
pub mod vocal;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OPTION_INVITATION: &str = "--invitation";

pub fn creer_fenetre() -> Result<Principale, slint::PlatformError> {
    let fenetre = Principale::new()?;
    coquille::installer_modeles(&fenetre);
    menu::brancher(&fenetre);
    confirmation::brancher(&fenetre);
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
    pub coquille: coquille::Coquille,
    pub entree: entree::Entree,
    pub client: estompe_client::http::Client,
    pub auth: std::sync::Arc<estompe_client::auth::Authentification>,
    pub vocal: std::sync::Arc<estompe_client::vocal::SessionVocale>,
}

#[cfg(feature = "livekit")]
fn connecteur_vocal() -> std::sync::Arc<dyn estompe_client::vocal::Connecteur> {
    std::sync::Arc::new(estompe_client::vocal::livekit::ConnecteurLiveKit)
}

#[cfg(not(feature = "livekit"))]
fn connecteur_vocal() -> std::sync::Arc<dyn estompe_client::vocal::Connecteur> {
    std::sync::Arc::new(estompe_client::vocal::ConnecteurIndisponible)
}

#[cfg(feature = "livekit")]
fn apm_vocal() -> Option<std::sync::Arc<estompe_client::audio::Apm>> {
    Some(estompe_client::vocal::livekit::apm())
}

#[cfg(not(feature = "livekit"))]
fn apm_vocal() -> Option<std::sync::Arc<estompe_client::audio::Apm>> {
    None
}

pub fn demarrer(
    session: &session::Session,
    fenetre: &Principale,
    invitation: Option<String>,
) -> Result<Application, String> {
    let canal = entree::canal();
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
    let (micro, sortie, traitement, ecoute) = session.memoire.lire(|p| {
        (
            p.micro.clone(),
            p.sortie_audio.clone(),
            p.traitement_micro,
            p.ecoute.clone(),
        )
    });
    let audio: std::sync::Arc<dyn estompe_client::audio::SystemeAudio> =
        std::sync::Arc::new(estompe_plateforme::AudioSysteme);
    let apm = apm_vocal();
    let vocal = std::sync::Arc::new(estompe_client::vocal::SessionVocale::demarrer(
        &entree.runtime(),
        estompe_client::vocal::Dependances {
            api: std::sync::Arc::new(reels.client.clone()),
            connecteur: connecteur_vocal(),
            audio: audio.clone(),
            apm: apm.clone(),
            reglages: estompe_client::vocal::Reglages {
                micro,
                sortie,
                traitement,
                ecoute,
                ..estompe_client::vocal::Reglages::default()
            },
        },
    ));
    let audio_messages = audio.clone();
    let outils_vocaux = vocal::OutilsVocaux {
        session: vocal.clone(),
        audio,
        raccourcis: std::sync::Arc::new(estompe_plateforme::RaccourcisSysteme),
        apm,
    };
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
        vocal: Some(outils_vocaux),
        son: estompe_plateforme::SonPeripheriques::nouveau(audio_messages, {
            let memoire = session.memoire.clone();
            std::sync::Arc::new(move |sens| {
                memoire.lire(|p| match sens {
                    estompe_plateforme::SensAudio::Entree => p.micro.clone(),
                    estompe_plateforme::SensAudio::Sortie => p.sortie_audio.clone(),
                })
            })
        }),
        conversions: std::sync::Arc::new(estompe_client::video::Conversions::du_systeme(
            session
                .dossiers
                .cache
                .join(estompe_client::video::DOSSIER_CONVERSIONS),
        )),
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
    suivre_la_fenetre(
        fenetre,
        coquille.suivi_du_focus(),
        coquille.suivi_du_depot(),
    );
    Ok(Application {
        coquille,
        entree,
        client: reels.client,
        auth: reels.auth,
        vocal,
    })
}

fn suivre_la_fenetre(
    fenetre: &Principale,
    focus: impl Fn(bool) + 'static,
    depot: impl Fn(&Principale, coquille::Depot) + 'static,
) {
    use coquille::Depot;
    use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
    use winit::event::WindowEvent;
    let faible = fenetre.as_weak();
    fenetre.window().on_winit_window_event(move |_, evenement| {
        let signal = match evenement {
            WindowEvent::Focused(actif) => {
                focus(*actif);
                None
            }
            WindowEvent::HoveredFile(_) => Some(Depot::Survol(true)),
            WindowEvent::HoveredFileCancelled => Some(Depot::Survol(false)),
            WindowEvent::DroppedFile(chemin) => Some(Depot::Fichiers(vec![chemin.clone()])),
            _ => None,
        };
        if let (Some(signal), Some(f)) = (signal, faible.upgrade()) {
            depot(&f, signal);
        }
        EventResult::Propagate
    });
}
