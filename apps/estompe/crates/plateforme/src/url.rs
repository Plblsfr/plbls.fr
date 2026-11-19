use crate::{Erreur, OuvertureUrl, Resultat};

#[derive(Debug, Clone, Copy, Default)]
pub struct NavigateurSysteme;

pub(crate) fn url_permise(url: &str) -> bool {
    let reste = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    matches!(reste, Some(r) if !r.is_empty() && !r.starts_with('/'))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

impl OuvertureUrl for NavigateurSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn ouvrir(&self, url: &str) -> Resultat<()> {
        if !url_permise(url) {
            return Err(Erreur::Refuse(
                "seules les adresses http et https s'ouvrent dans le navigateur".into(),
            ));
        }
        webbrowser::open(url).map_err(|e| Erreur::Echec(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seules_les_adresses_http_et_https_sont_permises() {
        for url in ["https://plbls.fr", "http://localhost:8080/chemin?a=1#b"] {
            assert!(url_permise(url), "{url}");
        }
        for url in [
            "",
            "plbls.fr",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://",
            "https:///chemin",
            "https://plbls.fr/a b",
            "https://plbls.fr/\ncalc.exe",
            "HTTPS://plbls.fr",
        ] {
            assert!(!url_permise(url), "{url:?}");
        }
    }

    #[test]
    fn une_adresse_refusee_n_ouvre_rien() {
        assert!(matches!(
            NavigateurSysteme.ouvrir("file:///etc/passwd"),
            Err(Erreur::Refuse(_))
        ));
    }
}
