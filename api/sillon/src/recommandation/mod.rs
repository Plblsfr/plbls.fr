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
    pub reservees: HashSet<Uuid>,
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
        } else if self.profil.reservees.contains(&piste) {
            Some("discovery_of_another")
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
    p.reservees = sqlx::query_scalar(
        "SELECT p.id FROM ajouts a JOIN albums al ON al.mbid = ANY(a.albums) JOIN pistes p ON p.album_id = al.id
         WHERE a.sub <> $1 AND a.origine = 'decouverte' AND a.etat <> 'supprime'
           AND NOT EXISTS (SELECT 1 FROM ajouts b WHERE b.sub = $1 AND al.mbid = ANY(b.albums))",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?
    .into_iter()
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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use crate::{
        analyse::{DIMENSION, VERSION},
        playlists::tests::catalogue,
        testutil::{Env, env},
    };

    async fn file(e: &Env, qui: &str, corps: Value) -> Value {
        let revision = e.get(qui, "/queue").await.json()["revision"].clone();
        let mut corps = corps;
        corps["revision"] = revision;
        let methode = if corps.get("source").is_some() {
            "PUT"
        } else {
            "PATCH"
        };
        let r = e.appel(Some(qui), methode, "/queue", Some(corps)).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        r.json()
    }

    fn suggerees(f: &Value) -> Vec<(usize, String, String)> {
        f["items"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, i)| i["suggested"] == true)
            .map(|(n, i)| {
                (
                    n,
                    i["id"].as_str().unwrap().to_string(),
                    i["track"]["id"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    async fn album(e: &Env, piste: &str) -> String {
        e.get("alice", &format!("/tracks/{piste}")).await.json()["album"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn suggestions_dans_la_file_sans_analyse_ni_listenbrainz() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 30).await;
        let source = json!({"type": "album", "id": album(&e, &p[0]).await});
        let de_la_source: Vec<&String> = p.iter().step_by(3).collect();
        assert!(e.state.analyse().voisins().is_empty() && e.state.config().listenbrainz.is_none());

        let f = file(
            &e,
            "alice",
            json!({"source": source, "shuffle": true, "suggestions": "off"}),
        )
        .await;
        assert_eq!(f["items"].as_array().unwrap().len(), 10);
        assert!(suggerees(&f).is_empty(), "désactivées : aucune");
        let f = file(
            &e,
            "alice",
            json!({"source": source, "shuffle": false, "suggestions": "frequent"}),
        )
        .await;
        assert!(suggerees(&f).is_empty(), "sans aléatoire : aucune");

        let f = file(
            &e,
            "alice",
            json!({"source": source, "shuffle": true, "suggestions": "frequent"}),
        )
        .await;
        let s = suggerees(&f);
        assert_eq!(
            s.iter().map(|x| x.0).collect::<Vec<_>>(),
            [4, 9],
            "une suggestion toutes les 4 pistes de la source : {f}"
        );
        assert!(s.iter().all(|x| !de_la_source.contains(&&x.2)));
        assert_eq!(f["items"].as_array().unwrap().len(), 12);
        assert!(
            f["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["discovery"] == false)
        );
        let mut jouees: Vec<&str> = f["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["suggested"] == false)
            .map(|i| i["track"]["id"].as_str().unwrap())
            .collect();
        jouees.sort();
        let mut attendues: Vec<&str> = de_la_source.iter().map(|x| x.as_str()).collect();
        attendues.sort();
        assert_eq!(jouees, attendues, "chaque piste de la source une fois");
        let leger = file(
            &e,
            "alice",
            json!({"source": source, "shuffle": true, "suggestions": "light"}),
        )
        .await;
        assert_eq!(
            suggerees(&leger).iter().map(|x| x.0).collect::<Vec<_>>(),
            [10]
        );

        let f = file(
            &e,
            "alice",
            json!({"source": source, "shuffle": true, "suggestions": "frequent"}),
        )
        .await;
        let (sautee, refusee) = (suggerees(&f)[0].clone(), suggerees(&f)[1].clone());
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/history",
                Some(
                    json!({"track": sautee.2, "played": 3.0, "ended_by": "skip", "item": sautee.1}),
                ),
            )
            .await;
        assert_eq!(r.json()["suggested"], true, "{}", r.texte());
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/recommendations/dismiss",
                Some(json!({"track": refusee.2})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT, "{}", r.texte());
        for _ in 0..8 {
            let f = file(
                &e,
                "alice",
                json!({"source": source, "shuffle": true, "suggestions": "frequent"}),
            )
            .await;
            let s = suggerees(&f);
            assert_eq!(s.len(), 2);
            assert!(
                s.iter().all(|x| x.2 != sautee.2 && x.2 != refusee.2),
                "suggestion sautée ou refusée reproposée"
            );
        }
        let detail = e
            .get(
                "admin",
                &format!(
                    "/admin/recommendations/explain?track={}&sub=sub-alice",
                    sautee.2
                ),
            )
            .await
            .json();
        assert_eq!(detail["excluded"], "skipped_recently");
        assert!(detail["signals"]["skips"].as_f64().unwrap() > 0.4);
        sqlx::query(
            "UPDATE ecoutes SET debut = debut - interval '15 days' WHERE sub = 'sub-alice'",
        )
        .execute(e.state.pool())
        .await
        .unwrap();
        let detail = e
            .get(
                "admin",
                &format!(
                    "/admin/recommendations/explain?track={}&sub=sub-alice",
                    sautee.2
                ),
            )
            .await
            .json();
        assert_eq!(detail["excluded"], Value::Null, "délai passé : {detail}");
        let chez_bruno = file(
            &e,
            "bruno",
            json!({"source": source, "shuffle": true, "suggestions": "frequent"}),
        )
        .await;
        assert_eq!(suggerees(&chez_bruno).len(), 2);

        let f = file(&e, "alice", json!({"suggestions": "off"})).await;
        assert!(suggerees(&f).is_empty(), "coupées : retirées de la suite");
        let f = file(&e, "alice", json!({"suggestions": "light"})).await;
        assert_eq!(suggerees(&f).len(), 1);
        let f = file(&e, "alice", json!({"index": 10})).await;
        assert_eq!(f["items"][10]["suggested"], true);
        let f = file(&e, "alice", json!({"shuffle": false})).await;
        assert_eq!(suggerees(&f).len(), 1, "seule la suggestion en cours reste");
        assert_eq!(
            f["items"][f["index"].as_u64().unwrap() as usize]["suggested"],
            true
        );

        for (corps, statut) in [
            (json!({}), StatusCode::BAD_REQUEST),
            (json!({"track": "x"}), StatusCode::BAD_REQUEST),
            (json!({"track": Uuid::new_v4()}), StatusCode::NOT_FOUND),
        ] {
            let r = e
                .appel(
                    Some("alice"),
                    "POST",
                    "/recommendations/dismiss",
                    Some(corps.clone()),
                )
                .await;
            assert_eq!(r.statut, statut, "{corps}");
        }
    }

    #[tokio::test]
    async fn fenetre_suivante_planifiee_en_avancant() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 300).await;
        let f = file(
            &e,
            "alice",
            json!({"source": {"type": "search", "query": "tout"}, "tracks": p[..200], "shuffle": true, "suggestions": "frequent"}),
        )
        .await;
        let premiere = suggerees(&f);
        assert_eq!(premiere.len(), 10, "fenêtre de 50 éléments");
        let f = file(&e, "alice", json!({"index": 20})).await;
        assert_eq!(suggerees(&f), premiere, "encore 25 éléments planifiés");
        let f = file(&e, "alice", json!({"index": 40})).await;
        let seconde = suggerees(&f);
        assert_eq!(seconde[..10], premiere[..]);
        assert!(seconde.len() > 15, "{}", seconde.len());
        assert!(
            seconde.windows(2).all(|s| s[1].0 - s[0].0 == 5),
            "rythme gardé d'une fenêtre à l'autre"
        );
        assert_eq!(f["index"], 40);
        let pistes: std::collections::HashSet<&String> = seconde.iter().map(|s| &s.2).collect();
        assert_eq!(pistes.len(), seconde.len());
        assert!(seconde.iter().all(|s| !p[..200].contains(&s.2)));
    }

    async fn analyser(e: &Env, vecteurs: &[(&String, f32)]) {
        for (piste, x) in vecteurs {
            let v: Vec<f32> = (0..DIMENSION)
                .map(|d| x * (1.0 + d as f32 / 10.0))
                .collect();
            sqlx::query(
                "INSERT INTO analyses (piste_id, contenu_le, version, vecteur, tonalite, majeur) VALUES ($1, now(), $2, $3, 0, true)",
            )
            .bind(Uuid::parse_str(piste).unwrap())
            .bind(VERSION)
            .bind(v)
            .execute(e.state.pool())
            .await
            .unwrap();
        }
        crate::analyse::recharger(&e.state).await.unwrap();
    }

    #[tokio::test]
    async fn pistes_similaires_et_explication_du_score() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 12).await;
        analyser(
            &e,
            &[
                (&p[0], 0.0),
                (&p[1], 0.1),
                (&p[2], 0.3),
                (&p[3], 5.0),
                (&p[4], 5.1),
                (&p[5], -8.0),
                (&p[6], 0.2),
            ],
        )
        .await;
        sqlx::query("UPDATE pistes SET present = false WHERE id = $1")
            .bind(Uuid::parse_str(&p[6]).unwrap())
            .execute(e.state.pool())
            .await
            .unwrap();
        let r = e.get("alice", &format!("/tracks/{}/similar", p[0])).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let ids: Vec<&str> = r.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect::<Vec<_>>()
            .into_iter()
            .map(|i| p.iter().find(|x| x.as_str() == i).unwrap().as_str())
            .collect();
        assert_eq!(ids, [&p[1], &p[2], &p[3], &p[4], &p[5]]);
        assert!(
            r.json()["items"][0]["album"]["title"].is_string(),
            "format des pistes de la bibliothèque"
        );
        let r = e
            .get("alice", &format!("/tracks/{}/similar?limit=2", p[0]))
            .await;
        assert_eq!(r.json()["items"].as_array().unwrap().len(), 2);
        let r = e.get("alice", &format!("/tracks/{}/similar", p[9])).await;
        assert_eq!(r.json(), json!({"items": []}), "piste non analysée");
        for (chemin, statut) in [
            (
                format!("/tracks/{}/similar?limit=0", p[0]),
                StatusCode::BAD_REQUEST,
            ),
            (
                format!("/tracks/{}/similar?limit=101", p[0]),
                StatusCode::BAD_REQUEST,
            ),
            (
                format!("/tracks/{}/similar", Uuid::new_v4()),
                StatusCode::NOT_FOUND,
            ),
            ("/tracks/x/similar".to_string(), StatusCode::NOT_FOUND),
        ] {
            assert_eq!(e.get("alice", &chemin).await.statut, statut, "{chemin}");
        }

        file(
            &e,
            "alice",
            json!({"source": {"type": "search", "query": "q"}, "tracks": [p[0], p[1]]}),
        )
        .await;
        let expliquer = |piste: &str| {
            let chemin = format!("/admin/recommendations/explain?track={piste}&sub=sub-alice");
            let e = &e;
            async move {
                let r = e.get("admin", &chemin).await;
                assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
                r.json()
            }
        };
        let proche = expliquer(&p[2]).await;
        let loin = expliquer(&p[5]).await;
        assert!(proche["signals"]["audio"].as_f64() > loin["signals"]["audio"].as_f64());
        assert!(proche["total"].as_f64() > loin["total"].as_f64());
        assert_eq!(
            proche["context"],
            json!({"tracks": 2, "previous": p[0], "analysed": true})
        );
        assert_eq!(proche["weights"]["audio"], 1.0);
        let somme: f64 = proche["contributions"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_f64().unwrap())
            .sum();
        assert!((somme - proche["total"].as_f64().unwrap()).abs() < 1e-4);
        assert_eq!(expliquer(&p[0]).await["excluded"], "in_source");
        assert_eq!(expliquer(&p[6]).await["excluded"], "unavailable");
        let neutre = expliquer(&p[9]).await;
        assert_eq!(
            neutre["signals"]["audio"], 0.5,
            "piste non analysée : neutre"
        );
        assert_eq!(neutre["inputs"]["distance_context"], Value::Null);

        let avant = expliquer(&p[4]).await;
        let r = e
            .appel(
                Some("alice"),
                "PUT",
                &format!("/likes/tracks/{}", p[3]),
                None,
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let apres = expliquer(&p[4]).await;
        assert!(
            apres["signals"]["affinity"].as_f64().unwrap()
                > avant["signals"]["affinity"].as_f64().unwrap() + 0.05,
            "voisine d'une piste likée : {avant} {apres}"
        );
        assert!(apres["total"].as_f64() > avant["total"].as_f64());
        assert!(apres["inputs"]["distance_liked"].as_f64().unwrap() < 0.5);
        let ailleurs = (expliquer(&p[5]).await, loin);
        assert_eq!(
            ailleurs.0["signals"]["affinity"],
            ailleurs.1["signals"]["affinity"]
        );

        for chemin in [
            "/admin/recommendations/explain".to_string(),
            format!("/admin/recommendations/explain?track={}", p[0]),
            "/admin/recommendations/explain?track=x&sub=sub-alice".to_string(),
        ] {
            assert_eq!(
                e.get("admin", &chemin).await.statut,
                StatusCode::BAD_REQUEST
            );
        }
        let r = e
            .get(
                "admin",
                &format!(
                    "/admin/recommendations/explain?track={}&sub=sub-alice",
                    Uuid::new_v4()
                ),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NOT_FOUND);
        let r = e
            .get(
                "alice",
                &format!(
                    "/admin/recommendations/explain?track={}&sub=sub-alice",
                    p[0]
                ),
            )
            .await;
        assert_eq!(r.statut, StatusCode::FORBIDDEN);
    }
}
