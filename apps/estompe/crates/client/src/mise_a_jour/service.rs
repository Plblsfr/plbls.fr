use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use estompe_plateforme::Installation;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::machine::{Contexte, EffetMaj, EtatMaj, EvenementMaj, MachineMaj};
use super::manifeste::Paquet;
use super::signature::verifier_fichier;
use super::telechargement::{deplacer, Telechargeur};
use super::{chemin_partiel, nom_de_fichier, ErreurMiseAJour, ATTENTE_AVANT_BLOCAGE};
use crate::journal::{Journal, Niveau};
use crate::version::MiseAJourDisponible;

pub const PREMIERE_VERIFICATION: Duration = Duration::from_secs(15);
pub const VERIFICATION_PERIODIQUE: Duration = Duration::from_secs(30 * 60);
pub const INTERVALLE_PREMIER_PLAN: Duration = Duration::from_secs(5 * 60);
const ESSAIS_DE_TELECHARGEMENT: usize = 3;
const SOURCE_JOURNAL: &str = "mise-a-jour";

pub type Futur<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub type Progres = Arc<dyn Fn(u64, u64) + Send + Sync>;
pub type SourceContexte = Arc<dyn Fn() -> Futur<Contexte> + Send + Sync>;
pub type Redemarreur = Arc<dyn Fn() + Send + Sync>;
pub type Bloqueur = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub trait SourceMaj: Send + Sync + 'static {
    fn verifier(&self) -> Futur<Result<Option<MiseAJourDisponible>, ErreurMiseAJour>>;
    fn telecharger(
        &self,
        mise_a_jour: MiseAJourDisponible,
        progres: Progres,
    ) -> Futur<Result<PathBuf, ErreurMiseAJour>>;
    fn controler(
        &self,
        mise_a_jour: &MiseAJourDisponible,
        installeur: &Path,
    ) -> Result<(), ErreurMiseAJour>;
}

struct InterneReseau {
    telechargeur: Telechargeur,
    adresse: String,
    cle: String,
    cible: String,
    version_installee: String,
    dossier: PathBuf,
    dernier: Mutex<Option<(String, Paquet)>>,
}

#[derive(Clone)]
pub struct SourceReseau(Arc<InterneReseau>);

impl SourceReseau {
    pub fn nouvelle(
        telechargeur: Telechargeur,
        adresse: &str,
        cle: &str,
        cible: &str,
        version_installee: &str,
        dossier: PathBuf,
    ) -> Self {
        Self(Arc::new(InterneReseau {
            telechargeur,
            adresse: adresse.to_string(),
            cle: cle.to_string(),
            cible: cible.to_string(),
            version_installee: version_installee.to_string(),
            dossier,
            dernier: Mutex::new(None),
        }))
    }

    fn paquet(&self, version: &str) -> Option<Paquet> {
        self.0
            .dernier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|(v, _)| v == version)
            .map(|(_, paquet)| paquet.clone())
    }

    async fn lire(&self) -> Result<Option<MiseAJourDisponible>, ErreurMiseAJour> {
        let manifeste = self.0.telechargeur.manifeste(&self.0.adresse).await?;
        let disponible = manifeste.disponibilite(&self.0.version_installee, &self.0.cible);
        *self.0.dernier.lock().unwrap_or_else(|e| e.into_inner()) = disponible
            .as_ref()
            .zip(manifeste.paquet_pour(&self.0.cible))
            .map(|(d, paquet)| (d.version.clone(), paquet.clone()));
        Ok(disponible)
    }

    async fn verifier_en_tache(
        chemin: PathBuf,
        signature: String,
        version: String,
        cle: String,
    ) -> Result<(), ErreurMiseAJour> {
        tokio::task::spawn_blocking(move || {
            let resultat = verifier_fichier(&chemin, &signature, &version, &cle);
            if resultat.is_err() {
                let _ = std::fs::remove_file(&chemin);
            }
            resultat
        })
        .await
        .unwrap_or_else(|e| Err(ErreurMiseAJour::Signature(e.to_string())))
    }

    async fn telecharger_et_verifier(
        self,
        mise_a_jour: MiseAJourDisponible,
        progres: Progres,
    ) -> Result<PathBuf, ErreurMiseAJour> {
        let paquet = match self.paquet(&mise_a_jour.version) {
            Some(paquet) => paquet,
            None => {
                self.lire().await?;
                self.paquet(&mise_a_jour.version)
                    .ok_or(ErreurMiseAJour::AucuneMiseAJour)?
            }
        };
        let destination = self.0.dossier.join(nom_de_fichier(&mise_a_jour.version));
        let partiel = chemin_partiel(&destination);
        let verification = |chemin: PathBuf| {
            Self::verifier_en_tache(
                chemin,
                paquet.signature.clone(),
                mise_a_jour.version.clone(),
                self.0.cle.clone(),
            )
        };
        if destination.exists() && verification(destination.clone()).await.is_ok() {
            if let Some(taille) = paquet.taille {
                progres(taille, taille);
            }
            return Ok(destination);
        }
        let mut essai = 0;
        let telecharge = loop {
            essai += 1;
            match self
                .0
                .telechargeur
                .telecharger(&paquet.url, &partiel, paquet.taille, progres.as_ref())
                .await
            {
                Ok(telecharge) => break telecharge,
                Err(ErreurMiseAJour::Reseau(_)) if essai < ESSAIS_DE_TELECHARGEMENT => continue,
                Err(erreur) => return Err(erreur),
            }
        };
        if let Some(attendus) = paquet.taille.filter(|t| *t != telecharge.octets) {
            let _ = std::fs::remove_file(&partiel);
            return Err(ErreurMiseAJour::Incomplet {
                recus: telecharge.octets,
                attendus,
            });
        }
        verification(partiel.clone()).await?;
        deplacer(&partiel, &destination)
    }
}

impl SourceMaj for SourceReseau {
    fn verifier(&self) -> Futur<Result<Option<MiseAJourDisponible>, ErreurMiseAJour>> {
        let source = self.clone();
        Box::pin(async move { source.lire().await })
    }

    fn telecharger(
        &self,
        mise_a_jour: MiseAJourDisponible,
        progres: Progres,
    ) -> Futur<Result<PathBuf, ErreurMiseAJour>> {
        Box::pin(self.clone().telecharger_et_verifier(mise_a_jour, progres))
    }

    fn controler(
        &self,
        mise_a_jour: &MiseAJourDisponible,
        installeur: &Path,
    ) -> Result<(), ErreurMiseAJour> {
        let paquet = self
            .paquet(&mise_a_jour.version)
            .ok_or(ErreurMiseAJour::AucuneMiseAJour)?;
        let resultat = verifier_fichier(
            installeur,
            &paquet.signature,
            &mise_a_jour.version,
            &self.0.cle,
        );
        if resultat.is_err() {
            let _ = std::fs::remove_file(installeur);
        }
        resultat
    }
}

#[derive(Clone)]
pub struct EmetteurMaj(mpsc::UnboundedSender<EvenementMaj>);

impl EmetteurMaj {
    pub fn envoyer(&self, evenement: EvenementMaj) {
        let _ = self.0.send(evenement);
    }
}

pub struct ReceptionMaj(mpsc::UnboundedReceiver<EvenementMaj>);

pub fn canal() -> (EmetteurMaj, ReceptionMaj) {
    let (envoi, reception) = mpsc::unbounded_channel();
    (EmetteurMaj(envoi), ReceptionMaj(reception))
}

pub struct OptionsMiseAJour {
    pub source: Arc<dyn SourceMaj>,
    pub installation: Arc<dyn Installation>,
    pub contexte: SourceContexte,
    pub redemarrer: Redemarreur,
    pub bloquer: Bloqueur,
    pub journal: Option<Arc<Journal>>,
}

#[derive(Clone)]
pub struct MiseAJour {
    emetteur: EmetteurMaj,
    etat: watch::Receiver<EtatMaj>,
}

impl MiseAJour {
    pub fn demarrer(
        runtime: &tokio::runtime::Handle,
        (emetteur, reception): (EmetteurMaj, ReceptionMaj),
        options: OptionsMiseAJour,
    ) -> Self {
        let automatique =
            options.installation.disponible() && options.installation.cible().is_some();
        let machine = MachineMaj::nouvelle(automatique);
        let (publication, etat) = watch::channel(machine.etat().clone());
        runtime.spawn(boucle(
            machine,
            reception,
            emetteur.clone(),
            publication,
            Arc::new(options),
        ));
        Self { emetteur, etat }
    }

    pub fn envoyer(&self, evenement: EvenementMaj) {
        self.emetteur.envoyer(evenement);
    }

    pub fn emetteur(&self) -> EmetteurMaj {
        self.emetteur.clone()
    }

    pub fn etat(&self) -> EtatMaj {
        self.etat.borrow().clone()
    }

    pub fn observer(&self) -> watch::Receiver<EtatMaj> {
        self.etat.clone()
    }
}

fn noter(options: &OptionsMiseAJour, niveau: Niveau, message: &str) {
    if let Some(journal) = &options.journal {
        journal.ecrire(niveau, SOURCE_JOURNAL, message);
    }
}

async fn boucle(
    mut machine: MachineMaj,
    mut reception: ReceptionMaj,
    emetteur: EmetteurMaj,
    publication: watch::Sender<EtatMaj>,
    options: Arc<OptionsMiseAJour>,
) {
    let mut derniere_lecture: Option<Instant> = None;
    while let Some(evenement) = reception.0.recv().await {
        if evenement == EvenementMaj::PremierPlan
            && derniere_lecture.is_some_and(|d| d.elapsed() < INTERVALLE_PREMIER_PLAN)
        {
            continue;
        }
        let contexte = if evenement.demande_le_contexte() {
            (options.contexte)().await
        } else {
            Contexte::default()
        };
        let effets = machine.traiter(evenement, contexte);
        publication.send_if_modified(|etat| {
            let change = etat != machine.etat();
            if change {
                *etat = machine.etat().clone();
            }
            change
        });
        for effet in effets {
            if effet == EffetMaj::LireManifeste {
                derniere_lecture = Some(Instant::now());
            }
            executer(effet, &emetteur, &options);
        }
    }
}

fn programmer(emetteur: EmetteurMaj) {
    tokio::spawn(async move {
        tokio::time::sleep(PREMIERE_VERIFICATION).await;
        emetteur.envoyer(EvenementMaj::Verifier { manuelle: false });
        loop {
            tokio::time::sleep(VERIFICATION_PERIODIQUE).await;
            emetteur.envoyer(EvenementMaj::Verifier { manuelle: false });
        }
    });
}

fn decompter(emetteur: EmetteurMaj) {
    tokio::spawn(async move {
        for _ in 0..ATTENTE_AVANT_BLOCAGE {
            tokio::time::sleep(Duration::from_secs(1)).await;
            emetteur.envoyer(EvenementMaj::Seconde);
        }
    });
}

fn progression(emetteur: EmetteurMaj) -> Progres {
    let dernier = Mutex::new(None::<u64>);
    Arc::new(move |fait, total| {
        let palier = (fait * 100).checked_div(total).unwrap_or(fait >> 20);
        let mut dernier = dernier.lock().unwrap_or_else(|e| e.into_inner());
        if *dernier != Some(palier) {
            *dernier = Some(palier);
            emetteur.envoyer(EvenementMaj::Progression { fait, total });
        }
    })
}

fn executer(effet: EffetMaj, emetteur: &EmetteurMaj, options: &Arc<OptionsMiseAJour>) {
    match effet {
        EffetMaj::Programmer => programmer(emetteur.clone()),
        EffetMaj::Decompter => decompter(emetteur.clone()),
        EffetMaj::LireManifeste => {
            let (emetteur, options) = (emetteur.clone(), options.clone());
            tokio::spawn(async move {
                let resultat = options.source.verifier().await;
                match &resultat {
                    Ok(Some(d)) => noter(
                        &options,
                        Niveau::Info,
                        &format!(
                            "version {} disponible{}",
                            d.version,
                            if d.imposee { " (imposée)" } else { "" }
                        ),
                    ),
                    Ok(None) => {}
                    Err(e) => noter(
                        &options,
                        Niveau::Avertissement,
                        &format!("vérification : {e}"),
                    ),
                }
                emetteur.envoyer(EvenementMaj::ManifesteLu(
                    resultat.map_err(|e| e.to_string()),
                ));
            });
        }
        EffetMaj::Bloquer {
            message,
            version_minimale,
        } => {
            noter(
                options,
                Niveau::Info,
                "mise à jour imposée : écran bloquant",
            );
            (options.bloquer)(&message, &version_minimale);
        }
        EffetMaj::Telecharger(mise_a_jour) => {
            let (emetteur, options) = (emetteur.clone(), options.clone());
            tokio::spawn(async move {
                let progres = progression(emetteur.clone());
                let resultat = options
                    .source
                    .telecharger(mise_a_jour.clone(), progres)
                    .await;
                match &resultat {
                    Ok(_) => noter(
                        &options,
                        Niveau::Info,
                        &format!("installeur {} téléchargé et vérifié", mise_a_jour.version),
                    ),
                    Err(e) => noter(
                        &options,
                        Niveau::Avertissement,
                        &format!("téléchargement : {e}"),
                    ),
                }
                emetteur.envoyer(EvenementMaj::Telecharge(
                    resultat.map_err(|e| e.to_string()),
                ));
            });
        }
        EffetMaj::Lancer {
            installeur,
            mise_a_jour,
        } => {
            let (emetteur, options) = (emetteur.clone(), options.clone());
            tokio::task::spawn_blocking(move || {
                let resultat = options
                    .source
                    .controler(&mise_a_jour, &installeur)
                    .and_then(|()| {
                        options
                            .installation
                            .lancer(&installeur)
                            .map_err(|e| ErreurMiseAJour::Installation(e.to_string()))
                    });
                match &resultat {
                    Ok(()) => noter(&options, Niveau::Info, "installeur lancé : redémarrage"),
                    Err(e) => noter(&options, Niveau::Erreur, &format!("installation : {e}")),
                }
                emetteur.envoyer(EvenementMaj::Lance(resultat.map_err(|e| e.to_string())));
            });
        }
        EffetMaj::Redemarrer => (options.redemarrer)(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mise_a_jour::essais::{
        installeur, politique_locale, CLE_ESSAI, SIGNATURE_ANCIENNE, SIGNATURE_BONNE,
    };
    use crate::mise_a_jour::machine::Phase;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use estompe_plateforme::{Erreur, Resultat};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const CIBLE: &str = "windows-x86_64";
    const DELAI: Duration = Duration::from_secs(10);

    #[derive(Default)]
    struct FausseInstallation {
        lancements: Mutex<Vec<(PathBuf, Vec<u8>)>>,
        echoue: bool,
    }

    impl Installation for FausseInstallation {
        fn disponible(&self) -> bool {
            true
        }
        fn cible(&self) -> Option<&'static str> {
            Some(CIBLE)
        }
        fn lancer(&self, installeur: &Path) -> Resultat<()> {
            let contenu = std::fs::read(installeur).unwrap_or_default();
            self.lancements
                .lock()
                .unwrap()
                .push((installeur.to_path_buf(), contenu));
            if self.echoue {
                Err(Erreur::Echec("l'installeur a refusé de démarrer".into()))
            } else {
                Ok(())
            }
        }
    }

    impl FausseInstallation {
        fn nombre(&self) -> usize {
            self.lancements.lock().unwrap().len()
        }
    }

    struct Banc {
        serveur: ServeurFactice,
        dossier: tempfile::TempDir,
        installation: Arc<FausseInstallation>,
        redemarrages: Arc<AtomicUsize>,
        blocages: Arc<Mutex<Vec<(String, String)>>>,
        mise_a_jour: MiseAJour,
        source: SourceReseau,
    }

    fn manifeste(
        adresse: &str,
        version: &str,
        signature: &str,
        taille: usize,
    ) -> serde_json::Value {
        serde_json::json!({
            "version": version,
            "notes": "Pastille de mentions.",
            "pub_date": "2026-11-21T12:00:00.000Z",
            "mandatory": false,
            "force": false,
            "minVersion": "",
            "message": "",
            "platforms": { CIBLE: {
                "url": format!("{adresse}/Estompe-natif-beta-{version}-setup.exe"),
                "signature": signature,
                "taille": taille
            } }
        })
    }

    async fn banc(
        version: &str,
        signature: &str,
        taille: usize,
        installation: FausseInstallation,
        telechargeur: Option<Telechargeur>,
        contexte: Contexte,
    ) -> Banc {
        let serveur = ServeurFactice::demarrer(Vec::<Reponse>::new()).await;
        serveur.router(
            "GET",
            "/natif.json",
            Reponse::json(
                200,
                manifeste(serveur.adresse(), version, signature, taille),
            ),
        );
        let dossier = tempfile::tempdir().unwrap();
        let telechargeur = telechargeur
            .unwrap_or_else(|| Telechargeur::nouveau(politique_locale(), false).unwrap());
        let source = SourceReseau::nouvelle(
            telechargeur,
            &format!("{}/natif.json", serveur.adresse()),
            CLE_ESSAI,
            CIBLE,
            "0.1.0",
            dossier.path().join("mises-a-jour"),
        );
        let installation = Arc::new(installation);
        let redemarrages = Arc::new(AtomicUsize::new(0));
        let blocages = Arc::new(Mutex::new(Vec::new()));
        let (r, b) = (redemarrages.clone(), blocages.clone());
        let mise_a_jour = MiseAJour::demarrer(
            &tokio::runtime::Handle::current(),
            canal(),
            OptionsMiseAJour {
                source: Arc::new(source.clone()),
                installation: installation.clone(),
                contexte: Arc::new(move || Box::pin(async move { contexte })),
                redemarrer: Arc::new(move || {
                    r.fetch_add(1, Ordering::SeqCst);
                }),
                bloquer: Arc::new(move |message, version| {
                    b.lock()
                        .unwrap()
                        .push((message.to_string(), version.to_string()))
                }),
                journal: None,
            },
        );
        Banc {
            serveur,
            dossier,
            installation,
            redemarrages,
            blocages,
            mise_a_jour,
            source,
        }
    }

    fn servir_installeur(
        banc: &Banc,
        version: &str,
        contenu: Vec<u8>,
        coupure: Option<usize>,
    ) -> Arc<AtomicUsize> {
        let servis = Arc::new(AtomicUsize::new(0));
        let compteur = servis.clone();
        let premiere = AtomicUsize::new(0);
        banc.serveur.router_dynamique(
            "GET",
            &format!("/Estompe-natif-beta-{version}-setup.exe"),
            move |requete| {
                let debut = requete
                    .entete("range")
                    .and_then(|v| v.strip_prefix("bytes="))
                    .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok())
                    .unwrap_or(0);
                let corps = &contenu[debut..];
                let reponse = if debut > 0 {
                    Reponse::brute(206, "application/octet-stream", corps).avec_entete(
                        "content-range",
                        &format!("bytes {debut}-{}/{}", contenu.len() - 1, contenu.len()),
                    )
                } else {
                    Reponse::brute(200, "application/octet-stream", corps)
                };
                match coupure {
                    Some(c) if premiere.fetch_add(1, Ordering::SeqCst) == 0 => {
                        compteur.fetch_add(c, Ordering::SeqCst);
                        reponse.coupee_apres(c)
                    }
                    _ => {
                        compteur.fetch_add(corps.len(), Ordering::SeqCst);
                        reponse
                    }
                }
            },
        );
        servis
    }

    async fn attendre(
        mise_a_jour: &MiseAJour,
        quoi: &str,
        condition: impl Fn(&EtatMaj) -> bool,
    ) -> EtatMaj {
        let mut etat = mise_a_jour.observer();
        let atteint = tokio::time::timeout(DELAI, etat.wait_for(|e| condition(e)))
            .await
            .ok()
            .and_then(|r| r.ok().map(|e| e.clone()));
        atteint.unwrap_or_else(|| panic!("{quoi} : état {:?}", mise_a_jour.etat()))
    }

    fn fichiers(dossier: &Path) -> Vec<String> {
        std::fs::read_dir(dossier.join("mises-a-jour"))
            .map(|entrees| {
                entrees
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn proposer(banc: &Banc) {
        banc.mise_a_jour
            .envoyer(EvenementMaj::Verifier { manuelle: true });
        attendre(&banc.mise_a_jour, "mise à jour proposée", |e| {
            e.phase == Phase::Disponible
        })
        .await;
    }

    #[tokio::test]
    async fn c1_manifeste_superieur_telechargement_repris_verifie_lance_puis_redemarre() {
        let contenu = installeur();
        let total = contenu.len();
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            total,
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        let servis = servir_installeur(&b, "0.2.0", contenu.clone(), Some(total * 40 / 100));
        let progressions = Arc::new(Mutex::new(Vec::new()));
        let mut observation = b.mise_a_jour.observer();
        let journal = progressions.clone();
        tokio::spawn(async move {
            while observation.changed().await.is_ok() {
                let etat = observation.borrow().clone();
                if etat.en_cours() {
                    journal.lock().unwrap().push(etat.progression());
                }
            }
        });
        proposer(&b).await;
        assert_eq!(
            b.mise_a_jour.etat().disponible.unwrap().notes,
            "Pastille de mentions."
        );
        b.mise_a_jour.envoyer(EvenementMaj::Installer);
        tokio::time::timeout(DELAI, async {
            while b.redemarrages.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("redémarrage demandé");
        let lancements = b.installation.lancements.lock().unwrap().clone();
        assert_eq!(lancements.len(), 1);
        let (chemin, lance) = &lancements[0];
        assert_eq!(
            chemin.file_name().unwrap(),
            "Estompe-natif-beta-0.2.0-setup.exe"
        );
        assert_eq!(*lance, contenu);
        assert_eq!(
            fichiers(b.dossier.path()),
            ["Estompe-natif-beta-0.2.0-setup.exe"]
        );
        let requetes: Vec<_> = b
            .serveur
            .requetes()
            .into_iter()
            .filter(|r| r.chemin.ends_with(".exe"))
            .collect();
        assert_eq!(requetes.len(), 2, "une coupure puis une reprise");
        assert_eq!(
            requetes[1].entete("range"),
            Some(format!("bytes={}-", total * 40 / 100).as_str())
        );
        assert_eq!(
            servis.load(Ordering::SeqCst),
            total,
            "rien n'est téléchargé deux fois"
        );
        let progressions = progressions.lock().unwrap().clone();
        assert!(
            progressions.windows(2).all(|p| p[0] <= p[1]),
            "{progressions:?}"
        );
        assert_eq!(progressions.last().copied(), Some(1.0));
        assert!(progressions.iter().any(|p| *p > 0.0 && *p < 1.0));
    }

    async fn installer_et_attendre_l_erreur(b: &Banc) -> EtatMaj {
        proposer(b).await;
        b.mise_a_jour.envoyer(EvenementMaj::Installer);
        attendre(&b.mise_a_jour, "erreur affichée", |e| !e.erreur.is_empty()).await
    }

    #[tokio::test]
    async fn c2_signature_invalide_rien_n_est_execute_et_le_fichier_est_supprime() {
        let mut modifie = installeur();
        modifie[4242] ^= 0x80;
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            modifie.len(),
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", modifie, None);
        let etat = installer_et_attendre_l_erreur(&b).await;
        assert!(
            etat.erreur
                .contains("signature de l'installeur est invalide"),
            "{}",
            etat.erreur
        );
        assert_eq!(etat.phase, Phase::Disponible);
        assert_eq!(b.installation.nombre(), 0);
        assert_eq!(b.redemarrages.load(Ordering::SeqCst), 0);
        assert!(fichiers(b.dossier.path()).is_empty());
    }

    #[tokio::test]
    async fn c2_signature_d_une_autre_version_refusee() {
        let b = banc(
            "0.2.0",
            SIGNATURE_ANCIENNE,
            installeur().len(),
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", installeur(), None);
        let etat = installer_et_attendre_l_erreur(&b).await;
        assert!(
            etat.erreur.contains("estompe-natif 0.1.9"),
            "{}",
            etat.erreur
        );
        assert_eq!(b.installation.nombre(), 0);
        assert!(fichiers(b.dossier.path()).is_empty());
    }

    #[tokio::test]
    async fn c2_fichier_tronque_rien_n_est_execute() {
        let tronque = installeur()[..200_000].to_vec();
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            installeur().len(),
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", tronque, None);
        let etat = installer_et_attendre_l_erreur(&b).await;
        assert!(etat.erreur.contains("incomplet"), "{}", etat.erreur);
        assert_eq!(b.installation.nombre(), 0);
        assert!(fichiers(b.dossier.path()).is_empty());
    }

    #[tokio::test]
    async fn c2_manifeste_d_une_version_inferieure_ou_egale_aucun_telechargement() {
        for version in ["0.1.0", "0.0.9"] {
            let b = banc(
                version,
                SIGNATURE_BONNE,
                10,
                FausseInstallation::default(),
                None,
                Contexte::default(),
            )
            .await;
            b.mise_a_jour
                .envoyer(EvenementMaj::Verifier { manuelle: true });
            let etat = attendre(&b.mise_a_jour, "à jour", |e| e.phase == Phase::AJour).await;
            assert!(etat.disponible.is_none());
            b.mise_a_jour.envoyer(EvenementMaj::Installer);
            let etat = attendre(&b.mise_a_jour, "rien à installer", |e| {
                !e.erreur.is_empty()
            })
            .await;
            assert_eq!(
                etat.erreur,
                "Aucune mise à jour à installer pour l'instant."
            );
            assert!(b
                .serveur
                .lignes()
                .iter()
                .all(|(chemin, _)| chemin == "/natif.json"));
            assert_eq!(b.installation.nombre(), 0);
        }
    }

    struct DisquePlein;

    impl std::io::Write for DisquePlein {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn c2_disque_plein_erreur_lisible_et_aucun_fichier_restant() {
        let telechargeur = Telechargeur::nouveau(politique_locale(), false)
            .unwrap()
            .avec_ecrivain(Arc::new(|chemin, _| {
                std::fs::create_dir_all(chemin.parent().unwrap())?;
                std::fs::write(chemin, b"")?;
                Ok(Box::new(DisquePlein) as Box<dyn std::io::Write + Send>)
            }));
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            installeur().len(),
            FausseInstallation::default(),
            Some(telechargeur),
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", installeur(), None);
        let etat = installer_et_attendre_l_erreur(&b).await;
        assert_eq!(
            etat.erreur,
            "Écriture de la mise à jour impossible : espace disque insuffisant."
        );
        assert_eq!(b.installation.nombre(), 0);
        assert!(fichiers(b.dossier.path()).is_empty());
    }

    #[tokio::test]
    async fn c2_un_installeur_qui_echoue_laisse_l_app_en_place() {
        let installation = FausseInstallation {
            echoue: true,
            ..FausseInstallation::default()
        };
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            installeur().len(),
            installation,
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", installeur(), None);
        let etat = installer_et_attendre_l_erreur(&b).await;
        assert!(
            etat.erreur.contains("l'installeur a refusé de démarrer"),
            "{}",
            etat.erreur
        );
        assert!(etat.erreur.contains("continue de fonctionner"));
        assert_eq!(etat.phase, Phase::Disponible);
        assert_eq!(b.installation.nombre(), 1);
        assert_eq!(b.redemarrages.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn l_installeur_est_reverifie_juste_avant_le_lancement() {
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            installeur().len(),
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", installeur(), None);
        proposer(&b).await;
        let dispo = b.mise_a_jour.etat().disponible.unwrap();
        let chemin = b
            .source
            .telecharger(dispo.clone(), Arc::new(|_, _| {}))
            .await
            .unwrap();
        assert_eq!(b.source.controler(&dispo, &chemin), Ok(()));
        let mut remplace = installeur();
        remplace[0] ^= 1;
        std::fs::write(&chemin, remplace).unwrap();
        assert!(matches!(
            b.source.controler(&dispo, &chemin),
            Err(ErreurMiseAJour::Signature(_))
        ));
        assert!(!chemin.exists());
    }

    #[tokio::test]
    async fn un_installeur_deja_telecharge_et_valide_n_est_pas_retelecharge() {
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            installeur().len(),
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        servir_installeur(&b, "0.2.0", installeur(), None);
        proposer(&b).await;
        let dispo = b.mise_a_jour.etat().disponible.unwrap();
        b.source
            .telecharger(dispo.clone(), Arc::new(|_, _| {}))
            .await
            .unwrap();
        b.source
            .telecharger(dispo, Arc::new(|_, _| {}))
            .await
            .unwrap();
        let telechargements = b
            .serveur
            .lignes()
            .iter()
            .filter(|(chemin, _)| chemin.ends_with(".exe"))
            .count();
        assert_eq!(telechargements, 1);
    }

    #[tokio::test]
    async fn une_mise_a_jour_imposee_bloque_par_l_entree() {
        let b = banc(
            "0.2.0",
            SIGNATURE_BONNE,
            10,
            FausseInstallation::default(),
            None,
            Contexte::default(),
        )
        .await;
        b.serveur.router(
            "GET",
            "/natif.json",
            Reponse::json(
                200,
                serde_json::json!({
                    "version": "0.2.0",
                    "mandatory": true,
                    "message": "Faille corrigée",
                    "platforms": { CIBLE: { "url": "x", "signature": "y" } }
                }),
            ),
        );
        b.mise_a_jour
            .envoyer(EvenementMaj::Verifier { manuelle: false });
        attendre(&b.mise_a_jour, "bloquée", |e| e.bloquee).await;
        assert_eq!(
            *b.blocages.lock().unwrap(),
            [("Faille corrigée".to_string(), "0.2.0".to_string())]
        );
    }

    struct SourceComptee(AtomicUsize);

    impl SourceMaj for SourceComptee {
        fn verifier(&self) -> Futur<Result<Option<MiseAJourDisponible>, ErreurMiseAJour>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        }
        fn telecharger(
            &self,
            _: MiseAJourDisponible,
            _: Progres,
        ) -> Futur<Result<PathBuf, ErreurMiseAJour>> {
            Box::pin(async { Err(ErreurMiseAJour::AucuneMiseAJour) })
        }
        fn controler(&self, _: &MiseAJourDisponible, _: &Path) -> Result<(), ErreurMiseAJour> {
            Ok(())
        }
    }

    async fn verifications_apres(mise_a_jour: &MiseAJour, duree: Duration) -> u64 {
        tokio::time::sleep(duree).await;
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        mise_a_jour.etat().verifications
    }

    #[tokio::test(start_paused = true)]
    async fn verifie_a_quinze_secondes_puis_toutes_les_trente_minutes_et_au_premier_plan() {
        let source = Arc::new(SourceComptee(AtomicUsize::new(0)));
        let mise_a_jour = MiseAJour::demarrer(
            &tokio::runtime::Handle::current(),
            canal(),
            OptionsMiseAJour {
                source: source.clone(),
                installation: Arc::new(FausseInstallation::default()),
                contexte: Arc::new(|| Box::pin(async { Contexte::default() })),
                redemarrer: Arc::new(|| {}),
                bloquer: Arc::new(|_, _| {}),
                journal: None,
            },
        );
        mise_a_jour.envoyer(EvenementMaj::Demarrer);
        mise_a_jour.envoyer(EvenementMaj::Demarrer);
        assert_eq!(
            verifications_apres(&mise_a_jour, Duration::from_secs(14)).await,
            0
        );
        assert_eq!(
            verifications_apres(&mise_a_jour, Duration::from_secs(1)).await,
            1
        );
        mise_a_jour.envoyer(EvenementMaj::PremierPlan);
        assert_eq!(
            verifications_apres(&mise_a_jour, Duration::from_secs(60)).await,
            1,
            "retour au premier plan juste après une vérification : ignoré"
        );
        assert_eq!(
            verifications_apres(
                &mise_a_jour,
                VERIFICATION_PERIODIQUE - Duration::from_secs(61)
            )
            .await,
            1
        );
        assert_eq!(
            verifications_apres(&mise_a_jour, Duration::from_secs(1)).await,
            2
        );
        assert_eq!(
            verifications_apres(&mise_a_jour, INTERVALLE_PREMIER_PLAN).await,
            2
        );
        mise_a_jour.envoyer(EvenementMaj::PremierPlan);
        assert_eq!(
            verifications_apres(&mise_a_jour, Duration::from_millis(1)).await,
            3
        );
        assert_eq!(
            verifications_apres(&mise_a_jour, VERIFICATION_PERIODIQUE * 2).await,
            5,
            "une seule planification malgré deux démarrages"
        );
        assert_eq!(source.0.load(Ordering::SeqCst), 5);
    }
}
