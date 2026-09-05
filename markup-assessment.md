# Coloured markup, written into the document

**Built.** This was a pre-build assessment and is now the reference for what a
highlight is, what each renderer can and cannot do with one, and what is still
missing. The plan it carried is done and is in git; the sections the code
points at — "the trap", steps 6 and 7 — are kept under those names.

**The stipulation it was written under:** markup is written **into the PDF**
and is there the next time the file is opened, by this app or any other.
Careful markup that vanishes between sessions is worse than no markup.

## What a highlight is

A standard markup annotation on a page:

- `/Subtype /Highlight`, `/Underline`, `/StrikeOut` or `/Squiggly` — four
  styles, all standard.
- `/QuadPoints` — eight numbers per marked run, in PDF user space, which is
  what `joinRuns` produces transformed through the page's viewport.
- `/C` — the colour, three numbers, and `/CA` for opacity.
- `/AP` — an appearance stream. **Not optional in practice**: many viewers draw
  only the appearance stream and ignore `/C`, so an annotation without one is
  invisible in half the world. Both renderers here write one.
- `/T`, `/Contents`, `/CreationDate` — who, a note, when. `/Contents` is where
  a comment on a highlight would go.

The anchoring problem that dominates every "remember where the reader marked"
design **goes away**: a highlight's anchor is its `/QuadPoints`, in the file, in
the page's own coordinate space, and the document answers "what is marked here".
No offset bookkeeping, no re-anchoring on zoom, no drift. What replaces it is
harder and more consequential — **this app writes to the reader's files** — and
that is what step 7 is about.

## What each renderer can do

**pdf.js (the Tauri app).** `Viewer.markSelection` builds the entry pdf.js's own
annotation editor would build — `annotationStorage.setValue` under the
`pdfjs_internal_editor_` prefix — and `saveDocument()` returns the whole file
back as an incremental update: original bytes untouched, new objects appended.
That shape is read out of `pdf.mjs` and `pdf.worker.mjs` rather than any
documented API, which is the risk it carries; `markup.test.mjs` saves a file and
reads a `/Highlight` back out of it, which keeps the risk visible.

Two limits in that worker decide most of the feature. `saveNewAnnotations` has
cases for `FREETEXT`, `HIGHLIGHT`, `INK`, `STAMP` and `SIGNATURE` alone, so
underline, strike-out and squiggly stay readable and are not writable. And
`Annotation.save()` is not overridden by any markup subtype, so **an annotation
already in the file cannot be edited or deleted through `saveDocument()` at
all** — which is why removal in the app is a rebuild from `.hylopdf-original`
rather than a call.

*The alternative that was rejected:* adopting pdf.js's `AnnotationEditorLayer`.
It is the supported path and brings the gesture, the colours and deletion for
nothing — and it brings pdf.js's own DOM into an app that builds every element
by hand, sits on top of the text layer selections are painted from, and adds a
second thing that owns the pointer inside a page.

**pdfium (the Dioxus experiment).** `FPDFPage_RemoveAnnot` deletes an annotation
already in the file, in one call — see `experiments/dioxus-reader/src/markup.rs`.
What it charges is that `save_to_bytes` is `FPDF_SaveAsCopy` with `flags = 0`
and `pdfium-render` does not expose the flags, so the save is a full rewrite
rather than an incremental update: nothing for an ordinary paper, the end of the
signature for a signed one.

## Drawing it, and the trap

**Saved markup draws itself.** Both renderers paint an annotation's appearance
stream into the page, so a highlight in the file arrives as part of the page
bitmap with no layer and no memory of its own — which is already true of other
people's highlights. It also means markup goes through recolouring like every
other pixel, and the colour-keeping path in `recolor` is right for it: a
saturated wash keeps its hue and moves its lightness.

**The trap is `WHITE_POINT`** (`themes.ts`, 235). Everything above level 235 is
called paper and carried to the theme's background, because a hairline printed
at 90% white would otherwise arrive as a bright cage around every hyperref box.
A conventional highlighter wash — yellow at 20-25% opacity over white — lands
around level 250. **It is paper by that rule and disappears on every recolouring
theme.** Three ways out; the third is what is built:

1. Only write colours saturated enough to clear the white point. Cheap, and it
   constrains "freely selectable" in a way the reader notices.
2. Raise or special-case `WHITE_POINT`. No — it is load-bearing for hyperref
   boxes and for scans with warm paper.
3. **Redraw the marked quads over the recoloured page**, from the pristine copy
   taken before recolouring, towards a theme-adapted markup colour. Which is
   what `restoreImages` and `tintLinks` already do for pictures and links: one
   clip over every rectangle, one `drawImage` per page. It gets the theme
   adaptation for free, which 1 and 2 do not. See `tintMarkup` in `viewer.ts`.

## Colours

Hex and nothing else, through `parseColor`/`readColor`, never handed raw to CSS
— the theme picker's swatch lied about a colour exactly once, and the fix was to
route it through the same reader the renderer uses.

Each highlight carries its own colour; the palette of six in settings
(`markup_color_1`…) is a shortcut, not a constraint. The colour written to `/C`
is the one the reader picked — that is what other applications show — and the
colour *drawn* on a recolouring theme is that colour adapted for contrast
against the theme's paper. A highlight is "the red one" on every theme, and it
is red in Preview.

## Step 6 — the sidebar

Markup is a second section in the Contents panel: the passage, the page, and a
jump. It is one list with the journal's entries, told apart by a word on the row
rather than by a section of their own, because to a reader they are one thing.
In the app, removal is not offered from a row for the reason above — nothing
removes a highlight through `saveDocument()`.

## Step 7 — the edges, which are the feature

Asked once per open by `readMarkupStanding`, and said at the first mark rather
than at the open:

- *Encrypted, unwritable, or very large* (`MARKUP_IN_FILE_LIMIT`, 100MB, because
  `saveDocument()` pulls the whole file into the worker and hands the whole file
  back): the mark is kept in the journal with real quads and its quote, and the
  reader is told once where it is.
- *Read-only* is asked of the disk rather than discovered from a failed rename —
  permission bits, a read-only volume, another owner and a sandbox all come back
  the same way, and only opening the file for writing answers truly.
- *Signed*: asked, not refused. It is their document, and an incremental update
  is exactly what a signature detects.
- *Syncing*: one sentence about which copy wins.
- *A scan*: "there is no text in this document to mark", which is a different
  sentence from "select something first".

## What this still does not do

- **A scan with no text cannot be marked.** No text layer, no selection, no
  quads. An area drag — a rectangle the reader draws — is the answer, and it is
  a `/Square` annotation. Unbuilt on both sides.
- **Comments on highlights are readable, not editable.** `/Contents` is written
  empty.
- **Underline, strike-out and squiggly are readable, not writable** — see the
  worker's own switch, above.
- **Nothing removes a highlight in the Tauri app** except the rebuild path.
- **A document that cannot be written keeps its markup in HyloPDF alone**, listed
  but not drawn on the page. Not lost, not portable, and the notice says which.

## Still to build: making a marked page look marked

A page *mark* — the pin, not the markup — exists in the Contents panel and
nowhere else. It should be visible on the page and findable in the document.
None of these changes what a mark *is*: a page number and a title in
`library.toml`, and each option is a different projection of that list.

**A mark stays out of the file.** It could be a `/Text` annotation now that this
app writes to files, and the answer is still no: a mark is where the reader was
going back to, which is navigation and not a change to the document. Putting a
sticky note on page 40 of somebody's shared PDF because they wanted to find it
again is not what they asked for. Offer it as a setting if anyone asks.

1. **A ribbon at the page's corner**, in the theme's accent, carrying the mark's
   title, clicking it takes the mark off. Pure DOM over the page, placed through
   `placeOverlay` so it follows a trim. About thirty lines. *Watch:* `.page` is
   `overflow: hidden`, so it sits just inside the right edge — which is margin
   on an untrimmed page and may be ink on a trimmed one, so it wants to be small
   and semi-transparent at rest.
2. **A dot on the thumbnail.** A class and a pseudo-element; the sidebar has to
   be *told* when marks change, and `showMarks` is already that single hook.

   Build 1 and 2 together: one says "this page", the other "these pages".
3. **A rail of ticks down the edge of the viewport**, each at its fractional
   position through the document. This is what makes marks navigable at book
   scale — forty marks in a list do not tell you they cluster in chapter 7. It
   needs `boxes`, so it needs the paged-mode guard, and it is the natural home
   for search matches and markup too, which argues for building it as "positions
   with colours" rather than as "marks".
4. **The page pill**, which already appears when the page changes: a glyph on it
   when the current page is marked is nearly free.
5. **The toolbar.** Redundant if 4 is built.
