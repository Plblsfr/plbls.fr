mod commun;

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use commun::coquille::{etat, FausseFabrique, ServeurMessages};
use commun::{
    attendre, attendre_l_interface, sur_l_interface, FausseApi, FausseAuth, FauxNavigateur,
};
use estompe_client::derniere_vue::DerniereVue;
use estompe_client::preferences::Preferences;
use estompe_client::store::messages::MESSAGES_PAR_CONVERSATION;
use estompe_client::store::CleConversation;
use estompe_natif::coquille::{Coquille, Pont};
use estompe_natif::entree::{self, Entree, Machine};
use estompe_natif::session::Memoire;
use estompe_natif::{creer_fenetre, ecrans, DonneesMessagerie, Page, Principale};
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, Model};

const NOMBRE: u64 = 50_000;
const VARIABLE: &str = "ESTOMPE_VOLUME";
const INTERVALLE: Duration = Duration::from_secs(1);
const BORNE_INSTANCES: usize = 80;
const CROISSANCE_MAX: u64 = 48 * 1024 * 1024;

struct Parcours {
    pas: f32,
    duree: Duration,
    pages_min: usize,
}

fn parcours() -> Parcours {
    if std::env::var(VARIABLE).is_ok_and(|v| v == "long") {
        Parcours {
            pas: 1400.0,
            duree: Duration::from_secs(120),
            pages_min: 10,
        }
    } else {
        Parcours {
            pas: 1400.0,
            duree: Duration::from_secs(8),
            pages_min: 3,
        }
    }
}

fn instancier(f: &Principale) {
    let _ = ElementHandle::find_by_element_type_name(f, "LigneMessageVue").next();
}

fn pomper(faible: &slint::Weak<Principale>) {
    sur_l_interface(faible, |f| {
        ElementHandle::find_by_element_type_name(f, "LigneMessageVue").count();
        slint::platform::update_timers_and_animations();
    });
}

fn lignes_du_modele(faible: &slint::Weak<Principale>) -> usize {
    sur_l_interface(faible, |f| {
        f.global::<DonneesMessagerie>().get_lignes().row_count()
    })
}

fn stabiliser(faible: &slint::Weak<Principale>, pont: &Pont) {
    let mut precedent = None;
    let mut identiques = 0;
    attendre(
        || {
            let n = sur_l_interface(faible, |f| {
                instancier(f);
                slint::platform::update_timers_and_animations();
                f.global::<DonneesMessagerie>().get_lignes().row_count()
            });
            let m = pont.messagerie();
            let courant = (
                m.position().round() as i64,
                n,
                m.nombre_de_lignes(),
                m.mesures(),
            );
            if Some(courant) == precedent && !m.chargement_en_cours() {
                identiques += 1;
            } else {
                identiques = 0;
            }
            precedent = Some(courant);
            identiques >= 2
        },
        "liste stable",
    );
}

fn resident() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

fn ecran_de(faible: &slint::Weak<Principale>, n: u64) -> Option<f32> {
    let premiere = format!("message n°{n}");
    sur_l_interface(faible, move |f| {
        ElementHandle::find_by_element_type_name(f, "CorpsMessage")
            .find(|e| {
                e.accessible_description()
                    .is_some_and(|d| d == premiere || d.starts_with(&format!("{premiere}\n")))
            })
            .map(|e| e.absolute_position().y)
    })
}

fn visible_au_milieu(faible: &slint::Weak<Principale>) -> Option<u64> {
    sur_l_interface(faible, |f| {
        let liste = ElementHandle::find_by_accessible_label(f, "Messages").next()?;
        let milieu = liste.absolute_position().y + liste.size().height / 2.0;
        ElementHandle::find_by_element_type_name(f, "CorpsMessage")
            .filter(|e| {
                let y = e.absolute_position().y;
                y >= milieu && y < milieu + 200.0
            })
            .filter_map(|e| {
                let d = e.accessible_description()?;
                d.strip_prefix("message n°")?
                    .split('\n')
                    .next()?
                    .parse()
                    .ok()
            })
            .next()
    })
}

fn defiler_de(faible: &slint::Weak<Principale>, pas: f32) {
    sur_l_interface(faible, move |f| {
        let g = f.global::<DonneesMessagerie>();
        g.set_defilement((g.get_defilement() + pas).min(0.0));
    });
}

fn scenario(
    faible: slint::Weak<Principale>,
    fabrique: Arc<FausseFabrique>,
    coquille: Arc<Coquille>,
    serveur: Arc<ServeurMessages>,
) {
    attendre_l_interface(
        &faible,
        |f| f.get_page() == Page::Coquille && f.global::<DonneesMessagerie>().get_active(),
        "vue des messages",
    );
    attendre(|| coquille.pont().is_some(), "pont");
    let pont = coquille.pont().unwrap();
    attendre(
        || pont.messagerie().nombre_de_lignes() > 40,
        "première page",
    );
    stabiliser(&faible, &pont);
    assert!(pont.messagerie().colle(), "ouverture en bas sans non-lu");

    let store = fabrique.store();
    let c1 = CleConversation::Salon("c1".into());
    let messages = || store.conversation(&c1).map_or(0, |c| c.messages.len());
    let Parcours {
        pas,
        duree: simulee,
        pages_min,
    } = parcours();
    let etapes = (simulee.as_millis() / INTERVALLE.as_millis()) as usize;
    let mut base = None;
    let mut max_instances = 0;
    let mut max_modele = 0;
    let mut max_messages = 0;
    let mut max_lignes = 0;
    let mut duree = Duration::ZERO;
    let mut pages = 0;
    for etape in 0..etapes {
        let debut = Instant::now();
        defiler_de(&faible, pas);
        i_slint_backend_testing::mock_elapsed_time(INTERVALLE);
        stabiliser(&faible, &pont);
        duree += debut.elapsed();
        let (n, modele) = if etape % 4 == 0 {
            let (n, modele) = sur_l_interface(&faible, |f| {
                (
                    ElementHandle::find_by_element_type_name(f, "LigneMessageVue").count(),
                    f.global::<DonneesMessagerie>().get_lignes().row_count(),
                )
            });
            (n, modele)
        } else {
            let modele = lignes_du_modele(&faible);
            (modele, modele)
        };
        max_instances = max_instances.max(n);
        max_modele = max_modele.max(modele);
        max_messages = max_messages.max(messages());
        max_lignes = max_lignes.max(pont.messagerie().nombre_de_lignes());
        if etape == etapes / 4 {
            base = resident();
            pages = serveur.nombre_de("GET /channels/c1/messages?limit=50&before=");
        }
    }
    let fin = resident();
    let pages_totales = serveur.nombre_de("GET /channels/c1/messages?limit=50&before=");
    let premier = store
        .conversation(&c1)
        .and_then(|c| c.premier_id().map(str::to_string))
        .unwrap_or_default();
    println!(
        "299 C2 : {etapes} pas de {pas} px ({} s simulées), {pages_totales} pages anciennes chargées ({} après le premier quart), premier message chargé n°{premier}",
        simulee.as_secs(),
        pages_totales - pages
    );
    println!(
        "299 C2 : lignes instanciées max {max_instances}, modèle Slint max {max_modele}, messages du store max {max_messages}, lignes projetées max {max_lignes}, rendus en cache {}",
        pont.messagerie().rendus_en_cache()
    );
    println!(
        "299 C2 : temps moyen par pas {:.1} ms",
        duree.as_secs_f64() * 1000.0 / etapes as f64
    );
    match (base, fin) {
        (Some(base), Some(fin)) => {
            println!(
                "299 C2 : mémoire résidente {:.1} Mo au premier quart, {:.1} Mo à la fin (écart {:+.1} Mo)",
                base as f64 / 1048576.0,
                fin as f64 / 1048576.0,
                (fin as f64 - base as f64) / 1048576.0
            );
            assert!(fin <= base + CROISSANCE_MAX, "mémoire {base} -> {fin}");
        }
        _ => println!("299 C2 : /proc/self/statm indisponible, seuls les compteurs sont vérifiés"),
    }
    assert!(
        pages_totales - pages >= pages_min,
        "l'historique a bien été remonté"
    );
    assert!(
        max_instances <= BORNE_INSTANCES,
        "{max_instances} lignes instanciées"
    );
    assert!(max_modele <= BORNE_INSTANCES);
    assert!(max_messages <= MESSAGES_PAR_CONVERSATION);
    assert!(max_lignes <= 2 * MESSAGES_PAR_CONVERSATION);

    attendre(
        || {
            stabiliser(&faible, &pont);
            visible_au_milieu(&faible).is_some()
        },
        "un message au milieu de l'écran",
    );
    serveur.bloquer(true);
    let demandes = serveur.nombre_de("GET /channels/c1/messages?limit=50&before=");
    let mut tentatives = 0;
    while serveur.nombre_de("GET /channels/c1/messages?limit=50&before=") == demandes {
        assert!(tentatives < 200, "aucune page ancienne demandée");
        tentatives += 1;
        defiler_de(&faible, 300.0);
        pomper(&faible);
    }
    let mut repere = 0;
    attendre(
        || {
            pomper(&faible);
            visible_au_milieu(&faible).map(|n| repere = n).is_some()
        },
        "repère choisi pendant le chargement",
    );
    let mut avant = None;
    attendre(
        || {
            pomper(&faible);
            let y = ecran_de(&faible, repere);
            let stable = y.is_some() && y == avant;
            avant = y;
            stable
        },
        "repère immobile pendant le chargement",
    );
    let avant = avant.unwrap();
    let lignes_avant = pont.messagerie().nombre_de_lignes();
    let position_avant = pont.messagerie().position();
    serveur.bloquer(false);
    attendre(
        || {
            pomper(&faible);
            !pont.messagerie().chargement_en_cours()
                && pont.messagerie().position() > position_avant + 500.0
        },
        "page ancienne ajoutée en tête",
    );
    stabiliser(&faible, &pont);
    let apres = ecran_de(&faible, repere).expect("repère toujours instancié");
    println!(
        "299 C3 : repère n°{repere} à y = {avant} avant et {apres} après l'ajout en tête ({} -> {} lignes, défilement {position_avant:.0} -> {:.0})",
        lignes_avant,
        pont.messagerie().nombre_de_lignes(),
        pont.messagerie().position()
    );
    assert!((avant - apres).abs() < 0.5, "{avant} -> {apres}");
}

#[test]
fn c2_c3_un_salon_de_cinquante_mille_messages_reste_borne_et_garde_sa_position() {
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
    let fabrique = FausseFabrique::sur(etat(), Vec::new(), &serveur.adresse);
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
        let resultat = catch_unwind(AssertUnwindSafe(|| scenario(faible, fabrique, c, serveur)));
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
