use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use estompe_plateforme::Image;
use serde::{Deserialize, Serialize};

use super::partage::ImagePartage;
use super::video::{yuv_vers_rgb, ImageI420, RecepteurVideo};

pub const TAILLE_MIN: u8 = 10;
pub const TAILLE_MAX: u8 = 50;
pub const MARGE_MAX: u16 = 120;
pub const PAS_MARGE: u16 = 4;
pub const POSITION_MAX: u16 = 1000;
pub const RATIO_APERCU: f32 = 16.0 / 9.0;

pub const COINS: [(u16, u16, &str); 4] = [
    (0, 0, "Haut gauche"),
    (POSITION_MAX, 0, "Haut droite"),
    (0, POSITION_MAX, "Bas gauche"),
    (POSITION_MAX, POSITION_MAX, "Bas droite"),
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormeIncrustation {
    #[default]
    Rectangle,
    Cercle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesIncrustation {
    pub active: bool,
    pub x: u16,
    pub y: u16,
    pub taille: u8,
    pub forme: FormeIncrustation,
    pub marge: u16,
    pub bordure: bool,
    pub ombre: bool,
    pub miroir: bool,
}

impl Default for ReglagesIncrustation {
    fn default() -> Self {
        Self {
            active: false,
            x: POSITION_MAX,
            y: POSITION_MAX,
            taille: 22,
            forme: FormeIncrustation::Rectangle,
            marge: 24,
            bordure: false,
            ombre: true,
            miroir: false,
        }
    }
}

impl ReglagesIncrustation {
    pub fn borne(self) -> Self {
        Self {
            x: self.x.min(POSITION_MAX),
            y: self.y.min(POSITION_MAX),
            taille: self.taille.clamp(TAILLE_MIN, TAILLE_MAX),
            marge: self.marge.min(MARGE_MAX),
            ..self
        }
    }

    pub fn coin(&self) -> Option<usize> {
        COINS
            .iter()
            .position(|(x, y, _)| *x == self.x && *y == self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
}

fn arrondi(v: f32) -> f32 {
    (v + 0.5).floor()
}

fn marge_effective(r: &ReglagesIncrustation, l: f32, h: f32, rl: f32, rh: f32) -> f32 {
    f32::from(r.marge)
        .min(((l - rl) / 2.0).max(0.0))
        .min(((h - rh) / 2.0).max(0.0))
}

pub fn zone_incrustation(r: &ReglagesIncrustation, largeur: u32, hauteur: u32, ratio: f32) -> Zone {
    let (l, h) = (largeur as f32, hauteur as f32);
    let ratio = if ratio.is_finite() && ratio > 0.0 {
        ratio
    } else {
        RATIO_APERCU
    };
    let rl = arrondi(f32::from(r.taille) / 100.0 * l);
    let rh = arrondi(match r.forme {
        FormeIncrustation::Cercle => rl,
        FormeIncrustation::Rectangle => rl / ratio,
    });
    let m = marge_effective(r, l, h, rl, rh);
    let px = f32::from(r.x) / f32::from(POSITION_MAX);
    let py = f32::from(r.y) / f32::from(POSITION_MAX);
    Zone {
        x: arrondi(m + px * (l - rl - 2.0 * m)) as i32,
        y: arrondi(m + py * (h - rh - 2.0 * m)) as i32,
        largeur: rl.max(0.0) as u32,
        hauteur: rh.max(0.0) as u32,
    }
}

pub fn position_depuis_pointeur(
    r: &ReglagesIncrustation,
    largeur: u32,
    hauteur: u32,
    ratio: f32,
    pointeur: (f32, f32),
) -> (u16, u16) {
    let zone = zone_incrustation(r, largeur, hauteur, ratio);
    let (l, h) = (largeur as f32, hauteur as f32);
    let (rl, rh) = (zone.largeur as f32, zone.hauteur as f32);
    let m = marge_effective(r, l, h, rl, rh);
    let axe = |p: f32, cote: f32, taille: f32| {
        let course = cote - taille - 2.0 * m;
        if course <= 0.0 {
            return 0;
        }
        let part = ((p - taille / 2.0 - m) / course).clamp(0.0, 1.0);
        arrondi(part * f32::from(POSITION_MAX)) as u16
    };
    (axe(pointeur.0, l, rl), axe(pointeur.1, h, rh))
}

pub fn rayon_des_coins(forme: FormeIncrustation, largeur: f32, hauteur: f32) -> f32 {
    match forme {
        FormeIncrustation::Cercle => largeur / 2.0,
        FormeIncrustation::Rectangle => largeur.min(hauteur) * 0.12,
    }
}

fn distance_rectangle_arrondi(x: f32, y: f32, largeur: f32, hauteur: f32, rayon: f32) -> f32 {
    let rayon = rayon.min(largeur / 2.0).min(hauteur / 2.0);
    let qx = (x - largeur / 2.0).abs() - (largeur / 2.0 - rayon);
    let qy = (y - hauteur / 2.0).abs() - (hauteur / 2.0 - rayon);
    let exterieur = qx.max(0.0).hypot(qy.max(0.0));
    exterieur + qx.max(qy).min(0.0) - rayon
}

fn lisse(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy)]
struct Poids {
    camera: u16,
    blanc: u8,
    alpha: u8,
}

#[derive(Clone, Copy)]
struct Echantillon {
    debut: u32,
    fin: u32,
    part: u16,
}

fn echantillons(
    origine: f32,
    etendue: f32,
    taille: u32,
    nombre: u32,
    miroir: bool,
) -> Vec<Echantillon> {
    let max = taille.saturating_sub(1) as f32;
    (0..nombre)
        .map(|i| {
            let u = (i as f32 + 0.5) / nombre.max(1) as f32;
            let u = if miroir { 1.0 - u } else { u };
            let p = (origine + u * etendue - 0.5).clamp(0.0, max);
            let debut = p.floor();
            Echantillon {
                debut: debut as u32,
                fin: ((debut as u32) + 1).min(taille.saturating_sub(1)),
                part: ((p - debut) * 256.0) as u16,
            }
        })
        .collect()
}

pub struct Gabarit {
    zone: Zone,
    medaillon: Zone,
    camera: (u32, u32),
    poids: Vec<Poids>,
    colonnes: Vec<Echantillon>,
    rangees: Vec<Echantillon>,
}

impl Gabarit {
    pub fn nouveau(camera: (u32, u32), r: &ReglagesIncrustation, zone: Zone) -> Self {
        let (lw, lh) = (zone.largeur as f32, zone.hauteur as f32);
        let rayon = rayon_des_coins(r.forme, lw, lh);
        let flou = (lw * 0.04).max(4.0);
        let decalage = (lw * 0.01).max(2.0);
        let marge = if r.ombre {
            (flou + decalage).ceil() as u32
        } else {
            0
        };
        let bordure = (lw * 0.015).max(2.0);
        let (vl, vh) = (zone.largeur + 2 * marge, zone.hauteur + 2 * marge);
        let mut poids = Vec::with_capacity((vl * vh) as usize);
        for j in 0..vh {
            let ly = j as f32 - marge as f32 + 0.5;
            for i in 0..vl {
                let lx = i as f32 - marge as f32 + 0.5;
                let d = distance_rectangle_arrondi(lx, ly, lw, lh, rayon);
                let couverture = (0.5 - d).clamp(0.0, 1.0);
                let contour = if r.bordure {
                    let dans_la_boite =
                        (lx.min(lw - lx).min(ly).min(lh - ly) + 0.5).clamp(0.0, 1.0);
                    (bordure / 2.0 + 0.5 - d.abs()).clamp(0.0, 1.0) * dans_la_boite
                } else {
                    0.0
                };
                let ombre = if r.ombre {
                    let ds = distance_rectangle_arrondi(lx, ly - decalage, lw, lh, rayon);
                    0.5 * (1.0 - lisse((ds + flou) / (2.0 * flou)))
                } else {
                    0.0
                };
                let alpha = contour + couverture * (1.0 - contour);
                let total = alpha + ombre * (1.0 - alpha);
                poids.push(Poids {
                    camera: (couverture * (1.0 - contour) * 256.0).round() as u16,
                    blanc: (contour * 255.0).round() as u8,
                    alpha: (total * 255.0).round() as u8,
                });
            }
        }
        let (cw, ch) = (camera.0 as f32, camera.1 as f32);
        let s = cw.min(ch * (lw / lh.max(1.0)));
        let sh = s * (lh / lw.max(1.0));
        let colonnes = echantillons((cw - s) / 2.0, s, camera.0, zone.largeur, r.miroir);
        let rangees = echantillons((ch - sh) / 2.0, sh, camera.1, zone.hauteur, false);
        Self {
            zone: Zone {
                x: zone.x - marge as i32,
                y: zone.y - marge as i32,
                largeur: vl,
                hauteur: vh,
            },
            medaillon: zone,
            camera,
            poids,
            colonnes,
            rangees,
        }
    }

    pub fn rendre(&self, camera: &ImageI420) -> Vignette {
        let mut pixels = vec![0u8; self.poids.len() * 4];
        let marge_x = (self.medaillon.x - self.zone.x) as usize;
        let marge_y = (self.medaillon.y - self.zone.y) as usize;
        let vl = self.zone.largeur as usize;
        let l = camera.largeur as usize;
        let cl = camera.largeur_chroma() as usize;
        let utilisable = (camera.largeur, camera.hauteur) == self.camera && camera.valide();
        for (j, (rangee, poids)) in pixels
            .chunks_exact_mut(vl * 4)
            .zip(self.poids.chunks_exact(vl))
            .enumerate()
        {
            let source_y = j
                .checked_sub(marge_y)
                .and_then(|y| self.rangees.get(y))
                .filter(|_| utilisable);
            for (i, (pixel, p)) in rangee.chunks_exact_mut(4).zip(poids).enumerate() {
                let mut rgb = [0u32; 3];
                if p.camera > 0 {
                    if let (Some(sy), Some(sx)) = (
                        source_y,
                        i.checked_sub(marge_x).and_then(|x| self.colonnes.get(x)),
                    ) {
                        let luma =
                            |x: u32, y: u32| u32::from(camera.y[y as usize * l + x as usize]);
                        let (fx, fy) = (u32::from(sx.part), u32::from(sy.part));
                        let haut =
                            luma(sx.debut, sy.debut) * (256 - fx) + luma(sx.fin, sy.debut) * fx;
                        let bas = luma(sx.debut, sy.fin) * (256 - fx) + luma(sx.fin, sy.fin) * fx;
                        let y = ((haut * (256 - fy) + bas * fy + 32768) >> 16) as u8;
                        let c = (sy.debut as usize / 2) * cl + sx.debut as usize / 2;
                        let [r, g, b] = yuv_vers_rgb(y, camera.u[c], camera.v[c]);
                        let k = u32::from(p.camera);
                        rgb = [
                            (u32::from(r) * k) >> 8,
                            (u32::from(g) * k) >> 8,
                            (u32::from(b) * k) >> 8,
                        ];
                    }
                }
                let blanc = u32::from(p.blanc);
                pixel[0] = (rgb[0] + blanc).min(255) as u8;
                pixel[1] = (rgb[1] + blanc).min(255) as u8;
                pixel[2] = (rgb[2] + blanc).min(255) as u8;
                pixel[3] = p.alpha;
            }
        }
        Vignette {
            zone: self.zone,
            pixels,
        }
    }
}

pub struct Vignette {
    pub zone: Zone,
    pub pixels: Vec<u8>,
}

pub struct Piece {
    pub zone: Zone,
    pub pixels: Vec<u8>,
}

impl Piece {
    pub fn restaurer(&self, image: &mut Image) {
        let pas = image.largeur as usize * 4;
        let ligne = self.zone.largeur as usize * 4;
        for (j, source) in self.pixels.chunks_exact(ligne).enumerate() {
            let debut = (self.zone.y as usize + j) * pas + self.zone.x as usize * 4;
            if let Some(cible) = image.rgba.get_mut(debut..debut + ligne) {
                cible.copy_from_slice(source);
            }
        }
    }
}

pub fn poser(vignette: &Vignette, image: &mut Image) -> Option<Piece> {
    let z = vignette.zone;
    let x0 = z.x.max(0);
    let y0 = z.y.max(0);
    let x1 = (z.x + z.largeur as i32).min(image.largeur as i32);
    let y1 = (z.y + z.hauteur as i32).min(image.hauteur as i32);
    if x1 <= x0 || y1 <= y0 || image.rgba.len() < (image.largeur * image.hauteur * 4) as usize {
        return None;
    }
    let (l, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let pas = image.largeur as usize * 4;
    let pas_vignette = z.largeur as usize * 4;
    let mut origine = Vec::with_capacity(l * h * 4);
    for j in 0..h {
        let debut = (y0 as usize + j) * pas + x0 as usize * 4;
        let cible = &mut image.rgba[debut..debut + l * 4];
        origine.extend_from_slice(cible);
        let sj = (y0 - z.y) as usize + j;
        let si = (x0 - z.x) as usize;
        let source = &vignette.pixels[sj * pas_vignette + si * 4..][..l * 4];
        for (c, s) in cible.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
            let a = u32::from(s[3]);
            if a == 0 {
                continue;
            }
            let reste = 255 - a;
            for k in 0..3 {
                c[k] = (u32::from(s[k]) + (u32::from(c[k]) * reste + 127) / 255).min(255) as u8;
            }
            c[3] = (a + (u32::from(c[3]) * reste + 127) / 255).min(255) as u8;
        }
    }
    Some(Piece {
        zone: Zone {
            x: x0,
            y: y0,
            largeur: l as u32,
            hauteur: h as u32,
        },
        pixels: origine,
    })
}

pub fn silhouette() -> Arc<ImageI420> {
    static IMAGE: OnceLock<Arc<ImageI420>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let (l, h) = (320u32, 180u32);
            let mut rgb = Vec::with_capacity((l * h * 3) as usize);
            for y in 0..h {
                for x in 0..l {
                    let (fx, fy) = (x as f32 / l as f32, y as f32 / h as f32);
                    let tete = ((fx - 0.5) / 0.11).hypot((fy - 0.42) / 0.2) <= 1.0;
                    let epaules = ((fx - 0.5) / 0.3).hypot((fy - 1.05) / 0.38) <= 1.0;
                    let pixel = if tete || epaules {
                        [0xc9, 0xc3, 0xd8]
                    } else {
                        let t = (fx + fy) / 2.0;
                        [
                            (0x2b as f32 + t * (0x4a - 0x2b) as f32) as u8,
                            (0x2f as f32 + t * (0x3a - 0x2f) as f32) as u8,
                            (0x4f as f32 + t * (0x5c - 0x4f) as f32) as u8,
                        ]
                    };
                    rgb.extend_from_slice(&pixel);
                }
            }
            Arc::new(ImageI420::depuis_rgb(l, h, &rgb, l as usize * 3))
        })
        .clone()
}

struct Cache {
    gabarit: Gabarit,
    cle_gabarit: (ReglagesIncrustation, u32, u32, u32, u32),
    vignette: Option<(u64, Vignette)>,
}

#[derive(Default)]
struct EtatCompositeur {
    reglages: ReglagesIncrustation,
    camera: Option<Arc<ImageI420>>,
    generation: u64,
    cache: Option<Cache>,
}

#[derive(Default)]
pub struct Compositeur {
    etat: Mutex<EtatCompositeur>,
}

impl Compositeur {
    pub fn nouveau(reglages: ReglagesIncrustation) -> Arc<Self> {
        let c = Self::default();
        c.verrou().reglages = reglages.borne();
        Arc::new(c)
    }

    fn verrou(&self) -> MutexGuard<'_, EtatCompositeur> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn regler(&self, reglages: ReglagesIncrustation) {
        self.verrou().reglages = reglages.borne();
    }

    pub fn reglages(&self) -> ReglagesIncrustation {
        self.verrou().reglages
    }

    pub fn camera(&self, image: Option<Arc<ImageI420>>) {
        let mut etat = self.verrou();
        etat.camera = image.filter(|i| i.valide() && i.largeur > 0 && i.hauteur > 0);
        etat.generation += 1;
    }

    pub fn a_une_camera(&self) -> bool {
        self.verrou().camera.is_some()
    }

    pub fn ratio(&self) -> f32 {
        self.verrou()
            .camera
            .as_ref()
            .map_or(RATIO_APERCU, |c| c.largeur as f32 / c.hauteur.max(1) as f32)
    }

    pub fn incruster(&self, image: &mut Image) -> Option<Piece> {
        let mut etat = self.verrou();
        let camera = etat.camera.clone()?;
        let generation = etat.generation;
        Self::composer(&mut etat, &camera, generation, image)
    }

    pub fn incruster_apercu(&self, image: &mut Image) -> Option<Piece> {
        let mut etat = self.verrou();
        let (camera, generation) = match etat.camera.clone() {
            Some(c) => (c, etat.generation),
            None => (silhouette(), 0),
        };
        Self::composer(&mut etat, &camera, generation, image)
    }

    fn composer(
        etat: &mut EtatCompositeur,
        camera: &ImageI420,
        generation: u64,
        image: &mut Image,
    ) -> Option<Piece> {
        if image.largeur == 0 || image.hauteur == 0 {
            return None;
        }
        let cle = (
            etat.reglages,
            image.largeur,
            image.hauteur,
            camera.largeur,
            camera.hauteur,
        );
        if etat.cache.as_ref().is_none_or(|c| c.cle_gabarit != cle) {
            let ratio = camera.largeur as f32 / camera.hauteur.max(1) as f32;
            let zone = zone_incrustation(&etat.reglages, image.largeur, image.hauteur, ratio);
            if zone.largeur < 2 || zone.hauteur < 2 {
                etat.cache = None;
                return None;
            }
            etat.cache = Some(Cache {
                gabarit: Gabarit::nouveau((camera.largeur, camera.hauteur), &etat.reglages, zone),
                cle_gabarit: cle,
                vignette: None,
            });
        }
        let cache = etat.cache.as_mut()?;
        if cache
            .vignette
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            cache.vignette = Some((generation, cache.gabarit.rendre(camera)));
        }
        poser(&cache.vignette.as_ref()?.1, image)
    }
}

#[derive(Default)]
pub struct ApercuIncruste {
    derniere: Mutex<Option<(Arc<ImagePartage>, Option<Piece>)>>,
}

impl ApercuIncruste {
    pub fn nouveau() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn pousser(
        &self,
        mut image: Image,
        compositeur: Option<&Compositeur>,
        recepteur: &RecepteurVideo,
    ) {
        let piece = compositeur
            .filter(|c| c.reglages().active)
            .and_then(|c| c.incruster_apercu(&mut image));
        let image = Arc::new(ImagePartage(image));
        recepteur.pousser(image.clone());
        if let Ok(mut d) = self.derniere.lock() {
            *d = Some((image, piece));
        }
    }

    pub fn recomposer(&self, compositeur: Option<&Compositeur>, recepteur: &RecepteurVideo) {
        let base = match self.derniere.lock() {
            Ok(d) => match d.as_ref() {
                Some((image, piece)) => {
                    let mut base = image.0.clone();
                    if let Some(piece) = piece {
                        piece.restaurer(&mut base);
                    }
                    base
                }
                None => return,
            },
            Err(_) => return,
        };
        self.pousser(base, compositeur, recepteur);
    }

    pub fn oublier(&self) {
        if let Ok(mut d) = self.derniere.lock() {
            *d = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn reglages() -> ReglagesIncrustation {
        ReglagesIncrustation {
            active: true,
            ..ReglagesIncrustation::default()
        }
    }

    fn ecran(largeur: u32, hauteur: u32, rgba: [u8; 4]) -> Image {
        Image {
            largeur,
            hauteur,
            rgba: rgba.repeat((largeur * hauteur) as usize),
        }
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.largeur + x) * 4) as usize;
        image.rgba[i..i + 4].try_into().unwrap()
    }

    fn camera_unie(largeur: u32, hauteur: u32, rgb: [u8; 3]) -> Arc<ImageI420> {
        let donnees = rgb.repeat((largeur * hauteur) as usize);
        Arc::new(ImageI420::depuis_rgb(
            largeur,
            hauteur,
            &donnees,
            largeur as usize * 3,
        ))
    }

    fn proche(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
    }

    #[test]
    fn la_zone_par_defaut_est_en_bas_a_droite_avec_la_marge() {
        let z = zone_incrustation(&reglages(), 1920, 1080, 16.0 / 9.0);
        assert_eq!(
            z,
            Zone {
                x: 1920 - 24 - 422,
                y: 1080 - 24 - 237,
                largeur: 422,
                hauteur: 237
            }
        );
    }

    #[test]
    fn la_zone_suit_la_position_la_taille_et_la_forme() {
        let r = ReglagesIncrustation {
            x: 0,
            y: 0,
            taille: 50,
            forme: FormeIncrustation::Cercle,
            marge: 0,
            ..reglages()
        };
        assert_eq!(
            zone_incrustation(&r, 1280, 720, 4.0 / 3.0),
            Zone {
                x: 0,
                y: 0,
                largeur: 640,
                hauteur: 640
            }
        );
        let milieu = ReglagesIncrustation {
            x: 500,
            y: 500,
            taille: 10,
            ..reglages()
        };
        assert_eq!(
            zone_incrustation(&milieu, 1000, 500, 2.0),
            Zone {
                x: 450,
                y: 225,
                largeur: 100,
                hauteur: 50
            }
        );
        let ratio_43 = ReglagesIncrustation {
            x: 0,
            y: 0,
            ..reglages()
        };
        assert_eq!(
            zone_incrustation(&ratio_43, 1920, 1080, 4.0 / 3.0),
            Zone {
                x: 24,
                y: 24,
                largeur: 422,
                hauteur: 317
            }
        );
    }

    #[test]
    fn la_marge_se_reduit_quand_l_incrustation_remplit_l_image() {
        let r = ReglagesIncrustation {
            taille: 50,
            forme: FormeIncrustation::Cercle,
            marge: 120,
            ..reglages()
        };
        let z = zone_incrustation(&r, 400, 220, 1.0);
        assert_eq!((z.largeur, z.hauteur), (200, 200));
        assert_eq!((z.x, z.y), (190, 10));
        let sans_place = zone_incrustation(&r, 400, 150, 1.0);
        assert_eq!((sans_place.x, sans_place.y), (200, -50));
    }

    #[test]
    fn les_reglages_sont_bornes() {
        let r = ReglagesIncrustation {
            x: 4000,
            y: 2000,
            taille: 90,
            marge: 900,
            ..reglages()
        }
        .borne();
        assert_eq!((r.x, r.y, r.taille, r.marge), (1000, 1000, 50, 120));
        assert_eq!(
            ReglagesIncrustation {
                taille: 1,
                ..reglages()
            }
            .borne()
            .taille,
            10
        );
        assert_eq!(reglages().coin(), Some(3));
        assert_eq!(
            ReglagesIncrustation {
                x: 400,
                ..reglages()
            }
            .coin(),
            None
        );
    }

    #[test]
    fn le_pointeur_place_le_centre_de_l_incrustation() {
        let r = reglages();
        assert_eq!(
            position_depuis_pointeur(&r, 640, 360, RATIO_APERCU, (0.0, 0.0)),
            (0, 0)
        );
        assert_eq!(
            position_depuis_pointeur(&r, 640, 360, RATIO_APERCU, (640.0, 360.0)),
            (1000, 1000)
        );
        assert_eq!(
            position_depuis_pointeur(&r, 640, 360, RATIO_APERCU, (320.0, 180.0)),
            (500, 500)
        );
        let (x, y) = position_depuis_pointeur(&r, 640, 360, RATIO_APERCU, (200.0, 100.0));
        let place = ReglagesIncrustation { x, y, ..r };
        let z = zone_incrustation(&place, 640, 360, RATIO_APERCU);
        assert!((z.x + z.largeur as i32 / 2 - 200).abs() <= 1);
        assert!((z.y + z.hauteur as i32 / 2 - 100).abs() <= 1);
    }

    #[test]
    fn la_camera_est_posee_dans_la_zone_et_rien_ailleurs() {
        let c = Compositeur::nouveau(ReglagesIncrustation {
            ombre: false,
            ..reglages()
        });
        let mut image = ecran(1920, 1080, [10, 20, 30, 255]);
        assert!(c.incruster(&mut image).is_none(), "sans caméra, rien");
        assert!(image.rgba.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
        c.camera(Some(camera_unie(1280, 720, [200, 40, 40])));
        let piece = c.incruster(&mut image).unwrap();
        let z = zone_incrustation(&c.reglages(), 1920, 1080, 16.0 / 9.0);
        assert_eq!(piece.zone, z);
        let centre = pixel(
            &image,
            z.x as u32 + z.largeur / 2,
            z.y as u32 + z.hauteur / 2,
        );
        assert!(proche(centre, [200, 40, 40, 255], 3), "{centre:?}");
        assert_eq!(pixel(&image, 100, 100), [10, 20, 30, 255]);
        assert_eq!(
            pixel(&image, z.x as u32 - 1, z.y as u32 + 50),
            [10, 20, 30, 255]
        );
        assert_eq!(
            pixel(&image, z.x as u32, z.y as u32),
            [10, 20, 30, 255],
            "coin arrondi"
        );
        assert_eq!(pixel(&image, 1919, 1079), [10, 20, 30, 255], "marge");
        piece.restaurer(&mut image);
        assert!(image.rgba.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn le_cercle_le_miroir_la_bordure_et_l_ombre() {
        let mut rgb = Vec::new();
        for _ in 0..90 {
            for x in 0..160 {
                rgb.extend_from_slice(if x < 80 {
                    &[220, 30, 30]
                } else {
                    &[30, 30, 220]
                });
            }
        }
        let camera = Arc::new(ImageI420::depuis_rgb(160, 90, &rgb, 480));
        let r = ReglagesIncrustation {
            x: 0,
            y: 0,
            marge: 40,
            forme: FormeIncrustation::Cercle,
            ombre: false,
            ..reglages()
        };
        let c = Compositeur::nouveau(r);
        c.camera(Some(camera));
        let fond = [0, 0, 0, 255];
        let mut image = ecran(1000, 600, fond);
        c.incruster(&mut image);
        let z = zone_incrustation(&r, 1000, 600, 16.0 / 9.0);
        assert_eq!((z.largeur, z.hauteur), (220, 220));
        let (cx, cy) = (z.x as u32 + 110, z.y as u32 + 110);
        assert!(
            pixel(&image, cx - 60, cy)[0] > 150,
            "gauche rouge sans miroir"
        );
        assert!(pixel(&image, cx + 60, cy)[2] > 150);
        assert_eq!(
            pixel(&image, z.x as u32 + 10, z.y as u32 + 10),
            fond,
            "hors du cercle"
        );
        let mut miroir = ecran(1000, 600, fond);
        c.regler(ReglagesIncrustation {
            miroir: true,
            bordure: true,
            ..r
        });
        c.incruster(&mut miroir);
        assert!(pixel(&miroir, cx - 60, cy)[2] > 150, "miroir");
        assert!(
            proche(pixel(&miroir, cx, z.y as u32), [255, 255, 255, 255], 8),
            "bordure"
        );
        let mut ombre = ecran(1000, 600, [200, 200, 200, 255]);
        c.regler(ReglagesIncrustation { ombre: true, ..r });
        c.incruster(&mut ombre);
        let sous = pixel(&ombre, cx, z.y as u32 + 222);
        assert!(sous[0] < 190, "ombre sous le médaillon : {sous:?}");
        assert_eq!(pixel(&ombre, 999, 0), [200, 200, 200, 255]);
    }

    #[test]
    fn camera_coupee_le_medaillon_disparait() {
        let c = Compositeur::nouveau(reglages());
        c.camera(Some(camera_unie(640, 360, [0, 200, 0])));
        let mut image = ecran(640, 360, [0, 0, 0, 255]);
        assert!(c.incruster(&mut image).is_some());
        c.camera(None);
        let mut suivante = ecran(640, 360, [0, 0, 0, 255]);
        assert!(c.incruster(&mut suivante).is_none());
        assert!(suivante.rgba.chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
        assert!(
            c.incruster_apercu(&mut suivante).is_some(),
            "l'aperçu montre une silhouette"
        );
    }

    #[test]
    fn l_apercu_se_recompose_a_chaque_reglage_sans_nouvelle_image() {
        let c = Compositeur::nouveau(ReglagesIncrustation {
            x: 0,
            y: 0,
            ombre: false,
            ..reglages()
        });
        let apercu = ApercuIncruste::nouveau();
        let recepteur = RecepteurVideo::nouveau();
        apercu.pousser(ecran(640, 360, [0, 0, 0, 255]), Some(&c), &recepteur);
        let lire = |x: u32, y: u32| {
            let mut rgba = vec![0u8; 640 * 360 * 4];
            recepteur.derniere().unwrap().vers_rgba(640, 360, &mut rgba);
            let i = ((y * 640 + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_ne!(lire(90, 60), [0, 0, 0, 255], "silhouette en haut à gauche");
        c.regler(ReglagesIncrustation {
            x: 1000,
            y: 1000,
            ombre: false,
            ..reglages()
        });
        apercu.recomposer(Some(&c), &recepteur);
        assert_eq!(
            lire(90, 60),
            [0, 0, 0, 255],
            "l'ancien médaillon est effacé"
        );
        assert_ne!(lire(550, 300), [0, 0, 0, 255]);
        c.regler(ReglagesIncrustation {
            active: false,
            ..c.reglages()
        });
        apercu.recomposer(Some(&c), &recepteur);
        assert_eq!(
            lire(550, 300),
            [0, 0, 0, 255],
            "désactivée, plus de médaillon"
        );
    }

    #[test]
    #[ignore]
    fn cout_de_l_incrustation_a_1080p() {
        let c = Compositeur::nouveau(reglages());
        let mut image = ecran(1920, 1080, [40, 40, 40, 255]);
        let cameras = [
            camera_unie(1280, 720, [200, 90, 90]),
            camera_unie(1280, 720, [90, 200, 90]),
        ];
        let images = 300u32;
        let debut = Instant::now();
        for n in 0..images {
            c.camera(Some(cameras[(n % 2) as usize].clone()));
            let _ = c.incruster(&mut image);
        }
        let par_image = debut.elapsed() / images;
        println!(
            "incrustation 1080p : {:.2} ms par image (nouvelle image de la caméra à chaque image), {:.1} % d'un cœur à 30 i/s",
            par_image.as_secs_f64() * 1000.0,
            par_image.as_secs_f64() * 30.0 * 100.0
        );
    }
}
