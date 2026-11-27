use std::collections::BTreeMap;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::Response,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    abonnements::etats,
    bilan::Nature,
    classement,
    decision::{self, Reglage},
    etat::AppState,
    evolution::{self, Fenetre, Mouvement, ReleveSolde},
    garde::Personne,
    habitudes,
    http::{bd, corps, identifiant, introuvable, invalide, ok},
    recurrents::recurrents,
    releves::{OPERATION, OPERATIONS},
    stats::{Calcul, Categorie, Donnees, Mode, Operation, Releve},
};

#[derive(Debug, Default, Deserialize)]
pub struct Demande {
    pub mode: Option<String>,
    pub period: Option<String>,
    pub window: Option<String>,
}

type LigneOperation = (
    i64,
    i64,
    NaiveDate,
    Option<NaiveDate>,
    Option<i64>,
    String,
    i64,
    Option<i64>,
    bool,
);

type LigneCategorie = (i64, Option<String>, String, String, Option<i64>, i32, bool);

pub async fn charger(pool: &PgPool, sub: &str) -> Result<Donnees, sqlx::Error> {
    let releves: Vec<(i64, NaiveDate, NaiveDate)> =
        sqlx::query_as("SELECT id, debut, date_arrete FROM releves WHERE sub = $1 ORDER BY id")
            .bind(sub)
            .fetch_all(pool)
            .await?;
    let operations: Vec<LigneOperation> = sqlx::query_as(
        "SELECT id, releve_id, date_operation, date_achat, categorie_id, sens, montant, commercant_id,
                type IN ('prelevement', 'echeance_pret', 'cotisation')
         FROM operations WHERE sub = $1 ORDER BY id",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    let categories: Vec<LigneCategorie> = sqlx::query_as(
        "SELECT id, cle, nom, nature, plafond, couleur, archivee FROM categories WHERE sub = $1 ORDER BY rang, id",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    let commercants: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, nom FROM commercants WHERE sub = $1")
            .bind(sub)
            .fetch_all(pool)
            .await?;
    Ok(Donnees {
        releves: releves
            .into_iter()
            .map(|(id, debut, fin)| Releve { id, debut, fin })
            .collect(),
        operations: operations
            .into_iter()
            .map(
                |(id, releve, jour, achat, categorie, sens, montant, commercant, preleve)| {
                    Operation {
                        id,
                        releve,
                        jour,
                        achat,
                        categorie,
                        credit: sens == "credit",
                        montant,
                        commercant,
                        preleve,
                    }
                },
            )
            .collect(),
        categories: categories
            .into_iter()
            .filter_map(|(id, cle, nom, nature, plafond, couleur, archivee)| {
                Some(Categorie {
                    id,
                    cle,
                    nom,
                    nature: Nature::lire(&nature)?,
                    plafond,
                    couleur,
                    archivee,
                })
            })
            .collect(),
        commercants: commercants.into_iter().collect::<BTreeMap<_, _>>(),
    })
}

fn mode(d: &Demande) -> Result<Mode, Response> {
    Mode::lire(d.mode.as_deref()).ok_or_else(|| invalide("mode : calendar ou statement"))
}

fn cle(d: &Demande, mode: Mode) -> Result<Option<&str>, Response> {
    let Some(cle) = d.period.as_deref() else {
        return Ok(None);
    };
    let valide = match mode {
        Mode::Calendaire => NaiveDate::parse_from_str(&format!("{cle}-01"), "%Y-%m-%d").is_ok(),
        Mode::Releve => NaiveDate::parse_from_str(cle, "%Y-%m-%d").is_ok(),
    };
    if valide && cle.is_ascii() {
        Ok(Some(cle))
    } else {
        Err(invalide(
            "period : AAAA-MM en mode calendar, date d'arrêté AAAA-MM-JJ en mode statement",
        ))
    }
}

pub async fn lister(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    Ok(ok(
        &json!({ "mode": mode.cle(), "periods": calcul.liste() }),
    ))
}

pub async fn lire(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    Ok(ok(&calcul.mois(i)))
}

pub async fn categorie(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    let mut detail = calcul.detail(i, id).ok_or_else(introuvable)?;
    let lues: Vec<(i64, Value)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT o.id, {OPERATION} FROM {OPERATIONS} WHERE o.sub = $1 AND o.id = ANY($2)"
    )))
    .bind(p.sub())
    .bind(&detail.dernieres)
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mut par_id: BTreeMap<i64, Value> = lues.into_iter().collect();
    detail.operations = detail
        .dernieres
        .iter()
        .filter_map(|id| par_id.remove(id))
        .collect();
    Ok(ok(&detail))
}

fn fenetre(d: &Demande) -> Result<Fenetre, Response> {
    Fenetre::lire(d.window.as_deref()).ok_or_else(|| invalide("window : 6, 12, 24 ou all"))
}

pub async fn historique(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let fenetre = fenetre(&d)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    Ok(ok(&evolution::historique(&calcul, fenetre)))
}

pub async fn soldes(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let fenetre = fenetre(&d)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let periodes = crate::stats::periodes(mode, &donnees);
    let plage = fenetre.plage(periodes.len());
    let bornes = periodes
        .get(plage.start)
        .zip(periodes.last())
        .map(|(a, b)| (a.from, b.known_through));
    let Some((du, au)) = bornes else {
        return Ok(ok(&json!({
            "mode": mode.cle(), "window": fenetre.cle(), "from": null, "to": null, "accounts": []
        })));
    };
    let releves: Vec<(i64, i64, NaiveDate, NaiveDate, i64, i64)> = sqlx::query_as(
        "SELECT id, compte_id, debut, date_arrete, ancien_solde, nouveau_solde
         FROM releves WHERE sub = $1 ORDER BY id",
    )
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let mouvements: Vec<(i64, NaiveDate, String, i64)> = sqlx::query_as(
        "SELECT releve_id, date_operation, sens, montant FROM operations WHERE sub = $1 ORDER BY id",
    )
    .bind(p.sub())
    .fetch_all(state.pool())
    .await
    .map_err(bd)?;
    let releves: Vec<ReleveSolde> = releves
        .into_iter()
        .map(|(id, compte, debut, fin, ancien, nouveau)| ReleveSolde {
            id,
            compte,
            debut,
            fin,
            ancien,
            nouveau,
        })
        .collect();
    let mouvements: Vec<Mouvement> = mouvements
        .into_iter()
        .map(|(releve, jour, sens, montant)| Mouvement {
            releve,
            jour,
            credit: sens == "credit",
            montant,
        })
        .collect();
    Ok(ok(&json!({
        "mode": mode.cle(),
        "window": fenetre.cle(),
        "from": du,
        "to": au,
        "accounts": evolution::soldes(&releves, &mouvements, du, au),
    })))
}

pub async fn annees(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    Ok(ok(&evolution::annees(&calcul, i)))
}

pub async fn habitudes(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let fenetre = habitudes::fenetre(d.window.as_deref())
        .ok_or_else(|| invalide("window : 1, 3, 6, 12 ou all"))?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    Ok(ok(&habitudes::habitudes(&calcul, fenetre)))
}

pub async fn inhabituelle(
    State(state): State<AppState>,
    p: Personne,
    Path(id): Path<String>,
) -> Result<Response, Response> {
    let id = identifiant(&id)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    if !donnees.operations.iter().any(|o| o.id == id) {
        return Err(introuvable());
    }
    let calcul = Calcul::nouveau(&donnees, Mode::Calendaire);
    Ok(ok(
        &json!({ "unusual": habitudes::inhabituelles(&calcul).remove(&id) }),
    ))
}

pub async fn resume(
    State(state): State<AppState>,
    p: Personne,
    Query(d): Query<Demande>,
) -> Result<Response, Response> {
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let etats = etats(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    Ok(ok(&decision::resumer(
        &calcul,
        i,
        &recurrents(&donnees, &etats),
    )))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hypothese {
    mode: Option<String>,
    period: Option<String>,
    #[serde(default)]
    categories: Vec<Reglage>,
    #[serde(default)]
    subscriptions: Vec<String>,
}

pub async fn simuler(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let h: Hypothese = corps(
        &octets,
        "corps attendu : {mode?, period?, categories?: [{id, percent} ou {id, target}], subscriptions?: [id]}",
    )?;
    let d = Demande {
        mode: h.mode,
        period: h.period,
        window: None,
    };
    let mode = mode(&d)?;
    let cle = cle(&d, mode)?;
    classement::pret(state.pool(), p.sub()).await.map_err(bd)?;
    let donnees = charger(state.pool(), p.sub()).await.map_err(bd)?;
    let etats = etats(state.pool(), p.sub()).await.map_err(bd)?;
    let calcul = Calcul::nouveau(&donnees, mode);
    let i = calcul.index(cle).ok_or_else(introuvable)?;
    decision::simuler(
        &calcul,
        i,
        &recurrents(&donnees, &etats),
        &h.categories,
        &h.subscriptions,
    )
    .map(|s| ok(&s))
    .map_err(invalide)
}
