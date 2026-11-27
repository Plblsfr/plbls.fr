use std::{collections::HashMap, time::Instant};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    response::Response,
};
use commun::evenements::{Donnees, Evenement};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    badges,
    bibliotheque::{COLONNES, jointures, public, uuid},
    etat::AppState,
    garde::Personne,
    http::{self, bd, introuvable, invalide, objet, ok},
    sablier::{self, Lectures},
};

pub const SEUIL_VU: f64 = 0.9;
pub const DUREE_MAX: f64 = 86_400.0;
const REPRISE_MAX: i64 = 30;

pub fn valider(position: Option<f64>, duree: Option<f64>) -> Result<(f64, f64), &'static str> {
    let (Some(p), Some(d)) = (position, duree) else {
        return Err("position et durée requises");
    };
    if !(p.is_finite() && d.is_finite())
        || d <= 0.0
        || d > DUREE_MAX
        || p < 0.0
        || p > d * 1.05 + 5.0
    {
        return Err("position ou durée invalide");
    }
    Ok((p.min(d), d))
}

pub async fn enregistrer(
    pool: &PgPool,
    sub: &str,
    titre: Uuid,
    position: f64,
    duree: f64,
    regarde: f64,
) -> Result<bool, sqlx::Error> {
    let termine = position >= duree * SEUIL_VU;
    sqlx::query(
        "INSERT INTO progression (sub, titre_id, position, duree, termine, secondes_vues)
         VALUES ($1, $2, $3, $4, $5, LEAST($4, $6))
         ON CONFLICT (sub, titre_id) DO UPDATE SET position = EXCLUDED.position, duree = EXCLUDED.duree,
           termine = EXCLUDED.termine, secondes_vues = LEAST(EXCLUDED.duree, progression.secondes_vues + $6),
           maj = now()",
    )
    .bind(sub)
    .bind(titre)
    .bind(position)
    .bind(duree)
    .bind(termine)
    .bind(regarde)
    .execute(pool)
    .await?;
    if !termine {
        return Ok(false);
    }
    sqlx::query_scalar(
        "INSERT INTO vus (sub, titre_id) VALUES ($1, $2) ON CONFLICT (sub, titre_id) DO UPDATE SET vu_le = now()
         RETURNING xmax = 0",
    )
    .bind(sub)
    .bind(titre)
    .fetch_one(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn rapporter(
    pool: &PgPool,
    lectures: &Lectures,
    sub: &str,
    id: Uuid,
    position: f64,
    duree: f64,
    maintenant: Instant,
    saute: f64,
) -> Result<Option<Vec<Evenement>>, sqlx::Error> {
    let Some(titre) = sablier::titre(pool, id).await? else {
        return Ok(None);
    };
    let secondes = lectures.crediter(sub, id, position, maintenant);
    let vu = enregistrer(
        pool,
        sub,
        id,
        position,
        duree,
        f64::from(secondes).max(saute),
    )
    .await?;
    let mut evenements = Vec::new();
    if secondes > 0 {
        evenements.push(Evenement::nouveau(sub, titre.lecture(secondes)));
    }
    if vu {
        evenements.push(Evenement::nouveau(sub, titre.vu()));
        if let Some(serie) = titre.serie
            && sablier::serie_terminee(pool, sub, serie).await?
        {
            evenements.push(Evenement::nouveau(
                sub,
                Donnees::EntracteSerieTerminee {
                    serie: serie.to_string(),
                },
            ));
        }
    }
    evenements.extend(
        badges::regarder(pool, sub, id)
            .await?
            .iter()
            .map(|b| b.evenement(false)),
    );
    Ok(Some(evenements))
}

pub async fn progression(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let (position, duree) = valider(
        corps.get("position").and_then(Value::as_f64),
        corps.get("duration").and_then(Value::as_f64),
    )
    .map_err(invalide)?;
    rapporter(
        state.pool(),
        state.lectures(),
        p.sub(),
        id,
        position,
        duree,
        Instant::now(),
        0.0,
    )
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?
    .into_iter()
    .for_each(|e| state.emetteur().emettre(e));
    Ok(http::sans_contenu())
}

pub async fn reprise(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "WITH commences AS (
            SELECT pr.titre_id AS id, pr.maj FROM progression pr JOIN titres x ON x.id = pr.titre_id
            WHERE pr.sub = $1 AND NOT pr.termine AND pr.position > 0 AND x.present
         ),
         dernier AS (
            SELECT DISTINCT ON (e.serie_id) e.serie_id, e.saison, e.episode, v.vu_le
            FROM vus v JOIN titres e ON e.id = v.titre_id
            WHERE v.sub = $1 AND e.genre = 'episode'
            ORDER BY e.serie_id, v.vu_le DESC
         ),
         suivants AS (
            SELECT DISTINCT ON (d.serie_id) n.id, d.vu_le AS maj
            FROM dernier d JOIN titres n ON n.serie_id = d.serie_id AND n.present
              AND (n.saison, n.episode) > (d.saison, d.episode)
            WHERE NOT EXISTS (SELECT 1 FROM vus w WHERE w.sub = $1 AND w.titre_id = n.id)
              AND NOT EXISTS (SELECT 1 FROM commences c JOIN titres ce ON ce.id = c.id WHERE ce.serie_id = d.serie_id)
            ORDER BY d.serie_id, n.saison, n.episode
         ),
         tous AS (SELECT id, maj FROM commences UNION ALL SELECT id, maj FROM suivants)
         SELECT {COLONNES}, tous.maj AS ordre {} JOIN tous ON tous.id = t.id ORDER BY tous.maj DESC LIMIT {REPRISE_MAX}",
        jointures("$1")
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(
        &json!({ "items": lignes.iter().map(public).collect::<Vec<_>>() }),
    ))
}

pub async fn historique(
    State(state): State<AppState>,
    p: Personne,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let lire = |nom: &str, defaut: i64, max: i64| -> Result<i64, Response> {
        match q.get(nom).filter(|v| !v.is_empty()) {
            None => Ok(defaut),
            Some(v) => v
                .parse::<i64>()
                .ok()
                .filter(|n| (0..=max).contains(n))
                .ok_or_else(|| invalide("pagination invalide")),
        }
    };
    let debut = lire("start", 0, 1_000_000)?;
    let limite = lire("limit", 40, 100)?.max(1);
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES}, count(*) OVER () AS total {} JOIN vus h ON h.titre_id = t.id AND h.sub = $1
         ORDER BY h.vu_le DESC OFFSET $2 LIMIT $3",
        jointures("$1")
    )))
    .bind(p.sub())
    .bind(debut)
    .bind(limite)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let total: i64 = match lignes.first() {
        Some(l) => l.get("total"),
        None => sqlx::query_scalar("SELECT count(*) FROM vus WHERE sub = $1")
            .bind(p.sub())
            .fetch_one(state.pool())
            .await
            .map_err(bd)?,
    };
    Ok(ok(
        &json!({ "items": lignes.iter().map(public).collect::<Vec<_>>(), "total": total }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bornes() {
        assert_eq!(valider(Some(10.0), Some(100.0)), Ok((10.0, 100.0)));
        assert_eq!(valider(Some(102.0), Some(100.0)), Ok((100.0, 100.0)));
        for (p, d) in [
            (Some(-1.0), Some(10.0)),
            (Some(1.0), Some(0.0)),
            (Some(1.0), None),
            (None, Some(1.0)),
            (Some(90_000.0), Some(10.0)),
            (Some(1.0), Some(100_000.0)),
            (Some(f64::NAN), Some(10.0)),
        ] {
            assert!(valider(p, d).is_err(), "{p:?} {d:?}");
        }
    }
}
