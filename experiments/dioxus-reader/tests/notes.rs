//! The notes a document already carries, made readable.
//!
//! `renderNotes` in `viewer.ts` and `showNote` in `main.ts`, ported: pdfium
//! paints an annotation's own appearance into the page, so a sticky note
//! arrives as the little icon it was drawn as and a comment arrives
//! highlighted — and the words behind either of them live in the annotation,
//! where nothing here was reading them. The icon sat there looking like a
//! button and was not one.

use dioxus_reader::fixture;
use dioxus_reader::harness::{Options, Reader};

fn annotated() -> Reader {
    Reader::open_with(&fixture::notes_pdf(), Options::default())
}

/// What counts as a note: anything with words in it, whatever its subtype —
/// and not a link, whose text is where it goes, nor an annotation with
/// nothing to read.
#[test]
fn a_note_is_any_annotation_with_words_in_it() {
    let reader = annotated();
    let spots = reader.harness.query_all(".note-spot").len();
    let edges = reader.harness.query_all(".note-edge").len();
    assert_eq!(spots, 1, "the sticky note, which is a marker");
    assert_eq!(edges, 1, "and the comment over a passage, which is a strip");
    // Two, and only two: the `/Square` with no `/Contents` has nothing to
    // read, and the link — which carries `/Contents` in this fixture on
    // purpose — is a link, whose text is where it goes. Either of them
    // counting would show up as a third spot here.
}

/// Pressing one opens it, and what it says is what the document says.
#[test]
fn pressing_a_note_opens_what_it_says() {
    let mut reader = annotated();
    assert!(reader.harness.query(".note-window").is_none());

    reader.click(".note-spot");
    let window = reader.harness.text_content(".note-window");
    assert!(
        window.contains("Check this against the second edition."),
        "the note's own words: {window:?}",
    );
    assert!(window.contains("A Reader"), "and who left it: {window:?}");
    assert!(window.contains("page 1"), "and where it is: {window:?}");

    reader.press("Escape");
    assert!(
        reader.harness.query(".note-window").is_none(),
        "Escape closes it, like every other window over the reader",
    );
}

/// A comment over a passage answers on the strip at its right edge, so the
/// words underneath stay in reach of a pointer that wants to select them.
#[test]
fn a_comment_over_a_passage_leaves_the_passage_alone() {
    let reader = annotated();
    let edge = reader.harness.layout_rect(".note-edge");
    let page = reader.harness.layout_rect(".page");
    assert!(edge.width < 20.0, "a strip, not a cover: {edge:?}");
    assert!(
        edge.x > page.x + page.width * 0.4,
        "at the right of what it comments on: {edge:?} on {page:?}",
    );
}
