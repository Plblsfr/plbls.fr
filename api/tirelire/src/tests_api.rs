use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::{
    garde::Garde,
    import, routes,
    testutil::{PASSAGE_ANNEE, TOTAL_FAUX, TROIS_PAGES, appel_sur, env, pages, pdf},
};

fn trois_pages() -> Vec<u8> {
    pdf(&pages(TROIS_PAGES), "a")
}

fn passage_annee() -> Vec<u8> {
    pdf(&pages(PASSAGE_ANNEE), "a")
}

fn nombre(v: &Value, cle: &str) -> usize {
    v[cle].as_array().map_or(usize::MAX, Vec::len)
}

#[tokio::test]
async fn chaque_route_protegee_refuse_sans_session_et_sans_permission() {
    let Some(e) = env().await else { return };
    let mut protegees = 0;
    for (route, _) in routes::table() {
        let chemin = route.chemin.replace("{id}", "1");
        let m = route.methode.as_str();
        let appel = |qui: Option<&'static str>| appel_sur(&e.app, qui, m, &chemin);
        match route.garde {
            Garde::Droit(d) => {
                protegees += 1;
                for qui in [None, Some("invalide")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
                let r = appel(Some("personne")).await;
                assert_eq!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin}");
                assert!(r.texte().contains(&d.nom()), "{m} {chemin} : {}", r.texte());
                let r = appel(Some("alice")).await;
                assert_ne!(r.statut, StatusCode::UNAUTHORIZED, "{m} {chemin}");
                assert_ne!(r.statut, StatusCode::FORBIDDEN, "{m} {chemin}");
            }
            Garde::Session(_) => {
                for qui in [None, Some("invalide")] {
                    assert_eq!(
                        appel(qui).await.statut,
                        StatusCode::UNAUTHORIZED,
                        "{m} {chemin} pour {qui:?}"
                    );
                }
                assert_eq!(appel(Some("personne")).await.statut, StatusCode::OK);
            }
            Garde::Public(_) => {
                assert_eq!(appel(None).await.statut, StatusCode::OK, "{m} {chemin}");
            }
        }
    }
    assert_eq!(protegees, 5);
}

#[tokio::test]
async fn moi_annonce_la_permission() {
    let Some(e) = env().await else { return };
    assert_eq!(
        e.get("alice", "/me").await.json(),
        json!({"sub": "sub-alice", "name": "alice", "email": "alice@x.fr", "permissions": {"access": true}})
    );
    assert_eq!(
        e.get("personne", "/me").await.json()["permissions"]["access"],
        false
    );
}

#[tokio::test]
async fn import_de_plusieurs_fichiers_traites_independamment() {
    let Some(e) = env().await else { return };
    let r = e
        .importer(
            "alice",
            &[
                ("mars.pdf", &trois_pages()),
                ("faux.pdf", &pdf(&pages(TOTAL_FAUX), "a")),
                ("notes.txt", b"bonjour"),
                ("janvier.pdf", &passage_annee()),
            ],
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    let resultats = r.json()["results"].clone();
    let vus: Vec<(&str, &str)> = resultats
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (x["file"].as_str().unwrap(), x["status"].as_str().unwrap()))
        .collect();
    assert_eq!(
        vus,
        [
            ("mars.pdf", "imported"),
            ("faux.pdf", "rejected"),
            ("notes.txt", "rejected"),
            ("janvier.pdf", "imported")
        ]
    );
    let mars = &resultats[0]["statement"];
    assert_eq!(
        (
            &mars["number"],
            &mars["year"],
            &mars["from"],
            &mars["to"],
            &mars["operations"],
            &mars["pages"]
        ),
        (
            &json!(3),
            &json!(2025),
            &json!("2025-02-07"),
            &json!("2025-03-07"),
            &json!(15),
            &json!(3)
        )
    );
    assert_eq!(
        (
            &mars["old_balance"],
            &mars["total_debits"],
            &mars["total_credits"],
            &mars["new_balance"]
        ),
        (
            &json!(120_436),
            &json!(89_133),
            &json!(192_839),
            &json!(224_142)
        )
    );
    assert_eq!(
        mars["checks"],
        json!({
            "total_debits": {"expected": 89_133, "read": 89_133},
            "total_credits": {"expected": 192_839, "read": 192_839},
            "new_balance": {"expected": 224_142, "read": 224_142},
            "operations": 15,
            "unrecognized_lines": 0,
        })
    );
    assert_eq!(mars["sha256"], import::empreinte(&trois_pages()));
    let faux = &resultats[1]["error"];
    assert_eq!(faux["code"], "checks_failed");
    assert_eq!(
        faux["gaps"],
        json!([{"check": "total_debits", "expected": 99_999, "read": 89_133}])
    );
    assert_eq!(nombre(faux, "unrecognized_lines"), 2);
    assert_eq!(faux["unrecognized_lines"][1]["page"], 1);
    assert_eq!(resultats[2]["error"]["code"], "not_a_pdf");
    assert_eq!(resultats[3]["statement"]["old_balance"], -12_000);

    let releves = e.get("alice", "/statements").await.json();
    let numeros: Vec<(i64, i64)> = releves["statements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (x["year"].as_i64().unwrap(), x["number"].as_i64().unwrap()))
        .collect();
    assert_eq!(numeros, [(2026, 1), (2025, 3)]);
    let comptes = e.get("alice", "/accounts").await.json();
    assert_eq!(nombre(&comptes, "accounts"), 1);
    let compte = &comptes["accounts"][0];
    assert_eq!(
        (
            &compte["bank"],
            &compte["iban"],
            &compte["bic"],
            &compte["holder"],
            &compte["statements"]
        ),
        (
            &json!("cmb"),
            &json!("FR7612345678900123456789034"),
            &json!("FICTFRPPXXX"),
            &json!("MME LEGRAND CAMILLE"),
            &json!(2)
        )
    );
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 20);
}

#[tokio::test]
async fn operations_paginees_et_filtrees() {
    let Some(e) = env().await else { return };
    let r = e
        .importer(
            "alice",
            &[("a.pdf", &trois_pages()), ("b.pdf", &passage_annee())],
        )
        .await
        .json();
    let mars = r["results"][0]["statement"]["id"].as_i64().unwrap();
    let page = e.get("alice", "/operations?limit=3").await.json();
    assert_eq!(
        (&page["total"], &page["limit"], &page["offset"]),
        (&json!(20), &json!(3), &json!(0))
    );
    let dates: Vec<&str> = page["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["date"].as_str().unwrap())
        .collect();
    assert_eq!(dates, ["2026-01-05", "2026-01-02", "2026-01-02"]);
    let suite = e.get("alice", "/operations?limit=3&offset=3").await.json();
    assert_eq!(suite["operations"][0]["date"], "2025-12-31");
    assert_eq!(nombre(&suite, "operations"), 3);
    let fin = e
        .get("alice", "/operations?limit=200&offset=18")
        .await
        .json();
    assert_eq!(nombre(&fin, "operations"), 2);

    let du_releve = e
        .get("alice", &format!("/operations?statement={mars}&limit=200"))
        .await
        .json();
    assert_eq!(du_releve["total"], 15);
    let remboursement = du_releve["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["direction"] == "credit" && o["type"] == "carte")
        .unwrap();
    assert_eq!(
        *remboursement,
        json!({
            "id": remboursement["id"],
            "statement_id": mars,
            "account_id": remboursement["account_id"],
            "date": "2025-02-20",
            "value_date": "2025-02-20",
            "purchase_date": "2025-02-20",
            "section": "PAIEMENTS PAR CARTE",
            "type": "carte",
            "label": "CARTE 20/02 Jeux En Ligne Fictifs Fictiville",
            "details": [],
            "merchant": "Jeux En Ligne Fictifs Fictiville",
            "amount": 1599,
            "direction": "credit",
            "merchant_id": null,
            "category_id": null,
        })
    );
    let prelevement = du_releve["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["label"] == "PRLV SEPA TELECOM IMAGINAIRE SA")
        .unwrap();
    assert_eq!(nombre(prelevement, "details"), 4);
    assert_eq!(prelevement["merchant"], Value::Null);

    let periode = e
        .get("alice", "/operations?from=2025-12-08&to=2026-01-02")
        .await
        .json();
    assert_eq!(periode["total"], 4);
    let achat = periode["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["amount"] == 123_456)
        .unwrap();
    assert_eq!(
        (&achat["date"], &achat["purchase_date"]),
        (&json!("2026-01-02"), &json!("2025-12-30"))
    );
    assert_eq!(
        e.get("alice", "/operations?from=2025-02-20&to=2025-02-20")
            .await
            .json()["total"],
        1
    );
    for faux in [
        "?limit=0",
        "?limit=201",
        "?limit=x",
        "?offset=-1",
        "?statement=abc",
        "?from=20250101",
        "?to=hier",
    ] {
        let r = e.get("alice", &format!("/operations{faux}")).await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
    }
}

#[tokio::test]
async fn reimport_idempotent() {
    let Some(e) = env().await else { return };
    let premier = e
        .importer("alice", &[("a.pdf", &trois_pages())])
        .await
        .json();
    let id = premier["results"][0]["statement"]["id"].clone();
    assert_eq!(premier["results"][0]["status"], "imported");
    let meme_releve_autre_fichier = pdf(&pages(TROIS_PAGES), "autre export");
    assert_ne!(meme_releve_autre_fichier, trois_pages());
    let second = e
        .importer(
            "alice",
            &[
                ("a.pdf", &trois_pages()),
                ("copie.pdf", &meme_releve_autre_fichier),
                ("encore.pdf", &trois_pages()),
            ],
        )
        .await
        .json();
    for i in 0..3 {
        let r = &second["results"][i];
        assert_eq!(r["status"], "already_imported", "{i}");
        assert_eq!(r["statement"]["id"], id, "{i}");
        assert_eq!(r["statement"]["sha256"], import::empreinte(&trois_pages()));
    }
    assert_eq!(
        nombre(&e.get("alice", "/statements").await.json(), "statements"),
        1
    );
    assert_eq!(
        nombre(&e.get("alice", "/accounts").await.json(), "accounts"),
        1
    );
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 15);
}

#[tokio::test]
async fn releve_refuse_n_enregistre_rien() {
    let Some(e) = env().await else { return };
    let r = e
        .importer("alice", &[("faux.pdf", &pdf(&pages(TOTAL_FAUX), "a"))])
        .await
        .json();
    assert_eq!(r["results"][0]["status"], "rejected");
    assert_eq!(r["results"][0]["statement"], Value::Null);
    assert_eq!(
        nombre(&e.get("alice", "/statements").await.json(), "statements"),
        0
    );
    assert_eq!(
        nombre(&e.get("alice", "/accounts").await.json(), "accounts"),
        0
    );
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 0);
}

#[tokio::test]
async fn donnees_cloisonnees_par_personne() {
    let Some(e) = env().await else { return };
    let alice = e
        .importer("alice", &[("a.pdf", &trois_pages())])
        .await
        .json();
    let id_alice = alice["results"][0]["statement"]["id"].as_i64().unwrap();
    for chemin in ["/statements", "/accounts", "/operations"] {
        let vu = e.get("bruno", chemin).await.json();
        let cle = chemin.trim_start_matches('/');
        assert_eq!(nombre(&vu, cle), 0, "{chemin}");
    }
    assert_eq!(
        e.get("bruno", &format!("/operations?statement={id_alice}"))
            .await
            .json()["total"],
        0
    );
    assert_eq!(
        e.appel(Some("bruno"), "DELETE", &format!("/statements/{id_alice}"))
            .await
            .statut,
        StatusCode::NOT_FOUND
    );
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 15);

    let bruno = e
        .importer("bruno", &[("a.pdf", &trois_pages())])
        .await
        .json();
    assert_eq!(bruno["results"][0]["status"], "imported");
    let id_bruno = bruno["results"][0]["statement"]["id"].as_i64().unwrap();
    assert_ne!(id_bruno, id_alice);
    assert_ne!(
        bruno["results"][0]["statement"]["account_id"],
        alice["results"][0]["statement"]["account_id"]
    );
    assert_eq!(
        e.appel(Some("bruno"), "DELETE", &format!("/statements/{id_bruno}"))
            .await
            .statut,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        nombre(&e.get("bruno", "/accounts").await.json(), "accounts"),
        0
    );
    assert_eq!(
        nombre(&e.get("alice", "/statements").await.json(), "statements"),
        1
    );
    assert_eq!(
        nombre(&e.get("alice", "/accounts").await.json(), "accounts"),
        1
    );
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 15);
}

#[tokio::test]
async fn suppression_d_un_releve() {
    let Some(e) = env().await else { return };
    let r = e
        .importer(
            "alice",
            &[("a.pdf", &trois_pages()), ("b.pdf", &passage_annee())],
        )
        .await
        .json();
    let mars = r["results"][0]["statement"]["id"].as_i64().unwrap();
    let janvier = r["results"][1]["statement"]["id"].as_i64().unwrap();
    let app = &e.app;
    let supprimer = |id: String| async move {
        appel_sur(app, Some("alice"), "DELETE", &format!("/statements/{id}"))
            .await
            .statut
    };
    assert_eq!(supprimer(mars.to_string()).await, StatusCode::NO_CONTENT);
    assert_eq!(supprimer(mars.to_string()).await, StatusCode::NOT_FOUND);
    assert_eq!(supprimer("abc".into()).await, StatusCode::NOT_FOUND);
    assert_eq!(e.get("alice", "/operations").await.json()["total"], 5);
    assert_eq!(
        nombre(&e.get("alice", "/accounts").await.json(), "accounts"),
        1
    );
    assert_eq!(supprimer(janvier.to_string()).await, StatusCode::NO_CONTENT);
    assert_eq!(
        nombre(&e.get("alice", "/accounts").await.json(), "accounts"),
        0
    );
    let orphelines: i64 = sqlx::query_scalar("SELECT count(*) FROM operations")
        .fetch_one(e.state.pool())
        .await
        .unwrap();
    assert_eq!(orphelines, 0);
    let de_nouveau = e
        .importer("alice", &[("a.pdf", &trois_pages())])
        .await
        .json();
    assert_eq!(de_nouveau["results"][0]["status"], "imported");
}

#[tokio::test]
async fn limites_de_l_envoi() {
    let Some(e) = env().await else { return };
    assert_eq!(
        e.importer("alice", &[]).await.statut,
        StatusCode::BAD_REQUEST
    );
    let vide = e.appel(Some("alice"), "POST", "/statements/import").await;
    assert_eq!(vide.statut, StatusCode::BAD_REQUEST);
    let trop: Vec<(&str, &[u8])> = vec![("x.txt", b"x".as_slice()); import::FICHIERS_MAX + 1];
    let r = e.importer("alice", &trop).await;
    assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{}", r.texte());
    let gros = vec![b'%'; import::FICHIER_MAX + 1];
    let r = e
        .importer("alice", &[("gros.pdf", &gros), ("a.pdf", &trois_pages())])
        .await
        .json();
    assert_eq!(r["results"][0]["error"]["code"], "too_large");
    assert_eq!(r["results"][1]["status"], "imported");
    let enorme = vec![b'%'; import::REQUETE_MAX + 1];
    let r = e.importer("alice", &[("enorme.pdf", &enorme)]).await;
    assert_eq!(r.statut, StatusCode::PAYLOAD_TOO_LARGE, "{}", r.texte());
    assert_eq!(
        nombre(&e.get("alice", "/statements").await.json(), "statements"),
        1
    );
}

#[tokio::test]
async fn requete_modifiante_avec_cookie_exige_une_origine_autorisee() {
    let Some(e) = env().await else { return };
    let appel = |origine: Option<&'static str>| {
        let mut b = axum::http::Request::builder()
            .method("DELETE")
            .uri("/statements/1")
            .header("cookie", "__Secure-plbls_sid=alice");
        if let Some(o) = origine {
            b = b.header("origin", o);
        }
        crate::testutil::requete(&e.app, b.body(axum::body::Body::empty()).unwrap())
    };
    assert_eq!(appel(None).await.statut, StatusCode::FORBIDDEN);
    assert_eq!(
        appel(Some("https://ailleurs.example")).await.statut,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        appel(Some("https://estompe.plbls.fr")).await.statut,
        StatusCode::NOT_FOUND
    );
}
