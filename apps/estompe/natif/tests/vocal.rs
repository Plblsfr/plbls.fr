mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use commun::coquille::{evenement, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{ChoixSon, FormatAudio, SensAudio};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::vocal::factice::{ApiVocaleFactice, ServeurVocalFactice};
use estompe_client::vocal::{
    CamerasFactices, CleVideo, Dependances, EvenementSalle, Mire, ReglagesPtt, SessionVocale,
    SourceAudio, SourceVideo,
};
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::vocal::OutilsVocaux;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesVideo, DonneesVocal, Page, ParticipantVocal, Principale,
};
use estompe_plateforme::RaccourcisFactices;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::json;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, SharedString};

const RETARD_MAXIMAL: Duration = Duration::from_millis(150);

fn cliquer(faible: &slint::Weak<Principale>, libelle: &'static str) {
    sur_l_interface(faible, move |f| {
        let element = ElementHandle::find_by_accessible_label(f, libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button) && e.size().height > 0.0)
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"));
        element.mock_single_click(PointerEventButton::Left);
    });
}

fn tuiles(f: &Principale) -> Vec<ParticipantVocal> {
    f.global::<DonneesVocal>().get_tuiles().iter().collect()
}

fn tuile(f: &Principale, utilisateur: &str) -> Option<ParticipantVocal> {
    tuiles(f).into_iter().find(|t| t.utilisateur == utilisateur)
}

fn presser(f: &Principale, texte: impl Into<SharedString>) {
    f.window()
        .dispatch_event(WindowEvent::KeyPressed { text: texte.into() });
}

fn relacher(f: &Principale, texte: impl Into<SharedString>) {
    f.window()
        .dispatch_event(WindowEvent::KeyReleased { text: texte.into() });
}

fn raccourci(faible: &slint::Weak<Principale>, lettre: &'static str) {
    sur_l_interface(faible, move |f| {
        presser(f, Key::Alt);
        presser(f, Key::Shift);
        presser(f, lettre);
        relacher(f, lettre);
        relacher(f, Key::Shift);
        relacher(f, Key::Alt);
    });
}

fn mesurer(
    faible: &slint::Weak<Principale>,
    condition: impl Fn(&Principale) -> bool + Send + Sync + Clone + 'static,
    quoi: &str,
) -> Duration {
    let debut = Instant::now();
    attendre_l_interface(faible, condition, quoi);
    debut.elapsed()
}

struct Banc {
    http: Arc<ServeurMessages>,
    serveur: ServeurVocalFactice,
    audio: SystemeFactice,
    api: Arc<ApiVocaleFactice>,
    session: Arc<SessionVocale>,
    visibilite: Visibilite,
    cameras: CamerasFactices,
}

type Visibilite = Arc<dyn Fn(&Principale, bool) + Send + Sync>;

struct Diffusion {
    arret: Arc<std::sync::atomic::AtomicBool>,
    fil: Option<std::thread::JoinHandle<()>>,
}

impl Diffusion {
    fn camera(serveur: &ServeurVocalFactice, identite: &'static str) -> Self {
        Self::source(serveur, identite, SourceVideo::Camera)
    }

    fn source(serveur: &ServeurVocalFactice, identite: &'static str, source: SourceVideo) -> Self {
        let arret = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fil = {
            let arret = arret.clone();
            let serveur = serveur.clone();
            std::thread::spawn(move || {
                let mire = Mire::nouvelle(1920, 1080);
                let mut n = 0;
                while !arret.load(std::sync::atomic::Ordering::SeqCst) {
                    serveur.pousser_image("v1", identite, source, Arc::new(mire.image(n)));
                    if source == SourceVideo::Partage {
                        serveur.pousser_son_partage("v1", identite, &[800; 480]);
                    }
                    n += 1;
                    std::thread::sleep(Duration::from_millis(33));
                }
            })
        };
        Self {
            arret,
            fil: Some(fil),
        }
    }
}

impl Drop for Diffusion {
    fn drop(&mut self) {
        self.arret.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

fn video_affichee(f: &Principale, utilisateur: &str) -> bool {
    tuile(f, utilisateur).is_some_and(|t| {
        t.video >= 0
            && f.global::<DonneesVideo>()
                .get_flux()
                .row_data(t.video as usize)
                .is_some_and(|l| l.pret && l.image.size().width > 0)
    })
}

fn requete_recue(serveur: &ServeurMessages, requete: &str, contenu: &str) -> bool {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .any(|(r, c)| r == requete && c.contains(contenu))
}

fn wav(echantillons: &[f32]) -> Vec<u8> {
    let donnees: Vec<u8> = echantillons
        .iter()
        .flat_map(|e| ((e * 32_767.0) as i16).to_le_bytes())
        .collect();
    let mut o = b"RIFF".to_vec();
    o.extend_from_slice(&(36 + donnees.len() as u32).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    for mot in [16u32, 1 | (1 << 16), 48_000, 96_000, 2 | (16 << 16)] {
        o.extend_from_slice(&mot.to_le_bytes());
    }
    o.extend_from_slice(b"data");
    o.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
    o.extend_from_slice(&donnees);
    o
}

fn centre(
    f: &Principale,
    libelle: &str,
    retenir: impl Fn(&ElementHandle) -> bool,
) -> LogicalPosition {
    let e = ElementHandle::find_by_accessible_label(f, libelle)
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button) && retenir(e))
        .unwrap_or_else(|| panic!("« {libelle} » introuvable"));
    let (position, taille) = (e.absolute_position(), e.size());
    LogicalPosition::new(
        position.x + taille.width / 2.0,
        position.y + taille.height / 2.0,
    )
}

fn glisser(faible: &slint::Weak<Principale>, qui: &'static str, vers: &'static str) {
    let (depart, arrivee) = sur_l_interface(faible, move |f| {
        (
            centre(f, qui, |e| (e.size().height - 30.0).abs() < 0.5),
            centre(f, vers, |e| e.size().height > 0.0),
        )
    });
    sur_l_interface(faible, move |f| {
        f.window().dispatch_event(WindowEvent::PointerPressed {
            position: depart,
            button: PointerEventButton::Left,
        });
        for pas in 1..=12 {
            let t = pas as f32 / 12.0;
            f.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(
                    depart.x + (arrivee.x - depart.x) * t,
                    depart.y + (arrivee.y - depart.y) * t,
                ),
            });
        }
        slint::platform::update_timers_and_animations();
    });
    attendre_l_interface(
        faible,
        |f| {
            slint::platform::update_timers_and_animations();
            f.global::<DonneesVocal>().get_glisse_depuis() == "v1"
        },
        "pendant le glisser, les autres salons vocaux sont proposés",
    );
    sur_l_interface(faible, move |f| {
        f.window().dispatch_event(WindowEvent::PointerReleased {
            position: arrivee,
            button: PointerEventButton::Left,
        });
    });
    attendre_l_interface(
        faible,
        |f| {
            slint::platform::update_timers_and_animations();
            f.global::<DonneesVocal>().get_glisse_depuis().is_empty()
        },
        "fin du glisser",
    );
}

fn voix_forte() -> Vec<f32> {
    (0..4_800)
        .map(|n| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / 48_000.0).sin())
        .collect()
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    banc: Banc,
    memoire: Memoire,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille,
        "coquille affichée",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<estompe_natif::DonneesCoquille>().get_vue().titre == "général",
        "dernière vue restaurée",
    );

    cliquer(&faible, "Salon vocal");
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_vue().ici && !v.get_vue().connexion && v.get_barre().visible
        },
        "c2 : rejoint en cliquant le salon vocal",
    );
    attendre(
        || banc.serveur.participants("v1").contains("u1"),
        "présent dans la salle",
    );
    attendre_l_interface(
        &faible,
        |f| tuiles(f).len() == 3,
        "une tuile par participant",
    );
    let (premiere, lieu, compte) = sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        let t = tuiles(f)[0].clone();
        (
            (t.moi, t.nom.to_string()),
            v.get_barre().lieu.to_string(),
            v.get_vue().compte.to_string(),
        )
    });
    assert!(premiere.0 && premiere.1 == "bob");
    attendre_l_interface(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| t.portrait.size().width > 0),
        "photo de profil de carol dans sa tuile",
    );
    assert!(fabrique
        .source
        .requetes_avatars()
        .iter()
        .any(|c| c.utilisateur == "carol" && c.serveur.is_none()));

    banc.serveur
        .publier_video("v1", "carol", SourceVideo::Camera);
    let diffusion = Diffusion::camera(&banc.serveur, "carol");
    attendre(
        || {
            banc.serveur
                .abonnes_video("v1", "carol", SourceVideo::Camera)
                .contains_key("u1")
        },
        "c334 : la caméra de carol, affichée, est demandée au serveur",
    );
    attendre_l_interface(
        &faible,
        |f| video_affichee(f, "carol"),
        "c334 : la caméra de carol s'affiche dans sa tuile",
    );
    let demande = banc
        .serveur
        .abonnes_video("v1", "carol", SourceVideo::Camera)["u1"];
    assert!(
        demande.largeur < 1920 && demande.hauteur < 1080,
        "{demande:?}"
    );
    let (largeur, libelle) = sur_l_interface(&faible, |f| {
        let t = tuile(f, "carol").unwrap();
        let l = f
            .global::<DonneesVideo>()
            .get_flux()
            .row_data(t.video as usize)
            .unwrap();
        (l.largeur, t.libelle_video.to_string())
    });
    assert!(largeur > 0 && largeur as u32 <= demande.largeur.max(1));
    assert_eq!(libelle, "caméra");
    let visibilite = banc.visibilite.clone();
    sur_l_interface(&faible, move |f| visibilite(f, false));
    attendre(
        || {
            banc.serveur
                .abonnes_video("v1", "carol", SourceVideo::Camera)
                .is_empty()
        },
        "c334 : fenêtre réduite, la caméra n'est plus demandée",
    );
    let recepteur = banc.session.videos().recepteur(&CleVideo::camera("carol"));
    let recues = recepteur.images_recues();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        recepteur.images_recues(),
        recues,
        "c334 : fenêtre réduite, plus aucune image reçue ni décodée"
    );
    let visibilite = banc.visibilite.clone();
    sur_l_interface(&faible, move |f| visibilite(f, true));
    attendre(
        || recepteur.images_recues() > recues,
        "c334 : fenêtre rouverte, les images reviennent",
    );
    attendre_l_interface(
        &faible,
        |f| video_affichee(f, "carol"),
        "c334 : la caméra de carol s'affiche de nouveau",
    );
    drop(diffusion);
    banc.serveur
        .retirer_video("v1", "carol", SourceVideo::Camera);
    attendre_l_interface(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| t.video < 0),
        "c334 : caméra coupée, la tuile revient à la photo de profil",
    );

    banc.serveur
        .publier_video("v1", "carol", SourceVideo::Partage);
    banc.serveur.publier_son_partage("v1", "carol");
    let diffusion = Diffusion::source(&banc.serveur, "carol", SourceVideo::Partage);
    attendre_l_interface(
        &faible,
        |f| {
            tuiles(f)
                .iter()
                .any(|t| t.partage && t.utilisateur == "carol" && !t.regarde && t.video < 0)
                && !f.global::<DonneesVocal>().get_vue().vedette
        },
        "c336 : le partage de carol a sa tuile « Regarder », sans mise en avant",
    );
    let partage = CleVideo::partage("carol");
    let recepteur = banc.session.videos().recepteur(&partage);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        banc.serveur
            .abonnes_video("v1", "carol", SourceVideo::Partage)
            .is_empty()
            && banc.serveur.abonnes_son_partage("v1", "carol").is_empty(),
        "c336 C2 : sans « Regarder », ni image ni son du partage demandés"
    );
    assert_eq!(recepteur.images_recues(), 0, "c336 C2 : aucune image reçue");
    cliquer(&faible, "Regarder");
    attendre(
        || {
            banc.serveur
                .abonnes_video("v1", "carol", SourceVideo::Partage)
                .contains_key("u1")
                && banc
                    .serveur
                    .abonnes_son_partage("v1", "carol")
                    .contains("u1")
        },
        "c336 : après « Regarder », image et son du partage sont demandés",
    );
    attendre_l_interface(
        &faible,
        |f| {
            tuiles(f).iter().any(|t| {
                t.partage
                    && t.vedette
                    && t.regarde
                    && t.video >= 0
                    && f.global::<DonneesVideo>()
                        .get_flux()
                        .row_data(t.video as usize)
                        .is_some_and(|l| l.pret)
            }) && f.global::<DonneesVocal>().get_vue().vedette
        },
        "c336 : le partage regardé s'affiche en grand",
    );
    attendre(
        || {
            banc.serveur
                .attributs("v1", "u1")
                .get("watching")
                .is_some_and(|w| w.contains("TR_carol_ecran"))
        },
        "c336 : le partage regardé est annoncé dans l'attribut watching",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .invoke_epingler("carol#screen".into())
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_vue().vedette,
        "c336 : « Détacher » remet le partage dans la grille",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .invoke_epingler("carol#screen".into())
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_vue().vedette,
        "c336 : « Mettre en grand » le remet en avant",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .invoke_plein_ecran("carol#screen".into())
    });
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_vue().plein_ecran == "carol#screen" && v.get_plein_cadre()
        },
        "c336 : plein écran de la tuile du partage",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_plein_ecran("".into())
    });
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_vue().plein_ecran.is_empty() && !v.get_plein_cadre()
        },
        "c336 : sortie du plein écran",
    );
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        v.invoke_ouvrir_menu_partage("carol".into());
        v.invoke_volume(40, true);
        v.invoke_choisir_qualite_recue(3);
    });
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_menu_ouvert() && v.get_menu().partage && v.get_menu().son_partage
        },
        "c336 : menu du partage avec son volume",
    );
    let session = banc.session.clone();
    attendre(
        move || {
            let e = session.instantane().ecoute;
            e.volume("carol", SourceAudio::Partage) == 40
                && e.volume("carol", SourceAudio::Micro) == 100
        },
        "c336 : le son du partage a son volume propre",
    );
    attendre(
        || {
            banc.serveur
                .abonnes_video("v1", "carol", SourceVideo::Partage)
                .get("u1")
                .is_some_and(|d| d.hauteur == 360)
        },
        "c336 : la qualité reçue choisie est demandée au serveur",
    );
    assert_eq!(
        memoire.lire(|p| p.qualite_partage),
        estompe_client::vocal::QualiteRecue::Basse
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().set_menu_ouvert(false);
    });
    cliquer(&faible, "Arrêter de regarder");
    attendre(
        || {
            banc.serveur
                .abonnes_video("v1", "carol", SourceVideo::Partage)
                .is_empty()
                && banc.serveur.abonnes_son_partage("v1", "carol").is_empty()
        },
        "c336 : « Arrêter de regarder » coupe image et son",
    );
    attendre_l_interface(
        &faible,
        |f| tuiles(f).iter().any(|t| t.partage && !t.regarde),
        "c336 : la tuile revient à « Regarder »",
    );
    drop(diffusion);
    banc.serveur.retirer_son_partage("v1", "carol");
    banc.serveur
        .retirer_video("v1", "carol", SourceVideo::Partage);
    attendre_l_interface(
        &faible,
        |f| !tuiles(f).iter().any(|t| t.partage),
        "c336 : partage arrêté, sa tuile disparaît",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_choisir_qualite_recue(0);
    });

    cliquer(&faible, "Activer la caméra");
    let cameras = banc.cameras.clone();
    attendre_l_interface(
        &faible,
        move |f| {
            cameras.pousser(1);
            let c = f.global::<DonneesVocal>().get_camera();
            c.ouvert
                && c.apercu >= 0
                && f.global::<DonneesVideo>()
                    .get_flux()
                    .row_data(c.apercu as usize)
                    .is_some_and(|l| l.pret)
        },
        "c335 : l'aperçu de la caméra s'affiche avant de l'activer",
    );
    assert_eq!(banc.cameras.ouvertes(), vec!["c1".to_string()]);
    assert!(!banc.serveur.video_publiee("v1", "u1", SourceVideo::Camera));
    let noms: Vec<String> = sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .get_cameras()
            .iter()
            .map(|c| c.nom.to_string())
            .collect()
    });
    assert_eq!(noms, ["Caméra intégrée", "Webcam USB"]);
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .invoke_choisir_camera("c2".into());
        f.global::<DonneesVocal>().invoke_choisir_qualite_camera(0);
    });
    attendre(
        || banc.cameras.ouvertes() == ["c2".to_string()],
        "c335 : l'aperçu passe sur la caméra choisie",
    );
    assert_eq!(
        memoire.lire(|p| (p.camera.clone(), p.qualite_camera)),
        (
            Some("c2".to_string()),
            estompe_client::vocal::QualiteCamera::Q360
        )
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_activer_camera();
    });
    attendre(
        || banc.serveur.video_publiee("v1", "u1", SourceVideo::Camera),
        "c335 : la caméra est publiée dans le salon",
    );
    assert_eq!(
        banc.serveur.format_publie("v1", "u1", SourceVideo::Camera),
        Some(estompe_client::vocal::QualiteCamera::Q360.format())
    );
    assert_eq!(banc.cameras.ouvertes(), vec!["c2".to_string()]);
    let cameras = banc.cameras.clone();
    attendre_l_interface(
        &faible,
        move |f| {
            cameras.pousser(1);
            !f.global::<DonneesVocal>().get_camera().ouvert
                && f.global::<DonneesVocal>().get_barre().camera_active
                && video_affichee(f, "u1")
        },
        "c335 : ma caméra s'affiche dans ma tuile",
    );
    assert!(
        banc.serveur
            .images_publiees("v1", "u1", SourceVideo::Camera)
            > 0
    );
    raccourci(&faible, "v");
    attendre(
        || !banc.serveur.video_publiee("v1", "u1", SourceVideo::Camera),
        "c335 : Alt+Maj+V coupe la caméra et la retire du salon",
    );
    assert!(banc.cameras.ouvertes().is_empty(), "caméra libérée");
    attendre_l_interface(
        &faible,
        |f| {
            !f.global::<DonneesVocal>().get_barre().camera_active
                && tuile(f, "u1").is_some_and(|t| t.video < 0)
        },
        "c335 : caméra coupée, ma tuile revient à ma photo",
    );
    banc.cameras.occuper("c2", true);
    cliquer(&faible, "Activer la caméra");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_camera().erreur
                == "Caméra déjà utilisée par une autre application."
        },
        "c335 : caméra prise par une autre application, message clair",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_fermer_camera();
    });
    banc.cameras.occuper("c2", false);
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_camera().ouvert,
        "fenêtre de la caméra fermée",
    );
    assert!(sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>()
            .get_participants()
            .iter()
            .any(|p| p.utilisateur == "carol" && p.portrait.size().width > 0)
    }));
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<estompe_natif::DonneesCoquille>()
                .get_salons()
                .iter()
                .any(|l| l.id == "v1" && l.duree == "4 h 00")
        },
        "durée du salon sous son nom dans le panneau",
    );

    cliquer(&faible, "Mode plein cadre");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_plein_cadre(),
        "mode plein cadre",
    );
    assert!(
        sur_l_interface(&faible, |f| {
            ElementHandle::find_by_accessible_label(f, "Mode plein cadre")
                .all(|e| e.size().height == 0.0)
                && ElementHandle::find_by_accessible_label(f, "Accueil")
                    .all(|e| e.size().height == 0.0)
        }),
        "rail, panneau et en-tête masqués"
    );
    sur_l_interface(&faible, |f| {
        presser(f, Key::Escape);
        relacher(f, Key::Escape);
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_plein_cadre(),
        "Échap quitte le mode plein cadre",
    );
    cliquer(&faible, "Mode plein cadre");
    cliquer(&faible, "Quitter le mode plein cadre (Échap)");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_plein_cadre(),
        "le bouton quitte le mode plein cadre",
    );
    assert_eq!(lieu, "Salon vocal · Les amis");
    assert_eq!(compte, "3 / 10");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>()
                .get_participants()
                .iter()
                .filter(|p| p.salon == "v1")
                .count()
                == 3
        },
        "participants listés sous le salon dans le panneau",
    );

    banc.serveur
        .envoyer("v1", "u1", EvenementSalle::Parole(vec!["carol".into()]));
    let retard = mesurer(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| t.parle),
        "c4 : carol parle",
    );
    eprintln!(
        "indicateur de parole d'un autre participant : {retard:?} après l'évènement de LiveKit"
    );
    assert!(retard < RETARD_MAXIMAL, "{retard:?}");
    banc.serveur
        .envoyer("v1", "u1", EvenementSalle::Parole(Vec::new()));
    attendre_l_interface(
        &faible,
        |f| tuile(f, "carol").is_some_and(|t| !t.parle),
        "carol se tait",
    );

    let voix = voix_forte();
    let debut = Instant::now();
    let mut retard_moi = None;
    while retard_moi.is_none() {
        for trame in voix.chunks(480) {
            banc.audio.pousser("micro", trame);
        }
        if sur_l_interface(&faible, |f| tuile(f, "u1").is_some_and(|t| t.parle)) {
            retard_moi = Some(debut.elapsed());
        }
        assert!(
            debut.elapsed() < Duration::from_secs(10),
            "sa propre voix jamais détectée"
        );
    }
    let retard_moi = retard_moi.unwrap();
    eprintln!("indicateur de sa propre parole : {retard_moi:?} après la première trame du micro");
    assert!(retard_moi < RETARD_MAXIMAL, "{retard_moi:?}");

    cliquer(&faible, "Couper le micro");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_barre().micro_coupe,
        "c2 : micro coupé",
    );
    attendre(
        || banc.serveur.micro_coupe("u1") == Some(true),
        "piste coupée côté salle",
    );
    attendre(
        || {
            banc.serveur
                .attributs("v1", "u1")
                .get("muted")
                .map(String::as_str)
                == Some("1")
        },
        "attribut muted publié",
    );
    attendre_l_interface(
        &faible,
        |f| tuile(f, "u1").is_some_and(|t| t.muet && !t.parle),
        "sa tuile affiche le micro coupé",
    );

    raccourci(&faible, "M");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().micro_coupe,
        "c3 : Alt+Maj+M réactive le micro",
    );
    raccourci(&faible, "D");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_barre().casque_coupe,
        "c3 : Alt+Maj+D coupe le son",
    );
    raccourci(&faible, "D");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().casque_coupe,
        "son rétabli",
    );

    cliquer(&faible, "carol");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_menu_ouvert()
                && f.global::<DonneesVocal>().get_menu().nom == "carol"
        },
        "menu de carol ouvert",
    );
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        v.invoke_volume(40, true);
        v.invoke_muet_pour_moi(true);
    });
    attendre(
        || {
            memoire.lire(|p| {
                p.ecoute.volume("carol", SourceAudio::Micro) == 40
                    && p.ecoute.muet("carol", SourceAudio::Micro)
            })
        },
        "volume et muet de carol retenus",
    );
    attendre(
        || {
            banc.session
                .instantane()
                .ecoute
                .muet("carol", SourceAudio::Micro)
        },
        "muet appliqué à la session",
    );
    sur_l_interface(&faible, |f| {
        presser(f, Key::Escape);
        relacher(f, Key::Escape);
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu fermé par Échap",
    );

    cliquer(&faible, "carol");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu de carol rouvert",
    );
    let (destinations, deconnecter) = sur_l_interface(&faible, |f| {
        let m = f.global::<DonneesVocal>().get_menu();
        (
            m.destinations
                .iter()
                .map(|d| (d.id.to_string(), d.nom.to_string()))
                .collect::<Vec<_>>(),
            m.peut_deconnecter,
        )
    });
    assert_eq!(
        destinations,
        [("k1".to_string(), "Ciné du dimanche".to_string())]
    );
    assert!(deconnecter);
    cliquer(&faible, "Déplacer vers « Ciné du dimanche »");
    attendre(
        || {
            requete_recue(
                &banc.http,
                "PUT /voice/v1/participants/carol/move",
                r#""channel_id":"k1""#,
            )
        },
        "déplacement demandé au serveur",
    );
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu fermé après le déplacement",
    );
    cliquer(&faible, "carol");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu de carol rouvert pour la déconnecter",
    );
    cliquer(&faible, "Déconnecter du vocal");
    attendre(
        || {
            requete_recue(
                &banc.http,
                "POST /voice/v1/participants/carol/disconnect",
                "",
            )
        },
        "déconnexion demandée au serveur",
    );
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_menu_ouvert(),
        "menu fermé après la déconnexion",
    );

    let deplacements = || {
        banc.http
            .corps
            .lock()
            .unwrap()
            .iter()
            .filter(|(r, c)| {
                r == "PUT /voice/v1/participants/carol/move" && c.contains(r#""channel_id":"k1""#)
            })
            .count()
    };
    let avant = deplacements();
    glisser(&faible, "carol", "Ciné du dimanche");
    attendre(
        || deplacements() == avant + 1,
        "glisser carol sur le salon cinéma la déplace par la route de « Déplacer vers »",
    );
    *banc.http.refus_vocal.lock().unwrap() = Some(403);
    glisser(&faible, "carol", "Ciné du dimanche");
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_erreur_depot() == "Action impossible : Rang insuffisant"
                && v.get_erreur_depot_salon() == "k1"
        },
        "le refus du serveur s'affiche sous le salon visé",
    );
    assert_eq!(deplacements(), avant + 2);
    *banc.http.refus_vocal.lock().unwrap() = None;
    glisser(&faible, "carol", "Ciné du dimanche");
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_erreur_depot().is_empty(),
        "un nouveau dépôt efface le refus",
    );
    attendre(
        || deplacements() == avant + 3,
        "troisième déplacement envoyé",
    );

    fabrique.envoyer(evenement(
        "voice.state",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "pseudo": "bob", "joined": true}),
    ));
    fabrique.envoyer(evenement(
        "voice.mod",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "mic_off": true, "sound_off": false}),
    ));
    attendre(
        || banc.session.instantane().micro_impose,
        "coupure du modérateur appliquée",
    );
    attendre_l_interface(
        &faible,
        |f| tuile(f, "u1").is_some_and(|t| t.micro_impose),
        "coupure du modérateur affichée",
    );
    fabrique.envoyer(evenement(
        "voice.mod",
        json!({"server_id": "s1", "channel_id": "v1", "user_id": "u1", "mic_off": false, "sound_off": false}),
    ));
    attendre(|| !banc.session.instantane().micro_impose, "coupure levée");

    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_ouvrir_reglages()
    });
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_reglages_ouverts()
                && f.global::<DonneesVocal>().get_micros().row_count() == 3
        },
        "réglages ouverts avec les micros",
    );
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        let mut r = v.get_reglages();
        r.ptt = true;
        v.invoke_regler(r, true);
    });
    attendre_l_interface(
        &faible,
        |f| {
            let r = f.global::<DonneesVocal>().get_reglages();
            r.ptt && r.ptt_message.contains("indisponible")
        },
        "c2 (#330) : le réglage dit que le raccourci global est indisponible",
    );
    attendre(
        || !banc.session.instantane().porte_ouverte,
        "porte fermée en appuyer pour parler",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_fermer_reglages()
    });
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_reglages_ouverts(),
        "réglages fermés",
    );
    sur_l_interface(&faible, |f| presser(f, "v"));
    attendre(
        || banc.session.instantane().porte_ouverte,
        "c2 (#330) : la touche ouvre le micro fenêtre active",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesVocal>().get_ptt_parle(),
        "« tu parles » affiché",
    );
    sur_l_interface(&faible, |f| relacher(f, "v"));
    attendre(
        || !banc.session.instantane().porte_ouverte,
        "relâchée, la porte se ferme",
    );
    assert!(memoire.lire(|p| p.ptt.actif));
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        v.invoke_choisir_son(true, 2);
        v.invoke_volume_sons(30, true);
    });
    attendre(
        || memoire.lire(|p| p.sons.arrivee == ChoixSon::Carillon && p.sons.volume == 30),
        "son d'arrivée et volume des sons retenus",
    );
    attendre_l_interface(
        &faible,
        |f| {
            let r = f.global::<DonneesVocal>().get_reglages();
            r.son_arrivee == 2 && r.son_depart == 1 && r.volume_sons == 30
        },
        "choix du son affiché dans les réglages",
    );

    let dossier_sons = memoire.dossier().join("sons");
    let essais = tempfile::tempdir().unwrap();
    let trop_gros = essais.path().join("trop-gros.wav");
    std::fs::write(&trop_gros, wav(&vec![0.2; 80_000])).unwrap();
    let court = essais.path().join("court.wav");
    std::fs::write(&court, wav(&voix_forte())).unwrap();
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_ouvrir_reglages()
    });
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            v.get_reglages_ouverts() && !v.get_reglages().perso_arrivee
        },
        "réglages ouverts, sans son personnalisé",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().set_reglages_decalage(-500.0)
    });
    *fabrique.fichiers.choix.lock().unwrap() = vec![trop_gros];
    cliquer(&faible, "Mon fichier…");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_son_erreur()
                == "Choisis un fichier audio de 150 Ko au plus."
        },
        "un fichier de plus de 150 Ko est refusé comme sur le web",
    );
    assert_eq!(memoire.lire(|p| p.sons.arrivee), ChoixSon::Carillon);
    assert!(!dossier_sons.join("arrivee").exists());
    let sorties = |audio: &SystemeFactice| {
        audio
            .ouvertures()
            .iter()
            .filter(|(sens, _)| *sens == SensAudio::Sortie)
            .count()
    };
    let lectures = sorties(&banc.audio);
    *fabrique.fichiers.choix.lock().unwrap() = vec![court.clone()];
    cliquer(&faible, "Mon fichier…");
    attendre(
        || memoire.lire(|p| p.sons.arrivee == ChoixSon::Personnalise),
        "le fichier choisi devient le son d'arrivée",
    );
    attendre_l_interface(
        &faible,
        |f| {
            let v = f.global::<DonneesVocal>();
            let r = v.get_reglages();
            r.perso_arrivee
                && !r.perso_depart
                && r.son_arrivee == 6
                && v.get_son_erreur().is_empty()
        },
        "« Mon fichier » proposé et choisi pour l'arrivée seulement",
    );
    assert_eq!(
        std::fs::read(dossier_sons.join("arrivee")).unwrap(),
        std::fs::read(&court).unwrap(),
        "le fichier est gardé sur l'appareil"
    );
    attendre(
        || sorties(&banc.audio) > lectures,
        "le son choisi est joué en aperçu",
    );
    cliquer(&faible, "Montée");
    attendre(
        || memoire.lire(|p| p.sons.arrivee == ChoixSon::Montee),
        "retour à un son prédéfini",
    );
    attendre_l_interface(
        &faible,
        |f| {
            let r = f.global::<DonneesVocal>().get_reglages();
            r.son_arrivee == 0 && r.perso_arrivee
        },
        "le fichier reste proposé après le retour au son prédéfini",
    );
    cliquer(&faible, "Mon fichier");
    attendre(
        || memoire.lire(|p| p.sons.arrivee == ChoixSon::Personnalise),
        "le fichier se choisit à nouveau sans le recharger",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesVocal>().invoke_fermer_reglages()
    });
    sur_l_interface(&faible, |f| {
        let v = f.global::<DonneesVocal>();
        let mut r = v.get_reglages();
        r.ptt = false;
        v.invoke_regler(r, true);
    });
    attendre(
        || banc.session.instantane().porte_ouverte,
        "appuyer pour parler désactivé",
    );

    fabrique.envoyer(evenement(
        "voice.moved",
        json!({"server_id": "s1", "from": "v1", "to": "k1", "by": "eve", "to_name": "Ciné du dimanche"}),
    ));
    attendre(
        || banc.serveur.participants("k1").contains("u1"),
        "déplacé dans le salon cinéma",
    );
    attendre_l_interface(
        &faible,
        |f| {
            let b = f.global::<DonneesVocal>().get_barre();
            b.salon == "k1" && b.message.contains("eve t'a déplacé")
        },
        "la barre suit le déplacement",
    );

    cliquer(&faible, "Quitter le vocal");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().visible,
        "c2 : raccroché",
    );
    attendre(
        || banc.serveur.participants("k1").is_empty(),
        "retiré de la salle",
    );
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_vue().ici,
        "écran pour rejoindre",
    );

    cliquer(&faible, "Rejoindre le salon");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>().get_vue().ici
                && f.global::<DonneesVocal>().get_barre().visible
        },
        "rejoint depuis le bouton",
    );
    cliquer(&faible, "Quitter le vocal");
    attendre_l_interface(
        &faible,
        |f| !f.global::<DonneesVocal>().get_barre().visible,
        "raccroché à nouveau",
    );
    banc.api.refuser("v1", 409, "voice_full");
    cliquer(&faible, "Rejoindre le salon");
    attendre_l_interface(
        &faible,
        |f| {
            f.global::<DonneesVocal>()
                .get_vue()
                .erreur
                .contains("complet")
        },
        "refus du serveur affiché sur l'écran pour rejoindre",
    );
}

#[test]
fn c2_rejoindre_couper_le_micro_raccrocher_et_commandes_au_clavier() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(
        Preferences {
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c1")),
            ptt: ReglagesPtt::default(),
            ..Preferences::default()
        },
        dossier.path().to_path_buf(),
        None,
    );
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let auth = FausseAuth::avec_session(true);
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(
            auth.clone(),
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let serveur = ServeurVocalFactice::nouveau();
    let audio = SystemeFactice::nouveau();
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
    audio.brancher(
        SensAudio::Sortie,
        "hp",
        "Haut-parleurs",
        FormatAudio {
            frequence: 48_000,
            canaux: 2,
        },
    );
    let api = Arc::new(ApiVocaleFactice::nouvelle("u1"));
    let cameras = CamerasFactices::nouvelles();
    cameras.brancher("c1", "Caméra intégrée");
    cameras.brancher("c2", "Webcam USB");
    let session = Arc::new(SessionVocale::demarrer(
        &entree.runtime(),
        Dependances {
            api: api.clone(),
            connecteur: Arc::new(serveur.clone()),
            audio: Arc::new(audio.clone()),
            cameras: Arc::new(cameras.clone()),
            apm: None,
            reglages: estompe_client::vocal::Reglages::default(),
        },
    ));
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let http = ServeurMessages::demarrer(0);
    let mut etat = commun::coquille::etat();
    etat.avatars.insert("carol".into(), 2);
    if let Some(serveur) = etat.details.get_mut("s1").and_then(|d| d.server.as_mut()) {
        serveur.owner_id = Some("u1".into());
    }
    let depuis = (Local.from_local_datetime(&midi).earliest().unwrap()
        - chrono::Duration::hours(4))
    .with_timezone(&Utc)
    .to_rfc3339_opts(SecondsFormat::Secs, true);
    if let Some(liste) = etat.vocal.get_mut("v1") {
        for p in liste.iter_mut() {
            p.depuis = Some(depuis.clone());
        }
    }
    let fabrique = FausseFabrique::sur(etat, Vec::new(), &http.adresse);
    *fabrique.vocal.lock().unwrap() = Some(OutilsVocaux {
        session: session.clone(),
        audio: Arc::new(audio.clone()),
        raccourcis: Arc::new(RaccourcisFactices::nouveaux(false)),
        cameras: Arc::new(cameras.clone()),
        apm: None,
    });
    let coquille = Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire.clone(),
        Arc::new(move || midi),
    );
    fenetre.show().unwrap();
    let banc = Banc {
        http,
        serveur,
        audio,
        api,
        session: session.clone(),
        visibilite: Arc::new(coquille.suivi_de_la_visibilite()),
        cameras,
    };
    let faible = fenetre.as_weak();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| {
            scenario(faible, fabrique, banc, memoire)
        }));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    session.fermer_bloquant(Duration::from_secs(3));
    drop(coquille);
    drop(entree);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
