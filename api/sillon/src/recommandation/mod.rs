pub mod score;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    analyse::Voisins,
    bibliotheque::{pistes_par_ids, uuid},
    etat::AppState,
    garde::Personne,
    http::{bd, introuvable, invalide, ok, sans_contenu},
    playlists::{champ_uuid, objet},
};
use score::{Candidat, Detail, Habitude, Poids};

pub const CANDIDATS: usize = 200;
pub const CANDIDATS_MIN: usize = 50;
pub const AIMEES_MAX: usize = 300;
pub const ARTISTES_CONTEXTE: usize = 30;
const SAUTS_JOURS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reglages {
    pub poids: Poids,
    pub delai_suggestion: chrono::Duration,
}

impl Reglages {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let jours: u32 = match env("SILLON_SUGGESTION_DELAI_JOURS") {
            None => 14,
            Some(v) => v
                .parse()
                .map_err(|_| format!("SILLON_SUGGESTION_DELAI_JOURS invalide : {v}"))?,
        };
        Ok(Self {
            poids: Poids::lire(&env("SILLON_RECO_POIDS").unwrap_or_default())?,
            delai_suggestion: chrono::Duration::days(i64::from(jours)),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub artiste: Uuid,
    pub album: Uuid,
    pub annee: Option<i32>,
    pub genres: Vec<i64>,
    pub artiste_mbid: String,
    pub present: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Profil {
    pub pistes: HashMap<Uuid, Habitude>,
    pub artistes: HashMap<Uuid, Habitude>,
    pub aimees: HashSet<Uuid>,
    pub artistes_aimes: HashSet<Uuid>,
    pub en_playlist: HashSet<Uuid>,
    pub refusees: HashSet<Uuid>,
    pub sautees: HashSet<Uuid>,
    pub decouvertes: HashSet<Uuid>,
}

#[derive(Clone, Debug, Default)]
pub struct Ambiance {
    pub barycentre: Option<Vec<f32>>,
    pub genres: HashSet<i64>,
    pub annee: Option<f32>,
    pub artistes: HashSet<Uuid>,
    pub similarites: HashMap<String, f32>,
    pub similarite_connue: bool,
}

#[derive(Clone, Debug)]
pub struct Donnees {
    pub voisins: Arc<Voisins>,
    pub metas: HashMap<Uuid, Meta>,
    pub profil: Profil,
    pub ambiance: Ambiance,
    pub aimees: Vec<Uuid>,
    pub distances_aimees: HashMap<Uuid, f32>,
    pub candidats: Vec<Uuid>,
    pub poids: Poids,
    pub maintenant: DateTime<Utc>,
}

impl Default for Donnees {
    fn default() -> Self {
        Self {
            voisins: Arc::default(),
            metas: HashMap::new(),
            profil: Profil::default(),
            ambiance: Ambiance::default(),
            aimees: Vec::new(),
            distances_aimees: HashMap::new(),
            candidats: Vec::new(),
            poids: Poids::default(),
            maintenant: DateTime::UNIX_EPOCH,
        }
    }
}

fn ecart(a: &[f32], b: &[f32]) -> f32 {
    let somme: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
    (somme / a.len().max(1) as f32).sqrt()
}

impl Ambiance {
    pub fn de(source: &[Uuid], metas: &HashMap<Uuid, Meta>, voisins: &Voisins) -> Self {
        let connues: Vec<&Meta> = source.iter().filter_map(|p| metas.get(p)).collect();
        let annees: Vec<f32> = connues
            .iter()
            .filter_map(|m| m.annee)
            .map(|a| a as f32)
            .collect();
        Self {
            barycentre: voisins.barycentre(source),
            genres: connues
                .iter()
                .flat_map(|m| m.genres.iter().copied())
                .collect(),
            annee: (!annees.is_empty()).then(|| annees.iter().sum::<f32>() / annees.len() as f32),
            artistes: connues.iter().map(|m| m.artiste).collect(),
            similarites: HashMap::new(),
            similarite_connue: false,
        }
    }
}

pub fn proches(voisins: &Voisins, a: &Ambiance, exclues: &HashSet<Uuid>) -> Vec<Uuid> {
    a.barycentre.as_ref().map_or_else(Vec::new, |b| {
        voisins
            .plus_proches(b, CANDIDATS, exclues)
            .into_iter()
            .map(|v| v.0)
            .collect()
    })
}

impl Donnees {
    fn distance_aimee(&self, piste: Uuid) -> Option<f32> {
        let v = self.voisins.vecteur(piste)?;
        self.aimees
            .iter()
            .filter(|l| **l != piste)
            .filter_map(|l| self.voisins.vecteur(*l))
            .map(|l| ecart(v, l))
            .min_by(f32::total_cmp)
    }

    pub fn preparer(&mut self) {
        self.distances_aimees = self
            .candidats
            .iter()
            .filter_map(|c| Some((*c, self.distance_aimee(*c)?)))
            .collect();
    }

    pub fn candidat(&self, piste: Uuid, precedente: Option<Uuid>) -> Candidat<'_> {
        let meta = self.metas.get(&piste);
        let vecteur = self.voisins.vecteur(piste);
        let a = &self.ambiance;
        let distance_aimee = self
            .distances_aimees
            .get(&piste)
            .copied()
            .or_else(|| self.distance_aimee(piste));
        let similarite = meta.and_then(|m| {
            let s = a.similarites.get(&m.artiste_mbid).copied();
            s.or(a.similarite_connue.then_some(0.0))
        });
        Candidat {
            distance_contexte: vecteur
                .zip(a.barycentre.as_deref())
                .map(|(v, b)| ecart(v, b)),
            distance_precedente: precedente.and_then(|p| self.voisins.distance(piste, p)),
            genres_communs: meta
                .filter(|m| !m.genres.is_empty() && !a.genres.is_empty())
                .map(|m| {
                    m.genres.iter().filter(|g| a.genres.contains(g)).count() as f32
                        / m.genres.len() as f32
                }),
            ecart_annees: meta
                .and_then(|m| m.annee)
                .zip(a.annee)
                .map(|(x, y)| (x as f32 - y).abs()),
            meme_artiste: meta.is_some_and(|m| a.artistes.contains(&m.artiste)),
            similarite,
            piste: self.profil.pistes.get(&piste),
            artiste: meta.and_then(|m| self.profil.artistes.get(&m.artiste)),
            aimee: self.profil.aimees.contains(&piste),
            artiste_aime: meta.is_some_and(|m| self.profil.artistes_aimes.contains(&m.artiste)),
            en_playlist: self.profil.en_playlist.contains(&piste),
            distance_aimee,
            decouverte: self.profil.decouvertes.contains(&piste)
                && self
                    .profil
                    .pistes
                    .get(&piste)
                    .is_none_or(|h| h.completes == 0),
        }
    }

    pub fn noter(&self, piste: Uuid, precedente: Option<Uuid>) -> Detail {
        score::noter(
            &self.candidat(piste, precedente),
            &self.poids,
            self.maintenant,
        )
    }

    pub fn ecartee(&self, piste: Uuid) -> Option<&'static str> {
        if self.profil.refusees.contains(&piste) {
            Some("dismissed")
        } else if self.profil.sautees.contains(&piste) {
            Some("skipped_recently")
        } else {
            None
        }
    }

    pub fn choisir(
        &self,
        precedente: Option<Uuid>,
        prises: &HashSet<Uuid>,
        artistes: &HashSet<Uuid>,
    ) -> Option<Uuid> {
        let dernier = precedente
            .and_then(|p| self.metas.get(&p))
            .map(|m| m.artiste);
        self.candidats
            .iter()
            .filter(|c| !prises.contains(c) && self.ecartee(**c).is_none())
            .map(|c| {
                let artiste = self.metas.get(c).map(|m| m.artiste);
                let nouveau = artiste.is_none_or(|a| !artistes.contains(&a) && Some(a) != dernier);
                (nouveau, self.noter(*c, precedente).total, *c)
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(b.2.cmp(&a.2)))
            .map(|c| c.2)
    }
}

async fn profil(
    pool: &PgPool,
    sub: &str,
    delai: chrono::Duration,
    maintenant: DateTime<Utc>,
) -> Result<Profil, sqlx::Error> {
    let mut p = Profil::default();
    let lignes = sqlx::query(
        "SELECT e.piste_id, p.artiste_id,
            (count(*) FILTER (WHERE e.fin = 'completed'))::int, (count(*) FILTER (WHERE e.fin = 'skipped'))::int,
            max(e.debut), max(e.debut) FILTER (WHERE e.fin = 'completed'),
            coalesce(array_agg(e.debut) FILTER (WHERE e.fin = 'skipped' AND e.debut > $2), '{}'),
            coalesce(bool_or(e.suggeree AND e.fin = 'skipped' AND e.debut > $3), false)
         FROM ecoutes e JOIN pistes p ON p.id = e.piste_id WHERE e.sub = $1 GROUP BY e.piste_id, p.artiste_id",
    )
    .bind(sub)
    .bind(maintenant - chrono::Duration::days(SAUTS_JOURS))
    .bind(maintenant - delai)
    .fetch_all(pool)
    .await?;
    for l in &lignes {
        let (piste, artiste): (Uuid, Uuid) = (l.get(0), l.get(1));
        let complete: Option<DateTime<Utc>> = l.get(5);
        let h = Habitude {
            completes: u32::try_from(l.get::<i32, _>(2)).unwrap_or(0),
            sauts: u32::try_from(l.get::<i32, _>(3)).unwrap_or(0),
            derniere: l.get(4),
            sauts_recents: l
                .get::<Vec<DateTime<Utc>>, _>(6)
                .into_iter()
                .filter(|s| complete.is_none_or(|c| *s > c))
                .collect(),
        };
        let a = p.artistes.entry(artiste).or_default();
        a.completes += h.completes;
        a.sauts += h.sauts;
        a.derniere = a.derniere.max(h.derniere);
        if l.get::<bool, _>(7) {
            p.sautees.insert(piste);
        }
        p.pistes.insert(piste, h);
    }
    let marques = sqlx::query(
        "SELECT 'aimee', cible FROM likes WHERE sub = $1 AND type = 'track'
         UNION ALL SELECT 'artiste', cible FROM likes WHERE sub = $1 AND type = 'artist'
         UNION ALL SELECT 'playlist', pp.piste_id FROM playlists_pistes pp JOIN playlists pl ON pl.id = pp.playlist_id WHERE pl.sub = $1
         UNION ALL SELECT 'refus', piste_id FROM refus WHERE sub = $1",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    for l in &marques {
        let id: Uuid = l.get(1);
        match l.get::<&str, _>(0) {
            "aimee" => p.aimees.insert(id),
            "artiste" => p.artistes_aimes.insert(id),
            "playlist" => p.en_playlist.insert(id),
            _ => p.refusees.insert(id),
        };
    }
    let decouvertes = sqlx::query(
        "SELECT al.id, p.id, (p.mbid_enregistrement = a.enregistrement OR p.cle = sans_accents(coalesce(a.titre_piste, '')))
         FROM ajouts a JOIN albums al ON al.mbid = ANY(a.albums) JOIN pistes p ON p.album_id = al.id
         WHERE a.sub = $1 AND a.origine = 'decouverte' AND a.etat <> 'supprime' AND p.present",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    let cibles: HashSet<Uuid> = decouvertes
        .iter()
        .filter(|l| l.get::<Option<bool>, _>(2) == Some(true))
        .map(|l| l.get(0))
        .collect();
    p.decouvertes = decouvertes
        .iter()
        .filter(|l| {
            l.get::<Option<bool>, _>(2) == Some(true) || !cibles.contains(&l.get::<Uuid, _>(0))
        })
        .map(|l| l.get(1))
        .collect();
    Ok(p)
}

pub async fn metas(pool: &PgPool, ids: &[Uuid]) -> Result<HashMap<Uuid, Meta>, sqlx::Error> {
    let lignes = sqlx::query(
        "SELECT p.id, p.artiste_id, p.album_id, al.annee, ar.mbid, p.present,
            ARRAY(SELECT genre_id FROM albums_genres WHERE album_id = al.id
                  UNION SELECT genre_id FROM artistes_genres WHERE artiste_id = ar.id)
         FROM pistes p JOIN albums al ON al.id = p.album_id JOIN artistes ar ON ar.id = p.artiste_id
         WHERE p.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .iter()
        .map(|l| {
            (
                l.get(0),
                Meta {
                    artiste: l.get(1),
                    album: l.get(2),
                    annee: l.get(3),
                    artiste_mbid: l.get(4),
                    present: l.get(5),
                    genres: l.get(6),
                },
            )
        })
        .collect())
}

pub fn artistes_principaux(source: &[Uuid], metas: &HashMap<Uuid, Meta>, n: usize) -> Vec<String> {
    let mut comptes: HashMap<&str, usize> = HashMap::new();
    for m in source.iter().filter_map(|p| metas.get(p)) {
        *comptes.entry(&m.artiste_mbid).or_default() += 1;
    }
    let mut l: Vec<(&str, usize)> = comptes.into_iter().collect();
    l.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    l.into_iter().take(n).map(|a| a.0.to_string()).collect()
}

async fn similarites(pool: &PgPool, a: &mut Ambiance, mbids: &[String]) -> Result<(), sqlx::Error> {
    let lignes = sqlx::query(
        "SELECT mbid, similaire, score FROM artistes_similaires WHERE mbid = ANY($1) OR similaire = ANY($1)",
    )
    .bind(mbids)
    .fetch_all(pool)
    .await?;
    for l in &lignes {
        let (source, autre, s): (String, String, f32) = (l.get(0), l.get(1), l.get(2));
        let cle = if mbids.contains(&source) {
            autre
        } else {
            source
        };
        let v = a.similarites.entry(cle).or_default();
        *v = v.max(s);
    }
    a.similarite_connue = !lignes.is_empty()
        || sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM similarites_releves WHERE genre = 'similaires' AND mbid = ANY($1))",
        )
        .bind(mbids)
        .fetch_one(pool)
        .await?;
    Ok(())
}

pub struct Demande<'a> {
    pub sub: &'a str,
    pub source: &'a [Uuid],
    pub exclues: &'a [Uuid],
    pub graine: i64,
    pub candidats: bool,
    pub maintenant: DateTime<Utc>,
}

pub async fn charger(state: &AppState, d: &Demande<'_>) -> Result<Donnees, sqlx::Error> {
    let pool = state.pool();
    let reglages = &state.config().reco;
    let voisins = state.analyse().voisins();
    let profil = profil(pool, d.sub, reglages.delai_suggestion, d.maintenant).await?;
    let mut metas = metas(pool, d.source).await?;
    let mut ambiance = Ambiance::de(d.source, &metas, &voisins);
    let principaux = artistes_principaux(d.source, &metas, ARTISTES_CONTEXTE);
    similarites(pool, &mut ambiance, &principaux).await?;
    let mut candidats = Vec::new();
    if d.candidats {
        let exclues: HashSet<Uuid> = d.source.iter().chain(d.exclues).copied().collect();
        candidats.extend(proches(&voisins, &ambiance, &exclues));
        let mut decouvertes: Vec<Uuid> = profil.decouvertes.iter().copied().collect();
        decouvertes.sort();
        candidats.extend(decouvertes);
        if candidats.len() < CANDIDATS_MIN {
            let autres: Vec<Uuid> = sqlx::query_scalar(
                "SELECT id FROM pistes WHERE present ORDER BY md5(id::text || $1) LIMIT $2",
            )
            .bind(d.graine.to_string())
            .bind((2 * CANDIDATS) as i64)
            .fetch_all(pool)
            .await?;
            candidats.extend(autres);
        }
        let mut vus = exclues;
        candidats.retain(|c| vus.insert(*c));
        let connues = self::metas(pool, &candidats).await?;
        candidats.retain(|c| connues.get(c).is_some_and(|m| m.present));
        metas.extend(connues);
    }
    let mut aimees: Vec<Uuid> = profil
        .aimees
        .iter()
        .copied()
        .filter(|l| voisins.vecteur(*l).is_some())
        .collect();
    aimees.sort();
    aimees.truncate(AIMEES_MAX);
    let mut donnees = Donnees {
        voisins,
        metas,
        profil,
        ambiance,
        aimees,
        distances_aimees: HashMap::new(),
        candidats,
        poids: reglages.poids,
        maintenant: d.maintenant,
    };
    donnees.preparer();
    Ok(donnees)
}

pub async fn similaires(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let n = match q.get("limit").filter(|v| !v.is_empty()) {
        None => 20,
        Some(v) => v
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=100).contains(n))
            .ok_or_else(|| invalide("limite invalide"))?,
    };
    let existe: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pistes WHERE id = $1)")
        .bind(id)
        .fetch_one(state.pool())
        .await
        .map_err(bd)?;
    if !existe {
        return Err(introuvable());
    }
    let proches: Vec<Uuid> = state
        .analyse()
        .voisins()
        .voisins(id, 2 * n)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.0)
        .collect();
    let mut pistes = pistes_par_ids(state.pool(), &proches).await.map_err(bd)?;
    pistes.retain(|p| p["available"] == true);
    pistes.truncate(n);
    Ok(ok(&json!({ "items": pistes })))
}

pub async fn refuser(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let piste = champ_uuid(&corps, "track")?.ok_or_else(|| invalide("track requis"))?;
    let pool = state.pool();
    let existe: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pistes WHERE id = $1)")
        .bind(piste)
        .fetch_one(pool)
        .await
        .map_err(bd)?;
    if !existe {
        return Err(introuvable());
    }
    sqlx::query("INSERT INTO refus (sub, piste_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(p.sub())
        .bind(piste)
        .execute(pool)
        .await
        .map_err(bd)?;
    sqlx::query(
        "INSERT INTO refus_albums (sub, mbid)
         SELECT DISTINCT $1, al.mbid FROM pistes pi JOIN albums al ON al.id = pi.album_id
           JOIN ajouts a ON al.mbid = ANY(a.albums)
         WHERE pi.id = $2 AND a.sub = $1 AND (a.origine = 'decouverte' OR a.promu_le IS NOT NULL)
         ON CONFLICT DO NOTHING",
    )
    .bind(p.sub())
    .bind(piste)
    .execute(pool)
    .await
    .map_err(bd)?;
    Ok(sans_contenu())
}

pub async fn expliquer(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let piste = q
        .get("track")
        .and_then(|t| Uuid::parse_str(t).ok())
        .ok_or_else(|| invalide("track : identifiant de piste attendu"))?;
    let sub = q
        .get("sub")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or_else(|| invalide("sub requis"))?;
    let pool = state.pool();
    let public = pistes_par_ids(pool, &[piste])
        .await
        .map_err(bd)?
        .pop()
        .ok_or_else(introuvable)?;
    let elements =
        sqlx::query("SELECT piste_id, suggeree FROM files_elements WHERE sub = $1 ORDER BY rang")
            .bind(sub)
            .fetch_all(pool)
            .await
            .map_err(bd)?;
    let index: Option<i32> = sqlx::query_scalar("SELECT index_courant FROM files WHERE sub = $1")
        .bind(sub)
        .fetch_optional(pool)
        .await
        .map_err(bd)?;
    let source: Vec<Uuid> = elements
        .iter()
        .filter(|e| !e.get::<bool, _>(1))
        .map(|e| e.get(0))
        .collect();
    let courante: Option<Uuid> = index
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| elements.get(i))
        .map(|e| e.get(0));
    let mut d = charger(
        &state,
        &Demande {
            sub,
            source: &source,
            exclues: &[],
            graine: 0,
            candidats: false,
            maintenant: Utc::now(),
        },
    )
    .await
    .map_err(bd)?;
    d.metas.extend(metas(pool, &[piste]).await.map_err(bd)?);
    let c = d.candidat(piste, courante);
    let detail = d.noter(piste, courante);
    let w = &d.poids;
    let exclue = if source.contains(&piste) {
        Some("in_source")
    } else if public["available"] != true {
        Some("unavailable")
    } else {
        d.ecartee(piste)
    };
    Ok(ok(&json!({
        "track": public,
        "sub": sub,
        "context": {"tracks": source.len(), "previous": courante, "analysed": c.distance_contexte.is_some()},
        "excluded": exclue,
        "weights": w,
        "signals": detail,
        "contributions": {
            "audio": w.audio * detail.audio,
            "metadata": w.metadonnees * detail.metadonnees,
            "similarity": w.similarite * detail.similarite,
            "affinity": w.affinite * detail.affinite,
            "discovery": w.decouverte * detail.decouverte,
            "skips": -w.sauts * detail.sauts,
            "recent_track": -w.piste_recente * detail.piste_recente,
            "recent_artist": -w.artiste_recent * detail.artiste_recent,
        },
        "total": detail.total,
        "inputs": {
            "distance_context": c.distance_contexte,
            "distance_previous": c.distance_precedente,
            "shared_genres": c.genres_communs,
            "year_gap": c.ecart_annees,
            "same_artist": c.meme_artiste,
            "artist_similarity": c.similarite,
            "plays": c.piste.map_or(0, |h| h.completes),
            "skips": c.piste.map_or(0, |h| h.sauts),
            "liked": c.aimee,
            "artist_liked": c.artiste_aime,
            "in_playlist": c.en_playlist,
            "distance_liked": c.distance_aimee,
            "discovery": c.decouverte,
        },
    })))
}
