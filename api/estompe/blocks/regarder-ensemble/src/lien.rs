use percent_encoding::percent_decode_str;
use reqwest::Url;

const HOTES_YOUTUBE: [&str; 6] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtube-nocookie.com",
    "www.youtube-nocookie.com",
];
const RESERVES_TWITCH: [&str; 16] = [
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

fn est_id(s: &str) -> bool {
    s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn est_slug(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn est_numerique(s: &str) -> bool {
    (1..=15).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

fn est_dailymotion(s: &str) -> bool {
    (4..=13).contains(&s.len())
        && s.starts_with('x')
        && s[1..]
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn chemin(url: &Url) -> String {
    percent_decode_str(url.path())
        .decode_utf8_lossy()
        .into_owned()
}

fn avec_schema(brut: &str) -> String {
    if brut.contains("://") {
        brut.to_string()
    } else {
        format!("https://{brut}")
    }
}

fn analyser(brut: &str) -> Option<Url> {
    let url = Url::parse(&avec_schema(brut)).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

fn id_dans_chemin(chemin: &str) -> Option<&str> {
    let reste = chemin.strip_prefix('/')?;
    let (genre, reste) = reste.split_once('/')?;
    if !["shorts", "embed", "live", "v"].contains(&genre) {
        return None;
    }
    let id = reste.get(..11)?;
    let suite = &reste[11..];
    (est_id(id) && (suite.is_empty() || suite.starts_with('/'))).then_some(id)
}

pub fn youtube_id(s: &str) -> Option<String> {
    let s = s.trim();
    if est_id(s) {
        return Some(s.to_string());
    }
    if s.len() > 300 {
        return None;
    }
    let url = analyser(s)?;
    let hote = url.host_str()?.to_lowercase();
    let chemin = chemin(&url);
    let id = if hote == "youtu.be" {
        chemin.trim_matches('/').to_string()
    } else if HOTES_YOUTUBE.contains(&hote.as_str()) {
        if let Some(id) = id_dans_chemin(&chemin) {
            id.to_string()
        } else if chemin == "/watch" {
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
    est_id(&id).then_some(id)
}

fn extension_video(chemin: &str) -> bool {
    let bas = chemin.to_lowercase();
    [".mp4", ".m4v", ".webm", ".ogv", ".m3u8"]
        .iter()
        .any(|e| bas.ends_with(e))
}

fn source_hors_youtube(brut: &str) -> Option<(&'static str, String)> {
    let brut = brut.trim();
    if brut.len() > 2048 || brut.is_empty() {
        return None;
    }
    let url = analyser(brut)?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let hote = url.host_str()?.to_lowercase();
    if !hote.contains('.') {
        return None;
    }
    let chemin = chemin(&url);
    let segments: Vec<&str> = chemin.trim_matches('/').split('/').collect();
    match hote.as_str() {
        "youtu.be" => None,
        h if HOTES_YOUTUBE.contains(&h) => None,
        "twitch.tv" | "www.twitch.tv" | "m.twitch.tv" => match segments.as_slice() {
            ["videos", id] if est_numerique(id) => Some(("twitch_vod", (*id).to_string())),
            [_, "clip", slug] if est_slug(slug, 1, 100) => {
                Some(("twitch_clip", (*slug).to_string()))
            }
            [chaine]
                if est_slug(chaine, 3, 25)
                    && !chaine.contains('-')
                    && !RESERVES_TWITCH.contains(&chaine.to_lowercase().as_str()) =>
            {
                Some(("twitch_live", chaine.to_lowercase()))
            }
            _ => None,
        },
        "clips.twitch.tv" => match segments.as_slice() {
            [slug] if est_slug(slug, 1, 100) => Some(("twitch_clip", (*slug).to_string())),
            _ => None,
        },
        "vimeo.com" | "www.vimeo.com" => segments
            .last()
            .filter(|id| est_numerique(id))
            .map(|id| ("vimeo", (*id).to_string())),
        "player.vimeo.com" => match segments.as_slice() {
            ["video", id] if est_numerique(id) => Some(("vimeo", (*id).to_string())),
            _ => None,
        },
        "dailymotion.com" | "www.dailymotion.com" => match segments.as_slice() {
            ["video", id] if est_dailymotion(id) => Some(("dailymotion", (*id).to_string())),
            _ => None,
        },
        "dai.ly" => match segments.as_slice() {
            [id] if est_dailymotion(id) => Some(("dailymotion", (*id).to_string())),
            _ => None,
        },
        _ if url.scheme() == "https" && extension_video(&chemin) => {
            Some(("file", url.as_str().to_string()))
        }
        _ => None,
    }
}

pub fn lien_video(s: &str) -> Option<(&'static str, String)> {
    source_hors_youtube(s).or_else(|| youtube_id(s).map(|id| ("youtube", id)))
}

#[cfg(test)]
mod tests {
    use super::{lien_video, youtube_id};

    #[test]
    fn identifiants_youtube() {
        for (entree, attendu) in [
            ("dQw4w9WgXcQ", Some("dQw4w9WgXcQ")),
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10s",
                Some("dQw4w9WgXcQ"),
            ),
            ("https://youtu.be/dQw4w9WgXcQ", Some("dQw4w9WgXcQ")),
            ("youtu.be/dQw4w9WgXcQ?t=3", Some("dQw4w9WgXcQ")),
            (
                "https://www.youtube.com/shorts/dQw4w9WgXcQ",
                Some("dQw4w9WgXcQ"),
            ),
            (
                "https://www.youtube.com/embed/dQw4w9WgXcQ/",
                Some("dQw4w9WgXcQ"),
            ),
            (
                "https://www.youtube.com/live/dQw4w9WgXcQ",
                Some("dQw4w9WgXcQ"),
            ),
            (
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
                Some("dQw4w9WgXcQ"),
            ),
            ("https://evil.example/watch?v=dQw4w9WgXcQ", None),
            (
                "https://www.youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
                None,
            ),
            ("https://www.youtube.com/watch?v=court", None),
            ("https://www.youtube.com/shorts/dQw4w9WgXcQx", None),
            ("https://www.youtube.com/playlist?list=PLabcdefghijkl", None),
            ("ftp://youtu.be/dQw4w9WgXcQ", None),
            ("", None),
        ] {
            assert_eq!(youtube_id(entree).as_deref(), attendu, "{entree}");
        }
    }

    #[test]
    fn sources_lisibles() {
        for (entree, attendu) in [
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                Some(("youtube", "dQw4w9WgXcQ")),
            ),
            (
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RDAMVM",
                Some(("youtube", "dQw4w9WgXcQ")),
            ),
            (
                "https://www.twitch.tv/videos/1234567",
                Some(("twitch_vod", "1234567")),
            ),
            (
                "https://www.twitch.tv/streamer/clip/FunnySlug-abc_1",
                Some(("twitch_clip", "FunnySlug-abc_1")),
            ),
            (
                "https://clips.twitch.tv/FunnySlug-abc_1",
                Some(("twitch_clip", "FunnySlug-abc_1")),
            ),
            (
                "https://www.twitch.tv/Streamer_1",
                Some(("twitch_live", "streamer_1")),
            ),
            ("https://www.twitch.tv/directory", None),
            ("https://vimeo.com/76979871", Some(("vimeo", "76979871"))),
            (
                "https://vimeo.com/channels/staffpicks/76979871",
                Some(("vimeo", "76979871")),
            ),
            (
                "https://player.vimeo.com/video/76979871",
                Some(("vimeo", "76979871")),
            ),
            (
                "https://www.dailymotion.com/video/x8abc12",
                Some(("dailymotion", "x8abc12")),
            ),
            ("https://dai.ly/x8abc12", Some(("dailymotion", "x8abc12"))),
            (
                "https://exemple.fr/a.mp4",
                Some(("file", "https://exemple.fr/a.mp4")),
            ),
            (
                "https://exemple.fr/LIVE.M3U8?x=1",
                Some(("file", "https://exemple.fr/LIVE.M3U8?x=1")),
            ),
            ("http://exemple.fr/a.mp4", None),
            ("https://exemple.fr/a.txt", None),
            ("https://www.netflix.com/watch/81234567", None),
            ("https://user@vimeo.com/76979871", None),
            ("https://vimeo.com/a.mp4", None),
        ] {
            let obtenu = lien_video(entree);
            let obtenu = obtenu.as_ref().map(|(p, r)| (*p, r.as_str()));
            assert_eq!(obtenu, attendu, "{entree}");
        }
    }
}
