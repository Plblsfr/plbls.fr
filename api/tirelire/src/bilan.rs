use std::collections::BTreeMap;

use chrono::NaiveDate;
use sqlx::PgPool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nature {
    Fixe,
    Variable,
    Epargne,
    Revenu,
    Neutre,
}

impl Nature {
    pub fn lire(cle: &str) -> Option<Nature> {
        Some(match cle {
            "fixe" => Nature::Fixe,
            "variable" => Nature::Variable,
            "epargne" => Nature::Epargne,
            "revenu" => Nature::Revenu,
            "neutre" => Nature::Neutre,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ligne {
    pub categorie: Option<(i64, Nature)>,
    pub credit: bool,
    pub montant: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub par_categorie: BTreeMap<i64, i64>,
    pub depenses_fixes: i64,
    pub depenses_variables: i64,
    pub epargne: i64,
    pub revenus: i64,
    pub neutre: i64,
    pub a_classer_debits: i64,
    pub a_classer_credits: i64,
}

impl Bilan {
    pub fn depenses(&self) -> i64 {
        self.depenses_fixes + self.depenses_variables
    }
}

pub fn bilan(lignes: &[Ligne]) -> Bilan {
    let mut b = Bilan::default();
    for l in lignes {
        let sortie = if l.credit { -l.montant } else { l.montant };
        let Some((id, nature)) = l.categorie else {
            if l.credit {
                b.a_classer_credits += l.montant;
            } else {
                b.a_classer_debits += l.montant;
            }
            continue;
        };
        let (total, valeur) = match nature {
            Nature::Fixe => (&mut b.depenses_fixes, sortie),
            Nature::Variable => (&mut b.depenses_variables, sortie),
            Nature::Epargne => (&mut b.epargne, sortie),
            Nature::Revenu => (&mut b.revenus, -sortie),
            Nature::Neutre => (&mut b.neutre, -sortie),
        };
        *total += valeur;
        *b.par_categorie.entry(id).or_default() += valeur;
    }
    b
}

pub async fn calculer(
    pool: &PgPool,
    sub: &str,
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Bilan, sqlx::Error> {
    let lignes: Vec<(Option<i64>, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT o.categorie_id, c.nature, o.sens, o.montant
         FROM operations o LEFT JOIN categories c ON c.id = o.categorie_id
         WHERE o.sub = $1 AND o.date_operation BETWEEN $2 AND $3",
    )
    .bind(sub)
    .bind(du)
    .bind(au)
    .fetch_all(pool)
    .await?;
    let lignes: Vec<Ligne> = lignes
        .into_iter()
        .map(|(categorie, nature, sens, montant)| Ligne {
            categorie: categorie.zip(nature.as_deref().and_then(Nature::lire)),
            credit: sens == "credit",
            montant,
        })
        .collect();
    Ok(bilan(&lignes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ligne(categorie: Option<(i64, Nature)>, credit: bool, montant: i64) -> Ligne {
        Ligne {
            categorie,
            credit,
            montant,
        }
    }

    #[test]
    fn remboursement_deduit_de_sa_categorie() {
        let courses = Some((1, Nature::Variable));
        let b = bilan(&[
            ligne(courses, false, 5000),
            ligne(courses, false, 1200),
            ligne(courses, true, 1200),
        ]);
        assert_eq!(b.par_categorie, BTreeMap::from([(1, 5000)]));
        assert_eq!(
            (b.depenses_variables, b.depenses(), b.revenus),
            (5000, 5000, 0)
        );
    }

    #[test]
    fn epargne_revenus_et_transferts_hors_depenses() {
        let b = bilan(&[
            ligne(Some((1, Nature::Fixe)), false, 70_000),
            ligne(Some((2, Nature::Variable)), false, 3000),
            ligne(Some((3, Nature::Epargne)), false, 25_000),
            ligne(Some((3, Nature::Epargne)), true, 5000),
            ligne(Some((4, Nature::Revenu)), true, 200_000),
            ligne(Some((4, Nature::Revenu)), false, 1000),
            ligne(Some((5, Nature::Neutre)), false, 10_000),
            ligne(Some((5, Nature::Neutre)), true, 10_000),
            ligne(None, false, 900),
            ligne(None, true, 400),
        ]);
        assert_eq!(
            (b.depenses_fixes, b.depenses_variables, b.depenses()),
            (70_000, 3000, 73_000)
        );
        assert_eq!((b.epargne, b.revenus, b.neutre), (20_000, 199_000, 0));
        assert_eq!((b.a_classer_debits, b.a_classer_credits), (900, 400));
        assert_eq!(
            b.par_categorie,
            BTreeMap::from([(1, 70_000), (2, 3000), (3, 20_000), (4, 199_000), (5, 0)])
        );
    }
}
