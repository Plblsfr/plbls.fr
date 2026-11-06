//! Serveur loopback de la redirection OAuth (RFC 8252). N'utilise que la bibliothèque standard.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Attend « GET /callback?... » et renvoie la query string ; ignore les autres requêtes (favicon, sondes).
pub fn accept(listener: TcpListener) -> Result<String, String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Some(query) = read(&mut stream) {
                    respond(&mut stream, "200 OK", "Connexion réussie. Tu peux fermer cet onglet et revenir à Estompe.");
                    return Ok(query);
                }
                // Autre requête (favicon, sonde…) : on l'ignore et on continue d'attendre.
                respond(&mut stream, "404 Not Found", "Introuvable.");
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("délai de connexion dépassé".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Renvoie la query de « GET /callback?... » ; None pour toute autre requête.
fn read(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    let line = head.lines().next()?;
    let target = line.strip_prefix("GET ")?.split(' ').next()?;
    let query = target.strip_prefix("/callback?")?;
    Some(query.to_string())
}

fn respond(stream: &mut TcpStream, status: &str, text: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Estompe</title><body style=\"font-family:system-ui;margin:3rem\">{text}<script>window.close()</script></body>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(request: &str) -> Option<String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let req = request.to_string();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(req.as_bytes()).unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let out = read(&mut stream);
        client.join().unwrap();
        out
    }

    #[test]
    fn extrait_la_query_du_callback() {
        assert_eq!(
            roundtrip("GET /callback?code=abc&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_deref(),
            Some("code=abc&state=xyz")
        );
    }

    #[test]
    fn ignore_les_autres_requetes() {
        assert_eq!(roundtrip("GET /favicon.ico HTTP/1.1\r\n\r\n"), None);
        assert_eq!(roundtrip("POST /callback?code=x HTTP/1.1\r\n\r\n"), None);
        assert_eq!(roundtrip("GET /callback HTTP/1.1\r\n\r\n"), None);
        assert_eq!(roundtrip("n'importe quoi"), None);
    }
}

#[cfg(test)]
mod accept_tests {
    use super::*;

    #[test]
    fn accept_repond_et_renvoie_la_query_en_ignorant_le_bruit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let get = |path: &str| {
                let mut s = TcpStream::connect(addr).unwrap();
                write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).unwrap();
                out
            };
            let noise = get("/favicon.ico");
            let real = get("/callback?code=abc&state=xyz");
            (noise, real)
        });
        assert_eq!(accept(listener).unwrap(), "code=abc&state=xyz");
        let (noise, real) = client.join().unwrap();
        assert!(noise.starts_with("HTTP/1.1 404"));
        assert!(real.starts_with("HTTP/1.1 200") && real.contains("Connexion réussie"));
    }
}
