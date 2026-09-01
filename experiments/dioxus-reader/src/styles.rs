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
         --muted: {}; --faint: {}; --hover: {}; --sunk: {}; --ground: {}; \
         --accent-soft: {};",
        hex(theme.text),
        hex(theme.background),
        hex(theme.accent),
        hex(theme.surface()),
        hex(theme.line()),
        hex(theme.muted()),
        hex(theme.faint()),
        hex(mix(theme.background, theme.text, 0.10)),
        hex(mix(theme.background, theme.text, 0.05)),
        // The ground the pages stand on, a shade away from the chrome so that
        // the paper has an edge without needing a border.
        hex(mix(theme.background, theme.text, 0.13)),
        // The ground a chip stands on while what it names is in force, and
        // the answer to a toolbar in which the only colour anywhere is a word
        // that has gone bright accent with nothing under it. `--accent-soft`
        // in `styles.css`, where every `on` state is this pair: the tint
        // says *this one*, and the accent on top of it is legible because the
        // tint is only a fifth of the way there.
        hex(mix(theme.background, theme.accent, 0.20)),
    ) + &format!(
        // What a page is before the renderer has reached it, which under a
        // recolouring theme is the theme's paper and otherwise is white.
        // **It used to be white either way**, and that is the whole of the
        // white flash a reader sees on a zoom step or a jump: a re-keyed page
        // is a new node with no texture, and for the frame or two before
        // pdfium answers, `.page`'s own background is what is on screen. A
        // white rectangle on a dark theme is a flash; the theme's paper is
        // the page arriving.
        " --page: {};",
        hex(theme.page()),
    ) + &format!(
        // What a match is painted in. The theme's own selection colours,
        // because a found word and a selected word are the same statement —
        // *this part of the page is the part you asked about* — and a theme
        // that has thought about one has thought about the other. The
        // current match is the accent, so that stepping through matches is
        // visible without reading the count.
        " --found: {}; --found-now: {}; --found-ink: {};",
        hex(theme.selection_area),
        hex(theme.accent),
        // The ink on a selected passage, which a theme names and otherwise
        // derives. It is here because the page field borrows it: a field
        // whose contents are all selected is drawn the way selected words are
        // drawn everywhere else in this app, in the theme's own two colours
        // rather than in whatever the platform paints a selection with.
        hex(theme.selection_text),
    )
}

/// The sheet itself, which never changes and is therefore parsed once.
pub const SHEET: &str = r#"
body { margin: 0;
  font-family: ui-sans-serif, -apple-system, "Helvetica Neue", Arial, sans-serif;
  font-size: 13.5px; line-height: 1.45; }

.root { display: flex; flex-direction: column; height: 100vh;
  background: var(--paper); color: var(--text); }

/* Every row of the window that is not the document carries `z-index`, and it
   is not decoration. Blitz paints by the rules — `.viewer` has
   `overflow: hidden` and a page scrolled past the top is clipped, which the
   screenshots show — but it **hit-tests without clipping**: a page whose box
   starts at -2789px is still hit-tested where its box is, which is over the
   toolbar and the find bar. So clicking "Done" in the find bar, with the
   document scrolled at all, landed on the page behind it and did nothing.
   `position: relative` and a `z-index` put these back in front, which is the
   same trap and the same fix as `.sidebar-resize` below, one level out. */
.toolbar, .findbar, .notice, .sidebar { position: relative; z-index: 1; }

.toolbar {
  display: flex; align-items: center; gap: 8px;
  height: 46px; flex: 0 0 auto; padding: 0 12px;
  background: var(--paper); border-bottom: 1px solid var(--line);
}
.spacer { flex: 1 1 auto; }

.chip {
  height: 30px; padding: 0 11px; border-radius: 9px; border: 0;
  background: transparent; color: var(--muted); font-size: 13.5px; font-weight: 500;
  white-space: nowrap;
}
.chip:hover { background: var(--hover); color: var(--text); }
/* A chip whose thing is in force. **The colour alone was not enough and was
   the wrong half.** Every theme in this app names a near-monochrome text
   colour — #2f3237, #e9eaee, #f8f8f2 — so a bar written in a shade of it is a
   bar of grey words whatever theme is on, and the only colour that ever
   appeared was the accent, arriving as one bright word among them with
   nothing under it. The tint is what carries the theme, and it is `.btn.on`
   in `styles.css` said exactly. */
.chip.on { background: var(--accent-soft); color: var(--accent); }
.chip.on:hover { background: var(--accent-soft); color: var(--accent); }

/* Minus, the readout, plus — one control rather than three labels, which is
   `.zoom-group` in the app. */
.zoom-group {
  display: flex; align-items: center; gap: 2px;
  padding: 2px; border-radius: 11px; background: var(--sunk);
}
.zoom-group .chip { height: 26px; border-radius: 8px; }
.zoom-group .chip.zoom-out, .zoom-group .chip.zoom-in {
  width: 26px; padding: 0; text-align: center;
}
.zoom-group .chip.fit { min-width: 74px; text-align: center; }

/* What a menu hangs off. The button is in the flow and the menu is not, so
   the row is the height of the button and the panel is under whichever
   button opened it — see `app.rs`, where this replaced a single layer pinned
   to the ends of the bar. `calc(100% + 8px)` is the toolbar's own lower edge
   read off the button rather than written down: the chip is 30px in a 46px
   row, and 8px is the space under it. */
.anchor { position: relative; display: flex; align-items: center; }

/* The menu itself, out of the flow inside its `.anchor`, so a panel taller
   than the toolbar costs the 46px row nothing.

   `z-index` is not decoration: Blitz hit-tests a positioned node ahead of
   what it is painted over only when it carries a non-zero one, and a menu
   that cannot be clicked is worse than no menu. 5 rather than 1 because it
   is over the toolbar as well as over the document. */
.menu {
  position: absolute; top: calc(100% + 8px); z-index: 5; min-width: 190px;
  padding: 6px; border-radius: 12px;
  border: 1px solid var(--line); background: var(--surface);
}
/* Under the left edge of the button, except the two near the right end of the
   bar, which would otherwise run off it. */
.menu.document, .menu.view { left: 0; }
.menu.theme { right: 0; }
/* Fourteen themes is taller than a short window, and this is the one list in
   the app that is a list rather than a handful. */
.menu.theme { max-height: 60vh; overflow: scroll; scrollbar-width: thin; }
/* No `width: 100%`. A menu is absolutely positioned and therefore shrinks to
   fit, and a percentage width inside a shrink-to-fit box is resolved against
   a width computed as though the percentage were not there — so the widest
   row set the menu's width and every other row was then laid out one row too
   narrow, which showed as the chord on the right clipped by four pixels.
   These are block-level flex containers and fill the line on their own. */
.menu-item {
  display: flex; align-items: center; gap: 8px;
  height: 30px; padding: 0 8px; border: 0; border-radius: 8px;
  background: transparent; color: var(--text); font-size: 13.5px;
  text-align: left;
}
.menu-item:hover { background: var(--hover); }
.menu-item.on { color: var(--accent); }
/* A column of its own so the labels line up whether or not a row is ticked. */
.menu-tick { flex: 0 0 12px; color: var(--accent); }
.menu-label { flex: 1 1 auto; white-space: nowrap; }
/* The chord, read off the keymap rather than written here — see
   `Viewer::chord_for`. Quiet, because it is an aside and not the item. */
.menu-key { flex: 0 0 auto; color: var(--faint); font-size: 12.5px; }
.menu-rule { height: 1px; margin: 5px 8px; background: var(--line); }

/* The title is a button now, because the document's menu hangs off it. It
   keeps the chip's shape and the title's own colour and truncation. */
.chip.title {
  flex: 0 1 auto; min-width: 0; color: var(--faint);
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent);
}
.chip.title:hover { color: var(--text); }

/* The page and the count, which is `.page-jump` in the app: a box you can
   type in, and the total beside it rather than inside it. The pair used to be
   one sunk pill with the number pushed against its left wall and the count
   after it, and there was no way to tell the two apart. */
.pill {
  height: 30px; padding: 0;
  display: flex; align-items: center; gap: 6px;
}
/* The number, typed. The unit is deliberately *not* in the field — the app
   learned that the hard way: a field reading "16 px" puts the caret wherever
   the pointer landed, so typing 30 gives "3016 px". Here the "/ 400" sits
   beside it in a span of its own for the same reason.

   Centred, which it was not: `text-align: right` was written here and Blitz
   lays a text input out from its leading edge whatever it says, so the number
   sat against the left wall of a 44px box — the "cramped" of the complaint,
   and a thing no test that reads the field's *value* can see. Centring is
   what the app does anyway. */
.page-field, .page-now {
  box-sizing: border-box;
  height: 28px; padding: 0 6px;
  border: 1px solid var(--line); border-radius: 7px;
  background: var(--sunk); color: var(--text); font-size: 13.5px;
  text-align: center;
}
.page-now:hover { border-color: var(--muted); }
/* The platform's focus ring is a blue rounded box that belongs to no theme in
   this app — under Hylo Ember it is the one cold thing on screen. The border
   says the same thing in the theme's own accent, which is
   `.page-jump input:focus` in `styles.css`. */
.page-field:focus { outline: none; border-color: var(--accent); }
/* And all of it selected, which is the state a page field opens in. There is
   no real selection under it — parley will select-all for a keystroke and for
   nothing else, so `Viewer::page_fresh` is the app's own emulation — and this
   is what makes the emulation *visible*: the theme's selection colours, the
   same pair a swept passage on the page is drawn in. Without it the field
   opened looking like a field somebody had merely clicked into, and the first
   digit replacing the whole number came as a surprise. */
.page-field.fresh {
  background: var(--found); color: var(--found-ink); border-color: var(--accent);
}
.of { color: var(--faint); font-size: 13.5px; }

/* The find bar: a row of the column, so the document is what gets shorter.
   Nothing here is over anything, which is the one place Blitz's missing
   `position: fixed` made the layout simpler rather than harder. */
.findbar {
  display: flex; align-items: center; gap: 6px;
  height: 40px; flex: 0 0 auto; padding: 0 12px;
  background: var(--surface); border-bottom: 1px solid var(--line);
}
.find-field {
  flex: 0 1 320px; height: 28px; padding: 0 10px;
  border: 1px solid var(--line); border-radius: 8px;
  background: var(--paper); color: var(--text); font-size: 13.5px;
}
.find-count { flex: 0 0 auto; min-width: 96px; color: var(--faint); }
.chip.find-previous, .chip.find-next { min-width: 30px; padding: 0 8px; }

/* A match, as a rectangle over the page. Behind the ink rather than over it:
   `mix-blend-mode` is out (see `AGENTS.md`, and Blitz has none), and an
   opaque box over a word hides the word. Blitz composites the widget's
   texture and this on top of it, so the alpha is what keeps the type
   readable — 0.38 is where a highlight is plainly there on paper and does not
   grey the letters on a dark theme. */
.hit { background: var(--found); opacity: 0.38; border-radius: 2px; }
.hit.current { background: var(--found-now); opacity: 0.45; }

/* What the reader has swept over. The same rectangle as a match and the same
   colour, because in this app's themes they are the same colour: `--found` is
   the theme's `selection_area`, and a found word and a selected word are the
   same statement about the page. A little stronger than a match, because a
   selection is the thing the reader is doing right now and a match is a thing
   the document was asked about a minute ago. */
.selected { background: var(--found); opacity: 0.45; border-radius: 2px; }

/* A link, as the area the document says it is. Nothing is drawn: the colour
   of a link is the *page's* business and is baked into the bitmap, exactly as
   `tintLinks` bakes it in the app — a rectangle drawn over the words would be
   the second thing saying so and the one that could disagree.

   `z-index` is not decoration. Blitz only hit-tests a positioned node ahead
   of its parent's normal-flow content when it carries a non-zero one, and
   this has to come out in front of the page's own widget — which is exactly
   the trap `.sidebar-resize` fell into, one level in. Above `.hit` for the
   same reason it is above the widget: where a match falls on a
   cross-reference, following the link is the gesture that meant something. */
.link { z-index: 2; cursor: pointer; }

/* Words can be swept, so the pointer says so over a page — and says the other
   thing over a link, which is the rule above winning by coming after it. */
.page { cursor: text; }

/* The toolbar, then the document with the panel beside it, then the notice.
   A row inside the column, which is the whole of what a sidebar is here —
   there is nothing floating over anything. */
.body { flex: 1 1 auto; display: flex; flex-direction: row; min-height: 0; }

.viewer {
  flex: 1 1 auto; overflow: hidden; background: var(--ground);
}

/* `box-sizing` so that the panel is exactly as wide as it says it is. The
   hairline down its right is a border, and a content box put it *outside* the
   width — so the document was laid out for a viewport one pixel wider than
   the box it was drawn into, and every page came out a pixel over its right
   edge and flush against its left. One pixel, and it is the same fault as the
   window resize above in miniature: the layout's idea of the viewport and the
   viewport disagreeing. */
.sidebar {
  flex: 0 0 auto; display: flex; flex-direction: column; box-sizing: border-box;
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

/* The results list. A row is two lines' worth of text on one line, cut off at
   the end rather than wrapped: what places a match is the words in front of
   it, and the sentence behind it is there to be recognised, not read. */
.results { padding: 4px 8px 8px 8px; overflow: scroll; scrollbar-width: thin; }
.results-count { margin: 2px 4px 6px 4px; color: var(--faint); }
.result {
  display: flex; align-items: baseline; gap: 8px; width: 100%;
  border: 0; border-radius: 7px; padding: 5px 6px; margin-bottom: 1px;
  background: transparent; text-align: left; font-size: 13px;
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent);
}
.result:hover { background: var(--hover); }
.result.current { background: var(--sunk); }
.result-page { flex: 0 0 auto; color: var(--faint); font-size: 12px; }
.result-line { flex: 1 1 auto; color: var(--muted); }
/* `pre`, because the space either side of the match is the whole difference
   between a line that reads as a sentence and "A**needle**in the first page":
   HTML collapses whitespace at the edge of an inline run, and these two runs
   are cut out of the document precisely at those edges. `results()` keeps a
   single space there deliberately — see `search.rs`. */
.result-before, .result-after { white-space: pre; }
.result-hit { color: var(--text); font-weight: 600; }

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
.thumb-picture { background: var(--page); box-shadow: 0 1px 3px rgba(0,0,0,0.16); }
.thumb-number { height: 18px; color: var(--faint); font-size: 12px; }
.thumb.current .thumb-number { color: var(--accent); }
/* The document: one box the size of the whole thing, with the pages placed in
   it. Blitz has no `position: static`, so an absolutely positioned node is
   placed against its immediate parent — which is exactly what this wants. */
.pages { position: relative; width: 100%; height: 100%; }
/* `--page`, not white. A page whose texture has not arrived draws as this
   and nothing else, and every re-key — a zoom step, a jump, a theme, a turn —
   makes a page whose texture has not arrived. See `variables` above. */
.page { background: var(--page); box-shadow: 0 1px 3px rgba(0,0,0,0.16), 0 8px 24px rgba(0,0,0,0.10); }

.notice {
  flex: 0 0 auto; height: 30px; display: flex; align-items: center;
  padding: 0 14px; background: var(--surface); border-top: 1px solid var(--line);
  color: var(--muted);
}

/* Presenting: full screen with nothing else on it. The chrome is gone from
   the DOM rather than hidden here — see `Viewer::chrome`, which is what gives
   the document the room the toolbar was using — so all that is left for CSS
   is the ground. It is the theme's paper rather than its `--ground`: with
   nothing else on screen the frame around the page is the only thing left
   that is not the page, and the darker shade reads as a border on a window
   that has none. */
.root.presenting .viewer { background: var(--paper); }
"#;
