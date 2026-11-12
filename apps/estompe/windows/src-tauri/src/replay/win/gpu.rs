use windows::core::{s, Interface, PCSTR};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{ID3DBlob, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_DRIVER_TYPE_UNKNOWN};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader,
    ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_OPTIMAL_SPEED, D3D11_VIEWPORT, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2DARRAY,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter1, IDXGIDevice};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFAttributes, IMFDXGIBuffer, IMFDXGIDeviceManager, IMFMediaType, IMFSample, IMFVideoSampleAllocatorEx, MFCreateAttributes, MFCreateDXGIDeviceManager,
    MFCreateDXGISurfaceBuffer, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateVideoSampleAllocatorEx, MFMediaType_Video, MFNominalRange_16_235, MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709, MFVideoTransFunc_709,
    MFVideoTransferMatrix_BT709, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION,
    MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX, MF_SA_D3D11_BINDFLAGS, MF_SA_D3D11_USAGE,
};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

use super::materiel::texte;
use crate::replay::cadence::Entree;

pub struct Appareil {
    pub d3d: ID3D11Device,
    pub contexte: ID3D11DeviceContext,
    pub winrt: IDirect3DDevice,
    pub gestionnaire: IMFDXGIDeviceManager,
}

impl Appareil {
    pub fn creer(adaptateur: &IDXGIAdapter1) -> Result<Self, String> {
        Self::creer_brut(adaptateur).map_err(|e| format!("appareil Direct3D indisponible : {}", texte(e)))
    }

    fn creer_brut(adaptateur: &IDXGIAdapter1) -> windows::core::Result<Self> {
        unsafe {
            let (mut d3d, mut contexte) = (None, None);
            let options = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
            D3D11CreateDevice(adaptateur, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), options, None, D3D11_SDK_VERSION, Some(&mut d3d), None, Some(&mut contexte))?;
            let (d3d, contexte): (ID3D11Device, ID3D11DeviceContext) = (d3d.ok_or_else(windows::core::Error::empty)?, contexte.ok_or_else(windows::core::Error::empty)?);
            let _ = d3d.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
            let winrt = CreateDirect3D11DeviceFromDXGIDevice(&d3d.cast::<IDXGIDevice>()?)?.cast::<IDirect3DDevice>()?;
            let (mut jeton, mut gestionnaire) = (0u32, None);
            MFCreateDXGIDeviceManager(&mut jeton, &mut gestionnaire)?;
            let gestionnaire = gestionnaire.ok_or_else(windows::core::Error::empty)?;
            gestionnaire.ResetDevice(&d3d, jeton)?;
            Ok(Self { d3d, contexte, winrt, gestionnaire })
        }
    }
}

pub fn type_nv12(largeur: u32, hauteur: u32, ips: u32) -> windows::core::Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        couleurs(&t, largeur, hauteur, ips)?;
        Ok(t)
    }
}

pub fn couleurs(t: &IMFMediaType, largeur: u32, hauteur: u32, ips: u32) -> windows::core::Result<()> {
    unsafe {
        t.SetUINT64(&MF_MT_FRAME_SIZE, (largeur as u64) << 32 | hauteur as u64)?;
        t.SetUINT64(&MF_MT_FRAME_RATE, (ips as u64) << 32 | 1)?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, 1 << 32 | 1)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
        t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
        t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
        t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
    }
}

const NUANCEUR: &str = "
Texture2D image : register(t0);
SamplerState lisse : register(s0);
struct Sommet { float4 position : SV_Position; float2 uv : TEXCOORD0; };
Sommet sommet(uint id : SV_VertexID) {
    Sommet s;
    s.uv = float2((id << 1) & 2, id & 2);
    s.position = float4(s.uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return s;
}
float4 pixel(Sommet s) : SV_Target {
    float3 c = max(image.Sample(lisse, s.uv).rgb, 0) * (80.0 / BLANC);
    float l = max(c.r, max(c.g, c.b));
    float m = l > 0.75 ? 0.75 + 0.25 * (1 - exp((0.75 - l) / 0.25)) : l;
    c = saturate(c * (l > 0 ? m / l : 1));
    float3 bas = c * 12.92;
    float3 haut = 1.055 * pow(c, 1 / 2.4) - 0.055;
    return float4(lerp(bas, haut, step(0.0031308, c)), 1);
}";

struct Tons {
    sommets: ID3D11VertexShader,
    pixels: ID3D11PixelShader,
    lisse: ID3D11SamplerState,
    cible: ID3D11Texture2D,
    vue: ID3D11RenderTargetView,
}

unsafe fn compiler(source: &str, entree: PCSTR, profil: PCSTR) -> windows::core::Result<ID3DBlob> {
    let mut code = None;
    D3DCompile(source.as_ptr() as *const _, source.len(), None, None, None, entree, profil, 0, 0, &mut code, None)?;
    code.ok_or_else(windows::core::Error::empty)
}

unsafe fn octets(blob: &ID3DBlob) -> &[u8] {
    std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize())
}

impl Tons {
    unsafe fn creer(a: &Appareil, largeur: u32, hauteur: u32, blanc: f32) -> windows::core::Result<Self> {
        let source = NUANCEUR.replace("BLANC", &format!("{blanc:.1}"));
        let (vs, ps) = (compiler(&source, s!("sommet"), s!("vs_4_0"))?, compiler(&source, s!("pixel"), s!("ps_4_0"))?);
        let (mut sommets, mut pixels, mut lisse, mut cible, mut vue) = (None, None, None, None, None);
        a.d3d.CreateVertexShader(octets(&vs), None, Some(&mut sommets))?;
        a.d3d.CreatePixelShader(octets(&ps), None, Some(&mut pixels))?;
        let echantillonneur = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ..Default::default()
        };
        a.d3d.CreateSamplerState(&echantillonneur, Some(&mut lisse))?;
        let texture = D3D11_TEXTURE2D_DESC {
            Width: largeur,
            Height: hauteur,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            ..Default::default()
        };
        a.d3d.CreateTexture2D(&texture, None, Some(&mut cible))?;
        let cible: ID3D11Texture2D = cible.ok_or_else(windows::core::Error::empty)?;
        a.d3d.CreateRenderTargetView(&cible, None, Some(&mut vue))?;
        let vide = windows::core::Error::empty;
        Ok(Self { sommets: sommets.ok_or_else(vide)?, pixels: pixels.ok_or_else(vide)?, lisse: lisse.ok_or_else(vide)?, cible, vue: vue.ok_or_else(vide)? })
    }

    unsafe fn appliquer(&self, a: &Appareil, source: &ID3D11Texture2D, largeur: u32, hauteur: u32) -> windows::core::Result<()> {
        let mut ressource = None;
        a.d3d.CreateShaderResourceView(source, None, Some(&mut ressource))?;
        let c = &a.contexte;
        c.OMSetRenderTargets(Some(&[Some(self.vue.clone())]), None);
        c.RSSetViewports(Some(&[D3D11_VIEWPORT { Width: largeur as f32, Height: hauteur as f32, MaxDepth: 1.0, ..Default::default() }]));
        c.IASetInputLayout(None);
        c.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        c.VSSetShader(&self.sommets, None);
        c.PSSetShader(&self.pixels, None);
        c.PSSetShaderResources(0, Some(&[ressource]));
        c.PSSetSamplers(0, Some(&[Some(self.lisse.clone())]));
        c.Draw(3, 0);
        c.PSSetShaderResources(0, Some(&[None]));
        c.OMSetRenderTargets(None, None);
        Ok(())
    }
}

const ANNEAU: usize = 8;

enum Cibles {
    Reserve(IMFVideoSampleAllocatorEx),
    Textures { anneau: Vec<ID3D11Texture2D>, suivante: std::cell::Cell<usize> },
    Memoire { cible: ID3D11Texture2D, lecture: ID3D11Texture2D, taille: (u32, u32) },
}

pub struct Convertisseur {
    video: ID3D11VideoDevice,
    contexte: ID3D11VideoContext,
    enumerateur: ID3D11VideoProcessorEnumerator,
    processeur: ID3D11VideoProcessor,
    cibles: Cibles,
    tons: Option<Tons>,
    source: (u32, u32),
}

unsafe fn texture_nv12(a: &Appareil, taille: (u32, u32), lecture: bool) -> windows::core::Result<ID3D11Texture2D> {
    let description = D3D11_TEXTURE2D_DESC {
        Width: taille.0,
        Height: taille.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: if lecture { D3D11_USAGE_STAGING } else { D3D11_USAGE_DEFAULT },
        BindFlags: if lecture { 0 } else { D3D11_BIND_RENDER_TARGET.0 as u32 },
        CPUAccessFlags: if lecture { D3D11_CPU_ACCESS_READ.0 as u32 } else { 0 },
        ..Default::default()
    };
    let mut texture = None;
    a.d3d.CreateTexture2D(&description, None, Some(&mut texture))?;
    texture.ok_or_else(windows::core::Error::empty)
}

impl Convertisseur {
    pub fn creer(a: &Appareil, source: (u32, u32), sortie: (u32, u32), ips: u32, blanc_hdr: Option<f32>, entree: Entree) -> Result<Self, String> {
        unsafe { Self::creer_brut(a, source, sortie, ips, blanc_hdr, entree) }.map_err(|e| format!("conversion vidéo sur la carte graphique impossible : {}", texte(e)))
    }

    unsafe fn creer_brut(a: &Appareil, source: (u32, u32), sortie: (u32, u32), ips: u32, blanc_hdr: Option<f32>, entree: Entree) -> windows::core::Result<Self> {
        let (video, contexte) = (a.d3d.cast::<ID3D11VideoDevice>()?, a.contexte.cast::<ID3D11VideoContext>()?);
        let cadence = DXGI_RATIONAL { Numerator: ips, Denominator: 1 };
        let contenu = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: cadence,
            InputWidth: source.0,
            InputHeight: source.1,
            OutputFrameRate: cadence,
            OutputWidth: sortie.0,
            OutputHeight: sortie.1,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        let enumerateur = video.CreateVideoProcessorEnumerator(&contenu)?;
        let processeur = video.CreateVideoProcessor(&enumerateur, 0)?;
        contexte.VideoProcessorSetStreamFrameFormat(&processeur, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
        contexte.VideoProcessorSetStreamColorSpace(&processeur, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
        contexte.VideoProcessorSetOutputColorSpace(&processeur, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 1 << 2 | 1 << 4 });
        contexte.VideoProcessorSetStreamAutoProcessingMode(&processeur, 0, false);

        let cibles = match entree {
            Entree::Reserve => {
                let mut allocateur = std::ptr::null_mut();
                MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut allocateur)?;
                let allocateur = IMFVideoSampleAllocatorEx::from_raw(allocateur);
                allocateur.SetDirectXManager(&a.gestionnaire)?;
                let mut attributs: Option<IMFAttributes> = None;
                MFCreateAttributes(&mut attributs, 2)?;
                let attributs = attributs.ok_or_else(windows::core::Error::empty)?;
                attributs.SetUINT32(&MF_SA_D3D11_BINDFLAGS, D3D11_BIND_RENDER_TARGET.0 as u32)?;
                attributs.SetUINT32(&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)?;
                allocateur.InitializeSampleAllocatorEx(2, 12, &attributs, &type_nv12(sortie.0, sortie.1, ips)?)?;
                Cibles::Reserve(allocateur)
            }
            Entree::Textures => {
                let mut anneau = Vec::with_capacity(ANNEAU);
                for _ in 0..ANNEAU {
                    anneau.push(texture_nv12(a, sortie, false)?);
                }
                Cibles::Textures { anneau, suivante: std::cell::Cell::new(0) }
            }
            Entree::Memoire => Cibles::Memoire { cible: texture_nv12(a, sortie, false)?, lecture: texture_nv12(a, sortie, true)?, taille: sortie },
        };
        let tons = match blanc_hdr {
            Some(blanc) => Some(Tons::creer(a, source.0, source.1, blanc)?),
            None => None,
        };
        Ok(Self { video, contexte, enumerateur, processeur, cibles, tons, source })
    }

    pub fn envoyee(&self) {
        if let Cibles::Textures { anneau, suivante } = &self.cibles {
            suivante.set((suivante.get() + 1) % anneau.len());
        }
    }

    pub fn convertir(&self, a: &Appareil, image: &ID3D11Texture2D) -> windows::core::Result<IMFSample> {
        unsafe {
            match &self.cibles {
                Cibles::Reserve(allocateur) => {
                    let echantillon = allocateur.AllocateSample()?;
                    let tampon = echantillon.GetBufferByIndex(0)?.cast::<IMFDXGIBuffer>()?;
                    let mut cible = std::ptr::null_mut();
                    tampon.GetResource(&ID3D11Texture2D::IID, &mut cible)?;
                    let cible = ID3D11Texture2D::from_raw(cible);
                    self.dessiner(a, image, &cible, tampon.GetSubresourceIndex()?)?;
                    Ok(echantillon)
                }
                Cibles::Textures { anneau, suivante } => {
                    let cible = &anneau[suivante.get()];
                    self.dessiner(a, image, cible, 0)?;
                    let tampon = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, cible, 0, false)?;
                    if let Ok(plan) = tampon.cast::<IMF2DBuffer>() {
                        tampon.SetCurrentLength(plan.GetContiguousLength()?)?;
                    }
                    let echantillon = MFCreateSample()?;
                    echantillon.AddBuffer(&tampon)?;
                    Ok(echantillon)
                }
                Cibles::Memoire { cible, lecture, taille } => {
                    self.dessiner(a, image, cible, 0)?;
                    a.contexte.CopyResource(lecture, cible);
                    let (largeur, hauteur) = (taille.0 as usize, taille.1 as usize);
                    let longueur = largeur * hauteur * 3 / 2;
                    let tampon = MFCreateMemoryBuffer(longueur as u32)?;
                    let mut plan = D3D11_MAPPED_SUBRESOURCE::default();
                    a.contexte.Map(lecture, 0, D3D11_MAP_READ, 0, Some(&mut plan))?;
                    let mut destination = std::ptr::null_mut();
                    let copie = tampon.Lock(&mut destination, None, None).map(|_| {
                        let (origine, pas) = (plan.pData as *const u8, plan.RowPitch as usize);
                        for ligne in 0..hauteur * 3 / 2 {
                            std::ptr::copy_nonoverlapping(origine.add(ligne * pas), destination.add(ligne * largeur), largeur);
                        }
                    });
                    a.contexte.Unmap(lecture, 0);
                    copie?;
                    tampon.Unlock()?;
                    tampon.SetCurrentLength(longueur as u32)?;
                    let echantillon = MFCreateSample()?;
                    echantillon.AddBuffer(&tampon)?;
                    Ok(echantillon)
                }
            }
        }
    }

    unsafe fn dessiner(&self, a: &Appareil, image: &ID3D11Texture2D, cible: &ID3D11Texture2D, tranche: u32) -> windows::core::Result<()> {
        let mut vue_sortie = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC::default();
        if tranche == 0 {
            vue_sortie.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
        } else {
            vue_sortie.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2DARRAY;
            vue_sortie.Anonymous.Texture2DArray.FirstArraySlice = tranche;
            vue_sortie.Anonymous.Texture2DArray.ArraySize = 1;
        }
        let mut sortie = None;
        self.video.CreateVideoProcessorOutputView(cible, &self.enumerateur, &vue_sortie, Some(&mut sortie))?;
        let entree_texture = match &self.tons {
            Some(tons) => {
                tons.appliquer(a, image, self.source.0, self.source.1)?;
                &tons.cible
            }
            None => image,
        };
        let vue_entree = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC { FourCC: 0, ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D, ..Default::default() };
        let mut entree = None;
        self.video.CreateVideoProcessorInputView(entree_texture, &self.enumerateur, &vue_entree, Some(&mut entree))?;
        let flux = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: std::mem::ManuallyDrop::new(entree), ..Default::default() };
        let resultat = self.contexte.VideoProcessorBlt(&self.processeur, sortie.as_ref(), 0, std::slice::from_ref(&flux));
        drop(std::mem::ManuallyDrop::into_inner(flux.pInputSurface));
        resultat
    }
}
