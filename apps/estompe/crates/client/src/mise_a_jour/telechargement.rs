use std::error::Error as _;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;

use super::manifeste::Manifeste;
use super::politique::PolitiqueAdresses;
use super::{ErreurMiseAJour, TAILLE_MAX, TAILLE_MAX_MANIFESTE};

const DELAI_CONNEXION: Duration = Duration::from_secs(15);
const DELAI_LECTURE: Duration = Duration::from_secs(30);
const DELAI_MANIFESTE: Duration = Duration::from_secs(30);

pub type EcrivainDisque =
    Arc<dyn Fn(&Path, bool) -> io::Result<Box<dyn Write + Send>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Telecharge {
    pub octets: u64,
    pub repris_a: u64,
}

#[derive(Clone)]
pub struct Telechargeur {
    http: reqwest::Client,
    politique: Arc<PolitiqueAdresses>,
    ecrivain: EcrivainDisque,
}

fn ecrivain_fichier() -> EcrivainDisque {
    Arc::new(|chemin: &Path, ajout: bool| {
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let fichier = OpenOptions::new()
            .create(true)
            .write(true)
            .append(ajout)
            .truncate(!ajout)
            .open(chemin)?;
        Ok(Box::new(fichier) as Box<dyn Write + Send>)
    })
}

fn erreur_disque(erreur: &io::Error) -> ErreurMiseAJour {
    ErreurMiseAJour::Disque(match erreur.kind() {
        io::ErrorKind::StorageFull => "espace disque insuffisant".to_string(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            "droits insuffisants sur le dossier des mises à jour".to_string()
        }
        _ => erreur.to_string(),
    })
}

fn message_de_transport(erreur: &reqwest::Error) -> ErreurMiseAJour {
    let mut source: Option<&dyn std::error::Error> = erreur.source();
    let mut dernier = erreur.to_string();
    while let Some(s) = source {
        dernier = s.to_string();
        source = s.source();
    }
    if erreur.is_redirect() {
        ErreurMiseAJour::Adresse(dernier)
    } else if erreur.is_timeout() {
        ErreurMiseAJour::Reseau("délai dépassé".into())
    } else {
        ErreurMiseAJour::Reseau(dernier)
    }
}

fn debut_de_la_plage(reponse: &reqwest::Response) -> Option<u64> {
    let valeur = reponse.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let plage = valeur.strip_prefix("bytes ")?;
    plage.split('-').next()?.trim().parse().ok()
}

fn retirer(chemin: &Path) {
    let _ = std::fs::remove_file(chemin);
}

impl Telechargeur {
    pub fn nouveau(
        politique: PolitiqueAdresses,
        mandataire_systeme: bool,
    ) -> Result<Self, ErreurMiseAJour> {
        let politique = Arc::new(politique);
        let mut constructeur = reqwest::Client::builder()
            .redirect(politique.redirections())
            .connect_timeout(DELAI_CONNEXION)
            .read_timeout(DELAI_LECTURE);
        if !mandataire_systeme {
            constructeur = constructeur.no_proxy();
        }
        let http = constructeur
            .build()
            .map_err(|e| ErreurMiseAJour::Reseau(e.to_string()))?;
        Ok(Self {
            http,
            politique,
            ecrivain: ecrivain_fichier(),
        })
    }

    pub fn avec_ecrivain(mut self, ecrivain: EcrivainDisque) -> Self {
        self.ecrivain = ecrivain;
        self
    }

    pub fn politique(&self) -> &PolitiqueAdresses {
        &self.politique
    }

    async fn obtenir(
        &self,
        adresse: &str,
        depuis: u64,
    ) -> Result<reqwest::Response, ErreurMiseAJour> {
        let url = self
            .politique
            .accepter(adresse)
            .map_err(|refus| ErreurMiseAJour::Adresse(refus.to_string()))?;
        let mut requete = self.http.get(url);
        if depuis > 0 {
            requete = requete.header(RANGE, format!("bytes={depuis}-"));
        }
        requete.send().await.map_err(|e| message_de_transport(&e))
    }

    pub async fn manifeste(&self, adresse: &str) -> Result<Manifeste, ErreurMiseAJour> {
        let url = self
            .politique
            .accepter(adresse)
            .map_err(|refus| ErreurMiseAJour::Adresse(refus.to_string()))?;
        let mut reponse = self
            .http
            .get(url)
            .timeout(DELAI_MANIFESTE)
            .send()
            .await
            .map_err(|e| message_de_transport(&e))?;
        if !reponse.status().is_success() {
            return Err(ErreurMiseAJour::Serveur(reponse.status().as_u16()));
        }
        let mut octets = Vec::new();
        while let Some(morceau) = reponse
            .chunk()
            .await
            .map_err(|e| message_de_transport(&e))?
        {
            octets.extend_from_slice(&morceau);
            if octets.len() as u64 > TAILLE_MAX_MANIFESTE {
                return Err(ErreurMiseAJour::Manifeste("réponse trop grande".into()));
            }
        }
        Manifeste::lire(&String::from_utf8_lossy(&octets))
    }

    pub async fn telecharger(
        &self,
        adresse: &str,
        partiel: &Path,
        taille: Option<u64>,
        progres: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> Result<Telecharge, ErreurMiseAJour> {
        let borne = taille.unwrap_or(TAILLE_MAX);
        if borne > TAILLE_MAX {
            retirer(partiel);
            return Err(ErreurMiseAJour::Taille);
        }
        let mut deja = std::fs::metadata(partiel).map(|m| m.len()).unwrap_or(0);
        if deja > borne {
            retirer(partiel);
            deja = 0;
        }
        let mut reponse = self.obtenir(adresse, deja).await?;
        let mut repris_a = 0;
        match reponse.status() {
            StatusCode::PARTIAL_CONTENT
                if deja > 0 && debut_de_la_plage(&reponse) == Some(deja) =>
            {
                repris_a = deja;
            }
            StatusCode::RANGE_NOT_SATISFIABLE if deja > 0 => {
                retirer(partiel);
                deja = 0;
                reponse = self.obtenir(adresse, 0).await?;
            }
            _ => deja = 0,
        }
        if !reponse.status().is_success() {
            return Err(ErreurMiseAJour::Serveur(reponse.status().as_u16()));
        }
        let total = reponse.content_length().map(|reste| reste + deja);
        if total.is_some_and(|t| t > borne) {
            retirer(partiel);
            return Err(ErreurMiseAJour::Taille);
        }
        let mut ecriture = match (self.ecrivain)(partiel, deja > 0) {
            Ok(ecriture) => ecriture,
            Err(e) => {
                retirer(partiel);
                return Err(erreur_disque(&e));
            }
        };
        let total_affiche = total.or(taille).unwrap_or(0);
        let mut fait = deja;
        progres(fait, total_affiche);
        loop {
            match reponse.chunk().await {
                Ok(Some(morceau)) => {
                    fait += morceau.len() as u64;
                    if fait > borne {
                        drop(ecriture);
                        retirer(partiel);
                        return Err(ErreurMiseAJour::Taille);
                    }
                    if let Err(e) = ecriture.write_all(&morceau) {
                        drop(ecriture);
                        retirer(partiel);
                        return Err(erreur_disque(&e));
                    }
                    progres(fait, total_affiche);
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = ecriture.flush();
                    return Err(message_de_transport(&e));
                }
            }
        }
        if let Err(e) = ecriture.flush() {
            drop(ecriture);
            retirer(partiel);
            return Err(erreur_disque(&e));
        }
        Ok(Telecharge {
            octets: fait,
            repris_a,
        })
    }
}

pub fn deplacer(partiel: &Path, destination: &Path) -> Result<PathBuf, ErreurMiseAJour> {
    std::fs::rename(partiel, destination).map_err(|e| {
        retirer(partiel);
        erreur_disque(&e)
    })?;
    Ok(destination.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mise_a_jour::essais::{installeur, politique_locale};
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn telechargeur() -> Telechargeur {
        Telechargeur::nouveau(politique_locale(), false).unwrap()
    }

    fn sans_progres() -> impl Fn(u64, u64) + Send + Sync {
        |_, _| {}
    }

    fn servir_avec_plages(
        serveur: &ServeurFactice,
        chemin: &str,
        contenu: Vec<u8>,
        coupure: Option<usize>,
    ) -> Arc<AtomicUsize> {
        let servis = Arc::new(AtomicUsize::new(0));
        let compteur = servis.clone();
        let premier = Arc::new(AtomicUsize::new(0));
        serveur.router_dynamique("GET", chemin, move |requete| {
            let debut = requete
                .entete("range")
                .and_then(|v| v.strip_prefix("bytes="))
                .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok())
                .unwrap_or(0);
            let corps = &contenu[debut..];
            let mut reponse = if debut > 0 {
                Reponse::brute(206, "application/octet-stream", corps).avec_entete(
                    "content-range",
                    &format!("bytes {debut}-{}/{}", contenu.len() - 1, contenu.len()),
                )
            } else {
                Reponse::brute(200, "application/octet-stream", corps)
            };
            let envoyes = match coupure {
                Some(c) if premier.fetch_add(1, Ordering::SeqCst) == 0 => {
                    reponse = reponse.coupee_apres(c);
                    c
                }
                _ => corps.len(),
            };
            compteur.fetch_add(envoyes, Ordering::SeqCst);
            reponse
        });
        servis
    }

    #[tokio::test]
    async fn telecharge_avec_une_progression_croissante_jusqu_au_total() {
        let serveur = ServeurFactice::demarrer(Vec::<Reponse>::new()).await;
        servir_avec_plages(&serveur, "/i.exe", installeur(), None);
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        let etapes = Arc::new(Mutex::new(Vec::new()));
        let journal = etapes.clone();
        let resultat = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                Some(installeur().len() as u64),
                &move |fait, total| journal.lock().unwrap().push((fait, total)),
            )
            .await
            .unwrap();
        assert_eq!(resultat.octets, installeur().len() as u64);
        assert_eq!(std::fs::read(&partiel).unwrap(), installeur());
        let etapes = etapes.lock().unwrap();
        assert!(etapes.windows(2).all(|p| p[0].0 <= p[1].0));
        assert_eq!(etapes.first(), Some(&(0, installeur().len() as u64)));
        assert_eq!(
            etapes.last(),
            Some(&(installeur().len() as u64, installeur().len() as u64))
        );
    }

    #[tokio::test]
    async fn une_coupure_garde_le_partiel_et_la_reprise_ne_demande_que_la_suite() {
        let serveur = ServeurFactice::demarrer(Vec::<Reponse>::new()).await;
        let total = installeur().len();
        let coupure = total * 40 / 100;
        let servis = servir_avec_plages(&serveur, "/i.exe", installeur(), Some(coupure));
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        let adresse = format!("{}/i.exe", serveur.adresse());
        let t = telechargeur();
        let premiere = t
            .telecharger(&adresse, &partiel, Some(total as u64), &sans_progres())
            .await;
        assert!(
            matches!(premiere, Err(ErreurMiseAJour::Reseau(_))),
            "{premiere:?}"
        );
        assert_eq!(std::fs::metadata(&partiel).unwrap().len(), coupure as u64);
        let seconde = t
            .telecharger(&adresse, &partiel, Some(total as u64), &sans_progres())
            .await
            .unwrap();
        assert_eq!(seconde.repris_a, coupure as u64);
        assert_eq!(std::fs::read(&partiel).unwrap(), installeur());
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].entete("range"), None);
        assert_eq!(
            requetes[1].entete("range"),
            Some(format!("bytes={coupure}-").as_str())
        );
        assert_eq!(
            servis.load(Ordering::SeqCst),
            total,
            "aucun octet servi deux fois"
        );
    }

    #[tokio::test]
    async fn un_serveur_sans_plages_fait_repartir_de_zero() {
        let serveur = ServeurFactice::demarrer([Reponse::brute(
            200,
            "application/octet-stream",
            &installeur(),
        )])
        .await;
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        std::fs::write(&partiel, vec![0xEE; 5000]).unwrap();
        let resultat = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                None,
                &sans_progres(),
            )
            .await
            .unwrap();
        assert_eq!(resultat.repris_a, 0);
        assert_eq!(std::fs::read(&partiel).unwrap(), installeur());
    }

    #[tokio::test]
    async fn un_416_efface_le_partiel_et_recommence() {
        let serveur = ServeurFactice::demarrer([
            Reponse::vide(416),
            Reponse::brute(200, "application/octet-stream", b"abc"),
        ])
        .await;
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        std::fs::write(&partiel, b"zzzz").unwrap();
        telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                None,
                &sans_progres(),
            )
            .await
            .unwrap();
        assert_eq!(std::fs::read(&partiel).unwrap(), b"abc");
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].entete("range"), Some("bytes=4-"));
        assert_eq!(requetes[1].entete("range"), None);
    }

    #[tokio::test]
    async fn refuse_au_dela_de_la_taille_annoncee() {
        let serveur = ServeurFactice::demarrer([Reponse::brute(
            200,
            "application/octet-stream",
            &installeur(),
        )])
        .await;
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        let resultat = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                Some(1000),
                &sans_progres(),
            )
            .await;
        assert_eq!(resultat, Err(ErreurMiseAJour::Taille));
        assert!(!partiel.exists());
        let trop = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                Some(TAILLE_MAX + 1),
                &sans_progres(),
            )
            .await;
        assert_eq!(trop, Err(ErreurMiseAJour::Taille));
        assert_eq!(serveur.requetes().len(), 1);
    }

    struct DisquePlein {
        restant: usize,
    }

    impl Write for DisquePlein {
        fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
            if self.restant < octets.len() {
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            self.restant -= octets.len();
            Ok(octets.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn un_disque_plein_supprime_le_partiel_avec_une_erreur_lisible() {
        let serveur = ServeurFactice::demarrer([Reponse::brute(
            200,
            "application/octet-stream",
            &installeur(),
        )])
        .await;
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        let cree = partiel.clone();
        let t = telechargeur().avec_ecrivain(Arc::new(move |chemin, _| {
            std::fs::write(chemin, b"")?;
            assert_eq!(chemin, cree);
            Ok(Box::new(DisquePlein { restant: 10_000 }) as Box<dyn Write + Send>)
        }));
        let resultat = t
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                None,
                &sans_progres(),
            )
            .await;
        let erreur = resultat.unwrap_err();
        assert_eq!(
            erreur,
            ErreurMiseAJour::Disque("espace disque insuffisant".into())
        );
        assert!(erreur.to_string().contains("espace disque insuffisant"));
        assert!(!partiel.exists());
    }

    #[tokio::test]
    async fn un_dossier_impossible_a_creer_est_une_erreur_de_disque() {
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "application/octet-stream", b"abc")])
                .await;
        let dossier = tempfile::tempdir().unwrap();
        let fichier = dossier.path().join("fichier");
        std::fs::write(&fichier, b"").unwrap();
        let resultat = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &fichier.join("sous").join("i.exe.partiel"),
                None,
                &sans_progres(),
            )
            .await;
        assert!(
            matches!(resultat, Err(ErreurMiseAJour::Disque(_))),
            "{resultat:?}"
        );
    }

    #[tokio::test]
    async fn une_redirection_hors_des_hotes_autorises_est_refusee() {
        let serveur = ServeurFactice::demarrer(Vec::<Reponse>::new()).await;
        let port = serveur.adresse().rsplit(':').next().unwrap().to_string();
        serveur.router(
            "GET",
            "/i.exe",
            Reponse::vide(302)
                .avec_entete("location", &format!("http://localhost:{port}/autre.exe")),
        );
        serveur.router(
            "GET",
            "/ici.exe",
            Reponse::vide(302).avec_entete("location", "/autre.exe"),
        );
        serveur.router(
            "GET",
            "/autre.exe",
            Reponse::brute(200, "application/octet-stream", b"abc"),
        );
        let dossier = tempfile::tempdir().unwrap();
        let partiel = dossier.path().join("i.exe.partiel");
        let refuse = telechargeur()
            .telecharger(
                &format!("{}/i.exe", serveur.adresse()),
                &partiel,
                None,
                &sans_progres(),
            )
            .await;
        match refuse {
            Err(ErreurMiseAJour::Adresse(raison)) => {
                assert!(raison.contains("localhost"), "{raison}")
            }
            autre => panic!("redirection acceptée : {autre:?}"),
        }
        assert!(!serveur.lignes().iter().any(|(c, _)| c == "/autre.exe"));
        let suivie = telechargeur()
            .telecharger(
                &format!("{}/ici.exe", serveur.adresse()),
                &partiel,
                None,
                &sans_progres(),
            )
            .await
            .unwrap();
        assert_eq!(suivie.octets, 3);
    }

    #[tokio::test]
    async fn une_adresse_refusee_n_emet_aucune_requete() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(200)]).await;
        let dossier = tempfile::tempdir().unwrap();
        let production = Telechargeur::nouveau(PolitiqueAdresses::production(), false).unwrap();
        let adresse = format!("{}/i.exe", serveur.adresse());
        assert!(matches!(
            production
                .telecharger(&adresse, &dossier.path().join("p"), None, &sans_progres())
                .await,
            Err(ErreurMiseAJour::Adresse(_))
        ));
        assert!(matches!(
            production.manifeste(&adresse).await,
            Err(ErreurMiseAJour::Adresse(_))
        ));
        assert!(serveur.requetes().is_empty());
    }

    #[tokio::test]
    async fn lit_le_manifeste_et_signale_les_erreurs_du_serveur() {
        let serveur = ServeurFactice::demarrer(Vec::<Reponse>::new()).await;
        serveur.router(
            "GET",
            "/natif.json",
            Reponse::json(
                200,
                serde_json::json!({"version": "0.2.0", "platforms": {}}),
            ),
        );
        serveur.router("GET", "/absent.json", Reponse::vide(404));
        serveur.router(
            "GET",
            "/gros.json",
            Reponse::brute(
                200,
                "application/json",
                &vec![b' '; TAILLE_MAX_MANIFESTE as usize + 10],
            ),
        );
        let t = telechargeur();
        let m = t
            .manifeste(&format!("{}/natif.json", serveur.adresse()))
            .await
            .unwrap();
        assert_eq!(m.publication.version, "0.2.0");
        assert_eq!(
            t.manifeste(&format!("{}/absent.json", serveur.adresse()))
                .await,
            Err(ErreurMiseAJour::Serveur(404))
        );
        assert!(matches!(
            t.manifeste(&format!("{}/gros.json", serveur.adresse()))
                .await,
            Err(ErreurMiseAJour::Manifeste(_))
        ));
    }
}
