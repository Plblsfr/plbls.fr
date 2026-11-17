use std::collections::HashMap;

use serde_json::Value;

use crate::{config::Lidarr, quota};

pub const TAILLE_IMAGE_MAX: usize = 10 * 1024 * 1024;
pub const REFUS: &str = "Lidarr a refusé l'ajout";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Lidarr,
    Surcharge,
}

impl Source {
    pub fn code(&self) -> &'static str {
        match self {
            Source::Lidarr => "lidarr",
            Source::Surcharge => "override",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profil {
    pub id: i64,
    pub nom: Option<String>,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReglagesAjout {
    pub racine: String,
    pub source_racine: Source,
    pub qualite: Profil,
    pub metadonnees: Profil,
    pub tags: Vec<i64>,
    pub debit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErreurReglages {
    Incomplet(&'static str),
    Lidarr(String),
}

impl From<String> for ErreurReglages {
    fn from(e: String) -> Self {
        ErreurReglages::Lidarr(e)
    }
}

impl From<ErreurReglages> for String {
    fn from(e: ErreurReglages) -> Self {
        match e {
            ErreurReglages::Incomplet(m) => format!("Lidarr : {m}"),
            ErreurReglages::Lidarr(e) => e,
        }
    }
}

fn plus_utilise<T: PartialEq + Clone>(candidats: &[T], valeurs: &[T]) -> Option<T> {
    let mut meilleur: Option<(&T, usize)> = None;
    for c in candidats {
        let n = valeurs.iter().filter(|v| *v == c).count();
        if n > 0 && meilleur.is_none_or(|(_, m)| n > m) {
            meilleur = Some((c, n));
        }
    }
    meilleur.map(|(c, _)| c.clone())
}

fn chemin(v: &Value, cle: &str) -> Option<String> {
    texte(v, cle)
        .map(|c| c.trim_end_matches('/').to_string())
        .filter(|c| !c.is_empty())
}

fn refus(corps: &str) -> Option<String> {
    let v: Value = serde_json::from_str(corps).ok()?;
    let liste = match v {
        Value::Array(l) => l,
        v => vec![v],
    };
    let mut messages: Vec<String> = liste
        .iter()
        .filter_map(|e| texte(e, "errorMessage"))
        .collect();
    messages.dedup();
    let m: String = messages.join(" ; ").chars().take(300).collect();
    (!m.is_empty()).then(|| format!("{REFUS} : {m}"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtisteLidarr {
    pub id: i64,
    pub mbid: String,
    pub nom: String,
    pub nom_tri: String,
    pub resume: Option<String>,
    pub genres: Vec<String>,
    pub image: Option<String>,
    pub fond: Option<String>,
    pub fichiers: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlbumLidarr {
    pub id: i64,
    pub mbid: String,
    pub titre: String,
    pub type_album: Option<String>,
    pub types_secondaires: Vec<String>,
    pub sortie: Option<chrono::NaiveDate>,
    pub genres: Vec<String>,
    pub pochette: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PisteLidarr {
    pub id: i64,
    pub album_id: i64,
    pub mbid_enregistrement: String,
    pub mbid_piste: Option<String>,
    pub titre: String,
    pub disque: i32,
    pub numero: Option<i32>,
    pub duree: Option<f64>,
    pub explicite: bool,
    pub fichier_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FichierLidarr {
    pub id: i64,
    pub chemin: String,
    pub taille: i64,
}

pub struct Client<'a> {
    lidarr: &'a Lidarr,
    http: &'a reqwest::Client,
}

fn erreur(e: reqwest::Error) -> String {
    e.without_url().to_string()
}

fn entier(v: &Value, cle: &str) -> Option<i64> {
    v[cle].as_i64().filter(|n| *n > 0)
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v[cle]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn chaines(v: &Value) -> Vec<String> {
    let mut l: Vec<String> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| g.as_str().or_else(|| g["name"].as_str()))
        .map(str::trim)
        .filter(|g| !g.is_empty() && g.chars().count() <= 64)
        .map(String::from)
        .collect();
    l.sort();
    l.dedup();
    l
}

fn image(v: &Value, genre: &str) -> Option<String> {
    v["images"]
        .as_array()?
        .iter()
        .find(|i| {
            i["coverType"]
                .as_str()
                .is_some_and(|t| t.eq_ignore_ascii_case(genre))
        })
        .and_then(|i| texte(i, "url").or_else(|| Some(genre.to_string())))
}

fn petit(v: Option<i64>) -> Option<i32> {
    v.and_then(|n| i32::try_from(n).ok())
}

impl<'a> Client<'a> {
    pub fn new(lidarr: &'a Lidarr, http: &'a reqwest::Client) -> Self {
        Self { lidarr, http }
    }

    pub fn requete(&self, methode: reqwest::Method, chemin: &str) -> reqwest::RequestBuilder {
        self.http
            .request(methode, format!("{}/api/v1{chemin}", self.lidarr.url))
            .header("X-Api-Key", &self.lidarr.cle)
    }

    pub async fn envoyer(&self, r: reqwest::RequestBuilder) -> Result<Value, String> {
        let rep = r.send().await.map_err(erreur)?;
        let statut = rep.status();
        if !statut.is_success() {
            let corps: String = rep
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(4000)
                .collect();
            let extrait: String = corps.chars().take(200).collect();
            tracing::warn!("Lidarr : HTTP {} {}", statut.as_u16(), extrait.trim());
            if statut == reqwest::StatusCode::BAD_REQUEST
                && let Some(m) = refus(&corps)
            {
                return Err(m);
            }
            return Err(format!("Lidarr : HTTP {}", statut.as_u16()));
        }
        let octets = rep.bytes().await.map_err(erreur)?;
        if octets.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&octets).map_err(|e| format!("Lidarr : réponse illisible : {e}"))
    }

    pub async fn get(&self, chemin: &str) -> Result<Value, String> {
        self.envoyer(self.requete(reqwest::Method::GET, chemin))
            .await
    }

    pub async fn liste(&self, chemin: &str) -> Result<Vec<Value>, String> {
        match self.get(chemin).await? {
            Value::Array(a) => Ok(a),
            _ => Err(format!("Lidarr : {chemin} : liste attendue")),
        }
    }

    pub async fn reglages_ajout(
        &self,
        surcharge: &quota::Reglages,
    ) -> Result<ReglagesAjout, ErreurReglages> {
        let dossiers = self.liste("/rootfolder").await?;
        let declares: Vec<(&Value, String)> = dossiers
            .iter()
            .filter_map(|d| Some((d, chemin(d, "path")?)))
            .collect();
        let accessibles: Vec<&(&Value, String)> = declares
            .iter()
            .filter(|d| d.0["accessible"] != false)
            .collect();
        let candidats: Vec<String> = if accessibles.is_empty() {
            declares.iter().map(|d| d.1.clone()).collect()
        } else {
            accessibles.iter().map(|d| d.1.clone()).collect()
        };
        let mut artistes: Option<Vec<Value>> = None;
        let (racine, source_racine) = match &surcharge.racine {
            Some(r) => (r.clone(), Source::Surcharge),
            None => {
                let r = match candidats.as_slice() {
                    [] => return Err(ErreurReglages::Incomplet("aucun dossier racine")),
                    [seul] => seul.clone(),
                    _ => {
                        let utilises: Vec<String> = self
                            .artistes_bruts(&mut artistes)
                            .await?
                            .iter()
                            .filter_map(|a| chemin(a, "rootFolderPath"))
                            .collect();
                        plus_utilise(&candidats, &utilises).unwrap_or_else(|| candidats[0].clone())
                    }
                };
                (r, Source::Lidarr)
            }
        };
        let dossier = declares
            .iter()
            .find(|d| d.1 == racine.trim_end_matches('/'))
            .map(|d| d.0);
        let racine = match (&source_racine, dossier.and_then(|d| texte(d, "path"))) {
            (Source::Lidarr, Some(brut)) => brut,
            _ => racine,
        };
        let tags = dossier
            .and_then(|d| d["defaultTags"].as_array())
            .map(|l| l.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();
        let qualites = self.liste("/qualityprofile").await?;
        let qualite = self
            .profil(
                &qualites,
                surcharge.profil_qualite,
                dossier.and_then(|d| entier(d, "defaultQualityProfileId")),
                "qualityProfileId",
                &mut artistes,
            )
            .await?
            .ok_or(ErreurReglages::Incomplet("aucun profil de qualité"))?;
        let metas = self.liste("/metadataprofile").await?;
        let metadonnees = self
            .profil(
                &metas,
                surcharge.profil_metadonnees,
                dossier.and_then(|d| entier(d, "defaultMetadataProfileId")),
                "metadataProfileId",
                &mut artistes,
            )
            .await?
            .ok_or(ErreurReglages::Incomplet("aucun profil de métadonnées"))?;
        Ok(ReglagesAjout {
            debit: quota::debit_profil(&qualites, qualite.id),
            racine,
            source_racine,
            qualite,
            metadonnees,
            tags,
        })
    }

    async fn artistes_bruts<'c>(
        &self,
        cache: &'c mut Option<Vec<Value>>,
    ) -> Result<&'c [Value], String> {
        if cache.is_none() {
            *cache = Some(self.liste("/artist").await?);
        }
        Ok(cache.as_deref().unwrap_or_default())
    }

    async fn profil(
        &self,
        profils: &[Value],
        surcharge: Option<i64>,
        defaut: Option<i64>,
        cle: &str,
        artistes: &mut Option<Vec<Value>>,
    ) -> Result<Option<Profil>, String> {
        let ids: Vec<i64> = profils.iter().filter_map(|p| entier(p, "id")).collect();
        let nom = |id: i64| {
            profils
                .iter()
                .find(|p| entier(p, "id") == Some(id))
                .and_then(|p| texte(p, "name"))
        };
        if let Some(id) = surcharge {
            return Ok(Some(Profil {
                id,
                nom: nom(id),
                source: Source::Surcharge,
            }));
        }
        let id = match defaut.filter(|d| ids.contains(d)) {
            Some(d) => Some(d),
            None if ids.is_empty() => None,
            None => {
                let utilises: Vec<i64> = self
                    .artistes_bruts(artistes)
                    .await?
                    .iter()
                    .filter_map(|a| entier(a, cle))
                    .collect();
                plus_utilise(&ids, &utilises).or_else(|| ids.first().copied())
            }
        };
        Ok(id.map(|id| Profil {
            id,
            nom: nom(id),
            source: Source::Lidarr,
        }))
    }

    pub async fn version(&self) -> Result<String, String> {
        let v = self.get("/system/status").await?;
        texte(&v, "version").ok_or_else(|| "Lidarr : version absente".into())
    }

    pub async fn artistes(&self) -> Result<Vec<ArtisteLidarr>, String> {
        Ok(self
            .liste("/artist")
            .await?
            .iter()
            .filter_map(|a| {
                let nom = texte(a, "artistName")?;
                Some(ArtisteLidarr {
                    id: entier(a, "id")?,
                    mbid: texte(a, "foreignArtistId")?,
                    nom_tri: texte(a, "sortName").unwrap_or_else(|| nom.clone()),
                    nom,
                    resume: texte(a, "overview"),
                    genres: chaines(&a["genres"]),
                    image: image(a, "poster"),
                    fond: image(a, "fanart"),
                    fichiers: a["statistics"]["trackFileCount"].as_i64(),
                })
            })
            .collect())
    }

    pub async fn albums(&self, artiste: i64) -> Result<Vec<AlbumLidarr>, String> {
        Ok(self
            .liste(&format!("/album?artistId={artiste}"))
            .await?
            .iter()
            .filter_map(|a| {
                Some(AlbumLidarr {
                    id: entier(a, "id")?,
                    mbid: texte(a, "foreignAlbumId")?,
                    titre: texte(a, "title")?,
                    type_album: texte(a, "albumType"),
                    types_secondaires: chaines(&a["secondaryTypes"]),
                    sortie: a["releaseDate"]
                        .as_str()
                        .and_then(|d| d.get(..10))
                        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
                    genres: chaines(&a["genres"]),
                    pochette: image(a, "cover"),
                })
            })
            .collect())
    }

    pub async fn pistes(&self, artiste: i64) -> Result<Vec<PisteLidarr>, String> {
        Ok(self
            .liste(&format!("/track?artistId={artiste}"))
            .await?
            .iter()
            .filter_map(|p| {
                let numero = petit(entier(p, "absoluteTrackNumber")).or_else(|| {
                    p["trackNumber"]
                        .as_str()
                        .and_then(|n| n.trim().parse::<i32>().ok())
                });
                Some(PisteLidarr {
                    id: entier(p, "id")?,
                    album_id: entier(p, "albumId")?,
                    mbid_enregistrement: texte(p, "foreignRecordingId")?,
                    mbid_piste: texte(p, "foreignTrackId"),
                    titre: texte(p, "title").unwrap_or_else(|| "Sans titre".into()),
                    disque: petit(entier(p, "mediumNumber")).unwrap_or(1),
                    numero,
                    duree: entier(p, "duration").map(|ms| ms as f64 / 1000.0),
                    explicite: p["explicit"] == true,
                    fichier_id: (p["hasFile"] == true)
                        .then(|| entier(p, "trackFileId"))
                        .flatten(),
                })
            })
            .collect())
    }

    pub async fn fichiers(&self, artiste: i64) -> Result<HashMap<i64, FichierLidarr>, String> {
        Ok(self
            .liste(&format!("/trackfile?artistId={artiste}"))
            .await?
            .iter()
            .filter_map(|f| {
                let id = entier(f, "id")?;
                Some((
                    id,
                    FichierLidarr {
                        id,
                        chemin: texte(f, "path")?,
                        taille: f["size"].as_i64().unwrap_or(0),
                    },
                ))
            })
            .collect())
    }

    pub async fn image(&self, chemin: &str) -> Result<Option<(String, Vec<u8>)>, String> {
        let rep = self
            .requete(reqwest::Method::GET, &format!("/mediacover/{chemin}"))
            .send()
            .await
            .map_err(erreur)?;
        if rep.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !rep.status().is_success() {
            return Err(format!("Lidarr : HTTP {}", rep.status().as_u16()));
        }
        let mime = rep
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !["image/jpeg", "image/png", "image/gif", "image/webp"].contains(&mime.as_str()) {
            return Ok(None);
        }
        if rep
            .content_length()
            .is_some_and(|n| n > TAILLE_IMAGE_MAX as u64)
        {
            return Ok(None);
        }
        let octets = rep.bytes().await.map_err(erreur)?;
        if octets.is_empty() || octets.len() > TAILLE_IMAGE_MAX {
            return Ok(None);
        }
        Ok(Some((mime, octets.to_vec())))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testutil::faux;

    #[tokio::test]
    async fn lecture_de_l_api_v1() {
        let s = faux(|r| {
            let corps = |v: Value| (200, v.to_string().into_bytes());
            match (r.chemin.as_str(), r.query.as_str()) {
                ("/api/v1/system/status", _) => corps(json!({"version": "2.9.6"})),
                ("/api/v1/artist", _) => corps(json!([
                    {"id": 3, "artistName": "Beyoncé", "foreignArtistId": "mb-a", "genres": ["Pop", "R&B", "Pop"],
                     "images": [{"coverType": "poster", "url": "/MediaCover/3/poster.jpg?lastWrite=1"}],
                     "statistics": {"trackFileCount": 2}},
                    {"id": 4, "artistName": "Sans MBID"}
                ])),
                ("/api/v1/album", "artistId=3") => corps(json!([
                    {"id": 7, "foreignAlbumId": "mb-al", "title": "Lemonade", "albumType": "Album",
                     "secondaryTypes": [{"id": 1, "name": "Live"}], "releaseDate": "2016-04-23T00:00:00Z",
                     "images": [{"coverType": "Cover", "url": "/MediaCover/Albums/7/cover.jpg"}]}
                ])),
                ("/api/v1/track", "artistId=3") => corps(json!([
                    {"id": 11, "albumId": 7, "foreignRecordingId": "mb-r", "title": "Hold Up", "trackNumber": "2",
                     "mediumNumber": 1, "duration": 221000, "hasFile": true, "trackFileId": 21},
                    {"id": 12, "albumId": 7, "foreignRecordingId": "mb-r2", "title": "B", "absoluteTrackNumber": 3,
                     "mediumNumber": 2, "hasFile": false, "trackFileId": 0},
                    {"id": 13, "albumId": 7, "title": "Sans enregistrement"}
                ])),
                ("/api/v1/trackfile", "artistId=3") => {
                    corps(json!([{"id": 21, "path": "/music/a.flac", "size": 42}, {"id": 22}]))
                }
                _ => (404, vec![]),
            }
        })
        .await;
        let l = Lidarr {
            url: s.url.clone(),
            cle: "cle".into(),
        };
        let http = reqwest::Client::new();
        let c = Client::new(&l, &http);
        assert_eq!(c.version().await.unwrap(), "2.9.6");
        let a = c.artistes().await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].genres, ["Pop", "R&B"]);
        assert_eq!(a[0].nom_tri, "Beyoncé");
        assert_eq!(
            a[0].image.as_deref(),
            Some("/MediaCover/3/poster.jpg?lastWrite=1")
        );
        assert_eq!(a[0].fond, None);
        let al = c.albums(3).await.unwrap();
        assert_eq!(al[0].types_secondaires, ["Live"]);
        assert_eq!(al[0].sortie, chrono::NaiveDate::from_ymd_opt(2016, 4, 23));
        assert!(al[0].pochette.is_some());
        let p = c.pistes(3).await.unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            (p[0].numero, p[0].duree, p[0].fichier_id),
            (Some(2), Some(221.0), Some(21))
        );
        assert_eq!(
            (p[1].numero, p[1].disque, p[1].fichier_id),
            (Some(3), 2, None)
        );
        let f = c.fichiers(3).await.unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[&21].taille, 42);
        assert!(s.requetes().iter().all(|r| r.entete("x-api-key") == "cle"));
    }

    #[test]
    fn refus_de_validation_et_plus_utilise() {
        let corps = r#"[{"propertyName":"RootFolderPath","errorMessage":"Root folder '/music' does not exist","attemptedValue":"/music","severity":"error"}]"#;
        assert_eq!(
            refus(corps).as_deref(),
            Some("Lidarr a refusé l'ajout : Root folder '/music' does not exist")
        );
        assert_eq!(
            refus(r#"{"message": "x", "errorMessage": "Profil inconnu"}"#).as_deref(),
            Some("Lidarr a refusé l'ajout : Profil inconnu")
        );
        assert_eq!(refus("panne"), None);
        assert_eq!(refus("[]"), None);
        assert_eq!(plus_utilise(&[1, 4], &[4, 4, 1, 9]), Some(4));
        assert_eq!(plus_utilise(&[1, 4], &[4, 1]), Some(1));
        assert_eq!(plus_utilise(&[1, 4], &[9]), None);
    }

    #[tokio::test]
    async fn erreur_lisible_et_image_filtree() {
        let s = crate::testutil::faux_avec_entetes(|r| match r.chemin.as_str() {
            "/api/v1/mediacover/artist/3/poster-250.jpg" => (
                200,
                vec![("content-type", "image/jpeg".into())],
                b"IMG".to_vec(),
            ),
            "/api/v1/mediacover/artist/3/fanart-180.jpg" => (
                200,
                vec![("content-type", "text/html".into())],
                b"<html>".to_vec(),
            ),
            _ => (500, vec![], vec![]),
        })
        .await;
        let l = Lidarr {
            url: s.url.clone(),
            cle: "cle".into(),
        };
        let http = reqwest::Client::new();
        let c = Client::new(&l, &http);
        assert_eq!(c.artistes().await, Err("Lidarr : HTTP 500".into()));
        assert_eq!(
            c.image("artist/3/poster-250.jpg").await,
            Ok(Some(("image/jpeg".into(), b"IMG".to_vec())))
        );
        assert_eq!(c.image("artist/3/fanart-180.jpg").await, Ok(None));
        let l = Lidarr {
            url: "http://127.0.0.1:9".into(),
            cle: "cle".into(),
        };
        let c = Client::new(&l, &http);
        assert!(c.version().await.is_err());
    }
}
