use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use estompe_client::journal::{filtrer, formater, lire_entrees, nom_d_export, Journal, Niveau};
use estompe_client::parametres::{
    annonce_alpha, chercher, onglet_a_ouvrir, raccourcis_fixes, raccourcis_globaux, voisin,
    DeblocageAlpha, Onglet, Raccourci, ONGLETS,
};
use estompe_client::preferences::{Densite, Preferences};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::messagerie::pont::Fuseau;
use crate::session::{theme_des_preferences, Memoire};
use crate::{
    DonneesCoquille, DonneesOverlayMessages, DonneesParametres, DonneesReplay, LigneJournal,
    LigneRaccourci, OngletParametres, Principale,
};

pub const LIGNES_AFFICHEES: usize = 500;

pub type Horloge = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct OutilsParametres {
    pub memoire: Memoire,
    pub journal: Arc<Journal>,
    pub fuseau: Fuseau,
    pub horloge: Horloge,
    pub dossier_export: PathBuf,
}

pub struct Parametres {
    outils: OutilsParametres,
    deblocage: RefCell<DeblocageAlpha>,
}

fn niveau_du_rang(rang: i32) -> Option<Niveau> {
    match rang {
        1 => Some(Niveau::Info),
        2 => Some(Niveau::Avertissement),
        3 => Some(Niveau::Erreur),
        _ => None,
    }
}

fn lignes_de_raccourcis(liste: Vec<Raccourci>) -> ModelRc<LigneRaccourci> {
    ModelRc::new(VecModel::from(
        liste
            .into_iter()
            .map(|r| LigneRaccourci {
                action: r.action.into(),
                touches: r.touches.into(),
                reglage: r.reglage.into(),
            })
            .collect::<Vec<_>>(),
    ))
}

pub fn onglets_visibles(f: &Principale) -> Vec<Onglet> {
    let replay = f.global::<DonneesReplay>().get_disponible();
    ONGLETS
        .into_iter()
        .filter(|o| *o != Onglet::Replay || replay)
        .collect()
}

pub fn afficher_onglets(f: &Principale) {
    let g = f.global::<DonneesParametres>();
    let trouves = chercher(&g.get_recherche(), &onglets_visibles(f));
    g.set_onglets(ModelRc::new(VecModel::from(
        trouves
            .into_iter()
            .map(|t| OngletParametres {
                id: t.onglet.id().into(),
                libelle: t.onglet.libelle().into(),
                icone: t.onglet.icone().into(),
                badge: t.onglet.badge().into(),
                trouves: t
                    .trouves
                    .iter()
                    .take(3)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" · ")
                    .into(),
            })
            .collect::<Vec<_>>(),
    )));
}

pub fn afficher_preferences(f: &Principale, p: &Preferences) {
    let g = f.global::<DonneesParametres>();
    g.set_apercus(p.apercus);
    g.set_journal_actif(p.journal_debogage);
    g.set_raccourcis(lignes_de_raccourcis(raccourcis_fixes()));
    g.set_touches_globales(lignes_de_raccourcis(raccourcis_globaux(
        p,
        f.global::<DonneesOverlayMessages>().get_disponible(),
        f.global::<DonneesReplay>().get_disponible(),
    )));
}

fn charger_signalements(f: &Principale, onglet: Onglet) {
    if onglet == Onglet::Signalements {
        f.global::<crate::DonneesSignalements>()
            .invoke_charger_mes();
    }
}

pub fn poser_onglet(f: &Principale, onglet: Onglet) {
    let g = f.global::<DonneesParametres>();
    g.set_onglet(onglet.id().into());
    g.set_titre(onglet.libelle().into());
}

impl Parametres {
    pub fn ouvrir(&self, f: &Principale, demande: &str) {
        let g = f.global::<DonneesParametres>();
        g.set_recherche(SharedString::new());
        let retenu = self.outils.memoire.lire(|p| p.onglet_parametres.clone());
        let onglet = onglet_a_ouvrir(demande, &retenu, &onglets_visibles(f));
        afficher_onglets(f);
        self.outils.memoire.lire(|p| afficher_preferences(f, p));
        poser_onglet(f, onglet);
        g.set_annonce(SharedString::new());
        g.set_message_journal(SharedString::new());
        self.rafraichir_journal(f);
        g.set_focus_initial(true);
        f.global::<crate::DonneesReglagesServeur>()
            .set_ouverte(false);
        f.global::<crate::DonneesAdmin>().set_ouverte(false);
        g.set_ouverte(true);
        charger_signalements(f, onglet);
    }

    pub fn fermer(&self, f: &Principale) {
        f.global::<DonneesParametres>().set_ouverte(false);
        let c = f.global::<DonneesCoquille>();
        c.set_demande_focus(c.get_demande_focus().wrapping_add(1));
    }

    pub fn choisir(&self, f: &Principale, id: &str) {
        let Some(onglet) = Onglet::depuis_id(id) else {
            return;
        };
        poser_onglet(f, onglet);
        self.outils
            .memoire
            .modifier(|p| p.onglet_parametres = id.to_string());
        self.outils.memoire.lire(|p| afficher_preferences(f, p));
        if onglet == Onglet::Debogage {
            self.rafraichir_journal(f);
        }
        charger_signalements(f, onglet);
    }

    pub fn deplacer(&self, f: &Principale, pas: i32) {
        let g = f.global::<DonneesParametres>();
        let affiches: Vec<Onglet> = g
            .get_onglets()
            .iter()
            .filter_map(|o| Onglet::depuis_id(&o.id))
            .collect();
        let Some(courant) = Onglet::depuis_id(&g.get_onglet()) else {
            return;
        };
        let cible = match pas {
            p if p <= -(affiches.len() as i32) => affiches.first().copied(),
            p if p >= affiches.len() as i32 => affiches.last().copied(),
            p => Some(voisin(&affiches, courant, p)),
        };
        if let Some(cible) = cible {
            self.choisir(f, cible.id());
            g.set_retour_focus(g.get_retour_focus().wrapping_add(1));
        }
    }

    pub fn version_cliquee(&self, f: &Principale) {
        let actuel = self.outils.memoire.lire(|p| p.alpha);
        if let Some(alpha) = self.deblocage.borrow_mut().cliquer(actuel) {
            self.outils.memoire.modifier(|p| p.alpha = alpha);
            f.global::<DonneesParametres>()
                .set_annonce(annonce_alpha(alpha).into());
        }
    }

    pub fn definir_journal(&self, f: &Principale, actif: bool) {
        self.outils.journal.activer(actif);
        self.outils.memoire.modifier(|p| p.journal_debogage = actif);
        f.global::<DonneesParametres>().set_journal_actif(actif);
        self.rafraichir_journal(f);
    }

    pub fn filtrer(&self, f: &Principale, niveau: i32, texte: &str) {
        let g = f.global::<DonneesParametres>();
        g.set_niveau(niveau);
        g.set_filtre(texte.into());
        self.rafraichir_journal(f);
    }

    pub fn rafraichir_journal(&self, f: &Principale) {
        let g = f.global::<DonneesParametres>();
        let entrees = lire_entrees(&self.outils.journal.lire());
        let filtre = g.get_filtre();
        let gardees = filtrer(&entrees, niveau_du_rang(g.get_niveau()), &filtre);
        let debut = gardees.len().saturating_sub(LIGNES_AFFICHEES);
        let fuseau = self.outils.fuseau.clone();
        g.set_total_lignes(entrees.len() as i32);
        g.set_lignes(ModelRc::new(VecModel::from(
            gardees[debut..]
                .iter()
                .map(|e| LigneJournal {
                    texte: formater(e, fuseau(e.t as i64)).into(),
                    niveau: e.l.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )));
    }

    pub fn copier_journal(&self, f: &Principale) {
        crate::menu::copier_texte(f, &self.outils.journal.exporter());
        f.global::<DonneesParametres>()
            .set_message_journal("Journal copié.".into());
    }

    pub fn exporter_journal(&self, f: &Principale) -> Option<PathBuf> {
        let maintenant = (self.outils.horloge)();
        let chemin = self
            .outils
            .dossier_export
            .join(nom_d_export(maintenant, (self.outils.fuseau)(maintenant)));
        let resultat = std::fs::create_dir_all(&self.outils.dossier_export)
            .and_then(|_| self.outils.journal.exporter_vers(&chemin));
        let message = match &resultat {
            Ok(()) => format!("Journal exporté : {}", chemin.display()),
            Err(e) => format!("Export impossible : {e}"),
        };
        f.global::<DonneesParametres>()
            .set_message_journal(message.into());
        resultat.ok().map(|_| chemin)
    }

    pub fn vider_journal(&self, f: &Principale) {
        self.outils.journal.vider();
        f.global::<DonneesParametres>()
            .set_message_journal("Journal vidé.".into());
        self.rafraichir_journal(f);
    }
}

pub fn brancher(f: &Principale, outils: OutilsParametres) -> Rc<Parametres> {
    let p = Rc::new(Parametres {
        outils,
        deblocage: RefCell::new(DeblocageAlpha::default()),
    });
    let g = f.global::<DonneesParametres>();
    macro_rules! rappel {
        ($methode:ident, |$p:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $p = p.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_ouvrir, |p, f, demande| p.ouvrir(&f, &demande));
    rappel!(on_fermer, |p, f| p.fermer(&f));
    rappel!(on_choisir, |p, f, id| p.choisir(&f, &id));
    rappel!(on_deplacer, |p, f, pas| p.deplacer(&f, pas));
    rappel!(on_chercher, |_p, f, _texte| afficher_onglets(&f));
    rappel!(on_theme_choisi, |p, _f, mode| p
        .outils
        .memoire
        .modifier(|m| m.theme = theme_des_preferences(mode)));
    rappel!(on_densite_choisie, |p, _f, compacte| p
        .outils
        .memoire
        .modifier(|m| m.densite = if compacte {
            Densite::Compacte
        } else {
            Densite::Confortable
        }));
    rappel!(on_animations_change, |p, _f, actif| p
        .outils
        .memoire
        .modifier(|m| m.reduire_animations = actif));
    rappel!(on_version_cliquee, |p, f| p.version_cliquee(&f));
    rappel!(on_journal_change, |p, f, actif| p
        .definir_journal(&f, actif));
    rappel!(on_filtrer, |p, f, niveau, texte| p
        .filtrer(&f, niveau, &texte));
    rappel!(on_rafraichir_journal, |p, f| p.rafraichir_journal(&f));
    rappel!(on_copier_journal, |p, f| p.copier_journal(&f));
    rappel!(on_exporter_journal, |p, f| {
        p.exporter_journal(&f);
    });
    rappel!(on_vider_journal, |p, f| p.vider_journal(&f));
    rappel!(on_se_deconnecter, |_p, f| f.invoke_se_deconnecter());
    p
}
