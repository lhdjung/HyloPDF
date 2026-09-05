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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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
use crate::recolor::Region;
use crate::layout::{View, MAX_PIXELS};
use crate::render::PageSource;
use crate::stats;
use crate::palette::Palette;

/// What a page has painted *into* it rather than drawn over it.
///
/// **Two things here are the colour of the page rather than a rectangle on top
/// of it.** A link, because a tinted box blended into the ink below is at the
/// mercy of the compositor and a dropped blend is a solid band across the line.
/// A selected passage, because a translucent rectangle leaves the words the
/// colour they were printed in — it says *something is selected here* rather
/// than *these words are selected*.
///
/// **In fractions of the page's box**, the one space both ends agree about: a
/// texture is drawn at the box's size times the density, held under a ceiling
/// and frozen mid-pinch, and a fraction survives all three.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ramped {
    /// Left, top, right, bottom. The document's own links.
    pub links: Vec<[f32; 4]>,
    /// The same, for what the reader has swept over.
    pub selection: Vec<[f32; 4]>,
}

/// What every page needs to know and none of them owns: which theme is on, and
/// what it has to paint into itself.
///
/// A widget is handed to Blitz once, by a write-once attribute, so it cannot be
/// given new props the way a component can. This is the shared cell the app
/// writes and every mounted page reads on its next paint — which is the frame
/// the theme change causes anyway.
#[derive(Clone)]
pub struct Chosen {
    theme: Rc<Cell<Palette>>,
    /// What each mounted page has to paint into itself, by page index.
    ///
    /// A shared cell for the reason the theme is one: a widget is handed to
    /// Blitz once and cannot be given new props, and a selection changes on
    /// every frame of a drag — putting it in the component's key would redraw
    /// the page from pdfium for every pixel the pointer moves.
    ramped: Rc<RefCell<HashMap<usize, Ramped>>>,
    /// Whether a zoom gesture is under way, in which case a page keeps the
    /// texture it has and is stretched to whatever size the layout is asking
    /// for this frame. See [`Chosen::holding`].
    holding: Rc<Cell<bool>>,
}

impl PartialEq for Chosen {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.theme, &other.theme)
    }
}

impl Chosen {
    pub fn new(theme: Palette) -> Self {
        Chosen {
            theme: Rc::new(Cell::new(theme)),
            ramped: Rc::new(RefCell::new(HashMap::new())),
            holding: Rc::new(Cell::new(false)),
        }
    }

    /// What every mounted page has to paint into itself, all of it at once.
    ///
    /// The whole map rather than an entry, because the caller has the whole
    /// answer — it is building the pages — and because replacing it is what
    /// keeps this from growing by a page for every page ever scrolled past.
    pub fn place(&self, ramped: HashMap<usize, Ramped>) {
        *self.ramped.borrow_mut() = ramped;
    }

    pub fn ramped(&self, index: usize) -> Ramped {
        self.ramped.borrow().get(&index).cloned().unwrap_or_default()
    }

    pub fn get(&self) -> Palette {
        self.theme.get()
    }

    pub fn set(&self, theme: Palette) {
        self.theme.set(theme);
    }

    /// **A pinch is not a hundred zoom steps to be drawn one at a time.**
    ///
    /// A trackpad sends a magnification event a frame, each changing the size
    /// every page is laid out at. Redrawing on each asks pdfium for a page sixty
    /// times a second — and because a texture registered on one frame must not
    /// be drawn until the next (see `fresh`), every one of those frames paints
    /// nothing, so the document goes blank until the fingers stop.
    ///
    /// While this is on, a page keeps the texture it has and is stretched to the
    /// box the layout asks for: the words grow under the fingers, a little soft,
    /// and come back sharp when the gesture settles. The frozen size is in the
    /// component key too, so nothing is re-keyed on the way.
    pub fn holding(&self) -> bool {
        self.holding.get()
    }

    pub fn hold(&self, holding: bool) {
        self.holding.set(holding);
    }
}

/// A page's texture belongs to the node, not to the widget.
///
/// The obvious design gives the widget one texture and replaces it whenever the
/// page is drawn again, unregistering the old one. **Unregistering a resource
/// from inside `paint` panics Vello** — "tried to draw an invalid empty image",
/// from the atlas upload — and leaving it registered leaks a whole page of
/// texture for every zoom step.
///
/// So the widget draws exactly once and the *component key* carries what
/// `keyFor()` carries: the page, its size, the theme. A change to any of them is
/// a different node — a new widget, a new texture, and the old node's resources
/// released by Blitz between frames, where it is safe.
///
/// Not in the key: the screen's density, because nothing tells the component
/// when the window moves to a screen of a different one. A page redrawn for that
/// reason leaks its old texture until it is unmounted.
pub struct PageWidget {
    document: Arc<dyn PageSource>,
    index: usize,
    /// How the page is turned and how much of it is drawn.
    ///
    /// Given once, like the page number, because it is in the component's key
    /// — the same reason the size is not stored here and the theme is. A turn
    /// or a trim is a different node, a new widget and a new texture, and the
    /// old one is released by Blitz between frames where it is safe.
    view: View,
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
    /// something else is unregistered in that frame too — which is what happens
    /// when every page is replaced at once. The new texture's override is
    /// missing at submit and Vello panics with "tried to draw an invalid empty
    /// image", from a stack naming neither cause.
    ///
    /// So a page is registered on one frame and drawn from the next.
    /// `requires_redraw` asks for that frame and then stops: a page is not an
    /// animation, but a page that has just arrived is one frame's worth of one.
    ///
    /// **And that only moves the collision a frame along.** The frame where
    /// every page is replaced at once is the whole of the problem, and it used
    /// to happen on every launch because the viewer was laid out at a default
    /// viewport and corrected on mount. `Reader` sizes it from the window before
    /// the first frame now. **Anything that re-keys every page at once brings
    /// this back.**
    fresh: bool,
    /// The same page, for a renderer that is not wgpu. See [`Software`].
    software: Option<Software>,
}

/// A page drawn for a renderer with no GPU behind it.
///
/// Two things want this. The harness builds its scene for `vello_cpu`, where
/// `renderer_specific_context()` has no `DeviceHandle` and every page would
/// otherwise come out blank — a screenshot test passing because it photographed
/// nothing. And `vello_cpu` is the fallback for hardware Vello's compute path
/// will not run on, where a reader whose pages are the one thing it cannot draw
/// is not a fallback.
///
/// It costs a copy the GPU path does not pay: a `peniko::ImageData` owns its
/// bytes and the recolouring reference works in RGBA, so the page is swizzled
/// and recoloured into a buffer of its own, one per mounted page.
struct Software {
    image: ImageData,
    width: u32,
    height: u32,
    /// What `PageTexture::wears` answers on the other path.
    theme: Palette,
    /// The page with its links tinted and nothing selected on it, kept so that
    /// a selection can be taken up again without going back to pdfium.
    ///
    /// **The same allocation as `image` until something is selected**, which is
    /// what an `Arc` is for: while there is no selection the two point at one
    /// buffer and this costs a word, and a selection makes the second copy that
    /// the ramp is written into. The GPU path keeps only what is under the
    /// runs, because there a copy of the page is 25MB of texture.
    plain: Arc<Vec<u8>>,
    /// What is painted through the selection ramp on top of it.
    selected: Vec<[f32; 4]>,
}

impl PageWidget {
    pub fn new(
        document: Arc<dyn PageSource>,
        index: usize,
        view: View,
        chosen: Chosen,
        shell: Option<Arc<dyn ShellProvider>>,
    ) -> Self {
        stats::add(&stats::MOUNTED, 1);
        PageWidget {
            document,
            index,
            view,
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



    /// The links on this page, as regions of the drawn page.
    ///
    /// **Links are tinted under every theme**, including the ones that leave
    /// the document alone — `renderPage` in `viewer.ts` calls `tintLinks`
    /// outside the `theme.recolor` branch, and the sentence beside it is the
    /// reason: a link that reads exactly like the sentence around it is a link
    /// nobody can see, and whether the page has been recoloured is beside that
    /// point. What changes with the theme is only what the link's *paper* maps
    /// to: the theme's background on a recoloured page, so the rectangle does
    /// not show as a patch, and the white pdf.js and pdfium both draw on where
    /// the page was left as it was printed.
    fn links(&self, theme: &Palette, width: u32, height: u32) -> Vec<Region> {
        let paper = if theme.recolor {
            theme.background
        } else {
            [0xff, 0xff, 0xff]
        };
        self.chosen
            .ramped(self.index)
            .links
            .iter()
            .map(|area| Region {
                area: [
                    area[0] * width as f32,
                    area[1] * height as f32,
                    area[2] * width as f32,
                    area[3] * height as f32,
                ],
                ink: theme.link,
                paper,
            })
            .collect()
    }

    /// The two colours a selection is painted between, the right way round for
    /// the page as it stands. See `regions.wgsl` and `selectionPaint` in
    /// `viewer.ts`.
    fn selection_ramp(theme: &Palette) -> (crate::recolor::Rgb, crate::recolor::Rgb) {
        let lit = theme.recolor
            && crate::palette::luminance(theme.text) > crate::palette::luminance(theme.background);
        if lit {
            (theme.selection_area, theme.selection_text)
        } else {
            (theme.selection_text, theme.selection_area)
        }
    }

    /// The same two questions, answered on the CPU. See [`Software`].
    ///
    /// The one difference that matters is where the theme goes on: there is no
    /// pass to re-run over a copy already uploaded, so a theme change is a
    /// re-render here. That is `keyFor()`'s own answer, and it is what makes
    /// this path a fallback rather than a design.
    fn ensure_software(&mut self, width: u32, height: u32) -> Option<()> {
        let theme = self.chosen.get();
        let selection = self.chosen.ramped(self.index).selection;
        if let Some(page) = self.software.as_ref() {
            if page.theme == theme
                && (self.chosen.holding() || (page.width == width && page.height == height))
            {
                if page.selected != selection {
                    self.paint_selection_software(&theme, selection);
                }
                return Some(());
            }
        }

        let mut pixels: Option<Vec<u8>> = None;
        let began = Instant::now();
        let outcome = self
            .document
            .render(self.index, width, height, self.view, &mut |bitmap| {
                // BGRA as pdfium wrote it, in RGBA order because that is what
                // the reference ramp reads — the swizzle the GPU path gets for
                // free by uploading as `Bgra8Unorm`.
                let mut rgba = bitmap.bgra.to_vec();
                for pixel in rgba.as_chunks_mut::<4>().0 {
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
                crate::recolor::duotone_cpu(
                    &mut rgba,
                    width,
                    height,
                    &self.links(&theme, width, height),
                );
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
        let pixels = Arc::new(pixels);
        self.software = Some(Software {
            image: ImageData {
                data: Blob::new(pixels.clone()),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width,
                height,
            },
            width,
            height,
            theme,
            plain: pixels,
            selected: Vec::new(),
        });
        if !selection.is_empty() {
            self.paint_selection_software(&theme, selection);
        }
        Some(())
    }

    /// The selection ramp over the page the CPU path has already drawn, and off
    /// again — from the copy kept beside it. See [`Software::plain`].
    fn paint_selection_software(&mut self, theme: &Palette, runs: Vec<[f32; 4]>) {
        let Some(page) = self.software.as_mut() else {
            return;
        };
        let (ink, paper) = Self::selection_ramp(theme);
        let (width, height) = (page.width, page.height);
        let regions: Vec<Region> = runs
            .iter()
            .map(|area| Region {
                area: [
                    (area[0] * width as f32).floor(),
                    (area[1] * height as f32).floor(),
                    (area[2] * width as f32).ceil(),
                    (area[3] * height as f32).ceil(),
                ],
                ink,
                paper,
            })
            .collect();
        // Nothing selected is the page as it was drawn, which is the buffer
        // already in hand — no copy at all rather than a second identical page.
        // See [`Software::plain`].
        let data = if regions.is_empty() {
            Blob::new(page.plain.clone())
        } else {
            let mut pixels = page.plain.as_ref().clone();
            crate::recolor::duotone_cpu(&mut pixels, width, height, &regions);
            Blob::new(Arc::new(pixels))
        };
        page.image = ImageData {
            data,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width,
            height,
        };
        page.selected = runs;
    }

    /// Draw the page if it is not already drawn at this size, and put the
    /// theme on it if it is not already wearing it.
    fn ensure(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32) -> Option<()> {
        let theme = self.chosen.get();
        let recolorer = Rc::clone(self.recolorer.as_ref()?);

        let selection = self.chosen.ramped(self.index).selection;
        if let Some(texture) = self.texture.as_ref() {
            // Size and theme together are what `keyFor()` is: a page that
            // matches both is the page already on the screen.
            //
            // …except under a zoom gesture, where the size is the one thing
            // deliberately allowed to differ: the texture is kept and
            // stretched rather than redrawn. See [`Chosen::holding`]. The
            // theme is still asked, because a theme changed mid-gesture is a
            // page that is the wrong colour rather than the wrong sharpness.
            if texture.wears(&theme) && (self.chosen.holding() || texture.is(width, height)) {
                // The selection is the one thing that moves without the page
                // being redrawn, and it is asked here rather than in the key
                // for exactly that reason.
                let (ink, paper) = Self::selection_ramp(&theme);
                let texture = self.texture.as_mut()?;
                recolorer.select(texture, &selection, ink, paper);
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
            .render(self.index, width, height, self.view, &mut |bitmap| {
                let began = Instant::now();
                uploaded_texture =
                    recolorer.upload(ctx, &bitmap, &theme, &self.links(&theme, width, height));
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
        if !selection.is_empty() {
            let (ink, paper) = Self::selection_ramp(&theme);
            let texture = self.texture.as_mut()?;
            recolorer.select(texture, &selection, ink, paper);
        }
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

        // A page drawn at a different size from its box — held under the
        // ceiling, or frozen for the length of a zoom gesture — is scaled to
        // fill it. Everything else lands at 1:1.
        //
        // **The scale goes in the scene transform, not the brush transform.**
        // `fill` in `anyrender_vello_hybrid` has two arms and the one for a
        // `PaintRef::Resource` ignores `brush_transform` completely: it takes
        // the shape's *origin*, draws the texture there at its own size, and
        // stops. Only the other arm reads it, which is why the software path
        // below is written the ordinary way and passes. So the destination
        // rectangle is the *texture's* and the scene transform stretches it,
        // which `draw_texture_rects` composes.
        //
        // Invisible until a page's texture is not the size of its box, and then
        // it is a page whose border grows under the reader's fingers while the
        // print inside stays where it was.
        let stretch = Affine::scale_non_uniform(
            width as f64 / texture.width as f64,
            height as f64 / texture.height as f64,
        );
        scene.fill(
            Fill::NonZero,
            stretch,
            anyrender::PaintRef::Resource(ImageBrush {
                image: texture.id(),
                sampler: ImageSampler::default(),
            }),
            None,
            &Rect::from_origin_size(
                (0.0, 0.0),
                (texture.width as f64, texture.height as f64),
            ),
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
