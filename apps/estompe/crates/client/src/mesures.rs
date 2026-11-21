use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use estompe_plateforme::{InfosMachine, MesureProcessus as SourceMesure};
use serde::{Deserialize, Serialize};

pub const FORMAT_MESURES: &str = "estompe-mesures/1";
pub const INTERVALLE_MS: u64 = 1000;
pub const INTERVALLE_MIN_MS: u64 = 200;
pub const GENRE_HOTE: &str = "hote";
const MO: f64 = 1024.0 * 1024.0;

pub const SCENARIOS: &[(&str, u64)] = &[
    ("repos", 300_000),
    ("gros-salon", 30_000),
    ("defilement", 10_000),
    ("flux", 60_000),
    ("jeu", 300_000),
    ("reduite", 120_000),
];

pub fn duree_scenario(id: &str) -> Option<u64> {
    SCENARIOS
        .iter()
        .find(|(nom, _)| *nom == id)
        .map(|(_, d)| *d)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MesureProcessus {
    pub pid: u32,
    #[serde(rename = "type")]
    pub genre: String,
    pub role: String,
    pub prive: u64,
    pub ws: u64,
    pub pic_ws: u64,
    pub cpu_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Echantillon {
    pub t: u64,
    pub processus: Vec<MesureProcessus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Releve {
    pub debut: u64,
    pub fin: u64,
    pub intervalle_ms: u64,
    pub echantillons: Vec<Echantillon>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marqueur {
    pub nom: String,
    pub debut: u64,
    pub fin: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatsMemoire {
    pub mediane_mo: f64,
    pub pic_mo: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatsGroupe {
    pub prive: StatsMemoire,
    pub ws: StatsMemoire,
    pub cpu_pct: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct StatsImages {
    pub n: usize,
    pub moyenne_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub perdues: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct StatsLatences {
    pub n: usize,
    pub mediane_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Latences {
    pub messages: StatsLatences,
    pub saisie: StatsLatences,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsFenetre {
    pub duree_s: f64,
    pub echantillons: usize,
    pub total: StatsGroupe,
    pub par_role: BTreeMap<String, StatsGroupe>,
    pub par_type: BTreeMap<String, StatsGroupe>,
    pub images: StatsImages,
    pub latences: Latences,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Systeme {
    pub webview2: Option<String>,
    pub windows: Option<String>,
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub coeurs: usize,
    pub ram_mo: Option<u64>,
    pub gpu: Vec<String>,
    pub facteur_echelle: Option<f64>,
    pub options_webview2: Vec<String>,
    pub demarrage_processus: Option<u64>,
}

impl Systeme {
    pub fn de_la_machine(machine: InfosMachine, facteur_echelle: Option<f64>) -> Self {
        Self {
            webview2: None,
            windows: machine.os.clone().filter(|os| os.starts_with("Windows")),
            os: machine.os,
            cpu: machine.cpu,
            coeurs: machine.coeurs,
            ram_mo: machine.ram_mo,
            gpu: machine.gpu,
            facteur_echelle,
            options_webview2: Vec::new(),
            demarrage_processus: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sonde {
    pub images: Vec<(u64, f64)>,
    pub messages: Vec<(u64, f64)>,
    pub saisie: Vec<(u64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statistiques {
    pub session: StatsFenetre,
    pub scenarios: BTreeMap<String, StatsFenetre>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportMesures {
    pub format: String,
    pub cree: String,
    pub app: String,
    pub systeme: Systeme,
    pub demarrage_ms: Option<u64>,
    pub debut: u64,
    pub fin: u64,
    pub intervalle_ms: u64,
    pub marqueurs: Vec<Marqueur>,
    pub echantillons: Vec<Echantillon>,
    pub sonde: Sonde,
    pub statistiques: Statistiques,
}

fn arrondi(x: f64, decimales: i32) -> f64 {
    let facteur = 10f64.powi(decimales);
    (x * facteur).round() / facteur
}

pub fn quantile(valeurs: &[f64], q: f64) -> f64 {
    if valeurs.is_empty() {
        return 0.0;
    }
    let mut v = valeurs.to_vec();
    v.sort_by(f64::total_cmp);
    let i = (v.len() - 1) as f64 * q;
    let bas = i.floor() as usize;
    let haut = (bas + 1).min(v.len() - 1);
    v[bas] + (v[haut] - v[bas]) * (i - bas as f64)
}

fn stats_memoire(valeurs: &[f64]) -> StatsMemoire {
    StatsMemoire {
        mediane_mo: arrondi(quantile(valeurs, 0.5), 1),
        pic_mo: arrondi(valeurs.iter().copied().fold(0.0, f64::max), 1),
    }
}

fn stats_groupe(
    echantillons: &[&Echantillon],
    garder: impl Fn(&MesureProcessus) -> bool,
) -> StatsGroupe {
    let somme = |e: &Echantillon, f: fn(&MesureProcessus) -> u64| {
        e.processus.iter().filter(|p| garder(p)).map(f).sum::<u64>() as f64 / MO
    };
    let prive: Vec<f64> = echantillons.iter().map(|e| somme(e, |p| p.prive)).collect();
    let ws: Vec<f64> = echantillons.iter().map(|e| somme(e, |p| p.ws)).collect();
    let mut bornes: HashMap<u32, (u64, u64)> = HashMap::new();
    for e in echantillons {
        for p in e.processus.iter().filter(|p| garder(p)) {
            bornes
                .entry(p.pid)
                .and_modify(|b| b.1 = p.cpu_ms)
                .or_insert((p.cpu_ms, p.cpu_ms));
        }
    }
    let duree = match (echantillons.first(), echantillons.last()) {
        (Some(premier), Some(dernier)) if echantillons.len() > 1 => {
            dernier.t.saturating_sub(premier.t)
        }
        _ => 0,
    };
    let cpu: u64 = bornes.values().map(|(c0, c1)| c1.saturating_sub(*c0)).sum();
    StatsGroupe {
        prive: stats_memoire(&prive),
        ws: stats_memoire(&ws),
        cpu_pct: if duree > 0 {
            arrondi(cpu as f64 / duree as f64 * 100.0, 1)
        } else {
            0.0
        },
    }
}

pub fn stats_fenetre(echantillons: &[Echantillon], debut: u64, fin: u64) -> StatsFenetre {
    let e: Vec<&Echantillon> = echantillons
        .iter()
        .filter(|x| x.t >= debut && x.t <= fin)
        .collect();
    let roles: BTreeSet<&str> = e
        .iter()
        .flat_map(|x| x.processus.iter().map(|p| p.role.as_str()))
        .collect();
    let genres: BTreeSet<&str> = e
        .iter()
        .flat_map(|x| x.processus.iter().map(|p| p.genre.as_str()))
        .collect();
    StatsFenetre {
        duree_s: arrondi(fin.saturating_sub(debut) as f64 / 1000.0, 1),
        echantillons: e.len(),
        total: stats_groupe(&e, |_| true),
        par_role: roles
            .iter()
            .map(|r| (r.to_string(), stats_groupe(&e, |p| p.role == *r)))
            .collect(),
        par_type: genres
            .iter()
            .map(|g| (g.to_string(), stats_groupe(&e, |p| p.genre == *g)))
            .collect(),
        images: StatsImages::default(),
        latences: Latences::default(),
    }
}

pub fn construire_export(
    releve: Releve,
    app: &str,
    systeme: Systeme,
    marqueurs: Vec<Marqueur>,
    maintenant: u64,
) -> ExportMesures {
    let session = stats_fenetre(&releve.echantillons, releve.debut, releve.fin);
    let scenarios = marqueurs
        .iter()
        .map(|m| {
            (
                m.nom.clone(),
                stats_fenetre(&releve.echantillons, m.debut, m.fin),
            )
        })
        .collect();
    ExportMesures {
        format: FORMAT_MESURES.into(),
        cree: crate::format::iso_de_ms(maintenant as i64),
        app: app.into(),
        systeme,
        demarrage_ms: None,
        debut: releve.debut,
        fin: releve.fin,
        intervalle_ms: releve.intervalle_ms,
        marqueurs,
        echantillons: releve.echantillons,
        sonde: Sonde::default(),
        statistiques: Statistiques { session, scenarios },
    }
}

pub fn maintenant_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn echantillonner(source: &dyn SourceMesure) -> Option<Echantillon> {
    let c = source.compteurs().ok()?;
    Some(Echantillon {
        t: maintenant_ms(),
        processus: vec![MesureProcessus {
            pid: std::process::id(),
            genre: GENRE_HOTE.into(),
            role: GENRE_HOTE.into(),
            prive: c.prive,
            ws: c.ws,
            pic_ws: c.pic_ws,
            cpu_ms: c.cpu_ms,
        }],
    })
}

pub struct Releveur {
    actif: Arc<AtomicBool>,
    debut: u64,
    intervalle: u64,
    echantillons: Arc<Mutex<Vec<Echantillon>>>,
    fil: JoinHandle<()>,
}

impl Releveur {
    pub fn demarrer(source: Arc<dyn SourceMesure>, intervalle_ms: u64) -> std::io::Result<Self> {
        let intervalle = intervalle_ms.max(INTERVALLE_MIN_MS);
        let actif = Arc::new(AtomicBool::new(true));
        let echantillons: Arc<Mutex<Vec<Echantillon>>> = Arc::default();
        let debut = maintenant_ms();
        let fil = {
            let (actif, echantillons) = (actif.clone(), echantillons.clone());
            std::thread::Builder::new()
                .name("mesures".into())
                .spawn(move || {
                    while actif.load(Ordering::Acquire) {
                        if let Some(e) = echantillonner(source.as_ref()) {
                            echantillons
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .push(e);
                        }
                        std::thread::park_timeout(Duration::from_millis(intervalle));
                    }
                })?
        };
        Ok(Self {
            actif,
            debut,
            intervalle,
            echantillons,
            fil,
        })
    }

    pub fn debut(&self) -> u64 {
        self.debut
    }

    pub fn arreter(self) -> Releve {
        self.actif.store(false, Ordering::Release);
        self.fil.thread().unpark();
        let _ = self.fil.join();
        let echantillons =
            std::mem::take(&mut *self.echantillons.lock().unwrap_or_else(|e| e.into_inner()));
        Releve {
            debut: self.debut,
            fin: maintenant_ms(),
            intervalle_ms: self.intervalle,
            echantillons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_plateforme::{CompteursProcessus, Erreur, Resultat};

    const REFERENCE: &str = "tests/donnees/mesures-natif.json";

    fn mesure(prive_mo: u64, cpu_ms: u64) -> MesureProcessus {
        MesureProcessus {
            pid: 4242,
            genre: GENRE_HOTE.into(),
            role: GENRE_HOTE.into(),
            prive: prive_mo * 1024 * 1024,
            ws: prive_mo * 2 * 1024 * 1024,
            pic_ws: prive_mo * 3 * 1024 * 1024,
            cpu_ms,
        }
    }

    fn releve() -> Releve {
        let echantillons = [(40, 0), (42, 10), (44, 30), (60, 230), (61, 430), (62, 640)]
            .iter()
            .enumerate()
            .map(|(i, (prive, cpu))| Echantillon {
                t: 1_000 + 1_000 * i as u64,
                processus: vec![mesure(*prive, *cpu)],
            })
            .collect();
        Releve {
            debut: 900,
            fin: 6_100,
            intervalle_ms: INTERVALLE_MS,
            echantillons,
        }
    }

    fn systeme() -> Systeme {
        Systeme::de_la_machine(
            InfosMachine {
                os: Some("Windows 11 24H2 (build 26100.2605)".into()),
                cpu: Some("AMD Ryzen 7 5800X".into()),
                coeurs: 16,
                ram_mo: Some(32_768),
                gpu: vec!["NVIDIA GeForce RTX 3070".into()],
            },
            Some(1.25),
        )
    }

    fn export() -> ExportMesures {
        construire_export(
            releve(),
            "natif 0.1.0",
            systeme(),
            vec![
                Marqueur {
                    nom: "repos".into(),
                    debut: 1_000,
                    fin: 3_000,
                },
                Marqueur {
                    nom: "jeu".into(),
                    debut: 4_000,
                    fin: 6_000,
                },
            ],
            1_795_000_000_000,
        )
    }

    #[test]
    fn quantiles_par_interpolation_comme_l_app_tauri() {
        assert_eq!(quantile(&[], 0.5), 0.0);
        assert_eq!(quantile(&[3.0, 1.0, 2.0], 0.5), 2.0);
        assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
        assert!((quantile(&[1.0, 2.0, 3.0, 4.0], 0.95) - 3.85).abs() < 1e-9);
    }

    #[test]
    fn statistiques_d_une_fenetre() {
        let s = stats_fenetre(&releve().echantillons, 1_000, 3_000);
        assert_eq!(s.echantillons, 3);
        assert_eq!(s.duree_s, 2.0);
        assert_eq!(s.total.prive.mediane_mo, 42.0);
        assert_eq!(s.total.prive.pic_mo, 44.0);
        assert_eq!(s.total.ws.mediane_mo, 84.0);
        assert_eq!(s.total.cpu_pct, 1.5);
        assert_eq!(s.par_role.keys().collect::<Vec<_>>(), ["hote"]);
        let jeu = stats_fenetre(&releve().echantillons, 4_000, 6_000);
        assert_eq!(jeu.total.cpu_pct, 20.5);
        assert_eq!(stats_fenetre(&[], 0, 10).total.cpu_pct, 0.0);
    }

    #[test]
    fn la_machine_windows_va_dans_le_champ_windows_comme_l_app_tauri() {
        let s = systeme();
        assert_eq!(s.windows, s.os);
        assert!(s.webview2.is_none() && s.options_webview2.is_empty());
        let linux = Systeme::de_la_machine(
            InfosMachine {
                os: Some("Ubuntu 24.04.3 LTS (noyau 6.8.0)".into()),
                ..InfosMachine::default()
            },
            None,
        );
        assert_eq!(linux.windows, None);
        assert_eq!(
            linux.os.as_deref(),
            Some("Ubuntu 24.04.3 LTS (noyau 6.8.0)")
        );
    }

    #[test]
    fn l_export_suit_le_format_de_l_app_tauri_et_l_export_de_reference() {
        let valeur = serde_json::to_value(export()).unwrap();
        assert_eq!(valeur["format"], FORMAT_MESURES);
        assert_eq!(valeur["cree"], "2026-11-18T11:06:40.000Z");
        assert_eq!(valeur["echantillons"][0]["processus"][0]["type"], "hote");
        assert_eq!(
            valeur["sonde"],
            serde_json::json!({ "images": [], "messages": [], "saisie": [] })
        );
        assert_eq!(valeur["statistiques"]["scenarios"]["jeu"]["images"]["n"], 0);
        let chemin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(REFERENCE);
        let texte = format!("{}\n", serde_json::to_string_pretty(&valeur).unwrap());
        if std::env::var("ESTOMPE_MESURES").as_deref() == Ok("regenerer") {
            std::fs::write(&chemin, &texte).unwrap();
        }
        let reference = std::fs::read_to_string(&chemin).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&reference).unwrap(),
            valeur,
            "export de référence périmé : ESTOMPE_MESURES=regenerer puis relire {REFERENCE}"
        );
    }

    struct SourceFactice(Mutex<u64>);

    impl SourceMesure for SourceFactice {
        fn disponible(&self) -> bool {
            true
        }
        fn compteurs(&self) -> Resultat<CompteursProcessus> {
            let mut n = self.0.lock().unwrap();
            *n += 1;
            if *n == 2 {
                return Err(Erreur::Indisponible("compteurs du processus"));
            }
            Ok(CompteursProcessus {
                prive: *n,
                ws: *n,
                pic_ws: *n,
                cpu_ms: *n,
            })
        }
        fn machine(&self) -> InfosMachine {
            InfosMachine::default()
        }
    }

    #[test]
    fn le_releveur_echantillonne_jusqu_a_l_arret_et_saute_une_lecture_ratee() {
        let releveur =
            Releveur::demarrer(Arc::new(SourceFactice(Mutex::new(0))), INTERVALLE_MIN_MS).unwrap();
        let debut = releveur.debut();
        std::thread::sleep(Duration::from_millis(3 * INTERVALLE_MIN_MS + 100));
        let releve = releveur.arreter();
        assert_eq!(releve.debut, debut);
        assert!(releve.fin >= debut);
        assert!(releve.echantillons.len() >= 2, "{:?}", releve.echantillons);
        assert_eq!(releve.echantillons[0].processus[0].prive, 1);
        assert_eq!(releve.echantillons[1].processus[0].prive, 3);
        assert!(releve
            .echantillons
            .iter()
            .all(|e| e.processus[0].pid == std::process::id()));
    }
}
