use chrono::{DateTime, Utc};
use commun::evenements::GenreBadge;
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::{catalogue::Domaine, droits::Droits};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Badge {
    pub key: String,
    pub kind: &'static str,
    pub title_id: String,
    pub season: Option<i32>,
    pub obtained_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,
}

fn genre_public(genre: GenreBadge) -> &'static str {
    match genre {
        GenreBadge::Film => "movie",
        GenreBadge::Saison => "season",
        GenreBadge::Serie => "series",
    }
}

pub fn cle(genre: GenreBadge, titre: &str, saison: i32) -> String {
    match genre {
        GenreBadge::Saison => format!("{}:{titre}:{saison}", genre_public(genre)),
        _ => format!("{}:{titre}", genre_public(genre)),
    }
}

pub fn lire_cle(cle: &str) -> Option<(GenreBadge, String, i32)> {
    let mut parts = cle.split(':');
    let genre = match parts.next()? {
        "movie" => GenreBadge::Film,
        "season" => GenreBadge::Saison,
        "series" => GenreBadge::Serie,
        _ => return None,
    };
    let titre = parts.next().filter(|t| !t.is_empty())?.to_string();
    let saison = match genre {
        GenreBadge::Saison => parts.next()?.parse::<i32>().ok().filter(|s| *s > 0)?,
        _ => 0,
    };
    parts.next().is_none().then_some((genre, titre, saison))
}

pub async fn enregistrer(
    tx: &mut PgConnection,
    sub: &str,
    genre: GenreBadge,
    titre: &str,
    saison: Option<u32>,
    date: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let saison = match genre {
        GenreBadge::Saison => match saison.and_then(|s| i32::try_from(s).ok()) {
            Some(s) if s > 0 => s,
            _ => return Ok(()),
        },
        _ => 0,
    };
    sqlx::query(
        "INSERT INTO badges (sub, genre, titre, saison, obtenu_le) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (sub, genre, titre, saison) DO UPDATE SET obtenu_le = LEAST(badges.obtenu_le, EXCLUDED.obtenu_le)",
    )
    .bind(sub)
    .bind(genre.nom())
    .bind(titre)
    .bind(saison)
    .bind(date)
    .execute(tx)
    .await?;
    Ok(())
}

type Ligne = (String, String, i32, DateTime<Utc>, bool);

async fn lignes(pool: &PgPool, sub: &str, partages: bool) -> Result<Vec<Ligne>, sqlx::Error> {
    sqlx::query_as(
        "SELECT genre, titre, saison, obtenu_le, partage FROM badges
         WHERE sub = $1 AND (partage OR NOT $2) ORDER BY obtenu_le DESC, genre, titre, saison DESC",
    )
    .bind(sub)
    .bind(partages)
    .fetch_all(pool)
    .await
}

fn badge((genre, titre, saison, obtenu_le, partage): Ligne, avec_partage: bool) -> Option<Badge> {
    let genre = GenreBadge::lire(&genre)?;
    Some(Badge {
        key: cle(genre, &titre, saison),
        kind: genre_public(genre),
        title_id: titre,
        season: (genre == GenreBadge::Saison).then_some(saison),
        obtained_at: obtenu_le,
        shared: avec_partage.then_some(partage),
    })
}

pub async fn reglages(
    pool: &PgPool,
    sub: &str,
    droits: &Droits,
) -> Result<Vec<Badge>, sqlx::Error> {
    if !Domaine::Entracte.accessible(droits) {
        return Ok(Vec::new());
    }
    Ok(lignes(pool, sub, false)
        .await?
        .into_iter()
        .filter_map(|l| badge(l, true))
        .collect())
}

pub async fn publics(
    pool: &PgPool,
    sub: &str,
    lecteur: &Droits,
) -> Result<Vec<Badge>, sqlx::Error> {
    if !Domaine::Entracte.accessible(lecteur) {
        return Ok(Vec::new());
    }
    Ok(lignes(pool, sub, true)
        .await?
        .into_iter()
        .filter_map(|l| badge(l, false))
        .collect())
}

pub async fn existants(pool: &PgPool, sub: &str) -> Result<Vec<String>, sqlx::Error> {
    Ok(lignes(pool, sub, false)
        .await?
        .into_iter()
        .filter_map(|l| badge(l, false).map(|b| b.key))
        .collect())
}

pub async fn partager(
    tx: &mut PgConnection,
    sub: &str,
    cle: &str,
    partage: bool,
) -> Result<(), sqlx::Error> {
    let Some((genre, titre, saison)) = lire_cle(cle) else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE badges SET partage = $5 WHERE sub = $1 AND genre = $2 AND titre = $3 AND saison = $4",
    )
    .bind(sub)
    .bind(genre.nom())
    .bind(titre)
    .bind(saison)
    .bind(partage)
    .execute(tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cles_aller_retour() {
        for (genre, saison) in [
            (GenreBadge::Film, 0),
            (GenreBadge::Saison, 3),
            (GenreBadge::Serie, 0),
        ] {
            let c = cle(genre, "t1", saison);
            assert_eq!(lire_cle(&c), Some((genre, "t1".to_string(), saison)));
        }
        assert_eq!(cle(GenreBadge::Saison, "t1", 2), "season:t1:2");
        assert_eq!(cle(GenreBadge::Film, "t1", 0), "movie:t1");
        for faux in [
            "",
            "movie",
            "movie:",
            "movie:t1:2",
            "season:t1",
            "season:t1:0",
            "season:t1:x",
            "series:t1:1",
            "episode:t1",
        ] {
            assert_eq!(lire_cle(faux), None, "{faux}");
        }
    }
}
