use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use libmpv2_sys as ffi;

use crate::vocal::RecepteurVideo;

use super::etat::Evenement;
use super::image::ImageLecteur;
use super::moteur::{ErreurLecteur, MoteurLecture, Rapport, Source};
use super::pistes::pistes_de_mpv;
use super::reglages::taille_de_rendu;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsMpv {
    pub image: bool,
    pub son: bool,
    pub sortie: Option<String>,
    pub decodage_materiel: bool,
}

impl Default for OptionsMpv {
    fn default() -> Self {
        Self {
            image: true,
            son: true,
            sortie: None,
            decodage_materiel: true,
        }
    }
}

pub fn options_initiales(options: &OptionsMpv) -> Vec<(&'static str, String)> {
    let mut liste: Vec<(&'static str, String)> = [
        ("config", "no"),
        ("terminal", "no"),
        ("idle", "yes"),
        ("keep-open", "yes"),
        ("input-default-bindings", "no"),
        ("input-vo-keyboard", "no"),
        ("osc", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("sub-auto", "no"),
        ("sw-fast", "yes"),
        ("network-timeout", "30"),
        ("user-agent", "Estompe natif"),
        ("audio-client-name", "Estompe"),
    ]
    .into_iter()
    .map(|(n, v)| (n, v.to_string()))
    .collect();
    liste.push(("vo", if options.image { "libmpv" } else { "null" }.into()));
    liste.push((
        "hwdec",
        if options.decodage_materiel {
            "auto-copy-safe"
        } else {
            "no"
        }
        .into(),
    ));
    if !options.son {
        liste.push(("ao", "null".into()));
    }
    if let Some(sortie) = &options.sortie {
        liste.push(("audio-device", sortie.clone()));
    }
    liste
}

#[derive(Debug, Clone, PartialEq)]
pub enum Valeur {
    Rien,
    Drapeau(bool),
    Reel(f64),
    Entier(i64),
    Texte(String),
}

const PROPRIETES: [(&CStr, ffi::mpv_format); 15] = [
    (c"pause", ffi::mpv_format_MPV_FORMAT_FLAG),
    (c"time-pos", ffi::mpv_format_MPV_FORMAT_DOUBLE),
    (c"duration", ffi::mpv_format_MPV_FORMAT_DOUBLE),
    (c"paused-for-cache", ffi::mpv_format_MPV_FORMAT_FLAG),
    (c"seeking", ffi::mpv_format_MPV_FORMAT_FLAG),
    (c"demuxer-cache-time", ffi::mpv_format_MPV_FORMAT_DOUBLE),
    (c"volume", ffi::mpv_format_MPV_FORMAT_DOUBLE),
    (c"mute", ffi::mpv_format_MPV_FORMAT_FLAG),
    (c"speed", ffi::mpv_format_MPV_FORMAT_DOUBLE),
    (c"track-list", ffi::mpv_format_MPV_FORMAT_STRING),
    (c"aid", ffi::mpv_format_MPV_FORMAT_STRING),
    (c"sid", ffi::mpv_format_MPV_FORMAT_STRING),
    (c"dwidth", ffi::mpv_format_MPV_FORMAT_INT64),
    (c"dheight", ffi::mpv_format_MPV_FORMAT_INT64),
    (c"eof-reached", ffi::mpv_format_MPV_FORMAT_FLAG),
];

fn piste_choisie(valeur: Valeur) -> Option<i64> {
    match valeur {
        Valeur::Texte(t) => t.parse().ok(),
        Valeur::Entier(i) => Some(i),
        _ => None,
    }
}

fn dimension(valeur: Valeur) -> u32 {
    match valeur {
        Valeur::Entier(n) => u32::try_from(n.max(0)).unwrap_or(u32::MAX),
        _ => 0,
    }
}

pub fn evenement_de_propriete(nom: &str, valeur: Valeur) -> Option<Evenement> {
    use Valeur::{Drapeau, Reel, Rien, Texte};
    Some(match (nom, valeur) {
        ("pause", Drapeau(b)) => Evenement::EnPause(b),
        ("time-pos", Reel(p)) => Evenement::Position(p),
        ("duration", Reel(d)) => Evenement::Duree(Some(d)),
        ("duration", Rien) => Evenement::Duree(None),
        ("paused-for-cache", Drapeau(b)) => Evenement::Tampon(b),
        ("seeking", Drapeau(b)) => Evenement::Recherche(b),
        ("demuxer-cache-time", Reel(t)) => Evenement::CacheJusqua(Some(t)),
        ("demuxer-cache-time", Rien) => Evenement::CacheJusqua(None),
        ("volume", Reel(v)) => Evenement::Volume(v),
        ("mute", Drapeau(b)) => Evenement::Muet(b),
        ("speed", Reel(v)) => Evenement::Vitesse(v),
        ("track-list", Texte(t)) => Evenement::Pistes(pistes_de_mpv(&t)),
        ("track-list", Rien) => Evenement::Pistes(Vec::new()),
        ("aid", v) => Evenement::PisteAudio(piste_choisie(v)),
        ("sid", v) => Evenement::SousTitres(piste_choisie(v)),
        ("dwidth", v) => Evenement::LargeurVideo(dimension(v)),
        ("dheight", v) => Evenement::HauteurVideo(dimension(v)),
        ("eof-reached", Drapeau(b)) => Evenement::Fin(b),
        ("eof-reached", Rien) => Evenement::Fin(false),
        _ => return None,
    })
}

pub fn message_d_erreur(erreur: &str, journal: Option<&str>) -> String {
    match journal {
        Some(j) if !j.is_empty() => format!("{erreur} ({j})"),
        _ => erreur.to_string(),
    }
}

struct Poignee(*mut ffi::mpv_handle);

unsafe impl Send for Poignee {}
unsafe impl Sync for Poignee {}

#[derive(Clone, Copy)]
struct Contexte(*mut ffi::mpv_render_context);

unsafe impl Send for Contexte {}
unsafe impl Sync for Contexte {}

#[derive(Default)]
struct EtatRendu {
    a_rendre: bool,
    forcer: bool,
    arret: bool,
    actif: bool,
    zone: (u32, u32),
    video: (u32, u32),
}

#[derive(Default)]
struct Rendu {
    etat: Mutex<EtatRendu>,
    signal: Condvar,
}

impl Rendu {
    fn verrou(&self) -> MutexGuard<'_, EtatRendu> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn modifier(&self, f: impl FnOnce(&mut EtatRendu)) {
        f(&mut self.verrou());
        self.signal.notify_all();
    }
}

unsafe extern "C" fn image_a_rendre(donnees: *mut c_void) {
    let rendu = unsafe { &*(donnees as *const Rendu) };
    rendu.modifier(|e| e.a_rendre = true);
}

fn texte_c(texte: &str) -> CString {
    CString::new(texte.replace('\0', "")).unwrap_or_default()
}

fn erreur_mpv(code: c_int) -> String {
    let texte = unsafe { ffi::mpv_error_string(code) };
    if texte.is_null() {
        return format!("erreur {code}");
    }
    unsafe { CStr::from_ptr(texte) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn texte_de(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

unsafe fn lire_valeur(format: ffi::mpv_format, donnees: *mut c_void) -> Valeur {
    if donnees.is_null() {
        return Valeur::Rien;
    }
    unsafe {
        match format {
            ffi::mpv_format_MPV_FORMAT_FLAG => Valeur::Drapeau(*(donnees as *const c_int) != 0),
            ffi::mpv_format_MPV_FORMAT_DOUBLE => Valeur::Reel(*(donnees as *const f64)),
            ffi::mpv_format_MPV_FORMAT_INT64 => Valeur::Entier(*(donnees as *const i64)),
            ffi::mpv_format_MPV_FORMAT_STRING => {
                let p = *(donnees as *const *const c_char);
                if p.is_null() {
                    Valeur::Rien
                } else {
                    Valeur::Texte(texte_de(p))
                }
            }
            _ => Valeur::Rien,
        }
    }
}

pub struct MoteurMpv {
    mpv: Arc<Poignee>,
    rendu: Arc<Rendu>,
    contexte: Option<Contexte>,
    images: Arc<RecepteurVideo>,
    arret: Arc<AtomicBool>,
    fil_evenements: Option<JoinHandle<()>>,
    fil_rendu: Option<JoinHandle<()>>,
}

impl MoteurMpv {
    pub fn nouveau(options: OptionsMpv, rapport: Rapport) -> Result<Self, ErreurLecteur> {
        let h = unsafe { ffi::mpv_create() };
        if h.is_null() {
            return Err(ErreurLecteur::Indisponible(
                "mpv_create a échoué (locale LC_NUMERIC autre que C ?)".into(),
            ));
        }
        let mpv = Arc::new(Poignee(h));
        let detruire = |message: String| {
            unsafe { ffi::mpv_terminate_destroy(h) };
            ErreurLecteur::Indisponible(message)
        };
        for (nom, valeur) in options_initiales(&options) {
            let (n, v) = (texte_c(nom), texte_c(&valeur));
            let code = unsafe { ffi::mpv_set_option_string(h, n.as_ptr(), v.as_ptr()) };
            if code < 0 && nom == "vo" {
                return Err(detruire(format!("option vo : {}", erreur_mpv(code))));
            }
        }
        let code = unsafe { ffi::mpv_initialize(h) };
        if code < 0 {
            return Err(detruire(format!("mpv_initialize : {}", erreur_mpv(code))));
        }
        unsafe { ffi::mpv_request_log_messages(h, c"error".as_ptr()) };
        for (i, (nom, format)) in PROPRIETES.iter().enumerate() {
            unsafe { ffi::mpv_observe_property(h, i as u64, nom.as_ptr(), *format) };
        }
        let rendu = Arc::new(Rendu::default());
        rendu.verrou().actif = true;
        let contexte = if options.image {
            let mut parametres = [
                ffi::mpv_render_param {
                    type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                    data: ffi::MPV_RENDER_API_TYPE_SW.as_ptr() as *mut c_void,
                },
                ffi::mpv_render_param {
                    type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                    data: ptr::null_mut(),
                },
            ];
            let mut ctx: *mut ffi::mpv_render_context = ptr::null_mut();
            let code =
                unsafe { ffi::mpv_render_context_create(&mut ctx, h, parametres.as_mut_ptr()) };
            if code < 0 || ctx.is_null() {
                return Err(detruire(format!("rendu logiciel : {}", erreur_mpv(code))));
            }
            unsafe {
                ffi::mpv_render_context_set_update_callback(
                    ctx,
                    Some(image_a_rendre),
                    Arc::as_ptr(&rendu) as *mut c_void,
                )
            };
            Some(Contexte(ctx))
        } else {
            None
        };
        let images = RecepteurVideo::nouveau();
        let arret = Arc::new(AtomicBool::new(false));
        let fil_rendu = contexte.and_then(|ctx| {
            let (rendu, images) = (rendu.clone(), images.clone());
            std::thread::Builder::new()
                .name("lecteur rendu".into())
                .spawn(move || boucle_rendu(ctx, &rendu, &images))
                .ok()
        });
        let fil_evenements = {
            let (mpv, rendu, arret) = (mpv.clone(), rendu.clone(), arret.clone());
            std::thread::Builder::new()
                .name("lecteur mpv".into())
                .spawn(move || boucle_evenements(&mpv, &arret, &rendu, &rapport))
                .ok()
        };
        Ok(Self {
            mpv,
            rendu,
            contexte,
            images,
            arret,
            fil_evenements,
            fil_rendu,
        })
    }

    fn commande(&self, arguments: &[&str]) {
        let textes: Vec<CString> = arguments.iter().map(|a| texte_c(a)).collect();
        let mut pointeurs: Vec<*const c_char> = textes.iter().map(|t| t.as_ptr()).collect();
        pointeurs.push(ptr::null());
        unsafe { ffi::mpv_command_async(self.mpv.0, 0, pointeurs.as_mut_ptr()) };
    }

    fn propriete_texte(&self, nom: &CStr, valeur: &str) {
        let valeur = texte_c(valeur);
        let mut p = valeur.as_ptr();
        unsafe {
            ffi::mpv_set_property_async(
                self.mpv.0,
                0,
                nom.as_ptr(),
                ffi::mpv_format_MPV_FORMAT_STRING,
                &mut p as *mut *const c_char as *mut c_void,
            )
        };
    }

    fn propriete_drapeau(&self, nom: &CStr, valeur: bool) {
        let mut v = c_int::from(valeur);
        unsafe {
            ffi::mpv_set_property_async(
                self.mpv.0,
                0,
                nom.as_ptr(),
                ffi::mpv_format_MPV_FORMAT_FLAG,
                &mut v as *mut c_int as *mut c_void,
            )
        };
    }

    fn propriete_reel(&self, nom: &CStr, valeur: f64) {
        let mut v = valeur;
        unsafe {
            ffi::mpv_set_property_async(
                self.mpv.0,
                0,
                nom.as_ptr(),
                ffi::mpv_format_MPV_FORMAT_DOUBLE,
                &mut v as *mut f64 as *mut c_void,
            )
        };
    }
}

fn boucle_rendu(contexte: Contexte, rendu: &Rendu, images: &RecepteurVideo) {
    loop {
        let (zone, video, actif, forcer) = {
            let mut e = rendu.verrou();
            while !e.a_rendre && !e.forcer && !e.arret {
                e = rendu.signal.wait(e).unwrap_or_else(|e| e.into_inner());
            }
            if e.arret {
                return;
            }
            e.a_rendre = false;
            (e.zone, e.video, e.actif, std::mem::take(&mut e.forcer))
        };
        let drapeaux = unsafe { ffi::mpv_render_context_update(contexte.0) };
        let neuve = drapeaux & u64::from(ffi::mpv_render_update_flag_MPV_RENDER_UPDATE_FRAME) != 0;
        if !actif || !(neuve || forcer) {
            continue;
        }
        let (largeur, hauteur) = taille_de_rendu(video, zone);
        if largeur == 0 || hauteur == 0 {
            continue;
        }
        let mut image = ImageLecteur::nouvelle(largeur, hauteur);
        let mut taille: [c_int; 2] = [largeur as c_int, hauteur as c_int];
        let mut pas: usize = image.pas_en_octets();
        let mut parametres = [
            ffi::mpv_render_param {
                type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_SIZE,
                data: taille.as_mut_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_FORMAT,
                data: c"rgb0".as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_STRIDE,
                data: &mut pas as *mut usize as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_POINTER,
                data: image.tampon().as_mut_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        let code = unsafe { ffi::mpv_render_context_render(contexte.0, parametres.as_mut_ptr()) };
        if code >= 0 {
            images.pousser(Arc::new(image));
        }
    }
}

fn boucle_evenements(mpv: &Poignee, arret: &AtomicBool, rendu: &Rendu, rapport: &Rapport) {
    let mut journal: Option<String> = None;
    loop {
        let evenement = unsafe { &*ffi::mpv_wait_event(mpv.0, -1.0) };
        if arret.load(Ordering::Acquire) {
            return;
        }
        match evenement.event_id {
            ffi::mpv_event_id_MPV_EVENT_SHUTDOWN => return,
            ffi::mpv_event_id_MPV_EVENT_LOG_MESSAGE if !evenement.data.is_null() => {
                let m = unsafe { &*(evenement.data as *const ffi::mpv_event_log_message) };
                let (prefixe, texte) = unsafe { (texte_de(m.prefix), texte_de(m.text)) };
                journal = Some(format!("{prefixe}: {}", texte.trim()));
            }
            ffi::mpv_event_id_MPV_EVENT_START_FILE => {
                journal = None;
                rapport(Evenement::Chargement);
            }
            ffi::mpv_event_id_MPV_EVENT_FILE_LOADED => rapport(Evenement::Charge),
            ffi::mpv_event_id_MPV_EVENT_END_FILE if !evenement.data.is_null() => {
                let fin = unsafe { &*(evenement.data as *const ffi::mpv_event_end_file) };
                rapport(match fin.reason {
                    ffi::mpv_end_file_reason_MPV_END_FILE_REASON_ERROR => Evenement::Erreur(
                        message_d_erreur(&erreur_mpv(fin.error), journal.as_deref()),
                    ),
                    ffi::mpv_end_file_reason_MPV_END_FILE_REASON_EOF => Evenement::Fin(true),
                    _ => Evenement::Arrete,
                });
            }
            ffi::mpv_event_id_MPV_EVENT_PROPERTY_CHANGE if !evenement.data.is_null() => {
                let p = unsafe { &*(evenement.data as *const ffi::mpv_event_property) };
                let nom = unsafe { texte_de(p.name) };
                let valeur = unsafe { lire_valeur(p.format, p.data) };
                if let Some(e) = evenement_de_propriete(&nom, valeur) {
                    match e {
                        Evenement::LargeurVideo(l) => rendu.modifier(|r| {
                            r.video.0 = l;
                            r.forcer = true;
                        }),
                        Evenement::HauteurVideo(h) => rendu.modifier(|r| {
                            r.video.1 = h;
                            r.forcer = true;
                        }),
                        _ => {}
                    }
                    rapport(e);
                }
            }
            _ => {}
        }
    }
}

impl MoteurLecture for MoteurMpv {
    fn charger(&self, source: &Source) {
        self.commande(&["change-list", "http-header-fields", "clr", ""]);
        for entete in source.entetes_http() {
            self.commande(&["change-list", "http-header-fields", "append", &entete]);
        }
        let depart = if source.depart > 0.0 {
            format!("{:.3}", source.depart)
        } else {
            "none".to_string()
        };
        self.propriete_texte(c"start", &depart);
        self.propriete_drapeau(c"pause", source.en_pause);
        self.commande(&["change-list", "sub-files", "clr", ""]);
        for sous_titres in &source.sous_titres {
            self.commande(&["change-list", "sub-files", "append", sous_titres]);
        }
        self.propriete_reel(c"sub-delay", -source.decalage_sous_titres);
        self.commande(&["loadfile", &source.adresse, "replace"]);
    }

    fn arreter(&self) {
        self.commande(&["stop"]);
    }

    fn mettre_en_pause(&self, pause: bool) {
        self.propriete_drapeau(c"pause", pause);
    }

    fn aller(&self, position: f64) {
        self.commande(&["seek", &format!("{position:.3}"), "absolute"]);
    }

    fn regler_volume(&self, volume: u8, muet: bool) {
        self.propriete_reel(c"volume", f64::from(volume));
        self.propriete_drapeau(c"mute", muet);
    }

    fn regler_vitesse(&self, vitesse: f64) {
        self.propriete_reel(c"speed", vitesse);
    }

    fn choisir_audio(&self, piste: Option<i64>) {
        self.propriete_texte(c"aid", &piste.map_or("no".into(), |p| p.to_string()));
    }

    fn choisir_sous_titres(&self, piste: Option<i64>) {
        self.propriete_texte(c"sid", &piste.map_or("no".into(), |p| p.to_string()));
    }

    fn activer_video(&self, active: bool) {
        self.rendu.modifier(|r| {
            r.actif = active;
            r.forcer = active;
        });
        self.propriete_texte(c"vid", if active { "auto" } else { "no" });
    }

    fn choisir_sortie(&self, peripherique: &str) {
        self.propriete_texte(c"audio-device", peripherique);
    }

    fn definir_zone(&self, zone: (u32, u32)) {
        self.rendu.modifier(|r| {
            if r.zone != zone {
                r.zone = zone;
                r.forcer = true;
            }
        });
    }

    fn images(&self) -> Arc<RecepteurVideo> {
        self.images.clone()
    }
}

impl Drop for MoteurMpv {
    fn drop(&mut self) {
        self.rendu.modifier(|r| r.arret = true);
        if let Some(fil) = self.fil_rendu.take() {
            let _ = fil.join();
        }
        if let Some(ctx) = self.contexte.take() {
            unsafe {
                ffi::mpv_render_context_set_update_callback(ctx.0, None, ptr::null_mut());
                ffi::mpv_render_context_free(ctx.0);
            }
        }
        self.arret.store(true, Ordering::Release);
        unsafe { ffi::mpv_wakeup(self.mpv.0) };
        if let Some(fil) = self.fil_evenements.take() {
            let _ = fil.join();
        }
        self.images.fermer();
        unsafe { ffi::mpv_terminate_destroy(self.mpv.0) };
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    use crate::lecteur::{EtatLecteur, GenrePiste, Lecteur, Phase};

    use super::*;

    const JETON: &str = "jeton-de-test";
    const DELAI: Duration = Duration::from_secs(60);

    #[test]
    fn options_et_proprietes_de_mpv() {
        let options = options_initiales(&OptionsMpv {
            image: false,
            son: false,
            sortie: Some("alsa/default".into()),
            decodage_materiel: true,
        });
        let options: BTreeMap<_, _> = options.into_iter().collect();
        assert_eq!(options["vo"], "null");
        assert_eq!(options["ao"], "null");
        assert_eq!(options["hwdec"], "auto-copy-safe");
        assert_eq!(options["audio-device"], "alsa/default");
        assert_eq!(options["keep-open"], "yes");
        assert_eq!(options["ytdl"], "no");
        let defaut: BTreeMap<_, _> = options_initiales(&OptionsMpv::default())
            .into_iter()
            .collect();
        assert_eq!(defaut["vo"], "libmpv");
        assert!(!defaut.contains_key("ao") && !defaut.contains_key("audio-device"));
        let e = |n: &str, v: Valeur| evenement_de_propriete(n, v);
        assert_eq!(
            e("pause", Valeur::Drapeau(true)),
            Some(Evenement::EnPause(true))
        );
        assert_eq!(e("time-pos", Valeur::Rien), None);
        assert_eq!(e("duration", Valeur::Rien), Some(Evenement::Duree(None)));
        assert_eq!(
            e("aid", Valeur::Texte("2".into())),
            Some(Evenement::PisteAudio(Some(2)))
        );
        assert_eq!(
            e("sid", Valeur::Texte("no".into())),
            Some(Evenement::SousTitres(None))
        );
        assert_eq!(
            e("dwidth", Valeur::Entier(-4)),
            Some(Evenement::LargeurVideo(0))
        );
        assert_eq!(e("dheight", Valeur::Rien), Some(Evenement::HauteurVideo(0)));
        assert_eq!(e("eof-reached", Valeur::Rien), Some(Evenement::Fin(false)));
        assert_eq!(e("inconnue", Valeur::Drapeau(true)), None);
        assert_eq!(
            message_d_erreur("loading failed", Some("ffmpeg: HTTP error 401")),
            "loading failed (ffmpeg: HTTP error 401)"
        );
        assert_eq!(message_d_erreur("x", Some("")), "x");
    }

    #[derive(Debug, Clone)]
    struct Requete {
        chemin: String,
        autorisee: bool,
    }

    struct ServeurMedias {
        adresse: String,
        requetes: Arc<Mutex<Vec<Requete>>>,
    }

    impl ServeurMedias {
        fn demarrer(dossier: PathBuf) -> Self {
            let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
            let adresse = format!("http://{}", ecoute.local_addr().unwrap());
            let requetes = Arc::new(Mutex::new(Vec::new()));
            let journal = requetes.clone();
            std::thread::spawn(move || {
                for flux in ecoute.incoming().flatten() {
                    let (dossier, journal) = (dossier.clone(), journal.clone());
                    std::thread::spawn(move || servir(flux, &dossier, &journal));
                }
            });
            Self { adresse, requetes }
        }

        fn requetes(&self) -> Vec<Requete> {
            self.requetes.lock().unwrap().clone()
        }
    }

    fn servir(flux: TcpStream, dossier: &Path, journal: &Mutex<Vec<Requete>>) {
        let mut lecture = BufReader::new(flux.try_clone().unwrap());
        let mut ecriture = flux;
        loop {
            let mut ligne = String::new();
            if lecture.read_line(&mut ligne).unwrap_or(0) == 0 {
                return;
            }
            let chemin = ligne.split_whitespace().nth(1).unwrap_or("/").to_string();
            let mut entetes = BTreeMap::new();
            loop {
                let mut l = String::new();
                if lecture.read_line(&mut l).unwrap_or(0) == 0 {
                    return;
                }
                let l = l.trim_end();
                if l.is_empty() {
                    break;
                }
                if let Some((nom, valeur)) = l.split_once(':') {
                    entetes.insert(nom.trim().to_ascii_lowercase(), valeur.trim().to_string());
                }
            }
            let autorisee = entetes.get("authorization").map(String::as_str)
                == Some(&format!("Bearer {JETON}"));
            journal.lock().unwrap().push(Requete {
                chemin: chemin.clone(),
                autorisee,
            });
            let fichier = dossier.join(chemin.trim_start_matches('/').split('?').next().unwrap());
            let reponse = match (autorisee, std::fs::read(&fichier)) {
                (false, _) => (401, "Unauthorized", Vec::new(), None),
                (true, Err(_)) => (404, "Not Found", Vec::new(), None),
                (true, Ok(octets)) => {
                    match entetes.get("range").and_then(|r| plage(r, octets.len())) {
                        Some((debut, fin)) => {
                            let total = octets.len();
                            (
                                206,
                                "Partial Content",
                                octets[debut..=fin].to_vec(),
                                Some((debut, fin, total)),
                            )
                        }
                        None => (200, "OK", octets, None),
                    }
                }
            };
            let (code, texte, corps, partielle) = reponse;
            let mut tete = format!(
                "HTTP/1.1 {code} {texte}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: keep-alive\r\n",
                corps.len()
            );
            if let Some((debut, fin, total)) = partielle {
                tete.push_str(&format!("Content-Range: bytes {debut}-{fin}/{total}\r\n"));
            }
            tete.push_str("\r\n");
            if ecriture.write_all(tete.as_bytes()).is_err() || ecriture.write_all(&corps).is_err() {
                return;
            }
        }
    }

    fn plage(entete: &str, taille: usize) -> Option<(usize, usize)> {
        let (debut, fin) = entete.strip_prefix("bytes=")?.split_once('-')?;
        let debut: usize = debut.parse().ok()?;
        let fin = if fin.is_empty() {
            taille.checked_sub(1)?
        } else {
            fin.parse::<usize>().ok()?.min(taille.checked_sub(1)?)
        };
        (debut <= fin).then_some((debut, fin))
    }

    fn ffmpeg_h264_aac() -> bool {
        let liste = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        if liste.contains("libx264") && liste.contains(" aac ") {
            return true;
        }
        assert!(
            std::env::var_os("ESTOMPE_FFMPEG_EXIGE").is_none(),
            "ffmpeg avec libx264 exigé (ESTOMPE_FFMPEG_EXIGE)"
        );
        println!("ignoré : ffmpeg avec libx264 indisponible");
        false
    }

    fn ffmpeg(arguments: &[&str]) {
        let statut = Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-y"])
            .args(arguments)
            .status()
            .unwrap();
        assert!(statut.success(), "ffmpeg {arguments:?}");
    }

    fn medias_de_test(dossier: &Path, duree: u32, taille: &str) {
        let duree = duree.to_string();
        let srt = dossier.join("st.srt");
        std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:02,000\nBonjour\n").unwrap();
        let video = format!("testsrc=size={taille}:rate=25:duration={duree}");
        let la = format!("sine=frequency=440:duration={duree}");
        let mi = format!("sine=frequency=660:duration={duree}");
        let srt = srt.to_string_lossy().into_owned();
        let mp4 = dossier.join("video.mp4").to_string_lossy().into_owned();
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            &video,
            "-f",
            "lavfi",
            "-i",
            &la,
            "-f",
            "lavfi",
            "-i",
            &mi,
            "-i",
            &srt,
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "25",
            "-c:a",
            "aac",
            "-c:s",
            "mov_text",
            "-metadata:s:a:0",
            "language=fre",
            "-metadata:s:a:1",
            "language=eng",
            "-metadata:s:s:0",
            "language=fre",
            "-movflags",
            "+faststart",
            "-t",
            &duree,
            &mp4,
        ]);
        std::fs::create_dir_all(dossier.join("hls")).unwrap();
        let liste = dossier
            .join("hls/index.m3u8")
            .to_string_lossy()
            .into_owned();
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            &video,
            "-f",
            "lavfi",
            "-i",
            &la,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "25",
            "-c:a",
            "aac",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "-hls_playlist_type",
            "vod",
            "-t",
            &duree,
            &liste,
        ]);
    }

    fn lecteur(image: bool) -> Lecteur {
        Lecteur::mpv(OptionsMpv {
            image,
            son: false,
            ..OptionsMpv::default()
        })
        .unwrap()
    }

    async fn attendre(
        lecteur: &Lecteur,
        quoi: &str,
        condition: impl Fn(&EtatLecteur) -> bool,
    ) -> EtatLecteur {
        let echec = |e: &EtatLecteur| e.phase == Phase::Erreur && quoi != "erreur";
        let mut suivi = lecteur.observer();
        let atteint = tokio::time::timeout(DELAI, suivi.wait_for(|e| condition(e) || echec(e)))
            .await
            .ok()
            .and_then(|r| r.ok().map(|e| e.clone()));
        match atteint {
            Some(e) if condition(&e) => e,
            _ => panic!("« {quoi} » non atteint : {:?}", lecteur.etat()),
        }
    }

    fn segments(dossier: &Path) -> usize {
        std::fs::read_dir(dossier.join("hls"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "ts")
            })
            .count()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn une_video_h264_aac_est_lue_de_bout_en_bout_avec_le_jeton_pistes_et_images() {
        if !ffmpeg_h264_aac() {
            return;
        }
        let dossier = tempfile::tempdir().unwrap();
        medias_de_test(dossier.path(), 3, "320x240");
        let serveur = ServeurMedias::demarrer(dossier.path().to_path_buf());
        let lecteur = lecteur(true);
        lecteur.definir_zone((160, 160));
        lecteur
            .charger(Source::adresse(format!("{}/video.mp4", serveur.adresse)).avec_jeton(JETON));
        let e = attendre(&lecteur, "chargé", |e| {
            e.phase == Phase::Pret && e.duree.is_some() && !e.pistes.is_empty()
        })
        .await;
        assert!((e.duree.unwrap() - 3.0).abs() < 0.2, "{:?}", e.duree);
        assert_eq!(e.pistes_de(GenrePiste::Audio).count(), 2);
        assert_eq!(e.pistes_de(GenrePiste::SousTitres).count(), 1);
        assert_eq!(
            e.pistes_de(GenrePiste::Audio).next().unwrap().libelle(),
            "Français"
        );
        let e = attendre(&lecteur, "taille", |e| e.taille_video().is_some()).await;
        assert_eq!(e.taille_video(), Some((320, 240)));
        assert!(lecteur.images().attendre(0, DELAI).is_some());
        let image = lecteur.images().derniere().unwrap();
        assert_eq!((image.largeur(), image.hauteur()), (160, 120));
        let mut rgba = vec![0; 160 * 120 * 4];
        image.vers_rgba(160, 120, &mut rgba);
        assert!(rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert!(rgba
            .chunks_exact(4)
            .any(|p| p[0] > 100 || p[1] > 100 || p[2] > 100));
        lecteur.choisir_audio(Some(2));
        lecteur.choisir_sous_titres(Some(1));
        attendre(&lecteur, "pistes choisies", |e| {
            e.audio == Some(2) && e.sous_titres == Some(1)
        })
        .await;
        attendre(&lecteur, "lecture", |e| e.position > 0.5 && !e.en_pause).await;
        lecteur.pause();
        attendre(&lecteur, "pause", |e| e.en_pause).await;
        lecteur.aller(2.0);
        attendre(&lecteur, "déplacement", |e| {
            e.position >= 1.9 && !e.recherche
        })
        .await;
        lecteur.definir_vitesse(2.0);
        lecteur.definir_volume(40);
        lecteur.lire();
        attendre(&lecteur, "presque fini", |e| e.position > 2.5).await;
        let e = attendre(&lecteur, "fin", |e| e.fin).await;
        assert_eq!((e.vitesse, e.volume), (2.0, 40));
        assert_eq!(Some(e.position), e.duree);
        let requetes = serveur.requetes();
        assert!(!requetes.is_empty());
        assert!(
            requetes
                .iter()
                .all(|r| r.autorisee && r.chemin == "/video.mp4"),
            "{requetes:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn un_flux_hls_est_lu_de_bout_en_bout_et_chaque_segment_porte_le_jeton() {
        if !ffmpeg_h264_aac() {
            return;
        }
        let dossier = tempfile::tempdir().unwrap();
        medias_de_test(dossier.path(), 4, "320x240");
        let serveur = ServeurMedias::demarrer(dossier.path().to_path_buf());
        let lecteur = lecteur(false);
        let debut = Instant::now();
        lecteur.charger(
            Source::adresse(format!("{}/hls/index.m3u8", serveur.adresse)).avec_jeton(JETON),
        );
        let e = attendre(&lecteur, "chargé", |e| {
            e.phase == Phase::Pret && e.duree.is_some()
        })
        .await;
        assert!((e.duree.unwrap() - 4.0).abs() < 0.3, "{:?}", e.duree);
        assert!(e.a_de_la_video());
        attendre(&lecteur, "dernier segment", |e| e.position > 3.0).await;
        attendre(&lecteur, "fin", |e| e.fin).await;
        assert!(debut.elapsed() >= Duration::from_secs(3));
        let requetes = serveur.requetes();
        assert!(requetes.iter().all(|r| r.autorisee), "{requetes:?}");
        let lus: std::collections::BTreeSet<_> = requetes
            .iter()
            .filter(|r| r.chemin.ends_with(".ts"))
            .map(|r| r.chemin.clone())
            .collect();
        assert_eq!(lus.len(), segments(dossier.path()), "{requetes:?}");
        assert!(requetes.iter().any(|r| r.chemin == "/hls/index.m3u8"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sans_le_bon_jeton_la_lecture_echoue_et_un_fichier_local_se_lit_en_pause() {
        if !ffmpeg_h264_aac() {
            return;
        }
        let dossier = tempfile::tempdir().unwrap();
        medias_de_test(dossier.path(), 2, "160x120");
        let serveur = ServeurMedias::demarrer(dossier.path().to_path_buf());
        let lecteur = lecteur(true);
        lecteur
            .charger(Source::adresse(format!("{}/video.mp4", serveur.adresse)).avec_jeton("faux"));
        let e = attendre(&lecteur, "erreur", |e| e.phase == Phase::Erreur).await;
        println!("erreur : {:?}", e.erreur);
        assert!(serveur.requetes().iter().all(|r| !r.autorisee));
        lecteur.charger(Source::fichier(&dossier.path().join("video.mp4")).en_pause());
        let e = attendre(&lecteur, "chargé en pause", |e| {
            e.phase == Phase::Pret && e.duree.is_some()
        })
        .await;
        assert!(e.en_pause && e.erreur.is_none());
        assert!(lecteur.images().attendre(0, DELAI).is_some());
        lecteur.definir_visible(false);
        attendre(&lecteur, "masqué", |e| !e.visible).await;
        lecteur.lire();
        let e = attendre(&lecteur, "lecture masquée", |e| e.position > 1.0).await;
        assert_eq!(e.taille_video(), None);
        attendre(&lecteur, "fin masquée", |e| e.fin).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn une_video_jointe_sortie_de_l_ecran_est_dechargee_par_mpv() {
        use crate::lecteur::{GenreMedia, LectureUnique, Presence};
        if !ffmpeg_h264_aac() {
            return;
        }
        let dossier = tempfile::tempdir().unwrap();
        medias_de_test(dossier.path(), 3, "160x120");
        let lecture = LectureUnique::nouvelle(std::sync::Arc::new(lecteur(true)));
        lecture.lire_piece(
            "m1",
            "v1",
            GenreMedia::Video,
            Source::fichier(&dossier.path().join("video.mp4")),
        );
        attendre(lecture.lecteur(), "lecture", |e| e.position > 0.5).await;
        assert!(lecture.lecteur().images().attendre(0, DELAI).is_some());
        assert!(!lecture.suivre(|_| Presence::Visible));
        assert!(lecture.suivre(|_| Presence::HorsEcran));
        let e = attendre(lecture.lecteur(), "déchargée", |e| e.phase == Phase::Vide).await;
        assert!(!e.visible && lecture.actif().is_none());
    }

    fn cpu_ms() -> u64 {
        use estompe_plateforme::{MesureProcessus, MesureSysteme};
        MesureSysteme.compteurs().map(|c| c.cpu_ms).unwrap_or(0)
    }

    async fn mesurer(
        chemin: &Path,
        image: bool,
        zone: (u32, u32),
        duree: Duration,
    ) -> (f64, f64, u64) {
        let lecteur = lecteur(image);
        lecteur.definir_zone(zone);
        lecteur.charger(Source::fichier(chemin));
        attendre(&lecteur, "lecture", |e| e.position > 1.0).await;
        let (images, cpu, t) = (lecteur.images().images_recues(), cpu_ms(), Instant::now());
        std::thread::sleep(duree);
        let lecture = (cpu_ms() - cpu) as f64 / t.elapsed().as_millis() as f64 * 100.0;
        let images = lecteur.images().images_recues() - images;
        lecteur.pause();
        attendre(&lecteur, "pause", |e| e.en_pause).await;
        std::thread::sleep(Duration::from_secs(1));
        let (cpu, t) = (cpu_ms(), Instant::now());
        std::thread::sleep(duree);
        let pause = (cpu_ms() - cpu) as f64 / t.elapsed().as_millis() as f64 * 100.0;
        (lecture, pause, images)
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mesure_cpu_en_lecture_1080p_et_en_pause() {
        if !ffmpeg_h264_aac() {
            return;
        }
        let secondes: u64 = std::env::var("ESTOMPE_MESURE_LECTEUR_S")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("1080p.mp4");
        let total = (secondes * 2 + 10).to_string();
        let source = format!("testsrc2=size=1920x1080:rate=30:duration={total}");
        let son = format!("sine=frequency=440:duration={total}");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            &source,
            "-f",
            "lavfi",
            "-i",
            &son,
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-b:v",
            "6M",
            "-c:a",
            "aac",
            "-t",
            &total,
            chemin.to_str().unwrap(),
        ]);
        let duree = Duration::from_secs(secondes);
        for (nom, image, zone) in [
            ("décodage seul (vo=null)", false, (0, 0)),
            ("rendu logiciel 1280x720", true, (1280, 720)),
            ("rendu logiciel 640x360", true, (640, 360)),
        ] {
            let (lecture, pause, images) = mesurer(&chemin, image, zone, duree).await;
            println!(
                "{nom} : lecture {lecture:.1} % d'un cœur ({images} images rendues en {secondes} s), pause {pause:.1} %"
            );
        }
    }
}
