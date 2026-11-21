use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::stream::BoxStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceVideo {
    Camera,
    Partage,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CleVideo {
    pub identite: String,
    pub source: SourceVideo,
}

impl CleVideo {
    pub fn camera(identite: &str) -> Self {
        Self {
            identite: identite.to_string(),
            source: SourceVideo::Camera,
        }
    }

    pub fn partage(identite: &str) -> Self {
        Self {
            identite: identite.to_string(),
            source: SourceVideo::Partage,
        }
    }

    pub fn texte(&self) -> String {
        match self.source {
            SourceVideo::Camera => format!("{}#camera", self.identite),
            SourceVideo::Partage => format!("{}#screen", self.identite),
        }
    }
}

pub trait TrameVideo: Send + Sync {
    fn largeur(&self) -> u32;
    fn hauteur(&self) -> u32;
    fn vers_rgba(&self, largeur: u32, hauteur: u32, sortie: &mut [u8]);
}

pub type ImageVideo = Arc<dyn TrameVideo>;
pub type FluxVideo = BoxStream<'static, ImageVideo>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandeVideo {
    pub largeur: u32,
    pub hauteur: u32,
}

pub fn taille_de_conversion(source: (u32, u32), zone: (u32, u32)) -> (u32, u32) {
    let (sl, sh) = (source.0.max(1), source.1.max(1));
    if zone.0 == 0 || zone.1 == 0 {
        return (sl, sh);
    }
    let echelle = (f64::from(zone.0) / f64::from(sl))
        .max(f64::from(zone.1) / f64::from(sh))
        .min(1.0);
    let l = (f64::from(sl) * echelle).round().max(1.0) as u32;
    let h = (f64::from(sh) * echelle).round().max(1.0) as u32;
    (l.min(sl), h.min(sh))
}

pub fn miroir_rgba(pixels: &mut [u8], largeur: u32) {
    let ligne = largeur as usize * 4;
    if ligne == 0 {
        return;
    }
    for rangee in pixels.chunks_exact_mut(ligne) {
        let (pixels, _) = rangee.as_chunks_mut::<4>();
        pixels.reverse();
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ImageI420 {
    pub largeur: u32,
    pub hauteur: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl std::fmt::Debug for ImageI420 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageI420")
            .field("largeur", &self.largeur)
            .field("hauteur", &self.hauteur)
            .finish()
    }
}

fn borne(valeur: i32) -> u8 {
    valeur.clamp(0, 255) as u8
}

pub fn yuv_vers_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = 298 * (i32::from(y) - 16);
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    [
        borne((c + 409 * e + 128) >> 8),
        borne((c - 100 * d - 208 * e + 128) >> 8),
        borne((c + 516 * d + 128) >> 8),
    ]
}

pub fn rgb_vers_yuv(r: u8, g: u8, b: u8) -> [u8; 3] {
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    [
        borne(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16),
        borne(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128),
        borne(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128),
    ]
}

fn correspondance(source: u32, cible: u32) -> Vec<u32> {
    let pas = (u64::from(source) << 16) / u64::from(cible.max(1));
    (0..cible)
        .map(|i| (((u64::from(i) * pas + (pas >> 1)) >> 16) as u32).min(source - 1))
        .collect()
}

impl ImageI420 {
    pub fn noire(largeur: u32, hauteur: u32) -> Self {
        let (cl, ch) = (largeur.div_ceil(2), hauteur.div_ceil(2));
        Self {
            largeur,
            hauteur,
            y: vec![16; (largeur * hauteur) as usize],
            u: vec![128; (cl * ch) as usize],
            v: vec![128; (cl * ch) as usize],
        }
    }

    pub fn largeur_chroma(&self) -> u32 {
        self.largeur.div_ceil(2)
    }

    pub fn hauteur_chroma(&self) -> u32 {
        self.hauteur.div_ceil(2)
    }

    pub fn valide(&self) -> bool {
        let chroma = (self.largeur_chroma() * self.hauteur_chroma()) as usize;
        self.largeur > 0
            && self.hauteur > 0
            && self.y.len() == (self.largeur * self.hauteur) as usize
            && self.u.len() == chroma
            && self.v.len() == chroma
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let cl = self.largeur_chroma();
        let i = (y * self.largeur + x) as usize;
        let c = ((y / 2) * cl + x / 2) as usize;
        yuv_vers_rgb(self.y[i], self.u[c], self.v[c])
    }

    pub fn remplir_rectangle(&mut self, x: u32, y: u32, l: u32, h: u32, rgb: [u8; 3]) {
        let [cy, cu, cv] = rgb_vers_yuv(rgb[0], rgb[1], rgb[2]);
        let x1 = (x + l).min(self.largeur);
        let y1 = (y + h).min(self.hauteur);
        let cl = self.largeur_chroma();
        for ligne in y.min(y1)..y1 {
            let debut = (ligne * self.largeur) as usize;
            self.y[debut + x as usize..debut + x1 as usize].fill(cy);
        }
        for ligne in (y / 2)..y1.div_ceil(2) {
            let debut = (ligne * cl) as usize;
            let (a, b) = ((x / 2) as usize, (x1.div_ceil(2)) as usize);
            self.u[debut + a..debut + b].fill(cu);
            self.v[debut + a..debut + b].fill(cv);
        }
    }

    pub fn depuis_rgb(largeur: u32, hauteur: u32, rgb: &[u8], pas: usize) -> Self {
        let mut image = Self::noire(largeur, hauteur);
        let cl = image.largeur_chroma() as usize;
        for y in 0..hauteur as usize {
            let ligne = &rgb[y * pas..y * pas + largeur as usize * 3];
            for (x, p) in ligne.as_chunks::<3>().0.iter().enumerate() {
                let [luma, u, v] = rgb_vers_yuv(p[0], p[1], p[2]);
                image.y[y * largeur as usize + x] = luma;
                if y % 2 == 0 && x % 2 == 0 {
                    image.u[(y / 2) * cl + x / 2] = u;
                    image.v[(y / 2) * cl + x / 2] = v;
                }
            }
        }
        image
    }

    pub fn depuis_gris(largeur: u32, hauteur: u32, gris: &[u8], pas: usize) -> Self {
        let mut image = Self::noire(largeur, hauteur);
        for y in 0..hauteur as usize {
            let l = largeur as usize;
            image.y[y * l..(y + 1) * l].copy_from_slice(&gris[y * pas..y * pas + l]);
        }
        image
    }

    pub fn depuis_yuyv(largeur: u32, hauteur: u32, yuyv: &[u8], pas: usize) -> Self {
        let mut image = Self::noire(largeur, hauteur);
        let (l, cl) = (largeur as usize, image.largeur_chroma() as usize);
        for y in 0..hauteur as usize {
            let ligne = &yuyv[y * pas..y * pas + l * 2];
            for (x, paire) in ligne.as_chunks::<4>().0.iter().enumerate() {
                image.y[y * l + 2 * x] = paire[0];
                if 2 * x + 1 < l {
                    image.y[y * l + 2 * x + 1] = paire[2];
                }
                if y % 2 == 0 {
                    image.u[(y / 2) * cl + x] = paire[1];
                    image.v[(y / 2) * cl + x] = paire[3];
                }
            }
        }
        image
    }

    pub fn depuis_nv12(
        largeur: u32,
        hauteur: u32,
        y_plan: &[u8],
        pas_y: usize,
        uv: &[u8],
        pas_uv: usize,
    ) -> Self {
        let mut image = Self::noire(largeur, hauteur);
        let (l, cl, ch) = (
            largeur as usize,
            image.largeur_chroma() as usize,
            image.hauteur_chroma() as usize,
        );
        for y in 0..hauteur as usize {
            image.y[y * l..(y + 1) * l].copy_from_slice(&y_plan[y * pas_y..y * pas_y + l]);
        }
        for y in 0..ch {
            let ligne = &uv[y * pas_uv..y * pas_uv + cl * 2];
            for (x, paire) in ligne.as_chunks::<2>().0.iter().enumerate() {
                image.u[y * cl + x] = paire[0];
                image.v[y * cl + x] = paire[1];
            }
        }
        image
    }
}

impl TrameVideo for ImageI420 {
    fn largeur(&self) -> u32 {
        self.largeur
    }

    fn hauteur(&self) -> u32 {
        self.hauteur
    }

    fn vers_rgba(&self, largeur: u32, hauteur: u32, sortie: &mut [u8]) {
        if largeur == 0 || hauteur == 0 || !self.valide() {
            return;
        }
        let colonnes = correspondance(self.largeur, largeur);
        let rangees = correspondance(self.hauteur, hauteur);
        let cl = self.largeur_chroma() as usize;
        let ligne = largeur as usize * 4;
        for (rangee, sy) in sortie.chunks_exact_mut(ligne).zip(rangees) {
            let y = &self.y[(sy * self.largeur) as usize..][..self.largeur as usize];
            let u = &self.u[(sy as usize / 2) * cl..][..cl];
            let v = &self.v[(sy as usize / 2) * cl..][..cl];
            for (pixel, sx) in rangee.as_chunks_mut::<4>().0.iter_mut().zip(&colonnes) {
                let sx = *sx as usize;
                let [r, g, b] = yuv_vers_rgb(y[sx], u[sx / 2], v[sx / 2]);
                *pixel = [r, g, b, 255];
            }
        }
    }
}

const BARRES: [[u8; 3]; 7] = [
    [191, 191, 191],
    [191, 191, 0],
    [0, 191, 191],
    [0, 191, 0],
    [191, 0, 191],
    [191, 0, 0],
    [0, 0, 191],
];

#[derive(Clone)]
pub struct Mire {
    base: ImageI420,
}

impl Mire {
    pub fn nouvelle(largeur: u32, hauteur: u32) -> Self {
        let mut base = ImageI420::noire(largeur, hauteur);
        let haut = hauteur * 2 / 3;
        let pas = largeur.div_ceil(BARRES.len() as u32);
        for (i, couleur) in BARRES.iter().enumerate() {
            base.remplir_rectangle(i as u32 * pas, 0, pas, haut, *couleur);
        }
        let bandes = 8u32;
        let largeur_bande = largeur.div_ceil(bandes);
        for i in 0..bandes {
            let gris = (i * 255 / (bandes - 1)) as u8;
            base.remplir_rectangle(
                i * largeur_bande,
                haut,
                largeur_bande,
                hauteur - haut,
                [gris; 3],
            );
        }
        Self { base }
    }

    pub fn largeur(&self) -> u32 {
        self.base.largeur
    }

    pub fn hauteur(&self) -> u32 {
        self.base.hauteur
    }

    pub fn image(&self, numero: u64) -> ImageI420 {
        let mut image = self.base.clone();
        let cote = (self.base.hauteur / 6).max(2);
        let course = self.base.largeur.saturating_sub(cote).max(1);
        let x = ((numero * 8) % u64::from(course)) as u32;
        let y = self.base.hauteur * 2 / 3 - cote / 2;
        image.remplir_rectangle(x, y, cote, cote, [255, 255, 255]);
        image
    }
}

#[derive(Default)]
struct EtatRecepteur {
    image: Option<ImageVideo>,
    generation: u64,
    ferme: bool,
}

#[derive(Default)]
pub struct RecepteurVideo {
    etat: Mutex<EtatRecepteur>,
    signal: Condvar,
    recues: AtomicU64,
}

impl RecepteurVideo {
    pub fn nouveau() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn verrou(&self) -> MutexGuard<'_, EtatRecepteur> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn pousser(&self, image: ImageVideo) {
        self.recues.fetch_add(1, Ordering::Relaxed);
        let mut etat = self.verrou();
        etat.image = Some(image);
        etat.generation += 1;
        self.signal.notify_all();
    }

    pub fn effacer(&self) {
        let mut etat = self.verrou();
        if etat.image.take().is_some() {
            etat.generation += 1;
            self.signal.notify_all();
        }
    }

    pub fn fermer(&self) {
        let mut etat = self.verrou();
        etat.ferme = true;
        self.signal.notify_all();
    }

    pub fn rouvrir(&self) {
        self.verrou().ferme = false;
    }

    pub fn derniere(&self) -> Option<ImageVideo> {
        self.verrou().image.clone()
    }

    pub fn generation(&self) -> u64 {
        self.verrou().generation
    }

    pub fn images_recues(&self) -> u64 {
        self.recues.load(Ordering::Relaxed)
    }

    pub fn attendre(&self, apres: u64, delai: Duration) -> Option<(u64, Option<ImageVideo>)> {
        let etat = self.verrou();
        let (etat, _) = self
            .signal
            .wait_timeout_while(etat, delai, |e| e.generation == apres && !e.ferme)
            .unwrap_or_else(|e| e.into_inner());
        if etat.ferme || etat.generation == apres {
            return None;
        }
        Some((etat.generation, etat.image.clone()))
    }

    pub fn reveiller(&self) {
        self.signal.notify_all();
    }
}

#[derive(Default)]
pub struct Videos {
    recepteurs: Mutex<HashMap<CleVideo, Arc<RecepteurVideo>>>,
}

impl Videos {
    pub fn nouvelles() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn verrou(&self) -> MutexGuard<'_, HashMap<CleVideo, Arc<RecepteurVideo>>> {
        self.recepteurs.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn recepteur(&self, cle: &CleVideo) -> Arc<RecepteurVideo> {
        self.verrou().entry(cle.clone()).or_default().clone()
    }

    pub fn existant(&self, cle: &CleVideo) -> Option<Arc<RecepteurVideo>> {
        self.verrou().get(cle).cloned()
    }

    pub fn effacer(&self, cle: &CleVideo) {
        if let Some(r) = self.existant(cle) {
            r.effacer();
        }
    }

    pub fn effacer_tout(&self) {
        for r in self.verrou().values() {
            r.effacer();
        }
    }

    pub fn images_recues(&self) -> u64 {
        self.verrou().values().map(|r| r.images_recues()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(image: &dyn TrameVideo, l: u32, h: u32) -> Vec<u8> {
        let mut sortie = vec![0; (l * h * 4) as usize];
        image.vers_rgba(l, h, &mut sortie);
        sortie
    }

    fn proche(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3)
    }

    #[test]
    fn la_conversion_ne_grandit_jamais_et_couvre_la_zone() {
        assert_eq!(taille_de_conversion((1920, 1080), (640, 360)), (640, 360));
        assert_eq!(taille_de_conversion((1920, 1080), (400, 400)), (711, 400));
        assert_eq!(taille_de_conversion((640, 360), (1920, 1080)), (640, 360));
        assert_eq!(taille_de_conversion((1280, 720), (0, 0)), (1280, 720));
        assert_eq!(taille_de_conversion((0, 0), (10, 10)), (1, 1));
    }

    #[test]
    fn les_couleurs_font_l_aller_retour_entre_rgb_et_yuv() {
        for rgb in [
            [0, 0, 0],
            [255, 255, 255],
            [191, 0, 0],
            [0, 191, 0],
            [20, 60, 200],
        ] {
            let [y, u, v] = rgb_vers_yuv(rgb[0], rgb[1], rgb[2]);
            assert!(proche(yuv_vers_rgb(y, u, v), rgb), "{rgb:?}");
        }
    }

    #[test]
    fn une_image_i420_se_convertit_et_se_reduit() {
        let mut image = ImageI420::noire(64, 32);
        image.remplir_rectangle(0, 0, 32, 32, [200, 30, 30]);
        image.remplir_rectangle(32, 0, 32, 32, [30, 30, 200]);
        let pleine = rgba(&image, 64, 32);
        assert!(proche([pleine[0], pleine[1], pleine[2]], [200, 30, 30]));
        let reduite = rgba(&image, 16, 8);
        let droite = (8 * 4 + 15) * 4;
        assert!(proche(
            [reduite[droite], reduite[droite + 1], reduite[droite + 2]],
            [30, 30, 200]
        ));
        assert!(reduite.chunks(4).all(|p| p[3] == 255));
    }

    #[test]
    fn les_formats_de_camera_donnent_la_meme_image() {
        let (l, h) = (8u32, 4u32);
        let rgb: Vec<u8> = (0..l * h)
            .flat_map(|i| {
                if i % l < 4 {
                    [200, 40, 40]
                } else {
                    [40, 40, 200]
                }
            })
            .collect();
        let reference = ImageI420::depuis_rgb(l, h, &rgb, l as usize * 3);
        let mut yuyv = Vec::new();
        for y in 0..h {
            for x in (0..l).step_by(2) {
                let i = (y * l + x) as usize;
                let c = ((y / 2) * reference.largeur_chroma() + x / 2) as usize;
                yuyv.extend([
                    reference.y[i],
                    reference.u[c],
                    reference.y[i + 1],
                    reference.v[c],
                ]);
            }
        }
        assert_eq!(
            ImageI420::depuis_yuyv(l, h, &yuyv, l as usize * 2),
            reference
        );
        let uv: Vec<u8> = reference
            .u
            .iter()
            .zip(&reference.v)
            .flat_map(|(u, v)| [*u, *v])
            .collect();
        assert_eq!(
            ImageI420::depuis_nv12(l, h, &reference.y, l as usize, &uv, l as usize),
            reference
        );
        let gris = ImageI420::depuis_gris(l, h, &reference.y, l as usize);
        assert_eq!(gris.y, reference.y);
    }

    #[test]
    fn le_miroir_inverse_chaque_ligne() {
        let mut pixels: Vec<u8> = (0..2 * 3 * 4).map(|i| i as u8).collect();
        miroir_rgba(&mut pixels, 3);
        assert_eq!(&pixels[0..4], &[8, 9, 10, 11]);
        assert_eq!(&pixels[12..16], &[20, 21, 22, 23]);
    }

    #[test]
    fn la_mire_bouge_d_une_image_a_l_autre() {
        let mire = Mire::nouvelle(320, 180);
        let a = mire.image(0);
        let b = mire.image(5);
        assert!(a.valide() && b.valide());
        assert_ne!(a.y, b.y);
        assert!(proche(a.pixel(1, 1), BARRES[0]));
    }

    #[test]
    fn le_recepteur_garde_la_derniere_image_et_compte_les_autres() {
        let recepteur = RecepteurVideo::nouveau();
        assert!(recepteur.attendre(0, Duration::ZERO).is_none());
        for _ in 0..3 {
            recepteur.pousser(Arc::new(ImageI420::noire(4, 4)));
        }
        assert_eq!(recepteur.images_recues(), 3);
        let (generation, image) = recepteur.attendre(0, Duration::ZERO).unwrap();
        assert_eq!(generation, 3);
        assert!(image.is_some());
        recepteur.effacer();
        let (_, image) = recepteur.attendre(generation, Duration::ZERO).unwrap();
        assert!(image.is_none());
        let attente = {
            let r = recepteur.clone();
            std::thread::spawn(move || r.attendre(r.generation(), Duration::from_secs(30)))
        };
        recepteur.fermer();
        assert!(attente.join().unwrap().is_none());
    }

    #[test]
    fn les_videos_partagent_un_recepteur_par_cle() {
        let videos = Videos::nouvelles();
        let cle = CleVideo::camera("u1");
        let a = videos.recepteur(&cle);
        a.pousser(Arc::new(ImageI420::noire(2, 2)));
        assert!(videos.recepteur(&cle).derniere().is_some());
        assert_eq!(cle.texte(), "u1#camera");
        videos.effacer_tout();
        assert!(a.derniere().is_none());
        assert_eq!(videos.images_recues(), 1);
    }
}
