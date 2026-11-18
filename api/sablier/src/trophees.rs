use std::collections::{HashMap, HashSet};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgExecutor, PgPool};

use crate::{
    catalogue::{self, Domaine, Stat, Unite},
    droits::Droits,
    etat::AppState,
    exploits::{self, Exploit},
    garde::Personne,
    http::{self, bd, corps, erreur, introuvable, invalide, ok, sans_contenu},
    periode::Periode,
    recaps,
};

pub const PALIERS: &str = "paliers";
pub const EXPLOIT: &str = "exploit";
pub const PALIERS_MAX: usize = 6;

pub const GRAINS: [(Periode, &str); 4] = [
    (Periode::Jour, "jour"),
    (Periode::Semaine, "semaine"),
    (Periode::Mois, "mois"),
    (Periode::Tout, "total"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct Trophee {
    pub id: i64,
    pub cle: String,
    pub nom: String,
    pub description: String,
    pub icone: String,
    pub cache: bool,
    pub actif: bool,
    pub genre: String,
    pub stat: Option<String>,
    pub periode: Option<String>,
    pub seuils: Vec<i64>,
    pub exploit: Option<String>,
    pub parametre: Option<i64>,
    pub ordre: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Regle {
    Paliers {
        stat: &'static Stat,
        grain: Periode,
        seuils: Vec<i64>,
    },
    Exploit {
        exploit: &'static Exploit,
        parametre: Option<i64>,
    },
}

impl Regle {
    pub fn domaine(&self) -> Domaine {
        match self {
            Regle::Paliers { stat, .. } => stat.domaine,
            Regle::Exploit { exploit, .. } => exploit.domaine,
        }
    }

    pub fn unite(&self) -> Unite {
        match self {
            Regle::Paliers { stat, .. } => stat.unite,
            Regle::Exploit { exploit, .. } => exploit.unite,
        }
    }

    pub fn paliers(&self) -> usize {
        match self {
            Regle::Paliers { seuils, .. } => seuils.len(),
            Regle::Exploit { .. } => 1,
        }
    }
}

impl Trophee {
    pub fn regle(&self) -> Option<Regle> {
        match self.genre.as_str() {
            PALIERS => {
                let periode = self.periode.as_deref()?;
                Some(Regle::Paliers {
                    stat: catalogue::trouver(self.stat.as_deref()?)?,
                    grain: GRAINS.iter().find(|(_, nom)| *nom == periode)?.0,
                    seuils: self.seuils.clone(),
                })
            }
            EXPLOIT => Some(Regle::Exploit {
                exploit: exploits::trouver(self.exploit.as_deref()?)?,
                parametre: self.parametre,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avancement {
    pub valeur: i64,
    pub seuils: Vec<i64>,
    pub atteints: Vec<Option<NaiveDate>>,
}

pub async fn avancement(
    pool: &PgPool,
    sub: &str,
    regle: &Regle,
    aujourd_hui: NaiveDate,
) -> Result<Avancement, sqlx::Error> {
    match regle {
        Regle::Paliers {
            stat,
            grain,
            seuils,
        } => {
            let p = catalogue::progression(pool, sub, stat, *grain, seuils, aujourd_hui).await?;
            Ok(Avancement {
                valeur: p.valeur,
                seuils: seuils.clone(),
                atteints: p.atteints,
            })
        }
        Regle::Exploit { exploit, parametre } => {
            let e = exploits::etat(pool, sub, exploit, *parametre, aujourd_hui).await?;
            Ok(Avancement {
                valeur: e.valeur,
                seuils: vec![e.seuil],
                atteints: vec![e.date],
            })
        }
    }
}

pub async fn charger(pool: &PgPool, seulement_actifs: bool) -> Result<Vec<Trophee>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM trophees WHERE actif OR NOT $1 ORDER BY ordre, id")
        .bind(seulement_actifs)
        .fetch_all(pool)
        .await
}

pub async fn evaluer(pool: &PgPool, sub: &str, aujourd_hui: NaiveDate) -> Result<u64, sqlx::Error> {
    let acquis: HashSet<(i64, i32)> =
        sqlx::query_as("SELECT trophee, palier FROM deblocages WHERE sub = $1")
            .bind(sub)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut nouveaux = 0;
    for t in charger(pool, true).await? {
        let Some(regle) = t.regle() else { continue };
        if (1i32..)
            .take(regle.paliers())
            .all(|p| acquis.contains(&(t.id, p)))
        {
            continue;
        }
        let a = avancement(pool, sub, &regle, aujourd_hui).await?;
        for (palier, date) in (1i32..).zip(a.atteints) {
            let Some(date) = date.filter(|_| !acquis.contains(&(t.id, palier))) else {
                continue;
            };
            nouveaux += sqlx::query(
                "INSERT INTO deblocages (sub, trophee, palier, date_deblocage)
                VALUES ($1, $2, $3, $4) ON CONFLICT (sub, trophee, palier) DO NOTHING",
            )
            .bind(sub)
            .bind(t.id)
            .bind(palier)
            .bind(date)
            .execute(pool)
            .await?
            .rows_affected();
        }
    }
    Ok(nouveaux)
}

pub async fn marquer(executeur: impl PgExecutor<'_>, sub: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO a_evaluer (sub) VALUES ($1)
        ON CONFLICT (sub) DO UPDATE SET marque_le = now()",
    )
    .bind(sub)
    .execute(executeur)
    .await?;
    Ok(())
}

pub async fn marquer_tout(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO a_evaluer (sub) SELECT DISTINCT sub FROM agregats_jour
        ON CONFLICT (sub) DO UPDATE SET marque_le = now()",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn rafraichir(
    pool: &PgPool,
    sub: &str,
    aujourd_hui: NaiveDate,
) -> Result<(), sqlx::Error> {
    let marque = sqlx::query("DELETE FROM a_evaluer WHERE sub = $1")
        .bind(sub)
        .execute(pool)
        .await?
        .rows_affected()
        == 1;
    if marque && let Err(e) = evaluer(pool, sub, aujourd_hui).await {
        marquer(pool, sub).await.ok();
        return Err(e);
    }
    Ok(())
}

pub async fn evaluer_les_marques(
    pool: &PgPool,
    aujourd_hui: NaiveDate,
) -> Result<usize, sqlx::Error> {
    let subs: Vec<String> = sqlx::query_scalar("SELECT sub FROM a_evaluer ORDER BY marque_le, sub")
        .fetch_all(pool)
        .await?;
    for sub in &subs {
        rafraichir(pool, sub, aujourd_hui).await?;
    }
    Ok(subs.len())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Public {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub tier: i32,
    pub unlocked_at: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Etat {
    pub trophee: Trophee,
    pub regle: Regle,
    pub paliers: Vec<(i32, NaiveDate, bool)>,
    pub partage: bool,
}

impl Etat {
    fn borne(&self, palier: i32) -> i32 {
        palier.min(i32::try_from(self.regle.paliers()).unwrap_or(i32::MAX))
    }

    pub fn palier(&self) -> i32 {
        self.borne(self.paliers.iter().map(|p| p.0).max().unwrap_or(0))
    }

    pub fn date(&self) -> Option<NaiveDate> {
        self.paliers.iter().max_by_key(|p| p.0).map(|p| p.1)
    }

    pub fn vu(&self) -> bool {
        self.paliers.iter().all(|p| p.2)
    }

    pub fn public(&self, du: NaiveDate, au: NaiveDate) -> Option<Public> {
        let (palier, date, _) = self
            .paliers
            .iter()
            .filter(|p| (du..=au).contains(&p.1))
            .max_by_key(|p| p.0)?;
        Some(Public {
            name: self.trophee.nom.clone(),
            description: self.trophee.description.clone(),
            icon: self.trophee.icone.clone(),
            tier: self.borne(*palier),
            unlocked_at: *date,
        })
    }
}

pub async fn etats(pool: &PgPool, sub: &str, droits: &Droits) -> Result<Vec<Etat>, sqlx::Error> {
    let mut paliers: HashMap<i64, Vec<(i32, NaiveDate, bool)>> = HashMap::new();
    let lignes: Vec<(i64, i32, NaiveDate, bool)> = sqlx::query_as(
        "SELECT trophee, palier, date_deblocage, vu_le IS NOT NULL FROM deblocages
        WHERE sub = $1 ORDER BY palier",
    )
    .bind(sub)
    .fetch_all(pool)
    .await?;
    for (trophee, palier, date, vu) in lignes {
        paliers.entry(trophee).or_default().push((palier, date, vu));
    }
    let partages: HashSet<i64> =
        sqlx::query_scalar("SELECT trophee FROM partages_trophees WHERE sub = $1")
            .bind(sub)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    Ok(charger(pool, true)
        .await?
        .into_iter()
        .filter_map(|trophee| {
            let regle = trophee.regle()?;
            regle.domaine().accessible(droits).then(|| Etat {
                paliers: paliers.remove(&trophee.id).unwrap_or_default(),
                partage: partages.contains(&trophee.id),
                trophee,
                regle,
            })
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Progres {
    pub value: i64,
    pub next_threshold: i64,
    pub unit: Unite,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Debloque {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub hidden: bool,
    pub tier: i32,
    pub tiers: usize,
    pub unlocked_at: NaiveDate,
    pub seen: bool,
    pub shared: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progres>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EnCours {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub tier: i32,
    pub tiers: usize,
    #[serde(flatten)]
    pub progres: Progres,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Trophees {
    pub unlocked: Vec<Debloque>,
    pub in_progress: Vec<EnCours>,
    pub hidden_remaining: usize,
}

pub async fn lire(
    pool: &PgPool,
    sub: &str,
    droits: &Droits,
    aujourd_hui: NaiveDate,
) -> Result<Trophees, sqlx::Error> {
    let mut r = Trophees::default();
    for e in etats(pool, sub, droits).await? {
        let (palier, paliers) = (e.palier(), e.regle.paliers());
        if palier == 0 && e.trophee.cache {
            r.hidden_remaining += 1;
            continue;
        }
        let atteint = usize::try_from(palier).unwrap_or_default();
        let progres = if atteint < paliers {
            let a = avancement(pool, sub, &e.regle, aujourd_hui).await?;
            a.seuils.get(atteint).map(|seuil| Progres {
                value: a.valeur,
                next_threshold: *seuil,
                unit: e.regle.unite(),
            })
        } else {
            None
        };
        let t = &e.trophee;
        if let Some(progres) = progres.clone() {
            r.in_progress.push(EnCours {
                id: t.id,
                key: t.cle.clone(),
                name: t.nom.clone(),
                description: t.description.clone(),
                icon: t.icone.clone(),
                tier: palier,
                tiers: paliers,
                progres,
            });
        }
        if let Some(date) = e.date() {
            r.unlocked.push(Debloque {
                id: t.id,
                key: t.cle.clone(),
                name: t.nom.clone(),
                description: t.description.clone(),
                icon: t.icone.clone(),
                hidden: t.cache,
                tier: palier,
                tiers: paliers,
                unlocked_at: date,
                seen: e.vu(),
                shared: e.partage,
                progress: progres,
            });
        }
    }
    Ok(r)
}

pub async fn non_vus(pool: &PgPool, sub: &str, droits: &Droits) -> Result<usize, sqlx::Error> {
    Ok(etats(pool, sub, droits)
        .await?
        .iter()
        .filter(|e| !e.vu())
        .count())
}

pub async fn voir(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    rafraichir(state.pool(), p.sub(), aujourd_hui)
        .await
        .map_err(bd)?;
    Ok(ok(&lire(state.pool(), p.sub(), &p.droits, aujourd_hui)
        .await
        .map_err(bd)?))
}

pub async fn voir_non_vus(
    State(state): State<AppState>,
    p: Personne,
) -> Result<Response, Response> {
    let pool = state.pool();
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    rafraichir(pool, p.sub(), aujourd_hui).await.map_err(bd)?;
    Ok(ok(&json!({
        "trophies": non_vus(pool, p.sub(), &p.droits).await.map_err(bd)?,
        "recaps": recaps::non_vus(pool, p.sub()).await.map_err(bd)?,
    })))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Choix {
    Tous(String),
    Liste(Vec<i64>),
}

impl Choix {
    pub fn lire(choix: Option<Self>) -> Result<(bool, Vec<i64>), Response> {
        match choix {
            None => Ok((false, Vec::new())),
            Some(Choix::Liste(ids)) => Ok((false, ids)),
            Some(Choix::Tous(t)) if t == "all" => Ok((true, Vec::new())),
            Some(Choix::Tous(_)) => Err(invalide(MESSAGE_VUS)),
        }
    }
}

const MESSAGE_VUS: &str = "trophies, recaps : liste d'identifiants ou \"all\"";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vus {
    trophies: Option<Choix>,
    recaps: Option<Choix>,
}

pub async fn marquer_vus(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let vus: Vus = corps(&octets, MESSAGE_VUS)?;
    let (tous, ids) = Choix::lire(vus.trophies)?;
    let recaps = Choix::lire(vus.recaps)?;
    sqlx::query(
        "UPDATE deblocages SET vu_le = now()
        WHERE sub = $1 AND vu_le IS NULL AND ($2 OR trophee = ANY($3))",
    )
    .bind(p.sub())
    .bind(tous)
    .bind(&ids)
    .execute(state.pool())
    .await
    .map_err(bd)?;
    recaps::marquer_vus(state.pool(), p.sub(), recaps.0, &recaps.1)
        .await
        .map_err(bd)?;
    Ok(sans_contenu())
}

fn genre_public(t: &Trophee) -> &'static str {
    if t.genre == PALIERS { "tiers" } else { EXPLOIT }
}

fn periodes() -> Vec<&'static str> {
    GRAINS.iter().map(|(p, _)| p.nom()).collect()
}

fn admin(t: &Trophee, debloque_par: i64) -> Value {
    let periode = t.periode.as_deref().and_then(|base| {
        GRAINS
            .iter()
            .find(|(_, nom)| *nom == base)
            .map(|(p, _)| p.nom())
    });
    json!({
        "id": t.id,
        "key": t.cle,
        "name": t.nom,
        "description": t.description,
        "icon": t.icone,
        "hidden": t.cache,
        "active": t.actif,
        "kind": genre_public(t),
        "stat": t.stat,
        "period": periode,
        "thresholds": t.seuils,
        "exploit": t.exploit,
        "parameter": t.parametre,
        "order": t.ordre,
        "unlocked_by": debloque_par,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saisie {
    key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    hidden: Option<bool>,
    active: Option<bool>,
    kind: Option<String>,
    stat: Option<String>,
    period: Option<String>,
    thresholds: Option<Vec<i64>>,
    exploit: Option<String>,
    parameter: Option<i64>,
    order: Option<i32>,
}

const MESSAGE_SAISIE: &str = "corps : key, name, description, icon, hidden, active, kind, stat, period, thresholds, exploit, parameter, order";

fn forme(texte: &str, max: usize, permis: fn(char) -> bool) -> bool {
    (1..=max).contains(&texte.len()) && texte.chars().all(permis)
}

fn valider(mut t: Trophee) -> Result<Trophee, String> {
    let minuscule = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    if !forme(&t.cle, 60, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
    }) {
        return Err("key : 1 à 60 caractères parmi a-z, 0-9, _ et -".into());
    }
    if !(1..=60).contains(&t.nom.chars().count()) {
        return Err("name : 1 à 60 caractères".into());
    }
    if !(1..=200).contains(&t.description.chars().count()) {
        return Err("description : 1 à 200 caractères".into());
    }
    if !(1..=40).contains(&t.icone.len()) || !t.icone.chars().all(|c| minuscule(c) || c == '-') {
        return Err("icon : 1 à 40 caractères parmi a-z, 0-9 et -".into());
    }
    match t.genre.as_str() {
        PALIERS => {
            if !t
                .stat
                .as_deref()
                .and_then(catalogue::trouver)
                .is_some_and(Stat::pour_paliers)
            {
                return Err("stat : une stat à paliers du catalogue".into());
            }
            if t.periode.is_none() {
                return Err(format!("period : {}", periodes().join(", ")));
            }
            let croissants = t.seuils.windows(2).all(|s| s[0] < s[1]);
            if !(1..=PALIERS_MAX).contains(&t.seuils.len()) || t.seuils[0] < 1 || !croissants {
                return Err(format!(
                    "thresholds : 1 à {PALIERS_MAX} seuils positifs strictement croissants"
                ));
            }
        }
        _ => {
            let Some(exploit) = t.exploit.as_deref().and_then(exploits::trouver) else {
                return Err("exploit : un exploit du registre".into());
            };
            t.parametre = t.parametre.or(exploit.parametre);
            match (exploit.parametre, t.parametre) {
                (None, Some(_)) => return Err("parameter : cet exploit n'en prend pas".into()),
                (Some(_), Some(p)) if p < 1 => return Err("parameter : 1 au moins".into()),
                _ => {}
            }
        }
    }
    Ok(t)
}

fn appliquer(mut t: Trophee, s: Saisie, creation: bool) -> Result<Trophee, String> {
    if creation {
        t.cle = s.key.unwrap_or_default();
        t.genre = match s.kind.as_deref() {
            Some("tiers") => PALIERS,
            Some(EXPLOIT) => EXPLOIT,
            _ => return Err("kind : tiers ou exploit".into()),
        }
        .to_string();
        t.actif = true;
    } else if s.key.is_some_and(|k| k != t.cle) || s.kind.is_some_and(|k| k != genre_public(&t)) {
        return Err("key et kind ne se modifient pas".into());
    }
    if let Some(v) = s.name {
        t.nom = v.trim().to_string();
    }
    if let Some(v) = s.description {
        t.description = v.trim().to_string();
    }
    if let Some(v) = s.icon {
        t.icone = v;
    }
    t.cache = s.hidden.unwrap_or(t.cache);
    t.actif = s.active.unwrap_or(t.actif);
    t.ordre = s.order.unwrap_or(t.ordre);
    if t.genre == PALIERS {
        if s.exploit.is_some() || s.parameter.is_some() {
            return Err("exploit, parameter : réservés au genre exploit".into());
        }
        t.stat = s.stat.or(t.stat);
        if let Some(v) = s.period {
            let (_, base) = GRAINS
                .iter()
                .find(|(p, _)| p.nom() == v)
                .ok_or_else(|| format!("period : {}", periodes().join(", ")))?;
            t.periode = Some((*base).to_string());
        }
        t.seuils = s.thresholds.unwrap_or(t.seuils);
    } else {
        if s.stat.is_some() || s.period.is_some() || s.thresholds.is_some() {
            return Err("stat, period, thresholds : réservés au genre tiers".into());
        }
        if s.exploit.is_some() {
            t.exploit = s.exploit;
            t.parametre = None;
        }
        t.parametre = s.parameter.or(t.parametre);
    }
    valider(t)
}

async fn recalculer(state: &AppState) {
    let pool = state.pool();
    let passe = async {
        marquer_tout(pool).await?;
        evaluer_les_marques(pool, state.aujourd_hui().await?).await
    };
    if let Err(e) = passe.await {
        tracing::warn!("trophées : recalcul sur l'historique reporté à la tâche de fond ({e})");
    }
}

pub async fn admin_lister(State(state): State<AppState>) -> Result<Response, Response> {
    let pool = state.pool();
    let comptes: HashMap<i64, i64> =
        sqlx::query_as("SELECT trophee, COUNT(DISTINCT sub) FROM deblocages GROUP BY trophee")
            .fetch_all(pool)
            .await
            .map_err(bd)?
            .into_iter()
            .collect();
    let liste: Vec<Value> = charger(pool, false)
        .await
        .map_err(bd)?
        .iter()
        .map(|t| admin(t, comptes.get(&t.id).copied().unwrap_or(0)))
        .collect();
    Ok(ok(&liste))
}

pub async fn admin_catalogue() -> Response {
    let stats: Vec<Value> = catalogue::STATS
        .iter()
        .map(|s| {
            let mut v = json!(s);
            v["tiers"] = json!(s.pour_paliers());
            v
        })
        .collect();
    ok(&json!({
        "stats": stats,
        "exploits": exploits::EXPLOITS,
        "periods": periodes(),
    }))
}

pub async fn admin_creer(
    State(state): State<AppState>,
    octets: Bytes,
) -> Result<Response, Response> {
    let saisie: Saisie = corps(&octets, MESSAGE_SAISIE)?;
    let ordre = saisie.order;
    let t = appliquer(Trophee::default(), saisie, true).map_err(|m| invalide(&m))?;
    let cree: Trophee = sqlx::query_as(
        "INSERT INTO trophees
            (cle, nom, description, icone, cache, actif, genre, stat, periode, seuils, exploit, parametre, ordre)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            COALESCE($13, (SELECT COALESCE(MAX(ordre), 0) + 1 FROM trophees)))
        RETURNING *",
    )
    .bind(&t.cle)
    .bind(&t.nom)
    .bind(&t.description)
    .bind(&t.icone)
    .bind(t.cache)
    .bind(t.actif)
    .bind(&t.genre)
    .bind(&t.stat)
    .bind(&t.periode)
    .bind(&t.seuils)
    .bind(&t.exploit)
    .bind(t.parametre)
    .bind(ordre)
    .fetch_one(state.pool())
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            erreur(StatusCode::CONFLICT, "conflict", "key déjà utilisée")
        } else {
            bd(e)
        }
    })?;
    if cree.actif {
        recalculer(&state).await;
    }
    Ok(http::json(StatusCode::CREATED, &admin(&cree, 0)))
}

pub async fn admin_modifier(
    State(state): State<AppState>,
    Path(id): Path<String>,
    octets: Bytes,
) -> Result<Response, Response> {
    let pool = state.pool();
    let id: i64 = id.parse().map_err(|_| introuvable())?;
    let saisie: Saisie = corps(&octets, MESSAGE_SAISIE)?;
    let ancien: Trophee = sqlx::query_as("SELECT * FROM trophees WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(bd)?
        .ok_or_else(introuvable)?;
    let t = appliquer(ancien, saisie, false).map_err(|m| invalide(&m))?;
    let modifie: Trophee = sqlx::query_as(
        "UPDATE trophees SET nom = $2, description = $3, icone = $4, cache = $5, actif = $6,
            stat = $7, periode = $8, seuils = $9, exploit = $10, parametre = $11, ordre = $12,
            modifie_le = now()
        WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(&t.nom)
    .bind(&t.description)
    .bind(&t.icone)
    .bind(t.cache)
    .bind(t.actif)
    .bind(&t.stat)
    .bind(&t.periode)
    .bind(&t.seuils)
    .bind(&t.exploit)
    .bind(t.parametre)
    .bind(t.ordre)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    if modifie.actif {
        recalculer(&state).await;
    }
    let debloque_par: i64 =
        sqlx::query_scalar("SELECT COUNT(DISTINCT sub) FROM deblocages WHERE trophee = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(bd)?;
    Ok(ok(&admin(&modifie, debloque_par)))
}

#[cfg(test)]
mod tests {
    use commun::evenements::Donnees;

    use super::*;
    use crate::{
        agregats,
        domaines::{entracte, messagerie},
        testutil::{Env, attendre, env, id_estompe, jour},
    };

    const ALICE: &str = "sub-alice";
    const BRUNO: &str = "sub-bruno";
    const DIANE: &str = "sub-diane";

    async fn ajouter(e: &Env, sub: &str, j: &str, stat: &str, cle: &str, n: i64) {
        agregats::ajouter(e.state.pool(), sub, jour(j), stat, cle, n)
            .await
            .unwrap();
    }

    async fn id_de(e: &Env, cle: &str) -> i64 {
        sqlx::query_scalar("SELECT id FROM trophees WHERE cle = $1")
            .bind(cle)
            .fetch_one(e.state.pool())
            .await
            .unwrap()
    }

    async fn deblocages(e: &Env, sub: &str, cle: &str) -> Vec<(i32, String, bool)> {
        sqlx::query_as(
            "SELECT d.palier, d.date_deblocage::text, d.vu_le IS NOT NULL
            FROM deblocages d JOIN trophees t ON t.id = d.trophee
            WHERE d.sub = $1 AND t.cle = $2 ORDER BY d.palier",
        )
        .bind(sub)
        .bind(cle)
        .fetch_all(e.state.pool())
        .await
        .unwrap()
    }

    fn le(palier: i32, date: &str, vu: bool) -> (i32, String, bool) {
        (palier, date.to_string(), vu)
    }

    fn trouve(reponse: &Value, liste: &str, cle: &str) -> Option<Value> {
        reponse[liste]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == cle)
            .cloned()
    }

    async fn pipelette_pour_alice(e: &Env) {
        for (j, cle, n) in [
            ("2026-03-01", "srv/a", 190),
            ("2026-03-03", "prive", 190),
            ("2026-03-05", "srv/a", 120),
        ] {
            ajouter(e, ALICE, j, messagerie::MESSAGES, cle, n).await;
        }
    }

    async fn modifier(e: &Env, cle: &str, corps: Value) -> Value {
        let chemin = format!("/admin/trophies/{}", id_de(e, cle).await);
        let r = e.appel(Some("admin"), "PATCH", &chemin, Some(corps)).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        r.json()
    }

    #[tokio::test]
    async fn jeu_de_depart_de_trente_deux_trophees_valides() {
        let Some(e) = env().await else { return };
        let tous = charger(e.state.pool(), false).await.unwrap();
        assert_eq!(tous.len(), 32);
        assert_eq!(tous.iter().filter(|t| t.cache).count(), 5);
        assert_eq!(tous.iter().filter(|t| t.genre == EXPLOIT).count(), 10);
        let ordres: Vec<i32> = tous.iter().map(|t| t.ordre).collect();
        assert_eq!(ordres, (1..=32).collect::<Vec<_>>());
        for t in &tous {
            assert!(t.actif && t.regle().is_some(), "{}", t.cle);
            assert_eq!(valider(t.clone()).as_ref(), Ok(t), "{}", t.cle);
        }
        let series = tous
            .iter()
            .filter(|t| t.stat.as_deref().is_some_and(|s| s.starts_with("streak.")))
            .count();
        assert_eq!(series, 4);
        let pellicule = tous
            .iter()
            .find(|t| t.cle == "heures_de_pellicule")
            .unwrap();
        assert_eq!(
            pellicule.seuils,
            [24 * 3600, 100 * 3600, 500 * 3600, 1500 * 3600]
        );
    }

    #[tokio::test]
    async fn c1_un_seuil_atteint_debloque_le_palier_une_seule_fois_avec_sa_date() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let aujourd_hui = jour("2026-11-18");
        let terminees = entracte::SERIES_TERMINEES;
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 0);
        ajouter(&e, DIANE, "2026-03-01", terminees, "", 1).await;
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 2);
        let generique = "jusqu_au_generique";
        assert_eq!(
            deblocages(&e, DIANE, generique).await,
            [le(1, "2026-03-01", false)]
        );
        assert_eq!(
            deblocages(&e, DIANE, "premier_jour").await,
            [le(1, "2026-03-01", false)]
        );
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 0);
        let r = e
            .appel(
                Some("diane"),
                "POST",
                "/seen",
                Some(json!({"trophies": "all"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        ajouter(&e, DIANE, "2026-03-04", terminees, "", 3).await;
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 0);
        ajouter(&e, DIANE, "2026-03-06", terminees, "", 2).await;
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 1);
        let attendus = [le(1, "2026-03-01", true), le(2, "2026-03-06", false)];
        assert_eq!(deblocages(&e, DIANE, generique).await, attendus);
        ajouter(&e, DIANE, "2026-03-02", terminees, "", 1).await;
        assert_eq!(evaluer(pool, DIANE, aujourd_hui).await.unwrap(), 0);
        assert_eq!(deblocages(&e, DIANE, generique).await, attendus);
        e.recevoir(
            "serie-7",
            DIANE,
            "2026-03-07T10:00:00Z",
            false,
            Donnees::EntracteSerieTerminee { serie: "s7".into() },
        )
        .await;
        let marquees: Vec<String> = sqlx::query_scalar("SELECT sub FROM a_evaluer")
            .fetch_all(pool)
            .await
            .unwrap();
        assert_eq!(marquees, [DIANE]);
        let r = e.get("diane", "/trophies").await;
        assert_eq!(r.statut, StatusCode::OK);
        let r = r.json();
        assert_eq!(
            trouve(&r, "unlocked", generique).unwrap(),
            json!({
                "id": id_de(&e, generique).await,
                "key": generique,
                "name": "Jusqu'au générique",
                "description": "Des séries terminées.",
                "icon": "clap",
                "hidden": false,
                "tier": 2,
                "tiers": 4,
                "unlocked_at": "2026-03-06",
                "seen": false,
                "shared": false,
                "progress": {"value": 8, "next_threshold": 15, "unit": "count"},
            })
        );
        assert_eq!(
            trouve(&r, "in_progress", generique).unwrap(),
            json!({
                "id": id_de(&e, generique).await,
                "key": generique,
                "name": "Jusqu'au générique",
                "description": "Des séries terminées.",
                "icon": "clap",
                "tier": 2,
                "tiers": 4,
                "value": 8,
                "next_threshold": 15,
                "unit": "count",
            })
        );
        let premier = trouve(&r, "unlocked", "premier_jour").unwrap();
        assert_eq!(
            (
                premier["tier"].clone(),
                premier["tiers"].clone(),
                premier["seen"].clone()
            ),
            (json!(1), json!(1), json!(true))
        );
        assert!(premier.get("progress").is_none());
        assert!(trouve(&r, "in_progress", "premier_jour").is_none());
        assert_eq!(r["unlocked"].as_array().unwrap().len(), 2);
        let restantes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM a_evaluer")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(restantes, 0);
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deblocages WHERE sub = $1")
            .bind(DIANE)
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn c2_seuil_releve_ne_retire_rien_seuil_abaisse_ou_trophee_cree_debloque_sur_l_historique()
     {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        pipelette_pour_alice(&e).await;
        ajouter(&e, BRUNO, "2026-03-02", messagerie::MESSAGES, "srv/a", 100).await;
        for sub in [ALICE, BRUNO] {
            evaluer(pool, sub, jour("2026-11-18")).await.unwrap();
        }
        assert_eq!(
            deblocages(&e, ALICE, "pipelette").await,
            [le(1, "2026-03-05", false)]
        );
        assert!(deblocages(&e, BRUNO, "pipelette").await.is_empty());
        let r = modifier(&e, "pipelette", json!({"thresholds": [1000, 5000]})).await;
        assert_eq!(
            (r["thresholds"].clone(), r["unlocked_by"].clone()),
            (json!([1000, 5000]), json!(1))
        );
        assert_eq!(
            deblocages(&e, ALICE, "pipelette").await,
            [le(1, "2026-03-05", false)]
        );
        let r = e.get("alice", "/trophies").await.json();
        let t = trouve(&r, "unlocked", "pipelette").unwrap();
        assert_eq!(
            (
                t["tier"].clone(),
                t["tiers"].clone(),
                t["unlocked_at"].clone()
            ),
            (json!(1), json!(2), json!("2026-03-05"))
        );
        assert_eq!(
            t["progress"],
            json!({"value": 500, "next_threshold": 5000, "unit": "count"})
        );
        let r = modifier(&e, "pipelette", json!({"thresholds": [50, 5000]})).await;
        assert_eq!(r["unlocked_by"], 2);
        assert_eq!(
            deblocages(&e, BRUNO, "pipelette").await,
            [le(1, "2026-03-02", false)]
        );
        assert_eq!(
            deblocages(&e, ALICE, "pipelette").await,
            [le(1, "2026-03-05", false)]
        );
        let r = e
            .appel(
                Some("admin"),
                "POST",
                "/admin/trophies",
                Some(json!({
                    "key": "grande_bavarde",
                    "name": "Grande bavarde",
                    "description": "Beaucoup de messages en un jour.",
                    "icon": "bulle",
                    "kind": "tiers",
                    "stat": "messaging.busiest_day",
                    "period": "day",
                    "thresholds": [100, 150],
                })),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(
            (r.json()["order"].clone(), r.json()["active"].clone()),
            (json!(33), json!(true))
        );
        assert_eq!(
            deblocages(&e, ALICE, "grande_bavarde").await,
            [le(1, "2026-03-01", false), le(2, "2026-03-01", false)]
        );
        assert_eq!(
            deblocages(&e, BRUNO, "grande_bavarde").await,
            [le(1, "2026-03-02", false)]
        );
        let r = e
            .appel(
                Some("admin"),
                "POST",
                "/admin/trophies",
                Some(json!({
                    "key": "bavarde",
                    "name": "Bavarde",
                    "description": "Cent quatre-vingts messages dans la journée.",
                    "icon": "bulle",
                    "hidden": true,
                    "kind": "exploit",
                    "exploit": "journee_bavarde",
                    "parameter": 180,
                })),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(
            deblocages(&e, ALICE, "bavarde").await,
            [le(1, "2026-03-01", false)]
        );
        assert!(deblocages(&e, BRUNO, "bavarde").await.is_empty());
        let r = e.get("admin", "/admin/trophies").await.json();
        let comptes: Vec<(String, i64)> = r
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["unlocked_by"] != 0)
            .map(|t| {
                (
                    t["key"].as_str().unwrap().to_string(),
                    t["unlocked_by"].as_i64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            comptes,
            [
                ("pipelette".to_string(), 2),
                ("premier_jour".to_string(), 2),
                ("grande_bavarde".to_string(), 2),
                ("bavarde".to_string(), 1),
            ]
        );
    }

    #[tokio::test]
    async fn c3_trophee_desactive_absent_des_reponses_puis_de_retour_avec_ses_deblocages() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        pipelette_pour_alice(&e).await;
        evaluer(pool, ALICE, jour("2026-11-18")).await.unwrap();
        let id = id_de(&e, "pipelette").await;
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/seen",
                Some(json!({"trophies": [id]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        let partage = json!({"trophies": {id.to_string(): true}});
        let r = e
            .appel(Some("alice"), "PUT", "/sharing", Some(partage.clone()))
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let avant = e.get("alice", "/trophies").await.json();
        let attendu = trouve(&avant, "unlocked", "pipelette").unwrap();
        assert_eq!(
            (attendu["seen"].clone(), attendu["shared"].clone()),
            (json!(true), json!(true))
        );
        assert_eq!(e.get("alice", "/unseen").await.json()["trophies"], 1);
        let r = modifier(&e, "pipelette", json!({"active": false})).await;
        assert_eq!(
            (r["active"].clone(), r["unlocked_by"].clone()),
            (json!(false), json!(1))
        );
        let profil = format!("/profiles/{}", id_estompe("alice"));
        for chemin in ["/trophies", "/sharing", &profil, "/unseen"] {
            let r = e.get("alice", chemin).await;
            assert_eq!(r.statut, StatusCode::OK, "{chemin}");
            assert!(!r.texte().contains("ipelette"), "{chemin} : {}", r.texte());
        }
        let r = e
            .appel(Some("alice"), "PUT", "/sharing", Some(partage))
            .await;
        assert_eq!(r.statut, StatusCode::BAD_REQUEST);
        ajouter(&e, ALICE, "2026-03-09", messagerie::MESSAGES, "prive", 150).await;
        marquer(pool, ALICE).await.unwrap();
        e.get("alice", "/trophies").await;
        assert_eq!(
            deblocages(&e, ALICE, "pipelette").await,
            [le(1, "2026-03-05", true)]
        );
        let liste = e.get("admin", "/admin/trophies").await.json();
        let inactif = liste
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == "pipelette")
            .unwrap();
        assert_eq!(inactif["active"], false);
        modifier(&e, "pipelette", json!({"active": true})).await;
        let apres = e.get("alice", "/trophies").await.json();
        let mut revenu = trouve(&apres, "unlocked", "pipelette").unwrap();
        assert_eq!(revenu["progress"]["value"], 650);
        revenu["progress"]["value"] = json!(500);
        assert_eq!(revenu, attendu);
        let r = e.get("alice", &profil).await.json();
        assert_eq!(
            r["trophies"],
            json!([{
                "name": "Pipelette",
                "description": "Des messages envoyés.",
                "icon": "bulle",
                "tier": 1,
                "unlocked_at": "2026-03-05",
            }])
        );
    }

    #[tokio::test]
    async fn c4_trophee_cache_non_debloque_absent_de_toute_reponse_hors_administration() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let caches: Vec<Trophee> = charger(pool, false)
            .await
            .unwrap()
            .into_iter()
            .filter(|t| t.cache)
            .collect();
        assert_eq!(caches.len(), 5);
        ajouter(&e, DIANE, "2026-03-01", entracte::INTROS_PASSEES, "", 49).await;
        ajouter(&e, DIANE, "2026-03-01", entracte::ABANDONS, "", 9).await;
        ajouter(
            &e,
            DIANE,
            "2026-03-01",
            messagerie::REACTIONS_DONNEES,
            "👍",
            499,
        )
        .await;
        ajouter(&e, DIANE, "2026-03-01", messagerie::MESSAGES, "srv/a", 150).await;
        let aujourd_hui = jour("2026-11-18");
        evaluer(pool, DIANE, aujourd_hui).await.unwrap();
        let interdits: Vec<String> = caches
            .iter()
            .flat_map(|t| {
                [
                    Some(t.nom.clone()),
                    Some(t.description.clone()),
                    Some(format!("\"{}\"", t.cle)),
                    t.stat.clone(),
                    t.exploit.clone(),
                ]
            })
            .flatten()
            .chain(
                caches
                    .iter()
                    .filter_map(|t| t.exploit.as_deref().and_then(exploits::trouver))
                    .flat_map(|x| [x.libelle.to_string(), x.condition.to_string()]),
            )
            .collect();
        assert_eq!(interdits.len(), 5 * 3 + 5 + 6);
        let profil = format!("/profiles/{}", id_estompe("diane"));
        let chemins = [
            "/trophies",
            "/unseen",
            "/sharing",
            "/streaks",
            "/recaps",
            &profil,
            "/usage",
            "/messaging",
        ];
        for chemin in chemins {
            let r = e.get("diane", chemin).await;
            assert_eq!(r.statut, StatusCode::OK, "{chemin}");
            for interdit in &interdits {
                assert!(
                    !r.texte().contains(interdit),
                    "{chemin} révèle « {interdit} »"
                );
            }
        }
        let r = e.get("diane", "/trophies").await.json();
        assert_eq!(r["hidden_remaining"], 5);
        assert_eq!(
            e.get("alice", "/trophies").await.json()["hidden_remaining"],
            3
        );
        assert_eq!(
            e.get("diane", "/admin/trophies").await.statut,
            StatusCode::FORBIDDEN
        );
        let r = e.get("admin", "/admin/trophies").await;
        assert_eq!(r.json().as_array().unwrap().len(), 32);
        assert!(interdits[..20].iter().all(|i| r.texte().contains(i)));
        ajouter(&e, DIANE, "2026-03-04", entracte::INTROS_PASSEES, "", 1).await;
        evaluer(pool, DIANE, aujourd_hui).await.unwrap();
        let r = e.get("diane", "/trophies").await.json();
        assert_eq!(r["hidden_remaining"], 4);
        let t = trouve(&r, "unlocked", "droit_au_but").unwrap();
        assert_eq!(
            (
                t["hidden"].clone(),
                t["tier"].clone(),
                t["unlocked_at"].clone()
            ),
            (json!(true), json!(1), json!("2026-03-04"))
        );
        assert_eq!(
            trouve(&r, "in_progress", "droit_au_but").unwrap()["next_threshold"],
            250
        );
        let texte = e.get("diane", "/trophies").await.texte();
        for interdit in ["Zappeur", "Signature", "Habitant du salon", "Mois parfait"] {
            assert!(!texte.contains(interdit), "{interdit}");
        }
    }

    #[tokio::test]
    async fn trophees_non_vus_puis_vus() {
        let Some(e) = env().await else { return };
        pipelette_pour_alice(&e).await;
        marquer(e.state.pool(), ALICE).await.unwrap();
        assert_eq!(
            e.get("alice", "/unseen").await.json(),
            json!({"trophies": 2, "recaps": 0})
        );
        let id = id_de(&e, "pipelette").await;
        let vu = |corps: Value| e.appel(Some("alice"), "POST", "/seen", Some(corps));
        assert_eq!(vu(json!({})).await.statut, StatusCode::NO_CONTENT);
        assert_eq!(
            vu(json!({"trophies": [id, 999_999]})).await.statut,
            StatusCode::NO_CONTENT
        );
        assert_eq!(e.get("alice", "/unseen").await.json()["trophies"], 1);
        let r = e
            .appel(
                Some("bruno"),
                "POST",
                "/seen",
                Some(json!({"trophies": "all"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::NO_CONTENT);
        assert_eq!(e.get("alice", "/unseen").await.json()["trophies"], 1);
        for faux in [
            json!({"trophies": "tout"}),
            json!({"trophies": ["a"]}),
            json!({"recaps": 3}),
            json!({"autre": []}),
        ] {
            let r = vu(faux.clone()).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
            assert_eq!(r.json()["error"], "invalid");
        }
        assert_eq!(
            vu(json!({"trophies": "all", "recaps": "all"})).await.statut,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            e.get("alice", "/unseen").await.json(),
            json!({"trophies": 0, "recaps": 0})
        );
        let r = e.get("alice", "/trophies").await.json();
        assert!(
            r["unlocked"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["seen"] == true)
        );
    }

    #[tokio::test]
    async fn personne_marquee_evaluee_par_la_tache_de_fond() {
        let Some(e) = env().await else { return };
        pipelette_pour_alice(&e).await;
        marquer(e.state.pool(), ALICE).await.unwrap();
        e.state.demarrer_taches();
        attendre(
            || async { !deblocages(&e, ALICE, "pipelette").await.is_empty() },
            "trophée débloqué par la tâche de fond",
        )
        .await;
        assert_eq!(
            deblocages(&e, ALICE, "pipelette").await,
            [le(1, "2026-03-05", false)]
        );
    }

    #[tokio::test]
    async fn configuration_des_trophees_validee_cote_serveur() {
        let Some(e) = env().await else { return };
        let creer = |corps: Value| e.appel(Some("admin"), "POST", "/admin/trophies", Some(corps));
        let base = json!({
            "key": "nouveau",
            "name": "Nouveau",
            "description": "Un trophée de plus.",
            "icon": "etoile",
            "kind": "tiers",
            "stat": "sillon.seconds",
            "period": "week",
            "thresholds": [10, 20],
        });
        let avec = |cle: &str, valeur: Value| {
            let mut c = base.clone();
            c[cle] = valeur;
            c
        };
        let sans = |cle: &str| {
            let mut c = base.clone();
            c.as_object_mut().unwrap().remove(cle);
            c
        };
        let exploit = json!({
            "key": "exploit",
            "name": "Exploit",
            "description": "Un exploit.",
            "icon": "etoile",
            "kind": "exploit",
            "exploit": "premier_jour",
        });
        let mut exploit_avec = exploit.clone();
        exploit_avec["parameter"] = json!(3);
        let mut exploit_inconnu = exploit.clone();
        exploit_inconnu["exploit"] = json!("saison_en_un_week_end");
        let mut exploit_a_seuils = exploit.clone();
        exploit_a_seuils["thresholds"] = json!([1]);
        let mut exploit_parametre_nul = exploit.clone();
        exploit_parametre_nul["exploit"] = json!("meme_emoji");
        exploit_parametre_nul["parameter"] = json!(0);
        for (faux, champ) in [
            (sans("key"), "key"),
            (avec("key", json!("Avec Espace")), "key"),
            (avec("key", json!("a".repeat(61))), "key"),
            (sans("name"), "name"),
            (avec("name", json!("  ")), "name"),
            (avec("name", json!("n".repeat(61))), "name"),
            (sans("description"), "description"),
            (avec("description", json!("d".repeat(201))), "description"),
            (sans("icon"), "icon"),
            (avec("icon", json!("Etoile")), "icon"),
            (avec("icon", json!("<svg>")), "icon"),
            (avec("icon", json!("i".repeat(41))), "icon"),
            (sans("kind"), "kind"),
            (avec("kind", json!("paliers")), "kind"),
            (sans("stat"), "stat"),
            (avec("stat", json!("sillon.genres")), "stat"),
            (avec("stat", json!("usage.first_day")), "stat"),
            (sans("period"), "period"),
            (avec("period", json!("year")), "period"),
            (sans("thresholds"), "thresholds"),
            (avec("thresholds", json!([])), "thresholds"),
            (avec("thresholds", json!([0, 5])), "thresholds"),
            (avec("thresholds", json!([5, 5])), "thresholds"),
            (avec("thresholds", json!([5, 4])), "thresholds"),
            (
                avec("thresholds", json!([1, 2, 3, 4, 5, 6, 7])),
                "thresholds",
            ),
            (avec("exploit", json!("premier_jour")), "exploit"),
            (exploit_avec, "parameter"),
            (exploit_inconnu, "exploit"),
            (exploit_a_seuils, "thresholds"),
            (exploit_parametre_nul, "parameter"),
            (avec("inconnu", json!(1)), "corps"),
        ] {
            let r = creer(faux.clone()).await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
            assert_eq!(r.json()["error"], "invalid");
            assert!(
                r.json()["message"].as_str().unwrap().contains(champ),
                "{faux} : {}",
                r.texte()
            );
        }
        let r = creer(avec("thresholds", json!([1, 2, 3, 4, 5, 6]))).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        let cree = r.json();
        assert_eq!(
            cree,
            json!({
                "id": cree["id"],
                "key": "nouveau",
                "name": "Nouveau",
                "description": "Un trophée de plus.",
                "icon": "etoile",
                "hidden": false,
                "active": true,
                "kind": "tiers",
                "stat": "sillon.seconds",
                "period": "week",
                "thresholds": [1, 2, 3, 4, 5, 6],
                "exploit": null,
                "parameter": null,
                "order": 33,
                "unlocked_by": 0,
            })
        );
        let r = creer(base.clone()).await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::CONFLICT, json!("conflict"))
        );
        let r = creer(exploit).await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        assert_eq!(
            (r.json()["kind"].clone(), r.json()["parameter"].clone()),
            (json!("exploit"), Value::Null)
        );
        let r = modifier(&e, "exploit", json!({"exploit": "meme_emoji", "order": 2})).await;
        assert_eq!(
            (r["parameter"].clone(), r["order"].clone()),
            (json!(500), json!(2))
        );
        let r = modifier(&e, "exploit", json!({"parameter": 40, "hidden": true})).await;
        assert_eq!(
            (r["parameter"].clone(), r["hidden"].clone()),
            (json!(40), json!(true))
        );
        let r = modifier(&e, "exploit", json!({"exploit": "mois_parfait"})).await;
        assert_eq!(r["parameter"], Value::Null);
        let r = modifier(&e, "nouveau", json!({"name": " Renommé ", "period": "all"})).await;
        assert_eq!(
            (
                r["name"].clone(),
                r["period"].clone(),
                r["thresholds"].clone()
            ),
            (json!("Renommé"), json!("all"), json!([1, 2, 3, 4, 5, 6]))
        );
        let chemin = format!("/admin/trophies/{}", id_de(&e, "nouveau").await);
        for faux in [
            json!({"key": "autre"}),
            json!({"kind": "exploit"}),
            json!({"thresholds": [3, 2]}),
            json!({"icon": ""}),
            json!({"parameter": 5}),
        ] {
            let r = e
                .appel(Some("admin"), "PATCH", &chemin, Some(faux.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
        }
        for inconnu in ["/admin/trophies/999999", "/admin/trophies/abc"] {
            let r = e
                .appel(Some("admin"), "PATCH", inconnu, Some(json!({"name": "x"})))
                .await;
            assert_eq!(r.statut, StatusCode::NOT_FOUND, "{inconnu}");
        }
        let r = e.get("admin", "/admin/catalogue").await;
        assert_eq!(r.statut, StatusCode::OK);
        let c = r.json();
        assert_eq!(c["periods"], json!(["day", "week", "month", "all"]));
        assert_eq!(c["stats"].as_array().unwrap().len(), 30);
        assert_eq!(
            c["stats"][0],
            json!({
                "key": "usage.active_seconds",
                "label": "Temps actif",
                "domain": "usage",
                "unit": "seconds",
                "shareable": true,
                "tiers": true,
            })
        );
        assert_eq!(c["exploits"].as_array().unwrap().len(), 10);
        assert_eq!(
            c["exploits"][1],
            json!({
                "key": "episodes_enchaines",
                "label": "Épisodes enchaînés",
                "condition": "au moins « paramètre » épisodes vus dans une même session",
                "domain": "entracte",
                "unit": "count",
                "default_parameter": 5,
            })
        );
    }

    #[tokio::test]
    async fn c7_trophee_cree_par_l_api_se_debloque_selon_les_regles_existantes() {
        let Some(e) = env().await else { return };
        let e = &e;
        let r = e
            .appel(
                Some("admin"),
                "POST",
                "/admin/trophies",
                Some(json!({
                    "key": "premiers_mots",
                    "name": "Premiers mots",
                    "description": "Des messages envoyés.",
                    "icon": "bulle",
                    "kind": "tiers",
                    "stat": "messaging.messages",
                    "period": "all",
                    "thresholds": [2, 4],
                })),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CREATED, "{}", r.texte());
        let message = |n: usize, date: &'static str| async move {
            let donnees = Donnees::EstompeMessage {
                serveur: None,
                salon: None,
                prive: true,
            };
            e.recevoir(&format!("message-{n}"), ALICE, date, false, donnees)
                .await
        };
        let etat = || async {
            let r = e.get("alice", "/trophies").await.json();
            (
                trouve(&r, "unlocked", "premiers_mots"),
                trouve(&r, "in_progress", "premiers_mots"),
            )
        };
        let (debloque, en_cours) = etat().await;
        assert_eq!(debloque, None);
        let en_cours = en_cours.unwrap();
        assert_eq!(
            (
                en_cours["tier"].clone(),
                en_cours["value"].clone(),
                en_cours["next_threshold"].clone()
            ),
            (json!(0), json!(0), json!(2))
        );
        message(1, "2026-03-01T10:00:00Z").await;
        assert_eq!(etat().await.0, None);
        message(2, "2026-03-02T10:00:00Z").await;
        message(3, "2026-03-03T10:00:00Z").await;
        let (debloque, en_cours) = etat().await;
        let debloque = debloque.unwrap();
        assert_eq!(
            (
                debloque["tier"].clone(),
                debloque["tiers"].clone(),
                debloque["unlocked_at"].clone(),
                debloque["seen"].clone()
            ),
            (json!(1), json!(2), json!("2026-03-02"), json!(false))
        );
        assert_eq!(en_cours.unwrap()["value"], 3);
        message(4, "2026-03-04T10:00:00Z").await;
        let (debloque, en_cours) = etat().await;
        let debloque = debloque.unwrap();
        assert_eq!(
            (debloque["tier"].clone(), debloque["unlocked_at"].clone()),
            (json!(2), json!("2026-03-04"))
        );
        assert_eq!(en_cours, None);
        assert_eq!(
            trouve(
                &e.get("bruno", "/trophies").await.json(),
                "unlocked",
                "premiers_mots"
            ),
            None
        );
    }
}
