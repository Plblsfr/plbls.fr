use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::{factory, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::gpu::Appareil;
use super::materiel::texte;

const REFUS: &str = "capture refusée par le jeu ou par Windows";

#[derive(Clone)]
pub struct Image {
    pub texture: ID3D11Texture2D,
    _cadre: Direct3D11CaptureFrame,
}

unsafe impl Send for Image {}

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    derniere: Arc<Mutex<Option<Image>>>,
    pub taille: (u32, u32),
}

pub fn element(fenetre: isize) -> Result<GraphicsCaptureItem, String> {
    if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
        return Err("capture de fenêtre indisponible sur cette version de Windows".into());
    }
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(texte)?;
    unsafe { interop.CreateForWindow::<GraphicsCaptureItem>(HWND(fenetre as *mut _)) }.map_err(|e| format!("{REFUS} : {}", texte(e)))
}

impl Capture {
    pub fn demarrer(a: &Appareil, element: &GraphicsCaptureItem, curseur: bool, hdr: bool, change: impl Fn() + Send + 'static) -> Result<Self, String> {
        Self::demarrer_brut(a, element, curseur, hdr, change).map_err(|e| format!("{REFUS} : {}", texte(e)))
    }

    fn demarrer_brut(a: &Appareil, element: &GraphicsCaptureItem, curseur: bool, hdr: bool, change: impl Fn() + Send + 'static) -> windows::core::Result<Self> {
        let taille = element.Size()?;
        let format = if hdr { DirectXPixelFormat::R16G16B16A16Float } else { DirectXPixelFormat::B8G8R8A8UIntNormalized };
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&a.winrt, format, 3, taille)?;
        let derniere = Arc::new(Mutex::new(None));
        let (partage, signale) = (derniere.clone(), AtomicBool::new(false));
        pool.FrameArrived(&TypedEventHandler::new(move |pool: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
            let Some(pool) = pool.as_ref() else { return Ok(()) };
            let cadre = pool.TryGetNextFrame()?;
            let contenu = cadre.ContentSize()?;
            if (contenu.Width, contenu.Height) != (taille.Width, taille.Height) {
                if !signale.swap(true, Ordering::Relaxed) {
                    change();
                }
                return Ok(());
            }
            let texture = unsafe { cadre.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface::<ID3D11Texture2D>()? };
            if let Ok(mut d) = partage.lock() {
                *d = Some(Image { texture, _cadre: cadre });
            }
            Ok(())
        }))?;
        let session = pool.CreateCaptureSession(element)?;
        let _ = session.SetIsCursorCaptureEnabled(curseur);
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        Ok(Self { pool, session, derniere, taille: (taille.Width.max(2) as u32, taille.Height.max(2) as u32) })
    }

    pub fn derniere(&self) -> Option<Image> {
        self.derniere.lock().ok().and_then(|d| d.clone())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
        if let Ok(mut d) = self.derniere.lock() {
            *d = None;
        }
    }
}
