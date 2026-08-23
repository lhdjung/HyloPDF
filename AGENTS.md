# HyloPDF, a smooth reading experience

Note: everything down to the horizontal rule describes what the project SHOULD be like. Below it, "Architecture of the built app" describes what it currently is.

## General
HyloPDF is a PDF reader written in Rust with Tauri. Cross-plattform, ergonomic, with a calm UI, and efficient: fast with no lags, little memory and CPU consumption, and a small binary.

Importantly, all settings are preserved throughout sessions, and all of them are independent of each other: changing one setting does not change any other setting.

## UI
The UI is clean and sleek. It is close to full-screen by default. In particular, the app should reserve much or most or all of the vertical axis for the document, as there is likely more room to the sides. No clunky or overbearing UI covers the document. However, true full-screen – no UI elements at all – is easily toggleable, and leaving it should be at least as easy and obvious.

Page progression is continuous scrolling by default, and it's a strong default: it can only ever be changed to anything else if the user explicitly opts into it. Not sure changing this should even be possible using shortcuts because continuous scrolling is so much better than the alternatives, and hitting such a keybind by accident would be frustrating.

There is no clutter in the UI. All elements are nice, modern, polished, and look straight out of professional web design. However, they should not have the typical vibe coded look, i.e., small caps (or caps in general), italics, exotic fonts, and a kind of dead, technical, sterile look. On the contrary, the look should be friendly and open; fresh and lively but in a subtle way.

UI elements might include symbols but they are definitely not just symbols, and not just tiny symbols. For each element, a combination of one symbol and one succinct text label would probably be good.

No animations unless the user takes an action. No pop-up windows that get into people's way.

## Theme settings
The app has dark mode that is easy to toggle (via UI or shortcuts) and that has a customizable definition: text, background, accent, and link colors can be any color chosen by the user, but with sensible defaults. It isn't black by default because the contrast would be too high. The text selection color should be customizable in the same way, and harmonize with each individual theme.

The app supports multiple themes, where each theme is a text-background color combination. Some themes are preinstalled, but users can define and name their own themes. Each theme has a name.

I guess, but I'm not certain, that themes are stored in some kind of config files (one per theme). Possible advantage: easily LLM-able if people want to create a theme but don't want to get in the technical weeds. If we do go this way, choose a good config file format, like TOML or whatever Ghostty uses.

## Preinstalled themes
Ignoring some settings, we have:
- Hylo Light: the default light theme, and the overall default theme. Doesn't change colors at all.
- Hylo Dark: the default dark theme. Text is white. Background is a dark grey, with maybe a tint of slate blue.
- Pzazz: cool and glamorous dark theme inspired by the Charm / Bubble Tea aesthetic.
- Dracula: text is pink, background is dark blue-ish. Some light blue and/or green is sprinkled in. Maybe that's not accurate – check the Dracula themes other apps use, and how that would translate into PDF theming.
- Gruvbox, for the oldies.
- Sepia: background is sepia, text is dark. Use whatever good sepia themes use.
- High contrast: background is perfect black, text is white.


---


# TODO
Tackle each chunk in a separate session. Make a Git commit after each numbered task except those in Chunk C, and except there's a reason to combine two or more tasks.

## What Chunk B left behind
- High Contrast puts a black page on a black backdrop, so nothing marks where
  one page ends and the next begins. The page shadow is the thing that would
  say so and it cannot, on black. Worth a look if that theme gets used.
- The toolbar takes the paper's colour but the chips inside it — a hover, a
  pressed switch, the zoom group — are still derived from `--surface`. On Sepia
  that reads as a cold note on warm paper. It is subtle, and the fix is a
  second family of derived shades, which is more than it is worth today.
- A selection dragged across a picture recolours that part of the picture, the
  same way it recolours type. It is the right answer for words and an arguable
  one for a photograph, and telling the two apart is more than the case is
  worth today.

## What Chunk C left behind
Both questions were answered rather than built: the renderer stays pdf.js, and
the Rust/TypeScript seam stays where it is. "The renderer is the replaceable
part, and it was tried" now carries the verdict on each of the three options
and on two more that were weighed, and "Where Rust ends and TypeScript begins"
carries the rule the first question produced. `pdfium-prototype` stays parked
and unmerged; nothing in it changed.
- Built since: `watch.rs`, which is what the second question was pointing at —
  the themes directory and the open document, watched in Rust. The Settings
  window redraws its theme list when the disk changes, but only between edits;
  a theme being written in the app is left alone.
- Keybindings as a config file, parsed and validated in Rust the way themes
  are. Wanted by the brief's customisation aims, and it costs one door.
- The pdfium prototype has two things worth keeping whatever happens to it: it
  opens a document two orders of magnitude quicker than pdf.js, and it can
  read an encrypted one. Neither is a reason to swap the renderer, and both
  will look like one the next time this comes up.

## Chunk D
CI had never been green, and the three reasons were unrelated to each other and
to the chunks. All three are fixed; each left a note below.
- `RunEvent::Opened` is an Apple Events variant and does not exist off Apple
  platforms, so Linux and Windows would not compile. Now `#[cfg]`-gated.
- The bundle step named the Apple signing variables in its `env`, and a secret
  this repository does not have arrives as an empty string rather than as
  nothing — which the Tauri bundler reads as a certificate and fails to import.
  macOS compiled perfectly and died at `security import` every time.
- `reader.test.mjs` pressed a hard-coded `Meta`, and the app takes its shortcut
  scheme from `navigator.platform`. On CI, which is Linux, ⌘F was not a
  keystroke the app knows, so the find bar never opened and the toolbar, once
  hidden, never came back — thirteen failures, none of them about what they
  said they were about. `HYLOPDF_PLATFORM=other` now reaches that from a Mac.

## What Chunk E left behind
Four changes to the bar and one measurement. The bar now derives its chips from
the paper it sits on rather than from the backdrop, so a hover, a held-down
button, the zoom group and the page field belong to whatever theme is on; the
three search switches wear the accent the way the buttons above them do; the
way out of the app is a Quit button beside Open instead of a corner of the
start screen; and the seventy-eight pixels macOS keeps for the traffic lights
are charged only when there are traffic lights to keep them for. What the fifth
question found is in "What a reading session costs" below.
- The chips are mixed towards the theme's own ink unless the paper cannot
  support it, which happens when a theme leaves the document alone and its
  chrome lands on white. The guard keeps the field visible; it does not fix the
  larger version of the same problem, which is that a dark theme with
  `recolor = false` shows light labels on a white bar. No built-in theme is in
  that shape, and any theme that is looks broken well before the chips do.
- High Contrast still puts a black page on a black backdrop, from Chunk B. The
  bar's own fields on it are now `#131313` on black — present, but only just.
  It is the theme's nature rather than the derivation's.
- The measurement found the app using 3.2GB to read a scanned book on a
  machine with 8GB, and the fix went in after it: pages carrying pictures now
  have a cap of their own. 900MB, and flat. "What a reading session costs"
  below carries the numbers, where the memory actually was, and the three
  plausible fixes that turned out to change nothing.
- `IMAGE_PAGE_CACHE` was six because six is comfortably more than the three
  pages that can be on screen, and nobody had measured what the room behind
  them cost. It is three now, and the measurement is in "What a reading session
  costs" below. Three is the mounted set and nothing behind it, and what it
  charges is one page decode when a reader scrolls back further than the screen.
- The thing the cap was containing has largely gone: pdf.js was asked to stop
  handing over ready-made bitmaps, and a scan that cost 630MB to read now costs
  263MB, flat, with the pages identical to the pixel and quicker to draw. See
  "Images cross the worker boundary compressed" below. The cap is still worth
  keeping, but it is photographs it earns its keep on now rather than scans.

# Architecture of the built app

Everything above is the brief. What follows describes the app as it actually
stands, so that a change can be made without reading every file first.

## Shape

A Tauri 2 desktop app. Rust owns the disk and the window; a TypeScript frontend
owns everything the reader sees. Pages are drawn by **pdf.js** (`pdfjs-dist`)
onto plain canvases. There is no framework and no state library — the interface
is built with `document.createElement`, and one `App` object holds the state.

```
src-tauri/          Rust: settings, themes, reading history, the window
  src/lib.rs        every #[tauri::command], window restore, file associations
  src/settings.rs   settings.toml — one flat table, one key written at a time
  src/theme.rs      one TOML file per theme, built-ins installed on first run
  src/library.rs    library.toml — where you were in each document
  src/watch.rs      the themes directory and the open document, watched
  themes/*.toml     the seven packaged themes, embedded with include_str!

tests/              node --test; `npm test` starts a dev server for them
  search.test.mjs   text folding and where a match lands
  recolor.test.mjs  the two recolouring paths, in WebKit
  theme.test.mjs    what a theme's five colours turn into
  reader.test.mjs   the whole interface, through the harness
  password.test.mjs an encrypted document: asking, refusing, and giving up
  helpers.mjs       compiling a .ts module to reach what it does not export
  fixtures/         PDFs are generated, not committed

src/                TypeScript: the interface
  main.ts           the App object: state, menus, keyboard, wiring
  viewer.ts         layout, rendering, scrolling, links   ← the heart of it
  themes.ts         theme → CSS variables, and the page recolouring itself
  search.ts         the full-document index, the fold, and match stepping
  sidebar.ts        contents and thumbnails
  settings.ts       the settings window
  ui.ts             menus, switches, the modal window, the notice line
  api.ts            the only file that talks to Rust
  icons.ts          the hand-drawn icon set
  styles.css        all of it; textlayer.css is pdf.js's own selection layer
```

## What lives where

**Rust never renders anything.** It hands over bytes, remembers things, and
asks the system to open a link or show a file. It also decides when the window
appears: the frontend calls `ready` once it can paint, so a dark theme never
flashes white on the way in.

**A document is read in pieces, never whole.** `open_for_reading` opens the
file and reports its length without reading any of it; `read_range` serves
slices of whichever document is currently open, raw rather than base64'd
through the JSON bridge. `FileRange` in `viewer.ts` is a pdf.js
`PDFDataRangeTransport` over those two, with `disableAutoFetch` and
`disableStream` so nothing is fetched speculatively. Handing pdf.js the whole
file instead meant three copies of every document in memory — the Rust buffer,
the JS array, and the worker's own — and reading all of a scanned volume before
showing any of it.

**Every command that touches the disk is `async`, and that is the only reason
for the keyword.** None of them await anything. A synchronous Tauri command
runs on the thread that received the IPC message, which is the thread drawing
the window: `remember_position` fires on every pause in a scroll, so a
whole-file rewrite of `library.toml` was landing in the middle of the one
gesture this app exists to make smooth. The price of moving them off is that
two can now run at once, which is what the locks in `settings.rs` and
`library.rs` are for, and why `atomic_write` gives every write its own temp
file.

**`api.ts` is the only door.** Nothing else imports `@tauri-apps/api`. It also
carries a browser fallback — settings in `localStorage`, a file input instead of
the native picker, and the packaged themes read out of `src-tauri/themes/*.toml`
at build time — so `npm run dev` can be opened in an ordinary browser while
working on the interface. The themes are read rather than restated because the
restated copy went stale, and a stale copy of a theme is invisible: the file is
right, and what is on screen is the copy.

**Settings are written a group at a time.** Each write still changes only the
entries it names and leaves unknown keys alone; the defaults table in
`settings.rs` doubles as a whitelist. What changed is the batching: `App.set`
records the change in memory and queues it, and `flushSettings` sends whatever
has collected on the next turn of the event loop. Settings almost never move
alone — a theme comes with the light or dark slot it fills, a zoom with its fit
mode — and one call per key meant two whole-file rewrites per change, each
re-reading what the other had just done. `App.setSoon` queues the same way but
waits 400ms, for values that move continuously like zoom during a pinch.
Anything still queued is flushed on the way out, before the window goes.

**Themes are files.** Seven built-ins are written into the user's themes
directory on every run so they can be read and copied, and so a change to a
shipped theme reaches a machine that already has the old one; the embedded
copies are authoritative, and a built-in file edited in place is overwritten.
Editing a built-in through the app saves a copy under an id of its own, which
is never touched, and every shipped file carries a banner saying so — silently
reverting someone's edit is a trap however defensible the policy is. A theme
names colours and a `recolor` flag, and nothing else; `selection` is optional
and derived from the accent when it is absent, and `selection_text` — the ink
on that selection — is optional and derived from `selection` when it is
absent. `applyTheme` derives every shade
the chrome uses — surface, line, three grades of muted text, the positive green
— from those colours, which is why a five-line file is enough.

**Two of the files the app reads can change without the app changing them, and
Rust says when they do.** A theme is TOML so that somebody can open it in an
editor, and a document is often a paper being recompiled underneath the reader;
`watch.rs` follows the themes directory always and the open document while
there is one, and emits `themes-changed` (with the whole set — seven themes of
five colours is cheaper to send than to ask for) or `document-changed` (with
the path). The frontend reapplies the theme in use without remembering it, or
reopens the document and puts the reader back where they were. This is the
shape of work that belongs on the Rust side: it lives on the disk, and what
crosses the bridge is a filename.

## The viewer

`viewer.ts` earns its size. Six things are worth knowing before changing it.

*The side margin belongs to the modes that have something to frame.* `PAD_X`
is what sets a page off from the window when it is narrower than one, and fit
width is the mode whose whole point is that it is not — so fit width computes
against the full `clientWidth` and the content ends up exactly as wide as the
viewport. Charging it for the margin left forty pixels of ground either side
of a page that had supposedly reached both edges. `PAD_Y` is not conditional,
because there is always something above a page.

*Landing on a page means landing on the space above it.* `scrollTo` with an
offset of zero backs off by the distance from the bottom of the page before to
the top of this one — read off the boxes rather than taken from a constant,
because that distance is the gap between pages in the middle of a document and
`PAD_Y` at the start of it, and they are not the same number. Using one for
the other left a strip of the previous page showing above a page just turned
to.

*The first page is measured; the rest are estimated and then corrected.* Page
one's size stands in for every page, the layout is built from it, and the app
paints. `measureRest` then walks the document in batches and lays out again
whenever a real size differs. Most documents are one size throughout, so the
correction is usually a no-op — and measuring all of them first meant a blank
window until the last of two thousand page proxies came back. `boxes[]` holds
the position and scale of every page, in order, which is what lets
`firstBoxEndingAfter` and `lastBoxStartingAbove` binary-search it instead of
scanning the whole book on every scroll frame.

*Only the pages near the viewport exist in the DOM.* `mount()` keeps a window of
slots around the viewport (`OVERSCAN`), discards the rest, and queues the rest
for rendering nearest-to-the-middle first. A nine hundred page book costs about
what a two page letter costs.

*Page proxies are an LRU, and dropping one means `cleanup()`.* pdf.js holds a
page's parsed operator list — every decoded image on it — from the first render
until `cleanup()` is called. `pageCache` keeps `PAGE_CACHE` of them, never one
that is mounted, and cleans up what it evicts. Without that, a long illustrated
book grew for as long as it was read and gave none of it back. `discard()` does
the same for the canvas, by resizing it to nothing: dropping the reference
makes it collectable, not collected.

*A rendered page is identified by `keyFor()` — its scale, the screen's density,
and its theme.* If the key still matches, the canvas is reused; change any of
them and the page repaints. The density is in there because it is half of how
many pixels the canvas gets, and `watchDensity` re-arms a `matchMedia` query so
a window dragged between screens of different densities actually hears about
it. This is the whole invalidation story.

*The text layer is built once per mounted page and thereafter only rescaled.*
pdf.js positions its spans in percentages and sizes them from
`--total-scale-factor`, which `place()` sets on every layout, so `renderText`
calls `TextLayer.update()` rather than rebuilding. Rebuilding meant re-streaming
the page's text out of the worker on every zoom step — and throwing away
whatever the reader had selected.

*Recolouring is baked into the bitmap, not applied by CSS.* `recolor()` in
`themes.ts` flattens the canvas to luminance with composite operations and
stretches it between the theme's two colours, so scrolling afterwards costs
nothing. The ramp is straight but for its top: `WHITE_POINT` calls everything
above level 235 paper, because a hairline printed at 90% white is invisible on
paper and, carried across by the same fraction, arrives as a bright cage around
every hyperref box. The blend path reaches it as a `color-dodge` fill and the
pixel path as a clamp in the lookup table, which is why the fallback's luma is
rounded rather than truncated — the dodge multiplies any disagreement between
the two by 255/235. What it costs is the other thing that lives up there: a
code block shaded 4% grey now merges into the background rather than showing as
a slightly paler block. That is the trade the constant makes, and the constant
is the only place to change it. Two things are painted back on top of that result: pictures, if
"Recolour pictures too" is off (pdf.js reports where images landed via
`recordImages`), and links, which are redrawn from the untouched copy and
recoloured towards the link colour. Both need a pristine copy of the canvas,
taken before recolouring, and both put it back under a *single* clip covering
every rectangle at once — one clip and one `drawImage` per page, not one per
picture, which on a page of typeset mathematics is the difference between a
frame and a stall.

## What a reading session costs

Measured on the release bundle — `npm run tauri build`, then the `.app` run on
a document — on an Apple silicon Mac with 8GB, in a 1280×860 window at two
device pixels to the point, fit width, no recolouring. Scrolled a viewport at a
time with a quarter second between, which is a fast reader rather than a
benchmark. The figure is physical footprint, which is what Activity Monitor
calls Memory and what counts against the machine; `vmmap` agrees with
`footprint` to the megabyte, and `ps` does not, because a gigabyte of this ends
up compressed and RSS does not count compressed pages. All four processes a
Tauri app runs in are summed: the app itself, and WebKit's WebContent, GPU and
Networking helpers.

| what is open                          | on opening | after 60 viewports | where it settles |
| ------------------------------------- | ---------: | -----------------: | ---------------: |
| nothing — the start screen            |      81MB  |                  — |                — |
| 400 pages of plain text (the fixture)  |    293MB  |             365MB  |                  |
| 1048 pages of typeset mathematics      |    374MB  |             480MB  | 690MB after 200  |
| 197 pages of textbook with figures     |    420MB  |             510MB  |                  |
| 27 slides, 22MB of pictures            |    285MB  |     491MB after 40 |                  |
| 315 pages of scan, 33MB                |    375MB  |             797MB  | 900MB after 200  |

That table predates `isOffscreenCanvasSupported: false` — see "Images cross the
worker boundary compressed" below — which takes a large bite out of its last
three rows. It has not been re-run on the release bundle since; the numbers that
have are the harness ones further down, and the two instruments are comparable
only with themselves.

**The Rust side is 26–33MB and never moves**, whatever is open and however long
it is read. Every number above that is the webview, and two things set it.

*The canvases of the mounted pages.* A page at fit width in that window is
2480×3210 device pixels, which is 32MB of canvas, and `OVERSCAN` keeps two or
three alive at once. That is most of the 200–300MB a document costs the moment
it opens, and it scales with the window: the same document read full screen on
a larger display costs proportionally more.

*The pictures on the pages that have been read.* This is the one that used to
hurt: the last column for the scan read 3.2GB, on a machine with eight, and
climbed for as long as anybody kept reading.

**Where the memory is matters more than how much of it there is, and it is not
where it looks.** The web content process — the JavaScript heap, the text
layers, the canvases on screen — sat at 150MB throughout and never moved. The
growth was all in the *GPU* process, and `vmmap` on it mid-scroll names the
owner exactly: fifteen `ImageBufferShareableMapped` regions of 60.1MB each,
one more appearing per page read. A page proxy holds every decoded image on its
page until `cleanup()` is called on it, a bitonal scan decodes to four bytes a
pixel — 3600×4400 is sixty megabytes — and `PAGE_CACHE` kept forty-eight
proxies. 48 × 60MB is the plateau, and the plateau is where it stopped.

So `IMAGE_PAGE_CACHE` caps the pages that carry pictures at three, and
everything else keeps the count of forty-eight; a typeset book never reaches
the second cap and reads exactly as it did. `holdsPictures` decides which is
which, and it has to ask the page's own object store rather than `recordImages`, which reports
where image *XObjects* landed: a bitonal scan arrives as an image mask and is
not among them, so every page of the scan reported no pictures at all. That
mistake is worth remembering, because it is the same one behind "scanned
documents lose all their text" below — masks are where a scan keeps everything.

Where the second cap should sit was measured afterwards, on forty pages of
synthetic scan — 3600×4400 bitonal, the shape that hurt — read end to end
through `scripts/memory-probe.mjs`:

| `IMAGE_PAGE_CACHE` | plateau |
| -----------------: | ------: |
| 6                  | ~790MB  |
| 3                  | ~630MB  |
| 2                  | ~600MB  |

About 36MB a page, and it stops paying at three because `OVERSCAN` keeps three
pages mounted and a mounted page is never evicted: below three the cap has
nothing left to take. Every figure there is ±50MB between one report and the
next, which is itself worth knowing — the GPU process gives memory back in
bursts rather than steadily, so a single reading is not evidence of anything.

**Images cross the worker boundary compressed, not expanded.** The cap above is
a way of living with a cost; this is the cost going away, and it is one option
to `getDocument`. pdf.js in a browser defaults to expanding every image to RGBA
in the worker, painting it into an `OffscreenCanvas` at the image's own
resolution and transferring an `ImageBitmap` — which is what those 60.1MB
regions in the GPU process are, and why a picture that is one bit per pixel on
the disk costs four bytes a pixel to have read. `isOffscreenCanvasSupported:
false` sends the decoded data instead and lets the main thread build the mask
canvas per render, where it is freed when the render ends. Measured in the
harness, which is not the release bundle but is the same instrument on both
sides:

| document                          | default | data, not bitmaps |
| --------------------------------- | ------: | ----------------: |
| 400 pages of plain text           |  265MB  |            263MB  |
| 40 pages of bitonal scan          |  634MB  |     **263MB**, flat |
| 27 pages of photographs           |  348MB  |            231MB  |
| one page of 12000×16000 bitonal   | 2489MB  |            248MB  |

It is not a trade against speed. Timed straight through pdf.js with no app
around it, a scan page goes from 92ms to 71ms and a photograph page from 122ms
to 110ms, because what leaves the worker is the compressed thing rather than the
expanded one; a page of type is unchanged. What it is a trade against is *where*
the work happens — the expansion is on the main thread now, so an image-heavy
page costs one frame of about 60ms that used to be spent in the worker. Against
that, the pixels are identical: every screenshot compared came back at an RMSE
of zero, under both themes and with picture recolouring both on and off.

The one thing given up is `ImageResizer`, which runs only on the default path
and shrinks an image the browser could not otherwise make a canvas for. That
sounds like a safety net and the last row of the table is what it actually does:
the resizer is reached by first building the 192-megapixel bitmap that is the
problem.

Three things were tried on the way and are not here, which is worth as much as
what is:

- *A byte budget rather than two caps.* The right idea in the abstract and it
  has no honest input: the size of a decoded image is not on offer anywhere in
  pdf.js's public surface, the main thread's copy of an image object is `null`
  by the time anyone can look at it, and estimating from the area a picture
  covers on the page is wrong by twenty times for a photograph scaled into a
  column. Two caps need no estimate.
- *A pool of canvases*, to stop the churn of one created per render. Built,
  measured, reverted: it changed nothing, because the canvases were never the
  thing accumulating.
- *A smaller `MAX_CANVAS_PIXELS`.* Halving it halved the canvases and moved the
  total not at all, which is what first said the problem was not the drawing.

What did survive from the hunt is a leak by a different door: an abandoned
render — the reader scrolled on, the theme changed, the render was cancelled —
dropped its canvas rather than releasing it. `renderSlot` now releases on every
path that does not put the canvas on screen.

`scripts/memory-probe.mjs` is how any of this is checked. It reads the
footprint from outside the browser, because there is no memory API in WebKit
worth the name, and `--regions` prints the `vmmap` breakdown that says which
process is holding what. It is not part of `npm test`: it wants a real
document, it takes minutes, and half of what it measures is the machine.

For scale, the shipped bundle is 5.8MB on disk (4.2MB as a `.dmg`), of which
5.7MB is the one binary — the whole frontend, pdf.js's worker, cmaps, standard
fonts and wasm decoders included, is compressed inside it.

## Things that will bite

**pdf.js runtime data must be given absolute URLs.** `cMapUrl`,
`standardFontDataUrl`, `iccUrl` and `wasmUrl` are handed to the pdf.js *worker*,
where a relative address resolves against the worker script rather than the
page. When they are wrong the worker silently drops what it cannot fetch, and
the failure is oblique: scanned documents lose all their text, because that text
lives in image masks. `asset()` in `viewer.ts` exists for this.

**pdf.js's image defaults are tuned for a browser tab, not for a reader.**
`isOffscreenCanvasSupported` defaults to on and hands the main thread
`ImageBitmap`s, which is the right call when a page is opened once and thrown
away and the wrong one when a document is read for an hour: a bitmap is four
bytes a pixel in the GPU process for as long as its page proxy lives. It is off
here, and "Images cross the worker boundary compressed" above has the numbers.
Anything that reaches for a pdf.js default around images is worth measuring
rather than trusting.

**Do not tint the document with `mix-blend-mode`.** WebKit drops the blend
against a composited canvas, and a dropped blend renders as a solid band across
the line. Anything that has to change the colour of ink goes onto the canvas.

**Selected words are repainted from the page, never from the text layer.**
The obvious way to colour selected text is to give `::selection` a `color`, and
it puts pdf.js's text layer on screen — spans that exist to be selected rather
than seen, carrying no weight, no style and a generic family, each stretched
horizontally to the total width the printer used. A page's bold type comes back
as regular, its mathematics comes back as boxes, and every letter shifts as its
line is stretched to fit. So `paintSelection` in `viewer.ts` copies the pixels
under each selected line off the page canvas, runs the copy through the same
luminance ramp that recolours a page — ink to the selection's text colour,
paper to its own — and lays it back over the line. Real glyphs, real weight,
real symbols. Three things about it are load-bearing: which way round the ramp
goes depends on the *page*, not the theme, because a recoloured dark page is
already light ink on dark paper; the rectangles are rounded outwards and
adjacent runs on a line are joined, because pdf.js's spans do not abut and the
gaps otherwise show as white rules through a highlighted sentence; and a run is
kept between repaints when its place, its colours and its density are
unchanged, which is what keeps a drag down a page to about half a millisecond a
frame rather than redrawing all of it. `::selection` keeps a plain background
underneath for the frame before the copies land.

**Canvas blend modes are checked before they are trusted.** `recolor()` is
built on `saturation`, which is non-separable and not implemented on a canvas
by every engine — WebKitGTK on Linux is the one we have least visibility into,
and a dropped blend mode does not throw, it silently does nothing and the page
comes out as printed under a theme meant to recolour it. `canBlend()` probes
once (an unsupported value is refused and the property keeps what it had) and
`recolorByPixel` is the fallback. The two agree to within one level out of 255;
`recolor.test.mjs` is what says so.

**`putImageData` ignores the clipping path.** It is the one drawing operation
that does. That is why `recolor()` takes an optional list of rectangles as well
as being called inside a clip: the fast path is bounded by the clip, the pixel
fallback has to be told, and `tintLinks` passes both. Get this wrong and
colouring the links repaints the entire page.

**A window fits its content unless it asks not to.** `showWindow` sizes to
what is in it; Settings passes `"full"` for the fixed 860×600 frame it needs
for its nav column. The default used to be the fixed frame, which gave a
one-field password prompt a half-empty window five hundred pixels tall.

**One instance, on every platform.** `RunEvent::Opened` is Apple Events and
fires on macOS alone; everywhere else the system answers "open this PDF" by
launching the app again with the path in `argv`. `tauri-plugin-single-instance`
routes that into `hand_over` — without it, three double-clicked documents meant
three processes writing over each other's `settings.toml`, which no lock inside
one of them can help with. The variant does not merely go unused off Apple
platforms, it does not exist there, so matching on it has to be `#[cfg]`-gated
or Linux and Windows will not compile — which is invisible from a Mac and is
the whole of what CI caught.

**Declining a password is not an empty password.** pdf.js reads any string
handed to `onPassword` as another attempt, so answering `""` when the reader
presses "Not now" got the question asked straight back, with no way out of it
at all. Giving up means passing an `Error`, which rejects the load. The
rejection travels through the worker and comes out as something else entirely,
so `load` remembers the decline itself and throws `Cancelled` — which `open`
recognises and says nothing about, because there is nothing to report.

**Document links are not anchors.** They carry `role="link"` and no `href`. An
anchor that carries the address navigates on a middle click, which never
reaches the `click` handler — so the webview left the app, taking the open
document with it. Both `click` and `auxclick` go through `onExternalLink`,
which is the only thing allowed to decide what opening a link means.

**The top of the window is the app's, not the system's.** `titleBarStyle:
Overlay` runs the document up under the title bar, so on macOS there is no
native strip to drag the window by: `#toolbar` carries
`data-tauri-drag-region="deep"`, and that needs
`core:window:allow-start-dragging` in the capability, which `core:window:default`
does *not* include. With the toolbar hidden and the window not in full screen,
`.title-drag` stands in for it — inert until the pointer reaches the top eight
pixels, the same reach that brings the peek handle down — and
`set_titlebar_buttons` takes the three traffic lights away to match. All three
hang off `applyChrome()`, which is why `syncFullscreen` calls it.

**`core:window:default` grants almost nothing but getters.** Every window verb
the frontend uses has to be named in the capability one at a time, and the
failure is a line in the console rather than anything visible: without
`allow-destroy` the window will not close, because `onCloseRequested` in the JS
API destroys the window itself unless the handler prevents the default.

**A full-screen change costs the page its keyboard.** The webview stops being
the window's first responder, so every shortcut dies until something is
clicked, and `el.viewer.focus()` is not enough to get it back — only
`setFocus()` on the window is. `reclaimKeyboard()` does both, and it runs from
`syncFullscreen`, once the window has stopped moving, rather than the moment
the switch is thrown: AppKit passes focus around until the animation ends.

**A file is watched through its directory, and a change is a burst.** A
document is replaced by writing another one beside it and renaming it over the
top — which is what `atomic_write` does and what compilers do — and a watch on
a file follows the file rather than the name, so it would go on watching
something nobody can see. Hence the watch on the parent directory, filtered by
name. For the same reason nothing acts on a single event: a save is three or
four, an atomic write is a create and a rename, and a LaTeX run is hundreds
over several seconds, so events are collected until the disk has been quiet for
`SETTLE`. And because the app writes into the themes directory itself, a theme
reload is decided by loading the themes and comparing them against the set the
frontend already has, never by the fact that something moved. Lastly, `follow`
returns without touching anything when it is handed the document it is already
following, which is what every reload does on its way back through
`open_for_reading`: remaking the watch would lose whatever landed in the gap,
and retaking the baseline from the disk would swallow a draft that arrived
during the reload — the next burst would find the file matching its own mark
and say nothing.

**A document is not believed until it ends the way a PDF ends.** A compiler
writes its output across the whole of a run, and reopening what is on the disk
in the middle of one would take the reader's document away and leave them on
the start screen. `whole()` reads five bytes at the front and a kilobyte at the
back, wants `%PDF-` and `%%EOF`, and then wants the size to be the same a
moment later. It does not prove the document is readable; it rules out the case
that actually happens.

**The webview's own context menu is suppressed** everywhere except editable
fields and live text selections, because it offers to reload the app (which
closes the document) and to open the inspector.

**Escape and menus.** A popover registers its own capturing key handler, and so
does the modal window; the app-level shortcut handler bows out while either is
open. Clicking the button that opened a menu closes it — `showPopover` tracks
its anchor for exactly that.

**The find bar is not a popover and dismisses itself by hand.** It holds the
keyboard and a query, so it cannot live in `#popovers`; `App.FIND_KEEPS_OPEN`
is the list of places the pointer may go without putting it away — itself, the
top strip, and the two layers that only ever open from up there. Everything
below the toolbar closes it, which is what the menus beside it do.

**Two of the three search switches change what is found, and one does not.**
"Match case" is a parameter to `fold`, and "Whole words" a boundary test done
against the folded text — so a word hyphenated across a line, whose soft hyphen
the fold has already taken out, is one whole word by the time it is tested.
"Highlight all" changes only how much of the result is painted, and so lives on
the viewer. All three are settings and outlive both the bar and the session. A
page already extracted is refolded rather than re-extracted when the case
setting moves: the trip into the worker is the expensive half.

**Option is not a letter on a Mac.** ⌥⌘G — go to a page, which is what Preview
binds it to — has to be matched on `event.code`, because Option turns a G into
a © and `event.key` has nothing left to compare. ⌘G, which steps through
matches, is guarded with `!event.altKey` or it takes the keystroke first.

## Testing the interface without taking the screen

**`npm test` is the first thing to run and the first thing to add to.** It
starts a dev server if one is not already up, generates the four-hundred-page
fixture if it is not there, and runs everything in `tests/`. Two of the three
files compile a module in memory to reach what it does not export — see
`tests/helpers.mjs` — which is how the text folding and both recolouring paths
get tested without widening a module's public surface to suit its tests.

**Drive the frontend headlessly. Do not synthesise input into the real app
unless the change is genuinely native.** `scripts/ui-harness.mjs` opens the
interface in Playwright's WebKit and gives you keys, wheel gestures, clicks and
screenshots against it:

```js
import { openApp } from "./scripts/ui-harness.mjs";
const app = await openApp({ pdf: "x.pdf", settings: { scroll_mode: "paged" } });
await app.press("ArrowRight");
console.log(await app.state());     // page, zoom, scroll, find bar, menus
await app.close();
```

Needs `npm run dev` running first. **WebKit, not Chromium**, and the default for
a reason: the app lives in a WKWebView, and the engines disagree about exactly
the things this app leans on — blend modes on a composited canvas, pinch zoom,
text layout. `{ engine: "chromium" }` is there for comparing the two.

**Press `MOD`, never `Meta`.** The app takes its whole shortcut scheme from
`navigator.platform` — `isMac` in `api.ts` — so the modifier is ⌘ on a Mac and
Ctrl on the two platforms this is not developed on. A test that hard-codes
`Meta` passes here and does nothing at all on CI, and the damage is quiet: the
find bar simply never opens, and a toolbar hidden by one test is never brought
back for the next four. The harness exports `MOD` for this, and
`HYLOPDF_PLATFORM=other` runs the whole suite under the other scheme — it lies
to `navigator.platform` too, so the app and the test agree about which machine
they are on. `HYLOPDF_PLATFORM=other npm test` is the cheap way to find out
what Linux will say, and it is worth running before touching a shortcut.

**`HYLOPDF_NO_BLEND=1` reads the whole app down the pixel fallback.** It refuses
the non-separable blend modes the way an engine without them does — silently,
by keeping the property's previous value — so `canBlend()` says no and every
recolour goes through `recolorByPixel`. `recolor.test.mjs` tests that function
against the blend chain; this is the only way to test *reading* under it, which
is the shape Linux may actually be in. It is also a good deal slower, which
makes it the switch to reach for when a test is suspected of waiting on a fixed
number of milliseconds.

**Wait for the condition, not for the clock.** Everything this app does after a
keystroke — recolouring a canvas, remounting a neighbourhood of pages, indexing
four hundred of them, relaying out for the sidebar — takes as long as the
machine takes. Locally a theme edit repaints in about 90ms, 170ms down the pixel
fallback; on a CI runner it went past the 800ms a test had slept for. Three
consecutive CI runs failed in three different places on the same cause, each
one passing on the run that found the next.

So fixed waits in `reader.test.mjs` are for *ordering* — something that has to
happen before the next step can be taken — and anything waiting for a *result*
polls for the result. The polls all share a shape: `waitForFunction` with a
generous timeout, `.catch(() => {})`, and then the original assertion, so a
thing that never arrives is reported as the state it is stuck in rather than as
a timeout, and the message stays the one worth reading.

Two things learned doing it. Waiting for work to *finish* is not the same as
waiting for the answer: between a query changing and the search starting, the
find bar still holds the last count, so a wait for "not scanning any more"
returns on the previous answer — wait for what the step expects instead. And a
test that has no wait of its own may be living on the one before it: "pages are
drawn" only ever passed because the test above it slept 1.5 seconds, and it
started failing the moment that sleep became a condition that is met sooner.

The suite got faster: 36s of sleeping down to about 11s of waiting.

Settings are seeded through the `localStorage` fallback in `api.ts`, so the
whole browser path is exercised: no Rust, no window, no traffic lights. Reading
a document goes through the same range-based path as the real app — the
fallback slices a `File` where Rust seeks a handle — so the transport is
exercised here too.

Anything that is really about the *window* — dragging it, full screen, the
title-bar buttons, the peek handle clearing the system bars — has to be checked
in the real app, and there is no way to do that quietly. **Nothing here has
ever run on WebKitGTK**; CI builds on Linux, which catches a build break but
not a rendering one. The recolouring is the part most likely to differ, which
is why it has a fallback and a test rather than a note saying it should be
fine.

**The real app can only be driven from the foreground.** Synthetic keys and
clicks go to whichever process is frontmost, so testing takes the machine away
from whoever is using it. `CGEventPostToPid` looks like a way out and is not:
posting to the app's pid works for keystrokes only while its window is still
key — a few seconds after it was last in front — and never for clicks or scroll
(tested: two clicks and a scroll burst, nothing moved). Window-targeted
screenshots *do* work in the background, `screencapture -l <windowid>`, and are
cleaner than a full-screen grab. Take the window id from
`CGWindowListCopyWindowInfo` filtered by pid; a window on another Space or in
full screen cannot be captured at all.

So: say plainly when you are about to drive the real app, and say when you have
stopped.

## The renderer is the replaceable part, and it was tried

pdf.js is the largest thing in the app by some distance: 1.2MB of worker, plus
cmaps, standard fonts and wasm decoders, against a Rust binary that is a
rounding error. It is also where the remaining costs are — a canvas per page in
JavaScript, recolouring done with composite operations because that is what a
canvas offers, and no encryption support beyond a password prompt.

The alternative was **`pdfium-render`**: rasterise pages in Rust and hand the
frontend a bitmap. That is no longer a thought experiment. It was built, run in
the real app, measured against the renderer it would replace, and parked on the
**`pdfium-prototype`** branch — a Rust side in `src-tauri/src/render.rs` and two
viewers over the same layout, `proto/pdfium.ts` through pdfium and
`proto/pdfjs.ts` through the app's own `Viewer`, so the only difference between
a run of one and a run of the other is which renderer answered. The branch
carries its own instructions. Nothing of it is on `main`, because the answer
came back no.

**pdfium draws faster than pdf.js, by between a third and eight times.** Per
page, at the 10.5 megapixels the app actually renders (fit width, retina, the
12M ceiling just above):

| document                | pdf.js | pdfium |
| ----------------------- | -----: | -----: |
| 400 pages of plain text | 27ms   | 3.6ms  |
| a typeset paper         | 33ms   | 21ms   |
| a paper full of figures | 82ms   | 36ms   |
| a scanned manual        | 80ms   | 10ms   |
| slides                  | 66ms   | 52ms   |

Text extraction is three times quicker, opening a document is two orders
quicker (pdf.js is mostly starting its worker), and recolouring in Rust is
13-17ms a page scalar, 2.8-3.5ms across the cores — against 1.5-5.6ms for the
canvas blend chain, which is the one thing pdf.js's side does better by
default.

**And none of that survives the trip into the webview.** A page at that size is
43MB of bitmap, and it has to cross a process boundary that pdf.js never
crosses, because pdf.js draws into a canvas that already lives in the web
content process. Measured end to end, in the app, on the same document and the
same scroll:

| what                          | pdf.js    | pdfium via `invoke` | pdfium via `page://` image |
| ----------------------------- | --------: | ------------------: | -------------------------: |
| per page, end to end          | —         | 47ms                | 82ms                       |
| app process                   | 89MB      | 172MB               | 124MB                      |
| web content process           | 92MB      | 158MB               | 573MB                      |
| **total, after 60 viewports** | **182MB** | **330MB**           | **697MB**                  |

The 3.6ms render becomes 47ms by the time the pixels are in a canvas, and the
memory roughly doubles: the bitmap exists in the Rust buffer, again as an
`ArrayBuffer` on the JavaScript heap, and again inside the canvas. Serving the
same bitmap as an image over a custom URI scheme — which sounds better, and
lets pdfium draw straight into the response with no copy at all — is worse
still, because WebKit's image cache holds every page it is given and the decode
costs more than the copy did. Both numbers are after fixing the obvious faults
(rendering off the main thread, `no-store`, releasing bitmaps on unmount); what
is left is the transport itself.

**So: no, and the three ways out were costed one at a time.** The costs that
would have justified the swap are not where they were assumed to be. What
pdfium is plainly better at is *drawing*, and drawing is not the bottleneck —
the frontier is the bridge, and every version of the swap still has to get 43MB
a page across it.

*Draw at the window's scale and refine.* The bytes crossing are a quarter of
what they are now and the time goes very nearly with them: the transport is
linear in bytes, and the two parts of it that can be timed from inside a web
content process say the same — a 42MB page costs 5ms to `putImageData` and 11ms
to copy once, the same page at a quarter of the size costs 1ms and 1ms. (That
pair is measured in headless WebKit rather than in the app, but it is the same
work, and it accounts for 16 of the 43ms; the rest is the crossing itself.) So a first
pass would land in about 12ms against pdf.js's 27ms on plain text and 80ms on a
scan. What it does not do is finish. A page the reader stops on still has to be
redrawn at the device's scale, so the settled cost is that 12ms *and* the full
47ms, every page that is actually read crosses the bridge one and a quarter
times over, and the memory — the other half of the objection — is unchanged for
exactly the pages that hold it. The reader also sees the seam: a page that
arrives soft and sharpens a moment later is the opposite of what "no animations
unless the user takes an action" is asking for. It buys latency during a fast
scroll through documents pdf.js is already slow on, and charges for it
everywhere else.

*Keep the bitmap on the native side.* This is the only one that wins outright,
because it does not cross the bridge at all — and it is no longer a change of
renderer. A layer under or over the webview owns the pixels, so the text layer,
the selection, find-in-page, the links, the outline and the thumbnails either
follow it into native drawing or stop lining up with the page they are drawn
over. `paintSelection`, `tintLinks`, `restoreImages` and every line of
`renderText` are "the pixels and the DOM are in one process" code, and that is
most of what makes this app pleasant to read in. It is a different application,
and a much larger one than the thing it would be replacing.

*Wait for shared memory.* This one is not a wait. Tauri's IPC is message
passing by deliberate choice, taken over shared memory for the isolation it
buys, and shared memory is not planned; the `SharedArrayBuffer` route that gets
raised whenever this comes up shares between frontend contexts and not between
the frontend and Rust, which the maintainers said in as many words in 2024.
There is nothing coming that this plan could arrive on.

Two more were weighed and are worse. *pdfium as wasm, inside the webview*
attacks the right thing — from wasm memory to a canvas is a copy, not a process
boundary — but the prebuilt that does not fall over on a document longer than a
few pages is the 13MB one, against the 5.5MB of pdf.js it would replace; it is
single-threaded, so the drawing advantage narrows to whatever wasm leaves of
it; and it is a C++ blob either way, so "more Rust" is not what it buys.
*pdfium for the small answers only* — text extraction, the outline, opening an
encrypted document, thumbnails, everywhere the answer is kilobytes rather than
megabytes — is the one shape the bridge does not spoil, and it fails on the
other axis: it ships both renderers to keep pdf.js drawing the pages anyway.

**What it would cost on disk, measured.** `libpdfium.dylib` for macOS arm64 is
7.7MB (3.5MB compressed), the bindings add 0.4MB to the Rust binary, and what
pdf.js would stop shipping is 5.5MB — the 1.2MB worker, 1.6MB of cmaps, 1.5MB
of wasm decoders, 0.8MB of standard fonts and about 0.35MB of the bundle. Call
it +2.6MB on one architecture, and rather more on a universal build, where
pdfium is 15MB. "Roughly a wash" was the guess and it is close to right,
slightly the wrong side of it.

**The decision is to keep pdf.js.** The 47ms measured above is 3.6ms of drawing
and about 43ms of bridge, and the bridge does not care what is on the page — so
adding each document's draw time to it says where the swap would actually land:

| document                | pdf.js | pdfium, end to end |
| ----------------------- | -----: | -----------------: |
| 400 pages of plain text | 27ms   | 47ms (measured)    |
| a typeset paper         | 33ms   | ~64ms              |
| a paper full of figures | 82ms   | ~79ms              |
| a scanned manual        | 80ms   | ~53ms              |
| slides                  | 66ms   | ~95ms              |

Slower on three of the five, and ahead only where pdf.js is spending its time
decoding images. Against what the brief asks for — fast with no lag, little
memory, a small binary — that is 1.8 times the memory and 2.6MB more on the
smallest build, for no reliable gain in speed. Encrypted documents and forms
would come with it and those are real; they are not worth this. Two things
would reopen it: the second option above, taken deliberately as a rewrite of
the viewer rather than as a swap of renderer, or a webview that lets a native
buffer become a canvas without copying it. Neither is close.

**Three things that will bite whoever picks this up again.** pdfium renders
into a buffer of your choosing (`PdfBitmap::from_bytes`), which is how the
`page://` path gets away without a copy — but Skia CHECKs that the buffer is
four-byte aligned, and a failed CHECK is a trap instruction: the process
vanishes with no panic, no message and no stack. The BMP wrapper has to be a
plain `BITMAPINFOHEADER`, because ImageIO, which is what decodes an image
inside a WKWebView, rejects a `BITMAPV4HEADER` outright and reports it as
`EncodingError: Loading error`. And `register_asynchronous_uri_scheme_protocol`
hands you the request on the thread that draws the window: answer it there and
the scroll stops for as long as the page takes.

`mupdf-rs` is faster and smaller but AGPL, which is a licensing decision rather
than a technical one. Nothing measured here bears on it.

**This was a change to the renderer, not to the framework.** Tauri is the right
shell for a UI like this one, and nothing above suggests otherwise. What made
the renderer swappable is that `viewer.ts` is the only file that imports pdf.js
for rendering (`search.ts` and `sidebar.ts` use it only through a
`PDFDocumentProxy` they are handed) and `api.ts` is the only door into Rust.
Keep both of those true and this stays a decision that can be made later — the
prototype needed no change to either.

## Where Rust ends and TypeScript begins

The renderer question answers a more general one, and the rule it leaves is
worth stating plainly: **work belongs in Rust when its inputs and outputs are
small next to the work itself, and in TypeScript when it touches pixels or the
DOM.** Pixels and the DOM live in the web content process, and nothing reaches
them without crossing a boundary that costs about a millisecond a megabyte.
Everything in `api.ts` already obeys that rule — settings, themes, the library,
the file picker, ranges of a file, the traffic lights — which is why the door
is as narrow as it is.

There is a second constraint and it catches most proposals before the first
one does: **every door in `api.ts` has a browser twin.** The harness runs the
whole interface with no Rust behind it, which is how reading, search,
recolouring and the password window get tested without taking anybody's screen.
Move something into Rust and you either write it twice or lose the test — and
pure computation, which is what looks most tempting to move, is exactly what
the tests reach for.

So, the candidates, and what happens to them:

*The search index, the fold and the match stepping.* The shape of the traffic
is right — a query in, a few offsets out — and the text would only have to
cross once, a megabyte or two a book. It still fails. `fold` is the most
tested function in the app, the browser path needs a matcher of its own
regardless, and the JavaScript is not the weak version: NFKD a character at a
time with an origin map back to the unfolded text is what the Rust would also
have to do. Nothing is bought.

*Recolouring.* Already measured on the branch, and the canvas wins before the
pixels even move: 1.5-5.6ms for the blend chain against 13-17ms scalar in Rust,
2.8-3.5ms across all the cores. Then the page would have to come back.

*The shades `applyTheme` derives.* Five colours in and fifteen out is a perfect
shape, and it is also forty lines of arithmetic that would need a twin. The
door costs more than the work.

*Layout, the LRU, the binary searches over `boxes[]`.* Microseconds, on numbers
that are already in the page.

What passes the rule is not moved work but new work, and all of it is about the
disk:

*Watching the themes directory.* Themes are files, and the brief wants them
hand-written and LLM-written; today an edit is seen on the next run. `notify`
in Rust, a path out over the bridge, `applyTheme` again — the payload is a
filename and the effect is that a theme can be written with the app open beside
it. The clearest win on this list, and the same watcher answers the document
recompiled by LaTeX under the reader's feet.

*Keybindings as a file.* Parsed and validated in Rust the way themes are,
dispatched in TypeScript. Config in, no traffic afterwards.

*Whatever comes next that has to be remembered* — annotations, bookmarks, a
thumbnail cache, per-document settings, printing, export. These belong beside
`library.rs` for the same reason it does, and each is a new door rather than a
moved one.

More Rust is not itself the goal; the brief asks for small, fast and calm, and
the seam that serves those is the one the app already has — Rust owns what is
on the disk and what the window does, TypeScript owns what is on the screen.
The renderer measurement is the strongest evidence for it: the largest piece of
work that could move to Rust is the one that most clearly should not.

## Running it

```
npm run tauri dev              # the app, with vite behind it
npm run tauri dev -- -- FILE   # …opened on a document
npm run dev                    # the interface alone, in a browser
npm run check                  # tsc --noEmit
npm test                       # node --test, with a dev server started for it
npm run tauri build            # .app and .dmg
```

`.github/workflows/ci.yml` runs the types, the tests and a build on every push,
and bundles the app on all three platforms — which is the only way the engines
this is not developed on get exercised at all. Signing is the one thing it
cannot do without secrets, and naming the variables the Tauri bundler reads is
not the way to leave the door open for them: the bundler goes by whether
`APPLE_CERTIFICATE` is *present*, and a secret the repository does not have
arrives as an empty string rather than as nothing at all, so the macOS job
compiled and then died at `security import` on every push. They come in under
other names now, and a macOS-only step promotes the ones that carry something.
An unsigned macOS build is quarantined anywhere but the machine that made it.

`scripts/sync-pdfjs.mjs` copies pdf.js's cmaps, standard fonts, ICC profiles and
wasm decoders into `public/pdfjs` before every dev run and build. Nothing is
fetched at runtime, and the app works offline.
