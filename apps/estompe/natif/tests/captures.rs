use estompe_client::auth::ErreurAuth;
use estompe_natif::ecrans::afficher;
use estompe_natif::entree::{self, Compte, Etat};
use estompe_natif::{creer_fenetre, ModeTheme, Mouvement, Principale, Theme};
use i_slint_backend_testing::{TestingBackend, TestingBackendOptions};
use slint::ComponentHandle;
use std::path::{Path, PathBuf};
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
    "coquille-clair", Clair, coquille;
    "coquille-serveur-rejoint-sombre", Sombre, coquille_serveur_rejoint;
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

fn coquille(f: &Principale) {
    afficher(
        f,
        &Etat::Connecte {
            compte: bob(),
            serveur: None,
        },
    );
}

fn coquille_serveur_rejoint(f: &Principale) {
    afficher(
        f,
        &Etat::Connecte {
            compte: bob(),
            serveur: Some(entree::ServeurRejoint {
                id: "s1".into(),
                nom: "Les amis".into(),
            }),
        },
    );
}

fn remettre_a_zero(f: &Principale) {
    f.set_demonstration(false);
    f.set_infobulle_forcee(false);
    f.set_modal_demo_ouvert(false);
    f.set_a_propos_ouvert(false);
    coquille(f);
    f.set_activation_pseudo("".into());
    f.set_invitation_saisie("".into());
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
