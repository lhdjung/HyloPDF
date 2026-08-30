//! The stylesheet, and the theme as fifteen variables written onto the root.
//!
//! In the app this is a 2,129-line `styles.css` and a set of CSS custom
//! properties that `applyTheme` writes onto the root. Blitz reads CSS through
//! Stylo, so the file could very nearly be carried over whole; what is here is
//! the part Phase 1 puts on the screen.
//!
//! **The theme was interpolated into the sheet and is now a set of variables,
//! and the reason is a crash rather than a preference.** A `<style>` element
//! whose text changes is a stylesheet mutation, and Stylo answers one by
//! walking the tree with `StylesheetInvalidationSet` — which calls
//! `each_class` on any element *snapshot* it finds on the way. Blitz takes a
//! cheap, state-only snapshot when something is hovered or pressed
//! (`snapshot_node_state_only`), and that snapshot has no attributes in it, and
//! `ServoElementSnapshot::each_class` unwraps them. So *clicking* the button
//! that changes the theme panicked in `stylo`, from a stack with nothing of
//! this app in it, while pressing `t` for the same action was fine — the
//! difference being only that a click leaves the pointer on a button.
//! `tests/reader.rs` found it on its first run. See `PHASE2.md`; it is an
//! upstream fault and this is not merely a way around it, because a theme
//! change now re-resolves the variables instead of re-parsing the sheet.
//!
//! Three properties the app uses are missing in Blitz and are handled here
//! rather than discovered later: `position: fixed` (the root is a flex column
//! instead, so nothing needs to leave the flow), `overflow: auto` (`scroll`,
//! with `scrollbar-width: thin`), and `text-overflow: ellipsis` (a `mask-image`
//! fade, which is arguably better and was checked in Phase 0).

use crate::theme::{mix, Theme};

/// The theme, as the declarations that go in the root's `style` attribute.
///
/// Every shade the chrome uses is derived from the five colours a theme names,
/// which is the claim `applyTheme` makes in the app and the reason a five-line
/// theme file is enough.
pub fn variables(theme: &Theme) -> String {
    let hex = |colour: [u8; 3]| format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2]);
    format!(
        "--text: {}; --paper: {}; --accent: {}; --surface: {}; --line: {}; \
         --muted: {}; --faint: {}; --hover: {}; --sunk: {}; --ground: {};",
        hex(theme.text),
        hex(theme.background),
        hex(theme.accent),
        hex(theme.surface()),
        hex(theme.line()),
        hex(theme.muted()),
        hex(mix(theme.background, theme.text, 0.38)),
        hex(mix(theme.background, theme.text, 0.10)),
        hex(mix(theme.background, theme.text, 0.05)),
        // The ground the pages stand on, a shade away from the chrome so that
        // the paper has an edge without needing a border.
        hex(mix(theme.background, theme.text, 0.13)),
    )
}

/// The sheet itself, which never changes and is therefore parsed once.
pub const SHEET: &str = r#"
body { margin: 0;
  font-family: ui-sans-serif, -apple-system, "Helvetica Neue", Arial, sans-serif;
  font-size: 13.5px; line-height: 1.45; }

.root { display: flex; flex-direction: column; height: 100vh;
  background: var(--paper); color: var(--text); }

.toolbar {
  display: flex; align-items: center; gap: 8px;
  height: 46px; flex: 0 0 auto; padding: 0 12px;
  background: var(--paper); border-bottom: 1px solid var(--line);
}
.title { color: var(--faint); white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent); }
.spacer { flex: 1 1 auto; }

.chip {
  height: 30px; padding: 0 11px; border-radius: 9px; border: 0;
  background: transparent; color: var(--muted); font-size: 13.5px; font-weight: 500;
  white-space: nowrap;
}
.chip:hover { background: var(--hover); color: var(--text); }
.chip.on { color: var(--accent); }

.pill {
  height: 30px; min-width: 74px; padding: 0 12px; border-radius: 9px;
  background: var(--sunk); color: var(--muted);
  display: flex; align-items: center; justify-content: center;
}

.viewer {
  flex: 1 1 auto; overflow: hidden; background: var(--ground);
}
/* The document: one box the size of the whole thing, with the pages placed in
   it. Blitz has no `position: static`, so an absolutely positioned node is
   placed against its immediate parent — which is exactly what this wants. */
.pages { position: relative; width: 100%; height: 100%; }
.page { background: #ffffff; box-shadow: 0 1px 3px rgba(0,0,0,0.16), 0 8px 24px rgba(0,0,0,0.10); }

.notice {
  flex: 0 0 auto; height: 30px; display: flex; align-items: center;
  padding: 0 14px; background: var(--surface); border-top: 1px solid var(--line);
  color: var(--muted);
}
"#;
