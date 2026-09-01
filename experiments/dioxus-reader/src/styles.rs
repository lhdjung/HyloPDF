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
    let hex = crate::palette::hex;
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
        // The theme's paper with an alpha on it, which is what the drop hint
        // is drawn over: the window has to stay visible under it. Written as
        // an eight-digit hex rather than through `color-mix`, so that what
        // reaches the renderer is a colour and not a function it may or may
        // not implement.
        " --veil: {}e0;",
        hex(theme.background),
    ) + &format!(
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

 /* `position: relative` so that the Settings scrim, which is absolute, is
   measured against the window rather than against whatever Blitz would
   otherwise pick — without it the scrim started below the toolbar and the bar
   stayed bright behind a window that claims to be modal. */
.root { position: relative; display: flex; flex-direction: column; height: 100vh;
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
.toolbar, .findbar { position: relative; z-index: 2; }
.sidebar { position: relative; z-index: 1; }

.toolbar {
  display: flex; align-items: center; gap: 8px;
  height: 46px; flex: 0 0 auto; padding: 0 12px;
  background: var(--paper); border-bottom: 1px solid var(--line);
}
/* **Three groups, and the middle one is why there are three.** The bar was
   one flat row with a `.spacer` in it, so the page readout sat wherever the
   row ran out of chips — which was the far right, beside the cog. The app's
   own arrangement, and its reasoning: the two side groups start from what
   they hold and share the slack, which keeps the page controls near the
   middle without promising they are in it. When there is not enough room for
   everything the left gives way, because it is the side with a title in it
   and a title has an ellipsis to shrink into. */
.bar-group { display: flex; align-items: center; gap: 8px; min-width: 0; }
.bar-left { flex: 1 1 auto; }
.bar-center { flex: 0 0 auto; }
.bar-right { flex: 1 0 auto; justify-content: flex-end; }

.chip {
  display: flex; align-items: center; gap: 7px;
  height: 30px; padding: 0 11px; border-radius: 9px; border: 0;
  background: transparent; color: var(--muted); font-size: 13.5px; font-weight: 500;
  white-space: nowrap;
}
.icon { flex: 0 0 auto; }
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
.menu.document, .menu.open, .menu.view { left: 0; }
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
/* …and a drawing where a shelf row has one, which needs the four extra
   pixels an icon is wider than a tick. */
.menu-tick .icon { width: 16px; height: 16px; margin-left: -2px; }
.menu-label { flex: 1 1 auto; white-space: nowrap; }
/* The chord, read off the keymap rather than written here — see
   `Viewer::chord_for`. Quiet, because it is an aside and not the item. */
.menu-key { flex: 0 0 auto; color: var(--faint); font-size: 12.5px; }
.menu-rule { height: 1px; margin: 5px 8px; background: var(--line); }
/* A heading over a run of items — the app's `ui.section`, and the Document
   menu's shelf is the one place that has one. Quieter and a size smaller than
   an item, and it is not a row you can point at. */
.menu-section {
  padding: 6px 8px 4px; color: var(--faint); font-size: 12px;
}

/* The title is a button now, because the document's menu hangs off it. It
   keeps the chip's shape and the title's own colour and truncation. */
/* Icon-only, so they get the zoom group's square rather than a chip's
   word-shaped one. */
.chip.page-previous, .chip.page-next { width: 30px; padding: 0; justify-content: center; }

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
/* The count, and the way to the list behind it — `Viewer::show_results`. It
   is a button, so it wears a button's clothes: transparent until there is
   something to show, and then a hover, which is `.find-status:not(:empty)` in
   the app. `.ready` rather than `:empty`, which Blitz does not implement. */
.find-count {
  flex: 0 0 auto; min-width: 96px; height: 26px; padding: 0 8px;
  border: 0; border-radius: 7px;
  background: transparent; color: var(--faint); font-size: 13.5px;
  justify-content: flex-start;
}
.find-count.ready:hover { background: var(--hover); color: var(--text); }
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

/* The toolbar, then the document with the panel beside it. A row inside the
   column, which is the whole of what a sidebar is here — there is nothing
   floating over anything.

   **`z-index: 0` is load-bearing and the reason is a rule two paragraphs up.**
   Blitz gathers every z-indexed box into the nearest stacking context and
   hit-tests that context only when the point falls inside the *union of the
   boxes it holds* — and a toolbar menu is inside `.toolbar`'s own context, so
   the root's union is the toolbar's 47px strip and nothing below it. A click
   on a menu item hanging under the bar therefore missed the menu entirely and
   landed on the page behind it, which reads as a menu that closes when you
   choose from it and does nothing else.

   It used to work by accident: the notice was a 30px row along the foot of
   the window carrying `z-index: 1` for the same hit-testing reason, so the
   root's union ran from the top of the toolbar to the bottom of the notice
   and happened to contain every menu. Taking that row away — the notice is a
   pill over the document now — took the menus with it, which is a fault
   nothing about a notice line should be able to cause.

   So the document says out loud what it was relying on: it is a layer of its
   own, under the toolbar's, and the union of the two is the window. */
.body { position: relative; z-index: 1; flex: 1 1 auto; display: flex; flex-direction: row; min-height: 0; }

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
.tabs { display: flex; flex: 0 0 auto; gap: 4px; padding: 8px 8px 6px 8px; min-width: 0; }
/* **Three of these have to fit across a panel the reader can drag to any
   width, and a flex item does not shrink below its content unless it is told
   it may.** `flex: 1 1 auto` with no `min-width` is that rule not being told:
   the three tabs kept the width of their words, the strip overflowed the
   panel, and the third one — Results, which is the one that comes and goes —
   was drawn *outside* the sidebar, over the document. Which is not only ugly.
   Blitz hit-tests where a box is rather than where it is painted, and the
   document's own layer is over the panel out there, so the tab could be seen
   and could not be clicked: "I can't go back to Results after clicking
   somewhere else" was this, and nothing to do with the tab.

   `flex: 1 1 0` and `min-width: 0` are what the app's own `.tab` carries, for
   the same three tabs and the same reason. The word gives way and the icon
   does not — a tab with half an icon on it reads as a rendering fault and a
   tab with a shortened word reads as a narrow panel — which is why the label
   is a span of its own and the shrinking happens there. */
.tab {
  display: flex; align-items: center; justify-content: center; gap: 6px;
  flex: 1 1 0; min-width: 0; overflow: hidden;
  height: 26px; padding: 0 6px; border: 0; border-radius: 8px;
  background: transparent; color: var(--muted); font-size: 13px; font-weight: 500;
}
.tab .icon { flex: 0 0 auto; }
/* `text-overflow: ellipsis` is not implemented — see the note at the top of
   this file — so the word fades out instead, the same mask `.chip.title` and
   `.result` use. */
.tab-label {
  flex: 0 1 auto; min-width: 0; overflow: hidden; white-space: nowrap;
  mask-image: linear-gradient(to right, #000 calc(100% - 10px), transparent);
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
/* `justify-content` and the `min-width: 0` on the line below it are the same
   fault as `.tab` above, one panel down, and they showed as something
   stranger: the page number went missing. A button is laid out as a *centring*
   flex box by the user-agent sheet, and `.result-line` would not shrink, so a
   row wider than a narrow panel overflowed at **both** ends — taking the
   number off the left while the mask faded the quote on the right. The number
   was there all along and was outside the panel. */
.result {
  display: flex; align-items: baseline; justify-content: flex-start; gap: 8px;
  width: 100%; min-width: 0;
  border: 0; border-radius: 7px; padding: 5px 6px; margin-bottom: 1px;
  background: transparent; text-align: left; font-size: 13px;
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent);
}
.result:hover { background: var(--hover); }
.result.current { background: var(--sunk); }
.result-page { flex: 0 0 auto; color: var(--faint); font-size: 12px; }
.result-line { flex: 1 1 auto; min-width: 0; overflow: hidden; color: var(--muted); }
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

/* The passages, under the pages. A row is a colour, the words themselves, and
   the way to take it off — the colour first, because that is what a reader
   picked and is how they will find the one they are looking for. */
.markup { margin-top: 10px; }
.markup-row { align-items: flex-start; }
.markup-dot {
  flex: 0 0 auto; width: 10px; height: 10px; margin-top: 8px; border-radius: 3px;
}
/* Two lines of the quote and then it stops. `text-overflow: ellipsis` is not
   implemented in Blitz — see the note on `.chip.title` — and a mask is what
   this file uses instead everywhere it wants the same thing. */
.markup-row .mark-go {
  height: auto; min-height: 26px; padding: 5px 6px; line-height: 1.35;
  white-space: normal; max-height: 40px;
  mask-image: linear-gradient(to bottom, #000 calc(100% - 12px), transparent);
}
.markup-restore {
  display: block; width: 100%; margin: 0 0 8px 0; padding: 6px 8px;
  border: 1px solid var(--line); border-radius: 8px;
  background: var(--accent-soft); color: var(--accent);
  font-size: 12px; text-align: left;
}
.markup-restore:hover { border-color: var(--accent); }
.markup-beside {
  display: block; color: var(--faint); font-size: 11px; margin-top: 2px;
}

/* The colour popover, over the passage it is about. It is not in `#popovers`
   and there is no such thing here — it belongs to the page, because the
   rectangle it is placed against is the page's. */
.markup-popover {
  display: flex; gap: 6px; padding: 7px; z-index: 6;
  background: var(--surface); border: 1px solid var(--line); border-radius: 11px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.18);
}
.markup-swatch {
  width: 22px; height: 22px; border: 1px solid var(--line); border-radius: 7px;
  padding: 0;
}
.markup-swatch:hover { border-color: var(--accent); }

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

/* A document being dragged over the window — the app's `#drop-hint`, and the
   half of "or drop a PDF anywhere in this window" that makes the sentence
   true. Over the whole window rather than over the document, because that is
   what the sentence promises, and `z-index` under Settings alone: a drag over
   a window whose Settings are open is not a drag onto the document. */
.drop-hint {
  position: absolute; top: 10px; left: 10px; right: 10px; bottom: 10px;
  z-index: 15; display: flex; align-items: center; justify-content: center;
  /* The app's own shape, and reading with it is what settled it: a solid
     ground reads as a curtain drawn over the window, and the one thing
     somebody dragging a file wants to see is the window they are dragging it
     onto. So it is a dashed border inset from the edges — the window itself
     saying it will catch this — over a veil of the theme's paper rather than
     the paper itself. `--veil` is that colour with an alpha on it, mixed in
     `variables` rather than written as `color-mix`, which Stylo's support for
     is not something to find out about from a screenshot. */
  border: 2px dashed var(--accent); border-radius: 16px;
  background: var(--veil); color: var(--accent);
  font-size: 16px; font-weight: 500;
}
/* Something that will not open. The same frame, so the hint does not jump
   when a folder crosses the window, and the theme's own muted ink rather than
   a red of its own: fourteen themes have no error colour between them, and
   inventing one here would be the only colour in this application that no
   theme chose. */
.drop-hint.refused { border-color: var(--faint); color: var(--muted); }
/* The word itself carries no ground of its own; it is the frame that says
   what will happen and the word that says what it is. */
.drop-hint-word { padding: 0 8px; }

/* -------------------------------------------------- the window with nothing
   in it

   The app's `#welcome`, and it stands where the document would. Centred in
   both axes, on the theme's own paper rather than on `--ground`: the ground
   is the shade a page stands on, and with no page there is nothing for it to
   set off. */
.start {
  flex: 1 1 auto; display: flex; align-items: center; justify-content: center;
  background: var(--paper); overflow: scroll; scrollbar-width: thin;
}
/* The app's `min(460px, 82vw)`. Blitz resolves `min()` and `vw`, and the
   narrow half matters: a window dragged down to its minimum still has a list
   in it rather than a list with its right-hand column off the edge. */
.start-inner { width: min(460px, 82vw); }
.start-name {
  margin: 0; text-align: center;
  font-size: 30px; font-weight: 600; letter-spacing: -0.01em; color: var(--text);
}
.start-sub {
  margin: 6px 0 22px; text-align: center; color: var(--muted); font-size: 15.5px;
}
/* The one filled button in this application, and the app's `.btn-primary`.
   Everything else in the chrome is a quiet chip on a transparent ground,
   which is right for a bar of them and wrong for a screen whose whole purpose
   is one action. */
.start-open {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  width: 100%; height: 38px; border: 0; border-radius: 10px;
  background: var(--accent); color: var(--paper);
  font-size: 14px; font-weight: 500;
}
.start-hint { margin-top: 22px; text-align: center; color: var(--faint); }

.recents { margin-top: 26px; }
.recents-title { padding: 0 8px 6px; color: var(--muted); font-size: 13px; }
/* A row is the button and the × beside it, and the × is a sibling rather than
   a child: a button inside a button is not a shape either the DOM or a
   pointer knows what to do with, and the app gets away with a `<span>` there
   only because it is listening for a click and stopping it. */
.recent { display: flex; align-items: center; border-radius: 9px; }
.recent:hover { background: var(--hover); }
.recent-open {
  display: flex; align-items: center; gap: 10px;
  flex: 1 1 auto; min-width: 0; height: 34px; padding: 0 4px 0 10px;
  border: 0; background: transparent; color: var(--text); font-size: 13.5px;
  text-align: left;
}
/* The name takes what is left and the page number keeps its column. `min-width:
   0` on both this and the row above it, because a flex item's floor is its
   content and a long title would otherwise push the page number off the end
   rather than being cut. */
.recent-name { flex: 1 1 auto; min-width: 0; overflow: hidden; white-space: nowrap; }
/* Right-aligned and tabular, so a three-digit page lines up with a one-digit
   one. Quieter and a size smaller than the name: it is a page reference in
   the margin rather than part of the title. */
.recent-page {
  flex: 0 0 auto; min-width: 3.4em; text-align: right;
  color: var(--faint); font-size: 12.5px; font-variant-numeric: tabular-nums;
}
/* Always there rather than revealed on hover, which is where this parts
   company with the app. `.recent:hover .recent-forget { visibility: visible }`
   is a rule about an ancestor's state, and Stylo resolves it correctly — but
   Blitz takes a state-only snapshot of the node the pointer is on, so the
   descendant is not re-resolved and the × appears on the first hover after
   something else forces a restyle rather than on this one. A control that
   appears late is worse than a control that is quietly always there. */
.recent-forget {
  flex: 0 0 auto; display: flex; align-items: center;
  height: 26px; padding: 0 8px; margin-right: 4px;
  border: 0; border-radius: 7px; background: transparent; color: var(--faint);
}
.recent-forget:hover { background: var(--sunk); color: var(--text); }

/* What the app says out loud, and it says it over the document rather than
   under it. This was a 30px row of the flex column, which cost the document
   that much whether or not there was anything to say and left the last thing
   said — usually a zoom percentage — along the bottom edge of the window for
   the rest of the session.

   The row is what centres the pill: `left: 50%` and a `translateX(-50%)` is
   how the app does it, and a flex row that fills the width does the same with
   no transform to trust. It takes no clicks, so a message over the foot of a
   page does not swallow a press on the page. */
.notice-line {
  position: absolute; left: 0; right: 0; bottom: 20px; z-index: 45;
  display: flex; align-items: center; justify-content: center;
  pointer-events: none;
}
.notice {
  max-width: 70%; padding: 9px 16px; border-radius: 999px;
  background: var(--surface); border: 1px solid var(--line);
  color: var(--text);
}

/* Presenting: full screen with nothing else on it. The chrome is gone from
   the DOM rather than hidden here — see `Viewer::chrome`, which is what gives
   the document the room the toolbar was using — so all that is left for CSS
   is the ground. It is the theme's paper rather than its `--ground`: with
   nothing else on screen the frame around the page is the only thing left
   that is not the page, and the darker shade reads as a border on a window
   that has none. */
.root.presenting .viewer { background: var(--paper); }

/* --------------------------------------------------------- the Settings window

   A scrim and a frame in the same document, which is `showWindow` in `ui.ts`.
   Blitz has no `position: fixed`, so the scrim is absolute against the root —
   which is a flex column filling the window, so the two come to the same
   thing. `z-index` above the menus, because Settings is over everything
   including the bar it was opened from. */
.window-scrim {
  position: absolute; top: 0; left: 0; right: 0; bottom: 0; z-index: 20;
  display: flex; align-items: center; justify-content: center;
  background: rgba(0,0,0,0.34);
}
.window {
  display: flex; flex-direction: column;
  width: 860px; height: 600px; max-width: 92%; max-height: 92%;
  border-radius: 16px; border: 1px solid var(--line);
  background: var(--paper); color: var(--text);
  box-shadow: 0 24px 64px rgba(0,0,0,0.28);
}
.window-bar {
  display: flex; align-items: center; flex: 0 0 auto;
  height: 46px; padding: 0 10px 0 18px; border-bottom: 1px solid var(--line);
}
.window-title { flex: 1 1 auto; font-size: 14px; font-weight: 600; }
.chip.window-close { width: 30px; padding: 0; justify-content: center; }
.window-body { flex: 1 1 auto; display: flex; flex-direction: row; min-height: 0; }
.window-nav {
  flex: 0 0 188px; display: flex; flex-direction: column; gap: 2px;
  padding: 12px 10px; border-right: 1px solid var(--line); background: var(--surface);
}
.nav-item {
  display: flex; align-items: center; gap: 9px;
  height: 32px; padding: 0 10px; border: 0; border-radius: 9px;
  background: transparent; color: var(--muted); font-size: 13.5px; font-weight: 500;
  text-align: left;
}
.nav-item:hover { background: var(--hover); color: var(--text); }
/* The same pair as a chip in force: the tint carries it and the accent is
   legible on the tint. See `.chip.on`. */
.nav-item.on { background: var(--accent-soft); color: var(--accent); }
/* `scroll`, not `auto` — Blitz has no `auto`, which is the note at the top of
   this file. Reading is the longest page and does not fit in 600px. */
.window-pane {
  flex: 1 1 auto; min-width: 0; padding: 18px 22px 24px 22px;
  overflow: scroll; scrollbar-width: thin;
}
.pane-title { margin: 0 0 4px 0; font-size: 19px; font-weight: 600; }
.pane-lede { margin: 0 0 12px 0; color: var(--faint); font-size: 14px; }
.pane-group {
  margin: 22px 0 8px 0; font-size: 12.5px; font-weight: 600;
  color: var(--faint);
}
.pane-note { margin: 0 0 12px 0; color: var(--faint); font-size: 13px; line-height: 1.5; }
.pane-actions { display: flex; gap: 8px; margin-top: 16px; }
.chip.action {
  border: 1px solid var(--line); background: var(--surface); color: var(--text);
}
.chip.action:hover { background: var(--hover); }

/* One setting. The control sits on the same line as the name and the sentence
   runs under both, which is what keeps a page of switches readable as prose
   rather than as a form. */
.field { padding: 12px 0; border-bottom: 1px solid var(--line); }
.field-head { display: flex; align-items: center; gap: 16px; }
.field-label { flex: 1 1 auto; font-size: 14px; }
.field-control { flex: 0 0 auto; display: flex; align-items: center; }
.field-note { margin: 6px 0 0 0; color: var(--faint); font-size: 12.5px; line-height: 1.5; }

.switch {
  width: 40px; height: 23px; padding: 2px; border: 0; border-radius: 12px;
  background: var(--sunk); display: flex; align-items: center;
}
.switch.on { background: var(--accent); }
.switch-knob {
  width: 19px; height: 19px; border-radius: 10px; background: var(--paper);
  box-shadow: 0 1px 2px rgba(0,0,0,0.24);
}
/* The knob moves by being pushed, not by being positioned: Blitz has the box
   model and this needs nothing else. */
.switch.on .switch-knob { margin-left: 17px; }

.segmented {
  display: flex; gap: 2px; padding: 2px; border-radius: 10px; background: var(--sunk);
}
.segment {
  height: 26px; padding: 0 10px; border: 0; border-radius: 8px;
  background: transparent; color: var(--muted); font-size: 13px; font-weight: 500;
  white-space: nowrap;
}
.segment:hover { color: var(--text); }
.segment.on { background: var(--paper); color: var(--text); }

.stepper {
  display: flex; align-items: center; gap: 2px;
  padding: 2px; border-radius: 10px; background: var(--sunk);
}
.stepper .chip { height: 26px; width: 26px; padding: 0; justify-content: center; }
/* The number, typed. Sized to its digits for the reason `.page-field` is:
   Blitz gives parley no alignment for an input's text, so a box wider than
   what is in it is a number against its left wall. */
.step-field {
  box-sizing: border-box;
  height: 26px; padding: 0 6px; border: 0; border-radius: 7px;
  background: transparent; color: var(--text); font-size: 13.5px;
}
.step-field:focus { outline: none; background: var(--paper); }
/* All of it selected, drawn the way the page field draws the same emulated
   state — the theme's own selection colours. */
.step-field.fresh { background: var(--found); color: var(--found-ink); }
.step-unit { color: var(--faint); font-size: 12.5px; padding-right: 4px; }

/* The theme list: a swatch of the three colours that decide what a theme
   looks like, and its name. Resolved through `parseColor` before they get
   here — a swatch showing a colour the renderer cannot read is the picker
   lying about the page. */
.theme-grid { display: flex; flex-wrap: wrap; gap: 10px; }
.theme-card {
  display: flex; flex-direction: column; gap: 6px;
  box-sizing: border-box; width: 132px; padding: 8px;
  border: 1px solid var(--line); border-radius: 12px;
  background: transparent; color: var(--text); text-align: left;
}
.theme-card:hover { background: var(--hover); }
.theme-card.on { border-color: var(--accent); background: var(--accent-soft); }
/* `align-self` and `box-sizing`, both load-bearing: a flex item in a column
   does not stretch here on its own, and the padding and the hairline would
   otherwise be added to the 52. */
.theme-swatch {
  display: flex; align-items: flex-end; align-self: stretch; gap: 5px;
  box-sizing: border-box;
  height: 52px; padding: 8px; border-radius: 8px; border: 1px solid var(--line);
}
.swatch-ink { flex: 1 1 auto; height: 6px; border-radius: 3px; }
.swatch-accent { flex: 0 0 18px; height: 6px; border-radius: 3px; }
.theme-name { font-size: 13px; font-weight: 500; }

/* The Keyboard page's table: two columns, and the chord in the quiet shade
   because it is the answer and the action is the question. */
.keys { display: flex; flex-wrap: wrap; }
.key-what { flex: 0 0 62%; padding: 5px 0; font-size: 13.5px; }
.key-chord { flex: 0 0 38%; padding: 5px 0; color: var(--faint); font-size: 13px; }
"#;
