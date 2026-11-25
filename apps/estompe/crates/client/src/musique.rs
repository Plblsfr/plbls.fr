use reqwest::Method;
use serde::Deserialize;

use crate::http::{encoder_composant, Client, Corps, ErreurApi};
use crate::regarder_ensemble::salon::{EtatPret, ParticipantSeance, SUSPENDUE};
use crate::regarder_ensemble::seance::{
    derniere_action, position_attendue, GenreSeance, SeanceEnCours,
};
use crate::store::{id_de, Etat, GenreSalon};
use crate::temps_reel::evenements::{ElementFile, MetaMorceau};
use crate::videos::{horloge, textes_temps};

pub const VOLUME_PAR_DEFAUT: u8 = 80;
pub const RECHERCHE_MIN: usize = 2;
pub const RECHERCHE_INDISPONIBLE: &str = "Recherche indisponible";
pub const ECOUTE_ENSEMBLE: &str = "Écoute ensemble";

pub fn horloge_morceau(secondes: f64) -> String {
    if secondes > 0.0 {
        horloge(secondes)
    } else {
        String::new()
    }
}

pub fn chemin_pochette(cover: &str) -> Option<&str> {
    cover.starts_with('/').then_some(cover)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cadrage {
    pub x: u32,
    pub y: u32,
    pub cote: u32,
}

pub fn cadrage_pochette(largeur: u32, hauteur: u32) -> Cadrage {
    let cote = (f64::from(largeur).min(f64::from(hauteur) * 0.75)).floor() as u32;
    Cadrage {
        x: (largeur - cote) / 2,
        y: (hauteur - cote) / 2,
        cote,
    }
}

pub fn couleur_moyenne(pixels: &[u8]) -> Option<[u8; 3]> {
    let (mut somme, mut n) = ([0u64; 3], 0u64);
    for p in pixels.chunks_exact(4).filter(|p| p[3] >= 128) {
        for (s, v) in somme.iter_mut().zip(p) {
            *s += u64::from(*v);
        }
        n += 1;
    }
    (n > 0).then(|| somme.map(|s| (s as f64 / n as f64).round() as u8))
}

pub fn est_lien(texte: &str) -> bool {
    let t = texte.trim().to_lowercase();
    ["http://", "https://", "music.youtube.com"]
        .iter()
        .any(|p| t.starts_with(p))
}

pub fn lien_du_morceau(id: &str) -> String {
    format!("https://music.youtube.com/watch?v={id}")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Repetition {
    #[default]
    Aucune,
    Morceau,
    File,
}

impl Repetition {
    pub fn lire(valeur: &str) -> Self {
        match valeur {
            "one" => Self::Morceau,
            "all" => Self::File,
            _ => Self::Aucune,
        }
    }

    pub fn cle(self) -> &'static str {
        match self {
            Self::Aucune => "",
            Self::Morceau => "one",
            Self::File => "all",
        }
    }

    pub fn suivante(self) -> Self {
        match self {
            Self::Aucune => Self::Morceau,
            Self::Morceau => Self::File,
            Self::File => Self::Aucune,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Aucune => "Répétition désactivée",
            Self::Morceau => "Répéter le morceau",
            Self::File => "Répéter la file",
        }
    }
}

pub fn texte_ecoute(meta: &MetaMorceau) -> String {
    [meta.title.as_str(), meta.artist.as_str()]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" – ")
}

pub fn ecoute_de(etat: &Etat, utilisateur: &str) -> String {
    etat.vocal
        .iter()
        .filter(|(_, ps)| ps.iter().any(|p| p.utilisateur == utilisateur))
        .find_map(|(salon, _)| {
            etat.seances
                .seance(GenreSeance::Musique, salon)
                .and_then(|s| s.etat.meta.as_ref())
        })
        .map(texte_ecoute)
        .unwrap_or_default()
}

pub fn peut_piloter(s: &SeanceEnCours, moi: &str) -> bool {
    !s.etat.host_only || s.etat.host == moi
}

pub fn position_musique(s: &SeanceEnCours, duree_lecteur: f64, maintenant_ms: i64) -> (f64, f64) {
    let duree = match s.etat.meta.as_ref().map_or(0, |m| m.duration) {
        d if d > 0 => d as f64,
        _ => duree_lecteur.max(0.0).round(),
    };
    let position = position_attendue(&s.etat, s.decalage, maintenant_ms);
    (
        if duree > 0.0 {
            position.min(duree)
        } else {
            position
        },
        duree,
    )
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Voisin {
    pub titre: String,
    pub infobulle: String,
    pub pochette: String,
}

fn voisin(e: &ElementFile, defaut: &str) -> Voisin {
    let meta = e.meta.as_ref();
    let titre = meta.map(|m| m.title.clone()).unwrap_or_default();
    let artiste = meta.map(|m| m.artist.as_str()).unwrap_or_default();
    Voisin {
        infobulle: if artiste.is_empty() {
            if titre.is_empty() {
                defaut.to_string()
            } else {
                titre.clone()
            }
        } else {
            format!(
                "{} · {artiste}",
                if titre.is_empty() { defaut } else { &titre }
            )
        },
        pochette: meta
            .and_then(|m| chemin_pochette(&m.cover))
            .unwrap_or_default()
            .to_string(),
        titre,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementMusique {
    pub id: String,
    pub titre: String,
    pub details: String,
    pub duree: String,
    pub pochette: String,
    pub monter: bool,
    pub descendre: bool,
    pub retirer: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VueMusique {
    pub titre: String,
    pub details: String,
    pub pochette: String,
    pub peut_piloter: bool,
    pub hote: bool,
    pub hote_seul: bool,
    pub lecture: bool,
    pub position: f64,
    pub duree: f64,
    pub texte_position: String,
    pub texte_duree: String,
    pub repetition: Repetition,
    pub melanger: bool,
    pub suivant: bool,
    pub precedent: Option<Voisin>,
    pub prochain: Option<Voisin>,
    pub statut: String,
    pub auditeurs: Vec<ParticipantSeance>,
    pub file: Vec<ElementMusique>,
}

pub struct ContexteMusique<'a> {
    pub moi: &'a str,
    pub personnes: &'a [(String, String)],
    pub duree_lecteur: f64,
    pub suspendu: bool,
    pub maintenant_ms: i64,
}

pub fn vue_musique(s: &SeanceEnCours, c: &ContexteMusique) -> Option<VueMusique> {
    let e = &s.etat;
    let meta = e.meta.as_ref()?;
    let peut = peut_piloter(s, c.moi);
    let (position, duree) = position_musique(s, c.duree_lecteur, c.maintenant_ms);
    let repetition = Repetition::lire(&e.repeat);
    let n = e.queue.len();
    let file = e
        .queue
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let m = q.meta.as_ref();
            ElementMusique {
                id: q.id.clone(),
                titre: m
                    .map(|m| m.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| q.video.clone()),
                details: match m {
                    Some(m) => format!("{} · {}", m.artist, q.by),
                    None => q.by.clone(),
                },
                duree: m.map_or_else(String::new, |m| horloge_morceau(m.duration as f64)),
                pochette: m
                    .and_then(|m| chemin_pochette(&m.cover))
                    .unwrap_or_default()
                    .to_string(),
                monter: peut && i > 0,
                descendre: peut && i + 1 < n,
                retirer: peut || q.by_id == c.moi,
            }
        })
        .collect();
    Some(VueMusique {
        titre: meta.title.clone(),
        details: [meta.artist.as_str(), meta.album.as_str()]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        pochette: chemin_pochette(&meta.cover).unwrap_or_default().to_string(),
        peut_piloter: peut,
        hote: e.host == c.moi,
        hote_seul: e.host_only,
        lecture: e.playing,
        position,
        duree,
        texte_position: horloge_morceau(position),
        texte_duree: horloge_morceau(duree),
        repetition,
        melanger: peut && n >= 2,
        suivant: peut && (n > 0 || repetition == Repetition::File),
        precedent: e.history.last().map(|h| voisin(h, "Précédent")),
        prochain: e.queue.first().map(|q| voisin(q, "Suivant")),
        statut: if c.suspendu {
            SUSPENDUE.to_string()
        } else {
            derniere_action(&e.act, &e.by)
        },
        auditeurs: c
            .personnes
            .iter()
            .map(|(id, pseudo)| ParticipantSeance {
                id: id.clone(),
                pseudo: pseudo.clone(),
                nom: if &e.host == id {
                    format!("{pseudo} (hôte)")
                } else {
                    pseudo.clone()
                },
                pret: match e.ready.get(id) {
                    Some(true) => EtatPret::Pret,
                    Some(false) => EtatPret::Chargement,
                    None => EtatPret::Inconnu,
                },
            })
            .collect(),
        file,
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VueMiniMusique {
    pub salon: String,
    pub titre: String,
    pub sous_titre: String,
    pub pochette: String,
    pub lecture: bool,
    pub peut_piloter: bool,
    pub position: f64,
    pub duree: f64,
    pub texte_position: String,
    pub texte_duree: String,
    pub texte_restant: String,
}

pub fn vue_mini(
    s: &SeanceEnCours,
    salon: &str,
    nom_salon: Option<&str>,
    peut: bool,
    maintenant_ms: i64,
) -> Option<VueMiniMusique> {
    let meta = s.etat.meta.as_ref()?;
    let (position, duree) = position_musique(s, 0.0, maintenant_ms);
    let (texte_position, texte_duree, texte_restant) = textes_temps(position, duree);
    Some(VueMiniMusique {
        salon: salon.to_string(),
        titre: meta.title.clone(),
        sous_titre: match nom_salon.filter(|n| !n.is_empty()) {
            Some(nom) => format!("{} · {ECOUTE_ENSEMBLE} · {nom}", meta.artist),
            None => meta.artist.clone(),
        },
        pochette: chemin_pochette(&meta.cover).unwrap_or_default().to_string(),
        lecture: s.etat.playing,
        peut_piloter: peut,
        position,
        duree,
        texte_position,
        texte_duree,
        texte_restant,
    })
}

pub fn minis_du_serveur(
    etat: &Etat,
    serveur: Option<&str>,
    connecte: Option<&str>,
    maintenant_ms: i64,
) -> (Vec<VueMiniMusique>, Option<VueMiniMusique>) {
    let moi = etat.moi();
    let seance = |salon: &str| etat.seances.seance(GenreSeance::Musique, salon);
    let ici = connecte.and_then(|c| {
        let s = seance(c)?;
        let nom = etat
            .salon(c)
            .map(|(_, s)| id_de(&s.name).to_string())
            .unwrap_or_default();
        vue_mini(s, c, Some(&nom), peut_piloter(s, moi), maintenant_ms)
    });
    let minis = serveur
        .map(|sid| etat.salons(sid))
        .unwrap_or_default()
        .iter()
        .filter(|c| c.kind == Some(GenreSalon::Voice))
        .map(|c| id_de(&c.id))
        .filter(|id| !(ici.is_some() && Some(*id) == connecte))
        .filter_map(|id| vue_mini(seance(id)?, id, None, false, maintenant_ms))
        .collect();
    (minis, ici)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MorceauTrouve {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: i64,
    pub cover: String,
}

fn chemin_seance(salon: &str, genre: GenreSeance) -> String {
    format!("/voice/{}/{}", encoder_composant(salon), genre.chemin())
}

impl Client {
    pub async fn rechercher_musique(&self, texte: &str) -> Result<Vec<MorceauTrouve>, ErreurApi> {
        self.get(&format!("/music/search?q={}", encoder_composant(texte)))
            .await
    }

    pub async fn repetition_seance(
        &self,
        salon: &str,
        genre: GenreSeance,
        repetition: Repetition,
    ) -> Result<(), ErreurApi> {
        let corps = serde_json::json!({ "repeat": repetition.cle() });
        self.envoyer(
            Method::PUT,
            &format!("{}/options", chemin_seance(salon, genre)),
            &Corps::json(&corps)?,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::format::iso_de_ms;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::store::ParticipantVocal;
    use crate::temps_reel::evenements::{EtatSeance, Seance};

    const T0: i64 = 1_793_000_000_000;

    fn meta(titre: &str, artiste: &str, duree: i64) -> MetaMorceau {
        MetaMorceau {
            title: titre.into(),
            artist: artiste.into(),
            album: String::new(),
            duration: duree,
            cover: format!("/music/cover/{titre}"),
        }
    }

    fn element(id: &str, titre: &str, par: &str) -> ElementFile {
        ElementFile {
            id: id.into(),
            provider: "youtube".into(),
            video: format!("v-{id}"),
            by: par.into(),
            by_id: par.into(),
            meta: Some(meta(titre, "Artiste", 200)),
        }
    }

    fn seance(etat: EtatSeance) -> SeanceEnCours {
        SeanceEnCours { etat, decalage: 0 }
    }

    fn etat_musique() -> EtatSeance {
        EtatSeance {
            rev: 3,
            provider: "youtube".into(),
            video: "aaaaaaaaaaa".into(),
            playing: true,
            pos: 10.0,
            rate: 1.0,
            at: iso_de_ms(T0),
            host: "hote".into(),
            host_only: false,
            by: "alice".into(),
            act: "video".into(),
            meta: Some(MetaMorceau {
                album: "Album".into(),
                ..meta("Titre", "Artiste", 205)
            }),
            ..EtatSeance::default()
        }
    }

    #[test]
    fn formate_la_duree_d_un_morceau() {
        assert_eq!(horloge_morceau(205.0), "3:25");
        assert_eq!(horloge_morceau(3723.0), "1:02:03");
        assert_eq!(horloge_morceau(9.0), "0:09");
        assert_eq!(horloge_morceau(0.0), "");
        assert_eq!(horloge_morceau(f64::NAN), "");
    }

    #[test]
    fn recadre_la_miniature_4_3_sur_la_pochette_carree_sans_les_bandes_noires() {
        assert_eq!(
            cadrage_pochette(480, 360),
            Cadrage {
                x: 105,
                y: 45,
                cote: 270
            }
        );
        assert_eq!(
            cadrage_pochette(100, 360),
            Cadrage {
                x: 0,
                y: 130,
                cote: 100
            }
        );
    }

    #[test]
    fn calcule_la_couleur_moyenne_d_une_pochette_en_ignorant_les_pixels_transparents() {
        assert_eq!(
            couleur_moyenne(&[255, 0, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0]),
            Some([128, 0, 128])
        );
        assert_eq!(couleur_moyenne(&[1, 2, 3, 0]), None);
        assert_eq!(couleur_moyenne(&[]), None);
    }

    #[test]
    fn la_pochette_est_servie_par_estompe_et_un_lien_ou_une_recherche_sont_distingues() {
        assert_eq!(chemin_pochette("/music/cover/x"), Some("/music/cover/x"));
        assert_eq!(chemin_pochette("https://i.ytimg.com/vi/x/hq.jpg"), None);
        assert!(est_lien("https://music.youtube.com/watch?v=x"));
        assert!(est_lien("  Music.youtube.com/playlist?list=y"));
        assert!(!est_lien("daft punk"));
        assert_eq!(
            lien_du_morceau("abc"),
            "https://music.youtube.com/watch?v=abc"
        );
    }

    #[test]
    fn la_position_avance_s_arrete_en_pause_et_ne_depasse_pas_la_duree() {
        let s = seance(etat_musique());
        assert_eq!(position_musique(&s, 0.0, T0 + 5000), (15.0, 205.0));
        assert_eq!(position_musique(&s, 0.0, T0 + 600_000), (205.0, 205.0));
        let pause = seance(EtatSeance {
            playing: false,
            ..etat_musique()
        });
        assert_eq!(position_musique(&pause, 0.0, T0 + 60_000), (10.0, 205.0));
        let sans_duree = seance(EtatSeance {
            meta: Some(meta("T", "A", 0)),
            ..etat_musique()
        });
        assert_eq!(position_musique(&sans_duree, 99.6, T0), (10.0, 100.0));
        assert_eq!(
            position_musique(&sans_duree, 0.0, T0 + 600_000),
            (610.0, 0.0)
        );
        let decale = SeanceEnCours {
            etat: etat_musique(),
            decalage: -2000,
        };
        assert_eq!(position_musique(&decale, 0.0, T0 + 5000), (13.0, 205.0));
    }

    #[test]
    fn la_repetition_suit_le_cycle_du_front() {
        let mut r = Repetition::lire("");
        let mut vues = Vec::new();
        for _ in 0..3 {
            r = r.suivante();
            vues.push((r.cle(), r.description()));
        }
        assert_eq!(
            vues,
            [
                ("one", "Répéter le morceau"),
                ("all", "Répéter la file"),
                ("", "Répétition désactivée")
            ]
        );
        assert_eq!(Repetition::lire("n'importe"), Repetition::Aucune);
    }

    fn contexte<'a>(moi: &'a str, personnes: &'a [(String, String)]) -> ContexteMusique<'a> {
        ContexteMusique {
            moi,
            personnes,
            duree_lecteur: 0.0,
            suspendu: false,
            maintenant_ms: T0 + 2000,
        }
    }

    #[test]
    fn en_ecoute_montre_titre_voisins_file_auditeurs_et_commandes_selon_les_droits() {
        let mut e = etat_musique();
        e.queue = vec![element("q1", "Deux", "bob"), element("q2", "Trois", "moi")];
        e.history = vec![element("h1", "Un", "alice")];
        e.ready = [("hote".to_string(), true), ("bob".to_string(), false)]
            .into_iter()
            .collect();
        let personnes = vec![
            ("hote".to_string(), "Hôte".to_string()),
            ("bob".to_string(), "Bob".to_string()),
            ("moi".to_string(), "Moi".to_string()),
        ];
        let v = vue_musique(&seance(e.clone()), &contexte("moi", &personnes)).unwrap();
        assert_eq!(
            (v.titre.as_str(), v.details.as_str(), v.pochette.as_str()),
            ("Titre", "Artiste · Album", "/music/cover/Titre")
        );
        assert_eq!(
            (v.texte_position.as_str(), v.texte_duree.as_str()),
            ("0:12", "3:25")
        );
        assert!(v.peut_piloter && v.melanger && v.suivant && !v.hote && v.lecture);
        assert_eq!(v.statut, "alice a changé de vidéo");
        assert_eq!(
            v.precedent,
            Some(Voisin {
                titre: "Un".into(),
                infobulle: "Un · Artiste".into(),
                pochette: "/music/cover/Un".into()
            })
        );
        assert_eq!(v.prochain.unwrap().titre, "Deux");
        assert_eq!(
            v.file
                .iter()
                .map(|f| (
                    f.titre.as_str(),
                    f.details.as_str(),
                    f.duree.as_str(),
                    f.monter,
                    f.descendre,
                    f.retirer
                ))
                .collect::<Vec<_>>(),
            [
                ("Deux", "Artiste · bob", "3:20", false, true, true),
                ("Trois", "Artiste · moi", "3:20", true, false, true)
            ]
        );
        assert_eq!(
            v.auditeurs
                .iter()
                .map(|a| (a.nom.as_str(), a.pret))
                .collect::<Vec<_>>(),
            [
                ("Hôte (hôte)", EtatPret::Pret),
                ("Bob", EtatPret::Chargement),
                ("Moi", EtatPret::Inconnu)
            ]
        );
        e.host_only = true;
        e.queue.truncate(1);
        let v = vue_musique(&seance(e.clone()), &contexte("moi", &personnes)).unwrap();
        assert!(!v.peut_piloter && !v.melanger && !v.suivant);
        assert!(!v.file[0].retirer);
        let hote = vue_musique(&seance(e.clone()), &contexte("hote", &personnes)).unwrap();
        assert!(hote.hote && hote.peut_piloter && hote.suivant && !hote.melanger);
        e.queue.clear();
        e.host_only = false;
        assert!(
            !vue_musique(&seance(e.clone()), &contexte("moi", &personnes))
                .unwrap()
                .suivant
        );
        e.repeat = "all".into();
        let v = vue_musique(&seance(e.clone()), &contexte("moi", &personnes)).unwrap();
        assert!(v.suivant && v.repetition == Repetition::File && v.prochain.is_none());
        let suspendu = ContexteMusique {
            suspendu: true,
            ..contexte("moi", &personnes)
        };
        assert_eq!(
            vue_musique(&seance(e.clone()), &suspendu).unwrap().statut,
            SUSPENDUE
        );
        e.meta = None;
        assert!(vue_musique(&seance(e), &contexte("moi", &personnes)).is_none());
    }

    fn etat_du_serveur() -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": "moi", "pseudo": "moi"})).unwrap(),
            ..Etat::default()
        };
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({
                "server": {"id": "s1", "name": "Amis", "owner_id": "moi"},
                "channels": [
                    {"id": "t1", "name": "général", "kind": "text", "position": 0},
                    {"id": "v1", "name": "Vocal", "kind": "voice", "position": 1},
                    {"id": "v2", "name": "Jeux", "kind": "voice", "position": 2},
                    {"id": "v3", "name": "Calme", "kind": "voice", "position": 3}
                ],
                "members": [{"id": "moi", "pseudo": "moi"}]
            }))
            .unwrap(),
        );
        for (salon, gens) in [("v1", ["moi", "bob"]), ("v2", ["carol", "dave"])] {
            etat.vocal.insert(
                salon.into(),
                gens.iter()
                    .map(|u| ParticipantVocal {
                        utilisateur: u.to_string(),
                        pseudo: u.to_string(),
                        ..ParticipantVocal::default()
                    })
                    .collect(),
            );
        }
        for (salon, titre) in [("v1", "Titre"), ("v2", "Autre")] {
            let mut e = etat_musique();
            e.meta = Some(meta(titre, "Artiste", 205));
            e.host_only = salon == "v2";
            etat.seances.appliquer(
                GenreSeance::Musique,
                &Seance {
                    channel_id: salon.into(),
                    state: Some(e),
                    ..Seance::default()
                },
                true,
                T0,
            );
        }
        etat
    }

    #[test]
    fn les_mini_lecteurs_du_serveur_et_celui_du_salon_ou_l_on_est() {
        let etat = etat_du_serveur();
        let (minis, ici) = minis_du_serveur(&etat, Some("s1"), Some("v1"), T0 + 5000);
        let ici = ici.unwrap();
        assert_eq!(
            (
                ici.salon.as_str(),
                ici.sous_titre.as_str(),
                ici.peut_piloter
            ),
            ("v1", "Artiste · Écoute ensemble · Vocal", true)
        );
        assert_eq!(
            (
                ici.texte_position.as_str(),
                ici.texte_duree.as_str(),
                ici.texte_restant.as_str()
            ),
            ("0:15", "3:25", "-3:10")
        );
        assert_eq!(
            minis
                .iter()
                .map(|m| (m.salon.as_str(), m.sous_titre.as_str(), m.peut_piloter))
                .collect::<Vec<_>>(),
            [("v2", "Artiste", false)]
        );
        let (minis, ici) = minis_du_serveur(&etat, Some("s1"), None, T0);
        assert!(ici.is_none());
        assert_eq!(minis.len(), 2);
        let (minis, ici) = minis_du_serveur(&etat, None, Some("v3"), T0);
        assert!(minis.is_empty() && ici.is_none());
    }

    #[test]
    fn l_ecoute_est_montree_pour_les_personnes_d_un_salon_ou_joue_la_musique() {
        let mut etat = etat_du_serveur();
        assert_eq!(ecoute_de(&etat, "bob"), "Titre – Artiste");
        assert_eq!(ecoute_de(&etat, "carol"), "Autre – Artiste");
        assert_eq!(ecoute_de(&etat, "eve"), "");
        etat.seances.appliquer(
            GenreSeance::Musique,
            &Seance {
                channel_id: "v1".into(),
                ..Seance::default()
            },
            true,
            T0,
        );
        assert_eq!(ecoute_de(&etat, "bob"), "");
        assert_eq!(
            texte_ecoute(&MetaMorceau {
                title: "Seul".into(),
                ..MetaMorceau::default()
            }),
            "Seul"
        );
    }

    #[tokio::test]
    async fn la_recherche_et_la_repetition_partent_aux_bons_chemins() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(
                200,
                json!([{"id": "aaaaaaaaaaa", "title": "Un", "artist": "Art", "album": "", "duration": 125, "cover": "/music/cover/aaaaaaaaaaa"}]),
            ),
            Reponse::vide(204),
        ])
        .await;
        let client = serveur.client();
        let trouves = client.rechercher_musique("daft punk").await.unwrap();
        assert_eq!(trouves[0].duration, 125);
        assert_eq!(trouves[0].title, "Un");
        client
            .repetition_seance("v1", GenreSeance::Musique, Repetition::Morceau)
            .await
            .unwrap();
        let r = serveur.requetes();
        assert_eq!(r[0].ligne(), ("/music/search?q=daft%20punk", "GET"));
        assert_eq!(r[1].ligne(), ("/voice/v1/music/options", "PUT"));
        assert_eq!(r[1].json(), json!({"repeat": "one"}));
    }
}
