use tokio::sync::{mpsc, oneshot};

use super::transport::{Avenir, Canal, TrameRecue, Transport};

pub struct FauxTransport {
    essais: mpsc::UnboundedSender<Essai>,
}

pub fn faux_transport() -> (FauxTransport, mpsc::UnboundedReceiver<Essai>) {
    let (essais, recus) = mpsc::unbounded_channel();
    (FauxTransport { essais }, recus)
}

impl Transport for FauxTransport {
    fn ouvrir(&self, adresse: String) -> Avenir<'static, Result<Box<dyn Canal>, String>> {
        let (reponse, attente) = oneshot::channel();
        let _ = self.essais.send(Essai { adresse, reponse });
        Box::pin(async move { attente.await.unwrap_or_else(|_| Err("abandon".to_string())) })
    }
}

pub struct Essai {
    pub adresse: String,
    reponse: oneshot::Sender<Result<Box<dyn Canal>, String>>,
}

impl Essai {
    pub fn ouvrir(self) -> FauxWs {
        let (vers_client, entree) = mpsc::unbounded_channel();
        let (sortie, du_client) = mpsc::unbounded_channel();
        let _ = self
            .reponse
            .send(Ok(Box::new(FauxCanal { entree, sortie }) as Box<dyn Canal>));
        FauxWs {
            vers_client,
            du_client,
        }
    }

    pub fn refuser(self) {
        let _ = self.reponse.send(Err("refusé".to_string()));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sortant {
    Texte(String),
    Fermeture(u16),
}

pub struct FauxWs {
    vers_client: mpsc::UnboundedSender<TrameRecue>,
    du_client: mpsc::UnboundedReceiver<Sortant>,
}

impl FauxWs {
    pub fn recevoir(&self, trame: serde_json::Value) {
        let _ = self.vers_client.send(TrameRecue::Texte(trame.to_string()));
    }

    pub fn couper(&self, code: u16) {
        let _ = self.vers_client.send(TrameRecue::Fermeture(Some(code)));
    }

    pub fn sortants(&mut self) -> Vec<Sortant> {
        let mut sortants = Vec::new();
        while let Ok(s) = self.du_client.try_recv() {
            sortants.push(s);
        }
        sortants
    }

    pub fn textes(&mut self) -> Vec<String> {
        self.sortants()
            .into_iter()
            .filter_map(|s| match s {
                Sortant::Texte(t) => Some(t),
                Sortant::Fermeture(_) => None,
            })
            .collect()
    }
}

struct FauxCanal {
    entree: mpsc::UnboundedReceiver<TrameRecue>,
    sortie: mpsc::UnboundedSender<Sortant>,
}

impl Canal for FauxCanal {
    fn envoyer(&mut self, texte: String) -> Avenir<'_, Result<(), String>> {
        let _ = self.sortie.send(Sortant::Texte(texte));
        Box::pin(async { Ok(()) })
    }

    fn recevoir(&mut self) -> Avenir<'_, TrameRecue> {
        Box::pin(async move {
            self.entree
                .recv()
                .await
                .unwrap_or(TrameRecue::Fermeture(None))
        })
    }

    fn fermer(&mut self, code: u16) -> Avenir<'_, ()> {
        let _ = self.sortie.send(Sortant::Fermeture(code));
        Box::pin(async {})
    }
}
