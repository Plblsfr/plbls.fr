use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use windows::core::{Interface, GUID};
use windows::Win32::Foundation::{E_UNEXPECTED, VARIANT_BOOL};
use windows::Win32::Graphics::Dxgi::IDXGIAdapter1;
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_UI4};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, eAVEncH264VProfile_High, eAVEncH265VProfile_Main_420_8, ICodecAPI, IMFActivate, IMFMediaEventGenerator, IMFSample, IMFShutdown, IMFTransform,
    METransformHaveOutput, METransformNeedInput, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFMediaType_Video, MFSampleExtension_CleanPoint, MFVideoFormat_H264, MFVideoFormat_HEVC, CODECAPI_AVEncCommonMeanBitRate,
    CODECAPI_AVEncCommonRateControlMode, CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize, CODECAPI_AVLowLatencyMode, MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_MESSAGE_SET_D3D_MANAGER, MFT_OUTPUT_DATA_BUFFER, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_E_TRANSFORM_STREAM_CHANGE,
    MF_MT_AVG_BITRATE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_LEVEL, MF_MT_MPEG2_PROFILE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC_UNLOCK,
};

use super::gpu::{couleurs, type_nv12, Appareil};
use super::materiel::{activations, initialiser, nom_activation, texte};
use super::SOURCE;
use crate::debuglog;
use crate::replay::cadence::Entree;
use crate::replay::mp4::{avec_parametres, en_longueurs};
use crate::replay::reglages::{niveau_h264, GOP_S};
use crate::replay::tampon::{Piste, Tampon, Unite};
use crate::replay::types::Codec;

pub struct Envoi<T>(pub T);

unsafe impl<T> Send for Envoi<T> {}

impl<T> Envoi<T> {
    pub fn prendre(self) -> T {
        self.0
    }
}

#[derive(Clone, Copy)]
pub struct Parametres {
    pub codec: Codec,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub debit_kbps: u32,
    pub base: i64,
}

pub struct Encodeur {
    tx: SyncSender<Envoi<IMFSample>>,
    pub nom: String,
    pub encodees: Arc<AtomicU64>,
}

impl Encodeur {
    pub fn envoyer(&self, echantillon: IMFSample) -> Result<bool, ()> {
        match self.tx.try_send(Envoi(echantillon)) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => Err(()),
        }
    }
}

unsafe fn regler(api: &ICodecAPI, cle: &GUID, valeur: Option<u32>) {
    let mut variante = VARIANT::default();
    let contenu = &mut *variante.Anonymous.Anonymous;
    match valeur {
        Some(v) => {
            contenu.vt = VT_UI4;
            contenu.Anonymous.ulVal = v;
        }
        None => {
            contenu.vt = VT_BOOL;
            contenu.Anonymous.boolVal = VARIANT_BOOL(-1);
        }
    }
    let _ = api.SetValue(cle, &variante);
}

unsafe fn configurer(activation: &IMFActivate, a: &Appareil, p: &Parametres, entree: Entree) -> windows::core::Result<(IMFTransform, IMFMediaEventGenerator)> {
    let transformation: IMFTransform = activation.ActivateObject()?;
    transformation.GetAttributes()?.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
    if entree != Entree::Memoire {
        transformation.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, a.gestionnaire.as_raw() as usize)?;
    }
    let sortie = MFCreateMediaType()?;
    sortie.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    let (format, profil) = match p.codec {
        Codec::H264 => (MFVideoFormat_H264, eAVEncH264VProfile_High.0),
        Codec::Hevc => (MFVideoFormat_HEVC, eAVEncH265VProfile_Main_420_8.0),
    };
    sortie.SetGUID(&MF_MT_SUBTYPE, &format)?;
    sortie.SetUINT32(&MF_MT_AVG_BITRATE, p.debit_kbps * 1000)?;
    sortie.SetUINT32(&MF_MT_MPEG2_PROFILE, profil as u32)?;
    couleurs(&sortie, p.largeur, p.hauteur, p.ips)?;
    let niveau = (p.codec == Codec::H264).then(|| niveau_h264((p.largeur, p.hauteur), p.ips));
    if let Some(niveau) = niveau {
        sortie.SetUINT32(&MF_MT_MPEG2_LEVEL, niveau)?;
    }
    match (transformation.SetOutputType(0, &sortie, 0), niveau) {
        (Ok(()), Some(niveau)) => debuglog::log("info", SOURCE, &format!("encodeur : niveau H.264 {}.{} demandé", niveau / 10, niveau % 10)),
        (Err(e), Some(niveau)) => {
            debuglog::log("warn", SOURCE, &format!("encodeur : niveau H.264 {}.{} refusé ({}), niveau laissé à l'encodeur", niveau / 10, niveau % 10, texte(e)));
            sortie.DeleteItem(&MF_MT_MPEG2_LEVEL)?;
            transformation.SetOutputType(0, &sortie, 0)?;
        }
        (resultat, None) => resultat?,
    }
    transformation.SetInputType(0, &type_nv12(p.largeur, p.hauteur, p.ips)?, 0)?;
    if let Ok(api) = transformation.cast::<ICodecAPI>() {
        regler(&api, &CODECAPI_AVEncCommonRateControlMode, Some(eAVEncCommonRateControlMode_CBR.0 as u32));
        regler(&api, &CODECAPI_AVEncCommonMeanBitRate, Some(p.debit_kbps * 1000));
        regler(&api, &CODECAPI_AVEncMPVGOPSize, Some(p.ips * GOP_S));
        regler(&api, &CODECAPI_AVEncMPVDefaultBPictureCount, Some(0));
        regler(&api, &CODECAPI_AVLowLatencyMode, None);
    }
    let evenements = transformation.cast::<IMFMediaEventGenerator>()?;
    transformation.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
    transformation.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
    Ok((transformation, evenements))
}

unsafe fn entete(transformation: &IMFTransform) -> Vec<u8> {
    let Ok(sortie) = transformation.GetOutputCurrentType(0) else { return Vec::new() };
    let taille = sortie.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).unwrap_or(0) as usize;
    let mut blob = vec![0u8; taille];
    if taille == 0 || sortie.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None).is_err() {
        return Vec::new();
    }
    en_longueurs(&blob)
}

pub unsafe fn octets(echantillon: &IMFSample) -> windows::core::Result<Vec<u8>> {
    let tampon = echantillon.ConvertToContiguousBuffer()?;
    let (mut pointeur, mut longueur) = (std::ptr::null_mut(), 0u32);
    tampon.Lock(&mut pointeur, None, Some(&mut longueur))?;
    let copie = std::slice::from_raw_parts(pointeur, longueur as usize).to_vec();
    tampon.Unlock()?;
    Ok(copie)
}

struct Sortie {
    transformation: IMFTransform,
    parametres: Parametres,
    entete: Vec<u8>,
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    encodees: Arc<AtomicU64>,
}

impl Sortie {
    unsafe fn lire(&mut self) -> windows::core::Result<()> {
        let mut change = false;
        let echantillon = loop {
            let mut sorties = [MFT_OUTPUT_DATA_BUFFER::default()];
            let flux = self.transformation.GetOutputStreamInfo(0)?;
            if flux.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32 == 0 {
                let place = flux.cbSize.max(self.parametres.largeur * self.parametres.hauteur * 2);
                let fourni = MFCreateSample()?;
                fourni.AddBuffer(&MFCreateMemoryBuffer(place)?)?;
                sorties[0].pSample = ManuallyDrop::new(Some(fourni));
            }
            let mut statut = 0u32;
            let resultat = self.transformation.ProcessOutput(0, &mut sorties, &mut statut);
            let [sortie] = sorties;
            let echantillon = ManuallyDrop::into_inner(sortie.pSample);
            drop(ManuallyDrop::into_inner(sortie.pEvents));
            match resultat {
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE && !change => {
                    let disponible = self.transformation.GetOutputAvailableType(0, 0)?;
                    self.transformation.SetOutputType(0, &disponible, 0)?;
                    self.entete = entete(&self.transformation);
                    change = true;
                    debuglog::log("info", SOURCE, "encodeur : format de sortie changé, sortie relue");
                }
                Err(e) if change && (e.code() == E_UNEXPECTED || e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT) => {
                    debuglog::log("info", SOURCE, &format!("encodeur : aucune sortie après le changement de format ({})", texte(e)));
                    return Ok(());
                }
                autre => {
                    autre?;
                    break echantillon;
                }
            }
        };
        let Some(echantillon) = echantillon else {
            debuglog::log("warn", SOURCE, "encodeur : sortie annoncée sans échantillon");
            return Ok(());
        };
        let cle = echantillon.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
        let (pts, duree) = (echantillon.GetSampleTime()?, echantillon.GetSampleDuration().unwrap_or(0));
        let brut = octets(&echantillon)?;
        let mut donnees = en_longueurs(&brut);
        if donnees.is_empty() && !brut.is_empty() {
            return Err(windows::core::Error::new(E_UNEXPECTED, "flux de l'encodeur sans codes de début"));
        }
        if cle {
            donnees = avec_parametres(self.parametres.codec, donnees, &mut self.entete);
        }
        let duree = if duree > 0 { duree } else { 10_000_000 / self.parametres.ips as i64 };
        if self.encodees.fetch_add(1, Ordering::Relaxed) == 0 {
            debuglog::log("info", SOURCE, &format!("encodeur : première image encodée ({} octets, image clé : {cle})", donnees.len()));
        }
        if !self.arret.load(Ordering::Relaxed) {
            if let Ok(mut t) = self.tampon.lock() {
                t.pousser(Piste::Video, Unite { pts: self.parametres.base + pts, duree, cle, donnees: donnees.into() });
            }
        }
        Ok(())
    }
}

unsafe fn boucle(evenements: &IMFMediaEventGenerator, sortie: &mut Sortie, rx: &Receiver<Envoi<IMFSample>>) -> windows::core::Result<()> {
    let (mut entrees, mut sorties, mut autres) = (0u64, 0u64, 0u64);
    loop {
        let evenement = evenements.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))?;
        evenement.GetStatus()?.ok()?;
        let genre = evenement.GetType()?;
        if genre == METransformNeedInput.0 as u32 {
            let Ok(echantillon) = rx.recv().map(Envoi::prendre) else { return Ok(()) };
            sortie.transformation.ProcessInput(0, &echantillon, 0)?;
            entrees += 1;
        } else if genre == METransformHaveOutput.0 as u32 {
            sortie.lire()?;
            sorties += 1;
        } else {
            autres += 1;
        }
        if entrees + sorties + autres <= 6 {
            debuglog::log("info", SOURCE, &format!("encodeur : évènement {genre} traité ({entrees} entrée(s), {sorties} sortie(s), {autres} autre(s), {} image(s) encodée(s))", sortie.encodees.load(Ordering::Relaxed)));
        }
    }
}

pub fn demarrer(
    a: &Appareil,
    adaptateur: &IDXGIAdapter1,
    parametres: Parametres,
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    entree: Entree,
    echec: impl FnOnce(String) + Send + 'static,
) -> Result<(Encodeur, Receiver<()>), String> {
    let mut derniere = "aucun encodeur matériel pour ce format".to_string();
    for activation in activations(parametres.codec, adaptateur) {
        match unsafe { configurer(&activation, a, &parametres, entree) } {
            Ok((transformation, evenements)) => {
                let nom = nom_activation(&activation);
                let (tx, rx) = sync_channel::<Envoi<IMFSample>>(2);
                let (fini, termine) = sync_channel::<()>(0);
                let encodees = Arc::new(AtomicU64::new(0));
                let compteur = encodees.clone();
                let fil = Envoi((transformation, evenements, activation));
                std::thread::spawn(move || {
                    let (transformation, evenements, activation) = fil.prendre();
                    let _fini = fini;
                    let _ = initialiser();
                    let mut sortie = Sortie { entete: unsafe { entete(&transformation) }, transformation, parametres, tampon, arret, encodees: compteur };
                    let resultat = unsafe { boucle(&evenements, &mut sortie, &rx) };
                    unsafe {
                        let _ = sortie.transformation.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
                        if let Ok(fermeture) = sortie.transformation.cast::<IMFShutdown>() {
                            let _ = fermeture.Shutdown();
                        }
                        let _ = activation.ShutdownObject();
                    }
                    if let Err(e) = resultat {
                        if !sortie.arret.load(Ordering::Relaxed) {
                            echec(format!("l'encodeur matériel s'est arrêté : {}", texte(e)));
                        }
                    }
                });
                return Ok((Encodeur { tx, nom, encodees }, termine));
            }
            Err(e) => {
                derniere = format!("{} : {}", nom_activation(&activation), texte(e));
                unsafe {
                    let _ = activation.ShutdownObject();
                }
            }
        }
    }
    Err(format!("session d'encodage matériel refusée ({derniere})"))
}
