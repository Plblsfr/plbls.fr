pub fn echapper_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn contient(s: &str) -> String {
    format!("%{}%", echapper_like(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jokers_echappes() {
        assert_eq!(echapper_like(r"a%b_c\d"), r"a\%b\_c\\d");
        assert_eq!(contient("50%"), r"%50\%%");
    }
}
