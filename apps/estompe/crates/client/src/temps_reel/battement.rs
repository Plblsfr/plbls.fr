use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, Instant};

use super::transport::Avenir;
use crate::http::ErreurApi;

pub const PERIODE_DEFAUT: u64 = 30;
const PERIODE_MIN: f64 = 5.0;
const PERIODE_MAX: f64 = 3600.0;

pub fn periode_valide(secondes: &Value) -> u64 {
    match secondes.as_f64() {
        Some(s) if secondes.is_number() && (PERIODE_MIN..=PERIODE_MAX).contains(&s) => s as u64,
        _ => PERIODE_DEFAUT,
    }
}

macro_rules! liste_fermee {
    ($nom:ident { $($variante:ident = $texte:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $nom {
            $($variante,)*
        }

        impl $nom {
            pub const TOUTES: &'static [$nom] = &[$($nom::$variante,)*];

            pub fn texte(self) -> &'static str {
                match self {
                    $($nom::$variante => $texte,)*
                }
            }
        }
    };
}

liste_fermee!(Plateforme {
    Web = "web",
    Pwa = "pwa",
    Windows = "windows",
    Android = "android",
});

liste_fermee!(Section {
    Messagerie = "messagerie",
    Entracte = "entracte",
    Sillon = "sillon",
    Regie = "regie",
    Sablier = "sablier",
    Admin = "admin",
    Autre = "autre",
});

liste_fermee!(Phase {
    Chargement = "loading",
    Connexion = "login",
    Refuse = "denied",
    Admin = "admin",
    Activation = "activate",
    Pret = "ready",
    Sections = "sections",
    Erreur = "error",
});

pub fn plateforme_de(id: &str) -> Plateforme {
    Plateforme::TOUTES
        .iter()
        .copied()
        .find(|p| p.texte() == id)
        .unwrap_or(Plateforme::Web)
}

pub fn section_de(route: &str) -> Section {
    match route {
        "home" | "channel" | "dm" => Section::Messagerie,
        "films" => Section::Entracte,
        "musique" => Section::Sillon,
        "regie" => Section::Regie,
        "admin" => Section::Admin,
        "stats" => Section::Sablier,
        _ => Section::Autre,
    }
}

pub fn session_ouverte(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::Pret | Phase::Sections | Phase::Admin | Phase::Activation | Phase::Refuse
    )
}

pub fn section_ouverte(phase: Phase, route: &str) -> Section {
    match phase {
        Phase::Admin => Section::Admin,
        Phase::Pret | Phase::Sections => section_de(route),
        _ => Section::Autre,
    }
}

pub trait EnvironnementBattement: Send + Sync + 'static {
    fn au_premier_plan(&self) -> bool;
    fn envoyer(&self) -> Avenir<'static, Result<(), ErreurApi>>;
    fn periode(&self) -> Option<Avenir<'static, Result<Value, ErreurApi>>> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalFenetre {
    Visibilite,
    Gel,
    Reprise,
    Focus,
    PerteFocus,
    Quittee,
    Affichee,
}

enum Commande {
    Demarrer,
    Arreter,
    Signal(SignalFenetre),
}

pub struct Battement {
    commandes: mpsc::UnboundedSender<Commande>,
    actif: watch::Receiver<bool>,
    tache: JoinHandle<()>,
}

impl Battement {
    pub fn nouveau(env: impl EnvironnementBattement) -> Self {
        let (commandes, recues) = mpsc::unbounded_channel();
        let (actif, suivi) = watch::channel(false);
        let acteur = Acteur {
            env: Box::new(env),
            commandes: recues,
            actif,
            suivi: false,
            secondes: PERIODE_DEFAUT,
            prochain: None,
            periode: None,
        };
        Self {
            commandes,
            actif: suivi,
            tache: tokio::spawn(acteur.executer()),
        }
    }

    pub fn demarrer(&self) {
        let _ = self.commandes.send(Commande::Demarrer);
    }

    pub fn arreter(&self) {
        let _ = self.commandes.send(Commande::Arreter);
    }

    pub fn signaler(&self, signal: SignalFenetre) {
        let _ = self.commandes.send(Commande::Signal(signal));
    }

    pub fn actif(&self) -> bool {
        *self.actif.borrow()
    }
}

impl Drop for Battement {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

struct Acteur {
    env: Box<dyn EnvironnementBattement>,
    commandes: mpsc::UnboundedReceiver<Commande>,
    actif: watch::Sender<bool>,
    suivi: bool,
    secondes: u64,
    prochain: Option<Instant>,
    periode: Option<Avenir<'static, Result<Value, ErreurApi>>>,
}

async fn dormir_jusqua(echeance: Option<Instant>) {
    match echeance {
        Some(echeance) => sleep_until(echeance).await,
        None => std::future::pending().await,
    }
}

impl Acteur {
    fn est_actif(&self) -> bool {
        self.prochain.is_some()
    }

    fn poser(&mut self, prochain: Option<Instant>) {
        self.prochain = prochain;
        self.actif.send_replace(prochain.is_some());
    }

    fn suspendre(&mut self) {
        self.poser(None);
    }

    fn battre(&mut self) -> bool {
        if !self.env.au_premier_plan() {
            self.suspendre();
            return false;
        }
        tokio::spawn(self.env.envoyer());
        true
    }

    fn periode(&self) -> Duration {
        Duration::from_secs(self.secondes)
    }

    fn evaluer(&mut self) {
        if !self.suivi || !self.env.au_premier_plan() {
            self.suspendre();
            return;
        }
        if self.est_actif() {
            return;
        }
        if self.battre() {
            self.poser(Some(Instant::now() + self.periode()));
        }
    }

    fn regler(&mut self, secondes: &Value) {
        let valeur = periode_valide(secondes);
        if valeur == self.secondes {
            return;
        }
        self.secondes = valeur;
        if self.est_actif() {
            self.poser(Some(Instant::now() + self.periode()));
        }
    }

    fn commande(&mut self, commande: Commande) {
        match commande {
            Commande::Demarrer => {
                if self.suivi {
                    return;
                }
                self.suivi = true;
                self.evaluer();
                self.periode = self.env.periode();
            }
            Commande::Arreter => {
                if !self.suivi {
                    return;
                }
                self.suivi = false;
                self.periode = None;
                self.suspendre();
            }
            Commande::Signal(SignalFenetre::Gel | SignalFenetre::Quittee) => {
                if self.suivi {
                    self.suspendre();
                }
            }
            Commande::Signal(_) => {
                if self.suivi {
                    self.evaluer();
                }
            }
        }
    }

    async fn executer(mut self) {
        loop {
            let prochain = self.prochain;
            tokio::select! {
                biased;
                _ = dormir_jusqua(prochain) => {
                    if let Some(echeance) = self.prochain {
                        if self.battre() {
                            self.poser(Some(echeance + self.periode()));
                        }
                    }
                }
                resultat = async { self.periode.as_mut().expect("période demandée").await }, if self.periode.is_some() => {
                    self.periode = None;
                    if let Ok(secondes) = resultat {
                        self.regler(&secondes);
                    }
                }
                commande = self.commandes.recv() => match commande {
                    Some(commande) => self.commande(commande),
                    None => return,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use serde_json::json;
    use tokio::time::sleep;

    use super::*;

    type FabriquePeriode = Box<dyn Fn() -> Avenir<'static, Result<Value, ErreurApi>> + Send + Sync>;

    type Echec = Box<dyn Fn(usize) -> bool + Send + Sync>;

    struct FauxEnv {
        visible: Arc<AtomicBool>,
        focus: Arc<AtomicBool>,
        envois: Arc<AtomicUsize>,
        echec: Echec,
        periode: Option<FabriquePeriode>,
    }

    impl EnvironnementBattement for FauxEnv {
        fn au_premier_plan(&self) -> bool {
            self.visible.load(Ordering::SeqCst) && self.focus.load(Ordering::SeqCst)
        }

        fn envoyer(&self) -> Avenir<'static, Result<(), ErreurApi>> {
            let n = self.envois.fetch_add(1, Ordering::SeqCst) + 1;
            let echoue = (self.echec)(n);
            Box::pin(async move {
                if echoue {
                    Err(ErreurApi::Reseau("réseau".into()))
                } else {
                    Ok(())
                }
            })
        }

        fn periode(&self) -> Option<Avenir<'static, Result<Value, ErreurApi>>> {
            self.periode.as_ref().map(|f| f())
        }
    }

    struct Monte {
        battement: Battement,
        visible: Arc<AtomicBool>,
        focus: Arc<AtomicBool>,
        envois: Arc<AtomicUsize>,
    }

    impl Monte {
        fn envois(&self) -> usize {
            self.envois.load(Ordering::SeqCst)
        }

        fn masquer(&self) {
            self.visible.store(false, Ordering::SeqCst);
            self.battement.signaler(SignalFenetre::Visibilite);
        }

        fn afficher(&self) {
            self.visible.store(true, Ordering::SeqCst);
            self.battement.signaler(SignalFenetre::Visibilite);
        }

        fn perdre_focus(&self) {
            self.focus.store(false, Ordering::SeqCst);
            self.battement.signaler(SignalFenetre::PerteFocus);
        }

        fn prendre_focus(&self) {
            self.focus.store(true, Ordering::SeqCst);
            self.battement.signaler(SignalFenetre::Focus);
        }
    }

    fn monter_avec(periode: Option<FabriquePeriode>, echec: Echec) -> Monte {
        let visible = Arc::new(AtomicBool::new(true));
        let focus = Arc::new(AtomicBool::new(true));
        let envois = Arc::new(AtomicUsize::new(0));
        let env = FauxEnv {
            visible: visible.clone(),
            focus: focus.clone(),
            envois: envois.clone(),
            echec,
            periode,
        };
        Monte {
            battement: Battement::nouveau(env),
            visible,
            focus,
            envois,
        }
    }

    fn monter() -> Monte {
        monter_avec(None, Box::new(|_| false))
    }

    fn periode_fixe(valeur: Result<Value, ErreurApi>) -> Option<FabriquePeriode> {
        Some(Box::new(move || {
            let valeur = valeur.clone();
            Box::pin(async move { valeur })
        }))
    }

    async fn avancer_ms(ms: u64) {
        sleep(Duration::from_millis(ms)).await;
    }

    async fn reglage() {
        avancer_ms(1).await;
    }

    const MS: u64 = PERIODE_DEFAUT * 1000;

    #[tokio::test(start_paused = true)]
    async fn bat_aussitot_puis_a_chaque_periode_au_premier_plan() {
        let m = monter();
        m.battement.demarrer();
        reglage().await;
        assert_eq!(m.envois(), 1);
        avancer_ms(MS - 2).await;
        assert_eq!(m.envois(), 1);
        avancer_ms(2).await;
        assert_eq!(m.envois(), 2);
        avancer_ms(MS * 3).await;
        assert_eq!(m.envois(), 5);
    }

    #[tokio::test(start_paused = true)]
    async fn n_envoie_rien_quand_la_page_est_masquee() {
        let m = monter();
        m.visible.store(false, Ordering::SeqCst);
        m.battement.demarrer();
        avancer_ms(MS * 10).await;
        assert_eq!(m.envois(), 0);
        assert!(!m.battement.actif());
    }

    #[tokio::test(start_paused = true)]
    async fn n_envoie_rien_sans_le_focus() {
        let m = monter();
        m.focus.store(false, Ordering::SeqCst);
        m.battement.demarrer();
        avancer_ms(MS * 10).await;
        assert_eq!(m.envois(), 0);
        assert!(!m.battement.actif());
    }

    #[tokio::test(start_paused = true)]
    async fn s_arrete_aussitot_quand_la_page_est_masquee() {
        let m = monter();
        m.battement.demarrer();
        avancer_ms(MS + 1).await;
        assert_eq!(m.envois(), 2);
        m.masquer();
        reglage().await;
        assert!(!m.battement.actif());
        avancer_ms(MS * 10).await;
        assert_eq!(m.envois(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn s_arrete_aussitot_a_la_perte_du_focus() {
        let m = monter();
        m.battement.demarrer();
        reglage().await;
        m.perdre_focus();
        reglage().await;
        assert!(!m.battement.actif());
        avancer_ms(MS * 10).await;
        assert_eq!(m.envois(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn s_arrete_quand_la_fenetre_est_quittee_ou_gelee() {
        for signal in [SignalFenetre::Quittee, SignalFenetre::Gel] {
            let m = monter();
            m.battement.demarrer();
            m.battement.signaler(signal);
            reglage().await;
            assert!(!m.battement.actif());
            avancer_ms(MS * 10).await;
            assert_eq!(m.envois(), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reprend_par_un_battement_immediat_au_retour_de_la_visibilite_et_du_focus() {
        let m = monter();
        m.battement.demarrer();
        reglage().await;
        m.masquer();
        m.perdre_focus();
        m.afficher();
        avancer_ms(MS * 2).await;
        assert_eq!(m.envois(), 1);
        m.prendre_focus();
        reglage().await;
        assert_eq!(m.envois(), 2);
        avancer_ms(MS - 2).await;
        assert_eq!(m.envois(), 2);
        avancer_ms(2).await;
        assert_eq!(m.envois(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn ne_double_pas_le_cycle_quand_un_signal_confirme_l_etat() {
        let m = monter();
        m.battement.demarrer();
        m.prendre_focus();
        m.afficher();
        m.battement.signaler(SignalFenetre::Affichee);
        m.battement.signaler(SignalFenetre::Reprise);
        m.battement.demarrer();
        reglage().await;
        assert_eq!(m.envois(), 1);
        assert!(m.battement.actif());
        avancer_ms(MS).await;
        assert_eq!(m.envois(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn n_envoie_pas_si_le_focus_est_perdu_sans_signal() {
        let m = monter();
        m.battement.demarrer();
        reglage().await;
        m.focus.store(false, Ordering::SeqCst);
        avancer_ms(MS * 3).await;
        assert_eq!(m.envois(), 1);
        assert!(!m.battement.actif());
        m.prendre_focus();
        reglage().await;
        assert_eq!(m.envois(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn avale_un_echec_de_l_envoi_et_continue_le_cycle() {
        let m = monter_avec(None, Box::new(|_| true));
        m.battement.demarrer();
        avancer_ms(MS * 3 + 1).await;
        assert_eq!(m.envois(), 4);
        assert!(m.battement.actif());
    }

    #[tokio::test(start_paused = true)]
    async fn suit_la_periode_annoncee_par_le_serveur() {
        let m = monter_avec(periode_fixe(Ok(json!(10))), Box::new(|_| false));
        m.battement.demarrer();
        reglage().await;
        assert_eq!(m.envois(), 1);
        avancer_ms(10_000).await;
        assert_eq!(m.envois(), 2);
        avancer_ms(20_000).await;
        assert_eq!(m.envois(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn garde_30_s_quand_la_periode_est_absente_invalide_ou_illisible() {
        for periode in [
            Ok(Value::Null),
            Ok(json!(0)),
            Err(ErreurApi::Reseau("401".into())),
        ] {
            let m = monter_avec(periode_fixe(periode), Box::new(|_| false));
            m.battement.demarrer();
            avancer_ms(MS + 1).await;
            assert_eq!(m.envois(), 2);
            m.battement.arreter();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ne_relance_pas_le_cycle_si_la_periode_arrive_page_masquee() {
        let m = monter_avec(periode_fixe(Ok(json!(10))), Box::new(|_| false));
        m.visible.store(false, Ordering::SeqCst);
        m.battement.demarrer();
        avancer_ms(MS).await;
        assert_eq!(m.envois(), 0);
        m.afficher();
        avancer_ms(10_001).await;
        assert_eq!(m.envois(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn n_ecoute_plus_rien_apres_l_arret() {
        let m = monter();
        m.battement.demarrer();
        m.battement.arreter();
        reglage().await;
        assert!(!m.battement.actif());
        m.prendre_focus();
        m.afficher();
        m.battement.signaler(SignalFenetre::Affichee);
        avancer_ms(MS * 3).await;
        assert_eq!(m.envois(), 1);
        m.battement.demarrer();
        reglage().await;
        assert_eq!(m.envois(), 2);
    }

    #[test]
    fn borne_la_periode() {
        assert_eq!(periode_valide(&json!(30)), 30);
        assert_eq!(periode_valide(&json!(45)), 45);
        for v in [
            Value::Null,
            json!("30"),
            Value::from(f64::NAN),
            Value::from(f64::INFINITY),
            json!(0),
            json!(-5),
            json!(4),
            json!(3601),
        ] {
            assert_eq!(periode_valide(&v), PERIODE_DEFAUT, "{v}");
        }
    }

    #[test]
    fn ramene_la_plateforme_a_la_liste_fermee() {
        for p in ["web", "pwa", "windows", "android"] {
            assert_eq!(plateforme_de(p).texte(), p);
        }
        assert_eq!(plateforme_de("ios"), Plateforme::Web);
        assert_eq!(plateforme_de(""), Plateforme::Web);
    }

    #[test]
    fn deduit_la_section_de_la_route() {
        for (route, section) in [
            ("home", Section::Messagerie),
            ("channel", Section::Messagerie),
            ("dm", Section::Messagerie),
            ("films", Section::Entracte),
            ("musique", Section::Sillon),
            ("regie", Section::Regie),
            ("admin", Section::Admin),
            ("stats", Section::Sablier),
            ("space", Section::Autre),
            ("join", Section::Autre),
            ("constructor", Section::Autre),
        ] {
            assert_eq!(section_de(route), section, "{route}");
        }
    }

    #[test]
    fn tient_compte_de_l_ecran_affiche_hors_de_la_messagerie() {
        assert_eq!(section_ouverte(Phase::Pret, "musique"), Section::Sillon);
        assert_eq!(section_ouverte(Phase::Sections, "films"), Section::Entracte);
        assert_eq!(section_ouverte(Phase::Admin, "home"), Section::Admin);
        assert_eq!(section_ouverte(Phase::Activation, "home"), Section::Autre);
        assert_eq!(section_ouverte(Phase::Refuse, "home"), Section::Autre);
    }

    #[test]
    fn ne_compte_une_session_que_connectee() {
        for p in [
            Phase::Pret,
            Phase::Sections,
            Phase::Admin,
            Phase::Activation,
            Phase::Refuse,
        ] {
            assert!(session_ouverte(p), "{p:?}");
        }
        for p in [Phase::Chargement, Phase::Connexion, Phase::Erreur] {
            assert!(!session_ouverte(p), "{p:?}");
        }
    }
}
