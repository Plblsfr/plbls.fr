use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::{
    classement,
    etat::AppState,
    garde::Personne,
    http::{bd, conflit, corps, cree, facultatif, identifiant, introuvable, invalide, ok},
};

pub const NATURES: [&str; 5] = ["fixe", "variable", "epargne", "revenu", "neutre"];
pub const NOM_MAX: usize = 40;
pub const CATEGORIES_MAX: i64 = 100;
pub const PLAFOND_MAX: i64 = 10_000_000_000;

pub struct CategorieParDefaut {
    pub cle: &'static str,
    pub nom: &'static str,
    pub nature: &'static str,
}

const fn categorie(
    cle: &'static str,
    nom: &'static str,
    nature: &'static str,
) -> CategorieParDefaut {
    CategorieParDefaut { cle, nom, nature }
}

pub const CATEGORIES: [CategorieParDefaut; 18] = [
    categorie("logement", "Logement", "fixe"),
    categorie("credit", "Crédit", "fixe"),
    categorie("assurances", "Assurances", "fixe"),
    categorie("telecom", "Télécom", "fixe"),
    categorie("energie", "Énergie", "fixe"),
    categorie("abonnements", "Abonnements", "fixe"),
    categorie("frais", "Frais bancaires", "fixe"),
    categorie("impots", "Impôts et taxes", "fixe"),
    categorie("courses", "Courses", "variable"),
    categorie("restaurants", "Restaurants", "variable"),
    categorie("transports", "Transports", "variable"),
    categorie("sante", "Santé", "variable"),
    categorie("loisirs", "Loisirs et sorties", "variable"),
    categorie("shopping", "Shopping", "variable"),
    categorie("divers", "Divers", "variable"),
    categorie("epargne", "Épargne", "epargne"),
    categorie("revenus", "Revenus", "revenu"),
    categorie("transferts", "Transferts neutres", "neutre"),
];

pub struct RegleParDefaut {
    pub cle: &'static str,
    pub categorie: &'static str,
    pub mots: &'static [&'static str],
    pub genre: Option<&'static str>,
    pub section: Option<&'static str>,
}

const fn mots(
    cle: &'static str,
    categorie: &'static str,
    mots: &'static [&'static str],
) -> RegleParDefaut {
    RegleParDefaut {
        cle,
        categorie,
        mots,
        genre: None,
        section: None,
    }
}

pub const REGLES: [RegleParDefaut; 15] = [
    RegleParDefaut {
        cle: "type-echeance-pret",
        categorie: "credit",
        mots: &[],
        genre: Some("echeance_pret"),
        section: None,
    },
    RegleParDefaut {
        cle: "type-cotisation",
        categorie: "frais",
        mots: &[],
        genre: Some("cotisation"),
        section: None,
    },
    RegleParDefaut {
        cle: "section-frais-bancaires",
        categorie: "frais",
        mots: &[],
        genre: None,
        section: Some("SERVICES ET FRAIS BANCAIRES"),
    },
    mots("epargne", "epargne", &["LIVRET", "EPARGNE"]),
    mots("revenus", "revenus", &["SALAIRE"]),
    mots("logement", "logement", &["LOYER"]),
    mots(
        "assurances",
        "assurances",
        &["ASSURANCE", "ASSURANCES", "MUTUELLE"],
    ),
    mots(
        "impots",
        "impots",
        &["IMPOT", "IMPOTS", "DGFIP", "TRESOR PUBLIC"],
    ),
    mots("telecom", "telecom", &["TELECOM"]),
    mots("energie", "energie", &["ELECTRICITE", "ENERGIE", "GAZ"]),
    mots(
        "sante",
        "sante",
        &[
            "PHARMACIE",
            "MEDECIN",
            "DOCTEUR",
            "DENTISTE",
            "OPTICIEN",
            "LABORATOIRE",
            "HOPITAL",
            "CLINIQUE",
        ],
    ),
    mots(
        "courses",
        "courses",
        &[
            "BOULANGERIE",
            "BOUCHERIE",
            "EPICERIE",
            "SUPERMARCHE",
            "HYPERMARCHE",
            "SUPERETTE",
            "PRIMEUR",
            "FROMAGERIE",
            "POISSONNERIE",
        ],
    ),
    mots(
        "restaurants",
        "restaurants",
        &[
            "RESTAURANT",
            "BRASSERIE",
            "PIZZERIA",
            "CREPERIE",
            "BISTROT",
            "TRAITEUR",
        ],
    ),
    mots(
        "transports",
        "transports",
        &[
            "SNCF",
            "PEAGE",
            "PARKING",
            "TAXI",
            "CARBURANT",
            "AUTOROUTE",
            "STATIONNEMENT",
        ],
    ),
    mots(
        "loisirs",
        "loisirs",
        &["CINEMA", "THEATRE", "MUSEE", "LIBRAIRIE", "PISCINE"],
    ),
];

pub async fn creer_defauts(c: &mut PgConnection, sub: &str) -> Result<(), sqlx::Error> {
    for (rang, d) in CATEGORIES.iter().enumerate() {
        let rang = i32::try_from(rang).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO categories (sub, cle, nom, nature, couleur, rang) VALUES ($1, $2, $3, $4, $5, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(sub)
        .bind(d.cle)
        .bind(d.nom)
        .bind(d.nature)
        .bind(rang)
        .execute(&mut *c)
        .await?;
    }
    for (rang, r) in REGLES.iter().enumerate() {
        let mots: Vec<&str> = r.mots.to_vec();
        sqlx::query(
            "INSERT INTO regles (sub, cle, categorie_id, mots, type, section, rang)
             SELECT $1, $2, c.id, $4, $5, $6, $7 FROM categories c WHERE c.sub = $1 AND c.cle = $3
             ON CONFLICT DO NOTHING",
        )
        .bind(sub)
        .bind(r.cle)
        .bind(r.categorie)
        .bind(&mots)
        .bind(r.genre)
        .bind(r.section)
        .bind(i32::try_from(rang).unwrap_or(i32::MAX))
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}

const CATEGORIE: &str = "jsonb_build_object(
    'id', c.id, 'key', c.cle, 'name', c.nom, 'kind', c.nature, 'monthly_cap', c.plafond,
    'color', c.couleur, 'position', c.rang, 'archived', c.archivee,
    'operations', (SELECT count(*) FROM operations o WHERE o.categorie_id = c.id),
    'merchants', (SELECT count(*) FROM commercants m WHERE m.categorie_id = c.id),
    'rules', (SELECT count(*) FROM regles r WHERE r.categorie_id = c.id))";

async fn lire(c: &mut PgConnection, sub: &str, id: i64) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {CATEGORIE} FROM categories c WHERE c.sub = $1 AND c.id = $2"
    )))
    .bind(sub)
    .bind(id)
    .fetch_optional(&mut *c)
    .await
}

pub async fn utilisable(c: &mut PgConnection, sub: &str, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM categories WHERE sub = $1 AND id = $2 AND NOT archivee)",
    )
    .bind(sub)
    .bind(id)
    .fetch_one(&mut *c)
    .await
}

pub async fn exiger(c: &mut PgConnection, sub: &str, id: i64) -> Result<(), Response> {
    if utilisable(c, sub, id).await.map_err(bd)? {
        Ok(())
    } else {
        Err(invalide("category_id : catégorie inconnue ou archivée"))
    }
}

pub async fn lister(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let categories: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {CATEGORIE} FROM categories c WHERE c.sub = $1 ORDER BY c.archivee, c.rang, c.id"
    )))
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    Ok(ok(&json!({ "categories": categories })))
}

fn nom_valide(nom: &str) -> Result<String, Response> {
    let nom = nom.split_whitespace().collect::<Vec<_>>().join(" ");
    if nom.is_empty() || nom.chars().count() > NOM_MAX || nom.chars().any(char::is_control) {
        return Err(invalide("name : 1 à 40 caractères"));
    }
    Ok(nom)
}

fn nature_valide(nature: &str) -> Result<&str, Response> {
    NATURES
        .contains(&nature)
        .then_some(nature)
        .ok_or_else(|| invalide("kind : fixe, variable, epargne, revenu ou neutre"))
}

fn plafond_valide(plafond: Option<i64>) -> Result<Option<i64>, Response> {
    match plafond {
        Some(p) if !(0..=PLAFOND_MAX).contains(&p) => {
            Err(invalide("monthly_cap : centimes, entier positif"))
        }
        p => Ok(p),
    }
}

fn doublon(e: sqlx::Error) -> Response {
    match e.as_database_error() {
        Some(d) if d.is_unique_violation() => conflit("une catégorie porte déjà ce nom"),
        _ => bd(e),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nouvelle {
    name: String,
    kind: String,
    #[serde(default)]
    monthly_cap: Option<i64>,
}

pub async fn creer(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let n: Nouvelle = corps(&octets, "corps attendu : {name, kind, monthly_cap?}")?;
    let nom = nom_valide(&n.name)?;
    let nature = nature_valide(&n.kind)?;
    let plafond = plafond_valide(n.monthly_cap)?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let nombre: i64 = sqlx::query_scalar("SELECT count(*) FROM categories WHERE sub = $1")
        .bind(p.sub())
        .fetch_one(&mut *tx)
        .await
        .map_err(bd)?;
    if nombre >= CATEGORIES_MAX {
        return Err(invalide("trop de catégories"));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO categories (sub, nom, nature, plafond, couleur, rang)
         SELECT $1, $2, $3, $4, COALESCE(max(couleur), -1) + 1, COALESCE(max(rang), -1) + 1
         FROM categories WHERE sub = $1
         RETURNING id",
    )
    .bind(p.sub())
    .bind(&nom)
    .bind(nature)
    .bind(plafond)
    .fetch_one(&mut *tx)
    .await
    .map_err(doublon)?;
    let categorie = lire(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(cree(&json!({ "category": categorie })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Modification {
    name: Option<String>,
    kind: Option<String>,
    #[serde(default, deserialize_with = "facultatif")]
    monthly_cap: Option<Option<i64>>,
    archived: Option<bool>,
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let m: Modification = corps(
        &octets,
        "corps attendu : {name?, kind?, monthly_cap?, archived?}",
    )?;
    let nom = m.name.as_deref().map(nom_valide).transpose()?;
    let nature = m.kind.as_deref().map(nature_valide).transpose()?;
    let plafond = m.monthly_cap.map(plafond_valide).transpose()?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    classement::verrouiller(&mut tx, p.sub())
        .await
        .map_err(bd)?;
    let trouvee = sqlx::query(
        "UPDATE categories SET
            nom = COALESCE($3, nom), nature = COALESCE($4, nature),
            plafond = CASE WHEN $5 THEN $6 ELSE plafond END,
            archivee = COALESCE($7, archivee)
         WHERE sub = $1 AND id = $2",
    )
    .bind(p.sub())
    .bind(id)
    .bind(nom)
    .bind(nature)
    .bind(plafond.is_some())
    .bind(plafond.flatten())
    .bind(m.archived)
    .execute(&mut *tx)
    .await
    .map_err(doublon)?
    .rows_affected();
    if trouvee == 0 {
        return Err(introuvable());
    }
    let categorie = lire(&mut tx, p.sub(), id).await.map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&json!({ "category": categorie })))
}
