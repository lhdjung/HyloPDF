//! The renderer, behind the one door it will always be behind.
//!
//! The assessment's rule for this tree: one trait — draw a page, ask how big
//! it is, and later ask for its text, its outline and its links — so that
//! pdfium is a decision that can be remade rather than a dependency spread
//! through the viewer. It is the same rule `viewer.ts` obeys today by being
//! the only file that imports pdf.js, and it is what would make `hayro` a
//! swap rather than a rewrite when it grows text extraction.
//!
//! Phase 1 needs two of those questions. The rest are named here and not
//! declared, because a trait method with no caller is a guess about what the
//! caller will want.

use std::sync::Arc;

use crate::layout::Size;

/// One page's pixels, as pdfium hands them over: BGRA, top row first.
///
/// BGRA rather than RGBA on purpose. The swizzle used to be a pass over every
/// pixel on the CPU — 1.6ms a page at 3.3 megapixels and 5.1ms at 10.1 — and
/// it is free on the GPU, because the texture is uploaded as `Bgra8Unorm` and
/// the recolouring shader that reads every pixel anyway sees it already in
/// order. So the bytes travel exactly as pdfium wrote them.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    /// What the renderer spent drawing it, in milliseconds.
    pub drew_in: f64,
}

/// What a document is, to everything above it.
pub trait PageSource: Send + Sync {
    fn pages(&self) -> usize;
    /// A page's size in PDF points, which is what the layout works in.
    fn size_of(&self, index: usize) -> Size;
    /// Draw one page at exactly this many pixels.
    fn render(&self, index: usize, width: u32, height: u32) -> Result<Bitmap, String>;
    /// What opening the document cost, in milliseconds — the other half of the
    /// comparison with pdf.js, which spends most of a document open starting
    /// its worker.
    fn opened_in(&self) -> f64;
}

/// A document, opened by whichever renderer this build carries.
pub fn open(path: &str) -> Result<Arc<dyn PageSource>, String> {
    Ok(Arc::new(crate::pdfium::Document::open(path)?))
}
