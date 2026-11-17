use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    etat::AppState,
    garde::Personne,
    http::{self, bd, erreur, horodatage, indisponible, invalide, ok},
    lidarr,
    quota::{self, ETATS_ACTIFS, Occupation},
};

pub const PERIODE_SUIVI: Duration = Duration::from_secs(300);
const VERROU: i64 = 0x5111_0005;
const RESULTATS_MAX: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    Album,
    Artiste,
}

impl Genre {
    pub fn code(self) -> &'static str {
        match self {
            Genre::Album => "album",
            Genre::Artiste => "artiste",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origine {
    Manuel,
    Decouverte,
}

impl Origine {
    pub fn code(self) -> &'static str {
        match self {
            Origine::Manuel => "manuel",
            Origine::Decouverte => "decouverte",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auteur {
    pub sub: String,
    pub nom: String,
}

impl From<&Personne> for Auteur {
    fn from(p: &Personne) -> Self {
        Self {
            sub: p.identite.sub.clone(),
            nom: p.identite.nom.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemandeAjout {
    pub mbid: String,
    pub personne: Auteur,
    pub origine: Origine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refus {
    DisquePlein {
        occupation: Occupation,
        estimation: i64,
    },
    LimiteJournaliere {
        limite: u32,
    },
    Exclu,
    BudgetDecouverte {
        utilise: i64,
        budget: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resultat {
    Envoye(Uuid),
    DejaPresent,
    EnCours(Uuid),
    Refuse(Refus),
}

#[derive(Debug)]
pub enum ErreurAjout {
    Invalide(&'static str),
    Introuvable,
    Etat(&'static str),
    NonConfigure,
    Incomplet(&'static str),
    Refus(String),
    Lidarr(String),
    Base(sqlx::Error),
}

impl ErreurAjout {
    fn de_lidarr(e: String) -> Self {
        if e.starts_with(lidarr::REFUS) {
            ErreurAjout::Refus(e)
        } else {
            ErreurAjout::Lidarr(e)
        }
    }
}

impl From<lidarr::ErreurReglages> for ErreurAjout {
    fn from(e: lidarr::ErreurReglages) -> Self {
        match e {
            lidarr::ErreurReglages::Incomplet(m) => ErreurAjout::Incomplet(m),
            lidarr::ErreurReglages::Lidarr(e) => ErreurAjout::Lidarr(e),
        }
    }
}

impl From<sqlx::Error> for ErreurAjout {
    fn from(e: sqlx::Error) -> Self {
        ErreurAjout::Base(e)
    }
}

impl From<String> for ErreurAjout {
    fn from(e: String) -> Self {
        ErreurAjout::de_lidarr(e)
    }
}

struct Cible {
    titre: String,
    artiste_mbid: String,
    artiste_nom: String,
    albums: Vec<String>,
    estimation: i64,
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v[cle]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn secondaires(a: &Value) -> Vec<String> {
    a["secondaryTypes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str().or_else(|| t["name"].as_str()))
        .map(String::from)
        .collect()
}

pub fn est_studio(a: &Value) -> bool {
    a["albumType"]
        .as_str()
        .is_some_and(|t| t.eq_ignore_ascii_case("album"))
        && secondaires(a).is_empty()
}

pub(crate) fn duree_album(a: &Value) -> f64 {
    let ms = a["duration"].as_i64().filter(|d| *d > 0).or_else(|| {
        a["releases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["duration"].as_i64())
            .max()
    });
    ms.map_or(0.0, |ms| ms as f64 / 1000.0)
}

fn annee(a: &Value) -> Option<i32> {
    a["releaseDate"]
        .as_str()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
}

pub fn mbid_valide(m: &str) -> Option<String> {
    Uuid::parse_str(m.trim()).ok().map(|u| u.to_string())
}

pub(crate) fn client(state: &AppState) -> Option<lidarr::Client<'_>> {
    state
        .config()
        .lidarr
        .as_ref()
        .map(|l| lidarr::Client::new(l, state.http()))
}

pub(crate) async fn chercher(
    c: &lidarr::Client<'_>,
    chemin: &str,
    terme: &str,
) -> Result<Vec<Value>, String> {
    match c
        .envoyer(c.requete(Method::GET, chemin).query(&[("term", terme)]))
        .await?
    {
        Value::Array(a) => Ok(a),
        _ => Err(format!("Lidarr : {chemin} : liste attendue")),
    }
}

fn enregistrements(v: Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a,
        mut v => v["records"].take().as_array().cloned().unwrap_or_default(),
    }
}

pub async fn est_exclu(pool: &PgPool, mbids: &[&str]) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM exclusions WHERE mbid = ANY($1))")
        .bind(mbids)
        .fetch_one(pool)
        .await
}

async fn presents(
    pool: &PgPool,
    table: &str,
    mbids: &[String],
) -> Result<HashSet<String>, sqlx::Error> {
    let l: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT mbid FROM {table} WHERE present AND mbid = ANY($1)"
    )))
    .bind(mbids)
    .fetch_all(pool)
    .await?;
    Ok(l.into_iter().collect())
}

async fn en_cours_parmi(pool: &PgPool, mbids: &[String]) -> Result<HashSet<String>, sqlx::Error> {
    let l: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT DISTINCT m FROM ajouts a, unnest(array_append(a.albums, a.mbid)) m
         WHERE a.etat IN {ETATS_ACTIFS} AND m = ANY($1)"
    )))
    .bind(mbids)
    .fetch_all(pool)
    .await?;
    Ok(l.into_iter().collect())
}

async fn exclus_parmi(pool: &PgPool, mbids: &[String]) -> Result<HashSet<String>, sqlx::Error> {
    let l: Vec<String> = sqlx::query_scalar("SELECT mbid FROM exclusions WHERE mbid = ANY($1)")
        .bind(mbids)
        .fetch_all(pool)
        .await?;
    Ok(l.into_iter().collect())
}

async fn actif(pool: &PgPool, genre: Genre, mbid: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM ajouts WHERE type = $1 AND mbid = $2 AND etat IN {ETATS_ACTIFS}"
    )))
    .bind(genre.code())
    .bind(mbid)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn reglages(
    state: &AppState,
    c: &lidarr::Client<'_>,
) -> Result<lidarr::ReglagesAjout, lidarr::ErreurReglages> {
    c.reglages_ajout(&state.config().ajouts).await
}

async fn cible(
    state: &AppState,
    c: &lidarr::Client<'_>,
    genre: Genre,
    mbid: &str,
) -> Result<Option<Cible>, ErreurAjout> {
    let debit = reglages(state, c).await?.debit;
    let terme = format!("lidarr:{mbid}");
    match genre {
        Genre::Album => {
            let l = chercher(c, "/album/lookup", &terme).await?;
            let a = l
                .iter()
                .find(|a| a["foreignAlbumId"].as_str() == Some(mbid))
                .ok_or(ErreurAjout::Introuvable)?;
            Ok(Some(Cible {
                titre: texte(a, "title").unwrap_or_else(|| "Sans titre".into()),
                artiste_mbid: texte(&a["artist"], "foreignArtistId")
                    .and_then(|m| mbid_valide(&m))
                    .ok_or(ErreurAjout::Introuvable)?,
                artiste_nom: texte(&a["artist"], "artistName").unwrap_or_default(),
                albums: vec![mbid.to_string()],
                estimation: quota::estimer(duree_album(a), debit),
            }))
        }
        Genre::Artiste => {
            let l = chercher(c, "/artist/lookup", &terme).await?;
            let a = l
                .iter()
                .find(|a| a["foreignArtistId"].as_str() == Some(mbid))
                .ok_or(ErreurAjout::Introuvable)?;
            let nom = texte(a, "artistName").unwrap_or_default();
            let studio: Vec<(String, f64)> = if nom.is_empty() {
                Vec::new()
            } else {
                chercher(c, "/album/lookup", &nom)
                    .await?
                    .iter()
                    .filter(|al| {
                        al["artist"]["foreignArtistId"].as_str() == Some(mbid) && est_studio(al)
                    })
                    .filter_map(|al| Some((texte(al, "foreignAlbumId")?, duree_album(al))))
                    .collect()
            };
            let mbids: Vec<String> = studio.iter().map(|s| s.0.clone()).collect();
            let deja = presents(state.pool(), "albums", &mbids).await?;
            if !studio.is_empty() && deja.len() >= studio.len() {
                return Ok(None);
            }
            let manquants: Vec<f64> = studio
                .iter()
                .filter(|s| !deja.contains(&s.0))
                .map(|s| s.1)
                .collect();
            let estimation = if manquants.is_empty() {
                quota::estimer(0.0, debit)
            } else {
                manquants.iter().map(|d| quota::estimer(*d, debit)).sum()
            };
            Ok(Some(Cible {
                titre: nom.clone(),
                artiste_mbid: mbid.to_string(),
                artiste_nom: nom,
                albums: Vec::new(),
                estimation,
            }))
        }
    }
}

async fn reserver(
    state: &AppState,
    genre: Genre,
    mbid: &str,
    c: &Cible,
    d: &DemandeAjout,
) -> Result<Result<Uuid, Resultat>, sqlx::Error> {
    let mut tx = state.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(VERROU)
        .execute(&mut *tx)
        .await?;
    let reglages = &state.config().ajouts;
    let (n, limite) = match d.origine {
        Origine::Manuel => (
            sqlx::query_scalar(
                "SELECT count(*) FROM ajouts WHERE sub = $1 AND origine = 'manuel' AND promu_le IS NULL
                   AND cree_le > now() - interval '1 day'",
            )
            .bind(&d.personne.sub)
            .fetch_one(&mut *tx)
            .await?,
            reglages.par_jour,
        ),
        Origine::Decouverte => (
            quota::decouvertes_du_jour(&mut tx, &d.personne.sub).await?,
            reglages.decouverte_par_jour,
        ),
    };
    if n >= i64::from(limite) {
        return Ok(Err(Resultat::Refuse(Refus::LimiteJournaliere { limite })));
    }
    if d.origine == Origine::Decouverte {
        let utilise = quota::budget_decouverte(&mut tx).await?;
        let budget = i64::try_from(reglages.decouverte_budget).unwrap_or(i64::MAX);
        if utilise.saturating_add(c.estimation) > budget {
            return Ok(Err(Resultat::Refuse(Refus::BudgetDecouverte {
                utilise,
                budget,
            })));
        }
    }
    let occupation = quota::occupation_sur(state, &mut tx).await?;
    if occupation.depasse(c.estimation) {
        return Ok(Err(Resultat::Refuse(Refus::DisquePlein {
            occupation,
            estimation: c.estimation,
        })));
    }
    let id: Option<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO ajouts (type, mbid, titre, artiste_mbid, artiste_nom, origine, sub, nom, estimation, albums)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (type, mbid) WHERE etat IN {ETATS_ACTIFS} DO NOTHING RETURNING id"
    )))
    .bind(genre.code())
    .bind(mbid)
    .bind(&c.titre)
    .bind(&c.artiste_mbid)
    .bind(&c.artiste_nom)
    .bind(d.origine.code())
    .bind(&d.personne.sub)
    .bind(&d.personne.nom)
    .bind(c.estimation)
    .bind(&c.albums)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(match id {
        Some(id) => Ok(id),
        None => Err(Resultat::EnCours(
            actif(state.pool(), genre, mbid).await?.unwrap_or_default(),
        )),
    })
}

pub async fn ajouter(
    state: &AppState,
    genre: Genre,
    d: DemandeAjout,
) -> Result<Resultat, ErreurAjout> {
    let mbid = mbid_valide(&d.mbid).ok_or(ErreurAjout::Invalide("mbid invalide"))?;
    let c = client(state).ok_or(ErreurAjout::NonConfigure)?;
    let pool = state.pool();
    if est_exclu(pool, &[&mbid]).await? {
        return Ok(Resultat::Refuse(Refus::Exclu));
    }
    if genre == Genre::Album
        && !presents(pool, "albums", std::slice::from_ref(&mbid))
            .await?
            .is_empty()
    {
        return Ok(Resultat::DejaPresent);
    }
    if let Some(id) = actif(pool, genre, &mbid).await? {
        return Ok(Resultat::EnCours(id));
    }
    let Some(cible) = cible(state, &c, genre, &mbid).await? else {
        return Ok(Resultat::DejaPresent);
    };
    if est_exclu(pool, &[&cible.artiste_mbid]).await? {
        return Ok(Resultat::Refuse(Refus::Exclu));
    }
    let id = match reserver(state, genre, &mbid, &cible, &d).await? {
        Ok(id) => id,
        Err(r) => return Ok(r),
    };
    match envoyer(state, id).await {
        Ok(_) => Ok(Resultat::Envoye(id)),
        Err(e) if e.starts_with(lidarr::REFUS) => {
            sqlx::query("UPDATE ajouts SET etat = 'echec', raison = $2, maj = now() WHERE id = $1")
                .bind(id)
                .bind(&e)
                .execute(pool)
                .await?;
            Err(ErreurAjout::Refus(e))
        }
        Err(e) => {
            sqlx::query("DELETE FROM ajouts WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?;
            Err(ErreurAjout::Lidarr(e))
        }
    }
}

pub async fn ajouter_album(state: &AppState, d: DemandeAjout) -> Result<Resultat, ErreurAjout> {
    ajouter(state, Genre::Album, d).await
}

pub async fn ajouter_artiste(state: &AppState, d: DemandeAjout) -> Result<Resultat, ErreurAjout> {
    ajouter(state, Genre::Artiste, d).await
}

pub async fn relancer(state: &AppState, id: Uuid) -> Result<Resultat, ErreurAjout> {
    let mut tx = state.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(VERROU)
        .execute(&mut *tx)
        .await?;
    let l = sqlx::query("SELECT etat, estimation FROM ajouts WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ErreurAjout::Introuvable)?;
    if l.get::<String, _>(0) != "echec" {
        return Err(ErreurAjout::Etat("seul un ajout en échec se relance"));
    }
    let estimation: i64 = l.get(1);
    let occupation = quota::occupation_sur(state, &mut tx).await?;
    if occupation.depasse(estimation) {
        return Ok(Resultat::Refuse(Refus::DisquePlein {
            occupation,
            estimation,
        }));
    }
    let r = sqlx::query(
        "UPDATE ajouts SET etat = 'envoye', raison = NULL, surveille = false, envoye_le = now(), maj = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await;
    match r {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ErreurAjout::Etat(
                "un ajout du même titre est déjà en cours",
            ));
        }
        r => {
            r?;
        }
    }
    tx.commit().await?;
    if let Err(e) = envoyer(state, id).await {
        sqlx::query("UPDATE ajouts SET etat = 'echec', raison = $2, maj = now() WHERE id = $1")
            .bind(id)
            .bind(&e)
            .execute(state.pool())
            .await?;
        return Err(ErreurAjout::de_lidarr(e));
    }
    Ok(Resultat::Envoye(id))
}

fn base(e: sqlx::Error) -> String {
    format!("base : {e}")
}

pub async fn envoyer(state: &AppState, id: Uuid) -> Result<bool, String> {
    let c = client(state).ok_or("Lidarr non configuré")?;
    let l = sqlx::query("SELECT type, mbid, artiste_mbid FROM ajouts WHERE id = $1")
        .bind(id)
        .fetch_one(state.pool())
        .await
        .map_err(base)?;
    let (genre, mbid, artiste_mbid): (String, String, String) = (l.get(0), l.get(1), l.get(2));
    let existant = c
        .liste("/artist")
        .await?
        .iter()
        .find(|a| a["foreignArtistId"].as_str() == Some(&artiste_mbid))
        .and_then(|a| a["id"].as_i64());
    let artiste_id = match existant {
        Some(id) => id,
        None => {
            let mut a = chercher(&c, "/artist/lookup", &format!("lidarr:{artiste_mbid}"))
                .await?
                .into_iter()
                .find(|a| a["foreignArtistId"].as_str() == Some(&artiste_mbid))
                .ok_or("Lidarr : artiste introuvable")?;
            let a_surveiller: Vec<&str> = if genre == "album" {
                vec![&mbid]
            } else {
                vec![]
            };
            let r = reglages(state, &c).await?;
            a["qualityProfileId"] = json!(r.qualite.id);
            a["metadataProfileId"] = json!(r.metadonnees.id);
            a["rootFolderPath"] = json!(r.racine);
            a["tags"] = json!(r.tags);
            a["monitored"] = json!(true);
            a["monitorNewItems"] = json!("none");
            a["addOptions"] = json!({
                "monitor": "none",
                "albumsToMonitor": a_surveiller,
                "searchForMissingAlbums": false,
            });
            c.envoyer(c.requete(Method::POST, "/artist").json(&a))
                .await?["id"]
                .as_i64()
                .ok_or("Lidarr : artiste non créé")?
        }
    };
    let albums = c.liste(&format!("/album?artistId={artiste_id}")).await?;
    let cibles: Vec<&Value> = albums
        .iter()
        .filter(|a| {
            if genre == "album" {
                a["foreignAlbumId"].as_str() == Some(&mbid)
            } else {
                est_studio(a)
            }
        })
        .collect();
    let ids: Vec<i64> = cibles.iter().filter_map(|a| a["id"].as_i64()).collect();
    let mbids: Vec<String> = cibles
        .iter()
        .filter_map(|a| texte(a, "foreignAlbumId"))
        .collect();
    if ids.is_empty() {
        if existant.is_some() {
            c.envoyer(
                c.requete(Method::POST, "/command")
                    .json(&json!({"name": "RefreshArtist", "artistId": artiste_id})),
            )
            .await?;
        }
        sqlx::query("UPDATE ajouts SET lidarr_artiste_id = $2, maj = now() WHERE id = $1")
            .bind(id)
            .bind(artiste_id)
            .execute(state.pool())
            .await
            .map_err(base)?;
        return Ok(false);
    }
    c.envoyer(
        c.requete(Method::PUT, "/album/monitor")
            .json(&json!({"albumIds": ids, "monitored": true})),
    )
    .await?;
    c.envoyer(
        c.requete(Method::POST, "/command")
            .json(&json!({"name": "AlbumSearch", "albumIds": ids})),
    )
    .await?;
    sqlx::query(
        "UPDATE ajouts SET lidarr_artiste_id = $2, lidarr_albums = $3,
           albums = CASE WHEN type = 'album' THEN albums ELSE $4 END,
           surveille = true, envoye_le = now(), maj = now() WHERE id = $1",
    )
    .bind(id)
    .bind(artiste_id)
    .bind(&ids)
    .bind(&mbids)
    .execute(state.pool())
    .await
    .map_err(base)?;
    Ok(true)
}

#[derive(Debug, Default)]
struct Lidarr {
    file: HashSet<i64>,
    echecs: HashMap<i64, String>,
    stats: HashMap<i64, (i64, i64)>,
}

const ECHECS: [&str; 2] = ["downloadFailed", "importFailed"];
const SUIVIS: [&str; 6] = [
    "grabbed",
    "downloadFailed",
    "importFailed",
    "downloadImported",
    "trackFileImported",
    "albumImportIncomplete",
];

fn raison_echec(r: &Value) -> String {
    texte(&r["data"], "message")
        .or_else(|| texte(&r["data"], "reason"))
        .map(|m| m.chars().take(300).collect())
        .unwrap_or_else(|| "téléchargement en échec dans Lidarr".into())
}

fn nouvel_etat(
    l: &PgRow,
    lidarr: &Lidarr,
    presents: &HashSet<String>,
    delai: Duration,
) -> Option<(&'static str, Option<String>)> {
    let ids: Vec<i64> = l.get("lidarr_albums");
    let mbids: Vec<String> = l.get("albums");
    let envoye: DateTime<Utc> = l.get("envoye_le");
    let surveille: bool = l.get("surveille");
    let complet = |id: &i64| lidarr.stats.get(id).is_some_and(|(f, t)| *t > 0 && f >= t);
    let candidat = if surveille
        && !ids.is_empty()
        && ids.iter().all(complet)
        && mbids.iter().all(|m| presents.contains(m))
    {
        Some(("disponible", None))
    } else if ids.iter().any(|id| lidarr.file.contains(id)) {
        Some(("telechargement", None))
    } else if let Some(r) = ids.iter().find_map(|id| lidarr.echecs.get(id)) {
        Some(("echec", Some(r.clone())))
    } else if ids
        .iter()
        .any(|id| lidarr.stats.get(id).is_some_and(|(f, _)| *f > 0))
    {
        Some(("partiel", None))
    } else if Utc::now() - envoye >= chrono::Duration::from_std(delai).unwrap_or_default() {
        Some((
            "echec",
            Some(format!(
                "aucun résultat après {} h",
                delai.as_secs().div_ceil(3600)
            )),
        ))
    } else {
        None
    };
    let etat: String = l.get("etat");
    candidat.filter(|(e, _)| *e != etat)
}

async fn lire_lidarr(
    c: &lidarr::Client<'_>,
    artistes: &[i64],
    depuis: &HashMap<i64, DateTime<Utc>>,
) -> Result<Lidarr, String> {
    let mut l = Lidarr::default();
    for r in enregistrements(c.get("/queue?page=1&pageSize=1000").await?) {
        if let Some(id) = r["albumId"].as_i64() {
            l.file.insert(id);
        }
    }
    let mut vus = HashSet::new();
    for r in enregistrements(
        c.get("/history?page=1&pageSize=250&sortKey=date&sortDirection=descending")
            .await?,
    ) {
        let (Some(id), Some(genre)) = (r["albumId"].as_i64(), r["eventType"].as_str()) else {
            continue;
        };
        let date = r["date"]
            .as_str()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
            .map(|d| d.with_timezone(&Utc));
        let Some(debut) = depuis.get(&id) else {
            continue;
        };
        if !SUIVIS.contains(&genre) || date.is_none_or(|d| d < *debut) || !vus.insert(id) {
            continue;
        }
        if ECHECS.contains(&genre) {
            l.echecs.insert(id, raison_echec(&r));
        }
    }
    for a in artistes {
        for al in c.liste(&format!("/album?artistId={a}")).await? {
            if let Some(id) = al["id"].as_i64() {
                let s = &al["statistics"];
                l.stats.insert(
                    id,
                    (
                        s["trackFileCount"].as_i64().unwrap_or(0),
                        s["trackCount"].as_i64().unwrap_or(0),
                    ),
                );
            }
        }
    }
    Ok(l)
}

async fn suivre_ou_erreur(state: &AppState) -> Result<(), String> {
    let Some(c) = client(state) else {
        return Ok(());
    };
    let a_envoyer: Vec<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM ajouts WHERE etat IN {ETATS_ACTIFS} AND NOT surveille ORDER BY cree_le"
    )))
    .fetch_all(state.pool())
    .await
    .map_err(base)?;
    for id in a_envoyer {
        if let Err(e) = envoyer(state, id).await {
            tracing::warn!("ajout {id} non envoyé à Lidarr : {e}");
        }
    }
    let actifs = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, etat, lidarr_artiste_id, lidarr_albums, albums, envoye_le, surveille FROM ajouts WHERE etat IN {ETATS_ACTIFS}"
    )))
    .fetch_all(state.pool())
    .await
    .map_err(base)?;
    if actifs.is_empty() {
        return Ok(());
    }
    let mut artistes: Vec<i64> = actifs
        .iter()
        .filter_map(|l| l.get::<Option<i64>, _>("lidarr_artiste_id"))
        .collect();
    artistes.sort_unstable();
    artistes.dedup();
    let mut depuis: HashMap<i64, DateTime<Utc>> = HashMap::new();
    for l in &actifs {
        for id in l.get::<Vec<i64>, _>("lidarr_albums") {
            depuis.insert(id, l.get("envoye_le"));
        }
    }
    let lidarr = lire_lidarr(&c, &artistes, &depuis).await?;
    let tous: Vec<String> = actifs
        .iter()
        .flat_map(|l| l.get::<Vec<String>, _>("albums"))
        .collect();
    let presents = presents(state.pool(), "albums", &tous)
        .await
        .map_err(base)?;
    let delai = state.config().ajouts.delai;
    for l in &actifs {
        if let Some((etat, raison)) = nouvel_etat(l, &lidarr, &presents, delai) {
            sqlx::query(
                "UPDATE ajouts SET etat = $3, raison = $4, maj = now() WHERE id = $1 AND etat = $2",
            )
            .bind(l.get::<Uuid, _>("id"))
            .bind(l.get::<String, _>("etat"))
            .bind(etat)
            .bind(raison)
            .execute(state.pool())
            .await
            .map_err(base)?;
        }
    }
    Ok(())
}

pub async fn suivre(state: &AppState) {
    if let Err(e) = suivre_ou_erreur(state).await {
        tracing::warn!("suivi des ajouts : {e}");
    }
}

pub async fn apres_webhook(state: &AppState, corps: &[u8]) {
    let v: Value = serde_json::from_slice(corps).unwrap_or_default();
    if !matches!(
        v["eventType"].as_str(),
        Some("Test" | "Health" | "HealthIssue" | "HealthRestored" | "ApplicationUpdate")
    ) {
        suivre(state).await;
    }
}

pub fn demarrer(state: &AppState) {
    if state.config().lidarr.is_none() {
        return;
    }
    let etat = state.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(PERIODE_SUIVI);
        loop {
            t.tick().await;
            suivre(&etat).await;
        }
    });
}

pub const COLONNES_AJOUT: &str = "a.id, a.type, a.mbid, a.titre, a.artiste_mbid, a.artiste_nom, a.origine, a.sub, a.nom,
    a.etat, a.raison, a.estimation, a.cree_le, a.maj,
    ARRAY(SELECT al.id FROM albums al WHERE al.present AND al.mbid = ANY(a.albums) ORDER BY al.sortie, al.id) AS presents";

pub fn etat_public(etat: &str) -> &'static str {
    match etat {
        "envoye" => "sent",
        "telechargement" => "downloading",
        "partiel" => "partial",
        "disponible" => "available",
        "echec" => "failed",
        _ => "deleted",
    }
}

pub fn etat_interne(public: &str) -> Option<&'static str> {
    [
        "envoye",
        "telechargement",
        "partiel",
        "disponible",
        "echec",
        "supprime",
    ]
    .into_iter()
    .find(|e| etat_public(e) == public)
}

pub fn ajout_public(l: &PgRow, admin: bool) -> Value {
    let mut v = json!({
        "id": l.get::<Uuid, _>("id"),
        "kind": if l.get::<String, _>("type") == "album" { "album" } else { "artist" },
        "mbid": l.get::<String, _>("mbid"),
        "title": l.get::<String, _>("titre"),
        "artist": {"mbid": l.get::<String, _>("artiste_mbid"), "name": l.get::<String, _>("artiste_nom")},
        "origin": if l.get::<String, _>("origine") == "manuel" { "manual" } else { "discovery" },
        "state": etat_public(&l.get::<String, _>("etat")),
        "reason": l.get::<Option<String>, _>("raison").map(|r| {
            if !admin && r.starts_with(lidarr::REFUS) {
                lidarr::REFUS.to_string()
            } else {
                r
            }
        }),
        "estimated_size": l.get::<i64, _>("estimation"),
        "albums": l.get::<Vec<Uuid>, _>("presents"),
        "created_at": horodatage(&l.get("cree_le")),
        "updated_at": horodatage(&l.get("maj")),
    });
    if admin {
        v["author"] = json!({"sub": l.get::<String, _>("sub"), "name": l.get::<String, _>("nom")});
    }
    v
}

pub async fn lire_ajout(state: &AppState, id: Uuid, admin: bool) -> Result<Value, Response> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_AJOUT} FROM ajouts a WHERE a.id = $1"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .map(|l| ajout_public(&l, admin))
    .ok_or_else(http::introuvable)
}

pub fn reponse_erreur(e: ErreurAjout) -> Response {
    reponse_erreur_pour(e, false)
}

pub fn reponse_erreur_admin(e: ErreurAjout) -> Response {
    reponse_erreur_pour(e, true)
}

fn reponse_erreur_pour(e: ErreurAjout, admin: bool) -> Response {
    match e {
        ErreurAjout::Invalide(m) => invalide(m),
        ErreurAjout::Introuvable => http::introuvable(),
        ErreurAjout::Etat(m) => erreur(StatusCode::CONFLICT, "invalid_state", m),
        ErreurAjout::NonConfigure => non_configure(),
        ErreurAjout::Incomplet(m) => {
            tracing::warn!("ajout : Lidarr : {m}");
            erreur(
                StatusCode::SERVICE_UNAVAILABLE,
                "lidarr_incomplete",
                &format!("Lidarr incomplet : {m}"),
            )
        }
        ErreurAjout::Refus(e) => {
            tracing::warn!("ajout : {e}");
            indisponible("lidarr_refused", if admin { &e } else { lidarr::REFUS })
        }
        ErreurAjout::Lidarr(e) => {
            tracing::warn!("ajout : {e}");
            indisponible("lidarr_unavailable", "Lidarr injoignable ou en erreur")
        }
        ErreurAjout::Base(e) => bd(e),
    }
}

pub fn non_configure() -> Response {
    erreur(
        StatusCode::SERVICE_UNAVAILABLE,
        "not_configured",
        "ajout de musique non configuré",
    )
}

pub async fn reponse_resultat(
    state: &AppState,
    r: Resultat,
    admin: bool,
) -> Result<Response, Response> {
    Ok(match r {
        Resultat::Envoye(id) => {
            http::json(StatusCode::CREATED, &lire_ajout(state, id, admin).await?)
        }
        Resultat::EnCours(id) => {
            let mut v = lire_ajout(state, id, admin).await?;
            v["existing"] = json!(true);
            ok(&v)
        }
        Resultat::DejaPresent => erreur(
            StatusCode::CONFLICT,
            "already_available",
            "déjà dans la bibliothèque",
        ),
        Resultat::Refuse(Refus::Exclu) => erreur(
            StatusCode::CONFLICT,
            "excluded",
            "retiré par un administrateur, ne peut pas être ajouté à nouveau",
        ),
        Resultat::Refuse(Refus::LimiteJournaliere { limite }) => erreur(
            StatusCode::TOO_MANY_REQUESTS,
            "daily_limit",
            &format!("limite de {limite} ajouts par jour atteinte"),
        ),
        Resultat::Refuse(Refus::BudgetDecouverte { .. }) => erreur(
            StatusCode::INSUFFICIENT_STORAGE,
            "discovery_budget",
            "budget de découverte atteint",
        ),
        Resultat::Refuse(Refus::DisquePlein {
            occupation,
            estimation,
        }) => {
            let mut v = occupation.public();
            v["error"] = json!("disk_full");
            v["message"] = json!("le disque de la musique serait plein");
            v["estimated"] = json!(estimation);
            http::json(StatusCode::INSUFFICIENT_STORAGE, &v)
        }
    })
}

fn objet(corps: &[u8]) -> Result<serde_json::Map<String, Value>, Response> {
    match serde_json::from_slice::<Value>(corps) {
        Ok(Value::Object(o)) => Ok(o),
        _ => Err(invalide("objet JSON attendu")),
    }
}

pub async fn creer(
    State(state): State<AppState>,
    p: Personne,
    corps: Bytes,
) -> Result<Response, Response> {
    let o = objet(&corps)?;
    let genre = match o.get("kind").and_then(Value::as_str) {
        Some("album") => Genre::Album,
        Some("artist") => Genre::Artiste,
        _ => return Err(invalide("kind attendu : album ou artist")),
    };
    let mbid = o.get("mbid").and_then(Value::as_str).unwrap_or_default();
    let d = DemandeAjout {
        mbid: mbid.to_string(),
        personne: Auteur::from(&p),
        origine: Origine::Manuel,
    };
    let r = ajouter(&state, genre, d).await.map_err(reponse_erreur)?;
    reponse_resultat(&state, r, false).await
}

pub async fn miennes(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_AJOUT} FROM ajouts a WHERE a.sub = $1 ORDER BY a.cree_le DESC, a.id LIMIT 200"
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| ajout_public(l, false)).collect::<Vec<_>>(),
    })))
}

pub async fn rechercher(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let terme = q
        .get("q")
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalide("recherche vide"))?;
    if terme.chars().count() > 100 {
        return Err(invalide("recherche trop longue"));
    }
    let c = client(&state).ok_or_else(non_configure)?;
    let lidarr = |e: String| reponse_erreur(ErreurAjout::Lidarr(e));
    let artistes = chercher(&c, "/artist/lookup", terme)
        .await
        .map_err(lidarr)?;
    let albums = chercher(&c, "/album/lookup", terme).await.map_err(lidarr)?;
    let artistes: Vec<(String, &Value)> = artistes
        .iter()
        .filter_map(|a| Some((mbid_valide(a["foreignArtistId"].as_str()?)?, a)))
        .take(RESULTATS_MAX)
        .collect();
    let albums: Vec<(String, &Value)> = albums
        .iter()
        .filter_map(|a| Some((mbid_valide(a["foreignAlbumId"].as_str()?)?, a)))
        .take(RESULTATS_MAX)
        .collect();
    let mbids: Vec<String> = artistes
        .iter()
        .chain(albums.iter())
        .map(|(m, _)| m.clone())
        .chain(
            albums
                .iter()
                .filter_map(|(_, a)| texte(&a["artist"], "foreignArtistId")),
        )
        .collect();
    let pool = state.pool();
    let (artistes_presents, albums_presents, en_cours, exclus) = (
        presents(pool, "artistes", &mbids).await.map_err(bd)?,
        presents(pool, "albums", &mbids).await.map_err(bd)?,
        en_cours_parmi(pool, &mbids).await.map_err(bd)?,
        exclus_parmi(pool, &mbids).await.map_err(bd)?,
    );
    let debit = reglages(&state, &c).await.ok().map(|r| r.debit);
    Ok(ok(&json!({
        "artists": artistes.iter().map(|(m, a)| json!({
            "mbid": m,
            "name": texte(a, "artistName"),
            "disambiguation": texte(a, "disambiguation"),
            "type": texte(a, "artistType"),
            "overview": texte(a, "overview"),
            "in_library": artistes_presents.contains(m),
            "in_progress": en_cours.contains(m),
            "excluded": exclus.contains(m),
        })).collect::<Vec<_>>(),
        "albums": albums.iter().map(|(m, a)| {
            let artiste = texte(&a["artist"], "foreignArtistId").unwrap_or_default();
            json!({
                "mbid": m,
                "title": texte(a, "title"),
                "type": texte(a, "albumType"),
                "secondary_types": secondaires(a),
                "year": annee(a),
                "duration": Some(duree_album(a)).filter(|d| *d > 0.0),
                "artist": {"mbid": artiste, "name": texte(&a["artist"], "artistName")},
                "in_library": albums_presents.contains(m),
                "in_progress": en_cours.contains(m),
                "excluded": exclus.contains(m) || exclus.contains(&artiste),
                "estimated_size": debit.map(|d| quota::estimer(duree_album(a), d)),
            })
        }).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use axum::http::StatusCode;

    use super::*;
    use crate::{
        routes,
        tests_api::{Bibliotheque, FLAC, LIDARR, Monde, bibliotheque, encode, generer},
        testutil::Requete,
    };

    pub(crate) type ReponseFausse = (u16, Vec<(&'static str, String)>, Vec<u8>);

    const DAFT: &str = "056e4f3e-d505-4dad-8ec1-d04f521cbb56";
    const DISCOVERY: &str = "48117b82-8c80-4bd6-9c8e-0d5b1fd7a1b1";
    const HOMEWORK: &str = "a5c4d3a3-6f1d-4b1a-9a6e-8b2d6b6c1a01";
    const ALIVE: &str = "b7e2a1c4-1f2e-4c3d-8a9b-0c1d2e3f4a5b";
    const SINGLE: &str = "c1d2e3f4-a5b6-4c7d-8e9f-0a1b2c3d4e5f";

    fn decoder(t: &str) -> String {
        let o = t.as_bytes();
        let mut r = Vec::new();
        let mut i = 0;
        while i < o.len() {
            match o[i] {
                b'+' => r.push(b' '),
                b'%' if i + 2 < o.len() => {
                    r.push(u8::from_str_radix(&t[i + 1..i + 3], 16).unwrap_or(b'?'));
                    i += 2;
                }
                b => r.push(b),
            }
            i += 1;
        }
        String::from_utf8_lossy(&r).into_owned()
    }

    fn parametre(r: &Requete, nom: &str) -> String {
        r.query
            .split('&')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| *k == nom)
            .map(|(_, v)| decoder(v))
            .unwrap_or_default()
    }

    fn correspond(terme: &str, mbid: &Value, noms: &[&Value]) -> bool {
        match terme.strip_prefix("lidarr:") {
            Some(m) => mbid.as_str() == Some(m),
            None => noms.iter().any(|n| {
                n.as_str()
                    .is_some_and(|n| n.to_lowercase().contains(&terme.to_lowercase()))
            }),
        }
    }

    fn suivant(m: &Monde) -> i64 {
        m.artistes
            .iter()
            .chain(&m.albums)
            .chain(&m.pistes)
            .chain(&m.fichiers)
            .filter_map(|x| x["id"].as_i64())
            .max()
            .unwrap_or(LIDARR)
            + 1
    }

    fn avec_stats(m: &Monde, a: &Value) -> Value {
        let mut a = a.clone();
        let pistes: Vec<&Value> = m
            .pistes
            .iter()
            .filter(|p| p["albumId"] == a["id"])
            .collect();
        a["statistics"] = json!({
            "trackCount": pistes.len(),
            "trackFileCount": pistes.iter().filter(|p| p["hasFile"] == true).count(),
        });
        a
    }

    pub(crate) fn lidarr_ajouts(r: &Requete, m: &mut Monde) -> Option<ReponseFausse> {
        let corps = |v: Value| {
            Some((
                200,
                vec![("content-type", "application/json".to_string())],
                v.to_string().into_bytes(),
            ))
        };
        let terme = parametre(r, "term");
        let id_final = |c: &str| c.rsplit('/').next().and_then(|i| i.parse::<i64>().ok());
        match (r.methode.as_str(), r.chemin.as_str()) {
            ("GET", "/api/v1/rootfolder") => corps(json!(m.dossiers)),
            ("GET", "/api/v1/qualityprofile") => corps(json!(m.qualites)),
            ("GET", "/api/v1/metadataprofile") => corps(json!(m.metas)),
            ("GET", "/api/v1/artist/lookup") => corps(json!(
                m.mb_artistes
                    .iter()
                    .filter(|a| correspond(&terme, &a["foreignArtistId"], &[&a["artistName"]]))
                    .collect::<Vec<_>>()
            )),
            ("GET", "/api/v1/album/lookup") => corps(json!(
                m.mb_albums
                    .iter()
                    .filter(|a| correspond(
                        &terme,
                        &a["foreignAlbumId"],
                        &[&a["title"], &a["artist"]["artistName"]]
                    ))
                    .collect::<Vec<_>>()
            )),
            ("POST", "/api/v1/artist") => {
                let mut a = r.json();
                let racine = a["rootFolderPath"].clone();
                if !m.dossiers.iter().any(|d| d["path"] == racine) {
                    return Some((
                        400,
                        vec![("content-type", "application/json".to_string())],
                        json!([{"propertyName": "RootFolderPath",
                            "errorMessage": format!("Root folder '{}' does not exist", racine.as_str().unwrap_or_default()),
                            "attemptedValue": racine, "severity": "error"}])
                        .to_string()
                        .into_bytes(),
                    ));
                }
                let id = suivant(m);
                a["id"] = json!(id);
                m.artistes.push(a.clone());
                let albums: Vec<Value> = m
                    .mb_albums
                    .iter()
                    .filter(|al| al["artist"]["foreignArtistId"] == a["foreignArtistId"])
                    .cloned()
                    .collect();
                for mut al in albums {
                    al["id"] = json!(suivant(m));
                    al["artistId"] = json!(id);
                    al["monitored"] = json!(false);
                    m.albums.push(al);
                }
                corps(a)
            }
            ("PUT", "/api/v1/album/monitor") => {
                let v = r.json();
                for al in m.albums.iter_mut() {
                    if v["albumIds"].as_array().unwrap().contains(&al["id"]) {
                        al["monitored"] = v["monitored"].clone();
                    }
                }
                corps(json!([]))
            }
            ("POST", "/api/v1/command") => corps(json!({"id": 1, "name": r.json()["name"]})),
            ("GET", "/api/v1/album") => {
                let artiste: Option<i64> = parametre(r, "artistId").parse().ok();
                let mbid = parametre(r, "foreignAlbumId");
                corps(json!(
                    m.albums
                        .iter()
                        .filter(|a| artiste.is_none_or(|i| a["artistId"] == i)
                            && (mbid.is_empty() || a["foreignAlbumId"] == mbid.as_str()))
                        .map(|a| avec_stats(m, a))
                        .collect::<Vec<_>>()
                ))
            }
            ("GET", "/api/v1/queue") => corps(json!({"records": m.file})),
            ("GET", "/api/v1/history") => corps(json!({"records": m.historique})),
            ("DELETE", c) if c.starts_with("/api/v1/album/") => {
                let id = id_final(c)?;
                m.albums.retain(|a| a["id"] != id);
                m.pistes.retain(|p| p["albumId"] != id);
                m.fichiers.retain(|f| f["albumId"] != id);
                Some((200, vec![], vec![]))
            }
            ("DELETE", c) if c.starts_with("/api/v1/artist/") => {
                let id = id_final(c)?;
                m.artistes.retain(|a| a["id"] != id);
                m.albums.retain(|a| a["artistId"] != id);
                m.pistes.retain(|p| p["artistId"] != id);
                m.fichiers.retain(|f| f["artistId"] != id);
                Some((200, vec![], vec![]))
            }
            _ => None,
        }
    }

    fn regler(b: &mut Bibliotheque, f: impl FnOnce(&mut quota::Reglages)) {
        let mut cfg = b.e.state.config().clone();
        f(&mut cfg.ajouts);
        b.e.state = AppState::new(cfg, b.e.state.pool().clone());
        b.e.app = routes::routeur(b.e.state.clone());
    }

    async fn monde_ajouts(f: impl FnOnce(&mut quota::Reglages)) -> Option<Bibliotheque> {
        let mut b = bibliotheque().await?;
        {
            let mut m = b.monde.lock().unwrap();
            let artiste = json!({"artistName": "Daft Punk", "foreignArtistId": DAFT});
            m.mb_artistes
                .push(json!({"artistName": "Daft Punk", "foreignArtistId": DAFT,
                "artistType": "Group", "disambiguation": "French duo", "overview": "Duo."}));
            for (mbid, titre, genre, secondaires, ms) in [
                (DISCOVERY, "Discovery", "Album", json!([]), 3_600_000),
                (HOMEWORK, "Homework", "Album", json!([]), 2_400_000),
                (ALIVE, "Alive 2007", "Album", json!(["Live"]), 4_000_000),
                (SINGLE, "One More Time", "Single", json!([]), 300_000),
            ] {
                m.mb_albums.push(json!({"foreignAlbumId": mbid, "title": titre, "albumType": genre,
                    "secondaryTypes": secondaires, "duration": ms, "releaseDate": "2001-03-12T00:00:00Z",
                    "artist": artiste}));
            }
        }
        regler(&mut b, f);
        Some(b)
    }

    async fn ajouter_http(
        b: &Bibliotheque,
        qui: &str,
        kind: &str,
        mbid: &str,
    ) -> crate::testutil::Rep {
        b.e.appel(
            Some(qui),
            "POST",
            "/additions",
            Some(json!({"kind": kind, "mbid": mbid})),
        )
        .await
    }

    fn requetes(b: &Bibliotheque, methode: &str, chemin: &str) -> Vec<Requete> {
        b.lidarr
            .requetes()
            .into_iter()
            .filter(|r| r.methode == methode && r.chemin == chemin)
            .collect()
    }

    fn album_lidarr(b: &Bibliotheque, mbid: &str) -> Value {
        b.monde
            .lock()
            .unwrap()
            .albums
            .iter()
            .find(|a| a["foreignAlbumId"] == mbid)
            .cloned()
            .unwrap()
    }

    fn surveilles(b: &Bibliotheque) -> Vec<String> {
        let mut l: Vec<String> = b
            .monde
            .lock()
            .unwrap()
            .albums
            .iter()
            .filter(|a| a["monitored"] == true)
            .map(|a| a["title"].as_str().unwrap().to_string())
            .collect();
        l.sort();
        l
    }

    async fn mon_ajout(b: &Bibliotheque, qui: &str, mbid: &str) -> Value {
        b.get(qui, "/additions/mine").await.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["mbid"] == mbid)
            .cloned()
            .unwrap_or(Value::Null)
    }

    async fn attendre_etat(b: &Bibliotheque, mbid: &str, etat: &str) -> Value {
        let limite = Instant::now() + Duration::from_secs(30);
        loop {
            let a = mon_ajout(b, "bruno", mbid).await;
            if a["state"] == etat {
                return a;
            }
            assert!(Instant::now() < limite, "état {etat} attendu : {a}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn importer(b: &Bibliotheque, mbid: &str) {
        generer(&b.chemin("Daft Punk/Discovery/01 One More Time.flac"), FLAC);
        let al = album_lidarr(b, mbid);
        let mut m = b.monde.lock().unwrap();
        let (piste, fichier) = (suivant(&m), suivant(&m) + 1);
        m.pistes.push(json!({"id": piste, "albumId": al["id"], "artistId": al["artistId"],
            "foreignRecordingId": "rec-omt", "title": "One More Time", "trackNumber": "1", "absoluteTrackNumber": 1,
            "mediumNumber": 1, "duration": 2000, "hasFile": true, "trackFileId": fichier}));
        m.fichiers.push(
            json!({"id": fichier, "artistId": al["artistId"], "albumId": al["id"],
            "path": "/music/Daft Punk/Discovery/01 One More Time.flac", "size": 1234}),
        );
    }

    async fn webhook(b: &Bibliotheque, evenement: &str) {
        let r =
            b.e.appel(
                None,
                "POST",
                "/webhooks/lidarr?token=jeton-webhook",
                Some(json!({"eventType": evenement})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn ajout_d_un_album_cree_l_artiste_avec_ce_seul_album_surveille_et_lance_la_recherche() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        let s = b.get("bruno", "/additions/search?q=daft").await;
        assert_eq!(s.statut, StatusCode::OK, "{}", s.texte());
        let sj = s.json();
        assert_eq!(sj["artists"][0]["mbid"], DAFT);
        assert_eq!(sj["artists"][0]["in_library"], false);
        let discovery = sj["albums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["mbid"] == DISCOVERY)
            .unwrap();
        assert_eq!(discovery["estimated_size"], 3600 * 1000 * 125);
        assert_eq!(discovery["in_progress"], false);
        b.sans_fuite(&s.texte(), "/additions/search");

        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        let a = r.json();
        assert_eq!(
            (
                a["state"].clone(),
                a["kind"].clone(),
                a["title"].clone(),
                a["origin"].clone()
            ),
            (
                json!("sent"),
                json!("album"),
                json!("Discovery"),
                json!("manual")
            )
        );
        assert_eq!(a["artist"], json!({"mbid": DAFT, "name": "Daft Punk"}));
        b.sans_fuite(&r.texte(), "POST /additions");

        let creations = requetes(&b, "POST", "/api/v1/artist");
        assert_eq!(creations.len(), 1);
        let c = creations[0].json();
        assert_eq!(c["foreignArtistId"], DAFT);
        assert_eq!(c["rootFolderPath"], "/data/music");
        assert_eq!(
            (
                c["qualityProfileId"].clone(),
                c["metadataProfileId"].clone(),
                c["tags"].clone()
            ),
            (json!(4), json!(3), json!([7]))
        );
        assert_eq!(c["monitored"], true);
        assert_eq!(c["addOptions"]["monitor"], "none");
        assert_eq!(surveilles(&b), ["Discovery"]);
        let id = album_lidarr(&b, DISCOVERY)["id"].clone();
        let recherches: Vec<Value> = requetes(&b, "POST", "/api/v1/command")
            .iter()
            .map(Requete::json)
            .collect();
        assert_eq!(
            recherches,
            [json!({"name": "AlbumSearch", "albumIds": [id]})]
        );

        let encore = ajouter_http(&b, "admin", "album", DISCOVERY).await;
        assert_eq!(encore.statut, StatusCode::OK);
        assert_eq!(
            (
                encore.json()["id"].clone(),
                encore.json()["existing"].clone()
            ),
            (a["id"].clone(), json!(true))
        );
        assert_eq!(requetes(&b, "POST", "/api/v1/artist").len(), 1);
        let s = b.get("bruno", "/additions/search?q=discovery").await.json();
        assert_eq!(s["albums"][0]["in_progress"], true);
        let mine = b.get("bruno", "/additions/mine").await;
        assert_eq!(mine.json()["items"].as_array().unwrap().len(), 1);
        assert!(mine.json()["items"][0].get("author").is_none());
        b.sans_fuite(&mine.texte(), "/additions/mine");
        assert!(
            b.get("admin", "/additions/mine").await.json()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        for (corps, statut) in [
            (
                json!({"kind": "album", "mbid": "pas-un-mbid"}),
                StatusCode::BAD_REQUEST,
            ),
            (
                json!({"kind": "piste", "mbid": DISCOVERY}),
                StatusCode::BAD_REQUEST,
            ),
            (
                json!({"kind": "album", "mbid": "00000000-0000-0000-0000-000000000009"}),
                StatusCode::NOT_FOUND,
            ),
        ] {
            let r =
                b.e.appel(Some("bruno"), "POST", "/additions", Some(corps.clone()))
                    .await;
            assert_eq!(r.statut, statut, "{corps}");
        }
    }

    #[tokio::test]
    async fn ajout_d_un_artiste_ne_surveille_que_ses_albums_studio() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        let r = ajouter_http(&b, "bruno", "artist", DAFT).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(r.json()["kind"], "artist");
        assert_eq!(r.json()["estimated_size"], (3600 + 2400) * 1000 * 125);
        assert_eq!(surveilles(&b), ["Discovery", "Homework"]);
        let c = requetes(&b, "POST", "/api/v1/command");
        let mut ids: Vec<i64> = c[0].json()["albumIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        let mut attendus = vec![
            album_lidarr(&b, DISCOVERY)["id"].as_i64().unwrap(),
            album_lidarr(&b, HOMEWORK)["id"].as_i64().unwrap(),
        ];
        attendus.sort_unstable();
        assert_eq!(
            (c.len(), c[0].json()["name"].clone(), ids),
            (1, json!("AlbumSearch"), attendus)
        );
        assert_eq!(
            requetes(&b, "POST", "/api/v1/artist")[0].json()["addOptions"]["monitor"],
            "none"
        );
    }

    #[tokio::test]
    async fn ajout_au_dela_du_plafond_refuse_sans_rien_envoyer_a_lidarr() {
        let Some(mut b) = monde_ajouts(|_| {}).await else {
            return;
        };
        b.synchro().await;
        let taille = crate::catalogue::taille_totale(b.e.state.pool())
            .await
            .unwrap();
        assert!(taille > 0);
        let discovery = 3600 * 1000 * 125;
        let homework = 2400 * 1000 * 125;
        regler(&mut b, |r| {
            r.plafond = Some((taille + discovery + homework - 1) as u64)
        });
        assert_eq!(
            ajouter_http(&b, "bruno", "album", DISCOVERY).await.statut,
            StatusCode::CREATED
        );
        let ecritures = |b: &Bibliotheque| {
            b.lidarr
                .requetes()
                .iter()
                .filter(|r| r.methode != "GET")
                .count()
        };
        let avant = ecritures(&b);
        let r = ajouter_http(&b, "bruno", "album", HOMEWORK).await;
        assert_eq!(r.statut, StatusCode::INSUFFICIENT_STORAGE, "{}", r.texte());
        let j = r.json();
        assert_eq!(j["error"], "disk_full");
        assert_eq!(
            (
                j["used"].clone(),
                j["pending"].clone(),
                j["estimated"].clone(),
                j["limit"].clone()
            ),
            (
                json!(taille),
                json!(discovery),
                json!(homework),
                json!(taille + discovery + homework - 1)
            )
        );
        assert_eq!(ecritures(&b), avant, "rien n'est envoyé à Lidarr");
        assert_eq!(surveilles(&b), ["Discovery"]);
        assert!(mon_ajout(&b, "bruno", HOMEWORK).await.is_null());
        let d = b.get("admin", "/admin/disk").await.json();
        assert_eq!(
            (d["used"].clone(), d["pending"].clone()),
            (json!(taille), json!(discovery))
        );
        assert_eq!(d["free"], homework - 1);
        let o = quota::occupation(&b.e.state).await.unwrap();
        assert_eq!((o.taille, o.en_cours), (taille, discovery));
    }

    #[tokio::test]
    async fn ajouts_simultanes_ne_depassent_pas_le_plafond_a_eux_deux() {
        let discovery: i64 = 3600 * 1000 * 125;
        let Some(b) = monde_ajouts(|r| r.plafond = Some(discovery as u64 + 1)).await else {
            return;
        };
        let demande = |mbid: &str, qui: &str| DemandeAjout {
            mbid: mbid.into(),
            personne: Auteur {
                sub: format!("sub-{qui}"),
                nom: qui.into(),
            },
            origine: Origine::Manuel,
        };
        let (a, h) = tokio::join!(
            ajouter_album(&b.e.state, demande(DISCOVERY, "bruno")),
            ajouter_album(&b.e.state, demande(HOMEWORK, "admin")),
        );
        let resultats = [a.unwrap(), h.unwrap()];
        let envoyes = resultats
            .iter()
            .filter(|r| matches!(r, Resultat::Envoye(_)))
            .count();
        let refuses = resultats
            .iter()
            .filter(|r| matches!(r, Resultat::Refuse(Refus::DisquePlein { .. })))
            .count();
        assert_eq!((envoyes, refuses), (1, 1), "{resultats:?}");
        let o = quota::occupation(&b.e.state).await.unwrap();
        assert!(!o.depasse(0), "{o:?}");
        assert_eq!(surveilles(&b).len(), 1);
    }

    #[tokio::test]
    async fn import_rend_l_ajout_disponible_et_ses_pistes_lisibles() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        assert_eq!(
            ajouter_http(&b, "bruno", "album", DISCOVERY).await.statut,
            StatusCode::CREATED
        );
        let id = album_lidarr(&b, DISCOVERY)["id"].clone();
        b.monde
            .lock()
            .unwrap()
            .file
            .push(json!({"albumId": id, "status": "downloading"}));
        webhook(&b, "Grab").await;
        attendre_etat(&b, DISCOVERY, "downloading").await;
        b.monde.lock().unwrap().file.clear();
        importer(&b, DISCOVERY);
        webhook(&b, "Download").await;
        let a = attendre_etat(&b, DISCOVERY, "available").await;
        let album = a["albums"][0].as_str().unwrap().to_string();
        let fiche = b.get("alice", &format!("/albums/{album}")).await.json();
        assert_eq!(fiche["title"], "Discovery");
        assert_eq!(fiche["tracks"][0]["available"], true);
        assert_eq!(fiche["tracks"][0]["format"]["codec"], "flac");
        let s = b.get("bruno", "/additions/search?q=discovery").await.json();
        assert_eq!(
            (
                s["albums"][0]["in_library"].clone(),
                s["albums"][0]["in_progress"].clone()
            ),
            (json!(true), json!(false))
        );
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::CONFLICT, json!("already_available"))
        );
        let gros = b.get("admin", "/admin/albums").await;
        let d = gros.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["title"] == "Discovery")
            .cloned()
            .unwrap();
        assert_eq!(d["added_by"], "bruno");
        assert!(d["size"].as_i64().unwrap() > 0);
        b.sans_fuite(&gros.texte(), "/admin/albums");
        assert_eq!(quota::occupation(&b.e.state).await.unwrap().en_cours, 0);
    }

    #[tokio::test]
    async fn echec_de_telechargement_visible_puis_relance() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        assert_eq!(
            ajouter_http(&b, "bruno", "album", DISCOVERY).await.statut,
            StatusCode::CREATED
        );
        let id = album_lidarr(&b, DISCOVERY)["id"].clone();
        b.monde.lock().unwrap().historique.push(
            json!({"albumId": id, "eventType": "downloadFailed",
            "date": (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
            "data": {"message": "Aucun indexeur n'a répondu"}}),
        );
        webhook(&b, "DownloadFailure").await;
        let a = attendre_etat(&b, DISCOVERY, "failed").await;
        assert_eq!(a["reason"], "Aucun indexeur n'a répondu");
        let liste = b.get("admin", "/admin/additions?state=failed").await;
        let l = liste.json();
        assert_eq!(l["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            l["items"][0]["author"],
            json!({"sub": "sub-bruno", "name": "bruno"})
        );
        assert_eq!(l["items"][0]["reason"], "Aucun indexeur n'a répondu");
        b.sans_fuite(&liste.texte(), "/admin/additions");
        assert_eq!(
            b.get("admin", "/admin/additions?state=x").await.statut,
            StatusCode::BAD_REQUEST
        );
        b.monde.lock().unwrap().historique.clear();
        let ajout = a["id"].as_str().unwrap();
        let r =
            b.e.appel(
                Some("admin"),
                "POST",
                &format!("/admin/additions/{ajout}/retry"),
                None,
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(
            (r.json()["state"].clone(), r.json()["reason"].clone()),
            (json!("sent"), Value::Null)
        );
        assert_eq!(requetes(&b, "POST", "/api/v1/command").len(), 2);
        let r =
            b.e.appel(
                Some("admin"),
                "POST",
                &format!("/admin/additions/{ajout}/retry"),
                None,
            )
            .await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::CONFLICT, json!("invalid_state"))
        );
    }

    #[tokio::test]
    async fn sans_resultat_apres_le_delai_l_ajout_echoue() {
        let Some(b) = monde_ajouts(|r| r.delai = Duration::ZERO).await else {
            return;
        };
        assert_eq!(
            ajouter_http(&b, "bruno", "album", HOMEWORK).await.statut,
            StatusCode::CREATED
        );
        assert_eq!(mon_ajout(&b, "bruno", HOMEWORK).await["state"], "sent");
        suivre(&b.e.state).await;
        let a = mon_ajout(&b, "bruno", HOMEWORK).await;
        assert_eq!(a["state"], "failed");
        assert!(
            a["reason"].as_str().unwrap().contains("aucun résultat"),
            "{a}"
        );
        assert_eq!(quota::occupation(&b.e.state).await.unwrap().en_cours, 0);
    }

    #[tokio::test]
    async fn permission_d_ajout_et_limite_journaliere() {
        let Some(b) = monde_ajouts(|r| r.par_jour = 1).await else {
            return;
        };
        for (m, c) in [
            ("GET", "/additions/search?q=daft"),
            ("POST", "/additions"),
            ("GET", "/additions/mine"),
        ] {
            let r =
                b.e.appel(
                    Some("alice"),
                    m,
                    c,
                    Some(json!({"kind": "album", "mbid": DISCOVERY})),
                )
                .await;
            assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {c}");
        }
        assert!(
            b.monde
                .lock()
                .unwrap()
                .albums
                .iter()
                .all(|a| a["foreignAlbumId"] != DISCOVERY)
        );
        assert_eq!(
            ajouter_http(&b, "bruno", "album", DISCOVERY).await.statut,
            StatusCode::CREATED
        );
        let r = ajouter_http(&b, "bruno", "album", HOMEWORK).await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::TOO_MANY_REQUESTS, json!("daily_limit"))
        );
        assert_eq!(
            ajouter_http(&b, "admin", "album", HOMEWORK).await.statut,
            StatusCode::CREATED
        );
    }

    #[tokio::test]
    async fn suppression_par_un_admin_retire_fichiers_et_lidarr_et_exclut() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        assert_eq!(
            ajouter_http(&b, "bruno", "album", DISCOVERY).await.statut,
            StatusCode::CREATED
        );
        importer(&b, DISCOVERY);
        webhook(&b, "Download").await;
        let a = attendre_etat(&b, DISCOVERY, "available").await;
        let album = a["albums"][0].as_str().unwrap().to_string();
        let piste = b.get("alice", &format!("/albums/{album}")).await.json()["tracks"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let lidarr_id = album_lidarr(&b, DISCOVERY)["id"].as_i64().unwrap();
        let r =
            b.e.appel(
                Some("admin"),
                "DELETE",
                &format!("/admin/albums/{album}"),
                None,
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT, "{}", r.texte());
        let suppressions = requetes(&b, "DELETE", &format!("/api/v1/album/{lidarr_id}"));
        assert_eq!(suppressions.len(), 1);
        assert_eq!(
            suppressions[0].query,
            "deleteFiles=true&addImportListExclusion=true"
        );
        assert_eq!(
            b.get("alice", &format!("/tracks/{piste}")).await.json()["available"],
            false
        );
        assert!(est_exclu(b.e.state.pool(), &[DISCOVERY]).await.unwrap());
        assert_eq!(mon_ajout(&b, "bruno", DISCOVERY).await["state"], "deleted");
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::CONFLICT, json!("excluded"))
        );
        let s = b.get("bruno", "/additions/search?q=discovery").await.json();
        assert_eq!(s["albums"][0]["excluded"], true);

        b.synchro().await;
        let sigur = b
            .get("alice", &format!("/search?q={}", encode("Sigur")))
            .await
            .json()["artists"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let r =
            b.e.appel(
                Some("admin"),
                "DELETE",
                &format!("/admin/artists/{sigur}"),
                None,
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(
            requetes(&b, "DELETE", &format!("/api/v1/artist/{}", LIDARR + 3))[0].query,
            "deleteFiles=true&addImportListExclusion=true"
        );
        assert_eq!(
            b.get("alice", &format!("/artists/{sigur}")).await.json()["available"],
            false
        );
        assert!(est_exclu(b.e.state.pool(), &["mb-sigur"]).await.unwrap());
        assert_eq!(
            b.e.appel(
                Some("admin"),
                "DELETE",
                &format!("/admin/albums/{}", Uuid::new_v4()),
                None
            )
            .await
            .statut,
            StatusCode::NOT_FOUND
        );
    }

    async fn reglages_de(
        b: &Bibliotheque,
    ) -> Result<lidarr::ReglagesAjout, lidarr::ErreurReglages> {
        reglages(&b.e.state, &client(&b.e.state).unwrap()).await
    }

    fn ecritures(b: &Bibliotheque) -> usize {
        b.lidarr
            .requetes()
            .iter()
            .filter(|r| r.methode != "GET")
            .count()
    }

    #[tokio::test]
    async fn plusieurs_dossiers_celui_des_artistes_existants_retenu_et_l_inaccessible_ecarte() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        {
            let mut m = b.monde.lock().unwrap();
            m.dossiers = vec![
                json!({"path": "/data/a", "defaultQualityProfileId": 4, "defaultMetadataProfileId": 3, "defaultTags": [], "accessible": true}),
                json!({"path": "/data/b/", "defaultQualityProfileId": 1, "defaultMetadataProfileId": 1, "defaultTags": [9], "accessible": true}),
                json!({"path": "/data/c", "defaultQualityProfileId": 4, "defaultMetadataProfileId": 3, "defaultTags": [], "accessible": false}),
            ];
            for (i, dossier) in ["/data/c", "/data/c", "/data/b/"].iter().enumerate() {
                m.artistes[i]["rootFolderPath"] = json!(dossier);
            }
        }
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        let c = requetes(&b, "POST", "/api/v1/artist")[0].json();
        assert_eq!(
            (
                c["rootFolderPath"].clone(),
                c["qualityProfileId"].clone(),
                c["metadataProfileId"].clone(),
                c["tags"].clone()
            ),
            (json!("/data/b/"), json!(1), json!(1), json!([9]))
        );
        assert_eq!(r.json()["estimated_size"], 3600 * 320 * 125);
        {
            let mut m = b.monde.lock().unwrap();
            for a in m.artistes.iter_mut() {
                a.as_object_mut().unwrap().remove("rootFolderPath");
            }
        }
        assert_eq!(
            reglages_de(&b).await.unwrap().racine,
            "/data/a",
            "sans artiste : le premier déclaré"
        );
        {
            let mut m = b.monde.lock().unwrap();
            m.artistes[0]["rootFolderPath"] = json!("/data/a");
            m.artistes[1]["rootFolderPath"] = json!("/data/b");
        }
        assert_eq!(
            reglages_de(&b).await.unwrap().racine,
            "/data/a",
            "à égalité : le premier déclaré"
        );
        {
            let mut m = b.monde.lock().unwrap();
            for d in m.dossiers.iter_mut() {
                d["accessible"] = json!(false);
            }
            m.artistes[0]["rootFolderPath"] = json!("/data/c");
            m.artistes[1]["rootFolderPath"] = json!("/data/c");
        }
        assert_eq!(
            reglages_de(&b).await.unwrap().racine,
            "/data/c",
            "aucun accessible : le plus utilisé"
        );
        b.monde.lock().unwrap().dossiers.truncate(1);
        let r = reglages_de(&b).await.unwrap();
        assert_eq!(
            (r.racine.as_str(), r.qualite.id, r.metadonnees.id, r.tags),
            ("/data/a", 4, 3, vec![])
        );
    }

    #[tokio::test]
    async fn profil_par_defaut_absent_ou_inconnu_repli_sur_le_plus_utilise_puis_le_premier() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        {
            let mut m = b.monde.lock().unwrap();
            m.dossiers[0]
                .as_object_mut()
                .unwrap()
                .remove("defaultQualityProfileId");
            m.dossiers[0]["defaultMetadataProfileId"] = json!(99);
            for (i, (q, md)) in [(4, 3), (4, 1), (1, 3)].into_iter().enumerate() {
                m.artistes[i]["qualityProfileId"] = json!(q);
                m.artistes[i]["metadataProfileId"] = json!(md);
            }
        }
        let r = reglages_de(&b).await.unwrap();
        assert_eq!(
            (
                r.qualite.clone(),
                r.metadonnees.clone(),
                r.source_racine.clone(),
                r.debit
            ),
            (
                lidarr::Profil {
                    id: 4,
                    nom: Some("Lossless".into()),
                    source: lidarr::Source::Lidarr
                },
                lidarr::Profil {
                    id: 3,
                    nom: Some("Albums studio".into()),
                    source: lidarr::Source::Lidarr
                },
                lidarr::Source::Lidarr,
                1000
            )
        );
        {
            let mut m = b.monde.lock().unwrap();
            for a in m.artistes.iter_mut() {
                a["qualityProfileId"] = json!(77);
                a.as_object_mut().unwrap().remove("metadataProfileId");
            }
        }
        let r = reglages_de(&b).await.unwrap();
        assert_eq!((r.qualite.id, r.metadonnees.id, r.debit), (1, 1, 320));
        let s = b.get("bruno", "/additions/search?q=discovery").await.json();
        assert_eq!(s["albums"][0]["estimated_size"], 3600 * 320 * 125);
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(r.json()["estimated_size"], 3600 * 320 * 125);
        let c = requetes(&b, "POST", "/api/v1/artist")[0].json();
        assert_eq!(
            (
                c["qualityProfileId"].clone(),
                c["metadataProfileId"].clone()
            ),
            (json!(1), json!(1))
        );
    }

    #[tokio::test]
    async fn surcharge_par_variable_l_emporte_sur_lidarr() {
        let Some(b) = monde_ajouts(|r| {
            r.profil_qualite = Some(1);
            r.profil_metadonnees = Some(1);
        })
        .await
        else {
            return;
        };
        let d = b.get("admin", "/admin/disk").await.json();
        assert_eq!(
            d["settings"],
            json!({
                "root_folder": "/data/music",
                "root_folder_source": "lidarr",
                "quality_profile": {"name": "Any", "source": "override"},
                "metadata_profile": {"name": "Standard", "source": "override"},
            })
        );
        assert_eq!(d["settings_error"], Value::Null);
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(r.json()["estimated_size"], 3600 * 320 * 125);
        let c = requetes(&b, "POST", "/api/v1/artist")[0].json();
        assert_eq!(
            (
                c["rootFolderPath"].clone(),
                c["qualityProfileId"].clone(),
                c["metadataProfileId"].clone()
            ),
            (json!("/data/music"), json!(1), json!(1))
        );
        let mut b = b;
        b.monde
            .lock()
            .unwrap()
            .dossiers
            .push(json!({"path": "/data/autre", "accessible": true}));
        regler(&mut b, |r| r.racine = Some("/data/autre".into()));
        let r = reglages_de(&b).await.unwrap();
        assert_eq!(
            (r.racine.as_str(), r.source_racine, r.tags),
            ("/data/autre", lidarr::Source::Surcharge, vec![])
        );
    }

    #[tokio::test]
    async fn reglages_retenus_visibles_par_l_admin() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        let d = b.get("admin", "/admin/disk").await.json();
        assert_eq!(d["configured"], true);
        assert_eq!(
            d["settings"],
            json!({
                "root_folder": "/data/music",
                "root_folder_source": "lidarr",
                "quality_profile": {"name": "Lossless", "source": "lidarr"},
                "metadata_profile": {"name": "Albums studio", "source": "lidarr"},
            })
        );
        assert_eq!(
            b.get("bruno", "/admin/disk").await.statut,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn lidarr_sans_dossier_racine_ni_profil_erreur_explicite_sans_ecriture() {
        let Some(b) = monde_ajouts(|_| {}).await else {
            return;
        };
        b.monde.lock().unwrap().dossiers.clear();
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(
            (
                r.statut,
                r.json()["error"].clone(),
                r.json()["message"].clone()
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                json!("lidarr_incomplete"),
                json!("Lidarr incomplet : aucun dossier racine")
            )
        );
        assert_eq!(ecritures(&b), 0, "rien n'est envoyé à Lidarr");
        assert!(mon_ajout(&b, "bruno", DISCOVERY).await.is_null());
        let d = b.get("admin", "/admin/disk").await.json();
        assert_eq!(
            (d["settings"].clone(), d["settings_error"].clone()),
            (Value::Null, json!("Lidarr : aucun dossier racine"))
        );
        let s = b.get("bruno", "/additions/search?q=discovery").await;
        assert_eq!(s.statut, StatusCode::OK);
        assert_eq!(s.json()["albums"][0]["estimated_size"], Value::Null);
        {
            let mut m = b.monde.lock().unwrap();
            m.dossiers
                .push(json!({"path": "/data/music", "accessible": true}));
            m.metas.clear();
        }
        let r = ajouter_http(&b, "bruno", "artist", DAFT).await;
        assert_eq!(
            (r.statut, r.json()["message"].clone()),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                json!("Lidarr incomplet : aucun profil de métadonnées")
            )
        );
        b.monde.lock().unwrap().qualites.clear();
        assert_eq!(
            reglages_de(&b).await,
            Err(lidarr::ErreurReglages::Incomplet("aucun profil de qualité"))
        );
        assert_eq!(ecritures(&b), 0);
    }

    #[tokio::test]
    async fn refus_de_validation_de_lidarr_remonte_dans_la_raison_cote_admin() {
        let Some(b) = monde_ajouts(|r| r.racine = Some("/music".into())).await else {
            return;
        };
        let r = ajouter_http(&b, "bruno", "album", DISCOVERY).await;
        assert_eq!(
            (
                r.statut,
                r.json()["error"].clone(),
                r.json()["message"].clone()
            ),
            (
                StatusCode::BAD_GATEWAY,
                json!("lidarr_refused"),
                json!("Lidarr a refusé l'ajout")
            )
        );
        b.sans_fuite(&r.texte(), "POST /additions refusé");
        let mien = b.get("bruno", "/additions/mine").await;
        let a = &mien.json()["items"][0];
        assert_eq!(
            (a["state"].clone(), a["reason"].clone()),
            (json!("failed"), json!("Lidarr a refusé l'ajout"))
        );
        b.sans_fuite(&mien.texte(), "/additions/mine refusé");
        let l = b.get("admin", "/admin/additions?state=failed").await.json();
        assert_eq!(
            l["items"][0]["reason"],
            "Lidarr a refusé l'ajout : Root folder '/music' does not exist"
        );
        assert!(!l.to_string().contains("cle-lidarr"));
        assert_eq!(quota::occupation(&b.e.state).await.unwrap().en_cours, 0);
        let id = a["id"].as_str().unwrap();
        let r =
            b.e.appel(
                Some("admin"),
                "POST",
                &format!("/admin/additions/{id}/retry"),
                None,
            )
            .await;
        assert_eq!(
            (
                r.statut,
                r.json()["error"].clone(),
                r.json()["message"].clone()
            ),
            (
                StatusCode::BAD_GATEWAY,
                json!("lidarr_refused"),
                json!("Lidarr a refusé l'ajout : Root folder '/music' does not exist")
            )
        );
        assert_eq!(mon_ajout(&b, "bruno", DISCOVERY).await["state"], "failed");
        assert!(requetes(&b, "POST", "/api/v1/command").is_empty());
    }
}
