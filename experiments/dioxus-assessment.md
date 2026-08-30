# Dioxus Native (Blitz) — an assessment, and a plan

The question is whether HyloPDF should be rewritten on **Dioxus Native**: Rust
all the way down, HTML and CSS rendered by **Blitz** onto the GPU through
**Vello**, with no webview anywhere. Not Dioxus Desktop, which is a webview and
would buy nothing over Tauri.

Everything below was checked against the crates and status pages as they stand
in August 2026, not from memory. Where a number is an estimate rather than a
measurement it says so.

**Phase 0 and Phase 1 have since been built and run** — `dioxus-spike/` and
`dioxus-reader/`, written up in `FINDINGS.md` and `PHASE1.md` beside this file.
Phase 1's gate came back *mixed*: a third less memory than Tauri (238MB against
346MB on the same 400-page document), four times faster per page, three times
the binary, and a fixed ~110MB floor that belongs to an empty Blitz window
rather than to anything this app does. **That floor has since been taken apart
— `FLOOR.md` — and none of it belonged to Blitz.** 173MB was a
scene-independent constant in `vello`'s buffer sizing, which `vello_hybrid`
does not have, and 96MB was Phase 1's own reader copying every page three times
on its way to the GPU. Measured as physical footprint rather than resident size,
which is the only unit that sees GPU memory, the same reading session is
**144MB against Tauri's 373MB** — so the gate below is met, not missed. Read
`FLOOR.md` first, then `PHASE1.md`. All four gates pass. Where this document and
that one disagree, that one was measured and this one was reasoned, and the
paragraphs below have been corrected where it mattered. The largest correction
is that the custom paint API this was planned against no longer exists: Blitz
replaced it with `blitz_dom::Widget`, and a page drawn through a widget costs
no frames when it is not moving.

---

## The answer first

**Yes, it is worth building — as an experiment on a branch, in the order given
in "The plan" below, with a kill switch after Phase 1.** Three things make it a
real proposition rather than a fantasy:

1. **The bridge that killed the pdfium prototype does not exist here.** That
   experiment failed because 43MB of bitmap per page had to cross an IPC
   boundary into a web content process. In Blitz, the renderer, the page
   bitmaps and the DOM are all in one process and the bitmap goes to the GPU
   as a `wgpu::Texture` — no copy, no boundary. The `pdfium-prototype` branch's
   own numbers say pdfium draws a page in 3.6-36ms; the 43ms of bridge on top
   of it simply goes away.
2. **Blitz is HTML and CSS.** That is the only reason this is a rewrite of the
   runtime and not a rewrite of the product. `styles.css` is 2,129 lines that
   encode the whole look the brief asks for, and Blitz is the only non-webview
   Rust UI stack that can read it. egui, iced and Slint would each mean
   redrawing the app from nothing in a foreign idiom.
3. **Roughly 2,450 lines of the Rust side port unchanged.** `settings.rs`,
   `theme.rs`, `library.rs`, `keys.rs` and `watch.rs` know nothing about Tauri
   beyond a `#[tauri::command]` attribute and an emit call.

And three things that should keep expectations honest:

1. **Blitz is alpha turning beta.** `dioxus-native 0.8.0-alpha.1`, on a `main`
   at `0.3.0-beta.2`; the published `0.7.10` predates the Custom Widget API and
   is not the version to build against. Its own status page scores 48% on the
   WPT `css` subsuite. The 0.3 Beta milestone is dated August 2026 and
   production readiness is "sometime in 2026" by the project's own account.
   The API moves: between `0.7.10` and `main`, custom painting, the window
   lifecycle and the event queue all changed shape. That is a cost of arriving
   early, and it is paid in the shell and nowhere else.
2. **The binary gets two to four times bigger**, not smaller. 6.2MB today;
   12-20MB is the realistic band. The brief permits this and should be held to
   it explicitly.
3. **The entire test apparatus goes.** Seventeen test files, the Playwright
   WebKit harness, and the browser twin in `api.ts` that makes them possible.
   There is a good replacement (see "Testing"), but it is a rewrite, and
   pretending otherwise is how a rewrite silently loses its safety net.

---

## What Dioxus Native actually is

```
your #[component] fns            Dioxus VirtualDom
        │  mutations
        ▼
DioxusDocument  ──wraps──▶  blitz-dom  (Stylo for CSS, Taffy for layout,
        │                              Parley for text shaping)
        ▼
   blitz-paint  ──▶  anyrender  ──▶  Vello / Vello Hybrid / Vello CPU
        │                                    │
   blitz-shell (winit, AccessKit, muda)      ▼
                                        wgpu → Metal / DX12 / Vulkan
```

**Stylo** is Servo's style engine, so CSS parsing and cascade are the real
thing. **Taffy** does flexbox and grid. **Parley** shapes and lays out text
with real kerning and subpixel positioning. **Vello** rasterises on the GPU
with compute shaders; `vello_hybrid` splits the work CPU/GPU for hardware
without them, and `vello_cpu` is a pure software fallback. `dioxus-native`
already depends on both `anyrender_vello` and `anyrender_vello_cpu`, which is
the answer to "what happens on a Linux VM with no GPU".

Licences: `dioxus-native` and `blitz` are MIT OR Apache-2.0; `stylo_taffy`
brings MPL-2.0, which this tree already tolerates (see "The licence" in
AGENTS.md — the MPL-2.0 crates under Tauri are copyleft per file and
unmodified, and Stylo is the same shape).

Feature flags that matter to us: `accessibility` (AccessKit), `menu` (muda —
the same menu crate Tauri already uses, so the macOS menu bar survives),
`hot-reload`.

---

## Why this is a different question from the pdfium prototype

`AGENTS.md` records the pdfium experiment in detail and its conclusion was
**no**, on one axis: the transport. Reproduced here because it is the whole
argument for looking again.

| what | pdf.js | pdfium via `invoke` | pdfium via `page://` |
| --- | ---: | ---: | ---: |
| per page, end to end | — | 47ms | 82ms |
| total after 60 viewports | 182MB | 330MB | 697MB |

The 47ms was 3.6ms of drawing and ~43ms of getting the pixels into a canvas in
another process. The three ways out were costed and all three failed: draw
small and refine (pays twice for every page actually read), keep the bitmap on
the native side (a different application), or wait for shared memory (not
coming — Tauri's IPC is message-passing by design).

**"Keep the bitmap on the native side" is exactly what Dioxus Native is**, and
the objection recorded against it was that the text layer, selection,
find-in-page, links, outline and thumbnails would all have to follow the pixels
into native drawing and stop being DOM. In Blitz they *are* DOM — Blitz's DOM
just happens to be composited in the same process as the pixels. The objection
was to a native *layer under a webview*. It does not apply to a native *engine
under the whole app*.

So the trade the prototype could not make is available here:

| | today (Tauri + pdf.js) | Dioxus Native + pdfium |
| --- | --- | --- |
| page draw | 27-82ms in JS | 3.6-36ms in Rust |
| pixels → screen | canvas in the same process (free) | `wgpu::Texture`, same process (free) |
| copies of a page in memory | worker + JS heap + canvas | one CPU buffer, then one GPU texture |
| recolouring | canvas blend chain, 1.5-5.6ms, or a pixel walk at 34-70ms | fragment shader, effectively free |
| binary | 6.2MB | est. 12-20MB |

The recolouring line is the sleeper. `themes.ts` is 835 lines of luma ramps,
`colouredRows()` sampling, blend-mode probing, a pixel fallback and a mask
allocator — all of it there because a `<canvas>` gives you composite operations
and nothing else. On the GPU it is one fragment shader over the page texture,
run at composite time, with no CPU readback and no cache invalidation on theme
change at all. `keyFor()` loses its theme component.

---

## The port map

What the app is made of, and where each piece lands.

### Rust that survives nearly unchanged (~2,450 lines)

| file | lines | change needed |
| --- | ---: | --- |
| `settings.rs` | 502 | drop `#[tauri::command]`; it is already async + locked + atomic |
| `theme.rs` | 463 | same; `build.rs` and the fourteen `themes/*.toml` are untouched |
| `library.rs` | 601 | same |
| `keys.rs` | 215 | same |
| `watch.rs` | 668 | `emit_to(window, …)` becomes a `EventLoopProxy::send_event` |

The `atomic_write`, the per-file locks, `whole()`, the settle window, the
`Exiting` flag, `library.open` as a list — all of it is about the disk and none
of it is about Tauri. This is the part of the app the seam rule protected, and
it pays off here exactly as advertised.

### Rust that is rewritten but keeps its logic (`lib.rs`, 2,431 lines)

33 commands become plain function calls or Dioxus signals — no IPC, no
serialisation, no `async` needed for the sake of the main thread (though the
disk writes should stay off it via `tokio`). The window management
(`hand_over`, `Placements`, `OpenFiles`, `OpenDocuments`, `spawn_window`,
`tidy_after`, the Dock menu) is rewritten against winit and a shell of our own
over `BlitzApplication` — see "Multi-window" below.

Doors that need a new crate rather than a Tauri plugin:

| what | today | native |
| --- | --- | --- |
| file picker | `tauri-plugin-dialog` | `rfd` |
| open a link / reveal a file | Tauri shell | `webbrowser` (already a `dioxus-native` dep) + `opener` |
| single instance | `tauri-plugin-single-instance` | `single-instance`, or a Unix socket / named pipe; `RunEvent::Opened` becomes a macOS `NSApplicationDelegate` |
| clipboard | webview | `arboard` |
| title-bar buttons | `objc2` (already direct) | unchanged |
| Dock menu | `objc2` + `ClassBuilder` (already direct) | unchanged |
| print | `print_document` shells out | unchanged |

### TypeScript that is reinvented (~11,900 lines + 2,129 CSS)

| file | lines | fate |
| --- | ---: | --- |
| `viewer.ts` | 3,674 | the layout math, `boxes[]`, `rows()`, the LRU, the binary searches, `measureCrop`, `keyFor` — all ports to Rust nearly line for line. The pdf.js calls become pdfium calls. `paintSelection`, `tintLinks`, `restoreImages` and most of `recolor` **delete**, replaced by a shader and real glyph rects. |
| `main.ts` | 3,539 | becomes Dioxus components + signals. The `App` object is a `Store`. |
| `api.ts` | 898 | **deletes.** There is no bridge to be the only door to. |
| `themes.ts` | 835 | `applyTheme`'s derived shades stay (as Rust); the recolouring half becomes WGSL. `parseColor`/`readColor` stay and stay strict. |
| `ui.ts` | 813 | menus, switches, modal, notice line → components. The stepper's two load-bearing behaviours (no unit in the field, arriving selects) must be re-earned. |
| `settings.ts` | 801 | the settings window → a second Dioxus window |
| `sidebar.ts` | 699 | contents / marks / thumbnails / results → components; the thumbnail LRU stays |
| `keys.ts` | 677 | the action table and `chordsOf` port to Rust against `keyboard-types` |
| `search.ts` | 540 | `fold` ports to Rust (`unicode-normalization`); this is a straight translation and the most heavily tested function in the app |
| `icons.ts` | 86 | see "SVG" below — the one file that needs a genuinely different design |
| `styles.css` | 2,129 | mostly survives; see the gap list |

---

## The renderer under it

Three candidates, and this is the decision that most shapes the branch.

### pdfium (`pdfium-render`) — the recommendation for the experiment

Already prototyped in this tree (`src-tauri/src/render.rs`, 468 lines on
`pdfium-prototype`), already measured, BSD-3. It brings everything pdf.js
gives us and some things it does not:

- rendering, at the speeds in the table above
- **per-character text boxes** (`PdfPageText`), which is what search, selection
  and markup quads all need — pdf.js's text layer exists only because the DOM
  needed something selectable, and pdfium hands us the geometry directly
- outline, destinations, links, page labels
- annotations, read **and written** — including deletion
- encrypted documents, opened with a password rather than refused
- forms

Cost: `libpdfium.dylib` is 7.7MB on macOS arm64, 15MB universal, plus 0.4MB of
bindings. Against 5.5MB of pdf.js runtime data that stops shipping, this is
roughly +2.6MB on a single architecture — which is the number already measured
on the prototype branch.

### hayro — the pure-Rust option, and the one to watch

`hayro` 0.7 (Apache-2.0, MSRV 1.92) is the most feature-complete pure-Rust PDF
renderer, adopted by Typst, and passes a 1,400-PDF suite scraped from PDFBOX
and pdf.js. It would make the app one static Rust binary with no C++ blob.

It is not ready to be the only renderer here, and the docs say so plainly:
performance "has not been a focus at all so far", there is **no text extraction
support**, no annotation handling and no encryption in the documented API. Text
extraction is the blocker: `hayro-interpret` emits glyph draw commands into a
`Device` you implement, so a text layer is buildable, but it is a project of its
own and it is the single most correctness-sensitive part of the app.

Apache-2.0-only is worth a note but not an objection: pdf.js is Apache-2.0
today, so the shipped binary already carries those terms while the project's own
code stays MIT OR Apache-2.0.

**Design for this.** Put the renderer behind one trait — `render_page`,
`text_runs`, `outline`, `links`, `labels`, `annotations` — the way `viewer.ts`
is today the only file importing pdf.js. That is the seam rule applied to the
new tree, and it is what makes "swap to hayro in 2027" a decision rather than a
rewrite.

### mupdf — still no

Faster and smaller than both. AGPL. Unchanged from the note already in
AGENTS.md: a licensing decision, not a technical one.

---

## What Blitz cannot do that this app currently does

This is the substance of the assessment. Every row was checked against
`blitz.is/status`, not assumed.

### Hard gaps, with the workaround

**`position: fixed` and `position: sticky` are not supported.** Neither is
`position: static`, which has a second consequence: *an absolutely positioned
node is always positioned relative to its immediate parent*, so there is no
containing-block escape. `styles.css` uses `fixed` in four places — the
toolbar, the find bar, the notice line, the modal backdrop.

*Workaround, and it is an improvement.* Make the window root a flex column: a
chrome row, the viewer as the flex-growing scroller, the status strip. Nothing
needs to be `fixed` because nothing is over a scrolling body any more. Overlays
(popovers, modal) become direct children of the root with `position: absolute`
and coordinates computed by hand — which `showPopover` already does, since it
already tracks its anchor.

**`overflow: auto` is not supported** (`scroll`, `hidden`, `clip`, `visible`
are). Five uses. Change to `scroll` and use `scrollbar-width` / `scrollbar-color`,
both of which are supported.

**`text-overflow: ellipsis` is not supported** (Parley #304). Eight uses — every
truncated label in the sidebar, the menus and the toolbar. Options: clip and
accept it, fade with a gradient mask (`mask-image` *is* supported), or measure
and truncate in Rust with a real "…". The mask route looks best and is arguably
nicer than an ellipsis — and Phase 0 confirmed it works, which was not a
foregone conclusion: `white-space: nowrap` was itself broken on `0.7.10`, so
the line wrapped instead of overflowing and there was nothing for a mask to
fade. It is fixed on `main` (a 120px box holds the sentence on one 16px line),
and the chrome spike's title fades out exactly as intended.

**SVG styling is not supported.** Static SVG renders; CSS applied to it does
not. `icons.ts` is 33 icons built as path strings styled entirely through CSS —
`stroke: currentColor`, `fill: currentColor`, theme variables. Every icon in the
app would render in whatever the default is.

*Workaround:* generate each icon as a complete SVG document with the colour
baked into presentation attributes, memoised per theme. Themes change rarely
and there are 33 icons, so this is a `HashMap<(Icon, Color), String>` and a
regeneration on theme change. Slightly clunky; entirely tractable. The
alternative — drawing icons as Vello paths through a tiny custom paint source —
is cleaner but couples the icon set to the renderer.

**`mix-blend-mode` is not supported.** Already forbidden by AGENTS.md ("Do not
tint the document with `mix-blend-mode`"), and the recolouring moves to a shader
anyway. No cost.

**`text-shadow` is not supported; `filter` and `backdrop-filter` are partial**
(full with the Skia backend, blur and drop-shadow only with Vello Hybrid). One
`backdrop-filter: blur(2px)` on the modal backdrop. Drop it or accept a plain
scrim.

**Clipboard events do not exist** — `cut`, `copy`, `paste` are all unsupported.
Copying selected text is a keybinding into `arboard`, which is a native app's
answer anyway and is *better* than the DOM's: we have the real text, not what
happens to be in a text layer.

**IME does not exist** — no `compositionstart` / `update` / `end`. The find
field and the settings fields cannot take composed input, which means CJK,
Vietnamese and accent-composition input into search is broken. This is the
gap with no workaround at this layer; it needs a fix upstream in Blitz. **Flag
it to the user as a real regression for a class of readers.**

**`ResizeObserver`, `IntersectionObserver` and the `resize` event do not
exist.** The app measures elements constantly — viewport width for fit modes,
the sidebar's dragged width, the thumbnail column's proportions. Replace with:
window size from winit (`use_window`), and element geometry from Dioxus's
`onmounted` + `get_client_rect()`, recomputed on the events that actually change
things (window resize, sidebar drag, toolbar toggle). More explicit, more code,
no loss of function.

**`change` and `select` events do not exist; `input` does.** Fine — the app's
fields are already driven on `input` with explicit commit semantics.

**`<input type="password">` is not supported.** The encrypted-document prompt
uses one. Masking has to be done by hand over a `type="text"` field, and doing
that well (the caret, selection, paste) is fiddlier than it sounds. Or ask for
the password in a native dialog.

**`<select>`, `<dialog>`, `<progress>`, `<meter>` are not supported.** The app
uses none of them — every menu and the modal are hand-built already. This is
the payoff for the "no clunky UI" rule having been taken seriously.

**Drag and drop events do not exist.** Nothing in the app uses them today
(the sidebar resize is pointer events, which are supported), but "drop a PDF on
the window to open it" is a feature this forecloses until Blitz adds it —
though winit's own `DroppedFile` event covers the file-drop-onto-window case,
which is the one that matters.

### Things that work and were worth checking

A custom widget drawing into a `wgpu::Texture` ✅ — `blitz_dom::Widget`
attached to an `<object data=…>`, which is what `use_wgpu` and `<canvas src=…>`
became. `clip-path` ✅ (the app clips
pictures and links). `mask-image`/`mask-composite` ✅. `opacity`, `box-shadow`,
`border-radius`, `z-index`, 2D `transform`, `filter: blur` ✅. Flexbox and grid
✅. `@font-face` and variable fonts ✅. `transitions` and `animations` ✅ — the
brief only wants them on user action anyway. `wheel` and `scroll` events ✅.
`user-select` ✅. `cursor` keywords ✅. `pointer-events` ✅. `keydown`/`keyup`
✅. `contextmenu` ✅ (the app suppresses the webview's own; here there is none
to suppress). AccessKit is a first-class feature flag.

### Multi-window: supported, but not through `launch()`

The app's whole "Two documents at once" architecture depends on this, so it was
checked precisely — and then built. `dioxus_native::Config` exposes only
`with_window_attributes`, one window, and **`DioxusNativeApplication::add_window`
does not do what its name suggests**: it pushes onto
`BlitzApplication::pending_windows`, which is drained when winit says surfaces
can be created and never again, and the Dioxus half of a window's setup —
the contexts and `initial_build()` — is done by `launch` for its one window
only. A window added that way comes up empty and stays empty.

What works, and is what `dioxus-spike/src/shell.rs` does in about three hundred
lines, is to own `BlitzApplication` directly — its fields are public — and do
the per-window setup yourself: `View::init`, provide the renderer and window
contexts into `ScopeId::ROOT`, `initial_build()`, resume, insert into
`inner.windows`. Each window gets its own `VirtualDom`, its own renderer and
its own surface, which is exactly the shape the app already has ("A window is a
whole second reader"). Three windows, one of them asked for from another
thread, is the spike that passes.

**It remains the highest-risk item on the list.** It is off the documented
path, it is written against public fields rather than a supported API,
multi-window has a history of being broken on Windows in this project, and the
app's window story is elaborate: `hand_over`, `Placements` (because a shown
window on macOS jumps to the launch window's frame — winit still gets the y
wrong by a title bar, so the position must still be set a second time),
geometry owned by `main`, the restore list, `Exiting`. It has been shown to
work on macOS and nowhere else. If it does not hold up on all three platforms,
the rewrite is not worth doing, because a reader who cannot open two papers
side by side has lost the thing this app most recently gained.

---

## What gets better

Not a consolation list — several of these are things the current architecture
cannot have at all.

**Selection stops being a pixel trick.** `paintSelection` copies pixels off the
page canvas, runs them through a luminance ramp and lays the copy back over the
line, because giving `::selection` a colour would put pdf.js's text layer on
screen and a page's bold type would come back as regular. With pdfium's
character boxes there is no text layer and no substitute font: draw the
selection rectangles under the page and tint the glyphs in the shader. ~200
lines of careful, load-bearing code deletes.

**Recolouring becomes a shader.** `colouredRows()` reading the page small,
`canBlend()` probing five blend modes, `recolorByPixel` with its mask
allocator, the `WHITE_POINT` clamp done twice because two paths must agree to
within one level out of 255 — all of it exists because a canvas offers
composite operations and a pixel array and nothing between. One WGSL function
does the whole ramp, keeps hue by construction in HSL, and costs nothing at
composite time. The theme also leaves `keyFor()`, so changing theme stops
invalidating every rendered page.

**Markup stops being clunky, which is the thing the brief flags.** The current
implementation is shaped entirely around one pdf.js limitation: `saveDocument()`
can create `FREETEXT`, `HIGHLIGHT`, `INK`, `STAMP` and `SIGNATURE` and
`Annotation.save()` is not overridden by any markup subtype, so **an annotation
already in the file cannot be edited or deleted at all**. Everything downstream
— the journal, `annotation_id: null`, rebuild-from-backup for removal,
byte-truncation undo — is scaffolding around that hole. pdfium (or `lopdf` for
an incremental update) can create, edit and delete annotations, and can write
underline, strike-out and squiggly as well as highlight. The journal shrinks
back to what it should be: a cache and a recovery log for documents that were
rebuilt underneath the reader.

**Startup and idle cost.** No webview process, no worker spin-up (which is
most of pdf.js's two-orders-slower document open), no `WKWebView` floor. The
Dioxus Native docs claim sub-1ms frame times and sub-100ms startup for simple
UIs; take that as a direction, not a promise, and measure it in Phase 1.

**Encrypted documents open.** pdf.js gets a password prompt and nothing else;
pdfium decrypts. The `password.test.mjs` path — ask, refuse, give up — stays,
and the third branch stops being "give up".

**One language.** `settings.test.mjs` exists solely because the settings table
is written three times (Rust defaults, `fallbackDefaults`, the `Settings`
type). Two of those copies disappear.

---

## What gets worse, stated plainly

**Binary size.** 6.2MB today. Add wgpu, Vello, Stylo, Taffy, Parley, winit,
AccessKit and pdfium; subtract 5.6MB of embedded frontend. The Dioxus docs
quote 10-15MB for native bundles; with pdfium, 12-20MB is the band to expect
and 15MB the number to plan against. **This is 2-3× the current binary and the
brief's goal (2) permits it only as a price for memory.** If Phase 1 does not
show a large memory win, the trade is not paid for. Phase 0 measured the top
of that band: the page spike, stripped and `opt-level = "s"`, is **12.4MB plus
7.2MB of `libpdfium.dylib`**, and it carries no settings, no themes, no
sidebar and no search.

**And the memory floor is not zero.** An empty window — one widget, one small
texture, no document — measured **112MB** of RSS, against 182MB for the whole
of a Tauri reading session today. One page mounted and drawn is 162MB. That is
the number Phase 1's gate is really against, and it is close enough to the
thing being replaced that it has to be measured against `vello_hybrid` as well
as `vello` before anything is concluded. The first Phase 0 pass, on older
versions of Blitz, wgpu and Vello, measured this floor at half the size.

**The test apparatus.** `npm test` starts a dev server, generates fixtures and
runs 17 files against a real WebKit through `scripts/ui-harness.mjs`. All of it
depends on `api.ts` having a browser twin, and `api.ts` disappears. This is
several thousand lines of test infrastructure and it is the reason the recent
critical-read pass could be done at all.

**GPU variance replaces engine variance.** Today's risk is "WebKitGTK might
drop a blend mode", which is why `canBlend()` and `recolorByPixel` exist. The
new risk is drivers: Vello wants compute shaders, `vello_hybrid` and
`vello_cpu` are the fallbacks, and choosing between them per machine is a new
thing to get right. Different problem, same shape, and at least the fallback is
first-party.

**Maturity.** Tauri 2 is production software with a large user base. Blitz is
alpha-to-beta with 48% of the WPT css subsuite. Bugs found here are bugs to be
fixed upstream or worked around, not bugs to be looked up.

**IME.** Named above. A real regression, with no local fix.

---

## Testing, which needs its own plan

Losing the harness is the largest hidden cost and the one most likely to be
discovered late. What replaces it is genuinely good, but it has to be built
deliberately.

**Blitz renders headlessly, deterministically, on the CPU.** `anyrender_vello_cpu`
is already a dependency, and Blitz's own HTML-to-image path exists for exactly
this. So: build the app's `DioxusDocument`, drive it, render to a pixmap,
compare against a reference PNG. No dev server, no browser, no Playwright, no
`HYLOPDF_PLATFORM=other` — the whole app under `cargo test`, in-process, with a
software rasteriser that produces the same pixels on every machine. That is
better than what exists today, which cannot test rendering across engines at
all and says so.

What ports directly, as `cargo test`:

- `search.test.mjs` → `fold` and match stepping, in Rust
- `recolor.test.mjs` → the shader's ramp against a reference implementation
- `theme.test.mjs`, `settings.test.mjs` → mostly already Rust-side
- `keys.test.mjs` → the chord table, both platforms, via `cfg`
- `labels.test.mjs`, `document.test.mjs`, `notext.test.mjs`, `notes.test.mjs`,
  `markup.test.mjs` → against the renderer trait, with the same generated
  fixtures
- `seams.test.mjs` → re-aimed: the renderer trait is the only door to pdfium

What needs building new: an in-process harness with `press`, `wheel`, `click`,
`state()` and `screenshot()` against a headless `DioxusDocument` — the same
surface `scripts/ui-harness.mjs` offers, in Rust. Budget this as a Phase 2
deliverable, not as something that emerges.

---

## Risks, and what would kill it

| risk | signal | verdict if it fires |
| --- | --- | --- |
| a shell of our own does not work on all three platforms | Phase 0 spike (passed on macOS; Windows and Linux unrun) | **stop** — multi-document is not negotiable |
| a widget per mounted page is slow or capped | Phase 0 spike: passed, at 4.1ms a page at 10.1MP — but every widget in the document is painted every frame, so the mounting window is load-bearing | fall back to one viewport-sized texture we composite into; if that is also bad, **stop** |
| Vello unusable on common Linux hardware | Phase 1 on a VM and an Intel iGPU | ship `vello_hybrid` by default, `vello_cpu` as fallback; if neither is smooth, **stop** |
| memory win is small | Phase 1 measurement | **stop** — the binary cost is unpaid |
| text quality worse than the webview (hinting, subpixel) | Phase 1 screenshots side by side | probably livable; the brief cares about the look |
| Blitz bugs block a UI element | continuous | fix upstream or work around; the CSS gap list above is the known set |
| IME | known now | flag to user; accept or wait for Blitz |

---

## The plan

Branch `dioxus-experiment` (it exists). Nothing lands on `main` until the whole
thing is at parity, exactly as the pdfium prototype was handled.

### Phase 0 — the four spikes, in a scratch crate (days, not weeks)

Answer the questions that can kill the project, before writing anything that
looks like the app. No PDF, no theme, no settings.

1. **Two windows.** `create_default_event_loop`, `DioxusNativeApplication::new`,
   `add_window`. Open two, type in both, close one, close the app. On macOS,
   Windows and Linux. Check that a window placed by hand stays where it was put
   after `show` — the `Placements` bug is a macOS AppKit behaviour and may well
   recur here.
2. **A page on the screen.** A `blitz_dom::Widget` that registers a
   `wgpu::Texture`, filled from a pdfium render, attached to an `<object>`.
   Then twenty of them in a scrolling column, mounted and unmounted. Measure:
   time per page, GPU memory, and whether one widget per page is a supported
   shape or a novelty. (Answered: it is supported, and every widget is painted
   every frame whether or not it is on screen — so the mounting window from
   `viewer.ts` has to be ported before any of this is measured properly.)
3. **The shader.** Port `recolor`'s luma-to-HSL ramp to WGSL and check it
   against `recolorByPixel` on a fixture page, to the same one-level-in-255
   tolerance `recolor.test.mjs` already uses.
4. **Chrome that looks right.** Rebuild the toolbar, one popover menu and the
   notice line from `styles.css` with `fixed` removed, and screenshot it beside
   the real app. This is where the CSS gap list either holds or does not.

**Gate:** all four, or stop and write down what failed.

### Phase 1 — a reader that reads (2-3 weeks) — **built; see `PHASE1.md`**

Open a document, scroll it continuously, fit width, zoom, one theme, recolour.
No sidebar, no search, no settings window, no markup, one window. Port
`viewer.ts`'s layout math wholesale — `boxes[]`, `rows()`, `firstBoxEndingAfter`,
`mount`, `OVERSCAN`, the page LRU. Keep the pdfium calls behind the renderer
trait from the first line.

**Then measure, against the same four documents in AGENTS.md's memory table and
the same five in its speed table.** Binary size, RSS after 60 viewports, time
per page, time to first page. This is the gate the whole proposal rests on:

| document | today | target |
| --- | ---: | --- |
| 400 pages of plain text | 346MB | under 150MB |
| 40 pages of bitonal scan | 351MB | under 150MB |
| 27 pages of photographs | 323MB | under 150MB |
| one page of 12000×16000 bitonal | 327MB | under 150MB |

Those targets are estimates from first principles — one process, one CPU buffer
per page in flight, GPU textures capped by the same LRU that caps canvases
today — not predictions. **If the number lands near 300MB, the experiment has
failed and the binary cost buys nothing.**

*It landed at 238MB resident, and the shape of it was not what this paragraph
expected: the pages cost almost nothing and the window costs almost everything.
See "The gate" in `PHASE1.md`. Then it landed at 144MB of footprint against
Tauri's 373MB, once the window's cost turned out to be a renderer's fixed
scratch buffers and the reader's own copying: see `FLOOR.md`, which is also
where the metric these targets should have been stated in is argued.*

### Phase 2 — the harness, before the app grows (1 week)

Headless `DioxusDocument`, software rasteriser, `press`/`click`/`wheel`/`state`/
`screenshot`, reference PNGs. Port `search`, `keys`, `theme` and `settings`
tests to `cargo test`. Do this *before* Phase 3, because the alternative is
writing 10,000 lines with no net and finding out at the end.

### Phase 3 — parity, in the order the app was built (6-10 weeks)

Roughly the order of the existing commit history, which is a reasonable order
because each step was chosen to be testable:

1. themes, settings, the settings window, the theme editor
2. the keyboard: the action table, chords, `keys.toml`, the Keyboard page
3. sidebar: contents, thumbnails with their LRU, marks
4. search: `fold`, the index, match stepping, the find bar and its three switches
5. links, destinations, page labels, the go-to field
6. spreads, trim, rotation, paged mode, presenting
7. the library: position, open documents, marks, the restore list
8. the watchers: themes directory, the document being recompiled
9. multi-window: `hand_over`, placements, the Dock menu, `Exiting`
10. markup — and this is where it stops being a port, because the annotation
    hole that shaped the current design is gone. Rebuild it as it should have
    been: create, edit and delete real annotations; keep the journal only for
    what a rebuilt document lost.

### Phase 4 — the decision

Same shape as the pdfium write-up: a table of measurements, a plain verdict,
and either a merge or a parked branch with its reasoning intact. The parked
branch is a perfectly good outcome and the pdfium one has already paid for
itself twice in this document.

**Total, honestly: three to five months of sustained work** to reach parity with
an app that took as long as it took. Anyone budgeting less has not counted
`viewer.ts`.

---

## Two things to decide before Phase 0

**1. The binary.** The brief says "small binary" and this trades it away. 15MB
instead of 6.2MB, for an expected 2-3× memory reduction and the end of the
webview floor. That is a reasonable trade and the brief's goal (2) explicitly
allows it, but it should be stated as a decision rather than discovered at the
end. If the answer is that the binary matters more, the alternative is to stop
here and instead spend the same weeks reducing memory inside the current
architecture — where the last such pass took 2521MB to 327MB, so there may well
be more to find.

**2. IME.** No composed input in the search field until Blitz has composition
events. If HyloPDF is meant for readers writing CJK, that is a blocker and the
Phase 0 gate should include asking upstream when it is coming.

## Sources

- [Blitz status: CSS](https://blitz.is/status/css), [elements](https://blitz.is/status/elements), [events](https://blitz.is/status/events), [WPT](https://blitz.is/status/wpt)
- [Blitz — About](https://blitz.is/about) and [the repository](https://github.com/DioxusLabs/blitz)
- [Blitz roadmap, issue #119](https://github.com/DioxusLabs/blitz/issues/119)
- [dioxus-native on docs.rs](https://docs.rs/dioxus-native) and [blitz-shell](https://docs.rs/blitz-shell) — but `main` is what Phase 0 builds against; the Custom Widget API is PR [#425](https://github.com/DioxusLabs/blitz/pull/425)
- [Dioxus: the Native platform](https://mintlify.wiki/DioxusLabs/dioxus/platforms/native) and [the native renderer on DeepWiki](https://deepwiki.com/DioxusLabs/dioxus/5.6-native-renderer-(blitzvello))
- [Vello](https://github.com/linebender/vello) and [vello_hybrid](https://docs.rs/vello_hybrid)
- [hayro](https://github.com/LaurenzV/hayro) and [hayro-interpret](https://docs.rs/hayro-interpret)
- This tree: `AGENTS.md`, and the `pdfium-prototype` branch's measurements
