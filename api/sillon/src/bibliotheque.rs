use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    catalogue,
    etat::AppState,
    http::{self, bd, horodatage, introuvable, invalide, ok},
    images::{self, Sujet, Taille},
};

type Params = Query<HashMap<String, String>>;

pub const COLONNES_ARTISTE: &str = "ar.id, ar.nom, ar.resume, ar.image IS NOT NULL AS a_image, ar.fond IS NOT NULL AS a_fond, ar.present,
    ARRAY(SELECT g.nom FROM artistes_genres ag JOIN genres g ON g.id = ag.genre_id WHERE ag.artiste_id = ar.id ORDER BY g.nom) AS genres,
    (SELECT count(*) FROM albums a WHERE a.artiste_id = ar.id AND a.present) AS albums_presents";

pub const COLONNES_ALBUM: &str = "al.id, al.titre, al.type, al.types_secondaires, al.sortie, al.annee, al.pochette IS NOT NULL AS a_pochette,
    al.present, al.present_le, ar.id AS artiste_id, ar.nom AS artiste_nom,
    ARRAY(SELECT g.nom FROM albums_genres ag JOIN genres g ON g.id = ag.genre_id WHERE ag.album_id = al.id ORDER BY g.nom) AS genres,
    (SELECT count(*) FROM pistes p WHERE p.album_id = al.id AND p.present) AS pistes_presentes,
    (SELECT sum(p.duree) FROM pistes p WHERE p.album_id = al.id AND p.present) AS duree_totale";

pub const COLONNES_PISTE: &str = "p.id, p.titre, p.disque, p.numero, p.duree, p.explicite, p.present,
    al.id AS album_id, al.titre AS album_titre, al.annee AS album_annee, al.pochette IS NOT NULL AS album_pochette,
    ar.id AS artiste_id, ar.nom AS artiste_nom,
    f.codec, f.conteneur, f.debit, f.frequence, f.canaux, f.bits";

pub const JOINTURES_PISTE: &str = "FROM pistes p JOIN albums al ON al.id = p.album_id JOIN artistes ar ON ar.id = p.artiste_id
    LEFT JOIN LATERAL (SELECT codec, conteneur, debit, frequence, canaux, bits FROM fichiers
                       WHERE piste_id = p.id ORDER BY taille DESC, id LIMIT 1) f ON true";

pub fn artiste_public(l: &PgRow) -> Value {
    json!({
        "id": l.get::<Uuid, _>("id"),
        "name": l.get::<String, _>("nom"),
        "overview": l.get::<Option<String>, _>("resume"),
        "genres": l.get::<Vec<String>, _>("genres"),
        "image": l.get::<bool, _>("a_image"),
        "fanart": l.get::<bool, _>("a_fond"),
        "available": l.get::<bool, _>("present"),
        "album_count": l.get::<i64, _>("albums_presents"),
    })
}

pub fn album_public(l: &PgRow) -> Value {
    json!({
        "id": l.get::<Uuid, _>("id"),
        "title": l.get::<String, _>("titre"),
        "type": l.get::<Option<String>, _>("type"),
        "secondary_types": l.get::<Vec<String>, _>("types_secondaires"),
        "release_date": l.get::<Option<NaiveDate>, _>("sortie").map(|d| d.to_string()),
        "year": l.get::<Option<i32>, _>("annee"),
        "genres": l.get::<Vec<String>, _>("genres"),
        "cover": l.get::<bool, _>("a_pochette"),
        "available": l.get::<bool, _>("present"),
        "added_at": l.get::<Option<DateTime<Utc>>, _>("present_le").as_ref().map(horodatage),
        "track_count": l.get::<i64, _>("pistes_presentes"),
        "duration": l.get::<Option<f64>, _>("duree_totale"),
        "artist": {"id": l.get::<Uuid, _>("artiste_id"), "name": l.get::<String, _>("artiste_nom")},
    })
}

pub fn piste_publique(l: &PgRow) -> Value {
    let codec: Option<String> = l.get("codec");
    json!({
        "id": l.get::<Uuid, _>("id"),
        "title": l.get::<String, _>("titre"),
        "disc": l.get::<i32, _>("disque"),
        "number": l.get::<Option<i32>, _>("numero"),
        "duration": l.get::<Option<f64>, _>("duree"),
        "explicit": l.get::<bool, _>("explicite"),
        "available": l.get::<bool, _>("present"),
        "album": {
            "id": l.get::<Uuid, _>("album_id"),
            "title": l.get::<String, _>("album_titre"),
            "year": l.get::<Option<i32>, _>("album_annee"),
            "cover": l.get::<bool, _>("album_pochette"),
        },
        "artist": {"id": l.get::<Uuid, _>("artiste_id"), "name": l.get::<String, _>("artiste_nom")},
        "format": codec.map(|c| json!({
            "codec": c,
            "container": l.get::<Option<String>, _>("conteneur"),
            "bitrate": l.get::<Option<i32>, _>("debit"),
            "sample_rate": l.get::<Option<i32>, _>("frequence"),
            "channels": l.get::<Option<i32>, _>("canaux"),
            "bits": l.get::<Option<i32>, _>("bits"),
        })),
    })
}

pub async fn pistes_par_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Value>, sqlx::Error> {
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_PISTE} {JOINTURES_PISTE} WHERE p.id = ANY($1)"
    )))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut par_id: HashMap<Uuid, Value> = lignes
        .iter()
        .map(|l| (l.get::<Uuid, _>("id"), piste_publique(l)))
        .collect();
    Ok(ids.iter().filter_map(|id| par_id.remove(id)).collect())
}

pub fn uuid(id: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(id).map_err(|_| introuvable())
}

fn limite(q: &HashMap<String, String>, defaut: i64, max: i64) -> Result<i64, Response> {
    match q.get("limit").filter(|v| !v.is_empty()) {
        None => Ok(defaut),
        Some(v) => v
            .parse::<i64>()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| invalide("limite invalide")),
    }
}

pub fn curseur(cle: &str, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(json!([cle, id]).to_string())
}

pub fn lire_curseur(q: &HashMap<String, String>) -> Result<Option<(String, Uuid)>, Response> {
    let Some(c) = q.get("cursor").filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let refus = || invalide("curseur invalide");
    if c.len() > 1024 {
        return Err(refus());
    }
    let v: Value = URL_SAFE_NO_PAD
        .decode(c)
        .ok()
        .and_then(|o| serde_json::from_slice(&o).ok())
        .ok_or_else(refus)?;
    let cle = v[0].as_str().ok_or_else(refus)?.to_string();
    let id = v[1]
        .as_str()
        .and_then(|i| Uuid::parse_str(i).ok())
        .ok_or_else(refus)?;
    Ok(Some((cle, id)))
}

fn page(lignes: &[PgRow], n: i64, public: fn(&PgRow) -> Value, cle: impl Fn(&PgRow) -> String) -> Value {
    let n = usize::try_from(n).unwrap_or(0);
    let suite = (lignes.len() > n)
        .then(|| lignes.get(n.saturating_sub(1)))
        .flatten()
        .map(|l| curseur(&cle(l), l.get("id")));
    json!({
        "items": lignes.iter().take(n).map(public).collect::<Vec<_>>(),
        "next_cursor": suite,
    })
}

fn texte_borne(q: &HashMap<String, String>, nom: &str, max: usize) -> Result<Option<String>, Response> {
    let t = q.get(nom).map(|t| t.trim()).filter(|t| !t.is_empty());
    if t.is_some_and(|t| t.chars().count() > max) {
        return Err(invalide(&format!("{nom} trop long")));
    }
    Ok(t.map(String::from))
}

pub async fn artistes(State(state): State<AppState>, Query(q): Params) -> Result<Response, Response> {
    let n = limite(&q, 50, 200)?;
    let genre = texte_borne(&q, "genre", 64)?;
    let apres = lire_curseur(&q)?;
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ARTISTE}, ar.tri FROM artistes ar
         WHERE ar.present AND ($1::text IS NULL OR (ar.tri, ar.id) > ($1, $2))
           AND ($3::text IS NULL OR EXISTS (SELECT 1 FROM artistes_genres ag JOIN genres g ON g.id = ag.genre_id
                                            WHERE ag.artiste_id = ar.id AND g.nom = $3))
         ORDER BY ar.tri, ar.id LIMIT $4"
    )))
    .bind(apres.as_ref().map(|a| a.0.clone()))
    .bind(apres.map(|a| a.1).unwrap_or_default())
    .bind(genre)
    .bind(n + 1)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&page(&lignes, n, artiste_public, |l| l.get("tri"))))
}

pub async fn artiste(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ARTISTE} FROM artistes ar WHERE ar.id = $1"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let albums = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ALBUM} FROM albums al JOIN artistes ar ON ar.id = al.artiste_id
         WHERE al.artiste_id = $1 ORDER BY al.sortie DESC NULLS LAST, al.cle, al.id"
    )))
    .bind(id)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut v = artiste_public(&l);
    v["albums"] = json!(albums.iter().map(album_public).collect::<Vec<_>>());
    Ok(ok(&v))
}

pub async fn album(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, Response> {
    let id = uuid(&id)?;
    let l = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ALBUM} FROM albums al JOIN artistes ar ON ar.id = al.artiste_id WHERE al.id = $1"
    )))
    .bind(id)
    .fetch_optional(state.pool())
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let pistes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_PISTE} {JOINTURES_PISTE} WHERE p.album_id = $1
         ORDER BY p.disque, p.numero NULLS LAST, p.cle, p.id"
    )))
    .bind(id)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut v = album_public(&l);
    v["tracks"] = json!(pistes.iter().map(piste_publique).collect::<Vec<_>>());
    Ok(ok(&v))
}

pub async fn piste(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, Response> {
    let id = uuid(&id)?;
    pistes_par_ids(state.pool(), &[id])
        .await
        .map_err(bd)?
        .pop()
        .map(|v| ok(&v))
        .ok_or_else(introuvable)
}

pub async fn recents(State(state): State<AppState>, Query(q): Params) -> Result<Response, Response> {
    let n = limite(&q, 50, 200)?;
    let apres = match lire_curseur(&q)? {
        None => None,
        Some((t, id)) => Some((
            DateTime::parse_from_rfc3339(&t)
                .map_err(|_| invalide("curseur invalide"))?
                .with_timezone(&Utc),
            id,
        )),
    };
    let lignes = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ALBUM} FROM albums al JOIN artistes ar ON ar.id = al.artiste_id
         WHERE al.present AND ($1::timestamptz IS NULL OR (al.present_le, al.id) < ($1, $2))
         ORDER BY al.present_le DESC, al.id DESC LIMIT $3"
    )))
    .bind(apres.map(|a| a.0))
    .bind(apres.map(|a| a.1).unwrap_or_default())
    .bind(n + 1)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&page(&lignes, n, album_public, |l| {
        l.get::<DateTime<Utc>, _>("present_le")
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    })))
}

pub async fn genres(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT g.nom,
           (SELECT count(*) FROM artistes_genres ag JOIN artistes a ON a.id = ag.artiste_id WHERE ag.genre_id = g.id AND a.present) AS artistes,
           (SELECT count(*) FROM albums_genres ag JOIN albums a ON a.id = ag.album_id WHERE ag.genre_id = g.id AND a.present) AS albums
         FROM genres g ORDER BY lower(g.nom), g.nom",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let items: Vec<Value> = lignes
        .iter()
        .filter(|l| l.get::<i64, _>(1) + l.get::<i64, _>(2) > 0)
        .map(|l| json!({"name": l.get::<String, _>(0), "artist_count": l.get::<i64, _>(1), "album_count": l.get::<i64, _>(2)}))
        .collect();
    Ok(ok(&json!({ "items": items })))
}

const CORRESPOND: &str = "x.present AND (x.recherche @@ plainto_tsquery('sillon', $1) OR x.cle LIKE sans_accents($2))";
const RANG: &str = "ts_rank(x.recherche, plainto_tsquery('sillon', $1)) DESC, (x.cle = sans_accents($1)) DESC, x.cle, x.id";

pub async fn recherche(State(state): State<AppState>, Query(q): Params) -> Result<Response, Response> {
    let texte = texte_borne(&q, "q", 100)?.ok_or_else(|| invalide("recherche vide"))?;
    let n = limite(&q, 10, 50)?;
    let motif = commun::sql::contient(&texte);
    let chercher = |sql: String| {
        let (texte, motif, pool) = (texte.clone(), motif.clone(), state.pool().clone());
        async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(texte)
                .bind(motif)
                .bind(n)
                .fetch_all(&pool)
                .await
        }
    };
    let artistes = chercher(format!(
        "SELECT {COLONNES_ARTISTE} FROM artistes ar JOIN artistes x ON x.id = ar.id WHERE {CORRESPOND} ORDER BY {RANG} LIMIT $3"
    ))
    .await
    .map_err(bd)?;
    let albums = chercher(format!(
        "SELECT {COLONNES_ALBUM} FROM albums al JOIN artistes ar ON ar.id = al.artiste_id JOIN albums x ON x.id = al.id
         WHERE {CORRESPOND} ORDER BY {RANG} LIMIT $3"
    ))
    .await
    .map_err(bd)?;
    let pistes = chercher(format!(
        "SELECT {COLONNES_PISTE} {JOINTURES_PISTE} JOIN pistes x ON x.id = p.id WHERE {CORRESPOND} ORDER BY {RANG} LIMIT $3"
    ))
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "artists": artistes.iter().map(artiste_public).collect::<Vec<_>>(),
        "albums": albums.iter().map(album_public).collect::<Vec<_>>(),
        "tracks": pistes.iter().map(piste_publique).collect::<Vec<_>>(),
    })))
}

async fn servir_image(
    state: &AppState,
    sujet: Sujet,
    id: Uuid,
    q: &HashMap<String, String>,
) -> Result<Response, Response> {
    let taille = Taille::lire(q.get("size").map(String::as_str))
        .ok_or_else(|| invalide("taille non proposée"))?;
    let sql = match sujet {
        Sujet::Portrait => "SELECT lidarr_id, image FROM artistes WHERE id = $1",
        Sujet::Fond => "SELECT lidarr_id, fond FROM artistes WHERE id = $1",
        Sujet::Pochette => "SELECT lidarr_id, pochette FROM albums WHERE id = $1",
    };
    let l = sqlx::query(sql)
        .bind(id)
        .fetch_optional(state.pool())
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let (Some(lidarr_id), Some(version)) = (l.get::<Option<i64>, _>(0), l.get::<Option<String>, _>(1)) else {
        return Err(introuvable());
    };
    let (mime, octets) = images::lire(state, sujet, id, lidarr_id, &version, taille)
        .await?
        .ok_or_else(introuvable)?;
    Ok(images::reponse(mime, octets))
}

pub async fn image_artiste(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Params,
) -> Result<Response, Response> {
    let sujet = match q.get("type").map(String::as_str) {
        None | Some("") | Some("poster") => Sujet::Portrait,
        Some("fanart") => Sujet::Fond,
        Some(_) => return Err(invalide("type d'image inconnu")),
    };
    servir_image(&state, sujet, uuid(&id)?, &q).await
}

pub async fn pochette(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Params,
) -> Result<Response, Response> {
    servir_image(&state, Sujet::Pochette, uuid(&id)?, &q).await
}

pub async fn ecartes(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT nom, raison, vu_le FROM fichiers_ecartes ORDER BY vu_le DESC, nom LIMIT 500",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({
        "items": lignes.iter().map(|l| json!({
            "name": l.get::<String, _>(0),
            "reason": l.get::<String, _>(1),
            "seen_at": horodatage(&l.get::<DateTime<Utc>, _>(2)),
        })).collect::<Vec<_>>(),
    })))
}

pub async fn etat(State(state): State<AppState>) -> Result<Response, Response> {
    let lignes = sqlx::query(
        "SELECT source, reussie_le, essai_le, erreur FROM synchronisations ORDER BY source",
    )
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let c = sqlx::query(
        "SELECT (SELECT count(*) FROM artistes WHERE present), (SELECT count(*) FROM albums WHERE present),
                (SELECT count(*) FROM pistes WHERE present), (SELECT count(*) FROM pistes WHERE NOT present),
                (SELECT count(*) FROM fichiers), (SELECT count(*) FROM fichiers_ecartes)",
    )
    .fetch_one(state.pool())
    .await
    .map_err(bd)?;
    let taille = catalogue::taille_totale(state.pool()).await.map_err(bd)?;
    Ok(ok(&json!({
        "sync": lignes.iter().map(|l| json!({
            "source": l.get::<String, _>(0),
            "succeeded_at": l.get::<Option<DateTime<Utc>>, _>(1).as_ref().map(horodatage),
            "attempted_at": l.get::<Option<DateTime<Utc>>, _>(2).as_ref().map(horodatage),
            "error": l.get::<Option<String>, _>(3),
        })).collect::<Vec<_>>(),
        "running": state.synchro_en_cours(),
        "configured": {
            "lidarr": state.config().lidarr.is_some(),
            "webhook": !state.config().jeton_webhook.is_empty(),
        },
        "library": {
            "artists": c.get::<i64, _>(0),
            "albums": c.get::<i64, _>(1),
            "tracks": c.get::<i64, _>(2),
            "absent_tracks": c.get::<i64, _>(3),
            "files": c.get::<i64, _>(4),
            "rejected": c.get::<i64, _>(5),
            "size": taille,
        },
    })))
}

pub async fn lancer_synchro(State(state): State<AppState>) -> Response {
    tokio::spawn(async move { catalogue::synchroniser(&state).await });
    http::json(StatusCode::ACCEPTED, &json!({ "started": true }))
}

pub fn evenement_utile(corps: &[u8]) -> bool {
    let v: Value = serde_json::from_slice(corps).unwrap_or_default();
    !matches!(v["eventType"].as_str(), Some("Test" | "Grab" | "HealthIssue" | "HealthRestored" | "ApplicationUpdate"))
}

pub async fn webhook(State(state): State<AppState>, corps: Bytes) -> Response {
    if evenement_utile(&corps) {
        tokio::spawn(async move { catalogue::synchroniser(&state).await });
    }
    http::sans_contenu()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curseur_aller_retour_et_refus() {
        let id = Uuid::new_v4();
        let q = |c: &str| HashMap::from([("cursor".to_string(), c.to_string())]);
        assert_eq!(
            lire_curseur(&q(&curseur("beyonce", id))).unwrap(),
            Some(("beyonce".to_string(), id))
        );
        assert_eq!(lire_curseur(&HashMap::new()).unwrap(), None);
        for c in ["%%%", "e30", &URL_SAFE_NO_PAD.encode("[\"a\", \"pas-un-uuid\"]"), &"a".repeat(2000)] {
            assert_eq!(lire_curseur(&q(c)).unwrap_err().status(), StatusCode::BAD_REQUEST, "{c}");
        }
    }

    #[test]
    fn evenements_du_webhook() {
        assert!(evenement_utile(br#"{"eventType": "Download"}"#));
        assert!(evenement_utile(br#"{"eventType": "TrackFileDelete"}"#));
        assert!(evenement_utile(b""));
        assert!(!evenement_utile(br#"{"eventType": "Test"}"#));
        assert!(!evenement_utile(br#"{"eventType": "Grab"}"#));
    }
}
