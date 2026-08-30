//! Where a page becomes a texture, and where the theme is applied.
//!
//! This is the thing the whole experiment is about. Under Tauri a rendered
//! page is a CPU buffer that has to reach a canvas in another process, and the
//! recolouring is a chain of canvas composite operations or, when a colour has
//! to be kept, a walk over every pixel at 34-70ms a page. Here the page goes
//! to the GPU once and the recolouring is a compute pass over it: the same
//! ramp, the same tolerance, and no readback.
//!
//! Two costs disappear on the way, and they are worth naming because they were
//! measured in Phase 0 and are not measured again:
//!
//! *The swizzle.* pdfium writes BGRA; a texture registered with Vello is read
//! as RGBA. The spike swapped two channels on the CPU, which is 1.6ms a page
//! at 3.3 megapixels and 5.1ms at 10.1. Uploading as `Bgra8Unorm` and letting
//! the shader read it is free, and it is why the pass runs even for a theme
//! that recolours nothing — the shader's passthrough branch is the swizzle.
//!
//! *Theme invalidation.* `keyFor()` in `viewer.ts` carries the theme, so
//! changing theme repaints every page from the renderer. Here a page's source
//! is kept on the GPU and a theme change re-runs the compute pass over it.

use std::rc::Rc;

use anyrender::{RenderContext, ResourceId};
use dioxus_native::DeviceHandle;

use crate::recolor::SHADER;
use crate::render::Bitmap;
use crate::theme::Theme;

/// A page on the GPU: one texture, wearing the theme.
///
/// The obvious design keeps two — what pdfium drew and what the theme made of
/// it — so that changing theme is a compute pass rather than a re-render. It
/// was built that way and measured that way, and the source copy costs 25MB a
/// page at the size a page is actually drawn: 100MB of texture for the three
/// pages a window holds, against 50MB without it. Re-rendering a page instead
/// costs 7ms, and a theme is changed by hand a few times a day. So the source
/// is uploaded, read once by the compute pass, and dropped inside `upload`.
///
/// That is the same answer `keyFor()` gives in the app — a theme change
/// repaints every mounted page — reached for the opposite reason: there, the
/// theme is in the key because a canvas has nowhere to keep the original;
/// here, it is because the original is the more expensive half.
pub struct PageTexture {
    painted: wgpu::Texture,
    id: ResourceId,
    /// The theme the painted copy was made with, or `None` while it is stale.
    themed: Option<Theme>,
    pub width: u32,
    pub height: u32,
}

impl PageTexture {
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Bytes resident on the GPU for this page.
    pub fn bytes(&self) -> u64 {
        self.width as u64 * self.height as u64 * 4
    }

    pub fn is(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }

    pub fn wears(&self, theme: &Theme) -> bool {
        self.themed.as_ref() == Some(theme)
    }
}

/// The compute pipeline, built once for a device and shared by every page.
pub struct Recolorer {
    device: DeviceHandle,
    pipeline: wgpu::ComputePipeline,
}

thread_local! {
    /// One pipeline per process, because there is one device per process. A
    /// widget asks for it when it first has a device to build it against.
    static SHARED: std::cell::RefCell<Option<Rc<Recolorer>>> = const { std::cell::RefCell::new(None) };
}

impl Recolorer {
    pub fn shared(device: &DeviceHandle) -> Rc<Recolorer> {
        SHARED.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(existing) = held.as_ref() {
                return Rc::clone(existing);
            }
            let made = Rc::new(Recolorer::new(device.clone()));
            *held = Some(Rc::clone(&made));
            made
        })
    }

    /// A renderer going away takes every pipeline built against its device
    /// with it. `destroy_surfaces` is where a widget hears about that, and
    /// this is the shared half of what it has to forget.
    pub fn forget() {
        SHARED.with(|held| *held.borrow_mut() = None);
    }

    fn new(device: DeviceHandle) -> Self {
        let module = device
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("recolor"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
        let pipeline =
            device
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("recolor"),
                    layout: None,
                    module: &module,
                    entry_point: Some("recolor"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        Recolorer { device, pipeline }
    }

    /// Put a freshly drawn page on the GPU and paint the theme onto it.
    pub fn upload(
        &self,
        ctx: &mut dyn RenderContext,
        bitmap: &Bitmap,
        theme: &Theme,
    ) -> Option<PageTexture> {
        let extent = wgpu::Extent3d {
            width: bitmap.width,
            height: bitmap.height,
            depth_or_array_layers: 1,
        };
        // pdfium's own byte order, uploaded as it stands. The shader reads it
        // in RGBA order because the format says so, so there is no swizzle.
        let source = self.device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("page: as drawn"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.device.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap.bgra,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bitmap.width * 4),
                rows_per_image: Some(bitmap.height),
            },
            extent,
        );

        let painted = self.device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("page: themed"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let id = ctx
            .try_register_custom_resource(Box::new(painted.clone()))
            .ok()?;
        let mut page = PageTexture {
            painted,
            id,
            themed: None,
            width: bitmap.width,
            height: bitmap.height,
        };
        self.paint(&mut page, &source, theme);
        // The source has been read; the GPU keeps only what is shown.
        drop(source);
        Some(page)
    }

    /// Run the ramp from the page as drawn onto the page as shown.
    fn paint(&self, page: &mut PageTexture, source: &wgpu::Texture, theme: &Theme) {
        // Two vec4s and nothing else, so that there is one possible layout
        // rather than a std140 rule to be right about: `w` on the ink carries
        // whether a pixel keeps a colour of its own, `w` on the paper carries
        // whether the pixel is left alone entirely.
        let mut uniform = [0f32; 8];
        for channel in 0..3 {
            uniform[channel] = theme.text[channel] as f32 / 255.0;
            uniform[4 + channel] = theme.background[channel] as f32 / 255.0;
        }
        uniform[3] = if theme.keep_colour { 1.0 } else { 0.0 };
        uniform[7] = if theme.recolor { 0.0 } else { 1.0 };

        let buffer = self.device.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("theme"),
            size: std::mem::size_of_val(&uniform) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.device
            .queue
            .write_buffer(&buffer, 0, as_bytes(&uniform));

        let bind_group = self
            .device
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("recolor"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &source.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &page.painted.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffer.as_entire_binding(),
                    },
                ],
            });

        let mut encoder = self
            .device
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(page.width.div_ceil(8), page.height.div_ceil(8), 1);
        }
        self.device.queue.submit([encoder.finish()]);
        page.themed = Some(*theme);
    }
}

/// Safe: `f32` has no padding and no invalid bit patterns, and the slice is
/// read only.
fn as_bytes(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
    }
}
