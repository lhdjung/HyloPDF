//! The find bar, through the interface.
//!
//! `search.rs`'s own tests are about folding and locating and need no
//! document; these are about the other half — that a keystroke opens the bar,
//! that what is typed into it reaches the scan, that the reader is taken to
//! the match, that the rectangles land on the page, and that the three
//! switches do what they say. Everything is read off the interface the way
//! somebody looking at the screen would read it, which is the rule
//! `PROGRESS.md` sets for `state()`.

use dioxus_reader::fixture;
use dioxus_reader::harness::{Options, Reader};

/// A reader over the six pages of prose, with the find bar already up.
fn searching() -> Reader {
    let mut reader = Reader::open_with(&fixture::prose_pdf(), Options::default());
    reader.press_chord("mod+f");
    reader
}

/// Type a query and let the scan finish.
fn look_for(reader: &mut Reader, query: &str) {
    reader.type_text(query);
    reader.scan_out();
}

#[test]
fn the_find_bar_opens_on_the_shortcut_and_closes_on_escape() {
    let mut reader = Reader::open_with(&fixture::prose_pdf(), Options::default());
    assert_eq!(reader.state().find, None, "the bar is not up to begin with");
    reader.press_chord("mod+f");
    assert_eq!(
        reader.state().find,
        Some(String::new()),
        "up, and saying nothing until there is something to say",
    );
    reader.press("Escape");
    assert_eq!(reader.state().find, None);
}

#[test]
fn what_is_typed_is_searched_for_and_counted() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    let state = reader.state();
    assert_eq!(state.query, "needle");
    // Three: one on page 1 and two on page 3.
    assert_eq!(state.find.as_deref(), Some("1 of 3"));
}

/// The reader is taken to the first match, and the page it lands on is the
/// page the match is on.
#[test]
fn the_first_match_brings_the_reader_to_it() {
    let mut reader = searching();
    // Start on page 3, so that "first" cannot mean "the top of the document".
    look_for(&mut reader, "beside");
    assert_eq!(reader.state().page, 3);
    assert_eq!(reader.state().find.as_deref(), Some("1 of 1"));
}

/// And the scan starts at the page being read, so the match it settles on is
/// the one under the reader's eyes rather than the first in the book.
#[test]
fn the_scan_starts_where_the_reader_is() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    // From the top, the first of three is on page 1.
    assert_eq!(reader.state().page, 1);
    reader.press("Escape");
    // From page 3 it is the one on page 3 — the same three matches, a
    // different one to settle on, because the scan starts under the reader's
    // eyes rather than at the front of the book.
    reader.press("l");
    reader.press("l");
    assert_eq!(reader.state().page, 3);
    reader.press_chord("mod+f");
    look_for(&mut reader, "needle");
    assert_eq!(reader.state().find.as_deref(), Some("2 of 3"));
    assert_eq!(reader.state().page, 3);
}

#[test]
fn stepping_walks_the_matches_and_wraps() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    assert_eq!(reader.state().find.as_deref(), Some("1 of 3"));
    reader.press_chord("mod+g");
    assert_eq!(reader.state().find.as_deref(), Some("2 of 3"));
    reader.press_chord("mod+g");
    assert_eq!(reader.state().find.as_deref(), Some("3 of 3"));
    reader.press_chord("mod+g");
    assert_eq!(reader.state().find.as_deref(), Some("1 of 3"), "and round");
    reader.press_chord("mod+shift+g");
    assert_eq!(reader.state().find.as_deref(), Some("3 of 3"));
}

/// A match is painted where it is, and "Highlight all" is the only thing that
/// changes how many of them are.
#[test]
fn matches_are_painted_on_the_page_and_the_switch_says_how_many() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    // Page 3 has two of them, and it is one page: go there.
    reader.press_chord("mod+g");
    let all = reader.state().hits;
    assert!(all >= 2, "two matches on the page, {all} painted");
    reader.click(".chip.find-all");
    let one = reader.state().hits;
    assert_eq!(one, 1, "only the match the reader is on");
    reader.click(".chip.find-all");
    assert_eq!(reader.state().hits, all, "and back");
}

/// Where a highlight is drawn is the question the whole of `CharBox` exists
/// for, so it is asked directly: the rectangle is inside the page it belongs
/// to, and it is not the whole of it.
#[test]
fn a_highlight_is_a_rectangle_inside_its_page() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    let page = reader.harness.layout_rect(".page");
    let hit = reader.harness.layout_rect(".hit");
    assert!(hit.width > 0.0 && hit.height > 0.0, "{hit:?}");
    assert!(
        hit.width < page.width / 2.0,
        "a word is not half the page: {hit:?} in {page:?}",
    );
    assert!(hit.height < page.height / 10.0, "{hit:?}");
}

/// The three things the fold is for, through the renderer rather than in
/// isolation — see `fixture::prose_pdf`, which found that two of the three
/// answers are not the ones the app gets from pdf.js.
#[test]
fn an_accent_a_ligature_and_a_soft_hyphen_are_all_findable_by_typing_the_word() {
    for (query, page) in [("resume", 5), ("find", 4), ("typography", 6)] {
        let mut reader = searching();
        look_for(&mut reader, query);
        let state = reader.state();
        assert!(
            state.find.as_deref().is_some_and(|said| said.starts_with("1 of ")),
            "{query}: {:?}",
            state.find,
        );
        assert_eq!(state.page, page, "{query} is on page {page}");
    }
}

/// "Match case" and "Whole words" change what is found; both are settings and
/// both outlive the bar they are set from.
#[test]
fn the_two_switches_change_what_is_found_and_are_remembered() {
    let config = std::env::temp_dir().join(format!("hylopdf-switches-{}", std::process::id()));
    let mut reader = Reader::open_with(
        &fixture::prose_pdf(),
        Options {
            config: config.clone(),
            ..Options::default()
        },
    );
    reader.press_chord("mod+f");
    // "in" is in "in the first page" and inside "Nothing", "again", "find"
    // and "line" — so the whole-words switch has something to take away.
    look_for(&mut reader, "in");
    let loose = reader.state().find.expect("a count");
    reader.click(".chip.find-words");
    reader.scan_out();
    let whole = reader.state().find.expect("a count");
    assert_ne!(loose, whole, "whole words changed nothing: {loose}");
    assert_eq!(whole, "1 of 1");

    // And "The" is capitalised on three pages while "the" is lower case on
    // one, which is what the case switch has to see.
    reader.press("Escape");
    reader.press_chord("mod+f");
    look_for(&mut reader, "The");
    let insensitive = reader.state().find.expect("a count");
    reader.click(".chip.find-case");
    reader.scan_out();
    let cased = reader.state().find.expect("a count");
    assert_ne!(insensitive, cased, "match case changed nothing: {insensitive}");

    // Both survive the reader being closed and opened again, which is what
    // makes them settings rather than a state of the bar.
    drop(reader);
    let mut again = Reader::open_with(
        &fixture::prose_pdf(),
        Options {
            config,
            ..Options::default()
        },
    );
    again.press_chord("mod+f");
    look_for(&mut again, "in");
    assert_eq!(
        again.state().find.as_deref(),
        Some(whole.as_str()),
        "the switches did not survive the restart",
    );
}

/// A document with nothing to search says so, which is a different sentence
/// from a document that does not contain the word.
#[test]
fn a_word_that_is_not_there_and_a_document_with_no_words_say_different_things() {
    let mut reader = searching();
    look_for(&mut reader, "aardvark");
    assert_eq!(reader.state().find.as_deref(), Some("None"));
    assert_eq!(reader.state().hits, 0);
}

/// The results tab is there while the bar is, and a row goes to its match.
#[test]
fn the_results_tab_lists_the_matches_and_a_row_goes_to_one() {
    let mut reader = Reader::open_with(&fixture::prose_pdf(), Options::default());
    reader.press_chord("mod+b");
    // The prose fixture carries no outline, so the panel opens on the pages —
    // which is `setDocument`'s own rule and the difference between a panel
    // and an empty box.
    assert_eq!(reader.state().sidebar.as_deref(), Some("pages"));
    reader.press_chord("mod+f");
    look_for(&mut reader, "needle");
    assert_eq!(
        reader.state().sidebar.as_deref(),
        Some("results"),
        "searching shows the results",
    );
    assert_eq!(reader.state().results, vec![0, 1, 2]);
    reader.click_nth(".result", 2);
    assert_eq!(reader.state().find.as_deref(), Some("3 of 3"));
    assert_eq!(reader.state().page, 3);

    // And it goes when the bar does, rather than sitting there empty.
    reader.click(".chip.find-close");
    assert_eq!(reader.state().find, None);
    assert_eq!(reader.state().sidebar.as_deref(), Some("pages"));
    assert!(reader.state().results.is_empty());
}

/// **A key typed into the field is not a shortcut.** Every keystroke in the
/// field also bubbles to the root, which is where this reader turns keys into
/// actions — so without the field stopping them, typing "just" would scroll
/// the document four times on the way to searching for it.
#[test]
fn typing_into_the_field_does_not_drive_the_document() {
    let mut reader = searching();
    let before = reader.state().scroll;
    // "j" scrolls a line, "g g" goes to the top, "t" changes the theme.
    let theme = reader.state().theme;
    reader.type_text("jggt");
    reader.scan_out();
    assert_eq!(reader.state().query, "jggt");
    assert_eq!(reader.state().scroll, before, "the document moved");
    assert_eq!(reader.state().theme, theme, "the theme changed");
}

/// And a click on one of the bar's own buttons leaves the keyboard in the
/// field, which is the rule `give_keyboard_back` follows: the innermost
/// element that asks for it wins.
#[test]
fn a_click_in_the_find_bar_leaves_the_keyboard_in_the_field() {
    let mut reader = searching();
    look_for(&mut reader, "need");
    reader.click(".chip.find-all");
    reader.type_text("le");
    reader.scan_out();
    assert_eq!(reader.state().query, "needle");
}

/// Closing the bar puts the index down, which is the whole memory policy —
/// and the highlights go with it, because a mark on the page that outlives
/// the bar that made it is a mark nobody asked for.
#[test]
fn closing_the_bar_takes_the_highlights_with_it() {
    let mut reader = searching();
    look_for(&mut reader, "needle");
    assert!(reader.state().hits > 0);
    reader.press("Escape");
    assert_eq!(reader.state().hits, 0);
    assert_eq!(reader.state().find, None);
}

/// The scan is sliced, so a long document is searched over several turns of
/// the event loop rather than one — which is the only thing standing between
/// the reader and half a second of a window that does not answer.
///
/// Asked of the `Viewer` rather than through the harness, and that is the
/// point: driving it through the interface means `settle()`, and a settle is
/// several turns of the loop, so what a test would be measuring is how many
/// slices a settle happens to run rather than what a slice does. Here the
/// slices are counted.
#[test]
fn one_slice_of_the_scan_does_not_read_the_whole_book() {
    use dioxus_reader::app::Viewer;
    use dioxus_reader::page::Chosen;
    use dioxus_reader::palette::FALLBACK;
    use dioxus_reader::store::Store;

    let config = std::env::temp_dir().join(format!("hylopdf-slices-{}", std::process::id()));
    let document = dioxus_reader::render::open(&Reader::book()).expect("the fixture");
    let pages = document.pages();
    let mut viewer = Viewer::new(document, Chosen::new(FALLBACK), Store::at(&config));
    viewer.resize(1100.0, 800.0);
    viewer.open_find();
    // "quick" is on every one of the four hundred pages.
    let token = viewer.find("quick").expect("something to scan");

    assert!(
        viewer.scan_slice(token),
        "one slice read the whole of a {pages}-page book",
    );
    let first = viewer.search.state().total;
    assert!(first > 0, "a slice that found nothing is not a slice");

    let mut slices = 1;
    while viewer.scan_slice(token) {
        slices += 1;
        assert!(slices < pages, "a slice that reads no pages is a loop");
    }
    assert!(slices > 1, "the whole book went in one slice after all");
    assert!(
        viewer.search.state().total > first,
        "the count did not climb: {first} after one slice of {slices}",
    );
    assert!(!viewer.search.state().scanning, "the scan did not finish");

    // And a scan that has been replaced stops at its next slice rather than
    // running to the end of a document nobody is searching any more.
    let stale = token;
    viewer.find("fox");
    assert!(!viewer.scan_slice(stale));
}
