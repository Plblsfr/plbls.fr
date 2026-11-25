use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use estompe_client::lecteur::mpv::OptionsMpv;
use estompe_client::lecteur::{
    libelle_vitesse, peripherique_mpv, ErreurLecteur, EtatLecteur, GenreMedia, GenrePiste, Lecteur,
    LectureUnique, Origine, Phase, Presence, Source, MESSAGE_ERREUR, VITESSES,
};
use slint::{ComponentHandle, Image};

use super::{Afficheur, Livraison, Pixels};
use crate::menu::{self, Ancre, Entree};
use crate::{DonneesLecteur, EtatLecteurVue, Principale};

const CLE: &str = "lecteur";
const HAUTEUR_BOUTON: f32 = 32.0;

pub fn lecteur_du_systeme(sortie: Option<&str>) -> Result<Lecteur, ErreurLecteur> {
    Lecteur::mpv(OptionsMpv {
        sortie: Some(peripherique_mpv(sortie)),
        ..OptionsMpv::default()
    })
}

pub fn vue(etat: &EtatLecteur, image: Image, image_prete: bool) -> EtatLecteurVue {
    EtatLecteurVue {
        image,
        image_prete,
        video: etat.a_de_la_video(),
        chargement: etat.en_attente(),
        en_pause: etat.en_pause,
        fin: etat.fin,
        position: etat.position as f32,
        duree: etat.duree.unwrap_or(0.0) as f32,
        temps: etat.temps().into(),
        volume: i32::from(etat.volume),
        muet: etat.muet,
        vitesse: libelle_vitesse(etat.vitesse).into(),
        plein_ecran: etat.plein_ecran,
        erreur: if etat.phase == Phase::Erreur {
            MESSAGE_ERREUR.into()
        } else {
            Default::default()
        },
        pistes_audio: etat.pistes_de(GenrePiste::Audio).count() as i32,
        pistes_sous_titres: etat.pistes_de(GenrePiste::SousTitres).count() as i32,
        sous_titres_actifs: etat.sous_titres.is_some(),
    }
}

fn appliquer(f: &Principale, etat: &EtatLecteur, plein_applique: &Mutex<bool>) {
    let g = f.global::<DonneesLecteur>();
    let actuel = g.get_etat();
    g.set_etat(vue(etat, actuel.image, actuel.image_prete));
    g.set_avance(etat.cache_jusqua.unwrap_or(0.0) as f32);
    let mut plein = plein_applique.lock().unwrap_or_else(|e| e.into_inner());
    if *plein != etat.plein_ecran {
        *plein = etat.plein_ecran;
        f.window().set_fullscreen(etat.plein_ecran);
    }
}

fn poser_image(f: &Principale, pixels: Option<Pixels>) {
    let g = f.global::<DonneesLecteur>();
    let mut etat = g.get_etat();
    match pixels {
        Some(p) => {
            etat.image = Image::from_rgba8(p);
            etat.image_prete = true;
        }
        None => {
            etat.image = Image::default();
            etat.image_prete = false;
        }
    }
    g.set_etat(etat);
}

fn livraison(fenetre: slint::Weak<Principale>) -> Livraison {
    Arc::new(move |_, pixels, en_vol| {
        let liberer = en_vol.clone();
        let resultat = fenetre.upgrade_in_event_loop(move |f| {
            liberer.store(false, Ordering::Release);
            poser_image(&f, pixels);
        });
        if resultat.is_err() {
            en_vol.store(false, Ordering::Release);
        }
    })
}

fn suivre(lecteur: &Arc<Lecteur>, fenetre: slint::Weak<Principale>) {
    let mut suivi = lecteur.observer();
    let faible = Arc::downgrade(lecteur);
    let plein = Arc::new(Mutex::new(false));
    let en_vol = Arc::new(AtomicBool::new(false));
    let _ = std::thread::Builder::new()
        .name("lecteur etat".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().build() else {
                return;
            };
            rt.block_on(async move {
                while suivi.changed().await.is_ok() {
                    suivi.borrow_and_update();
                    if en_vol.swap(true, Ordering::AcqRel) {
                        continue;
                    }
                    let (faible, plein, liberer) = (faible.clone(), plein.clone(), en_vol.clone());
                    let resultat = fenetre.upgrade_in_event_loop(move |f| {
                        liberer.store(false, Ordering::Release);
                        if let Some(lecteur) = faible.upgrade() {
                            appliquer(&f, &lecteur.etat(), &plein);
                        }
                    });
                    if resultat.is_err() {
                        break;
                    }
                }
            });
        });
}

fn entrees_pistes(etat: &EtatLecteur, genre: GenrePiste, aucune: bool) -> Vec<Entree> {
    let choisie = match genre {
        GenrePiste::Audio => etat.audio,
        _ => etat.sous_titres,
    };
    let mut entrees: Vec<Entree> = Vec::new();
    if aucune {
        entrees.push(Entree::nouvelle("aucune", "Aucun", "").cochee(choisie.is_none()));
    }
    entrees.extend(etat.pistes_de(genre).map(|p| {
        Entree::nouvelle(&p.id.to_string(), &p.libelle(), "").cochee(choisie == Some(p.id))
    }));
    entrees
}

fn ancre(x: f32, y: f32) -> Ancre {
    Ancre::Element {
        x,
        haut: y - HAUTEUR_BOUTON,
        bas: y,
    }
}

pub struct GestionLecteur {
    lecture: Arc<LectureUnique>,
    fenetre: slint::Weak<Principale>,
    _afficheur: Afficheur,
}

impl GestionLecteur {
    pub fn nouvelle(f: &Principale, lecteur: Lecteur) -> Self {
        let lecture = Arc::new(LectureUnique::nouvelle(Arc::new(lecteur)));
        let fenetre = f.as_weak();
        brancher(f, &lecture);
        suivre(lecture.lecteur(), fenetre.clone());
        let afficheur = Afficheur::demarrer(
            CLE.to_string(),
            lecture.lecteur().images(),
            (0, 0),
            false,
            Arc::new(AtomicU64::new(0)),
            livraison(fenetre.clone()),
        );
        Self {
            lecture,
            fenetre,
            _afficheur: afficheur,
        }
    }

    pub fn lecteur(&self) -> &Arc<Lecteur> {
        self.lecture.lecteur()
    }

    pub fn lecture(&self) -> &Arc<LectureUnique> {
        &self.lecture
    }

    pub fn ouvrir(&self, titre: &str, source: Source) {
        if let Some(f) = self.fenetre.upgrade() {
            poser_image(&f, None);
        }
        self.lecture.lancer(
            Origine::Autre(titre.to_string()),
            GenreMedia::Video,
            true,
            source,
        );
        if let Some(f) = self.fenetre.upgrade() {
            let g = f.global::<DonneesLecteur>();
            g.set_titre(titre.into());
            g.set_telechargeable(false);
            g.set_ouvert(true);
            afficher_piece(&f, &self.lecture);
        }
    }

    pub fn lire_piece(&self, message: &str, piece: &str, genre: GenreMedia, source: Source) {
        let nouvelle = self.lecture.piece_active().as_deref() != Some(piece);
        if let (true, Some(f)) = (nouvelle, self.fenetre.upgrade()) {
            poser_image(&f, None);
        }
        self.lecture.lire_piece(message, piece, genre, source);
        if let Some(f) = self.fenetre.upgrade() {
            afficher_piece(&f, &self.lecture);
        }
    }

    pub fn agrandir(&self, titre: &str, piece: &str, plein: bool) -> bool {
        if !self.lecture.agrandir(piece) {
            return false;
        }
        if let Some(f) = self.fenetre.upgrade() {
            let g = f.global::<DonneesLecteur>();
            g.set_titre(titre.into());
            g.set_telechargeable(true);
            g.set_ouvert(true);
        }
        if plein {
            self.lecture.lecteur().definir_plein_ecran(true);
        }
        true
    }

    pub fn suivre(&self, presence: impl Fn(&str) -> Presence) -> bool {
        let arrete = self.lecture.suivre(presence);
        if arrete {
            if let Some(f) = self.fenetre.upgrade() {
                afficher_piece(&f, &self.lecture);
            }
        }
        arrete
    }

    pub fn arreter(&self) {
        if let Some(f) = self.fenetre.upgrade() {
            if f.global::<DonneesLecteur>().get_ouvert() {
                fermer(&f, &self.lecture);
                return;
            }
            self.lecture.arreter();
            afficher_piece(&f, &self.lecture);
        } else {
            self.lecture.arreter();
        }
    }

    pub fn fermer(&self) {
        if let Some(f) = self.fenetre.upgrade() {
            fermer(&f, &self.lecture);
        }
    }
}

fn afficher_piece(f: &Principale, lecture: &LectureUnique) {
    f.global::<DonneesLecteur>()
        .set_piece(lecture.piece_active().unwrap_or_default().into());
}

fn fermer(f: &Principale, lecture: &LectureUnique) {
    if !lecture.arreter() {
        lecture.lecteur().arreter();
        lecture.lecteur().definir_plein_ecran(false);
        lecture.lecteur().definir_visible(false);
    }
    let g = f.global::<DonneesLecteur>();
    g.set_ouvert(false);
    g.set_telechargeable(false);
    afficher_piece(f, lecture);
}

fn avec(lecteur: &Weak<Lecteur>, action: impl FnOnce(&Lecteur)) {
    if let Some(l) = lecteur.upgrade() {
        action(&l);
    }
}

fn brancher(f: &Principale, lecture: &Arc<LectureUnique>) {
    let lecteur = lecture.lecteur();
    let g = f.global::<DonneesLecteur>();
    let l = Arc::downgrade(lecteur);
    g.on_basculer(move || avec(&l, Lecteur::basculer));
    let l = Arc::downgrade(lecteur);
    g.on_aller(move |position| avec(&l, |l| l.aller(f64::from(position))));
    let l = Arc::downgrade(lecteur);
    g.on_volume(move |volume| avec(&l, |l| l.definir_volume(volume.clamp(0, 100) as u8)));
    let l = Arc::downgrade(lecteur);
    g.on_muet(move || avec(&l, Lecteur::basculer_muet));
    let l = Arc::downgrade(lecteur);
    g.on_plein_ecran(move || avec(&l, Lecteur::basculer_plein_ecran));
    let l = Arc::downgrade(lecteur);
    g.on_touche(move |texte, ctrl, meta, alt| {
        l.upgrade()
            .is_some_and(|l| l.touche(&texte, ctrl, meta, alt))
    });
    let (l, fenetre) = (Arc::downgrade(lecteur), f.as_weak());
    g.on_zone_image(move |largeur, hauteur| {
        let echelle = fenetre.upgrade().map_or(1.0, |f| f.window().scale_factor());
        let cote = |c: f32| (c * echelle).round().max(0.0) as u32;
        avec(&l, |l| l.definir_zone((cote(largeur), cote(hauteur))));
    });
    let (l, fenetre) = (Arc::downgrade(lecture), f.as_weak());
    g.on_fermer(move || {
        if let (Some(f), Some(l)) = (fenetre.upgrade(), l.upgrade()) {
            fermer(&f, &l);
        }
    });
    let (l, fenetre) = (Arc::downgrade(lecteur), f.as_weak());
    g.on_menu_vitesse(move |x, y| {
        let (Some(f), Some(lecteur)) = (fenetre.upgrade(), l.upgrade()) else {
            return;
        };
        let actuelle = lecteur.etat().vitesse;
        let entrees = VITESSES
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Entree::nouvelle(&i.to_string(), &libelle_vitesse(*v), "")
                    .cochee((actuelle - v).abs() < 1e-6)
            })
            .collect();
        let l = l.clone();
        menu::ouvrir(
            &f,
            "Vitesse de lecture",
            ancre(x, y),
            entrees,
            move |_, id| {
                let vitesse = id.parse::<usize>().ok().and_then(|i| VITESSES.get(i));
                if let (Some(v), Some(l)) = (vitesse, l.upgrade()) {
                    l.definir_vitesse(*v);
                }
                false
            },
        );
    });
    let (l, fenetre) = (Arc::downgrade(lecteur), f.as_weak());
    g.on_menu_audio(move |x, y| {
        let (Some(f), Some(lecteur)) = (fenetre.upgrade(), l.upgrade()) else {
            return;
        };
        let entrees = entrees_pistes(&lecteur.etat(), GenrePiste::Audio, false);
        let l = l.clone();
        menu::ouvrir(&f, "Piste audio", ancre(x, y), entrees, move |_, id| {
            if let (Ok(piste), Some(l)) = (id.parse::<i64>(), l.upgrade()) {
                l.choisir_audio(Some(piste));
            }
            false
        });
    });
    let (l, fenetre) = (Arc::downgrade(lecteur), f.as_weak());
    g.on_menu_sous_titres(move |x, y| {
        let (Some(f), Some(lecteur)) = (fenetre.upgrade(), l.upgrade()) else {
            return;
        };
        let entrees = entrees_pistes(&lecteur.etat(), GenrePiste::SousTitres, true);
        let l = l.clone();
        menu::ouvrir(&f, "Sous-titres", ancre(x, y), entrees, move |_, id| {
            if let Some(l) = l.upgrade() {
                l.choisir_sous_titres(id.parse::<i64>().ok());
            }
            false
        });
    });
}

#[cfg(test)]
mod tests {
    use estompe_client::lecteur::{Evenement, MediaFactice};

    use super::*;

    #[test]
    fn vue_d_un_lecteur_charge_puis_en_erreur() {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media("v.mp4", MediaFactice::video(65.0));
        lecteur.charger(Source::adresse("v.mp4").en_pause());
        moteur.emettre(Evenement::Vitesse(1.5));
        let v = vue(&lecteur.etat(), Image::default(), false);
        assert!(v.video && v.en_pause && !v.chargement);
        assert_eq!(v.temps.as_str(), "0 s / 1:05");
        assert_eq!(v.vitesse.as_str(), "1,5×");
        assert_eq!((v.pistes_audio, v.pistes_sous_titres), (2, 1));
        assert!(!v.sous_titres_actifs && v.erreur.is_empty());
        let entrees = entrees_pistes(&lecteur.etat(), GenrePiste::SousTitres, true);
        assert_eq!(entrees.len(), 2);
        assert!(entrees[0].coche && !entrees[1].coche);
        assert_eq!(entrees[1].libelle, "Français");
        moteur.ajouter_echec("x.mp4", "HTTP 401");
        lecteur.charger(Source::adresse("x.mp4"));
        let v = vue(&lecteur.etat(), Image::default(), false);
        assert_eq!(v.erreur.as_str(), MESSAGE_ERREUR);
        assert!(!v.chargement);
    }
}
