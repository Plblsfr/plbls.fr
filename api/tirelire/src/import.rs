use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::Response,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
    classement,
    cmb::{self, Refus, Releve, Sens},
    etat::AppState,
    extraction::{self, Erreur},
    garde::Personne,
    http::{bd, erreur, invalide, ok},
    normalisation, releves,
};

pub const BANQUE: &str = "cmb";
pub const REQUETE_MAX: usize = 32 * 1024 * 1024;
pub const FICHIER_MAX: usize = 8 * 1024 * 1024;
pub const FICHIERS_MAX: usize = 24;
const NOM_MAX: usize = 200;

pub fn empreinte(octets: &[u8]) -> String {
    Sha256::digest(octets)
        .iter()
        .map(|o| format!("{o:02x}"))
        .collect()
}

fn refus(code: &str, message: &str) -> Value {
    json!({ "status": "rejected", "error": { "code": code, "message": message } })
}

fn refus_d_extraction(e: Erreur) -> Value {
    match e {
        Erreur::PasUnPdf => refus("not_a_pdf", "le fichier n'est pas un PDF"),
        Erreur::Illisible => refus("unreadable_pdf", "PDF illisible ou protégé"),
        Erreur::TropGrand => refus("too_large", "PDF trop volumineux"),
    }
}

fn refus_d_analyse(r: &Refus) -> Value {
    if r.format_inconnu {
        return refus(
            "unknown_format",
            "ce PDF n'est pas un relevé de compte du Crédit Mutuel de Bretagne",
        );
    }
    json!({
        "status": "rejected",
        "error": {
            "code": "checks_failed",
            "message": "relevé refusé : les contrôles ne passent pas",
            "missing": r.manquants,
            "gaps": r.ecarts,
            "unrecognized_lines": r.lignes,
        },
    })
}

pub fn lire(octets: &[u8]) -> Result<Releve, Value> {
    let pages = extraction::mots_du_pdf(octets).map_err(refus_d_extraction)?;
    cmb::analyser(&pages).map_err(|r| refus_d_analyse(&r))
}

fn controle(r: &Releve) -> Value {
    let pareil = |v: i64| json!({ "expected": v, "read": v });
    json!({
        "total_debits": pareil(r.total_debits),
        "total_credits": pareil(r.total_credits),
        "new_balance": pareil(r.nouveau_solde),
        "operations": r.operations.len(),
        "unrecognized_lines": 0,
    })
}

async fn enregistrer(
    pool: &PgPool,
    sub: &str,
    empreinte: &str,
    r: &Releve,
) -> Result<Option<i64>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    classement::verrouiller(&mut tx, sub).await?;
    let compte: i64 = sqlx::query_scalar(
        "INSERT INTO comptes (sub, banque, iban, bic, numero, reference, titulaire)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (sub, banque, iban) DO UPDATE
         SET bic = EXCLUDED.bic, numero = EXCLUDED.numero, reference = EXCLUDED.reference, titulaire = EXCLUDED.titulaire
         RETURNING id",
    )
    .bind(sub)
    .bind(BANQUE)
    .bind(&r.compte.iban)
    .bind(&r.compte.bic)
    .bind(&r.compte.numero)
    .bind(&r.compte.reference)
    .bind(&r.compte.titulaire)
    .fetch_one(&mut *tx)
    .await?;
    let releve: Option<i64> = sqlx::query_scalar(
        "INSERT INTO releves (compte_id, sub, numero, annee, debut, date_arrete, ancien_solde, nouveau_solde,
                              total_debits, total_credits, pages, empreinte, controle)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (compte_id, annee, numero, date_arrete) DO NOTHING
         RETURNING id",
    )
    .bind(compte)
    .bind(sub)
    .bind(r.numero)
    .bind(r.annee)
    .bind(r.debut)
    .bind(r.date_arrete)
    .bind(r.ancien_solde)
    .bind(r.nouveau_solde)
    .bind(r.total_debits)
    .bind(r.total_credits)
    .bind(i32::try_from(r.pages).unwrap_or(i32::MAX))
    .bind(empreinte)
    .bind(controle(r))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(releve) = releve else {
        tx.rollback().await?;
        return Ok(None);
    };
    for (rang, op) in r.operations.iter().enumerate() {
        sqlx::query(
            "INSERT INTO operations (releve_id, compte_id, sub, rang, page, date_operation, date_valeur, date_achat,
                                     section, type, libelle, details, commercant_brut, montant, sens, commercant_cle)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(releve)
        .bind(compte)
        .bind(sub)
        .bind(i32::try_from(rang).unwrap_or(i32::MAX))
        .bind(i32::try_from(op.page).unwrap_or(i32::MAX))
        .bind(op.date_operation)
        .bind(op.date_valeur)
        .bind(op.date_achat)
        .bind(&op.section)
        .bind(op.genre.cle())
        .bind(&op.libelle)
        .bind(&op.details)
        .bind(&op.commercant)
        .bind(op.montant)
        .bind(match op.sens {
            Sens::Debit => "debit",
            Sens::Credit => "credit",
        })
        .bind(
            normalisation::tiers(op.genre.cle(), &op.libelle, op.commercant.as_deref())
                .map(|t| t.cle),
        )
        .execute(&mut *tx)
        .await?;
    }
    classement::rejouer(&mut tx, sub).await?;
    tx.commit().await?;
    Ok(Some(releve))
}

async fn deja(pool: &PgPool, sub: &str, id: i64) -> Result<Value, sqlx::Error> {
    Ok(json!({ "status": "already_imported", "statement": releves::lire(pool, sub, id).await? }))
}

pub async fn traiter(pool: &PgPool, sub: &str, octets: Vec<u8>) -> Result<Value, sqlx::Error> {
    if octets.len() > FICHIER_MAX {
        return Ok(refus("too_large", "fichier trop volumineux"));
    }
    let empreinte = empreinte(&octets);
    if let Some(id) = releves::par_empreinte(pool, sub, &empreinte).await? {
        return deja(pool, sub, id).await;
    }
    let lecture = tokio::task::spawn_blocking(move || lire(&octets)).await;
    let releve = match lecture {
        Ok(Ok(r)) => r,
        Ok(Err(refus)) => return Ok(refus),
        Err(_) => return Ok(refus_d_extraction(Erreur::Illisible)),
    };
    match enregistrer(pool, sub, &empreinte, &releve).await? {
        Some(id) => {
            tracing::info!(
                "relevé importé : {} opérations, {} pages",
                releve.operations.len(),
                releve.pages
            );
            Ok(json!({ "status": "imported", "statement": releves::lire(pool, sub, id).await? }))
        }
        None => match releves::par_identite(pool, sub, &releve).await? {
            Some(id) => deja(pool, sub, id).await,
            None => Ok(refus("conflict", "relevé en cours d'import, à renvoyer")),
        },
    }
}

fn nom_de(champ: &axum::extract::multipart::Field<'_>) -> String {
    champ
        .file_name()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(NOM_MAX)
        .collect()
}

pub async fn importer(
    State(state): State<AppState>,
    p: Personne,
    mut corps: Multipart,
) -> Result<Response, Response> {
    let illisible = |e: axum::extract::multipart::MultipartError| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            erreur(
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "envoi trop volumineux",
            )
        } else {
            invalide("corps multipart illisible")
        }
    };
    let mut resultats: Vec<Value> = Vec::new();
    while let Some(champ) = corps.next_field().await.map_err(illisible)? {
        if champ.name() != Some("file") {
            continue;
        }
        if resultats.len() >= FICHIERS_MAX {
            return Err(invalide("trop de fichiers dans un même envoi"));
        }
        let nom = nom_de(&champ);
        let octets = champ.bytes().await.map_err(illisible)?.to_vec();
        let mut resultat = traiter(state.pool(), p.sub(), octets).await.map_err(bd)?;
        resultat["file"] = Value::String(nom);
        resultats.push(resultat);
    }
    if resultats.is_empty() {
        return Err(invalide("aucun fichier : champ multipart « file » attendu"));
    }
    Ok(ok(&json!({ "results": resultats })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{PASSAGE_ANNEE, TOTAL_FAUX, TROIS_PAGES, pages, pdf};

    #[test]
    fn empreinte_sha256() {
        assert_eq!(
            empreinte(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn pdf_lu_comme_sa_fixture() {
        for fixture in [TROIS_PAGES, PASSAGE_ANNEE] {
            let attendu = cmb::analyser(&pages(fixture)).unwrap();
            assert_eq!(lire(&pdf(&pages(fixture), "a")).unwrap(), attendu);
        }
    }

    #[test]
    fn refus_structures() {
        let r = lire(&pdf(&pages(TOTAL_FAUX), "a")).unwrap_err();
        assert_eq!(r["status"], "rejected");
        assert_eq!(r["error"]["code"], "checks_failed");
        assert_eq!(
            r["error"]["gaps"],
            json!([{"check": "total_debits", "expected": 99_999, "read": 89_133}])
        );
        assert_eq!(
            r["error"]["unrecognized_lines"],
            json!([
                {"page": 1, "text": "13/02 13/02/2025 VIR INST vers COMPTE SANS MONTANT"},
                {"page": 1, "text": "DETAIL ORPHELIN 0001"},
            ])
        );
        assert_eq!(r["error"]["missing"], json!([]));
        assert_eq!(lire(b"bonjour").unwrap_err()["error"]["code"], "not_a_pdf");
        assert_eq!(
            lire(b"%PDF-1.4 tronqu").unwrap_err()["error"]["code"],
            "unreadable_pdf"
        );
        let autre = pages("page 1\n50 50 30 Facture\n90 50 30 fictive\n");
        assert_eq!(
            lire(&pdf(&autre, "a")).unwrap_err()["error"]["code"],
            "unknown_format"
        );
    }

    #[test]
    fn releves_reels() {
        let Ok(dossier) = std::env::var("TIRELIRE_RELEVES_REELS") else {
            return;
        };
        let mut lus = 0;
        for entree in std::fs::read_dir(&dossier).expect("TIRELIRE_RELEVES_REELS : dossier lisible")
        {
            let chemin = entree.unwrap().path();
            let est_pdf = chemin
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            if !est_pdf {
                continue;
            }
            lus += 1;
            match lire(&std::fs::read(&chemin).unwrap()) {
                Ok(r) => eprintln!(
                    "relevé réel {lus} : {} opérations, {} pages, contrôles passés",
                    r.operations.len(),
                    r.pages
                ),
                Err(refus) => {
                    let e = &refus["error"];
                    let nombre = |cle: &str| e[cle].as_array().map_or(0, Vec::len);
                    panic!(
                        "relevé réel {lus} refusé ({}) : {} élément(s) manquant(s), {} écart(s), {} ligne(s) non reconnue(s)",
                        e["code"],
                        nombre("missing"),
                        nombre("gaps"),
                        nombre("unrecognized_lines")
                    );
                }
            }
        }
        assert!(lus > 0, "TIRELIRE_RELEVES_REELS : aucun PDF");
    }
}
