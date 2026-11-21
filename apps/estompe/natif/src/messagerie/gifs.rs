use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::gifs::{
    haut_de_rangee, hauteur_totale, rangee_de, rangees_visibles, ranger, voisin, DemandeGifs, Gif,
    RangeeGifs, RechercheGifs, ATTRIBUTION, ECART, IDEES, MARGE_DE_SUITE, PAR_RANGEE,
    TEMPORISATION, TENDANCES,
};
use estompe_client::http::ErreurApi;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::medias::Decodee;
use crate::{CelluleGif, DonneesGifs, Principale, RangeeGif};

pub const MARGE_BARRE: f32 = 8.0;
pub const LARGEUR_PAR_DEFAUT: f32 = 380.0;
pub const HAUTEUR_PAR_DEFAUT: f32 = 300.0;
pub const IDEES_EN_HAUT: usize = 5;

thread_local! {
    static TEMPORISATEUR: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static ANIMATEUR: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Animation {
    indice: usize,
    restant: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatistiquesGifs {
    pub reveils: usize,
    pub images_changees: usize,
}

#[derive(Default)]
pub struct EtatGifs {
    pub ouvert: bool,
    pub recherche: RechercheGifs,
    pub rangees: Vec<RangeeGifs>,
    pub actif: Option<usize>,
    pub statistiques: StatistiquesGifs,
    largeur: f32,
    haut: f32,
    hauteur: f32,
    images: HashMap<String, Decodee>,
    animations: HashMap<usize, Animation>,
    minuterie: bool,
}

impl EtatGifs {
    pub fn indices_visibles(&self) -> std::ops::Range<usize> {
        let r = rangees_visibles(&self.rangees, self.haut, self.hauteur);
        let debut = self.rangees.get(r.start).map_or(0, |x| x.debut);
        let fin = r
            .end
            .checked_sub(1)
            .and_then(|n| self.rangees.get(n))
            .map_or(debut, |x| x.indices().end);
        debut..fin
    }

    pub fn animees(&self) -> Vec<usize> {
        let mut animees: Vec<usize> = self.animations.keys().copied().collect();
        animees.sort_unstable();
        animees
    }

    fn image(&self, index: usize) -> slint::Image {
        let Some(gif) = self.recherche.resultats.get(index) else {
            return slint::Image::default();
        };
        match (self.images.get(&gif.id), self.animations.get(&index)) {
            (Some(Decodee::Animee(a)), Some(etat)) => {
                slint::Image::from_rgba8(a.images[etat.indice % a.images.len()].0.clone())
            }
            (Some(d), _) => slint::Image::from_rgba8(d.premiere().clone()),
            (None, _) => slint::Image::default(),
        }
    }

    fn cellule(&self, index: usize, largeur: f32) -> CelluleGif {
        let titre = self
            .recherche
            .resultats
            .get(index)
            .map(|g| g.title.as_str())
            .filter(|t| !t.is_empty())
            .unwrap_or("GIF");
        CelluleGif {
            index: index as i32,
            titre: titre.into(),
            image: self.image(index),
            largeur,
        }
    }

    fn rangee(&self, r: &RangeeGifs) -> RangeeGif {
        let cellules: Vec<CelluleGif> = r
            .indices()
            .zip(&r.largeurs)
            .map(|(i, l)| self.cellule(i, *l))
            .collect();
        RangeeGif {
            cellules: ModelRc::new(VecModel::from(cellules)),
            hauteur: r.hauteur,
        }
    }

    fn ranger(&mut self) {
        self.rangees = ranger(&self.recherche.resultats, self.largeur - MARGE_BARRE);
    }

    fn proche_du_bas(&self) -> bool {
        self.haut + self.hauteur > hauteur_totale(&self.rangees) - MARGE_DE_SUITE
    }
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Default)]
pub struct Gifs {
    etat: Arc<Mutex<EtatGifs>>,
}

impl Gifs {
    pub fn etat(&self) -> MutexGuard<'_, EtatGifs> {
        verrou(&self.etat)
    }

    pub fn ouvert(&self) -> bool {
        self.etat().ouvert
    }

    pub fn ouvrir(&self, f: &Principale) -> DemandeGifs {
        let demande = {
            let mut e = self.etat();
            e.ouvert = true;
            e.actif = None;
            e.haut = 0.0;
            if e.largeur <= 0.0 {
                e.largeur = LARGEUR_PAR_DEFAUT;
            }
            if e.hauteur <= 0.0 {
                e.hauteur = HAUTEUR_PAR_DEFAUT;
            }
            e.recherche.lancer("")
        };
        let g = f.global::<DonneesGifs>();
        let idees: Vec<SharedString> = std::iter::once(TENDANCES)
            .chain(IDEES)
            .map(SharedString::from)
            .collect();
        g.set_idees_haut(ModelRc::new(VecModel::from(
            idees[..IDEES_EN_HAUT].to_vec(),
        )));
        g.set_idees_bas(ModelRc::new(VecModel::from(
            idees[IDEES_EN_HAUT..].to_vec(),
        )));
        g.set_attribution(ATTRIBUTION.into());
        g.set_recherche(SharedString::new());
        g.set_defilement(0.0);
        g.set_ouvert(true);
        self.afficher(f, true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
        demande
    }

    pub fn fermer(&self, f: &Principale) {
        TEMPORISATEUR.with(|t| *t.borrow_mut() = None);
        ANIMATEUR.with(|a| *a.borrow_mut() = None);
        {
            let mut e = self.etat();
            e.ouvert = false;
            e.actif = None;
            e.images.clear();
            e.animations.clear();
            e.minuterie = false;
            e.recherche = RechercheGifs::default();
            e.rangees.clear();
        }
        let g = f.global::<DonneesGifs>();
        g.set_ouvert(false);
        g.set_rangees(ModelRc::default());
    }

    pub fn temporiser(&self, mut lancer: impl FnMut() + 'static) {
        TEMPORISATEUR.with(|t| {
            t.borrow_mut()
                .get_or_insert_with(slint::Timer::default)
                .start(slint::TimerMode::SingleShot, TEMPORISATION, move || {
                    lancer()
                });
        });
    }

    pub fn lancer(&self, f: &Principale, terme: &str) -> DemandeGifs {
        TEMPORISATEUR.with(|t| {
            if let Some(t) = t.borrow().as_ref() {
                t.stop();
            }
        });
        let demande = {
            let mut e = self.etat();
            e.actif = None;
            e.haut = 0.0;
            e.animations.clear();
            let demande = e.recherche.lancer(terme);
            e.rangees.clear();
            demande
        };
        f.global::<DonneesGifs>().set_defilement(0.0);
        self.afficher(f, true);
        demande
    }

    pub fn idee(&self, f: &Principale, libelle: &str) -> DemandeGifs {
        let terme = if libelle == TENDANCES { "" } else { libelle };
        f.global::<DonneesGifs>().set_recherche(terme.into());
        self.lancer(f, terme)
    }

    pub fn recevoir(
        &self,
        f: &Principale,
        sequence: u64,
        resultat: Result<Vec<Gif>, ErreurApi>,
    ) -> Option<Vec<Gif>> {
        let (debut, nouveaux) = {
            let mut e = self.etat();
            if !e.ouvert {
                return None;
            }
            let debut = e.recherche.recevoir(sequence, resultat)?;
            e.ranger();
            (debut, e.recherche.resultats[debut..].to_vec())
        };
        let premiere = debut / PAR_RANGEE;
        let rangees = f.global::<DonneesGifs>().get_rangees();
        match rangees.as_any().downcast_ref::<VecModel<RangeeGif>>() {
            Some(m) if debut > 0 && m.row_count() >= premiere => {
                let e = self.etat();
                while m.row_count() > premiere {
                    m.remove(m.row_count() - 1);
                }
                for r in &e.rangees[premiere..] {
                    m.push(e.rangee(r));
                }
                drop(e);
                self.afficher(f, false);
            }
            _ => self.afficher(f, true),
        }
        self.suivre_animations(f);
        Some(nouveaux)
    }

    pub fn suite_si_besoin(&self) -> Option<DemandeGifs> {
        let mut e = self.etat();
        if !e.ouvert || !e.proche_du_bas() {
            return None;
        }
        e.recherche.suite()
    }

    pub fn installer_image(&self, f: &Principale, id: &str, image: Decodee) {
        let index = {
            let mut e = self.etat();
            let Some(index) = e.recherche.resultats.iter().position(|g| g.id == id) else {
                return;
            };
            e.images.insert(id.to_string(), image);
            index
        };
        self.remplacer(f, index);
        self.suivre_animations(f);
    }

    fn remplacer(&self, f: &Principale, index: usize) {
        let e = self.etat();
        let Some(r) = rangee_de(&e.rangees, index) else {
            return;
        };
        let rangees = f.global::<DonneesGifs>().get_rangees();
        let Some(rangee) = rangees.row_data(r) else {
            return;
        };
        let colonne = index - e.rangees[r].debut;
        if let Some(mut cellule) = rangee.cellules.row_data(colonne) {
            cellule.image = e.image(index);
            rangee.cellules.set_row_data(colonne, cellule);
        }
    }

    pub fn defile(
        &self,
        f: &Principale,
        haut: f32,
        hauteur: f32,
        largeur: f32,
    ) -> Option<DemandeGifs> {
        let reranger = {
            let mut e = self.etat();
            if !e.ouvert {
                return None;
            }
            e.haut = haut.max(0.0);
            if hauteur > 0.0 {
                e.hauteur = hauteur;
            }
            let reranger = largeur > 0.0 && (largeur - e.largeur).abs() >= 1.0;
            if reranger {
                e.largeur = largeur;
                e.ranger();
            }
            reranger
        };
        if reranger {
            self.afficher(f, true);
        }
        self.suivre_animations(f);
        self.suite_si_besoin()
    }

    pub fn deplacer(&self, f: &Principale, dx: i32, dy: i32) {
        let (haut, bas) = {
            let mut e = self.etat();
            e.actif = voisin(&e.rangees, e.actif, dx, dy);
            let Some(r) = e.actif.and_then(|i| rangee_de(&e.rangees, i)) else {
                drop(e);
                self.afficher(f, false);
                return;
            };
            let haut = haut_de_rangee(&e.rangees, r);
            (haut, haut + e.rangees[r].hauteur)
        };
        let g = f.global::<DonneesGifs>();
        let (vu, hauteur) = {
            let e = self.etat();
            (e.haut, e.hauteur)
        };
        if haut < vu {
            g.set_defilement(-haut);
        } else if bas > vu + hauteur {
            g.set_defilement(-(bas + ECART - hauteur));
        }
        self.afficher(f, false);
    }

    pub fn choisir(&self, index: Option<usize>) -> Option<Gif> {
        let e = self.etat();
        let index = index.or(e.actif)?;
        e.recherche.resultats.get(index).cloned()
    }

    fn afficher(&self, f: &Principale, rangees: bool) {
        let g = f.global::<DonneesGifs>();
        let e = self.etat();
        g.set_en_cours(e.recherche.en_cours);
        g.set_message(e.recherche.message.as_str().into());
        g.set_actif(e.actif.map_or(-1, |i| i as i32));
        let active = if e.recherche.terme.is_empty() {
            TENDANCES
        } else {
            e.recherche.terme.as_str()
        };
        g.set_idee_active(active.into());
        if rangees {
            let lignes: Vec<RangeeGif> = e.rangees.iter().map(|r| e.rangee(r)).collect();
            g.set_rangees(ModelRc::new(VecModel::from(lignes)));
        }
    }

    pub fn suivre_animations(&self, f: &Principale) {
        let (prochain, deja) = {
            let mut garde = self.etat();
            let e = &mut *garde;
            let mut actives = HashMap::new();
            if e.ouvert {
                for index in e.indices_visibles() {
                    let Some(gif) = e.recherche.resultats.get(index) else {
                        continue;
                    };
                    let Some(Decodee::Animee(a)) = e.images.get(&gif.id) else {
                        continue;
                    };
                    let etat = e.animations.get(&index).copied().unwrap_or(Animation {
                        indice: 0,
                        restant: a.images[0].1,
                    });
                    actives.insert(index, etat);
                }
            }
            e.animations = actives;
            let prochain = e.animations.values().map(|a| a.restant).min();
            let deja = std::mem::replace(&mut e.minuterie, prochain.is_some());
            (prochain, deja)
        };
        match prochain {
            Some(d) if !deja => self.armer(f, d),
            Some(_) => {}
            None => ANIMATEUR.with(|a| {
                if let Some(t) = a.borrow().as_ref() {
                    t.stop();
                }
            }),
        }
    }

    fn armer(&self, f: &Principale, d: Duration) {
        let gifs = self.clone();
        let faible = f.as_weak();
        ANIMATEUR.with(|a| {
            a.borrow_mut()
                .get_or_insert_with(slint::Timer::default)
                .start(slint::TimerMode::SingleShot, d, move || {
                    if let Some(f) = faible.upgrade() {
                        gifs.avancer(&f, d);
                    }
                });
        });
    }

    fn avancer(&self, f: &Principale, ecoule: Duration) {
        let (changees, prochain) = {
            let mut garde = self.etat();
            let e = &mut *garde;
            e.statistiques.reveils += 1;
            let mut changees = Vec::new();
            for (index, etat) in e.animations.iter_mut() {
                let Some(Decodee::Animee(a)) = e
                    .recherche
                    .resultats
                    .get(*index)
                    .and_then(|g| e.images.get(&g.id))
                else {
                    continue;
                };
                let mut reste = ecoule;
                let mut change = false;
                while reste >= etat.restant {
                    reste -= etat.restant;
                    etat.indice = (etat.indice + 1) % a.images.len();
                    etat.restant = a.images[etat.indice].1;
                    change = true;
                }
                etat.restant -= reste;
                if change {
                    changees.push(*index);
                }
            }
            e.statistiques.images_changees += changees.len();
            let prochain = e.animations.values().map(|a| a.restant).min();
            e.minuterie = prochain.is_some();
            (changees, prochain)
        };
        for index in changees {
            self.remplacer(f, index);
        }
        if let Some(d) = prochain {
            self.armer(f, d);
        }
    }
}
