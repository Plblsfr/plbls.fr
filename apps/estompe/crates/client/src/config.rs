use std::fmt;

use reqwest::Url;

pub const ADRESSE_PRODUCTION: &str = "https://chat.plbls.fr";
pub const VARIABLE_ADRESSE: &str = "ESTOMPE_API_URL";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origine {
    Environnement,
    Preference,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurAdresse {
    Vide,
    Illisible(String),
    Schema(String),
    SansHote,
    Identifiants,
    ParametresInterdits,
}

impl fmt::Display for ErreurAdresse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vide => write!(f, "adresse vide"),
            Self::Illisible(detail) => write!(f, "adresse illisible : {detail}"),
            Self::Schema(schema) => write!(f, "schéma « {schema} » refusé : http ou https attendu"),
            Self::SansHote => write!(f, "adresse sans nom d'hôte"),
            Self::Identifiants => write!(f, "l'adresse ne doit pas contenir d'identifiants"),
            Self::ParametresInterdits => write!(f, "l'adresse ne doit contenir ni « ? » ni « # »"),
        }
    }
}

impl std::error::Error for ErreurAdresse {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdresseServeur {
    pub url: String,
    pub origine: Origine,
    pub rejets: Vec<(Origine, ErreurAdresse)>,
}

pub fn valider_adresse(saisie: &str) -> Result<String, ErreurAdresse> {
    let saisie = saisie.trim();
    if saisie.is_empty() {
        return Err(ErreurAdresse::Vide);
    }
    let url = Url::parse(saisie).map_err(|e| ErreurAdresse::Illisible(e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ErreurAdresse::Schema(url.scheme().to_string()));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ErreurAdresse::SansHote);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ErreurAdresse::Identifiants);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ErreurAdresse::ParametresInterdits);
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub fn choisir_adresse(environnement: Option<&str>, preference: Option<&str>) -> AdresseServeur {
    let mut rejets = Vec::new();
    let candidats = [
        (Origine::Environnement, environnement),
        (Origine::Preference, preference),
    ];
    for (origine, valeur) in candidats {
        let Some(valeur) = valeur.filter(|v| !v.trim().is_empty()) else {
            continue;
        };
        match valider_adresse(valeur) {
            Ok(url) => {
                return AdresseServeur {
                    url,
                    origine,
                    rejets,
                }
            }
            Err(erreur) => rejets.push((origine, erreur)),
        }
    }
    AdresseServeur {
        url: ADRESSE_PRODUCTION.to_string(),
        origine: Origine::Production,
        rejets,
    }
}

pub fn adresse_serveur(preference: Option<&str>) -> AdresseServeur {
    choisir_adresse(std::env::var(VARIABLE_ADRESSE).ok().as_deref(), preference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_par_defaut() {
        let adresse = choisir_adresse(None, None);
        assert_eq!(adresse.url, "https://chat.plbls.fr");
        assert_eq!(adresse.origine, Origine::Production);
        assert!(adresse.rejets.is_empty());
        assert_eq!(
            valider_adresse(ADRESSE_PRODUCTION).unwrap(),
            ADRESSE_PRODUCTION
        );
    }

    #[test]
    fn la_preference_remplace_la_production_et_l_environnement_remplace_la_preference() {
        let preference = choisir_adresse(None, Some("http://localhost:8080/"));
        assert_eq!(
            (preference.url.as_str(), preference.origine),
            ("http://localhost:8080", Origine::Preference)
        );
        let environnement =
            choisir_adresse(Some("https://test.example"), Some("http://localhost:8080"));
        assert_eq!(
            (environnement.url.as_str(), environnement.origine),
            ("https://test.example", Origine::Environnement)
        );
    }

    #[test]
    fn une_valeur_invalide_est_ecartee_et_signalee() {
        let adresse = choisir_adresse(Some("ftp://test.example"), Some("pas une adresse"));
        assert_eq!(adresse.url, ADRESSE_PRODUCTION);
        assert_eq!(adresse.origine, Origine::Production);
        assert_eq!(adresse.rejets.len(), 2);
        assert_eq!(
            adresse.rejets[0],
            (
                Origine::Environnement,
                ErreurAdresse::Schema("ftp".to_string())
            )
        );
        assert_eq!(adresse.rejets[1].0, Origine::Preference);
        let vide = choisir_adresse(Some("  "), Some(""));
        assert_eq!(vide.origine, Origine::Production);
        assert!(vide.rejets.is_empty());
    }

    #[test]
    fn valide_et_normalise_une_adresse() {
        assert_eq!(
            valider_adresse(" https://Chat.Plbls.fr/ ").unwrap(),
            "https://chat.plbls.fr"
        );
        assert_eq!(
            valider_adresse("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            valider_adresse("https://test.example/estompe/").unwrap(),
            "https://test.example/estompe"
        );
        assert_eq!(valider_adresse(""), Err(ErreurAdresse::Vide));
        assert_eq!(
            valider_adresse("file:///etc/passwd"),
            Err(ErreurAdresse::Schema("file".to_string()))
        );
        assert_eq!(
            valider_adresse("https://moi:secret@test.example"),
            Err(ErreurAdresse::Identifiants)
        );
        assert_eq!(
            valider_adresse("https://test.example/?a=1"),
            Err(ErreurAdresse::ParametresInterdits)
        );
        assert_eq!(
            valider_adresse("https://test.example/#x"),
            Err(ErreurAdresse::ParametresInterdits)
        );
        assert!(matches!(
            valider_adresse("chat.plbls.fr"),
            Err(ErreurAdresse::Illisible(_))
        ));
    }
}
