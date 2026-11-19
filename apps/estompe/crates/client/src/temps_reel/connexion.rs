use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::BuildHasher;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, sleep_until, Instant};

use super::evenements::{decoder, Evenement, Trame};
use super::reseau::{CoupureDifferee, EtatReseau, StatutConnexion, BANDEAU_APRES};
use super::transport::{Avenir, Canal, TrameRecue, Transport, TransportWebSocket};
use crate::http::{encoder_composant, Client, Corps, ErreurApi, OptionsAppel};
use crate::journal::{Journal, Niveau};
use crate::types::PostWsTicketReponse;

pub const PING_TOUTES_LES: Duration = Duration::from_secs(10);
pub const MORTE_APRES: Duration = Duration::from_secs(25);
pub const ATTENTE_MAX: Duration = Duration::from_secs(5);
pub const RENOUVELER_TOUTES_LES: Duration = Duration::from_secs(60);
pub const SESSION_EXPIREE: u16 = 4401;
pub const ACCES_RETIRE: u16 = 4403;
pub const FERMETURE_NORMALE: u16 = 1000;
pub const TRAME_PING: &str = r#"{"type":"ping"}"#;

const SOURCE_JOURNAL: &str = "temps réel";

pub trait SourceTicket: Send + Sync + 'static {
    fn ticket(&self) -> Avenir<'static, Result<String, ErreurApi>>;
}

impl SourceTicket for Client {
    fn ticket(&self) -> Avenir<'static, Result<String, ErreurApi>> {
        let client = self.clone();
        Box::pin(async move {
            client
                .requete::<PostWsTicketReponse>(
                    Method::POST,
                    "/ws-ticket",
                    &Corps::Aucun,
                    OptionsAppel::default(),
                )
                .await?
                .and_then(|r| r.ticket)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| ErreurApi::ReponseInvalide("ticket absent".to_string()))
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Statut(StatutConnexion),
    Reseau(EtatReseau),
    Evenement(Box<Evenement>),
    Reconnecte,
    SessionExpiree,
    AccesRetire,
}

type Alea = Arc<dyn Fn() -> f64 + Send + Sync>;

pub struct OptionsTempsReel {
    adresse: String,
    tickets: Arc<dyn SourceTicket>,
    transport: Arc<dyn Transport>,
    alea: Alea,
    journal: Option<Arc<Journal>>,
}

impl OptionsTempsReel {
    pub fn nouvelles(adresse_ws: impl Into<String>, tickets: impl SourceTicket) -> Self {
        Self {
            adresse: adresse_ws.into(),
            tickets: Arc::new(tickets),
            transport: Arc::new(TransportWebSocket::nouveau()),
            alea: Arc::new(alea_systeme),
            journal: None,
        }
    }

    pub fn avec_transport(mut self, transport: impl Transport) -> Self {
        self.transport = Arc::new(transport);
        self
    }

    pub fn avec_alea(mut self, alea: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
        self.alea = Arc::new(alea);
        self
    }

    pub fn avec_journal(mut self, journal: Arc<Journal>) -> Self {
        self.journal = Some(journal);
        self
    }
}

pub fn adresse_ws(adresse_api: &str, origine: &str) -> String {
    let base = if adresse_api.is_empty() {
        origine
    } else {
        adresse_api
    };
    let base = match base.strip_prefix("http") {
        Some(reste) => format!("ws{reste}"),
        None => base.to_string(),
    };
    format!("{}/ws", base.strip_suffix('/').unwrap_or(&base))
}

pub fn adresse_avec_ticket(adresse: &str, ticket: &str) -> String {
    let separateur = if adresse.contains('?') { '&' } else { '?' };
    format!("{adresse}{separateur}ticket={}", encoder_composant(ticket))
}

pub fn attente_de_base(tentative: u32) -> Duration {
    match tentative {
        0 => Duration::ZERO,
        n => {
            Duration::from_millis(1000u64.saturating_mul(1u64 << (n - 1).min(16))).min(ATTENTE_MAX)
        }
    }
}

pub fn attente_avant_essai(tentative: u32, alea: f64) -> Duration {
    let base = attente_de_base(tentative);
    base / 2 + (base / 2).mul_f64(alea.clamp(0.0, 1.0))
}

fn alea_systeme() -> f64 {
    let bruit = RandomState::new().hash_one(std::time::Instant::now());
    (bruit >> 11) as f64 / (1u64 << 53) as f64
}

pub struct FluxTempsReel(mpsc::UnboundedReceiver<Signal>);

impl FluxTempsReel {
    pub async fn suivant(&mut self) -> Option<Signal> {
        self.0.recv().await
    }

    pub fn essayer(&mut self) -> Option<Signal> {
        self.0.try_recv().ok()
    }

    #[cfg(test)]
    pub(crate) fn factice() -> (mpsc::UnboundedSender<Signal>, Self) {
        let (entree, sortie) = mpsc::unbounded_channel();
        (entree, Self(sortie))
    }
}

enum Commande {
    Relancer,
    Arreter,
}

pub struct TempsReel {
    commandes: mpsc::UnboundedSender<Commande>,
    reseau: watch::Receiver<EtatReseau>,
    tache: JoinHandle<()>,
}

impl TempsReel {
    pub fn demarrer(options: OptionsTempsReel) -> (Self, FluxTempsReel) {
        let (commandes, recues) = mpsc::unbounded_channel();
        let (sortie, flux) = mpsc::unbounded_channel();
        let (reseau, etat) = watch::channel(EtatReseau::Reconnexion);
        let acteur = Acteur {
            adresse: options.adresse,
            tickets: options.tickets,
            transport: options.transport,
            alea: options.alea,
            journal: options.journal,
            commandes: recues,
            sortie,
            reseau,
            statut: StatutConnexion::Fermee,
            coupure: CoupureDifferee::nouvelle(BANDEAU_APRES),
            tentative: 0,
            deja_ouverte: false,
        };
        let tache = tokio::spawn(acteur.executer());
        (
            Self {
                commandes,
                reseau: etat,
                tache,
            },
            FluxTempsReel(flux),
        )
    }

    pub fn avec_jeton(
        adresse_api: &str,
        fournisseur: impl Fn() -> Option<String> + Send + Sync + 'static,
    ) -> Result<(Self, FluxTempsReel), ErreurApi> {
        let client = Client::nouveau(adresse_api)?.avec_jeton(fournisseur);
        let adresse = adresse_ws(client.adresse(), "");
        Ok(Self::demarrer(OptionsTempsReel::nouvelles(adresse, client)))
    }

    pub fn reseau(&self) -> watch::Receiver<EtatReseau> {
        self.reseau.clone()
    }

    pub fn relancer(&self) {
        let _ = self.commandes.send(Commande::Relancer);
    }

    pub async fn arreter(self) {
        let _ = self.commandes.send(Commande::Arreter);
        let _ = self.tache.await;
    }
}

enum Interruption {
    Arret,
    Relance,
}

enum Fin {
    Arret,
    SessionExpiree,
    AccesRetire,
}

enum Coupure {
    Arret,
    Code(Option<u16>),
}

struct Acteur {
    adresse: String,
    tickets: Arc<dyn SourceTicket>,
    transport: Arc<dyn Transport>,
    alea: Alea,
    journal: Option<Arc<Journal>>,
    commandes: mpsc::UnboundedReceiver<Commande>,
    sortie: mpsc::UnboundedSender<Signal>,
    reseau: watch::Sender<EtatReseau>,
    statut: StatutConnexion,
    coupure: CoupureDifferee,
    tentative: u32,
    deja_ouverte: bool,
}

async fn dormir_jusqua(echeance: Option<Instant>) {
    match echeance {
        Some(echeance) => sleep_until(echeance).await,
        None => std::future::pending().await,
    }
}

impl Acteur {
    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = &self.journal {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }

    fn emettre(&self, signal: Signal) {
        let _ = self.sortie.send(signal);
    }

    fn etat_reseau(&self, terminal: bool) -> EtatReseau {
        if terminal {
            EtatReseau::HorsLigne
        } else if self.statut == StatutConnexion::Ouverte {
            EtatReseau::Connecte
        } else if self.coupure.coupe() {
            EtatReseau::HorsLigne
        } else {
            EtatReseau::Reconnexion
        }
    }

    fn publier_reseau(&self, terminal: bool) {
        let etat = self.etat_reseau(terminal);
        if self
            .reseau
            .send_if_modified(|e| std::mem::replace(e, etat) != etat)
        {
            self.emettre(Signal::Reseau(etat));
        }
    }

    fn statut(&mut self, statut: StatutConnexion) {
        self.statut = statut;
        self.emettre(Signal::Statut(statut));
        self.coupure.signaler(Some(statut), Instant::now());
        self.publier_reseau(false);
    }

    fn echeance_coupure(&mut self) {
        self.coupure.avancer(Instant::now());
        self.publier_reseau(false);
    }

    async fn attendre<T>(
        &mut self,
        avenir: impl Future<Output = T>,
        relance_possible: bool,
    ) -> Result<T, Interruption> {
        tokio::pin!(avenir);
        loop {
            let echeance = self.coupure.echeance();
            tokio::select! {
                biased;
                commande = self.commandes.recv() => match commande {
                    Some(Commande::Relancer) if relance_possible => return Err(Interruption::Relance),
                    Some(Commande::Relancer) => {}
                    Some(Commande::Arreter) | None => return Err(Interruption::Arret),
                },
                _ = dormir_jusqua(echeance) => self.echeance_coupure(),
                valeur = &mut avenir => return Ok(valeur),
            }
        }
    }

    async fn patienter(&mut self) -> Result<(), Interruption> {
        let delai = attente_avant_essai(self.tentative, (self.alea)());
        self.tentative = self.tentative.saturating_add(1);
        match self.attendre(sleep(delai), true).await {
            Ok(()) => Ok(()),
            Err(Interruption::Relance) => {
                self.tentative = 0;
                Ok(())
            }
            Err(Interruption::Arret) => Err(Interruption::Arret),
        }
    }

    async fn executer(mut self) {
        let fin = self.boucle().await;
        if self.statut != StatutConnexion::Fermee {
            self.statut = StatutConnexion::Fermee;
            self.emettre(Signal::Statut(StatutConnexion::Fermee));
        }
        self.coupure.signaler(None, Instant::now());
        self.publier_reseau(true);
        match fin {
            Fin::Arret => {}
            Fin::SessionExpiree => self.emettre(Signal::SessionExpiree),
            Fin::AccesRetire => self.emettre(Signal::AccesRetire),
        }
    }

    async fn boucle(&mut self) -> Fin {
        loop {
            self.statut(StatutConnexion::Connexion);
            let ticket = match self.attendre(self.tickets.ticket(), false).await {
                Err(_) => return Fin::Arret,
                Ok(Err(erreur)) if erreur.statut() == 401 => return Fin::SessionExpiree,
                Ok(Err(erreur)) => {
                    self.noter(Niveau::Avertissement, &format!("ticket refusé : {erreur}"));
                    if self.patienter().await.is_err() {
                        return Fin::Arret;
                    }
                    continue;
                }
                Ok(Ok(ticket)) => ticket,
            };
            let adresse = adresse_avec_ticket(&self.adresse, &ticket);
            let coupure = match self.attendre(self.transport.ouvrir(adresse), false).await {
                Err(_) => return Fin::Arret,
                Ok(Err(erreur)) => {
                    self.noter(
                        Niveau::Avertissement,
                        &format!("ouverture impossible : {erreur}"),
                    );
                    Coupure::Code(None)
                }
                Ok(Ok(canal)) => self.connectee(canal).await,
            };
            match coupure {
                Coupure::Arret => return Fin::Arret,
                Coupure::Code(Some(SESSION_EXPIREE)) => return Fin::SessionExpiree,
                Coupure::Code(Some(ACCES_RETIRE)) => return Fin::AccesRetire,
                Coupure::Code(code) => {
                    self.statut(StatutConnexion::Fermee);
                    self.noter(
                        Niveau::Avertissement,
                        &format!("WebSocket fermé (code {code:?})"),
                    );
                    if self.patienter().await.is_err() {
                        return Fin::Arret;
                    }
                }
            }
        }
    }

    fn recevoir_texte(&self, texte: &str) {
        match decoder(texte) {
            Trame::Evenement(e) if matches!(*e, Evenement::Pong | Evenement::Renouvele) => {}
            Trame::Evenement(evenement) => self.emettre(Signal::Evenement(evenement)),
            Trame::Inconnue(type_evenement) => self.noter(
                Niveau::Info,
                &format!("évènement inconnu ignoré : {type_evenement}"),
            ),
            Trame::Illisible(erreur) => self.noter(
                Niveau::Avertissement,
                &format!("trame illisible ignorée : {erreur}"),
            ),
        }
    }

    async fn connectee(&mut self, mut canal: Box<dyn Canal>) -> Coupure {
        self.tentative = 0;
        self.statut(StatutConnexion::Ouverte);
        if self.deja_ouverte {
            self.emettre(Signal::Reconnecte);
        }
        self.deja_ouverte = true;
        let ouverture = Instant::now();
        let mut dernier_vu = ouverture;
        let mut ping = ouverture + PING_TOUTES_LES;
        let mut renouvellement = ouverture + RENOUVELER_TOUTES_LES;
        let mut demande: Option<Avenir<'static, Result<String, ErreurApi>>> = None;
        loop {
            tokio::select! {
                biased;
                commande = self.commandes.recv() => match commande {
                    Some(Commande::Relancer) => {
                        if dernier_vu.elapsed() > PING_TOUTES_LES
                            && canal.envoyer(TRAME_PING.to_string()).await.is_err()
                        {
                            return Coupure::Code(None);
                        }
                    }
                    Some(Commande::Arreter) | None => {
                        canal.fermer(FERMETURE_NORMALE).await;
                        return Coupure::Arret;
                    }
                },
                trame = canal.recevoir() => {
                    dernier_vu = Instant::now();
                    match trame {
                        TrameRecue::Texte(texte) => self.recevoir_texte(&texte),
                        TrameRecue::Fermeture(code) => return Coupure::Code(code),
                    }
                }
                _ = sleep_until(ping) => {
                    ping += PING_TOUTES_LES;
                    if dernier_vu.elapsed() > MORTE_APRES {
                        canal.fermer(FERMETURE_NORMALE).await;
                        return Coupure::Code(None);
                    }
                    if canal.envoyer(TRAME_PING.to_string()).await.is_err() {
                        return Coupure::Code(None);
                    }
                }
                _ = sleep_until(renouvellement) => {
                    renouvellement += RENOUVELER_TOUTES_LES;
                    if demande.is_none() {
                        demande = Some(self.tickets.ticket());
                    }
                }
                resultat = async { demande.as_mut().expect("demande en cours").await }, if demande.is_some() => {
                    demande = None;
                    if let Ok(ticket) = resultat {
                        let trame = json!({ "type": "renew", "data": { "ticket": ticket } }).to_string();
                        if canal.envoyer(trame).await.is_err() {
                            return Coupure::Code(None);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use serde_json::{json, Value};
    use tokio::sync::{mpsc::UnboundedReceiver, oneshot};

    use super::super::faux_transport::{faux_transport, Essai, FauxWs, Sortant};
    use super::super::serveur_ws_factice::ServeurWs;
    use super::*;
    use crate::http::Refus;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    type Fabrique = Box<dyn Fn(usize) -> Avenir<'static, Result<String, ErreurApi>> + Send + Sync>;

    struct FauxTickets {
        n: Arc<AtomicUsize>,
        fabrique: Fabrique,
    }

    impl SourceTicket for FauxTickets {
        fn ticket(&self) -> Avenir<'static, Result<String, ErreurApi>> {
            let n = self.n.fetch_add(1, Ordering::SeqCst) + 1;
            (self.fabrique)(n)
        }
    }

    fn numerotes() -> Fabrique {
        Box::new(|n| Box::pin(async move { Ok(format!("t{n}")) }))
    }

    fn refus_401() -> ErreurApi {
        ErreurApi::NonAutorise(Refus {
            statut: 401,
            code: "unauthorized".into(),
            message: String::new(),
            donnees: Default::default(),
        })
    }

    struct Banc {
        rt: Option<TempsReel>,
        flux: FluxTempsReel,
        essais: UnboundedReceiver<Essai>,
        tickets: Arc<AtomicUsize>,
    }

    impl Banc {
        fn rt(&self) -> &TempsReel {
            self.rt.as_ref().unwrap()
        }

        async fn essai(&mut self) -> Essai {
            self.essais.recv().await.unwrap()
        }

        fn aucun_essai(&mut self) -> bool {
            self.essais.try_recv().is_err()
        }

        fn signaux(&mut self) -> Vec<Signal> {
            let mut signaux = Vec::new();
            while let Some(s) = self.flux.essayer() {
                signaux.push(s);
            }
            signaux
        }

        fn statuts(&mut self) -> Vec<StatutConnexion> {
            statuts(&self.signaux())
        }

        async fn arreter(&mut self) {
            self.rt.take().unwrap().arreter().await;
        }
    }

    fn statuts(signaux: &[Signal]) -> Vec<StatutConnexion> {
        signaux
            .iter()
            .filter_map(|s| match s {
                Signal::Statut(st) => Some(*st),
                _ => None,
            })
            .collect()
    }

    fn types(signaux: &[Signal]) -> Vec<&'static str> {
        signaux
            .iter()
            .filter_map(|s| match s {
                Signal::Evenement(e) => Some(e.type_evenement()),
                _ => None,
            })
            .collect()
    }

    fn monter_avec(adresse: &str, fabrique: Fabrique) -> Banc {
        let (transport, essais) = faux_transport();
        let n = Arc::new(AtomicUsize::new(0));
        let tickets = FauxTickets {
            n: n.clone(),
            fabrique,
        };
        let options = OptionsTempsReel::nouvelles(adresse, tickets)
            .avec_transport(transport)
            .avec_alea(|| 1.0);
        let (rt, flux) = TempsReel::demarrer(options);
        Banc {
            rt: Some(rt),
            flux,
            essais,
            tickets: n,
        }
    }

    fn monter() -> Banc {
        monter_avec("wss://chat.example/ws", numerotes())
    }

    async fn avancer(duree: Duration) {
        sleep(duree).await;
    }

    async fn reglage() {
        avancer(Duration::from_millis(1)).await;
    }

    async fn vivant(ws: &FauxWs, duree: Duration) {
        let mut ecoule = Duration::ZERO;
        while ecoule < duree {
            avancer(PING_TOUTES_LES).await;
            ws.recevoir(json!({"type": "pong", "data": null}));
            ecoule += PING_TOUTES_LES;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ticket_ajoute_a_une_adresse_qui_a_deja_des_parametres() {
        let mut b = monter_avec(
            "wss://chat.example/ws?platform=windows&v=1.0.0",
            numerotes(),
        );
        assert_eq!(
            b.essai().await.adresse,
            "wss://chat.example/ws?platform=windows&v=1.0.0&ticket=t1"
        );
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn se_connecte_avec_un_ticket_et_transmet_les_evenements_sauf_pong() {
        let mut b = monter();
        let essai = b.essai().await;
        assert_eq!(essai.adresse, "wss://chat.example/ws?ticket=t1");
        let ws = essai.ouvrir();
        ws.recevoir(json!({"type": "ready", "data": {"user_id": "u1"}}));
        ws.recevoir(json!({"type": "pong", "data": null}));
        ws.recevoir(json!({"type": "renewed", "data": null}));
        ws.recevoir(json!({"type": "message.created", "data": {"id": "1"}}));
        reglage().await;
        let signaux = b.signaux();
        assert_eq!(types(&signaux), ["ready", "message.created"]);
        assert_eq!(
            statuts(&signaux),
            [StatutConnexion::Connexion, StatutConnexion::Ouverte]
        );
        assert!(matches!(
            signaux.iter().find(|s| matches!(s, Signal::Evenement(_))),
            Some(Signal::Evenement(e)) if matches!(&**e, Evenement::Pret(p) if p.user_id == "u1")
        ));
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn se_reconnecte_avec_un_nouveau_ticket_et_signale_la_reconnexion() {
        let mut b = monter();
        let ws = b.essai().await.ouvrir();
        reglage().await;
        assert!(!b.signaux().contains(&Signal::Reconnecte));
        let coupure = Instant::now();
        ws.couper(1006);
        let essai = b.essai().await;
        assert_eq!(Instant::now(), coupure);
        assert!(essai.adresse.ends_with("ticket=t2"));
        let _ws = essai.ouvrir();
        reglage().await;
        let signaux = b.signaux();
        assert_eq!(
            signaux.iter().filter(|s| **s == Signal::Reconnecte).count(),
            1
        );
        let ouverte = signaux
            .iter()
            .position(|s| *s == Signal::Statut(StatutConnexion::Ouverte))
            .unwrap();
        assert_eq!(
            signaux[ouverte + 1..].first(),
            Some(&Signal::Reseau(EtatReseau::Connecte))
        );
        assert!(signaux[ouverte..].contains(&Signal::Reconnecte));
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn reessaie_aussitot_puis_attend_de_plus_en_plus_plafonne_a_5_s() {
        let mut b = monter();
        let mut essai = b.essai().await;
        let mut attentes = Vec::new();
        for _ in 0..8 {
            let avant = Instant::now();
            essai.refuser();
            essai = b.essai().await;
            attentes.push(Instant::now() - avant);
        }
        let secondes: Vec<u64> = attentes.iter().map(|d| d.as_millis() as u64).collect();
        assert_eq!(secondes, [0, 1000, 2000, 4000, 5000, 5000, 5000, 5000]);
        assert!(attentes[3] > attentes[1]);
        assert!(attentes.iter().all(|a| *a <= ATTENTE_MAX));
        b.arreter().await;
    }

    #[test]
    fn paliers_d_attente_et_part_aleatoire() {
        let paliers: Vec<u128> = (0..7).map(|t| attente_de_base(t).as_millis()).collect();
        assert_eq!(paliers, [0, 1000, 2000, 4000, 5000, 5000, 5000]);
        assert_eq!(attente_de_base(u32::MAX), ATTENTE_MAX);
        assert_eq!(attente_avant_essai(2, 0.0), Duration::from_millis(1000));
        assert_eq!(attente_avant_essai(2, 0.5), Duration::from_millis(1500));
        assert_eq!(attente_avant_essai(2, 1.0), Duration::from_millis(2000));
        assert_eq!(attente_avant_essai(0, 1.0), Duration::ZERO);
        for _ in 0..100 {
            let alea = alea_systeme();
            assert!((0.0..1.0).contains(&alea));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ne_reboucle_pas_quand_la_session_a_expire() {
        let mut b = monter();
        let ws = b.essai().await.ouvrir();
        ws.couper(SESSION_EXPIREE);
        avancer(Duration::from_secs(60)).await;
        let signaux = b.signaux();
        assert_eq!(
            signaux
                .iter()
                .filter(|s| **s == Signal::SessionExpiree)
                .count(),
            1
        );
        assert_eq!(signaux.last(), Some(&Signal::SessionExpiree));
        assert!(b.aucun_essai());
        assert_eq!(*b.rt().reseau().borrow(), EtatReseau::HorsLigne);
    }

    #[tokio::test(start_paused = true)]
    async fn ne_reboucle_pas_quand_l_acces_a_ete_retire() {
        let mut b = monter();
        let ws = b.essai().await.ouvrir();
        ws.couper(ACCES_RETIRE);
        avancer(Duration::from_secs(60)).await;
        let signaux = b.signaux();
        assert_eq!(
            signaux
                .iter()
                .filter(|s| **s == Signal::AccesRetire)
                .count(),
            1
        );
        assert!(!signaux.contains(&Signal::SessionExpiree));
        assert!(b.aucun_essai());
    }

    #[tokio::test(start_paused = true)]
    async fn arrete_tout_si_le_ticket_est_refuse_en_401() {
        let mut b = monter_avec(
            "wss://chat.example/ws",
            Box::new(|_| Box::pin(async { Err(refus_401()) })),
        );
        avancer(Duration::from_secs(60)).await;
        let signaux = b.signaux();
        assert!(signaux.contains(&Signal::SessionExpiree));
        assert!(b.aucun_essai());
        assert_eq!(b.tickets.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reessaie_quand_le_ticket_echoue_pour_une_autre_raison() {
        let mut b = monter_avec(
            "wss://chat.example/ws",
            Box::new(|n| {
                Box::pin(async move {
                    if n == 1 {
                        Err(ErreurApi::Reseau("réseau".into()))
                    } else {
                        Ok("ok".to_string())
                    }
                })
            }),
        );
        assert!(b.essai().await.adresse.ends_with("ticket=ok"));
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn envoie_un_ping_regulier_et_coupe_une_connexion_silencieuse() {
        let mut b = monter();
        let mut ws = b.essai().await.ouvrir();
        reglage().await;
        avancer(PING_TOUTES_LES).await;
        assert_eq!(ws.textes(), [TRAME_PING]);
        ws.recevoir(json!({"type": "pong", "data": null}));
        avancer(Duration::from_secs(20)).await;
        assert!(b.aucun_essai());
        assert_eq!(ws.textes(), [TRAME_PING, TRAME_PING]);
        avancer(Duration::from_secs(10)).await;
        assert_eq!(ws.sortants(), [Sortant::Fermeture(FERMETURE_NORMALE)]);
        let _ = b.essai().await;
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn retente_aussitot_au_retour_du_reseau() {
        let mut b = monter();
        let mut essai = b.essai().await;
        for _ in 0..4 {
            essai.refuser();
            essai = b.essai().await;
        }
        essai.refuser();
        reglage().await;
        assert!(b.aucun_essai());
        let avant = Instant::now();
        b.rt().relancer();
        let essai = b.essai().await;
        assert_eq!(Instant::now(), avant);
        let mut ws = essai.ouvrir();
        reglage().await;
        b.rt().relancer();
        reglage().await;
        assert!(ws.textes().is_empty());
        avancer(PING_TOUTES_LES - Duration::from_millis(2)).await;
        ws.recevoir(json!({"type": "pong", "data": null}));
        avancer(PING_TOUTES_LES).await;
        assert_eq!(ws.textes(), [TRAME_PING]);
        b.rt().relancer();
        reglage().await;
        assert_eq!(ws.textes(), [TRAME_PING]);
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn renouvelle_la_session_sur_la_connexion_ouverte_sans_la_fermer() {
        let mut b = monter();
        let mut ws = b.essai().await.ouvrir();
        reglage().await;
        assert_eq!(
            b.statuts(),
            [StatutConnexion::Connexion, StatutConnexion::Ouverte]
        );
        vivant(&ws, RENOUVELER_TOUTES_LES).await;
        assert!(ws
            .textes()
            .contains(&r#"{"data":{"ticket":"t2"},"type":"renew"}"#.to_string()));
        ws.recevoir(json!({"type": "renewed", "data": null}));
        vivant(&ws, RENOUVELER_TOUTES_LES).await;
        let textes: Vec<Value> = ws
            .textes()
            .iter()
            .map(|t| serde_json::from_str(t).unwrap())
            .collect();
        assert!(textes.contains(&json!({"type": "renew", "data": {"ticket": "t3"}})));
        assert!(b.aucun_essai());
        assert!(b.signaux().is_empty());
        b.arreter().await;
    }

    #[tokio::test(start_paused = true)]
    async fn n_envoie_pas_de_renouvellement_si_le_ticket_echoue_ou_si_la_connexion_est_fermee() {
        let (liberer, attente) = oneshot::channel::<String>();
        let attente = Arc::new(Mutex::new(Some(attente)));
        let mut b = monter_avec(
            "wss://chat.example/ws",
            Box::new(move |n| {
                let attente = attente.clone();
                Box::pin(async move {
                    match n {
                        2 => Err(ErreurApi::Reseau("réseau".into())),
                        3 => {
                            let recepteur = attente.lock().unwrap().take().unwrap();
                            recepteur
                                .await
                                .map_err(|_| ErreurApi::Reseau("abandon".into()))
                        }
                        n => Ok(format!("t{n}")),
                    }
                })
            }),
        );
        let mut ws = b.essai().await.ouvrir();
        reglage().await;
        vivant(&ws, 2 * RENOUVELER_TOUTES_LES).await;
        assert_eq!(b.tickets.load(Ordering::SeqCst), 3);
        ws.couper(1006);
        reglage().await;
        let _ = liberer.send("tardif".into());
        reglage().await;
        assert!(ws.textes().iter().all(|t| !t.contains("renew")));
        b.arreter().await;
    }

    #[tokio::test]
    async fn renouvelle_le_jeton_avant_chaque_ticket() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(200, json!({"ticket": "t1"})),
            Reponse::json(200, json!({"ticket": "t2"})),
        ])
        .await;
        let jetons = Arc::new(AtomicUsize::new(0));
        let partage = jetons.clone();
        let client = serveur
            .client()
            .avec_jeton(move || Some(format!("jeton{}", partage.fetch_add(1, Ordering::SeqCst))));
        assert_eq!(client.ticket().await.unwrap(), "t1");
        assert_eq!(client.ticket().await.unwrap(), "t2");
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].ligne(), ("/ws-ticket", "POST"));
        assert_eq!(requetes[0].entete("authorization"), Some("Bearer jeton0"));
        assert_eq!(requetes[1].entete("authorization"), Some("Bearer jeton1"));
        serveur.programmer([
            Reponse::json(401, json!({"error": "unauthorized"})),
            Reponse::json(401, json!({"error": "unauthorized"})),
        ]);
        assert_eq!(client.ticket().await.unwrap_err().statut(), 401);
        serveur.programmer([Reponse::json(200, json!({}))]);
        assert!(matches!(
            client.ticket().await,
            Err(ErreurApi::ReponseInvalide(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn arreter_ferme_et_empeche_toute_reconnexion() {
        let mut b = monter();
        let mut ws = b.essai().await.ouvrir();
        reglage().await;
        b.arreter().await;
        assert_eq!(ws.sortants(), [Sortant::Fermeture(FERMETURE_NORMALE)]);
        avancer(Duration::from_secs(60)).await;
        assert!(b.aucun_essai());
        let signaux = b.signaux();
        assert_eq!(statuts(&signaux).last(), Some(&StatutConnexion::Fermee));
        assert_eq!(signaux.last(), Some(&Signal::Reseau(EtatReseau::HorsLigne)));
    }

    #[tokio::test(start_paused = true)]
    async fn etat_du_reseau_connecte_reconnexion_puis_hors_ligne() {
        let mut b = monter();
        let reseau = b.rt().reseau();
        assert_eq!(*reseau.borrow(), EtatReseau::Reconnexion);
        let ws = b.essai().await.ouvrir();
        reglage().await;
        assert_eq!(*reseau.borrow(), EtatReseau::Connecte);
        drop(ws);
        let mut essai = b.essai().await;
        assert_eq!(*reseau.borrow(), EtatReseau::Reconnexion);
        let coupure = Instant::now();
        while Instant::now() - coupure < BANDEAU_APRES {
            essai.refuser();
            essai = b.essai().await;
        }
        assert_eq!(*reseau.borrow(), EtatReseau::HorsLigne);
        let _ws = essai.ouvrir();
        reglage().await;
        assert_eq!(*reseau.borrow(), EtatReseau::Connecte);
        let changements: Vec<EtatReseau> = b
            .signaux()
            .into_iter()
            .filter_map(|s| match s {
                Signal::Reseau(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(
            changements,
            [
                EtatReseau::Connecte,
                EtatReseau::Reconnexion,
                EtatReseau::HorsLigne,
                EtatReseau::Connecte
            ]
        );
        b.arreter().await;
    }

    #[test]
    fn adresse_ws_derivee_de_l_api() {
        assert_eq!(
            adresse_ws("https://chat.plbls.fr", ""),
            "wss://chat.plbls.fr/ws"
        );
        assert_eq!(
            adresse_ws("http://localhost:8080/", ""),
            "ws://localhost:8080/ws"
        );
        assert_eq!(
            adresse_ws("", "https://estompe.plbls.fr"),
            "wss://estompe.plbls.fr/ws"
        );
        assert_eq!(
            adresse_avec_ticket("wss://h/ws", "a b"),
            "wss://h/ws?ticket=a%20b"
        );
    }

    async fn attendre_signal(
        flux: &mut FluxTempsReel,
        voulu: impl Fn(&Signal) -> bool,
    ) -> Vec<Signal> {
        let mut vus = Vec::new();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let signal = flux.suivant().await.expect("flux fermé");
                let fini = voulu(&signal);
                vus.push(signal);
                if fini {
                    return;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("signal attendu absent, reçus : {vus:?}"));
        vus
    }

    fn message(id: &str) -> Value {
        json!({"type": "message.created", "data": {"id": id, "channel_id": "c1"}})
    }

    fn ids(signaux: &[Signal]) -> Vec<String> {
        signaux
            .iter()
            .filter_map(|s| match s {
                Signal::Evenement(e) => match &**e {
                    Evenement::MessageCree(m) => m.id.clone(),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    async fn monter_reel() -> (ServeurFactice, ServeurWs, TempsReel, FluxTempsReel) {
        let tickets = (1..=20).map(|n| Reponse::json(200, json!({"ticket": format!("t{n}")})));
        let http = ServeurFactice::demarrer(tickets).await;
        let ws = ServeurWs::demarrer().await;
        let client = http.client().avec_jeton(|| Some("jeton".into()));
        let options = OptionsTempsReel::nouvelles(ws.adresse(), client).avec_alea(|| 0.0);
        let (rt, flux) = TempsReel::demarrer(options);
        (http, ws, rt, flux)
    }

    #[tokio::test]
    async fn c2_serveur_coupe_puis_retabli_reconnexion_et_reprise_sans_doublon_ni_perte() {
        let (http, serveur, rt, mut flux) = monter_reel().await;
        let mut recus = attendre_signal(&mut flux, |s| {
            *s == Signal::Statut(StatutConnexion::Ouverte)
        })
        .await;
        serveur.attendre(|e| e.connexions == 1).await;
        for id in ["1", "2", "3"] {
            serveur.publier(message(id));
        }
        recus.extend(attendre_signal(&mut flux, |s| ids(std::slice::from_ref(s)) == ["3"]).await);
        serveur.couper();
        recus.extend(
            attendre_signal(&mut flux, |s| *s == Signal::Statut(StatutConnexion::Fermee)).await,
        );
        serveur.attendre(|e| e.refusees >= 2).await;
        assert_eq!(*rt.reseau().borrow(), EtatReseau::Reconnexion);
        serveur.retablir();
        recus.extend(attendre_signal(&mut flux, |s| *s == Signal::Reconnecte).await);
        let etat = serveur.attendre(|e| e.connexions == 2).await;
        assert_eq!(*rt.reseau().borrow(), EtatReseau::Connecte);
        for id in ["4", "5"] {
            serveur.publier(message(id));
        }
        recus.extend(attendre_signal(&mut flux, |s| ids(std::slice::from_ref(s)) == ["5"]).await);
        rt.arreter().await;
        while let Some(s) = flux.essayer() {
            recus.push(s);
        }
        assert_eq!(ids(&recus), ["1", "2", "3", "4", "5"]);
        assert_eq!(
            recus.iter().filter(|s| **s == Signal::Reconnecte).count(),
            1
        );
        let reconnecte = recus.iter().position(|s| *s == Signal::Reconnecte).unwrap();
        assert_eq!(ids(&recus[..reconnecte]), ["1", "2", "3"]);
        assert_eq!(ids(&recus[reconnecte..]), ["4", "5"]);
        assert_eq!(etat.tickets.len(), 2);
        assert_ne!(etat.tickets[0], etat.tickets[1]);
        let tickets_http = http.requetes().len();
        assert_eq!(tickets_http, 2 + etat.refusees);
        assert!(http
            .requetes()
            .iter()
            .all(|r| r.entete("authorization") == Some("Bearer jeton")));
        println!(
            "C2 : évènements reçus {:?}, reconnexions signalées 1, connexions {} (tickets {:?}), essais refusés pendant la coupure {}",
            ids(&recus),
            etat.connexions,
            etat.tickets,
            etat.refusees
        );
    }

    #[tokio::test]
    async fn c3_type_d_evenement_inconnu_ne_fait_pas_tomber_la_connexion() {
        let (_http, serveur, rt, mut flux) = monter_reel().await;
        attendre_signal(&mut flux, |s| {
            *s == Signal::Statut(StatutConnexion::Ouverte)
        })
        .await;
        serveur.attendre(|e| e.connexions == 1).await;
        serveur.publier(message("1"));
        serveur.publier(json!({"type": "evenement.futur", "data": {"x": [1, 2, 3]}}));
        serveur.publier(json!({"type": "presence", "data": {"user_id": 5}}));
        serveur.publier(json!("pas une enveloppe"));
        serveur.publier(message("2"));
        let recus = attendre_signal(&mut flux, |s| ids(std::slice::from_ref(s)) == ["2"]).await;
        assert_eq!(ids(&recus), ["1", "2"]);
        assert!(!statuts(&recus).contains(&StatutConnexion::Fermee));
        assert_eq!(*rt.reseau().borrow(), EtatReseau::Connecte);
        let etat = serveur.etat();
        assert_eq!((etat.connexions, etat.refusees), (1, 0));
        rt.arreter().await;
        let mut fin = Vec::new();
        while let Some(s) = flux.essayer() {
            fin.push(s);
        }
        assert!(types(&fin).is_empty());
        println!(
            "C3 : évènements reçus {:?}, connexions {}, la connexion est restée ouverte après le type inconnu",
            ids(&recus),
            etat.connexions
        );
    }
}
