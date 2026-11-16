use std::{collections::HashMap, path::Path};

use chrono::{DateTime, Datelike, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    config::chemin_local,
    etat::AppState,
    ffprobe::{self, Analyse},
    filtre::{self, Raison},
    images::{self, Sujet},
    lidarr::{self, AlbumLidarr, ArtisteLidarr, FichierLidarr, PisteLidarr},
};

pub const SOURCE: &str = "lidarr";

#[derive(Clone, Debug, PartialEq)]
pub struct FichierPiste {
    pub id: Uuid,
    pub piste_id: Uuid,
    pub chemin: String,
    pub taille: i64,
    pub conteneur: String,
    pub codec: String,
    pub debit: Option<i32>,
    pub frequence: Option<i32>,
    pub canaux: Option<i32>,
    pub bits: Option<i32>,
    pub duree: Option<f64>,
    pub contenu_le: DateTime<Utc>,
}

const COLONNES_FICHIER: &str = "id, piste_id, chemin, taille, conteneur, codec, debit, frequence, canaux, bits, duree, contenu_le";

fn fichier_piste(l: &sqlx::postgres::PgRow) -> FichierPiste {
    FichierPiste {
        id: l.get("id"),
        piste_id: l.get("piste_id"),
        chemin: l.get("chemin"),
        taille: l.get("taille"),
        conteneur: l.get("conteneur"),
        codec: l.get("codec"),
        debit: l.get("debit"),
        frequence: l.get("frequence"),
        canaux: l.get("canaux"),
        bits: l.get("bits"),
        duree: l.get("duree"),
        contenu_le: l.get("contenu_le"),
    }
}

pub async fn fichier_de_piste(
    pool: &PgPool,
    piste: Uuid,
) -> Result<Option<FichierPiste>, sqlx::Error> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_FICHIER} FROM fichiers WHERE piste_id = $1 ORDER BY taille DESC, id LIMIT 1"
    )))
    .bind(piste)
    .fetch_optional(pool)
    .await?;
    Ok(l.as_ref().map(fichier_piste))
}

pub async fn fichiers_modifies_depuis(
    pool: &PgPool,
    depuis: DateTime<Utc>,
    limite: i64,
) -> Result<Vec<FichierPiste>, sqlx::Error> {
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_FICHIER} FROM fichiers WHERE contenu_le > $1 ORDER BY contenu_le, id LIMIT $2"
    )))
    .bind(depuis)
    .bind(limite)
    .fetch_all(pool)
    .await?;
    Ok(l.iter().map(fichier_piste).collect())
}

pub async fn taille_totale(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT coalesce(sum(taille), 0)::bigint FROM fichiers")
        .fetch_one(pool)
        .await
}

pub async fn noter(pool: &PgPool, source: &str, erreur: Option<&str>) {
    let r = sqlx::query(
        "INSERT INTO synchronisations (source, reussie_le, essai_le, erreur)
         VALUES ($1, CASE WHEN $2::text IS NULL THEN now() END, now(), $2)
         ON CONFLICT (source) DO UPDATE SET essai_le = now(), erreur = $2,
           reussie_le = CASE WHEN $2::text IS NULL THEN now() ELSE synchronisations.reussie_le END",
    )
    .bind(source)
    .bind(erreur)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("synchronisation {source} non notée : {e}");
    }
    if let Some(e) = erreur {
        tracing::warn!("synchronisation {source} : {e}");
    }
}

struct Etat {
    local: String,
    nouveau: Option<(i64, Option<DateTime<Utc>>, Analyse)>,
    duree: Option<f64>,
}

struct Synchro<'a> {
    state: &'a AppState,
    lisibles: usize,
    inaccessibles: usize,
}

fn base(e: sqlx::Error) -> String {
    format!("base : {e}")
}

async fn lier_genres(
    pool: &PgPool,
    table: &str,
    colonne: &str,
    id: Uuid,
    genres: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO genres (nom) SELECT unnest($1::text[]) ON CONFLICT (nom) DO NOTHING")
        .bind(genres)
        .execute(pool)
        .await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {table} WHERE {colonne} = $1 AND genre_id NOT IN (SELECT id FROM genres WHERE nom = ANY($2))"
    )))
    .bind(id)
    .bind(genres)
    .execute(pool)
    .await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} ({colonne}, genre_id) SELECT $1, id FROM genres WHERE nom = ANY($2) ON CONFLICT DO NOTHING"
    )))
    .bind(id)
    .bind(genres)
    .execute(pool)
    .await
    .map(|_| ())
}

impl Synchro<'_> {
    fn pool(&self) -> &PgPool {
        self.state.pool()
    }

    async fn ecarter(&self, chemin: &str, raison: Raison) -> Result<Option<Etat>, sqlx::Error> {
        sqlx::query("DELETE FROM fichiers WHERE chemin = $1")
            .bind(chemin)
            .execute(self.pool())
            .await?;
        sqlx::query(
            "INSERT INTO fichiers_ecartes (chemin, nom, raison) VALUES ($1, $2, $3)
             ON CONFLICT (chemin) DO UPDATE SET raison = EXCLUDED.raison, nom = EXCLUDED.nom, vu_le = now()",
        )
        .bind(chemin)
        .bind(filtre::nom_de_fichier(chemin))
        .bind(raison.code())
        .execute(self.pool())
        .await?;
        Ok(None)
    }

    async fn fichier(&mut self, f: &FichierLidarr) -> Result<Option<Etat>, sqlx::Error> {
        let config = self.state.config();
        let Some(local) = chemin_local(&config.chemins, &config.medias, &f.chemin) else {
            tracing::warn!(
                "fichier hors de SILLON_MEDIAS écarté : {}",
                filtre::nom_de_fichier(&f.chemin)
            );
            self.inaccessibles += 1;
            return self.ecarter(&f.chemin, Raison::HorsMedias).await;
        };
        if let Err(r) = filtre::nom(&local) {
            return self.ecarter(&local, r).await;
        }
        let meta = match tokio::fs::metadata(&local).await {
            Ok(m) if m.is_file() => m,
            _ => {
                self.inaccessibles += 1;
                return Ok(None);
            }
        };
        self.lisibles += 1;
        let taille = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let modifie: Option<DateTime<Utc>> = meta
            .modified()
            .ok()
            .map(DateTime::<Utc>::from)
            .and_then(|d| {
                chrono::DurationRound::duration_trunc(d, chrono::TimeDelta::microseconds(1)).ok()
            });
        let connu = sqlx::query("SELECT taille, modifie_le, duree FROM fichiers WHERE chemin = $1")
            .bind(&local)
            .fetch_optional(self.pool())
            .await?;
        if let Some(l) = connu
            && l.get::<i64, _>(0) == taille
            && l.get::<Option<DateTime<Utc>>, _>(1) == modifie
        {
            return Ok(Some(Etat {
                local,
                nouveau: None,
                duree: l.get(2),
            }));
        }
        let analyse = match ffprobe::analyser(&config.ffprobe, Path::new(&local)).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("ffprobe {} : {e}", filtre::nom_de_fichier(&local));
                return self.ecarter(&local, Raison::AnalyseImpossible).await;
            }
        };
        if let Err(r) = filtre::verdict(&local, &analyse.conteneur, analyse.codec.is_some()) {
            return self.ecarter(&local, r).await;
        }
        Ok(Some(Etat {
            local,
            duree: analyse.duree,
            nouveau: Some((taille, modifie, analyse)),
        }))
    }

    async fn enregistrer_artiste(&self, a: &ArtisteLidarr) -> Result<Uuid, sqlx::Error> {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO artistes (mbid, lidarr_id, nom, nom_tri, resume, image, fond) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (mbid) DO UPDATE SET lidarr_id = EXCLUDED.lidarr_id, nom = EXCLUDED.nom, nom_tri = EXCLUDED.nom_tri,
               resume = EXCLUDED.resume, image = EXCLUDED.image, fond = EXCLUDED.fond
             RETURNING id",
        )
        .bind(&a.mbid)
        .bind(a.id)
        .bind(&a.nom)
        .bind(&a.nom_tri)
        .bind(&a.resume)
        .bind(&a.image)
        .bind(&a.fond)
        .fetch_one(self.pool())
        .await?;
        lier_genres(self.pool(), "artistes_genres", "artiste_id", id, &a.genres).await?;
        if let Some(v) = &a.image {
            images::precharger(self.state, Sujet::Portrait, id, a.id, v).await;
        }
        Ok(id)
    }

    async fn enregistrer_album(&self, artiste: Uuid, a: &AlbumLidarr) -> Result<Uuid, sqlx::Error> {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO albums (mbid, lidarr_id, artiste_id, titre, type, types_secondaires, sortie, annee, pochette)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (mbid) DO UPDATE SET lidarr_id = EXCLUDED.lidarr_id, artiste_id = EXCLUDED.artiste_id,
               titre = EXCLUDED.titre, type = EXCLUDED.type, types_secondaires = EXCLUDED.types_secondaires,
               sortie = EXCLUDED.sortie, annee = EXCLUDED.annee, pochette = EXCLUDED.pochette
             RETURNING id",
        )
        .bind(&a.mbid)
        .bind(a.id)
        .bind(artiste)
        .bind(&a.titre)
        .bind(&a.type_album)
        .bind(&a.types_secondaires)
        .bind(a.sortie)
        .bind(a.sortie.map(|d| d.year()))
        .bind(&a.pochette)
        .fetch_one(self.pool())
        .await?;
        lier_genres(self.pool(), "albums_genres", "album_id", id, &a.genres).await?;
        if let Some(v) = &a.pochette {
            images::precharger(self.state, Sujet::Pochette, id, a.id, v).await;
        }
        Ok(id)
    }

    async fn enregistrer_piste(
        &self,
        artiste: Uuid,
        album: Uuid,
        p: &PisteLidarr,
        duree: Option<f64>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(
            "INSERT INTO pistes (album_id, artiste_id, mbid_enregistrement, mbid_piste, lidarr_id, titre, disque, numero, duree, explicite)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (album_id, mbid_enregistrement) DO UPDATE SET artiste_id = EXCLUDED.artiste_id,
               mbid_piste = EXCLUDED.mbid_piste, lidarr_id = EXCLUDED.lidarr_id, titre = EXCLUDED.titre,
               disque = EXCLUDED.disque, numero = EXCLUDED.numero, duree = EXCLUDED.duree, explicite = EXCLUDED.explicite
             RETURNING id",
        )
        .bind(album)
        .bind(artiste)
        .bind(&p.mbid_enregistrement)
        .bind(&p.mbid_piste)
        .bind(p.id)
        .bind(&p.titre)
        .bind(p.disque)
        .bind(p.numero)
        .bind(duree.or(p.duree))
        .bind(p.explicite)
        .fetch_one(self.pool())
        .await
    }

    async fn enregistrer_fichier(
        &self,
        piste: Uuid,
        f: &FichierLidarr,
        e: &Etat,
    ) -> Result<(), sqlx::Error> {
        match &e.nouveau {
            None => {
                sqlx::query(
                    "UPDATE fichiers SET vu_le = now(), piste_id = $2, lidarr_id = $3 WHERE chemin = $1",
                )
                .bind(&e.local)
                .bind(piste)
                .bind(f.id)
                .execute(self.pool())
                .await?;
            }
            Some((taille, modifie, a)) => {
                sqlx::query(
                    "INSERT INTO fichiers (piste_id, lidarr_id, chemin, taille, modifie_le, conteneur, codec, debit, frequence, canaux, bits, duree)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT (chemin) DO UPDATE SET piste_id = EXCLUDED.piste_id, lidarr_id = EXCLUDED.lidarr_id,
                       taille = EXCLUDED.taille, modifie_le = EXCLUDED.modifie_le, contenu_le = now(), vu_le = now(),
                       conteneur = EXCLUDED.conteneur, codec = EXCLUDED.codec, debit = EXCLUDED.debit,
                       frequence = EXCLUDED.frequence, canaux = EXCLUDED.canaux, bits = EXCLUDED.bits, duree = EXCLUDED.duree",
                )
                .bind(piste)
                .bind(f.id)
                .bind(&e.local)
                .bind(taille)
                .bind(modifie)
                .bind(&a.conteneur)
                .bind(a.codec.as_deref().unwrap_or_default())
                .bind(a.debit)
                .bind(a.frequence)
                .bind(a.canaux)
                .bind(a.bits)
                .bind(a.duree)
                .execute(self.pool())
                .await?;
            }
        }
        sqlx::query("DELETE FROM fichiers_ecartes WHERE chemin = $1")
            .bind(&e.local)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    async fn artiste(
        &mut self,
        a: &ArtisteLidarr,
        albums: &[AlbumLidarr],
        pistes: &[PisteLidarr],
        fichiers: &HashMap<i64, FichierLidarr>,
    ) -> Result<(), sqlx::Error> {
        let albums: HashMap<i64, &AlbumLidarr> = albums.iter().map(|al| (al.id, al)).collect();
        let mut artiste = None;
        let mut ids_albums: HashMap<i64, Uuid> = HashMap::new();
        for p in pistes {
            let (Some(f), Some(album)) = (
                p.fichier_id.and_then(|id| fichiers.get(&id)),
                albums.get(&p.album_id),
            ) else {
                continue;
            };
            let Some(e) = self.fichier(f).await? else {
                continue;
            };
            let artiste = match artiste {
                Some(id) => id,
                None => *artiste.insert(self.enregistrer_artiste(a).await?),
            };
            let album = match ids_albums.get(&album.id) {
                Some(id) => *id,
                None => {
                    let id = self.enregistrer_album(artiste, album).await?;
                    ids_albums.insert(album.id, id);
                    id
                }
            };
            let piste = self.enregistrer_piste(artiste, album, p, e.duree).await?;
            self.enregistrer_fichier(piste, f, &e).await?;
        }
        Ok(())
    }

    async fn lidarr(&mut self) -> Result<(), String> {
        let Some(cfg) = self.state.config().lidarr.clone() else {
            return Ok(());
        };
        let client = lidarr::Client::new(&cfg, self.state.http());
        client.version().await?;
        let debut: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(self.pool())
            .await
            .map_err(base)?;
        for a in client.artistes().await? {
            if a.fichiers == Some(0) {
                continue;
            }
            let fichiers = client.fichiers(a.id).await?;
            if fichiers.is_empty() {
                continue;
            }
            let pistes = client.pistes(a.id).await?;
            let albums = client.albums(a.id).await?;
            self.artiste(&a, &albums, &pistes, &fichiers)
                .await
                .map_err(base)?;
        }
        if self.inaccessibles > 0 && self.lisibles == 0 {
            return Err(
                "dossier de la musique inaccessible : aucun fichier lisible, rien n'est retiré"
                    .into(),
            );
        }
        purger(self.pool(), debut).await.map_err(base)
    }
}

async fn purger(pool: &PgPool, debut: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM fichiers WHERE vu_le < $1")
        .bind(debut)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM fichiers_ecartes WHERE vu_le < $1 - interval '1 day'")
        .bind(debut)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn recalculer_presence(pool: &PgPool) -> Result<(), sqlx::Error> {
    for (table, existe) in [
        ("pistes", "SELECT 1 FROM fichiers f WHERE f.piste_id = x.id"),
        (
            "albums",
            "SELECT 1 FROM pistes p WHERE p.album_id = x.id AND p.present",
        ),
        (
            "artistes",
            "SELECT 1 FROM albums a WHERE a.artiste_id = x.id AND a.present",
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} t SET present = n.present,
               present_le = CASE WHEN n.present THEN now() ELSE t.present_le END
             FROM (SELECT x.id, EXISTS ({existe}) AS present FROM {table} x) n
             WHERE t.id = n.id AND t.present IS DISTINCT FROM n.present"
        )))
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn synchroniser(state: &AppState) {
    loop {
        let Ok(verrou) = state.verrou_synchro().try_lock() else {
            state.synchro_a_relancer();
            return;
        };
        state.synchro_relancee();
        passe(state).await;
        drop(verrou);
        if !state.synchro_relancee() {
            break;
        }
    }
    crate::analyse::signaler(state).await;
}

async fn passe(state: &AppState) {
    if state.config().lidarr.is_none() {
        return;
    }
    let mut s = Synchro {
        state,
        lisibles: 0,
        inaccessibles: 0,
    };
    let mut r = s.lidarr().await;
    if let Err(e) = recalculer_presence(state.pool()).await
        && r.is_ok()
    {
        r = Err(base(e));
    }
    noter(state.pool(), SOURCE, r.err().as_deref()).await;
}
