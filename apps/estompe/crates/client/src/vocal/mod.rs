pub mod erreurs;
pub mod etat;
pub mod factice;
#[cfg(feature = "livekit")]
pub mod livekit;
pub mod mixage;
pub mod ptt;
pub mod salle;
pub mod session;

pub use erreurs::{CauseMicro, ErreurVocal};
pub use etat::{etats_affiches, EtatAffiche, EtatVocal, Phase};
pub use mixage::{Ecoute, Limiteur, Mixeur};
pub use ptt::{EtatPtt, PousserPourParler, ReglagesPtt, SourceTouche};
pub use salle::{
    Connecteur, ConnecteurIndisponible, Connexion, ErreurSalle, EvenementSalle, InfoParticipant,
    JetonSalle, Qualite, RaisonDeconnexion, Salle, SortieMicro, SourceAudio,
};
pub use session::{ApiVocale, AvisVocal, Dependances, Reglages, SessionVocale, DELAI_FERMETURE};

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use estompe_plateforme::{FormatAudio, SensAudio};
    use tokio::runtime::Handle;

    use super::factice::{ApiVocaleFactice, ServeurVocalFactice};
    use super::*;
    use crate::audio::factice::SystemeFactice;
    use crate::audio::{Bascule, ModeBruit, ReglagesMicro, ECHANTILLONS_PAR_TRAME};

    struct Poste {
        audio: SystemeFactice,
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
        let audio = audio(avec_micro);
        let api = Arc::new(ApiVocaleFactice::nouvelle(identite));
        let session = SessionVocale::demarrer(
            &Handle::current(),
            Dependances {
                api: api.clone(),
                connecteur: Arc::new(serveur.clone()),
                audio: Arc::new(audio.clone()),
                apm: None,
                reglages,
            },
        );
        Poste {
            audio,
            api,
            session,
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
}
