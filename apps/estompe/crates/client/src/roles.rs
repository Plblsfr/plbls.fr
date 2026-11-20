use crate::types::{GetServersIdReponse, GetServersIdReponseMembersElement, Role};

pub type Membre = GetServersIdReponseMembersElement;
pub type Detail = GetServersIdReponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoPermission {
    pub cle: &'static str,
    pub libelle: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupePermissions {
    pub titre: &'static str,
    pub permissions: &'static [InfoPermission],
}

const fn permission(
    cle: &'static str,
    libelle: &'static str,
    description: &'static str,
) -> InfoPermission {
    InfoPermission {
        cle,
        libelle,
        description,
    }
}

pub const GROUPES_PERMISSIONS: &[GroupePermissions] = &[
    GroupePermissions {
        titre: "Général",
        permissions: &[
            permission("manage_server", "Gérer le serveur", "Changer le nom et l'icône du serveur."),
            permission("manage_roles", "Gérer les rôles", "Créer, modifier, ordonner et attribuer les rôles situés sous le sien."),
            permission("manage_channels", "Gérer les salons", "Créer, renommer, supprimer les salons et régler leurs permissions."),
            permission("create_invites", "Créer des invitations", "Créer et révoquer les liens d'invitation."),
            permission("view_audit_log", "Voir le journal d'administration", "Consulter le journal de modération et l'historique des membres."),
        ],
    },
    GroupePermissions {
        titre: "Membres",
        permissions: &[
            permission("kick_members", "Expulser des membres", "Retirer un membre du serveur ; il peut revenir avec une invitation."),
            permission("ban_members", "Bannir des membres", "Interdire le retour d'un membre, définitivement ou pour une durée."),
            permission("manage_nicknames", "Changer le pseudo des autres", "Modifier le pseudo d'un membre sur ce serveur."),
            permission("timeout_members", "Exclure temporairement des membres", "Bloquer les actions d'un membre pendant une durée, ou l'avertir."),
        ],
    },
    GroupePermissions {
        titre: "Salons texte",
        permissions: &[
            permission("view_channel", "Voir le salon", "Voir le salon et lire ses messages."),
            permission("send_messages", "Envoyer des messages", "Écrire dans les salons texte."),
            permission("attach_files", "Envoyer des pièces jointes et des messages vocaux", "Joindre des fichiers et enregistrer des messages vocaux."),
            permission("add_reactions", "Ajouter des réactions", "Réagir aux messages avec un emoji."),
            permission("mention_everyone", "Mentionner @everyone et les rôles", "Notifier tout le salon ou les membres d'un rôle."),
            permission("manage_messages", "Gérer les messages", "Supprimer et épingler les messages des autres, clore leurs sondages."),
            permission("create_polls", "Créer des sondages", "Lancer un sondage dans un salon."),
        ],
    },
    GroupePermissions {
        titre: "Salons vocaux",
        permissions: &[
            permission("connect", "Rejoindre", "Se connecter aux salons vocaux."),
            permission("speak", "Parler", "Utiliser le micro."),
            permission("video", "Vidéo", "Allumer la caméra."),
            permission("share_screen", "Partager l'écran", "Partager son écran."),
            permission("mute_members", "Couper le micro des membres", "Couper ou rétablir le micro d'un participant."),
            permission("deafen_members", "Couper le son des membres", "Couper ou rétablir le son d'un participant."),
            permission("move_members", "Déplacer des membres", "Changer un participant de salon vocal."),
            permission("disconnect_members", "Déconnecter des membres", "Retirer un participant du vocal sans l'expulser du serveur."),
        ],
    },
    GroupePermissions {
        titre: "Esquisses",
        permissions: &[permission(
            "organize_watch",
            "Organiser des séances",
            "Planifier une séance « Regarder ensemble » ou d'écoute et marquer les vidéos et morceaux comme vus ou écoutés. Publier et gérer les publications des autres suivent les permissions des salons texte.",
        )],
    },
];

pub fn permissions_de_salon() -> Vec<&'static str> {
    GROUPES_PERMISSIONS
        .iter()
        .skip(3)
        .flat_map(|g| g.permissions.iter().map(|p| p.cle))
        .collect()
}

pub fn libelle_permission(cle: &str) -> &str {
    GROUPES_PERMISSIONS
        .iter()
        .flat_map(|g| g.permissions.iter())
        .find(|p| p.cle == cle)
        .map_or(cle, |p| p.libelle)
}

fn position(role: &Role) -> i64 {
    role.position.unwrap_or(0)
}

pub fn roles_du_membre<'a>(detail: Option<&'a Detail>, membre: Option<&Membre>) -> Vec<&'a Role> {
    let roles = detail.and_then(|d| d.roles.as_deref()).unwrap_or_default();
    let ids = membre.and_then(|m| m.roles.as_deref()).unwrap_or_default();
    let mut trouves: Vec<&Role> = ids
        .iter()
        .filter_map(|id| roles.iter().rev().find(|r| r.id.as_deref() == Some(id)))
        .collect();
    trouves.sort_by_key(|r| std::cmp::Reverse(position(r)));
    trouves
}

pub fn couleur_du_pseudo<'a>(
    detail: Option<&'a Detail>,
    membre: Option<&Membre>,
) -> Option<&'a str> {
    roles_du_membre(detail, membre)
        .into_iter()
        .find_map(|r| r.color.as_deref().filter(|c| !c.is_empty()))
}

pub fn position_la_plus_haute(detail: Option<&Detail>, membre: Option<&Membre>) -> i64 {
    roles_du_membre(detail, membre)
        .first()
        .map_or(0, |r| position(r))
}

fn jours_depuis_epoque(annee: i64, mois: i64, jour: i64) -> i64 {
    let annee = if mois <= 2 { annee - 1 } else { annee };
    let ere = annee.div_euclid(400);
    let annee_ere = annee.rem_euclid(400);
    let jour_annee = (153 * ((mois + 9) % 12) + 2) / 5 + jour - 1;
    let jour_ere = annee_ere * 365 + annee_ere / 4 - annee_ere / 100 + jour_annee;
    ere * 146_097 + jour_ere - 719_468
}

fn nombre(texte: &str, taille: usize) -> Option<i64> {
    if texte.len() != taille || !texte.bytes().all(|o| o.is_ascii_digit()) {
        return None;
    }
    texte.parse().ok()
}

pub fn date_en_millisecondes(date: &str) -> Option<i64> {
    let (jour, heure) = date.split_once(['T', 't', ' '])?;
    let mut j = jour.split('-');
    let (annee, mois, quantieme) = (
        nombre(j.next()?, 4)?,
        nombre(j.next()?, 2)?,
        nombre(j.next()?, 2)?,
    );
    if j.next().is_some() || !(1..=12).contains(&mois) || !(1..=31).contains(&quantieme) {
        return None;
    }
    let position_decalage = heure.find(['Z', 'z', '+', '-']);
    let (horloge, decalage) = match position_decalage {
        Some(p) => heure.split_at(p),
        None => (heure, "Z"),
    };
    let (hms, fraction) = horloge.split_once('.').unwrap_or((horloge, ""));
    let mut h = hms.split(':');
    let (heures, minutes, secondes) = (
        nombre(h.next()?, 2)?,
        nombre(h.next()?, 2)?,
        h.next().map_or(Some(0), |s| nombre(s, 2))?,
    );
    if h.next().is_some() || heures > 24 || minutes > 59 || secondes > 59 {
        return None;
    }
    let millis = if fraction.is_empty() {
        0
    } else if fraction.bytes().all(|o| o.is_ascii_digit()) {
        let tronque: String = fraction.chars().chain("00".chars()).take(3).collect();
        tronque.parse::<i64>().ok()?
    } else {
        return None;
    };
    let minutes_decalage = match decalage {
        "Z" | "z" => 0,
        _ => {
            let signe = if decalage.starts_with('-') { -1 } else { 1 };
            let (dh, dm) = decalage[1..].split_once(':')?;
            signe * (nombre(dh, 2)? * 60 + nombre(dm, 2)?)
        }
    };
    let secondes_totales = jours_depuis_epoque(annee, mois, quantieme) * 86_400
        + heures * 3600
        + minutes * 60
        + secondes
        - minutes_decalage * 60;
    Some(secondes_totales * 1000 + millis)
}

pub fn est_exclu(fin_exclusion: Option<&str>, maintenant_ms: i64) -> bool {
    fin_exclusion
        .filter(|f| !f.is_empty())
        .and_then(date_en_millisecondes)
        .is_some_and(|fin| fin > maintenant_ms)
}

pub fn nom_affiche<'a>(pseudo: &'a str, pseudo_de_serveur: Option<&'a str>) -> &'a str {
    pseudo_de_serveur
        .filter(|n| !n.is_empty())
        .unwrap_or(pseudo)
}

pub fn membre_exclu(membre: Option<&Membre>, maintenant_ms: i64) -> bool {
    est_exclu(
        membre.and_then(|m| m.timeout_until.as_deref()),
        maintenant_ms,
    )
}

pub fn nom_du_membre(membre: &Membre) -> &str {
    nom_affiche(
        membre.pseudo.as_deref().unwrap_or_default(),
        membre.nickname.as_deref(),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupeMembres<'a> {
    pub role: Option<&'a Role>,
    pub membres: Vec<&'a Membre>,
}

pub fn grouper_membres<'a>(
    detail: &'a Detail,
    membres: impl IntoIterator<Item = &'a Membre>,
) -> Vec<GroupeMembres<'a>> {
    let mut affiches: Vec<&Role> = detail
        .roles
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|r| r.hoist == Some(true) && r.is_default != Some(true))
        .collect();
    affiches.sort_by_key(|r| std::cmp::Reverse(position(r)));
    let mut groupes: Vec<GroupeMembres> = affiches
        .into_iter()
        .map(|role| GroupeMembres {
            role: Some(role),
            membres: Vec::new(),
        })
        .collect();
    let mut reste = GroupeMembres {
        role: None,
        membres: Vec::new(),
    };
    for membre in membres {
        let siens = membre.roles.as_deref().unwrap_or_default();
        let groupe = groupes.iter_mut().find(|g| {
            g.role
                .and_then(|r| r.id.as_deref())
                .is_some_and(|id| siens.iter().any(|s| s == id))
        });
        match groupe {
            Some(g) => g.membres.push(membre),
            None => reste.membres.push(membre),
        }
    }
    groupes.retain(|g| !g.membres.is_empty());
    if !reste.membres.is_empty() {
        groupes.push(reste);
    }
    groupes
}

fn proprietaire(detail: &Detail) -> Option<&str> {
    detail.server.as_ref().and_then(|s| s.owner_id.as_deref())
}

fn trouver_membre<'a>(detail: &'a Detail, id: &str) -> Option<&'a Membre> {
    detail
        .members
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|m| m.id.as_deref() == Some(id))
}

pub fn domine(detail: Option<&Detail>, acteur: &str, cible: &str) -> bool {
    let Some(detail) = detail else {
        return false;
    };
    if acteur == cible || proprietaire(detail) == Some(cible) {
        return false;
    }
    if proprietaire(detail) == Some(acteur) {
        return true;
    }
    position_la_plus_haute(Some(detail), trouver_membre(detail, acteur))
        > position_la_plus_haute(Some(detail), trouver_membre(detail, cible))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rang {
    Position(i64),
    Createur,
}

pub fn rang_du_membre(detail: Option<&Detail>, utilisateur: &str) -> Rang {
    let Some(detail) = detail else {
        return Rang::Position(0);
    };
    if proprietaire(detail) == Some(utilisateur) {
        return Rang::Createur;
    }
    Rang::Position(position_la_plus_haute(
        Some(detail),
        trouver_membre(detail, utilisateur),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Server;
    use std::collections::HashSet;

    fn role(id: &str, position: i64, modifs: impl FnOnce(&mut Role)) -> Role {
        let mut r = Role {
            id: Some(id.into()),
            name: Some(id.into()),
            color: Some(String::new()),
            icon: Some(String::new()),
            position: Some(position),
            hoist: Some(false),
            permissions: Some(vec![]),
            is_default: Some(false),
        };
        modifs(&mut r);
        r
    }

    fn detail(roles: Vec<Role>, membres: Vec<Membre>) -> Detail {
        Detail {
            server: Some(Server {
                id: Some("s".into()),
                name: Some("s".into()),
                owner_id: Some("a".into()),
                icon_version: Some(0),
            }),
            channels: Some(vec![]),
            members: Some(membres),
            roles: Some(roles),
            permissions: None,
        }
    }

    fn m(id: &str, roles: &[&str]) -> Membre {
        Membre {
            id: Some(id.into()),
            pseudo: Some(id.into()),
            online: Some(false),
            roles: Some(roles.iter().map(|r| r.to_string()).collect()),
            ..Membre::default()
        }
    }

    fn d() -> Detail {
        detail(
            vec![
                role("admin", 3, |r| {
                    r.color = Some("#ff0000".into());
                    r.hoist = Some(true);
                }),
                role("dj", 2, |r| r.color = Some("#00ff00".into())),
                role("ami", 1, |r| r.hoist = Some(true)),
                role("e", 0, |r| r.is_default = Some(true)),
            ],
            vec![],
        )
    }

    fn ids_roles(roles: Vec<&Role>) -> Vec<String> {
        roles.iter().filter_map(|r| r.id.clone()).collect()
    }

    #[test]
    fn classe_les_roles_d_un_membre_du_plus_eleve_au_plus_bas() {
        let d = d();
        assert_eq!(
            ids_roles(roles_du_membre(
                Some(&d),
                Some(&m("x", &["ami", "admin", "dj"]))
            )),
            ["admin", "dj", "ami"]
        );
        assert_eq!(
            position_la_plus_haute(Some(&d), Some(&m("x", &["ami", "dj"]))),
            2
        );
        assert_eq!(position_la_plus_haute(Some(&d), Some(&m("x", &[]))), 0);
    }

    #[test]
    fn prend_la_couleur_du_role_le_plus_eleve_qui_en_a_une() {
        let d = d();
        assert_eq!(
            couleur_du_pseudo(Some(&d), Some(&m("x", &["ami", "dj"]))),
            Some("#00ff00")
        );
        assert_eq!(couleur_du_pseudo(Some(&d), Some(&m("x", &["ami"]))), None);
        assert_eq!(
            couleur_du_pseudo(Some(&d), Some(&m("x", &["dj", "admin"]))),
            Some("#ff0000")
        );
    }

    #[test]
    fn groupe_les_membres_par_role_affiche_separement() {
        let d = d();
        let membres = vec![
            m("a", &["ami"]),
            m("b", &["dj"]),
            m("c", &["ami", "admin"]),
            m("d", &[]),
        ];
        let groupes = grouper_membres(&d, &membres);
        let vue: Vec<(Option<String>, Vec<String>)> = groupes
            .iter()
            .map(|g| {
                (
                    g.role.and_then(|r| r.id.clone()),
                    g.membres.iter().filter_map(|x| x.id.clone()).collect(),
                )
            })
            .collect();
        assert_eq!(
            vue,
            vec![
                (Some("admin".to_string()), vec!["c".to_string()]),
                (Some("ami".to_string()), vec!["a".to_string()]),
                (None, vec!["b".to_string(), "d".to_string()]),
            ]
        );
    }

    #[test]
    fn ne_cree_pas_de_groupe_vide() {
        let d = d();
        let membres = vec![m("a", &[])];
        let roles: Vec<Option<&Role>> = grouper_membres(&d, &membres)
            .iter()
            .map(|g| g.role)
            .collect();
        assert_eq!(roles, vec![None]);
    }

    #[test]
    fn decrit_25_permissions_chacune_une_seule_fois() {
        let cles: Vec<&str> = GROUPES_PERMISSIONS
            .iter()
            .flat_map(|g| g.permissions.iter().map(|p| p.cle))
            .collect();
        assert_eq!(cles.len(), 25);
        assert_eq!(cles.iter().collect::<HashSet<_>>().len(), 25);
        assert_eq!(permissions_de_salon().len(), 9);
        assert_eq!(libelle_permission("speak"), "Parler");
        assert_eq!(libelle_permission("inconnue"), "inconnue");
    }

    #[test]
    fn ne_gere_que_les_membres_sous_son_role_le_plus_eleve_jamais_le_createur() {
        let mut dd = detail(
            d().roles.unwrap(),
            vec![
                m("a", &["admin"]),
                m("b", &["dj"]),
                m("c", &["dj"]),
                m("o", &[]),
            ],
        );
        dd.server.as_mut().unwrap().owner_id = Some("o".into());
        assert!(domine(Some(&dd), "a", "b"));
        assert!(!domine(Some(&dd), "b", "a"));
        assert!(!domine(Some(&dd), "b", "c"));
        assert!(!domine(Some(&dd), "a", "a"));
        assert!(!domine(Some(&dd), "a", "o"));
        assert!(domine(Some(&dd), "o", "a"));
        assert!(!domine(None, "o", "a"));
    }

    #[test]
    fn classe_les_rangs() {
        let mut dd = detail(d().roles.unwrap(), vec![m("a", &["admin"]), m("o", &[])]);
        dd.server.as_mut().unwrap().owner_id = Some("o".into());
        assert_eq!(rang_du_membre(Some(&dd), "o"), Rang::Createur);
        assert_eq!(rang_du_membre(Some(&dd), "a"), Rang::Position(3));
        assert_eq!(rang_du_membre(Some(&dd), "inconnu"), Rang::Position(0));
        assert_eq!(rang_du_membre(None, "a"), Rang::Position(0));
        assert!(Rang::Createur > Rang::Position(99));
    }

    #[test]
    fn reconnait_une_exclusion_temporaire_en_cours() {
        let maintenant = date_en_millisecondes("2026-10-26T12:00:00Z").unwrap();
        assert!(est_exclu(Some("2026-10-26T12:00:01Z"), maintenant));
        assert!(!est_exclu(Some("2026-10-26T12:00:00Z"), maintenant));
        assert!(!est_exclu(None, maintenant));
        let exclu = |fin: Option<&str>| Membre {
            timeout_until: fin.map(str::to_string),
            ..Membre::default()
        };
        assert!(membre_exclu(
            Some(&exclu(Some("2026-10-26T12:00:01Z"))),
            maintenant
        ));
        assert!(!membre_exclu(
            Some(&exclu(Some("2026-10-26T12:00:00Z"))),
            maintenant
        ));
        assert!(!membre_exclu(Some(&exclu(None)), maintenant));
        assert!(!membre_exclu(None, maintenant));
    }

    #[test]
    fn lit_les_dates_iso() {
        assert_eq!(date_en_millisecondes("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            date_en_millisecondes("2026-10-26T12:00:00.500Z"),
            Some(1_793_016_000_500)
        );
        assert_eq!(
            date_en_millisecondes("2026-10-26T14:00:00+02:00"),
            date_en_millisecondes("2026-10-26T12:00:00Z")
        );
        assert_eq!(date_en_millisecondes("pas une date"), None);
    }

    #[test]
    fn prefere_le_pseudo_de_serveur() {
        assert_eq!(nom_affiche("bob", Some("Bobby")), "Bobby");
        assert_eq!(nom_affiche("bob", Some("")), "bob");
        assert_eq!(nom_affiche("bob", None), "bob");
        let membre = |pseudo_de_serveur: Option<&str>| Membre {
            pseudo: Some("bob".into()),
            nickname: pseudo_de_serveur.map(str::to_string),
            ..Membre::default()
        };
        assert_eq!(nom_du_membre(&membre(Some("Bobby"))), "Bobby");
        assert_eq!(nom_du_membre(&membre(Some(""))), "bob");
        assert_eq!(nom_du_membre(&membre(None)), "bob");
    }
}
