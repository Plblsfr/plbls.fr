use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::descripteurs::Descripteurs;

#[derive(Clone, Debug, PartialEq)]
pub struct Tache {
    pub fichier_id: Uuid,
    pub contenu_le: DateTime<Utc>,
    pub tentatives: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Avancement {
    pub attente: i64,
    pub en_cours: i64,
    pub faites: i64,
    pub erreurs: i64,
}

impl Avancement {
    pub fn finie(&self) -> bool {
        self.attente == 0 && self.en_cours == 0
    }
}

pub async fn reprendre(pool: &PgPool, tentatives_max: i32) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "UPDATE taches_analyse SET modifie_le = now(),
           etat = CASE WHEN tentatives >= $1 THEN 'erreur' ELSE 'attente' END,
           erreur = CASE WHEN tentatives >= $1 THEN 'analyse interrompue' ELSE erreur END
         WHERE etat = 'en_cours'",
    )
    .bind(tentatives_max)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

pub async fn alimenter(pool: &PgPool, version: i32) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "INSERT INTO taches_analyse (fichier_id, contenu_le, version)
         SELECT f.id, f.contenu_le, $1 FROM pistes p
         JOIN LATERAL (SELECT id, contenu_le FROM fichiers WHERE piste_id = p.id ORDER BY taille DESC, id LIMIT 1) f ON true
         ON CONFLICT (fichier_id) DO UPDATE SET contenu_le = EXCLUDED.contenu_le, version = EXCLUDED.version,
           etat = 'attente', tentatives = 0, erreur = NULL, modifie_le = now()
         WHERE taches_analyse.contenu_le <> EXCLUDED.contenu_le OR taches_analyse.version < EXCLUDED.version",
    )
    .bind(version)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

pub async fn prendre(pool: &PgPool) -> Result<Option<Tache>, sqlx::Error> {
    let l = sqlx::query(
        "UPDATE taches_analyse SET etat = 'en_cours', tentatives = tentatives + 1, modifie_le = now()
         WHERE fichier_id = (SELECT fichier_id FROM taches_analyse WHERE etat = 'attente'
                             ORDER BY tentatives, contenu_le DESC, fichier_id LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING fichier_id, contenu_le, tentatives",
    )
    .fetch_optional(pool)
    .await?;
    Ok(l.map(|l| Tache {
        fichier_id: l.get(0),
        contenu_le: l.get(1),
        tentatives: l.get(2),
    }))
}

pub async fn fichier(pool: &PgPool, t: &Tache) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let l = sqlx::query("SELECT piste_id, chemin FROM fichiers WHERE id = $1 AND contenu_le = $2")
        .bind(t.fichier_id)
        .bind(t.contenu_le)
        .fetch_optional(pool)
        .await?;
    Ok(l.map(|l| (l.get(0), l.get(1))))
}

pub async fn enregistrer(
    pool: &PgPool,
    piste: Uuid,
    contenu_le: DateTime<Utc>,
    version: i32,
    d: &Descripteurs,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO analyses (piste_id, contenu_le, version, vecteur, tempo, tonalite, majeur)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (piste_id) DO UPDATE SET contenu_le = EXCLUDED.contenu_le, version = EXCLUDED.version,
           vecteur = EXCLUDED.vecteur, tempo = EXCLUDED.tempo, tonalite = EXCLUDED.tonalite,
           majeur = EXCLUDED.majeur, analyse_le = now()",
    )
    .bind(piste)
    .bind(contenu_le)
    .bind(version)
    .bind(d.vecteur())
    .bind(d.tempo)
    .bind(i16::from(d.tonalite))
    .bind(d.majeur)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn clore(pool: &PgPool, t: &Tache, etat: &str, erreur: Option<&str>, rendre: i32) {
    let r = sqlx::query(
        "UPDATE taches_analyse SET etat = $3, erreur = $4, tentatives = tentatives - $5, modifie_le = now()
         WHERE fichier_id = $1 AND contenu_le = $2 AND etat = 'en_cours'",
    )
    .bind(t.fichier_id)
    .bind(t.contenu_le)
    .bind(etat)
    .bind(erreur)
    .bind(rendre)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("file d'analyse : {e}");
    }
}

pub async fn terminer(pool: &PgPool, t: &Tache) {
    clore(pool, t, "faite", None, 0).await;
}

pub async fn echouer(pool: &PgPool, t: &Tache, erreur: &str, tentatives_max: i32) {
    let etat = if t.tentatives >= tentatives_max {
        "erreur"
    } else {
        "attente"
    };
    clore(pool, t, etat, Some(erreur), 0).await;
}

pub async fn remettre(pool: &PgPool, t: &Tache) {
    clore(pool, t, "attente", None, 1).await;
}

pub async fn relancer(pool: &PgPool) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "UPDATE taches_analyse SET etat = 'attente', tentatives = 0, erreur = NULL, modifie_le = now() WHERE etat = 'erreur'",
    )
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

pub async fn avancement(pool: &PgPool) -> Result<Avancement, sqlx::Error> {
    let l = sqlx::query(
        "SELECT count(*) FILTER (WHERE etat = 'attente'), count(*) FILTER (WHERE etat = 'en_cours'),
                count(*) FILTER (WHERE etat = 'faite'), count(*) FILTER (WHERE etat = 'erreur') FROM taches_analyse",
    )
    .fetch_one(pool)
    .await?;
    Ok(Avancement {
        attente: l.get(0),
        en_cours: l.get(1),
        faites: l.get(2),
        erreurs: l.get(3),
    })
}
