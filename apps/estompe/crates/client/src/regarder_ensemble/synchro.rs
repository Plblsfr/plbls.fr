use crate::temps_reel::evenements::EtatSeance;

use super::fournisseurs::{EtatLecture, LecteurSeance, ModeSynchro};
use super::seance::{correction, position_attendue, Correction};

pub const COMMANDE_PERDUE_MS: i64 = 3000;
pub const COUPE_CIRCUIT_N: usize = 20;
pub const COUPE_CIRCUIT_PAUSE_MS: i64 = 5000;
pub const TOLERANCE_SAUT: f64 = 0.5;
pub const CHARGEMENT_LENT_MS: i64 = 15000;
pub const ATTENTE_MS: i64 = 1000;
pub const BLOQUE_MS: i64 = 3000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PhaseSynchro {
    #[default]
    Chargement,
    Synchro,
    Attente,
    Tampon,
    Erreur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSeance {
    Lire,
    Pause,
    Aller,
    Vitesse,
}

impl ActionSeance {
    pub fn nom(self) -> &'static str {
        match self {
            Self::Lire => "play",
            Self::Pause => "pause",
            Self::Aller => "seek",
            Self::Vitesse => "rate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Commande {
    pub action: ActionSeance,
    pub position: Option<f64>,
    pub vitesse: Option<f64>,
}

impl Commande {
    pub fn lire(position: f64) -> Self {
        Self {
            action: ActionSeance::Lire,
            position: Some(position),
            vitesse: None,
        }
    }

    pub fn pause(position: f64) -> Self {
        Self {
            action: ActionSeance::Pause,
            position: Some(position),
            vitesse: None,
        }
    }

    pub fn aller(position: f64) -> Self {
        Self {
            action: ActionSeance::Aller,
            position: Some(position),
            vitesse: None,
        }
    }

    pub fn vitesse(vitesse: f64) -> Self {
        Self {
            action: ActionSeance::Vitesse,
            position: None,
            vitesse: Some(vitesse),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortieSynchro {
    Envoyer {
        numero: u64,
        action: ActionSeance,
        position: Option<f64>,
        vitesse: Option<f64>,
        video: String,
    },
    Fin {
        rev: i64,
    },
    Pret(bool),
    Attente,
    Refus(Option<String>),
    Journal(String),
}

#[derive(Debug, Default)]
pub struct MoteurSynchro {
    phase: PhaseSynchro,
    position: f64,
    bloque: bool,
    attente_possible: bool,
    serveur: Option<(EtatSeance, i64)>,
    video: String,
    rev_repetition: Option<i64>,
    base: Option<i64>,
    numero: u64,
    file: Vec<Commande>,
    echeance_commande: Option<i64>,
    fin_cran: Option<i64>,
    reprise: Option<i64>,
    horodatages: Vec<i64>,
    attente_depuis: Option<i64>,
    chargement_depuis: Option<i64>,
    cle: Option<(i64, String)>,
    pret_envoye: Option<bool>,
    pas_pret_depuis: Option<i64>,
    attente_envoyee: bool,
    sorties: Vec<SortieSynchro>,
}

impl MoteurSynchro {
    pub fn nouveau(attente_possible: bool) -> Self {
        Self {
            attente_possible,
            ..Self::default()
        }
    }

    pub fn phase(&self) -> PhaseSynchro {
        self.phase
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn bloque(&self) -> bool {
        self.bloque
    }

    pub fn suspendu(&self) -> bool {
        self.phase == PhaseSynchro::Erreur
    }

    pub fn lent(&self, maintenant: i64) -> bool {
        self.phase == PhaseSynchro::Chargement
            && self
                .chargement_depuis
                .is_some_and(|t| maintenant - t > CHARGEMENT_LENT_MS)
    }

    pub fn en_chargement(&self, lecteur: &dyn LecteurSeance) -> bool {
        self.phase != PhaseSynchro::Erreur
            && (self.phase == PhaseSynchro::Chargement || lecteur.etat() == EtatLecture::Tampon)
    }

    pub fn serveur(&self) -> Option<&EtatSeance> {
        self.serveur.as_ref().map(|(e, _)| e)
    }

    pub fn sorties(&mut self) -> Vec<SortieSynchro> {
        std::mem::take(&mut self.sorties)
    }

    pub fn prochaine_echeance(&self) -> Option<i64> {
        [self.echeance_commande, self.fin_cran, self.reprise]
            .into_iter()
            .flatten()
            .min()
    }

    fn attendue(&self, maintenant: i64) -> f64 {
        self.serveur
            .as_ref()
            .map_or(0.0, |(e, d)| position_attendue(e, *d, maintenant))
    }

    fn journal(&mut self, message: String) {
        self.sorties.push(SortieSynchro::Journal(message));
    }

    pub fn echeances(&mut self, lecteur: &mut dyn LecteurSeance, maintenant: i64) {
        if self.echeance_commande.is_some_and(|t| maintenant >= t) {
            self.echouer(lecteur, None, maintenant);
        }
        if self.fin_cran.is_some_and(|t| maintenant >= t) {
            self.fin_cran = None;
            lecteur.regler_vitesse(self.serveur().map_or(1.0, |e| e.rate));
        }
        if self.reprise.is_some_and(|t| maintenant >= t) {
            self.reprise = None;
            self.horodatages.clear();
            self.phase = PhaseSynchro::Synchro;
            self.appliquer_reception(lecteur, maintenant);
        }
    }

    pub fn recevoir(
        &mut self,
        lecteur: &mut dyn LecteurSeance,
        etat: EtatSeance,
        decalage: i64,
        maintenant: i64,
    ) {
        self.serveur = Some((etat, decalage));
        self.echeances(lecteur, maintenant);
        self.appliquer_reception(lecteur, maintenant);
    }

    fn appliquer_reception(&mut self, lecteur: &mut dyn LecteurSeance, maintenant: i64) {
        let Some((s, _)) = &self.serveur else {
            return;
        };
        if self.phase == PhaseSynchro::Erreur {
            return;
        }
        let rev = s.rev;
        let cle = (rev, s.at.clone());
        if self.cle.as_ref() == Some(&cle) {
            return;
        }
        self.cle = Some(cle);
        self.horodatages.retain(|t| maintenant - t < 1000);
        self.horodatages.push(maintenant);
        if self.horodatages.len() > COUPE_CIRCUIT_N {
            self.couper(maintenant);
            return;
        }
        if let Some(base) = self.base {
            if rev <= base {
                return;
            }
            self.base = None;
            self.echeance_commande = None;
            if self.phase == PhaseSynchro::Attente {
                self.phase = PhaseSynchro::Synchro;
            }
        }
        self.appliquer_serveur(lecteur, maintenant);
        self.vider(maintenant);
    }

    fn couper(&mut self, maintenant: i64) {
        self.journal("coupe-circuit : trop d’évènements, session suspendue".into());
        self.phase = PhaseSynchro::Erreur;
        self.file.clear();
        self.base = None;
        self.echeance_commande = None;
        self.fin_cran = None;
        self.reprise = Some(maintenant + COUPE_CIRCUIT_PAUSE_MS);
    }

    fn appliquer_serveur(&mut self, lecteur: &mut dyn LecteurSeance, maintenant: i64) {
        let Some((s, _)) = self.serveur.clone() else {
            return;
        };
        let encore = s.act == "repeat" && Some(s.rev) != self.rev_repetition;
        if s.video != self.video || encore {
            self.rev_repetition = Some(s.rev);
            self.video = s.video.clone();
            self.pret_envoye = None;
            self.pas_pret_depuis = None;
            self.attente_envoyee = false;
            self.bloque = false;
            self.attente_depuis = None;
            self.phase = PhaseSynchro::Chargement;
            self.chargement_depuis = Some(maintenant);
            self.fin_cran = None;
            let depart = if s.playing {
                self.attendue(maintenant)
            } else {
                s.pos
            };
            lecteur.charger(&s.video, depart, s.playing);
            return;
        }
        if self.phase == PhaseSynchro::Chargement || lecteur.mode() == ModeSynchro::Lancement {
            return;
        }
        let st = lecteur.etat();
        let attendue = self.attendue(maintenant);
        let en_cours = matches!(st, EtatLecture::Lecture | EtatLecture::Tampon);
        if s.playing {
            if !en_cours {
                lecteur.lire();
            }
            if (lecteur.temps() - attendue).abs() >= 1.0 {
                lecteur.aller(attendue);
            }
        } else {
            if en_cours {
                lecteur.pause();
            }
            if (lecteur.temps() - s.pos).abs() > TOLERANCE_SAUT {
                lecteur.aller(s.pos);
            }
        }
        if lecteur.vitesse_reglable() && lecteur.vitesse() != s.rate {
            lecteur.regler_vitesse(s.rate);
        }
        self.position = if s.playing { attendue } else { s.pos };
    }

    pub fn commande(&mut self, lecteur: &mut dyn LecteurSeance, cmd: Commande, maintenant: i64) {
        self.echeances(lecteur, maintenant);
        if matches!(self.phase, PhaseSynchro::Erreur | PhaseSynchro::Chargement)
            || self.serveur.is_none()
        {
            return;
        }
        match cmd.action {
            ActionSeance::Lire => lecteur.lire(),
            ActionSeance::Pause => lecteur.pause(),
            ActionSeance::Aller => {
                let p = cmd.position.unwrap_or(0.0);
                lecteur.aller(p);
                self.position = p;
            }
            ActionSeance::Vitesse => lecteur.regler_vitesse(cmd.vitesse.unwrap_or(1.0)),
        }
        let pp = matches!(cmd.action, ActionSeance::Lire | ActionSeance::Pause);
        self.file.retain(|q| {
            q.action != cmd.action
                && !(pp
                    && matches!(
                        q.action,
                        ActionSeance::Aller | ActionSeance::Lire | ActionSeance::Pause
                    ))
        });
        self.file.push(cmd);
        self.vider(maintenant);
    }

    fn vider(&mut self, maintenant: i64) {
        if self.base.is_some() || self.file.is_empty() {
            return;
        }
        let Some(rev) = self.serveur().map(|e| e.rev) else {
            return;
        };
        let cmd = self.file.remove(0);
        self.base = Some(rev);
        self.phase = PhaseSynchro::Attente;
        self.journal(format!("commande {} (rév {rev})", cmd.action.nom()));
        self.echeance_commande = Some(maintenant + COMMANDE_PERDUE_MS);
        self.numero += 1;
        self.sorties.push(SortieSynchro::Envoyer {
            numero: self.numero,
            action: cmd.action,
            position: cmd.position,
            vitesse: cmd.vitesse,
            video: self.video.clone(),
        });
    }

    pub fn reponse(
        &mut self,
        lecteur: &mut dyn LecteurSeance,
        numero: u64,
        resultat: Result<(), String>,
        maintenant: i64,
    ) {
        if let Err(e) = resultat {
            if numero == self.numero && self.base.is_some() {
                self.echouer(lecteur, Some(e), maintenant);
            }
        }
    }

    fn echouer(
        &mut self,
        lecteur: &mut dyn LecteurSeance,
        erreur: Option<String>,
        maintenant: i64,
    ) {
        self.echeance_commande = None;
        self.base = None;
        self.file.clear();
        self.phase = PhaseSynchro::Synchro;
        self.sorties.push(SortieSynchro::Refus(erreur));
        if self.serveur.is_some() {
            self.appliquer_serveur(lecteur, maintenant);
        }
    }

    pub fn observer(&mut self, lecteur: &mut dyn LecteurSeance, st: EtatLecture, maintenant: i64) {
        if st == EtatLecture::Tampon {
            self.signaler(false);
        } else if st == EtatLecture::Lecture
            || (matches!(st, EtatLecture::Pause | EtatLecture::Amorce)
                && lecteur.pret() != Some(false))
        {
            self.signaler(true);
        }
        if st == EtatLecture::Lecture {
            self.bloque = false;
            self.attente_depuis = None;
        }
        if self.phase == PhaseSynchro::Erreur {
            return;
        }
        if st == EtatLecture::Fin {
            if let Some(rev) = self.serveur().map(|e| e.rev) {
                self.sorties.push(SortieSynchro::Fin { rev });
            }
            return;
        }
        let pret = matches!(
            st,
            EtatLecture::Lecture | EtatLecture::Pause | EtatLecture::Amorce
        );
        if self.phase == PhaseSynchro::Chargement && pret {
            self.phase = if self.base.is_none() {
                PhaseSynchro::Synchro
            } else {
                PhaseSynchro::Attente
            };
            if self.base.is_none() && self.serveur().is_some_and(|e| !e.playing) {
                self.appliquer_serveur(lecteur, maintenant);
            }
        } else if self.phase == PhaseSynchro::Synchro && st == EtatLecture::Tampon {
            self.phase = PhaseSynchro::Tampon;
        } else if self.phase == PhaseSynchro::Tampon && st != EtatLecture::Tampon {
            self.phase = PhaseSynchro::Synchro;
        }
    }

    fn signaler(&mut self, pret: bool) {
        if self.pret_envoye == Some(pret) {
            return;
        }
        self.pret_envoye = Some(pret);
        self.sorties.push(SortieSynchro::Pret(pret));
    }

    fn surveiller_pret(&mut self, lecteur: &dyn LecteurSeance, maintenant: i64) {
        let Some(s) = self.serveur() else {
            return;
        };
        let (en_lecture, attendus) = (s.playing, !s.waiting.is_empty());
        let st = lecteur.etat();
        let en_retard = self.phase == PhaseSynchro::Chargement
            || st == EtatLecture::Tampon
            || (st != EtatLecture::Lecture
                && self.pret_envoye == Some(false)
                && lecteur.pret() == Some(false));
        if !en_retard {
            self.pas_pret_depuis = None;
            self.attente_envoyee = false;
            if self.pret_envoye == Some(false)
                && !matches!(st, EtatLecture::NonDemarre | EtatLecture::Fin)
            {
                self.signaler(true);
            }
            return;
        }
        let depuis = *self.pas_pret_depuis.get_or_insert(maintenant);
        if self.attente_envoyee
            || !self.attente_possible
            || !(en_lecture || attendus)
            || maintenant - depuis < ATTENTE_MS
        {
            return;
        }
        self.attente_envoyee = true;
        self.pret_envoye = Some(false);
        self.sorties.push(SortieSynchro::Attente);
    }

    pub fn tick(&mut self, lecteur: &mut dyn LecteurSeance, maintenant: i64) {
        self.echeances(lecteur, maintenant);
        if self.serveur.is_none()
            || self.phase == PhaseSynchro::Erreur
            || lecteur.mode() == ModeSynchro::Lancement
        {
            return;
        }
        self.surveiller_pret(lecteur, maintenant);
        if self.phase == PhaseSynchro::Chargement {
            return;
        }
        let Some((s, _)) = &self.serveur else {
            return;
        };
        let (en_lecture, vitesse) = (s.playing, s.rate);
        let st = lecteur.etat();
        let pos = lecteur.temps();
        self.position = pos;
        if !en_lecture || self.base.is_some() || !self.file.is_empty() {
            self.attente_depuis = None;
            return;
        }
        if st != EtatLecture::Lecture {
            if st != EtatLecture::Tampon {
                let depuis = *self.attente_depuis.get_or_insert(maintenant);
                if maintenant - depuis > BLOQUE_MS {
                    self.bloque = true;
                }
            }
            return;
        }
        if self.fin_cran.is_some() {
            return;
        }
        if lecteur.vitesse_reglable() && lecteur.vitesse() != vitesse {
            lecteur.regler_vitesse(vitesse);
            return;
        }
        let attendue = self.attendue(maintenant);
        let ecart = pos - attendue;
        let Some(fix) = correction(ecart, attendue, vitesse) else {
            return;
        };
        if matches!(fix, Correction::Vitesse { .. }) && !lecteur.vitesse_reglable() {
            return;
        }
        if ecart.abs() >= TOLERANCE_SAUT {
            let quoi = match fix {
                Correction::Saut(_) => "saut".to_string(),
                Correction::Vitesse { vitesse, .. } => format!("vitesse {vitesse}"),
            };
            self.journal(format!("écart {ecart:.2} s → {quoi}"));
        }
        match fix {
            Correction::Saut(p) => lecteur.aller(p),
            Correction::Vitesse { vitesse, ms } => {
                lecteur.regler_vitesse(vitesse);
                self.fin_cran = Some(maintenant + ms);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::format::iso_de_ms;
    use crate::roles::date_en_millisecondes;

    pub(crate) const AT: &str = "2026-10-28T10:00:00.000Z";

    pub(crate) fn base() -> i64 {
        date_en_millisecondes(AT).unwrap()
    }

    #[derive(Debug)]
    pub(crate) struct LecteurTest {
        pub st: EtatLecture,
        pub t: f64,
        pub r: f64,
        pub pret: Option<bool>,
        pub appels: Vec<String>,
        pub suit: bool,
    }

    impl LecteurTest {
        pub(crate) fn nouveau(st: EtatLecture, suit: bool) -> Self {
            Self {
                st,
                t: 10.0,
                r: 1.0,
                pret: None,
                appels: Vec::new(),
                suit,
            }
        }
    }

    impl LecteurSeance for LecteurTest {
        fn charger(&mut self, reference: &str, _depart: f64, _lecture: bool) {
            self.appels.push(format!("load {reference}"));
        }
        fn lire(&mut self) {
            self.appels.push("play".into());
            if self.suit {
                self.st = EtatLecture::Lecture;
            }
        }
        fn pause(&mut self) {
            self.appels.push("pause".into());
            self.st = EtatLecture::Pause;
        }
        fn aller(&mut self, position: f64) {
            self.appels.push(format!("seek {position}"));
            if self.suit {
                self.t = position;
            }
        }
        fn temps(&self) -> f64 {
            self.t
        }
        fn vitesse(&self) -> f64 {
            self.r
        }
        fn regler_vitesse(&mut self, vitesse: f64) {
            self.appels.push(format!("rate {vitesse}"));
            if self.suit {
                self.r = vitesse;
            }
        }
        fn etat(&self) -> EtatLecture {
            self.st
        }
        fn pret(&self) -> Option<bool> {
            self.pret
        }
    }

    fn mk() -> EtatSeance {
        EtatSeance {
            rev: 1,
            provider: "youtube".into(),
            video: "v".into(),
            playing: true,
            pos: 10.0,
            rate: 1.0,
            at: AT.into(),
            host: "me".into(),
            by_id: "me".into(),
            by: "moi".into(),
            act: "play".into(),
            ..EtatSeance::default()
        }
    }

    struct Banc {
        e: MoteurSynchro,
        l: LecteurTest,
        etat: EtatSeance,
        maintenant: i64,
        envoyes: Vec<SortieSynchro>,
        refus: usize,
        fins: Vec<i64>,
        prets: Vec<bool>,
        attentes: usize,
    }

    impl Banc {
        fn sync() -> Self {
            let mut b = Self {
                e: MoteurSynchro::nouveau(false),
                l: LecteurTest::nouveau(EtatLecture::Lecture, true),
                etat: mk(),
                maintenant: base(),
                envoyes: Vec::new(),
                refus: 0,
                fins: Vec::new(),
                prets: Vec::new(),
                attentes: 0,
            };
            b.recevoir();
            b.observer(EtatLecture::Lecture);
            b.l.appels.clear();
            b
        }

        fn cinema(attente: bool) -> Self {
            let mut b = Self {
                e: MoteurSynchro::nouveau(attente),
                l: LecteurTest::nouveau(EtatLecture::NonDemarre, false),
                etat: EtatSeance {
                    provider: "film".into(),
                    host_only: true,
                    ..mk()
                },
                maintenant: base(),
                envoyes: Vec::new(),
                refus: 0,
                fins: Vec::new(),
                prets: Vec::new(),
                attentes: 0,
            };
            b.l.pret = Some(false);
            b.recevoir();
            b
        }

        fn ramasser(&mut self) {
            for s in self.e.sorties() {
                match s {
                    SortieSynchro::Envoyer { .. } => self.envoyes.push(s),
                    SortieSynchro::Refus(_) => self.refus += 1,
                    SortieSynchro::Fin { rev } => self.fins.push(rev),
                    SortieSynchro::Pret(p) => self.prets.push(p),
                    SortieSynchro::Attente => self.attentes += 1,
                    SortieSynchro::Journal(_) => {}
                }
            }
        }

        fn recevoir(&mut self) {
            self.e
                .recevoir(&mut self.l, self.etat.clone(), 0, self.maintenant);
            self.ramasser();
        }

        fn pousser(&mut self, f: impl FnOnce(&mut EtatSeance)) {
            f(&mut self.etat);
            self.etat.at = iso_de_ms(self.maintenant);
            self.recevoir();
        }

        fn observer(&mut self, st: EtatLecture) {
            self.e.observer(&mut self.l, st, self.maintenant);
            self.ramasser();
        }

        fn commande(&mut self, c: Commande) {
            self.e.commande(&mut self.l, c, self.maintenant);
            self.ramasser();
        }

        fn tick(&mut self) {
            self.e.tick(&mut self.l, self.maintenant);
            self.ramasser();
        }

        fn avancer(&mut self, ms: i64) {
            self.maintenant += ms;
            self.e.echeances(&mut self.l, self.maintenant);
            self.ramasser();
        }

        fn passer(&mut self, ms: i64) {
            let mut i = 0;
            while i < ms {
                self.maintenant += 500;
                self.tick();
                i += 500;
            }
        }

        fn actions(&self) -> Vec<(ActionSeance, Option<f64>)> {
            self.envoyes
                .iter()
                .map(|s| match s {
                    SortieSynchro::Envoyer {
                        action, position, ..
                    } => (*action, *position),
                    _ => unreachable!(),
                })
                .collect()
        }

        fn repondre_echec(&mut self) {
            let numero = self.e.numero;
            self.e
                .reponse(&mut self.l, numero, Err("refusé".into()), self.maintenant);
            self.ramasser();
        }
    }

    #[test]
    fn charge_la_video_puis_passe_a_synchronise_quand_le_lecteur_repond() {
        let b = Banc::sync();
        assert_eq!(b.e.phase(), PhaseSynchro::Synchro);
    }

    #[test]
    fn signale_une_video_qui_ne_se_charge_pas_et_plus_des_que_le_lecteur_repond() {
        let mut b = Banc::sync();
        b.pousser(|s| {
            s.rev = 2;
            s.video = "w".into();
        });
        assert_eq!(b.e.phase(), PhaseSynchro::Chargement);
        b.avancer(14000);
        assert!(!b.e.lent(b.maintenant));
        b.avancer(2000);
        assert!(b.e.lent(b.maintenant));
        b.observer(EtatLecture::Lecture);
        assert!(!b.e.lent(b.maintenant));
    }

    #[test]
    fn l_etat_recu_est_applique_sans_jamais_produire_de_commande() {
        let mut b = Banc::sync();
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.pos = 42.0;
            s.act = "pause".into();
        });
        assert_eq!(b.l.appels, ["pause", "seek 42"]);
        b.pousser(|s| {
            s.rev = 3;
            s.playing = true;
            s.act = "play".into();
        });
        assert!(b.l.appels.contains(&"play".to_string()));
        assert!(b.envoyes.is_empty());
    }

    #[test]
    fn une_commande_s_applique_tout_de_suite_puis_attend_la_confirmation() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        assert_eq!(b.l.appels, ["pause"]);
        assert_eq!(
            b.envoyes,
            [SortieSynchro::Envoyer {
                numero: 1,
                action: ActionSeance::Pause,
                position: Some(12.0),
                vitesse: None,
                video: "v".into()
            }]
        );
        assert_eq!(b.e.phase(), PhaseSynchro::Attente);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.pos = 12.0;
            s.act = "pause".into();
        });
        assert_eq!(b.e.phase(), PhaseSynchro::Synchro);
    }

    #[test]
    fn un_etat_plus_ancien_que_la_derniere_commande_n_est_pas_applique() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        b.l.appels.clear();
        b.pousser(|s| {
            s.rev = 1;
            s.playing = true;
            s.pos = 10.0;
        });
        assert!(b.l.appels.is_empty());
        assert_eq!(b.e.phase(), PhaseSynchro::Attente);
    }

    #[test]
    fn des_commandes_enchainees_partent_une_par_une_la_derniere_de_chaque_reglage_gardee() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        b.commande(Commande::aller(20.0));
        b.commande(Commande::aller(30.0));
        b.commande(Commande::lire(30.0));
        assert_eq!(b.actions(), [(ActionSeance::Pause, Some(12.0))]);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.pos = 12.0;
            s.act = "pause".into();
        });
        assert_eq!(
            b.actions(),
            [
                (ActionSeance::Pause, Some(12.0)),
                (ActionSeance::Lire, Some(30.0))
            ]
        );
    }

    #[test]
    fn une_commande_refusee_ramene_le_lecteur_a_l_etat_du_serveur() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        b.l.appels.clear();
        b.repondre_echec();
        assert_eq!(b.refus, 1);
        assert!(b.l.appels.contains(&"play".to_string()));
        assert_eq!(b.e.phase(), PhaseSynchro::Synchro);
        b.repondre_echec();
        assert_eq!(b.refus, 1);
    }

    #[test]
    fn une_commande_perdue_est_abandonnee_au_bout_de_quelques_secondes() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        assert_eq!(b.e.prochaine_echeance(), Some(b.maintenant + 3000));
        b.avancer(3100);
        assert_eq!(b.refus, 1);
        assert_eq!(b.e.phase(), PhaseSynchro::Synchro);
    }

    #[test]
    fn le_recalage_ne_relance_jamais_la_lecture_ne_touche_pas_la_pause_et_attend_la_commande() {
        let mut b = Banc::sync();
        b.l.st = EtatLecture::Pause;
        b.tick();
        assert!(b.l.appels.is_empty());
        b.l.st = EtatLecture::Lecture;
        b.l.t = 3.0;
        b.commande(Commande::vitesse(1.5));
        b.l.appels.clear();
        b.tick();
        assert!(b.l.appels.is_empty());
    }

    #[test]
    fn grand_ecart_par_un_saut_petit_par_un_cran_une_correction_a_la_fois() {
        let mut b = Banc::sync();
        b.l.t = 3.0;
        b.tick();
        assert_eq!(b.l.appels, ["seek 10"]);
        b.l.appels.clear();
        b.l.t = 9.5;
        b.tick();
        b.tick();
        assert_eq!(b.l.appels, ["rate 1.25"]);
        b.avancer(2000);
        assert_eq!(b.l.appels, ["rate 1.25", "rate 1"]);
    }

    #[test]
    fn les_evenements_du_lecteur_n_emettent_aucune_commande_seule_la_fin_est_signalee() {
        let mut b = Banc::sync();
        b.l.st = EtatLecture::Pause;
        b.observer(EtatLecture::Pause);
        b.observer(EtatLecture::Tampon);
        b.observer(EtatLecture::Lecture);
        assert!(b.envoyes.is_empty());
        b.observer(EtatLecture::Fin);
        assert_eq!(b.fins, [1]);
    }

    #[test]
    fn un_etat_identique_ne_relance_rien() {
        let mut b = Banc::sync();
        b.recevoir();
        b.recevoir();
        assert!(b.l.appels.is_empty());
    }

    #[test]
    fn un_changement_de_video_charge_la_nouvelle_et_abandonne_les_commandes() {
        let mut b = Banc::sync();
        b.commande(Commande::pause(12.0));
        b.pousser(|s| {
            s.rev = 2;
            s.video = "w".into();
            s.playing = true;
            s.pos = 0.0;
            s.act = "video".into();
        });
        assert!(b.l.appels.contains(&"load w".to_string()));
        assert_eq!(b.e.phase(), PhaseSynchro::Chargement);
    }

    #[test]
    fn une_tempete_d_evenements_suspend_la_session_puis_reprend() {
        let mut b = Banc::sync();
        for i in 2..40 {
            b.pousser(|s| {
                s.rev = i;
                s.pos = i as f64;
            });
        }
        assert!(b.e.suspendu());
        b.l.appels.clear();
        b.pousser(|s| s.rev = 99);
        assert!(b.l.appels.is_empty());
        b.avancer(5100);
        assert!(!b.e.suspendu());
    }

    #[test]
    fn dix_participants_restent_coherents_le_dernier_etat_l_emporte() {
        let mut b = Banc::sync();
        for i in 2..12 {
            b.pousser(|s| {
                s.rev = i;
                s.playing = i % 2 == 0;
                s.pos = i as f64;
                s.act = if i % 2 == 1 { "pause" } else { "play" }.into();
            });
            b.avancer(120);
        }
        assert_eq!(b.l.st, EtatLecture::Pause);
        assert!(b.envoyes.is_empty());
    }

    #[test]
    fn lecture_automatique_refusee_signalee_sans_relancer_le_lecteur() {
        let mut b = Banc::sync();
        b.l.st = EtatLecture::Pause;
        for _ in 0..8 {
            b.tick();
            b.avancer(500);
        }
        assert!(b.e.bloque());
        assert!(b.l.appels.is_empty());
    }

    #[test]
    fn un_morceau_repete_est_relu_depuis_le_debut() {
        let mut b = Banc::sync();
        b.pousser(|s| {
            s.rev = 2;
            s.act = "repeat".into();
            s.pos = 0.0;
        });
        assert_eq!(b.l.appels, ["load v"]);
        b.observer(EtatLecture::Lecture);
        b.l.appels.clear();
        b.pousser(|s| s.pos = 0.0);
        assert!(b.l.appels.is_empty());
    }

    #[test]
    fn un_lecteur_sans_reglage_de_vitesse_ne_rattrape_que_les_grands_ecarts() {
        struct SansVitesse(LecteurTest);
        impl LecteurSeance for SansVitesse {
            fn vitesse_reglable(&self) -> bool {
                false
            }
            fn charger(&mut self, r: &str, d: f64, l: bool) {
                self.0.charger(r, d, l)
            }
            fn lire(&mut self) {
                self.0.lire()
            }
            fn pause(&mut self) {
                self.0.pause()
            }
            fn aller(&mut self, p: f64) {
                self.0.aller(p)
            }
            fn temps(&self) -> f64 {
                self.0.temps()
            }
            fn vitesse(&self) -> f64 {
                2.0
            }
            fn regler_vitesse(&mut self, v: f64) {
                self.0.regler_vitesse(v)
            }
            fn etat(&self) -> EtatLecture {
                self.0.etat()
            }
        }
        let mut l = SansVitesse(LecteurTest::nouveau(EtatLecture::Lecture, true));
        let mut e = MoteurSynchro::nouveau(false);
        e.recevoir(&mut l, mk(), 0, base());
        e.observer(&mut l, EtatLecture::Lecture, base());
        l.0.appels.clear();
        l.0.t = 9.5;
        e.tick(&mut l, base());
        assert!(l.0.appels.is_empty());
        l.0.t = 5.0;
        e.tick(&mut l, base());
        assert_eq!(l.0.appels, ["seek 10"]);
    }

    #[test]
    fn la_pause_commune_est_demandee_quand_le_chargement_dure_une_seule_fois() {
        let mut b = Banc::cinema(true);
        b.passer(500);
        assert_eq!(b.attentes, 0);
        b.passer(1500);
        assert_eq!(b.attentes, 1);
        b.passer(5000);
        assert_eq!(b.attentes, 1);
        assert!(b.e.en_chargement(&b.l));
    }

    #[test]
    fn rien_n_est_demande_pour_une_courte_attente_de_donnees() {
        let mut b = Banc::cinema(true);
        b.l.st = EtatLecture::Lecture;
        b.observer(EtatLecture::Lecture);
        b.l.st = EtatLecture::Tampon;
        b.observer(EtatLecture::Tampon);
        b.passer(500);
        b.l.st = EtatLecture::Lecture;
        b.observer(EtatLecture::Lecture);
        b.passer(3000);
        assert_eq!(b.attentes, 0);
        assert_eq!(b.prets.last(), Some(&true));
    }

    #[test]
    fn une_fois_en_pause_ne_se_dit_pret_qu_avec_assez_de_video_chargee() {
        let mut b = Banc::cinema(true);
        b.l.st = EtatLecture::Tampon;
        b.observer(EtatLecture::Tampon);
        b.passer(1500);
        assert_eq!(b.attentes, 1);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.act = "wait".into();
            s.waiting = vec!["me".into()];
        });
        b.l.st = EtatLecture::Pause;
        b.observer(EtatLecture::Pause);
        b.passer(2000);
        assert!(!b.prets.contains(&true));
        b.l.pret = Some(true);
        b.passer(500);
        assert_eq!(b.prets.last(), Some(&true));
    }

    #[test]
    fn la_pause_commune_arrivee_pendant_son_chargement_est_appliquee() {
        let mut b = Banc::cinema(true);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.act = "wait".into();
            s.waiting = vec!["bob".into()];
        });
        b.l.pret = Some(true);
        b.l.st = EtatLecture::Lecture;
        b.observer(EtatLecture::Lecture);
        assert!(b.l.appels.contains(&"pause".to_string()));
    }

    #[test]
    fn rejoint_l_attente_en_cours_si_son_lecteur_charge_a_son_tour() {
        let mut b = Banc::cinema(true);
        b.l.pret = Some(true);
        b.l.st = EtatLecture::Pause;
        b.observer(EtatLecture::Pause);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.act = "wait".into();
            s.waiting = vec!["bob".into()];
        });
        b.l.st = EtatLecture::Tampon;
        b.observer(EtatLecture::Tampon);
        b.passer(1500);
        assert_eq!(b.attentes, 1);
    }

    #[test]
    fn rien_n_est_demande_en_pause_voulue_ni_hors_du_salon_cinema() {
        let mut b = Banc::cinema(true);
        b.pousser(|s| {
            s.rev = 2;
            s.playing = false;
            s.act = "pause".into();
        });
        b.passer(3000);
        assert_eq!(b.attentes, 0);
        let mut u = Banc::cinema(false);
        u.passer(3000);
        assert_eq!(u.attentes, 0);
    }
}
