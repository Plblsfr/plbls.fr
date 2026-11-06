//! Encodeurs matériels derrière un trait commun ; entrée = texture NV12 sur le GPU.
#![cfg(windows)]
mod amf;
mod mf;
pub use amf::AmfEncoder;
pub use mf::MfEncoder;

use bobyn_buffer::{Codec, Packet};
use windows::core::Result;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

pub trait Encoder {
    fn name(&self) -> &str;
    /// Renvoie les paquets disponibles ; l'image est ignorée si l'encodeur est en retard.
    fn encode(&mut self, nv12: &ID3D11Texture2D, pts_us: i64) -> Result<Vec<Packet>>;
    fn dropped(&self) -> u64;
}

/// Codecs que le GPU sait encoder, d'après un essai d'initialisation en 720p (H.264 est toujours proposé : repli Media Foundation).
pub fn supported(device: &windows::Win32::Graphics::Direct3D11::ID3D11Device) -> Vec<Codec> {
    Codec::ALL.into_iter().filter(|&c| c == Codec::H264 || AmfEncoder::new(device, c, 1280, 720, 60, 10, 1).is_ok()).collect()
}

/// Ouvre le meilleur encodeur disponible : AMF d'abord (accès direct au matériel), puis Media Foundation pour H.264 seulement.
pub fn open(device: &windows::Win32::Graphics::Direct3D11::ID3D11Device, codec: Codec, w: u32, h: u32, fps: u32, mbps: u32, gop_seconds: u32) -> Result<Box<dyn Encoder>> {
    match AmfEncoder::new(device, codec, w, h, fps, mbps, gop_seconds) {
        Ok(e) => Ok(Box::new(e)),
        Err(amf_err) if codec == Codec::H264 => {
            eprintln!("AMF indisponible ({amf_err}), repli sur Media Foundation");
            Ok(Box::new(MfEncoder::new(device, w, h, fps, mbps, gop_seconds)?))
        }
        Err(e) => Err(e),
    }
}
