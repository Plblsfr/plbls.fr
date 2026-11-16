use std::{pin::Pin, sync::Arc, time::Duration};

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use estompe_core::{
    http::{write_err, write_json},
    messages::{conv_du_message, id_message},
    servers::uuid_valide,
    state::{AppState, MusiqueResolue, Musiques, RefusMusique},
    users::User,
};
use reqwest::Url;
use serde_json::{Value, json};

const GENRES: [&str; 4] = ["album", "artist", "track", "playlist"];

pub struct Sillon {
    url: String,
    jeton: String,
    http: reqwest::Client,
}

fn borne(v: &Value) -> String {
    v.as_str().unwrap_or_default().chars().take(300).collect()
}

fn resolue(v: &Value, genre: &str, id: &str) -> Option<MusiqueResolue> {
    if v["type"] != genre || v["id"] != id {
        return None;
    }
    let image = match (v["cover"]["kind"].as_str(), v["cover"]["id"].as_str()) {
        (Some(k @ ("album" | "artist")), Some(i)) if uuid_valide(i) => {
            Some((k.to_string(), i.to_string()))
        }
        _ => None,
    };
    Some(MusiqueResolue {
        genre: genre.to_string(),
        id: id.to_string(),
        titre: borne(&v["title"]),
        artiste: borne(&v["artist"]),
        album: v["album"]
            .as_str()
            .filter(|a| uuid_valide(a))
            .map(String::from),
        image,
    })
}

impl Sillon {
    pub fn nouveau(url: &str, jeton: &str) -> Self {
        Sillon {
            url: url.trim_end_matches('/').to_string(),
            jeton: jeton.to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client HTTP"),
        }
    }

    async fn chercher(
        &self,
        sub: &str,
        genre: &str,
        id: &str,
    ) -> Result<MusiqueResolue, RefusMusique> {
        if !uuid_valide(id) || !["album", "artist", "track"].contains(&genre) {
            return Err(RefusMusique::Introuvable);
        }
        let rep = self
            .http
            .get(format!("{}/internal/preview", self.url))
            .query(&[("type", genre), ("id", id), ("sub", sub)])
            .bearer_auth(&self.jeton)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("carte Sillon : Sillon injoignable : {}", e.without_url());
                RefusMusique::Indisponible
            })?;
        match rep.status().as_u16() {
            200 => {}
            403 => return Err(RefusMusique::Permission),
            404 => return Err(RefusMusique::Introuvable),
            s => {
                tracing::warn!("carte Sillon : Sillon a répondu HTTP {s}");
                return Err(RefusMusique::Indisponible);
            }
        }
        let octets = rep.bytes().await.map_err(|_| RefusMusique::Indisponible)?;
        if octets.len() > 1 << 16 {
            return Err(RefusMusique::Indisponible);
        }
        let v: Value = serde_json::from_slice(&octets).map_err(|_| RefusMusique::Indisponible)?;
        resolue(&v, genre, id).ok_or(RefusMusique::Indisponible)
    }
}

impl Musiques for Sillon {
    fn apercu<'a>(
        &'a self,
        sub: &'a str,
        genre: &'a str,
        id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<MusiqueResolue, RefusMusique>> + Send + 'a>> {
        Box::pin(self.chercher(sub, genre, id))
    }
}

pub fn brancher(state: &AppState) {
    let c = state.config();
    if c.sillon_actif() {
        state.definir_musiques(Arc::new(Sillon::nouveau(&c.sillon_url, &c.sillon_token)));
    }
}

pub fn lien(brut: &str, origines: &[String]) -> Option<(&'static str, String)> {
    let u = Url::parse(brut.trim()).ok()?;
    let origine = u.origin().ascii_serialization();
    if !origines.iter().any(|o| o.trim_end_matches('/') == origine)
        || !matches!(u.path(), "" | "/")
        || u.query().is_some()
    {
        return None;
    }
    let mut parties = u.fragment()?.strip_prefix("/musique/")?.split('/');
    let (section, id) = (parties.next()?, parties.next()?);
    if parties.next().is_some() || !uuid_valide(id) {
        return None;
    }
    let genre = GENRES.into_iter().find(|g| *g == section)?;
    Some((genre, id.to_ascii_lowercase()))
}

fn sans_carte() -> Response {
    write_err(
        StatusCode::NOT_FOUND,
        "no_preview",
        "pas d'aperçu pour ce lien",
    )
}

async fn auteur(state: &AppState, message: &str, lecteur: &User, url: &str) -> Option<String> {
    let id = id_message(message).ok()?;
    conv_du_message(state, id, &lecteur.id).await.ok()?;
    let (contenu, sub) = sqlx::query_as::<_, (String, String)>(
        "SELECT m.content, u.keycloak_sub FROM messages m JOIN users u ON u.id = m.author_id WHERE m.id = $1",
    )
    .bind(id)
    .fetch_optional(state.pool().ok()?)
    .await
    .ok()??;
    contenu.contains(url.trim()).then_some(sub)
}

pub async fn carte(
    state: &AppState,
    musiques: &dyn Musiques,
    lecteur: &User,
    (genre, id): (&str, String),
    message: &str,
    url: &str,
) -> Response {
    if genre == "playlist" {
        return sans_carte();
    }
    let Some(sub) = auteur(state, message, lecteur, url).await else {
        return sans_carte();
    };
    let Ok(m) = musiques.apercu(&sub, genre, &id).await else {
        return sans_carte();
    };
    let ecoute = sub == lecteur.sub || musiques.apercu(&lecteur.sub, genre, &id).await.is_ok();
    let image = m.image.as_ref().map(|(k, i)| json!({ "kind": k, "id": i }));
    let mut r = write_json(
        StatusCode::OK,
        &json!({
            "url": url.trim(),
            "title": m.titre,
            "description": m.artiste,
            "site_name": "Sillon",
            "sillon": {
                "type": m.genre,
                "id": m.id,
                "title": m.titre,
                "artist": m.artiste,
                "album": m.album,
                "cover": image,
                "listen": ecoute,
            },
        }),
    );
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=600"),
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_core::testutil::faux;

    const ID: &str = "11111111-2222-3333-4444-555555555555";
    const AUTRE: &str = "11111111-2222-3333-4444-666666666666";

    #[test]
    fn lien_de_la_section_musique_de_l_app() {
        let o = ["https://estompe.plbls.fr/".to_string()];
        assert_eq!(
            lien(
                &format!("https://estompe.plbls.fr/#/musique/album/{ID}"),
                &o
            ),
            Some(("album", ID.into()))
        );
        assert_eq!(
            lien(
                &format!(
                    " https://estompe.plbls.fr#/musique/artist/{}",
                    ID.to_uppercase()
                ),
                &o
            ),
            Some(("artist", ID.into()))
        );
        assert_eq!(
            lien(
                &format!("https://estompe.plbls.fr/#/musique/playlist/{ID}"),
                &o
            ),
            Some(("playlist", ID.into()))
        );
        for faux in [
            format!("https://autre.fr/#/musique/album/{ID}"),
            format!("http://estompe.plbls.fr/#/musique/album/{ID}"),
            format!("https://estompe.plbls.fr:8443/#/musique/album/{ID}"),
            format!("https://estompe.plbls.fr/x/#/musique/album/{ID}"),
            format!("https://estompe.plbls.fr/?a=1#/musique/album/{ID}"),
            format!("https://estompe.plbls.fr/#/musique/album/{ID}/plus"),
            format!("https://estompe.plbls.fr/#/films/album/{ID}"),
            format!("https://estompe.plbls.fr/#/musique/genre/{ID}"),
            "https://estompe.plbls.fr/#/musique/album/../admin".into(),
            "https://estompe.plbls.fr/#/musique/album".into(),
            "pas une url".into(),
        ] {
            assert_eq!(lien(&faux, &o), None, "{faux}");
        }
        assert_eq!(
            lien(
                &format!("https://estompe.plbls.fr/#/musique/album/{ID}"),
                &[]
            ),
            None
        );
    }

    #[tokio::test]
    async fn apercu_demande_a_sillon_avec_le_jeton_de_service() {
        let s = faux(|r| {
            if r.entete("authorization") != "Bearer jeton" {
                return (401, vec![]);
            }
            match r.query.as_str() {
                q if q == format!("type=album&id={ID}&sub=alice") => (
                    200,
                    format!(r#"{{"type":"album","id":"{ID}","title":"Lemonade","artist":"Beyoncé","album":"{ID}","cover":{{"kind":"album","id":"{ID}"}}}}"#)
                        .into_bytes(),
                ),
                q if q == format!("type=artist&id={ID}&sub=alice") => (
                    200,
                    format!(r#"{{"type":"artist","id":"{ID}","title":"B","artist":"B","album":null,"cover":{{"kind":"autre","id":"/x"}}}}"#)
                        .into_bytes(),
                ),
                q if q == format!("type=track&id={ID}&sub=alice") => (
                    200,
                    format!(r#"{{"type":"track","id":"{AUTRE}","title":"x"}}"#).into_bytes(),
                ),
                q if q.ends_with("sub=bob") => (403, vec![]),
                _ => (404, vec![]),
            }
        })
        .await;
        let m = Sillon::nouveau(&format!("{}/", s.url), "jeton");
        assert_eq!(
            m.apercu("alice", "album", ID).await,
            Ok(MusiqueResolue {
                genre: "album".into(),
                id: ID.into(),
                titre: "Lemonade".into(),
                artiste: "Beyoncé".into(),
                album: Some(ID.into()),
                image: Some(("album".into(), ID.into())),
            })
        );
        let artiste = m.apercu("alice", "artist", ID).await.unwrap();
        assert_eq!((artiste.album, artiste.image), (None, None));
        assert_eq!(
            m.apercu("alice", "track", ID).await,
            Err(RefusMusique::Indisponible)
        );
        assert_eq!(
            m.apercu("bob", "album", ID).await,
            Err(RefusMusique::Permission)
        );
        assert_eq!(
            m.apercu("alice", "album", AUTRE).await,
            Err(RefusMusique::Introuvable)
        );
        let appels = s.requetes().len();
        assert_eq!(
            m.apercu("alice", "playlist", ID).await,
            Err(RefusMusique::Introuvable)
        );
        assert_eq!(
            m.apercu("alice", "album", "../admin").await,
            Err(RefusMusique::Introuvable)
        );
        assert_eq!(s.requetes().len(), appels);
        assert_eq!(
            Sillon::nouveau(&s.url, "autre")
                .apercu("alice", "album", ID)
                .await,
            Err(RefusMusique::Indisponible)
        );
        let url = s.url.clone();
        drop(s);
        assert_eq!(
            Sillon::nouveau(&url, "jeton")
                .apercu("alice", "album", ID)
                .await,
            Err(RefusMusique::Indisponible)
        );
    }
}
