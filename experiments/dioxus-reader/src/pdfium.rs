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
use crate::render::{Bitmap, Heading, PageSource};

/// The lock every call into pdfium is taken behind.
///
/// **`pdfium-render`'s `thread_safe` feature does not make pdfium thread
/// safe.** All it does is `unsafe impl Send for Pdfium` and `Sync` beside it,
/// plus a `Send + Sync` bound on the bindings accessor; nothing in the crate
/// serialises a call. pdfium itself has process-wide state and no locking, and
/// two threads inside it abort the process — `SIGABRT`, no panic, no message,
/// no stack, which is a C++ `CHECK` failing the way `PROGRESS.md` describes.
///
/// It was invisible while there was one document on one thread. It arrived
/// with the harness: `cargo test` runs its tests in parallel, four of them
/// opened four documents, and the whole binary vanished with exit code 134 and
/// nothing on stderr. So the lock is the library's, not the document's — a
/// per-document lock is exactly what was there and exactly what does not help.
static LIBRARY: Mutex<()> = Mutex::new(());

fn library() -> std::sync::MutexGuard<'static, ()> {
    LIBRARY.lock().unwrap_or_else(|e| e.into_inner())
}

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
    path: String,
    sizes: Vec<Size>,
    /// The document's own table of contents, read once when it is opened.
    ///
    /// Read at open rather than on demand, unlike everything else here, and
    /// for the reason the app reads it at open too: it is the one question
    /// whose answer decides what the sidebar *is* — a column of chapters or a
    /// sentence saying there are none — and a document of four hundred pages
    /// has an outline of tens of lines. What it costs is a walk of the
    /// bookmark tree behind the same lock every other call takes; what it
    /// saves is the sidebar having to be an async component to ask.
    outline: Vec<Heading>,
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
        // Asked before pdfium is, because pdfium answers it badly: a missing
        // file comes back as `IoError(Os { code: 2, kind: NotFound, … })`,
        // which is a Rust type name and a struct in front of the one fact
        // worth saying. It is also much the commonest way to fail here.
        if !std::path::Path::new(path).is_file() {
            return Err(format!("{path}: there is no such file."));
        }
        let _library = library();
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
        let outline = read_outline(&document);
        Ok(Document {
            path: path.to_string(),
            sizes,
            outline,
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

    fn path(&self) -> &str {
        &self.path
    }

    fn outline(&self) -> Vec<Heading> {
        self.outline.clone()
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
        // The library first, then the document — always in that order, which
        // is what keeps two documents from deadlocking each other.
        let _library = library();
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

/// The bookmark tree, flattened into the rows the sidebar draws.
///
/// Walked by hand rather than through `iter_all_descendants`, because the one
/// thing a row needs beyond its title and its page is how far to indent it,
/// and an iterator that flattens the tree has already thrown the depth away.
///
/// Three things it refuses. A bookmark with no title is skipped, because a
/// row with nothing written on it is a row nobody can aim at — `sidebar.ts`
/// writes "Untitled" instead, which is a worse answer to the same question
/// and one this reader does not have to repeat. A destination that does not
/// resolve leaves `page` at `None` rather than dropping the row, because the
/// heading is still the document's own account of itself. And the walk stops
/// at `LIMIT` entries and `DEPTH` levels: a malformed document can point a
/// bookmark's child or its next sibling at its own ancestor, and a table of
/// contents is not the place to find that out by running out of memory.
fn read_outline(document: &PdfDocument<'static>) -> Vec<Heading> {
    /// As many rows as anybody will ever scroll through, and few enough that
    /// a cycle cannot cost anything.
    const LIMIT: usize = 20_000;
    /// The PDF specification sets no limit on nesting and no real document
    /// goes past a handful.
    const DEPTH: usize = 16;

    let bookmarks = document.bookmarks();
    // `root()` is the *first top-level bookmark*, not a node above them all,
    // so the top level of the outline is that bookmark and its siblings.
    let mut top = Vec::new();
    let mut next = bookmarks.root();
    while let Some(bookmark) = next {
        next = bookmark.next_sibling();
        top.push(bookmark);
        if top.len() >= LIMIT {
            break;
        }
    }

    let mut headings = Vec::new();
    // Depth-first. Children are pushed in reverse so that popping them comes
    // back to reading order, and the level above is pushed in reverse for the
    // same reason.
    let mut stack: Vec<_> = top.into_iter().rev().map(|node| (node, 0usize)).collect();
    while let Some((bookmark, depth)) = stack.pop() {
        if headings.len() >= LIMIT {
            break;
        }
        let title = bookmark.title().unwrap_or_default().trim().to_string();
        // A destination of its own first, then the one its action carries:
        // most bookmarks have the first, and a bookmark written as a GoTo
        // action has only the second.
        let action = bookmark.action();
        let page = bookmark
            .destination()
            .and_then(|destination| destination.page_index().ok())
            .or_else(|| {
                action
                    .as_ref()?
                    .as_local_destination_action()?
                    .destination()
                    .ok()?
                    .page_index()
                    .ok()
            })
            .map(|index| index as usize + 1);
        if !title.is_empty() {
            headings.push(Heading { title, depth, page });
        }
        // Only children, never siblings: `iter_direct_children` already walks
        // the sibling chain under a node, so following a sibling here as well
        // is how every entry but the first of its level gets listed twice —
        // which is exactly what the first version of this did.
        if depth + 1 < DEPTH {
            let children: Vec<_> = bookmark.iter_direct_children().collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
    headings
}
