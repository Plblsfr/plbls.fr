use url::Url;

use crate::settings::Server;

/// Origine (`scheme://host[:port]`) autorisée : https sur plbls.fr ou un sous-domaine,
/// ou présente dans `extra_origins`.
pub fn origin_allowed(server: &Server, origin: &str) -> bool {
    let origin = origin.trim_end_matches('/');
    if server
        .extra_origins
        .iter()
        .any(|o| o.trim_end_matches('/') == origin)
    {
        return true;
    }
    let Ok(url) = Url::parse(origin) else {
        return false;
    };
    if url.scheme() != "https" || url.path() != "/" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let suffix = &server.allowed_origin_suffix;
    host == suffix || host.ends_with(&format!(".{suffix}"))
}

/// URL de redirection valide (même règle sur son origine), sinon l'URL par défaut : pas d'open redirect.
pub fn safe_redirect(server: &Server, wanted: Option<&str>) -> String {
    if let Some(w) = wanted
        && let Ok(url) = Url::parse(w)
    {
        let origin = url.origin().ascii_serialization();
        if origin_allowed(server, &origin) && url.username().is_empty() {
            return url.to_string();
        }
    }
    server.default_redirect.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            bind: String::new(),
            public_url: "https://security.plbls.fr".into(),
            default_redirect: "https://plbls.fr/".into(),
            allowed_origin_suffix: "plbls.fr".into(),
            extra_origins: vec!["chrome-extension://abc".into()],
            ws_max_lifetime_secs: 900,
        }
    }

    #[test]
    fn origins() {
        let s = server();
        assert!(origin_allowed(&s, "https://plbls.fr"));
        assert!(origin_allowed(&s, "https://app.plbls.fr"));
        assert!(origin_allowed(&s, "chrome-extension://abc"));
        assert!(!origin_allowed(&s, "http://app.plbls.fr"));
        assert!(!origin_allowed(&s, "https://evilplbls.fr"));
        assert!(!origin_allowed(&s, "https://plbls.fr.evil.com"));
    }

    #[test]
    fn redirects() {
        let s = server();
        assert_eq!(
            safe_redirect(&s, Some("https://app.plbls.fr/x?y=1")),
            "https://app.plbls.fr/x?y=1"
        );
        assert_eq!(
            safe_redirect(&s, Some("https://evil.com/")),
            "https://plbls.fr/"
        );
        assert_eq!(
            safe_redirect(&s, Some("https://plbls.fr@evil.com/")),
            "https://plbls.fr/"
        );
        assert_eq!(safe_redirect(&s, Some("//evil.com")), "https://plbls.fr/");
        assert_eq!(safe_redirect(&s, None), "https://plbls.fr/");
    }
}
