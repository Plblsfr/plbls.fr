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
use crate::messagerie::vocal::{MicroPartage, SortiePartagee};
use crate::session::Memoire;
use crate::vocal::{OutilsVocaux, PERIODE_MESURE};
use crate::{
    DonneesCoquille, DonneesCreationSondage, DonneesGifs, DonneesMessageVocal, DonneesMessagerie,
    DonneesNouveauPrive, DonneesRecherche, DonneesSaisie, DonneesSelecteur, Principale,
};
use estompe_client::brouillons::Brouillons;
use estompe_client::video::Conversions;
use estompe_plateforme::SonPeripheriques;

const RAFRAICHIR_ACCUEIL: Duration = Duration::from_secs(60);
const SECONDE: Duration = Duration::from_secs(1);
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
    fn vocal(&self) -> Option<OutilsVocaux> {
        None
    }
    fn conversions(&self) -> Arc<Conversions> {
        Arc::new(Conversions::aucune())
    }
    fn micro(&self) -> MicroPartage {
        Arc::new(son_par_defaut())
    }
    fn sortie_son(&self) -> SortiePartagee {
        Arc::new(son_par_defaut())
    }
    fn jeux(&self) -> Option<watch::Receiver<crate::jeux::EtatJeux>> {
        None
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
    pub vocal: Option<OutilsVocaux>,
    pub jeux: Option<watch::Receiver<crate::jeux::EtatJeux>>,
    pub conversions: Arc<Conversions>,
    pub son: SonPeripheriques,
}

fn son_par_defaut() -> SonPeripheriques {
    SonPeripheriques::par_defaut(Arc::new(estompe_plateforme::AudioSysteme))
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

    fn vocal(&self) -> Option<OutilsVocaux> {
        self.vocal.clone()
    }

    fn conversions(&self) -> Arc<Conversions> {
        self.conversions.clone()
    }

    fn micro(&self) -> MicroPartage {
        Arc::new(self.son.clone())
    }

    fn sortie_son(&self) -> SortiePartagee {
        Arc::new(self.son.clone())
    }

    fn jeux(&self) -> Option<watch::Receiver<crate::jeux::EtatJeux>> {
        self.jeux.clone()
    }
}

pub trait ObservateurSession: Send + Sync {
    fn ouverte(&self, store: &Store);
    fn changements(&self, store: &Store, lot: &[estompe_client::store::Changement]);
    fn fermee(&self);
}

type Observateur = Arc<Mutex<Option<Arc<dyn ObservateurSession>>>>;

fn observateur(contexte: &Contexte) -> Option<Arc<dyn ObservateurSession>> {
    contexte
        .observateur
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
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
    observateur: Observateur,
    tache: JoinHandle<()>,
    arret: ArretCoquille,
}

type DemandeArret = (
    Option<oneshot::Sender<()>>,
    Option<std::sync::mpsc::Receiver<()>>,
);

#[derive(Clone)]
pub struct ArretCoquille {
    interne: Arc<Mutex<DemandeArret>>,
}

impl ArretCoquille {
    pub fn arreter(&self, delai: Duration) -> bool {
        let (arret, fini) = {
            let mut interne = self.interne.lock().unwrap_or_else(|e| e.into_inner());
            (interne.0.take(), interne.1.take())
        };
        let Some(fini) = fini else {
            return true;
        };
        if let Some(arret) = arret {
            let _ = arret.send(());
        }
        fini.recv_timeout(delai).is_ok()
    }
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
        let observateur: Observateur = Arc::new(Mutex::new(None));
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
            observateur: observateur.clone(),
        };
        let (envoi_arret, arret) = oneshot::channel();
        let (envoi_fini, fini) = std::sync::mpsc::channel();
        let tache = entree
            .runtime()
            .spawn(cycle(entree.observer(), contexte, arret, envoi_fini));
        Self {
            emplacement,
            focus,
            observateur,
            tache,
            arret: ArretCoquille {
                interne: Arc::new(Mutex::new((Some(envoi_arret), Some(fini)))),
            },
        }
    }

    pub fn arret(&self) -> ArretCoquille {
        self.arret.clone()
    }

    pub fn observer(&self, observateur: Arc<dyn ObservateurSession>) {
        *self.observateur.lock().unwrap_or_else(|e| e.into_inner()) = Some(observateur);
    }

    pub fn source_pont(&self) -> SourcePont {
        let emplacement = self.emplacement.clone();
        Arc::new(move || pont_courant(&emplacement))
    }

    pub fn fenetre_active_partagee(&self) -> Arc<AtomicBool> {
        self.focus.clone()
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
                pont.vocal().fenetre_active(actif);
            }
        }
    }

    pub fn fenetre_active(&self, actif: bool) {
        (self.suivi_du_focus())(actif);
    }

    pub fn suivi_de_la_visibilite(&self) -> impl Fn(&Principale, bool) + Send + Sync + 'static {
        let emplacement = self.emplacement.clone();
        move |f, visible| {
            if let Some(pont) = pont_courant(&emplacement) {
                pont.vocal().fenetre_visible(f, visible);
            }
        }
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
    rappel_messagerie!(on_voter, |p, _f, id, option| p
        .messagerie()
        .voter(&id, &option));
    rappel_messagerie!(on_clore_sondage, |p, _f, id| p
        .messagerie()
        .clore_sondage(&id));
    rappel_messagerie!(on_supprimer_sondage, |p, f, id| p
        .messagerie()
        .supprimer_sondage(&f, &id));
    rappel_messagerie!(on_basculer_votants, |p, f, id| p
        .messagerie()
        .basculer_votants(&f, &id));
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
    rappel_saisie!(on_creer_sondage, |p, f| p
        .messagerie()
        .ouvrir_creation_sondage(&f));
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
    let vocal = f.global::<DonneesMessageVocal>();
    macro_rules! rappel_vocal {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            vocal.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_vocal!(on_demarrer, |p, f| p.messagerie().demarrer_vocal(&f));
    rappel_vocal!(on_annuler, |p, f| p.messagerie().annuler_vocal(&f));
    rappel_vocal!(on_arreter, |p, f| p.messagerie().arreter_vocal(&f));
    rappel_vocal!(on_envoyer, |p, f| p.messagerie().envoyer_vocal(&f));
    rappel_vocal!(on_basculer, |p, f, _message, piece| p
        .messagerie()
        .basculer_lecture(&f, &piece));
    rappel_vocal!(on_placer, |p, f, _message, piece, fraction| p
        .messagerie()
        .placer_lecture(&f, &piece, fraction));
    rappel_vocal!(on_changer_vitesse, |p, f| p
        .messagerie()
        .changer_vitesse(&f));
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
    rappel_saisie!(on_ouvrir_gifs, |p, f| p.messagerie().ouvrir_gifs(&f));
    let gifs = f.global::<DonneesGifs>();
    macro_rules! rappel_gifs {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            gifs.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_gifs!(on_chercher, |p, _f, texte| p
        .messagerie()
        .chercher_gifs(&texte));
    rappel_gifs!(on_idee, |p, f, libelle| p
        .messagerie()
        .idee_de_gif(&f, &libelle));
    rappel_gifs!(on_choisir, |p, f, index| p
        .messagerie()
        .choisir_gif(&f, usize::try_from(index).ok()));
    rappel_gifs!(on_choisir_actif, |p, f| p
        .messagerie()
        .choisir_gif(&f, None));
    rappel_gifs!(on_deplacer, |p, f, dx, dy| p
        .messagerie()
        .gifs()
        .deplacer(&f, dx, dy));
    rappel_gifs!(on_defile, |p, f, haut, hauteur, largeur| p
        .messagerie()
        .defile_gifs(&f, haut, hauteur, largeur));
    rappel_gifs!(on_fermer, |p, f| p.messagerie().fermer_gifs(&f));
    let creation = f.global::<DonneesCreationSondage>();
    macro_rules! rappel_creation {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            creation.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_creation!(on_modifie, |p, f| p.messagerie().creation_modifiee(&f));
    rappel_creation!(on_choix_modifie, |p, f, index, texte| p
        .messagerie()
        .choix_modifie(&f, index, &texte));
    rappel_creation!(on_ajouter_choix, |p, f| p.messagerie().ajouter_choix(&f));
    rappel_creation!(on_retirer_choix, |p, f, index| p
        .messagerie()
        .retirer_choix(&f, index));
    rappel_creation!(on_choisir_cloture, |p, f, index| p
        .messagerie()
        .choisir_cloture(&f, index));
    rappel_creation!(on_publier, |p, f| p.messagerie().publier_sondage(&f));
    rappel_creation!(on_fermer, |p, f| p.messagerie().fermer_creation_sondage(&f));
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
    rappel!(on_menu_moi, |p, f, x, haut, bas| p
        .menu_moi(&f, Ancre::Element { x, haut, bas }));
    rappel!(on_tout_lire, |p, _f| p.tout_lire());
    rappel!(on_nouveau_prive, |p, f| p.ouvrir_nouveau_prive(&f));
    let parametres = f.global::<crate::DonneesParametres>();
    macro_rules! rappel_parametres {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let emplacement = emplacement.clone();
            let faible = f.as_weak();
            parametres.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (pont_courant(&emplacement), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_parametres!(on_apercus_change, |p, f, actif| p
        .definir_apercus(&f, actif));
    rappel_parametres!(on_type_change, |p, f, genre, actif| p
        .definir_type_notification(&f, &genre, actif));
    let nouveau = f.global::<DonneesNouveauPrive>();
    macro_rules! rappel_nouveau {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let emplacement = emplacement.clone();
            let faible = f.as_weak();
            nouveau.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (pont_courant(&emplacement), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_nouveau!(on_modifie, |p, f, texte| p
        .nouveau_prive_modifie(&f, &texte));
    rappel_nouveau!(on_naviguer, |p, f, pas| p.naviguer_nouveau_prive(&f, pas));
    rappel_nouveau!(on_choisir, |p, f, index| p.choisir_personne(&f, index));
    rappel_nouveau!(on_valider, |p, f| p.valider_nouveau_prive(&f));
    rappel_nouveau!(on_fermer_liste, |p, f| p.fermer_liste_nouveau_prive(&f));
    rappel_nouveau!(on_fermer, |p, f| p.fermer_nouveau_prive(&f));
    let recherche = f.global::<DonneesRecherche>();
    macro_rules! rappel_recherche {
        ($methode:ident, |$pont:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let emplacement = emplacement.clone();
            let faible = f.as_weak();
            recherche.$methode(move |$($arg),*| {
                if let (Some($pont), Some($f)) = (pont_courant(&emplacement), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    rappel_recherche!(on_ouvrir, |p, f| p.ouvrir_recherche(&f));
    rappel_recherche!(on_modifie, |p, f, texte| p.recherche_modifiee(&f, &texte));
    rappel_recherche!(on_naviguer, |p, f, pas| p.naviguer_recherche(&f, pas));
    rappel_recherche!(on_completer, |p, f| p.completer_recherche(&f));
    rappel_recherche!(on_valider, |p, f| p.valider_recherche(&f));
    rappel_recherche!(on_choisir_entree, |p, f, index| p.choisir_entree(&f, index));
    rappel_recherche!(on_choisir_portee, |p, f, index| p.choisir_portee(&f, index));
    rappel_recherche!(on_choisir_tri, |p, f, par_date| p.choisir_tri(&f, par_date));
    rappel_recherche!(on_ouvrir_resultat, |p, f, id| p.ouvrir_resultat(&f, &id));
    rappel_recherche!(on_chercher_recente, |p, f, texte| p
        .chercher_recente(&f, &texte));
    rappel_recherche!(on_effacer_recentes, |p, f| p.effacer_recentes(&f));
    rappel_recherche!(on_defile, |p, f, haut, vue, contenu| p
        .defile_recherche(&f, haut, vue, contenu));
    rappel_recherche!(on_fermer, |p, f| p.fermer_recherche(&f));
    let source = emplacement.clone();
    brancher_messagerie(f, Arc::new(move || pont_courant(&source)));
    let source = emplacement.clone();
    let ouverture = emplacement.clone();
    crate::vocal::pont::brancher_rappels(
        f,
        Arc::new(move || pont_courant(&source).map(|p| p.vocal().clone())),
        Arc::new(move |f, salon| {
            if let Some(pont) = pont_courant(&ouverture) {
                pont.ouvrir_salon_de_son_serveur(f, salon);
            }
        }),
    );
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
    observateur: Observateur,
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
    tache: JoinHandle<()>,
}

impl SessionActive {
    fn arreter(mut self, contexte: &Contexte) -> JoinHandle<()> {
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
        self.tache
    }
}

async fn cycle(
    mut etats: watch::Receiver<Etat>,
    contexte: Contexte,
    mut arret: oneshot::Receiver<()>,
    fini: std::sync::mpsc::Sender<()>,
) {
    let mut session: Option<SessionActive> = None;
    let mut dernier_rejoint: Option<String> = None;
    loop {
        let etat = etats.borrow_and_update().clone();
        match &etat {
            Etat::Connecte { compte, serveur } => {
                if session.as_ref().map(|s| s.compte.as_str()) != Some(compte.id.as_str()) {
                    if let Some(ancienne) = session.take() {
                        drop(ancienne.arreter(&contexte));
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
                    drop(ancienne.arreter(&contexte));
                }
            }
        }
        tokio::select! {
            changement = etats.changed() => if changement.is_err() {
                break;
            },
            _ = &mut arret => break,
        }
    }
    if let Some(ancienne) = session.take() {
        let _ = ancienne.arreter(&contexte).await;
    }
    let _ = fini.send(());
}

fn demarrer_session(contexte: &Contexte, compte: String) -> SessionActive {
    let (arret, attente_arret) = oneshot::channel();
    let (rejoints, recus) = mpsc::unbounded_channel();
    let generation = contexte
        .emplacement
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .0;
    let tache = contexte
        .runtime
        .spawn(session(contexte.clone(), generation, attente_arret, recus));
    SessionActive {
        compte,
        arret: Some(arret),
        rejoints,
        tache,
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
        .definir_son(contexte.fabrique.micro(), contexte.fabrique.sortie_son());
    let memoire = contexte.memoire.clone();
    pont.messagerie().definir_ecoutes(
        contexte.memoire.lire(|p| p.vocaux_ecoutes.clone()),
        Arc::new(move |ecoutes| memoire.modifier(|p| p.vocaux_ecoutes = ecoutes)),
    );
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
    pont.vocal().definir_fichiers(contexte.fabrique.fichiers());
    let outils = contexte.fabrique.vocal();
    if let Some(outils) = &outils {
        pont.vocal().brancher(outils.clone());
        if !contexte.focus.load(Ordering::SeqCst) {
            pont.vocal().fenetre_active(false);
        }
    }
    let mut jeux = contexte.fabrique.jeux();
    let annonce = crate::jeux::Annonce::default();
    let mut etat_vocal = outils.as_ref().map(|o| o.session.etat());
    let mut avis_vocal = outils.as_ref().map(|o| o.session.abonner_avis());
    let mut mesure = tokio::time::interval(PERIODE_MESURE);
    let mut secondes = tokio::time::interval(SECONDE);
    secondes.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    mesure.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
    if let Some(o) = observateur(&contexte) {
        o.ouverte(&store);
    }
    let reseau_initial = *reseau.borrow_and_update();
    if let Some(jeux) = jeux.as_mut() {
        let etat = jeux.borrow_and_update().clone();
        annonce.appliquer(
            &store,
            &contexte.runtime,
            reseau_initial == EtatReseau::Connecte,
            &etat,
        );
    }
    let p = pont.clone();
    let etat_vocal_initial = etat_vocal.as_mut().map(|e| e.borrow_and_update().clone());
    contexte.sur_l_interface(move |f| {
        p.definir_reseau(f, reseau_initial);
        p.demarrer(f);
        if let Some(etat) = etat_vocal_initial {
            p.vocal().definir_etat(f, etat);
        }
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
                    if let Some(o) = observateur(&contexte) {
                        o.changements(&store, &lot);
                    }
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
                if let Some(jeux) = jeux.as_ref() {
                    let courant = jeux.borrow().clone();
                    annonce.appliquer(&store, &contexte.runtime, etat == EtatReseau::Connecte, &courant);
                }
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.definir_reseau(f, etat));
            }
            changement = async { jeux.as_mut().expect("suivi des jeux").changed().await }, if jeux.is_some() => {
                match changement {
                    Ok(()) => {
                        let etat = jeux.as_mut().expect("suivi des jeux").borrow_and_update().clone();
                        let connecte = *reseau.borrow() == EtatReseau::Connecte;
                        annonce.appliquer(&store, &contexte.runtime, connecte, &etat);
                    }
                    Err(_) => jeux = None,
                }
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
            changement = async { etat_vocal.as_mut().expect("suivi du vocal").changed().await }, if etat_vocal.is_some() => {
                match changement {
                    Ok(()) => {
                        let etat = etat_vocal.as_mut().expect("suivi du vocal").borrow_and_update().clone();
                        let p = pont.clone();
                        contexte.sur_l_interface(move |f| p.vocal().definir_etat(f, etat));
                    }
                    Err(_) => etat_vocal = None,
                }
            }
            avis = async { avis_vocal.as_mut().expect("avis du vocal").recv().await }, if avis_vocal.is_some() => {
                match avis {
                    Ok(avis) => pont.vocal().avis(&avis),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => avis_vocal = None,
                }
            }
            _ = secondes.tick(), if pont.durees_vocales() => {
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.rafraichir_durees(f));
            }
            _ = mesure.tick(), if pont.vocal().mesure_active() => {
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.vocal().mesurer(f));
            }
            _ = expiration_des_saisies(&store) => {
                store.expirer_saisies();
                let p = pont.clone();
                contexte.sur_l_interface(move |f| p.messagerie().expirer_saisies(f));
            }
        }
    }
    pont.messagerie().saisie().enregistrer_maintenant();
    pont.vocal().arreter();
    if let Some(o) = observateur(&contexte) {
        o.fermee();
    }
    sections.abort();
    suivi.abort();
    if let Some(temps_reel) = temps_reel {
        temps_reel.arreter().await;
    }
}
