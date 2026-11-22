use std::rc::Rc;
use std::sync::Arc;

use estompe_client::overlay::{
    CoinOverlay, ModeOverlay, QuandOverlay, ReglagesOverlay, TailleOverlay,
};
use slint::ComponentHandle;

use super::voix::ServiceOverlayVoix;
use crate::session::Memoire;
use crate::{DonneesOverlay, Principale};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_deux_interrupteurs_donnent_le_moment_d_affichage() {
        assert_eq!(quand(false, true), QuandOverlay::Jamais);
        assert_eq!(quand(false, false), QuandOverlay::Jamais);
        assert_eq!(quand(true, true), QuandOverlay::Jeux);
        assert_eq!(quand(true, false), QuandOverlay::Toujours);
    }
}
