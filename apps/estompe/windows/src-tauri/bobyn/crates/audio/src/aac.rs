//! Encodage AAC-LC 128 kb/s par le MFT Media Foundation, en une passe, au moment de la sauvegarde.
use bobyn_buffer::{CHANNELS, RATE};
use std::mem::ManuallyDrop;
use windows::core::{Error, Result};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

fn media_type(subtype: windows::core::GUID, bytes_per_sec: u32, aac: bool) -> Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        t.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE as u32)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_sec)?;
        if aac {
            t.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?; // trames brutes, sans ADTS
            t.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?; // AAC-LC niveau 2
        } else {
            t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, CHANNELS as u32 * 2)?;
        }
        Ok(t)
    }
}

/// Encode du PCM 16 bits entrelacé 48 kHz stéréo ; renvoie une trame AAC brute (1024 échantillons) par élément.
pub fn encode(pcm: &[i16]) -> Result<Vec<Vec<u8>>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        let result = run(pcm);
        let _ = MFShutdown();
        result
    }
}

unsafe fn run(pcm: &[i16]) -> Result<Vec<Vec<u8>>> {
    let mft: IMFTransform = CoCreateInstance(&AACMFTEncoder, None, CLSCTX_INPROC_SERVER)?;
    // Le type de sortie d'abord : l'encodeur n'accepte les types d'entrée qu'après.
    mft.SetOutputType(0, &media_type(MFAudioFormat_AAC, 16_000, true)?, 0)?;
    mft.SetInputType(0, &media_type(MFAudioFormat_PCM, (RATE as u32) * CHANNELS as u32 * 2, false)?, 0)?;
    let out_size = mft.GetOutputStreamInfo(0)?.cbSize.max(1024);
    mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
    mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

    let mut frames = Vec::new();
    let drain = |frames: &mut Vec<Vec<u8>>| -> Result<()> {
        loop {
            let sample = MFCreateSample()?;
            sample.AddBuffer(&MFCreateMemoryBuffer(out_size)?)?;
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(Some(sample)), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
            let mut status = 0;
            let r = mft.ProcessOutput(0, &mut buffers, &mut status);
            let sample = ManuallyDrop::take(&mut buffers[0].pSample);
            drop(ManuallyDrop::take(&mut buffers[0].pEvents));
            match r {
                Ok(()) => {}
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) => return Err(e),
            }
            let buffer = sample.ok_or_else(|| Error::new(E_FAIL, "AAC : sortie sans échantillon"))?.ConvertToContiguousBuffer()?;
            let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
            buffer.Lock(&mut ptr, None, Some(&mut len))?;
            frames.push(std::slice::from_raw_parts(ptr, len as usize).to_vec());
            buffer.Unlock()?;
        }
    };
    // Entrée par blocs d'une seconde : bornée en mémoire, sans pression sur l'encodeur.
    for (i, block) in pcm.chunks(RATE as usize * CHANNELS).enumerate() {
        let bytes = block.len() * 2;
        let buffer = MFCreateMemoryBuffer(bytes as u32)?;
        let (mut ptr, mut max) = (std::ptr::null_mut(), 0);
        buffer.Lock(&mut ptr, Some(&mut max), None)?;
        std::ptr::copy_nonoverlapping(block.as_ptr() as *const u8, ptr, bytes);
        buffer.Unlock()?;
        buffer.SetCurrentLength(bytes as u32)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(i as i64 * 10_000_000)?; // blocs d'une seconde, en 100 ns
        sample.SetSampleDuration((block.len() / CHANNELS) as i64 * 10_000_000 / RATE)?;
        mft.ProcessInput(0, &sample, 0)?;
        drain(&mut frames)?;
    }
    mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)?;
    mft.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
    drain(&mut frames)?;
    Ok(frames)
}
