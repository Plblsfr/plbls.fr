use std::fmt;

use crate::traits::TypeSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdSource {
    Ecran(u32),
    Fenetre(u32),
}

impl IdSource {
    pub fn lire(id: &str) -> Option<Self> {
        let (genre, numero) = id.split_once(':')?;
        let numero = numero.parse().ok()?;
        match genre {
            "ecran" => Some(Self::Ecran(numero)),
            "fenetre" => Some(Self::Fenetre(numero)),
            _ => None,
        }
    }

    pub fn genre(self) -> TypeSource {
        match self {
            Self::Ecran(_) => TypeSource::Ecran,
            Self::Fenetre(_) => TypeSource::Fenetre,
        }
    }

    pub fn numero(self) -> u32 {
        match self {
            Self::Ecran(n) | Self::Fenetre(n) => n,
        }
    }
}

impl fmt::Display for IdSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ecran(n) => write!(f, "ecran:{n}"),
            Self::Fenetre(n) => write!(f, "fenetre:{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_identifiant_fait_l_aller_retour() {
        for id in [IdSource::Ecran(0), IdSource::Fenetre(4_294_967_295)] {
            assert_eq!(IdSource::lire(&id.to_string()), Some(id));
        }
        assert_eq!(IdSource::Fenetre(7).genre(), TypeSource::Fenetre);
        assert_eq!(IdSource::Ecran(3).numero(), 3);
    }

    #[test]
    fn un_identifiant_inconnu_est_refuse() {
        for id in [
            "",
            "ecran",
            "ecran:",
            "ecran:-1",
            "m:3",
            "fenetre:x",
            "camera:1",
        ] {
            assert_eq!(IdSource::lire(id), None, "{id}");
        }
    }
}
