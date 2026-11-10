use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::{
    AppState,
    moteur::{Brute, Effectifs, Effet, Regle, Source, appliquer_portee},
};

const SQL_REGLES: &str = r#"
WITH grp AS (
    SELECT g.id, g.name, m.expires_at
    FROM group_members m JOIN groups g ON g.id = m.group_id
    WHERE m.sub = $1 AND (m.expires_at IS NULL OR m.expires_at > now())
), liaisons AS (
    SELECT b.id::text AS liaison, b.role_id, b.scope_type, b.scope_id, b.expires_at AS liaison_exp,
           NULL::text AS groupe_id, NULL::text AS groupe_nom, NULL::timestamptz AS groupe_exp
    FROM role_bindings b
    WHERE b.subject_kind = 'account' AND b.subject_id = $1 AND (b.expires_at IS NULL OR b.expires_at > now())
    UNION ALL
    SELECT b.id::text, b.role_id, b.scope_type, b.scope_id, b.expires_at, grp.id::text, grp.name, grp.expires_at
    FROM role_bindings b JOIN grp ON b.subject_kind = 'group' AND b.subject_id = grp.id::text
    WHERE b.expires_at IS NULL OR b.expires_at > now()
    UNION ALL
    SELECT NULL, r.id, NULL, NULL, NULL, NULL, NULL, NULL FROM roles r WHERE r.system = 'tout-le-monde'
)
SELECT 'role' AS genre, l.liaison, ro.id::text AS role_id, ro.name AS role_nom, ro.system,
       l.scope_type, l.scope_id, l.groupe_id, l.groupe_nom,
       ru.id::text AS regle_id, ru.permission, ru.resource, ru.effect,
       ru.expires_at AS regle_exp, l.liaison_exp, l.groupe_exp,
       p.app || ':' || p.resource_type AS type_ressource
FROM liaisons l
JOIN roles ro ON ro.id = l.role_id
JOIN rules ru ON ru.subject_kind = 'role' AND ru.subject_id = ro.id::text
JOIN permissions p ON p.name = ru.permission
WHERE p.obsolete_at IS NULL AND (ru.expires_at IS NULL OR ru.expires_at > now())
UNION ALL
SELECT 'groupe', NULL, NULL, NULL, NULL, NULL, NULL, grp.id::text, grp.name,
       ru.id::text, ru.permission, ru.resource, ru.effect, ru.expires_at, NULL, grp.expires_at,
       p.app || ':' || p.resource_type
FROM grp
JOIN rules ru ON ru.subject_kind = 'group' AND ru.subject_id = grp.id::text
JOIN permissions p ON p.name = ru.permission
WHERE p.obsolete_at IS NULL AND (ru.expires_at IS NULL OR ru.expires_at > now())
UNION ALL
SELECT 'compte', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       ru.id::text, ru.permission, ru.resource, ru.effect, ru.expires_at, NULL, NULL,
       p.app || ':' || p.resource_type
FROM rules ru
JOIN permissions p ON p.name = ru.permission
WHERE ru.subject_kind = 'account' AND ru.subject_id = $1
  AND p.obsolete_at IS NULL AND (ru.expires_at IS NULL OR ru.expires_at > now())
"#;

fn plus_tot(dates: [Option<DateTime<Utc>>; 3]) -> Option<DateTime<Utc>> {
    dates.into_iter().flatten().min()
}

pub async fn regles(db: &PgPool, sub: &str) -> sqlx::Result<Vec<Regle>> {
    let lignes = sqlx::query(SQL_REGLES).bind(sub).fetch_all(db).await?;
    let mut brutes = Vec::with_capacity(lignes.len());
    for l in lignes {
        let genre: String = l.try_get("genre")?;
        let source = match genre.as_str() {
            "role" => {
                let system: Option<String> = l.try_get("system")?;
                let role_id: String = l.try_get("role_id")?;
                if system.as_deref() == Some("tout-le-monde") {
                    Source::ToutLeMonde { role_id }
                } else {
                    Source::Role {
                        role_id,
                        role: l.try_get("role_nom")?,
                        binding_id: l
                            .try_get::<Option<String>, _>("liaison")?
                            .unwrap_or_default(),
                        group_id: l.try_get("groupe_id")?,
                        group: l.try_get("groupe_nom")?,
                        scope_type: l.try_get("scope_type")?,
                        scope_id: l.try_get("scope_id")?,
                    }
                }
            }
            "groupe" => Source::Groupe {
                group_id: l
                    .try_get::<Option<String>, _>("groupe_id")?
                    .unwrap_or_default(),
                group: l
                    .try_get::<Option<String>, _>("groupe_nom")?
                    .unwrap_or_default(),
            },
            _ => Source::Compte,
        };
        let effet: String = l.try_get("effect")?;
        brutes.push(Brute {
            regle_id: l.try_get("regle_id")?,
            permission: l.try_get("permission")?,
            ressource: l.try_get("resource")?,
            effet: Effet::depuis(&effet).unwrap_or(Effet::Deny),
            type_ressource: l.try_get("type_ressource")?,
            expire: plus_tot([
                l.try_get("regle_exp")?,
                l.try_get("liaison_exp")?,
                l.try_get("groupe_exp")?,
            ]),
            source,
        });
    }
    Ok(appliquer_portee(brutes))
}

pub async fn effectifs(state: &AppState, sub: &str) -> sqlx::Result<Effectifs> {
    let version = state.cache.version(sub).await;
    if let Some(v) = version
        && let Some(e) = state.cache.lire(v, sub).await
    {
        return Ok(e);
    }
    let r = regles(&state.db, sub).await?;
    let generation = version.map_or(0, |v| v.generation);
    let e = Effectifs::calculer(sub, generation, &r, Utc::now(), state.cache.ttl());
    if let Some(v) = version {
        state.cache.ecrire(v, &e).await;
    }
    Ok(e)
}
