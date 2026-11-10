use std::collections::HashMap;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

pub const FICHIER: &str = "/etc/plbls.env";

#[derive(Debug, Clone, Default)]
pub struct Env {
    valeurs: HashMap<String, String>,
}

impl Env {
    pub fn charger() -> Self {
        let chemin = std::env::var("PLBLS_ENV").unwrap_or_else(|_| FICHIER.to_string());
        let fichier = std::fs::read_to_string(&chemin).unwrap_or_default();
        Self::fusion(&fichier, std::env::vars())
    }

    pub fn fusion(fichier: &str, processus: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut valeurs = lire_fichier(fichier);
        valeurs.extend(processus);
        Env { valeurs }
    }

    pub fn depuis<'a>(paires: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Env {
            valeurs: paires
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    pub fn get(&self, cle: &str) -> Option<String> {
        self.valeurs.get(cle).cloned()
    }

    pub fn vars(&self) -> &HashMap<String, String> {
        &self.valeurs
    }

    pub fn jetons(&self, prefixe: &str) -> String {
        jetons(self.valeurs.iter(), prefixe)
    }
}

pub fn lire_fichier(contenu: &str) -> HashMap<String, String> {
    contenu
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), crate::config::nettoie(v)))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

pub fn jetons<'a>(
    vars: impl IntoIterator<Item = (&'a String, &'a String)>,
    prefixe: &str,
) -> String {
    let mut paires: Vec<String> = vars
        .into_iter()
        .filter_map(|(k, v)| {
            let nom = k.strip_prefix(prefixe)?;
            let v = crate::config::nettoie(v);
            (!nom.is_empty() && !v.is_empty())
                .then(|| format!("{}:{v}", nom.to_lowercase().replace('_', "-")))
        })
        .collect();
    paires.sort();
    paires.join(",")
}

fn encode(valeur: &str) -> String {
    utf8_percent_encode(valeur, NON_ALPHANUMERIC).to_string()
}

fn decoupe(url: &str) -> Option<(&str, &str, &str)> {
    let (schema, reste) = url.split_once("://")?;
    let fin = reste.find(['/', '?']).unwrap_or(reste.len());
    let autorite = &reste[..fin];
    let requete = reste[fin..].find('?').map_or("", |i| &reste[fin + i..]);
    Some((schema, autorite, requete))
}

fn identifiants(utilisateur: &str, mot_de_passe: &str) -> String {
    match (utilisateur.is_empty(), mot_de_passe.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{}@", encode(utilisateur)),
        _ => format!("{}:{}@", encode(utilisateur), encode(mot_de_passe)),
    }
}

fn hote(autorite: &str, remplace: bool) -> &str {
    match autorite.rsplit_once('@') {
        Some((_, h)) if remplace => h,
        _ => autorite,
    }
}

pub fn url_base(get: impl Fn(&str) -> Option<String>, base: &str) -> Result<String, String> {
    let lu = |k: &str| {
        get(k)
            .map(|v| crate::config::nettoie(&v))
            .unwrap_or_default()
    };
    let serveur = lu("DATABASE_URL");
    if serveur.is_empty() {
        return Err(
            "DATABASE_URL manquant (serveur PostgreSQL, ex. postgres://postgres-main:5432)".into(),
        );
    }
    let (schema, autorite, requete) =
        decoupe(&serveur).ok_or_else(|| format!("DATABASE_URL invalide : {serveur}"))?;
    let (utilisateur, mot_de_passe) = (lu("DATABASE_USER"), lu("DATABASE_PASSWORD"));
    let remplace = !utilisateur.is_empty() || !mot_de_passe.is_empty();
    Ok(format!(
        "{schema}://{}{}/{base}{requete}",
        identifiants(&utilisateur, &mot_de_passe),
        hote(autorite, remplace)
    ))
}

pub fn url_redis(get: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let lu = |k: &str| {
        get(k)
            .map(|v| crate::config::nettoie(&v))
            .unwrap_or_default()
    };
    let url = lu("REDIS_URL");
    let mot_de_passe = lu("REDIS_PASSWORD");
    if url.is_empty() || mot_de_passe.is_empty() {
        return Ok(url);
    }
    let (schema, autorite, _) =
        decoupe(&url).ok_or_else(|| format!("REDIS_URL invalide : {url}"))?;
    let reste = &url[schema.len() + 3 + autorite.len()..];
    Ok(format!(
        "{schema}://:{}@{}{reste}",
        encode(&mot_de_passe),
        hote(autorite, true)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lecteur<'a>(paires: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |k| {
            paires
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn fichier_puis_processus() {
        let env = Env::fusion(
            "# commentaire\n\nA=1\nB=\"deux\"\n C = 3 \nsans-egal\n",
            [("A".to_string(), "env".to_string())],
        );
        assert_eq!(env.get("A").as_deref(), Some("env"));
        assert_eq!(env.get("B").as_deref(), Some("deux"));
        assert_eq!(env.get("C").as_deref(), Some("3"));
        assert_eq!(env.get("sans-egal"), None);
    }

    #[test]
    fn base_construite_depuis_le_serveur_commun() {
        let get = lecteur(&[
            ("DATABASE_URL", "postgres://postgres-main:5432"),
            ("DATABASE_USER", "plbls"),
            ("DATABASE_PASSWORD", "a@b:c/d"),
        ]);
        assert_eq!(
            url_base(&get, "aegis").unwrap(),
            "postgres://plbls:a%40b%3Ac%2Fd@postgres-main:5432/aegis"
        );
    }

    #[test]
    fn base_garde_les_options_et_les_identifiants_de_l_url() {
        let get = lecteur(&[(
            "DATABASE_URL",
            "postgres://u:p@h:5432/postgres?sslmode=disable",
        )]);
        assert_eq!(
            url_base(&get, "pli").unwrap(),
            "postgres://u:p@h:5432/pli?sslmode=disable"
        );
        let get = lecteur(&[
            ("DATABASE_URL", "postgres://u:p@h/?sslmode=require"),
            ("DATABASE_USER", "plbls"),
        ]);
        assert_eq!(
            url_base(&get, "pli").unwrap(),
            "postgres://plbls@h/pli?sslmode=require"
        );
    }

    #[test]
    fn base_sans_serveur_refusee() {
        assert!(
            url_base(lecteur(&[]), "pli")
                .unwrap_err()
                .contains("DATABASE_URL")
        );
        assert!(url_base(lecteur(&[("DATABASE_URL", "postgres-main")]), "pli").is_err());
    }

    #[test]
    fn redis_avec_mot_de_passe() {
        let get = lecteur(&[
            ("REDIS_URL", "redis://redis:6379/0"),
            ("REDIS_PASSWORD", "s3c r"),
        ]);
        assert_eq!(url_redis(&get).unwrap(), "redis://:s3c%20r@redis:6379/0");
        let get = lecteur(&[("REDIS_URL", "redis://127.0.0.1:6379")]);
        assert_eq!(url_redis(&get).unwrap(), "redis://127.0.0.1:6379");
        assert_eq!(url_redis(lecteur(&[("REDIS_PASSWORD", "x")])).unwrap(), "");
    }

    #[test]
    fn jetons_par_prefixe() {
        let env = Env::depuis([
            ("AEGIS_TOKEN_PLI", "b"),
            ("AEGIS_TOKEN_ESTOMPE", "\"a\""),
            ("AEGIS_TOKEN_VIDE", ""),
            ("AEGIS_TOKEN_", "x"),
            ("AEGIS_URL", "http://aegis:8080"),
        ]);
        assert_eq!(env.jetons("AEGIS_TOKEN_"), "estompe:a,pli:b");
    }
}
