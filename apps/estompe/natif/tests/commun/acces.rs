use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::coquille::{Repondeur, ServeurMessages};

pub const PREFIXE: &str = "/admin/acces";

pub fn catalogue() -> Value {
    json!({"apps": [
        {"key": "estompe", "name": "Estompe", "description": "", "seen_at": "2026-11-18T08:00:00Z",
         "resource_types": [{"app": "estompe", "key": "server", "name": "Serveurs", "listable": true}],
         "permissions": [
            {"name": "estompe:access", "app": "estompe", "key": "access", "category": "Accès", "label": "Utiliser Estompe", "description": "Base de toutes les autres fonctionnalités", "resource_type": null, "open_default": true, "obsolete_at": null, "rules_count": 1},
            {"name": "estompe:servers.create", "app": "estompe", "key": "servers.create", "category": "Serveurs", "label": "Créer un serveur", "description": "Créer de nouveaux serveurs", "resource_type": null, "open_default": true, "obsolete_at": null, "rules_count": 1},
            {"name": "estompe:gifs.use", "app": "estompe", "key": "gifs.use", "category": "Messages", "label": "Utiliser les GIF", "description": "Chercher et envoyer des GIF", "resource_type": "server", "open_default": true, "obsolete_at": null, "rules_count": 2},
            {"name": "estompe:vocal.screen", "app": "estompe", "key": "vocal.screen", "category": "Vocal", "label": "Partager son écran", "description": "Diffuser son écran dans un salon vocal", "resource_type": "server", "open_default": false, "obsolete_at": null, "rules_count": 0},
            {"name": "estompe:ancienne", "app": "estompe", "key": "ancienne", "category": "Messages", "label": "Ancienne permission", "description": "Retirée du manifeste", "resource_type": null, "open_default": false, "obsolete_at": "2026-11-01T00:00:00Z", "rules_count": 1}
         ]},
        {"key": "aegis", "name": "Aegis", "description": "", "seen_at": "2026-11-18T08:00:00Z", "resource_types": [],
         "permissions": [
            {"name": "aegis:admin", "app": "aegis", "key": "admin", "category": "Administration", "label": "Administrer les accès", "description": "Toutes les permissions", "resource_type": null, "open_default": false, "obsolete_at": null, "rules_count": 1}
         ]}
    ]})
}

fn compte(sub: &str, email: Option<&str>, nom: Option<&str>, keycloak: bool) -> Value {
    json!({"sub": sub, "email": email, "display_name": nom, "last_seen_at": "2026-11-19T09:15:00Z", "in_keycloak": keycloak})
}

pub fn comptes_de_base() -> Vec<Value> {
    vec![
        json!({"sub": "s-alice", "email": "alice@exemple.fr", "display_name": "Alice Martin", "last_seen_at": "2026-11-19T09:15:00Z", "in_keycloak": true, "roles_count": 1, "groups_count": 1, "allows_count": 0, "denies_count": 0}),
        json!({"sub": "s-bob", "email": "bob@exemple.fr", "display_name": null, "last_seen_at": "2026-11-18T17:40:00Z", "in_keycloak": true, "roles_count": 0, "groups_count": 0, "allows_count": 0, "denies_count": 1}),
        json!({"sub": "s-carol", "email": null, "display_name": "Carol", "last_seen_at": null, "in_keycloak": false, "roles_count": 0, "groups_count": 0, "allows_count": 0, "denies_count": 0}),
    ]
}

pub fn comptes_nombreux(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            json!({"sub": format!("s-{i:05}"), "email": format!("compte{i:05}@exemple.fr"), "display_name": null,
                   "last_seen_at": "2026-11-19T09:15:00Z", "in_keycloak": true, "roles_count": i % 3, "groups_count": i % 2, "allows_count": 0, "denies_count": 0})
        })
        .collect()
}

pub fn roles() -> Vec<Value> {
    vec![
        json!({"id": "r-tous", "name": "Tout le monde", "description": "Appliqué à chaque compte connecté", "system": "tout-le-monde", "created_at": "2026-01-01T00:00:00Z", "rules_count": 2, "bindings_count": 0, "accounts_count": 3,
               "rules": [
                  {"id": "rr1", "subject_kind": "role", "subject_id": "r-tous", "permission": "estompe:access", "resource": "*", "effect": "allow", "expires_at": null, "note": "", "granted_by": "aegis", "created_at": "2026-01-01T00:00:00Z", "obsolete": false},
                  {"id": "rr2", "subject_kind": "role", "subject_id": "r-tous", "permission": "estompe:gifs.use", "resource": "*", "effect": "allow", "expires_at": null, "note": "", "granted_by": "aegis", "created_at": "2026-01-01T00:00:00Z", "obsolete": false}
               ], "bindings": []}),
        json!({"id": "r-admin", "name": "Administrateur", "description": "", "system": "admin", "created_at": "2026-01-01T00:00:00Z", "rules_count": 1, "bindings_count": 1, "accounts_count": 1,
               "rules": [{"id": "rr3", "subject_kind": "role", "subject_id": "r-admin", "permission": "aegis:admin", "resource": "*", "effect": "allow", "expires_at": null, "note": "", "granted_by": "aegis", "created_at": "2026-01-01T00:00:00Z", "obsolete": false}],
               "bindings": [{"id": "b-adm", "role_id": "r-admin", "role": "Administrateur", "subject_kind": "account", "subject_id": "s-alice", "subject_label": "alice@exemple.fr", "scope_type": null, "scope_id": null, "expires_at": null, "granted_by": "aegis", "created_at": "2026-02-01T10:00:00Z"}]}),
        json!({"id": "r-modo", "name": "Modération", "description": "Modérer les salons", "system": null, "created_at": "2026-03-01T00:00:00Z", "rules_count": 3, "bindings_count": 1, "accounts_count": 2,
               "rules": [
                  {"id": "rr4", "subject_kind": "role", "subject_id": "r-modo", "permission": "estompe:servers.create", "resource": "*", "effect": "deny", "expires_at": null, "note": "", "granted_by": "s-alice", "created_at": "2026-03-01T00:00:00Z", "obsolete": false},
                  {"id": "rr5", "subject_kind": "role", "subject_id": "r-modo", "permission": "estompe:vocal.screen", "resource": "*", "effect": "allow", "expires_at": null, "note": "", "granted_by": "s-alice", "created_at": "2026-03-01T00:00:00Z", "obsolete": false},
                  {"id": "rr6", "subject_kind": "role", "subject_id": "r-modo", "permission": "estompe:vocal.screen", "resource": "s9", "effect": "deny", "expires_at": null, "note": "", "granted_by": "s-alice", "created_at": "2026-03-01T00:00:00Z", "obsolete": false}
               ],
               "bindings": [{"id": "b-modo", "role_id": "r-modo", "role": "Modération", "subject_kind": "group", "subject_id": "g-equipe", "subject_label": "Équipe", "scope_type": "estompe:server", "scope_id": "s1", "expires_at": "2026-12-31T23:00:00Z", "granted_by": "s-alice", "created_at": "2026-03-02T10:00:00Z"}]}),
    ]
}

pub fn groupes() -> Value {
    json!([
        {"id": "g-equipe", "name": "Équipe", "description": "", "created_at": "2026-03-01T00:00:00Z", "members_count": 2, "roles_count": 1},
        {"id": "g-invites", "name": "Invités", "description": "", "created_at": "2026-04-01T00:00:00Z", "members_count": 0, "roles_count": 0}
    ])
}

pub fn groupe_equipe() -> Value {
    json!({"id": "g-equipe", "name": "Équipe", "description": "", "created_at": "2026-03-01T00:00:00Z", "members_count": 2, "roles_count": 1,
           "members": [
              {"account": compte("s-alice", Some("alice@exemple.fr"), Some("Alice Martin"), true), "expires_at": null, "added_by": "s-alice", "added_at": "2026-03-01T10:00:00Z"},
              {"account": compte("s-carol", None, Some("Carol"), false), "expires_at": "2027-01-15T12:00:00Z", "added_by": "s-alice", "added_at": "2026-05-04T08:30:00Z"}
           ],
           "bindings": [{"id": "b-modo", "role_id": "r-modo", "role": "Modération", "subject_kind": "group", "subject_id": "g-equipe", "subject_label": "Équipe", "scope_type": "estompe:server", "scope_id": "s1", "expires_at": "2026-12-31T23:00:00Z", "granted_by": "s-alice", "created_at": "2026-03-02T10:00:00Z"}],
           "rules": [{"id": "rg1", "subject_kind": "group", "subject_id": "g-equipe", "permission": "estompe:servers.create", "resource": "*", "effect": "allow", "expires_at": null, "note": "ticket 12", "granted_by": "s-alice", "created_at": "2026-03-03T10:00:00Z", "obsolete": false}]})
}

pub fn ressources() -> Value {
    json!([{"id": "s1", "name": "Les amis"}, {"id": "s9", "name": "Atelier"}])
}

#[derive(Default)]
struct Etat {
    comptes: Vec<Value>,
    regles: Vec<Value>,
    journal: Vec<Value>,
    prochain: u64,
    refus: Vec<String>,
}

#[derive(Clone)]
pub struct AegisFactice {
    etat: Arc<Mutex<Etat>>,
}

fn decoder(texte: &str) -> String {
    let octets = texte.as_bytes();
    let mut sortie = Vec::new();
    let mut i = 0;
    while i < octets.len() {
        match octets[i] {
            b'%' if i + 2 < octets.len() => {
                let hex = std::str::from_utf8(&octets[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(o) => {
                        sortie.push(o);
                        i += 3;
                    }
                    Err(_) => {
                        sortie.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                sortie.push(b' ');
                i += 1;
            }
            o => {
                sortie.push(o);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

fn parametre(chemin: &str, nom: &str) -> Option<String> {
    let (_, requete) = chemin.split_once('?')?;
    requete.split('&').find_map(|p| {
        let (cle, valeur) = p.split_once('=')?;
        (cle == nom).then(|| decoder(valeur))
    })
}

pub fn journal_initial() -> Vec<Value> {
    vec![
        json!({"id": 2, "at": "2026-11-18T14:05:00Z", "actor": "s-alice", "action": "binding.create", "target_kind": "role", "target_id": "r-modo", "target_label": "Modération", "subject": null,
               "detail": {"scope_type": "estompe:server", "scope_id": "s1", "expires_at": "2026-12-31T23:00:00Z"},
               "actor_account": compte("s-alice", Some("alice@exemple.fr"), Some("Alice Martin"), true), "subject_account": null}),
        json!({"id": 1, "at": "2026-11-17T09:00:00Z", "actor": "manifeste:estompe", "action": "manifest", "target_kind": "app", "target_id": "estompe", "target_label": "Estompe", "subject": null,
               "detail": {"added": ["estompe:vocal.screen"], "removed": ["estompe:ancienne"]}, "actor_account": null, "subject_account": null}),
    ]
}

pub fn regle_directe_de_bob() -> Value {
    json!({"id": "rb1", "subject_kind": "account", "subject_id": "s-bob", "permission": "estompe:servers.create", "resource": "*", "effect": "deny", "expires_at": null, "note": "abus", "granted_by": "s-alice", "created_at": "2026-11-10T10:00:00Z", "obsolete": false})
}

impl AegisFactice {
    pub fn nouveau(comptes: Vec<Value>) -> Self {
        Self {
            etat: Arc::new(Mutex::new(Etat {
                comptes,
                regles: vec![regle_directe_de_bob()],
                journal: journal_initial(),
                prochain: 100,
                refus: Vec::new(),
            })),
        }
    }

    pub fn refuser(&self, debut: &str, refuse: bool) {
        let mut e = self.etat.lock().unwrap();
        e.refus.retain(|r| r != debut);
        if refuse {
            e.refus.push(debut.to_string());
        }
    }

    pub fn brancher(&self, serveur: &ServeurMessages) {
        let aegis = self.clone();
        let repondeur: Repondeur = Arc::new(move |methode, chemin, corps| {
            let reste = chemin.strip_prefix(PREFIXE)?;
            Some(aegis.repondre(methode, reste, corps))
        });
        *serveur.repondeur.lock().unwrap() = Some(repondeur);
    }

    pub fn detail(&self, sub: &str) -> Value {
        let e = self.etat.lock().unwrap();
        detail_de(&e, sub)
    }

    pub fn effectifs(&self, sub: &str) -> Value {
        let e = self.etat.lock().unwrap();
        effectifs_de(&e, sub)
    }

    fn repondre(&self, methode: &str, chemin: &str, corps: &str) -> (u16, String) {
        let mut e = self.etat.lock().unwrap();
        if e.refus.iter().any(|r| chemin.starts_with(r.as_str())) {
            return (
                503,
                json!({"error": "permission_unavailable", "message": "aegis est injoignable"})
                    .to_string(),
            );
        }
        let base = chemin.split('?').next().unwrap_or(chemin);
        let morceaux: Vec<&str> = base.trim_start_matches('/').split('/').collect();
        let recu: Value = serde_json::from_str(corps).unwrap_or_default();
        let ok = |v: Value| (200, v.to_string());
        match (methode, morceaux.as_slice()) {
            ("GET", ["catalog"]) => ok(catalogue()),
            ("GET", ["accounts"]) => {
                let q = parametre(chemin, "q").unwrap_or_default().to_lowercase();
                let limite: usize = parametre(chemin, "limit")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(50)
                    .min(200);
                let decalage: usize = parametre(chemin, "offset")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(0);
                let trouves: Vec<&Value> = e
                    .comptes
                    .iter()
                    .filter(|c| q.is_empty() || c.to_string().to_lowercase().contains(&q))
                    .collect();
                let page: Vec<&Value> = trouves
                    .iter()
                    .skip(decalage)
                    .take(limite)
                    .copied()
                    .collect();
                ok(json!({"accounts": page, "total": trouves.len(), "from_keycloak": 0}))
            }
            ("GET", ["accounts", sub]) => ok(detail_de(&e, sub)),
            ("GET", ["accounts", sub, "effective"]) => ok(effectifs_de(&e, sub)),
            ("POST", ["simulate"]) => {
                let refus = recu["resource"].as_str() == Some("s9");
                ok(
                    json!({"allowed": !refus, "reason": if refus { "refusé par le rôle Modération sur Atelier" } else { "accordé par Tout le monde" }, "rules": []}),
                )
            }
            ("GET", ["roles"]) => ok(Value::Array(
                roles()
                    .into_iter()
                    .map(|mut r| {
                        let o = r.as_object_mut().unwrap();
                        o.remove("rules");
                        o.remove("bindings");
                        r
                    })
                    .collect(),
            )),
            ("GET", ["roles", id]) => match roles().into_iter().find(|r| r["id"] == *id) {
                Some(r) => ok(r),
                None => (404, json!({"error": "not_found"}).to_string()),
            },
            ("PUT", ["roles", id]) => {
                let mut r = roles()
                    .into_iter()
                    .find(|r| r["id"] == *id)
                    .unwrap_or_default();
                r["name"] = recu["name"].clone();
                ok(r)
            }
            ("GET", ["groups"]) => ok(groupes()),
            ("GET", ["groups", "g-equipe"]) => ok(groupe_equipe()),
            ("GET", ["resources", "estompe", "server"]) => {
                let q = parametre(chemin, "q").unwrap_or_default().to_lowercase();
                let ids = parametre(chemin, "ids");
                let l: Vec<Value> = ressources()
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| {
                        q.is_empty() || r["name"].as_str().unwrap().to_lowercase().contains(&q)
                    })
                    .filter(|r| {
                        ids.as_ref()
                            .is_none_or(|ids| ids.split(',').any(|i| r["id"] == i))
                    })
                    .cloned()
                    .collect();
                ok(Value::Array(l))
            }
            ("GET", ["permissions", _, "holders"]) => ok(json!([
                {"rule": roles()[0]["rules"][1].clone(), "subject_label": "Tout le monde", "account": null},
                {"rule": e.regles.first().cloned().unwrap_or_default(), "subject_label": "bob@exemple.fr", "account": null}
            ])),
            ("POST", ["rules"]) => {
                e.prochain += 1;
                let id = format!("r{}", e.prochain);
                let regle = json!({"id": id, "subject_kind": recu["subject_kind"], "subject_id": recu["subject_id"], "permission": recu["permission"],
                    "resource": recu["resource"], "effect": recu["effect"], "expires_at": recu["expires_at"], "note": recu["note"],
                    "granted_by": "s-moi", "created_at": "2026-11-19T12:00:00Z", "obsolete": false});
                e.regles.push(regle.clone());
                let sujet = recu["subject_id"].as_str().unwrap_or_default().to_string();
                let compte_sujet = e
                    .comptes
                    .iter()
                    .find(|c| c["sub"] == sujet.as_str())
                    .cloned();
                let n = e.prochain;
                e.journal.insert(0, json!({"id": n, "at": "2026-11-19T12:00:00Z", "actor": "s-moi", "action": format!("rule.{}", recu["effect"].as_str().unwrap_or("allow")),
                    "target_kind": "permission", "target_id": recu["permission"], "target_label": null, "subject": sujet,
                    "detail": {"resource": recu["resource"], "note": recu["note"], "expires_at": recu["expires_at"]},
                    "actor_account": compte("s-moi", Some("moi@exemple.fr"), None, true), "subject_account": compte_sujet}));
                ok(regle)
            }
            ("DELETE", ["rules", id]) => {
                e.regles.retain(|r| r["id"] != *id);
                ok(json!({}))
            }
            ("POST", ["bindings"])
            | ("PUT", ["groups", _, "members", _])
            | ("DELETE", ["groups", _, "members", _])
            | ("DELETE", ["bindings", _]) => ok(json!({})),
            ("GET", ["audit"]) => {
                let action = parametre(chemin, "action");
                let sujet = parametre(chemin, "subject");
                let l: Vec<Value> = e
                    .journal
                    .iter()
                    .filter(|l| action.as_ref().is_none_or(|a| l["action"] == a.as_str()))
                    .filter(|l| sujet.as_ref().is_none_or(|s| l["subject"] == s.as_str()))
                    .cloned()
                    .collect();
                ok(Value::Array(l))
            }
            _ => (
                404,
                json!({"error": "not_found", "message": format!("{methode} {chemin}")}).to_string(),
            ),
        }
    }
}

fn regles_de<'a>(e: &'a Etat, sub: &str) -> Vec<&'a Value> {
    e.regles.iter().filter(|r| r["subject_id"] == sub).collect()
}

fn detail_de(e: &Etat, sub: &str) -> Value {
    let c = e
        .comptes
        .iter()
        .find(|c| c["sub"] == sub)
        .cloned()
        .unwrap_or_else(|| compte(sub, None, None, true));
    let account = json!({"sub": c["sub"], "email": c["email"], "display_name": c["display_name"], "last_seen_at": c["last_seen_at"], "in_keycloak": c["in_keycloak"]});
    let (bindings, groups, group_bindings) = if sub == "s-alice" {
        (
            json!([{"id": "b-adm", "role_id": "r-admin", "role": "Administrateur", "subject_kind": "account", "subject_id": "s-alice", "subject_label": "alice@exemple.fr", "scope_type": null, "scope_id": null, "expires_at": null, "granted_by": "aegis", "created_at": "2026-02-01T10:00:00Z"}]),
            json!([{"group_id": "g-equipe", "name": "Équipe", "expires_at": null, "added_by": "s-alice", "added_at": "2026-03-01T10:00:00Z"}]),
            groupe_equipe()["bindings"].clone(),
        )
    } else {
        (json!([]), json!([]), json!([]))
    };
    json!({"account": account, "bindings": bindings, "groups": groups, "rules": regles_de(e, sub), "group_bindings": group_bindings})
}

fn effectifs_de(e: &Etat, sub: &str) -> Value {
    let tout = |p: &str| json!({"rule_id": "rr", "permission": p, "resource": "*", "effect": "allow", "expires_at": null, "source": {"kind": "tout_le_monde", "role_id": "r-tous"}});
    let mut permissions = vec![
        json!({"permission": "estompe:access", "state": "autorise", "allow": ["*"], "deny": [], "rules": [tout("estompe:access")]}),
        json!({"permission": "aegis:admin", "state": "non_defini", "allow": [], "deny": [], "rules": []}),
    ];
    let directes = regles_de(e, sub);
    let par_compte = |p: &str| -> Vec<Value> {
        directes
            .iter()
            .filter(|r| r["permission"] == p)
            .map(|r| json!({"rule_id": r["id"], "permission": p, "resource": r["resource"], "effect": r["effect"], "expires_at": r["expires_at"], "source": {"kind": "compte"}}))
            .collect()
    };
    let mut gifs = vec![tout("estompe:gifs.use")];
    gifs.extend(par_compte("estompe:gifs.use"));
    let refus_gifs: Vec<Value> = gifs
        .iter()
        .filter(|r| r["effect"] == "deny")
        .map(|r| r["resource"].clone())
        .collect();
    permissions.push(json!({"permission": "estompe:gifs.use", "state": if refus_gifs.is_empty() { "autorise" } else { "partiel" }, "allow": ["*"], "deny": refus_gifs, "rules": gifs}));
    let creer = par_compte("estompe:servers.create");
    let refuse = creer.iter().any(|r| r["effect"] == "deny");
    let mut regles_creer = vec![tout("estompe:servers.create")];
    regles_creer.extend(creer);
    permissions.push(json!({"permission": "estompe:servers.create", "state": if refuse { "refuse" } else { "autorise" }, "allow": if refuse { json!([]) } else { json!(["*"]) }, "deny": if refuse { json!(["*"]) } else { json!([]) }, "rules": regles_creer}));
    if sub == "s-alice" {
        permissions.push(json!({"permission": "estompe:vocal.screen", "state": "partiel", "allow": ["s1"], "deny": ["s9"], "rules": [
            {"rule_id": "rr5", "permission": "estompe:vocal.screen", "resource": "s1", "effect": "allow", "expires_at": "2026-12-31T23:00:00Z", "source": {"kind": "role", "role_id": "r-modo", "role": "Modération", "binding_id": "b-modo", "group_id": "g-equipe", "group": "Équipe", "scope_type": "estompe:server", "scope_id": "s1"}},
            {"rule_id": "rr6", "permission": "estompe:vocal.screen", "resource": "s9", "effect": "deny", "expires_at": null, "source": {"kind": "groupe", "group_id": "g-equipe", "group": "Équipe"}}
        ]}));
    } else {
        permissions.push(json!({"permission": "estompe:vocal.screen", "state": "non_defini", "allow": [], "deny": [], "rules": []}));
    }
    json!({"account": detail_de(e, sub)["account"], "permissions": permissions})
}
