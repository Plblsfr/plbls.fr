use std::time::Duration;

use commun::{emission::CHAMP, evenements::Evenement};
use redis::aio::MultiplexedConnection;
use sqlx::PgPool;

use crate::{agregats, config::Config, domaines};

const LOT: usize = 100;
const ATTENTE_BLOCAGE: Duration = Duration::from_secs(5);
const DELAI: Duration = Duration::from_secs(5);
const PAUSE_APRES_PANNE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reglages {
    pub url: String,
    pub flux: String,
    pub groupe: String,
    pub consommateur: String,
}

impl Reglages {
    pub fn de(c: &Config) -> Option<Self> {
        Some(Reglages {
            url: c.redis_url.clone()?,
            flux: c.flux.clone(),
            groupe: c.groupe.clone(),
            consommateur: c.consommateur.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    Applique,
    Doublon,
    Ecarte(String),
}

pub async fn traiter(pool: &PgPool, fuseau: &str, json: &str) -> Result<Issue, sqlx::Error> {
    let e = match Evenement::lire(json) {
        Ok(e) => e,
        Err(motif) => return Ok(Issue::Ecarte(motif)),
    };
    let mut tx = pool.begin().await?;
    let insere = sqlx::query(
        "INSERT INTO evenements (id, sub, source, type, date, importe, donnees)
        VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&e.id)
    .bind(&e.sub)
    .bind(e.source.nom())
    .bind(e.donnees.nom())
    .bind(e.date)
    .bind(e.importe)
    .bind(sqlx::types::Json(e.donnees.contenu()))
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !insere {
        return Ok(Issue::Doublon);
    }
    let jour = agregats::jour_de(&mut *tx, fuseau, e.date).await?;
    domaines::appliquer(&mut tx, &e, jour).await?;
    tx.commit().await?;
    Ok(Issue::Applique)
}

type Entree = (String, Option<Vec<Vec<u8>>>);
type Lecture = Option<Vec<(String, Vec<Entree>)>>;

fn charge(champs: Option<Vec<Vec<u8>>>) -> Option<String> {
    let champs = champs?;
    champs
        .as_chunks::<2>()
        .0
        .iter()
        .find(|[nom, _]| nom == CHAMP.as_bytes())
        .map(|[_, valeur]| String::from_utf8_lossy(valeur).into_owned())
}

async fn connecter(url: &str) -> Result<MultiplexedConnection, String> {
    let client = redis::Client::open(url).map_err(|e| format!("adresse : {:?}", e.kind()))?;
    let reglages = redis::AsyncConnectionConfig::new()
        .set_connection_timeout(Some(DELAI))
        .set_response_timeout(None);
    client
        .get_multiplexed_async_connection_with_config(&reglages)
        .await
        .map_err(|e| format!("connexion : {e}"))
}

async fn creer_groupe(c: &mut MultiplexedConnection, r: &Reglages) -> Result<(), String> {
    let mut commande = redis::cmd("XGROUP");
    commande
        .arg("CREATE")
        .arg(&r.flux)
        .arg(&r.groupe)
        .arg("0")
        .arg("MKSTREAM");
    match tokio::time::timeout(DELAI, commande.query_async::<String>(c)).await {
        Err(_) => Err("XGROUP CREATE : délai dépassé".into()),
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) if e.code() == Some("BUSYGROUP") => Ok(()),
        Ok(Err(e)) => Err(format!("XGROUP CREATE : {e}")),
    }
}

async fn lire(
    c: &mut MultiplexedConnection,
    r: &Reglages,
    en_attente: bool,
) -> Result<Vec<Entree>, String> {
    let mut commande = redis::cmd("XREADGROUP");
    commande
        .arg("GROUP")
        .arg(&r.groupe)
        .arg(&r.consommateur)
        .arg("COUNT")
        .arg(LOT);
    if !en_attente {
        commande
            .arg("BLOCK")
            .arg(ATTENTE_BLOCAGE.as_millis() as u64);
    }
    commande
        .arg("STREAMS")
        .arg(&r.flux)
        .arg(if en_attente { "0" } else { ">" });
    let reponse = tokio::time::timeout(ATTENTE_BLOCAGE + DELAI, commande.query_async::<Lecture>(c))
        .await
        .map_err(|_| "XREADGROUP : délai dépassé".to_string())?
        .map_err(|e| format!("XREADGROUP : {e}"))?;
    Ok(reponse
        .unwrap_or_default()
        .into_iter()
        .flat_map(|(_, entrees)| entrees)
        .collect())
}

async fn accuser(c: &mut MultiplexedConnection, r: &Reglages, id: &str) -> Result<(), String> {
    let mut commande = redis::cmd("XACK");
    commande.arg(&r.flux).arg(&r.groupe).arg(id);
    tokio::time::timeout(DELAI, commande.query_async::<i64>(c))
        .await
        .map_err(|_| "XACK : délai dépassé".to_string())?
        .map_err(|e| format!("XACK : {e}"))?;
    Ok(())
}

async fn session(
    pool: &PgPool,
    fuseau: &str,
    r: &Reglages,
    connectee: &mut bool,
) -> Result<std::convert::Infallible, String> {
    let mut c = connecter(&r.url).await?;
    creer_groupe(&mut c, r).await?;
    *connectee = true;
    let mut en_attente = true;
    loop {
        let entrees = lire(&mut c, r, en_attente).await?;
        if entrees.is_empty() {
            en_attente = false;
            continue;
        }
        for (id, champs) in entrees {
            let issue = match charge(champs) {
                Some(json) => traiter(pool, fuseau, &json)
                    .await
                    .map_err(|e| format!("base : {e}"))?,
                None => Issue::Ecarte(format!("champ « {CHAMP} » absent")),
            };
            if let Issue::Ecarte(motif) = &issue {
                tracing::warn!("file d'évènements : entrée {id} écartée ({motif})");
            }
            accuser(&mut c, r, &id).await?;
        }
    }
}

pub async fn consommer(pool: PgPool, fuseau: String, r: Reglages) {
    let mut en_panne = false;
    loop {
        let mut connectee = false;
        let Err(e) = session(&pool, &fuseau, &r, &mut connectee).await;
        if connectee {
            en_panne = false;
        }
        if !en_panne {
            tracing::warn!(
                "file d'évènements : consommation interrompue, nouvel essai toutes les {} s ({e})",
                PAUSE_APRES_PANNE.as_secs()
            );
            en_panne = true;
        }
        tokio::time::sleep(PAUSE_APRES_PANNE).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use commun::{emission::Emetteur, evenements::Donnees};
    use tokio::task::JoinHandle;

    use super::*;
    use crate::testutil::{Env, attendre, env};

    const FUSEAU: &str = "Europe/Paris";

    fn ecoute(sub: &str) -> Evenement {
        Evenement::nouveau(
            sub,
            Donnees::SillonEcoute {
                piste: "p1".into(),
                secondes: 180,
                comptee: true,
            },
        )
    }

    async fn ids(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM evenements ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    fn tries(evenements: &[&Evenement]) -> Vec<String> {
        let mut v: Vec<String> = evenements.iter().map(|e| e.id.clone()).collect();
        v.sort();
        v
    }

    #[tokio::test]
    async fn evenement_stocke_une_fois_meme_rejoue() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let evenement = Evenement::importe(
            "import:sillon:ecoute:7",
            "sub-alice",
            crate::testutil::instant("2026-03-28T23:30:00Z"),
            Donnees::SillonEcoute {
                piste: "p1".into(),
                secondes: 180,
                comptee: true,
            },
        );
        assert_eq!(
            traiter(pool, FUSEAU, &evenement.json()).await.unwrap(),
            Issue::Applique
        );
        for _ in 0..2 {
            assert_eq!(
                traiter(pool, FUSEAU, &evenement.json()).await.unwrap(),
                Issue::Doublon
            );
        }
        let lignes: Vec<(String, String, String, String, bool, serde_json::Value)> =
            sqlx::query_as("SELECT id, sub, source, type, importe, donnees FROM evenements")
                .fetch_all(pool)
                .await
                .unwrap();
        assert_eq!(
            lignes,
            [(
                "import:sillon:ecoute:7".to_string(),
                "sub-alice".to_string(),
                "sillon".to_string(),
                "sillon.ecoute".to_string(),
                true,
                serde_json::json!({"piste": "p1", "secondes": 180, "comptee": true}),
            )]
        );
        let mut simultanes = Vec::new();
        for _ in 0..8 {
            let json = ecoute("sub-bruno").json();
            for _ in 0..2 {
                let (pool, json) = (pool.clone(), json.clone());
                simultanes.push(tokio::spawn(async move {
                    traiter(&pool, FUSEAU, &json).await.unwrap()
                }));
            }
        }
        let mut appliques = 0;
        for t in simultanes {
            if t.await.unwrap() == Issue::Applique {
                appliques += 1;
            }
        }
        assert_eq!(appliques, 8);
        assert_eq!(ids(pool).await.len(), 9);
    }

    #[tokio::test]
    async fn evenement_mal_forme_ou_inconnu_ecarte() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let bon = ecoute("sub-alice");
        let inconnu = bon.json().replace("sillon.ecoute", "sillon.inconnu");
        let incoherent = bon
            .json()
            .replace("\"source\":\"sillon\"", "\"source\":\"estompe\"");
        for mauvais in [
            "",
            "pas du json",
            "{}",
            inconnu.as_str(),
            incoherent.as_str(),
        ] {
            assert!(
                matches!(
                    traiter(pool, FUSEAU, mauvais).await.unwrap(),
                    Issue::Ecarte(_)
                ),
                "{mauvais}"
            );
        }
        assert!(ids(pool).await.is_empty());
        assert_eq!(charge(None), None);
        assert_eq!(charge(Some(vec![b"autre".to_vec(), b"x".to_vec()])), None);
        assert_eq!(
            charge(Some(vec![
                b"autre".to_vec(),
                b"x".to_vec(),
                b"e".to_vec(),
                b"{}".to_vec()
            ]))
            .as_deref(),
            Some("{}")
        );
    }

    static COMPTEUR: AtomicUsize = AtomicUsize::new(0);

    struct Banc {
        e: Env,
        r: Reglages,
        redis: MultiplexedConnection,
        emetteur: Emetteur,
    }

    impl Banc {
        fn demarrer(&self) -> JoinHandle<()> {
            tokio::spawn(consommer(
                self.e.state.pool().clone(),
                FUSEAU.to_string(),
                self.r.clone(),
            ))
        }

        async fn publier(&self, evenements: &[&Evenement]) {
            for e in evenements {
                self.emetteur.publier(e).await.unwrap();
            }
        }

        async fn brut(&mut self, valeur: &str) {
            let _: String = redis::cmd("XADD")
                .arg(&self.r.flux)
                .arg("*")
                .arg(CHAMP)
                .arg(valeur)
                .query_async(&mut self.redis)
                .await
                .unwrap();
        }

        async fn non_accuses(&mut self) -> i64 {
            let (n, _, _, _): (i64, redis::Value, redis::Value, redis::Value) =
                redis::cmd("XPENDING")
                    .arg(&self.r.flux)
                    .arg(&self.r.groupe)
                    .query_async(&mut self.redis)
                    .await
                    .unwrap();
            n
        }

        async fn stockes(&self, attendus: &[&Evenement]) {
            let attendus = tries(attendus);
            let pool = self.e.state.pool();
            attendre(
                || async { ids(pool).await == attendus },
                "évènements consommés et stockés",
            )
            .await;
        }

        async fn tout_accuse(&mut self) {
            let limite = tokio::time::Instant::now() + Duration::from_secs(20);
            while self.non_accuses().await != 0 {
                assert!(
                    tokio::time::Instant::now() < limite,
                    "délai dépassé : accusés"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        async fn fermer(mut self) {
            let _: i64 = redis::cmd("DEL")
                .arg(&self.r.flux)
                .query_async(&mut self.redis)
                .await
                .unwrap();
        }
    }

    async fn banc() -> Option<Banc> {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("ignoré : TEST_REDIS_URL absent");
            return None;
        };
        let e = env().await?;
        let n = COMPTEUR.fetch_add(1, Ordering::Relaxed);
        let r = Reglages {
            url: url.clone(),
            flux: format!("sablier:test:file:{}:{n}", std::process::id()),
            groupe: format!("groupe-{n}"),
            consommateur: "sablier-test".into(),
        };
        let redis = connecter(&url).await.expect("connexion à TEST_REDIS_URL");
        let emetteur = Emetteur::sur(&url, &r.flux, 10_000);
        Some(Banc {
            e,
            r,
            redis,
            emetteur,
        })
    }

    #[tokio::test]
    async fn evenements_publies_pendant_l_arret_consommes_au_redemarrage() {
        let Some(mut b) = banc().await else { return };
        let (e1, e2, e3, e4) = (ecoute("a"), ecoute("a"), ecoute("b"), ecoute("b"));
        b.publier(&[&e1, &e2]).await;
        let tache = b.demarrer();
        b.stockes(&[&e1, &e2]).await;
        b.tout_accuse().await;
        tache.abort();
        let _ = tache.await;
        b.publier(&[&e3, &e4]).await;
        assert_eq!(ids(b.e.state.pool()).await, tries(&[&e1, &e2]));
        let tache = b.demarrer();
        b.stockes(&[&e1, &e2, &e3, &e4]).await;
        b.tout_accuse().await;
        tache.abort();
        b.fermer().await;
    }

    #[tokio::test]
    async fn evenements_lus_sans_accuse_repris_au_redemarrage() {
        let Some(mut b) = banc().await else { return };
        let (e1, e2, e3) = (ecoute("a"), ecoute("a"), ecoute("a"));
        creer_groupe(&mut b.redis, &b.r).await.unwrap();
        b.publier(&[&e1, &e2]).await;
        let lus = lire(&mut b.redis, &b.r, false).await.unwrap();
        assert_eq!(lus.len(), 2);
        assert_eq!(b.non_accuses().await, 2);
        b.publier(&[&e3]).await;
        assert!(ids(b.e.state.pool()).await.is_empty());
        let tache = b.demarrer();
        b.stockes(&[&e1, &e2, &e3]).await;
        b.tout_accuse().await;
        tache.abort();
        b.fermer().await;
    }

    #[tokio::test]
    async fn evenement_rejoue_compte_une_fois_et_mal_forme_sans_blocage() {
        let Some(mut b) = banc().await else { return };
        let (e1, e2) = (ecoute("a"), ecoute("b"));
        b.publier(&[&e1, &e1]).await;
        b.brut("pas du json").await;
        b.brut(&e2.json().replace("sillon.ecoute", "sillon.inconnu"))
            .await;
        let _: String = redis::cmd("XADD")
            .arg(&b.r.flux)
            .arg("*")
            .arg("autre")
            .arg(vec![0xff_u8, 0xfe])
            .query_async(&mut b.redis)
            .await
            .unwrap();
        b.publier(&[&e2, &e1]).await;
        let tache = b.demarrer();
        b.stockes(&[&e1, &e2]).await;
        b.tout_accuse().await;
        let n: i64 = redis::cmd("XLEN")
            .arg(&b.r.flux)
            .query_async(&mut b.redis)
            .await
            .unwrap();
        assert_eq!(n, 7);
        tache.abort();
        b.fermer().await;
    }

    #[tokio::test]
    async fn redis_injoignable_la_consommation_attend_sans_tomber() {
        let Some(e) = env().await else { return };
        let r = Reglages {
            url: "redis://127.0.0.1:1".into(),
            flux: "sablier:test:injoignable".into(),
            groupe: "g".into(),
            consommateur: "c".into(),
        };
        let mut connectee = false;
        let erreur = session(e.state.pool(), FUSEAU, &r, &mut connectee)
            .await
            .unwrap_err();
        assert!(erreur.starts_with("connexion"), "{erreur}");
        assert!(!connectee);
        let tache = tokio::spawn(consommer(e.state.pool().clone(), FUSEAU.to_string(), r));
        tokio::task::yield_now().await;
        assert!(!tache.is_finished());
        tache.abort();
        let mut cfg = e.state.config().clone();
        assert_eq!(Reglages::de(&cfg), None);
        cfg.redis_url = Some("redis://127.0.0.1:6379".into());
        let reglages = Reglages::de(&cfg).unwrap();
        assert_eq!(
            (
                reglages.flux.as_str(),
                reglages.groupe.as_str(),
                reglages.consommateur.as_str()
            ),
            ("sablier:evenements", "sablier", "sablier-api")
        );
    }
}
