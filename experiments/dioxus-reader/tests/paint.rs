//! What actually lands on the screen.
//!
//! This is the half the app's own harness never had. `recolor.test.mjs` tests
//! the ramp against a reference and `reader.test.mjs` tests that the interface
//! moves; nothing on either side could say that a page was *drawn*, because a
//! canvas in a headless WebKit is a canvas nobody looks at. Here the whole
//! window is rasterised on the CPU and the pixels are read back.
//!
//! The assertions are about measurable properties rather than about a
//! reference PNG, and that is a decision rather than a shortcut — see
//! `PROGRESS.md`. A reference image is only as portable as the fonts that went
//! into it, and the toolbar is drawn in whatever `ui-sans-serif` resolves to
//! on the machine.

use dioxus_reader::harness::{Options, Reader};
use dioxus_reader::palette;
use dioxus_reader::theme;

/// Hylo Dark, as the app's own theme file defines it. The list is fourteen
/// long now and read off the app's `themes/` directory, so a test that wants
/// *the dark one* asks for it by id rather than by a place in an array.
fn hylo_dark() -> (usize, palette::Palette) {
    let index = theme::BUILT_IN
        .iter()
        .position(|(id, _)| *id == theme::DEFAULT_DARK)
        .expect("Hylo Dark ships");
    let parsed: theme::Theme =
        toml::from_str(theme::BUILT_IN[index].1).expect("Hylo Dark parses");
    (index, palette::resolve(&parsed, true))
}

/// The rectangle a page occupies, in device pixels, pulled in a little so
/// that a shadow or a rounding does not land in the sample.
fn page_rect(reader: &Reader) -> (u32, u32, u32, u32) {
    let rect = reader.harness.layout_rect(".page");
    (
        rect.x as u32 + 8,
        rect.y as u32 + 8,
        (rect.x + rect.width) as u32 - 8,
        (rect.y + rect.height) as u32 - 8,
    )
}

#[test]
fn a_page_is_drawn_where_the_layout_puts_it() {
    let mut reader = Reader::open(&Reader::book());
    let page = page_rect(&reader);
    let shot = reader.screenshot();

    // The paper is the page's own white, and it is not the ground the page
    // stands on. Two samples: one inside the page, one in the margin above it
    // — which is where the ground shows at fit width, the page having reached
    // both sides.
    let inside = shot.at(page.0 + 40, page.1 + 40);
    let above = shot.at(page.0 + 40, page.1 - 14);
    assert!(
        inside[0] > 240 && inside[1] > 240 && inside[2] > 240,
        "the page is paper: {inside:?}"
    );
    assert!(
        above[0] < 240,
        "and the ground it stands on is not: {above:?}"
    );
}

#[test]
fn there_is_ink_on_the_page() {
    let mut reader = Reader::open(&Reader::book());
    let page = page_rect(&reader);
    let shot = reader.screenshot();
    // The fixture is one line of text near the top of each page, so the band
    // that holds it is where to look — and the rest of the page is paper,
    // which is why this is a band and not the whole page.
    let band = (page.0, page.1 + 100, page.2, page.1 + 200);
    let ink = shot.unlike([255, 255, 255], band);
    assert!(
        ink > 0.01,
        "a page with text on it has ink: {:.4} of the band",
        ink
    );
}

#[test]
fn a_recolouring_theme_reaches_the_page() {
    let light = {
        let mut reader = Reader::open(&Reader::book());
        let page = page_rect(&reader);
        reader.screenshot().mean(page)
    };
    let dark = {
        let mut reader = Reader::open_with(
            &Reader::book(),
            Options {
                theme: Some(hylo_dark().0),
                ..Default::default()
            },
        );
        let page = page_rect(&reader);
        reader.screenshot().mean(page)
    };
    assert!(light[0] > 200.0, "a light page is paper: {light:?}");
    assert!(
        dark[0] < 80.0,
        "and a dark one is not — the recolouring is on the pixels, not on the CSS: {dark:?}"
    );
    assert!(hylo_dark().1.recolor, "…which is what Hylo Dark asks for");
}

#[test]
fn the_ink_survives_the_theme() {
    // The whole argument of the ramp is that a dark theme is not a blackout:
    // paper becomes ink and ink becomes paper, so the *contrast* in the band
    // that holds the text is preserved.
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            theme: Some(hylo_dark().0),
            ..Default::default()
        },
    );
    let page = page_rect(&reader);
    let shot = reader.screenshot();
    let band = (page.0, page.1 + 100, page.2, page.1 + 200);
    let paper = shot.mean((page.0, page.3 - 100, page.2, page.3));
    let ink = shot.unlike(
        [paper[0] as u8, paper[1] as u8, paper[2] as u8],
        band,
    );
    assert!(
        ink > 0.01,
        "the letters are still there, in the other colour: {ink:.4}"
    );
}

#[test]
fn the_theme_reaches_the_chrome_too() {
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            theme: Some(hylo_dark().0),
            ..Default::default()
        },
    );
    let shot = reader.screenshot();
    let bar = shot.mean((0, 0, 1100, 40));
    let paper = hylo_dark().1.background;
    for channel in 0..3 {
        assert!(
            (bar[channel] - paper[channel] as f64).abs() < 24.0,
            "the toolbar wears the theme's paper: {bar:?} against {paper:?}"
        );
    }
}

#[test]
fn scrolling_changes_what_is_drawn() {
    let mut reader = Reader::open(&Reader::book());
    let first = reader.screenshot();
    for _ in 0..4 {
        reader.wheel_screen();
    }
    let later = reader.screenshot();
    let mut different = 0u64;
    for at in (0..first.rgba.len()).step_by(4 * 37) {
        if first.rgba[at] != later.rgba[at] {
            different += 1;
        }
    }
    assert!(
        different > 100,
        "four screenfuls later the window is not the same picture: {different}"
    );
}

/* ---------------------------------------------------- a zoom, under a pinch */

/// The size a page's texture is drawn at, off the page's own attribute.
fn drawn_of(reader: &Reader) -> String {
    reader
        .harness
        .attr(".page", "data-drawn")
        .expect("a mounted page says what it is drawn at")
}

/// **A pinch is one gesture, not a hundred zoom steps.**
///
/// A trackpad sends a magnification event a frame. Each one changes the size
/// every page is laid out at, and a page whose size changed is a page whose
/// texture is registered afresh — which cannot be *drawn* until the frame
/// after it is registered, see `fresh` in `page.rs`. So the document went
/// blank for the whole of a pinch and came back when the fingers stopped,
/// which is what the reader saw and reported.
///
/// What holds it together is that the drawn size is frozen for the length of
/// the gesture and the page is stretched to whatever the layout asks for.
/// That is the pair this checks: the box grows, the drawn size does not.
#[test]
fn a_pinch_stretches_the_page_it_has_rather_than_drawing_a_new_one() {
    let mut reader = Reader::open_with(&Reader::book(), Options::default());
    let before_box = reader.harness.layout_rect(".page").width;
    let before_drawn = drawn_of(&reader);

    for _ in 0..6 {
        reader.pinch(0.05);
    }
    let after_box = reader.harness.layout_rect(".page").width;
    let after_drawn = drawn_of(&reader);

    assert!(
        after_box > before_box + 1.0,
        "the page grew under the fingers: {before_box} → {after_box}",
    );
    assert_eq!(
        after_drawn, before_drawn,
        "and it is the same texture, stretched, rather than six new ones",
    );
}

/// …and when the fingers stop, it is drawn again at the size it now is.
///
/// The gesture has no end in it — macOS sends magnification and says nothing
/// about the last one — so what ends it is the gap after it. See
/// `ZOOM_SETTLES` and `Viewer::settle_zoom`.
#[test]
fn and_when_the_fingers_stop_the_page_is_drawn_at_the_size_it_reached() {
    let mut reader = Reader::open_with(&Reader::book(), Options::default());
    for _ in 0..6 {
        reader.pinch(0.05);
    }
    let held = drawn_of(&reader);
    let grown = reader.harness.layout_rect(".page").width;

    let settled = reader.wait_until(3.0, |reader| drawn_of(reader) != held);
    assert!(settled, "the gesture settled and the page was drawn again");
    let (width, _) = drawn_of(&reader)
        .split_once('x')
        .map(|(w, h)| (w.to_string(), h.to_string()))
        .expect("the attribute is <width>x<height>");
    let width: f64 = width.parse().expect("a number");
    assert!(
        (width - grown as f64).abs() <= 1.0,
        "drawn at {width} for a box of {grown}",
    );
}
