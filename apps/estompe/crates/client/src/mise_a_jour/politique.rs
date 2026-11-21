use std::fmt;
use std::sync::Arc;

use url::Url;

pub const HOTES_PRODUCTION: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];
pub const REDIRECTIONS_MAX: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refus {
    Illisible,
    Schema(String),
    Identifiants,
    Port(u16),
    Hote(String),
}

impl fmt::Display for Refus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Illisible => f.write_str("adresse illisible"),
            Self::Schema(schema) => write!(f, "« {schema} » au lieu de https"),
            Self::Identifiants => f.write_str("identifiants dans l'adresse"),
            Self::Port(port) => write!(f, "port {port} non autorisé"),
            Self::Hote(hote) => write!(f, "hôte « {hote} » hors de la liste autorisée"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolitiqueAdresses {
    pub https_seulement: bool,
    pub ports_explicites: bool,
    pub hotes: Vec<String>,
}

impl PolitiqueAdresses {
    pub fn production() -> Self {
        Self {
            https_seulement: true,
            ports_explicites: false,
            hotes: HOTES_PRODUCTION.iter().map(|h| h.to_string()).collect(),
        }
    }

    pub fn accepter(&self, adresse: &str) -> Result<Url, Refus> {
        let url = Url::parse(adresse.trim()).map_err(|_| Refus::Illisible)?;
        match url.scheme() {
            "https" => {}
            "http" if !self.https_seulement => {}
            autre => return Err(Refus::Schema(autre.to_string())),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Refus::Identifiants);
        }
        if let (Some(port), false) = (url.port(), self.ports_explicites) {
            return Err(Refus::Port(port));
        }
        let hote = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if !self.hotes.contains(&hote) {
            return Err(Refus::Hote(hote));
        }
        Ok(url)
    }

    pub fn redirections(self: &Arc<Self>) -> reqwest::redirect::Policy {
        let politique = self.clone();
        reqwest::redirect::Policy::custom(move |essai| {
            if essai.previous().len() > REDIRECTIONS_MAX {
                return essai.error(format!("plus de {REDIRECTIONS_MAX} redirections"));
            }
            match politique.accepter(essai.url().as_str()) {
                Ok(_) => essai.follow(),
                Err(refus) => essai.error(format!("redirection refusée : {refus}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_production_n_accepte_que_https_sur_les_hotes_de_github() {
        let p = PolitiqueAdresses::production();
        for bonne in [
            "https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json",
            "https://objects.githubusercontent.com/github-production-release-asset/1/2",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sig=x",
            "https://GitHub.com/Plblsfr/plbls.fr/releases/download/x/y.exe",
            "https://github.com:443/a",
        ] {
            assert!(p.accepter(bonne).is_ok(), "{bonne}");
        }
        assert_eq!(
            p.accepter("http://github.com/a"),
            Err(Refus::Schema("http".into()))
        );
        assert_eq!(
            p.accepter("ftp://github.com/a"),
            Err(Refus::Schema("ftp".into()))
        );
        assert_eq!(
            p.accepter("https://evil.example/a"),
            Err(Refus::Hote("evil.example".into()))
        );
        assert_eq!(
            p.accepter("https://github.com.evil.example/a"),
            Err(Refus::Hote("github.com.evil.example".into()))
        );
        assert_eq!(
            p.accepter("https://evilgithub.com/a"),
            Err(Refus::Hote("evilgithub.com".into()))
        );
        assert_eq!(
            p.accepter("https://user@github.com/a"),
            Err(Refus::Identifiants)
        );
        assert_eq!(
            p.accepter("https://user:mdp@github.com/a"),
            Err(Refus::Identifiants)
        );
        assert_eq!(
            p.accepter("https://github.com:8443/a"),
            Err(Refus::Port(8443))
        );
        assert_eq!(p.accepter("pas une adresse"), Err(Refus::Illisible));
        assert_eq!(
            p.accepter("https://140.82.121.4/a"),
            Err(Refus::Hote("140.82.121.4".into()))
        );
    }

    #[test]
    fn une_politique_d_essai_accepte_le_serveur_local_et_son_port() {
        let p = PolitiqueAdresses {
            https_seulement: false,
            ports_explicites: true,
            hotes: vec!["127.0.0.1".into()],
        };
        assert!(p.accepter("http://127.0.0.1:4321/natif.json").is_ok());
        assert!(p.accepter("http://localhost:4321/natif.json").is_err());
    }
}
