use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_plateforme::{libelle_touche, touche, EtatTouche, RaccourcisGlobaux};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

pub const TOUCHE_PAR_DEFAUT: &str = "KeyV";
pub const DELAI_MAXIMAL_MS: u16 = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesPtt {
    pub actif: bool,
    pub touche: String,
    pub delai_ms: u16,
}

impl Default for ReglagesPtt {
    fn default() -> Self {
        Self {
            actif: false,
            touche: TOUCHE_PAR_DEFAUT.to_string(),
            delai_ms: 0,
        }
    }
}

impl ReglagesPtt {
    pub fn borne(mut self) -> Self {
        if touche(&self.touche).is_none() {
            self.touche = TOUCHE_PAR_DEFAUT.to_string();
        }
        self.delai_ms = self.delai_ms.min(DELAI_MAXIMAL_MS);
        self
    }

    pub fn libelle(&self) -> String {
        libelle_touche(&self.touche)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceTouche {
    Fenetre,
    Globale,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatPtt {
    pub actif: bool,
    pub global: bool,
    pub parle: bool,
    pub erreur: Option<String>,
}

pub type Porte = Arc<dyn Fn(bool) + Send + Sync>;
pub type Suivi = Arc<dyn Fn(EtatPtt) + Send + Sync>;

struct Interne {
    reglages: ReglagesPtt,
    en_vocal: bool,
    fenetre_active: bool,
    enfoncees: BTreeSet<SourceTouche>,
    ouverte: bool,
    generation: u64,
    enregistree: Option<String>,
    erreur: Option<String>,
}

#[derive(Clone)]
pub struct PousserPourParler {
    interne: Arc<Mutex<Interne>>,
    raccourcis: Arc<dyn RaccourcisGlobaux>,
    runtime: Handle,
    porte: Porte,
    suivi: Arc<Mutex<Option<Suivi>>>,
}

pub const MESSAGE_INDISPONIBLE: &str =
    "Raccourci global indisponible sur ce système : la touche ne marche que lorsque la fenêtre d'Estompe est active.";

impl PousserPourParler {
    pub fn nouveau(
        runtime: &Handle,
        raccourcis: Arc<dyn RaccourcisGlobaux>,
        reglages: ReglagesPtt,
        porte: Porte,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne {
                reglages: reglages.borne(),
                en_vocal: false,
                fenetre_active: true,
                enfoncees: BTreeSet::new(),
                ouverte: true,
                generation: 0,
                enregistree: None,
                erreur: None,
            })),
            raccourcis,
            runtime: runtime.clone(),
            porte,
            suivi: Arc::new(Mutex::new(None)),
        }
    }

    pub fn suivre(&self, suivi: Suivi) {
        *self.suivi.lock().unwrap_or_else(|e| e.into_inner()) = Some(suivi);
        self.publier();
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn global_disponible(&self) -> bool {
        self.raccourcis.disponible()
    }

    pub fn reglages(&self) -> ReglagesPtt {
        self.verrou().reglages.clone()
    }

    pub fn etat(&self) -> EtatPtt {
        let i = self.verrou();
        EtatPtt {
            actif: i.reglages.actif,
            global: i.enregistree.is_some(),
            parle: i.reglages.actif && i.ouverte,
            erreur: i.erreur.clone(),
        }
    }

    fn publier(&self) {
        let suivi = self.suivi.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(suivi) = suivi {
            suivi(self.etat());
        }
    }

    pub fn configurer(&self, reglages: ReglagesPtt) {
        {
            let mut i = self.verrou();
            i.reglages = reglages.borne();
            i.enfoncees.clear();
            i.generation += 1;
        }
        self.synchroniser();
    }

    pub fn en_vocal(&self, en_vocal: bool) {
        {
            let mut i = self.verrou();
            if i.en_vocal == en_vocal {
                return;
            }
            i.en_vocal = en_vocal;
            i.enfoncees.clear();
            i.generation += 1;
        }
        self.synchroniser();
    }

    pub fn fenetre_active(&self, active: bool) {
        let relacher = {
            let mut i = self.verrou();
            i.fenetre_active = active;
            !active && i.enfoncees.contains(&SourceTouche::Fenetre)
        };
        if relacher {
            self.touche(SourceTouche::Fenetre, false);
        }
    }

    pub fn est_la_touche(&self, code: &str) -> bool {
        let i = self.verrou();
        i.reglages.actif && i.en_vocal && i.reglages.touche == code
    }

    pub fn touche(&self, source: SourceTouche, appui: bool) {
        let (ouvrir, fermer_apres) = {
            let mut i = self.verrou();
            if !i.reglages.actif || !i.en_vocal {
                return;
            }
            if appui && source == SourceTouche::Globale && i.fenetre_active {
                return;
            }
            if appui {
                if !i.enfoncees.insert(source) && i.ouverte {
                    return;
                }
                i.generation += 1;
                let ouvrir = !i.ouverte;
                i.ouverte = true;
                (ouvrir, None)
            } else {
                if !i.enfoncees.remove(&source) || !i.enfoncees.is_empty() {
                    return;
                }
                i.generation += 1;
                (false, Some((i.generation, i.reglages.delai_ms)))
            }
        };
        if ouvrir {
            (self.porte)(true);
            self.publier();
        }
        if let Some((generation, delai)) = fermer_apres {
            if delai == 0 {
                self.fermer_si(generation);
            } else {
                let moi = self.clone();
                self.runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(u64::from(delai))).await;
                    moi.fermer_si(generation);
                });
            }
        }
    }

    fn fermer_si(&self, generation: u64) {
        {
            let mut i = self.verrou();
            if i.generation != generation || !i.enfoncees.is_empty() || !i.ouverte {
                return;
            }
            i.ouverte = false;
        }
        (self.porte)(false);
        self.publier();
    }

    fn synchroniser(&self) {
        let (voulue, actuelle, ouverte) = {
            let i = self.verrou();
            let voulue = (i.reglages.actif && i.en_vocal).then(|| i.reglages.touche.clone());
            (voulue, i.enregistree.clone(), !i.reglages.actif)
        };
        if actuelle != voulue {
            if let Some(ancienne) = &actuelle {
                let _ = self.raccourcis.retirer(ancienne);
            }
            let mut erreur = None;
            let mut enregistree = None;
            if let Some(code) = &voulue {
                if !self.raccourcis.disponible() {
                    erreur = Some(MESSAGE_INDISPONIBLE.to_string());
                } else {
                    let moi = self.clone();
                    match self.raccourcis.enregistrer(
                        code,
                        Box::new(move |etat| {
                            moi.touche(SourceTouche::Globale, etat == EtatTouche::Appui)
                        }),
                    ) {
                        Ok(()) => enregistree = Some(code.clone()),
                        Err(e) => {
                            erreur = Some(format!(
                                "La touche {} ne peut pas servir de raccourci global ({e}) : elle ne marche que lorsque la fenêtre d'Estompe est active.",
                                libelle_touche(code)
                            ))
                        }
                    }
                }
            }
            let mut i = self.verrou();
            i.enregistree = enregistree;
            i.erreur = erreur;
        }
        {
            let mut i = self.verrou();
            i.ouverte = ouverte;
        }
        (self.porte)(ouverte);
        self.publier();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_plateforme::RaccourcisFactices;

    struct Banc {
        ptt: PousserPourParler,
        raccourcis: Arc<RaccourcisFactices>,
        portes: Arc<Mutex<Vec<bool>>>,
    }

    fn banc(disponible: bool, delai_ms: u16) -> Banc {
        let raccourcis = Arc::new(RaccourcisFactices::nouveaux(disponible));
        let portes = Arc::new(Mutex::new(Vec::new()));
        let p = portes.clone();
        let ptt = PousserPourParler::nouveau(
            &Handle::current(),
            raccourcis.clone(),
            ReglagesPtt {
                actif: true,
                touche: "KeyV".into(),
                delai_ms,
            },
            Arc::new(move |ouverte| p.lock().unwrap().push(ouverte)),
        );
        Banc {
            ptt,
            raccourcis,
            portes,
        }
    }

    impl Banc {
        fn derniere(&self) -> Option<bool> {
            self.portes.lock().unwrap().last().copied()
        }
    }

    async fn laisser_passer(duree: Duration) {
        tokio::time::sleep(duree).await;
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn c1_la_porte_s_ouvre_a_l_appui_et_se_ferme_au_relachement_plus_le_delai() {
        let b = banc(true, 300);
        b.ptt.en_vocal(true);
        assert_eq!(
            b.derniere(),
            Some(false),
            "porte fermée en appuyer pour parler"
        );
        assert_eq!(b.raccourcis.enregistres(), ["KeyV"]);
        b.ptt.fenetre_active(false);
        b.raccourcis.appuyer("KeyV");
        assert_eq!(b.derniere(), Some(true));
        assert!(b.ptt.etat().parle && b.ptt.etat().global);
        b.raccourcis.appuyer("KeyV");
        b.raccourcis.relacher("KeyV");
        laisser_passer(Duration::from_millis(299)).await;
        assert_eq!(
            b.derniere(),
            Some(true),
            "toujours ouverte pendant le délai"
        );
        laisser_passer(Duration::from_millis(2)).await;
        assert_eq!(b.derniere(), Some(false), "fermée après le délai");
        b.raccourcis.appuyer("KeyV");
        b.raccourcis.relacher("KeyV");
        laisser_passer(Duration::from_millis(100)).await;
        b.raccourcis.appuyer("KeyV");
        laisser_passer(Duration::from_millis(500)).await;
        assert_eq!(
            b.derniere(),
            Some(true),
            "un nouvel appui annule la fermeture prévue"
        );
        assert_eq!(*b.portes.lock().unwrap(), [false, true, false, true]);
        b.ptt.en_vocal(false);
        assert!(
            b.raccourcis.enregistres().is_empty(),
            "raccourci libéré hors du vocal"
        );
        assert_eq!(b.derniere(), Some(false));
    }

    #[tokio::test(start_paused = true)]
    async fn sans_delai_la_porte_se_ferme_aussitot_et_la_fenetre_active_garde_la_main() {
        let b = banc(true, 0);
        b.ptt.en_vocal(true);
        b.raccourcis.appuyer("KeyV");
        assert_eq!(
            b.derniere(),
            Some(false),
            "fenêtre active : le raccourci global ne double pas la touche"
        );
        b.raccourcis.relacher("KeyV");
        b.ptt.touche(SourceTouche::Fenetre, true);
        assert_eq!(b.derniere(), Some(true));
        b.ptt.touche(SourceTouche::Fenetre, false);
        assert_eq!(b.derniere(), Some(false));
        b.ptt.touche(SourceTouche::Fenetre, true);
        b.ptt.fenetre_active(false);
        assert_eq!(
            b.derniere(),
            Some(false),
            "la fenêtre perd le focus : touche relâchée"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn c2_indisponible_le_reglage_le_dit_et_la_touche_marche_fenetre_active() {
        let b = banc(false, 0);
        b.ptt.en_vocal(true);
        let etat = b.ptt.etat();
        assert!(!etat.global);
        assert_eq!(etat.erreur.as_deref(), Some(MESSAGE_INDISPONIBLE));
        b.ptt.touche(SourceTouche::Fenetre, true);
        assert_eq!(b.derniere(), Some(true));
        b.ptt.touche(SourceTouche::Fenetre, false);
        assert_eq!(b.derniere(), Some(false));
    }

    #[tokio::test(start_paused = true)]
    async fn desactive_la_porte_reste_ouverte_et_la_touche_ne_fait_rien() {
        let b = banc(true, 0);
        b.ptt.en_vocal(true);
        b.ptt.configurer(ReglagesPtt::default());
        assert_eq!(b.derniere(), Some(true));
        assert!(b.raccourcis.enregistres().is_empty());
        b.ptt.touche(SourceTouche::Fenetre, false);
        assert_eq!(b.derniere(), Some(true));
        assert!(!b.ptt.est_la_touche("KeyV"));
        b.ptt.configurer(ReglagesPtt {
            actif: true,
            touche: "F13".into(),
            delai_ms: 5000,
        });
        assert_eq!(b.raccourcis.enregistres(), ["F13"]);
        assert_eq!(b.ptt.reglages().delai_ms, DELAI_MAXIMAL_MS);
        assert!(b.ptt.est_la_touche("F13"));
    }

    #[test]
    fn des_reglages_invalides_reviennent_aux_valeurs_par_defaut() {
        let r = ReglagesPtt {
            actif: true,
            touche: "Inconnue".into(),
            delai_ms: 9000,
        }
        .borne();
        assert_eq!(
            (r.touche.as_str(), r.delai_ms),
            (TOUCHE_PAR_DEFAUT, DELAI_MAXIMAL_MS)
        );
        assert_eq!(r.libelle(), "V");
    }
}
