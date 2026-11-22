use std::time::{Duration, Instant};

use estompe_client::replay::automate::Pause;
use estompe_client::replay::galerie;
use estompe_client::replay::types::{ClipEnregistre, Resolution};
use estompe_plateforme::{
    application_de_chemin, ApplicationPremierPlan, CibleSon, EcransFactices, EncodageFactice,
    FormatAudio, PremierPlanFactice, RaccourcisFactices, SonsFactices, SourceCapture, TypeSource,
};

use super::*;

const PID: u32 = 4242;
const FENETRE: u64 = 77;
const SOURCE: &str = "fenetre:77";

struct Banc {
    service: Arc<ServiceReplay>,
    encodage: EncodageFactice,
    ecrans: EcransFactices,
    sons: SonsFactices,
    raccourcis: Arc<RaccourcisFactices>,
    premier_plan: PremierPlanFactice,
    evenements: broadcast::Receiver<EvenementReplay>,
    dossier: tempfile::TempDir,
}

fn application() -> ApplicationPremierPlan {
    ApplicationPremierPlan {
        processus: PID,
        fenetre: FENETRE,
        plein_ecran: true,
        ..application_de_chemin(r"C:\Jeux\test.exe")
    }
}

fn jeu() -> JeuDetecte {
    JeuDetecte {
        nom: "Jeu: test".into(),
        application: application(),
    }
}

fn reglages() -> ReglagesMoteur {
    ReglagesMoteur {
        replay: ReglagesReplay {
            actif: true,
            ips: 30,
            duree_s: 15,
            resolution: Resolution::R720,
            ..ReglagesReplay::default()
        },
        ..ReglagesMoteur::default()
    }
}

fn banc(encodage: EncodageFactice, prises: Vec<String>) -> Banc {
    let ecrans = EcransFactices::nouveaux();
    ecrans.brancher(SourceCapture {
        id: SOURCE.into(),
        nom: "Jeu".into(),
        application: "test.exe".into(),
        genre: TypeSource::Fenetre,
        largeur: 640,
        hauteur: 360,
        principale: false,
        processus: Some(PID),
        estompe: false,
    });
    let sons = SonsFactices::nouveaux();
    let raccourcis = Arc::new(RaccourcisFactices::nouveaux(true));
    let premier_plan = PremierPlanFactice::nouveau();
    premier_plan.ouvrir(vec![application()]);
    let dossier = tempfile::tempdir().unwrap();
    let service = ServiceReplay::demarrer(
        Dependances {
            encodage: Arc::new(encodage.clone()),
            ecrans: Arc::new(ecrans.clone()),
            sons: Arc::new(sons.clone()),
            micro: None,
            premier_plan: Arc::new(premier_plan.clone()),
            raccourcis: raccourcis.clone(),
            touches_prises: Arc::new(move || prises.clone()),
            videos: Some(dossier.path().to_path_buf()),
            journaux: Some(dossier.path().join("journaux")),
            version: "1.0.0".into(),
            horloge: Some(Arc::new(|| Horodatage::depuis_epoque(1_794_441_600))),
        },
        reglages(),
    );
    let evenements = service.evenements();
    Banc {
        service,
        encodage,
        ecrans,
        sons,
        raccourcis,
        premier_plan,
        evenements,
        dossier,
    }
}

fn attendre(mut condition: impl FnMut() -> bool) {
    let fin = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < fin, "délai dépassé");
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl Banc {
    fn armer(&self) {
        self.service.definir_jeu(Some(jeu()));
        attendre(|| {
            self.ecrans.pousser(SOURCE);
            self.service.actuel().format.is_some()
        });
    }

    fn filmer(&self, duree: Duration) {
        let fin = Instant::now() + duree;
        let stereo = FormatAudio {
            frequence: 48_000,
            canaux: 2,
        };
        while Instant::now() < fin {
            self.ecrans.pousser(SOURCE);
            self.sons.pousser(&[0.1; 2 * 480], stereo);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn evenement(&mut self) -> EvenementReplay {
        let mut recu = None;
        attendre(|| {
            recu = self.evenements.try_recv().ok();
            recu.is_some()
        });
        recu.unwrap()
    }

    fn clip(&mut self) -> ClipEnregistre {
        loop {
            match self.evenement() {
                EvenementReplay::Clip(clip) => return clip,
                EvenementReplay::Erreur(e) if e.code == "capture-en-retard" => {}
                autre => panic!("évènement inattendu : {autre:?}"),
            }
        }
    }

    fn erreur(&mut self) -> ErreurReplay {
        match self.evenement() {
            EvenementReplay::Erreur(e) => e,
            autre => panic!("évènement inattendu : {autre:?}"),
        }
    }
}

#[test]
fn le_jeu_detecte_arme_le_tampon_et_le_raccourci_sauve_un_clip() {
    let mut b = banc(EncodageFactice::nouveau(), Vec::new());
    assert!(b.service.disponibilite().disponible);
    b.armer();
    let etat = b.service.actuel();
    assert_eq!(etat.etat, Etat::Arme);
    assert_eq!(etat.jeu.as_deref(), Some("Jeu: test"));
    let format = etat.format.unwrap();
    assert_eq!((format.largeur, format.hauteur, format.ips), (640, 360, 30));
    assert_eq!(etat.raccourci.as_deref(), Some("F10"));
    assert_eq!(b.raccourcis.enregistres(), vec!["F10"]);
    assert_eq!(b.sons.ouverts(), vec![CibleSon::Application(PID)]);
    b.filmer(Duration::from_millis(1200));
    b.raccourcis.appuyer("F10");
    let clip = b.clip();
    let dossier = b.dossier.path().join(galerie::SOUS_DOSSIER);
    assert_eq!(
        clip.chemin,
        dossier.join("Jeu test 2026-11-12 00-00-00.mp4")
    );
    assert_eq!(b.service.dossier(), Ok(dossier.clone()));
    let octets = std::fs::read(&clip.chemin).unwrap();
    assert_eq!(&octets[4..8], b"ftyp");
    assert_eq!(clip.taille_octets, octets.len() as u64);
    assert!(
        clip.duree_s > 0.5 && clip.duree_s < 15.0,
        "{}",
        clip.duree_s
    );
    let annexe = galerie::lire_annexe(&dossier, &clip.chemin).unwrap();
    assert_eq!(
        (annexe.jeu.as_str(), annexe.ips, annexe.pistes_audio),
        ("Jeu: test", 30, 1)
    );
    assert_eq!(b.encodage.videos()[0].images_par_groupe, 30);
    b.service.sauvegarder_les_dernieres(1);
    let court = b.clip();
    assert!(
        court.duree_s > 0.0 && court.duree_s < 2.1,
        "{}",
        court.duree_s
    );
    assert_eq!(galerie::lister(&dossier).len(), 2);
}

#[test]
fn pause_hors_du_premier_plan_puis_tampon_vide_a_la_fermeture() {
    let mut b = banc(EncodageFactice::nouveau(), Vec::new());
    b.armer();
    b.filmer(Duration::from_millis(400));
    b.service.definir_jeu(None);
    attendre(|| b.service.actuel().etat == Etat::Pause(Pause::PremierPlan));
    assert!(b.service.actuel().format.is_none());
    b.service.sauvegarder();
    assert!(matches!(b.clip().duree_s, d if d > 0.0));
    b.premier_plan.ouvrir(Vec::new());
    b.service.definir_jeu(None);
    attendre(|| b.service.actuel().etat == Etat::Inactif);
    b.service.sauvegarder();
    assert_eq!(b.erreur().code, "tampon-vide");
}

#[test]
fn sans_encodage_materiel_le_replay_est_indisponible_avec_sa_raison() {
    let mut b = banc(EncodageFactice::avec(None), Vec::new());
    let d = b.service.disponibilite();
    assert_eq!(
        (d.disponible, d.raison.as_deref()),
        (false, Some(HORS_WINDOWS))
    );
    b.service.definir_jeu(Some(jeu()));
    attendre(|| b.service.actuel().etat == Etat::Indisponible(HORS_WINDOWS.into()));
    assert!(b.raccourcis.enregistres().is_empty());
    assert!(b.ecrans.ouverts().is_empty());
    assert!(b.service.options(&ReglagesReplay::default()).is_err());
    b.service.sauvegarder();
    assert_eq!(b.erreur().code, "tampon-vide");
}

#[test]
fn une_touche_deja_prise_n_est_pas_enregistree() {
    let b = banc(EncodageFactice::nouveau(), vec!["F10".into()]);
    b.armer();
    assert!(b.raccourcis.enregistres().is_empty());
    assert_eq!(b.service.actuel().raccourci, None);
    b.service.configurer(ReglagesMoteur {
        replay: ReglagesReplay {
            touche: "F9".into(),
            ..reglages().replay
        },
        ..reglages()
    });
    attendre(|| b.raccourcis.enregistres() == vec!["F9"]);
    b.service.configurer(ReglagesMoteur {
        replay: ReglagesReplay {
            actif: false,
            ..reglages().replay
        },
        ..reglages()
    });
    attendre(|| b.service.actuel().etat == Etat::Inactif);
    assert!(b.raccourcis.enregistres().is_empty());
    assert!(b.ecrans.ouverts().is_empty());
}

#[test]
fn un_encodeur_muet_rend_le_replay_indisponible() {
    let encodage = EncodageFactice::nouveau();
    encodage.rendre_muet(true);
    let mut b = banc(encodage, Vec::new());
    b.armer();
    let fin = Instant::now() + Duration::from_secs(10);
    let erreur = loop {
        b.filmer(Duration::from_millis(50));
        if let Ok(EvenementReplay::Erreur(e)) = b.evenements.try_recv() {
            break e;
        }
        assert!(Instant::now() < fin, "délai dépassé");
    };
    assert_eq!(erreur.code, ENCODEUR_MUET);
    assert!(matches!(b.service.actuel().etat, Etat::Indisponible(_)));
    assert!(b.encodage.images() > 0);
}

#[test]
fn un_encodeur_arrete_est_signale() {
    let mut b = banc(EncodageFactice::nouveau(), Vec::new());
    b.armer();
    b.encodage.arreter("pilote perdu");
    let e = b.erreur();
    assert_eq!(e.code, "encodeur-arrete");
    assert!(e.message.contains("pilote perdu"));
    let options = b.service.options(&ReglagesReplay::default()).unwrap();
    assert_eq!(options.codec, estompe_client::replay::types::Codec::H264);
}

fn vocal_partageant(actif: bool) -> estompe_client::vocal::EtatVocal {
    use estompe_client::vocal::{PartageLocal, QualitePartage, ResolutionPartage};
    let mut etat = estompe_client::vocal::EtatVocal::default();
    etat.partage = actif.then(|| PartageLocal {
        source: SourceCapture {
            id: "ecran:1".into(),
            nom: "Écran".into(),
            application: String::new(),
            genre: TypeSource::Ecran,
            largeur: 1920,
            hauteur: 1080,
            principale: true,
            processus: None,
            estompe: false,
        },
        qualite: QualitePartage {
            resolution: ResolutionPartage::R1080,
            ips: 30,
        },
        reduite: false,
        son: false,
    });
    etat
}

#[test]
fn pendant_un_partage_d_ecran_un_echec_met_le_replay_en_pause_au_lieu_de_l_arreter() {
    assert!(!partage_actif(&vocal_partageant(false)));
    assert!(partage_actif(&vocal_partageant(true)));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let encodage = EncodageFactice::nouveau();
    encodage.rendre_muet(true);
    let b = banc(encodage, Vec::new());
    let (vocal, recepteur) = watch::channel(vocal_partageant(false));
    b.service.suivre_le_partage(runtime.handle(), recepteur);
    vocal.send_replace(vocal_partageant(true));
    runtime.block_on(async {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    });
    b.armer();
    let fin = Instant::now() + Duration::from_secs(10);
    while b.service.actuel().etat != Etat::Pause(Pause::Partage) {
        assert!(
            !matches!(b.service.actuel().etat, Etat::Indisponible(_)),
            "le partage d'écran est prioritaire : pas d'arrêt"
        );
        b.filmer(Duration::from_millis(50));
        assert!(Instant::now() < fin, "délai dépassé");
    }
    vocal.send_replace(vocal_partageant(false));
    runtime.block_on(async {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    });
    attendre(|| b.service.actuel().etat != Etat::Pause(Pause::Partage));
}
