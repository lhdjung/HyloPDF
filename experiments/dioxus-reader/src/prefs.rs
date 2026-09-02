//! The Settings window: five pages in a frame over the reader.
//!
//! This is `settings.ts` and the `showWindow` half of `ui.ts`, and the shape
//! is the app's own: a scrim and a frame in the same document rather than a
//! window of the system's. It matters more here than there. A second winit
//! window would be a second [`Viewer`] over a second `Store`, and every
//! setting changed in it would reach the reader on its next launch —
//! `AGENTS.md` describes exactly that staleness between two reader windows,
//! and it is tolerable between two documents and not between a switch and the
//! thing the switch is about.
//!
//! What is not here, and is not pretended: the theme editor, which is a page
//! of its own. `follow_system_theme` used to be beside it for want of a
//! signal; the signal is `WindowEvent::ThemeChanged` and the switch is on the
//! Appearance page now — see [`crate::app::Appearance`].

use dioxus::prelude::*;

use crate::app::{Icon, Pane, Viewer};
use crate::keymap;
use crate::layout::{Fit, Mode, Spread};

/// The whole window, or nothing at all.
#[component]
pub fn Settings(viewer: Signal<Viewer>, frame: crate::app::Frame) -> Element {
    let held = viewer.read();
    let Some(pane) = held.pane else {
        return rsx! {};
    };
    let wearing = held.palette();
    let (ink, ink_on) = (
        crate::palette::hex(wearing.muted()),
        crate::palette::hex(wearing.accent),
    );
    drop(held);

    rsx! {
        // The scrim takes the press, which is how clicking beside the window
        // closes it — and the frame stops it, which is how clicking inside
        // does not. Blitz has no `position: fixed`, so this is absolute
        // against the root; see `.window-scrim` in `styles.rs`.
        div {
            class: "window-scrim",
            onmousedown: move |event| {
                event.stop_propagation();
                viewer.write().close_settings();
            },
            div {
                class: "window",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "Settings",
                onmousedown: move |event| event.stop_propagation(),
                div { class: "window-bar",
                    span { class: "window-title", "Settings" }
                    button {
                        class: "chip window-close",
                        "aria-label": "Close",
                        onclick: move |_| { viewer.write().close_settings(); },
                        Icon { name: "close", stroke: ink.clone() }
                    }
                }
                div { class: "window-body",
                    nav { class: "window-nav", "aria-label": "Settings",
                        for page in Pane::ALL {
                            button {
                                key: "{page.label()}",
                                class: if page == pane { "nav-item on" } else { "nav-item" },
                                onclick: move |_| viewer.write().show_pane(page),
                                Icon {
                                    name: page.icon(),
                                    stroke: if page == pane { ink_on.clone() } else { ink.clone() },
                                }
                                "{page.label()}"
                            }
                        }
                    }
                    div { class: "window-pane",
                        match pane {
                            Pane::Reading => rsx! { Reading { viewer } },
                            Pane::Appearance => rsx! { Appearance { viewer } },
                            Pane::Window => rsx! { WindowPage { viewer, frame: frame.clone() } },
                            Pane::Keyboard => rsx! { Keyboard { viewer } },
                            Pane::About => rsx! { About { viewer } },
                        }
                    }
                }
            }
        }
    }
}

/* ------------------------------------------------------------ the pieces */

/// One setting: what it is called, the control, and the sentence under it.
///
/// `ui.field` in the app, and the note is the part worth keeping — every
/// switch in that window says what it is for in a sentence, which is most of
/// why the window reads as calm rather than as a form.
#[component]
fn Field(label: String, #[props(default)] note: Option<String>, children: Element) -> Element {
    rsx! {
        div { class: "field",
            div { class: "field-head",
                span { class: "field-label", "{label}" }
                div { class: "field-control", {children} }
            }
            if let Some(note) = note {
                p { class: "field-note", "{note}" }
            }
        }
    }
}

/// A switch. `role="switch"` and `aria-checked`, because it is a button that
/// answers a yes-or-no question and the shape of it says nothing.
#[component]
pub(crate) fn Toggle(on: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        button {
            class: if on { "switch on" } else { "switch" },
            role: "switch",
            "aria-checked": if on { "true" } else { "false" },
            onclick: move |_| onchange.call(!on),
            span { class: "switch-knob" }
        }
    }
}

/// A row of choices, one of which is in force. `ui.segmented`.
#[component]
fn Segmented(options: Vec<(String, String)>, chosen: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        div { class: "segmented",
            for (value, label) in options {
                button {
                    key: "{value}",
                    class: if value == chosen { "segment on" } else { "segment" },
                    "aria-pressed": if value == chosen { "true" } else { "false" },
                    onclick: {
                        let value = value.clone();
                        move |_| onchange.call(value.clone())
                    },
                    "{label}"
                }
            }
        }
    }
}

/// A number with a step either side of it, and the number can be typed.
///
/// **The unit is beside the field and not in it**, which the app learned the
/// hard way: a field reading "16 px" puts the caret wherever the pointer
/// landed, so typing 30 gives "3016 px" and a setting at its maximum. And the
/// box is the width of what is in it, because Blitz cannot centre an input's
/// text — see the comment on `.pill` in `app.rs`, which is the same finding
/// one window along.
#[component]
pub(crate) fn Stepper(
    viewer: Signal<Viewer>,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    #[props(default)] unit: Option<String>,
    onchange: EventHandler<f64>,
) -> Element {
    let shown = format!("{}", value.round() as i64);
    // **What the field is showing, which is the number until somebody types
    // into it.** Two things make this local state rather than a straight echo
    // of `value`. A typed number is clamped on the way out, so a field being
    // typed into can disagree with the setting for a keystroke or two — "9"
    // on its way to "90" in a stepper whose maximum is 64 — and echoing the
    // clamped number back would rewrite the editor's text under the caret.
    // And Blitz's `set_text` moves no caret, so a rewrite puts it at the
    // front and the next digit lands in front of the last one. See the page
    // field in `app.rs`, which is the same finding one window along.
    let mut typed = use_signal(|| None::<String>);
    // And whether nothing has been typed yet, which is the emulated
    // "all of it is selected": parley will select-all for a keystroke and for
    // nothing else, so arriving in a field cannot select what is in it and
    // the first thing typed has to replace it by hand. Without this, a caret
    // that starts at offset 0 means Backspace does nothing and a typed digit
    // goes in front of the number that is already there — 20 with "3" typed
    // into it is 320, which clamps to the maximum.
    let mut fresh = use_signal(|| false);
    let showing = typed.read().clone().unwrap_or_else(|| shown.clone());
    let width = (14.0 + 8.5 * showing.chars().count() as f64).max(34.0);
    rsx! {
        div { class: "stepper",
            button {
                class: "chip step-down",
                "aria-label": "Less",
                onclick: move |_| {
                    typed.set(None);
                    fresh.set(false);
                    onchange.call((value - step).clamp(min, max));
                },
                "−"
            }
            input {
                class: if *fresh.read() { "step-field fresh" } else { "step-field" },
                style: "width: {width}px;",
                r#type: "text",
                value: "{showing}",
                "data-keyboard": "stepper",
                // Arriving is the start of the "all of it is selected" state,
                // and a second press inside is the end of it — which is what
                // a click into a selected field does anywhere else, and what
                // keeps the arithmetic in `oninput` true.
                onmousedown: move |_| {
                    if typed.read().is_none() {
                        typed.set(Some(shown.clone()));
                        fresh.set(true);
                    } else {
                        fresh.set(false);
                    }
                },
                // A typed value is clamped to the range and never snapped to
                // the step: the step is how far one press moves, not a list
                // of the answers allowed. `ui.stepper` in the app says so.
                oninput: move |event| {
                    let raw = event.value();
                    let was = typed.read().clone().unwrap_or_default();
                    // Fresh means the caret was at the front, so whatever
                    // arrived is at the front of the value and taking the old
                    // number off the end leaves exactly what was typed.
                    let text = if *fresh.read() {
                        fresh.set(false);
                        raw.strip_suffix(&was).unwrap_or(&raw).to_string()
                    } else {
                        raw
                    };
                    typed.set(Some(text.clone()));
                    if let Ok(number) = text.trim().parse::<f64>() {
                        onchange.call(number.clamp(min, max));
                    }
                },
                // **Escape has to be answered here, and that is Blitz's focus
                // rule rather than a nicety.** The keyboard goes to the
                // innermost element asking for it (see
                // `app::give_keyboard_back`), and a stepper on the page is
                // that element the moment the window opens — so a plain key
                // stopped here, which every plain key must be or it reaches
                // the root and scrolls the document behind the window, would
                // swallow the one key that closes the thing the reader is
                // looking at.
                onkeydown: move |event| {
                    let modifiers = event.modifiers();
                    if !crate::keymap::plain(modifiers) {
                        return;
                    }
                    let key = event.key();
                    event.stop_propagation();
                    match key {
                        Key::Escape => {
                            typed.set(None);
                            fresh.set(false);
                            viewer.write().close_settings();
                        }
                        // Backspace on a field whose contents are all
                        // "selected" empties it, which is what Backspace on a
                        // real selection does. The editor's own would delete
                        // what is before the caret, and the caret is at the
                        // front, so without this it does nothing at all.
                        Key::Backspace | Key::Delete if *fresh.read() => {
                            event.prevent_default();
                            fresh.set(false);
                            typed.set(Some(String::new()));
                        }
                        _ => {}
                    }
                },
            }
            if let Some(unit) = unit {
                span { class: "step-unit", "{unit}" }
            }
            button {
                class: "chip step-up",
                "aria-label": "More",
                onclick: move |_| {
                    typed.set(None);
                    fresh.set(false);
                    onchange.call((value + step).clamp(min, max));
                },
                "+"
            }
        }
    }
}

/// A heading inside a page, and a paragraph that is not about one setting.
#[component]
fn Note(text: String) -> Element {
    rsx! { p { class: "pane-note", "{text}" } }
}

/* ------------------------------------------------------------- the pages */

#[component]
fn Reading(viewer: Signal<Viewer>) -> Element {
    let held = viewer.read();
    let mode = held.layout.mode;
    let spread = held.layout.spread;
    let gap = held.layout.gap;
    let fit = held.layout.fit;
    let zoom = held.layout.zoom;
    let trimming = held.trims_margins();
    let (remember, reopen, pill) = (
        held.store.flag("remember_position"),
        held.store.flag("reopen_last_document"),
        held.store.flag("show_page_pill"),
    );
    drop(held);

    rsx! {
        h2 { class: "pane-title", "Reading" }
        Field {
            label: "Page progression",
            note: "No shortcut can change it by accident.",
            Segmented {
                options: vec![
                    ("continuous".into(), "Continuous (default)".into()),
                    ("paged".into(), "One page at a time".into()),
                ],
                chosen: if mode == Mode::Paged { "paged".to_string() } else { "continuous".to_string() },
                onchange: move |value: String| {
                    let mode = if value == "paged" { Mode::Paged } else { Mode::Continuous };
                    viewer.write().set_scroll_mode(mode);
                },
            }
        }
        Field {
            label: "Pages side by side",
            note: "Two pages across uses a wide window the way a book does. \u{201c}Cover alone\u{201d} leaves page one on its own, so that every spread after it falls the way it was printed.",
            Segmented {
                options: vec![
                    ("single".into(), "One (default)".into()),
                    ("two".into(), "Two".into()),
                    ("cover".into(), "Two, cover alone".into()),
                ],
                chosen: match spread {
                    Spread::Single => "single".to_string(),
                    Spread::Two => "two".to_string(),
                    Spread::Cover => "cover".to_string(),
                },
                onchange: move |value: String| {
                    let spread = match value.as_str() {
                        "two" => Spread::Two,
                        "cover" => Spread::Cover,
                        _ => Spread::Single,
                    };
                    viewer.write().set_spread(spread);
                },
            }
        }
        Field {
            label: "Space between pages",
            note: "How much room to leave between one page and the next.",
            Stepper {
                viewer,
                value: gap, min: 0.0, max: 64.0, step: 4.0, unit: "px",
                onchange: move |value| viewer.write().set_page_gap(value),
            }
        }
        Field {
            label: "Trim the margins",
            note: "Scanned books and anything typeset with an inch of white down each side spend a quarter of the window on paper. This measures where the ink starts — over a sample of the pages, so every page keeps the same scale — and gives that room back to the words.",
            Toggle { on: trimming, onchange: move |on| viewer.write().set_trim(on) }
        }
        Field {
            label: "Zoom",
            note: "Fit width follows the window; a fixed zoom stays where you put it.",
            Segmented {
                options: vec![
                    ("width".into(), "Fit width (default)".into()),
                    ("page".into(), "Fit page".into()),
                    ("actual".into(), "Fixed".into()),
                ],
                chosen: match fit {
                    Fit::Width => "width".to_string(),
                    Fit::Page => "page".to_string(),
                    Fit::Actual => "actual".to_string(),
                },
                onchange: move |value: String| {
                    match value.as_str() {
                        "page" => viewer.write().set_fit(Fit::Page),
                        "actual" => viewer.write().actual_size(),
                        _ => viewer.write().set_fit(Fit::Width),
                    }
                },
            }
        }
        // Only where there is a fixed zoom to set, which is what `zoomField`
        // hides itself for in the app.
        if fit == Fit::Actual {
            Field {
                label: "Fixed zoom",
                Stepper {
                    viewer,
                    value: (zoom * 100.0).round(), min: 25.0, max: 600.0, step: 25.0, unit: "%",
                    onchange: move |value: f64| viewer.write().set_zoom(value / 100.0),
                }
            }
        }
        Field {
            label: "Come back to where I stopped",
            note: "Each document reopens on the page you left it on.",
            Toggle { on: remember, onchange: move |on| viewer.write().set_flag("remember_position", on) }
        }
        Field {
            label: "Open what I was reading",
            note: "Start on the documents that were open when you last quit — a window each, where there was more than one. Closing a document yourself means you are done with it, and it is not reopened.",
            Toggle { on: reopen, onchange: move |on| viewer.write().set_flag("reopen_last_document", on) }
        }
        Field {
            label: "Show page count while scrolling",
            note: "A brief \u{201c}page 23 of 197\u{201d} while you scroll with the toolbar hidden.",
            Toggle { on: pill, onchange: move |on| viewer.write().set_flag("show_page_pill", on) }
        }
    }
}

#[component]
fn Appearance(viewer: Signal<Viewer>) -> Element {
    let held = viewer.read();
    let editing = held.editing.clone();
    let chosen = held.store.theme_index();
    let worn = held.store.theme().clone();
    let themes: Vec<(usize, String, [String; 3])> = held
        .store
        .themes()
        .iter()
        .enumerate()
        .map(|(index, theme)| {
            // Resolved rather than handed over raw. Nothing may show a
            // theme's colour without going through `parseColor` — a swatch
            // that shows a colour the renderer cannot read is the picker
            // lying about the page. See `AGENTS.md`.
            let colours = crate::palette::resolve(theme, true);
            (
                index,
                theme.name.clone(),
                [
                    crate::palette::hex(colours.background),
                    crate::palette::hex(colours.text),
                    crate::palette::hex(colours.accent),
                ],
            )
        })
        .collect();
    let dark = held.store.dark_now();
    let following = held.store.flag("follow_system_theme");
    let recolor_images = held.store.flag("recolor_images");
    // What the machine says, which is the difference between a switch that
    // does something and a switch that cannot: a platform reporting nothing
    // has no light and dark to follow, and saying so is better than leaving
    // an inert switch on the page.
    let machine = held.store.outside();
    let folder = held.store.themes_dir().display().to_string();
    drop(held);
    let mac = keymap::this_machine();

    rsx! {
        h2 { class: "pane-title", "Appearance" }
        // The three switches, in `appearancePage`'s own order.
        Field {
            label: "Follow the system",
            note: match machine {
                Some(_) => "Take the light theme when the machine is light and the dark one when it is dark. Choosing a theme that disagrees turns this off.".to_string(),
                None => "This machine does not report an appearance, so there is nothing to follow.".to_string(),
            },
            Toggle { on: following, onchange: move |on| viewer.write().set_follow_system(on) }
        }
        Field {
            label: "Dark mode",
            note: format!("Switches between the light theme and the dark theme you last chose. {}", if mac { "⌘D" } else { "Ctrl+D" }),
            Toggle { on: dark, onchange: move |on| viewer.write().set_dark(on) }
        }
        Field {
            label: "Recolour pictures too",
            note: "On, pictures take the theme along with the rest of the page. Off, they stay exactly as printed.".to_string(),
            Toggle { on: recolor_images, onchange: move |on| viewer.write().set_recolor_images(on) }
        }
        if let Some(draft) = editing {
            ThemeEditor { viewer, draft }
        } else {
            h3 { class: "pane-group", "Themes" }
            div { class: "theme-grid",
                for (index, name, colours) in themes {
                    button {
                        key: "{index}",
                        class: if index == chosen { "theme-card on" } else { "theme-card" },
                        "aria-pressed": if index == chosen { "true" } else { "false" },
                        onclick: move |_| viewer.write().set_theme(index),
                        div {
                            class: "theme-swatch",
                            style: "background: {colours[0]};",
                            span { class: "swatch-ink", style: "background: {colours[1]};" }
                            span { class: "swatch-accent", style: "background: {colours[2]};" }
                        }
                        span { class: "theme-name", "{name}" }
                    }
                }
            }
            div { class: "pane-actions",
                button {
                    class: "chip action",
                    onclick: move |_| viewer.write().begin_theme(None),
                    "New theme…"
                }
                button {
                    class: "chip action",
                    onclick: {
                        let worn = worn.clone();
                        move |_| viewer.write().begin_theme(Some(worn.clone()))
                    },
                    // A built-in is copied rather than edited, which is the
                    // app's own rule and its own wording: a shipped theme is
                    // written back on every run, so an edit in place would be
                    // silently reverted.
                    {if worn.built_in {
                        format!("Make a copy of {}…", worn.name)
                    } else {
                        format!("Edit {}…", worn.name)
                    }}
                }
                if !worn.built_in {
                    button {
                        class: "chip action danger",
                        onclick: {
                            let worn = worn.clone();
                            move |_| {
                                viewer.write().begin_theme(Some(worn.clone()));
                                viewer.write().delete_theme();
                            }
                        },
                        "Delete {worn.name}"
                    }
                }
            }
            Note { text: format!("Theme files live in {folder}. They are plain text — a theme can be written by hand, or copied to another computer.") }
        }
    }
}

/// A theme being written, field by field. `themeEditor` in `settings.ts`.
///
/// **One difference from the app, and it is the platform's.** There, each
/// colour is an `<input type="color">` beside a hex field — the operating
/// system's own picker. Blitz has no colour input, so what is here is the
/// swatch and the hex field, and the swatch is a preview rather than a way in.
/// Everything else is the app's: the same seven fields in the same order with
/// the same sentences under them, the draft worn while it is being written,
/// and Cancel, Save and Delete at the foot.
#[component]
fn ThemeEditor(viewer: Signal<Viewer>, draft: crate::theme::Theme) -> Element {
    // What the page will actually use, which is what the fields have to show:
    // four of the seven are derived when the file does not name them, and a
    // field standing in with something else is the picker lying again.
    let shown = crate::palette::resolve(&draft, true);
    let hex = crate::palette::hex;
    let fresh = draft.id.trim().is_empty();

    rsx! {
        h3 { class: "pane-group", {if fresh { "New theme" } else { "Edit theme" }} }
        Field { label: "Name",
            TextField {
                value: draft.name.clone(),
                onchange: move |value| viewer.write().draft_set("name", value),
            }
        }
        Field {
            label: "Text",
            note: "The colour the words are printed in.".to_string(),
            ColorField {
                value: hex(shown.text),
                onchange: move |value| viewer.write().draft_set("text", value),
            }
        }
        Field {
            label: "Background",
            note: "The colour of the paper behind them.".to_string(),
            ColorField {
                value: hex(shown.background),
                onchange: move |value| viewer.write().draft_set("background", value),
            }
        }
        Field {
            label: "Accent",
            note: "The current page, the ring around whatever has the keyboard, and anything else that needs to stand out.".to_string(),
            ColorField {
                value: hex(shown.accent),
                onchange: move |value| viewer.write().draft_set("accent", value),
            }
        }
        Field {
            label: "Links",
            note: "Links in the document take this colour, wherever the page is recoloured.".to_string(),
            ColorField {
                value: hex(shown.link),
                onchange: move |value| viewer.write().draft_set("link", value),
            }
        }
        Field {
            label: "Selection area",
            note: "The colour behind text you have selected. Left alone it follows the accent.".to_string(),
            ColorField {
                value: hex(shown.selection_area),
                onchange: move |value| viewer.write().draft_set("selection_area", value),
            }
        }
        Field {
            label: "Selected text",
            note: "The words inside that area. Left alone they take the opposite of it.".to_string(),
            ColorField {
                value: hex(shown.selection_text),
                onchange: move |value| viewer.write().draft_set("selection_text", value),
            }
        }
        Field {
            label: "Recolour the document",
            note: "Off leaves every page exactly as it was printed.".to_string(),
            Toggle {
                on: draft.recolor,
                onchange: move |on| viewer.write().draft_recolor(on),
            }
        }
        div { class: "pane-actions",
            button {
                class: "chip action",
                onclick: move |_| viewer.write().cancel_theme(),
                "Cancel"
            }
            button {
                class: "chip action primary",
                onclick: move |_| viewer.write().save_theme(),
                "Save theme"
            }
            // Only a theme already on disk can be deleted: "New theme…" and a
            // copy of a built-in have not been saved yet.
            if !fresh {
                button {
                    class: "chip action danger",
                    onclick: move |_| viewer.write().delete_theme(),
                    "Delete this theme"
                }
            }
        }
    }
}

/// A line of text somebody types. The app's `ui.textField`.
#[component]
fn TextField(value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        input {
            class: "text-field",
            r#type: "text",
            value: "{value}",
            oninput: move |event| onchange.call(event.value()),
        }
    }
}

/// A colour: what it looks like, and the six digits that say so.
///
/// The hex field takes every notation the renderer reads rather than only the
/// long one — a theme file may perfectly well say `#fff` — and what leaves
/// here is always the six-digit form, because that is what is written back to
/// the file. A value the renderer cannot read is simply not passed on, which
/// is `readColor` returning null in the app.
#[component]
fn ColorField(value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        span { class: "color-field",
            span { class: "color-swatch", style: "background: {value};" }
            input {
                class: "text-field color-hex",
                r#type: "text",
                value: "{value}",
                oninput: move |event| {
                    if let Some(read) = crate::palette::read_colour(&event.value()) {
                        onchange.call(crate::palette::hex(read));
                    }
                },
            }
        }
    }
}

#[component]
fn WindowPage(viewer: Signal<Viewer>, frame: crate::app::Frame) -> Element {
    let held = viewer.read();
    let toolbar = held.toolbar;
    let sidebar = held.sidebar_open;
    let width = held.sidebar_width;
    let full = held.full_screen;
    let presenting = held.presenting;
    drop(held);
    let mac = keymap::this_machine();

    rsx! {
        h2 { class: "pane-title", "Window" }
        Field {
            label: "Show toolbar",
            note: format!("The bar along the top. Hidden, the page number appears briefly as you scroll, and the top edge of the window brings the bar back. {}", if mac { "⌘T" } else { "Ctrl+T" }),
            Toggle { on: toolbar, onchange: move |_| viewer.write().toggle_toolbar() }
        }
        Field {
            label: "Show contents sidebar",
            note: format!("Chapters and page thumbnails, down the left. {}", if mac { "⌘B" } else { "Ctrl+B" }),
            Toggle { on: sidebar, onchange: move |_| viewer.write().toggle_sidebar() }
        }
        Field {
            label: "Sidebar width",
            note: "It can also be dragged by its edge.",
            Stepper {
                viewer,
                value: width, min: crate::sidebar::MIN_WIDTH, max: crate::sidebar::MAX_WIDTH, step: 8.0, unit: "px",
                onchange: move |value| viewer.write().set_sidebar_width(value),
            }
        }
        // Both of these are the *window's* rather than the page's, so throwing
        // one is an `Ask` — which is why this component takes the `Frame` the
        // reader holds. They were a sentence here until it did.
        Field {
            label: "Full screen",
            note: format!(
                "The window fills the screen. {} — and Escape leaves again.",
                if mac { "⌘⌃F" } else { "F11" },
            ),
            Toggle {
                on: full,
                onchange: {
                    let frame = frame.clone();
                    move |on| {
                        viewer.write().set_full_screen(on);
                        frame.ask(crate::app::Ask::FullScreen(on));
                    }
                },
            }
        }
        Field {
            label: "Presenting",
            note: format!(
                "Full screen with nothing else on it: the two switches above, thrown together, and Escape puts both back. {}",
                if mac { "⌘⇧P" } else { "Ctrl+Shift+P" },
            ),
            Toggle {
                on: presenting,
                onchange: {
                    let frame = frame.clone();
                    move |on| {
                        let full = viewer.write().present(on);
                        frame.ask(crate::app::Ask::FullScreen(full));
                    }
                },
            }
        }
    }
}

#[component]
fn Keyboard(viewer: Signal<Viewer>) -> Element {
    // **Drawn from the keymap, never from a list of its own.** The app's page
    // was a hand-written table once and it had already drifted: it named ⌘T
    // twice and could not have known about a key the reader rebound. Every
    // row here is an action out of `keymap.rs` with whatever `keys.toml` gave
    // it, so the page is a view of what the reader will actually get.
    let held = viewer.read();
    let keymap = held.keymap.clone();
    let keys_file = held.store.dir().join("keys.toml").display().to_string();
    drop(held);
    let mac = keymap.mac();

    rsx! {
        h2 { class: "pane-title", "Keyboard" }
        // What could not be read comes first: a key that does nothing is
        // otherwise found out about by pressing it.
        if !keymap.problems.is_empty() {
            h3 { class: "pane-group", "In your keys.toml" }
            for problem in keymap.problems.clone() {
                Note { text: problem }
            }
        }
        for group in keymap::GROUPS {
            {
                let rows: Vec<(String, String)> = keymap::ACTIONS
                    .iter()
                    .filter(|spec| spec.group == group)
                    .filter_map(|spec| {
                        let chords = keymap.by_action.get(&spec.id)?;
                        if chords.is_empty() {
                            return None;
                        }
                        let shown = chords
                            .iter()
                            .map(|chord| keymap::shown(chord, mac))
                            .collect::<Vec<_>>()
                            .join("  or  ");
                        Some((spec.label.to_string(), shown))
                    })
                    .collect();
                rsx! {
                    if !rows.is_empty() {
                        h3 { class: "pane-group", "{group.as_str()}" }
                        div { class: "keys",
                            for (what, chord) in rows {
                                span { key: "{what}", class: "key-what", "{what}" }
                                span { class: "key-chord", "{chord}" }
                            }
                        }
                    }
                }
            }
        }
        h3 { class: "pane-group", "Without the keyboard" }
        div { class: "keys",
            span { class: "key-what", "Bring the toolbar back when it is hidden" }
            span { class: "key-chord", "The top edge of the window" }
            span { class: "key-what", "Open something else you have been reading" }
            span { class: "key-chord", "The document's name in the bar" }
            span { class: "key-what", "Move a page that is wider than the window" }
            span { class: "key-chord", "Two fingers across, or ⇧ and the wheel" }
        }
        h3 { class: "pane-group", "Changing keybinds" }
        Note { text: "Every keybind is in keys.toml in your config folder, commented out. Uncomment a line to change its keys, then Reload — the file is deliberately not watched, because the app writes to that folder several times a minute while you are scrolling." }
        div { class: "pane-actions",
            // The file itself, opened in whatever edits text here. The app's
            // own first button, and the reason it is beside Reload: the two
            // are one gesture — change the file, then say so.
            OpenPath {
                viewer,
                label: "Open keys file".to_string(),
                path: keys_file,
            }
            button {
                class: "chip action",
                onclick: move |_| viewer.write().reload_keys(),
                "Reload"
            }
        }
    }
}

#[component]
fn About(viewer: Signal<Viewer>) -> Element {
    let held = viewer.read();
    let config = held.store.dir().display().to_string();
    let themes = held.store.themes_dir().display().to_string();
    let settings_file = held.store.dir().join("settings.toml").display().to_string();
    drop(held);

    rsx! {
        h2 { class: "pane-title", "HyloPDF" }
        p { class: "pane-lede", "A calm place to read." }
        Note { text: "This is the Dioxus Native experiment: the same reader, drawn by Blitz and pdfium rather than by a webview and pdf.js." }
        Note { text: "Your settings and your themes are plain text. Nothing is stored anywhere else, and nothing leaves this computer." }
        div { class: "keys",
            span { class: "key-what", "Settings and keys" }
            span { class: "key-chord", "{config}" }
            span { class: "key-what", "Themes" }
            span { class: "key-chord", "{themes}" }
        }
        div { class: "pane-actions",
            OpenPath {
                viewer,
                label: "Open settings file".to_string(),
                path: settings_file,
            }
            OpenPath {
                viewer,
                label: "Open themes folder".to_string(),
                path: themes.clone(),
            }
        }
    }
}

/// A button that hands a path to whatever the platform opens it with.
///
/// Three of these, and all three are about the same thing: the files this
/// reader keeps are plain text, and the point of saying so is that they can be
/// opened. Through [`crate::app::Reveal`], which is the one door in this crate
/// that starts another program — so a test writes the path down instead.
#[component]
fn OpenPath(viewer: Signal<Viewer>, label: String, path: String) -> Element {
    let reveal = use_hook(|| {
        dioxus_core::try_consume_context::<crate::app::Reveal>()
            .unwrap_or_else(crate::app::Reveal::to_the_system)
    });
    rsx! {
        button {
            class: "chip action",
            onclick: move |_| {
                if let Err(said) = reveal.show(&path) {
                    viewer.write().notice = said;
                }
            },
            "{label}"
        }
    }
}
