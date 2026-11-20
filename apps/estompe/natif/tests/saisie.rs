mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::NaiveDate;
use commun::coquille::{etat_avec_membres, gif_anime, png, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::brouillons::Brouillons;
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::gifs::TROP_DE_RECHERCHES;
use estompe_client::pieces::{ATTENDRE_LES_FICHIERS, FICHIERS_EN_ECHEC};
use estompe_client::preferences::Preferences;
use estompe_client::video::{codec_mp4, Conversions, Convertisseur, DOSSIER_CONVERSIONS};
use estompe_natif::coquille::{Coquille, Depot, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{
    creer_fenetre, ecrans, DonneesConfirmation, DonneesCopie, DonneesCoquille,
    DonneesCreationSondage, DonneesGifs, DonneesMenu, DonneesMessagerie, DonneesSaisie,
    DonneesSelecteur, GenreBandeau, GenreLigneMessage, LigneMessage, Page, PieceSaisie, Principale,
    SondageMessage,
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
                == ["😂", "👍"]
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
    let lien = format!("{}/#/s/s1/c1/299", serveur.adresse);
    attendre_l_interface(
        faible,
        move |f| f.global::<DonneesSaisie>().get_texte() == lien.as_str(),
        "sans fichier ni image, Ctrl+V colle le texte copié par « Copier le lien »",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesSaisie>().set_texte("".into());
    });

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
    conversion_video(faible, serveur, fabrique, dossier);
}

fn fichiers_de(dossier: &std::path::Path) -> usize {
    std::fs::read_dir(dossier).map_or(0, |d| d.count())
}

fn conversion_video(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
    dossier: &std::path::Path,
) {
    let hevc = dossier.join("hevc.mp4");
    if fabrique.conversions.lock().unwrap().is_none() {
        println!("317 : conversion ignorée, ffmpeg indisponible");
        return;
    }
    let conversions = dossier.join(DOSSIER_CONVERSIONS);
    let avant = serveur.nombre_de("POST /files");
    *fabrique.fichiers.choix.lock().unwrap() = vec![hevc.clone()];
    cliquer(faible, "Joindre un fichier");
    attendre_l_interface(
        faible,
        |f| {
            pieces(f)
                .first()
                .is_some_and(|p| p.nom == "hevc.mp4" && !p.en_cours && !p.echec)
        },
        "vidéo convertie puis envoyée",
    );
    assert_eq!(serveur.nombre_de("POST /files"), avant + 1);
    let (id, contenu) = serveur
        .contenus
        .lock()
        .unwrap()
        .last_key_value()
        .map(|(id, c)| (id.clone(), c.clone()))
        .unwrap();
    assert_eq!(serveur.nom_de_piece(&id), "hevc.mp4");
    assert_eq!(
        codec_mp4(&mut std::io::Cursor::new(&contenu)).unwrap(),
        Some("avc1".into()),
        "vidéo reçue en H.264"
    );
    assert_eq!(
        fichiers_de(&conversions),
        1,
        "fichier converti gardé pour l'envoi"
    );
    cliquer_le_champ(faible, "Écrire dans #général");
    taper(faible, "une vidéo");
    touche(faible, Key::Return);
    attendre(
        || serveur.envois().last().map(String::as_str) == Some("une vidéo"),
        "message envoyé avec la vidéo",
    );
    attendre_l_interface(faible, |f| pieces(f).is_empty(), "vidéo retirée du champ");
    attendre(
        || fichiers_de(&conversions) == 0,
        "fichier converti supprimé après l'envoi",
    );
    println!(
        "317 C1 : vidéo HEVC jointe, envoyée en H.264 (kind=video), fichier temporaire supprimé"
    );

    let messages = serveur.envois().len();
    *serveur.fichiers_bloques.lock().unwrap() = true;
    *fabrique.fichiers.choix.lock().unwrap() = vec![hevc.clone()];
    cliquer(faible, "Joindre un fichier");
    attendre_l_interface(
        faible,
        |f| pieces(f).first().is_some_and(|p| p.en_cours),
        "vidéo en cours de conversion ou d'envoi",
    );
    cliquer(faible, "Annuler l'envoi de hevc.mp4");
    attendre_l_interface(faible, |f| pieces(f).is_empty(), "vidéo annulée retirée");
    attendre(
        || fichiers_de(&conversions) == 0,
        "aucun fichier temporaire après l'annulation",
    );
    assert_eq!(serveur.envois().len(), messages, "aucun message envoyé");
    *serveur.fichiers_bloques.lock().unwrap() = false;
    assert!(hevc.is_file(), "l'original n'est pas touché");
    println!("317 C3 : conversion ou envoi annulé, aucun fichier temporaire");
}

fn traiter_les_changements(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |_| slint::platform::update_timers_and_animations());
}

fn entrees_du_menu(f: &Principale) -> Vec<String> {
    let g = f.global::<DonneesMenu>();
    if !g.get_ouvert() {
        return Vec::new();
    }
    g.get_entrees().iter().map(|e| e.id.to_string()).collect()
}

fn cliquer_message(
    faible: &slint::Weak<Principale>,
    texte: &'static str,
    bouton: PointerEventButton,
) {
    sur_l_interface(faible, move |f| {
        let e = ElementHandle::find_by_element_type_name(f, "CorpsMessage")
            .find(|e| e.accessible_description().is_some_and(|d| d == texte))
            .expect("message affiché");
        let position = e.absolute_position();
        let position =
            slint::LogicalPosition::new(position.x + 66.0, position.y + e.size().height / 2.0);
        f.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        f.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: bouton,
        });
        f.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: bouton,
        });
    });
}

fn menu_du_message(faible: &slint::Weak<Principale>, texte: &'static str, maj_f10: bool) {
    traiter_les_changements(faible);
    cliquer_message(faible, texte, PointerEventButton::Left);
    if maj_f10 {
        sur_l_interface(faible, |f| {
            f.window().dispatch_event(WindowEvent::KeyPressed {
                text: Key::Shift.into(),
            });
            appuyer(f, Key::F10);
            f.window().dispatch_event(WindowEvent::KeyReleased {
                text: Key::Shift.into(),
            });
        });
    } else {
        touche(faible, Key::Menu);
    }
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesMenu>().get_ouvert(),
        "menu du message ouvert au clavier",
    );
    traiter_les_changements(faible);
}

fn choisir_au_clavier(faible: &slint::Weak<Principale>, action: &'static str) {
    let rang = sur_l_interface(faible, move |f| {
        entrees_du_menu(f)
            .iter()
            .position(|e| e == action)
            .unwrap_or_else(|| panic!("entrée {action} dans {:?}", entrees_du_menu(f)))
    });
    for _ in 0..rang {
        touche(faible, Key::DownArrow);
    }
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesMenu>().get_ouvert(),
        "menu refermé après le choix",
    );
}

fn reactions_de(f: &Principale, id: &str) -> Vec<(String, bool)> {
    lignes(f)
        .into_iter()
        .find(|l| l.id == id)
        .map(|l| {
            l.annexes
                .reactions
                .iter()
                .map(|r| (r.emoji.to_string(), r.moi))
                .collect()
        })
        .unwrap_or_default()
}

fn non_lus_du_salon(f: &Principale, salon: &str) -> i32 {
    f.global::<DonneesCoquille>()
        .get_salons()
        .iter()
        .find(|s| s.id == salon)
        .map_or(0, |s| s.non_lus)
}

fn actions_et_menus(faible: &slint::Weak<Principale>, serveur: &ServeurMessages, pont: &Pont) {
    let message = "message n°299";
    cliquer_message(faible, "message n°298", PointerEventButton::Right);
    attendre_l_interface(
        faible,
        |f| entrees_du_menu(f).first().is_some_and(|e| e == "repondre"),
        "clic droit sur un message : menu ouvert au curseur",
    );
    traiter_les_changements(faible);
    touche(faible, Key::Escape);
    menu_du_message(faible, message, false);
    let entrees = sur_l_interface(faible, entrees_du_menu);
    assert_eq!(
        entrees,
        [
            "repondre",
            "epingler",
            "reagir",
            "copier-texte",
            "copier-lien",
            "non-lu",
            "signaler",
            "supprimer"
        ],
        "menu complet sur le message d'un autre avec le droit de gérer les messages"
    );
    touche(faible, Key::Escape);
    attendre_l_interface(
        faible,
        |f| !f.global::<DonneesMenu>().get_ouvert(),
        "Échap ferme le menu",
    );
    menu_du_message(faible, message, true);
    choisir_au_clavier(faible, "repondre");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_bandeau().genre == GenreBandeau::Reponse,
        "Répondre ouvre le bandeau de réponse",
    );
    touche(faible, Key::Escape);

    menu_du_message(faible, message, false);
    choisir_au_clavier(faible, "epingler");
    attendre(
        || serveur.nombre_de("PUT /messages/299/pin") == 1,
        "épinglage envoyé",
    );

    menu_du_message(faible, message, false);
    choisir_au_clavier(faible, "reagir");
    attendre_l_interface(
        faible,
        |f| {
            let g = f.global::<DonneesSelecteur>();
            g.get_ouvert() && g.get_rangees().row_count() > 0
        },
        "le sélecteur s'ouvre pour une réaction",
    );
    taper(faible, "thumbsup");
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| reactions_de(f, "299") == [("👍".to_string(), true)],
        "réaction ajoutée par le sélecteur",
    );
    attendre(
        || serveur.nombre_de("PUT /messages/299/reactions/") == 1,
        "réaction envoyée",
    );
    cliquer_message(faible, message, PointerEventButton::Left);
    touche(faible, Key::Tab);
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| reactions_de(f, "299").is_empty(),
        "Entrée sur la réaction la retire",
    );
    attendre(
        || serveur.nombre_de("DELETE /messages/299/reactions/") == 1,
        "retrait de la réaction envoyé",
    );

    menu_du_message(faible, message, false);
    choisir_au_clavier(faible, "copier-texte");
    assert_eq!(
        sur_l_interface(faible, |f| f
            .global::<DonneesCopie>()
            .get_texte()
            .to_string()),
        message
    );
    menu_du_message(faible, message, false);
    choisir_au_clavier(faible, "copier-lien");
    let lien = format!("{}/#/s/s1/c1/299", serveur.adresse);
    assert_eq!(
        sur_l_interface(faible, |f| f
            .global::<DonneesCopie>()
            .get_texte()
            .to_string()),
        lien
    );

    menu_du_message(faible, message, false);
    choisir_au_clavier(faible, "non-lu");
    attendre_l_interface(
        faible,
        |f| {
            non_lus_du_salon(f, "c1") > 0
                && lignes(f)
                    .iter()
                    .any(|l| l.genre == GenreLigneMessage::Nouveau)
        },
        "marqué comme non lu : compteur du salon et séparateur",
    );
    sur_l_interface(faible, |f| {
        ElementHandle::find_by_accessible_label(f, "général")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("salon dans le panneau")
            .mock_single_click(PointerEventButton::Right);
    });
    attendre_l_interface(
        faible,
        |f| entrees_du_menu(f) == ["lu", "lien"],
        "menu contextuel du salon",
    );
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| non_lus_du_salon(f, "c1") == 0,
        "« Marquer comme lu » depuis le panneau",
    );

    menu_du_message(faible, message, false);
    touche(faible, Key::End);
    touche(faible, Key::Return);
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "Supprimer demande confirmation",
    );
    traiter_les_changements(faible);
    touche(faible, Key::Tab);
    touche(faible, Key::Tab);
    touche(faible, Key::Return);
    attendre(
        || {
            serveur
                .requetes
                .lock()
                .unwrap()
                .iter()
                .any(|r| r == "DELETE /messages/299")
        },
        "suppression envoyée après confirmation",
    );
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesConfirmation>().get_ouverte()
                && !lignes(f).iter().any(|l| l.id == "299")
        },
        "message retiré de la liste",
    );

    sur_l_interface(faible, |f| {
        f.global::<DonneesMessagerie>().invoke_aller_en_bas()
    });
    attendre(|| pont.messagerie().colle(), "liste revenue en bas");
    traiter_les_changements(faible);
    menu_du_message(faible, "Bonjour à tous, modifié", false);
    choisir_au_clavier(faible, "modifier");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_bandeau().genre == GenreBandeau::Modification,
        "Modifier ouvre le bandeau de modification",
    );
    touche(faible, Key::Escape);

    sur_l_interface(faible, |f| {
        ElementHandle::find_by_accessible_label(f, "Caro")
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .expect("carol dans la liste des membres")
            .mock_single_click(PointerEventButton::Right);
    });
    attendre_l_interface(
        faible,
        |f| entrees_du_menu(f) == ["profil", "mentionner", "pseudo"],
        "menu contextuel d'un membre",
    );
    choisir_au_clavier(faible, "mentionner");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesSaisie>().get_texte() == "@carol ",
        "« Mentionner » insère la mention dans le champ",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesSaisie>().set_texte("".into());
    });
    println!("318 C3 : menu au clavier (Menu, Maj+F10), répondre, épingler, réagir, retirer, copier, non lu, supprimer confirmé, modifier ; menus du panneau et des membres");
}

fn sondage_affiche(f: &Principale) -> Option<(String, SondageMessage)> {
    lignes(f)
        .into_iter()
        .rev()
        .find(|l| l.annexes.sondage.present)
        .map(|l| (l.id.to_string(), l.annexes.sondage))
}

fn options_du_sondage(f: &Principale) -> Vec<(String, i32, bool, String)> {
    sondage_affiche(f)
        .map(|(_, s)| {
            s.options
                .iter()
                .map(|o| (o.libelle.to_string(), o.votes, o.moi, o.votants.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn creation(f: &Principale) -> (bool, String, usize) {
    let g = f.global::<DonneesCreationSondage>();
    (
        g.get_ouvert(),
        g.get_erreur().to_string(),
        g.get_choix().row_count(),
    )
}

fn rendre_visible(faible: &slint::Weak<Principale>, libelle: &'static str) {
    attendre_l_interface(
        faible,
        move |f| {
            slint::platform::update_timers_and_animations();
            f.global::<DonneesMessagerie>().invoke_aller_en_bas();
            let liste = ElementHandle::find_by_accessible_label(f, "Messages")
                .find(|e| e.accessible_role() == Some(AccessibleRole::List))
                .expect("liste des messages");
            let bas = liste.absolute_position().y + liste.size().height;
            ElementHandle::find_by_accessible_label(f, libelle)
                .next()
                .is_some_and(|e| e.absolute_position().y + e.size().height <= bas)
        },
        "élément visible dans la liste",
    );
}

fn sondages(
    faible: &slint::Weak<Principale>,
    serveur: &ServeurMessages,
    fabrique: &FausseFabrique,
) {
    cliquer(faible, "Créer un sondage");
    attendre_l_interface(faible, |f| creation(f).0, "fenêtre de création ouverte");
    traiter_les_changements(faible);
    cliquer(faible, "Publier le sondage");
    attendre_l_interface(
        faible,
        |f| creation(f).1 == "Écris une question.",
        "erreur sans question",
    );
    cliquer_le_champ(faible, "Question");
    taper(faible, "Pizza ou sushi ?");
    attendre_l_interface(
        faible,
        |f| creation(f).1 == "Il faut au moins 2 choix remplis.",
        "l'erreur suit la saisie",
    );
    cliquer_le_champ(faible, "Choix 1");
    taper(faible, "Pizza");
    cliquer_le_champ(faible, "Choix 2");
    taper(faible, "Sushi");
    attendre_l_interface(faible, |f| creation(f).1.is_empty(), "formulaire valide");
    cliquer(faible, "Ajouter un choix");
    attendre_l_interface(faible, |f| creation(f).2 == 3, "troisième choix ajouté");
    cliquer(faible, "Supprimer le choix 3");
    attendre_l_interface(faible, |f| creation(f).2 == 2, "troisième choix retiré");
    cliquer(faible, "Personnalisée");
    cliquer_le_champ(faible, "Date de clôture");
    taper(faible, "01/01/2020 10:00");
    cliquer(faible, "Publier le sondage");
    attendre_l_interface(
        faible,
        |f| {
            creation(f)
                == (
                    true,
                    "La date de clôture doit être dans le futur.".into(),
                    2,
                )
        },
        "échéance passée refusée",
    );
    cliquer(faible, "Sans clôture");
    cliquer(faible, "Publier le sondage");
    attendre_l_interface(
        faible,
        |f| !creation(f).0,
        "fenêtre refermée à la publication",
    );
    attendre(
        || {
            serveur.corps.lock().unwrap().iter().any(|(r, c)| {
                r == "POST /channels/c1/messages"
                    && serde_json::from_str::<serde_json::Value>(c).unwrap()
                        == serde_json::json!({"content": "Pizza ou sushi ?", "file_ids": [], "poll": {"options": ["Pizza", "Sushi"], "multiple": false}})
            })
        },
        "sondage envoyé",
    );
    attendre_l_interface(
        faible,
        |f| {
            sondage_affiche(f).is_some_and(|(id, s)| !id.starts_with("local:") && s.votable)
                && options_du_sondage(f).len() == 2
        },
        "sondage publié dans la liste",
    );
    let id = sur_l_interface(faible, |f| sondage_affiche(f).unwrap().0);
    println!("319 C3 : création au clavier et à la souris, erreurs de validation, sondage publié");

    sur_l_interface(faible, |f| {
        ElementHandle::find_by_element_type_name(f, "OptionVue")
            .find(|e| {
                e.accessible_label()
                    .is_some_and(|l| l.starts_with("Pizza,"))
            })
            .expect("choix Pizza")
            .mock_single_click(PointerEventButton::Left);
    });
    attendre_l_interface(
        faible,
        |f| options_du_sondage(f)[0] == ("Pizza".into(), 1, true, String::new()),
        "vote affiché tout de suite",
    );
    let chemin = format!("PUT /messages/{id}/vote");
    attendre(|| serveur.nombre_de(&chemin) == 1, "vote envoyé");
    let message = id.clone();
    fabrique.envoyer(commun::coquille::evenement(
        "poll.voted",
        serde_json::json!({"message_id": message, "user_id": "u1", "options": ["o1"], "counts": {"o1": 2, "o2": 0}}),
    ));
    attendre_l_interface(
        faible,
        |f| options_du_sondage(f)[0] == ("Pizza".into(), 2, true, String::new()),
        "totaux confirmés par le temps réel",
    );
    println!("319 C2 : vote affiché tout de suite puis confirmé par le temps réel");

    cliquer(faible, "Voir les votants");
    attendre_l_interface(
        faible,
        |f| {
            sondage_affiche(f).is_some_and(|(_, s)| s.votants_ouverts)
                && options_du_sondage(f)[0].3 == "carol, moi"
        },
        "votants affichés",
    );
    rendre_visible(faible, "Masquer les votants");
    cliquer(faible, "Masquer les votants");
    attendre_l_interface(
        faible,
        |f| sondage_affiche(f).is_some_and(|(_, s)| !s.votants_ouverts),
        "votants masqués",
    );
    cliquer(faible, "Clore");
    attendre_l_interface(
        faible,
        |f| sondage_affiche(f).is_some_and(|(_, s)| s.clos && !s.votable),
        "sondage clos",
    );
    assert_eq!(
        serveur.nombre_de(&format!("POST /messages/{id}/poll/close")),
        1
    );
    cliquer(faible, "Supprimer le sondage");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesConfirmation>().get_ouverte(),
        "suppression du sondage à confirmer",
    );
    sur_l_interface(faible, |f| {
        f.global::<DonneesConfirmation>().invoke_repondre(true)
    });
    let suppression = format!("DELETE /messages/{id}");
    attendre(|| serveur.nombre_de(&suppression) == 1, "sondage supprimé");
    attendre_l_interface(
        faible,
        |f| sondage_affiche(f).is_none(),
        "sondage retiré de la liste",
    );
    println!("319 : voir les votants, clore et supprimer depuis la carte");
    cliquer_le_champ(faible, "Écrire dans #général");
}

fn avancer(faible: &slint::Weak<Principale>, duree: std::time::Duration) {
    sur_l_interface(faible, move |_| {
        i_slint_backend_testing::mock_elapsed_time(duree);
        slint::platform::update_timers_and_animations();
    });
}

fn gifs_affiches(f: &Principale) -> Vec<String> {
    f.global::<DonneesGifs>()
        .get_rangees()
        .iter()
        .flat_map(|r| {
            r.cellules
                .iter()
                .map(|c| c.titre.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn animes_et_visibles(pont: &Pont) -> (Vec<usize>, Vec<usize>, usize) {
    let messagerie = pont.messagerie();
    let e = messagerie.gifs().etat();
    (
        e.animees(),
        e.indices_visibles().collect(),
        e.recherche.resultats.len(),
    )
}

fn selecteur_de_gifs(faible: &slint::Weak<Principale>, serveur: &ServeurMessages, pont: &Pont) {
    for n in 0..4 {
        serveur.servir_fichier(&format!("/giphy/{n}.gif"), "image/gif", gif_anime(4));
    }
    let p = pont.clone();
    sur_l_interface(faible, move |f| p.messagerie().repondre_a(f, "300"));
    cliquer(faible, "GIF");
    attendre_l_interface(
        faible,
        |f| gifs_affiches(f).len() == 24 && gifs_affiches(f)[0] == "tendances 0",
        "tendances affichées à l'ouverture",
    );
    assert_eq!(serveur.nombre_de("GET /gifs/trending?offset=0"), 1);
    let avant = serveur.nombre_de("GET /gifs/");
    for c in "chatonsxyz".chars() {
        taper(faible, &c.to_string());
        avancer(faible, std::time::Duration::from_millis(50));
    }
    assert_eq!(
        sur_l_interface(faible, |f| f
            .global::<DonneesGifs>()
            .get_recherche()
            .to_string()),
        "chatonsxyz",
        "le champ de recherche a le focus à l'ouverture"
    );
    assert_eq!(
        serveur.nombre_de("GET /gifs/"),
        avant,
        "aucune requête pendant la frappe"
    );
    avancer(faible, std::time::Duration::from_millis(400));
    attendre_l_interface(
        faible,
        |f| gifs_affiches(f).first().map(String::as_str) == Some("chatonsxyz 0"),
        "résultats de la recherche",
    );
    let requetes = serveur.nombre_de("GET /gifs/");
    assert_eq!(requetes - avant, 1);
    assert_eq!(
        serveur.nombre_de("GET /gifs/search?q=chatonsxyz&offset=0"),
        1
    );
    println!("320 C2 : 10 caractères tapés en 500 ms, {requetes} requêtes en tout (tendances à l'ouverture et recherche)");

    attendre(
        || {
            let (animes, visibles, _) = animes_et_visibles(pont);
            !visibles.is_empty() && animes == visibles
        },
        "aperçus visibles chargés et animés",
    );
    let (animes, visibles, total) = animes_et_visibles(pont);
    assert!(animes.len() < total, "seuls les GIF à l'écran sont animés");
    let changees = || pont.messagerie().gifs().etat().statistiques.images_changees;
    let debut = changees();
    for _ in 0..10 {
        avancer(faible, std::time::Duration::from_millis(100));
    }
    let delta = changees() - debut;
    assert!(delta > 0 && delta <= animes.len() * 10);
    println!("320 C3 : {} GIF visibles sur {total} animés ({visibles:?}), {delta} images changées en 1 s simulée", animes.len());

    let bas = {
        let messagerie = pont.messagerie();
        let e = messagerie.gifs().etat();
        estompe_client::gifs::hauteur_totale(&e.rangees) - 300.0
    };
    sur_l_interface(faible, move |f| {
        f.global::<DonneesGifs>().set_defilement(-bas);
        slint::platform::update_timers_and_animations();
    });
    attendre(
        || serveur.nombre_de("GET /gifs/search?q=chatonsxyz&offset=24") == 1,
        "suite chargée au défilement",
    );
    attendre_l_interface(faible, |f| gifs_affiches(f).len() == 48, "48 GIF affichés");
    attendre(
        || {
            let (animes, visibles, _) = animes_et_visibles(pont);
            !animes.is_empty() && animes == visibles
        },
        "GIF du bas animés",
    );
    let (animes, _, _) = animes_et_visibles(pont);
    assert!(
        !animes.contains(&0),
        "un GIF sorti de l'écran n'est plus animé"
    );
    println!("320 : suite chargée au défilement, le premier GIF sorti de l'écran n'est plus animé ({animes:?})");

    touche(faible, Key::DownArrow);
    touche(faible, Key::RightArrow);
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesGifs>().get_actif() == 1,
        "flèches dans la grille",
    );
    touche(faible, Key::Return);
    attendre(
        || serveur.nombre_de("POST /gifs/import") == 1,
        "GIF importé",
    );
    attendre(
        || {
            serveur
                .identifiants_envoyes("POST /channels/c1/messages")
                .last()
                .is_some_and(|ids| ids.len() == 1)
        },
        "message envoyé avec le GIF en pièce jointe",
    );
    let corps = serveur.corps.lock().unwrap().clone();
    let import = corps
        .iter()
        .find(|(r, _)| r == "POST /gifs/import")
        .unwrap();
    assert!(import
        .1
        .contains("https://media1.giphy.com/media/chatonsxyz-1/giphy.gif"));
    let envoi = corps
        .iter()
        .rev()
        .find(|(r, _)| r == "POST /channels/c1/messages")
        .unwrap();
    assert!(envoi.1.contains(r#""reply_to":"300""#), "{}", envoi.1);
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesGifs>().get_ouvert()
                && f.global::<DonneesSaisie>().get_bandeau().genre == GenreBandeau::Aucun
                && derniere_ligne(f).is_some_and(|l| l.annexes.pieces_jointes.row_count() == 1)
        },
        "sélecteur fermé, réponse envoyée, GIF dans la liste",
    );
    println!("320 : GIF choisi au clavier, importé puis envoyé en réponse");

    *serveur.refus_de_gifs.lock().unwrap() = Some(429);
    cliquer(faible, "GIF");
    attendre_l_interface(
        faible,
        |f| f.global::<DonneesGifs>().get_message() == TROP_DE_RECHERCHES,
        "limite de débit annoncée",
    );
    let refusees = serveur.nombre_de("GET /gifs/");
    sur_l_interface(faible, |f| {
        f.global::<DonneesGifs>().set_defilement(-100_000.0);
        slint::platform::update_timers_and_animations();
    });
    for _ in 0..5 {
        avancer(faible, std::time::Duration::from_secs(1));
    }
    assert_eq!(
        serveur.nombre_de("GET /gifs/"),
        refusees,
        "aucune nouvelle tentative"
    );
    println!("320 : 429 annoncé clairement, aucune nouvelle tentative en 5 s simulées");

    sur_l_interface(faible, |f| {
        f.window().set_size(slint::LogicalSize::new(760.0, 900.0));
        slint::platform::update_timers_and_animations();
    });
    let (selecteur, vue) = sur_l_interface(faible, |f| {
        let bornes = |e: ElementHandle| (e.absolute_position().x, e.size().width);
        let selecteur = ElementHandle::find_by_accessible_label(f, "Choisir un GIF")
            .next()
            .expect("sélecteur de GIF");
        let vue = ElementHandle::find_by_accessible_label(f, "Messages")
            .find(|e| e.accessible_role() == Some(AccessibleRole::List))
            .expect("liste des messages");
        (bornes(selecteur), bornes(vue))
    });
    sur_l_interface(faible, |f| {
        f.window().set_size(slint::LogicalSize::new(1180.0, 900.0));
        slint::platform::update_timers_and_animations();
    });
    assert!(
        vue.1 < 416.0
            && selecteur.0 >= vue.0 + 8.0
            && selecteur.0 + selecteur.1 <= vue.0 + vue.1 - 8.0,
        "sélecteur {selecteur:?}, vue {vue:?}"
    );
    println!("320 C1 : focus sur la recherche à l'ouverture ; dans une vue de {} px le sélecteur reste dedans ({selecteur:?})", vue.1);
    touche(faible, Key::Escape);
    attendre_l_interface(
        faible,
        |f| {
            !f.global::<DonneesGifs>().get_ouvert()
                && !f.global::<DonneesSaisie>().get_gifs_ouvert()
        },
        "Échap ferme le sélecteur de GIF",
    );
    *serveur.refus_de_gifs.lock().unwrap() = None;
    let p = pont.clone();
    sur_l_interface(faible, move |f| {
        p.messagerie().saisie().placer(f, "", 0);
        p.messagerie().saisie_modifiee(f, "");
    });
    traiter_les_changements(faible);
    cliquer_le_champ(faible, "Écrire dans #général");
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

    actions_et_menus(&faible, &serveur, &pont);
    sondages(&faible, &serveur, &fabrique);
    selecteur_de_gifs(&faible, &serveur, &pont);
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
    if commun::video_hevc(dossier.path()).is_some() {
        *fabrique.conversions.lock().unwrap() = Some(Arc::new(Conversions::avec(Convertisseur {
            ffmpeg: "ffmpeg".into(),
            encodeurs: vec!["libx264".into()],
            dossier: dossier.path().join(DOSSIER_CONVERSIONS),
        })));
    }
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
