use crate::store::{id_de, Etat, GenreSalon};

use super::film::{PistesFilm, PrefTampon, Qualite};
use super::fournisseurs::libelle_fournisseur;
use super::pilote::VuePilote;
use super::seance::{derniere_action, salon_auto, texte_attente, GenreSeance, SeanceEnCours};

pub const FILE_MAX: usize = 10;
pub const VITESSES_SEANCE: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
pub const SUSPENDUE: &str = "Synchronisation suspendue quelques secondes : trop d’évènements.";
pub const SYNCHRONISEE: &str = "Lecture synchronisée";
pub const NE_SE_CHARGE_PAS: &str = "La vidéo ne se charge pas.";
pub const REPRENDRE_SANS_ATTENDRE: &str = " Appuyez sur lecture pour reprendre sans attendre.";

pub fn horloge_seance(t: f64) -> String {
    let t = t.max(0.0);
    format!(
        "{}:{:02}",
        (t / 60.0).floor() as u64,
        (t % 60.0).floor() as u64
    )
}

pub fn libelle_vitesse_seance(v: f64) -> String {
    format!("×{}", v).replace('.', ",")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choix {
    pub id: String,
    pub libelle: String,
    pub coche: bool,
}

fn choix(id: impl Into<String>, libelle: impl Into<String>, coche: bool) -> Choix {
    Choix {
        id: id.into(),
        libelle: libelle.into(),
        coche,
    }
}

pub const SANS_SOUS_TITRES: &str = "aucune";

pub fn choix_audio(p: &PistesFilm) -> Vec<Choix> {
    p.audio
        .iter()
        .map(|a| choix(a.n.to_string(), a.nom(), a.n == p.audio_n))
        .collect()
}

pub fn choix_sous_titres(p: &PistesFilm) -> Vec<Choix> {
    let mut v = vec![choix(
        SANS_SOUS_TITRES,
        "Sans sous-titres",
        p.sous_titres_n.is_none(),
    )];
    v.extend(
        p.sous_titres
            .iter()
            .map(|t| choix(t.n.to_string(), t.nom(), Some(t.n) == p.sous_titres_n)),
    );
    v
}

pub fn choix_qualites(p: &PistesFilm) -> Vec<Choix> {
    p.qualites
        .iter()
        .map(|q| choix(q.libelle(), q.libelle(), *q == p.qualite))
        .collect()
}

pub fn qualite_choisie(p: &PistesFilm, id: &str) -> Option<Qualite> {
    p.qualites.iter().copied().find(|q| q.libelle() == id)
}

pub fn choix_prechargement(actuel: PrefTampon) -> Vec<Choix> {
    PrefTampon::TOUS
        .iter()
        .map(|t| {
            choix(
                t.cle(),
                format!("Préchargement : {}", t.libelle()),
                *t == actuel,
            )
        })
        .collect()
}

pub fn prechargement_choisi(id: &str) -> Option<PrefTampon> {
    PrefTampon::TOUS.into_iter().find(|t| t.cle() == id)
}

pub fn choix_vitesses(actuelle: f64) -> Vec<Choix> {
    VITESSES_SEANCE
        .iter()
        .enumerate()
        .map(|(i, v)| {
            choix(
                i.to_string(),
                libelle_vitesse_seance(*v),
                (v - actuelle).abs() < 1e-6,
            )
        })
        .collect()
}

pub fn vitesse_choisie(id: &str) -> Option<f64> {
    id.parse::<usize>()
        .ok()
        .and_then(|i| VITESSES_SEANCE.get(i).copied())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatPret {
    Pret,
    Chargement,
    Inconnu,
}

impl EtatPret {
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Pret => "prêt",
            Self::Chargement => "chargement",
            Self::Inconnu => "…",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticipantSeance {
    pub id: String,
    pub pseudo: String,
    pub nom: String,
    pub pret: EtatPret,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementFileVue {
    pub id: String,
    pub libelle: String,
    pub monter: bool,
    pub descendre: bool,
    pub retirer: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum MessageSeance {
    #[default]
    Aucun,
    Erreur(String),
    Lent,
    Bloque,
    Attente(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VueSeance {
    pub genre: GenreSeance,
    pub titre: String,
    pub sous_titre: String,
    pub ajoute_par: String,
    pub statut: String,
    pub suspendu: bool,
    pub peut_piloter: bool,
    pub hote: bool,
    pub hote_seul: bool,
    pub bascule_hote: bool,
    pub suivant: Option<String>,
    pub arreter: bool,
    pub direct: bool,
    pub lecture: bool,
    pub message: MessageSeance,
    pub passer_suivante: bool,
    pub grand_bouton: bool,
    pub chargement: bool,
    pub passer_intro: bool,
    pub barre: bool,
    pub position: f64,
    pub duree: f64,
    pub temps: String,
    pub vitesse: f64,
    pub navigateur: Option<String>,
    pub participants: Vec<ParticipantSeance>,
    pub file: Vec<ElementFileVue>,
    pub compte_file: String,
    pub en_lecture: String,
}

pub struct ContexteVue<'a> {
    pub genre: GenreSeance,
    pub moi: &'a str,
    pub personnes: &'a [(String, String)],
    pub lecteur: bool,
    pub intro_passable: bool,
}

fn titre_de(provider: &str, titre: Option<&str>) -> String {
    match titre {
        Some(t) if !t.is_empty() => t.to_string(),
        _ if provider.is_empty() => "Vidéo".to_string(),
        _ => libelle_fournisseur(provider).to_string(),
    }
}

fn sans_schema(adresse: &str) -> &str {
    adresse
        .strip_prefix("https://")
        .or_else(|| adresse.strip_prefix("http://"))
        .unwrap_or(adresse)
}

pub fn vue_seance(seance: &SeanceEnCours, pilote: &VuePilote, c: &ContexteVue) -> VueSeance {
    let s = &seance.etat;
    let peut = pilote.peut_piloter;
    let hote = s.host == c.moi;
    let film = s.provider == "film";
    let direct = matches!(s.provider.as_str(), "twitch_live" | "twitch_clip");
    let titre = titre_de(&s.provider, s.meta.as_ref().map(|m| m.title.as_str()));
    let fournisseur = titre_de(&s.provider, None);
    let ajoute_par = format!("ajouté par {}", s.by);
    let a_un_titre = s.meta.as_ref().is_some_and(|m| !m.title.is_empty());
    let personnes: Vec<(&str, &str)> = c
        .personnes
        .iter()
        .map(|(u, p)| (u.as_str(), p.as_str()))
        .collect();
    let attente = texte_attente(&s.waiting, c.moi, &personnes);
    let message = if !pilote.erreur.is_empty() {
        MessageSeance::Erreur(pilote.erreur.clone())
    } else if pilote.lent {
        MessageSeance::Lent
    } else if pilote.bloque {
        MessageSeance::Bloque
    } else if !attente.is_empty() {
        MessageSeance::Attente(if peut {
            format!("{attente}{REPRENDRE_SANS_ATTENDRE}")
        } else {
            attente
        })
    } else {
        MessageSeance::Aucun
    };
    let libre = matches!(message, MessageSeance::Aucun | MessageSeance::Attente(_));
    let action = derniere_action(&s.act, &s.by);
    let n = s.queue.len();
    let file = s
        .queue
        .iter()
        .enumerate()
        .map(|(i, q)| ElementFileVue {
            id: q.id.clone(),
            libelle: if q.provider == "film" {
                q.meta
                    .as_ref()
                    .map(|m| m.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Épisode".to_string())
            } else {
                let video = if q.provider == "file" {
                    sans_schema(&q.video)
                } else {
                    q.video.as_str()
                };
                format!("{} · {video} · {}", titre_de(&q.provider, None), q.by)
            },
            monter: peut && i > 0,
            descendre: peut && i + 1 < n,
            retirer: peut || q.by_id == c.moi,
        })
        .collect();
    let en_lecture = if a_un_titre {
        titre.clone()
    } else if s.provider == "file" {
        sans_schema(&s.video).to_string()
    } else {
        fournisseur.clone()
    };
    VueSeance {
        genre: c.genre,
        titre,
        sous_titre: if a_un_titre {
            format!("{fournisseur} · {ajoute_par}")
        } else {
            ajoute_par.clone()
        },
        ajoute_par,
        statut: if pilote.suspendu {
            SUSPENDUE.to_string()
        } else if action.is_empty() {
            SYNCHRONISEE.to_string()
        } else {
            action
        },
        suspendu: pilote.suspendu,
        peut_piloter: peut,
        hote,
        hote_seul: s.host_only,
        bascule_hote: hote && !film,
        suivant: (peut && n > 0)
            .then(|| if film { "Épisode suivant" } else { "Suivante" }.to_string()),
        arreter: peut,
        direct,
        lecture: s.playing,
        passer_suivante: n > 0 || peut,
        grand_bouton: libre && c.lecteur && !direct && !s.playing && peut,
        chargement: libre && c.lecteur && !direct && !(!s.playing && peut) && pilote.chargement,
        message,
        passer_intro: c.intro_passable,
        barre: c.lecteur && !direct,
        position: pilote.position,
        duree: pilote.duree,
        temps: if pilote.duree > 0.0 {
            format!(
                "{} / {}",
                horloge_seance(pilote.position),
                horloge_seance(pilote.duree)
            )
        } else {
            horloge_seance(pilote.position)
        },
        vitesse: s.rate,
        navigateur: pilote.navigateur.clone(),
        participants: c
            .personnes
            .iter()
            .map(|(id, pseudo)| ParticipantSeance {
                id: id.clone(),
                pseudo: pseudo.clone(),
                nom: if &s.host == id {
                    format!("{pseudo} (hôte)")
                } else {
                    pseudo.clone()
                },
                pret: match s.ready.get(id) {
                    Some(true) => EtatPret::Pret,
                    Some(false) => EtatPret::Chargement,
                    None => EtatPret::Inconnu,
                },
            })
            .collect(),
        file,
        compte_file: format!("{n} / {FILE_MAX}"),
        en_lecture,
    }
}

pub fn file_deplacee(ids: &[String], i: usize, monter: bool) -> Option<Vec<String>> {
    let j = if monter { i.checked_sub(1)? } else { i + 1 };
    if i >= ids.len() || j >= ids.len() {
        return None;
    }
    let mut ids = ids.to_vec();
    ids.swap(i, j);
    Some(ids)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SalonPossible {
    pub id: String,
    pub serveur: String,
    pub nom: String,
    pub nom_serveur: Option<String>,
    pub en_cours: Option<String>,
    pub ici: bool,
    pub personnes: Vec<(String, String)>,
}

impl SalonPossible {
    pub fn plus(&self) -> Option<String> {
        (self.personnes.len() > 3).then(|| format!("+{}", self.personnes.len() - 3))
    }
}

pub fn titre_choix(genre: GenreSeance) -> &'static str {
    match genre {
        GenreSeance::Musique => "Écouter ensemble dans…",
        GenreSeance::Cinema => "Regarder au cinéma dans…",
        GenreSeance::Regarder => "Regarder ensemble dans…",
    }
}

pub fn aucun_salon(genre: GenreSeance, un_serveur: bool) -> String {
    let salon = if genre == GenreSeance::Cinema {
        "salon cinéma"
    } else {
        "salon vocal"
    };
    if un_serveur {
        format!("Aucun {salon} disponible sur ce serveur.")
    } else {
        format!("Aucun {salon} disponible.")
    }
}

fn genre_du_salon(genre: GenreSeance) -> GenreSalon {
    match genre {
        GenreSeance::Cinema => GenreSalon::Cinema,
        _ => GenreSalon::Voice,
    }
}

pub fn personnes_du_salon(etat: &Etat, salon: &str) -> Vec<(String, String)> {
    etat.participants(salon)
        .iter()
        .filter(|p| !p.utilisateur.ends_with("#screen"))
        .map(|p| (p.utilisateur.clone(), p.pseudo.clone()))
        .collect()
}

pub fn salons_possibles(
    etat: &Etat,
    serveur: Option<&str>,
    genre: GenreSeance,
    connecte: Option<&str>,
) -> Vec<SalonPossible> {
    let serveurs: Vec<String> = match serveur {
        Some(s) => vec![s.to_string()],
        None => etat.ids_serveurs_ordonnes(),
    };
    let voulu = genre_du_salon(genre);
    let mut salons: Vec<SalonPossible> = serveurs
        .iter()
        .flat_map(|sid| {
            etat.salons(sid)
                .iter()
                .filter(move |c| {
                    c.kind == Some(voulu) && etat.peut(Some(sid), "connect", Some(id_de(&c.id)))
                })
                .map(move |c| {
                    let id = id_de(&c.id).to_string();
                    SalonPossible {
                        en_cours: etat.seances.seance(genre, &id).map(|s| {
                            s.etat
                                .meta
                                .as_ref()
                                .map(|m| m.title.clone())
                                .filter(|t| !t.is_empty())
                                .unwrap_or_else(|| s.etat.video.clone())
                        }),
                        ici: Some(id.as_str()) == connecte,
                        personnes: personnes_du_salon(etat, &id),
                        nom: id_de(&c.name).to_string(),
                        nom_serveur: serveur.is_none().then(|| {
                            etat.serveur(sid)
                                .map(|s| id_de(&s.name).to_string())
                                .unwrap_or_default()
                        }),
                        serveur: sid.clone(),
                        id,
                    }
                })
        })
        .collect();
    salons.sort_by_key(|s| !s.ici);
    salons
}

pub fn salon_choisi_seul<'a>(
    salons: &'a [SalonPossible],
    connecte: Option<&str>,
) -> Option<&'a SalonPossible> {
    salon_auto(salons, connecte, |s| s.id.as_str())
}

pub fn texte_sans_acces(autres: &[&str]) -> String {
    let debut = match autres {
        [] => "Une séance est en cours. Tu restes en vocal".to_string(),
        [un] => format!("{un} regarde cette séance. Tu restes en vocal"),
        _ => format!(
            "{} regardent cette séance. Tu restes en vocal avec eux",
            autres.join(", ")
        ),
    };
    format!("{debut}, mais tu n'as pas encore accès aux films et séries d’Entracte.")
}

pub fn demande_d_acces(pseudo: &str, salon: &str) -> (String, String) {
    (
        "Accès aux films et séries demandé".to_string(),
        format!("{pseudo} demande la permission entracte:access pour rejoindre une séance dans le salon cinéma « {salon} »."),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::temps_reel::evenements::{ElementFile, EtatSeance, MetaMorceau};

    fn etat_seance() -> EtatSeance {
        EtatSeance {
            rev: 3,
            provider: "film".into(),
            video: "f1".into(),
            playing: true,
            pos: 45.0,
            rate: 1.0,
            host: "u".into(),
            by: "Ana".into(),
            by_id: "u".into(),
            act: "play".into(),
            meta: Some(MetaMorceau {
                title: "Le Film".into(),
                ..MetaMorceau::default()
            }),
            ..EtatSeance::default()
        }
    }

    fn pilote(peut: bool) -> VuePilote {
        VuePilote {
            position: 45.0,
            duree: 3000.0,
            peut_piloter: peut,
            ..VuePilote::default()
        }
    }

    fn personnes() -> Vec<(String, String)> {
        vec![
            ("u".to_string(), "Ana".to_string()),
            ("b".to_string(), "Bob".to_string()),
            ("c".to_string(), "Cléo".to_string()),
        ]
    }

    fn vue(etat: EtatSeance, p: VuePilote, moi: &str, intro: bool) -> VueSeance {
        let personnes = personnes();
        vue_seance(
            &SeanceEnCours { etat, decalage: 0 },
            &p,
            &ContexteVue {
                genre: GenreSeance::Cinema,
                moi,
                personnes: &personnes,
                lecteur: true,
                intro_passable: intro,
            },
        )
    }

    #[test]
    fn la_seance_montre_titre_temps_participants_et_commandes_selon_les_droits() {
        let mut e = etat_seance();
        e.ready.insert("u".into(), true);
        e.ready.insert("b".into(), false);
        e.queue = vec![ElementFile {
            id: "q1".into(),
            provider: "film".into(),
            video: "f2".into(),
            by: "Ana".into(),
            by_id: "u".into(),
            meta: Some(MetaMorceau {
                title: "Épisode 2".into(),
                ..MetaMorceau::default()
            }),
        }];
        let v = vue(e.clone(), pilote(true), "u", true);
        assert_eq!(v.titre, "Le Film");
        assert_eq!(v.sous_titre, "Films et séries · ajouté par Ana");
        assert_eq!(v.statut, "Ana a relancé la lecture");
        assert_eq!(v.temps, "0:45 / 50:00");
        assert!(
            v.hote && !v.bascule_hote,
            "un film n'a pas « Seul l'hôte pilote »"
        );
        assert_eq!(v.suivant.as_deref(), Some("Épisode suivant"));
        assert!(v.arreter && v.passer_intro && v.barre && !v.grand_bouton);
        assert_eq!(v.compte_file, "1 / 10");
        assert_eq!(v.file[0].libelle, "Épisode 2");
        assert!(v.file[0].retirer && !v.file[0].monter && !v.file[0].descendre);
        assert_eq!(
            v.participants
                .iter()
                .map(|p| (p.nom.as_str(), p.pret.libelle()))
                .collect::<Vec<_>>(),
            [("Ana (hôte)", "prêt"), ("Bob", "chargement"), ("Cléo", "…")]
        );
        e.host_only = true;
        e.playing = false;
        let v = vue(e, pilote(false), "b", false);
        assert!(!v.peut_piloter && v.suivant.is_none() && !v.arreter && !v.grand_bouton);
        assert!(!v.file[0].retirer);
        assert!(v.hote_seul && !v.hote);
    }

    #[test]
    fn un_fichier_heberge_se_pilote_par_l_hote_seul_et_la_file_se_reordonne() {
        let mut e = etat_seance();
        e.provider = "file".into();
        e.video = "https://x.test/v.mp4".into();
        e.meta = None;
        e.playing = false;
        e.queue = ["a", "b"]
            .iter()
            .map(|id| ElementFile {
                id: id.to_string(),
                provider: "file".into(),
                video: format!("https://x.test/{id}.mp4"),
                by: "Bob".into(),
                by_id: "b".into(),
                meta: None,
            })
            .collect();
        let v = vue(e, pilote(true), "u", false);
        assert_eq!(v.titre, "Fichier");
        assert_eq!(v.sous_titre, "ajouté par Ana");
        assert_eq!(v.en_lecture, "x.test/v.mp4");
        assert!(v.bascule_hote && v.grand_bouton && !v.chargement);
        assert_eq!(v.suivant.as_deref(), Some("Suivante"));
        assert_eq!(v.file[0].libelle, "Fichier · x.test/a.mp4 · Bob");
        assert!(v.file[1].monter && !v.file[1].descendre);
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(file_deplacee(&ids, 1, true).unwrap(), ["b", "a", "c"]);
        assert_eq!(file_deplacee(&ids, 1, false).unwrap(), ["a", "c", "b"]);
        assert!(file_deplacee(&ids, 0, true).is_none());
        assert!(file_deplacee(&ids, 2, false).is_none());
    }

    #[test]
    fn l_attente_l_erreur_la_lenteur_et_le_blocage_se_remplacent_dans_l_ordre() {
        let mut e = etat_seance();
        e.waiting = vec!["b".into()];
        let v = vue(e.clone(), pilote(true), "u", false);
        assert_eq!(
            v.message,
            MessageSeance::Attente(format!(
                "Lecture en pause : Bob charge la vidéo. Elle reprendra toute seule.{REPRENDRE_SANS_ATTENDRE}"
            ))
        );
        let v = vue(e.clone(), pilote(false), "c", false);
        assert_eq!(
            v.message,
            MessageSeance::Attente(
                "Lecture en pause : Bob charge la vidéo. Elle reprendra toute seule.".into()
            )
        );
        let mut p = pilote(true);
        p.bloque = true;
        assert_eq!(
            vue(e.clone(), p.clone(), "u", false).message,
            MessageSeance::Bloque
        );
        p.lent = true;
        assert_eq!(
            vue(e.clone(), p.clone(), "u", false).message,
            MessageSeance::Lent
        );
        p.erreur = "Lecture impossible.".into();
        p.suspendu = true;
        let v = vue(e, p, "u", false);
        assert_eq!(
            v.message,
            MessageSeance::Erreur("Lecture impossible.".into())
        );
        assert_eq!(v.statut, SUSPENDUE);
        assert!(v.passer_suivante);
    }

    #[test]
    fn menus_des_pistes_de_la_qualite_du_prechargement_et_de_la_vitesse() {
        use crate::regarder_ensemble::film::PisteFilm;
        let piste = |n: i64, label: &str| PisteFilm {
            n,
            label: Some(label.into()),
            ..PisteFilm::default()
        };
        let p = PistesFilm {
            audio: vec![piste(0, "Français"), piste(1, "Anglais")],
            sous_titres: vec![piste(0, "Français (forcés)")],
            audio_n: 1,
            sous_titres_n: None,
            qualites: vec![Qualite::Originale, Qualite::P720],
            qualite: Qualite::P720,
            intro: None,
        };
        let a = choix_audio(&p);
        assert_eq!(
            (a[1].id.as_str(), a[1].libelle.as_str(), a[1].coche),
            ("1", "Anglais", true)
        );
        let st = choix_sous_titres(&p);
        assert_eq!((st[0].id.as_str(), st[0].coche), (SANS_SOUS_TITRES, true));
        assert_eq!(st[1].libelle, "Français (forcés)");
        let q = choix_qualites(&p);
        assert!(q[1].coche && !q[0].coche);
        assert_eq!(qualite_choisie(&p, "720p"), Some(Qualite::P720));
        assert_eq!(qualite_choisie(&p, "1080p"), None);
        let t = choix_prechargement(PrefTampon::Long);
        assert_eq!(t[1].libelle, "Préchargement : Long (15 min)");
        assert!(t[1].coche);
        assert_eq!(prechargement_choisi("all"), Some(PrefTampon::Tout));
        let v = choix_vitesses(1.25);
        assert_eq!(v.iter().filter(|c| c.coche).count(), 1);
        assert_eq!(vitesse_choisie("3"), Some(1.25));
        assert_eq!(vitesse_choisie("9"), None);
    }

    #[test]
    fn horloge_et_vitesses() {
        assert_eq!(horloge_seance(0.0), "0:00");
        assert_eq!(horloge_seance(3725.9), "62:05");
        assert_eq!(libelle_vitesse_seance(1.25), "×1,25");
        assert_eq!(libelle_vitesse_seance(2.0), "×2");
    }

    fn etat_du_choix() -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": "me", "pseudo": "moi"})).unwrap(),
            serveurs: serde_json::from_value(json!([
                {"id": "s1", "name": "Amis", "owner_id": "me"},
                {"id": "s2", "name": "Club", "owner_id": "x"}
            ]))
            .unwrap(),
            ..Etat::default()
        };
        for (sid, salons) in [
            (
                "s1",
                json!([
                    {"id": "t1", "name": "général", "kind": "text", "position": 0},
                    {"id": "v1", "name": "Vocal", "kind": "voice", "position": 1},
                    {"id": "v2", "name": "Jeux", "kind": "voice", "position": 2},
                    {"id": "k1", "name": "Ciné", "kind": "cinema", "position": 3}
                ]),
            ),
            (
                "s2",
                json!([
                    {"id": "v3", "name": "Salon", "kind": "voice", "position": 0},
                    {"id": "v4", "name": "Privé", "kind": "voice", "position": 1, "permissions": ["view_channel"]}
                ]),
            ),
        ] {
            etat.details.insert(
                sid.into(),
                serde_json::from_value(json!({
                    "server": {"id": sid, "name": sid, "owner_id": "me"},
                    "channels": salons,
                    "members": [{"id": "me", "pseudo": "moi"}]
                }))
                .unwrap(),
            );
        }
        etat.vocal.insert(
            "v2".into(),
            ["a", "b", "c", "d", "e#screen", "f"]
                .iter()
                .map(|u| crate::store::ParticipantVocal {
                    utilisateur: u.to_string(),
                    pseudo: u.to_uppercase(),
                    ..Default::default()
                })
                .collect(),
        );
        etat
    }

    #[test]
    fn le_choix_du_salon_liste_les_salons_du_genre_le_salon_connecte_d_abord() {
        let mut etat = etat_du_choix();
        let vue = crate::temps_reel::evenements::Seance {
            channel_id: "v1".into(),
            state: Some(EtatSeance {
                video: "abc".into(),
                ..EtatSeance::default()
            }),
            ..Default::default()
        };
        etat.seances.appliquer(GenreSeance::Regarder, &vue, true, 0);
        let salons = salons_possibles(&etat, Some("s1"), GenreSeance::Regarder, Some("v2"));
        assert_eq!(
            salons.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["v2", "v1"]
        );
        assert!(salons[0].ici && salons[0].nom_serveur.is_none());
        assert_eq!(salons[0].personnes.len(), 5);
        assert_eq!(salons[0].plus().as_deref(), Some("+2"));
        assert_eq!(salons[1].en_cours.as_deref(), Some("abc"));
        assert_eq!(salon_choisi_seul(&salons, Some("v2")).unwrap().id, "v2");
        assert!(salon_choisi_seul(&salons, None).is_none());
        let tous = salons_possibles(&etat, None, GenreSeance::Regarder, None);
        assert_eq!(tous.len(), 3);
        assert_eq!(tous[2].nom_serveur.as_deref(), Some("Club"));
        let cine = salons_possibles(&etat, Some("s1"), GenreSeance::Cinema, None);
        assert_eq!(salon_choisi_seul(&cine, None).unwrap().id, "k1");
        assert_eq!(titre_choix(GenreSeance::Cinema), "Regarder au cinéma dans…");
        assert_eq!(
            aucun_salon(GenreSeance::Cinema, true),
            "Aucun salon cinéma disponible sur ce serveur."
        );
        assert_eq!(
            aucun_salon(GenreSeance::Regarder, false),
            "Aucun salon vocal disponible."
        );
    }

    #[test]
    fn sans_acces_au_titre_le_texte_nomme_les_spectateurs() {
        assert_eq!(
            texte_sans_acces(&[]),
            "Une séance est en cours. Tu restes en vocal, mais tu n'as pas encore accès aux films et séries d’Entracte."
        );
        assert!(
            texte_sans_acces(&["Ana"]).starts_with("Ana regarde cette séance. Tu restes en vocal,")
        );
        assert!(texte_sans_acces(&["Ana", "Bob"])
            .starts_with("Ana, Bob regardent cette séance. Tu restes en vocal avec eux,"));
        let (titre, texte) = demande_d_acces("moi", "Ciné");
        assert_eq!(titre, "Accès aux films et séries demandé");
        assert!(texte.contains("entracte:access") && texte.contains("« Ciné »"));
    }
}
