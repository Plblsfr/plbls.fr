use axum::response::Response;
use serde_json::json;

use crate::{garde::Personne, http::ok};

pub async fn moi(p: Personne) -> Response {
    ok(&json!({
        "sub": p.identite.sub,
        "name": p.identite.nom,
        "email": p.identite.email,
        "permissions": p.droits,
    }))
}

pub async fn sante() -> Response {
    ok(&json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
