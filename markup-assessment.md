# Coloured markup, written into the document

An assessment of adding reader-drawn colour markup to HyloPDF — arbitrary text
runs, arbitrary colours, several of them on a page — followed by a plan, and
then a shorter note on making the page marks that already exist visible on the
pages they are on.

Written against the tree at `bfed355`, `pdfjs-dist` 5.7.284. Every file, line
and API named below was read rather than remembered.

**The stipulation this is written under:** markup is written **into the PDF**
and is there the next time the file is opened, by this app or any other.
Careful markup that vanishes between sessions is worse than no markup, and a
reader who marks up a paper expects the marks to be in the paper. The earlier
version of this document argued the opposite from the "marks are not
annotations" line in `main.ts`; that line is disavowed and has been changed.

## The short answer

**Realistic, and writing into the file makes the viewer side simpler rather
than harder.** The reason is that a PDF already has the concept, pdf.js already
reads it, pdf.js already writes it, and this app already renders it:

| what markup needs | what already does it |
| --- | --- |
| a format for "these words, this colour" | `/Subtype /Highlight` with `/QuadPoints` and `/C` — the PDF spec's own, understood by Preview, Acrobat, Zotero, everything |
| writing that into a file without rewriting it | pdf.js `PDFDocumentProxy.saveDocument()` (`api.d.ts:1072`), which does an **incremental update** (`incrementalUpdate` in the shipped worker): original bytes untouched, new objects appended |
| drawing them | already happens — pdf.js paints an annotation's appearance stream into the page canvas, which is why other people's highlights already arrive highlighted |
| reading them back | `page.getAnnotations({ intent: "display" })` is already called on every mounted page (`viewer.ts:2234`), and `data.quadPoints` is in what it returns |
| a selection turned into rectangles on one page | `joinRuns` (`viewer.ts:2599`), which already rounds outwards and closes the gaps between pdf.js's spans |
| ink recoloured to a chosen colour, theme-aware | `drawRun` (`viewer.ts:508`), `duotone` and `restoreImages`/`tintLinks` (`themes.ts:384`, `themes.ts:778`, `viewer.ts:2691`) |

The anchoring problem that dominated the previous assessment mostly **goes
away**: a highlight's anchor is its `/QuadPoints`, in the file, in the page's
own coordinate space, and the document is the thing that answers "what is
marked here". No offset bookkeeping, no re-anchoring on zoom, no drift.

What replaces it is a harder and more consequential problem: **this app would
now write to the reader's files.** That is the riskiest thing HyloPDF has ever
done, it is where every remaining hour of this feature should go, and the rest
of this document is mostly about it.

Estimate: **1200–1600 lines net** across Rust, TypeScript, CSS and tests, and
call it a week rather than a few days. The risk is concentrated in one place
instead of spread thin, which is the better shape.

## What a highlight is

A standard markup annotation on a page:

- `/Subtype /Highlight`, `/Underline`, `/StrikeOut` or `/Squiggly` — four
  styles, all standard, all free.
- `/QuadPoints` — eight numbers per marked run, in PDF user space. This is
  exactly what `joinRuns` already produces, transformed through the page's
  viewport.
- `/C` — the colour, three numbers, and `/CA` for opacity.
- `/AP` — an appearance stream. **Not optional in practice**: many viewers draw
  only the appearance stream and ignore `/C`, so an annotation without one is
  invisible in half the world. pdf.js writes one; that is a strong argument for
  letting pdf.js do the writing rather than hand-rolling it in Rust.
- `/T`, `/Contents`, `/CreationDate` — who, a note, when. `/Contents` is where
  a comment on a highlight goes, which is the feature after this one and costs
  a field now rather than a migration later.

**pdf.js writes these itself.** The shipped worker has
`AnnotationFactory` cases writing `Subtype /Highlight` with an appearance
stream, driven by entries in `doc.annotationStorage` of the shape
`{ annotationType: AnnotationEditorType.HIGHLIGHT, color, opacity, quadPoints,
rect, pageIndex, … }`. `saveDocument()` then returns the whole new file as a
`Uint8Array`.

Two ways to put entries into that storage:

**Adopt pdf.js's annotation editor** (`AnnotationEditorLayer`,
`AnnotationEditorUIManager`, `DrawLayer`, `ColorPicker` — all exported by
`pdfjs-dist` 5.7). This is the supported path and it comes with the gesture,
the free colours and the deletion of existing highlights for nothing. It also
brings pdf.js's own DOM and its own look into an app that builds every element
by hand, sits on top of the text layer this app paints selections from, and
adds a second thing that owns the pointer inside a page. Worth a day's spike;
expect to reject it on fit.

**Write the storage entries directly** — construct the same object shape and
call `annotationStorage.setValue(...)`. Half a day, no foreign DOM, and it
depends on an internal shape rather than a documented API, so it is pinned to a
pdf.js version and needs a test that actually saves a file and reads a
`/Highlight` back out of it. This is the recommendation, with that test as the
condition.

The third option — **write the annotation in Rust** with `lopdf` or a
hand-rolled incremental update — is the one to keep in the back pocket. It is
the right long-term answer for very large files (only the appended bytes cross
the bridge and touch the disk, instead of the whole document twice), and it is
the wrong first answer, because appearance streams, encryption and xref
arithmetic are all things pdf.js already gets right.

## Writing to somebody's file

This is the section to read twice. None of it is exotic; all of it is the
difference between a feature and a bug report about a damaged document.

**Incremental update is the only acceptable form.** The original bytes are
never rewritten — new objects, a new xref pointing back at the old one with
`/Prev`, appended. A file that was fine before is byte-for-byte still in there,
which means a bad write is recoverable and a partial write is, at worst, an
update the next reader's xref repair will skip. pdf.js does it this way.
Anything that rewrites the whole document from a parsed model — which is what
the obvious `lopdf` route does — is a much larger promise about round-tripping
every strange file in the world, and this app should not make it.

**The document watcher will see our own write.** `watch.rs` follows the open
document and emits `document-changed`, and the frontend reopens the file and
puts the reader back where they were. Write a highlight and the app reloads the
document under the reader's hands — mid-scroll, losing the selection, on every
mark. This is the same shape as the themes directory, which the app also writes
to and also watches, and it is solved there by deciding from the *content*
rather than from the fact that something moved. Here the cheapest correct
answer is for the write to go through Rust, which already owns both the disk
and the watch, and for `follow` to hold a "this one is ours" mark that
`whole()`'s settle window clears. **Do not skip this.** It is invisible until
the first end-to-end test and then it is the whole feature failing.

**A file is open for reading while it is being written.** `OpenFiles` keys a
read handle by window (`lib.rs`), and a second window may have the same
document open. The write path must take the same lock the read path does, and
after a successful write both windows' transports are looking at a file that
has grown. Reopening is the honest answer, and the app is already good at it —
that is the `document-changed` path — which is another reason the write should
go through Rust and come back as the same event, with the "ours" mark meaning
"reload, but quietly, and keep the place".

**`saveDocument()` pulls the whole file.** The `SaveDocument` handler in the
worker begins with `requestLoadedStream()`, so a document this app has
deliberately been reading in 64KB pieces is fetched in full the moment the
reader marks a sentence. For a paper that is nothing; for a 500MB scan it is
the one place the piecewise reading is defeated, and then the new bytes come
back across the bridge and are written out again. Bound it: save on a debounce
rather than per gesture, save on close and on window destroy, and above a size
threshold say plainly that markup on this document is being kept beside it
rather than in it. The Rust-side incremental writer is the fix if that
threshold turns out to be low.

**Write atomically, and keep one way back.** `atomic_write` already exists and
is the right shape — temp file beside the target, then rename. The first time
this app appends to a given document it should also leave the original
recoverable: a `.hylopdf-original` copy beside it, or nothing at all if the
reader has said not to. The point is not to be clever, it is that the first
person whose thesis comes back wrong will not care how careful the design was.

**Things that make a file unwritable, all of which happen:** the file is
read-only or on a read-only volume; it is in a cloud-synced folder that will
resolve a conflict by keeping the wrong side; it is signed, and an incremental
update is exactly the thing a signature is there to detect (do not refuse —
warn once, plainly, and let the reader decide); it is encrypted, where pdf.js's
writer does encrypt what it writes but this app has one encrypted-document test
(`password.test.mjs`) and would need more; it is being recompiled by a
compiler that will overwrite it wholesale in a minute. Every one of these ends
in the same place: **the markup is kept in the journal and the reader is told,
once, in one line.**

## The journal, which is not the store

Since the file holds the truth, a local copy is no longer where markup lives.
It is still worth keeping, for four things a file cannot do:

1. **Surviving a recompile.** The case this app goes out of its way to support
   — a paper rebuilt by LaTeX under the reader — destroys every annotation in
   the file, because the file is replaced. The journal is what makes it
   possible to offer them back, matched by the quoted text with the matcher
   `search.ts` already has (`fold` handles the ligatures and soft hyphens that
   defeat naive matching).
2. **Documents that cannot be written**, above.
3. **Listing without opening.** A "marked passages across my library" view, and
   the sidebar's list before the page is mounted.
4. **Recovering from a bad write.**

So the journal stores, per document, per highlight: the quads, the colour, the
style, the quoted text, the page, the timestamp, and the annotation's id in the
file once it has one. It goes beside `marks` in `library.toml` through a door
shaped like `toggle_mark` — with the same TOML trap in mind, that an array of
tables written above a plain key swallows it (`Entry.marks` is last in its
struct for exactly this reason, and two tests say so). Nest one array of tables
inside another and it has to be last at both levels; a flat `Vec<f64>` of
quads, four numbers each, is a plain key and sidesteps it.

**The rule for divergence, stated once:** on open, the file wins and the
journal is rebuilt from what `getAnnotations` returns. The journal is a cache
and a recovery log, never an authority. Anything else is two sources of truth
and a bug that only reproduces on somebody else's machine.

Volume: a heavily marked book is hundreds of records, and `library.toml` is
rewritten whole under a lock on every change. If a document passes roughly a
thousand, move that document's journal to a sidecar (`markup/<hash>.toml`) — a
new door of the same shape, built when the reason is measurable.

## Drawing it, and the one trap

**Saved markup draws itself.** pdf.js paints an annotation's appearance stream
into the page canvas, so a highlight in the file arrives on screen as part of
the page bitmap with no layer, no overlay and no memory of its own. That is
already true today for other people's highlights. It also means markup goes
through recolouring like every other pixel, and the colour-keeping path added
to `recolor` is exactly right for it: a saturated wash keeps its hue and moves
its lightness, so a yellow highlight comes back light yellow on Hylo Dark
rather than grey.

**The trap is `WHITE_POINT`** (`themes.ts:44`, 235). Everything above level 235
is called paper and carried to the theme's background, because a hairline
printed at 90% white is invisible on paper and would otherwise arrive as a
bright cage around every hyperref box. A conventional highlighter wash — yellow
at 20–25% opacity over white — lands around level 250. **It is paper by that
rule, and it disappears entirely on every recolouring theme.** A reader would
mark a page, switch to dark mode, and find their markup gone. Three ways out,
and the third is the one that fits this codebase:

1. Only write saturated colours at an opacity high enough to clear the white
   point. Cheap, and it constrains "freely selectable" in a way the reader will
   notice.
2. Raise or special-case `WHITE_POINT`. No — it is load-bearing for hyperref
   boxes and for scans with warm paper, and the constant's own comment explains
   what it is buying.
3. **Redraw the marked quads over the recoloured page**, from the pristine copy
   taken before recolouring, toward a theme-adapted markup colour. This is
   precisely what `restoreImages` and `tintLinks` already do for pictures and
   links: one clip covering every rectangle at once, one `drawImage` per page.
   Markup quads become a third class of region alongside those two, they come
   from the `getAnnotations` call the viewer already makes, and the work is a
   variation on a function that exists rather than a new path. It also gets the
   theme adaptation for free, which options 1 and 2 do not.

Take option 3, and note that it wants the annotation quads on
`page.imageCoordinates`'s neighbour rather than in a new cache — the same
per-page bag the recolour path already carries.

**Unsaved markup is the app's own.** Between the reader marking a passage and
the save landing, the highlight does not exist in the file and pdf.js will not
draw it. `drawSelection` (`viewer.ts:473`) is almost exactly the painter for
that gap: it takes a slot, rectangles and a pair of colours, joins them into
runs, keys each run by place and colour, keeps what has not changed and
releases the rest. Reuse it for a "pending markup" layer, drop the layer when
the page next renders with the annotation baked in, and the reader sees the
mark appear under the pointer and never sees it flicker.

That leaves one invalidation to add: `keyFor` (`viewer.ts:1918`) decides when a
page is repainted, and it must learn about a markup revision, or a page marked
and saved will not redraw.

## Colours

The theme rule applies unchanged: **hex and nothing else, through
`parseColor`/`readColor` (`themes.ts:103`)**, never handed raw to CSS — the
theme picker's swatch lied about a colour exactly once and the fix was to route
it through the same reader the renderer uses. `ui.colorField` (`ui.ts:387`) is
already the control, used six times in the settings window.

Each highlight carries its own colour, which is what makes them freely
selectable and combinable; a palette of six in settings (`markup_colors`) is
the shortcut, not the constraint. The colour written to `/C` is the one the
reader picked — that is what other applications will show — and the colour
*drawn* on a recolouring theme is that colour adapted for contrast against the
theme's paper (`luminance` and `contrastRatio` are already in `themes.ts`). A
highlight is "the red one" on every theme, and it is red in Preview.

## The gesture, the sidebar, getting it out

- **A popover by the selection** on mouseup: a row of swatches, a "more" entry
  opening the colour field, and the four styles. `ui.showPopover` exists; the
  find bar's `FIND_KEEPS_OPEN` list and the popover Escape handling
  (AGENTS.md, "Escape and menus") both have to learn about it.
- **Actions in the keymap**, because everything here is an action first:
  `markup` (mark the selection in the current colour), `markup-color-1..6`,
  `markup-remove`. `copy-quote` (`keys.ts:151`) is the sibling that also works
  off the live selection, and its neighbour `mod+shift+h` is free.
- **A menu entry** under the document title, beside "Mark this page".
- The webview's context menu is already suppressed except over a live selection
  (`main.ts:1963`), which makes a right-click over marked text the natural home
  for "Remove markup".
- **The Contents panel** already carries a "Marked" section (`sidebar.ts:315`);
  markup is a second section — the quote in its colour, click to jump, a button
  to remove. A fourth tab is right at fifty highlights, not at the first.
- **Copy all markup as Markdown**, one blockquote per highlight with its page
  label. `copyQuote` (`main.ts:1128`) already formats a quote with its page and
  the document's title; this is that over a list, about forty lines, and it is
  what makes markup useful outside the reader.

Removal has a subtlety worth naming: deleting a highlight this app wrote is
another incremental update saying so. Deleting one **somebody else** put in the
file is a different act and should look different — offer it, but not on the
same button, and never silently.

**Correction, from actually building step 5 against pdfjs-dist 5.7:** the
paragraph above is wrong about how deletion would work, and the mistake is
worth recording so nobody re-derives it the hard way. `saveDocument()`'s
per-page pass (`Page.save` in the shipped worker) calls `annotation.save(...)`
on every annotation already in the file, and that is where an edit or a
deletion of an *existing* annotation would have to happen — but
`HighlightAnnotation`, `UnderlineAnnotation`, `SquigglyAnnotation` and
`StrikeOutAnnotation` none of them override it, so it resolves to the base
`Annotation.save()`, which returns `null` and does nothing. `deleted: true`
only has an effect on the *other* path, `saveNewAnnotations`, which is for
annotations that do not exist in the file yet — a deleted one there is simply
skipped, which removes nothing. So in this version of pdf.js, **an existing
`/Highlight` cannot be edited or deleted through `saveDocument()` at all**,
whether this app wrote it a moment ago or found it already in the file.
Removing markup for real needs the hand-rolled incremental-update writer the
plan already keeps in its back pocket for very large files; it did not get
built for step 5. What step 5 ships instead: creating a highlight works fully
and is proven end to end (see below), and there is no "remove markup" gesture
yet.

**And a second correction:** "pdf.js already writes it" (the short answer,
above) is only true for `/Highlight`. `AnnotationFactory.saveNewAnnotations`'s
switch only has cases for `FREETEXT`, `HIGHLIGHT`, `INK`, `STAMP` and
`SIGNATURE` — `UnderlineAnnotation`, `SquigglyAnnotation` and
`StrikeOutAnnotation` have no `createNewAnnotation`/`createNewDict` of their
own to be called at all. All three stay readable (`markupOf` already handled
them from step 3) but are not writable through this door. Highlight is the one
style step 5 builds.

## What this still does not do

- **A scan with no text cannot be marked this way.** No text layer, no
  selection, no quads. An area drag — a rectangle the reader draws — is the
  answer, it is a `/Square` annotation, and the storage above already holds it
  (quads, empty text). Design it in now, build it later.
- **Printing does not include markup**, because `print_document` hands the
  file to the system printer — though once markup is *in* the file, this fixes
  itself for everything saved, which is a second argument for the stipulation.
- **Comments on highlights are not editable**, only readable, as today. The
  `/Contents` field is written empty and the feature after this one fills it.
- **Two windows on the same document still go stale** in memory, as marks do.
  The write path forces this to be dealt with rather than noted, which is an
  improvement: whichever window writes, both reopen.
- **Underline, strike-out and squiggly cannot be created**, only read — see
  the correction above. Highlight is the one style this app can write.
- **Nothing removes a highlight**, this app's own or somebody else's — see the
  first correction above. The gesture only adds.
- **A document that cannot be written keeps its markup in HyloPDF alone**, and
  markup kept that way is listed but not drawn on the page — see step 7. It is
  not lost and it is not portable, and the notice says which.

## The plan

Ordered so the thing that could sink it is proven before anything is built on
it. The riskiest part is no longer the anchor; it is the round trip.

**0 · Spike the round trip, one day.** In the harness: open a fixture, put a
`HIGHLIGHT` entry into `annotationStorage`, `saveDocument()`, write the bytes,
reopen, and assert `getAnnotations` returns a `/Highlight` with the right quads
and colour — and that Preview shows it. Everything else depends on this being
true against pdf.js 5.7 with a range transport behind it. If the direct storage
route does not hold up, this is where the editor-layer alternative gets its
day.

**1 · The write door, in Rust.** `write_document(path, bytes)` beside the read
path: the same per-window lock, `atomic_write`, the one-time original copy, and
— the part that is easy to forget — telling `watch.rs` that this write is ours
so the reader is not thrown out of their scroll. Rust tests for the lock, the
atomic replace and the watcher suppression, which is where `cargo test` earns
its place in CI again.

**2 · The journal.** `Highlight` in `library.rs`, its door, its browser twin in
`api.ts` (without which the harness sees none of this), the flat-quads
decision, and a Rust test round-tripping an entry that has both `marks` and
`highlights` — the TOML ordering trap.

**3 · Reading markup out of the file.** A sibling to `notesIn`
(`viewer.ts:2634`) that picks the markup subtypes out of the annotations the
viewer already fetches, and the journal rebuilt from it on open. No UI yet;
a document someone else highlighted should now list its highlights.

**4 · Drawing.** The quad redraw over the recoloured page (option 3 above),
`keyFor` learning a markup revision, and the pending-markup layer lifted out of
`drawSelection`. Test under `HYLOPDF_NO_BLEND=1`, since that is the path Linux
may actually be on, and check a marked page on Hylo Light, Hylo Dark and
High Contrast — the white-point trap shows up on exactly two of those three.

**5 · The gesture and the colours.** Popover, actions, palette in settings
(keeping `tests/settings.test.mjs` green — it checks the Rust table, the
browser fallback and the `Settings` type against each other), theme adaptation,
the debounce, and the save on close.

**Done, in this shape:** `Viewer.captureSelection`/`Viewer.markSelection` in
`viewer.ts` build the `annotationStorage` entry and call `saveDocument()`;
`writeDocument` in `api.ts` is the door (with a browser twin that updates the
in-memory `File` and fires a synthetic `document-changed`, which is what makes
the whole gesture testable with no Rust behind it); `App.markSelection` and
`App.showMarkupPopover` in `main.ts` are the orchestration, bound to ⌘⇧H
(`markup` in `keys.ts`); the six-colour palette is `markup_color_1..6` in
`settings.rs` — independent scalar settings rather than one list, because this
settings table has no list type at all (see `same_shape` in `settings.rs`) and
adding one was a larger, riskier change than the feature asked for.

**Not done, on purpose:** the palette has no settings-window UI yet (only
sensible defaults, editable today by hand in `settings.toml`, same as anything
else there); the debounce and the save-on-close are not built — every mark
saves immediately, which is correct and simple for anything that is not a very
large document (see the two corrections above the plan for why a debounce
matters less than it looked like it would, and `markSelection`'s own comment in
`main.ts`); `keyFor` never learned a markup revision and `drawSelection` was
never asked for a pending-markup layer, because saving immediately and
reloading through the existing `document-changed` path already invalidates
everything a full `Viewer.load()` would — see the doc comment on
`Viewer.markSelection` for why that turned out to make the deferred machinery
unnecessary rather than merely delayed. No document-title-menu entry either:
clicking to open that menu collapses the text selection before the menu's own
`onSelect` ever runs, the same way clicking a colour swatch does — the swatch
path is fixed by capturing the selection when the popover opens rather than
when a swatch is clicked (see `captureSelection`), but the menu would need the
same capture one layer further out, over a click that also has to open a menu
first, and it was not worth it for a feature the keyboard shortcut already
reaches from anywhere. **Proven working**, not just tested against pdf.js's own
read path: `tests/markup.test.mjs` selects real text, drives the actual
gesture (⌘⇧H, click a swatch) and reads the highlight back through the normal
journal-sync path; separately, the bytes `saveDocument()` produced were run
through `mutool` (MuPDF, a PDF engine with no relation to pdf.js) and
rendered — a correctly positioned, correctly coloured highlight, on the exact
twelve characters selected.

**6 · The sidebar and the export.** Second section in Contents, jump, remove,
copy-all-as-Markdown.

**Done, in this shape:** `Sidebar.showHighlights` (`sidebar.ts`) is a second
section below `showMarks`'s "Marked" — always positioned right after it,
whichever of the two renders first, because `showHighlights` inserts itself
with `this.marksEl.after(box)` rather than a blind `prepend` — a coloured
swatch (`ui.swatch`, reused rather than a new one) and the quote, click to
jump. `App.showHighlights`/`App.highlights()` in `main.ts` are the
orchestration, called from the end of `syncMarkup` once the file has actually
been read — there is no synchronous path the way marks have one, because a
highlight's list has no meaning before the file says what it is. The section
heading carries "Copy all as Markdown" (`App.copyAllMarkup`), one blockquote
per highlight with its page label, the same shape `copyQuote` already gives a
single passage.

**Not done, on purpose: removal.** The plan above says "jump, remove,
copy-all-as-Markdown", and the middle one did not get built — see the
corrections above, under step 5: `saveDocument()` in this version of pdf.js
cannot edit or delete an annotation already in the file, whether this app
wrote it a moment ago or found it already there. A "remove" button in the
sidebar could only take the entry out of the *journal*, and `syncMarkup`
rebuilds the journal from the file on every open — so the entry would be back
the moment the document was reopened, with nothing to explain why. A button
that quietly undoes itself is worse than no button; `showHighlights`'s own doc
comment says this in place, for whoever next reads the code without this
document open beside it. Building it for real needs the hand-rolled
incremental-update writer the plan already keeps in its back pocket.

**A gap the plan did not name: `quote` was always empty.** `toHighlight` in
`viewer.ts` returned `quote: ""` unconditionally, with a doc comment saying
reading the words out from under a quad was "a job of its own for whichever
screen shows a highlight this app did not draw" — which is what this step is.
Without it, "the quote in its colour" in the sidebar plan had nothing to show:
every row would have read "Page 4" and nothing else, sighted or not. `quoteFor`
now reads a marked page's text once (`readTextItems`, grouped by page so a
page with three highlights pays for its text once) and keeps a text item only
when it sits *wholly* inside a quad's bounding box, not merely centred in
it — a producer that writes a whole line as one `Tj` (which the app's own test
fixtures do) hands back one item far wider than a highlight drawn over part of
it, and a centre-point test would have credited the highlight with the rest of
the line. The cost is the reverse case: a highlight over part of a wide item
gets no quote at all rather than the wrong one, which is the safer failure and
is exactly what the pre-existing `notes.pdf` fixture exercises — its
highlight's quote is still `""`, not because nothing changed but because its
one giant text item is wider than the box drawn over part of it.
`tests/fixtures/make-pdf.mjs`'s new `quote` mode is what proves the code path
that fixture cannot: one page, five separate `Tj` calls so pdf.js hands back
one item per word, and a `/Highlight` over the middle two — `quick brown` back
out, `The` and `fox` on either side excluded.

**7 · The edges, which are the feature.** Read-only and cloud-synced files;
signed documents; encrypted documents; the size threshold and the notice that
goes with it; the recompile path offering markup back; a scan saying plainly
there is nothing here to mark. Each is one notice line and one branch, and
together they are the difference between this being trustworthy and being
something people turn off.

**Done, in this shape.** `App.readMarkupStanding` (`main.ts`) asks four
questions once when a document opens and says nothing about any of them;
`MarkupStanding` is the answer it leaves behind, and `App.markSelection` is
where it is finally worth a sentence. The notice belongs to the first mark
because that is when it means something: telling somebody on open that a
document they have not marked cannot be marked is noise, and telling them
mid-gesture is the app arguing.

*Is it writable?* Asked of the disk, in Rust: `document_writability` opens the
file for writing and closes it again, which is the only form of the question
whose answer is actually true — a read-only file, a read-only volume, a file
somebody else owns and a sandbox that has not granted the path all come back
the same way, and none of them can be read off the permission bits. It reports
the size (see the threshold below) and names the syncing folder, if any, from
the same trip. `Writability` in `api.ts` has a browser twin seeded from
`localStorage`, which is what makes a read-only document testable in a harness
that has no disk: `openApp({ writability: { writable: false, reason: "…" } })`.

*Is it encrypted?* `Viewer.encrypted` — did pdf.js ask for a password on the
way in. pdf.js's writer does encrypt what it writes; this app has one
encrypted-document test, and markup that lands in a file nobody can open again
is the worst outcome available here, so it goes beside the document until
there is a suite that says otherwise.

*Is it too large?* `MARKUP_IN_FILE_LIMIT`, a hundred megabytes.
`saveDocument()` begins with `requestLoadedStream()` — the one place a document
this app deliberately reads 64KB at a time is read end to end — and then hands
the whole file back across the bridge to be written out again. This is the
answer the plan expected a debounce to be: past a certain size the round trip
is not a thing to make quicker, it is a thing not to make. The constant is not
a setting, because it is a fact about the round trip rather than a preference.

*Is it signed?* `reportFormFields` already made the one trip into the worker
that can answer this, so it answers it: a field of type `signature` among the
field objects. Not a refusal — `ui.confirmWrite` asks, once per document, and
the reader decides, because it is their document and an incremental update is
precisely the change a signature exists to detect.

All three of the first group end in the same place, which is what the plan
asked for: **the markup is kept in the journal and the reader is told, once,
in one line.** `App.journalSelection` is that path, and the entry it writes
carries the same quads `Viewer.quadsFor` gives the write path and the words
that were selected — so it lists, copies out and can be put into the file
later exactly like markup that did land in one. What it does not do is appear
on the page, and the notice says so rather than leaving the reader to notice.
The sidebar marks those rows as not being in the document (`.highlight-row.aside`):
a row that looked identical to markup in the file would be telling a small lie.

**The recompile path, which is the one that needed real machinery.**
`Viewer.findQuote` re-anchors a lost highlight by looking its quote up again —
`fold` from `search.ts`, now exported and imported by `viewer.ts` for this one
purpose, because a rebuilt paper has usually been re-typeset as well as moved,
and a ligature the new run of LaTeX chose differently would otherwise lose the
passage. It starts at the page the highlight used to be on and works outwards,
stopping at the first page that carries the words; `quadsAround` turns the
matched text items back into quads, one per line, padded below the baseline so
that `quoteFor` reads the same words back out of them afterwards.
`App.restoreMarkup` puts every one it found back in a single `saveDocument()`
and one write, drops the journal entries it restored *before* writing (or they
would come back as lost forever, holding the old file's quads), and says how
many it could not find rather than putting the markup on the nearest words.

It is offered rather than done: a button in the Contents panel's Markup
heading, shown only when the journal holds something the file does not and the
document would take it. Re-anchoring is a guess, however good a one, and this
app does not write to somebody's file without being asked.

**And `syncMarkup` is where the authority rule got its exception.** "On open,
the file wins and the journal is rebuilt from it" held for as long as the
journal was only ever a copy of the file. It is not any more: a mark kept
beside an unwritable document is the only copy there is, and a mark a rebuild
took away is the only reason the offer above exists. Both survive the rebuild,
both carry `annotation_id: null` — which is exactly what they have in common,
the journal knowing about them and the file not — and everything the file does
carry still comes from the file. The rule is unchanged for every highlight
that has an id.

**One bug fell out of building it, and it was not a markup bug.** `App.open`
built a fresh library entry for the document it was opening, dropping whatever
the old one held — so a document's marks were gone from memory the moment it
finished opening, and only came back because `toggleMark` reads the list back
from Rust. Harmless for marks and fatal here: a reload is an open, every mark
causes a reload, and `syncMarkup` reconciles the journal against the file. A
journal emptied on the way in reconciles to whatever the file happens to say,
which for an unwritable document is nothing at all. The entry now carries the
marks and the journal across, and `syncMarkup` is handed the journal
explicitly rather than reading it back out of a field somebody might rebuild
again.

**Tests.** Four in `markup.test.mjs`, on top of the six that were there: a
read-only document keeping markup beside it and saying so once (and not
twice); a scan saying there is nothing to mark rather than "select something
first"; a signed document asking, being refused, being agreed to, and not
asking again; and the whole recompile round trip — mark a word, put the
document's original bytes back the way a rebuild would, watch the offer
appear, press it, and find the highlight in the file again on the same word,
in the colour it was. Three in `lib.rs` for the writability probe: a read-only
file, a file that is not there, and the syncing folders named by prefix within
a whole path component ("OneDrive - Acme", "Dropbox (Personal)"). One new
fixture mode, `signed`, which is an AcroForm with a signature field —
`getFieldObjects` reports it and nothing else about it is real, which is
exactly the part being tested.

**Not done, on purpose.** Removal still does not exist, for the reason under
step 6: this version of pdf.js cannot edit or delete an annotation that is
already in the file. A journal entry the reader wants gone has no gesture
either — the same trap, one layer out, since `syncMarkup` would keep it only
if the file lacks it, which it does by definition. The area drag for a scan is
still designed and not built. And the notice for a very large document is the
threshold rather than the Rust-side incremental writer that would remove the
need for one; that writer is still the thing to build if the threshold turns
out to bite, and it is still what removal would need first.

**Tests to add**, in the shape the suite already has: a save-and-reopen
round trip (the phase 0 spike, kept), `markup.test.mjs` for quads from a
selection and the re-anchor-by-quote path, reader-level select → mark →
reopen → still there, the Rust tests above, and a `seams` addition if a new
import lands in `viewer.ts`. Everything waits for a condition rather than for a
clock.

---

# Bonus: making a marked page look marked

A mark today exists in the Contents panel and nowhere else. It should be
visible on the page and findable in the document. Five options, best value
first.

**A question the stipulation reopens:** now that this app writes to files, a
page mark *could* be a `/Text` annotation — a sticky note in the corner, which
every other reader would show. Recommendation is still no, and for a reason
that is not the disavowed one: a mark is where the reader was going back to,
which is navigation state and not a change to the document, and putting a
sticky note on page 40 of somebody's shared PDF because they wanted to find it
again is not what they asked for. Markup is a change to the document and
belongs in it; a mark is a bookmark and belongs beside it. Offer writing marks
into the file as a setting if anyone asks for it.

**1 · A ribbon at the page's corner.** A small tab in the theme's accent at the
top-right of a marked page, carrying the mark's title as its label, clicking it
takes the mark off. The physical metaphor — a bookmark sticking out of a book —
and it reads instantly with no legend. Pure DOM over the page, placed through
`placeOverlay` (`viewer.ts:1152`) so it follows a trim, no canvas, survives
zoom. Roughly thirty lines and some CSS.

*One thing to watch:* `.page` is `overflow: hidden` (`styles.css:738`), so the
ribbon cannot hang outside the paper — it sits just inside the right edge. On
an untrimmed page that is margin and it looks like a bookmark; with the margins
trimmed away it may land on ink, so it wants to be small and semi-transparent
at rest, solid on hover.

**2 · A dot on the thumbnail.** The Pages tab already builds a button per page
(`sidebar.ts`), so this is a class and a pseudo-element — about ten lines. The
one real requirement is that the sidebar be *told* when marks change;
`showMarks` (`main.ts:1098`) is already that single hook, so it grows a second
call rather than a second mechanism.

Build 1 and 2 together. One says "this page", the other says "these pages".

**3 · A rail of ticks down the edge of the viewport.** Each mark at its
fractional position through the document, click to jump. This is what makes
marks navigable at book scale — a list of forty marks does not tell you they
cluster in chapter 7. It needs `boxes`, so it needs the paged-mode guard, and
it is the natural home for search matches and for markup too, which argues for
building it as "positions with colours" rather than as "marks". Sixty lines
plus styling, after 1 and 2 have been lived with.

**4 · The page pill.** `.page-pill` (`styles.css:1380`) already appears when
the page changes; a small glyph on it when the current page is marked is nearly
free and gives the mark toggle the feedback it currently lacks anywhere outside
a menu.

**5 · The toolbar.** The document menu shows the mark as a tick
(`main.ts:1525`) and nothing at the top level does. If 4 is built this is
redundant; if it is not, the toolbar is where it belongs.

None of these changes what a mark *is*, which is why they are cheap: a mark is
already a page number and a title in `library.toml`, and every option above is
a different projection of that same list. The work is presentation, and it is
the presentation that is missing.
