use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use estompe_client::replay::montage::{arguments_apercu, FluxApercu, Plan};
use estompe_plateforme::{commande_d_arriere_plan, FluxSon, SortieSon};
use slint::{Rgba8Pixel, SharedPixelBuffer};

pub type SortiePartagee = Arc<dyn SortieSon + Send + Sync>;
pub type ImageApercu = SharedPixelBuffer<Rgba8Pixel>;

const TAUX_APERCU: f64 = 48_000.0;

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn image_rgba(octets: &[u8], largeur: u32, hauteur: u32) -> Option<ImageApercu> {
    (octets.len() == (largeur * hauteur * 4) as usize)
        .then(|| SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(octets, largeur, hauteur))
}

pub struct Lecteur {
    ffmpeg: PathBuf,
    generation: Arc<AtomicU64>,
    enfants: Arc<Mutex<Vec<Child>>>,
    flux: Arc<Mutex<Option<FluxSon>>>,
}

impl Lecteur {
    pub fn nouveau(ffmpeg: PathBuf) -> Self {
        Self {
            ffmpeg,
            generation: Arc::new(AtomicU64::new(0)),
            enfants: Arc::new(Mutex::new(Vec::new())),
            flux: Arc::new(Mutex::new(None)),
        }
    }

    pub fn arreter(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        for mut enfant in verrou(&self.enfants).drain(..) {
            let _ = enfant.kill();
            let _ = enfant.wait();
        }
        *verrou(&self.flux) = None;
        generation
    }

    fn lancer(&self, dossier: &Path, plan: &Plan, flux: FluxApercu) -> Option<ChildStdout> {
        let mut enfant = commande_d_arriere_plan(&self.ffmpeg)
            .args(["-hide_banner", "-nostdin"])
            .args(arguments_apercu(dossier, plan, flux))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let sortie = enfant.stdout.take();
        verrou(&self.enfants).push(enfant);
        sortie
    }

    pub fn image(
        &self,
        dossier: PathBuf,
        plan: Plan,
        rappel: impl FnOnce(Option<ImageApercu>) + Send + 'static,
    ) {
        let generation = self.arreter();
        let sortie = self.lancer(&dossier, &plan, FluxApercu::Video);
        let courante = self.generation.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-apercu".into())
            .spawn(move || {
                let mut octets = Vec::new();
                if let Some(mut s) = sortie {
                    let _ = s.read_to_end(&mut octets);
                }
                if courante.load(Ordering::SeqCst) == generation {
                    rappel(image_rgba(&octets, plan.largeur, plan.hauteur));
                }
            });
    }

    pub fn lire(
        &self,
        dossier: PathBuf,
        plan: Plan,
        sortie_son: Option<SortiePartagee>,
        image: impl Fn(u64, ImageApercu) + Send + 'static,
        fin: impl FnOnce() + Send + 'static,
    ) {
        let generation = self.arreter();
        let video = self.lancer(&dossier, &plan, FluxApercu::Video);
        let demarre = Arc::new(AtomicBool::new(false));
        if let Some(sortie_son) = sortie_son.filter(|s| s.disponible() && plan.son()) {
            if let Some(son) = self.lancer(&dossier, &plan, FluxApercu::Son) {
                self.jouer(son, sortie_son, demarre.clone(), generation);
            }
        }
        let courante = self.generation.clone();
        let taille = (plan.largeur * plan.hauteur * 4) as usize;
        let periode = 1.0 / f64::from(plan.ips);
        let _ = std::thread::Builder::new()
            .name("estompe-lecture-montage".into())
            .spawn(move || {
                let Some(mut video) = video else {
                    fin();
                    return;
                };
                let mut tampon = vec![0u8; taille];
                let mut depart: Option<Instant> = None;
                let mut k = 0u64;
                while video.read_exact(&mut tampon).is_ok() {
                    if courante.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let debut = *depart.get_or_insert_with(|| {
                        demarre.store(true, Ordering::SeqCst);
                        Instant::now()
                    });
                    let echeance = Duration::from_secs_f64(k as f64 * periode);
                    let ecoule = debut.elapsed();
                    if echeance > ecoule {
                        std::thread::sleep(echeance - ecoule);
                    }
                    if debut.elapsed() < echeance + Duration::from_secs_f64(periode * 2.0) {
                        if let Some(i) = image_rgba(&tampon, plan.largeur, plan.hauteur) {
                            image(k, i);
                        }
                    }
                    k += 1;
                }
                if courante.load(Ordering::SeqCst) == generation {
                    fin();
                }
            });
    }

    fn jouer(
        &self,
        mut son: ChildStdout,
        sortie: SortiePartagee,
        demarre: Arc<AtomicBool>,
        generation: u64,
    ) {
        let echantillons: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let ecrits = echantillons.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-son-montage".into())
            .spawn(move || {
                let mut morceau = [0u8; 4096];
                while let Ok(n) = son.read(&mut morceau) {
                    if n == 0 {
                        break;
                    }
                    let mut e = verrou(&ecrits);
                    e.extend(
                        morceau[..n - n % 4]
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|o| f32::from_le_bytes(*o)),
                    );
                }
            });
        let flux = self.flux.clone();
        let courante = self.generation.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-sortie-montage".into())
            .spawn(move || {
                let mut phase = 0.0f64;
                let lus = echantillons.clone();
                let resultat = sortie.jouer(Box::new(move |tampon, taux| {
                    if !demarre.load(Ordering::SeqCst) {
                        tampon.fill(0.0);
                        return;
                    }
                    let pas = TAUX_APERCU / f64::from(taux.max(1));
                    let mut e = verrou(&lus);
                    for s in tampon.iter_mut() {
                        *s = e.front().copied().unwrap_or(0.0);
                        phase += pas;
                        while phase >= 1.0 {
                            e.pop_front();
                            phase -= 1.0;
                        }
                    }
                }));
                if let Ok(f) = resultat {
                    if courante.load(Ordering::SeqCst) == generation {
                        *verrou(&flux) = Some(f);
                    }
                }
            });
    }
}

impl Drop for Lecteur {
    fn drop(&mut self) {
        self.arreter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_rgba_aux_dimensions_du_plan() {
        let i = image_rgba(&[10, 20, 30, 255, 1, 2, 3, 255], 2, 1).unwrap();
        assert_eq!((i.width(), i.height(), i.as_slice()[1].g), (2, 1, 2));
        assert!(image_rgba(&[0; 7], 2, 1).is_none());
    }
}
