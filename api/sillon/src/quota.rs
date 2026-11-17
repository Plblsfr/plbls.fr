use std::time::Duration;

use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::etat::AppState;

fn nombre<T: std::str::FromStr>(nom: &str, valeur: Option<String>, defaut: T) -> Result<T, String> {
    valeur.map_or(Ok(defaut), |v| {
        v.parse().map_err(|_| format!("{nom} invalide : {v}"))
    })
}

pub const GO: u64 = 1 << 30;
pub const DUREE_ALBUM_DEFAUT: f64 = 3600.0;
const DEBIT_DEFAUT: u64 = 1000;

pub const ETATS_ACTIFS: &str = "('envoye', 'telechargement', 'partiel')";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reglages {
    pub racine: Option<String>,
    pub profil_qualite: Option<i64>,
    pub profil_metadonnees: Option<i64>,
    pub par_jour: u32,
    pub plafond: Option<u64>,
    pub delai: Duration,
    pub decouverte_par_jour: u32,
    pub decouverte_budget: u64,
}

impl Reglages {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let go: u64 = nombre("SILLON_MEDIAS_GO", env("SILLON_MEDIAS_GO"), 0)?;
        let heures: u64 = nombre(
            "SILLON_AJOUTS_DELAI_HEURES",
            env("SILLON_AJOUTS_DELAI_HEURES"),
            72,
        )?;
        let profil = |nom: &str| -> Result<Option<i64>, String> {
            Ok(Some(nombre::<i64>(nom, env(nom), 0)?).filter(|p| *p > 0))
        };
        Ok(Self {
            racine: env("SILLON_LIDARR_RACINE")
                .map(|r| r.trim().trim_end_matches('/').to_string())
                .filter(|r| !r.is_empty()),
            profil_qualite: profil("SILLON_LIDARR_QUALITE")?,
            profil_metadonnees: profil("SILLON_LIDARR_METADONNEES")?,
            par_jour: nombre("SILLON_AJOUTS_PAR_JOUR", env("SILLON_AJOUTS_PAR_JOUR"), 10)?,
            plafond: (go > 0).then(|| go.saturating_mul(GO)),
            delai: Duration::from_secs(heures.saturating_mul(3600)),
            decouverte_par_jour: nombre(
                "SILLON_DECOUVERTE_PAR_JOUR",
                env("SILLON_DECOUVERTE_PAR_JOUR"),
                2,
            )?,
            decouverte_budget: nombre::<u64>(
                "SILLON_DECOUVERTE_GO",
                env("SILLON_DECOUVERTE_GO"),
                20,
            )?
            .saturating_mul(GO),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occupation {
    pub taille: i64,
    pub en_cours: i64,
    pub plafond: Option<i64>,
}

impl Occupation {
    pub fn depasse(&self, estimation: i64) -> bool {
        self.plafond.is_some_and(|p| {
            self.taille
                .saturating_add(self.en_cours)
                .saturating_add(estimation)
                > p
        })
    }

    pub fn public(&self) -> Value {
        json!({
            "used": self.taille,
            "pending": self.en_cours,
            "limit": self.plafond,
            "free": self.plafond.map(|p| (p - self.taille - self.en_cours).max(0)),
        })
    }
}

pub async fn en_cours(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT coalesce(sum(greatest(0, a.estimation - coalesce((
            SELECT sum(f.taille) FROM fichiers f JOIN pistes p ON p.id = f.piste_id JOIN albums al ON al.id = p.album_id
            WHERE al.mbid = ANY(a.albums)), 0))), 0)::bigint
         FROM ajouts a WHERE a.etat IN {ETATS_ACTIFS}"
    )))
    .fetch_one(conn)
    .await
}

pub async fn occupation_sur(
    state: &AppState,
    conn: &mut PgConnection,
) -> Result<Occupation, sqlx::Error> {
    Ok(Occupation {
        taille: sqlx::query_scalar("SELECT coalesce(sum(taille), 0)::bigint FROM fichiers")
            .fetch_one(&mut *conn)
            .await?,
        en_cours: en_cours(conn).await?,
        plafond: state
            .config()
            .ajouts
            .plafond
            .map(|p| i64::try_from(p).unwrap_or(i64::MAX)),
    })
}

pub async fn occupation(state: &AppState) -> Result<Occupation, sqlx::Error> {
    let mut conn = state.pool().acquire().await?;
    occupation_sur(state, &mut conn).await
}

pub async fn budget_decouverte(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT coalesce(sum(CASE WHEN a.etat = 'disponible' THEN t.taille ELSE greatest(a.estimation, t.taille) END), 0)::bigint
         FROM ajouts a JOIN LATERAL (
            SELECT coalesce(sum(f.taille), 0)::bigint AS taille FROM fichiers f JOIN pistes p ON p.id = f.piste_id
              JOIN albums al ON al.id = p.album_id WHERE al.mbid = ANY(a.albums)) t ON true
         WHERE a.origine = 'decouverte' AND a.etat IN ('envoye', 'telechargement', 'partiel', 'disponible')",
    )
    .fetch_one(conn)
    .await
}

pub async fn decouvertes_du_jour(conn: &mut PgConnection, sub: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM ajouts WHERE sub = $1 AND (origine = 'decouverte' OR promu_le IS NOT NULL)
           AND cree_le > now() - interval '1 day'",
    )
    .bind(sub)
    .fetch_one(conn)
    .await
}

pub fn debit_qualite(nom: &str) -> u64 {
    let n = nom.to_ascii_lowercase();
    if n.contains("24bit") || n.contains("24-bit") || n.contains("24 bit") {
        return 2304;
    }
    if n.contains("wav") && !n.contains("wavpack") {
        return 1411;
    }
    if ["flac", "alac", "ape", "wavpack", "lossless"]
        .iter()
        .any(|f| n.contains(f))
    {
        return 1000;
    }
    let chiffres: String = n
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    chiffres
        .parse::<u64>()
        .ok()
        .filter(|k| (32..=2000).contains(k))
        .unwrap_or(320)
}

fn qualites_permises(items: &Value, permis: bool, sortie: &mut Vec<String>) {
    for i in items.as_array().into_iter().flatten() {
        let ok = permis || i["allowed"] == true;
        if let Some(n) = i["quality"]["name"].as_str()
            && ok
        {
            sortie.push(n.to_string());
        }
        qualites_permises(&i["items"], ok, sortie);
    }
}

pub fn debit_profil(profils: &[Value], id: i64) -> u64 {
    let mut noms = Vec::new();
    if let Some(p) = profils.iter().find(|p| p["id"].as_i64() == Some(id)) {
        qualites_permises(&p["items"], false, &mut noms);
    }
    noms.iter()
        .map(|n| debit_qualite(n))
        .max()
        .unwrap_or(DEBIT_DEFAUT)
}

pub fn estimer(duree_secondes: f64, debit_kbps: u64) -> i64 {
    let s = if duree_secondes > 0.0 {
        duree_secondes
    } else {
        DUREE_ALBUM_DEFAUT
    };
    (s * debit_kbps as f64 * 125.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debits_des_qualites_de_lidarr() {
        assert_eq!(debit_qualite("FLAC"), 1000);
        assert_eq!(debit_qualite("FLAC 24bit"), 2304);
        assert_eq!(debit_qualite("WAV"), 1411);
        assert_eq!(debit_qualite("WavPack"), 1000);
        assert_eq!(debit_qualite("MP3-320"), 320);
        assert_eq!(debit_qualite("AAC-256"), 256);
        assert_eq!(debit_qualite("MP3-VBR-V0"), 320);
        assert_eq!(debit_qualite("Unknown"), 320);
    }

    #[test]
    fn debit_du_profil_le_plus_haut_permis() {
        let profils = vec![
            json!({"id": 1, "items": [
                {"quality": {"name": "FLAC"}, "allowed": false},
                {"name": "MP3", "allowed": true, "items": [{"quality": {"name": "MP3-256"}, "allowed": false},
                                                         {"quality": {"name": "MP3-192"}, "allowed": false}]},
            ]}),
            json!({"id": 2, "items": [{"quality": {"name": "FLAC"}, "allowed": true}]}),
        ];
        assert_eq!(debit_profil(&profils, 1), 256);
        assert_eq!(debit_profil(&profils, 2), 1000);
        assert_eq!(debit_profil(&profils, 3), DEBIT_DEFAUT);
        assert_eq!(estimer(8.0, 1000), 1_000_000);
        assert_eq!(estimer(0.0, 8), 3_600_000);
    }

    #[test]
    fn plafond_depasse() {
        let o = Occupation {
            taille: 60,
            en_cours: 30,
            plafond: Some(100),
        };
        assert!(!o.depasse(10));
        assert!(o.depasse(11));
        assert_eq!(o.public()["free"], 10);
        assert!(!Occupation { plafond: None, ..o }.depasse(i64::MAX));
    }

    #[test]
    fn reglages_lus() {
        let env = |k: &str| match k {
            "SILLON_LIDARR_RACINE" => Some("/music/".to_string()),
            "SILLON_LIDARR_QUALITE" => Some("2".into()),
            "SILLON_LIDARR_METADONNEES" => Some("3".into()),
            "SILLON_AJOUTS_PAR_JOUR" => Some("4".into()),
            "SILLON_MEDIAS_GO" => Some("500".into()),
            "SILLON_AJOUTS_DELAI_HEURES" => Some("1".into()),
            _ => None,
        };
        let r = Reglages::from_env(&env).unwrap();
        assert_eq!(
            (r.racine.as_deref(), r.profil_qualite, r.profil_metadonnees),
            (Some("/music"), Some(2), Some(3))
        );
        assert_eq!((r.par_jour, r.plafond), (4, Some(500 * GO)));
        assert_eq!(r.delai, Duration::from_secs(3600));
        let defaut = Reglages::from_env(&|_| None).unwrap();
        assert_eq!(
            (
                defaut.racine,
                defaut.profil_qualite,
                defaut.profil_metadonnees
            ),
            (None, None, None)
        );
        let zero = Reglages::from_env(&|k| (k == "SILLON_LIDARR_QUALITE").then(|| "0".into()));
        assert_eq!(zero.unwrap().profil_qualite, None);
        assert!(
            Reglages::from_env(&|k| (k == "SILLON_LIDARR_METADONNEES").then(|| "x".into()))
                .is_err()
        );
        assert_eq!((defaut.par_jour, defaut.plafond), (10, None));
        assert_eq!(
            (defaut.decouverte_par_jour, defaut.decouverte_budget),
            (2, 20 * GO)
        );
        let regle = Reglages::from_env(&|k| match k {
            "SILLON_DECOUVERTE_PAR_JOUR" => Some("1".into()),
            "SILLON_DECOUVERTE_GO" => Some("0".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!((regle.decouverte_par_jour, regle.decouverte_budget), (1, 0));
        assert!(
            Reglages::from_env(&|k| (k == "SILLON_DECOUVERTE_GO").then(|| "-1".into())).is_err()
        );
        assert_eq!(defaut.delai, Duration::from_secs(72 * 3600));
        assert!(Reglages::from_env(&|k| (k == "SILLON_MEDIAS_GO").then(|| "x".into())).is_err());
    }
}
