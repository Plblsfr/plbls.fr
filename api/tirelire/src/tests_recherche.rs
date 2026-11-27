use axum::http::StatusCode;
use chrono::NaiveDate;
use serde_json::{Value, json};

use crate::testutil::{Env, env};

struct Op {
    jour: &'static str,
    achat: Option<&'static str>,
    section: &'static str,
    genre: &'static str,
    libelle: &'static str,
    details: &'static [&'static str],
    brut: Option<&'static str>,
    montant: i64,
    sens: &'static str,
}

const fn carte(
    jour: &'static str,
    achat: &'static str,
    libelle: &'static str,
    brut: &'static str,
    montant: i64,
) -> Op {
    Op {
        jour,
        achat: Some(achat),
        section: "PAIEMENTS PAR CARTE",
        genre: "carte",
        libelle,
        details: &[],
        brut: Some(brut),
        montant,
        sens: "debit",
    }
}

const fn autre(
    jour: &'static str,
    section: &'static str,
    genre: &'static str,
    libelle: &'static str,
    details: &'static [&'static str],
    montant: i64,
    sens: &'static str,
) -> Op {
    Op {
        jour,
        achat: None,
        section,
        genre,
        libelle,
        details,
        brut: None,
        montant,
        sens,
    }
}

const EMIS: &str = "VIREMENTS EMIS ET PRELEVEMENTS";

const MARS: [Op; 7] = [
    carte(
        "2026-03-05",
        "2026-03-04",
        "CARTE 04/03 CRÈMERIE DE L'ÉTANG",
        "CRÈMERIE DE L'ÉTANG",
        1250,
    ),
    carte(
        "2026-03-06",
        "2026-03-05",
        "CARTE 05/03 CAFE 100% ARABICA",
        "CAFE 100% ARABICA",
        390,
    ),
    carte(
        "2026-03-07",
        "2026-03-06",
        "CARTE 06/03 CAFE 100 ARABICA",
        "CAFE 100 ARABICA",
        410,
    ),
    autre(
        "2026-03-08",
        EMIS,
        "prelevement",
        "PRLV SEPA TELECOM_IMAGINAIRE SA",
        &["REF EXEMPLE_0001", "MANDAT \"ABC\"; SUITE"],
        2999,
        "debit",
    ),
    autre(
        "2026-03-09",
        EMIS,
        "prelevement",
        "PRLV SEPA TELECOMXIMAGINAIRE",
        &[],
        1999,
        "debit",
    ),
    autre(
        "2026-03-10",
        "VIREMENTS RECUS",
        "virement",
        "VIR ATELIER MERIDIEN SALAIRE",
        &["Salaire de mars", "Œuvre sociale"],
        264_000,
        "credit",
    ),
    autre(
        "2026-03-11",
        "AUTRES OPERATIONS",
        "autre",
        "=SOMME(A1:A9)",
        &["+33 EXEMPLE", "@essai"],
        100,
        "debit",
    ),
];

const AVRIL: [Op; 2] = [
    autre(
        "2026-04-02",
        EMIS,
        "virement_instantane",
        "VIR INST vers BERNARD PAUL",
        &[],
        5000,
        "debit",
    ),
    carte(
        "2026-04-03",
        "2026-04-02",
        "CARTE 02/04 CREMERIE DE L'ETANG",
        "CREMERIE DE L'ETANG",
        800,
    ),
];

const DE_BRUNO: [Op; 1] = [carte(
    "2026-03-05",
    "2026-03-04",
    "CARTE 04/03 CRÈMERIE SECRÈTE",
    "CRÈMERIE SECRÈTE",
    777,
)];

fn jour(texte: &str) -> NaiveDate {
    NaiveDate::parse_from_str(texte, "%Y-%m-%d").unwrap()
}

fn encode(texte: &str) -> String {
    texte
        .bytes()
        .map(|o| match o {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' => (o as char).to_string(),
            _ => format!("%{o:02X}"),
        })
        .collect()
}

impl Env {
    async fn compte(&self, sub: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO comptes (sub, banque, iban) VALUES ($1, 'cmb', 'FR00 0000 0000 0000 0000 0000 000')
             ON CONFLICT (sub, banque, iban) DO UPDATE SET iban = EXCLUDED.iban RETURNING id",
        )
        .bind(sub)
        .fetch_one(self.state.pool())
        .await
        .unwrap()
    }

    async fn releve(&self, sub: &str, numero: i32, debut: &str, fin: &str, ops: &[Op]) -> i64 {
        let compte = self.compte(sub).await;
        let releve: i64 = sqlx::query_scalar(
            "INSERT INTO releves (compte_id, sub, numero, annee, debut, date_arrete, ancien_solde, nouveau_solde,
                                  total_debits, total_credits, pages, empreinte, controle)
             VALUES ($1, $2, $3, 2026, $4, $5, 0, 0, 0, 0, 1, $6, '{}') RETURNING id",
        )
        .bind(compte)
        .bind(sub)
        .bind(numero)
        .bind(jour(debut))
        .bind(jour(fin))
        .bind(format!("{sub}-{numero}"))
        .fetch_one(self.state.pool())
        .await
        .unwrap();
        for (rang, o) in ops.iter().enumerate() {
            let details: Vec<String> = o.details.iter().map(|d| (*d).to_string()).collect();
            sqlx::query(
                "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, date_achat,
                                         section, type, libelle, details, commercant_brut, montant, sens)
                 VALUES ($1, $2, $3, $4, 1, $5, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(releve)
            .bind(compte)
            .bind(sub)
            .bind(i32::try_from(rang).unwrap())
            .bind(jour(o.jour))
            .bind(o.achat.map(jour))
            .bind(o.section)
            .bind(o.genre)
            .bind(o.libelle)
            .bind(&details)
            .bind(o.brut)
            .bind(o.montant)
            .bind(o.sens)
            .execute(self.state.pool())
            .await
            .unwrap();
        }
        releve
    }

    async fn page(&self, qui: &str, filtre: &str) -> Value {
        let r = self
            .get(qui, &format!("/operations?limit=200{filtre}"))
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{filtre} : {}", r.texte());
        r.json()
    }

    async fn libelles(&self, qui: &str, filtre: &str) -> Vec<String> {
        let page = self.page(qui, filtre).await;
        let lignes = page["operations"].as_array().unwrap();
        assert_eq!(page["total"], lignes.len(), "{filtre}");
        lignes
            .iter()
            .map(|o| o["label"].as_str().unwrap().to_string())
            .collect()
    }

    async fn cherche(&self, qui: &str, q: &str) -> Vec<String> {
        self.libelles(qui, &format!("&q={}", encode(q))).await
    }

    async fn trouve(&self, qui: &str, libelle: &str) -> Value {
        self.page(qui, "").await["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["label"] == libelle)
            .cloned()
            .expect("opération semée")
    }

    async fn semer(&self) -> (i64, i64, i64) {
        let mars = self
            .releve("sub-alice", 3, "2026-03-01", "2026-03-31", &MARS)
            .await;
        let avril = self
            .releve("sub-alice", 4, "2026-04-01", "2026-04-30", &AVRIL)
            .await;
        let bruno = self
            .releve("sub-bruno", 3, "2026-03-01", "2026-03-31", &DE_BRUNO)
            .await;
        (mars, avril, bruno)
    }
}

const CREMERIE_MARS: &str = "CARTE 04/03 CRÈMERIE DE L'ÉTANG";
const CREMERIE_AVRIL: &str = "CARTE 02/04 CREMERIE DE L'ETANG";
const CAFE_POURCENT: &str = "CARTE 05/03 CAFE 100% ARABICA";
const CAFE: &str = "CARTE 06/03 CAFE 100 ARABICA";
const TELECOM_SOULIGNE: &str = "PRLV SEPA TELECOM_IMAGINAIRE SA";
const TELECOM: &str = "PRLV SEPA TELECOMXIMAGINAIRE";
const SALAIRE: &str = "VIR ATELIER MERIDIEN SALAIRE";
const FORMULE: &str = "=SOMME(A1:A9)";
const VIREMENT: &str = "VIR INST vers BERNARD PAUL";

#[tokio::test]
async fn recherche_sans_casse_ni_accents_et_jokers_litteraux() {
    let Some(e) = env().await else { return };
    e.semer().await;
    for q in [
        "cremerie",
        "CRÈMERIE",
        "crèmerie de l'étang",
        "Etang",
        "ÉTANG",
    ] {
        assert_eq!(
            e.cherche("alice", q).await,
            [CREMERIE_AVRIL, CREMERIE_MARS],
            "{q}"
        );
    }
    assert_eq!(e.cherche("alice", "100%").await, [CAFE_POURCENT]);
    assert_eq!(e.cherche("alice", "%").await, [CAFE_POURCENT]);
    assert_eq!(e.cherche("alice", "100").await, [CAFE, CAFE_POURCENT]);
    assert_eq!(e.cherche("alice", "telecom_").await, [TELECOM_SOULIGNE]);
    assert_eq!(e.cherche("alice", "_").await, [TELECOM_SOULIGNE]);
    assert_eq!(
        e.cherche("alice", "telecom").await,
        [TELECOM, TELECOM_SOULIGNE]
    );
    assert!(e.cherche("alice", "\\").await.is_empty());
    assert!(e.cherche("alice", "introuvable").await.is_empty());

    assert_eq!(e.cherche("alice", "oeuvre").await, [SALAIRE]);
    assert_eq!(e.cherche("alice", "salaire de MARS").await, [SALAIRE]);
    assert_eq!(e.cherche("alice", "  meridien   œuvre ").await, [SALAIRE]);
    assert_eq!(
        e.cherche("alice", "mandat \"abc\";").await,
        [TELECOM_SOULIGNE]
    );
    assert!(e.cherche("alice", "salaire cremerie").await.is_empty());
    assert_eq!(e.cherche("alice", "sa prlv").await, [TELECOM_SOULIGNE]);
    assert_eq!(e.cherche("alice", "arabica carte").await.len(), 2);
    assert_eq!(e.cherche("alice", "ARABICA\nCARTE").await.len(), 2);
    assert_eq!(e.libelles("alice", "&q=").await.len(), 9);
    assert_eq!(e.libelles("alice", "&q=%20%20").await.len(), 9);

    let paul = e.trouve("alice", VIREMENT).await;
    assert!(e.cherche("alice", "tonton").await.is_empty());
    let r = e
        .envoyer(
            "alice",
            "PATCH",
            &format!("/merchants/{}", paul["merchant_id"]),
            &json!({ "name": "Tonton Paulo" }),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    assert_eq!(e.cherche("alice", "tonton paulo").await, [VIREMENT]);
    assert_eq!(e.cherche("alice", "bernard").await, [VIREMENT]);

    for refuse in [
        format!("q={}", "a".repeat(101)),
        "q=a%20b%20c%20d%20e%20f%20g%20h%20i".to_string(),
        "min=abc".to_string(),
        "min=-1".to_string(),
        "max=1.5".to_string(),
        "min=200&max=100".to_string(),
        "direction=sortie".to_string(),
        "type=cheque".to_string(),
    ] {
        for chemin in ["/operations", "/operations/export.csv"] {
            let r = e.get("alice", &format!("{chemin}?{refuse}")).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{chemin}?{refuse}");
            assert_eq!(r.json()["error"], "invalid");
        }
    }
}

#[tokio::test]
async fn filtres_combines_et_totaux_de_la_selection() {
    let Some(e) = env().await else { return };
    let (mars, avril, _) = e.semer().await;

    let tout = e.page("alice", "").await;
    assert_eq!(tout["total"], 9);
    assert_eq!(
        tout["total_debits"],
        1250 + 390 + 410 + 2999 + 1999 + 100 + 5000 + 800
    );
    assert_eq!(tout["total_credits"], 264_000);

    let r = e.get("alice", "/operations?limit=1&offset=2").await.json();
    assert_eq!(r["operations"].as_array().unwrap().len(), 1);
    assert_eq!(r["operations"][0]["label"], FORMULE);
    assert_eq!(r["total"], 9);
    assert_eq!(r["total_debits"], 12_948);
    assert_eq!(r["total_credits"], 264_000);

    assert_eq!(
        e.libelles("alice", "&min=2999&max=5000").await,
        [VIREMENT, TELECOM_SOULIGNE]
    );
    assert_eq!(e.libelles("alice", "&min=5001").await, [SALAIRE]);
    assert_eq!(e.libelles("alice", "&max=100").await, [FORMULE]);
    assert_eq!(
        e.libelles("alice", "&min=390&max=390").await,
        [CAFE_POURCENT]
    );
    assert_eq!(e.libelles("alice", "&direction=credit").await, [SALAIRE]);
    assert_eq!(e.libelles("alice", "&direction=debit").await.len(), 8);
    assert_eq!(
        e.libelles("alice", "&type=prelevement").await,
        [TELECOM, TELECOM_SOULIGNE]
    );
    assert_eq!(e.libelles("alice", "&type=virement").await, [SALAIRE]);
    assert_eq!(
        e.libelles("alice", "&type=virement_instantane").await,
        [VIREMENT]
    );
    assert_eq!(e.libelles("alice", "&type=autre").await, [FORMULE]);
    assert!(e.libelles("alice", "&type=cotisation").await.is_empty());

    assert_eq!(
        e.libelles("alice", "&q=cremerie&from=2026-04-01").await,
        [CREMERIE_AVRIL]
    );
    assert_eq!(
        e.libelles("alice", "&q=cremerie&to=2026-03-31").await,
        [CREMERIE_MARS]
    );
    assert_eq!(
        e.libelles("alice", &format!("&q=cremerie&statement={avril}"))
            .await,
        [CREMERIE_AVRIL]
    );
    assert_eq!(
        e.libelles("alice", &format!("&statement={mars}&type=carte&max=400"))
            .await,
        [CAFE_POURCENT]
    );
    assert_eq!(
        e.libelles("alice", &format!("&statement={avril}&direction=debit"))
            .await,
        [CREMERIE_AVRIL, VIREMENT]
    );
    assert!(
        e.libelles("alice", "&direction=debit&type=virement")
            .await
            .is_empty()
    );
    assert_eq!(
        e.libelles("alice", "&type=carte&min=400&q=arabica").await,
        [CAFE]
    );
    let p = e
        .page(
            "alice",
            "&type=carte&direction=debit&from=2026-03-06&to=2026-04-03",
        )
        .await;
    assert_eq!(p["total"], 3);
    assert_eq!(p["total_debits"], 390 + 410 + 800);
    assert_eq!(p["total_credits"], 0);
    let p = e.page("alice", "&q=salaire&min=1").await;
    assert_eq!(
        (&p["total"], &p["total_debits"], &p["total_credits"]),
        (&json!(1), &json!(0), &json!(264_000))
    );
    let p = e.page("alice", "&q=introuvable").await;
    assert_eq!(
        (&p["total"], &p["total_debits"], &p["total_credits"]),
        (&json!(0), &json!(0), &json!(0))
    );

    let cremerie = e.trouve("alice", CREMERIE_MARS).await;
    let commercant = cremerie["merchant_id"].as_i64().unwrap();
    assert_eq!(
        e.trouve("alice", CREMERIE_AVRIL).await["merchant_id"],
        commercant
    );
    let courses = e.get("alice", "/categories").await.json()["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "courses")
        .and_then(|c| c["id"].as_i64())
        .unwrap();
    let r = e
        .envoyer(
            "alice",
            "PUT",
            &format!("/operations/{}/category", cremerie["id"]),
            &json!({ "category_id": courses, "scope": "operation" }),
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    assert_eq!(
        e.libelles("alice", &format!("&category={courses}&q=cremerie"))
            .await,
        [CREMERIE_MARS]
    );
    assert_eq!(
        e.libelles("alice", "&unclassified=true&q=cremerie").await,
        [CREMERIE_AVRIL]
    );
    assert_eq!(
        e.libelles("alice", &format!("&merchant={commercant}"))
            .await,
        [CREMERIE_AVRIL, CREMERIE_MARS]
    );
    assert_eq!(
        e.libelles("alice", &format!("&merchant={commercant}&min=1000"))
            .await,
        [CREMERIE_MARS]
    );
    assert!(
        e.libelles("alice", &format!("&merchant={commercant}&direction=credit"))
            .await
            .is_empty()
    );
    let p = e
        .page("alice", "&unclassified=true&direction=debit&max=1000")
        .await;
    let attendues: Vec<&str> = p["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["label"].as_str().unwrap())
        .collect();
    assert_eq!(attendues, [CREMERIE_AVRIL, FORMULE, CAFE, CAFE_POURCENT]);
    assert_eq!(p["total_debits"], 800 + 100 + 410 + 390);
}

#[tokio::test]
async fn recherche_et_export_cloisonnes_par_personne() {
    let Some(e) = env().await else { return };
    let (mars, _, de_bruno) = e.semer().await;
    const SECRETE: &str = "CARTE 04/03 CRÈMERIE SECRÈTE";

    assert_eq!(e.cherche("bruno", "cremerie").await, [SECRETE]);
    assert_eq!(e.cherche("bruno", "secrete").await, [SECRETE]);
    assert!(e.cherche("alice", "secrete").await.is_empty());
    assert!(e.cherche("bruno", "etang").await.is_empty());
    assert!(e.cherche("bruno", "salaire").await.is_empty());
    assert!(
        e.libelles("alice", &format!("&statement={de_bruno}"))
            .await
            .is_empty()
    );
    assert!(
        e.libelles("bruno", &format!("&statement={mars}"))
            .await
            .is_empty()
    );
    let p = e.page("bruno", "").await;
    assert_eq!(
        (&p["total"], &p["total_debits"], &p["total_credits"]),
        (&json!(1), &json!(777), &json!(0))
    );
    let d_alice = e.trouve("alice", CREMERIE_MARS).await["merchant_id"]
        .as_i64()
        .unwrap();
    assert!(
        e.libelles("bruno", &format!("&merchant={d_alice}"))
            .await
            .is_empty()
    );

    let r = e.get("bruno", "/operations/export.csv").await;
    assert_eq!(r.statut, StatusCode::OK);
    let csv = r.texte();
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.contains("SECRÈTE"));
    assert!(!csv.contains("ARABICA") && !csv.contains("SALAIRE") && !csv.contains("ÉTANG"));
    assert_eq!(
        r.entetes["content-disposition"],
        "attachment; filename=\"tirelire-operations-2026-03-05_2026-03-05.csv\""
    );
    let r = e
        .get("bruno", &format!("/operations/export.csv?statement={mars}"))
        .await;
    assert_eq!(r.texte().lines().count(), 1);
    let r = e.get("alice", "/operations/export.csv").await;
    let csv = r.texte();
    assert_eq!(csv.lines().count(), 10);
    assert!(!csv.contains("SECRÈTE") && !csv.contains("7,77"));
    let r = e.get("alice", "/operations/export.csv?q=secrete").await;
    assert_eq!(r.texte().lines().count(), 1);

    assert_eq!(
        e.appel(None, "GET", "/operations/export.csv").await.statut,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.get("personne", "/operations/export.csv").await.statut,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn export_csv_exact() {
    let Some(e) = env().await else { return };
    e.semer().await;
    let r = e
        .get(
            "alice",
            "/operations/export.csv?from=2026-03-08&to=2026-04-03",
        )
        .await;
    assert_eq!(r.statut, StatusCode::OK);
    assert_eq!(r.entetes["content-type"], "text/csv; charset=utf-8");
    assert_eq!(r.entetes["cache-control"], "no-store");
    assert_eq!(
        r.entetes["content-disposition"],
        "attachment; filename=\"tirelire-operations-2026-03-08_2026-04-03.csv\""
    );
    assert_eq!(&r.corps[..3], [0xEF, 0xBB, 0xBF]);
    let attendu = [
        "\u{feff}Date;Date de valeur;Date d'achat;Libellé;Détails;Commerçant;Catégorie;Nature;Type;Section;Débit;Crédit;Montant;Relevé",
        "03/04/2026;03/04/2026;02/04/2026;CARTE 02/04 CREMERIE DE L'ETANG;;Cremerie de L Etang;;;Carte;PAIEMENTS PAR CARTE;8,00;;-8,00;Relevé n° 4 de 2026",
        "02/04/2026;02/04/2026;;VIR INST vers BERNARD PAUL;;Bernard Paul;;;Virement instantané;VIREMENTS EMIS ET PRELEVEMENTS;50,00;;-50,00;Relevé n° 4 de 2026",
        "11/03/2026;11/03/2026;;'=SOMME(A1:A9);'+33 EXEMPLE | @essai;Somme A1 A9;;;Autre opération;AUTRES OPERATIONS;1,00;;-1,00;Relevé n° 3 de 2026",
        "10/03/2026;10/03/2026;;VIR ATELIER MERIDIEN SALAIRE;Salaire de mars | Œuvre sociale;Atelier Meridien Salaire;Revenus;Revenu;Virement;VIREMENTS RECUS;;2640,00;2640,00;Relevé n° 3 de 2026",
        "09/03/2026;09/03/2026;;PRLV SEPA TELECOMXIMAGINAIRE;;Telecomximaginaire;;;Prélèvement;VIREMENTS EMIS ET PRELEVEMENTS;19,99;;-19,99;Relevé n° 3 de 2026",
        "08/03/2026;08/03/2026;;PRLV SEPA TELECOM_IMAGINAIRE SA;\"REF EXEMPLE_0001 | MANDAT \"\"ABC\"\"; SUITE\";Telecom Imaginaire;Télécom;Dépense fixe;Prélèvement;VIREMENTS EMIS ET PRELEVEMENTS;29,99;;-29,99;Relevé n° 3 de 2026",
        "",
    ]
    .join("\r\n");
    assert_eq!(r.texte(), attendu);

    let r = e
        .get(
            "alice",
            "/operations/export.csv?q=introuvable&from=2026-03-01",
        )
        .await;
    assert_eq!(
        r.entetes["content-disposition"],
        "attachment; filename=\"tirelire-operations-depuis-2026-03-01.csv\""
    );
    assert_eq!(
        r.texte(),
        format!("{}{}", crate::export::BOM, crate::export::ENTETE)
    );

    let r = e
        .get(
            "alice",
            "/operations/export.csv?q=cremerie&direction=debit&type=carte&min=1000",
        )
        .await;
    let csv = r.texte();
    let lignes: Vec<&str> = csv.lines().collect();
    assert_eq!(lignes.len(), 2);
    assert!(
        lignes[1].starts_with("05/03/2026;05/03/2026;04/03/2026;CARTE 04/03 CRÈMERIE DE L'ÉTANG;;")
    );
    assert!(lignes[1].ends_with(";12,50;;-12,50;Relevé n° 3 de 2026"));
}

#[tokio::test]
async fn export_en_flux_de_toutes_les_lignes() {
    let Some(e) = env().await else { return };
    let releve = e
        .releve("sub-alice", 5, "2026-05-01", "2026-05-31", &[])
        .await;
    sqlx::query(
        "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, section, type, libelle, montant, sens)
         SELECT $1, r.compte_id, r.sub, n, 1, DATE '2026-05-01' + (n % 31), DATE '2026-05-01' + (n % 31),
                'PAIEMENTS PAR CARTE', 'autre', 'OPERATION EXEMPLE ' || n, n, CASE WHEN n % 5 = 0 THEN 'credit' ELSE 'debit' END
         FROM releves r, generate_series(1, 3000) n WHERE r.id = $1",
    )
    .bind(releve)
    .execute(e.state.pool())
    .await
    .unwrap();
    let r = e.get("alice", "/operations/export.csv").await;
    assert_eq!(r.statut, StatusCode::OK);
    let csv = r.texte();
    assert!(csv.len() > 64 * 1024);
    assert_eq!(csv.lines().count(), 3001);
    assert!(csv.lines().all(|l| l.split(';').count() == 14));
    assert!(csv.ends_with("\r\n"));
    let p = e.page("alice", "").await;
    assert_eq!(p["total"], 3000);
    let credits: i64 = (1..=3000).filter(|n| n % 5 == 0).sum();
    let debits: i64 = (1..=3000).filter(|n| n % 5 != 0).sum();
    assert_eq!(p["total_credits"], credits);
    assert_eq!(p["total_debits"], debits);
    let r = e
        .get("alice", "/operations/export.csv?direction=credit&min=2990")
        .await;
    assert_eq!(r.texte().lines().count(), 4);
}
