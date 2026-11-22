use std::sync::mpsc::{sync_channel, Receiver};
use std::time::Duration;

use windows::core::{factory, Error, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow};

use crate::capture::bgra_vers_rgba;
use crate::capture::flux::{Commande, Releve, Rythme, Suivi, PAS_MAX};
use crate::traits::{Image, RecepteurCapture};

const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const TAMPONS: i32 = 2;

pub(super) fn existe(fenetre: isize) -> bool {
    unsafe { IsWindow(fenetre as *mut _) != 0 }
}

pub(super) fn reduite(fenetre: isize) -> bool {
    unsafe { IsIconic(fenetre as *mut _) != 0 }
}

struct Fenetre {
    d3d: ID3D11Device,
    contexte: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    taille: SizeInt32,
    lecture: Option<(ID3D11Texture2D, (u32, u32))>,
    reveil: Receiver<()>,
}

impl Fenetre {
    fn ouvrir(fenetre: isize, curseur: bool) -> windows::core::Result<Self> {
        if !GraphicsCaptureSession::IsSupported()? {
            return Err(Error::empty());
        }
        let (mut d3d, mut contexte) = (None, None);
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                Some(&mut contexte),
            )?;
        }
        let (d3d, contexte): (ID3D11Device, ID3D11DeviceContext) = (
            d3d.ok_or_else(Error::empty)?,
            contexte.ok_or_else(Error::empty)?,
        );
        let winrt = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&d3d.cast::<IDXGIDevice>()?)? }
            .cast::<IDirect3DDevice>()?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let element =
            unsafe { interop.CreateForWindow::<GraphicsCaptureItem>(HWND(fenetre as *mut _)) }?;
        let taille = element.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt, FORMAT, TAMPONS, taille)?;
        let (signal, reveil) = sync_channel(1);
        pool.FrameArrived(&TypedEventHandler::new(
            move |_: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
                let _ = signal.try_send(());
                Ok(())
            },
        ))?;
        let session = pool.CreateCaptureSession(&element)?;
        let _ = session.SetIsCursorCaptureEnabled(curseur);
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        Ok(Self {
            d3d,
            contexte,
            winrt,
            pool,
            session,
            taille,
            lecture: None,
            reveil,
        })
    }

    fn derniere(&self) -> Option<Direct3D11CaptureFrame> {
        let mut cadre = None;
        while let Ok(suivant) = self.pool.TryGetNextFrame() {
            cadre = Some(suivant);
        }
        cadre
    }

    fn lire(&mut self, cadre: &Direct3D11CaptureFrame) -> windows::core::Result<Option<Image>> {
        let contenu = cadre.ContentSize()?;
        let texture: ID3D11Texture2D = unsafe {
            cadre
                .Surface()?
                .cast::<IDirect3DDxgiInterfaceAccess>()?
                .GetInterface()?
        };
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut description) };
        let dimensions = (description.Width, description.Height);
        if self.lecture.as_ref().map(|(_, d)| *d) != Some(dimensions) {
            let staging = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
                ..description
            };
            let mut cree = None;
            unsafe { self.d3d.CreateTexture2D(&staging, None, Some(&mut cree))? };
            self.lecture = Some((cree.ok_or_else(Error::empty)?, dimensions));
        }
        let Some((lecture, _)) = self.lecture.as_ref() else {
            return Ok(None);
        };
        let largeur = (contenu.Width.max(0) as u32).min(description.Width) as usize;
        let hauteur = (contenu.Height.max(0) as u32).min(description.Height) as usize;
        if largeur < 2 || hauteur < 2 {
            return Ok(None);
        }
        let mut plan = D3D11_MAPPED_SUBRESOURCE::default();
        let pixels = unsafe {
            self.contexte.CopyResource(lecture, &texture);
            self.contexte
                .Map(lecture, 0, D3D11_MAP_READ, 0, Some(&mut plan))?;
            let pas = plan.RowPitch as usize;
            let pixels = if plan.pData.is_null() || pas < largeur * 4 {
                None
            } else {
                Some(bgra_vers_rgba(
                    std::slice::from_raw_parts(
                        plan.pData as *const u8,
                        pas * (hauteur - 1) + largeur * 4,
                    ),
                    pas,
                    largeur,
                    hauteur,
                ))
            };
            self.contexte.Unmap(lecture, 0);
            pixels
        };
        Ok(pixels.map(|rgba| Image {
            largeur: largeur as u32,
            hauteur: hauteur as u32,
            rgba,
        }))
    }

    fn suivre_taille(&mut self, cadre: &Direct3D11CaptureFrame) -> windows::core::Result<()> {
        let contenu = cadre.ContentSize()?;
        if contenu.Width > 0
            && contenu.Height > 0
            && (contenu.Width, contenu.Height) != (self.taille.Width, self.taille.Height)
        {
            self.pool.Recreate(&self.winrt, FORMAT, TAMPONS, contenu)?;
            self.taille = contenu;
        }
        Ok(())
    }
}

impl Drop for Fenetre {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

pub(super) fn capturer(
    fenetre: isize,
    commande: &Commande,
    recepteur: &mut RecepteurCapture,
) -> Result<(), String> {
    let mut curseur = commande.curseur();
    let mut source = Fenetre::ouvrir(fenetre, curseur).map_err(|e| e.to_string())?;
    let mut rythme = Rythme::nouveau(commande);
    let mut suivi = Suivi::default();
    let mut en_attente: Option<Direct3D11CaptureFrame> = None;
    while !commande.arretee() {
        if commande.curseur() != curseur {
            curseur = !curseur;
            let _ = source.session.SetIsCursorCaptureEnabled(curseur);
        }
        if !existe(fenetre) {
            suivi.transmettre(Releve::Fermee, recepteur);
            return Ok(());
        }
        if reduite(fenetre) {
            en_attente = None;
            match suivi.transmettre(Releve::Reduite, recepteur) {
                Some(pause) => std::thread::sleep(pause),
                None => return Ok(()),
            }
            continue;
        }
        let delai = if en_attente.is_some() {
            rythme.attente(commande)
        } else {
            Duration::from_millis(100)
        };
        let _ = source.reveil.recv_timeout(delai.min(PAS_MAX * 2));
        if let Some(cadre) = source.derniere() {
            en_attente = Some(cadre);
        }
        if en_attente.is_none() || !rythme.due(commande) {
            continue;
        }
        let Some(cadre) = en_attente.take() else {
            continue;
        };
        let image = source.lire(&cadre).map_err(|e| e.to_string())?;
        let taille = source.suivre_taille(&cadre);
        drop(cadre);
        taille.map_err(|e| e.to_string())?;
        if let Some(image) = image {
            if suivi.transmettre(Releve::Image(image), recepteur).is_none() {
                break;
            }
        }
    }
    Ok(())
}
