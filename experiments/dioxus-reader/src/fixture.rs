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

/// A body of PDF objects, numbered from 1, and the file they make.
struct Pdf {
    objects: Vec<String>,
}

impl Pdf {
    fn new() -> Pdf {
        Pdf {
            objects: Vec::new(),
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
        let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1);
        for offset in &offsets {
            let _ = writeln!(out, "{offset:010} 00000 n ");
        }
        let _ = write!(
            out,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            self.objects.len() + 1,
            xref,
        );
        out
    }
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
    section(
        "Front matter",
        1,
        &[section("Preface", 2, &[])],
    ),
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
    let path: PathBuf = std::env::temp_dir().join("hylopdf-fixture-contents.pdf");
    if !path.is_file() {
        let bytes = build(12);
        // Written beside itself and renamed, for the same reason
        // `atomic_write` does it: `cargo test` runs in parallel and two tests
        // wanting this fixture at once must not read half of it.
        let temp = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&temp, &bytes).expect("write the fixture");
        std::fs::rename(&temp, &path).expect("put the fixture in place");
    }
    path.to_string_lossy().into_owned()
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
