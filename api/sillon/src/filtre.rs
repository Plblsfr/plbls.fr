pub const AUDIO: [&str; 12] = [
    "flac", "mp3", "ogg", "oga", "opus", "m4a", "aac", "wav", "aif", "aiff", "wv", "ape",
];
const AUTRES: [&str; 24] = [
    "exe", "bat", "cmd", "scr", "ps1", "sh", "lnk", "msi", "zip", "rar", "7z", "iso", "nfo", "txt",
    "url", "html", "htm", "jpg", "png", "cue", "log", "m3u", "mkv", "mp4",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raison {
    HorsMedias,
    ExtensionInterdite,
    DoubleExtension,
    ConteneurIncoherent,
    SansFluxAudio,
    NomSuspect,
    AnalyseImpossible,
}

impl Raison {
    pub fn code(self) -> &'static str {
        match self {
            Raison::HorsMedias => "hors_medias",
            Raison::ExtensionInterdite => "extension_interdite",
            Raison::DoubleExtension => "double_extension",
            Raison::ConteneurIncoherent => "conteneur_incoherent",
            Raison::SansFluxAudio => "sans_flux_audio",
            Raison::NomSuspect => "nom_suspect",
            Raison::AnalyseImpossible => "analyse_impossible",
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
        "flac" => &["flac"],
        "mp3" => &["mp3"],
        "ogg" | "oga" | "opus" => &["ogg"],
        "m4a" => &["mp4", "m4a", "mov"],
        "aac" => &["aac"],
        "wav" => &["wav"],
        "aif" | "aiff" => &["aiff"],
        "wv" => &["wv"],
        "ape" => &["ape"],
        _ => &[],
    };
    conteneur
        .split(',')
        .map(|c| c.trim().to_ascii_lowercase())
        .any(|c| attendus.contains(&c.as_str()))
}

fn extension_connue(segment: &str) -> bool {
    let s = segment.to_ascii_lowercase();
    AUDIO.contains(&s.as_str()) || AUTRES.contains(&s.as_str())
}

pub fn extension(chemin: &str) -> String {
    let nom = nom_de_fichier(chemin);
    match nom.rsplit_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => String::new(),
    }
}

pub fn nom(chemin: &str) -> Result<(), Raison> {
    let nom = nom_de_fichier(chemin);
    if nom_suspect(nom) {
        return Err(Raison::NomSuspect);
    }
    let segments: Vec<&str> = nom.split('.').collect();
    if segments.len() > 2
        && segments[1..segments.len() - 1]
            .iter()
            .any(|s| extension_connue(s))
    {
        return Err(Raison::DoubleExtension);
    }
    if segments.len() < 2 || !AUDIO.contains(&extension(chemin).as_str()) {
        return Err(Raison::ExtensionInterdite);
    }
    Ok(())
}

pub fn verdict(chemin: &str, conteneur: &str, flux_audio: bool) -> Result<(), Raison> {
    nom(chemin)?;
    if !conteneur_coherent(&extension(chemin), conteneur) {
        return Err(Raison::ConteneurIncoherent);
    }
    if !flux_audio {
        return Err(Raison::SansFluxAudio);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conteneur(e: &str) -> &'static str {
        match e {
            "flac" => "flac",
            "mp3" => "mp3",
            "ogg" | "oga" | "opus" => "ogg",
            "m4a" => "mov,mp4,m4a,3gp,3g2,mj2",
            "aac" => "aac",
            "wav" => "wav",
            "aif" | "aiff" => "aiff",
            "wv" => "wv",
            _ => "ape",
        }
    }

    #[test]
    fn extensions_audio_autorisees() {
        for e in AUDIO {
            let chemin = format!("/music/Artiste/Album (2020)/01 - Titre.{e}");
            assert_eq!(verdict(&chemin, conteneur(e), true), Ok(()), "{e}");
        }
        assert_eq!(verdict("/m/A. B. - C.FLAC", "flac", true), Ok(()));
    }

    #[test]
    fn refus() {
        for (nom, c, raison) in [
            ("a.exe", "flac", Raison::ExtensionInterdite),
            ("a.mkv", "matroska", Raison::ExtensionInterdite),
            ("sans", "flac", Raison::ExtensionInterdite),
            ("a.exe.flac", "flac", Raison::DoubleExtension),
            ("a.flac.exe", "flac", Raison::DoubleExtension),
            ("a.mp3.flac", "flac", Raison::DoubleExtension),
            ("a.flac", "ogg", Raison::ConteneurIncoherent),
            ("a.mp3", "", Raison::ConteneurIncoherent),
            ("a\u{202e}calf.exe", "flac", Raison::NomSuspect),
            ("a.flac ", "flac", Raison::NomSuspect),
            ("", "flac", Raison::NomSuspect),
        ] {
            assert_eq!(verdict(&format!("/m/{nom}"), c, true), Err(raison), "{nom:?}");
        }
        assert_eq!(verdict("/m/a.flac", "flac", false), Err(Raison::SansFluxAudio));
    }
}
