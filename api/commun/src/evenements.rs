use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ID_MAX: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Entracte,
    Sillon,
    Estompe,
}

impl Source {
    pub fn nom(self) -> &'static str {
        match self {
            Source::Entracte => "entracte",
            Source::Sillon => "sillon",
            Source::Estompe => "estompe",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Genre {
    Film,
    Episode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sens {
    Donnee,
    Recue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "donnees")]
pub enum Donnees {
    #[serde(rename = "entracte.lecture")]
    EntracteLecture {
        titre: String,
        genre: Genre,
        serie: Option<String>,
        secondes: u32,
    },
    #[serde(rename = "entracte.vu")]
    EntracteVu {
        titre: String,
        genre: Genre,
        serie: Option<String>,
    },
    #[serde(rename = "entracte.serie_terminee")]
    EntracteSerieTerminee { serie: String },
    #[serde(rename = "entracte.abandon")]
    EntracteAbandon { titre: String, genre: Genre },
    #[serde(rename = "entracte.intro_passee")]
    EntracteIntroPassee { titre: String },
    #[serde(rename = "sillon.ecoute")]
    SillonEcoute {
        piste: String,
        secondes: u32,
        comptee: bool,
    },
    #[serde(rename = "estompe.message")]
    EstompeMessage {
        serveur: Option<String>,
        salon: Option<String>,
        prive: bool,
    },
    #[serde(rename = "estompe.fichier")]
    EstompeFichier {
        serveur: Option<String>,
        salon: Option<String>,
        prive: bool,
        nombre: u32,
    },
    #[serde(rename = "estompe.reaction")]
    EstompeReaction { sens: Sens, emoji: String },
    #[serde(rename = "estompe.vocal.temps")]
    EstompeVocalTemps {
        serveur: Option<String>,
        salon: String,
        secondes: u32,
    },
    #[serde(rename = "estompe.vocal.appel")]
    EstompeVocalAppel {
        serveur: Option<String>,
        salon: String,
        secondes: u32,
    },
    #[serde(rename = "estompe.partage_ecran")]
    EstompePartageEcran { secondes: u32 },
    #[serde(rename = "estompe.cinema.temps")]
    EstompeCinemaTemps { secondes: u32 },
    #[serde(rename = "estompe.cinema.seance")]
    EstompeCinemaSeance {},
}

impl Donnees {
    pub fn nom(&self) -> &'static str {
        match self {
            Donnees::EntracteLecture { .. } => "entracte.lecture",
            Donnees::EntracteVu { .. } => "entracte.vu",
            Donnees::EntracteSerieTerminee { .. } => "entracte.serie_terminee",
            Donnees::EntracteAbandon { .. } => "entracte.abandon",
            Donnees::EntracteIntroPassee { .. } => "entracte.intro_passee",
            Donnees::SillonEcoute { .. } => "sillon.ecoute",
            Donnees::EstompeMessage { .. } => "estompe.message",
            Donnees::EstompeFichier { .. } => "estompe.fichier",
            Donnees::EstompeReaction { .. } => "estompe.reaction",
            Donnees::EstompeVocalTemps { .. } => "estompe.vocal.temps",
            Donnees::EstompeVocalAppel { .. } => "estompe.vocal.appel",
            Donnees::EstompePartageEcran { .. } => "estompe.partage_ecran",
            Donnees::EstompeCinemaTemps { .. } => "estompe.cinema.temps",
            Donnees::EstompeCinemaSeance {} => "estompe.cinema.seance",
        }
    }

    pub fn source(&self) -> Source {
        match self {
            Donnees::EntracteLecture { .. }
            | Donnees::EntracteVu { .. }
            | Donnees::EntracteSerieTerminee { .. }
            | Donnees::EntracteAbandon { .. }
            | Donnees::EntracteIntroPassee { .. } => Source::Entracte,
            Donnees::SillonEcoute { .. } => Source::Sillon,
            Donnees::EstompeMessage { .. }
            | Donnees::EstompeFichier { .. }
            | Donnees::EstompeReaction { .. }
            | Donnees::EstompeVocalTemps { .. }
            | Donnees::EstompeVocalAppel { .. }
            | Donnees::EstompePartageEcran { .. }
            | Donnees::EstompeCinemaTemps { .. }
            | Donnees::EstompeCinemaSeance {} => Source::Estompe,
        }
    }

    pub fn contenu(&self) -> Value {
        serde_json::to_value(self)
            .ok()
            .and_then(|mut v| v.get_mut("donnees").map(Value::take))
            .unwrap_or(Value::Null)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evenement {
    pub id: String,
    pub sub: String,
    pub source: Source,
    pub date: DateTime<Utc>,
    #[serde(default)]
    pub importe: bool,
    #[serde(flatten)]
    pub donnees: Donnees,
}

impl Evenement {
    pub fn nouveau(sub: impl Into<String>, donnees: Donnees) -> Self {
        Evenement {
            id: uuid::Uuid::new_v4().to_string(),
            sub: sub.into(),
            source: donnees.source(),
            date: Utc::now(),
            importe: false,
            donnees,
        }
    }

    pub fn importe(
        id: impl Into<String>,
        sub: impl Into<String>,
        date: DateTime<Utc>,
        donnees: Donnees,
    ) -> Self {
        Evenement {
            id: id.into(),
            sub: sub.into(),
            source: donnees.source(),
            date,
            importe: true,
            donnees,
        }
    }

    pub fn verifier(&self) -> Result<(), String> {
        let longueur = self.id.chars().count();
        if longueur == 0 || longueur > ID_MAX {
            return Err(format!("id de {longueur} caractères (1 à {ID_MAX})"));
        }
        if self.sub.trim().is_empty() {
            return Err("sub vide".into());
        }
        if self.source != self.donnees.source() {
            return Err(format!(
                "source {} incohérente avec le type {}",
                self.source.nom(),
                self.donnees.nom()
            ));
        }
        Ok(())
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn lire(texte: &str) -> Result<Self, String> {
        let e: Evenement = serde_json::from_str(texte).map_err(|e| e.to_string())?;
        e.verifier()?;
        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn date() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 29, 1, 30, 0).unwrap()
    }

    fn fige(e: &Evenement, attendu: &str) {
        assert_eq!(e.json(), attendu);
        assert_eq!(&Evenement::lire(attendu).unwrap(), e);
    }

    #[test]
    fn json_fige_d_un_type_par_source() {
        let mut e = Evenement::importe(
            "import:entracte:lecture:42",
            "sub-alice",
            date(),
            Donnees::EntracteLecture {
                titre: "t1".into(),
                genre: Genre::Episode,
                serie: Some("s1".into()),
                secondes: 600,
            },
        );
        fige(
            &e,
            r#"{"id":"import:entracte:lecture:42","sub":"sub-alice","source":"entracte","date":"2026-03-29T01:30:00Z","importe":true,"type":"entracte.lecture","donnees":{"titre":"t1","genre":"episode","serie":"s1","secondes":600}}"#,
        );
        e.id = "e1".into();
        e.importe = false;
        e.donnees = Donnees::SillonEcoute {
            piste: "p1".into(),
            secondes: 187,
            comptee: true,
        };
        e.source = Source::Sillon;
        fige(
            &e,
            r#"{"id":"e1","sub":"sub-alice","source":"sillon","date":"2026-03-29T01:30:00Z","importe":false,"type":"sillon.ecoute","donnees":{"piste":"p1","secondes":187,"comptee":true}}"#,
        );
        e.donnees = Donnees::EstompeMessage {
            serveur: None,
            salon: Some("c1".into()),
            prive: false,
        };
        e.source = Source::Estompe;
        fige(
            &e,
            r#"{"id":"e1","sub":"sub-alice","source":"estompe","date":"2026-03-29T01:30:00Z","importe":false,"type":"estompe.message","donnees":{"serveur":null,"salon":"c1","prive":false}}"#,
        );
        e.donnees = Donnees::EstompeCinemaSeance {};
        fige(
            &e,
            r#"{"id":"e1","sub":"sub-alice","source":"estompe","date":"2026-03-29T01:30:00Z","importe":false,"type":"estompe.cinema.seance","donnees":{}}"#,
        );
    }

    fn toutes() -> Vec<Donnees> {
        let t = || "t".to_string();
        vec![
            Donnees::EntracteLecture {
                titre: t(),
                genre: Genre::Film,
                serie: None,
                secondes: 1,
            },
            Donnees::EntracteVu {
                titre: t(),
                genre: Genre::Episode,
                serie: Some(t()),
            },
            Donnees::EntracteSerieTerminee { serie: t() },
            Donnees::EntracteAbandon {
                titre: t(),
                genre: Genre::Film,
            },
            Donnees::EntracteIntroPassee { titre: t() },
            Donnees::SillonEcoute {
                piste: t(),
                secondes: 2,
                comptee: false,
            },
            Donnees::EstompeMessage {
                serveur: Some(t()),
                salon: Some(t()),
                prive: false,
            },
            Donnees::EstompeFichier {
                serveur: None,
                salon: None,
                prive: true,
                nombre: 3,
            },
            Donnees::EstompeReaction {
                sens: Sens::Recue,
                emoji: "👍".into(),
            },
            Donnees::EstompeVocalTemps {
                serveur: None,
                salon: t(),
                secondes: 4,
            },
            Donnees::EstompeVocalAppel {
                serveur: Some(t()),
                salon: t(),
                secondes: 5,
            },
            Donnees::EstompePartageEcran { secondes: 6 },
            Donnees::EstompeCinemaTemps { secondes: 7 },
            Donnees::EstompeCinemaSeance {},
        ]
    }

    #[test]
    fn aller_retour_de_tous_les_types() {
        let noms: Vec<&str> = toutes().iter().map(Donnees::nom).collect();
        assert_eq!(
            noms,
            [
                "entracte.lecture",
                "entracte.vu",
                "entracte.serie_terminee",
                "entracte.abandon",
                "entracte.intro_passee",
                "sillon.ecoute",
                "estompe.message",
                "estompe.fichier",
                "estompe.reaction",
                "estompe.vocal.temps",
                "estompe.vocal.appel",
                "estompe.partage_ecran",
                "estompe.cinema.temps",
                "estompe.cinema.seance",
            ]
        );
        for d in toutes() {
            let e = Evenement::nouveau("sub-alice", d.clone());
            assert!(!e.importe && e.id.len() == 36);
            assert_eq!(e.source, d.source());
            assert!(d.nom().starts_with(e.source.nom()), "{}", d.nom());
            let v: Value = serde_json::from_str(&e.json()).unwrap();
            assert_eq!(v["type"], d.nom());
            assert_eq!(v["source"], e.source.nom());
            assert_eq!(v["donnees"], d.contenu());
            assert_eq!(Evenement::lire(&e.json()).unwrap(), e);
        }
    }

    #[test]
    fn valeurs_fermees_en_minuscules() {
        let d = Donnees::EstompeReaction {
            sens: Sens::Donnee,
            emoji: "🎉".into(),
        };
        assert_eq!(d.contenu(), json!({"sens": "donnee", "emoji": "🎉"}));
        let d = Donnees::EntracteAbandon {
            titre: "t".into(),
            genre: Genre::Film,
        };
        assert_eq!(d.contenu(), json!({"titre": "t", "genre": "film"}));
    }

    #[test]
    fn importe_absent_vaut_faux_et_champ_facultatif_absent_accepte() {
        let e = Evenement::lire(
            r#"{"id":"e1","sub":"s","source":"estompe","date":"2026-03-29T01:30:00+02:00","type":"estompe.message","donnees":{"prive":true}}"#,
        )
        .unwrap();
        assert!(!e.importe);
        assert_eq!(
            e.date,
            Utc.with_ymd_and_hms(2026, 3, 28, 23, 30, 0).unwrap()
        );
        assert_eq!(
            e.donnees,
            Donnees::EstompeMessage {
                serveur: None,
                salon: None,
                prive: true
            }
        );
    }

    #[test]
    fn evenements_refuses() {
        let base = json!({"id": "e1", "sub": "s", "source": "sillon", "date": "2026-03-29T01:30:00Z",
            "type": "sillon.ecoute", "donnees": {"piste": "p", "secondes": 1, "comptee": true}});
        assert!(Evenement::lire(&base.to_string()).is_ok());
        let avec = |cle: &str, v: Value| {
            let mut b = base.clone();
            b[cle] = v;
            Evenement::lire(&b.to_string())
        };
        assert!(avec("type", json!("sillon.inconnu")).is_err());
        assert!(avec("source", json!("entracte")).unwrap_err().contains("incohérente"));
        assert!(avec("source", json!("autre")).is_err());
        assert!(avec("id", json!("")).is_err());
        assert!(avec("id", json!("x".repeat(ID_MAX))).is_ok());
        assert!(avec("id", json!("x".repeat(ID_MAX + 1))).is_err());
        assert!(avec("sub", json!(" ")).is_err());
        assert!(avec("date", json!("hier")).is_err());
        assert!(avec("donnees", json!({"piste": "p", "secondes": -1, "comptee": true})).is_err());
        assert!(avec("donnees", json!({"piste": "p"})).is_err());
        assert!(Evenement::lire("pas du json").is_err());
        assert!(Evenement::lire("[]").is_err());
    }
}
