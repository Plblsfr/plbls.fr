use estompe_client::format::formater_duree;
use estompe_client::roles::domine;
use estompe_client::store::{Etat, GenreSalon};
use estompe_client::vocal::salle::est_partage;
use estompe_client::vocal::{EtatVocal, Phase, Qualite};

use crate::coquille::avatars::CleAvatar;
use crate::coquille::membres::cle_avatar;
use crate::coquille::projection::{initiales, teinte};

fn id_de(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

pub const PLACES: usize = 10;
pub const SEUIL_JAUGE_DB: f32 = -80.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub salon: String,
    pub utilisateur: String,
    pub nom: String,
    pub initiales: String,
    pub teinte: i32,
    pub moi: bool,
    pub parle: bool,
    pub muet: bool,
    pub sourd: bool,
    pub micro_impose: bool,
    pub son_impose: bool,
    pub barres: i32,
    pub qualite: &'static str,
    pub avatar: Option<CleAvatar>,
    pub portrait: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vue {
    pub salon: String,
    pub nom: String,
    pub cinema: bool,
    pub ici: bool,
    pub connexion: bool,
    pub reconnexion: bool,
    pub peut_rejoindre: bool,
    pub erreur: String,
    pub compte: String,
    pub resume: String,
    pub seul: bool,
    pub colonnes: i32,
    pub rangees: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barre {
    pub visible: bool,
    pub connexion: bool,
    pub reconnexion: bool,
    pub titre: String,
    pub salon: String,
    pub serveur: String,
    pub lieu: String,
    pub duree: String,
    pub barres: i32,
    pub qualite: &'static str,
    pub micro_coupe: bool,
    pub casque_coupe: bool,
    pub micro_impose: bool,
    pub son_impose: bool,
    pub peut_parler: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub utilisateur: String,
    pub nom: String,
    pub muet: bool,
    pub volume: i32,
    pub peut_couper_micro: bool,
    pub peut_couper_son: bool,
    pub micro_impose: bool,
    pub son_impose: bool,
    pub destinations: Vec<(String, String)>,
    pub peut_deconnecter: bool,
    pub erreur: String,
}

pub fn barres(qualite: Qualite) -> (i32, &'static str) {
    match qualite {
        Qualite::Excellente => (3, "Connexion excellente"),
        Qualite::Bonne => (3, "Connexion bonne"),
        Qualite::Faible => (2, "Connexion médiocre"),
        Qualite::Perdue => (1, "Connexion perdue"),
        Qualite::Inconnue => (0, ""),
    }
}

pub fn grille(nombre: usize) -> (i32, i32) {
    if nombre <= 1 {
        return (1, 1);
    }
    let colonnes = match nombre {
        0..=4 => 2,
        5..=9 => 3,
        _ => 4,
    };
    (colonnes as i32, nombre.div_ceil(colonnes) as i32)
}

pub fn ici(vocal: &EtatVocal, salon: &str) -> bool {
    vocal.dans(salon)
}

pub fn participants(
    etat: &Etat,
    vocal: &EtatVocal,
    salon: &str,
    moi: &str,
    je_parle: bool,
) -> Vec<Participant> {
    let ici = ici(vocal, salon);
    let mut liste: Vec<(String, String, bool, bool)> = etat
        .participants(salon)
        .iter()
        .filter(|p| !est_partage(&p.utilisateur))
        .map(|p| {
            (
                p.utilisateur.clone(),
                p.pseudo.clone(),
                p.micro_coupe,
                p.son_coupe,
            )
        })
        .collect();
    if ici && !liste.iter().any(|(u, ..)| u == moi) {
        liste.insert(
            0,
            (moi.to_string(), etat.pseudo().to_string(), false, false),
        );
    }
    let affiches = vocal.affiches();
    let serveur = etat.serveur_du_salon(salon);
    liste
        .into_iter()
        .map(|(utilisateur, nom, micro_impose, son_impose)| {
            let est_moi = utilisateur == moi;
            let (muet, sourd) = match (ici, est_moi) {
                (true, true) => (vocal.micro_coupe, vocal.casque_coupe),
                (true, false) => affiches
                    .get(&utilisateur)
                    .map_or((false, false), |a| (a.muet, a.sourd)),
                (false, _) => (false, false),
            };
            let parle = ici
                && !muet
                && !sourd
                && (vocal.parlent.contains(&utilisateur) || (est_moi && je_parle));
            let (n, libelle) = if ici {
                barres(vocal.qualite(&utilisateur))
            } else {
                (0, "")
            };
            Participant {
                salon: salon.to_string(),
                initiales: initiales(&nom),
                teinte: teinte(&utilisateur),
                moi: est_moi,
                parle,
                muet,
                sourd,
                micro_impose,
                son_impose,
                barres: n,
                qualite: libelle,
                avatar: serveur
                    .as_deref()
                    .and_then(|s| cle_avatar(etat, s, &utilisateur)),
                portrait: false,
                nom,
                utilisateur,
            }
        })
        .collect()
}

pub fn salons_vocaux(etat: &Etat, serveur: &str) -> Vec<(String, String)> {
    etat.salons(serveur)
        .iter()
        .filter(|s| matches!(s.kind, Some(GenreSalon::Voice | GenreSalon::Cinema)))
        .map(|s| (id_de(&s.id).to_string(), id_de(&s.name).to_string()))
        .collect()
}

pub fn participants_du_serveur(
    etat: &Etat,
    vocal: &EtatVocal,
    serveur: &str,
    moi: &str,
    je_parle: bool,
) -> Vec<Participant> {
    etat.salons(serveur)
        .iter()
        .filter(|s| matches!(s.kind, Some(GenreSalon::Voice | GenreSalon::Cinema)))
        .flat_map(|s| participants(etat, vocal, id_de(&s.id), moi, je_parle))
        .collect()
}

fn pluriel(n: usize) -> &'static str {
    if n > 1 {
        "s"
    } else {
        ""
    }
}

pub fn vue(etat: &Etat, vocal: &EtatVocal, salon: &str, nombre: usize) -> Vue {
    let (serveur, nom, cinema) = etat
        .salon(salon)
        .map(|(d, s)| {
            (
                d.server.as_ref().map(|s| id_de(&s.id).to_string()),
                id_de(&s.name).to_string(),
                s.kind == Some(GenreSalon::Cinema),
            )
        })
        .unwrap_or_default();
    let ici = ici(vocal, salon);
    let concerne = vocal.salon.as_deref() == Some(salon) || !vocal.actif();
    let (colonnes, rangees) = grille(nombre);
    Vue {
        salon: salon.to_string(),
        nom,
        cinema,
        ici,
        connexion: ici && vocal.phase == Phase::Connexion,
        reconnexion: ici && vocal.phase == Phase::Reconnexion,
        peut_rejoindre: etat.peut(serveur.as_deref(), "connect", Some(salon)),
        erreur: if concerne {
            vocal.erreur.clone().unwrap_or_default()
        } else {
            String::new()
        },
        compte: format!("{nombre} / {PLACES}"),
        resume: if nombre == 0 {
            "Personne pour le moment.".to_string()
        } else {
            format!("{nombre} / {PLACES} participant{}", pluriel(nombre))
        },
        seul: ici && nombre == 1,
        colonnes,
        rangees,
    }
}

pub fn barre(
    etat: &Etat,
    vocal: &EtatVocal,
    moi: &str,
    maintenant_ms: i64,
    debut_ms: Option<i64>,
) -> Barre {
    let Some(salon) = vocal.salon.as_deref().filter(|_| vocal.actif()) else {
        return Barre {
            visible: false,
            connexion: false,
            reconnexion: false,
            titre: String::new(),
            salon: String::new(),
            serveur: String::new(),
            lieu: String::new(),
            duree: String::new(),
            barres: 0,
            qualite: "",
            micro_coupe: false,
            casque_coupe: false,
            micro_impose: false,
            son_impose: false,
            peut_parler: true,
            message: vocal.erreur.clone().unwrap_or_default(),
        };
    };
    let (serveur, nom_serveur, nom_salon) = etat
        .salon(salon)
        .map(|(d, s)| {
            let serveur = d.server.as_ref();
            (
                serveur
                    .map(|s| id_de(&s.id).to_string())
                    .unwrap_or_default(),
                serveur
                    .map(|s| id_de(&s.name).to_string())
                    .unwrap_or_default(),
                id_de(&s.name).to_string(),
            )
        })
        .unwrap_or_default();
    let (n, libelle) = barres(vocal.qualite(moi));
    let titre = match vocal.phase {
        Phase::Connexion => "Connexion…",
        Phase::Reconnexion => "Reconnexion…",
        _ => "Vocal connecté",
    };
    let duree = match (vocal.phase, debut_ms) {
        (Phase::Connecte | Phase::Reconnexion, Some(debut)) => {
            formater_duree((maintenant_ms - debut) as f64 / 1000.0)
        }
        _ => String::new(),
    };
    let message = vocal
        .micro_indisponible
        .map(|c| c.erreur())
        .or_else(|| vocal.erreur.clone())
        .unwrap_or_default();
    Barre {
        visible: true,
        connexion: vocal.phase == Phase::Connexion,
        reconnexion: vocal.phase == Phase::Reconnexion,
        titre: titre.to_string(),
        lieu: if nom_serveur.is_empty() {
            nom_salon.clone()
        } else {
            format!("{nom_salon} · {nom_serveur}")
        },
        salon: salon.to_string(),
        serveur,
        duree,
        barres: n,
        qualite: libelle,
        micro_coupe: vocal.micro_coupe,
        casque_coupe: vocal.casque_coupe,
        micro_impose: vocal.micro_impose,
        son_impose: vocal.son_impose,
        peut_parler: etat.peut(
            etat.serveur_du_salon(salon).as_deref(),
            "speak",
            Some(salon),
        ),
        message,
    }
}

pub fn menu(
    etat: &Etat,
    vocal: &EtatVocal,
    salon: &str,
    utilisateur: &str,
    moi: &str,
) -> Option<Menu> {
    if utilisateur == moi {
        return None;
    }
    let participant = etat
        .participants(salon)
        .iter()
        .find(|p| p.utilisateur == utilisateur);
    let nom = participant
        .map(|p| p.pseudo.clone())
        .or_else(|| vocal.participants.get(utilisateur).map(|p| p.nom.clone()))?;
    let serveur = etat.serveur_du_salon(salon);
    let detail = serveur.as_deref().and_then(|s| etat.details.get(s));
    let modere = domine(detail, moi, utilisateur);
    let destinations = if modere && etat.peut(serveur.as_deref(), "move_members", None) {
        serveur
            .as_deref()
            .map(|s| salons_vocaux(etat, s))
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| id != salon)
            .collect()
    } else {
        Vec::new()
    };
    let source = estompe_client::vocal::SourceAudio::Micro;
    Some(Menu {
        utilisateur: utilisateur.to_string(),
        nom,
        muet: vocal.ecoute.muet(utilisateur, source),
        volume: i32::from(vocal.ecoute.volume(utilisateur, source)),
        peut_couper_micro: modere && etat.peut(serveur.as_deref(), "mute_members", Some(salon)),
        peut_couper_son: modere && etat.peut(serveur.as_deref(), "deafen_members", Some(salon)),
        micro_impose: participant.is_some_and(|p| p.micro_coupe),
        son_impose: participant.is_some_and(|p| p.son_coupe),
        destinations,
        peut_deconnecter: modere
            && participant.is_some()
            && etat.peut(serveur.as_deref(), "disconnect_members", None),
        erreur: String::new(),
    })
}

pub fn jauge(db: f32) -> f32 {
    ((db - SEUIL_JAUGE_DB) / -SEUIL_JAUGE_DB).clamp(0.0, 1.0)
}

pub fn parle_micro_coupe(vocal: &EtatVocal, db: f32) -> bool {
    vocal.actif() && vocal.micro_coupe && !vocal.casque_coupe && db > vocal.traitement.seuil as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;
    use estompe_client::store::ParticipantVocal;
    use estompe_client::vocal::InfoParticipant;
    use std::collections::BTreeMap;

    fn connecte(salon: &str) -> EtatVocal {
        let mut v = EtatVocal::default();
        v.rejoindre(salon);
        v.connecte(
            "me".into(),
            vec![
                InfoParticipant {
                    identite: "me".into(),
                    nom: "bob".into(),
                    local: true,
                    ..InfoParticipant::default()
                },
                InfoParticipant {
                    identite: "carol".into(),
                    nom: "carol".into(),
                    attributs: BTreeMap::from([("muted".to_string(), "1".to_string())]),
                    qualite: Qualite::Faible,
                    ..InfoParticipant::default()
                },
            ],
        );
        v
    }

    fn avec_vocal(liste: &[(&str, &str)]) -> Etat {
        let mut e = etat();
        e.vocal.insert(
            "v1".into(),
            liste
                .iter()
                .map(|(u, p)| ParticipantVocal {
                    utilisateur: u.to_string(),
                    pseudo: p.to_string(),
                    ..ParticipantVocal::default()
                })
                .collect(),
        );
        e
    }

    #[test]
    fn la_grille_suit_le_nombre_de_tuiles_comme_le_front() {
        let cas: Vec<_> = [1, 2, 4, 5, 9, 10, 12].iter().map(|n| grille(*n)).collect();
        assert_eq!(
            cas,
            [(1, 1), (2, 1), (2, 2), (3, 2), (3, 3), (4, 3), (4, 3)]
        );
    }

    #[test]
    fn soi_meme_apparait_des_la_connexion_et_l_etat_affiche_vient_des_attributs() {
        let e = avec_vocal(&[("carol", "carol"), ("carol#screen", "carol")]);
        let mut v = connecte("v1");
        v.parlent.insert("carol".into());
        let liste = participants(&e, &v, "v1", "me", true);
        let resume: Vec<_> = liste
            .iter()
            .map(|p| (p.utilisateur.as_str(), p.moi, p.parle, p.muet))
            .collect();
        assert_eq!(
            resume,
            [("me", true, true, false), ("carol", false, true, false)]
        );
        assert_eq!(liste[1].qualite, "Connexion médiocre");
        v.parlent.clear();
        let carol = &participants(&e, &v, "v1", "me", false)[1];
        assert!(
            carol.muet && !carol.parle,
            "micro coupé affiché quand elle ne parle pas"
        );
        let hors = participants(&e, &EtatVocal::default(), "v1", "me", true);
        assert_eq!(hors.len(), 1);
        assert!(!hors[0].parle && !hors[0].muet && hors[0].barres == 0);
    }

    #[test]
    fn micro_coupe_ou_sourdine_ne_montrent_jamais_le_halo() {
        let e = avec_vocal(&[]);
        let mut v = connecte("v1");
        v.basculer_micro();
        let moi = &participants(&e, &v, "v1", "me", true)[0];
        assert!(moi.muet && !moi.parle);
        v.basculer_casque();
        let moi = &participants(&e, &v, "v1", "me", true)[0];
        assert!(moi.sourd && !moi.parle);
    }

    #[test]
    fn les_coupures_du_serveur_viennent_du_store() {
        let mut e = avec_vocal(&[("carol", "carol")]);
        e.vocal.get_mut("v1").unwrap()[0].micro_coupe = true;
        e.vocal.get_mut("v1").unwrap()[0].son_coupe = true;
        let carol = &participants(&e, &EtatVocal::default(), "v1", "me", false)[0];
        assert!(carol.micro_impose && carol.son_impose);
    }

    #[test]
    fn la_vue_annonce_le_compte_l_erreur_et_la_permission() {
        let e = avec_vocal(&[("carol", "carol")]);
        let v = vue(&e, &EtatVocal::default(), "v1", 1);
        assert_eq!(
            (v.nom.as_str(), v.resume.as_str(), v.ici, v.peut_rejoindre),
            ("Salon vocal", "1 / 10 participant", false, true)
        );
        assert_eq!(
            vue(&e, &EtatVocal::default(), "v1", 0).resume,
            "Personne pour le moment."
        );
        let mut refuse = EtatVocal::default();
        refuse.erreur = Some("Salon vocal complet (10 participants maximum).".into());
        assert_eq!(
            vue(&e, &refuse, "v1", 1).erreur,
            "Salon vocal complet (10 participants maximum)."
        );
        let mut ailleurs = connecte("t2");
        ailleurs.erreur = Some("Connexion au vocal perdue.".into());
        assert_eq!(vue(&e, &ailleurs, "v1", 1).erreur, "");
        let mut seul = connecte("v1");
        seul.participants.clear();
        let v = vue(&e, &seul, "v1", 1);
        assert!(v.ici && v.seul && !v.connexion);
    }

    #[test]
    fn la_barre_donne_le_lieu_la_duree_et_la_qualite() {
        let e = avec_vocal(&[]);
        assert!(!barre(&e, &EtatVocal::default(), "me", 0, None).visible);
        let mut v = connecte("v1");
        v.participants.get_mut("me").unwrap().qualite = Qualite::Excellente;
        let b = barre(&e, &v, "me", 125_000, Some(0));
        assert_eq!(
            (
                b.titre.as_str(),
                b.lieu.as_str(),
                b.duree.as_str(),
                b.barres,
                b.serveur.as_str()
            ),
            ("Vocal connecté", "Salon vocal · Les amis", "2:05", 3, "s1")
        );
        let mut c = EtatVocal::default();
        c.rejoindre("v1");
        let b = barre(&e, &c, "me", 0, None);
        assert!(b.connexion && b.duree.is_empty());
        assert_eq!(b.titre, "Connexion…");
    }

    #[test]
    fn le_menu_d_un_participant_suit_le_rang_et_les_permissions() {
        let mut e = avec_vocal(&[("carol", "carol")]);
        e.vocal.get_mut("v1").unwrap()[0].micro_coupe = true;
        let v = connecte("v1");
        assert!(menu(&e, &v, "v1", "me", "me").is_none());
        let m = menu(&e, &v, "v1", "carol", "me").unwrap();
        assert_eq!(
            (m.nom.as_str(), m.volume, m.muet, m.micro_impose),
            ("carol", 100, false, true)
        );
        assert!(
            m.peut_couper_micro && m.peut_couper_son,
            "le créateur du serveur modère"
        );
        assert_eq!(
            m.destinations,
            [("k1".to_string(), "Ciné".to_string())],
            "les autres salons vocaux et cinéma du serveur"
        );
        assert!(m.peut_deconnecter);
        let m = menu(&e, &v, "v1", "carol", "dave").unwrap();
        assert!(!m.peut_couper_micro && !m.peut_couper_son);
        assert!(m.destinations.is_empty() && !m.peut_deconnecter);
    }

    #[test]
    fn la_tuile_porte_la_cle_de_l_avatar_du_serveur_sinon_globale() {
        let mut e = avec_vocal(&[("carol", "carol")]);
        let v = connecte("v1");
        assert_eq!(participants(&e, &v, "v1", "me", false)[1].avatar, None);
        e.avatars.insert("carol".into(), 4);
        assert_eq!(
            participants(&e, &v, "v1", "me", false)[1].avatar,
            Some(CleAvatar {
                utilisateur: "carol".into(),
                version: 4,
                serveur: None,
            })
        );
    }

    #[test]
    fn jauge_de_moins_80_a_0_dbfs_et_parole_micro_coupe() {
        assert_eq!(jauge(-100.0), 0.0);
        assert_eq!(jauge(-40.0), 0.5);
        assert_eq!(jauge(3.0), 1.0);
        let mut v = connecte("v1");
        assert!(!parle_micro_coupe(&v, -20.0));
        v.basculer_micro();
        assert!(parle_micro_coupe(&v, -20.0));
        assert!(!parle_micro_coupe(&v, -60.0));
    }
}
