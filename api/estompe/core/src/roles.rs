use std::collections::{HashMap, HashSet};

use sqlx::PgPool;

pub type Perm = u64;

pub const MANAGE_SERVER: Perm = 1 << 0;
pub const MANAGE_ROLES: Perm = 1 << 1;
pub const MANAGE_CHANNELS: Perm = 1 << 2;
pub const CREATE_INVITES: Perm = 1 << 3;
pub const VIEW_AUDIT_LOG: Perm = 1 << 4;
pub const KICK_MEMBERS: Perm = 1 << 5;
pub const BAN_MEMBERS: Perm = 1 << 6;
pub const MANAGE_NICKNAMES: Perm = 1 << 7;
pub const VIEW_CHANNEL: Perm = 1 << 8;
pub const SEND_MESSAGES: Perm = 1 << 9;
pub const ATTACH_FILES: Perm = 1 << 10;
pub const ADD_REACTIONS: Perm = 1 << 11;
pub const MENTION_EVERYONE: Perm = 1 << 12;
pub const MANAGE_MESSAGES: Perm = 1 << 13;
pub const CREATE_POLLS: Perm = 1 << 14;
pub const CONNECT: Perm = 1 << 15;
pub const SPEAK: Perm = 1 << 16;
pub const VIDEO: Perm = 1 << 17;
pub const SHARE_SCREEN: Perm = 1 << 18;
pub const MUTE_MEMBERS: Perm = 1 << 19;
pub const DEAFEN_MEMBERS: Perm = 1 << 20;
pub const MOVE_MEMBERS: Perm = 1 << 21;
pub const TIMEOUT_MEMBERS: Perm = 1 << 25;
pub const DISCONNECT_MEMBERS: Perm = 1 << 26;
pub const ORGANIZE_WATCH: Perm = 1 << 27;

pub const ALL: Perm = (1 << 28) - 1;
pub const EVERYONE: Perm = VIEW_CHANNEL
    | SEND_MESSAGES
    | ATTACH_FILES
    | ADD_REACTIONS
    | CREATE_POLLS
    | CONNECT
    | SPEAK
    | VIDEO
    | SHARE_SCREEN
    | ORGANIZE_WATCH;
pub const TIMED_OUT: Perm = SEND_MESSAGES
    | ATTACH_FILES
    | ADD_REACTIONS
    | MENTION_EVERYONE
    | CREATE_POLLS
    | CONNECT
    | SPEAK
    | VIDEO
    | SHARE_SCREEN;
pub const LOCKED: Perm = SEND_MESSAGES | ATTACH_FILES | ADD_REACTIONS | CREATE_POLLS;

const KEYS: [(&str, Perm); 25] = [
    ("manage_server", MANAGE_SERVER),
    ("manage_roles", MANAGE_ROLES),
    ("manage_channels", MANAGE_CHANNELS),
    ("create_invites", CREATE_INVITES),
    ("view_audit_log", VIEW_AUDIT_LOG),
    ("kick_members", KICK_MEMBERS),
    ("ban_members", BAN_MEMBERS),
    ("manage_nicknames", MANAGE_NICKNAMES),
    ("view_channel", VIEW_CHANNEL),
    ("send_messages", SEND_MESSAGES),
    ("attach_files", ATTACH_FILES),
    ("add_reactions", ADD_REACTIONS),
    ("mention_everyone", MENTION_EVERYONE),
    ("manage_messages", MANAGE_MESSAGES),
    ("create_polls", CREATE_POLLS),
    ("connect", CONNECT),
    ("speak", SPEAK),
    ("video", VIDEO),
    ("share_screen", SHARE_SCREEN),
    ("mute_members", MUTE_MEMBERS),
    ("deafen_members", DEAFEN_MEMBERS),
    ("move_members", MOVE_MEMBERS),
    ("timeout_members", TIMEOUT_MEMBERS),
    ("disconnect_members", DISCONNECT_MEMBERS),
    ("organize_watch", ORGANIZE_WATCH),
];

pub fn cles(p: Perm) -> Vec<&'static str> {
    KEYS.iter()
        .filter(|(_, q)| p & q == *q)
        .map(|(k, _)| *k)
        .collect()
}

pub fn parse(cles: &[String]) -> Option<Perm> {
    cles.iter().try_fold(0, |p, c| {
        KEYS.iter().find(|(k, _)| k == c).map(|(_, q)| p | q)
    })
}

pub fn contient(p: Perm, q: Perm) -> bool {
    p & q == q
}

pub const OWNER_TOP: i32 = i32::MAX;

pub fn top(rs: &[Role]) -> i32 {
    rs.iter().map(|r| r.position).fold(0, i32::max)
}

pub fn can_manage_role(acteur_top: i32, position: i32) -> bool {
    acteur_top > position
}

pub fn can_grant(acteur: Perm, change: Perm) -> bool {
    change & !acteur == 0
}

#[derive(Clone, Debug, Default)]
pub struct Role {
    pub id: String,
    pub position: i32,
    pub perms: Perm,
    pub default: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Override {
    pub role_id: String,
    pub member: bool,
    pub allow: Perm,
    pub deny: Perm,
}

pub fn base(rs: &[Role]) -> Perm {
    rs.iter().fold(0, |p, r| p | r.perms)
}

pub fn dans_salon(base: Perm, rs: &[Role], ov: &[Override]) -> Perm {
    let mut p = base;
    let membre: HashSet<&str> = rs.iter().map(|r| r.id.as_str()).collect();
    let everyone = rs
        .iter()
        .filter(|r| r.default)
        .map(|r| r.id.as_str())
        .next_back()
        .unwrap_or("");
    let (mut allow, mut deny) = (0, 0);
    for o in ov {
        if o.member || (o.role_id != everyone && membre.contains(o.role_id.as_str())) {
            allow |= o.allow;
            deny |= o.deny;
        } else if o.role_id == everyone {
            p = p & !o.deny | o.allow;
        }
    }
    p = p & !deny | allow;
    if contient(p, VIEW_CHANNEL) { p } else { 0 }
}

#[derive(Clone, Debug, Default)]
pub struct Access {
    pub owner: bool,
    pub timed_out: bool,
    pub sub: String,
    pub retire: Perm,
    pub roles: Vec<Role>,
    ov: HashMap<String, Vec<Override>>,
    locked: HashSet<String>,
}

impl Access {
    pub fn base(&self) -> Perm {
        if self.owner {
            return ALL & !self.retire;
        }
        let p = base(&self.roles) & !self.retire;
        if self.timed_out { p & !TIMED_OUT } else { p }
    }

    pub fn top(&self) -> i32 {
        if self.owner {
            OWNER_TOP
        } else {
            top(&self.roles)
        }
    }

    pub fn can(&self, p: Perm) -> bool {
        contient(self.base(), p)
    }

    pub fn channel(&self, channel_id: &str) -> Perm {
        if self.owner {
            return ALL & !self.retire;
        }
        let vide = Vec::new();
        let mut p = dans_salon(
            self.base(),
            &self.roles,
            self.ov.get(channel_id).unwrap_or(&vide),
        );
        if self.timed_out {
            p &= !TIMED_OUT;
        }
        if self.locked.contains(channel_id) && !contient(p, MANAGE_MESSAGES) {
            p &= !LOCKED;
        }
        p & !self.retire
    }
}

pub async fn access_for(
    pool: &PgPool,
    server_id: &str,
    user: &str,
) -> Result<Option<Access>, sqlx::Error> {
    let Some((owner, timed_out, sub)) = sqlx::query_as::<_, (bool, bool, String)>(
        "SELECT s.owner_id = m.user_id, COALESCE(m.timeout_until > now(), false), u.keycloak_sub FROM servers s JOIN server_members m ON m.server_id = s.id JOIN users u ON u.id = m.user_id WHERE s.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(server_id)
    .bind(user)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let roles = sqlx::query_as::<_, (String, i32, i64, bool)>(
        "SELECT id::text, position, permissions, is_default FROM roles
         WHERE server_id=$1::uuid AND (is_default OR id IN (SELECT role_id FROM member_roles WHERE user_id=$2::uuid))",
    )
    .bind(server_id)
    .bind(user)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, position, perms, default)| Role {
        id,
        position,
        perms: perms as Perm,
        default,
    })
    .collect();
    let mut ov = overrides_serveur(pool, server_id, None).await?;
    for (salon, _, o) in overrides_membres(pool, server_id, None, Some(user)).await? {
        ov.entry(salon).or_default().push(o);
    }
    let locked = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM channels WHERE server_id=$1::uuid AND locked",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    Ok(Some(Access {
        owner,
        timed_out,
        sub,
        retire: 0,
        roles,
        ov,
        locked,
    }))
}

async fn overrides_serveur(
    pool: &PgPool,
    server_id: &str,
    channel_id: Option<&str>,
) -> Result<HashMap<String, Vec<Override>>, sqlx::Error> {
    let lignes = sqlx::query_as::<_, (String, String, i64, i64)>(
        "SELECT o.channel_id::text, o.role_id::text, o.allow, o.deny FROM channel_overrides o JOIN channels c ON c.id = o.channel_id
         WHERE c.server_id=$1::uuid AND ($2 = '' OR c.id::text = $2)",
    )
    .bind(server_id)
    .bind(channel_id.unwrap_or(""))
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<Override>> = HashMap::new();
    for (salon, role_id, allow, deny) in lignes {
        out.entry(salon).or_default().push(Override {
            role_id,
            member: false,
            allow: allow as Perm,
            deny: deny as Perm,
        });
    }
    Ok(out)
}

async fn overrides_membres(
    pool: &PgPool,
    server_id: &str,
    channel_id: Option<&str>,
    user: Option<&str>,
) -> Result<Vec<(String, String, Override)>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String, String, i64, i64)>(
        "SELECT o.channel_id::text, o.user_id::text, o.allow, o.deny FROM channel_member_overrides o JOIN channels c ON c.id = o.channel_id
         WHERE c.server_id=$1::uuid AND ($2 = '' OR c.id::text = $2) AND ($3 = '' OR o.user_id::text = $3)",
    )
    .bind(server_id)
    .bind(channel_id.unwrap_or(""))
    .bind(user.unwrap_or(""))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(salon, membre, allow, deny)| {
        (
            salon,
            membre,
            Override {
                role_id: String::new(),
                member: true,
                allow: allow as Perm,
                deny: deny as Perm,
            },
        )
    })
    .collect())
}

pub struct LigneRole {
    pub id: String,
    pub name: String,
    pub color: String,
    pub icon: String,
    pub position: i32,
    pub hoist: bool,
    pub permissions: i64,
    pub is_default: bool,
}

pub const COLONNES_ROLE: &str =
    "id::text, name, color, icon, position, hoist, permissions, is_default";

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LigneRole {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id: row.try_get(0)?,
            name: row.try_get(1)?,
            color: row.try_get(2)?,
            icon: row.try_get(3)?,
            position: row.try_get(4)?,
            hoist: row.try_get(5)?,
            permissions: row.try_get(6)?,
            is_default: row.try_get(7)?,
        })
    }
}

impl LigneRole {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "color": self.color,
            "icon": self.icon,
            "position": self.position,
            "hoist": self.hoist,
            "permissions": cles(self.permissions as Perm),
            "is_default": self.is_default,
        })
    }
}

pub async fn roles_du_serveur(
    pool: &PgPool,
    server_id: &str,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    Ok(sqlx::query_as::<_, LigneRole>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLONNES_ROLE} FROM roles WHERE server_id=$1::uuid ORDER BY position DESC"
    )))
    .bind(server_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.json())
    .collect())
}

pub async fn roles_des_membres(
    pool: &PgPool,
    server_id: &str,
) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
    let lignes = sqlx::query_as::<_, (String, String)>(
        "SELECT mr.user_id::text, mr.role_id::text FROM member_roles mr JOIN roles r ON r.id = mr.role_id
         WHERE mr.server_id=$1::uuid ORDER BY r.position DESC",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (u, r) in lignes {
        out.entry(u).or_default().push(r);
    }
    Ok(out)
}

pub async fn voyeurs_du_salon(
    pool: &PgPool,
    server_id: &str,
    channel_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let ouvert: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM channel_overrides WHERE channel_id=$1::uuid) AND NOT EXISTS (SELECT 1 FROM channel_member_overrides WHERE channel_id=$1::uuid)
         AND EXISTS (SELECT 1 FROM roles WHERE server_id=$2::uuid AND is_default AND permissions & $3 = $3)",
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(VIEW_CHANNEL as i64)
    .fetch_one(pool)
    .await?;
    let membres = sqlx::query_as::<_, (String, bool)>(
        "SELECT m.user_id::text, sv.owner_id = m.user_id FROM server_members m JOIN servers sv ON sv.id = m.server_id WHERE m.server_id=$1::uuid",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?;
    if ouvert {
        return Ok(membres.into_iter().map(|(id, _)| id).collect());
    }
    let par_id: HashMap<String, Role> = sqlx::query_as::<_, (String, i32, i64, bool)>(
        "SELECT id::text, position, permissions, is_default FROM roles WHERE server_id=$1::uuid",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, position, perms, default)| {
        (
            id.clone(),
            Role {
                id,
                position,
                perms: perms as Perm,
                default,
            },
        )
    })
    .collect();
    let mr = roles_des_membres(pool, server_id).await?;
    let ov = overrides_serveur(pool, server_id, Some(channel_id))
        .await?
        .remove(channel_id)
        .unwrap_or_default();
    let mo = overrides_membres(pool, server_id, Some(channel_id), None).await?;
    let mut out = Vec::new();
    for (id, owner) in membres {
        if owner {
            out.push(id);
            continue;
        }
        let mut own: Vec<Role> = par_id.values().filter(|r| r.default).cloned().collect();
        for rid in mr.get(&id).into_iter().flatten() {
            own.push(par_id.get(rid).cloned().unwrap_or_default());
        }
        let mut eff = ov.clone();
        eff.extend(
            mo.iter()
                .filter(|(_, u, _)| *u == id)
                .map(|(_, _, o)| o.clone()),
        );
        if contient(dans_salon(base(&own), &own, &eff), VIEW_CHANNEL) {
            out.push(id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: &str, position: i32, perms: Perm, default: bool) -> Role {
        Role {
            id: id.into(),
            position,
            perms,
            default,
        }
    }

    fn ov(role_id: &str, allow: Perm, deny: Perm) -> Override {
        Override {
            role_id: role_id.into(),
            member: false,
            allow,
            deny,
        }
    }

    #[test]
    fn everyone_egale_la_migration() {
        const FILE_DE_LECTURE: Perm = 3 << 23;
        const EVERYONE_MIGRATION: Perm = 25_677_568;
        assert_eq!(
            EVERYONE & !ORGANIZE_WATCH | FILE_DE_LECTURE,
            EVERYONE_MIGRATION
        );
        assert_eq!(ALL, (1 << 28) - 1);
    }

    const BITS_LIBRES: Perm = 7 << 22;
    const NB_CLES: usize = (ALL.count_ones() - BITS_LIBRES.count_ones()) as usize;

    #[test]
    fn cles_dans_l_ordre() {
        assert_eq!(
            cles(VIEW_CHANNEL | MANAGE_SERVER | ORGANIZE_WATCH),
            ["manage_server", "view_channel", "organize_watch"]
        );
        assert_eq!(cles(ALL).len(), NB_CLES);
    }

    #[test]
    fn union_des_roles() {
        let rs = [
            role("e", 0, VIEW_CHANNEL, true),
            role("a", 2, MANAGE_ROLES, false),
            role("b", 1, SPEAK, false),
        ];
        assert_eq!(base(&rs), VIEW_CHANNEL | MANAGE_ROLES | SPEAK);
    }

    #[test]
    fn exceptions_de_salon() {
        let everyone = role("e", 0, VIEW_CHANNEL | SEND_MESSAGES, true);
        let moderateur = role("m", 1, MANAGE_MESSAGES, false);
        let rs = [everyone.clone(), moderateur];
        let b = base(&rs);
        assert_eq!(dans_salon(b, &rs, &[]), b);

        let prive = [ov("e", 0, VIEW_CHANNEL), ov("m", VIEW_CHANNEL, 0)];
        assert!(contient(dans_salon(b, &rs, &prive), VIEW_CHANNEL));
        assert_eq!(
            dans_salon(everyone.perms, std::slice::from_ref(&everyone), &prive),
            0
        );

        let refus = [ov("m", 0, SEND_MESSAGES)];
        assert!(!contient(dans_salon(b, &rs, &refus), SEND_MESSAGES));
        let etranger = [ov("m", 0, SEND_MESSAGES), ov("x", SEND_MESSAGES, 0)];
        assert!(!contient(dans_salon(b, &rs, &etranger), SEND_MESSAGES));
        let priorite = [ov("e", 0, SEND_MESSAGES), ov("m", SEND_MESSAGES, 0)];
        assert!(contient(dans_salon(b, &rs, &priorite), SEND_MESSAGES));
    }

    #[test]
    fn exclusion_et_verrou_gardent_la_lecture() {
        assert_eq!(TIMED_OUT & VIEW_CHANNEL, 0);
        assert_eq!(LOCKED & VIEW_CHANNEL, 0);
        assert_eq!(LOCKED & MANAGE_MESSAGES, 0);
        let acces = Access {
            timed_out: true,
            roles: vec![role("e", 0, EVERYONE, true)],
            ..Access::default()
        };
        assert!(contient(acces.channel("c"), VIEW_CHANNEL));
        assert!(!contient(acces.channel("c"), SEND_MESSAGES));
        let proprietaire = Access {
            owner: true,
            ..Access::default()
        };
        assert_eq!(proprietaire.channel("c"), ALL);
    }

    #[test]
    fn hierarchie_et_octroi() {
        assert!(!can_manage_role(2, 2) && can_manage_role(3, 2) && !can_manage_role(1, 2));
        assert!(can_manage_role(OWNER_TOP, 1000));
        assert!(!can_grant(SPEAK, SPEAK | VIDEO) && can_grant(SPEAK | VIDEO, VIDEO));
        let rs = [
            role("e", 0, 0, true),
            role("a", 4, 0, false),
            role("b", 2, 0, false),
        ];
        assert_eq!(top(&rs), 4);
        assert_eq!(top(&[]), 0);
        let proprietaire = Access {
            owner: true,
            roles: rs.to_vec(),
            ..Access::default()
        };
        assert_eq!(proprietaire.top(), OWNER_TOP);
        assert_eq!(
            Access {
                roles: rs.to_vec(),
                ..Access::default()
            }
            .top(),
            4
        );
    }

    #[test]
    fn cles_aller_retour() {
        let cles_de = |p| cles(p).into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(
            parse(&["manage_roles".into(), "connect".into()]),
            Some(MANAGE_ROLES | CONNECT)
        );
        assert_eq!(parse(&["inconnue".into()]), None);
        assert_eq!(parse(&[]), Some(0));
        assert_eq!(parse(&cles_de(ALL)), Some(ALL & !BITS_LIBRES));
        assert_eq!(
            parse(&cles_de(VIEW_CHANNEL | SPEAK)),
            Some(VIEW_CHANNEL | SPEAK)
        );
        assert_eq!(KEYS.len(), NB_CLES);
    }

    #[test]
    fn chaque_combinaison_de_surcharges() {
        const T: Perm = SEND_MESSAGES;
        let effet = |n: u8, p: Perm| match n {
            1 => (p, 0),
            2 => (0, p),
            _ => (0, 0),
        };
        let mut cas = 0;
        for tous in 0..3u8 {
            for du_role in 0..3u8 {
                for a_le_role in [false, true] {
                    for du_membre in 0..3u8 {
                        for base_t in [false, true] {
                            for voit in [false, true] {
                                let tous_perms = if voit { VIEW_CHANNEL } else { 0 }
                                    | if base_t { T } else { 0 };
                                let everyone = role("e", 0, tous_perms, true);
                                let vip = role("v", 1, 0, false);
                                let rs: Vec<Role> = if a_le_role {
                                    vec![everyone, vip]
                                } else {
                                    vec![everyone]
                                };
                                let mut surcharges = Vec::new();
                                let (a, d) = effet(tous, T);
                                surcharges.push(ov("e", a, d));
                                let (a, d) = effet(du_role, T);
                                surcharges.push(ov("v", a, d));
                                let (a, d) = effet(du_membre, T);
                                surcharges.push(Override {
                                    member: true,
                                    ..ov("", a, d)
                                });
                                let obtenu = contient(dans_salon(base(&rs), &rs, &surcharges), T);
                                let apres_tous = match tous {
                                    1 => true,
                                    2 => false,
                                    _ => base_t,
                                };
                                let refuse = (a_le_role && du_role == 2) || du_membre == 2;
                                let autorise = (a_le_role && du_role == 1) || du_membre == 1;
                                let attendu = voit && ((apres_tous && !refuse) || autorise);
                                assert_eq!(
                                    obtenu, attendu,
                                    "tous={tous} role={du_role} a_le_role={a_le_role} membre={du_membre} base={base_t} voit={voit}"
                                );
                                cas += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cas, 216);
    }
}
