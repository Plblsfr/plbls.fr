use crate::entree::{self, AuthReelle, Emetteur, Services};
use crate::{ModeTheme, Mouvement, Principale, Theme};
use estompe_client::auth::{Authentification, ConfigAuth};
use estompe_client::config::{self, AdresseServeur};
use estompe_client::http::{Client, Reglages};
use estompe_client::journal::{Journal, Niveau};
use estompe_client::preferences::{self, EtatFichier, Fenetre, Preferences};
use estompe_plateforme::{CoffreSysteme, Dossiers, NavigateurSysteme, OuvertureUrl, IDENTIFIANT};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub const LARGEUR_MIN: u32 = 720;
pub const HAUTEUR_MIN: u32 = 480;
const COTE_MAX: u32 = 16_384;

pub struct ServicesReels {
    pub client: Client,
    pub auth: Arc<Authentification>,
    pub services: Services,
}

pub struct Session {
    pub dossiers: Dossiers,
    pub preferences: RefCell<Preferences>,
    pub journal: Arc<Journal>,
    pub adresse: AdresseServeur,
}

impl Session {
    pub fn ouvrir(dossiers: Dossiers) -> Rc<Self> {
        let chargement = Preferences::charger(&dossiers.donnees);
        let journal = Arc::new(Journal::ouvrir(&dossiers.journaux));
        journal.activer(chargement.preferences.journal_debogage);
        let adresse = config::adresse_serveur(chargement.preferences.adresse_serveur.as_deref());
        journal.ecrire(
            Niveau::Info,
            "demarrage",
            &format!(
                "Estompe natif {}, serveur {} ({:?})",
                crate::VERSION,
                adresse.url,
                adresse.origine
            ),
        );
        if chargement.etat == EtatFichier::Corrompu {
            journal.ecrire(
                Niveau::Avertissement,
                "preferences",
                "fichier de préférences illisible : valeurs par défaut reprises",
            );
        }
        for (origine, erreur) in &adresse.rejets {
            journal.ecrire(
                Niveau::Avertissement,
                "configuration",
                &format!("adresse du serveur écartée ({origine:?}) : {erreur:?}"),
            );
        }
        Rc::new(Self {
            dossiers,
            preferences: RefCell::new(chargement.preferences),
            journal,
            adresse,
        })
    }

    pub fn services(&self, emetteur: &Emetteur) -> Result<ServicesReels, String> {
        let navigateur: Arc<dyn OuvertureUrl + Send + Sync> = Arc::new(NavigateurSysteme);
        let auth = Authentification::nouvelle(
            ConfigAuth::depuis_environnement(),
            Arc::new(CoffreSysteme::new(IDENTIFIANT)),
        )
        .map_err(|e| e.to_string())?
        .avec_journal(self.journal.clone());
        let auth = Arc::new(auth);
        let client = entree::client_de_l_app(&self.adresse.url, Reglages::default(), emetteur)
            .map_err(|e| e.to_string())?
            .avec_jeton(auth.fournisseur_jeton())
            .avec_journal(self.journal.clone());
        let services = Services {
            auth: Arc::new(AuthReelle {
                auth: auth.clone(),
                navigateur: navigateur.clone(),
            }),
            api: Arc::new(client.clone()),
            navigateur,
            lien_telechargement: entree::lien_telechargement(&self.adresse.url),
            journal: Some(self.journal.clone()),
        };
        Ok(ServicesReels {
            client,
            auth,
            services,
        })
    }

    pub fn enregistrer(&self) {
        if let Err(e) = self
            .preferences
            .borrow()
            .enregistrer(&self.dossiers.donnees)
        {
            self.journal.ecrire(
                Niveau::Erreur,
                "preferences",
                &format!("enregistrement impossible : {e}"),
            );
        }
    }

    pub fn brancher(self: &Rc<Self>, fenetre: &Principale) {
        appliquer(fenetre, &self.preferences.borrow());
        let session = self.clone();
        fenetre.on_theme_change(move |mode| {
            session.preferences.borrow_mut().theme = theme_des_preferences(mode);
            session.enregistrer();
        });
        let session = self.clone();
        let faible = fenetre.as_weak();
        fenetre.window().on_close_requested(move || {
            if let Some(fenetre) = faible.upgrade() {
                session.retenir_geometrie(&fenetre);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    pub fn retenir_geometrie(&self, fenetre: &Principale) {
        self.preferences.borrow_mut().fenetre = Some(geometrie(fenetre));
        self.enregistrer();
    }
}

pub fn mode_theme(theme: preferences::Theme) -> ModeTheme {
    match theme {
        preferences::Theme::Auto => ModeTheme::Auto,
        preferences::Theme::Clair => ModeTheme::Clair,
        preferences::Theme::Sombre => ModeTheme::Sombre,
    }
}

pub fn theme_des_preferences(mode: ModeTheme) -> preferences::Theme {
    match mode {
        ModeTheme::Auto => preferences::Theme::Auto,
        ModeTheme::Clair => preferences::Theme::Clair,
        ModeTheme::Sombre => preferences::Theme::Sombre,
    }
}

pub fn geometrie_valide(f: &Fenetre) -> bool {
    (LARGEUR_MIN..=COTE_MAX).contains(&f.largeur) && (HAUTEUR_MIN..=COTE_MAX).contains(&f.hauteur)
}

pub fn appliquer(fenetre: &Principale, preferences: &Preferences) {
    fenetre
        .global::<Theme>()
        .set_mode(mode_theme(preferences.theme));
    fenetre
        .global::<Mouvement>()
        .set_reduit(preferences.reduire_animations);
    if let Some(f) = preferences.fenetre.filter(geometrie_valide) {
        fenetre
            .window()
            .set_size(slint::LogicalSize::new(f.largeur as f32, f.hauteur as f32));
        fenetre
            .window()
            .set_position(slint::LogicalPosition::new(f.x as f32, f.y as f32));
    }
}

pub fn geometrie(fenetre: &Principale) -> Fenetre {
    let echelle = fenetre.window().scale_factor();
    let taille = fenetre.window().size().to_logical(echelle);
    let position = fenetre.window().position().to_logical(echelle);
    Fenetre {
        x: position.x.round() as i32,
        y: position.y.round() as i32,
        largeur: taille.width.round() as u32,
        hauteur: taille.height.round() as u32,
    }
}
