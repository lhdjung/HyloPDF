//! The stylesheet, themed.
//!
//! In the app this is a 2,129-line `styles.css` and a set of CSS custom
//! properties that `applyTheme` writes onto the root. Blitz reads CSS through
//! Stylo, so the file could very nearly be carried over whole; what is here is
//! the part Phase 1 puts on the screen, with the theme interpolated rather
//! than set as variables. Variables would work — Stylo has them — and the
//! interpolation is what makes it obvious that every shade comes from the five
//! colours a theme names, which is the claim `applyTheme` makes.
//!
//! Three properties the app uses are missing in Blitz and are handled here
//! rather than discovered later: `position: fixed` (the root is a flex column
//! instead, so nothing needs to leave the flow), `overflow: auto` (`scroll`,
//! with `scrollbar-width: thin`), and `text-overflow: ellipsis` (a `mask-image`
//! fade, which is arguably better and was checked in Phase 0).

use crate::theme::{mix, Theme};

pub fn sheet(theme: &Theme) -> String {
    let hex = |colour: [u8; 3]| format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2]);
    let text = hex(theme.text);
    let paper = hex(theme.background);
    let accent = hex(theme.accent);
    let surface = hex(theme.surface());
    let line = hex(theme.line());
    let muted = hex(theme.muted());
    let faint = hex(mix(theme.background, theme.text, 0.38));
    let hover = hex(mix(theme.background, theme.text, 0.10));
    let sunk = hex(mix(theme.background, theme.text, 0.05));
    // The ground the pages stand on, a shade away from the chrome so that the
    // paper has an edge without needing a border.
    let ground = hex(mix(theme.background, theme.text, 0.13));

    format!(
        r#"
body {{ margin: 0; background: {paper}; color: {text};
  font-family: ui-sans-serif, -apple-system, "Helvetica Neue", Arial, sans-serif;
  font-size: 13.5px; line-height: 1.45; }}

.root {{ display: flex; flex-direction: column; height: 100vh; }}

.toolbar {{
  display: flex; align-items: center; gap: 8px;
  height: 46px; flex: 0 0 auto; padding: 0 12px;
  background: {paper}; border-bottom: 1px solid {line};
}}
.title {{ color: {faint}; white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent); }}
.spacer {{ flex: 1 1 auto; }}

.chip {{
  height: 30px; padding: 0 11px; border-radius: 9px; border: 0;
  background: transparent; color: {muted}; font-size: 13.5px; font-weight: 500;
  white-space: nowrap;
}}
.chip:hover {{ background: {hover}; color: {text}; }}
.chip.on {{ color: {accent}; }}

.pill {{
  height: 30px; min-width: 74px; padding: 0 12px; border-radius: 9px;
  background: {sunk}; color: {muted};
  display: flex; align-items: center; justify-content: center;
}}

.viewer {{
  flex: 1 1 auto; overflow: hidden; background: {ground};
}}
/* The document: one box the size of the whole thing, with the pages placed in
   it. Blitz has no `position: static`, so an absolutely positioned node is
   placed against its immediate parent — which is exactly what this wants. */
.pages {{ position: relative; width: 100%; height: 100%; }}
.page {{ background: #ffffff; box-shadow: 0 1px 3px rgba(0,0,0,0.16), 0 8px 24px rgba(0,0,0,0.10); }}

.notice {{
  flex: 0 0 auto; height: 30px; display: flex; align-items: center;
  padding: 0 14px; background: {surface}; border-top: 1px solid {line};
  color: {muted};
}}
"#
    )
}
