mod connus;

use std::time::{Duration, Instant};

use estompe_plateforme::{nom_de_fichier, ApplicationPremierPlan};
use serde::{Deserialize, Serialize};

pub use connus::nom_connu;

pub const PERIODE_SONDAGE: Duration = Duration::from_secs(5);
pub const DELAI_DE_GRACE: Duration = Duration::from_secs(60);
pub const LONGUEUR_MAX_ACTIVITE: usize = 64;
pub const LONGUEUR_MAX_CHEMIN: usize = 260;
pub const JEUX_MAX: usize = 200;

const HORS_JEU: &[&str] = &[
    "explorer.exe",
    "estompe.exe",
    "estompe-natif.exe",
    "applicationframehost.exe",
    "searchhost.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "taskmgr.exe",
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
    "mpv.exe",
    "vlc.exe",
    "mpc-hc64.exe",
    "wmplayer.exe",
    "microsoft.media.player.exe",
    "powerpnt.exe",
    "discord.exe",
    "screenclippinghost.exe",
    "snippingtool.exe",
];

pub fn hors_jeu(exe: &str) -> bool {
    HORS_JEU.iter().any(|n| n.eq_ignore_ascii_case(exe.trim()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesJeux {
    pub partager: bool,
    pub ajoutes: Vec<String>,
    pub exclus: Vec<String>,
}

fn liste_propre(liste: Vec<String>, valeur: impl Fn(&str) -> String) -> Vec<String> {
    let mut propre: Vec<String> = Vec::new();
    for element in liste {
        ajouter_jeu(&mut propre, &valeur(&element));
    }
    propre.truncate(JEUX_MAX);
    propre
}

impl ReglagesJeux {
    pub fn borne(self) -> Self {
        Self {
            partager: self.partager,
            ajoutes: liste_propre(self.ajoutes, |c| c.trim().to_string()),
            exclus: liste_propre(self.exclus, nom_executable),
        }
    }

    pub fn ajouter(&mut self, chemin: &str) -> bool {
        ajouter_jeu(&mut self.ajoutes, chemin)
    }

    pub fn exclure(&mut self, nom: &str) -> bool {
        ajouter_jeu(&mut self.exclus, &nom_executable(nom))
    }

    pub fn retirer_ajoute(&mut self, chemin: &str) -> bool {
        retirer(&mut self.ajoutes, chemin)
    }

    pub fn retirer_exclu(&mut self, nom: &str) -> bool {
        retirer(&mut self.exclus, nom)
    }
}

fn retirer(liste: &mut Vec<String>, valeur: &str) -> bool {
    let avant = liste.len();
    liste.retain(|x| x != valeur);
    liste.len() != avant
}

pub fn nom_executable(chemin: &str) -> String {
    nom_de_fichier(chemin.trim()).trim().to_string()
}

pub fn ajouter_jeu(liste: &mut Vec<String>, valeur: &str) -> bool {
    let v = valeur.trim();
    if v.is_empty()
        || v.chars().count() > LONGUEUR_MAX_CHEMIN
        || liste.len() >= JEUX_MAX
        || liste.iter().any(|x| x.to_lowercase() == v.to_lowercase())
    {
        return false;
    }
    liste.push(v.to_string());
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regle {
    Exclu,
    Ajoute,
    Aucune,
}

fn comparable(c: &str) -> String {
    c.trim().replace('/', "\\").to_lowercase()
}

pub fn regle(exe: &str, chemin: &str, ajoutes: &[String], exclus: &[String]) -> Regle {
    if exclus.iter().any(|e| comparable(e) == comparable(exe)) {
        Regle::Exclu
    } else if ajoutes.iter().any(|a| comparable(a) == comparable(chemin)) {
        Regle::Ajoute
    } else {
        Regle::Aucune
    }
}

fn jeu_connu(application: &ApplicationPremierPlan) -> Option<String> {
    if hors_jeu(&application.executable) {
        return None;
    }
    nom_connu(&application.executable, &application.chemin)
}

fn radical(exe: &str) -> String {
    let exe = exe.trim();
    match exe.len().checked_sub(4) {
        Some(n) if exe.is_char_boundary(n) && exe[n..].eq_ignore_ascii_case(".exe") => {
            exe[..n].to_string()
        }
        _ => exe.to_string(),
    }
}

fn nom_de_secours(application: &ApplicationPremierPlan) -> String {
    let radical = radical(&application.executable);
    match radical.to_lowercase().as_str() {
        "java" | "javaw" if !application.titre.trim().is_empty() => {
            application.titre.trim().to_string()
        }
        _ => radical,
    }
}

fn regle_de(application: &ApplicationPremierPlan, reglages: &ReglagesJeux) -> Regle {
    regle(
        &application.executable,
        &application.chemin,
        &reglages.ajoutes,
        &reglages.exclus,
    )
}

pub fn reconnaitre(
    application: &ApplicationPremierPlan,
    reglages: &ReglagesJeux,
) -> Option<String> {
    match regle_de(application, reglages) {
        Regle::Exclu => None,
        Regle::Ajoute => {
            Some(jeu_connu(application).unwrap_or_else(|| radical(&application.executable)))
        }
        Regle::Aucune => jeu_connu(application).or_else(|| {
            (!hors_jeu(&application.executable) && application.plein_ecran)
                .then(|| nom_de_secours(application))
        }),
    }
}

pub fn jeu_ouvert(fenetres: &[ApplicationPremierPlan], reglages: &ReglagesJeux) -> Option<String> {
    fenetres.iter().find_map(|f| match regle_de(f, reglages) {
        Regle::Exclu => None,
        Regle::Ajoute => Some(jeu_connu(f).unwrap_or_else(|| radical(&f.executable))),
        Regle::Aucune => jeu_connu(f),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JeuDetecte {
    pub nom: String,
    pub application: ApplicationPremierPlan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detection {
    pub premier_plan: Option<JeuDetecte>,
    pub en_cours: Option<String>,
}

pub fn detecter(
    application: Option<ApplicationPremierPlan>,
    fenetres: impl FnOnce() -> Vec<ApplicationPremierPlan>,
    reglages: &ReglagesJeux,
) -> Detection {
    let premier_plan = application.and_then(|application| {
        reconnaitre(&application, reglages).map(|nom| JeuDetecte { nom, application })
    });
    let en_cours = match &premier_plan {
        Some(jeu) => Some(jeu.nom.clone()),
        None => jeu_ouvert(&fenetres(), reglages),
    };
    Detection {
        premier_plan,
        en_cours,
    }
}

pub fn borner_activite(nom: &str) -> String {
    nom.trim().chars().take(LONGUEUR_MAX_ACTIVITE).collect()
}

#[derive(Debug, Clone, Default)]
pub struct SuiviActivite {
    dernier: Option<(String, Instant)>,
}

impl SuiviActivite {
    pub fn observer(&mut self, vu: Option<&str>, maintenant: Instant) -> String {
        if let Some(nom) = vu.map(borner_activite).filter(|n| !n.is_empty()) {
            self.dernier = Some((nom.clone(), maintenant));
            return nom;
        }
        match &self.dernier {
            Some((nom, quand)) if maintenant.saturating_duration_since(*quand) < DELAI_DE_GRACE => {
                nom.clone()
            }
            _ => {
                self.dernier = None;
                String::new()
            }
        }
    }

    pub fn oublier(&mut self) {
        self.dernier = None;
    }
}

pub fn activite_partagee(activite: &str, partager: bool) -> String {
    if partager {
        borner_activite(activite)
    } else {
        String::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Publication {
    envoyee: Option<String>,
}

impl Publication {
    pub fn a_envoyer(&mut self, connecte: bool, activite: &str, partager: bool) -> Option<String> {
        if !connecte {
            self.envoyee = None;
            return None;
        }
        let nom = activite_partagee(activite, partager);
        if self.envoyee.as_deref() == Some(nom.as_str()) {
            return None;
        }
        self.envoyee = Some(nom.clone());
        Some(nom)
    }

    pub fn echec(&mut self, nom: &str) {
        if self.envoyee.as_deref() == Some(nom) {
            self.envoyee = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_plateforme::application_de_chemin;

    fn app(chemin: &str, plein_ecran: bool) -> ApplicationPremierPlan {
        ApplicationPremierPlan {
            plein_ecran,
            ..application_de_chemin(chemin)
        }
    }

    fn reglages(ajoutes: &[&str], exclus: &[&str]) -> ReglagesJeux {
        ReglagesJeux {
            partager: true,
            ajoutes: ajoutes.iter().map(|s| s.to_string()).collect(),
            exclus: exclus.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn jeux_ajoutes_et_exclus() {
        let ajoutes = [r"D:\Jeux\Mon Jeu\jeu.exe".to_string()];
        let exclus = ["Launcher.exe".to_string()];
        assert_eq!(
            regle("jeu.exe", r"d:\jeux\mon jeu\JEU.EXE", &ajoutes, &exclus),
            Regle::Ajoute
        );
        assert_eq!(
            regle("jeu.exe", "D:/Jeux/Mon Jeu/jeu.exe", &ajoutes, &exclus),
            Regle::Ajoute
        );
        assert_eq!(
            regle("jeu.exe", r"C:\Autre\jeu.exe", &ajoutes, &exclus),
            Regle::Aucune
        );
        assert_eq!(
            regle("launcher.exe", r"C:\x\launcher.exe", &ajoutes, &exclus),
            Regle::Exclu
        );
        assert_eq!(
            regle(
                "jeu.exe",
                r"D:\Jeux\Mon Jeu\jeu.exe",
                &ajoutes,
                &["JEU.exe".to_string()]
            ),
            Regle::Exclu
        );
        assert_eq!(regle("cs2.exe", r"C:\cs2.exe", &[], &[]), Regle::Aucune);
    }

    #[test]
    fn outils_de_windows_jamais_pris_pour_un_jeu() {
        assert!(hors_jeu("ScreenClippingHost.exe"));
        assert!(hors_jeu("SnippingTool.exe"));
        assert!(hors_jeu("explorer.exe"));
        assert!(hors_jeu("estompe-natif.exe"));
        assert!(!hors_jeu("cod.exe"));
    }

    #[test]
    fn reconnait_un_jeu_connu_un_jeu_ajoute_et_une_application_plein_ecran() {
        let r = reglages(&[r"D:\Jeux\Mon Jeu\Jeu.exe"], &[]);
        assert_eq!(
            reconnaitre(&app(r"C:\Games\cs2.exe", false), &r).as_deref(),
            Some("Counter-Strike 2")
        );
        assert_eq!(
            reconnaitre(&app(r"D:\Jeux\Mon Jeu\Jeu.exe", false), &r).as_deref(),
            Some("Jeu")
        );
        assert_eq!(
            reconnaitre(&app(r"C:\Inconnu\factorio.exe", true), &r).as_deref(),
            Some("factorio")
        );
        assert_eq!(
            reconnaitre(&app(r"C:\Inconnu\factorio.exe", false), &r),
            None
        );
        assert_eq!(
            reconnaitre(&app(r"C:\Program Files\Google\chrome.exe", true), &r),
            None
        );
        let java = ApplicationPremierPlan {
            titre: "Mon serveur".into(),
            ..app(r"C:\Java\bin\javaw.exe", true)
        };
        assert_eq!(reconnaitre(&java, &r).as_deref(), Some("Mon serveur"));
    }

    #[test]
    fn un_jeu_exclu_n_est_ni_detecte_ni_publie() {
        let r = reglages(&[r"D:\Jeux\Mon Jeu\jeu.exe"], &["cs2.exe", "JEU.EXE"]);
        let cs2 = app(r"C:\Games\cs2.exe", true);
        let ajoute = app(r"D:\Jeux\Mon Jeu\jeu.exe", true);
        assert_eq!(reconnaitre(&cs2, &r), None);
        assert_eq!(reconnaitre(&ajoute, &r), None);
        let detection = detecter(Some(cs2.clone()), || vec![cs2.clone(), ajoute.clone()], &r);
        assert_eq!(detection, Detection::default());
        let mut suivi = SuiviActivite::default();
        let activite = suivi.observer(detection.en_cours.as_deref(), Instant::now());
        assert_eq!(activite, "");
        let mut publication = Publication::default();
        assert_eq!(
            publication.a_envoyer(true, &activite, r.partager),
            Some(String::new())
        );
    }

    #[test]
    fn un_jeu_ouvert_derriere_une_autre_fenetre_reste_en_cours() {
        let r = reglages(&[r"D:\Jeux\perso.exe"], &["dota2.exe"]);
        let navigateur = app(r"C:\Firefox\firefox.exe", false);
        let fenetres = || {
            vec![
                navigateur.clone(),
                app(r"C:\Steam\dota2.exe", false),
                app(r"D:\Jeux\perso.exe", false),
            ]
        };
        let detection = detecter(Some(navigateur.clone()), fenetres, &r);
        assert_eq!(detection.premier_plan, None);
        assert_eq!(detection.en_cours.as_deref(), Some("perso"));
        let jeu = app(r"C:\Games\cs2.exe", true);
        let mut enumere = false;
        let detection = detecter(
            Some(jeu.clone()),
            || {
                enumere = true;
                Vec::new()
            },
            &r,
        );
        assert!(
            !enumere,
            "pas d'énumération des fenêtres avec un jeu au premier plan"
        );
        assert_eq!(
            detection.premier_plan,
            Some(JeuDetecte {
                nom: "Counter-Strike 2".into(),
                application: jeu
            })
        );
        assert_eq!(detection.en_cours.as_deref(), Some("Counter-Strike 2"));
    }

    #[test]
    fn l_activite_survit_une_minute_a_la_fermeture_du_jeu() {
        let mut suivi = SuiviActivite::default();
        let t0 = Instant::now();
        assert_eq!(suivi.observer(Some("Hades"), t0), "Hades");
        assert_eq!(suivi.observer(None, t0 + Duration::from_secs(30)), "Hades");
        assert_eq!(suivi.observer(None, t0 + DELAI_DE_GRACE), "");
        assert_eq!(suivi.observer(None, t0 + Duration::from_secs(61)), "");
        assert_eq!(suivi.observer(Some("Hades"), t0), "Hades");
        suivi.oublier();
        assert_eq!(suivi.observer(None, t0), "");
        let long = "x".repeat(100);
        assert_eq!(
            suivi.observer(Some(&long), t0).chars().count(),
            LONGUEUR_MAX_ACTIVITE
        );
    }

    #[test]
    fn la_publication_suit_le_reglage_et_la_connexion() {
        let mut p = Publication::default();
        assert_eq!(p.a_envoyer(false, "Hades", true), None);
        assert_eq!(p.a_envoyer(true, "Hades", false), Some(String::new()));
        assert_eq!(p.a_envoyer(true, "Hades", false), None);
        assert_eq!(p.a_envoyer(true, "Hades", true), Some("Hades".into()));
        assert_eq!(p.a_envoyer(true, "Hades", true), None);
        p.echec("Hades");
        assert_eq!(p.a_envoyer(true, "Hades", true), Some("Hades".into()));
        assert_eq!(p.a_envoyer(false, "Hades", true), None);
        assert_eq!(p.a_envoyer(true, "Hades", true), Some("Hades".into()));
        assert_eq!(p.a_envoyer(true, "", true), Some(String::new()));
        p.echec("Hades");
        assert_eq!(p.a_envoyer(true, "", true), None);
    }

    #[test]
    fn les_listes_de_jeux_restent_propres() {
        let mut r = ReglagesJeux::default();
        assert!(r.ajouter(r" D:\Jeux\a.exe "));
        assert!(!r.ajouter(r"d:\jeux\A.EXE"));
        assert!(!r.ajouter("  "));
        assert!(r.exclure(r"C:\x\Launcher.exe"));
        assert!(!r.exclure("launcher.exe"));
        assert_eq!(r.ajoutes, [r"D:\Jeux\a.exe"]);
        assert_eq!(r.exclus, ["Launcher.exe"]);
        assert!(r.retirer_exclu("Launcher.exe") && !r.retirer_exclu("Launcher.exe"));
        assert!(r.retirer_ajoute(r"D:\Jeux\a.exe"));
        assert!(!r.ajouter(&"a".repeat(LONGUEUR_MAX_CHEMIN + 1)));
        let borne = ReglagesJeux {
            partager: true,
            ajoutes: vec!["a.exe".into(), "A.exe".into(), "".into()],
            exclus: vec![r"C:\b.exe".into(), "b.exe".into()],
        }
        .borne();
        assert_eq!(
            borne,
            ReglagesJeux {
                partager: true,
                ajoutes: vec!["a.exe".into()],
                exclus: vec!["b.exe".into()],
            }
        );
        let beaucoup = ReglagesJeux {
            ajoutes: (0..JEUX_MAX + 10).map(|i| format!("{i}.exe")).collect(),
            ..ReglagesJeux::default()
        }
        .borne();
        assert_eq!(beaucoup.ajoutes.len(), JEUX_MAX);
    }

    #[test]
    fn nom_de_l_executable() {
        assert_eq!(nom_executable(r"C:\Jeux\Hades.exe "), "Hades.exe");
        assert_eq!(nom_executable("hades.exe"), "hades.exe");
        assert_eq!(radical("Hades.EXE"), "Hades");
        assert_eq!(radical("é"), "é");
    }

    #[test]
    #[ignore = "mesure : cargo test -p estompe-client --release --lib -- --ignored --nocapture cout_du_sondage"]
    fn cout_du_sondage() {
        let r = reglages(&[r"D:\Jeux\perso.exe"], &["launcher.exe"]);
        let fenetres: Vec<_> = (0..30)
            .map(|i| app(&format!(r"C:\Programmes\outil{i}\outil{i}.exe"), false))
            .collect();
        let tours = 20_000;
        let debut = Instant::now();
        for _ in 0..tours {
            let d = detecter(
                Some(app(r"C:\Programmes\editeur\editeur.exe", false)),
                || fenetres.clone(),
                &r,
            );
            std::hint::black_box(d);
        }
        let par_tour = debut.elapsed() / tours;
        let part = par_tour.as_secs_f64() / PERIODE_SONDAGE.as_secs_f64() * 100.0;
        println!("reconnaissance : {par_tour:?} par sondage, {part:.5} % d'un cœur");
    }
}
