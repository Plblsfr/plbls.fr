use std::time::{Duration, Instant};

#[cfg_attr(not(all(windows, feature = "capture-fenetre")), allow(dead_code))]
pub struct Cadence {
    intervalle: Duration,
    prochaine: Instant,
}

#[cfg_attr(not(all(windows, feature = "capture-fenetre")), allow(dead_code))]
impl Cadence {
    pub fn nouvelle(ips: u32, depart: Instant) -> Self {
        Self { intervalle: Duration::from_micros(1_000_000 / ips.clamp(1, 240) as u64), prochaine: depart }
    }

    pub fn attente(&self, t: Instant) -> Duration {
        self.prochaine.checked_sub(self.intervalle / 4).map_or(Duration::ZERO, |seuil| seuil.saturating_duration_since(t))
    }

    pub fn due(&mut self, t: Instant) -> bool {
        if !self.attente(t).is_zero() {
            return false;
        }
        self.prochaine = (self.prochaine + self.intervalle).max(t.checked_sub(self.intervalle).unwrap_or(t));
        true
    }
}

#[cfg_attr(not(all(windows, feature = "capture-fenetre")), allow(dead_code))]
pub fn bgra_vers_rgba(source: &[u8], pas: usize, largeur: usize, hauteur: usize) -> Vec<u8> {
    let mut sortie = vec![0u8; largeur * hauteur * 4];
    if pas < largeur * 4 || largeur == 0 {
        return sortie;
    }
    for (ligne, cible) in source.chunks(pas).zip(sortie.chunks_exact_mut(largeur * 4)) {
        if ligne.len() < largeur * 4 {
            break;
        }
        for (s, c) in ligne[..largeur * 4].chunks_exact(4).zip(cible.chunks_exact_mut(4)) {
            c[0] = s[2];
            c[1] = s[1];
            c[2] = s[0];
            c[3] = 255;
        }
    }
    sortie
}

#[cfg(all(windows, feature = "capture-fenetre"))]
mod imp {
    use super::{bgra_vers_rgba, Cadence};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{sync_channel, Receiver};
    use std::time::{Duration, Instant};

    use image::RgbaImage;
    use windows::core::{factory, Error, Interface};
    use windows::Foundation::TypedEventHandler;
    use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Foundation::{HMODULE, HWND};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
        D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
    use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

    const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
    const TAMPONS: i32 = 2;

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
        fn ouvrir(fenetre: isize) -> windows::core::Result<Self> {
            if !GraphicsCaptureSession::IsSupported()? {
                return Err(Error::empty());
            }
            let (mut d3d, mut contexte) = (None, None);
            unsafe {
                D3D11CreateDevice(None::<&IDXGIAdapter>, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, Some(&mut contexte))?;
            }
            let (d3d, contexte): (ID3D11Device, ID3D11DeviceContext) = (d3d.ok_or_else(Error::empty)?, contexte.ok_or_else(Error::empty)?);
            let winrt = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&d3d.cast::<IDXGIDevice>()?)? }.cast::<IDirect3DDevice>()?;
            let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
            let element = unsafe { interop.CreateForWindow::<GraphicsCaptureItem>(HWND(fenetre as *mut _)) }?;
            let taille = element.Size()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt, FORMAT, TAMPONS, taille)?;
            let (signal, reveil) = sync_channel(1);
            pool.FrameArrived(&TypedEventHandler::new(move |_: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
                let _ = signal.try_send(());
                Ok(())
            }))?;
            let session = pool.CreateCaptureSession(&element)?;
            let _ = session.SetIsCursorCaptureEnabled(true);
            let _ = session.SetIsBorderRequired(false);
            session.StartCapture()?;
            Ok(Self { d3d, contexte, winrt, pool, session, taille, lecture: None, reveil })
        }

        fn derniere(&self) -> Option<Direct3D11CaptureFrame> {
            let mut cadre = None;
            while let Ok(suivant) = self.pool.TryGetNextFrame() {
                cadre = Some(suivant);
            }
            cadre
        }

        fn lire(&mut self, cadre: &Direct3D11CaptureFrame) -> windows::core::Result<Option<RgbaImage>> {
            let contenu = cadre.ContentSize()?;
            let texture: ID3D11Texture2D = unsafe { cadre.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()? };
            let mut description = D3D11_TEXTURE2D_DESC::default();
            unsafe { texture.GetDesc(&mut description) };
            let dimensions = (description.Width, description.Height);
            if self.lecture.as_ref().map(|(_, d)| *d) != Some(dimensions) {
                let staging = D3D11_TEXTURE2D_DESC { Usage: D3D11_USAGE_STAGING, BindFlags: 0, CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, MiscFlags: 0, ..description };
                let mut cree = None;
                unsafe { self.d3d.CreateTexture2D(&staging, None, Some(&mut cree))? };
                self.lecture = Some((cree.ok_or_else(Error::empty)?, dimensions));
            }
            let Some((lecture, _)) = self.lecture.as_ref() else { return Ok(None) };
            let largeur = (contenu.Width.max(0) as u32).min(description.Width) as usize;
            let hauteur = (contenu.Height.max(0) as u32).min(description.Height) as usize;
            if largeur < 2 || hauteur < 2 {
                return Ok(None);
            }
            let mut plan = D3D11_MAPPED_SUBRESOURCE::default();
            let pixels = unsafe {
                self.contexte.CopyResource(lecture, &texture);
                self.contexte.Map(lecture, 0, D3D11_MAP_READ, 0, Some(&mut plan))?;
                let pas = plan.RowPitch as usize;
                let pixels = if plan.pData.is_null() || pas < largeur * 4 {
                    None
                } else {
                    Some(bgra_vers_rgba(std::slice::from_raw_parts(plan.pData as *const u8, pas * (hauteur - 1) + largeur * 4), pas, largeur, hauteur))
                };
                self.contexte.Unmap(lecture, 0);
                pixels
            };
            Ok(pixels.and_then(|p| RgbaImage::from_raw(largeur as u32, hauteur as u32, p)))
        }

        fn suivre_taille(&mut self, cadre: &Direct3D11CaptureFrame) -> windows::core::Result<()> {
            let contenu = cadre.ContentSize()?;
            if contenu.Width > 0 && contenu.Height > 0 && (contenu.Width, contenu.Height) != (self.taille.Width, self.taille.Height) {
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

    pub fn capturer(fenetre: isize, ips: u32, arret: &AtomicBool, mut envoyer: impl FnMut(RgbaImage) -> bool) -> Result<(), String> {
        let mut source = Fenetre::ouvrir(fenetre).map_err(|e| e.to_string())?;
        let mut cadence = Cadence::nouvelle(ips, Instant::now());
        let mut en_attente: Option<Direct3D11CaptureFrame> = None;
        while !arret.load(Ordering::Relaxed) {
            let delai = if en_attente.is_some() { cadence.attente(Instant::now()) } else { Duration::from_millis(500) };
            let _ = source.reveil.recv_timeout(delai);
            if let Some(cadre) = source.derniere() {
                en_attente = Some(cadre);
            }
            if en_attente.is_none() || !cadence.due(Instant::now()) {
                continue;
            }
            let Some(cadre) = en_attente.take() else { continue };
            let image = source.lire(&cadre).map_err(|e| e.to_string())?;
            let taille = source.suivre_taille(&cadre);
            drop(cadre);
            taille.map_err(|e| e.to_string())?;
            if let Some(image) = image {
                if !envoyer(image) {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(windows, feature = "capture-fenetre"))]
pub use imp::capturer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_laisse_passer_une_image_par_intervalle() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(30, depart);
        let pas = Duration::from_micros(1_000_000 / 60);
        let passees = (0..120u32).filter(|i| cadence.due(depart + pas * *i)).count();
        assert_eq!(passees, 60);
    }

    #[test]
    fn cadence_tolere_une_image_legerement_en_avance() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(60, depart);
        assert!(cadence.due(depart));
        assert!(cadence.due(depart + Duration::from_millis(15)));
        assert!(!cadence.due(depart + Duration::from_millis(20)));
    }

    #[test]
    fn cadence_borne_le_rattrapage_apres_un_silence() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(30, depart);
        assert!(cadence.due(depart));
        let reprise = depart + Duration::from_secs(5);
        let passees = (0..20u64).filter(|i| cadence.due(reprise + Duration::from_millis(*i))).count();
        assert_eq!(passees, 3);
    }

    #[test]
    fn cadence_donne_le_delai_avant_la_prochaine_image() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(40, depart);
        assert!(cadence.due(depart));
        let attente = cadence.attente(depart);
        assert_eq!(attente, Duration::from_micros(18_750));
        assert!(cadence.due(depart + attente));
    }

    #[test]
    fn bgra_devient_rgba_sans_le_remplissage_de_ligne() {
        let source = [1, 2, 3, 0, 4, 5, 6, 0, 99, 99, 7, 8, 9, 0, 10, 11, 12, 0];
        assert_eq!(bgra_vers_rgba(&source, 10, 2, 2), vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]);
    }

    #[test]
    fn bgra_refuse_un_pas_trop_court() {
        assert_eq!(bgra_vers_rgba(&[1, 2, 3, 4], 2, 1, 1), vec![0, 0, 0, 0]);
    }
}
