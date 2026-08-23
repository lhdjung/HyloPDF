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

## Chunk B
3. I don't think the Pzazz theme really captures the Charm CLI aesthetic (and edited to try capture it better, but feel free to override). Charm uses colors that are roughly:
  - Pink: R: 252, G: 113, B: 255
  - Purple: R: 94, G: 69, B: 247
  - Green: R: 27, G: 253, B: 184
  I tried using the according hex colors in src-tauri/themes/pzazz.toml but green as accent color doesn't work – the accent color is always dark purple. However, the green does properly show in the "Open a document" button on the landing page.
4. Add sepia and high contrast themes (one each); see above. Make good decisions on colors not mentioned there.
5. "A theme is two colours: the ink and the paper. Everything else follows from them." – is that still correct given the other colors?
6. The toolbar should have the same colors (text and background) as the document.
7. Deseleccting "Show toolbar" should close the "Settings" dropdown.

## Chunk C
1. Below talks about switching the viewer to pdfium-render and lists some options for doing this in a performant way. Would the switch be good? (It should then be on the pdfium-prototype branch.) If so, which option or options?
2. Any other ways to use more Rust in the app, especially relative to JS/TS?

## Chunk D
CI hit issues before chunks were tackled:
    Checking tauri-plugin-single-instance v2.4.3
error[E0599]: no variant named `Opened` found for enum `tauri::RunEvent`
   --> src/lib.rs:667:37
    |
667 |             if let tauri::RunEvent::Opened { urls } = event {
    |                                     ^^^^^^ variant not found in `tauri::RunEvent`

For more information about this error, try `rustc --explain E0599`.
error: could not compile `hylopdf` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `hylopdf` (lib test) due to 1 previous error
Error: Process completed with exit code 101.


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
  themes/*.toml     the five packaged themes, embedded with include_str!

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
the native picker — so `npm run dev` can be opened in an ordinary browser while
working on the interface.

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

**Themes are files.** Five built-ins are written into the user's themes
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

## Things that will bite

**pdf.js runtime data must be given absolute URLs.** `cMapUrl`,
`standardFontDataUrl`, `iccUrl` and `wasmUrl` are handed to the pdf.js *worker*,
where a relative address resolves against the worker script rather than the
page. When they are wrong the worker silently drops what it cannot fetch, and
the failure is oblique: scanned documents lose all their text, because that text
lives in image masks. `asset()` in `viewer.ts` exists for this.

**Do not tint the document with `mix-blend-mode`.** WebKit drops the blend
against a composited canvas, and a dropped blend renders as a solid band across
the line. Anything that has to change the colour of ink goes onto the canvas.

**A `::selection` background is not the colour it was given.** WebKit
composites one at 80%, whatever the stylesheet says, so a fifth of whatever is
underneath survives it. Under a page that matters: the text layer is
transparent ink, so colouring selected words means giving `::selection` a
`color` — and the printed words that show through the wash sit half a letter
away from the ones being painted, because the text layer scales a whole span to
the width the printer gave it and the two sets of glyphs drift apart across a
line. The survivor reads as a shadow. A custom highlight has no such rule and
paints exactly what it is given, so `onSelectionChange` in `viewer.ts` says the
selection again as `::highlight(document-selection)` and `styles.css` spends the
theme's two selection colours there. Nothing about what is selected changes;
`::selection` keeps the same pair, and an engine without the Highlight API
simply gets the wash and the shadow, which is what every engine had before.

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
one of them can help with.

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
came back "not yet".

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

**So: not yet, and here is what would change it.** The costs that would have
justified the swap are not where they were assumed to be. What pdfium is
plainly better at is *drawing*, and drawing is not the bottleneck — the frontier
is the bridge. The versions of this that could win:

- Draw at the window's scale rather than the device's on a first pass and
  refine, so the bytes crossing are a quarter of what they are now.
- Keep the bitmap on the native side entirely — a layer under or over the
  webview, which is how Chrome's own PDF viewer does it. That is a much larger
  change than swapping a renderer, and it takes the text layer, selection and
  find-in-page with it.
- Wait for something that shares memory across the boundary. Nothing in Tauri
  exposes an `IOSurface` today.

**What it would cost on disk, measured.** `libpdfium.dylib` for macOS arm64 is
7.7MB (3.5MB compressed), the bindings add 0.4MB to the Rust binary, and what
pdf.js would stop shipping is 5.5MB — the 1.2MB worker, 1.6MB of cmaps, 1.5MB
of wasm decoders, 0.8MB of standard fonts and about 0.35MB of the bundle. Call
it +2.6MB on one architecture, and rather more on a universal build, where
pdfium is 15MB. "Roughly a wash" was the guess and it is close to right,
slightly the wrong side of it.

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
cannot do without secrets; the workflow names the variables the Tauri bundler
reads, and an unsigned macOS build is quarantined anywhere but the machine that
made it.

`scripts/sync-pdfjs.mjs` copies pdf.js's cmaps, standard fonts, ICC profiles and
wasm decoders into `public/pdfjs` before every dev run and build. Nothing is
fetched at runtime, and the app works offline.
