use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rule {
    pub mandatory: bool,
    #[serde(rename = "minVersion")]
    pub min_version: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Surcharge {
    mandatory: Option<bool>,
    #[serde(rename = "minVersion")]
    min_version: Option<String>,
    message: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Policy {
    #[serde(flatten)]
    regle: Rule,
    platforms: std::collections::HashMap<String, Surcharge>,
}

const EMBARQUEE: &str = include_str!("../updates.json");

pub fn charger() -> Policy {
    parse(EMBARQUEE.as_bytes())
}

pub fn parse(octets: &[u8]) -> Policy {
    serde_json::from_slice(octets).unwrap_or_default()
}

impl Policy {
    pub fn pour(&self, plateforme: &str) -> Rule {
        let plateforme = if plateforme == "pwa" {
            "web"
        } else {
            plateforme
        };
        let mut r = self.regle.clone();
        if let Some(o) = self.platforms.get(plateforme) {
            if let Some(m) = o.mandatory {
                r.mandatory = m;
            }
            if let Some(m) = &o.min_version {
                r.min_version = m.clone();
            }
            if let Some(m) = &o.message {
                r.message = m.clone();
            }
        }
        r
    }
}

fn versions(s: &str) -> Option<[i64; 3]> {
    let mut n = [0; 3];
    let morceaux: Vec<&str> = s.split('.').collect();
    if morceaux.len() > 3 {
        return None;
    }
    for (i, p) in morceaux.iter().enumerate() {
        n[i] = p.parse::<i64>().ok().filter(|v| *v >= 0)?;
    }
    Some(n)
}

pub fn en_dessous(v: &str, min: &str) -> bool {
    match (versions(v), versions(min)) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{charger, en_dessous, parse};

    #[test]
    fn en_dessous_compare_x_y_z() {
        for (v, min, attendu) in [
            ("0.4.9", "0.5.0", true),
            ("0.5.0", "0.5.0", false),
            ("0.6.0", "0.5.0", false),
            ("1.0.0", "0.9.9", false),
            ("0.9.9", "1.0.0", true),
            ("1.2", "1.2.1", true),
            ("1", "1.0.0", false),
            ("0.10.0", "0.9.0", false),
        ] {
            assert_eq!(en_dessous(v, min), attendu, "{v} < {min}");
        }
    }

    #[test]
    fn version_illisible_ne_bloque_personne() {
        for (v, min) in [
            ("", "1.0.0"),
            ("abc", "1.0.0"),
            ("1.0.0", ""),
            ("1.0.0", "x"),
            ("1.0.0.0", "2.0.0"),
            ("0.1.0", "1.0.0.0"),
            ("-1.0.0", "1.0.0"),
            ("1..0", "2.0.0"),
        ] {
            assert!(!en_dessous(v, min), "{v} < {min}");
        }
        assert!(en_dessous("+1.0.0", "2.0.0"));
    }

    #[test]
    fn pour_applique_les_surcharges() {
        let p = parse(
            br#"{"mandatory":true,"minVersion":"0.5.0","message":"commun","platforms":{"android":{"minVersion":"1.0.0","message":"faille corrigee"},"web":{"mandatory":false}}}"#,
        );
        let commun = p.pour("windows");
        assert!(commun.mandatory);
        assert_eq!(commun.min_version, "0.5.0");
        assert_eq!(commun.message, "commun");
        let android = p.pour("android");
        assert!(android.mandatory);
        assert_eq!(android.min_version, "1.0.0");
        assert_eq!(android.message, "faille corrigee");
        assert!(!p.pour("web").mandatory);
        assert_eq!(p.pour("pwa"), p.pour("web"));
        assert_eq!(p.pour("inconnue"), commun);
        assert_eq!(p.pour(""), commun);
    }

    #[test]
    fn json_illisible_donne_une_politique_vide() {
        for brut in [&b"pas du json"[..], b"", b"[]", b"{\"mandatory\":\"oui\"}"] {
            let r = parse(brut).pour("web");
            assert!(!r.mandatory);
            assert_eq!(r.min_version, "");
            assert_eq!(r.message, "");
        }
    }

    #[test]
    fn la_politique_embarquee_ne_force_rien() {
        let r = charger().pour("android");
        assert!(!r.mandatory);
        assert_eq!(r.min_version, "");
    }
}
