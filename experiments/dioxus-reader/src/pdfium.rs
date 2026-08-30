//! pdfium behind `PageSource`.
//!
//! Two things about pdfium are restated from `render.rs` on the
//! `pdfium-prototype` branch, because they are properties of the library and
//! not of the app: it has one global initialiser and no thread safety, so
//! there is one instance for the process and a lock around it; and a page
//! costs nothing once dropped, so there is no page cache to keep here — what
//! is cached is the texture, one layer up, where the memory actually is.

use std::sync::Mutex;
use std::time::Instant;

use pdfium_render::prelude::*;

use crate::layout::Size;
use crate::render::{Bitmap, PageSource};

/// The one pdfium instance, created on first use and kept for the life of the
/// process. Leaked deliberately: every document and page borrows from it.
fn pdfium() -> Result<&'static Pdfium, String> {
    static INSTANCE: Mutex<Option<&'static Pdfium>> = Mutex::new(None);
    let mut held = INSTANCE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(instance) = *held {
        return Ok(instance);
    }
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
        &library_dir(),
    ))
    .map_err(|e| format!("pdfium could not be loaded: {e}"))?;
    let instance: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
    *held = Some(instance);
    Ok(instance)
}

/// Where `libpdfium` is. In the experiment it is vendored beside the spike; in
/// a bundled app it would sit beside the executable. Nothing is fetched at
/// runtime, which is the promise the pdf.js assets make today.
fn library_dir() -> String {
    if let Ok(dir) = std::env::var("HYLO_PDFIUM") {
        return dir;
    }
    format!("{}/../dioxus-spike/vendor/lib", env!("CARGO_MANIFEST_DIR"))
}

pub struct Document {
    inner: Mutex<Open>,
    sizes: Vec<Size>,
    opened_in: f64,
}

struct Open {
    document: PdfDocument<'static>,
    /// The one buffer every page is drawn into.
    ///
    /// pdfium will make its own if asked (`render_with_config`), and then
    /// `as_raw_bytes()` copies it into a `Vec` — two allocations of 24MB per
    /// page at the sizes this app draws at, freed immediately and *not* handed
    /// back by the allocator. `PdfBitmap::from_bytes` renders into a buffer we
    /// own instead, so a document scrolled from end to end allocates once.
    ///
    /// It lives behind the same lock as the document because pdfium is not
    /// thread safe and every render is already serialised through it — so
    /// "one buffer" and "one page drawn at a time" are the same statement.
    scratch: Vec<u8>,
}

// pdfium is not thread safe and everything here is behind the lock; the
// `PdfDocument` borrows from the leaked instance, which lives forever.
unsafe impl Send for Open {}

impl Document {
    pub fn open(path: &str) -> Result<Self, String> {
        let began = Instant::now();
        let pdfium = pdfium()?;
        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| format!("{path}: {e}"))?;
        let sizes = document
            .pages()
            .iter()
            .map(|page| Size {
                width: page.width().value as f64,
                height: page.height().value as f64,
            })
            .collect();
        Ok(Document {
            sizes,
            opened_in: began.elapsed().as_secs_f64() * 1000.0,
            inner: Mutex::new(Open {
                document,
                scratch: Vec::new(),
            }),
        })
    }
}

impl PageSource for Document {
    fn pages(&self) -> usize {
        self.sizes.len()
    }

    fn size_of(&self, index: usize) -> Size {
        self.sizes[index]
    }

    fn opened_in(&self) -> f64 {
        self.opened_in
    }

    fn render(
        &self,
        index: usize,
        width: u32,
        height: u32,
        take: &mut dyn FnMut(Bitmap),
    ) -> Result<(), String> {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let held = &mut *held;
        let page = held
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("page {index}: {e}"))?;

        // A row can be wider than the pixels in it; ask rather than assume.
        // (For BGRA it works out at exactly four bytes a pixel, because the
        // stride is rounded up to a multiple of four and it is already one.)
        let wanted = PdfBitmap::bytes_required_for_size_and_format(
            width as i32,
            height as i32,
            PdfBitmapFormat::BGRA,
        );
        if held.scratch.len() != wanted {
            // Only when the size actually changes — which is a zoom or a
            // window resize, not a page turn.
            held.scratch.clear();
            held.scratch.resize(wanted, 0);
        }
        let mut bitmap =
            PdfBitmap::from_bytes(
                width as i32,
                height as i32,
                PdfBitmapFormat::BGRA,
                &mut held.scratch,
            )
                .map_err(|e| format!("page {index}: {e}"))?;

        let config = PdfRenderConfig::new().set_target_size(width as i32, height as i32);
        let began = Instant::now();
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(|e| format!("page {index}: {e}"))?;
        let drew_in = began.elapsed().as_secs_f64() * 1000.0;
        drop(bitmap);

        take(Bitmap {
            width,
            height,
            bgra: &held.scratch,
            drew_in,
        });
        Ok(())
    }
}
