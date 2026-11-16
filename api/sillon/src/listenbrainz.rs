use std::{
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use tokio::sync::Mutex;

use crate::{ajouts::mbid_valide, etat::AppState};

pub const ALGORITHME: &str =
    "session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30";
pub const SIMILAIRES_MAX: usize = 50;
pub const POPULAIRES_MAX: usize = 20;
pub const PAUSE_PANNE: i64 = 300;
const TEXTE_MAX: usize = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reglages {
    pub url: String,
    pub labs: String,
    pub intervalle: Duration,
    pub fraicheur: chrono::Duration,
}

impl Reglages {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let nombre = |nom: &str, defaut: u64| match env(nom) {
            None => Ok(defaut),
            Some(v) => v
                .parse::<u64>()
                .map_err(|_| format!("{nom} invalide : {v}")),
        };
        let url =
            env("SILLON_LISTENBRAINZ_URL").unwrap_or_else(|| "https://api.listenbrainz.org".into());
        let intervalle = nombre("SILLON_LISTENBRAINZ_INTERVALLE_MS", 1000)?;
        let jours = nombre("SILLON_SIMILARITE_JOURS", 30)?;
        if url == "off" {
            return Ok(None);
        }
        Ok(Some(Self {
            url: url.trim_end_matches('/').to_string(),
            labs: env("SILLON_LISTENBRAINZ_LABS_URL")
                .unwrap_or_else(|| "https://labs.api.listenbrainz.org".into())
                .trim_end_matches('/')
                .to_string(),
            intervalle: Duration::from_millis(intervalle),
            fraicheur: chrono::Duration::days(i64::try_from(jours).unwrap_or(30)),
        }))
    }
}

pub struct Etat {
    prochain: Mutex<Instant>,
    panne: AtomicI64,
}

impl Default for Etat {
    fn default() -> Self {
        Self {
            prochain: Mutex::new(Instant::now()),
            panne: AtomicI64::new(0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Similaire {
    pub mbid: String,
    pub nom: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Populaire {
    pub mbid: String,
    pub titre: String,
    pub album_mbid: Option<String>,
    pub album_titre: Option<String>,
    pub ecoutes: i64,
}

fn objets<'a>(v: &'a Value, cle: &str, sortie: &mut Vec<&'a Value>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| objets(x, cle, sortie)),
        Value::Object(o) if o.get(cle).is_some_and(Value::is_string) => sortie.push(v),
        Value::Object(o) => o.values().for_each(|x| objets(x, cle, sortie)),
        _ => {}
    }
}

fn texte(v: &Value, cles: &[&str]) -> Option<String> {
    cles.iter()
        .filter_map(|c| v[*c].as_str())
        .map(str::trim)
        .find(|t| !t.is_empty())
        .map(|t| t.chars().take(TEXTE_MAX).collect())
}

fn nombre(v: &Value, cles: &[&str]) -> Option<f64> {
    cles.iter()
        .filter_map(|c| v[*c].as_f64().or_else(|| v[*c].as_str()?.parse().ok()))
        .find(|n| n.is_finite())
}

pub fn lire_similaires(v: &Value, source: &str) -> Vec<Similaire> {
    let mut bruts = Vec::new();
    objets(v, "artist_mbid", &mut bruts);
    let mut l: Vec<Similaire> = bruts
        .into_iter()
        .filter_map(|o| {
            Some(Similaire {
                mbid: mbid_valide(o["artist_mbid"].as_str()?)?,
                nom: texte(o, &["name", "artist_name"]).unwrap_or_default(),
                score: nombre(o, &["score"]).filter(|s| *s > 0.0)? as f32,
            })
        })
        .filter(|s| s.mbid != source)
        .collect();
    l.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.mbid.cmp(&b.mbid)));
    l.dedup_by(|b, a| a.mbid == b.mbid);
    l.truncate(SIMILAIRES_MAX);
    let max = l.first().map_or(1.0, |s| s.score);
    for s in &mut l {
        s.score = (s.score / max).clamp(0.0, 1.0);
    }
    l
}

pub fn lire_populaires(v: &Value) -> Vec<Populaire> {
    let mut bruts = Vec::new();
    objets(v, "recording_mbid", &mut bruts);
    let mut l: Vec<Populaire> = bruts
        .into_iter()
        .filter_map(|o| {
            Some(Populaire {
                mbid: mbid_valide(o["recording_mbid"].as_str()?)?,
                titre: texte(o, &["recording_name", "title", "name"])?,
                album_mbid: texte(o, &["release_group_mbid"]).and_then(|m| mbid_valide(&m)),
                album_titre: texte(o, &["release_name", "release_group_name"]),
                ecoutes: nombre(o, &["total_listen_count", "listen_count"]).unwrap_or(0.0) as i64,
            })
        })
        .collect();
    l.sort_by_key(|p| std::cmp::Reverse(p.ecoutes));
    let mut vus = std::collections::HashSet::new();
    l.retain(|p| vus.insert(p.mbid.clone()));
    l.truncate(POPULAIRES_MAX);
    l
}

async fn appeler(state: &AppState, url: String, query: &[(&str, &str)]) -> Result<Value, String> {
    let (Some(r), e) = (&state.config().listenbrainz, state.listenbrainz()) else {
        return Err("ListenBrainz désactivé".into());
    };
    if Utc::now().timestamp() < e.panne.load(Ordering::SeqCst) {
        return Err("ListenBrainz en pause après une erreur".into());
    }
    {
        let mut prochain = e.prochain.lock().await;
        tokio::time::sleep_until((*prochain).into()).await;
        *prochain = Instant::now() + r.intervalle;
    }
    let resultat = async {
        let rep = state
            .http()
            .get(url)
            .query(query)
            .header("user-agent", concat!("sillon/", env!("CARGO_PKG_VERSION")))
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if !rep.status().is_success() {
            return Err(format!("HTTP {}", rep.status().as_u16()));
        }
        rep.json::<Value>()
            .await
            .map_err(|e| format!("réponse illisible : {}", e.without_url()))
    }
    .await;
    if resultat.is_err() {
        e.panne
            .store(Utc::now().timestamp() + PAUSE_PANNE, Ordering::SeqCst);
    }
    resultat.map_err(|e| format!("ListenBrainz : {e}"))
}

async fn frais(state: &AppState, genre: &str, mbid: &str) -> Result<bool, sqlx::Error> {
    let Some(r) = &state.config().listenbrainz else {
        return Ok(true);
    };
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM similarites_releves WHERE genre = $1 AND mbid = $2 AND releve_le > $3)",
    )
    .bind(genre)
    .bind(mbid)
    .bind(Utc::now() - r.fraicheur)
    .fetch_one(state.pool())
    .await
}

async fn noter_releve(
    tx: &mut sqlx::PgConnection,
    genre: &str,
    mbid: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO similarites_releves (genre, mbid) VALUES ($1, $2)
         ON CONFLICT (genre, mbid) DO UPDATE SET releve_le = now()",
    )
    .bind(genre)
    .bind(mbid)
    .execute(tx)
    .await?;
    Ok(())
}

async fn rafraichir_similaires(state: &AppState, mbid: &str) -> Result<(), String> {
    let Some(r) = &state.config().listenbrainz else {
        return Ok(());
    };
    let v = appeler(
        state,
        format!("{}/similar-artists/json", r.labs),
        &[("artist_mbids", mbid), ("algorithm", ALGORITHME)],
    )
    .await?;
    let l = lire_similaires(&v, mbid);
    let base = |e: sqlx::Error| format!("base : {e}");
    let mut tx = state.pool().begin().await.map_err(base)?;
    sqlx::query("DELETE FROM artistes_similaires WHERE mbid = $1")
        .bind(mbid)
        .execute(&mut *tx)
        .await
        .map_err(base)?;
    sqlx::query(
        "INSERT INTO artistes_similaires (mbid, similaire, nom, score)
         SELECT $1, * FROM unnest($2::text[], $3::text[], $4::real[])",
    )
    .bind(mbid)
    .bind(l.iter().map(|s| s.mbid.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|s| s.nom.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|s| s.score).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await
    .map_err(base)?;
    noter_releve(&mut tx, "similaires", mbid)
        .await
        .map_err(base)?;
    tx.commit().await.map_err(base)
}

pub async fn similaires(state: &AppState, mbid: &str) -> Result<Vec<Similaire>, sqlx::Error> {
    if mbid_valide(mbid).is_some()
        && !frais(state, "similaires", mbid).await?
        && let Err(e) = rafraichir_similaires(state, mbid).await
    {
        tracing::warn!("artistes similaires : {e}");
    }
    let lignes = sqlx::query(
        "SELECT similaire, nom, score FROM artistes_similaires WHERE mbid = $1 ORDER BY score DESC, similaire",
    )
    .bind(mbid)
    .fetch_all(state.pool())
    .await?;
    Ok(lignes
        .iter()
        .map(|l| Similaire {
            mbid: l.get(0),
            nom: l.get(1),
            score: l.get(2),
        })
        .collect())
}

async fn rafraichir_populaires(state: &AppState, mbid: &str) -> Result<(), String> {
    let Some(r) = &state.config().listenbrainz else {
        return Ok(());
    };
    let v = appeler(
        state,
        format!("{}/1/popularity/top-recordings-for-artist/{mbid}", r.url),
        &[],
    )
    .await?;
    let l = lire_populaires(&v);
    let base = |e: sqlx::Error| format!("base : {e}");
    let mut tx = state.pool().begin().await.map_err(base)?;
    sqlx::query("DELETE FROM enregistrements_populaires WHERE artiste = $1")
        .bind(mbid)
        .execute(&mut *tx)
        .await
        .map_err(base)?;
    sqlx::query(
        "INSERT INTO enregistrements_populaires (artiste, rang, mbid, titre, album_mbid, album_titre, ecoutes)
         SELECT $1, r::int, m, t, am, at, e FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::bigint[])
           WITH ORDINALITY AS x(m, t, am, at, e, r)",
    )
    .bind(mbid)
    .bind(l.iter().map(|p| p.mbid.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|p| p.titre.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|p| p.album_mbid.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|p| p.album_titre.clone()).collect::<Vec<_>>())
    .bind(l.iter().map(|p| p.ecoutes).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await
    .map_err(base)?;
    noter_releve(&mut tx, "populaires", mbid)
        .await
        .map_err(base)?;
    tx.commit().await.map_err(base)
}

pub async fn populaires(state: &AppState, mbid: &str) -> Result<Vec<Populaire>, sqlx::Error> {
    if mbid_valide(mbid).is_some()
        && !frais(state, "populaires", mbid).await?
        && let Err(e) = rafraichir_populaires(state, mbid).await
    {
        tracing::warn!("enregistrements populaires : {e}");
    }
    let lignes = sqlx::query(
        "SELECT mbid, titre, album_mbid, album_titre, ecoutes FROM enregistrements_populaires WHERE artiste = $1 ORDER BY rang",
    )
    .bind(mbid)
    .fetch_all(state.pool())
    .await?;
    Ok(lignes
        .iter()
        .map(|l| Populaire {
            mbid: l.get(0),
            titre: l.get(1),
            album_mbid: l.get(2),
            album_titre: l.get(3),
            ecoutes: l.get(4),
        })
        .collect())
}
