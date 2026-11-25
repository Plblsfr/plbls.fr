pub mod appels;
pub mod solo;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::format::{date_locale_de_ms, iso_de_ms};
use crate::regarder_ensemble::libelle_fournisseur;
use crate::roles::date_en_millisecondes;

pub use solo::{ElementSolo, FabriqueSolo, Lancement, ModeSolo, PlaceSolo, Solo, VueSolo};

pub const SEANCE_EN_COURS_MS: i64 = 3 * 3600 * 1000;
pub const TITRE_MAX: usize = 60;
pub const RAPPELS: [(i64, &str); 7] = [
    (0, "Aucun"),
    (5, "5 min avant"),
    (10, "10 min avant"),
    (15, "15 min avant"),
    (30, "30 min avant"),
    (60, "1 h avant"),
    (1440, "1 jour avant"),
];

const JOURS_COURTS: [&str; 7] = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."];
const MOIS_COURTS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuteurVideo {
    pub id: String,
    pub pseudo: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostVideo {
    pub message_id: String,
    pub author: AuteurVideo,
    pub created_at: String,
    pub provider: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub url: String,
    pub title: String,
    pub site: String,
    pub has_thumb: bool,
    pub watched_at: Option<String>,
    pub reactions: i64,
    pub liked: bool,
    pub replies: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListeVideos {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub collaborative: bool,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rsvp {
    #[serde(rename = "yes")]
    Oui,
    #[serde(rename = "maybe")]
    PeutEtre,
    #[serde(rename = "no")]
    Non,
}

impl Rsvp {
    pub const TOUS: [Rsvp; 3] = [Rsvp::Oui, Rsvp::PeutEtre, Rsvp::Non];

    pub fn libelle(self) -> &'static str {
        match self {
            Rsvp::Oui => "Je viens",
            Rsvp::PeutEtre => "Peut-être",
            Rsvp::Non => "Non",
        }
    }

    pub fn groupe(self) -> &'static str {
        match self {
            Rsvp::Oui => "Viennent",
            Rsvp::PeutEtre => "Peut-être",
            Rsvp::Non => "Absents",
        }
    }

    pub fn cle(self) -> &'static str {
        match self {
            Rsvp::Oui => "yes",
            Rsvp::PeutEtre => "maybe",
            Rsvp::Non => "no",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeanceVideos {
    pub id: String,
    pub title: String,
    pub starts_at: String,
    pub voice_channel_id: String,
    pub remind_minutes: i64,
    pub videos: Vec<String>,
    pub created_by: String,
    pub started_at: Option<String>,
    pub rsvps: BTreeMap<String, Rsvp>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableauVideos {
    pub posts: Vec<PostVideo>,
    pub lists: Vec<ListeVideos>,
    pub sessions: Vec<SeanceVideos>,
}

pub fn jouable(provider: &str) -> bool {
    provider != "simple"
}

pub fn musique(provider: &str) -> bool {
    provider == "ytmusic"
}

fn ressemble_a_un_lien(texte: &str) -> bool {
    let t = texte.trim_start().to_ascii_lowercase();
    t.starts_with("http://") || t.starts_with("https://")
}

pub fn titre_video(p: &PostVideo) -> String {
    if !p.title.is_empty() && p.title != p.url && !ressemble_a_un_lien(&p.title) {
        return p.title.clone();
    }
    if p.provider == "simple" {
        return "Lien vidéo".to_string();
    }
    match libelle_fournisseur(&p.provider) {
        l if l == p.provider => "Vidéo web".to_string(),
        l => format!("Vidéo {l}"),
    }
}

pub fn lien_lecture(p: &PostVideo) -> &str {
    if p.provider == "page" {
        &p.reference
    } else {
        &p.url
    }
}

pub fn source_de(provider: &str) -> &str {
    if provider.starts_with("twitch") {
        "twitch"
    } else if provider == "page" {
        "file"
    } else {
        provider
    }
}

pub fn libelle_source(source: &str) -> &str {
    match source {
        "twitch" => "Twitch",
        autre => libelle_fournisseur(autre),
    }
}

pub fn libelle_etiquette(provider: &str) -> &str {
    match libelle_fournisseur(provider) {
        l if l == provider => "Lien",
        l => l,
    }
}

pub fn artiste_de(site: &str) -> &str {
    let titres = site
        .strip_suffix(" titres")
        .or_else(|| site.strip_suffix(" titre"));
    if titres.is_some_and(|n| !n.is_empty() && n.bytes().all(|o| o.is_ascii_digit())) {
        return "";
    }
    site.split(" · ").next().unwrap_or_default()
}

pub fn est_morceau(reference: &str) -> bool {
    reference.len() == 11
        && reference
            .bytes()
            .all(|o| o.is_ascii_alphanumeric() || o == b'_' || o == b'-')
}

pub fn lien_musique(reference: &str) -> String {
    if est_morceau(reference) {
        format!("https://music.youtube.com/watch?v={reference}")
    } else {
        format!("https://music.youtube.com/playlist?list={reference}")
    }
}

pub fn melange<T: Clone>(a: &[T], mut hasard: impl FnMut() -> f64) -> Vec<T> {
    let mut sortie = a.to_vec();
    for i in (1..sortie.len()).rev() {
        let j = ((hasard() * (i + 1) as f64).floor() as usize).min(i);
        sortie.swap(i, j);
    }
    sortie
}

pub fn hasard() -> f64 {
    let mut octets = [0u8; 8];
    let _ = getrandom::fill(&mut octets);
    (u64::from_le_bytes(octets) >> 11) as f64 / (1u64 << 53) as f64
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FiltreVues {
    #[default]
    Toutes,
    Vues,
    NonVues,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filtre {
    pub source: String,
    pub vues: FiltreVues,
    pub auteur: String,
    pub liste: String,
    pub artiste: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tri {
    #[default]
    Recentes,
    Reactions,
    Titre,
}

fn replier(c: char) -> &'static str {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => "a",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'î' | 'ï' | 'í' | 'ì' => "i",
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' => "o",
        'ù' | 'û' | 'ü' | 'ú' => "u",
        'ç' => "c",
        'ÿ' => "y",
        'ñ' => "n",
        'œ' => "oe",
        'æ' => "ae",
        _ => "",
    }
}

pub fn cle_alphabetique(texte: &str) -> String {
    texte
        .to_lowercase()
        .chars()
        .map(|c| match replier(c) {
            "" => c.to_string(),
            r => r.to_string(),
        })
        .collect()
}

fn numero(id: &str) -> f64 {
    id.parse::<f64>().unwrap_or(0.0)
}

fn par_recence(a: &PostVideo, b: &PostVideo) -> std::cmp::Ordering {
    numero(&b.message_id).total_cmp(&numero(&a.message_id))
}

pub fn filtrer<'a>(
    posts: &'a [PostVideo],
    f: &Filtre,
    tri: Tri,
    listes: &[ListeVideos],
) -> Vec<&'a PostVideo> {
    let dans_liste: Option<HashSet<&str>> = (!f.liste.is_empty()).then(|| {
        listes
            .iter()
            .find(|l| l.id == f.liste)
            .map(|l| l.items.iter().map(String::as_str).collect())
            .unwrap_or_default()
    });
    let mut sortie: Vec<&PostVideo> = posts
        .iter()
        .filter(|p| f.source.is_empty() || source_de(&p.provider) == f.source)
        .filter(|p| match f.vues {
            FiltreVues::Toutes => true,
            FiltreVues::Vues => p.watched_at.is_some(),
            FiltreVues::NonVues => p.watched_at.is_none(),
        })
        .filter(|p| f.auteur.is_empty() || p.author.id == f.auteur)
        .filter(|p| f.artiste.is_empty() || artiste_de(&p.site) == f.artiste)
        .filter(|p| {
            dans_liste
                .as_ref()
                .is_none_or(|l| l.contains(p.message_id.as_str()))
        })
        .collect();
    match tri {
        Tri::Recentes => sortie.sort_by(|a, b| par_recence(a, b)),
        Tri::Reactions => {
            sortie.sort_by(|a, b| b.reactions.cmp(&a.reactions).then(par_recence(a, b)))
        }
        Tri::Titre => sortie.sort_by(|a, b| {
            cle_alphabetique(&titre_video(a))
                .cmp(&cle_alphabetique(&titre_video(b)))
                .then(par_recence(a, b))
        }),
    }
    sortie
}

pub fn elements_de_liste<'a>(l: &ListeVideos, posts: &'a [PostVideo]) -> Vec<&'a PostVideo> {
    let par_id: HashMap<&str, &PostVideo> =
        posts.iter().map(|p| (p.message_id.as_str(), p)).collect();
    l.items
        .iter()
        .filter_map(|id| par_id.get(id.as_str()).copied())
        .collect()
}

pub fn deplace<T: Clone>(a: &[T], i: usize, d: i64) -> Vec<T> {
    let j = i as i64 + d;
    if j < 0 || j >= a.len() as i64 || i >= a.len() {
        return a.to_vec();
    }
    let mut sortie = a.to_vec();
    sortie.swap(i, j as usize);
    sortie
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComptesRsvp {
    pub oui: usize,
    pub peut_etre: usize,
    pub non: usize,
}

impl ComptesRsvp {
    pub fn de(&self, r: Rsvp) -> usize {
        match r {
            Rsvp::Oui => self.oui,
            Rsvp::PeutEtre => self.peut_etre,
            Rsvp::Non => self.non,
        }
    }
}

pub fn compte_rsvp(s: &SeanceVideos) -> ComptesRsvp {
    let mut c = ComptesRsvp::default();
    for r in s.rsvps.values() {
        match r {
            Rsvp::Oui => c.oui += 1,
            Rsvp::PeutEtre => c.peut_etre += 1,
            Rsvp::Non => c.non += 1,
        }
    }
    c
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseSeance {
    AVenir,
    EnCours,
}

pub fn phase_seance(s: &SeanceVideos, maintenant_ms: i64) -> PhaseSeance {
    let commencee = s.started_at.as_deref().is_some_and(|d| !d.is_empty());
    if commencee || date_en_millisecondes(&s.starts_at).is_some_and(|t| t <= maintenant_ms) {
        PhaseSeance::EnCours
    } else {
        PhaseSeance::AVenir
    }
}

pub fn seances_proches(seances: &[SeanceVideos], maintenant_ms: i64) -> Vec<&SeanceVideos> {
    seances
        .iter()
        .filter(|s| {
            phase_seance(s, maintenant_ms) == PhaseSeance::AVenir
                || (s.started_at.is_some()
                    && date_en_millisecondes(&s.starts_at)
                        .is_some_and(|t| maintenant_ms - t < SEANCE_EN_COURS_MS))
        })
        .take(2)
        .collect()
}

fn jour_local(ms: i64, decalage_minutes: i32) -> i64 {
    (ms + i64::from(decalage_minutes) * 60_000).div_euclid(86_400_000)
}

pub fn echeance(iso: &str, maintenant_ms: i64, decalage_minutes: i32) -> String {
    let Some(t) = date_en_millisecondes(iso) else {
        return String::new();
    };
    let minutes = ((t - maintenant_ms) as f64 / 60_000.0).round() as i64;
    if minutes <= 0 {
        return "maintenant".to_string();
    }
    if minutes < 60 {
        return format!("dans {minutes} min");
    }
    let d = date_locale_de_ms(t, decalage_minutes);
    let hm = format!("{:02}:{:02}", d.heure, d.minute);
    match jour_local(t, decalage_minutes) - jour_local(maintenant_ms, decalage_minutes) {
        0 => format!("aujourd’hui {hm}"),
        1 => format!("demain {hm}"),
        _ => format!(
            "{} {} {} {hm}",
            JOURS_COURTS[d.jour_de_semaine as usize],
            d.jour,
            MOIS_COURTS[d.mois as usize - 1]
        ),
    }
}

pub fn date_longue(iso: &str, decalage_minutes: i32) -> String {
    let Some(t) = date_en_millisecondes(iso) else {
        return String::new();
    };
    let d = date_locale_de_ms(t, decalage_minutes);
    format!(
        "{} {} {} à {:02}:{:02}",
        JOURS[d.jour_de_semaine as usize],
        d.jour,
        MOIS[d.mois as usize - 1],
        d.heure,
        d.minute
    )
}

pub fn pastille_date(iso: &str, decalage_minutes: i32) -> (String, String) {
    let Some(t) = date_en_millisecondes(iso) else {
        return (String::new(), String::new());
    };
    let d = date_locale_de_ms(t, decalage_minutes);
    (
        MOIS_COURTS[d.mois as usize - 1]
            .trim_end_matches('.')
            .to_uppercase(),
        format!("{:02}", d.jour),
    )
}

pub fn valeur_locale(ms: i64, decalage_minutes: i32) -> String {
    let d = date_locale_de_ms(ms, decalage_minutes);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        d.annee, d.mois, d.jour, d.heure, d.minute
    )
}

pub fn date_du_formulaire(ms: i64, decalage_minutes: i32) -> String {
    let d = date_locale_de_ms(ms, decalage_minutes);
    format!(
        "{:02}/{:02}/{:04} {:02}:{:02}",
        d.jour, d.mois, d.annee, d.heure, d.minute
    )
}

pub fn lire_date_du_formulaire(texte: &str, decalage_minutes: i32) -> Option<i64> {
    let t = texte.trim();
    let (jour, heure) = t.split_once([' ', 'T'])?;
    let heure = heure.trim();
    let jour = match jour.split('/').collect::<Vec<_>>().as_slice() {
        [j, m, a] => format!("{a:0>4}-{m:0>2}-{j:0>2}"),
        _ => jour.to_string(),
    };
    let (h, m) = heure.split_once([':', 'h'])?;
    let iso = format!("{jour}T{h:0>2}:{m:0>2}Z");
    date_en_millisecondes(&iso).map(|ms| ms - i64::from(decalage_minutes) * 60_000)
}

pub fn libelle_rappel(minutes: i64) -> String {
    RAPPELS
        .iter()
        .find(|(m, _)| *m == minutes)
        .map_or_else(|| format!("{minutes} min avant"), |(_, l)| l.to_string())
}

pub fn detail_seance(s: &SeanceVideos, salon: &str, decalage_minutes: i32) -> String {
    let n = s.videos.len();
    let mut d = format!(
        "{} · {} · {n} vidéo{}",
        date_longue(&s.starts_at, decalage_minutes),
        if salon.is_empty() {
            "salon vocal"
        } else {
            salon
        },
        if n > 1 { "s" } else { "" }
    );
    if s.remind_minutes > 0 {
        d.push_str(&format!(" · rappel {} min avant", s.remind_minutes));
    }
    d
}

pub fn statut_seance(s: &SeanceVideos, maintenant_ms: i64, decalage_minutes: i32) -> String {
    match phase_seance(s, maintenant_ms) {
        PhaseSeance::EnCours if s.started_at.is_some() => "En cours".to_string(),
        PhaseSeance::EnCours => "À l’heure".to_string(),
        PhaseSeance::AVenir => echeance(&s.starts_at, maintenant_ms, decalage_minutes),
    }
}

pub fn reponses_par_groupe(
    s: &SeanceVideos,
    nom: impl Fn(&str) -> String,
) -> Vec<(Rsvp, Vec<String>)> {
    Rsvp::TOUS
        .into_iter()
        .filter_map(|r| {
            let noms: Vec<String> = s
                .rsvps
                .iter()
                .filter(|(_, v)| **v == r)
                .map(|(u, _)| nom(u))
                .collect();
            (!noms.is_empty()).then_some((r, noms))
        })
        .collect()
}

pub fn texte_elements(n: usize) -> String {
    format!("{n} élément{}", if n > 1 { "s" } else { "" })
}

pub fn texte_a_suivre(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("À suivre : {n} titre{}", if n > 1 { "s" } else { "" })
    }
}

pub fn texte_vide(musique: bool, aucun_post: bool) -> &'static str {
    match (musique, aucun_post) {
        (true, true) => {
            "Aucun morceau pour le moment. Colle un lien YouTube Music ci-dessus pour lancer le salon."
        }
        (false, true) => {
            "Aucune vidéo pour le moment. Colle un lien ci-dessus pour lancer le salon."
        }
        (true, false) => "Aucun morceau ne correspond aux filtres.",
        (false, false) => "Aucune vidéo ne correspond aux filtres.",
    }
}

pub fn horloge(secondes: f64) -> String {
    let s = if secondes.is_finite() {
        secondes.max(0.0).floor() as u64
    } else {
        0
    };
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn textes_temps(position: f64, duree: f64) -> (String, String, String) {
    let shown = if duree > 0.0 {
        position.min(duree)
    } else {
        position
    };
    (
        horloge(shown),
        horloge(duree),
        format!("-{}", horloge((duree - shown).max(0.0))),
    )
}

pub fn appliquer_reaction(
    tableaux: &mut BTreeMap<String, TableauVideos>,
    message: &str,
    emoji: &str,
    ajoutee: bool,
    mienne: bool,
) -> Vec<String> {
    let mut salons = Vec::new();
    for (salon, t) in tableaux.iter_mut() {
        let Some(p) = t.posts.iter_mut().find(|p| p.message_id == message) else {
            continue;
        };
        p.reactions = (p.reactions + if ajoutee { 1 } else { -1 }).max(0);
        if mienne && emoji == "👍" {
            p.liked = ajoutee;
        }
        salons.push(salon.clone());
    }
    salons
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSeance {
    Liste(String),
    Videos(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NouvelleSeance {
    pub title: String,
    pub starts_at: String,
    pub voice_channel_id: String,
    pub remind_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_ids: Option<Vec<String>>,
}

pub fn nouvelle_seance(
    titre: &str,
    debut_ms: Option<i64>,
    salon_vocal: &str,
    rappel: i64,
    file: &FileSeance,
) -> Option<NouvelleSeance> {
    let titre: String = titre.trim().chars().take(TITRE_MAX).collect();
    let debut = debut_ms?;
    if titre.is_empty() || salon_vocal.is_empty() {
        return None;
    }
    let (list_id, message_ids) = match file {
        FileSeance::Liste(l) if !l.is_empty() => (Some(l.clone()), None),
        FileSeance::Videos(v) if !v.is_empty() => (None, Some(v.clone())),
        _ => return None,
    };
    Some(NouvelleSeance {
        title: titre,
        starts_at: iso_de_ms(debut),
        voice_channel_id: salon_vocal.to_string(),
        remind_minutes: rappel,
        list_id,
        message_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: &str, provider: &str) -> PostVideo {
        PostVideo {
            message_id: id.into(),
            author: AuteurVideo {
                id: "a".into(),
                pseudo: "a".into(),
            },
            created_at: "2026-10-28T10:00:00Z".into(),
            provider: provider.into(),
            reference: "x".into(),
            url: "https://youtu.be/x".into(),
            ..PostVideo::default()
        }
    }

    fn posts() -> Vec<PostVideo> {
        vec![
            PostVideo {
                reactions: 1,
                ..post("10", "youtube")
            },
            PostVideo {
                author: AuteurVideo {
                    id: "b".into(),
                    pseudo: "b".into(),
                },
                reactions: 5,
                watched_at: Some("2026-10-28T11:00:00Z".into()),
                ..post("9", "twitch_vod")
            },
            PostVideo {
                reference: "https://x.fr/a.mp4".into(),
                url: "https://x.fr/page".into(),
                ..post("100", "page")
            },
            PostVideo {
                reactions: 5,
                ..post("8", "simple")
            },
        ]
    }

    fn ids(l: &[&PostVideo]) -> Vec<String> {
        l.iter().map(|p| p.message_id.clone()).collect()
    }

    #[test]
    fn trie_par_recence_numerique_puis_par_reactions() {
        let p = posts();
        let f = Filtre::default();
        assert_eq!(
            ids(&filtrer(&p, &f, Tri::Recentes, &[])),
            ["100", "10", "9", "8"]
        );
        assert_eq!(
            ids(&filtrer(&p, &f, Tri::Reactions, &[])),
            ["9", "8", "10", "100"]
        );
    }

    #[test]
    fn filtre_par_source_vue_auteur_et_liste() {
        let p = posts();
        let avec = |f: Filtre| ids(&filtrer(&p, &f, Tri::Recentes, &[]));
        assert_eq!(
            avec(Filtre {
                source: "twitch".into(),
                ..Filtre::default()
            }),
            ["9"]
        );
        assert_eq!(
            avec(Filtre {
                source: "file".into(),
                ..Filtre::default()
            }),
            ["100"]
        );
        assert_eq!(
            avec(Filtre {
                vues: FiltreVues::Vues,
                ..Filtre::default()
            }),
            ["9"]
        );
        assert_eq!(
            avec(Filtre {
                vues: FiltreVues::NonVues,
                ..Filtre::default()
            }),
            ["100", "10", "8"]
        );
        assert_eq!(
            avec(Filtre {
                auteur: "b".into(),
                ..Filtre::default()
            }),
            ["9"]
        );
        let listes = [ListeVideos {
            id: "l1".into(),
            name: "x".into(),
            created_by: "a".into(),
            collaborative: true,
            items: vec!["8".into(), "10".into()],
        }];
        let f = Filtre {
            liste: "l1".into(),
            ..Filtre::default()
        };
        assert_eq!(ids(&filtrer(&p, &f, Tri::Recentes, &listes)), ["10", "8"]);
    }

    #[test]
    fn regroupe_les_sources_et_choisit_le_lien_a_lire() {
        let p = posts();
        assert_eq!(source_de("twitch_clip"), "twitch");
        assert_eq!(source_de("vimeo"), "vimeo");
        assert_eq!(lien_lecture(&p[2]), "https://x.fr/a.mp4");
        assert_eq!(lien_lecture(&p[0]), "https://youtu.be/x");
        assert_eq!(libelle_source("twitch"), "Twitch");
        assert_eq!(libelle_source("file"), "Fichier");
        assert_eq!(libelle_etiquette("simple"), "Lien");
        assert_eq!(libelle_etiquette("inconnu"), "Lien");
    }

    #[test]
    fn une_liste_suit_son_ordre_et_ignore_les_cartes_supprimees() {
        let p = posts();
        let l = ListeVideos {
            id: "l".into(),
            name: "x".into(),
            created_by: "a".into(),
            collaborative: true,
            items: vec!["8".into(), "404".into(), "10".into()],
        };
        assert_eq!(ids(&elements_de_liste(&l, &p)), ["8", "10"]);
    }

    #[test]
    fn deplace_un_element_sans_sortir_des_bornes() {
        let a = ["a", "b", "c"];
        assert_eq!(deplace(&a, 1, -1), ["b", "a", "c"]);
        assert_eq!(deplace(&a, 2, 1), ["a", "b", "c"]);
        assert_eq!(deplace(&a, 0, 1), ["b", "a", "c"]);
        assert_eq!(deplace(&a, 0, -1), ["a", "b", "c"]);
    }

    #[test]
    fn compte_les_reponses() {
        let s = SeanceVideos {
            rsvps: [
                ("a", Rsvp::Oui),
                ("b", Rsvp::PeutEtre),
                ("c", Rsvp::Oui),
                ("d", Rsvp::Non),
            ]
            .into_iter()
            .map(|(u, r)| (u.to_string(), r))
            .collect(),
            ..SeanceVideos::default()
        };
        assert_eq!(
            compte_rsvp(&s),
            ComptesRsvp {
                oui: 2,
                peut_etre: 1,
                non: 1
            }
        );
        let groupes = reponses_par_groupe(&s, |u| u.to_uppercase());
        assert_eq!(groupes[0], (Rsvp::Oui, vec!["A".to_string(), "C".into()]));
        assert_eq!(groupes.len(), 3);
    }

    fn seance(debut: &str, commencee: Option<&str>) -> SeanceVideos {
        SeanceVideos {
            starts_at: debut.into(),
            started_at: commencee.map(str::to_string),
            ..SeanceVideos::default()
        }
    }

    #[test]
    fn distingue_seance_a_venir_et_en_cours() {
        let maintenant = date_en_millisecondes("2026-10-28T10:00:00Z").unwrap();
        assert_eq!(
            phase_seance(&seance("2026-10-28T10:05:00Z", None), maintenant),
            PhaseSeance::AVenir
        );
        assert_eq!(
            phase_seance(&seance("2026-10-28T09:59:00Z", None), maintenant),
            PhaseSeance::EnCours
        );
        assert_eq!(
            phase_seance(
                &seance("2026-10-28T10:05:00Z", Some("2026-10-28T10:00:00Z")),
                maintenant
            ),
            PhaseSeance::EnCours
        );
        let toutes = [
            seance("2026-10-28T06:00:00Z", Some("2026-10-28T06:00:00Z")),
            seance("2026-10-28T09:00:00Z", Some("2026-10-28T09:00:00Z")),
            seance("2026-10-28T09:30:00Z", None),
            seance("2026-10-28T11:00:00Z", None),
            seance("2026-10-29T11:00:00Z", None),
        ];
        let proches = seances_proches(&toutes, maintenant);
        assert_eq!(
            proches
                .iter()
                .map(|s| s.starts_at.as_str())
                .collect::<Vec<_>>(),
            ["2026-10-28T09:00:00Z", "2026-10-28T11:00:00Z"]
        );
    }

    #[test]
    fn decrit_l_echeance() {
        let maintenant = date_en_millisecondes("2026-10-28T10:00:00Z").unwrap();
        assert_eq!(
            echeance("2026-10-28T10:12:00Z", maintenant, 0),
            "dans 12 min"
        );
        assert_eq!(
            echeance("2026-10-28T09:00:00Z", maintenant, 0),
            "maintenant"
        );
        assert!(echeance("2026-10-29T10:00:00Z", maintenant, 60).starts_with("demain "));
        assert_eq!(
            echeance("2026-10-28T19:30:00Z", maintenant, 60),
            "aujourd’hui 20:30"
        );
        assert_eq!(
            echeance("2026-10-30T19:30:00Z", maintenant, 60),
            "ven. 30 oct. 20:30"
        );
        assert_eq!(echeance("pas une date", maintenant, 0), "");
        assert_eq!(
            statut_seance(&seance("2026-10-28T10:12:00Z", None), maintenant, 0),
            "dans 12 min"
        );
        assert_eq!(
            statut_seance(
                &seance("2026-10-28T09:00:00Z", Some("2026-10-28T09:00:00Z")),
                maintenant,
                0
            ),
            "En cours"
        );
    }

    #[test]
    fn formate_une_valeur_de_date_en_heure_locale_et_la_relit() {
        let ms = date_en_millisecondes("2026-10-28T19:30:00Z").unwrap();
        assert_eq!(valeur_locale(ms, 60), "2026-10-28T20:30");
        assert_eq!(date_du_formulaire(ms, 60), "28/10/2026 20:30");
        assert_eq!(lire_date_du_formulaire("28/10/2026 20:30", 60), Some(ms));
        assert_eq!(lire_date_du_formulaire("2026-10-28T20:30", 60), Some(ms));
        assert_eq!(lire_date_du_formulaire("2026-10-28 20h30", 60), Some(ms));
        assert_eq!(lire_date_du_formulaire("demain", 60), None);
        assert_eq!(
            pastille_date("2026-10-28T19:30:00Z", 60),
            ("OCT".into(), "28".into())
        );
        assert_eq!(
            date_longue("2026-10-28T19:30:00Z", 60),
            "mercredi 28 octobre à 20:30"
        );
    }

    #[test]
    fn ne_montre_jamais_le_lien_brut() {
        let base = PostVideo {
            provider: "youtube".into(),
            url: "https://youtu.be/dQw4w9WgXcQ".into(),
            ..PostVideo::default()
        };
        assert_eq!(
            titre_video(&PostVideo {
                title: "Ma vidéo".into(),
                ..base.clone()
            }),
            "Ma vidéo"
        );
        assert_eq!(titre_video(&base), "Vidéo YouTube");
        assert_eq!(
            titre_video(&PostVideo {
                title: base.url.clone(),
                ..base.clone()
            }),
            "Vidéo YouTube"
        );
        assert_eq!(
            titre_video(&PostVideo {
                provider: "simple".into(),
                url: "https://x.fr/a".into(),
                ..PostVideo::default()
            }),
            "Lien vidéo"
        );
    }

    fn morceau(id: &str, site: &str, titre: &str) -> PostVideo {
        PostVideo {
            site: site.into(),
            title: titre.into(),
            reference: "dQw4w9WgXcQ".into(),
            ..post(id, "ytmusic")
        }
    }

    #[test]
    fn lit_l_artiste_de_la_carte_pas_le_nombre_de_titres_d_une_playlist() {
        assert_eq!(artiste_de("Daft Punk · Discovery · 3:25"), "Daft Punk");
        assert_eq!(artiste_de("12 titres"), "");
        assert_eq!(artiste_de("1 titre"), "");
        assert_eq!(artiste_de(""), "");
    }

    #[test]
    fn filtre_par_artiste_et_trie_par_titre() {
        let l = [
            morceau("1", "B · x", "Zèbre"),
            morceau("2", "A · x", "Abeille"),
            morceau("3", "A · y", "Moto"),
            morceau("4", "C · z", "écureuil"),
        ];
        let f = Filtre {
            artiste: "A".into(),
            ..Filtre::default()
        };
        assert_eq!(ids(&filtrer(&l, &f, Tri::Recentes, &[])), ["3", "2"]);
        assert_eq!(
            filtrer(&l, &Filtre::default(), Tri::Titre, &[])
                .iter()
                .map(|p| p.title.as_str())
                .collect::<Vec<_>>(),
            ["Abeille", "écureuil", "Moto", "Zèbre"]
        );
    }

    #[test]
    fn distingue_un_morceau_d_une_playlist_dans_le_lien_a_lire() {
        assert!(est_morceau("dQw4w9WgXcQ"));
        assert!(!est_morceau("OLAK5uy_abcdefghij"));
        assert_eq!(
            lien_musique("dQw4w9WgXcQ"),
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            lien_musique("OLAK5uy_abcdefghij"),
            "https://music.youtube.com/playlist?list=OLAK5uy_abcdefghij"
        );
    }

    #[test]
    fn melange_sans_perdre_ni_dupliquer_de_morceau() {
        let a = [1, 2, 3, 4, 5, 6];
        let mut s = melange(&a, || 0.0);
        assert_ne!(s, a);
        s.sort();
        assert_eq!(s, a);
        assert_eq!(a, [1, 2, 3, 4, 5, 6]);
        let mut t = melange(&a, hasard);
        t.sort();
        assert_eq!(t, a);
        assert!((0.0..1.0).contains(&hasard()));
    }

    #[test]
    fn la_barre_des_mini_lecteurs_affiche_position_duree_et_reste() {
        assert_eq!(horloge(0.0), "0:00");
        assert_eq!(horloge(65.9), "1:05");
        assert_eq!(horloge(3725.0), "1:02:05");
        assert_eq!(horloge(f64::NAN), "0:00");
        assert_eq!(
            textes_temps(42.0, 205.0),
            ("0:42".to_string(), "3:25".to_string(), "-2:43".to_string())
        );
        assert_eq!(textes_temps(300.0, 205.0).0, "3:25");
    }

    #[test]
    fn une_reaction_recue_met_a_jour_le_compte_et_mon_pouce() {
        let mut tableaux = BTreeMap::from([(
            "w1".to_string(),
            TableauVideos {
                posts: posts(),
                ..TableauVideos::default()
            },
        )]);
        assert_eq!(
            appliquer_reaction(&mut tableaux, "10", "👍", true, true),
            ["w1"]
        );
        let p = &tableaux["w1"].posts[0];
        assert_eq!((p.reactions, p.liked), (2, true));
        appliquer_reaction(&mut tableaux, "10", "🎉", false, false);
        appliquer_reaction(&mut tableaux, "10", "🎉", false, false);
        appliquer_reaction(&mut tableaux, "10", "🎉", false, false);
        let p = &tableaux["w1"].posts[0];
        assert_eq!((p.reactions, p.liked), (0, true));
        assert!(appliquer_reaction(&mut tableaux, "404", "👍", true, true).is_empty());
    }

    #[test]
    fn une_seance_ne_se_planifie_qu_avec_un_titre_une_date_un_salon_et_une_file() {
        let debut = date_en_millisecondes("2026-10-28T19:30:00Z");
        let liste = FileSeance::Liste("l1".into());
        assert_eq!(
            nouvelle_seance(" Soirée ", debut, "v1", 10, &liste),
            Some(NouvelleSeance {
                title: "Soirée".into(),
                starts_at: "2026-10-28T19:30:00.000Z".into(),
                voice_channel_id: "v1".into(),
                remind_minutes: 10,
                list_id: Some("l1".into()),
                message_ids: None,
            })
        );
        let choisies = FileSeance::Videos(vec!["8".into()]);
        let s = nouvelle_seance("Soirée", debut, "v1", 0, &choisies).unwrap();
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({"title": "Soirée", "starts_at": "2026-10-28T19:30:00.000Z", "voice_channel_id": "v1", "remind_minutes": 0, "message_ids": ["8"]})
        );
        assert!(nouvelle_seance("", debut, "v1", 0, &liste).is_none());
        assert!(nouvelle_seance("Soirée", None, "v1", 0, &liste).is_none());
        assert!(nouvelle_seance("Soirée", debut, "", 0, &liste).is_none());
        assert!(nouvelle_seance("Soirée", debut, "v1", 0, &FileSeance::Videos(vec![])).is_none());
        assert_eq!(libelle_rappel(60), "1 h avant");
        assert_eq!(libelle_rappel(7), "7 min avant");
    }

    #[test]
    fn le_tableau_du_serveur_se_lit() {
        let t: TableauVideos = serde_json::from_value(serde_json::json!({
            "posts": [{"message_id": "10", "author": {"id": "a", "pseudo": "alice"}, "created_at": "2026-10-28T10:00:00Z", "provider": "youtube", "ref": "dQw4w9WgXcQ", "url": "https://youtu.be/dQw4w9WgXcQ", "title": "T", "site": "S", "has_thumb": true, "watched_at": null, "reactions": 2, "liked": true, "replies": 1}],
            "lists": [{"id": "l1", "name": "À voir", "created_by": "a", "collaborative": false, "items": ["10"]}],
            "sessions": [{"id": "s1", "title": "Soirée", "starts_at": "2026-10-28T19:30:00Z", "voice_channel_id": "v1", "remind_minutes": 10, "videos": ["10"], "created_by": "a", "started_at": null, "rsvps": {"a": "yes", "b": "maybe"}}]
        }))
        .unwrap();
        assert_eq!(t.posts[0].reference, "dQw4w9WgXcQ");
        assert!(t.posts[0].has_thumb && t.posts[0].liked);
        assert!(!t.lists[0].collaborative);
        assert_eq!(t.sessions[0].rsvps["b"], Rsvp::PeutEtre);
        assert_eq!(texte_elements(1), "1 élément");
        assert_eq!(texte_a_suivre(2), "À suivre : 2 titres");
        assert_eq!(texte_a_suivre(0), "");
        assert_eq!(
            detail_seance(&t.sessions[0], "Salon vocal", 60),
            "mercredi 28 octobre à 20:30 · Salon vocal · 1 vidéo · rappel 10 min avant"
        );
    }
}
