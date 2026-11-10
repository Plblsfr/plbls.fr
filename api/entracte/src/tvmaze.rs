use serde_json::Value;

use crate::config::Config;

pub async fn suggestions(cfg: &Config, http: &reqwest::Client, q: &str) -> Vec<String> {
    if cfg.tvmaze_url.is_empty() {
        return Vec::new();
    }
    let rep = match http
        .get(format!("{}/search/shows", cfg.tvmaze_url))
        .query(&[("q", q)])
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::warn!("TVmaze : HTTP {}", r.status().as_u16());
            return Vec::new();
        }
        Err(e) => {
            tracing::warn!("TVmaze injoignable : {}", e.without_url());
            return Vec::new();
        }
    };
    match rep.json::<Value>().await {
        Ok(v) => noms(&v),
        Err(e) => {
            tracing::warn!("TVmaze : réponse illisible : {}", e.without_url());
            Vec::new()
        }
    }
}

fn noms(v: &Value) -> Vec<String> {
    let mut noms: Vec<String> = Vec::new();
    for nom in v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["show"]["name"].as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty() && n.chars().count() <= 100)
    {
        if !noms.iter().any(|n| n.eq_ignore_ascii_case(nom)) {
            noms.push(nom.to_string());
        }
        if noms.len() == 3 {
            break;
        }
    }
    noms
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn noms_distincts_et_limites() {
        let v = json!([
            {"score": 0.9, "show": {"name": "Daredevil"}},
            {"score": 0.8, "show": {"name": "daredevil"}},
            {"score": 0.7, "show": {"name": " "}},
            {"score": 0.6, "show": {"name": "Daredevil: Born Again"}},
            {"score": 0.5, "show": {}},
            {"score": 0.4, "show": {"name": "Dare"}},
            {"score": 0.3, "show": {"name": "Devil"}}
        ]);
        assert_eq!(noms(&v), ["Daredevil", "Daredevil: Born Again", "Dare"]);
        assert!(noms(&json!({"x": 1})).is_empty());
    }
}
