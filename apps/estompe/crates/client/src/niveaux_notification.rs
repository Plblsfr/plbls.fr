use std::collections::BTreeMap;

use crate::types::{NotifySettings, NotifySettingsGeneral, NotifySettingsLevelsValeur};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Niveau {
    Tous,
    Mentions,
    Rien,
}

impl Niveau {
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Tous => "Tous les messages",
            Self::Mentions => "Mentions seulement",
            Self::Rien => "Rien",
        }
    }

    fn de_valeur(v: NotifySettingsLevelsValeur) -> Option<Self> {
        match v {
            NotifySettingsLevelsValeur::All => Some(Self::Tous),
            NotifySettingsLevelsValeur::Mentions => Some(Self::Mentions),
            NotifySettingsLevelsValeur::None => Some(Self::Rien),
            NotifySettingsLevelsValeur::Inconnu => None,
        }
    }

    fn valeur(self) -> NotifySettingsLevelsValeur {
        match self {
            Self::Tous => NotifySettingsLevelsValeur::All,
            Self::Mentions => NotifySettingsLevelsValeur::Mentions,
            Self::Rien => NotifySettingsLevelsValeur::None,
        }
    }

    fn de_general(v: NotifySettingsGeneral) -> Option<Self> {
        match v {
            NotifySettingsGeneral::All => Some(Self::Tous),
            NotifySettingsGeneral::Mentions => Some(Self::Mentions),
            NotifySettingsGeneral::None => Some(Self::Rien),
            NotifySettingsGeneral::Inconnu => None,
        }
    }

    fn general(self) -> NotifySettingsGeneral {
        match self {
            Self::Tous => NotifySettingsGeneral::All,
            Self::Mentions => NotifySettingsGeneral::Mentions,
            Self::Rien => NotifySettingsGeneral::None,
        }
    }

    pub fn rang(niveau: Option<Self>) -> i32 {
        match niveau {
            None => 0,
            Some(Self::Tous) => 1,
            Some(Self::Mentions) => 2,
            Some(Self::Rien) => 3,
        }
    }

    pub fn du_rang(rang: i32) -> Option<Self> {
        match rang {
            1 => Some(Self::Tous),
            2 => Some(Self::Mentions),
            3 => Some(Self::Rien),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origine {
    Sourdine,
    Salon,
    Prive,
    Serveur,
    General,
    Defaut,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Lieu<'a> {
    pub salon: Option<&'a str>,
    pub serveur: Option<&'a str>,
    pub prive: Option<&'a str>,
}

pub fn niveau_de(r: &NotifySettings, id: &str) -> Option<Niveau> {
    r.levels
        .as_ref()
        .and_then(|l| l.get(id).copied())
        .and_then(Niveau::de_valeur)
}

pub fn general(r: &NotifySettings) -> Option<Niveau> {
    r.general.and_then(Niveau::de_general)
}

pub fn sourdine(r: &NotifySettings, id: &str, maintenant: i64) -> Option<i64> {
    r.mutes
        .as_ref()
        .and_then(|m| m.get(id).copied())
        .filter(|fin| *fin == 0 || *fin > maintenant)
}

pub fn niveau_effectif(r: &NotifySettings, lieu: Lieu, maintenant: i64) -> (Niveau, Origine) {
    let ids = [
        (lieu.salon, Origine::Salon),
        (lieu.prive, Origine::Prive),
        (lieu.serveur, Origine::Serveur),
    ];
    if ids
        .iter()
        .filter_map(|(id, _)| *id)
        .any(|id| sourdine(r, id, maintenant).is_some())
    {
        return (Niveau::Rien, Origine::Sourdine);
    }
    if let Some((n, origine)) = ids
        .iter()
        .find_map(|(id, origine)| id.and_then(|id| niveau_de(r, id)).map(|n| (n, *origine)))
    {
        return (n, origine);
    }
    match (lieu.prive.is_some(), general(r)) {
        (_, Some(Niveau::Rien)) => (Niveau::Rien, Origine::General),
        (true, _) => (Niveau::Tous, Origine::Defaut),
        (false, Some(n)) => (n, Origine::General),
        (false, None) => (Niveau::Mentions, Origine::Defaut),
    }
}

pub fn description_effective(niveau: Niveau, origine: Origine) -> String {
    let d = match origine {
        Origine::Sourdine => return "En sourdine : aucune notification".to_string(),
        Origine::Salon => "réglage du salon",
        Origine::Prive => "réglage de la conversation",
        Origine::Serveur => "réglage du serveur",
        Origine::General => "réglage général",
        Origine::Defaut => "par défaut",
    };
    format!("Actuellement : {} ({d})", niveau.libelle().to_lowercase())
}

pub fn avec_niveau(r: &NotifySettings, id: &str, niveau: Option<Niveau>) -> NotifySettings {
    let mut levels = r.levels.clone().unwrap_or_default();
    match niveau {
        Some(n) => {
            levels.insert(id.to_string(), n.valeur());
        }
        None => {
            levels.remove(id);
        }
    }
    NotifySettings {
        levels: Some(levels),
        ..r.clone()
    }
}

pub fn avec_general(r: &NotifySettings, niveau: Option<Niveau>) -> NotifySettings {
    NotifySettings {
        general: niveau.map(Niveau::general),
        ..r.clone()
    }
}

pub fn apercu(r: &NotifySettings) -> bool {
    r.preview.unwrap_or(true)
}

pub fn son(r: &NotifySettings) -> bool {
    r.sound.unwrap_or(true)
}

pub fn avec_apercu(r: &NotifySettings, actif: bool) -> NotifySettings {
    NotifySettings {
        preview: Some(actif),
        ..r.clone()
    }
}

pub fn avec_son(r: &NotifySettings, actif: bool) -> NotifySettings {
    NotifySettings {
        sound: Some(actif),
        ..r.clone()
    }
}

pub fn ignore_groupe(r: &NotifySettings, serveur: &str) -> bool {
    r.ignore_group
        .as_ref()
        .is_some_and(|g| g.iter().any(|s| s == serveur))
}

pub fn avec_ignore_groupe(r: &NotifySettings, serveur: &str, actif: bool) -> NotifySettings {
    let mut groupe: Vec<String> = r
        .ignore_group
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s != serveur)
        .collect();
    if actif {
        groupe.push(serveur.to_string());
    }
    NotifySettings {
        ignore_group: Some(groupe),
        ..r.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DureeSourdine {
    QuinzeMinutes,
    UneHeure,
    HuitHeures,
    UnJour,
    Toujours,
}

pub const DUREES: [DureeSourdine; 5] = [
    DureeSourdine::QuinzeMinutes,
    DureeSourdine::UneHeure,
    DureeSourdine::HuitHeures,
    DureeSourdine::UnJour,
    DureeSourdine::Toujours,
];

const MINUTE_MS: i64 = 60_000;

impl DureeSourdine {
    pub fn id(self) -> &'static str {
        match self {
            Self::QuinzeMinutes => "sourdine-15m",
            Self::UneHeure => "sourdine-1h",
            Self::HuitHeures => "sourdine-8h",
            Self::UnJour => "sourdine-24h",
            Self::Toujours => "sourdine-toujours",
        }
    }

    pub fn depuis_id(id: &str) -> Option<Self> {
        DUREES.into_iter().find(|d| d.id() == id)
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::QuinzeMinutes => "Pendant 15 minutes",
            Self::UneHeure => "Pendant 1 heure",
            Self::HuitHeures => "Pendant 8 heures",
            Self::UnJour => "Pendant 24 heures",
            Self::Toujours => "Jusqu'à réactivation",
        }
    }

    pub fn fin(self, maintenant: i64) -> i64 {
        let minutes = match self {
            Self::QuinzeMinutes => 15,
            Self::UneHeure => 60,
            Self::HuitHeures => 8 * 60,
            Self::UnJour => 24 * 60,
            Self::Toujours => return 0,
        };
        maintenant + minutes * MINUTE_MS
    }
}

fn sourdines_actives(r: &NotifySettings, maintenant: i64) -> BTreeMap<String, i64> {
    r.mutes
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, fin)| *fin == 0 || *fin > maintenant)
        .collect()
}

pub fn avec_sourdine(
    r: &NotifySettings,
    id: &str,
    duree: Option<DureeSourdine>,
    maintenant: i64,
) -> NotifySettings {
    let mut mutes = sourdines_actives(r, maintenant);
    match duree {
        Some(d) => {
            mutes.insert(id.to_string(), d.fin(maintenant));
        }
        None => {
            mutes.remove(id);
        }
    }
    NotifySettings {
        mutes: (!mutes.is_empty()).then_some(mutes),
        ..r.clone()
    }
}

pub fn prochaine_levee(r: &NotifySettings, maintenant: i64) -> Option<i64> {
    sourdines_actives(r, maintenant)
        .into_values()
        .filter(|fin| *fin > 0)
        .min()
}

pub fn salons_regles<'a>(
    r: &NotifySettings,
    salons: impl IntoIterator<Item = &'a str>,
    maintenant: i64,
) -> Vec<&'a str> {
    salons
        .into_iter()
        .filter(|id| niveau_de(r, id).is_some() || sourdine(r, id, maintenant).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::reglages_par_defaut;

    const T: i64 = 1_795_077_000_000;

    fn salon() -> Lieu<'static> {
        Lieu {
            salon: Some("c1"),
            serveur: Some("s1"),
            prive: None,
        }
    }

    fn prive() -> Lieu<'static> {
        Lieu {
            prive: Some("d1"),
            ..Lieu::default()
        }
    }

    #[test]
    fn le_salon_passe_avant_le_serveur_qui_passe_avant_le_general() {
        let r = reglages_par_defaut();
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Mentions, Origine::Defaut)
        );
        assert_eq!(
            niveau_effectif(&r, prive(), T),
            (Niveau::Tous, Origine::Defaut)
        );
        let r = avec_general(&r, Some(Niveau::Tous));
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Tous, Origine::General)
        );
        let r = avec_niveau(&r, "s1", Some(Niveau::Rien));
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Rien, Origine::Serveur)
        );
        let r = avec_niveau(&r, "c1", Some(Niveau::Mentions));
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Mentions, Origine::Salon)
        );
        let r = avec_niveau(&r, "c1", None);
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Rien, Origine::Serveur)
        );
        assert_eq!(niveau_de(&r, "c1"), None);
    }

    #[test]
    fn le_general_ne_coupe_les_conversations_privees_que_sur_rien() {
        let r = avec_general(&reglages_par_defaut(), Some(Niveau::Mentions));
        assert_eq!(
            niveau_effectif(&r, prive(), T),
            (Niveau::Tous, Origine::Defaut)
        );
        let r = avec_general(&r, Some(Niveau::Rien));
        assert_eq!(
            niveau_effectif(&r, prive(), T),
            (Niveau::Rien, Origine::General)
        );
        let r = avec_niveau(&r, "d1", Some(Niveau::Mentions));
        assert_eq!(
            niveau_effectif(&r, prive(), T),
            (Niveau::Mentions, Origine::Prive)
        );
        assert_eq!(general(&avec_general(&r, None)), None);
        assert_eq!(
            serde_json::to_value(avec_general(&r, Some(Niveau::Tous))).unwrap()["general"],
            "all"
        );
    }

    #[test]
    fn une_sourdine_passe_avant_tout_niveau() {
        let r = avec_niveau(&reglages_par_defaut(), "c1", Some(Niveau::Tous));
        let r = avec_sourdine(&r, "s1", Some(DureeSourdine::UneHeure), T);
        assert_eq!(
            niveau_effectif(&r, salon(), T),
            (Niveau::Rien, Origine::Sourdine)
        );
        assert_eq!(
            description_effective(Niveau::Rien, Origine::Sourdine),
            "En sourdine : aucune notification"
        );
        assert_eq!(
            description_effective(Niveau::Tous, Origine::Salon),
            "Actuellement : tous les messages (réglage du salon)"
        );
    }

    #[test]
    fn une_sourdine_a_duree_limitee_se_leve_seule_a_son_terme() {
        let r = avec_sourdine(
            &reglages_par_defaut(),
            "c1",
            Some(DureeSourdine::QuinzeMinutes),
            T,
        );
        let fin = T + 15 * MINUTE_MS;
        assert_eq!(sourdine(&r, "c1", T), Some(fin));
        assert_eq!(prochaine_levee(&r, T), Some(fin));
        assert_eq!(niveau_effectif(&r, salon(), fin - 1).1, Origine::Sourdine);
        assert_eq!(sourdine(&r, "c1", fin), None);
        assert_eq!(
            niveau_effectif(&r, salon(), fin),
            (Niveau::Mentions, Origine::Defaut)
        );
        assert_eq!(prochaine_levee(&r, fin), None);
        let r = avec_sourdine(&r, "s1", Some(DureeSourdine::Toujours), fin);
        assert_eq!(
            r.mutes.clone().unwrap().into_iter().collect::<Vec<_>>(),
            [("s1".to_string(), 0)],
            "la sourdine échue est retirée au prochain enregistrement"
        );
        assert_eq!(sourdine(&r, "s1", i64::MAX - 1), Some(0));
        assert_eq!(prochaine_levee(&r, fin), None);
        let r = avec_sourdine(&r, "s1", None, fin);
        assert_eq!(r.mutes, None);
    }

    #[test]
    fn les_durees_de_sourdine_ont_un_identifiant_de_menu() {
        for d in DUREES {
            assert_eq!(DureeSourdine::depuis_id(d.id()), Some(d));
        }
        assert_eq!(DureeSourdine::UnJour.fin(0), 86_400_000);
        assert_eq!(DureeSourdine::HuitHeures.fin(0), 8 * 3_600_000);
        assert_eq!(DureeSourdine::depuis_id("lu"), None);
    }

    #[test]
    fn le_rang_d_un_niveau_suit_l_ordre_des_choix() {
        for n in [
            None,
            Some(Niveau::Tous),
            Some(Niveau::Mentions),
            Some(Niveau::Rien),
        ] {
            assert_eq!(Niveau::du_rang(Niveau::rang(n)), n);
        }
        assert_eq!(Niveau::du_rang(9), None);
    }

    #[test]
    fn apercu_son_et_mentions_de_groupe_gardent_le_reste() {
        let r = avec_niveau(&reglages_par_defaut(), "s1", Some(Niveau::Rien));
        let r = avec_son(&avec_apercu(&r, false), false);
        assert!(!apercu(&r) && !son(&r));
        assert!(apercu(&NotifySettings::default()) && son(&NotifySettings::default()));
        let r = avec_ignore_groupe(&r, "s1", true);
        let r = avec_ignore_groupe(&r, "s1", true);
        assert!(ignore_groupe(&r, "s1") && !ignore_groupe(&r, "s2"));
        assert_eq!(r.ignore_group.as_deref().map(<[String]>::len), Some(1));
        assert!(!ignore_groupe(&avec_ignore_groupe(&r, "s1", false), "s1"));
        assert_eq!(niveau_de(&r, "s1"), Some(Niveau::Rien));
    }

    #[test]
    fn les_salons_regles_ont_un_niveau_ou_une_sourdine_active() {
        let r = avec_niveau(&reglages_par_defaut(), "c2", Some(Niveau::Tous));
        let r = avec_sourdine(&r, "c3", Some(DureeSourdine::UneHeure), T);
        assert_eq!(salons_regles(&r, ["c1", "c2", "c3"], T), ["c2", "c3"]);
        assert_eq!(salons_regles(&r, ["c1", "c2", "c3"], T + 3_600_000), ["c2"]);
    }
}
