use axum::http::StatusCode;
use chrono::NaiveDate;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

use crate::{
    bilan, classement,
    testutil::{Env, PASSAGE_ANNEE, TROIS_PAGES, env, monter, pages, pdf, postgres},
};

fn mars() -> Vec<u8> {
    pdf(&pages(TROIS_PAGES), "a")
}

fn janvier() -> Vec<u8> {
    pdf(&pages(PASSAGE_ANNEE), "a")
}

fn liste(v: &Value, cle: &str) -> Vec<Value> {
    v[cle].as_array().cloned().unwrap_or_default()
}

impl Env {
    async fn categorie(&self, qui: &str, cle: &str) -> i64 {
        liste(&self.get(qui, "/categories").await.json(), "categories")
            .iter()
            .find(|c| c["key"] == cle)
            .and_then(|c| c["id"].as_i64())
            .expect("catégorie par défaut")
    }

    async fn operations(&self, qui: &str, filtre: &str) -> Vec<Value> {
        liste(
            &self
                .get(qui, &format!("/operations?limit=200{filtre}"))
                .await
                .json(),
            "operations",
        )
    }

    async fn operation(&self, qui: &str, libelle: &str) -> Value {
        self.operations(qui, "")
            .await
            .into_iter()
            .find(|o| o["label"].as_str().is_some_and(|l| l.contains(libelle)))
            .expect("opération de la fixture")
    }

    async fn classement(&self, qui: &str, libelle: &str) -> (Option<i64>, Option<String>) {
        let o = self.operation(qui, libelle).await;
        (
            o["category_id"].as_i64(),
            o["category_source"].as_str().map(str::to_string),
        )
    }

    async fn a_classer(&self, qui: &str) -> i64 {
        self.get(qui, "/unclassified/count").await.json()["operations"]
            .as_i64()
            .unwrap()
    }
}

fn de_regle(categorie: i64) -> (Option<i64>, Option<String>) {
    (Some(categorie), Some("regle".into()))
}

fn du_commercant(categorie: i64) -> (Option<i64>, Option<String>) {
    (Some(categorie), Some("commercant".into()))
}

#[tokio::test]
async fn categories_et_regles_par_defaut_creees_une_seule_fois() {
    let Some(e) = env().await else { return };
    let premieres = liste(&e.get("alice", "/categories").await.json(), "categories");
    let vues: Vec<(&str, &str)> = premieres
        .iter()
        .map(|c| (c["key"].as_str().unwrap(), c["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(vues.len(), 18);
    for (cle, nature) in [
        ("logement", "fixe"),
        ("credit", "fixe"),
        ("assurances", "fixe"),
        ("telecom", "fixe"),
        ("energie", "fixe"),
        ("courses", "variable"),
        ("epargne", "epargne"),
        ("revenus", "revenu"),
        ("transferts", "neutre"),
    ] {
        assert!(vues.contains(&(cle, nature)), "{cle}");
    }
    let couleurs: std::collections::BTreeSet<i64> = premieres
        .iter()
        .map(|c| c["color"].as_i64().unwrap())
        .collect();
    assert_eq!(couleurs.len(), 18);
    let regles = liste(&e.get("alice", "/rules").await.json(), "rules");
    assert_eq!(regles.len(), 15);
    assert_eq!(regles[0]["type"], "echeance_pret");
    assert_eq!(regles[0]["position"], 0);
    assert_eq!(regles[2]["section"], "SERVICES ET FRAIS BANCAIRES");

    let logement = e.categorie("alice", "logement").await;
    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/categories/{logement}"),
            &json!({"name": "  Loyer   et charges ", "monthly_cap": 78_000, "archived": true}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    assert_eq!(r.json()["category"]["name"], "Loyer et charges");
    let regle = regles[5]["id"].as_i64().unwrap();
    assert_eq!(
        e.appel(Some("alice"), "DELETE", &format!("/rules/{regle}"))
            .await
            .statut,
        StatusCode::OK
    );
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let secondes = liste(&e.get("alice", "/categories").await.json(), "categories");
    assert_eq!(secondes.len(), 18);
    let modifiee = secondes.last().unwrap();
    assert_eq!(
        (
            &modifiee["id"],
            &modifiee["name"],
            &modifiee["monthly_cap"],
            &modifiee["archived"]
        ),
        (
            &json!(logement),
            &json!("Loyer et charges"),
            &json!(78_000),
            &json!(true)
        )
    );
    assert_eq!(
        liste(&e.get("alice", "/rules").await.json(), "rules").len(),
        14
    );
    let personnes: i64 = sqlx::query_scalar("SELECT count(*) FROM personnes")
        .fetch_one(e.state.pool())
        .await
        .unwrap();
    assert_eq!(personnes, 1);
}

#[tokio::test]
async fn categories_creees_modifiees_et_validees() {
    let Some(e) = env().await else { return };
    let r = e
        .envoyer(
            "alice",
            "POST",
            "/categories",
            &json!({"name": "Animaux", "kind": "variable", "monthly_cap": 4000}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
    let c = r.json()["category"].clone();
    assert_eq!(
        (
            &c["key"],
            &c["name"],
            &c["kind"],
            &c["monthly_cap"],
            &c["color"]
        ),
        (
            &Value::Null,
            &json!("Animaux"),
            &json!("variable"),
            &json!(4000),
            &json!(18)
        )
    );
    let id = c["id"].as_i64().unwrap();
    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/categories/{id}"),
            &json!({"kind": "fixe", "monthly_cap": null}),
        )
        .await
        .json();
    assert_eq!(
        (&r["category"]["kind"], &r["category"]["monthly_cap"]),
        (&json!("fixe"), &Value::Null)
    );
    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/categories/{id}"),
            &json!({"name": "Animaux de compagnie"}),
        )
        .await
        .json();
    assert_eq!(r["category"]["kind"], "fixe");
    for (corps, statut) in [
        (
            json!({"name": "animaux DE compagnie", "kind": "variable"}),
            StatusCode::CONFLICT,
        ),
        (
            json!({"name": " ", "kind": "variable"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name": "x".repeat(41), "kind": "variable"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name": "Jeux", "kind": "plaisir"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name": "Jeux", "kind": "variable", "monthly_cap": -1}),
            StatusCode::BAD_REQUEST,
        ),
        (json!({"name": "Jeux"}), StatusCode::BAD_REQUEST),
        (
            json!({"name": "Jeux", "kind": "variable", "sub": "sub-bruno"}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let r = e.envoyer("alice", "POST", "/categories", &corps).await;
        assert_eq!(r.statut, statut, "{corps}");
    }
    assert_eq!(
        e.envoyer(
            "alice",
            "PATCH",
            "/categories/999999",
            &json!({"name": "Rien"})
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.envoyer(
            "alice",
            "PATCH",
            &format!("/categories/{id}"),
            &json!({"name": "Crédit"})
        )
        .await
        .statut,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn import_classe_par_les_regles_par_defaut_et_file_a_classer() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let courses = e.categorie("alice", "courses").await;
    let frais = e.categorie("alice", "frais").await;
    for (libelle, cle) in [
        ("TELECOM IMAGINAIRE", "telecom"),
        ("ECH PRET", "credit"),
        ("BOULANGERIE DUPAIN", "courses"),
        ("EPICERIE DU COIN", "courses"),
        ("F COTISATION", "frais"),
        ("MUTUELLE DU PHARE", "assurances"),
        ("LIBRAIRIE DES PAGES", "loisirs"),
    ] {
        let attendue = e.categorie("alice", cle).await;
        assert_eq!(
            e.classement("alice", libelle).await,
            de_regle(attendue),
            "{libelle}"
        );
    }
    for libelle in ["DURAND LOUISE", "ATELIER DES LUTINS", "GARAGE DUPNEU"] {
        assert_eq!(
            e.classement("alice", libelle).await,
            (None, None),
            "{libelle}"
        );
    }
    let boulangerie = e.operation("alice", "BOULANGERIE DUPAIN").await;
    assert_eq!(boulangerie["merchant_name"], "Boulangerie Dupain Rennes");
    let regle = boulangerie["rule_id"].as_i64().unwrap();
    let regles = liste(&e.get("alice", "/rules").await.json(), "rules");
    let utilisee = regles.iter().find(|r| r["id"] == regle).unwrap();
    assert_eq!(utilisee["key"], "courses");
    assert_eq!(utilisee["operations"], 2);

    assert_eq!(
        e.get("alice", "/unclassified/count").await.json(),
        json!({"operations": 8, "merchants": 7, "debits": 13_509, "credits": 192_839})
    );
    let file = e.get("alice", "/unclassified").await.json();
    assert_eq!(
        (&file["operations"], &file["merchants"]),
        (&json!(8), &json!(7))
    );
    let groupes = liste(&file, "groups");
    assert_eq!(groupes.len(), 7);
    let jeux = &groupes[0];
    assert_eq!(
        (
            &jeux["merchant_name"],
            &jeux["operations"],
            &jeux["debits"],
            &jeux["credits"],
            &jeux["first_date"],
            &jeux["last_date"]
        ),
        (
            &json!("Jeux En Ligne Fictifs Fictiville"),
            &json!(2),
            &json!(1599),
            &json!(1599),
            &json!("2025-02-20"),
            &json!("2025-02-21")
        )
    );
    assert_eq!(liste(jeux, "sample").len(), 2);
    assert_eq!(jeux["sample"][0]["date"], "2025-02-21");

    assert_eq!(e.operations("alice", "&unclassified=true").await.len(), 8);
    assert_eq!(e.operations("alice", "&unclassified=false").await.len(), 15);
    assert_eq!(
        e.operations("alice", &format!("&category={courses}"))
            .await
            .len(),
        2
    );
    assert_eq!(
        e.operations("alice", &format!("&category={frais}&unclassified=1"))
            .await
            .len(),
        0
    );
    let commercant = jeux["merchant_id"].as_i64().unwrap();
    let du_commercant = e
        .operations("alice", &format!("&merchant={commercant}"))
        .await;
    assert_eq!(du_commercant.len(), 2);
    for faux in ["?category=x", "?merchant=-1", "?unclassified=oui"] {
        assert_eq!(
            e.get("alice", &format!("/operations{faux}")).await.statut,
            StatusCode::BAD_REQUEST,
            "{faux}"
        );
    }
    let id = boulangerie["id"].as_i64().unwrap();
    assert_eq!(
        e.get("alice", &format!("/operations/{id}")).await.json()["operation"],
        boulangerie
    );
    for inconnu in ["/operations/999999", "/operations/abc", "/operations/0"] {
        assert_eq!(
            e.get("alice", inconnu).await.statut,
            StatusCode::NOT_FOUND,
            "{inconnu}"
        );
    }
}

#[tokio::test]
async fn regles_retroactives_ordonnees_et_appliquees_aux_imports_suivants() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let loisirs = e.categorie("alice", "loisirs").await;
    let abonnements = e.categorie("alice", "abonnements").await;
    let shopping = e.categorie("alice", "shopping").await;
    let courses = e.categorie("alice", "courses").await;
    let r = e
        .envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": ["ciné étoile", "CINE  ETOILE"], "category_id": abonnements}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
    let cree = r.json();
    assert_eq!(cree["reclassified"], 1);
    assert_eq!(cree["rule"]["keywords"], json!(["CINE ETOILE"]));
    assert_eq!(
        (&cree["rule"]["position"], &cree["rule"]["operations"]),
        (&json!(0), &json!(1))
    );
    let cine = cree["rule"]["id"].as_i64().unwrap();
    assert_eq!(
        e.classement("alice", "CINE ETOILE").await,
        de_regle(abonnements)
    );
    assert_eq!(e.a_classer("alice").await, 7);

    e.importer("alice", &[("b.pdf", &janvier())]).await;
    let cines = e
        .operations("alice", &format!("&category={abonnements}"))
        .await;
    assert_eq!(cines.len(), 2);
    assert!(
        cines
            .iter()
            .all(|o| o["rule_id"] == cine && o["category_source"] == "regle")
    );

    let r = e
        .envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": ["RENNES"], "category_id": shopping}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 4);
    let rennes = r["rule"]["id"].as_i64().unwrap();
    assert_eq!(
        e.classement("alice", "CINE ETOILE").await,
        de_regle(shopping)
    );
    assert_eq!(
        e.classement("alice", "BOULANGERIE DUPAIN").await,
        de_regle(shopping)
    );

    let mut ordre: Vec<i64> = liste(&e.get("alice", "/rules").await.json(), "rules")
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    assert_eq!(&ordre[..2], [rennes, cine]);
    ordre.remove(0);
    ordre.push(rennes);
    let r = e
        .envoyer("alice", "PUT", "/rules/order", &json!({"ids": ordre}))
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    assert_eq!(r.json()["reclassified"], 4);
    assert_eq!(r.json()["rules"][0]["id"], cine);
    assert_eq!(r.json()["rules"][16]["position"], 16);
    assert_eq!(
        e.classement("alice", "CINE ETOILE").await,
        de_regle(abonnements)
    );
    assert_eq!(
        e.classement("alice", "BOULANGERIE DUPAIN").await,
        de_regle(courses)
    );
    for faux in [
        json!({"ids": []}),
        json!({"ids": [cine]}),
        json!({"ids": ordre.iter().chain([&cine]).collect::<Vec<_>>()}),
    ] {
        assert_eq!(
            e.envoyer("alice", "PUT", "/rules/order", &faux)
                .await
                .statut,
            StatusCode::BAD_REQUEST,
            "{faux}"
        );
    }

    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/rules/{cine}"),
            &json!({"keywords": ["LIBRAIRIE", "CINE ETOILE"], "category_id": shopping}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 3);
    assert_eq!(
        e.classement("alice", "LIBRAIRIE DES PAGES").await,
        de_regle(shopping)
    );
    let r = e
        .appel(Some("alice"), "DELETE", &format!("/rules/{cine}"))
        .await;
    assert_eq!(r.json(), json!({"reclassified": 3}));
    assert_eq!(
        e.classement("alice", "LIBRAIRIE DES PAGES").await,
        de_regle(loisirs)
    );
    assert_eq!(
        e.appel(Some("alice"), "DELETE", &format!("/rules/{cine}"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );

    let r = e
        .envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"type": "virement_instantane", "section": "Virements reçus", "category_id": abonnements}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 1);
    assert_eq!(r["rule"]["section"], "VIREMENTS RECUS");
    assert_eq!(
        e.classement("alice", "DURAND LOUISE").await,
        de_regle(abonnements)
    );
    assert_eq!(e.classement("alice", "BERNARD PAUL").await, (None, None));
    let structurelle = r["rule"]["id"].as_i64().unwrap();
    assert_eq!(
        e.envoyer(
            "alice",
            "PATCH",
            &format!("/rules/{structurelle}"),
            &json!({"type": null, "section": null})
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
    for faux in [
        json!({"keywords": [], "category_id": abonnements}),
        json!({"keywords": ["x"], "category_id": abonnements}),
        json!({"keywords": ["--"], "category_id": abonnements}),
        json!({"keywords": ["CINE"], "category_id": 999_999}),
        json!({"keywords": ["CINE"], "type": "troc", "category_id": abonnements}),
        json!({"keywords": ["CINE"]}),
        json!({"keywords": (0..13).map(|i| format!("MOT{i}X")).collect::<Vec<_>>(), "category_id": abonnements}),
    ] {
        assert_eq!(
            e.envoyer("alice", "POST", "/rules", &faux).await.statut,
            StatusCode::BAD_REQUEST,
            "{faux}"
        );
    }
    let archivee = e.categorie("alice", "divers").await;
    e.envoyer(
        "alice",
        "PATCH",
        &format!("/categories/{archivee}"),
        &json!({"archived": true}),
    )
    .await;
    assert_eq!(
        e.envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": ["CINE"], "category_id": archivee})
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn reclassement_du_commercant_exception_sur_une_operation_et_rejeu_idempotent() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let loisirs = e.categorie("alice", "loisirs").await;
    let shopping = e.categorie("alice", "shopping").await;
    let divers = e.categorie("alice", "divers").await;
    let jeux = e
        .operations("alice", "")
        .await
        .into_iter()
        .filter(|o| o["label"].as_str().unwrap().contains("Jeux En Ligne"))
        .collect::<Vec<_>>();
    assert_eq!(jeux.len(), 2);
    let (achat, remboursement) = (
        jeux[0]["id"].as_i64().unwrap(),
        jeux[1]["id"].as_i64().unwrap(),
    );
    let commercant = jeux[0]["merchant_id"].as_i64().unwrap();

    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{achat}/category"),
            &json!({"category_id": loisirs, "scope": "merchant"}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    assert_eq!(r.json()["reclassified"], 2);
    assert_eq!(r.json()["operation"]["category_source"], "commercant");
    let classees = e
        .operations("alice", &format!("&merchant={commercant}"))
        .await;
    assert!(
        classees
            .iter()
            .all(|o| o["category_id"] == loisirs && o["category_source"] == "commercant")
    );
    assert_eq!(e.a_classer("alice").await, 6);
    assert_eq!(
        e.get("alice", &format!("/merchants/{commercant}"))
            .await
            .json()["merchant"]["category_id"],
        loisirs
    );

    let r = e
        .envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": ["JEUX"], "category_id": shopping}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 0);
    assert_eq!(
        e.classement("alice", "CARTE 19/02 Jeux").await,
        du_commercant(loisirs)
    );

    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{remboursement}/category"),
            &json!({"category_id": divers, "scope": "operation"}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 1);
    assert_eq!(
        (
            &r["operation"]["category_id"],
            &r["operation"]["category_source"]
        ),
        (&json!(divers), &json!("operation"))
    );
    assert_eq!(
        e.classement("alice", "CARTE 19/02 Jeux").await,
        du_commercant(loisirs)
    );

    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/merchants/{commercant}/category"),
            &json!({"category_id": null}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 1);
    assert_eq!(r["merchant"]["category_id"], Value::Null);
    assert_eq!(
        e.classement("alice", "CARTE 19/02 Jeux").await,
        de_regle(shopping)
    );
    assert_eq!(
        e.classement("alice", "CARTE 20/02 Jeux").await,
        (Some(divers), Some("operation".into()))
    );

    let mut c = e.state.pool().acquire().await.unwrap();
    assert_eq!(classement::rejouer(&mut c, "sub-alice").await.unwrap(), 0);
    assert_eq!(classement::rejouer(&mut c, "sub-alice").await.unwrap(), 0);
    let avant = e.operations("alice", "").await;
    sqlx::query("UPDATE personnes SET normalisation = 0")
        .execute(&mut *c)
        .await
        .unwrap();
    drop(c);
    assert_eq!(e.operations("alice", "").await, avant);

    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{remboursement}/category"),
            &json!({"category_id": null, "scope": "operation"}),
        )
        .await
        .json();
    assert_eq!(r["operation"]["category_source"], "regle");
    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{remboursement}/category"),
            &json!({"category_id": divers, "scope": "operation"}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{remboursement}/category"),
            &json!({"category_id": loisirs, "scope": "merchant"}),
        )
        .await
        .json();
    assert_eq!(r["operation"]["category_source"], "commercant");
    assert_eq!(r["reclassified"], 2);

    for (corps, statut) in [
        (
            json!({"category_id": loisirs, "scope": "tout"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"category_id": 999_999, "scope": "operation"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"category_id": 999_999, "scope": "merchant"}),
            StatusCode::BAD_REQUEST,
        ),
        (json!({"scope": 3}), StatusCode::BAD_REQUEST),
    ] {
        assert_eq!(
            e.envoyer(
                "alice",
                "PUT",
                &format!("/operations/{achat}/category"),
                &corps
            )
            .await
            .statut,
            statut,
            "{corps}"
        );
    }
    assert_eq!(
        e.envoyer(
            "alice",
            "PUT",
            "/operations/999999/category",
            &json!({"category_id": loisirs, "scope": "operation"})
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn operation_sans_commercant_classee_seule() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let divers = e.categorie("alice", "divers").await;
    let id = e.operation("alice", "GARAGE DUPNEU").await["id"]
        .as_i64()
        .unwrap();
    sqlx::query("UPDATE operations SET commercant_cle = NULL, commercant_id = NULL WHERE id = $1")
        .bind(id)
        .execute(e.state.pool())
        .await
        .unwrap();
    let file = e.get("alice", "/unclassified").await.json();
    let seule = liste(&file, "groups")
        .into_iter()
        .find(|g| g["merchant_id"].is_null())
        .unwrap();
    assert_eq!(
        (&seule["operations"], &seule["sample"][0]["id"]),
        (&json!(1), &json!(id))
    );
    assert_eq!(file["merchants"], 7);
    assert_eq!(
        e.envoyer(
            "alice",
            "PUT",
            &format!("/operations/{id}/category"),
            &json!({"category_id": divers, "scope": "merchant"})
        )
        .await
        .statut,
        StatusCode::BAD_REQUEST
    );
    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{id}/category"),
            &json!({"category_id": divers, "scope": "operation"}),
        )
        .await
        .json();
    assert_eq!(r["operation"]["category_source"], "operation");
    assert_eq!(e.a_classer("alice").await, 7);
}

#[tokio::test]
async fn fusion_et_separation_de_commercants_memorisees() {
    let Some(e) = env().await else { return };
    let r = e
        .importer("alice", &[("a.pdf", &mars()), ("b.pdf", &janvier())])
        .await
        .json();
    let releves: Vec<i64> = liste(&r, "results")
        .iter()
        .map(|x| x["statement"]["id"].as_i64().unwrap())
        .collect();
    let loisirs = e.categorie("alice", "loisirs").await;
    let commercants = liste(&e.get("alice", "/merchants").await.json(), "merchants");
    assert_eq!(commercants.len(), 15);
    assert!(
        commercants
            .iter()
            .all(|c| c["variants"] == 1 && c["category_id"].is_null())
    );
    let de = |nom: &str| {
        commercants
            .iter()
            .find(|c| c["name"] == nom)
            .and_then(|c| c["id"].as_i64())
            .unwrap()
    };
    let (cine, librairie, voyages) = (
        de("Cine Etoile Rennes"),
        de("Librairie des Pages Quimper"),
        de("Voyages du Large Quimper"),
    );
    assert_eq!(
        liste(
            &e.get("alice", "/merchants?q=quimper").await.json(),
            "merchants"
        )
        .len(),
        2
    );
    assert_eq!(
        liste(
            &e.get("alice", "/merchants?q=100%25").await.json(),
            "merchants"
        )
        .len(),
        0
    );
    e.envoyer(
        "alice",
        "PUT",
        &format!("/merchants/{librairie}/category"),
        &json!({"category_id": loisirs}),
    )
    .await;

    let r = e
        .envoyer(
            "alice",
            "POST",
            &format!("/merchants/{cine}/merge"),
            &json!({"merchant_ids": [voyages, librairie]}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let fusion = r.json();
    assert_eq!(fusion["reclassified"], 3);
    let m = &fusion["merchant"];
    assert_eq!(
        (
            &m["id"],
            &m["name"],
            &m["category_id"],
            &m["operations"],
            &m["variants"],
            &m["debits"]
        ),
        (
            &json!(cine),
            &json!("Cine Etoile Rennes"),
            &json!(loisirs),
            &json!(4),
            &json!(3),
            &json!(128_546)
        )
    );
    assert_eq!(
        m["variant_list"],
        json!([
            {"key": "CINE ETOILE RENNES", "manual": false, "operations": 2},
            {"key": "LIBRAIRIE DES PAGES QUIMPER", "manual": true, "operations": 1},
            {"key": "VOYAGES DU LARGE QUIMPER", "manual": true, "operations": 1},
        ])
    );
    assert_eq!(
        e.get("alice", &format!("/merchants/{voyages}"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        liste(&e.get("alice", "/merchants").await.json(), "merchants").len(),
        13
    );
    let du_groupe = e.operations("alice", &format!("&merchant={cine}")).await;
    assert_eq!(du_groupe.len(), 4);
    assert!(
        du_groupe
            .iter()
            .all(|o| o["category_id"] == loisirs && o["merchant_name"] == "Cine Etoile Rennes")
    );

    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/merchants/{cine}"),
            &json!({"name": " Sorties  du samedi "}),
        )
        .await
        .json();
    assert_eq!(r["merchant"]["name"], "Sorties du samedi");

    for id in &releves {
        e.appel(Some("alice"), "DELETE", &format!("/statements/{id}"))
            .await;
    }
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 0);
    let restants = liste(&e.get("alice", "/merchants").await.json(), "merchants");
    assert_eq!(restants.len(), 1);
    assert_eq!(
        (&restants[0]["id"], &restants[0]["operations"]),
        (&json!(cine), &json!(0))
    );
    e.importer("alice", &[("a.pdf", &mars()), ("b.pdf", &janvier())])
        .await;
    let du_groupe = e.operations("alice", &format!("&merchant={cine}")).await;
    assert_eq!(du_groupe.len(), 4);
    assert!(
        du_groupe
            .iter()
            .all(|o| o["category_id"] == loisirs && o["category_source"] == "commercant")
    );

    let r = e
        .envoyer(
            "alice",
            "POST",
            &format!("/merchants/{cine}/split"),
            &json!({"keys": ["VOYAGES DU LARGE QUIMPER"]}),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let separation = r.json();
    assert_eq!(
        (
            &separation["merchant"]["variants"],
            &separation["merchant"]["operations"]
        ),
        (&json!(2), &json!(3))
    );
    let cree = &separation["created"][0];
    assert_eq!(
        (
            &cree["name"],
            &cree["category_id"],
            &cree["operations"],
            &cree["variant_list"][0]["manual"]
        ),
        (
            &json!("Voyages du Large Quimper"),
            &json!(loisirs),
            &json!(1),
            &json!(true)
        )
    );
    let nouveau = cree["id"].as_i64().unwrap();
    assert_eq!(
        e.operation("alice", "VOYAGES DU LARGE").await["merchant_id"],
        nouveau
    );
    for (chemin, corps) in [
        (format!("/merchants/{cine}/split"), json!({"keys": []})),
        (
            format!("/merchants/{cine}/split"),
            json!({"keys": ["INCONNUE"]}),
        ),
        (
            format!("/merchants/{cine}/split"),
            json!({"keys": ["CINE ETOILE RENNES", "LIBRAIRIE DES PAGES QUIMPER"]}),
        ),
        (
            format!("/merchants/{nouveau}/split"),
            json!({"keys": ["VOYAGES DU LARGE QUIMPER"]}),
        ),
        (
            format!("/merchants/{cine}/merge"),
            json!({"merchant_ids": []}),
        ),
        (
            format!("/merchants/{cine}/merge"),
            json!({"merchant_ids": [cine]}),
        ),
        (
            format!("/merchants/{cine}/merge"),
            json!({"merchant_ids": [999_999]}),
        ),
        (format!("/merchants/{cine}"), json!({"name": ""})),
    ] {
        let methode = if chemin.ends_with(&cine.to_string()) {
            "PATCH"
        } else {
            "POST"
        };
        assert_eq!(
            e.envoyer("alice", methode, &chemin, &corps).await.statut,
            StatusCode::BAD_REQUEST,
            "{chemin} {corps}"
        );
    }
    assert_eq!(
        e.envoyer(
            "alice",
            "POST",
            "/merchants/999999/merge",
            &json!({"merchant_ids": [cine]})
        )
        .await
        .statut,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn categorisation_cloisonnee_par_personne() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    e.importer("bruno", &[("a.pdf", &mars())]).await;
    let loisirs_alice = e.categorie("alice", "loisirs").await;
    let loisirs_bruno = e.categorie("bruno", "loisirs").await;
    assert_ne!(loisirs_alice, loisirs_bruno);
    let op_alice = e.operation("alice", "CINE ETOILE").await;
    let (op, commercant) = (
        op_alice["id"].as_i64().unwrap(),
        op_alice["merchant_id"].as_i64().unwrap(),
    );
    let regle_alice = e
        .envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": ["GARAGE"], "category_id": loisirs_alice}),
        )
        .await
        .json()["rule"]["id"]
        .as_i64()
        .unwrap();
    let etat_alice = e.operations("alice", "").await;

    for (methode, chemin, corps) in [
        ("GET", format!("/operations/{op}"), Value::Null),
        (
            "PUT",
            format!("/operations/{op}/category"),
            json!({"category_id": loisirs_bruno, "scope": "operation"}),
        ),
        (
            "PUT",
            format!("/operations/{op}/category"),
            json!({"category_id": loisirs_bruno, "scope": "merchant"}),
        ),
        ("GET", format!("/merchants/{commercant}"), Value::Null),
        (
            "PATCH",
            format!("/merchants/{commercant}"),
            json!({"name": "À moi"}),
        ),
        (
            "PUT",
            format!("/merchants/{commercant}/category"),
            json!({"category_id": loisirs_bruno}),
        ),
        (
            "POST",
            format!("/merchants/{commercant}/split"),
            json!({"keys": ["CINE ETOILE RENNES"]}),
        ),
        (
            "PATCH",
            format!("/categories/{loisirs_alice}"),
            json!({"name": "À moi"}),
        ),
        (
            "PATCH",
            format!("/rules/{regle_alice}"),
            json!({"category_id": loisirs_bruno}),
        ),
        ("DELETE", format!("/rules/{regle_alice}"), Value::Null),
    ] {
        let r = if corps.is_null() {
            e.appel(Some("bruno"), methode, &chemin).await
        } else {
            e.envoyer("bruno", methode, &chemin, &corps).await
        };
        assert_eq!(r.statut, StatusCode::NOT_FOUND, "{methode} {chemin}");
    }
    let de_bruno = e.operation("bruno", "CINE ETOILE").await;
    let commercant_bruno = de_bruno["merchant_id"].as_i64().unwrap();
    assert_ne!(commercant_bruno, commercant);
    for (methode, chemin, corps) in [
        (
            "POST",
            "/rules".to_string(),
            json!({"keywords": ["CINE"], "category_id": loisirs_alice}),
        ),
        (
            "PUT",
            format!("/merchants/{commercant_bruno}/category"),
            json!({"category_id": loisirs_alice}),
        ),
        (
            "PUT",
            format!("/operations/{}/category", de_bruno["id"]),
            json!({"category_id": loisirs_alice, "scope": "operation"}),
        ),
        (
            "POST",
            format!("/merchants/{commercant_bruno}/merge"),
            json!({"merchant_ids": [commercant]}),
        ),
        (
            "PUT",
            "/rules/order".to_string(),
            json!({"ids": [regle_alice]}),
        ),
    ] {
        assert_eq!(
            e.envoyer("bruno", methode, &chemin, &corps).await.statut,
            StatusCode::BAD_REQUEST,
            "{methode} {chemin}"
        );
    }
    assert_eq!(
        e.get("bruno", &format!("/operations?category={loisirs_alice}"))
            .await
            .json()["total"],
        0
    );
    assert_eq!(
        e.get("bruno", &format!("/operations?merchant={commercant}"))
            .await
            .json()["total"],
        0
    );
    let r = e
        .envoyer(
            "bruno",
            "POST",
            "/rules",
            &json!({"keywords": ["CINE ETOILE"], "category_id": loisirs_bruno}),
        )
        .await
        .json();
    assert_eq!(r["reclassified"], 1);
    e.envoyer(
        "bruno",
        "PATCH",
        &format!("/categories/{loisirs_bruno}"),
        &json!({"name": "Sorties"}),
    )
    .await;
    assert_eq!(e.operations("alice", "").await, etat_alice);
    assert_eq!(e.a_classer("alice").await, 7);
    assert_eq!(e.a_classer("bruno").await, 7);
    assert_eq!(
        liste(&e.get("alice", "/rules").await.json(), "rules").len(),
        16
    );
    let noms: Vec<Value> = liste(&e.get("alice", "/categories").await.json(), "categories")
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    assert!(noms.contains(&json!("Loisirs et sorties")) && !noms.contains(&json!("Sorties")));
}

#[tokio::test]
async fn bilan_remboursement_deduit_et_epargne_hors_depenses() {
    let Some(e) = env().await else { return };
    e.importer("alice", &[("a.pdf", &mars())]).await;
    let loisirs = e.categorie("alice", "loisirs").await;
    let epargne = e.categorie("alice", "epargne").await;
    let revenus = e.categorie("alice", "revenus").await;
    let courses = e.categorie("alice", "courses").await;
    for (mot, categorie) in [
        ("JEUX", loisirs),
        ("CINE", loisirs),
        ("BERNARD", epargne),
        ("ATELIER", revenus),
    ] {
        e.envoyer(
            "alice",
            "POST",
            "/rules",
            &json!({"keywords": [mot], "category_id": categorie}),
        )
        .await;
    }
    let jour = |a, m, j| NaiveDate::from_ymd_opt(a, m, j).unwrap();
    let b = bilan::calculer(
        e.state.pool(),
        "sub-alice",
        jour(2025, 2, 1),
        jour(2025, 3, 31),
    )
    .await
    .unwrap();
    assert_eq!(b.par_categorie[&loisirs], 875 + 2415);
    assert_eq!(b.par_categorie[&courses], 960);
    assert_eq!((b.epargne, b.revenus), (3700, 190_000));
    assert_eq!(b.depenses_variables, 875 + 2415 + 960);
    assert_eq!(b.depenses_fixes, 1999 + 25_000 + 250 + 45_000);
    assert_eq!(b.depenses(), 76_499);
    assert_eq!(b.depenses() + b.epargne + b.a_classer_debits, 89_133 - 1599);
    assert_eq!(b.revenus + b.a_classer_credits, 192_839 - 1599);
    let fevrier = bilan::calculer(
        e.state.pool(),
        "sub-alice",
        jour(2025, 2, 20),
        jour(2025, 2, 20),
    )
    .await
    .unwrap();
    assert_eq!(fevrier.par_categorie[&loisirs], -1599);
    let personne = bilan::calculer(
        e.state.pool(),
        "sub-bruno",
        jour(2025, 2, 1),
        jour(2025, 3, 31),
    )
    .await
    .unwrap();
    assert_eq!(personne, bilan::Bilan::default());
}

#[tokio::test]
async fn migration_sur_une_base_de_l_etape_1() {
    let Some((base, url)) = postgres().await else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    crate::MIGRATIONS.run_to(1, &pool).await.unwrap();
    let compte: i64 = sqlx::query_scalar(
        "INSERT INTO comptes (sub, banque, iban) VALUES ('sub-alice', 'cmb', 'FR7600000000000000000000000') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let releve: i64 = sqlx::query_scalar(
        "INSERT INTO releves (compte_id, sub, numero, annee, debut, date_arrete, ancien_solde, nouveau_solde,
                              total_debits, total_credits, pages, empreinte, controle)
         VALUES ($1, 'sub-alice', 1, 2025, '2025-01-01', '2025-01-31', 0, 0, 0, 0, 1, 'e', '{}') RETURNING id",
    )
    .bind(compte)
    .fetch_one(&pool)
    .await
    .unwrap();
    for (rang, genre, section, libelle, brut) in [
        (
            0,
            "carte",
            "PAIEMENTS PAR CARTE",
            "CARTE 03/01 BOULANGERIE DU PONT 0042 VILLENEUVE",
            Some("BOULANGERIE DU PONT 0042 VILLENEUVE"),
        ),
        (
            1,
            "carte",
            "PAIEMENTS PAR CARTE",
            "CARTE 09/01 Boulangerie du Pont Villeneuve",
            Some("Boulangerie du Pont Villeneuve"),
        ),
        (
            2,
            "echeance_pret",
            "VIREMENTS EMIS ET PRELEVEMENTS",
            "ECH PRET 0000AB00000001",
            None,
        ),
        (
            3,
            "virement",
            "VIREMENTS EMIS ET PRELEVEMENTS",
            "VIR VERS COMPTE FICTIF",
            None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, section, type,
                                     libelle, commercant_brut, montant, sens)
             VALUES ($1, $2, 'sub-alice', $3, 1, '2025-01-10', '2025-01-10', $4, $5, $6, $7, 500, 'debit')",
        )
        .bind(releve)
        .bind(compte)
        .bind(rang)
        .bind(section)
        .bind(genre)
        .bind(libelle)
        .bind(brut)
        .execute(&pool)
        .await
        .unwrap();
    }
    crate::MIGRATIONS.run(&pool).await.unwrap();
    let vierges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM operations WHERE commercant_cle IS NULL AND commercant_id IS NULL AND categorie_id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vierges, 4);
    let cles: Vec<String> = sqlx::query_scalar(
        "SELECT conname::text FROM pg_constraint WHERE conrelid = 'operations'::regclass AND contype = 'f' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for attendue in ["operations_categorie_fk", "operations_commercant_fk"] {
        assert!(
            cles.iter().any(|c| c == attendue),
            "{attendue} absente de {cles:?}"
        );
    }
    let hors_table = sqlx::query("UPDATE operations SET categorie_id = 999999")
        .execute(&pool)
        .await;
    assert!(hors_table.is_err());

    let e = monter(base, &url, pool).await;
    let courses = e.categorie("alice", "courses").await;
    let credit = e.categorie("alice", "credit").await;
    let ops = e.operations("alice", "").await;
    assert_eq!(ops.len(), 4);
    let boulangeries: Vec<&Value> = ops.iter().filter(|o| o["category_id"] == courses).collect();
    assert_eq!(boulangeries.len(), 2);
    assert_eq!(
        boulangeries[0]["merchant_id"],
        boulangeries[1]["merchant_id"]
    );
    assert_eq!(
        boulangeries[0]["merchant_name"],
        "Boulangerie du Pont Villeneuve"
    );
    assert_eq!(e.classement("alice", "ECH PRET").await, de_regle(credit));
    assert_eq!(e.classement("alice", "COMPTE FICTIF").await, (None, None));
    assert_eq!(e.a_classer("alice").await, 1);
    assert_eq!(
        liste(&e.get("alice", "/merchants").await.json(), "merchants").len(),
        3
    );
}

#[tokio::test]
async fn releves_reels_categorises() {
    let Ok(dossier) = std::env::var("TIRELIRE_RELEVES_REELS") else {
        return;
    };
    let Some(e) = env().await else { return };
    let mut importes = 0;
    for entree in std::fs::read_dir(&dossier).expect("TIRELIRE_RELEVES_REELS : dossier lisible") {
        let chemin = entree.unwrap().path();
        if !chemin
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
        {
            continue;
        }
        let r = e
            .importer("alice", &[("releve.pdf", &std::fs::read(&chemin).unwrap())])
            .await
            .json();
        assert_eq!(r["results"][0]["status"], "imported", "relevé réel refusé");
        importes += 1;
    }
    assert!(importes > 0, "TIRELIRE_RELEVES_REELS : aucun PDF");
    let (total, classees, bruts, regroupes, sans_tiers): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(categorie_id), count(DISTINCT COALESCE(commercant_brut, libelle)),
                count(DISTINCT commercant_cle), count(*) FILTER (WHERE commercant_cle IS NULL)
         FROM operations",
    )
    .fetch_one(e.state.pool())
    .await
    .unwrap();
    let file = e.get("alice", "/unclassified/count").await.json();
    eprintln!(
        "relevés réels : {importes} relevé(s), {total} opérations, {classees} classées par les règles par défaut, {} à classer en {} groupes ; {bruts} commerçants ou libellés distincts avant regroupement, {regroupes} après ; {sans_tiers} opération(s) sans tiers",
        file["operations"], file["merchants"]
    );
    assert_eq!(file["operations"].as_i64(), Some(total - classees));
    let par_type: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT type, count(*), count(categorie_id) FROM operations GROUP BY type ORDER BY type",
    )
    .fetch_all(e.state.pool())
    .await
    .unwrap();
    for (genre, nombre, classees) in par_type {
        eprintln!("relevés réels : type {genre} : {classees} classées sur {nombre}");
    }
    let mut c = e.state.pool().acquire().await.unwrap();
    assert_eq!(classement::rejouer(&mut c, "sub-alice").await.unwrap(), 0);
}
