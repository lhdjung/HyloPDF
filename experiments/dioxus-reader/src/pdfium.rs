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
use crate::render::{Bitmap, Heading, Link, PageSource, PageText, Rect, Target};

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
    /// What the document calls its own pages, read once beside their sizes,
    /// and empty when it calls them 1 to n. See [`PageSource::labels`].
    labels: Vec<String>,
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
        // One pass, because loading a page is what both of these cost and
        // `pages().iter()` loads each one. Four hundred pages is a few
        // milliseconds; asking twice would be twice that for no reason.
        let mut sizes = Vec::with_capacity(document.pages().len() as usize);
        let mut labels = Vec::with_capacity(sizes.capacity());
        for page in document.pages().iter() {
            sizes.push(Size {
                width: page.width().value as f64,
                height: page.height().value as f64,
            });
            labels.push(page.label().unwrap_or_default().to_string());
        }
        let outline = read_outline(&document);
        Ok(Document {
            path: path.to_string(),
            labels: own_numbering(labels),
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

    fn labels(&self) -> Vec<String> {
        self.labels.clone()
    }

    /// The links on one page.
    ///
    /// Three things about pdfium's answer are worth knowing.
    ///
    /// *A link's area comes from the annotation and not from the action.*
    /// `FPDFLink_GetAnnotRect` is the `/Rect` of the `/Link` annotation, in
    /// the page's own points counting from the bottom — so the flip is the
    /// same one [`PageSource::text_of`] does, in the same place, for the same
    /// reason.
    ///
    /// *A destination arrives two ways and a document uses either.* Most
    /// links carry a `/Dest`, which `destination()` answers; one written as a
    /// `/GoTo` action carries it under `/A` instead, which is
    /// `as_local_destination_action`. The bookmark walk below has exactly this
    /// shape and for exactly this reason.
    ///
    /// *And a link with neither is dropped rather than kept as a dead
    /// rectangle.* A `/Launch` action naming a program, a `/JavaScript`
    /// action, a `/Dest` that resolves to no page: each of them is a hit area
    /// over printed words that would do nothing when it was clicked, which
    /// reads as the app being broken rather than as the document being odd.
    fn links_of(&self, index: usize) -> Vec<Link> {
        let _library = library();
        let held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(page) = held.document.pages().get(index as i32) else {
            return Vec::new();
        };
        let height = page.height().value as f64;
        let mut links = Vec::new();
        for link in page.links().iter() {
            let Ok(rect) = link.rect() else { continue };
            let area = Rect {
                left: rect.left().value as f64,
                top: height - rect.top().value as f64,
                width: (rect.right().value - rect.left().value) as f64,
                height: (rect.top().value - rect.bottom().value) as f64,
            };
            // A link with no area is not a link anybody can click, whatever
            // it points at.
            if area.width <= 0.0 || area.height <= 0.0 {
                continue;
            }
            let action = link.action();
            let away = action
                .as_ref()
                .and_then(|action| action.as_uri_action())
                .and_then(|uri| uri.uri().ok())
                .filter(|uri| !uri.is_empty());
            let target = match away {
                Some(uri) => Target::Away(uri),
                None => {
                    let place = link.destination().or_else(|| {
                        action
                            .as_ref()?
                            .as_local_destination_action()?
                            .destination()
                            .ok()
                    });
                    let Some(place) = place else { continue };
                    let Ok(page) = place.page_index() else {
                        continue;
                    };
                    Target::Place {
                        page: page as usize + 1,
                        offset: offset_within(&place, height),
                    }
                }
            };
            links.push(Link { rect: area, target });
        }
        links
    }

    fn opened_in(&self) -> f64 {
        self.opened_in
    }

    /// One page's characters and where each of them sits.
    ///
    /// Three things about this are pdfium's and are worth knowing before
    /// changing it.
    ///
    /// *`loose_bounds`, not `tight_bounds`.* The tight box is the glyph's own
    /// outline, so a highlight drawn from it clips the ascenders and descenders
    /// of the very words it is meant to mark, and a lower-case run comes out
    /// half the height of the line it is on. The loose box is the character's
    /// cell — the line's height, the advance's width — which is what a reader
    /// means by "highlight this".
    ///
    /// *A character can have no box at all.* pdfium generates spaces and line
    /// breaks that the printer never drew, and asking one for its bounds fails
    /// rather than returning nothing. Those characters are in the text — they
    /// are what makes two words two words — so they are kept, with a box of no
    /// size, and [`PageText::quads`] is what skips them.
    ///
    /// *And this is one FFI call per character.* At a couple of thousand
    /// characters a page it is the cost of the whole feature; see
    /// `search.rs` for what that measures at and what the reader does about
    /// it.
    fn text_of(&self, index: usize) -> PageText {
        let _library = library();
        let held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(page) = held.document.pages().get(index as i32) else {
            return PageText::default();
        };
        // pdfium counts from the bottom of the page and the layout counts from
        // the top, so the flip happens here, where the page height is already
        // in hand.
        let height = page.height().value as f64;
        let text = page.text();
        let Ok(text) = text else {
            return PageText::default();
        };
        let chars = text.chars();
        let mut out = PageText {
            chars: Vec::with_capacity(chars.len()),
            boxes: Vec::with_capacity(chars.len()),
        };
        for character in chars.iter() {
            let Some(value) = character.unicode_char() else {
                continue;
            };
            let glyph = character
                .loose_bounds()
                .map(|rect| Rect {
                    left: rect.left().value as f64,
                    top: height - rect.top().value as f64,
                    width: (rect.right().value - rect.left().value) as f64,
                    height: (rect.top().value - rect.bottom().value) as f64,
                })
                .unwrap_or(Rect {
                    left: 0.0,
                    top: 0.0,
                    width: 0.0,
                    height: 0.0,
                });
            out.chars.push(value);
            out.boxes.push(glyph);
        }
        out
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

/// How far down its page a destination sits, as a fraction of the page's
/// height.
///
/// `offsetWithin` in `viewer.ts`, said in pdfium's own terms: `/XYZ` and the
/// `Fit*` forms that name a top edge are the ones that say where on the page
/// they mean, and everything else means the top of it. pdfium answers all of
/// them through one call, so the six view settings collapse to "is there a y,
/// and is a y what this form means".
///
/// Clamped at 0.95 for the app's reason: a destination at the very bottom of a
/// page scrolls that page out of the window entirely, and the reader lands
/// looking at the next one with nothing to say why.
fn offset_within(destination: &PdfDestination, height: f64) -> f64 {
    use PdfDestinationViewSettings as View;
    let top = match destination.view_settings() {
        Ok(View::SpecificCoordinatesAndZoom(_, Some(y), _)) => y,
        Ok(View::FitPageHorizontallyToWindow(Some(y))) => y,
        Ok(View::FitBoundsHorizontallyToWindow(Some(y))) => y,
        _ => return 0.0,
    };
    if height <= 0.0 {
        return 0.0;
    }
    // pdfium counts from the bottom of the page, as it does everywhere else
    // here.
    ((height - top.value as f64) / height).clamp(0.0, 0.95)
}

/// The labels, unless they say nothing.
///
/// See [`PageSource::labels`]: a document that labels its pages 1 to n has
/// said exactly what the position already said, and a list of those is a list
/// every lookup above would run for no reason. A document with no labels at
/// all comes back from pdfium as a list of empty strings and is the same case.
fn own_numbering(labels: Vec<String>) -> Vec<String> {
    let says_nothing = labels
        .iter()
        .enumerate()
        .all(|(index, label)| label.is_empty() || label == &(index + 1).to_string());
    if says_nothing {
        Vec::new()
    } else {
        labels
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
