use std::collections::{BTreeSet, HashMap};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::format::date_locale;
use crate::http::{encoder_composant, Client, Corps, ErreurApi};

pub const ECHEC: &str = "aegis a refusé la demande";
pub const PAGE_COMPTES: usize = 200;
pub const LIMITE_JOURNAL: usize = 300;
pub const LIMITE_HISTORIQUE: usize = 200;
pub const LIMITE_CHOIX_COMPTE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Effet {
    #[default]
    Allow,
    Deny,
}

impl Effet {
    pub fn cle(self) -> &'static str {
        match self {
            Effet::Allow => "allow",
            Effet::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sujet {
    Role,
    Group,
    #[default]
    Account,
}

impl Sujet {
    pub fn cle(self) -> &'static str {
        match self {
            Sujet::Role => "role",
            Sujet::Group => "group",
            Sujet::Account => "account",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Sujet::Role => "rôle",
            Sujet::Group => "groupe",
            Sujet::Account => "compte",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Etat {
    Autorise,
    Partiel,
    Refuse,
    #[default]
    NonDefini,
}

impl Etat {
    pub fn libelle(self) -> &'static str {
        match self {
            Etat::Autorise => "Autorisé",
            Etat::Partiel => "Partiel",
            Etat::Refuse => "Refusé",
            Etat::NonDefini => "Non défini",
        }
    }

    pub fn ton(self) -> &'static str {
        match self {
            Etat::Autorise => "ok",
            Etat::Partiel => "att",
            Etat::Refuse => "dgr",
            Etat::NonDefini => "",
        }
    }

    pub fn icone(self) -> &'static str {
        match self {
            Etat::Autorise => "ok",
            Etat::Partiel => "filtre",
            Etat::Refuse => "x",
            Etat::NonDefini => "minus",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Compte {
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    #[serde(default = "vrai")]
    pub in_keycloak: bool,
}

fn vrai() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompteResume {
    #[serde(flatten)]
    pub compte: Compte,
    #[serde(default)]
    pub roles_count: i64,
    #[serde(default)]
    pub groups_count: i64,
    #[serde(default)]
    pub allows_count: i64,
    #[serde(default)]
    pub denies_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PageComptes {
    #[serde(default)]
    pub accounts: Vec<CompteResume>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub from_keycloak: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegleVue {
    pub id: String,
    #[serde(default)]
    pub subject_kind: Sujet,
    #[serde(default)]
    pub subject_id: String,
    pub permission: String,
    #[serde(default = "partout")]
    pub resource: String,
    pub effect: Effet,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub granted_by: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub obsolete: bool,
}

fn partout() -> String {
    "*".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LiaisonVue {
    pub id: String,
    #[serde(default)]
    pub role_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub subject_kind: Sujet,
    #[serde(default)]
    pub subject_id: String,
    #[serde(default)]
    pub subject_label: Option<String>,
    #[serde(default)]
    pub scope_type: Option<String>,
    #[serde(default)]
    pub scope_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub granted_by: String,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Appartenance {
    pub group_id: String,
    pub name: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub added_by: String,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompteDetail {
    pub account: Compte,
    #[serde(default)]
    pub bindings: Vec<LiaisonVue>,
    #[serde(default)]
    pub groups: Vec<Appartenance>,
    #[serde(default)]
    pub rules: Vec<RegleVue>,
    #[serde(default)]
    pub group_bindings: Vec<LiaisonVue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    ToutLeMonde {
        #[serde(default)]
        role_id: String,
    },
    Role {
        #[serde(default)]
        role_id: String,
        role: String,
        #[serde(default)]
        binding_id: String,
        #[serde(default)]
        group_id: Option<String>,
        #[serde(default)]
        group: Option<String>,
        #[serde(default)]
        scope_type: Option<String>,
        #[serde(default)]
        scope_id: Option<String>,
    },
    Groupe {
        #[serde(default)]
        group_id: String,
        group: String,
    },
    Compte,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regle {
    #[serde(default)]
    pub rule_id: String,
    pub permission: String,
    #[serde(default = "partout")]
    pub resource: String,
    pub effect: Effet,
    #[serde(default)]
    pub expires_at: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Explication {
    pub permission: String,
    #[serde(default)]
    pub state: Etat,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub rules: Vec<Regle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EffectifsExpliques {
    #[serde(default)]
    pub account: Compte,
    #[serde(default)]
    pub permissions: Vec<Explication>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Decision {
    pub allowed: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub rules: Vec<Regle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PermissionVue {
    pub name: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub open_default: bool,
    #[serde(default)]
    pub obsolete_at: Option<String>,
    #[serde(default)]
    pub rules_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TypeVue {
    #[serde(default)]
    pub app: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub listable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppVue {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub seen_at: String,
    #[serde(default)]
    pub resource_types: Vec<TypeVue>,
    #[serde(default)]
    pub permissions: Vec<PermissionVue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Catalogue {
    #[serde(default)]
    pub apps: Vec<AppVue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Systeme {
    #[serde(rename = "tout-le-monde")]
    ToutLeMonde,
    #[serde(rename = "admin")]
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleResume {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub system: Option<Systeme>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub rules_count: i64,
    #[serde(default)]
    pub bindings_count: i64,
    #[serde(default)]
    pub accounts_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleDetail {
    #[serde(flatten)]
    pub resume: RoleResume,
    #[serde(default)]
    pub rules: Vec<RegleVue>,
    #[serde(default)]
    pub bindings: Vec<LiaisonVue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GroupeResume {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub members_count: i64,
    #[serde(default)]
    pub roles_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Membre {
    pub account: Compte,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub added_by: String,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GroupeDetail {
    #[serde(flatten)]
    pub resume: GroupeResume,
    #[serde(default)]
    pub members: Vec<Membre>,
    #[serde(default)]
    pub bindings: Vec<LiaisonVue>,
    #[serde(default)]
    pub rules: Vec<RegleVue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EntreeJournal {
    pub id: i64,
    pub at: String,
    #[serde(default)]
    pub actor: String,
    pub action: String,
    #[serde(default)]
    pub target_kind: String,
    #[serde(default)]
    pub target_id: String,
    #[serde(default)]
    pub target_label: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub detail: Value,
    #[serde(default)]
    pub actor_account: Option<Compte>,
    #[serde(default)]
    pub subject_account: Option<Compte>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Ressource {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Detenteur {
    pub rule: RegleVue,
    #[serde(default)]
    pub subject_label: Option<String>,
    #[serde(default)]
    pub account: Option<Compte>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegleSaisie {
    pub permission: String,
    pub resource: String,
    pub effect: Effet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Copie {
    #[serde(default)]
    pub bindings: i64,
    #[serde(default)]
    pub groups: i64,
    #[serde(default)]
    pub rules: i64,
    #[serde(default)]
    pub removed: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeCopie {
    #[default]
    Ajouter,
    Remplacer,
}

impl ModeCopie {
    pub fn cle(self) -> &'static str {
        match self {
            ModeCopie::Ajouter => "ajouter",
            ModeCopie::Remplacer => "remplacer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NouvelleLiaison {
    pub role_id: String,
    pub sujet: Sujet,
    pub sujet_id: String,
    pub portee: Option<(String, String)>,
    pub expire: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NouvelleRegle {
    pub sujet: Sujet,
    pub sujet_id: String,
    pub permission: String,
    pub ressource: String,
    pub effet: Effet,
    pub expire: Option<String>,
    pub note: String,
}

pub const ONGLETS: [(&str, &str); 5] = [
    ("comptes", "Comptes"),
    ("roles", "Rôles"),
    ("groupes", "Groupes"),
    ("catalogue", "Catalogue"),
    ("journal", "Journal"),
];

pub const ACTIONS: [(&str, &str); 19] = [
    ("manifest", "manifeste reçu"),
    ("role.create", "rôle créé"),
    ("role.update", "rôle modifié"),
    ("role.delete", "rôle supprimé"),
    ("group.create", "groupe créé"),
    ("group.update", "groupe modifié"),
    ("group.delete", "groupe supprimé"),
    ("group.member.add", "ajouté au groupe"),
    ("group.member.remove", "retiré du groupe"),
    ("binding.create", "rôle donné"),
    ("binding.delete", "rôle retiré"),
    ("rule.allow", "permission accordée"),
    ("rule.deny", "permission refusée"),
    ("rule.update", "règle modifiée"),
    ("rule.delete", "règle supprimée"),
    ("account.copy", "droits copiés"),
    ("account.purge", "droits retirés"),
    ("permissions.purge", "permissions obsolètes purgées"),
    ("expire", "expiré"),
];

pub fn libelle_action(action: &str) -> &str {
    ACTIONS
        .iter()
        .find(|(cle, _)| *cle == action)
        .map_or(action, |(_, l)| l)
}

pub fn ton_action(a: &str) -> &'static str {
    if a.ends_with(".deny")
        || a.ends_with(".delete")
        || a.ends_with(".remove")
        || a.ends_with(".purge")
        || a == "expire"
    {
        "dgr"
    } else if a.ends_with(".update") {
        "info"
    } else {
        "ok"
    }
}

pub fn nom_compte(c: Option<&Compte>, sub: &str) -> String {
    match c {
        Some(c) => c
            .email
            .clone()
            .or_else(|| c.display_name.clone())
            .unwrap_or_else(|| {
                if c.sub.is_empty() {
                    sub.to_string()
                } else {
                    c.sub.clone()
                }
            }),
        None => sub.to_string(),
    }
}

pub fn quand(iso: Option<&str>, decalage_minutes: i32) -> String {
    iso.and_then(|d| date_locale(d, decalage_minutes))
        .map(|d| {
            format!(
                "{:02}/{:02}/{} {:02}:{:02}",
                d.jour, d.mois, d.annee, d.heure, d.minute
            )
        })
        .unwrap_or_else(|| "—".into())
}

pub fn expire_ou_jamais(iso: Option<&str>, decalage_minutes: i32) -> String {
    match iso {
        Some(d) => quand(Some(d), decalage_minutes),
        None => "jamais".into(),
    }
}

fn jours_depuis_epoque(annee: i64, mois: u32, jour: u32) -> i64 {
    let a = if mois <= 2 { annee - 1 } else { annee };
    let ere = a.div_euclid(400);
    let yoe = a - ere * 400;
    let m = i64::from(mois);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(jour) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    ere * 146_097 + doe - 719_468
}

fn jours_du_mois(annee: i64, mois: u32) -> u32 {
    match mois {
        2 if (annee % 4 == 0 && annee % 100 != 0) || annee % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub const FORMAT_DATE: &str = "JJ/MM/AAAA HH:MM";

pub fn vers_iso(saisie: &str, decalage_minutes: i32) -> Result<Option<String>, String> {
    let s = saisie.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let invalide = || format!("Date invalide : « {s} » (format {FORMAT_DATE}).");
    let (date, heure) = s.split_once(' ').unwrap_or((s, "00:00"));
    let morceaux: Vec<&str> = date.split('/').collect();
    let [j, m, a] = morceaux.as_slice() else {
        return Err(invalide());
    };
    let (Ok(jour), Ok(mois), Ok(annee)) = (j.parse::<u32>(), m.parse::<u32>(), a.parse::<i64>())
    else {
        return Err(invalide());
    };
    let (h, mi) = heure.trim().split_once(':').unwrap_or((heure.trim(), "0"));
    let (Ok(h), Ok(mi)) = (h.parse::<i64>(), mi.parse::<i64>()) else {
        return Err(invalide());
    };
    if !(1..=12).contains(&mois)
        || jour == 0
        || jour > jours_du_mois(annee, mois)
        || !(0..24).contains(&h)
        || !(0..60).contains(&mi)
        || !(1970..=9999).contains(&annee)
    {
        return Err(invalide());
    }
    let ms = (jours_depuis_epoque(annee, mois, jour) * 1440 + h * 60 + mi
        - i64::from(decalage_minutes))
        * 60_000;
    Ok(Some(crate::format::iso_de_ms(ms)))
}

pub fn origine(s: &Source) -> String {
    match s {
        Source::ToutLeMonde { .. } => "Tout le monde".into(),
        Source::Compte => "règle directe".into(),
        Source::Groupe { group, .. } => format!("groupe {group}"),
        Source::Role {
            role,
            group,
            scope_id,
            ..
        } => {
            let mut t = format!("rôle {role}");
            if let Some(g) = group {
                t.push_str(&format!(" (via le groupe {g})"));
            }
            if let Some(s) = scope_id {
                t.push_str(&format!(" sur {s}"));
            }
            t
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiltreEffectifs {
    #[default]
    Distingue,
    Autorise,
    Refuse,
    Tout,
}

pub const FILTRES_EFFECTIFS: [FiltreEffectifs; 4] = [
    FiltreEffectifs::Distingue,
    FiltreEffectifs::Autorise,
    FiltreEffectifs::Refuse,
    FiltreEffectifs::Tout,
];

pub fn se_distingue(e: &Explication) -> bool {
    e.rules
        .iter()
        .any(|r| !matches!(r.source, Source::ToutLeMonde { .. }))
}

impl FiltreEffectifs {
    pub fn libelle(self) -> &'static str {
        match self {
            FiltreEffectifs::Distingue => "Ce qui le distingue",
            FiltreEffectifs::Autorise => "Autorisées",
            FiltreEffectifs::Refuse => "Refusées",
            FiltreEffectifs::Tout => "Toutes",
        }
    }

    pub fn garde(self, e: &Explication) -> bool {
        match self {
            FiltreEffectifs::Distingue => se_distingue(e),
            FiltreEffectifs::Autorise => matches!(e.state, Etat::Autorise | Etat::Partiel),
            FiltreEffectifs::Refuse => e.state == Etat::Refuse,
            FiltreEffectifs::Tout => true,
        }
    }
}

pub fn compter_effectifs(perms: &[Explication]) -> [usize; 4] {
    FILTRES_EFFECTIFS.map(|f| perms.iter().filter(|e| f.garde(e)).count())
}

fn uniques(noms: impl Iterator<Item = String>) -> Vec<String> {
    let mut vus = Vec::new();
    for n in noms {
        if !vus.contains(&n) {
            vus.push(n);
        }
    }
    vus
}

pub fn resume_effectif(e: &Explication) -> String {
    if e.rules.is_empty() {
        return "aucune règle : refusé".into();
    }
    let refus: Vec<&Regle> = e.rules.iter().filter(|r| r.effect == Effet::Deny).collect();
    let decisives: Vec<&Regle> = if e.state == Etat::Refuse {
        refus.clone()
    } else {
        e.rules
            .iter()
            .filter(|r| r.effect == Effet::Allow)
            .collect()
    };
    let base: Vec<&Regle> = if decisives.is_empty() {
        e.rules.iter().collect()
    } else {
        decisives
    };
    let noms = uniques(base.iter().map(|r| origine(&r.source)));
    let verbe = if e.state == Etat::Refuse {
        "refusé par"
    } else {
        "accordé par"
    };
    let reste = if e.state == Etat::Partiel && !refus.is_empty() {
        format!(
            " · refus de {}",
            uniques(refus.iter().map(|r| origine(&r.source))).join(", ")
        )
    } else {
        String::new()
    };
    format!("{verbe} {}{reste}", noms.join(", "))
}

fn cle_regle(r: &RegleSaisie) -> (String, String) {
    (r.permission.clone(), r.resource.clone())
}

pub fn permissions_modifiees(avant: &[RegleSaisie], apres: &[RegleSaisie]) -> Vec<String> {
    let a: HashMap<_, _> = avant.iter().map(|r| (cle_regle(r), r.effect)).collect();
    let b: HashMap<_, _> = apres.iter().map(|r| (cle_regle(r), r.effect)).collect();
    let mut touchees: Vec<String> = Vec::new();
    let mut toucher = |p: &str| {
        if !touchees.iter().any(|t| t == p) {
            touchees.push(p.to_string());
        }
    };
    for r in avant {
        if b.get(&cle_regle(r)) != Some(&r.effect) {
            toucher(&r.permission);
        }
    }
    for r in apres {
        if a.get(&cle_regle(r)) != Some(&r.effect) {
            toucher(&r.permission);
        }
    }
    touchees
}

fn pluriel(n: usize, mot: &str) -> String {
    format!("{n} {mot}{}", if n > 1 { "s" } else { "" })
}

pub fn resume_modifications(libelles: &[String], autres: usize) -> String {
    let n = libelles.len() + autres;
    if n == 0 {
        return String::new();
    }
    let s = if n > 1 { "s" } else { "" };
    let montres = libelles
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let reste = libelles.len() - libelles.len().min(3);
    let mut t = format!("{n} modification{s} non enregistrée{s}");
    if !montres.is_empty() {
        t.push_str(&format!(" · {montres}"));
    }
    if reste > 0 {
        t.push_str(&format!(
            " et {reste} autre{}",
            if reste > 1 { "s" } else { "" }
        ));
    }
    t
}

pub fn initiales(c: &Compte) -> String {
    let base = match c.display_name.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => c
            .email
            .as_deref()
            .unwrap_or(&c.sub)
            .split('@')
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    let mots: Vec<&str> = base
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '_' | '-'))
        .filter(|m| !m.is_empty())
        .collect();
    let premiere = |i: usize| mots.get(i).and_then(|m| m.chars().next());
    let mut t: String = premiere(0).unwrap_or('?').to_uppercase().collect();
    if let Some(c) = premiere(1) {
        t.extend(c.to_uppercase());
    }
    t
}

pub fn teinte(cle: &str) -> i32 {
    let n = cle
        .encode_utf16()
        .fold(7u32, |n, ch| n.wrapping_mul(31).wrapping_add(u32::from(ch)));
    (n % 6) as i32 + 1
}

pub fn libelles_permissions(c: &Catalogue) -> HashMap<String, PermissionVue> {
    c.apps
        .iter()
        .flat_map(|a| a.permissions.iter().map(|p| (p.name.clone(), p.clone())))
        .collect()
}

pub fn type_de(noms: &HashMap<String, PermissionVue>, permission: &str) -> Option<String> {
    let p = noms.get(permission)?;
    p.resource_type.as_ref().map(|t| format!("{}:{t}", p.app))
}

pub fn libelle_permission(noms: &HashMap<String, PermissionVue>, permission: &str) -> String {
    noms.get(permission)
        .map(|p| p.label.clone())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| permission.to_string())
}

pub fn resume_compte(c: &CompteResume, decalage_minutes: i32) -> String {
    let morceaux: Vec<String> = [
        (c.roles_count > 0).then(|| pluriel(c.roles_count as usize, "rôle")),
        (c.groups_count > 0).then(|| pluriel(c.groups_count as usize, "groupe")),
        (c.denies_count > 0).then(|| format!("{} refus", c.denies_count)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if morceaux.is_empty() {
        format!(
            "vu {}",
            quand(c.compte.last_seen_at.as_deref(), decalage_minutes)
        )
    } else {
        morceaux.join(" · ")
    }
}

pub fn qui_a_le_role(r: &RoleResume) -> String {
    if r.system == Some(Systeme::ToutLeMonde) {
        "tous les comptes".into()
    } else {
        pluriel(r.accounts_count as usize, "compte")
    }
}

pub fn note_role(r: &RoleDetail) -> String {
    let groupes = r
        .bindings
        .iter()
        .filter(|l| l.subject_kind == Sujet::Group)
        .count();
    let comptes = r.bindings.len() - groupes;
    if r.resume.system == Some(Systeme::ToutLeMonde) {
        format!(
            "Rôle système, appliqué d'office · {} dans l'annuaire",
            pluriel(r.resume.accounts_count as usize, "compte")
        )
    } else {
        format!(
            "S'ajoute à « Tout le monde » · {} et {}",
            pluriel(comptes, "compte"),
            pluriel(groupes, "groupe")
        )
    }
}

pub fn resume_groupe(g: &GroupeResume) -> String {
    format!(
        "{} · {}",
        pluriel(g.members_count as usize, "membre"),
        pluriel(g.roles_count as usize, "rôle")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Categorie<'a> {
    pub nom: String,
    pub permissions: Vec<&'a PermissionVue>,
}

pub fn categories<'a>(
    app: Option<&'a AppVue>,
    garde: impl Fn(&PermissionVue) -> bool,
) -> Vec<Categorie<'a>> {
    let perms: Vec<&PermissionVue> = app
        .map(|a| {
            a.permissions
                .iter()
                .filter(|p| p.obsolete_at.is_none() && garde(p))
                .collect()
        })
        .unwrap_or_default();
    let nom = |p: &PermissionVue| {
        if p.category.is_empty() {
            "Autres".to_string()
        } else {
            p.category.clone()
        }
    };
    let mut ordre: Vec<String> = Vec::new();
    for p in &perms {
        let n = nom(p);
        if !ordre.contains(&n) {
            ordre.push(n);
        }
    }
    ordre
        .into_iter()
        .map(|c| Categorie {
            permissions: perms.iter().copied().filter(|p| nom(p) == c).collect(),
            nom: c,
        })
        .collect()
}

pub fn correspond_au_filtre(p: &PermissionVue, filtre: &str) -> bool {
    let f = filtre.trim().to_lowercase();
    f.is_empty()
        || format!("{} {} {}", p.label, p.name, p.category)
            .to_lowercase()
            .contains(&f)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cellule {
    pub effet: Option<Effet>,
    pub exceptions: usize,
}

pub fn cellule(role: Option<&RoleDetail>, permission: &str) -> Cellule {
    let regles: Vec<&RegleVue> = role
        .map(|r| {
            r.rules
                .iter()
                .filter(|x| x.permission == permission)
                .collect()
        })
        .unwrap_or_default();
    Cellule {
        effet: regles.iter().find(|x| x.resource == "*").map(|x| x.effect),
        exceptions: regles.iter().filter(|x| x.resource != "*").count(),
    }
}

pub fn mot_effet(effet: Option<Effet>) -> &'static str {
    match effet {
        Some(Effet::Allow) => "autorise",
        Some(Effet::Deny) => "refuse",
        None => "ne dit rien",
    }
}

pub fn exceptions(n: usize) -> String {
    format!("{n} exception{}", if n > 1 { "s" } else { "" })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParApp<'a> {
    pub cle: String,
    pub nom: String,
    pub lignes: Vec<&'a Explication>,
}

pub fn effectifs_par_app<'a>(
    catalogue: &Catalogue,
    permissions: &'a [Explication],
    filtre: FiltreEffectifs,
) -> Vec<ParApp<'a>> {
    let mut vus = BTreeSet::new();
    let mut groupes: Vec<ParApp<'a>> = catalogue
        .apps
        .iter()
        .map(|a| ParApp {
            cle: a.key.clone(),
            nom: a.name.clone(),
            lignes: a
                .permissions
                .iter()
                .filter_map(|p| {
                    let e = permissions.iter().find(|x| x.permission == p.name)?;
                    vus.insert(e.permission.clone());
                    filtre.garde(e).then_some(e)
                })
                .collect(),
        })
        .collect();
    groupes.push(ParApp {
        cle: String::new(),
        nom: "Autres".into(),
        lignes: permissions
            .iter()
            .filter(|e| !vus.contains(&e.permission) && filtre.garde(e))
            .collect(),
    });
    groupes.retain(|g| !g.lignes.is_empty());
    groupes
}

pub fn message_copie(essai: &Copie, source: &str, mode: ModeCopie) -> String {
    let mut t = format!(
        "{} rôle(s), {} groupe(s) et {} règle(s) de {source} seront copiés",
        essai.bindings, essai.groups, essai.rules
    );
    if mode == ModeCopie::Remplacer {
        t.push_str(&format!(
            ", après retrait des {} droit(s) actuels",
            essai.removed
        ));
    }
    t.push('.');
    t
}

pub fn acteur(l: &EntreeJournal) -> String {
    if l.actor.starts_with("service:") || l.actor.starts_with("manifeste:") || l.actor == "aegis" {
        l.actor.clone()
    } else {
        nom_compte(l.actor_account.as_ref(), &l.actor)
    }
}

pub fn cible(l: &EntreeJournal, noms: &HashMap<String, PermissionVue>) -> String {
    if l.target_kind == "permission" {
        if let Some(p) = noms.get(&l.target_id) {
            return p.label.clone();
        }
    }
    l.target_label
        .clone()
        .unwrap_or_else(|| l.target_id.clone())
}

pub fn pour_qui(l: &EntreeJournal) -> Option<String> {
    let sujet = l.subject.as_deref()?;
    (l.target_kind != "account").then(|| nom_compte(l.subject_account.as_ref(), sujet))
}

pub fn extra_journal(l: &EntreeJournal, decalage_minutes: i32) -> String {
    let d = &l.detail;
    let liste = |cle: &str| {
        d.get(cle)
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
    };
    [
        d.get("expires_at")
            .and_then(Value::as_str)
            .map(|e| format!("jusqu'au {}", quand(Some(e), decalage_minutes))),
        d.get("note")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .map(|n| format!("« {n} »")),
        liste("added").map(|a| format!("+ {a}")),
        liste("removed").map(|r| format!("− {r}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RessourceJournal {
    Regle { type_: Option<String>, id: String },
    Portee { type_: Option<String>, id: String },
}

pub fn ressource_journal(
    l: &EntreeJournal,
    noms: &HashMap<String, PermissionVue>,
) -> Option<RessourceJournal> {
    let d = &l.detail;
    if let Some(r) = d
        .get("resource")
        .and_then(Value::as_str)
        .filter(|r| *r != "*")
    {
        return Some(RessourceJournal::Regle {
            type_: type_de(noms, &l.target_id),
            id: r.to_string(),
        });
    }
    d.get("scope_id")
        .and_then(Value::as_str)
        .map(|id| RessourceJournal::Portee {
            type_: d
                .get("scope_type")
                .and_then(Value::as_str)
                .map(str::to_string),
            id: id.to_string(),
        })
}

pub fn types_listables(c: &Catalogue) -> Vec<(String, String)> {
    c.apps
        .iter()
        .flat_map(|a| {
            a.resource_types
                .iter()
                .filter(|t| t.listable)
                .map(move |t| {
                    (
                        format!("{}:{}", a.key, t.key),
                        format!("{} ({})", t.name, a.name),
                    )
                })
        })
        .collect()
}

pub fn permissions_actives(c: &Catalogue) -> Vec<&PermissionVue> {
    c.apps
        .iter()
        .flat_map(|a| a.permissions.iter().filter(|p| p.obsolete_at.is_none()))
        .collect()
}

pub fn obsoletes(c: &Catalogue) -> usize {
    c.apps
        .iter()
        .flat_map(|a| &a.permissions)
        .filter(|p| p.obsolete_at.is_some())
        .count()
}

pub fn chemin_ressources(type_: &str) -> String {
    format!(
        "/resources/{}",
        type_
            .split(':')
            .map(encoder_composant)
            .collect::<Vec<_>>()
            .join("/")
    )
}

#[derive(Debug, Clone, Default)]
pub struct NomsRessources {
    connus: HashMap<(String, String), String>,
}

impl NomsRessources {
    pub fn nom(&self, type_: Option<&str>, id: &str) -> String {
        if id == "*" {
            return "partout".into();
        }
        type_
            .and_then(|t| self.connus.get(&(t.to_string(), id.to_string())))
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }

    pub fn connu(&self, type_: &str, id: &str) -> bool {
        self.connus
            .contains_key(&(type_.to_string(), id.to_string()))
    }

    pub fn retenir(&mut self, type_: &str, id: &str, nom: &str) {
        self.connus
            .insert((type_.to_string(), id.to_string()), nom.to_string());
    }
}

pub fn cles_du_catalogue(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|l| {
            let reste = l.trim_start().strip_prefix("e(")?.trim_start();
            let reste = reste.strip_prefix('"')?;
            let fin = reste.find('"')?;
            let cle = &reste[..fin];
            cle.chars()
                .all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
                })
                .then(|| cle.to_string())
        })
        .collect()
}

pub fn permissions_utilisees(source: &str) -> Vec<String> {
    let code = source.split("#[cfg(test)]").next().unwrap_or(source);
    let mut trouvees = Vec::new();
    for (motif, prefixe) in [(".capacite(\"", ""), ("\"estompe:", "")] {
        let mut reste = code;
        while let Some(i) = reste.find(motif) {
            reste = &reste[i + motif.len()..];
            let fin = reste.find('"').unwrap_or(0);
            let cle = &reste[..fin];
            if !cle.is_empty()
                && !cle.contains(|c: char| c.is_whitespace() || c == '{')
                && !cle.starts_with('/')
            {
                trouvees.push(format!("{prefixe}{cle}"));
            }
        }
    }
    trouvees
}

impl Client {
    async fn acces<T: serde::de::DeserializeOwned + Default>(
        &self,
        methode: Method,
        chemin: &str,
        corps: Option<Value>,
    ) -> Result<T, ErreurApi> {
        let corps = match corps {
            Some(v) => Corps::json(&v)?,
            None => Corps::Aucun,
        };
        self.requete(
            methode,
            &format!("/admin/acces{chemin}"),
            &corps,
            Default::default(),
        )
        .await
        .map(Option::unwrap_or_default)
    }

    async fn acces_vide(
        &self,
        methode: Method,
        chemin: &str,
        corps: Option<Value>,
    ) -> Result<(), ErreurApi> {
        self.acces::<Value>(methode, chemin, corps)
            .await
            .map(|_| ())
    }

    pub async fn catalogue_acces(&self) -> Result<Catalogue, ErreurApi> {
        self.acces(Method::GET, "/catalog", None).await
    }

    pub async fn comptes_acces(
        &self,
        recherche: &str,
        limite: usize,
        decalage: usize,
    ) -> Result<PageComptes, ErreurApi> {
        let mut chemin = format!("/accounts?limit={limite}");
        if decalage > 0 {
            chemin.push_str(&format!("&offset={decalage}"));
        }
        if !recherche.is_empty() {
            chemin.push_str(&format!("&q={}", encoder_composant(recherche)));
        }
        self.acces(Method::GET, &chemin, None).await
    }

    pub async fn compte_acces(&self, sub: &str) -> Result<CompteDetail, ErreurApi> {
        self.acces(
            Method::GET,
            &format!("/accounts/{}", encoder_composant(sub)),
            None,
        )
        .await
    }

    pub async fn effectifs_acces(&self, sub: &str) -> Result<EffectifsExpliques, ErreurApi> {
        self.acces(
            Method::GET,
            &format!("/accounts/{}/effective", encoder_composant(sub)),
            None,
        )
        .await
    }

    pub async fn journal_acces(
        &self,
        action: &str,
        sujet: Option<&str>,
        limite: usize,
    ) -> Result<Vec<EntreeJournal>, ErreurApi> {
        let mut chemin = String::from("/audit?");
        if let Some(s) = sujet {
            chemin.push_str(&format!("subject={}&", encoder_composant(s)));
        }
        chemin.push_str(&format!("limit={limite}"));
        if !action.is_empty() {
            chemin.push_str(&format!("&action={}", encoder_composant(action)));
        }
        self.acces(Method::GET, &chemin, None).await
    }

    pub async fn simuler_acces(
        &self,
        sub: &str,
        permission: &str,
        ressource: Option<&str>,
    ) -> Result<Decision, ErreurApi> {
        self.acces(
            Method::POST,
            "/simulate",
            Some(json!({"sub": sub, "permission": permission, "resource": ressource})),
        )
        .await
    }

    pub async fn copier_droits(
        &self,
        sub: &str,
        source: &str,
        mode: ModeCopie,
        essai: bool,
    ) -> Result<Copie, ErreurApi> {
        let mut corps = json!({"from": source, "mode": mode.cle()});
        if essai {
            corps["dry_run"] = json!(true);
        }
        self.acces(
            Method::POST,
            &format!("/accounts/{}/copy", encoder_composant(sub)),
            Some(corps),
        )
        .await
    }

    pub async fn retirer_droits(&self, sub: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!("/accounts/{}/grants", encoder_composant(sub)),
            None,
        )
        .await
    }

    pub async fn roles_acces(&self) -> Result<Vec<RoleResume>, ErreurApi> {
        self.acces(Method::GET, "/roles", None).await
    }

    pub async fn role_acces(&self, id: &str) -> Result<RoleDetail, ErreurApi> {
        self.acces(
            Method::GET,
            &format!("/roles/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn creer_role_acces(&self, nom: &str) -> Result<RoleDetail, ErreurApi> {
        self.acces(
            Method::POST,
            "/roles",
            Some(json!({"name": nom, "rules": []})),
        )
        .await
    }

    pub async fn modifier_role_acces(
        &self,
        id: &str,
        nom: &str,
        description: &str,
        regles: &[RegleSaisie],
    ) -> Result<RoleDetail, ErreurApi> {
        self.acces(
            Method::PUT,
            &format!("/roles/{}", encoder_composant(id)),
            Some(json!({"name": nom, "description": description, "rules": regles})),
        )
        .await
    }

    pub async fn supprimer_role_acces(&self, id: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!("/roles/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn groupes_acces(&self) -> Result<Vec<GroupeResume>, ErreurApi> {
        self.acces(Method::GET, "/groups", None).await
    }

    pub async fn groupe_acces(&self, id: &str) -> Result<GroupeDetail, ErreurApi> {
        self.acces(
            Method::GET,
            &format!("/groups/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn creer_groupe_acces(&self, nom: &str) -> Result<GroupeDetail, ErreurApi> {
        self.acces(Method::POST, "/groups", Some(json!({"name": nom})))
            .await
    }

    pub async fn supprimer_groupe_acces(&self, id: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!("/groups/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn ajouter_au_groupe(
        &self,
        groupe: &str,
        sub: &str,
        expire: Option<&str>,
    ) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::PUT,
            &format!(
                "/groups/{}/members/{}",
                encoder_composant(groupe),
                encoder_composant(sub)
            ),
            Some(json!({"expires_at": expire})),
        )
        .await
    }

    pub async fn retirer_du_groupe(&self, groupe: &str, sub: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!(
                "/groups/{}/members/{}",
                encoder_composant(groupe),
                encoder_composant(sub)
            ),
            None,
        )
        .await
    }

    pub async fn creer_liaison(&self, l: &NouvelleLiaison) -> Result<(), ErreurApi> {
        let (scope_type, scope_id) = match &l.portee {
            Some((t, i)) => (Some(t.as_str()), Some(i.as_str())),
            None => (None, None),
        };
        self.acces_vide(
            Method::POST,
            "/bindings",
            Some(json!({
                "role_id": l.role_id, "subject_kind": l.sujet.cle(), "subject_id": l.sujet_id,
                "scope_type": scope_type, "scope_id": scope_id, "expires_at": l.expire,
            })),
        )
        .await
    }

    pub async fn supprimer_liaison(&self, id: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!("/bindings/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn creer_regle(&self, r: &NouvelleRegle) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::POST,
            "/rules",
            Some(json!({
                "subject_kind": r.sujet.cle(), "subject_id": r.sujet_id, "permission": r.permission,
                "resource": r.ressource, "effect": r.effet.cle(), "expires_at": r.expire, "note": r.note,
            })),
        )
        .await
    }

    pub async fn supprimer_regle(&self, id: &str) -> Result<(), ErreurApi> {
        self.acces_vide(
            Method::DELETE,
            &format!("/rules/{}", encoder_composant(id)),
            None,
        )
        .await
    }

    pub async fn detenteurs_acces(&self, permission: &str) -> Result<Vec<Detenteur>, ErreurApi> {
        self.acces(
            Method::GET,
            &format!("/permissions/{}/holders", encoder_composant(permission)),
            None,
        )
        .await
    }

    pub async fn purger_permissions_obsoletes(&self) -> Result<(), ErreurApi> {
        self.acces_vide(Method::POST, "/permissions/purge-obsolete", None)
            .await
    }

    pub async fn ressources_acces(
        &self,
        type_: &str,
        recherche: &str,
    ) -> Result<Vec<Ressource>, ErreurApi> {
        let mut chemin = chemin_ressources(type_);
        if !recherche.is_empty() {
            chemin.push_str(&format!("?q={}", encoder_composant(recherche)));
        }
        self.acces(Method::GET, &chemin, None).await
    }

    pub async fn noms_de_ressources(
        &self,
        type_: &str,
        ids: &[String],
    ) -> Result<Vec<Ressource>, ErreurApi> {
        let chemin = format!(
            "{}?ids={}",
            chemin_ressources(type_),
            encoder_composant(&ids.join(","))
        );
        self.acces(Method::GET, &chemin, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::SECTIONS;

    const CATALOGUE: &str = include_str!("../../../../../api/estompe/core/src/catalogue.rs");

    fn sources() -> Vec<(String, String)> {
        let racine = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut fichiers = Vec::new();
        let mut a_voir = vec![
            racine.join("client/src"),
            racine.join("plateforme/src"),
            racine.join("../natif/src"),
            racine.join("../natif/ui"),
        ];
        while let Some(d) = a_voir.pop() {
            let Ok(entrees) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entrees.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if p.file_name().is_some_and(|n| n != "genere") {
                        a_voir.push(p);
                    }
                } else if p.extension().is_some_and(|x| x == "rs" || x == "slint") {
                    if let Ok(texte) = std::fs::read_to_string(&p) {
                        fichiers.push((p.display().to_string(), texte));
                    }
                }
            }
        }
        fichiers
    }

    #[test]
    fn lit_le_catalogue_de_l_api() {
        let cles = cles_du_catalogue(CATALOGUE);
        assert!(cles.len() > 40, "{} clés", cles.len());
        assert!(cles.contains("access"));
        assert!(sources().len() > 100);
    }

    #[test]
    fn n_utilise_que_des_permissions_du_catalogue() {
        let cles = cles_du_catalogue(CATALOGUE);
        let inconnues: Vec<String> = sources()
            .iter()
            .filter(|(f, _)| !f.ends_with("acces.rs"))
            .flat_map(|(f, texte)| {
                permissions_utilisees(texte)
                    .into_iter()
                    .filter(|k| !cles.contains(k))
                    .map(move |k| format!("{f}: {k}"))
            })
            .collect();
        assert_eq!(inconnues, Vec::<String>::new());
        assert_eq!(
            permissions_utilisees(
                "x.capacite(\"gifs.use\"); \"estompe:admin.view\" #[cfg(test)] \"estompe:faux\""
            ),
            ["gifs.use", "admin.view"]
        );
    }

    #[test]
    fn chaque_section_d_administration_suit_une_permission_du_catalogue() {
        let cles = cles_du_catalogue(CATALOGUE);
        for s in SECTIONS {
            if let Some(c) = s.capacite {
                assert!(cles.contains(c.trim_start_matches("estompe:")), "{c}");
            }
        }
    }

    fn regle(effect: Effet, source: Source) -> Regle {
        Regle {
            rule_id: "r".into(),
            permission: "p".into(),
            resource: "*".into(),
            effect,
            expires_at: None,
            source,
        }
    }

    fn role(nom: &str) -> Source {
        Source::Role {
            role_id: "x".into(),
            role: nom.into(),
            binding_id: "b".into(),
            group_id: None,
            group: None,
            scope_type: None,
            scope_id: None,
        }
    }

    #[test]
    fn lit_les_reponses_d_aegis() {
        let page: PageComptes = serde_json::from_value(json!({
            "accounts": [{"sub": "s1", "email": "a@b.fr", "display_name": null, "last_seen_at": null, "in_keycloak": false, "roles_count": 2, "groups_count": 0, "allows_count": 0, "denies_count": 1}],
            "total": 1, "from_keycloak": 0
        }))
        .unwrap();
        assert_eq!(page.accounts[0].compte.email.as_deref(), Some("a@b.fr"));
        assert_eq!(resume_compte(&page.accounts[0], 60), "2 rôles · 1 refus");
        let e: Explication = serde_json::from_value(json!({
            "permission": "estompe:access", "state": "partiel", "allow": ["*"], "deny": ["s1"],
            "rules": [{"rule_id": "1", "permission": "estompe:access", "resource": "*", "effect": "allow", "expires_at": null, "source": {"kind": "tout_le_monde", "role_id": "t"}},
                      {"rule_id": "2", "permission": "estompe:access", "resource": "s1", "effect": "deny", "expires_at": null, "source": {"kind": "groupe", "group_id": "g", "group": "Invités"}}]
        }))
        .unwrap();
        assert_eq!(e.state, Etat::Partiel);
        assert_eq!(
            resume_effectif(&e),
            "accordé par Tout le monde · refus de groupe Invités"
        );
        let r: RoleDetail = serde_json::from_value(json!({
            "id": "r1", "name": "Tout le monde", "description": "", "system": "tout-le-monde", "created_at": "", "rules_count": 0, "bindings_count": 0, "accounts_count": 3,
            "rules": [], "bindings": []
        }))
        .unwrap();
        assert_eq!(r.resume.system, Some(Systeme::ToutLeMonde));
        assert_eq!(qui_a_le_role(&r.resume), "tous les comptes");
    }

    #[test]
    fn nomme_un_compte_et_ses_initiales() {
        let mut c = Compte {
            sub: "abc".into(),
            email: Some("jean.dupont@x.fr".into()),
            ..Compte::default()
        };
        assert_eq!(nom_compte(Some(&c), ""), "jean.dupont@x.fr");
        assert_eq!(initiales(&c), "JD");
        c.display_name = Some("Émile".into());
        assert_eq!(initiales(&c), "É");
        c.email = None;
        assert_eq!(nom_compte(Some(&c), ""), "Émile");
        assert_eq!(nom_compte(None, "sub-1"), "sub-1");
        assert!((1..=6).contains(&teinte("abc")));
        assert_eq!(teinte("abc"), teinte("abc"));
    }

    #[test]
    fn convertit_les_dates_saisies() {
        assert_eq!(vers_iso("", 60), Ok(None));
        assert_eq!(
            vers_iso("05/01/2027 10:30", 60),
            Ok(Some("2027-01-05T09:30:00.000Z".into()))
        );
        assert_eq!(
            vers_iso("29/02/2028", 0),
            Ok(Some("2028-02-29T00:00:00.000Z".into()))
        );
        assert!(vers_iso("30/02/2027", 0).is_err());
        assert!(vers_iso("demain", 0).is_err());
        assert_eq!(quand(Some("2027-01-05T09:30:00Z"), 60), "05/01/2027 10:30");
        assert_eq!(quand(None, 60), "—");
        assert_eq!(expire_ou_jamais(None, 0), "jamais");
    }

    #[test]
    fn explique_l_origine_des_droits() {
        assert_eq!(origine(&Source::Compte), "règle directe");
        assert_eq!(
            origine(&Source::Role {
                role_id: "x".into(),
                role: "Modo".into(),
                binding_id: "b".into(),
                group_id: Some("g".into()),
                group: Some("Équipe".into()),
                scope_type: None,
                scope_id: Some("s1".into()),
            }),
            "rôle Modo (via le groupe Équipe) sur s1"
        );
        let refuse = Explication {
            permission: "p".into(),
            state: Etat::Refuse,
            rules: vec![
                regle(Effet::Allow, role("A")),
                regle(Effet::Deny, Source::Compte),
            ],
            ..Explication::default()
        };
        assert_eq!(resume_effectif(&refuse), "refusé par règle directe");
        let vide = Explication::default();
        assert_eq!(resume_effectif(&vide), "aucune règle : refusé");
        let base = Explication {
            permission: "q".into(),
            state: Etat::Autorise,
            rules: vec![regle(
                Effet::Allow,
                Source::ToutLeMonde {
                    role_id: "t".into(),
                },
            )],
            ..Explication::default()
        };
        assert!(!se_distingue(&base));
        assert!(se_distingue(&refuse));
        assert_eq!(compter_effectifs(&[refuse, base, vide]), [1, 1, 1, 3]);
    }

    #[test]
    fn resume_les_modifications_d_un_role() {
        let r = |p: &str, res: &str, e: Effet| RegleSaisie {
            permission: p.into(),
            resource: res.into(),
            effect: e,
        };
        let avant = [r("a", "*", Effet::Allow), r("b", "*", Effet::Deny)];
        let apres = [
            r("a", "*", Effet::Allow),
            r("b", "*", Effet::Allow),
            r("c", "s1", Effet::Deny),
        ];
        assert_eq!(permissions_modifiees(&avant, &apres), ["b", "c"]);
        assert_eq!(resume_modifications(&[], 0), "");
        assert_eq!(
            resume_modifications(&["A".into()], 0),
            "1 modification non enregistrée · A"
        );
        assert_eq!(
            resume_modifications(
                &["A".into(), "B".into(), "C".into(), "D".into(), "E".into()],
                1
            ),
            "6 modifications non enregistrées · A, B, C et 2 autres"
        );
    }

    #[test]
    fn range_les_permissions_par_categorie_et_lit_la_grille() {
        let p = |nom: &str, cat: &str, obsolete: bool| PermissionVue {
            name: nom.into(),
            label: nom.into(),
            category: cat.into(),
            obsolete_at: obsolete.then(|| "x".into()),
            ..PermissionVue::default()
        };
        let app = AppVue {
            key: "estompe".into(),
            name: "Estompe".into(),
            permissions: vec![
                p("a", "Salons", false),
                p("b", "", false),
                p("c", "Salons", false),
                p("d", "Salons", true),
            ],
            ..AppVue::default()
        };
        let cats = categories(Some(&app), |_| true);
        assert_eq!(
            cats.iter()
                .map(|c| (c.nom.as_str(), c.permissions.len()))
                .collect::<Vec<_>>(),
            [("Salons", 2), ("Autres", 1)]
        );
        assert!(correspond_au_filtre(&app.permissions[0], " SAL"));
        assert!(!correspond_au_filtre(&app.permissions[1], "salons"));
        let role = RoleDetail {
            rules: vec![
                RegleVue {
                    id: "1".into(),
                    permission: "a".into(),
                    resource: "*".into(),
                    effect: Effet::Deny,
                    ..RegleVue::default()
                },
                RegleVue {
                    id: "2".into(),
                    permission: "a".into(),
                    resource: "s1".into(),
                    effect: Effet::Allow,
                    ..RegleVue::default()
                },
            ],
            ..RoleDetail::default()
        };
        assert_eq!(
            cellule(Some(&role), "a"),
            Cellule {
                effet: Some(Effet::Deny),
                exceptions: 1
            }
        );
        assert_eq!(
            cellule(None, "a"),
            Cellule {
                effet: None,
                exceptions: 0
            }
        );
        assert_eq!(exceptions(2), "2 exceptions");
        let catalogue = Catalogue { apps: vec![app] };
        assert_eq!(obsoletes(&catalogue), 1);
        assert_eq!(permissions_actives(&catalogue).len(), 3);
    }

    #[test]
    fn decrit_une_ligne_du_journal() {
        let l: EntreeJournal = serde_json::from_value(json!({
            "id": 1, "at": "2026-11-20T10:00:00Z", "actor": "s-eve", "action": "rule.allow", "target_kind": "permission",
            "target_id": "estompe:gifs.use", "target_label": null, "subject": "s-bob",
            "detail": {"resource": "s1", "note": "ticket 4", "expires_at": "2026-12-01T00:00:00Z"},
            "actor_account": {"sub": "s-eve", "email": "eve@x.fr", "display_name": null, "last_seen_at": null, "in_keycloak": true},
            "subject_account": null
        }))
        .unwrap();
        let mut noms = HashMap::new();
        noms.insert(
            "estompe:gifs.use".to_string(),
            PermissionVue {
                name: "estompe:gifs.use".into(),
                app: "estompe".into(),
                label: "Utiliser les GIF".into(),
                resource_type: Some("server".into()),
                ..PermissionVue::default()
            },
        );
        assert_eq!(acteur(&l), "eve@x.fr");
        assert_eq!(cible(&l, &noms), "Utiliser les GIF");
        assert_eq!(pour_qui(&l).as_deref(), Some("s-bob"));
        assert_eq!(libelle_action(&l.action), "permission accordée");
        assert_eq!(ton_action("rule.delete"), "dgr");
        assert_eq!(ton_action("role.update"), "info");
        assert_eq!(
            extra_journal(&l, 60),
            "jusqu'au 01/12/2026 01:00 · « ticket 4 »"
        );
        assert_eq!(
            ressource_journal(&l, &noms),
            Some(RessourceJournal::Regle {
                type_: Some("estompe:server".into()),
                id: "s1".into()
            })
        );
        assert_eq!(
            chemin_ressources("estompe:server"),
            "/resources/estompe/server"
        );
        let mut n = NomsRessources::default();
        assert_eq!(n.nom(Some("estompe:server"), "s1"), "s1");
        n.retenir("estompe:server", "s1", "Les amis");
        assert_eq!(n.nom(Some("estompe:server"), "s1"), "Les amis");
        assert_eq!(n.nom(None, "*"), "partout");
    }

    #[test]
    fn annonce_une_copie_de_droits() {
        let essai = Copie {
            bindings: 2,
            groups: 1,
            rules: 0,
            removed: 4,
        };
        assert_eq!(
            message_copie(&essai, "eve@x.fr", ModeCopie::Remplacer),
            "2 rôle(s), 1 groupe(s) et 0 règle(s) de eve@x.fr seront copiés, après retrait des 4 droit(s) actuels."
        );
    }
}
