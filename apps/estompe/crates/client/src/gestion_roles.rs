use std::collections::BTreeSet;

use crate::emoji::replier;
use crate::reglages_serveur::proprietaire_de;
use crate::roles::{
    nom_du_membre, position_la_plus_haute, Detail, GroupePermissions, Membre, Rang,
    GROUPES_PERMISSIONS,
};
use crate::store::{id_de, texte_du_genre, Etat, GenreSalon};
use crate::types::{Override, Role, RoleInput};

pub const MAX_NOM_ROLE: usize = 30;
pub const MAX_ICONE_ROLE: usize = 8;
pub const MAX_ROLES: usize = 50;
pub const MEMBRES_PROPOSES: usize = 6;
pub const NOUVEAU_ROLE: &str = "Nouveau rôle";
pub const COULEUR_PAR_DEFAUT: &str = "#99aab5";
pub const COULEURS_ROLE: [&str; 12] = [
    "#99aab5", "#1abc9c", "#2ecc71", "#3498db", "#9b59b6", "#e91e63", "#f1c40f", "#e67e22",
    "#e74c3c", "#2f5d8a", "#b0413e", "#607d8b",
];
pub const NOM_INVALIDE: &str = "Le nom doit faire de 1 à 30 caractères";
pub const COULEUR_INVALIDE: &str = "Couleur attendue au format #rrggbb";
pub const ICONE_INVALIDE: &str = "Icône : 8 caractères au plus";

pub struct Contexte<'a> {
    pub detail: &'a Detail,
    pub moi: &'a str,
    pub proprietaire: Option<String>,
}

fn position(role: &Role) -> i64 {
    role.position.unwrap_or(0)
}

fn est_defaut(role: &Role) -> bool {
    role.is_default == Some(true)
}

impl<'a> Contexte<'a> {
    pub fn depuis(etat: &'a Etat, serveur: &str) -> Option<Self> {
        Some(Self {
            detail: etat.details.get(serveur)?,
            moi: etat.moi(),
            proprietaire: proprietaire_de(etat, serveur),
        })
    }

    pub fn createur(&self) -> bool {
        self.proprietaire.as_deref() == Some(self.moi)
    }

    fn membre(&self, id: &str) -> Option<&'a Membre> {
        self.detail
            .members
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|m| id_de(&m.id) == id)
    }

    pub fn rang_de(&self, id: &str) -> Rang {
        if self.proprietaire.as_deref() == Some(id) {
            return Rang::Createur;
        }
        Rang::Position(position_la_plus_haute(Some(self.detail), self.membre(id)))
    }

    pub fn mon_rang(&self) -> Rang {
        self.rang_de(self.moi)
    }

    pub fn roles(&self) -> &'a [Role] {
        self.detail.roles.as_deref().unwrap_or_default()
    }

    pub fn role(&self, id: &str) -> Option<&'a Role> {
        self.roles().iter().find(|r| id_de(&r.id) == id)
    }

    pub fn roles_ordonnes(&self) -> Vec<&'a Role> {
        let mut liste: Vec<&Role> = self.roles().iter().filter(|r| !est_defaut(r)).collect();
        liste.sort_by_key(|r| std::cmp::Reverse(position(r)));
        liste
    }

    pub fn role_par_defaut(&self) -> Option<&'a Role> {
        self.roles().iter().rfind(|r| est_defaut(r))
    }

    fn sous_mon_rang(&self, position: i64) -> bool {
        Rang::Position(position) < self.mon_rang()
    }

    pub fn role_modifiable(&self, role: &Role) -> bool {
        est_defaut(role) || self.sous_mon_rang(position(role))
    }

    pub fn role_deplacable(&self, role: &Role) -> bool {
        !est_defaut(role) && self.sous_mon_rang(position(role))
    }

    pub fn permission_accordable(&self, cle: &str) -> bool {
        self.createur()
            || self
                .detail
                .permissions
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|p| p == cle)
    }

    pub fn membre_gerable(&self, id: &str) -> bool {
        self.createur() || self.mon_rang() > self.rang_de(id)
    }

    pub fn membres_du_role(&self, role: &str) -> Vec<&'a Membre> {
        let mut liste: Vec<&Membre> = self
            .detail
            .members
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|m| {
                m.roles
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|r| r == role)
            })
            .collect();
        liste.sort_by_key(|m| replier(nom_du_membre(m)));
        liste
    }

    pub fn membres_proposes(&self, role: &str, recherche: &str) -> Vec<&'a Membre> {
        let cherche = replier(recherche.trim());
        if cherche.is_empty() {
            return Vec::new();
        }
        self.detail
            .members
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|m| {
                !m.roles
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|r| r == role)
            })
            .filter(|m| self.membre_gerable(id_de(&m.id)))
            .filter(|m| {
                replier(nom_du_membre(m)).contains(&cherche)
                    || replier(m.pseudo.as_deref().unwrap_or_default()).contains(&cherche)
            })
            .take(MEMBRES_PROPOSES)
            .collect()
    }

    pub fn nouvel_ordre(&self, de: usize, vers: usize) -> Option<Vec<String>> {
        let liste = self.roles_ordonnes();
        if de == vers
            || de >= liste.len()
            || vers >= liste.len()
            || !self.role_deplacable(liste[de])
            || !self.role_deplacable(liste[vers])
        {
            return None;
        }
        let mut ids: Vec<String> = liste.iter().map(|r| id_de(&r.id).to_string()).collect();
        let id = ids.remove(de);
        ids.insert(vers, id);
        Some(ids)
    }

    fn mes_roles(&self) -> Vec<&'a Role> {
        let siens = self
            .membre(self.moi)
            .and_then(|m| m.roles.as_deref())
            .unwrap_or_default();
        self.roles()
            .iter()
            .filter(|r| est_defaut(r) || siens.iter().any(|s| s == id_de(&r.id)))
            .collect()
    }

    fn base(&self, remplacement: Option<(&str, Option<&[String]>)>) -> BTreeSet<String> {
        let mut base = BTreeSet::new();
        for r in self.mes_roles() {
            let permissions = match remplacement {
                Some((id, apres)) if id == id_de(&r.id) => apres,
                _ => r.permissions.as_deref(),
            };
            base.extend(permissions.unwrap_or_default().iter().cloned());
        }
        base
    }

    pub fn pertes(&self, role: &str, apres: Option<&[String]>) -> Vec<&'static str> {
        if self.createur() {
            return Vec::new();
        }
        let avant = self.base(None);
        let ensuite = self.base(Some((role, apres)));
        libelles(avant.difference(&ensuite))
    }

    pub fn pertes_si_retire(&self, role: &str, membre: &str) -> Vec<&'static str> {
        if membre != self.moi {
            return Vec::new();
        }
        self.pertes(role, None)
    }

    fn dans_salon(&self, surcharges: &[Override]) -> BTreeSet<String> {
        let roles = self.mes_roles();
        let tous = self.role_par_defaut().map(|r| id_de(&r.id)).unwrap_or("");
        let mien = |id: &str| id != tous && roles.iter().any(|r| id_de(&r.id) == id);
        let mut p = self.base(None);
        let (mut autorise, mut refuse) = (BTreeSet::new(), BTreeSet::new());
        for o in surcharges {
            let id = id_de(&o.role_id);
            let a = o.allow.as_deref().unwrap_or_default();
            let r = o.deny.as_deref().unwrap_or_default();
            if mien(id) {
                autorise.extend(a.iter().cloned());
                refuse.extend(r.iter().cloned());
            } else if id == tous {
                p.retain(|x| !r.contains(x));
                p.extend(a.iter().cloned());
            }
        }
        p.retain(|x| !refuse.contains(x));
        p.extend(autorise);
        if p.contains("view_channel") {
            p
        } else {
            BTreeSet::new()
        }
    }

    pub fn pertes_dans_salon(&self, avant: &[Override], apres: &[Override]) -> Vec<&'static str> {
        if self.createur() {
            return Vec::new();
        }
        let a = self.dans_salon(avant);
        let b = self.dans_salon(apres);
        libelles(a.difference(&b))
    }
}

fn libelles<'b>(cles: impl Iterator<Item = &'b String>) -> Vec<&'static str> {
    let cles: Vec<&String> = cles.collect();
    GROUPES_PERMISSIONS
        .iter()
        .flat_map(|g| g.permissions.iter())
        .filter(|p| cles.iter().any(|c| *c == p.cle))
        .map(|p| p.libelle)
        .collect()
}

pub fn message_de_perte(pertes: &[&str]) -> String {
    format!(
        "Tu vas perdre {} : {}. Tu ne pourras peut-être pas revenir en arrière.",
        if pertes.len() > 1 {
            "ces permissions"
        } else {
            "cette permission"
        },
        pertes.join(", ")
    )
}

pub fn couleur_valide(couleur: &str) -> bool {
    couleur.is_empty()
        || (couleur.len() == 7
            && couleur.starts_with('#')
            && couleur[1..].bytes().all(|o| o.is_ascii_hexdigit()))
}

pub fn nom_de_role(saisie: &str) -> String {
    saisie.chars().take(MAX_NOM_ROLE).collect()
}

pub fn icone_de_role(saisie: &str) -> String {
    saisie.chars().take(MAX_ICONE_ROLE).collect()
}

pub fn libelle_du_role(role: &Role) -> String {
    format!(
        "{} {}",
        role.icon.as_deref().unwrap_or_default(),
        role.name.as_deref().unwrap_or_default()
    )
    .trim()
    .to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrouillonRole {
    pub nom: String,
    pub couleur: String,
    pub icone: String,
    pub separe: bool,
    pub permissions: Vec<String>,
}

impl BrouillonRole {
    pub fn depuis(role: &Role) -> Self {
        Self {
            nom: role.name.clone().unwrap_or_default(),
            couleur: role.color.clone().unwrap_or_default(),
            icone: role.icon.clone().unwrap_or_default(),
            separe: role.hoist == Some(true),
            permissions: role.permissions.clone().unwrap_or_default(),
        }
    }

    pub fn nouveau() -> Self {
        Self {
            nom: NOUVEAU_ROLE.to_string(),
            ..Self::default()
        }
    }

    pub fn a(&self, cle: &str) -> bool {
        self.permissions.iter().any(|p| p == cle)
    }

    pub fn basculer(&mut self, cle: &str, actif: bool) {
        self.permissions.retain(|p| p != cle);
        if actif {
            self.permissions.push(cle.to_string());
        }
    }

    pub fn erreur(&self) -> Option<&'static str> {
        let nom = self.nom.trim();
        if nom.is_empty() || nom.chars().count() > MAX_NOM_ROLE {
            Some(NOM_INVALIDE)
        } else if !couleur_valide(&self.couleur) {
            Some(COULEUR_INVALIDE)
        } else if self.icone.chars().count() > MAX_ICONE_ROLE {
            Some(ICONE_INVALIDE)
        } else {
            None
        }
    }

    pub fn requete(&self) -> RoleInput {
        RoleInput {
            name: self.nom.trim().to_string(),
            color: Some(self.couleur.to_lowercase()),
            icon: Some(self.icone.trim().to_string()),
            hoist: Some(self.separe),
            permissions: Some(self.permissions_ordonnees()),
        }
    }

    pub fn permissions_ordonnees(&self) -> Vec<String> {
        GROUPES_PERMISSIONS
            .iter()
            .flat_map(|g| g.permissions.iter())
            .filter(|p| self.a(p.cle))
            .map(|p| p.cle.to_string())
            .collect()
    }

    pub fn differe(&self, role: &Role) -> bool {
        let initial = Self::depuis(role);
        self.nom.trim() != initial.nom
            || self.couleur.to_lowercase() != initial.couleur.to_lowercase()
            || self.icone.trim() != initial.icone
            || self.separe != initial.separe
            || self.permissions.iter().collect::<BTreeSet<_>>()
                != initial.permissions.iter().collect::<BTreeSet<_>>()
    }
}

pub fn permissions_filtrees(
    filtre: &str,
) -> Vec<(&'static str, Vec<&'static crate::roles::InfoPermission>)> {
    let cherche = replier(filtre.trim());
    GROUPES_PERMISSIONS
        .iter()
        .map(|g| {
            (
                g.titre,
                g.permissions
                    .iter()
                    .filter(|p| {
                        cherche.is_empty()
                            || replier(&format!("{} {}", p.libelle, p.description))
                                .contains(&cherche)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, p)| !p.is_empty())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatPermission {
    Herite,
    Autorise,
    Refuse,
}

impl EtatPermission {
    pub fn rang(self) -> i32 {
        match self {
            Self::Herite => 0,
            Self::Autorise => 1,
            Self::Refuse => 2,
        }
    }

    pub fn depuis_rang(rang: i32) -> Self {
        match rang {
            1 => Self::Autorise,
            2 => Self::Refuse,
            _ => Self::Herite,
        }
    }
}

pub fn surcharge_du_role<'b>(surcharges: &'b [Override], role: &str) -> Option<&'b Override> {
    surcharges.iter().find(|o| id_de(&o.role_id) == role)
}

pub fn etat_de(surcharge: Option<&Override>, cle: &str) -> EtatPermission {
    let contient = |l: Option<&Vec<String>>| l.is_some_and(|l| l.iter().any(|p| p == cle));
    match surcharge {
        Some(o) if contient(o.allow.as_ref()) => EtatPermission::Autorise,
        Some(o) if contient(o.deny.as_ref()) => EtatPermission::Refuse,
        _ => EtatPermission::Herite,
    }
}

pub fn surcharge_changee(
    surcharge: Option<&Override>,
    role: &str,
    cle: &str,
    etat: EtatPermission,
) -> Override {
    let garder = |l: Option<&Vec<String>>| -> Vec<String> {
        l.into_iter()
            .flatten()
            .filter(|p| *p != cle)
            .cloned()
            .collect()
    };
    let mut allow = garder(surcharge.and_then(|o| o.allow.as_ref()));
    let mut deny = garder(surcharge.and_then(|o| o.deny.as_ref()));
    match etat {
        EtatPermission::Autorise => allow.push(cle.to_string()),
        EtatPermission::Refuse => deny.push(cle.to_string()),
        EtatPermission::Herite => {}
    }
    Override {
        role_id: Some(role.to_string()),
        allow: Some(allow),
        deny: Some(deny),
    }
}

pub fn surcharges_apres(surcharges: &[Override], nouvelle: &Override) -> Vec<Override> {
    let role = id_de(&nouvelle.role_id);
    surcharges
        .iter()
        .filter(|o| id_de(&o.role_id) != role)
        .cloned()
        .chain(std::iter::once(nouvelle.clone()))
        .collect()
}

pub fn groupes_du_salon(genre: Option<GenreSalon>) -> Vec<&'static GroupePermissions> {
    let titres: &[&str] = match texte_du_genre(genre) {
        "voice" | "cinema" => &["Salons vocaux"],
        "videos" | "music" => &["Salons texte", "Esquisses"],
        _ => &["Salons texte"],
    };
    GROUPES_PERMISSIONS
        .iter()
        .filter(|g| titres.contains(&g.titre))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Server;

    fn role(id: &str, position: i64, permissions: &[&str], defaut: bool) -> Role {
        Role {
            id: Some(id.into()),
            name: Some(id.into()),
            color: Some(String::new()),
            icon: Some(String::new()),
            position: Some(position),
            hoist: Some(false),
            permissions: Some(permissions.iter().map(|p| p.to_string()).collect()),
            is_default: Some(defaut),
        }
    }

    fn m(id: &str, roles: &[&str]) -> Membre {
        Membre {
            id: Some(id.into()),
            pseudo: Some(id.into()),
            roles: Some(roles.iter().map(|r| r.to_string()).collect()),
            ..Membre::default()
        }
    }

    fn detail(mes_permissions: &[&str]) -> Detail {
        Detail {
            server: Some(Server {
                id: Some("s".into()),
                name: Some("s".into()),
                owner_id: Some("o".into()),
                icon_version: Some(0),
            }),
            channels: Some(vec![]),
            members: Some(vec![
                m("o", &[]),
                m("a", &["admin"]),
                m("b", &["dj"]),
                m("c", &["dj"]),
                m("d", &["ami"]),
                m("Élise", &[]),
            ]),
            roles: Some(vec![
                role(
                    "admin",
                    3,
                    &["manage_roles", "manage_channels", "kick_members"],
                    false,
                ),
                role("dj", 2, &["speak", "manage_roles"], false),
                role("ami", 1, &[], false),
                role("e", 0, &["view_channel", "send_messages"], true),
            ]),
            permissions: Some(mes_permissions.iter().map(|p| p.to_string()).collect()),
        }
    }

    fn ctx<'a>(d: &'a Detail, moi: &'a str) -> Contexte<'a> {
        Contexte {
            detail: d,
            moi,
            proprietaire: Some("o".into()),
        }
    }

    fn ids(roles: Vec<&Role>) -> Vec<String> {
        roles.iter().map(|r| id_de(&r.id).to_string()).collect()
    }

    #[test]
    fn ordonne_les_roles_du_plus_eleve_au_plus_bas_sans_tout_le_monde() {
        let d = detail(&[]);
        let c = ctx(&d, "a");
        assert_eq!(ids(c.roles_ordonnes()), ["admin", "dj", "ami"]);
        assert_eq!(c.role_par_defaut().and_then(|r| r.id.as_deref()), Some("e"));
    }

    #[test]
    fn ne_modifie_pas_un_role_au_niveau_du_sien_ou_au_dessus() {
        let d = detail(&["manage_roles"]);
        let b = ctx(&d, "b");
        assert!(!b.role_modifiable(c_role(&d, "admin")));
        assert!(!b.role_modifiable(c_role(&d, "dj")));
        assert!(b.role_modifiable(c_role(&d, "ami")));
        assert!(b.role_modifiable(c_role(&d, "e")));
        assert!(!b.role_deplacable(c_role(&d, "dj")));
        assert!(b.role_deplacable(c_role(&d, "ami")));
        assert!(!b.role_deplacable(c_role(&d, "e")));
        let o = ctx(&d, "o");
        assert!(o.role_modifiable(c_role(&d, "admin")));
        assert!(o.role_deplacable(c_role(&d, "admin")));
        let sans_role = ctx(&d, "Élise");
        assert!(!sans_role.role_modifiable(c_role(&d, "ami")));
        assert!(sans_role.role_modifiable(c_role(&d, "e")));
    }

    fn c_role<'a>(d: &'a Detail, id: &str) -> &'a Role {
        d.roles
            .as_deref()
            .unwrap()
            .iter()
            .find(|r| r.id.as_deref() == Some(id))
            .unwrap()
    }

    #[test]
    fn n_accorde_que_les_permissions_que_l_on_a() {
        let d = detail(&["manage_roles", "speak"]);
        let b = ctx(&d, "b");
        assert!(b.permission_accordable("speak"));
        assert!(!b.permission_accordable("ban_members"));
        assert!(ctx(&d, "o").permission_accordable("ban_members"));
    }

    #[test]
    fn ne_gere_que_les_membres_sous_son_rang() {
        let d = detail(&[]);
        let a = ctx(&d, "a");
        assert!(a.membre_gerable("b"));
        assert!(!a.membre_gerable("o"));
        assert!(!a.membre_gerable("a"));
        let b = ctx(&d, "b");
        assert!(!b.membre_gerable("c"));
        assert!(!b.membre_gerable("a"));
        let o = ctx(&d, "o");
        assert!(o.membre_gerable("a"));
        assert!(o.membre_gerable("o"));
    }

    #[test]
    fn reordonne_sans_passer_au_dessus_d_un_role_fige() {
        let d = detail(&[]);
        let b = ctx(&d, "b");
        assert_eq!(b.nouvel_ordre(2, 1), None);
        assert_eq!(b.nouvel_ordre(2, 2), None);
        let a = ctx(&d, "a");
        assert_eq!(
            a.nouvel_ordre(2, 1),
            Some(vec!["admin".into(), "ami".into(), "dj".into()])
        );
        assert_eq!(a.nouvel_ordre(1, 0), None);
        assert_eq!(a.nouvel_ordre(1, 9), None);
        let o = ctx(&d, "o");
        assert_eq!(
            o.nouvel_ordre(0, 2),
            Some(vec!["dj".into(), "ami".into(), "admin".into()])
        );
    }

    #[test]
    fn propose_les_membres_a_ajouter_sans_accents() {
        let d = detail(&[]);
        let a = ctx(&d, "a");
        assert!(a.membres_proposes("ami", "").is_empty());
        let noms = |l: Vec<&Membre>| -> Vec<String> {
            l.iter().map(|m| id_de(&m.id).to_string()).collect()
        };
        assert_eq!(noms(a.membres_proposes("ami", "eli")), ["Élise"]);
        assert_eq!(noms(a.membres_proposes("ami", "d")), Vec::<String>::new());
        assert_eq!(noms(a.membres_proposes("dj", "o")), Vec::<String>::new());
        assert_eq!(noms(ctx(&d, "o").membres_proposes("dj", "o")), ["o"]);
        assert_eq!(noms(a.membres_du_role("dj")), ["b", "c"]);
    }

    #[test]
    fn signale_les_permissions_que_l_on_perdrait() {
        let d = detail(&[]);
        let b = ctx(&d, "b");
        assert_eq!(
            b.pertes("dj", Some(&["speak".to_string()])),
            ["Gérer les rôles"]
        );
        assert_eq!(b.pertes("dj", None), ["Gérer les rôles", "Parler"]);
        assert!(b.pertes("ami", Some(&[])).is_empty());
        assert_eq!(
            b.pertes("e", Some(&["view_channel".to_string()])),
            ["Envoyer des messages"]
        );
        assert!(ctx(&d, "o").pertes("dj", None).is_empty());
        assert!(b.pertes_si_retire("dj", "c").is_empty());
        assert_eq!(b.pertes_si_retire("dj", "b"), ["Gérer les rôles", "Parler"]);
        assert!(ctx(&d, "a").pertes("dj", None).is_empty());
    }

    #[test]
    fn signale_les_pertes_dans_un_salon() {
        let d = detail(&[]);
        let b = ctx(&d, "b");
        let refus = |role: &str, cle: &str| Override {
            role_id: Some(role.into()),
            allow: Some(vec![]),
            deny: Some(vec![cle.into()]),
        };
        assert_eq!(
            b.pertes_dans_salon(&[], &[refus("dj", "speak")]),
            ["Parler"]
        );
        assert!(b
            .pertes_dans_salon(&[], &[refus("ami", "speak")])
            .is_empty());
        assert_eq!(
            b.pertes_dans_salon(&[], &[refus("e", "view_channel")])
                .len(),
            4
        );
        let retour = Override {
            role_id: Some("dj".into()),
            allow: Some(vec!["view_channel".into()]),
            deny: Some(vec![]),
        };
        assert!(b
            .pertes_dans_salon(&[], &[refus("e", "view_channel"), retour])
            .is_empty());
        assert!(ctx(&d, "o")
            .pertes_dans_salon(&[], &[refus("e", "view_channel")])
            .is_empty());
    }

    #[test]
    fn prepare_et_valide_le_brouillon() {
        let d = detail(&[]);
        let mut b = BrouillonRole::depuis(c_role(&d, "dj"));
        assert!(!b.differe(c_role(&d, "dj")));
        b.basculer("speak", false);
        b.basculer("kick_members", true);
        assert!(b.differe(c_role(&d, "dj")));
        assert_eq!(b.permissions_ordonnees(), ["manage_roles", "kick_members"]);
        b.nom = "  DJ  ".into();
        b.couleur = "#AABBCC".into();
        let r = b.requete();
        assert_eq!(r.name, "DJ");
        assert_eq!(r.color.as_deref(), Some("#aabbcc"));
        assert_eq!(b.erreur(), None);
        b.nom = " ".into();
        assert_eq!(b.erreur(), Some(NOM_INVALIDE));
        b.nom = "x".into();
        b.couleur = "rouge".into();
        assert_eq!(b.erreur(), Some(COULEUR_INVALIDE));
        b.couleur = String::new();
        b.icone = "123456789".into();
        assert_eq!(b.erreur(), Some(ICONE_INVALIDE));
        assert_eq!(nom_de_role(&"a".repeat(40)).chars().count(), 30);
        assert_eq!(icone_de_role("🎧🎧🎧🎧🎧🎧🎧🎧🎧").chars().count(), 8);
        assert_eq!(BrouillonRole::nouveau().nom, NOUVEAU_ROLE);
    }

    #[test]
    fn filtre_les_permissions_sans_accents() {
        assert_eq!(permissions_filtrees("").len(), 5);
        let r = permissions_filtrees("ecran");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "Salons vocaux");
        assert_eq!(r[0].1[0].cle, "share_screen");
        assert!(permissions_filtrees("zzz").is_empty());
    }

    #[test]
    fn calcule_les_trois_etats_d_une_surcharge() {
        let o = Override {
            role_id: Some("dj".into()),
            allow: Some(vec!["speak".into()]),
            deny: Some(vec!["video".into()]),
        };
        assert_eq!(etat_de(Some(&o), "speak"), EtatPermission::Autorise);
        assert_eq!(etat_de(Some(&o), "video"), EtatPermission::Refuse);
        assert_eq!(etat_de(Some(&o), "connect"), EtatPermission::Herite);
        assert_eq!(etat_de(None, "speak"), EtatPermission::Herite);
        let n = surcharge_changee(Some(&o), "dj", "speak", EtatPermission::Refuse);
        assert_eq!(n.allow, Some(vec![]));
        assert_eq!(n.deny, Some(vec!["video".into(), "speak".into()]));
        let h = surcharge_changee(Some(&o), "dj", "video", EtatPermission::Herite);
        assert_eq!(h.deny, Some(vec![]));
        let liste = surcharges_apres(std::slice::from_ref(&o), &h);
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].deny, Some(vec![]));
        for e in [
            EtatPermission::Herite,
            EtatPermission::Autorise,
            EtatPermission::Refuse,
        ] {
            assert_eq!(EtatPermission::depuis_rang(e.rang()), e);
        }
    }

    #[test]
    fn montre_les_permissions_du_type_de_salon() {
        let titres = |g: Option<GenreSalon>| -> Vec<&str> {
            groupes_du_salon(g).iter().map(|g| g.titre).collect()
        };
        assert_eq!(titres(Some(GenreSalon::Text)), ["Salons texte"]);
        assert_eq!(titres(Some(GenreSalon::Voice)), ["Salons vocaux"]);
        assert_eq!(titres(Some(GenreSalon::Cinema)), ["Salons vocaux"]);
        assert_eq!(
            titres(Some(GenreSalon::Music)),
            ["Salons texte", "Esquisses"]
        );
        assert_eq!(titres(None), ["Salons texte"]);
    }

    #[test]
    fn verifie_les_couleurs_et_les_libelles() {
        assert!(couleur_valide(""));
        assert!(couleur_valide("#a1B2c3"));
        assert!(!couleur_valide("#a1B2c"));
        assert!(!couleur_valide("a1B2c3f"));
        assert!(COULEURS_ROLE.iter().all(|c| couleur_valide(c)));
        let mut r = role("dj", 1, &[], false);
        r.icon = Some("🎧".into());
        assert_eq!(libelle_du_role(&r), "🎧 dj");
        assert_eq!(message_de_perte(&["Parler"]), "Tu vas perdre cette permission : Parler. Tu ne pourras peut-être pas revenir en arrière.");
    }
}
