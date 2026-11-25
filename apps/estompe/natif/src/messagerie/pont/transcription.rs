use estompe_client::http::ErreurApi;
use estompe_client::transcription::{proposee, Suivi};

use super::{verrou_de, LireAlpha, PontMessagerie};
use crate::Principale;

impl PontMessagerie {
    pub fn definir_alpha(&self, lire: LireAlpha) {
        *self.alpha.lock().unwrap_or_else(|e| e.into_inner()) = lire;
    }

    pub(super) fn contexte_transcription(&self) -> (bool, String) {
        let alpha = verrou_de(&self.alpha)();
        self.store
            .lire(|e| (proposee(e.compte.transcription, alpha), e.moi().to_string()))
    }

    pub fn noter_transcription(&self, piece: &str, suivi: Suivi) {
        self.noter_piece(piece, |e| e.transcription = suivi);
    }

    pub fn transcription_proposee(&self) -> bool {
        self.contexte_transcription().0
    }

    pub fn transcrire(&self, f: &Principale, piece: &str) {
        if !self.transcription_proposee() {
            return;
        }
        let mut lancer = false;
        self.noter_piece(piece, |e| lancer = e.transcription.demander());
        if !lancer {
            return;
        }
        self.rafraichir(f);
        let pont = self.clone();
        let piece = piece.to_string();
        self.runtime.spawn(async move {
            let resultat = pont.store.transcrire(&piece).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.transcription_recue(&f, &piece, resultat));
        });
    }

    fn transcription_recue(&self, f: &Principale, piece: &str, resultat: Result<(), ErreurApi>) {
        self.noter_piece(piece, |e| match &resultat {
            Ok(()) => e.transcription.demande_aboutie(),
            Err(erreur) => e.transcription.demande_echouee(erreur),
        });
        self.rafraichir(f);
    }

    pub fn transcrire_apres_envoi(&self, piece: &str) {
        if !self.transcription_proposee() {
            return;
        }
        let store = self.store.clone();
        let piece = piece.to_string();
        self.runtime.spawn(async move {
            let _ = store.transcrire(&piece).await;
        });
    }

    pub fn basculer_transcription(&self, f: &Principale, piece: &str) {
        self.noter_piece(piece, |e| e.transcription.basculer_masquee());
        self.rafraichir(f);
    }

    pub fn deplier_transcription(&self, f: &Principale, piece: &str) {
        self.noter_piece(piece, |e| e.transcription.basculer_depliee());
        self.rafraichir(f);
    }

    pub fn modifier_transcription(&self, f: &Principale, message: &str, piece: &str) {
        let Some(texte) = self
            .piece_du_message(message, piece)
            .and_then(|p| p.transcription)
        else {
            return;
        };
        self.noter_piece(piece, |e| e.transcription.modifier(&texte));
        self.rafraichir(f);
    }

    pub fn ecrire_transcription(&self, piece: &str, texte: &str) {
        self.noter_piece(piece, |e| e.transcription.ecrire(texte));
    }

    pub fn annuler_transcription(&self, f: &Principale, piece: &str) {
        self.noter_piece(piece, |e| e.transcription.annuler());
        self.rafraichir(f);
    }

    pub fn enregistrer_transcription(&self, f: &Principale, piece: &str, texte: &str) {
        let mut a_envoyer = None;
        self.noter_piece(piece, |e| a_envoyer = e.transcription.enregistrer(texte));
        let Some(texte) = a_envoyer else {
            return;
        };
        self.rafraichir(f);
        let pont = self.clone();
        let piece = piece.to_string();
        self.runtime.spawn(async move {
            let resultat = pont.store.modifier_transcription(&piece, &texte).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.transcription_enregistree(&f, &piece, resultat));
        });
    }

    fn transcription_enregistree(
        &self,
        f: &Principale,
        piece: &str,
        resultat: Result<(), ErreurApi>,
    ) {
        self.noter_piece(piece, |e| match &resultat {
            Ok(()) => e.transcription.enregistrement_abouti(),
            Err(erreur) => e.transcription.enregistrement_echoue(erreur),
        });
        self.rafraichir(f);
    }
}
