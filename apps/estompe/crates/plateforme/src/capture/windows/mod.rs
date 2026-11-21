mod curseur;
mod fenetre;

use std::fmt::Display;
use std::sync::mpsc::RecvTimeoutError;

use xcap::image::RgbaImage;
use xcap::{Monitor, Window};

use super::flux::{lancer, sonder, Commande, Releve, Rythme, PAS_MAX};
use super::{fenetre_retenue, FluxCapture, IdSource};
use crate::traits::{
    EvenementCapture, Image, OptionsCapture, RecepteurCapture, SourceCapture, TypeSource,
};
use crate::{Erreur, Resultat};

fn echec(e: impl Display) -> Erreur {
    Erreur::Echec(e.to_string())
}

fn image_de(image: RgbaImage) -> Image {
    Image {
        largeur: image.width(),
        hauteur: image.height(),
        rgba: image.into_raw(),
    }
}

fn poignee(numero: u32) -> isize {
    numero as i32 as isize
}

fn ecran(numero: u32) -> Resultat<Monitor> {
    Monitor::all()
        .map_err(echec)?
        .into_iter()
        .find(|m| m.id().ok() == Some(numero))
        .ok_or_else(|| echec("écran introuvable"))
}

fn fenetre(numero: u32) -> Resultat<Window> {
    Window::all()
        .map_err(echec)?
        .into_iter()
        .find(|w| w.id().ok() == Some(numero))
        .ok_or_else(|| echec("fenêtre introuvable"))
}

pub(super) fn disponible() -> bool {
    true
}

pub(super) fn sources() -> Resultat<Vec<SourceCapture>> {
    let mut sources = Vec::new();
    for m in Monitor::all().map_err(echec)? {
        let (Ok(id), Ok(largeur), Ok(hauteur)) = (m.id(), m.width(), m.height()) else {
            continue;
        };
        sources.push(SourceCapture {
            id: IdSource::Ecran(id).to_string(),
            nom: m.friendly_name().or_else(|_| m.name()).unwrap_or_default(),
            application: String::new(),
            genre: TypeSource::Ecran,
            largeur,
            hauteur,
            principale: m.is_primary().unwrap_or(false),
            processus: None,
            estompe: false,
        });
    }
    let moi = std::process::id();
    for w in Window::all().map_err(echec)? {
        let (Ok(id), Ok(titre), Ok(largeur), Ok(hauteur)) =
            (w.id(), w.title(), w.width(), w.height())
        else {
            continue;
        };
        if !fenetre_retenue(&titre, largeur, hauteur, w.is_minimized().unwrap_or(true)) {
            continue;
        }
        let processus = w.pid().ok();
        sources.push(SourceCapture {
            id: IdSource::Fenetre(id).to_string(),
            nom: titre,
            application: w.app_name().unwrap_or_default(),
            genre: TypeSource::Fenetre,
            largeur,
            hauteur,
            principale: false,
            processus,
            estompe: processus == Some(moi),
        });
    }
    Ok(sources)
}

fn relever(id: IdSource, curseur: bool) -> Releve {
    let resultat = match id {
        IdSource::Ecran(numero) => ecran(numero).and_then(|m| {
            let mut image = image_de(m.capture_image().map_err(echec)?);
            if curseur {
                curseur::dessiner(&mut image, (m.x().unwrap_or(0), m.y().unwrap_or(0)));
            }
            Ok(image)
        }),
        IdSource::Fenetre(numero) => {
            if !fenetre::existe(poignee(numero)) {
                return Releve::Fermee;
            }
            if fenetre::reduite(poignee(numero)) {
                return Releve::Reduite;
            }
            fenetre(numero).and_then(|w| {
                let mut image = image_de(w.capture_image().map_err(echec)?);
                if curseur {
                    curseur::dessiner(&mut image, (w.x().unwrap_or(0), w.y().unwrap_or(0)));
                }
                Ok(image)
            })
        }
    };
    resultat.map_or_else(|e| Releve::Echec(e.to_string()), Releve::Image)
}

pub(super) fn capturer(id: IdSource) -> Resultat<Image> {
    match relever(id, false) {
        Releve::Image(image) => Ok(image),
        Releve::Reduite => Err(echec("fenêtre réduite")),
        Releve::Fermee => Err(echec("fenêtre fermée")),
        Releve::Echec(raison) => Err(Erreur::Echec(raison)),
    }
}

fn diffuser_ecran(numero: u32, commande: &Commande, recepteur: &mut RecepteurCapture) {
    let Ok(m) = ecran(numero) else {
        recepteur(EvenementCapture::Fermee);
        return;
    };
    let origine = (m.x().unwrap_or(0), m.y().unwrap_or(0));
    let enregistreur = m
        .video_recorder()
        .ok()
        .filter(|(enregistreur, _)| enregistreur.start().is_ok());
    let Some((enregistreur, trames)) = enregistreur else {
        sonder(commande, recepteur, |curseur| {
            relever(IdSource::Ecran(numero), curseur)
        });
        return;
    };
    let mut rythme = Rythme::nouveau(commande);
    while !commande.arretee() {
        let attente = rythme.attente(commande);
        if !attente.is_zero() {
            std::thread::sleep(attente.min(PAS_MAX));
            continue;
        }
        let trame = match trames.recv_timeout(PAS_MAX * 2) {
            Ok(trame) => trame,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                recepteur(EvenementCapture::Echec(
                    "capture de l'écran interrompue".into(),
                ));
                break;
            }
        };
        let trame = trames.try_iter().last().unwrap_or(trame);
        if !rythme.due(commande) {
            continue;
        }
        let mut image = Image {
            largeur: trame.width,
            hauteur: trame.height,
            rgba: trame.raw,
        };
        if commande.curseur() {
            curseur::dessiner(&mut image, origine);
        }
        if !recepteur(EvenementCapture::Trame(image)) {
            break;
        }
    }
    let _ = enregistreur.stop();
}

fn diffuser_fenetre(numero: u32, commande: &Commande, recepteur: &mut RecepteurCapture) {
    if fenetre::capturer(poignee(numero), commande, recepteur).is_ok() {
        return;
    }
    sonder(commande, recepteur, |curseur| {
        relever(IdSource::Fenetre(numero), curseur)
    });
}

pub(super) fn demarrer(
    id: IdSource,
    options: OptionsCapture,
    recepteur: RecepteurCapture,
) -> Resultat<FluxCapture> {
    match id {
        IdSource::Ecran(numero) => {
            ecran(numero)?;
            lancer(
                &id.to_string(),
                options,
                recepteur,
                move |commande, recepteur| diffuser_ecran(numero, commande, recepteur),
            )
        }
        IdSource::Fenetre(numero) => {
            if !fenetre::existe(poignee(numero)) {
                return Err(echec("fenêtre introuvable"));
            }
            lancer(
                &id.to_string(),
                options,
                recepteur,
                move |commande, recepteur| diffuser_fenetre(numero, commande, recepteur),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::CaptureEcran;
    use crate::CaptureSysteme;
    use std::sync::mpsc;
    use std::time::Duration;

    fn exige() -> bool {
        std::env::var_os("ESTOMPE_CAPTURE_EXIGE").is_some_and(|v| v == "1")
    }

    #[test]
    fn windows_liste_et_capture_le_premier_ecran() {
        let capture = CaptureSysteme;
        assert!(capture.disponible());
        let sources = match capture.sources() {
            Ok(sources) if sources.iter().any(|s| s.genre == TypeSource::Ecran) => sources,
            autre => {
                assert!(!exige(), "aucun écran : {autre:?}");
                eprintln!("ignoré : aucun écran capturable ({autre:?})");
                return;
            }
        };
        let ecran = sources
            .iter()
            .find(|s| s.genre == TypeSource::Ecran)
            .unwrap();
        let image = match capture.capturer(&ecran.id) {
            Ok(image) => image,
            Err(e) => {
                assert!(!exige(), "capture de l'écran : {e}");
                eprintln!("ignoré : capture de l'écran impossible ({e})");
                return;
            }
        };
        assert_eq!(
            image.rgba.len(),
            (image.largeur * image.hauteur * 4) as usize
        );
        let (envoi, reception) = mpsc::channel();
        let flux = capture
            .demarrer(
                &ecran.id,
                OptionsCapture {
                    ips: 30,
                    curseur: true,
                },
                Box::new(move |e| envoi.send(e).is_ok()),
            )
            .unwrap();
        std::thread::sleep(Duration::from_secs(1));
        flux.arreter();
        let trames = reception
            .try_iter()
            .filter(|e| matches!(e, EvenementCapture::Trame(_)))
            .count();
        eprintln!("{trames} images de l'écran en 1 s à 30 i/s");
        assert!(trames <= 34);
        assert!(!exige() || trames >= 15, "{trames} images en 1 s");
    }

    #[test]
    fn windows_refuse_une_fenetre_inexistante() {
        assert!(CaptureSysteme
            .demarrer("fenetre:1", OptionsCapture::default(), Box::new(|_| true))
            .is_err());
    }
}
