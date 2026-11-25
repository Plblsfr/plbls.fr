use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use crate::http::Client;
use crate::lecteur::EtatLecteur;
use crate::temps_reel::evenements::Seance;

use super::avance::{budget, Avance, OptionsAvance, TelechargeurHttp};
use super::depot::{ouvrir_depot, partager, purger_restes, DepotPartage, RESTES_MS};
use super::film::{tampon_effectif, DemandeFilm, PrefTampon, PreferencesFilm, Reseau};
use super::pilote::{ActionPilote, AppelSeance, PiloteSeance, TICK_MS};
use super::relais::Relais;
use super::seance::{GenreSeance, SeanceEnCours};

pub type HorlogeSeance = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub enum SignalSeance {
    Change,
    Refus(String),
    Info(String),
    Journal(String),
    Etat(Box<Seance>),
}

pub struct OutilsSeance {
    pub client: Client,
    pub entracte: Option<Client>,
    pub relais: Option<Arc<Relais>>,
    pub racine_depot: Option<PathBuf>,
    pub horloge: HorlogeSeance,
    pub preferences: Arc<dyn Fn(PreferencesFilm) + Send + Sync>,
    pub signal: Arc<dyn Fn(SignalSeance) + Send + Sync>,
    pub suivi: Option<watch::Receiver<EtatLecteur>>,
    pub position: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub tampon: PrefTampon,
}

struct Contexte {
    salon: String,
    genre: GenreSeance,
    pilote: Arc<Mutex<PiloteSeance>>,
    reveil: Arc<Notify>,
    client: Client,
    entracte: Option<Client>,
    relais: Option<Arc<Relais>>,
    racine: Option<PathBuf>,
    horloge: HorlogeSeance,
    preferences: Arc<dyn Fn(PreferencesFilm) + Send + Sync>,
    signal: Arc<dyn Fn(SignalSeance) + Send + Sync>,
    position: Arc<dyn Fn() -> f64 + Send + Sync>,
    tampon: Arc<Mutex<PrefTampon>>,
    depot: Mutex<Option<DepotPartage>>,
    runtime: Handle,
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Contexte {
    fn maintenant(&self) -> i64 {
        (self.horloge)()
    }

    fn traiter(self: &Arc<Self>, actions: Vec<ActionPilote>) {
        for action in actions {
            match action {
                ActionPilote::Appel(appel) => self.appeler(appel),
                ActionPilote::Film(demande) => self.film(demande),
                ActionPilote::Refus(m) => (self.signal)(SignalSeance::Refus(m)),
                ActionPilote::Info(m) => (self.signal)(SignalSeance::Info(m)),
                ActionPilote::Journal(m) => (self.signal)(SignalSeance::Journal(m)),
            }
        }
    }

    fn appeler(self: &Arc<Self>, appel: AppelSeance) {
        let c = self.clone();
        self.runtime.spawn(async move {
            let resultat = c
                .client
                .executer_appel_seance(&c.salon, c.genre, &appel)
                .await;
            let reponse = match resultat {
                Ok(Some(vue)) => {
                    (c.signal)(SignalSeance::Etat(Box::new(vue)));
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(e) => Err(e.message().to_string()),
            };
            let maintenant = c.maintenant();
            verrou(&c.pilote).reponse_appel(&appel, reponse, maintenant);
            c.reveil.notify_one();
        });
    }

    fn fermer_depot(&self) {
        if let Some(r) = &self.relais {
            r.definir_avance(None);
        }
        if let Some(d) = verrou(&self.depot).take() {
            verrou(&d).fermer(true);
        }
    }

    fn ouvrir_depot(&self, cle: Option<String>) {
        let (Some(relais), Some(entracte)) = (&self.relais, &self.entracte) else {
            return;
        };
        self.fermer_depot();
        let depot = partager(ouvrir_depot(
            cle.as_deref(),
            self.racine.as_deref(),
            self.maintenant(),
        ));
        *verrou(&self.depot) = Some(depot.clone());
        let tampon = self.tampon.clone();
        let position = self.position.clone();
        let _contexte = self.runtime.enter();
        relais.definir_avance(Some(Avance::nouvelle(
            move || budget(*verrou(&tampon)),
            move || position(),
            Arc::new(TelechargeurHttp::nouveau(entracte.clone())),
            OptionsAvance {
                depot: Some(depot),
                parallele: None,
            },
        )));
    }

    fn film(self: &Arc<Self>, demande: DemandeFilm) {
        match demande {
            DemandeFilm::Depot(cle) => self.ouvrir_depot(cle),
            DemandeFilm::FermerDepot => self.fermer_depot(),
            DemandeFilm::Reveiller => {
                if let Some(a) = self.relais.as_ref().and_then(|r| r.avance()) {
                    let _contexte = self.runtime.enter();
                    a.reveiller();
                }
            }
            DemandeFilm::Preferences(p) => {
                *verrou(&self.tampon) = tampon_effectif(&p, Reseau::Inconnu);
                (self.preferences)(p);
            }
            autre => {
                let Some(entracte) = self.entracte.clone() else {
                    return;
                };
                let c = self.clone();
                self.runtime.spawn(async move {
                    if let Some(reponse) = entracte.executer_demande_film(autre).await {
                        let maintenant = c.maintenant();
                        verrou(&c.pilote).reponse_film(reponse, maintenant);
                        c.reveil.notify_one();
                    }
                });
            }
        }
    }
}

pub struct SeanceActive {
    contexte: Arc<Contexte>,
    tache: JoinHandle<()>,
}

async fn boucle(c: Arc<Contexte>, mut suivi: Option<watch::Receiver<EtatLecteur>>) {
    loop {
        let actions = verrou(&c.pilote).actions();
        c.traiter(actions);
        (c.signal)(SignalSeance::Change);
        let maintenant = c.maintenant();
        let attente = verrou(&c.pilote)
            .prochaine_echeance()
            .map_or(TICK_MS, |e| e - maintenant)
            .clamp(0, TICK_MS);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(attente as u64)) => {
                let maintenant = c.maintenant();
                verrou(&c.pilote).tick(maintenant);
            }
            _ = c.reveil.notified() => {}
            change = async { suivi.as_mut().expect("suivi du lecteur").changed().await }, if suivi.is_some() => {
                match change {
                    Ok(()) => {
                        let maintenant = c.maintenant();
                        verrou(&c.pilote).lecteur_change(maintenant);
                    }
                    Err(_) => suivi = None,
                }
            }
        }
    }
}

impl SeanceActive {
    pub fn demarrer(
        runtime: &Handle,
        salon: &str,
        genre: GenreSeance,
        pilote: PiloteSeance,
        outils: OutilsSeance,
    ) -> Self {
        purger_restes(
            outils.racine_depot.as_deref(),
            None,
            (outils.horloge)(),
            RESTES_MS,
        );
        let contexte = Arc::new(Contexte {
            salon: salon.to_string(),
            genre,
            pilote: Arc::new(Mutex::new(pilote)),
            reveil: Arc::new(Notify::new()),
            client: outils.client,
            entracte: outils.entracte,
            relais: outils.relais,
            racine: outils.racine_depot,
            horloge: outils.horloge,
            preferences: outils.preferences,
            signal: outils.signal,
            position: outils.position,
            tampon: Arc::new(Mutex::new(outils.tampon)),
            depot: Mutex::new(None),
            runtime: runtime.clone(),
        });
        let tache = runtime.spawn(boucle(contexte.clone(), outils.suivi));
        Self { contexte, tache }
    }

    pub fn salon(&self) -> &str {
        &self.contexte.salon
    }

    pub fn genre(&self) -> GenreSeance {
        self.contexte.genre
    }

    pub fn avec<R>(&self, action: impl FnOnce(&mut PiloteSeance, i64) -> R) -> R {
        let maintenant = self.contexte.maintenant();
        let r = action(&mut verrou(&self.contexte.pilote), maintenant);
        self.contexte.reveil.notify_one();
        r
    }

    pub fn lire<R>(&self, lecture: impl FnOnce(&PiloteSeance, i64) -> R) -> R {
        let maintenant = self.contexte.maintenant();
        lecture(&verrou(&self.contexte.pilote), maintenant)
    }

    pub fn recevoir(&self, seance: Option<&SeanceEnCours>) {
        self.avec(|p, t| p.recevoir(seance, t));
    }

    pub fn reconnecte(&self) {
        self.avec(|p, _| p.reconnecte());
    }

    pub fn arreter(self) {
        self.quitter(false);
    }

    pub fn ceder(self) {
        self.quitter(true);
    }

    fn quitter(self, ceder: bool) {
        self.tache.abort();
        let actions = {
            let mut p = verrou(&self.contexte.pilote);
            if ceder {
                p.ceder();
            } else {
                p.detruire();
            }
            p.actions()
        };
        self.contexte.traiter(
            actions
                .into_iter()
                .filter(|a| matches!(a, ActionPilote::Film(_)))
                .collect(),
        );
        self.contexte.fermer_depot();
    }
}

impl Drop for SeanceActive {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::format::iso_de_ms;
    use crate::lecteur::{Lecteur, MediaFactice, MoteurFactice};
    use crate::regarder_ensemble::depot::{dossier_de, RACINE};
    use crate::regarder_ensemble::film::LecteurFilm;
    use crate::regarder_ensemble::fournisseurs::{LecteurFichier, LecteurSeance};
    use crate::regarder_ensemble::pilote::FabriqueLecteur;
    use crate::regarder_ensemble::synchro::Commande;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::temps_reel::evenements::EtatSeance;

    const VIDEO: &str = "https://x.test/v.mp4";

    type Signaux = Arc<std::sync::Mutex<Vec<SignalSeance>>>;

    fn maintenant() -> i64 {
        crate::mesures::maintenant_ms() as i64
    }

    fn outils(
        client: Client,
        entracte: Option<Client>,
        relais: Option<Arc<Relais>>,
        racine: Option<PathBuf>,
        suivi: Option<watch::Receiver<EtatLecteur>>,
    ) -> (
        OutilsSeance,
        Signaux,
        Arc<std::sync::Mutex<Vec<PreferencesFilm>>>,
    ) {
        let signaux: Signaux = Arc::default();
        let prefs: Arc<std::sync::Mutex<Vec<PreferencesFilm>>> = Arc::default();
        let (s, p) = (signaux.clone(), prefs.clone());
        (
            OutilsSeance {
                client,
                entracte,
                relais,
                racine_depot: racine,
                horloge: Arc::new(maintenant),
                preferences: Arc::new(move |x| p.lock().unwrap().push(x)),
                signal: Arc::new(move |x| {
                    if x != SignalSeance::Change {
                        s.lock().unwrap().push(x)
                    }
                }),
                suivi,
                position: Arc::new(|| 0.0),
                tampon: PrefTampon::Normal,
            },
            signaux,
            prefs,
        )
    }

    async fn attendre(mut condition: impl FnMut() -> bool, quoi: &str) {
        for _ in 0..500 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition non atteinte : {quoi}");
    }

    fn etat(provider: &str, video: &str, playing: bool) -> EtatSeance {
        EtatSeance {
            rev: 2,
            provider: provider.into(),
            video: video.into(),
            playing,
            pos: 0.0,
            rate: 1.0,
            at: iso_de_ms(maintenant()),
            host: "moi".into(),
            ..EtatSeance::default()
        }
    }

    fn lecteur_fichier() -> (Arc<Lecteur>, MoteurFactice, FabriqueLecteur) {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media(VIDEO, MediaFactice::video(600.0));
        let lecteur = Arc::new(lecteur);
        let l = lecteur.clone();
        let fabrique: FabriqueLecteur = Box::new(move |_| {
            Some(Box::new(LecteurFichier::nouveau(l.clone())) as Box<dyn LecteurSeance>)
        });
        (lecteur, moteur, fabrique)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn les_commandes_partent_au_serveur_et_un_refus_est_signale() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        serveur.router(
            "POST",
            "/voice/v1/watch/action",
            Reponse::json(
                403,
                json!({"error": "forbidden", "message": "Seul l’hôte pilote"}),
            ),
        );
        let (lecteur, _moteur, fabrique) = lecteur_fichier();
        let (o, signaux, _) = outils(serveur.client(), None, None, None, Some(lecteur.observer()));
        let s = SeanceActive::demarrer(
            &Handle::current(),
            "v1",
            GenreSeance::Regarder,
            PiloteSeance::nouveau(GenreSeance::Regarder, "moi", fabrique),
            o,
        );
        s.recevoir(Some(&SeanceEnCours {
            etat: etat("file", VIDEO, false),
            decalage: 0,
        }));
        attendre(
            || {
                serveur
                    .lignes()
                    .iter()
                    .any(|(c, m)| c == "/voice/v1/watch/ready" && m == "POST")
            },
            "le lecteur se dit prêt",
        )
        .await;
        assert!(s.avec(|p, t| p.commande(Commande::lire(0.0), t)).is_ok());
        attendre(
            || {
                serveur.requetes().iter().any(|r| {
                    r.ligne() == ("/voice/v1/watch/action", "POST") && r.json()["action"] == "play"
                })
            },
            "la commande part",
        )
        .await;
        attendre(
            || {
                signaux
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|x| matches!(x, SignalSeance::Refus(m) if m.contains("hôte")))
            },
            "le refus est signalé",
        )
        .await;
        serveur.router(
            "GET",
            "/voice/v1/watch",
            Reponse::json(200, json!({"state": {"rev": 9, "provider": "file", "video": VIDEO}, "now": iso_de_ms(maintenant())})),
        );
        s.reconnecte();
        attendre(
            || {
                signaux.lock().unwrap().iter().any(|x| {
                    matches!(x, SignalSeance::Etat(v) if v.channel_id == "v1" && v.state.as_ref().map(|e| e.rev) == Some(9))
                })
            },
            "l'état frais est relu à la reconnexion",
        )
        .await;
        s.arreter();
        assert_eq!(lecteur.etat().phase, crate::lecteur::Phase::Vide);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn un_film_ouvre_son_depot_retient_les_preferences_et_ferme_tout_en_quittant() {
        let estompe = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let entracte = ServeurFactice::demarrer([Reponse::vide(404)]).await;
        entracte.router(
            "GET",
            "/titles/f1",
            Reponse::json(200, json!({"audio": []})),
        );
        entracte.router(
            "POST",
            "/titles/f1/playback",
            Reponse::json(200, json!({"stream_id": "s1", "mode": "hls", "url": "/stream/j1/hls.m3u8", "duration": 3000.0, "cache": "f1-original-0"})),
        );
        entracte.router("DELETE", "/streams/s1", Reponse::vide(204));
        entracte.router("PUT", "/titles/f1/progress", Reponse::vide(204));
        let racine = tempfile::tempdir().unwrap();
        let ancien = racine.path().join(RACINE).join("ancien");
        std::fs::create_dir_all(&ancien).unwrap();
        let relais = Arc::new(Relais::demarrer(entracte.client()).await.unwrap());
        let (lecteur, _moteur) = Lecteur::factice();
        let lecteur = Arc::new(lecteur);
        let (l, base, adresse) = (
            lecteur.clone(),
            entracte.adresse().to_string(),
            relais.adresse().to_string(),
        );
        let fabrique: FabriqueLecteur = Box::new(move |_| {
            Some(Box::new(
                LecteurFilm::nouveau(l.clone(), base.clone(), PreferencesFilm::default())
                    .avec_relais(adresse.clone()),
            ) as Box<dyn LecteurSeance>)
        });
        let (o, _, prefs) = outils(
            estompe.client(),
            Some(entracte.client()),
            Some(relais.clone()),
            Some(racine.path().to_path_buf()),
            None,
        );
        let s = SeanceActive::demarrer(
            &Handle::current(),
            "k1",
            GenreSeance::Cinema,
            PiloteSeance::nouveau(GenreSeance::Cinema, "moi", fabrique),
            o,
        );
        assert!(
            !ancien.exists(),
            "les restes d'une séance interrompue sont purgés"
        );
        s.recevoir(Some(&SeanceEnCours {
            etat: etat("film", "f1", true),
            decalage: 0,
        }));
        let dossier = racine.path().join(RACINE).join(dossier_de("f1-original-0"));
        attendre(
            || dossier.exists() && relais.avance().is_some(),
            "le dépôt du titre est ouvert",
        )
        .await;
        s.avec(|p, _| p.regler_tampon(PrefTampon::Tout));
        attendre(
            || {
                prefs
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|p| p.tampon == PrefTampon::Tout)
            },
            "le réglage de préchargement est retenu",
        )
        .await;
        s.arreter();
        attendre(
            || {
                entracte
                    .lignes()
                    .iter()
                    .any(|(c, m)| c == "/streams/s1" && m == "DELETE")
            },
            "le flux est fermé en quittant",
        )
        .await;
        assert!(relais.avance().is_none());
        assert!(
            !dossier.exists(),
            "les segments du titre sont supprimés à sa fermeture"
        );
    }
}
