use std::mem::ManuallyDrop;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use wasapi::{AudioClient, DeviceEnumerator, Direction};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFTransform, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Audio, MFTEnumEx, MFT_CATEGORY_AUDIO_ENCODER,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_OUTPUT_DATA_BUFFER, MFT_REGISTER_TYPE_INFO, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
};
use windows::Win32::System::Com::CoTaskMemFree;

use super::encodeur::octets;
use super::materiel::{initialiser, texte};
use crate::replay::cadence::Alignement;
use crate::replay::micro::{self, Traitement, FREQUENCE};
use crate::replay::tampon::{Piste, Tampon, Unite, UNITE_TEMPS};
use crate::son_wasapi::{capturer, flottants, pcm16};

const BLOC: usize = 1024;

struct Aac {
    transformation: IMFTransform,
    canaux: usize,
    attente: Vec<i16>,
    entres: u64,
    sortis: u64,
    taille_sortie: u32,
}

fn temps(echantillons: u64) -> i64 {
    (echantillons as i128 * UNITE_TEMPS as i128 / FREQUENCE as i128) as i64
}

impl Aac {
    fn creer(canaux: usize, kbps: u32) -> Result<Self, String> {
        initialiser()?;
        unsafe { Self::creer_brut(canaux, kbps) }.map_err(|e| format!("encodeur AAC indisponible : {}", texte(e)))
    }

    unsafe fn creer_brut(canaux: usize, kbps: u32) -> windows::core::Result<Self> {
        let sortie_voulue = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Audio, guidSubtype: MFAudioFormat_AAC };
        let (mut liste, mut nombre) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0u32);
        MFTEnumEx(MFT_CATEGORY_AUDIO_ENCODER, MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER, None, Some(&sortie_voulue), &mut liste, &mut nombre)?;
        let trouves: Vec<IMFActivate> = if liste.is_null() { Vec::new() } else { (0..nombre as usize).filter_map(|i| std::ptr::read(liste.add(i))).collect() };
        CoTaskMemFree(Some(liste as *const _));
        let transformation: IMFTransform = trouves.first().ok_or_else(windows::core::Error::empty)?.ActivateObject()?;
        let entree = MFCreateMediaType()?;
        entree.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        entree.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        entree.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        entree.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, FREQUENCE)?;
        entree.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, canaux as u32)?;
        entree.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, canaux as u32 * 2)?;
        entree.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, FREQUENCE * canaux as u32 * 2)?;
        let sortie = MFCreateMediaType()?;
        sortie.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        sortie.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        sortie.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        sortie.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, FREQUENCE)?;
        sortie.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, canaux as u32)?;
        sortie.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, kbps * 125)?;
        sortie.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        sortie.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
        transformation.SetInputType(0, &entree, 0)?;
        transformation.SetOutputType(0, &sortie, 0)?;
        transformation.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        let taille_sortie = transformation.GetOutputStreamInfo(0)?.cbSize.max(8192);
        Ok(Self { transformation, canaux, attente: Vec::new(), entres: 0, sortis: 0, taille_sortie })
    }

    unsafe fn coder(&mut self, pcm: &[i16], recu: &mut impl FnMut(i64, i64, Vec<u8>)) -> windows::core::Result<()> {
        self.attente.extend_from_slice(pcm);
        let par_bloc = BLOC * self.canaux;
        while self.attente.len() >= par_bloc {
            let bloc: Vec<u8> = self.attente.drain(..par_bloc).flat_map(i16::to_le_bytes).collect();
            let tampon = MFCreateMemoryBuffer(bloc.len() as u32)?;
            let mut pointeur = std::ptr::null_mut();
            tampon.Lock(&mut pointeur, None, None)?;
            std::ptr::copy_nonoverlapping(bloc.as_ptr(), pointeur, bloc.len());
            tampon.Unlock()?;
            tampon.SetCurrentLength(bloc.len() as u32)?;
            let echantillon = MFCreateSample()?;
            echantillon.AddBuffer(&tampon)?;
            echantillon.SetSampleTime(temps(self.entres))?;
            echantillon.SetSampleDuration(temps(BLOC as u64))?;
            self.entres += BLOC as u64;
            self.transformation.ProcessInput(0, &echantillon, 0)?;
            self.vider(recu)?;
        }
        Ok(())
    }

    unsafe fn vider(&mut self, recu: &mut impl FnMut(i64, i64, Vec<u8>)) -> windows::core::Result<()> {
        loop {
            let echantillon = MFCreateSample()?;
            echantillon.AddBuffer(&MFCreateMemoryBuffer(self.taille_sortie)?)?;
            let mut sorties = [MFT_OUTPUT_DATA_BUFFER { pSample: ManuallyDrop::new(Some(echantillon)), ..Default::default() }];
            let mut statut = 0u32;
            let resultat = self.transformation.ProcessOutput(0, &mut sorties, &mut statut);
            let [sortie] = sorties;
            let echantillon = ManuallyDrop::into_inner(sortie.pSample);
            drop(ManuallyDrop::into_inner(sortie.pEvents));
            match resultat {
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                autre => autre?,
            }
            let Some(echantillon) = echantillon else { return Ok(()) };
            let donnees = octets(&echantillon)?;
            if !donnees.is_empty() {
                let pts = echantillon.GetSampleTime().unwrap_or_else(|_| temps(self.sortis));
                recu(pts, temps(BLOC as u64), donnees);
                self.sortis += BLOC as u64;
            }
        }
    }
}

pub struct Contexte {
    pub horloge: Instant,
    pub base: i64,
    pub tampon: Arc<Mutex<Tampon>>,
    pub arret: Arc<AtomicBool>,
}

fn piste(piste: Piste, canaux: usize, kbps: u32, ouvrir: impl FnOnce() -> Result<AudioClient, String>, mut traitement: Option<Traitement>, c: Contexte, pret: impl FnOnce(Result<(), String>)) {
    let aac = std::cell::RefCell::new(None);
    let mut alignement = Alignement::nouveau(FREQUENCE);
    let (tampon, base, horloge) = (c.tampon.clone(), c.base, c.horloge);
    let mut pousser = move |pts: i64, duree: i64, donnees: Vec<u8>| {
        if let Ok(mut t) = tampon.lock() {
            t.pousser(piste, Unite { pts: base + pts, duree, cle: true, donnees: donnees.into() });
        }
    };
    capturer(
        ouvrir,
        canaux,
        &c.arret,
        |ouverture| {
            let encodeur = ouverture.and_then(|_| Aac::creer(canaux, kbps));
            pret(encodeur.map(|e| *aac.borrow_mut() = Some(e)));
        },
        |file| {
            let mut aac = aac.borrow_mut();
            let Some(aac) = aac.as_mut() else { return false };
            let entiers = file.len() / (4 * canaux) * (4 * canaux);
            if entiers == 0 {
                return true;
            }
            let recus: Vec<u8> = file.drain(..entiers).collect();
            let ecoule = (horloge.elapsed().as_nanos() / 100) as i64;
            let silence = alignement.silence(ecoule, (entiers / (4 * canaux)) as u64);
            let mut pcm = vec![0i16; silence as usize * canaux];
            match traitement.as_mut() {
                Some(t) => pcm.extend(t.traiter(&flottants(&recus).collect::<Vec<f32>>())),
                None => pcm.extend(pcm16(&recus)),
            }
            unsafe { aac.coder(&pcm, &mut pousser) }.is_ok()
        },
    );
}

pub fn jeu(pid: u32, kbps: u32, c: Contexte, pret: Sender<Result<(), String>>) {
    let ouvrir = || AudioClient::new_application_loopback_client(pid, true).map_err(|e| e.to_string());
    piste(Piste::Jeu, 2, kbps, ouvrir, None, c, |r| {
        let _ = pret.send(r);
    });
}

pub fn micro(libelle: Option<String>, reduction: String, gain: f32, kbps: u32, c: Contexte, pret: Sender<Result<bool, String>>) {
    let par_defaut = std::cell::Cell::new(false);
    let ouvrir = || -> Result<AudioClient, String> {
        let enumerateur = DeviceEnumerator::new().map_err(|e| e.to_string())?;
        let collection = enumerateur.get_device_collection(&Direction::Capture).map_err(|e| e.to_string())?;
        let nombre = collection.get_nbr_devices().map_err(|e| e.to_string())?;
        let noms: Vec<String> = (0..nombre).map(|i| collection.get_device_at_index(i).and_then(|d| d.get_friendlyname()).unwrap_or_default()).collect();
        let peripherique = match micro::choisir(&noms, libelle.as_deref()) {
            Some(i) => collection.get_device_at_index(i as u32),
            None => {
                par_defaut.set(libelle.is_some());
                enumerateur.get_default_device(&Direction::Capture)
            }
        };
        peripherique.and_then(|p| p.get_iaudioclient()).map_err(|e| format!("micro inaccessible : {e}"))
    };
    piste(Piste::Micro, 1, kbps, ouvrir, Some(Traitement::nouveau(&reduction, gain)), c, |r| {
        let _ = pret.send(r.map(|_| par_defaut.get()));
    });
}
