//! Encodeur AMD AMF (H.264, HEVC, AV1) : entrée = texture NV12 D3D11 du GPU, sortie = paquets Annex-B.
//!
//! AMF expose des interfaces COM-like en C++ ; on appelle leurs méthodes par numéro de slot de vtable,
//! relevés dans les en-têtes officiels du SDK (AMF 1.5, `core/*.h`, `components/Component.h`).
use crate::Encoder;
use bobyn_buffer::{Codec, Packet};
use std::ffi::c_void;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{s, Error, Interface, Result, GUID};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const SILENCE_LIMIT: Duration = Duration::from_secs(3);

// Résultats AMF_RESULT
const AMF_OK: i32 = 0;
const AMF_REPEAT: i32 = 24;
const AMF_INPUT_FULL: i32 = 25;
// AMF_MEMORY_DX11 n'est pas utilisé directement ; formats et versions :
const SURFACE_NV12: i32 = 1;
const DX11_0: i32 = 110;

// Slots des vtables (ordre des méthodes dans les en-têtes).
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

/// IID de AMFBuffer : {b04b7248-b6f0-4321-b691-baa4740f9fcb}
const IID_BUFFER: GUID = GUID::from_values(0xb04b7248, 0xb6f0, 0x4321, [0xb6, 0x91, 0xba, 0xa4, 0x74, 0x0f, 0x9f, 0xcb]);

type Obj = *mut c_void;

/// AMFVariantStruct : type (int32) puis une union de 16 octets alignée sur 8.
#[repr(C)]
#[derive(Clone, Copy)]
struct Variant {
    kind: i32,
    data: [u64; 2],
}

impl Variant {
    fn int(v: i64) -> Self {
        Self { kind: 2, data: [v as u64, 0] }
    }
    fn boolean(v: bool) -> Self {
        Self { kind: 1, data: [v as u64, 0] }
    }
    fn size(w: u32, h: u32) -> Self {
        Self { kind: 5, data: [(h as u64) << 32 | w as u64, 0] }
    }
    fn rate(num: u32, den: u32) -> Self {
        Self { kind: 7, data: [(den as u64) << 32 | num as u64, 0] }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Récupère la fonction du slot `index` de la vtable de `obj`.
unsafe fn slot<F: Copy>(obj: Obj, index: usize) -> F {
    if obj.is_null() {
        panic!("AMF : objet nul");
    }
    let vtbl = *(obj as *const *const usize);
    if vtbl.is_null() {
        panic!("AMF : vtable nulle");
    }
    std::mem::transmute_copy(&*vtbl.add(index))
}

unsafe fn release(obj: Obj) {
    if !obj.is_null() {
        slot::<unsafe extern "system" fn(Obj) -> i32>(obj, RELEASE)(obj);
    }
}

fn check(code: i32, what: &str) -> Result<()> {
    if code == AMF_OK {
        Ok(())
    } else {
        Err(Error::new(E_FAIL, format!("AMF {what} : code {code}")))
    }
}

unsafe fn set(obj: Obj, name: &str, v: Variant) -> i32 {
    slot::<unsafe extern "system" fn(Obj, *const u16, Variant) -> i32>(obj, SET_PROPERTY)(obj, wide(name).as_ptr(), v)
}

pub struct AmfEncoder {
    codec: Codec,
    context: Obj,
    component: Obj,
    idr_period: i64,
    dropped: u64,
    submitted: u64,
    last_output: Instant,
}

impl AmfEncoder {
    pub fn new(device: &ID3D11Device, codec: Codec, width: u32, height: u32, fps: u32, mbps: u32, gop_seconds: u32) -> Result<Self> {
        unsafe {
            let dll = LoadLibraryW(windows::core::w!("amfrt64.dll"))?;
            let init = GetProcAddress(dll, s!("AMFInit")).ok_or_else(|| Error::new(E_FAIL, "AMFInit introuvable"))?;
            let query = GetProcAddress(dll, s!("AMFQueryVersion")).ok_or_else(|| Error::new(E_FAIL, "AMFQueryVersion introuvable"))?;
            let mut version = 0u64;
            check(std::mem::transmute::<_, unsafe extern "system" fn(*mut u64) -> i32>(query)(&mut version), "AMFQueryVersion")?;
            let mut factory: Obj = std::ptr::null_mut();
            check(std::mem::transmute::<_, unsafe extern "system" fn(u64, *mut Obj) -> i32>(init)(version, &mut factory), "AMFInit")?;

            // Le factory n'a pas de AddRef/Release : ses méthodes commencent à CreateContext (slot 0).
            let mut context: Obj = std::ptr::null_mut();
            check(slot::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(factory, 0)(factory, &mut context), "CreateContext")?;
            let built = (|| {
                check(slot::<unsafe extern "system" fn(Obj, Obj, i32) -> i32>(context, CTX_INIT_DX11)(context, device.as_raw(), DX11_0), "InitDX11")?;
                let mut component: Obj = std::ptr::null_mut();
                let id = wide(match codec {
                    Codec::H264 => "AMFVideoEncoderVCE_AVC",
                    Codec::Hevc => "AMFVideoEncoderHW_HEVC",
                    Codec::Av1 => "AMFVideoEncoderHW_AV1",
                });
                check(slot::<unsafe extern "system" fn(Obj, Obj, *const u16, *mut Obj) -> i32>(factory, 1)(factory, context, id.as_ptr(), &mut component), "CreateComponent")?;
                let idr_period = (fps * gop_seconds) as i64;
                let bits = mbps as i64 * 1_000_000;
                // Mêmes réglages pour les trois codecs ; seuls les noms et les valeurs d'énumération changent.
                let props: Vec<(&str, Variant)> = match codec {
                    Codec::H264 => vec![
                        ("Usage", Variant::int(2)), // LOW_LATENCY : pas d'images B, sortie immédiate
                        ("Profile", Variant::int(100)), // High
                        ("QualityPreset", Variant::int(0)), // BALANCED
                        ("FrameSize", Variant::size(width, height)),
                        ("FrameRate", Variant::rate(fps, 1)),
                        ("RateControlMethod", Variant::int(1)), // CBR
                        ("TargetBitrate", Variant::int(bits)),
                        ("PeakBitrate", Variant::int(bits)),
                        ("BPicturesPattern", Variant::int(0)),
                        ("IDRPeriod", Variant::int(idr_period)),
                        ("HeaderInsertionSpacing", Variant::int(idr_period)),
                        ("InsertSPS", Variant::boolean(true)),
                    ],
                    Codec::Hevc => vec![
                        ("HevcUsage", Variant::int(2)), // LOW_LATENCY
                        ("HevcProfile", Variant::int(1)), // Main
                        ("HevcQualityPreset", Variant::int(5)), // BALANCED
                        ("HevcFrameSize", Variant::size(width, height)),
                        ("HevcFrameRate", Variant::rate(fps, 1)),
                        ("HevcRateControlMethod", Variant::int(3)), // CBR
                        ("HevcTargetBitrate", Variant::int(bits)),
                        ("HevcPeakBitrate", Variant::int(bits)),
                        ("HevcGOPSize", Variant::int(idr_period)),
                        ("HevcHeaderInsertionMode", Variant::int(2)), // IDR_ALIGNED
                    ],
                    Codec::Av1 => vec![
                        ("Av1Usage", Variant::int(1)), // LOW_LATENCY
                        ("Av1Profile", Variant::int(1)), // Main
                        ("Av1QualityPreset", Variant::int(70)), // BALANCED
                        ("Av1FrameSize", Variant::size(width, height)),
                        ("Av1FrameRate", Variant::rate(fps, 1)),
                        ("Av1RateControlMethod", Variant::int(3)), // CBR
                        ("Av1TargetBitrate", Variant::int(bits)),
                        ("Av1PeakBitrate", Variant::int(bits)),
                        ("Av1GOPSize", Variant::int(idr_period)),
                        ("Av1AlignmentMode", Variant::int(3)), // NO_RESTRICTIONS : 1080 sans recadrage à 1088
                        ("Av1HeaderInsertionMode", Variant::int(2)), // KEY_FRAME_ALIGNED
                    ],
                };
                for (name, v) in props {
                    check(set(component, name, v), name)?;
                }
                check(slot::<unsafe extern "system" fn(Obj, i32, i32, i32) -> i32>(component, COMP_INIT)(component, SURFACE_NV12, width as i32, height as i32), "Init")?;
                Ok((component, idr_period))
            })();
            match built {
                Ok((component, idr_period)) => Ok(Self { codec, context, component, idr_period, dropped: 0, submitted: 0, last_output: Instant::now() }),
                Err(e) => {
                    slot::<unsafe extern "system" fn(Obj) -> i32>(context, CTX_TERMINATE)(context);
                    release(context);
                    Err(e)
                }
            }
        }
    }

    /// Récupère les paquets prêts (sans bloquer).
    unsafe fn drain(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        loop {
            let mut data: Obj = std::ptr::null_mut();
            let r = slot::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(self.component, COMP_QUERY_OUTPUT)(self.component, &mut data);
            if r == AMF_REPEAT || data.is_null() {
                return Ok(());
            }
            check(r, "QueryOutput")?;
            let packet = self.packet(data);
            release(data);
            out.push(packet?);
            self.last_output = Instant::now();
        }
    }

    unsafe fn packet(&self, data: Obj) -> Result<Packet> {
        let mut buffer: Obj = std::ptr::null_mut();
        check(slot::<unsafe extern "system" fn(Obj, *const GUID, *mut Obj) -> i32>(data, QUERY_INTERFACE)(data, &IID_BUFFER, &mut buffer), "QueryInterface(AMFBuffer)")?;
        let size = slot::<unsafe extern "system" fn(Obj) -> usize>(buffer, BUFFER_GET_SIZE)(buffer);
        let native = slot::<unsafe extern "system" fn(Obj) -> *const u8>(buffer, BUFFER_GET_NATIVE)(buffer);
        let bytes: Arc<[u8]> = Arc::from(std::slice::from_raw_parts(native, size));
        let pts = slot::<unsafe extern "system" fn(Obj) -> i64>(data, DATA_GET_PTS)(data);
        let mut kind = Variant::int(-1);
        let name = wide(match self.codec {
            Codec::H264 => "OutputDataType",
            Codec::Hevc => "HevcOutputDataType",
            Codec::Av1 => "Av1OutputFrameType",
        });
        slot::<unsafe extern "system" fn(Obj, *const u16, *mut Variant) -> i32>(data, GET_PROPERTY)(data, name.as_ptr(), &mut kind);
        release(buffer);
        Ok(Packet { pts_us: pts / 10, keyframe: kind.data[0] as i64 == 0, data: bytes }) // 0 = IDR (H.264, HEVC) ou KEY (AV1)
    }
}

impl Encoder for AmfEncoder {
    fn name(&self) -> &str {
        match self.codec {
            Codec::H264 => "AMD AMF H.264",
            Codec::Hevc => "AMD AMF HEVC",
            Codec::Av1 => "AMD AMF AV1",
        }
    }

    fn encode(&mut self, nv12: &ID3D11Texture2D, pts_us: i64) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        unsafe {
            self.drain(&mut out)?;
            let mut surface: Obj = std::ptr::null_mut();
            check(
                slot::<unsafe extern "system" fn(Obj, Obj, *mut Obj, Obj) -> i32>(self.context, CTX_SURFACE_FROM_DX11)(self.context, nv12.as_raw(), &mut surface, std::ptr::null_mut()),
                "CreateSurfaceFromDX11Native",
            )?;
            slot::<unsafe extern "system" fn(Obj, i64)>(surface, DATA_SET_PTS)(surface, pts_us * 10);
            // L'encodeur ignore IDRPeriod en mode faible latence : on force nous-mêmes une image clé par GOP.
            if self.submitted as i64 % self.idr_period == 0 {
                match self.codec {
                    Codec::H264 => {
                        set(surface, "ForcePictureType", Variant::int(2)); // IDR
                        set(surface, "InsertSPS", Variant::boolean(true));
                        set(surface, "InsertPPS", Variant::boolean(true));
                    }
                    Codec::Hevc => {
                        set(surface, "HevcForcePictureType", Variant::int(2)); // IDR
                        set(surface, "HevcInsertHeader", Variant::boolean(true));
                    }
                    Codec::Av1 => {
                        set(surface, "Av1ForceFrameType", Variant::int(1)); // KEY
                        set(surface, "Av1ForceInsertSequenceHeader", Variant::boolean(true));
                    }
                }
            }
            let r = slot::<unsafe extern "system" fn(Obj, Obj) -> i32>(self.component, COMP_SUBMIT_INPUT)(self.component, surface);
            release(surface);
            if r == AMF_INPUT_FULL {
                self.dropped += 1;
            } else {
                check(r, "SubmitInput")?;
                self.submitted += 1;
            }
            self.drain(&mut out)?;
            if self.submitted > 0 && out.is_empty() && self.last_output.elapsed() > SILENCE_LIMIT {
                return Err(Error::new(E_FAIL, "l'encodeur AMF ne produit plus aucun paquet"));
            }
        }
        Ok(out)
    }

    fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl Drop for AmfEncoder {
    fn drop(&mut self) {
        unsafe {
            slot::<unsafe extern "system" fn(Obj) -> i32>(self.component, COMP_TERMINATE)(self.component);
            release(self.component);
            slot::<unsafe extern "system" fn(Obj) -> i32>(self.context, CTX_TERMINATE)(self.context);
            release(self.context);
        }
    }
}
