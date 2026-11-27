use std::fmt::Write;

use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use chrono::NaiveDate;
use futures_util::{StreamExt, stream};
use tokio::sync::mpsc;

use crate::{
    classement,
    etat::AppState,
    garde::Personne,
    http::bd,
    selection::{Filtre, ORDRE, OU, Resume, Selection, lier},
};

pub const BOM: &str = "\u{feff}";
pub const ENTETE: &str = "Date;Date de valeur;Date d'achat;Libellé;Détails;Commerçant;Catégorie;Nature;Type;Section;Débit;Crédit;Montant;Relevé\r\n";
const TAMPON: usize = 16 * 1024;

#[derive(Debug, sqlx::FromRow)]
pub struct Ligne {
    pub date_operation: NaiveDate,
    pub date_valeur: NaiveDate,
    pub date_achat: Option<NaiveDate>,
    pub libelle: String,
    pub details: Vec<String>,
    pub commercant: Option<String>,
    pub categorie: Option<String>,
    pub nature: Option<String>,
    pub genre: String,
    pub section: String,
    pub montant: i64,
    pub sens: String,
    pub annee: i32,
    pub numero: i32,
}

pub fn cellule(texte: &str) -> String {
    let texte: String = texte
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let risque = texte
        .trim_start_matches([' ', '\u{a0}'])
        .starts_with(['=', '+', '-', '@', '\t', '\r', '\n']);
    let texte = if risque { format!("'{texte}") } else { texte };
    if texte.contains([';', '"', '\n', '\r', '\t']) || texte.trim() != texte {
        format!("\"{}\"", texte.replace('"', "\"\""))
    } else {
        texte
    }
}

pub fn montant(centimes: i64) -> String {
    let signe = if centimes < 0 { "-" } else { "" };
    let abs = centimes.unsigned_abs();
    format!("{signe}{},{:02}", abs / 100, abs % 100)
}

fn jour(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

fn nature(cle: &str) -> &str {
    match cle {
        "fixe" => "Dépense fixe",
        "variable" => "Dépense variable",
        "epargne" => "Épargne",
        "revenu" => "Revenu",
        "neutre" => "Transfert neutre",
        autre => autre,
    }
}

fn genre(cle: &str) -> &str {
    match cle {
        "carte" => "Carte",
        "prelevement" => "Prélèvement",
        "virement" => "Virement",
        "virement_instantane" => "Virement instantané",
        "echeance_pret" => "Échéance de prêt",
        "cotisation" => "Cotisation",
        "autre" => "Autre opération",
        autre => autre,
    }
}

pub fn ecrire(sortie: &mut String, l: &Ligne) {
    let debit = l.sens == "debit";
    let _ = write!(
        sortie,
        "{};{};{};{};{};{};{};{};{};{};{};{};{};{}\r\n",
        jour(l.date_operation),
        jour(l.date_valeur),
        l.date_achat.map(jour).unwrap_or_default(),
        cellule(&l.libelle),
        cellule(&l.details.join(" | ")),
        cellule(l.commercant.as_deref().unwrap_or_default()),
        cellule(l.categorie.as_deref().unwrap_or_default()),
        cellule(l.nature.as_deref().map(nature).unwrap_or_default()),
        cellule(genre(&l.genre)),
        cellule(&l.section),
        if debit {
            montant(l.montant)
        } else {
            String::new()
        },
        if debit {
            String::new()
        } else {
            montant(l.montant)
        },
        montant(if debit { -l.montant } else { l.montant }),
        cellule(&format!("Relevé n° {} de {}", l.numero, l.annee)),
    );
}

pub fn nom_de_fichier(s: &Selection, r: &Resume) -> String {
    match (s.du.or(r.premiere), s.au.or(r.derniere)) {
        (Some(du), Some(au)) => format!("tirelire-operations-{du}_{au}.csv"),
        (Some(du), None) => format!("tirelire-operations-depuis-{du}.csv"),
        (None, Some(au)) => format!("tirelire-operations-jusqu-au-{au}.csv"),
        (None, None) => "tirelire-operations.csv".to_string(),
    }
}

pub async fn exporter(
    State(state): State<AppState>,
    p: Personne,
    Query(f): Query<Filtre>,
) -> Result<Response, Response> {
    let selection = Selection::lire(&f)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let resume = selection.resumer(state.pool(), p.sub()).await.map_err(bd)?;
    let nom = nom_de_fichier(&selection, &resume);
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    let pool = state.pool().clone();
    let sub = p.sub().to_string();
    tokio::spawn(async move {
        let sql = format!(
            "SELECT o.date_operation, o.date_valeur, o.date_achat, o.libelle, o.details,
                coalesce(m.nom, o.commercant_brut) AS commercant, c.nom AS categorie, c.nature,
                o.type AS genre, o.section, o.montant, o.sens, r.annee, r.numero
             FROM operations o
             LEFT JOIN commercants m ON m.id = o.commercant_id
             LEFT JOIN categories c ON c.id = o.categorie_id
             JOIN releves r ON r.id = o.releve_id
             WHERE {} ORDER BY {ORDRE}",
            *OU
        );
        let mut lignes = lier!(
            sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(sql)),
            &sub,
            &selection
        )
        .fetch(&pool);
        let mut tampon = format!("{BOM}{ENTETE}");
        while let Some(ligne) = lignes.next().await {
            match ligne {
                Ok(l) => ecrire(&mut tampon, &l),
                Err(e) => {
                    tracing::warn!("export : {e}");
                    let _ = tx
                        .send(Err(std::io::Error::other("export interrompu")))
                        .await;
                    return;
                }
            }
            if tampon.len() >= TAMPON
                && tx
                    .send(Ok(Bytes::from(std::mem::take(&mut tampon))))
                    .await
                    .is_err()
            {
                return;
            }
        }
        let _ = tx.send(Ok(Bytes::from(tampon))).await;
    });
    let corps = Body::from_stream(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|morceau| (morceau, rx))
    }));
    let mut r = corps.into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{nom}\""))
            .map_err(|_| crate::http::interne())?,
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cellules_protegees_contre_les_formules() {
        assert_eq!(cellule("CARTE 01/02 EPICERIE"), "CARTE 01/02 EPICERIE");
        assert_eq!(cellule("=SOMME(A1:A9)"), "'=SOMME(A1:A9)");
        assert_eq!(cellule("+33 1 00 00 00 00"), "'+33 1 00 00 00 00");
        assert_eq!(cellule("-10 % EXEMPLE"), "'-10 % EXEMPLE");
        assert_eq!(cellule("@EXEMPLE"), "'@EXEMPLE");
        assert_eq!(cellule("\t=1+1"), "\"'\t=1+1\"");
        assert_eq!(cellule("\r=1+1"), "\"'\r=1+1\"");
        assert_eq!(cellule("  =1+1"), "'  =1+1");
        assert_eq!(cellule("=A1;B1"), "\"'=A1;B1\"");
        assert_eq!(cellule("A-B=C"), "A-B=C");
    }

    #[test]
    fn cellules_entre_guillemets() {
        assert_eq!(cellule("a;b"), "\"a;b\"");
        assert_eq!(cellule("dit \"oui\""), "\"dit \"\"oui\"\"\"");
        assert_eq!(cellule("ligne\nsuite"), "\"ligne\nsuite\"");
        assert_eq!(cellule(" bord "), "\" bord \"");
        assert_eq!(cellule("nul\0cloche\u{7}"), "nulcloche");
        assert_eq!(cellule(""), "");
    }

    #[test]
    fn montants_a_virgule() {
        assert_eq!(montant(0), "0,00");
        assert_eq!(montant(5), "0,05");
        assert_eq!(montant(-5), "-0,05");
        assert_eq!(montant(123_456_789), "1234567,89");
        assert_eq!(montant(-264_000), "-2640,00");
    }

    #[test]
    fn nom_de_fichier_avec_la_periode() {
        let j = |a, m, j| NaiveDate::from_ymd_opt(a, m, j);
        let resume = |premiere, derniere| Resume {
            total: 0,
            debits: 0,
            credits: 0,
            premiere,
            derniere,
        };
        let vide = Selection::default();
        assert_eq!(
            nom_de_fichier(&vide, &resume(None, None)),
            "tirelire-operations.csv"
        );
        assert_eq!(
            nom_de_fichier(&vide, &resume(j(2026, 1, 3), j(2026, 3, 28))),
            "tirelire-operations-2026-01-03_2026-03-28.csv"
        );
        let filtre = Selection {
            du: j(2026, 2, 1),
            au: j(2026, 2, 28),
            ..Selection::default()
        };
        assert_eq!(
            nom_de_fichier(&filtre, &resume(j(2026, 2, 4), j(2026, 2, 20))),
            "tirelire-operations-2026-02-01_2026-02-28.csv"
        );
        let depuis = Selection {
            du: j(2026, 2, 1),
            ..Selection::default()
        };
        assert_eq!(
            nom_de_fichier(&depuis, &resume(None, None)),
            "tirelire-operations-depuis-2026-02-01.csv"
        );
    }
}
