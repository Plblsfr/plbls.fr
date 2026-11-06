use std::{collections::HashMap, time::Duration};

use reqwest::{Client as Http, Response};
use serde::Deserialize;

use crate::analyse::est_id_video;

const IDS_MAX: usize = 50;
const REPONSE_MAX: usize = 2 << 20;
pub const BASE: &str = "https://www.googleapis.com/youtube/v3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Titre {
    pub id: String,
    pub titre: String,
    pub artiste: String,
    pub album: String,
    pub duree: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Liste {
    pub titre: String,
    pub auteur: String,
    pub total: u32,
    pub ids: Vec<String>,
}

pub struct Client {
    cle: String,
    base: String,
    http: Http,
}

pub fn http() -> Http {
    Http::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client HTTP de la musique")
}

pub fn http_api() -> Http {
    Http::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client HTTP de la musique")
}

pub async fn lire_limite(mut reponse: Response, max: usize) -> Result<Vec<u8>, reqwest::Error> {
    let mut corps = Vec::new();
    while corps.len() < max {
        match reponse.chunk().await? {
            Some(morceau) => corps.extend_from_slice(&morceau),
            None => break,
        }
    }
    corps.truncate(max);
    Ok(corps)
}

pub(crate) fn sans_topic(nom: &str) -> String {
    nom.strip_suffix(" - Topic").unwrap_or(nom).to_string()
}

#[derive(Deserialize, Default)]
struct Videos {
    #[serde(default)]
    items: Vec<VideoItem>,
}

#[derive(Deserialize, Default)]
struct VideoItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    snippet: VideoSnippet,
    #[serde(default, rename = "contentDetails")]
    details: VideoDetails,
}

#[derive(Deserialize, Default)]
struct VideoSnippet {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "channelTitle")]
    chaine: String,
}

#[derive(Deserialize, Default)]
struct VideoDetails {
    #[serde(default)]
    duration: String,
}

#[derive(Deserialize, Default)]
struct Recherche {
    #[serde(default)]
    items: Vec<ResultatRecherche>,
}

#[derive(Deserialize, Default)]
struct ResultatRecherche {
    #[serde(default)]
    id: IdRecherche,
}

#[derive(Deserialize, Default)]
struct IdRecherche {
    #[serde(default, rename = "videoId")]
    video: String,
}

#[derive(Deserialize, Default)]
struct Playlists {
    #[serde(default)]
    items: Vec<PlaylistItem>,
}

#[derive(Deserialize, Default)]
struct PlaylistItem {
    #[serde(default)]
    snippet: PlaylistSnippet,
    #[serde(default, rename = "contentDetails")]
    details: PlaylistDetails,
}

#[derive(Deserialize, Default)]
struct PlaylistDetails {
    #[serde(default, rename = "itemCount")]
    total: u32,
}

#[derive(Deserialize, Default)]
struct PlaylistSnippet {
    #[serde(default)]
    title: String,
    #[serde(default, rename = "channelTitle")]
    auteur: String,
}

#[derive(Deserialize, Default)]
struct Elements {
    #[serde(default)]
    items: Vec<Element>,
}

#[derive(Deserialize, Default)]
struct Element {
    #[serde(default, rename = "contentDetails")]
    details: ElementDetails,
}

#[derive(Deserialize, Default)]
struct ElementDetails {
    #[serde(default, rename = "videoId")]
    video: String,
}

impl Client {
    pub fn new(cle: &str, base: &str, http: Http) -> Self {
        Self {
            cle: cle.to_string(),
            base: base.trim_end_matches('/').to_string(),
            http,
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        chemin: &str,
        params: &[(&str, &str)],
    ) -> Result<T, String> {
        let reponse = self
            .http
            .get(format!("{}{chemin}", self.base))
            .query(params)
            .query(&[("key", self.cle.as_str())])
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(format!("youtube : statut {}", reponse.status()));
        }
        let corps = lire_limite(reponse, REPONSE_MAX)
            .await
            .map_err(|e| e.without_url().to_string())?;
        serde_json::from_slice(&corps).map_err(|e| e.to_string())
    }

    pub async fn videos(&self, ids: &[String]) -> Result<Vec<Titre>, String> {
        if ids.is_empty() || ids.len() > IDS_MAX {
            return Err("music : 1 à 50 identifiants".into());
        }
        let r: Videos = self
            .get(
                "/videos",
                &[("part", "snippet,contentDetails"), ("id", &ids.join(","))],
            )
            .await?;
        let mut par_id: HashMap<String, Titre> = r
            .items
            .into_iter()
            .map(|it| {
                (
                    it.id.clone(),
                    Titre {
                        id: it.id,
                        titre: it.snippet.title,
                        artiste: sans_topic(&it.snippet.chaine),
                        album: album_de(&it.snippet.description),
                        duree: duree_iso(&it.details.duration),
                    },
                )
            })
            .collect();
        Ok(ids.iter().filter_map(|id| par_id.remove(id)).collect())
    }

    pub async fn rechercher(&self, q: &str) -> Result<Vec<String>, String> {
        let r: Recherche = self
            .get(
                "/search",
                &[
                    ("part", "id"),
                    ("type", "video"),
                    ("videoCategoryId", "10"),
                    ("maxResults", "8"),
                    ("q", q),
                ],
            )
            .await?;
        Ok(r.items
            .into_iter()
            .map(|it| it.id.video)
            .filter(|v| est_id_video(v))
            .take(8)
            .collect())
    }

    pub async fn liste(&self, id: &str) -> Result<Liste, String> {
        let p: Playlists = self
            .get(
                "/playlists",
                &[("part", "snippet,contentDetails"), ("id", id)],
            )
            .await?;
        let it = p
            .items
            .into_iter()
            .next()
            .ok_or_else(|| "liste introuvable".to_string())?;
        let e: Elements = self
            .get(
                "/playlistItems",
                &[
                    ("part", "contentDetails"),
                    ("playlistId", id),
                    ("maxResults", "50"),
                ],
            )
            .await?;
        Ok(Liste {
            titre: it.snippet.title,
            auteur: sans_topic(&it.snippet.auteur),
            total: it.details.total,
            ids: e
                .items
                .into_iter()
                .map(|e| e.details.video)
                .filter(|v| est_id_video(v))
                .collect(),
        })
    }
}

pub fn duree_iso(s: &str) -> u32 {
    let Some(reste) = s.strip_prefix("PT") else {
        return 0;
    };
    let mut total: u64 = 0;
    let mut chiffres = String::new();
    let mut ordre = 0;
    for c in reste.chars() {
        if c.is_ascii_digit() {
            chiffres.push(c);
            continue;
        }
        let (rang, facteur) = match c {
            'H' => (1, 3600),
            'M' => (2, 60),
            'S' => (3, 1),
            _ => return 0,
        };
        if chiffres.is_empty() || rang <= ordre {
            return 0;
        }
        ordre = rang;
        let Ok(n) = chiffres.parse::<u64>() else {
            return 0;
        };
        total = total.saturating_add(n.saturating_mul(facteur));
        chiffres.clear();
    }
    if !chiffres.is_empty() {
        return 0;
    }
    u32::try_from(total).unwrap_or(0)
}

pub fn album_de(description: &str) -> String {
    if !description.starts_with("Provided to YouTube by") {
        return String::new();
    }
    let normalise = description.replace("\r\n", "\n");
    let blocs: Vec<&str> = normalise.split("\n\n").collect();
    if blocs.len() < 3 || !blocs[1].contains(" · ") {
        return String::new();
    }
    let album = blocs[2].trim();
    if !album.is_empty() && !album.starts_with('℗') && album.len() <= 200 {
        album.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Client, Liste, Titre, album_de, duree_iso, http};
    use estompe_core::testutil::{Faux, faux};

    #[test]
    fn duree_iso_lit_les_durees() {
        for (entree, attendu) in [
            ("PT3M25S", 205),
            ("PT1H2M3S", 3723),
            ("PT45S", 45),
            ("PT2M", 120),
            ("P0D", 0),
            ("", 0),
            ("n'importe quoi", 0),
            ("PT", 0),
            ("PT3S2M", 0),
            ("PT12", 0),
            ("PT99999999999999999999S", 0),
        ] {
            assert_eq!(duree_iso(entree), attendu, "{entree}");
        }
    }

    #[test]
    fn album_de_lit_le_troisieme_bloc() {
        let d = "Provided to YouTube by Label\n\nMon titre · Mon artiste\n\nMon album\n\n℗ 2020 Label\n\nReleased on: 2020-01-01";
        assert_eq!(album_de(d), "Mon album");
        assert_eq!(album_de(&d.replace('\n', "\r\n")), "Mon album");
        for s in [
            "",
            "Une vidéo comme une autre\n\nAvec du texte\n\nEt plus",
            "Provided to YouTube by L\n\nSans séparateur\n\nX",
            "Provided to YouTube by L\n\nT · A\n\n℗ 2020",
            "Provided to YouTube by L\n\nT · A",
        ] {
            assert_eq!(album_de(s), "", "{s:?}");
        }
        let long = format!("Provided to YouTube by L\n\nT · A\n\n{}", "a".repeat(201));
        assert_eq!(album_de(&long), "");
    }

    async fn serveur() -> Faux {
        faux(|r| {
            if !r.query.contains("key=k") {
                return (403, vec![]);
            }
            let corps = match r.chemin.as_str() {
                "/videos" => r#"{"items":[
{"id":"bbbbbbbbbbb","snippet":{"title":"B","description":"","channelTitle":"Art - Topic","thumbnails":{"high":{"url":"https://i.ytimg.com/b.jpg"},"default":{"url":"d"}}},"contentDetails":{"duration":"PT1M"}},
{"id":"aaaaaaaaaaa","snippet":{"title":"A","description":"Provided to YouTube by L\n\nA · Art\n\nAlb\n\n℗ x","channelTitle":"Art - Topic","thumbnails":{"maxres":{"url":"https://i.ytimg.com/a.jpg"},"high":{"url":"h"}}},"contentDetails":{"duration":"PT3M25S"}}]}"#.to_string(),
                "/playlists" if r.query.contains("id=OLAK5uy_x") => r#"{"items":[{"snippet":{"title":"Alb","channelTitle":"Art - Topic","thumbnails":{"high":{"url":"https://i.ytimg.com/l.jpg"}}},"contentDetails":{"itemCount":2}}]}"#.to_string(),
                "/playlists" => r#"{"items":[]}"#.to_string(),
                "/playlistItems" => r#"{"items":[{"contentDetails":{"videoId":""}},{"contentDetails":{"videoId":"aaaaaaaaaaa"}},{"contentDetails":{"videoId":"court"}},{"contentDetails":{"videoId":"bbbbbbbbbbb"}}]}"#.to_string(),
                "/search" => {
                    let mut items: Vec<String> = (0..10)
                        .map(|i| format!(r#"{{"id":{{"videoId":"video{i:06}"}}}}"#))
                        .collect();
                    items.insert(0, r#"{"id":{"channelId":"c"}}"#.to_string());
                    items.insert(1, r#"{"id":{"videoId":"court"}}"#.to_string());
                    format!(r#"{{"items":[{}]}}"#, items.join(","))
                }
                _ => return (404, vec![]),
            };
            (200, corps.into_bytes())
        })
        .await
    }

    fn client(f: &Faux, cle: &str) -> Client {
        Client::new(cle, &f.url, http())
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[tokio::test]
    async fn videos_garde_l_ordre_et_omet_les_introuvables() {
        let f = serveur().await;
        let c = client(&f, "k");
        let ts = c
            .videos(&ids(&["aaaaaaaaaaa", "bbbbbbbbbbb", "ccccccccccc"]))
            .await
            .unwrap();
        assert_eq!(
            ts,
            vec![
                Titre {
                    id: "aaaaaaaaaaa".into(),
                    titre: "A".into(),
                    artiste: "Art".into(),
                    album: "Alb".into(),
                    duree: 205,
                },
                Titre {
                    id: "bbbbbbbbbbb".into(),
                    titre: "B".into(),
                    artiste: "Art".into(),
                    album: String::new(),
                    duree: 60,
                },
            ]
        );
        assert!(
            f.requetes()[0]
                .query
                .contains("id=aaaaaaaaaaa%2Cbbbbbbbbbbb%2Cccccccccccc")
        );
    }

    #[tokio::test]
    async fn videos_refuse_une_cle_rejetee_et_les_listes_hors_bornes() {
        let f = serveur().await;
        assert!(
            client(&f, "mauvaise")
                .videos(&ids(&["aaaaaaaaaaa"]))
                .await
                .is_err()
        );
        let c = client(&f, "k");
        assert!(c.videos(&[]).await.is_err());
        let trop: Vec<String> = (0..51).map(|i| format!("{i:011}")).collect();
        assert!(c.videos(&trop).await.is_err());
        assert_eq!(f.appels(), 1);
    }

    #[tokio::test]
    async fn liste_decrit_la_playlist() {
        let f = serveur().await;
        let c = client(&f, "k");
        assert_eq!(
            c.liste("OLAK5uy_x").await.unwrap(),
            Liste {
                titre: "Alb".into(),
                auteur: "Art".into(),
                total: 2,
                ids: ids(&["aaaaaaaaaaa", "bbbbbbbbbbb"]),
            }
        );
        assert_eq!(c.liste("absente").await.unwrap_err(), "liste introuvable");
    }

    #[tokio::test]
    async fn rechercher_limite_a_huit_et_ignore_les_entrees_sans_video() {
        let f = serveur().await;
        let r = client(&f, "k").rechercher("daft punk").await.unwrap();
        assert_eq!(
            r,
            (0..8).map(|i| format!("video{i:06}")).collect::<Vec<_>>()
        );
        let q = &f.requetes()[0].query;
        assert!(q.contains("videoCategoryId=10") && q.contains("maxResults=8"));
        assert!(q.contains("q=daft+punk") || q.contains("q=daft%20punk"));
    }

    #[tokio::test]
    async fn les_erreurs_reseau_ne_contiennent_pas_la_cle() {
        let c = Client::new("secret", "http://127.0.0.1:1", http());
        let e = c.videos(&ids(&["aaaaaaaaaaa"])).await.unwrap_err();
        assert!(!e.contains("secret"), "{e}");
        let e = c.rechercher("daft punk").await.unwrap_err();
        assert!(!e.contains("secret"), "{e}");
        let e = c.liste("OLAK5uy_x").await.unwrap_err();
        assert!(!e.contains("secret"), "{e}");
    }
}
