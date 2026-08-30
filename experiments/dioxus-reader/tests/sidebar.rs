//! The panel on the left, driven headlessly.
//!
//! `sidebar.test.mjs` in the app is five tests and every one of them is about
//! the thumbnail cache: what is drawn, what is given back, what a theme change
//! redraws, what a closed panel does not draw. None of those five has anything
//! to assert here, because there is no cache — a thumbnail belongs to its row
//! and a row exists only while it is in view. What replaces them is the one
//! question underneath all five: *is the column's mounting window doing its
//! job*, which `src/sidebar.rs` asks of the geometry directly and this file
//! asks of the DOM.
//!
//! The rest is the half the app's own file never covered: the table of
//! contents, the marks, and the document getting narrower when the panel
//! opens.

use dioxus_reader::fixture;
use dioxus_reader::harness::{Options, Reader};

/// A twelve-page document that carries its own table of contents. See
/// `src/fixture.rs`: written in Rust so that `cargo test` needs cargo and
/// nothing else.
fn with_contents() -> Reader {
    Reader::open(&fixture::contents_pdf())
}

/// Four hundred pages of plain text, and no outline at all — which is the
/// commoner document of the two.
fn book() -> Reader {
    Reader::open(&Reader::book())
}

#[test]
fn the_panel_opens_and_shuts_and_is_remembered() {
    let config;
    {
        let mut reader = with_contents();
        assert_eq!(reader.state().sidebar, None, "shut, which is the default");
        reader.press_chord("mod+b");
        assert_eq!(reader.state().sidebar.as_deref(), Some("contents"));
        reader.press_chord("mod+b");
        assert_eq!(reader.state().sidebar, None);
        reader.press_chord("mod+b");
        config = reader.config.clone();
    }
    // A setting, like every other: the next run opens on it.
    let again = Reader::open_with(
        &fixture::contents_pdf(),
        Options {
            config,
            ..Default::default()
        },
    );
    assert_eq!(again.state().sidebar.as_deref(), Some("contents"));
}

#[test]
fn a_document_that_carries_its_contents_lists_them() {
    let mut reader = with_contents();
    reader.press_chord("mod+b");
    let rows = reader.harness.query_all(".outline-item");
    let expected = fixture::expected_headings();
    assert_eq!(rows.len(), expected.len(), "one row per heading");
    // The titles, in the document's own order.
    let titles: Vec<String> = rows
        .iter()
        .map(|node| {
            reader
                .harness
                .base()
                .get_node(*node)
                .map(|node| node.text_content())
                .unwrap_or_default()
        })
        .collect();
    let wanted: Vec<String> = expected.iter().map(|(title, ..)| title.clone()).collect();
    assert_eq!(titles, wanted);
    // And a nested heading is indented further than its parent, which is the
    // whole of what `depth` is for. Measured off the pixels rather than off
    // the box: the rows are all the width of the panel and the indent is
    // padding, so their rectangles are identical and only the ink moves.
    let bands: Vec<(u32, u32, u32, u32)> = rows[..2]
        .iter()
        .map(|node| {
            let rect = reader.harness.layout_rect_of(*node);
            (
                rect.x as u32,
                rect.y as u32 + 2,
                (rect.x + rect.width) as u32,
                (rect.y + rect.height) as u32 - 2,
            )
        })
        .collect();
    let shot = reader.screenshot();
    let parent = shot.leftmost_ink(bands[0]).expect("\"Front matter\" is drawn");
    let child = shot.leftmost_ink(bands[1]).expect("\"Preface\" is drawn");
    assert!(
        child > parent,
        "a heading under another starts further in: {child} against {parent}",
    );
}

#[test]
fn a_document_with_no_contents_says_so() {
    let mut reader = book();
    reader.press_chord("mod+b");
    // It opens on the pages rather than on an empty box — and the sentence is
    // there behind the other tab rather than nowhere.
    assert_eq!(reader.state().sidebar.as_deref(), Some("pages"));
    reader.click(".tab[data-tab='contents']");
    assert_eq!(
        reader.harness.text_content(".sidebar-empty"),
        "This document has no table of contents."
    );
}

#[test]
fn clicking_a_heading_goes_to_its_page() {
    let mut reader = with_contents();
    reader.press_chord("mod+b");
    assert_eq!(reader.state().page, 1);
    // "Chapter Two", which the fixture puts on page 8.
    let rows = reader.harness.query_all(".outline-item");
    let (title, _, page) = fixture::expected_headings()[6].clone();
    assert_eq!(title, "Chapter Two");
    let rect = reader.harness.layout_rect_of(rows[6]);
    let (x, y) = rect.center();
    reader.click_at(x, y);
    assert_eq!(reader.state().page, page);
}

#[test]
fn the_heading_the_reader_is_under_is_the_one_marked() {
    let mut reader = with_contents();
    reader.press_chord("mod+b");
    // Page 1 is under the first heading.
    assert_eq!(
        reader.harness.text_content(".outline-item.current"),
        "Front matter"
    );
    // Page 8 is "Chapter Two", and page 9 is still under it — a heading holds
    // until the next one starts.
    reader.press_chord("mod+b");
    for _ in 0..8 {
        reader.press("l");
    }
    reader.press_chord("mod+b");
    assert_eq!(reader.state().page, 9);
    assert_eq!(
        reader.harness.text_content(".outline-item.current"),
        "Chapter Two"
    );
}

#[test]
fn the_column_mounts_what_is_in_view_and_nothing_else() {
    let mut reader = book();
    reader.press_chord("mod+b");
    reader.click(".tab[data-tab='pages']");
    let first = reader.state().thumbs;
    assert!(!first.is_empty(), "the column is drawn");
    assert!(
        first.len() < 20,
        "four hundred pages, {} thumbnails: the column is a window, not a list",
        first.len()
    );
    assert_eq!(first[0], 1, "and it starts at the top");

    // Scrolling it moves the window rather than adding to it. This is the
    // whole of what `THUMB_CACHE` was for in the app, and there is nothing to
    // cap because nothing accumulates.
    reader.wheel_over(".panel.thumb-column", 3_000.0);
    let later = reader.state().thumbs;
    assert!(!later.is_empty());
    assert!(
        later.len() < 20,
        "{} thumbnails after scrolling",
        later.len()
    );
    assert!(
        later[0] > first[0],
        "the column moved: {} to {}",
        first[0],
        later[0]
    );
    assert!(
        later.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "and it is still a band"
    );
}

#[test]
fn the_column_follows_the_document_and_stops_there() {
    let mut reader = book();
    reader.press_chord("mod+b");
    reader.click(".tab[data-tab='pages']");
    assert!(reader.state().thumbs.contains(&1));
    // Read a long way into the book: the column comes with.
    for _ in 0..40 {
        reader.press(" ");
    }
    let page = reader.state().page;
    assert!(page > 8, "the reader has moved: page {page}");
    assert!(
        reader.state().thumbs.contains(&page),
        "the thumbnail for page {page} is in the column: {:?}",
        reader.state().thumbs
    );
}

#[test]
fn a_thumbnail_is_a_picture_of_its_page() {
    let mut reader = book();
    reader.press_chord("mod+b");
    reader.click(".tab[data-tab='pages']");
    let rect = reader.harness.layout_rect(".thumb-picture");
    let shot = reader.screenshot();
    let band = (
        rect.x as u32 + 4,
        rect.y as u32 + 4,
        (rect.x + rect.width) as u32 - 4,
        (rect.y + rect.height) as u32 - 4,
    );
    // Paper, with something on it. The fixture is one line of text near the
    // top of a page, so a thumbnail of it is mostly white and not entirely.
    let mean = shot.mean(band);
    assert!(mean[0] > 200.0, "a thumbnail is paper: {mean:?}");
    let ink = shot.unlike([255, 255, 255], band);
    assert!(ink > 0.0005, "…with ink on it: {ink:.4} of the picture");
}

#[test]
fn the_document_gets_narrower_when_the_panel_opens() {
    let mut reader = book();
    let wide = reader.harness.layout_rect(".page").width;
    reader.press_chord("mod+b");
    let narrow = reader.harness.layout_rect(".page").width;
    let panel = reader.harness.layout_rect(".sidebar").width;
    assert!(panel > 100.0, "the panel has a width: {panel}");
    assert!(
        (wide - narrow - panel).abs() < 2.0,
        "the page gives up exactly what the panel takes: {wide} - {narrow} against {panel}",
    );
}

#[test]
fn a_mark_is_a_toggle_and_survives_being_closed() {
    let config;
    let page;
    {
        let mut reader = with_contents();
        for _ in 0..3 {
            reader.press("l");
        }
        page = reader.state().page;
        assert_eq!(page, 4);
        reader.press_chord("mod+shift+b");
        assert_eq!(reader.state().notice, "Marked page 4");
        reader.press_chord("mod+b");
        // Named for the section it falls in, which the fixture calls
        // "A section" — a mark named "Page 4" is worth a great deal less.
        assert_eq!(reader.harness.text_content(".mark-go"), "A section");
        // The same gesture, doing the same thing.
        reader.press_chord("mod+shift+b");
        assert_eq!(reader.state().notice, "Took the mark off page 4");
        assert_eq!(reader.harness.query(".mark-go"), None);
        reader.press_chord("mod+shift+b");
        config = reader.config.clone();
    }
    let again = Reader::open_with(
        &fixture::contents_pdf(),
        Options {
            config,
            ..Default::default()
        },
    );
    assert_eq!(again.harness.text_content(".mark-go"), "A section");
}

#[test]
fn a_mark_goes_back_to_where_it_was_put() {
    let mut reader = with_contents();
    for _ in 0..5 {
        reader.press("l");
    }
    assert_eq!(reader.state().page, 6);
    reader.press_chord("mod+shift+b");
    reader.press("Home");
    assert_eq!(reader.state().page, 1);
    reader.press_chord("mod+b");
    reader.click(".mark-go");
    assert_eq!(reader.state().page, 6);
}

#[test]
fn a_mark_can_be_taken_off_from_the_panel() {
    let mut reader = with_contents();
    reader.press_chord("mod+shift+b");
    reader.press_chord("mod+b");
    assert!(reader.harness.query(".mark-go").is_some());
    reader.click(".mark-drop");
    assert_eq!(reader.harness.query(".mark-go"), None);
}
