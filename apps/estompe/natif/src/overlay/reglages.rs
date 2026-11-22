use std::rc::Rc;
use std::sync::Arc;

use estompe_client::overlay::{
    CoinOverlay, ModeOverlay, QuandOverlay, ReglagesOverlay, TailleOverlay,
};
use estompe_client::overlay_messages::{GenresMessages, ReglagesMessages};
use estompe_plateforme::libelle_touche;
use slint::ComponentHandle;

use super::messages::ServiceOverlayMessages;
use super::voix::ServiceOverlayVoix;
use crate::session::Memoire;
use crate::vocal::touches::code_de_texte;
use crate::{DonneesOverlay, DonneesOverlayMessages, Principale};

pub fn afficher(f: &Principale, reglages: &ReglagesOverlay) {
    let g = f.global::<DonneesOverlay>();
    g.set_actif(reglages.quand != QuandOverlay::Jamais);
    g.set_en_jeu(reglages.quand != QuandOverlay::Toujours);
    g.set_coin(reglages.coin.code().into());
    g.set_taille(
        TailleOverlay::TOUTES
            .iter()
            .position(|t| *t == reglages.taille)
            .unwrap_or(1) as i32,
    );
    g.set_parlent(reglages.mode == ModeOverlay::Parlent);
    g.set_opacite(i32::from(reglages.opacite));
}

pub fn quand(actif: bool, en_jeu: bool) -> QuandOverlay {
    match (actif, en_jeu) {
        (false, _) => QuandOverlay::Jamais,
        (true, true) => QuandOverlay::Jeux,
        (true, false) => QuandOverlay::Toujours,
    }
}

struct Reglages {
    service: Arc<ServiceOverlayVoix>,
    memoire: Memoire,
}

impl Reglages {
    fn modifier(&self, f: &Principale, modification: impl FnOnce(&mut ReglagesOverlay)) {
        self.memoire.modifier(|p| {
            modification(&mut p.overlay);
            p.overlay = p.overlay.borne();
        });
        let reglages = self.memoire.lire(|p| p.overlay);
        afficher(f, &reglages);
        self.service.definir_reglages(reglages);
    }
}

pub fn brancher(f: &Principale, service: Arc<ServiceOverlayVoix>, memoire: Memoire) {
    let g = f.global::<DonneesOverlay>();
    g.set_disponible(service.disponible());
    afficher(f, &memoire.lire(|p| p.overlay));
    let r = Rc::new(Reglages { service, memoire });
    macro_rules! rappel {
        ($methode:ident, |$r:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $r = r.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_definir_actif, |r, f, actif| r.modifier(&f, |o| {
        o.quand = quand(actif, o.quand != QuandOverlay::Toujours);
    }));
    rappel!(on_definir_en_jeu, |r, f, en_jeu| r.modifier(&f, |o| {
        o.quand = quand(o.quand != QuandOverlay::Jamais, en_jeu);
    }));
    rappel!(on_definir_coin, |r, f, code| {
        if let Some(coin) = CoinOverlay::de_code(&code) {
            r.modifier(&f, |o| o.coin = coin);
        }
    });
    rappel!(on_definir_taille, |r, f, rang| {
        if let Some(taille) = usize::try_from(rang)
            .ok()
            .and_then(|i| TailleOverlay::TOUTES.get(i))
        {
            r.modifier(&f, |o| o.taille = *taille);
        }
    });
    rappel!(on_definir_parlent, |r, f, parlent| r.modifier(&f, |o| {
        o.mode = if parlent {
            ModeOverlay::Parlent
        } else {
            ModeOverlay::Tous
        };
    }));
    rappel!(on_definir_opacite, |r, f, valeur, fini| {
        let opacite = valeur.round().clamp(0.0, 100.0) as u8;
        if fini {
            r.modifier(&f, |o| o.opacite = opacite);
        } else {
            let reglages = ReglagesOverlay {
                opacite,
                ..r.memoire.lire(|p| p.overlay)
            }
            .borne();
            r.service.definir_reglages(reglages);
        }
    });
}

pub fn afficher_messages(f: &Principale, reglages: &ReglagesMessages) {
    let g = f.global::<DonneesOverlayMessages>();
    g.set_genres(
        GenresMessages::TOUS
            .iter()
            .position(|x| *x == reglages.genres)
            .unwrap_or(0) as i32,
    );
    g.set_en_jeu(reglages.en_jeu);
    g.set_coin(reglages.coin.code().into());
    g.set_duree(i32::from(reglages.duree));
    g.set_masquer(reglages.masquer_contenu);
    g.set_sourdine(reglages.sourdine_ne_pas_deranger);
    g.set_touche_reponse(libelle_touche(&reglages.touche_reponse).into());
    g.set_touche_ouvrir(libelle_touche(&reglages.touche_ouvrir).into());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoixTouche {
    Retenue(String),
    Refusee(&'static str),
}

pub const TOUCHE_INVALIDE: &str = "Cette touche ne peut pas servir : choisis une lettre, un chiffre, une touche de fonction ou une touche de modification.";
pub const TOUCHE_PRISE: &str =
    "Cette touche sert déjà (appui pour parler ou autre raccourci de l'overlay).";

pub fn choisir_touche(texte: &str, autre: &str, prises: &[String]) -> ChoixTouche {
    match code_de_texte(texte) {
        None => ChoixTouche::Refusee(TOUCHE_INVALIDE),
        Some(code) if code == autre || prises.contains(&code) => ChoixTouche::Refusee(TOUCHE_PRISE),
        Some(code) => ChoixTouche::Retenue(code),
    }
}

struct ReglagesMsg {
    service: Arc<ServiceOverlayMessages>,
    memoire: Memoire,
}

impl ReglagesMsg {
    fn modifier(&self, f: &Principale, modification: impl FnOnce(&mut ReglagesMessages)) {
        self.memoire.modifier(|p| {
            modification(&mut p.overlay_messages);
            p.overlay_messages = p.overlay_messages.clone().borne();
        });
        let reglages = self.memoire.lire(|p| p.overlay_messages.clone());
        afficher_messages(f, &reglages);
        self.service.definir_reglages(reglages);
    }

    fn touche(&self, f: &Principale, rang: i32, texte: &str) -> bool {
        let actuels = self.memoire.lire(|p| p.overlay_messages.clone());
        let prises = self.memoire.lire(|p| {
            if p.ptt.actif {
                vec![p.ptt.touche.clone()]
            } else {
                Vec::new()
            }
        });
        let autre = if rang == 0 {
            &actuels.touche_ouvrir
        } else {
            &actuels.touche_reponse
        };
        let g = f.global::<DonneesOverlayMessages>();
        match choisir_touche(texte, autre, &prises) {
            ChoixTouche::Refusee(raison) => {
                g.set_erreur(raison.into());
                false
            }
            ChoixTouche::Retenue(code) => {
                g.set_erreur("".into());
                self.modifier(f, |r| {
                    if rang == 0 {
                        r.touche_reponse = code;
                    } else {
                        r.touche_ouvrir = code;
                    }
                });
                true
            }
        }
    }
}

pub fn brancher_messages(f: &Principale, service: Arc<ServiceOverlayMessages>, memoire: Memoire) {
    let g = f.global::<DonneesOverlayMessages>();
    g.set_disponible(service.disponible());
    g.set_saisie_disponible(service.saisie_disponible());
    afficher_messages(f, &memoire.lire(|p| p.overlay_messages.clone()));
    let r = Rc::new(ReglagesMsg { service, memoire });
    macro_rules! rappel {
        ($methode:ident, |$r:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $r = r.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_definir_genres, |r, f, rang| {
        if let Some(genres) = usize::try_from(rang)
            .ok()
            .and_then(|i| GenresMessages::TOUS.get(i))
        {
            r.modifier(&f, |o| o.genres = *genres);
        }
    });
    rappel!(on_definir_en_jeu, |r, f, en_jeu| r
        .modifier(&f, |o| o.en_jeu = en_jeu));
    rappel!(on_definir_coin, |r, f, code| {
        if let Some(coin) = CoinOverlay::de_code(&code) {
            r.modifier(&f, |o| o.coin = coin);
        }
    });
    rappel!(on_definir_duree, |r, f, valeur, fini| {
        if fini {
            let duree = valeur.round().clamp(0.0, 255.0) as u8;
            r.modifier(&f, |o| o.duree = duree);
        }
    });
    rappel!(on_definir_masquer, |r, f, masquer| r
        .modifier(&f, |o| o.masquer_contenu = masquer));
    rappel!(on_definir_sourdine, |r, f, sourdine| r
        .modifier(&f, |o| o.sourdine_ne_pas_deranger = sourdine));
    let faible = f.as_weak();
    g.on_touche_choisie(move |rang, texte| {
        faible.upgrade().is_some_and(|f| r.touche(&f, rang, &texte))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_touche_de_l_overlay_ne_reprend_ni_l_autre_ni_celle_pour_parler() {
        assert_eq!(
            choisir_touche("r", "F7", &[]),
            ChoixTouche::Retenue("KeyR".into())
        );
        assert_eq!(
            choisir_touche("r", "KeyR", &[]),
            ChoixTouche::Refusee(TOUCHE_PRISE)
        );
        assert_eq!(
            choisir_touche("v", "F7", &["KeyV".into()]),
            ChoixTouche::Refusee(TOUCHE_PRISE)
        );
        assert_eq!(
            choisir_touche("ab", "F7", &[]),
            ChoixTouche::Refusee(TOUCHE_INVALIDE)
        );
    }

    #[test]
    fn les_deux_interrupteurs_donnent_le_moment_d_affichage() {
        assert_eq!(quand(false, true), QuandOverlay::Jamais);
        assert_eq!(quand(false, false), QuandOverlay::Jamais);
        assert_eq!(quand(true, true), QuandOverlay::Jeux);
        assert_eq!(quand(true, false), QuandOverlay::Toujours);
    }
}
