use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use estompe_client::vocal::video::{miroir_rgba, taille_de_conversion};
use estompe_client::vocal::{CleVideo, ImageVideo, RecepteurVideo, TrameVideo};
use slint::{ComponentHandle, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::coquille::pont::vec_de;
use crate::{DonneesVideo, FluxVideo, Principale};

pub const CADENCE_MAX: u32 = 60;
const ATTENTE: Duration = Duration::from_millis(250);
const PAUSE_EN_VOL: Duration = Duration::from_millis(1);

pub type Pixels = SharedPixelBuffer<Rgba8Pixel>;

pub fn convertir(trame: &dyn TrameVideo, zone: (u32, u32), miroir: bool) -> Option<Pixels> {
    let (largeur, hauteur) = taille_de_conversion((trame.largeur(), trame.hauteur()), zone);
    if trame.largeur() == 0 || trame.hauteur() == 0 {
        return None;
    }
    let mut pixels = Pixels::new(largeur, hauteur);
    trame.vers_rgba(largeur, hauteur, pixels.make_mut_bytes());
    if miroir {
        miroir_rgba(pixels.make_mut_bytes(), largeur);
    }
    Some(pixels)
}

#[derive(Clone)]
pub struct FluxVoulu {
    pub cle: CleVideo,
    pub recepteur: Arc<RecepteurVideo>,
    pub zone: (u32, u32),
    pub miroir: bool,
}

pub type Livraison = Arc<dyn Fn(String, Option<Pixels>, Arc<AtomicBool>) + Send + Sync>;

struct Commun {
    arret: AtomicBool,
    zone: Mutex<(u32, u32)>,
    en_vol: Arc<AtomicBool>,
}

pub struct Afficheur {
    commun: Arc<Commun>,
    recepteur: Arc<RecepteurVideo>,
    miroir: bool,
    fil: Option<JoinHandle<()>>,
}

fn attendre_le_tour(commun: &Commun, dernier: Option<Instant>) -> bool {
    if let Some(dernier) = dernier {
        let periode = Duration::from_secs(1) / CADENCE_MAX;
        let ecoule = dernier.elapsed();
        if ecoule < periode {
            std::thread::sleep(periode - ecoule);
        }
    }
    while commun.en_vol.load(Ordering::Acquire) {
        if commun.arret.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(PAUSE_EN_VOL);
    }
    !commun.arret.load(Ordering::Acquire)
}

impl Afficheur {
    pub fn demarrer(
        cle: String,
        recepteur: Arc<RecepteurVideo>,
        zone: (u32, u32),
        miroir: bool,
        convertis: Arc<AtomicU64>,
        livrer: Livraison,
    ) -> Self {
        let commun = Arc::new(Commun {
            arret: AtomicBool::new(false),
            zone: Mutex::new(zone),
            en_vol: Arc::new(AtomicBool::new(false)),
        });
        let fil = {
            let commun = commun.clone();
            let recepteur = recepteur.clone();
            std::thread::Builder::new()
                .name(format!("video {cle}"))
                .spawn(move || {
                    let mut apres = u64::MAX;
                    let mut dernier: Option<Instant> = None;
                    while !commun.arret.load(Ordering::Acquire) {
                        let Some((generation, _)) = recepteur.attendre(apres, ATTENTE) else {
                            continue;
                        };
                        apres = generation;
                        if !attendre_le_tour(&commun, dernier) {
                            break;
                        }
                        let image: Option<ImageVideo> = recepteur.derniere();
                        let zone = *commun.zone.lock().unwrap_or_else(|e| e.into_inner());
                        let pixels = image.and_then(|i| convertir(&*i, zone, miroir));
                        if pixels.is_some() {
                            convertis.fetch_add(1, Ordering::Relaxed);
                        }
                        dernier = Some(Instant::now());
                        commun.en_vol.store(true, Ordering::Release);
                        livrer(cle.clone(), pixels, commun.en_vol.clone());
                    }
                })
                .ok()
        };
        Self {
            commun,
            recepteur,
            miroir,
            fil,
        }
    }

    pub fn redimensionner(&self, zone: (u32, u32)) {
        let mut actuelle = self.commun.zone.lock().unwrap_or_else(|e| e.into_inner());
        if *actuelle != zone {
            *actuelle = zone;
        }
    }

    fn meme_source(&self, voulu: &FluxVoulu) -> bool {
        Arc::ptr_eq(&self.recepteur, &voulu.recepteur) && self.miroir == voulu.miroir
    }
}

impl Drop for Afficheur {
    fn drop(&mut self) {
        self.commun.arret.store(true, Ordering::Release);
        self.recepteur.reveiller();
        drop(self.fil.take());
    }
}

pub struct GestionVideo {
    afficheurs: BTreeMap<CleVideo, Afficheur>,
    convertis: Arc<AtomicU64>,
    livrer: Livraison,
}

pub fn livraison_slint(fenetre: slint::Weak<Principale>) -> Livraison {
    Arc::new(move |cle, pixels, en_vol| {
        let liberer = en_vol.clone();
        let resultat = fenetre.upgrade_in_event_loop(move |f| {
            liberer.store(false, Ordering::Release);
            poser_pixels(&f, &cle, pixels);
        });
        if resultat.is_err() {
            en_vol.store(false, Ordering::Release);
        }
    })
}

pub fn installer_modeles(f: &Principale) {
    f.global::<DonneesVideo>()
        .set_flux(ModelRc::new(VecModel::<FluxVideo>::default()));
}

pub fn poser_pixels(f: &Principale, cle: &str, pixels: Option<Pixels>) {
    let flux = f.global::<DonneesVideo>().get_flux();
    let modele = vec_de(&flux);
    let Some(i) = modele.iter().position(|r| r.cle == cle) else {
        return;
    };
    let mut ligne = modele.row_data(i).unwrap_or_default();
    match pixels {
        Some(p) => {
            ligne.largeur = p.width() as i32;
            ligne.hauteur = p.height() as i32;
            ligne.image = slint::Image::from_rgba8(p);
            ligne.pret = true;
        }
        None => {
            ligne.image = slint::Image::default();
            ligne.pret = false;
        }
    }
    modele.set_row_data(i, ligne);
}

impl GestionVideo {
    pub fn nouvelle(livrer: Livraison) -> Self {
        Self {
            afficheurs: BTreeMap::new(),
            convertis: Arc::new(AtomicU64::new(0)),
            livrer,
        }
    }

    pub fn images_converties(&self) -> u64 {
        self.convertis.load(Ordering::Relaxed)
    }

    pub fn compteur(&self) -> Arc<AtomicU64> {
        self.convertis.clone()
    }

    pub fn actifs(&self) -> usize {
        self.afficheurs.len()
    }

    pub fn synchroniser(&mut self, f: &Principale, voulus: &[FluxVoulu]) {
        self.afficheurs
            .retain(|cle, a| voulus.iter().any(|v| &v.cle == cle && a.meme_source(v)));
        for voulu in voulus {
            match self.afficheurs.get(&voulu.cle) {
                Some(a) => a.redimensionner(voulu.zone),
                None => {
                    let afficheur = Afficheur::demarrer(
                        voulu.cle.texte(),
                        voulu.recepteur.clone(),
                        voulu.zone,
                        voulu.miroir,
                        self.convertis.clone(),
                        self.livrer.clone(),
                    );
                    self.afficheurs.insert(voulu.cle.clone(), afficheur);
                }
            }
        }
        let flux = f.global::<DonneesVideo>().get_flux();
        let modele = vec_de(&flux);
        let anciennes: Vec<FluxVideo> = modele.iter().collect();
        let cles: Vec<String> = voulus.iter().map(|v| v.cle.texte()).collect();
        if anciennes
            .iter()
            .map(|l| l.cle.as_str())
            .eq(cles.iter().map(String::as_str))
        {
            return;
        }
        let lignes: Vec<FluxVideo> = cles
            .into_iter()
            .map(|cle| {
                anciennes
                    .iter()
                    .find(|l| l.cle == cle)
                    .cloned()
                    .unwrap_or(FluxVideo {
                        cle: cle.into(),
                        ..FluxVideo::default()
                    })
            })
            .collect();
        modele.set_vec(lignes);
    }

    pub fn arreter(&mut self, f: &Principale) {
        self.synchroniser(f, &[]);
    }
}

pub fn indice(f: &Principale, cle: &CleVideo) -> i32 {
    let texte = cle.texte();
    f.global::<DonneesVideo>()
        .get_flux()
        .iter()
        .position(|l| l.cle == texte)
        .map_or(-1, |i| i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::vocal::ImageI420;

    #[test]
    fn la_conversion_respecte_la_zone_et_le_miroir() {
        let mut image = ImageI420::noire(64, 32);
        image.remplir_rectangle(0, 0, 32, 32, [220, 20, 20]);
        image.remplir_rectangle(32, 0, 32, 32, [20, 20, 220]);
        let droite = convertir(&image, (32, 16), false).unwrap();
        assert_eq!((droite.width(), droite.height()), (32, 16));
        assert!(droite.as_slice()[0].r > 150);
        let miroir = convertir(&image, (32, 16), true).unwrap();
        assert!(miroir.as_slice()[0].b > miroir.as_slice()[0].r);
    }

    #[test]
    fn l_afficheur_ne_convertit_qu_une_image_a_la_fois() {
        let recepteur = RecepteurVideo::nouveau();
        let livrees = Arc::new(Mutex::new(Vec::new()));
        let en_attente: Arc<Mutex<Option<Arc<AtomicBool>>>> = Arc::new(Mutex::new(None));
        let livrer: Livraison = {
            let livrees = livrees.clone();
            let en_attente = en_attente.clone();
            Arc::new(move |_, pixels: Option<Pixels>, en_vol| {
                livrees.lock().unwrap().push(pixels.map(|p| p.width()));
                *en_attente.lock().unwrap() = Some(en_vol);
            })
        };
        let compteur = Arc::new(AtomicU64::new(0));
        let afficheur = Afficheur::demarrer(
            "u1#camera".into(),
            recepteur.clone(),
            (40, 20),
            false,
            compteur.clone(),
            livrer,
        );
        let attendre = |n: usize| {
            let debut = Instant::now();
            while livrees.lock().unwrap().len() < n {
                assert!(debut.elapsed() < Duration::from_secs(10), "livraison {n}");
                std::thread::yield_now();
            }
        };
        attendre(1);
        assert_eq!(livrees.lock().unwrap()[0], None);
        en_attente
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .store(false, Ordering::Release);
        for _ in 0..5 {
            recepteur.pousser(Arc::new(ImageI420::noire(80, 40)));
        }
        attendre(2);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(livrees.lock().unwrap().len(), 2, "une seule image en vol");
        assert_eq!(livrees.lock().unwrap()[1], Some(40));
        drop(afficheur);
        assert!(compteur.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    #[ignore = "mesure longue : ESTOMPE_MESURE_VIDEO_S=600 cargo test --release --lib -- --ignored --nocapture mesure_une_piste"]
    fn mesure_une_piste_1080p_a_30_images_par_seconde() {
        use estompe_client::vocal::Mire;
        use estompe_plateforme::{MesureProcessus, MesureSysteme};
        let duree = std::env::var("ESTOMPE_MESURE_VIDEO_S")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);
        let mesure = MesureSysteme;
        let recepteur = RecepteurVideo::nouveau();
        let livrer: Livraison = Arc::new(|_, pixels: Option<Pixels>, en_vol| {
            drop(pixels.map(slint::Image::from_rgba8));
            en_vol.store(false, Ordering::Release);
        });
        let compteur = Arc::new(AtomicU64::new(0));
        let afficheur = Afficheur::demarrer(
            "mesure".into(),
            recepteur.clone(),
            (640, 360),
            false,
            compteur.clone(),
            livrer,
        );
        let mire = Mire::nouvelle(1920, 1080);
        let debut = Instant::now();
        let mut avant = mesure.compteurs().unwrap();
        let mut instant = Instant::now();
        let mut memoires = Vec::new();
        let mut n = 0u64;
        while debut.elapsed() < Duration::from_secs(duree) {
            recepteur.pousser(Arc::new(mire.image(n)));
            n += 1;
            let prochaine = debut + Duration::from_secs(1) * n as u32 / 30;
            if let Some(attente) = prochaine.checked_duration_since(Instant::now()) {
                std::thread::sleep(attente);
            }
            if instant.elapsed() >= Duration::from_secs(10) {
                let apres = mesure.compteurs().unwrap();
                let cpu =
                    (apres.cpu_ms - avant.cpu_ms) as f64 / instant.elapsed().as_millis() as f64;
                memoires.push(apres.prive);
                println!(
                    "{:>4} s : {} images converties, mémoire privée {:.1} Mo, CPU {:.1} % d'un cœur",
                    debut.elapsed().as_secs(),
                    compteur.load(Ordering::Relaxed),
                    apres.prive as f64 / 1e6,
                    100.0 * cpu
                );
                avant = apres;
                instant = Instant::now();
            }
        }
        drop(afficheur);
        let converties = compteur.load(Ordering::Relaxed);
        println!("{n} images produites, {converties} converties en 640 × 360");
        assert!(converties as f64 >= 0.9 * n as f64);
        if let (Some(min), Some(max)) =
            (memoires.iter().skip(1).min(), memoires.iter().skip(1).max())
        {
            println!(
                "mémoire privée après la première mesure : {:.1} à {:.1} Mo",
                *min as f64 / 1e6,
                *max as f64 / 1e6
            );
        }
    }
}
