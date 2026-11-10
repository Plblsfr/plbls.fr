use serde::{Deserialize, Serialize};

pub use commun::identite::sign;

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
