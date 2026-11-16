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
    interdits: &mut HashSet<String>,
    choisi: &Titre<'_>,
    tous: &[Titre<'_>],
) -> Result<Option<String>, ErreurAjout> {
    let artiste = choisi.artiste;
    if !albums.contains_key(&artiste.mbid) {
        let c = ajouts::client(state).ok_or(ErreurAjout::NonConfigure)?;
        let l: Vec<Value> = ajouts::chercher(&c, "/album/lookup", &artiste.nom)
            .await?
            .into_iter()
            .filter(|a| {
                a["artist"]["foreignArtistId"].as_str() == Some(&artiste.mbid)
                    && ajouts::est_studio(a)
            })
            .collect();
        let mbids: Vec<&str> = l
            .iter()
            .filter_map(|a| a["foreignAlbumId"].as_str())
            .collect();
        interdits.extend(
            sqlx::query_scalar::<_, String>("SELECT mbid FROM exclusions WHERE mbid = ANY($1)")
                .bind(&mbids)
                .fetch_all(state.pool())
                .await?,
        );
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
            let mbid = ajouts::mbid_valide(a["foreignAlbumId"].as_str()?)
                .filter(|m| !interdits.contains(m))?;
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
    let mut interdits: HashSet<String> =
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
        let Some(mbid) = album_de(state, &mut albums, &mut interdits, t, &titres).await? else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        config::Config,
        listenbrainz::tests::reglages,
        routes,
        tests_api::{Bibliotheque, FLAC, LIDARR, bibliotheque_avec, generer},
        testutil::{Faux, Requete, attendre, faux},
    };

    const BEYONCE: &str = "859d0860-d480-4efd-970c-c05d5f1776b8";
    const DAFT: &str = "056e4f3e-d505-4dad-8ec1-d04f521cbb56";
    const JUSTICE: &str = "f3b8e107-abe8-4743-b6a3-4a4ee995e71f";
    const DISCOVERY: &str = "48117b82-8c80-4bd6-9c8e-0d5b1fd7a1b1";
    const HOMEWORK: &str = "a5c4d3a3-6f1d-4b1a-9a6e-8b2d6b6c1a01";
    const ALIVE: &str = "b7e2a1c4-1f2e-4c3d-8a9b-0c1d2e3f4a5b";
    const COURT: &str = "c1d2e3f4-a5b6-4c7d-8e9f-0a1b2c3d4e5f";

    fn enregistrement(i: u128) -> String {
        Uuid::from_u128(0xabc0_0000 + i).to_string()
    }

    fn reponse_lb(r: &Requete) -> (u16, Vec<u8>) {
        let corps = if r.chemin == "/similar-artists/json" && r.query.contains(BEYONCE) {
            json!([
                {"artist_mbid": DAFT, "name": "Daft Punk", "score": 900},
                {"artist_mbid": JUSTICE, "name": "Justice", "score": 300},
            ])
        } else if r.chemin.ends_with(DAFT) {
            json!([
                {"recording_mbid": enregistrement(1), "recording_name": "One More Time", "release_name": "Discovery", "total_listen_count": 900},
                {"recording_mbid": enregistrement(2), "recording_name": "One more time", "release_name": "Alive 2007", "total_listen_count": 800},
                {"recording_mbid": enregistrement(3), "recording_name": "One More Time", "release_name": "Musique, vol. 1", "total_listen_count": 700},
                {"recording_mbid": enregistrement(4), "recording_name": "Around the World", "release_name": "Homework", "total_listen_count": 600},
                {"recording_mbid": enregistrement(5), "recording_name": "Formation", "release_name": "Homework", "total_listen_count": 500},
            ])
        } else {
            json!([])
        };
        (200, corps.to_string().into_bytes())
    }

    type Regle = fn(&mut Config);

    struct Banc {
        b: Bibliotheque,
        lb: Faux,
    }

    async fn banc(regler: impl FnOnce(&mut Config) + Send + 'static) -> Option<Banc> {
        let lb = faux(reponse_lb).await;
        let url = lb.url.clone();
        let b = bibliotheque_avec(move |c| {
            c.ajouts.racine = "/music".into();
            c.ajouts.profil_qualite = 1;
            c.ajouts.profil_metadonnees = 2;
            c.ajouts.decouverte_par_jour = 5;
            c.listenbrainz = Some(reglages(&url, Duration::ZERO));
            regler(c);
        })
        .await?;
        {
            let mut m = b.monde.lock().unwrap();
            m.artistes[0]["foreignArtistId"] = json!(BEYONCE);
            for i in 0..3 {
                let chemin = format!("Beyonce/Lemonade/1{i} Bonus {i}.flac");
                generer(&b.chemin(&chemin), FLAC);
                m.pistes.push(json!({"id": LIDARR + 150 + i, "albumId": LIDARR + 10, "artistId": LIDARR + 1,
                    "foreignRecordingId": format!("rec-bonus-{i}"), "title": format!("Bonus {i}"), "trackNumber": (10 + i).to_string(),
                    "absoluteTrackNumber": 10 + i, "mediumNumber": 1, "duration": 2000, "hasFile": true, "trackFileId": LIDARR + 250 + i}));
                m.fichiers.push(
                    json!({"id": LIDARR + 250 + i, "artistId": LIDARR + 1, "albumId": LIDARR + 10,
                    "path": format!("/music/{chemin}"), "size": 1234}),
                );
            }
            let artiste = json!({"artistName": "Daft Punk", "foreignArtistId": DAFT});
            m.mb_artistes.push(artiste.clone());
            for (mbid, titre, secondaires, ms) in [
                (DISCOVERY, "Discovery", json!([]), 3_600_000),
                (HOMEWORK, "Homework", json!([]), 2_400_000),
                (ALIVE, "Alive 2007", json!(["Live"]), 600_000),
                (COURT, "Musique Vol. 1", json!([]), 1_200_000),
            ] {
                m.mb_albums.push(json!({"foreignAlbumId": mbid, "title": titre, "albumType": "Album",
                    "secondaryTypes": secondaires, "duration": ms, "releaseDate": "2001-03-12T00:00:00Z", "artist": artiste}));
            }
        }
        b.synchro().await;
        Some(Banc { b, lb })
    }

    impl Banc {
        fn regler(&mut self, f: impl FnOnce(&mut Config)) {
            let mut cfg = self.b.e.state.config().clone();
            f(&mut cfg);
            self.b.e.state = AppState::new(cfg, self.b.e.state.pool().clone());
            self.b.e.app = routes::routeur(self.b.e.state.clone());
        }

        async fn ecouter(&self, qui: &str) -> Value {
            let e = &self.b.e;
            let album = self.b.album("Lemonade").await["id"].clone();
            let revision = e.get(qui, "/queue").await.json()["revision"].clone();
            let avant = e.state.decouverte().terminees();
            let r = e
                .appel(
                    Some(qui),
                    "PUT",
                    "/queue",
                    Some(
                        json!({"revision": revision, "source": {"type": "album", "id": album},
                        "shuffle": true, "suggestions": "frequent"}),
                    ),
                )
                .await;
            assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
            attendre(
                || e.state.decouverte().terminees() > avant,
                "fin de la recherche de découvertes",
            )
            .await;
            r.json()
        }

        fn ecritures(&self) -> Vec<(String, String, Value)> {
            self.b
                .lidarr
                .requetes()
                .into_iter()
                .filter(|r| r.methode != "GET")
                .map(|r| (r.methode.clone(), r.chemin.clone(), r.json()))
                .collect()
        }

        fn surveilles(&self) -> Vec<String> {
            let mut l: Vec<String> = self
                .b
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

        async fn importer(&self, mbid: &str, dossier: &str, titre: &str, rang: i64) {
            let chemin = format!("Daft Punk/{dossier}/01 {titre}.flac");
            generer(&self.b.chemin(&chemin), FLAC);
            {
                let mut m = self.b.monde.lock().unwrap();
                let al = m
                    .albums
                    .iter()
                    .find(|a| a["foreignAlbumId"] == mbid)
                    .cloned()
                    .unwrap();
                let (piste, fichier) = (LIDARR + 900 + 2 * rang, LIDARR + 901 + 2 * rang);
                m.pistes.push(json!({"id": piste, "albumId": al["id"], "artistId": al["artistId"],
                    "foreignRecordingId": format!("rec-{rang}"), "title": titre, "trackNumber": "1", "absoluteTrackNumber": 1,
                    "mediumNumber": 1, "duration": 2000, "hasFile": true, "trackFileId": fichier}));
                m.fichiers.push(
                    json!({"id": fichier, "artistId": al["artistId"], "albumId": al["id"],
                    "path": format!("/music/{chemin}"), "size": 1234}),
                );
            }
            self.b.synchro().await;
            crate::ajouts::suivre(&self.b.e.state).await;
        }

        async fn admin(&self) -> Value {
            let r = self.b.get("admin", "/admin/discovery").await;
            assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
            self.b.sans_fuite(&r.texte(), "/admin/discovery");
            r.json()
        }

        async fn piste(&self, titre: &str) -> String {
            sqlx::query_scalar("SELECT id::text FROM pistes WHERE titre = $1 AND present")
                .bind(titre)
                .fetch_one(self.b.e.state.pool())
                .await
                .unwrap()
        }
    }

    fn suggerees(f: &Value) -> Vec<&Value> {
        f["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["suggested"] == true)
            .collect()
    }

    #[tokio::test]
    async fn ecoute_en_aleatoire_envoie_a_lidarr_l_album_court_d_un_titre_absent_puis_le_propose() {
        let Some(t) = banc(|_| {}).await else { return };
        let f = t.ecouter("bruno").await;
        assert!(
            f["items"].as_array().unwrap().len() >= 4,
            "la file est rendue sans attendre"
        );
        let e = t.ecritures();
        assert_eq!(
            e.iter()
                .map(|x| (x.0.as_str(), x.1.as_str()))
                .collect::<Vec<_>>(),
            [
                ("POST", "/api/v1/artist"),
                ("PUT", "/api/v1/album/monitor"),
                ("POST", "/api/v1/command")
            ],
            "{e:?}"
        );
        assert_eq!(e[0].2["foreignArtistId"], DAFT);
        assert_eq!(e[2].2["name"], "AlbumSearch");
        assert_eq!(
            t.surveilles(),
            ["Musique Vol. 1"],
            "le plus court des albums studio qui portent le titre"
        );
        let l = sqlx::query(
            "SELECT origine, sub, nom, mbid, similaire_a, titre_piste, enregistrement FROM ajouts",
        )
        .fetch_all(t.b.e.state.pool())
        .await
        .unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(
            (0..7).map(|i| l[0].get::<String, _>(i)).collect::<Vec<_>>(),
            [
                "decouverte",
                "sub-bruno",
                "bruno",
                COURT,
                "Beyoncé",
                "One More Time",
                &enregistrement(1)
            ]
        );
        let a = t.admin().await;
        assert_eq!(a["enabled"], true);
        assert_eq!(a["items"].as_array().unwrap().len(), 1);
        let ajout = &a["items"][0];
        assert_eq!(
            (
                &ajout["origin"],
                &ajout["state"],
                &ajout["title"],
                &ajout["similar_to"],
                &ajout["track_title"],
                &ajout["author"]["name"]
            ),
            (
                &json!("discovery"),
                &json!("sent"),
                &json!("Musique Vol. 1"),
                &json!("Beyoncé"),
                &json!("One More Time"),
                &json!("bruno")
            )
        );
        assert_eq!(a["budget"]["used"], 1200 * 1000 * 125);
        assert_eq!(a["budget"]["limit"], 20u64 << 30);
        assert_eq!(a["budget"]["daily_limit"], 5);
        assert_eq!(a["removal_candidates"], json!([]));
        let miens = t.b.get("bruno", "/additions/mine").await.json();
        assert_eq!(miens["items"][0]["origin"], "discovery");

        let f = t.ecouter("bruno").await;
        assert!(
            suggerees(&f).iter().all(|i| i["discovery"] == false),
            "pas encore importé : rien de marqué"
        );
        t.importer(COURT, "Musique", "One More Time", 0).await;
        let piste = t.piste("One More Time").await;
        let f = t.ecouter("bruno").await;
        let s = suggerees(&f);
        assert!(!s.is_empty(), "{f}");
        assert_eq!(
            (&s[0]["track"]["id"], &s[0]["discovery"]),
            (&json!(piste), &json!(true)),
            "la découverte passe en tête des suggestions"
        );
        assert!(s[1..].iter().all(|i| i["discovery"] == false));
        for autre in ["alice", "admin"] {
            let f = t.ecouter(autre).await;
            assert!(
                f["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|i| i["track"]["id"] != piste.as_str() && i["discovery"] == false),
                "{autre} : {f}"
            );
        }
        assert_eq!(
            t.surveilles(),
            ["Homework", "Musique Vol. 1"],
            "écoutes suivantes : titre suivant, jamais deux fois le même album"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ajouts WHERE sub <> 'sub-bruno'")
                .fetch_one(t.b.e.state.pool())
                .await
                .unwrap(),
            0,
            "alice n'a pas sillon:add ; pour admin, les albums restants sont déjà en cours"
        );
        assert!(
            t.lb.appels() <= 3,
            "similaires et populaires gardés en base"
        );
    }

    #[tokio::test]
    async fn aucun_envoi_quand_un_garde_fou_s_applique() {
        let Some(mut t) = banc(|c| c.ajouts.par_jour = 0).await else {
            return;
        };
        let cas: [(&str, Regle); 5] = [
            ("budget de découverte atteint", |c| {
                c.ajouts.decouverte_budget = 1
            }),
            ("budget de découverte nul", |c| {
                c.ajouts.decouverte_budget = 0
            }),
            ("plafond global atteint", |c| c.ajouts.plafond = Some(1)),
            ("limite journalière atteinte", |c| {
                c.ajouts.decouverte_par_jour = 0
            }),
            ("interrupteur global coupé", |c| {
                c.decouverte.active = false
            }),
        ];
        let initiale = t.b.e.state.config().clone();
        for (quoi, regle) in cas {
            t.regler(|c| {
                *c = initiale.clone();
                regle(c);
            });
            t.ecouter("bruno").await;
            assert_eq!(t.ecritures(), [], "{quoi}");
        }
        t.regler(|c| *c = initiale.clone());
        t.ecouter("alice").await;
        assert_eq!(t.ecritures(), [], "personne sans sillon:add");
        let r =
            t.b.e
                .appel(
                    Some("bruno"),
                    "PUT",
                    "/preferences",
                    Some(json!({"discovery": false})),
                )
                .await;
        assert_eq!(r.statut, StatusCode::OK);
        t.ecouter("bruno").await;
        assert_eq!(t.ecritures(), [], "découverte coupée dans les préférences");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ajouts")
                .fetch_one(t.b.e.state.pool())
                .await
                .unwrap(),
            0
        );
        assert_eq!(t.admin().await["budget"]["used"], 0);

        t.b.e
            .appel(
                Some("bruno"),
                "PUT",
                "/preferences",
                Some(json!({"discovery": true})),
            )
            .await;
        t.ecouter("bruno").await;
        assert_eq!(
            t.surveilles(),
            ["Musique Vol. 1"],
            "garde-fous levés : envoi, la limite des ajouts manuels (0) ne compte pas"
        );
        t.regler(|c| {
            c.ajouts.par_jour = 3;
            c.ajouts.decouverte_par_jour = 1;
        });
        t.ecouter("bruno").await;
        assert_eq!(
            t.surveilles(),
            ["Musique Vol. 1"],
            "une découverte par jour"
        );
        let r =
            t.b.e
                .appel(
                    Some("bruno"),
                    "POST",
                    "/additions",
                    Some(json!({"kind": "album", "mbid": DISCOVERY})),
                )
                .await;
        assert_eq!(
            r.statut,
            StatusCode::CREATED,
            "l'ajout manuel a sa propre limite : {}",
            r.texte()
        );
        t.regler(|c| c.ajouts.decouverte_budget = 1);
        let r =
            t.b.e
                .appel(
                    Some("bruno"),
                    "POST",
                    "/additions",
                    Some(json!({"kind": "album", "mbid": HOMEWORK})),
                )
                .await;
        assert_eq!(
            r.statut,
            StatusCode::CREATED,
            "le budget de découverte ne bloque pas les ajouts manuels"
        );
    }

    #[tokio::test]
    async fn album_exclu_ou_refuse_jamais_redemande() {
        let Some(t) = banc(|_| {}).await else { return };
        let pool = t.b.e.state.pool();
        sqlx::query("INSERT INTO exclusions (mbid, type, titre, par) VALUES ($1, 'album', 'Musique Vol. 1', 'sub-admin')")
            .bind(COURT)
            .execute(pool)
            .await
            .unwrap();
        t.ecouter("bruno").await;
        assert_eq!(t.surveilles(), ["Discovery"], "l'album exclu est passé");
        t.importer(DISCOVERY, "Discovery", "One More Time", 0).await;
        let piste = t.piste("One More Time").await;
        let r =
            t.b.e
                .appel(
                    Some("bruno"),
                    "POST",
                    "/recommendations/dismiss",
                    Some(json!({"track": piste})),
                )
                .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT, "{}", r.texte());
        let refuses: Vec<String> =
            sqlx::query_scalar("SELECT mbid FROM refus_albums WHERE sub = 'sub-bruno'")
                .fetch_all(pool)
                .await
                .unwrap();
        assert_eq!(refuses, [DISCOVERY]);
        let f = t.ecouter("bruno").await;
        assert!(
            f["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["track"]["id"] != piste.as_str()),
            "piste refusée plus jamais suggérée"
        );
        assert_eq!(t.surveilles(), ["Discovery", "Homework"]);
        {
            let mut m = t.b.monde.lock().unwrap();
            let ids: Vec<Value> = m
                .albums
                .iter()
                .filter(|a| a["foreignAlbumId"] == DISCOVERY || a["foreignAlbumId"] == HOMEWORK)
                .map(|a| a["id"].clone())
                .collect();
            m.pistes.retain(|p| !ids.contains(&p["albumId"]));
            m.fichiers.retain(|f| !ids.contains(&f["albumId"]));
            for a in m.albums.iter_mut() {
                a["monitored"] = json!(false);
            }
        }
        t.b.synchro().await;
        sqlx::query("UPDATE ajouts SET etat = 'echec'")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO exclusions (mbid, type, titre, par) VALUES ($1, 'album', 'Homework', 'sub-admin')")
            .bind(HOMEWORK)
            .execute(pool)
            .await
            .unwrap();
        let avant = t.ecritures().len();
        t.ecouter("bruno").await;
        assert_eq!(
            t.ecritures().len(),
            avant,
            "album refusé par la personne, exclus par l'admin : rien n'est redemandé"
        );
        assert_eq!(t.surveilles(), [] as [&str; 0]);
        t.ecouter("admin").await;
        assert_eq!(
            t.surveilles(),
            ["Discovery"],
            "le refus de bruno ne vaut que pour lui"
        );
        sqlx::query("DELETE FROM exclusions")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO exclusions (mbid, type, titre, par) VALUES ($1, 'artiste', 'Daft Punk', 'sub-admin')")
            .bind(DAFT)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM ajouts")
            .execute(pool)
            .await
            .unwrap();
        let avant = t.ecritures().len();
        t.ecouter("admin").await;
        assert_eq!(t.ecritures().len(), avant, "artiste exclu");
    }

    #[tokio::test]
    async fn decouverte_likee_hors_budget_et_jamais_ecoutee_proposee_a_la_suppression() {
        let Some(t) = banc(|_| {}).await else { return };
        let pool = t.b.e.state.pool();
        t.ecouter("bruno").await;
        t.importer(COURT, "Musique", "One More Time", 0).await;
        t.ecouter("bruno").await;
        t.importer(HOMEWORK, "Homework", "Around the World", 1)
            .await;
        sqlx::query(
            "UPDATE fichiers SET taille = CASE WHEN chemin LIKE '%Around%' THEN 5000 ELSE 1234 END WHERE chemin LIKE '%Daft Punk%'",
        )
        .execute(pool)
        .await
        .unwrap();
        let a = t.admin().await;
        assert_eq!(a["items"].as_array().unwrap().len(), 2);
        assert_eq!(a["items"][0]["state"], "available", "{a}");
        let taille = |v: &Value| v["size"].as_i64().unwrap();
        assert_eq!(
            a["budget"]["used"].as_i64().unwrap(),
            a["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(taille)
                .sum::<i64>()
        );
        assert_eq!(a["budget"]["used"].as_i64().unwrap(), 5000 + 1234);
        assert_eq!(
            a["removal_candidates"],
            json!([]),
            "pendant le délai d'essai : rien"
        );
        sqlx::query("UPDATE albums SET present_le = now() - interval '31 days'")
            .execute(pool)
            .await
            .unwrap();
        let a = t.admin().await;
        let c = a["removal_candidates"].as_array().unwrap();
        assert_eq!(
            c.iter()
                .map(|x| (x["album"]["title"].as_str().unwrap(), taille(x)))
                .collect::<Vec<_>>(),
            [("Homework", 5000), ("Musique Vol. 1", 1234)],
            "jamais écoutées en entier après le délai, la plus grosse d'abord"
        );
        assert_eq!(
            (
                &c[0]["author"]["name"],
                &c[0]["similar_to"],
                &c[0]["track_title"],
                &c[0]["skips"]
            ),
            (
                &json!("bruno"),
                &json!("Beyoncé"),
                &json!("Around the World"),
                &json!(0)
            )
        );

        let ecoutee = t.piste("Around the World").await;
        let r =
            t.b.e
                .appel(
                    Some("alice"),
                    "POST",
                    "/history",
                    Some(json!({"track": ecoutee, "played": 2.0, "ended_by": "end"})),
                )
                .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        let a = t.admin().await;
        assert_eq!(
            a["removal_candidates"].as_array().unwrap().len(),
            1,
            "écoutée en entier : gardée"
        );
        assert_eq!(
            a["removal_candidates"][0]["album"]["title"],
            "Musique Vol. 1"
        );

        let likee = t.piste("One More Time").await;
        let r =
            t.b.e
                .appel(
                    Some("alice"),
                    "PUT",
                    &format!("/likes/tracks/{likee}"),
                    None,
                )
                .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let a = t.admin().await;
        assert_eq!(a["budget"]["used"], 5000, "découverte likée : hors budget");
        assert_eq!(a["removal_candidates"], json!([]));
        assert_eq!(a["items"].as_array().unwrap().len(), 1);
        let origines: Vec<(String, bool)> =
            sqlx::query_as("SELECT origine, promu_le IS NOT NULL FROM ajouts ORDER BY cree_le")
                .fetch_all(pool)
                .await
                .unwrap();
        assert_eq!(
            origines,
            [
                ("manuel".to_string(), true),
                ("decouverte".to_string(), false)
            ]
        );
        let f = t.ecouter("alice").await;
        assert!(
            f["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["discovery"] == false),
            "ajout ordinaire : plus marqué"
        );

        sqlx::query("DELETE FROM ecoutes")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("UPDATE albums SET present_le = now()")
            .execute(pool)
            .await
            .unwrap();
        for _ in 0..3 {
            t.b.e
                .appel(
                    Some("bruno"),
                    "POST",
                    "/history",
                    Some(json!({"track": ecoutee, "played": 0.2, "ended_by": "skip"})),
                )
                .await;
        }
        let a = t.admin().await;
        assert_eq!(a["removal_candidates"][0]["album"]["title"], "Homework");
        assert_eq!(
            a["removal_candidates"][0]["skips"], 3,
            "sautée plusieurs fois"
        );
        let playlist = crate::playlists::tests::creer(&t.b.e, "admin", "Soirée").await;
        let r =
            t.b.e
                .appel(
                    Some("admin"),
                    "POST",
                    &format!("/playlists/{playlist}/tracks"),
                    Some(json!({"tracks": [ecoutee]})),
                )
                .await;
        assert!(r.statut.is_success(), "{}", r.texte());
        let a = t.admin().await;
        assert_eq!(
            (&a["budget"]["used"], &a["removal_candidates"], &a["items"]),
            (&json!(0), &json!([]), &json!([])),
            "mise en playlist : ajout ordinaire"
        );
        assert_eq!(
            t.b.get("bruno", "/admin/discovery").await.statut,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn listenbrainz_en_panne_la_file_se_construit() {
        let lb = faux(|_| (500, b"panne".to_vec())).await;
        let url = lb.url.clone();
        let Some(mut t) = banc(|_| {}).await else {
            return;
        };
        t.regler(|c| c.listenbrainz = Some(reglages(&url, Duration::ZERO)));
        let f = t.ecouter("bruno").await;
        assert!(f["items"].as_array().unwrap().len() >= 4);
        assert_eq!(f["shuffle"], true);
        assert_eq!(lb.appels(), 1);
        assert_eq!(t.ecritures(), []);
        let f = t.ecouter("bruno").await;
        assert!(f["items"].as_array().unwrap().len() >= 4);
        assert_eq!(lb.appels(), 1, "en pause après l'erreur");
    }

    #[test]
    fn reglages_et_cle_des_titres() {
        let r = Reglages::from_env(&|_| None).unwrap();
        assert_eq!((r.active, r.essai), (true, chrono::Duration::days(30)));
        let env = |k: &str| match k {
            "SILLON_DECOUVERTE" => Some("false".to_string()),
            "SILLON_DECOUVERTE_ESSAI_JOURS" => Some("7".into()),
            _ => None,
        };
        let r = Reglages::from_env(&env).unwrap();
        assert_eq!((r.active, r.essai), (false, chrono::Duration::days(7)));
        for nom in ["SILLON_DECOUVERTE", "SILLON_DECOUVERTE_ESSAI_JOURS"] {
            assert!(
                Reglages::from_env(&|k| (k == nom).then(|| "x".into()))
                    .unwrap_err()
                    .contains(nom)
            );
        }
        assert_eq!(cle("Musique, vol. 1"), cle("Musique Vol. 1"));
        assert_eq!(cle("Été – 85"), "été85");
        assert_ne!(cle("Discovery"), cle("Discovery II"));
    }
}
