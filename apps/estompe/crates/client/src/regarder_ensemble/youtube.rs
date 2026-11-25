use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Map, Value};
use url::Url;

use crate::apercu_liens::{url_integree, VideoYoutube};

use super::fournisseurs::{EtatLecture, LecteurSeance};
use super::seance::message_erreur_youtube;

pub const ORIGINE_LECTEUR: &str = "https://www.youtube-nocookie.com";
pub const HOTE_LECTEUR: &str = "www.youtube-nocookie.com";
pub const ECOUTE_MS: i64 = 250;
pub const PAGE_CHARGEE: &str = "charge";
pub const RUNTIME_ABSENT: &str =
    "Le lecteur YouTube n’a pas pu démarrer sur cet appareil : la vidéo s’ouvre dans le navigateur.";
pub const TITRE_VISIONNEUSE: &str = "Vidéo YouTube";
const HOTES_YOUTUBE: [&str; 6] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
    HOTE_LECTEUR,
];

static WIDGETS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InfoYoutube {
    pub pret: bool,
    pub etat: Option<EtatLecture>,
    pub temps: Option<f64>,
    pub duree: Option<f64>,
    pub vitesse: Option<f64>,
    pub erreur: Option<i64>,
    pub bloque: bool,
}

fn nombre(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

fn etat_youtube(code: f64) -> EtatLecture {
    match code {
        0.0 => EtatLecture::Fin,
        1.0 => EtatLecture::Lecture,
        2.0 => EtatLecture::Pause,
        3.0 => EtatLecture::Tampon,
        5.0 => EtatLecture::Amorce,
        _ => EtatLecture::NonDemarre,
    }
}

fn objet_de(texte: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(texte).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

pub fn lire_message(texte: &str) -> Option<InfoYoutube> {
    let m = objet_de(texte)?;
    let info = m.get("info");
    match m.get("event").and_then(Value::as_str)? {
        "onReady" => Some(InfoYoutube {
            pret: true,
            ..InfoYoutube::default()
        }),
        "onStateChange" => nombre(info).map(|c| InfoYoutube {
            etat: Some(etat_youtube(c)),
            ..InfoYoutube::default()
        }),
        "onError" => Some(InfoYoutube {
            erreur: Some(nombre(info).map_or(0, |c| c as i64)),
            ..InfoYoutube::default()
        }),
        "onAutoplayBlocked" => Some(InfoYoutube {
            bloque: true,
            ..InfoYoutube::default()
        }),
        "onPlaybackRateChange" => nombre(info).map(|v| InfoYoutube {
            vitesse: Some(v),
            ..InfoYoutube::default()
        }),
        "initialDelivery" | "infoDelivery" => {
            let i = info.and_then(Value::as_object)?;
            Some(InfoYoutube {
                temps: nombre(i.get("currentTime")),
                duree: nombre(i.get("duration")),
                vitesse: nombre(i.get("playbackRate")),
                etat: nombre(i.get("playerState")).map(etat_youtube),
                ..InfoYoutube::default()
            })
        }
        _ => None,
    }
}

fn origine_de(adresse: &str) -> Option<String> {
    let url = Url::parse(adresse).ok()?;
    let origine = url.origin();
    origine.is_tuple().then(|| origine.ascii_serialization())
}

pub fn message_accepte(source: &str, origine_page: &str) -> bool {
    origine_de(source).is_some_and(|o| o == origine_page)
}

pub fn navigation_permise(adresse: &str, origine_page: &str) -> bool {
    if adresse == "about:blank" {
        return true;
    }
    let Ok(url) = Url::parse(adresse) else {
        return false;
    };
    if origine_de(adresse).is_some_and(|o| o == origine_page) {
        return true;
    }
    url.scheme() == "https" && url.host_str() == Some(HOTE_LECTEUR) && url.port().is_none()
}

pub fn ouverture_externe(adresse: &str) -> Option<String> {
    let url = Url::parse(adresse).ok()?;
    let hote = url.host_str()?;
    (url.scheme() == "https" && HOTES_YOUTUBE.contains(&hote)).then(|| url.to_string())
}

pub fn script_envoi(message: &str) -> String {
    format!(
        "window.estompe&&window.estompe({})",
        Value::String(message.to_string())
    )
}

fn echapper(texte: &str) -> String {
    texte
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLecteur {
    pub video: String,
    pub depart: f64,
    pub lecture: bool,
    pub pilotee: bool,
    pub widget: u64,
}

impl PageLecteur {
    pub fn visionneuse(video: &VideoYoutube) -> Self {
        Self {
            video: video.id.clone(),
            depart: video.debut as f64,
            lecture: true,
            pilotee: false,
            widget: 0,
        }
    }

    pub fn adresse(&self, origine_page: &str) -> String {
        if !self.pilotee {
            return url_integree(&VideoYoutube {
                id: self.video.clone(),
                debut: self.depart.max(0.0).floor() as u64,
            });
        }
        let mut url = Url::parse(ORIGINE_LECTEUR).expect("adresse du lecteur");
        url.path_segments_mut()
            .expect("chemin du lecteur")
            .push("embed")
            .push(&self.video);
        url.query_pairs_mut()
            .append_pair("enablejsapi", "1")
            .append_pair("origin", origine_page)
            .append_pair("widgetid", &self.widget.to_string())
            .append_pair("playsinline", "1")
            .append_pair("rel", "0")
            .append_pair("controls", "0")
            .append_pair("disablekb", "1")
            .append_pair("fs", "0")
            .append_pair("start", &(self.depart.max(0.0).floor() as u64).to_string())
            .append_pair("autoplay", if self.lecture { "1" } else { "0" });
        url.to_string()
    }

    pub fn html(&self, origine_page: &str) -> String {
        let script = if self.pilotee {
            format!(
                "<script>const f=document.getElementById('f');const O='{ORIGINE_LECTEUR}';\
addEventListener('message',e=>{{if(e.origin===O&&e.source===f.contentWindow)window.ipc.postMessage(typeof e.data==='string'?e.data:JSON.stringify(e.data))}});\
window.estompe=m=>{{if(f.contentWindow)f.contentWindow.postMessage(m,O)}};\
f.addEventListener('load',()=>window.ipc.postMessage(JSON.stringify({{estompe:'{PAGE_CHARGEE}'}})));</script>"
            )
        } else {
            String::new()
        };
        let scripts = if self.pilotee {
            "'unsafe-inline'"
        } else {
            "'none'"
        };
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; frame-src {ORIGINE_LECTEUR}; script-src {scripts}; style-src 'unsafe-inline'\">\
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}iframe{{display:block;width:100%;height:100%;border:0}}</style></head>\
<body><iframe id=\"f\" src=\"{}\" title=\"Lecteur YouTube\" allow=\"autoplay; encrypted-media; picture-in-picture; fullscreen\" allowfullscreen referrerpolicy=\"strict-origin-when-cross-origin\"></iframe>{script}</body></html>",
            echapper(&self.adresse(origine_page))
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Entrant {
    Chargee,
    Info(InfoYoutube),
}

#[derive(Debug, Default)]
struct EtatLien {
    page: Option<PageLecteur>,
    chargee: bool,
    sortants: Vec<String>,
    entrants: VecDeque<Entrant>,
    indisponible: Option<String>,
    ferme: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LienVue(Arc<Mutex<EtatLien>>);

impl LienVue {
    pub fn nouveau() -> Self {
        Self::default()
    }

    fn verrou(&self) -> MutexGuard<'_, EtatLien> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn meme(&self, autre: &LienVue) -> bool {
        Arc::ptr_eq(&self.0, &autre.0)
    }

    fn ouvrir(&self, page: PageLecteur) {
        let mut e = self.verrou();
        e.page = Some(page);
        e.chargee = false;
        e.sortants.clear();
    }

    fn envoyer(&self, message: String) {
        self.verrou().sortants.push(message);
    }

    fn fermer(&self) {
        let mut e = self.verrou();
        e.ferme = true;
        e.page = None;
        e.sortants.clear();
    }

    fn entrants(&self) -> Vec<Entrant> {
        self.verrou().entrants.drain(..).collect()
    }

    fn prendre_indisponible(&self) -> Option<String> {
        self.verrou().indisponible.take()
    }

    pub fn page_a_ouvrir(&self) -> Option<PageLecteur> {
        self.verrou().page.take()
    }

    pub fn sortants(&self) -> Vec<String> {
        std::mem::take(&mut self.verrou().sortants)
    }

    pub fn chargee(&self) -> bool {
        self.verrou().chargee
    }

    pub fn ferme(&self) -> bool {
        self.verrou().ferme
    }

    pub fn recevoir(&self, source: &str, origine_page: &str, texte: &str) -> bool {
        if !message_accepte(source, origine_page) {
            return false;
        }
        let entrant = match objet_de(texte) {
            Some(m) if m.get("estompe").and_then(Value::as_str) == Some(PAGE_CHARGEE) => {
                Entrant::Chargee
            }
            _ => match lire_message(texte) {
                Some(i) => Entrant::Info(i),
                None => return false,
            },
        };
        let mut e = self.verrou();
        if e.ferme {
            return false;
        }
        if entrant == Entrant::Chargee {
            e.chargee = true;
        }
        e.entrants.push_back(entrant);
        true
    }

    pub fn signaler_indisponible(&self, raison: &str) {
        let mut e = self.verrou();
        e.indisponible = Some(raison.to_string());
        e.page = None;
        e.sortants.clear();
    }
}

pub struct LecteurYoutube {
    lien: LienVue,
    widget: u64,
    page: bool,
    chargee: bool,
    pret: bool,
    attente: Vec<(String, Value)>,
    derniere_ecoute: Option<i64>,
    etat: EtatLecture,
    temps: f64,
    duree: f64,
    vitesse: f64,
    etats: VecDeque<EtatLecture>,
    erreurs: VecDeque<i64>,
}

impl LecteurYoutube {
    pub fn nouveau(lien: LienVue) -> Self {
        Self {
            lien,
            widget: WIDGETS.fetch_add(1, Ordering::Relaxed),
            page: false,
            chargee: false,
            pret: false,
            attente: Vec::new(),
            derniere_ecoute: None,
            etat: EtatLecture::NonDemarre,
            temps: 0.0,
            duree: 0.0,
            vitesse: 1.0,
            etats: VecDeque::new(),
            erreurs: VecDeque::new(),
        }
    }

    pub fn lien(&self) -> &LienVue {
        &self.lien
    }

    pub fn widget(&self) -> u64 {
        self.widget
    }

    fn poster(&self, mut message: Map<String, Value>) {
        message.insert("id".into(), json!(self.widget));
        message.insert("channel".into(), json!("widget"));
        self.lien.envoyer(Value::Object(message).to_string());
    }

    fn envoyer(&self, fonction: &str, arguments: Value) {
        let mut m = Map::new();
        m.insert("event".into(), json!("command"));
        m.insert("func".into(), json!(fonction));
        m.insert("args".into(), arguments);
        self.poster(m);
    }

    fn appeler(&mut self, fonction: &str, arguments: Value) {
        if self.pret {
            self.envoyer(fonction, arguments);
        } else {
            self.attente.push((fonction.to_string(), arguments));
        }
    }

    fn ecouter(&mut self, maintenant: i64) {
        let mut m = Map::new();
        m.insert("event".into(), json!("listening"));
        self.poster(m);
        self.derniere_ecoute = Some(maintenant);
    }

    fn absorber(&mut self, maintenant: i64) {
        for entrant in self.lien.entrants() {
            match entrant {
                Entrant::Chargee => {
                    if !self.pret {
                        self.chargee = true;
                        self.ecouter(maintenant);
                    }
                }
                Entrant::Info(i) => {
                    if i.pret && !self.pret {
                        self.pret = true;
                        for (f, a) in std::mem::take(&mut self.attente) {
                            self.envoyer(&f, a);
                        }
                    }
                    if let Some(t) = i.temps {
                        self.temps = t;
                    }
                    if let Some(d) = i.duree {
                        self.duree = d;
                    }
                    if let Some(v) = i.vitesse {
                        self.vitesse = v;
                    }
                    if let Some(s) = i.etat {
                        if s != self.etat {
                            self.etat = s;
                            self.etats.push_back(s);
                        }
                    }
                    if let Some(code) = i.erreur {
                        self.erreurs.push_back(code);
                    }
                }
            }
        }
        if self.chargee
            && !self.pret
            && self
                .derniere_ecoute
                .is_some_and(|t| maintenant - t >= ECOUTE_MS)
        {
            self.ecouter(maintenant);
        }
    }
}

impl LecteurSeance for LecteurYoutube {
    fn charger(&mut self, reference: &str, depart: f64, lecture: bool) {
        self.temps = depart;
        if !self.page {
            self.page = true;
            self.lien.ouvrir(PageLecteur {
                video: reference.to_string(),
                depart,
                lecture,
                pilotee: true,
                widget: self.widget,
            });
            return;
        }
        let fonction = if lecture {
            "loadVideoById"
        } else {
            "cueVideoById"
        };
        self.appeler(
            fonction,
            json!([{ "videoId": reference, "startSeconds": depart }]),
        );
    }

    fn lire(&mut self) {
        self.appeler("playVideo", json!([]));
    }

    fn pause(&mut self) {
        self.appeler("pauseVideo", json!([]));
    }

    fn aller(&mut self, position: f64) {
        self.temps = position;
        self.appeler("seekTo", json!([position, true]));
    }

    fn temps(&self) -> f64 {
        self.temps
    }

    fn vitesse(&self) -> f64 {
        self.vitesse
    }

    fn regler_vitesse(&mut self, vitesse: f64) {
        self.appeler("setPlaybackRate", json!([vitesse]));
    }

    fn etat(&self) -> EtatLecture {
        self.etat
    }

    fn duree(&self) -> f64 {
        self.duree
    }

    fn regler_volume(&mut self, volume: u8) {
        self.appeler("setVolume", json!([volume]));
    }

    fn suivre(&mut self, maintenant_ms: i64) -> Option<EtatLecture> {
        self.absorber(maintenant_ms);
        self.etats.pop_front()
    }

    fn erreur(&mut self) -> Option<String> {
        self.erreurs
            .pop_front()
            .map(|code| message_erreur_youtube(code).to_string())
    }

    fn repli(&mut self) -> Option<String> {
        self.lien.prendre_indisponible()
    }

    fn detruire(&mut self) {
        self.lien.fermer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apercu_liens::video_youtube;

    const PAGE: &str = "https://estompe.localhost";
    const SOURCE: &str = "https://estompe.localhost/lecteur";

    fn info(etat: Option<EtatLecture>) -> InfoYoutube {
        InfoYoutube {
            etat,
            ..InfoYoutube::default()
        }
    }

    #[test]
    fn les_messages_du_lecteur_sont_lus_comme_dans_le_front() {
        assert_eq!(
            lire_message(r#"{"event":"onReady","id":1,"channel":"widget"}"#),
            Some(InfoYoutube {
                pret: true,
                ..InfoYoutube::default()
            })
        );
        assert_eq!(
            lire_message(r#"{"event":"onStateChange","info":1}"#),
            Some(info(Some(EtatLecture::Lecture)))
        );
        assert_eq!(
            lire_message(
                r#"{"event":"infoDelivery","info":{"currentTime":12.5,"duration":90,"playbackRate":1.25,"playerState":2}}"#
            ),
            Some(InfoYoutube {
                temps: Some(12.5),
                duree: Some(90.0),
                vitesse: Some(1.25),
                etat: Some(EtatLecture::Pause),
                ..InfoYoutube::default()
            })
        );
        assert_eq!(
            lire_message(r#"{"event":"infoDelivery","info":{"currentTime":"x","videoData":{}}}"#),
            Some(InfoYoutube::default())
        );
        assert_eq!(
            lire_message(r#"{"event":"onError","info":150}"#),
            Some(InfoYoutube {
                erreur: Some(150),
                ..InfoYoutube::default()
            })
        );
        assert_eq!(
            lire_message(r#"{"event":"onAutoplayBlocked"}"#),
            Some(InfoYoutube {
                bloque: true,
                ..InfoYoutube::default()
            })
        );
        for x in [
            "pas du json",
            "[1]",
            "null",
            "42",
            r#"{"event":"inconnu"}"#,
            r#"{"event":"onStateChange","info":"1"}"#,
        ] {
            assert_eq!(lire_message(x), None, "{x}");
        }
    }

    #[test]
    fn chaque_etat_du_lecteur_youtube_a_son_equivalent() {
        let etat = |c: i64| {
            lire_message(&format!(r#"{{"event":"onStateChange","info":{c}}}"#)).and_then(|i| i.etat)
        };
        assert_eq!(etat(-1), Some(EtatLecture::NonDemarre));
        assert_eq!(etat(0), Some(EtatLecture::Fin));
        assert_eq!(etat(1), Some(EtatLecture::Lecture));
        assert_eq!(etat(2), Some(EtatLecture::Pause));
        assert_eq!(etat(3), Some(EtatLecture::Tampon));
        assert_eq!(etat(5), Some(EtatLecture::Amorce));
        assert_eq!(etat(4), Some(EtatLecture::NonDemarre));
        assert_eq!(
            lire_message(r#"{"event":"onPlaybackRateChange","info":2}"#).and_then(|i| i.vitesse),
            Some(2.0)
        );
        assert_eq!(
            lire_message(r#"{"event":"onError"}"#).and_then(|i| i.erreur),
            Some(0)
        );
    }

    #[test]
    fn la_vue_ne_navigue_que_vers_sa_page_et_le_lecteur_youtube() {
        for permise in [
            "https://estompe.localhost/lecteur",
            "https://estompe.localhost/",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?enablejsapi=1",
            "about:blank",
        ] {
            assert!(navigation_permise(permise, PAGE), "{permise}");
        }
        for refusee in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://accounts.google.com/ServiceLogin",
            "http://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com:8443/embed/x",
            "https://www.youtube-nocookie.com.evil.test/embed/x",
            "https://evil.test/?https://www.youtube-nocookie.com",
            "http://estompe.localhost/lecteur",
            "https://estompe.localhost.evil.test/",
            "file:///C:/Users/moi/AppData/Roaming/Estompe/session.json",
            "estompe://localhost/lecteur",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "",
            "pas une adresse",
        ] {
            assert!(!navigation_permise(refusee, PAGE), "{refusee}");
        }
    }

    #[test]
    fn une_nouvelle_fenetre_n_ouvre_que_youtube_dans_le_navigateur() {
        assert_eq!(
            ouverture_externe("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert!(ouverture_externe("https://youtu.be/dQw4w9WgXcQ").is_some());
        assert_eq!(ouverture_externe("https://evil.test/"), None);
        assert_eq!(ouverture_externe("http://www.youtube.com/"), None);
        assert_eq!(ouverture_externe("javascript:alert(1)"), None);
    }

    #[test]
    fn seuls_les_messages_de_la_page_sont_acceptes() {
        let lien = LienVue::nouveau();
        assert!(!lien.recevoir("https://evil.test/", PAGE, r#"{"event":"onReady"}"#));
        assert!(!lien.recevoir(
            "https://www.youtube-nocookie.com/embed/x",
            PAGE,
            r#"{"event":"onReady"}"#
        ));
        assert!(!lien.recevoir(SOURCE, PAGE, "pas du json"));
        assert!(!lien.recevoir(SOURCE, PAGE, r#"{"event":"inconnu"}"#));
        assert!(!lien.chargee());
        assert!(lien.recevoir(SOURCE, PAGE, r#"{"estompe":"charge"}"#));
        assert!(lien.chargee());
        assert!(lien.recevoir(SOURCE, PAGE, r#"{"event":"onReady"}"#));
        assert_eq!(lien.entrants().len(), 2);
    }

    #[test]
    fn la_page_pilotee_n_embarque_que_l_iframe_du_lecteur_avec_son_referent() {
        let page = PageLecteur {
            video: "dQw4w9WgXcQ".into(),
            depart: 30.7,
            lecture: true,
            pilotee: true,
            widget: 7,
        };
        let src = Url::parse(&page.adresse(PAGE)).unwrap();
        let q = |k: &str| {
            src.query_pairs()
                .find(|(c, _)| c == k)
                .map(|(_, v)| v.into_owned())
        };
        assert_eq!(src.origin().ascii_serialization(), ORIGINE_LECTEUR);
        assert_eq!(src.path(), "/embed/dQw4w9WgXcQ");
        assert_eq!(
            [
                q("enablejsapi"),
                q("start"),
                q("autoplay"),
                q("origin"),
                q("widgetid"),
                q("controls")
            ],
            [
                Some("1".into()),
                Some("30".into()),
                Some("1".into()),
                Some(PAGE.into()),
                Some("7".into()),
                Some("0".into())
            ]
        );
        let html = page.html(PAGE);
        assert!(html.contains("referrerpolicy=\"strict-origin-when-cross-origin\""));
        assert!(html.contains(&format!("frame-src {ORIGINE_LECTEUR};")));
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("e.origin===O&&e.source===f.contentWindow"));
        assert!(html.contains("&amp;origin=https%3A%2F%2Festompe.localhost"));
        assert_eq!(html.matches("<script").count(), 1);
        assert!(!html.contains("<script src"));
        let piege = PageLecteur {
            video: "\"><script>alert(1)</script>".into(),
            ..page
        };
        assert!(!piege.html(PAGE).contains("\"><script>alert"));
    }

    #[test]
    fn la_visionneuse_reprend_l_adresse_de_la_carte_sans_script() {
        let video = video_youtube("https://youtu.be/dQw4w9WgXcQ?t=90").unwrap();
        let page = PageLecteur::visionneuse(&video);
        assert_eq!(page.adresse(PAGE), url_integree(&video));
        let html = page.html(PAGE);
        assert!(!html.contains("<script"));
        assert!(html.contains("script-src 'none'"));
        assert!(html.contains("referrerpolicy=\"strict-origin-when-cross-origin\""));
    }

    #[test]
    fn un_message_part_en_chaine_javascript() {
        assert_eq!(
            script_envoi(r#"{"event":"command","func":"x'</script>"}"#),
            r#"window.estompe&&window.estompe("{\"event\":\"command\",\"func\":\"x'</script>\"}")"#
        );
    }

    fn envoyes(lien: &LienVue) -> Vec<Value> {
        lien.sortants()
            .iter()
            .map(|m| serde_json::from_str(m).unwrap())
            .collect()
    }

    #[test]
    fn commandes_gardees_jusqu_a_on_ready_puis_etat_position_et_erreurs() {
        let lien = LienVue::nouveau();
        let mut l = LecteurYoutube::nouveau(lien.clone());
        l.charger("dQw4w9WgXcQ", 30.0, true);
        let page = lien.page_a_ouvrir().expect("page demandée");
        assert_eq!(
            (page.video.as_str(), page.depart, page.lecture, page.pilotee),
            ("dQw4w9WgXcQ", 30.0, true, true)
        );
        assert_eq!(page.widget, l.widget());
        assert_eq!(lien.page_a_ouvrir(), None);
        lien.recevoir(SOURCE, PAGE, r#"{"estompe":"charge"}"#);
        l.pause();
        assert_eq!(l.suivre(0), None);
        let m = envoyes(&lien);
        assert_eq!(m.len(), 1);
        assert_eq!(
            m[0],
            json!({"event": "listening", "id": l.widget(), "channel": "widget"})
        );
        l.suivre(100);
        assert!(lien.sortants().is_empty());
        l.suivre(260);
        assert_eq!(envoyes(&lien)[0]["event"], "listening");
        assert!(!lien.recevoir("https://evil.test/", PAGE, r#"{"event":"onReady"}"#));
        l.suivre(300);
        assert!(lien.sortants().is_empty());
        lien.recevoir(SOURCE, PAGE, r#"{"event":"onReady"}"#);
        l.suivre(320);
        l.aller(42.0);
        let m = envoyes(&lien);
        assert_eq!(
            m.iter()
                .map(|x| (x["func"].clone(), x["args"].clone()))
                .collect::<Vec<_>>(),
            vec![
                (json!("pauseVideo"), json!([])),
                (json!("seekTo"), json!([42.0, true]))
            ]
        );
        assert!(m.iter().all(|x| x["event"] == "command"
            && x["id"] == json!(l.widget())
            && x["channel"] == "widget"));
        l.suivre(900);
        assert!(lien.sortants().is_empty());
        lien.recevoir(
            SOURCE,
            PAGE,
            r#"{"event":"infoDelivery","info":{"currentTime":43,"duration":200,"playerState":1}}"#,
        );
        lien.recevoir(
            SOURCE,
            PAGE,
            r#"{"event":"infoDelivery","info":{"currentTime":44,"playerState":1}}"#,
        );
        let mut etats = Vec::new();
        while let Some(s) = l.suivre(1000) {
            etats.push(s);
        }
        assert_eq!(
            (l.temps(), l.duree(), l.etat(), etats, l.erreur()),
            (
                44.0,
                200.0,
                EtatLecture::Lecture,
                vec![EtatLecture::Lecture],
                None
            )
        );
        l.charger("aaaaaaaaaaa", 5.0, false);
        assert_eq!(lien.page_a_ouvrir(), None);
        let m = envoyes(&lien);
        assert_eq!(m.last().unwrap()["func"], "cueVideoById");
        assert_eq!(
            m.last().unwrap()["args"],
            json!([{"videoId": "aaaaaaaaaaa", "startSeconds": 5.0}])
        );
        lien.recevoir(SOURCE, PAGE, r#"{"event":"onError","info":150}"#);
        l.suivre(1100);
        assert_eq!(l.erreur().as_deref(), Some(message_erreur_youtube(150)));
        assert_eq!(l.erreur(), None);
        l.regler_vitesse(1.5);
        l.regler_volume(0);
        l.lire();
        assert_eq!(
            envoyes(&lien)
                .iter()
                .map(|x| x["func"].clone())
                .collect::<Vec<_>>(),
            vec![
                json!("setPlaybackRate"),
                json!("setVolume"),
                json!("playVideo")
            ]
        );
        l.detruire();
        assert!(lien.ferme());
        assert!(!lien.recevoir(SOURCE, PAGE, r#"{"event":"onReady"}"#));
    }

    #[test]
    fn un_runtime_absent_est_signale_une_fois() {
        let lien = LienVue::nouveau();
        let mut l = LecteurYoutube::nouveau(lien.clone());
        l.charger("dQw4w9WgXcQ", 0.0, false);
        assert!(lien.page_a_ouvrir().is_some());
        lien.signaler_indisponible(RUNTIME_ABSENT);
        assert_eq!(l.repli().as_deref(), Some(RUNTIME_ABSENT));
        assert_eq!(l.repli(), None);
    }

    #[test]
    fn deux_lecteurs_ont_des_widgets_et_des_liens_distincts() {
        let (a, b) = (LienVue::nouveau(), LienVue::nouveau());
        assert!(a.meme(&a.clone()) && !a.meme(&b));
        assert_ne!(
            LecteurYoutube::nouveau(a).widget(),
            LecteurYoutube::nouveau(b).widget()
        );
    }
}
