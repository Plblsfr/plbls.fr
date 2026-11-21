mod commun;

use chrono::NaiveDate;
use estompe_client::appels::EmojiPersonnalise;
use estompe_client::auth::ErreurAuth;
use estompe_client::mise_a_jour::{ErreurMiseAJour, EtatMaj, Phase};
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::messages::ChangementMessages;
use estompe_client::store::{Changement, CleConversation, Store};
use estompe_client::temps_reel::EtatReseau;
use estompe_client::types::LinkPreview;
use estompe_client::version::MiseAJourDisponible;
use estompe_natif::coquille::pont::vider_modeles;
use estompe_natif::coquille::{
    avatars, brancher_messagerie, CleAvatar, Icone, Navigation, Pont, Profils,
};
use estompe_natif::ecrans::afficher;
use estompe_natif::entree::{self, Compte, Etat};
use estompe_natif::messagerie::medias::{self, CleImage, Format, Medias, Source};
use estompe_natif::messagerie::pont::Telechargement;
use estompe_natif::messagerie::selecteur::Cible;
use estompe_natif::mise_a_jour;
use estompe_natif::{
    creer_fenetre, DonneesCoquille, DonneesCreationSondage, DonneesMessagerie, DonneesSaisie,
    DonneesSelecteur, DonneesVocal, EtapeVocal, ModeTheme, Mouvement, Principale, Theme,
};
use i_slint_backend_testing::{TestingBackend, TestingBackendOptions};
use slint::{ComponentHandle, Model};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const LARGEUR: u32 = 1180;
const HAUTEUR: u32 = 900;
const ECART_PAR_CANAL: u8 = 24;
const PART_TOLEREE: f64 = 0.001;
const VARIABLE: &str = "ESTOMPE_CAPTURES";

struct Ecran {
    nom: &'static str,
    theme: ModeTheme,
    preparer: fn(&Principale),
}

macro_rules! ecrans {
    ($($nom:literal, $theme:ident, $preparer:ident;)*) => {
        &[$(Ecran { nom: $nom, theme: ModeTheme::$theme, preparer: $preparer },)*]
    };
}

const ECRANS: &[Ecran] = ecrans![
    "demonstration-clair", Clair, demonstration;
    "demonstration-sombre", Sombre, demonstration;
    "modal-clair", Clair, modal;
    "modal-sombre", Sombre, modal;
    "a-propos-clair", Clair, a_propos;
    "a-propos-sombre", Sombre, a_propos;
    "demarrage-clair", Clair, demarrage;
    "connexion-clair", Clair, connexion;
    "connexion-sombre", Sombre, connexion;
    "connexion-attente-clair", Clair, connexion_attente;
    "connexion-attente-sombre", Sombre, connexion_attente;
    "connexion-erreur-clair", Clair, connexion_erreur;
    "connexion-erreur-sombre", Sombre, connexion_erreur;
    "verification-sombre", Sombre, verification;
    "refus-clair", Clair, refus;
    "refus-sombre", Sombre, refus;
    "activation-clair", Clair, activation;
    "activation-erreur-clair", Clair, activation_erreur;
    "activation-erreur-sombre", Sombre, activation_erreur;
    "activation-en-cours-clair", Clair, activation_en_cours;
    "invitation-clair", Clair, invitation;
    "invitation-erreur-clair", Clair, invitation_erreur;
    "invitation-erreur-sombre", Sombre, invitation_erreur;
    "invitation-en-cours-clair", Clair, invitation_en_cours;
    "injoignable-clair", Clair, injoignable;
    "mise-a-jour-clair", Clair, mise_a_jour;
    "mise-a-jour-sombre", Sombre, mise_a_jour;
    "mise-a-jour-bandeau-clair", Clair, mise_a_jour_bandeau;
    "mise-a-jour-bandeau-sombre", Sombre, mise_a_jour_bandeau;
    "mise-a-jour-panneau-a-jour-clair", Clair, mise_a_jour_panneau_a_jour;
    "mise-a-jour-telechargement-clair", Clair, mise_a_jour_telechargement;
    "mise-a-jour-telechargement-sombre", Sombre, mise_a_jour_telechargement;
    "mise-a-jour-erreur-clair", Clair, mise_a_jour_erreur;
    "mise-a-jour-forcee-clair", Clair, mise_a_jour_forcee;
    "mise-a-jour-forcee-sombre", Sombre, mise_a_jour_forcee;
    "coquille-clair", Clair, coquille_salon;
    "coquille-sombre", Sombre, coquille_salon;
    "coquille-panneau-replie-clair", Clair, coquille_panneau_replie;
    "coquille-hors-ligne-clair", Clair, coquille_hors_ligne;
    "coquille-reconnexion-sombre", Sombre, coquille_reconnexion;
    "coquille-accueil-clair", Clair, coquille_accueil;
    "coquille-accueil-sombre", Sombre, coquille_accueil;
    "coquille-vocal-clair", Clair, coquille_vocal;
    "coquille-prive-sombre", Sombre, coquille_prive;
    "coquille-vide-clair", Clair, coquille_vide;
    "coquille-chargement-clair", Clair, connecte;
    "membres-clair", Clair, membres;
    "membres-sombre", Sombre, membres;
    "membres-filtre-clair", Clair, membres_filtre;
    "marge-epingles-clair", Clair, marge_epingles;
    "marge-epingles-sombre", Sombre, marge_epingles;
    "carte-profil-clair", Clair, carte_profil;
    "carte-profil-sombre", Sombre, carte_profil;
    "carte-profil-moi-clair", Clair, carte_profil_moi;
    "messagerie-variantes-clair", Clair, messagerie_variantes;
    "messagerie-variantes-sombre", Sombre, messagerie_variantes;
    "messagerie-lecteurs-clair", Clair, messagerie_lecteurs;
    "messagerie-lecteurs-sombre", Sombre, messagerie_lecteurs;
    "messagerie-markdown-clair", Clair, messagerie_markdown;
    "messagerie-markdown-sombre", Sombre, messagerie_markdown;
    "messages-prives-clair", Clair, messages_prives;
    "messages-prives-sombre", Sombre, messages_prives;
    "messagerie-emoji-clair", Clair, messagerie_emoji;
    "messagerie-emoji-sombre", Sombre, messagerie_emoji;
    "messagerie-pieces-clair", Clair, messagerie_pieces;
    "messagerie-pieces-sombre", Sombre, messagerie_pieces;
    "messagerie-visionneuse-clair", Clair, messagerie_visionneuse;
    "messagerie-cartes-clair", Clair, messagerie_cartes;
    "messagerie-cartes-sombre", Sombre, messagerie_cartes;
    "messagerie-sondages-clair", Clair, messagerie_sondages;
    "messagerie-sondages-sombre", Sombre, messagerie_sondages;
    "messagerie-defilement-clair", Clair, messagerie_defilement;
    "messagerie-defilement-sombre", Sombre, messagerie_defilement;
    "saisie-lignes-clair", Clair, saisie_lignes;
    "saisie-lignes-sombre", Sombre, saisie_lignes;
    "saisie-reponse-clair", Clair, saisie_reponse;
    "saisie-modification-sombre", Sombre, saisie_modification;
    "saisie-lecture-seule-clair", Clair, saisie_lecture_seule;
    "saisie-non-envoye-clair", Clair, saisie_non_envoye;
    "saisie-non-envoye-sombre", Sombre, saisie_non_envoye;
    "saisie-mentions-clair", Clair, saisie_mentions;
    "saisie-mentions-sombre", Sombre, saisie_mentions;
    "saisie-pieces-clair", Clair, saisie_pieces;
    "saisie-pieces-sombre", Sombre, saisie_pieces;
    "saisie-depot-clair", Clair, saisie_depot;
    "saisie-vocal-enregistrement-clair", Clair, saisie_vocal_enregistrement;
    "saisie-vocal-enregistrement-sombre", Sombre, saisie_vocal_enregistrement;
    "saisie-vocal-ecoute-clair", Clair, saisie_vocal_ecoute;
    "saisie-vocal-ecoute-sombre", Sombre, saisie_vocal_ecoute;
    "messagerie-vocal-clair", Clair, messagerie_vocal;
    "messagerie-vocal-sombre", Sombre, messagerie_vocal;
    "selecteur-emoji-clair", Clair, selecteur_emoji;
    "selecteur-emoji-sombre", Sombre, selecteur_emoji;
    "selecteur-sans-resultat-clair", Clair, selecteur_sans_resultat;
    "selecteur-gifs-clair", Clair, selecteur_gifs;
    "selecteur-gifs-sombre", Sombre, selecteur_gifs;
    "selecteur-gifs-limite-clair", Clair, selecteur_gifs_limite;
    "messagerie-actions-clair", Clair, messagerie_actions;
    "messagerie-actions-sombre", Sombre, messagerie_actions;
    "messagerie-menu-clair", Clair, messagerie_menu;
    "messagerie-menu-sombre", Sombre, messagerie_menu;
    "confirmation-clair", Clair, confirmation_suppression;
    "confirmation-sombre", Sombre, confirmation_suppression;
    "sondage-creation-clair", Clair, sondage_creation;
    "sondage-creation-sombre", Sombre, sondage_creation;
    "sondage-erreur-clair", Clair, sondage_erreur;
    "sondage-votants-clair", Clair, sondage_votants;
    "sondage-votants-sombre", Sombre, sondage_votants;
    "recherche-resultats-clair", Clair, recherche_resultats;
    "recherche-resultats-sombre", Sombre, recherche_resultats;
    "recherche-sans-resultat-clair", Clair, recherche_sans_resultat;
    "recherche-erreur-clair", Clair, recherche_erreur;
    "recherche-erreur-sombre", Sombre, recherche_erreur;
    "palette-clair", Clair, palette;
    "palette-sombre", Sombre, palette;
    "nouveau-prive-clair", Clair, nouveau_prive;
    "nouveau-prive-sombre", Sombre, nouveau_prive;
    "nouveau-prive-erreur-clair", Clair, nouveau_prive_erreur;
    "menu-statut-clair", Clair, menu_statut;
    "menu-statut-sombre", Sombre, menu_statut;
];

fn bob() -> Compte {
    Compte {
        id: "u1".into(),
        pseudo: "bob".into(),
    }
}

fn demonstration(f: &Principale) {
    f.set_demonstration(true);
    f.set_infobulle_forcee(true);
}

fn modal(f: &Principale) {
    demonstration(f);
    f.set_modal_demo_ouvert(true);
}

fn a_propos(f: &Principale) {
    coquille_accueil(f);
    f.set_a_propos_ouvert(true);
}

fn demarrage(f: &Principale) {
    afficher(f, &Etat::Demarrage);
}

fn connexion(f: &Principale) {
    afficher(f, &Etat::Deconnecte);
}

fn connexion_attente(f: &Principale) {
    afficher(f, &Etat::AttenteNavigateur);
}

fn connexion_erreur(f: &Principale) {
    afficher(
        f,
        &Etat::Erreur(entree::message_de_connexion(&ErreurAuth::DelaiDepasse)),
    );
}

fn verification(f: &Principale) {
    afficher(f, &Etat::Verification);
}

fn refus(f: &Principale) {
    afficher(f, &Etat::Refuse);
}

fn activation(f: &Principale) {
    afficher(
        f,
        &Etat::AActiver {
            erreur: String::new(),
            champ_en_faute: false,
            en_cours: false,
        },
    );
}

fn activation_erreur(f: &Principale) {
    afficher(
        f,
        &Etat::AActiver {
            erreur: "Ce pseudo est déjà pris.".into(),
            champ_en_faute: true,
            en_cours: false,
        },
    );
    f.set_activation_pseudo("bob".into());
}

fn activation_en_cours(f: &Principale) {
    afficher(
        f,
        &Etat::AActiver {
            erreur: String::new(),
            champ_en_faute: false,
            en_cours: true,
        },
    );
    f.set_activation_pseudo("bob_2".into());
}

fn invitation(f: &Principale) {
    afficher(
        f,
        &Etat::Invitation {
            erreur: String::new(),
            en_cours: false,
        },
    );
}

fn invitation_erreur(f: &Principale) {
    afficher(
        f,
        &Etat::Invitation {
            erreur: entree::INVITATION_INVALIDE.into(),
            en_cours: false,
        },
    );
    f.set_invitation_saisie("ABCDE-23456".into());
}

fn invitation_en_cours(f: &Principale) {
    afficher(
        f,
        &Etat::Invitation {
            erreur: String::new(),
            en_cours: true,
        },
    );
}

fn injoignable(f: &Principale) {
    afficher(
        f,
        &Etat::Injoignable("Connexion au serveur impossible.".into()),
    );
}

fn mise_a_jour(f: &Principale) {
    afficher(
        f,
        &Etat::MiseAJourRequise {
            message: "Une faille de sécurité a été corrigée : installe la nouvelle version.".into(),
            version_minimale: "0.2.0".into(),
        },
    );
}

fn version_proposee(imposee: bool) -> Option<MiseAJourDisponible> {
    Some(MiseAJourDisponible {
        version: "0.2.0".into(),
        notes: "Notifications du système avec « Répondre » et « Marquer comme lu », pastille de mentions sur la barre des tâches, icône dans la zone de notification et démarrage avec la session.".into(),
        imposee,
        ..MiseAJourDisponible::default()
    })
}

fn etat_maj(phase: Phase, disponible: Option<MiseAJourDisponible>) -> EtatMaj {
    EtatMaj {
        automatique: true,
        phase,
        disponible,
        ..EtatMaj::default()
    }
}

fn afficher_maj(f: &Principale, etat: &EtatMaj) {
    mise_a_jour::afficher(f, etat);
    let _ = f.window().take_snapshot();
    slint::platform::update_timers_and_animations();
}

fn mise_a_jour_bandeau(f: &Principale) {
    coquille_accueil(f);
    afficher_maj(f, &etat_maj(Phase::Disponible, version_proposee(false)));
}

fn mise_a_jour_panneau_a_jour(f: &Principale) {
    a_propos(f);
    afficher_maj(f, &etat_maj(Phase::AJour, None));
}

fn mise_a_jour_telechargement(f: &Principale) {
    a_propos(f);
    afficher_maj(
        f,
        &etat_maj(
            Phase::Telechargement {
                fait: 40,
                total: 100,
            },
            version_proposee(false),
        ),
    );
}

fn mise_a_jour_erreur(f: &Principale) {
    a_propos(f);
    afficher_maj(
        f,
        &EtatMaj {
            erreur: ErreurMiseAJour::Reseau("délai dépassé".into()).to_string(),
            ..etat_maj(Phase::Inactif, None)
        },
    );
}

fn mise_a_jour_forcee(f: &Principale) {
    mise_a_jour(f);
    afficher_maj(
        f,
        &etat_maj(
            Phase::Telechargement {
                fait: 62,
                total: 100,
            },
            version_proposee(true),
        ),
    );
}

fn connecte(f: &Principale) {
    afficher(
        f,
        &Etat::Connecte {
            compte: bob(),
            serveur: None,
        },
    );
}

fn runtime() -> tokio::runtime::Handle {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("runtime des captures")
        })
        .handle()
        .clone()
}

fn icone_du_club() -> Icone {
    let mut tampon = Icone::new(64, 64);
    for (i, pixel) in tampon.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % 64) as f32 - 31.5, (i / 64) as f32 - 31.5);
        let rayon = (x * x + y * y).sqrt();
        *pixel = if rayon < 14.0 {
            slint::Rgba8Pixel::new(246, 198, 214, 255)
        } else if (x + y).abs() < 6.0 {
            slint::Rgba8Pixel::new(208, 104, 138, 255)
        } else {
            slint::Rgba8Pixel::new(47, 93, 138, 255)
        };
    }
    tampon
}

fn salon(serveur: &str, salon: Option<&str>) -> Route {
    Route::Salon {
        serveur: serveur.into(),
        salon: salon.map(String::from),
        message: None,
    }
}

fn coquille(f: &Principale, etat: estompe_client::store::Etat, route: Route) -> Pont {
    coquille_avec(
        f,
        etat,
        route,
        Profils::nouveaux(Arc::new(commun::coquille::SourceFactice::default()), None),
    )
}

fn coquille_avec(
    f: &Principale,
    etat: estompe_client::store::Etat,
    route: Route,
    profils: Arc<Profils>,
) -> Pont {
    coquille_preparee(f, etat, route, profils, |_| {})
}

fn installer_conversations(store: &Store) {
    use commun::coquille::{conversation_courte, conversation_privee};
    for salon in ["c1", "c3", "t1"] {
        store.installer_conversation(
            &CleConversation::Salon(salon.into()),
            conversation_courte(salon),
            false,
        );
    }
    store.installer_conversation(
        &CleConversation::Prive("d1".into()),
        conversation_privee(),
        false,
    );
}

fn stabiliser(f: &Principale) {
    let mut precedent = None;
    for _ in 0..40 {
        let _ = f.window().take_snapshot();
        slint::platform::update_timers_and_animations();
        let m = f.global::<DonneesMessagerie>();
        let etat = (
            m.get_defilement(),
            m.get_haut(),
            m.get_bas(),
            m.get_lignes().row_count(),
        );
        if precedent == Some(etat) {
            break;
        }
        precedent = Some(etat);
    }
}

fn coquille_preparee(
    f: &Principale,
    etat: estompe_client::store::Etat,
    route: Route,
    profils: Arc<Profils>,
    preparer: impl FnOnce(&Store),
) -> Pont {
    coquille_complete(f, etat, route, profils, None, preparer)
}

const MAINTENANT_MS: i64 = 1_795_077_000_000;

fn coquille_complete(
    f: &Principale,
    etat: estompe_client::store::Etat,
    route: Route,
    profils: Arc<Profils>,
    medias: Option<Arc<Medias>>,
    preparer: impl FnOnce(&Store),
) -> Pont {
    coquille_complete_sur(f, etat, route, profils, medias, None, preparer)
}

fn coquille_complete_sur(
    f: &Principale,
    etat: estompe_client::store::Etat,
    route: Route,
    profils: Arc<Profils>,
    medias: Option<Arc<Medias>>,
    adresse: Option<&str>,
    preparer: impl FnOnce(&Store),
) -> Pont {
    connecte(f);
    let matin = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap();
    let store = match adresse {
        Some(adresse) => commun::coquille::store_sur(etat, adresse),
        None => commun::coquille::store(etat),
    };
    installer_conversations(&store);
    preparer(&store);
    let pont = Pont::nouveau(
        store,
        f.as_weak(),
        runtime(),
        None,
        Arc::new(move || matin),
        Navigation::nouvelle(route),
        profils,
    );
    pont.installer_icone(f, "s3", 2, icone_du_club());
    pont.definir_reseau(f, EtatReseau::Connecte);
    pont.definir_sections(
        f,
        vec![Section::Entracte, Section::Sillon, Section::Sablier],
    );
    pont.messagerie().definir_fuseau(Arc::new(|_| 60));
    pont.messagerie()
        .definir_horloge(Arc::new(|| MAINTENANT_MS));
    if let Some(medias) = medias {
        pont.messagerie().definir_medias(medias);
    }
    let source = pont.clone();
    brancher_messagerie(f, Arc::new(move || Some(source.clone())));
    pont.demarrer(f);
    stabiliser(f);
    pont
}

fn coquille_salon(f: &Principale) {
    coquille(f, commun::coquille::etat(), salon("s1", Some("c1")));
}

fn coquille_panneau_replie(f: &Principale) {
    coquille_salon(f);
    f.global::<DonneesCoquille>().set_panneau_replie(true);
}

fn coquille_hors_ligne(f: &Principale) {
    let pont = coquille(f, commun::coquille::etat(), salon("s2", None));
    pont.definir_reseau(f, EtatReseau::HorsLigne);
}

fn coquille_reconnexion(f: &Principale) {
    let pont = coquille(f, commun::coquille::etat(), salon("s1", Some("c3")));
    pont.definir_reseau(f, EtatReseau::Reconnexion);
}

fn coquille_accueil(f: &Principale) {
    coquille(f, commun::coquille::etat(), Route::Accueil);
}

fn coquille_vocal(f: &Principale) {
    coquille(f, commun::coquille::etat(), salon("s1", Some("v1")));
}

fn coquille_prive(f: &Principale) {
    coquille(
        f,
        commun::coquille::etat(),
        Route::MessagePrive {
            id: "d1".into(),
            message: None,
        },
    );
}

fn coquille_vide(f: &Principale) {
    let mut etat = commun::coquille::etat();
    etat.serveurs.clear();
    etat.details.clear();
    etat.messages_prives.clear();
    etat.mentions.clear();
    etat.non_lus.clear();
    etat.vocal.clear();
    let pont = coquille(f, etat, Route::Accueil);
    pont.definir_sections(f, Vec::new());
}

fn portrait(fond: [u8; 3], motif: [u8; 3]) -> Icone {
    let mut tampon = Icone::new(96, 96);
    for (i, pixel) in tampon.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % 96) as f32 - 47.5, (i / 96) as f32 - 47.5);
        let tete = (x * x + (y + 10.0) * (y + 10.0)).sqrt() < 20.0;
        let buste = y > 18.0 && (x * x + (y - 52.0) * (y - 52.0)).sqrt() < 40.0;
        let [r, v, b] = if tete || buste { motif } else { fond };
        *pixel = slint::Rgba8Pixel::new(r, v, b, 255);
    }
    tampon
}

fn profils_des_membres() -> Arc<Profils> {
    let profils = Profils::nouveaux(Arc::new(commun::coquille::SourceFactice::default()), None);
    for (utilisateur, version, fond, motif) in [
        ("eve", 2, [47, 93, 138], [246, 198, 214]),
        ("carol", 1, [242, 166, 90], [90, 50, 30]),
        ("gaelle", 4, [120, 160, 110], [250, 240, 220]),
    ] {
        profils.installer_avatar(
            CleAvatar {
                utilisateur: utilisateur.into(),
                version,
                serveur: None,
            },
            avatars::arrondir(portrait(fond, motif)),
        );
    }
    let badge = commun::coquille::badge;
    profils.installer_badges("elise", vec![badge("Fondatrice", "#d0688a")]);
    profils.installer_badges(
        "ines",
        vec![
            badge("Mélomane", "#2f5d8a"),
            badge("Cinéphile", "#b0413e"),
            badge("Bêta", "#3d8c5a"),
        ],
    );
    profils.installer_badges(
        "carol",
        vec![badge("Artiste", "#b0413e"), badge("Bêta", "#3d8c5a")],
    );
    profils
}

fn membres_ouverts(f: &Principale) -> Pont {
    f.global::<DonneesCoquille>().set_marge_ouverte(true);
    coquille_avec(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
    )
}

fn membres(f: &Principale) {
    membres_ouverts(f);
}

fn marge_epingles(f: &Principale) {
    let pont = coquille_preparee(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        |store| {
            installer_conversations(store);
            let mut epingles = commun::coquille::conversation_variantes();
            epingles.retain(|m| {
                m.id.as_deref()
                    .is_some_and(|id| ["100", "102", "107"].contains(&id))
            });
            for m in &mut epingles {
                m.pinned_at = Some("2026-11-19T10:00:00Z".into());
            }
            store.installer_epingles(&salon_c1(), epingles);
        },
    );
    pont.basculer_epingles(f);
    stabiliser(f);
}

fn membres_filtre(f: &Principale) {
    let pont = membres_ouverts(f);
    f.global::<DonneesCoquille>()
        .set_filtre_membres("car".into());
    pont.filtrer_membres(f, "car");
}

fn carte_profil(f: &Principale) {
    let pont = membres_ouverts(f);
    pont.ouvrir_membre(f, "carol");
}

fn carte_profil_moi(f: &Principale) {
    let pont = membres_ouverts(f);
    pont.ouvrir_membre(f, "u1");
}

fn salon_c1() -> CleConversation {
    CleConversation::Salon("c1".into())
}

fn messagerie_variantes(f: &Principale) {
    let pont = coquille_preparee(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        |store| {
            store.installer_conversation(
                &salon_c1(),
                commun::coquille::conversation_variantes(),
                false,
            )
        },
    );
    pont.appliquer(
        f,
        &[
            Changement::Messages(ChangementMessages::Supprime {
                conversation: salon_c1(),
                message: "99".into(),
            }),
            Changement::Messages(ChangementMessages::MiseEnEvidence {
                conversation: salon_c1(),
                message: "109".into(),
            }),
        ],
    );
    stabiliser(f);
}

fn messagerie_defilement(f: &Principale) {
    let _pont = coquille_preparee(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        |store| {
            let mut messages = Vec::new();
            for _ in 0..4 {
                messages.extend(commun::coquille::conversation_variantes());
            }
            for (n, m) in messages.iter_mut().enumerate() {
                m.id = Some((1000 + n).to_string());
            }
            store.installer_conversation(&salon_c1(), messages, false)
        },
    );
    stabiliser(f);
    f.window()
        .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(700.0, 500.0),
            delta_x: 0.0,
            delta_y: 120.0,
        });
    stabiliser(f);
}

fn messagerie_lecteurs(f: &Principale) {
    let pont = coquille_preparee(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        |store| {
            store.installer_conversation(
                &salon_c1(),
                commun::coquille::conversation_variantes(),
                false,
            );
            for (n, (id, pseudo)) in [
                ("carol", "carol"),
                ("dave", "dave"),
                ("eve", "Ève"),
                ("u4", "Hugo"),
                ("u5", "Inès"),
                ("u6", "Jade"),
                ("u7", "Karim"),
            ]
            .into_iter()
            .enumerate()
            {
                let signal = commun::coquille::evenement(
                    "read.updated",
                    serde_json::json!({"channel_id": "c1", "user_id": id, "pseudo": pseudo,
                        "last_id": if n < 6 { "109" } else { "107" },
                        "read_at": format!("2026-11-19T09:{:02}:00Z", 10 + n)}),
                );
                if let estompe_client::temps_reel::Signal::Evenement(e) = signal {
                    store.appliquer(&e);
                }
            }
        },
    );
    let _ = pont;
    stabiliser(f);
}

fn messagerie_markdown(f: &Principale) {
    let pont = coquille_preparee(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        |store| {
            store.installer_conversation(&salon_c1(), commun::coquille::galerie_markdown(), true)
        },
    );
    pont.messagerie().lien_clique(f, "201", "s:1");
    stabiliser(f);
}

fn messages_prives(f: &Principale) {
    let mut etat = commun::coquille::etat();
    etat.en_ligne.insert("dave".into());
    etat.statuts.insert(
        "dave".into(),
        serde_json::from_value(serde_json::json!("dnd")).unwrap(),
    );
    etat.non_lus.insert(CleConversation::Prive("d3".into()), 5);
    let pont = coquille_preparee(
        f,
        etat,
        Route::MessagePrive {
            id: "d1".into(),
            message: None,
        },
        Profils::nouveaux(Arc::new(commun::coquille::SourceFactice::default()), None),
        |store| {
            for (id, prive, auteur) in [("500", "d3", "elise"), ("501", "d2", "dave")] {
                let mut message = commun::coquille::message(id, "", auteur);
                message["channel_id"] = serde_json::Value::Null;
                message["dm_id"] = serde_json::json!(prive);
                if let estompe_client::temps_reel::Signal::Evenement(e) =
                    commun::coquille::evenement("message.created", message)
                {
                    store.appliquer(&e);
                }
            }
        },
    );
    lire_comme_a_l_ouverture(f, &pont, CleConversation::Prive("d1".into()), "303");
}

fn lire_comme_a_l_ouverture(f: &Principale, pont: &Pont, cle: CleConversation, dernier: &str) {
    let store = pont.store().clone();
    let mut abonne = store.abonner();
    let (champ, id) = match &cle {
        CleConversation::Prive(id) => ("dm_id", id.clone()),
        CleConversation::Salon(id) => ("channel_id", id.clone()),
    };
    let mut lecture = serde_json::json!({"user_id": "u1", "pseudo": "bob", "last_id": dernier, "read_at": "2026-11-19T08:30:00Z"});
    lecture[champ] = serde_json::json!(id);
    if let estompe_client::temps_reel::Signal::Evenement(e) =
        commun::coquille::evenement("read.updated", lecture)
    {
        store.appliquer(&e);
    }
    while let Ok(lot) = abonne.try_recv() {
        pont.appliquer(f, &lot);
    }
    stabiliser(f);
}

fn photo(largeur: u32, hauteur: u32, ciel: [u8; 3], sol: [u8; 3]) -> Vec<u8> {
    let image = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
        let (fx, fy) = (x as f32 / largeur as f32, y as f32 / hauteur as f32);
        let horizon = 0.62 + 0.05 * (fx * 9.0).sin();
        let soleil = ((fx - 0.72).powi(2) + (fy - 0.3).powi(2) * 0.6).sqrt() < 0.09;
        let melange = |a: [u8; 3], b: [u8; 3], t: f32| {
            image::Rgb([0, 1, 2].map(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t) as u8))
        };
        if soleil {
            image::Rgb([255, 214, 120])
        } else if fy < horizon {
            melange(ciel, [250, 236, 220], fy / horizon)
        } else {
            melange(sol, [40, 30, 25], (fy - horizon) / (1.0 - horizon))
        }
    });
    let mut octets = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut octets, image::ImageFormat::Png)
        .unwrap();
    octets.into_inner()
}

fn visage(fond: [u8; 3]) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(64, 64, |x, y| {
        let (x, y) = (x as f32 - 31.5, y as f32 - 33.5);
        let tete = (x * x + y * y).sqrt() < 24.0;
        let oreille = y < -10.0 && (x.abs() - 16.0).abs() < (y + 30.0) * 0.5 && y > -30.0;
        let oeil = ((x.abs() - 9.0).powi(2) + (y + 4.0).powi(2)).sqrt() < 4.0;
        let bouche = (y - 9.0).abs() < 1.5 && x.abs() < 7.0;
        if oeil || bouche {
            image::Rgba([40, 30, 25, 255])
        } else if tete || oreille {
            image::Rgba([fond[0], fond[1], fond[2], 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    let mut octets = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut octets, image::ImageFormat::Png)
        .unwrap();
    octets.into_inner()
}

fn installer(medias: &Medias, source: Source, format: Format, octets: &[u8]) {
    let (image, origine) = medias::decoder(octets, format, usize::MAX).expect("image décodée");
    medias.installer_image(CleImage { source, format }, image, origine);
}

fn medias_des_captures() -> Arc<Medias> {
    let medias = Medias::nouveaux(Arc::new(commun::coquille::SourceMediasFactice), None);
    let photos = [
        ("a1", photo(1600, 1066, [120, 170, 220], [214, 180, 120])),
        ("a2", photo(900, 1200, [240, 170, 120], [200, 150, 90])),
        ("a3", photo(1200, 800, [208, 104, 138], [60, 70, 90])),
        ("g1", photo(480, 360, [150, 210, 160], [90, 120, 80])),
    ];
    for (id, octets) in &photos {
        installer(
            &medias,
            Source::Fichier((*id).into()),
            Format::Vignette,
            octets,
        );
    }
    installer(
        &medias,
        Source::Fichier("a2".into()),
        Format::Visionneuse,
        &photos[1].1,
    );
    for (n, (largeur, hauteur, ciel, sol)) in apercus_de_gifs().into_iter().enumerate() {
        installer(
            &medias,
            Source::Externe(format!("https://media1.giphy.com/captures/{n}.gif")),
            Format::Gif,
            &photo(largeur, hauteur, ciel, sol),
        );
    }
    medias.installer_emojis(vec![
        EmojiPersonnalise {
            id: "e1".into(),
            server_id: "s1".into(),
            name: "chaton".into(),
            created_by: "eve".into(),
        },
        EmojiPersonnalise {
            id: "e2".into(),
            server_id: "s1".into(),
            name: "fete".into(),
            created_by: "eve".into(),
        },
    ]);
    installer(
        &medias,
        Source::Emoji("e1".into()),
        Format::Emoji,
        &visage([242, 166, 90]),
    );
    installer(
        &medias,
        Source::Emoji("e2".into()),
        Format::Emoji,
        &visage([208, 104, 138]),
    );
    let apercu = |valeur: serde_json::Value| -> Option<LinkPreview> {
        Some(serde_json::from_value(valeur).unwrap())
    };
    medias.installer_apercu(
        "https://plbls.fr/journal/estompe-natif",
        Some("700"),
        apercu(serde_json::json!({
            "url": "https://plbls.fr/journal/estompe-natif",
            "title": "Estompe natif : la messagerie sans webview",
            "description": "Comment l'app de bureau affiche emoji, images et sondages avec Slint, sans navigateur embarqué.",
            "site_name": "plbls.fr",
            "image": "https://plbls.fr/journal/estompe-natif.png"
        })),
    );
    installer(
        &medias,
        Source::Externe("https://plbls.fr/journal/estompe-natif.png".into()),
        Format::Carte,
        &photo(400, 300, [47, 93, 138], [246, 198, 214]),
    );
    medias.installer_apercu(
        "https://exemple.fr/article",
        Some("701"),
        apercu(serde_json::json!({
            "url": "https://exemple.fr/article",
            "title": "Un article sans illustration",
            "description": "La carte garde le site, le titre et la description sur deux lignes au plus.",
            "site_name": "exemple.fr"
        })),
    );
    medias.installer_apercu(
        "https://youtu.be/dQw4w9WgXcQ",
        Some("702"),
        apercu(serde_json::json!({
            "url": "https://youtu.be/dQw4w9WgXcQ",
            "title": "Rick Astley - Never Gonna Give You Up",
            "site_name": "YouTube"
        })),
    );
    installer(
        &medias,
        Source::Externe("https://img.youtube.com/vi/dQw4w9WgXcQ/hqdefault.jpg".into()),
        Format::Miniature,
        &photo(480, 360, [30, 30, 40], [180, 60, 60]),
    );
    medias
}

fn messagerie_avec(f: &Principale, messages: Vec<estompe_client::types::Message>) -> Pont {
    let pont = coquille_complete(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        Some(medias_des_captures()),
        move |store| store.installer_conversation(&salon_c1(), messages, false),
    );
    stabiliser(f);
    pont
}

fn messagerie_emoji(f: &Principale) {
    messagerie_avec(f, commun::coquille::conversation_emoji());
}

fn messagerie_pieces(f: &Principale) {
    let pont = messagerie_avec(f, commun::coquille::conversation_pieces());
    pont.messagerie()
        .noter_telechargement("f2", Telechargement::EnCours, 42, "");
    pont.messagerie().noter_telechargement(
        "f1",
        Telechargement::Termine,
        100,
        "Enregistré dans /home/bob/Téléchargements",
    );
    pont.messagerie().rafraichir_maintenant(f);
    stabiliser(f);
}

fn messagerie_vocal(f: &Principale) {
    let pont = messagerie_avec(f, commun::coquille::conversation_vocaux());
    let son: Arc<estompe_plateforme::SonSysteme> = Arc::new(estompe_plateforme::SonSysteme);
    pont.messagerie().definir_son(son.clone(), son);
    pont.messagerie()
        .definir_ecoutes(vec!["m2".into()], Arc::new(|_| {}));
    pont.messagerie().marquer_illisible("m3");
    pont.messagerie().rafraichir_maintenant(f);
    let v = f.global::<DonneesVocal>();
    v.set_piece("m2".into());
    v.set_en_lecture(true);
    v.set_avance(0.4);
    v.set_temps("0:01".into());
    v.set_vitesse("1,5×".into());
    stabiliser(f);
}

fn messagerie_visionneuse(f: &Principale) {
    let pont = messagerie_avec(f, commun::coquille::conversation_pieces());
    pont.messagerie().image_cliquee(f, "600", 1);
    pont.messagerie().visionneuse_zoom(f, 1.25);
    stabiliser(f);
}

fn messagerie_cartes(f: &Principale) {
    messagerie_avec(f, commun::coquille::conversation_cartes());
}

fn messagerie_sondages(f: &Principale) {
    messagerie_avec(f, commun::coquille::conversation_sondages());
}

fn sondage_rempli(f: &Principale, question: &str, choix: &[&str]) -> Pont {
    let pont = saisie_courte(f);
    pont.messagerie().ouvrir_creation_sondage(f);
    f.global::<DonneesCreationSondage>()
        .set_question(question.into());
    pont.messagerie().creation_modifiee(f);
    for _ in 2..choix.len() {
        pont.messagerie().ajouter_choix(f);
    }
    for (i, c) in choix.iter().enumerate() {
        pont.messagerie().choix_modifie(f, i as i32, c);
    }
    pont
}

fn sondage_creation(f: &Principale) {
    let pont = sondage_rempli(f, "On mange où vendredi ?", &["Pizza", "Sushi", ""]);
    f.global::<DonneesCreationSondage>().set_multiple(true);
    pont.messagerie().choisir_cloture(f, 2);
    stabiliser(f);
}

fn sondage_erreur(f: &Principale) {
    let pont = sondage_rempli(f, "Quel jour pour la sortie ?", &["Samedi", "Dimanche"]);
    pont.messagerie().choisir_cloture(f, 5);
    f.global::<DonneesCreationSondage>()
        .set_date("31/02/2026 18:00".into());
    pont.messagerie().publier_sondage(f);
    stabiliser(f);
}

fn sondage_votants(f: &Principale) {
    let pont = messagerie_avec(f, commun::coquille::conversation_sondages());
    let personnes = |noms: &[&str]| -> Vec<estompe_client::types::Person> {
        noms.iter()
            .map(|n| estompe_client::types::Person {
                id: Some(n.to_lowercase()),
                pseudo: Some(n.to_string()),
            })
            .collect()
    };
    pont.messagerie().afficher_votants(
        f,
        "704",
        [
            ("o1".to_string(), personnes(&["Bob", "Carol", "Dave"])),
            ("o2".to_string(), personnes(&["Ève"])),
            ("o3".to_string(), personnes(&["Inès", "Caro"])),
        ]
        .into(),
    );
    stabiliser(f);
}

fn recherche_lancee(f: &Principale, saisie: &str) -> (Pont, u64) {
    let pont = coquille(f, commun::coquille::etat(), salon("s1", Some("c1")));
    pont.messagerie().definir_fuseau(Arc::new(|_| 60));
    pont.ouvrir_recherche(f);
    f.global::<estompe_natif::DonneesRecherche>()
        .set_saisie(saisie.into());
    pont.recherche_modifiee(f, saisie);
    pont.lancer_recherche(f);
    let sequence = pont.recherche().unwrap().messages.sequence();
    (pont, sequence)
}

fn recherche_resultats(f: &Principale) {
    let (pont, sequence) = recherche_lancee(f, "réunion -demain de:carol");
    let page = serde_json::from_value(serde_json::json!({
        "messages": [
            {"id": "41", "channel_id": "c1", "author": {"id": "carol", "pseudo": "carol"}, "created_at": "2026-11-18T17:42:00Z", "content": "La réunion de samedi est déplacée à 15 h, pensez à prévenir ceux qui ne lisent pas ce salon.", "attachments": []},
            {"id": "38", "channel_id": "c1", "author": {"id": "carol", "pseudo": "carol"}, "created_at": "2026-11-12T09:05:00Z", "content": "Compte rendu des réunions du mois en pièce jointe.", "attachments": [{"id": "f1"}, {"id": "f2"}]},
            {"id": "12", "dm_id": "d1", "author": {"id": "carol", "pseudo": "carol"}, "created_at": "2026-11-02T20:15:00Z", "content": "Tu viens à la RÉUNION de jeudi ?", "attachments": []}
        ],
        "has_more": false
    }))
    .unwrap();
    pont.recherche_recue(f, sequence, Ok(page));
    stabiliser(f);
}

fn recherche_sans_resultat(f: &Principale) {
    let (pont, sequence) = recherche_lancee(f, "pique-nique");
    pont.recherche_recue(f, sequence, Ok(Default::default()));
    stabiliser(f);
}

fn recherche_erreur(f: &Principale) {
    let (pont, sequence) = recherche_lancee(f, "a:inconnu");
    let refus = estompe_client::http::ErreurApi::Autre(estompe_client::http::Refus {
        statut: 400,
        code: "invalid_query".into(),
        message: "Filtre « a: » inconnu.".into(),
        donnees: Default::default(),
    });
    pont.recherche_recue(f, sequence, Err(refus));
    stabiliser(f);
}

fn palette(f: &Principale) {
    let pont = coquille(f, commun::coquille::etat(), salon("s1", Some("c1")));
    pont.ouvrir_recherche(f);
    f.global::<estompe_natif::DonneesRecherche>()
        .set_saisie("a".into());
    pont.recherche_modifiee(f, "a");
    pont.naviguer_recherche(f, 1);
    pont.naviguer_recherche(f, 1);
    pont.naviguer_recherche(f, 1);
    pont.naviguer_recherche(f, 1);
    stabiliser(f);
}

fn nouveau_prive_saisi(f: &Principale, saisie: &str) -> Pont {
    let pont = coquille(f, commun::coquille::etat(), Route::Accueil);
    pont.ouvrir_nouveau_prive(f);
    let g = f.global::<estompe_natif::DonneesNouveauPrive>();
    g.set_saisie(saisie.into());
    g.set_curseur(saisie.len() as i32);
    g.set_demande_curseur(g.get_demande_curseur() + 1);
    pont.nouveau_prive_modifie(f, saisie);
    pont
}

fn nouveau_prive(f: &Principale) {
    let pont = nouveau_prive_saisi(f, "e");
    pont.naviguer_nouveau_prive(f, 1);
    stabiliser(f);
}

fn nouveau_prive_erreur(f: &Principale) {
    let pont = nouveau_prive_saisi(f, "zoe");
    let refus = estompe_client::http::ErreurApi::Introuvable(estompe_client::http::Refus {
        statut: 404,
        code: "user_not_found".into(),
        message: "introuvable".into(),
        donnees: Default::default(),
    });
    pont.nouveau_prive_recu(f, Err(refus));
    stabiliser(f);
}

fn menu_statut(f: &Principale) {
    let mut etat = commun::coquille::etat();
    etat.statuts.insert(
        "u1".into(),
        estompe_client::temps_reel::evenements::StatutPresence::Dnd,
    );
    let pont = coquille(f, etat, Route::Accueil);
    let barre = i_slint_backend_testing::ElementHandle::find_by_element_type_name(f, "BoutonMoi")
        .next()
        .expect("barre de l'utilisateur");
    let position = barre.absolute_position();
    pont.menu_moi(
        f,
        estompe_natif::menu::Ancre::Element {
            x: position.x,
            haut: position.y,
            bas: position.y + barre.size().height,
        },
    );
    stabiliser(f);
}

fn remettre_a_zero(f: &Principale) {
    estompe_natif::menu::fermer(f);
    f.global::<DonneesCreationSondage>().set_ouvert(false);
    f.global::<estompe_natif::DonneesGifs>().set_ouvert(false);
    f.global::<estompe_natif::DonneesRecherche>()
        .set_ouvert(false);
    f.global::<estompe_natif::DonneesNouveauPrive>()
        .set_ouvert(false);
    estompe_natif::confirmation::repondre(f, false);
    f.global::<DonneesSaisie>().set_depot_survole(false);
    let v = f.global::<DonneesVocal>();
    v.set_possible(false);
    v.set_etape(EtapeVocal::Aucune);
    v.set_piece("".into());
    v.set_en_lecture(false);
    v.set_avance(0.0);
    v.set_vitesse("1×".into());
    f.window()
        .dispatch_event(slint::platform::WindowEvent::PointerExited);
    f.set_demonstration(false);
    f.set_infobulle_forcee(false);
    f.set_modal_demo_ouvert(false);
    f.set_a_propos_ouvert(false);
    vider_modeles(f);
    f.global::<DonneesCoquille>().set_panneau_replie(false);
    f.global::<DonneesCoquille>().set_marge_ouverte(false);
    connecte(f);
    f.set_activation_pseudo("".into());
    f.set_invitation_saisie("".into());
    mise_a_jour::afficher(f, &EtatMaj::default());
}

fn references() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/references")
}

fn sorties() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("captures")
}

fn rendre(f: &Principale, ecran: &Ecran) -> image::RgbaImage {
    remettre_a_zero(f);
    f.global::<Theme>().set_mode(ecran.theme);
    (ecran.preparer)(f);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    let capture = f.window().take_snapshot().expect("capture de la fenêtre");
    image::RgbaImage::from_raw(
        capture.width(),
        capture.height(),
        capture.as_bytes().to_vec(),
    )
    .expect("tampon de la capture")
}

fn comparer(obtenu: &image::RgbaImage, reference: &image::RgbaImage) -> (f64, image::RgbaImage) {
    let mut difference = image::RgbaImage::new(obtenu.width(), obtenu.height());
    if obtenu.dimensions() != reference.dimensions() {
        return (1.0, difference);
    }
    let mut ecarts = 0u64;
    for ((o, r), d) in obtenu
        .pixels()
        .zip(reference.pixels())
        .zip(difference.pixels_mut())
    {
        let ecart =
            o.0.iter()
                .zip(r.0)
                .any(|(a, b)| a.abs_diff(b) > ECART_PAR_CANAL);
        let gris = ((o.0[0] as u32 + o.0[1] as u32 + o.0[2] as u32) / 3 / 4 + 190) as u8;
        *d = if ecart {
            ecarts += 1;
            image::Rgba([220, 0, 60, 255])
        } else {
            image::Rgba([gris, gris, gris, 255])
        };
    }
    let total = u64::from(obtenu.width()) * u64::from(obtenu.height());
    (ecarts as f64 / total as f64, difference)
}

#[test]
fn les_ecrans_correspondent_a_leurs_references() {
    let consigne = std::env::var(VARIABLE).unwrap_or_default();
    if consigne == "ignorer" {
        eprintln!("ignoré : {VARIABLE}=ignorer");
        return;
    }
    let regenerer = consigne == "regenerer";
    slint::platform::set_platform(Box::new(TestingBackend::new(TestingBackendOptions {
        mock_time: true,
        threading: false,
        renderer_name: Some("software".into()),
    })))
    .expect("backend de test");
    let f = creer_fenetre().expect("fenêtre");
    f.set_version("0.0.0".into());
    f.set_debogage(true);
    f.global::<Mouvement>().set_reduit(true);
    f.window()
        .set_size(slint::PhysicalSize::new(LARGEUR, HAUTEUR));
    f.show().expect("affichage");

    let mut echecs = Vec::new();
    for ecran in ECRANS {
        let obtenu = rendre(&f, ecran);
        let chemin = references().join(format!("{}.png", ecran.nom));
        if regenerer {
            std::fs::create_dir_all(references()).expect("dossier des références");
            obtenu.save(&chemin).expect("écriture de la référence");
            eprintln!("référence écrite : {}", chemin.display());
            continue;
        }
        let Ok(reference) = image::open(&chemin).map(|i| i.into_rgba8()) else {
            echecs.push(format!(
                "{} : référence absente ({})",
                ecran.nom,
                chemin.display()
            ));
            continue;
        };
        let (part, difference) = comparer(&obtenu, &reference);
        if part > PART_TOLEREE {
            let dossier = sorties();
            std::fs::create_dir_all(&dossier).expect("dossier des sorties");
            obtenu
                .save(dossier.join(format!("{}.obtenu.png", ecran.nom)))
                .expect("écriture de la capture");
            reference
                .save(dossier.join(format!("{}.reference.png", ecran.nom)))
                .expect("copie de la référence");
            difference
                .save(dossier.join(format!("{}.difference.png", ecran.nom)))
                .expect("écriture de la différence");
            echecs.push(format!(
                "{} : {:.3} % de pixels différents (seuil {:.3} %), images dans {}",
                ecran.nom,
                part * 100.0,
                PART_TOLEREE * 100.0,
                dossier.display()
            ));
        }
    }
    assert!(
        echecs.is_empty(),
        "captures différentes de leur référence :\n{}\nSi le changement est voulu : {VARIABLE}=regenerer cargo test --test captures",
        echecs.join("\n")
    );
}

#[test]
fn la_comparaison_tolere_le_bruit_et_signale_un_changement_de_couleur() {
    let reference = image::RgbaImage::from_pixel(100, 100, image::Rgba([208, 104, 138, 255]));
    let mut bruit = reference.clone();
    for p in bruit.pixels_mut() {
        p.0[0] += 8;
    }
    assert_eq!(comparer(&bruit, &reference).0, 0.0);
    let mut change = reference.clone();
    for (x, _, p) in change.enumerate_pixels_mut() {
        if x < 10 {
            *p = image::Rgba([47, 93, 138, 255]);
        }
    }
    let (part, difference) = comparer(&change, &reference);
    assert!((part - 0.1).abs() < 1e-9);
    assert_eq!(difference.get_pixel(0, 0).0, [220, 0, 60, 255]);
    let autre_taille = image::RgbaImage::new(50, 100);
    assert_eq!(comparer(&autre_taille, &reference).0, 1.0);
}

fn survoler_le_message(f: &Principale, texte: &str) -> (f32, f32) {
    let e = i_slint_backend_testing::ElementHandle::find_by_element_type_name(f, "CorpsMessage")
        .find(|e| e.accessible_description().is_some_and(|d| d == texte))
        .expect("message affiché");
    let p = e.absolute_position();
    let position = slint::LogicalPosition::new(p.x + 200.0, p.y + e.size().height / 2.0);
    f.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    (position.x, position.y)
}

fn messagerie_actions(f: &Principale) {
    let _pont = saisie_courte(f);
    stabiliser(f);
    survoler_le_message(f, "Salut, prêt pour la réunion de 10 h ?");
    stabiliser(f);
}

fn messagerie_menu(f: &Principale) {
    let pont = saisie_courte(f);
    stabiliser(f);
    let (x, y) = survoler_le_message(f, "Salut, prêt pour la réunion de 10 h ?");
    pont.messagerie()
        .ouvrir_menu_message(f, "401", estompe_natif::menu::Ancre::Point { x, y });
    stabiliser(f);
}

fn confirmation_suppression(f: &Principale) {
    let pont = saisie_courte(f);
    stabiliser(f);
    pont.messagerie().action_message(f, "401", "supprimer");
    stabiliser(f);
}

fn saisie_courte(f: &Principale) -> Pont {
    messagerie_avec(f, commun::coquille::conversation_courte("c1"))
}

fn saisie_lignes(f: &Principale) {
    let pont = saisie_courte(f);
    let texte = "Une première ligne\nune deuxième\net une troisième, assez longue pour montrer que le champ grandit avec son contenu.";
    f.global::<DonneesSaisie>().set_texte(texte.into());
    pont.messagerie().saisie_modifiee(f, texte);
    stabiliser(f);
}

fn saisie_reponse(f: &Principale) {
    let pont = saisie_courte(f);
    let id = pont
        .store()
        .conversation(&salon_c1())
        .and_then(|c| c.messages.first().and_then(|m| m.id.clone()))
        .unwrap();
    pont.messagerie().repondre_a(f, &id);
    stabiliser(f);
}

fn saisie_modification(f: &Principale) {
    let pont = saisie_courte(f);
    let id = pont
        .store()
        .conversation(&salon_c1())
        .and_then(|c| c.messages.last().and_then(|m| m.id.clone()))
        .unwrap();
    pont.messagerie().modifier_message(f, &id);
    stabiliser(f);
}

fn saisie_lecture_seule(f: &Principale) {
    let mut etat = commun::coquille::etat_avec_membres();
    if let Some(detail) = etat.details.get_mut("s1") {
        for salon in detail.channels.iter_mut().flatten() {
            if salon.id.as_deref() == Some("c1") {
                salon.permissions = Some(vec!["view_channel".into()]);
            }
        }
    }
    coquille_complete(
        f,
        etat,
        salon("s1", Some("c1")),
        profils_des_membres(),
        Some(medias_des_captures()),
        |store| {
            store.installer_conversation(
                &salon_c1(),
                commun::coquille::conversation_courte("c1"),
                false,
            )
        },
    );
    stabiliser(f);
}

fn saisie_non_envoye(f: &Principale) {
    let serveur = commun::coquille::ServeurMessages::demarrer(10);
    *serveur.refus_d_envoi.lock().unwrap() = Some(429);
    let adresse = serveur.adresse.clone();
    coquille_complete_sur(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils_des_membres(),
        Some(medias_des_captures()),
        Some(&adresse),
        |store| {
            store.installer_conversation(
                &salon_c1(),
                commun::coquille::conversation_courte("c1"),
                false,
            );
            store.definir_horloge(Arc::new(|| MAINTENANT_MS));
            let reseau = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            reseau.block_on(store.envoyer(
                &salon_c1(),
                estompe_client::store::messages::Brouillon::texte(
                    "Ce message a été refusé par le serveur.",
                ),
            ));
        },
    );
    stabiliser(f);
}

fn saisie_vocal_enregistrement(f: &Principale) {
    saisie_courte(f);
    let v = f.global::<DonneesVocal>();
    v.set_etape(EtapeVocal::Enregistrement);
    v.set_duree("0:12".into());
    let niveaux: Vec<f32> = commun::coquille::forme_de_test(40)
        .into_iter()
        .map(|n| f32::from(n) / 100.0)
        .collect();
    v.set_niveaux(slint::ModelRc::new(slint::VecModel::from(niveaux)));
    stabiliser(f);
}

fn saisie_vocal_ecoute(f: &Principale) {
    saisie_courte(f);
    let v = f.global::<DonneesVocal>();
    v.set_etape(EtapeVocal::Ecoute);
    v.set_duree("0:12".into());
    v.set_forme(slint::ModelRc::new(slint::VecModel::from(
        estompe_natif::messagerie::pont::barres(&commun::coquille::forme_de_test(40)),
    )));
    v.set_piece("apercu".into());
    v.set_en_lecture(true);
    v.set_avance(0.35);
    v.set_temps("0:04".into());
    stabiliser(f);
}

fn saisie_mentions(f: &Principale) {
    let pont = saisie_courte(f);
    let texte = "On se retrouve ce soir avec @e";
    pont.messagerie().saisie().placer(f, texte, texte.len());
    pont.messagerie().saisie_modifiee(f, texte);
    stabiliser(f);
}

fn saisie_pieces(f: &Principale) {
    let pont = saisie_courte(f);
    let saisie = pont.messagerie().saisie();
    let limites = estompe_client::fichiers::Limites::default();
    let cles = saisie.avec_etat(|e| {
        let cles: Vec<u64> = (0..3).map(|_| e.nouvelle_piece()).collect();
        let liste = e.liste_courante().unwrap();
        for (cle, nom) in cles
            .iter()
            .zip(["photo.png", "rapport-annuel.pdf", "archive.zip"])
        {
            liste.ajouter(*cle, nom, &limites).unwrap();
        }
        liste.terminer(
            cles[0],
            Ok(estompe_client::types::Attachment {
                id: Some("f1".into()),
                name: Some("photo.png".into()),
                size: Some(48_213),
                ..Default::default()
            }),
        );
        liste.progression(cles[1], 420_000, 1_000_000);
        liste.terminer(cles[2], Err("Stockage plein".into()));
        cles
    });
    let image = image::load_from_memory(&commun::coquille::png([200, 40, 90])).unwrap();
    let reduite = estompe_natif::messagerie::medias::reduire(
        image,
        estompe_natif::messagerie::medias::Format::Piece,
    );
    saisie.definir_vignette(
        cles[0],
        slint::SharedPixelBuffer::clone_from_slice(
            reduite.as_raw(),
            reduite.width(),
            reduite.height(),
        ),
    );
    let texte = "Les documents de samedi";
    saisie.placer(f, texte, texte.len());
    pont.messagerie().saisie_modifiee(f, texte);
    saisie.afficher_pieces(f);
    stabiliser(f);
}

fn saisie_depot(f: &Principale) {
    let _pont = saisie_courte(f);
    f.global::<DonneesSaisie>().set_depot_survole(true);
    stabiliser(f);
}

fn selecteur_ouvert(f: &Principale) -> Pont {
    let pont = saisie_courte(f);
    pont.messagerie().ouvrir_selecteur(f, Cible::Saisie);
    pont
}

fn selecteur_emoji(f: &Principale) {
    let pont = selecteur_ouvert(f);
    pont.messagerie().selecteur().deplacer(f, 0, 1);
    pont.messagerie().selecteur().deplacer(f, 1, 0);
    stabiliser(f);
}

fn selecteur_sans_resultat(f: &Principale) {
    let pont = selecteur_ouvert(f);
    f.global::<DonneesSelecteur>()
        .set_recherche("zzzzzz".into());
    pont.messagerie().selecteur().chercher(f, "zzzzzz");
    stabiliser(f);
}

fn apercus_de_gifs() -> Vec<(u32, u32, [u8; 3], [u8; 3])> {
    vec![
        (200, 150, [246, 198, 214], [208, 104, 138]),
        (150, 200, [120, 170, 220], [47, 93, 138]),
        (200, 112, [242, 166, 90], [120, 70, 40]),
        (200, 200, [150, 210, 160], [90, 120, 80]),
        (200, 120, [200, 190, 240], [100, 80, 160]),
        (160, 200, [240, 220, 140], [180, 140, 60]),
        (200, 150, [180, 220, 230], [60, 120, 140]),
        (200, 180, [230, 160, 160], [140, 60, 60]),
    ]
}

fn gifs_ouverts(f: &Principale) -> (Pont, u64) {
    let pont = saisie_courte(f);
    pont.messagerie().ouvrir_gifs(f);
    let sequence = pont.messagerie().gifs().etat().recherche.sequence();
    (pont, sequence)
}

fn selecteur_gifs(f: &Principale) {
    let (pont, sequence) = gifs_ouverts(f);
    let titres = [
        "Bravo", "Merci", "Fou rire", "Ok", "Coucou", "Fête", "Pouce", "Bisou",
    ];
    let gifs = apercus_de_gifs()
        .into_iter()
        .enumerate()
        .map(|(n, (largeur, hauteur, _, _))| estompe_client::gifs::Gif {
            id: format!("g{n}"),
            title: titres[n].into(),
            preview: format!("https://media1.giphy.com/captures/{n}.gif"),
            url: format!("https://media1.giphy.com/captures/{n}/giphy.gif"),
            width: largeur,
            height: hauteur,
        })
        .collect();
    pont.messagerie().recevoir_gifs(f, sequence, Ok(gifs));
    pont.messagerie().gifs().deplacer(f, 0, 1);
    stabiliser(f);
}

fn selecteur_gifs_limite(f: &Principale) {
    let (pont, sequence) = gifs_ouverts(f);
    let refus = estompe_client::http::ErreurApi::Autre(estompe_client::http::Refus {
        statut: 429,
        code: "too_many_requests".into(),
        message: String::new(),
        donnees: Default::default(),
    });
    pont.messagerie().recevoir_gifs(f, sequence, Err(refus));
    stabiliser(f);
}
