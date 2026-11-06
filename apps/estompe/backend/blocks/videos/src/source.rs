use std::sync::OnceLock;

use regex::Regex;
use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub provider: String,
    pub reference: String,
    pub url: String,
}

const HOTES_YOUTUBE: [&str; 6] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtube-nocookie.com",
    "www.youtube-nocookie.com",
];

const TWITCH_RESERVES: [&str; 16] = [
    "videos",
    "directory",
    "downloads",
    "p",
    "settings",
    "subscriptions",
    "inventory",
    "wallet",
    "turbo",
    "jobs",
    "store",
    "login",
    "signup",
    "search",
    "friends",
    "messages",
];

fn regex(cellule: &'static OnceLock<Regex>, motif: &str) -> &'static Regex {
    cellule.get_or_init(|| Regex::new(motif).expect("expression"))
}

fn re_url() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r#"https?://[^\s<>"'()\[\]]+"#)
}

fn re_chaine_twitch() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^[A-Za-z0-9_]{3,25}$")
}

fn re_slug() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^[A-Za-z0-9_-]{1,100}$")
}

pub fn re_numero() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^\d{1,15}$")
}

fn re_dailymotion() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^x[a-z0-9]{3,12}$")
}

fn re_fichier() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"(?i)\.(mp4|m4v|webm|ogv|m3u8)$")
}

fn re_id_youtube() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^[A-Za-z0-9_-]{11}$")
}

fn re_chemin_youtube() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    regex(&R, r"^/(?:shorts|embed|live|v)/([A-Za-z0-9_-]{11})(?:/|$)")
}

fn site(hote: &str) -> &'static str {
    match hote {
        "twitch.tv" | "www.twitch.tv" | "m.twitch.tv" => "twitch",
        "clips.twitch.tv" => "twitch_clips",
        "vimeo.com" | "www.vimeo.com" => "vimeo",
        "player.vimeo.com" => "vimeo_player",
        "dailymotion.com" | "www.dailymotion.com" => "dailymotion",
        "dai.ly" => "dai.ly",
        _ => "",
    }
}

pub fn jouable(fournisseur: &str) -> bool {
    !fournisseur.is_empty() && fournisseur != "simple"
}

fn id_youtube(brut: &str) -> Option<String> {
    let url = Url::parse(brut).ok()?;
    let hote = url.host_str()?.to_lowercase();
    let id = if hote == "youtu.be" {
        url.path().trim_matches('/').to_string()
    } else if HOTES_YOUTUBE.contains(&hote.as_str()) {
        if let Some(m) = re_chemin_youtube().captures(url.path()) {
            m[1].to_string()
        } else if url.path() == "/watch" {
            url.query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    re_id_youtube().is_match(&id).then_some(id)
}

pub fn analyser(brut: &str) -> Option<Source> {
    let brut = brut.trim();
    if brut.len() > 2048 || brut.is_empty() {
        return None;
    }
    let complet = if brut.contains("://") {
        brut.to_string()
    } else {
        format!("https://{brut}")
    };
    let apres = complet.split_once("://").map_or("", |(_, r)| r);
    let autorite = apres.split(['/', '?', '#']).next().unwrap_or_default();
    if autorite.contains('@') {
        return None;
    }
    let url = Url::parse(&complet).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() {
        return None;
    }
    let hote = url.host_str()?.to_lowercase();
    if !hote.contains('.') {
        return None;
    }
    let chemin = url.path().trim_matches('/').to_string();
    let segs: Vec<&str> = chemin.split('/').collect();
    let mut src = Source {
        provider: String::new(),
        reference: String::new(),
        url: complet.clone(),
    };
    let mut fixer = |p: &str, r: &str| {
        src.provider = p.to_string();
        src.reference = r.to_string();
    };
    let kind = site(&hote);
    if hote == "youtu.be" || HOTES_YOUTUBE.contains(&hote.as_str()) {
        if let Some(lien) = estompe_block_musique::analyser_youtube_music(&complet) {
            let id = match lien {
                estompe_block_musique::Lien::Piste(i) | estompe_block_musique::Lien::Liste(i) => i,
            };
            fixer("ytmusic", &id);
        } else if let Some(id) = id_youtube(&complet) {
            fixer("youtube", &id);
        }
    } else if kind == "twitch" {
        if segs.len() == 2 && segs[0] == "videos" && re_numero().is_match(segs[1]) {
            fixer("twitch_vod", segs[1]);
        } else if segs.len() == 3 && segs[1] == "clip" && re_slug().is_match(segs[2]) {
            fixer("twitch_clip", segs[2]);
        } else if segs.len() == 1
            && re_chaine_twitch().is_match(segs[0])
            && !TWITCH_RESERVES.contains(&segs[0].to_lowercase().as_str())
        {
            fixer("twitch_live", &segs[0].to_lowercase());
        }
    } else if kind == "twitch_clips" {
        if segs.len() == 1 && re_slug().is_match(segs[0]) {
            fixer("twitch_clip", segs[0]);
        }
    } else if kind == "vimeo" || kind == "vimeo_player" {
        let id = segs[segs.len() - 1];
        if re_numero().is_match(id) && (kind == "vimeo" || (segs.len() == 2 && segs[0] == "video"))
        {
            fixer("vimeo", id);
        }
    } else if kind == "dailymotion" {
        if segs.len() == 2 && segs[0] == "video" && re_dailymotion().is_match(segs[1]) {
            fixer("dailymotion", segs[1]);
        }
    } else if kind == "dai.ly" {
        if segs.len() == 1 && re_dailymotion().is_match(segs[0]) {
            fixer("dailymotion", segs[0]);
        }
    } else if url.scheme() == "https" && re_fichier().is_match(url.path()) {
        fixer("file", url.as_str());
    }
    if src.provider.is_empty() {
        src.provider = "simple".into();
    }
    Some(src)
}

pub fn premier_lien(contenu: &str) -> Option<Source> {
    re_url()
        .find_iter(contenu)
        .take(3)
        .find_map(|m| analyser(m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?'])))
}

pub fn titre_de_repli(fournisseur: &str) -> &'static str {
    match fournisseur {
        "youtube" => "Vidéo YouTube",
        "twitch_vod" => "Rediffusion Twitch",
        "twitch_clip" => "Clip Twitch",
        "twitch_live" => "Direct Twitch",
        "vimeo" => "Vidéo Vimeo",
        "dailymotion" => "Vidéo Dailymotion",
        "file" => "Fichier vidéo",
        "ytmusic" => "Musique",
        _ => "Lien vidéo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(brut: &str) -> (String, String) {
        analyser(brut).map_or_else(Default::default, |s| (s.provider, s.reference))
    }

    #[test]
    fn reconnait_chaque_source() {
        let cas = [
            ("https://youtu.be/dQw4w9WgXcQ", "youtube", "dQw4w9WgXcQ"),
            ("youtube.com/watch?v=dQw4w9WgXcQ", "youtube", "dQw4w9WgXcQ"),
            (
                "https://www.youtube.com/shorts/dQw4w9WgXcQ",
                "youtube",
                "dQw4w9WgXcQ",
            ),
            (
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
                "ytmusic",
                "dQw4w9WgXcQ",
            ),
            (
                "https://music.youtube.com/playlist?list=OLAK5uy_album123456",
                "ytmusic",
                "OLAK5uy_album123456",
            ),
            (
                "https://www.twitch.tv/videos/123456",
                "twitch_vod",
                "123456",
            ),
            (
                "https://www.twitch.tv/chaine/clip/Slug-1",
                "twitch_clip",
                "Slug-1",
            ),
            ("https://clips.twitch.tv/Slug-1", "twitch_clip", "Slug-1"),
            ("https://twitch.tv/MaChaine", "twitch_live", "machaine"),
            ("https://twitch.tv/directory", "simple", ""),
            ("https://vimeo.com/12345", "vimeo", "12345"),
            ("https://player.vimeo.com/video/12345", "vimeo", "12345"),
            ("https://player.vimeo.com/12345", "simple", ""),
            (
                "https://www.dailymotion.com/video/x8abc12",
                "dailymotion",
                "x8abc12",
            ),
            ("https://dai.ly/x8abc12", "dailymotion", "x8abc12"),
            (
                "https://exemple.fr/film.MP4",
                "file",
                "https://exemple.fr/film.MP4",
            ),
            ("http://exemple.fr/film.mp4", "simple", ""),
            ("https://exemple.fr/page", "simple", ""),
            ("https://www.youtube.com/watch?v=court", "simple", ""),
        ];
        for (brut, fournisseur, reference) in cas {
            assert_eq!(
                p(brut),
                (fournisseur.to_string(), reference.to_string()),
                "{brut}"
            );
        }
    }

    #[test]
    fn refuse_les_liens_invalides() {
        for brut in [
            "",
            "ftp://exemple.fr/a",
            "https://u@exemple.fr/a",
            "https://localhost/a",
            "pas un lien",
        ] {
            assert!(analyser(brut).is_none(), "{brut}");
        }
    }

    #[test]
    fn premier_lien_retire_la_ponctuation_et_limite_a_trois() {
        let s = premier_lien("regardez https://youtu.be/dQw4w9WgXcQ, super").unwrap();
        assert_eq!(s.reference, "dQw4w9WgXcQ");
        assert!(premier_lien("rien ici").is_none());
        let sans = "http://a http://b http://c https://youtu.be/dQw4w9WgXcQ";
        assert!(premier_lien(sans).is_none());
    }

    #[test]
    fn titres_de_repli_sans_lien_brut() {
        for f in ["youtube", "twitch_vod", "vimeo", "simple", "file", ""] {
            let t = titre_de_repli(f);
            assert!(!t.is_empty() && !t.contains("http"));
        }
        assert!(jouable("youtube") && !jouable("simple") && !jouable(""));
    }
}
