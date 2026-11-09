#[cfg(all(feature = "overlay-natif", windows))]
mod calque;
#[cfg(feature = "overlay-natif")]
mod moteur;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Participant {
    pub id: String,
    pub pseudo: String,
    #[serde(default)]
    pub avatar: String,
    pub speaking: bool,
    pub muted: bool,
    pub deaf: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EtatOverlay {
    pub size: String,
    pub items: Vec<Participant>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub largeur: u32,
    pub hauteur: u32,
    pub echelle: f32,
}

#[cfg_attr(not(feature = "overlay-natif"), allow(dead_code))]
pub fn taille_avatar(taille: &str) -> u32 {
    match taille {
        "sm" => 28,
        "lg" => 56,
        _ => 40,
    }
}

#[cfg_attr(not(feature = "overlay-natif"), allow(dead_code))]
pub fn initiales(pseudo: &str) -> String {
    pseudo.chars().take(2).collect::<String>().to_uppercase()
}

#[tauri::command]
pub fn overlay_natif_disponible() -> bool {
    cfg!(feature = "overlay-natif")
}

#[tauri::command]
pub fn overlay_natif_afficher(etat: EtatOverlay, placement: Placement) -> Result<(), String> {
    #[cfg(feature = "overlay-natif")]
    return moteur::envoyer(moteur::Commande::Afficher(etat, placement));
    #[cfg(not(feature = "overlay-natif"))]
    {
        let _ = (etat, placement);
        Err("overlay natif absent de ce build".into())
    }
}

#[tauri::command]
pub fn overlay_natif_fermer() -> Result<(), String> {
    #[cfg(feature = "overlay-natif")]
    return moteur::envoyer(moteur::Commande::Fermer);
    #[cfg(not(feature = "overlay-natif"))]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memes_tailles_et_initiales_que_le_front() {
        assert_eq!(taille_avatar("sm"), 28);
        assert_eq!(taille_avatar("md"), 40);
        assert_eq!(taille_avatar("lg"), 56);
        assert_eq!(initiales("alice"), "AL");
        assert_eq!(initiales("é"), "É");
    }

    #[test]
    fn lit_l_etat_emis_pour_l_overlay() {
        let e: EtatOverlay = serde_json::from_str(
            r#"{"size":"md","items":[{"id":"u1","pseudo":"alice","avatar":"","speaking":true,"muted":false,"deaf":false}]}"#,
        )
        .unwrap();
        assert_eq!(e.items[0].pseudo, "alice");
        let p: Placement =
            serde_json::from_str(r#"{"x":-1904,"y":16,"largeur":325,"hauteur":60,"echelle":1.25}"#)
                .unwrap();
        assert_eq!(p.x, -1904);
    }

    #[test]
    fn disponible_selon_le_build() {
        assert_eq!(overlay_natif_disponible(), cfg!(feature = "overlay-natif"));
    }
}
