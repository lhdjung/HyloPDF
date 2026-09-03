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

use crate::palette::Palette;

/// The theme, as the declarations that go in the root's `style` attribute.
///
/// Every shade the chrome uses is derived from the five colours a theme names,
/// which is the claim `applyTheme` makes in the app and the reason a five-line
/// theme file is enough.
pub fn variables(theme: &Palette) -> String {
    let hex = crate::palette::hex;
    format!(
        // **Every one of these is `applyTheme`'s own arithmetic** — see
        // `Palette`, where each is named after the variable it stands for in
        // `styles.css`. The bar's four are a family of their own because the
        // toolbar sits on the *paper* and everything else floats on the
        // surface.
        "--text: {}; --paper: {}; --accent: {}; --surface: {}; --line: {}; \
         --muted: {}; --faint: {}; --note: {}; --hover: {}; --sunk: {}; \
         --ground: {}; --scrim: {}; --accent-soft: {}; --accent-contrast: {}; \
         --positive: {}; --negative: {}; --negative-contrast: {}; \
         --bar-hover: {}; --bar-sunk: {}; --bar-line: {}; --bar-accent: {};",
        hex(theme.text),
        hex(theme.background),
        hex(theme.accent),
        hex(theme.surface()),
        hex(theme.line()),
        hex(theme.muted()),
        hex(theme.faint()),
        hex(theme.note()),
        hex(theme.surface_hover()),
        hex(theme.surface_sunk()),
        hex(theme.ground()),
        theme.scrim(),
        hex(theme.accent_soft()),
        hex(theme.accent_contrast()),
        hex(theme.positive()),
        hex(theme.negative()),
        hex(theme.negative_contrast()),
        hex(theme.bar_hover()),
        hex(theme.bar_sunk()),
        hex(theme.bar_line()),
        hex(theme.bar_accent()),
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
/* `* { box-sizing: border-box }` is the first line of `styles.css` and it was
   missing here, which made every `width`, `min-width` and `height` in this
   sheet mean something slightly different from the same number over there —
   the padding on top rather than inside. It shows up as near-misses, which
   are the worst kind: `.zoom-level`'s `min-width: 62px` came out as 82 in a
   group that was already the widest thing in the bar. Five rules below had
   been patched to say it for themselves, one at a time, as each near-miss was
   noticed; they are left saying it, because each one is load-bearing where it
   stands and now agrees with the rule above it. */
* { box-sizing: border-box; }

body { margin: 0;
  font-family: ui-sans-serif, -apple-system, "Helvetica Neue", Arial, sans-serif;
  font-size: 13.5px; line-height: 1.45;
  /* **The same font as the app's, and this line is what makes it the same
     type.** Both sides resolve `-apple-system` to the identical file —
     `/System/Library/Fonts/SFNS.ttf`, which fontique reaches through
     `GenericFamily::SystemUi` and WebKit through `system-ui` — and at 27px
     and above the two lay out the same string to within a third of a pixel.
     Below that they diverge by about a tenth, and the reason is SF's `trak`
     table: the system font carries tracking that grows as the size falls,
     WebKit applies it, and parley does not read the table at all.

     So the chrome came out tighter and darker than the app's — the "more
     machine-like, a tiny bit too strong" of the complaint, which is what a
     UI face looks like with its small-size tracking taken away. Measured
     against WebKit over the same string at every size this sheet uses, the
     missing advance is 0.61px a character at 11px, 0.59px at 13.5px and
     0.55px at 16px — flat enough across the band that one number says it,
     and 0.6px is that number. It is wrong above about 17px, where SF's
     tracking falls away towards nothing; the two headings that live up
     there say so themselves. */
  letter-spacing: 0.6px; }

 /* `position: relative` so that the Settings scrim, which is absolute, is
   measured against the window rather than against whatever Blitz would
   otherwise pick — without it the scrim started below the toolbar and the bar
   stayed bright behind a window that claims to be modal. */
.root { position: relative; display: flex; flex-direction: column; height: 100vh;
  /* `body { background: var(--bg) }` in the app: the ground, not the paper. */
  background: var(--ground); color: var(--text);
  /* **The chrome is not text to be selected, and until this line every button
     in it could be highlighted instead of pressed.** Blitz decides a gesture
     is a selection as soon as the pointer moves two pixels with the button
     down — which is most presses made with a mouse — and from then on the
     press is a drag and the click never happens. A browser does not have this
     problem because its user-agent stylesheet says `user-select: none` for a
     button; Blitz's does not, so the rule is said here for the whole window.
     `#toolbar` and `#sidebar` carry it in the app for the same reason, one
     level down. See `blitz-button-select.md`. */
  user-select: none; }

/* …and on everything in it, because it does not inherit: Blitz reads the
   property off the node the press landed on, and a button under a root that
   says `none` still answers `auto`. */
.root * { user-select: none; }

/* …and the two places where selecting *is* the point: a field being typed in,
   and the text of somebody's note. The document itself is not among them —
   its selection is drawn by `select.rs` from pdfium's own character boxes and
   never went through the DOM. */
.root input, .root textarea, .root .note-text { user-select: text; }

/* Every row of the window that is not the document carries `z-index`, and it
   is not decoration. Blitz paints by the rules — `.viewer` has
   `overflow: hidden` and a page scrolled past the top is clipped, which the
   screenshots show — but it **hit-tests without clipping**: a page whose box
   starts at -2789px is still hit-tested where its box is, which is over the
   toolbar and the find bar. So clicking "Done" in the find bar, with the
   document scrolled at all, landed on the page behind it and did nothing.
   `position: relative` and a `z-index` put these back in front, which is the
   same trap and the same fix as `.sidebar-resize` below, one level out. */
.toolbar { position: relative; z-index: 2; }
.sidebar { position: relative; z-index: 1; }

.toolbar {
  /* Twelve between the three groups and six inside each of them, which is
     `#toolbar { gap: 12px }` over `.bar-group { gap: 6px }` in the app: the
     seam between two groups reads as a seam only if it is wider than the one
     between two chips. This said six in both places, so the bar was one
     undifferentiated row of fourteen controls. */
  display: flex; align-items: center; gap: 12px;
  height: 46px; flex: 0 0 auto; padding: 0 10px;
  /* The paper, not the backdrop: the bar runs along the top of the document
     and belongs to it — `#toolbar { background: var(--page-paper) }` in the
     app, with a line off the same family. */
  background: var(--page); border-bottom: 1px solid var(--bar-line);
}
/* **Three groups, and the middle one is why there are three.** The bar was
   one flat row with a `.spacer` in it, so the page readout sat wherever the
   row ran out of chips — which was the far right, beside the cog. The app's
   own arrangement, and its reasoning: the two side groups start from what
   they hold and share the slack, which keeps the page controls near the
   middle without promising they are in it. When there is not enough room for
   everything the left gives way, because it is the side with a title in it
   and a title has an ellipsis to shrink into.

   **The bases are nought, where the app's are `auto`, and that one difference
   is what puts the page controls in the middle.** With a basis of `auto` each
   side starts from what it holds and the slack is split evenly on top, so the
   middle sits off centre by exactly half the difference between the two
   sides — which here is a hundred and eleven pixels, at every window width,
   for ever. The app can afford that because seventy-eight pixels of its bar
   are given over to the traffic lights and the offset lands roughly where the
   eye expects the middle to be anyway; this window keeps its own title bar,
   has no traffic lights to make room for, and so the same rule reads as a
   page counter that has slid to the left.

   A basis of nought asks for nothing, so both sides are given an equal share
   of the whole bar and the middle is genuinely in the middle. What stops the
   right side from being squeezed off the edge is `min-width`: `auto` on a
   flex item is its content, so `.bar-right` can grow past its share and never
   below it, and `.bar-left` says `0` and is therefore the side that gives way
   — which is the app's rule, and its reason, kept exactly. Below about
   1500px there is not enough bar for an even split and the right takes what
   it needs; above it the middle is centred. */
.bar-group { display: flex; align-items: center; gap: 6px; }
.bar-left { flex: 1 1 0; min-width: 0; }
.bar-center { flex: 0 0 auto; }
.bar-right { flex: 1 0 0; justify-content: flex-end; }

.chip {
  display: flex; align-items: center; gap: 7px;
  height: 30px; padding: 0 10px; border-radius: 9px; border: 0;
  background: transparent; color: var(--muted); font-size: 13.5px; font-weight: 500;
  white-space: nowrap;
}
.icon { flex: 0 0 auto; }
.chip:hover { background: var(--hover); color: var(--text); }
/* On the bar, a hover and a held-down state come from the paper the bar sits
   on rather than from the surface — `#toolbar .btn:hover` in the app, and its
   reason: the bar belongs to the document, so a chip mixed from the backdrop
   is a chip from another theme. Everything else wearing `.chip` floats on
   `--surface` and keeps the surface's. */
.toolbar .chip:hover { background: var(--bar-hover); }
.toolbar .chip.on, .toolbar .chip.on:hover { background: var(--bar-accent); }
.toolbar .zoom-group { background: var(--bar-sunk); }
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
   `.zoom-group` in the app.

   **Every number here is now the app's**, and between them they were eleven
   pixels of extra width in the one group that had none to spare: this was the
   widest thing in the bar and the reason the page controls sat a hundred and
   twenty pixels left of the middle. `.zoom-group` is `padding: 2px` and
   `var(--radius)`, the three buttons are a chip's own 30px tall with the two
   ends 30px square, and `.zoom-level` asks for 62. The readout is wider than
   that in practice — "Fit width" is about 75 — so the minimum only decides
   what "100%" gets, which is the point of having one. */
.zoom-group {
  display: flex; align-items: center; gap: 2px;
  padding: 2px; border-radius: 9px; background: var(--sunk);
}
.zoom-group .chip { height: 30px; border-radius: 9px; }
.zoom-group .chip.zoom-out, .zoom-group .chip.zoom-in {
  width: 30px; padding: 0; text-align: center;
}
.zoom-group .chip.fit { min-width: 62px; text-align: center; }

/* What a menu hangs off. The button is in the flow and the menu is not, so
   the row is the height of the button and the panel is under whichever
   button opened it — see `app.rs`, where this replaced a single layer pinned
   to the ends of the bar. `calc(100% + 8px)` is the toolbar's own lower edge
   read off the button rather than written down: the chip is 30px in a 46px
   row, and 8px is the space under it. */
.anchor.titled { margin-left: 6px; min-width: 0; }
.anchor { position: relative; display: flex; align-items: center; }

/* The menu itself, out of the flow inside its `.anchor`, so a panel taller
   than the toolbar costs the 46px row nothing.

   `z-index` is not decoration: Blitz hit-tests a positioned node ahead of
   what it is painted over only when it carries a non-zero one, and a menu
   that cannot be clicked is worse than no menu. 5 rather than 1 because it
   is over the toolbar as well as over the document. */
/* **A menu is written a size larger than the bar it hangs off**, which is
   `.popover { font-size: 14.5px }` in the app and was missing here — so every
   menu in this reader was set in the toolbar's 13.5 and came out a list of
   small grey words. The size is the whole reason a menu reads as somewhere
   you have arrived rather than as more chrome, and every number below is
   `.popover`'s: 232 to 340 wide, six of padding, and a shadow, because a menu
   floats over the document and a border alone does not say so. */
.menu {
  position: absolute; top: calc(100% + 8px); z-index: 5;
  min-width: 232px; max-width: 340px; font-size: 14.5px;
  padding: 6px; border-radius: 12px;
  border: 1px solid var(--line); background: var(--surface);
  box-shadow: 0 10px 34px rgba(0,0,0,0.16);
}
/* Under the left edge of the button, except the two near the right end of the
   bar, which would otherwise run off it. */
.menu.document, .menu.open, .menu.view { left: 0; }
.menu.theme, .menu.settings { right: 0; }
/* Fourteen themes is taller than a short window, and this is the one list in
   the app that is a list rather than a handful. */
.menu.theme { max-height: 60vh; overflow: scroll; scrollbar-width: thin; }
/* Wide enough for "Show page count while scrolling" and its note beside a
   switch, which is the widest row any menu here has. */
.menu.settings { min-width: 330px; }
/* No `width: 100%`. A menu is absolutely positioned and therefore shrinks to
   fit, and a percentage width inside a shrink-to-fit box is resolved against
   a width computed as though the percentage were not there — so the widest
   row set the menu's width and every other row was then laid out one row too
   narrow, which showed as the chord on the right clipped by four pixels.
   These are block-level flex containers and fill the line on their own. */
/* **Padding rather than a fixed height**, which is `.popover-item` and is not
   a detail: a row given 30px and no padding is 30px whatever is in it, and a
   row given seven pixels above and below grows with the type — so the same
   rule holds at 14.5 here as at 13.5, and the row comes out the app's 35.
   The ink is the quiet shade until the pointer is on it, which is the app's
   `--text-soft` over `--text`: a menu of fourteen themes all in full-strength
   ink reads as fourteen things shouting. */
.menu-item {
  display: flex; align-items: center; gap: 10px;
  padding: 7px 10px; border: 0; border-radius: 8px;
  background: transparent; color: var(--muted);
  text-align: left;
}
.menu-item:hover { background: var(--hover); color: var(--text); }
.menu-item.on { color: var(--accent); }
/* A column of its own so the labels line up whether or not a row is ticked.
   Fourteen, which is `.popover-item .check`'s own width and the width of the
   drawing that goes in it. */
.menu-tick { flex: 0 0 14px; color: var(--accent); }
/* …and a drawing where a shelf row has one, which needs the two extra
   pixels an icon is wider than a tick. */
.menu-tick .icon { width: 16px; height: 16px; margin-left: -2px; }
/* `min-width: 0` and `overflow` are what `max-width` on the menu needs: a
   theme somebody named at length would otherwise push the menu past its cap
   rather than being cut at it. The app cuts it with an ellipsis, which is a
   thing this renderer has not got. */
.menu-label { flex: 1 1 auto; min-width: 0; overflow: hidden; white-space: nowrap; }
/* The chord, read off the keymap rather than written here — see
   `Viewer::chord_for`. An aside and not the item, and the app's own shade for
   one — `--text-note`, which is only a little quieter than the label, because
   the note beside a row is meant to be read. Fading it to `--faint` and
   shrinking it to 12 made it decoration. */
.menu-key { flex: 0 0 auto; color: var(--note); }
.menu-rule { height: 1px; margin: 5px 8px; background: var(--line); }
/* A row of a menu that holds a control rather than a choice — `.popover-row`
   in the app, where the label carries the weight and the control sits at the
   end of it. */
.menu-row { display: flex; align-items: center; gap: 10px; padding: 6px 10px; }
.menu-row-label { flex: 1 1 auto; color: var(--text); }
/* `--text-note` and the row's own size, which is `.popover-note` in the app
   and carries its warning with it: "the difference between them is weight
   rather than size … telling the two apart by fading the note is what made it
   unreadable". This had it at `--faint` and 12px, which is both of the things
   that comment says not to do. */
.menu-row-note { color: var(--note); }
/* A theme, two letters wide, in its own colours — `ui.swatch` in the app, and
   its numbers: a wide shallow chip rather than a square, and an inset ring in
   a neutral grey rather than a border in the theme's line colour, so that a
   swatch on a dark theme is not outlined in something darker than it. */
.swatch {
  display: flex; align-items: center; justify-content: center;
  flex: 0 0 auto; width: 22px; height: 16px; border-radius: 4px;
  box-shadow: inset 0 0 0 1px rgba(127,127,127,0.35);
  font-size: 11px; font-weight: 600;
}
/* A heading over a run of items — the app's `ui.section`, and the Document
   menu's shelf is the one place that has one. Told from an item by colour
   alone, at the menu's own size: `.popover-section` names no size either, and
   a heading a size smaller than what it heads reads as a footnote over the
   list rather than as a title on it. */
.menu-section {
  padding: 8px 10px 4px; color: var(--faint);
}

/* The title is a button now, because the document's menu hangs off it. It
   keeps the chip's shape and the title's own colour and truncation. */
/* Icon-only, so they get the zoom group's square rather than a chip's
   word-shaped one. */
.chip.page-previous, .chip.page-next { width: 30px; padding: 0; justify-content: center; }

/* **`flex: 1 1 0`, not `0 1 auto`.** The app's `.doc-title` is
   `min-width: 0` with an ellipsis and shrinks from its content; here a flex
   item whose basis is its content does not shrink at all — the group gave way
   and the button inside it kept its full width, so a long file name was drawn
   straight across the page controls in the middle of the bar. A basis of zero
   is the shape `.tab` already uses for the same reason one panel over. The
   `max-width` is the app's own 34ch, so that a short name does not leave a
   button half the bar wide. */
/* `.doc-title` in the app, and the numbers are its numbers: no icon, 13px,
   the faint ink, 8px of padding and 6px of air in front of it.

   **`flex: 0 1 auto`, not `1 1 0`**, and the difference is the whole of what
   a reader sees. A basis of zero asks for nothing and is given whatever the
   bar has left over, which in a bar holding fourteen controls is nothing at
   all: the name came out twenty pixels wide, three letters of it visible, at
   every window size. `auto` asks for the name and gives way under pressure,
   which is what the app does and what an ellipsis is for. `min-width: 0` is
   what lets it give way at all — without it the automatic minimum size of a
   `nowrap` run is the whole run, and the bar overflows instead. */
.chip.title {
  /* The air in front of the name is on `.anchor.titled`, not here, so that
     the menu still comes down flush with the button it belongs to. */
  flex: 0 1 auto; min-width: 0; max-width: 276px;
  padding: 0 8px; font-size: 13px; font-weight: 400; color: var(--faint);
  white-space: nowrap; overflow: hidden;
}
/* **The fade is what an ellipsis would be, and it belongs only to a name that
   has actually run out of box.** It was on `.chip.title` itself, so every
   document was faded over its last twenty-four pixels — which on `book.pdf`,
   a button sixty-four pixels wide, is more than a third of it, and reads
   exactly as the reader described it: a button too small for its name, going
   pale at the edge. The app has no such thing on a name that fits;
   `text-overflow: ellipsis` shows nothing until there is something to cut.

   Blitz has no `text-overflow`, so the fade stands in for it, and *when* to
   draw it has to be decided outside the sheet: `app.rs` puts this class on
   only when the name is longer than the box can hold. `max-width` is the
   app's `34ch` in pixels, which is what 34ch resolves to at 13px in the
   engine the app runs in — `ch` is a unit this renderer need not have. */
.chip.title.clipped {
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent);
}
.chip.title:hover { color: var(--text); }

/* The page and the count, which is `.page-jump` in the app: a box you can
   type in, and the total beside it rather than inside it. The pair used to be
   one sunk pill with the number pushed against its left wall and the count
   after it, and there was no way to tell the two apart. */
/* **Four pixels either side, and no ground of its own.** `.page-jump` in the
   app is a transparent box with `padding: 0 4px` holding one bordered field
   and one plain count; this had no padding and a `--bar-sunk` fill under the
   pair (see `.toolbar .pill`, now gone), so "of 400" was written on a grey
   panel that ended exactly where the last zero did. That is the whole of
   "cramped, almost cut off on the right": a word with a background behind it
   and no room between the two. The field keeps its own sunk ground, because
   in the app the field is the only part of this that is a control. */
.pill {
  height: 30px; padding: 0 4px;
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
  /* The bar's own family rather than the surface's, which is what
     `.page-jump input` names in the app: this control stands on the paper the
     document is on, not on a floating surface. */
  border: 1px solid var(--bar-line); border-radius: 7px;
  background: var(--bar-sunk); color: var(--text); font-size: 13.5px;
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

/* **The find bar is the app's own card, and it was a row of the column
   here.** A row is simpler — nothing is over anything, and Blitz has no
   `position: fixed` — and it is not what the app does or what a find bar is:
   it took forty pixels off the document for as long as it was up, so opening
   it moved the page the reader was looking at. `styles.css` puts it under the
   toolbar at the right, over the document, in two rows: the query and its
   four buttons, and the three switches indented under the field so they read
   as belonging to the query rather than as three more buttons.

   `top` is set in `app.rs` rather than in a `calc`, because the toolbar can
   be put away and the bar has to come up to meet the window's edge —
   `#shell[data-toolbar="hidden"] .find-bar` in the app, which is a selector
   this port has no shell attribute for. */
.find-bar {
  position: absolute; right: 18px; z-index: 30;
  display: flex; flex-direction: column;
  padding: 6px 8px 8px 12px;
  background: var(--surface); border: 1px solid var(--line); border-radius: 12px;
  box-shadow: 0 8px 26px rgba(0,0,0,0.14);
}
.find-row { display: flex; align-items: center; gap: 8px; }
/* Indented to the field above them rather than to the bar, so the three read
   as belonging to the query and not as three more buttons. */
.find-options { display: flex; align-items: center; gap: 2px; margin-left: 20px; padding-top: 4px; }
/* On, and saying so as loudly as the buttons up in the bar do. These three
   are settings that outlive the bar and the session, so "Highlight all" being
   on is a thing to see at a glance rather than a tick to go looking for. The
   drawing is there whether it is ticked or not, so turning one on does not
   shuffle the other two sideways under the pointer — which is why the tick is
   faint rather than absent. */
.find-option {
  display: flex; align-items: center; gap: 5px;
  height: 22px; padding: 0 7px; border: 0; border-radius: 7px;
  background: transparent; color: var(--faint); font-size: 12.5px;
}
.find-option:hover { background: var(--hover); color: var(--muted); }
.find-option.on { background: var(--accent-soft); color: var(--accent); }
.find-icon { display: flex; align-items: center; color: var(--faint); }
.find-field {
  width: 230px; height: 28px; padding: 0;
  border: 0; background: transparent; color: var(--text); font-size: 13.5px;
}
/* `.find-bar input:focus { outline: none }` in the app, and the same reason
   the page field carries it: the ring Blitz draws round a focused input is a
   blue box the length of the field, which over a card that is already the
   thing being looked at reads as an error state. */
.find-field:focus { outline: none; }
/* The count, and the way to the list behind it — `Viewer::show_results`. It
   is a button, so it wears a button's clothes: transparent until there is
   something to show, and then a hover, which is `.find-status:not(:empty)` in
   the app. `.ready` rather than `:empty`, which Blitz does not implement. */
.find-count {
  flex: 0 0 auto; min-width: 60px; height: 24px; padding: 0 6px;
  border: 0; border-radius: 6px;
  background: transparent; color: var(--faint); font-size: 12.5px;
  justify-content: flex-end;
}
.find-count.ready:hover { background: var(--hover); color: var(--text); }
/* A chip wearing a drawing and no word: square, with the drawing centred in
   it. `.btn.icon-only` in the app, where the find bar's close button is one
   too. */
.chip.icon-only { width: 30px; padding: 0; justify-content: center; }

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

/* **A note somebody left in the document.** pdfium paints the annotation's
   own appearance into the page — a sticky note arrives as the little icon it
   was drawn as — so nothing is painted here: this is the hit area over it,
   and it shows itself on hover. Above `.link` for the same reason `.link` is
   above the page: a note over a cross-reference is the more specific thing.
   `.note-edge` is the strip at the right of a comment that covers a passage;
   see [`crate::render::Note`]. */
.note-spot, .note-edge { z-index: 3; cursor: pointer; border-radius: 3px; }
.note-spot:hover, .note-edge:hover { background: var(--accent-soft); }
.note-edge { margin-left: 2px; }

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
/* And `font-size: 14.5px`, which is `#sidebar`'s own and was missing: the
   panel is read the way a menu is read — at arm's length, in a column, a line
   at a time — so it is written a size larger than the bar, and every list in
   it inherits that rather than naming a size of its own. A contents list set
   in 13 is a contents list nobody reads. */
.sidebar {
  flex: 0 0 auto; display: flex; flex-direction: column; box-sizing: border-box;
  min-height: 0; font-size: 14.5px;
  background: var(--surface); border-right: 1px solid var(--line);
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
  /* Two, not one, since `.tabs` below took one: they are siblings in the
     same stacking context and they overlap for the height of the tab strip,
     so equal numbers would hand the top 35px of the drag edge to the tabs. */
  z-index: 2; cursor: col-resize;
}
.sidebar-resize:hover { background: var(--accent); opacity: 0.35; }
/* **And the tabs are the same trap again, one level in.** A thumbnail is
   absolutely positioned at `top - thumb_scroll`, so the column scrolled down
   at all puts rows at negative offsets — clipped by `.panel`'s
   `overflow: hidden` when it paints, and hit-tested where their boxes say
   they are, which is over this strip. Clicking Contents from a scrolled Pages
   tab therefore landed on a thumbnail nobody could see and did nothing at
   all, and it worked perfectly at the top of the column, which is where
   anybody checking it would look. */
.tabs { position: relative; z-index: 1; display: flex; flex: 0 0 auto; gap: 4px; padding: 8px 8px 6px 8px; min-width: 0; }
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
  display: flex; align-items: center; justify-content: center; gap: 5px;
  flex: 1 1 0; min-width: 0; overflow: hidden;
  height: 28px; padding: 0 4px; border: 0; border-radius: 7px;
  background: transparent; color: var(--muted); font-size: 12.5px; font-weight: 500;
}
.tab .icon { flex: 0 0 auto; }
/* `text-overflow: ellipsis` is not implemented — see the note at the top of
   this file — so the word fades out instead, the same mask `.chip.title` and
   `.result` use. */
.tab-label {
  flex: 0 1 auto; min-width: 0; overflow: hidden; white-space: nowrap;
}
/* **And only where a word can actually be cut.** The mask was on the label
   itself, and a label is `flex: 0 1 auto` — its box *is* its word — so the
   last ten pixels of every tab were faded whether or not anything had run out
   of room, which on a two-tab strip with a hundred and fifteen pixels a tab is
   simply wrong. It is the same fault `.chip.title` had and the same shape of
   fix: `sidebar.rs` says `tight` when there are three tabs and the panel is
   narrower than all three of them labelled. The rows below get away with an
   unconditional mask because their boxes are the full width of the panel, so
   the faded band falls on empty ground when the text is short. */
.tabs.tight .tab-label {
  mask-image: linear-gradient(to right, #000 calc(100% - 10px), transparent);
}
.tab:hover { background: var(--hover); color: var(--text); }
/* **The tint carries the theme, here as in the bar.** This said `--sunk` and
   `--text`, which is a grey chip of grey words under every one of the fourteen
   themes — and it is exactly the fault `.chip.on` had and the same fix:
   `.tab[aria-selected="true"]` in the app is the accent at a fifth of its
   strength with the accent written on it. Which tab is open is the one thing
   this strip exists to say. */
.tab.on { background: var(--accent-soft); color: var(--accent); }

.panel { flex: 1 1 auto; overflow: hidden; min-height: 0; }
.thumb-column { position: relative; }
.sidebar-empty { margin: 10px 12px; color: var(--faint); }

/* Padding rather than a fixed height, and the panel's own size rather than a
   size of its own — both are `.outline-item` in the app, and both matter for
   the same reason: a heading is a line of somebody's prose, and a list of them
   set in 13 in a 26px slot is a list nobody reads. The `padding-left` written
   on each row in `sidebar.rs` is the indent and overrides the 8 here. */
.outline-item {
  display: block; width: 100%; border: 0; border-radius: 7px;
  padding: 5px 8px;
  background: transparent; color: var(--muted); text-align: left;
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 20px), transparent);
}
.outline-item:hover { background: var(--hover); color: var(--text); }
/* Where the reader is, said the way every other "this one" in this app says
   it: the accent under the words as well as in them. Colour alone is what
   `.chip.on` and `.tab.on` were both wrong about — see `.tab.on`. */
.outline-item.current { background: var(--accent-soft); color: var(--accent); }

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
  border: 0; border-radius: 9px; padding: 7px 10px; margin-bottom: 1px;
  background: transparent; text-align: left; font-size: 12.5px;
  white-space: nowrap; overflow: hidden;
  mask-image: linear-gradient(to right, #000 calc(100% - 24px), transparent);
}
.result:hover { background: var(--hover); color: var(--text); }
.result.current { background: var(--accent-soft); color: var(--accent); }
/* `min-width` so that the numbers line up down the column rather than every
   quote starting at a different place — `.result-page` in the app, in ems
   because it is a measure of digits and not of pixels. */
.result-page {
  flex: 0 0 auto; min-width: 2.2em; color: var(--faint); font-size: 12px;
}
.result-line { flex: 1 1 auto; min-width: 0; overflow: hidden; color: var(--muted); }
/* `pre`, because the space either side of the match is the whole difference
   between a line that reads as a sentence and "A**needle**in the first page":
   HTML collapses whitespace at the edge of an inline run, and these two runs
   are cut out of the document precisely at those edges. `results()` keeps a
   single space there deliberately — see `search.rs`. */
.result-before, .result-after { white-space: pre; }
/* The matched words, drawn the way a swept passage on the page is drawn: the
   theme's own selection colours, which is `.result-line mark` in the app. A
   bold run in the ink colour said "this is the word" in the one way that
   cannot be told apart from a heading. */
.result-hit {
  background: var(--found); color: var(--found-ink); border-radius: 2px;
}

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

/* And what a mark already in the document says when it is clicked. The same
   card as the swatches above it, because it is the same kind of thing in the
   same place — the difference is that it names what it does rather than
   showing six colours, since there is exactly one thing to do here. */
.mark-popover {
  display: flex; align-items: center; gap: 8px; padding: 7px 9px; z-index: 6;
  background: var(--surface); border: 1px solid var(--line); border-radius: 11px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.18);
}
.mark-dot {
  width: 12px; height: 12px; border-radius: 6px; border: 1px solid var(--line);
}
.mark-remove {
  border: 0; background: transparent; padding: 0;
  color: var(--text); font-size: 13px; white-space: nowrap;
}
.mark-remove:hover { color: var(--accent); }

/* The column takes its shape from the pictures in it, and only the rows near
   the view are here at all — see `sidebar.rs`. */
.thumbs { position: relative; width: 100%; }
.thumb {
  border: 0; background: transparent; padding: 0;
  display: flex; flex-direction: column; align-items: center;
}
/* **A ring, not a drop shadow, and the page being read wears it in the
   accent.** `.thumb canvas` in the app is `0 0 0 1px var(--line)` and
   `.thumb.current canvas` is `0 0 0 2px var(--accent)`: a hairline round every
   thumbnail so a white page is told from a white panel, and a coloured one
   round the page you are on. This had a drop shadow under every picture and
   nothing at all on the current one — so which page was being read was said
   by the number under it and by nothing else, at a size where the number is
   twelve pixels tall in a column three hundred pixels long. A shadow is what
   a page on the desk has; a thumbnail in a list is not on a desk. */
.thumb-picture {
  background: var(--page); border-radius: 4px;
  box-shadow: 0 0 0 1px var(--line);
}
.thumb.current .thumb-picture { box-shadow: 0 0 0 2px var(--accent); }
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
  margin: 6px 0 22px; text-align: center; color: var(--note); font-size: 15.5px;
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
.start-hint { margin-top: 22px; text-align: center; color: var(--note); }

.recents { margin-top: 26px; }
.recents-title { padding: 0 8px 6px; color: var(--note); }
/* A row is the button and the × beside it, and the × is a sibling rather than
   a child: a button inside a button is not a shape either the DOM or a
   pointer knows what to do with, and the app gets away with a `<span>` there
   only because it is listening for a click and stopping it. */
.recent { display: flex; align-items: center; border-radius: 9px; }
.recent:hover { background: var(--hover); }
.recent:hover .recent-open { color: var(--text); }
.recent-open {
  display: flex; align-items: center; gap: 10px;
  flex: 1 1 auto; min-width: 0; height: 34px; padding: 0 4px 0 10px;
  border: 0; background: transparent; color: var(--muted);
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
/* A shadow, because this is the one thing in the reader that floats over the
   document with nothing behind it — `.notice` in the app carries one, and a
   pill with a hairline and no shadow reads as a shape drawn on the page
   rather than as something laid over it. `gap` is for the tick beside "Saved". */
.notice {
  display: flex; align-items: center; gap: 8px;
  max-width: 70%; padding: 9px 16px; border-radius: 999px;
  background: var(--surface); border: 1px solid var(--line);
  box-shadow: 0 6px 20px rgba(0,0,0,0.16);
  color: var(--text);
}

/* The handle that gives the toolbar back, centred along the top edge by a row
   rather than by `left: 50%` and a transform — `.notice-line`'s reason again.
   It is in the DOM only while it is being reached for (see
   `Viewer::reach_for_toolbar`), which is what the app's `.visible` class does
   with a transform it can animate and this cannot.

   In full screen it sits clear of the system's own bars, which slide down
   over exactly this band when the pointer reaches for it. */
.peek-line {
  position: absolute; left: 0; right: 0; top: 0; z-index: 30;
  display: flex; align-items: flex-start; justify-content: center;
}
.toolbar-peek {
  display: flex; align-items: center; gap: 6px;
  padding: 5px 13px 7px; border: 1px solid var(--line); border-top: 0;
  border-radius: 0 0 11px 11px;
  background: var(--surface); box-shadow: 0 6px 18px rgba(0,0,0,0.16);
  color: var(--muted); font-size: 13.5px;
}
.toolbar-peek.clear {
  margin-top: 38px; border-top: 1px solid var(--line); border-radius: 11px;
}

/* **The page pill**: where the reader is, said in the middle of the lower
   edge while they scroll with the toolbar away — `#page-pill` in the app,
   which is the one thing that answers "which page is this" when the bar that
   usually answers it has been put down. Centred by a row rather than by
   `left: 50%` and a transform, for the reason `.notice-line` above is.
   `box-shadow` and no animation: the app fades it in over 160ms, and this
   file's rule is that nothing moves unless the reader moved it. */
.pill-line {
  position: absolute; left: 0; right: 0; bottom: 20px; z-index: 20;
  display: flex; align-items: center; justify-content: center;
  pointer-events: none;
}
.page-pill {
  padding: 6px 14px; border-radius: 999px;
  background: var(--surface); border: 1px solid var(--line);
  box-shadow: 0 6px 20px rgba(0,0,0,0.14);
  color: var(--muted); font-size: 13.5px;
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
/* **The scrim is the theme's backdrop at 62%, not black at 34%.** The app says
   `color-mix(in srgb, var(--bg) 62%, transparent)`, and the difference is not
   subtle: a black wash over Hylo Light darkens a pale reader into something
   that looks switched off, and over Hylo Ember it turns a warm room grey. A
   wash of the app's own ground leaves every theme recognisably itself, which
   is the point of having themes. `--scrim` is that colour with its alpha
   already in it, mixed in `palette.rs` where the rest of the shades are.
   (The app also blurs what is behind it; `backdrop-filter` is not something
   this renderer has, and a scrim without it is still a scrim.) */
.window-scrim {
  position: absolute; top: 0; left: 0; right: 0; bottom: 0; z-index: 20;
  display: flex; align-items: center; justify-content: center;
  background: var(--scrim);
}
/* `font-size` and `background` are the two that were wrong, and the second is
   the one you can see: `.window` in the app stands on `--surface`, the shade
   everything that floats is mixed to, and this had it on `--paper` — the
   colour of the *page*. On a light theme the two are near enough that nobody
   would look twice; on a dark one the Settings window came up the colour of a
   sheet of paper in a dark room. And 14.5px, for the reason the sidebar and
   the menus have it: this is somewhere you have arrived, not more chrome. */
.window {
  display: flex; flex-direction: column; font-size: 14.5px;
  width: 860px; height: 600px; max-width: 92%; max-height: 92%;
  border-radius: 14px; border: 1px solid var(--line);
  background: var(--surface); color: var(--text);
  box-shadow: 0 18px 60px rgba(0,0,0,0.28);
}
.window-bar {
  display: flex; align-items: center; gap: 10px; flex: 0 0 auto;
  height: 48px; padding: 0 10px 0 18px; border-bottom: 1px solid var(--line);
}
.window-title { flex: 1 1 auto; font-size: 15px; font-weight: 600; }
.chip.window-close { width: 30px; padding: 0; justify-content: center; }
.window-body { flex: 1 1 auto; display: flex; flex-direction: row; min-height: 0; }
/* A note is a paragraph, not a settings window: it fits what is in it. */
.note-window { width: 440px; height: auto; max-height: 70%; }
.note-body { padding: 16px 18px 18px; display: flex; flex-direction: column; gap: 10px; }
.note-where { margin: 0; color: var(--faint); font-size: 12.5px; }
.note-text { margin: 0; color: var(--text); }
.note-said { margin: 0; color: var(--faint); font-size: 12.5px; }

/* The password window. `.window-ask` in the app, and it is the one window in
   this reader that fits what is in it in both directions: a lede, a field and
   two buttons is four rows, and a 600px frame around them would be a window
   asking a one-line question in a hall. */
.ask-window { width: 420px; height: auto; }
.ask-body { padding: 16px 20px 18px; display: flex; flex-direction: column; gap: 12px; }
.ask-body .pane-lede { margin: 0; }
.ask-field { width: 100%; height: 32px; }
/* The gap above is the column's, not the row's — see `.pane-actions`, which
   carries a margin for the pages in Settings where it follows a long list. */
.ask-actions { margin-top: 0; justify-content: flex-end; }

/* The Information window: what the document says about itself, a row a fact.
   `showDocumentDetails` in `main.ts` and `ui.field` under it. */
.details-window { min-width: 420px; max-width: 560px; }
.details-name { margin: 0 0 10px 0; font-size: 15px; font-weight: 600; }
.details-row { display: flex; gap: 12px; padding: 4px 0; }
.details-label { flex: 0 0 34%; color: var(--faint); font-size: 13px; }
.details-value { flex: 1 1 0; min-width: 0; color: var(--text); font-size: 13px; }
/* The nav column is the *sunk* shade, not the surface — `.window-nav` in the
   app — which is what tells the column from the page beside it now that the
   window itself is the surface. Its width and padding are the app's too. */
.window-nav {
  flex: 0 0 186px; display: flex; flex-direction: column; gap: 2px;
  padding: 10px 8px; border-right: 1px solid var(--line); background: var(--sunk);
}
.nav-item {
  display: flex; align-items: center; gap: 9px;
  height: 32px; padding: 0 10px; border: 0; border-radius: 9px;
  background: transparent; color: var(--muted); font-weight: 500;
  text-align: left;
}
.nav-item:hover { background: var(--hover); color: var(--text); }
/* The same pair as a chip in force: the tint carries it and the accent is
   legible on the tint. See `.chip.on`. */
.nav-item.on { background: var(--accent-soft); color: var(--accent); }
/* `scroll`, not `auto` — Blitz has no `auto`, which is the note at the top of
   this file. Reading is the longest page and does not fit in 600px. */
.window-pane {
  flex: 1 1 auto; min-width: 0; padding: 18px 26px 28px 26px;
  overflow: scroll; scrollbar-width: thin;
}
/* `letter-spacing` is the app's own `-0.01em`, and it does a second job here:
   the 0.6px of tracking `body` stands in with is right for the 11-16px band
   the rest of this sheet lives in and too much at nineteen. See `body`. */
.pane-title { margin: 0 0 4px 0; font-size: 19px; font-weight: 600; letter-spacing: -0.01em; }
/* **Every sentence in this window was a shade too quiet and a size too
   small.** `--text-note` is the app's shade for the small print beside a
   setting, and `themes.ts` carries the reason next to the number: at 0.38 it
   fell under 4.5:1 against the paper and "the sentence that explains a switch
   was harder to read than the switch". These said `--faint`, which is 0.52 —
   past the point that comment is about — and named sizes of their own under a
   window that is 14.5. */
.pane-lede { margin: 0 0 12px 0; color: var(--note); }
.pane-group {
  margin: 22px 0 8px 0; font-weight: 500;
  color: var(--faint);
}
.pane-note { margin: 0 0 12px 0; color: var(--note); line-height: 1.5; }
.pane-actions { display: flex; gap: 8px; margin-top: 16px; }
/* The three shapes an action button takes, which is `ui.button`'s `kind`. */
.chip.action { border: 1px solid var(--line); }
.chip.action.primary { background: var(--accent); color: var(--accent-contrast); border-color: var(--accent); }
.chip.action.primary:hover { background: var(--accent); color: var(--accent-contrast); }
.chip.action.danger { background: var(--negative); color: var(--negative-contrast); border-color: var(--negative); }
.chip.action.danger:hover { background: var(--negative); color: var(--negative-contrast); }

/* A field somebody types into, and a colour: the swatch shows what the page
   will use, the six digits say it. */
.text-field {
  height: 28px; min-width: 0; padding: 0 8px;
  border: 1px solid var(--line); border-radius: 8px;
  background: var(--paper); color: var(--text); font-size: 13.5px;
}
.text-field:focus { outline: none; border-color: var(--accent); }
.color-field { display: flex; align-items: center; gap: 6px; }
.color-swatch {
  width: 26px; height: 26px; border-radius: 7px; border: 1px solid var(--line);
}
.color-hex { width: 96px; }
.chip.action {
  border: 1px solid var(--line); background: var(--surface); color: var(--text);
}
.chip.action:hover { background: var(--hover); }

/* One setting. The control sits on the same line as the name and the sentence
   runs under both, which is what keeps a page of switches readable as prose
   rather than as a form. */
.field { padding: 13px 0; border-bottom: 1px solid var(--line); }
.field-head { display: flex; align-items: center; gap: 18px; }
.field-label { flex: 1 1 auto; color: var(--text); font-weight: 500; }
.field-control { flex: 0 0 auto; display: flex; align-items: center; }
.field-note { margin: 6px 0 0 0; color: var(--note); font-size: 14px; line-height: 1.5; }

/* Every number is `.switch`'s in the app, and they add up to a smaller,
   quieter control than this had: 34 by 20 rather than 40 by 23, a knob of 14
   in the faint ink rather than one of 19 in white under a drop shadow, and a
   hairline round the track when it is off. What that buys is a switch that
   reads as *off* when it is off — a white knob on a pale track is a lamp with
   the light on, and a row of them down the Settings window all looked live. */
.switch {
  width: 34px; height: 20px; padding: 3px; border: 0; border-radius: 10px;
  background: var(--sunk); box-shadow: inset 0 0 0 1px var(--line);
  display: flex; align-items: center;
}
.switch.on { background: var(--accent); box-shadow: none; }
.switch-knob {
  width: 14px; height: 14px; border-radius: 7px; background: var(--faint);
}
/* On, the knob is the ink the accent is written in — `--accent-contrast` in
   the app, which is what keeps it legible on the accent whatever the accent
   is. */
.switch.on .switch-knob { background: var(--accent-contrast); }
/* The knob moves by being pushed, not by being positioned: Blitz has the box
   model and this needs nothing else. Fourteen is the track less its two
   paddings less the knob, which is the same journey `left: 3px` to
   `left: 17px` makes in `styles.css`. */
.switch.on .switch-knob { margin-left: 14px; }

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
