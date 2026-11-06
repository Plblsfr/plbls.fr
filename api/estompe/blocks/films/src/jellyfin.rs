use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use estompe_core::{config::Config, journal};
use reqwest::{Client, RequestBuilder, Response, Url, header::HeaderValue};
use serde::Deserialize;

use crate::filtre::{self, Raison};

const CHAMPS: &str = "Path,MediaSources,MediaStreams,Overview,Genres,ProductionYear";
const CACHE_ENTREES: usize = 256;
pub const IMAGES_OCTETS: usize = 64 << 20;
pub const IMAGE_MAX: usize = 8 << 20;
const LISTE_MAX: usize = 32 << 20;
const DELAI_REQUETE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Piste {
    #[serde(rename = "Type")]
    pub genre: Option<String>,
    pub index: Option<i64>,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub display_title: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Source {
    pub id: Option<String>,
    pub path: Option<String>,
    pub container: Option<String>,
    pub media_streams: Option<Vec<Piste>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Element {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "Type")]
    pub genre: Option<String>,
    pub path: Option<String>,
    pub container: Option<String>,
    pub production_year: Option<i64>,
    pub overview: Option<String>,
    pub run_time_ticks: Option<i64>,
    pub genres: Option<Vec<String>>,
    pub index_number: Option<i64>,
    pub parent_index_number: Option<i64>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub image_tags: Option<HashMap<String, String>>,
    pub backdrop_image_tags: Option<Vec<String>>,
    pub media_sources: Option<Vec<Source>>,
    pub media_streams: Option<Vec<Piste>>,
}

impl Element {
    pub fn jouable(&self) -> bool {
        matches!(self.genre.as_deref(), Some("Movie" | "Episode"))
    }

    pub fn source(&self) -> Option<&Source> {
        self.media_sources.as_deref()?.first()
    }

    pub fn chemin(&self) -> &str {
        self.source()
            .and_then(|s| s.path.as_deref())
            .or(self.path.as_deref())
            .unwrap_or_default()
    }

    pub fn conteneur(&self) -> &str {
        self.source()
            .and_then(|s| s.container.as_deref())
            .or(self.container.as_deref())
            .unwrap_or_default()
    }

    pub fn source_id(&self) -> &str {
        self.source()
            .and_then(|s| s.id.as_deref())
            .unwrap_or(&self.id)
    }

    fn pistes_de(&self, genre: &str) -> Vec<&Piste> {
        self.source()
            .and_then(|s| s.media_streams.as_deref())
            .filter(|p| !p.is_empty())
            .or(self.media_streams.as_deref())
            .unwrap_or_default()
            .iter()
            .filter(|p| p.genre.as_deref() == Some(genre))
            .collect()
    }

    pub fn audio(&self) -> Vec<&Piste> {
        self.pistes_de("Audio")
    }

    pub fn sous_titres(&self) -> Vec<&Piste> {
        self.pistes_de("Subtitle")
            .into_iter()
            .filter(|p| {
                filtre::sous_titre_autorise(
                    p.codec.as_deref().unwrap_or_default(),
                    p.path.as_deref(),
                )
            })
            .collect()
    }

    pub fn rejet(&self) -> Option<Raison> {
        if !self.jouable() {
            return None;
        }
        let video = self
            .pistes_de("Video")
            .iter()
            .any(|p| p.codec.as_deref().is_some_and(|c| !c.is_empty()));
        filtre::verdict(self.chemin(), self.conteneur(), video).err()
    }

    pub fn duree(&self) -> u32 {
        u32::try_from(self.run_time_ticks.unwrap_or_default().max(0) / 10_000_000)
            .unwrap_or(u32::MAX)
    }

    pub fn titre(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    pub fn direct_possible(&self, audio: usize) -> bool {
        const CONTENEURS: [&str; 4] = ["mp4", "m4v", "mov", "webm"];
        const VIDEO: [&str; 3] = ["h264", "vp9", "av1"];
        const SON: [&str; 4] = ["aac", "mp3", "opus", "vorbis"];
        let codec_dans = |p: &Piste, liste: &[&str]| {
            p.codec
                .as_deref()
                .is_some_and(|c| liste.contains(&c.to_ascii_lowercase().as_str()))
        };
        let extension = self
            .chemin()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        audio == 0
            && CONTENEURS.contains(&extension.as_str())
            && self
                .pistes_de("Video")
                .iter()
                .all(|p| codec_dans(p, &VIDEO))
            && self.audio().first().is_none_or(|p| codec_dans(p, &SON))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReponseListe {
    items: Option<Vec<Element>>,
    total_record_count: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Liste {
    pub items: Vec<Element>,
    pub total: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Nom {
    name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReponseNoms {
    items: Option<Vec<Nom>>,
}

#[derive(Debug)]
pub struct Panne;

pub struct Image {
    pub octets: Vec<u8>,
    pub mime: HeaderValue,
}

#[derive(Default)]
struct Images {
    entrees: HashMap<String, Arc<Image>>,
    ordre: VecDeque<String>,
    taille: usize,
}

pub struct Jellyfin {
    http: Client,
    listes: Mutex<HashMap<String, (Instant, Arc<Liste>)>>,
    images: Mutex<Images>,
}

impl Jellyfin {
    pub fn nouveau() -> Self {
        Jellyfin {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .read_timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client HTTP"),
            listes: Mutex::default(),
            images: Mutex::default(),
        }
    }

    pub fn requete(&self, config: &Config, url: Url) -> RequestBuilder {
        self.http
            .get(url)
            .header("Authorization", autorisation(&config.jellyfin_api_key))
    }

    pub fn url(&self, config: &Config, chemin: &str) -> Result<Url, Panne> {
        Url::parse(&format!("{}{chemin}", config.jellyfin_url)).map_err(|_| Panne)
    }

    pub async fn liste(
        &self,
        config: &Config,
        parametres: &[(&str, String)],
    ) -> Result<Liste, Panne> {
        let mut tous: Vec<(&str, &str)> = vec![("Fields", CHAMPS), ("EnableImages", "true")];
        tous.extend(parametres.iter().map(|(k, v)| (*k, v.as_str())));
        let reponse = envoyer(
            self.requete(config, self.url(config, "/Items")?)
                .query(&tous)
                .timeout(DELAI_REQUETE),
        )
        .await
        .map_err(journaliser)?
        .error_for_status()
        .map_err(journaliser)?;
        let octets = lire_borne(reponse, LISTE_MAX).await?;
        let lue: ReponseListe = serde_json::from_slice(&octets).map_err(|_| Panne)?;
        let mut items = lue.items.unwrap_or_default();
        items.retain(|e| id_valide(&e.id));
        Ok(Liste {
            total: lue.total_record_count.unwrap_or(items.len() as i64),
            items,
        })
    }

    pub async fn genres(&self, config: &Config) -> Result<Vec<String>, Panne> {
        let reponse = envoyer(
            self.requete(config, self.url(config, "/Genres")?)
                .query(&[("IncludeItemTypes", "Movie,Series"), ("Recursive", "true")])
                .timeout(DELAI_REQUETE),
        )
        .await
        .map_err(journaliser)?
        .error_for_status()
        .map_err(journaliser)?;
        let octets = lire_borne(reponse, LISTE_MAX).await?;
        let lue: ReponseNoms = serde_json::from_slice(&octets).map_err(|_| Panne)?;
        Ok(lue
            .items
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| n.name)
            .collect())
    }

    pub fn liste_en_cache(&self, cle: &str, duree: Duration) -> Option<Arc<Liste>> {
        let m = self.listes.lock().unwrap_or_else(|e| e.into_inner());
        let (quand, liste) = m.get(cle)?;
        (quand.elapsed() < duree).then(|| liste.clone())
    }

    pub fn garder_liste(&self, cle: String, liste: Arc<Liste>, duree: Duration) {
        let mut m = self.listes.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|_, (quand, _)| quand.elapsed() < duree);
        if m.len() >= CACHE_ENTREES {
            m.clear();
        }
        m.insert(cle, (Instant::now(), liste));
    }

    pub async fn image(
        &self,
        config: &Config,
        id: &str,
        genre: &str,
        largeur: u32,
    ) -> Result<Option<Arc<Image>>, Panne> {
        let cle = format!("{id}|{genre}|{largeur}");
        if let Some(i) = self
            .images
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entrees
            .get(&cle)
        {
            return Ok(Some(i.clone()));
        }
        let reponse = envoyer(
            self.requete(
                config,
                self.url(config, &format!("/Items/{id}/Images/{genre}"))?,
            )
            .query(&[("maxWidth", largeur.to_string()), ("quality", "85".into())])
            .timeout(DELAI_REQUETE),
        )
        .await
        .map_err(journaliser)?;
        if reponse.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let reponse = reponse.error_for_status().map_err(journaliser)?;
        let mime = reponse
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or_default().trim().to_string())
            .filter(|v| ["image/jpeg", "image/png", "image/webp"].contains(&v.as_str()))
            .and_then(|v| HeaderValue::from_str(&v).ok())
            .ok_or(Panne)?;
        let octets = lire_borne(reponse, IMAGE_MAX).await?;
        let image = Arc::new(Image { octets, mime });
        let mut c = self.images.lock().unwrap_or_else(|e| e.into_inner());
        c.taille += image.octets.len();
        c.ordre.push_back(cle.clone());
        c.entrees.insert(cle, image.clone());
        while c.taille > IMAGES_OCTETS {
            let Some(ancienne) = c.ordre.pop_front() else {
                break;
            };
            if let Some(i) = c.entrees.remove(&ancienne) {
                c.taille -= i.octets.len();
            }
        }
        Ok(Some(image))
    }

    pub async fn arreter_transcodage(&self, config: &Config, flux: &str) {
        let Ok(url) = self.url(config, "/Videos/ActiveEncodings") else {
            return;
        };
        let envoi = envoyer(
            self.http
                .delete(url)
                .header("Authorization", autorisation(&config.jellyfin_api_key))
                .query(&[("deviceId", "estompe"), ("playSessionId", flux)])
                .timeout(DELAI_REQUETE),
        )
        .await;
        if let Err(e) = envoi {
            tracing::warn!("films : arrêt du transcodage : {}", e.without_url());
        }
    }
}

fn autorisation(cle: &str) -> String {
    format!("MediaBrowser Token=\"{cle}\"")
}

pub async fn envoyer(requete: RequestBuilder) -> Result<Response, reqwest::Error> {
    journal::sortant("jellyfin", requete).await
}

pub fn id_valide(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn journaliser(e: reqwest::Error) -> Panne {
    tracing::warn!("films : Jellyfin : {}", e.without_url());
    Panne
}

pub async fn lire_borne(mut reponse: Response, max: usize) -> Result<Vec<u8>, Panne> {
    let mut sortie = Vec::new();
    while let Some(m) = reponse.chunk().await.map_err(journaliser)? {
        if sortie.len() + m.len() > max {
            return Err(Panne);
        }
        sortie.extend_from_slice(&m);
    }
    Ok(sortie)
}

#[cfg(test)]
mod tests {
    use super::autorisation;

    #[test]
    fn la_cle_part_dans_l_en_tete_authorization_mediabrowser() {
        assert_eq!(autorisation("abc"), "MediaBrowser Token=\"abc\"");
    }
}
