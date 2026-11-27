use std::collections::HashMap;

use sqlx::{PgConnection, PgPool};

use crate::{categories, normalisation};

pub const NORMALISATION: i32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regle {
    pub id: i64,
    pub categorie: i64,
    pub mots: Vec<String>,
    pub genre: Option<String>,
    pub section: Option<String>,
}

impl Regle {
    pub fn convient(&self, genre: &str, section: &str, libelle: &str) -> bool {
        self.genre.as_deref().is_none_or(|g| g == genre)
            && self.section.as_deref().is_none_or(|s| s == section)
            && (self.mots.is_empty()
                || self
                    .mots
                    .iter()
                    .any(|m| normalisation::contient(libelle, m)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origine {
    Commercant,
    Regle(i64),
}

impl Origine {
    pub fn cle(self) -> &'static str {
        match self {
            Origine::Commercant => "commercant",
            Origine::Regle(_) => "regle",
        }
    }

    pub fn regle(self) -> Option<i64> {
        match self {
            Origine::Commercant => None,
            Origine::Regle(id) => Some(id),
        }
    }
}

pub fn choisir(
    du_commercant: Option<i64>,
    regles: &[Regle],
    genre: &str,
    section: &str,
    libelle: &str,
) -> Option<(i64, Origine)> {
    if let Some(c) = du_commercant {
        return Some((c, Origine::Commercant));
    }
    let section = normalisation::texte(section);
    let libelle = normalisation::texte(libelle);
    regles
        .iter()
        .find(|r| r.convient(genre, &section, &libelle))
        .map(|r| (r.categorie, Origine::Regle(r.id)))
}

type LigneDeRegle = (i64, i64, Vec<String>, Option<String>, Option<String>);

pub async fn regles(c: &mut PgConnection, sub: &str) -> Result<Vec<Regle>, sqlx::Error> {
    let lignes: Vec<LigneDeRegle> = sqlx::query_as(
        "SELECT id, categorie_id, mots, type, section FROM regles WHERE sub = $1 ORDER BY rang, id",
    )
    .bind(sub)
    .fetch_all(&mut *c)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|(id, categorie, mots, genre, section)| Regle {
            id,
            categorie,
            mots,
            genre,
            section,
        })
        .collect())
}

async fn normaliser(c: &mut PgConnection, sub: &str) -> Result<(), sqlx::Error> {
    let lignes: Vec<(i64, String, String, Option<String>)> =
        sqlx::query_as("SELECT id, type, libelle, commercant_brut FROM operations WHERE sub = $1")
            .bind(sub)
            .fetch_all(&mut *c)
            .await?;
    let (ids, cles): (Vec<i64>, Vec<Option<String>>) = lignes
        .iter()
        .map(|(id, genre, libelle, brut)| {
            (
                *id,
                normalisation::tiers(genre, libelle, brut.as_deref()).map(|t| t.cle),
            )
        })
        .unzip();
    sqlx::query(
        "UPDATE operations o SET commercant_cle = u.cle
         FROM UNNEST($1::bigint[], $2::text[]) AS u(id, cle)
         WHERE o.id = u.id AND o.sub = $3 AND o.commercant_cle IS DISTINCT FROM u.cle",
    )
    .bind(&ids)
    .bind(&cles)
    .bind(sub)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn rattacher(c: &mut PgConnection, sub: &str) -> Result<(), sqlx::Error> {
    let nouvelles: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.commercant_cle FROM operations o
         WHERE o.sub = $1 AND o.commercant_cle IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM variantes v WHERE v.sub = o.sub AND v.cle = o.commercant_cle)
         ORDER BY 1",
    )
    .bind(sub)
    .fetch_all(&mut *c)
    .await?;
    for cle in nouvelles {
        sqlx::query(
            "WITH nouveau AS (INSERT INTO commercants (sub, nom) VALUES ($1, $2) RETURNING id)
             INSERT INTO variantes (sub, cle, commercant_id) SELECT $1, $3, id FROM nouveau",
        )
        .bind(sub)
        .bind(normalisation::nom(&cle))
        .bind(&cle)
        .execute(&mut *c)
        .await?;
    }
    sqlx::query(
        "UPDATE operations o SET commercant_id = v.commercant_id FROM variantes v
         WHERE o.sub = $1 AND v.sub = o.sub AND v.cle = o.commercant_cle
           AND o.commercant_id IS DISTINCT FROM v.commercant_id",
    )
    .bind(sub)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE operations SET commercant_id = NULL
         WHERE sub = $1 AND commercant_cle IS NULL AND commercant_id IS NOT NULL",
    )
    .bind(sub)
    .execute(&mut *c)
    .await?;
    Ok(())
}

type Ligne = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<i64>,
);

pub async fn rejouer(c: &mut PgConnection, sub: &str) -> Result<i64, sqlx::Error> {
    rattacher(&mut *c, sub).await?;
    let regles = regles(&mut *c, sub).await?;
    let des_commercants: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id, categorie_id FROM commercants WHERE sub = $1 AND categorie_id IS NOT NULL",
    )
    .bind(sub)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .collect();
    let lignes: Vec<Ligne> = sqlx::query_as(
        "SELECT id, type, section, libelle, commercant_id, categorie_id, origine, regle_id
         FROM operations WHERE sub = $1 AND origine IS DISTINCT FROM 'operation'",
    )
    .bind(sub)
    .fetch_all(&mut *c)
    .await?;
    let mut ids = Vec::new();
    let mut categories: Vec<Option<i64>> = Vec::new();
    let mut origines: Vec<Option<&'static str>> = Vec::new();
    let mut de_regle: Vec<Option<i64>> = Vec::new();
    for (id, genre, section, libelle, commercant, categorie, origine, regle) in &lignes {
        let du_commercant = commercant.and_then(|m| des_commercants.get(&m).copied());
        let choix = choisir(du_commercant, &regles, genre, section, libelle);
        let voulu = (
            choix.map(|(c, _)| c),
            choix.map(|(_, o)| o.cle()),
            choix.and_then(|(_, o)| o.regle()),
        );
        if voulu != (*categorie, origine.as_deref(), *regle) {
            ids.push(*id);
            categories.push(voulu.0);
            origines.push(voulu.1);
            de_regle.push(voulu.2);
        }
    }
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE operations o SET categorie_id = u.categorie, origine = u.origine, regle_id = u.regle
         FROM UNNEST($1::bigint[], $2::bigint[], $3::text[], $4::bigint[]) AS u(id, categorie, origine, regle)
         WHERE o.id = u.id AND o.sub = $5",
    )
    .bind(&ids)
    .bind(&categories)
    .bind(&origines)
    .bind(&de_regle)
    .bind(sub)
    .execute(&mut *c)
    .await?;
    Ok(i64::try_from(ids.len()).unwrap_or(i64::MAX))
}

pub async fn verrouiller(c: &mut PgConnection, sub: &str) -> Result<(), sqlx::Error> {
    let nouvelle: Option<String> = sqlx::query_scalar(
        "INSERT INTO personnes (sub) VALUES ($1) ON CONFLICT (sub) DO NOTHING RETURNING sub",
    )
    .bind(sub)
    .fetch_optional(&mut *c)
    .await?;
    if nouvelle.is_some() {
        categories::creer_defauts(&mut *c, sub).await?;
    }
    let version: i32 =
        sqlx::query_scalar("SELECT normalisation FROM personnes WHERE sub = $1 FOR UPDATE")
            .bind(sub)
            .fetch_one(&mut *c)
            .await?;
    if version < NORMALISATION {
        normaliser(&mut *c, sub).await?;
        sqlx::query("UPDATE personnes SET normalisation = $2 WHERE sub = $1")
            .bind(sub)
            .bind(NORMALISATION)
            .execute(&mut *c)
            .await?;
        rejouer(&mut *c, sub).await?;
    }
    Ok(())
}

pub async fn pret(pool: &PgPool, sub: &str) -> Result<(), sqlx::Error> {
    let version: Option<i32> =
        sqlx::query_scalar("SELECT normalisation FROM personnes WHERE sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?;
    if version == Some(NORMALISATION) {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    verrouiller(&mut tx, sub).await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regle(id: i64, categorie: i64, mots: &[&str]) -> Regle {
        Regle {
            id,
            categorie,
            mots: mots.iter().map(|m| (*m).to_string()).collect(),
            genre: None,
            section: None,
        }
    }

    #[test]
    fn premiere_regle_qui_convient_dans_l_ordre() {
        let regles = [
            regle(1, 10, &["BOULANGERIE", "FOURNIL"]),
            regle(2, 20, &["PONT"]),
            regle(3, 30, &["BOULANGERIE DU PONT"]),
        ];
        let choix = |libelle: &str| choisir(None, &regles, "carte", "PAIEMENTS PAR CARTE", libelle);
        assert_eq!(
            choix("CARTE 01/02 Boulangerie du Pont"),
            Some((10, Origine::Regle(1)))
        );
        assert_eq!(
            choix("CARTE 01/02 CAFE DU PONT"),
            Some((20, Origine::Regle(2)))
        );
        assert_eq!(
            choix("CARTE 01/02 LE FOURNIL"),
            Some((10, Origine::Regle(1)))
        );
        assert_eq!(choix("CARTE 01/02 PONTON"), None);
        let inverse = [regles[2].clone(), regles[0].clone()];
        assert_eq!(
            choisir(None, &inverse, "carte", "", "BOULANGERIE DU PONT"),
            Some((30, Origine::Regle(3)))
        );
    }

    #[test]
    fn regle_du_commercant_avant_les_mots_cles() {
        let regles = [regle(1, 10, &["BOULANGERIE"])];
        assert_eq!(
            choisir(Some(99), &regles, "carte", "", "BOULANGERIE DU PONT"),
            Some((99, Origine::Commercant))
        );
    }

    #[test]
    fn conditions_de_type_et_de_section() {
        let pret = Regle {
            genre: Some("echeance_pret".into()),
            ..regle(1, 10, &[])
        };
        let frais = Regle {
            section: Some("SERVICES ET FRAIS BANCAIRES".into()),
            ..regle(2, 20, &[])
        };
        let les_deux = Regle {
            genre: Some("virement".into()),
            ..regle(3, 30, &["LIVRET"])
        };
        let regles = [pret, frais, les_deux];
        assert_eq!(
            choisir(None, &regles, "echeance_pret", "AUTRES", "ECH PRET 123"),
            Some((10, Origine::Regle(1)))
        );
        assert_eq!(
            choisir(
                None,
                &regles,
                "autre",
                "Services et frais bancaires",
                "FRAIS FICTIFS"
            ),
            Some((20, Origine::Regle(2)))
        );
        assert_eq!(
            choisir(
                None,
                &regles,
                "virement",
                "VIREMENTS EMIS",
                "VIR VERS LIVRET"
            ),
            Some((30, Origine::Regle(3)))
        );
        assert_eq!(
            choisir(
                None,
                &regles,
                "carte",
                "PAIEMENTS PAR CARTE",
                "CARTE LIVRET"
            ),
            None
        );
    }
}
