//! BGRA -> NV12 et mise à l'échelle sur le moteur vidéo du GPU (pas le moteur 3D).
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use windows::core::{Interface, Result};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

const OUTPUTS: usize = 3;

pub struct Converter {
    device: ID3D11VideoDevice,
    context: ID3D11VideoContext1,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    // Rotation de textures : l'encodeur garde la sienne le temps de la lire.
    targets: Vec<(ID3D11Texture2D, ID3D11VideoProcessorOutputView)>,
    turn: usize,
    // WGC recycle un petit nombre de textures : une vue d'entrée par texture suffit.
    inputs: HashMap<usize, ID3D11VideoProcessorInputView>,
}

impl Converter {
    pub fn new(device: &ID3D11Device, context: &ID3D11DeviceContext, in_size: (u32, u32), out_size: (u32, u32)) -> Result<Self> {
        unsafe {
            let video_device: ID3D11VideoDevice = device.cast()?;
            let video_context: ID3D11VideoContext1 = context.cast()?;
            let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputWidth: in_size.0,
                InputHeight: in_size.1,
                OutputWidth: out_size.0,
                OutputHeight: out_size.1,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
                ..Default::default()
            };
            let enumerator = video_device.CreateVideoProcessorEnumerator(&content)?;
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
            video_context.VideoProcessorSetStreamColorSpace1(&processor, 0, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
            video_context.VideoProcessorSetOutputColorSpace1(&processor, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709);

            let desc = D3D11_TEXTURE2D_DESC {
                Width: out_size.0,
                Height: out_size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC { ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D, ..Default::default() };
            let mut targets = Vec::with_capacity(OUTPUTS);
            for _ in 0..OUTPUTS {
                let mut tex = None;
                device.CreateTexture2D(&desc, None, Some(&mut tex))?;
                let tex = tex.unwrap();
                let mut view = None;
                video_device.CreateVideoProcessorOutputView(&tex, &enumerator, &view_desc, Some(&mut view))?;
                targets.push((tex, view.unwrap()));
            }
            Ok(Self { device: video_device, context: video_context, enumerator, processor, targets, turn: 0, inputs: HashMap::new() })
        }
    }

    pub fn convert(&mut self, bgra: &ID3D11Texture2D) -> Result<ID3D11Texture2D> {
        unsafe {
            let key = bgra.as_raw() as usize;
            if !self.inputs.contains_key(&key) {
                let mut desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC { ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D, ..Default::default() };
                desc.Anonymous.Texture2D = D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 };
                let mut view = None;
                self.device.CreateVideoProcessorInputView(bgra, &self.enumerator, &desc, Some(&mut view))?;
                if self.inputs.len() >= 32 {
                    self.inputs.clear();
                }
                self.inputs.insert(key, view.unwrap());
            }
            let (target, out_view) = &self.targets[self.turn];
            self.turn = (self.turn + 1) % OUTPUTS;
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(self.inputs[&key].clone())),
                ..Default::default()
            };
            let result = self.context.VideoProcessorBlt(&self.processor, out_view, 0, std::slice::from_ref(&stream));
            drop(ManuallyDrop::into_inner(stream.pInputSurface));
            result?;
            Ok(target.clone())
        }
    }
}
