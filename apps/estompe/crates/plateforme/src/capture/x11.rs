use std::fmt::Display;
use std::sync::OnceLock;

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::composite::{self, ConnectionExt as _};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, Drawable, ImageFormat, ImageOrder, MapState, Window,
};
use x11rb::rust_connection::RustConnection;

use super::flux::{lancer, sonder, Releve};
use super::{bgra_vers_rgba, fenetre_retenue, incruster_curseur, FluxCapture, IdSource};
use crate::raccourcis::session_wayland;
use crate::traits::{Image, OptionsCapture, RecepteurCapture, SourceCapture, TypeSource};
use crate::{Erreur, Resultat};

const NOM_ECRAN: &str = "Écran";

fn echec(e: impl Display) -> Erreur {
    Erreur::Echec(e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Zone {
    x: i16,
    y: i16,
    largeur: u16,
    hauteur: u16,
}

struct Atomes {
    clients: u32,
    nom: u32,
    utf8: u32,
    etat: u32,
    cachee: u32,
    pid: u32,
}

struct Session {
    connexion: RustConnection,
    racine: Window,
    atomes: Atomes,
    curseur: bool,
}

enum EtatFenetre {
    Visible,
    Reduite,
    Fermee,
}

fn atome(connexion: &RustConnection, nom: &str) -> Resultat<u32> {
    Ok(connexion
        .intern_atom(false, nom.as_bytes())
        .map_err(echec)?
        .reply()
        .map_err(echec)?
        .atom)
}

impl Session {
    fn ouvrir() -> Resultat<Self> {
        if session_wayland() {
            return Err(Erreur::Indisponible("capture d'écran sous Wayland"));
        }
        if std::env::var_os("DISPLAY").is_none_or(|d| d.is_empty()) {
            return Err(Erreur::Indisponible("capture d'écran sans serveur X"));
        }
        let (connexion, ecran) =
            x11rb::connect(None).map_err(|_| Erreur::Indisponible("capture d'écran"))?;
        let setup = connexion.setup();
        let racine = setup.roots[ecran].root;
        let profondeur = setup.roots[ecran].root_depth;
        let format_32 = setup
            .pixmap_formats
            .iter()
            .any(|f| f.depth == profondeur && f.bits_per_pixel == 32);
        if setup.image_byte_order != ImageOrder::LSB_FIRST || !format_32 {
            return Err(Erreur::Indisponible("capture d'écran : format de pixels"));
        }
        let atomes = Atomes {
            clients: atome(&connexion, "_NET_CLIENT_LIST")?,
            nom: atome(&connexion, "_NET_WM_NAME")?,
            utf8: atome(&connexion, "UTF8_STRING")?,
            etat: atome(&connexion, "_NET_WM_STATE")?,
            cachee: atome(&connexion, "_NET_WM_STATE_HIDDEN")?,
            pid: atome(&connexion, "_NET_WM_PID")?,
        };
        let _ = connexion
            .composite_query_version(0, 4)
            .map(|c| c.reply().is_ok());
        let _ = connexion
            .randr_query_version(1, 5)
            .map(|c| c.reply().is_ok());
        let curseur = connexion
            .xfixes_query_version(4, 0)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some();
        Ok(Self {
            connexion,
            racine,
            atomes,
            curseur,
        })
    }

    fn propriete(&self, fenetre: Window, atome: u32, genre: impl Into<u32>) -> Option<Vec<u8>> {
        let reponse = self
            .connexion
            .get_property(false, fenetre, atome, genre, 0, u32::MAX / 4)
            .ok()?
            .reply()
            .ok()?;
        (reponse.format != 0).then_some(reponse.value)
    }

    fn propriete_32(&self, fenetre: Window, atome: u32, genre: impl Into<u32>) -> Vec<u32> {
        self.propriete(fenetre, atome, genre)
            .map(|octets| {
                octets
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_ne_bytes(*c))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn zone_racine(&self) -> Resultat<Zone> {
        let g = self
            .connexion
            .get_geometry(self.racine)
            .map_err(echec)?
            .reply()
            .map_err(echec)?;
        Ok(Zone {
            x: 0,
            y: 0,
            largeur: g.width,
            hauteur: g.height,
        })
    }

    fn ecrans(&self) -> Resultat<Vec<(u32, String, Zone, bool)>> {
        let moniteurs = self
            .connexion
            .randr_get_monitors(self.racine, true)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.monitors)
            .unwrap_or_default();
        if moniteurs.is_empty() {
            return Ok(vec![(0, NOM_ECRAN.into(), self.zone_racine()?, true)]);
        }
        let aucun_principal = !moniteurs.iter().any(|m| m.primary);
        Ok(moniteurs
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                let nom = self
                    .connexion
                    .get_atom_name(m.name)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| NOM_ECRAN.into());
                let zone = Zone {
                    x: m.x,
                    y: m.y,
                    largeur: m.width,
                    hauteur: m.height,
                };
                (m.name, nom, zone, m.primary || (aucun_principal && i == 0))
            })
            .collect())
    }

    fn zone_ecran(&self, numero: u32) -> Resultat<Zone> {
        self.ecrans()?
            .into_iter()
            .find(|(id, ..)| *id == numero)
            .map(|(_, _, zone, _)| zone)
            .ok_or_else(|| echec("écran introuvable"))
    }

    fn titre(&self, fenetre: Window) -> String {
        self.propriete(fenetre, self.atomes.nom, self.atomes.utf8)
            .or_else(|| self.propriete(fenetre, AtomEnum::WM_NAME.into(), AtomEnum::ANY))
            .map(|octets| String::from_utf8_lossy(&octets).into_owned())
            .unwrap_or_default()
    }

    fn application(&self, fenetre: Window) -> String {
        let classe = self
            .propriete(fenetre, AtomEnum::WM_CLASS.into(), AtomEnum::STRING)
            .unwrap_or_default();
        let mut parties = classe
            .split(|o| *o == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned());
        let instance = parties.next().unwrap_or_default();
        parties.next().unwrap_or(instance)
    }

    fn etat(&self, fenetre: Window) -> Result<EtatFenetre, Erreur> {
        let attributs = match self
            .connexion
            .get_window_attributes(fenetre)
            .map_err(echec)?
            .reply()
        {
            Ok(a) => a,
            Err(ReplyError::X11Error(_)) => return Ok(EtatFenetre::Fermee),
            Err(e) => return Err(echec(e)),
        };
        let cachee = self
            .propriete_32(fenetre, self.atomes.etat, AtomEnum::ATOM)
            .contains(&self.atomes.cachee);
        Ok(if attributs.map_state != MapState::VIEWABLE || cachee {
            EtatFenetre::Reduite
        } else {
            EtatFenetre::Visible
        })
    }

    fn origine(&self, fenetre: Window) -> (i32, i32) {
        self.connexion
            .translate_coordinates(fenetre, self.racine, 0, 0)
            .ok()
            .and_then(|c| c.reply().ok())
            .map_or((0, 0), |r| (i32::from(r.dst_x), i32::from(r.dst_y)))
    }

    fn decrire(&self, fenetre: Window) -> Option<SourceCapture> {
        let geometrie = self.connexion.get_geometry(fenetre).ok()?.reply().ok()?;
        let titre = self.titre(fenetre);
        let reduite = !matches!(self.etat(fenetre).ok()?, EtatFenetre::Visible);
        let (largeur, hauteur) = (u32::from(geometrie.width), u32::from(geometrie.height));
        if !fenetre_retenue(&titre, largeur, hauteur, reduite) {
            return None;
        }
        let processus = self
            .propriete_32(fenetre, self.atomes.pid, AtomEnum::CARDINAL)
            .first()
            .copied();
        Some(SourceCapture {
            id: IdSource::Fenetre(fenetre).to_string(),
            nom: titre,
            application: self.application(fenetre),
            genre: TypeSource::Fenetre,
            largeur,
            hauteur,
            principale: false,
            processus,
            estompe: processus == Some(std::process::id()),
        })
    }

    fn sources(&self) -> Resultat<Vec<SourceCapture>> {
        let mut sources: Vec<SourceCapture> = self
            .ecrans()?
            .into_iter()
            .map(|(id, nom, zone, principale)| SourceCapture {
                id: IdSource::Ecran(id).to_string(),
                nom,
                application: String::new(),
                genre: TypeSource::Ecran,
                largeur: u32::from(zone.largeur),
                hauteur: u32::from(zone.hauteur),
                principale,
                processus: None,
                estompe: false,
            })
            .collect();
        let fenetres = self.propriete_32(self.racine, self.atomes.clients, AtomEnum::WINDOW);
        sources.extend(fenetres.into_iter().rev().filter_map(|f| self.decrire(f)));
        Ok(sources)
    }

    fn image(&self, dessin: Drawable, zone: Zone) -> Resultat<Image> {
        let reponse = self
            .connexion
            .get_image(
                ImageFormat::Z_PIXMAP,
                dessin,
                zone.x,
                zone.y,
                zone.largeur,
                zone.hauteur,
                u32::MAX,
            )
            .map_err(echec)?
            .reply()
            .map_err(echec)?;
        let (largeur, hauteur) = (usize::from(zone.largeur), usize::from(zone.hauteur));
        if !matches!(reponse.depth, 24 | 32) || hauteur == 0 {
            return Err(echec(format!(
                "profondeur de couleur non prise en charge : {}",
                reponse.depth
            )));
        }
        let pas = reponse.data.len() / hauteur;
        if pas < largeur * 4 {
            return Err(echec("image incomplète"));
        }
        Ok(Image {
            largeur: u32::from(zone.largeur),
            hauteur: u32::from(zone.hauteur),
            rgba: bgra_vers_rgba(&reponse.data, pas, largeur, hauteur),
        })
    }

    fn rediriger(&self, fenetre: Window) {
        let _ = self
            .connexion
            .composite_redirect_window(fenetre, composite::Redirect::AUTOMATIC)
            .map(|c| c.check());
    }

    fn image_fenetre(&self, fenetre: Window) -> Resultat<Image> {
        let geometrie = self
            .connexion
            .get_geometry(fenetre)
            .map_err(echec)?
            .reply()
            .map_err(echec)?;
        let zone = Zone {
            x: 0,
            y: 0,
            largeur: geometrie.width,
            hauteur: geometrie.height,
        };
        let pixmap = self.connexion.generate_id().map_err(echec)?;
        let nommee = self
            .connexion
            .composite_name_window_pixmap(fenetre, pixmap)
            .map_err(echec)?
            .check()
            .is_ok();
        if !nommee {
            return self.image(fenetre, zone);
        }
        let image = self.image(pixmap, zone);
        let _ = self.connexion.free_pixmap(pixmap);
        image.or_else(|_| self.image(fenetre, zone))
    }

    fn curseur(&self, image: &mut Image, origine: (i32, i32)) {
        if !self.curseur {
            return;
        }
        let Some(c) = self
            .connexion
            .xfixes_get_cursor_image()
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return;
        };
        incruster_curseur(
            image,
            i32::from(c.x) - i32::from(c.xhot) - origine.0,
            i32::from(c.y) - i32::from(c.yhot) - origine.1,
            u32::from(c.width),
            u32::from(c.height),
            &c.cursor_image,
        );
    }

    fn relever(&self, id: IdSource, curseur: bool) -> Releve {
        let resultat = match id {
            IdSource::Ecran(numero) => self.zone_ecran(numero).and_then(|zone| {
                let mut image = self.image(self.racine, zone)?;
                if curseur {
                    self.curseur(&mut image, (i32::from(zone.x), i32::from(zone.y)));
                }
                Ok(image)
            }),
            IdSource::Fenetre(fenetre) => match self.etat(fenetre) {
                Ok(EtatFenetre::Fermee) => return Releve::Fermee,
                Ok(EtatFenetre::Reduite) => return Releve::Reduite,
                Ok(EtatFenetre::Visible) => self.image_fenetre(fenetre).map(|mut image| {
                    if curseur {
                        self.curseur(&mut image, self.origine(fenetre));
                    }
                    image
                }),
                Err(e) => Err(e),
            },
        };
        resultat.map_or_else(|e| Releve::Echec(e.to_string()), Releve::Image)
    }

    fn preparer(&self, id: IdSource) {
        if let IdSource::Fenetre(fenetre) = id {
            self.rediriger(fenetre);
        }
    }
}

pub(super) fn disponible() -> bool {
    static DISPONIBLE: OnceLock<bool> = OnceLock::new();
    *DISPONIBLE.get_or_init(|| Session::ouvrir().is_ok())
}

pub(super) fn sources() -> Resultat<Vec<SourceCapture>> {
    Session::ouvrir()?.sources()
}

pub(super) fn capturer(id: IdSource) -> Resultat<Image> {
    let session = Session::ouvrir()?;
    session.preparer(id);
    match session.relever(id, false) {
        Releve::Image(image) => Ok(image),
        Releve::Reduite => Err(echec("fenêtre réduite")),
        Releve::Fermee => Err(echec("fenêtre fermée")),
        Releve::Echec(raison) => Err(Erreur::Echec(raison)),
    }
}

pub(super) fn demarrer(
    id: IdSource,
    options: OptionsCapture,
    recepteur: RecepteurCapture,
) -> Resultat<FluxCapture> {
    let session = Session::ouvrir()?;
    if let IdSource::Fenetre(fenetre) = id {
        if matches!(session.etat(fenetre)?, EtatFenetre::Fermee) {
            return Err(echec("fenêtre introuvable"));
        }
    } else if let IdSource::Ecran(numero) = id {
        session.zone_ecran(numero)?;
    }
    session.preparer(id);
    lancer(
        &id.to_string(),
        options,
        recepteur,
        move |commande, recepteur| {
            sonder(commande, recepteur, |curseur| session.relever(id, curseur))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{CaptureEcran, EvenementCapture};
    use crate::CaptureSysteme;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use x11rb::protocol::xproto::{CreateGCAux, CreateWindowAux, PropMode, Rectangle, WindowClass};
    use x11rb::wrapper::ConnectionExt as _;

    const ROUGE: u32 = 0x00c0_2040;

    fn serveur_x() -> bool {
        if session_wayland() || std::env::var_os("DISPLAY").is_none_or(|d| d.is_empty()) {
            eprintln!("ignoré : aucun serveur X (DISPLAY absent ou session Wayland)");
            return false;
        }
        true
    }

    struct FenetreDeTest {
        connexion: RustConnection,
        racine: Window,
        fenetre: Window,
        clients: u32,
    }

    impl FenetreDeTest {
        fn creer() -> Option<Self> {
            let (connexion, ecran) = x11rb::connect(None).unwrap();
            let racine = connexion.setup().roots[ecran].root;
            let atome = |nom: &str| atome(&connexion, nom).unwrap();
            let (clients, controle) =
                (atome("_NET_CLIENT_LIST"), atome("_NET_SUPPORTING_WM_CHECK"));
            let gestionnaire = connexion
                .get_property(false, racine, controle, AtomEnum::WINDOW, 0, 1)
                .unwrap()
                .reply()
                .unwrap();
            if !gestionnaire.value.is_empty() {
                eprintln!("ignoré : un gestionnaire de fenêtres tient déjà _NET_CLIENT_LIST");
                return None;
            }
            let fenetre = connexion.generate_id().unwrap();
            connexion
                .create_window(
                    0,
                    fenetre,
                    racine,
                    10,
                    20,
                    200,
                    100,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new().background_pixel(ROUGE),
                )
                .unwrap();
            let utf8 = atome("UTF8_STRING");
            connexion
                .change_property8(
                    PropMode::REPLACE,
                    fenetre,
                    atome("_NET_WM_NAME"),
                    utf8,
                    "Fenêtre d'essai".as_bytes(),
                )
                .unwrap();
            connexion
                .change_property8(
                    PropMode::REPLACE,
                    fenetre,
                    AtomEnum::WM_CLASS,
                    AtomEnum::STRING,
                    b"essai\0Essai\0",
                )
                .unwrap();
            connexion
                .change_property32(
                    PropMode::REPLACE,
                    fenetre,
                    atome("_NET_WM_PID"),
                    AtomEnum::CARDINAL,
                    &[std::process::id()],
                )
                .unwrap();
            connexion
                .change_property32(
                    PropMode::REPLACE,
                    racine,
                    clients,
                    AtomEnum::WINDOW,
                    &[fenetre],
                )
                .unwrap();
            let essai = Self {
                connexion,
                racine,
                fenetre,
                clients,
            };
            essai.afficher(true);
            Some(essai)
        }

        fn afficher(&self, visible: bool) {
            if visible {
                self.connexion.map_window(self.fenetre).unwrap();
                let gc = self.connexion.generate_id().unwrap();
                self.connexion
                    .create_gc(gc, self.fenetre, &CreateGCAux::new().foreground(ROUGE))
                    .unwrap();
                self.connexion
                    .poly_fill_rectangle(
                        self.fenetre,
                        gc,
                        &[Rectangle {
                            x: 0,
                            y: 0,
                            width: 200,
                            height: 100,
                        }],
                    )
                    .unwrap();
                self.connexion.free_gc(gc).unwrap();
            } else {
                self.connexion.unmap_window(self.fenetre).unwrap();
            }
            self.connexion.sync().unwrap();
        }

        fn id(&self) -> String {
            IdSource::Fenetre(self.fenetre).to_string()
        }
    }

    impl Drop for FenetreDeTest {
        fn drop(&mut self) {
            let _ = self.connexion.delete_property(self.racine, self.clients);
            let _ = self.connexion.destroy_window(self.fenetre);
            let _ = self.connexion.sync();
        }
    }

    fn attendre(reception: &mpsc::Receiver<EvenementCapture>, voulu: &EvenementCapture) {
        let fin = Instant::now() + Duration::from_secs(5);
        loop {
            let reste = fin.saturating_duration_since(Instant::now());
            match reception.recv_timeout(reste) {
                Ok(e) if &e == voulu => return,
                Ok(EvenementCapture::Echec(raison)) => panic!("échec de la capture : {raison}"),
                Ok(_) => {}
                Err(_) => panic!("{voulu:?} attendu"),
            }
        }
    }

    #[test]
    fn x11_liste_les_ecrans_et_capture_l_ecran_principal() {
        if !serveur_x() {
            return;
        }
        let capture = CaptureSysteme;
        assert!(capture.disponible());
        let sources = capture.sources().unwrap();
        let ecran = sources
            .iter()
            .find(|s| s.genre == TypeSource::Ecran)
            .expect("au moins un écran");
        assert!(sources.iter().any(|s| s.principale));
        let image = capture.capturer(&ecran.id).unwrap();
        assert_eq!(
            (image.largeur, image.hauteur),
            (ecran.largeur, ecran.hauteur)
        );
        assert_eq!(
            image.rgba.len(),
            (image.largeur * image.hauteur * 4) as usize
        );
        let miniature = capture.miniature(&ecran.id, 90).unwrap();
        assert_eq!(miniature.hauteur, 90.min(ecran.hauteur));
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
        assert!(
            (15..=34).contains(&trames),
            "{trames} images de l'écran en 1 s à 30 i/s"
        );
        assert!(matches!(
            capture.capturer("ecran:4294967295"),
            Err(Erreur::Echec(_))
        ));
    }

    #[test]
    fn x11_capture_une_fenetre_en_continu_et_signale_reduction_et_fermeture() {
        if !serveur_x() {
            return;
        }
        let Some(essai) = FenetreDeTest::creer() else {
            return;
        };
        let capture = CaptureSysteme;
        let source = capture
            .sources()
            .unwrap()
            .into_iter()
            .find(|s| s.id == essai.id())
            .expect("la fenêtre d'essai est proposée");
        assert_eq!(source.nom, "Fenêtre d'essai");
        assert_eq!(source.application, "Essai");
        assert_eq!((source.largeur, source.hauteur), (200, 100));
        assert_eq!(source.processus, Some(std::process::id()));
        assert!(
            source.estompe,
            "une fenêtre de ce processus est celle d'Estompe"
        );
        let image = capture.capturer(&source.id).unwrap();
        assert_eq!((image.largeur, image.hauteur), (200, 100));
        assert_eq!(&image.rgba[..4], &[0xc0, 0x20, 0x40, 255]);

        let (envoi, reception) = mpsc::channel();
        let flux = capture
            .demarrer(
                &source.id,
                OptionsCapture {
                    ips: 30,
                    curseur: true,
                },
                Box::new(move |e| envoi.send(e).is_ok()),
            )
            .unwrap();
        let depart = Instant::now();
        let mut trames = 0;
        while depart.elapsed() < Duration::from_millis(1000) {
            if let Ok(EvenementCapture::Trame(image)) =
                reception.recv_timeout(Duration::from_millis(100))
            {
                assert_eq!((image.largeur, image.hauteur), (200, 100));
                trames += 1;
            }
        }
        assert!(
            (20..=34).contains(&trames),
            "{trames} images en 1 s à 30 i/s"
        );
        essai.afficher(false);
        attendre(&reception, &EvenementCapture::Reduite);
        assert!(capture
            .sources()
            .unwrap()
            .iter()
            .all(|s| s.id != essai.id()));
        essai.afficher(true);
        attendre(&reception, &EvenementCapture::Restauree);
        drop(essai);
        attendre(&reception, &EvenementCapture::Fermee);
        let fin = Instant::now() + Duration::from_secs(2);
        while flux.actif() && Instant::now() < fin {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !flux.actif(),
            "le flux s'arrête quand la fenêtre est fermée"
        );
    }

    #[test]
    fn x11_refuse_une_fenetre_inexistante() {
        if !serveur_x() {
            return;
        }
        let capture = CaptureSysteme;
        assert!(capture
            .demarrer("fenetre:1", OptionsCapture::default(), Box::new(|_| true))
            .is_err());
    }
}
