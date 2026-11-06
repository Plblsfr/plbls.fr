use sqlx::{PgPool, Row};

pub const VOIR_SALON: i64 = 1 << 8;

pub struct Role {
    pub id: String,
    pub permissions: i64,
    pub defaut: bool,
}

pub struct Exception {
    pub role_id: String,
    pub membre: bool,
    pub autorise: i64,
    pub refuse: i64,
}

pub fn peut_voir(roles: &[Role], exceptions: &[Exception]) -> bool {
    let mut p = roles.iter().fold(0, |acc, r| acc | r.permissions);
    let (mut autorise, mut refuse) = (0, 0);
    for e in exceptions {
        if e.membre {
            autorise |= e.autorise;
            refuse |= e.refuse;
        } else if roles.iter().any(|r| r.defaut && r.id == e.role_id) {
            p = p & !e.refuse | e.autorise;
        } else if roles.iter().any(|r| r.id == e.role_id) {
            autorise |= e.autorise;
            refuse |= e.refuse;
        }
    }
    (p & !refuse | autorise) & VOIR_SALON == VOIR_SALON
}

async fn voit_salon(
    pool: &PgPool,
    serveur: &str,
    salon: &str,
    user: &str,
) -> Result<bool, sqlx::Error> {
    let proprietaire: Option<bool> = sqlx::query_scalar(
        "SELECT s.owner_id = m.user_id FROM servers s JOIN server_members m ON m.server_id = s.id WHERE s.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(serveur)
    .bind(user)
    .fetch_optional(pool)
    .await?;
    match proprietaire {
        None => return Ok(false),
        Some(true) => return Ok(true),
        Some(false) => {}
    }
    let roles = sqlx::query(
        "SELECT id::text, permissions, is_default FROM roles WHERE server_id=$1::uuid AND (is_default OR id IN (SELECT role_id FROM member_roles WHERE user_id=$2::uuid))",
    )
    .bind(serveur)
    .bind(user)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|l| Role {
        id: l.get(0),
        permissions: l.get(1),
        defaut: l.get(2),
    });
    let mut exceptions: Vec<Exception> = sqlx::query(
        "SELECT role_id::text, allow, deny FROM channel_overrides WHERE channel_id=$1::uuid",
    )
    .bind(salon)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|l| Exception {
        role_id: l.get(0),
        membre: false,
        autorise: l.get(1),
        refuse: l.get(2),
    })
    .collect();
    exceptions.extend(
        sqlx::query(
            "SELECT allow, deny FROM channel_member_overrides WHERE channel_id=$1::uuid AND user_id=$2::uuid",
        )
        .bind(salon)
        .bind(user)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|l| Exception {
            role_id: String::new(),
            membre: true,
            autorise: l.get(0),
            refuse: l.get(1),
        }),
    );
    Ok(peut_voir(&roles.collect::<Vec<_>>(), &exceptions))
}

pub async fn voit_message(pool: &PgPool, message: i64, user: &str) -> Result<bool, sqlx::Error> {
    let ligne = sqlx::query(
        "SELECT m.channel_id::text, ch.server_id::text,
            (m.dm_id IS NOT NULL AND EXISTS (SELECT 1 FROM dm_conversations d WHERE d.id = m.dm_id AND (d.user_a=$2::uuid OR d.user_b=$2::uuid)))
         FROM messages m LEFT JOIN channels ch ON ch.id = m.channel_id
         WHERE m.id=$1 AND (
            (m.channel_id IS NOT NULL AND EXISTS (SELECT 1 FROM server_members sm WHERE sm.server_id = ch.server_id AND sm.user_id=$2::uuid))
            OR (m.dm_id IS NOT NULL AND EXISTS (SELECT 1 FROM dm_conversations d WHERE d.id = m.dm_id AND (d.user_a=$2::uuid OR d.user_b=$2::uuid))))",
    )
    .bind(message)
    .bind(user)
    .fetch_optional(pool)
    .await?;
    let Some(ligne) = ligne else {
        return Ok(false);
    };
    let (salon, serveur): (Option<String>, Option<String>) = (ligne.get(0), ligne.get(1));
    match (salon, serveur) {
        (Some(salon), Some(serveur)) => voit_salon(pool, &serveur, &salon, user).await,
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::{Exception, Role, VOIR_SALON, peut_voir};

    const TOUS: i64 = 25_677_568;

    fn role(id: &str, permissions: i64, defaut: bool) -> Role {
        Role {
            id: id.into(),
            permissions,
            defaut,
        }
    }

    fn exc(role_id: &str, membre: bool, autorise: i64, refuse: i64) -> Exception {
        Exception {
            role_id: role_id.into(),
            membre,
            autorise,
            refuse,
        }
    }

    #[test]
    fn sans_exception_la_permission_de_base_decide() {
        assert!(peut_voir(&[role("e", TOUS, true)], &[]));
        assert!(!peut_voir(&[role("e", TOUS & !VOIR_SALON, true)], &[]));
        assert!(!peut_voir(&[], &[]));
    }

    #[test]
    fn exception_de_tout_le_monde_puis_des_roles_du_membre() {
        let roles = [role("e", TOUS, true), role("r", 0, false)];
        assert!(!peut_voir(&roles, &[exc("e", false, 0, VOIR_SALON)]));
        assert!(peut_voir(
            &roles,
            &[
                exc("e", false, 0, VOIR_SALON),
                exc("r", false, VOIR_SALON, 0)
            ]
        ));
        assert!(peut_voir(&roles, &[exc("autre", false, 0, VOIR_SALON)]));
    }

    #[test]
    fn exception_de_membre_prime_sur_tout_le_monde() {
        let roles = [role("e", TOUS, true)];
        assert!(!peut_voir(&roles, &[exc("", true, 0, VOIR_SALON)]));
        let ferme = [role("e", TOUS & !VOIR_SALON, true)];
        assert!(peut_voir(&ferme, &[exc("", true, VOIR_SALON, 0)]));
        assert!(peut_voir(&roles, &[exc("", true, VOIR_SALON, VOIR_SALON)]));
    }
}
