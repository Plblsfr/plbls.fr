use crate::commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, evenement, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::store::Etat;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    confirmation, creer_fenetre, ecrans, menu, DonneesConfirmation, DonneesCoquille,
    DonneesMessagerie, DonneesReglagesServeur, DonneesRoles, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use serde_json::{json, Value};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model};

const MOI: [&str; 8] = [
    "view_channel",
    "send_messages",
    "add_reactions",
    "connect",
    "speak",
    "manage_roles",
    "manage_channels",
    "kick_members",
];

fn etat_des_roles() -> Etat {
    let mut etat = etat_avec_membres();
    for s in etat.serveurs.iter_mut() {
        if s.id.as_deref() == Some("s1") {
            s.owner_id = Some("eve".into());
        }
    }
    let detail = etat.details.get_mut("s1").unwrap();
    detail.server.as_mut().unwrap().owner_id = Some("eve".into());
    detail.roles = Some(
        serde_json::from_value(json!([
            {"id": "r0", "name": "everyone", "is_default": true, "position": 0, "hoist": false, "color": "", "icon": "", "permissions": ["view_channel", "send_messages", "connect", "speak"]},
            {"id": "radm", "name": "Admin", "position": 8, "hoist": true, "color": "#e74c3c", "icon": "", "permissions": ["manage_server", "manage_roles", "manage_channels", "ban_members"]},
            {"id": "rm", "name": "Modération", "position": 5, "hoist": true, "color": "#2f5d8a", "icon": "", "permissions": ["manage_roles", "manage_channels", "kick_members", "send_messages"]},
            {"id": "ra", "name": "Artistes", "position": 2, "hoist": false, "color": "#b0413e", "icon": "", "permissions": ["attach_files"]}
        ]))
        .unwrap(),
    );
    for m in detail.members.as_mut().unwrap().iter_mut() {
        if m.id.as_deref() == Some("eve") {
            m.roles = Some(vec!["radm".into(), "rm".into()]);
        }
    }
    detail.permissions = Some(MOI.iter().map(|p| p.to_string()).collect());
    etat
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &str) {
    let libelle = libelle.to_string();
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, &libelle)
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("« {libelle} » introuvable"))
            .mock_single_click(PointerEventButton::Left);
    });
}

fn roles(f: &Principale) -> Vec<String> {
    f.global::<DonneesRoles>()
        .get_roles()
        .iter()
        .map(|r| r.libelle.to_string())
        .collect()
}

fn requetes(serveur: &ServeurMessages, ligne: &str) -> usize {
    serveur
        .requetes
        .lock()
        .unwrap()
        .iter()
        .filter(|r| *r == ligne)
        .count()
}

fn dernier_corps(serveur: &ServeurMessages, ligne: &str) -> Value {
    serveur
        .corps
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(r, _)| r == ligne)
        .map(|(_, c)| serde_json::from_str(c).unwrap_or_default())
        .unwrap_or_default()
}

fn choisir_role(faible: &slint::Weak<Principale>, id: &'static str) {
    sur_l_interface(faible, move |f| {
        f.global::<DonneesRoles>().invoke_choisir_role(id.into())
    });
    attendre_l_interface(
        faible,
        move |f| f.global::<DonneesRoles>().get_choisi() == id,
        "rôle choisi",
    );
}

fn basculer(faible: &slint::Weak<Principale>, cle: &'static str, actif: bool) {
    sur_l_interface(faible, move |f| {
        f.global::<DonneesRoles>()
            .invoke_basculer_permission(cle.into(), actif)
    });
}

fn hierarchie(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    cliquer(faible, "Paramètres du serveur");
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("roles".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            let deplacables: Vec<bool> = g.get_roles().iter().map(|r| r.deplacable).collect();
            roles(f) == ["Admin", "Modération", "Artistes", "everyone"]
                && deplacables == [false, false, true, false]
                && g.get_compte() == "RÔLES · 4"
        },
        "rôles du plus haut au plus bas, seul celui sous le mien se déplace",
    );
    for id in ["radm", "rm"] {
        choisir_role(faible, id);
        attendre_l_interface(
            faible,
            |f| !f.global::<DonneesRoles>().get_modifiable(),
            "rôle au niveau du mien ou au-dessus en lecture seule",
        );
        sur_l_interface(faible, |f| {
            let g = f.global::<DonneesRoles>();
            g.invoke_basculer_permission("kick_members".into(), true);
            g.invoke_nom_modifie("Pirate".into());
            g.invoke_enregistrer();
            g.invoke_supprimer();
            g.invoke_deplacer_role(1, 2);
            g.invoke_retirer_membre("eve".into());
        });
        assert!(sur_l_interface(faible, |f| {
            !f.global::<DonneesConfirmation>().get_ouverte()
                && f.global::<DonneesRoles>()
                    .get_membres()
                    .iter()
                    .all(|m| !m.gerable)
        }));
    }
    assert_eq!(requetes(serveur, "PATCH /servers/s1/roles/radm"), 0);
    assert_eq!(requetes(serveur, "PATCH /servers/s1/roles/rm"), 0);
    assert_eq!(requetes(serveur, "PUT /servers/s1/roles-order"), 0);
    assert_eq!(
        requetes(serveur, "DELETE /servers/s1/members/eve/roles/rm"),
        0
    );
    println!("365 C2 : un rôle au niveau du mien ou au-dessus ne se modifie pas");
}

fn modifier_un_role(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    choisir_role(faible, "ra");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            let permissions: Vec<(String, bool, bool)> = g
                .get_groupes()
                .iter()
                .flat_map(|groupe| groupe.permissions.iter().collect::<Vec<_>>())
                .map(|p| (p.cle.to_string(), p.allumee, p.modifiable))
                .collect();
            g.get_modifiable()
                && g.get_nom() == "Artistes"
                && permissions.len() == 25
                && permissions.contains(&("attach_files".into(), true, false))
                && permissions.contains(&("speak".into(), false, true))
                && g.get_membres()
                    .iter()
                    .map(|m| m.nom.to_string())
                    .collect::<Vec<_>>()
                    == ["Caro", "Inès"]
        },
        "éditeur du rôle : permissions accordables et membres",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesRoles>();
        g.set_nom("Peintres".into());
        g.invoke_nom_modifie("Peintres".into());
        g.invoke_couleur_choisie("#3498DB".into());
        g.set_filtre("ecran".into());
        g.invoke_filtre_modifie("ecran".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            g.get_groupes().row_count() == 1 && g.get_couleur_hex() == "#3498db" && g.get_modifie()
        },
        "filtre sans accents et couleur choisie",
    );
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesRoles>();
        g.set_filtre("".into());
        g.invoke_filtre_modifie("".into());
        g.invoke_basculer_permission("ban_members".into(), true);
    });
    basculer(faible, "speak", true);
    cliquer(faible, "Enregistrer");
    attendre(
        || requetes(serveur, "PATCH /servers/s1/roles/ra") == 1,
        "rôle enregistré",
    );
    assert_eq!(
        dernier_corps(serveur, "PATCH /servers/s1/roles/ra"),
        json!({"name": "Peintres", "color": "#3498db", "icon": "", "hoist": false, "permissions": ["attach_files", "speak"]})
    );
    attendre_l_interface(
        faible,
        |f| {
            roles(f) == ["Admin", "Modération", "Peintres", "everyone"]
                && f.global::<DonneesReglagesServeur>().get_annonce() == "Rôle enregistré."
                && !f.global::<DonneesRoles>().get_modifie()
        },
        "liste rechargée avec le nouveau nom",
    );
    println!("365 : rôle renommé, coloré, permission accordée");
}

fn glisser(faible: &slint::Weak<Principale>, libelle: &str, pas: f32) {
    let libelle = libelle.to_string();
    sur_l_interface(faible, move |f| {
        let ligne = ElementHandle::find_by_accessible_label(f, &libelle)
            .next()
            .unwrap_or_else(|| panic!("ligne « {libelle} » introuvable"));
        let (p, t) = (ligne.absolute_position(), ligne.size());
        let depart = LogicalPosition::new(p.x + 40.0, p.y + t.height / 2.0);
        let w = f.window();
        w.dispatch_event(WindowEvent::PointerMoved { position: depart });
        w.dispatch_event(WindowEvent::PointerPressed {
            position: depart,
            button: PointerEventButton::Left,
        });
        for i in 1..=4 {
            w.dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(depart.x, depart.y + pas * i as f32 / 4.0),
            });
        }
        let fin = LogicalPosition::new(depart.x, depart.y + pas);
        w.dispatch_event(WindowEvent::PointerReleased {
            position: fin,
            button: PointerEventButton::Left,
        });
    });
}

fn creer_ordonner_supprimer(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    cliquer(faible, "Nouveau rôle");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            roles(f)
                == [
                    "Admin",
                    "Modération",
                    "Peintres",
                    "Nouveau rôle",
                    "everyone",
                ]
                && g.get_nom() == "Nouveau rôle"
                && g.get_modifiable()
        },
        "rôle créé sous les autres et choisi",
    );
    assert_eq!(
        dernier_corps(serveur, "POST /servers/s1/roles")["name"],
        "Nouveau rôle"
    );
    glisser(faible, "Rôle Nouveau rôle", -44.0);
    attendre(
        || requetes(serveur, "PUT /servers/s1/roles-order") == 1,
        "ordre envoyé après le glisser",
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/roles-order"),
        json!({"ids": ["radm", "rm", "rn4", "ra"]})
    );
    attendre_l_interface(
        faible,
        |f| {
            roles(f)
                == [
                    "Admin",
                    "Modération",
                    "Nouveau rôle",
                    "Peintres",
                    "everyone",
                ]
        },
        "nouvel ordre affiché",
    );
    glisser(faible, "Rôle Nouveau rôle", -88.0);
    cliquer(faible, "Descendre Nouveau rôle");
    attendre(
        || requetes(serveur, "PUT /servers/s1/roles-order") == 2,
        "flèche vers le bas",
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/roles-order"),
        json!({"ids": ["radm", "rm", "ra", "rn4"]})
    );
    attendre_l_interface(
        faible,
        |f| {
            roles(f)
                == [
                    "Admin",
                    "Modération",
                    "Peintres",
                    "Nouveau rôle",
                    "everyone",
                ]
        },
        "un rôle ne passe pas au-dessus du mien, la flèche le redescend",
    );
    choisir_role(faible, "rn4");
    cliquer(faible, "Supprimer le rôle");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "suppression à confirmer",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    attendre(
        || requetes(serveur, "DELETE /servers/s1/roles/rn4") == 1,
        "rôle supprimé",
    );
    attendre_l_interface(
        faible,
        |f| {
            roles(f) == ["Admin", "Modération", "Peintres", "everyone"]
                && f.global::<DonneesRoles>().get_choisi() == ""
        },
        "rôle retiré de la liste",
    );
    println!("365 : rôle créé, déplacé par glisser et par flèche, supprimé");
}

fn attribuer(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    choisir_role(faible, "ra");
    sur_l_interface(faible, |f| {
        let g = f.global::<DonneesRoles>();
        g.set_recherche("dav".into());
        g.invoke_recherche_modifiee("dav".into());
    });
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesRoles>()
                .get_proposes()
                .iter()
                .map(|m| m.nom.to_string())
                .collect::<Vec<_>>()
                == ["dave"]
        },
        "membre proposé",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_ajouter_membre("dave".into())
    });
    attendre(
        || requetes(serveur, "PUT /servers/s1/members/dave/roles/ra") == 1,
        "rôle attribué",
    );
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesReglagesServeur>().get_en_cours(),
        "attribution terminée",
    );
    assert!(sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .get_membres()
            .iter()
            .all(|m| m.gerable)
    }));
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_retirer_membre("carol".into())
    });
    attendre(
        || requetes(serveur, "DELETE /servers/s1/members/carol/roles/ra") == 1,
        "rôle retiré",
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            g.get_membres()
                .iter()
                .map(|m| m.nom.to_string())
                .collect::<Vec<_>>()
                == ["dave", "Inès"]
                && g.get_recherche() == ""
                && g.get_proposes().row_count() == 0
        },
        "membres du rôle à jour",
    );
    println!("365 : rôle attribué puis retiré");
}

fn garde_fou(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    choisir_role(faible, "r0");
    basculer(faible, "connect", false);
    cliquer(faible, "Enregistrer");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesConfirmation>();
            g.get_ouverte() && g.get_demande().message.contains("Rejoindre")
        },
        "perte de ses propres droits à confirmer",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, false));
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesConfirmation>().get_ouverte(),
        "confirmation refermée",
    );
    assert_eq!(requetes(serveur, "PATCH /servers/s1/roles/r0"), 0);
    cliquer(faible, "Enregistrer");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "confirmation redemandée",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, true));
    attendre(
        || requetes(serveur, "PATCH /servers/s1/roles/r0") == 1,
        "perte confirmée",
    );
    assert_eq!(
        dernier_corps(serveur, "PATCH /servers/s1/roles/r0")["permissions"],
        json!(["view_channel", "send_messages", "speak"])
    );
    println!("365 : perdre ses propres droits demande une confirmation");
}

fn permissions_d_un_salon(faible: &slint::Weak<Principale>, serveur: &ServeurMessages) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>()
            .invoke_choisir("salons".into())
    });
    attendre_l_interface(
        faible,
        |f| {
            let groupes: Vec<String> = f
                .global::<DonneesRoles>()
                .get_salons()
                .iter()
                .filter(|s| !s.groupe.is_empty())
                .map(|s| s.groupe.to_string())
                .collect();
            groupes
                == [
                    "SALONS TEXTE",
                    "ESQUISSES",
                    "SALONS VOCAUX",
                    "SALONS CINÉMA",
                ]
        },
        "salons groupés par type",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_basculer_salon("c1".into())
    });
    attendre(
        || requetes(serveur, "GET /servers/s1/channels/c1/overrides") == 1,
        "surcharges chargées",
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            let etats: Vec<i32> = g.get_permissions_salon().iter().map(|p| p.etat).collect();
            g.get_role_salon() == "everyone" && etats.len() == 7 && etats[1] == 2
        },
        "trois états, hérités ou repris du serveur",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_definir_permission_salon("view_channel".into(), 2)
    });
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesConfirmation>();
            g.get_ouverte() && g.get_demande().message.contains("Voir le salon")
        },
        "se retirer la vue du salon demande une confirmation",
    );
    sur_l_interface(faible, |f| confirmation::repondre(f, false));
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_definir_permission_salon("add_reactions".into(), 2)
    });
    attendre(
        || requetes(serveur, "PUT /servers/s1/channels/c1/overrides/r0") == 1,
        "refus envoyé",
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/channels/c1/overrides/r0"),
        json!({"allow": [], "deny": ["send_messages", "add_reactions"]})
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_menu_role_salon(10.0, 10.0, 40.0);
        menu::choisir(f, "ra");
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesRoles>().get_role_salon() == "Peintres",
        "rôle changé par le menu",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_definir_permission_salon("send_messages".into(), 1)
    });
    attendre(
        || requetes(serveur, "PUT /servers/s1/channels/c1/overrides/ra") == 1,
        "autorisation envoyée",
    );
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesRoles>();
            g.get_permissions_salon()
                .iter()
                .find(|p| p.cle == "send_messages")
                .is_some_and(|p| p.etat == 1)
        },
        "autorisé affiché",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesRoles>()
            .invoke_definir_permission_salon("send_messages".into(), 0)
    });
    attendre(
        || requetes(serveur, "PUT /servers/s1/channels/c1/overrides/ra") == 2,
        "retour à hériter",
    );
    assert_eq!(
        dernier_corps(serveur, "PUT /servers/s1/channels/c1/overrides/ra"),
        json!({"allow": [], "deny": []})
    );
    println!("365 : surcharges d'un salon en trois états, par rôle");
}

fn temps_reel(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
) {
    sur_l_interface(faible, |f| {
        f.global::<DonneesReglagesServeur>().invoke_fermer()
    });
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesMessagerie>().get_lecture_seule(),
        "j'écris dans général",
    );
    {
        let mut detail = serveur.detail_s1.lock().unwrap();
        let d = detail.as_mut().unwrap();
        d["permissions"] = json!(["view_channel", "connect", "speak", "kick_members"]);
        for c in d["channels"].as_array_mut().unwrap() {
            if c["id"] == "c1" {
                c["permissions"] = json!(["view_channel"]);
            }
        }
    }
    fabrique.envoyer(evenement("roles.updated", json!({"server_id": "s1"})));
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesMessagerie>().get_lecture_seule(),
        "un autre client m'a retiré l'écriture : zone de saisie en lecture seule",
    );
    cliquer(faible, "Paramètres du serveur");
    attendre_l_interface(
        faible,
        |f| {
            let onglets: Vec<String> = f
                .global::<DonneesReglagesServeur>()
                .get_onglets()
                .iter()
                .map(|o| o.id.to_string())
                .collect();
            onglets == ["vue", "emojis", "membres"]
        },
        "rôles et salons disparus des réglages",
    );
    println!("365 C3 : une permission changée par un autre s'applique en temps réel");
}

fn scenario(
    faible: slint::Weak<Principale>,
    serveur: Arc<ServeurMessages>,
    coquille: Arc<Coquille>,
    fabrique: Arc<FausseFabrique>,
) {
    attendre_l_interface(
        &faible,
        |f| {
            f.get_page() == Page::Coquille
                && f.global::<DonneesCoquille>().get_vue().titre == "général"
        },
        "salon ouvert",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    hierarchie(&faible, &serveur);
    modifier_un_role(&faible, &serveur);
    creer_ordonner_supprimer(&faible, &serveur);
    attribuer(&faible, &serveur);
    garde_fou(&faible, &serveur);
    permissions_d_un_salon(&faible, &serveur);
    temps_reel(&faible, &serveur, &fabrique);
}

#[test]
fn gerer_les_roles_et_les_permissions() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(20);
    let etat = etat_des_roles();
    *serveur.detail_s1.lock().unwrap() =
        Some(serde_json::to_value(etat.details.get("s1").unwrap()).unwrap());
    *serveur.surcharges.lock().unwrap() = vec![
        json!({"channel_id": "c1", "role_id": "r0", "allow": [], "deny": ["send_messages"]}),
        json!({"channel_id": "c1", "role_id": "rm", "allow": ["send_messages"], "deny": []}),
    ];
    let dossier = tempfile::tempdir().unwrap();
    let memoire = Memoire::nouvelle(
        Preferences {
            derniere_vue: Some(DerniereVue::enregistrer("u1", "#/s/s1/c1")),
            ..Preferences::default()
        },
        dossier.path().to_path_buf(),
        None,
    );
    let fenetre = creer_fenetre().unwrap();
    fenetre
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 900.0));
    let canal = entree::canal();
    ecrans::brancher(&fenetre, &canal.0);
    let entree = Entree::demarrer(
        Machine::nouvelle(None),
        commun::services(
            FausseAuth::avec_session(true),
            FausseApi::compte_actif(),
            Arc::new(FauxNavigateur::default()),
        ),
        canal,
        ecrans::publieur(&fenetre),
    )
    .unwrap();
    let fabrique = FausseFabrique::sur(etat, Vec::new(), &serveur.adresse);
    let midi = NaiveDate::from_ymd_opt(2026, 11, 19)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let coquille = Arc::new(Coquille::brancher(
        &fenetre,
        &entree,
        fabrique.clone(),
        memoire,
        Arc::new(move || midi),
    ));
    fenetre.show().unwrap();
    let faible = fenetre.as_weak();
    let c = coquille.clone();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, serveur, c, fabrique)));
        let _ = slint::quit_event_loop();
        resultat
    });
    slint::run_event_loop_until_quit().unwrap();
    let resultat = pilote.join().unwrap();
    drop(coquille);
    drop(entree);
    if let Err(panique) = resultat {
        resume_unwind(panique);
    }
}
