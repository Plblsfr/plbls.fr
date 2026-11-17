use std::{collections::HashMap, io::Cursor, time::Duration};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::Response,
};
use image::{ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    etat::AppState,
    http::{self, bd, introuvable, invalide},
    lidarr,
};

pub const IMAGE_AJOUT_MAX: usize = 5 << 20;
const TELECHARGEMENTS: usize = 4;
const URL_MAX: usize = 2048;
const COTE_MAX: u32 = 8000;
const AGENT: &str = concat!("Sillon/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sujet {
    Portrait,
    Fond,
    Pochette,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taille {
    Petite,
    Moyenne,
    Grande,
}

impl Taille {
    pub fn lire(texte: Option<&str>) -> Option<Self> {
        match texte {
            None | Some("") | Some("small") => Some(Taille::Petite),
            Some("medium") => Some(Taille::Moyenne),
            Some("large") => Some(Taille::Grande),
            Some(_) => None,
        }
    }

    const TOUTES: [Taille; 3] = [Taille::Petite, Taille::Moyenne, Taille::Grande];

    fn cote(self) -> u32 {
        match self {
            Taille::Petite => 250,
            Taille::Moyenne => 500,
            Taille::Grande => 1200,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Taille::Petite => "small",
            Taille::Moyenne => "medium",
            Taille::Grande => "large",
        }
    }
}

impl Sujet {
    fn code(self) -> &'static str {
        match self {
            Sujet::Portrait => "poster",
            Sujet::Fond => "fanart",
            Sujet::Pochette => "cover",
        }
    }

    pub fn chemin_lidarr(self, lidarr_id: i64, taille: Taille) -> String {
        let genre = if self == Sujet::Pochette {
            "album"
        } else {
            "artist"
        };
        let suffixe = match (self, taille) {
            (_, Taille::Grande) => "",
            (Sujet::Fond, Taille::Petite) => "-180",
            (Sujet::Fond, Taille::Moyenne) => "-360",
            (_, Taille::Petite) => "-250",
            (_, Taille::Moyenne) => "-500",
        };
        format!("{genre}/{lidarr_id}/{}{suffixe}.jpg", self.code())
    }
}

fn cle(sujet: Sujet, id: Uuid, taille: Taille) -> String {
    format!("{}/{id}/{}", sujet.code(), taille.code())
}

pub async fn lire(
    state: &AppState,
    sujet: Sujet,
    id: Uuid,
    lidarr_id: i64,
    version: &str,
    taille: Taille,
) -> Result<Option<(String, Vec<u8>)>, Response> {
    let c = cle(sujet, id, taille);
    let en_cache = sqlx::query("SELECT mime, octets, version FROM images WHERE cle = $1")
        .bind(&c)
        .fetch_optional(state.pool())
        .await
        .map_err(http::bd)?;
    if let Some(l) = &en_cache
        && l.get::<String, _>(2) == version
    {
        return Ok(Some((l.get(0), l.get(1))));
    }
    let Some(cfg) = state.config().lidarr.as_ref() else {
        return Ok(en_cache.map(|l| (l.get(0), l.get(1))));
    };
    match lidarr::Client::new(cfg, state.http())
        .image(&sujet.chemin_lidarr(lidarr_id, taille))
        .await
    {
        Ok(Some((mime, octets))) => {
            sqlx::query(
                "INSERT INTO images (cle, version, mime, octets) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (cle) DO UPDATE SET version = EXCLUDED.version, mime = EXCLUDED.mime,
                   octets = EXCLUDED.octets, cree_le = now()",
            )
            .bind(&c)
            .bind(version)
            .bind(&mime)
            .bind(&octets)
            .execute(state.pool())
            .await
            .map_err(http::bd)?;
            Ok(Some((mime, octets)))
        }
        Ok(None) => Ok(None),
        Err(e) => match en_cache {
            Some(l) => Ok(Some((l.get(0), l.get(1)))),
            None => {
                tracing::warn!("image : {e}");
                Err(http::indisponible(
                    "lidarr_unavailable",
                    "images injoignables",
                ))
            }
        },
    }
}

pub async fn precharger(state: &AppState, sujet: Sujet, id: Uuid, lidarr_id: i64, version: &str) {
    if let Err(r) = lire(state, sujet, id, lidarr_id, version, Taille::Petite).await {
        tracing::debug!("préchargement d'image : HTTP {}", r.status());
    }
}

pub fn reponse(mime: String, octets: Vec<u8>) -> Response {
    let mut r = Response::new(Body::from(octets));
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("image/jpeg")),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    r
}

pub struct Distantes {
    client: reqwest::Client,
    prives: bool,
    places: Semaphore,
}

impl Distantes {
    pub fn nouveau(prives: bool) -> Self {
        Self {
            client: commun::http::client_public(Duration::from_secs(10), prives),
            prives,
            places: Semaphore::new(TELECHARGEMENTS),
        }
    }

    async fn telecharger(&self, brut: &str) -> Option<Vec<u8>> {
        let u = reqwest::Url::parse(brut).ok()?;
        if !commun::http::url_publique(&u, self.prives) {
            tracing::debug!("image d'ajout : adresse refusée");
            return None;
        }
        let _place = self.places.acquire().await.ok()?;
        let mut rep = match self
            .client
            .get(u)
            .header(header::USER_AGENT, AGENT)
            .header(header::ACCEPT, "image/jpeg,image/png,image/webp,image/gif")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("image d'ajout : {}", commun::http::cause(&e));
                return None;
            }
        };
        if !rep.status().is_success()
            || rep
                .content_length()
                .is_some_and(|n| n > IMAGE_AJOUT_MAX as u64)
        {
            return None;
        }
        let mut corps = Vec::new();
        while let Some(morceau) = rep.chunk().await.ok()? {
            corps.extend_from_slice(&morceau);
            if corps.len() > IMAGE_AJOUT_MAX {
                return None;
            }
        }
        format_image(&corps).map(|_| corps)
    }
}

pub fn format_image(octets: &[u8]) -> Option<ImageFormat> {
    image::guess_format(octets).ok().filter(|f| {
        matches!(
            f,
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP
        )
    })
}

fn declinaisons(octets: &[u8]) -> Option<Vec<(Taille, &'static str, Vec<u8>)>> {
    let mut lecteur = ImageReader::with_format(Cursor::new(octets), format_image(octets)?);
    let mut limites = Limits::default();
    limites.max_image_width = Some(COTE_MAX);
    limites.max_image_height = Some(COTE_MAX);
    limites.max_alloc = Some(256 << 20);
    lecteur.limits(limites);
    let image = lecteur.decode().ok()?;
    Taille::TOUTES
        .into_iter()
        .map(|t| {
            let n = t.cote();
            let r = if image.width().max(image.height()) > n {
                image.resize(n, n, FilterType::Triangle)
            } else {
                image.clone()
            };
            let mut sortie = Cursor::new(Vec::new());
            let mime = if r.color().has_alpha() {
                r.write_to(&mut sortie, ImageFormat::Png).ok()?;
                "image/png"
            } else {
                r.to_rgb8()
                    .write_with_encoder(JpegEncoder::new_with_quality(&mut sortie, 85))
                    .ok()?;
                "image/jpeg"
            };
            Some((t, mime, sortie.into_inner()))
        })
        .collect()
}

pub fn image_distante(v: &Value, genres: &[&str]) -> Option<String> {
    let absolue = |i: &Value, cle: &str| {
        i[cle]
            .as_str()
            .map(str::trim)
            .filter(|u| u.len() <= URL_MAX)
            .filter(|u| {
                reqwest::Url::parse(u).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
            })
            .map(String::from)
    };
    let images = v["images"].as_array()?;
    genres.iter().find_map(|g| {
        images
            .iter()
            .filter(|i| {
                i["coverType"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case(g))
            })
            .find_map(|i| absolue(i, "remoteUrl").or_else(|| absolue(i, "url")))
    })
}

pub fn reference(url: &str) -> String {
    Sha256::digest(url.as_bytes())[..16]
        .iter()
        .map(|o| format!("{o:02x}"))
        .collect()
}

pub async fn enregistrer(pool: &PgPool, urls: &[String]) -> Result<(), sqlx::Error> {
    let uniques: HashMap<String, &String> = urls.iter().map(|u| (reference(u), u)).collect();
    if uniques.is_empty() {
        return Ok(());
    }
    let (ids, urls): (Vec<String>, Vec<String>) =
        uniques.into_iter().map(|(i, u)| (i, u.clone())).unzip();
    sqlx::query(
        "INSERT INTO images_ajout (id, url) SELECT * FROM unnest($1::text[], $2::text[])
         ON CONFLICT (id) DO UPDATE SET vue_le = now()",
    )
    .bind(&ids)
    .bind(&urls)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn purger(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM images_ajout i WHERE i.vue_le < now() - interval '7 days'
           AND NOT EXISTS (SELECT 1 FROM ajouts a WHERE a.image = i.id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM images m WHERE m.cle LIKE 'ajout/%'
           AND NOT EXISTS (SELECT 1 FROM images_ajout i WHERE i.id = split_part(m.cle, '/', 2))",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn cle_ajout(id: &str, taille: Taille) -> String {
    format!("ajout/{id}/{}", taille.code())
}

pub async fn image_ajout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, Response> {
    let taille = Taille::lire(q.get("size").map(String::as_str))
        .ok_or_else(|| invalide("taille non proposée"))?;
    if id.len() != 32 || !id.bytes().all(|o| matches!(o, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(introuvable());
    }
    let pool = state.pool();
    let en_cache = sqlx::query("SELECT mime, octets FROM images WHERE cle = $1")
        .bind(cle_ajout(&id, taille))
        .fetch_optional(pool)
        .await
        .map_err(bd)?;
    if let Some(l) = en_cache {
        return Ok(reponse(l.get(0), l.get(1)));
    }
    let url: String = sqlx::query_scalar(
        "SELECT url FROM images_ajout i WHERE i.id = $1
           AND (i.vue_le > now() - interval '7 days' OR EXISTS (SELECT 1 FROM ajouts a WHERE a.image = i.id))",
    )
    .bind(&id)
    .fetch_optional(pool)
    .await
    .map_err(bd)?
    .ok_or_else(introuvable)?;
    let octets = state
        .images()
        .telecharger(&url)
        .await
        .ok_or_else(introuvable)?;
    let variantes = tokio::task::spawn_blocking(move || declinaisons(&octets))
        .await
        .ok()
        .flatten()
        .ok_or_else(introuvable)?;
    let mut servie = None;
    for (t, mime, octets) in variantes {
        sqlx::query(
            "INSERT INTO images (cle, version, mime, octets) VALUES ($1, '', $2, $3)
             ON CONFLICT (cle) DO UPDATE SET mime = EXCLUDED.mime, octets = EXCLUDED.octets, cree_le = now()",
        )
        .bind(cle_ajout(&id, t))
        .bind(mime)
        .bind(&octets)
        .execute(pool)
        .await
        .map_err(bd)?;
        if t == taille {
            servie = Some((mime.to_string(), octets));
        }
    }
    let (mime, octets) = servie.ok_or_else(introuvable)?;
    Ok(reponse(mime, octets))
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{DynamicImage, GenericImageView, Rgb, RgbImage, Rgba, RgbaImage};
    use serde_json::json;

    use super::*;

    pub(crate) fn image_de_test(largeur: u32, hauteur: u32, format: ImageFormat) -> Vec<u8> {
        let image = if format == ImageFormat::Png {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                largeur,
                hauteur,
                Rgba([20, 120, 200, 128]),
            ))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(largeur, hauteur, Rgb([200, 40, 40])))
        };
        let mut sortie = Cursor::new(Vec::new());
        image.write_to(&mut sortie, format).unwrap();
        sortie.into_inner()
    }

    pub(crate) fn dimensions(octets: &[u8]) -> (u32, u32) {
        image::load_from_memory(octets).unwrap().dimensions()
    }

    #[test]
    fn image_distante_prend_remote_url_puis_url_absolue_du_bon_type() {
        let v = json!({"images": [
            {"coverType": "fanart", "url": "/MediaCover/1/fanart.jpg", "remoteUrl": "https://img.exemple/fanart.jpg"},
            {"coverType": "Poster", "url": "/MediaCover/1/poster.jpg", "remoteUrl": "https://img.exemple/poster.jpg"},
        ]});
        assert_eq!(
            image_distante(&v, &["poster", "fanart"]).as_deref(),
            Some("https://img.exemple/poster.jpg")
        );
        assert_eq!(
            image_distante(&v, &["banner", "fanart"]).as_deref(),
            Some("https://img.exemple/fanart.jpg")
        );
        let v = json!({"images": [{"coverType": "cover", "url": "http://img.exemple/c.jpg"}]});
        assert_eq!(
            image_distante(&v, &["cover"]).as_deref(),
            Some("http://img.exemple/c.jpg")
        );
        for sans in [
            json!({}),
            json!({"images": "x"}),
            json!({"images": [{"coverType": "cover", "url": "/MediaCover/2/cover.jpg"}]}),
            json!({"images": [{"coverType": "cover", "remoteUrl": "ftp://img.exemple/c.jpg"}]}),
            json!({"images": [{"coverType": "cover", "remoteUrl": "javascript:alert(1)"}]}),
            json!({"images": [{"coverType": "cover", "remoteUrl": format!("https://img.exemple/{}", "a".repeat(3000))}]}),
            json!({"images": [{"coverType": "banner", "remoteUrl": "https://img.exemple/b.jpg"}]}),
        ] {
            assert_eq!(image_distante(&sans, &["cover"]), None, "{sans}");
        }
        let r = reference("https://img.exemple/poster.jpg");
        assert_eq!(r.len(), 32);
        assert!(
            r.bytes()
                .all(|o| o.is_ascii_hexdigit() && !o.is_ascii_uppercase())
        );
        assert_eq!(r, reference("https://img.exemple/poster.jpg"));
        assert_ne!(r, reference("https://img.exemple/fanart.jpg"));
    }

    #[test]
    fn declinaisons_reduites_sans_agrandir_et_formats_refuses() {
        let v = declinaisons(&image_de_test(1600, 900, ImageFormat::Jpeg)).unwrap();
        let tailles: Vec<(Taille, &str, (u32, u32))> =
            v.iter().map(|(t, m, o)| (*t, *m, dimensions(o))).collect();
        assert_eq!(
            tailles,
            [
                (Taille::Petite, "image/jpeg", (250, 141)),
                (Taille::Moyenne, "image/jpeg", (500, 281)),
                (Taille::Grande, "image/jpeg", (1200, 675)),
            ]
        );
        let v = declinaisons(&image_de_test(300, 300, ImageFormat::Png)).unwrap();
        assert_eq!(
            v.iter()
                .map(|(_, m, o)| (*m, dimensions(o)))
                .collect::<Vec<_>>(),
            [
                ("image/png", (250, 250)),
                ("image/png", (300, 300)),
                ("image/png", (300, 300)),
            ]
        );
        assert!(declinaisons(&image_de_test(COTE_MAX + 1, 2, ImageFormat::Png)).is_none());
        assert!(declinaisons(b"<html>pas une image</html>").is_none());
        assert!(declinaisons(b"\xff\xd8\xff\xe0tronque").is_none());
        assert_eq!(format_image(b"<html>"), None);
        assert_eq!(
            format_image(&image_de_test(2, 2, ImageFormat::Gif)),
            Some(ImageFormat::Gif)
        );
    }

    #[test]
    fn chemins_et_tailles_de_lidarr() {
        assert_eq!(
            Sujet::Portrait.chemin_lidarr(3, Taille::Petite),
            "artist/3/poster-250.jpg"
        );
        assert_eq!(
            Sujet::Fond.chemin_lidarr(3, Taille::Moyenne),
            "artist/3/fanart-360.jpg"
        );
        assert_eq!(
            Sujet::Pochette.chemin_lidarr(9, Taille::Grande),
            "album/9/cover.jpg"
        );
        assert_eq!(Taille::lire(None), Some(Taille::Petite));
        assert_eq!(Taille::lire(Some("large")), Some(Taille::Grande));
        assert_eq!(Taille::lire(Some("huge")), None);
    }
}
