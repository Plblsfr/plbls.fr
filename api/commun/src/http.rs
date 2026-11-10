use std::time::Duration;

pub fn client(delai: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(delai)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client HTTP")
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
