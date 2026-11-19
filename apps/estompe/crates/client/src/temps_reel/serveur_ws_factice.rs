use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Default)]
pub struct EtatServeur {
    pub connexions: usize,
    pub refusees: usize,
    pub tickets: Vec<String>,
    pub recus: Vec<String>,
    pub fermees: usize,
}

#[derive(Default)]
struct Partage {
    refuser: bool,
    courante: Option<(mpsc::UnboundedSender<String>, JoinHandle<()>)>,
}

pub struct ServeurWs {
    adresse: String,
    partage: Arc<Mutex<Partage>>,
    etat: watch::Sender<EtatServeur>,
    tache: JoinHandle<()>,
}

fn ticket_de(requete: &Request) -> String {
    requete
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .find_map(|p| p.strip_prefix("ticket="))
        .unwrap_or_default()
        .to_string()
}

struct LireTicket<'a>(&'a mut String);

impl Callback for LireTicket<'_> {
    fn on_request(self, requete: &Request, reponse: Response) -> Result<Response, ErrorResponse> {
        *self.0 = ticket_de(requete);
        Ok(reponse)
    }
}

impl ServeurWs {
    pub async fn demarrer() -> Self {
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = format!("ws://{}/ws", ecoute.local_addr().unwrap());
        let partage = Arc::new(Mutex::new(Partage::default()));
        let (etat, _) = watch::channel(EtatServeur::default());
        let tache = tokio::spawn({
            let partage = partage.clone();
            let etat = etat.clone();
            async move {
                while let Ok((flux, _)) = ecoute.accept().await {
                    if partage.lock().unwrap().refuser {
                        drop(flux);
                        etat.send_modify(|e| e.refusees += 1);
                        continue;
                    }
                    let mut ticket = String::new();
                    let ws =
                        tokio_tungstenite::accept_hdr_async(flux, LireTicket(&mut ticket)).await;
                    let Ok(ws) = ws else { continue };
                    let (vers_client, mut a_envoyer) = mpsc::unbounded_channel::<String>();
                    let etat_connexion = etat.clone();
                    let connexion = tokio::spawn(async move {
                        let (mut envoi, mut reception) = ws.split();
                        loop {
                            tokio::select! {
                                texte = a_envoyer.recv() => match texte {
                                    Some(texte) => {
                                        if envoi.send(Message::text(texte)).await.is_err() {
                                            break;
                                        }
                                    }
                                    None => break,
                                },
                                message = reception.next() => match message {
                                    Some(Ok(Message::Text(texte))) => {
                                        etat_connexion.send_modify(|e| e.recus.push(texte.to_string()));
                                    }
                                    Some(Ok(_)) => {}
                                    Some(Err(_)) | None => break,
                                },
                            }
                        }
                        etat_connexion.send_modify(|e| e.fermees += 1);
                    });
                    if let Some((_, precedente)) = partage
                        .lock()
                        .unwrap()
                        .courante
                        .replace((vers_client, connexion))
                    {
                        precedente.abort();
                    }
                    etat.send_modify(|e| {
                        e.connexions += 1;
                        e.tickets.push(ticket);
                    });
                }
            }
        });
        Self {
            adresse,
            partage,
            etat,
            tache,
        }
    }

    pub fn adresse(&self) -> &str {
        &self.adresse
    }

    pub fn etat(&self) -> EtatServeur {
        self.etat.borrow().clone()
    }

    pub async fn attendre(&self, condition: impl Fn(&EtatServeur) -> bool) -> EtatServeur {
        let mut suivi = self.etat.subscribe();
        let etat = suivi.wait_for(|e| condition(e)).await.unwrap().clone();
        etat
    }

    pub fn publier(&self, trame: serde_json::Value) {
        let partage = self.partage.lock().unwrap();
        let (vers_client, _) = partage.courante.as_ref().expect("aucune connexion");
        vers_client.send(trame.to_string()).unwrap();
    }

    pub fn couper(&self) {
        let mut partage = self.partage.lock().unwrap();
        partage.refuser = true;
        if let Some((_, connexion)) = partage.courante.take() {
            connexion.abort();
        }
    }

    pub fn retablir(&self) {
        self.partage.lock().unwrap().refuser = false;
    }
}

impl Drop for ServeurWs {
    fn drop(&mut self) {
        self.tache.abort();
        if let Some((_, connexion)) = self.partage.lock().unwrap().courante.take() {
            connexion.abort();
        }
    }
}
