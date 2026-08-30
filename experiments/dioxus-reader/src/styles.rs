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
//! `tests/reader.rs` found it on its first run. See `PROGRESS.md`; it is an
//! upstream fault and this is not merely a way around it, because a theme
//! change now re-resolves the variables instead of re-parsing the sheet.
//!
//! Three properties the app uses are missing in Blitz and are handled here
//! rather than discovered later: `position: fixed` (the root is a flex column
//! instead, so nothing needs to leave the flow), `overflow: auto` (`scroll`,
//! with `scrollbar-width: thin`), and `text-overflow: ellipsis` (a `mask-image`
//! fade, which is arguably better and was checked in Phase 0).

use crate::palette::{mix, Palette};

/// The theme, as the declarations that go in the root's `style` attribute.
///
/// Every shade the chrome uses is derived from the five colours a theme names,
/// which is the claim `applyTheme` makes in the app and the reason a five-line
/// theme file is enough.
pub fn variables(theme: &Palette) -> String {
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

/* The toolbar, then the document with the panel beside it, then the notice.
   A row inside the column, which is the whole of what a sidebar is here —
   there is nothing floating over anything. */
.body { flex: 1 1 auto; display: flex; flex-direction: row; min-height: 0; }

.viewer {
  flex: 1 1 auto; overflow: hidden; background: var(--ground);
}

.sidebar {
  position: relative; flex: 0 0 auto; display: flex; flex-direction: column;
  min-height: 0; background: var(--surface); border-right: 1px solid var(--line);
}
/* Centred on the border rather than beside it, so the grab target is wider
   than the one line it visually is. */
.sidebar-resize {
  /* `z-index` is not decoration here: Blitz only checks a positioned node
     ahead of its parent's normal-flow content during hit-testing when it
     carries a non-zero z-index (`pos_z_hoisted_children` in
     `blitz-dom/src/node/node.rs`) — without it this loses every hit test to
     `.panel`, which is later in the DOM and just as wide. */
  position: absolute; top: 0; right: -3px; width: 6px; height: 100%;
  z-index: 1; cursor: col-resize;
}
.sidebar-resize:hover { background: var(--accent); opacity: 0.35; }
.tabs { display: flex; flex: 0 0 auto; gap: 4px; padding: 8px 8px 6px 8px; }
.tab {
  flex: 1 1 auto; height: 26px; border: 0; border-radius: 8px;
  background: transparent; color: var(--muted); font-size: 13px; font-weight: 500;
}
.tab:hover { background: var(--hover); color: var(--text); }
.tab.on { background: var(--sunk); color: var(--text); }

.panel { flex: 1 1 auto; overflow: hidden; min-height: 0; }
.thumb-column { position: relative; }
.sidebar-empty { margin: 10px 12px; color: var(--faint); }

.outline-item {
  display: block; width: 100%; height: 26px; border: 0; border-radius: 7px;
  background: transparent; color: var(--muted); text-align: left;
  font-size: 13px; white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 20px), transparent);
}
.outline-item:hover { background: var(--hover); color: var(--text); }
.outline-item.current { color: var(--accent); }

.marks { padding: 4px 8px 8px 8px; border-bottom: 1px solid var(--line); }
.marks-title { margin: 2px 4px 6px 4px; color: var(--faint); }
.mark { display: flex; align-items: center; gap: 4px; }
.mark-go {
  /* `display: block` as well as `text-align`, because a button is laid out as
     a centring flex box by the user-agent sheet and the alignment inside it is
     the flex box's rather than the text's — which is why `.outline-item`, which
     says the same two things, comes out left-aligned and this did not. */
  display: block;
  flex: 1 1 auto; height: 26px; border: 0; border-radius: 7px;
  background: transparent; color: var(--text); text-align: left; font-size: 13px;
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 20px), transparent);
}
.mark-go:hover { background: var(--hover); }
.mark-drop {
  flex: 0 0 auto; width: 22px; height: 22px; border: 0; border-radius: 6px;
  background: transparent; color: var(--faint);
}
.mark-drop:hover { background: var(--hover); color: var(--text); }

/* The column takes its shape from the pictures in it, and only the rows near
   the view are here at all — see `sidebar.rs`. */
.thumbs { position: relative; width: 100%; }
.thumb {
  border: 0; background: transparent; padding: 0;
  display: flex; flex-direction: column; align-items: center;
}
.thumb-picture { background: #ffffff; box-shadow: 0 1px 3px rgba(0,0,0,0.16); }
.thumb-number { height: 18px; color: var(--faint); font-size: 12px; }
.thumb.current .thumb-number { color: var(--accent); }
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
