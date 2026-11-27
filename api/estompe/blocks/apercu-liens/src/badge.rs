use std::{sync::OnceLock, time::Duration};

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use estompe_core::{
    http::{write_err, write_json},
    servers::uuid_valide,
    state::AppState,
    users::User,
};
use reqwest::Url;
use serde_json::{Value, json};

use crate::sillon::auteur;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cle {
    pub genre: &'static str,
    pub titre: String,
    pub saison: Option<u32>,
}

impl Cle {
    pub fn texte(&self) -> String {
        match self.saison {
            Some(n) => format!("{}:{}:{n}", self.genre, self.titre),
            None => format!("{}:{}", self.genre, self.titre),
        }
    }

    fn etiquette(&self) -> String {
        match (self.genre, self.saison) {
            ("movie", _) => "Film terminé".into(),
            (_, Some(n)) => format!("Saison {n} terminée"),
            _ => "Série terminée".into(),
        }
    }
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client HTTP")
    })
}

pub fn lire_cle(brut: &str) -> Option<Cle> {
    let brut = brut.replace("%3A", ":").replace("%3a", ":");
    let mut parties = brut.split(':');
    let genre = match parties.next()? {
        "movie" => "movie",
        "season" => "season",
        "series" => "series",
        _ => return None,
    };
    let titre = parties
        .next()
        .filter(|t| uuid_valide(t))?
        .to_ascii_lowercase();
    let saison = match genre {
        "season" => Some(
            parties
                .next()?
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=999).contains(n))?,
        ),
        _ => None,
    };
    parties.next().is_none().then_some(Cle {
        genre,
        titre,
        saison,
    })
}

pub fn lien(brut: &str, origines: &[String]) -> Option<Cle> {
    let u = Url::parse(brut.trim()).ok()?;
    let origine = u.origin().ascii_serialization();
    if !origines.iter().any(|o| o.trim_end_matches('/') == origine)
        || !matches!(u.path(), "" | "/")
        || u.query().is_some()
    {
        return None;
    }
    let cle = u.fragment()?.strip_prefix("/films/badge/")?;
    if cle.contains('/') {
        return None;
    }
    lire_cle(cle)
}

fn borne(v: &Value, max: usize) -> Option<String> {
    v.as_str().map(|s| s.chars().take(max).collect())
}

fn nombre(v: &Value) -> Value {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map_or(Value::Null, |n| json!(n))
}

pub fn carte_de(cle: &Cle, v: &Value) -> Option<Value> {
    let saison = v["season"].as_u64().map(|n| n as u32);
    if v["kind"] != cle.genre || v["title_id"] != cle.titre.as_str() || saison != cle.saison {
        return None;
    }
    let date = borne(&v["obtained_at"], 40)?;
    chrono::DateTime::parse_from_rfc3339(&date).ok()?;
    Some(json!({
        "kind": cle.genre,
        "title_id": cle.titre,
        "season": cle.saison,
        "title": borne(&v["title"], 300)?,
        "season_title": borne(&v["season_title"], 300),
        "year": v["year"].as_i64().filter(|a| (1880..=2100).contains(a)),
        "poster": v["poster"].as_bool().unwrap_or(false),
        "season_poster": v["season_poster"].as_bool().unwrap_or(false),
        "episodes": v["episodes"].as_u64(),
        "duration": nombre(&v["duration"]),
        "obtained_at": date,
    }))
}

pub async fn chercher(url: &str, jeton: &str, sub: &str, cle: &Cle) -> Option<Value> {
    let rep = client()
        .get(format!(
            "{}/internal/badges/{}",
            url.trim_end_matches('/'),
            cle.texte()
        ))
        .query(&[("sub", sub)])
        .bearer_auth(jeton)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(
                "carte de badge : Entracte injoignable : {}",
                e.without_url()
            )
        })
        .ok()?;
    if rep.status() != StatusCode::OK {
        if rep.status() != StatusCode::NOT_FOUND {
            tracing::warn!("carte de badge : Entracte a répondu HTTP {}", rep.status());
        }
        return None;
    }
    let octets = rep.bytes().await.ok()?;
    if octets.len() > 1 << 16 {
        return None;
    }
    let v: Value = serde_json::from_slice(&octets).ok()?;
    carte_de(cle, &v)
}

fn sans_carte() -> Response {
    write_err(
        StatusCode::NOT_FOUND,
        "no_preview",
        "pas d'aperçu pour ce lien",
    )
}

pub async fn carte(
    state: &AppState,
    lecteur: &User,
    cle: Cle,
    message: &str,
    url: &str,
) -> Response {
    let c = state.config();
    if !c.entracte_actif() {
        return sans_carte();
    }
    let Some(sub) = auteur(state, message, lecteur, url).await else {
        return sans_carte();
    };
    let Some(badge) = chercher(&c.entracte_url, &c.entracte_token, &sub, &cle).await else {
        return sans_carte();
    };
    let mut r = write_json(
        StatusCode::OK,
        &json!({
            "url": url.trim(),
            "title": badge["title"],
            "description": cle.etiquette(),
            "site_name": "Entracte",
            "entracte_badge": badge,
        }),
    );
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=600"),
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_core::testutil::faux;

    const ID: &str = "11111111-2222-3333-4444-555555555555";

    fn cle(genre: &'static str, saison: Option<u32>) -> Cle {
        Cle {
            genre,
            titre: ID.into(),
            saison,
        }
    }

    #[test]
    fn lien_d_un_badge_de_l_app() {
        let o = ["https://estompe.plbls.fr/".to_string()];
        assert_eq!(
            lien(
                &format!("https://estompe.plbls.fr/#/films/badge/season%3A{ID}%3A2"),
                &o
            ),
            Some(cle("season", Some(2)))
        );
        assert_eq!(
            lien(
                &format!(
                    " https://estompe.plbls.fr#/films/badge/movie:{}",
                    ID.to_uppercase()
                ),
                &o
            ),
            Some(cle("movie", None))
        );
        assert_eq!(
            lien(
                &format!("https://estompe.plbls.fr/#/films/badge/series:{ID}"),
                &o
            ),
            Some(cle("series", None))
        );
        for faux in [
            format!("https://autre.fr/#/films/badge/movie:{ID}"),
            format!("https://estompe.plbls.fr/?a=1#/films/badge/movie:{ID}"),
            format!("https://estompe.plbls.fr/x/#/films/badge/movie:{ID}"),
            format!("https://estompe.plbls.fr/#/films/badge/movie:{ID}/plus"),
            format!("https://estompe.plbls.fr/#/films/badge/movie:{ID}:1"),
            format!("https://estompe.plbls.fr/#/films/badge/season:{ID}"),
            format!("https://estompe.plbls.fr/#/films/badge/season:{ID}:0"),
            format!("https://estompe.plbls.fr/#/films/badge/episode:{ID}"),
            format!("https://estompe.plbls.fr/#/films/item/{ID}"),
            "https://estompe.plbls.fr/#/films/badge/movie:../admin".into(),
            "pas une url".into(),
        ] {
            assert_eq!(lien(&faux, &o), None, "{faux}");
        }
        assert_eq!(cle("season", Some(2)).texte(), format!("season:{ID}:2"));
        assert_eq!(cle("series", None).etiquette(), "Série terminée");
        assert_eq!(cle("season", Some(3)).etiquette(), "Saison 3 terminée");
    }

    fn badge() -> Value {
        json!({"kind": "season", "title_id": ID, "season": 2, "title": "La Série", "season_title": "Saison 2",
               "year": 2019, "poster": true, "season_poster": false, "episodes": 8, "duration": 21600.0,
               "obtained_at": "2026-11-26T23:10:00.000Z", "chemin": "/srv/medias"})
    }

    #[test]
    fn carte_bornee_et_fidele_a_la_cle() {
        let c = carte_de(&cle("season", Some(2)), &badge()).unwrap();
        assert_eq!(
            c,
            json!({"kind": "season", "title_id": ID, "season": 2, "title": "La Série", "season_title": "Saison 2",
                   "year": 2019, "poster": true, "season_poster": false, "episodes": 8, "duration": 21600.0,
                   "obtained_at": "2026-11-26T23:10:00.000Z"})
        );
        assert_eq!(carte_de(&cle("season", Some(3)), &badge()), None);
        assert_eq!(carte_de(&cle("series", None), &badge()), None);
        let mut b = badge();
        b["obtained_at"] = json!("hier");
        assert_eq!(carte_de(&cle("season", Some(2)), &b), None);
        let mut b = badge();
        b["title"] = json!("x".repeat(400));
        b["year"] = json!(12);
        b["duration"] = json!(-4);
        let c = carte_de(&cle("season", Some(2)), &b).unwrap();
        assert_eq!(c["title"].as_str().unwrap().chars().count(), 300);
        assert_eq!(
            (c["year"].clone(), c["duration"].clone()),
            (Value::Null, Value::Null)
        );
    }

    #[tokio::test]
    async fn badge_demande_a_entracte_avec_le_jeton_de_service() {
        let s = faux(|r| {
            if r.entete("authorization") != "Bearer jeton" {
                return (401, vec![]);
            }
            if r.chemin == format!("/internal/badges/season:{ID}:2") && r.query == "sub=alice" {
                return (200, badge().to_string().into_bytes());
            }
            (404, vec![])
        })
        .await;
        let c = cle("season", Some(2));
        assert_eq!(
            chercher(&format!("{}/", s.url), "jeton", "alice", &c)
                .await
                .unwrap()["title"],
            "La Série"
        );
        assert_eq!(chercher(&s.url, "jeton", "bob", &c).await, None);
        assert_eq!(chercher(&s.url, "autre", "alice", &c).await, None);
        let url = s.url.clone();
        drop(s);
        assert_eq!(chercher(&url, "jeton", "alice", &c).await, None);
    }
}
