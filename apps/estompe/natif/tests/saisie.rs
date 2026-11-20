mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, png, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::brouillons::Brouillons;
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::pieces::{ATTENDRE_LES_FICHIERS, FICHIERS_EN_ECHEC};
use estompe_client::preferences::Preferences;
use estompe_natif::coquille::{Coquille, Depot};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesCoquille, DonneesMessagerie, DonneesSaisie, DonneesSelecteur,
    GenreBandeau, LigneMessage, Page, PieceSaisie, Principale,
};
use estompe_plateforme::ContenuPressePapiers;
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

fn pieces(f: &Principale) -> Vec<PieceSaisie> {
    f.global::<DonneesSaisie>().get_pieces().iter().collect()
}

fn cliquer(faible: &slint::Weak<Principale>, libelle: &str) {
    let libelle = libelle.to_string();
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_accessible_label(f, &libelle)
            .next()
            .unwrap_or_else(|| panic!("élément « {libelle} »"))
            .mock_single_click(PointerEventButton::Left);
    });
}

fn coller(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        f.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        appuyer(f, "v");
        f.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
    });
}

fn erreur(f: &Principale) -> String {
    f.global::<DonneesSaisie>().get_erreur().to_string()
}

fn pieces_jointes(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
    coquille: &Coquille,
    dossier: &std::path::Path,
) {
    let notes = dossier.join("notes.txt");
    std::fs::write(&notes, "des notes").unwrap();
    let photo = dossier.join("photo.png");
    std::fs::write(&photo, png([200, 40, 90])).unwrap();
    let gros = dossier.join("gros.bin");
    std::fs::File::create(&gros)
        .unwrap()
        .set_len(11 * 1024 * 1024)
        .unwrap();
    let moyen = dossier.join("moyen.bin");
    std::fs::write(&moyen, vec![1u8; 2 * 1024 * 1024]).unwrap();

    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_joindre_possible(),
        "bouton « Joindre un fichier » proposé",
    );
    *fabrique.fichiers.choix.lock().unwrap() = vec![notes.clone(), photo.clone()];
    cliquer(faible, "Joindre un fichier");
    attendre_l_interface(
        faible,
        |f| {
            let p = pieces(f);
            p.len() == 2
                && p.iter().all(|p| !p.en_cours && !p.echec)
                && p[0].nom == "notes.txt"
                && p[0].detail == "9 o"
                && !p[0].a_vignette
                && p[1].a_vignette
        },
        "deux pièces envoyées, aperçu de l'image, nom et taille du fichier",
    );
    assert_eq!(serveur.nombre_de("POST /files"), 2);

    let avant = serveur.nombre_de("POST /files");
    *fabrique.fichiers.choix.lock().unwrap() = vec![gros.clone()];
    cliquer(faible, "Joindre un fichier");
    attendre_l_interface(
        faible,
        |f| erreur(f) == "Fichier trop gros : « gros.bin » fait 11.0 Mo, 10 Mo maximum.",
        "fichier trop gros refusé avec la limite",
    );
    assert_eq!(sur_l_interface(faible, |f| pieces(f).len()), 2);
    assert_eq!(serveur.nombre_de("POST /files"), avant, "rien n'est envoyé");
    println!("316 C2 : fichier de 11 Mo refusé avant tout envoi, limite de 10 Mo affichée");

    cliquer_le_champ(faible, "Écrire dans #général");
    taper(faible, "avec pièces");
    touche(faible, Key::Return);
    attendre(
        || serveur.envois().last().map(String::as_str) == Some("avec pièces"),
        "message envoyé avec ses pièces",
    );
    let ids = serveur
        .identifiants_envoyes("POST /channels/c1/messages")
        .pop()
        .unwrap();
    assert_eq!(
        ids.iter()
            .map(|id| serveur.nom_de_piece(id))
            .collect::<Vec<_>>(),
        ["notes.txt", "photo.png"],
        "pièces dans l'ordre du champ"
    );
    attendre_l_interface(
        faible,
        |f| pieces(f).is_empty() && derniere_ligne(f).is_some_and(|l| !l.id.starts_with("local:")),
        "pièces retirées du champ après l'envoi",
    );

    *serveur.refus_de_fichier.lock().unwrap() = Some(507);
    *fabrique.fichiers.choix.lock().unwrap() = vec![notes.clone()];
    cliquer(faible, "Joindre un fichier");
    attendre_l_interface(
        faible,
        |f| {
            pieces(f)
                .first()
                .is_some_and(|p| p.echec && p.detail == "échec : Stockage plein")
        },
        "échec de l'envoi expliqué",
    );
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| erreur(f) == FICHIERS_EN_ECHEC,
        "un fichier en échec bloque l'envoi",
    );
    *serveur.refus_de_fichier.lock().unwrap() = None;
    cliquer(faible, "Réessayer notes.txt");
    attendre_l_interface(
        faible,
        |f| pieces(f).first().is_some_and(|p| !p.echec && !p.en_cours),
        "reprise après l'échec",
    );
    cliquer(faible, "Retirer notes.txt");
    attendre_l_interface(faible, |f| pieces(f).is_empty(), "pièce retirée");

    let messages = serveur.envois().len();
    *serveur.fichiers_bloques.lock().unwrap() = true;
    *fabrique.presse_papiers.contenu.lock().unwrap() =
        Some(ContenuPressePapiers::Fichiers(vec![moyen.clone()]));
    cliquer_le_champ(faible, "Écrire dans #général");
    coller(faible);
    attendre_l_interface(
        faible,
        |f| {
            pieces(f)
                .first()
                .is_some_and(|p| p.en_cours && p.nom == "moyen.bin")
        },
        "fichier collé en cours d'envoi",
    );
    attendre(
        || serveur.nombre_de("POST /files bloqué") == 1,
        "envoi reçu par le serveur",
    );
    taper(faible, "pas encore");
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| erreur(f) == ATTENDRE_LES_FICHIERS,
        "le message attend la fin des envois",
    );
    cliquer(faible, "Annuler l'envoi de moyen.bin");
    attendre(
        || serveur.nombre_de("FERMÉE /files") == 1,
        "connexion de l'envoi fermée par l'annulation",
    );
    attendre_l_interface(faible, |f| pieces(f).is_empty(), "pièce annulée retirée");
    assert_eq!(serveur.envois().len(), messages, "aucun message envoyé");
    attendre(
        || {
            coquille
                .pont()
                .unwrap()
                .messagerie()
                .saisie()
                .televersements_en_cours()
                == 0
        },
        "plus aucun envoi en cours",
    );
    println!("316 C3 : annuler l'envoi ferme la connexion et n'envoie pas le message");
    *serveur.fichiers_bloques.lock().unwrap() = false;
    sur_l_interface(faible, |f| {
        f.global::<DonneesSaisie>().set_texte("".into());
    });

    let image = image::RgbaImage::from_pixel(30, 20, image::Rgba([10, 120, 200, 255]));
    *fabrique.presse_papiers.contenu.lock().unwrap() =
        Some(ContenuPressePapiers::Image(estompe_plateforme::Image {
            largeur: 30,
            hauteur: 20,
            rgba: image.into_raw(),
        }));
    coller(faible);
    attendre_l_interface(
        faible,
        |f| {
            pieces(f)
                .first()
                .is_some_and(|p| p.nom == "capture.png" && p.a_vignette && !p.en_cours)
        },
        "capture collée envoyée en PNG",
    );
    *fabrique.presse_papiers.contenu.lock().unwrap() = None;
    coller(faible);
    assert_eq!(sur_l_interface(faible, |f| pieces(f).len()), 1);

    sur_l_interface(faible, {
        let depot = coquille.suivi_du_depot();
        move |f| depot(f, Depot::Survol(true))
    });
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_depot_survole(),
        "zone de dépôt affichée au survol",
    );
    let chemin = photo.clone();
    sur_l_interface(faible, {
        let depot = coquille.suivi_du_depot();
        move |f| depot(f, Depot::Fichiers(vec![chemin.clone()]))
    });
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesSaisie>().get_depot_survole()
                && pieces(f).len() == 2
                && pieces(f).iter().all(|p| !p.en_cours)
        },
        "fichier déposé sur la fenêtre puis envoyé",
    );
    touche(faible, Key::Return);
    attendre(
        || {
            serveur
                .identifiants_envoyes("POST /channels/c1/messages")
                .last()
                .is_some_and(|ids| ids.len() == 2)
                && serveur.envois().last().map(String::as_str) == Some("")
        },
        "message sans texte avec deux pièces",
    );
    attendre_l_interface(
        faible,
        |f| {
            derniere_ligne(f).is_some_and(|l| {
                !l.id.starts_with("local:") && l.annexes.pieces_jointes.row_count() == 2
            })
        },
        "message du serveur avec ses pièces",
    );
    println!(
        "316 : choix, collage de fichiers et d'une capture, dépôt, aperçu, reprise après échec"
    );

    let dernier = sur_l_interface(faible, |f| derniere_ligne(f).unwrap().id.to_string());
    touche(faible, Key::UpArrow);
    attendre_l_interface(
        faible,
        |f| {
            f.global::<DonneesSaisie>().get_bandeau().genre == GenreBandeau::Modification
                && pieces(f).len() == 2
        },
        "modification avec les pièces du message",
    );
    cliquer(faible, "Retirer capture.png");
    attendre_l_interface(faible, |f| pieces(f).len() == 1, "pièce retirée du message");
    touche(faible, Key::Return);
    attendre(
        || {
            serveur
                .identifiants_envoyes(&format!("PATCH /messages/{dernier}"))
                .last()
                .is_some_and(|ids| ids.len() == 1)
        },
        "liste finale des pièces envoyée",
    );
    attendre_l_interface(
        faible,
        |f| {
            derniere_ligne(f)
                .is_some_and(|l| l.modifie && l.annexes.pieces_jointes.row_count() == 1)
        },
        "message modifié avec une seule pièce",
    );
    println!("316 : pièces jointes d'un message modifiées (EditAttachments)");
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
    fabrique: Arc<FausseFabrique>,
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
    sur_l_interface(&faible, |f| {
        f.global::<DonneesSaisie>().set_texte("".into());
    });
    pieces_jointes(&faible, &serveur, &fabrique, &coquille, &dossier);
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
        let resultat = catch_unwind(AssertUnwindSafe(|| {
            scenario(faible, serveur, c, chemin, fabrique)
        }));
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
