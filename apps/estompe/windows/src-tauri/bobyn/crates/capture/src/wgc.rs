//! Windows.Graphics.Capture : l'image arrive en texture GPU, réveil par événement.
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use windows::core::{factory, IInspectable, Interface, Result};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::*;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{HMONITOR, MonitorFromPoint, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

pub struct Frame {
    pub texture: ID3D11Texture2D,
    /// `SystemRelativeTime`, en unités de 100 ns.
    pub time_100ns: i64,
    // La texture appartient au pool tant que ce frame n'est pas libéré.
    _frame: Direct3D11CaptureFrame,
}

// Les frames d'un pool free-threaded passent d'un thread à l'autre sans contrainte d'appartement.
struct SendFrame(Frame);
unsafe impl Send for SendFrame {}

#[derive(Default)]
struct Mailbox {
    latest: Mutex<Option<SendFrame>>,
    arrived: Condvar,
}

pub struct Capture {
    pub width: u32,
    pub height: u32,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    token: i64,
    mailbox: Arc<Mailbox>,
    /// Source fermée ou redimensionnée : le pipeline doit être recréé (tailles fixes en aval).
    lost: Arc<AtomicBool>,
    item: GraphicsCaptureItem,
    closed_token: i64,
}

impl Capture {
    /// Capture l'écran principal.
    pub fn primary_monitor(device: &ID3D11Device, cursor: bool, min_interval_100ns: i64) -> Result<Self> {
        unsafe {
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let item = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?.CreateForMonitor(monitor)?;
            Self::start(device, item, cursor, min_interval_100ns)
        }
    }

    pub fn monitor(device: &ID3D11Device, monitor: HMONITOR, cursor: bool, min_interval_100ns: i64) -> Result<Self> {
        let item = unsafe { factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?.CreateForMonitor(monitor)? };
        Self::start(device, item, cursor, min_interval_100ns)
    }

    /// Capture une fenêtre précise (sans les autres fenêtres ni notifications qui la recouvrent).
    pub fn window(device: &ID3D11Device, hwnd: HWND, cursor: bool, min_interval_100ns: i64) -> Result<Self> {
        let item = unsafe { factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?.CreateForWindow(hwnd)? };
        Self::start(device, item, cursor, min_interval_100ns)
    }

    pub fn lost(&self) -> bool {
        self.lost.load(Relaxed)
    }

    /// `min_interval_100ns` : cadence maximale de livraison des images (Windows 11 22H2+), pour ne pas faire travailler le compositeur à la fréquence du jeu.
    fn start(device: &ID3D11Device, item: GraphicsCaptureItem, cursor: bool, min_interval_100ns: i64) -> Result<Self> {
        let size = item.Size()?;
        let dxgi: IDXGIDevice = device.cast()?;
        let winrt_device: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()? };
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt_device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 4, size)?;
        let mailbox = Arc::new(Mailbox::default());
        let lost = Arc::new(AtomicBool::new(false));
        let (mb, p, lo) = (mailbox.clone(), pool.clone(), lost.clone());
        let token = pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |_, _| {
            // On vide le pool pour qu'il ne manque jamais de tampon ; seule la plus récente est gardée.
            let mut newest = None;
            while let Ok(f) = p.TryGetNextFrame() {
                newest = Some(f);
            }
            if let Some(f) = &newest {
                if f.ContentSize().map_or(false, |c| c.Width != size.Width || c.Height != size.Height) {
                    lo.store(true, Relaxed);
                    mb.arrived.notify_one();
                }
            }
            if let Some(f) = newest.and_then(|f| to_frame(f)) {
                *mb.latest.lock().unwrap() = Some(SendFrame(f));
                mb.arrived.notify_one();
            }
            Ok(())
        }))?;
        let lo = lost.clone();
        let mb = mailbox.clone();
        let closed_token = item.Closed(&TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
            lo.store(true, Relaxed);
            mb.arrived.notify_one();
            Ok(())
        }))?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(cursor)?;
        let _ = session.SetIsBorderRequired(false); // Windows 11 seulement
        let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: min_interval_100ns });
        session.StartCapture()?;
        Ok(Self { width: size.Width as u32, height: size.Height as u32, pool, session, token, mailbox, lost, item, closed_token })
    }

    /// Attend la prochaine image, au plus `timeout`. Un écran statique ne livre rien.
    pub fn next_frame(&self, timeout: Duration) -> Option<Frame> {
        let guard = self.mailbox.latest.lock().unwrap();
        let (mut guard, _) = self.mailbox.arrived.wait_timeout_while(guard, timeout, |f| f.is_none() && !self.lost.load(Relaxed)).unwrap();
        guard.take().map(|f| f.0)
    }
}

fn to_frame(frame: Direct3D11CaptureFrame) -> Option<Frame> {
    let time_100ns = frame.SystemRelativeTime().ok()?.Duration;
    let access: IDirect3DDxgiInterfaceAccess = frame.Surface().ok()?.cast().ok()?;
    let texture = unsafe { access.GetInterface::<ID3D11Texture2D>().ok()? };
    Some(Frame { texture, time_100ns, _frame: frame })
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.pool.RemoveFrameArrived(self.token); // casse la référence pool -> handler -> pool
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
