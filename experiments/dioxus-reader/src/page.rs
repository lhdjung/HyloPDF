//! One page, drawn by the renderer and handed to Vello as a texture.
//!
//! This is what a `<canvas>` and its 2D context became. The lifecycle is the
//! same three moments a `CustomPaintSource` had — the renderer came up, the
//! renderer went away, draw yourself — and the fourth is the one that decided
//! the experiment: `requires_redraw`, which says whether the document has to
//! keep painting on this widget's account. A page is not an animation, so it
//! says no, and a document sitting still costs no frames.
//!
//! What replaces `keyFor()` is the pair of questions in `paint`: is the
//! texture the size the layout is asking for, and is it wearing the theme the
//! reader has chosen. The first re-renders the page; the second is a compute
//! pass over the copy already on the GPU. Under Tauri both were the same
//! question and both cost a full render.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use anyrender::{PaintScene, RenderContext, Scene};
use blitz_dom::node::ComputedStyles;
use blitz_dom::Widget;
use dioxus_native::DeviceHandle;
use peniko::kurbo::{Affine, Rect};
use peniko::{Blob, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageSampler};

use blitz_traits::shell::ShellProvider;

use crate::gpu::{PageTexture, Recolorer};
use crate::layout::MAX_PIXELS;
use crate::render::PageSource;
use crate::stats;
use crate::theme::Theme;

/// What every page needs to know and none of them owns: which theme is on.
///
/// A widget is handed to Blitz once, by a write-once attribute, so it cannot
/// be given new props the way a component can. This is the shared cell the app
/// writes and every mounted page reads on its next paint — which is the frame
/// the theme change causes anyway.
#[derive(Clone)]
pub struct Chosen {
    theme: Rc<Cell<Theme>>,
}

impl PartialEq for Chosen {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.theme, &other.theme)
    }
}

impl Chosen {
    pub fn new(theme: Theme) -> Self {
        Chosen {
            theme: Rc::new(Cell::new(theme)),
        }
    }

    pub fn get(&self) -> Theme {
        self.theme.get()
    }

    pub fn set(&self, theme: Theme) {
        self.theme.set(theme);
    }
}

/// A page's texture belongs to the node, not to the widget.
///
/// The obvious design gives the widget one texture and replaces it whenever
/// the page has to be drawn again — a new size, a new theme — unregistering
/// the old one on the way. **Unregistering a resource from inside `paint`
/// panics Vello**: "tried to draw an invalid empty image", from the atlas
/// upload at the start of the frame, and no amount of deferring it by a frame
/// helped. Leaving it registered instead is the other obvious answer and it
/// leaks a whole page of texture for every zoom step.
///
/// So the widget draws exactly once and the *component key* carries what
/// `keyFor()` carries: the page, the size it is drawn at, and the theme. A
/// change to any of them is a different node — a new widget, a new texture,
/// and the old node's resources released by Blitz through
/// `remove_custom_widget`, which runs between frames where it is safe.
///
/// The one thing that is not in the key is the screen's density, because
/// nothing tells the component when the window moves to a screen of a
/// different one. A page redrawn for that reason does leak its old texture
/// until it is unmounted, which is the next time the reader scrolls.
pub struct PageWidget {
    document: Arc<dyn PageSource>,
    index: usize,
    chosen: Chosen,
    /// How a widget asks for another frame. The `fresh` dance below needs the
    /// frame after the one that registered the texture, and
    /// `requires_redraw()` cannot ask for it: `is_animating()` is read at the
    /// start of a frame, before the paint that would set the flag. So the
    /// widget asks the shell directly, which is what the shell provider is
    /// for.
    shell: Option<Arc<dyn ShellProvider>>,
    device: Option<DeviceHandle>,
    recolorer: Option<Rc<Recolorer>>,
    texture: Option<PageTexture>,
    /// Whether the texture was registered during the frame being painted.
    ///
    /// Registering a texture and drawing it in the same frame works until
    /// something else is unregistered in that frame as well — which is exactly
    /// what happens when the window's real size arrives and every page is
    /// replaced at once. The new texture's override is then missing at submit
    /// and Vello panics with "tried to draw an invalid empty image", from a
    /// stack that says nothing about either of the two events that caused it.
    /// It reproduces on the third frame of every run.
    ///
    /// So a page is registered on one frame and drawn from the next.
    /// `requires_redraw` asks for that next frame and then stops asking, which
    /// is the one legitimate use of the flag that Phase 0 spent its time
    /// establishing must otherwise stay false: a page is not an animation, but
    /// a page that has just arrived is one frame's worth of one.
    fresh: bool,
    /// The same page, for a renderer that is not wgpu. See [`Software`].
    software: Option<Software>,
}

/// A page drawn for a renderer with no GPU behind it.
///
/// Two things want this. The harness is the loud one: `paint_scene` runs a
/// widget's `paint` with whatever `RenderContext` the scene is being built
/// for, and a headless test builds it for `vello_cpu`, where
/// `renderer_specific_context()` has no `DeviceHandle` in it and every page
/// would otherwise come out blank — a screenshot test that passes because it
/// photographed nothing. The quiet one is the risk row in the assessment:
/// `vello_cpu` is the fallback for hardware Vello's compute path will not run
/// on, and a reader whose pages are the one thing it cannot draw is not a
/// fallback.
///
/// It costs a copy the GPU path does not pay. pdfium writes BGRA into a buffer
/// it reuses; a `peniko::ImageData` owns its bytes through a `Blob`, and the
/// recolouring reference works in RGBA — so the page is swizzled and
/// recoloured into a buffer of its own, one per mounted page. That is the
/// price of not having a device to hand the bytes to, and it is why this is
/// the path not taken when there is one.
struct Software {
    image: ImageData,
    width: u32,
    height: u32,
    /// What `PageTexture::wears` answers on the other path.
    theme: Theme,
}

impl PageWidget {
    pub fn new(
        document: Arc<dyn PageSource>,
        index: usize,
        chosen: Chosen,
        shell: Option<Arc<dyn ShellProvider>>,
    ) -> Self {
        stats::add(&stats::MOUNTED, 1);
        PageWidget {
            document,
            index,
            chosen,
            shell,
            device: None,
            recolorer: None,
            texture: None,
            fresh: false,
            software: None,
        }
    }

    /// How many pixels this page is drawn at: what the layout asked for, held
    /// under the ceiling. A page drawn at more pixels than the screen can show
    /// is bytes nobody reads, and at high zoom on a large page it is a great
    /// many of them.
    fn drawn_size(width: u32, height: u32) -> (u32, u32) {
        let pixels = width as f64 * height as f64;
        if pixels <= MAX_PIXELS {
            return (width.max(1), height.max(1));
        }
        let shrink = (MAX_PIXELS / pixels).sqrt();
        (
            ((width as f64 * shrink).round() as u32).max(1),
            ((height as f64 * shrink).round() as u32).max(1),
        )
    }



    /// The same two questions, answered on the CPU. See [`Software`].
    ///
    /// The one difference that matters is where the theme goes on: there is no
    /// pass to re-run over a copy already uploaded, so a theme change is a
    /// re-render here. That is `keyFor()`'s own answer, and it is what makes
    /// this path a fallback rather than a design.
    fn ensure_software(&mut self, width: u32, height: u32) -> Option<()> {
        let theme = self.chosen.get();
        if let Some(page) = self.software.as_ref() {
            if page.width == width && page.height == height && page.theme == theme {
                return Some(());
            }
        }

        let mut pixels: Option<Vec<u8>> = None;
        let began = Instant::now();
        let outcome = self
            .document
            .render(self.index, width, height, &mut |bitmap| {
                // BGRA as pdfium wrote it, in RGBA order because that is what
                // the reference ramp reads — the swizzle the GPU path gets for
                // free by uploading as `Bgra8Unorm`.
                let mut rgba = bitmap.bgra.to_vec();
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                if theme.recolor {
                    crate::recolor::recolor_cpu(
                        &mut rgba,
                        theme.text,
                        theme.background,
                        theme.keep_colour,
                    );
                }
                pixels = Some(rgba);
            });
        let drew = began.elapsed();
        if let Err(err) = outcome {
            eprintln!("{err}");
            return None;
        }
        let pixels = pixels?;

        if let Some(old) = self.software.take() {
            stats::sub(&stats::RESIDENT, (old.width as u64) * (old.height as u64) * 4);
        }
        stats::add(&stats::DRAWN, 1);
        stats::add(&stats::DREW_US, drew.as_micros() as u64);
        stats::add(&stats::RESIDENT, (width as u64) * (height as u64) * 4);
        self.software = Some(Software {
            image: ImageData {
                data: Blob::new(Arc::new(pixels)),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width,
                height,
            },
            width,
            height,
            theme,
        });
        Some(())
    }

    /// Draw the page if it is not already drawn at this size, and put the
    /// theme on it if it is not already wearing it.
    fn ensure(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32) -> Option<()> {
        let theme = self.chosen.get();
        let recolorer = Rc::clone(self.recolorer.as_ref()?);

        if let Some(texture) = self.texture.as_ref() {
            // Size and theme together are what `keyFor()` is: a page that
            // matches both is the page already on the screen.
            if texture.is(width, height) && texture.wears(&theme) {
                return Some(());
            }
        }

        // The page is drawn into the renderer's own buffer and uploaded from
        // it, inside the borrow — which is why the upload happens in a closure
        // rather than after the call. See `render::Bitmap`.
        let began = Instant::now();
        let mut uploaded_texture = None;
        let mut uploaded = std::time::Duration::ZERO;
        let outcome = self
            .document
            .render(self.index, width, height, &mut |bitmap| {
                let began = Instant::now();
                uploaded_texture = recolorer.upload(ctx, &bitmap, &theme);
                uploaded = began.elapsed();
            });
        let drew = began.elapsed() - uploaded;
        if let Err(err) = outcome {
            eprintln!("{err}");
            return None;
        }
        let texture = uploaded_texture?;

        // The old texture, if there is one, is dropped without being
        // unregistered — see the note above the struct. It is the widget's
        // node going away that gives it back, and that is Blitz's job.
        if let Some(old) = self.texture.take() {
            stats::sub(&stats::RESIDENT, old.bytes());
        }

        stats::add(&stats::DRAWN, 1);
        stats::add(&stats::DREW_US, drew.as_micros() as u64);
        stats::add(&stats::UPLOADED_US, uploaded.as_micros() as u64);
        stats::add(&stats::RESIDENT, texture.bytes());
        self.texture = Some(texture);
        self.fresh = true;
        // And a frame to draw it in.
        if let Some(shell) = &self.shell {
            shell.request_redraw();
        }
        Some(())
    }
}

impl Widget for PageWidget {
    /// The renderer came up. Anything held from before it did belongs to a
    /// renderer that no longer exists — `WindowRenderer::suspend` drains every
    /// registered texture, and an id that outlives that is what Vello reports
    /// as "tried to draw an invalid empty image", from inside the next frame's
    /// atlas upload rather than from the call that orphaned it.
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        if let Some(texture) = self.texture.take() {
            stats::sub(&stats::RESIDENT, texture.bytes());
        }
        Recolorer::forget();
        // No device means this is not wgpu — a headless test or the CPU
        // fallback — and the page is drawn into a `peniko::ImageData` instead.
        // See [`Software`].
        if let Some(device) = render_ctx
            .renderer_specific_context()
            .and_then(|ctx| ctx.downcast::<DeviceHandle>().ok())
        {
            self.recolorer = Some(Recolorer::shared(&device));
            self.device = Some(*device);
        }
    }

    /// The renderer is going away, and with it every resource registered
    /// against it. Whatever is kept across this moment is a texture belonging
    /// to a renderer that no longer exists — which is not a blank page but a
    /// dead one: "tried to draw an invalid empty image".
    fn destroy_surfaces(&mut self) {
        if let Some(texture) = self.texture.take() {
            stats::sub(&stats::RESIDENT, texture.bytes());
        }
        if let Some(page) = self.software.take() {
            stats::sub(&stats::RESIDENT, (page.width as u64) * (page.height as u64) * 4);
        }
        self.device = None;
        self.recolorer = None;
        Recolorer::forget();
    }

    /// A page is not an animation — except for the single frame between
    /// registering its texture and drawing it. See `fresh`.
    fn requires_redraw(&self) -> bool {
        self.fresh
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Scene {
        stats::PAINTS.fetch_add(1, Ordering::Relaxed);

        let mut scene = Scene::new();
        if self.device.is_none() && self.software.is_none() {
            // `complete_resume` calls this for every widget in the document
            // when the renderer comes up; a page mounted later is caught here.
            self.can_create_surfaces(render_ctx);
        }
        // `width` and `height` arrive in *device* pixels — blitz-paint takes
        // them off the content box, which is already scaled — and `scale` is
        // passed alongside for whatever wants to know. Multiplying by it again
        // draws every page at twice the size it is shown at.
        let (drawn_width, drawn_height) = Self::drawn_size(width, height);

        // No device means no wgpu behind the scene being built — a headless
        // test, or the CPU fallback. Everything below the size is the same
        // question asked of a `peniko::ImageData` instead of a texture, and
        // none of the frame-ordering dance applies: there is nothing to
        // register, so there is nothing to register too early.
        if self.device.is_none() {
            if self.ensure_software(drawn_width, drawn_height).is_none() {
                return scene;
            }
            let Some(page) = self.software.as_ref() else {
                return scene;
            };
            let stretch = Affine::scale_non_uniform(
                width as f64 / page.width as f64,
                height as f64 / page.height as f64,
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ImageBrush {
                    image: &page.image,
                    sampler: ImageSampler::default(),
                },
                Some(stretch),
                &Rect::from_origin_size((0.0, 0.0), (width as f64, height as f64)),
            );
            return scene;
        }

        if self.ensure(render_ctx, drawn_width, drawn_height).is_none() {
            return scene;
        }
        let Some(texture) = self.texture.as_ref() else {
            return scene;
        };
        // A texture registered during this frame must not be drawn during it.
        // See `fresh` on the struct: this is the frame that registers, and
        // `requires_redraw` asks for the one that draws.
        if self.fresh {
            self.fresh = false;
            return scene;
        }

        // A page held under the ceiling is drawn smaller than its box, so the
        // brush is scaled up to fill it. Everything else lands at 1:1.
        let stretch = Affine::scale_non_uniform(
            width as f64 / texture.width as f64,
            height as f64 / texture.height as f64,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            anyrender::PaintRef::Resource(ImageBrush {
                image: texture.id(),
                sampler: ImageSampler::default(),
            }),
            Some(stretch),
            &Rect::from_origin_size((0.0, 0.0), (width as f64, height as f64)),
        );
        scene
    }
}

impl Drop for PageWidget {
    fn drop(&mut self) {
        // Unmounting is where the memory actually goes back, so this is the
        // half of `mount()`/`OVERSCAN` that the accounting can see.
        if let Some(texture) = self.texture.take() {
            stats::sub(&stats::RESIDENT, texture.bytes());
        }
        if let Some(page) = self.software.take() {
            stats::sub(&stats::RESIDENT, (page.width as u64) * (page.height as u64) * 4);
        }
        stats::sub(&stats::MOUNTED, 1);
    }
}
