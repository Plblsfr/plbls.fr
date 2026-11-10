use super::*;

pub(super) fn non_trouve() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

pub(super) fn maintenant() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn signer(cle: &[u8; 32], id: &str, exp: u64) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(cle).expect("clé HMAC");
    m.update(format!("{id}.{exp}").as_bytes());
    let hex: String = m
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{exp}.{hex}")
}

pub(super) fn egaux(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a), Sha256::digest(b));
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn lien_valide(cle: &[u8; 32], id: &str, jeton: &str, maintenant: u64) -> bool {
    let Some((exp, _)) = jeton.split_once('.') else {
        return false;
    };
    let Ok(exp) = exp.parse::<u64>() else {
        return false;
    };
    maintenant < exp && egaux(jeton, &signer(cle, id, exp))
}

pub fn nom_propre(s: &str) -> String {
    let s = s.replace('\\', "/");
    let base = if s.is_empty() {
        "."
    } else {
        let coupe = s.trim_end_matches('/');
        if coupe.is_empty() {
            "/"
        } else {
            coupe.rsplit('/').next().unwrap_or(coupe)
        }
    };
    let sans_controles: Vec<char> = base.chars().filter(|c| !c.is_control()).collect();
    let debut = sans_controles.len().saturating_sub(100);
    let s: String = sans_controles[debut..].iter().collect();
    if matches!(s.as_str(), "." | "/" | "") {
        "fichier".into()
    } else {
        s
    }
}

pub(super) fn nom_ascii(s: &str) -> String {
    s.chars()
        .map(|c| {
            if (c as u32) < 0x20 || (c as u32) > 0x7e || c == '"' || c == '\\' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

pub(super) fn echappe_chemin(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~$&+=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(super) fn extension(nom: &str) -> String {
    nom.rfind('.')
        .map(|i| nom[i..].to_lowercase())
        .unwrap_or_default()
}

pub(super) fn parametre(requete: Option<&str>, nom: &str) -> String {
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(&s.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned()
    };
    requete
        .unwrap_or_default()
        .split('&')
        .filter_map(|p| p.split_once('=').or(Some((p, ""))))
        .find(|(k, _)| decode(k) == nom)
        .map(|(_, v)| decode(v))
        .unwrap_or_default()
}

pub(super) fn analyser_vocal(duree: &str, onde: &str) -> Option<(i32, Vec<i32>)> {
    let d: i64 = duree.parse().ok()?;
    if !(1..=DUREE_VOCAL_MAX_MS).contains(&d) {
        return None;
    }
    let parts: Vec<&str> = onde.split(',').collect();
    if parts.len() > ONDE_MAX {
        return None;
    }
    let mut v = Vec::with_capacity(parts.len());
    for p in parts {
        let n: i64 = p.parse().ok()?;
        if !(0..=100).contains(&n) {
            return None;
        }
        v.push(n as i32);
    }
    Some((d as i32, v))
}
