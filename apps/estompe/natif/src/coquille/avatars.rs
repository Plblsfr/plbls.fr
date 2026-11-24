use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::appels::Badge;
use estompe_client::cache::{cle_valide, CacheDisque};
use estompe_client::http::{Client, ErreurApi};
use estompe_client::lru::Lru;
use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::entree::Futur;

pub type Image = SharedPixelBuffer<Rgba8Pixel>;

pub const COTE_AVATAR: u32 = 96;
pub const BORNE_DISQUE: u64 = 64 * 1024 * 1024;
pub const CAPACITE_AVATARS: usize = 256;
pub const CAPACITE_BADGES: usize = 1024;
pub const CAPACITE_ICONES_BADGES: usize = 256;
pub const COTE_ICONE_BADGE: u32 = 64;
pub const DOSSIER_AVATARS: &str = "avatars";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CleAvatar {
    pub utilisateur: String,
    pub version: i64,
    pub serveur: Option<String>,
}

impl CleAvatar {
    pub fn nom_de_fichier(&self) -> Option<String> {
        let nom = match &self.serveur {
            Some(serveur) => format!("s.{serveur}.u.{}.v{}", self.utilisateur, self.version),
            None => format!("u.{}.v{}", self.utilisateur, self.version),
        };
        cle_valide(&nom).then_some(nom)
    }
}

pub trait SourceProfils: Send + Sync + 'static {
    fn avatar(&self, cle: &CleAvatar) -> Futur<Result<Vec<u8>, ErreurApi>>;
    fn badges(&self, utilisateur: &str) -> Futur<Result<Vec<Badge>, ErreurApi>>;
    fn icone_badge(&self, _badge: &str) -> Futur<Result<Vec<u8>, ErreurApi>> {
        Box::pin(async { Err(ErreurApi::DelaiDepasse) })
    }
}

impl SourceProfils for Client {
    fn avatar(&self, cle: &CleAvatar) -> Futur<Result<Vec<u8>, ErreurApi>> {
        let client = self.clone();
        let cle = cle.clone();
        Box::pin(async move {
            client
                .avatar(&cle.utilisateur, cle.serveur.as_deref())
                .await
                .map(|octets| octets.to_vec())
        })
    }

    fn badges(&self, utilisateur: &str) -> Futur<Result<Vec<Badge>, ErreurApi>> {
        let client = self.clone();
        let utilisateur = utilisateur.to_string();
        Box::pin(async move { client.badges_de(&utilisateur).await })
    }

    fn icone_badge(&self, badge: &str) -> Futur<Result<Vec<u8>, ErreurApi>> {
        let client = self.clone();
        let badge = badge.to_string();
        Box::pin(async move {
            client
                .icone_de_badge(&badge)
                .await
                .map(|octets| octets.to_vec())
        })
    }
}

pub fn est_un_svg(octets: &[u8]) -> bool {
    let debut = &octets[..octets.len().min(512)];
    let texte = String::from_utf8_lossy(debut);
    let texte = texte.trim_start_matches('\u{feff}').trim_start();
    texte.starts_with("<svg") || (texte.starts_with("<?xml") && texte.contains("<svg"))
}

pub fn decoder_icone_badge(octets: &[u8]) -> Option<slint::Image> {
    if octets.is_empty() {
        return None;
    }
    if est_un_svg(octets) {
        return slint::Image::load_from_svg_data(octets).ok();
    }
    decoder_image(octets, COTE_ICONE_BADGE).map(slint::Image::from_rgba8)
}

thread_local! {
    static IMAGES_BADGES: RefCell<Lru<(String, i64), Option<slint::Image>>> =
        RefCell::new(Lru::nouveau(CAPACITE_ICONES_BADGES));
}

pub fn image_de_badge(profils: &Profils, badge: &Badge) -> Option<slint::Image> {
    if badge.icon_version > 0 {
        let cle = (badge.id.clone(), badge.icon_version);
        let image = IMAGES_BADGES.with(|images| {
            let mut images = images.borrow_mut();
            if let Some(image) = images.lire(&cle) {
                return Some(image.clone());
            }
            let octets = profils.icone_badge(&badge.id, badge.icon_version)?;
            let image = decoder_icone_badge(&octets);
            images.inserer(cle, image.clone());
            Some(image)
        });
        if let Some(Some(image)) = image {
            return Some(image);
        }
    }
    let emoji = badge.icon.trim();
    if emoji.is_empty() {
        return None;
    }
    crate::messagerie::emoji::image(emoji)
}

pub fn decoder_image(octets: &[u8], cote: u32) -> Option<Image> {
    let image = image::load_from_memory(octets).ok()?;
    let image = if image.width() > cote || image.height() > cote {
        image.thumbnail(cote, cote)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    ))
}

pub fn arrondir(mut image: Image) -> Image {
    let colonnes = (image.width() as usize).max(1);
    let (largeur, hauteur) = (image.width() as f32, image.height() as f32);
    let rayon = largeur.min(hauteur) * 0.3;
    for (i, pixel) in image.make_mut_slice().iter_mut().enumerate() {
        let x = (i % colonnes) as f32 + 0.5;
        let y = (i / colonnes) as f32 + 0.5;
        let dx = (rayon - x).max(x - (largeur - rayon)).max(0.0);
        let dy = (rayon - y).max(y - (hauteur - rayon)).max(0.0);
        if dx == 0.0 || dy == 0.0 {
            continue;
        }
        let couverture = (rayon + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
        pixel.a = (f32::from(pixel.a) * couverture).round() as u8;
    }
    image
}

pub fn decoder_avatar(octets: &[u8]) -> Option<Image> {
    let image = image::load_from_memory(octets).ok()?;
    let cote = image.width().min(image.height());
    let carre = image.crop_imm(
        (image.width() - cote) / 2,
        (image.height() - cote) / 2,
        cote,
        cote,
    );
    let reduit = if cote > COTE_AVATAR {
        carre.thumbnail(COTE_AVATAR, COTE_AVATAR)
    } else {
        carre
    };
    let rgba = reduit.into_rgba8();
    Some(arrondir(SharedPixelBuffer::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    )))
}

type IconesBadges = Lru<(String, i64), Arc<Vec<u8>>>;

pub struct Profils {
    source: Arc<dyn SourceProfils>,
    disque: Option<CacheDisque>,
    avatars: Mutex<Lru<CleAvatar, Image>>,
    badges: Mutex<Lru<String, Vec<Badge>>>,
    icones_badges: Mutex<IconesBadges>,
}

fn verrou<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Profils {
    pub fn nouveaux(source: Arc<dyn SourceProfils>, disque: Option<CacheDisque>) -> Arc<Self> {
        Self::avec_capacites(source, disque, CAPACITE_AVATARS, CAPACITE_BADGES)
    }

    pub fn avec_capacites(
        source: Arc<dyn SourceProfils>,
        disque: Option<CacheDisque>,
        avatars: usize,
        badges: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            source,
            disque,
            avatars: Mutex::new(Lru::nouveau(avatars)),
            badges: Mutex::new(Lru::nouveau(badges)),
            icones_badges: Mutex::new(Lru::nouveau(CAPACITE_ICONES_BADGES)),
        })
    }

    pub fn disque(&self) -> Option<&CacheDisque> {
        self.disque.as_ref()
    }

    pub fn avatar(&self, cle: &CleAvatar) -> Option<Image> {
        verrou(&self.avatars).lire(cle).cloned()
    }

    pub fn avatars_en_memoire(&self) -> usize {
        verrou(&self.avatars).len()
    }

    pub fn installer_avatar(&self, cle: CleAvatar, image: Image) {
        verrou(&self.avatars).inserer(cle, image);
    }

    pub async fn charger_avatar(&self, cle: CleAvatar) -> Option<Image> {
        if let Some(image) = self.avatar(&cle) {
            return Some(image);
        }
        let nom = cle.nom_de_fichier();
        if let (Some(disque), Some(nom)) = (&self.disque, &nom) {
            if let Some(octets) = disque.lire(nom) {
                match decoder_avatar(&octets) {
                    Some(image) => {
                        self.installer_avatar(cle, image.clone());
                        return Some(image);
                    }
                    None => disque.retirer(nom),
                }
            }
        }
        let octets = self.source.avatar(&cle).await.ok()?;
        let image = decoder_avatar(&octets)?;
        if let (Some(disque), Some(nom)) = (&self.disque, &nom) {
            let _ = disque.ecrire(nom, &octets);
        }
        self.installer_avatar(cle, image.clone());
        Some(image)
    }

    pub fn badges(&self, utilisateur: &str) -> Option<Vec<Badge>> {
        verrou(&self.badges).lire(&utilisateur.to_string()).cloned()
    }

    pub fn installer_badges(&self, utilisateur: &str, badges: Vec<Badge>) {
        verrou(&self.badges).inserer(utilisateur.to_string(), badges);
    }

    pub async fn charger_badges(&self, utilisateur: String) -> Vec<Badge> {
        if let Some(badges) = self.badges(&utilisateur) {
            return badges;
        }
        let mut badges = self.source.badges(&utilisateur).await.unwrap_or_default();
        badges.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));
        for badge in badges.iter().filter(|b| b.icon_version > 0) {
            if self.icone_badge(&badge.id, badge.icon_version).is_none() {
                let octets = self.source.icone_badge(&badge.id).await.unwrap_or_default();
                self.installer_icone_badge(&badge.id, badge.icon_version, octets);
            }
        }
        self.installer_badges(&utilisateur, badges.clone());
        badges
    }

    pub fn icone_badge(&self, badge: &str, version: i64) -> Option<Arc<Vec<u8>>> {
        verrou(&self.icones_badges)
            .lire(&(badge.to_string(), version))
            .cloned()
    }

    pub fn installer_icone_badge(&self, badge: &str, version: i64, octets: Vec<u8>) {
        verrou(&self.icones_badges).inserer((badge.to_string(), version), Arc::new(octets));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn png(cote: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(cote, cote, image::Rgba([208, 104, 138, 255]));
        let mut octets = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        octets.into_inner()
    }

    #[derive(Default)]
    struct Source {
        avatars: AtomicUsize,
        badges: AtomicUsize,
        icones: AtomicUsize,
    }

    impl SourceProfils for Source {
        fn avatar(&self, cle: &CleAvatar) -> Futur<Result<Vec<u8>, ErreurApi>> {
            self.avatars.fetch_add(1, Ordering::SeqCst);
            let resultat = if cle.utilisateur == "sans" {
                Err(ErreurApi::Reseau("introuvable".into()))
            } else {
                Ok(png(300))
            };
            Box::pin(async move { resultat })
        }

        fn badges(&self, utilisateur: &str) -> Futur<Result<Vec<Badge>, ErreurApi>> {
            self.badges.fetch_add(1, Ordering::SeqCst);
            let badges = if utilisateur == "carol" {
                Ok(vec![
                    Badge {
                        id: "b2".into(),
                        name: "Zèbre".into(),
                        sort_order: 1,
                        icon_version: 1,
                        ..Badge::default()
                    },
                    Badge {
                        id: "b1".into(),
                        name: "Abeille".into(),
                        sort_order: 1,
                        icon_version: 2,
                        ..Badge::default()
                    },
                    Badge {
                        id: "b0".into(),
                        name: "Premier".into(),
                        sort_order: 0,
                        icon: "🏅".into(),
                        ..Badge::default()
                    },
                    Badge {
                        id: "b3".into(),
                        name: "Perdu".into(),
                        sort_order: 2,
                        icon_version: 4,
                        ..Badge::default()
                    },
                ])
            } else {
                Err(ErreurApi::DelaiDepasse)
            };
            Box::pin(async move { badges })
        }

        fn icone_badge(&self, badge: &str) -> Futur<Result<Vec<u8>, ErreurApi>> {
            self.icones.fetch_add(1, Ordering::SeqCst);
            let octets = match badge {
                "b2" => Ok(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#2f5d8a"/></svg>"##.to_vec()),
                "b1" => Ok(png(80)),
                _ => Err(ErreurApi::DelaiDepasse),
            };
            Box::pin(async move { octets })
        }
    }

    fn cle(utilisateur: &str, version: i64) -> CleAvatar {
        CleAvatar {
            utilisateur: utilisateur.into(),
            version,
            serveur: None,
        }
    }

    #[test]
    fn l_avatar_est_recadre_en_carre_et_ses_coins_arrondis() {
        let large = image::RgbaImage::from_pixel(400, 200, image::Rgba([10, 20, 30, 255]));
        let mut octets = std::io::Cursor::new(Vec::new());
        large
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        let avatar = decoder_avatar(&octets.into_inner()).unwrap();
        assert_eq!(
            (avatar.width(), avatar.height()),
            (COTE_AVATAR, COTE_AVATAR)
        );
        let pixels = avatar.as_slice();
        let cote = COTE_AVATAR as usize;
        assert_eq!(pixels[0].a, 0, "coin transparent");
        assert_eq!(pixels[cote * cote - 1].a, 0);
        assert_eq!(pixels[cote * (cote / 2) + cote / 2].a, 255, "centre opaque");
        assert_eq!(pixels[cote / 2].a, 255, "milieu du bord opaque");
        assert!(decoder_avatar(b"rien").is_none());
    }

    #[test]
    fn la_cle_tient_compte_du_serveur_et_refuse_les_identifiants_dangereux() {
        assert_eq!(
            cle("carol", 3).nom_de_fichier().as_deref(),
            Some("u.carol.v3")
        );
        let propre = CleAvatar {
            serveur: Some("s1".into()),
            ..cle("carol", 3)
        };
        assert_eq!(propre.nom_de_fichier().as_deref(), Some("s.s1.u.carol.v3"));
        assert_eq!(cle("../x", 1).nom_de_fichier(), None);
        assert_eq!(cle("a/b", 1).nom_de_fichier(), None);
    }

    #[tokio::test]
    async fn un_avatar_charge_n_est_plus_redemande_meme_apres_redemarrage() {
        let dossier = tempfile::tempdir().unwrap();
        let source = Arc::new(Source::default());
        let profils = Profils::nouveaux(
            source.clone(),
            Some(CacheDisque::ouvrir(dossier.path(), BORNE_DISQUE).unwrap()),
        );
        let image = profils.charger_avatar(cle("carol", 3)).await.unwrap();
        assert_eq!((image.width(), image.height()), (COTE_AVATAR, COTE_AVATAR));
        assert!(profils.charger_avatar(cle("carol", 3)).await.is_some());
        assert_eq!(source.avatars.load(Ordering::SeqCst), 1);
        assert!(profils.disque().unwrap().contient("u.carol.v3"));
        let relance = Profils::nouveaux(
            source.clone(),
            Some(CacheDisque::ouvrir(dossier.path(), BORNE_DISQUE).unwrap()),
        );
        assert!(relance.avatar(&cle("carol", 3)).is_none());
        assert!(relance.charger_avatar(cle("carol", 3)).await.is_some());
        assert_eq!(
            source.avatars.load(Ordering::SeqCst),
            1,
            "relu sur le disque"
        );
        assert!(relance.charger_avatar(cle("carol", 4)).await.is_some());
        assert_eq!(source.avatars.load(Ordering::SeqCst), 2, "nouvelle version");
        assert!(relance.charger_avatar(cle("sans", 1)).await.is_none());
        assert!(!relance.disque().unwrap().contient("u.sans.v1"));
    }

    #[tokio::test]
    async fn un_fichier_corrompu_est_ignore_et_redemande() {
        let dossier = tempfile::tempdir().unwrap();
        std::fs::write(dossier.path().join("u.carol.v3.cache"), b"pas une image").unwrap();
        let source = Arc::new(Source::default());
        let profils = Profils::nouveaux(
            source.clone(),
            Some(CacheDisque::ouvrir(dossier.path(), BORNE_DISQUE).unwrap()),
        );
        assert!(profils.charger_avatar(cle("carol", 3)).await.is_some());
        assert_eq!(source.avatars.load(Ordering::SeqCst), 1);
        let relu = std::fs::read(dossier.path().join("u.carol.v3.cache")).unwrap();
        assert!(decoder_avatar(&relu).is_some(), "fichier réécrit");
    }

    #[tokio::test]
    async fn le_cache_memoire_est_borne() {
        let source = Arc::new(Source::default());
        let profils = Profils::avec_capacites(source.clone(), None, 2, 2);
        for utilisateur in ["a", "b", "c"] {
            profils.charger_avatar(cle(utilisateur, 1)).await.unwrap();
        }
        assert_eq!(profils.avatars_en_memoire(), 2);
        assert!(profils.avatar(&cle("a", 1)).is_none());
        profils.charger_avatar(cle("a", 1)).await.unwrap();
        assert_eq!(source.avatars.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn les_badges_sont_tries_et_demandes_une_fois_meme_en_echec() {
        let source = Arc::new(Source::default());
        let profils = Profils::nouveaux(source.clone(), None);
        let noms: Vec<String> = profils
            .charger_badges("carol".into())
            .await
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(noms, ["Premier", "Abeille", "Zèbre", "Perdu"]);
        profils.charger_badges("carol".into()).await;
        assert!(profils.charger_badges("dave".into()).await.is_empty());
        profils.charger_badges("dave".into()).await;
        assert_eq!(source.badges.load(Ordering::SeqCst), 2);
        assert_eq!(source.icones.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn les_badges_montrent_leur_image_sinon_leur_emoji() {
        let source = Arc::new(Source::default());
        let profils = Profils::nouveaux(source.clone(), None);
        let badges = profils.charger_badges("carol".into()).await;
        let taille = |b: &Badge| image_de_badge(&profils, b).map(|i| i.size());
        assert_eq!(taille(&badges[0]).map(|t| t.width > 0), Some(true), "emoji");
        assert_eq!(
            taille(&badges[1]).map(|t| (t.width, t.height)),
            Some((64, 64)),
            "png réduit"
        );
        assert!(taille(&badges[2]).is_some_and(|t| t.width > 0), "svg");
        assert_eq!(taille(&badges[3]), None, "icône en échec, sans emoji");
        assert!(est_un_svg(b"\xef\xbb\xbf <?xml version=\"1.0\"?><svg/>"));
        assert!(!est_un_svg(&png(4)));
        assert!(decoder_icone_badge(b"").is_none());
        profils.charger_badges("carol".into()).await;
        assert_eq!(source.icones.load(Ordering::SeqCst), 3);
    }
}
