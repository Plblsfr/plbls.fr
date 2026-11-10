use subtle::ConstantTimeEq;

pub fn trouver<'a>(jetons: &'a str, donne: &str) -> Option<&'a str> {
    jetons
        .split(',')
        .filter_map(|e| e.trim().split_once(':'))
        .filter(|(_, t)| !t.is_empty())
        .find(|(_, t)| bool::from(t.as_bytes().ct_eq(donne.as_bytes())))
        .map(|(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jetons_de_service() {
        let jetons = "chat:abc, radar:def,casse,vide:";
        assert_eq!(trouver(jetons, "abc"), Some("chat"));
        assert_eq!(trouver(jetons, "def"), Some("radar"));
        assert_eq!(trouver(jetons, "non"), None);
        assert_eq!(trouver(jetons, ""), None);
    }
}
