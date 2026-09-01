//! The icon table against the app's, by reading `src/icons.ts`.
//!
//! `icons.rs` is a copy of the drawings in `src/icons.ts`, because the app's
//! file is TypeScript and cannot be mounted the way `theme.rs` and
//! `settings.rs` are — see `src/lib.rs`. A copy of a drawing is exactly the
//! kind of copy `AGENTS.md` warns about: it goes stale silently, because both
//! sides draw *something* and only one of them is looked at. This is the gate
//! that keeps it honest, and it is the same trick `settings.test.mjs` plays on
//! the settings table in the app.

use std::path::Path;

fn app_icons() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/icons.ts");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The `d`/`x`/`cx`… soup of one icon in `icons.ts`, by its key.
///
/// The table there is `name: '<path .../>'` with the value in single quotes and
/// a name that is either bare or quoted. Parsed rather than matched with a
/// regular expression over the whole file, because a run of icons is easier to
/// get wrong than to read.
fn drawing(source: &str, name: &str) -> Option<String> {
    let mut shapes = String::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let head = line.trim();
        let key = head.split(':').next()?.trim().trim_matches(|c| c == '"' || c == '\'');
        if key != name {
            continue;
        }
        // The value is either on this line or on the next — prettier breaks a
        // long one onto its own line, and about half of them are long.
        let rest = head.split_once(':')?.1.trim().to_string();
        let value = if rest.is_empty() { lines.next()?.trim().to_string() } else { rest };
        shapes.push_str(value.trim_end_matches(',').trim_matches('\''));
        return Some(shapes);
    }
    None
}

/// Every name this reader draws, and whether the app draws it the same way.
#[test]
fn every_icon_is_the_app_s_own_drawing() {
    let source = app_icons();
    // Named here rather than iterated, because `icons.rs` is a `match` and a
    // match cannot be enumerated. A name added there and not here is not a
    // drift — it is an icon nothing has asked for yet.
    let shared = [
        "contents", "pages", "search", "minus", "plus", "close", "document", "fitWidth", "mark",
        "window", "folder", "theme", "up", "down", "book", "sidebar", "keyboard", "info",
    ];
    for name in shared {
        let ours = dioxus_reader::icons::path(name)
            .unwrap_or_else(|| panic!("{name} is drawn here"));
        let theirs = drawing(&source, name)
            .unwrap_or_else(|| panic!("{name} is in the app's icons.ts"));
        assert_eq!(ours, theirs, "{name} has drifted from the app's drawing");
    }
}

/// And the one that is this reader's own, which the app has no button for.
#[test]
fn the_crop_icon_is_ours_and_the_app_has_no_such_name() {
    assert!(dioxus_reader::icons::path("crop").is_some());
    assert!(
        drawing(&app_icons(), "crop").is_none(),
        "the app has grown a crop icon — take ours out and copy theirs",
    );
}
