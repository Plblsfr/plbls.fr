use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const JOKER: &str = "*";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Effet {
    Allow,
    Deny,
}

impl Effet {
    pub fn depuis(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Effet::Allow),
            "deny" => Some(Effet::Deny),
            _ => None,
        }
    }

    pub fn texte(self) -> &'static str {
        match self {
            Effet::Allow => "allow",
            Effet::Deny => "deny",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    ToutLeMonde {
        role_id: String,
    },
    Role {
        role_id: String,
        role: String,
        binding_id: String,
        group_id: Option<String>,
        group: Option<String>,
        scope_type: Option<String>,
        scope_id: Option<String>,
    },
    Groupe {
        group_id: String,
        group: String,
    },
    Compte,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Brute {
    pub regle_id: String,
    pub permission: String,
    pub ressource: String,
    pub effet: Effet,
    pub type_ressource: Option<String>,
    pub expire: Option<DateTime<Utc>>,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Regle {
    pub rule_id: String,
    pub permission: String,
    pub resource: String,
    pub effect: Effet,
    pub expires_at: Option<DateTime<Utc>>,
    pub source: Source,
}

pub fn appliquer_portee(brutes: Vec<Brute>) -> Vec<Regle> {
    brutes
        .into_iter()
        .filter_map(|b| {
            let ressource = match &b.source {
                Source::Role {
                    scope_type: Some(t),
                    scope_id: Some(id),
                    ..
                } => {
                    if b.type_ressource.as_deref() != Some(t.as_str()) {
                        return None;
                    }
                    if b.ressource == JOKER || b.ressource == *id {
                        id.clone()
                    } else {
                        return None;
                    }
                }
                _ => b.ressource,
            };
            Some(Regle {
                rule_id: b.regle_id,
                permission: b.permission,
                resource: ressource,
                effect: b.effet,
                expires_at: b.expire,
                source: b.source,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Acces {
    pub allow: BTreeSet<String>,
    pub deny: BTreeSet<String>,
}

impl Acces {
    pub fn autorise(&self, ressource: &str) -> bool {
        if self.deny.contains(JOKER) || self.deny.contains(ressource) {
            return false;
        }
        self.allow.contains(JOKER) || self.allow.contains(ressource)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Effectifs {
    pub sub: String,
    pub generation: i64,
    pub valid_until: DateTime<Utc>,
    pub permissions: BTreeMap<String, Acces>,
}

impl Effectifs {
    pub fn calculer(
        sub: &str,
        generation: i64,
        regles: &[Regle],
        maintenant: DateTime<Utc>,
        ttl: chrono::Duration,
    ) -> Self {
        let mut permissions: BTreeMap<String, Acces> = BTreeMap::new();
        let mut valid_until = maintenant + ttl;
        for r in regles {
            if let Some(e) = r.expires_at {
                if e <= maintenant {
                    continue;
                }
                valid_until = valid_until.min(e);
            }
            let a = permissions.entry(r.permission.clone()).or_default();
            match r.effect {
                Effet::Allow => a.allow.insert(r.resource.clone()),
                Effet::Deny => a.deny.insert(r.resource.clone()),
            };
        }
        permissions.retain(|_, a| !a.allow.is_empty());
        Effectifs {
            sub: sub.to_string(),
            generation,
            valid_until,
            permissions,
        }
    }

    pub fn autorise(&self, permission: &str, ressource: Option<&str>) -> bool {
        self.permissions
            .get(permission)
            .is_some_and(|a| a.autorise(ressource.unwrap_or(JOKER)))
    }

    pub fn filtrer(mut self, app: Option<&str>) -> Self {
        if let Some(app) = app {
            let prefixe = format!("{app}:");
            self.permissions.retain(|k, _| k.starts_with(&prefixe));
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Etat {
    Autorise,
    Partiel,
    Refuse,
    NonDefini,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Explication {
    pub permission: String,
    pub state: Etat,
    pub allow: BTreeSet<String>,
    pub deny: BTreeSet<String>,
    pub rules: Vec<Regle>,
}

pub fn expliquer(permission: &str, regles: &[Regle], maintenant: DateTime<Utc>) -> Explication {
    let rules: Vec<Regle> = regles
        .iter()
        .filter(|r| r.permission == permission && r.expires_at.is_none_or(|e| e > maintenant))
        .cloned()
        .collect();
    let mut a = Acces::default();
    for r in &rules {
        match r.effect {
            Effet::Allow => a.allow.insert(r.resource.clone()),
            Effet::Deny => a.deny.insert(r.resource.clone()),
        };
    }
    let state = if rules.is_empty() {
        Etat::NonDefini
    } else if a.autorise(JOKER) && a.deny.is_empty() {
        Etat::Autorise
    } else if a.deny.contains(JOKER) || a.allow.is_empty() {
        Etat::Refuse
    } else {
        Etat::Partiel
    };
    Explication {
        permission: permission.to_string(),
        state,
        allow: a.allow,
        deny: a.deny,
        rules,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
    pub rules: Vec<Regle>,
}

pub fn decider(
    permission: &str,
    ressource: Option<&str>,
    regles: &[Regle],
    maintenant: DateTime<Utc>,
) -> Decision {
    let r = ressource.unwrap_or(JOKER);
    let pertinentes: Vec<Regle> = regles
        .iter()
        .filter(|g| {
            g.permission == permission
                && (g.resource == JOKER || g.resource == r)
                && g.expires_at.is_none_or(|e| e > maintenant)
        })
        .cloned()
        .collect();
    let refus: Vec<Regle> = pertinentes
        .iter()
        .filter(|g| g.effect == Effet::Deny)
        .cloned()
        .collect();
    if !refus.is_empty() {
        return Decision {
            allowed: false,
            reason: "refusé : une règle de refus s'applique (le refus l'emporte toujours)".into(),
            rules: refus,
        };
    }
    if pertinentes.is_empty() {
        return Decision {
            allowed: false,
            reason: "refusé : aucune règle n'accorde cette permission".into(),
            rules: vec![],
        };
    }
    Decision {
        allowed: true,
        reason: "autorisé".into(),
        rules: pertinentes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(perm: &str, res: &str, effet: Effet, source: Source) -> Brute {
        Brute {
            regle_id: format!("{perm}-{res}"),
            permission: perm.into(),
            ressource: res.into(),
            effet,
            type_ressource: Some("estompe:server".into()),
            expire: None,
            source,
        }
    }

    fn role_portee(scope: Option<&str>) -> Source {
        Source::Role {
            role_id: "r".into(),
            role: "Modérateur".into(),
            binding_id: "b".into(),
            group_id: None,
            group: None,
            scope_type: scope.map(|_| "estompe:server".into()),
            scope_id: scope.map(str::to_string),
        }
    }

    fn calc(regles: &[Regle]) -> Effectifs {
        Effectifs::calculer("s", 1, regles, Utc::now(), chrono::Duration::minutes(10))
    }

    #[test]
    fn le_refus_l_emporte_toujours() {
        let r = appliquer_portee(vec![
            brute("e:a", "*", Effet::Allow, Source::Compte),
            brute("e:a", "srv1", Effet::Deny, role_portee(None)),
        ]);
        let e = calc(&r);
        assert!(e.autorise("e:a", Some("srv2")));
        assert!(!e.autorise("e:a", Some("srv1")));
        let r2 = appliquer_portee(vec![
            brute("e:a", "srv1", Effet::Allow, Source::Compte),
            brute("e:a", "*", Effet::Deny, Source::Compte),
        ]);
        assert!(!calc(&r2).autorise("e:a", Some("srv1")));
    }

    #[test]
    fn rien_d_accorde_vaut_refus() {
        let e = calc(&[]);
        assert!(!e.autorise("e:a", None));
        let r = appliquer_portee(vec![brute("e:a", "*", Effet::Deny, Source::Compte)]);
        assert!(calc(&r).permissions.is_empty());
    }

    #[test]
    fn ressource_precise_ne_donne_pas_le_joker() {
        let r = appliquer_portee(vec![brute("e:a", "srv1", Effet::Allow, Source::Compte)]);
        let e = calc(&r);
        assert!(e.autorise("e:a", Some("srv1")));
        assert!(!e.autorise("e:a", None));
    }

    #[test]
    fn portee_d_une_liaison() {
        let mut autre = brute("e:b", "*", Effet::Allow, role_portee(Some("srv1")));
        autre.type_ressource = None;
        let r = appliquer_portee(vec![
            brute("e:a", "*", Effet::Allow, role_portee(Some("srv1"))),
            brute("e:c", "srv2", Effet::Allow, role_portee(Some("srv1"))),
            autre,
        ]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].resource, "srv1");
    }

    #[test]
    fn expiration_et_validite() {
        let now = Utc::now();
        let mut b = brute("e:a", "*", Effet::Allow, Source::Compte);
        b.expire = Some(now + chrono::Duration::minutes(2));
        let mut passee = brute("e:z", "*", Effet::Allow, Source::Compte);
        passee.expire = Some(now - chrono::Duration::minutes(2));
        let r = appliquer_portee(vec![b, passee]);
        let e = Effectifs::calculer("s", 1, &r, now, chrono::Duration::minutes(10));
        assert!(e.autorise("e:a", None) && !e.autorise("e:z", None));
        assert_eq!(e.valid_until, now + chrono::Duration::minutes(2));
    }

    #[test]
    fn filtre_par_application() {
        let r = appliquer_portee(vec![
            brute("estompe:a", "*", Effet::Allow, Source::Compte),
            brute("pli:deposit", "*", Effet::Allow, Source::Compte),
        ]);
        let e = calc(&r).filtrer(Some("pli"));
        assert_eq!(e.permissions.keys().collect::<Vec<_>>(), ["pli:deposit"]);
    }

    #[test]
    fn explication_et_decision() {
        let now = Utc::now();
        let r = appliquer_portee(vec![
            brute("e:a", "*", Effet::Allow, Source::Compte),
            brute("e:a", "srv1", Effet::Deny, Source::Compte),
            brute("e:b", "*", Effet::Allow, Source::Compte),
            brute("e:c", "*", Effet::Deny, Source::Compte),
        ]);
        assert_eq!(expliquer("e:a", &r, now).state, Etat::Partiel);
        assert_eq!(expliquer("e:b", &r, now).state, Etat::Autorise);
        assert_eq!(expliquer("e:c", &r, now).state, Etat::Refuse);
        assert_eq!(expliquer("e:d", &r, now).state, Etat::NonDefini);
        let d = decider("e:a", Some("srv1"), &r, now);
        assert!(!d.allowed);
        assert_eq!(d.rules.len(), 1);
        assert!(decider("e:a", Some("srv2"), &r, now).allowed);
        assert!(!decider("e:d", None, &r, now).allowed);
    }
}
