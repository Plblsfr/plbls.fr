pub mod decodage;
pub mod descripteurs;
pub mod file;
pub mod voisins;

use std::{
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{extract::State, http::StatusCode, response::Response};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::Row;
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

use crate::{
    etat::AppState,
    http::{self, bd, horodatage, ok},
    volume,
};

pub use descripteurs::{DIMENSION, Descripteurs};
pub use voisins::Voisins;

pub const VERSION: i32 = 1;
pub const TENTATIVES: i32 = 3;
const PERIODE: Duration = Duration::from_secs(300);
const SCRUTATION: Duration = Duration::from_millis(200);
const RECHARGEMENT: u64 = 100;
const ERREURS_LISTEES: i64 = 200;

pub struct Analyse {
    voisins: RwLock<Arc<Voisins>>,
    reveil: watch::Sender<u64>,
    a_recharger: AtomicBool,
    depuis: AtomicU64,
    pauses: AtomicU64,
}

impl Default for Analyse {
    fn default() -> Self {
        Self {
            voisins: RwLock::default(),
            reveil: watch::channel(0).0,
            a_recharger: AtomicBool::new(false),
            depuis: AtomicU64::new(0),
            pauses: AtomicU64::new(0),
        }
    }
}

impl Analyse {
    pub fn voisins(&self) -> Arc<Voisins> {
        self.voisins
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn reveiller(&self) {
        self.reveil.send_modify(|n| *n += 1);
    }

    pub fn pauses(&self) -> u64 {
        self.pauses.load(Ordering::SeqCst)
    }
}

pub async fn analyser(ffmpeg: &str, chemin: &Path) -> Result<Descripteurs, String> {
    if tokio::fs::metadata(chemin).await.is_err() {
        return Err("fichier introuvable".into());
    }
    decodage::decoder(ffmpeg, chemin, |mut blocs| {
        let mut e = descripteurs::Extracteur::nouveau();
        while let Some(b) = blocs.blocking_recv() {
            e.pousser(&b);
        }
        e.terminer()
    })
    .await?
    .ok_or_else(|| "signal trop court".to_string())
}

pub async fn recharger(state: &AppState) -> Result<usize, sqlx::Error> {
    let a = state.analyse();
    a.a_recharger.store(false, Ordering::SeqCst);
    a.depuis.store(0, Ordering::SeqCst);
    let lignes = sqlx::query(
        "SELECT a.piste_id, a.vecteur FROM analyses a JOIN pistes p ON p.id = a.piste_id
         WHERE p.present AND a.version = $1 AND cardinality(a.vecteur) = $2",
    )
    .bind(VERSION)
    .bind(DIMENSION as i32)
    .fetch_all(state.pool())
    .await?;
    let bruts: Vec<(Uuid, Vec<f32>)> = lignes.iter().map(|l| (l.get(0), l.get(1))).collect();
    let v = tokio::task::spawn_blocking(move || Voisins::construire(bruts))
        .await
        .unwrap_or_default();
    let n = v.len();
    *a.voisins.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(v);
    Ok(n)
}

pub async fn signaler(state: &AppState) {
    if let Err(e) = file::alimenter(state.pool(), VERSION).await {
        tracing::warn!("file d'analyse : {e}");
    }
    state.analyse().reveiller();
}

async fn executer(state: &AppState, t: &file::Tache) -> Result<(), String> {
    let pool = state.pool();
    let base = |e: sqlx::Error| format!("base : {e}");
    let Some((piste, chemin)) = file::fichier(pool, t).await.map_err(base)? else {
        return Ok(());
    };
    let ffmpeg = &state.config().ffmpeg;
    let d = analyser(ffmpeg, Path::new(&chemin)).await?;
    file::enregistrer(pool, piste, t.contenu_le, VERSION, &d)
        .await
        .map_err(base)?;
    if volume::lire(pool, piste, t.contenu_le)
        .await
        .map_err(base)?
        .is_none()
    {
        let m = volume::mesurer(ffmpeg, Path::new(&chemin)).await?;
        volume::enregistrer(pool, piste, t.contenu_le, &m)
            .await
            .map_err(base)?;
    }
    Ok(())
}

async fn transcodage(state: &AppState, attendu: bool) {
    while (state.lecture().transcodeur().en_cours() > 0) != attendu {
        tokio::time::sleep(SCRUTATION).await;
    }
}

async fn traiter(state: &AppState, t: &file::Tache) {
    let a = state.analyse();
    let r = tokio::select! {
        r = executer(state, t) => Some(r),
        () = transcodage(state, true) => None,
    };
    match r {
        None => {
            a.pauses.fetch_add(1, Ordering::SeqCst);
            file::remettre(state.pool(), t).await;
        }
        Some(Ok(())) => {
            file::terminer(state.pool(), t).await;
            a.a_recharger.store(true, Ordering::SeqCst);
            a.depuis.fetch_add(1, Ordering::SeqCst);
        }
        Some(Err(e)) => {
            tracing::warn!("analyse, tentative {} : {e}", t.tentatives);
            file::echouer(state.pool(), t, &e, TENTATIVES).await;
        }
    }
}

async fn recharger_ou_noter(state: &AppState) {
    match recharger(state).await {
        Ok(n) => tracing::info!("analyse : {n} pistes en mémoire"),
        Err(e) => tracing::warn!("analyse, chargement des vecteurs : {e}"),
    }
}

async fn ouvrier(state: AppState) {
    let a = state.analyse();
    let mut reveil = a.reveil.subscribe();
    loop {
        if state.lecture().transcodeur().en_cours() > 0 {
            a.pauses.fetch_add(1, Ordering::SeqCst);
            transcodage(&state, false).await;
        }
        match file::prendre(state.pool()).await {
            Ok(Some(t)) => {
                traiter(&state, &t).await;
                if a.depuis.load(Ordering::SeqCst) >= RECHARGEMENT {
                    recharger_ou_noter(&state).await;
                }
            }
            Ok(None) => {
                if a.a_recharger.load(Ordering::SeqCst) {
                    recharger_ou_noter(&state).await;
                }
                let _ = tokio::time::timeout(PERIODE, reveil.changed()).await;
                if let Err(e) = file::alimenter(state.pool(), VERSION).await {
                    tracing::warn!("file d'analyse : {e}");
                }
            }
            Err(e) => {
                tracing::warn!("file d'analyse : {e}");
                tokio::time::sleep(PERIODE).await;
            }
        }
    }
}

pub fn demarrer(state: &AppState) -> JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        let pool = state.pool();
        if let Err(e) = file::reprendre(pool, TENTATIVES).await {
            tracing::warn!("file d'analyse : {e}");
        }
        if let Err(e) = file::alimenter(pool, VERSION).await {
            tracing::warn!("file d'analyse : {e}");
        }
        recharger_ou_noter(&state).await;
        let mut ouvriers = tokio::task::JoinSet::new();
        for _ in 0..state.config().analyse_parallele {
            ouvriers.spawn(ouvrier(state.clone()));
        }
        while ouvriers.join_next().await.is_some() {}
    })
}

pub async fn etat(State(state): State<AppState>) -> Result<Response, Response> {
    let pool = state.pool();
    let f = file::avancement(pool).await.map_err(bd)?;
    let c = sqlx::query(
        "SELECT (SELECT count(*) FROM pistes WHERE present),
                (SELECT count(*) FROM analyses a JOIN pistes p ON p.id = a.piste_id WHERE p.present AND a.version = $1)",
    )
    .bind(VERSION)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    let erreurs = sqlx::query(
        "SELECT p.id, p.titre, t.erreur, t.tentatives, t.modifie_le FROM taches_analyse t
         JOIN fichiers f ON f.id = t.fichier_id JOIN pistes p ON p.id = f.piste_id
         WHERE t.etat = 'erreur' ORDER BY t.modifie_le DESC, p.id LIMIT $1",
    )
    .bind(ERREURS_LISTEES)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "version": VERSION,
        "parallel": state.config().analyse_parallele,
        "paused": state.lecture().transcodeur().en_cours() > 0,
        "tasks": { "pending": f.attente, "running": f.en_cours, "done": f.faites, "failed": f.erreurs },
        "tracks": c.get::<i64, _>(0),
        "analysed": c.get::<i64, _>(1),
        "loaded": state.analyse().voisins().len(),
        "errors": erreurs.iter().map(|l| json!({
            "track": l.get::<Uuid, _>(0),
            "title": l.get::<String, _>(1),
            "error": l.get::<Option<String>, _>(2),
            "attempts": l.get::<i32, _>(3),
            "at": horodatage(&l.get::<DateTime<Utc>, _>(4)),
        })).collect::<Vec<_>>(),
    })))
}

pub async fn relancer(State(state): State<AppState>) -> Result<Response, Response> {
    let n = file::relancer(state.pool()).await.map_err(bd)?;
    signaler(&state).await;
    Ok(http::json(StatusCode::ACCEPTED, &json!({ "requeued": n })))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::{
        tests_api::{Bibliotheque, FLAC, MP3, bibliotheque_avec, piste},
        transcodage::{Cible, Format, Travail},
    };

    const CLIC: &str = "aevalsrc='0.8*sin(2*PI*1000*t)*lt(mod(t\\,PERIODE)\\,0.02)':s=44100:d=20";
    const MORCEAU: &str = "aevalsrc='0.25*sin(2*PI*220*t)*(0.6+0.4*sin(2*PI*2*t))+0.15*sin(2*PI*330*t)\
        +0.3*sin(2*PI*1760*t)*lt(mod(t\\,0.5)\\,0.05)':s=44100:d=15[s];\
        anoisesrc=color=pink:amplitude=0.05:duration=15:seed=3[n];[s][n]amix=inputs=2:normalize=0";
    const AUTRE: &str = "aevalsrc='0.2*sin(2*PI*587*t)+0.2*sin(2*PI*880*t)*lt(mod(t\\,0.4)\\,0.1)':s=44100:d=15[s];\
        anoisesrc=color=white:amplitude=0.1:duration=15:seed=4[n];[s][n]amix=inputs=2:normalize=0";
    const OPUS: &[&str] = &["-c:a", "libopus", "-b:a", "64k"];

    fn signal(dest: &Path, source: &str, codec: &[&str]) {
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let ok = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(["-filter_complex", source])
            .args(codec)
            .arg(dest)
            .status()
            .unwrap()
            .success();
        assert!(ok, "génération de {}", dest.display());
    }

    async fn mesure(dossier: &Path, nom: &str, source: &str, codec: &[&str]) -> Descripteurs {
        let f = dossier.join(nom);
        signal(&f, source, codec);
        analyser("ffmpeg", &f).await.unwrap()
    }

    fn distance(a: &Descripteurs, b: &Descripteurs) -> f32 {
        let somme: f32 = a
            .vecteur()
            .iter()
            .zip(b.vecteur())
            .map(|(x, y)| (x - y) * (x - y))
            .sum();
        (somme / DIMENSION as f32).sqrt()
    }

    #[tokio::test]
    async fn tempo_centroide_et_energie_des_signaux_generes() {
        if !crate::tests_api::ffmpeg_present() {
            eprintln!("ignoré : ffmpeg absent");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        for (bpm, periode) in [(120.0, "0.5"), (90.0, "2/3"), (150.0, "0.4"), (70.0, "6/7")] {
            let clic = mesure(
                d.path(),
                "clic.flac",
                &CLIC.replace("PERIODE", periode),
                FLAC,
            )
            .await;
            let tempo = clic.tempo.unwrap();
            assert!((tempo - bpm).abs() < 1.0, "{bpm} : {tempo}");
        }
        let sinus = mesure(
            d.path(),
            "sinus.flac",
            "sine=frequency=1000:duration=10",
            FLAC,
        )
        .await;
        assert!((sinus.centroide - 1000.0).abs() < 20.0, "{sinus:?}");
        assert!((sinus.energie + 21.07).abs() < 0.2, "{sinus:?}");
        assert!(
            (sinus.passages - 2000.0 / 22_050.0).abs() < 0.005,
            "{sinus:?}"
        );
        assert!(
            sinus.etalement < 300.0 && sinus.dynamique < 0.1,
            "{sinus:?}"
        );
        assert_eq!((sinus.tempo, sinus.tonalite), (None, 11));
        assert!((sinus.duree - 10.0).abs() < 0.05, "{sinus:?}");
        let faible = mesure(
            d.path(),
            "faible.flac",
            "sine=frequency=1000:duration=10,volume=-6dB",
            FLAC,
        )
        .await;
        assert!(
            (sinus.energie - faible.energie - 6.0).abs() < 0.2,
            "{faible:?}"
        );
        let blanc = mesure(
            d.path(),
            "blanc.flac",
            "anoisesrc=color=white:amplitude=0.5:duration=10:seed=1",
            FLAC,
        )
        .await;
        assert!((blanc.centroide - 5512.5).abs() < 300.0, "{blanc:?}");
        assert!(
            blanc.etalement > 2500.0 && blanc.passages > 0.4,
            "{blanc:?}"
        );
        assert!(blanc.energie > -16.0 && blanc.energie < -13.0, "{blanc:?}");
        let rose = mesure(
            d.path(),
            "rose.flac",
            "anoisesrc=color=pink:amplitude=0.5:duration=10:seed=1",
            FLAC,
        )
        .await;
        assert!(rose.centroide < blanc.centroide - 1000.0, "{rose:?}");
        assert!(rose.pente < blanc.pente, "{rose:?}");
    }

    #[tokio::test]
    async fn deux_encodages_voisins_deux_signaux_eloignes() {
        if !crate::tests_api::ffmpeg_present() {
            eprintln!("ignoré : ffmpeg absent");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let flac = mesure(d.path(), "a.flac", MORCEAU, FLAC).await;
        let mp3 = mesure(d.path(), "a.mp3", MORCEAU, MP3).await;
        let autre = mesure(d.path(), "b.flac", AUTRE, FLAC).await;
        let autre_mp3 = mesure(d.path(), "b.mp3", AUTRE, MP3).await;
        let (memes, autres) = (distance(&flac, &mp3), distance(&autre, &autre_mp3));
        let entre = distance(&flac, &autre).min(distance(&mp3, &autre_mp3));
        eprintln!("mêmes signaux {memes} et {autres}, signaux différents {entre}");
        assert!(memes < 0.05 && autres < 0.05, "{memes} {autres}");
        assert!(entre > 0.3, "{entre}");
        assert!((flac.tempo.unwrap() - mp3.tempo.unwrap()).abs() < 0.5);
        assert_eq!((flac.tonalite, flac.majeur), (mp3.tonalite, mp3.majeur));
    }

    #[tokio::test]
    async fn fichier_absent_ou_illisible_refuse_sans_chemin() {
        if !crate::tests_api::ffmpeg_present() {
            eprintln!("ignoré : ffmpeg absent");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let absent = d.path().join("absent.flac");
        assert_eq!(
            analyser("ffmpeg", &absent).await.unwrap_err(),
            "fichier introuvable"
        );
        let faux = d.path().join("faux.flac");
        std::fs::write(&faux, vec![7u8; 4096]).unwrap();
        assert_eq!(
            analyser("ffmpeg", &faux).await.unwrap_err(),
            "décodage refusé par ffmpeg"
        );
        let court = d.path().join("court.flac");
        signal(&court, "sine=frequency=440:duration=0.5", FLAC);
        assert_eq!(
            analyser("ffmpeg", &court).await.unwrap_err(),
            "signal trop court"
        );
    }

    struct Banc {
        b: Bibliotheque,
        dossier: tempfile::TempDir,
    }

    impl Banc {
        fn etat(&self) -> &AppState {
            &self.b.e.state
        }

        async fn id(&self, album: &str, titre: &str) -> Uuid {
            let a = self.b.album(album).await;
            Uuid::parse_str(piste(&a, titre)["id"].as_str().unwrap()).unwrap()
        }

        async fn file(&self) -> file::Avancement {
            file::avancement(self.etat().pool()).await.unwrap()
        }

        async fn tentatives(&self) -> Vec<i32> {
            sqlx::query_scalar("SELECT tentatives FROM taches_analyse ORDER BY tentatives")
                .fetch_all(self.etat().pool())
                .await
                .unwrap()
        }

        async fn compte(&self, table: &str) -> i64 {
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(self.etat().pool())
                .await
                .unwrap()
        }

        async fn attendre(&self, quoi: &str, condition: impl Fn(file::Avancement) -> bool) {
            let limite = Instant::now() + Duration::from_secs(60);
            loop {
                let f = self.file().await;
                if condition(f) {
                    return;
                }
                assert!(Instant::now() < limite, "délai dépassé : {quoi} ({f:?})");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        fn marquer(&self, nom: &str) {
            std::fs::write(self.dossier.path().join(nom), b"").unwrap();
        }

        fn transcoder(&self, cle: &str, ffmpeg: &str) {
            let lance = self.etat().lecture().transcodeur().lancer(
                cle,
                Travail {
                    ffmpeg: ffmpeg.to_string(),
                    source: self.b.chemin("Beyonce/Lemonade/01 Formation.flac"),
                    cible: Cible {
                        format: Format::Opus,
                        kbps: 96,
                    },
                    canaux: Some(1),
                    frequence: Some(44_100),
                },
            );
            assert!(lance.is_ok());
        }
    }

    async fn banc(ffmpeg: Option<String>) -> Option<Banc> {
        let dossier = tempfile::tempdir().unwrap();
        let cache: PathBuf = dossier.path().join("cache");
        let b = bibliotheque_avec(move |c| {
            c.cache = cache;
            if let Some(f) = ffmpeg {
                c.ffmpeg = f;
            }
        })
        .await?;
        Some(Banc { b, dossier })
    }

    fn faux_ffmpeg(dossier: &Path) -> String {
        let chemin = dossier.join("ffmpeg");
        let d = dossier.display();
        std::fs::write(
            &chemin,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n  *libopus*) while [ ! -e \"{d}/transcodage\" ]; do sleep 0.05; done;;\n  \
                 *f32le*) while [ ! -e \"{d}/analyse\" ]; do sleep 0.05; done;;\nesac\nexec ffmpeg \"$@\"\n"
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        chemin.to_string_lossy().into_owned()
    }

    async fn banc_bloquant() -> Option<Banc> {
        let scripts = tempfile::tempdir().unwrap();
        let ffmpeg = faux_ffmpeg(scripts.path());
        let mut t = banc(Some(ffmpeg)).await?;
        t.dossier = scripts;
        Some(t)
    }

    #[tokio::test]
    async fn import_analyse_niveau_et_voisins_en_memoire() {
        let Some(t) = banc(None).await else { return };
        signal(
            &t.b.chemin("Beyonce/Lemonade/01 Formation.flac"),
            MORCEAU,
            FLAC,
        );
        signal(&t.b.chemin("Beyonce/Lemonade/02 Hold Up.mp3"), MORCEAU, MP3);
        signal(&t.b.chemin("Sigur Ros/Takk/01 Glosoli.flac"), AUTRE, FLAC);
        let etat = t.etat();
        assert!(etat.analyse().voisins().is_empty());
        let _ouvriers = demarrer(etat);
        t.b.synchro().await;
        t.attendre("analyse à l'import", |f| f.faites == 4 && f.finie())
            .await;
        crate::testutil::attendre(
            || etat.analyse().voisins().len() == 4,
            "vecteurs en mémoire",
        )
        .await;
        let v = etat.analyse().voisins();
        let (formation, hold_up) = (
            t.id("Lemonade", "Formation").await,
            t.id("Lemonade", "Hold Up").await,
        );
        let (sorry, glosoli) = (
            t.id("Lemonade", "Sorry").await,
            t.id("Takk…", "Glósóli").await,
        );
        let absente = Uuid::new_v4();
        let (memes, autres) = (
            v.distance(formation, hold_up).unwrap(),
            v.distance(formation, glosoli).unwrap(),
        );
        eprintln!("FLAC et MP3 du même signal {memes}, autre signal {autres}");
        assert!(memes < 0.1 && autres > 10.0 * memes, "{memes} {autres}");
        assert!(v.distance(formation, sorry).unwrap() > 10.0 * memes);
        let proches = v.voisins(formation, 3).unwrap();
        assert_eq!(proches[0].0, hold_up);
        assert_eq!(proches.len(), 3);
        assert_eq!(v.vecteur(formation).unwrap().len(), DIMENSION);
        assert_eq!(v.voisins(absente, 3), None);
        assert_eq!(v.distance(formation, absente), None);
        assert_eq!(t.compte("mesures").await, 4);
        assert_eq!(t.compte("analyses").await, 4);
        let (version, tempo): (i32, Option<f32>) =
            sqlx::query_as("SELECT version, tempo FROM analyses WHERE piste_id = $1")
                .bind(formation)
                .fetch_one(etat.pool())
                .await
                .unwrap();
        assert_eq!(version, VERSION);
        assert!((tempo.unwrap() - 120.0).abs() < 1.0, "{tempo:?}");
        let r = t.b.e.get("admin", "/admin/analysis").await;
        assert_eq!(r.statut, StatusCode::OK);
        let j = r.json();
        assert_eq!(
            j["tasks"],
            json!({"pending": 0, "running": 0, "done": 4, "failed": 0})
        );
        assert_eq!(
            (&j["analysed"], &j["loaded"], &j["version"]),
            (&json!(4), &json!(4), &json!(VERSION))
        );
        assert_eq!((&j["paused"], &j["errors"]), (&json!(false), &json!([])));
        assert_eq!(
            t.b.e.get("alice", "/admin/analysis").await.statut,
            StatusCode::FORBIDDEN
        );
        t.b.synchro().await;
        assert_eq!(file::alimenter(etat.pool(), VERSION).await.unwrap(), 0);
        assert_eq!(t.tentatives().await, [1, 1, 1, 1]);
    }

    #[tokio::test]
    async fn fichier_illisible_en_erreur_apres_ses_tentatives_sans_bloquer_la_file() {
        let Some(t) = banc(None).await else { return };
        let etat = t.etat();
        t.b.synchro().await;
        let illisible = t.b.chemin("Beyonce/Lemonade/03 Sorry.opus");
        std::fs::write(&illisible, vec![7u8; 4096]).unwrap();
        let _ouvriers = demarrer(etat);
        t.attendre("file vidée", |f| f.finie() && f.faites + f.erreurs == 4)
            .await;
        let f = t.file().await;
        assert_eq!((f.faites, f.erreurs), (3, 1));
        assert_eq!(t.tentatives().await, [1, 1, 1, TENTATIVES]);
        assert_eq!(t.compte("analyses").await, 3);
        let r = t.b.e.get("admin", "/admin/analysis").await;
        let j = r.json();
        assert_eq!(j["tasks"]["failed"], 1);
        let erreurs = j["errors"].as_array().unwrap();
        assert_eq!(erreurs.len(), 1);
        assert_eq!(erreurs[0]["track"], json!(t.id("Lemonade", "Sorry").await));
        assert_eq!(erreurs[0]["title"], "Sorry");
        assert_eq!(erreurs[0]["error"], "décodage refusé par ffmpeg");
        assert_eq!(erreurs[0]["attempts"], TENTATIVES);
        t.b.sans_fuite(&r.texte(), "erreurs d'analyse");
        assert!(!r.texte().contains("Sorry.opus"));
        signal(&illisible, "sine=frequency=440:duration=2", OPUS);
        let r =
            t.b.e
                .appel(Some("admin"), "POST", "/admin/analysis/retry", None)
                .await;
        assert_eq!(r.statut, StatusCode::ACCEPTED);
        assert_eq!(r.json(), json!({"requeued": 1}));
        t.attendre("relance", |f| f.finie() && f.faites == 4).await;
        assert_eq!(t.compte("analyses").await, 4);
        let r =
            t.b.e
                .appel(Some("admin"), "POST", "/admin/analysis/retry", None)
                .await;
        assert_eq!(r.json(), json!({"requeued": 0}));
    }

    #[tokio::test]
    async fn arret_pendant_une_analyse_reprise_au_redemarrage_sans_doublon() {
        let Some(t) = banc_bloquant().await else {
            return;
        };
        let etat = t.etat();
        t.b.synchro().await;
        let ouvriers = demarrer(etat);
        t.attendre("analyse en cours", |f| f.en_cours == 1).await;
        ouvriers.abort();
        assert!(ouvriers.await.unwrap_err().is_cancelled());
        let f = t.file().await;
        assert_eq!((f.en_cours, f.attente, f.faites), (1, 3, 0));
        assert_eq!(t.compte("analyses").await, 0);
        t.marquer("analyse");
        let redemarre = AppState::new(etat.config().clone(), etat.pool().clone());
        let _ouvriers = demarrer(&redemarre);
        t.attendre("reprise", |f| f.finie() && f.faites == 4).await;
        assert_eq!(t.tentatives().await, [1, 1, 1, 2]);
        assert_eq!(t.compte("taches_analyse").await, 4);
        assert_eq!(t.compte("analyses").await, 4);
        assert_eq!(t.compte("mesures").await, 4);
        crate::testutil::attendre(|| redemarre.analyse().voisins().len() == 4, "vecteurs").await;
        sqlx::query(
            "UPDATE taches_analyse SET etat = 'en_cours', tentatives = $1
             WHERE fichier_id = (SELECT fichier_id FROM taches_analyse ORDER BY fichier_id LIMIT 1)",
        )
        .bind(TENTATIVES)
        .execute(etat.pool())
        .await
        .unwrap();
        assert_eq!(file::reprendre(etat.pool(), TENTATIVES).await.unwrap(), 1);
        let f = t.file().await;
        assert_eq!((f.erreurs, f.faites, f.en_cours), (1, 3, 0));
    }

    #[tokio::test]
    async fn version_plus_ancienne_remise_en_file() {
        let Some(t) = banc(None).await else { return };
        let etat = t.etat();
        let _ouvriers = demarrer(etat);
        t.b.synchro().await;
        t.attendre("première analyse", |f| f.finie() && f.faites == 4)
            .await;
        crate::testutil::attendre(|| etat.analyse().voisins().len() == 4, "vecteurs").await;
        for table in ["taches_analyse", "analyses"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE {table} SET version = version - 1"
            )))
            .execute(etat.pool())
            .await
            .unwrap();
        }
        assert_eq!(recharger(etat).await.unwrap(), 0);
        assert!(etat.analyse().voisins().is_empty());
        let avant: DateTime<Utc> = sqlx::query_scalar("SELECT max(analyse_le) FROM analyses")
            .fetch_one(etat.pool())
            .await
            .unwrap();
        assert_eq!(file::alimenter(etat.pool(), VERSION).await.unwrap(), 4);
        assert_eq!(t.file().await.faites, 0);
        etat.analyse().reveiller();
        t.attendre("nouvelle analyse", |f| f.finie() && f.faites == 4)
            .await;
        let refaites: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM analyses WHERE version = $1 AND analyse_le > $2",
        )
        .bind(VERSION)
        .bind(avant)
        .fetch_one(etat.pool())
        .await
        .unwrap();
        assert_eq!(refaites, 4);
        assert_eq!(t.compte("analyses").await, 4);
        crate::testutil::attendre(|| etat.analyse().voisins().len() == 4, "vecteurs rechargés")
            .await;
        assert_eq!(file::alimenter(etat.pool(), VERSION).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn pas_d_analyse_pendant_un_transcodage_de_lecture() {
        let Some(t) = banc_bloquant().await else {
            return;
        };
        let etat = t.etat();
        let ffmpeg = etat.config().ffmpeg.clone();
        let transcodeur = etat.lecture().transcodeur().clone();
        t.marquer("analyse");
        t.b.synchro().await;
        t.transcoder("a.opus", &ffmpeg);
        assert_eq!(transcodeur.en_cours(), 1);
        let _ouvriers = demarrer(etat);
        crate::testutil::attendre(|| etat.analyse().pauses() >= 1, "analyse en pause").await;
        let f = t.file().await;
        assert_eq!((f.attente, f.en_cours, f.faites), (4, 0, 0));
        assert_eq!(t.tentatives().await, [0, 0, 0, 0]);
        assert_eq!(
            t.b.e.get("admin", "/admin/analysis").await.json()["paused"],
            true
        );
        assert_eq!(transcodeur.en_cours(), 1);
        t.marquer("transcodage");
        t.attendre("analyse après le transcodage", |f| {
            f.finie() && f.faites == 4
        })
        .await;
        assert_eq!(transcodeur.en_cours(), 0);

        for marque in ["analyse", "transcodage"] {
            std::fs::remove_file(t.dossier.path().join(marque)).unwrap();
        }
        sqlx::query("UPDATE taches_analyse SET version = version - 1")
            .execute(etat.pool())
            .await
            .unwrap();
        signaler(etat).await;
        t.attendre("analyse en cours", |f| f.en_cours == 1).await;
        let pauses = etat.analyse().pauses();
        t.transcoder("b.opus", &ffmpeg);
        let analyse = etat.analyse();
        t.attendre("analyse interrompue", |f| {
            f.en_cours == 0 && analyse.pauses() > pauses
        })
        .await;
        assert_eq!(transcodeur.en_cours(), 1);
        assert_eq!(t.file().await.attente, 4);
        assert_eq!(t.tentatives().await, [0, 0, 0, 0]);
        t.marquer("analyse");
        t.marquer("transcodage");
        t.attendre("fin", |f| f.finie() && f.faites == 4).await;
        assert_eq!(t.tentatives().await, [1, 1, 1, 1]);
    }
}
