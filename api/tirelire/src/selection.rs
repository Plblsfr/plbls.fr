use axum::response::Response;
use chrono::NaiveDate;
use serde::Deserialize;
use sqlx::PgPool;

use crate::{cmb::Genre, http::invalide};

pub const RECHERCHE_MAX: usize = 100;
pub const MOTS_MAX: usize = 8;

const ACCENTS: &str = "àáâãäåçèéêëìíîïñòóôõöùúûüýÿÀÁÂÃÄÅÇÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝŸ";
const SANS_ACCENT: &str = "aaaaaaceeeeiiiinooooouuuuyyaaaaaaceeeeiiiinooooouuuuyy";

fn plat(expression: &str) -> String {
    format!(
        "replace(replace(replace(replace(translate(lower({expression}), '{ACCENTS}', '{SANS_ACCENT}'), 'œ', 'oe'), 'Œ', 'oe'), 'æ', 'ae'), 'Æ', 'ae')"
    )
}

pub static OU: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let texte = plat(
        "o.libelle || E'\\n' || array_to_string(o.details, E'\\n') || E'\\n' || coalesce(o.commercant_brut, '') || E'\\n' || coalesce(m.nom, '')",
    );
    let mot = plat("mot");
    format!(
        "o.sub = $1 AND ($2::bigint IS NULL OR o.releve_id = $2)
        AND ($3::date IS NULL OR o.date_operation >= $3) AND ($4::date IS NULL OR o.date_operation <= $4)
        AND ($5::bigint IS NULL OR o.categorie_id = $5) AND ($6::bigint IS NULL OR o.commercant_id = $6)
        AND (NOT $7 OR o.categorie_id IS NULL)
        AND ($8::text[] IS NULL OR {texte} LIKE ALL (SELECT {mot} FROM unnest($8::text[]) AS mot))
        AND ($9::bigint IS NULL OR o.montant >= $9) AND ($10::bigint IS NULL OR o.montant <= $10)
        AND ($11::text IS NULL OR o.sens = $11) AND ($12::text IS NULL OR o.type = $12)"
    )
});

pub const ORDRE: &str = "o.date_operation DESC, o.releve_id DESC, o.rang DESC";

#[derive(Debug, Default, Deserialize)]
pub struct Filtre {
    pub statement: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub category: Option<String>,
    pub merchant: Option<String>,
    pub unclassified: Option<String>,
    pub q: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
    pub direction: Option<String>,
    #[serde(rename = "type")]
    pub genre: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub releve: Option<i64>,
    pub du: Option<NaiveDate>,
    pub au: Option<NaiveDate>,
    pub categorie: Option<i64>,
    pub commercant: Option<i64>,
    pub a_classer: bool,
    pub mots: Option<Vec<String>>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub sens: Option<&'static str>,
    pub genre: Option<&'static str>,
}

pub struct Resume {
    pub total: i64,
    pub debits: i64,
    pub credits: i64,
    pub premiere: Option<NaiveDate>,
    pub derniere: Option<NaiveDate>,
}

macro_rules! lier {
    ($requete:expr, $sub:expr, $selection:expr) => {
        $requete
            .bind($sub)
            .bind($selection.releve)
            .bind($selection.du)
            .bind($selection.au)
            .bind($selection.categorie)
            .bind($selection.commercant)
            .bind($selection.a_classer)
            .bind($selection.mots.as_deref())
            .bind($selection.min)
            .bind($selection.max)
            .bind($selection.sens)
            .bind($selection.genre)
    };
}
pub(crate) use lier;

pub fn entier(valeur: &Option<String>, message: &str) -> Result<Option<i64>, Response> {
    valeur
        .as_deref()
        .map(|v| v.parse::<i64>().ok().filter(|n| *n >= 0))
        .map(|n| n.ok_or_else(|| invalide(message)))
        .transpose()
}

fn jour(valeur: &Option<String>, message: &str) -> Result<Option<NaiveDate>, Response> {
    valeur
        .as_deref()
        .map(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| invalide(message)))
        .transpose()
}

fn booleen(valeur: &Option<String>, message: &str) -> Result<bool, Response> {
    match valeur.as_deref() {
        None | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(_) => Err(invalide(message)),
    }
}

pub fn mots(q: Option<&str>) -> Result<Option<Vec<String>>, Response> {
    let Some(q) = q else { return Ok(None) };
    if q.chars().count() > RECHERCHE_MAX || q.chars().any(|c| c == '\0') {
        return Err(invalide("q : 100 caractères au plus"));
    }
    let mots: Vec<String> = q.split_whitespace().map(commun::sql::contient).collect();
    if mots.len() > MOTS_MAX {
        return Err(invalide("q : 8 mots au plus"));
    }
    Ok((!mots.is_empty()).then_some(mots))
}

impl Selection {
    pub fn lire(f: &Filtre) -> Result<Self, Response> {
        let min = entier(&f.min, "min : montant en centimes, entier positif")?;
        let max = entier(&f.max, "max : montant en centimes, entier positif")?;
        if min.zip(max).is_some_and(|(min, max)| min > max) {
            return Err(invalide("min : supérieur à max"));
        }
        let du = jour(&f.from, "from : date AAAA-MM-JJ")?;
        let au = jour(&f.to, "to : date AAAA-MM-JJ")?;
        let sens = match f.direction.as_deref() {
            None => None,
            Some("debit") => Some("debit"),
            Some("credit") => Some("credit"),
            Some(_) => return Err(invalide("direction : debit ou credit")),
        };
        let genre = f
            .genre
            .as_deref()
            .map(|g| {
                Genre::TOUS
                    .iter()
                    .map(|t| t.cle())
                    .find(|cle| *cle == g)
                    .ok_or_else(|| invalide("type : type d'opération inconnu"))
            })
            .transpose()?;
        Ok(Self {
            releve: entier(&f.statement, "statement : identifiant d'un relevé")?,
            du,
            au,
            categorie: entier(&f.category, "category : identifiant d'une catégorie")?,
            commercant: entier(&f.merchant, "merchant : identifiant d'un commerçant")?,
            a_classer: booleen(&f.unclassified, "unclassified : true ou false")?,
            mots: mots(f.q.as_deref())?,
            min,
            max,
            sens,
            genre,
        })
    }

    pub async fn resumer(&self, pool: &PgPool, sub: &str) -> Result<Resume, sqlx::Error> {
        let (total, debits, credits, premiere, derniere) = lier!(
            sqlx::query_as::<_, (i64, i64, i64, Option<NaiveDate>, Option<NaiveDate>)>(
                sqlx::AssertSqlSafe(format!(
                    "SELECT count(*),
                        coalesce(sum(o.montant) FILTER (WHERE o.sens = 'debit'), 0)::bigint,
                        coalesce(sum(o.montant) FILTER (WHERE o.sens = 'credit'), 0)::bigint,
                        min(o.date_operation), max(o.date_operation)
                     FROM {} WHERE {}",
                    crate::releves::OPERATIONS,
                    *OU
                ))
            ),
            sub,
            self
        )
        .fetch_one(pool)
        .await?;
        Ok(Resume {
            total,
            debits,
            credits,
            premiere,
            derniere,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_d_accents_de_meme_longueur() {
        assert_eq!(ACCENTS.chars().count(), SANS_ACCENT.chars().count());
        assert!(SANS_ACCENT.is_ascii());
    }

    #[test]
    fn mots_de_la_recherche_echappes() {
        assert_eq!(mots(None).unwrap(), None);
        assert_eq!(mots(Some("  \t ")).unwrap(), None);
        assert_eq!(
            mots(Some(r" 100%  a_b \ ")).unwrap(),
            Some(vec![
                r"%100\%%".to_string(),
                r"%a\_b%".to_string(),
                r"%\\%".to_string()
            ])
        );
        assert!(mots(Some(&"a".repeat(RECHERCHE_MAX + 1))).is_err());
        assert!(mots(Some("a b c d e f g h i")).is_err());
        assert!(mots(Some(&"é".repeat(RECHERCHE_MAX))).is_ok());
    }

    #[test]
    fn filtres_valides_et_refuses() {
        let f = |modifie: fn(&mut Filtre)| {
            let mut f = Filtre::default();
            modifie(&mut f);
            Selection::lire(&f)
        };
        assert_eq!(f(|_| {}).unwrap(), Selection::default());
        let s = f(|f| {
            f.min = Some("100".into());
            f.max = Some("100".into());
            f.direction = Some("credit".into());
            f.genre = Some("virement_instantane".into());
        })
        .unwrap();
        assert_eq!(
            (s.min, s.max, s.sens, s.genre),
            (
                Some(100),
                Some(100),
                Some("credit"),
                Some("virement_instantane")
            )
        );
        assert!(f(|f| f.min = Some("-1".into())).is_err());
        assert!(f(|f| f.max = Some("1,5".into())).is_err());
        assert!(
            f(|f| {
                f.min = Some("2".into());
                f.max = Some("1".into());
            })
            .is_err()
        );
        assert!(f(|f| f.direction = Some("Debit".into())).is_err());
        assert!(f(|f| f.genre = Some("cheque".into())).is_err());
    }
}
