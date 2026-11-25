pub const VOLUME_MAX: u8 = 100;
pub const PAS_VOLUME: u8 = 5;
pub const PAS_DEPLACEMENT: f64 = 5.0;
pub const PAS_DEPLACEMENT_FILM: f64 = 10.0;
pub const VITESSES: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
pub const VITESSE_MIN: f64 = 0.25;
pub const VITESSE_MAX: f64 = 4.0;

pub const TOUCHE_FLECHE_HAUT: &str = "\u{f700}";
pub const TOUCHE_FLECHE_BAS: &str = "\u{f701}";
pub const TOUCHE_FLECHE_GAUCHE: &str = "\u{f702}";
pub const TOUCHE_FLECHE_DROITE: &str = "\u{f703}";
pub const TOUCHE_ECHAP: &str = "\u{1b}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raccourci {
    Basculer,
    Reculer,
    Avancer,
    PlusFort,
    MoinsFort,
    Muet,
    PleinEcran,
    QuitterPleinEcran,
    PasserIntro,
}

pub fn raccourci(touche: &str, ctrl: bool, meta: bool, alt: bool) -> Option<Raccourci> {
    if ctrl || meta || alt {
        return None;
    }
    let minuscule = touche.to_lowercase();
    let touche = if touche.chars().count() == 1 {
        minuscule.as_str()
    } else {
        touche
    };
    Some(match touche {
        " " | "k" => Raccourci::Basculer,
        TOUCHE_FLECHE_GAUCHE | "ArrowLeft" | "j" => Raccourci::Reculer,
        TOUCHE_FLECHE_DROITE | "ArrowRight" | "l" => Raccourci::Avancer,
        TOUCHE_FLECHE_HAUT | "ArrowUp" => Raccourci::PlusFort,
        TOUCHE_FLECHE_BAS | "ArrowDown" => Raccourci::MoinsFort,
        "m" => Raccourci::Muet,
        "f" => Raccourci::PleinEcran,
        TOUCHE_ECHAP | "Escape" => Raccourci::QuitterPleinEcran,
        "s" => Raccourci::PasserIntro,
        _ => return None,
    })
}

pub fn borner_vitesse(vitesse: f64) -> f64 {
    if vitesse.is_finite() {
        vitesse.clamp(VITESSE_MIN, VITESSE_MAX)
    } else {
        1.0
    }
}

pub fn vitesse_suivante(vitesse: f64) -> f64 {
    VITESSES
        .iter()
        .copied()
        .find(|v| *v > vitesse + 1e-6)
        .unwrap_or(VITESSES[0])
}

pub fn libelle_vitesse(vitesse: f64) -> String {
    let texte = format!("{:.2}", borner_vitesse(vitesse));
    let texte = texte.trim_end_matches('0').trim_end_matches('.');
    format!("{}×", texte.replace('.', ","))
}

pub fn position_bornee(cible: f64, duree: Option<f64>) -> f64 {
    let cible = if cible.is_finite() { cible } else { 0.0 };
    match duree {
        Some(d) => cible.min(d).max(0.0),
        None => cible.max(0.0),
    }
}

pub fn volume_ajuste(volume: u8, muet: bool, ecart: i16) -> (u8, bool) {
    let depart = if muet { 0 } else { i16::from(volume) };
    let v = (depart + ecart).clamp(0, i16::from(VOLUME_MAX)) as u8;
    (v, v == 0)
}

pub fn son_bascule(volume: u8, muet: bool) -> (u8, bool) {
    if !muet || volume > 0 {
        (volume, !muet)
    } else {
        (VOLUME_MAX, false)
    }
}

pub fn taille_de_rendu(video: (u32, u32), zone: (u32, u32)) -> (u32, u32) {
    if video.0 == 0 || video.1 == 0 {
        return (0, 0);
    }
    if zone.0 == 0 || zone.1 == 0 {
        return video;
    }
    let echelle = (f64::from(zone.0) / f64::from(video.0))
        .min(f64::from(zone.1) / f64::from(video.1))
        .min(1.0);
    let cote = |c: u32| ((f64::from(c) * echelle).round() as u32).max(1);
    (cote(video.0), cote(video.1))
}

pub fn peripherique_mpv(sortie: Option<&str>) -> String {
    let Some((hote, id)) = sortie.and_then(|s| s.split_once(':')) else {
        return "auto".to_string();
    };
    if id.is_empty() {
        return "auto".to_string();
    }
    match hote {
        "wasapi" => format!(
            "wasapi/{}",
            id.strip_prefix("{0.0.0.00000000}.").unwrap_or(id)
        ),
        "alsa" | "pulseaudio" | "pulse" | "pipewire" | "jack" | "coreaudio" => {
            let hote = if hote == "pulseaudio" { "pulse" } else { hote };
            format!("{hote}/{id}")
        }
        _ => "auto".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raccourcis_du_front() {
        let r = |t: &str| raccourci(t, false, false, false);
        assert_eq!(r(" "), Some(Raccourci::Basculer));
        assert_eq!(r("K"), Some(Raccourci::Basculer));
        assert_eq!(r(TOUCHE_FLECHE_GAUCHE), Some(Raccourci::Reculer));
        assert_eq!(r("ArrowLeft"), Some(Raccourci::Reculer));
        assert_eq!(r("j"), Some(Raccourci::Reculer));
        assert_eq!(r(TOUCHE_FLECHE_DROITE), Some(Raccourci::Avancer));
        assert_eq!(r("l"), Some(Raccourci::Avancer));
        assert_eq!(r(TOUCHE_FLECHE_HAUT), Some(Raccourci::PlusFort));
        assert_eq!(r(TOUCHE_FLECHE_BAS), Some(Raccourci::MoinsFort));
        assert_eq!(r("M"), Some(Raccourci::Muet));
        assert_eq!(r("f"), Some(Raccourci::PleinEcran));
        assert_eq!(r(TOUCHE_ECHAP), Some(Raccourci::QuitterPleinEcran));
        assert_eq!(r("s"), Some(Raccourci::PasserIntro));
        assert_eq!(r("x"), None);
        assert_eq!(r("arrowleft"), None);
        assert_eq!(raccourci("k", true, false, false), None);
        assert_eq!(raccourci("k", false, true, false), None);
        assert_eq!(raccourci("k", false, false, true), None);
    }

    #[test]
    fn vitesses() {
        assert_eq!(vitesse_suivante(1.0), 1.25);
        assert_eq!(vitesse_suivante(2.0), 0.5);
        assert_eq!(vitesse_suivante(1.1), 1.25);
        assert_eq!(vitesse_suivante(3.0), 0.5);
        assert_eq!(libelle_vitesse(1.0), "1×");
        assert_eq!(libelle_vitesse(1.5), "1,5×");
        assert_eq!(libelle_vitesse(0.75), "0,75×");
        assert_eq!(libelle_vitesse(10.0), "4×");
        assert_eq!(borner_vitesse(0.0), VITESSE_MIN);
        assert_eq!(borner_vitesse(f64::NAN), 1.0);
    }

    #[test]
    fn position_et_volume_comme_le_front() {
        assert_eq!(position_bornee(-3.0, Some(60.0)), 0.0);
        assert_eq!(position_bornee(75.0, Some(60.0)), 60.0);
        assert_eq!(position_bornee(75.0, None), 75.0);
        assert_eq!(position_bornee(f64::NAN, None), 0.0);
        assert_eq!(volume_ajuste(50, false, 5), (55, false));
        assert_eq!(volume_ajuste(98, false, 5), (100, false));
        assert_eq!(volume_ajuste(3, false, -5), (0, true));
        assert_eq!(volume_ajuste(80, true, 5), (5, false));
        assert_eq!(son_bascule(70, false), (70, true));
        assert_eq!(son_bascule(70, true), (70, false));
        assert_eq!(son_bascule(0, true), (100, false));
        assert_eq!(son_bascule(0, false), (0, true));
    }

    #[test]
    fn taille_de_rendu_contenue_sans_agrandir() {
        assert_eq!(taille_de_rendu((1920, 1080), (640, 640)), (640, 360));
        assert_eq!(taille_de_rendu((1920, 1080), (1280, 360)), (640, 360));
        assert_eq!(taille_de_rendu((640, 360), (1920, 1080)), (640, 360));
        assert_eq!(taille_de_rendu((1920, 1080), (0, 0)), (1920, 1080));
        assert_eq!(taille_de_rendu((0, 1080), (640, 360)), (0, 0));
        assert_eq!(taille_de_rendu((4000, 10), (100, 100)), (100, 1));
    }

    #[test]
    fn sortie_son_vers_audio_device_de_mpv() {
        assert_eq!(peripherique_mpv(None), "auto");
        assert_eq!(
            peripherique_mpv(Some("wasapi:{0.0.0.00000000}.{a1b2-c3}")),
            "wasapi/{a1b2-c3}"
        );
        assert_eq!(peripherique_mpv(Some("wasapi:{a1b2}")), "wasapi/{a1b2}");
        assert_eq!(
            peripherique_mpv(Some("alsa:hw:CARD=PCH,DEV=0")),
            "alsa/hw:CARD=PCH,DEV=0"
        );
        assert_eq!(peripherique_mpv(Some("pulseaudio:casque")), "pulse/casque");
        assert_eq!(peripherique_mpv(Some("asio:x")), "auto");
        assert_eq!(peripherique_mpv(Some("alsa:")), "auto");
        assert_eq!(peripherique_mpv(Some("sans-hote")), "auto");
    }
}
