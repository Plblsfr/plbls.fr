use reqwest::Client;
use serde::Deserialize;

use crate::youtube::lire_limite;

pub const BASE: &str = "https://www.youtube.com/oembed";
const AGENT: &str = "Mozilla/5.0 (compatible; EstompeLinkPreview/1.1; +https://estompe.plbls.fr)";
const CORPS_MAX: usize = 64 << 10;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Info {
    pub titre: String,
    pub auteur: String,
}

#[derive(Deserialize, Default)]
struct Reponse {
    #[serde(default)]
    title: String,
    #[serde(default)]
    author_name: String,
}

pub struct Oembed {
    base: String,
    http: Client,
}

fn compacter(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

impl Oembed {
    pub fn new(base: &str, http: Client) -> Self {
        Self {
            base: base.to_string(),
            http,
        }
    }

    pub async fn recuperer(&self, id_video: &str) -> Result<Info, String> {
        let cible = format!("https://www.youtube.com/watch?v={id_video}");
        let reponse = self
            .http
            .get(&self.base)
            .query(&[("url", cible.as_str()), ("format", "json")])
            .header(reqwest::header::USER_AGENT, AGENT)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(format!("oEmbed : statut {}", reponse.status()));
        }
        let corps = lire_limite(reponse, CORPS_MAX)
            .await
            .map_err(|e| e.without_url().to_string())?;
        let r: Reponse = serde_json::from_slice(&corps).map_err(|e| e.to_string())?;
        Ok(Info {
            titre: compacter(&r.title, 200),
            auteur: compacter(&r.author_name, 100),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AGENT, Info, Oembed};
    use crate::youtube::http;
    use estompe_core::testutil::faux;

    #[tokio::test]
    async fn recupere_le_titre_et_l_auteur() {
        let f = faux(|_| {
            (
                200,
                r#"{"title":"  Mon   titre ","author_name":"Artiste - Topic"}"#.into(),
            )
        })
        .await;
        let o = Oembed::new(&f.url, http());
        assert_eq!(
            o.recuperer("dQw4w9WgXcQ").await.unwrap(),
            Info {
                titre: "Mon titre".into(),
                auteur: "Artiste - Topic".into()
            }
        );
        let r = &f.requetes()[0];
        assert!(r.query.contains("format=json"));
        assert!(r.query.contains("watch%3Fv%3DdQw4w9WgXcQ"));
        assert_eq!(r.entete("user-agent"), AGENT);
    }

    #[tokio::test]
    async fn tronque_le_titre_et_l_auteur() {
        let f = faux(|_| {
            (
                200,
                format!(
                    r#"{{"title":"{}","author_name":"{}"}}"#,
                    "é".repeat(201),
                    "a".repeat(100)
                )
                .into_bytes(),
            )
        })
        .await;
        let i = Oembed::new(&f.url, http())
            .recuperer("dQw4w9WgXcQ")
            .await
            .unwrap();
        assert_eq!(i.titre, format!("{}…", "é".repeat(200)));
        assert_eq!(i.auteur, "a".repeat(100));
    }

    #[tokio::test]
    async fn erreur_si_le_statut_n_est_pas_200() {
        let f = faux(|_| (404, vec![])).await;
        assert!(
            Oembed::new(&f.url, http())
                .recuperer("dQw4w9WgXcQ")
                .await
                .is_err()
        );
        let f = faux(|_| (200, b"pas du json".to_vec())).await;
        assert!(
            Oembed::new(&f.url, http())
                .recuperer("dQw4w9WgXcQ")
                .await
                .is_err()
        );
    }
}
