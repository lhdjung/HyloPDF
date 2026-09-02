//! **The port, judged against the app rather than against somebody's memory
//! of it.**
//!
//! Every interface fault the reader reported was found by using the two side
//! by side, which is a slow way to find one fault at a time. This file is the
//! fast way to find all of them: `tests/parity/app-inventory.json` is taken
//! from the *running Tauri app*, in WebKit, through `scripts/ui-harness.mjs` —
//! what each toolbar group holds, what each menu lists in what order, what the
//! sidebar's tabs are called, and what all twenty-two of `applyTheme`'s
//! custom properties resolve to. The assertions below read that file and ask
//! the port the same questions.
//!
//! Regenerate the fixture, with `npm run dev` running:
//!
//! ```text
//! node experiments/dioxus-reader/tests/parity/take-inventory.mjs
//! ```
//!
//! It is committed, so a change to the app's interface shows up here as a
//! failure the next time somebody takes it — which is the point. What this
//! cannot cover is anything about the *window* (dragging it, full screen, the
//! traffic lights) and anything the app draws with a browser widget the
//! renderer has no counterpart for; those are named where they occur.

use dioxus_reader::harness::{Options, Reader};
use serde_json::Value;

fn app() -> Value {
    let raw = include_str!("parity/app-inventory.json");
    serde_json::from_str(raw).expect("the app's inventory")
}

fn reader() -> Reader {
    // The size the fixture was taken at, because a bar that has run out of
    // room drops things out of it.
    Reader::open_with(
        &Reader::book(),
        Options {
            width: 1280,
            height: 860,
            ..Default::default()
        },
    )
}

/// Labels out of a list of inventory rows, skipping what has no words.
fn labels(rows: &Value) -> Vec<String> {
    rows.as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row.get("label").and_then(Value::as_str))
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect()
}

/// **The toolbar holds what the app's holds, in the app's order.**
///
/// By the words on each control, because that is what a reader sees. The
/// icon-only ones — the page arrows, the two ends of the zoom group — have no
/// words and are counted by the group's length instead.
#[test]
fn the_toolbar_says_what_the_app_s_says() {
    let reader = reader();
    let app = app();
    for (group, selector) in [
        ("bar-left", ".bar-left"),
        ("bar-center", ".bar-center"),
        ("bar-right", ".bar-right"),
    ] {
        let want = labels(&app["toolbar"][group]);
        let got: Vec<String> = reader
            .text_all(&format!("{selector} button, {selector} .of"))
            .into_iter()
            .filter(|label| !label.is_empty())
            .collect();
        assert_eq!(got, want, "{group}");
    }
}

/// **And it is the same size as the app's, control for control.**
///
/// This is the one measurement in the file, and it is here because it is the
/// only question that can see the *type*. A chip is its padding plus its icon
/// plus its word; the padding and the icon are numbers both readers already
/// agree on, so what is left over is the word, and the word is the font. The
/// port's every chip came out about five per cent narrow — `Contents` 96
/// against 101.3, `of 400` 38 against 41.5 — because parley does not read SF's
/// `trak` table and WebKit does, so the system font arrived with its
/// small-size tracking missing. Nothing that compares *labels* could have said
/// so, and what a reader saw was a bar that was tighter and darker than the
/// app's without being able to name what had changed. See `body` in
/// `styles.rs`.
///
/// Two pixels of tolerance, and it has to be that loose: the app's numbers
/// are fractional and Blitz rounds a box to whole pixels, so 101.3 can only
/// ever come back as 101.
#[test]
fn the_toolbar_is_the_size_of_the_app_s() {
    let reader = reader();
    let app = app();
    // The middle group, whole. The two sides hold the same controls at the
    // same widths — which is what is asserted below — but where they *end* is
    // the flex arrangement, and that is deliberately not the app's: see
    // `.bar-left` in `styles.rs` on why the bases are nought here and `auto`
    // there.
    let want = app["toolbar"]["bar-center-box"]["width"]
        .as_f64()
        .expect("the middle group's width");
    let got = reader.width_of(".bar-center").expect("the middle group");
    assert!(
        (got - want).abs() <= 2.0,
        "the middle group is {got} wide and the app's is {want}",
    );

    // Every control that carries a word, by the class this reader gives it.
    // The icon-only ones are left out on purpose: a square is a square in
    // both readers and says nothing about the type.
    for (id, selector) in [
        ("contents", ".chip.contents"),
        ("close-doc", ".chip.close-doc"),
        ("doc-title", ".chip.title"),
        ("page-count", ".of"),
        ("find", ".chip.find"),
        ("rotate-left", ".chip.rotate-left"),
        ("rotate-right", ".chip.rotate-right"),
        ("zoom-level", ".chip.fit"),
    ] {
        let want = app["toolbar"]
            .as_object()
            .expect("the toolbar")
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|row| row.get("width"))
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("no {id} in the inventory"));
        let got = reader
            .width_of(selector)
            .unwrap_or_else(|| panic!("no {selector}"));
        assert!(
            (got - want).abs() <= 2.0,
            "{id} is {got} wide and the app's is {want}"
        );
    }
}

/// Every menu the toolbar opens, item for item and rule for rule.
///
/// The app's own five: Open, the document's name, the zoom readout, Theme and
/// the cog. A rule is a row in this comparison because where the rules fall is
/// half of what a menu reads like.
#[test]
fn every_menu_lists_what_the_app_s_lists() {
    let mut reader = reader();
    let app = app();
    for (chip, menu, id) in [
        (".chip.open", "open", "open"),
        (".chip.title", "document", "doc-title"),
        (".chip.fit", "view", "zoom-level"),
        (".chip.theme", "theme", "theme"),
        (".chip.settings", "settings", "settings"),
    ] {
        // The app writes a rule as a row of its own and this reader writes it
        // as an empty `.menu-rule`, so both sides are read as "label, or
        // nothing" and compared in order.
        let want: Vec<String> = app["menus"][id]
            .as_array()
            .unwrap_or_else(|| panic!("no {id} menu in the inventory"))
            .iter()
            .map(|row| {
                row.get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    // The app puts the shortcut inside the row it belongs to;
                    // this reader puts it in a span of its own. Compare the
                    // words, not the chord — `tests/keys.rs` is where chords
                    // are checked, against `keys.ts`.
                    .split('⌘')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .collect();
        reader.click(chip);
        let got = reader.text_all(&format!(
            ".menu.{menu} .menu-label, .menu.{menu} .menu-row-label, \
             .menu.{menu} .menu-section, .menu.{menu} .menu-rule"
        ));
        assert_eq!(got, want, "the {menu} menu");
        reader.press("Escape");
    }
}

/// The sidebar's tabs. Results is hidden until something has been searched
/// for, in both, which is why there are two here and not three.
#[test]
fn the_sidebar_has_the_app_s_tabs() {
    let mut reader = reader();
    let want = labels(&app()["sidebar"]);
    reader.click(".chip.contents");
    assert_eq!(reader.text_all(".tab"), want);
}

/// **Every colour the chrome is built from, against the app's own.**
///
/// `applyTheme` writes twenty-two custom properties onto the root and this
/// reader writes the same values under names of its own; the pairs are below.
/// They were all near-misses of the app's arithmetic — a surface pulled 6%
/// towards the ink where the app pulls it 55% towards white — which is the
/// kind of difference nobody can name from a screenshot and every one of
/// these catches.
#[test]
fn the_theme_resolves_to_the_app_s_colours() {
    let reader = reader();
    let app = app();
    let style = reader.harness.attr(".root", "style").unwrap_or_default();
    let of = |name: &str| {
        style
            .split(';')
            .filter_map(|entry| entry.split_once(':'))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim().to_string())
            .unwrap_or_else(|| panic!("no {name} on the root"))
    };
    for (theirs, ours) in [
        ("--bg", "--ground"),
        ("--surface", "--surface"),
        ("--surface-hover", "--hover"),
        ("--surface-sunk", "--sunk"),
        ("--line", "--line"),
        ("--text", "--text"),
        ("--text-soft", "--muted"),
        ("--text-note", "--note"),
        ("--text-faint", "--faint"),
        ("--accent", "--accent"),
        ("--accent-soft", "--accent-soft"),
        ("--accent-contrast", "--accent-contrast"),
        ("--positive", "--positive"),
        ("--negative", "--negative"),
        ("--negative-contrast", "--negative-contrast"),
        ("--page-paper", "--page"),
        ("--bar-hover", "--bar-hover"),
        ("--bar-sunk", "--bar-sunk"),
        ("--bar-line", "--bar-line"),
        ("--bar-accent", "--bar-accent"),
        ("--selection-area", "--found"),
        ("--selection-text", "--found-ink"),
    ] {
        let want = app["theme"][theirs].as_str().expect(theirs);
        assert_eq!(of(ours), want, "{ours} against the app's {theirs}");
    }
}

/// The Settings window: the pages it has, the fields on each in order, the
/// headings between them, and the buttons at the foot.
///
/// This is the largest surface in the app and the least looked at, which is
/// exactly why it is here: the theme editor was missing entirely, "Recolour
/// pictures too" was on the wrong page, and Full screen and Presenting were a
/// sentence rather than two switches.
#[test]
fn the_settings_window_has_the_app_s_pages() {
    let mut reader = reader();
    let app = app();
    let settings = &app["settings"];

    reader.press_chord("mod+,");
    let nav: Vec<String> = settings["nav"]
        .as_array()
        .expect("nav")
        .iter()
        .map(|name| name.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(reader.text_all(".nav-item"), nav, "the pages themselves");

    for (at, page) in nav.iter().enumerate() {
        reader.click_nth(".nav-item", at);
        let want = &settings["pages"][page];
        let list = |key: &str| -> Vec<String> {
            want[key]
                .as_array()
                .unwrap_or_else(|| panic!("{page} has no {key}"))
                .iter()
                .map(|row| row.as_str().unwrap_or_default().to_string())
                .collect()
        };
        assert_eq!(
            reader.text_all(".field-label"),
            list("fields"),
            "{page}: fields"
        );
        assert_eq!(
            reader.text_all(".pane-group"),
            list("groups"),
            "{page}: headings"
        );
        assert_eq!(
            reader.text_all(".pane-actions button"),
            list("actions"),
            "{page}: buttons",
        );
    }
}

/// The find bar's three switches, which are the only words in it — everything
/// else there is an icon.
#[test]
fn the_find_bar_offers_the_app_s_three_switches() {
    let mut reader = reader();
    let want = labels(&app()["find"]["rows"]);
    reader.press_chord("mod+f");
    assert_eq!(reader.text_all(".find-option"), want);
}
