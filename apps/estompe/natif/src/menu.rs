use std::cell::RefCell;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{DonneesCopie, DonneesMenu, EntreeMenu, Principale};

pub const MARGE_FENETRE: f32 = 8.0;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entree {
    pub id: String,
    pub libelle: String,
    pub icone: String,
    pub danger: bool,
    pub separe: bool,
    pub bientot: bool,
    pub coche: bool,
    pub raccourci: String,
}

impl Entree {
    pub fn nouvelle(id: &str, libelle: &str, icone: &str) -> Self {
        Self {
            id: id.to_string(),
            libelle: libelle.to_string(),
            icone: icone.to_string(),
            ..Self::default()
        }
    }

    pub fn danger(self) -> Self {
        Self {
            danger: true,
            ..self
        }
    }

    pub fn separee(self) -> Self {
        Self {
            separe: true,
            ..self
        }
    }

    pub fn bientot(self) -> Self {
        Self {
            bientot: true,
            ..self
        }
    }

    pub fn cochee(self, coche: bool) -> Self {
        Self { coche, ..self }
    }

    pub fn raccourci(self, raccourci: &str) -> Self {
        Self {
            raccourci: raccourci.to_string(),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ancre {
    Point { x: f32, y: f32 },
    Element { x: f32, haut: f32, bas: f32 },
}

pub fn placer(
    x: f32,
    y: f32,
    largeur: f32,
    hauteur: f32,
    vue: (f32, f32),
    marge: f32,
) -> (f32, f32) {
    (
        marge.max(x.min(vue.0 - largeur - marge)),
        marge.max(y.min(vue.1 - hauteur - marge)),
    )
}

pub fn retourner(y: f32, hauteur: f32, vue: f32, haut: Option<f32>, marge: f32) -> f32 {
    match haut {
        Some(haut) if y + hauteur > vue - marge && haut - hauteur >= marge => haut - hauteur,
        _ => y,
    }
}

pub fn position(ancre: Ancre, largeur: f32, hauteur: f32, vue: (f32, f32)) -> (f32, f32) {
    let (x, y) = match ancre {
        Ancre::Point { x, y } => (x, y),
        Ancre::Element { x, haut, bas } => {
            (x, retourner(bas, hauteur, vue.1, Some(haut), MARGE_FENETRE))
        }
    };
    placer(x, y, largeur, hauteur, vue, MARGE_FENETRE)
}

pub fn hauteur(entrees: &[Entree], entree: f32, separation: f32, marge: f32) -> f32 {
    let separations = entrees.iter().filter(|e| e.separe).count() as f32;
    2.0 * marge + entrees.len() as f32 * entree + separations * separation
}

type Action = Box<dyn FnOnce(&Principale, &str) -> bool>;

thread_local! {
    static ACTION: RefCell<Option<Action>> = RefCell::new(None);
}

pub fn ouvrir(
    f: &Principale,
    titre: &str,
    ancre: Ancre,
    entrees: Vec<Entree>,
    action: impl FnOnce(&Principale, &str) -> bool + 'static,
) -> bool {
    if entrees.is_empty() {
        return false;
    }
    let g = f.global::<DonneesMenu>();
    let taille = f.window().size().to_logical(f.window().scale_factor());
    let haut = hauteur(
        &entrees,
        g.get_hauteur_entree(),
        g.get_hauteur_separation(),
        g.get_marge(),
    );
    let (x, y) = position(ancre, g.get_largeur(), haut, (taille.width, taille.height));
    ACTION.with(|a| *a.borrow_mut() = Some(Box::new(action)));
    g.set_titre(titre.into());
    g.set_entrees(ModelRc::new(VecModel::from(
        entrees
            .into_iter()
            .map(|e| EntreeMenu {
                id: e.id.into(),
                libelle: e.libelle.into(),
                icone: e.icone.into(),
                danger: e.danger,
                separe: e.separe,
                bientot: e.bientot,
                coche: e.coche,
                raccourci: e.raccourci.into(),
            })
            .collect::<Vec<_>>(),
    )));
    g.set_x(x);
    g.set_y(y);
    g.set_focus_ailleurs(false);
    g.set_ouvert(true);
    g.set_ouvertures(g.get_ouvertures().wrapping_add(1));
    true
}

pub fn choisir(f: &Principale, id: &str) {
    let Some(action) = ACTION.with(|a| a.borrow_mut().take()) else {
        return;
    };
    let ailleurs = action(f, id);
    if ACTION.with(|a| a.borrow().is_some()) {
        return;
    }
    let g = f.global::<DonneesMenu>();
    g.set_focus_ailleurs(ailleurs);
    g.set_ouvert(false);
}

pub fn fermer(f: &Principale) {
    ACTION.with(|a| a.borrow_mut().take());
    let g = f.global::<DonneesMenu>();
    if g.get_ouvert() {
        g.set_focus_ailleurs(false);
        g.set_ouvert(false);
    }
}

pub fn copier_texte(f: &Principale, texte: &str) {
    let g = f.global::<DonneesCopie>();
    g.set_texte(texte.into());
    g.set_demandes(g.get_demandes().wrapping_add(1));
}

pub fn brancher(f: &Principale) {
    let g = f.global::<DonneesMenu>();
    let faible = f.as_weak();
    g.on_choisir(move |id| {
        if let Some(f) = faible.upgrade() {
            choisir(&f, &id);
        }
    });
    let faible = f.as_weak();
    g.on_fermer(move || {
        if let Some(f) = faible.upgrade() {
            fermer(&f);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s_ouvre_au_curseur_quand_il_y_a_de_la_place() {
        assert_eq!(
            placer(100.0, 100.0, 200.0, 150.0, (800.0, 600.0), 8.0),
            (100.0, 100.0)
        );
    }

    #[test]
    fn se_replie_a_gauche_et_en_haut_pres_des_bords() {
        assert_eq!(
            placer(790.0, 590.0, 200.0, 150.0, (800.0, 600.0), 8.0),
            (592.0, 442.0)
        );
    }

    #[test]
    fn reste_sous_le_bouton_quand_il_y_a_de_la_place() {
        assert_eq!(retourner(100.0, 150.0, 600.0, Some(80.0), 8.0), 100.0);
    }

    #[test]
    fn se_retourne_au_dessus_du_bouton_en_bas_de_la_fenetre() {
        assert_eq!(retourner(560.0, 150.0, 600.0, Some(540.0), 8.0), 390.0);
        assert_eq!(
            position(
                Ancre::Element {
                    x: 700.0,
                    haut: 540.0,
                    bas: 560.0
                },
                200.0,
                150.0,
                (800.0, 600.0)
            ),
            (592.0, 390.0)
        );
    }

    #[test]
    fn reste_en_place_sans_ancre_ou_si_le_haut_manque_aussi() {
        assert_eq!(retourner(560.0, 150.0, 600.0, None, 8.0), 560.0);
        assert_eq!(retourner(300.0, 400.0, 500.0, Some(100.0), 8.0), 300.0);
    }

    #[test]
    fn la_hauteur_compte_les_entrees_et_les_separations() {
        let entrees = [
            Entree::nouvelle("a", "A", ""),
            Entree::nouvelle("b", "B", "").separee().danger(),
        ];
        assert_eq!(hauteur(&entrees, 32.0, 9.0, 4.0), 81.0);
        assert!(entrees[1].danger && !entrees[0].separe);
        let statut = Entree::nouvelle("absent", "Absent", "lune")
            .cochee(true)
            .raccourci("Ctrl+K");
        assert!(statut.coche && statut.raccourci == "Ctrl+K");
    }
}
