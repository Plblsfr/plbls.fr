use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::http::{Client, Reglages};

#[derive(Debug, Clone)]
pub struct Reponse {
    statut: u16,
    entetes: Vec<(String, String)>,
    corps: Vec<u8>,
}

impl Reponse {
    pub fn vide(statut: u16) -> Self {
        Self {
            statut,
            entetes: Vec::new(),
            corps: Vec::new(),
        }
    }

    pub fn json(statut: u16, corps: serde_json::Value) -> Self {
        Self::brute(statut, "application/json", corps.to_string().as_bytes())
    }

    pub fn brute(statut: u16, type_mime: &str, corps: &[u8]) -> Self {
        Self::vide(statut)
            .avec_entete("content-type", type_mime)
            .avec_corps(corps)
    }

    pub fn avec_entete(mut self, nom: &str, valeur: &str) -> Self {
        self.entetes.push((nom.to_string(), valeur.to_string()));
        self
    }

    fn avec_corps(mut self, corps: &[u8]) -> Self {
        self.corps = corps.to_vec();
        self
    }

    fn octets(&self) -> Vec<u8> {
        let raison = StatusCode::from_u16(self.statut)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or("Inconnu");
        let mut tete = format!("HTTP/1.1 {} {raison}\r\nconnection: close\r\n", self.statut);
        for (nom, valeur) in &self.entetes {
            tete += &format!("{nom}: {valeur}\r\n");
        }
        if self.statut != 204 {
            tete += &format!("content-length: {}\r\n", self.corps.len());
        }
        [tete.as_bytes(), b"\r\n", &self.corps].concat()
    }
}

#[derive(Debug, Clone)]
pub enum Comportement {
    Repondre(Reponse),
    Silence,
}

impl From<Reponse> for Comportement {
    fn from(reponse: Reponse) -> Self {
        Self::Repondre(reponse)
    }
}

#[derive(Debug, Clone)]
pub struct Requete {
    pub methode: String,
    pub chemin: String,
    pub entetes: Vec<(String, String)>,
    pub corps: Vec<u8>,
}

impl Requete {
    pub fn entete(&self, nom: &str) -> Option<&str> {
        self.entetes
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nom))
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.corps).unwrap()
    }

    pub fn texte(&self) -> String {
        String::from_utf8_lossy(&self.corps).into_owned()
    }

    pub fn ligne(&self) -> (&str, &str) {
        (self.chemin.as_str(), self.methode.as_str())
    }
}

#[derive(Default)]
struct Etat {
    file: VecDeque<Comportement>,
    requetes: Vec<Requete>,
}

pub struct ServeurFactice {
    adresse: String,
    etat: Arc<Mutex<Etat>>,
    tache: JoinHandle<()>,
}

impl ServeurFactice {
    pub async fn demarrer<C: Into<Comportement>>(reponses: impl IntoIterator<Item = C>) -> Self {
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = format!("http://{}", ecoute.local_addr().unwrap());
        let etat = Arc::new(Mutex::new(Etat::default()));
        let partage = etat.clone();
        let tache = tokio::spawn(async move {
            while let Ok((flux, _)) = ecoute.accept().await {
                tokio::spawn(servir(flux, partage.clone()));
            }
        });
        let serveur = Self {
            adresse,
            etat,
            tache,
        };
        serveur.programmer(reponses);
        serveur
    }

    pub fn programmer<C: Into<Comportement>>(&self, reponses: impl IntoIterator<Item = C>) {
        self.etat.lock().unwrap().file = reponses.into_iter().map(Into::into).collect();
    }

    pub fn adresse(&self) -> &str {
        &self.adresse
    }

    pub fn requetes(&self) -> Vec<Requete> {
        self.etat.lock().unwrap().requetes.clone()
    }

    pub fn lignes(&self) -> Vec<(String, String)> {
        self.requetes()
            .into_iter()
            .map(|r| (r.chemin, r.methode))
            .collect()
    }

    pub fn reglages() -> Reglages {
        Reglages {
            mandataire_systeme: false,
            ..Reglages::default()
        }
    }

    pub fn client(&self) -> Client {
        Client::avec_reglages(&self.adresse, Self::reglages()).unwrap()
    }
}

impl Drop for ServeurFactice {
    fn drop(&mut self) {
        self.tache.abort();
    }
}

fn position(octets: &[u8], motif: &[u8]) -> Option<usize> {
    octets
        .windows(motif.len())
        .position(|fenetre| fenetre == motif)
}

async fn lire_requete(flux: &mut TcpStream) -> Option<Requete> {
    let mut recu = Vec::new();
    let mut tampon = [0u8; 8192];
    let fin_tete = loop {
        if let Some(p) = position(&recu, b"\r\n\r\n") {
            break p;
        }
        let lus = flux.read(&mut tampon).await.ok()?;
        if lus == 0 {
            return None;
        }
        recu.extend_from_slice(&tampon[..lus]);
    };
    let tete = String::from_utf8_lossy(&recu[..fin_tete]).into_owned();
    let mut lignes = tete.split("\r\n");
    let mut premiere = lignes.next()?.split(' ');
    let (methode, chemin) = (premiere.next()?.to_string(), premiere.next()?.to_string());
    let entetes: Vec<(String, String)> = lignes
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let longueur = entetes
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut corps = recu[fin_tete + 4..].to_vec();
    while corps.len() < longueur {
        let lus = flux.read(&mut tampon).await.ok()?;
        if lus == 0 {
            return None;
        }
        corps.extend_from_slice(&tampon[..lus]);
    }
    Some(Requete {
        methode,
        chemin,
        entetes,
        corps,
    })
}

async fn servir(mut flux: TcpStream, etat: Arc<Mutex<Etat>>) {
    let Some(requete) = lire_requete(&mut flux).await else {
        return;
    };
    let comportement = {
        let mut etat = etat.lock().unwrap();
        etat.requetes.push(requete);
        if etat.file.len() > 1 {
            etat.file.pop_front()
        } else {
            etat.file.front().cloned()
        }
    };
    match comportement.unwrap_or(Comportement::Repondre(Reponse::vide(500))) {
        Comportement::Repondre(reponse) => {
            let _ = flux.write_all(&reponse.octets()).await;
            let _ = flux.shutdown().await;
        }
        Comportement::Silence => std::future::pending::<()>().await,
    }
}
