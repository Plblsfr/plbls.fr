use std::{pin::Pin, sync::Arc, time::Duration};

use estompe_core::state::{AppState, FilmEtSuite, FilmResolu, Films, RefusFilm};
use serde_json::Value;

pub struct Entracte {
    url: String,
    jeton: String,
    http: reqwest::Client,
}

fn resolu(v: &Value) -> Option<FilmResolu> {
    let id = v["id"].as_str().filter(|i| super::uuid_valide(i))?;
    Some(FilmResolu {
        id: id.to_string(),
        titre: v["title"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect(),
        duree: v["duration"]
            .as_f64()
            .filter(|d| d.is_finite() && *d > 0.0)
            .map_or(0, |d| d.round().min(f64::from(u32::MAX)) as u32),
    })
}

impl Entracte {
    pub fn nouveau(url: &str, jeton: &str) -> Self {
        Entracte {
            url: url.trim_end_matches('/').to_string(),
            jeton: jeton.to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client HTTP"),
        }
    }

    async fn chercher(&self, sub: &str, id: &str) -> FilmEtSuite {
        if !super::uuid_valide(id) {
            return Err(RefusFilm::Introuvable);
        }
        let rep = self
            .http
            .get(format!("{}/internal/titles/{id}", self.url))
            .query(&[("sub", sub)])
            .bearer_auth(&self.jeton)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("cinéma : Entracte injoignable : {}", e.without_url());
                RefusFilm::Indisponible
            })?;
        match rep.status().as_u16() {
            200 => {}
            404 => return Err(RefusFilm::Introuvable),
            s => {
                tracing::warn!("cinéma : Entracte a répondu HTTP {s}");
                return Err(RefusFilm::Indisponible);
            }
        }
        let octets = rep.bytes().await.map_err(|_| RefusFilm::Indisponible)?;
        if octets.len() > 1 << 20 {
            return Err(RefusFilm::Indisponible);
        }
        let v: Value = serde_json::from_slice(&octets).map_err(|_| RefusFilm::Indisponible)?;
        if v["allowed"] != Value::Bool(true) {
            return Err(RefusFilm::Permission);
        }
        let premier = resolu(&v).ok_or(RefusFilm::Indisponible)?;
        let suite = v["next"]
            .as_array()
            .map(|a| a.iter().filter_map(resolu).collect())
            .unwrap_or_default();
        Ok((premier, suite))
    }
}

impl Films for Entracte {
    fn resoudre<'a>(
        &'a self,
        sub: &'a str,
        id: &'a str,
    ) -> Pin<Box<dyn Future<Output = FilmEtSuite> + Send + 'a>> {
        Box::pin(self.chercher(sub, id))
    }
}

pub fn brancher(state: &AppState) {
    let c = state.config();
    if c.entracte_actif() {
        state.definir_films(Arc::new(Entracte::nouveau(
            &c.entracte_url,
            &c.entracte_token,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_core::testutil::faux;

    const ID: &str = "11111111-2222-3333-4444-555555555555";
    const SUIVANT: &str = "11111111-2222-3333-4444-666666666666";

    #[tokio::test]
    async fn titre_verifie_par_entracte() {
        let e = faux(|r| {
            if r.entete("authorization") != "Bearer jeton" {
                return (401, vec![]);
            }
            match (r.chemin.as_str(), r.query.as_str()) {
                (p, "sub=alice") if p == format!("/internal/titles/{ID}") => (
                    200,
                    format!(r#"{{"id":"{ID}","title":"S · S1E1 · Pilote","duration":2400.4,"allowed":true,"next":[{{"id":"{SUIVANT}","title":"S · S1E2","duration":2500}},{{"id":"/x","title":"y"}}]}}"#)
                        .into_bytes(),
                ),
                (p, "sub=bob") if p == format!("/internal/titles/{ID}") => {
                    (200, format!(r#"{{"id":"{ID}","title":"t","allowed":false,"next":[]}}"#).into_bytes())
                }
                _ => (404, vec![]),
            }
        })
        .await;
        let r = Entracte::nouveau(&e.url, "jeton");
        let (premier, suite) = r.resoudre("alice", ID).await.unwrap();
        assert_eq!(
            premier,
            FilmResolu {
                id: ID.into(),
                titre: "S · S1E1 · Pilote".into(),
                duree: 2400
            }
        );
        assert_eq!(suite.len(), 1);
        assert_eq!(suite[0].id, SUIVANT);
        assert_eq!(r.resoudre("bob", ID).await, Err(RefusFilm::Permission));
        assert_eq!(
            r.resoudre("alice", SUIVANT).await,
            Err(RefusFilm::Introuvable)
        );
        assert_eq!(
            r.resoudre("alice", "../admin").await,
            Err(RefusFilm::Introuvable)
        );
        let faux_jeton = Entracte::nouveau(&e.url, "autre");
        assert_eq!(
            faux_jeton.resoudre("alice", ID).await,
            Err(RefusFilm::Indisponible)
        );
        let url = e.url.clone();
        drop(e);
        assert_eq!(
            Entracte::nouveau(&url, "jeton").resoudre("alice", ID).await,
            Err(RefusFilm::Indisponible)
        );
    }
}
