use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::http::{tete_de_partie, Client, ErreurApi, Partie, DELAI_TELECHARGEMENT};

pub const TAILLE_DES_MORCEAUX: usize = 64 * 1024;
pub const MORCEAUX_EN_VOL: usize = 4;

pub type Progression = Arc<dyn Fn(u64, u64) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceEnvoi {
    Chemin(PathBuf),
    Octets(Bytes),
}

impl SourceEnvoi {
    pub fn taille(&self) -> io::Result<u64> {
        match self {
            Self::Chemin(chemin) => std::fs::metadata(chemin).map(|m| m.len()),
            Self::Octets(octets) => Ok(octets.len() as u64),
        }
    }

    fn ouvrir(&self) -> io::Result<Box<dyn Read + Send>> {
        match self {
            Self::Chemin(chemin) => Ok(Box::new(File::open(chemin)?)),
            Self::Octets(octets) => Ok(Box::new(Cursor::new(octets.clone()))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Televersement {
    pub parties: Vec<Partie>,
    pub nom: String,
    pub type_mime: String,
    pub source: SourceEnvoi,
}

struct CorpsEnFlux {
    recepteur: mpsc::Receiver<io::Result<Bytes>>,
    reste: u64,
}

impl Body for CorpsEnFlux {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        if self.reste == 0 {
            return Poll::Ready(None);
        }
        match self.recepteur.poll_recv(cx) {
            Poll::Ready(Some(Ok(morceau))) => {
                self.reste = self.reste.saturating_sub(morceau.len() as u64);
                Poll::Ready(Some(Ok(Frame::data(morceau))))
            }
            Poll::Ready(Some(Err(erreur))) => Poll::Ready(Some(Err(erreur))),
            Poll::Ready(None) => Poll::Ready(Some(Err(fin_prematuree()))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.reste == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.reste)
    }
}

fn fin_prematuree() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "fichier plus court que prévu")
}

fn produire(
    tete: Bytes,
    source: SourceEnvoi,
    taille: u64,
    fin: Bytes,
    emetteur: mpsc::Sender<io::Result<Bytes>>,
    progres: Progression,
) {
    if emetteur.blocking_send(Ok(tete)).is_err() {
        return;
    }
    let mut lecteur = match source.ouvrir() {
        Ok(lecteur) => lecteur.take(taille),
        Err(erreur) => {
            let _ = emetteur.blocking_send(Err(erreur));
            return;
        }
    };
    let mut envoyes = 0u64;
    progres(0, taille);
    loop {
        let mut tampon = vec![0u8; TAILLE_DES_MORCEAUX];
        match lecteur.read(&mut tampon) {
            Ok(0) => break,
            Ok(lus) => {
                tampon.truncate(lus);
                if emetteur.blocking_send(Ok(Bytes::from(tampon))).is_err() {
                    return;
                }
                envoyes += lus as u64;
                progres(envoyes, taille);
            }
            Err(erreur) if erreur.kind() == io::ErrorKind::Interrupted => {}
            Err(erreur) => {
                let _ = emetteur.blocking_send(Err(erreur));
                return;
            }
        }
    }
    let dernier = if envoyes < taille {
        Err(fin_prematuree())
    } else {
        Ok(fin)
    };
    let _ = emetteur.blocking_send(dernier);
}

fn frontiere() -> String {
    let mut octets = [0u8; 16];
    let _ = getrandom::fill(&mut octets);
    let hexa: String = octets.iter().map(|o| format!("{o:02x}")).collect();
    format!("estompe-flux-{hexa}")
}

impl Client {
    pub async fn televerser_en_flux<T: DeserializeOwned>(
        &self,
        chemin: &str,
        envoi: &Televersement,
        progres: Progression,
    ) -> Result<T, ErreurApi> {
        let taille = envoi
            .source
            .taille()
            .map_err(|e| ErreurApi::RequeteInvalide(e.to_string()))?;
        let frontiere = frontiere();
        let mut tete = Vec::new();
        for partie in &envoi.parties {
            tete.extend_from_slice(tete_de_partie(&frontiere, partie).as_bytes());
            tete.extend_from_slice(&partie.contenu);
            tete.extend_from_slice(b"\r\n");
        }
        let fichier = Partie::fichier("file", &envoi.nom, &envoi.type_mime, Bytes::new());
        tete.extend_from_slice(tete_de_partie(&frontiere, &fichier).as_bytes());
        let tete = Bytes::from(tete);
        let fin = Bytes::from(format!("\r\n--{frontiere}--\r\n"));
        let longueur = tete.len() as u64 + taille + fin.len() as u64;
        let methode = Method::POST;
        let mut premiere = true;
        loop {
            let (emetteur, recepteur) = mpsc::channel(MORCEAUX_EN_VOL);
            let (t, s, f, p) = (
                tete.clone(),
                envoi.source.clone(),
                fin.clone(),
                progres.clone(),
            );
            tokio::task::spawn_blocking(move || produire(t, s, taille, f, emetteur, p));
            let corps = reqwest::Body::wrap(CorpsEnFlux {
                recepteur,
                reste: longueur,
            });
            let reponse = self
                .preparer(&methode, chemin, DELAI_TELECHARGEMENT)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={frontiere}"),
                )
                .header(CONTENT_LENGTH, longueur)
                .body(corps)
                .send()
                .await
                .map_err(|e| self.transport(&methode, chemin, e))?;
            if reponse.status() == StatusCode::UNAUTHORIZED && premiere {
                premiere = false;
                continue;
            }
            let reponse = self.conclure(&methode, chemin, reponse, false).await?;
            return self
                .lire_json(&methode, chemin, reponse)
                .await?
                .ok_or_else(|| ErreurApi::ReponseInvalide("réponse sans corps JSON".to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::types::Attachment;

    type Releves = Arc<Mutex<Vec<(u64, u64)>>>;

    fn suivi() -> (Progression, Releves) {
        let releves = Arc::new(Mutex::new(Vec::new()));
        let r = releves.clone();
        (
            Arc::new(move |n, t| r.lock().unwrap().push((n, t))),
            releves,
        )
    }

    #[tokio::test]
    async fn un_fichier_part_en_flux_avec_sa_progression() {
        let serveur =
            ServeurFactice::demarrer([Reponse::json(201, json!({"id": "f1", "name": "clip.mp4"}))])
                .await;
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("clip.mp4");
        let contenu: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&chemin, &contenu).unwrap();
        let (progres, releves) = suivi();
        let piece: Attachment = serveur
            .client()
            .envoyer_piece(
                "clip.mp4",
                "video/mp4",
                SourceEnvoi::Chemin(chemin),
                progres,
            )
            .await
            .unwrap();
        assert_eq!(piece.id.as_deref(), Some("f1"));
        let requete = &serveur.requetes()[0];
        assert_eq!(requete.ligne(), ("/files", "POST"));
        assert_eq!(
            requete.entete("content-length").unwrap(),
            requete.corps.len().to_string()
        );
        let frontiere = requete
            .entete("content-type")
            .unwrap()
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap()
            .to_string();
        let texte = requete.texte();
        assert!(texte.starts_with(&format!(
            "--{frontiere}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\nvideo\r\n"
        )));
        assert!(texte
            .contains("name=\"file\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\n"));
        let marque = b"Content-Type: video/mp4\r\n\r\n";
        let debut = requete
            .corps
            .windows(marque.len())
            .position(|f| f == marque)
            .unwrap()
            + marque.len();
        assert_eq!(&requete.corps[debut..debut + contenu.len()], &contenu[..]);
        assert!(requete
            .corps
            .ends_with(format!("\r\n--{frontiere}--\r\n").as_bytes()));
        let releves = releves.lock().unwrap().clone();
        assert_eq!(releves.first(), Some(&(0, 300_000)));
        assert_eq!(releves.last(), Some(&(300_000, 300_000)));
        assert!(releves.windows(2).all(|p| p[0].0 <= p[1].0));
    }

    #[tokio::test]
    async fn un_401_isole_renvoie_le_fichier_entier() {
        let serveur =
            ServeurFactice::demarrer([Reponse::vide(401), Reponse::json(201, json!({"id": "f2"}))])
                .await;
        let (progres, _) = suivi();
        let piece = serveur
            .client()
            .envoyer_piece(
                "a.txt",
                "text/plain",
                SourceEnvoi::Octets(Bytes::from_static(b"bonjour")),
                progres,
            )
            .await
            .unwrap();
        assert_eq!(piece.id.as_deref(), Some("f2"));
        let requetes = serveur.requetes();
        assert_eq!(requetes.len(), 2);
        assert!(requetes
            .iter()
            .all(|r| r.texte().contains("\r\n\r\nbonjour\r\n--")));
        assert!(!requetes[0].texte().contains("name=\"kind\""));
    }

    #[tokio::test]
    async fn un_refus_du_serveur_garde_son_message() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            413,
            json!({"error": "file_too_large", "message": "Fichier trop gros"}),
        )])
        .await;
        let (progres, _) = suivi();
        let erreur = serveur
            .client()
            .envoyer_piece(
                "a.bin",
                MIME,
                SourceEnvoi::Octets(Bytes::from_static(b"x")),
                progres,
            )
            .await
            .unwrap_err();
        assert_eq!((erreur.statut(), erreur.code()), (413, "file_too_large"));
    }

    const MIME: &str = "application/octet-stream";

    #[tokio::test]
    async fn annuler_l_envoi_ferme_la_connexion() {
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = format!("http://{}", ecoute.local_addr().unwrap());
        let (debut_tx, debut_rx) = tokio::sync::oneshot::channel::<()>();
        let (fin_tx, fin_rx) = tokio::sync::oneshot::channel::<u64>();
        tokio::spawn(async move {
            let (mut flux, _) = ecoute.accept().await.unwrap();
            let mut tampon = vec![0u8; 16 * 1024];
            let mut recus = 0u64;
            let mut debut = Some(debut_tx);
            loop {
                match flux.read(&mut tampon).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        recus += n as u64;
                        if let Some(d) = debut.take() {
                            let _ = d.send(());
                        }
                        tokio::task::yield_now().await;
                    }
                }
            }
            let _ = flux.shutdown().await;
            let _ = fin_tx.send(recus);
        });
        let client = Client::avec_reglages(&adresse, ServeurFactice::reglages()).unwrap();
        let envoyes = Arc::new(AtomicU64::new(0));
        let e = envoyes.clone();
        let progres: Progression = Arc::new(move |n, _| e.store(n, Ordering::SeqCst));
        let total = 64 * 1024 * 1024;
        let tache = tokio::spawn(async move {
            let source = SourceEnvoi::Octets(Bytes::from(vec![7u8; total]));
            client
                .envoyer_piece("gros.bin", MIME, source, progres)
                .await
        });
        debut_rx.await.unwrap();
        tache.abort();
        assert!(tache.await.unwrap_err().is_cancelled());
        let recus = fin_rx.await.unwrap();
        assert!(recus < total as u64, "connexion fermée avant la fin");
        assert!(envoyes.load(Ordering::SeqCst) < total as u64);
    }

    fn memoire_residente() -> Option<u64> {
        let etat = std::fs::read_to_string("/proc/self/status").ok()?;
        let ligne = etat.lines().find(|l| l.starts_with("VmRSS:"))?;
        let ko: u64 = ligne.split_whitespace().nth(1)?.parse().ok()?;
        Some(ko * 1024)
    }

    #[tokio::test]
    #[ignore = "mesure : cargo test -p estompe-client mesure_memoire -- --ignored --nocapture"]
    async fn mesure_memoire_d_un_envoi_de_500_mo() {
        let Some(avant) = memoire_residente() else {
            println!("mesure indisponible sur cet OS");
            return;
        };
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = format!("http://{}", ecoute.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut flux, _) = ecoute.accept().await.unwrap();
            let mut tampon = vec![0u8; 256 * 1024];
            let mut recu = Vec::new();
            let mut fin_tete = None;
            let mut longueur = 0u64;
            let mut corps = 0u64;
            loop {
                let n = flux.read(&mut tampon).await.unwrap();
                if n == 0 {
                    return;
                }
                match fin_tete {
                    None => {
                        recu.extend_from_slice(&tampon[..n]);
                        if let Some(p) = recu.windows(4).position(|f| f == b"\r\n\r\n") {
                            let tete = String::from_utf8_lossy(&recu[..p]).to_lowercase();
                            longueur = tete
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap();
                            corps = (recu.len() - p - 4) as u64;
                            fin_tete = Some(p);
                        }
                    }
                    Some(_) => corps += n as u64,
                }
                if fin_tete.is_some() && corps >= longueur {
                    let reponse = "{\"id\":\"f\"}";
                    let _ = flux
                        .write_all(format!("HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reponse}", reponse.len()).as_bytes())
                        .await;
                    return;
                }
            }
        });
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("gros.bin");
        let taille = 500 * 1024 * 1024;
        std::fs::File::create(&chemin)
            .unwrap()
            .set_len(taille)
            .unwrap();
        let pic = Arc::new(AtomicU64::new(avant));
        let p = pic.clone();
        let progres: Progression = Arc::new(move |_, _| {
            if let Some(m) = memoire_residente() {
                p.fetch_max(m, Ordering::SeqCst);
            }
        });
        let client = Client::avec_reglages(&adresse, ServeurFactice::reglages()).unwrap();
        let piece = client
            .envoyer_piece("gros.bin", MIME, SourceEnvoi::Chemin(chemin), progres)
            .await
            .unwrap();
        assert_eq!(piece.id.as_deref(), Some("f"));
        let croissance = pic.load(Ordering::SeqCst).saturating_sub(avant);
        println!(
            "envoi de 500 Mo : mémoire résidente avant {} Mo, croissance maximale {:.1} Mo",
            avant / 1_048_576,
            croissance as f64 / 1_048_576.0
        );
        assert!(croissance < 64 * 1024 * 1024);
    }
}
