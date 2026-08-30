# Phase 1: a reader that reads

**The question this file ends on has been answered — `FLOOR.md`, beside this
one, and it changes the numbers below.** Two things in particular. *Every
memory figure here is `ps -o rss`, which on macOS does not see GPU memory*, so
the table understates by up to three times and the `vello`/`vello_hybrid`
comparison at the bottom of "The gate" is wrong rather than imprecise — the
real difference is 208MB against 19MB. And *the ~110MB floor this file could
not explain was never Blitz's*: 173MB of it is a scene-independent constant in
`vello`'s buffer sizing, and 96MB of it was this crate making three copies of
every page it drew. With `vello_hybrid` and a page buffer that is reused, the
same reading session is **144MB against Tauri's 373MB**, measured the same way.
Read `FLOOR.md` before acting on anything below.

`dioxus-assessment.md` is the plan, `FINDINGS.md` is what Phase 0's four spikes
answered, and this is Phase 1: **open a document, scroll it, fit it, zoom it,
put a theme on it** — with the layout ported from `viewer.ts` rather than
reinvented, the renderer behind one trait from the first line, and then the
measurement the whole proposal rests on.

The crate is `dioxus-reader/`, beside the spike. It is a reader you can use:

```
cargo run --release -- book.pdf                  # read it
cargo run --release -- book.pdf --theme 1        # …in Hylo Dark
cargo run --release -- book.pdf --measure 60     # read it, and say what it cost
cargo run --release -- book.pdf --quit 5         # open, sit still, report, close
cargo test                                       # the layout, the theme, the shader
```

Wheel scrolls. `j`/`k` and the arrows move a line, `d`/`u` half a screen, space
and Page Up/Down a screen, Home/End the ends, `n`/`p` a page, `+`/`-` zoom, `0`
fit width, `9` fit page, `s` spreads, `t` theme. All of it was checked in the
real app, and the screenshots are in this session's scratchpad rather than in
the repository.

---

## The gate

The assessment says Phase 1 ends in a measurement and that the measurement
decides whether there is a Phase 3. Here it is, from `--measure 60` — sixty
screenfuls of a document, driven by synthetic wheel events through the shell,
with nothing taken from the machine.

**One machine, one sitting, macOS 15 on Apple silicon, release build, a
1100×900 window at 2×.**

| | Tauri + pdf.js today | Dioxus Native + pdfium |
| --- | ---: | ---: |
| 400 pages of plain text, after 60 screenfuls | 346MB | **238MB** |
| …the same, in a recolouring theme | 346MB | **238MB** |
| a 14-page paper of figures, after 60 screenfuls | — | 286MB |
| one page drawn | 27ms (pdf.js) | **6.6ms** drawing + 5.2ms uploading |
| opening a 400-page document | ~2 orders slower (worker start) | **30-100ms** |
| pages resident while reading | canvas + proxy + worker copy | **2 pages, 25MB each** |
| binary | 6.2MB | **11.9MB + 6.9MB of pdfium** |

Three of those lines are the answer and they do not all point the same way.

**The per-page cost is as good as the assessment hoped.** A page is drawn in
6.6ms, uploaded in 5.2ms, and exists exactly once — one `wgpu::Texture`, no
worker copy, no `ArrayBuffer`, no canvas. Two pages are mounted at a time, so
the document contributes 50MB and does not grow with the book: the 400-page
fixture and a 14-page paper cost the same. The bridge that killed the pdfium
prototype is genuinely absent — 43ms a page became 5.2ms of `write_texture` in
the same process.

**Recolouring is free, which is the clearest win on the list.** The dark theme
costs the same milliseconds and the same megabytes as the light one, because
the ramp is a compute pass over the page as it is uploaded, and pdfium's BGRA
becomes RGBA in the same pass — the CPU swizzle Phase 0 measured at 1.6-5.1ms a
page is gone with it. In the app this is 835 lines of `themes.ts`, a blend-mode
probe, a pixel fallback and a mask allocator; here it is one WGSL function that
`cargo test` holds to the same one-level-in-255 tolerance as the app's own two
paths.

**And the memory win is real but modest, because the floor is the stack rather
than the pages.** 238MB against 346MB is a third off. The assessment's target
was "under 150MB", and the reason it is not met has nothing to do with
documents:

| what | RSS |
| --- | ---: |
| the process, document open, before any window | 15-22MB |
| a window at 600×400, one page mounted (7MB of texture) | 184MB |
| a window at 1100×900, one page mounted (25MB) | 234MB |
| a window at 1600×1200, one page mounted (47MB) | 298MB |
| a window at 1100×900, sixty screenfuls read (50MB) | 238MB |

Reading a 400-page book costs **four megabytes more than opening it**. What
costs 200MB is having a window at all, and it grows with the surface: from
600×400 to 1600×1200 the textures account for 40MB of the 114MB difference and
the renderer's own buffers for the other 74MB.

**Two of Phase 0's own binaries, rebuilt in release, say where most of that
floor comes from.** `widget` — one window, one 640×480 texture, no pdfium, no
text — is **105MB**, and `chrome` — the toolbar, a popover, real text and SVG
icons, still no pdfium — is **111MB**. So *before this experiment writes a
line*, a Blitz window costs about 110MB on this machine, and the reader's
600×400 floor of 184MB is that plus pdfium's dylib and open document (15-22MB,
measured before the window exists), one page of texture, and about 45MB that is
not yet accounted for and is most likely the page bitmap the allocator has not
given back plus wgpu's own buffers. **The thing to attack is the 110MB, and it
belongs to the stack rather than to the app.**

**`vello_hybrid` was measured beside `vello`, as the assessment asked, and it
is not the answer either**: 228MB idle and 230MB after sixty screenfuls,
against 234MB and 238MB. Three per cent, and pages draw correctly through both.
That closes item 3 on Phase 0's list.

*It did not close it, and this paragraph is the reason `FLOOR.md` exists.*
Those are resident sizes, and a GPU buffer is charged to a process's physical
footprint rather than to its resident size — so the one measurement taken to
decide between two renderers was taken in the one unit that cannot see the
difference between them. As footprint: **208MB against 18.8MB** on an empty
window. `vello_hybrid` is the default now.

So the honest summary of the gate: **the per-page architecture is vindicated
and the floor is not.** The binary is three times the size, the memory is two
thirds, and the number that would justify the trade — a floor well under
Tauri's whole reading session — is a fixed ~200MB that this experiment has not
yet explained. **Phase 1's real deliverable is that the next question is now a
specific one:** what is in the 200MB, and how much of it is wgpu/Metal's
allocator, Stylo, fontique's system-font enumeration, and pdfium's own
mappings. None of that is document-shaped, and all of it is measurable with the
`--quit` mode this crate already has.

---

## What was built

```
dioxus-reader/
  src/layout.rs     viewer.ts's layout, ported: rows, boxes, the two binary
                    searches, the mounting window, anchors, the render ceiling
  src/render.rs     the one door to a renderer: pages, sizes, pixels
  src/pdfium.rs     pdfium behind it
  src/gpu.rs        upload + the recolouring compute pass
  src/recolor.wgsl  the ramp, from Phase 0, plus a passthrough branch
  src/page.rs       one page as a `blitz_dom::Widget`
  src/app.rs        the reader: state, components, keyboard, wheel
  src/styles.rs     the stylesheet, themed, with the three missing properties
                    worked around
  src/theme.rs      five colours, derived shades, strict hex
  src/shell.rs      Phase 0's shell, plus synthetic event injection
  src/stats.rs      what a session costs, counted where it happens
  src/main.rs       the window, `--measure`, `--quit`, `--theme`, `--width`
  tests/recolor.rs  the shader against the reference implementation
```

**The layout port is the part that went exactly as advertised.** `relayout`,
`rows`, `row_of`, `first_box_ending_after`, `last_box_starting_above`,
`mounted`, `page_at`, `anchor` and `scroll_target` are `viewer.ts` line for
line, with the comments that explain *why* carried over with the lines. It is
250 lines of Rust with no renderer, no widget and no window in it, and eleven
tests that `cargo test` runs in no measurable time — including the two that the
app can only assert by opening a browser: that the searches agree with a scan
over the whole document, and that the mounting window contains every page in
the overscan band and no page outside it.

**The renderer trait held its shape.** `render.rs` names four things and
declares three; `pdfium.rs` is the only file that mentions pdfium; nothing else
imports it. That is the seam rule applied to the new tree, and it is what would
make hayro a decision rather than a rewrite.

---

## Seven things about Blitz that cost time

Phase 0 said the API moves and the cost is paid in the shell. That was true and
incomplete: most of what follows is not in the shell.

**1. `MountedData` panics rather than failing.** `scroll`, `get_client_rect`
and `set_focus` all take `doc_mut()`, and every place a component can call one
from is already inside a borrow of the document: a DOM event handler runs
inside `EventDriver`'s borrow, a mounted handler inside
`flush_queued_mounted_events`'s. The result is `RefCell already borrowed`, from
a stack that names neither. `NodeHandle::try_doc` exists and says as much in
its own doc comment — the safe method is the one a reader does not need.

*So the scroll offset is ours.* The viewer does not use `overflow: scroll`; it
holds a number, the wheel moves it, and the pages are placed against it. That
is what `viewer.ts` does in all but the last step anyway. What is lost is the
scrollbar and the platform's fling — a scrollbar we would have to draw, and
momentum arrives from the trackpad in the event stream regardless. It is the
largest single thing this phase gave up.

**2. The viewport has to be asked for, not observed.** There is no
`ResizeObserver` and no `resize` event, and `get_client_rect` is the call above
that panics. The window's own size minus a chrome height this file knows is
what the layout runs on.

**3. A key with nothing focused goes to `<html>`.** Blitz sends keys to the
focused node and falls back to the *root element*, which is above anything a
component can put a handler on, and events bubble up rather than down. So the
reader's root takes focus when it mounts, and then one `onkeydown` is the
app-level handler `main.ts` has. Before that, every key in the app did nothing
and nothing said why.

**4. `use_window_event` is closed to a shell of our own.** It consumes an
`Rc<WindowEventHandlers>` from a context only `DioxusNativeApplication`
provides, and the type is private to `dioxus-native`. Add it to Phase 0's list
of things the shell has to do without — beside the navigation provider, the
HTML parser provider, and `add_window` not adding a window.

**5. A texture must not be registered and drawn in the same frame.** It works
until something else is unregistered in that frame too — which is exactly what
happens when the window's real size arrives and every page is replaced at once
— and then Vello panics with "tried to draw an invalid empty image" from the
atlas upload, on the third frame of every run. The fix is a page registered on
one frame and drawn from the next, with the widget asking the shell provider
for that next frame, because `requires_redraw()` cannot: `is_animating()` is
read at the *start* of a frame, before the paint that would set the flag.

**6. A page's texture belongs to its node, not to its widget.** Unregistering
from inside `paint` is what item 5 forbids, and leaving a replaced texture
registered leaks a page for every zoom step. So what `keyFor()` carries — the
page, the size it is drawn at, the theme it wears — is the *component key*: a
change to any of them is a different node, a new widget, a new texture, and the
old node's resources released by Blitz between frames, where it is safe.

**7. Keeping the page as drawn costs more than redrawing it.** The design that
keeps both copies — pdfium's output and the themed one — makes a theme change a
compute pass rather than a re-render, and costs 25MB a page: 100MB of texture
for the three pages a window holds, against 50MB. Re-rendering costs 7ms. The
source is uploaded, read once by the compute pass, and dropped.

---

## What is not built

No sidebar, no search, no outline, no links, no text layer, no selection, no
markup, no settings window, no library, no watchers, one window. Those are
Phase 3 and the point of leaving them out is that the numbers above are about
the thing being proposed rather than about a half-built app. Two things that
*are* in the assessment's Phase 1 scope and are not here: `measureCrop` (trim
margins) and paged mode, both of which are layout and both of which the ported
`Layout` has room for.

**The Phase 2 harness has its seed.** `Shell` takes an `Inject(WindowEvent)`
embedder event and hands it to the window through `View::handle_winit_event`,
which is public — so a test can move the pointer, turn the wheel and press a
key with no OS involvement and the window in the background. `--measure` is
built on it. What it still needs is a way to read a frame back (Blitz's own
HTML-to-image path over `anyrender_vello_cpu`) and the `state()` the Playwright
harness offers.

---

## The decision this leaves

The experiment is not failing and it is not yet paying for itself. Against the
brief's goal 2 — fast, small, little memory — Phase 1 says: **faster per page
by a factor of four, a third less memory, three times the binary.** The third
is worth having; it is not the "two to three times" the binary cost was
supposed to buy, and the reason is a fixed ~200MB floor that is nothing to do
with PDFs.

Three ways forward, in the order I would take them:

*Done, and the answer is in `FLOOR.md`: none of it was Blitz's. The list below
stands as it was written; item 1 is closed, and item 2 is now next.*

1. **Find out what the 110MB is made of** — the two spike binaries above make
   this a small, bounded question, because they reach it with none of this
   crate's code in the process: an empty Blitz window on this machine costs
   about as much as the whole of Tauri's app process. If a large share is
   fontique enumerating every system font at startup, or wgpu's allocator
   holding staging buffers, both have levers, and both would be worth an
   upstream issue. If it is Metal's per-surface overhead, it does not get
   better, and the honest conclusion is that a GPU renderer's floor is
   comparable to a webview's — which is the one finding that would end this
   experiment.
2. **Phase 2, the harness**, before the app grows — unchanged from the
   assessment, and the injection path already exists.
3. **Then Phase 3**, which is the long one.

Nothing here argues for stopping. It argues for measuring the floor before
writing another ten thousand lines against it.
