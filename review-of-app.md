# A critical read of HyloPDF, from the outside

> **Status.** Everything below has been acted on except the three items marked
> **not done** here, which are named again in place. The commits are one per
> point, in the order §6 sets out.
>
> | | |
> |---|---|
> | §1.1 back from a link | done — ⌘[ / ⌥←, and the mouse's side buttons |
> | §1.2 printing | done as a hand-over: ⌘P gives the document to a program that prints, and says so — though the notice lands after focus has already left for that program, so it is easy to miss; see the note in place |
> | §1.3 page labels | done — toolbar, pill, thumbnails and the go-to field |
> | §1.4 recents while reading | done — the document's name in the bar carries the list |
> | §1.5 two documents at once | **not done**; see the note in place |
> | §1.6 selection past the mounted band | done, in part — ⌘A still means a page; the "Copy this page's text" button was removed rather than kept, see the note in place |
> | §2.1 rotate | done — ⌘R / ⌘L |
> | §2.2 two pages side by side | done, with the cover on its own |
> | §2.3 trim the margins | done — measured over a sample, off by default |
> | §2.4 search results list | done — a third tab, reached from the count |
> | §2.5 follow the system's light and dark | done, and on by default |
> | §2.6 reopen the last document | done, and on by default |
> | §2.7 document properties | done, and the document's own title is used where it is worth using |
> | §2.8 zoom shortcuts | done — ⌘1 actual size, ⌘2 fit page |
> | §2.9 hand tool | done — the middle button drags |
> | §2.10 presentation mode | done — ⌘⇧P |
> | §2.11 remappable keys | **not done**; see the note in place |
> | §3 no-text scans, the search cap, the missing help key, the menu-bar gap | done |
> | §3 soft at very high zoom | **not done**; see the note in place |
> | §4 the three silent failures | done |
> | §5 marks, and quoting with a page number | done; reading a document's own notes, too |


What follows is a review of the app as a *reader*, not as a codebase: what
somebody arriving from Preview, Acrobat, Edge, Sumatra, Okular, Zathura or
Skim will reach for in the first hour, and what they will find. It was done by
reading the source rather than by watching anybody use it, so everything here
is a claim about what the app can and cannot do, with the file that says so.
Where it guesses at how much people will mind, it says it is guessing.

Scope is taken as given: HyloPDF is a reader, and annotations are out. Section
5 is about which parts of the "annotations" demand are not actually
annotations, because two of them are cheap and are worth having whatever is
decided about the rest.

The app is in unusually good shape on the axes the brief names — the memory
discipline, the theming, the position memory, the live reload when a compiler
rewrites the document, the password handling, the keyboard and screen-reader
work. None of that is repeated below. This is a list of what is missing and
what is awkward.

---

## 1. The six that will be missed first

Ranked by how quickly an ordinary reading session runs into them.

### 1.1 There is no way back from a link

Following a citation, a footnote or a cross-reference is a one-way trip.
`goToDestination` (`src/viewer.ts:1587`) scrolls and forgets; so do the outline
buttons in `src/sidebar.ts` and the page-number field. Nothing keeps a history,
so the reader who clicks "see Theorem 4.2" on page 12 and lands on page 190 has
to remember where they were and type it back.

Every other reader binds this: Preview ⌘[ / ⌘], Acrobat and Sumatra Alt+←,
Okular the same, and all of them also answer the mouse's back button. It is the
single most-used navigation command in a technical document after scrolling.

Cheap: a stack of `{page, offset}` in the viewer, pushed by
`goToDestination`, `goToPage` from the outline and the page field, and
`revealMatch`; ⌘[ / ⌘] plus `auxclick` buttons 3 and 4. Half a day, and it
removes the largest reason to keep another reader installed.

### 1.2 Printing does not exist

Nothing in the app or in Rust answers ⌘P — the string "print" appears in the
tree only in prose. This is the one absence that will read as "unfinished"
rather than as "minimal", because printing is not a power feature; it is what
people do with a boarding pass, a form, a chapter they want on paper.

There is no cheap route, which is presumably why it is not there. The two
honest ones: hand the path to the platform's print service from Rust (a new
door in `api.ts`, one per platform), or do what pdf.js's own viewer does —
render the requested pages to canvases at print resolution in a hidden document
and call `window.print()`. The second keeps the work on the side of the app that
already knows how to rasterise, and it inherits the page range and the printer
dialog for free.

If it stays out for 0.1, say so somewhere the reader will look, rather than
letting ⌘P do nothing.

**Update.** Built as the hand-over: `print_document` opens the file in the
platform's own viewer and `App.print` says so. What the review did not weigh
is that the notice fires at the exact moment the reader loses it — focus has
already left for the other program by the time it appears, so the one
sentence explaining the hand-over is easy to never see. A label that says what
is about to happen ("Print in Preview…" rather than "Print…") would carry the
weight the notice currently cannot; not yet done.

### 1.3 The page number is the wrong number

There is no `getPageLabels` anywhere in `src/`. The toolbar and the go-to field
speak in physical page indices, so a book with roman front matter is off by
twenty for its whole length, and "go to page 314" from an index or a citation
lands somewhere else. Every reader that matters here shows the label — Preview
puts it in the toolbar, Acrobat accepts either, Sumatra shows "xii (12 / 340)".

This is one pdf.js call, a map, and a change to the field's parser: accept a
label first, fall back to the index. It disproportionately affects exactly the
documents this app is for — books, theses, standards, anything typeset properly.

### 1.4 The recents list disappears the moment you start reading

`renderRecents` fills `#recents`, which lives inside `#welcome`, which is shown
only under `#shell[data-empty="true"]` (`src/styles.css:708`). So the list of
what you were reading — with the page you stopped on, which is the nicest
detail in the app — is reachable only when nothing is open. With a document up,
the only route to another document is the file picker, and the only route back
to the welcome screen is closing the one you are reading.

The fix is a menu, not a new screen: "Open recent" hanging off the Open button
(or off the document title, which already has a context menu at
`src/main.ts:1039`), listing the same twenty-four entries `library.toml` holds.

### 1.5 One document at a time, with no way to say otherwise

The single-instance plugin routes every "open with" into `hand_over`
(`src-tauri/src/lib.rs:606`), and the frontend's `open` replaces whatever is on
screen. So opening a second PDF closes the first, and there is no tab strip, no
second window, and no split.

The reasoning for single-instance is sound and is about `settings.toml`, not
about windows. But comparing two papers, or reading a paper beside its
appendix, is ordinary work, and every one of the comparison apps does it —
Preview by windows, Acrobat and Edge by tabs, Sumatra by both.

**Not done.** This is the largest item on the list by cost, because the app's
state is one `App` object and one window. Two viable shapes: a second Tauri window sharing
the Rust side (settings and library are already lock-serialised, so the hard
part is done), or accept the constraint deliberately and document it. What is
not viable is the current middle: a reader who double-clicks a second PDF
loses their place in the first with no warning. At minimum, that should not be
silent.

### 1.6 A selection cannot be longer than about a screen and a half

Pages outside `OVERSCAN` (0.6 viewports, `src/viewer.ts:125`) are removed from
the DOM, and their text layer goes with them. So a selection dragged past the
mounted window is cut off, and ⌘A selects only what happens to be mounted.
Copying a paragraph works. Copying a section does not, and — worse — it looks
like it did, because the highlight is on screen while the drag is happening.

Two mitigations, in increasing order of cost: a "Copy text of this page" item
in a document context menu, which covers most of what people actually want; or
keeping text layers (not canvases — they are cheap by comparison) for a much
wider band than the canvases, so a selection has something to anchor to.
Whatever is chosen, the current silent truncation is the bad case: text
disappears from the clipboard without anything saying so.

**Update.** The cheap mitigation was built and then taken back out. `⌘A`
(`selectThisPage`) is real and stays: it selects a mounted page's own text
layer by DOM range, says how far a selection goes and why, and a reader can
copy that selection normally. What did not survive is the separate one-click
"Copy this page's text" menu item, which went through pdf.js's own
`getTextContent` on a page proxy rather than the rendered text layer — and
wrapped any failure of that call, `catch { return "" }`, in the same message
as a page that genuinely has no text. On at least one real document it
reported exactly that for a page with several kilobytes of extractable text.
A button whose failure mode is indistinguishable from its correct answer is
worse than not having it, so it is gone rather than fixed; a real fix, if
it happens, is still one of the two mitigations above, done properly.

---

## 2. The next tier

Things people will notice in the first week rather than the first hour.

| | What is missing | Who misses it | Rough cost |
|---|---|---|---|
| 2.1 | **Rotate the view** | Anyone with a sideways scan or a landscape table; the fix in every other reader is one keystroke | Small — a rotation in `getViewport` and in the layout boxes |
| 2.2 | **Two pages side by side** | Wide screens, slides, facing-page books. "Fit page" on a 16:9 display wastes half the width | Medium — the layout already places boxes; this is a second placement rule |
| 2.3 | **Crop the margins** ("fit content") | Scanned books and LaTeX with 1.5-inch margins, where fit-width gives you 60% type and 40% paper. Zathura, Sumatra and Sioyek all have it, and it is the highest-value zoom feature for the documents this app is aimed at | Medium — the bounding box of the ink per page, which the recolouring pass is already reading pixels for |
| 2.4 | **A results list for search** | Search says "3 of 128" and nothing else. Acrobat, Okular and Sumatra list the hits with a line of context, which is how you scan for the right one rather than stepping through 128 | Medium — the text and offsets are already indexed in `search.ts`; this is a third sidebar tab |
| 2.5 | **Follow the system's light/dark** | Nothing in `src/` reads `prefers-color-scheme` (`matchMedia` appears once, for screen density, `src/viewer.ts:827`). The app has a light theme and a dark theme and a switch between them, and no "Auto" — so a reader whose machine turns dark at sunset has to turn the app dark by hand | Small, and it fits the existing `light_theme` / `dark_theme` pair exactly |
| 2.6 | **Reopen what I was reading** | Launching always lands on the welcome screen. The library knows the answer; this is an opt-in setting away | Small |
| 2.7 | **Document properties** | Title, author, page size, producer. Also: the window title and the recents list use the file name, so a shelf of `2310.06825v3.pdf` is unreadable when the PDF's own metadata has the title | Small |
| 2.8 | **Zoom shortcuts that match the neighbours** | ⌘0 is fit width (`src/main.ts:1472`); in Preview ⌘0 is actual size and fit-width is elsewhere. There is no shortcut at all for fit page or 100% | Trivial, but pick deliberately — this is muscle memory and there is no right answer, only a documented one |
| 2.9 | **A hand tool / drag to pan** | Zoomed into a map or a large figure, the only way sideways is the scrollbar or shift-wheel. Preview and Acrobat let you drag the page | Small — a modifier-drag or a mode |
| 2.10 | **Presentation mode as a named thing** | Full screen + hide toolbar + one page at a time is already presentation mode; nobody will find it by assembling three switches. One item that sets all three, and Escape out | Trivial |
| 2.11 | **Remappable keys** — *not done* | Only j/k are offered to the Vim-shaped reader — no h/l, no gg/G, no ctrl-d/u, and nothing is remappable. `AGENTS.md` already names a keybindings file as future work; the audience that cares about this is exactly the audience that would otherwise use Zathura or Sioyek | Medium |

---

## 3. Frictions in what is already there

These are not missing features; they are places where the app as built will
confuse somebody.

**A scan with no text layer explains nothing.** Search returns "None"
(`src/main.ts:868`), selection does nothing, and the Contents tab is empty. All
three are correct and all three look like bugs. One line — "This document has
no text in it; it is a scan" — turns three mysteries into one fact. The app
already knows: the first page's text runs come back empty.

**The search cap stops the scan, not just the count.** `MATCH_LIMIT` is 2000
and, on reaching it, `find` breaks out of the page loop entirely
(`src/search.ts:200`). So searching a common word in a long book indexes the
first stretch and silently stops: the later pages are not "capped", they are
unsearched, and stepping through matches will never reach them. The "+" in
"2000+" is doing a lot of work. Either say what it means ("first 2000 matches")
or keep counting without keeping every hit.

**Very high zoom goes soft** — *not done*. `MAX_CANVAS_PIXELS` is 12 million
(`src/viewer.ts:131`), so past roughly 300–400% on a large page the render is
downsampled and the type blurs — at precisely the moment somebody is zooming in
to read a footnote or inspect a figure. The cap is right; the answer is to
render only the visible tile at full density rather than the whole page.

**The two platforms disagree about what a menu bar is.** Tauri installs its
default macOS menu (`enable_macos_default_menu`, on by default in 2.11.5), so a
Mac gets Copy, Select All, Close Window, Hide and Quit for free. Windows and
Linux get none of that, and the app supplies no menu bar of its own — so on
those platforms there is no discoverable Copy, no Open Recent, no Print, and no
File menu at all. Whatever is decided about a menu bar, it should be decided
once rather than inherited differently on each platform.

**There is no help key.** The Keyboard page in Settings is good and complete
enough to be the answer to "what can this thing do", and it is three clicks
away behind a gear. F1, or ⌘/, should open it. The page also omits two things
the app does listen for: pinch and ⌘-scroll to zoom, and the top-edge reveal of
the toolbar.

**"Toolbar hidden" is a good notice with a short life.** It is the only route
back and it is spoken once, transiently. The peek handle covers this in
practice; worth checking with somebody who has never seen the app.

---

## 4. Silent failures worth naming

The app is careful about this in several places (`unreadableColors`, the
password decline, the reload notice). Three cases are not yet covered:

1. **Form fields.** `page.render` is called with pdf.js's default annotation
   mode, so a fillable form draws its fields and its existing values, and none
   of them can be typed into — there is no annotation layer anywhere in `src/`.
   A person opening a tax form will click a box and get nothing, with no
   explanation. Detecting a form is one call; saying "this document has fields
   HyloPDF cannot fill" costs a notice.
2. **Opening a second document** closes the first without a word (§1.5).
3. **A truncated selection** copies less than was highlighted (§1.6).

---

## 5. Where "annotations" is the wrong word for the demand

Annotations are out of scope, and that is a defensible line. But three things
usually bundled with them are not annotations, and two are cheap:

- **Bookmarks of your own** — named places in a document, saved per file.
  Sumatra, Okular and every e-reader have them, and they are pure reading, not
  markup. `library.toml` is already the right home, and `library.rs` is already
  the right shape for it. This is the item I would expect the most demand for
  from the "I miss X" direction.
- **Copy a quote with its page number.** One clipboard format, no document
  modification, and it is what academic readers actually do with a selection.
- **Reading existing annotations** already works — highlights and notes made in
  another app are drawn into the page canvas by pdf.js. What is missing is
  seeing a comment's *text*: a popup annotation renders as an icon with nothing
  behind it. A read-only comments list in the sidebar is not an annotation
  feature; it is a viewer feature, and it makes "no annotations" a much easier
  sentence to say.

Genuinely out, and rightly: creating highlights and notes, editing, redaction,
signing, page manipulation, export and "save a copy". If demand does arrive, it
will arrive for highlighting first, and the honest answer for a long time is
"open it in something else" — which the "Show in Finder" item already supports.

---

## 6. If it were mine, in this order

(This was the plan; everything in it is done except the eighth.)

1. Back/forward after a jump (§1.1) — a day, and it removes the biggest daily
   annoyance.
2. Page labels (§1.3) — half a day, and it fixes a wrongness rather than an
   absence.
3. Open Recent while reading (§1.4) — half a day.
4. The three silent failures (§4) — a day between them, and each one converts a
   "this is broken" into a "this app knows what it is".
5. Follow the system appearance (§2.5) and reopen the last document (§2.6) —
   both small, both the kind of thing whose absence reads as unfinished.
6. Rotate (§2.1) and crop margins (§2.3) — the two view features the target
   documents most want.
7. Printing (§1.2) — larger, unavoidable before anybody calls this their
   default PDF reader.
8. Two documents at once (§1.5) — the architectural one. Decide it rather than
   inherit it.

Everything in §2 past that point is worth doing and none of it is urgent.

---

## 7. Why the three left undone were left undone

**Two documents at once (§1.5)** is not a feature, it is a change to two
things this app is built on. `open_for_reading` and `read_range` serve
"whichever document is currently open" — one file handle, global — and every
chrome setting, including the window's own geometry, is global too, so a
second window would spend its life overwriting the first one's. Doing it
properly means keying the file handle, and deciding which settings belong to
the app and which to a window. Nothing about it can be exercised in the
harness either: a second window is a Tauri path, and the harness has no Rust
behind it. It deserves the decision the review asked for, taken deliberately,
rather than a change made at the end of a long day. What has changed in the
meantime is the sting: the position in the outgoing document was always kept,
the way back to it is now one click in the bar, and the app says what it
closed and why.

**Remappable keys (§2.11)** wants a file parsed and validated in Rust and a
dispatch table in TypeScript, and the table is the problem: `wireKeyboard` is
twenty-five branches whose order is load-bearing — ⌘⇧F before ⌘F, ⌥⌘G matched
on `event.code` because Option turns a G into a ©, plain keys only when
nothing is held — and this pass added ten more of them. Rewriting the app's
most delicate input path immediately after widening it is the way to introduce
the kind of bug the tests do not catch. It is a good next piece of work and it
should be its own.

**Sharpness past 300% (§3)** is the same machinery the margin trimming now
uses — draw a sub-rectangle of the page at full density — pointed at the
viewport instead of at the ink. What makes it more than an afternoon is what
else reads the page canvas: `paintSelection` copies pixels off it assuming the
canvas is the page box, and the recolouring works in rectangles of the same
space. A tile that follows the viewport has to compose with a crop that does
not, and getting that wrong is a visual fault no test in this repository can
see. The renderer took three changes today; this one can wait for a pass of
its own.
