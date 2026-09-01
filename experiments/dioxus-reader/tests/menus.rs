//! The toolbar's menus, and the door they opened.
//!
//! Two of the four things a reader looking at this experiment beside the app
//! notices first: there are no dropdown menus, and there is no way to open
//! another document. They are one piece of work — the picker is a menu item
//! before it is anything else — and this is what says so.
//!
//! What is *not* here is anything about the picker itself. `Pick` is a
//! context holding one closure for the reason `Clip` is one: the real answer
//! is a modal window belonging to the operating system, and a suite that
//! opened one would sit there until somebody clicked it. So the harness
//! answers with a path and what is tested is everything downstream of the
//! answer.

use dioxus_reader::app::Ask;
use dioxus_reader::fixture;
use dioxus_reader::harness::{Options, Reader};

fn reader() -> Reader {
    Reader::open(&fixture::contents_pdf())
}

/// One at a time, and every way out of one.
#[test]
fn a_menu_opens_and_closes() {
    let mut reader = reader();
    assert_eq!(reader.state().menu, None, "nothing is down to start with");

    reader.click(".chip.theme");
    assert_eq!(reader.state().menu.as_deref(), Some("theme"));

    // A second menu replaces the first rather than joining it. Nothing inside
    // one menu knows the others exist, which is why the state is the
    // reader's — see `app::Menu`.
    reader.click(".chip.fit");
    assert_eq!(reader.state().menu.as_deref(), Some("view"));

    // Its own button closes it, which is the gesture that would have been
    // broken by dismissing on every press: the press would close it on the
    // way down and the click would open it straight back up.
    reader.click(".chip.fit");
    assert_eq!(reader.state().menu, None);

    reader.click(".chip.theme");
    reader.press("Escape");
    assert_eq!(reader.state().menu, None, "Escape closes the menu");

    // And a press anywhere the menu is not. The document is the ordinary
    // "anywhere" and is also the one place a press does something else, so it
    // is the one worth pressing.
    reader.click(".chip.theme");
    reader.click(".viewer");
    assert_eq!(reader.state().menu, None, "a press elsewhere closes it");
}

/// Escape closes the menu *first*, ahead of everything else it is the way out
/// of. A reader with the find bar open and a menu down means the menu.
#[test]
fn escape_takes_the_menu_before_the_find_bar() {
    let mut reader = reader();
    reader.press_chord("mod+f");
    assert!(reader.state().find.is_some());
    reader.click(".chip.theme");

    reader.press("Escape");
    assert_eq!(reader.state().menu, None);
    assert!(reader.state().find.is_some(), "the bar is still up");

    reader.press("Escape");
    assert!(reader.state().find.is_none(), "and now it is not");
}

/// Fourteen themes reached by pressing `t` fourteen times is what a menu is
/// for. The list is every theme installed, the one in use is ticked, and
/// choosing one wears it.
#[test]
fn the_theme_menu_is_the_whole_list() {
    let mut reader = reader();
    reader.click(".chip.theme");
    let names = reader.harness.query_all(".menu.theme .menu-item").len();
    assert_eq!(names, 14, "every shipped theme is in the menu");
    assert_eq!(
        reader.harness.query_all(".menu.theme .menu-item.on").len(),
        1,
        "exactly one is ticked",
    );

    reader.click_nth(".menu.theme .menu-item", 2);
    assert_eq!(reader.state().theme, "Hylo Ember");
    assert_eq!(reader.state().menu, None);
}

/// The View menu holds what the fit chip could not say and the `s` key was
/// standing in for: three fits, three spreads, and the two rotations.
#[test]
fn the_view_menu_chooses_a_spread() {
    let mut reader = reader();
    let one = reader.state().mounted.len();
    reader.click(".chip.fit");
    // Fit width, fit page, actual size, then the three spreads.
    reader.click_nth(".menu.view .menu-item", 4);
    assert_eq!(reader.state().menu, None);
    assert!(
        reader.state().mounted.len() > one,
        "two pages side by side mount more than one did",
    );
}

/// **A menu item says which key asks for the same thing, and it reads that
/// off the keymap.** A hand-written chord beside a menu item cannot show a
/// rebound one, which is the drift the app's Keyboard page was rewritten to
/// stop — see `Viewer::chord_for`.
#[test]
fn a_menu_item_shows_the_key_that_is_actually_bound() {
    let mut reader = Reader::open_with(
        &fixture::contents_pdf(),
        Options {
            keys: [("open".to_string(), vec!["mod+shift+o".to_string()])]
                .into_iter()
                .collect(),
            ..Options::default()
        },
    );
    reader.click(".chip.title");
    // The first key in the menu is Open's, which is the item that was
    // rebound.
    let shown = reader.harness.text_content(".menu.document .menu-key");
    assert!(
        shown.contains('O') && (shown.contains('⇧') || shown.contains("Shift")),
        "the rebound chord is what the menu shows, not the shipped one: {shown:?}",
    );
}

/// ⌘O, and the whole of what opening a document in this window means.
#[test]
fn open_puts_a_different_document_in_this_window() {
    let second = fixture::titled_pdf("The Second One");
    let mut reader = Reader::open_with(
        &fixture::contents_pdf(),
        Options {
            picks: vec![second.clone()],
            ..Options::default()
        },
    );
    let before = reader.state();
    assert_eq!(before.pages, 12);

    reader.press_chord("mod+o");
    let after = reader.state();
    assert_eq!(after.pages, 3, "the new document's pages");
    assert_eq!(after.title, "The Second One", "and its name");
    assert_eq!(after.page, 1, "opened at the front");

    // What the process has to be told, and the only route to it: the desk and
    // the watch are neither of them the window's. See `Ask::Showing`.
    assert!(
        reader.asks().iter().any(|ask| matches!(
            ask,
            Ask::Showing { path, title } if path == &second && title == "The Second One"
        )),
        "the window said what it is showing now: {:?}",
        reader.asks(),
    );
}

/// A picker the reader closed is not a document, and nothing moves.
#[test]
fn a_cancelled_picker_changes_nothing() {
    let mut reader = reader();
    let before = reader.state();
    reader.press_chord("mod+o");
    assert_eq!(reader.state().pages, before.pages);
    assert_eq!(reader.state().title, before.title);
    assert!(
        !reader
            .asks()
            .iter()
            .any(|ask| matches!(ask, Ask::Showing { .. })),
        "nothing was said about a document that was never chosen",
    );
}

/// The same picker, the other door: a window of its own, and this one left
/// exactly as it was. This is the app's "Open document in new window…", which
/// is a menu item there and not a key — so it is a menu item here too.
#[test]
fn open_in_a_new_window_leaves_this_one_alone() {
    let second = fixture::titled_pdf("Beside It");
    let mut reader = Reader::open_with(
        &fixture::contents_pdf(),
        Options {
            picks: vec![second.clone()],
            ..Options::default()
        },
    );
    let before = reader.state();

    reader.click(".chip.title");
    reader.click_nth(".menu.document .menu-item", 1);

    assert_eq!(reader.state().pages, before.pages, "this window is untouched");
    assert_eq!(reader.state().title, before.title);
    assert_eq!(
        reader.asks().last(),
        Some(&Ask::NewWindowOn(second)),
        "and a window was asked for on the other document",
    );
}

/// Opening the document that is already here is not an open at all.
#[test]
fn opening_what_is_already_open_says_so() {
    let here = fixture::contents_pdf();
    let mut reader = Reader::open_with(
        &here,
        Options {
            picks: vec![here.clone()],
            ..Options::default()
        },
    );
    reader.press_chord("mod+o");
    assert!(
        reader.state().notice.contains("already open"),
        "said so: {:?}",
        reader.state().notice,
    );
    assert!(
        !reader
            .asks()
            .iter()
            .any(|ask| matches!(ask, Ask::Showing { .. })),
        "and told nobody anything had changed",
    );
}
