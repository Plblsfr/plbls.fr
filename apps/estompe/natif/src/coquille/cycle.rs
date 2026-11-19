use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_client::http::{Client, ErreurApi};
use estompe_client::journal::{Journal, Niveau};
use estompe_client::sections::{sections_ouvertes, Section};
use estompe_client::store::{ErreurStore, FinFlux, Store};
use estompe_client::temps_reel::{
    adresse_ws, EtatReseau, FluxTempsReel, OptionsTempsReel, TempsReel,
};
use slint::ComponentHandle;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use super::navigation::{route_de_demarrage, Navigation};
use super::pont::{installer_modeles, vider_modeles, Horloge, Pont};
use crate::entree::{Emetteur, Entree, Etat, Evenement, Futur};
use crate::session::Memoire;
use crate::{DonneesCoquille, Principale};

const RAFRAICHIR_ACCUEIL: Duration = Duration::from_secs(60);
const ATTENTE_MAX: Duration = Duration::from_secs(30);
const SOURCE_JOURNAL: &str = "coquille";

pub struct Ouverture {
    pub store: Store,
    pub flux: FluxTempsReel,
    pub reseau: watch::Receiver<EtatReseau>,
    pub temps_reel: Option<TempsReel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchecOuverture {
    SessionPerdue,
    AccesPerdu,
    Abandon,
}

pub trait Fabrique: Send + Sync + 'static {
    fn ouvrir(&self, ordre_serveurs: Vec<String>) -> Futur<Result<Ouverture, EchecOuverture>>;
    fn sections(&self) -> Futur<Vec<Section>>;
}

pub struct FabriqueReelle {
    pub client: Client,
    pub jeton: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub adresses_sections: Vec<(Section, String)>,
    pub journal: Option<Arc<Journal>>,
}

fn noter(journal: &Option<Arc<Journal>>, niveau: Niveau, message: &str) {
    if let Some(journal) = journal {
        journal.ecrire(niveau, SOURCE_JOURNAL, message);
    }
}

pub fn echec_du_chargement(erreur: &ErreurStore) -> Option<EchecOuverture> {
    match erreur {
        ErreurStore::CompteInactif => Some(EchecOuverture::AccesPerdu),
        ErreurStore::Api(ErreurApi::NonAutorise(_)) => Some(EchecOuverture::SessionPerdue),
        ErreurStore::Api(ErreurApi::MiseAJourRequise(_)) => Some(EchecOuverture::Abandon),
        ErreurStore::Api(_) => None,
    }
}

impl Fabrique for FabriqueReelle {
    fn ouvrir(&self, ordre_serveurs: Vec<String>) -> Futur<Result<Ouverture, EchecOuverture>> {
        let client = self.client.clone();
        let journal = self.journal.clone();
        Box::pin(async move {
            let mut options =
                OptionsTempsReel::nouvelles(adresse_ws(client.adresse(), ""), client.clone());
            if let Some(journal) = &journal {
                options = options.avec_journal(journal.clone());
            }
            let (temps_reel, flux) = TempsReel::demarrer(options);
            let store = Store::nouveau(client, &ordre_serveurs);
            let mut attente = Duration::from_secs(1);
            loop {
                match store.charger().await {
                    Ok(()) => break,
                    Err(erreur) => {
                        noter(
                            &journal,
                            Niveau::Avertissement,
                            &format!("chargement du store : {erreur}"),
                        );
                        if let Some(echec) = echec_du_chargement(&erreur) {
                            temps_reel.arreter().await;
                            return Err(echec);
                        }
                        tokio::time::sleep(attente).await;
                        attente = (attente * 2).min(ATTENTE_MAX);
                    }
                }
            }
            Ok(Ouverture {
                store,
                flux,
                reseau: temps_reel.reseau(),
                temps_reel: Some(temps_reel),
            })
        })
    }

    fn sections(&self) -> Futur<Vec<Section>> {
        let adresses = self.adresses_sections.clone();
        let jeton = self.jeton.clone();
        Box::pin(async move {
            sections_ouvertes(&adresses, |adresse| {
                let jeton = jeton.clone();
                Ok(Client::nouveau(adresse)?
                    .avec_plateforme(crate::entree::PLATEFORME, crate::VERSION)
                    .avec_jeton(move || jeton()))
            })
            .await
        })
    }
}

type Emplacement = Arc<Mutex<(u64, Option<Pont>)>>;

fn pont_courant(emplacement: &Emplacement) -> Option<Pont> {
    emplacement
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .1
        .clone()
}

pub struct Coquille {
    emplacement: Emplacement,
    tache: JoinHandle<()>,
}

impl Coquille {
    pub fn brancher(
        f: &Principale,
        entree: &Entree,
        fabrique: Arc<dyn Fabrique>,
        memoire: Memoire,
        horloge: Horloge,
    ) -> Self {
        let emplacement: Emplacement = Arc::new(Mutex::new((0, None)));
        brancher_rappels(f, &emplacement, &memoire);
        let contexte = Contexte {
            emetteur: entree.emetteur(),
            fabrique,
            memoire,
            horloge,
            fenetre: f.as_weak(),
            emplacement: emplacement.clone(),
            runtime: entree.runtime(),
        };
        let tache = entree.runtime().spawn(cycle(entree.observer(), contexte));
        Self { emplacement, tache }
    }

    pub fn pont(&self) -> Option<Pont> {
        pont_courant(&self.emplacement)
    }
}

impl Drop for Coquille {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

fn brancher_rappels(f: &Principale, emplacement: &Emplacement, memoire: &Memoire) {
    installer_modeles(f);
    let g = f.global::<DonneesCoquille>();
    g.set_panneau_replie(memoire.lire(|p| p.panneau_replie));
    macro_rules! rappel {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let emplacement = emplacement.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (pont_courant(&emplacement), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_ouvrir_accueil, |p, f| p.ouvrir_accueil(&f));
    rappel!(on_ouvrir_serveur, |p, f, id| p.ouvrir_serveur(&f, &id));
    rappel!(on_ouvrir_salon, |p, f, id| p.ouvrir_salon(&f, &id));
    rappel!(on_ouvrir_prive, |p, f, id| p.ouvrir_prive(&f, &id));
    rappel!(on_ouvrir_section, |p, f, id| p.ouvrir_section(&f, &id));
    rappel!(on_ouvrir_conversation, |p, f, cle| p
        .ouvrir_conversation(&f, &cle));
    rappel!(on_deplacer_serveur, |p, _f, de, vers| p
        .deplacer_serveur(&de, &vers));
    rappel!(on_serveur_voisin, |p, f, suivant| p
        .serveur_voisin(&f, suivant));
    let faible = f.as_weak();
    let memoire = memoire.clone();
    g.on_panneau_bascule(move |replie| {
        if let Some(f) = faible.upgrade() {
            f.global::<DonneesCoquille>().set_panneau_replie(replie);
        }
        memoire.modifier(|p| p.panneau_replie = replie);
    });
    let faible = f.as_weak();
    g.on_marge_bascule(move |ouverte| {
        if let Some(f) = faible.upgrade() {
            f.global::<DonneesCoquille>().set_marge_ouverte(ouverte);
        }
    });
}

#[derive(Clone)]
struct Contexte {
    emetteur: Emetteur,
    fabrique: Arc<dyn Fabrique>,
    memoire: Memoire,
    horloge: Horloge,
    fenetre: slint::Weak<Principale>,
    emplacement: Emplacement,
    runtime: tokio::runtime::Handle,
}

impl Contexte {
    fn sur_l_interface(&self, action: impl FnOnce(&Principale) + Send + 'static) {
        let _ = self.fenetre.upgrade_in_event_loop(move |f| action(&f));
    }
}

struct SessionActive {
    compte: String,
    arret: Option<oneshot::Sender<()>>,
    rejoints: mpsc::UnboundedSender<String>,
}

impl SessionActive {
    fn arreter(mut self, contexte: &Contexte) {
        if let Some(arret) = self.arret.take() {
            let _ = arret.send(());
        }
        let mut emplacement = contexte
            .emplacement
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        emplacement.0 += 1;
        emplacement.1 = None;
        drop(emplacement);
        contexte.sur_l_interface(vider_modeles);
    }
}

async fn cycle(mut etats: watch::Receiver<Etat>, contexte: Contexte) {
    let mut session: Option<SessionActive> = None;
    let mut dernier_rejoint: Option<String> = None;
    loop {
        let etat = etats.borrow_and_update().clone();
        match &etat {
            Etat::Connecte { compte, serveur } => {
                if session.as_ref().map(|s| s.compte.as_str()) != Some(compte.id.as_str()) {
                    if let Some(ancienne) = session.take() {
                        ancienne.arreter(&contexte);
                    }
                    session = Some(demarrer_session(&contexte, compte.id.clone()));
                }
                if let (Some(serveur), Some(active)) = (serveur, &session) {
                    if dernier_rejoint.as_deref() != Some(serveur.id.as_str()) {
                        dernier_rejoint = Some(serveur.id.clone());
                        let _ = active.rejoints.send(serveur.id.clone());
                    }
                }
            }
            Etat::Invitation { .. } => dernier_rejoint = None,
            _ => {
                dernier_rejoint = None;
                if let Some(ancienne) = session.take() {
                    ancienne.arreter(&contexte);
                }
            }
        }
        if etats.changed().await.is_err() {
            break;
        }
    }
    if let Some(ancienne) = session.take() {
        ancienne.arreter(&contexte);
    }
}

fn demarrer_session(contexte: &Contexte, compte: String) -> SessionActive {
    let (arret, attente_arret) = oneshot::channel();
    let (rejoints, recus) = mpsc::unbounded_channel();
    let generation = contexte
        .emplacement
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .0;
    contexte
        .runtime
        .spawn(session(contexte.clone(), generation, attente_arret, recus));
    SessionActive {
        compte,
        arret: Some(arret),
        rejoints,
    }
}

fn evenement_d_echec(echec: EchecOuverture) -> Option<Evenement> {
    match echec {
        EchecOuverture::SessionPerdue => Some(Evenement::SessionPerdue),
        EchecOuverture::AccesPerdu => Some(Evenement::AccesPerdu),
        EchecOuverture::Abandon => None,
    }
}

pub fn evenement_de_fin(fin: FinFlux) -> Option<Evenement> {
    match fin {
        FinFlux::SessionExpiree => Some(Evenement::SessionPerdue),
        FinFlux::AccesRetire => Some(Evenement::AccesPerdu),
        FinFlux::Termine => None,
    }
}

async fn session(
    contexte: Contexte,
    generation: u64,
    mut arret: oneshot::Receiver<()>,
    mut rejoints: mpsc::UnboundedReceiver<String>,
) {
    let ordre = contexte.memoire.lire(|p| p.ordre_serveurs.clone());
    let ouverture = tokio::select! {
        _ = &mut arret => return,
        ouverture = contexte.fabrique.ouvrir(ordre) => ouverture,
    };
    let Ouverture {
        store,
        flux,
        mut reseau,
        temps_reel,
    } = match ouverture {
        Ok(ouverture) => ouverture,
        Err(echec) => {
            if let Some(evenement) = evenement_d_echec(echec) {
                contexte.emetteur.envoyer(evenement);
            }
            return;
        }
    };
    let mut abonne = store.abonner();
    let derniere_vue = contexte.memoire.lire(|p| p.derniere_vue.clone());
    let route = store.lire(|etat| route_de_demarrage(etat, derniere_vue.as_ref()));
    let pont = Pont::nouveau(
        store.clone(),
        contexte.fenetre.clone(),
        contexte.runtime.clone(),
        Some(contexte.memoire.clone()),
        contexte.horloge.clone(),
        Navigation::nouvelle(route),
    );
    let installe = {
        let mut emplacement = contexte
            .emplacement
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let courante = emplacement.0 == generation;
        if courante {
            emplacement.1 = Some(pont.clone());
        }
        courante
    };
    if !installe {
        if let Some(temps_reel) = temps_reel {
            temps_reel.arreter().await;
        }
        return;
    }
    let reseau_initial = *reseau.borrow_and_update();
    let p = pont.clone();
    contexte.sur_l_interface(move |f| {
        p.definir_reseau(f, reseau_initial);
        p.demarrer(f);
    });
    let sections = {
        let fabrique = contexte.fabrique.clone();
        let p = pont.clone();
        let c = contexte.clone();
        contexte.runtime.spawn(async move {
            let sections = fabrique.sections().await;
            c.sur_l_interface(move |f| p.definir_sections(f, sections));
        })
    };
    let mut suivi = store.brancher(flux);
    let mut horloge = tokio::time::interval(RAFRAICHIR_ACCUEIL);
    horloge.tick().await;
    let mut reseau_suivi = true;
    loop {
        tokio::select! {
            _ = &mut arret => break,
            lot = abonne.recv() => match lot {
                Some(lot) => {
                    let p = pont.clone();
                    contexte.sur_l_interface(move |f| p.appliquer(f, &lot));
                }
                None => break,
            },
            changement = reseau.changed(), if reseau_suivi => {
                if changement.is_err() {
                    reseau_suivi = false;
                    continue;
                }
                let etat = *reseau.borrow_and_update();
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.definir_reseau(f, etat));
            }
            fin = &mut suivi => {
                if let Some(evenement) = fin.ok().and_then(evenement_de_fin) {
                    contexte.emetteur.envoyer(evenement);
                }
                break;
            }
            Some(serveur) = rejoints.recv() => {
                if store.lire(|etat| etat.serveur(&serveur).is_none()) {
                    let _ = store.rafraichir_serveurs().await;
                }
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.ouvrir_serveur(f, &serveur));
            }
            _ = horloge.tick() => {
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.rafraichir_accueil(f));
            }
        }
    }
    sections.abort();
    suivi.abort();
    if let Some(temps_reel) = temps_reel {
        temps_reel.arreter().await;
    }
}
