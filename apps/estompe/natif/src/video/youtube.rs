use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use estompe_client::apercu_liens::VideoYoutube;
use estompe_client::journal::{Journal, Niveau};
use estompe_client::regarder_ensemble::adresse_navigateur;
use estompe_client::regarder_ensemble::youtube::{
    navigation_permise, ouverture_externe, script_envoi, LienVue, PageLecteur, TITRE_VISIONNEUSE,
};
use estompe_plateforme::{CadreVue, OptionsVueWeb, VueWeb, VuesWeb};
use slint::ComponentHandle;

use crate::messagerie::pont::Navigateur;
use crate::{DonneesVueWeb, Principale};

pub const PLACE_SEANCE: &str = "seance";
pub const PLACE_VISIONNEUSE: &str = "visionneuse";
pub const PLACE_SOLO: &str = "solo";
pub const PLACE_MUSIQUE: &str = "musique";
pub const DOSSIER_VUE_WEB: &str = "vue-web";
const SOURCE_JOURNAL: &str = "youtube";
const SANS_NOUVELLES: Duration = Duration::from_millis(600);
const VERIFICATION: Duration = Duration::from_millis(250);

pub type RappelLien = Rc<dyn Fn()>;

pub struct OutilsVueWeb {
    pub vues: Rc<dyn VuesWeb>,
    pub dossier: PathBuf,
    pub navigateur: Navigateur,
    pub journal: Option<Arc<Journal>>,
}

struct Ouverte {
    vue: Box<dyn VueWeb>,
    lien: Option<LienVue>,
    montree: bool,
}

#[derive(Clone, Copy)]
struct Cadre {
    cadre: CadreVue,
    montree: bool,
    vu: Instant,
}

#[derive(Default)]
struct Etat {
    outils: Option<Rc<OutilsVueWeb>>,
    cadres: HashMap<String, Cadre>,
    ouvertes: HashMap<String, Ouverte>,
    adresse_visionneuse: String,
    verification: Option<slint::Timer>,
}

thread_local! {
    static ETAT: RefCell<Etat> = RefCell::new(Etat::default());
    static EN_OUVERTURE: Cell<bool> = const { Cell::new(false) };
}

fn avec<R>(action: impl FnOnce(&mut Etat) -> R) -> Option<R> {
    ETAT.with(|e| e.try_borrow_mut().ok().map(|mut e| action(&mut e)))
}

fn outils() -> Option<Rc<OutilsVueWeb>> {
    avec(|e| e.outils.clone()).flatten()
}

fn journal(message: &str) {
    if let Some(j) = outils().and_then(|o| o.journal.clone()) {
        j.ecrire(Niveau::Avertissement, SOURCE_JOURNAL, message);
    }
}

pub fn disponible() -> bool {
    outils().is_some_and(|o| o.vues.disponible())
}

pub fn installer(f: &Principale, o: OutilsVueWeb) {
    avec(|e| e.outils = Some(Rc::new(o)));
    let g = f.global::<DonneesVueWeb>();
    g.on_cadre(|nom, x, y, largeur, hauteur, montree| {
        placer(
            nom.as_str(),
            CadreVue {
                x,
                y,
                largeur,
                hauteur,
            },
            montree,
        );
    });
    let faible = f.as_weak();
    g.on_fermer_visionneuse(move || {
        if let Some(f) = faible.upgrade() {
            fermer_visionneuse(&f);
        }
    });
    let faible = f.as_weak();
    g.on_navigateur_visionneuse(move || {
        let adresse = avec(|e| e.adresse_visionneuse.clone()).unwrap_or_default();
        if let Some(o) = outils() {
            let _ = o.navigateur.ouvrir(&adresse);
        }
        if let Some(f) = faible.upgrade() {
            fermer_visionneuse(&f);
        }
    });
}

fn visible(o: &Ouverte, c: Option<&Cadre>) -> bool {
    c.is_some_and(|c| c.montree && c.vu.elapsed() < SANS_NOUVELLES)
        && o.lien.as_ref().is_none_or(LienVue::chargee)
}

fn actualiser(place: &str) {
    let a_faire = avec(|e| {
        let c = e.cadres.get(place).copied();
        let o = e.ouvertes.get_mut(place)?;
        let montree = visible(o, c.as_ref());
        let change = std::mem::replace(&mut o.montree, montree) != montree;
        Some((c.map(|c| c.cadre), montree, change))
    })
    .flatten();
    let Some((cadre, montree, change)) = a_faire else {
        return;
    };
    avec(|e| {
        if let Some(o) = e.ouvertes.get(place) {
            if let Some(c) = cadre {
                let _ = o.vue.placer(c);
            }
            if change {
                let _ = o.vue.montrer(montree);
            }
        }
    });
}

fn placer(place: &str, cadre: CadreVue, montree: bool) {
    let ancien = avec(|e| {
        e.cadres
            .insert(
                place.to_string(),
                Cadre {
                    cadre,
                    montree,
                    vu: Instant::now(),
                },
            )
            .map(|c| (c.cadre, c.montree))
    })
    .flatten();
    let ouverte = avec(|e| e.ouvertes.get(place).map(|o| o.montree)).flatten();
    match ouverte {
        Some(m) if ancien == Some((cadre, montree)) && m == montree => {}
        Some(_) => actualiser(place),
        None => {}
    }
}

fn verifier() {
    let places: Vec<String> = avec(|e| e.ouvertes.keys().cloned().collect()).unwrap_or_default();
    for p in places {
        actualiser(&p);
    }
}

fn surveiller() {
    let vide = avec(|e| e.ouvertes.is_empty()).unwrap_or(true);
    avec(|e| {
        if vide {
            e.verification = None;
        } else if e.verification.is_none() {
            let t = slint::Timer::default();
            t.start(slint::TimerMode::Repeated, VERIFICATION, verifier);
            e.verification = Some(t);
        }
    });
}

fn poignee(f: &Principale) -> Option<estompe_plateforme::RawWindowHandle> {
    use slint::winit_030::winit::raw_window_handle::HasWindowHandle;
    use slint::winit_030::WinitWindowAccessor;
    f.window()
        .with_winit_window(|w| w.window_handle().ok().map(|p| p.as_raw()))
        .flatten()
}

fn fermer(place: &str) {
    let ouverte = avec(|e| e.ouvertes.remove(place)).flatten();
    drop(ouverte);
    surveiller();
}

fn ouvrir(
    f: &Principale,
    place: &str,
    page: &PageLecteur,
    lien: Option<LienVue>,
    rappel: Option<RappelLien>,
) -> Result<(), String> {
    if EN_OUVERTURE.with(|e| e.replace(true)) {
        return Err("ouverture déjà en cours".into());
    }
    let resultat = ouvrir_sans_reentree(f, place, page, lien, rappel);
    EN_OUVERTURE.with(|e| e.set(false));
    resultat
}

fn ouvrir_sans_reentree(
    f: &Principale,
    place: &str,
    page: &PageLecteur,
    lien: Option<LienVue>,
    rappel: Option<RappelLien>,
) -> Result<(), String> {
    let o = outils().ok_or("vue web non installée")?;
    if !o.vues.disponible() {
        return Err("vue web absente de cette version".into());
    }
    fermer(place);
    let fenetre = poignee(f).ok_or("fenêtre sans poignée native")?;
    let cadre = avec(|e| e.cadres.get(place).map(|c| c.cadre))
        .flatten()
        .unwrap_or_default();
    let origine = o.vues.origine();
    let navigateur = o.navigateur.clone();
    let recepteur = lien.clone();
    let options = OptionsVueWeb {
        html: page.html(origine),
        cadre,
        visible: false,
        dossier: o.dossier.clone(),
        navigation: Box::new(move |adresse: &str| navigation_permise(adresse, origine)),
        nouvelle_fenetre: Box::new(move |adresse: &str| {
            if let Some(a) = ouverture_externe(adresse) {
                let _ = navigateur.ouvrir(&a);
            }
        }),
        message: Box::new(move |source: &str, texte: &str| {
            if let Some(l) = &recepteur {
                if l.recevoir(source, origine, texte) {
                    if let Some(r) = &rappel {
                        r();
                    }
                }
            }
        }),
    };
    let vue = o.vues.ouvrir(fenetre, options).map_err(|e| e.to_string())?;
    avec(|e| {
        e.ouvertes.insert(
            place.to_string(),
            Ouverte {
                vue,
                lien,
                montree: false,
            },
        )
    });
    surveiller();
    actualiser(place);
    Ok(())
}

fn pomper(f: &Principale, place: &str, lien: Option<&LienVue>, rappel: RappelLien) -> bool {
    let actuelle = avec(|e| e.ouvertes.get(place).and_then(|o| o.lien.clone())).flatten();
    let Some(lien) = lien.filter(|l| !l.ferme()) else {
        if actuelle.is_some() {
            fermer(place);
        }
        return false;
    };
    if let Some(page) = lien.page_a_ouvrir() {
        if let Err(e) = ouvrir(f, place, &page, Some(lien.clone()), Some(rappel.clone())) {
            journal(&format!("lecteur YouTube ({place}) : {e}"));
            lien.signaler_indisponible(&e);
            rappel();
        }
    } else if actuelle.is_some_and(|a| !a.meme(lien)) {
        fermer(place);
    }
    let messages = lien.sortants();
    avec(|e| {
        if let Some(o) = e.ouvertes.get(place) {
            for m in &messages {
                let _ = o.vue.executer(&script_envoi(m));
            }
        }
    });
    actualiser(place);
    avec(|e| e.ouvertes.contains_key(place)).unwrap_or(false)
}

pub fn pomper_seance(f: &Principale, lien: Option<&LienVue>, rappel: RappelLien) {
    if EN_OUVERTURE.with(Cell::get) {
        return;
    }
    let ouverte = pomper(f, PLACE_SEANCE, lien, rappel);
    let g = f.global::<DonneesVueWeb>();
    if g.get_seance() != ouverte {
        g.set_seance(ouverte);
    }
}

pub fn pomper_solo(f: &Principale, lien: Option<&LienVue>, rappel: RappelLien) {
    if EN_OUVERTURE.with(Cell::get) {
        return;
    }
    let ouverte = pomper(f, PLACE_SOLO, lien, rappel);
    let g = f.global::<DonneesVueWeb>();
    if g.get_solo() != ouverte {
        g.set_solo(ouverte);
    }
}

pub fn pomper_musique(f: &Principale, lien: Option<&LienVue>, rappel: RappelLien) {
    if EN_OUVERTURE.with(Cell::get) {
        return;
    }
    let ouverte = pomper(f, PLACE_MUSIQUE, lien, rappel);
    let g = f.global::<DonneesVueWeb>();
    if g.get_musique() != ouverte {
        g.set_musique(ouverte);
    }
}

pub fn ouvrir_visionneuse(f: &Principale, video: &VideoYoutube) -> bool {
    if !disponible() {
        return false;
    }
    let page = PageLecteur::visionneuse(video);
    if let Err(e) = ouvrir(f, PLACE_VISIONNEUSE, &page, None, None) {
        journal(&format!("visionneuse YouTube : {e}"));
        return false;
    }
    avec(|e| {
        e.adresse_visionneuse =
            adresse_navigateur("youtube", &video.id, video.debut as f64).unwrap_or_default();
    });
    f.global::<DonneesVueWeb>()
        .set_visionneuse(TITRE_VISIONNEUSE.into());
    true
}

pub fn fermer_visionneuse(f: &Principale) {
    fermer(PLACE_VISIONNEUSE);
    f.global::<DonneesVueWeb>().set_visionneuse("".into());
}
