use std::cell::Cell;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::IDXGIAdapter1;
use windows::Win32::Media::MediaFoundation::{IMFDXGIBuffer, IMFSample};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use super::encodeur::{Depot, Encodeur, Envoi, Parametres};
use super::gpu::{texture_nv12, Appareil};
use super::materiel::{adaptateur_de, initialiser, texte};
use super::SOURCE;
use crate::debuglog;
use crate::replay::garde::{ligne_essai, lire_essai, Essai, FIN_ESSAI};
use crate::replay::mp4::en_longueurs;
use crate::replay::reglages::GOP_S;
use crate::replay::tampon::Tampon;
use crate::replay::types::Codec;

const AMF_OK: i32 = 0;
const AMF_REPEAT: i32 = 24;
const AMF_INPUT_FULL: i32 = 25;
const SURFACE_NV12: i32 = 1;
const DX11_0: i32 = 110;

const RELEASE: usize = 1;
const QUERY_INTERFACE: usize = 2;
const SET_PROPERTY: usize = 3;
const GET_PROPERTY: usize = 4;
const CTX_TERMINATE: usize = 13;
const CTX_INIT_DX11: usize = 18;
const CTX_SURFACE_FROM_DX11: usize = 49;
const COMP_INIT: usize = 17;
const COMP_TERMINATE: usize = 19;
const COMP_SUBMIT_INPUT: usize = 22;
const COMP_QUERY_OUTPUT: usize = 23;
const DATA_SET_PTS: usize = 19;
const DATA_GET_PTS: usize = 20;
const BUFFER_GET_SIZE: usize = 24;
const BUFFER_GET_NATIVE: usize = 25;

const IID_BUFFER: GUID = GUID::from_values(0xb04b7248, 0xb6f0, 0x4321, [0xb6, 0x91, 0xba, 0xa4, 0x74, 0x0f, 0x9f, 0xcb]);
const ESSAIS_ENTREE_PLEINE: u32 = 20;
const ATTENTE: Duration = Duration::from_millis(4);

type Objet = *mut c_void;
type Recevoir<'a> = &'a mut dyn FnMut(&[u8], i64, bool) -> bool;

#[repr(C)]
#[derive(Clone, Copy)]
struct Variante {
    genre: i32,
    donnees: [u64; 2],
}

impl Variante {
    fn entier(v: i64) -> Self {
        Self { genre: 2, donnees: [v as u64, 0] }
    }

    fn booleen(v: bool) -> Self {
        Self { genre: 1, donnees: [v as u64, 0] }
    }

    fn taille(l: u32, h: u32) -> Self {
        Self { genre: 5, donnees: [(h as u64) << 32 | l as u64, 0] }
    }

    fn cadence(numerateur: u32, denominateur: u32) -> Self {
        Self { genre: 7, donnees: [(denominateur as u64) << 32 | numerateur as u64, 0] }
    }
}

fn large(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn methode<F: Copy>(objet: Objet, rang: usize) -> F {
    if objet.is_null() {
        panic!("AMF : objet nul");
    }
    let table = *(objet as *const *const usize);
    if table.is_null() {
        panic!("AMF : table nulle");
    }
    std::mem::transmute_copy(&*table.add(rang))
}

unsafe fn liberer(objet: Objet) {
    if !objet.is_null() {
        methode::<unsafe extern "system" fn(Objet) -> i32>(objet, RELEASE)(objet);
    }
}

fn verifier(code: i32, quoi: &str) -> Result<(), String> {
    if code == AMF_OK {
        Ok(())
    } else {
        Err(format!("AMF {quoi} : code {code}"))
    }
}

unsafe fn regler(objet: Objet, nom: &str, v: Variante) -> i32 {
    methode::<unsafe extern "system" fn(Objet, *const u16, Variante) -> i32>(objet, SET_PROPERTY)(objet, large(nom).as_ptr(), v)
}

struct Amf {
    codec: Codec,
    contexte: Objet,
    composant: Objet,
    periode_cle: u64,
    soumises: u64,
    refusees: u64,
    duree: i64,
}

impl Amf {
    unsafe fn ouvrir(appareil: &ID3D11Device, p: &Parametres) -> Result<Self, String> {
        let bibliotheque = LoadLibraryW(large("amfrt64.dll").as_ptr());
        if bibliotheque.is_null() {
            return Err("amfrt64.dll absente (pilote AMD requis)".into());
        }
        let initialiser = GetProcAddress(bibliotheque, c"AMFInit".as_ptr().cast()).ok_or("AMFInit introuvable")?;
        let version = GetProcAddress(bibliotheque, c"AMFQueryVersion".as_ptr().cast()).ok_or("AMFQueryVersion introuvable")?;
        let mut numero = 0u64;
        verifier(std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "system" fn(*mut u64) -> i32>(version)(&mut numero), "AMFQueryVersion")?;
        let mut fabrique: Objet = std::ptr::null_mut();
        verifier(std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "system" fn(u64, *mut Objet) -> i32>(initialiser)(numero, &mut fabrique), "AMFInit")?;
        if fabrique.is_null() {
            return Err("AMF : fabrique nulle".into());
        }
        let mut contexte: Objet = std::ptr::null_mut();
        verifier(methode::<unsafe extern "system" fn(Objet, *mut Objet) -> i32>(fabrique, 0)(fabrique, &mut contexte), "CreateContext")?;
        if contexte.is_null() {
            return Err("AMF : contexte nul".into());
        }
        let periode_cle = (p.ips * GOP_S).max(1) as u64;
        match Self::composant(fabrique, contexte, appareil, p, periode_cle as i64) {
            Ok(composant) => Ok(Self { codec: p.codec, contexte, composant, periode_cle, soumises: 0, refusees: 0, duree: 10_000_000 / p.ips.max(1) as i64 }),
            Err(e) => {
                methode::<unsafe extern "system" fn(Objet) -> i32>(contexte, CTX_TERMINATE)(contexte);
                liberer(contexte);
                Err(e)
            }
        }
    }

    unsafe fn composant(fabrique: Objet, contexte: Objet, appareil: &ID3D11Device, p: &Parametres, periode_cle: i64) -> Result<Objet, String> {
        verifier(methode::<unsafe extern "system" fn(Objet, Objet, i32) -> i32>(contexte, CTX_INIT_DX11)(contexte, appareil.as_raw(), DX11_0), "InitDX11")?;
        let identifiant = large(match p.codec {
            Codec::H264 => "AMFVideoEncoderVCE_AVC",
            Codec::Hevc => "AMFVideoEncoderHW_HEVC",
        });
        let mut composant: Objet = std::ptr::null_mut();
        let creation = methode::<unsafe extern "system" fn(Objet, Objet, *const u16, *mut Objet) -> i32>(fabrique, 1)(fabrique, contexte, identifiant.as_ptr(), &mut composant);
        verifier(creation, "CreateComponent")?;
        if composant.is_null() {
            return Err("AMF : composant nul".into());
        }
        let bits = p.debit_kbps as i64 * 1000;
        let reglages: Vec<(&str, Variante)> = match p.codec {
            Codec::H264 => vec![
                ("Usage", Variante::entier(2)),
                ("Profile", Variante::entier(100)),
                ("QualityPreset", Variante::entier(0)),
                ("FrameSize", Variante::taille(p.largeur, p.hauteur)),
                ("FrameRate", Variante::cadence(p.ips, 1)),
                ("RateControlMethod", Variante::entier(1)),
                ("TargetBitrate", Variante::entier(bits)),
                ("PeakBitrate", Variante::entier(bits)),
                ("BPicturesPattern", Variante::entier(0)),
                ("IDRPeriod", Variante::entier(periode_cle)),
                ("HeaderInsertionSpacing", Variante::entier(periode_cle)),
                ("InsertSPS", Variante::booleen(true)),
            ],
            Codec::Hevc => vec![
                ("HevcUsage", Variante::entier(2)),
                ("HevcProfile", Variante::entier(1)),
                ("HevcQualityPreset", Variante::entier(5)),
                ("HevcFrameSize", Variante::taille(p.largeur, p.hauteur)),
                ("HevcFrameRate", Variante::cadence(p.ips, 1)),
                ("HevcRateControlMethod", Variante::entier(3)),
                ("HevcTargetBitrate", Variante::entier(bits)),
                ("HevcPeakBitrate", Variante::entier(bits)),
                ("HevcGOPSize", Variante::entier(periode_cle)),
                ("HevcHeaderInsertionMode", Variante::entier(2)),
            ],
        };
        let pret = reglages.into_iter().try_for_each(|(nom, v)| verifier(regler(composant, nom, v), nom)).and_then(|_| {
            verifier(methode::<unsafe extern "system" fn(Objet, i32, i32, i32) -> i32>(composant, COMP_INIT)(composant, SURFACE_NV12, p.largeur as i32, p.hauteur as i32), "Init")
        });
        if let Err(e) = pret {
            liberer(composant);
            return Err(e);
        }
        Ok(composant)
    }

    unsafe fn vider(&mut self, recevoir: Recevoir) -> Result<(), String> {
        loop {
            let mut donnee: Objet = std::ptr::null_mut();
            let code = methode::<unsafe extern "system" fn(Objet, *mut Objet) -> i32>(self.composant, COMP_QUERY_OUTPUT)(self.composant, &mut donnee);
            if code == AMF_REPEAT || donnee.is_null() {
                return Ok(());
            }
            let lu = verifier(code, "QueryOutput").and_then(|_| self.deposer(donnee, recevoir));
            liberer(donnee);
            lu?;
        }
    }

    unsafe fn deposer(&self, donnee: Objet, recevoir: Recevoir) -> Result<(), String> {
        let mut tampon: Objet = std::ptr::null_mut();
        verifier(methode::<unsafe extern "system" fn(Objet, *const GUID, *mut Objet) -> i32>(donnee, QUERY_INTERFACE)(donnee, &IID_BUFFER, &mut tampon), "QueryInterface(AMFBuffer)")?;
        if tampon.is_null() {
            return Err("AMF : tampon nul".into());
        }
        let taille = methode::<unsafe extern "system" fn(Objet) -> usize>(tampon, BUFFER_GET_SIZE)(tampon);
        let natif = methode::<unsafe extern "system" fn(Objet) -> *const u8>(tampon, BUFFER_GET_NATIVE)(tampon);
        let octets = if natif.is_null() { Vec::new() } else { std::slice::from_raw_parts(natif, taille).to_vec() };
        liberer(tampon);
        let pts = methode::<unsafe extern "system" fn(Objet) -> i64>(donnee, DATA_GET_PTS)(donnee);
        let mut genre = Variante::entier(-1);
        let nom = large(match self.codec {
            Codec::H264 => "OutputDataType",
            Codec::Hevc => "HevcOutputDataType",
        });
        methode::<unsafe extern "system" fn(Objet, *const u16, *mut Variante) -> i32>(donnee, GET_PROPERTY)(donnee, nom.as_ptr(), &mut genre);
        if recevoir(&octets, pts, genre.donnees[0] as i64 == 0) {
            Ok(())
        } else {
            Err("flux AMF sans codes de début".into())
        }
    }

    unsafe fn soumettre(&mut self, echantillon: &IMFSample, recevoir: Recevoir) -> Result<(), String> {
        let pts = echantillon.GetSampleTime().map_err(texte)?;
        let tampon = echantillon.GetBufferByIndex(0).and_then(|t| t.cast::<IMFDXGIBuffer>()).map_err(texte)?;
        let mut brute = std::ptr::null_mut();
        tampon.GetResource(&ID3D11Texture2D::IID, &mut brute).map_err(texte)?;
        self.soumettre_texture(&ID3D11Texture2D::from_raw(brute), pts, recevoir)
    }

    unsafe fn soumettre_texture(&mut self, texture: &ID3D11Texture2D, pts: i64, recevoir: Recevoir) -> Result<(), String> {
        let mut surface: Objet = std::ptr::null_mut();
        let creation = methode::<unsafe extern "system" fn(Objet, Objet, *mut Objet, Objet) -> i32>(self.contexte, CTX_SURFACE_FROM_DX11)(self.contexte, texture.as_raw(), &mut surface, std::ptr::null_mut());
        verifier(creation, "CreateSurfaceFromDX11Native")?;
        if surface.is_null() {
            return Err("AMF : surface nulle".into());
        }
        methode::<unsafe extern "system" fn(Objet, i64)>(surface, DATA_SET_PTS)(surface, pts);
        if self.soumises % self.periode_cle == 0 {
            match self.codec {
                Codec::H264 => {
                    regler(surface, "ForcePictureType", Variante::entier(2));
                    regler(surface, "InsertSPS", Variante::booleen(true));
                    regler(surface, "InsertPPS", Variante::booleen(true));
                }
                Codec::Hevc => {
                    regler(surface, "HevcForcePictureType", Variante::entier(2));
                    regler(surface, "HevcInsertHeader", Variante::booleen(true));
                }
            }
        }
        let mut code = AMF_INPUT_FULL;
        for _ in 0..ESSAIS_ENTREE_PLEINE {
            code = methode::<unsafe extern "system" fn(Objet, Objet) -> i32>(self.composant, COMP_SUBMIT_INPUT)(self.composant, surface);
            if code != AMF_INPUT_FULL {
                break;
            }
            if let Err(e) = self.vider(recevoir) {
                liberer(surface);
                return Err(e);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        liberer(surface);
        if code == AMF_INPUT_FULL {
            self.refusees += 1;
            return Ok(());
        }
        verifier(code, "SubmitInput")?;
        self.soumises += 1;
        Ok(())
    }

    unsafe fn tourner(&mut self, rx: &Receiver<Envoi<IMFSample>>, depot: &mut Depot) -> Result<(), String> {
        let duree = self.duree;
        let mut recevoir = |octets: &[u8], pts: i64, cle: bool| depot.deposer(octets, pts, duree, cle);
        loop {
            self.vider(&mut recevoir)?;
            match rx.recv_timeout(ATTENTE) {
                Ok(echantillon) => self.soumettre(&echantillon.prendre(), &mut recevoir)?,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }
}

impl Drop for Amf {
    fn drop(&mut self) {
        unsafe {
            methode::<unsafe extern "system" fn(Objet) -> i32>(self.composant, COMP_TERMINATE)(self.composant);
            liberer(self.composant);
            methode::<unsafe extern "system" fn(Objet) -> i32>(self.contexte, CTX_TERMINATE)(self.contexte);
            liberer(self.contexte);
        }
    }
}

const TAILLES_SONDEES: [(u32, u32); 6] = [(7680, 4320), (4096, 2160), (3840, 2160), (2560, 1440), (1920, 1080), (1280, 720)];
const TAILLE_ESSAI: (u32, u32) = (1280, 720);
const IMAGES_ESSAI: i64 = 30;
const DELAI_IMAGE: Duration = Duration::from_secs(3);
const DELAI_ESSAI: Duration = Duration::from_secs(20);
const SANS_FENETRE: u32 = 0x0800_0000;
pub const ARGUMENT_ESSAI: &str = "--essai-amf";

fn sonder(appareil: &ID3D11Device, codec: Codec) -> Option<(u32, u32)> {
    TAILLES_SONDEES.into_iter().find(|&(largeur, hauteur)| {
        let essai = Parametres { codec, largeur, hauteur, ips: 60, debit_kbps: 10_000, base: 0 };
        unsafe { Amf::ouvrir(appareil, &essai) }.is_ok()
    })
}

unsafe fn encoder_une_image(appareil: &Appareil, codec: Codec) -> Result<(), String> {
    let p = Parametres { codec, largeur: TAILLE_ESSAI.0, hauteur: TAILLE_ESSAI.1, ips: 60, debit_kbps: 8000, base: 0 };
    let mut amf = Amf::ouvrir(&appareil.d3d, &p)?;
    let texture = texture_nv12(appareil, TAILLE_ESSAI, false).map_err(texte)?;
    let (images, correcte) = (Cell::new(0u32), Cell::new(true));
    let mut recevoir = |octets: &[u8], _: i64, cle: bool| {
        if images.get() == 0 && (!cle || en_longueurs(octets).is_empty()) {
            correcte.set(false);
        }
        images.set(images.get() + 1);
        true
    };
    let limite = Instant::now() + DELAI_IMAGE;
    let mut soumises = 0i64;
    while images.get() == 0 && Instant::now() < limite {
        if soumises < IMAGES_ESSAI {
            amf.soumettre_texture(&texture, soumises * amf.duree, &mut recevoir)?;
            soumises += 1;
        }
        amf.vider(&mut recevoir)?;
        std::thread::sleep(Duration::from_millis(5));
    }
    match (images.get(), correcte.get()) {
        (0, _) => Err("aucune image encodée en 3 s".into()),
        (_, false) => Err("première image sans image clé ou sans codes de début".into()),
        _ => Ok(()),
    }
}

pub fn essai_encodeur(mut arguments: impl Iterator<Item = String>) -> Option<i32> {
    if arguments.next().as_deref() != Some(ARGUMENT_ESSAI) {
        return None;
    }
    let (Some(haut), Some(bas)) = (arguments.next().and_then(|a| a.parse::<i32>().ok()), arguments.next().and_then(|a| a.parse::<u32>().ok())) else { return Some(2) };
    let mut sortie = std::io::stdout();
    let _ = initialiser();
    let Some(adaptateur) = adaptateur_de(haut, bas) else { return Some(3) };
    let Ok(appareil) = Appareil::creer(&adaptateur) else { return Some(4) };
    for (codec, nom) in [(Codec::H264, "h264"), (Codec::Hevc, "hevc")] {
        let Some(taille) = sonder(&appareil.d3d, codec) else { continue };
        let ligne = match unsafe { encoder_une_image(&appareil, codec) } {
            Ok(()) => ligne_essai(nom, taille),
            Err(e) => format!("amf refus {nom} {e}"),
        };
        let _ = writeln!(sortie, "{ligne}");
        let _ = sortie.flush();
    }
    let _ = writeln!(sortie, "{FIN_ESSAI}");
    let _ = sortie.flush();
    Some(0)
}

pub fn essai_hors_processus(adaptateur: &IDXGIAdapter1) -> Result<Essai, String> {
    let luid = unsafe { adaptateur.GetDesc1() }.map_err(texte)?.AdapterLuid;
    let programme = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut commande = Command::new(programme);
    commande.args([ARGUMENT_ESSAI.to_string(), luid.HighPart.to_string(), luid.LowPart.to_string()]);
    commande.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).creation_flags(SANS_FENETRE);
    let mut enfant = commande.spawn().map_err(|e| e.to_string())?;
    let mut tuyau = enfant.stdout.take().ok_or("sortie de l'essai illisible")?;
    let lecture = std::thread::spawn(move || {
        let mut texte = String::new();
        let _ = tuyau.read_to_string(&mut texte);
        texte
    });
    let limite = Instant::now() + DELAI_ESSAI;
    let code = loop {
        match enfant.try_wait() {
            Ok(Some(statut)) => break statut.code(),
            Ok(None) if Instant::now() < limite => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = enfant.kill();
                let _ = enfant.wait();
                break None;
            }
        }
    };
    let texte = lecture.join().unwrap_or_default();
    for ligne in texte.lines().filter(|l| l.starts_with("amf refus")) {
        debuglog::log("warn", SOURCE, &format!("essai d'AMF : {ligne}"));
    }
    lire_essai(&texte).ok_or_else(|| match code {
        Some(c) => format!("le programme d'essai s'est arrêté avec le code {c:#x}"),
        None => "le programme d'essai n'a pas répondu en 20 s".to_string(),
    })
}

pub fn demarrer(
    a: &Appareil,
    parametres: Parametres,
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    echec: impl FnOnce(String) + Send + 'static,
) -> Result<(Encodeur, Receiver<()>), String> {
    let (tx, rx) = sync_channel::<Envoi<IMFSample>>(2);
    let (fini, termine) = sync_channel::<()>(0);
    let (pret, attente) = channel::<Result<(), String>>();
    let encodees = Arc::new(AtomicU64::new(0));
    let mut depot = Depot { parametres, entete: Vec::new(), tampon, arret, encodees: encodees.clone() };
    let appareil = Envoi(a.d3d.clone());
    std::thread::spawn(move || {
        let _fini = fini;
        let _ = initialiser();
        let d3d = appareil.prendre();
        let mut amf = match unsafe { Amf::ouvrir(&d3d, &parametres) } {
            Ok(amf) => {
                let _ = pret.send(Ok(()));
                amf
            }
            Err(e) => {
                let _ = pret.send(Err(e));
                return;
            }
        };
        let resultat = unsafe { amf.tourner(&rx, &mut depot) };
        debuglog::log("info", SOURCE, &format!("encodeur AMF : {} images soumises, {} refusées (entrée pleine)", amf.soumises, amf.refusees));
        drop(amf);
        if let Err(e) = resultat {
            if !depot.arret.load(Ordering::Relaxed) {
                echec(format!("l'encodeur AMF s'est arrêté : {e}"));
            }
        }
    });
    attente.recv().map_err(|e| e.to_string())??;
    let nom = match parametres.codec {
        Codec::H264 => "AMD AMF H.264",
        Codec::Hevc => "AMD AMF HEVC",
    };
    Ok((Encodeur::nouveau(tx, nom.into(), encodees), termine))
}
