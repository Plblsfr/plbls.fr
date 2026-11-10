use super::*;

pub type Params = Path<HashMap<String, String>>;

pub(super) enum Echec {
    NonTrouve,
    NomPris,
    TropDeSalons,
    Banni,
    Base(sqlx::Error),
}

impl From<sqlx::Error> for Echec {
    fn from(e: sqlx::Error) -> Self {
        Echec::Base(e)
    }
}

pub fn unique(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23505"))
}

pub fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

pub(super) fn echec_interne(e: Echec) -> Response {
    match e {
        Echec::Base(e) => bd(e),
        _ => internal(),
    }
}

pub fn mauvais() -> Response {
    write_err(StatusCode::BAD_REQUEST, "bad_request", "JSON invalide")
}

pub fn non_trouve(message: &str) -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", message)
}

pub fn interdit() -> Response {
    write_err(StatusCode::FORBIDDEN, "forbidden", "permission manquante")
}

pub fn sans_contenu() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn pool(state: &AppState) -> Result<&PgPool, Response> {
    state.pool().map_err(DbIndisponible::into_response)
}

pub fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

pub struct SalonVisible {
    pub id: String,
    pub serveur: String,
    pub kind: String,
    pub limite: i32,
    pub perms: Perm,
}

pub async fn salon_visible(
    state: &AppState,
    cid: &str,
    user: &str,
) -> Result<SalonVisible, Response> {
    if !uuid_valide(cid) {
        return Err(non_trouve("introuvable"));
    }
    let pool = pool(state)?;
    let (serveur, kind, limite) = sqlx::query_as::<_, (String, String, i32)>(
        "SELECT c.server_id::text, c.kind, c.user_limit FROM channels c
         JOIN server_members m ON m.server_id = c.server_id WHERE c.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(cid)
    .bind(user)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(|| non_trouve("introuvable"))?;
    let perms = state
        .acces_serveur(&serveur, user)
        .await
        .map_err(bd)?
        .ok_or_else(|| non_trouve("introuvable"))?
        .channel(cid);
    if !roles::contient(perms, roles::VIEW_CHANNEL) {
        return Err(non_trouve("introuvable"));
    }
    Ok(SalonVisible {
        id: cid.to_string(),
        serveur,
        kind,
        limite,
        perms,
    })
}

pub fn nom_propre(s: &str, max: usize) -> Option<String> {
    let s = s.trim();
    let n = s.chars().count();
    (1..=max)
        .contains(&n)
        .then_some(s)
        .filter(|s| !s.chars().any(char::is_control))
        .map(str::to_string)
}

pub fn entier(o: &Map<String, Value>, nom: &str) -> Option<i64> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(0),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

pub fn booleen(o: &Map<String, Value>, nom: &str) -> Option<bool> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

pub fn liste(o: &Map<String, Value>, nom: &str) -> Option<Vec<String>> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>(),
        _ => None,
    }
}

pub fn rfc3339(s: &str) -> String {
    match s.split_once('.') {
        Some((debut, fraction)) => match fraction.trim_end_matches('0') {
            "" => format!("{debut}Z"),
            f => format!("{debut}.{f}Z"),
        },
        None => format!("{s}Z"),
    }
}

pub fn horodatage(colonne: &str) -> String {
    format!("to_char({colonne} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US')")
}

pub(super) fn nouveau_code() -> String {
    let mut b = [0u8; 10];
    rand::rng().fill_bytes(&mut b);
    let c = |x: u8| ALPHABET_CODE[x as usize % ALPHABET_CODE.len()] as char;
    let (a, z) = b.split_at(5);
    format!(
        "{}-{}",
        a.iter().copied().map(c).collect::<String>(),
        z.iter().copied().map(c).collect::<String>()
    )
}
