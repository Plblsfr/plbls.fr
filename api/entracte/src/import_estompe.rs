use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

use commun::config::nettoie;

use crate::progression::{DUREE_MAX, SEUIL_VU};

pub const COMMANDE: &str = "importer-estompe";
const LOT: usize = 100;

pub struct Jellyfin {
    pub url: String,
    pub cle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NonResolu {
    pub nom: String,
    pub jellyfin_id: String,
    pub raison: &'static str,
    pub lignes: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rapport {
    pub lignes: usize,
    pub personnes: usize,
    pub progressions: usize,
    pub gardees: usize,
    pub vus: usize,
    pub invalides: usize,
    pub non_resolus: Vec<NonResolu>,
}

impl fmt::Display for Rapport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Import depuis Estompe : {} lignes lues, {} personnes.",
            self.lignes, self.personnes
        )?;
        writeln!(
            f,
            "Progressions importées : {} ; gardées car plus récentes dans Entracte : {}.",
            self.progressions, self.gardees
        )?;
        writeln!(f, "« Déjà vus » importés : {}.", self.vus)?;
        writeln!(f, "Lignes écartées (durée invalide) : {}.", self.invalides)?;
        let lignes: usize = self.non_resolus.iter().map(|n| n.lignes).sum();
        writeln!(
            f,
            "Titres non résolus : {} ({lignes} lignes).",
            self.non_resolus.len()
        )?;
        for n in &self.non_resolus {
            writeln!(
                f,
                "- {} [{}] : {} ({} lignes)",
                n.nom, n.jellyfin_id, n.raison, n.lignes
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Element {
    nom: String,
    genre: String,
    tmdb: Option<i64>,
    serie: Option<String>,
    nom_serie: Option<String>,
    saison: Option<i32>,
    episode: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cible {
    Film(i64),
    Episode(i64, i32, i32),
}

fn cle(id: &str) -> String {
    id.chars()
        .filter(|c| *c != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn numero(v: &Value) -> Option<i32> {
    v.as_i64()
        .filter(|n| *n >= 0)
        .and_then(|n| i32::try_from(n).ok())
}

fn element(v: &Value) -> Option<(String, Element)> {
    let id = v["Id"].as_str().filter(|i| !i.is_empty())?;
    let tmdb = v["ProviderIds"].as_object().and_then(|p| {
        p.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("tmdb"))
            .and_then(|(_, v)| {
                v.as_str()
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .or_else(|| v.as_i64())
            })
            .filter(|n| *n > 0)
    });
    let texte = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    Some((
        cle(id),
        Element {
            nom: texte("Name").unwrap_or_default(),
            genre: texte("Type").unwrap_or_default(),
            tmdb,
            serie: texte("SeriesId").map(|s| cle(&s)),
            nom_serie: texte("SeriesName"),
            saison: numero(&v["ParentIndexNumber"]),
            episode: numero(&v["IndexNumber"]),
        },
    ))
}

async fn elements(
    http: &reqwest::Client,
    source: &Jellyfin,
    ids: &[String],
) -> Result<HashMap<String, Element>, String> {
    let mut sortie = HashMap::new();
    for lot in ids.chunks(LOT) {
        let rep = http
            .get(format!("{}/Items", source.url))
            .header(
                "Authorization",
                format!("MediaBrowser Token=\"{}\"", source.cle),
            )
            .query(&[
                ("Ids", lot.join(",")),
                ("Fields", "ProviderIds".into()),
                ("EnableImages", "false".into()),
            ])
            .send()
            .await
            .map_err(|e| format!("Jellyfin injoignable : {}", e.without_url()))?;
        let statut = rep.status();
        if !statut.is_success() {
            return Err(format!("Jellyfin : HTTP {}", statut.as_u16()));
        }
        let corps: Value = rep
            .json()
            .await
            .map_err(|e| format!("Jellyfin : réponse illisible : {}", e.without_url()))?;
        sortie.extend(
            corps["Items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(element),
        );
    }
    Ok(sortie)
}

fn cible(e: &Element, series: &HashMap<String, Element>) -> Result<Cible, &'static str> {
    match e.genre.as_str() {
        "Movie" => e.tmdb.map(Cible::Film).ok_or("film sans identifiant TMDB"),
        "Episode" => {
            let (Some(saison), Some(episode)) = (e.saison, e.episode) else {
                return Err("épisode sans numéro de saison ou d'épisode");
            };
            let serie = e
                .serie
                .as_ref()
                .and_then(|s| series.get(s))
                .ok_or("série absente de Jellyfin")?;
            serie
                .tmdb
                .map(|t| Cible::Episode(t, saison, episode))
                .ok_or("série sans identifiant TMDB")
        }
        _ => Err("ni film ni épisode"),
    }
}

fn nom(e: &Element) -> String {
    match (e.genre.as_str(), &e.nom_serie, e.saison, e.episode) {
        ("Episode", Some(s), Some(a), Some(b)) => format!("{s} S{a:02}E{b:02} {}", e.nom),
        _ => e.nom.clone(),
    }
}

async fn titre(entracte: &PgPool, c: Cible) -> Result<Option<Uuid>, sqlx::Error> {
    match c {
        Cible::Film(t) => {
            sqlx::query_scalar("SELECT id FROM titres WHERE genre = 'film' AND tmdb_id = $1")
                .bind(t)
                .fetch_optional(entracte)
                .await
        }
        Cible::Episode(t, s, e) => {
            sqlx::query_scalar(
                "SELECT e.id FROM titres e JOIN titres s ON s.id = e.serie_id
                 WHERE s.genre = 'serie' AND s.tmdb_id = $1 AND e.genre = 'episode' AND e.saison = $2 AND e.episode = $3",
            )
            .bind(t)
            .bind(s)
            .bind(e)
            .fetch_optional(entracte)
            .await
        }
    }
}

pub async fn lecture_seule(url: &str) -> Result<PgPool, sqlx::Error> {
    let options =
        PgConnectOptions::from_str(url)?.options([("default_transaction_read_only", "on")]);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
}

pub async fn importer(
    entracte: &PgPool,
    estompe: &PgPool,
    http: &reqwest::Client,
    source: &Jellyfin,
) -> Result<Rapport, String> {
    let bd = |e: sqlx::Error| format!("base : {e}");
    let catalogue: i64 = sqlx::query_scalar("SELECT count(*) FROM titres WHERE genre <> 'serie'")
        .fetch_one(entracte)
        .await
        .map_err(bd)?;
    if catalogue == 0 {
        return Err("catalogue d'Entracte vide : lancer une synchronisation avant l'import".into());
    }
    let lignes = sqlx::query(
        "SELECT u.keycloak_sub, i.jellyfin_id, p.position, p.duree, p.vu, p.maj
         FROM films_progression p JOIN films_items i ON i.id = p.item_id JOIN users u ON u.id = p.user_id
         ORDER BY p.maj, i.jellyfin_id, u.keycloak_sub",
    )
    .fetch_all(estompe)
    .await
    .map_err(|e| format!("base d'Estompe : {e}"))?;
    let ids: Vec<String> = lignes
        .iter()
        .map(|l| l.get::<String, _>(1))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let trouves = elements(http, source, &ids).await?;
    let manquantes: Vec<String> = trouves
        .values()
        .filter_map(|e| e.serie.clone())
        .filter(|s| !trouves.contains_key(s))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut series = elements(http, source, &manquantes).await?;
    series.extend(trouves.iter().map(|(k, v)| (k.clone(), v.clone())));
    let mut resolus: HashMap<String, Result<Uuid, (String, &'static str)>> = HashMap::new();
    for id in &ids {
        let resultat = match trouves.get(&cle(id)) {
            None => Err((String::new(), "absent de Jellyfin")),
            Some(e) => match cible(e, &series) {
                Err(r) => Err((nom(e), r)),
                Ok(c) => titre(entracte, c)
                    .await
                    .map_err(bd)?
                    .ok_or((nom(e), "absent du catalogue d'Entracte")),
            },
        };
        resolus.insert(id.clone(), resultat);
    }
    let mut rapport = Rapport {
        lignes: lignes.len(),
        ..Rapport::default()
    };
    let mut personnes = BTreeSet::new();
    let mut non_resolus: BTreeMap<String, NonResolu> = BTreeMap::new();
    let mut tx = entracte.begin().await.map_err(bd)?;
    for l in &lignes {
        let sub: String = l.get(0);
        let jellyfin_id: String = l.get(1);
        let position: f64 = l.get(2);
        let duree: f64 = l.get(3);
        let vu: bool = l.get(4);
        let maj: DateTime<Utc> = l.get(5);
        personnes.insert(sub.clone());
        let titre_id = match &resolus[&jellyfin_id] {
            Ok(t) => *t,
            Err((nom, raison)) => {
                non_resolus
                    .entry(jellyfin_id.clone())
                    .or_insert_with(|| NonResolu {
                        nom: if nom.is_empty() {
                            jellyfin_id.clone()
                        } else {
                            nom.clone()
                        },
                        jellyfin_id: jellyfin_id.clone(),
                        raison,
                        lignes: 0,
                    })
                    .lignes += 1;
                continue;
            }
        };
        if !(duree.is_finite() && duree > 0.0 && duree <= DUREE_MAX && position.is_finite()) {
            rapport.invalides += 1;
            continue;
        }
        let position = position.clamp(0.0, duree);
        let ecrite: Option<i32> = sqlx::query_scalar(
            "INSERT INTO progression (sub, titre_id, position, duree, termine, maj) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (sub, titre_id) DO UPDATE SET position = EXCLUDED.position, duree = EXCLUDED.duree,
               termine = EXCLUDED.termine, maj = EXCLUDED.maj
             WHERE progression.maj < EXCLUDED.maj
             RETURNING 1",
        )
        .bind(&sub)
        .bind(titre_id)
        .bind(position)
        .bind(duree)
        .bind(position >= duree * SEUIL_VU)
        .bind(maj)
        .fetch_optional(&mut *tx)
        .await
        .map_err(bd)?;
        if ecrite.is_some() {
            rapport.progressions += 1;
        } else {
            rapport.gardees += 1;
        }
        if vu {
            let ecrit: Option<i32> = sqlx::query_scalar(
                "INSERT INTO vus (sub, titre_id, vu_le) VALUES ($1, $2, $3)
                 ON CONFLICT (sub, titre_id) DO UPDATE SET vu_le = EXCLUDED.vu_le WHERE vus.vu_le < EXCLUDED.vu_le
                 RETURNING 1",
            )
            .bind(&sub)
            .bind(titre_id)
            .bind(maj)
            .fetch_optional(&mut *tx)
            .await
            .map_err(bd)?;
            rapport.vus += usize::from(ecrit.is_some());
        }
    }
    tx.commit().await.map_err(bd)?;
    rapport.personnes = personnes.len();
    rapport.non_resolus = non_resolus.into_values().collect();
    rapport.non_resolus.sort();
    Ok(rapport)
}

pub async fn lancer(brut: impl Fn(&str) -> Option<String>) -> Result<Rapport, String> {
    let env = |k: &str| {
        brut(k)
            .map(|v| nettoie(&v))
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("{k} manquant"))
    };
    let entracte_url = env("ENTRACTE_DATABASE_URL")?;
    let estompe_url = env("ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL")?;
    let source = Jellyfin {
        url: env("ENTRACTE_IMPORT_JELLYFIN_URL")?
            .trim_end_matches('/')
            .to_string(),
        cle: env("ENTRACTE_IMPORT_JELLYFIN_API_KEY")?,
    };
    let entracte = PgPoolOptions::new()
        .max_connections(2)
        .connect(&entracte_url)
        .await
        .map_err(|e| format!("base d'Entracte : {e}"))?;
    let estompe = lecture_seule(&estompe_url)
        .await
        .map_err(|e| format!("base d'Estompe : {e}"))?;
    importer(&entracte, &estompe, &crate::etat::client_http(), &source).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testutil::{BasePostgres, Faux, faux, postgres};

    const SCHEMA_ESTOMPE: &str = "
        CREATE TABLE users (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), keycloak_sub text NOT NULL UNIQUE, pseudo text NOT NULL);
        CREATE TABLE films_items (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), jellyfin_id text UNIQUE NOT NULL);
        CREATE TABLE films_progression (
            user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            item_id uuid NOT NULL REFERENCES films_items(id) ON DELETE CASCADE,
            position double precision NOT NULL,
            duree double precision NOT NULL,
            vu boolean NOT NULL DEFAULT false,
            maj timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (user_id, item_id)
        );";

    struct Bases {
        _bases: (BasePostgres, BasePostgres),
        entracte: PgPool,
        estompe: PgPool,
        estompe_url: String,
    }

    async fn bases() -> Option<Bases> {
        let (b1, entracte_url) = postgres().await?;
        let (b2, estompe_url) = postgres().await?;
        let entracte = PgPool::connect(&entracte_url).await.unwrap();
        crate::MIGRATIONS.run(&entracte).await.unwrap();
        let estompe = PgPool::connect(&estompe_url).await.unwrap();
        sqlx::raw_sql(SCHEMA_ESTOMPE)
            .execute(&estompe)
            .await
            .unwrap();
        Some(Bases {
            _bases: (b1, b2),
            entracte,
            estompe,
            estompe_url,
        })
    }

    async fn titre_entracte(
        pool: &PgPool,
        genre: &str,
        tmdb: i64,
        serie: Option<(Uuid, i32, i32)>,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO titres (genre, tmdb_id, serie_id, saison, episode, titre, present)
             VALUES ($1, $2, $3, $4, $5, 'x', true) RETURNING id",
        )
        .bind(genre)
        .bind(tmdb)
        .bind(serie.map(|s| s.0))
        .bind(serie.map(|s| s.1))
        .bind(serie.map(|s| s.2))
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn ligne_estompe(
        pool: &PgPool,
        sub: &str,
        jellyfin: &str,
        position: f64,
        duree: f64,
        vu: bool,
        maj: &str,
    ) {
        sqlx::query(
            "WITH u AS (INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $1)
                        ON CONFLICT (keycloak_sub) DO UPDATE SET pseudo = EXCLUDED.pseudo RETURNING id),
                  i AS (INSERT INTO films_items (jellyfin_id) VALUES ($2)
                        ON CONFLICT (jellyfin_id) DO UPDATE SET jellyfin_id = EXCLUDED.jellyfin_id RETURNING id)
             INSERT INTO films_progression (user_id, item_id, position, duree, vu, maj)
             SELECT u.id, i.id, $3, $4, $5, $6::timestamptz FROM u, i",
        )
        .bind(sub)
        .bind(jellyfin)
        .bind(position)
        .bind(duree)
        .bind(vu)
        .bind(maj)
        .execute(pool)
        .await
        .unwrap();
    }

    fn catalogue_jellyfin() -> Vec<Value> {
        vec![
            json!({"Id": "aaaa0001", "Name": "Matrix", "Type": "Movie", "ProviderIds": {"Tmdb": "603", "Imdb": "tt0133093"}}),
            json!({"Id": "aaaa0002", "Name": "Le Nord", "Type": "Episode", "SeriesId": "bbbb0001",
                   "SeriesName": "Trônes", "ParentIndexNumber": 1, "IndexNumber": 2, "ProviderIds": {"Tmdb": "63057"}}),
            json!({"Id": "bbbb0001", "Name": "Trônes", "Type": "Series", "ProviderIds": {"Tmdb": "1399"}}),
            json!({"Id": "aaaa0003", "Name": "Sans fiche", "Type": "Movie", "ProviderIds": {}}),
            json!({"Id": "aaaa0004", "Name": "Pas dans Radarr", "Type": "Movie", "ProviderIds": {"Tmdb": "999"}}),
            json!({"Id": "aaaa0005", "Name": "Orphelin", "Type": "Episode", "SeriesId": "bbbb0404",
                   "SeriesName": "Disparue", "ParentIndexNumber": 1, "IndexNumber": 1}),
        ]
    }

    async fn jellyfin() -> Faux {
        let catalogue = catalogue_jellyfin();
        faux(move |r| {
            if r.chemin != "/Items" || r.entete("authorization") != "MediaBrowser Token=\"cle-jf\""
            {
                return (401, Vec::new());
            }
            let ids: Vec<String> = url::form_urlencoded::parse(r.query.as_bytes())
                .filter(|(k, _)| k == "Ids")
                .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
                .collect();
            let items: Vec<&Value> = catalogue
                .iter()
                .filter(|i| ids.iter().any(|id| id == i["Id"].as_str().unwrap()))
                .collect();
            (200, serde_json::to_vec(&json!({ "Items": items })).unwrap())
        })
        .await
    }

    fn source(j: &Faux) -> Jellyfin {
        Jellyfin {
            url: j.url.clone(),
            cle: "cle-jf".into(),
        }
    }

    async fn progression(pool: &PgPool, sub: &str, titre: Uuid) -> Option<(f64, bool, String)> {
        sqlx::query_as(
            "SELECT position, termine, to_char(maj AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') FROM progression
             WHERE sub = $1 AND titre_id = $2",
        )
        .bind(sub)
        .bind(titre)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    async fn vu(pool: &PgPool, sub: &str, titre: Uuid) -> Option<String> {
        sqlx::query_scalar(
            "SELECT to_char(vu_le AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') FROM vus WHERE sub = $1 AND titre_id = $2",
        )
        .bind(sub)
        .bind(titre)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    #[test]
    fn lecture_de_jellyfin() {
        let (id, e) = element(
            &json!({"Id": "AB-CD", "Name": "x", "Type": "Movie", "ProviderIds": {"tmdb": "12"}}),
        )
        .unwrap();
        assert_eq!((id.as_str(), e.tmdb), ("abcd", Some(12)));
        let (_, e) =
            element(&json!({"Id": "a", "Type": "Movie", "ProviderIds": {"Tmdb": ""}})).unwrap();
        assert_eq!(
            cible(&e, &HashMap::new()),
            Err("film sans identifiant TMDB")
        );
        let (_, e) =
            element(&json!({"Id": "a", "Type": "Episode", "SeriesId": "s", "IndexNumber": 3}))
                .unwrap();
        assert_eq!(
            cible(&e, &HashMap::new()),
            Err("épisode sans numéro de saison ou d'épisode")
        );
        let (_, e) = element(&json!({"Id": "a", "Type": "Series"})).unwrap();
        assert_eq!(cible(&e, &HashMap::new()), Err("ni film ni épisode"));
        assert!(element(&json!({"Name": "sans id"})).is_none());
    }

    #[tokio::test]
    async fn import_progressions_vus_et_titres_non_resolus() {
        let Some(b) = bases().await else { return };
        let film = titre_entracte(&b.entracte, "film", 603, None).await;
        let serie = titre_entracte(&b.entracte, "serie", 1399, None).await;
        let episode = titre_entracte(&b.entracte, "episode", 63057, Some((serie, 1, 2))).await;
        let j = jellyfin().await;
        for (sub, id, pos, duree, vu, maj) in [
            (
                "alice",
                "aaaa0001",
                6000.0,
                6100.0,
                true,
                "2026-03-01 20:00Z",
            ),
            (
                "alice",
                "aaaa0002",
                1200.0,
                3000.0,
                false,
                "2026-03-02 21:00Z",
            ),
            ("bob", "aaaa0001", 300.0, 6100.0, true, "2026-04-01 20:00Z"),
            ("bob", "aaaa0002", 2950.0, 3000.0, true, "2026-02-01 20:00Z"),
            ("alice", "aaaa0003", 10.0, 100.0, false, "2026-01-01 10:00Z"),
            ("bob", "aaaa0003", 10.0, 100.0, false, "2026-01-01 10:00Z"),
            ("alice", "aaaa0004", 10.0, 100.0, true, "2026-01-01 10:00Z"),
            ("alice", "aaaa0005", 10.0, 100.0, false, "2026-01-01 10:00Z"),
            ("alice", "supprime", 10.0, 100.0, true, "2026-01-01 10:00Z"),
            ("carole", "aaaa0001", 10.0, 0.0, false, "2026-01-01 10:00Z"),
        ] {
            ligne_estompe(&b.estompe, sub, id, pos, duree, vu, maj).await;
        }
        sqlx::query("INSERT INTO progression (sub, titre_id, position, duree, maj) VALUES ('bob', $1, 1500, 3000, '2026-05-01 08:00Z')")
            .bind(episode)
            .execute(&b.entracte)
            .await
            .unwrap();
        let estompe = lecture_seule(&b.estompe_url).await.unwrap();
        let http = crate::etat::client_http();
        let rapport = importer(&b.entracte, &estompe, &http, &source(&j))
            .await
            .unwrap();
        assert_eq!(
            (
                rapport.lignes,
                rapport.personnes,
                rapport.progressions,
                rapport.gardees,
                rapport.vus,
                rapport.invalides
            ),
            (10, 3, 3, 1, 3, 1),
            "{rapport}"
        );
        let non_resolus: Vec<(&str, &str, usize)> = rapport
            .non_resolus
            .iter()
            .map(|n| (n.nom.as_str(), n.raison, n.lignes))
            .collect();
        assert_eq!(
            non_resolus,
            vec![
                ("Disparue S01E01 Orphelin", "série absente de Jellyfin", 1),
                ("Pas dans Radarr", "absent du catalogue d'Entracte", 1),
                ("Sans fiche", "film sans identifiant TMDB", 2),
                ("supprime", "absent de Jellyfin", 1),
            ]
        );
        assert!(
            rapport
                .to_string()
                .contains("Titres non résolus : 4 (5 lignes).")
        );
        assert_eq!(
            progression(&b.entracte, "alice", film).await,
            Some((6000.0, true, "2026-03-01 20:00".into()))
        );
        assert_eq!(
            vu(&b.entracte, "alice", film).await.as_deref(),
            Some("2026-03-01 20:00")
        );
        assert_eq!(
            progression(&b.entracte, "alice", episode).await,
            Some((1200.0, false, "2026-03-02 21:00".into()))
        );
        assert_eq!(vu(&b.entracte, "alice", episode).await, None);
        assert_eq!(
            progression(&b.entracte, "bob", film).await,
            Some((300.0, false, "2026-04-01 20:00".into()))
        );
        assert_eq!(
            vu(&b.entracte, "bob", film).await.as_deref(),
            Some("2026-04-01 20:00")
        );
        assert_eq!(
            progression(&b.entracte, "bob", episode).await,
            Some((1500.0, false, "2026-05-01 08:00".into())),
            "progression plus récente d'Entracte gardée"
        );
        assert_eq!(
            vu(&b.entracte, "bob", episode).await.as_deref(),
            Some("2026-02-01 20:00")
        );
        assert_eq!(progression(&b.entracte, "carole", film).await, None);

        let compte = |t: &'static str| {
            let pool = b.entracte.clone();
            async move {
                sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                    "SELECT count(*) FROM {t}"
                )))
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        let avant = (compte("progression").await, compte("vus").await);
        sqlx::query("UPDATE progression SET position = 100, termine = false, maj = '2026-06-01 08:00Z' WHERE sub = 'alice' AND titre_id = $1")
            .bind(film)
            .execute(&b.entracte)
            .await
            .unwrap();
        let deuxieme = importer(&b.entracte, &estompe, &http, &source(&j))
            .await
            .unwrap();
        assert_eq!(
            (deuxieme.progressions, deuxieme.gardees, deuxieme.vus),
            (0, 4, 0),
            "{deuxieme}"
        );
        assert_eq!(deuxieme.non_resolus, rapport.non_resolus);
        assert_eq!((compte("progression").await, compte("vus").await), avant);
        assert_eq!(
            progression(&b.entracte, "alice", film).await,
            Some((100.0, false, "2026-06-01 08:00".into()))
        );
    }

    #[tokio::test]
    async fn base_d_estompe_en_lecture_seule_et_catalogue_vide_refuse() {
        let Some(b) = bases().await else { return };
        let estompe = lecture_seule(&b.estompe_url).await.unwrap();
        let ecriture = sqlx::query("INSERT INTO films_items (jellyfin_id) VALUES ('x')")
            .execute(&estompe)
            .await
            .unwrap_err();
        assert!(ecriture.to_string().contains("read-only"), "{ecriture}");
        let j = jellyfin().await;
        let refus = importer(
            &b.entracte,
            &estompe,
            &crate::etat::client_http(),
            &source(&j),
        )
        .await
        .unwrap_err();
        assert!(refus.contains("catalogue d'Entracte vide"), "{refus}");
        assert_eq!(j.appels(), 0);
        titre_entracte(&b.entracte, "film", 603, None).await;
        ligne_estompe(
            &b.estompe,
            "alice",
            "aaaa0001",
            10.0,
            100.0,
            false,
            "2026-01-01 10:00Z",
        )
        .await;
        let panne = importer(
            &b.entracte,
            &estompe,
            &crate::etat::client_http(),
            &Jellyfin {
                url: j.url.clone(),
                cle: "mauvaise".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(panne, "Jellyfin : HTTP 401");
        let lignes: i64 = sqlx::query_scalar("SELECT count(*) FROM progression")
            .fetch_one(&b.entracte)
            .await
            .unwrap();
        assert_eq!(lignes, 0);
    }

    #[tokio::test]
    async fn variables_manquantes() {
        let erreur = lancer(|k| (k == "ENTRACTE_DATABASE_URL").then(|| "postgres://x/e".into()))
            .await
            .unwrap_err();
        assert_eq!(erreur, "ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL manquant");
    }
}
