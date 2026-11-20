use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erreur {
    Indisponible(&'static str),
    Refuse(String),
    Echec(String),
}

pub type Resultat<T> = Result<T, Erreur>;

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indisponible(fonction) => write!(f, "{fonction} : indisponible sur ce système"),
            Self::Refuse(raison) => write!(f, "refusé : {raison}"),
            Self::Echec(raison) => f.write_str(raison),
        }
    }
}

impl std::error::Error for Erreur {}
