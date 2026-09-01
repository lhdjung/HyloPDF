//! The Settings window: five pages, and the switches in them doing what they
//! say.
//!
//! It is a window in the flow rather than one of the system's — see
//! `src/prefs.rs` — which is why it can be tested here at all: a second winit
//! window would be a second `Viewer` over a second `Store`, and the harness
//! has no windows.

use dioxus_reader::harness::{Options, Reader};
use dioxus_reader::theme;

fn book() -> Reader {
    Reader::open(&Reader::book())
}

/// Whether the window is up.
fn open(reader: &Reader) -> bool {
    reader.harness.query(".window").is_some()
}

/// Which page the nav column has in force.
fn page(reader: &Reader) -> String {
    reader.harness.text_content(".nav-item.on")
}

#[test]
fn the_settings_window_opens_on_its_key_and_leaves_on_escape() {
    let mut reader = book();
    assert!(!open(&reader));
    reader.press_chord("mod+,");
    assert!(open(&reader), "⌘, is the app's own key for it");
    assert_eq!(page(&reader), "Reading");

    reader.press("Escape");
    assert!(!open(&reader));

    // And it comes back to the page it was left on, which is what a window
    // with a nav column is expected to do — `currentPage` in `settings.ts`.
    reader.press_chord("mod+,");
    reader.click_nth(".nav-item", 3);
    assert_eq!(page(&reader), "Keyboard");
    reader.press("Escape");
    reader.press_chord("mod+,");
    assert_eq!(page(&reader), "Keyboard");
}

#[test]
fn the_document_menu_opens_it_too() {
    // ⌘, is not discoverable and the app has a Settings button in its bar.
    // There is no room for one here, so it hangs off the menu that already
    // says what this window is.
    let mut reader = book();
    reader.click(".chip.title");
    let items = reader.harness.query_all(".menu.document .menu-item").len();
    reader.click_nth(".menu.document .menu-item", items - 1);
    assert!(open(&reader));
    assert!(reader.harness.query(".menu.document").is_none(), "and the menu goes");
}

#[test]
fn a_press_beside_the_window_closes_it_and_a_press_inside_does_not() {
    let mut reader = book();
    reader.press_chord("mod+,");

    // Inside: the frame stops the press before the scrim sees it.
    let (x, y) = reader.harness.center_of(".window-pane");
    reader.click_at(x, y);
    assert!(open(&reader), "a press inside stays inside");

    // Beside it: the scrim's own press, which is `showWindow`'s scrim in the
    // app doing the same thing.
    reader.click_at(20.0, 500.0);
    assert!(!open(&reader));
}

#[test]
fn a_switch_changes_the_reader_and_is_written_down() {
    let mut reader = book();
    reader.press_chord("mod+,");
    assert_eq!(reader.harness.text_content(".chip.trim"), "Trim");

    // "Trim the margins" is the fourth field on the Reading page and the
    // first switch on it. What is asserted is the *toolbar*, because that is
    // where a reader would see it: the chip and the switch are two views of
    // one setting and this is the pair agreeing.
    reader.click(".switch");
    assert_eq!(reader.harness.text_content(".chip.trim"), "Trimmed");
    assert_eq!(
        reader.harness.attr(".switch", "aria-checked").as_deref(),
        Some("true"),
        "and the switch says so, which is what a screen reader is told",
    );

    // Written down rather than only done: a second reader on the same config
    // directory opens trimming.
    let config = reader.config.clone();
    let beside = Reader::open_with(
        &Reader::book(),
        Options { config, ..Options::default() },
    );
    assert_eq!(beside.harness.text_content(".chip.trim"), "Trimmed");
}

#[test]
fn a_row_of_choices_changes_what_is_in_force() {
    let mut reader = book();
    reader.press_chord("mod+,");
    // Page progression is the first segmented control, and paged is its
    // second option. Nothing else in this app can reach it: there is
    // deliberately no shortcut for it, which is the brief's own rule.
    reader.click_nth(".segmented .segment", 1);
    // Paged mode lays out one row and nothing else — see `Layout::relayout`,
    // where that is the whole of the difference between the two modes — so
    // the pages in the DOM are what says it happened.
    assert_eq!(reader.state().mounted, vec![1]);
    assert_eq!(
        reader.harness.attr(".segmented .segment", "aria-pressed").as_deref(),
        Some("false"),
        "and the one that was in force stands down",
    );
}

#[test]
fn a_number_can_be_stepped_and_typed() {
    let mut reader = book();
    // Fit page first, so that there is more than one page in the window to
    // measure between: at fit width a page of this fixture is taller than the
    // window and only one is mounted.
    reader.press_chord("mod+2");
    reader.press_chord("mod+,");
    // Measured off the pages rather than off the setting: the gap is a
    // distance on the screen, and the screen is where it has to appear.
    let gap = |reader: &Reader| {
        let pages = reader.harness.query_all(".page");
        let first = reader.harness.layout_rect_of(pages[0]);
        let second = reader.harness.layout_rect_of(pages[1]);
        (second.y - (first.y + first.height)).round()
    };
    assert_eq!(gap(&reader), 16.0, "the default");

    // The first stepper on the Reading page is the space between pages.
    reader.click(".stepper .step-up");
    assert_eq!(gap(&reader), 20.0, "one press is one step");

    // And a typed value is clamped to the range but never snapped to the
    // step: the step is how far one press moves, not a list of the answers
    // allowed. `ui.stepper` in the app says the same.
    reader.click(".step-field");
    reader.press("Backspace");
    reader.press("Backspace");
    reader.type_text("30");
    assert_eq!(gap(&reader), 30.0);
}

#[test]
fn the_keyboard_page_is_drawn_from_the_keymap() {
    // A hand-written table of shortcuts drifts the moment a key moves — the
    // app's did, naming ⌘T twice — so every row is an action out of the
    // keymap with whatever `keys.toml` gave it.
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            keys: [("next-page".to_string(), vec!["n".to_string()])]
                .into_iter()
                .collect(),
            ..Options::default()
        },
    );
    reader.press_chord("mod+,");
    reader.click_nth(".nav-item", 3);

    let listed: String = reader
        .harness
        .query_all(".keys")
        .iter()
        .map(|&node| reader.harness.layout_rect_of(node))
        .map(|_| String::new())
        .collect::<String>()
        + &reader.harness.text_content(".window-pane");
    assert!(
        listed.contains("Next pageN"),
        "a rebound key is the key it was rebound to: {listed}",
    );
    assert!(
        listed.contains("Search this document"),
        "and the rest of the keymap is still listed",
    );
}

#[test]
fn a_theme_is_chosen_from_its_own_swatch() {
    let mut reader = book();
    reader.press_chord("mod+,");
    reader.click_nth(".nav-item", 1);

    let cards = reader.harness.query_all(".theme-card").len();
    assert_eq!(cards, theme::BUILT_IN.len(), "every shipped theme is listed");

    // The second card, which is the dark one the Hylo family opens with.
    reader.click_nth(".theme-card", 1);
    assert_eq!(reader.state().theme, "Hylo Dark");
}

/* --------------------------------------------------- dark mode, and the machine */

/// Which of the Appearance page's switches is on, by its label.
fn switched(reader: &Reader, label: &str) -> bool {
    let labels = reader.text_all(".field-label");
    let index = labels
        .iter()
        .position(|found| found == label)
        .unwrap_or_else(|| panic!("no field called {label}: {labels:?}"));
    reader.attribute_all("[role='switch']", "aria-checked")[index] == "true"
}

/// The Appearance page, with the switches on it.
fn appearance(reader: &mut Reader) {
    reader.press_chord("mod+,");
    reader.click_nth(".nav-item", 1);
    assert_eq!(page(reader), "Appearance");
}

#[test]
fn dark_mode_is_a_key_and_a_switch_and_they_are_the_same_thing() {
    let mut reader = Reader::open(&Reader::book());
    assert_eq!(reader.state().theme, "Hylo Light");

    // ⌘D, which until now answered "Dark mode is not built yet".
    reader.press_chord("mod+d");
    assert_eq!(reader.state().theme, "Hylo Dark");
    reader.press_chord("mod+d");
    assert_eq!(reader.state().theme, "Hylo Light");

    // And the switch on the Appearance page, which is the same call.
    appearance(&mut reader);
    assert!(!switched(&reader, "Dark mode"));
    reader.click_nth("[role='switch']", 0);
    assert!(switched(&reader, "Dark mode"));
    reader.press("Escape");
    assert_eq!(reader.state().theme, "Hylo Dark");
}

#[test]
fn dark_mode_returns_to_the_pair_the_reader_chose() {
    // Sepia by day and Tokyo Night by night, which is the whole reason there
    // are two remembered slots rather than one remembered theme.
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            settings: vec![
                ("theme".into(), "sepia".into()),
                ("light_theme".into(), "sepia".into()),
                ("dark_theme".into(), "tokyo-night".into()),
            ],
            ..Options::default()
        },
    );
    assert_eq!(reader.state().theme, "Sepia");
    reader.press_chord("mod+d");
    assert_eq!(reader.state().theme, "Tokyo Night");
    reader.press_chord("mod+d");
    assert_eq!(reader.state().theme, "Sepia", "not Hylo Light");
}

#[test]
fn a_dark_machine_is_read_in_the_dark_from_the_first_frame() {
    // Not "the theme changes shortly after launch": the reader asks the
    // window before it lays anything out, so a dark machine never sees a
    // white page on the way in. There is no frame here in which it is light.
    let reader = Reader::open_with(
        &Reader::book(),
        Options { appearance: Some(true), ..Options::default() },
    );
    assert_eq!(reader.state().theme, "Hylo Dark");
}

#[test]
fn the_machine_changing_its_mind_is_followed_and_then_is_not() {
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options { appearance: Some(false), ..Options::default() },
    );
    assert_eq!(reader.state().theme, "Hylo Light");

    // Evening.
    reader.set_appearance(Some(true));
    assert_eq!(reader.state().theme, "Hylo Dark");
    reader.set_appearance(Some(false));
    assert_eq!(reader.state().theme, "Hylo Light");

    // Now the reader overrules it, by pressing ⌘D at noon. Following stops,
    // and the reader is told where the switch is — otherwise the machine's
    // next word would take the choice straight back off them.
    reader.press_chord("mod+d");
    assert_eq!(reader.state().theme, "Hylo Dark");
    assert!(
        reader.state().notice.contains("No longer following"),
        "{}",
        reader.state().notice
    );
    appearance(&mut reader);
    assert!(!switched(&reader, "Light or dark follow system"));
    reader.press("Escape");

    // …and the machine going light again leaves them where they are.
    reader.set_appearance(Some(false));
    assert_eq!(reader.state().theme, "Hylo Dark");
}

#[test]
fn following_can_be_switched_back_on_and_takes_effect_at_once() {
    // A switch that says "follow the system" and leaves a light theme up on a
    // dark machine has not been believed by anybody.
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            appearance: Some(true),
            settings: vec![("follow_system_theme".into(), false.into())],
            ..Options::default()
        },
    );
    assert_eq!(reader.state().theme, "Hylo Light", "not following, so not moved");

    appearance(&mut reader);
    assert!(!switched(&reader, "Light or dark follow system"));
    reader.click_nth("[role='switch']", 1);
    assert!(switched(&reader, "Light or dark follow system"));
    reader.press("Escape");
    assert_eq!(reader.state().theme, "Hylo Dark");
}

#[test]
fn a_machine_that_will_not_say_leaves_the_reader_alone() {
    // winit answers `Option<Theme>` and the `None` is real. Read as "light"
    // it would move every reader on such a platform to the light theme at
    // every launch, and turn following off the first time they chose a dark
    // one — which is why `Store::outside` is an `Option` all the way down.
    let mut reader = Reader::open_with(
        &Reader::book(),
        Options {
            settings: vec![
                ("theme".into(), theme::DEFAULT_DARK.into()),
                ("dark_theme".into(), theme::DEFAULT_DARK.into()),
            ],
            ..Options::default()
        },
    );
    assert_eq!(reader.state().theme, "Hylo Dark");
    reader.set_appearance(None);
    assert_eq!(reader.state().theme, "Hylo Dark");

    appearance(&mut reader);
    // The switch still reads the setting rather than what the setting can do
    // today — a control that reads back other than what is in the file is the
    // picker lying about the page. The sentence under it is where the
    // machine's silence is said.
    assert!(switched(&reader, "Light or dark follow system"));
    let notes = reader.text_all(".field-note");
    assert!(
        notes.iter().any(|note| note.contains("does not report an appearance")),
        "{notes:?}"
    );
}
