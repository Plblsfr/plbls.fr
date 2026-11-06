//! Connexion OAuth (boucle locale), requêtes natives vers Keycloak et le gateway, coffre de secrets.

use crate::callback;
use std::net::TcpListener;
use std::sync::Mutex;

use tauri::State;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::Manager;

/// Serveur loopback de la redirection OAuth (RFC 8252) : écoute sur 127.0.0.1, port choisi par l'OS.
#[derive(Default)]
pub struct OAuth(Mutex<Option<TcpListener>>);

/// Ouvre l'écoute et renvoie le port à mettre dans redirect_uri.
#[tauri::command]
pub fn oauth_start(state: State<OAuth>) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    *state.0.lock().map_err(|e| e.to_string())? = Some(listener);
    Ok(port)
}

/// Attend la redirection de Keycloak et renvoie la query string (code, state ou error).
#[tauri::command]
pub async fn oauth_wait(#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(unused_variables))] app: tauri::AppHandle, state: State<'_, OAuth>) -> Result<String, String> {
    let listener = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("connexion non démarrée")?;
    let query = tauri::async_runtime::spawn_blocking(move || callback::accept(listener))
        .await
        .map_err(|e| e.to_string())??;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    show_main(&app); // le navigateur a le focus : on ramène Estompe au premier plan
    Ok(query)
}

#[derive(serde::Serialize)]
pub struct NativeResponse {
    pub status: u16,
    pub body: String,
}

/// Hôtes joignables par native_fetch : Keycloak, le gateway et le serveur de mises à jour. Rien d'autre.
const NATIVE_HOSTS: [&str; 3] = ["auth.plbls.fr", "security.plbls.fr", "estompe.plbls.fr"];
/// En-têtes que la webview peut poser (jamais Origin, Cookie ni Host : ils contourneraient les protections CSRF).
const NATIVE_HEADERS: [&str; 3] = ["authorization", "content-type", "accept"];

fn check_native(method: &str, u: &reqwest::Url, headers: &[(String, String)]) -> Result<(), &'static str> {
    if u.scheme() != "https" || !u.host_str().is_some_and(|h| NATIVE_HOSTS.contains(&h)) || u.port().is_some() {
        return Err("adresse non autorisée");
    }
    if !matches!(method, "GET" | "POST") {
        return Err("méthode non autorisée");
    }
    if headers.iter().any(|(k, _)| !NATIVE_HEADERS.contains(&k.to_ascii_lowercase().as_str())) {
        return Err("en-tête non autorisé");
    }
    Ok(())
}

/// Requête vers Keycloak ou le gateway faite depuis Rust : le webview enverrait l'en-tête Origin (https://tauri.localhost), que Keycloak refuse.
#[tauri::command]
pub async fn native_fetch(method: String, url: String, headers: Vec<(String, String)>, body: Option<String>) -> Result<NativeResponse, String> {
    let u = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    check_native(&method, &u, &headers)?;
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = reqwest::Client::new().request(m, u);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.body(b);
    }
    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    Ok(NativeResponse { status, body: res.text().await.map_err(|e| e.to_string())? })
}

// Service = identifiant de l'app : le build de test (autre identifiant) a son propre coffre.
fn entry(app: &tauri::AppHandle, key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(&app.config().identifier, key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_set(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    entry(&app, &key)?.set_password(&value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_get(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    match entry(&app, &key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn secret_delete(app: tauri::AppHandle, key: String) -> Result<(), String> {
    match entry(&app, &key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        // Windows refuse parfois de donner le focus à une app en arrière-plan : passer brièvement « toujours au-dessus » le force
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
        let _ = w.set_always_on_top(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(method: &str, url: &str, headers: &[(&str, &str)]) -> Result<(), &'static str> {
        let h: Vec<_> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        check_native(method, &reqwest::Url::parse(url).unwrap(), &h)
    }

    #[test]
    fn native_fetch_liste_blanche() {
        assert!(check("POST", "https://auth.plbls.fr/realms/default/protocol/openid-connect/token", &[("Content-Type", "x")]).is_ok());
        assert!(check("GET", "https://security.plbls.fr/auth/me", &[("Authorization", "Bearer x")]).is_ok());
        assert!(check("GET", "https://estompe.plbls.fr/updates/android.json", &[]).is_ok());
        for url in ["https://evil.plbls.fr/", "https://chat.plbls.fr/", "https://plbls.fr/", "http://auth.plbls.fr/", "https://auth.plbls.fr:8443/", "https://auth.plbls.fr.evil.com/"] {
            assert!(check("GET", url, &[]).is_err(), "{url}");
        }
        assert!(check("DELETE", "https://auth.plbls.fr/", &[]).is_err());
        for h in ["Origin", "origin", "Cookie", "Host", "X-Autre"] {
            assert!(check("GET", "https://auth.plbls.fr/", &[(h, "x")]).is_err(), "{h}");
        }
    }
}
