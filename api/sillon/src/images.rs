use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::Response,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{etat::AppState, http, lidarr};

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

#[cfg(test)]
mod tests {
    use super::*;

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
