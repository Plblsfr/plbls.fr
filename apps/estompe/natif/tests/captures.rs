mod commun;

use commun::references::{comparer, controler, VARIABLE};

use chrono::NaiveDate;
use estompe_client::appels::EmojiPersonnalise;
use estompe_client::auth::ErreurAuth;
use estompe_client::mise_a_jour::{ErreurMiseAJour, EtatMaj, Phase};
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::messages::ChangementMessages;
use estompe_client::store::{Changement, CleConversation, ParticipantVocal, Store};
use estompe_client::temps_reel::EtatReseau;
use estompe_client::types::LinkPreview;
use estompe_client::version::MiseAJourDisponible;
use estompe_client::vocal::{
    CleVideo, Compositeur, EtatVocal, FormeIncrustation, ImagePartage, InfoParticipant, Mire,
    PartageLocal, Phase as PhaseVocal, Qualite, QualitePartage, ReglagesIncrustation,
    ResolutionPartage, TrameVideo,
};
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
    creer_fenetre, DonneesCoquille, DonneesCreationSondage, DonneesMessageVocal, DonneesMessagerie,
    DonneesSaisie, DonneesSelecteur, EtapeVocal, ModeTheme, Mouvement, Principale, Theme,
};
use estompe_plateforme::{Image, SourceCapture, TypeSource};
use i_slint_backend_testing::{TestingBackend, TestingBackendOptions};
use slint::{ComponentHandle, Model};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const LARGEUR: u32 = 1180;
const HAUTEUR: u32 = 900;

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
    "jeux-overlay-clair", Clair, jeux_overlay;
    "jeux-overlay-sombre", Sombre, jeux_overlay;
    "jeux-replay-clair", Clair, jeux_replay;
    "jeux-replay-sombre", Sombre, jeux_replay;
    "jeux-replay-indisponible-clair", Clair, jeux_replay_indisponible;
    "jeux-replay-stockage-clair", Clair, jeux_replay_stockage;
    "mesures-vide-clair", Clair, mesures_vide;
    "mesures-scenario-clair", Clair, mesures_scenario;
    "mesures-scenario-sombre", Sombre, mesures_scenario;
    "clips-vide-clair", Clair, clips_vide;
    "clips-remplie-clair", Clair, clips_remplie;
    "clips-remplie-sombre", Sombre, clips_remplie;
    "clips-quota-clair", Clair, clips_quota;
    "clips-envoi-clair", Clair, clips_envoi;
    "clips-montages-clair", Clair, clips_montages;
    "editeur-clair", Clair, editeur;
    "editeur-sombre", Sombre, editeur;
    "editeur-export-clair", Clair, editeur_export;
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
    "vocal-1-clair", Clair, vocal_seul;
    "vocal-4-sombre", Sombre, vocal_quatre;
    "vocal-4-clair", Clair, vocal_quatre;
    "vocal-video-clair", Clair, vocal_video;
    "vocal-video-sombre", Sombre, vocal_video;
    "vocal-camera-apercu-clair", Clair, vocal_camera_apercu;
    "vocal-camera-apercu-sombre", Sombre, vocal_camera_apercu;
    "vocal-camera-erreur-clair", Clair, vocal_camera_erreur;
    "vocal-camera-active-sombre", Sombre, vocal_camera_active;
    "vocal-12-clair", Clair, vocal_douze;
    "vocal-2-clair", Clair, vocal_deux;
    "vocal-6-clair", Clair, vocal_six;
    "vocal-partage-attente-clair", Clair, vocal_partage_attente;
    "vocal-partage-clair", Clair, vocal_partage;
    "vocal-partage-sombre", Sombre, vocal_partage;
    "vocal-partage-plein-ecran-clair", Clair, vocal_partage_plein_ecran;
    "vocal-partage-spectateurs-clair", Clair, vocal_partage_spectateurs;
    "vocal-partage-spectateurs-sombre", Sombre, vocal_partage_spectateurs;
    "vocal-partage-sources-clair", Clair, vocal_partage_sources;
    "vocal-partage-sources-sombre", Sombre, vocal_partage_sources;
    "vocal-partage-aucune-source-clair", Clair, vocal_partage_aucune_source;
    "vocal-partage-apercu-clair", Clair, vocal_partage_apercu;
    "vocal-partage-apercu-sombre", Sombre, vocal_partage_apercu;
    "vocal-partage-incrustation-clair", Clair, vocal_partage_incrustation;
    "vocal-partage-incrustation-sombre", Sombre, vocal_partage_incrustation;
    "vocal-partage-en-cours-clair", Clair, vocal_partage_en_cours;
    "vocal-partage-en-cours-sombre", Sombre, vocal_partage_en_cours;
    "vocal-reconnexion-sombre", Sombre, vocal_reconnexion;
    "vocal-menu-clair", Clair, vocal_menu;
    "vocal-menu-pointeur-clair", Clair, vocal_menu_pointeur;
    "vocal-menu-pointeur-sombre", Sombre, vocal_menu_pointeur;
    "vocal-moderation-clair", Clair, vocal_moderation;
    "vocal-moderation-sombre", Sombre, vocal_moderation;
    "vocal-plein-cadre-clair", Clair, vocal_plein_cadre;
    "vocal-plein-cadre-sombre", Sombre, vocal_plein_cadre;
    "vocal-sons-clair", Clair, vocal_sons;
    "vocal-sons-perso-clair", Clair, vocal_sons_perso;
    "vocal-sons-perso-sombre", Sombre, vocal_sons_perso;
    "vocal-glisser-clair", Clair, vocal_glisser;
    "vocal-glisser-sombre", Sombre, vocal_glisser;
    "vocal-glisser-refus-clair", Clair, vocal_glisser_refus;
    "vocal-reglages-clair", Clair, vocal_reglages;
    "vocal-reglages-sombre", Sombre, vocal_reglages;
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
    "carte-profil-badges-clair", Clair, carte_profil_badges;
    "carte-profil-badges-sombre", Sombre, carte_profil_badges;
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
    "menu-sourdine-clair", Clair, menu_sourdine;
    "menu-sourdine-sombre", Sombre, menu_sourdine;
    "menu-sourdine-duree-clair", Clair, menu_sourdine_duree;
    "parametres-compte-clair", Clair, parametres_compte;
    "parametres-compte-sombre", Sombre, parametres_compte;
    "parametres-compte-profil-clair", Clair, parametres_compte_profil;
    "parametres-compte-profil-sombre", Sombre, parametres_compte_profil;
    "recadrage-avatar-clair", Clair, recadrage_avatar;
    "recadrage-avatar-sombre", Sombre, recadrage_avatar;
    "creer-rejoindre-clair", Clair, creer_rejoindre;
    "creer-rejoindre-sombre", Sombre, creer_rejoindre;
    "creation-salon-clair", Clair, creation_salon;
    "creation-salon-sombre", Sombre, creation_salon;
    "creation-salon-erreur-clair", Clair, creation_salon_erreur;
    "reglages-serveur-vue-clair", Clair, reglages_serveur_vue;
    "reglages-serveur-vue-sombre", Sombre, reglages_serveur_vue;
    "reglages-serveur-membre-clair", Clair, reglages_serveur_membre;
    "reglages-serveur-invitations-clair", Clair, reglages_serveur_invitations;
    "reglages-serveur-emojis-clair", Clair, reglages_serveur_emojis;
    "reglages-serveur-emojis-sombre", Sombre, reglages_serveur_emojis;
    "reglages-serveur-membres-clair", Clair, reglages_serveur_membres;
    "reglages-serveur-bannis-clair", Clair, reglages_serveur_bannis;
    "reglages-serveur-roles-clair", Clair, reglages_serveur_roles;
    "parametres-notifications-clair", Clair, parametres_notifications;
    "parametres-notifications-serveur-clair", Clair, parametres_notifications_serveur;
    "parametres-notifications-serveur-sombre", Sombre, parametres_notifications_serveur;
    "parametres-voix-clair", Clair, parametres_voix;
    "parametres-apparence-clair", Clair, parametres_apparence;
    "parametres-apparence-sombre", Sombre, parametres_apparence;
    "parametres-application-clair", Clair, parametres_application;
    "parametres-debogage-clair", Clair, parametres_debogage;
    "parametres-debogage-sombre", Sombre, parametres_debogage;
    "parametres-signalements-clair", Clair, parametres_signalements;
    "parametres-raccourcis-clair", Clair, parametres_raccourcis;
    "parametres-replay-clair", Clair, parametres_replay;
    "parametres-recherche-clair", Clair, parametres_recherche;
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

fn jeux_overlay(f: &Principale) {
    coquille_accueil(f);
    let j = f.global::<estompe_natif::DonneesJeux>();
    j.set_disponible(true);
    j.set_jeu_en_cours("Counter-Strike 2".into());
    let o = f.global::<estompe_natif::DonneesOverlay>();
    o.set_disponible(true);
    o.set_actif(true);
    o.set_en_jeu(true);
    o.set_coin("tr".into());
    o.set_taille(1);
    o.set_parlent(false);
    o.set_opacite(80);
    f.global::<estompe_natif::DonneesReplay>()
        .set_resume("Désactivé.".into());
    j.set_ouvert(true);
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

const DEBUT_DU_VOCAL: &str = "2026-11-19T08:47:00Z";

const PRENOMS: [&str; 11] = [
    "carol", "dave", "Élise", "farid", "gaëlle", "hugo", "inès", "jules", "karim", "léa", "malo",
];

fn etat_vocal(salon: &str, autres: &[&str], preparer: impl FnOnce(&mut EtatVocal)) -> EtatVocal {
    let mut v = EtatVocal::default();
    v.rejoindre(salon);
    let mut infos = vec![InfoParticipant {
        identite: "u1".into(),
        nom: "bob".into(),
        local: true,
        qualite: Qualite::Excellente,
        ..InfoParticipant::default()
    }];
    infos.extend(autres.iter().map(|id| InfoParticipant {
        identite: id.to_string(),
        nom: id.to_string(),
        qualite: Qualite::Bonne,
        ..InfoParticipant::default()
    }));
    v.connecte("u1".into(), infos);
    preparer(&mut v);
    v
}

fn attribut(v: &mut EtatVocal, identite: &str, cle: &str) {
    if let Some(p) = v.participants.get_mut(identite) {
        p.attributs.insert(cle.to_string(), "1".to_string());
    }
}

fn profils_du_vocal() -> Arc<Profils> {
    let profils = Profils::nouveaux(Arc::new(commun::coquille::SourceFactice::default()), None);
    for (utilisateur, fond, motif) in [
        ("carol", [242, 166, 90], [90, 50, 30]),
        ("Élise", [120, 160, 110], [250, 240, 220]),
    ] {
        profils.installer_avatar(
            CleAvatar {
                utilisateur: utilisateur.into(),
                version: 1,
                serveur: None,
            },
            avatars::arrondir(portrait(fond, motif)),
        );
    }
    profils
}

fn vocal_avec(
    f: &Principale,
    nombre: usize,
    preparer_store: impl FnOnce(&mut estompe_client::store::Etat),
    preparer: impl FnOnce(&mut EtatVocal),
) -> Pont {
    let autres: Vec<&str> = PRENOMS
        .iter()
        .take(nombre.saturating_sub(1))
        .copied()
        .collect();
    let mut etat = commun::coquille::etat();
    etat.vocal.insert(
        "v1".into(),
        autres
            .iter()
            .map(|id| ParticipantVocal {
                utilisateur: id.to_string(),
                pseudo: id.to_string(),
                depuis: Some(DEBUT_DU_VOCAL.into()),
                ..ParticipantVocal::default()
            })
            .collect(),
    );
    for utilisateur in ["carol", "Élise"] {
        etat.avatars.insert(utilisateur.into(), 1);
    }
    preparer_store(&mut etat);
    f.global::<estompe_natif::DonneesVocal>()
        .set_menu_au_pointeur(false);
    let pont = coquille_avec(f, etat, salon("s1", Some("v1")), profils_du_vocal());
    pont.vocal()
        .definir_etat(f, etat_vocal("v1", &autres, preparer));
    stabiliser(f);
    pont
}

fn vocal_seul(f: &Principale) {
    vocal_avec(f, 1, |_| {}, |_| {});
}

fn vocal_quatre(f: &Principale) {
    vocal_avec(
        f,
        4,
        |_| {},
        |v| {
            v.parlent.insert("carol".into());
            attribut(v, "dave", "muted");
            attribut(v, "Élise", "deaf");
            if let Some(p) = v.participants.get_mut("Élise") {
                p.qualite = Qualite::Faible;
            }
        },
    );
}

fn vocal_video(f: &Principale) {
    let pont = vocal_avec(
        f,
        4,
        |_| {},
        |v| {
            v.videos.insert(CleVideo::camera("carol"));
            v.parlent.insert("carol".into());
        },
    );
    let mire = Mire::nouvelle(1280, 720).image(40);
    pont.vocal()
        .poser_image_video(f, &CleVideo::camera("carol"), &mire);
    stabiliser(f);
}

fn cameras_du_poste() -> Vec<estompe_plateforme::InfoCamera> {
    [("c1", "Caméra intégrée"), ("c2", "Webcam USB HD")]
        .into_iter()
        .map(|(id, nom)| estompe_plateforme::InfoCamera {
            id: id.into(),
            nom: nom.into(),
        })
        .collect()
}

fn vocal_camera_apercu(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |v| v.camera = Some("c2".into()));
    pont.vocal().ouvrir_camera(f);
    pont.vocal().presenter_camera(f, cameras_du_poste(), None);
    let mire = Mire::nouvelle(1280, 720).image(25);
    pont.vocal()
        .poser_image_video(f, &estompe_natif::vocal::pont::cle_apercu(), &mire);
    stabiliser(f);
}

fn vocal_camera_erreur(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_camera(f);
    pont.vocal().presenter_camera(
        f,
        cameras_du_poste(),
        Some(&estompe_plateforme::ErreurCamera::Occupee),
    );
    stabiliser(f);
}

fn vocal_camera_active(f: &Principale) {
    let pont = vocal_avec(
        f,
        2,
        |_| {},
        |v| {
            v.camera_active = true;
            v.camera_erreur = None;
        },
    );
    let mire = Mire::nouvelle(1280, 720).image(60);
    pont.vocal()
        .poser_image_video(f, &CleVideo::camera("u1"), &mire);
    stabiliser(f);
}

fn vocal_douze(f: &Principale) {
    vocal_avec(
        f,
        12,
        |e| {
            if let Some(liste) = e.vocal.get_mut("v1") {
                liste[3].micro_coupe = true;
                liste[4].son_coupe = true;
            }
        },
        |v| {
            v.basculer_micro();
            v.parlent.insert("hugo".into());
            attribut(v, "jules", "muted");
        },
    );
}

fn vocal_deux(f: &Principale) {
    vocal_avec(
        f,
        2,
        |_| {},
        |v| {
            v.parlent.insert("carol".into());
        },
    );
}

fn vocal_six(f: &Principale) {
    let pont = vocal_avec(
        f,
        6,
        |_| {},
        |v| {
            v.videos.insert(CleVideo::camera("dave"));
            attribut(v, "farid", "muted");
        },
    );
    let mire = Mire::nouvelle(1280, 720).image(10);
    pont.vocal()
        .poser_image_video(f, &CleVideo::camera("dave"), &mire);
    stabiliser(f);
}

fn avec_partage(v: &mut EtatVocal, regarde: bool) {
    let cle = CleVideo::partage("carol");
    v.videos.insert(cle.clone());
    v.pistes.insert(cle, "TR_carol_ecran".into());
    v.sons_partage.insert("carol".into());
    if regarde {
        v.regardes.insert("carol".into(), true);
        v.vus.insert("TR_carol_ecran".into());
    }
    for identite in ["dave", "Élise"] {
        if let Some(p) = v.participants.get_mut(identite) {
            p.attributs
                .insert("watching".to_string(), "TR_carol_ecran".to_string());
        }
    }
}

fn partage_affiche(f: &Principale, pont: &Pont) {
    let mire = Mire::nouvelle(1920, 1080).image(30);
    pont.vocal()
        .poser_image_video(f, &CleVideo::partage("carol"), &mire);
    stabiliser(f);
}

fn vocal_partage_attente(f: &Principale) {
    vocal_avec(f, 4, |_| {}, |v| avec_partage(v, false));
}

fn vocal_partage(f: &Principale) {
    let pont = vocal_avec(
        f,
        4,
        |_| {},
        |v| {
            avec_partage(v, true);
            v.parlent.insert("dave".into());
        },
    );
    partage_affiche(f, &pont);
}

fn vocal_partage_plein_ecran(f: &Principale) {
    let pont = vocal_avec(f, 4, |_| {}, |v| avec_partage(v, true));
    pont.vocal().plein_ecran(f, "carol#screen");
    partage_affiche(f, &pont);
}

fn vocal_partage_spectateurs(f: &Principale) {
    let pont = vocal_avec(f, 4, |_| {}, |v| avec_partage(v, true));
    partage_affiche(f, &pont);
    pont.vocal().ouvrir_menu_partage(f, "carol");
    stabiliser(f);
}

fn source_du_poste(id: &str, nom: &str, largeur: u32, hauteur: u32) -> SourceCapture {
    SourceCapture {
        id: id.into(),
        nom: nom.into(),
        application: if id.starts_with("fenetre") {
            "Éditeur".into()
        } else {
            String::new()
        },
        genre: if id.starts_with("fenetre") {
            TypeSource::Fenetre
        } else {
            TypeSource::Ecran
        },
        largeur,
        hauteur,
        principale: id == "ecran:0",
        processus: None,
        estompe: false,
    }
}

fn sources_du_poste() -> (Vec<SourceCapture>, Vec<(String, Image)>) {
    let sources = vec![
        source_du_poste("ecran:0", "Écran 1", 2560, 1440),
        source_du_poste("ecran:1", "Écran 2", 1920, 1080),
        source_du_poste("fenetre:7", "Notes de réunion", 1280, 800),
    ];
    let miniatures = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mire = Mire::nouvelle(320, 180).image(i as u64 * 7);
            let mut rgba = vec![0u8; 320 * 180 * 4];
            mire.vers_rgba(320, 180, &mut rgba);
            (
                s.id.clone(),
                Image {
                    largeur: 320,
                    hauteur: 180,
                    rgba,
                },
            )
        })
        .collect();
    (sources, miniatures)
}

fn vocal_partage_sources(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_partage(f);
    let (sources, miniatures) = sources_du_poste();
    pont.vocal().presenter_partage(f, sources, miniatures, None);
    stabiliser(f);
}

fn vocal_partage_aucune_source(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_partage(f);
    pont.vocal().onglet_partage(f, true);
    pont.vocal().presenter_partage(
        f,
        vec![source_du_poste("ecran:0", "Écran 1", 2560, 1440)],
        Vec::new(),
        Some("Impossible de lister les écrans et fenêtres."),
    );
    stabiliser(f);
}

fn vocal_partage_apercu(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_partage(f);
    let (sources, miniatures) = sources_du_poste();
    pont.vocal().presenter_partage(f, sources, miniatures, None);
    pont.vocal().choisir_source(f, "ecran:1");
    let mire = Mire::nouvelle(1920, 1080).image(12);
    pont.vocal()
        .poser_image_video(f, &estompe_natif::vocal::pont::cle_apercu_partage(), &mire);
    stabiliser(f);
}

fn vocal_partage_incrustation(f: &Principale) {
    let reglages = ReglagesIncrustation {
        active: true,
        x: 0,
        forme: FormeIncrustation::Cercle,
        bordure: true,
        ..ReglagesIncrustation::default()
    };
    let pont = vocal_avec(
        f,
        2,
        |_| {},
        |v| {
            v.camera_active = true;
            v.reglages_incrustation = reglages;
        },
    );
    pont.vocal().ouvrir_partage(f);
    let (sources, miniatures) = sources_du_poste();
    pont.vocal().presenter_partage(f, sources, miniatures, None);
    pont.vocal().choisir_source(f, "ecran:1");
    let compositeur = Compositeur::nouveau(reglages);
    compositeur.camera(Some(Arc::new(Mire::nouvelle(640, 360).image(3))));
    let mire = Mire::nouvelle(1920, 1080).image(12);
    let mut ecran = Image {
        largeur: 1920,
        hauteur: 1080,
        rgba: vec![0u8; 1920 * 1080 * 4],
    };
    mire.vers_rgba(1920, 1080, &mut ecran.rgba);
    compositeur.incruster_apercu(&mut ecran);
    pont.vocal().poser_image_video(
        f,
        &estompe_natif::vocal::pont::cle_apercu_partage(),
        &ImagePartage(ecran),
    );
    stabiliser(f);
}

fn vocal_partage_en_cours(f: &Principale) {
    let pont = vocal_avec(
        f,
        3,
        |_| {},
        |v| {
            let source = source_du_poste("fenetre:7", "Notes de réunion", 1280, 800);
            v.partage = Some(PartageLocal {
                source,
                qualite: QualitePartage {
                    resolution: ResolutionPartage::R720,
                    ips: 30,
                },
                reduite: false,
                son: false,
            });
            let cle = CleVideo::partage("u1");
            v.pistes.insert(cle, "TR_u1_ecran".into());
            if let Some(p) = v.participants.get_mut("carol") {
                p.attributs
                    .insert("watching".to_string(), "TR_u1_ecran".to_string());
            }
        },
    );
    let mire = Mire::nouvelle(1280, 800).image(5);
    pont.vocal()
        .poser_image_video(f, &CleVideo::partage("u1"), &mire);
    stabiliser(f);
}

fn vocal_reconnexion(f: &Principale) {
    vocal_avec(f, 3, |_| {}, |v| v.phase = PhaseVocal::Reconnexion);
}

fn vocal_menu(f: &Principale) {
    let pont = vocal_avec(f, 4, |_| {}, |_| {});
    pont.vocal().ouvrir_menu(f, "carol", "v1");
    stabiliser(f);
}

fn vocal_menu_pointeur(f: &Principale) {
    let pont = vocal_avec(f, 4, |_| {}, |_| {});
    let v = f.global::<estompe_natif::DonneesVocal>();
    v.set_menu_au_pointeur(true);
    v.set_menu_x(1100.0);
    v.set_menu_y(700.0);
    pont.vocal().ouvrir_menu(f, "carol", "v1");
    stabiliser(f);
}

fn vocal_moderation(f: &Principale) {
    let pont = vocal_avec(
        f,
        4,
        |e| {
            if let Some(serveur) = e.details.get_mut("s1").and_then(|d| d.server.as_mut()) {
                serveur.owner_id = Some("u1".into());
            }
        },
        |_| {},
    );
    pont.vocal().ouvrir_menu(f, "carol", "v1");
    stabiliser(f);
}

fn vocal_plein_cadre(f: &Principale) {
    let pont = vocal_avec(
        f,
        4,
        |_| {},
        |v| {
            v.parlent.insert("Élise".into());
        },
    );
    pont.vocal().basculer_plein_cadre(f, true);
    stabiliser(f);
}

fn vocal_sons(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_reglages(f);
    stabiliser(f);
    f.global::<estompe_natif::DonneesVocal>()
        .set_reglages_decalage(-500.0);
    stabiliser(f);
}

fn vocal_sons_perso(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_reglages(f);
    stabiliser(f);
    let v = f.global::<estompe_natif::DonneesVocal>();
    let mut r = v.get_reglages();
    r.perso_arrivee = true;
    r.son_arrivee = 6;
    v.set_reglages(r);
    v.set_son_erreur("Choisis un fichier audio de 150 Ko au plus.".into());
    v.set_reglages_decalage(-560.0);
    stabiliser(f);
}

fn vocal_moderateur(f: &Principale) -> Pont {
    vocal_avec(
        f,
        4,
        |e| {
            if let Some(serveur) = e.details.get_mut("s1").and_then(|d| d.server.as_mut()) {
                serveur.owner_id = Some("u1".into());
            }
        },
        |_| {},
    )
}

fn vocal_glisser(f: &Principale) {
    vocal_moderateur(f);
    f.global::<estompe_natif::DonneesVocal>()
        .set_glisse_depuis("v1".into());
    stabiliser(f);
}

fn vocal_glisser_refus(f: &Principale) {
    vocal_moderateur(f);
    let v = f.global::<estompe_natif::DonneesVocal>();
    v.set_erreur_depot_salon("k1".into());
    v.set_erreur_depot("Action impossible : Rang insuffisant".into());
    stabiliser(f);
}

fn vocal_reglages(f: &Principale) {
    let pont = vocal_avec(f, 2, |_| {}, |_| {});
    pont.vocal().ouvrir_reglages(f);
    stabiliser(f);
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

fn carte_profil_badges(f: &Principale) {
    let profils = profils_des_membres();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11" fill="#2f5d8a"/><path d="M7 12l3 3 7-7" stroke="#ffffff" stroke-width="2.5" fill="none"/></svg>"##;
    let etoile = image::RgbaImage::from_fn(48, 48, |x, y| {
        let (dx, dy) = (x as f32 - 23.5, y as f32 - 23.5);
        if dx.abs() + dy.abs() < 22.0 {
            image::Rgba([242, 166, 90, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    etoile.write_to(&mut png, image::ImageFormat::Png).unwrap();
    profils.installer_icone_badge("verifie", 1, svg.to_vec());
    profils.installer_icone_badge("pionniere", 2, png.into_inner());
    let badge = |id: &str, nom: &str, description: &str, icone: &str, version: i64| {
        estompe_client::appels::Badge {
            id: id.into(),
            name: nom.into(),
            description: description.into(),
            icon: icone.into(),
            icon_version: version,
            color: "#d0688a".into(),
            ..Default::default()
        }
    };
    profils.installer_badges(
        "carol",
        vec![
            badge(
                "verifie",
                "Vérifiée",
                "Compte vérifié par l'équipe d'Estompe.",
                "",
                1,
            ),
            badge("pionniere", "Pionnière", "", "", 2),
            badge("artiste", "Artiste", "", "🎨", 0),
        ],
    );
    f.global::<DonneesCoquille>().set_marge_ouverte(true);
    let pont = coquille_avec(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
        profils,
    );
    pont.ouvrir_membre(f, "carol");
    stabiliser(f);
    let badge = i_slint_backend_testing::ElementHandle::find_by_element_type_name(f, "BadgeCarte")
        .next()
        .expect("badge de la carte");
    let p = badge.absolute_position();
    let taille = badge.size();
    f.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(
                p.x + taille.width / 2.0,
                p.y + taille.height / 2.0,
            ),
        });
    stabiliser(f);
}

fn etat_profil() -> estompe_client::store::Etat {
    let mut etat = commun::coquille::etat_avec_membres();
    etat.avatars.insert("u1".into(), 3);
    etat.avatars_serveur
        .insert("s2".into(), [("u1".to_string(), 2)].into());
    if let Some(m) = etat
        .details
        .get_mut("s1")
        .and_then(|d| d.members.as_mut())
        .and_then(|m| m.iter_mut().find(|m| m.id.as_deref() == Some("u1")))
    {
        m.banner_color = Some("#2f5d8a".into());
    }
    etat
}

fn profils_du_compte() -> Arc<Profils> {
    let profils = profils_des_membres();
    for (serveur, version, fond, motif) in [
        (None, 3, [208, 104, 138], [250, 230, 236]),
        (Some("s2"), 2, [61, 140, 90], [230, 245, 235]),
    ] {
        profils.installer_avatar(
            CleAvatar {
                utilisateur: "u1".into(),
                version,
                serveur: serveur.map(str::to_string),
            },
            avatars::arrondir(portrait(fond, motif)),
        );
    }
    profils
}

fn parametres_compte_profil(f: &Principale) {
    let pont = coquille_avec(f, etat_profil(), Route::Accueil, profils_du_compte());
    afficher_parametres(f, estompe_client::parametres::Onglet::Compte);
    pont.projeter_profil(f);
    stabiliser(f);
}

fn paysage() -> image::RgbaImage {
    image::RgbaImage::from_fn(900, 600, |x, y| {
        let ciel = y < 330;
        let soleil = ((x as f32 - 620.0).powi(2) + (y as f32 - 190.0).powi(2)).sqrt() < 70.0;
        let colline = (y as f32) > 330.0 + 60.0 * ((x as f32) / 140.0).sin();
        let [r, v, b] = if soleil {
            [242, 166, 90]
        } else if ciel {
            [(150 + y / 6) as u8, (190 + y / 8) as u8, 230]
        } else if colline {
            [61, 140, 90]
        } else {
            [120, 170, 110]
        };
        image::Rgba([r, v, b, 255])
    })
}

fn recadrage_avatar(f: &Principale) {
    let pont = coquille_avec(f, etat_profil(), Route::Accueil, profils_du_compte());
    afficher_parametres(f, estompe_client::parametres::Onglet::Compte);
    pont.projeter_profil(f);
    pont.ouvrir_recadrage(f, None, paysage());
    pont.zoomer_recadrage(f, 1.6);
    pont.deplacer_recadrage(f, -60.0, 20.0);
    stabiliser(f);
}

fn creer_rejoindre(f: &Principale) {
    let pont = coquille(f, commun::coquille::etat(), Route::Accueil);
    pont.ouvrir_creer_rejoindre(f);
    let g = f.global::<estompe_natif::DonneesCreerRejoindre>();
    g.set_nom("Atelier vélo".into());
    pont.nom_de_serveur_modifie(f, "Atelier vélo");
    let carre = image::imageops::resize(
        &image::imageops::crop_imm(&paysage(), 300, 0, 600, 600).to_image(),
        256,
        256,
        image::imageops::FilterType::Triangle,
    );
    pont.icone_recadree(f, Some(("image/png", Vec::new(), carre)));
    g.set_code("ABCDE-23456".into());
    pont.code_modifie(f, "ABCDE-23456");
    stabiliser(f);
}

fn creation_salon_ouverte(f: &Principale) -> Pont {
    let pont = coquille(
        f,
        commun::coquille::etat_avec_membres(),
        salon("s1", Some("c1")),
    );
    pont.ouvrir_creation_salon(f, "s1", estompe_client::store::GenreSalon::Text, None);
    pont
}

fn creation_salon(f: &Principale) {
    let pont = creation_salon_ouverte(f);
    f.global::<estompe_natif::DonneesCreationSalon>()
        .set_nom("jeux du soir".into());
    pont.nom_de_salon_modifie(f, "jeux du soir");
    pont.basculer_salon_prive(f, true);
    pont.basculer_role_du_salon(f, 1);
    pont.basculer_membre_du_salon(f, 1);
    stabiliser(f);
}

fn reglages_serveur(
    f: &Principale,
    etat: estompe_client::store::Etat,
    serveur: &str,
    onglet: &str,
) -> Pont {
    let pont = coquille_complete(
        f,
        etat,
        salon(serveur, None),
        Profils::nouveaux(Arc::new(commun::coquille::SourceFactice::default()), None),
        Some(medias_des_captures()),
        |_| {},
    );
    pont.ouvrir_reglages_serveur(f, serveur);
    pont.choisir_onglet_serveur(f, onglet);
    pont
}

fn reglages_serveur_vue(f: &Principale) {
    let pont = reglages_serveur(f, commun::coquille::etat_avec_membres(), "s1", "vue");
    f.global::<estompe_natif::DonneesReglagesServeur>()
        .set_nom("Les copains".into());
    pont.nom_des_reglages_modifie(f, "Les copains");
    let carre = image::imageops::resize(
        &image::imageops::crop_imm(&paysage(), 300, 0, 600, 600).to_image(),
        256,
        256,
        image::imageops::FilterType::Triangle,
    );
    pont.icone_des_reglages_recadree(f, Some(("image/png", Vec::new(), carre)));
    stabiliser(f);
}

fn reglages_serveur_membre(f: &Principale) {
    let mut etat = commun::coquille::etat_avec_membres();
    etat.details.get_mut("s2").unwrap().permissions =
        Some(serde_json::from_value(serde_json::json!(["view_channel"])).unwrap());
    reglages_serveur(f, etat, "s2", "vue");
    stabiliser(f);
}

fn reglages_serveur_invitations(f: &Principale) {
    let pont = reglages_serveur(
        f,
        commun::coquille::etat_avec_membres(),
        "s1",
        "invitations",
    );
    pont.choisir_expiration(f, 3);
    pont.choisir_utilisation(f, 2);
    pont.poser_invitations(
        f,
        "s1",
        serde_json::from_value(serde_json::json!([
            {"code": "ABCDE-23456", "created_by": "bob", "uses": 3},
            {"code": "FGHIJ-78901", "created_by": "Ève", "uses": 1, "max_uses": 10, "expires_at": "2026-11-20T09:30:00Z"}
        ]))
        .unwrap(),
    );
    stabiliser(f);
}

fn reglages_serveur_emojis(f: &Principale) {
    let pont = reglages_serveur(f, commun::coquille::etat_avec_membres(), "s1", "emojis");
    pont.emoji_choisi(
        f,
        estompe_natif::coquille::pont::emoji_depuis(&visage([120, 170, 220]), "Chat bleu.png"),
    );
    f.global::<estompe_natif::DonneesReglagesServeur>()
        .set_nom_emoji("chat_bleu".into());
    stabiliser(f);
}

fn reglages_serveur_membres(f: &Principale) {
    reglages_serveur(f, commun::coquille::etat_avec_membres(), "s1", "membres");
    stabiliser(f);
}

fn reglages_serveur_bannis(f: &Principale) {
    let pont = reglages_serveur(f, commun::coquille::etat_avec_membres(), "s1", "bannis");
    pont.poser_bannis(
        f,
        "s1",
        serde_json::from_value(serde_json::json!([
            {"user_id": "zed", "pseudo": "zed", "by": "bob", "at": "2026-11-18T10:00:00Z", "reason": "Spam répété dans #général"},
            {"user_id": "yan", "pseudo": "Yan", "by": "Ève", "at": "2026-11-17T21:15:00Z", "expires_at": "2026-11-24T21:15:00Z"}
        ]))
        .unwrap(),
    );
    stabiliser(f);
}

fn reglages_serveur_roles(f: &Principale) {
    reglages_serveur(f, commun::coquille::etat_avec_membres(), "s1", "roles");
    stabiliser(f);
}

fn creation_salon_erreur(f: &Principale) {
    let pont = creation_salon_ouverte(f);
    pont.choisir_genre_de_salon(f, 1);
    pont.choisir_genre_de_salon(f, 0);
    f.global::<estompe_natif::DonneesCreationSalon>()
        .set_nom("Général".into());
    pont.nom_de_salon_modifie(f, "Général");
    pont.choisir_mode_lent(f, 4);
    f.global::<estompe_natif::DonneesCreationSalon>()
        .set_avances(true);
    stabiliser(f);
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
    let son = Arc::new(estompe_plateforme::SonPeripheriques::par_defaut(Arc::new(
        estompe_plateforme::AudioSysteme,
    )));
    pont.messagerie().definir_son(son.clone(), son);
    pont.messagerie()
        .definir_ecoutes(vec!["m2".into()], Arc::new(|_| {}));
    pont.messagerie().marquer_illisible("m3");
    pont.messagerie().rafraichir_maintenant(f);
    let v = f.global::<DonneesMessageVocal>();
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

fn ancre_du_salon(f: &Principale, nom: &str) -> estompe_natif::menu::Ancre {
    let salon = i_slint_backend_testing::ElementHandle::find_by_accessible_label(f, nom)
        .next()
        .expect("salon du panneau");
    let position = salon.absolute_position();
    estompe_natif::menu::Ancre::Point {
        x: position.x + 40.0,
        y: position.y + salon.size().height,
    }
}

fn menu_sourdine(f: &Principale) {
    let pont = coquille(f, etat_notifications(), salon("s1", Some("c1")));
    let ancre = ancre_du_salon(f, "organisation-des-vacances-d-ete");
    pont.menu_salon(f, "c3", ancre);
    stabiliser(f);
}

fn menu_sourdine_duree(f: &Principale) {
    let pont = coquille(f, commun::coquille::etat(), salon("s1", Some("c1")));
    let ancre = ancre_du_salon(f, "photos");
    pont.menu_salon(f, "c4", ancre);
    estompe_natif::menu::choisir(f, "sourdine");
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
    let v = f.global::<DonneesMessageVocal>();
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
    let jeux = f.global::<estompe_natif::DonneesJeux>();
    jeux.set_ouvert(false);
    jeux.set_disponible(false);
    jeux.set_jeu_en_cours("".into());
    f.global::<estompe_natif::DonneesOverlay>()
        .set_disponible(false);
    let replay = f.global::<estompe_natif::DonneesReplay>();
    replay.set_ouvert(false);
    replay.set_materiel_lu(false);
    replay.set_disponible(false);
    replay.set_actif(false);
    replay.set_tourne(false);
    replay.set_decalage(0.0);
    replay.set_plafond("".into());
    let clips = f.global::<estompe_natif::DonneesClips>();
    clips.set_ouvert(false);
    clips.set_envoi_ouvert(false);
    clips.set_visible(false);
    clips.set_envoi_disponible(false);
    clips.set_stockage_lu(false);
    clips.set_presque(false);
    clips.set_depasse(false);
    clips.set_alerte("".into());
    clips.set_vide_reglages(false);
    clips.set_clips(slint::ModelRc::default());
    clips.set_jeux(slint::ModelRc::default());
    clips.set_travaux(slint::ModelRc::default());
    clips.set_decalage(0.0);
    clips.set_modifier_disponible(false);
    let editeur = f.global::<estompe_natif::DonneesEditeur>();
    editeur.set_ouvert(false);
    editeur.set_pret(false);
    editeur.set_export_ouvert(false);
    editeur.set_export_en_cours(false);
    editeur.set_choix_clip(false);
    editeur.set_message("".into());
    editeur.set_projets(slint::ModelRc::default());
    let mesures = f.global::<estompe_natif::DonneesMesures>();
    mesures.set_ouvert(false);
    mesures.set_disponible(true);
    mesures.set_en_session(false);
    mesures.set_peut_exporter(false);
    mesures.set_scenarios(slint::ModelRc::default());
    mesures.set_scenario_en_cours("".into());
    mesures.set_aide_en_cours("".into());
    mesures.set_reste(0);
    mesures.set_message("".into());
    mesures.set_erreur("".into());
    vider_modeles(f);
    f.global::<estompe_natif::DonneesOverlayMessages>()
        .set_disponible(false);
    let p = f.global::<estompe_natif::DonneesParametres>();
    p.set_recherche("".into());
    p.set_annonce("".into());
    p.set_message_journal("".into());
    p.set_lignes(slint::ModelRc::default());
    p.set_total_lignes(0);
    p.set_journal_actif(false);
    p.set_type_salon(true);
    p.set_demarrage_disponible(false);
    p.set_serveur_notif("".into());
    f.global::<DonneesCoquille>().set_panneau_replie(false);
    f.global::<DonneesCoquille>().set_marge_ouverte(false);
    connecte(f);
    f.set_activation_pseudo("".into());
    f.set_invitation_saisie("".into());
    f.global::<estompe_natif::DonneesVocal>()
        .set_menu_ouvert(false);
    f.global::<estompe_natif::DonneesVocal>()
        .set_reglages_ouverts(false);
    mise_a_jour::afficher(f, &EtatMaj::default());
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
    f.global::<estompe_natif::DonneesParametres>()
        .set_version("0.0.0".into());
    f.set_debogage(true);
    f.global::<Mouvement>().set_reduit(true);
    f.window()
        .set_size(slint::PhysicalSize::new(LARGEUR, HAUTEUR));
    f.show().expect("affichage");

    let mut echecs = Vec::new();
    for ecran in ECRANS {
        let obtenu = rendre(&f, ecran);
        if let Err(echec) = controler(ecran.nom, &obtenu, regenerer) {
            echecs.push(echec);
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
    let v = f.global::<DonneesMessageVocal>();
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
    let v = f.global::<DonneesMessageVocal>();
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

fn jeux_replay(f: &Principale) {
    jeux_overlay(f);
    let r = f.global::<estompe_natif::DonneesReplay>();
    let choix = |libelles: &[&str], choisi: usize, grises: &[usize]| {
        slint::ModelRc::new(slint::VecModel::from(
            libelles
                .iter()
                .enumerate()
                .map(|(i, l)| estompe_natif::ChoixReplay {
                    libelle: (*l).into(),
                    disponible: !grises.contains(&i),
                    choisi: i == choisi,
                })
                .collect::<Vec<_>>(),
        ))
    };
    r.set_materiel_lu(true);
    r.set_disponible(true);
    r.set_actif(true);
    r.set_tourne(true);
    r.set_etat(
        "Le tampon tourne sur Counter-Strike 2 : H.264, 1920 × 1080, 60 i/s. Encodeur : AMD AMF."
            .into(),
    );
    r.set_resume(r.get_etat());
    r.set_touche("F10".into());
    r.set_resolutions(choix(
        &["720p", "1080p", "1440p", "4K", "Native (celle du jeu)"],
        1,
        &[2, 3],
    ));
    r.set_cadences(choix(&["30", "60", "120", "144"], 1, &[2, 3]));
    r.set_qualite(2);
    r.set_duree_max(300);
    r.set_duree(90);
    r.set_codec(
        "Codec utilisé : H.264, environ 12 Mbit/s ; mémoire du tampon : environ 200 Mo.".into(),
    );
    r.set_jeux("Jeux ajoutés à la main : 1 jeu, exclus : 0 jeu. Ils se règlent dans « Jeux et activité », pour la détection comme pour l'instant replay.".into());
    r.set_dossier(r"C:\Users\alice\Videos\Estompe".into());
    r.set_dossier_defaut(true);
    r.set_quota(20);
    r.set_ouvert(true);
}

fn jeux_replay_indisponible(f: &Principale) {
    jeux_overlay(f);
    let r = f.global::<estompe_natif::DonneesReplay>();
    r.set_materiel_lu(true);
    r.set_disponible(false);
    r.set_raison("instant replay disponible sous Windows uniquement".into());
    r.set_etat("Désactivé.".into());
    r.set_dossier("/home/alice/Vidéos/Estompe".into());
    r.set_ouvert(true);
}

fn jeux_replay_stockage(f: &Principale) {
    jeux_replay(f);
    let r = f.global::<estompe_natif::DonneesReplay>();
    r.set_plafond(
        "Durée plafonnée à 1 min 30 au lieu de 5 min : mémoire libre insuffisante (1.0 Go libres)."
            .into(),
    );
    r.set_dossier_defaut(false);
    r.set_dossier(r"D:\Clips".into());
    r.set_decalage(-600.0);
}

fn vignette(teinte: u8) -> slint::Image {
    let mut tampon = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(160, 90);
    for (i, p) in tampon.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % 160) as u8, (i / 160) as u8);
        *p = slint::Rgba8Pixel::new(teinte, x, y.saturating_mul(2), 255);
    }
    slint::Image::from_rgba8(tampon)
}

fn ligne_clip(
    nom: &str,
    infos: &str,
    details: &str,
    duree: &str,
    teinte: Option<u8>,
) -> estompe_natif::LigneClip {
    estompe_natif::LigneClip {
        chemin: format!("/home/alice/Vidéos/Estompe/{nom}.mp4").into(),
        nom: nom.into(),
        infos: infos.into(),
        details: details.into(),
        duree: duree.into(),
        origine: "".into(),
        a_vignette: teinte.is_some(),
        vignette: teinte.map(vignette).unwrap_or_default(),
    }
}

fn clips_vide(f: &Principale) {
    coquille_accueil(f);
    let c = f.global::<estompe_natif::DonneesClips>();
    c.set_visible(true);
    c.set_stockage_lu(true);
    c.set_espace("0 Ko sur 20 Go".into());
    c.set_part(0.0);
    c.set_vide_titre("Aucun clip".into());
    c.set_vide_texte("L'instant replay est désactivé.".into());
    c.set_vide_reglages(true);
    c.set_ouvert(true);
}

fn clips_remplie(f: &Principale) {
    coquille_accueil(f);
    let c = f.global::<estompe_natif::DonneesClips>();
    c.set_visible(true);
    c.set_envoi_disponible(true);
    c.set_total(4);
    c.set_stockage_lu(true);
    c.set_espace("1,2 Go sur 20 Go".into());
    c.set_part(0.06);
    let mut montage = ligne_clip(
        "Ace au B",
        "Valorant · 9 mars 2026 à 20:00",
        "62 Mo · 1080p · 60 i/s",
        "0:45",
        Some(200),
    );
    montage.origine = "Valorant 2026-03-07 09-05-02".into();
    c.set_clips(slint::ModelRc::new(slint::VecModel::from(vec![
        montage,
        ligne_clip(
            "Counter-Strike 2 2026-03-08 21-14-00",
            "Counter-Strike 2 · 8 mars 2026 à 21:14",
            "148 Mo · 1440p · 144 i/s · HEVC · piste micro",
            "1:30",
            Some(40),
        ),
        ligne_clip(
            "Valorant 2026-03-07 09-05-02",
            "Valorant · 7 mars 2026 à 09:05",
            "96 Mo · 1080p · 60 i/s",
            "1:00",
            Some(120),
        ),
        ligne_clip(
            "copie",
            "Autres · 2 janvier 2026 à 08:00",
            "12 Mo",
            "",
            None,
        ),
    ])));
    c.set_jeux(slint::ModelRc::new(slint::VecModel::from(vec![
        estompe_natif::JeuClips {
            jeu: "Counter-Strike 2".into(),
            nombre: 1,
            choisi: false,
        },
        estompe_natif::JeuClips {
            jeu: "Valorant".into(),
            nombre: 2,
            choisi: false,
        },
        estompe_natif::JeuClips {
            jeu: "Autres".into(),
            nombre: 1,
            choisi: false,
        },
    ])));
    c.set_ouvert(true);
}

fn clips_quota(f: &Principale) {
    clips_remplie(f);
    let c = f.global::<estompe_natif::DonneesClips>();
    c.set_espace("19 Go sur 20 Go".into());
    c.set_part(0.93);
    c.set_presque(true);
    c.set_alerte("Quota presque atteint (93 %) : supprime d'anciens clips ou augmente le quota pour continuer à enregistrer.".into());
    c.set_travaux(slint::ModelRc::new(slint::VecModel::from(vec![
        estompe_natif::TravailClip {
            id: 1,
            libelle: "Conversion de « Counter-Strike 2 2026-03-08 21-14-00 »".into(),
            progression: 0.42,
            a_progression: true,
        },
    ])));
}

fn clips_montages(f: &Principale) {
    clips_remplie(f);
    let c = f.global::<estompe_natif::DonneesClips>();
    c.set_modifier_disponible(true);
    f.global::<estompe_natif::DonneesEditeur>()
        .set_projets(slint::ModelRc::new(slint::VecModel::from(vec![
            estompe_natif::ProjetMontage {
                id: "2026-03-09-20-00-00-1".into(),
                nom: "Ace au B".into(),
                infos: "0:45,00 · 2 clips · 9 mars 2026 à 20:00".into(),
                manquants: "".into(),
            },
            estompe_natif::ProjetMontage {
                id: "2026-03-08-21-30-00-1".into(),
                nom: "Soirée du 8".into(),
                infos: "1:12,50 · 1 clip · 8 mars 2026 à 21:30".into(),
                manquants: "Clip introuvable : Counter-Strike 2 2026-03-08 21-14-00.mp4".into(),
            },
        ])));
}

fn image_apercu() -> slint::Image {
    let (l, h) = (640u32, 360u32);
    let mut tampon = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(l, h);
    for (i, p) in tampon.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i as u32 % l, i as u32 / l);
        let ciel = y < h * 3 / 5;
        *p = if ciel {
            slint::Rgba8Pixel::new(60 + (y / 4) as u8, 110 + (y / 5) as u8, 190, 255)
        } else {
            slint::Rgba8Pixel::new(70 + (x / 10) as u8, 120, 60, 255)
        };
    }
    slint::Image::from_rgba8(tampon)
}

fn onde(graine: usize) -> String {
    let pics: Vec<f32> = (0..200)
        .map(|i| {
            (((i * 37 + graine * 11) % 23) as f32 / 23.0)
                * if (i / 40 + graine).is_multiple_of(3) {
                    0.3
                } else {
                    1.0
                }
        })
        .collect();
    estompe_client::replay::montage::trace_onde(&pics, 14.0)
}

fn editeur(f: &Principale) {
    coquille_accueil(f);
    let d = f.global::<estompe_natif::DonneesEditeur>();
    let choix = |liste: &[&str], choisi: usize| {
        slint::ModelRc::new(slint::VecModel::from(
            liste
                .iter()
                .enumerate()
                .map(|(i, l)| estompe_natif::ChoixEditeur {
                    libelle: (*l).into(),
                    choisi: i == choisi,
                })
                .collect::<Vec<_>>(),
        ))
    };
    let segment =
        |debut: f32, largeur: f32, garde: bool, choisi: bool, libelle: &str, muet: bool| {
            estompe_natif::SegmentEditeur {
                debut,
                largeur,
                garde,
                choisi,
                libelle: libelle.into(),
                clip: "Valorant 2026-03-07 09-05-02".into(),
                muet,
            }
        };
    d.set_nom("Ace au B".into());
    d.set_duree("0:41,50".into());
    d.set_temps("0:12,25".into());
    d.set_segments(slint::ModelRc::new(slint::VecModel::from(vec![
        segment(0.0, 0.25, true, false, "", false),
        segment(0.25, 0.15, false, false, "", false),
        segment(0.4, 0.35, true, true, "0,5×", false),
        segment(0.75, 0.25, true, false, "", true),
    ])));
    d.set_tete(0.52);
    d.set_entree(0.0);
    d.set_sortie(1.0);
    d.set_pistes(slint::ModelRc::new(slint::VecModel::from(vec![
        estompe_natif::PisteEditeur {
            nom: "Jeu".into(),
            volume: 100.0,
            muet: false,
            onde: onde(1).into(),
        },
        estompe_natif::PisteEditeur {
            nom: "Micro".into(),
            volume: 65.0,
            muet: true,
            onde: onde(2).into(),
        },
    ])));
    d.set_clip_choisi("Valorant 2026-03-07 09-05-02".into());
    d.set_garde(true);
    d.set_vitesses(choix(&["0,25×", "0,5×", "1×", "1,5×", "2×", "4×"], 1));
    d.set_saccade(true);
    d.set_peut_annuler(true);
    d.set_rapports(choix(&["Libre", "16:9", "1:1", "9:16", "4:5"], 0));
    d.set_peut_reculer(true);
    d.set_peut_avancer(true);
    d.set_image(image_apercu());
    d.set_a_image(true);
    d.set_pret(true);
    d.set_ouvert(true);
}

fn editeur_export(f: &Principale) {
    editeur(f);
    let d = f.global::<estompe_natif::DonneesEditeur>();
    let choix = |liste: &[&str], choisi: usize| {
        slint::ModelRc::new(slint::VecModel::from(
            liste
                .iter()
                .enumerate()
                .map(|(i, l)| estompe_natif::ChoixEditeur {
                    libelle: (*l).into(),
                    choisi: i == choisi,
                })
                .collect::<Vec<_>>(),
        ))
    };
    d.set_hauteurs(choix(&["Cadre (1920 × 1080)", "720p", "480p"], 0));
    d.set_cadences(choix(&["60 i/s", "30 i/s"], 0));
    d.set_export_format("1920 × 1080, 60 i/s, 0:41,50".into());
    d.set_export_en_cours(true);
    d.set_export_libelle("Export de « Soirée du 8 » (encodeur logiciel)".into());
    d.set_export_progression(0.37);
    d.set_export_ouvert(true);
}

fn clips_envoi(f: &Principale) {
    clips_remplie(f);
    let c = f.global::<estompe_natif::DonneesClips>();
    c.set_envoi_clip("Counter-Strike 2 2026-03-08 21-14-00 · 148 Mo".into());
    c.set_envoi_alerte_titre("Clip plus gros que la limite d'envoi (100 Mo)".into());
    c.set_envoi_alerte("Il peut être réencodé plus léger, en H.264, pour tenir dans la limite : la qualité de la copie envoyée baisse, le clip d'origine n'est pas modifié.".into());
    c.set_envoi_action("Réencoder et envoyer".into());
    c.set_destinations(slint::ModelRc::new(slint::VecModel::from(vec![
        estompe_natif::DestinationClip {
            cle: "c:c1".into(),
            nom: "# general".into(),
            ou: "Les copains".into(),
            choisie: false,
        },
        estompe_natif::DestinationClip {
            cle: "c:c4".into(),
            nom: "# clips".into(),
            ou: "Les copains".into(),
            choisie: true,
        },
        estompe_natif::DestinationClip {
            cle: "d:d1".into(),
            nom: "Bob".into(),
            ou: "Message privé".into(),
            choisie: false,
        },
    ])));
    c.set_destination_choisie(true);
    c.set_envoi_ouvert(true);
}

fn mesures_vide(f: &Principale) {
    let g = f.global::<estompe_natif::DonneesMesures>();
    g.set_disponible(true);
    g.set_peut_exporter(true);
    g.set_message("Session terminée : exporte les mesures pour les partager.".into());
    g.set_ouvert(true);
}

fn mesures_scenario(f: &Principale) {
    let g = f.global::<estompe_natif::DonneesMesures>();
    let ligne = |id: &str, titre: &str, fait: bool| estompe_natif::LigneScenario {
        id: id.into(),
        titre: titre.into(),
        aide: "".into(),
        fait,
    };
    g.set_disponible(true);
    g.set_en_session(true);
    g.set_scenarios(slint::ModelRc::new(slint::VecModel::from(vec![
        ligne("repos", "Repos 5 min", true),
        ligne("gros-salon", "Ouverture d'un gros salon", false),
        ligne("jeu", "Partie de jeu avec overlays", false),
    ])));
    g.set_scenario_en_cours("Flux de messages".into());
    g.set_aide_en_cours(
        "Pendant 1 minute, laissez arriver des messages dans le salon ouvert.".into(),
    );
    g.set_reste(42);
    g.set_ouvert(true);
}

fn parametres(f: &Principale, onglet: estompe_client::parametres::Onglet) {
    coquille_accueil(f);
    afficher_parametres(f, onglet);
}

fn afficher_parametres(f: &Principale, onglet: estompe_client::parametres::Onglet) {
    let mut p = estompe_client::preferences::Preferences::default();
    p.ptt.actif = true;
    p.ptt.touche = "KeyV".into();
    estompe_natif::parametres::afficher_onglets(f);
    estompe_natif::parametres::afficher_preferences(f, &p);
    estompe_natif::parametres::poser_onglet(f, onglet);
    let g = f.global::<estompe_natif::DonneesParametres>();
    g.set_demarrage_disponible(true);
    g.set_ouverte(true);
}

fn parametres_compte(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Compte);
}

fn maintenant_de_la_coquille() -> i64 {
    use chrono::TimeZone;
    let matin = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap();
    chrono::Local
        .from_local_datetime(&matin)
        .earliest()
        .unwrap()
        .timestamp_millis()
}

fn etat_notifications() -> estompe_client::store::Etat {
    let mut etat = commun::coquille::etat();
    etat.statuts.insert(
        "u1".into(),
        estompe_client::temps_reel::evenements::StatutPresence::Dnd,
    );
    etat.reglages_notification = serde_json::from_value(serde_json::json!({
        "levels": {"s1": "all", "c2": "none"},
        "types": {"dm": true, "mention": true, "channel": false, "voice": true},
        "preview": true,
        "sound": false,
        "general": "mentions",
        "ignore_group": ["s1"],
        "mutes": {"c3": maintenant_de_la_coquille() + 45 * 60_000}
    }))
    .unwrap();
    etat
}

fn parametres_notifications(f: &Principale) {
    let pont = coquille(f, etat_notifications(), Route::Accueil);
    afficher_parametres(f, estompe_client::parametres::Onglet::Notifications);
    pont.projeter_parametres(f);
    stabiliser(f);
}

fn parametres_notifications_serveur(f: &Principale) {
    parametres_notifications(f);
    f.window()
        .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(760.0, 500.0),
            delta_x: 0.0,
            delta_y: -5000.0,
        });
    stabiliser(f);
}

fn parametres_voix(f: &Principale) {
    f.global::<estompe_natif::DonneesJeux>()
        .set_disponible(true);
    parametres(f, estompe_client::parametres::Onglet::Voix);
}

fn parametres_apparence(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Apparence);
}

fn parametres_application(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Application);
    f.global::<estompe_natif::DonneesParametres>()
        .set_annonce("Fonctions alpha débloquées.".into());
}

fn parametres_debogage(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Debogage);
    let g = f.global::<estompe_natif::DonneesParametres>();
    g.set_journal_actif(true);
    let ligne = |texte: &str, niveau: &str| estompe_natif::LigneJournal {
        texte: texte.into(),
        niveau: niveau.into(),
    };
    g.set_lignes(slint::ModelRc::new(slint::VecModel::from(vec![
        ligne(
            "2026-11-19 09:30:00.123 [INFO] debug: mode debug activé",
            "info",
        ),
        ligne(
            "2026-11-19 09:30:02.481 [INFO] vocal: connexion au salon v1",
            "info",
        ),
        ligne(
            "2026-11-19 09:30:05.004 [WARN] réseau: reconnexion dans 2 s",
            "warn",
        ),
        ligne(
            "2026-11-19 09:30:07.950 [ERROR] partage: capture refusée par le système",
            "error",
        ),
    ])));
    g.set_total_lignes(4);
    g.set_message_journal(
        "Journal exporté : /home/bob/Téléchargements/estompe-debug-2026-11-19-09-30-10.log".into(),
    );
}

fn parametres_signalements(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Signalements);
}

fn parametres_raccourcis(f: &Principale) {
    f.global::<estompe_natif::DonneesOverlayMessages>()
        .set_disponible(true);
    f.global::<estompe_natif::DonneesReplay>()
        .set_disponible(true);
    parametres(f, estompe_client::parametres::Onglet::Raccourcis);
}

fn parametres_replay(f: &Principale) {
    let r = f.global::<estompe_natif::DonneesReplay>();
    r.set_disponible(true);
    r.set_resume(
        "Activé : les 30 dernières secondes du jeu sont gardées, F10 enregistre un clip.".into(),
    );
    parametres(f, estompe_client::parametres::Onglet::Replay);
}

fn parametres_recherche(f: &Principale) {
    parametres(f, estompe_client::parametres::Onglet::Voix);
    f.global::<estompe_natif::DonneesParametres>()
        .set_recherche("micro".into());
    estompe_natif::parametres::afficher_onglets(f);
}
