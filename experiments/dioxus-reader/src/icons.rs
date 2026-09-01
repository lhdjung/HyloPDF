//! The app's icon set, as it is in `src/icons.ts`.
//!
//! **The paths are copied and the wrapper is not.** `icons.ts` is a table of
//! path data with one function around it that writes an `<svg>`; the table is
//! the drawing and is carried over character for character, because two
//! drawings of the same 24px grid that drift apart is exactly the kind of copy
//! `AGENTS.md` warns about — and there is no way to mount this file the way
//! `theme.rs` and `settings.rs` are mounted, because it is TypeScript.
//! `tests/icons.rs` reads `src/icons.ts` and checks the two tables against
//! each other, which is what keeps the copy honest.
//!
//! Only the icons this reader's chrome actually uses are here. Adding one is
//! adding its row from `icons.ts`; the test will say if a row has moved on the
//! other side.

/// The shapes of one icon, as the inside of an `<svg viewBox="0 0 24 24">`.
///
/// Named by the app's own key, so that a button here and a button there ask
/// for the same drawing by the same name.
pub fn path(name: &str) -> Option<&'static str> {
    Some(match name {
        "contents" => r#"<path d="M4 6.5h.01M4 12h.01M4 17.5h.01M9 6.5h11M9 12h11M9 17.5h7"/>"#,
        "pages" => r#"<rect x="4" y="4" width="7" height="7" rx="1.4"/><rect x="13" y="4" width="7" height="7" rx="1.4"/><rect x="4" y="13" width="7" height="7" rx="1.4"/><rect x="13" y="13" width="7" height="7" rx="1.4"/>"#,
        "search" => r#"<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>"#,
        "minus" => r#"<path d="M5.5 12h13"/>"#,
        "plus" => r#"<path d="M12 5.5v13M5.5 12h13"/>"#,
        "close" => r#"<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>"#,
        "document" => r#"<path d="M13.5 3.5H7.5A2 2 0 0 0 5.5 5.5v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8.5z"/><path d="M13.5 3.5v5h5"/>"#,
        "fitWidth" => r#"<path d="M4 6.5v11M20 6.5v11M8 12h8M8 12l2.5-2.5M8 12l2.5 2.5M16 12l-2.5-2.5M16 12l2.5 2.5"/>"#,
        "mark" => r#"<path d="M7 4h10v16l-5-4-5 4z"/>"#,
        "window" => r#"<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M3.5 9.2h17"/>"#,
        "folder" => r#"<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.2l2 2.2h7.8A2.5 2.5 0 0 1 21 9.7v7.8A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5z"/>"#,
        "theme" => r#"<circle cx="12" cy="12" r="8.2"/><path d="M12 3.8a8.2 8.2 0 0 1 0 16.4z" fill="currentColor" stroke="none"/>"#,
        "up" => r#"<path d="M6 14.5L12 8.5l6 6"/>"#,
        "down" => r#"<path d="M6 9.5l6 6 6-6"/>"#,
        // **This one is not in `icons.ts`, and it is the only one.** Trimming
        // the margins is a chip in this reader's toolbar and lives in the
        // app's settings, so the app never needed a drawing for it. Two
        // corner brackets on the same 24px grid, in the same weight, which is
        // the shape every editor uses for a crop.
        "book" => r#"<path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H10a2 2 0 0 1 2 2v13a1.8 1.8 0 0 0-1.8-1.5H4z"/><path d="M20 5.5A1.5 1.5 0 0 0 18.5 4H14a2 2 0 0 0-2 2v13a1.8 1.8 0 0 1 1.8-1.5H20z"/>"#,
        "sidebar" => r#"<rect x="3.5" y="4.5" width="17" height="15" rx="2.2"/><path d="M10 4.5v15"/>"#,
        "keyboard" => r#"<rect x="2.8" y="6.5" width="18.4" height="11" rx="2.2"/><path d="M6.5 10h.01M9.8 10h.01M13.1 10h.01M16.4 10h.01M6.5 13.2h.01M9.8 13.2h.01M13.1 13.2h.01M16.4 13.2h.01M8.5 16h7"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="8.2"/><path d="M12 11v5.2"/><path d="M12 7.9h.01"/>"#,
        "crop" => r#"<path d="M7.5 3.5v13h13"/><path d="M3.5 7.5h13v13"/>"#,
        _ => return None,
    })
}
