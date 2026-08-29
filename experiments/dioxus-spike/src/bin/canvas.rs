//! The smallest custom-paint source there is, through `dioxus_native::launch`.
//!
//! A control for the page spike: if a canvas draws here and does not draw
//! under the shell, the fault is the shell's; if it draws in neither, the
//! fault is in how the canvas is asked for.

use anyrender_vello::wgpu;
use dioxus::prelude::*;
use dioxus_native::{use_wgpu, CustomPaintCtx, CustomPaintSource, DeviceHandle, TextureHandle};

fn main() {
    dioxus_native::launch(App);
}

struct Solid {
    device: Option<DeviceHandle>,
    texture: Option<TextureHandle>,
    size: Option<(u32, u32)>,
}

impl CustomPaintSource for Solid {
    fn resume(&mut self, device_handle: &DeviceHandle) {
        eprintln!("canvas: source resumed");
        self.device = Some(device_handle.clone());
    }

    fn suspend(&mut self) {
        self.device = None;
        self.texture = None;
        self.size = None;
    }

    fn render(
        &mut self,
        mut ctx: CustomPaintCtx<'_>,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Option<TextureHandle> {
        eprintln!(
            "canvas: render {width}x{height} @{scale}, holding blob {:?}",
            self.texture.as_ref().map(|t| t.0.data.id())
        );
        let device = self.device.clone()?;
        let want = (
            ((width as f64 * scale) as u32).max(1),
            ((height as f64 * scale) as u32).max(1),
        );
        if self.size == Some(want) {
            return self.texture.clone();
        }
        let texture = device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solid"),
            size: wgpu::Extent3d {
                width: want.0,
                height: want.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let pixels: Vec<u8> = (0..want.0 * want.1)
            .flat_map(|_| [220u8, 90, 60, 255])
            .collect();
        device.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(want.0 * 4),
                rows_per_image: Some(want.1),
            },
            wgpu::Extent3d {
                width: want.0,
                height: want.1,
                depth_or_array_layers: 1,
            },
        );
        // Not unregistered here: vello checks its overrides when the frame is
        // submitted, which is after this returns.
        let handle = ctx.register_texture(texture);
        eprintln!("canvas: registered blob {} for {}x{}", handle.0.data.id(), want.0, want.1);
        self.size = Some(want);
        self.texture = Some(handle.clone());
        Some(handle)
    }
}

#[component]
fn App() -> Element {
    let source = use_wgpu(|| Solid {
        device: None,
        texture: None,
        size: None,
    });
    eprintln!("canvas: paint source {source}");

    rsx! {
        div { style: "padding: 40px; background: #222; height: 100vh;",
            canvas { "src": "{source}", style: "display: block; width: 320px; height: 240px;" }
            canvas { "src": "0", style: "display: block; width: 320px; height: 120px;" }
        }
    }
}
