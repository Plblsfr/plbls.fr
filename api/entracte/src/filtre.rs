const VIDEOS: [&str; 6] = ["mkv", "mp4", "m4v", "webm", "mov", "avi"];
const SOUS_TITRES: [&str; 4] = ["srt", "vtt", "ass", "ssa"];
const CODECS_SOUS_TITRES: [&str; 6] = ["srt", "subrip", "vtt", "webvtt", "ass", "ssa"];
const INTERDITES: [&str; 17] = [
    "exe", "bat", "cmd", "scr", "ps1", "sh", "lnk", "msi", "zip", "rar", "7z", "iso", "nfo", "txt",
    "url", "html", "htm",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raison {
    ExtensionInterdite,
    DoubleExtension,
    ConteneurIncoherent,
    SansFluxVideo,
    NomSuspect,
}

impl Raison {
    pub fn code(self) -> &'static str {
        match self {
            Raison::ExtensionInterdite => "extension_interdite",
            Raison::DoubleExtension => "double_extension",
            Raison::ConteneurIncoherent => "conteneur_incoherent",
            Raison::SansFluxVideo => "sans_flux_video",
            Raison::NomSuspect => "nom_suspect",
        }
    }
}

pub fn nom_de_fichier(chemin: &str) -> &str {
    chemin.rsplit(['/', '\\']).next().unwrap_or_default()
}

fn nom_suspect(nom: &str) -> bool {
    nom.is_empty()
        || nom.ends_with(['.', ' '])
        || nom.chars().any(|c| {
            c.is_control() || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

fn conteneur_coherent(extension: &str, conteneur: &str) -> bool {
    let attendus: &[&str] = match extension {
        "mkv" => &["mkv", "matroska", "webm"],
        "webm" => &["webm", "matroska"],
        "mp4" | "m4v" | "mov" => &["mp4", "m4v", "mov", "m4a", "3gp", "3g2", "mj2"],
        "avi" => &["avi"],
        _ => &[],
    };
    conteneur
        .split(',')
        .map(|c| c.trim().to_ascii_lowercase())
        .any(|c| attendus.contains(&c.as_str()))
}

fn extension_connue(segment: &str) -> bool {
    let s = segment.to_ascii_lowercase();
    let s = s.as_str();
    VIDEOS.contains(&s) || SOUS_TITRES.contains(&s) || INTERDITES.contains(&s)
}

fn double_extension(segments: &[&str]) -> bool {
    segments.len() > 2
        && segments[1..segments.len() - 1]
            .iter()
            .any(|s| extension_connue(s))
}

pub fn verdict(chemin: &str, conteneur: &str, flux_video: bool) -> Result<(), Raison> {
    let nom = nom_de_fichier(chemin);
    if nom_suspect(nom) {
        return Err(Raison::NomSuspect);
    }
    let segments: Vec<&str> = nom.split('.').collect();
    if double_extension(&segments) {
        return Err(Raison::DoubleExtension);
    }
    let extension = if segments.len() > 1 {
        segments[segments.len() - 1].to_ascii_lowercase()
    } else {
        String::new()
    };
    if !VIDEOS.contains(&extension.as_str()) {
        return Err(Raison::ExtensionInterdite);
    }
    if !conteneur_coherent(&extension, conteneur) {
        return Err(Raison::ConteneurIncoherent);
    }
    if !flux_video {
        return Err(Raison::SansFluxVideo);
    }
    Ok(())
}

pub fn sous_titre_autorise(codec: &str, chemin: Option<&str>) -> bool {
    if !CODECS_SOUS_TITRES.contains(&codec.to_ascii_lowercase().as_str()) {
        return false;
    }
    let Some(chemin) = chemin else { return true };
    let nom = nom_de_fichier(chemin);
    if nom_suspect(nom) {
        return false;
    }
    let segments: Vec<&str> = nom.split('.').collect();
    segments.len() > 1
        && !segments[1..segments.len() - 1].iter().any(|s| {
            let s = s.to_ascii_lowercase();
            VIDEOS.contains(&s.as_str()) || INTERDITES.contains(&s.as_str())
        })
        && SOUS_TITRES.contains(&segments[segments.len() - 1].to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conteneur(extension: &str) -> &'static str {
        match extension {
            "mkv" => "mkv",
            "webm" => "webm",
            "avi" => "avi",
            _ => "mov,mp4,m4a,3gp,3g2,mj2",
        }
    }

    #[test]
    fn extensions_video_autorisees() {
        for e in VIDEOS {
            let chemin = format!("/medias/Film (2024)/Film.2024.1080p.{e}");
            assert_eq!(verdict(&chemin, conteneur(e), true), Ok(()), "{e}");
        }
        assert_eq!(verdict("/m/FILM.MKV", "matroska,webm", true), Ok(()));
        assert_eq!(verdict("C:\\m\\Film.mkv", "mkv", true), Ok(()));
    }

    #[test]
    fn extensions_interdites() {
        for e in INTERDITES {
            assert_eq!(
                verdict(&format!("/m/film.{e}"), "mkv", true),
                Err(Raison::ExtensionInterdite),
                "{e}"
            );
        }
        assert_eq!(
            verdict("/m/sans-extension", "mkv", true),
            Err(Raison::ExtensionInterdite)
        );
        assert_eq!(
            verdict("/m/film.srt", "mkv", true),
            Err(Raison::ExtensionInterdite)
        );
    }

    #[test]
    fn double_extension_refusee() {
        for nom in ["film.mkv.exe", "film.exe.mkv", "film.MP4.mkv", "a.txt.mp4"] {
            assert_eq!(
                verdict(&format!("/m/{nom}"), "mkv", true),
                Err(Raison::DoubleExtension),
                "{nom}"
            );
        }
    }

    #[test]
    fn conteneur_incoherent_refuse() {
        for (nom, c) in [
            ("film.mp4", "mkv"),
            ("film.mkv", "avi"),
            ("film.avi", "mov,mp4"),
            ("film.mkv", ""),
            ("film.webm", "mp4"),
        ] {
            assert_eq!(
                verdict(&format!("/m/{nom}"), c, true),
                Err(Raison::ConteneurIncoherent),
                "{nom} {c}"
            );
        }
    }

    #[test]
    fn sans_flux_video_refuse() {
        assert_eq!(
            verdict("/m/film.mkv", "mkv", false),
            Err(Raison::SansFluxVideo)
        );
    }

    #[test]
    fn noms_suspects_refuses() {
        for nom in [
            "film\u{202e}vkm.exe",
            "film\u{0007}.mkv",
            "film.mkv ",
            "film.mkv.",
            "",
        ] {
            assert_eq!(
                verdict(&format!("/m/{nom}"), "mkv", true),
                Err(Raison::NomSuspect),
                "{nom:?}"
            );
        }
    }

    #[test]
    fn sous_titres_texte_seulement() {
        for c in ["srt", "subrip", "vtt", "webvtt", "ass", "ssa", "SubRip"] {
            assert!(sous_titre_autorise(c, None), "{c}");
        }
        for c in ["pgssub", "dvdsub", ""] {
            assert!(!sous_titre_autorise(c, None), "{c}");
        }
        for e in SOUS_TITRES {
            assert!(sous_titre_autorise("srt", Some(&format!("/m/film.fr.{e}"))));
        }
        for nom in [
            "film.exe",
            "film.srt.exe",
            "film.exe.srt",
            "film.mkv.srt",
            "sans",
        ] {
            assert!(
                !sous_titre_autorise("srt", Some(&format!("/m/{nom}"))),
                "{nom}"
            );
        }
    }
}
