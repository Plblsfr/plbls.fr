use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};

#[cfg(test)]
pub use commun::identite::verify;
pub use commun::identite::{Champs, sign};

/// Identité propagée aux micro-services.
/// `roles` = rôles realm tels quels + rôles client sous la forme `client:role`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub email: String,
    pub roles: Vec<String>,
    pub client: String,
}

impl Identity {
    pub fn has_any_role(&self, wanted: &[String]) -> bool {
        wanted.iter().any(|w| self.roles.contains(w))
    }
}

const A_ENCODER: &AsciiSet = &CONTROLS.add(b' ').add(b'%');

pub fn encoder(v: &str) -> String {
    utf8_percent_encode(v, A_ENCODER).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_en_ascii_visible() {
        assert_eq!(encoder("demo@plbls.fr"), "demo@plbls.fr");
        assert_eq!(encoder("Jérôme Lœuf"), "J%C3%A9r%C3%B4me%20L%C5%93uf");
        assert_eq!(encoder("a%b\nc"), "a%25b%0Ac");
        assert!(axum::http::HeaderValue::from_str(&encoder("Zoé 😀")).is_ok());
    }
}
