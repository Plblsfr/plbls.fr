use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use axum::{extract::State, response::Response};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    ajouts::{
        self, Auteur, COLONNES_AJOUT, DemandeAjout, ErreurAjout, Origine, Refus, Resultat,
        ajout_public,
    },
    bibliotheque::{COLONNES_ALBUM, album_public},
    droits::Droit,
    etat::AppState,
    garde::Personne,
    http::{bd, horodatage, ok},
    listenbrainz::{self, Populaire},
    quota,
    recommandation::{
        self, Demande, Donnees,
        score::{self, Candidat},
    },
};

pub const ARTISTES_CONTEXTE: usize = 5;
pub const ARTISTES_SIMILAIRES: usize = 5;
pub const TITRES_PAR_ARTISTE: usize = 10;
pub const ESSAIS: usize = 5;
pub const SOURCE_MAX: usize = 2000;
pub const SAUTS_SUPPRESSION: i64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reglages {
    pub active: bool,
    pub essai: chrono::Duration,
}

impl Reglages {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let active = match env("SILLON_DECOUVERTE").as_deref() {
            None | Some("true") => true,
            Some("false") => false,
            Some(v) => return Err(format!("SILLON_DECOUVERTE invalide : {v}")),
        };
        let jours: u32 = match env("SILLON_DECOUVERTE_ESSAI_JOURS") {
            None => 30,
            Some(v) => v
                .parse()
                .map_err(|_| format!("SILLON_DECOUVERTE_ESSAI_JOURS invalide : {v}"))?,
        };
        Ok(Self {
            active,
            essai: chrono::Duration::days(i64::from(jours)),
        })
    }
}

#[derive(Default)]
pub struct Etat {
    en_cours: Mutex<HashSet<String>>,
    terminees: AtomicU64,
}

impl Etat {
    pub fn terminees(&self) -> u64 {
        self.terminees.load(Ordering::SeqCst)
    }

    fn personnes(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.en_cours.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn cle(titre: &str) -> String {
    titre
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub async fn promouvoir(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE ajouts a SET origine = 'manuel', promu_le = now(), maj = now()
         WHERE a.origine = 'decouverte' AND EXISTS (
            SELECT 1 FROM albums al JOIN pistes p ON p.album_id = al.id
            WHERE al.mbid = ANY(a.albums) AND (
                EXISTS (SELECT 1 FROM likes l WHERE (l.type = 'track' AND l.cible = p.id) OR (l.type = 'album' AND l.cible = al.id))
                OR EXISTS (SELECT 1 FROM playlists_pistes pp WHERE pp.piste_id = p.id)))",
    )
    .execute(pool)
    .await?
    .rows_affected())
}

async fn permise(state: &AppState, sub: &str, peut_ajouter: bool) -> Result<bool, sqlx::Error> {
    let cfg = state.config();
    if !cfg.decouverte.active || !peut_ajouter || cfg.lidarr.is_none() || !cfg.ajouts.configure() {
        return Ok(false);
    }
    let pool = state.pool();
    if !crate::preferences::decouverte_de(pool, sub).await? {
        return Ok(false);
    }
    promouvoir(pool).await?;
    let mut conn = pool.acquire().await?;
    Ok(quota::decouvertes_du_jour(&mut conn, sub).await?
        < i64::from(cfg.ajouts.decouverte_par_jour)
        && quota::budget_decouverte(&mut conn).await?
            < i64::try_from(cfg.ajouts.decouverte_budget).unwrap_or(i64::MAX))
}

struct Proche {
    mbid: String,
    nom: String,
    score: f32,
    similaire_a: String,
}

struct Titre<'a> {
    artiste: &'a Proche,
    rang: usize,
    populaire: Populaire,
    note: f32,
}

async fn proches(state: &AppState, principaux: &[String]) -> Result<Vec<Proche>, sqlx::Error> {
    let noms: HashMap<String, String> =
        sqlx::query("SELECT mbid, nom FROM artistes WHERE mbid = ANY($1)")
            .bind(principaux)
            .fetch_all(state.pool())
            .await?
            .iter()
            .map(|l| (l.get(0), l.get(1)))
            .collect();
    let mut par_mbid: HashMap<String, Proche> = HashMap::new();
    for source in principaux {
        for s in listenbrainz::similaires(state, source).await? {
            if principaux.contains(&s.mbid) {
                continue;
            }
            let p = par_mbid.entry(s.mbid.clone()).or_insert(Proche {
                mbid: s.mbid,
                nom: s.nom,
                score: 0.0,
                similaire_a: String::new(),
            });
            if s.score > p.score {
                p.score = s.score;
                p.similaire_a = noms.get(source).cloned().unwrap_or_default();
            }
        }
    }
    let mut l: Vec<Proche> = par_mbid.into_values().collect();
    l.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.mbid.cmp(&b.mbid)));
    Ok(l)
}

async fn absents<'a>(
    pool: &PgPool,
    titres: Vec<(&'a Proche, usize, Populaire)>,
) -> Result<Vec<(&'a Proche, usize, Populaire)>, sqlx::Error> {
    let presents: Vec<i64> = sqlx::query_scalar(
        "SELECT t.r FROM unnest($1::text[], $2::text[], $3::text[]) WITH ORDINALITY AS t(enregistrement, artiste, titre, r)
         WHERE EXISTS (SELECT 1 FROM pistes p JOIN artistes ar ON ar.id = p.artiste_id
                       WHERE p.present AND (p.mbid_enregistrement = t.enregistrement
                                            OR (ar.mbid = t.artiste AND p.cle = sans_accents(t.titre))))",
    )
    .bind(titres.iter().map(|t| t.2.mbid.clone()).collect::<Vec<_>>())
    .bind(titres.iter().map(|t| t.0.mbid.clone()).collect::<Vec<_>>())
    .bind(titres.iter().map(|t| t.2.titre.clone()).collect::<Vec<_>>())
    .fetch_all(pool)
    .await?;
    Ok(titres
        .into_iter()
        .zip(1i64..)
        .filter(|(_, r)| !presents.contains(r))
        .map(|(t, _)| t)
        .collect())
}

async fn noter<'a>(
    pool: &PgPool,
    d: &Donnees,
    titres: Vec<(&'a Proche, usize, Populaire)>,
) -> Result<Vec<Titre<'a>>, sqlx::Error> {
    let mbids: Vec<String> = titres.iter().map(|t| t.0.mbid.clone()).collect();
    let connus: HashMap<String, (Uuid, Vec<i64>)> = sqlx::query(
        "SELECT ar.mbid, ar.id, ARRAY(SELECT genre_id FROM artistes_genres WHERE artiste_id = ar.id)
         FROM artistes ar WHERE ar.mbid = ANY($1)",
    )
    .bind(&mbids)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|l| (l.get(0), (l.get(1), l.get(2))))
    .collect();
    let mut l: Vec<Titre> = titres
        .into_iter()
        .map(|(artiste, rang, populaire)| {
            let connu = connus.get(&artiste.mbid);
            let genres = connu
                .map(|c| &c.1)
                .filter(|g| !g.is_empty() && !d.ambiance.genres.is_empty());
            let c = Candidat {
                similarite: Some(artiste.score),
                genres_communs: genres.map(|g| {
                    g.iter().filter(|x| d.ambiance.genres.contains(x)).count() as f32
                        / g.len() as f32
                }),
                artiste: connu.and_then(|c| d.profil.artistes.get(&c.0)),
                artiste_aime: connu.is_some_and(|c| d.profil.artistes_aimes.contains(&c.0)),
                ..Candidat::default()
            };
            Titre {
                note: score::noter(&c, &d.poids, d.maintenant).total - rang as f32 * 0.01,
                artiste,
                rang,
                populaire,
            }
        })
        .collect();
    l.sort_by(|a, b| {
        b.note
            .total_cmp(&a.note)
            .then(a.artiste.mbid.cmp(&b.artiste.mbid))
            .then(a.rang.cmp(&b.rang))
    });
    Ok(l)
}

async fn album_de(
    state: &AppState,
    albums: &mut HashMap<String, Vec<Value>>,
    choisi: &Titre<'_>,
    tous: &[Titre<'_>],
) -> Result<Option<String>, ErreurAjout> {
    let artiste = choisi.artiste;
    if !albums.contains_key(&artiste.mbid) {
        let c = ajouts::client(state).ok_or(ErreurAjout::NonConfigure)?;
        let l = ajouts::chercher(&c, "/album/lookup", &artiste.nom)
            .await?
            .into_iter()
            .filter(|a| {
                a["artist"]["foreignArtistId"].as_str() == Some(&artiste.mbid)
                    && ajouts::est_studio(a)
            })
            .collect();
        albums.insert(artiste.mbid.clone(), l);
    }
    let titre = cle(&choisi.populaire.titre);
    let porteurs: Vec<&Populaire> = tous
        .iter()
        .filter(|t| t.artiste.mbid == artiste.mbid && cle(&t.populaire.titre) == titre)
        .map(|t| &t.populaire)
        .collect();
    let mut l: Vec<(f64, String)> = albums[&artiste.mbid]
        .iter()
        .filter_map(|a| {
            let mbid = ajouts::mbid_valide(a["foreignAlbumId"].as_str()?)?;
            let nom = cle(a["title"].as_str().unwrap_or_default());
            porteurs
                .iter()
                .any(|p| {
                    p.album_mbid.as_deref() == Some(&mbid)
                        || p.album_titre.as_deref().is_some_and(|t| cle(t) == nom)
                })
                .then(|| (ajouts::duree_album(a), mbid))
        })
        .collect();
    l.sort_by(|a, b| {
        (a.0 <= 0.0)
            .cmp(&(b.0 <= 0.0))
            .then(a.0.total_cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });
    Ok(l.into_iter().next().map(|a| a.1))
}

async fn chercher(
    state: &AppState,
    auteur: &Auteur,
    peut_ajouter: bool,
    source: &[Uuid],
) -> Result<Option<Uuid>, ErreurAjout> {
    let pool = state.pool();
    let source = &source[..source.len().min(SOURCE_MAX)];
    let metas = recommandation::metas(pool, source).await?;
    let principaux = recommandation::artistes_principaux(source, &metas, ARTISTES_CONTEXTE);
    let mut proches = proches(state, &principaux).await?;
    if !permise(state, &auteur.sub, peut_ajouter).await? {
        return Ok(None);
    }
    let exclus: Vec<String> =
        sqlx::query_scalar("SELECT mbid FROM exclusions WHERE mbid = ANY($1)")
            .bind(proches.iter().map(|p| p.mbid.clone()).collect::<Vec<_>>())
            .fetch_all(pool)
            .await?;
    proches.retain(|p| !exclus.contains(&p.mbid) && !p.nom.is_empty());
    proches.truncate(ARTISTES_SIMILAIRES);
    let mut titres = Vec::new();
    for p in &proches {
        let l = listenbrainz::populaires(state, &p.mbid).await?;
        titres.extend(
            l.into_iter()
                .take(TITRES_PAR_ARTISTE)
                .enumerate()
                .map(|(rang, t)| (p, rang, t)),
        );
    }
    let titres = absents(pool, titres).await?;
    if titres.is_empty() {
        return Ok(None);
    }
    let d = recommandation::charger(
        state,
        &Demande {
            sub: &auteur.sub,
            source,
            exclues: &[],
            graine: 0,
            candidats: false,
            maintenant: Utc::now(),
        },
    )
    .await?;
    let titres = noter(pool, &d, titres).await?;
    let refuses: HashSet<String> =
        sqlx::query_scalar("SELECT mbid FROM refus_albums WHERE sub = $1")
            .bind(&auteur.sub)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut albums = HashMap::new();
    let mut vus = HashSet::new();
    for t in &titres {
        if !vus.insert((t.artiste.mbid.clone(), cle(&t.populaire.titre))) {
            continue;
        }
        if vus.len() > ESSAIS {
            break;
        }
        let Some(mbid) = album_de(state, &mut albums, t, &titres).await? else {
            continue;
        };
        if refuses.contains(&mbid) || ajouts::est_exclu(pool, &[&mbid, &t.artiste.mbid]).await? {
            continue;
        }
        let demande = DemandeAjout {
            mbid,
            personne: auteur.clone(),
            origine: Origine::Decouverte,
        };
        match ajouts::ajouter_album(state, demande).await? {
            Resultat::Envoye(id) => {
                sqlx::query(
                    "UPDATE ajouts SET similaire_a = $2, enregistrement = $3, titre_piste = $4 WHERE id = $1",
                )
                .bind(id)
                .bind(&t.artiste.similaire_a)
                .bind(&t.populaire.mbid)
                .bind(&t.populaire.titre)
                .execute(pool)
                .await?;
                return Ok(Some(id));
            }
            Resultat::DejaPresent | Resultat::EnCours(_) | Resultat::Refuse(Refus::Exclu) => {}
            Resultat::Refuse(_) => return Ok(None),
        }
    }
    Ok(None)
}

pub fn declencher(state: &AppState, p: &Personne, source: Vec<Uuid>) {
    if state.config().listenbrainz.is_none() || source.is_empty() {
        return;
    }
    if !state.decouverte().personnes().insert(p.sub().to_string()) {
        return;
    }
    let (state, auteur, peut_ajouter) = (state.clone(), Auteur::from(p), p.droits.a(Droit::Ajout));
    tokio::spawn(async move {
        match chercher(&state, &auteur, peut_ajouter, &source).await {
            Ok(Some(id)) => tracing::info!("découverte : ajout {id}"),
            Ok(None) => {}
            Err(e) => tracing::warn!("découverte : {e:?}"),
        }
        let e = state.decouverte();
        e.personnes().remove(&auteur.sub);
        e.terminees.fetch_add(1, Ordering::SeqCst);
    });
}

const TAILLE_ALBUMS: &str =
    "(SELECT coalesce(sum(f.taille), 0)::bigint FROM fichiers f JOIN pistes p ON p.id = f.piste_id
    JOIN albums al ON al.id = p.album_id WHERE al.mbid = ANY(a.albums))";

pub async fn admin(State(state): State<AppState>) -> Result<Response, Response> {
    let pool = state.pool();
    let cfg = state.config();
    promouvoir(pool).await.map_err(bd)?;
    let mut conn = pool.acquire().await.map_err(bd)?;
    let utilise = quota::budget_decouverte(&mut conn).await.map_err(bd)?;
    drop(conn);
    let ajouts = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_AJOUT}, a.similaire_a, a.titre_piste, {TAILLE_ALBUMS} AS taille
         FROM ajouts a WHERE a.origine = 'decouverte' ORDER BY a.cree_le DESC, a.id LIMIT 500"
    )))
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let propositions = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ALBUM}, t.taille, a.id AS ajout_id, a.sub, a.nom, a.similaire_a, a.titre_piste, e.sauts
         FROM ajouts a JOIN albums al ON al.mbid = ANY(a.albums) JOIN artistes ar ON ar.id = al.artiste_id
         JOIN LATERAL (SELECT coalesce(sum(f.taille), 0)::bigint AS taille FROM fichiers f JOIN pistes p ON p.id = f.piste_id
                       WHERE p.album_id = al.id) t ON true
         JOIN LATERAL (SELECT count(*) FILTER (WHERE ec.fin = 'skipped') AS sauts, count(*) FILTER (WHERE ec.fin = 'completed') AS completes
                       FROM ecoutes ec JOIN pistes p ON p.id = ec.piste_id WHERE p.album_id = al.id) e ON true
         WHERE a.origine = 'decouverte' AND a.etat <> 'supprime' AND al.present AND e.completes = 0
           AND (al.present_le < $1 OR e.sauts >= $2)
         ORDER BY t.taille DESC, al.id LIMIT 500"
    )))
    .bind(Utc::now() - cfg.decouverte.essai)
    .bind(SAUTS_SUPPRESSION)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    let raison = |l: &sqlx::postgres::PgRow| {
        (
            json!(
                l.get::<Option<String>, _>("similaire_a")
                    .filter(|s| !s.is_empty())
            ),
            json!(l.get::<Option<String>, _>("titre_piste")),
        )
    };
    Ok(ok(&json!({
        "enabled": cfg.decouverte.active && cfg.listenbrainz.is_some() && cfg.lidarr.is_some() && cfg.ajouts.configure(),
        "budget": {
            "used": utilise,
            "limit": cfg.ajouts.decouverte_budget,
            "daily_limit": cfg.ajouts.decouverte_par_jour,
            "trial_days": cfg.decouverte.essai.num_days(),
        },
        "items": ajouts.iter().map(|l| {
            let mut v = ajout_public(l, true);
            (v["similar_to"], v["track_title"]) = raison(l);
            v["size"] = json!(l.get::<i64, _>("taille"));
            v
        }).collect::<Vec<_>>(),
        "removal_candidates": propositions.iter().map(|l| {
            let (similaire_a, titre) = raison(l);
            json!({
                "addition": l.get::<Uuid, _>("ajout_id"),
                "album": album_public(l),
                "size": l.get::<i64, _>("taille"),
                "author": {"sub": l.get::<String, _>("sub"), "name": l.get::<String, _>("nom")},
                "similar_to": similaire_a,
                "track_title": titre,
                "skips": l.get::<i64, _>("sauts"),
                "available_since": l.get::<Option<chrono::DateTime<Utc>>, _>("present_le").as_ref().map(horodatage),
            })
        }).collect::<Vec<_>>(),
    })))
}
