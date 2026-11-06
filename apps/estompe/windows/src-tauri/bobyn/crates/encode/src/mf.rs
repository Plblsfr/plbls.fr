//! Encodeur H.264 matériel via Media Foundation. Le MFT est asynchrone : il annonce par événement
//! qu'il veut une image (`NeedInput`) ou qu'un paquet est prêt (`HaveOutput`).
use crate::Encoder;
use bobyn_buffer::Packet;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{Error, Interface, Result, GUID};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Variant::*;

/// Si l'encodeur n'a rien produit pendant ce délai alors qu'on lui a donné des images, on abandonne.
const SILENCE_LIMIT: Duration = Duration::from_secs(3);

pub struct MfEncoder {
    mft: IMFTransform,
    events: IMFMediaEventGenerator,
    _manager: IMFDXGIDeviceManager,
    name: String,
    frame_duration: i64,
    /// Nombre d'images que l'encodeur a demandées et qu'on ne lui a pas encore données.
    wanted: u32,
    allocates_output: bool,
    output_size: u32,
    /// SPS/PPS de l'encodeur, ajoutés aux images clés qui n'en portent pas.
    header: Vec<u8>,
    dropped: u64,
    submitted: u64,
    last_output: Instant,
}

fn variant_u32(v: u32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 { vt: VT_UI4, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: VARIANT_0_0_0 { ulVal: v } }),
        },
    }
}

fn pair(hi: u32, lo: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

impl MfEncoder {
    pub fn new(device: &ID3D11Device, width: u32, height: u32, fps: u32, mbps: u32, gop_seconds: u32) -> Result<Self> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let r = Self::open(device, width, height, fps, mbps, gop_seconds);
            if r.is_err() {
                let _ = MFShutdown();
            }
            r
        }
    }

    unsafe fn open(device: &ID3D11Device, w: u32, h: u32, fps: u32, mbps: u32, gop_s: u32) -> Result<Self> {
        let (mut token, mut manager) = (0, None);
        MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
        let manager = manager.unwrap();
        manager.ResetDevice(device, token)?;

        let nv12 = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
        let h264 = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
        let (mut list, mut count) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0);
        MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER, Some(&nv12), Some(&h264), &mut list, &mut count)?;
        let candidates: Vec<IMFActivate> = (0..count as usize).filter_map(|i| (*list.add(i)).take()).collect();
        CoTaskMemFree(Some(list as _));

        let mut last = Error::new(E_FAIL, "aucun encodeur H.264 matériel");
        for act in candidates {
            match Self::configure(&act, &manager, w, h, fps, mbps, gop_s) {
                Ok(e) => return Ok(e),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    unsafe fn configure(act: &IMFActivate, manager: &IMFDXGIDeviceManager, w: u32, h: u32, fps: u32, mbps: u32, gop_s: u32) -> Result<Self> {
        let mut name = [0u16; 128];
        let mut len = 0;
        let name = match act.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut name, Some(&mut len)) {
            Ok(()) => String::from_utf16_lossy(&name[..len as usize]),
            Err(_) => "encodeur matériel".into(),
        };
        let mft: IMFTransform = act.ActivateObject()?;
        let attrs = mft.GetAttributes()?;
        attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
        attrs.SetUINT32(&MF_LOW_LATENCY, 1)?;
        mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)?;

        // Débit constant, une image clé par `gop_s` secondes, pas d'images B (elles retardent la sortie).
        if let Ok(api) = mft.cast::<ICodecAPI>() {
            let set = |key: &GUID, v: u32| {
                let _ = api.SetValue(key, &variant_u32(v));
            };
            set(&CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_CBR.0 as u32);
            set(&CODECAPI_AVEncCommonMeanBitRate, mbps * 1_000_000);
            set(&CODECAPI_AVEncMPVGOPSize, fps * gop_s);
            set(&CODECAPI_AVEncMPVDefaultBPictureCount, 0);
            set(&CODECAPI_AVLowLatencyMode, 1);
        }

        let video_type = |subtype: &GUID| -> Result<IMFMediaType> {
            let t = MFCreateMediaType()?;
            t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            t.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            t.SetUINT64(&MF_MT_FRAME_SIZE, pair(w, h))?;
            t.SetUINT64(&MF_MT_FRAME_RATE, pair(fps, 1))?;
            t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
            t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            Ok(t)
        };
        let out = video_type(&MFVideoFormat_H264)?;
        out.SetUINT32(&MF_MT_AVG_BITRATE, mbps * 1_000_000)?;
        out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
        mft.SetOutputType(0, &out, 0)?;
        mft.SetInputType(0, &video_type(&MFVideoFormat_NV12)?, 0)?;

        let info = mft.GetOutputStreamInfo(0)?;
        let current = mft.GetOutputCurrentType(0)?;
        let header = match current.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
            Ok(n) => {
                let mut b = vec![0u8; n as usize];
                current.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut b, None)?;
                b
            }
            Err(_) => Vec::new(),
        };
        let events: IMFMediaEventGenerator = mft.cast()?;
        mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)?;
        mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
        Ok(Self {
            mft,
            events,
            _manager: manager.clone(),
            name,
            frame_duration: 10_000_000 / fps as i64,
            wanted: 0,
            allocates_output: info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0,
            output_size: info.cbSize.max(1 << 20),
            header,
            dropped: 0,
            submitted: 0,
            last_output: Instant::now(),
        })
    }

    /// Lit les événements en attente sans bloquer ; renvoie les paquets sortis.
    unsafe fn drain(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        loop {
            let event = match self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                Ok(e) => e,
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(()),
                Err(e) => return Err(e),
            };
            let kind = event.GetType()?;
            if kind == METransformNeedInput.0 as u32 {
                self.wanted += 1;
            } else if kind == METransformHaveOutput.0 as u32 {
                if let Some(p) = self.take_output()? {
                    self.last_output = Instant::now();
                    out.push(p);
                }
            }
        }
    }

    unsafe fn take_output(&mut self) -> Result<Option<Packet>> {
        let provided = if self.allocates_output {
            None
        } else {
            let s = MFCreateSample()?;
            s.AddBuffer(&MFCreateMemoryBuffer(self.output_size)?)?;
            Some(s)
        };
        let mut buffers = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(provided), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
        let mut status = 0;
        let result = self.mft.ProcessOutput(0, &mut buffers, &mut status);
        let sample = ManuallyDrop::take(&mut buffers[0].pSample);
        drop(ManuallyDrop::take(&mut buffers[0].pEvents));
        match result {
            Ok(()) => {}
            Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
            Err(e) => return Err(e),
        }
        let Some(sample) = sample else { return Ok(None) };
        let buffer = sample.ConvertToContiguousBuffer()?;
        let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
        buffer.Lock(&mut ptr, None, Some(&mut len))?;
        let bytes = std::slice::from_raw_parts(ptr, len as usize).to_vec();
        buffer.Unlock()?;
        let keyframe = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
        let data = if keyframe && !self.header.is_empty() && !has_sps(&bytes) { [self.header.as_slice(), &bytes].concat() } else { bytes };
        Ok(Some(Packet { pts_us: sample.GetSampleTime()? / 10, keyframe, data: Arc::from(data) }))
    }
}

fn has_sps(annex_b: &[u8]) -> bool {
    annex_b.windows(4).any(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == 7)
}

impl Encoder for MfEncoder {
    fn name(&self) -> &str {
        &self.name
    }

    fn encode(&mut self, nv12: &ID3D11Texture2D, pts_us: i64) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        unsafe {
            self.drain(&mut out)?;
            if self.wanted == 0 {
                // L'encodeur n'est pas prêt : on saute cette image plutôt que de bloquer le pipeline.
                self.dropped += 1;
                if self.submitted > 0 && self.last_output.elapsed() > SILENCE_LIMIT {
                    return Err(Error::new(E_FAIL, "l'encodeur matériel ne produit plus aucun paquet"));
                }
                return Ok(out);
            }
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, nv12, 0, false)?;
            buffer.SetCurrentLength(buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(pts_us * 10)?;
            sample.SetSampleDuration(self.frame_duration)?;
            self.mft.ProcessInput(0, &sample, 0)?;
            self.wanted -= 1;
            self.submitted += 1;
            self.drain(&mut out)?;
        }
        Ok(out)
    }

    fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl Drop for MfEncoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, 0);
            let _ = MFShutdown();
        }
    }
}
