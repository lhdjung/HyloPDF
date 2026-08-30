//! The renderer, behind the one door it will always be behind.
//!
//! The assessment's rule for this tree: one trait — draw a page, ask how big
//! it is, and later ask for its text, its outline and its links — so that
//! pdfium is a decision that can be remade rather than a dependency spread
//! through the viewer. It is the same rule `viewer.ts` obeys today by being
//! the only file that imports pdf.js, and it is what would make `hayro` a
//! swap rather than a rewrite when it grows text extraction.
//!
//! Phase 1 needed two of those questions, Phase 3's sidebar added the third
//! and its search the fourth — a page's text, and where every character of it
//! sits. The rest are named here and not declared,
//! because a trait method with no caller is a guess about what the caller
//! will want.

use std::sync::Arc;

use crate::layout::Size;

/// One page's pixels, as pdfium hands them over: BGRA, top row first.
///
/// BGRA rather than RGBA on purpose. The swizzle used to be a pass over every
/// pixel on the CPU — 1.6ms a page at 3.3 megapixels and 5.1ms at 10.1 — and
/// it is free on the GPU, because the texture is uploaded as `Bgra8Unorm` and
/// the recolouring shader that reads every pixel anyway sees it already in
/// order. So the bytes travel exactly as pdfium wrote them.
///
/// **Borrowed, not owned, and that is the whole point.** A page at the size
/// this app draws them is 24MB, and the first version of this trait returned a
/// `Vec` — which meant pdfium's own buffer, the `Vec` `as_raw_bytes()` copies
/// it into, and the `to_vec()` on top of that: three copies of every page,
/// alive at once, allocated and freed for every page drawn. macOS's allocator
/// does not hand large freed blocks straight back, so they showed up in the
/// process's physical footprint as `MALLOC_LARGE (empty)` — 120MB of it, on a
/// reader holding 46MB of actual page. The renderer draws into a buffer it
/// keeps and lends the bytes out for exactly as long as the upload takes.
pub struct Bitmap<'a> {
    pub width: u32,
    pub height: u32,
    pub bgra: &'a [u8],
    /// What the renderer spent drawing it, in milliseconds.
    pub drew_in: f64,
}

/// One line of a document's own table of contents.
///
/// Flat, with a depth, rather than a tree of children — which is what
/// `buildOutline` in `sidebar.ts` walks a tree to produce, and it produces
/// exactly this: a row, indented by its depth, that goes to a page. Nothing
/// above this ever asks an entry for its children, so the tree is flattened
/// where it is read rather than carried up to be flattened again.
///
/// `page` is one-based, and `None` for an entry whose destination the
/// document does not actually resolve — a broken outline is common enough
/// that it is a row you cannot click rather than a row that is not there.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub title: String,
    pub depth: usize,
    pub page: Option<usize>,
}

/// One character of a page, and where it sits on it.
///
/// **This is the thing pdf.js could not give and the reason the search here is
/// half the size of the app's.** pdf.js hands over *runs* — a string and a
/// transform — so `search.ts` has to fold the runs into one string, keep a
/// `starts[]` saying where each run began, binary-search that to turn an
/// offset back into a run and an offset inside it, and then hand the pair to
/// the DOM to be measured against a text layer whose spans exist only to be
/// selected. Every one of those steps is a place to be wrong, and the app has
/// a comment at each of them saying which way.
///
/// pdfium answers per character (`FPDFText_GetLooseCharBox`), so a match is a
/// range of characters and a range of characters is already a list of
/// rectangles. `starts`, `items`, `position()` and the text layer all go.
///
/// The box is in **PDF points with the origin at the top left**, which is
/// [`crate::layout`]'s space rather than the PDF's own: multiply by a
/// [`crate::layout::PageBox`]'s `scale` and it is where the highlight goes.
/// pdfium counts from the bottom, and the conversion belongs on the side that
/// knows the page height rather than in every caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharBox {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// A page's text, and where every character of it is.
///
/// `chars` and `boxes` are the same length and are indexed together — which is
/// the whole of the data structure, and is why `search.rs` can be about
/// searching.
#[derive(Clone, Debug, Default)]
pub struct PageText {
    pub chars: Vec<char>,
    pub boxes: Vec<CharBox>,
}

impl PageText {
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// The characters `from..to` as rectangles, one per line rather than one
    /// per character.
    ///
    /// A match is drawn as a few boxes and not as ninety, and the join is done
    /// here because it is the same question `joinRuns` in `viewer.ts` answers
    /// for the same reason: pdf.js's spans do not abut, so the gaps between
    /// them show as white rules through a highlighted sentence. Characters
    /// abut rather better than spans do, and the rule still holds — a run is
    /// extended while the next character sits on the same line, and a new one
    /// begins when it does not.
    pub fn quads(&self, from: usize, to: usize) -> Vec<CharBox> {
        let mut quads: Vec<CharBox> = Vec::new();
        for index in from..to.min(self.boxes.len()) {
            let glyph = self.boxes[index];
            // A character with no size is a space pdfium generated rather than
            // one the printer drew, and it would otherwise stretch a run to
            // the far edge of the page.
            if glyph.width <= 0.0 || glyph.height <= 0.0 {
                continue;
            }
            match quads.last_mut() {
                // The same line, if the two overlap vertically by most of
                // their height — which is a looser test than "the same top",
                // because a line of type is full of characters that sit a
                // fraction high or low.
                Some(run)
                    if overlap(run.top, run.height, glyph.top, glyph.height) > 0.5
                        && glyph.left + glyph.width > run.left
                        && glyph.left < run.left + run.width + glyph.height =>
                {
                    let right = (run.left + run.width).max(glyph.left + glyph.width);
                    let bottom = (run.top + run.height).max(glyph.top + glyph.height);
                    run.left = run.left.min(glyph.left);
                    run.top = run.top.min(glyph.top);
                    run.width = right - run.left;
                    run.height = bottom - run.top;
                }
                _ => quads.push(glyph),
            }
        }
        quads
    }
}

/// How much of the shorter of two vertical spans the two share, as a fraction.
fn overlap(top: f64, height: f64, other_top: f64, other_height: f64) -> f64 {
    let shared = (top + height).min(other_top + other_height) - top.max(other_top);
    let shortest = height.min(other_height);
    if shortest <= 0.0 {
        0.0
    } else {
        (shared / shortest).max(0.0)
    }
}

/// What a document is, to everything above it.
pub trait PageSource: Send + Sync {
    fn pages(&self) -> usize;
    /// A page's size in PDF points, which is what the layout works in.
    fn size_of(&self, index: usize) -> Size;
    /// Draw one page at exactly this many pixels, and lend the pixels to
    /// `take` for as long as it wants them.
    ///
    /// A callback rather than a return value because the buffer is the
    /// renderer's and is reused — see [`Bitmap`].
    fn render(
        &self,
        index: usize,
        width: u32,
        height: u32,
        take: &mut dyn FnMut(Bitmap) ,
    ) -> Result<(), String>;
    /// Where the document was opened from.
    ///
    /// Not a rendering question, and it is here because it is the only thing
    /// that identifies one document from another for anything that has to
    /// write it down — the library keys its entries by path, and the reader
    /// asks the document rather than being told twice. Empty for a document
    /// that came from nowhere in particular, which nothing here produces yet
    /// and a document handed over as bytes one day would.
    fn path(&self) -> &str {
        ""
    }

    /// The document's own table of contents, in reading order, flattened.
    ///
    /// Empty when there is none, which is most documents — and the sidebar
    /// says so in as many words rather than showing an empty column.
    fn outline(&self) -> Vec<Heading> {
        Vec::new()
    }

    /// A page's text, and where every character of it sits.
    ///
    /// Empty for a page with nothing on it, and empty for a *renderer* that
    /// cannot answer — which is not hypothetical: hayro, the pure-Rust
    /// renderer the assessment names as the one to watch, has no text
    /// extraction at all. So this has a default, and a reader over a renderer
    /// with no text in it finds nothing rather than failing to build. What
    /// says so on the screen is [`crate::search::State::textless`], which the
    /// app already needed for a scan nobody put through OCR.
    fn text_of(&self, _index: usize) -> PageText {
        PageText::default()
    }

    /// What opening the document cost, in milliseconds — the other half of the
    /// comparison with pdf.js, which spends most of a document open starting
    /// its worker.
    fn opened_in(&self) -> f64;
}

/// A document, opened by whichever renderer this build carries.
pub fn open(path: &str) -> Result<Arc<dyn PageSource>, String> {
    Ok(Arc::new(crate::pdfium::Document::open(path)?))
}
