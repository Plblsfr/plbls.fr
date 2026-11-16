use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    bibliotheque::uuid,
    catalogue::{self, FichierPiste},
    config::Config,
    droits::Droit,
    etat::AppState,
    flux::{self, Billet, Debut, VALIDITE, Variante, maintenant_unix},
    http::{bd, erreur, horodatage, indisponible, introuvable, invalide, ok},
    identite::{Identite, justificatif},
    preferences,
    transcodage::{Cible, Disponible, Format, Occupe, Qualite, Transcodeur, Travail},
    volume,
};

const ATTENTE_DEBUT: Duration = Duration::from_secs(60);
const SANS_PERTE: [&str; 5] = ["flac", "alac", "pcm", "wavpack", "ape"];

pub struct Lecture {
    transcodeur: Arc<Transcodeur>,
    mesures: Arc<Semaphore>,
}

impl Lecture {
    pub fn nouveau(cfg: &Config) -> Self {
        Self {
            transcodeur: Arc::new(Transcodeur::nouveau(
                cfg.cache.clone(),
                cfg.cache_max_octets,
                cfg.max_transcodages,
            )),
            mesures: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn transcodeur(&self) -> &Arc<Transcodeur> {
        &self.transcodeur
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Support {
    pub codecs: Vec<String>,
    pub conteneurs: Vec<String>,
}

impl Support {
    pub fn lire(v: &Value) -> Result<Self, Response> {
        let liste = |k: &str| -> Result<Vec<String>, Response> {
            match v.get(k) {
                None | Some(Value::Null) => Ok(Vec::new()),
                Some(Value::Array(a)) if a.len() <= 32 => a
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 32)
                            .map(str::to_ascii_lowercase)
                            .ok_or_else(|| invalide(&format!("{k} : liste de noms attendue")))
                    })
                    .collect(),
                Some(_) => Err(invalide(&format!("{k} : liste de noms attendue"))),
            }
        };
        Ok(Support {
            codecs: liste("codecs")?,
            conteneurs: liste("containers")?,
        })
    }

    fn lit(&self, codec: &str, conteneur: &str) -> bool {
        self.codecs.iter().any(|c| c == codec) && self.conteneurs.iter().any(|c| c == conteneur)
    }
}

pub fn codec_public(codec: &str) -> &str {
    if codec.starts_with("pcm_") {
        "pcm"
    } else {
        codec
    }
}

pub fn famille(conteneur: &str) -> &'static str {
    let c = conteneur.to_ascii_lowercase();
    let noms: Vec<&str> = c.split(',').collect();
    let a = |n: &str| noms.contains(&n);
    if a("mp4") || a("m4a") || a("mov") {
        "mp4"
    } else if a("webm") || a("matroska") {
        "webm"
    } else {
        ["flac", "mp3", "ogg", "wav", "aiff", "aac", "wv", "ape"]
            .into_iter()
            .find(|n| a(n))
            .unwrap_or("autre")
    }
}

pub fn mime_direct(conteneur: &str, codec: &str) -> String {
    match famille(conteneur) {
        "flac" => "audio/flac".into(),
        "mp3" => "audio/mpeg".into(),
        "ogg" => format!("audio/ogg; codecs={}", codec_public(codec)),
        "mp4" => "audio/mp4".into(),
        "webm" => "audio/webm".into(),
        "wav" => "audio/wav".into(),
        "aiff" => "audio/aiff".into(),
        "aac" => "audio/aac".into(),
        "wv" => "audio/x-wavpack".into(),
        "ape" => "audio/x-ape".into(),
        _ => "application/octet-stream".into(),
    }
}

pub fn decider(f: &FichierPiste, s: &Support, q: Qualite) -> Variante {
    let codec = codec_public(&f.codec);
    let lisible = s.lit(codec, famille(&f.conteneur));
    let leger = !SANS_PERTE.contains(&codec)
        && f.debit
            .is_some_and(|d| u64::try_from(d).unwrap_or(u64::MAX) <= u64::from(q.kbps()) * 1100);
    if lisible && (q == Qualite::Originale || leger) {
        return Variante::Direct;
    }
    let format = if s.lit("opus", "ogg") {
        Format::Opus
    } else {
        Format::Aac
    };
    Variante::Transcode(Cible {
        format,
        kbps: q.kbps(),
    })
}

pub fn cle_de_cache(f: &FichierPiste, c: Cible) -> String {
    format!(
        "{}-{}-{}.{}",
        f.id.simple(),
        f.contenu_le.timestamp_micros(),
        c.texte(),
        c.format.extension()
    )
}

fn occupe() -> Response {
    let mut r = erreur(
        StatusCode::SERVICE_UNAVAILABLE,
        "server_busy",
        "trop de transcodages en cours, réessayez dans quelques secondes",
    );
    r.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    r
}

fn lancer(state: &AppState, f: &FichierPiste, c: Cible) -> Result<(), Occupe> {
    state.lecture().transcodeur.lancer(
        &cle_de_cache(f, c),
        Travail {
            ffmpeg: state.config().ffmpeg.clone(),
            source: PathBuf::from(&f.chemin),
            cible: c,
            canaux: f.canaux,
            frequence: f.frequence,
        },
    )
}

fn mesurer_album(state: &AppState, piste: Uuid) {
    let etat = state.clone();
    tokio::spawn(async move {
        let Ok(_jeton) = etat.lecture().mesures.clone().acquire_owned().await else {
            return;
        };
        let pool = etat.pool();
        let Ok(pistes) = volume::non_mesurees_de_l_album(pool, piste).await else {
            return;
        };
        for p in pistes {
            if let Ok(Some(f)) = catalogue::fichier_de_piste(pool, p).await {
                let _ = volume::mesure_de(pool, &etat.config().ffmpeg, &f).await;
            }
        }
    });
}

pub async fn ouvrir(
    State(state): State<AppState>,
    p: crate::garde::Personne,
    Path(id): Path<String>,
    corps: Bytes,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let v: Value = if corps.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&corps).map_err(|_| invalide("JSON attendu"))?
    };
    if !v.is_object() {
        return Err(invalide("objet JSON attendu"));
    }
    let support = Support::lire(&v)?;
    let pool = state.pool();
    let qualite = match v.get("quality") {
        None | Some(Value::Null) => preferences::qualite_de(pool, p.sub()).await.map_err(bd)?,
        Some(q) => q
            .as_str()
            .and_then(Qualite::depuis)
            .ok_or_else(|| invalide("qualité invalide : originale, haute, normale ou basse"))?,
    };
    let f = catalogue::fichier_de_piste(pool, id)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let variante = decider(&f, &support, qualite);
    if let Variante::Transcode(c) = variante {
        lancer(&state, &f, c).map_err(|_| occupe())?;
    }
    let mesure = volume::mesure_de(pool, &state.config().ffmpeg, &f)
        .await
        .map_err(bd)?;
    mesurer_album(&state, id);
    let gains = volume::gains(pool, id, mesure.as_ref()).await.map_err(bd)?;
    let exp = maintenant_unix() + VALIDITE.as_secs();
    let jeton = flux::signer(
        &state.config().secret_flux,
        &Billet {
            sub: p.sub().to_string(),
            piste: id,
            fichier: f.id,
            variante,
            exp,
        },
    );
    let frequence = mesure
        .as_ref()
        .map(|m| m.frequence)
        .or(f.frequence)
        .unwrap_or(0);
    let echantillons = mesure
        .as_ref()
        .map(|m| m.echantillons)
        .or_else(|| f.duree.map(|d| (d * f64::from(frequence)).round() as i64));
    let (mode, qualite, format, sortie, delai) = match variante {
        Variante::Direct => (
            "direct",
            qualite,
            json!({
                "codec": codec_public(&f.codec),
                "container": famille(&f.conteneur),
                "mime": mime_direct(&f.conteneur, &f.codec),
                "bitrate": f.debit,
            }),
            frequence,
            0,
        ),
        Variante::Transcode(c) => (
            "transcode",
            Qualite::depuis_kbps(c.kbps).unwrap_or(Qualite::Haute),
            json!({
                "codec": c.format.codec(),
                "container": c.format.conteneur(),
                "mime": c.format.mime(),
                "bitrate": c.kbps * 1000,
            }),
            c.format.frequence(f.frequence),
            c.format.delai(),
        ),
    };
    let expire = chrono::DateTime::from_timestamp(i64::try_from(exp).unwrap_or(i64::MAX), 0)
        .unwrap_or_default();
    Ok(ok(&json!({
        "track": id,
        "url": format!("/stream/{jeton}"),
        "expires_at": horodatage(&expire),
        "mode": mode,
        "quality": qualite.nom(),
        "format": format,
        "duration": mesure.as_ref().map(volume::Mesure::duree).or(f.duree),
        "samples": echantillons,
        "sample_rate": frequence,
        "output_sample_rate": sortie,
        "encoder_delay": delai,
        "gain": {
            "reference": volume::CIBLE,
            "loudness": gains.niveau,
            "track": gains.piste,
            "track_peak": gains.pic_piste,
            "album": gains.album,
            "album_peak": gains.pic_album,
        },
    })))
}

fn jeton_refuse() -> Response {
    erreur(
        StatusCode::UNAUTHORIZED,
        "stream_expired",
        "flux expiré ou invalide",
    )
}

async fn acces(state: &AppState, jeton: &str, entetes: &HeaderMap) -> Result<Billet, Response> {
    let b = flux::valider(&state.config().secret_flux, jeton, maintenant_unix())
        .ok_or_else(jeton_refuse)?;
    if let Some(j) = justificatif(entetes)
        && let Ok(id) = state.identites().resoudre(Some(&j)).await
        && id.sub != b.sub
    {
        return Err(erreur(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce flux appartient à une autre personne",
        ));
    }
    let identite = Identite {
        sub: b.sub.clone(),
        nom: String::new(),
        email: String::new(),
    };
    match state.droits_de(&identite).await {
        Ok(d) if d.a(Droit::Acces) => Ok(b),
        Ok(_) => Err(crate::garde::interdit(Droit::Acces)),
        Err(_) => Err(erreur(
            StatusCode::SERVICE_UNAVAILABLE,
            "aegis_unavailable",
            "service de droits injoignable",
        )),
    }
}

pub async fn flux(
    State(state): State<AppState>,
    Path(jeton): Path<String>,
    entetes: HeaderMap,
) -> Result<Response, Response> {
    let b = acces(&state, &jeton, &entetes).await?;
    let f = catalogue::fichier_de_piste(state.pool(), b.piste)
        .await
        .map_err(bd)?
        .filter(|f| f.id == b.fichier)
        .ok_or_else(introuvable)?;
    let c = match b.variante {
        Variante::Direct => {
            let mime = mime_direct(&f.conteneur, &f.codec);
            return flux::servir_fichier(std::path::Path::new(&f.chemin), &mime, &entetes, None)
                .await;
        }
        Variante::Transcode(c) => c,
    };
    let cle = cle_de_cache(&f, c);
    let transcodeur = &state.lecture().transcodeur;
    for essai in 0..2 {
        match transcodeur.obtenir(&cle) {
            Some(Disponible::Pret(chemin, garde)) => {
                return flux::servir_fichier(&chemin, c.format.mime(), &entetes, Some(garde)).await;
            }
            Some(Disponible::EnCours(fichier, rx, garde)) => {
                return match flux::servir_croissant(
                    fichier,
                    rx,
                    garde,
                    c.format.mime(),
                    ATTENTE_DEBUT,
                )
                .await
                {
                    Debut::Pret(r) => Ok(r),
                    Debut::Echec => {
                        Err(indisponible("transcode_failed", "le transcodage a échoué"))
                    }
                    Debut::Delai => Err(indisponible(
                        "transcode_timeout",
                        "le transcodage ne démarre pas",
                    )),
                };
            }
            None if essai == 0 => lancer(&state, &f, c).map_err(|_| occupe())?,
            None => break,
        }
    }
    Err(indisponible("transcode_failed", "le transcodage a échoué"))
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path as CheminFs,
        time::{Duration, Instant},
    };

    use axum::body::{Body, to_bytes};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        tests_api::{Bibliotheque, bibliotheque_avec, generer, piste},
        testutil::{Rep, appel_sur},
    };

    struct Banc {
        b: Bibliotheque,
        cache: tempfile::TempDir,
    }

    impl Banc {
        async fn id(&self, album: &str, titre: &str) -> String {
            let a = self.b.album(album).await;
            piste(&a, titre)["id"].as_str().unwrap().to_string()
        }

        async fn lire(&self, qui: &str, id: &str, corps: Value) -> Rep {
            self.b
                .e
                .appel(
                    Some(qui),
                    "POST",
                    &format!("/tracks/{id}/playback"),
                    Some(corps),
                )
                .await
        }

        async fn flux(&self, url: &str, entetes: &[(&str, &str)]) -> Rep {
            self.b.e.appel_avec(None, "GET", url, None, entetes).await
        }

        fn transcodeur(&self) -> &Arc<Transcodeur> {
            self.b.e.state.lecture().transcodeur()
        }

        fn en_cache(&self, ext: &str) -> Vec<std::path::PathBuf> {
            let mut v: Vec<_> = std::fs::read_dir(self.cache.path())
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.to_string_lossy().ends_with(ext))
                .collect();
            v.sort();
            v
        }
    }

    async fn banc(regler: impl FnOnce(&mut Config) + Send + 'static) -> Option<Banc> {
        let cache = tempfile::tempdir().unwrap();
        let dossier = cache.path().to_path_buf();
        let b = bibliotheque_avec(move |c| {
            c.cache = dossier;
            regler(c);
        })
        .await?;
        Some(Banc { b, cache })
    }

    async fn attendre_async<F: std::future::Future<Output = bool>>(
        mut condition: impl FnMut() -> F,
        quoi: &str,
    ) {
        let limite = Instant::now() + Duration::from_secs(30);
        while !condition().await {
            assert!(Instant::now() < limite, "délai dépassé : {quoi}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn echantillons_decodes(octets: &[u8], ext: &str) -> usize {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join(format!("a.{ext}"));
        std::fs::write(&f, octets).unwrap();
        let s = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-i"])
            .arg(&f)
            .args(["-f", "u8", "-ac", "1", "pipe:1"])
            .output()
            .unwrap();
        assert!(s.status.success());
        s.stdout.len()
    }

    async fn codec_de(octets: &[u8], ext: &str) -> Option<String> {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join(format!("a.{ext}"));
        std::fs::write(&f, octets).unwrap();
        crate::ffprobe::analyser("ffprobe", &f).await.unwrap().codec
    }

    const SANS_FLAC: fn() -> Value =
        || json!({"codecs": ["opus", "mp3", "aac"], "containers": ["ogg", "mp3", "mp4"]});

    fn fichier(codec: &str, conteneur: &str, debit: Option<i32>) -> FichierPiste {
        FichierPiste {
            id: Uuid::from_u128(1),
            piste_id: Uuid::from_u128(2),
            chemin: "/m/a".into(),
            taille: 1,
            conteneur: conteneur.into(),
            codec: codec.into(),
            debit,
            frequence: Some(44_100),
            canaux: Some(2),
            bits: None,
            duree: Some(1.0),
            contenu_le: chrono::Utc::now(),
        }
    }

    #[test]
    fn decision_directe_ou_transcodee() {
        let navigateur = Support {
            codecs: vec!["flac".into(), "mp3".into(), "opus".into(), "aac".into()],
            conteneurs: vec!["flac".into(), "mp3".into(), "ogg".into(), "mp4".into()],
        };
        let safari = Support {
            codecs: vec!["mp3".into(), "aac".into(), "alac".into()],
            conteneurs: vec!["mp3".into(), "mp4".into()],
        };
        let flac = fichier("flac", "flac", Some(900_000));
        let mp3 = fichier("mp3", "mp3", Some(128_000));
        let alac = fichier("alac", "mov,mp4,m4a,3gp,3g2,mj2", None);
        let opus = |kbps| {
            Variante::Transcode(Cible {
                format: Format::Opus,
                kbps,
            })
        };
        let aac = |kbps| {
            Variante::Transcode(Cible {
                format: Format::Aac,
                kbps,
            })
        };
        assert_eq!(
            decider(&flac, &navigateur, Qualite::Originale),
            Variante::Direct
        );
        assert_eq!(decider(&flac, &navigateur, Qualite::Normale), opus(160));
        assert_eq!(decider(&flac, &safari, Qualite::Originale), aac(256));
        assert_eq!(decider(&flac, &safari, Qualite::Basse), aac(96));
        assert_eq!(decider(&mp3, &safari, Qualite::Normale), Variante::Direct);
        assert_eq!(decider(&mp3, &safari, Qualite::Basse), aac(96));
        assert_eq!(
            decider(&alac, &safari, Qualite::Originale),
            Variante::Direct
        );
        assert_eq!(decider(&alac, &safari, Qualite::Haute), aac(256));
        assert_eq!(
            decider(&mp3, &Support::default(), Qualite::Originale),
            aac(256)
        );
        assert_eq!(famille("matroska,webm"), "webm");
        assert_eq!(famille("wav"), "wav");
        assert_eq!(famille("bizarre"), "autre");
        assert_eq!(mime_direct("ogg", "vorbis"), "audio/ogg; codecs=vorbis");
        assert_eq!(mime_direct("mov,mp4,m4a,3gp,3g2,mj2", "alac"), "audio/mp4");
        assert_eq!(codec_public("pcm_s24le"), "pcm");
        assert!(Support::lire(&json!({"codecs": "flac"})).is_err());
        assert!(Support::lire(&json!({"codecs": [1]})).is_err());
        assert!(Support::lire(&json!({"containers": vec!["ogg"; 33]})).is_err());
        assert_eq!(
            Support::lire(&json!({"codecs": ["FLAC"]})).unwrap().codecs,
            ["flac"]
        );
    }

    #[tokio::test]
    async fn flac_servi_tel_quel_ou_en_opus_puis_depuis_le_cache() {
        let Some(t) = banc(|_| {}).await else { return };
        t.b.synchro().await;
        let id = t.id("Lemonade", "Formation").await;
        let source = std::fs::read(t.b.chemin("Beyonce/Lemonade/01 Formation.flac")).unwrap();
        let r = t
            .lire(
                "alice",
                &id,
                json!({"codecs": ["flac", "opus"], "containers": ["flac", "ogg"], "quality": "originale"}),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let j = r.json();
        assert_eq!(j["mode"], "direct");
        assert_eq!(j["quality"], "originale");
        assert_eq!(
            j["format"],
            json!({"codec": "flac", "container": "flac", "mime": "audio/flac", "bitrate": j["format"]["bitrate"]})
        );
        assert_eq!(
            (&j["samples"], &j["sample_rate"], &j["encoder_delay"]),
            (&json!(88_200), &json!(44_100), &json!(0))
        );
        assert_eq!(j["duration"], json!(2.0));
        let mut sans_url = j.clone();
        sans_url["url"] = Value::Null;
        t.b.sans_fuite(&sans_url.to_string(), "lecture");
        let url = j["url"].as_str().unwrap();
        assert!(url.starts_with("/stream/"), "{url}");
        let r = t.flux(url, &[("range", "bytes=10-19")]).await;
        assert_eq!(r.statut, StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            r.entetes["content-range"],
            format!("bytes 10-19/{}", source.len()).as_str()
        );
        assert_eq!(r.entetes["content-type"], "audio/flac");
        assert_eq!(r.corps, source[10..20]);
        let r = t.flux(url, &[]).await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.entetes["accept-ranges"], "bytes");
        assert_eq!(r.corps, source);
        let r = t.flux(url, &[("range", "bytes=999999-")]).await;
        assert_eq!(r.statut, StatusCode::RANGE_NOT_SATISFIABLE);

        let mut corps = SANS_FLAC();
        corps["quality"] = json!("normale");
        let j = t.lire("alice", &id, corps.clone()).await.json();
        assert_eq!(j["mode"], "transcode", "{j}");
        assert_eq!(j["quality"], "normale");
        assert_eq!(
            j["format"],
            json!({"codec": "opus", "container": "ogg", "mime": "audio/ogg; codecs=opus", "bitrate": 160_000})
        );
        assert_eq!(
            (&j["samples"], &j["sample_rate"], &j["output_sample_rate"]),
            (&json!(88_200), &json!(44_100), &json!(48_000))
        );
        assert_eq!(t.transcodeur().lancements(), 1);
        let r = t.flux(j["url"].as_str().unwrap(), &[]).await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.entetes["content-type"], "audio/ogg; codecs=opus");
        let opus = r.corps;
        assert_eq!(codec_de(&opus, "opus").await.as_deref(), Some("opus"));
        assert_eq!(echantillons_decodes(&opus, "opus"), 96_000);
        attendre_async(
            || async { t.transcodeur().en_cours() == 0 },
            "fin de l'encodage",
        )
        .await;
        let en_cache = t.en_cache(".opus");
        assert_eq!(en_cache.len(), 1);
        assert_eq!(std::fs::read(&en_cache[0]).unwrap(), opus);

        let j = t.lire("alice", &id, corps).await.json();
        let r = t
            .flux(j["url"].as_str().unwrap(), &[("range", "bytes=0-99")])
            .await;
        assert_eq!(r.statut, StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.corps, opus[..100]);
        assert_eq!(t.transcodeur().lancements(), 1, "ffmpeg relancé");

        let j = t
            .lire(
                "alice",
                &id,
                json!({"codecs": ["mp3"], "containers": ["mp3"], "quality": "basse"}),
            )
            .await
            .json();
        assert_eq!(
            (
                &j["format"]["codec"],
                &j["format"]["bitrate"],
                &j["encoder_delay"]
            ),
            (&json!("aac"), &json!(96_000), &json!(1024))
        );
        assert_eq!(j["samples"], 88_200);
        let aac = t.flux(j["url"].as_str().unwrap(), &[]).await.corps;
        assert_eq!(codec_de(&aac, "aac").await.as_deref(), Some("aac"));
        assert!(echantillons_decodes(&aac, "aac") >= 88_200 + 1024);
        assert_eq!(t.transcodeur().lancements(), 2);

        let hold = t.id("Lemonade", "Hold Up").await;
        let mp3 = json!({"codecs": ["mp3"], "containers": ["mp3"], "quality": "haute"});
        assert_eq!(t.lire("alice", &hold, mp3).await.json()["mode"], "direct");

        for faux in [
            json!({"quality": "maximale"}),
            json!({"codecs": "flac"}),
            json!([]),
        ] {
            let r = t.lire("alice", &id, faux.clone()).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
        }
        assert_eq!(
            t.lire("alice", &Uuid::new_v4().to_string(), json!({}))
                .await
                .statut,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn synchro_sans_changement_garde_jetons_cache_et_mesures() {
        let Some(t) = banc(|_| {}).await else { return };
        t.b.synchro().await;
        let id = t.id("Lemonade", "Formation").await;
        let piste_id = Uuid::parse_str(&id).unwrap();
        let pool = t.b.e.state.pool().clone();
        let direct = t
            .lire(
                "alice",
                &id,
                json!({"codecs": ["flac"], "containers": ["flac"], "quality": "originale"}),
            )
            .await
            .json();
        let transcode = t.lire("alice", &id, SANS_FLAC()).await.json();
        assert_eq!(transcode["mode"], "transcode", "{transcode}");
        let r = t.flux(transcode["url"].as_str().unwrap(), &[]).await;
        assert_eq!(r.statut, StatusCode::OK);
        attendre_async(
            || async { t.transcodeur().en_cours() == 0 },
            "fin de l'encodage",
        )
        .await;
        assert_eq!(t.transcodeur().lancements(), 1);
        let avant = catalogue::fichier_de_piste(&pool, piste_id)
            .await
            .unwrap()
            .unwrap();
        let mesure = volume::lire(&pool, piste_id, avant.contenu_le)
            .await
            .unwrap()
            .expect("mesure enregistrée à la première lecture");

        t.b.synchro().await;

        let apres = catalogue::fichier_de_piste(&pool, piste_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((apres.id, apres.contenu_le), (avant.id, avant.contenu_le));
        let relue = volume::lire(&pool, piste_id, apres.contenu_le)
            .await
            .unwrap()
            .expect("mesure perdue après la synchronisation");
        assert_eq!(relue.niveau, mesure.niveau);
        for j in [&direct, &transcode] {
            let r = t.flux(j["url"].as_str().unwrap(), &[]).await;
            assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        }
        assert_eq!(t.transcodeur().lancements(), 1, "ffmpeg relancé");
        assert_eq!(t.en_cache(".opus").len(), 1);
        let encore = t.lire("alice", &id, SANS_FLAC()).await.json();
        for cle in ["loudness", "track", "track_peak"] {
            assert_eq!(encore["gain"][cle], transcode["gain"][cle], "{cle}");
        }
    }

    #[tokio::test]
    async fn url_de_flux_expiree_modifiee_ou_d_une_autre_piste_refusee() {
        let Some(t) = banc(|_| {}).await else { return };
        t.b.synchro().await;
        let formation = t.id("Lemonade", "Formation").await;
        let glosoli = t.id("Takk…", "Glósóli").await;
        let j = t
            .lire(
                "alice",
                &formation,
                json!({"codecs": ["flac"], "containers": ["flac"]}),
            )
            .await
            .json();
        let jeton = j["url"].as_str().unwrap().strip_prefix("/stream/").unwrap();
        let secret = t.b.e.state.config().secret_flux.clone();
        let billet = flux::valider(&secret, jeton, maintenant_unix()).unwrap();
        assert_eq!(billet.sub, "sub-alice");
        assert_eq!(billet.piste.to_string(), formation);
        let url = |b: &Billet| format!("/stream/{}", flux::signer(&secret, b));
        let refuse = |r: Rep| {
            assert_eq!(r.statut, StatusCode::UNAUTHORIZED, "{}", r.texte());
            assert_eq!(r.json()["error"], "stream_expired");
        };
        let expire = Billet {
            exp: maintenant_unix() - 1,
            ..billet.clone()
        };
        refuse(t.flux(&url(&expire), &[]).await);
        let autre_piste = Billet {
            piste: Uuid::parse_str(&glosoli).unwrap(),
            ..billet.clone()
        };
        let signe_autre = flux::signer(&secret, &autre_piste);
        let (corps_autre, _) = signe_autre.rsplit_once('.').unwrap();
        let (_, code) = jeton.rsplit_once('.').unwrap();
        refuse(t.flux(&format!("/stream/{corps_autre}.{code}"), &[]).await);
        refuse(
            t.flux(
                &format!("/stream/{}", flux::signer(&"x".repeat(32), &billet)),
                &[],
            )
            .await,
        );
        refuse(t.flux(&format!("/stream/{jeton}x"), &[]).await);
        assert_eq!(
            t.flux(&url(&autre_piste), &[]).await.statut,
            StatusCode::NOT_FOUND
        );
        let valide = format!("/stream/{jeton}");
        assert_eq!(t.flux(&valide, &[]).await.statut, StatusCode::OK);
        let r = t.flux(&valide, &[("authorization", "Bearer alice")]).await;
        assert_eq!(r.statut, StatusCode::OK);
        let r = t.flux(&valide, &[("authorization", "Bearer bruno")]).await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
        let r = t.flux(&valide, &[("origin", "https://ailleurs.fr")]).await;
        assert_eq!(r.entetes["access-control-allow-origin"], "*");
        assert!(r.entetes.get("access-control-allow-credentials").is_none());
        let r =
            t.b.e
                .appel_avec(
                    None,
                    "OPTIONS",
                    &valide,
                    None,
                    &[("origin", "https://ailleurs.fr")],
                )
                .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(r.entetes["access-control-allow-headers"], "Range");
        let sans_acces = Billet {
            sub: "sub-personne".into(),
            ..billet
        };
        let r = t.flux(&url(&sans_acces), &[]).await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
        assert!(r.texte().contains("sillon:access"));
    }

    #[tokio::test]
    async fn gains_ramenent_deux_niveaux_a_la_cible() {
        let Some(t) = banc(|_| {}).await else { return };
        generer(
            &t.b.chemin("Sigur Ros/Takk/01 Glosoli.flac"),
            &["-af", "volume=-12dB", "-c:a", "flac"],
        );
        t.b.synchro().await;
        let formation = t.id("Lemonade", "Formation").await;
        let glosoli = t.id("Takk…", "Glósóli").await;
        let flac = json!({"codecs": ["flac"], "containers": ["flac"]});
        let a = t.lire("alice", &formation, flac.clone()).await.json();
        let b = t.lire("alice", &glosoli, flac.clone()).await.json();
        let (la, lb) = (
            a["gain"]["loudness"].as_f64().unwrap(),
            b["gain"]["loudness"].as_f64().unwrap(),
        );
        assert!((la - lb - 12.0).abs() < 0.5, "{a} {b}");
        assert_eq!(a["gain"]["reference"], volume::CIBLE);
        let dossier = tempfile::tempdir().unwrap();
        let mut niveaux = Vec::new();
        for (j, source) in [
            (&a, "Beyonce/Lemonade/01 Formation.flac"),
            (&b, "Sigur Ros/Takk/01 Glosoli.flac"),
        ] {
            let gain = j["gain"]["track"].as_f64().unwrap();
            assert!(j["gain"]["track_peak"].as_f64().unwrap() < 0.0);
            let sortie = dossier.path().join(format!("{gain}.flac"));
            let ok = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
                .arg(t.b.chemin(source))
                .args(["-af", &format!("volume={gain}dB"), "-c:a", "flac"])
                .arg(&sortie)
                .status()
                .unwrap()
                .success();
            assert!(ok);
            let m = volume::mesurer("ffmpeg", CheminFs::new(&sortie))
                .await
                .unwrap();
            assert!((m.niveau - volume::CIBLE).abs() <= 0.5, "{m:?}");
            niveaux.push(m.niveau);
        }
        assert!((niveaux[0] - niveaux[1]).abs() <= 0.5, "{niveaux:?}");
        assert_eq!(b["gain"]["album"], b["gain"]["track"]);
        let pool = t.b.e.state.pool().clone();
        let id = Uuid::parse_str(&formation).unwrap();
        attendre_async(
            || {
                let pool = pool.clone();
                async move {
                    volume::non_mesurees_de_l_album(&pool, id)
                        .await
                        .unwrap()
                        .is_empty()
                }
            },
            "mesure des pistes de l'album",
        )
        .await;
        let mesurees: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM mesures m JOIN pistes p ON p.id = m.piste_id JOIN albums a ON a.id = p.album_id WHERE a.titre = 'Lemonade'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(mesurees, 3);
        let a = t.lire("alice", &formation, flac).await.json();
        let album = a["gain"]["album"].as_f64().unwrap();
        assert!(
            (album - a["gain"]["track"].as_f64().unwrap()).abs() < 3.0,
            "{a}"
        );
        assert!(
            a["gain"]["album_peak"].as_f64().unwrap() >= a["gain"]["track_peak"].as_f64().unwrap()
        );
    }

    fn faux_ffmpeg(dossier: &CheminFs, cas: &str) -> String {
        let chemin = dossier.join("ffmpeg");
        std::fs::write(
            &chemin,
            format!("#!/bin/sh\ncase \"$*\" in\n  *libopus*) {cas};;\nesac\nexec ffmpeg \"$@\"\n"),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        chemin.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn au_dela_du_plafond_erreur_explicite_et_flux_complet_pendant_l_encodage() {
        let outils = tempfile::tempdir().unwrap();
        let barriere = outils.path().join("go");
        let ffmpeg = faux_ffmpeg(
            outils.path(),
            &format!(
                "while [ ! -e '{}' ]; do sleep 0.05; done",
                barriere.display()
            ),
        );
        let Some(t) = banc(move |c| {
            c.ffmpeg = ffmpeg;
            c.max_transcodages = 1;
        })
        .await
        else {
            return;
        };
        t.b.synchro().await;
        let formation = t.id("Lemonade", "Formation").await;
        let glosoli = t.id("Takk…", "Glósóli").await;
        let j = t.lire("alice", &formation, SANS_FLAC()).await.json();
        assert_eq!(j["mode"], "transcode", "{j}");
        assert_eq!(t.transcodeur().en_cours(), 1);
        let r = t.lire("alice", &glosoli, SANS_FLAC()).await;
        assert_eq!(r.statut, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(r.json()["error"], "server_busy");
        assert_eq!(r.entetes["retry-after"], "5");
        let app = t.b.e.app.clone();
        let url = j["url"].as_str().unwrap().to_string();
        let lecture =
            tokio::spawn(async move { appel_sur(&app, None, "GET", &url, None, &[]).await });
        attendre_async(
            || async { t.transcodeur().lecteurs() == 1 },
            "flux ouvert pendant l'encodage",
        )
        .await;
        std::fs::write(&barriere, b"").unwrap();
        let r = lecture.await.unwrap();
        assert_eq!(r.statut, StatusCode::OK);
        assert!(r.entetes.get("content-length").is_none());
        assert_eq!(r.entetes["accept-ranges"], "none");
        let en_cache = t.en_cache(".opus");
        assert_eq!(en_cache.len(), 1);
        assert_eq!(std::fs::read(&en_cache[0]).unwrap(), r.corps);
        assert_eq!(echantillons_decodes(&r.corps, "opus"), 96_000);
        let r = t.lire("alice", &glosoli, SANS_FLAC()).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    }

    #[tokio::test]
    async fn encodage_en_echec_jamais_servi_comme_complet() {
        let outils = tempfile::tempdir().unwrap();
        let ffmpeg = faux_ffmpeg(
            outils.path(),
            "for a in \"$@\"; do d=$a; done; printf OggS > \"$d\"; exit 1",
        );
        let Some(t) = banc(move |c| c.ffmpeg = ffmpeg).await else {
            return;
        };
        t.b.synchro().await;
        let formation = t.id("Lemonade", "Formation").await;
        let j = t.lire("alice", &formation, SANS_FLAC()).await.json();
        let req = axum::http::Request::builder()
            .uri(j["url"].as_str().unwrap())
            .body(Body::empty())
            .unwrap();
        let r = t.b.e.app.clone().oneshot(req).await.unwrap();
        let statut = r.status();
        let corps = to_bytes(r.into_body(), usize::MAX).await;
        assert!(
            !(statut == StatusCode::OK && corps.is_ok()),
            "flux tronqué servi comme complet : {statut}"
        );
        attendre_async(
            || async { t.transcodeur().en_cours() == 0 },
            "fin des essais",
        )
        .await;
        assert!(t.en_cache(".opus").is_empty() && t.en_cache(".part").is_empty());
    }

    #[tokio::test]
    async fn preference_de_qualite_par_personne() {
        let Some(t) = banc(|_| {}).await else { return };
        t.b.synchro().await;
        let e = &t.b.e;
        let r = e.get("alice", "/preferences").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(
            r.json(),
            json!({"quality": "originale", "qualities": ["originale", "haute", "normale", "basse"]})
        );
        let r = e
            .appel(
                Some("alice"),
                "PUT",
                "/preferences",
                Some(json!({"quality": "basse"})),
            )
            .await;
        assert_eq!(r.json()["quality"], "basse");
        assert_eq!(
            e.get("alice", "/preferences").await.json()["quality"],
            "basse"
        );
        assert_eq!(
            e.get("bruno", "/preferences").await.json()["quality"],
            "originale"
        );
        for faux in [json!({"quality": "max"}), json!({}), json!("basse")] {
            let r = e
                .appel(Some("alice"), "PUT", "/preferences", Some(faux.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
        }
        let id = t.id("Lemonade", "Formation").await;
        let j = t.lire("alice", &id, SANS_FLAC()).await.json();
        assert_eq!(
            (&j["quality"], &j["format"]["bitrate"]),
            (&json!("basse"), &json!(96_000))
        );
        let j = t.lire("bruno", &id, SANS_FLAC()).await.json();
        assert_eq!(
            (&j["quality"], &j["format"]["bitrate"]),
            (&json!("haute"), &json!(256_000))
        );
    }
}
