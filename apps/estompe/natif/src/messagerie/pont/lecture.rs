use std::sync::Arc;

use estompe_client::lecteur::{source_de_piece, GenreMedia, Presence};

use super::super::lignes::GenrePiece;
use super::{verrou_de, PontMessagerie};
use crate::video::lecteur::GestionLecteur;
use crate::Principale;

pub fn genre_media(genre: GenrePiece) -> Option<GenreMedia> {
    match genre {
        GenrePiece::Video => Some(GenreMedia::Video),
        GenrePiece::Audio => Some(GenreMedia::Audio),
        _ => None,
    }
}

impl PontMessagerie {
    pub fn definir_lecteur(&self, lecteur: Arc<GestionLecteur>) {
        *self.lecteur.lock().unwrap_or_else(|e| e.into_inner()) = Some(lecteur);
    }

    pub fn lecteur(&self) -> Option<Arc<GestionLecteur>> {
        verrou_de(&self.lecteur)
    }

    pub fn arreter_media(&self) {
        if let Some(gestion) = self.lecteur() {
            gestion.arreter();
        }
    }

    pub fn lire_media(&self, f: &Principale, message: &str, piece: &str) {
        let Some(gestion) = self.lecteur() else {
            return;
        };
        let Some(genre) = self
            .piece_du_message(message, piece)
            .and_then(|p| genre_media(p.genre))
        else {
            return;
        };
        self.arreter_lecture(f);
        let client = self.store.client();
        gestion.lire_piece(
            message,
            piece,
            genre,
            source_de_piece(client.adresse(), piece, client.jeton()),
        );
    }

    pub fn agrandir_media(&self, f: &Principale, message: &str, piece: &str) {
        let Some(gestion) = self.lecteur() else {
            return;
        };
        let Some(donnees) = self
            .piece_du_message(message, piece)
            .filter(|p| p.genre == GenrePiece::Video)
        else {
            return;
        };
        if gestion.lecture().piece_active().as_deref() != Some(piece) {
            self.lire_media(f, message, piece);
        }
        gestion.agrandir(&donnees.nom, piece, false);
    }

    pub fn telecharger_media(&self, f: &Principale) {
        let Some(actif) = self.lecteur().and_then(|g| g.lecture().actif()) else {
            return;
        };
        if let (Some(message), Some(piece)) = (actif.message(), actif.piece()) {
            self.telecharger_piece(f, message, piece);
        }
    }

    pub(super) fn suivre_media(&self) {
        let Some(gestion) = self.lecteur() else {
            return;
        };
        let Some(message) = gestion
            .lecture()
            .actif()
            .filter(|a| !a.agrandi)
            .and_then(|a| a.message().map(str::to_string))
        else {
            return;
        };
        let presence = {
            let i = self.verrou();
            if !i.index.contains_key(&message) {
                Presence::Absente
            } else if i.defilement.visible(&message) {
                Presence::Visible
            } else {
                Presence::HorsEcran
            }
        };
        gestion.suivre(|_| presence);
    }
}
