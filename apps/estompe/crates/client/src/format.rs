use crate::roles::date_en_millisecondes;
use crate::types::Message;

pub const ID_BOT: &str = "00000000-0000-0000-0000-000000000001";

const ECART_MAXIMAL_DE_GROUPE_MS: i64 = 5 * 60_000;

const JOURS: [&str; 7] = [
    "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
];

const MOIS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

pub fn est_bot(id_utilisateur: &str) -> bool {
    id_utilisateur == ID_BOT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateLocale {
    pub annee: i64,
    pub mois: u32,
    pub jour: u32,
    pub heure: u32,
    pub minute: u32,
    pub seconde: u32,
    pub jour_de_semaine: u32,
}

pub fn date_locale_de_ms(ms: i64, decalage_minutes: i32) -> DateLocale {
    let local = ms + i64::from(decalage_minutes) * 60_000;
    let jours = local.div_euclid(86_400_000);
    let dans_le_jour = local.rem_euclid(86_400_000) / 1000;
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let jour = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let annee = yoe + ere * 400 + i64::from(mois <= 2);
    DateLocale {
        annee,
        mois,
        jour,
        heure: (dans_le_jour / 3600) as u32,
        minute: (dans_le_jour % 3600 / 60) as u32,
        seconde: (dans_le_jour % 60) as u32,
        jour_de_semaine: (jours + 4).rem_euclid(7) as u32,
    }
}

pub fn iso_de_ms(ms: i64) -> String {
    let d = date_locale_de_ms(ms, 0);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        d.annee,
        d.mois,
        d.jour,
        d.heure,
        d.minute,
        d.seconde,
        ms.rem_euclid(1000)
    )
}

pub fn date_locale(iso: &str, decalage_minutes: i32) -> Option<DateLocale> {
    date_en_millisecondes(iso).map(|ms| date_locale_de_ms(ms, decalage_minutes))
}

pub fn heure_de(iso: &str, decalage_minutes: i32) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    Some(format!("{:02}:{:02}", d.heure, d.minute))
}

pub fn jour_de(iso: &str, decalage_minutes: i32) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    Some(format!(
        "{} {} {} {}",
        JOURS[d.jour_de_semaine as usize],
        d.jour,
        MOIS[d.mois as usize - 1],
        d.annee
    ))
}

pub fn jour_sans_annee(ms: i64, decalage_minutes: i32) -> String {
    let d = date_locale_de_ms(ms, decalage_minutes);
    format!(
        "{} {} {}",
        JOURS[d.jour_de_semaine as usize],
        d.jour,
        MOIS[d.mois as usize - 1]
    )
}

pub fn cle_de_jour(iso: &str, decalage_minutes: i32) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    Some(format!("{}-{}-{}", d.annee, d.mois - 1, d.jour))
}

pub fn date_heure_complete(iso: &str, decalage_minutes: i32) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    Some(format!(
        "{:02}/{:02}/{} {:02}:{:02}:{:02}",
        d.jour, d.mois, d.annee, d.heure, d.minute, d.seconde
    ))
}

pub fn heure_ou_date_courte(
    iso: &str,
    maintenant_ms: i64,
    decalage_minutes: i32,
) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    let aujourd_hui = date_locale_de_ms(maintenant_ms, decalage_minutes);
    if (d.annee, d.mois, d.jour) == (aujourd_hui.annee, aujourd_hui.mois, aujourd_hui.jour) {
        Some(format!("{:02}:{:02}", d.heure, d.minute))
    } else {
        Some(format!("{:02}/{:02}", d.jour, d.mois))
    }
}

pub fn salutation(maintenant_ms: i64, decalage_minutes: i32) -> &'static str {
    let h = date_locale_de_ms(maintenant_ms, decalage_minutes).heure;
    if !(5..18).contains(&h) {
        "Bonsoir"
    } else {
        "Bonjour"
    }
}

pub fn taille_lisible(octets: u64) -> String {
    let n = u128::from(octets);
    if n < 1024 {
        return format!("{n} o");
    }
    if n < 1024 * 1024 {
        if n < 10 * 1024 {
            let dixiemes = (n * 10 + 512) / 1024;
            return format!("{}.{} Ko", dixiemes / 10, dixiemes % 10);
        }
        return format!("{} Ko", (n + 512) / 1024);
    }
    let dixiemes = (n * 10 + 524_288) / 1_048_576;
    format!("{}.{} Mo", dixiemes / 10, dixiemes % 10)
}

pub fn initiales(pseudo: &str) -> String {
    pseudo.chars().take(2).collect::<String>().to_uppercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reperes<'a> {
    pub auteur: &'a str,
    pub cree_le: &'a str,
}

pub fn reperes(message: &Message) -> Reperes<'_> {
    Reperes {
        auteur: message
            .author
            .as_ref()
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default(),
        cree_le: message.created_at.as_deref().unwrap_or_default(),
    }
}

pub fn prolonge(
    precedent: Option<Reperes<'_>>,
    message: Reperes<'_>,
    decalage_minutes: i32,
) -> bool {
    let Some(precedent) = precedent else {
        return false;
    };
    if precedent.auteur != message.auteur
        || cle_de_jour(precedent.cree_le, decalage_minutes)
            != cle_de_jour(message.cree_le, decalage_minutes)
    {
        return false;
    }
    match (
        date_en_millisecondes(message.cree_le),
        date_en_millisecondes(precedent.cree_le),
    ) {
        (Some(a), Some(b)) => a - b < ECART_MAXIMAL_DE_GROUPE_MS,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ligne {
    Jour {
        cle: String,
        libelle: String,
    },
    Nouveau {
        cle: String,
    },
    Message {
        cle: String,
        index: usize,
        suite: bool,
    },
}

pub fn lignes_de_messages(
    messages: &[Message],
    nouveau_depuis: Option<&str>,
    decalage_minutes: i32,
) -> Vec<Ligne> {
    let mut lignes = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let precedent = i.checked_sub(1).map(|p| &messages[p]);
        let id = m.id.as_deref().unwrap_or_default();
        let courant = reperes(m);
        let nouveau_jour = precedent.is_none_or(|p| {
            cle_de_jour(reperes(p).cree_le, decalage_minutes)
                != cle_de_jour(courant.cree_le, decalage_minutes)
        });
        if nouveau_jour {
            lignes.push(Ligne::Jour {
                cle: format!("day-{id}"),
                libelle: jour_de(courant.cree_le, decalage_minutes).unwrap_or_default(),
            });
        }
        let premier_non_lu = nouveau_depuis == Some(id);
        if premier_non_lu {
            lignes.push(Ligne::Nouveau {
                cle: format!("new-{id}"),
            });
        }
        lignes.push(Ligne::Message {
            cle: id.to_string(),
            index: i,
            suite: !premier_non_lu && prolonge(precedent.map(reperes), courant, decalage_minutes),
        });
    }
    lignes
}

pub fn libelle_statut(statut: &str) -> Option<&'static str> {
    match statut {
        "off" => Some("Hors ligne"),
        "online" => Some("En ligne"),
        "idle" => Some("Absent"),
        "dnd" => Some("Ne pas déranger"),
        _ => None,
    }
}

pub fn pastille(n: u64) -> String {
    if n > 99 {
        "99+".to_string()
    } else {
        n.to_string()
    }
}

pub fn formater_duree(secondes: f64) -> String {
    let s = if secondes.is_nan() {
        0
    } else {
        secondes.max(0.0).floor() as u64
    };
    if s < 60 {
        return format!("{s} s");
    }
    let m = s / 60;
    if m < 60 {
        return format!("{m}:{:02}", s % 60);
    }
    format!("{} h {:02}", m / 60, m % 60)
}

pub fn formater_total_vocal(secondes: f64) -> String {
    let m = if secondes.is_nan() {
        0
    } else {
        (secondes / 60.0).floor().max(0.0) as u64
    };
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {:02}", m / 60, m % 60)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempsVocal {
    pub total: f64,
    pub en_cours: Option<f64>,
}

pub fn temps_vocal(
    total: Option<f64>,
    releve_a_ms: f64,
    depuis_ms: Option<f64>,
    maintenant_ms: f64,
) -> TempsVocal {
    let en_direct = depuis_ms.map_or(0.0, |d| {
        ((maintenant_ms - releve_a_ms.max(d)) / 1000.0).max(0.0)
    });
    TempsVocal {
        total: total.unwrap_or(0.0) + en_direct,
        en_cours: depuis_ms.map(|d| ((maintenant_ms - d) / 1000.0).max(0.0)),
    }
}

pub fn arrivee_la_plus_ancienne(arrivees: &[Option<&str>]) -> Option<i64> {
    arrivees
        .iter()
        .filter_map(|a| date_en_millisecondes((*a)?))
        .min()
}

pub fn jeu_du_replay(nom: &str) -> String {
    const GABARIT: &[u8; 24] = b"_dddd-dd-dd_dd-dd-dd.mp4";
    let octets = nom.as_bytes();
    if octets.len() <= GABARIT.len() {
        return String::new();
    }
    let debut = octets.len() - GABARIT.len();
    if !nom.is_char_boundary(debut) {
        return String::new();
    }
    let (jeu, fin) = nom.split_at(debut);
    let conforme = fin.bytes().zip(GABARIT).all(|(o, &g)| {
        if g == b'd' {
            o.is_ascii_digit()
        } else {
            o.to_ascii_lowercase() == g
        }
    });
    let terminateur = |c: char| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
    if !conforme || jeu.chars().any(terminateur) {
        return String::new();
    }
    jeu.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Person;

    fn message(id: &str, auteur: &str, cree_le: &str) -> Message {
        Message {
            id: Some(id.to_string()),
            author: Some(Person {
                id: Some(auteur.to_string()),
                pseudo: Some(auteur.to_string()),
            }),
            created_at: Some(cree_le.to_string()),
            ..Message::default()
        }
    }

    fn rep<'a>(auteur: &'a str, cree_le: &'a str) -> Reperes<'a> {
        Reperes { auteur, cree_le }
    }

    #[test]
    fn formate_les_tailles() {
        assert_eq!(taille_lisible(12), "12 o");
        assert_eq!(taille_lisible(1536), "1.5 Ko");
        assert_eq!(taille_lisible(50 * 1024), "50 Ko");
        assert_eq!(taille_lisible(3_670_016), "3.5 Mo");
    }

    #[test]
    fn arrondit_les_tailles_comme_tofixed() {
        assert_eq!(taille_lisible(0), "0 o");
        assert_eq!(taille_lisible(1023), "1023 o");
        assert_eq!(taille_lisible(1024), "1.0 Ko");
        assert_eq!(taille_lisible(1280), "1.3 Ko");
        assert_eq!(taille_lisible(10_239), "10.0 Ko");
        assert_eq!(taille_lisible(10_240), "10 Ko");
        assert_eq!(taille_lisible(10_751), "10 Ko");
        assert_eq!(taille_lisible(10_752), "11 Ko");
        assert_eq!(taille_lisible(1_048_575), "1024 Ko");
        assert_eq!(taille_lisible(1_048_576), "1.0 Mo");
        assert_eq!(taille_lisible(10 * 1024 * 1024), "10.0 Mo");
        assert!(taille_lisible(u64::MAX).ends_with(" Mo"));
    }

    #[test]
    fn regroupe_les_messages_proches_du_meme_auteur() {
        let a = rep("u1", "2026-10-13T10:00:00Z");
        assert!(prolonge(Some(a), rep("u1", "2026-10-13T10:04:00Z"), 0));
        assert!(!prolonge(Some(a), rep("u1", "2026-10-13T10:06:00Z"), 0));
        assert!(!prolonge(Some(a), rep("u2", "2026-10-13T10:01:00Z"), 0));
        assert!(!prolonge(None, a, 0));
    }

    #[test]
    fn le_regroupement_s_arrete_a_cinq_minutes_pile_et_au_changement_de_jour() {
        let a = rep("u1", "2026-10-13T10:00:00Z");
        assert!(prolonge(Some(a), rep("u1", "2026-10-13T10:04:59Z"), 0));
        assert!(!prolonge(Some(a), rep("u1", "2026-10-13T10:05:00Z"), 0));
        let tard = rep("u1", "2026-10-13T23:58:00Z");
        assert!(!prolonge(Some(tard), rep("u1", "2026-10-14T00:01:00Z"), 0));
        assert!(prolonge(Some(tard), rep("u1", "2026-10-14T00:01:00Z"), 120));
        let avant_minuit_local = rep("u1", "2026-10-13T21:58:00Z");
        assert!(prolonge(
            Some(avant_minuit_local),
            rep("u1", "2026-10-13T22:01:00Z"),
            0
        ));
        assert!(!prolonge(
            Some(avant_minuit_local),
            rep("u1", "2026-10-13T22:01:00Z"),
            120
        ));
        assert!(!prolonge(Some(a), rep("u1", "pas une date"), 0));
    }

    #[test]
    fn l_heure_suit_le_decalage() {
        assert_eq!(
            heure_de("2026-10-13T10:05:00Z", 0).as_deref(),
            Some("10:05")
        );
        assert_eq!(
            heure_de("2026-10-13T10:05:00Z", 120).as_deref(),
            Some("12:05")
        );
        assert_eq!(
            heure_de("2026-10-13T00:05:00Z", 0).as_deref(),
            Some("00:05")
        );
        assert_eq!(
            heure_de("2026-10-13T00:05:00Z", -300).as_deref(),
            Some("19:05")
        );
        assert_eq!(
            heure_de("2026-10-13T10:05:00+02:00", 0).as_deref(),
            Some("08:05")
        );
        assert_eq!(heure_de("n'importe quoi", 0), None);
    }

    #[test]
    fn le_jour_est_en_toutes_lettres() {
        assert_eq!(
            jour_de("2026-10-13T10:05:00Z", 0).as_deref(),
            Some("mardi 13 octobre 2026")
        );
        assert_eq!(
            jour_de("2026-10-01T12:00:00Z", 0).as_deref(),
            Some("jeudi 1 octobre 2026")
        );
        assert_eq!(
            jour_de("2026-02-28T12:00:00Z", 0).as_deref(),
            Some("samedi 28 février 2026")
        );
        assert_eq!(
            jour_de("2026-12-31T12:00:00Z", 0).as_deref(),
            Some("jeudi 31 décembre 2026")
        );
        assert_eq!(
            jour_de("2027-03-07T12:00:00Z", 0).as_deref(),
            Some("dimanche 7 mars 2027")
        );
        assert_eq!(
            jour_de("2026-10-13T23:30:00Z", 120).as_deref(),
            Some("mercredi 14 octobre 2026")
        );
        assert_eq!(
            jour_de("2024-02-29T12:00:00Z", 0).as_deref(),
            Some("jeudi 29 février 2024")
        );
        assert_eq!(jour_de("rien", 0), None);
    }

    #[test]
    fn la_cle_de_jour_compte_les_mois_depuis_zero() {
        assert_eq!(
            cle_de_jour("2026-10-13T10:00:00Z", 0).as_deref(),
            Some("2026-9-13")
        );
        assert_eq!(
            cle_de_jour("2026-01-05T10:00:00Z", 0).as_deref(),
            Some("2026-0-5")
        );
        assert_eq!(
            cle_de_jour("2026-10-13T23:30:00Z", 120).as_deref(),
            Some("2026-9-14")
        );
        assert_eq!(
            cle_de_jour("2026-10-13T00:30:00Z", -60).as_deref(),
            Some("2026-9-12")
        );
    }

    #[test]
    fn la_date_complete_est_celle_du_navigateur_francais() {
        assert_eq!(
            date_heure_complete("2026-10-13T09:05:07Z", 0).as_deref(),
            Some("13/10/2026 09:05:07")
        );
        assert_eq!(
            date_heure_complete("2026-10-13T09:05:07Z", 60).as_deref(),
            Some("13/10/2026 10:05:07")
        );
    }

    #[test]
    fn l_accueil_affiche_l_heure_aujourd_hui_et_la_date_sinon() {
        let maintenant = date_en_millisecondes("2026-10-13T18:00:00Z").unwrap();
        assert_eq!(
            heure_ou_date_courte("2026-10-13T10:05:00Z", maintenant, 0).as_deref(),
            Some("10:05")
        );
        assert_eq!(
            heure_ou_date_courte("2026-10-12T10:05:00Z", maintenant, 0).as_deref(),
            Some("12/10")
        );
        assert_eq!(
            heure_ou_date_courte("2026-10-13T23:05:00Z", maintenant, 120).as_deref(),
            Some("14/10")
        );
        assert_eq!(heure_ou_date_courte("x", maintenant, 0), None);
    }

    #[test]
    fn le_jour_sans_annee_et_la_salutation() {
        let ms = date_en_millisecondes("2026-10-13T10:05:00Z").unwrap();
        assert_eq!(jour_sans_annee(ms, 0), "mardi 13 octobre");
        assert_eq!(salutation(ms, 0), "Bonjour");
        assert_eq!(salutation(ms, 8 * 60), "Bonsoir");
        let nuit = date_en_millisecondes("2026-10-13T04:59:00Z").unwrap();
        assert_eq!(salutation(nuit, 0), "Bonsoir");
        assert_eq!(salutation(nuit + 60_000, 0), "Bonjour");
    }

    #[test]
    fn les_initiales_sont_les_deux_premiers_caracteres_en_majuscules() {
        assert_eq!(initiales("alice"), "AL");
        assert_eq!(initiales("b"), "B");
        assert_eq!(initiales(""), "");
        assert_eq!(initiales("élodie"), "ÉL");
    }

    #[test]
    fn la_pastille_est_plafonnee() {
        assert_eq!(pastille(0), "0");
        assert_eq!(pastille(99), "99");
        assert_eq!(pastille(100), "99+");
    }

    #[test]
    fn les_statuts_ont_un_libelle() {
        assert_eq!(libelle_statut("off"), Some("Hors ligne"));
        assert_eq!(libelle_statut("online"), Some("En ligne"));
        assert_eq!(libelle_statut("idle"), Some("Absent"));
        assert_eq!(libelle_statut("dnd"), Some("Ne pas déranger"));
        assert_eq!(libelle_statut("autre"), None);
    }

    #[test]
    fn seul_le_compte_systeme_est_un_bot() {
        assert!(est_bot("00000000-0000-0000-0000-000000000001"));
        assert!(!est_bot("00000000-0000-0000-0000-000000000002"));
        assert!(!est_bot(""));
    }

    #[test]
    fn les_durees() {
        assert_eq!(formater_duree(0.0), "0 s");
        assert_eq!(formater_duree(42.9), "42 s");
        assert_eq!(formater_duree(59.0), "59 s");
        assert_eq!(formater_duree(60.0), "1:00");
        assert_eq!(formater_duree(425.0), "7:05");
        assert_eq!(formater_duree(3599.0), "59:59");
        assert_eq!(formater_duree(3600.0), "1 h 00");
        assert_eq!(formater_duree(7500.0), "2 h 05");
        assert_eq!(formater_duree(-5.0), "0 s");
        assert_eq!(formater_duree(f64::NAN), "0 s");
    }

    #[test]
    fn le_total_vocal_est_a_la_minute_pres() {
        assert_eq!(formater_total_vocal(0.0), "0 min");
        assert_eq!(formater_total_vocal(59.0), "0 min");
        assert_eq!(formater_total_vocal(720.0), "12 min");
        assert_eq!(formater_total_vocal(3599.0), "59 min");
        assert_eq!(formater_total_vocal(11_100.0), "3 h 05");
    }

    #[test]
    fn le_temps_vocal_avance_depuis_le_dernier_releve() {
        let t = temps_vocal(Some(100.0), 10_000.0, Some(4_000.0), 20_000.0);
        assert_eq!(
            t,
            TempsVocal {
                total: 110.0,
                en_cours: Some(16.0)
            }
        );
        let t = temps_vocal(Some(100.0), 10_000.0, Some(15_000.0), 20_000.0);
        assert_eq!(
            t,
            TempsVocal {
                total: 105.0,
                en_cours: Some(5.0)
            }
        );
        let t = temps_vocal(None, 10_000.0, None, 20_000.0);
        assert_eq!(
            t,
            TempsVocal {
                total: 0.0,
                en_cours: None
            }
        );
        let t = temps_vocal(Some(7.0), 10_000.0, Some(30_000.0), 20_000.0);
        assert_eq!(
            t,
            TempsVocal {
                total: 7.0,
                en_cours: Some(0.0)
            }
        );
    }

    #[test]
    fn l_arrivee_la_plus_ancienne_ignore_les_heures_inconnues() {
        let t = |s: &str| date_en_millisecondes(s).unwrap();
        assert_eq!(arrivee_la_plus_ancienne(&[]), None);
        assert_eq!(arrivee_la_plus_ancienne(&[None, Some("")]), None);
        assert_eq!(
            arrivee_la_plus_ancienne(&[
                Some("2026-10-13T10:05:00Z"),
                None,
                Some("2026-10-13T10:00:00Z"),
                Some("pas une date"),
            ]),
            Some(t("2026-10-13T10:00:00Z"))
        );
    }

    #[test]
    fn le_jeu_d_un_replay_vient_de_son_nom() {
        assert_eq!(
            jeu_du_replay("Rocket_League_2026-10-13_21-05-09.mp4"),
            "Rocket League"
        );
        assert_eq!(jeu_du_replay("Doom_2026-10-13_21-05-09.MP4"), "Doom");
        assert_eq!(jeu_du_replay("Doom_2026-10-13_21-05-09.webm"), "");
        assert_eq!(jeu_du_replay("_2026-10-13_21-05-09.mp4"), "");
        assert_eq!(jeu_du_replay("vacances.mp4"), "");
        assert_eq!(jeu_du_replay("Doom_2026-10-13_21-05-0.mp4"), "");
        assert_eq!(jeu_du_replay("Dé_2026-10-13_21-05-09.mp4"), "Dé");
        assert_eq!(jeu_du_replay("a\nb_2026-10-13_21-05-09.mp4"), "");
    }

    #[test]
    fn les_lignes_ont_un_separateur_par_jour_et_un_marqueur_de_non_lus() {
        let messages = vec![
            message("1", "u1", "2026-10-13T10:00:00Z"),
            message("2", "u1", "2026-10-13T10:02:00Z"),
            message("3", "u2", "2026-10-13T10:03:00Z"),
            message("4", "u2", "2026-10-14T10:04:00Z"),
            message("5", "u2", "2026-10-14T10:05:00Z"),
        ];
        let lignes = lignes_de_messages(&messages, Some("5"), 0);
        let jour = |cle: &str, libelle: &str| Ligne::Jour {
            cle: cle.into(),
            libelle: libelle.into(),
        };
        let msg = |cle: &str, index: usize, suite: bool| Ligne::Message {
            cle: cle.into(),
            index,
            suite,
        };
        assert_eq!(
            lignes,
            vec![
                jour("day-1", "mardi 13 octobre 2026"),
                msg("1", 0, false),
                msg("2", 1, true),
                msg("3", 2, false),
                jour("day-4", "mercredi 14 octobre 2026"),
                msg("4", 3, false),
                Ligne::Nouveau {
                    cle: "new-5".into()
                },
                msg("5", 4, false),
            ]
        );
        assert_eq!(lignes_de_messages(&[], None, 0), vec![]);
    }

    #[test]
    fn sans_marqueur_un_message_proche_prolonge_le_precedent() {
        let messages = vec![
            message("1", "u1", "2026-10-14T10:04:00Z"),
            message("2", "u1", "2026-10-14T10:05:00Z"),
        ];
        let lignes = lignes_de_messages(&messages, None, 0);
        assert_eq!(lignes.len(), 3);
        assert_eq!(
            lignes[2],
            Ligne::Message {
                cle: "2".into(),
                index: 1,
                suite: true
            }
        );
    }

    #[test]
    fn le_changement_de_jour_suit_le_decalage() {
        let messages = vec![
            message("1", "u1", "2026-10-13T21:58:00Z"),
            message("2", "u1", "2026-10-13T22:01:00Z"),
        ];
        assert_eq!(lignes_de_messages(&messages, None, 0).len(), 3);
        let decale = lignes_de_messages(&messages, None, 120);
        assert_eq!(decale.len(), 4);
        assert!(
            matches!(&decale[2], Ligne::Jour { libelle, .. } if libelle == "mercredi 14 octobre 2026")
        );
        assert!(matches!(&decale[3], Ligne::Message { suite: false, .. }));
    }
}
