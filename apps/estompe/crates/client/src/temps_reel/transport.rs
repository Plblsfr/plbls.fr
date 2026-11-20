use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{
    connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};

pub type Avenir<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const DELAI_FERMETURE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrameRecue {
    Texte(String),
    Fermeture(Option<u16>),
}

pub trait Canal: Send + 'static {
    fn envoyer(&mut self, texte: String) -> Avenir<'_, Result<(), String>>;
    fn recevoir(&mut self) -> Avenir<'_, TrameRecue>;
    fn fermer(&mut self, code: u16) -> Avenir<'_, ()>;
}

pub trait Transport: Send + Sync + 'static {
    fn ouvrir(&self, adresse: String) -> Avenir<'static, Result<Box<dyn Canal>, String>>;
}

pub struct TransportWebSocket {
    tls: Result<Arc<ClientConfig>, String>,
}

impl TransportWebSocket {
    pub fn nouveau() -> Self {
        let fournisseur = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let tls = ClientConfig::builder_with_provider(fournisseur)
            .with_safe_default_protocol_versions()
            .and_then(|c| c.with_platform_verifier())
            .map(|c| Arc::new(c.with_no_client_auth()))
            .map_err(|e| e.to_string());
        Self { tls }
    }
}

impl Default for TransportWebSocket {
    fn default() -> Self {
        Self::nouveau()
    }
}

impl Transport for TransportWebSocket {
    fn ouvrir(&self, adresse: String) -> Avenir<'static, Result<Box<dyn Canal>, String>> {
        let tls = self.tls.clone();
        Box::pin(async move {
            let connecteur = if adresse.starts_with("wss:") {
                Connector::Rustls(tls?)
            } else {
                Connector::Plain
            };
            let (flux, _) = connect_async_tls_with_config(adresse, None, true, Some(connecteur))
                .await
                .map_err(|e| e.to_string())?;
            Ok(Box::new(CanalWebSocket(flux)) as Box<dyn Canal>)
        })
    }
}

struct CanalWebSocket(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl Canal for CanalWebSocket {
    fn envoyer(&mut self, texte: String) -> Avenir<'_, Result<(), String>> {
        Box::pin(async move {
            self.0
                .send(Message::text(texte))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn recevoir(&mut self) -> Avenir<'_, TrameRecue> {
        Box::pin(async move {
            loop {
                match self.0.next().await {
                    Some(Ok(Message::Text(texte))) => return TrameRecue::Texte(texte.to_string()),
                    Some(Ok(Message::Binary(octets))) => {
                        if let Ok(texte) = String::from_utf8(octets.to_vec()) {
                            return TrameRecue::Texte(texte);
                        }
                    }
                    Some(Ok(Message::Close(cadre))) => {
                        return TrameRecue::Fermeture(cadre.map(|c| u16::from(c.code)))
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return TrameRecue::Fermeture(None),
                }
            }
        })
    }

    fn fermer(&mut self, code: u16) -> Avenir<'_, ()> {
        Box::pin(async move {
            let cadre = CloseFrame {
                code: code.into(),
                reason: "".into(),
            };
            let _ = tokio::time::timeout(DELAI_FERMETURE, self.0.close(Some(cadre))).await;
        })
    }
}
