use estompe_client::mesures::{
    construire_export, duree_scenario, maintenant_ms, ExportMesures, Marqueur, Releveur, Systeme,
    INTERVALLE_MS,
};
use estompe_plateforme::MesureProcessus;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub const OPTION_MESURES: &str = "--mesures";
pub const OPTION_SCENARIO: &str = "--scenario";
pub const OPTION_ATTENTE: &str = "--attente";
pub const OPTION_DUREE: &str = "--duree";
pub const SCENARIO_PAR_DEFAUT: &str = "jeu";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsMesures {
    pub fichier: PathBuf,
    pub scenario: String,
    pub attente: Duration,
    pub duree: Duration,
}

impl OptionsMesures {
    pub fn fin(&self) -> Duration {
        self.attente + self.duree
    }
}

fn valeur(arguments: &[String], option: &str) -> Result<Option<String>, String> {
    let prefixe = format!("{option}=");
    for (i, argument) in arguments.iter().enumerate() {
        if argument == option {
            return arguments
                .get(i + 1)
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("{option} attend une valeur"));
        }
        if let Some(v) = argument.strip_prefix(&prefixe) {
            return Ok(Some(v.to_string()));
        }
    }
    Ok(None)
}

fn secondes(arguments: &[String], option: &str) -> Result<Option<Duration>, String> {
    valeur(arguments, option)?
        .map(|v| {
            v.parse::<u64>()
                .map(Duration::from_secs)
                .map_err(|_| format!("{option} attend un nombre de secondes : « {v} »"))
        })
        .transpose()
}

pub fn options_des_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<Option<OptionsMesures>, String> {
    let arguments: Vec<String> = arguments.into_iter().collect();
    let Some(fichier) = valeur(&arguments, OPTION_MESURES)? else {
        return Ok(None);
    };
    let scenario =
        valeur(&arguments, OPTION_SCENARIO)?.unwrap_or_else(|| SCENARIO_PAR_DEFAUT.into());
    let duree = match secondes(&arguments, OPTION_DUREE)? {
        Some(d) => d,
        None => duree_scenario(&scenario)
            .map(Duration::from_millis)
            .ok_or_else(|| format!("scénario « {scenario} » sans durée connue : {OPTION_DUREE}"))?,
    };
    Ok(Some(OptionsMesures {
        fichier: PathBuf::from(fichier),
        scenario,
        attente: secondes(&arguments, OPTION_ATTENTE)?.unwrap_or_default(),
        duree,
    }))
}

pub fn marqueur(options: &OptionsMesures, debut: u64, maintenant: u64) -> Option<Marqueur> {
    let debut_scenario = debut + options.attente.as_millis() as u64;
    let fin_prevue = debut_scenario + options.duree.as_millis() as u64;
    (maintenant > debut_scenario).then(|| Marqueur {
        nom: options.scenario.clone(),
        debut: debut_scenario,
        fin: maintenant.min(fin_prevue),
    })
}

pub struct MesureEnCours {
    options: OptionsMesures,
    source: Arc<dyn MesureProcessus>,
    releveur: Releveur,
}

impl MesureEnCours {
    pub fn demarrer(
        options: OptionsMesures,
        source: Arc<dyn MesureProcessus>,
    ) -> Result<Self, String> {
        if !source.disponible() {
            return Err("mesure du processus indisponible sur cet OS".into());
        }
        let releveur = Releveur::demarrer(source.clone(), INTERVALLE_MS)
            .map_err(|e| format!("fil de mesure : {e}"))?;
        Ok(Self {
            options,
            source,
            releveur,
        })
    }

    pub fn options(&self) -> &OptionsMesures {
        &self.options
    }

    pub fn terminer(self, facteur_echelle: Option<f64>) -> ExportMesures {
        let releve = self.releveur.arreter();
        let marqueurs = marqueur(&self.options, releve.debut, releve.fin)
            .into_iter()
            .collect();
        construire_export(
            releve,
            &format!("natif {}", crate::VERSION),
            Systeme::de_la_machine(self.source.machine(), facteur_echelle),
            marqueurs,
            maintenant_ms(),
        )
    }

    pub fn ecrire(self, facteur_echelle: Option<f64>) -> Result<PathBuf, String> {
        let fichier = self.options.fichier.clone();
        let export = self.terminer(facteur_echelle);
        if let Some(dossier) = fichier.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dossier).map_err(|e| format!("{}: {e}", dossier.display()))?;
        }
        let texte = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
        std::fs::write(&fichier, format!("{texte}\n"))
            .map_err(|e| format!("{}: {e}", fichier.display()))?;
        Ok(fichier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_plateforme::{CompteursProcessus, InfosMachine, Resultat};

    fn args(liste: &[&str]) -> Vec<String> {
        liste.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sans_option_aucune_mesure() {
        assert_eq!(
            options_des_arguments(args(&["--essai-de-lancement"])),
            Ok(None)
        );
    }

    #[test]
    fn lit_le_fichier_le_scenario_l_attente_et_la_duree() {
        assert_eq!(
            options_des_arguments(args(&["--mesures", "m/s1.json"])),
            Ok(Some(OptionsMesures {
                fichier: PathBuf::from("m/s1.json"),
                scenario: "jeu".into(),
                attente: Duration::ZERO,
                duree: Duration::from_secs(300),
            }))
        );
        let o = options_des_arguments(args(&[
            "--mesures=a.json",
            "--scenario",
            "vocal",
            "--attente=60",
            "--duree",
            "120",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(o.scenario, "vocal");
        assert_eq!(o.fin(), Duration::from_secs(180));
        assert_eq!(
            options_des_arguments(args(&["--mesures", "a.json", "--scenario", "repos"]))
                .unwrap()
                .unwrap()
                .duree,
            Duration::from_secs(300)
        );
    }

    #[test]
    fn refuse_une_option_incomplete_ou_un_scenario_sans_duree() {
        assert!(options_des_arguments(args(&["--mesures"])).is_err());
        assert!(options_des_arguments(args(&["--mesures", "--scenario", "jeu"])).is_err());
        assert!(options_des_arguments(args(&["--mesures", "a.json", "--duree", "cinq"])).is_err());
        assert!(options_des_arguments(args(&["--mesures", "a.json", "--scenario", "x"])).is_err());
    }

    #[test]
    fn le_marqueur_commence_apres_l_attente_et_s_arrete_a_la_fermeture() {
        let o = OptionsMesures {
            fichier: PathBuf::from("a.json"),
            scenario: "jeu".into(),
            attente: Duration::from_secs(10),
            duree: Duration::from_secs(300),
        };
        assert_eq!(marqueur(&o, 1_000, 5_000), None);
        let m = marqueur(&o, 1_000, 400_000).unwrap();
        assert_eq!((m.debut, m.fin), (11_000, 311_000));
        assert_eq!(marqueur(&o, 1_000, 60_000).unwrap().fin, 60_000);
    }

    struct Source;

    impl MesureProcessus for Source {
        fn disponible(&self) -> bool {
            true
        }
        fn compteurs(&self) -> Resultat<CompteursProcessus> {
            Ok(CompteursProcessus {
                prive: 50 << 20,
                ws: 80 << 20,
                pic_ws: 90 << 20,
                cpu_ms: 7,
            })
        }
        fn machine(&self) -> InfosMachine {
            InfosMachine {
                os: Some("Windows 11".into()),
                coeurs: 8,
                ..InfosMachine::default()
            }
        }
    }

    #[test]
    fn ecrit_un_export_au_format_de_l_app_tauri() {
        let dossier = tempfile::tempdir().unwrap();
        let options = OptionsMesures {
            fichier: dossier.path().join("sous/dossier/s1.json"),
            scenario: "jeu".into(),
            attente: Duration::ZERO,
            duree: Duration::from_secs(300),
        };
        let mesure = MesureEnCours::demarrer(options, Arc::new(Source)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let fichier = mesure.ecrire(Some(1.5)).unwrap();
        let export: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fichier).unwrap()).unwrap();
        assert_eq!(export["format"], "estompe-mesures/1");
        assert_eq!(export["app"], format!("natif {}", crate::VERSION));
        assert_eq!(export["systeme"]["windows"], "Windows 11");
        assert_eq!(export["systeme"]["facteur_echelle"], 1.5);
        assert_eq!(export["marqueurs"][0]["nom"], "jeu");
        assert_eq!(
            export["statistiques"]["session"]["total"]["prive"]["mediane_mo"],
            50.0
        );
        assert!(export["statistiques"]["scenarios"]["jeu"].is_object());
    }
}
