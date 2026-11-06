use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use rand::{RngExt, seq::SliceRandom};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

pub const FILE_MAX: usize = 50;
pub const POSITION_MAX: f64 = 24.0 * 3600.0;
const VITESSES: [f64; 8] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
const SEUL_HOTE: &str = "seul l'hôte pilote la lecture";
pub const PROVIDER_FILM: &str = "film";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
    pub cover: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: String,
    pub provider: String,
    pub video: String,
    pub by: String,
    pub by_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Etat {
    pub rev: i64,
    pub provider: String,
    pub video: String,
    pub playing: bool,
    pub pos: f64,
    pub rate: f64,
    #[serde(serialize_with = "serialiser_heure")]
    pub at: SystemTime,
    pub host: String,
    pub host_only: bool,
    pub by_id: String,
    pub by: String,
    pub act: String,
    pub queue: Vec<Item>,
    pub ready: BTreeMap<String, bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    pub repeat: String,
    pub history: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refus {
    pub statut: StatusCode,
    pub message: String,
}

pub type Resultat = Result<Option<Etat>, Refus>;

pub struct Action {
    pub action: String,
    pub position: f64,
    pub rate: f64,
    pub rev: i64,
    pub video: String,
}

pub fn refus(statut: StatusCode, message: &str) -> Refus {
    Refus {
        statut,
        message: message.to_string(),
    }
}

pub fn nouvel_id() -> String {
    let octets: [u8; 8] = rand::rng().random();
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

pub fn heure(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secondes = d.as_secs() as i64;
    let jours = secondes.div_euclid(86_400);
    let reste = secondes.rem_euclid(86_400);
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let jde = z.rem_euclid(146_097);
    let annee_ere = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
    let jour_annee = jde - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
    let mp = (5 * jour_annee + 2) / 153;
    let jour = jour_annee - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = annee_ere + ere * 400 + i64::from(mois <= 2);
    let base = format!(
        "{annee:04}-{mois:02}-{jour:02}T{:02}:{:02}:{:02}",
        reste / 3600,
        reste % 3600 / 60,
        reste % 60
    );
    match format!("{:09}", d.subsec_nanos()).trim_end_matches('0') {
        "" => format!("{base}Z"),
        f => format!("{base}.{f}Z"),
    }
}

fn serialiser_heure<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&heure(*t))
}

pub fn peut_piloter(s: &Etat, uid: &str) -> bool {
    !s.host_only || s.host == uid
}

fn tronquer(file: &mut Vec<Item>) {
    file.truncate(FILE_MAX);
}

impl Etat {
    fn nouveau(hote: &str, now: SystemTime) -> Self {
        Etat {
            rev: 0,
            provider: String::new(),
            video: String::new(),
            playing: false,
            pos: 0.0,
            rate: 0.0,
            at: now,
            host: hote.to_string(),
            host_only: false,
            by_id: String::new(),
            by: String::new(),
            act: String::new(),
            queue: Vec::new(),
            ready: BTreeMap::new(),
            meta: None,
            repeat: String::new(),
            history: Vec::new(),
        }
    }

    fn courant(&self) -> Item {
        Item {
            id: nouvel_id(),
            provider: self.provider.clone(),
            video: self.video.clone(),
            by: String::new(),
            by_id: String::new(),
            meta: self.meta.clone(),
        }
    }

    fn lire(&mut self, it: Item) {
        self.provider = it.provider;
        self.video = it.video;
        self.meta = it.meta;
        self.playing = true;
        self.rate = 1.0;
        self.pos = 0.0;
        self.act = "video".into();
    }

    fn avancer(&mut self) -> bool {
        if self.queue.is_empty() && self.repeat == "all" && !self.history.is_empty() {
            let mut tout = std::mem::take(&mut self.history);
            tout.push(self.courant());
            let premier = tout.remove(0);
            self.queue = tout;
            self.lire(premier);
            return true;
        }
        if self.queue.is_empty() {
            return false;
        }
        self.history.push(self.courant());
        if self.history.len() > FILE_MAX {
            self.history.remove(0);
        }
        let it = self.queue.remove(0);
        self.lire(it);
        true
    }

    pub fn position(&self, now: SystemTime) -> f64 {
        if !self.playing {
            return self.pos;
        }
        let ecoule = match now.duration_since(self.at) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        };
        self.pos + ecoule * self.rate
    }
}

pub struct Utilisateur<'a> {
    pub id: &'a str,
    pub pseudo: &'a str,
}

pub fn demarrer(
    cur: Option<&Etat>,
    u: &Utilisateur,
    items: Vec<Item>,
    file: Option<Vec<Item>>,
    now: SystemTime,
) -> Resultat {
    if let Some(s) = cur
        && !peut_piloter(s, u.id)
    {
        return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
    }
    let mut n = cur.cloned().unwrap_or_else(|| Etat::nouveau(u.id, now));
    let mut items = items.into_iter();
    let Some(premier) = items.next() else {
        return Err(refus(StatusCode::BAD_REQUEST, "aucun élément à lire"));
    };
    if premier.provider == PROVIDER_FILM {
        n.host = u.id.to_string();
        n.host_only = true;
    } else if n.provider == PROVIDER_FILM {
        n.host_only = false;
    }
    if let Some(file) = file {
        n.queue = file;
    }
    let mut queue: Vec<Item> = items.collect();
    queue.append(&mut n.queue);
    n.queue = queue;
    tronquer(&mut n.queue);
    n.lire(premier);
    n.at = now;
    n.by = u.pseudo.to_string();
    n.by_id = u.id.to_string();
    n.ready = BTreeMap::new();
    Ok(Some(n))
}

pub fn lancer_file(
    cur: Option<&Etat>,
    par: &Utilisateur,
    mut items: Vec<Item>,
    now: SystemTime,
) -> Resultat {
    if let Some(s) = cur {
        let mut n = s.clone();
        n.queue.extend(items);
        tronquer(&mut n.queue);
        return Ok(Some(n));
    }
    items.truncate(FILE_MAX + 1);
    let mut items = items.into_iter();
    let Some(premier) = items.next() else {
        return Err(refus(StatusCode::BAD_REQUEST, "aucun élément à lire"));
    };
    let mut n = Etat::nouveau(par.id, now);
    n.lire(premier);
    n.by = par.pseudo.to_string();
    n.by_id = par.id.to_string();
    n.queue = items.collect();
    Ok(Some(n))
}

pub fn arreter(s: &Etat, uid: &str) -> Resultat {
    if !peut_piloter(s, uid) {
        return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
    }
    Ok(None)
}

pub fn agir(s: &Etat, u: &Utilisateur, a: &Action, now: SystemTime) -> Resultat {
    let mut n = s.clone();
    n.by = u.pseudo.to_string();
    n.by_id = u.id.to_string();
    n.act = a.action.clone();
    n.at = now;
    if a.action == "ended" {
        if s.provider == PROVIDER_FILM && !peut_piloter(s, u.id) {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        if a.rev != s.rev {
            return Ok(Some(s.clone()));
        }
        n.pos = 0.0;
        n.playing = false;
        n.ready = BTreeMap::new();
        if s.repeat == "one" || (s.repeat == "all" && s.queue.is_empty() && s.history.is_empty()) {
            n.playing = true;
            n.rate = 1.0;
            n.act = "repeat".into();
        } else {
            n.avancer();
        }
        return Ok(Some(n));
    }
    if !peut_piloter(s, u.id) {
        return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
    }
    if !a.video.is_empty() && a.video != s.video {
        return Err(refus(StatusCode::CONFLICT, "la vidéo a changé"));
    }
    match a.action.as_str() {
        "play" => {
            n.playing = true;
            n.pos = a.position;
        }
        "pause" => {
            n.playing = false;
            n.pos = a.position;
        }
        "seek" => n.pos = a.position,
        "previous" => {
            if let Some(prev) = n.history.pop() {
                let mut queue = vec![n.courant()];
                queue.extend(s.queue.iter().cloned());
                tronquer(&mut queue);
                n.queue = queue;
                n.lire(prev);
            } else {
                n.playing = true;
                n.pos = 0.0;
                n.act = "seek".into();
            }
        }
        "shuffle" => n.queue.shuffle(&mut rand::rng()),
        "rate" => {
            if !VITESSES.contains(&a.rate) {
                return Err(refus(StatusCode::BAD_REQUEST, "vitesse invalide"));
            }
            n.pos = s.position(now);
            n.rate = a.rate;
        }
        _ => return Err(refus(StatusCode::BAD_REQUEST, "action inconnue")),
    }
    Ok(Some(n))
}

pub fn pret(s: &Etat, uid: &str, pret: bool) -> Resultat {
    let mut n = s.clone();
    n.ready.insert(uid.to_string(), pret);
    n.rev -= 1;
    Ok(Some(n))
}

pub fn options(s: &Etat, uid: &str, host_only: Option<bool>, repeat: Option<&str>) -> Resultat {
    let mut n = s.clone();
    if let Some(h) = host_only {
        if s.host != uid {
            return Err(refus(
                StatusCode::FORBIDDEN,
                "seul l'hôte change cette option",
            ));
        }
        if !h && s.provider == PROVIDER_FILM {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        n.host_only = h;
    }
    if let Some(r) = repeat {
        if !peut_piloter(s, uid) {
            return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
        }
        if !["", "one", "all"].contains(&r) {
            return Err(refus(StatusCode::BAD_REQUEST, "mode de répétition inconnu"));
        }
        n.repeat = r.to_string();
    }
    Ok(Some(n))
}

pub fn ajouter(s: &Etat, mut items: Vec<Item>) -> Resultat {
    if s.provider == PROVIDER_FILM && items.iter().any(|i| !peut_piloter(s, &i.by_id)) {
        return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
    }
    if s.queue.len() >= FILE_MAX {
        return Err(refus(StatusCode::CONFLICT, "file d'attente pleine"));
    }
    items.truncate(FILE_MAX - s.queue.len());
    let mut n = s.clone();
    n.queue.extend(items);
    Ok(Some(n))
}

pub fn retirer(s: &Etat, uid: &str, id: &str) -> Resultat {
    let mut n = s.clone();
    n.queue = Vec::new();
    let mut trouve = false;
    for it in &s.queue {
        if it.id == id {
            if it.by_id != uid && !peut_piloter(s, uid) {
                return Err(refus(
                    StatusCode::FORBIDDEN,
                    "seul l'hôte retire les vidéos des autres",
                ));
            }
            trouve = true;
            continue;
        }
        n.queue.push(it.clone());
    }
    if !trouve {
        return Err(refus(
            StatusCode::NOT_FOUND,
            "vidéo introuvable dans la file",
        ));
    }
    Ok(Some(n))
}

pub fn reordonner(s: &Etat, uid: &str, ids: &[String]) -> Resultat {
    if !peut_piloter(s, uid) {
        return Err(refus(StatusCode::FORBIDDEN, SEUL_HOTE));
    }
    let mut par_id: HashMap<&str, &Item> = s.queue.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut n = s.clone();
    n.queue = Vec::new();
    for id in ids {
        let Some(it) = par_id.remove(id.as_str()) else {
            return Err(refus(StatusCode::CONFLICT, "la file a changé"));
        };
        n.queue.push(it.clone());
    }
    if !par_id.is_empty() {
        return Err(refus(StatusCode::CONFLICT, "la file a changé"));
    }
    Ok(Some(n))
}

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Etat>>);

impl Sessions {
    fn verrou(&self) -> std::sync::MutexGuard<'_, HashMap<String, Etat>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn appliquer(
        &self,
        salon: &str,
        besoin: bool,
        f: impl FnOnce(Option<&Etat>, SystemTime) -> Resultat,
    ) -> Resultat {
        let mut m = self.verrou();
        let cur = m.get(salon);
        if besoin && cur.is_none() {
            return Err(refus(StatusCode::NOT_FOUND, "aucune session dans ce salon"));
        }
        match f(cur, SystemTime::now())? {
            None => {
                m.remove(salon);
                Ok(None)
            }
            Some(mut n) => {
                n.rev += 1;
                m.insert(salon.to_string(), n.clone());
                Ok(Some(n))
            }
        }
    }

    pub fn etat(&self, salon: &str) -> Option<Etat> {
        self.verrou().get(salon).cloned()
    }

    pub fn existe(&self, salon: &str) -> bool {
        self.verrou().contains_key(salon)
    }

    pub fn parmi(&self, salons: &[String]) -> Vec<(String, Etat)> {
        let m = self.verrou();
        salons
            .iter()
            .filter_map(|s| m.get(s).map(|e| (s.clone(), e.clone())))
            .collect()
    }

    pub fn quitter(&self, salon: &str, membre: &str, restants: &[String]) -> Option<Option<Etat>> {
        let mut m = self.verrou();
        let s = m.get(salon)?;
        let Some(premier) = restants.first() else {
            m.remove(salon);
            return Some(None);
        };
        let mut n = s.clone();
        n.ready.remove(membre);
        if s.host == membre {
            n.host.clone_from(premier);
        }
        n.rev = s.rev + 1;
        m.insert(salon.to_string(), n.clone());
        Some(Some(n))
    }
}

pub fn vue(etat: Option<&Etat>, now: SystemTime) -> Value {
    json!({ "state": etat, "now": heure(now) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn it(id: &str, video: &str, par: &str) -> Item {
        Item {
            id: id.into(),
            provider: "youtube".into(),
            video: video.into(),
            by: par.into(),
            by_id: par.into(),
            meta: None,
        }
    }

    fn film(id: &str, par: &str) -> Item {
        Item {
            provider: PROVIDER_FILM.into(),
            ..it(id, id, par)
        }
    }

    #[test]
    fn un_film_est_toujours_pilote_par_celui_qui_le_lance() {
        let e = demarrer(None, &u("a"), vec![film("x", "a")], None, t(100))
            .unwrap()
            .unwrap();
        assert!(e.host_only);
        assert_eq!(e.host, "a");
        assert_eq!(
            options(&e, "a", Some(false), None).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        for action in ["play", "pause", "seek"] {
            let a = Action {
                action: action.into(),
                position: 1.0,
                rate: 0.0,
                rev: e.rev,
                video: String::new(),
            };
            assert_eq!(
                agir(&e, &u("b"), &a, t(101)).unwrap_err().statut,
                StatusCode::FORBIDDEN,
                "{action}"
            );
            assert!(agir(&e, &u("a"), &a, t(101)).is_ok(), "{action}");
        }
        assert_eq!(
            ajouter(&e, vec![film("y", "b")]).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert!(ajouter(&e, vec![film("y", "a")]).is_ok());
    }

    #[test]
    fn lancer_un_film_sur_une_session_ouverte_change_d_hote() {
        let ouverte = demarrer(None, &u("a"), vec![it("1", "v", "a")], None, t(100))
            .unwrap()
            .unwrap();
        assert!(!ouverte.host_only);
        let e = demarrer(Some(&ouverte), &u("b"), vec![film("x", "b")], None, t(101))
            .unwrap()
            .unwrap();
        assert_eq!(e.host, "b");
        assert!(e.host_only);
        let retour = demarrer(Some(&e), &u("b"), vec![it("2", "w", "b")], None, t(102))
            .unwrap()
            .unwrap();
        assert!(!retour.host_only, "une vidéo ordinaire rouvre le pilotage");
    }

    #[test]
    fn episode_suivant_par_la_file() {
        let mut e = demarrer(
            None,
            &u("a"),
            vec![film("e1", "a"), film("e2", "a"), film("e3", "a")],
            None,
            t(100),
        )
        .unwrap()
        .unwrap();
        e.rev = 1;
        assert_eq!(e.video, "e1");
        assert_eq!(e.queue.len(), 2);
        let fin = Action {
            action: "ended".into(),
            position: 0.0,
            rate: 0.0,
            rev: 1,
            video: String::new(),
        };
        assert_eq!(
            agir(&e, &u("b"), &fin, t(200)).unwrap_err().statut,
            StatusCode::FORBIDDEN,
            "un spectateur ne fait pas avancer la séance"
        );
        let n = agir(&e, &u("a"), &fin, t(200)).unwrap().unwrap();
        assert_eq!(n.video, "e2");
        assert_eq!(n.provider, PROVIDER_FILM);
        assert!(n.playing);
        assert_eq!(n.pos, 0.0);
        assert!(n.host_only);
    }

    fn u(id: &str) -> Utilisateur<'_> {
        Utilisateur { id, pseudo: id }
    }

    fn t(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(s)
    }

    fn demarre(items: Vec<Item>) -> Etat {
        let mut e = demarrer(None, &u("a"), items, None, t(100))
            .unwrap()
            .unwrap();
        e.rev = 1;
        e
    }

    fn act(nom: &str, position: f64) -> Action {
        Action {
            action: nom.into(),
            position,
            rate: 0.0,
            rev: 0,
            video: String::new(),
        }
    }

    #[test]
    fn lancer_file_cree_ou_complete_la_session() {
        let tous: Vec<Item> = (0..60).map(|i| it(&i.to_string(), "v", "a")).collect();
        let e = lancer_file(None, &u("a"), tous.clone(), t(100))
            .unwrap()
            .unwrap();
        assert_eq!(
            (e.video.as_str(), e.queue.len(), e.playing),
            ("v", FILE_MAX, true)
        );
        assert_eq!(
            (e.host.as_str(), e.act.as_str(), e.rate),
            ("a", "video", 1.0)
        );
        let suite = lancer_file(Some(&e), &u("b"), vec![it("x", "w", "b")], t(200))
            .unwrap()
            .unwrap();
        assert_eq!(
            (suite.queue.len(), suite.host.as_str(), suite.at),
            (FILE_MAX, "a", e.at)
        );
        let court = demarre(vec![it("1", "v", "a")]);
        let suite = lancer_file(Some(&court), &u("b"), vec![it("x", "w", "b")], t(200))
            .unwrap()
            .unwrap();
        assert_eq!(suite.queue.last().map(|i| i.id.as_str()), Some("x"));
        assert!(lancer_file(None, &u("a"), vec![], t(1)).is_err());
    }

    #[test]
    fn heure_rfc3339_comme_go() {
        assert_eq!(heure(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            heure(UNIX_EPOCH + Duration::new(1_709_210_096, 120_000_000)),
            "2024-02-29T12:34:56.12Z"
        );
        assert_eq!(
            heure(UNIX_EPOCH + Duration::new(4_102_444_799, 1)),
            "2099-12-31T23:59:59.000000001Z"
        );
    }

    #[test]
    fn demarrage_un_album_met_les_suivants_devant_la_file() {
        let e = demarrer(
            None,
            &u("a"),
            vec![it("1", "v1", "a"), it("2", "v2", "a"), it("3", "v3", "a")],
            Some(vec![it("4", "v4", "a")]),
            t(5),
        )
        .unwrap()
        .unwrap();
        assert_eq!(e.video, "v1");
        assert!(e.playing && e.rate == 1.0 && e.pos == 0.0);
        assert_eq!(e.host, "a");
        let ids: Vec<_> = e.queue.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["2", "3", "4"]);
        assert_eq!(e.act, "video");
    }

    #[test]
    fn demarrage_par_un_autre_que_l_hote_refuse_si_hote_seul() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.host_only = true;
        let r = demarrer(Some(&e), &u("b"), vec![it("2", "v2", "b")], None, t(1));
        assert_eq!(r.unwrap_err().statut, StatusCode::FORBIDDEN);
        let n = demarrer(Some(&e), &u("a"), vec![it("2", "v2", "a")], None, t(1))
            .unwrap()
            .unwrap();
        assert_eq!(n.video, "v2");
        assert!(n.host_only);
    }

    #[test]
    fn pause_play_seek_et_rate() {
        let e = demarre(vec![it("1", "v1", "a")]);
        let p = agir(&e, &u("b"), &act("pause", 42.0), t(110))
            .unwrap()
            .unwrap();
        assert!(!p.playing);
        assert_eq!(p.pos, 42.0);
        assert_eq!(p.by_id, "b");
        assert_eq!(p.act, "pause");
        let s = agir(&p, &u("b"), &act("seek", 7.0), t(111))
            .unwrap()
            .unwrap();
        assert_eq!(s.pos, 7.0);
        assert!(!s.playing);
        let mut lecture = act("play", 10.0);
        lecture.video = "v1".into();
        let l = agir(&s, &u("b"), &lecture, t(120)).unwrap().unwrap();
        assert!(l.playing);
        let mut vitesse = act("rate", 0.0);
        vitesse.rate = 1.5;
        let r = agir(&l, &u("b"), &vitesse, t(130)).unwrap().unwrap();
        assert_eq!(r.pos, 20.0);
        assert_eq!(r.rate, 1.5);
        assert_eq!(r.position(t(140)), 35.0);
        vitesse.rate = 9.0;
        assert_eq!(
            agir(&l, &u("b"), &vitesse, t(130)).unwrap_err().message,
            "vitesse invalide"
        );
        assert_eq!(
            agir(&l, &u("b"), &act("danse", 0.0), t(130))
                .unwrap_err()
                .statut,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn commande_sur_une_autre_video_rejetee() {
        let e = demarre(vec![it("1", "v1", "a")]);
        let mut a = act("seek", 30.0);
        a.video = "autre".into();
        assert_eq!(
            agir(&e, &u("a"), &a, t(1)).unwrap_err().statut,
            StatusCode::CONFLICT
        );
        a.video = "v1".into();
        assert_eq!(agir(&e, &u("a"), &a, t(1)).unwrap().unwrap().pos, 30.0);
    }

    #[test]
    fn hote_seul_bloque_les_autres_sauf_ended() {
        let mut e = demarre(vec![it("1", "v1", "a"), it("2", "v2", "a")]);
        e.host_only = true;
        assert_eq!(
            agir(&e, &u("b"), &act("pause", 1.0), t(1))
                .unwrap_err()
                .statut,
            StatusCode::FORBIDDEN
        );
        let mut fin = act("ended", 0.0);
        fin.rev = e.rev;
        assert!(agir(&e, &u("b"), &fin, t(1)).is_ok());
    }

    #[test]
    fn ended_avance_une_fois_puis_ignore_la_revision_perimee() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = vec![it("2", "v2", "a"), it("3", "v3", "a")];
        let mut fin = act("ended", 0.0);
        fin.rev = e.rev;
        let mut n = agir(&e, &u("b"), &fin, t(5)).unwrap().unwrap();
        assert_eq!(n.video, "v2");
        assert_eq!(n.history.len(), 1);
        assert_eq!(n.queue.len(), 1);
        assert!(n.playing && n.pos == 0.0 && n.ready.is_empty());
        n.rev = e.rev + 1;
        let ignore = agir(&n, &u("a"), &fin, t(6)).unwrap().unwrap();
        assert_eq!(ignore.video, "v2");
        assert_eq!(ignore.queue.len(), 1);
        assert_eq!(ignore.act, n.act);
    }

    #[test]
    fn ended_file_vide_arrete_la_lecture() {
        let e = demarre(vec![it("1", "v1", "a")]);
        let mut fin = act("ended", 0.0);
        fin.rev = e.rev;
        let n = agir(&e, &u("a"), &fin, t(5)).unwrap().unwrap();
        assert!(!n.playing);
        assert_eq!(n.video, "v1");
        assert_eq!(n.act, "ended");
    }

    #[test]
    fn repetition_d_un_morceau_et_de_la_file() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = vec![it("2", "v2", "a")];
        let mut un = options(&e, "a", None, Some("one")).unwrap().unwrap();
        un.rev = 3;
        let mut fin = act("ended", 0.0);
        fin.rev = 3;
        let r = agir(&un, &u("a"), &fin, t(9)).unwrap().unwrap();
        assert_eq!((r.video.as_str(), r.act.as_str()), ("v1", "repeat"));
        assert!(r.playing);
        assert_eq!(r.queue.len(), 1);

        let mut tout = options(&e, "a", None, Some("all")).unwrap().unwrap();
        for _ in 0..4 {
            fin.rev = tout.rev;
            tout = agir(&tout, &u("a"), &fin, t(9)).unwrap().unwrap();
            tout.rev += 1;
        }
        assert_eq!(tout.video, "v1");
        assert!(tout.playing);
        assert_eq!(tout.queue.len() + tout.history.len(), 1);
    }

    #[test]
    fn precedent_reprend_l_historique_ou_revient_au_debut() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = vec![it("2", "v2", "a")];
        let debut = agir(&e, &u("a"), &act("previous", 0.0), t(5))
            .unwrap()
            .unwrap();
        assert_eq!((debut.video.as_str(), debut.act.as_str()), ("v1", "seek"));
        assert!(debut.playing && debut.pos == 0.0);

        let mut fin = act("ended", 0.0);
        fin.rev = e.rev;
        let mut suivant = agir(&e, &u("a"), &fin, t(5)).unwrap().unwrap();
        suivant.rev += 1;
        assert_eq!(suivant.video, "v2");
        let retour = agir(&suivant, &u("a"), &act("previous", 0.0), t(6))
            .unwrap()
            .unwrap();
        assert_eq!(retour.video, "v1");
        assert!(retour.history.is_empty());
        assert_eq!(retour.queue.len(), 1);
        assert_eq!(retour.queue[0].video, "v2");
    }

    #[test]
    fn melange_garde_les_memes_elements() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = (0..10).map(|i| it(&i.to_string(), "v", "a")).collect();
        let n = agir(&e, &u("a"), &act("shuffle", 0.0), t(5))
            .unwrap()
            .unwrap();
        let mut avant: Vec<_> = e.queue.iter().map(|i| i.id.clone()).collect();
        let mut apres: Vec<_> = n.queue.iter().map(|i| i.id.clone()).collect();
        avant.sort();
        apres.sort();
        assert_eq!(avant, apres);
    }

    #[test]
    fn pret_ne_change_pas_la_revision() {
        let s = Sessions::default();
        s.appliquer("c", false, |_, now| {
            demarrer(None, &u("a"), vec![it("1", "v1", "a")], None, now)
        })
        .unwrap();
        let avant = s.etat("c").unwrap();
        let n = s
            .appliquer("c", true, |e, _| pret(e.unwrap(), "b", true))
            .unwrap()
            .unwrap();
        assert_eq!(n.rev, avant.rev);
        assert_eq!(n.ready.get("b"), Some(&true));
    }

    #[test]
    fn options_hote_et_repetition() {
        let e = demarre(vec![it("1", "v1", "a")]);
        assert_eq!(
            options(&e, "b", Some(true), None).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        let h = options(&e, "a", Some(true), None).unwrap().unwrap();
        assert!(h.host_only);
        assert_eq!(
            options(&h, "b", None, Some("one")).unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            options(&e, "a", None, Some("toujours"))
                .unwrap_err()
                .message,
            "mode de répétition inconnu"
        );
        let r = options(&e, "a", None, Some("all")).unwrap().unwrap();
        assert_eq!(r.repeat, "all");
        assert!(!r.host_only);
    }

    #[test]
    fn file_d_attente() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = vec![it("2", "v2", "b"), it("3", "v3", "a")];
        assert!(retirer(&e, "b", "3").is_ok());
        e.host_only = true;
        assert_eq!(
            retirer(&e, "b", "3").unwrap_err().statut,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            retirer(&e, "b", "x").unwrap_err().statut,
            StatusCode::NOT_FOUND
        );
        assert_eq!(retirer(&e, "b", "2").unwrap().unwrap().queue.len(), 1);
        assert_eq!(
            reordonner(&e, "b", &["3".into(), "2".into()])
                .unwrap_err()
                .statut,
            StatusCode::FORBIDDEN
        );
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let r = reordonner(&e, "a", &ids(&["3", "2"])).unwrap().unwrap();
        assert_eq!(r.queue[0].id, "3");
        for mauvais in [ids(&["3"]), ids(&["3", "3"]), ids(&["3", "2", "9"])] {
            assert_eq!(
                reordonner(&e, "a", &mauvais).unwrap_err().statut,
                StatusCode::CONFLICT
            );
        }
    }

    #[test]
    fn file_pleine_et_album_tronque() {
        let mut e = demarre(vec![it("1", "v1", "a")]);
        e.queue = (0..48).map(|i| it(&format!("q{i}"), "v", "a")).collect();
        let n = ajouter(&e, (0..5).map(|i| it(&format!("n{i}"), "v", "a")).collect())
            .unwrap()
            .unwrap();
        assert_eq!(n.queue.len(), FILE_MAX);
        assert_eq!(
            ajouter(&n, vec![it("z", "v", "a")]).unwrap_err().statut,
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn depart_du_dernier_arrete_celui_de_l_hote_passe_la_main() {
        let s = Sessions::default();
        s.appliquer("c", false, |_, now| {
            demarrer(None, &u("a"), vec![it("1", "v1", "a")], None, now)
        })
        .unwrap();
        s.appliquer("c", true, |e, _| pret(e.unwrap(), "a", true))
            .unwrap();
        let rev = s.etat("c").unwrap().rev;
        assert_eq!(s.quitter("autre", "a", &[]), None);
        let n = s.quitter("c", "a", &["b".into()]).unwrap().unwrap();
        assert_eq!(n.host, "b");
        assert_eq!(n.rev, rev + 1);
        assert!(n.ready.is_empty());
        assert_eq!(s.quitter("c", "b", &[]), Some(None));
        assert!(!s.existe("c"));
    }

    #[test]
    fn session_requise_sinon_404() {
        let s = Sessions::default();
        let e = s.appliquer("c", true, |_, _| Ok(None)).unwrap_err();
        assert_eq!(e.statut, StatusCode::NOT_FOUND);
    }

    #[test]
    fn une_revision_par_commande_sous_charge() {
        let s = Arc::new(Sessions::default());
        s.appliquer("c", false, |_, now| {
            demarrer(None, &u("a"), vec![it("1", "v1", "a")], None, now)
        })
        .unwrap();
        let base = s.etat("c").unwrap().rev;
        let fils: Vec<_> = (0..10)
            .map(|_| {
                let s = s.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        s.appliquer("c", true, |e, now| {
                            let mut n = e.unwrap().clone();
                            n.playing = i % 2 == 0;
                            n.pos = f64::from(i);
                            n.at = now;
                            Ok(Some(n))
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for f in fils {
            f.join().unwrap();
        }
        assert_eq!(s.etat("c").unwrap().rev, base + 2000);
    }
}
