//! What the window looks like when it is not being asked anything: where the
//! page sits in it, what colour it is before it has been drawn, and what
//! colour the toolbar's own labels are.
//!
//! Grievances from reading with it, and every one is the sort of thing a test
//! suite that asks "does it work" will never raise: a correct answer badly
//! placed, badly coloured, unreachable, or computed against a window that had
//! stopped being the window.

use dioxus_reader::harness::{Options, Reader};
use dioxus_reader::theme;

fn book() -> Reader {
    Reader::open(&Reader::book())
}

/// A shipped theme, by the id it is named by rather than by where it happens
/// to sit in the list.
fn shipped(id: &str) -> usize {
    theme::BUILT_IN
        .iter()
        .position(|(name, _)| *name == id)
        .unwrap_or_else(|| panic!("{id} ships"))
}

/// A reader wearing one, with the fixture open.
fn wearing(id: &str) -> Reader {
    Reader::open_with(
        &Reader::book(),
        Options {
            theme: Some(shipped(id)),
            ..Options::default()
        },
    )
}

/// How much ground there is either side of the page — the two numbers that
/// have to agree for a page to be centred.
fn margins(reader: &Reader) -> (f32, f32) {
    let page = reader.harness.layout_rect(".page");
    let viewer = reader.harness.layout_rect(".viewer");
    (
        page.x - viewer.x,
        (viewer.x + viewer.width) - (page.x + page.width),
    )
}

#[test]
fn a_page_narrower_than_the_window_stands_in_the_middle_of_it() {
    let mut reader = book();
    reader.press_chord("mod+2");
    let (left, right) = margins(&reader);
    assert!(left > 10.0, "there is ground either side of it: {left}");
    assert!((left - right).abs() <= 1.0, "{left} against {right}");
}

#[test]
fn a_page_wider_than_the_window_is_centred_and_can_be_reached() {
    let mut reader = book();
    reader.press_chord("mod+1");
    for _ in 0..4 {
        reader.press_chord("mod+=");
    }
    assert_eq!(reader.state().zoom, "200%");

    // Wider than the window, and hanging out of it by the same amount at
    // both ends. **It used to hang out of the right alone**, pinned twenty
    // pixels from the left with the rest of it off the screen and no way to
    // scroll there: `#viewer` in the app is `overflow: auto` and `#pages` is
    // `margin: 0 auto`, and Blitz has neither.
    let (left, right) = margins(&reader);
    assert!(left < -40.0, "the page is wider than the window: {left}");
    assert!((left - right).abs() <= 1.0, "{left} against {right}");

    // And the far edge can be brought into view.
    reader.wheel_across(200.0);
    let (panned_left, panned_right) = margins(&reader);
    assert!(panned_left < left - 100.0, "{left} -> {panned_left}");
    assert!(panned_right > right + 100.0, "{right} -> {panned_right}");

    // Zooming back out to something that fits puts it back in the middle
    // rather than leaving it where the pan left it.
    reader.press_chord("mod+2");
    let (left, right) = margins(&reader);
    assert!((left - right).abs() <= 1.0, "{left} against {right}");
}

#[test]
fn a_window_that_changes_size_lays_the_document_out_again() {
    // **The one fault behind two complaints**: the page not centred, and Fit
    // width fitting a width the window no longer had. Blitz answers
    // `SurfaceResized` by moving its own viewport and asking for a redraw, and
    // tells nobody — so the chrome followed the window and `Viewer::layout`
    // kept the viewport it was handed when the window was mounted. A window
    // opened at 1100 and dragged to 1600 laid its pages out for 1100 inside a
    // `.viewer` that was now 1600: the page centred in a `.pages` box narrower
    // than the window, which is a page against the left of the screen.
    //
    // See `Shell::on_resized` and the `window-resized` arm in `app.rs`, which
    // are the two halves of the wire this drives.
    let mut reader = book();
    reader.press_chord("mod+0");
    let (left, right) = margins(&reader);
    assert!((left - right).abs() <= 1.0, "{left} against {right}");

    reader.resize(1600, 1000);
    let viewer = reader.harness.layout_rect(".viewer");
    assert!(viewer.width > 1500.0, "the window is wider: {}", viewer.width);
    let page = reader.harness.layout_rect(".page");
    assert!(
        (page.width - viewer.width).abs() <= 1.0,
        "fit width fits the width it has now: page {} in {}",
        page.width,
        viewer.width,
    );

    // And a mode with something to centre is centred in the window it has.
    reader.press_chord("mod+2");
    let (left, right) = margins(&reader);
    assert!(left > 10.0, "there is ground either side of it: {left}");
    assert!((left - right).abs() <= 1.0, "{left} against {right}");
}

#[test]
fn the_document_is_centred_beside_an_open_panel() {
    // One pixel, and the same fault as the resize above in miniature: the
    // panel's hairline is a border, a content box put it outside the width the
    // panel was given, and the document was laid out for a viewport a pixel
    // wider than the box it was drawn into. Every page came out flush against
    // the panel with its far edge a pixel over the window.
    let mut reader = book();
    reader.press_chord("mod+b");
    reader.press_chord("mod+2");
    let (left, right) = margins(&reader);
    assert!((left - right).abs() <= 1.0, "{left} against {right}");

    let panel = reader.harness.layout_rect(".sidebar");
    let viewer = reader.harness.layout_rect(".viewer");
    assert!(
        (panel.x + panel.width - viewer.x).abs() <= 0.5,
        "the panel ends where the document starts: {} against {}",
        panel.x + panel.width,
        viewer.x,
    );
}

#[test]
fn a_menu_comes_down_under_the_button_that_opened_it() {
    // They were one layer pinned to the ends of the bar, so the View menu —
    // whose button sits between Trim and the theme — came down under the page
    // field, three chips to the right of what had been clicked. Each is in an
    // anchor of its own now; see `.anchor` in `styles.rs`.
    let mut reader = book();
    reader.click(".chip.fit");
    let menu = reader.harness.layout_rect(".menu.view");
    let chip = reader.harness.layout_rect(".chip.fit");
    assert!(
        (menu.x - chip.x).abs() <= 1.0,
        "the View menu is under its own button: {} against {}",
        menu.x,
        chip.x,
    );
    assert!(menu.y > chip.y, "and below it");

    let mut reader = book();
    reader.click(".chip.title");
    let menu = reader.harness.layout_rect(".menu.document");
    let chip = reader.harness.layout_rect(".chip.title");
    assert!((menu.x - chip.x).abs() <= 1.0, "{} against {}", menu.x, chip.x);

    // The theme menu is the one aligned by its right edge, because it is wider
    // than its button and near the end of the bar.
    let mut reader = book();
    reader.click(".chip.theme");
    let menu = reader.harness.layout_rect(".menu.theme");
    let chip = reader.harness.layout_rect(".chip.theme");
    let (menu_end, chip_end) = (menu.x + menu.width, chip.x + chip.width);
    assert!(
        (menu_end - chip_end).abs() <= 1.0,
        "the Theme menu ends where its button does: {menu_end} against {chip_end}",
    );
}

#[test]
fn an_undrawn_page_is_the_theme_s_paper_and_not_white() {
    // Hylo Ember: a recolouring theme, so a page under it is drawn on the
    // theme's own paper and a page that has not been drawn yet must be too.
    // A white rectangle on a dark theme is the flash a reader sees on every
    // zoom step and every jump — a re-keyed page is a new node with no
    // texture, and until pdfium answers, this is what is on screen.
    let reader = wearing("hylo-dark");
    let style = reader.harness.attr(".root", "style").unwrap_or_default();
    let paper = paper_of(&style);
    let page = value_of(&style, "--page");
    assert_eq!(page, paper, "the page is the theme's paper: {style}");
    assert_ne!(page, "#ffffff");
}

#[test]
fn a_page_no_theme_is_recolouring_is_white() {
    // Hylo Light does not recolour, so the paper on screen is the paper the
    // printer used, whatever the chrome around it is.
    let reader = wearing(theme::DEFAULT_LIGHT);
    let style = reader.harness.attr(".root", "style").unwrap_or_default();
    assert_eq!(value_of(&style, "--page"), "#ffffff");
}

#[test]
fn the_toolbar_wears_the_theme_rather_than_a_grey() {
    // Mark, Trim, the zoom and the two steppers are all `--muted`, and it was
    // mixed halfway between the paper and the ink — which is a mid-grey
    // whatever the two ends are, so all fourteen themes put very nearly the
    // same colour in the bar. What is asserted is the distance from the
    // theme's own ink: near it, and much nearer than the halfway shade was.
    for id in ["hylo-light", "hylo-dark", "hylo-ember", "sepia", "nord"] {
        let reader = wearing(id);
        let style = reader.harness.attr(".root", "style").unwrap_or_default();
        let ink = rgb(&value_of(&style, "--text"));
        let paper = rgb(&value_of(&style, "--paper"));
        let muted = rgb(&value_of(&style, "--muted"));
        let span = distance(ink, paper);
        let off = distance(ink, muted);
        assert!(
            off < span * 0.35,
            "{id}: the bar is the theme's ink and not a grey — {off} of {span}",
        );
    }
}

#[test]
fn the_toolbar_carries_the_app_s_icons_in_the_theme_s_shades() {
    // Every button in the app's `index.html` has a `data-icon` and none here
    // did, which is the other half of a bar that read as grey words. The
    // colour is asserted because it is the part that has no cascade behind it:
    // an inline `<svg>` reaches usvg as its own document — see `Icon` — so a
    // `currentColor` icon comes out black on every theme, which on Hylo Dark
    // is invisible.
    let mut reader = wearing("hylo-dark");
    let style = reader.harness.attr(".root", "style").unwrap_or_default();
    let muted = value_of(&style, "--muted");
    let accent = value_of(&style, "--accent");

    assert_eq!(
        reader.harness.attr(".chip.contents .icon", "stroke").as_deref(),
        Some(muted.as_str()),
        "an idle chip's icon is the quiet shade",
    );

    // And a chip whose thing is in force takes the accent, icon and all. This
    // was the Mark chip until the bar stopped having one — a mark is set once
    // and read from the Contents panel, so a permanent button for it was a
    // permanent button for something nobody presses twice in an hour, and the
    // app has never had one. Contents is the same shape: a chip with an icon,
    // a word, and an on state.
    reader.press_chord("mod+b");
    assert_eq!(
        reader.harness.attr(".chip.contents .icon", "stroke").as_deref(),
        Some(accent.as_str()),
    );

    // The panel's tabs too, which are the other place a label stands alone.
    assert!(reader.harness.query(".tab .icon").is_some(), "the tabs carry them");
}

/* ----------------------------------------------------------- reading it */

fn value_of(style: &str, name: &str) -> String {
    style
        .split(';')
        .find_map(|piece| {
            let (key, value) = piece.split_once(':')?;
            (key.trim() == name).then(|| value.trim().to_string())
        })
        .unwrap_or_else(|| panic!("no {name} in {style}"))
}

fn paper_of(style: &str) -> String {
    value_of(style, "--paper")
}

fn rgb(hex: &str) -> [f64; 3] {
    let hex = hex.trim_start_matches('#');
    [0, 2, 4].map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap() as f64)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/* ------------------------------------------------------- the page field */

/// Whether the field is up rather than the readout it replaces.
fn typing(reader: &Reader) -> bool {
    reader.harness.query(".page-field").is_some()
}

#[test]
fn the_page_field_opens_holding_the_page_it_is_on() {
    let mut reader = book();
    reader.press("p");
    reader.type_text("37");
    reader.press("Enter");
    assert_eq!(reader.state().page, 37);

    // **And it opens holding it**, rather than empty. The app selects the
    // field's contents (`el.pageNumber.select()`); parley will do that only
    // when a keystroke asks it to and there is no imperative door onto it, so
    // the selection is emulated — the number is there, and the first thing
    // typed replaces all of it.
    reader.press("p");
    assert!(typing(&reader));
    assert_eq!(reader.state().label, "37");

    reader.press("9");
    assert_eq!(reader.state().label, "9");
    // And the second digit lands *after* the first. It did not: writing the
    // character in through the value attribute replaced the editor's string
    // and left the caret at the front, so "50" was typed as "05" — which
    // parses to page 5 and passes every test written in one digit.
    reader.press("2");
    assert_eq!(reader.state().label, "92");
    reader.press("Enter");
    assert_eq!(reader.state().page, 92);
}

#[test]
fn the_page_field_shows_that_all_of_it_is_selected() {
    // The emulated select-all was invisible: the field opened looking like a
    // field somebody had clicked into, and the first digit replacing the whole
    // number came as a surprise. `.page-field.fresh` is the theme's own
    // selection colours — the pair a swept passage on the page is drawn in.
    let mut reader = book();
    reader.press("p");
    let class = reader.harness.attr(".page-field", "class").unwrap_or_default();
    assert!(class.contains("fresh"), "opened selected: {class}");

    // And typing ends it, because from then on there is a caret and a number
    // being built rather than a value standing in for a selection.
    reader.press("9");
    let class = reader.harness.attr(".page-field", "class").unwrap_or_default();
    assert!(!class.contains("fresh"), "typed into: {class}");
}

#[test]
fn the_page_box_is_the_width_of_the_number_in_it() {
    // Blitz gives parley no alignment for a text input's own text and calls
    // `set_width(None)`, so `text-align: center` on one does nothing at all —
    // which left the page number pinned against the left wall of a box wide
    // enough for four digits. Centring is not available; a box that fits is,
    // and it is the better answer. See the comment on `.pill` in `app.rs`.
    let mut reader = book();
    let one = reader.harness.layout_rect(".page-now").width;
    reader.press("p");
    reader.type_text("250");
    let three = reader.harness.layout_rect(".page-field").width;
    assert!(three > one + 8.0, "three digits is wider than one: {three} against {one}");

    // And the readout the field replaces is the same width, so opening it
    // moves nothing else in the bar.
    reader.press("Enter");
    let now = reader.harness.layout_rect(".page-now").width;
    assert!((now - three).abs() <= 1.0, "{now} against {three}");
}

#[test]
fn a_chip_in_force_stands_on_the_accent_rather_than_wearing_it() {
    // Every theme in this app names a near-monochrome text colour, so a bar
    // written in a shade of it is grey whatever theme is on — and the only
    // colour that ever appeared was the accent, arriving as one bright word
    // among the grey with nothing under it. The tint is what carries the
    // theme. `--accent-soft` is a fifth of the way from the paper to the
    // accent: plainly the accent, and still somewhere a word can be read.
    for id in ["hylo-light", "hylo-ember", "dracula"] {
        let reader = wearing(id);
        let style = reader.harness.attr(".root", "style").unwrap_or_default();
        let paper = rgb(&value_of(&style, "--paper"));
        let accent = rgb(&value_of(&style, "--accent"));
        let soft = rgb(&value_of(&style, "--accent-soft"));
        let span = distance(paper, accent);
        assert!(
            distance(paper, soft) > span * 0.1 && distance(accent, soft) > span * 0.5,
            "{id}: a tint of the accent, not the accent — {soft:?}",
        );
    }
}

#[test]
fn backspace_on_a_field_nobody_has_typed_into_empties_it() {
    let mut reader = book();
    reader.press("p");
    assert_eq!(reader.state().label, "1");
    reader.press("Backspace");
    assert_eq!(reader.state().label, "");
}

#[test]
fn enter_on_a_field_nobody_has_typed_into_is_never_mind() {
    let mut reader = book();
    reader.press("p");
    reader.type_text("40");
    reader.press("Enter");
    assert_eq!(reader.state().page, 40);

    // Opening the field and pressing Enter goes nowhere — and, in
    // particular, does not put an entry in the history for a jump that never
    // happened, which stepping back proves.
    reader.press("p");
    reader.press("Enter");
    assert!(!typing(&reader));
    assert_eq!(reader.state().page, 40);
    reader.press_chord("mod+[");
    assert_eq!(reader.state().page, 1);
}

#[test]
fn a_press_anywhere_else_puts_the_page_field_away() {
    let mut reader = book();
    reader.press("p");
    reader.type_text("250");
    assert!(typing(&reader));

    // Clicking the document abandons it and puts the current page back,
    // which is the field's own `blur` handler in `main.ts`. Nothing else
    // took the field down: it held the keyboard until Escape or Enter, and a
    // reader who had clicked away from it was typing into a field they were
    // no longer looking at.
    let (x, y) = reader.point_on(1, (0.5, 0.5));
    reader.click_at(x, y);
    assert!(!typing(&reader));
    assert_eq!(reader.state().label, "1");
    assert_eq!(reader.state().page, 1);
}

/// And the page field, for the same reason: Backspace is a command rather
/// than a key on a Mac. See `a_query_can_be_corrected_as_well_as_typed` in
/// `tests/search.rs`, which is the same fault in the other field.
#[test]
fn a_typed_page_can_be_corrected() {
    let mut reader = Reader::open(&Reader::book());
    reader.press_chord("mod+alt+g");
    reader.type_text("123");
    assert_eq!(
        reader.harness.attr(".page-field", "value").as_deref(),
        Some("123"),
    );
    reader.press("Backspace");
    assert_eq!(
        reader.harness.attr(".page-field", "value").as_deref(),
        Some("12"),
        "a digit typed by mistake can be taken back",
    );
}
