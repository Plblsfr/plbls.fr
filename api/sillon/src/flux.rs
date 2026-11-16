use std::{
    io::SeekFrom,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    sync::watch,
};
use uuid::Uuid;

use crate::transcodage::{Cible, Etape, GardeLecture};

pub const VALIDITE: Duration = Duration::from_secs(6 * 3600);
const MORCEAU: usize = 1 << 16;
const PAS: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variante {
    Direct,
    Transcode(Cible),
}

impl Variante {
    fn texte(self) -> String {
        match self {
            Variante::Direct => "direct".into(),
            Variante::Transcode(c) => c.texte(),
        }
    }

    fn depuis(texte: &str) -> Option<Self> {
        match texte {
            "direct" => Some(Variante::Direct),
            t => Cible::depuis(t).map(Variante::Transcode),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Billet {
    pub sub: String,
    pub piste: Uuid,
    pub fichier: Uuid,
    pub variante: Variante,
    pub exp: u64,
}

pub fn maintenant_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn corps(b: &Billet) -> String {
    format!(
        "{}.{}.{}.{}.{}",
        b.piste.simple(),
        b.fichier.simple(),
        b.variante.texte(),
        b.exp,
        URL_SAFE_NO_PAD.encode(&b.sub)
    )
}

fn mac(secret: &str, corps: &str) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    m.update(corps.as_bytes());
    m
}

pub fn signer(secret: &str, b: &Billet) -> String {
    let c = corps(b);
    let code = mac(secret, &c).finalize().into_bytes();
    format!("{c}.{}", URL_SAFE_NO_PAD.encode(code))
}

pub fn valider(secret: &str, jeton: &str, unix: u64) -> Option<Billet> {
    let (c, code) = jeton.rsplit_once('.')?;
    mac(secret, c)
        .verify_slice(&URL_SAFE_NO_PAD.decode(code).ok()?)
        .ok()?;
    let m: Vec<&str> = c.split('.').collect();
    let [piste, fichier, variante, exp, sub] = m.as_slice() else {
        return None;
    };
    let b = Billet {
        sub: String::from_utf8(URL_SAFE_NO_PAD.decode(sub).ok()?).ok()?,
        piste: Uuid::parse_str(piste).ok()?,
        fichier: Uuid::parse_str(fichier).ok()?,
        variante: Variante::depuis(variante)?,
        exp: exp.parse().ok()?,
    };
    (b.exp > unix && !b.sub.is_empty()).then_some(b)
}

fn plage(entetes: &HeaderMap, taille: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(v) = entetes.get(header::RANGE) else {
        return Ok(None);
    };
    let texte = v.to_str().map_err(|_| ())?;
    let (debut, fin) = texte
        .strip_prefix("bytes=")
        .ok_or(())?
        .split_once('-')
        .ok_or(())?;
    if debut.contains(',') || fin.contains(',') || taille == 0 {
        return Err(());
    }
    let nombre = |s: &str| s.trim().parse::<u64>().map_err(|_| ());
    let (a, b) = match (debut.trim().is_empty(), fin.trim().is_empty()) {
        (true, true) => return Err(()),
        (true, false) => {
            let n = nombre(fin)?.min(taille);
            if n == 0 {
                return Err(());
            }
            (taille - n, taille - 1)
        }
        (false, true) => (nombre(debut)?, taille - 1),
        (false, false) => (nombre(debut)?, nombre(fin)?.min(taille - 1)),
    };
    if a > b || a >= taille {
        return Err(());
    }
    Ok(Some((a, b)))
}

fn entetes_de_flux(r: &mut Response, mime: &str) {
    let h = r.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

pub async fn servir_fichier(
    chemin: &Path,
    mime: &str,
    entetes: &HeaderMap,
    garde: Option<GardeLecture>,
) -> Result<Response, Response> {
    let mut f = tokio::fs::File::open(chemin).await.map_err(|e| {
        tracing::warn!("lecture d'un flux : {e}");
        crate::http::introuvable()
    })?;
    let taille = f
        .metadata()
        .await
        .map_err(|_| crate::http::introuvable())?
        .len();
    let mut r = Response::new(Body::empty());
    let (debut, longueur) = match plage(entetes, taille) {
        Err(()) => {
            let mut r = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            if let Ok(v) = HeaderValue::from_str(&format!("bytes */{taille}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            return Ok(r);
        }
        Ok(None) => (0, taille),
        Ok(Some((a, b))) => {
            *r.status_mut() = StatusCode::PARTIAL_CONTENT;
            if let Ok(v) = HeaderValue::from_str(&format!("bytes {a}-{b}/{taille}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            (a, b - a + 1)
        }
    };
    f.seek(SeekFrom::Start(debut))
        .await
        .map_err(|_| crate::http::interne())?;
    entetes_de_flux(&mut r, mime);
    let h = r.headers_mut();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(longueur));
    let lecteur = f.take(longueur);
    let flux = futures_util::stream::unfold((lecteur, garde), |(mut lecteur, garde)| async move {
        let mut tampon = vec![0u8; MORCEAU];
        match lecteur.read(&mut tampon).await {
            Ok(0) => None,
            Ok(n) => {
                tampon.truncate(n);
                Some((Ok(Bytes::from(tampon)), (lecteur, garde)))
            }
            Err(e) => Some((Err(e), (lecteur, garde))),
        }
    });
    *r.body_mut() = Body::from_stream(flux);
    Ok(r)
}

struct Croissant {
    fichier: tokio::fs::File,
    rx: watch::Receiver<Etape>,
    _garde: GardeLecture,
    fin: bool,
}

impl Croissant {
    async fn suivant(&mut self) -> Option<std::io::Result<Bytes>> {
        if self.fin {
            return None;
        }
        loop {
            let etape = *self.rx.borrow();
            let mut tampon = vec![0u8; MORCEAU];
            let n = match self.fichier.read(&mut tampon).await {
                Ok(n) => n,
                Err(e) => {
                    self.fin = true;
                    return Some(Err(e));
                }
            };
            if n > 0 {
                tampon.truncate(n);
                return Some(Ok(Bytes::from(tampon)));
            }
            match etape {
                Etape::Fini => return None,
                Etape::Echec => {
                    self.fin = true;
                    return Some(Err(std::io::Error::other("transcodage échoué")));
                }
                Etape::EnCours => {
                    let _ = tokio::time::timeout(PAS, self.rx.changed()).await;
                }
            }
        }
    }
}

pub enum Debut {
    Pret(Response),
    Echec,
    Delai,
}

pub async fn servir_croissant(
    fichier: std::fs::File,
    mut rx: watch::Receiver<Etape>,
    garde: GardeLecture,
    mime: &str,
    attente: Duration,
) -> Debut {
    let fichier = tokio::fs::File::from_std(fichier);
    let limite = tokio::time::Instant::now() + attente;
    loop {
        let etape = *rx.borrow();
        let ecrit = fichier.metadata().await.map(|m| m.len()).unwrap_or(0);
        match etape {
            Etape::Echec => return Debut::Echec,
            Etape::Fini => break,
            Etape::EnCours if ecrit > 0 => break,
            Etape::EnCours => {}
        }
        if tokio::time::Instant::now() > limite {
            return Debut::Delai;
        }
        let _ = tokio::time::timeout(PAS, rx.changed()).await;
    }
    let etat = Croissant {
        fichier,
        rx,
        _garde: garde,
        fin: false,
    };
    let flux =
        futures_util::stream::unfold(
            etat,
            |mut e| async move { e.suivant().await.map(|m| (m, e)) },
        );
    let mut r = Response::new(Body::from_stream(flux));
    entetes_de_flux(&mut r, mime);
    r.headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("none"));
    Debut::Pret(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcodage::Format;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn billet() -> Billet {
        Billet {
            sub: "sub-alice".into(),
            piste: Uuid::from_u128(1),
            fichier: Uuid::from_u128(2),
            variante: Variante::Transcode(Cible {
                format: Format::Opus,
                kbps: 160,
            }),
            exp: 2_000,
        }
    }

    #[test]
    fn jeton_signe_expire_ou_modifie_refuse() {
        let b = billet();
        let j = signer(SECRET, &b);
        assert_eq!(valider(SECRET, &j, 1_000), Some(b.clone()));
        assert_eq!(valider(SECRET, &j, 2_000), None);
        assert_eq!(valider(&SECRET.replace('0', "1"), &j, 1_000), None);
        let autre = signer(
            SECRET,
            &Billet {
                piste: Uuid::from_u128(3),
                ..b.clone()
            },
        );
        let (corps_autre, _) = autre.rsplit_once('.').unwrap();
        let (_, code) = j.rsplit_once('.').unwrap();
        for faux in [
            format!("{corps_autre}.{code}"),
            j.replace(".2000.", ".9000."),
            j.replace("opus160", "direct"),
            format!("{j}x"),
            format!("{j}.x"),
            String::new(),
            "a.b".into(),
        ] {
            assert_eq!(valider(SECRET, &faux, 1_000), None, "{faux}");
        }
        let direct = Billet {
            variante: Variante::Direct,
            ..b
        };
        assert_eq!(
            valider(SECRET, &signer(SECRET, &direct), 1_000),
            Some(direct)
        );
    }

    #[test]
    fn plages() {
        let h = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::RANGE, v.parse().unwrap());
            h
        };
        assert_eq!(plage(&HeaderMap::new(), 100), Ok(None));
        assert_eq!(plage(&h("bytes=0-9"), 100), Ok(Some((0, 9))));
        assert_eq!(plage(&h("bytes=90-"), 100), Ok(Some((90, 99))));
        assert_eq!(plage(&h("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(plage(&h("bytes=50-500"), 100), Ok(Some((50, 99))));
        for faux in [
            "bytes=100-",
            "bytes=5-1",
            "bytes=-0",
            "octets=0-1",
            "bytes=0-1,3-4",
            "bytes=-",
        ] {
            assert_eq!(plage(&h(faux), 100), Err(()), "{faux}");
        }
    }
}
