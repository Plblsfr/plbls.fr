mod commun;

use std::sync::Arc;

use commun::attendre;
use commun::references::{controler, VARIABLE};
use estompe_client::overlay::{taille_fenetre, Placement, TailleOverlay};
use estompe_client::overlay_messages::taille_fenetre as taille_messages;
use estompe_natif::coquille::CleAvatar;
use estompe_natif::overlay::messages::{
    appliquer as appliquer_messages, EtatMessages, LigneMessage,
};
use estompe_natif::overlay::voix::{appliquer, AvatarOverlay, EtatVoix, LigneVoix};
use estompe_natif::overlay::MoteurOverlay;
use estompe_natif::{OverlayMessages, OverlayVoix};
use estompe_plateforme::{AffichageFactice, OverlayFactice};
use slint::{Rgba8Pixel, SharedPixelBuffer};

const FOND: [u8; 3] = [32, 40, 48];

struct Cas {
    nom: &'static str,
    taille: TailleOverlay,
    lignes: Vec<LigneVoix>,
}

fn avatar(utilisateur: &str, couleur: [u8; 3]) -> AvatarOverlay {
    let mut image = SharedPixelBuffer::<Rgba8Pixel>::new(96, 96);
    for (i, p) in image.make_mut_slice().iter_mut().enumerate() {
        let bande = (i % 96) / 24 % 2 == 0;
        *p = Rgba8Pixel {
            r: if bande { couleur[0] } else { couleur[0] / 2 },
            g: if bande { couleur[1] } else { couleur[1] / 2 },
            b: if bande { couleur[2] } else { couleur[2] / 2 },
            a: 255,
        };
    }
    AvatarOverlay {
        cle: CleAvatar {
            utilisateur: utilisateur.into(),
            version: 1,
            serveur: None,
        },
        image,
    }
}

fn ligne(pseudo: &str, parle: bool, muet: bool, sourd: bool) -> LigneVoix {
    LigneVoix {
        id: pseudo.into(),
        pseudo: pseudo.into(),
        initiales: pseudo.chars().take(2).collect::<String>().to_uppercase(),
        avatar: None,
        parle,
        muet,
        sourd,
    }
}

fn quatre() -> Vec<LigneVoix> {
    vec![
        LigneVoix {
            avatar: Some(avatar("alice", [220, 60, 60])),
            ..ligne("alice", true, false, false)
        },
        ligne("bob", false, false, false),
        ligne("carol", false, true, false),
        ligne("dave_au_pseudo_vraiment_tres_long", false, true, true),
    ]
}

fn cas() -> Vec<Cas> {
    let huit = (0..8)
        .map(|i| {
            let pseudo = [
                "alice", "bob", "carol", "dave", "eve", "fanny", "gael", "hugo",
            ][i];
            LigneVoix {
                avatar: (i % 3 == 0).then(|| avatar(pseudo, [60, 120, 220])),
                ..ligne(pseudo, i == 1 || i == 4, i == 5, i == 6)
            }
        })
        .collect();
    vec![
        Cas {
            nom: "overlay-voix-1-md",
            taille: TailleOverlay::Moyenne,
            lignes: vec![ligne("alice", false, false, false)],
        },
        Cas {
            nom: "overlay-voix-locuteur-md",
            taille: TailleOverlay::Moyenne,
            lignes: vec![
                LigneVoix {
                    avatar: Some(avatar("alice", [220, 60, 60])),
                    ..ligne("alice", true, false, false)
                },
                ligne("bob", true, false, false),
            ],
        },
        Cas {
            nom: "overlay-voix-coupes-md",
            taille: TailleOverlay::Moyenne,
            lignes: vec![
                ligne("carol", false, true, false),
                ligne("dave", false, false, true),
            ],
        },
        Cas {
            nom: "overlay-voix-4-sm",
            taille: TailleOverlay::Petite,
            lignes: quatre(),
        },
        Cas {
            nom: "overlay-voix-4-md",
            taille: TailleOverlay::Moyenne,
            lignes: quatre(),
        },
        Cas {
            nom: "overlay-voix-4-lg",
            taille: TailleOverlay::Grande,
            lignes: quatre(),
        },
        Cas {
            nom: "overlay-voix-8-md",
            taille: TailleOverlay::Moyenne,
            lignes: huit,
        },
    ]
}

fn sur_fond(a: &AffichageFactice) -> image::RgbaImage {
    let mut sortie = image::RgbaImage::new(a.image.largeur, a.image.hauteur);
    for (p, s) in a.image.rgba.chunks_exact(4).zip(sortie.pixels_mut()) {
        let reste = 255 - u32::from(p[3]);
        let canal = |c: usize| (u32::from(p[c]) + u32::from(FOND[c]) * reste / 255).min(255) as u8;
        *s = image::Rgba([canal(0), canal(1), canal(2), 255]);
    }
    sortie
}

#[test]
fn l_overlay_des_voix_correspond_a_ses_references() {
    let consigne = std::env::var(VARIABLE).unwrap_or_default();
    if consigne == "ignorer" {
        eprintln!("ignoré : {VARIABLE}=ignorer");
        return;
    }
    let regenerer = consigne == "regenerer";
    let factice = OverlayFactice::nouveau(None, 1.0);
    let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
    let scene = moteur.scene(OverlayVoix::new, appliquer);
    let mut echecs = Vec::new();
    for (i, c) in cas().into_iter().enumerate() {
        let fenetre = taille_fenetre(c.taille, c.lignes.len());
        let placement = Placement {
            x: 0,
            y: 0,
            largeur: fenetre.largeur as u32,
            hauteur: fenetre.hauteur as u32,
            echelle: 1.0,
        };
        assert!(scene.afficher(
            EtatVoix {
                taille: c.taille,
                lignes: c.lignes,
            },
            placement,
            255,
        ));
        attendre(|| factice.affichages() == i + 1, c.nom);
        let affichage = factice.calques()[0].clone().expect("calque affiché");
        assert_eq!(
            (affichage.image.largeur, affichage.image.hauteur),
            (placement.largeur, placement.hauteur)
        );
        if let Err(echec) = controler(c.nom, &sur_fond(&affichage), regenerer) {
            echecs.push(echec);
        }
    }
    assert_eq!(moteur.rendus(), 7);
    assert!(
        echecs.is_empty(),
        "overlay différent de ses références :\n{}\nSi le changement est voulu : {VARIABLE}=regenerer cargo test --test overlay",
        echecs.join("\n")
    );
}

fn message(titre: &str, corps: &str, avatar: Option<AvatarOverlay>) -> LigneMessage {
    LigneMessage {
        titre: titre.into(),
        corps: corps.into(),
        initiales: titre.chars().take(2).collect::<String>().to_uppercase(),
        avatar,
    }
}

#[test]
fn l_overlay_des_messages_correspond_a_ses_references() {
    let consigne = std::env::var(VARIABLE).unwrap_or_default();
    if consigne == "ignorer" {
        eprintln!("ignoré : {VARIABLE}=ignorer");
        return;
    }
    let regenerer = consigne == "regenerer";
    let factice = OverlayFactice::nouveau(None, 1.0);
    let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
    let scene = moteur.scene(OverlayMessages::new, appliquer_messages);
    let cas = [
        (
            "overlay-messages-prive",
            EtatMessages {
                lignes: vec![message(
                    "alice",
                    "On lance une partie ce soir ?",
                    Some(avatar("alice", [220, 60, 60])),
                )],
                saisie: None,
            },
        ),
        (
            "overlay-messages-mention",
            EtatMessages {
                lignes: vec![
                    message("bob", "salut", None),
                    message(
                        "carol dans #général",
                        "@moi tu peux regarder la carte quand tu as un moment ? Le message est long et se coupe",
                        None,
                    ),
                ],
                saisie: None,
            },
        ),
        (
            "overlay-messages-saisie-vide",
            EtatMessages {
                lignes: vec![message("alice", "Tu viens ?", None)],
                saisie: Some(String::new()),
            },
        ),
        (
            "overlay-messages-saisie",
            EtatMessages {
                lignes: vec![message("alice", "Tu viens ?", None)],
                saisie: Some("J'arrive dans deux minutes".into()),
            },
        ),
    ];
    let mut echecs = Vec::new();
    for (i, (nom, etat)) in cas.into_iter().enumerate() {
        let fenetre = taille_messages(etat.lignes.len(), etat.saisie.is_some());
        let placement = Placement {
            x: 0,
            y: 0,
            largeur: fenetre.largeur as u32,
            hauteur: fenetre.hauteur as u32,
            echelle: 1.0,
        };
        assert!(scene.afficher(etat, placement, 255));
        attendre(|| factice.affichages() == i + 1, nom);
        let affichage = factice.calques()[0].clone().expect("calque affiché");
        assert_eq!(
            (affichage.image.largeur, affichage.image.hauteur),
            (placement.largeur, placement.hauteur)
        );
        if let Err(echec) = controler(nom, &sur_fond(&affichage), regenerer) {
            echecs.push(echec);
        }
    }
    assert!(
        echecs.is_empty(),
        "overlay différent de ses références :\n{}\nSi le changement est voulu : {VARIABLE}=regenerer cargo test --test overlay",
        echecs.join("\n")
    );
}
