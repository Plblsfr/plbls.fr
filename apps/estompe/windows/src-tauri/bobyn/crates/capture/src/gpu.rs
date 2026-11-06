//! Device D3D11 sur le GPU dédié (le plus de mémoire vidéo, hors adaptateur logiciel).
use windows::core::{Error, Interface, Result};
use windows::Win32::Foundation::{E_FAIL, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;

pub struct Gpu {
    pub name: String,
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
}

pub fn open() -> Result<Gpu> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut best: Option<(usize, IDXGIAdapter1, String)> = None;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else { break };
            let desc = adapter.GetDesc1()?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            if best.as_ref().map_or(true, |b| desc.DedicatedVideoMemory > b.0) {
                let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
                best = Some((desc.DedicatedVideoMemory, adapter, name));
            }
        }
        let (_, adapter, name) = best.ok_or_else(|| Error::new(E_FAIL, "aucun GPU matériel"))?;
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let device = device.unwrap();
        // L'encodeur matériel utilise ce device depuis ses propres threads.
        let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
        Ok(Gpu { name, device, context: context.unwrap() })
    }
}
