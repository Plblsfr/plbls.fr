slint::include_modules!();

pub mod bureau;
pub mod confirmation;
pub mod coquille;
pub mod ecrans;
pub mod entree;
pub mod jeux;
pub mod menu;
pub mod messagerie;
pub mod mesures;
pub mod mise_a_jour;
pub mod session;
pub mod video;
pub mod vocal;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OPTION_INVITATION: &str = "--invitation";

pub fn creer_fenetre() -> Result<Principale, slint::PlatformError> {
    let fenetre = Principale::new()?;
    coquille::installer_modeles(&fenetre);
    menu::brancher(&fenetre);
    confirmation::brancher(&fenetre);
    fenetre.set_version(VERSION.into());
    fenetre
        .global::<DonneesMiseAJour>()
        .set_version_installee(VERSION.into());
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
    pub vocal: std::sync::Arc<estompe_client::vocal::SessionVocale>,
    pub jeux: std::sync::Arc<jeux::ServiceJeux>,
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
    let canal_maj = estompe_client::mise_a_jour::canal();
    let mut reels = session.services(&canal.0)?;
    let plateforme = estompe_plateforme::Plateforme::de_l_os();
    reels.services.installer = Some(mise_a_jour::installateur(canal_maj.0.clone()));
    let navigateur = reels.services.navigateur.clone();
    ecrans::brancher(fenetre, &canal.0);
    let entree = entree::Entree::demarrer(
        entree::Machine::nouvelle(invitation)
            .avec_installation(plateforme.installation.disponible()),
        reels.services,
        canal,
        ecrans::publieur(fenetre),
    )
    .map_err(|e| format!("fil de l'entrée : {e}"))?;
    let (micro, sortie, traitement, ecoute, camera, qualite_camera, partage, incrustation) =
        session.memoire.lire(|p| {
            (
                p.micro.clone(),
                p.sortie_audio.clone(),
                p.traitement_micro,
                p.ecoute.clone(),
                p.camera.clone(),
                p.qualite_camera,
                p.partage,
                p.incrustation,
            )
        });
    let audio: std::sync::Arc<dyn estompe_client::audio::SystemeAudio> =
        std::sync::Arc::new(estompe_plateforme::AudioSysteme);
    let apm = apm_vocal();
    let cameras: std::sync::Arc<dyn estompe_plateforme::SystemeCamera> =
        std::sync::Arc::new(estompe_plateforme::CamerasSysteme);
    let vocal = std::sync::Arc::new(estompe_client::vocal::SessionVocale::demarrer(
        &entree.runtime(),
        estompe_client::vocal::Dependances {
            api: std::sync::Arc::new(reels.client.clone()),
            connecteur: connecteur_vocal(),
            audio: audio.clone(),
            cameras: cameras.clone(),
            ecrans: plateforme.capture_ecran.clone(),
            sons: plateforme.capture_son.clone(),
            apm: apm.clone(),
            reglages: estompe_client::vocal::Reglages {
                micro,
                sortie,
                traitement,
                ecoute,
                camera,
                qualite_camera,
                partage,
                incrustation,
                ..estompe_client::vocal::Reglages::default()
            },
        },
    ));
    let audio_messages = audio.clone();
    let outils_vocaux = vocal::OutilsVocaux {
        session: vocal.clone(),
        audio,
        raccourcis: std::sync::Arc::new(estompe_plateforme::RaccourcisSysteme),
        cameras,
        ecrans: plateforme.capture_ecran.clone(),
        sons: plateforme.capture_son.clone(),
        apm,
    };
    let service_jeux = std::sync::Arc::new(jeux::ServiceJeux::demarrer(
        &entree.runtime(),
        plateforme.premier_plan.clone(),
        {
            let memoire = session.memoire.clone();
            std::sync::Arc::new(move || memoire.lire(|p| p.jeux.clone()))
        },
        estompe_client::jeux::PERIODE_SONDAGE,
    ));
    jeux::reglages::brancher(
        fenetre,
        service_jeux.clone(),
        session.memoire.clone(),
        std::sync::Arc::new(estompe_plateforme::FichiersSysteme),
        &entree.runtime(),
    );
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
        jeux: Some(service_jeux.abonner()),
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
    mise_a_jour::brancher(fenetre, &service_maj, &entree.runtime());
    let suivi = coquille.suivi_du_focus();
    let premier_plan = service_maj.emetteur();
    suivre_la_fenetre(
        fenetre,
        move |actif| {
            suivi(actif);
            if actif {
                premier_plan.envoyer(estompe_client::mise_a_jour::EvenementMaj::PremierPlan);
            }
        },
        coquille.suivi_du_depot(),
        coquille.suivi_de_la_visibilite(),
    );
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
    let session_vocale = vocal.clone();
    bureau
        .fermeture()
        .avant_de_quitter(bureau::EtapeFermeture::Vocal, move || {
            session_vocale.fermer_bloquant(estompe_client::vocal::DELAI_FERMETURE);
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
        vocal,
        jeux: service_jeux,
    })
}

fn suivre_la_fenetre(
    fenetre: &Principale,
    focus: impl Fn(bool) + 'static,
    depot: impl Fn(&Principale, coquille::Depot) + 'static,
    visibilite: impl Fn(&Principale, bool) + 'static,
) {
    use coquille::Depot;
    use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
    use winit::event::WindowEvent;
    let faible = fenetre.as_weak();
    let cachee = std::cell::Cell::new((false, false));
    fenetre.window().on_winit_window_event(move |_, evenement| {
        let avant = cachee.get();
        match evenement {
            WindowEvent::Occluded(occultee) => cachee.set((*occultee, avant.1)),
            WindowEvent::Resized(taille) => {
                cachee.set((avant.0, taille.width == 0 || taille.height == 0))
            }
            _ => {}
        }
        let apres = cachee.get();
        if (avant.0 || avant.1) != (apres.0 || apres.1) {
            if let Some(f) = faible.upgrade() {
                visibilite(&f, !(apres.0 || apres.1));
            }
        }
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
