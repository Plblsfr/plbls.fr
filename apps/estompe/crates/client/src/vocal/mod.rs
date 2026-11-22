pub mod camera;
pub mod erreurs;
pub mod etat;
pub mod factice;
pub mod incrustation;
#[cfg(feature = "livekit")]
pub mod livekit;
pub mod mixage;
pub mod partage;
pub mod ptt;
pub mod salle;
pub mod scene;
pub mod session;
pub mod video;

pub use camera::{
    message_camera, ApercuCamera, CamerasFactices, CaptureCamera, FormatVideo, QualiteCamera,
};
pub use erreurs::{CauseMicro, ErreurVocal};
pub use etat::{etats_affiches, EtatAffiche, EtatVocal, Phase};
pub use incrustation::{
    position_depuis_pointeur, zone_incrustation, ApercuIncruste, Compositeur, FormeIncrustation,
    ReglagesIncrustation, COINS, MARGE_MAX, PAS_MARGE, RATIO_APERCU, TAILLE_MAX, TAILLE_MIN,
};
pub use mixage::{Ecoute, Limiteur, Mixeur};
pub use partage::{
    borner, debit_partage, mbits, mode_partage, note_resolution, palier_inferieur, resume_partage,
    Degradation, ImagePartage, IndiceContenu, PartageLocal, PlafondPartage, Prereglage,
    QualitePartage, ReglagesPartage, ResolutionPartage, CADENCES_PARTAGE, PREREGLAGES,
};
pub use ptt::{EtatPtt, PousserPourParler, ReglagesPtt, SourceTouche};
pub use salle::{
    Connecteur, ConnecteurIndisponible, Connexion, ErreurSalle, EvenementSalle, InfoParticipant,
    JetonSalle, Limitation, Qualite, RaisonDeconnexion, Salle, SortieMicro, SortieVideo,
    SourceAudio,
};
pub use scene::{
    construire_elements, dimensions_grille, doit_recevoir, rassembler_spectateurs, ElementScene,
    Epinglage, QualiteRecue, SourcePiste, Spectateurs, TuileVideo,
};
pub use session::{ApiVocale, AvisVocal, Dependances, Reglages, SessionVocale, DELAI_FERMETURE};
pub use video::{
    CleVideo, DemandeVideo, FluxVideo, ImageI420, ImageVideo, Mire, RecepteurVideo, SourceVideo,
    TrameVideo, Videos,
};

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use estompe_plateforme::{
        CaptureSon, CibleSon, EcransFactices, FormatAudio, Indisponible, SensAudio, SonsFactices,
        SourceCapture, TypeSource,
    };
    use tokio::runtime::Handle;

    use super::factice::{ApiVocaleFactice, ServeurVocalFactice};
    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{Bascule, ModeBruit, ReglagesMicro, ECHANTILLONS_PAR_TRAME};

    struct Poste {
        audio: SystemeFactice,
        cameras: CamerasFactices,
        ecrans: EcransFactices,
        sons: SonsFactices,
        api: Arc<ApiVocaleFactice>,
        session: SessionVocale,
    }

    fn audio(avec_micro: bool) -> SystemeFactice {
        let audio = SystemeFactice::nouveau();
        if avec_micro {
            audio.brancher(
                SensAudio::Entree,
                "micro",
                "Micro",
                FormatAudio {
                    frequence: 48_000,
                    canaux: 1,
                },
            );
            audio.brancher(
                SensAudio::Entree,
                "casque",
                "Casque",
                FormatAudio {
                    frequence: 16_000,
                    canaux: 1,
                },
            );
        }
        audio.brancher(
            SensAudio::Sortie,
            "hp",
            "Haut-parleurs",
            FormatAudio {
                frequence: 48_000,
                canaux: 2,
            },
        );
        audio
    }

    fn sans_bruit() -> Reglages {
        Reglages {
            traitement: ReglagesMicro {
                bruit: ModeBruit::Aucun,
                ..ReglagesMicro::default()
            },
            ..Reglages::default()
        }
    }

    fn poste(serveur: &ServeurVocalFactice, identite: &str, avec_micro: bool) -> Poste {
        poste_regle(serveur, identite, avec_micro, sans_bruit())
    }

    fn poste_regle(
        serveur: &ServeurVocalFactice,
        identite: &str,
        avec_micro: bool,
        reglages: Reglages,
    ) -> Poste {
        poste_avec(serveur, identite, avec_micro, reglages, true)
    }

    fn poste_avec(
        serveur: &ServeurVocalFactice,
        identite: &str,
        avec_micro: bool,
        reglages: Reglages,
        son: bool,
    ) -> Poste {
        let audio = audio(avec_micro);
        let cameras = CamerasFactices::nouvelles();
        cameras.brancher("c1", "Caméra intégrée");
        cameras.brancher("c2", "Webcam USB");
        let api = Arc::new(ApiVocaleFactice::nouvelle(identite));
        let ecrans = EcransFactices::nouveaux();
        ecrans.brancher(source_capture("ecran:0", TypeSource::Ecran, 2560, 1440));
        ecrans.brancher(source_capture("fenetre:7", TypeSource::Fenetre, 1280, 800));
        let sons = SonsFactices::nouveaux();
        let capture_son: Arc<dyn CaptureSon> = if son {
            Arc::new(sons.clone())
        } else {
            Arc::new(Indisponible)
        };
        let session = SessionVocale::demarrer(
            &Handle::current(),
            Dependances {
                api: api.clone(),
                connecteur: Arc::new(serveur.clone()),
                audio: Arc::new(audio.clone()),
                cameras: Arc::new(cameras.clone()),
                ecrans: Arc::new(ecrans.clone()),
                sons: capture_son,
                apm: None,
                reglages,
            },
        );
        Poste {
            audio,
            cameras,
            ecrans,
            sons,
            api,
            session,
        }
    }

    fn source_capture(id: &str, genre: TypeSource, largeur: u32, hauteur: u32) -> SourceCapture {
        SourceCapture {
            id: id.to_string(),
            nom: format!("Source {id}"),
            application: String::new(),
            genre,
            largeur,
            hauteur,
            principale: false,
            processus: (genre == TypeSource::Fenetre).then_some(4242),
            estompe: false,
        }
    }

    async fn attendre(
        session: &SessionVocale,
        condition: impl Fn(&EtatVocal) -> bool,
    ) -> EtatVocal {
        let mut etat = session.etat();
        tokio::time::timeout(Duration::from_secs(600), async {
            loop {
                let courant = etat.borrow_and_update().clone();
                if condition(&courant) {
                    return courant;
                }
                etat.changed().await.unwrap();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("état attendu jamais atteint : {:?}", session.instantane()))
    }

    async fn attendre_que(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(600), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("condition jamais remplie");
    }

    fn connecte_a(salon: &'static str) -> impl Fn(&EtatVocal) -> bool {
        move |e| e.phase == Phase::Connecte && e.salon.as_deref() == Some(salon)
    }

    #[tokio::test(start_paused = true)]
    async fn deux_participants_se_voient_et_s_entendent() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        let etat = attendre(&alice.session, connecte_a("v1")).await;
        assert_eq!(etat.identite.as_deref(), Some("alice"));
        assert_eq!(serveur.attributs("v1", "alice")["muted"], "");
        bob.session.rejoindre("v1");
        attendre(&bob.session, |e| e.participants.len() == 2).await;
        attendre(&alice.session, |e| e.participants.contains_key("bob")).await;
        alice
            .audio
            .pousser("micro", &[0.5; 10 * ECHANTILLONS_PAR_TRAME + 1]);
        assert_eq!(serveur.trames_recues("alice"), 10);
        attendre_que(|| {
            bob.audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().any(|e| *e > 0.4))
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn sans_regarder_aucune_image_ni_son_du_partage_n_est_recu() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        serveur.publier_video("v1", "alice", SourceVideo::Partage);
        serveur.publier_son_partage("v1", "alice");
        bob.session.rejoindre("v1");
        let cle = CleVideo::partage("alice");
        let etat = attendre(&bob.session, |e| {
            e.video(&cle) && e.sons_partage.contains("alice")
        })
        .await;
        assert!(!etat.regarde("alice"));
        let demande = DemandeVideo {
            largeur: 1280,
            hauteur: 720,
        };
        bob.session
            .definir_affichage([(cle.clone(), demande)].into_iter().collect());
        let mire = Mire::nouvelle(160, 90);
        let image = || -> ImageVideo { Arc::new(mire.image(0)) };
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(serveur
            .abonnes_video("v1", "alice", SourceVideo::Partage)
            .is_empty());
        assert!(serveur.abonnes_son_partage("v1", "alice").is_empty());
        assert_eq!(
            serveur.pousser_image("v1", "alice", SourceVideo::Partage, image()),
            0
        );
        assert_eq!(serveur.pousser_son_partage("v1", "alice", &[1000; 480]), 0);
        let recepteur = bob.session.videos().recepteur(&cle);
        assert_eq!(recepteur.images_recues(), 0);
        assert_eq!(serveur.images_envoyees("bob"), 0);
        assert_eq!(serveur.attributs("v1", "bob")["watching"], "");

        bob.session.regarder("alice", Some(true));
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Partage)
                .is_empty()
                && serveur.abonnes_son_partage("v1", "alice").contains("bob")
        })
        .await;
        assert_eq!(
            serveur.abonnes_video("v1", "alice", SourceVideo::Partage)["bob"],
            demande
        );
        serveur.pousser_image("v1", "alice", SourceVideo::Partage, image());
        attendre_que(|| recepteur.images_recues() == 1).await;
        attendre_que(|| {
            serveur
                .attributs("v1", "bob")
                .get("watching")
                .map(String::as_str)
                == Some("TR_alice_ecran")
        })
        .await;
        let etat = attendre(&alice.session, |e| {
            e.participants
                .get("bob")
                .is_some_and(|p| p.attributs.get("watching").is_some_and(|w| !w.is_empty()))
        })
        .await;
        let mut alice_etat = etat;
        alice_etat
            .pistes
            .insert(cle.clone(), "TR_alice_ecran".into());
        assert_eq!(
            alice_etat.spectateurs()[&cle],
            [("bob".to_string(), "bob".to_string())]
        );

        bob.session.regarder("alice", Some(false));
        attendre_que(|| {
            serveur
                .abonnes_video("v1", "alice", SourceVideo::Partage)
                .is_empty()
                && serveur.abonnes_son_partage("v1", "alice").is_empty()
        })
        .await;
        attendre_que(|| recepteur.derniere().is_none()).await;
        attendre_que(|| serveur.attributs("v1", "bob")["watching"].is_empty()).await;
        assert_eq!(
            serveur.pousser_image("v1", "alice", SourceVideo::Partage, image()),
            0
        );
        assert_eq!(recepteur.images_recues(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn le_son_d_un_partage_regarde_est_mixe_avec_son_volume_propre() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", false);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        bob.session.rejoindre("v1");
        attendre(&bob.session, connecte_a("v1")).await;
        serveur.publier_son_partage("v1", "alice");
        attendre(&bob.session, |e| e.sons_partage.contains("alice")).await;
        bob.session.regarder("alice", Some(true));
        attendre_que(|| serveur.abonnes_son_partage("v1", "alice").contains("bob")).await;
        let fort = || {
            for _ in 0..10 {
                serveur.pousser_son_partage("v1", "alice", &[16_000; ECHANTILLONS_PAR_TRAME]);
            }
        };
        fort();
        attendre_que(|| {
            bob.audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().any(|e| *e > 0.2))
        })
        .await;
        bob.session
            .definir_muet("alice", SourceAudio::Partage, true);
        attendre(&bob.session, |e| {
            e.ecoute.muet("alice", SourceAudio::Partage)
        })
        .await;
        assert!(!bob
            .session
            .instantane()
            .ecoute
            .muet("alice", SourceAudio::Micro));
        bob.session.regarder("alice", Some(false));
        attendre_que(|| serveur.abonnes_son_partage("v1", "alice").is_empty()).await;
        serveur.retirer_son_partage("v1", "alice");
        attendre(&bob.session, |e| !e.sons_partage.contains("alice")).await;
    }

    #[tokio::test(start_paused = true)]
    async fn regarder_automatiquement_et_ne_jamais_rejouer_son_propre_partage() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste_regle(
            &serveur,
            "bob",
            true,
            Reglages {
                regarder_auto: true,
                ..sans_bruit()
            },
        );
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        bob.session.rejoindre("v1");
        attendre(&bob.session, connecte_a("v1")).await;
        serveur.publier_video("v1", "bob#screen", SourceVideo::Partage);
        serveur.publier_son_partage("v1", "bob#screen");
        serveur.publier_video("v1", "alice", SourceVideo::Partage);
        serveur.publier_son_partage("v1", "alice");
        let propre = CleVideo::partage("bob");
        let autre = CleVideo::partage("alice");
        attendre(&bob.session, |e| {
            e.video(&propre) && e.video(&autre) && e.sons_partage.len() == 2
        })
        .await;
        let demande = DemandeVideo {
            largeur: 640,
            hauteur: 360,
        };
        bob.session.definir_affichage(
            [(propre.clone(), demande), (autre.clone(), demande)]
                .into_iter()
                .collect(),
        );
        attendre_que(|| {
            serveur
                .abonnes_video("v1", "bob#screen", SourceVideo::Partage)
                .contains_key("bob")
                && serveur
                    .abonnes_video("v1", "alice", SourceVideo::Partage)
                    .contains_key("bob")
                && serveur.abonnes_son_partage("v1", "alice").contains("bob")
        })
        .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(serveur.abonnes_son_partage("v1", "bob#screen").is_empty());
        bob.session.regarder("alice", Some(false));
        attendre_que(|| serveur.abonnes_son_partage("v1", "alice").is_empty()).await;
        serveur.retirer_video("v1", "alice", SourceVideo::Partage);
        let etat = attendre(&bob.session, |e| !e.video(&autre)).await;
        assert!(
            !etat.regardes.contains_key("alice"),
            "le choix est oublié avec le partage"
        );
        serveur.publier_video("v1", "alice", SourceVideo::Partage);
        attendre_que(|| {
            serveur
                .abonnes_video("v1", "alice", SourceVideo::Partage)
                .contains_key("bob")
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn seules_les_videos_affichees_sont_recues() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        serveur.publier_video("v1", "alice", SourceVideo::Camera);
        bob.session.rejoindre("v1");
        let cle = CleVideo::camera("alice");
        attendre(&bob.session, |e| e.video(&cle)).await;
        let mire = Mire::nouvelle(160, 90);
        let image = || -> ImageVideo { Arc::new(mire.image(0)) };
        assert_eq!(
            serveur.pousser_image("v1", "alice", SourceVideo::Camera, image()),
            0
        );
        let demande = DemandeVideo {
            largeur: 320,
            hauteur: 180,
        };
        bob.session
            .definir_affichage([(cle.clone(), demande)].into_iter().collect());
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Camera)
                .is_empty()
        })
        .await;
        assert_eq!(
            serveur.abonnes_video("v1", "alice", SourceVideo::Camera)["bob"],
            demande
        );
        let recepteur = bob.session.videos().recepteur(&cle);
        for _ in 0..3 {
            serveur.pousser_image("v1", "alice", SourceVideo::Camera, image());
        }
        attendre_que(|| recepteur.images_recues() == 3).await;
        assert!(recepteur.derniere().is_some());
        bob.session.definir_affichage(Default::default());
        attendre_que(|| recepteur.derniere().is_none()).await;
        assert!(serveur
            .abonnes_video("v1", "alice", SourceVideo::Camera)
            .is_empty());
        for _ in 0..3 {
            serveur.pousser_image("v1", "alice", SourceVideo::Camera, image());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(recepteur.images_recues(), 3);
        assert_eq!(serveur.images_envoyees("bob"), 3);
        serveur.retirer_video("v1", "alice", SourceVideo::Camera);
        attendre(&bob.session, |e| !e.video(&cle)).await;
        bob.session
            .definir_affichage([(cle.clone(), demande)].into_iter().collect());
        serveur.publier_video("v1", "alice", SourceVideo::Camera);
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Camera)
                .is_empty()
        })
        .await;
        alice.session.quitter();
        attendre(&bob.session, |e| !e.video(&cle)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn la_camera_est_publiee_changee_puis_retiree() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        bob.session.rejoindre("v1");
        attendre(&bob.session, connecte_a("v1")).await;
        let cle = CleVideo::camera("alice");
        bob.session.definir_affichage(
            [(
                cle.clone(),
                DemandeVideo {
                    largeur: 640,
                    hauteur: 360,
                },
            )]
            .into_iter()
            .collect(),
        );
        alice.session.activer_camera(true);
        attendre(&alice.session, |e| e.camera_active && !e.camera_demarrage).await;
        assert!(serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        assert_eq!(
            serveur.format_publie("v1", "alice", SourceVideo::Camera),
            Some(QualiteCamera::Q720.format())
        );
        assert_eq!(alice.cameras.ouvertes(), vec!["c1".to_string()]);
        attendre(&bob.session, |e| e.video(&cle)).await;
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Camera)
                .is_empty()
        })
        .await;
        let locale = alice.session.videos().recepteur(&cle);
        let distante = bob.session.videos().recepteur(&cle);
        alice.cameras.pousser(3);
        assert_eq!(locale.images_recues(), 3);
        assert_eq!(
            serveur.images_publiees("v1", "alice", SourceVideo::Camera),
            3
        );
        attendre_que(|| distante.images_recues() == 3).await;
        let image = distante.derniere().unwrap();
        assert_eq!((image.largeur(), image.hauteur()), (1280, 720));
        alice.session.choisir_camera(Some("c2".into()));
        attendre(&alice.session, |e| e.camera.as_deref() == Some("c2")).await;
        assert_eq!(alice.cameras.ouvertes(), vec!["c2".to_string()]);
        assert!(alice.session.instantane().camera_active);
        alice.cameras.pousser(1);
        attendre_que(|| distante.images_recues() == 4).await;
        alice.session.definir_qualite_camera(QualiteCamera::Q360);
        attendre(&alice.session, |e| {
            e.qualite_camera == QualiteCamera::Q360 && e.camera_active
        })
        .await;
        attendre_que(|| {
            serveur.format_publie("v1", "alice", SourceVideo::Camera)
                == Some(QualiteCamera::Q360.format())
        })
        .await;
        alice.session.activer_camera(false);
        attendre(&alice.session, |e| !e.camera_active).await;
        assert!(alice.cameras.ouvertes().is_empty());
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        assert!(locale.derniere().is_none());
        attendre(&bob.session, |e| !e.video(&cle)).await;
        assert!(alice.session.instantane().camera_erreur.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn une_camera_indisponible_revient_a_desactivee_avec_la_raison() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.cameras.occuper("c1", true);
        alice.session.activer_camera(true);
        let etat = attendre(&alice.session, |e| e.camera_erreur.is_some()).await;
        assert!(!etat.camera_active && !etat.camera_demarrage);
        assert_eq!(
            etat.camera_erreur.as_deref(),
            Some("Caméra déjà utilisée par une autre application.")
        );
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        alice.cameras.occuper("c1", false);
        alice.session.activer_camera(true);
        attendre(&alice.session, |e| e.camera_active).await;
        alice.cameras.debrancher("c1");
        let etat = attendre(&alice.session, |e| {
            e.camera_erreur.as_deref() == Some("Caméra débranchée.")
        })
        .await;
        assert!(!etat.camera_active);
        assert_eq!(etat.camera_erreur.as_deref(), Some("Caméra débranchée."));
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        alice.cameras.debrancher("c2");
        alice.session.activer_camera(true);
        let etat = attendre(&alice.session, |e| {
            e.camera_erreur.as_deref() == Some("Aucune caméra trouvée.")
        })
        .await;
        assert!(!etat.camera_active);
        assert_eq!(etat.phase, Phase::Connecte);
    }

    #[tokio::test(start_paused = true)]
    async fn rejoindre_camera_activee_la_publie_des_la_connexion() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre_avec_camera("v1");
        attendre(&alice.session, |e| {
            e.phase == Phase::Connecte && e.camera_active
        })
        .await;
        assert!(serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        alice.session.quitter();
        let etat = attendre(&alice.session, |e| !e.actif()).await;
        assert!(!etat.camera_active);
        assert!(alice.cameras.ouvertes().is_empty());
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Camera));
        alice.session.rejoindre("v1");
        let etat = attendre(&alice.session, connecte_a("v1")).await;
        assert!(!etat.camera_active);
    }

    #[tokio::test(start_paused = true)]
    async fn quitter_previent_et_retire_le_participant() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let mut avis = alice.session.abonner_avis();
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        assert_eq!(avis.recv().await.unwrap(), AvisVocal::Rejoint("v1".into()));
        alice.session.quitter();
        let etat = attendre(&alice.session, |e| !e.actif()).await;
        assert!(!etat.micro_coupe && !etat.casque_coupe && etat.salon.is_none());
        assert_eq!(avis.recv().await.unwrap(), AvisVocal::Quitte("v1".into()));
        assert!(serveur.participants("v1").is_empty());
        assert!(alice.audio.flux_ouverts().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn rejoindre_s_annonce_avant_la_connexion_et_un_refus_ramene_a_inactif() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.api.refuser("plein", 409, "voice_full");
        alice.api.retarder(Duration::from_secs(1));
        let mut etat = alice.session.etat();
        alice.session.rejoindre("plein");
        etat.changed().await.unwrap();
        assert_eq!(etat.borrow().phase, Phase::Connexion);
        let fin = attendre(&alice.session, |e| e.erreur.is_some()).await;
        assert_eq!(fin.phase, Phase::Inactif);
        assert_eq!(
            fin.erreur.as_deref(),
            Some("Salon vocal complet (10 participants maximum).")
        );
        alice.api.refuser("prive", 403, "forbidden");
        alice.session.rejoindre("prive");
        let fin = attendre(&alice.session, |e| {
            e.erreur.as_deref() == Some("Tu n'as pas le droit de rejoindre ce salon.")
        })
        .await;
        assert_eq!(fin.phase, Phase::Inactif);
        assert_eq!(serveur.connexions(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn sans_micro_reste_dans_le_salon_en_ecoute_seule() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", false);
        alice.session.rejoindre("v1");
        let etat = attendre(&alice.session, connecte_a("v1")).await;
        assert_eq!(etat.micro_indisponible, Some(CauseMicro::Absent));
        assert!(etat.micro_coupe && !etat.micro_emet());
        assert!(etat.erreur.unwrap().ends_with("Vous êtes en écoute seule."));
        alice.session.basculer_micro();
        alice.session.basculer_casque();
        let etat = attendre(&alice.session, |e| e.casque_coupe).await;
        assert!(etat.micro_coupe);
        assert_eq!(serveur.attributs("v1", "alice")["muted"], "1");
    }

    #[tokio::test(start_paused = true)]
    async fn micro_et_casque_coupes_sont_publies() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.basculer_micro();
        attendre(&alice.session, |e| e.micro_coupe).await;
        attendre_que(|| serveur.attributs("v1", "alice")["muted"] == "1").await;
        assert_eq!(serveur.micro_coupe("alice"), Some(true));
        alice.audio.pousser("micro", &[0.5; ECHANTILLONS_PAR_TRAME]);
        assert_eq!(serveur.trames_recues("alice"), 0);
        alice.session.basculer_casque();
        attendre_que(|| serveur.attributs("v1", "alice")["deaf"] == "1").await;
        alice.session.basculer_casque();
        let etat = attendre(&alice.session, |e| !e.casque_coupe).await;
        assert!(
            etat.micro_coupe,
            "le micro retrouve son état d'avant la sourdine"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn coupe_par_un_moderateur_impossible_de_reactiver_son_micro() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.imposer(true, false);
        attendre(&alice.session, |e| e.micro_impose && e.micro_coupe).await;
        for _ in 0..3 {
            alice.session.basculer_micro();
        }
        alice.session.basculer_casque();
        let etat = attendre(&alice.session, |e| e.casque_coupe).await;
        assert!(etat.micro_coupe);
        alice.session.basculer_casque();
        let etat = attendre(&alice.session, |e| !e.casque_coupe).await;
        assert!(etat.micro_coupe && !etat.micro_emet());
        assert_eq!(serveur.micro_coupe("alice"), Some(true));
        alice.audio.pousser("micro", &[0.5; ECHANTILLONS_PAR_TRAME]);
        assert_eq!(serveur.trames_recues("alice"), 0);
        assert!(serveur.envoyer(
            "v1",
            "alice",
            EvenementSalle::Permissions {
                publier_micro: true,
                recevoir: false,
            }
        ));
        let etat = attendre(&alice.session, |e| !e.micro_impose).await;
        assert!(etat.son_impose && etat.micro_coupe, "{etat:?}");
        alice.session.basculer_micro();
        attendre(&alice.session, |e| e.micro_emet()).await;
    }

    #[tokio::test(start_paused = true)]
    async fn coupure_du_reseau_de_20_secondes_retour_sans_action_et_micro_conserve() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        bob.session.rejoindre("v1");
        attendre(&alice.session, |e| e.participants.len() == 2).await;
        alice.session.basculer_micro();
        attendre_que(|| serveur.attributs("v1", "alice")["muted"] == "1").await;
        let debut = tokio::time::Instant::now();
        serveur.couper("v1", "alice", RaisonDeconnexion::Reseau);
        alice.api.hors_ligne.store(true, Ordering::SeqCst);
        attendre(&alice.session, |e| e.phase == Phase::Reconnexion).await;
        attendre(&bob.session, |e| !e.participants.contains_key("alice")).await;
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(alice.session.instantane().phase, Phase::Reconnexion);
        serveur.hors_ligne(false);
        alice.api.hors_ligne.store(false, Ordering::SeqCst);
        let etat = attendre(&alice.session, connecte_a("v1")).await;
        assert!(debut.elapsed() < Duration::from_secs(35));
        assert!(etat.micro_coupe, "état du micro conservé");
        assert_eq!(serveur.attributs("v1", "alice")["muted"], "1");
        attendre(&bob.session, |e| e.participants.contains_key("alice")).await;
        alice.session.basculer_micro();
        attendre(&alice.session, |e| e.micro_emet()).await;
        alice
            .audio
            .pousser("micro", &[0.5; 2 * ECHANTILLONS_PAR_TRAME]);
        assert!(serveur.trames_recues("alice") > 0);
    }

    #[tokio::test(start_paused = true)]
    async fn la_reconnexion_du_sdk_republie_l_etat_du_micro() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.basculer_casque();
        attendre_que(|| serveur.attributs("v1", "alice")["deaf"] == "1").await;
        serveur.envoyer("v1", "alice", EvenementSalle::Reconnexion);
        attendre(&alice.session, |e| e.phase == Phase::Reconnexion).await;
        serveur.envoyer("v1", "alice", EvenementSalle::Reconnecte);
        let etat = attendre(&alice.session, connecte_a("v1")).await;
        assert!(etat.casque_coupe && etat.micro_coupe);
        assert_eq!(serveur.connexions(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn un_seul_vocal_par_compte_sans_reconnexion_en_boucle() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        serveur.couper("v1", "alice", RaisonDeconnexion::IdentiteDupliquee);
        let etat = attendre(&alice.session, |e| !e.actif()).await;
        assert!(etat.erreur.is_none());
        tokio::time::sleep(Duration::from_secs(120)).await;
        assert_eq!(serveur.connexions(), 1);
        assert_eq!(alice.api.appels.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn deplace_par_un_moderateur_rejoint_le_salon_cible_en_gardant_son_etat() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let mut avis = alice.session.abonner_avis();
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.basculer_micro();
        attendre(&alice.session, |e| e.micro_coupe).await;
        alice.session.deplace_par_moderation("autre", "v3");
        alice.session.deplace_par_moderation("v1", "v2");
        let etat = attendre(&alice.session, connecte_a("v2")).await;
        assert!(etat.micro_coupe);
        attendre_que(|| serveur.participants("v1").is_empty()).await;
        assert_eq!(serveur.attributs("v2", "alice")["muted"], "1");
        let mut recus = Vec::new();
        while let Ok(a) = avis.try_recv() {
            recus.push(a);
        }
        assert!(recus.contains(&AvisVocal::Deplace("v2".into())));
        assert!(!recus.contains(&AvisVocal::Deplace("v3".into())));

        serveur.couper("v2", "alice", RaisonDeconnexion::Retire);
        attendre(&alice.session, |e| !e.actif()).await;
        alice.session.deplace_par_moderation("v2", "v4");
        attendre(&alice.session, connecte_a("v4")).await;

        serveur.couper("v4", "alice", RaisonDeconnexion::Retire);
        attendre(&alice.session, |e| !e.actif()).await;
        tokio::time::sleep(Duration::from_secs(6)).await;
        alice.session.deplace_par_moderation("v4", "v5");
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(!alice.session.instantane().actif());
        assert!(serveur.participants("v5").is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn passer_d_un_salon_a_l_autre_ne_garde_qu_une_salle() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.deplacer("v2");
        attendre(&alice.session, connecte_a("v2")).await;
        attendre_que(|| serveur.participants("v1").is_empty()).await;
        alice.session.rejoindre("v2");
        alice.session.rejoindre("v3");
        attendre(&alice.session, connecte_a("v3")).await;
        attendre_que(|| serveur.participants("v2").is_empty()).await;
        assert_eq!(serveur.participants("v3").len(), 1);
        assert_eq!(alice.audio.flux_ouverts().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn fermer_l_app_sort_proprement_du_vocal() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.fermer().await;
        assert!(serveur.participants("v1").is_empty());
        alice.session.rejoindre("v1");
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(serveur.participants("v1").is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn change_de_micro_a_chaud_et_bascule_quand_il_est_debranche() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let mut avis = alice.session.abonner_avis();
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        assert_eq!(avis.recv().await.unwrap(), AvisVocal::Rejoint("v1".into()));
        alice.session.choisir_micro(Some("casque".into()));
        assert_eq!(
            avis.recv().await.unwrap(),
            AvisVocal::Peripherique(SensAudio::Entree, Bascule::Choix("casque".into()))
        );
        assert_eq!(alice.session.instantane().micro.as_deref(), Some("casque"));
        alice.audio.debrancher("casque");
        assert_eq!(
            avis.recv().await.unwrap(),
            AvisVocal::Peripherique(SensAudio::Entree, Bascule::Repli("micro".into()))
        );
        alice
            .audio
            .pousser("micro", &[0.5; ECHANTILLONS_PAR_TRAME + 1]);
        assert_eq!(serveur.trames_recues("alice"), 1);
    }

    async fn entendre(poste: &Poste, attendu: f32) {
        attendre_que(|| {
            poste
                .audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().skip(4).all(|e| (e - attendu).abs() < 0.01))
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn le_volume_d_un_participant_est_retrouve_apres_avoir_quitte_puis_rejoint() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        bob.session.rejoindre("v1");
        attendre(&alice.session, |e| e.participants.len() == 2).await;
        alice.session.definir_volume("bob", SourceAudio::Micro, 50);
        alice.session.definir_volume_general(80);
        attendre(&alice.session, |e| {
            e.ecoute.volume("bob", SourceAudio::Micro) == 50
        })
        .await;
        let parler = || {
            bob.audio
                .pousser("micro", &[0.5; 4 * ECHANTILLONS_PAR_TRAME])
        };
        attendre_que(|| {
            parler();
            alice
                .audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().any(|e| (e - 0.2).abs() < 0.01))
        })
        .await;
        alice.session.quitter();
        attendre(&alice.session, |e| !e.actif()).await;
        alice.session.rejoindre("v1");
        let etat = attendre(&alice.session, |e| e.participants.len() == 2).await;
        assert_eq!(etat.ecoute.volume("bob", SourceAudio::Micro), 50);
        attendre_que(|| {
            parler();
            alice
                .audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().any(|e| (e - 0.2).abs() < 0.01))
        })
        .await;
        alice.session.definir_muet("bob", SourceAudio::Micro, true);
        attendre(&alice.session, |e| e.ecoute.muet("bob", SourceAudio::Micro)).await;
        parler();
        alice.audio.tirer("hp", 4 * ECHANTILLONS_PAR_TRAME);
        parler();
        entendre(&alice, 0.0).await;
    }

    #[tokio::test(start_paused = true)]
    async fn la_detection_de_voix_envoie_du_silence_sous_le_seuil_et_mesure_toujours() {
        let serveur = ServeurVocalFactice::nouveau();
        let mut reglages = sans_bruit();
        reglages.traitement.detection_voix = true;
        reglages.traitement.seuil = -30;
        let alice = poste_regle(&serveur, "alice", true, reglages);
        let bob = poste(&serveur, "bob", true);
        alice.session.rejoindre("v1");
        bob.session.rejoindre("v1");
        attendre(&bob.session, |e| e.participants.len() == 2).await;
        assert_eq!(
            alice.session.niveau_micro().db,
            crate::audio::micro::SILENCE_DB
        );
        let faible = || {
            alice
                .audio
                .pousser("micro", &[0.01; 4 * ECHANTILLONS_PAR_TRAME])
        };
        faible();
        let niveau = alice.session.niveau_micro();
        assert!(!niveau.voix && (niveau.db + 40.0).abs() < 0.5, "{niveau:?}");
        attendre_que(|| {
            faible();
            bob.audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().all(|e| *e == 0.0))
        })
        .await;
        alice
            .audio
            .pousser("micro", &[0.5; 4 * ECHANTILLONS_PAR_TRAME]);
        assert!(alice.session.niveau_micro().voix);
        attendre_que(|| {
            alice
                .audio
                .pousser("micro", &[0.5; 2 * ECHANTILLONS_PAR_TRAME]);
            bob.audio
                .tirer("hp", 2 * ECHANTILLONS_PAR_TRAME)
                .is_some_and(|s| s.iter().any(|e| *e > 0.4))
        })
        .await;
        alice.session.basculer_micro();
        attendre(&alice.session, |e| e.micro_coupe).await;
        let avant = serveur.trames_recues("alice");
        alice
            .audio
            .pousser("micro", &[0.25; 2 * ECHANTILLONS_PAR_TRAME]);
        assert_eq!(serveur.trames_recues("alice"), avant);
        assert!((alice.session.niveau_micro().db + 12.04).abs() < 0.1);
        alice.session.definir_traitement(ReglagesMicro {
            gain: 900,
            ..ReglagesMicro::default()
        });
        let etat = attendre(&alice.session, |e| e.traitement.gain != 100).await;
        assert_eq!(
            etat.traitement.gain,
            crate::audio::micro::GAIN_MICRO_MAXIMAL
        );
        alice.session.quitter();
        let etat = attendre(&alice.session, |e| !e.actif()).await;
        assert_eq!(
            etat.traitement.gain,
            crate::audio::micro::GAIN_MICRO_MAXIMAL
        );
        assert_eq!(
            alice.session.niveau_micro().db,
            crate::audio::micro::SILENCE_DB
        );
    }

    fn source_de(poste: &Poste, id: &str) -> SourceCapture {
        use estompe_plateforme::CaptureEcran;
        poste
            .ecrans
            .sources()
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
    }

    async fn partage_pret(serveur: &ServeurVocalFactice) -> (Poste, Poste, CleVideo) {
        let alice = poste(serveur, "alice", true);
        let bob = poste(serveur, "bob", true);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        bob.session.rejoindre("v1");
        attendre(&bob.session, |e| e.participants.contains_key("alice")).await;
        (alice, bob, CleVideo::partage("alice"))
    }

    #[tokio::test]
    async fn publier_changer_de_source_et_de_qualite_puis_arreter_un_partage() {
        let serveur = ServeurVocalFactice::nouveau();
        let (alice, bob, cle) = partage_pret(&serveur).await;
        alice.session.partager(source_de(&alice, "ecran:0"));
        let etat = attendre(&alice.session, |e| e.partage.is_some()).await;
        let partage = etat.partage.unwrap();
        assert_eq!(partage.qualite.resolution, ResolutionPartage::R1080);
        assert_eq!(partage.qualite.ips, 30);
        let format = serveur
            .format_publie("v1", "alice", SourceVideo::Partage)
            .unwrap();
        assert_eq!(
            (format.hauteur, format.images_par_seconde, format.debit),
            (1080, 30, 3_000_000)
        );
        assert!(format.degradation == Degradation::GarderCadence && !format.simulcast);
        assert_eq!(alice.ecrans.ouverts(), vec![("ecran:0".to_string(), 30)]);
        assert_eq!(
            etat.pistes.get(&cle).map(String::as_str),
            Some("TR_alice_ecran")
        );
        bob.session.regarder("alice", Some(true));
        bob.session.definir_affichage(
            [(
                cle.clone(),
                DemandeVideo {
                    largeur: 640,
                    hauteur: 360,
                },
            )]
            .into_iter()
            .collect(),
        );
        attendre(&bob.session, |e| e.video(&cle)).await;
        let chez_bob = bob.session.videos().recepteur(&cle);
        let chez_moi = alice.session.videos().recepteur(&cle);
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Partage)
                .is_empty()
        })
        .await;
        alice.ecrans.pousser("ecran:0");
        attendre_que(|| chez_bob.images_recues() >= 1 && chez_moi.images_recues() >= 1).await;
        assert_eq!(
            serveur.dimensions_publiees("v1", "alice", SourceVideo::Partage),
            Some((1920, 1080))
        );

        alice.session.changer_source(source_de(&alice, "fenetre:7"));
        attendre(&alice.session, |e| {
            e.partage
                .as_ref()
                .is_some_and(|p| p.source.id == "fenetre:7")
        })
        .await;
        assert_eq!(alice.ecrans.ouverts(), vec![("fenetre:7".to_string(), 30)]);
        alice.ecrans.pousser("fenetre:7");
        attendre_que(|| chez_bob.images_recues() >= 2).await;
        assert_eq!(
            serveur.dimensions_publiees("v1", "alice", SourceVideo::Partage),
            Some((1152, 720))
        );
        assert!(serveur.video_publiee("v1", "alice", SourceVideo::Partage));

        alice.session.changer_source(source_de(&alice, "ecran:0"));
        attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.source.id == "ecran:0")
        })
        .await;
        alice.session.regler_partage(ReglagesPartage {
            resolution: ResolutionPartage::R720,
            ips: 60,
            ..ReglagesPartage::default()
        });
        attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.qualite.ips == 60)
        })
        .await;
        assert_eq!(alice.ecrans.ouverts(), vec![("ecran:0".to_string(), 60)]);
        let avant = chez_bob.images_recues();
        alice.ecrans.pousser("ecran:0");
        attendre_que(|| chez_bob.images_recues() > avant).await;
        assert_eq!(
            serveur.dimensions_publiees("v1", "alice", SourceVideo::Partage),
            Some((1280, 720))
        );

        alice.session.arreter_partage();
        attendre(&alice.session, |e| {
            e.partage.is_none() && !e.pistes.contains_key(&cle)
        })
        .await;
        assert!(alice.ecrans.ouverts().is_empty());
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Partage));
        attendre(&bob.session, |e| !e.video(&cle)).await;
        assert!(chez_moi.derniere().is_none());
        alice.session.fermer().await;
        bob.session.fermer().await;
    }

    #[tokio::test]
    async fn la_source_qui_disparait_arrete_le_partage_avec_un_message() {
        let serveur = ServeurVocalFactice::nouveau();
        let (alice, bob, cle) = partage_pret(&serveur).await;
        alice.session.partager(source_de(&alice, "fenetre:7"));
        attendre(&alice.session, |e| e.partage.is_some()).await;
        attendre(&bob.session, |e| e.video(&cle)).await;
        alice.ecrans.reduire("fenetre:7");
        attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.reduite)
        })
        .await;
        alice.ecrans.retirer("fenetre:7");
        let etat = attendre(&alice.session, |e| e.partage.is_none()).await;
        assert_eq!(
            etat.partage_erreur.as_deref(),
            Some("Partage arrêté : la fenêtre partagée a été fermée.")
        );
        assert!(!serveur.video_publiee("v1", "alice", SourceVideo::Partage));
        attendre(&bob.session, |e| !e.video(&cle)).await;
        alice.session.partager(source_de(&alice, "ecran:0"));
        let etat = attendre(&alice.session, |e| e.partage.is_some()).await;
        assert_eq!(etat.partage_erreur, None);
        alice.ecrans.echouer("ecran:0", "périphérique perdu");
        let etat = attendre(&alice.session, |e| e.partage.is_none()).await;
        assert_eq!(
            etat.partage_erreur.as_deref(),
            Some("Partage arrêté : la capture de l'écran a échoué.")
        );
        alice.session.fermer().await;
        bob.session.fermer().await;
    }

    #[tokio::test]
    async fn le_plafond_du_serveur_et_la_taille_de_la_source_bornent_le_partage() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        alice.api.plafonner(Some(PlafondPartage {
            ips: 30,
            hauteur: 1440,
        }));
        alice.session.regler_partage(ReglagesPartage {
            resolution: ResolutionPartage::R2160,
            ips: 120,
            ..ReglagesPartage::default()
        });
        alice.session.partager(source_de(&alice, "ecran:0"));
        let etat = attendre(&alice.session, |e| e.reglages_partage.ips == 120).await;
        assert!(etat.partage.is_none(), "rien ne se partage hors d'un salon");
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.partager(source_de(&alice, "fenetre:7"));
        let etat = attendre(&alice.session, |e| e.partage.is_some()).await;
        assert_eq!(
            etat.plafond_partage,
            PlafondPartage {
                ips: 30,
                hauteur: 1440
            }
        );
        let q = etat.partage.unwrap().qualite;
        assert_eq!((q.resolution, q.ips), (ResolutionPartage::R720, 30));
        alice.session.changer_source(source_de(&alice, "ecran:0"));
        let etat = attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.source.id == "ecran:0")
        })
        .await;
        assert_eq!(
            etat.partage.unwrap().qualite.resolution,
            ResolutionPartage::R1440
        );
        alice.session.quitter();
        let etat = attendre(&alice.session, |e| !e.actif()).await;
        assert!(etat.partage.is_none());
        assert!(alice.ecrans.ouverts().is_empty());
        alice.session.fermer().await;
    }

    #[tokio::test(start_paused = true)]
    async fn le_partage_baisse_d_un_palier_quand_la_connexion_ne_suit_pas() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste(&serveur, "alice", true);
        let mut avis = alice.session.abonner_avis();
        alice.session.regler_partage(ReglagesPartage {
            resolution: ResolutionPartage::R1440,
            ips: 60,
            fluidite: true,
            ..ReglagesPartage::default()
        });
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.partager(source_de(&alice, "ecran:0"));
        attendre(&alice.session, |e| e.partage.is_some()).await;
        serveur.limiter("alice", Limitation::Debit);
        let etat = attendre(&alice.session, |e| {
            e.partage
                .as_ref()
                .is_some_and(|p| p.qualite.resolution == ResolutionPartage::R1080)
        })
        .await;
        assert_eq!(etat.partage.unwrap().qualite.ips, 60);
        loop {
            if let AvisVocal::PartageReduit(q) = avis.recv().await.unwrap() {
                assert_eq!(q.resolution, ResolutionPartage::R1080);
                break;
            }
        }
        serveur.limiter("alice", Limitation::Aucune);
        alice.session.regler_partage(ReglagesPartage {
            resolution: ResolutionPartage::R1440,
            ips: 60,
            fluidite: true,
            auto: false,
            ..ReglagesPartage::default()
        });
        attendre(&alice.session, |e| {
            e.partage
                .as_ref()
                .is_some_and(|p| p.qualite.resolution == ResolutionPartage::R1440)
        })
        .await;
        serveur.limiter("alice", Limitation::Processeur);
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert_eq!(
            alice
                .session
                .instantane()
                .partage
                .unwrap()
                .qualite
                .resolution,
            ResolutionPartage::R1440,
            "sans réduction automatique, la qualité choisie est gardée"
        );
        alice.session.fermer().await;
    }

    fn pixel_du_partage(recepteur: &RecepteurVideo, x: u32, y: u32) -> [u8; 4] {
        let image = recepteur.derniere().unwrap();
        let (l, h) = (image.largeur(), image.hauteur());
        let mut rgba = vec![0u8; (l * h * 4) as usize];
        image.vers_rgba(l, h, &mut rgba);
        let i = ((y * l + x) * 4) as usize;
        rgba[i..i + 4].try_into().unwrap()
    }

    #[tokio::test]
    async fn la_camera_est_incrustee_dans_le_partage_puis_disparait_quand_elle_est_coupee() {
        let serveur = ServeurVocalFactice::nouveau();
        let (alice, bob, cle) = partage_pret(&serveur).await;
        let camera_alice = CleVideo::camera("alice");
        alice.session.activer_camera(true);
        attendre(&alice.session, |e| e.camera_active).await;
        attendre(&bob.session, |e| e.video(&camera_alice)).await;
        let demande = DemandeVideo {
            largeur: 640,
            hauteur: 360,
        };
        bob.session
            .definir_affichage([(camera_alice.clone(), demande)].into_iter().collect());
        attendre_que(|| {
            serveur
                .abonnes_video("v1", "alice", SourceVideo::Camera)
                .contains_key("bob")
        })
        .await;
        let reglages = ReglagesIncrustation {
            active: true,
            ..ReglagesIncrustation::default()
        };
        alice.session.regler_incrustation(reglages);
        alice.session.partager(source_de(&alice, "fenetre:7"));
        attendre(&alice.session, |e| e.partage.is_some() && e.incrustation).await;
        assert_eq!(serveur.attributs("v1", "alice")["overlay"], "1");
        attendre(&bob.session, |e| e.camera_incrustee("alice")).await;
        attendre_que(|| {
            !serveur
                .abonnes_video("v1", "alice", SourceVideo::Camera)
                .contains_key("bob")
        })
        .await;

        alice.cameras.pousser(1);
        attendre_que(|| alice.session.compositeur().a_une_camera()).await;
        let chez_moi = alice.session.videos().recepteur(&cle);
        let avant = chez_moi.images_recues();
        alice.ecrans.pousser("fenetre:7");
        attendre_que(|| chez_moi.images_recues() > avant).await;
        let zone = zone_incrustation(&reglages, 1280, 800, 16.0 / 9.0);
        let (cx, cy) = (
            (zone.x + zone.largeur as i32 / 2) as u32,
            (zone.y + zone.hauteur as i32 / 2) as u32,
        );
        let ecran = pixel_du_partage(&chez_moi, 10, 10);
        assert_eq!(ecran[1], 128, "hors du médaillon, l'écran : {ecran:?}");
        let medaillon = pixel_du_partage(&chez_moi, cx, cy);
        assert!(
            medaillon[0] < 20 && medaillon[1] > 170 && medaillon[2] < 20,
            "dans le médaillon, la barre verte de la mire : {medaillon:?}"
        );

        alice.cameras.pousser(1);
        let figee = chez_moi.images_recues();
        attendre_que(|| chez_moi.images_recues() > figee + 2).await;

        alice.session.activer_camera(false);
        let etat = attendre(&alice.session, |e| !e.camera_active && !e.incrustation).await;
        assert!(etat.partage.is_some(), "le partage continue sans la caméra");
        attendre_que(|| pixel_du_partage(&chez_moi, cx, cy) == ecran).await;
        assert_eq!(serveur.attributs("v1", "alice")["overlay"], "");
        assert!(serveur.video_publiee("v1", "alice", SourceVideo::Partage));
        alice.session.fermer().await;
        bob.session.fermer().await;
    }

    #[tokio::test]
    async fn le_son_de_la_fenetre_partagee_est_publie_sans_celui_d_estompe_pour_un_ecran() {
        let serveur = ServeurVocalFactice::nouveau();
        let (alice, bob, _) = partage_pret(&serveur).await;
        alice.session.partager(source_de(&alice, "fenetre:7"));
        let etat = attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.son)
        })
        .await;
        assert_eq!(etat.partage_erreur, None);
        assert_eq!(alice.sons.ouverts(), vec![CibleSon::Application(4242)]);
        assert!(serveur.son_partage_publie("v1", "alice"));
        attendre(&bob.session, |e| e.sons_partage.contains("alice")).await;
        bob.session.regarder("alice", Some(true));
        attendre_que(|| serveur.abonnes_son_partage("v1", "alice").contains("bob")).await;
        let format = FormatAudio {
            frequence: 44_100,
            canaux: 2,
        };
        alice.sons.pousser(&[0.25; 2 * 441 * 3], format);
        assert!(serveur.trames_son_partage("alice") >= 2);

        alice.session.changer_source(source_de(&alice, "ecran:0"));
        attendre(&alice.session, |e| {
            e.partage
                .as_ref()
                .is_some_and(|p| p.source.id == "ecran:0" && p.son)
        })
        .await;
        assert_eq!(
            alice.sons.ouverts(),
            vec![CibleSon::SystemeSauf(std::process::id())]
        );

        alice.session.regler_partage(ReglagesPartage {
            son: false,
            ..ReglagesPartage::default()
        });
        attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| !p.son)
        })
        .await;
        assert!(alice.sons.ouverts().is_empty());
        assert!(!serveur.son_partage_publie("v1", "alice"));
        attendre(&bob.session, |e| !e.sons_partage.contains("alice")).await;
        alice.session.regler_partage(ReglagesPartage::default());
        attendre(&alice.session, |e| {
            e.partage.as_ref().is_some_and(|p| p.son)
        })
        .await;
        alice.session.arreter_partage();
        attendre(&alice.session, |e| e.partage.is_none()).await;
        assert!(alice.sons.ouverts().is_empty());
        assert!(!serveur.son_partage_publie("v1", "alice"));
        alice.session.fermer().await;
        bob.session.fermer().await;
    }

    #[tokio::test]
    async fn sans_capture_du_son_le_partage_part_sans_le_son() {
        let serveur = ServeurVocalFactice::nouveau();
        let alice = poste_avec(&serveur, "alice", true, sans_bruit(), false);
        alice.session.rejoindre("v1");
        attendre(&alice.session, connecte_a("v1")).await;
        alice.session.partager(source_de(&alice, "fenetre:7"));
        let etat = attendre(&alice.session, |e| e.partage.is_some()).await;
        assert!(!etat.partage.unwrap().son && etat.partage_erreur.is_none());
        assert!(!serveur.son_partage_publie("v1", "alice"));
        alice.session.fermer().await;

        let bob = poste(&serveur, "bob", true);
        bob.sons.refuser(Some("Windows trop ancien"));
        bob.session.rejoindre("v1");
        attendre(&bob.session, connecte_a("v1")).await;
        bob.session.partager(source_de(&bob, "ecran:0"));
        let etat = attendre(&bob.session, |e| {
            e.partage.is_some() && e.partage_erreur.is_some()
        })
        .await;
        assert_eq!(
            etat.partage_erreur.as_deref(),
            Some("Partage sans le son : capture du son impossible.")
        );
        assert!(!etat.partage.unwrap().son);
        assert!(!serveur.son_partage_publie("v1", "bob"));
        assert!(serveur.video_publiee("v1", "bob", SourceVideo::Partage));
        bob.session.fermer().await;
    }
}
