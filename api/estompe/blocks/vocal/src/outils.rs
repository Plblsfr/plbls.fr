use super::*;

pub(super) fn bd(e: sqlx::Error) -> Response {
    tracing::warn!("base : {e}");
    internal()
}

pub(super) fn serveur_introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "serveur introuvable")
}

pub(super) fn membre_introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "membre introuvable")
}

pub(super) fn interdit() -> Response {
    write_err(StatusCode::FORBIDDEN, "forbidden", "permission manquante")
}

pub(super) fn injoignable() -> Response {
    write_err(
        StatusCode::BAD_GATEWAY,
        "voice_unavailable",
        "serveur vocal injoignable",
    )
}

pub(super) fn absent_du_vocal() -> Response {
    write_err(
        StatusCode::CONFLICT,
        "not_in_voice",
        "ce participant n'est plus dans le salon",
    )
}

pub(super) fn param<'a>(p: &'a HashMap<String, String>, nom: &str) -> &'a str {
    p.get(nom).map_or("", String::as_str)
}

pub(super) fn booleen(o: &Map<String, Value>, nom: &str) -> Option<bool> {
    match champ(o, nom) {
        None | Some(Value::Null) => Some(false),
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

pub(super) fn rfc3339(s: &str) -> String {
    match s.split_once('.') {
        Some((debut, fraction)) => match fraction.trim_end_matches('0') {
            "" => format!("{debut}Z"),
            f => format!("{debut}.{f}Z"),
        },
        None => format!("{s}Z"),
    }
}

pub(super) fn mod_de(mic_off: bool, sound_off: bool, p: Perm) -> Mod {
    Mod {
        mic_off,
        sound_off,
        no_speak: !roles::contient(p, roles::SPEAK),
        no_video: !roles::contient(p, roles::VIDEO),
        no_screen: !roles::contient(p, roles::SHARE_SCREEN),
    }
}
