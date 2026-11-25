use std::sync::{Arc, Mutex, MutexGuard};

use crate::http::encoder_composant;

use super::moteur::Source;
use super::Lecteur;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreMedia {
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origine {
    Piece { message: String, piece: String },
    Autre(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaActif {
    pub origine: Origine,
    pub genre: GenreMedia,
    pub agrandi: bool,
}

impl MediaActif {
    pub fn piece(&self) -> Option<&str> {
        match &self.origine {
            Origine::Piece { piece, .. } => Some(piece),
            Origine::Autre(_) => None,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match &self.origine {
            Origine::Piece { message, .. } => Some(message),
            Origine::Autre(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Visible,
    HorsEcran,
    Absente,
}

pub struct LectureUnique {
    lecteur: Arc<Lecteur>,
    actif: Mutex<Option<MediaActif>>,
}

impl LectureUnique {
    pub fn nouvelle(lecteur: Arc<Lecteur>) -> Self {
        Self {
            lecteur,
            actif: Mutex::new(None),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Option<MediaActif>> {
        self.actif.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn lecteur(&self) -> &Arc<Lecteur> {
        &self.lecteur
    }

    pub fn actif(&self) -> Option<MediaActif> {
        self.verrou().clone()
    }

    pub fn piece_active(&self) -> Option<String> {
        self.verrou()
            .as_ref()
            .and_then(|a| a.piece().map(str::to_string))
    }

    pub fn lancer(&self, origine: Origine, genre: GenreMedia, agrandi: bool, source: Source) {
        *self.verrou() = Some(MediaActif {
            origine,
            genre,
            agrandi,
        });
        self.lecteur.definir_visible(genre == GenreMedia::Video);
        self.lecteur.charger(source);
    }

    pub fn reserver(&self, origine: Origine, genre: GenreMedia) {
        *self.verrou() = Some(MediaActif {
            origine,
            genre,
            agrandi: true,
        });
        self.lecteur.definir_visible(genre == GenreMedia::Video);
    }

    pub fn liberer(&self, origine: &Origine) -> bool {
        let a_moi = self
            .verrou()
            .as_ref()
            .is_some_and(|a| &a.origine == origine);
        a_moi && self.arreter()
    }

    pub fn lire_piece(&self, message: &str, piece: &str, genre: GenreMedia, source: Source) {
        if self.piece_active().as_deref() == Some(piece) {
            self.lecteur.basculer();
            return;
        }
        self.lancer(
            Origine::Piece {
                message: message.to_string(),
                piece: piece.to_string(),
            },
            genre,
            false,
            source,
        );
    }

    pub fn agrandir(&self, piece: &str) -> bool {
        let agrandi = match self.verrou().as_mut() {
            Some(a) if a.piece() == Some(piece) && a.genre == GenreMedia::Video => {
                a.agrandi = true;
                true
            }
            _ => false,
        };
        if agrandi {
            self.lecteur.definir_visible(true);
        }
        agrandi
    }

    pub fn arreter(&self) -> bool {
        if self.verrou().take().is_none() {
            return false;
        }
        self.lecteur.arreter();
        self.lecteur.definir_plein_ecran(false);
        self.lecteur.definir_visible(false);
        true
    }

    pub fn suivre(&self, presence: impl Fn(&str) -> Presence) -> bool {
        let arreter = self
            .verrou()
            .as_ref()
            .filter(|a| !a.agrandi)
            .and_then(|a| Some((a.genre, presence(a.message()?))))
            .is_some_and(|ecart| {
                matches!(
                    ecart,
                    (_, Presence::Absente) | (GenreMedia::Video, Presence::HorsEcran)
                )
            });
        arreter && self.arreter()
    }
}

pub fn source_de_piece(adresse: &str, piece: &str, jeton: Option<String>) -> Source {
    let source = Source::adresse(format!(
        "{}/files/{}",
        adresse.trim_end_matches('/'),
        encoder_composant(piece)
    ));
    match jeton.filter(|j| !j.is_empty()) {
        Some(jeton) => source.avec_jeton(jeton),
        None => source,
    }
}

pub fn jeu_du_nom(nom: &str) -> Option<String> {
    let radical = nom
        .len()
        .checked_sub(4)
        .filter(|&i| nom.is_char_boundary(i) && nom[i..].eq_ignore_ascii_case(".mp4"))
        .map(|i| &nom[..i])?;
    let (jeu, date) = radical
        .len()
        .checked_sub(20)
        .and_then(|i| radical.is_char_boundary(i).then(|| radical.split_at(i)))?;
    let motif = "_dddd-dd-dd_dd-dd-dd";
    let conforme = date.len() == motif.len()
        && date.bytes().zip(motif.bytes()).all(|(c, m)| match m {
            b'd' => c.is_ascii_digit(),
            _ => c == m,
        });
    (conforme && !jeu.is_empty()).then(|| jeu.replace('_', " "))
}

#[cfg(test)]
mod tests {
    use super::super::{CommandeMoteur, MediaFactice, MoteurFactice, Phase};
    use super::*;

    const VIDEO: &str = "https://api.plbls.fr/files/v1";
    const SON: &str = "https://api.plbls.fr/files/s1";

    fn lecture() -> (LectureUnique, MoteurFactice) {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media(VIDEO, MediaFactice::video(30.0));
        moteur.ajouter_media(SON, MediaFactice::son(200.0));
        (LectureUnique::nouvelle(Arc::new(lecteur)), moteur)
    }

    fn presence(
        visible: &'static [&'static str],
        presents: &'static [&'static str],
    ) -> impl Fn(&str) -> Presence {
        move |m| {
            if visible.contains(&m) {
                Presence::Visible
            } else if presents.contains(&m) {
                Presence::HorsEcran
            } else {
                Presence::Absente
            }
        }
    }

    #[test]
    fn la_source_d_une_piece_porte_le_jeton_en_en_tete() {
        let s = source_de_piece("https://api.plbls.fr/", "a b", Some("j".into()));
        assert_eq!(s.adresse, "https://api.plbls.fr/files/a%20b");
        assert_eq!(s.entetes_http(), ["Authorization: Bearer j"]);
        assert!(source_de_piece("https://x", "f", Some(String::new()))
            .jeton
            .is_none());
    }

    #[test]
    fn une_seule_lecture_a_la_fois_la_derniere_remplace_l_autre() {
        let (l, moteur) = lecture();
        l.lire_piece("m1", "v1", GenreMedia::Video, Source::adresse(VIDEO));
        assert_eq!(l.piece_active().as_deref(), Some("v1"));
        assert!(l.lecteur().etat().visible);
        l.lire_piece("m2", "s1", GenreMedia::Audio, Source::adresse(SON));
        assert_eq!(l.piece_active().as_deref(), Some("s1"));
        assert!(
            !l.lecteur().etat().visible,
            "un son n'a pas d'image à décoder"
        );
        assert_eq!(l.lecteur().etat().duree, Some(200.0));
        let charges: Vec<String> = moteur
            .commandes()
            .into_iter()
            .filter_map(|c| match c {
                CommandeMoteur::Charger(s) => Some(s.adresse),
                _ => None,
            })
            .collect();
        assert_eq!(charges, [VIDEO, SON]);
        moteur.vider_commandes();
        l.lire_piece("m2", "s1", GenreMedia::Audio, Source::adresse(SON));
        assert_eq!(moteur.commandes(), [CommandeMoteur::Pause(true)]);
        l.lancer(
            Origine::Autre("clip".into()),
            GenreMedia::Video,
            true,
            Source::adresse(VIDEO),
        );
        assert_eq!(l.piece_active(), None);
    }

    #[test]
    fn une_video_hors_de_l_ecran_s_arrete_et_libere_son_decodeur() {
        let (l, moteur) = lecture();
        l.lire_piece("m1", "v1", GenreMedia::Video, Source::adresse(VIDEO));
        moteur.avancer(3.0);
        assert!(!l.suivre(presence(&["m1"], &["m1"])));
        moteur.vider_commandes();
        assert!(l.suivre(presence(&[], &["m1"])));
        assert_eq!(
            moteur.commandes(),
            [CommandeMoteur::Arreter, CommandeMoteur::Video(false)]
        );
        assert_eq!(l.actif(), None);
        assert_eq!(l.lecteur().etat().phase, Phase::Vide);
        assert!(!l.suivre(presence(&[], &[])));
    }

    #[test]
    fn un_son_continue_hors_de_l_ecran_et_s_arrete_hors_de_la_conversation() {
        let (l, moteur) = lecture();
        l.lire_piece("m2", "s1", GenreMedia::Audio, Source::adresse(SON));
        assert!(!l.suivre(presence(&[], &["m2"])));
        moteur.vider_commandes();
        assert!(l.suivre(presence(&[], &[])));
        assert_eq!(moteur.commandes()[0], CommandeMoteur::Arreter);
        assert!(!l.arreter());
    }

    #[test]
    fn une_video_agrandie_ne_depend_plus_de_sa_ligne() {
        let (l, moteur) = lecture();
        l.lire_piece("m1", "v1", GenreMedia::Video, Source::adresse(VIDEO));
        assert!(!l.agrandir("autre"));
        assert!(l.agrandir("v1"));
        moteur.vider_commandes();
        assert!(!l.suivre(presence(&[], &[])));
        assert!(moteur.commandes().is_empty());
        assert!(l.arreter());
        assert!(!l.lecteur().etat().visible);
    }

    #[test]
    fn une_seance_reserve_le_lecteur_et_ne_le_libere_que_si_elle_l_a_encore() {
        let (l, moteur) = lecture();
        l.lire_piece("m1", "v1", GenreMedia::Video, Source::adresse(VIDEO));
        let seance = Origine::Autre("seance".into());
        l.reserver(seance.clone(), GenreMedia::Video);
        assert_eq!(l.piece_active(), None);
        assert!(l.lecteur().etat().visible);
        moteur.vider_commandes();
        assert!(!l.suivre(presence(&[], &[])));
        assert!(moteur.commandes().is_empty());
        l.lire_piece("m2", "s1", GenreMedia::Audio, Source::adresse(SON));
        assert!(!l.liberer(&seance));
        assert_eq!(l.piece_active().as_deref(), Some("s1"));
        l.reserver(seance.clone(), GenreMedia::Video);
        moteur.vider_commandes();
        assert!(l.liberer(&seance));
        assert_eq!(moteur.commandes()[0], CommandeMoteur::Arreter);
        assert_eq!(l.actif(), None);
    }

    #[test]
    fn le_jeu_est_lu_dans_le_nom_d_un_replay() {
        assert_eq!(
            jeu_du_nom("Rocket_League_2026-11-19_21-04-33.mp4").as_deref(),
            Some("Rocket League")
        );
        assert_eq!(
            jeu_du_nom("Valorant_2026-01-02_03-04-05.MP4").as_deref(),
            Some("Valorant")
        );
        assert_eq!(jeu_du_nom("vacances.mp4"), None);
        assert_eq!(jeu_du_nom("_2026-01-02_03-04-05.mp4"), None);
        assert_eq!(jeu_du_nom("Jeu_2026-01-02_03-04-05.webm"), None);
        assert_eq!(jeu_du_nom("Jeu_2026-01-0x_03-04-05.mp4"), None);
        assert_eq!(jeu_du_nom("é.mp4"), None);
    }
}
