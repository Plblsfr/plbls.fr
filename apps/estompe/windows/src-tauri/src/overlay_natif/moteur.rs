use super::{initiales, taille_avatar, EtatOverlay, Participant, Placement};
use base64::Engine as _;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{Image, Model, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

const POMPE: Duration = Duration::from_millis(100);
const CACHE_AVATARS: usize = 32;
const COTE_AVATAR: u32 = 128;

slint::slint! {
    export struct Ligne { pseudo: string, initiales: string, avatar: image, a-avatar: bool, parle: bool, muet: bool, sourd: bool }

    export component Overlay inherits Window {
        in property <[Ligne]> lignes;
        in property <length> avatar: 40px;
        background: transparent;
        default-font-family: "Segoe UI";
        VerticalLayout {
            alignment: start;
            spacing: 8px;
            for l in lignes: HorizontalLayout {
                spacing: 10px;
                alignment: start;
                Rectangle {
                    width: avatar;
                    height: avatar;
                    border-radius: self.width / 2;
                    background: #444;
                    border-width: 3px;
                    border-color: l.parle ? #4ade80 : #444;
                    drop-shadow-blur: l.parle ? 10px : 0;
                    drop-shadow-color: #4ade80;
                    if l.a-avatar: Image { x: 3px; y: 3px; width: parent.width - 6px; height: self.width; source: l.avatar; image-fit: fill; }
                    if !l.a-avatar: Text { text: l.initiales; color: white; font-size: 16px; font-weight: 600; horizontal-alignment: center; vertical-alignment: center; }
                }
                Rectangle {
                    background: #00000099;
                    border-radius: 6px;
                    height: 21px;
                    y: (parent.height - self.height) / 2;
                    horizontal-stretch: 0;
                    HorizontalLayout {
                        padding-left: 8px;
                        padding-right: 8px;
                        Text { text: l.pseudo; color: white; font-size: 14px; vertical-alignment: center; overflow: elide; }
                    }
                }
                if l.sourd || l.muet: Text { text: l.sourd ? "🔇" : "🎙️🚫"; font-size: 16px; vertical-alignment: center; }
            }
        }
    }
}

pub enum Commande {
    Afficher(EtatOverlay, Placement),
    Fermer,
}

pub trait Sortie {
    fn afficher(
        &mut self,
        pixels: &[PremultipliedRgbaColor],
        largeur: u32,
        hauteur: u32,
        x: i32,
        y: i32,
    );
    fn masquer(&mut self);
    fn pomper(&mut self) {}
}

struct Plateforme(Rc<MinimalSoftwareWindow>);

impl Platform for Plateforme {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

pub fn disque(source: image::RgbaImage) -> image::RgbaImage {
    let cote = source.width().min(source.height()).max(1);
    let carre = image::imageops::crop_imm(
        &source,
        (source.width() - cote) / 2,
        (source.height() - cote) / 2,
        cote,
        cote,
    )
    .to_image();
    let mut img = image::imageops::resize(
        &carre,
        COTE_AVATAR,
        COTE_AVATAR,
        image::imageops::FilterType::Triangle,
    );
    let rayon = COTE_AVATAR as f32 / 2.0;
    for (x, y, p) in img.enumerate_pixels_mut() {
        let d = ((x as f32 + 0.5 - rayon).powi(2) + (y as f32 + 0.5 - rayon).powi(2)).sqrt();
        let couverture = (rayon - d + 0.5).clamp(0.0, 1.0);
        p[3] = (p[3] as f32 * couverture).round() as u8;
    }
    img
}

pub fn decoder_avatar(url: &str) -> Option<Image> {
    let (_, donnees) = url.strip_prefix("data:")?.split_once(";base64,")?;
    let octets = base64::engine::general_purpose::STANDARD
        .decode(donnees)
        .ok()?;
    let rgba = disque(image::load_from_memory(&octets).ok()?.into_rgba8());
    let tampon = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    );
    Some(Image::from_rgba8(tampon))
}

struct Avatars(HashMap<String, Option<Image>>);

impl Avatars {
    fn image(&mut self, url: &str) -> Option<Image> {
        if url.is_empty() {
            return None;
        }
        if self.0.len() >= CACHE_AVATARS && !self.0.contains_key(url) {
            self.0.clear();
        }
        self.0
            .entry(url.to_owned())
            .or_insert_with(|| decoder_avatar(url))
            .clone()
    }
}

fn ligne(p: &Participant, avatars: &mut Avatars) -> Ligne {
    let avatar = avatars.image(&p.avatar);
    Ligne {
        pseudo: p.pseudo.as_str().into(),
        initiales: initiales(&p.pseudo).into(),
        a_avatar: avatar.is_some(),
        avatar: avatar.unwrap_or_default(),
        parle: p.speaking,
        muet: p.muted,
        sourd: p.deaf,
    }
}

pub fn boucle(rx: Receiver<Commande>, mut sortie: impl Sortie) {
    let fenetre = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    if slint::platform::set_platform(Box::new(Plateforme(fenetre.clone()))).is_err() {
        return;
    }
    let Ok(ui) = Overlay::new() else { return };
    let modele = Rc::new(VecModel::<Ligne>::default());
    ui.set_lignes(modele.clone().into());
    if ui.show().is_err() {
        return;
    }
    let mut avatars = Avatars(HashMap::new());
    let mut tampon: Vec<PremultipliedRgbaColor> = Vec::new();
    loop {
        match rx.recv_timeout(POMPE) {
            Ok(Commande::Afficher(etat, p)) => {
                let (l, h) = (p.largeur.max(1), p.hauteur.max(1));
                fenetre.dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: p.echelle,
                });
                fenetre.set_size(PhysicalSize::new(l, h));
                ui.set_avatar(taille_avatar(&etat.size) as f32);
                let lignes: Vec<Ligne> =
                    etat.items.iter().map(|i| ligne(i, &mut avatars)).collect();
                if modele.row_count() != lignes.len()
                    || modele.iter().zip(&lignes).any(|(a, b)| a != *b)
                {
                    modele.set_vec(lignes);
                }
                slint::platform::update_timers_and_animations();
                tampon.clear();
                tampon.resize((l * h) as usize, PremultipliedRgbaColor::default());
                fenetre.request_redraw();
                fenetre.draw_if_needed(|r| {
                    r.render(&mut tampon, l as usize);
                });
                sortie.afficher(&tampon, l, h, p.x, p.y);
            }
            Ok(Commande::Fermer) => sortie.masquer(),
            Err(RecvTimeoutError::Timeout) => sortie.pomper(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

static CANAL: Mutex<Option<Sender<Commande>>> = Mutex::new(None);

fn demarrer() -> Result<Sender<Commande>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("overlay-natif".into())
        .spawn(move || {
            #[cfg(windows)]
            match super::calque::Calque::nouveau() {
                Some(c) => boucle(rx, c),
                None => crate::debuglog::log(
                    "error",
                    "overlay-natif",
                    "fenêtre native impossible à créer",
                ),
            }
            #[cfg(not(windows))]
            boucle(rx, SansSortie);
        })
        .map_err(|e| e.to_string())?;
    Ok(tx)
}

pub fn envoyer(c: Commande) -> Result<(), String> {
    let mut canal = CANAL.lock().map_err(|e| e.to_string())?;
    if canal.is_none() {
        *canal = Some(demarrer()?);
    }
    canal
        .as_ref()
        .ok_or("overlay natif indisponible")?
        .send(c)
        .map_err(|_| "overlay natif arrêté".to_string())
}

#[cfg(not(windows))]
struct SansSortie;

#[cfg(not(windows))]
impl Sortie for SansSortie {
    fn afficher(&mut self, _: &[PremultipliedRgbaColor], _: u32, _: u32, _: i32, _: i32) {}
    fn masquer(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::SyncSender;

    struct Capture(SyncSender<Option<(Vec<PremultipliedRgbaColor>, u32, u32)>>);

    impl Sortie for Capture {
        fn afficher(
            &mut self,
            pixels: &[PremultipliedRgbaColor],
            largeur: u32,
            hauteur: u32,
            _: i32,
            _: i32,
        ) {
            let _ = self.0.send(Some((pixels.to_vec(), largeur, hauteur)));
        }
        fn masquer(&mut self) {
            let _ = self.0.send(None);
        }
    }

    fn participant(pseudo: &str, avatar: &str, speaking: bool, muted: bool) -> Participant {
        Participant {
            id: pseudo.into(),
            pseudo: pseudo.into(),
            avatar: avatar.into(),
            speaking,
            muted,
            deaf: false,
        }
    }

    fn png_rouge() -> String {
        let mut octets = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 30, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut octets),
                image::ImageFormat::Png,
            )
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(octets)
        )
    }

    #[test]
    fn avatar_recadre_en_disque_transparent_aux_coins() {
        let d = disque(image::RgbaImage::from_pixel(
            200,
            100,
            image::Rgba([1, 2, 3, 255]),
        ));
        assert_eq!(d.dimensions(), (COTE_AVATAR, COTE_AVATAR));
        assert_eq!(d.get_pixel(0, 0)[3], 0);
        assert_eq!(d.get_pixel(COTE_AVATAR / 2, COTE_AVATAR / 2)[3], 255);
        assert_eq!(d.get_pixel(10, 10)[3], 0);
        assert!(d.get_pixel(COTE_AVATAR / 2, 0)[3] > 0);
    }

    #[test]
    fn decode_les_avatars_en_data_url_et_ignore_le_reste() {
        assert!(decoder_avatar(&png_rouge()).is_some());
        assert!(decoder_avatar("blob:https://tauri.localhost/x").is_none());
        assert!(decoder_avatar("data:image/png;base64,pasdubase64").is_none());
    }

    #[test]
    fn rendu_transparent_avec_contour_vert_du_locuteur_et_avatar() {
        let (tx, rx) = mpsc::channel();
        let (cap, frames) = mpsc::sync_channel(4);
        std::thread::spawn(move || boucle(rx, Capture(cap)));
        let etat = EtatOverlay {
            size: "md".into(),
            items: vec![
                participant("alice", "", true, false),
                participant("bob_le_bricoleur", &png_rouge(), false, true),
            ],
        };
        tx.send(Commande::Afficher(
            etat,
            Placement {
                x: 0,
                y: 0,
                largeur: 260,
                hauteur: 96,
                echelle: 1.0,
            },
        ))
        .unwrap();
        let (px, l, h) = frames
            .recv_timeout(Duration::from_secs(20))
            .unwrap()
            .unwrap();
        assert_eq!((l, h), (260, 96));
        let at = |x: u32, y: u32| px[(y * l + x) as usize];
        assert_eq!(at(255, 90).alpha, 0);
        let anneau = at(20, 1);
        assert!(
            anneau.green > 150 && anneau.red < 120 && anneau.alpha > 200,
            "{anneau:?}"
        );
        let avatar = at(20, 48 + 20);
        assert!(avatar.red > 180 && avatar.green < 80, "{avatar:?}");
        let nom = (60..200)
            .map(|x| at(x, 20))
            .filter(|c| c.alpha > 100)
            .count();
        if let Some(chemin) = std::env::var_os("OVERLAY_NATIF_CAPTURE") {
            let brut = |v: u8, a: u8| {
                if a == 0 {
                    0
                } else {
                    (v as u32 * 255 / a as u32).min(255) as u8
                }
            };
            let rgba: Vec<u8> = px
                .iter()
                .flat_map(|c| {
                    [
                        brut(c.red, c.alpha),
                        brut(c.green, c.alpha),
                        brut(c.blue, c.alpha),
                        c.alpha,
                    ]
                })
                .collect();
            image::save_buffer(chemin, &rgba, l, h, image::ExtendedColorType::Rgba8).unwrap();
        }
        assert!(nom > 20, "{nom}");
        assert_eq!(at(20, 48 + 2).alpha, 255);
        assert!(at(48 + 6, 48 + 6).alpha < 255);
        tx.send(Commande::Fermer).unwrap();
        assert!(frames
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_none());
    }
}
