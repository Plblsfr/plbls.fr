use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    arr::{self, FichierArr},
    config::chemin_local,
    etat::AppState,
    ffprobe::{self, Analyse},
    filtre,
    tmdb::{EpisodeTmdb, Fiche, Tmdb},
};

const FRAICHEUR_METADONNEES: chrono::Duration = chrono::Duration::days(7);

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

struct Synchro<'a> {
    state: &'a AppState,
    tmdb_en_erreur: Option<String>,
    saisons: HashMap<(i64, i32), Vec<EpisodeTmdb>>,
    lus: usize,
    inaccessibles: usize,
}

fn perime(date: Option<DateTime<Utc>>) -> bool {
    date.is_none_or(|d| Utc::now() - d > FRAICHEUR_METADONNEES)
}

impl Synchro<'_> {
    fn pool(&self) -> &PgPool {
        self.state.pool()
    }

    fn tmdb(&self) -> Tmdb<'_> {
        Tmdb::new(self.state.config(), self.state.http())
    }

    async fn fiche(&mut self, film: bool, tmdb_id: i64) -> Option<Fiche> {
        let t = self.tmdb();
        if !t.configure() {
            return None;
        }
        let r = if film {
            t.film(tmdb_id).await
        } else {
            t.serie(tmdb_id).await
        };
        match r {
            Ok(f) => Some(f),
            Err(crate::tmdb::Erreur::Introuvable) => None,
            Err(e) => {
                self.tmdb_en_erreur = Some(e.to_string());
                None
            }
        }
    }

    async fn titre_principal(
        &mut self,
        genre: &str,
        tmdb_id: i64,
        titre: &str,
        annee: Option<i32>,
        source_id: i64,
    ) -> Result<Uuid, sqlx::Error> {
        let colonne = if genre == "film" {
            "radarr_id"
        } else {
            "sonarr_id"
        };
        let ligne = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO titres (genre, tmdb_id, titre, annee, {colonne}) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (genre, tmdb_id) WHERE genre <> 'episode' DO UPDATE SET {colonne} = EXCLUDED.{colonne}
             RETURNING id, metadonnees_le"
        )))
        .bind(genre)
        .bind(tmdb_id)
        .bind(titre)
        .bind(annee)
        .bind(source_id)
        .fetch_one(self.pool())
        .await?;
        let id: Uuid = ligne.get(0);
        if perime(ligne.get(1))
            && let Some(f) = self.fiche(genre == "film", tmdb_id).await
        {
            self.appliquer_fiche(id, &f).await?;
            if genre == "serie" {
                for s in &f.saisons {
                    sqlx::query(
                        "INSERT INTO saisons (serie_id, numero, titre, episodes, affiche) VALUES ($1, $2, $3, $4, $5)
                         ON CONFLICT (serie_id, numero) DO UPDATE SET titre = EXCLUDED.titre,
                           episodes = EXCLUDED.episodes, affiche = EXCLUDED.affiche",
                    )
                    .bind(id)
                    .bind(s.numero)
                    .bind(&s.titre)
                    .bind(s.episodes)
                    .bind(&s.affiche)
                    .execute(self.pool())
                    .await?;
                }
            }
            if let Some(a) = &f.affiche {
                crate::images::precharger(self.state, a).await;
            }
        }
        Ok(id)
    }

    async fn appliquer_fiche(&self, id: Uuid, f: &Fiche) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE titres SET titre = CASE WHEN $2 = '' THEN titre ELSE $2 END, titre_original = $3,
               annee = coalesce($4, annee), resume = $5, genres = $6, affiche = $7, fond = $8,
               duree = coalesce(duree, $9), metadonnees_le = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(&f.titre)
        .bind(&f.titre_original)
        .bind(f.annee)
        .bind(&f.resume)
        .bind(&f.genres)
        .bind(&f.affiche)
        .bind(&f.fond)
        .bind(f.duree)
        .execute(self.pool())
        .await
        .map(|_| ())
    }

    async fn episode(
        &mut self,
        serie: Uuid,
        tmdb_serie: i64,
        saison: i32,
        numero: i32,
        titre: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let ligne = sqlx::query(
            "INSERT INTO titres (genre, tmdb_id, serie_id, saison, episode, titre) VALUES ('episode', $1, $2, $3, $4, $5)
             ON CONFLICT (serie_id, saison, episode) WHERE genre = 'episode' DO UPDATE SET tmdb_id = EXCLUDED.tmdb_id
             RETURNING id, metadonnees_le",
        )
        .bind(tmdb_serie)
        .bind(serie)
        .bind(saison)
        .bind(numero)
        .bind(if titre.is_empty() { format!("Épisode {numero}") } else { titre.to_string() })
        .fetch_one(self.pool())
        .await?;
        let id: Uuid = ligne.get(0);
        if perime(ligne.get(1)) {
            if !self.saisons.contains_key(&(tmdb_serie, saison)) && self.tmdb().configure() {
                match self.tmdb().saison(tmdb_serie, saison).await {
                    Ok(e) => {
                        self.saisons.insert((tmdb_serie, saison), e);
                    }
                    Err(crate::tmdb::Erreur::Introuvable) => {
                        self.saisons.insert((tmdb_serie, saison), Vec::new());
                    }
                    Err(e) => self.tmdb_en_erreur = Some(e.to_string()),
                }
            }
            if let Some(e) = self
                .saisons
                .get(&(tmdb_serie, saison))
                .and_then(|l| l.iter().find(|e| e.numero == numero))
            {
                let f = Fiche {
                    titre: e.titre.clone(),
                    resume: e.resume.clone(),
                    fond: e.image.clone(),
                    duree: e.duree,
                    ..Fiche::default()
                };
                self.appliquer_fiche(id, &f).await?;
            }
        }
        Ok(id)
    }

    fn medias_absents(&mut self) -> Result<(), String> {
        let tous = self.inaccessibles > 0 && self.inaccessibles == self.lus;
        self.lus = 0;
        self.inaccessibles = 0;
        if tous {
            return Err(
                "dossier des médias inaccessible : aucun fichier lisible, rien n'est retiré".into(),
            );
        }
        Ok(())
    }

    async fn fichier(&mut self, titre: Uuid, f: &FichierArr) -> Result<bool, sqlx::Error> {
        let config = self.state.config();
        let Some(local) = chemin_local(&config.chemins, &config.medias, &f.chemin) else {
            tracing::warn!("fichier hors de ENTRACTE_MEDIAS ignoré : {}", f.chemin);
            self.lus += 1;
            self.inaccessibles += 1;
            return Ok(false);
        };
        let nom = filtre::nom_de_fichier(&local).to_string();
        let pool = self.state.pool().clone();
        let ecarter = |raison: &'static str| {
            let pool = pool.clone();
            let local = local.clone();
            let nom = nom.clone();
            async move {
                sqlx::query("DELETE FROM fichiers WHERE chemin = $1")
                    .bind(&local)
                    .execute(&pool)
                    .await?;
                sqlx::query(
                    "INSERT INTO fichiers_ecartes (chemin, nom, raison) VALUES ($1, $2, $3)
                     ON CONFLICT (chemin) DO UPDATE SET raison = EXCLUDED.raison, nom = EXCLUDED.nom, vu_le = now()",
                )
                .bind(&local)
                .bind(&nom)
                .bind(raison)
                .execute(&pool)
                .await
                .map(|_| false)
            }
        };
        if let Err(r) = filtre::verdict(&local, "mkv,matroska,webm,mp4,m4v,mov,avi", true) {
            return ecarter(r.code()).await;
        }
        self.lus += 1;
        let meta = match tokio::fs::metadata(&local).await {
            Ok(m) if m.is_file() => m,
            _ => {
                self.inaccessibles += 1;
                return Ok(false);
            }
        };
        let taille = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let modifie: Option<DateTime<Utc>> = meta.modified().ok().map(DateTime::<Utc>::from);
        let connu = sqlx::query("SELECT taille, modifie_le FROM fichiers WHERE chemin = $1")
            .bind(&local)
            .fetch_optional(self.pool())
            .await?;
        if let Some(l) = connu
            && l.get::<i64, _>(0) == taille
            && l.get::<Option<DateTime<Utc>>, _>(1) == modifie
        {
            sqlx::query("UPDATE fichiers SET vu_le = now(), titre_id = $2 WHERE chemin = $1")
                .bind(&local)
                .bind(titre)
                .execute(self.pool())
                .await?;
            return Ok(true);
        }
        let chemin = PathBuf::from(&local);
        let mut analyse = match ffprobe::analyser(&self.state.config().ffprobe, &chemin).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("ffprobe {nom} : {e}");
                return ecarter("analyse_impossible").await;
            }
        };
        if let Err(r) = filtre::verdict(&local, &analyse.conteneur, analyse.video.is_some()) {
            return ecarter(r.code()).await;
        }
        analyse
            .sous_titres
            .extend(ffprobe::sous_titres_externes(Path::new(&local)).await);
        enregistrer_fichier(self.pool(), titre, &local, taille, modifie, &analyse).await?;
        sqlx::query("DELETE FROM fichiers_ecartes WHERE chemin = $1")
            .bind(&local)
            .execute(self.pool())
            .await?;
        Ok(true)
    }

    async fn radarr(&mut self) -> Result<(), String> {
        let Some(cfg) = self.state.config().radarr.clone() else {
            return Ok(());
        };
        let debut = Utc::now();
        let films = arr::Client::new(&cfg, self.state.http()).films().await?;
        for f in films {
            let Some(fichier) = &f.fichier else { continue };
            let r: Result<(), sqlx::Error> = async {
                let id = self
                    .titre_principal("film", f.tmdb_id, &f.titre, f.annee, f.id)
                    .await?;
                self.fichier(id, fichier).await?;
                Ok(())
            }
            .await;
            r.map_err(|e| format!("base : {e}"))?;
        }
        self.medias_absents()?;
        purger(self.pool(), "film", debut)
            .await
            .map_err(|e| format!("base : {e}"))
    }

    async fn sonarr(&mut self) -> Result<(), String> {
        let Some(cfg) = self.state.config().sonarr.clone() else {
            return Ok(());
        };
        let debut = Utc::now();
        let client = arr::Client::new(&cfg, self.state.http());
        for s in client.series().await? {
            let Some(tmdb_id) = s.tmdb_id else {
                tracing::info!(
                    "série {} sans identifiant TMDB dans Sonarr : ignorée",
                    s.titre
                );
                continue;
            };
            let episodes = client.episodes(s.id).await?;
            if episodes.iter().all(|e| e.fichier.is_none()) {
                continue;
            }
            let r: Result<(), sqlx::Error> = async {
                let serie = self
                    .titre_principal("serie", tmdb_id, &s.titre, s.annee, s.id)
                    .await?;
                for e in episodes.iter().filter(|e| e.saison > 0) {
                    let Some(f) = &e.fichier else { continue };
                    let id = self
                        .episode(serie, tmdb_id, e.saison, e.episode, &e.titre)
                        .await?;
                    self.fichier(id, f).await?;
                }
                Ok(())
            }
            .await;
            r.map_err(|e| format!("base : {e}"))?;
        }
        self.medias_absents()?;
        purger(self.pool(), "episode", debut)
            .await
            .map_err(|e| format!("base : {e}"))
    }
}

pub async fn enregistrer_fichier(
    pool: &PgPool,
    titre: Uuid,
    chemin: &str,
    taille: i64,
    modifie: Option<DateTime<Utc>>,
    analyse: &Analyse,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO fichiers (titre_id, chemin, taille, modifie_le, infos) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (chemin) DO UPDATE SET titre_id = EXCLUDED.titre_id, taille = EXCLUDED.taille,
           modifie_le = EXCLUDED.modifie_le, infos = EXCLUDED.infos, vu_le = now()",
    )
    .bind(titre)
    .bind(chemin)
    .bind(taille)
    .bind(modifie)
    .bind(sqlx::types::Json(analyse))
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM sous_titres WHERE fichier_id = (SELECT id FROM fichiers WHERE chemin = $1)",
    )
    .bind(chemin)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE titres SET duree = coalesce($2, duree) WHERE id = $1")
        .bind(titre)
        .bind(analyse.duree)
        .execute(pool)
        .await?;
    Ok(())
}

async fn purger(pool: &PgPool, genre: &str, debut: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM fichiers f USING titres t WHERE f.titre_id = t.id AND t.genre = $1 AND f.vu_le < $2",
    )
    .bind(genre)
    .bind(debut)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM fichiers_ecartes WHERE vu_le < $1 - interval '1 day'")
        .bind(debut)
        .execute(pool)
        .await?;
    recalculer_presence(pool).await
}

pub async fn recalculer_presence(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE titres t SET present = p.present, present_le = CASE WHEN p.present AND NOT t.present THEN now() ELSE t.present_le END
         FROM (SELECT id, EXISTS (SELECT 1 FROM fichiers f WHERE f.titre_id = titres.id) AS present
               FROM titres WHERE genre <> 'serie') p
         WHERE t.id = p.id AND t.present IS DISTINCT FROM p.present",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE titres s SET present = p.present, present_le = CASE WHEN p.present AND NOT s.present THEN now() ELSE s.present_le END
         FROM (SELECT id, EXISTS (SELECT 1 FROM titres e WHERE e.serie_id = titres.id AND e.present) AS present
               FROM titres WHERE genre = 'serie') p
         WHERE s.id = p.id AND s.present IS DISTINCT FROM p.present",
    )
    .execute(pool)
    .await
    .map(|_| ())
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
}

async fn passe(state: &AppState) {
    let mut s = Synchro {
        state,
        tmdb_en_erreur: None,
        saisons: HashMap::new(),
        lus: 0,
        inaccessibles: 0,
    };
    if state.config().radarr.is_some() {
        let r = s.radarr().await;
        noter(state.pool(), "radarr", r.err().as_deref()).await;
    }
    if state.config().sonarr.is_some() {
        let r = s.sonarr().await;
        noter(state.pool(), "sonarr", r.err().as_deref()).await;
    }
    if Tmdb::new(state.config(), state.http()).configure() {
        noter(state.pool(), "tmdb", s.tmdb_en_erreur.as_deref()).await;
    }
    crate::demandes::marquer_disponibles(state).await;
}
