use chrono::{DateTime, Utc};
use serde::Serialize;

pub const NEUTRE: f32 = 0.5;
pub const DEMI_VIE_AFFINITE_JOURS: f32 = 90.0;
pub const DEMI_VIE_SAUT_JOURS: f32 = 7.0;
pub const DEMI_VIE_PISTE_JOURS: f32 = 3.0;
pub const DEMI_VIE_ARTISTE_JOURS: f32 = 2.0 / 24.0;
pub const SAUTS_PLEINS: f32 = 2.0;
pub const ECART_ANNEES_MAX: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Poids {
    pub audio: f32,
    #[serde(rename = "metadata")]
    pub metadonnees: f32,
    #[serde(rename = "similarity")]
    pub similarite: f32,
    #[serde(rename = "affinity")]
    pub affinite: f32,
    #[serde(rename = "discovery")]
    pub decouverte: f32,
    #[serde(rename = "skips")]
    pub sauts: f32,
    #[serde(rename = "recent_track")]
    pub piste_recente: f32,
    #[serde(rename = "recent_artist")]
    pub artiste_recent: f32,
}

impl Default for Poids {
    fn default() -> Self {
        Self {
            audio: 1.0,
            metadonnees: 0.6,
            similarite: 0.5,
            affinite: 1.0,
            decouverte: 2.0,
            sauts: 1.5,
            piste_recente: 1.0,
            artiste_recent: 0.4,
        }
    }
}

impl Poids {
    pub fn lire(texte: &str) -> Result<Self, String> {
        let mut p = Self::default();
        for paire in texte.split(',').map(str::trim).filter(|x| !x.is_empty()) {
            let (nom, valeur) = paire
                .split_once('=')
                .ok_or_else(|| format!("SILLON_RECO_POIDS : « {paire} » sans « = »"))?;
            let v: f32 = valeur
                .trim()
                .parse()
                .ok()
                .filter(|v: &f32| v.is_finite() && (0.0..=100.0).contains(v))
                .ok_or_else(|| format!("SILLON_RECO_POIDS : valeur invalide dans « {paire} »"))?;
            let champ = match nom.trim() {
                "audio" => &mut p.audio,
                "metadonnees" => &mut p.metadonnees,
                "similarite" => &mut p.similarite,
                "affinite" => &mut p.affinite,
                "decouverte" => &mut p.decouverte,
                "sauts" => &mut p.sauts,
                "piste_recente" => &mut p.piste_recente,
                "artiste_recent" => &mut p.artiste_recent,
                autre => return Err(format!("SILLON_RECO_POIDS : poids inconnu « {autre} »")),
            };
            *champ = v;
        }
        Ok(p)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Habitude {
    pub completes: u32,
    pub sauts: u32,
    pub derniere: Option<DateTime<Utc>>,
    pub sauts_recents: Vec<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Candidat<'a> {
    pub distance_contexte: Option<f32>,
    pub distance_precedente: Option<f32>,
    pub genres_communs: Option<f32>,
    pub ecart_annees: Option<f32>,
    pub meme_artiste: bool,
    pub similarite: Option<f32>,
    pub piste: Option<&'a Habitude>,
    pub artiste: Option<&'a Habitude>,
    pub aimee: bool,
    pub artiste_aime: bool,
    pub en_playlist: bool,
    pub distance_aimee: Option<f32>,
    pub decouverte: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Detail {
    pub audio: f32,
    #[serde(rename = "metadata")]
    pub metadonnees: f32,
    #[serde(rename = "similarity")]
    pub similarite: f32,
    #[serde(rename = "affinity")]
    pub affinite: f32,
    #[serde(rename = "discovery")]
    pub decouverte: f32,
    #[serde(rename = "skips")]
    pub sauts: f32,
    #[serde(rename = "recent_track")]
    pub piste_recente: f32,
    #[serde(rename = "recent_artist")]
    pub artiste_recent: f32,
    pub total: f32,
}

pub fn proximite(distance: f32) -> f32 {
    1.0 / (1.0 + distance.max(0.0))
}

fn recul(depuis: Option<DateTime<Utc>>, maintenant: DateTime<Utc>, demi_vie_jours: f32) -> f32 {
    depuis.map_or(0.0, |d| {
        let jours = (maintenant - d).num_seconds().max(0) as f32 / 86_400.0;
        0.5f32.powf(jours / demi_vie_jours)
    })
}

fn gout(h: Option<&Habitude>, maintenant: DateTime<Utc>) -> f32 {
    h.map_or(NEUTRE, |h| {
        let taux = (h.completes as f32 + 1.0) / ((h.completes + h.sauts) as f32 + 2.0);
        NEUTRE + (taux - NEUTRE) * recul(h.derniere, maintenant, DEMI_VIE_AFFINITE_JOURS)
    })
}

pub fn sauts(h: Option<&Habitude>, maintenant: DateTime<Utc>) -> f32 {
    h.map_or(0.0, |h| {
        let somme: f32 = h
            .sauts_recents
            .iter()
            .map(|s| recul(Some(*s), maintenant, DEMI_VIE_SAUT_JOURS))
            .sum();
        (somme / SAUTS_PLEINS).min(1.0)
    })
}

pub fn noter(c: &Candidat, p: &Poids, maintenant: DateTime<Utc>) -> Detail {
    let distances: Vec<f32> = [c.distance_contexte, c.distance_precedente]
        .into_iter()
        .flatten()
        .map(proximite)
        .collect();
    let audio = if distances.is_empty() {
        NEUTRE
    } else {
        distances.iter().sum::<f32>() / distances.len() as f32
    };
    let epoque = c
        .ecart_annees
        .map_or(NEUTRE, |e| (1.0 - e.abs() / ECART_ANNEES_MAX).max(0.0));
    let lien = if c.meme_artiste {
        1.0
    } else if c.similarite.is_some_and(|s| s > 0.0) {
        0.5
    } else {
        0.0
    };
    let metadonnees = 0.5 * c.genres_communs.unwrap_or(NEUTRE) + 0.25 * epoque + 0.25 * lien;
    let similarite = if c.meme_artiste {
        1.0
    } else {
        c.similarite.unwrap_or(NEUTRE).clamp(0.0, 1.0)
    };
    let voisine = c.distance_aimee.map_or(0.0, |d| (1.0 - d).max(0.0));
    let affinite = (0.6 * gout(c.piste, maintenant)
        + 0.4 * gout(c.artiste, maintenant)
        + 0.25 * f32::from(u8::from(c.aimee))
        + 0.15 * f32::from(u8::from(c.artiste_aime))
        + 0.1 * f32::from(u8::from(c.en_playlist))
        + 0.2 * voisine)
        .clamp(0.0, 1.0);
    let decouverte = f32::from(u8::from(c.decouverte));
    let sauts = sauts(c.piste, maintenant);
    let piste_recente = recul(
        c.piste.and_then(|h| h.derniere),
        maintenant,
        DEMI_VIE_PISTE_JOURS,
    );
    let artiste_recent = recul(
        c.artiste.and_then(|h| h.derniere),
        maintenant,
        DEMI_VIE_ARTISTE_JOURS,
    );
    let total = p.audio * audio
        + p.metadonnees * metadonnees
        + p.similarite * similarite
        + p.affinite * affinite
        + p.decouverte * decouverte
        - p.sauts * sauts
        - p.piste_recente * piste_recente
        - p.artiste_recent * artiste_recent;
    Detail {
        audio,
        metadonnees,
        similarite,
        affinite,
        decouverte,
        sauts,
        piste_recente,
        artiste_recent,
        total,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn total(c: &Candidat) -> f32 {
        noter(c, &Poids::default(), t0()).total
    }

    #[test]
    fn signaux_absents_neutres_et_poids_appliques() {
        let d = noter(&Candidat::default(), &Poids::default(), t0());
        assert_eq!(
            (d.audio, d.similarite, d.affinite),
            (NEUTRE, NEUTRE, NEUTRE)
        );
        assert_eq!(d.metadonnees, 0.5 * NEUTRE + 0.25 * NEUTRE);
        assert_eq!(
            (d.sauts, d.piste_recente, d.artiste_recent, d.decouverte),
            (0.0, 0.0, 0.0, 0.0)
        );
        let p = Poids::default();
        let attendu = p.audio * d.audio
            + p.metadonnees * d.metadonnees
            + p.similarite * d.similarite
            + p.affinite * d.affinite;
        assert!((d.total - attendu).abs() < 1e-6);
        let nul = Poids::lire("audio=0,metadonnees=0,similarite=0,affinite=0").unwrap();
        assert_eq!(noter(&Candidat::default(), &nul, t0()).total, 0.0);
        let analysee = Candidat {
            distance_contexte: Some(1.0),
            distance_precedente: Some(1.0),
            ..Candidat::default()
        };
        assert_eq!(
            total(&analysee),
            total(&Candidat::default()),
            "distance 1 = neutre"
        );
    }

    #[test]
    fn proximite_metadonnees_et_similarite_montent_le_score() {
        let base = total(&Candidat::default());
        let proche = Candidat {
            distance_contexte: Some(0.1),
            distance_precedente: Some(0.2),
            ..Candidat::default()
        };
        let loin = Candidat {
            distance_contexte: Some(3.0),
            ..Candidat::default()
        };
        assert!(total(&proche) > base && base > total(&loin));
        let genres = Candidat {
            genres_communs: Some(1.0),
            ecart_annees: Some(0.0),
            ..Candidat::default()
        };
        let sans = Candidat {
            genres_communs: Some(0.0),
            ecart_annees: Some(40.0),
            ..Candidat::default()
        };
        assert!(total(&genres) > base && base > total(&sans));
        let lie = Candidat {
            similarite: Some(0.9),
            ..Candidat::default()
        };
        let etranger = Candidat {
            similarite: Some(0.0),
            ..Candidat::default()
        };
        let meme = Candidat {
            meme_artiste: true,
            ..Candidat::default()
        };
        assert!(total(&meme) > total(&lie) && total(&lie) > base && base > total(&etranger));
    }

    #[test]
    fn affinite_ecoutes_likes_playlist_et_decroissance() {
        let base = total(&Candidat::default());
        let hier = t0() - Duration::days(1);
        let ecoutee = Habitude {
            completes: 5,
            derniere: Some(t0() - Duration::days(30)),
            ..Habitude::default()
        };
        let ancienne = Habitude {
            derniere: Some(t0() - Duration::days(720)),
            ..ecoutee.clone()
        };
        let avec = |h: &Habitude| {
            noter(
                &Candidat {
                    piste: Some(h),
                    ..Candidat::default()
                },
                &Poids::default(),
                t0(),
            )
        };
        assert!(avec(&ecoutee).affinite > avec(&ancienne).affinite);
        assert!(avec(&ancienne).affinite > NEUTRE);
        assert!((avec(&ancienne).affinite - NEUTRE) < 0.01);
        let sautee = Habitude {
            sauts: 4,
            derniere: Some(hier),
            sauts_recents: vec![hier, hier, hier],
            ..Habitude::default()
        };
        let d = avec(&sautee);
        assert!(d.affinite < NEUTRE);
        assert_eq!(d.sauts, 1.0);
        assert!(d.piste_recente > 0.7 && d.total < base - 1.5);
        let un_saut = Habitude {
            sauts: 1,
            derniere: Some(hier),
            sauts_recents: vec![t0() - Duration::days(7)],
            ..Habitude::default()
        };
        assert!((avec(&un_saut).sauts - 0.25).abs() < 1e-3);
        for c in [
            Candidat {
                aimee: true,
                ..Candidat::default()
            },
            Candidat {
                artiste_aime: true,
                ..Candidat::default()
            },
            Candidat {
                en_playlist: true,
                ..Candidat::default()
            },
        ] {
            assert!(total(&c) > base);
        }
        let artiste = Habitude {
            completes: 3,
            derniere: Some(t0() - Duration::minutes(10)),
            ..Habitude::default()
        };
        let d = noter(
            &Candidat {
                artiste: Some(&artiste),
                ..Candidat::default()
            },
            &Poids::default(),
            t0(),
        );
        assert!(d.artiste_recent > 0.9 && d.affinite > NEUTRE);
    }

    #[test]
    fn voisine_d_une_piste_likee_et_decouverte_prioritaire() {
        let base = total(&Candidat::default());
        let voisine = Candidat {
            distance_aimee: Some(0.2),
            ..Candidat::default()
        };
        let eloignee = Candidat {
            distance_aimee: Some(1.5),
            ..Candidat::default()
        };
        assert!(total(&voisine) > base);
        assert_eq!(total(&eloignee), base);
        let decouverte = Candidat {
            decouverte: true,
            ..Candidat::default()
        };
        let parfaite = Candidat {
            distance_contexte: Some(0.0),
            distance_precedente: Some(0.0),
            genres_communs: Some(1.0),
            ecart_annees: Some(0.0),
            similarite: Some(1.0),
            ..Candidat::default()
        };
        assert!(total(&decouverte) > total(&parfaite));
    }

    #[test]
    fn poids_lus_et_refuses() {
        assert_eq!(Poids::lire("").unwrap(), Poids::default());
        let p = Poids::lire(" audio = 2 , sauts=0.5 ").unwrap();
        assert_eq!((p.audio, p.sauts, p.affinite), (2.0, 0.5, 1.0));
        for faux in ["audio", "audio=x", "audio=-1", "inconnu=1", "audio=nan"] {
            assert!(Poids::lire(faux).is_err(), "{faux}");
        }
    }
}
