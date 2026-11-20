mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::brouillons::Brouillons;
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::Coquille;
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesMessagerie, DonneesSaisie, DonneesSelecteur,
    GenreBandeau, LigneMessage, Page, Principale,
};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

const NOMBRE: u64 = 300;

fn lignes(f: &Principale) -> Vec<LigneMessage> {
    f.global::<DonneesMessagerie>()
        .get_lignes()
        .iter()
        .collect()
}

fn derniere_ligne(f: &Principale) -> Option<LigneMessage> {
    lignes(f).into_iter().rev().find(|l| !l.id.is_empty())
}

fn appuyer(f: &Principale, texte: impl Into<SharedString>) {
    let texte = texte.into();
    f.window().dispatch_event(WindowEvent::KeyPressed {
        text: texte.clone(),
    });
    f.window()
        .dispatch_event(WindowEvent::KeyReleased { text: texte });
}

fn taper(faible: &slint::Weak<Principale>, texte: &str) {
    let texte = texte.to_string();
    sur_l_interface(faible, move |f| {
        for c in texte.chars() {
            appuyer(f, c.to_string());
        }
    });
}

fn touche(faible: &slint::Weak<Principale>, touche: Key) {
    sur_l_interface(faible, move |f| appuyer(f, touche));
}

fn maj_entree(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        f.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        appuyer(f, Key::Return);
        f.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    });
}

fn texte(faible: &slint::Weak<Principale>) -> String {
    sur_l_interface(faible, |f| {
        f.global::<DonneesSaisie>().get_texte().to_string()
    })
}

fn cliquer_le_champ(faible: &slint::Weak<Principale>, indice: &'static str) {
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, indice)
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .expect("champ de saisie")
            .mock_single_click(PointerEventButton::Left);
    });
}

fn propositions(f: &Principale) -> Vec<String> {
    f.global::<DonneesSaisie>()
        .get_propositions()
        .iter()
        .map(|p| p.libelle.to_string())
        .collect()
}

fn ouvrir_le_selecteur_au_clavier(faible: &slint::Weak<Principale>) {
    touche(faible, Key::Tab);
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSelecteur>();
            g.get_ouvert() && g.get_rangees().row_count() > 0
        },
        "sélecteur ouvert par Tab puis Entrée",
    );
}

fn mentions_et_selecteur(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    dossier: &std::path::Path,
) {
    taper(faible, "Salut @ca");
    attendre_l_interface(
        faible,
        |f| propositions(f) == ["@carol"],
        "@ propose les personnes",
    );
    assert_eq!(
        sur_l_interface(faible, |f| f
            .global::<DonneesSaisie>()
            .get_titre_propositions()
            .to_string()),
        "Mentionner"
    );
    touche(faible, Key::Tab);
    attendre_l_interface(
        faible,
        |f| {
            propositions(f).is_empty() && f.global::<DonneesSaisie>().get_texte() == "Salut @carol "
        },
        "Tab insère la mention",
    );
    taper(faible, "#gé");
    attendre_l_interface(
        faible,
        |f| propositions(f) == ["#général"],
        "# propose les salons",
    );
    touche(faible, Key::Escape);
    attendre_l_interface(
        faible,
        |f| propositions(f).is_empty(),
        "Échap ferme la liste",
    );
    assert_eq!(texte(faible), "Salut @carol #gé");
    taper(faible, " :jo");
    attendre_l_interface(
        faible,
        |f| propositions(f).iter().any(|p| p == ":joy:"),
        ": propose les smileys",
    );
    let rang = sur_l_interface(faible, |f| {
        propositions(f).iter().position(|p| p == ":joy:").unwrap()
    });
    for _ in 0..rang {
        touche(faible, Key::DownArrow);
    }
    attendre_l_interface(
        faible,
        move |f| f.global::<DonneesSaisie>().get_proposition_choisie() == rang as i32,
        "flèche bas parcourt la liste",
    );
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "Salut @carol #gé 😂",
        "Entrée insère le smiley",
    );
    taper(faible, " :thumbsup:");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "Salut @carol #gé 😂 👍",
        "conversion de :thumbsup: à la saisie",
    );
    println!("315 : @, # et : proposés, flèches, Tab, Entrée, Échap ; :thumbsup: converti");

    ouvrir_le_selecteur_au_clavier(faible);
    taper(faible, "joy");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSelecteur>();
            g.get_rangees()
                .row_data(0)
                .and_then(|r| r.cellules.row_data(0))
                .is_some_and(|c| c.libelle == "visage riant aux larmes")
        },
        "recherche par code",
    );
    touche(faible, Key::DownArrow);
    touche(faible, Key::RightArrow);
    touche(faible, Key::LeftArrow);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSelecteur>();
            g.get_actif() == 0 && g.get_pied_code() == ":joy:"
        },
        "flèches dans la grille, nom et code en pied",
    );
    maj_entree(faible);
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesSelecteur>().get_ouvert()
                && f.global::<DonneesSaisie>().get_texte().ends_with("👍😂")
        },
        "Maj+Entrée insère sans fermer",
    );
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesSelecteur>().get_ouvert()
                && f.global::<DonneesSaisie>().get_texte() == "Salut @carol #gé 😂 👍😂😂"
        },
        "Entrée insère et ferme",
    );
    attendre(
        || {
            estompe_client::preferences::Preferences::charger(dossier)
                .preferences
                .emoji_recents
                == ["😂"]
        },
        "récents enregistrés dans les préférences",
    );

    ouvrir_le_selecteur_au_clavier(faible);
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesSelecteur>()
                .get_onglets()
                .row_data(0)
                .is_some_and(|o| o.libelle == "Récents")
        },
        "onglet des récents en tête",
    );
    touche(faible, Key::Tab);
    touche(faible, Key::RightArrow);
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSelecteur>();
            g.get_onglet_actif() == 1 && g.get_pied_nom() == "visage rieur"
        },
        "Tab, flèche et Entrée vont à une catégorie",
    );
    touche(faible, Key::DownArrow);
    touche(faible, Key::Escape);
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesSelecteur>().get_ouvert(),
        "Échap ferme le sélecteur",
    );
    taper(faible, "!");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "Salut @carol #gé 😂 👍😂😂!",
        "le focus revient au champ",
    );
    touche(faible, Key::Return);
    attendre(
        || serveur.envois().last().map(String::as_str) == Some("Salut @carol #gé 😂 👍😂😂!"),
        "message envoyé",
    );
    println!("315 C3 : sélecteur ouvert, parcouru et refermé au clavier, récents gardés");
}

fn brouillon_du_salon(f: &Principale, salon: &str) -> bool {
    f.global::<DonneesCoquille>()
        .get_salons()
        .iter()
        .find(|s| s.id == salon)
        .is_some_and(|s| s.brouillon)
}

fn scenario(
    faible: slint::Weak<Principale>,
    serveur: Arc<ServeurMessages>,
    coquille: Arc<Coquille>,
    dossier: std::path::PathBuf,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille && f.global::<DonneesMessagerie>().get_active(),
        "vue des messages ouverte",
    );
    attendre(|| coquille.pont().is_some(), "pont de la session");
    let pont = coquille.pont().unwrap();
    attendre(
        || pont.messagerie().nombre_de_lignes() > 40,
        "première page chargée",
    );
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_indice() == "Écrire dans #général",
        "indice du champ",
    );

    cliquer_le_champ(&faible, "Écrire dans #général");
    taper(&faible, "Bonjour à tous");
    maj_entree(&faible);
    taper(&faible, "deuxième ligne");
    assert_eq!(texte(&faible), "Bonjour à tous\ndeuxième ligne");
    touche(&faible, Key::Return);
    assert_eq!(texte(&faible), "", "le champ se vide à l'envoi");
    attendre(
        || serveur.envois() == ["Bonjour à tous\ndeuxième ligne"],
        "message reçu par le serveur",
    );
    attendre_l_interface(
        &faible,
        |f| derniere_ligne(f).is_some_and(|l| l.id == "301" && l.etat_envoi.is_empty()),
        "message du serveur en bas de la liste",
    );
    attendre(|| pont.messagerie().colle(), "liste collée en bas");
    println!("314 C2 : envoi par Entrée, Maj+Entrée ajoute une ligne, message 301 en bas");

    touche(&faible, Key::UpArrow);
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesSaisie>();
            g.get_bandeau().genre == GenreBandeau::Modification
                && g.get_texte() == "Bonjour à tous\ndeuxième ligne"
        },
        "flèche haut : modification du dernier message",
    );
    touche(&faible, Key::Escape);
    attendre_l_interface(
        &faible,
        |f| {
            let g = f.global::<DonneesSaisie>();
            g.get_bandeau().genre == GenreBandeau::Aucun && g.get_texte().is_empty()
        },
        "Échap annule la modification",
    );
    touche(&faible, Key::UpArrow);
    sur_l_interface(&faible, |f| {
        f.global::<DonneesSaisie>()
            .set_texte("Bonjour à tous, modifié".into());
    });
    touche(&faible, Key::Return);
    attendre(
        || serveur.nombre_de("PATCH /messages/301") == 1,
        "modification envoyée",
    );
    attendre_l_interface(
        &faible,
        |f| {
            derniere_ligne(f)
                .is_some_and(|l| l.modifie && l.texte_brut == "Bonjour à tous, modifié")
        },
        "message modifié dans la liste",
    );

    *serveur.refus_d_envoi.lock().unwrap() = Some(403);
    taper(&faible, "pas le droit");
    touche(&faible, Key::Return);
    attendre_l_interface(
        &faible,
        |f| {
            derniere_ligne(f).is_some_and(|l| {
                l.envoi_echec
                    && l.etat_envoi.starts_with("Non envoyé")
                    && l.id.starts_with("local:")
            })
        },
        "message non envoyé avec sa raison",
    );
    let local = sur_l_interface(&faible, |f| derniere_ligne(f).unwrap().id.to_string());
    *serveur.refus_d_envoi.lock().unwrap() = None;
    sur_l_interface(&faible, move |f| {
        f.global::<DonneesMessagerie>()
            .invoke_reessayer_envoi(local.as_str().into())
    });
    attendre_l_interface(
        &faible,
        |f| derniere_ligne(f).is_some_and(|l| l.id == "302" && !l.envoi_echec),
        "réessayer envoie le message",
    );

    mentions_et_selecteur(&faible, &serveur, &dossier);

    taper(&faible, "brouillon à garder");
    attendre_l_interface(
        &faible,
        |f| brouillon_du_salon(f, "c1"),
        "marque du brouillon dans le panneau",
    );
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_salon("c4".into())
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_indice() == "Écrire dans #photos",
        "autre salon ouvert",
    );
    assert_eq!(texte(&faible), "");
    sur_l_interface(&faible, |f| {
        f.global::<DonneesCoquille>()
            .invoke_ouvrir_salon("c1".into())
    });
    attendre_l_interface(
        &faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "brouillon à garder",
        "brouillon retrouvé au retour",
    );
    attendre(
        || Brouillons::charger(&dossier).texte("c:c1") == "brouillon à garder",
        "brouillon enregistré sur le disque pour le prochain lancement",
    );
    println!("314 C3 : brouillon retrouvé au retour dans le salon et relu depuis le disque");
}

#[test]
fn saisie_envoi_modification_et_brouillons() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let serveur = ServeurMessages::demarrer(NOMBRE);
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
    let fabrique = FausseFabrique::sur(etat_avec_membres(), Vec::new(), &serveur.adresse);
    *fabrique.dossier_brouillons.lock().unwrap() = Some(dossier.path().to_path_buf());
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
    let chemin = dossier.path().to_path_buf();
    let pilote = std::thread::spawn(move || {
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, serveur, c, chemin)));
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
