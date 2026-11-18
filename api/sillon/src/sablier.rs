use std::fmt;

use chrono::{DateTime, Utc};
use commun::{
    emission::Emetteur,
    evenements::{Donnees, Evenement},
};
use sqlx::{
    PgPool, Row,
    postgres::{PgPoolOptions, PgRow},
};
use uuid::Uuid;

pub const COMMANDE: &str = "importer-sablier";
pub const PREFIXE: &str = "import:sillon:ecoute:";
const LOT: i64 = 500;

#[derive(Clone, Debug, PartialEq)]
pub struct Ecoute {
    pub id: Uuid,
    pub sub: String,
    pub piste: Uuid,
    pub debut: DateTime<Utc>,
    pub secondes: f64,
    pub comptee: bool,
}

impl Ecoute {
    pub fn lire(l: &PgRow) -> Self {
        Ecoute {
            id: l.get("id"),
            sub: l.get("sub"),
            piste: l.get("piste_id"),
            debut: l.get("debut"),
            secondes: l.get("duree_ecoutee"),
            comptee: l.get("compte"),
        }
    }

    pub fn evenement(&self, importe: bool) -> Evenement {
        let mut e = Evenement::importe(
            format!("{PREFIXE}{}", self.id),
            &self.sub,
            self.debut,
            Donnees::SillonEcoute {
                piste: self.piste.to_string(),
                secondes: self.secondes.round() as u32,
                comptee: self.comptee,
            },
        );
        e.importe = importe;
        e
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub evenements: usize,
    pub secondes: u64,
    pub comptees: usize,
}

impl fmt::Display for Bilan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Import de l'historique de Sillon : {} évènements publiés.",
            self.evenements
        )?;
        writeln!(f, "Secondes écoutées : {}.", self.secondes)?;
        writeln!(f, "Écoutes comptées : {}.", self.comptees)
    }
}

pub async fn rejouer(
    pool: &PgPool,
    mut publier: impl AsyncFnMut(Evenement) -> Result<(), String>,
) -> Result<Bilan, String> {
    let mut bilan = Bilan::default();
    let mut apres: Option<Uuid> = None;
    loop {
        let lignes = sqlx::query(
            "SELECT id, sub, piste_id, debut, duree_ecoutee, compte FROM ecoutes
             WHERE $1::uuid IS NULL OR id > $1 ORDER BY id LIMIT $2",
        )
        .bind(apres)
        .bind(LOT)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("historique de Sillon : {e}"))?;
        let Some(derniere) = lignes.last() else {
            return Ok(bilan);
        };
        apres = Some(derniere.get("id"));
        for l in &lignes {
            let e = Ecoute::lire(l).evenement(true);
            let (id, donnees) = (e.id.clone(), e.donnees.clone());
            publier(e).await.map_err(|erreur| {
                format!(
                    "évènement {id} : {erreur} ({} évènements publiés avant l'arrêt, la commande peut être relancée)",
                    bilan.evenements
                )
            })?;
            if let Donnees::SillonEcoute {
                secondes, comptee, ..
            } = donnees
            {
                bilan.evenements += 1;
                bilan.secondes += u64::from(secondes);
                bilan.comptees += usize::from(comptee);
            }
        }
    }
}

pub async fn importer(pool: &PgPool, emetteur: &Emetteur) -> Result<Bilan, String> {
    if !emetteur.actif() {
        return Err(format!(
            "émission inactive ({} non renseignée ou invalide)",
            commun::emission::VARIABLE_URL
        ));
    }
    rejouer(pool, async |e| emetteur.publier(&e).await).await
}

pub async fn lancer(brut: impl Fn(&str) -> Option<String>) -> Result<Bilan, String> {
    let url = commun::env::url_base(&brut, "sillon")?;
    let emetteur = Emetteur::depuis_env(&brut);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|e| format!("base de Sillon : {e}"))?;
    importer(&pool, &emetteur).await
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::TimeZone;
    use redis::aio::MultiplexedConnection;
    use serde_json::{Value, json};

    use super::*;
    use crate::{playlists::tests::catalogue, testutil::env};

    static COMPTEUR: AtomicUsize = AtomicUsize::new(0);

    pub(crate) struct Flux {
        pub emetteur: Emetteur,
        cle: String,
        connexion: MultiplexedConnection,
    }

    impl Flux {
        pub(crate) async fn ouvrir() -> Option<Self> {
            let Ok(url) = std::env::var("TEST_REDIS_URL") else {
                eprintln!("ignoré : TEST_REDIS_URL absent");
                return None;
            };
            let cle = format!(
                "sablier:test:sillon:{}:{}",
                std::process::id(),
                COMPTEUR.fetch_add(1, Ordering::Relaxed)
            );
            let connexion = redis::Client::open(url.as_str())
                .unwrap()
                .get_multiplexed_async_connection()
                .await
                .expect("connexion à TEST_REDIS_URL");
            Some(Flux {
                emetteur: Emetteur::sur(&url, &cle, 100_000),
                cle,
                connexion,
            })
        }

        pub(crate) async fn lus(&mut self) -> Vec<Evenement> {
            let entrees: Vec<(String, Vec<String>)> = redis::cmd("XRANGE")
                .arg(&self.cle)
                .arg("-")
                .arg("+")
                .query_async(&mut self.connexion)
                .await
                .unwrap();
            entrees
                .iter()
                .map(|(_, champs)| Evenement::lire(&champs[1]).unwrap())
                .collect()
        }

        pub(crate) async fn attendre(&mut self, n: usize) -> Vec<Evenement> {
            let limite = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                let lus = self.lus().await;
                if lus.len() >= n {
                    return lus;
                }
                assert!(
                    std::time::Instant::now() < limite,
                    "délai dépassé : {n} évènements dans le flux"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }

        pub(crate) async fn effacer(&mut self) {
            let _: i64 = redis::cmd("DEL")
                .arg(&self.cle)
                .query_async(&mut self.connexion)
                .await
                .unwrap();
        }
    }

    pub(crate) async fn rejoues(pool: &PgPool) -> (Bilan, Vec<Evenement>) {
        let mut evenements = Vec::new();
        let bilan = rejouer(pool, async |e| {
            evenements.push(e);
            Ok(())
        })
        .await
        .unwrap();
        (bilan, evenements)
    }

    pub(crate) fn sans_donnee_interne(e: &Evenement) {
        let v: Value = serde_json::from_str(&e.json()).unwrap();
        let cles = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
        let mut racine = cles(&v);
        racine.sort();
        assert_eq!(
            racine,
            ["date", "donnees", "id", "importe", "source", "sub", "type"]
        );
        let mut donnees = cles(&v["donnees"]);
        donnees.sort();
        assert_eq!(donnees, ["comptee", "piste", "secondes"]);
        let texte = e.json().to_lowercase();
        for interdit in ["lidarr", "chemin", "/srv", ".flac", "987654321"] {
            assert!(!texte.contains(interdit), "{interdit} dans {texte}");
        }
    }

    async fn historique(pool: &PgPool, n: i32) -> Vec<String> {
        let pistes = catalogue(pool, 3).await;
        sqlx::query("UPDATE pistes SET lidarr_id = 987654321")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO fichiers (piste_id, lidarr_id, chemin, taille, conteneur, codec)
             SELECT id, 987654321, '/srv/musique/' || id || '.flac', 1, 'flac', 'flac' FROM pistes",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ecoutes (sub, piste_id, debut, duree_ecoutee, duree_piste, fin, compte)
             SELECT 'sub-' || (i % 3), (SELECT id FROM pistes ORDER BY numero OFFSET i % 3 LIMIT 1),
                    '2026-01-01T00:00:00Z'::timestamptz + i * interval '1 hour', (i % 200) + 0.3, 200,
                    'skipped', (i % 200) + 0.3 >= 30
             FROM generate_series(1, $1) AS i",
        )
        .bind(n)
        .execute(pool)
        .await
        .unwrap();
        pistes
    }

    #[test]
    fn evenement_d_une_ecoute() {
        let ecoute = Ecoute {
            id: Uuid::from_u128(7),
            sub: "sub-alice".into(),
            piste: Uuid::from_u128(9),
            debut: Utc.with_ymd_and_hms(2026, 3, 29, 1, 30, 0).unwrap(),
            secondes: 19.6,
            comptee: false,
        };
        let reel = ecoute.evenement(false);
        assert_eq!(
            serde_json::from_str::<Value>(&reel.json()).unwrap(),
            json!({
                "id": "import:sillon:ecoute:00000000-0000-0000-0000-000000000007",
                "sub": "sub-alice",
                "source": "sillon",
                "date": "2026-03-29T01:30:00Z",
                "importe": false,
                "type": "sillon.ecoute",
                "donnees": {"piste": "00000000-0000-0000-0000-000000000009", "secondes": 20, "comptee": false},
            })
        );
        assert!(reel.verifier().is_ok());
        sans_donnee_interne(&reel);
        let importe = ecoute.evenement(true);
        assert!(importe.importe);
        assert_eq!(
            Evenement {
                importe: false,
                ..importe
            },
            reel
        );
        let longue = Ecoute {
            secondes: 120.4,
            comptee: true,
            ..ecoute
        };
        assert_eq!(
            longue.evenement(true).donnees,
            Donnees::SillonEcoute {
                piste: longue.piste.to_string(),
                secondes: 120,
                comptee: true
            }
        );
    }

    #[test]
    fn bilan_affiche() {
        let bilan = Bilan {
            evenements: 3,
            secondes: 140,
            comptees: 1,
        };
        assert_eq!(
            bilan.to_string(),
            "Import de l'historique de Sillon : 3 évènements publiés.\nSecondes écoutées : 140.\nÉcoutes comptées : 1.\n"
        );
    }

    #[tokio::test]
    async fn import_relance_memes_identifiants_et_totaux_de_l_historique() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        assert_eq!(rejoues(pool).await, (Bilan::default(), Vec::new()));
        let pistes = historique(pool, 1203).await;
        let (bilan, premiers) = rejoues(pool).await;
        let (encore, seconds) = rejoues(pool).await;
        assert_eq!(bilan, encore);
        assert_eq!(premiers, seconds);
        let l = sqlx::query(
            "SELECT count(*), sum(round(duree_ecoutee::numeric))::bigint, count(*) FILTER (WHERE compte) FROM ecoutes",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            (
                bilan.evenements as i64,
                bilan.secondes as i64,
                bilan.comptees as i64
            ),
            (l.get(0), l.get(1), l.get(2))
        );
        assert_eq!((bilan.evenements, premiers.len()), (1203, 1203));
        let ids: std::collections::HashSet<&str> = premiers.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), 1203);
        let lignes = sqlx::query("SELECT id, sub, piste_id, debut, compte FROM ecoutes")
            .fetch_all(pool)
            .await
            .unwrap();
        for l in &lignes {
            let id = format!("{PREFIXE}{}", l.get::<Uuid, _>("id"));
            let ev = premiers.iter().find(|e| e.id == id).unwrap();
            assert!(ev.importe);
            assert_eq!(ev.sub, l.get::<String, _>("sub"));
            assert_eq!(ev.date, l.get::<DateTime<Utc>, _>("debut"));
            let Donnees::SillonEcoute { piste, comptee, .. } = &ev.donnees else {
                panic!("type inattendu : {}", ev.donnees.nom());
            };
            assert!(pistes.contains(piste));
            assert_eq!(*comptee, l.get::<bool, _>("compte"));
            sans_donnee_interne(ev);
        }
        let erreur = importer(pool, &Emetteur::inactif()).await.unwrap_err();
        assert!(erreur.contains(commun::emission::VARIABLE_URL), "{erreur}");
        let arrete = Emetteur::depuis(Some("redis://127.0.0.1:1"), None);
        let erreur = importer(pool, &arrete).await.unwrap_err();
        assert!(erreur.contains("connexion"), "{erreur}");
        assert!(erreur.contains("0 évènements publiés"), "{erreur}");
    }

    #[tokio::test]
    async fn import_publie_dans_le_flux_les_memes_evenements_a_chaque_lancement() {
        let Some(e) = env().await else { return };
        let Some(mut flux) = Flux::ouvrir().await else {
            return;
        };
        let pool = e.state.pool();
        historique(pool, 620).await;
        let (attendu, mut attendus) = rejoues(pool).await;
        let bilan = importer(pool, &flux.emetteur).await.unwrap();
        assert_eq!(bilan, attendu);
        let trier = |v: &mut Vec<Evenement>| v.sort_by(|a, b| a.id.cmp(&b.id));
        trier(&mut attendus);
        let mut lus = flux.lus().await;
        trier(&mut lus);
        assert_eq!(lus, attendus);
        assert!(lus.iter().all(|e| e.importe));
        assert_eq!(importer(pool, &flux.emetteur).await.unwrap(), attendu);
        let mut relus = flux.lus().await;
        assert_eq!(relus.len(), 1240);
        trier(&mut relus);
        relus.dedup();
        assert_eq!(relus, attendus);
        flux.effacer().await;
    }
}
