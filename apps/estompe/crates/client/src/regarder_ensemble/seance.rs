use std::collections::HashMap;

use crate::roles::date_en_millisecondes;
use crate::temps_reel::evenements::{EtatSeance, Seance};
use crate::types::ChannelKind;

pub const SAUT_AU_DELA: f64 = 1.0;
pub const IGNORE_EN_DECA: f64 = 0.15;
pub const CRAN: f64 = 0.25;
pub const VITESSE_CRAN_MIN: f64 = 0.25;
pub const VITESSE_CRAN_MAX: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenreSeance {
    Regarder,
    Musique,
    Cinema,
}

impl GenreSeance {
    pub const TOUS: [GenreSeance; 3] = [Self::Regarder, Self::Musique, Self::Cinema];

    pub fn chemin(self) -> &'static str {
        match self {
            Self::Regarder => "watch",
            Self::Musique => "music",
            Self::Cinema => "cinema",
        }
    }

    pub fn genre_salon(self) -> ChannelKind {
        match self {
            Self::Cinema => ChannelKind::Cinema,
            _ => ChannelKind::Voice,
        }
    }

    pub fn etiquette(self) -> &'static str {
        match self {
            Self::Regarder => "regarder",
            Self::Musique => "musique",
            Self::Cinema => "cinema",
        }
    }
}

pub fn salon_auto<'a, S>(
    salons: &'a [S],
    connecte: Option<&str>,
    id: impl Fn(&S) -> &str,
) -> Option<&'a S> {
    salons
        .iter()
        .find(|s| Some(id(s)) == connecte)
        .or_else(|| (salons.len() == 1 && connecte.is_none()).then(|| &salons[0]))
}

pub fn decalage_horloge(maintenant_serveur: &str, maintenant_client_ms: i64) -> i64 {
    date_en_millisecondes(maintenant_serveur).map_or(0, |s| s - maintenant_client_ms)
}

pub fn position_attendue(etat: &EtatSeance, decalage_ms: i64, maintenant_ms: i64) -> f64 {
    if !etat.playing {
        return etat.pos;
    }
    let depuis = date_en_millisecondes(&etat.at).unwrap_or(maintenant_ms + decalage_ms);
    let ecoule = ((maintenant_ms + decalage_ms - depuis) as f64 / 1000.0).max(0.0);
    (etat.pos + ecoule * etat.rate).max(0.0)
}

pub fn perime(courante: Option<i64>, suivante: i64) -> bool {
    courante.is_some_and(|c| suivante < c)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Correction {
    Saut(f64),
    Vitesse { vitesse: f64, ms: i64 },
}

pub fn correction(ecart: f64, attendue: f64, vitesse: f64) -> Option<Correction> {
    let d = ecart.abs();
    if d >= SAUT_AU_DELA {
        return Some(Correction::Saut(attendue));
    }
    if d < IGNORE_EN_DECA {
        return None;
    }
    let cible = if ecart < 0.0 {
        vitesse + CRAN
    } else {
        vitesse - CRAN
    };
    if !(VITESSE_CRAN_MIN..=VITESSE_CRAN_MAX).contains(&cible) {
        return Some(Correction::Saut(attendue));
    }
    Some(Correction::Vitesse {
        vitesse: cible,
        ms: (d / CRAN * 1000.0).round() as i64,
    })
}

pub fn message_erreur_youtube(code: i64) -> &'static str {
    match code {
        2 => "Lien de vidéo invalide.",
        5 => "Cette vidéo ne peut pas être lue par le lecteur intégré.",
        100 => "Vidéo introuvable : supprimée ou privée.",
        101 | 150 => {
            "Le propriétaire de la vidéo n'autorise pas sa lecture dans une autre application."
        }
        153 => "YouTube refuse la lecture intégrée ici (origine ou référent absent).",
        _ => "Lecture impossible.",
    }
}

pub fn derniere_action(act: &str, par: &str) -> String {
    let qui = if par.is_empty() { "Quelqu’un" } else { par };
    match act {
        "play" => format!("{qui} a relancé la lecture"),
        "pause" => format!("{qui} a mis en pause"),
        "seek" => format!("{qui} a changé la position"),
        "rate" => format!("{qui} a changé la vitesse"),
        "video" => format!("{qui} a changé de vidéo"),
        "wait" => format!("Lecture en pause : {qui} charge la vidéo"),
        _ => String::new(),
    }
}

pub fn texte_attente(attendus: &[String], moi: &str, personnes: &[(&str, &str)]) -> String {
    if attendus.is_empty() {
        return String::new();
    }
    let autres = attendus.iter().filter(|id| *id != moi).map(|id| {
        personnes
            .iter()
            .find(|(u, _)| u == id)
            .map_or("quelqu’un", |(_, p)| *p)
    });
    let mut noms: Vec<&str> = Vec::new();
    if attendus.iter().any(|id| id == moi) {
        noms.push("votre appareil");
    }
    noms.extend(autres);
    let liste = match noms.split_last() {
        Some((dernier, debut)) if !debut.is_empty() => format!("{} et {dernier}", debut.join(", ")),
        _ => noms.first().copied().unwrap_or_default().to_string(),
    };
    let verbe = if noms.len() > 1 { "chargent" } else { "charge" };
    format!("Lecture en pause : {liste} {verbe} la vidéo. Elle reprendra toute seule.")
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeanceEnCours {
    pub etat: EtatSeance,
    pub decalage: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegistreSeances {
    seances: HashMap<(GenreSeance, String), SeanceEnCours>,
}

impl RegistreSeances {
    pub fn appliquer(
        &mut self,
        genre: GenreSeance,
        vue: &Seance,
        frais: bool,
        maintenant_ms: i64,
    ) -> bool {
        let cle = (genre, vue.channel_id.clone());
        match &vue.state {
            Some(etat) => {
                if !frais && perime(self.seances.get(&cle).map(|s| s.etat.rev), etat.rev) {
                    return false;
                }
                let decalage = vue
                    .now
                    .as_deref()
                    .map_or(0, |n| decalage_horloge(n, maintenant_ms));
                self.seances.insert(
                    cle,
                    SeanceEnCours {
                        etat: etat.clone(),
                        decalage,
                    },
                );
                true
            }
            None => self.seances.remove(&cle).is_some(),
        }
    }

    pub fn seance(&self, genre: GenreSeance, salon: &str) -> Option<&SeanceEnCours> {
        self.seances.get(&(genre, salon.to_string()))
    }

    pub fn retirer_salon(&mut self, salon: &str) {
        self.seances.retain(|(_, s), _| s != salon);
    }

    pub fn salons(&self, genre: GenreSeance) -> impl Iterator<Item = &str> {
        self.seances
            .keys()
            .filter(move |(g, _)| *g == genre)
            .map(|(_, s)| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: &str = "2026-10-28T10:00:00.000Z";

    fn base() -> i64 {
        date_en_millisecondes(AT).unwrap()
    }

    fn st() -> EtatSeance {
        EtatSeance {
            playing: true,
            pos: 30.0,
            rate: 1.0,
            at: AT.into(),
            ..EtatSeance::default()
        }
    }

    #[test]
    fn la_position_avance_depuis_l_horodatage_du_serveur() {
        assert_eq!(position_attendue(&st(), 0, base() + 5000), 35.0);
        let rapide = EtatSeance { rate: 1.5, ..st() };
        assert_eq!(position_attendue(&rapide, 0, base() + 4000), 36.0);
    }

    #[test]
    fn la_position_reste_fixe_en_pause() {
        let pause = EtatSeance {
            playing: false,
            ..st()
        };
        assert_eq!(position_attendue(&pause, 0, base() + 60000), 30.0);
    }

    #[test]
    fn le_decalage_d_horloge_est_corrige() {
        let decalage = decalage_horloge(AT, base() + 2000);
        assert_eq!(decalage, -2000);
        assert_eq!(position_attendue(&st(), decalage, base() + 5000), 33.0);
    }

    #[test]
    fn la_position_ne_remonte_jamais_avant_le_debut() {
        let debut = EtatSeance { pos: 0.0, ..st() };
        assert_eq!(position_attendue(&debut, 0, base() - 3000), 0.0);
    }

    #[test]
    fn un_participant_en_cours_de_route_demarre_a_la_bonne_position() {
        let film = EtatSeance { pos: 600.0, ..st() };
        assert_eq!(position_attendue(&film, 0, base() + 90000), 690.0);
    }

    #[test]
    fn les_films_se_lancent_dans_un_salon_cinema_le_reste_dans_un_vocal() {
        assert_eq!(GenreSeance::Cinema.genre_salon(), ChannelKind::Cinema);
        assert_eq!(GenreSeance::Regarder.genre_salon(), ChannelKind::Voice);
        assert_eq!(GenreSeance::Musique.genre_salon(), ChannelKind::Voice);
        assert_eq!(GenreSeance::Musique.chemin(), "music");
    }

    #[test]
    fn un_salon_n_est_choisi_seul_que_si_l_on_y_est_ou_s_il_est_le_seul() {
        #[derive(Debug, PartialEq)]
        struct Salon(&'static str);
        fn id(s: &Salon) -> &str {
            s.0
        }
        let un = [Salon("c1")];
        assert_eq!(salon_auto(&un, Some("c1"), id), Some(&Salon("c1")));
        assert_eq!(salon_auto(&un, None, id), Some(&Salon("c1")));
        assert_eq!(salon_auto(&un, Some("v1"), id), None);
        assert_eq!(salon_auto(&[Salon("c1"), Salon("c2")], None, id), None);
    }

    #[test]
    fn un_petit_ecart_est_tolere() {
        assert_eq!(correction(0.1, 50.0, 1.0), None);
        assert_eq!(correction(-0.14, 50.0, 1.0), None);
    }

    #[test]
    fn un_petit_retard_est_rattrape_d_un_cran_de_vitesse() {
        assert_eq!(
            correction(-0.5, 50.0, 1.0),
            Some(Correction::Vitesse {
                vitesse: 1.25,
                ms: 2000
            })
        );
    }

    #[test]
    fn un_lecteur_en_avance_ralentit() {
        assert_eq!(
            correction(0.25, 50.0, 1.0),
            Some(Correction::Vitesse {
                vitesse: 0.75,
                ms: 1000
            })
        );
    }

    #[test]
    fn au_dela_d_une_seconde_le_lecteur_saute() {
        assert_eq!(correction(-1.0, 50.0, 1.0), Some(Correction::Saut(50.0)));
        assert_eq!(correction(7.0, 50.0, 1.0), Some(Correction::Saut(50.0)));
    }

    #[test]
    fn hors_des_vitesses_permises_le_lecteur_saute() {
        assert_eq!(correction(-0.5, 50.0, 2.0), Some(Correction::Saut(50.0)));
        assert_eq!(correction(0.5, 50.0, 0.25), Some(Correction::Saut(50.0)));
    }

    #[test]
    fn la_derniere_action_est_nommee() {
        assert_eq!(derniere_action("pause", "Alice"), "Alice a mis en pause");
        assert_eq!(derniere_action("ended", "Alice"), "");
        assert_eq!(
            derniere_action("seek", ""),
            "Quelqu’un a changé la position"
        );
    }

    #[test]
    fn une_video_non_integrable_est_expliquee() {
        assert!(message_erreur_youtube(150).contains("n'autorise pas"));
        assert!(message_erreur_youtube(100).contains("introuvable"));
        assert_eq!(message_erreur_youtube(999), "Lecture impossible.");
    }

    #[test]
    fn un_etat_plus_ancien_est_rejete_le_meme_ou_un_plus_recent_accepte() {
        assert!(perime(Some(5), 4));
        assert!(!perime(Some(5), 5));
        assert!(!perime(Some(5), 6));
        assert!(!perime(None, 1));
    }

    #[test]
    fn les_personnes_attendues_sont_nommees() {
        let personnes = [("a", "Alice"), ("b", "Bob")];
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(texte_attente(&[], "a", &personnes), "");
        assert_eq!(
            texte_attente(&ids(&["b"]), "a", &personnes),
            "Lecture en pause : Bob charge la vidéo. Elle reprendra toute seule."
        );
        assert_eq!(
            texte_attente(&ids(&["a", "b"]), "a", &personnes),
            "Lecture en pause : votre appareil et Bob chargent la vidéo. Elle reprendra toute seule."
        );
        assert_eq!(
            texte_attente(&ids(&["z"]), "a", &personnes),
            "Lecture en pause : quelqu’un charge la vidéo. Elle reprendra toute seule."
        );
        assert_eq!(
            derniere_action("wait", "Bob"),
            "Lecture en pause : Bob charge la vidéo"
        );
        assert_eq!(derniere_action("resume", "Bob"), "");
    }

    #[test]
    fn le_registre_ignore_un_etat_croise_sauf_au_rafraichissement() {
        let mut registre = RegistreSeances::default();
        let vue = |rev: i64| Seance {
            channel_id: "v1".into(),
            state: Some(EtatSeance { rev, ..st() }),
            now: Some("2026-10-28T10:00:01.000Z".into()),
            ..Seance::default()
        };
        assert!(registre.appliquer(GenreSeance::Regarder, &vue(5), false, base()));
        assert_eq!(
            registre
                .seance(GenreSeance::Regarder, "v1")
                .unwrap()
                .decalage,
            1000
        );
        assert!(!registre.appliquer(GenreSeance::Regarder, &vue(4), false, base()));
        assert!(registre.appliquer(GenreSeance::Regarder, &vue(4), true, base()));
        assert_eq!(
            registre
                .seance(GenreSeance::Regarder, "v1")
                .unwrap()
                .etat
                .rev,
            4
        );
        assert!(registre.seance(GenreSeance::Cinema, "v1").is_none());
        assert_eq!(
            registre.salons(GenreSeance::Regarder).collect::<Vec<_>>(),
            ["v1"]
        );
        let fin = Seance {
            channel_id: "v1".into(),
            ..Seance::default()
        };
        assert!(registre.appliquer(GenreSeance::Regarder, &fin, false, base()));
        assert!(registre.seance(GenreSeance::Regarder, "v1").is_none());
        registre.appliquer(GenreSeance::Musique, &vue(1), false, base());
        registre.retirer_salon("v1");
        assert!(registre.seance(GenreSeance::Musique, "v1").is_none());
    }
}
