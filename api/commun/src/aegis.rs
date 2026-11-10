use serde_json::Value;

use crate::http::cause;

pub async fn envoyer_manifeste(
    http: &reqwest::Client,
    url: &str,
    jeton: &str,
    manifeste: &Value,
) -> Result<(), String> {
    let r = http
        .put(format!("{}/v1/manifest", url.trim_end_matches('/')))
        .bearer_auth(jeton)
        .json(manifeste)
        .send()
        .await
        .map_err(|e| format!("aegis injoignable ({})", cause(&e)))?;
    if r.status().is_success() {
        Ok(())
    } else {
        Err(format!("aegis a répondu HTTP {}", r.status().as_u16()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::http::{client, tests::serveur};

    #[tokio::test]
    async fn manifeste_envoye_avec_le_jeton() {
        let (url, tache) =
            serveur("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        let http = client(Duration::from_secs(5));
        let m = serde_json::json!({ "app": "x" });
        envoyer_manifeste(&http, &format!("{url}/"), "jeton", &m)
            .await
            .unwrap();
        let requete = tache.await.unwrap();
        assert!(requete.starts_with("PUT /v1/manifest "), "{requete}");
        assert!(
            requete
                .to_ascii_lowercase()
                .contains("authorization: bearer jeton")
        );
        assert!(requete.ends_with(r#"{"app":"x"}"#), "{requete}");
    }

    #[tokio::test]
    async fn refus_et_injoignable() {
        let (url, tache) =
            serveur("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        let http = client(Duration::from_secs(5));
        let m = serde_json::json!({});
        assert_eq!(
            envoyer_manifeste(&http, &url, "j", &m).await.unwrap_err(),
            "aegis a répondu HTTP 401"
        );
        tache.await.unwrap();
        let e = envoyer_manifeste(&http, "http://127.0.0.1:1", "j", &m)
            .await
            .unwrap_err();
        assert!(e.starts_with("aegis injoignable ("), "{e}");
    }
}
