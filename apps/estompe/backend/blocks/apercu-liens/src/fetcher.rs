use std::{
    collections::HashMap,
    error::Error,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use html5gum::{DefaultEmitter, Token, Tokenizer};
use reqwest::{
    Client, Response, Url,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{ACCEPT, CONTENT_TYPE, USER_AGENT},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};

const AGENT: &str = "Mozilla/5.0 (compatible; EstompeLinkPreview/1.1; +https://estompe.plbls.fr)";
const CORPS_MAX: usize = 512 << 10;
const IMAGE_MAX: usize = 400 << 10;
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const CACHE_MAX: usize = 500;
const URL_MAX: usize = 2048;
const OEMBED_MAX: usize = 64 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Erreur {
    UrlInvalide,
    Bloquee,
    SansApercu,
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Erreur::UrlInvalide => "URL invalide",
            Erreur::Bloquee => "adresse non autorisée",
            Erreur::SansApercu => "pas d'aperçu disponible",
        })
    }
}

impl Error for Erreur {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub site_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub video: String,
}

const INTERDITS_V4: [(Ipv4Addr, u32); 14] = [
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

const INTERDITS_V6: [(Ipv6Addr, u32); 8] = [
    (Ipv6Addr::UNSPECIFIED, 96),
    (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96),
    (Ipv6Addr::new(0x64, 0xff9b, 1, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
    (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
    (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
];

fn dans(adresse: u128, base: u128, longueur: u32, bits: u32) -> bool {
    (adresse ^ base) >> (bits - longueur) == 0
}

fn est_publique(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => !INTERDITS_V4
            .iter()
            .any(|&(b, n)| dans(u32::from(a).into(), u32::from(b).into(), n, 32)),
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => est_publique(v4.into()),
            None => !INTERDITS_V6
                .iter()
                .any(|&(b, n)| dans(a.into(), b.into(), n, 128)),
        },
    }
}

struct ResolveurPublic;

impl Resolve for ResolveurPublic {
    fn resolve(&self, nom: Name) -> Resolving {
        Box::pin(async move {
            let adresses: Vec<SocketAddr> = tokio::net::lookup_host((nom.as_str(), 0))
                .await?
                .filter(|a| est_publique(a.ip()))
                .collect();
            if adresses.is_empty() {
                return Err(Box::new(Erreur::Bloquee) as Box<dyn Error + Send + Sync>);
            }
            Ok(Box::new(adresses.into_iter()) as Addrs)
        })
    }
}

fn verifier_url(u: &Url, non_filtre: bool) -> Result<(), Erreur> {
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none_or(str::is_empty)
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err(Erreur::UrlInvalide);
    }
    if non_filtre {
        return Ok(());
    }
    if u.port().is_some_and(|p| p != 80 && p != 443) {
        return Err(Erreur::Bloquee);
    }
    let ip = u
        .host_str()
        .and_then(|h| h.trim_matches(['[', ']']).parse::<IpAddr>().ok());
    match ip {
        Some(ip) if !est_publique(ip) => Err(Erreur::Bloquee),
        _ => Ok(()),
    }
}

fn sans_hote(brut: &str) -> bool {
    let minuscules = brut.to_ascii_lowercase();
    minuscules
        .strip_prefix("https:")
        .or_else(|| minuscules.strip_prefix("http:"))
        .is_some_and(|reste| !reste.starts_with("//") || reste[2..].starts_with(['/', '\\']))
}

fn a_userinfo(brut: &str) -> bool {
    brut.split_once("://").is_some_and(|(_, reste)| {
        reste
            .split(['/', '\\', '?', '#'])
            .next()
            .is_some_and(|autorite| autorite.contains('@'))
    })
}

fn nettoyer(mut u: Url) -> Url {
    let paires: Vec<(String, String)> = u.query_pairs().into_owned().collect();
    let mut gardees: Vec<_> = paires
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k.as_str(),
                "si" | "fbclid" | "gclid" | "igshid" | "mc_cid" | "mc_eid" | "feature" | "pp"
            ) && !k.starts_with("utm_")
        })
        .cloned()
        .collect();
    if gardees.len() == paires.len() {
        return u;
    }
    gardees.sort_by(|a, b| a.0.cmp(&b.0));
    if gardees.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(&gardees);
    }
    u
}

fn clip(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s
    }
}

fn est_page_erreur(p: &Preview) -> bool {
    const MARQUEURS: [&str; 12] = [
        "navigateur est obsolète",
        "browser is no longer supported",
        "update your browser",
        "avant d'accéder à",
        "before you continue to",
        "just a moment",
        "attention required",
        "access denied",
        "are you a robot",
        "captcha",
        "enable javascript",
        "please verify you are a human",
    ];
    let t = p.title.to_lowercase();
    MARQUEURS.iter().any(|m| t.contains(m))
}

fn type_image(corps: &[u8]) -> Option<&'static str> {
    if corps.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if corps.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if corps.starts_with(b"GIF87a") || corps.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if corps.len() >= 14 && &corps[..4] == b"RIFF" && &corps[8..14] == b"WEBPVP" {
        Some("image/webp")
    } else {
        None
    }
}

fn texte(octets: &[u8]) -> String {
    String::from_utf8_lossy(octets).into_owned()
}

fn parse(corps: &[u8], base: &Url) -> Option<Preview> {
    let mut meta: HashMap<String, String> = HashMap::new();
    let mut titre = String::new();
    let mut apres_titre = false;
    let mut emetteur = DefaultEmitter::default();
    emetteur.naively_switch_states(true);
    for Ok(jeton) in Tokenizer::new_with_emitter(corps, emetteur) {
        if matches!(jeton, Token::Error(_)) {
            continue;
        }
        let attendu = std::mem::take(&mut apres_titre);
        match jeton {
            Token::StartTag(t) => match t.name.as_slice() {
                b"meta" => {
                    let attr = |nom: &[u8]| {
                        t.attributes
                            .iter()
                            .find(|(k, _)| k.as_slice() == nom)
                            .map(|(_, v)| texte(&v.value))
                    };
                    let cle = attr(b"property")
                        .or_else(|| attr(b"name"))
                        .unwrap_or_default()
                        .to_lowercase();
                    let contenu = attr(b"content").unwrap_or_default();
                    if !cle.is_empty() && !contenu.is_empty() {
                        meta.entry(cle).or_insert(contenu);
                    }
                }
                b"title" => apres_titre = true,
                b"body" => break,
                _ => {}
            },
            Token::String(s) if attendu && titre.is_empty() => titre = texte(&s.value),
            Token::EndTag(t) if t.name.as_slice() == b"head" => break,
            _ => {}
        }
    }
    let premier = |cles: &[&str]| {
        cles.iter()
            .find_map(|k| meta.get(*k).filter(|v| !v.is_empty()))
            .cloned()
            .unwrap_or_default()
    };
    let t = premier(&["og:title", "twitter:title"]);
    let t = if t.is_empty() { titre } else { t };
    let mut p = Preview {
        title: clip(&t, 200),
        description: clip(
            &premier(&["og:description", "twitter:description", "description"]),
            400,
        ),
        site_name: clip(&premier(&["og:site_name"]), 100),
        ..Preview::default()
    };
    let img = premier(&["og:image", "twitter:image"]);
    if !img.is_empty()
        && img.len() <= URL_MAX
        && let Ok(u) = base.join(&img)
        && matches!(u.scheme(), "http" | "https")
    {
        p.image = u.to_string();
    }
    let video = premier(&[
        "og:video:secure_url",
        "og:video:url",
        "og:video",
        "twitter:player:stream",
    ]);
    if !video.is_empty()
        && video.len() <= URL_MAX
        && let Ok(u) = base.join(&video)
        && u.scheme() == "https"
    {
        p.video = u.to_string();
    }
    if p.title.is_empty() && p.description.is_empty() {
        return None;
    }
    Some(p)
}

fn bloquee(e: &reqwest::Error) -> bool {
    let mut cause: Option<&(dyn Error + 'static)> = Some(e);
    while let Some(c) = cause {
        if c.downcast_ref::<Erreur>() == Some(&Erreur::Bloquee) {
            return true;
        }
        cause = c.source();
    }
    false
}

async fn lire_limite(mut reponse: Response, max: usize) -> Vec<u8> {
    let mut corps = Vec::new();
    while corps.len() < max {
        match reponse.chunk().await {
            Ok(Some(morceau)) => corps.extend_from_slice(&morceau),
            _ => break,
        }
    }
    corps.truncate(max);
    corps
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct OEmbed {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub thumbnail_url: String,
}

pub struct Fetcher {
    client: Client,
    non_filtre: bool,
    cache: Mutex<HashMap<String, (Option<Preview>, Instant)>>,
}

impl Fetcher {
    pub fn new(non_filtre: bool) -> Self {
        let mut client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(6))
            .connect_timeout(Duration::from_secs(4))
            .pool_max_idle_per_host(0)
            .redirect(Policy::custom(move |a| {
                if a.previous().len() >= 3 {
                    a.error("trop de redirections")
                } else if let Err(e) = verifier_url(a.url(), non_filtre) {
                    a.error(e)
                } else {
                    a.follow()
                }
            }));
        if !non_filtre {
            client = client.dns_resolver(Arc::new(ResolveurPublic));
        }
        Fetcher {
            client: client.build().expect("client HTTP de l'aperçu de liens"),
            non_filtre,
            cache: Mutex::default(),
        }
    }

    fn lire_url(&self, brut: &str) -> Result<Url, Erreur> {
        if brut.len() > URL_MAX {
            return Err(Erreur::UrlInvalide);
        }
        if a_userinfo(brut) || sans_hote(brut.trim()) {
            return Err(Erreur::UrlInvalide);
        }
        let mut u = Url::parse(brut.trim()).map_err(|_| Erreur::UrlInvalide)?;
        u.set_fragment(None);
        verifier_url(&u, self.non_filtre)?;
        Ok(nettoyer(u))
    }

    fn cache(&self) -> MutexGuard<'_, HashMap<String, (Option<Preview>, Instant)>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn forget(&self, brut: &str) {
        if let Ok(u) = self.lire_url(brut) {
            self.cache().remove(u.as_str());
        }
    }

    pub async fn fetch(&self, brut: &str) -> Result<Preview, Erreur> {
        let u = self.lire_url(brut)?;
        let cle = u.to_string();
        if let Some((p, exp)) = self.cache().get(&cle)
            && Instant::now() < *exp
        {
            return p.clone().ok_or(Erreur::SansApercu);
        }
        let p = match self.recuperer(&u).await {
            Ok(p) => Some(p),
            Err(Erreur::SansApercu) => None,
            Err(e) => return Err(e),
        };
        let mut cache = self.cache();
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(cle, (p.clone(), Instant::now() + CACHE_TTL));
        p.ok_or(Erreur::SansApercu)
    }

    async fn recuperer(&self, u: &Url) -> Result<Preview, Erreur> {
        let reponse = self
            .client
            .get(u.clone())
            .header(USER_AGENT, AGENT)
            .header(ACCEPT, "text/html")
            .send()
            .await
            .map_err(|e| {
                if bloquee(&e) {
                    Erreur::Bloquee
                } else {
                    Erreur::SansApercu
                }
            })?;
        let html = reponse
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_lowercase().starts_with("text/html"));
        if reponse.status() != reqwest::StatusCode::OK || !html {
            return Err(Erreur::SansApercu);
        }
        let base = reponse.url().clone();
        let corps = lire_limite(reponse, CORPS_MAX).await;
        let mut p = parse(&corps, &base)
            .filter(|p| !est_page_erreur(p))
            .ok_or(Erreur::SansApercu)?;
        p.url = u.to_string();
        if !p.image.is_empty() {
            p.image = self.image(&p.image).await;
        }
        Ok(p)
    }

    pub async fn image(&self, brut: &str) -> String {
        let Ok(u) = Url::parse(brut) else {
            return String::new();
        };
        if verifier_url(&u, self.non_filtre).is_err() {
            return String::new();
        }
        let Ok(reponse) = self
            .client
            .get(u)
            .header(USER_AGENT, AGENT)
            .header(ACCEPT, "image/png,image/jpeg,image/gif,image/webp")
            .send()
            .await
        else {
            return String::new();
        };
        if reponse.status() != reqwest::StatusCode::OK {
            return String::new();
        }
        let corps = lire_limite(reponse, IMAGE_MAX + 1).await;
        if corps.is_empty() || corps.len() > IMAGE_MAX {
            return String::new();
        }
        match type_image(&corps) {
            Some(t) => format!("data:{t};base64,{}", STANDARD.encode(&corps)),
            None => String::new(),
        }
    }
    pub async fn oembed(&self, endpoint: &str, cible: &str) -> Result<OEmbed, Erreur> {
        let mut u = Url::parse(endpoint).map_err(|_| Erreur::UrlInvalide)?;
        verifier_url(&u, self.non_filtre)?;
        u.query_pairs_mut()
            .append_pair("url", cible)
            .append_pair("format", "json");
        let reponse = self
            .client
            .get(u)
            .header(USER_AGENT, AGENT)
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| {
                if bloquee(&e) {
                    Erreur::Bloquee
                } else {
                    Erreur::SansApercu
                }
            })?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Erreur::SansApercu);
        }
        let corps = lire_limite(reponse, OEMBED_MAX).await;
        let mut o: OEmbed = serde_json::from_slice(&corps).map_err(|_| Erreur::SansApercu)?;
        o.title = clip(&o.title, 200);
        o.author_name = clip(&o.author_name, 100);
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CACHE_MAX, Erreur, Fetcher, Preview, a_userinfo, est_publique, parse, sans_hote,
        verifier_url,
    };
    use axum::{
        Router,
        extract::Request,
        http::{StatusCode, header},
        response::{IntoResponse, Redirect},
    };
    use reqwest::Url;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };
    use tokio::net::TcpListener;

    const PAGE: &str = r#"<!doctype html><html><head><title>Titre brut</title>
<meta property="og:title" content="Titre &lt;b&gt;OG&lt;/b&gt;">
<meta property="og:description" content="  Une   description ">
<meta property="og:image" content="/img/a.png">
<meta property="og:site_name" content="Exemple">
</head><body><meta property="og:title" content="ignoré"></body></html>"#;

    fn html(corps: &str) -> axum::response::Response {
        ([(header::CONTENT_TYPE, "text/html")], corps.to_string()).into_response()
    }

    async fn site() -> (String, Arc<AtomicUsize>) {
        let appels = Arc::new(AtomicUsize::new(0));
        let compte = appels.clone();
        let app = Router::new().fallback(move |req: Request| {
            let compte = compte.clone();
            async move {
                compte.fetch_add(1, Ordering::SeqCst);
                match req.uri().path() {
                    "/ok" => html(PAGE),
                    "/titre" => html("<html><head><title>Seulement un titre</title></head>"),
                    "/vide" => html("<html><head></head><body>rien</body></html>"),
                    "/img/a.png" => (
                        [(header::CONTENT_TYPE, "image/png")],
                        b"\x89PNG\r\n\x1a\n0000".to_vec(),
                    )
                        .into_response(),
                    "/img/b.svg" => (
                        [(header::CONTENT_TYPE, "image/svg+xml")],
                        "<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>",
                    )
                        .into_response(),
                    "/image" => (
                        [(header::CONTENT_TYPE, "image/png")],
                        b"\x89PNG".to_vec(),
                    )
                        .into_response(),
                    "/obsolete" => html(
                        "<html><head><title>Votre navigateur est obsolète, veuillez le mettre à jour.</title></head>",
                    ),
                    "/consentement" => html(
                        "<html><head><title>Avant d'accéder à YouTube</title></head>",
                    ),
                    "/ua" => html(&format!(
                        "<html><head><title>{}</title></head>",
                        req.headers()[header::USER_AGENT].to_str().unwrap()
                    )),
                    "/redir" => Redirect::temporary("/ok").into_response(),
                    "/boucle" => Redirect::temporary("/boucle").into_response(),
                    _ => StatusCode::NOT_FOUND.into_response(),
                }
            }
        });
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", ecoute.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(ecoute, app).await.ok() });
        (url, appels)
    }

    #[tokio::test]
    async fn fetch_extrait_l_apercu() {
        let (site, _) = site().await;
        let f = Fetcher::new(true);
        let p = f.fetch(&format!("{site}/ok")).await.unwrap();
        assert_eq!(p.title, "Titre <b>OG</b>");
        assert_eq!(p.description, "Une description");
        assert_eq!(p.site_name, "Exemple");
        assert!(p.image.starts_with("data:image/png;base64,"), "{p:?}");
        assert_eq!(f.image(&format!("{site}/img/b.svg")).await, "");
        assert_eq!(
            f.fetch(&format!("{site}/titre")).await.unwrap().title,
            "Seulement un titre"
        );
        assert!(
            !f.fetch(&format!("{site}/redir"))
                .await
                .unwrap()
                .title
                .is_empty()
        );
        assert_eq!(
            f.fetch(&format!("{site}/ua")).await.unwrap().title,
            super::AGENT
        );
        for chemin in [
            "/vide",
            "/image",
            "/absent",
            "/boucle",
            "/obsolete",
            "/consentement",
        ] {
            assert_eq!(
                f.fetch(&format!("{site}{chemin}")).await,
                Err(Erreur::SansApercu),
                "{chemin}"
            );
        }
    }

    #[tokio::test]
    async fn le_cache_evite_un_second_appel_et_forget_l_oublie() {
        let (site, appels) = site().await;
        let f = Fetcher::new(true);
        let ok = format!("{site}/ok?v=1&si=abc&utm_source=x&fbclid=y");
        let p = f.fetch(&ok).await.unwrap();
        assert_eq!(p.url, format!("{site}/ok?v=1"));
        let _ = f.fetch(&format!("{site}/vide")).await;
        let n = appels.load(Ordering::SeqCst);
        f.fetch(&format!("{site}/ok?v=1&si=autre")).await.unwrap();
        assert_eq!(
            f.fetch(&format!("{site}/vide")).await,
            Err(Erreur::SansApercu)
        );
        assert_eq!(appels.load(Ordering::SeqCst), n);
        f.forget(&format!("{site}/ok?v=1"));
        f.fetch(&format!("{site}/ok?v=1")).await.unwrap();
        assert!(appels.load(Ordering::SeqCst) > n);
    }

    #[tokio::test]
    async fn les_refus_ne_sont_pas_mis_en_cache_mais_l_absence_d_apercu_si() {
        let (site, _) = site().await;
        let f = Fetcher::new(false);
        assert_eq!(f.fetch("http://localhost/").await, Err(Erreur::Bloquee));
        assert!(f.cache().is_empty());
        let g = Fetcher::new(true);
        assert_eq!(
            g.fetch(&format!("{site}/vide")).await,
            Err(Erreur::SansApercu)
        );
        assert_eq!(g.cache().len(), 1);
    }

    #[test]
    fn parse_lit_la_video() {
        let base = Url::parse("https://exemple.fr/page").unwrap();
        for (html, attendu) in [
            (
                r#"<head><title>t</title><meta property="og:video:secure_url" content="https://cdn.exemple.fr/a.mp4"></head>"#,
                "https://cdn.exemple.fr/a.mp4",
            ),
            (
                r#"<head><title>t</title><meta property="og:video" content="/media/a.webm"></head>"#,
                "https://exemple.fr/media/a.webm",
            ),
            (
                r#"<head><title>t</title><meta property="og:video" content="http://exemple.fr/a.mp4"></head>"#,
                "",
            ),
            ("<head><title>t</title></head>", ""),
        ] {
            let p = parse(html.as_bytes(), &base).unwrap_or_else(|| panic!("{html}"));
            assert_eq!(p.video, attendu, "{html}");
        }
    }

    #[tokio::test]
    async fn adresses_et_schemas_refuses() {
        let (site, appels) = site().await;
        let f = Fetcher::new(false);
        let avec_identifiants = site.replace("http://", "http://localhost:80@");
        for u in [
            site.as_str(),
            avec_identifiants.as_str(),
            "http://127.0.0.1/",
            "http://127.0.0.1:80/",
            "http://[::1]/",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.64.0.1/",
            "http://0.0.0.0/",
            "http://[::ffff:127.0.0.1]/",
            "http://0177.0.0.1/",
            "http://2130706433/",
            "http://localhost/",
        ] {
            let e = f.fetch(u).await;
            assert!(
                matches!(e, Err(Erreur::Bloquee | Erreur::UrlInvalide)),
                "{u} : {e:?}"
            );
        }
        assert_eq!(f.fetch("http://localhost/").await, Err(Erreur::Bloquee));
        for u in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "gopher://x/",
            "javascript:alert(1)",
            "",
            "http://",
            "pas une url",
        ] {
            assert_eq!(f.fetch(u).await, Err(Erreur::UrlInvalide), "{u:?}");
        }
        assert_eq!(appels.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn est_publique_suit_la_liste_de_go() {
        for (ip, attendu) in [
            ("8.8.8.8", true),
            ("1.1.1.1", true),
            ("2606:4700:4700::1111", true),
            ("127.0.0.1", false),
            ("10.1.2.3", false),
            ("172.16.0.1", false),
            ("192.168.0.1", false),
            ("169.254.1.1", false),
            ("100.64.0.1", false),
            ("0.0.0.0", false),
            ("224.0.0.1", false),
            ("::1", false),
            ("fe80::1", false),
            ("fc00::1", false),
            ("::ffff:10.0.0.1", false),
            ("198.18.0.1", false),
            ("255.255.255.255", false),
            ("2002:7f00:1::", false),
            ("::7f00:1", false),
            ("64:ff9b:1::1", false),
        ] {
            assert_eq!(est_publique(ip.parse().unwrap()), attendu, "{ip}");
        }
    }

    #[tokio::test]
    async fn le_contenu_de_title_et_de_script_est_du_texte() {
        let base = Url::parse("https://exemple.fr/").unwrap();
        let p = parse(
            b"<head><script>var s = '</head><body>';</script><title>A <b> B</title><meta name=\"description\" content=\"d\"></head>",
            &base,
        )
        .unwrap();
        assert_eq!((p.title.as_str(), p.description.as_str()), ("A <b> B", "d"));
    }

    #[test]
    fn sans_hote_refuse_les_liens_http_sans_hote() {
        for u in ["http:///x", "https:///", "http:x", "HTTP:/x", "http:\\\\x"] {
            assert!(sans_hote(u), "{u:?}");
        }
        for u in ["http://x/", "https://a.fr", "ftp:///x", "x"] {
            assert!(!sans_hote(u), "{u:?}");
        }
    }

    #[test]
    fn verifier_url_controle_les_cibles_de_redirection() {
        for (u, attendu) in [
            ("http://10.0.0.1/", Err(Erreur::Bloquee)),
            ("http://exemple.fr:8080/", Err(Erreur::Bloquee)),
            ("http://u:p@exemple.fr/", Err(Erreur::UrlInvalide)),
            ("ftp://exemple.fr/", Err(Erreur::UrlInvalide)),
            ("https://exemple.fr:443/", Ok(())),
        ] {
            assert_eq!(verifier_url(&Url::parse(u).unwrap(), false), attendu, "{u}");
        }
    }

    #[test]
    fn userinfo_vide_detecte() {
        assert!(a_userinfo("http://@exemple.fr/"));
        assert!(a_userinfo("http://u@exemple.fr/"));
        assert!(!a_userinfo("http://exemple.fr/a@b"));
        assert!(!a_userinfo("http://exemple.fr/?mail=a@b"));
    }

    #[tokio::test]
    async fn image_filtree_refuse_le_local() {
        let (site, _) = site().await;
        assert_eq!(
            Fetcher::new(false)
                .image(&format!("{site}/img/a.png"))
                .await,
            ""
        );
        assert!(
            Fetcher::new(true)
                .image(&format!("{site}/img/a.png"))
                .await
                .starts_with("data:image/png;base64,")
        );
    }

    #[tokio::test]
    async fn cache_expire_evince_et_ignore_le_fragment() {
        let (site, appels) = site().await;
        let f = Fetcher::new(true);
        let ok = format!("{site}/ok");
        let perime = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("horloge");
        f.cache().insert(
            ok.clone(),
            (
                Some(Preview {
                    title: "périmé".into(),
                    ..Preview::default()
                }),
                perime,
            ),
        );
        assert_eq!(f.fetch(&ok).await.unwrap().title, "Titre <b>OG</b>");
        assert_eq!(appels.load(Ordering::SeqCst), 2);
        f.fetch(&format!("{ok}#ancre")).await.unwrap();
        assert_eq!(appels.load(Ordering::SeqCst), 2);
        let futur = Instant::now() + Duration::from_secs(600);
        for i in 0..CACHE_MAX {
            f.cache().insert(format!("cle{i}"), (None, futur));
        }
        f.fetch(&format!("{site}/titre")).await.unwrap();
        assert_eq!(f.cache().len(), 1);
    }

    #[test]
    fn preview_omet_les_champs_vides() {
        let p = Preview {
            url: "u".into(),
            title: "t".into(),
            ..Preview::default()
        };
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"url":"u","title":"t"}"#
        );
    }
}
