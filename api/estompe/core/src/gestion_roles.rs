use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::Response,
    routing::{get, patch, post, put},
};
use percent_encoding::percent_decode_str;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    audit::{self, Filtre},
    http::{champ_texte, decode, write_err, write_json},
    openapi,
    roles::{self, COLONNES_ROLE, LigneRole, Perm},
    servers::{
        MAX_ROLES, Params, acces, bd, booleen, evenement_serveur, id_salon, liste, mauvais,
        nom_propre, non_trouve, param, pool, sans_contenu, unique, uuid_valide,
    },
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/servers/{id}/roles", post(creer))
        .route("/servers/{id}/roles-order", put(ordonner))
        .route(
            "/servers/{id}/roles/{rid}",
            patch(modifier).delete(supprimer),
        )
        .route(
            "/servers/{id}/members/{uid}/roles/{rid}",
            put(donner_role).delete(retirer_role),
        )
        .route(
            "/servers/{id}/channels/{cid}/overrides",
            get(lister_surcharges),
        )
        .route(
            "/servers/{id}/channels/{cid}/overrides/{rid}",
            put(definir_surcharge),
        )
        .route("/servers/{id}/audit-log", get(journal));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("serveurs", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Post,
        "/servers/{id}/roles",
        "Créer un rôle, placé juste au-dessus de « @tout le monde » (permission `manage_roles` ; 50 max). On n'accorde que des permissions qu'on a soi-même.",
        &[
            ("201", "Créé"),
            (
                "400",
                "`invalid_role` (nom, couleur, icône ou permission inconnue)",
            ),
            ("403", "Refusé"),
            ("409", "`name_taken` ou `too_many_roles`"),
        ],
    ),
    (
        HttpMethod::Patch,
        "/servers/{id}/roles/{rid}",
        "Modifier un rôle situé sous son rôle le plus élevé (seules les permissions de « @tout le monde » changent)",
        &[
            ("200", "ok"),
            ("400", "invalid_role"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "name_taken"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/roles/{rid}",
        "Supprimer un rôle situé sous son rôle le plus élevé",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            (
                "409",
                "`default_role` : « @tout le monde » ne se supprime pas",
            ),
        ],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/roles-order",
        "Réordonner les rôles (permission `manage_roles`) ; les rôles au niveau du vôtre ou au-dessus ne bougent pas",
        &[("204", "ok"), ("400", "invalid_order"), ("403", "Refusé")],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/members/{uid}/roles/{rid}",
        "Donner un rôle à un membre (permission `manage_roles` ; rôle et membre sous son rôle le plus élevé)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("409", "default_role"),
        ],
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/members/{uid}/roles/{rid}",
        "Retirer un rôle à un membre (mêmes règles)",
        &[
            ("204", "ok"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/channels/{cid}/overrides",
        "Exceptions de permissions du salon par rôle (permission `manage_channels`)",
        &[("200", "ok"), ("403", "Refusé")],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/channels/{cid}/overrides/{rid}",
        "Autoriser ou refuser des permissions à un rôle sur ce salon ; deux listes vides = hériter (permission `manage_channels`, et on n'y met que des permissions qu'on a)",
        &[
            ("204", "ok"),
            ("400", "invalid_override"),
            ("403", "Refusé"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
        ],
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/audit-log",
        "Journal d'administration et de modération — rôles, expulsions, bannissements, exclusions, avertissements, déplacements vocaux, suppressions de messages ; 200 dernières actions (permission `view_audit_log`)",
        &[("200", "ok"), ("403", "Refusé")],
    ),
];

enum Echec {
    NonTrouve,
    NomPris,
    TropDeRoles,
    OrdreInvalide,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Base(e)
    }
}

fn erreur(e: Echec) -> Response {
    match e {
        Echec::NomPris => write_err(
            StatusCode::CONFLICT,
            "name_taken",
            "un rôle porte déjà ce nom",
        ),
        Echec::TropDeRoles => write_err(
            StatusCode::CONFLICT,
            "too_many_roles",
            "50 rôles maximum par serveur",
        ),
        Echec::NonTrouve => non_trouve("introuvable"),
        Echec::OrdreInvalide => write_err(
            StatusCode::BAD_REQUEST,
            "invalid_order",
            "la liste doit contenir chaque rôle une fois",
        ),
        Echec::Base(e) => bd(e),
    }
}

struct EntreeRole {
    name: String,
    color: String,
    icon: String,
    hoist: bool,
    permissions: Perm,
}

impl EntreeRole {
    fn lire(e: &Map<String, Value>) -> Result<Self, Response> {
        Ok(EntreeRole {
            name: champ_texte(e, "name").ok_or_else(mauvais)?,
            color: champ_texte(e, "color").ok_or_else(mauvais)?,
            icon: champ_texte(e, "icon").ok_or_else(mauvais)?,
            hoist: booleen(e, "hoist").ok_or_else(mauvais)?,
            permissions: permissions(e, "permissions")?,
        })
    }

    fn valide(&mut self) -> bool {
        let Some(nom) = nom_propre(&self.name, 30) else {
            return false;
        };
        self.name = nom;
        let couleur = self.color.is_empty()
            || (self.color.len() == 7
                && self.color.starts_with('#')
                && self.color[1..].bytes().all(|b| b.is_ascii_hexdigit()));
        couleur && self.icon.chars().count() <= 8 && !self.icon.chars().any(char::is_control)
    }
}

fn permissions(e: &Map<String, Value>, nom: &str) -> Result<Perm, Response> {
    liste(e, nom)
        .and_then(|l| roles::parse(&l))
        .ok_or_else(mauvais)
}

fn role_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_role",
        "nom : 1 à 30 caractères ; couleur : #rrggbb ; icône : 8 caractères au plus",
    )
}

fn role_au_dessus() -> Response {
    write_err(
        StatusCode::FORBIDDEN,
        "forbidden",
        "ce rôle est au-dessus du vôtre",
    )
}

pub(crate) async fn signaler(state: &AppState, pool: &PgPool, serveur: &str, membre: &str) {
    let charge = if membre.is_empty() {
        json!({ "server_id": serveur })
    } else {
        json!({ "server_id": serveur, "user_id": membre })
    };
    evenement_serveur(state, pool, serveur, "roles.updated", &charge).await;
    state.droits_modifies(serveur, membre).await;
}

async fn libelle(pool: &PgPool, requete: &'static str, id: &str) -> Option<String> {
    match sqlx::query_scalar::<_, String>(requete)
        .bind(id)
        .fetch_optional(pool)
        .await
    {
        Ok(v) => Some(v.unwrap_or_default()),
        Err(e) => {
            tracing::warn!(error = %e, "lecture du libellé d'audit");
            None
        }
    }
}

fn cle_etrangere(e: sqlx::Error) -> Echec {
    match &e {
        sqlx::Error::Database(d) if d.code().as_deref() == Some("23503") => Echec::NonTrouve,
        _ => e.into(),
    }
}

async fn role_pour(pool: &PgPool, serveur: &str, rid: &str) -> Result<LigneRole, Response> {
    let introuvable = || non_trouve("rôle introuvable");
    if !uuid_valide(rid) {
        return Err(introuvable());
    }
    sqlx::query_as::<_, LigneRole>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ROLE} FROM roles WHERE id=$2::uuid AND server_id=$1::uuid"
    )))
    .bind(serveur)
    .bind(rid)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)
}

async fn creer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_ROLES).await?;
    let mut entree = EntreeRole::lire(&decode(body).await?)?;
    if !entree.valide() {
        return Ok(role_invalide());
    }
    if !roles::can_grant(acc.base(), entree.permissions) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez accorder que des permissions que vous avez",
        ));
    }
    let ligne = creer_role(pool, &sv.id, &entree).await.map_err(erreur)?;
    audit::audit(pool, &sv.id, &u.id, "role.create", &ligne.name).await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(write_json(StatusCode::CREATED, &ligne.json()))
}

async fn creer_role(pool: &PgPool, serveur: &str, r: &EntreeRole) -> Result<LigneRole, Echec> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM servers WHERE id=$1::uuid FOR UPDATE")
        .bind(serveur)
        .execute(&mut *tx)
        .await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM roles WHERE server_id=$1::uuid")
        .bind(serveur)
        .fetch_one(&mut *tx)
        .await?;
    if n > MAX_ROLES as i64 {
        return Err(Echec::TropDeRoles);
    }
    sqlx::query(
        "UPDATE roles SET position = position + 1 WHERE server_id=$1::uuid AND NOT is_default",
    )
    .bind(serveur)
    .execute(&mut *tx)
    .await?;
    let ligne = sqlx::query_as::<_, LigneRole>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO roles (server_id, name, color, icon, hoist, permissions, position) VALUES ($1::uuid,$2,$3,$4,$5,$6,1)
         RETURNING {COLONNES_ROLE}"
    )))
    .bind(serveur)
    .bind(&r.name)
    .bind(&r.color)
    .bind(&r.icon)
    .bind(r.hoist)
    .bind(r.permissions as i64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| if unique(&e) { Echec::NomPris } else { e.into() })?;
    tx.commit().await?;
    Ok(ligne)
}

async fn modifier(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_ROLES).await?;
    let mut ligne = role_pour(pool, &sv.id, param(&p, "rid")).await?;
    let mut entree = EntreeRole::lire(&decode(body).await?)?;
    if ligne.is_default {
        entree.name = ligne.name.clone();
        entree.color = ligne.color.clone();
        entree.icon = ligne.icon.clone();
        entree.hoist = ligne.hoist;
    } else if !roles::can_manage_role(acc.top(), ligne.position) {
        return Ok(role_au_dessus());
    }
    if !entree.valide() {
        return Ok(role_invalide());
    }
    if !roles::can_grant(acc.base(), entree.permissions ^ ligne.permissions as Perm) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez modifier que des permissions que vous avez",
        ));
    }
    ligne.name = entree.name.clone();
    ligne.color = entree.color.clone();
    ligne.icon = entree.icon.clone();
    ligne.hoist = entree.hoist;
    ligne.permissions = entree.permissions as i64;
    maj_role(pool, &sv.id, &ligne.id, &entree)
        .await
        .map_err(erreur)?;
    audit::audit(pool, &sv.id, &u.id, "role.update", &ligne.name).await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(write_json(StatusCode::OK, &ligne.json()))
}

async fn maj_role(pool: &PgPool, serveur: &str, id: &str, r: &EntreeRole) -> Result<(), Echec> {
    let res = sqlx::query(
        "UPDATE roles SET name=$3, color=$4, icon=$5, hoist=$6, permissions=$7 WHERE id=$2::uuid AND server_id=$1::uuid",
    )
    .bind(serveur)
    .bind(id)
    .bind(&r.name)
    .bind(&r.color)
    .bind(&r.icon)
    .bind(r.hoist)
    .bind(r.permissions as i64)
    .execute(pool)
    .await
    .map_err(|e| if unique(&e) { Echec::NomPris } else { e.into() })?;
    if res.rows_affected() == 0 {
        return Err(Echec::NonTrouve);
    }
    Ok(())
}

async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_ROLES).await?;
    let ligne = role_pour(pool, &sv.id, param(&p, "rid")).await?;
    if ligne.is_default {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "default_role",
            "« @tout le monde » ne peut pas être supprimé",
        ));
    }
    if !roles::can_manage_role(acc.top(), ligne.position) {
        return Ok(role_au_dessus());
    }
    supprimer_role(pool, &sv.id, &ligne.id)
        .await
        .map_err(erreur)?;
    audit::audit(pool, &sv.id, &u.id, "role.delete", &ligne.name).await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(sans_contenu())
}

async fn supprimer_role(pool: &PgPool, serveur: &str, id: &str) -> Result<(), Echec> {
    let mut tx = pool.begin().await?;
    let position: i32 = sqlx::query_scalar(
        "DELETE FROM roles WHERE id=$2::uuid AND server_id=$1::uuid AND NOT is_default RETURNING position",
    )
    .bind(serveur)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Echec::NonTrouve)?;
    sqlx::query(
        "UPDATE roles SET position = position - 1 WHERE server_id=$1::uuid AND position > $2",
    )
    .bind(serveur)
    .bind(position)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn ordonner(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_ROLES).await?;
    let ids = liste(&decode(body).await?, "ids").ok_or_else(mauvais)?;
    let actuels = sqlx::query_as::<_, (String, i32, bool)>(
        "SELECT id::text, position, is_default FROM roles WHERE server_id=$1::uuid ORDER BY position DESC",
    )
    .bind(&sv.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    for (i, id) in ids.iter().enumerate() {
        let bouge = actuels.iter().any(|(rid, position, defaut)| {
            rid == id
                && !defaut
                && *position >= acc.top()
                && *position as i64 != (ids.len() - i) as i64
        });
        if bouge {
            return Ok(write_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "vous ne pouvez pas déplacer un rôle au niveau du vôtre ou au-dessus",
            ));
        }
    }
    ordonner_roles(pool, &sv.id, &ids).await.map_err(erreur)?;
    audit::audit(pool, &sv.id, &u.id, "role.reorder", "").await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(sans_contenu())
}

async fn ordonner_roles(pool: &PgPool, serveur: &str, ids: &[String]) -> Result<(), Echec> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM servers WHERE id=$1::uuid FOR UPDATE")
        .bind(serveur)
        .execute(&mut *tx)
        .await?;
    let (n, correspondants): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE id::text = ANY($2)) FROM roles WHERE server_id=$1::uuid AND NOT is_default",
    )
    .bind(serveur)
    .bind(ids)
    .fetch_one(&mut *tx)
    .await?;
    if n != ids.len() as i64 || correspondants != n {
        return Err(Echec::OrdreInvalide);
    }
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE roles SET position=$2 WHERE id=$1::uuid")
            .bind(id)
            .bind((ids.len() - i) as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn donner_role(
    state: State<AppState>,
    u: ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    role_membre(state, u, p, true).await
}

async fn retirer_role(
    state: State<AppState>,
    u: ActiveUser,
    p: Params,
) -> Result<Response, Response> {
    role_membre(state, u, p, false).await
}

async fn role_membre(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    on: bool,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_ROLES).await?;
    let ligne = role_pour(pool, &sv.id, param(&p, "rid")).await?;
    if ligne.is_default {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "default_role",
            "« @tout le monde » s'applique à tous les membres",
        ));
    }
    let uid = param(&p, "uid");
    if !uuid_valide(uid) {
        return Ok(non_trouve("membre introuvable"));
    }
    let Some(cible) = state.acces_serveur(&sv.id, uid).await.map_err(bd)? else {
        return Ok(erreur(Echec::NonTrouve));
    };
    let proprietaire_lui_meme = uid == u.id && acc.owner;
    if !roles::can_manage_role(acc.top(), ligne.position)
        || (!proprietaire_lui_meme && !roles::can_manage_role(acc.top(), cible.top()))
    {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "ce rôle ou ce membre est au-dessus de votre rôle",
        ));
    }
    if changer_role(pool, &sv.id, uid, &ligne.id, on)
        .await
        .map_err(erreur)?
    {
        let action = if on { "role.assign" } else { "role.unassign" };
        let mut cible_nom = ligne.name.clone();
        if let Some(pseudo) = libelle(pool, "SELECT pseudo FROM users WHERE id=$1::uuid", uid).await
        {
            cible_nom = format!("{cible_nom} → {pseudo}");
        }
        audit::audit(pool, &sv.id, &u.id, action, &cible_nom).await;
        signaler(&state, pool, &sv.id, uid).await;
    }
    Ok(sans_contenu())
}

async fn changer_role(
    pool: &PgPool,
    serveur: &str,
    membre: &str,
    role: &str,
    on: bool,
) -> Result<bool, Echec> {
    let res = if on {
        sqlx::query(
            "INSERT INTO member_roles (server_id, user_id, role_id) SELECT $1::uuid,$2::uuid,id FROM roles WHERE id=$3::uuid AND server_id=$1::uuid AND NOT is_default
             ON CONFLICT DO NOTHING",
        )
    } else {
        sqlx::query(
            "DELETE FROM member_roles WHERE server_id=$1::uuid AND user_id=$2::uuid AND role_id=$3::uuid",
        )
    }
    .bind(serveur)
    .bind(membre)
    .bind(role)
    .execute(pool)
    .await
    .map_err(cle_etrangere)?;
    Ok(res.rows_affected() == 1)
}

async fn lister_surcharges(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let l = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT o.role_id::text, o.allow, o.deny FROM channel_overrides o JOIN channels c ON c.id = o.channel_id
         WHERE c.server_id=$1::uuid AND c.id=$2::uuid",
    )
    .bind(&sv.id)
    .bind(&cid)
    .fetch_all(pool)
    .await
    .map_err(bd)?
    .into_iter()
    .map(|(role_id, allow, deny)| {
        json!({
            "role_id": role_id,
            "allow": roles::cles(allow as Perm),
            "deny": roles::cles(deny as Perm),
        })
    })
    .collect::<Vec<_>>();
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

async fn definir_surcharge(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, acc) = acces(&state, &u, param(&p, "id"), roles::MANAGE_CHANNELS).await?;
    let cid = id_salon(&p)?;
    let rid = param(&p, "rid");
    let ligne = role_pour(pool, &sv.id, rid).await?;
    if !ligne.is_default && !roles::can_manage_role(acc.top(), ligne.position) {
        return Ok(role_au_dessus());
    }
    let e = decode(body).await?;
    let allow = permissions(&e, "allow")?;
    let deny = permissions(&e, "deny")?;
    if allow & deny != 0 {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_override",
            "une permission ne peut pas être à la fois autorisée et refusée",
        ));
    }
    if !roles::can_grant(acc.base(), allow | deny) {
        return Ok(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "vous ne pouvez modifier que des permissions que vous avez",
        ));
    }
    enregistrer_surcharge(pool, &sv.id, &cid, rid, allow, deny)
        .await
        .map_err(erreur)?;
    let salon = libelle(pool, "SELECT name FROM channels WHERE id=$1::uuid", &cid)
        .await
        .unwrap_or_default();
    audit::audit(pool, &sv.id, &u.id, "channel.override", &salon).await;
    signaler(&state, pool, &sv.id, "").await;
    Ok(sans_contenu())
}

async fn enregistrer_surcharge(
    pool: &PgPool,
    serveur: &str,
    salon: &str,
    role: &str,
    allow: Perm,
    deny: Perm,
) -> Result<(), Echec> {
    let existe: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM channels c JOIN roles r ON r.server_id = c.server_id WHERE c.id=$2::uuid AND r.id=$3::uuid AND c.server_id=$1::uuid)",
    )
    .bind(serveur)
    .bind(salon)
    .bind(role)
    .fetch_one(pool)
    .await?;
    if !existe {
        return Err(Echec::NonTrouve);
    }
    if allow == 0 && deny == 0 {
        sqlx::query("DELETE FROM channel_overrides WHERE channel_id=$1::uuid AND role_id=$2::uuid")
            .bind(salon)
            .bind(role)
            .execute(pool)
            .await
            .map_err(cle_etrangere)?;
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO channel_overrides (channel_id, role_id, allow, deny) VALUES ($1::uuid,$2::uuid,$3,$4)
         ON CONFLICT (channel_id, role_id) DO UPDATE SET allow = EXCLUDED.allow, deny = EXCLUDED.deny",
    )
    .bind(salon)
    .bind(role)
    .bind(allow as i64)
    .bind(deny as i64)
    .execute(pool)
    .await
    .map_err(cle_etrangere)?;
    Ok(())
}

pub(crate) fn premier_parametre(requete: &str, nom: &str) -> String {
    requete
        .split('&')
        .filter_map(|paire| paire.split_once('=').or(Some((paire, ""))))
        .find_map(|(k, v)| {
            let decode = |s: &str| {
                percent_decode_str(&s.replace('+', " "))
                    .decode_utf8_lossy()
                    .into_owned()
            };
            (decode(k) == nom).then(|| decode(v))
        })
        .unwrap_or_default()
}

async fn journal(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), roles::VIEW_AUDIT_LOG).await?;
    let requete = requete.unwrap_or_default();
    let filtre = Filtre {
        acteur: premier_parametre(&requete, "actor"),
        cible: premier_parametre(&requete, "target"),
        action: premier_parametre(&requete, "action"),
    };
    let l = audit::journal(pool, &sv.id, &filtre).await.map_err(bd)?;
    Ok(write_json(StatusCode::OK, &Value::Array(l)))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{
        identity::Cred,
        servers::tests::{Env, appel, env, invitation_de, membre, serveur},
    };

    async fn rejoint(e: &Env, nom: &str, sid: &str) -> String {
        let id = membre(e, nom).await;
        let code = invitation_de(e, "alice", sid, "").await;
        let (s, _) = appel(
            e,
            nom,
            "POST",
            &format!("/server-invites/{code}/join"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        id
    }

    async fn nouveau_role(e: &Env, qui: &str, sid: &str, nom: &str, perms: &[&str]) -> String {
        let corps = json!({ "name": nom, "permissions": perms }).to_string();
        let (s, v) = appel(
            e,
            qui,
            "POST",
            &format!("/servers/{sid}/roles"),
            Some(&corps),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    async fn code(e: &Env, qui: &str, methode: &str, chemin: &str, corps: &str) -> (u16, String) {
        let corps = (!corps.is_empty()).then_some(corps);
        let (s, v) = appel(e, qui, methode, chemin, corps).await;
        (s.as_u16(), v["error"].as_str().unwrap_or("").to_string())
    }

    struct Cadre {
        e: Env,
        sid: String,
        base: String,
        roles: String,
        tous: String,
        moderateur: String,
        ami: String,
    }

    async fn cadre() -> Option<Cadre> {
        let e = env().await?;
        membre(&e, "alice").await;
        let sid = serveur(&e, "Rôles", "alice").await;
        let moderateur = rejoint(&e, "mod", &sid).await;
        let ami = rejoint(&e, "ami", &sid).await;
        let base = format!("/servers/{sid}");
        let (_, v) = appel(&e, "ami", "GET", &base, None).await;
        let tous = v["roles"][0]["id"].as_str().unwrap().to_string();
        let roles = format!("{base}/roles");
        Some(Cadre {
            e,
            sid,
            base,
            roles,
            tous,
            moderateur,
            ami,
        })
    }

    #[tokio::test]
    async fn creation_et_validation_des_roles() {
        let Some(Cadre {
            e,
            sid,
            base,
            roles,
            tous,
            ..
        }) = cadre().await
        else {
            return;
        };
        let (_, v) = appel(&e, "ami", "GET", &base, None).await;
        assert_eq!(v["roles"].as_array().unwrap().len(), 1);
        assert_eq!(v["roles"][0]["name"], "@tout le monde");
        assert_eq!(
            code(
                &e,
                "ami",
                "POST",
                &format!("{base}/roles"),
                r#"{"name":"x"}"#
            )
            .await
            .0,
            403
        );
        assert_eq!(
            code(&e, "alice", "DELETE", &format!("{base}/roles/{tous}"), "").await,
            (409, "default_role".into())
        );

        nouveau_role(&e, "alice", &sid, "Admin", &["manage_roles"]).await;
        nouveau_role(&e, "alice", &sid, "Ami", &["send_messages"]).await;
        assert_eq!(
            code(&e, "alice", "POST", &roles, r#"{"name":"ami"}"#).await,
            (409, "name_taken".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                &roles,
                r#"{"name":"Mauve","color":"violet"}"#
            )
            .await,
            (400, "invalid_role".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                &roles,
                r#"{"name":"Icône","icon":"123456789"}"#
            )
            .await,
            (400, "invalid_role".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                &roles,
                r#"{"name":"Inconnu","permissions":["vol"]}"#
            )
            .await,
            (400, "bad_request".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "POST",
                &roles,
                r##"{"name":"Couleur","color":"#a1B2c3","icon":"éé"}"##
            )
            .await
            .0,
            201
        );
    }

    #[tokio::test]
    async fn ordre_et_suppression_des_roles() {
        let Some(Cadre {
            e,
            sid,
            base,
            roles,
            tous,
            ..
        }) = cadre().await
        else {
            return;
        };
        let haut = nouveau_role(
            &e,
            "alice",
            &sid,
            "Admin",
            &["manage_roles", "manage_messages", "mute_members"],
        )
        .await;
        let bas = nouveau_role(&e, "alice", &sid, "Ami", &["send_messages"]).await;
        let couleur = nouveau_role(&e, "alice", &sid, "Couleur", &[]).await;
        let ordre = format!("{base}/roles-order");
        let ids = |l: &[&str]| json!({ "ids": l }).to_string();
        assert_eq!(
            code(&e, "alice", "PUT", &ordre, &ids(&[&couleur, &bas, &haut]))
                .await
                .0,
            204
        );
        assert_eq!(
            code(&e, "alice", "PUT", &ordre, &ids(&[&couleur, &haut, &bas]))
                .await
                .0,
            204
        );
        for mauvais in [
            ids(&[&haut]),
            ids(&[&haut, &haut, &bas]),
            ids(&[&couleur, &haut, "pas-un-uuid"]),
            ids(&[&couleur, &haut, &bas, &tous]),
        ] {
            assert_eq!(
                code(&e, "alice", "PUT", &ordre, &mauvais).await,
                (400, "invalid_order".into()),
                "{mauvais}"
            );
        }
        assert_eq!(
            code(&e, "alice", "PUT", &ordre, "[1]").await,
            (400, "bad_request".into())
        );
        assert_eq!(
            code(&e, "alice", "DELETE", &format!("{roles}/{couleur}"), "")
                .await
                .0,
            204
        );
        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let positions: Vec<_> = v["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r["name"].as_str().unwrap(), r["position"].as_i64().unwrap()))
            .collect();
        assert_eq!(positions, [("Admin", 2), ("Ami", 1), ("@tout le monde", 0)]);
    }

    #[tokio::test]
    async fn attribution_des_roles() {
        let Some(Cadre {
            e,
            sid,
            base,
            tous,
            moderateur,
            ..
        }) = cadre().await
        else {
            return;
        };
        let haut = nouveau_role(
            &e,
            "alice",
            &sid,
            "Admin",
            &["manage_roles", "manage_messages", "mute_members"],
        )
        .await;
        let donner = |m: &str, r: &str| format!("{base}/members/{m}/roles/{r}");
        assert_eq!(
            code(&e, "alice", "PUT", &donner(&moderateur, &haut), "")
                .await
                .0,
            204
        );
        assert_eq!(
            code(&e, "alice", "PUT", &donner(&moderateur, &tous), "").await,
            (409, "default_role".into())
        );
        assert_eq!(
            code(&e, "alice", "PUT", &donner("pas-un-uuid", &haut), "")
                .await
                .0,
            404
        );
        assert_eq!(
            code(&e, "alice", "PUT", &donner(&moderateur, "pas-un-uuid"), "")
                .await
                .0,
            404
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &donner("0123abcd-0123-4567-89ab-0123456789ab", &haut),
                ""
            )
            .await
            .0,
            404
        );
    }

    #[tokio::test]
    async fn hierarchie_des_roles() {
        let Some(Cadre {
            e,
            sid,
            base,
            roles,
            tous,
            moderateur,
            ami,
            ..
        }) = cadre().await
        else {
            return;
        };
        let haut = nouveau_role(
            &e,
            "alice",
            &sid,
            "Admin",
            &["manage_roles", "manage_messages", "mute_members"],
        )
        .await;
        let bas = nouveau_role(&e, "alice", &sid, "Ami", &["send_messages"]).await;
        let ordre = format!("{base}/roles-order");
        let ids = |l: &[&str]| json!({ "ids": l }).to_string();
        let donner = |m: &str, r: &str| format!("{base}/members/{m}/roles/{r}");
        assert_eq!(
            code(&e, "alice", "PUT", &donner(&moderateur, &haut), "")
                .await
                .0,
            204
        );
        let mien = nouveau_role(&e, "mod", &sid, "Sous-modo", &["manage_messages"]).await;
        assert_eq!(
            code(
                &e,
                "mod",
                "POST",
                &roles,
                r#"{"name":"Trop","permissions":["manage_server"]}"#
            )
            .await,
            (403, "forbidden".into())
        );
        let un_de_trop =
            json!({ "name": "Sous-modo", "permissions": ["manage_messages", "kick_members"] })
                .to_string();
        assert_eq!(
            code(&e, "mod", "PATCH", &format!("{roles}/{mien}"), &un_de_trop)
                .await
                .0,
            403
        );
        assert_eq!(
            code(
                &e,
                "mod",
                "PATCH",
                &format!("{roles}/{haut}"),
                r#"{"name":"Admin"}"#
            )
            .await
            .0,
            403
        );
        assert_eq!(
            code(&e, "mod", "DELETE", &format!("{roles}/{haut}"), "")
                .await
                .0,
            403
        );
        assert_eq!(
            code(&e, "mod", "PUT", &donner(&ami, &haut), "").await.0,
            403
        );
        let proprietaire: String =
            sqlx::query_scalar("SELECT owner_id::text FROM servers WHERE id=$1::uuid")
                .bind(&sid)
                .fetch_one(e.state.pool().ok().unwrap())
                .await
                .unwrap();
        assert_eq!(
            code(&e, "mod", "PUT", &donner(&proprietaire, &mien), "")
                .await
                .0,
            403
        );
        assert_eq!(
            code(&e, "mod", "PUT", &ordre, &ids(&[&mien, &haut, &bas]))
                .await
                .0,
            403
        );
        assert_eq!(
            code(&e, "mod", "PUT", &donner(&ami, &mien), "").await.0,
            204
        );
        assert_eq!(
            code(&e, "mod", "PUT", &donner(&ami, &mien), "").await.0,
            204
        );
        assert_eq!(
            code(&e, "mod", "DELETE", &donner(&ami, &mien), "").await.0,
            204
        );
        assert_eq!(
            code(&e, "mod", "DELETE", &format!("{roles}/{mien}"), "")
                .await
                .0,
            204
        );
        assert_eq!(code(&e, "ami", "PUT", &donner(&ami, &bas), "").await.0, 403);

        let (s, v) = appel(
            &e,
            "mod",
            "PATCH",
            &format!("{roles}/{tous}"),
            Some(r#"{"name":"autre","permissions":["view_channel","connect"]}"#),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["name"], "@tout le monde");
        assert_eq!(v["permissions"], json!(["view_channel", "connect"]));
        assert_eq!(
            code(
                &e,
                "alice",
                "PATCH",
                &format!("{roles}/{}", "0123abcd-0123-4567-89ab-0123456789ab"),
                "{}"
            )
            .await
            .0,
            404
        );

        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let de_ami = v["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["user_id"] == ami.as_str() || m["id"] == ami.as_str())
            .unwrap();
        assert_eq!(de_ami["roles"], json!([]));
    }

    #[tokio::test]
    async fn journal_d_audit() {
        let Some(Cadre {
            e,
            sid,
            base,
            roles,
            ami,
            ..
        }) = cadre().await
        else {
            return;
        };
        let journal = nouveau_role(&e, "alice", &sid, "Journal", &["send_messages"]).await;
        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let ids_ordonnables: Vec<&str> = v["roles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["is_default"] == false)
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        let ordre = json!({ "ids": ids_ordonnables }).to_string();
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/roles-order"), &ordre)
                .await
                .0,
            204
        );
        let lien = format!("{base}/members/{ami}/roles/{journal}");
        let role = format!("{roles}/{journal}");
        assert_eq!(
            code(&e, "alice", "PATCH", &role, r#"{"name":"Journal 2"}"#)
                .await
                .0,
            200
        );
        assert_eq!(code(&e, "alice", "PUT", &lien, "").await.0, 204);
        assert_eq!(code(&e, "alice", "DELETE", &lien, "").await.0, 204);
        assert_eq!(code(&e, "alice", "DELETE", &role, "").await.0, 204);
        let (s, v) = appel(&e, "ami", "GET", &format!("{base}/audit-log"), None).await;
        assert_eq!(
            (s, v["error"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden"))
        );
        let (s, v) = appel(&e, "alice", "GET", &format!("{base}/audit-log"), None).await;
        assert_eq!(s, StatusCode::OK);
        let actions: Vec<_> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["action"].as_str().unwrap())
            .collect();
        for attendue in [
            "role.create",
            "role.update",
            "role.delete",
            "role.assign",
            "role.unassign",
            "role.reorder",
        ] {
            assert!(
                actions.contains(&attendue),
                "{attendue} absent de {actions:?}"
            );
        }
        let assign = v
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["action"] == "role.assign")
            .unwrap();
        assert_eq!(assign["target"], "Journal 2 → ami");
        assert_eq!(assign["actor"], "alice");
        assert_eq!(
            code(&e, "alice", "GET", "/servers/pas-un-uuid/audit-log", "")
                .await
                .0,
            404
        );
    }

    #[tokio::test]
    async fn limite_de_cinquante_roles() {
        let Some(e) = env().await else { return };
        membre(&e, "alice").await;
        let sid = serveur(&e, "Limite", "alice").await;
        let roles = format!("/servers/{sid}/roles");
        for i in 0..50 {
            let corps = json!({ "name": format!("r{i}") }).to_string();
            assert_eq!(
                code(&e, "alice", "POST", &roles, &corps).await.0,
                201,
                "{i}"
            );
        }
        assert_eq!(
            code(&e, "alice", "POST", &roles, r#"{"name":"de trop"}"#).await,
            (409, "too_many_roles".into())
        );
    }

    #[tokio::test]
    async fn surcharges_de_salon() {
        let Some(e) = env().await else { return };
        membre(&e, "alice").await;
        let sid = serveur(&e, "Salon réservé", "alice").await;
        let vip = rejoint(&e, "vip", &sid).await;
        rejoint(&e, "autre", &sid).await;
        let role = nouveau_role(&e, "alice", &sid, "VIP", &[]).await;
        let base = format!("/servers/{sid}");
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{base}/members/{vip}/roles/{role}"),
                ""
            )
            .await
            .0,
            204
        );
        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let tous = v["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["is_default"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (s, v) = appel(
            &e,
            "alice",
            "POST",
            &format!("{base}/channels"),
            Some(r#"{"name":"secret","kind":"text"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        let salon = v["id"].as_str().unwrap().to_string();
        let chemin = format!("{base}/channels/{salon}/overrides");

        assert_eq!(code(&e, "autre", "GET", &chemin, "").await.0, 403);
        assert_eq!(
            code(
                &e,
                "autre",
                "PUT",
                &format!("{chemin}/{tous}"),
                r#"{"deny":["view_channel"]}"#
            )
            .await
            .0,
            403
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{chemin}/{tous}"),
                r#"{"deny":["view_channel"],"allow":["view_channel"]}"#
            )
            .await,
            (400, "invalid_override".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{chemin}/{tous}"),
                r#"{"deny":["vol"]}"#
            )
            .await,
            (400, "bad_request".into())
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{chemin}/{tous}"),
                r#"{"deny":["view_channel"]}"#
            )
            .await
            .0,
            204
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{chemin}/{role}"),
                r#"{"allow":["view_channel"]}"#
            )
            .await
            .0,
            204
        );
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{chemin}/pas-un-uuid"), "{}")
                .await
                .0,
            404
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{chemin}/0123abcd-0123-4567-89ab-0123456789ab"),
                "{}"
            )
            .await
            .0,
            404
        );
        let autre_salon = "0123abcd-0123-4567-89ab-0123456789ab";
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{base}/channels/{autre_salon}/overrides/{role}"),
                "{}"
            )
            .await
            .0,
            404
        );
        let (s, v) = appel(
            &e,
            "alice",
            "GET",
            &format!("{base}/channels/{autre_salon}/overrides"),
            None,
        )
        .await;
        assert_eq!((s, v), (StatusCode::OK, json!([])));

        let visible = |qui: &'static str| {
            let (e, base, salon) = (&e, base.clone(), salon.clone());
            async move {
                let (_, v) = appel(e, qui, "GET", &base, None).await;
                v["channels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["id"] == salon.as_str())
            }
        };
        assert!(!visible("autre").await);
        assert!(visible("vip").await);

        let (_, l) = appel(&e, "alice", "GET", &chemin, None).await;
        assert_eq!(l.as_array().unwrap().len(), 2);
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{chemin}/{tous}"), "{}")
                .await
                .0,
            204
        );
        let (_, l) = appel(&e, "alice", "GET", &chemin, None).await;
        assert_eq!(
            l,
            json!([{ "role_id": role, "allow": ["view_channel"], "deny": [] }])
        );
        assert!(visible("autre").await);

        let (_, v) = appel(
            &e,
            "alice",
            "GET",
            &format!("{base}/audit-log?action=channel"),
            None,
        )
        .await;
        assert_eq!(v[0]["action"], "channel.override");
        assert_eq!(v[0]["target"], "secret");
        assert_eq!(v.as_array().unwrap().len(), 3);

        let gerant = rejoint(&e, "gerant", &sid).await;
        let haut = nouveau_role(
            &e,
            "alice",
            &sid,
            "Admin",
            &["manage_server", "manage_roles", "manage_channels"],
        )
        .await;
        let gestion = nouveau_role(
            &e,
            "alice",
            &sid,
            "Gestion",
            &["manage_channels", "view_channel", "send_messages"],
        )
        .await;
        let bas = nouveau_role(&e, "alice", &sid, "Bas", &["send_messages"]).await;
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{base}/members/{gerant}/roles/{gestion}"),
                ""
            )
            .await
            .0,
            204
        );
        let refus = r#"{"deny":["view_channel","send_messages"]}"#;
        for r in [&haut, &gestion, &role] {
            assert_eq!(
                code(&e, "gerant", "PUT", &format!("{chemin}/{r}"), refus).await,
                (403, "forbidden".into()),
                "{r}"
            );
            assert_eq!(
                code(&e, "gerant", "PUT", &format!("{chemin}/{r}"), "{}")
                    .await
                    .0,
                403
            );
        }
        for r in [&bas, &tous] {
            assert_eq!(
                code(&e, "gerant", "PUT", &format!("{chemin}/{r}"), refus)
                    .await
                    .0,
                204,
                "{r}"
            );
        }
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{chemin}/{haut}"), refus)
                .await
                .0,
            204
        );
        let (_, l) = appel(&e, "alice", "GET", &chemin, None).await;
        let ids: Vec<&str> = l
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["role_id"].as_str())
            .collect();
        assert!(!ids.contains(&gestion.as_str()));
        assert!(ids.contains(&bas.as_str()) && ids.contains(&haut.as_str()));
    }

    #[tokio::test]
    async fn evenements_et_rappel_de_droits() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let sid = serveur(&e, "Évènements", "alice").await;
        let bob = rejoint(&e, "bob", &sid).await;
        let appels = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let notes = appels.clone();
        e.state.sur_droits_modifies(move |_, serveur, membre| {
            let notes = notes.clone();
            async move { notes.lock().unwrap().push((serveur, membre)) }
        });
        let (_c, mut rx, _) = e.state.hub().register(
            &alice,
            Cred {
                value: "a".into(),
                cookie: false,
            },
        );
        let base = format!("/servers/{sid}");
        let types = |rx: &mut tokio::sync::mpsc::Receiver<String>| {
            let mut out = Vec::new();
            while let Ok(m) = rx.try_recv() {
                let v: Value = serde_json::from_str(&m).unwrap();
                out.push((v["type"].as_str().unwrap().to_string(), v["data"].clone()));
            }
            out
        };
        let attendu = |n| vec![(sid.clone(), String::new()); n];
        let serveur_seul = (String::from("roles.updated"), json!({ "server_id": sid }));

        let role = nouveau_role(&e, "alice", &sid, "R", &[]).await;
        assert_eq!(types(&mut rx), vec![serveur_seul.clone()]);
        let (s, _) = appel(
            &e,
            "alice",
            "PATCH",
            &format!("{base}/roles/{role}"),
            Some(r#"{"name":"R2"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(types(&mut rx), vec![serveur_seul.clone()]);
        let (s, _) = appel(
            &e,
            "alice",
            "PUT",
            &format!("{base}/roles-order"),
            Some(&json!({ "ids": [role] }).to_string()),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(types(&mut rx), vec![serveur_seul.clone()]);
        let (_, v) = appel(&e, "alice", "GET", &base, None).await;
        let salon = v["channels"][0]["id"].as_str().unwrap().to_string();
        let tous = v["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["is_default"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (s, _) = appel(
            &e,
            "alice",
            "PUT",
            &format!("{base}/channels/{salon}/overrides/{tous}"),
            Some(r#"{"deny":["send_messages"]}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(types(&mut rx), vec![serveur_seul.clone()]);
        assert_eq!(*appels.lock().unwrap(), attendu(4));

        let lien = format!("{base}/members/{bob}/roles/{role}");
        for (methode, change) in [
            ("PUT", true),
            ("PUT", false),
            ("DELETE", true),
            ("DELETE", false),
        ] {
            appels.lock().unwrap().clear();
            let (s, _) = appel(&e, "alice", methode, &lien, None).await;
            assert_eq!(s, StatusCode::NO_CONTENT);
            let recus = types(&mut rx);
            if change {
                assert_eq!(
                    recus,
                    [(
                        "roles.updated".to_string(),
                        json!({ "server_id": sid, "user_id": bob })
                    )]
                );
                assert_eq!(*appels.lock().unwrap(), [(sid.clone(), bob.clone())]);
            } else {
                assert!(recus.is_empty());
                assert!(appels.lock().unwrap().is_empty());
            }
        }

        appels.lock().unwrap().clear();
        let (s, _) = appel(&e, "alice", "DELETE", &format!("{base}/roles/{role}"), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(types(&mut rx), [serveur_seul]);
        assert_eq!(*appels.lock().unwrap(), attendu(1));

        let refus = appel(
            &e,
            "bob",
            "POST",
            &format!("{base}/roles"),
            Some(r#"{"name":"x"}"#),
        )
        .await
        .0;
        assert_eq!(refus, StatusCode::FORBIDDEN);
        assert!(types(&mut rx).is_empty());
        assert_eq!(appels.lock().unwrap().len(), 1);
    }

    #[test]
    fn parametres_de_requete() {
        assert_eq!(
            premier_parametre("actor=a&action=role+x%C3%A9&action=2", "action"),
            "role xé"
        );
        assert_eq!(premier_parametre("actor=a", "target"), "");
        assert_eq!(premier_parametre("", "actor"), "");
    }
}
