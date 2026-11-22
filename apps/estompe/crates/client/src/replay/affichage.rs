use std::time::Duration;

use crate::overlay::{CoinOverlay, TailleLogique};

use super::automate::Etat;
use super::reglages::Options;
use super::types::{ClipEnregistre, Codec, ErreurReplay, DUREE_MAX_S, DUREE_MIN_S, QUOTA_MIN};

pub const MESSAGES_MAX: usize = 3;
pub const LARGEUR: u32 = 320;
pub const HAUTEUR_INDICATEUR: u32 = 26;
pub const LIGNE: u32 = 44;
pub const ECART: u32 = 6;
pub const PAS_DUREE_S: u32 = 15;
pub const GO: u64 = QUOTA_MIN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenreMessage {
    Ok,
    Info,
    Erreur,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageReplay {
    pub genre: GenreMessage,
    pub texte: String,
}

impl MessageReplay {
    pub fn nouveau(genre: GenreMessage, texte: impl Into<String>) -> Self {
        Self {
            genre,
            texte: texte.into(),
        }
    }
}

pub fn duree_clip(secondes: f64) -> String {
    let t = if secondes.is_finite() {
        secondes.max(0.0).round() as u64
    } else {
        0
    };
    format!("{}:{:02}", t / 60, t % 60)
}

pub fn duree_tampon(secondes: u32) -> String {
    match (secondes / 60, secondes % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s:02}"),
    }
}

pub fn taille(octets: u64) -> String {
    let virgule = |n: f64| {
        let texte = if n < 10.0 {
            format!("{n:.1}")
        } else {
            format!("{n:.0}")
        };
        texte.replace('.', ",").trim_end_matches(",0").to_string()
    };
    if octets >= GO {
        format!("{} Go", virgule(octets as f64 / GO as f64))
    } else if octets >= 1024 * 1024 {
        format!("{} Mo", (octets as f64 / (1024.0 * 1024.0)).round())
    } else {
        let ko = (octets as f64 / 1024.0).round() as u64;
        format!("{} Ko", if octets > 0 { ko.max(1) } else { 0 })
    }
}

pub fn nom_codec(codec: Codec) -> &'static str {
    match codec {
        Codec::H264 => "H.264",
        Codec::Hevc => "HEVC",
    }
}

pub fn duree_max(options: Option<&Options>) -> u32 {
    options
        .map_or(DUREE_MAX_S, |o| o.duree_max_s)
        .clamp(DUREE_MIN_S, DUREE_MAX_S)
}

pub fn duree_choisie(valeur: f32, maximum: u32) -> u32 {
    let valeur = if valeur.is_finite() { valeur } else { 0.0 };
    let pas = (valeur / PAS_DUREE_S as f32).round().max(0.0) as u32 * PAS_DUREE_S;
    pas.clamp(DUREE_MIN_S, maximum.clamp(DUREE_MIN_S, DUREE_MAX_S))
}

pub fn quota_choisi(go: f32) -> u64 {
    let go = if go.is_finite() { go.round() } else { 0.0 };
    (go.max(0.0) as u64).saturating_mul(GO).max(QUOTA_MIN)
}

pub fn message_etat(avant: &Etat, apres: &Etat) -> Option<MessageReplay> {
    let change = avant != apres;
    let suite = |r: Option<String>| r.map(|r| format!(" : {r}")).unwrap_or_default();
    match apres {
        Etat::Pause(_) if change => Some(MessageReplay::nouveau(
            GenreMessage::Info,
            format!("Instant replay en pause{}", suite(apres.raison())),
        )),
        Etat::Indisponible(_) if change => Some(MessageReplay::nouveau(
            GenreMessage::Erreur,
            format!("Instant replay indisponible{}", suite(apres.raison())),
        )),
        Etat::Arme if matches!(avant, Etat::Pause(_)) => Some(MessageReplay::nouveau(
            GenreMessage::Info,
            "Instant replay repris",
        )),
        _ => None,
    }
}

pub fn message_clip(clip: &ClipEnregistre) -> MessageReplay {
    MessageReplay::nouveau(
        GenreMessage::Ok,
        format!("Clip enregistré ({})", duree_clip(clip.duree_s)),
    )
}

pub fn message_erreur(erreur: &ErreurReplay) -> MessageReplay {
    let texte = if erreur.message.trim().is_empty() {
        format!("Instant replay : erreur ({})", erreur.code)
    } else {
        erreur.message.clone()
    };
    MessageReplay::nouveau(GenreMessage::Erreur, texte)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileReplay {
    messages: Vec<(MessageReplay, Duration)>,
}

impl FileReplay {
    pub fn pousser(&mut self, message: MessageReplay, maintenant: Duration, duree: Duration) {
        self.messages.push((message, maintenant + duree));
        let surplus = self.messages.len().saturating_sub(MESSAGES_MAX);
        self.messages.drain(..surplus);
    }

    pub fn expirer(&mut self, maintenant: Duration) -> bool {
        let avant = self.messages.len();
        self.messages.retain(|(_, fin)| *fin > maintenant);
        self.messages.len() != avant
    }

    pub fn prochaine_echeance(&self) -> Option<Duration> {
        self.messages.iter().map(|(_, fin)| *fin).min()
    }

    pub fn messages(&self) -> impl Iterator<Item = &MessageReplay> {
        self.messages.iter().map(|(m, _)| m)
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn vider(&mut self) {
        self.messages.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtatIndicateur {
    pub indicateur: bool,
    pub messages: Vec<MessageReplay>,
}

pub fn etat_indicateur(etat: &Etat, actif: bool, file: &FileReplay) -> Option<EtatIndicateur> {
    let indicateur = actif && *etat == Etat::Arme;
    (indicateur || !file.is_empty()).then(|| EtatIndicateur {
        indicateur,
        messages: file.messages().cloned().collect(),
    })
}

pub fn taille_fenetre(etat: &EtatIndicateur) -> TailleLogique {
    let messages = etat.messages.len() as u32;
    let lignes = messages + u32::from(etat.indicateur);
    let hauteur = if etat.indicateur {
        HAUTEUR_INDICATEUR
    } else {
        0
    } + messages * LIGNE
        + lignes.saturating_sub(1) * ECART;
    TailleLogique {
        largeur: f64::from(LARGEUR),
        hauteur: f64::from(hauteur.max(1)),
    }
}

pub fn coin(participants: Option<CoinOverlay>, messages: Option<CoinOverlay>) -> CoinOverlay {
    [
        CoinOverlay::HautDroite,
        CoinOverlay::HautGauche,
        CoinOverlay::BasDroite,
        CoinOverlay::BasGauche,
    ]
    .into_iter()
    .find(|c| Some(*c) != participants && Some(*c) != messages)
    .unwrap_or(CoinOverlay::HautDroite)
}

pub fn a_gauche(coin: CoinOverlay) -> bool {
    matches!(
        coin,
        CoinOverlay::HautGauche | CoinOverlay::CentreGauche | CoinOverlay::BasGauche
    )
}

#[cfg(test)]
mod tests {
    use super::super::automate::Pause;
    use super::*;

    const S: Duration = Duration::from_secs(1);

    fn clip(duree_s: f64) -> ClipEnregistre {
        ClipEnregistre {
            chemin: "/c/a.mp4".into(),
            nom: "a".into(),
            jeu: "Jeu".into(),
            duree_s,
            taille_octets: 1,
        }
    }

    #[test]
    fn formats_lisibles() {
        assert_eq!(duree_clip(29.6), "0:30");
        assert_eq!(duree_clip(125.0), "2:05");
        assert_eq!(duree_clip(f64::NAN), "0:00");
        assert_eq!(duree_tampon(45), "45 s");
        assert_eq!(duree_tampon(120), "2 min");
        assert_eq!(duree_tampon(75), "1 min 15");
        assert_eq!(taille(0), "0 Ko");
        assert_eq!(taille(10), "1 Ko");
        assert_eq!(taille(5 * 1024 * 1024), "5 Mo");
        assert_eq!(taille(GO * 3 / 2), "1,5 Go");
        assert_eq!(taille(20 * GO), "20 Go");
        assert_eq!(taille(2 * GO), "2 Go");
        assert_eq!(nom_codec(Codec::Hevc), "HEVC");
    }

    #[test]
    fn les_valeurs_de_l_interface_restent_dans_les_bornes() {
        assert_eq!(duree_choisie(1000.0, DUREE_MAX_S), DUREE_MAX_S);
        assert_eq!(duree_choisie(-5.0, DUREE_MAX_S), DUREE_MIN_S);
        assert_eq!(duree_choisie(f32::NAN, DUREE_MAX_S), DUREE_MIN_S);
        assert_eq!(duree_choisie(52.0, DUREE_MAX_S), 45);
        assert_eq!(duree_choisie(200.0, 90), 90);
        assert_eq!(duree_choisie(200.0, 0), DUREE_MIN_S);
        assert_eq!(duree_max(None), DUREE_MAX_S);
        assert_eq!(quota_choisi(0.0), QUOTA_MIN);
        assert_eq!(quota_choisi(f32::INFINITY), QUOTA_MIN);
        assert_eq!(quota_choisi(5.4), 5 * GO);
    }

    #[test]
    fn messages_des_changements_d_etat() {
        let pause = Etat::Pause(Pause::PremierPlan);
        assert_eq!(
            message_etat(&Etat::Arme, &pause),
            Some(MessageReplay::nouveau(
                GenreMessage::Info,
                "Instant replay en pause : le jeu n'est plus au premier plan"
            ))
        );
        assert_eq!(message_etat(&pause, &pause), None);
        assert_eq!(
            message_etat(&pause, &Etat::Pause(Pause::Partage))
                .unwrap()
                .texte,
            "Instant replay en pause : le partage d'écran est prioritaire"
        );
        assert_eq!(
            message_etat(&pause, &Etat::Arme).unwrap().texte,
            "Instant replay repris"
        );
        assert_eq!(message_etat(&Etat::Inactif, &Etat::Arme), None);
        let refus = Etat::Indisponible("encodeur muet".into());
        assert_eq!(
            message_etat(&Etat::Arme, &refus),
            Some(MessageReplay::nouveau(
                GenreMessage::Erreur,
                "Instant replay indisponible : encodeur muet"
            ))
        );
        assert_eq!(message_etat(&refus, &refus), None);
        assert_eq!(message_clip(&clip(30.2)).texte, "Clip enregistré (0:30)");
        assert_eq!(
            message_erreur(&ErreurReplay::nouvelle("quota", "")).texte,
            "Instant replay : erreur (quota)"
        );
        assert_eq!(
            message_erreur(&ErreurReplay::nouvelle("x", "Rien à enregistrer")).genre,
            GenreMessage::Erreur
        );
    }

    #[test]
    fn la_file_garde_trois_messages_et_les_expire() {
        let mut f = FileReplay::default();
        for i in 0..4 {
            f.pousser(
                MessageReplay::nouveau(GenreMessage::Ok, format!("m{i}")),
                S * i,
                S * 8,
            );
        }
        assert_eq!(f.len(), MESSAGES_MAX);
        assert_eq!(f.messages().next().unwrap().texte, "m1");
        assert_eq!(f.prochaine_echeance(), Some(S * 9));
        assert!(!f.expirer(S * 9 - Duration::from_millis(1)));
        assert!(f.expirer(S * 9));
        assert_eq!(f.len(), 2);
        f.vider();
        assert!(f.is_empty() && f.prochaine_echeance().is_none());
    }

    #[test]
    fn indicateur_seulement_tampon_arme_et_taille_selon_les_lignes() {
        let mut f = FileReplay::default();
        assert_eq!(etat_indicateur(&Etat::Arme, false, &f), None);
        assert_eq!(etat_indicateur(&Etat::Inactif, true, &f), None);
        let seul = etat_indicateur(&Etat::Arme, true, &f).unwrap();
        assert!(seul.indicateur && seul.messages.is_empty());
        assert_eq!(taille_fenetre(&seul).hauteur, 26.0);
        f.pousser(message_clip(&clip(30.0)), Duration::ZERO, S);
        f.pousser(
            message_erreur(&ErreurReplay::nouvelle("x", "y")),
            Duration::ZERO,
            S,
        );
        let deux = etat_indicateur(&Etat::Arme, true, &f).unwrap();
        assert_eq!(taille_fenetre(&deux).hauteur, (26 + 2 * 44 + 2 * 6) as f64);
        let sans = etat_indicateur(&Etat::Inactif, true, &f).unwrap();
        assert!(!sans.indicateur);
        assert_eq!(taille_fenetre(&sans).hauteur, (2 * 44 + 6) as f64);
        assert_eq!(taille_fenetre(&sans).largeur, 320.0);
    }

    #[test]
    fn le_coin_evite_les_deux_autres_overlays() {
        assert_eq!(coin(None, None), CoinOverlay::HautDroite);
        assert_eq!(
            coin(
                Some(CoinOverlay::CentreGauche),
                Some(CoinOverlay::HautDroite)
            ),
            CoinOverlay::HautGauche
        );
        assert_eq!(
            coin(Some(CoinOverlay::HautGauche), Some(CoinOverlay::HautDroite)),
            CoinOverlay::BasDroite
        );
        assert!(a_gauche(CoinOverlay::BasGauche) && !a_gauche(CoinOverlay::HautDroite));
    }
}
