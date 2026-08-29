//! Spike 2: a page on the screen, and then twenty of them in a column.
//!
//! The question the whole experiment turns on. pdfium draws a page in single
//! milliseconds — `render.rs` beside this says 3.4ms at 10.1 megapixels — and
//! the reason that was not worth having under Tauri is that the pixels then
//! had to cross into another process. Here they do not: the bitmap becomes a
//! `wgpu::Texture` in the process that is already drawing the window.
//!
//! So what is being measured is not the drawing. It is everything the page
//! costs *after* pdfium is done with it, and there are three parts to that,
//! each of which was a surprise worth writing down.
//!
//! *A `<canvas>` in a Blitz document makes the document animate for ever.*
//! `load_custom_paint_src` sets `has_canvas`, `is_animating()` is
//! `has_canvas | has_active_animations`, and `View::redraw` asks for another
//! frame whenever the document is animating. So one page on screen means the
//! whole scene repaints at the display's rate, awake or idle, which is the
//! opposite of what "no animations unless the user takes an action" asks for.
//!
//! *A registered texture is copied into Vello's image atlas every frame.*
//! `Renderer::register_texture` says so in as many words. A page at 10
//! megapixels is 40MB, so twenty mounted pages is 800MB of GPU-to-GPU copying
//! per frame if every one of them is really copied.
//!
//! *pdfium hands over BGRA and Vello wants RGBA*, so something has to swizzle.
//! Here it is the CPU, once per page, and it is measured; the shape the app
//! would actually use is the recolouring compute pass, which has to read every
//! pixel anyway and can swap two channels for nothing.
//!
//! `--pages N` mounts N pages. `--quit N` ends after N seconds, which is what
//! makes this measurable without anybody sitting in front of it.

use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::time::Instant;

use anyrender_vello::wgpu;
use dioxus::prelude::*;
use dioxus_native::{
    use_wgpu, CustomPaintCtx, CustomPaintSource, DeviceHandle, LogicalSize, TextureHandle,
    WindowAttributes,
};
use dioxus_spike::pdf::Document;
use dioxus_spike::shell::{Shell, WindowSpec};

fn main() {
    let args: Vec<String> = env::args().collect();
    let flag = |name: &str, fallback: usize| -> usize {
        args.iter()
            .position(|a| a == name)
            .and_then(|at| args.get(at + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    };
    let pages = flag("--pages", 20);
    let quit_after = flag("--quit", 0);
    // How wide a page is drawn. Narrow pages put more of them on the screen at
    // once, which is the only way to ask what N resident page textures cost
    // without a scroll gesture to make.
    let page_width = flag("--width", 800);
    let path = args
        .iter()
        .skip(1)
        .find(|a| a.ends_with(".pdf"))
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{}/../../tests/fixtures/book.pdf",
                env!("CARGO_MANIFEST_DIR")
            )
        });

    let document = match Document::open(&path) {
        Ok(doc) => Rc::new(doc),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    println!(
        "pages: {} pages in {path}, opened in {:.0}ms",
        document.pages(),
        document.opened_in
    );

    let event_loop = blitz_shell::create_default_event_loop::<blitz_shell::BlitzShellEvent>();
    let mut shell = Shell::new(event_loop.create_proxy());
    shell.trace = false;
    let windows = shell.windows();

    let attributes = WindowAttributes::default()
        .with_title("Spike: pages")
        .with_inner_size(LogicalSize::new(900.0, 900.0));
    let vdom = VirtualDom::new_with_props(
        Reader,
        ReaderProps {
            document: document.clone(),
            pages: pages.min(document.pages()),
            page_width: page_width as f64,
        },
    );
    windows.open(WindowSpec::new(vdom, attributes));

    if quit_after > 0 {
        let remote = windows.remote();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(quit_after as u64));
            STATS.with(|_| ());
            remote.quit();
        });
    }

    event_loop.run_app(&mut shell).unwrap();
}

/// What a run of this costs, counted where it happens.
#[derive(Default)]
struct Stats {
    frames: u64,
    /// Pages drawn by pdfium, which should settle at one per mounted page.
    drawn: u64,
    /// What pdfium spent, and what the swizzle and the upload spent after it.
    drew_ms: f64,
    swizzled_ms: f64,
    uploaded_ms: f64,
    /// The pixels alive on the GPU, as pages come and go.
    resident: u64,
    began: Option<Instant>,
    reported: Option<Instant>,
}

impl Stats {
    /// A line a second, printed from wherever the frame happened to be. It is
    /// here rather than on a timer because the tokio runtime a `use_future`
    /// would need is started by `dioxus_native::launch`, which a shell that
    /// makes its own windows does not call.
    fn report(&mut self) {
        let now = Instant::now();
        let since = match self.reported {
            Some(then) if (now - then).as_secs_f64() < 1.0 => return,
            Some(then) => (now - then).as_secs_f64(),
            None => {
                self.reported = Some(now);
                return;
            }
        };
        self.reported = Some(now);
        println!(
            "pages: {:.0} frames/s | {} pages drawn, {:.1}ms each in pdfium, \
             {:.1}ms swizzling, {:.1}ms uploading | {:.0}MB of texture resident",
            self.frames as f64 / since,
            self.drawn,
            self.drew_ms / self.drawn.max(1) as f64,
            self.swizzled_ms / self.drawn.max(1) as f64,
            self.uploaded_ms / self.drawn.max(1) as f64,
            self.resident as f64 * 4.0 / 1e6,
        );
        self.frames = 0;
    }
}

thread_local! {
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

/// One page, drawn by pdfium and handed to Vello as a texture.
struct PageSource {
    document: Rc<Document>,
    index: usize,
    device: Option<DeviceHandle>,
    texture: Option<TextureHandle>,
    /// The size the texture was drawn at, in device pixels. This is `keyFor()`
    /// in `viewer.ts`, with everything the app does not have yet left out.
    drawn_at: Option<(u32, u32)>,
    pixels: u64,
    /// The texture this page used to be, waiting to be given back.
    ///
    /// Unregistering it in the same `render` that replaces it kills the
    /// process: vello checks its image overrides when the frame is submitted,
    /// which is after this returns, and reports the one just taken out as
    /// "an invalid empty image ... unregistered before this render was
    /// submitted". So a replaced texture is released at the start of the next
    /// frame instead, when nothing can still be pointing at it.
    stale: Option<TextureHandle>,
}

impl PageSource {
    fn new(document: Rc<Document>, index: usize) -> Self {
        Self {
            document,
            index,
            device: None,
            texture: None,
            drawn_at: None,
            pixels: 0,
            stale: None,
        }
    }
}

impl CustomPaintSource for PageSource {
    fn resume(&mut self, device_handle: &DeviceHandle) {
        // A resume is a new device and a new `vello::Renderer`, so whatever
        // was registered with the old one is gone. Keeping the handle across
        // it is what kills the process on the next frame.
        self.device = Some(device_handle.clone());
        self.texture = None;
        self.stale = None;
        self.drawn_at = None;
    }

    fn suspend(&mut self) {
        self.device = None;
        self.texture = None;
        self.drawn_at = None;
    }

    fn render(
        &mut self,
        mut ctx: CustomPaintCtx<'_>,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Option<TextureHandle> {
        if std::env::var("SPIKE_TRACE").is_ok() {
            eprintln!(
                "source {}: render {width}x{height} @{scale}, device {}",
                self.index,
                self.device.is_some()
            );
        }
        if let Some(stale) = self.stale.take() {
            ctx.unregister_texture(stale);
        }
        let device = self.device.clone()?;
        // `width` and `height` arrive in *device* pixels — blitz-paint takes
        // them off the content box, which is already scaled — and `scale` is
        // passed alongside for whatever wants to know. Multiplying by it again
        // draws every page at twice the size it is shown at.
        let _ = scale;
        let want = (width.max(1), height.max(1));
        if self.drawn_at == Some(want) {
            // Nothing has changed, so nothing is redrawn — but note that the
            // texture is still handed back, and Vello still copies it into its
            // atlas. Not redrawing is not the same as not paying.
            STATS.with(|stats| {
                let mut stats = stats.borrow_mut();
                stats.frames += 1;
                stats.report();
            });
            return self.texture.clone();
        }

        let bitmap = match self.document.render(self.index, want.0, want.1) {
            Ok(bitmap) => bitmap,
            Err(err) => {
                eprintln!("{err}");
                return None;
            }
        };

        // pdfium's BGRA into Vello's Rgba8Unorm. The real app would do this in
        // the recolouring pass, which reads every pixel anyway.
        let began = Instant::now();
        let mut rgba = bitmap.bgra;
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let swizzled = began.elapsed().as_secs_f64() * 1000.0;

        let began = Instant::now();
        self.stale = self.texture.take();
        let texture = device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("page"),
            size: wgpu::Extent3d {
                width: bitmap.width,
                height: bitmap.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Rgba8Unorm and COPY_SRC are what `register_texture` requires:
            // the data is copied into Vello's image atlas at the start of
            // every frame.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        device.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bitmap.width * 4),
                rows_per_image: Some(bitmap.height),
            },
            wgpu::Extent3d {
                width: bitmap.width,
                height: bitmap.height,
                depth_or_array_layers: 1,
            },
        );
        let handle = ctx.register_texture(texture);
        if std::env::var("SPIKE_TRACE").is_ok() {
            eprintln!(
                "source {}: registered blob {} at {}x{}",
                self.index,
                handle.0.data.id(),
                bitmap.width,
                bitmap.height
            );
        }
        let uploaded = began.elapsed().as_secs_f64() * 1000.0;

        self.pixels = bitmap.width as u64 * bitmap.height as u64;
        self.drawn_at = Some(want);
        self.texture = Some(handle.clone());

        STATS.with(|stats| {
            let mut stats = stats.borrow_mut();
            stats.began.get_or_insert_with(Instant::now);
            stats.drawn += 1;
            stats.drew_ms += bitmap.drew_in;
            stats.swizzled_ms += swizzled;
            stats.uploaded_ms += uploaded;
            stats.resident += self.pixels;
            stats.frames += 1;
            stats.report();
        });
        Some(handle)
    }
}

impl Drop for PageSource {
    fn drop(&mut self) {
        STATS.with(|stats| {
            let mut stats = stats.borrow_mut();
            stats.resident = stats.resident.saturating_sub(self.pixels);
        });
    }
}

#[component]
fn Reader(document: Rc<Document>, pages: usize, page_width: f64) -> Element {
    rsx! {
        style { {STYLE} }
        div { class: "column",
            for index in 0..pages {
                Page { document: document.clone(), index, width: page_width }
            }
        }
    }
}

/// One page in the column: a box of the page's own proportions, with a canvas
/// filling it. The width is fixed here rather than fitted, because fit width
/// is the layout's question and this spike is about the pixels.
#[component]
fn Page(document: Rc<Document>, index: usize, width: f64) -> Element {
    let size = document.sizes[index];
    let height = (width * size.height as f64 / size.width as f64).round();
    let source = use_wgpu({
        let document = document.clone();
        move || PageSource::new(document, index)
    });

    rsx! {
        div { class: "page", style: "width: {width}px; height: {height}px;",
            canvas { "src": "{source}", style: "display: block; width: {width}px; height: {height}px;" }
        }
    }
}

const STYLE: &str = r#"
body { margin: 0; background: #2b2b31; }
.column {
  height: 100vh; overflow: scroll;
  display: flex; flex-direction: column; align-items: center; gap: 24px;
  padding: 24px 0;
}
.page { flex-shrink: 0; background: #fff; box-shadow: 0 1px 6px rgba(0,0,0,0.4); }
"#;
