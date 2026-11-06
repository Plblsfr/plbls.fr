use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use rand::{
    RngExt,
    seq::{IndexedRandom, SliceRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mots;

pub const JOUEURS_MAX: usize = 12;
pub const CHOIX_MS: i64 = 15_000;
pub const FIN_TOUR_MS: i64 = 5_000;
const TEXTE_MAX: usize = 100;
const MOT_PERSO_MIN: usize = 2;
const MOT_PERSO_MAX: usize = 32;
const MOTS_PERSO_MAX: usize = 200;
const TRACES_MAX: usize = 1000;
const POINTS_MAX: usize = 20_000;
const LOT_MAX: usize = 64;
const SEUL_HOTE: &str = "seul l'hôte fait cela";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refus {
    pub statut: StatusCode,
    pub message: String,
}

pub fn refus(statut: StatusCode, message: &str) -> Refus {
    Refus {
        statut,
        message: message.to_string(),
    }
}

pub fn ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub struct Utilisateur<'a> {
    pub id: &'a str,
    pub pseudo: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Lobby,
    Choosing,
    Drawing,
    TurnEnd,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reglages {
    pub rounds: u32,
    pub turn_secs: u32,
    pub word_choices: u32,
    pub max_players: u32,
    pub lang: String,
    pub custom_only: bool,
    pub custom_count: usize,
    #[serde(skip)]
    pub custom_words: Vec<String>,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            rounds: 3,
            turn_secs: 80,
            word_choices: 3,
            max_players: JOUEURS_MAX as u32,
            lang: "fr".into(),
            custom_only: false,
            custom_count: 0,
            custom_words: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReglagesMaj {
    pub rounds: Option<u32>,
    pub turn_secs: Option<u32>,
    pub word_choices: Option<u32>,
    pub max_players: Option<u32>,
    pub lang: Option<String>,
    pub custom_only: Option<bool>,
    pub custom_words: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Joueur {
    pub id: String,
    pub pseudo: String,
    pub points: i64,
    pub turn_points: i64,
    pub connected: bool,
    pub spectator: bool,
    pub found: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub id: String,
    pub tool: String,
    pub color: String,
    pub size: u32,
    pub pts: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Stroke(Trace),
    Undo,
    Clear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diffusion {
    pub kind: &'static str,
    pub from: String,
    pub pseudo: String,
    pub text: String,
    pub to: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Etat {
    pub rev: i64,
    pub server_id: String,
    pub host: String,
    pub phase: Phase,
    pub settings: Reglages,
    pub players: Vec<Joueur>,
    pub round: u32,
    pub drawer: String,
    pub deadline: Option<i64>,
    pub mask: String,
    pub word_len: usize,
    pub reveal: Option<String>,
    pub reason: String,
    pub draw_seq: u64,
    pub reports: usize,
    pub skip_votes: usize,
    pub winners: Vec<String>,
    #[serde(skip)]
    order: Vec<String>,
    #[serde(skip)]
    word: String,
    #[serde(skip)]
    choices: Vec<String>,
    #[serde(skip)]
    hints: Vec<usize>,
    #[serde(skip)]
    guessers: Vec<String>,
    #[serde(skip)]
    guess_total: i64,
    #[serde(skip)]
    used: Vec<String>,
    #[serde(skip)]
    strokes: Vec<Trace>,
    #[serde(skip)]
    kicked: Vec<String>,
    #[serde(skip)]
    votes: Vec<(String, String, String)>,
    #[serde(skip)]
    reported: Vec<String>,
}

pub fn plier(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(sans_accent)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn sans_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prec: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cour = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prec[j] + usize::from(ca != cb);
            cour.push(sub.min(prec[j + 1] + 1).min(cour[j] + 1));
        }
        prec = cour;
    }
    prec[b.len()]
}

pub fn proche(proposition: &str, mot: &str) -> bool {
    let max = if mot.chars().count() <= 4 { 1 } else { 2 };
    (1..=max).contains(&distance(proposition, mot))
}

fn est_lettre(c: char) -> bool {
    c.is_alphanumeric()
}

fn seuil(n: usize) -> usize {
    (2 * n).div_ceil(3).max(1)
}

impl Etat {
    pub fn nouveau(serveur: &str, hote: &Utilisateur) -> Self {
        let mut e = Etat {
            rev: 0,
            server_id: serveur.to_string(),
            host: hote.id.to_string(),
            phase: Phase::Lobby,
            settings: Reglages::default(),
            players: Vec::new(),
            round: 0,
            drawer: String::new(),
            deadline: None,
            mask: String::new(),
            word_len: 0,
            reveal: None,
            reason: String::new(),
            draw_seq: 0,
            reports: 0,
            skip_votes: 0,
            winners: Vec::new(),
            order: Vec::new(),
            word: String::new(),
            choices: Vec::new(),
            hints: Vec::new(),
            guessers: Vec::new(),
            guess_total: 0,
            used: Vec::new(),
            strokes: Vec::new(),
            kicked: Vec::new(),
            votes: Vec::new(),
            reported: Vec::new(),
        };
        e.players.push(Joueur::nouveau(hote, false));
        e
    }

    fn joueur(&self, id: &str) -> Option<&Joueur> {
        self.players.iter().find(|j| j.id == id)
    }

    fn joueur_mut(&mut self, id: &str) -> Option<&mut Joueur> {
        self.players.iter_mut().find(|j| j.id == id)
    }

    fn actifs(&self) -> impl Iterator<Item = &Joueur> {
        self.players.iter().filter(|j| j.connected && !j.spectator)
    }

    fn en_jeu(&self) -> bool {
        matches!(
            self.phase,
            Phase::Choosing | Phase::Drawing | Phase::TurnEnd
        )
    }

    pub fn rejoindre(&mut self, u: &Utilisateur) -> Result<(), Refus> {
        if self.kicked.iter().any(|k| k == u.id) {
            return Err(refus(StatusCode::FORBIDDEN, "vous avez été exclu"));
        }
        if let Some(j) = self.joueur_mut(u.id) {
            j.connected = true;
            return Ok(());
        }
        if self.players.len() >= self.settings.max_players as usize {
            return Err(refus(StatusCode::CONFLICT, "partie pleine"));
        }
        let spectateur = self.phase != Phase::Lobby;
        self.players.push(Joueur::nouveau(u, spectateur));
        Ok(())
    }

    pub fn regler(&mut self, uid: &str, m: ReglagesMaj) -> Result<(), Refus> {
        if self.host != uid {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        if !matches!(self.phase, Phase::Lobby | Phase::Ended) {
            return Err(refus(StatusCode::CONFLICT, "la partie a commencé"));
        }
        let mut r = self.settings.clone();
        let bornes = |v: Option<u32>, min: u32, max: u32, cible: &mut u32| match v {
            Some(v) if !(min..=max).contains(&v) => false,
            Some(v) => {
                *cible = v;
                true
            }
            None => true,
        };
        let ok = bornes(m.rounds, 1, 10, &mut r.rounds)
            && bornes(m.turn_secs, 20, 240, &mut r.turn_secs)
            && bornes(m.word_choices, 1, 5, &mut r.word_choices)
            && bornes(m.max_players, 2, JOUEURS_MAX as u32, &mut r.max_players);
        if !ok {
            return Err(refus(StatusCode::BAD_REQUEST, "réglage hors limites"));
        }
        if let Some(l) = m.lang {
            if !mots::langue_valide(&l) {
                return Err(refus(StatusCode::BAD_REQUEST, "langue inconnue"));
            }
            r.lang = l;
        }
        if let Some(c) = m.custom_only {
            r.custom_only = c;
        }
        if let Some(mots) = m.custom_words {
            let propres: Vec<String> = mots.iter().map(|m| m.trim().to_string()).collect();
            if propres.len() > MOTS_PERSO_MAX
                || propres
                    .iter()
                    .any(|m| !(MOT_PERSO_MIN..=MOT_PERSO_MAX).contains(&m.chars().count()))
                || propres.iter().any(|m| plier(m).is_empty())
            {
                return Err(refus(
                    StatusCode::BAD_REQUEST,
                    "mots personnalisés invalides",
                ));
            }
            r.custom_count = propres.len();
            r.custom_words = propres;
        }
        if r.custom_only && r.custom_words.is_empty() {
            return Err(refus(StatusCode::BAD_REQUEST, "aucun mot personnalisé"));
        }
        if (r.max_players as usize) < self.players.len() {
            return Err(refus(
                StatusCode::BAD_REQUEST,
                "moins de places que de joueurs",
            ));
        }
        self.settings = r;
        Ok(())
    }

    pub fn lancer(&mut self, uid: &str, now: i64) -> Result<(), Refus> {
        if self.host != uid {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        if !matches!(self.phase, Phase::Lobby | Phase::Ended) {
            return Err(refus(StatusCode::CONFLICT, "la partie a commencé"));
        }
        if self.players.iter().filter(|j| j.connected).count() < 2 {
            return Err(refus(StatusCode::CONFLICT, "il faut au moins 2 joueurs"));
        }
        self.players.retain(|j| j.connected);
        for j in &mut self.players {
            j.points = 0;
            j.spectator = false;
        }
        self.round = 0;
        self.order.clear();
        self.used.clear();
        self.winners.clear();
        self.tour_suivant(now);
        Ok(())
    }

    fn terminer(&mut self) {
        self.phase = Phase::Ended;
        self.deadline = None;
        self.drawer.clear();
        self.word.clear();
        self.choices.clear();
        self.mask.clear();
        self.word_len = 0;
        let max = self.players.iter().map(|j| j.points).max().unwrap_or(0);
        self.winners = self
            .players
            .iter()
            .filter(|j| j.points == max)
            .map(|j| j.id.clone())
            .collect();
    }

    fn tour_suivant(&mut self, now: i64) {
        loop {
            if self.order.is_empty() {
                if self.round >= self.settings.rounds {
                    return self.terminer();
                }
                self.round += 1;
                for j in &mut self.players {
                    j.spectator = false;
                }
                self.order = self.actifs().map(|j| j.id.clone()).collect();
                if self.order.len() < 2 {
                    return self.terminer();
                }
            }
            let id = self.order.remove(0);
            if self.actifs().any(|j| j.id == id) {
                self.drawer = id;
                break;
            }
        }
        for j in &mut self.players {
            j.found = false;
            j.turn_points = 0;
        }
        self.hints.clear();
        self.guessers.clear();
        self.strokes.clear();
        self.draw_seq += 1;
        self.votes.retain(|v| v.1 == "kick");
        self.reported.clear();
        self.reports = 0;
        self.skip_votes = 0;
        self.word.clear();
        self.mask.clear();
        self.word_len = 0;
        self.reveal = None;
        self.reason.clear();
        self.choices = self.tirer();
        self.phase = Phase::Choosing;
        self.deadline = Some(now + CHOIX_MS);
    }

    fn tirer(&mut self) -> Vec<String> {
        let perso = self.settings.custom_words.clone();
        let mut pool: Vec<String> = perso;
        if !self.settings.custom_only {
            pool.extend(
                mots::liste(&self.settings.lang)
                    .iter()
                    .map(|m| m.to_string()),
            );
        }
        let mut libres: Vec<String> = pool
            .iter()
            .filter(|m| !self.used.contains(m))
            .cloned()
            .collect();
        if libres.len() < self.settings.word_choices as usize {
            self.used.clear();
            libres = pool;
        }
        libres.shuffle(&mut rand::rng());
        libres.truncate(self.settings.word_choices as usize);
        libres
    }

    fn commencer_dessin(&mut self, mot: String, now: i64) {
        self.used.push(mot.clone());
        self.word_len = mot.chars().count();
        self.word = mot;
        self.choices.clear();
        self.hints.clear();
        self.maj_masque();
        self.guessers = self
            .actifs()
            .filter(|j| j.id != self.drawer)
            .map(|j| j.id.clone())
            .collect();
        self.guess_total = 0;
        self.phase = Phase::Drawing;
        self.deadline = Some(now + i64::from(self.settings.turn_secs) * 1000);
    }

    fn maj_masque(&mut self) {
        self.mask = self
            .word
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if est_lettre(c) && !self.hints.contains(&i) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
    }

    pub fn choisir(&mut self, uid: &str, index: usize, now: i64) -> Result<(), Refus> {
        if self.phase != Phase::Choosing {
            return Err(refus(StatusCode::CONFLICT, "pas de choix en cours"));
        }
        if self.drawer != uid {
            return Err(refus(StatusCode::FORBIDDEN, "seul le dessinateur choisit"));
        }
        let Some(mot) = self.choices.get(index).cloned() else {
            return Err(refus(StatusCode::BAD_REQUEST, "choix invalide"));
        };
        self.commencer_dessin(mot, now);
        Ok(())
    }

    fn tous_ont_trouve(&self) -> bool {
        self.guessers
            .iter()
            .filter_map(|g| self.joueur(g))
            .filter(|j| j.connected)
            .all(|j| j.found)
    }

    pub fn proposer(
        &mut self,
        uid: &str,
        texte: &str,
        now: i64,
    ) -> Result<(Vec<Diffusion>, bool), Refus> {
        let Some(p) = self.joueur(uid) else {
            return Err(refus(StatusCode::FORBIDDEN, "vous ne jouez pas"));
        };
        if p.spectator {
            return Err(refus(
                StatusCode::FORBIDDEN,
                "spectateur jusqu'à la manche suivante",
            ));
        }
        let texte = texte.trim();
        let n = texte.chars().count();
        if n == 0 || n > TEXTE_MAX {
            return Err(refus(
                StatusCode::BAD_REQUEST,
                "message de 1 à 100 caractères",
            ));
        }
        let pseudo = p.pseudo.clone();
        let deja = p.found;
        let msg = |kind, to: Option<Vec<String>>, text: &str| Diffusion {
            kind,
            from: uid.to_string(),
            pseudo: pseudo.clone(),
            text: text.to_string(),
            to,
        };
        if self.phase != Phase::Drawing {
            return Ok((vec![msg("chat", None, texte)], false));
        }
        if uid == self.drawer || deja {
            let mut dest: Vec<String> = self
                .players
                .iter()
                .filter(|j| j.found)
                .map(|j| j.id.clone())
                .collect();
            dest.push(self.drawer.clone());
            return Ok((vec![msg("chat_private", Some(dest), texte)], false));
        }
        let prop = plier(texte);
        let mot = plier(&self.word);
        if prop == mot {
            let total = (i64::from(self.settings.turn_secs) * 1000).max(1);
            let reste = (self.deadline.unwrap_or(now) - now).clamp(0, total);
            let rang = self.players.iter().filter(|j| j.found).count() as i64;
            let pts = (100 + 400 * reste / total - 25 * rang).max(50);
            if let Some(j) = self.joueur_mut(uid) {
                j.found = true;
                j.turn_points = pts;
                j.points += pts;
            }
            self.guess_total += pts;
            let sortie = vec![msg("found", None, "")];
            if self.tous_ont_trouve() {
                self.fin_tour("all_found", now);
            }
            return Ok((sortie, true));
        }
        if proche(&prop, &mot) {
            return Ok((vec![msg("close", Some(vec![uid.to_string()]), "")], false));
        }
        Ok((vec![msg("guess", None, texte)], false))
    }

    fn fin_tour(&mut self, raison: &str, now: i64) {
        if !matches!(self.phase, Phase::Choosing | Phase::Drawing) {
            return;
        }
        if self.phase == Phase::Drawing && raison != "drawer_left" {
            let part = self.guess_total / self.guessers.len().max(1) as i64;
            let d = self.drawer.clone();
            if let Some(j) = self.joueur_mut(&d) {
                j.turn_points = part;
                j.points += part;
            }
        }
        self.reveal = (!self.word.is_empty()).then(|| self.word.clone());
        self.reason = raison.to_string();
        self.choices.clear();
        self.phase = Phase::TurnEnd;
        self.deadline = Some(now + FIN_TOUR_MS);
    }

    pub fn avancer(&mut self, now: i64) -> bool {
        if self.en_jeu() && self.actifs().count() < 2 {
            self.terminer();
            return true;
        }
        let echu = self.deadline.is_some_and(|d| d <= now);
        match self.phase {
            Phase::Choosing if echu => {
                if let Some(m) = self.choices.choose(&mut rand::rng()).cloned() {
                    self.commencer_dessin(m, now);
                } else {
                    self.fin_tour("skipped", now);
                }
                true
            }
            Phase::Drawing => {
                let mut change = false;
                let total = i64::from(self.settings.turn_secs) * 1000;
                let ecoule = total - (self.deadline.unwrap_or(now) - now);
                let lettres: Vec<usize> = self
                    .word
                    .chars()
                    .enumerate()
                    .filter(|(_, c)| est_lettre(*c))
                    .map(|(i, _)| i)
                    .collect();
                let cible = ((ecoule * 3 / total.max(1)).clamp(0, 2) as usize)
                    .min(lettres.len().saturating_sub(1));
                while self.hints.len() < cible {
                    let restantes: Vec<usize> = lettres
                        .iter()
                        .copied()
                        .filter(|i| !self.hints.contains(i))
                        .collect();
                    let i = restantes[rand::rng().random_range(0..restantes.len())];
                    self.hints.push(i);
                    change = true;
                }
                if change {
                    self.maj_masque();
                }
                if echu {
                    self.fin_tour("time", now);
                    change = true;
                }
                change
            }
            Phase::TurnEnd if echu => {
                self.tour_suivant(now);
                true
            }
            _ => false,
        }
    }

    pub fn quitter(&mut self, uid: &str, now: i64) -> bool {
        let Some(i) = self.players.iter().position(|j| j.id == uid) else {
            return true;
        };
        if self.phase == Phase::Lobby {
            self.players.remove(i);
        } else {
            self.players[i].connected = false;
        }
        let Some(suivant) = self
            .players
            .iter()
            .find(|j| j.connected)
            .map(|j| j.id.clone())
        else {
            return false;
        };
        if self.host == uid {
            self.host = suivant;
        }
        self.apres_depart(uid, now);
        true
    }

    fn apres_depart(&mut self, uid: &str, now: i64) {
        if matches!(self.phase, Phase::Choosing | Phase::Drawing) && self.drawer == uid {
            self.fin_tour("drawer_left", now);
        } else if self.phase == Phase::Drawing && self.tous_ont_trouve() {
            self.fin_tour("all_found", now);
        }
    }

    pub fn voter(
        &mut self,
        uid: &str,
        genre: &str,
        cible: Option<&str>,
        now: i64,
    ) -> Result<(), Refus> {
        let Some(p) = self.joueur(uid) else {
            return Err(refus(StatusCode::FORBIDDEN, "vous ne jouez pas"));
        };
        if p.spectator {
            return Err(refus(StatusCode::FORBIDDEN, "spectateur"));
        }
        let hote = self.host == uid;
        match genre {
            "report" => {
                if self.phase != Phase::Drawing || uid == self.drawer {
                    return Err(refus(StatusCode::CONFLICT, "aucun dessin à signaler"));
                }
                if !self.reported.iter().any(|r| r == uid) {
                    self.reported.push(uid.to_string());
                }
                self.reports = self.reported.len();
            }
            "skip" => {
                if !matches!(self.phase, Phase::Choosing | Phase::Drawing) {
                    return Err(refus(StatusCode::CONFLICT, "aucun tour à passer"));
                }
                let cle = (uid.to_string(), "skip".to_string(), String::new());
                if !self.votes.contains(&cle) {
                    self.votes.push(cle);
                }
                let voix = self.votes.iter().filter(|v| v.1 == "skip").count();
                self.skip_votes = voix;
                let n = self.actifs().filter(|j| j.id != self.drawer).count();
                if hote || voix >= seuil(n) {
                    self.fin_tour("skipped", now);
                }
            }
            "kick" => {
                let Some(c) = cible.and_then(|c| self.joueur(c)).map(|j| j.id.clone()) else {
                    return Err(refus(StatusCode::NOT_FOUND, "joueur introuvable"));
                };
                if c == uid || c == self.host {
                    return Err(refus(StatusCode::BAD_REQUEST, "cible invalide"));
                }
                let cle = (uid.to_string(), "kick".to_string(), c.clone());
                if !self.votes.contains(&cle) {
                    self.votes.push(cle);
                }
                let voix = self
                    .votes
                    .iter()
                    .filter(|v| v.1 == "kick" && v.2 == c)
                    .count();
                let n = self.actifs().filter(|j| j.id != c).count();
                if hote || (self.en_jeu() && voix >= seuil(n)) {
                    self.exclure(&c, now);
                }
            }
            _ => return Err(refus(StatusCode::BAD_REQUEST, "vote inconnu")),
        }
        Ok(())
    }

    fn exclure(&mut self, id: &str, now: i64) {
        self.players.retain(|j| j.id != id);
        self.order.retain(|o| o != id);
        self.votes.retain(|v| v.2 != id && v.0 != id);
        self.kicked.push(id.to_string());
        self.apres_depart(id, now);
    }

    pub fn dessiner(&mut self, uid: &str, op: Op) -> Result<(u64, Op), Refus> {
        if self.phase != Phase::Drawing || self.drawer != uid {
            return Err(refus(StatusCode::FORBIDDEN, "seul le dessinateur dessine"));
        }
        let invalide = |m: &str| Err(refus(StatusCode::BAD_REQUEST, m));
        match &op {
            Op::Stroke(t) => {
                if t.id.is_empty() || t.id.len() > 32 {
                    return invalide("identifiant de trait invalide");
                }
                if !["pen", "eraser", "fill"].contains(&t.tool.as_str()) {
                    return invalide("outil inconnu");
                }
                let c = t.color.as_bytes();
                if c.len() != 7 || c[0] != b'#' || !c[1..].iter().all(u8::is_ascii_hexdigit) {
                    return invalide("couleur invalide");
                }
                if !(1..=64).contains(&t.size) {
                    return invalide("épaisseur invalide");
                }
                if t.pts.is_empty()
                    || t.pts.len() > LOT_MAX
                    || (t.tool == "fill" && t.pts.len() != 1)
                {
                    return invalide("lot de points invalide");
                }
                if t.pts.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
                    return invalide("coordonnées hors de la zone");
                }
                let total: usize = self.strokes.iter().map(|s| s.pts.len()).sum();
                if total + t.pts.len() > POINTS_MAX {
                    return Err(refus(StatusCode::CONFLICT, "dessin trop chargé"));
                }
                match self.strokes.iter_mut().find(|s| s.id == t.id) {
                    Some(s) => {
                        if s.tool != t.tool
                            || s.color != t.color
                            || s.size != t.size
                            || s.tool == "fill"
                        {
                            return invalide("trait incohérent");
                        }
                        s.pts.extend(t.pts.iter().copied());
                    }
                    None => {
                        if self.strokes.len() >= TRACES_MAX {
                            return Err(refus(StatusCode::CONFLICT, "dessin trop chargé"));
                        }
                        self.strokes.push(t.clone());
                    }
                }
            }
            Op::Undo => {
                self.strokes.pop();
            }
            Op::Clear => self.strokes.clear(),
        }
        self.draw_seq += 1;
        Ok((self.draw_seq, op))
    }

    pub fn dessin(&self) -> Value {
        json!({ "seq": self.draw_seq, "strokes": self.strokes })
    }

    pub fn prive(&self, uid: &str) -> Value {
        if self.drawer != uid {
            return json!({});
        }
        match self.phase {
            Phase::Choosing => json!({ "choices": self.choices }),
            Phase::Drawing => json!({ "word": self.word }),
            _ => json!({}),
        }
    }
}

impl Joueur {
    fn nouveau(u: &Utilisateur, spectateur: bool) -> Self {
        Joueur {
            id: u.id.to_string(),
            pseudo: u.pseudo.to_string(),
            points: 0,
            turn_points: 0,
            connected: true,
            spectator: spectateur,
            found: false,
        }
    }
}

#[derive(Default)]
pub struct Parties(Mutex<HashMap<String, Etat>>);

impl Parties {
    fn verrou(&self) -> std::sync::MutexGuard<'_, HashMap<String, Etat>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn creer_ou_rejoindre(
        &self,
        salon: &str,
        serveur: &str,
        u: &Utilisateur,
    ) -> Result<Etat, Refus> {
        let mut m = self.verrou();
        let e = match m.get_mut(salon) {
            Some(e) => {
                e.rejoindre(u)?;
                e
            }
            None => m
                .entry(salon.to_string())
                .or_insert_with(|| Etat::nouveau(serveur, u)),
        };
        e.rev += 1;
        Ok(e.clone())
    }

    pub fn agir<R>(
        &self,
        salon: &str,
        f: impl FnOnce(&mut Etat, i64) -> Result<R, Refus>,
    ) -> Result<(Etat, R), Refus> {
        let mut m = self.verrou();
        let e = m
            .get_mut(salon)
            .ok_or_else(|| refus(StatusCode::NOT_FOUND, "aucune partie dans ce salon"))?;
        let r = f(e, ms(SystemTime::now()))?;
        e.rev += 1;
        Ok((e.clone(), r))
    }

    pub fn dessiner(&self, salon: &str, uid: &str, op: Op) -> Result<(u64, Op), Refus> {
        let mut m = self.verrou();
        m.get_mut(salon)
            .ok_or_else(|| refus(StatusCode::NOT_FOUND, "aucune partie dans ce salon"))?
            .dessiner(uid, op)
    }

    pub fn arreter(&self, salon: &str, uid: &str) -> Result<(), Refus> {
        let mut m = self.verrou();
        let e = m
            .get(salon)
            .ok_or_else(|| refus(StatusCode::NOT_FOUND, "aucune partie dans ce salon"))?;
        if e.host != uid {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        m.remove(salon);
        Ok(())
    }

    pub fn etat(&self, salon: &str) -> Option<Etat> {
        self.verrou().get(salon).cloned()
    }

    pub fn quitter(&self, salon: &str, membre: &str, now: i64) -> Option<Option<Etat>> {
        let mut m = self.verrou();
        let e = m.get_mut(salon)?;
        if !e.quitter(membre, now) {
            m.remove(salon);
            return Some(None);
        }
        e.rev += 1;
        Some(Some(e.clone()))
    }

    pub fn tic(&self, now: i64) -> Vec<(String, Etat)> {
        let mut m = self.verrou();
        let mut sortie = Vec::new();
        for (salon, e) in m.iter_mut() {
            if e.avancer(now) {
                e.rev += 1;
                sortie.push((salon.clone(), e.clone()));
            }
        }
        sortie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: &str) -> Utilisateur<'_> {
        Utilisateur { id, pseudo: id }
    }

    fn salle(ids: &[&str]) -> Etat {
        let mut e = Etat::nouveau("s", &u(ids[0]));
        for id in &ids[1..] {
            e.rejoindre(&u(id)).unwrap();
        }
        e
    }

    fn en_dessin(ids: &[&str], mot: &str) -> Etat {
        let mut e = salle(ids);
        e.settings.word_choices = 1;
        e.lancer(ids[0], 0).unwrap();
        e.choices = vec![mot.to_string()];
        e.choisir(&e.drawer.clone(), 0, 0).unwrap();
        e
    }

    fn pts(e: &Etat, id: &str) -> i64 {
        e.joueur(id).unwrap().points
    }

    fn autre(e: &Etat) -> String {
        e.players
            .iter()
            .find(|j| j.id != e.drawer)
            .unwrap()
            .id
            .clone()
    }

    #[test]
    fn validation_sans_casse_accents_ni_espaces() {
        assert_eq!(plier("  Éléphant "), "elephant");
        assert_eq!(plier("Ice  Cream"), "icecream");
        let mut e = en_dessin(&["a", "b"], "éléphant");
        let b = autre(&e);
        let (d, change) = e.proposer(&b, "  ELEPHANT ", 1000).unwrap();
        assert!(change && d[0].kind == "found" && d[0].text.is_empty() && d[0].to.is_none());
    }

    #[test]
    fn mot_proche_prive_au_joueur() {
        let mut e = en_dessin(&["a", "b"], "maison");
        let b = autre(&e);
        let (d, change) = e.proposer(&b, "maisson", 10).unwrap();
        assert!(!change);
        assert_eq!(
            (d[0].kind, d[0].to.clone()),
            ("close", Some(vec![b.clone()]))
        );
        assert!(d[0].text.is_empty());
        let (d, _) = e.proposer(&b, "voiture", 10).unwrap();
        assert_eq!((d[0].kind, d[0].to.clone()), ("guess", None));
        assert!(proche("chat", "chut"));
        assert!(!proche("chat", "chien"));
        assert!(!proche("chat", "chat"));
        assert!(proche("maison", "maisan"));
        assert!(proche("maison", "mason"));
    }

    #[test]
    fn mot_jamais_dans_l_etat_public() {
        let mut e = en_dessin(&["a", "b", "c"], "dragon");
        let public = serde_json::to_string(&e).unwrap();
        assert!(!public.contains("dragon"));
        assert_eq!(e.mask, "______");
        let b = autre(&e);
        assert_eq!(e.prive(&b), json!({}));
        assert_eq!(e.prive(&e.drawer), json!({ "word": "dragon" }));
        e.settings.custom_words = vec!["secret".into()];
        assert!(!serde_json::to_string(&e).unwrap().contains("secret"));
        e.fin_tour("time", 5);
        assert!(serde_json::to_string(&e).unwrap().contains("dragon"));
    }

    #[test]
    fn choix_masque_au_devineur_et_tirage_distinct() {
        let mut e = salle(&["a", "b"]);
        e.lancer("a", 0).unwrap();
        assert_eq!(e.phase, Phase::Choosing);
        assert_eq!(e.choices.len(), 3);
        let mut c = e.choices.clone();
        c.dedup();
        assert_eq!(c.len(), 3);
        let b = autre(&e);
        assert_eq!(e.prive(&b), json!({}));
        assert!(!serde_json::to_string(&e).unwrap().contains(&e.choices[0]));
        assert_eq!(
            e.choisir(&b, 0, 1).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        let d = e.drawer.clone();
        assert_eq!(
            e.choisir(&d, 3, 1).unwrap_err().statut,
            StatusCode::BAD_REQUEST
        );
        e.choisir(&d, 2, 1).unwrap();
        assert_eq!(e.phase, Phase::Drawing);
    }

    #[test]
    fn choix_automatique_apres_le_delai() {
        let mut e = salle(&["a", "b"]);
        e.lancer("a", 0).unwrap();
        assert!(!e.avancer(CHOIX_MS - 1));
        assert!(e.avancer(CHOIX_MS));
        assert_eq!(e.phase, Phase::Drawing);
        assert_eq!(e.deadline, Some(CHOIX_MS + 80_000));
    }

    #[test]
    fn points_degressifs_et_dessinateur() {
        let mut e = en_dessin(&["a", "b", "c"], "chat");
        let ids: Vec<String> = e
            .players
            .iter()
            .filter(|j| j.id != e.drawer)
            .map(|j| j.id.clone())
            .collect();
        e.proposer(&ids[0], "chat", 0).unwrap();
        assert_eq!(pts(&e, &ids[0]), 500);
        e.proposer(&ids[1], "chat", 40_000).unwrap();
        assert_eq!(pts(&e, &ids[1]), 100 + 200 - 25);
        assert_eq!(e.phase, Phase::TurnEnd);
        assert_eq!(e.reason, "all_found");
        assert_eq!(pts(&e, &e.drawer.clone()), (500 + 275) / 2);
    }

    #[test]
    fn dessinateur_sans_trouveur_ne_marque_rien() {
        let mut e = en_dessin(&["a", "b"], "chat");
        assert!(e.avancer(80_000));
        assert_eq!((e.phase, e.reason.as_str()), (Phase::TurnEnd, "time"));
        assert_eq!(e.reveal.as_deref(), Some("chat"));
        assert_eq!(pts(&e, "a") + pts(&e, "b"), 0);
    }

    #[test]
    fn messages_du_dessinateur_et_des_trouveurs_restent_prives() {
        let mut e = en_dessin(&["a", "b", "c"], "chat");
        let d = e.drawer.clone();
        let ids: Vec<String> = e
            .players
            .iter()
            .filter(|j| j.id != d)
            .map(|j| j.id.clone())
            .collect();
        e.proposer(&ids[0], "chat", 1000).unwrap();
        let (m, _) = e.proposer(&d, "pense à un animal", 1000).unwrap();
        let mut dest = m[0].to.clone().unwrap();
        dest.sort();
        let mut attendu = vec![d.clone(), ids[0].clone()];
        attendu.sort();
        assert_eq!((m[0].kind, dest), ("chat_private", attendu.clone()));
        let (m, change) = e.proposer(&ids[0], "chat", 1000).unwrap();
        assert!(!change && m[0].kind == "chat_private");
        let pas_trouve = e.proposer(&ids[1], "bonjour", 1000).unwrap().0;
        assert_eq!(pas_trouve[0].kind, "guess");
    }

    #[test]
    fn indices_progressifs_jamais_toutes_les_lettres() {
        let mut e = en_dessin(&["a", "b"], "ab");
        assert!(!e.avancer(10_000));
        assert!(e.avancer(27_000));
        assert_eq!(e.mask.matches('_').count(), 1);
        e.avancer(54_000);
        assert_eq!(e.mask.matches('_').count(), 1);
        let mut e = en_dessin(&["a", "b"], "ice cream");
        assert_eq!(e.mask, "___ _____");
        e.avancer(27_000);
        assert_eq!(e.mask.matches('_').count(), 7);
        e.avancer(54_000);
        assert_eq!(e.mask.matches('_').count(), 6);
        assert!(e.mask.chars().nth(3) == Some(' '));
        let mut seul = en_dessin(&["a", "b"], "x");
        seul.avancer(54_000);
        assert_eq!(seul.mask, "_");
    }

    #[test]
    fn chaque_joueur_dessine_une_fois_par_manche() {
        let mut e = salle(&["a", "b", "c"]);
        e.settings.rounds = 2;
        e.lancer("a", 0).unwrap();
        let mut dessinateurs = Vec::new();
        let mut now = 0;
        while e.phase != Phase::Ended {
            if e.phase == Phase::Choosing {
                dessinateurs.push((e.round, e.drawer.clone()));
                let d = e.drawer.clone();
                e.choisir(&d, 0, now).unwrap();
            }
            now += 100_000;
            e.avancer(now);
        }
        assert_eq!(dessinateurs.len(), 6);
        for manche in 1..=2 {
            let mut v: Vec<_> = dessinateurs
                .iter()
                .filter(|(m, _)| *m == manche)
                .map(|(_, d)| d.clone())
                .collect();
            v.sort();
            assert_eq!(v, ["a", "b", "c"]);
        }
        assert!(!e.winners.is_empty());
    }

    #[test]
    fn fin_de_partie_classement_et_rejouer() {
        let mut e = en_dessin(&["a", "b"], "chat");
        e.settings.rounds = 1;
        let b = autre(&e);
        e.proposer(&b, "chat", 0).unwrap();
        e.avancer(FIN_TOUR_MS);
        let d = e.drawer.clone();
        e.choisir(&d, 0, 0).unwrap();
        e.avancer(1_000_000);
        e.avancer(2_000_000);
        assert_eq!(e.phase, Phase::Ended);
        assert!(e.winners.contains(&b));
        assert_eq!(e.lancer("b", 0).unwrap_err().statut, StatusCode::FORBIDDEN);
        e.lancer("a", 0).unwrap();
        assert_eq!(
            (e.phase, pts(&e, "a"), pts(&e, "b")),
            (Phase::Choosing, 0, 0)
        );
    }

    #[test]
    fn lancement_et_reglages() {
        let mut e = salle(&["a"]);
        assert_eq!(e.lancer("a", 0).unwrap_err().statut, StatusCode::CONFLICT);
        e.rejoindre(&u("b")).unwrap();
        assert_eq!(e.lancer("b", 0).unwrap_err().statut, StatusCode::FORBIDDEN);
        let m = |rounds| ReglagesMaj {
            rounds,
            ..Default::default()
        };
        assert_eq!(
            e.regler("b", m(Some(2))).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            e.regler("a", m(Some(0))).unwrap_err().statut,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            e.regler("a", m(Some(11))).unwrap_err().statut,
            StatusCode::BAD_REQUEST
        );
        let perso = ReglagesMaj {
            custom_words: Some(vec!["a".into()]),
            ..Default::default()
        };
        assert!(e.regler("a", perso).is_err());
        let seuls = ReglagesMaj {
            custom_only: Some(true),
            ..Default::default()
        };
        assert!(e.regler("a", seuls).is_err());
        e.regler(
            "a",
            ReglagesMaj {
                rounds: Some(2),
                turn_secs: Some(30),
                word_choices: Some(2),
                lang: Some("en".into()),
                custom_only: Some(true),
                custom_words: Some(vec!["Zèbre rayé".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        e.lancer("a", 5).unwrap();
        assert_eq!(e.choices, ["Zèbre rayé"]);
        assert_eq!(
            e.regler("a", m(Some(2))).unwrap_err().statut,
            StatusCode::CONFLICT
        );
        assert_eq!(e.lancer("a", 5).unwrap_err().statut, StatusCode::CONFLICT);
    }

    #[test]
    fn de_deux_a_douze_joueurs() {
        let ids: Vec<String> = (0..13).map(|i| format!("j{i}")).collect();
        let mut e = Etat::nouveau("s", &u(&ids[0]));
        for id in &ids[1..12] {
            e.rejoindre(&u(id)).unwrap();
        }
        assert_eq!(
            e.rejoindre(&u(&ids[12])).unwrap_err().statut,
            StatusCode::CONFLICT
        );
        let mut p = salle(&["a", "b", "c"]);
        p.regler(
            "a",
            ReglagesMaj {
                max_players: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(p.rejoindre(&u("d")).is_err());
        assert!(
            p.regler(
                "a",
                ReglagesMaj {
                    max_players: Some(2),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn arrivee_en_cours_spectateur_puis_joueur() {
        let mut e = en_dessin(&["a", "b"], "chat");
        e.rejoindre(&u("c")).unwrap();
        assert!(e.joueur("c").unwrap().spectator);
        assert_eq!(
            e.proposer("c", "chat", 0).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        e.avancer(80_000);
        e.avancer(80_000 + FIN_TOUR_MS);
        let d = e.drawer.clone();
        e.choisir(&d, 0, 0).unwrap();
        e.avancer(1_000_000);
        e.avancer(2_000_000);
        assert_eq!(e.round, 2);
        assert!(!e.joueur("c").unwrap().spectator);
        assert_eq!(e.order.len(), 2);
    }

    #[test]
    fn deconnexion_du_devineur_garde_les_points_et_reprise() {
        let mut e = en_dessin(&["a", "b", "c"], "chat");
        let d = e.drawer.clone();
        let ids: Vec<String> = e
            .players
            .iter()
            .filter(|j| j.id != d)
            .map(|j| j.id.clone())
            .collect();
        e.proposer(&ids[0], "chat", 0).unwrap();
        assert!(e.quitter(&ids[0], 10));
        assert_eq!(e.phase, Phase::Drawing);
        assert!(e.quitter(&ids[1], 10));
        assert_eq!(e.phase, Phase::TurnEnd);
        assert!(pts(&e, &ids[0]) > 0);
        e.rejoindre(&u(&ids[0])).unwrap();
        assert!(e.joueur(&ids[0]).unwrap().connected);
        assert!(pts(&e, &ids[0]) > 0);
    }

    #[test]
    fn deconnexion_du_dessinateur_passe_au_suivant() {
        let mut e = en_dessin(&["a", "b", "c"], "chat");
        let d = e.drawer.clone();
        assert!(e.quitter(&d, 10));
        assert_eq!(
            (e.phase, e.reason.as_str()),
            (Phase::TurnEnd, "drawer_left")
        );
        assert_eq!(pts(&e, &d), 0);
        e.avancer(10 + FIN_TOUR_MS);
        assert_eq!(e.phase, Phase::Choosing);
        assert_ne!(e.drawer, d);
        let mut e = salle(&["a", "b", "c"]);
        e.lancer("a", 0).unwrap();
        let d = e.drawer.clone();
        e.quitter(&d, 1);
        assert_eq!(e.reveal, None);
        assert_eq!(e.phase, Phase::TurnEnd);
    }

    #[test]
    fn deconnexion_de_l_hote_et_plus_personne() {
        let mut e = salle(&["a", "b", "c"]);
        assert!(e.quitter("a", 0));
        assert_eq!(e.host, "b");
        assert_eq!(e.players.len(), 2);
        let mut e = en_dessin(&["a", "b", "c"], "chat");
        assert!(e.quitter("a", 0));
        assert_eq!(e.host, "b");
        assert!(e.joueur("a").is_some());
        assert!(e.quitter("b", 0));
        assert!(e.avancer(1));
        assert_eq!(e.phase, Phase::Ended);
        assert!(!e.quitter("c", 0));
    }

    #[test]
    fn traits_vectoriels_valides_et_rejoues() {
        let mut e = en_dessin(&["a", "b"], "chat");
        let d = e.drawer.clone();
        let b = autre(&e);
        let t = |id: &str, pts: Vec<[f32; 2]>| Trace {
            id: id.into(),
            tool: "pen".into(),
            color: "#aa00FF".into(),
            size: 4,
            pts,
        };
        assert_eq!(
            e.dessiner(&b, Op::Clear).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        let (seq, _) = e
            .dessiner(&d, Op::Stroke(t("1", vec![[0.1, 0.2]])))
            .unwrap();
        e.dessiner(&d, Op::Stroke(t("1", vec![[0.3, 0.4], [0.5, 0.5]])))
            .unwrap();
        e.dessiner(&d, Op::Stroke(t("2", vec![[0.0, 1.0]])))
            .unwrap();
        assert_eq!(seq, e.draw_seq - 2);
        assert_eq!(e.strokes.len(), 2);
        assert_eq!(e.strokes[0].pts.len(), 3);
        assert_eq!(e.dessin()["strokes"][0]["pts"][2], json!([0.5, 0.5]));
        e.dessiner(&d, Op::Undo).unwrap();
        assert_eq!(e.strokes.len(), 1);
        e.dessiner(&d, Op::Clear).unwrap();
        assert!(e.strokes.is_empty());
        for mauvais in [
            Trace {
                color: "rouge".into(),
                ..t("1", vec![[0.0, 0.0]])
            },
            Trace {
                size: 65,
                ..t("1", vec![[0.0, 0.0]])
            },
            Trace {
                tool: "laser".into(),
                ..t("1", vec![[0.0, 0.0]])
            },
            t("1", vec![[1.5, 0.0]]),
            t("1", vec![]),
            t("1", vec![[0.0, 0.0]; 65]),
            t("", vec![[0.0, 0.0]]),
            Trace {
                tool: "fill".into(),
                ..t("1", vec![[0.0, 0.0], [0.1, 0.1]])
            },
        ] {
            assert_eq!(
                e.dessiner(&d, Op::Stroke(mauvais)).unwrap_err().statut,
                StatusCode::BAD_REQUEST
            );
        }
        e.dessiner(&d, Op::Stroke(t("1", vec![[0.0, 0.0]])))
            .unwrap();
        let autre_couleur = Trace {
            color: "#000000".into(),
            ..t("1", vec![[0.0, 0.0]])
        };
        assert!(e.dessiner(&d, Op::Stroke(autre_couleur)).is_err());
        let mut plein = 0;
        while e
            .dessiner(
                &d,
                Op::Stroke(t(&format!("p{plein}"), vec![[0.0, 0.0]; 64])),
            )
            .is_ok()
        {
            plein += 1;
        }
        assert!(plein < TRACES_MAX && plein > 100);
        assert!(serde_json::from_value::<Op>(json!({"op":"stroke","id":"1","tool":"pen","color":"#000000","size":2,"pts":[[0.5,0.5]]})).is_ok());
        assert_eq!(
            serde_json::to_value(Op::Undo).unwrap(),
            json!({"op":"undo"})
        );
    }

    #[test]
    fn votes_signalement_et_exclusion() {
        let mut e = en_dessin(&["a", "b", "c", "d"], "chat");
        let d = e.drawer.clone();
        let ids: Vec<String> = e
            .players
            .iter()
            .filter(|j| j.id != d)
            .map(|j| j.id.clone())
            .collect();
        assert_eq!(
            e.voter(&d, "report", None, 1).unwrap_err().statut,
            StatusCode::CONFLICT
        );
        e.voter(&ids[0], "report", None, 1).unwrap();
        e.voter(&ids[0], "report", None, 1).unwrap();
        assert_eq!(e.reports, 1);
        e.voter(&ids[1], "skip", None, 1).unwrap();
        e.voter(&ids[1], "skip", None, 1).unwrap();
        assert_eq!((e.phase, e.skip_votes), (Phase::Drawing, 1));
        e.voter(&ids[2], "skip", None, 1).unwrap();
        assert_eq!((e.phase, e.reason.as_str()), (Phase::TurnEnd, "skipped"));
        assert_eq!(
            e.voter("a", "nimporte", None, 1).unwrap_err().statut,
            StatusCode::BAD_REQUEST
        );

        let mut e = salle(&["a", "b", "c", "d"]);
        e.voter("a", "kick", Some("d"), 0).unwrap();
        assert!(e.joueur("d").is_none());
        assert_eq!(
            e.rejoindre(&u("d")).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            e.voter("a", "kick", Some("a"), 0).unwrap_err().statut,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            e.voter("a", "kick", Some("zz"), 0).unwrap_err().statut,
            StatusCode::NOT_FOUND
        );

        let mut e = en_dessin(&["a", "b", "c", "d"], "chat");
        let d = e.drawer.clone();
        let cible = e
            .players
            .iter()
            .map(|j| j.id.clone())
            .find(|i| *i != d && i != "a")
            .unwrap();
        let votants: Vec<String> = e
            .players
            .iter()
            .map(|j| j.id.clone())
            .filter(|i| *i != cible && i != "a")
            .collect();
        e.voter(&votants[0], "kick", Some(&cible), 1).unwrap();
        assert!(e.joueur(&cible).is_some());
        e.voter(&votants[1], "kick", Some(&cible), 1).unwrap();
        assert!(e.joueur(&cible).is_none());
    }

    #[test]
    fn parties_agir_et_arreter() {
        let p = Parties::default();
        assert_eq!(
            p.agir("x", |_, _| Ok(())).unwrap_err().statut,
            StatusCode::NOT_FOUND
        );
        let e = p.creer_ou_rejoindre("x", "s", &u("a")).unwrap();
        assert_eq!((e.rev, e.host.as_str()), (1, "a"));
        let e = p.creer_ou_rejoindre("x", "s", &u("b")).unwrap();
        assert_eq!((e.rev, e.players.len()), (2, 2));
        let (e, ()) = p.agir("x", |e, now| e.lancer("a", now)).unwrap();
        assert_eq!(e.rev, 3);
        assert_eq!(
            p.arreter("x", "b").unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert!(p.tic(0).is_empty());
        assert_eq!(p.tic(ms(SystemTime::now()) + 20_000).len(), 1);
        assert!(matches!(p.quitter("x", "a", 0), Some(Some(_))));
        assert!(matches!(p.quitter("x", "b", 0), Some(None)));
        assert!(p.etat("x").is_none());
        p.creer_ou_rejoindre("y", "s", &u("a")).unwrap();
        p.arreter("y", "a").unwrap();
        assert!(p.etat("y").is_none());
    }
}
