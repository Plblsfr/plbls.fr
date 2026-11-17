use std::{
    error::Error,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
};

pub fn client(delai: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(delai)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client HTTP")
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

pub fn est_publique(ip: IpAddr) -> bool {
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

#[derive(Debug)]
pub struct AdresseRefusee;

impl fmt::Display for AdresseRefusee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("adresse non publique refusée")
    }
}

impl Error for AdresseRefusee {}

struct ResolveurPublic;

impl Resolve for ResolveurPublic {
    fn resolve(&self, nom: Name) -> Resolving {
        Box::pin(async move {
            let adresses: Vec<SocketAddr> = tokio::net::lookup_host((nom.as_str(), 0))
                .await?
                .filter(|a| est_publique(a.ip()))
                .collect();
            if adresses.is_empty() {
                return Err(Box::new(AdresseRefusee) as Box<dyn Error + Send + Sync>);
            }
            Ok(Box::new(adresses.into_iter()) as Addrs)
        })
    }
}

pub fn url_publique(u: &Url, prives: bool) -> bool {
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none_or(str::is_empty)
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return false;
    }
    if prives {
        return true;
    }
    if u.port().is_some_and(|p| p != 80 && p != 443) {
        return false;
    }
    u.host_str()
        .and_then(|h| h.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_none_or(est_publique)
}

pub fn client_public(delai: Duration, prives: bool) -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .no_proxy()
        .timeout(delai)
        .connect_timeout(delai.min(Duration::from_secs(5)))
        .redirect(reqwest::redirect::Policy::custom(move |a| {
            if a.previous().len() >= 3 {
                a.error("trop de redirections")
            } else if !url_publique(a.url(), prives) {
                a.error(AdresseRefusee)
            } else {
                a.follow()
            }
        }));
    if !prives {
        b = b.dns_resolver(Arc::new(ResolveurPublic));
    }
    b.build().expect("client HTTP public")
}

pub fn cause(e: &reqwest::Error) -> String {
    let mut source: Option<&dyn std::error::Error> = Some(e);
    let mut dernier = e.to_string();
    while let Some(s) = source {
        dernier = s.to_string();
        source = s.source();
    }
    dernier
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub async fn serveur(reponse: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", ecoute.local_addr().unwrap());
        let tache = tokio::spawn(async move {
            let (mut c, _) = ecoute.accept().await.unwrap();
            let mut lu = Vec::new();
            let mut tampon = [0u8; 4096];
            loop {
                let n = c.read(&mut tampon).await.unwrap();
                lu.extend_from_slice(&tampon[..n]);
                let texte = String::from_utf8_lossy(&lu).to_string();
                if let Some((entetes, corps)) = texte.split_once("\r\n\r\n") {
                    let longueur = entetes
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if corps.len() >= longueur || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            c.write_all(reponse.as_bytes()).await.unwrap();
            c.shutdown().await.ok();
            String::from_utf8_lossy(&lu).to_string()
        });
        (url, tache)
    }

    #[tokio::test]
    async fn redirection_refusee() {
        let (url, tache) = serveur(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/ailleurs\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let r = client(Duration::from_secs(5))
            .get(&url)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 302);
        tache.await.unwrap();
    }

    #[test]
    fn adresses_publiques_et_urls_acceptees() {
        for (ip, attendu) in [
            ("1.1.1.1", true),
            ("93.184.216.34", true),
            ("127.0.0.1", false),
            ("10.1.2.3", false),
            ("172.20.0.1", false),
            ("192.168.1.1", false),
            ("169.254.169.254", false),
            ("100.64.0.1", false),
            ("0.0.0.0", false),
            ("::1", false),
            ("::ffff:127.0.0.1", false),
            ("fd00::1", false),
            ("fe80::1", false),
            ("2606:4700::1111", true),
        ] {
            assert_eq!(est_publique(ip.parse().unwrap()), attendu, "{ip}");
        }
        let u = |t: &str| Url::parse(t).unwrap();
        assert!(url_publique(&u("https://coverartarchive.org/a.jpg"), false));
        for refusee in [
            "ftp://exemple.fr/a.jpg",
            "file:///etc/passwd",
            "http://127.0.0.1/a.jpg",
            "http://[::1]/a.jpg",
            "http://10.0.0.2/a.jpg",
            "http://169.254.169.254/latest",
            "http://exemple.fr:8686/a.jpg",
            "http://moi:secret@exemple.fr/a.jpg",
        ] {
            assert!(!url_publique(&u(refusee), false), "{refusee}");
        }
        assert!(url_publique(&u("http://127.0.0.1:9000/a.jpg"), true));
        assert!(!url_publique(&u("file:///etc/passwd"), true));
    }

    #[tokio::test]
    async fn client_public_refuse_un_nom_resolu_en_adresse_locale() {
        let (url, _tache) =
            serveur("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").await;
        let local = url.replace("127.0.0.1", "localhost");
        let e = client_public(Duration::from_secs(5), false)
            .get(&local)
            .send()
            .await
            .unwrap_err();
        assert!(cause(&e).contains("non publique"), "{}", cause(&e));
        let r = client_public(Duration::from_secs(5), true)
            .get(&local)
            .send()
            .await
            .unwrap();
        assert_eq!(r.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn cause_racine() {
        let e = client(Duration::from_secs(5))
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .unwrap_err();
        assert!(
            !cause(&e).contains("error sending request"),
            "{}",
            cause(&e)
        );
    }
}
