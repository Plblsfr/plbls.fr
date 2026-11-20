use serde::{Deserialize, Serialize};

fn composantes(version: &str) -> Vec<i64> {
    version
        .split('.')
        .map(|morceau| morceau.trim().parse::<i64>().unwrap_or(0))
        .collect()
}

pub fn est_plus_recente(a: &str, b: &str) -> bool {
    let (x, y) = (composantes(a), composantes(b));
    for i in 0..3 {
        let (p, q) = (
            x.get(i).copied().unwrap_or(0),
            y.get(i).copied().unwrap_or(0),
        );
        if p != q {
            return p > q;
        }
    }
    false
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Publication {
    pub version: String,
    pub notes: Option<String>,
    pub mandatory: Option<bool>,
    pub force: Option<bool>,
    #[serde(rename = "minVersion")]
    pub version_minimale: Option<String>,
    pub message: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MiseAJourDisponible {
    pub version: String,
    pub notes: String,
    pub message: String,
    pub imposee: bool,
    pub sha256: String,
}

pub fn sous_la_version_minimale(installee: &str, minimale: &str) -> bool {
    !minimale.is_empty() && est_plus_recente(minimale, installee)
}

pub fn disponibilite(installee: &str, publication: &Publication) -> Option<MiseAJourDisponible> {
    if installee.is_empty()
        || publication.version.is_empty()
        || !est_plus_recente(&publication.version, installee)
    {
        return None;
    }
    let imposee = publication.mandatory == Some(true)
        || publication.force == Some(true)
        || sous_la_version_minimale(
            installee,
            publication.version_minimale.as_deref().unwrap_or_default(),
        );
    Some(MiseAJourDisponible {
        version: publication.version.clone(),
        notes: publication.notes.clone().unwrap_or_default(),
        message: publication.message.clone().unwrap_or_default(),
        imposee,
        sha256: publication.sha256.clone().unwrap_or_default(),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RefusVersion {
    pub error: String,
    pub message: String,
    pub min_version: String,
}

pub fn refus_mise_a_jour(statut: u16, corps: &str) -> Option<String> {
    if statut != 426 {
        return None;
    }
    let refus = serde_json::from_str::<RefusVersion>(corps).unwrap_or_default();
    Some(refus.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publication(version: &str) -> Publication {
        Publication {
            version: version.into(),
            ..Publication::default()
        }
    }

    #[test]
    fn compare_les_versions_numeriquement() {
        assert!(est_plus_recente("0.10.0", "0.9.9"));
        assert!(est_plus_recente("1.0.0", "0.99.99"));
        assert!(!est_plus_recente("0.3.1", "0.3.1"));
        assert!(!est_plus_recente("0.3.0", "0.3.1"));
    }

    #[test]
    fn ne_propose_rien_si_a_jour_ou_version_inconnue() {
        let a_jour = Publication {
            mandatory: Some(true),
            ..publication("1.2.0")
        };
        assert_eq!(disponibilite("1.2.0", &a_jour), None);
        let minimale = Publication {
            version_minimale: Some("1.3.0".into()),
            ..publication("1.2.0")
        };
        assert_eq!(disponibilite("1.3.0", &minimale), None);
        assert_eq!(disponibilite("", &publication("1.2.0")), None);
    }

    #[test]
    fn propose_une_mise_a_jour_reportable_par_defaut() {
        let p = Publication {
            notes: Some("n".into()),
            ..publication("1.2.0")
        };
        let dispo = disponibilite("1.1.0", &p).unwrap();
        assert_eq!(dispo.version, "1.2.0");
        assert!(!dispo.imposee);
        assert_eq!(dispo.notes, "n");
    }

    #[test]
    fn impose_si_obligatoire_y_compris_ancien_champ_force() {
        let p = Publication {
            mandatory: Some(true),
            message: Some("faille".into()),
            ..publication("1.2.0")
        };
        let dispo = disponibilite("1.1.0", &p).unwrap();
        assert!(dispo.imposee);
        assert_eq!(dispo.message, "faille");
        let ancien = Publication {
            force: Some(true),
            ..publication("1.2.0")
        };
        assert!(disponibilite("1.1.0", &ancien).unwrap().imposee);
    }

    #[test]
    fn impose_sous_la_version_minimale() {
        let p = Publication {
            version_minimale: Some("1.0.0".into()),
            ..publication("1.2.0")
        };
        assert!(disponibilite("0.5.0", &p).unwrap().imposee);
        assert!(!disponibilite("1.0.0", &p).unwrap().imposee);
    }

    #[test]
    fn lit_le_json_publie() {
        let p: Publication = serde_json::from_str(
            r#"{"version":"999.0.0","mandatory":true,"message":"urgent","minVersion":"998.0.0"}"#,
        )
        .unwrap();
        let dispo = disponibilite("0.1.0", &p).unwrap();
        assert!(dispo.imposee);
        assert_eq!(dispo.message, "urgent");
        assert_eq!(p.version_minimale.as_deref(), Some("998.0.0"));
    }

    #[test]
    fn traduit_le_refus_426_de_l_api() {
        let corps = r#"{"error":"upgrade_required","message":"version trop ancienne","min_version":"1.0.0"}"#;
        assert_eq!(
            refus_mise_a_jour(426, corps).as_deref(),
            Some("version trop ancienne")
        );
        assert_eq!(refus_mise_a_jour(200, corps), None);
        assert_eq!(refus_mise_a_jour(426, "pas du json").as_deref(), Some(""));
    }

    #[test]
    fn sous_la_version_minimale_ignore_une_minimale_vide() {
        assert!(sous_la_version_minimale("0.4.9", "0.5.0"));
        assert!(!sous_la_version_minimale("0.5.0", "0.5.0"));
        assert!(!sous_la_version_minimale("0.1.0", ""));
    }
}
