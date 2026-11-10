use std::path::PathBuf;

pub const TTLS: [u32; 4] = [3600, 86400, 604800, 2592000];

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub gateway_url: String,
    pub gateway_internal_url: String,
    pub deposit_permission: String,
    pub max_bytes: usize,
    pub max_pending: usize,
    pub permission: Option<(String, String)>,
    pub aegis: Option<(String, String)>,
    pub public_dir: PathBuf,
}

pub fn nettoie(valeur: &str) -> String {
    let v = valeur.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

fn nombre<T: std::str::FromStr>(nom: &str, valeur: &str) -> Result<T, String> {
    valeur
        .trim()
        .parse()
        .map_err(|_| format!("{nom} invalide : {valeur}"))
}

pub fn dossier_public() -> PathBuf {
    let local = PathBuf::from("public");
    if local.join("index.html").is_file() {
        local
    } else {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/pli/public"))
    }
}

impl Config {
    pub fn from_env(brut: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let env = |k: &str| brut(k).map(|v| nettoie(&v)).filter(|v| !v.is_empty());
        let database_url = brut("PLI_DATABASE_URL")
            .filter(|v| !v.is_empty())
            .ok_or("PLI_DATABASE_URL manquant (ex. postgres://user:pass@host:5432/pli)")?;
        let port = match brut("PORT").filter(|v| !v.is_empty()) {
            Some(p) => nombre("PORT", &p)?,
            None => 8080,
        };
        let gateway_url = env("GATEWAY_URL").unwrap_or_else(|| "https://security.plbls.fr".into());
        let gateway_internal_url =
            env("GATEWAY_INTERNAL_URL").unwrap_or_else(|| gateway_url.clone());
        let max_bytes = match env("MAX_BYTES") {
            Some(v) => nombre("MAX_BYTES", &v)?,
            None => 25 * 1024 * 1024,
        };
        let max_pending = match env("MAX_PENDING_PER_USER") {
            Some(v) => nombre("MAX_PENDING_PER_USER", &v)?,
            None => 50,
        };
        Ok(Self {
            port,
            database_url,
            gateway_url,
            gateway_internal_url,
            deposit_permission: env("DEPOSIT_PERMISSION").unwrap_or_else(|| "pli:deposit".into()),
            max_bytes,
            max_pending,
            permission: env("PERMISSION_URL").zip(env("PERMISSION_SERVICE_TOKEN")),
            aegis: env("AEGIS_URL").zip(env("PLI_AEGIS_TOKEN")),
            public_dir: dossier_public(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge(paires: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_env(|k| {
            paires
                .iter()
                .find(|(c, _)| *c == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn valeurs_par_defaut_et_guillemets() {
        assert!(charge(&[]).unwrap_err().contains("PLI_DATABASE_URL"));
        let c = charge(&[("PLI_DATABASE_URL", "postgres://x/pli")]).unwrap();
        assert_eq!(c.port, 8080);
        assert_eq!(c.gateway_url, "https://security.plbls.fr");
        assert_eq!(c.gateway_internal_url, c.gateway_url);
        assert_eq!(c.deposit_permission, "pli:deposit");
        assert_eq!(c.max_bytes, 25 * 1024 * 1024);
        assert_eq!(c.max_pending, 50);
        assert!(c.permission.is_none());
        let c = charge(&[
            ("PLI_DATABASE_URL", "postgres://x/pli"),
            ("PORT", "9000"),
            ("GATEWAY_URL", " \"https://g\" "),
            ("PERMISSION_URL", "'http://p'"),
            ("PERMISSION_SERVICE_TOKEN", "\"tok\""),
            ("MAX_BYTES", "1000"),
        ])
        .unwrap();
        assert_eq!(c.port, 9000);
        assert_eq!(c.gateway_url, "https://g");
        assert_eq!(c.gateway_internal_url, "https://g");
        assert_eq!(c.permission, Some(("http://p".into(), "tok".into())));
        assert_eq!(c.max_bytes, 1000);
        assert!(c.public_dir.join("index.html").is_file());
    }
}
