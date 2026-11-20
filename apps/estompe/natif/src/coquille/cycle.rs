use std::sync::atomic::{AtomicBool, Ordering};
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

use super::avatars::Profils;
use super::navigation::{route_de_demarrage, Navigation};
use super::pont::{installer_modeles, vider_modeles, Horloge, Pont};
use crate::entree::{Emetteur, Entree, Etat, Evenement, Futur};
use crate::menu::Ancre;
use crate::messagerie::medias::Medias;
use crate::messagerie::pont::{Navigateur, Presse, SystemeFichiers};
use crate::messagerie::selecteur::Cible as CibleSelecteur;
use crate::session::Memoire;
use crate::{DonneesCoquille, DonneesMessagerie, DonneesSaisie, DonneesSelecteur, Principale};
use estompe_client::brouillons::Brouillons;
use estompe_client::video::Conversions;

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
    fn profils(&self) -> Arc<Profils>;
    fn medias(&self) -> Option<Arc<Medias>> {
        None
    }
    fn navigateur(&self) -> Navigateur {
        Arc::new(estompe_plateforme::NavigateurSysteme)
    }
    fn fichiers(&self) -> SystemeFichiers {
        Arc::new(estompe_plateforme::FichiersSysteme)
    }
    fn presse_papiers(&self) -> Presse {
        Arc::new(estompe_plateforme::PressePapiersSysteme)
    }
    fn brouillons(&self) -> Brouillons {
        Brouillons::en_memoire()
    }
    fn conversions(&self) -> Arc<Conversions> {
        Arc::new(Conversions::aucune())
    }
}

pub struct FabriqueReelle {
    pub client: Client,
    pub jeton: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub adresses_sections: Vec<(Section, String)>,
    pub journal: Option<Arc<Journal>>,
    pub profils: Arc<Profils>,
    pub medias: Arc<Medias>,
    pub navigateur: Navigateur,
    pub dossier_donnees: Option<std::path::PathBuf>,
    pub conversions: Arc<Conversions>,
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

    fn profils(&self) -> Arc<Profils> {
        self.profils.clone()
    }

    fn medias(&self) -> Option<Arc<Medias>> {
        Some(self.medias.clone())
    }

    fn navigateur(&self) -> Navigateur {
        self.navigateur.clone()
    }

    fn brouillons(&self) -> Brouillons {
        self.dossier_donnees
            .as_deref()
            .map(Brouillons::charger)
            .unwrap_or_default()
    }

    fn conversions(&self) -> Arc<Conversions> {
        self.conversions.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Depot {
    Survol(bool),
    Fichiers(Vec<std::path::PathBuf>),
}

pub struct Coquille {
    emplacement: Emplacement,
    focus: Arc<AtomicBool>,
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
        let focus = Arc::new(AtomicBool::new(true));
        brancher_rappels(f, &emplacement, &memoire);
        let contexte = Contexte {
            emetteur: entree.emetteur(),
            fabrique,
            memoire,
            horloge,
            fenetre: f.as_weak(),
            emplacement: emplacement.clone(),
            runtime: entree.runtime(),
            focus: focus.clone(),
        };
        let tache = entree.runtime().spawn(cycle(entree.observer(), contexte));
        Self {
            emplacement,
            focus,
            tache,
        }
    }

    pub fn pont(&self) -> Option<Pont> {
        pont_courant(&self.emplacement)
    }

    pub fn suivi_du_focus(&self) -> impl Fn(bool) + Send + Sync + 'static {
        let emplacement = self.emplacement.clone();
        let focus = self.focus.clone();
        move |actif| {
            if focus.swap(actif, Ordering::SeqCst) == actif {
                return;
            }
            if let Some(pont) = pont_courant(&emplacement) {
                pont.messagerie().fenetre_active(actif);
            }
        }
    }

    pub fn fenetre_active(&self, actif: bool) {
        (self.suivi_du_focus())(actif);
    }

    pub fn suivi_du_depot(&self) -> impl Fn(&Principale, Depot) + 'static {
        let emplacement = self.emplacement.clone();
        move |f, depot| {
            if let Some(pont) = pont_courant(&emplacement) {
                match depot {
                    Depot::Survol(survol) => pont.messagerie().survol_depot(f, survol),
                    Depot::Fichiers(chemins) => pont.messagerie().deposer(f, chemins),
                }
            }
        }
    }

    pub fn deposer(&self, f: &Principale, depot: Depot) {
        (self.suivi_du_depot())(f, depot);
    }
}

impl Drop for Coquille {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

pub type SourcePont = Arc<dyn Fn() -> Option<Pont> + Send + Sync>;

pub fn brancher_messagerie(f: &Principale, source: SourcePont) {
    let m = f.global::<DonneesMessagerie>();
    macro_rules! rappel_messagerie {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            m.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_messagerie!(on_defilee, |p, f, position, vue, largeur| p
        .messagerie()
        .defilee(&f, position, vue, largeur));
    rappel_messagerie!(on_ligne_mesuree, |p, f, cle, hauteur| p
        .messagerie()
        .ligne_mesuree(&f, &cle, hauteur));
    rappel_messagerie!(on_lien_clique, |p, f, id, cible| {
        if let Some(utilisateur) = p.messagerie().lien_clique(&f, &id, &cible) {
            p.ouvrir_membre(&f, &utilisateur);
        }
    });
    rappel_messagerie!(on_reponse_cliquee, |p, _f, id| p
        .messagerie()
        .reponse_cliquee(&id));
    rappel_messagerie!(on_auteur_clique, |p, f, utilisateur| p
        .ouvrir_membre(&f, &utilisateur));
    rappel_messagerie!(on_aller_en_bas, |p, f| p.messagerie().aller_en_bas(&f));
    rappel_messagerie!(on_voir_epingle, |p, _f, id| p
        .messagerie()
        .voir_epingle(&id));
    rappel_messagerie!(on_recharger_epingles, |p, f| p
        .messagerie()
        .charger_epingles(&f));
    rappel_messagerie!(on_reessayer, |p, f| p.messagerie().reessayer(&f));
    rappel_messagerie!(on_theme_change, |p, f, _sombre| p
        .messagerie()
        .theme_change(&f));
    rappel_messagerie!(on_image_cliquee, |p, f, message, index| p
        .messagerie()
        .image_cliquee(&f, &message, index));
    rappel_messagerie!(on_telecharger, |p, f, message, piece| p
        .messagerie()
        .telecharger_piece(&f, &message, &piece));
    rappel_messagerie!(on_ouvrir_fichier, |p, f, _message, piece| p
        .messagerie()
        .ouvrir_fichier(&f, &piece));
    rappel_messagerie!(on_carte_cliquee, |p, _f, _message, url| p
        .messagerie()
        .carte_cliquee(&url));
    rappel_messagerie!(on_visionneuse_voisine, |p, f, sens| p
        .messagerie()
        .visionneuse_voisine(&f, sens));
    rappel_messagerie!(on_visionneuse_zoom, |p, f, facteur| p
        .messagerie()
        .visionneuse_zoom(&f, facteur));
    rappel_messagerie!(on_visionneuse_copier, |p, _f| p
        .messagerie()
        .visionneuse_copier());
    rappel_messagerie!(on_visionneuse_enregistrer, |p, f| p
        .messagerie()
        .visionneuse_enregistrer(&f));
    rappel_messagerie!(on_fermer_visionneuse, |p, f| p
        .messagerie()
        .fermer_visionneuse(&f));
    rappel_messagerie!(on_reessayer_envoi, |p, _f, id| p
        .messagerie()
        .reessayer_envoi(&id));
    rappel_messagerie!(on_abandonner_envoi, |p, _f, id| p
        .messagerie()
        .abandonner_envoi(&id));
    rappel_messagerie!(on_action_message, |p, f, id, action| p
        .messagerie()
        .action_message(&f, &id, &action));
    rappel_messagerie!(on_menu_message, |p, f, id, x, y| p
        .messagerie()
        .ouvrir_menu_message(&f, &id, Ancre::Point { x, y }));
    rappel_messagerie!(on_menu_message_bouton, |p, f, id, x, haut, bas| p
        .messagerie()
        .ouvrir_menu_message(&f, &id, Ancre::Element { x, haut, bas }));
    rappel_messagerie!(on_reagir, |p, _f, id, emoji| p
        .messagerie()
        .reagir(&id, &emoji));
    let s = f.global::<DonneesSaisie>();
    macro_rules! rappel_saisie {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            s.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_saisie!(on_modifie, |p, f, texte| {
        if p.messagerie().saisie_modifiee(&f, &texte) {
            p.rafraichir_brouillons(&f);
        }
    });
    rappel_saisie!(on_envoyer, |p, f, texte| {
        if p.messagerie().envoyer_saisie(&f, &texte) {
            p.rafraichir_brouillons(&f);
        }
    });
    rappel_saisie!(on_annuler_bandeau, |p, f| p
        .messagerie()
        .annuler_bandeau(&f));
    rappel_saisie!(on_fleche_haut, |p, f| p.messagerie().fleche_haut(&f));
    rappel_saisie!(on_curseur_deplace, |p, f, texte, curseur| p
        .messagerie()
        .curseur_deplace(&f, &texte, curseur));
    rappel_saisie!(on_naviguer, |p, f, pas| p
        .messagerie()
        .naviguer_propositions(&f, pas));
    rappel_saisie!(on_choisir, |p, f, index| {
        if p.messagerie().choisir_proposition(&f, index) {
            p.rafraichir_brouillons(&f);
        }
    });
    rappel_saisie!(on_fermer_propositions, |p, f| p
        .messagerie()
        .fermer_propositions(&f));
    rappel_saisie!(on_joindre, |p, _f| p.messagerie().choisir_pieces());
    rappel_saisie!(on_retirer_piece, |p, f, cle| p
        .messagerie()
        .retirer_piece(&f, cle));
    rappel_saisie!(on_reessayer_piece, |p, f, cle| p
        .messagerie()
        .reessayer_piece(&f, cle));
    {
        let source = source.clone();
        let faible = f.as_weak();
        s.on_coller(move || match (source(), faible.upgrade()) {
            (Some(p), Some(f)) => p.messagerie().coller(&f),
            _ => false,
        });
    }
    rappel_saisie!(on_ouvrir_selecteur, |p, f| p
        .messagerie()
        .ouvrir_selecteur(&f, CibleSelecteur::Saisie));
    let selecteur = f.global::<DonneesSelecteur>();
    macro_rules! rappel_selecteur {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            selecteur.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_selecteur!(on_chercher, |p, f, texte| p
        .messagerie()
        .selecteur()
        .chercher(&f, &texte));
    rappel_selecteur!(on_deplacer, |p, f, dx, dy| p
        .messagerie()
        .selecteur()
        .deplacer(&f, dx, dy));
    rappel_selecteur!(on_choisir, |p, f, index, garder| {
        if p.messagerie()
            .choisir_emoji(&f, usize::try_from(index).ok(), garder)
        {
            p.rafraichir_brouillons(&f);
        }
    });
    rappel_selecteur!(on_choisir_actif, |p, f, garder| {
        if p.messagerie().choisir_emoji(&f, None, garder) {
            p.rafraichir_brouillons(&f);
        }
    });
    rappel_selecteur!(on_survoler, |p, f, index| p
        .messagerie()
        .selecteur()
        .survoler(&f, index));
    rappel_selecteur!(on_aller_a, |p, f, onglet| p
        .messagerie()
        .selecteur()
        .aller_a(&f, onglet.max(0) as usize));
    rappel_selecteur!(on_defile, |p, f, haut, hauteur| p
        .messagerie()
        .selecteur()
        .defile(&f, haut, hauteur));
    rappel_selecteur!(on_fermer, |p, f| p.messagerie().fermer_selecteur(&f));
}

fn brancher_rappels(f: &Principale, emplacement: &Emplacement, memoire: &Memoire) {
    installer_modeles(f);
    let g = f.global::<DonneesCoquille>();
    g.set_panneau_replie(memoire.lire(|p| p.panneau_replie));
    g.set_marge_ouverte(memoire.lire(|p| p.marge_ouverte));
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
    rappel!(on_membres_defiles, |p, f, defilement, vue| p
        .membres_defiles(&f, defilement, vue));
    rappel!(on_filtrer_membres, |p, f, filtre| p
        .filtrer_membres(&f, &filtre));
    rappel!(on_ouvrir_membre, |p, f, utilisateur| p
        .ouvrir_membre(&f, &utilisateur));
    rappel!(on_fermer_carte, |p, f| p.fermer_carte(&f));
    rappel!(on_onglet_marge_choisi, |p, f, onglet| p
        .choisir_onglet_marge(&f, onglet));
    rappel!(on_epingles_bascule, |p, f| p.basculer_epingles(&f));
    rappel!(on_menu_salon, |p, f, id, x, y| p.menu_salon(
        &f,
        &id,
        Ancre::Point { x, y }
    ));
    rappel!(on_menu_prive, |p, f, id, x, y| p.menu_prive(
        &f,
        &id,
        Ancre::Point { x, y }
    ));
    rappel!(on_menu_membre, |p, f, id, x, y| p.menu_membre(
        &f,
        &id,
        Ancre::Point { x, y }
    ));
    let source = emplacement.clone();
    brancher_messagerie(f, Arc::new(move || pont_courant(&source)));
    let faible = f.as_weak();
    let memoire_panneau = memoire.clone();
    g.on_panneau_bascule(move |replie| {
        if let Some(f) = faible.upgrade() {
            f.global::<DonneesCoquille>().set_panneau_replie(replie);
        }
        memoire_panneau.modifier(|p| p.panneau_replie = replie);
    });
    let faible = f.as_weak();
    let memoire_marge = memoire.clone();
    let emplacement = emplacement.clone();
    g.on_marge_bascule(move |ouverte| {
        if let Some(f) = faible.upgrade() {
            f.global::<DonneesCoquille>().set_marge_ouverte(ouverte);
            if let Some(pont) = pont_courant(&emplacement) {
                pont.marge_basculee(&f);
            }
        }
        memoire_marge.modifier(|p| p.marge_ouverte = ouverte);
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
    focus: Arc<AtomicBool>,
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

async fn expiration_des_saisies(store: &Store) {
    match store.prochaine_expiration() {
        Some(instant) => tokio::time::sleep_until(instant).await,
        None => std::future::pending().await,
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
        contexte.fabrique.profils(),
    );
    pont.messagerie()
        .definir_navigateur(contexte.fabrique.navigateur());
    if let Some(medias) = contexte.fabrique.medias() {
        pont.messagerie().definir_medias(medias);
    }
    pont.messagerie()
        .definir_fichiers(contexte.fabrique.fichiers());
    pont.messagerie()
        .definir_presse_papiers(contexte.fabrique.presse_papiers());
    pont.messagerie()
        .definir_conversions(contexte.fabrique.conversions());
    pont.messagerie()
        .saisie()
        .definir_brouillons(contexte.fabrique.brouillons());
    pont.messagerie()
        .definir_apercus(contexte.memoire.lire(|p| p.apercus));
    let memoire = contexte.memoire.clone();
    pont.messagerie().selecteur().definir_recents(
        contexte.memoire.lire(|p| p.emoji_recents.clone()),
        Arc::new(move |recents| memoire.modifier(|p| p.emoji_recents = recents)),
    );
    if !contexte.focus.load(Ordering::SeqCst) {
        pont.messagerie().fenetre_active(false);
    }
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
            _ = expiration_des_saisies(&store) => {
                store.expirer_saisies();
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.messagerie().expirer_saisies(f));
            }
        }
    }
    pont.messagerie().saisie().enregistrer_maintenant();
    sections.abort();
    suivi.abort();
    if let Some(temps_reel) = temps_reel {
        temps_reel.arreter().await;
    }
}
