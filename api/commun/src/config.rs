pub fn nettoie(valeur: &str) -> String {
    let v = valeur.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guillemets_et_espaces_retires() {
        assert_eq!(nettoie("  abc "), "abc");
        assert_eq!(nettoie("\"abc\""), "abc");
        assert_eq!(nettoie("'abc'"), "abc");
        assert_eq!(nettoie("'abc\""), "'abc\"");
        assert_eq!(nettoie("\""), "\"");
        assert_eq!(nettoie(""), "");
    }
}
