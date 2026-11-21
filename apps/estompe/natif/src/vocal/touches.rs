use slint::platform::Key;

const SPECIALES: &[(Key, &str)] = &[
    (Key::Space, "Space"),
    (Key::Tab, "Tab"),
    (Key::CapsLock, "CapsLock"),
    (Key::Shift, "ShiftLeft"),
    (Key::ShiftR, "ShiftRight"),
    (Key::Control, "ControlLeft"),
    (Key::ControlR, "ControlRight"),
    (Key::Alt, "AltLeft"),
    (Key::AltGr, "AltRight"),
    (Key::Insert, "Insert"),
    (Key::Delete, "Delete"),
    (Key::Home, "Home"),
    (Key::End, "End"),
    (Key::PageUp, "PageUp"),
    (Key::PageDown, "PageDown"),
    (Key::UpArrow, "ArrowUp"),
    (Key::DownArrow, "ArrowDown"),
    (Key::LeftArrow, "ArrowLeft"),
    (Key::RightArrow, "ArrowRight"),
    (Key::Pause, "Pause"),
    (Key::ScrollLock, "ScrollLock"),
    (Key::F1, "F1"),
    (Key::F2, "F2"),
    (Key::F3, "F3"),
    (Key::F4, "F4"),
    (Key::F5, "F5"),
    (Key::F6, "F6"),
    (Key::F7, "F7"),
    (Key::F8, "F8"),
    (Key::F9, "F9"),
    (Key::F10, "F10"),
    (Key::F11, "F11"),
    (Key::F12, "F12"),
    (Key::F13, "F13"),
    (Key::F14, "F14"),
    (Key::F15, "F15"),
    (Key::F16, "F16"),
    (Key::F17, "F17"),
    (Key::F18, "F18"),
    (Key::F19, "F19"),
    (Key::F20, "F20"),
    (Key::F21, "F21"),
    (Key::F22, "F22"),
    (Key::F23, "F23"),
    (Key::F24, "F24"),
];

pub fn code_de_texte(texte: &str) -> Option<String> {
    let mut caracteres = texte.chars();
    let c = caracteres.next()?;
    if caracteres.next().is_some() {
        return None;
    }
    if c.is_ascii_alphabetic() {
        return Some(format!("Key{}", c.to_ascii_uppercase()));
    }
    if c.is_ascii_digit() {
        return Some(format!("Digit{c}"));
    }
    SPECIALES
        .iter()
        .find(|(touche, _)| char::from(*touche) == c)
        .map(|(_, code)| (*code).to_string())
}

pub fn texte_de_code(code: &str) -> Option<String> {
    if let Some(lettre) = code.strip_prefix("Key").filter(|l| l.len() == 1) {
        return Some(lettre.to_ascii_lowercase());
    }
    if let Some(chiffre) = code.strip_prefix("Digit").filter(|l| l.len() == 1) {
        return Some(chiffre.to_string());
    }
    SPECIALES
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(touche, _)| char::from(*touche).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_touche_tapee_devient_le_code_du_raccourci_et_inversement() {
        assert_eq!(code_de_texte("v").as_deref(), Some("KeyV"));
        assert_eq!(code_de_texte("V").as_deref(), Some("KeyV"));
        assert_eq!(code_de_texte("4").as_deref(), Some("Digit4"));
        assert_eq!(code_de_texte(" ").as_deref(), Some("Space"));
        let f13: slint::SharedString = Key::F13.into();
        assert_eq!(code_de_texte(&f13).as_deref(), Some("F13"));
        let maj: slint::SharedString = Key::Shift.into();
        assert_eq!(code_de_texte(&maj).as_deref(), Some("ShiftLeft"));
        assert_eq!(code_de_texte("é"), None);
        assert_eq!(code_de_texte("ab"), None);
        assert_eq!(code_de_texte(""), None);
        for code in ["KeyV", "Digit4", "Space", "F13", "ShiftLeft", "ArrowUp"] {
            let texte = texte_de_code(code).unwrap();
            assert_eq!(code_de_texte(&texte).as_deref(), Some(code));
            assert!(
                estompe_plateforme::touche(code).is_some(),
                "{code} connu de la plateforme"
            );
        }
    }
}
