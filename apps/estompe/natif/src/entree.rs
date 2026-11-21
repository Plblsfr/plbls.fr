use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use estompe_client::appels::code_d_invitation;
use estompe_client::auth::{Authentification, ErreurAuth, Parcours};
use estompe_client::http::{Client, ErreurApi, Reglages};
use estompe_client::journal::{Journal, Niveau};
use estompe_client::types::{GetMeReponse, Person, Server};
use estompe_plateforme::OuvertureUrl;
use tokio::sync::{mpsc, oneshot, watch};

pub const PLATEFORME: &str = "natif";
pub const AIDE_PSEUDO: &str = "3 à 20 caractères : lettres, chiffres, _ . -";
pub const INVITATION_INVALIDE: &str = "Cette invitation est invalide ou a été révoquée.";
pub const REFUS_DU_SERVEUR: &str =
    "Connexion refusée par le serveur. Réessaie ou contacte l'administrateur.";
const SOURCE_JOURNAL: &str = "entrée";

pub type Futur<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait Authentifieur: Send + Sync + 'static {
    fn reprendre(&self) -> Futur<Result<bool, ErreurAuth>>;
    fn connecter(&self, parcours: Parcours) -> Futur<Result<(), ErreurAuth>>;
    fn annuler(&self);
    fn deconnecter(&self) -> Futur<()>;
}

pub trait ApiEntree: Send + Sync + 'static {
    fn moi(&self) -> Futur<Result<GetMeReponse, ErreurApi>>;
    fn activer(&self, pseudo: String) -> Futur<Result<Person, ErreurApi>>;
    fn rejoindre(&self, code: String) -> Futur<Result<Server, ErreurApi>>;
}

pub struct AuthReelle {
    pub auth: Arc<Authentification>,
    pub navigateur: Arc<dyn OuvertureUrl + Send + Sync>,
}

impl Authentifieur for AuthReelle {
    fn reprendre(&self) -> Futur<Result<bool, ErreurAuth>> {
        let auth = self.auth.clone();
        Box::pin(async move { auth.reprendre().await })
    }

    fn connecter(&self, parcours: Parcours) -> Futur<Result<(), ErreurAuth>> {
        let auth = self.auth.clone();
        let navigateur = self.navigateur.clone();
        Box::pin(async move {
            let connexion = auth.demarrer_connexion(&*navigateur, parcours)?;
            connexion.terminer().await
        })
    }

    fn annuler(&self) {
        self.auth.annuler_connexion();
    }

    fn deconnecter(&self) -> Futur<()> {
        let auth = self.auth.clone();
        Box::pin(async move {
            let _ = auth.deconnecter().await;
        })
    }
}

impl ApiEntree for Client {
    fn moi(&self) -> Futur<Result<GetMeReponse, ErreurApi>> {
        let client = self.clone();
        Box::pin(async move { client.moi().await })
    }

    fn activer(&self, pseudo: String) -> Futur<Result<Person, ErreurApi>> {
        let client = self.clone();
        Box::pin(async move { client.activer(&pseudo).await })
    }

    fn rejoindre(&self, code: String) -> Futur<Result<Server, ErreurApi>> {
        let client = self.clone();
        Box::pin(async move { client.rejoindre_serveur(&code).await })
    }
}

pub struct Services {
    pub auth: Arc<dyn Authentifieur>,
    pub api: Arc<dyn ApiEntree>,
    pub navigateur: Arc<dyn OuvertureUrl + Send + Sync>,
    pub lien_telechargement: String,
    pub installer: Option<Installateur>,
    pub journal: Option<Arc<Journal>>,
}

pub type Installateur = Arc<dyn Fn() + Send + Sync>;

impl Services {
    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = &self.journal {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }
}

pub fn client_de_l_app(
    adresse: &str,
    reglages: Reglages,
    emetteur: &Emetteur,
) -> Result<Client, ErreurApi> {
    let signal = emetteur.clone();
    let perte = emetteur.clone();
    Ok(Client::avec_reglages(adresse, reglages)?
        .avec_plateforme(PLATEFORME, crate::VERSION)
        .a_la_perte_de_session(move || perte.envoyer(Evenement::SessionPerdue))
        .a_la_mise_a_jour_requise(move |message| {
            signal.envoyer(Evenement::MiseAJourSignalee {
                message: message.to_string(),
                version_minimale: String::new(),
            })
        }))
}

pub fn lien_telechargement(adresse_serveur: &str) -> String {
    adresse_serveur.trim_end_matches('/').to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compte {
    pub id: String,
    pub pseudo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeurRejoint {
    pub id: String,
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Etat {
    Demarrage,
    Deconnecte,
    AttenteNavigateur,
    Erreur(String),
    Verification,
    Injoignable(String),
    Refuse,
    AActiver {
        erreur: String,
        champ_en_faute: bool,
        en_cours: bool,
    },
    Invitation {
        erreur: String,
        en_cours: bool,
    },
    Connecte {
        compte: Compte,
        serveur: Option<ServeurRejoint>,
    },
    MiseAJourRequise {
        message: String,
        version_minimale: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Evenement {
    Lancer,
    SeConnecter,
    CreerCompte,
    Annuler,
    Reessayer,
    SeDeconnecter,
    Activer(String),
    OuvrirInvitation,
    Rejoindre(String),
    RetourAccueil,
    Telecharger,
    OuvrirPageTelechargement,
    SessionPerdue,
    AccesPerdu,
    MiseAJourSignalee {
        message: String,
        version_minimale: String,
    },
    RepriseTerminee(Result<bool, ErreurAuth>),
    ConnexionTerminee {
        tentative: u64,
        resultat: Result<(), ErreurAuth>,
    },
    CompteVerifie(Result<GetMeReponse, ErreurApi>),
    ActivationTerminee(Result<Person, ErreurApi>),
    RejointeTerminee(Result<Server, ErreurApi>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effet {
    Reprendre,
    Connecter { parcours: Parcours, tentative: u64 },
    AnnulerConnexion,
    VerifierCompte,
    Activer(String),
    Rejoindre(String),
    Deconnecter,
    OuvrirTelechargement,
    Installer,
}

fn refus_de_version(erreur: &ErreurApi) -> Option<(String, String)> {
    let ErreurApi::MiseAJourRequise(refus) = erreur else {
        return None;
    };
    let minimale = refus
        .donnees
        .get("min_version")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    Some((refus.message.clone(), minimale.to_string()))
}

impl Evenement {
    fn mise_a_jour(&self) -> Option<(String, String)> {
        match self {
            Self::MiseAJourSignalee {
                message,
                version_minimale,
            } => Some((message.clone(), version_minimale.clone())),
            Self::CompteVerifie(Err(e)) => refus_de_version(e),
            Self::ActivationTerminee(Err(e)) => refus_de_version(e),
            Self::RejointeTerminee(Err(e)) => refus_de_version(e),
            _ => None,
        }
    }
}

pub fn message_de_connexion(erreur: &ErreurAuth) -> String {
    match erreur {
        ErreurAuth::Annulee => "Connexion annulée.".to_string(),
        ErreurAuth::DelaiDepasse => {
            "Le navigateur n'a pas terminé la connexion à temps. Recommence quand tu es prêt.".to_string()
        }
        ErreurAuth::NavigateurIndisponible(_) => {
            "Impossible d'ouvrir le navigateur. Vérifie qu'un navigateur par défaut est installé, puis réessaie.".to_string()
        }
        ErreurAuth::Refusee { message, .. } => format!("Connexion refusée : {message}"),
        ErreurAuth::SessionExpiree => "Ta session a expiré : reconnecte-toi.".to_string(),
        ErreurAuth::Reseau(_) => {
            "Le service de connexion est injoignable. Vérifie ta connexion à Internet.".to_string()
        }
        ErreurAuth::Serveur(statut) => {
            format!("Le service de connexion ne répond pas correctement ({statut}). Réessaie dans un instant.")
        }
        ErreurAuth::ReponseInvalide(_) | ErreurAuth::Systeme(_) => {
            format!("Connexion impossible : {erreur}.")
        }
    }
}

pub fn message_api(erreur: &ErreurApi) -> String {
    let message = erreur.message();
    if message.is_empty() {
        erreur.to_string()
    } else {
        message.to_string()
    }
}

pub fn pseudo_valide(pseudo: &str) -> bool {
    let longueur = pseudo.chars().count();
    (3..=20).contains(&longueur)
        && pseudo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

pub struct Machine {
    etat: Etat,
    compte: Option<Compte>,
    invitation: Option<String>,
    tentative: u64,
    installation: bool,
}

impl Machine {
    pub fn nouvelle(invitation: Option<String>) -> Self {
        Self {
            etat: Etat::Demarrage,
            compte: None,
            invitation: invitation
                .map(|saisie| code_d_invitation(&saisie))
                .filter(|code| !code.is_empty()),
            tentative: 0,
            installation: false,
        }
    }

    pub fn avec_installation(mut self, installation: bool) -> Self {
        self.installation = installation;
        self
    }

    pub fn etat(&self) -> &Etat {
        &self.etat
    }

    pub fn invitation_en_attente(&self) -> Option<&str> {
        self.invitation.as_deref()
    }

    fn verifier(&mut self) -> Option<Effet> {
        self.etat = Etat::Verification;
        Some(Effet::VerifierCompte)
    }

    fn mettre_a_jour(&mut self, message: String, version_minimale: String) {
        self.etat = match std::mem::replace(&mut self.etat, Etat::Demarrage) {
            Etat::MiseAJourRequise {
                message: avant,
                version_minimale: minimale_avant,
            } => Etat::MiseAJourRequise {
                message: if avant.is_empty() { message } else { avant },
                version_minimale: if minimale_avant.is_empty() {
                    version_minimale
                } else {
                    minimale_avant
                },
            },
            _ => Etat::MiseAJourRequise {
                message,
                version_minimale,
            },
        };
    }

    pub fn traiter(&mut self, evenement: Evenement) -> Option<Effet> {
        if let Some((message, minimale)) = evenement.mise_a_jour() {
            self.mettre_a_jour(message, minimale);
            return None;
        }
        if matches!(self.etat, Etat::MiseAJourRequise { .. }) {
            return match evenement {
                Evenement::Telecharger if self.installation => Some(Effet::Installer),
                Evenement::Telecharger | Evenement::OuvrirPageTelechargement => {
                    Some(Effet::OuvrirTelechargement)
                }
                _ => None,
            };
        }
        match (evenement, &self.etat) {
            (Evenement::Lancer, Etat::Demarrage) | (Evenement::Reessayer, Etat::Injoignable(_)) => {
                self.etat = Etat::Demarrage;
                Some(Effet::Reprendre)
            }
            (Evenement::RepriseTerminee(resultat), Etat::Demarrage) => match resultat {
                Ok(true) => self.verifier(),
                Ok(false) => {
                    self.etat = Etat::Deconnecte;
                    None
                }
                Err(erreur) if erreur.exige_connexion() => {
                    self.etat = Etat::Erreur(message_de_connexion(&erreur));
                    None
                }
                Err(erreur) => {
                    self.etat = Etat::Injoignable(message_de_connexion(&erreur));
                    None
                }
            },
            (
                ev @ (Evenement::SeConnecter | Evenement::CreerCompte),
                Etat::Deconnecte | Etat::Erreur(_),
            ) => {
                self.tentative += 1;
                self.etat = Etat::AttenteNavigateur;
                let parcours = if ev == Evenement::CreerCompte {
                    Parcours::Inscription
                } else {
                    Parcours::Connexion
                };
                Some(Effet::Connecter {
                    parcours,
                    tentative: self.tentative,
                })
            }
            (Evenement::Annuler, Etat::AttenteNavigateur) => {
                self.etat = Etat::Deconnecte;
                Some(Effet::AnnulerConnexion)
            }
            (
                Evenement::ConnexionTerminee {
                    tentative,
                    resultat,
                },
                Etat::AttenteNavigateur,
            ) if tentative == self.tentative => match resultat {
                Ok(()) => self.verifier(),
                Err(ErreurAuth::Annulee) => {
                    self.etat = Etat::Deconnecte;
                    None
                }
                Err(erreur) => {
                    self.etat = Etat::Erreur(message_de_connexion(&erreur));
                    None
                }
            },
            (Evenement::CompteVerifie(resultat), Etat::Verification) => match resultat {
                Ok(moi) => self.accueillir(moi),
                Err(ErreurApi::NonAutorise(_)) => {
                    self.etat = Etat::Erreur(REFUS_DU_SERVEUR.to_string());
                    None
                }
                Err(erreur) => {
                    self.etat = Etat::Injoignable(message_api(&erreur));
                    None
                }
            },
            (Evenement::Reessayer, Etat::Refuse) => self.verifier(),
            (
                Evenement::SeDeconnecter,
                Etat::Refuse
                | Etat::AActiver { .. }
                | Etat::Invitation { .. }
                | Etat::Connecte { .. }
                | Etat::Injoignable(_),
            ) => {
                self.compte = None;
                self.etat = Etat::Deconnecte;
                Some(Effet::Deconnecter)
            }
            (
                Evenement::Activer(pseudo),
                Etat::AActiver {
                    en_cours: false, ..
                },
            ) => {
                let pseudo = pseudo.trim().to_string();
                if !pseudo_valide(&pseudo) {
                    self.etat = Etat::AActiver {
                        erreur: AIDE_PSEUDO.to_string(),
                        champ_en_faute: true,
                        en_cours: false,
                    };
                    return None;
                }
                self.etat = Etat::AActiver {
                    erreur: String::new(),
                    champ_en_faute: false,
                    en_cours: true,
                };
                Some(Effet::Activer(pseudo))
            }
            (Evenement::ActivationTerminee(resultat), Etat::AActiver { en_cours: true, .. }) => {
                match resultat {
                    Ok(_) => self.verifier(),
                    Err(erreur) if matches!(erreur.code(), "already_activated" | "forbidden") => {
                        self.verifier()
                    }
                    Err(erreur) => {
                        self.etat = Etat::AActiver {
                            erreur: message_api(&erreur),
                            champ_en_faute: erreur.code().contains("pseudo"),
                            en_cours: false,
                        };
                        None
                    }
                }
            }
            (Evenement::OuvrirInvitation, Etat::Connecte { .. }) => {
                self.etat = Etat::Invitation {
                    erreur: String::new(),
                    en_cours: false,
                };
                None
            }
            (
                Evenement::Rejoindre(saisie),
                Etat::Invitation {
                    en_cours: false, ..
                },
            ) => {
                let code = code_d_invitation(&saisie);
                if code.is_empty() {
                    return None;
                }
                self.rejoindre(code)
            }
            (Evenement::RejointeTerminee(resultat), Etat::Invitation { en_cours: true, .. }) => {
                match resultat {
                    Ok(serveur) => {
                        let serveur = ServeurRejoint {
                            id: serveur.id.unwrap_or_default(),
                            nom: serveur.name.unwrap_or_default(),
                        };
                        self.entrer(Some(serveur))
                    }
                    Err(erreur) => {
                        let erreur = if matches!(erreur, ErreurApi::Introuvable(_)) {
                            INVITATION_INVALIDE.to_string()
                        } else {
                            message_api(&erreur)
                        };
                        self.etat = Etat::Invitation {
                            erreur,
                            en_cours: false,
                        };
                        None
                    }
                }
            }
            (
                Evenement::RetourAccueil,
                Etat::Invitation {
                    en_cours: false, ..
                },
            ) => self.entrer(None),
            (Evenement::SessionPerdue, Etat::Connecte { .. } | Etat::Invitation { .. }) => {
                self.compte = None;
                self.etat = Etat::Erreur(message_de_connexion(&ErreurAuth::SessionExpiree));
                None
            }
            (Evenement::AccesPerdu, Etat::Connecte { .. } | Etat::Invitation { .. }) => {
                self.compte = None;
                self.verifier()
            }
            _ => None,
        }
    }

    fn rejoindre(&mut self, code: String) -> Option<Effet> {
        self.etat = Etat::Invitation {
            erreur: String::new(),
            en_cours: true,
        };
        Some(Effet::Rejoindre(code))
    }

    fn entrer(&mut self, serveur: Option<ServeurRejoint>) -> Option<Effet> {
        if let Some(compte) = self.compte.clone() {
            self.etat = Etat::Connecte { compte, serveur };
        }
        None
    }

    fn accueillir(&mut self, moi: GetMeReponse) -> Option<Effet> {
        if moi.allowed != Some(true) {
            self.etat = Etat::Refuse;
            return None;
        }
        if moi.activated != Some(true) {
            self.etat = Etat::AActiver {
                erreur: String::new(),
                champ_en_faute: false,
                en_cours: false,
            };
            return None;
        }
        self.compte = Some(Compte {
            id: moi.id.unwrap_or_default(),
            pseudo: moi.pseudo.unwrap_or_default(),
        });
        match self.invitation.take() {
            Some(code) => self.rejoindre(code),
            None => self.entrer(None),
        }
    }
}

#[derive(Clone)]
pub struct Emetteur(mpsc::UnboundedSender<Evenement>);

impl Emetteur {
    pub fn envoyer(&self, evenement: Evenement) {
        let _ = self.0.send(evenement);
    }
}

pub struct Reception(mpsc::UnboundedReceiver<Evenement>);

pub fn canal() -> (Emetteur, Reception) {
    let (envoi, reception) = mpsc::unbounded_channel();
    (Emetteur(envoi), Reception(reception))
}

pub type Publieur = Arc<dyn Fn(&Etat) + Send + Sync>;

pub struct Entree {
    emetteur: Emetteur,
    etat: watch::Receiver<Etat>,
    runtime: tokio::runtime::Handle,
    arret: Option<oneshot::Sender<()>>,
    fil: Option<JoinHandle<()>>,
}

impl Entree {
    pub fn demarrer(
        machine: Machine,
        services: Services,
        (emetteur, reception): (Emetteur, Reception),
        publier: Publieur,
    ) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        let (envoi_etat, etat) = watch::channel(machine.etat().clone());
        let (arret, attente_arret) = oneshot::channel::<()>();
        let boucle_emetteur = emetteur.clone();
        let fil = std::thread::Builder::new()
            .name("estompe-entree".into())
            .spawn(move || {
                runtime.block_on(async move {
                    tokio::select! {
                        () = boucle(machine, Arc::new(services), reception, boucle_emetteur, envoi_etat, publier) => {}
                        _ = attente_arret => {}
                    }
                });
            })?;
        emetteur.envoyer(Evenement::Lancer);
        Ok(Self {
            emetteur,
            etat,
            runtime: handle,
            arret: Some(arret),
            fil: Some(fil),
        })
    }

    pub fn envoyer(&self, evenement: Evenement) {
        self.emetteur.envoyer(evenement);
    }

    pub fn emetteur(&self) -> Emetteur {
        self.emetteur.clone()
    }

    pub fn etat(&self) -> Etat {
        self.etat.borrow().clone()
    }

    pub fn observer(&self) -> watch::Receiver<Etat> {
        self.etat.clone()
    }

    pub fn runtime(&self) -> tokio::runtime::Handle {
        self.runtime.clone()
    }

    pub fn attendre(
        &self,
        condition: impl Fn(&Etat) -> bool,
        delai: Duration,
    ) -> Result<Etat, Etat> {
        let mut etat = self.etat.clone();
        let attente = self.runtime.block_on(async {
            tokio::time::timeout(delai, etat.wait_for(|e| condition(e)))
                .await
                .map(|r| r.map(|e| e.clone()))
        });
        match attente {
            Ok(Ok(atteint)) => Ok(atteint),
            _ => Err(self.etat()),
        }
    }
}

impl Drop for Entree {
    fn drop(&mut self) {
        if let Some(arret) = self.arret.take() {
            let _ = arret.send(());
        }
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

async fn boucle(
    mut machine: Machine,
    services: Arc<Services>,
    mut reception: Reception,
    emetteur: Emetteur,
    etat: watch::Sender<Etat>,
    publier: Publieur,
) {
    publier(machine.etat());
    while let Some(evenement) = reception.0.recv().await {
        let effet = machine.traiter(evenement);
        if *etat.borrow() != *machine.etat() {
            services.noter(
                Niveau::Info,
                &format!("écran : {}", nom_de_l_etat(machine.etat())),
            );
            etat.send_replace(machine.etat().clone());
            publier(machine.etat());
        }
        if let Some(effet) = effet {
            executer(effet, &services, &emetteur);
        }
    }
}

fn nom_de_l_etat(etat: &Etat) -> &'static str {
    match etat {
        Etat::Demarrage => "démarrage",
        Etat::Deconnecte => "déconnecté",
        Etat::AttenteNavigateur => "attente du navigateur",
        Etat::Erreur(_) => "erreur de connexion",
        Etat::Verification => "vérification du compte",
        Etat::Injoignable(_) => "serveur injoignable",
        Etat::Refuse => "accès refusé",
        Etat::AActiver { .. } => "activation du compte",
        Etat::Invitation { .. } => "invitation",
        Etat::Connecte { .. } => "connecté",
        Etat::MiseAJourRequise { .. } => "mise à jour requise",
    }
}

fn lancer<T: Send + 'static>(
    emetteur: &Emetteur,
    futur: Futur<T>,
    evenement: impl FnOnce(T) -> Evenement + Send + 'static,
) {
    let emetteur = emetteur.clone();
    tokio::spawn(async move {
        let resultat = futur.await;
        emetteur.envoyer(evenement(resultat));
    });
}

fn executer(effet: Effet, services: &Arc<Services>, emetteur: &Emetteur) {
    match effet {
        Effet::Reprendre => lancer(
            emetteur,
            services.auth.reprendre(),
            Evenement::RepriseTerminee,
        ),
        Effet::Connecter {
            parcours,
            tentative,
        } => lancer(
            emetteur,
            services.auth.connecter(parcours),
            move |resultat| Evenement::ConnexionTerminee {
                tentative,
                resultat,
            },
        ),
        Effet::AnnulerConnexion => services.auth.annuler(),
        Effet::VerifierCompte => lancer(emetteur, services.api.moi(), Evenement::CompteVerifie),
        Effet::Activer(pseudo) => lancer(
            emetteur,
            services.api.activer(pseudo),
            Evenement::ActivationTerminee,
        ),
        Effet::Rejoindre(code) => lancer(
            emetteur,
            services.api.rejoindre(code),
            Evenement::RejointeTerminee,
        ),
        Effet::Deconnecter => {
            tokio::spawn(services.auth.deconnecter());
        }
        Effet::Installer => match &services.installer {
            Some(installer) => installer(),
            None => executer(Effet::OuvrirTelechargement, services, emetteur),
        },
        Effet::OuvrirTelechargement => {
            let services = services.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(erreur) = services.navigateur.ouvrir(&services.lien_telechargement) {
                    services.noter(
                        Niveau::Erreur,
                        &format!("lien de téléchargement non ouvert : {erreur}"),
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::http::Refus;
    use serde_json::{json, Map, Value};

    fn moi(allowed: bool, activated: bool) -> GetMeReponse {
        GetMeReponse {
            allowed: Some(allowed),
            activated: Some(activated),
            id: Some("u1".into()),
            pseudo: Some("bob".into()),
            ..GetMeReponse::default()
        }
    }

    fn refus(statut: u16, code: &str, message: &str, donnees: Value) -> Refus {
        let Value::Object(donnees) = donnees else {
            panic!("objet attendu")
        };
        Refus {
            statut,
            code: code.into(),
            message: message.into(),
            donnees,
        }
    }

    fn compte() -> Compte {
        Compte {
            id: "u1".into(),
            pseudo: "bob".into(),
        }
    }

    fn connectee(invitation: Option<&str>) -> Machine {
        let mut m = Machine::nouvelle(invitation.map(String::from));
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Ok(true)));
        m
    }

    #[test]
    fn sans_session_la_reprise_mene_a_l_ecran_de_connexion() {
        let mut m = Machine::nouvelle(None);
        assert_eq!(*m.etat(), Etat::Demarrage);
        assert_eq!(m.traiter(Evenement::Lancer), Some(Effet::Reprendre));
        assert_eq!(m.traiter(Evenement::RepriseTerminee(Ok(false))), None);
        assert_eq!(*m.etat(), Etat::Deconnecte);
    }

    #[test]
    fn connexion_puis_verification_du_compte_jusqu_a_la_coquille() {
        let mut m = Machine::nouvelle(None);
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Ok(false)));
        assert_eq!(
            m.traiter(Evenement::SeConnecter),
            Some(Effet::Connecter {
                parcours: Parcours::Connexion,
                tentative: 1
            })
        );
        assert_eq!(*m.etat(), Etat::AttenteNavigateur);
        assert_eq!(
            m.traiter(Evenement::ConnexionTerminee {
                tentative: 1,
                resultat: Ok(())
            }),
            Some(Effet::VerifierCompte)
        );
        assert_eq!(*m.etat(), Etat::Verification);
        m.traiter(Evenement::CompteVerifie(Ok(moi(true, true))));
        assert_eq!(
            *m.etat(),
            Etat::Connecte {
                compte: compte(),
                serveur: None
            }
        );
    }

    #[test]
    fn la_session_perdue_ramene_a_la_connexion_et_l_acces_retire_a_la_verification() {
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Ok(moi(true, true))));
        assert!(matches!(m.etat(), Etat::Connecte { .. }));
        assert_eq!(m.traiter(Evenement::SessionPerdue), None);
        assert_eq!(
            *m.etat(),
            Etat::Erreur(message_de_connexion(&ErreurAuth::SessionExpiree))
        );
        assert_eq!(m.traiter(Evenement::SessionPerdue), None);
        assert_eq!(m.traiter(Evenement::AccesPerdu), None);
        assert!(matches!(m.etat(), Etat::Erreur(_)));

        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Ok(moi(true, true))));
        m.traiter(Evenement::OuvrirInvitation);
        assert_eq!(
            m.traiter(Evenement::AccesPerdu),
            Some(Effet::VerifierCompte)
        );
        assert_eq!(*m.etat(), Etat::Verification);
        m.traiter(Evenement::CompteVerifie(Ok(moi(false, true))));
        assert_eq!(*m.etat(), Etat::Refuse);
    }

    #[test]
    fn creer_un_compte_ouvre_le_parcours_d_inscription() {
        let mut m = Machine::nouvelle(None);
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Ok(false)));
        assert_eq!(
            m.traiter(Evenement::CreerCompte),
            Some(Effet::Connecter {
                parcours: Parcours::Inscription,
                tentative: 1
            })
        );
    }

    #[test]
    fn annuler_revient_a_la_connexion_et_ignore_le_resultat_de_la_tentative_abandonnee() {
        let mut m = Machine::nouvelle(None);
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Ok(false)));
        m.traiter(Evenement::SeConnecter);
        assert_eq!(m.traiter(Evenement::Annuler), Some(Effet::AnnulerConnexion));
        assert_eq!(*m.etat(), Etat::Deconnecte);
        m.traiter(Evenement::SeConnecter);
        assert_eq!(
            m.traiter(Evenement::ConnexionTerminee {
                tentative: 1,
                resultat: Err(ErreurAuth::Annulee)
            }),
            None
        );
        assert_eq!(*m.etat(), Etat::AttenteNavigateur);
        m.traiter(Evenement::ConnexionTerminee {
            tentative: 2,
            resultat: Err(ErreurAuth::Annulee),
        });
        assert_eq!(*m.etat(), Etat::Deconnecte);
    }

    #[test]
    fn les_erreurs_de_connexion_ont_un_message_et_permettent_un_nouvel_essai() {
        for (erreur, extrait) in [
            (ErreurAuth::DelaiDepasse, "à temps"),
            (
                ErreurAuth::NavigateurIndisponible("x".into()),
                "ouvrir le navigateur",
            ),
            (
                ErreurAuth::Refusee {
                    code: "invalid_grant".into(),
                    message: "Session not active".into(),
                },
                "Connexion refusée : Session not active",
            ),
            (ErreurAuth::Reseau("dns".into()), "injoignable"),
            (ErreurAuth::Serveur(503), "503"),
        ] {
            let mut m = Machine::nouvelle(None);
            m.traiter(Evenement::Lancer);
            m.traiter(Evenement::RepriseTerminee(Ok(false)));
            m.traiter(Evenement::SeConnecter);
            m.traiter(Evenement::ConnexionTerminee {
                tentative: 1,
                resultat: Err(erreur.clone()),
            });
            let Etat::Erreur(message) = m.etat().clone() else {
                panic!("{erreur:?} : {:?}", m.etat())
            };
            assert!(message.contains(extrait), "{message}");
            assert!(matches!(
                m.traiter(Evenement::SeConnecter),
                Some(Effet::Connecter { tentative: 2, .. })
            ));
        }
    }

    #[test]
    fn reprise_impossible_session_expiree_ou_serveur_injoignable() {
        let mut m = Machine::nouvelle(None);
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Err(ErreurAuth::SessionExpiree)));
        assert_eq!(
            *m.etat(),
            Etat::Erreur("Ta session a expiré : reconnecte-toi.".into())
        );
        let mut m = Machine::nouvelle(None);
        m.traiter(Evenement::Lancer);
        m.traiter(Evenement::RepriseTerminee(Err(ErreurAuth::Reseau(
            "x".into(),
        ))));
        assert!(matches!(m.etat(), Etat::Injoignable(_)));
        assert_eq!(m.traiter(Evenement::Reessayer), Some(Effet::Reprendre));
        assert_eq!(*m.etat(), Etat::Demarrage);
    }

    #[test]
    fn compte_sans_permission_puis_reessayer_et_se_deconnecter() {
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Ok(moi(false, false))));
        assert_eq!(*m.etat(), Etat::Refuse);
        assert_eq!(m.traiter(Evenement::Reessayer), Some(Effet::VerifierCompte));
        m.traiter(Evenement::CompteVerifie(Ok(moi(false, true))));
        assert_eq!(
            m.traiter(Evenement::SeDeconnecter),
            Some(Effet::Deconnecter)
        );
        assert_eq!(*m.etat(), Etat::Deconnecte);
    }

    #[test]
    fn me_refuse_401_ou_serveur_injoignable() {
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Err(ErreurApi::NonAutorise(
            refus(401, "unauthorized", "", json!({})),
        ))));
        assert_eq!(*m.etat(), Etat::Erreur(REFUS_DU_SERVEUR.into()));
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Err(ErreurApi::Reseau("x".into()))));
        assert_eq!(
            *m.etat(),
            Etat::Injoignable("Connexion au serveur impossible.".into())
        );
    }

    #[test]
    fn activation_valide_le_pseudo_avant_d_appeler_le_serveur() {
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Ok(moi(true, false))));
        assert!(matches!(
            m.etat(),
            Etat::AActiver {
                en_cours: false,
                ..
            }
        ));
        for invalide in ["ab", "nom avec espace", "abcdefghijklmnopqrstu", "é-é-é"] {
            assert_eq!(m.traiter(Evenement::Activer(invalide.into())), None);
            assert_eq!(
                *m.etat(),
                Etat::AActiver {
                    erreur: AIDE_PSEUDO.into(),
                    champ_en_faute: true,
                    en_cours: false
                }
            );
        }
        assert_eq!(
            m.traiter(Evenement::Activer("  bob_2.x-y ".into())),
            Some(Effet::Activer("bob_2.x-y".into()))
        );
        assert_eq!(m.traiter(Evenement::Activer("autre".into())), None);
        m.traiter(Evenement::ActivationTerminee(Err(ErreurApi::Conflit(
            refus(409, "pseudo_taken", "pseudo déjà pris", json!({})),
        ))));
        assert_eq!(
            *m.etat(),
            Etat::AActiver {
                erreur: "pseudo déjà pris".into(),
                champ_en_faute: true,
                en_cours: false
            }
        );
        m.traiter(Evenement::Activer("bob".into()));
        assert_eq!(
            m.traiter(Evenement::ActivationTerminee(Ok(Person::default()))),
            Some(Effet::VerifierCompte)
        );
    }

    #[test]
    fn activation_deja_faite_ou_interdite_relit_le_compte() {
        for code in ["already_activated", "forbidden"] {
            let mut m = connectee(None);
            m.traiter(Evenement::CompteVerifie(Ok(moi(true, false))));
            m.traiter(Evenement::Activer("bob".into()));
            assert_eq!(
                m.traiter(Evenement::ActivationTerminee(Err(ErreurApi::Autre(refus(
                    400,
                    code,
                    "x",
                    json!({})
                ))))),
                Some(Effet::VerifierCompte),
                "{code}"
            );
        }
    }

    #[test]
    fn une_invitation_du_lancement_est_rejointe_apres_la_verification() {
        let mut m = connectee(Some("https://chat.plbls.fr/#/join/abcde-23456"));
        assert_eq!(m.invitation_en_attente(), Some("ABCDE-23456"));
        assert_eq!(
            m.traiter(Evenement::CompteVerifie(Ok(moi(true, true)))),
            Some(Effet::Rejoindre("ABCDE-23456".into()))
        );
        assert_eq!(
            *m.etat(),
            Etat::Invitation {
                erreur: String::new(),
                en_cours: true
            }
        );
        m.traiter(Evenement::RejointeTerminee(Ok(Server {
            id: Some("s1".into()),
            name: Some("Amis".into()),
            ..Server::default()
        })));
        assert_eq!(
            *m.etat(),
            Etat::Connecte {
                compte: compte(),
                serveur: Some(ServeurRejoint {
                    id: "s1".into(),
                    nom: "Amis".into()
                })
            }
        );
        assert_eq!(m.invitation_en_attente(), None);
    }

    #[test]
    fn invitation_invalide_puis_saisie_d_un_autre_code_ou_retour() {
        let mut m = connectee(None);
        m.traiter(Evenement::CompteVerifie(Ok(moi(true, true))));
        assert_eq!(m.traiter(Evenement::OuvrirInvitation), None);
        assert_eq!(m.traiter(Evenement::Rejoindre("   ".into())), None);
        assert_eq!(
            m.traiter(Evenement::Rejoindre("zzzzz-11111".into())),
            Some(Effet::Rejoindre("ZZZZZ-11111".into()))
        );
        m.traiter(Evenement::RejointeTerminee(Err(ErreurApi::Introuvable(
            refus(404, "not_found", "introuvable", json!({})),
        ))));
        assert_eq!(
            *m.etat(),
            Etat::Invitation {
                erreur: INVITATION_INVALIDE.into(),
                en_cours: false
            }
        );
        m.traiter(Evenement::RetourAccueil);
        assert_eq!(
            *m.etat(),
            Etat::Connecte {
                compte: compte(),
                serveur: None
            }
        );
    }

    #[test]
    fn un_refus_426_bloque_tout_sauf_le_lien_de_telechargement() {
        let mut m = connectee(None);
        let erreur = ErreurApi::MiseAJourRequise(refus(
            426,
            "upgrade_required",
            "faille corrigée",
            json!({"min_version": "9.0.0"}),
        ));
        assert_eq!(m.traiter(Evenement::CompteVerifie(Err(erreur))), None);
        let attendu = Etat::MiseAJourRequise {
            message: "faille corrigée".into(),
            version_minimale: "9.0.0".into(),
        };
        assert_eq!(*m.etat(), attendu);
        for evenement in [
            Evenement::Lancer,
            Evenement::Reessayer,
            Evenement::SeConnecter,
            Evenement::SeDeconnecter,
            Evenement::Activer("bob".into()),
            Evenement::Rejoindre("ABCDE-23456".into()),
            Evenement::OuvrirInvitation,
            Evenement::RepriseTerminee(Ok(true)),
            Evenement::CompteVerifie(Ok(moi(true, true))),
        ] {
            assert_eq!(m.traiter(evenement.clone()), None, "{evenement:?}");
            assert_eq!(*m.etat(), attendu);
        }
        assert_eq!(
            m.traiter(Evenement::Telecharger),
            Some(Effet::OuvrirTelechargement)
        );
    }

    #[test]
    fn l_ecran_force_installe_directement_quand_l_installation_est_disponible() {
        let signal = || Evenement::MiseAJourSignalee {
            message: "obligatoire".into(),
            version_minimale: "0.2.0".into(),
        };
        let mut m = connectee(None).avec_installation(true);
        m.traiter(signal());
        assert_eq!(m.traiter(Evenement::Telecharger), Some(Effet::Installer));
        assert_eq!(
            m.traiter(Evenement::OuvrirPageTelechargement),
            Some(Effet::OuvrirTelechargement)
        );
        assert_eq!(m.traiter(Evenement::Lancer), None);
        let mut sans = connectee(None);
        sans.traiter(signal());
        assert_eq!(
            sans.traiter(Evenement::Telecharger),
            Some(Effet::OuvrirTelechargement)
        );
        let mut hors_ecran = connectee(None).avec_installation(true);
        assert_eq!(hors_ecran.traiter(Evenement::Telecharger), None);
        assert_eq!(
            hors_ecran.traiter(Evenement::OuvrirPageTelechargement),
            None
        );
    }

    #[test]
    fn le_signal_du_client_et_le_refus_se_completent() {
        let mut m = connectee(None);
        m.traiter(Evenement::MiseAJourSignalee {
            message: "trop ancienne".into(),
            version_minimale: String::new(),
        });
        m.traiter(Evenement::CompteVerifie(Err(ErreurApi::MiseAJourRequise(
            refus(
                426,
                "upgrade_required",
                "autre",
                json!({"min_version": "2.0.0"}),
            ),
        ))));
        assert_eq!(
            *m.etat(),
            Etat::MiseAJourRequise {
                message: "trop ancienne".into(),
                version_minimale: "2.0.0".into()
            }
        );
        let sans_donnees = ErreurApi::MiseAJourRequise(Refus {
            statut: 426,
            code: "upgrade_required".into(),
            message: String::new(),
            donnees: Map::new(),
        });
        assert_eq!(
            refus_de_version(&sans_donnees),
            Some((String::new(), String::new()))
        );
    }

    #[test]
    fn le_lien_de_telechargement_est_la_page_du_serveur() {
        assert_eq!(
            lien_telechargement("https://chat.plbls.fr/"),
            "https://chat.plbls.fr"
        );
    }
}
