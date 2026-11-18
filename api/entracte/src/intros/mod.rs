pub mod comparaison;
pub mod empreinte;

use std::path::{Path as CheminFs, PathBuf};

use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    bibliotheque::uuid,
    config::Intro,
    etat::AppState,
    filtre,
    http::{self, bd, horodatage, introuvable, invalide, objet, ok},
};
use comparaison::Segment;
use empreinte::Outil;

pub const COMMANDE: &str = "analyser-intro";
const CLE_MODE: &str = "intros_mode";
const EXTENSIONS: [&str; 6] = ["mkv", "mp4", "m4v", "webm", "avi", "mov"];

const SIGNATURES: &str = "SELECT t.serie_id, t.saison,
       md5(string_agg(f.id::text || ':' || f.taille::text || ':' || coalesce(extract(epoch FROM f.modifie_le)::text, ''), ',' ORDER BY f.id)) AS signature,
       count(DISTINCT t.id) AS episodes
     FROM titres t JOIN fichiers f ON f.titre_id = t.id
     WHERE t.genre = 'episode'
     GROUP BY t.serie_id, t.saison";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Manuel,
    Automatique,
}

impl Mode {
    pub fn code(self) -> &'static str {
        match self {
            Mode::Manuel => "manuel",
            Mode::Automatique => "automatique",
        }
    }

    pub fn lire(texte: &str) -> Option<Self> {
        match texte {
            "manuel" => Some(Mode::Manuel),
            "automatique" => Some(Mode::Automatique),
            _ => None,
        }
    }
}

pub async fn mode(pool: &PgPool) -> Result<Mode, sqlx::Error> {
    let v: Option<String> = sqlx::query_scalar("SELECT valeur FROM reglages WHERE cle = $1")
        .bind(CLE_MODE)
        .fetch_optional(pool)
        .await?;
    Ok(v.as_deref().and_then(Mode::lire).unwrap_or(Mode::Manuel))
}

pub async fn regler_mode(pool: &PgPool, mode: Mode) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO reglages (cle, valeur) VALUES ($1, $2)
         ON CONFLICT (cle) DO UPDATE SET valeur = EXCLUDED.valeur",
    )
    .bind(CLE_MODE)
    .bind(mode.code())
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn oublier_si_change(
    pool: &PgPool,
    chemin: &str,
    taille: i64,
    modifie: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    for table in ["intros", "empreintes"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE fichier_id = (SELECT id FROM fichiers WHERE chemin = $1
               AND (taille <> $2 OR modifie_le IS DISTINCT FROM $3))"
        )))
        .bind(chemin)
        .bind(taille)
        .bind(modifie)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn invalider(pool: &PgPool) -> Result<u64, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "WITH actuelles AS ({SIGNATURES})
         DELETE FROM intros_saisons s
         WHERE s.etat IN ('trouvee', 'rien', 'erreur')
           AND NOT EXISTS (SELECT 1 FROM actuelles a
                           WHERE a.serie_id = s.serie_id AND a.saison = s.saison AND a.signature = s.signature)"
    )))
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

pub async fn mettre_en_file(
    pool: &PgPool,
    serie: Option<Uuid>,
    saison: Option<i32>,
    refaire: bool,
) -> Result<u64, sqlx::Error> {
    let conflit = if refaire {
        "DO UPDATE SET etat = 'en_file', erreur = NULL, demande_le = now(), maj = now()
         WHERE intros_saisons.etat NOT IN ('en_file', 'en_cours')"
    } else {
        "DO NOTHING"
    };
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "WITH actuelles AS ({SIGNATURES})
         INSERT INTO intros_saisons (serie_id, saison, etat)
         SELECT serie_id, saison, 'en_file' FROM actuelles
         WHERE ($1::uuid IS NULL OR serie_id = $1) AND ($2::integer IS NULL OR saison = $2)
         ORDER BY serie_id, saison
         ON CONFLICT (serie_id, saison) {conflit}"
    )))
    .bind(serie)
    .bind(saison)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

pub async fn apres_synchro(state: &AppState) {
    let r: Result<(), sqlx::Error> = async {
        invalider(state.pool()).await?;
        if mode(state.pool()).await? == Mode::Automatique
            && mettre_en_file(state.pool(), None, None, false).await? > 0
        {
            state.reveiller_intros();
        }
        Ok(())
    }
    .await;
    if let Err(e) = r {
        tracing::warn!("intros après synchronisation : {e}");
    }
}

pub async fn reprendre(pool: &PgPool) -> Result<u64, sqlx::Error> {
    sqlx::query("UPDATE intros_saisons SET etat = 'en_file', maj = now() WHERE etat = 'en_cours'")
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

async fn prendre(pool: &PgPool) -> Result<Option<(Uuid, i32)>, sqlx::Error> {
    let l = sqlx::query(
        "UPDATE intros_saisons SET etat = 'en_cours', maj = now()
         WHERE (serie_id, saison) = (SELECT serie_id, saison FROM intros_saisons WHERE etat = 'en_file'
                                     ORDER BY demande_le, serie_id, saison LIMIT 1 FOR UPDATE SKIP LOCKED)
           AND NOT EXISTS (SELECT 1 FROM intros_saisons WHERE etat = 'en_cours')
         RETURNING serie_id, saison",
    )
    .fetch_optional(pool)
    .await?;
    Ok(l.map(|l| (l.get(0), l.get(1))))
}

struct Episode {
    titre: Uuid,
    fichier: Uuid,
    chemin: String,
    taille: i64,
    modifie: Option<DateTime<Utc>>,
}

enum Verdict {
    Trouvee(Vec<(Uuid, Uuid, Segment)>),
    Rien,
}

async fn empreinte_de(state: &AppState, outil: Outil, e: &Episode) -> Result<Vec<u32>, String> {
    let connue: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT points FROM empreintes WHERE fichier_id = $1 AND taille = $2 AND modifie_le IS NOT DISTINCT FROM $3",
    )
    .bind(e.fichier)
    .bind(e.taille)
    .bind(e.modifie)
    .fetch_optional(state.pool())
    .await
    .map_err(|e| format!("base : {e}"))?;
    if let Some(octets) = connue {
        return Ok(empreinte::depuis_octets(&octets));
    }
    let cfg = state.config();
    let points =
        empreinte::extraire(&cfg.ffmpeg, &cfg.intro, outil, CheminFs::new(&e.chemin)).await?;
    sqlx::query(
        "INSERT INTO empreintes (fichier_id, taille, modifie_le, points) VALUES ($1, $2, $3, $4)
         ON CONFLICT (fichier_id) DO UPDATE SET taille = EXCLUDED.taille, modifie_le = EXCLUDED.modifie_le,
           points = EXCLUDED.points, cree_le = now()",
    )
    .bind(e.fichier)
    .bind(e.taille)
    .bind(e.modifie)
    .bind(empreinte::en_octets(&points))
    .execute(state.pool())
    .await
    .map_err(|e| format!("base : {e}"))?;
    Ok(points)
}

async fn signature_de(pool: &PgPool, serie: Uuid, saison: i32) -> Result<String, sqlx::Error> {
    let s: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT signature FROM ({SIGNATURES}) a WHERE serie_id = $1 AND saison = $2"
    )))
    .bind(serie)
    .bind(saison)
    .fetch_optional(pool)
    .await?;
    Ok(s.unwrap_or_default())
}

async fn analyser_saison(state: &AppState, serie: Uuid, saison: i32) -> Result<Verdict, String> {
    let base = |e: sqlx::Error| format!("base : {e}");
    let lignes = sqlx::query(
        "SELECT titre, fichier, chemin, taille, modifie_le FROM (
           SELECT DISTINCT ON (t.id) t.id AS titre, t.episode, f.id AS fichier, f.chemin, f.taille, f.modifie_le
           FROM titres t JOIN fichiers f ON f.titre_id = t.id
           WHERE t.genre = 'episode' AND t.serie_id = $1 AND t.saison = $2
           ORDER BY t.id, f.taille DESC) e
         ORDER BY episode",
    )
    .bind(serie)
    .bind(saison)
    .fetch_all(state.pool())
    .await
    .map_err(base)?;
    let episodes: Vec<Episode> = lignes
        .iter()
        .map(|l| Episode {
            titre: l.get(0),
            fichier: l.get(1),
            chemin: l.get(2),
            taille: l.get(3),
            modifie: l.get(4),
        })
        .collect();
    if episodes.len() < 2 {
        return Ok(Verdict::Rien);
    }
    let outil = state
        .outil_empreinte()
        .await
        .ok_or("Chromaprint indisponible : ni muxer chromaprint dans ffmpeg, ni fpcalc")?;
    let mut lus: Vec<(&Episode, Vec<u32>)> = Vec::new();
    let mut echecs = 0usize;
    for e in &episodes {
        match empreinte_de(state, outil, e).await {
            Ok(p) => lus.push((e, p)),
            Err(m) => {
                echecs += 1;
                tracing::warn!(
                    "empreinte de {} impossible : {m}",
                    filtre::nom_de_fichier(&e.chemin)
                );
            }
        }
    }
    if lus.len() < 2 {
        return Err(format!(
            "empreinte audio impossible pour {echecs} épisode(s) sur {}",
            episodes.len()
        ));
    }
    let reglages = state.config().intro.reglages();
    let empreintes: Vec<Vec<u32>> = lus.iter().map(|(_, p)| p.clone()).collect();
    let segments =
        tokio::task::spawn_blocking(move || comparaison::analyser(&empreintes, &reglages))
            .await
            .map_err(|_| "comparaison interrompue".to_string())?;
    let trouves: Vec<(Uuid, Uuid, Segment)> = lus
        .iter()
        .zip(segments)
        .filter_map(|((e, _), s)| s.map(|s| (e.titre, e.fichier, s)))
        .collect();
    if trouves.is_empty() {
        Ok(Verdict::Rien)
    } else {
        Ok(Verdict::Trouvee(trouves))
    }
}

async fn enregistrer(
    pool: &PgPool,
    serie: Uuid,
    saison: i32,
    signature: &str,
    resultat: Result<Verdict, String>,
) -> Result<(), sqlx::Error> {
    let (etat, erreur, trouves) = match resultat {
        Ok(Verdict::Trouvee(t)) => ("trouvee", None, t),
        Ok(Verdict::Rien) => ("rien", None, Vec::new()),
        Err(m) => ("erreur", Some(m), Vec::new()),
    };
    let mut tx = pool.begin().await?;
    let maj = sqlx::query(
        "UPDATE intros_saisons SET etat = $3, signature = $4, erreur = $5, maj = now()
         WHERE serie_id = $1 AND saison = $2 AND etat = 'en_cours'",
    )
    .bind(serie)
    .bind(saison)
    .bind(etat)
    .bind(signature)
    .bind(&erreur)
    .execute(&mut *tx)
    .await?;
    if maj.rows_affected() == 0 {
        return tx.rollback().await;
    }
    if etat != "erreur" {
        sqlx::query(
            "DELETE FROM intros i USING titres t
             WHERE i.titre_id = t.id AND t.serie_id = $1 AND t.saison = $2",
        )
        .bind(serie)
        .bind(saison)
        .execute(&mut *tx)
        .await?;
        for (titre, fichier, s) in trouves {
            sqlx::query(
                "INSERT INTO intros (titre_id, fichier_id, debut, fin)
                 SELECT $1, $2, $3, $4 WHERE EXISTS (SELECT 1 FROM fichiers WHERE id = $2 AND titre_id = $1)",
            )
            .bind(titre)
            .bind(fichier)
            .bind(s.debut)
            .bind(s.fin)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await
}

pub async fn traiter_file(state: &AppState) {
    loop {
        let (serie, saison) = match prendre(state.pool()).await {
            Ok(Some(s)) => s,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("file des intros : {e}");
                return;
            }
        };
        let signature = match signature_de(state.pool(), serie, saison).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("file des intros : {e}");
                return;
            }
        };
        let resultat = analyser_saison(state, serie, saison).await;
        if let Err(m) = &resultat {
            tracing::warn!("analyse des intros, saison {saison} : {m}");
        }
        if let Err(e) = enregistrer(state.pool(), serie, saison, &signature, resultat).await {
            tracing::warn!("résultat des intros non enregistré : {e}");
            return;
        }
    }
}

pub async fn travailleur(state: AppState) {
    if let Err(e) = reprendre(state.pool()).await {
        tracing::warn!("reprise de la file des intros : {e}");
    }
    loop {
        traiter_file(&state).await;
        state.attendre_intros().await;
    }
}

pub async fn intro_de(pool: &PgPool, fichier: Uuid) -> Result<Option<Segment>, sqlx::Error> {
    let l = sqlx::query("SELECT debut, fin FROM intros WHERE fichier_id = $1")
        .bind(fichier)
        .fetch_optional(pool)
        .await?;
    Ok(l.map(|l| Segment {
        debut: l.get(0),
        fin: l.get(1),
    }))
}

pub fn en_json(s: Option<Segment>) -> Value {
    let arrondi = |v: f64| (v * 10.0).round() / 10.0;
    s.map_or(
        Value::Null,
        |s| json!({ "debut": arrondi(s.debut), "fin": arrondi(s.fin) }),
    )
}

pub async fn liste(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "WITH actuelles AS ({SIGNATURES})
         SELECT s.id, s.titre, s.annee, a.saison, a.episodes, i.etat, i.erreur, i.maj,
           (SELECT count(*) FROM intros x JOIN titres e ON e.id = x.titre_id
            WHERE e.serie_id = s.id AND e.saison = a.saison) AS trouves
         FROM actuelles a
         JOIN titres s ON s.id = a.serie_id
         LEFT JOIN intros_saisons i ON i.serie_id = a.serie_id AND i.saison = a.saison
         ORDER BY lower(s.titre), s.id, a.saison"
    )))
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut series: Vec<Value> = Vec::new();
    for l in &lignes {
        let id: Uuid = l.get(0);
        let saison = json!({
            "number": l.get::<i32, _>(3),
            "episodes": l.get::<i64, _>(4),
            "found": l.get::<i64, _>(8),
            "state": l.get::<Option<String>, _>(5).unwrap_or_else(|| "jamais".into()),
            "error": l.get::<Option<String>, _>(6),
            "updated_at": l.get::<Option<DateTime<Utc>>, _>(7).as_ref().map(horodatage),
        });
        match series.last_mut() {
            Some(s) if s["id"] == json!(id) => {
                if let Some(a) = s["seasons"].as_array_mut() {
                    a.push(saison);
                }
            }
            _ => series.push(json!({
                "id": id,
                "title": l.get::<String, _>(1),
                "year": l.get::<Option<i32>, _>(2),
                "seasons": [saison],
            })),
        }
    }
    Ok(ok(&json!({
        "mode": mode(state.pool()).await.map_err(bd)?.code(),
        "available": state.outil_empreinte().await.is_some(),
        "items": series,
    })))
}

pub async fn changer_mode(
    State(state): State<AppState>,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let m = corps
        .get("mode")
        .and_then(Value::as_str)
        .and_then(Mode::lire)
        .ok_or_else(|| invalide("mode attendu : manuel ou automatique"))?;
    regler_mode(state.pool(), m).await.map_err(bd)?;
    if m == Mode::Automatique
        && mettre_en_file(state.pool(), None, None, false)
            .await
            .map_err(bd)?
            > 0
    {
        state.reveiller_intros();
    }
    Ok(ok(&json!({ "mode": m.code() })))
}

async fn serie_existe(pool: &PgPool, id: Uuid) -> Result<(), Response> {
    let existe: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM titres WHERE id = $1 AND genre = 'serie')",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    if existe { Ok(()) } else { Err(introuvable()) }
}

pub async fn analyser(
    State(state): State<AppState>,
    Path(id): Path<String>,
    corps: Body,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let corps = objet(corps).await?;
    let saison = match corps.get("season") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalide("saison invalide"))?,
        ),
    };
    serie_existe(state.pool(), id).await?;
    let n = mettre_en_file(state.pool(), Some(id), saison, true)
        .await
        .map_err(bd)?;
    if n > 0 {
        state.reveiller_intros();
    }
    Ok(http::json(StatusCode::ACCEPTED, &json!({ "queued": n })))
}

pub async fn effacer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = uuid(&id)?;
    serie_existe(state.pool(), id).await?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    for requete in [
        "DELETE FROM intros i USING titres t WHERE i.titre_id = t.id AND t.serie_id = $1",
        "DELETE FROM empreintes e USING fichiers f, titres t
         WHERE e.fichier_id = f.id AND f.titre_id = t.id AND t.serie_id = $1",
        "DELETE FROM intros_saisons WHERE serie_id = $1",
    ] {
        sqlx::query(requete)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(bd)?;
    }
    tx.commit().await.map_err(bd)?;
    Ok(http::sans_contenu())
}

fn videos(dossier: &CheminFs) -> Result<Vec<PathBuf>, String> {
    let mut fichiers: Vec<PathBuf> = std::fs::read_dir(dossier)
        .map_err(|e| format!("dossier illisible : {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    fichiers.sort();
    Ok(fichiers)
}

pub fn horaire(secondes: f64) -> String {
    let dixiemes = (secondes.max(0.0) * 10.0).round() as u64;
    format!(
        "{:02}:{:02}.{}",
        dixiemes / 600,
        dixiemes / 10 % 60,
        dixiemes % 10
    )
}

pub fn rapport(noms: &[String], segments: &[Option<Segment>]) -> String {
    let largeur = noms.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let mut texte = String::new();
    for (nom, s) in noms.iter().zip(segments) {
        let bornes = match s {
            Some(s) => format!(
                "{} → {} ({:.0} s)",
                horaire(s.debut),
                horaire(s.fin),
                s.fin - s.debut
            ),
            None => "rien trouvé".into(),
        };
        let marge = " ".repeat(largeur - nom.chars().count());
        texte.push_str(&format!("{nom}{marge}  {bornes}\n"));
    }
    let trouves = segments.iter().filter(|s| s.is_some()).count();
    texte.push_str(&format!(
        "{trouves} intro(s) sur {} épisode(s)\n",
        segments.len()
    ));
    texte
}

pub async fn lancer(
    env: impl Fn(&str) -> Option<String>,
    dossier: Option<String>,
) -> Result<String, String> {
    let dossier = dossier.ok_or(format!(
        "usage : entracte {COMMANDE} <dossier d'une saison>"
    ))?;
    let cfg = Intro::from_env(&env)?;
    let ffmpeg = env("ENTRACTE_FFMPEG")
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "ffmpeg".into());
    let fichiers = videos(CheminFs::new(&dossier))?;
    if fichiers.len() < 2 {
        return Err(format!(
            "{} fichier(s) vidéo dans ce dossier : il en faut au moins 2",
            fichiers.len()
        ));
    }
    let outil = empreinte::outil(&ffmpeg, &cfg.fpcalc)
        .await
        .ok_or("Chromaprint indisponible : ni muxer chromaprint dans ffmpeg, ni fpcalc")?;
    eprintln!("empreintes par {}", outil.nom());
    let mut noms = Vec::new();
    let mut empreintes = Vec::new();
    for f in &fichiers {
        let nom = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let debut = std::time::Instant::now();
        match empreinte::extraire(&ffmpeg, &cfg, outil, f).await {
            Ok(p) => {
                eprintln!(
                    "{nom} : {} points en {:.1} s",
                    p.len(),
                    debut.elapsed().as_secs_f64()
                );
                noms.push(nom);
                empreintes.push(p);
            }
            Err(m) => eprintln!("{nom} : empreinte impossible ({m})"),
        }
    }
    if empreintes.len() < 2 {
        return Err("moins de 2 empreintes : aucune comparaison possible".into());
    }
    let reglages = cfg.reglages();
    let debut = std::time::Instant::now();
    let segments =
        tokio::task::spawn_blocking(move || comparaison::analyser(&empreintes, &reglages))
            .await
            .map_err(|_| "comparaison interrompue".to_string())?;
    eprintln!("comparaison en {:.1} s", debut.elapsed().as_secs_f64());
    Ok(rapport(&noms, &segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapport_et_horaires() {
        assert_eq!(horaire(0.0), "00:00.0");
        assert_eq!(horaire(83.46), "01:23.5");
        assert_eq!(horaire(-3.0), "00:00.0");
        let r = rapport(
            &["S01E01.mkv".into(), "S01E02 long.mkv".into()],
            &[
                Some(Segment {
                    debut: 62.0,
                    fin: 121.5,
                }),
                None,
            ],
        );
        assert_eq!(
            r,
            "S01E01.mkv       01:02.0 → 02:01.5 (60 s)\nS01E02 long.mkv  rien trouvé\n1 intro(s) sur 2 épisode(s)\n"
        );
    }

    #[test]
    fn modes() {
        assert_eq!(Mode::lire("automatique"), Some(Mode::Automatique));
        assert_eq!(Mode::lire(Mode::Manuel.code()), Some(Mode::Manuel));
        assert_eq!(Mode::lire("auto"), None);
        assert_eq!(en_json(None), Value::Null);
        assert_eq!(
            en_json(Some(Segment {
                debut: 12.34,
                fin: 71.06
            })),
            json!({ "debut": 12.3, "fin": 71.1 })
        );
    }
}
