//! Documents written by hand, for the tests that need a shape the app's own
//! fixtures do not have.
//!
//! `Reader::book()` points at `tests/fixtures/book.pdf`, which the app
//! generates with `node tests/fixtures/make-pdf.mjs` — and it stays that way,
//! because it is the document every memory number in `PROGRESS.md` was taken
//! on and a second copy of it would be a second thing to keep true.
//!
//! What is here instead is the shape the app has no fixture for at all: a
//! document that carries its own table of contents. It is written in Rust
//! rather than added to `make-pdf.mjs` for one reason, and it is the reason
//! `PROGRESS.md` gives for wanting this suite on three platforms: `cargo test`
//! should need cargo and nothing else. A fixture that needs Node to exist
//! first is a test that does not run on a machine which has not built the app.
//!
//! The writer is deliberately the dullest possible one — uncompressed streams,
//! one object per line of the body, a real `xref` table computed from the byte
//! offsets — because what is being tested is the reader, and a fixture that
//! needs debugging is a fixture that has stopped being evidence.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A body of PDF objects, numbered from 1, and the file they make.
struct Pdf {
    objects: Vec<String>,
    /// The `/Info` dictionary, if this document has one. Named in the trailer
    /// rather than reachable from the catalogue, which is where a PDF keeps
    /// the title it calls itself by and is why it is a field here.
    info: Option<usize>,
}

impl Pdf {
    fn new() -> Pdf {
        Pdf {
            objects: Vec::new(),
            info: None,
        }
    }

    /// Reserve an object number without saying yet what is in it, which is
    /// what a tree of cross-references needs: an outline item names its
    /// parent, its siblings and its page, and half of them are written after
    /// it.
    fn reserve(&mut self) -> usize {
        self.objects.push(String::new());
        self.objects.len()
    }

    fn put(&mut self, id: usize, body: impl Into<String>) {
        self.objects[id - 1] = body.into();
    }

    fn add(&mut self, body: impl Into<String>) -> usize {
        let id = self.reserve();
        self.put(id, body);
        id
    }

    /// The whole file: header, body, `xref` and trailer.
    fn bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len());
        for (index, body) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            let _ = write!(out, "{} 0 obj\n{}\nendobj\n", index + 1, body);
        }
        let xref = out.len();
        let _ = write!(
            out,
            "xref\n0 {}\n0000000000 65535 f \n",
            self.objects.len() + 1
        );
        for offset in &offsets {
            let _ = writeln!(out, "{offset:010} 00000 n ");
        }
        let info = match self.info {
            Some(id) => format!(" /Info {id} 0 R"),
            None => String::new(),
        };
        let _ = write!(
            out,
            "trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{}\n%%EOF\n",
            self.objects.len() + 1,
            xref,
        );
        out
    }
}

/// A fixture on disk: built once and reused, and written so that two tests
/// asking for it at the same moment both get the whole of it.
///
/// **Both halves of that are load-bearing, and the second one was wrong.** The
/// rename is what `atomic_write` does and for its reason — a reader must never
/// see half a file — but the temporary was named for the *process*, and
/// `cargo test` runs its tests as threads of one process. Two tests wanting a
/// fixture neither had yet wrote the same temporary and both renamed it: the
/// first succeeded, the second failed with `NotFound` on a file it had just
/// written. A counter is what makes the name a thread's rather than a
/// process's; the rename itself is already atomic, so whichever lands last
/// wins and both are the same bytes.
fn written(name: &str, build: impl FnOnce() -> Vec<u8>) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path: PathBuf = std::env::temp_dir().join(name);
    if !path.is_file() {
        let bytes = build();
        let temp = path.with_extension(format!(
            "{}.{}.part",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&temp, &bytes).expect("write the fixture");
        std::fs::rename(&temp, &path).expect("put the fixture in place");
    }
    path.to_string_lossy().into_owned()
}

/// One line of the table of contents a fixture is asked for: a title, the
/// page it goes to, and the entries under it.
pub struct Section {
    pub title: &'static str,
    pub page: usize,
    pub under: &'static [Section],
}

const fn section(title: &'static str, page: usize, under: &'static [Section]) -> Section {
    Section { title, page, under }
}

/// The contents the fixture carries, and therefore what a test asserts on:
/// seven rows over three levels, in this order.
pub const CONTENTS: &[Section] = &[
    section("Front matter", 1, &[section("Preface", 2, &[])]),
    section(
        "Chapter One",
        3,
        &[
            section("A section", 4, &[section("Under a section", 5, &[])]),
            section("Another section", 6, &[]),
        ],
    ),
    section("Chapter Two", 8, &[]),
    section("Index", 11, &[]),
];

/// The same list flattened the way [`crate::render::Heading`] flattens it:
/// title, depth, page.
pub fn expected_headings() -> Vec<(String, usize, usize)> {
    fn walk(into: &mut Vec<(String, usize, usize)>, sections: &[Section], depth: usize) {
        for section in sections {
            into.push((section.title.to_string(), depth, section.page));
            walk(into, section.under, depth + 1);
        }
    }
    let mut flat = Vec::new();
    walk(&mut flat, CONTENTS, 0);
    flat
}

/// A twelve-page document that carries [`CONTENTS`] as its own outline.
///
/// Written once per process into the temp directory and reused, because
/// writing it is a millisecond and every test that wants it wants the same
/// bytes.
pub fn contents_pdf() -> String {
    written("hylopdf-fixture-contents.pdf", || build(12))
}

/// A document of `pages` pages, written where you say, right now.
///
/// **The opposite of everything else in this file**, and deliberately: those
/// are cached in the temp directory and shared, because two tests wanting the
/// same fixture want the same bytes. This one is what a recompile looks like
/// — a named file that has to be *rewritten* while a reader is holding it
/// open — so it takes a path, writes every time, and each draft is a
/// different length so that a test can tell one from the other.
///
/// Written through the app's own [`crate::atomic_write`], which is what
/// `watch.rs` expects to see: a compiler replaces a document by writing
/// another one beside it and renaming it over the top, which is why the watch
/// is on the directory and not on the file.
pub fn draft(path: &std::path::Path, pages: usize) {
    crate::atomic_write(path, &build(pages)).expect("write the draft");
}

/// Three plain pages under a title the document gives itself.
///
/// The one shape none of the fixtures above has: an `/Info` dictionary with a
/// `/Title` in it, which is what `2310.06825v3.pdf` usually carries instead of
/// a name worth reading. Parameterised rather than fixed because the
/// interesting half of the feature is the *judgement* — a great many documents
/// name themselves "Microsoft Word - report.doc" or the file name over again,
/// and each of those is worse than the file name because it looks deliberate.
/// See [`crate::store::worth_calling`].
pub fn titled_pdf(title: &str) -> String {
    // The name on disk has to follow the title, or the second call would be
    // handed the first one's file. It is a digest rather than the title itself
    // because a title is somebody's prose and a file name is not.
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in title.as_bytes() {
        digest ^= *byte as u64;
        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let named = title.to_string();
    written(
        &format!("hylopdf-fixture-titled-{digest:016x}.pdf"),
        move || build_titled(&named),
    )
}

fn build_titled(title: &str) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let font = pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let page_ids: Vec<usize> = (0..3).map(|_| pdf.reserve()).collect();
    for (index, &id) in page_ids.iter().enumerate() {
        let stream = format!("BT /F1 18 Tf 72 700 Td (Page {}.) Tj ET", index + 1);
        let content = pdf.add(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
        pdf.put(
            id,
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 {font} 0 R >> >> /Contents {content} 0 R >>"
            ),
        );
    }
    pdf.put(
        tree,
        format!(
            "<< /Type /Pages /Count 3 /Kids [{}] >>",
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    );
    pdf.put(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    // A literal string, so the three characters that end one early are the
    // three to escape.
    let escaped: String = title
        .chars()
        .flat_map(|c| match c {
            '(' | ')' | '\\' => vec!['\\', c],
            other => vec![other],
        })
        .collect();
    pdf.info = Some(pdf.add(format!("<< /Title ({escaped}) >>")));
    pdf.bytes()
}

/// Six pages of prose, written to exercise the *fold* through the renderer
/// rather than in isolation.
///
/// `search.rs` tests `fold` directly and that is the right place for it, but
/// it proves nothing about what pdfium actually reports — and building this
/// found that two of the three answers are not what the app sees:
///
/// * **A ligature comes back already split.** pdfium hands over "f" and "i",
///   with a box each, whatever the font says. See the `/Differences` font
///   below.
/// * **An accent comes back precomposed**, U+00E9 rather than "e" and a
///   combining mark, so the fold's decompose-and-drop is what makes "resume"
///   find "résumé". As it is in the app.
/// * **A soft hyphen comes back as a soft hyphen** — but only if the document
///   says so in a `/ToUnicode` map. Written as the byte 0255 in
///   WinAnsiEncoding it is an ordinary hyphen, because that is what the
///   encoding says code 0255 is, which took a probe to notice and is the
///   reason there is a third font here.
///
/// Two things about the file. Everything above ASCII is written as a PDF octal
/// escape — `\351` for é, `\255` for the soft hyphen — because the bytes in a
/// PDF string are the font's codes rather than UTF-8, and writing the
/// character in the Rust source would put two bytes where the document wants
/// one. And the ligature has a font of its own: `/fi` is not in
/// WinAnsiEncoding, so it takes an `/Encoding` with a `/Differences` array to
/// name the glyph and a `/ToUnicode` map to say what it means — which is the
/// same pair of objects a real typesetter emits, and is the reason a
/// professionally set document does not contain "fi".
pub fn prose_pdf() -> String {
    written("hylopdf-fixture-prose.pdf", build_prose)
}

/// What each page of [`prose_pdf`] says: a list of runs, each naming the font
/// it is set in.
///
/// `F1` is Helvetica in WinAnsiEncoding and is everything ordinary. `F2` and
/// `F3` exist for one character each, and which of them a character needs is
/// the interesting part — see [`prose_pdf`].
pub const PROSE: &[&[(&str, &str)]] = &[
    &[("F1", "A needle in the first page.")],
    &[("F1", "Nothing to look for on this one.")],
    &[("F1", "The needle again, and a needle beside it.")],
    &[("F1", "Type "), ("F2", r"\001nd"), ("F1", " to find it.")],
    &[("F1", r"Her r\351sum\351 is filed on this page.")],
    &[
        ("F1", "The word typo"),
        ("F3", r"\002"),
        ("F1", "graphy broke across a line."),
    ],
];

fn build_prose() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let plain = pdf
        .add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    // The ligature: one glyph, named in a `/Differences` array, drawn by one
    // byte in the content stream.
    //
    // **And pdfium hands it back as two characters, "f" and "i", with a box
    // each** — with a `/ToUnicode` saying U+FB01 and without one alike, which
    // was worth finding out rather than assuming. So on this renderer the
    // ligature half of `fold` has nothing to do, where on pdf.js it is the
    // difference between finding "find" in a typeset book and not. The fold
    // keeps it: it is the app's own tested behaviour, hayro will not do
    // pdfium's normalising for us, and a document can carry U+FB01 by other
    // routes. What this page tests is the claim a reader would make — that
    // searching for "find" finds a word set with a ligature — which is true
    // here for a different reason than it is true in the app.
    let ligature = pdf.add(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
         /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding \
         /Differences [1 /fi] >> >>",
    );
    // The soft hyphen, which needs a map for the opposite reason: in
    // WinAnsiEncoding the code 0255 *is* `hyphen`, so writing the byte gets an
    // ordinary hyphen back and the fold has nothing to do. A `/ToUnicode`
    // saying U+00AD is how a document actually carries one, and is what a
    // typesetter breaking a word across a line emits.
    let map = to_unicode(&mut pdf, "1 beginbfchar <02> <00ad> endbfchar");
    let soft = pdf.add(format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
         /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding \
         /Differences [2 /hyphen] >> /ToUnicode {map} 0 R >>"
    ));

    let page_ids: Vec<usize> = PROSE.iter().map(|_| pdf.reserve()).collect();
    for (index, &id) in page_ids.iter().enumerate() {
        let mut stream = String::from("BT 18 Tf 72 700 Td");
        for (font, text) in PROSE[index] {
            stream.push_str(&format!(" /{font} 18 Tf ({text}) Tj"));
        }
        stream.push_str(" ET");
        let content = pdf.add(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
        pdf.put(
            id,
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 {plain} 0 R /F2 {ligature} 0 R \
                 /F3 {soft} 0 R >> >> /Contents {content} 0 R >>"
            ),
        );
    }
    pdf.put(
        tree,
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            page_ids.len(),
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    );
    pdf.put(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.bytes()
}

/// A `/ToUnicode` CMap around one `beginbfchar` block.
fn to_unicode(pdf: &mut Pdf, chars: &str) -> usize {
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
         /CMapName /Custom def /CMapType 2 def\n\
         1 begincodespacerange <00> <ff> endcodespacerange\n\
         {chars}\n\
         endcmap CMapName currentdict /CMap defineresource pop end end"
    );
    pdf.add(format!(
        "<< /Length {} >>\nstream\n{}\nendstream",
        cmap.len(),
        cmap
    ))
}

/// What [`links_pdf`] carries, so that a test can name a link rather than an
/// index: the page it is on, the area it covers in **top-left points**, and
/// where it goes.
pub const LINKS: &[(usize, [f64; 4], &str)] = &[
    // Page one: one address, one place in this document. Both are written the
    // ordinary way, with a `/Dest` on the annotation.
    (1, [72.0, 72.0, 128.0, 20.0], "https://example.com/paper"),
    (1, [72.0, 122.0, 128.0, 20.0], "page 5"),
    // Page two: the same thing said the other way, as a `/GoTo` action — which
    // is the route `link.action()` answers and `link.destination()` does not.
    (2, [72.0, 72.0, 128.0, 20.0], "page 6"),
    // And a link that points nowhere at all, which is dropped rather than kept
    // as a rectangle that does nothing when it is clicked.
    (3, [72.0, 72.0, 128.0, 20.0], ""),
];

/// Where the internal link on page one lands, as a fraction of the page.
///
/// The destination is `/XYZ null 400 null` on a page 792 points tall, and
/// pdfium counts from the bottom: 392 points down of 792.
pub const LINK_OFFSET: f64 = (792.0 - 400.0) / 792.0;

/// What the labelled pages of [`links_pdf`] are called, in order.
///
/// Three pages of front matter numbered in lower-case roman, then a body that
/// starts again at 1 — which is the shape the whole of `label` and
/// `page_for_label` exists for, and the shape no fixture in the app has.
pub const LABELS: &[&str] = &["i", "ii", "iii", "1", "2", "3"];

/// Where the ink sits on every page of [`margins_pdf`], as fractions of the
/// page: left, top, right, bottom.
///
/// Deliberately not the same on both axes. A crop that keeps the page's shape
/// is a crop a layout test cannot see, and the whole of what trimming does to
/// a reader is change the shape of what is in front of them.
pub const INK: (f64, f64, f64, f64) = (0.2, 0.1, 0.8, 0.9);

/// Three pages with a black rectangle on each and nothing else.
///
/// The one fixture here that is about *pixels* rather than about text, and it
/// exists because the margins have to be arithmetic: every other document in
/// this file carries a single line of type, whose ink box is a band a few
/// percent tall, and a crop measured off one is decided entirely by the
/// clamp in [`crate::crop`] rather than by the page. Here the answer is
/// [`INK`] padded, and a test can say so in numbers.
pub fn margins_pdf() -> String {
    written("hylopdf-fixture-margins.pdf", build_margins)
}

fn build_margins() -> Vec<u8> {
    const PAGES: usize = 3;
    const WIDTH: f64 = 612.0;
    const HEIGHT: f64 = 792.0;
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let page_ids: Vec<usize> = (0..PAGES).map(|_| pdf.reserve()).collect();

    // PDF space counts from the bottom, so the fractions above are turned
    // over on the way in: what `INK` calls the top is the far edge from the
    // origin.
    let left = INK.0 * WIDTH;
    let right = INK.2 * WIDTH;
    let bottom = (1.0 - INK.3) * HEIGHT;
    let top = (1.0 - INK.1) * HEIGHT;
    let stream = format!(
        "0 0 0 rg {left:.2} {bottom:.2} {:.2} {:.2} re f",
        right - left,
        top - bottom
    );
    let content = pdf.add(format!(
        "<< /Length {} >>\nstream\n{}\nendstream",
        stream.len(),
        stream
    ));

    for &id in &page_ids {
        pdf.put(
            id,
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] \
                 /Resources << >> /Contents {content} 0 R >>"
            ),
        );
    }
    pdf.put(
        tree,
        format!(
            "<< /Type /Pages /Count {PAGES} /Kids [{}] >>",
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    );
    pdf.put(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    pdf.bytes()
}

/// Six pages that carry links and number their own pages.
///
/// Two documents in one, because they are the same item of the plan and a
/// second fixture is a second thing to keep true. The links are written three
/// ways on purpose — see [`LINKS`] — and the labels are the `/PageLabels`
/// number tree, which is the only way a PDF says what is printed on a page.
pub fn links_pdf() -> String {
    written("hylopdf-fixture-links.pdf", build_links)
}

fn build_links() -> Vec<u8> {
    const PAGES: usize = 6;
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let font = pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let page_ids: Vec<usize> = (0..PAGES).map(|_| pdf.reserve()).collect();

    // The annotations, written after the pages are numbered because two of
    // them name a page. `/Rect` is the PDF's own space, counting from the
    // bottom, which is where [`LINKS`] stops agreeing with the file.
    let external = pdf.add(
        "<< /Type /Annot /Subtype /Link /Rect [72 700 200 720] /Border [0 0 0] \
         /A << /S /URI /URI (https://example.com/paper) >> >>",
    );
    let internal = pdf.add(format!(
        "<< /Type /Annot /Subtype /Link /Rect [72 650 200 670] /Border [0 0 0] \
         /Dest [{} 0 R /XYZ null 400 null] >>",
        page_ids[4],
    ));
    let by_action = pdf.add(format!(
        "<< /Type /Annot /Subtype /Link /Rect [72 700 200 720] /Border [0 0 0] \
         /A << /S /GoTo /D [{} 0 R /Fit] >> >>",
        page_ids[5],
    ));
    // Neither an action nor a destination: a rectangle that means nothing.
    let empty = pdf.add("<< /Type /Annot /Subtype /Link /Rect [72 700 200 720] /Border [0 0 0] >>");

    for (index, &id) in page_ids.iter().enumerate() {
        let text = format!("Page {} of the fixture.", index + 1);
        let stream = format!("BT /F1 18 Tf 72 700 Td ({text}) Tj ET");
        let content = pdf.add(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
        let annots = match index {
            0 => format!(" /Annots [{external} 0 R {internal} 0 R]"),
            1 => format!(" /Annots [{by_action} 0 R]"),
            2 => format!(" /Annots [{empty} 0 R]"),
            _ => String::new(),
        };
        pdf.put(
            id,
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 {font} 0 R >> >> /Contents {content} 0 R{annots} >>"
            ),
        );
    }
    pdf.put(
        tree,
        format!(
            "<< /Type /Pages /Count {PAGES} /Kids [{}] >>",
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    );
    // The number tree: pages 0..2 in lower-case roman, then a fresh decimal
    // run starting at 1. `/St` is where the run starts and is the half a
    // document usually gets wrong.
    pdf.put(
        catalog,
        format!(
            "<< /Type /Catalog /Pages {tree} 0 R \
             /PageLabels << /Nums [0 << /S /r >> 3 << /S /D /St 1 >>] >> >>"
        ),
    );
    pdf.bytes()
}

fn build(pages: usize) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let font = pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");

    let page_ids: Vec<usize> = (0..pages).map(|_| pdf.reserve()).collect();
    for (index, &id) in page_ids.iter().enumerate() {
        let text = format!("Page {} of the fixture.", index + 1);
        let stream = format!("BT /F1 18 Tf 72 700 Td ({text}) Tj ET");
        let content = pdf.add(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
        pdf.put(
            id,
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 {font} 0 R >> >> /Contents {content} 0 R >>"
            ),
        );
    }
    pdf.put(
        tree,
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages,
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    );

    let outline = pdf.reserve();
    let top = write_sections(&mut pdf, CONTENTS, outline, &page_ids);
    pdf.put(
        outline,
        format!(
            "<< /Type /Outlines /Count {} {} >>",
            CONTENTS.len(),
            first_and_last(&top),
        ),
    );
    pdf.put(
        catalog,
        format!(
            "<< /Type /Catalog /Pages {tree} 0 R /Outlines {outline} 0 R /PageMode /UseOutlines >>"
        ),
    );
    pdf.bytes()
}

/// Write one level of the outline and return the object number of each entry
/// in it, in order — so the level above can chain them.
fn write_sections(
    pdf: &mut Pdf,
    sections: &[Section],
    parent: usize,
    page_ids: &[usize],
) -> Vec<usize> {
    let ids: Vec<usize> = sections.iter().map(|_| pdf.reserve()).collect();
    for (at, section) in sections.iter().enumerate() {
        let children = write_sections(pdf, section.under, ids[at], page_ids);
        let page = page_ids[(section.page - 1).min(page_ids.len() - 1)];
        let mut body = format!(
            "<< /Title ({}) /Parent {parent} 0 R /Dest [{page} 0 R /XYZ null null null]",
            section.title,
        );
        if at > 0 {
            body.push_str(&format!(" /Prev {} 0 R", ids[at - 1]));
        }
        if at + 1 < ids.len() {
            body.push_str(&format!(" /Next {} 0 R", ids[at + 1]));
        }
        if !children.is_empty() {
            // A positive count is an outline the viewer opens; the sign is
            // the whole of what "expanded" means in the format.
            body.push_str(&format!(
                " /Count {} {}",
                children.len(),
                first_and_last(&children)
            ));
        }
        body.push_str(" >>");
        pdf.put(ids[at], body);
    }
    ids
}

fn first_and_last(ids: &[usize]) -> String {
    match (ids.first(), ids.last()) {
        (Some(first), Some(last)) => format!("/First {first} 0 R /Last {last} 0 R"),
        _ => String::new(),
    }
}
