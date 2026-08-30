//! The reader, driven: the file `reader.test.mjs` is in this tree.
//!
//! Everything here goes through `harness::Reader`, which is a real
//! `DioxusDocument` with real style, real layout and the real event pipeline,
//! and no window anywhere. Assertions are on what the interface says about
//! itself — the pill, the chips, the notice — rather than on the `Viewer`
//! behind them, for the same reason the app's own harness reads the DOM: a
//! test that reaches past the interface cannot tell you the interface is
//! wired up.

use dioxus_reader::harness::{Options, Reader};

fn book() -> Reader {
    Reader::open(&Reader::book())
}

#[test]
fn a_document_opens_on_its_first_page() {
    let reader = book();
    let state = reader.state();
    assert_eq!(state.pages, 400);
    assert_eq!(state.page, 1);
    assert_eq!(state.scroll, 0.0);
    assert_eq!(state.zoom, "Fit width");
    assert_eq!(state.mounted, vec![1]);
}

#[test]
fn the_wheel_moves_the_document() {
    let mut reader = book();
    reader.wheel_screen();
    let after = reader.state();
    assert!(after.scroll > 700.0, "{after:?}");

    // And it stops at the end rather than running past it.
    for _ in 0..3 {
        reader.press("End");
    }
    let end = reader.state();
    assert_eq!(end.page, 400);
    reader.wheel_screen();
    assert_eq!(reader.state().scroll, end.scroll, "the end is the end");
}

#[test]
fn the_keys_move_the_reader() {
    let mut reader = book();
    let screen = reader.state();

    reader.press("j");
    let line = reader.state().scroll;
    assert!(line > 0.0 && line < 100.0, "a line is a line: {line}");

    reader.press("k");
    assert_eq!(reader.state().scroll, screen.scroll);

    reader.press("d");
    let half = reader.state().scroll;
    reader.press("Home");
    reader.press(" ");
    let whole = reader.state().scroll;
    assert!(
        whole > half * 1.5,
        "a screen is more than half of one: {whole} against {half}"
    );

    reader.press("End");
    assert_eq!(reader.state().page, 400);
    reader.press("Home");
    assert_eq!(reader.state().scroll, 0.0);

    // A page at a time, by name.
    reader.press("n");
    assert_eq!(reader.state().page, 2);
    reader.press("n");
    assert_eq!(reader.state().page, 3);
    reader.press("p");
    assert_eq!(reader.state().page, 2);
}

#[test]
fn only_the_pages_near_the_viewport_are_in_the_document() {
    let mut reader = book();
    for _ in 0..12 {
        reader.wheel_screen();
    }
    let state = reader.state();
    assert!(
        state.mounted.len() <= 3,
        "a 400-page book holds a handful of pages: {state:?}"
    );
    assert!(
        state.mounted.contains(&state.page),
        "the page being read is one of them: {state:?}"
    );
    // Contiguous, in order — the mounting window is a band, not a set.
    for pair in state.mounted.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "{state:?}");
    }
}

#[test]
fn fit_and_zoom_say_what_they_did() {
    let mut reader = book();
    let wide = reader.harness.layout_rect(".page");
    assert!((wide.width - 1100.0).abs() < 1.0, "fit width fills it: {wide:?}");

    reader.press("9");
    assert_eq!(reader.state().zoom, "Fit page");
    let fitted = reader.harness.layout_rect(".page");
    let viewer = reader.harness.layout_rect(".viewer");
    assert!(
        fitted.height <= viewer.height + 0.5,
        "a fitted page is on the screen: {fitted:?} in {viewer:?}"
    );

    reader.press("+");
    let closer = reader.state();
    assert!(closer.zoom.ends_with('%'), "{closer:?}");
    assert_eq!(closer.notice, closer.zoom);
    let bigger = reader.harness.layout_rect(".page");
    reader.press("-");
    let smaller = reader.harness.layout_rect(".page");
    assert!(smaller.width < bigger.width, "{smaller:?} {bigger:?}");
}

#[test]
fn zooming_keeps_the_reader_where_they_were() {
    let mut reader = book();
    for _ in 0..5 {
        reader.wheel_screen();
    }
    let before = reader.state().page;
    reader.press("+");
    reader.press("+");
    assert_eq!(reader.state().page, before, "a zoom is not a page turn");
}

#[test]
fn the_toolbar_is_clickable() {
    let mut reader = book();
    // The chips, in the order they are in the toolbar: fit, out, in, theme.
    reader.click_nth(".chip", 2);
    assert!(reader.state().zoom.ends_with('%'));
    reader.click_nth(".chip", 0);
    assert_eq!(reader.state().zoom, "Fit width");
    reader.click_nth(".chip", 3);
    assert_eq!(reader.state().theme, "Hylo Dark");
}

#[test]
fn a_theme_is_named_where_it_is_changed() {
    let mut reader = book();
    assert_eq!(reader.state().theme, "Hylo Light");
    reader.press("t");
    let dark = reader.state();
    assert_eq!(dark.theme, "Hylo Dark");
    assert_eq!(dark.notice, "Hylo Dark", "the notice says what happened");
}

#[test]
fn spreads_put_two_pages_side_by_side() {
    let mut reader = book();
    reader.press("n");
    reader.press("s");
    let state = reader.state();
    assert!(
        state.mounted.len() >= 2,
        "a spread holds a pair: {state:?}"
    );
    let pages = reader.harness.query_all(".page");
    let rects: Vec<_> = pages
        .iter()
        .map(|node| reader.harness.layout_rect_of(*node))
        .collect();
    assert!(
        rects.iter().any(|rect| rect.x > 1.0),
        "one of them is to the right of the other: {rects:?}"
    );
}

#[test]
fn the_window_can_be_a_different_size() {
    let small = Reader::open_with(
        &Reader::book(),
        Options {
            width: 640,
            height: 480,
            ..Default::default()
        },
    );
    let page = small.harness.layout_rect(".page");
    assert!(
        (page.width - 640.0).abs() < 1.0,
        "fit width fits the window it is given: {page:?}"
    );
}
