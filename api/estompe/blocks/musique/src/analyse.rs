use percent_encoding::percent_decode_str;
use reqwest::Url;

const HOTES_LISTES: [&str; 4] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
];
const HOTE_MUSIQUE: &str = "music.youtube.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lien {
    Piste(String),
    Liste(String),
}

fn est_caractere_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

pub fn est_id_video(s: &str) -> bool {
    s.len() == 11 && s.chars().all(est_caractere_id)
}

const ALPHABET_ID: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn id_video_canonique(s: &str) -> Option<String> {
    if !est_id_video(s) {
        return None;
    }
    s.bytes()
        .map(|octet| {
            let rang: usize = ALPHABET_ID.iter().position(|&a| a == octet)?;
            Some(char::from(ALPHABET_ID[rang]))
        })
        .collect()
}

fn est_id_liste(s: &str) -> bool {
    (12..=64).contains(&s.len()) && s.chars().all(est_caractere_id)
}

fn url_de(brut: &str) -> Option<Url> {
    let brut = brut.trim();
    if brut.len() > 2048 || brut.contains('\\') {
        return None;
    }
    let complet = if brut.contains("://") {
        brut.to_string()
    } else {
        format!("https://{brut}")
    };
    let apres_schema = complet.split_once("://").map_or("", |(_, reste)| reste);
    let autorite = apres_schema
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if autorite.contains('@') {
        return None;
    }
    let url = Url::parse(&complet).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

fn parametre(url: &Url, nom: &str) -> String {
    url.query_pairs()
        .find(|(k, _)| k == nom)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

fn chemin(url: &Url) -> String {
    percent_decode_str(url.path())
        .decode_utf8_lossy()
        .into_owned()
}

fn analyser(url: &Url) -> Option<(Lien, String)> {
    let hote = url.host_str()?.to_lowercase();
    let chemin = chemin(url);
    if chemin == "/playlist" && HOTES_LISTES.contains(&hote.as_str()) {
        let liste = parametre(url, "list");
        return est_id_liste(&liste).then_some((Lien::Liste(liste), hote));
    }
    if chemin == "/watch" && hote == HOTE_MUSIQUE {
        let video = parametre(url, "v");
        return est_id_video(&video).then_some((Lien::Piste(video), hote));
    }
    None
}

pub fn analyser_url(brut: &str) -> Option<Lien> {
    analyser(&url_de(brut)?).map(|(lien, _)| lien)
}

pub fn analyser_youtube_music(brut: &str) -> Option<Lien> {
    let (lien, hote) = analyser(&url_de(brut)?)?;
    (hote == HOTE_MUSIQUE).then_some(lien)
}

#[cfg(test)]
mod tests {
    use super::{Lien, analyser_url, analyser_youtube_music, est_id_video, id_video_canonique};

    fn piste(s: &str) -> Option<Lien> {
        Some(Lien::Piste(s.into()))
    }

    fn liste(s: &str) -> Option<Lien> {
        Some(Lien::Liste(s.into()))
    }

    #[test]
    fn reconnait_les_pistes_de_youtube_music() {
        assert_eq!(
            analyser_url("https://music.youtube.com/watch?v=dQw4w9WgXcQ"),
            piste("dQw4w9WgXcQ")
        );
        assert_eq!(
            analyser_url("  music.youtube.com/watch?v=dQw4w9WgXcQ&list=RDAMVM  "),
            piste("dQw4w9WgXcQ")
        );
        assert_eq!(
            analyser_url("http://music.youtube.com/watch?v=dQw4w9WgXcQ"),
            piste("dQw4w9WgXcQ")
        );
        assert_eq!(
            analyser_url("https://MUSIC.YouTube.com/watch?v=dQw4w9WgXcQ"),
            piste("dQw4w9WgXcQ")
        );
        assert_eq!(
            analyser_url("https://music.youtube.com/%77atch?v=dQw4w9WgXcQ"),
            piste("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn reconnait_les_listes_sur_les_hotes_youtube() {
        for hote in [
            "music.youtube.com",
            "youtube.com",
            "www.youtube.com",
            "m.youtube.com",
        ] {
            assert_eq!(
                analyser_url(&format!("https://{hote}/playlist?list=OLAK5uy_abcdefghij")),
                liste("OLAK5uy_abcdefghij"),
                "{hote}"
            );
        }
        for hote in ["youtube-nocookie.com", "www.youtube-nocookie.com"] {
            assert_eq!(
                analyser_url(&format!("https://{hote}/playlist?list=OLAK5uy_abcdefghij")),
                None,
                "{hote}"
            );
        }
        assert_eq!(
            analyser_url("https://exemple.fr/playlist?list=OLAK5uy_abcdefghij"),
            None
        );
    }

    #[test]
    fn refuse_les_liens_hors_sujet() {
        for brut in [
            "",
            "https://music.youtube.com/",
            "https://music.youtube.com/watch",
            "https://music.youtube.com/watch?v=court",
            "https://music.youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/playlist?list=court",
            "https://www.youtube.com/playlist?list=",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQa",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "ftp://music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://user@music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://@music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com\\watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com./watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch/?v=dQw4w9WgXcQ",
            "https://evil.fr/?u=music.youtube.com/watch?v=dQw4w9WgXcQ",
        ] {
            assert_eq!(analyser_url(brut), None, "{brut}");
        }
    }

    #[test]
    fn cas_de_reference_du_go() {
        for (entree, attendu) in [
            (
                "https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=x",
                piste("dQw4w9WgXcQ"),
            ),
            (
                "music.youtube.com/playlist?list=OLAK5uy_abcdefgh",
                liste("OLAK5uy_abcdefgh"),
            ),
            (
                "https://www.youtube.com/playlist?list=PLabcdefghijkl",
                liste("PLabcdefghijkl"),
            ),
            ("https://www.youtube.com/watch?v=dQw4w9WgXcQ", None),
            ("https://music.youtube.com/watch?v=court", None),
            (
                "https://music.youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
                None,
            ),
            ("https://music.youtube.com/playlist?list=court", None),
            ("https://user@music.youtube.com/watch?v=dQw4w9WgXcQ", None),
        ] {
            assert_eq!(analyser_url(entree), attendu, "{entree}");
        }
    }

    #[test]
    fn borne_la_longueur_des_listes() {
        let url = |n: usize| format!("https://youtube.com/playlist?list={}", "a".repeat(n));
        assert_eq!(analyser_url(&url(11)), None);
        assert_eq!(analyser_url(&url(12)), liste(&"a".repeat(12)));
        assert_eq!(analyser_url(&url(64)), liste(&"a".repeat(64)));
        assert_eq!(analyser_url(&url(65)), None);
    }

    #[test]
    fn borne_la_longueur_du_lien() {
        let long = format!(
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&x={}",
            "a".repeat(2048)
        );
        assert_eq!(analyser_url(&long), None);
    }

    #[test]
    fn youtube_music_exclut_les_autres_hotes() {
        assert_eq!(
            analyser_youtube_music("music.youtube.com/playlist?list=OLAK5uy_abcdefghij"),
            liste("OLAK5uy_abcdefghij")
        );
        assert_eq!(
            analyser_youtube_music("https://www.youtube.com/playlist?list=OLAK5uy_abcdefghij"),
            None
        );
        assert_eq!(analyser_youtube_music("n'importe quoi"), None);
    }

    #[test]
    fn identifiant_de_video() {
        for id in ["dQw4w9WgXcQ", "a-_Z09abcde"] {
            assert_eq!(id_video_canonique(id).as_deref(), Some(id));
        }
        for id in [
            "dQw4w9WgXc",
            "dQw4w9WgXcQa",
            "dQw4w9WgXc!",
            "dQw4w9WgXcé",
            "../../etc/x",
        ] {
            assert_eq!(id_video_canonique(id), None, "{id}");
        }
        assert!(est_id_video("dQw4w9WgXcQ"));
        assert!(est_id_video("a-_Z09abcde"));
        assert!(!est_id_video("dQw4w9WgXc"));
        assert!(!est_id_video("dQw4w9WgXcQa"));
        assert!(!est_id_video("dQw4w9WgXc!"));
        assert!(!est_id_video("dQw4w9WgXcé"));
    }
}
