use std::collections::HashMap;

use axum::http::HeaderMap;

struct Fenetre {
    n: u32,
    jusqua: u64,
}

pub struct RateLimiter {
    max: u32,
    fenetre_ms: u64,
    hits: HashMap<String, Fenetre>,
}

impl RateLimiter {
    pub fn new(max: u32, fenetre_ms: u64) -> Self {
        Self {
            max,
            fenetre_ms,
            hits: HashMap::new(),
        }
    }

    pub fn check(&mut self, cle: &str, maintenant: u64) -> bool {
        if let Some(h) = self.hits.get_mut(cle)
            && h.jusqua > maintenant
        {
            h.n = h.n.saturating_add(1);
            return h.n <= self.max;
        }
        if self.hits.len() > 10_000 {
            self.hits.retain(|_, v| v.jusqua > maintenant);
        }
        self.hits.insert(
            cle.to_string(),
            Fenetre {
                n: 1,
                jusqua: maintenant + self.fenetre_ms,
            },
        );
        true
    }
}

pub fn client_ip(entetes: &HeaderMap, socket: Option<std::net::SocketAddr>) -> String {
    let xff: Vec<&str> = entetes
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    let joint = xff.join(", ");
    if !joint.is_empty()
        && let Some(dernier) = joint.rsplit(',').next()
    {
        return dernier.trim().to_string();
    }
    socket.map_or_else(|| "?".to_string(), |s| s.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenetre_fixe_par_cle() {
        let mut ok = RateLimiter::new(2, 1000);
        assert!(ok.check("a", 0));
        assert!(ok.check("a", 1));
        assert!(!ok.check("a", 2));
        assert!(ok.check("b", 2));
        assert!(ok.check("a", 1001));
    }

    #[test]
    fn ip_du_dernier_saut() {
        let mut h = HeaderMap::new();
        let socket = "10.0.0.1:1234".parse().ok();
        assert_eq!(client_ip(&h, socket), "10.0.0.1");
        assert_eq!(client_ip(&h, None), "?");
        h.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2 ".parse().unwrap());
        assert_eq!(client_ip(&h, socket), "2.2.2.2");
    }
}
