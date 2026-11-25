use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::apercu_liens::{cle_de_cache, TAILLE_DU_CACHE};
use estompe_client::appels::{resoudre_emoji, EmojiPersonnalise};
use estompe_client::cache::CacheDisque;
use estompe_client::http::{Client, ErreurApi};
use estompe_client::lru::Lru;
use estompe_client::types::LinkPreview;
use image::{AnimationDecoder, ImageFormat};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use tokio::sync::Semaphore;

use crate::entree::Futur;

pub type Image = SharedPixelBuffer<Rgba8Pixel>;
pub type Progres = Box<dyn FnMut(u64, Option<u64>) + Send>;

pub const DOSSIER_MEDIAS: &str = "medias";
pub const BORNE_DISQUE_MEDIAS: u64 = 256 * 1024 * 1024;
pub const PLAFOND_MEMOIRE_IMAGES: usize = 96 * 1024 * 1024;
pub const CAPACITE_DIMENSIONS: usize = 4096;
pub const DECODAGES_SIMULTANES: usize = 2;
pub const COTE_DECODABLE_MAX: u32 = 16_384;
pub const ALLOCATION_DECODAGE_MAX: u64 = 512 * 1024 * 1024;
pub const IMAGES_ANIMEES_MAX: usize = 240;
pub const DELAI_IMAGE_MIN: Duration = Duration::from_millis(20);
pub const DELAI_IMAGE_PAR_DEFAUT: Duration = Duration::from_millis(100);
pub const TAILLE_EXTERNE_MAX: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    Fichier(String),
    Affiche(String),
    Emoji(String),
    Externe(String),
}

impl Source {
    pub fn nom_de_cache(&self) -> String {
        match self {
            Self::Fichier(id) => format!("f.{}", nettoyer(id)),
            Self::Affiche(id) => format!("a.{}", nettoyer(id)),
            Self::Emoji(id) => format!("e.{}", nettoyer(id)),
            Self::Externe(url) => {
                let mut empreinte = std::collections::hash_map::DefaultHasher::new();
                url.hash(&mut empreinte);
                format!("x.{:016x}.{}", empreinte.finish(), url.len())
            }
        }
    }
}

fn nettoyer(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    Vignette,
    Affiche,
    Visionneuse,
    Emoji,
    Carte,
    Miniature,
    Piece,
    Gif,
}

impl Format {
    pub fn boite(self) -> (u32, u32) {
        match self {
            Self::Vignette => (360, 300),
            Self::Affiche => (360, 203),
            Self::Visionneuse => (2048, 2048),
            Self::Emoji => (96, 96),
            Self::Carte => (168, 168),
            Self::Miniature => (520, 293),
            Self::Piece => (128, 128),
            Self::Gif => (200, 200),
        }
    }

    pub fn anime(self) -> bool {
        matches!(self, Self::Vignette | Self::Emoji | Self::Gif)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CleImage {
    pub source: Source,
    pub format: Format,
}

#[derive(Clone)]
pub struct Animation {
    pub images: Vec<(Image, Duration)>,
}

#[derive(Clone)]
pub enum Decodee {
    Fixe(Image),
    Animee(Arc<Animation>),
}

impl Decodee {
    pub fn octets(&self) -> usize {
        let taille = |i: &Image| i.width() as usize * i.height() as usize * 4;
        match self {
            Self::Fixe(i) => taille(i),
            Self::Animee(a) => a.images.iter().map(|(i, _)| taille(i)).sum(),
        }
    }

    pub fn premiere(&self) -> &Image {
        match self {
            Self::Fixe(i) => i,
            Self::Animee(a) => &a.images[0].0,
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        let i = self.premiere();
        (i.width(), i.height())
    }
}

pub fn ajuster(largeur: u32, hauteur: u32, boite: (u32, u32)) -> (u32, u32) {
    if largeur == 0 || hauteur == 0 {
        return (0, 0);
    }
    let k = (f64::from(boite.0) / f64::from(largeur))
        .min(f64::from(boite.1) / f64::from(hauteur))
        .min(1.0);
    (
        ((f64::from(largeur) * k).round() as u32).max(1),
        ((f64::from(hauteur) * k).round() as u32).max(1),
    )
}

fn tampon(image: image::RgbaImage) -> Image {
    SharedPixelBuffer::clone_from_slice(image.as_raw(), image.width(), image.height())
}

fn recadrer(image: image::DynamicImage, rapport: (u32, u32)) -> image::DynamicImage {
    let (l, h) = (image.width(), image.height());
    let (rl, rh) = (u64::from(rapport.0), u64::from(rapport.1));
    if u64::from(l) * rh > u64::from(h) * rl {
        let nl = ((u64::from(h) * rl / rh) as u32).max(1);
        image.crop_imm((l - nl) / 2, 0, nl, h)
    } else {
        let nh = ((u64::from(l) * rh / rl) as u32).max(1);
        image.crop_imm(0, (h - nh) / 2, l, nh)
    }
}

pub fn arrondir_coins(image: &mut image::RgbaImage, rayon: f32) {
    let (largeur, hauteur) = (image.width() as f32, image.height() as f32);
    let rayon = rayon.min(largeur / 2.0).min(hauteur / 2.0);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
        let dx = (rayon - x).max(x - (largeur - rayon)).max(0.0);
        let dy = (rayon - y).max(y - (hauteur - rayon)).max(0.0);
        if dx == 0.0 || dy == 0.0 {
            continue;
        }
        let couverture = (rayon + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
        pixel.0[3] = (f32::from(pixel.0[3]) * couverture).round() as u8;
    }
}

impl Format {
    fn rapport(self) -> Option<(u32, u32)> {
        match self {
            Self::Carte | Self::Piece => Some((1, 1)),
            Self::Miniature | Self::Affiche => Some((16, 9)),
            _ => None,
        }
    }

    fn rayon(self) -> f32 {
        match self {
            Self::Vignette | Self::Miniature | Self::Affiche => 10.0,
            Self::Carte | Self::Piece | Self::Gif => 8.0,
            Self::Emoji | Self::Visionneuse => 0.0,
        }
    }
}

pub fn reduire(image: image::DynamicImage, format: Format) -> image::RgbaImage {
    let image = match format.rapport() {
        Some(rapport) => recadrer(image, rapport),
        None => image,
    };
    let (l, h) = ajuster(image.width(), image.height(), format.boite());
    let mut sortie = if (l, h) == (image.width(), image.height()) {
        image.into_rgba8()
    } else {
        image.thumbnail_exact(l, h).into_rgba8()
    };
    if format.rayon() > 0.0 {
        arrondir_coins(&mut sortie, format.rayon());
    }
    sortie
}

fn delai(d: image::Delay) -> Duration {
    let (n, dd) = d.numer_denom_ms();
    let ms = n.checked_div(dd).unwrap_or(0);
    let duree = Duration::from_millis(u64::from(ms));
    if duree < DELAI_IMAGE_MIN {
        DELAI_IMAGE_PAR_DEFAUT
    } else {
        duree
    }
}

fn images_animees<'a>(
    images: impl Iterator<Item = image::ImageResult<image::Frame>> + 'a,
    format: Format,
    plafond: usize,
) -> Result<Vec<(Image, Duration)>, String> {
    let mut sortie = Vec::new();
    let mut octets = 0usize;
    for image in images.take(IMAGES_ANIMEES_MAX) {
        let image = image.map_err(|e| e.to_string())?;
        let d = delai(image.delay());
        let reduite = reduire(image::DynamicImage::ImageRgba8(image.into_buffer()), format);
        octets += reduite.as_raw().len();
        if octets > plafond && !sortie.is_empty() {
            break;
        }
        sortie.push((tampon(reduite), d));
    }
    if sortie.is_empty() {
        return Err("animation vide".into());
    }
    Ok(sortie)
}

fn lire_original(octets: &[u8], format: ImageFormat) -> Result<image::DynamicImage, String> {
    let mut lecteur = image::ImageReader::with_format(Cursor::new(octets), format);
    let mut limites = image::Limits::default();
    limites.max_image_width = Some(COTE_DECODABLE_MAX);
    limites.max_image_height = Some(COTE_DECODABLE_MAX);
    limites.max_alloc = Some(ALLOCATION_DECODAGE_MAX);
    lecteur.limits(limites);
    lecteur.decode().map_err(|e| e.to_string())
}

pub fn decoder_original(octets: &[u8]) -> Result<image::RgbaImage, String> {
    let format = image::guess_format(octets).map_err(|e| e.to_string())?;
    lire_original(octets, format).map(image::DynamicImage::into_rgba8)
}

pub fn decoder(
    octets: &[u8],
    cible: Format,
    plafond: usize,
) -> Result<(Decodee, (u32, u32)), String> {
    let animer = cible.anime();
    let format = image::guess_format(octets).map_err(|e| e.to_string())?;
    if animer && format == ImageFormat::Gif {
        let decodeur =
            image::codecs::gif::GifDecoder::new(Cursor::new(octets)).map_err(|e| e.to_string())?;
        let origine = image::ImageDecoder::dimensions(&decodeur);
        let images = images_animees(decodeur.into_frames(), cible, plafond)?;
        return Ok(finir(images, origine));
    }
    if animer && format == ImageFormat::WebP {
        let decodeur = image::codecs::webp::WebPDecoder::new(Cursor::new(octets))
            .map_err(|e| e.to_string())?;
        if decodeur.has_animation() {
            let origine = image::ImageDecoder::dimensions(&decodeur);
            let images = images_animees(decodeur.into_frames(), cible, plafond)?;
            return Ok(finir(images, origine));
        }
    }
    let image = lire_original(octets, format)?;
    let origine = (image.width(), image.height());
    Ok((Decodee::Fixe(tampon(reduire(image, cible))), origine))
}

fn finir(mut images: Vec<(Image, Duration)>, origine: (u32, u32)) -> (Decodee, (u32, u32)) {
    if images.len() == 1 {
        let (image, _) = images.remove(0);
        return (Decodee::Fixe(image), origine);
    }
    (Decodee::Animee(Arc::new(Animation { images })), origine)
}

pub trait SourceMedias: Send + Sync + 'static {
    fn octets(&self, source: &Source) -> Futur<Result<Vec<u8>, ErreurApi>>;
    fn emojis(&self) -> Futur<Result<Vec<EmojiPersonnalise>, ErreurApi>>;
    fn apercu(
        &self,
        url: &str,
        message: Option<&str>,
    ) -> Futur<Result<Option<LinkPreview>, ErreurApi>>;
    fn telecharger(
        &self,
        fichier: &str,
        destination: PathBuf,
        progres: Progres,
    ) -> Futur<Result<u64, ErreurApi>>;
}

impl SourceMedias for Client {
    fn octets(&self, source: &Source) -> Futur<Result<Vec<u8>, ErreurApi>> {
        let client = self.clone();
        let source = source.clone();
        Box::pin(async move {
            match &source {
                Source::Fichier(id) => client.contenu_de_fichier(id).await,
                Source::Affiche(id) => client.miniature_de_fichier(id).await,
                Source::Emoji(id) => client.image_d_emoji(id).await,
                Source::Externe(url) => client.octets_externes(url, TAILLE_EXTERNE_MAX).await,
            }
            .map(|o| o.to_vec())
        })
    }

    fn emojis(&self) -> Futur<Result<Vec<EmojiPersonnalise>, ErreurApi>> {
        let client = self.clone();
        Box::pin(async move { client.emojis_personnalises().await })
    }

    fn apercu(
        &self,
        url: &str,
        message: Option<&str>,
    ) -> Futur<Result<Option<LinkPreview>, ErreurApi>> {
        let client = self.clone();
        let url = url.to_string();
        let message = message.map(str::to_string);
        Box::pin(async move { client.apercu_de_lien(&url, message.as_deref()).await })
    }

    fn telecharger(
        &self,
        fichier: &str,
        destination: PathBuf,
        mut progres: Progres,
    ) -> Futur<Result<u64, ErreurApi>> {
        let client = self.clone();
        let chemin = format!(
            "/files/{}?download=1",
            estompe_client::http::encoder_composant(fichier)
        );
        Box::pin(async move {
            client
                .telecharger(&chemin, &destination, &mut progres)
                .await
        })
    }
}

struct MemoireImages {
    images: Lru<CleImage, Decodee>,
    octets: usize,
    plafond: usize,
}

impl MemoireImages {
    fn inserer(&mut self, cle: CleImage, image: Decodee) {
        let taille = image.octets();
        if taille > self.plafond {
            return;
        }
        if let Some(ancienne) = self.images.retirer(&cle) {
            self.octets -= ancienne.octets();
        }
        self.octets += taille;
        self.images.inserer(cle, image);
        while self.octets > self.plafond {
            match self.images.retirer_le_plus_ancien() {
                Some((_, ancienne)) => self.octets -= ancienne.octets(),
                None => break,
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatistiquesMedias {
    pub decodages: usize,
    pub decodages_hors_interface: usize,
    pub telechargements: usize,
}

pub struct Medias {
    source: Arc<dyn SourceMedias>,
    disque: Option<CacheDisque>,
    memoire: Mutex<MemoireImages>,
    dimensions: Mutex<Lru<Source, (u32, u32)>>,
    echecs: Mutex<HashSet<CleImage>>,
    emojis: Mutex<Option<Vec<EmojiPersonnalise>>>,
    apercus: Mutex<Lru<String, Option<LinkPreview>>>,
    decodages: Semaphore,
    fil_interface: Mutex<Option<std::thread::ThreadId>>,
    compteurs: [AtomicUsize; 3],
}

fn verrou<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Medias {
    pub fn nouveaux(source: Arc<dyn SourceMedias>, disque: Option<CacheDisque>) -> Arc<Self> {
        Self::avec_plafond(source, disque, PLAFOND_MEMOIRE_IMAGES)
    }

    pub fn avec_plafond(
        source: Arc<dyn SourceMedias>,
        disque: Option<CacheDisque>,
        plafond: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            source,
            disque,
            memoire: Mutex::new(MemoireImages {
                images: Lru::nouveau(usize::MAX),
                octets: 0,
                plafond,
            }),
            dimensions: Mutex::new(Lru::nouveau(CAPACITE_DIMENSIONS)),
            echecs: Mutex::new(HashSet::new()),
            emojis: Mutex::new(None),
            apercus: Mutex::new(Lru::nouveau(TAILLE_DU_CACHE)),
            decodages: Semaphore::new(DECODAGES_SIMULTANES),
            fil_interface: Mutex::new(None),
            compteurs: Default::default(),
        })
    }

    pub fn source(&self) -> &Arc<dyn SourceMedias> {
        &self.source
    }

    pub fn noter_fil_interface(&self) {
        *verrou(&self.fil_interface) = Some(std::thread::current().id());
    }

    pub fn statistiques(&self) -> StatistiquesMedias {
        StatistiquesMedias {
            decodages: self.compteurs[0].load(Ordering::SeqCst),
            decodages_hors_interface: self.compteurs[1].load(Ordering::SeqCst),
            telechargements: self.compteurs[2].load(Ordering::SeqCst),
        }
    }

    pub fn octets_en_memoire(&self) -> usize {
        verrou(&self.memoire).octets
    }

    pub fn plafond(&self) -> usize {
        verrou(&self.memoire).plafond
    }

    pub fn image(&self, cle: &CleImage) -> Option<Decodee> {
        verrou(&self.memoire).images.lire(cle).cloned()
    }

    pub fn en_echec(&self, cle: &CleImage) -> bool {
        verrou(&self.echecs).contains(cle)
    }

    pub fn dimensions(&self, source: &Source) -> Option<(u32, u32)> {
        verrou(&self.dimensions).lire(source).copied()
    }

    pub fn installer_image(&self, cle: CleImage, image: Decodee, origine: (u32, u32)) {
        verrou(&self.dimensions).inserer(cle.source.clone(), origine);
        let mut memoire = verrou(&self.memoire);
        if image.octets() > memoire.plafond {
            drop(memoire);
            verrou(&self.echecs).insert(cle);
            return;
        }
        memoire.inserer(cle, image);
    }

    pub async fn octets_de(&self, source: &Source) -> Option<Vec<u8>> {
        let nom = source.nom_de_cache();
        if let Some(octets) = self.disque.as_ref().and_then(|d| d.lire(&nom)) {
            return Some(octets);
        }
        let octets = self.source.octets(source).await.ok()?;
        self.compteurs[2].fetch_add(1, Ordering::SeqCst);
        if let Some(disque) = &self.disque {
            let _ = disque.ecrire(&nom, &octets);
        }
        Some(octets)
    }

    pub async fn charger_image(&self, cle: CleImage) -> Option<Decodee> {
        if let Some(image) = self.image(&cle) {
            return Some(image);
        }
        let octets = match self.octets_de(&cle.source).await {
            Some(octets) => octets,
            None => {
                verrou(&self.echecs).insert(cle);
                return None;
            }
        };
        let _jeton = self.decodages.acquire().await.ok()?;
        let plafond = self.plafond() / 2;
        let interface = *verrou(&self.fil_interface);
        let format = cle.format;
        let resultat = tokio::task::spawn_blocking(move || {
            let hors_interface = Some(std::thread::current().id()) != interface;
            (decoder(&octets, format, plafond), hors_interface)
        })
        .await
        .ok();
        let Some((decodage, hors_interface)) = resultat else {
            verrou(&self.echecs).insert(cle);
            return None;
        };
        self.compteurs[0].fetch_add(1, Ordering::SeqCst);
        if hors_interface {
            self.compteurs[1].fetch_add(1, Ordering::SeqCst);
        }
        match decodage {
            Ok((image, origine)) => {
                self.installer_image(cle, image.clone(), origine);
                Some(image)
            }
            Err(_) => {
                if let Some(disque) = &self.disque {
                    disque.retirer(&cle.source.nom_de_cache());
                }
                verrou(&self.echecs).insert(cle);
                None
            }
        }
    }

    pub fn emojis(&self) -> Option<Vec<EmojiPersonnalise>> {
        verrou(&self.emojis).clone()
    }

    pub fn installer_emojis(&self, emojis: Vec<EmojiPersonnalise>) {
        *verrou(&self.emojis) = Some(emojis);
    }

    pub fn emoji(&self, nom: &str, serveur: Option<&str>) -> Option<EmojiPersonnalise> {
        let emojis = verrou(&self.emojis);
        resoudre_emoji(emojis.as_deref()?, nom, serveur).cloned()
    }

    pub async fn charger_emojis(&self) -> bool {
        match self.source.emojis().await {
            Ok(emojis) => {
                let change = self.emojis().as_ref() != Some(&emojis);
                self.installer_emojis(emojis);
                change
            }
            Err(_) => false,
        }
    }

    pub fn apercu(&self, url: &str, message: Option<&str>) -> Option<Option<LinkPreview>> {
        verrou(&self.apercus)
            .lire(&cle_de_cache(message, url))
            .cloned()
    }

    pub fn installer_apercu(&self, url: &str, message: Option<&str>, apercu: Option<LinkPreview>) {
        verrou(&self.apercus).inserer(cle_de_cache(message, url), apercu);
    }

    pub async fn charger_apercu(&self, url: &str, message: Option<&str>) -> Option<LinkPreview> {
        if let Some(apercu) = self.apercu(url, message) {
            return apercu;
        }
        let apercu = self.source.apercu(url, message).await.ok().flatten();
        self.installer_apercu(url, message, apercu.clone());
        apercu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    pub fn png(largeur: u32, hauteur: u32) -> Vec<u8> {
        let image = image::GrayImage::from_pixel(largeur, hauteur, image::Luma([180]));
        let mut octets = Cursor::new(Vec::new());
        image.write_to(&mut octets, ImageFormat::Png).unwrap();
        octets.into_inner()
    }

    pub fn gif(images: usize) -> Vec<u8> {
        let mut octets = Vec::new();
        {
            let mut encodeur = image::codecs::gif::GifEncoder::new(&mut octets);
            for i in 0..images {
                let couleur = image::Rgba([(i * 60) as u8, 100, 200, 255]);
                let image = image::RgbaImage::from_pixel(40, 30, couleur);
                encodeur
                    .encode_frame(image::Frame::from_parts(
                        image,
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(80, 1),
                    ))
                    .unwrap();
            }
        }
        octets
    }

    #[derive(Default)]
    struct SourceEssai {
        requetes: AtomicUsize,
        contenu: Mutex<Vec<u8>>,
    }

    impl SourceMedias for SourceEssai {
        fn octets(&self, _: &Source) -> Futur<Result<Vec<u8>, ErreurApi>> {
            self.requetes.fetch_add(1, Ordering::SeqCst);
            let contenu = verrou(&self.contenu).clone();
            Box::pin(async move { Ok(contenu) })
        }
        fn emojis(&self) -> Futur<Result<Vec<EmojiPersonnalise>, ErreurApi>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn apercu(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Futur<Result<Option<LinkPreview>, ErreurApi>> {
            self.requetes.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        }
        fn telecharger(&self, _: &str, _: PathBuf, _: Progres) -> Futur<Result<u64, ErreurApi>> {
            Box::pin(async { Ok(0) })
        }
    }

    fn cle(id: &str, format: Format) -> CleImage {
        CleImage {
            source: Source::Fichier(id.into()),
            format,
        }
    }

    #[test]
    fn l_image_est_reduite_a_sa_boite_en_gardant_ses_proportions() {
        assert_eq!(ajuster(4000, 2000, (360, 300)), (360, 180));
        assert_eq!(ajuster(1000, 3000, (360, 300)), (100, 300));
        assert_eq!(ajuster(100, 50, (360, 300)), (100, 50));
        assert_eq!(ajuster(0, 50, (360, 300)), (0, 0));
        let (image, origine) = decoder(&png(1200, 600), Format::Vignette, usize::MAX).unwrap();
        assert_eq!(origine, (1200, 600));
        assert_eq!(image.dimensions(), (360, 180));
        let coins = image.premiere().as_slice();
        assert_eq!(coins[0].a, 0, "coin arrondi");
        assert_eq!(coins[360 * 90 + 180].a, 255);
        let (carte, _) = decoder(&png(1200, 600), Format::Carte, usize::MAX).unwrap();
        assert_eq!(carte.dimensions(), (168, 168));
        let (miniature, _) = decoder(&png(1200, 1200), Format::Miniature, usize::MAX).unwrap();
        assert_eq!(miniature.dimensions(), (520, 293));
        assert!(decoder(b"pas une image", Format::Vignette, usize::MAX).is_err());
        assert_eq!(
            decoder_original(&png(1200, 600)).unwrap().dimensions(),
            (1200, 600)
        );
    }

    #[test]
    fn un_gif_anime_garde_ses_images_et_leurs_delais() {
        let (anime, origine) = decoder(&gif(4), Format::Vignette, usize::MAX).unwrap();
        assert_eq!(origine, (40, 30));
        let Decodee::Animee(a) = anime else {
            panic!("animation attendue");
        };
        assert_eq!(a.images.len(), 4);
        assert!(a
            .images
            .iter()
            .all(|(_, d)| *d == Duration::from_millis(80)));
        let (fixe, _) = decoder(&gif(4), Format::Visionneuse, usize::MAX).unwrap();
        assert!(matches!(fixe, Decodee::Fixe(_)));
        let (seule, _) = decoder(&gif(1), Format::Vignette, usize::MAX).unwrap();
        assert!(matches!(seule, Decodee::Fixe(_)));
        let (borne, _) = decoder(&gif(4), Format::Vignette, 40 * 30 * 4 * 2).unwrap();
        let Decodee::Animee(b) = borne else {
            panic!("animation attendue");
        };
        assert_eq!(b.images.len(), 2);
    }

    #[test]
    fn une_image_demesuree_est_refusee_au_lieu_d_etre_allouee() {
        let erreur = decoder(
            &png(COTE_DECODABLE_MAX + 1, 1),
            Format::Vignette,
            usize::MAX,
        );
        assert!(erreur.is_err());
    }

    #[test]
    fn le_nom_de_cache_est_sur_pour_toute_source() {
        assert_eq!(Source::Fichier("a/../b".into()).nom_de_cache(), "f.a____b");
        assert_eq!(Source::Affiche("v1".into()).nom_de_cache(), "a.v1");
        assert_eq!(Source::Emoji("e1".into()).nom_de_cache(), "e.e1");
        let externe = Source::Externe("https://x.fr/a.png?b=1".into()).nom_de_cache();
        assert!(estompe_client::cache::cle_valide(&externe), "{externe}");
        assert_ne!(
            externe,
            Source::Externe("https://x.fr/a.png?b=2".into()).nom_de_cache()
        );
    }

    #[tokio::test]
    async fn c2_une_image_de_8000_pixels_est_decodee_hors_de_l_interface_et_reste_sous_le_plafond()
    {
        let source = Arc::new(SourceEssai::default());
        *verrou(&source.contenu) = png(8000, 8000);
        let dossier = tempfile::tempdir().unwrap();
        let plafond = 4 * 1024 * 1024;
        let medias = Medias::avec_plafond(
            source.clone(),
            Some(CacheDisque::ouvrir(dossier.path(), BORNE_DISQUE_MEDIAS).unwrap()),
            plafond,
        );
        medias.noter_fil_interface();
        let debut = std::time::Instant::now();
        let image = medias
            .charger_image(cle("geante", Format::Vignette))
            .await
            .unwrap();
        let duree = debut.elapsed();
        assert_eq!(image.dimensions(), (300, 300));
        assert_eq!(
            medias.dimensions(&Source::Fichier("geante".into())),
            Some((8000, 8000))
        );
        let stats = medias.statistiques();
        assert_eq!((stats.decodages, stats.decodages_hors_interface), (1, 1));
        assert!(medias.octets_en_memoire() <= plafond);
        for i in 0..20 {
            *verrou(&source.contenu) = png(1000 + i, 1000);
            medias
                .charger_image(cle(&format!("i{i}"), Format::Vignette))
                .await
                .unwrap();
            assert!(medias.octets_en_memoire() <= plafond);
        }
        assert!(medias.image(&cle("geante", Format::Vignette)).is_none());
        println!(
            "303 C2 : 8000 × 8000 décodée en {:.0} ms hors du fil d'interface, vignette 300 × 300, mémoire {} o pour un plafond de {plafond} o",
            duree.as_secs_f64() * 1000.0,
            medias.octets_en_memoire()
        );
        let requetes = source.requetes.load(Ordering::SeqCst);
        medias
            .charger_image(cle("geante", Format::Vignette))
            .await
            .unwrap();
        assert_eq!(
            source.requetes.load(Ordering::SeqCst),
            requetes,
            "relue sur le disque"
        );
    }

    #[tokio::test]
    async fn un_echec_est_retenu_et_un_apercu_n_est_demande_qu_une_fois() {
        let source = Arc::new(SourceEssai::default());
        *verrou(&source.contenu) = b"pas une image".to_vec();
        let medias = Medias::nouveaux(source.clone(), None);
        assert!(medias
            .charger_image(cle("x", Format::Vignette))
            .await
            .is_none());
        assert!(medias.en_echec(&cle("x", Format::Vignette)));
        assert!(medias
            .charger_apercu("https://a.fr", Some("1"))
            .await
            .is_none());
        assert!(medias
            .charger_apercu("https://a.fr", Some("1"))
            .await
            .is_none());
        assert_eq!(medias.apercu("https://a.fr", Some("1")), Some(None));
        assert_eq!(source.requetes.load(Ordering::SeqCst), 2);
    }
}
