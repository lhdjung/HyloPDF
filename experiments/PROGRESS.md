# The Dioxus Native experiment: where it stands

`brief.md` is the ask and `dioxus-assessment.md` is the plan. This file is what
building it actually found — Phases 0 to 3 — and it is the only status file:
the four it replaces (`FINDINGS.md`, `PHASE1.md`, `FLOOR.md`, `PHASE2.md`) each
opened by correcting the one before, which is three quarters of a document to
read before reaching a true sentence. They are in git at `d2b0370` if the
working is ever wanted.

**The experiment is passing its gates, and the one thing it is now blocked on
upstream is IME** — see the end of this file: the find bar built in Phase 3
item 4 cannot take composed input, because Blitz has no composition events,
and that is a decision rather than something to work around. Four upstream
*faults* were found and all four are worked around in this tree, with a test
each that will fail the day they are fixed.

```
cd dioxus-reader
cargo run --release                          # the 400-page fixture
cargo run --release -- ~/paper.pdf           # a document of your own
cargo run --release -- --theme 4             # …in the fifth theme in the list
cargo run --release -- --measure 60          # read it, and say what it cost
cargo run --release -- --quit 5              # open, sit still, report, close
cargo test                                   # 191 tests, about a minute and a half
cargo test -- --ignored                      # the one that aborts on purpose
```

With no path it opens **whatever was open when it was last put down**, and
the app's own `tests/fixtures/book.pdf` when there was nothing. See Phase 3
item 7 — and note that `--measure` and `--quit` deliberately do *not* restore:
every number below was taken on that fixture, and a measuring run that quietly
used whatever had last been read would not be comparable with any of them.
(The fixture used to be documented as `-- book.pdf`, which is a path relative
to wherever cargo was run from and was therefore usually not there — and
pdfium reports a missing file as a Debug-printed `io::Error`. Both halves of
that are fixed.)

**The keys are the app's own**, because `keys.ts` and `keys.toml` are ported —
see Phase 3 item 2. `j`/`k` and the arrows move a line, `d`/`u` half a screen,
space and Page Up/Down a screen, Home/End and `g g`/`G` the ends, `h`/`l` and
the left/right arrows a page, ⌘+ and ⌘− zoom, ⌘0 fit width, ⌘1 actual size,
⌘2 fit page, ⌘B the sidebar, ⌘⇧B a mark on the page you are on, ⌘F the find
bar with ⌘G and ⌘⇧G through the matches and Escape out of it, `p` or ⌥⌘G the
page field, ⌘[ and ⌘] back and forward through the places you jumped from,
⌘R and ⌘L turn the page a quarter each way.
`s` spreads
and `t` the next theme are this experiment's own and are not in the app. Any
of them can be rebound in `keys.toml`; a key bound to something not built yet
says so on the notice line. The theme, the zoom, the fit, the spread, the
sidebar, the trim, the marks and **the page you had got to** are all still
there the next time it opens.

**Two things are settings and nothing else.** Trimming the margins is the
chip marked Trim in the toolbar — the app puts it in a menu, and there are no
menus here yet — and one page at a time is `scroll_mode = "paged"` in
`settings.toml` with no key and no chip at all, which is the brief's own
instruction about it. See Phase 3 item 6.

## The numbers

One machine, one sitting, macOS 15 on Apple silicon, release builds, a
1100×900 window at 2×, `tests/fixtures/book.pdf` (400 pages of plain text).
Every memory figure is **physical footprint** — what Activity Monitor shows and
what the kernel charges against a limit. Resident size is the wrong unit here
and cost this experiment a wrong conclusion once; see "Measure footprint, never
RSS" below.

| | Tauri + pdf.js | Dioxus Native + pdfium |
| --- | ---: | ---: |
| document open, nobody scrolling | 373MB | **144MB** |
| after ~60 screenfuls | 466MB | ~200MB, settling to 140MB |
| one page drawn | 27ms | **3.2ms** |
| …uploaded to the GPU | — | 4.7ms |
| opening a 400-page document | ~2 orders slower (worker start) | 30-150ms |
| pages resident while reading | canvas + proxy + worker copy | 2, at 23MB |
| binary | 6.2MB | 12MB + 7.2MB pdfium |
| the sidebar open, thumbnails and all | measured with it shut | +24MB |
| the whole document indexed for a search | tens of MB, given back with the bar | 15MB, given back with the bar |
| …and reading it to build that index | pdf.js: mostly worker | **62ms for 400 pages** |

The Tauri column is the installed app on the same document measured the same
way, summed over its four processes. **The assessment's Phase 1 gate — under
150MB against 346MB — is met**: 144MB against 373MB, a factor of 2.6, for
twice the binary. That is the trade the brief's goal 2 permits, and it is paid.

**The sidebar row is the app's own warning answered.** `AGENTS.md` says its
memory table was taken with the panel shut, and that the thumbnail column is
where a fourth leak would hide next. Here the column is four thumbnails at
1.2ms each and 12MB of texture, the whole panel costs 123MB → 147MB of
footprint on the same document, and it does not grow: fifty screenfuls of
column end where ten of them did. See Phase 3 item 3 — there is no thumbnail
cache, because the mounting window is one.

What is *not* measured: Windows, Linux, and any document but this one. A
scanned volume is the shape most likely to behave differently.

---

## Phase 0 — the four spikes

`dioxus-spike/`. All four gates pass, on Blitz `main` at `64eb2785`
(`0.3.0-beta.2`, published as `dioxus-native 0.8.0-alpha.1`), wgpu 29, winit
`0.31.0-beta.2`, pdfium `chromium/8021`.

```
cargo run --bin windows -- --auto 3   # three windows, made from a thread
cargo run --bin pages   -- --pages 20 # a document, drawn by pdfium
cargo run --bin chrome  -- --menu theme
cargo run --bin widget                # one widget, and the frames it costs
cargo run --bin probe                 # the DOM, with no window in front of it
cargo run --bin floor -- --all        # the stack, one layer at a time
```

`libpdfium.dylib` is not committed: `vendor/lib/` is filled from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), or
pointed at with `SPIKE_PDFIUM`. Blitz comes in through **path dependencies
into a clone beside this repository** (`../../../blitz`), because the Custom
Widget API this rests on is on `main` and only partly on crates.io. Move to a
git dependency when the next alpha lands.

**That clone is a build dependency and it is not in this repository**, so a
machine that does not have it gets `failed to load manifest for dependency
blitz-dom` and nothing else — which is the whole error, and it names a path
rather than saying what to do about it. Put it back with:

```sh
git clone https://github.com/DioxusLabs/blitz ~/rust_projects/blitz
```

The reader has since been moved onto that repository's **`main`**, which is
`c6dec888` at the time of writing, and it compiles and passes the whole suite
there with no change on either side. Two things worth knowing from doing it:
the API this rests on has not moved since `64eb2785`, and **all four of the
upstream faults in `tests/upstream.rs` are still faults** — every one of those
tests still passes, and they are written to pass while the bug is there and
fail the day it is fixed.

**A page is a `blitz_dom::Widget`, and it costs no frames when it is still.**
The older `<canvas src=…>` paint source set `has_canvas` on the document, which
made `is_animating()` true for ever: a steady 60fps at 52-62% of a core with
nobody touching the machine. `Widget` replaced it upstream and added
`requires_redraw()`, which a page answers `false` to. Measured on the same
binary: 2 paints in 5.4 seconds against 320 with `--animate`.

**But every widget in the document is painted every frame, on screen or not.**
`build_custom_widget_scenes` walks all of them. Twenty pages mounted meant
twenty drawn by pdfium and 265MB of texture. So `mount()` and `OVERSCAN` from
`viewer.ts` are load-bearing rather than free, and had to be ported before any
memory number meant anything.

**A shell of our own is required for a second window.**
`DioxusNativeApplication::add_window` does not do what its name says: it pushes
onto `BlitzApplication::pending_windows`, which is drained in
`can_create_surfaces()` and nowhere else, and the Dioxus half of the setup —
the contexts, `initial_build()` — is done by `launch` for its one window only.
A window added that way comes up empty and stays empty. `shell.rs` owns
`BlitzApplication` directly, whose fields are public, and its own comment has
the shape. Five things it cannot state, because they are about what happens
when you get them wrong:

- **Resuming is two steps** — `View::resume` starts the renderer and the
  renderer answers with `BlitzShellEvent::ResumeReady` — so a view must be in
  `inner.windows` before that event is drained, or the first frame never lands.
- **`blitz-shell` needs its `custom-widget` feature on** even though
  `dioxus-native` turns it on for blitz-dom and blitz-paint. Without it a
  dropped widget's resources are never unregistered. Nothing fails loudly;
  textures just leak.
- **The navigation and HTML parser providers are private** to `dioxus-native`
  and have to be restated (`nav.rs`, six lines), or `dangerous_inner_html`
  silently does nothing.
- **`use_window_event` is closed to a shell of our own**: it consumes an
  `Rc<WindowEventHandlers>` from a context only `DioxusNativeApplication`
  provides, and the type is private.
- **macOS still places a window wrong by exactly a title bar** (64 physical
  pixels of y). `set_outer_position` right after `View::init` fixes it — the
  same answer `Placements` in the app's `lib.rs` already gives.

**The chrome is recognisably the app's**, rebuilt from `styles.css` with the
unsupported properties taken out. The gap list and its workarounds are in
`dioxus-assessment.md`; two things the spike added to it. **Icons need their
colour baked into presentation attributes** — CSS does not reach inside an SVG
— *and* the `svg` feature has to be on, which it is not in a
`default-features = false` build; the two failures look identical, a toolbar of
blank icons. And **a `<canvas>`, or an `<object>` carrying a widget, must be
`display: block`** or it lays out at 0×0, which looks exactly like a blank
window with no cause.

`cargo run --bin probe` is what found that: it builds a document and reads it
back with no GPU and no window, and it answered the animation question and the
`nowrap` question in one run. It is the seed the Phase 2 harness grew from.

**The shader is right to one level in 255.** `recolorByPixel` from `themes.ts`
is ported twice, to Rust and to WGSL, and held to the tolerance
`recolor.test.mjs` already holds the app's two paths to. Two notes for whoever
touches it: `target` is a reserved word in WGSL, and the uniform block is two
`vec4`s rather than two `vec3`s and a float, because there is then one possible
layout rather than a std140 rule to be right about — the `vec3` version
compiled, ran, and silently read the flag as zero.

---

## Phase 1 — a reader that reads

`dioxus-reader/`. Open a document, scroll it, fit it, zoom it, theme it, with
the layout ported from `viewer.ts` rather than reinvented and the renderer
behind one trait from the first line.

**The layout port went exactly as advertised.** `relayout`, `rows`, `row_of`,
`first_box_ending_after`, `last_box_starting_above`, `mounted`, `page_at`,
`anchor` and `scroll_target` are `viewer.ts` line for line, with the comments
that explain *why* carried over with the lines. 250 lines of Rust with no
renderer, no widget and no window in it, and eleven tests — including the two
the app can only assert by opening a browser: that the binary searches agree
with a scan over the whole document, and that the mounting window holds every
page in the overscan band and no page outside it.

**The renderer trait held its shape.** `render.rs` names four things;
`pdfium.rs` is the only file that mentions pdfium; nothing else imports it.
That is the seam rule applied to the new tree, and it is what would make hayro
a decision rather than a rewrite.

**The per-page architecture is vindicated.** A page is drawn in 3.2ms,
uploaded in 4.7ms, and exists exactly once — one `wgpu::Texture`, no worker
copy, no `ArrayBuffer`, no canvas. Two pages are mounted at a time, so the
document contributes ~46MB and does not grow with the book. The bridge that
killed the pdfium prototype is genuinely absent: 43ms a page became 4.7ms of
`write_texture` in the same process.

**Recolouring is free.** A dark theme costs the same milliseconds and the same
megabytes as a light one, because the ramp is a compute pass over the page as
it is uploaded, and pdfium's BGRA becomes RGBA in the same pass — the CPU
swizzle Phase 0 measured at 1.6-5.1ms a page is gone with it. In the app this
is 835 lines of `themes.ts`, a blend-mode probe, a pixel fallback and a mask
allocator; here it is one WGSL function.

### Seven things about Blitz that cost time

**1. `MountedData` panics rather than failing.** `scroll`, `get_client_rect`
and `set_focus` all take `doc_mut()`, and every place a component can call one
from is already inside a borrow of the document: a DOM event handler runs
inside `EventDriver`'s borrow, a mounted handler inside
`flush_queued_mounted_events`'s. The result is `RefCell already borrowed` from
a stack naming neither. `NodeHandle::try_doc` exists and says as much in its
own doc comment — the safe method is the one a reader does not need.

*So the scroll offset is ours.* The viewer holds a number, the wheel moves it,
and the pages are placed against it — which is what `viewer.ts` does in all but
the last step anyway. What is lost is the scrollbar and the platform's fling:
a scrollbar we would have to draw, and momentum arrives from the trackpad in
the event stream regardless. It is the largest single thing given up.

**2. The viewport has to be asked for, not observed.** No `ResizeObserver`, no
`resize` event, and `get_client_rect` is the call above that panics. The window
size minus a stated chrome height is what the layout runs on.

**3. A key with nothing focused goes to `<html>`**, which is above anything a
component can put a handler on, and events bubble up rather than down. So the
reader's root takes focus when it mounts and one `onkeydown` is the app-level
handler `main.ts` has. Before that every key did nothing and nothing said why.

**4. A texture must not be registered and drawn in the same frame.** It works
until something else is unregistered in that frame too — exactly what happens
when the window's real size arrives and every page is replaced at once — and
then Vello panics with "tried to draw an invalid empty image" from the atlas
upload, on the third frame of every run. A page is registered on one frame and
drawn from the next, with the widget asking the shell for that next frame,
because `requires_redraw()` cannot: `is_animating()` is read at the *start* of
a frame, before the paint that would set the flag.

**5. A page's texture belongs to its node, not to its widget.** Unregistering
from inside `paint` is what item 4 forbids, and leaving a replaced texture
registered leaks a page for every zoom step. So what `keyFor()` carries — the
page, the size, the theme — is the *component key*: a change to any of them is
a different node, a new widget, a new texture, and the old node's resources
released by Blitz between frames, where it is safe.

**6. Keeping the page as drawn costs more than redrawing it.** Holding
pdfium's output beside the themed copy makes a theme change a compute pass
rather than a re-render, and costs 23MB a page — against 3.2ms to redraw. The
source is uploaded, read once by the compute pass, and dropped.

**7. Two `data-` attributes are the seam for state that has no pixels** —
where the reader is scrolled to, and which page each `.page` node is. The
mounting window is the most load-bearing thing in `layout.rs` and is otherwise
invisible from outside. Everything else a test asserts on is text somebody
could read off the screen, which is the better bar.

---

## The floor, and what was actually in it

Phase 1 measured a fixed ~110MB that an empty Blitz window cost before this
experiment wrote a line, and named the one finding that would end the
experiment: that the floor belongs to the stack and does not get better. **It
is the opposite. None of the floor belonged to Blitz.**

### Measure footprint, never RSS

`stats::rss_mb` shelled out to `ps -o rss`, and on macOS **a GPU buffer is
charged to a process's physical footprint and only partly to its resident
size.** The two disagree by a factor of three on exactly this workload. The
clearest case is the renderer choice, measured on an empty window with one
frame drawn:

| renderer | rss | footprint |
| --- | ---: | ---: |
| `vello` | 95MB | **208MB** |
| `vello_hybrid` | 85MB | **19MB** |

Eleven times, in the number that matters, and invisible in the one that was
being read — Phase 1 measured the two at 3% apart and concluded they were
equivalent. `stats.rs` reports both now and summarises on footprint, which
costs a few hundred milliseconds (`vmmap --summary` is the only thing that
answers without linking against mach) and so is read where a session ends and
never in a frame.

### The ablation

`dioxus-spike/src/bin/floor.rs` builds the stack a layer at a time, one
process per stage, because a stage cannot be unbuilt — a wgpu device that has
existed has already made its allocator's arenas.

| stage | footprint |
| --- | ---: |
| the process alone | 1.8MB |
| + a winit window | 15.7MB |
| + a wgpu instance, adapter and device | 16.4MB |
| + `vello`, resumed, one empty frame | **208.0MB** |
| + `vello_hybrid`, resumed, one empty frame | **18.8MB** |

**Nothing in Blitz, Stylo, Parley, fontique's system-font enumeration or winit
costs anything worth naming.** A window with a GPU device and a swapchain
behind it is 16MB. What costs 190MB is the first frame Vello draws, and it
costs the same whether the frame is empty or full.

`vello_encoding`'s `BufferSizes::new` is why: seven scene-independent
constants — `lines` and `segments` at 50.3MB each, `ptcl` at 33.6MB, `tiles`
and `seg_counts` at 16.8MB each, `blend_spill` 4.2MB, `bin_data` 1.0MB —
**173.0MB** in total, with a comment in the source saying they were "hand
picked to accommodate the vello test scenes as well as paris-30k" and *should*
be derived from the scene. `vmmap` puts 179MB of the `widget` spike's 227MB
under *Owned physical footprint (unmapped) (graphics)*.

**`vello_hybrid` is the default now** — not as a fallback for hardware without
compute, which is how the assessment lists it, but for memory. It allocates
none of the above, is upstream's own default, takes the same textures through
the same `try_register_custom_resource`, and draws pages correctly. `vello`
stays behind a cargo feature so the comparison stays runnable.

### And 96MB of it was ours

With Vello's scratch out of the picture the reader still sat at 240MB. `vmmap`
named it: six `MALLOC_LARGE` regions, 120MB, every one of them **`(empty)`** —
freed, and still charged, because macOS's allocator does not hand large blocks
straight back. 23.9M is exactly one page at this window size, and there were
three per page because:

```rust
let bitmap = page.render_with_config(&config)?;   // pdfium's own buffer
Ok(Bitmap { bgra: bitmap.as_raw_bytes().to_vec(), .. })
//                 ^ returns an owned Vec        ^ and copies it again
```

**`PdfBitmap::as_raw_bytes` is not a view.** It is `FPDFBitmap_GetBuffer_as_vec`,
which allocates and copies; the `.to_vec()` allocates and copies again. The
renderer now draws into a buffer it keeps — `PdfBitmap::from_bytes` wraps a
slice we own, so pdfium renders straight into it — and lends the bytes to a
callback for exactly as long as the upload takes. It is resized only when the
page size changes, so a document scrolled end to end allocates once. Worth
96MB and 3.2ms a page: Phase 1 reported 6.6ms and attributed it to pdfium;
half of it was memcpy.

*One thing that looked like the same fix and was not.* Keeping the source
texture and reusing it costs a permanent 24MB and changes the mid-scroll
figure not at all, because the pile during a scroll is the *themed* textures,
which wgpu cannot free until the submission that read them has retired. The
comment in `gpu.rs` says so, so it is not tried again.

### What is left in the 144MB

46MB of page textures (two mounted, and it does not grow with the book), 43MB
of swapchain (three IOSurfaces at 2200×1800, which is winit's and wgpu's
rather than ours), 24MB for the page buffer, 21MB of small allocations across
Rust, Stylo, Parley and pdfium, ~10MB of everything else. None of it is a
mystery and none of it is a fixed cost of the stack.

**Mid-scroll is the one number still worth chasing**: 60 screenfuls takes the
footprint to ~200MB and the peak to 390MB, settling within a second of
stopping. That is themed textures dropped and not yet retired. The fix, when
it is worth making, is what `viewer.ts` already has — a pool of page-sized
textures rather than a new one per page, which is `pageCache` and `discard()`
in a different register.

---

## Phase 2 — the harness

`src/harness.rs`, behind a `harness` feature that `cargo test` turns on and
`cargo build` leaves off. The release binary is 12MB either way.

```rust
use dioxus_reader::harness::{Options, Reader};

let mut reader = Reader::open(&Reader::book());
reader.press("j");                    // and "ArrowDown", "Home", " ", "+"
reader.wheel_screen();
reader.click_nth(".chip", 3);
let state = reader.state();           // page, pages, zoom, theme, notice, scroll, mounted
reader.screenshot().save("/tmp/page.png");
```

No window, no GPU, no compositor, no PDF worker in another process.
`Reader::open` is about 40ms and the whole reader suite is under half a second.

**Most of it is upstream's.** `blitz-test-harness` — which did not exist when
the assessment was written — builds a `DioxusDocument`, resolves style and
layout against a stated viewport, synthesises pointer, wheel, key and IME
events through the real event pipeline, and offers the DOM inspection the
assertions are written against. So Phase 2 was three things rather than a
harness from nothing: a reader to drive, a `state()` that reads the interface
the way somebody looking at it would (the page off the pill, the zoom off its
chip, the theme off the button that changes it — deliberately not out of the
`Viewer`, because it was the *wiring* that was broken both times something
was), and a `screenshot()`, which is the half the app's own harness never had.

Three things had to change in the reader, each smaller than the test it makes
possible and each an improvement on its own account: **a page can be drawn
without a GPU** (`Software` in `page.rs` runs pdfium's BGRA through
`recolor_cpu` — which is also the `vello_cpu` fallback the assessment's risk
table asks for, and a fallback whose pages are the one thing it cannot draw is
not a fallback); **the window is asked for a number, not for itself** (a
`Screen`, answered out of the real window by the shell and out of two numbers
by the harness); and the two `data-` attributes above.

### What it caught, on its first run

**A click on the theme button crashed the app.** A panic inside Stylo, from a
stack with nothing of this app in it, on a gesture anybody would make. Pressing
`t` for the same action was fine, which is why Phase 1 had not found it.

- A `<style>` element whose text changes is a **stylesheet mutation**, and
  Stylo answers one by walking the tree with `StylesheetInvalidationSet`.
- That walk calls `each_class` on any element **snapshot** it finds, and
  `ServoElementSnapshot::each_class` goes through `get_attr`, which is
  `self.attrs.as_ref().unwrap()`.
- Blitz takes a **state-only** snapshot for a hover or a press
  (`snapshot_node_state_only`, "cheaper … as it does not capture attributes"),
  and that snapshot has `attrs: None`.

So a click is two things at once — the pointer lands on a button, which is a
snapshot, and the handler rewrites the stylesheet — and the second walks over
the first. Two further conditions must hold, which is why the first three
attempts at a minimal reproduction all passed: the changed sheet must contain a
**class selector**, and some rule must depend on the **state bits**.
`tests/upstream.rs` is the twenty-line reproduction with both; it catches the
panic rather than letting it fly, so it *passes while the bug is there* and
fails the day it is fixed. Either side could fix it — Stylo's element-wrapper
path guards with `has_attrs()` and this path does not, and equally Blitz could
fill the attributes in. Against `stylo 0.20.0`, `blitz-dom 0.3.0-beta.2`.

**The reader no longer rewrites its stylesheet, and that is a better design
anyway.** The theme was interpolated into the sheet, so every change re-parsed
60 lines of CSS; it is now ten custom properties in the root's `style`
attribute, and a theme change re-resolves variables. An attribute change is a
snapshot that *does* carry attributes, so the crash cannot happen.

**`pdfium-render`'s `thread_safe` feature does not serialise anything.** It is
two `unsafe impl`s — `Send` and `Sync` for `Pdfium` — and a bound on the
bindings accessor. pdfium itself has process-wide state and no locking, so two
threads inside it abort the process: `SIGABRT`, exit 134, no panic, no message,
no stack. Invisible while there was one document on one thread; it arrived the
moment there was a test suite, because `cargo test` runs test functions in
parallel. `pdfium.rs` takes a process-wide lock in front of every call now —
the library's lock, not the document's, because a per-document lock is exactly
what was already there and exactly what does not help. It costs nothing
measurable and it is the thing to remember if pages are ever drawn off the main
thread.

### What is tested

| file | what it holds |
| --- | --- |
| `tests/reader.rs` | the interface: opening, the wheel, ten keys, the mounting window, fit and zoom, keeping your place through a zoom, the toolbar, spreads, a window of another size, the whole theme list, and settings surviving a restart |
| `tests/paint.rs` | the pixels: a page where the layout puts it, ink on it, a recolouring theme reaching the page and the chrome, the ink surviving the theme, the picture changing when you scroll |
| `tests/keys.rs` | the keyboard: chords, the table, `keys.toml`, a rebound key, and the dispatch — `tests/keys.test.mjs`, carried across with the port |
| `tests/sidebar.rs` | the panel: the contents listed and indented, a heading clicked and the one the reader is under, the column's mounting window, a thumbnail with ink on it, the document giving up exactly the panel's width, and a mark made, named, followed, taken off and remembered |
| `tests/search.rs` | the find bar: opening and closing it, what is typed reaching the scan, the match the reader lands on, stepping and wrapping, a highlight's rectangle on the page, the three switches, the results tab, a key typed into the field not driving the document, and one slice not reading a whole book |
| `tests/links.rs` | the links: where one is, where following it lands, the two ways a document writes a destination, an address handed to the system, a link that points nowhere; and the history, the labels and the page field |
| `tests/view.rs` | the margins measured off a sample and taken away, the page turned, a link that turns with it, and the two together |
| `tests/paged.rs` | one page at a time: what is laid out, what a page turn is, the ends of the document, and every chord in the keymap failing to leave the mode |
| `tests/library.rs` | where you were, kept and put back; the switch that turns it off; the page remembered in paged mode; what a document calls itself and when that is not a name; what was open, and what has been deleted |
| `tests/cost.rs` | the memory assertion |
| `tests/upstream.rs` | the four faults above, as the smallest thing that shows each |
| `tests/recolor.rs` | the shader against the reference |
| `src/layout.rs` | fourteen tests on the ported layout, three of them on the turn, the crop and where a rectangle lands under both |
| `src/theme.rs`, `src/settings.rs`, `src/keys.rs`, `src/library.rs` | thirty, and they are the app's own — see Phase 3 |
| `src/sidebar.rs` | four on the thumbnail column's geometry |
| `src/crop.rs` | seven: the ink box, the padding, the clamp, the refusals, and the sample |
| `src/search.rs` | eighteen: the fold, the origin map, whole words, the scan order, stepping, the cap, and the quads a match becomes |
| `src/store.rs`, `src/palette.rs` | the layer between them and the reader |

**And it is asked of the thumbnail column too**, in the same test rather than
a second one, because the counters are the process's and two tests running at
once would each be reading the other's pages.

**The memory test is a growth bound, not a ceiling.** What a process costs to
start depends on the machine, the allocator and how many fonts are installed,
and none of that is what a leak looks like. So: ten screenfuls to reach a
steady state, then forty more, and the footprint may not climb by more than
60MB across them — it climbs by zero. The regression it exists to catch is the
one that cost 96MB and went unnoticed through the whole of Phase 1.

**There are no reference PNGs, and that is a decision.** The rasteriser is
deterministic; the *fonts* are not. The toolbar is drawn in whatever
`ui-sans-serif` resolves to, which is a different file on a Mac, in a container
and on whatever a contributor is using, and pdfium's text rendering moves
between versions. A byte comparison would fail everywhere but here, and a
tolerance turns the test into "the picture is roughly the same shape". So the
pixel tests assert *measurable properties*: paper is paper and the ground
beside it is not, a band that should hold text is not uniform, a recolouring
theme moves the mean of the page below 80 while the light one leaves it above
200, the toolbar wears the theme's own paper to within a level or two. Each is
a sentence somebody could check by looking, which is the right bar for a test
that stands in for looking. Reference images become possible the day the
harness ships its own font.

---

## Phase 3 — parity, in progress

The assessment's order: themes and settings, then the keyboard, then the
sidebar, then search, then links and labels, spreads and trim, the library,
the watchers, multi-window, and markup last because that is where it stops
being a port.

### 1. Themes and settings — done, and the port was free

**`src/theme.rs` and `src/settings.rs` in this crate are
`src-tauri/src/theme.rs` and `src-tauri/src/settings.rs`, mounted by `#[path]`,
with no copy and nothing removed.** Everything they need is `atomic_write` and
each other. That is the assessment's central claim about the Rust side —
roughly 2,450 lines port with no change at all — tested rather than asserted,
and it came back stronger than the claim: the change needed was not "drop
`#[tauri::command]`" but nothing whatsoever, because neither file ever carried
one. Only `lib.rs` did.

A copy would have been the ordinary thing and it would have been wrong, for the
reason `AGENTS.md` gives about every other copy in this tree: the copy goes
stale, and a stale copy of a theme loader is invisible, because the file is
right and what is on screen is the copy. Mounting the files means the
experiment cannot drift, and the day one of them grows a Tauri dependency this
crate stops compiling and says which line did it.

**Their own tests come with them**, and `cargo test` runs seventeen of them
here unmodified: the settings write race, hand-edited files that cannot change
what a setting is, a later version's setting surviving a downgrade, a
hand-written theme deleted and edited under the name its author gave it, a
built-in that saves a copy rather than being overwritten. Those are not this
crate's tests and this crate did not write them; they are the port working.

`build.rs` is the app's, pointed at the app's `themes/` directory rather than a
second copy of the fourteen files — so the built-in set is the same directory
on both sides, and a shipped theme that will not parse or that names a colour
the renderer cannot read fails this build too, by file and field.

What is new here rather than ported:

- **`palette.rs`** — `applyTheme` and `parseColor` from `themes.ts`: strict
  hex, the derived shades, and `resolve`, which turns a theme's seven optional
  strings into the fixed set of colours the shader and the stylesheet use. The
  split is one `themes.ts` already makes and never named. `unreadable` names
  the fields at fault so a hand-written theme finds out, rather than silently
  rendering black on white.
- **`store.rs`** — the layer the reader talks to: which theme is in use,
  resolved, and a way to change a setting that writes it down. A group at a
  time, because settings almost never move alone — a theme with the light or
  dark slot it fills, a zoom with its fit mode. **A theme is remembered by id,
  never by position**, because the list changes when somebody adds a file to
  the directory; an id naming nothing falls back to the default rather than to
  nothing, which is what makes deleting the theme you are wearing survivable.
- **`config.rs`** — `atomic_write`, and a config directory that is
  deliberately **not the installed app's**: this crate rewrites every shipped
  theme on every run, and pointed at the same directory it would be editing
  the files of the app it is being compared against, very likely while that
  app is open. `HYLOPDF_CONFIG` overrides it, which is what the tests use.

**What this replaces is the whole bridge.** In the app the same work is
`api.ts` (898 lines), thirty-three commands, a browser twin of every one, and
`settings.test.mjs` existing solely because the settings table is written out
three times — Rust defaults, `fallbackDefaults`, the `Settings` type. Here a
component calls a method, the table is stated once in the file that already
stated it, and there is no second copy to drift. That test has nothing left to
check and does not exist here.

*One thing deliberately not done.* There is no settings *window* and no theme
editor — those are interface, and an interface is what the items after the
keyboard are for. The keyboard itself is done — item 2 below — and the settings
window is now the oldest thing outstanding on this list.

*The other thing on this line has since been done.* Writes happened on
whichever thread asked, which the app moved off the main thread because
`remember_position` fires on every pause in a scroll — and nothing here did
that until the library landed. It does now, and the answer is one thread for
that one write: see item 7.

### 2. The keyboard — done, and it took a file with it

The reader answered eleven keys through a `match` on `event.key()`. That is
the shape the app spent a rewrite getting out of, and it had already started
failing here in the way the app's version did: a modifier was something an arm
had to *remember* to check, so `+` answered ⌘+ and ⌥+ alike, and ⌘0 could not
be expressed at all. It is now the app's own table, and a chord is looked up
rather than matched.

**`src/keys.rs` is the app's, mounted like `theme.rs` and `settings.rs` beside
it, and `src/keymap.rs` is `keys.ts` in Rust.** That is the third module to go
across untouched and the most interesting of them, because *its other half is
TypeScript*. In the app the split is argued for at length — `keys.rs` owns the
file, `keys.ts` owns the meaning of a line, and validating in Rust as well
would have meant the same parser written twice — and the argument reads as
though the bridge is what forced it. It is not: with both halves in Rust and
no bridge between them, the same split is still the right one, and **nothing
about it had to change**. `keys.rs` compiles here with no edit, its five tests
run unmodified, and the template it installs is the app's `keys.toml`.

What the port cost: 700 lines of `keys.ts` became 640 of `keymap.rs`, and two
things got smaller on the way.

*`isMac` became a parameter.* In the app it is a module-level constant
imported from `api.ts`, so asking `parseChord` what it would say on Windows
means compiling the module a second time with the constant substituted — which
is what `tests/keys.test.mjs` does, and why the app carries
`HYLOPDF_PLATFORM=other` to lie to `navigator.platform` for the tests that
cannot. Here `mac` is an argument, both platforms are two calls, and the
environment variable has nothing left to do.

*The dispatch came out of the handler.* `wireKeyboard` decides what a
keystroke means while reading and writing four fields of the `App` object, so
the sequence logic — `g`, then what follows it — can only be tested by pressing
keys at a browser. `Keymap::resolve` is that logic as a function of the chords
and what is pending, and `a_pending_prefix_is_continued_dropped_or_used_on_its_own`
asks it directly.

**Every action in the app's table is carried, including the ones this reader
cannot do**, because the point is that `keys.toml` means the same thing on
both sides: a table missing half its rows would report the other half as
things HyloPDF cannot do, in the reader's own file. What is not built says so
on the notice line — "Print — handed to a program that prints is not built
yet" — which turns the
keyboard into a live list of what Phase 3 has left, and is a better answer than
silence to somebody pressing ⌘P.

Two actions are this experiment's and are in a list of their own so that the
app's table stays exactly the app's: `t` for the next theme and `s` for
spreads, both of which exist only because there are no menus yet. The test that
holds the table against the shipped `keys.toml` asserts they are *not* in it.

*One number moved.* Half a screen is now half of what a screen scrolls rather
than half of the window: the app's `scrollByViewport` keeps 60px of the old
screen on the new one, and `d` twice landing somewhere Space once does not is
the sort of thing a reader notices and cannot name.

*The harness grew two things, both small.* `Options.keys` writes a real
`keys.toml` into the reader's config directory before it opens, so what a test
exercises is the app's own loader reading a real file — `openApp({ keys: … })`
does the same against the browser twin. And `press_chord("mod+0")` presses a
chord written the way the binding under test is written, which keeps the
platform out of the test exactly as `MOD` does in the app's harness.

*What is still missing is the Keyboard page*, which is where the problems
belong: `keys.toml` is reported in one line at the bottom of the window here,
and the app shows the whole list beside the keys it is about. That is a
settings window, which is item 1's other half and waits on the same interface
work.

### 3. The sidebar — done, and it lost half its code on the way

The document's own table of contents, the pages the reader has pinned, and a
column of thumbnails. ⌘B opens it, ⌘⇧B marks the page — both the app's own
bindings, out of the table ported in item 2. Whether the panel is open is a
setting and the marks are the library, so both are there again next time.

**`sidebar.ts` is 699 lines and about half of them are memory management.**
`THUMB_CACHE`, `drawn`, `tasks`, `flights`, `trim()`, `forget(release)`,
`isVisible()` and an `IntersectionObserver` to drive them. `src/sidebar.rs` is
516 lines including its own tests and its comments, and none of that is in
it, because **the thumbnail cache is the mounting window**. A thumbnail in the app is a `<canvas>` that
lives as long as the column does, so drawing one is a commitment and the cap
is what bounds it; here it is a `PageWidget` on a node, so it lives as long as
the node does, and the node exists only while its row is in view. Scrolling
away gives the texture back through `Drop`. There is nothing to trim because
nothing accumulates.

That is the same rule `mount()` and `OVERSCAN` already apply to the document,
applied to the column — and it is not a saving so much as the only design
available, because of what Phase 0 found: every widget in the document is
painted every frame whether it is on screen or not. An unmounted row is not
tidiness, it is the difference between a column that costs nothing and one
that costs four hundred pdfium renders.

What the app buys with `THUMB_CACHE` is that scrolling back a little does not
redraw. Measured, that is not worth a cache here: a thumbnail is 1.2ms against
a page's 2.9ms at a fiftieth of the pixels, and the number that made the app's
cache necessary — a megabyte a canvas, nine hundred of them held for the life
of the document — cannot arise from a design where the picture belongs to the
row. **The whole panel costs 24MB**, and the memory test now scrolls the
column as well as the document, which is `AGENTS.md`'s own warning about where
a fourth leak would hide, answered.

*A thumbnail wears the theme for free*, which the app's could not: it is the
same widget reading the same `Chosen`, so the column and the page cannot
disagree about what theme is on. The app's `redrawVisible` had to cancel a
render in flight to avoid starting a second one into the same canvas, and had
a bug there for a long time.

**`library.rs` is the fourth of the app's modules mounted by `#[path]`**, and
it came across for the marks: a pin in a page has to be somewhere the next run
can read it. Eight more of the app's own tests run here unmodified. Only
`touch` and `toggle_mark` are called — where you were, what was open in each
window and the markup journal are the items of Phase 3 that are about those
things — and, as with the other three, nothing in it had to change.

*A mark is named for the section it falls in*, which is `sectionFor` in
`sidebar.ts` and is the reason the outline is read at open rather than when
the panel is first shown: "A section" is worth a great deal more than "Page 4"
to somebody looking at a list of their own marks a week later.

**The renderer seam grew its third and fourth questions**, both of which
`render.rs` had named and not declared since Phase 1: `outline()` and
`path()`. The outline comes out of pdfium's bookmark tree flattened into rows
with a depth, which is what `buildOutline` walks a tree to produce anyway. Two
things about that walk are worth keeping: `iter_direct_children` already walks
the sibling chain under a node, so following siblings *as well* lists every
entry but the first of its level twice — which is exactly what the first
version did; and a malformed document can point a bookmark at its own
ancestor, so the walk is capped at 20,000 rows and sixteen levels rather than
finding that out by running out of memory.

**And there is a fixture written in Rust now.** `src/fixture.rs` writes a
twelve-page document carrying a three-level table of contents, because the app
has no fixture with an outline in it and adding one to `make-pdf.mjs` would
mean `cargo test` needed Node — which is the opposite of what "run this suite
on three platforms" is asking for. `Reader::book()` still points at the app's
own 400-page fixture, deliberately: it is the document every number above was
taken on.

*One thing about the panel is a decision rather than a port.* Which tab is
showing is not remembered, because the app has no setting for it either and
inventing one would mean adding a key to `settings.rs` — the file this crate
*mounts* rather than edits. A document with no contents opens on the pages,
which is what `setDocument` does and is the difference between a panel and an
empty box.

*And the panel's third tab arrived with item 4*, which is why the tab list can
change: Results is there while the find bar is and gone when it is not, and
the panel has to be able to fall back to one of the other two.

**The edge can be dragged now, and it found a Blitz trap of its own.**
`.sidebar-resize` is a 6px strip absolutely positioned over the panel's right
border; picking it up sets `resize_from` on the `Viewer`, and `app.rs` puts the
matching `onmousemove`/`onmouseup` on the *root* rather than the handle,
because widening the panel carries the pointer out from under whatever
started the drag — root is the one ancestor a bubbling event cannot leave.
`drag_sidebar` is a no-op without a drag under way, which is what lets that
handler sit on every mouse move in the window without costing a render it did
not ask for.

The trap: the handle never received a single mousedown. `hit_inner` in
`blitz-dom` only checks a positioned descendant *ahead* of its parent's
normal-flow content when that descendant carries a non-zero `z-index` — that
is the sole test for `pos_z_hoisted_children`. Without one, an absolutely
positioned node is still hit-tested in plain DOM order alongside its normal
siblings, so `.panel` (later in the DOM, and just as wide) won the hit test
over the handle stacked visually on top of it. `z-index: 1` fixed it; `right:
-3px` centres the grab target on the border rather than beside it, which
matters more here than in a browser because there is no cursor-only affordance
to make up the difference. `tests/sidebar.rs` drags the handle through the
harness's real `mouse_down_at`/`move_mouse_to`/`mouse_up_at` and
checks the clamp at both ends and that the width survives being closed —
though not past the harness's own window edge, which is the same limit a real
window has without pointer capture: a drag that leaves the window stops being
tracked, in this app and in an ordinary web page alike.

**And the first working version flickered the whole document white while
dragging.** `sidebar_width` sits in `PageWidget`'s key beside the page and the
theme (see `page.rs`), so calling the ordinary `resize()` from `drag_sidebar`
on every `mousemove` — right, by the pattern everything else here follows —
meant a new key, and therefore a new widget with no texture yet, for every
mounted page on every frame of the drag: a fresh pdfium render and upload each
time, with nothing to show while either ran, which is `.page`'s own CSS
background (`#ffffff`, unthemed, because the paper colour is baked into the
bitmap by `recolor()` rather than declared in the stylesheet — see
`AGENTS.md`, "Recolouring is baked into the bitmap, not applied by CSS"). Fast
enough dragging read as the document flashing white regardless of theme.
`drag_sidebar` now moves only `sidebar_width` itself, which is a plain style
attribute `.sidebar` reads — the boundary line still tracks the pointer
exactly, because that is flexbox reacting to the width, not a relayout; the
document's own boxes are untouched until `finish_resize_sidebar` runs the one
relayout the drag deferred, on release. The same trick `toggle_sidebar` and
the settings write already used, just applied to the layout as well as the
write. `the_document_does_not_relayout_until_the_drag_ends` is the regression
test: the `.page` rect is unchanged through a mousedown and a move with the
button still down, and only changes once `mouse_up_at` lands.

### A click cost the reader its keyboard, and had since Phase 1

Found by the first test that pressed a key *after* clicking something, which
is the whole reason this item's tests are worth their length.

Blitz clears the focus when a click lands on nothing it knows how to focus —
it walks up from the target looking for a text input, a checkbox, a radio, a
summary, a label or a link, and a plain `<button>` is on none of those lists.
A key with nothing focused goes to `<html>`, which is above anything a
component can put a handler on. So from the first click on any chip, tab or
row, every shortcut in this reader did nothing at all.

**And the page cannot answer it.** `MountedData::set_focus` takes `doc_mut()`
the moment it is called, and every place a component can call it from is
already inside a borrow of the document — including a task spawned from one,
which is polled inside that same borrow. It panics with "RefCell already
borrowed" from a stack naming neither.

So the element that wants the keyboard says so — `data-keyboard` on the
reader's root — and whoever owns the window hands it back after a click:
`shell.rs` in the real app, and the harness for a window that does not exist,
in the same one line through the same function. The policy is that focus
landing *inside* the reader belongs to whatever took it, which is what makes
this survived the find bar's field arriving in item 4 — with one addition,
which is that the innermost element asking for the keyboard is the one that
gets it. `tests/upstream.rs` has
the twenty-line reproduction and `tests/reader.rs` the regression.

It is the third upstream fault this experiment has found and the second that
`shell.rs` pays for. What would end it is either blitz honouring `tabindex` in
that walk, which is what a browser does, or a `set_focus` that queues instead
of borrowing.

### 4. Search — done, and it is half the size it is in the app

⌘F opens a bar under the toolbar, typing searches, ⌘G and ⌘⇧G walk the
matches, Escape closes it, and the panel grows a third tab listing the results
with a line of the document either side of each. "Match case", "Whole words"
and "Highlight all" are the app's three switches and the app's three settings,
so they outlive the bar and the session.

**`search.ts` is 540 lines and `search.rs` is 600 before its tests, and the
difference is not Rust — it is that pdfium answers per character.** pdf.js
hands over *runs*, a string and a transform, and a run is not where a word is:
so the app joins the runs into one string, keeps a `starts[]` saying where each
began, binary-searches that to turn a match back into a run and an offset
inside it, hands the pair to the DOM as a `Range`, and measures the range
against a text layer of spans that exist only to be selected. `FPDFText_GetLooseCharBox` makes a match a
range of characters and a range of characters a list of rectangles, so
`items`, `starts`, `position()` and the text layer are all simply absent —
along with the four comments in `viewer.ts` explaining which way round each of
them goes.

**And a highlight is a node rather than pixels.** `paintSelection` and
`paintHighlights` copy the page canvas, run the copy through the luminance
ramp and lay it back over the line, because giving `::selection` a colour puts
pdf.js's text layer on screen and a page's bold type comes back regular, its
mathematics comes back as boxes, and every letter shifts. There is no text
layer here and nothing to put on screen: a match is a rectangle in PDF points,
so it is a `div` over the page in the theme's own selection colours and the
glyphs under it are the ones pdfium drew. `.hit` is two lines of CSS.

**What is ported exactly is `fold`**, because it is the app's most heavily
tested function and every line of it is a fact about typography rather than
about JavaScript: ligatures split, accents decomposed and their marks dropped,
soft hyphens taken out, case optional and the other three not offered as
choices because nobody types a soft hyphen on purpose. One thing came *out* on
the way — `search.ts` iterates its input by code point deliberately, with a
comment, because indexing a JavaScript string walks UTF-16 code units and
`normalize` on half a character does nothing, so a document set in
mathematical bold could not be searched with the letters on the keyboard. A
`Vec<char>` cannot be half a character and the bug cannot be written.

*The fold is also tested through the renderer and not only in isolation*, and
that found two things worth knowing, both in `fixture::prose_pdf`:

- **pdfium splits ligatures itself.** A `/fi` glyph named in a `/Differences`
  array comes back as "f" and "i" with a box each — with a `/ToUnicode` saying
  U+FB01 and without one alike. So on this renderer the ligature half of the
  fold has nothing to do, where on pdf.js it is the difference between finding
  "find" in a typeset book and not. It stays: it is the app's own tested
  behaviour, hayro will not do pdfium's normalising for us, and a document can
  carry U+FB01 by other routes.
- **A soft hyphen is only a soft hyphen if the document says so.** Written as
  the byte 0255 under WinAnsiEncoding it is an ordinary hyphen, because that
  is what the encoding says code 0255 *is* — so the fixture needs a
  `/ToUnicode` to produce the case the fold exists for. That took a probe to
  notice and would have made a passing test that proved nothing.

An accent, meanwhile, arrives precomposed, exactly as it does in the app, and
"resume" finds "résumé" because of the fold and for no other reason.

**The scan is sliced, and the reason has changed.** In the app the streaming
is there because pdf.js is slow *per page* and because every flush makes the
browser lay out the text layer again — a single letter in a long article turned
typing into a slideshow. Here a page costs 0.18ms in the 400-page fixture and
1.3ms in a 376-page book of typeset mathematics: 62ms and 498ms for the whole
document. A page is nothing; half a second is still half a second, and a window
that stops answering for it while somebody is typing is precisely what "fast
with no lags" is about. So there are still slices, at 8ms each, and
`one_slice_of_the_scan_does_not_read_the_whole_book` counts them.

*What a slice yields to is dioxus's scheduler, not a timer.* `breathe()` in
`search.ts` is `setTimeout(resolve, 0)`; here it is a future that wakes itself
and returns `Pending`, so `poll_tasks` puts it back in the queue, sees the
signal the slice just wrote, and hands the turn to whoever is driving the
document — the event loop in the real app, `pump()` in the harness. There is
no clock anywhere in it.

**The index costs about seventy bytes a character and goes when the bar
does** — 15MB for the 400-page fixture, measured. That is the app's own trade
in the app's own words, "a fair trade while the find bar is up and no trade at
all once it is closed", and it is settled the same way. Two notes for whoever
wants it smaller: the boxes are half of it and could be dropped for a page
with no match on it, and the footprint does not *fall* when the index is put
down — the allocator keeps the blocks and the next thing that needs memory
uses them, which is the same macOS behaviour Phase 1 spent a day on.

Measured on one machine in one sitting, the same document idle, before and
after: 225MB and 200MB. The search costs nothing when the bar is down, and the
difference between those two numbers is the machine rather than the change.
The binary went from 12,653,968 bytes to 12,819,488 — **162KB**, most of it
`unicode-normalization`'s NFKD tables, which is what a Rust binary pays for
what `String.prototype.normalize` gives a webview for free.

#### Two more Blitz traps, and both of them are about the find bar

**A focused text field takes a chord as typing, whatever is held down.** ⌘G
stepped to the next match *and* put a "g" in the query, which started a search
for something nobody typed — and because the answer arrived a slice later, it
looked like the search losing its results rather than like a keystroke going
to two places. Blitz dispatches `keydown` to the field and bubbles it to the
root, and then applies its own default action to the field regardless of the
modifiers. So the field's handler stops propagation for anything plain (or
"just" typed into it would scroll the document four times on the way) and
calls `prevent_default` for anything modified — except `a`, `c`, `v`, `x` and
`z`, which are what a text field owns. That is the same shape as the app's
find bar handing arrows and Home back to a focused field, arrived at from the
other direction.

**And hit-testing does not clip on overflow.** `.viewer` has `overflow:
hidden` and Blitz *paints* correctly — a page scrolled past the top is clipped,
which the screenshots show — but a page whose box starts at −2789px is still
hit-tested where its box says it is, which is over the toolbar and the find
bar. So clicking "Done" with the document scrolled at all landed on the page
behind it and did nothing, and clicking it at the top of a document worked
perfectly, which is the worst way round. Every row of the window that is not
the document now carries `position: relative` and a `z-index`, which is the
same trap and the same fix as `.sidebar-resize` in item 3, one level out.

*And the keyboard handback grew to cover keys.* `shell.rs` gave the focus back
to the reader after a click; a key can take the focused node away with it —
Escape closes the find bar and the field it was typed into stops existing —
after which every shortcut in the reader is dead again, which is the third
appearance of the same upstream fault. `give_keyboard_back` also picks the
*innermost* element that asks for it now, which is what lets the field keep
the keyboard while the bar is up and hands it back to the root when the bar
goes: one rule, two elements, and no state anywhere saying which.

### 5. Links, destinations, page labels and the go-to field — done

A cross-reference is a rectangle you can click, a citation's page number is a
number you can type, and both of them remember where you were so that ⌘[ comes
back. Four things that look like four features and are one: they are the
document's own account of where its parts are, and every one of them is a
*jump*.

**The renderer seam grew its fifth and sixth questions**, `links_of` and
`labels`, and both are what the assessment's rule asks for — small in, small
out. Three things about pdfium's answers.

*A destination arrives two ways and a document uses either.* Most links carry
a `/Dest` on the annotation; one written as a `/GoTo` action carries it under
`/A`. That is the same fork `read_outline` already had for bookmarks, which is
the sort of thing worth noticing: it is not a quirk of outlines, it is how the
format works, and a reader that follows one route finds half the links in the
wild. `fixture::links_pdf` writes one of each on purpose.

*Where on the page a destination means is one call, not six.*
`offsetWithin` in `viewer.ts` reads a raw destination array and switches on
`XYZ`, `FitH` and `FitBH` by name; pdfium's `FPDFDest_GetLocationInPage`
answers all seven view forms through `PdfDestinationViewSettings`, so the
whole of it is "is there a y, and is a y what this form means". The 0.95 clamp
is the app's and is kept for the app's reason: a destination at the very
bottom of a page scrolls that page out of the window, and the reader lands
looking at the next one with nothing to say why.

*And a link with neither an action nor a destination is dropped at the
renderer.* A `/Launch` naming a program, a `/JavaScript`, a `/Dest` that
resolves to no page: each is a hit area over printed words that does nothing
when it is clicked, which reads as the app being broken rather than as the
document being odd. `a_link_that_points_nowhere_is_not_a_link` asks the
renderer rather than the DOM, because it is a question about the document.

**A link is a node and nothing is drawn**, which is the second half of what
item 4 found about highlights. In the app, `tintLinks` bakes the link colour
into the bitmap *and* `renderLinks` lays real anchors over the top in
percentages of the page — two things, because a canvas cannot be clicked. Here
the colour is still the page's business (pdfium draws whatever the document
says, and `recolor` keeps a hue), and the clickable half is a `div` in points
multiplied by the page's own scale. What goes with it is the whole of the app's
percentage arithmetic and the comment explaining which fraction is of what:
the page's box *is* the render here, so points are one multiplication from
pixels and a fraction would be two.

*It is deliberately not an `<a href>`*, which is the app's own decision made
again for a different reason. There it is that an anchor carrying the address
navigates on a middle click, which never reaches the click handler, so the
webview left the app and took the document with it. Here an `href` would go
through `nav.rs` — the chrome's door, which knows only http, https and
mailto — and an internal link would find no scheme it allows and do nothing at
all. Both ways round, the rule is that one place decides what following a link
means.

**Where an address goes is a context, not a call.** `Away` is `Screen`'s
shape: the default is the system browser and is right in the app, and the
harness provides its own, so `a_link_out_of_the_document_is_handed_to_the_system`
asserts on the address without a browser window arriving on somebody's screen
halfway through `cargo test`. A `Viewer` has no browser in it and neither has a
test.

**The history is fifty places deep and only jumps go in it.** `jumpTo` in
`viewer.ts`, carried over with the distinction it exists for: scrolling,
turning a page and stepping through search results move *through* a document
and leave no trace; following a cross-reference, picking a chapter and typing
a page number move *across* it. `scrolling_is_not_a_jump` is that sentence as
a test. The one thing the app does that this had to be told to do as well is
that a jump landing where the reader already is is not a jump — otherwise
Escape from the page field, which re-runs the jump with the number already
there, fills the history with copies of one place.

**And the page field is a readout that becomes a field, which is where this
parts company with the app.** The app's is always an `<input>`. Here that
cannot work, and it is Blitz's focus rule that decides it rather than taste:
the keyboard is handed back to the innermost element asking for it, so a field
that is always in the toolbar either always asks — and then every keystroke in
the reader goes into it, and no shortcut works again — or stops asking while
still holding the focus, which is the same dead keyboard one level along. That
second one is what the first version did, and the test that caught it is the
one that presses `j` after Escape.

The find bar has neither problem because its field *stops existing* when the
bar closes and the focus goes with it. So the page field does the same: a
button showing the label, an input while it is being typed into, and
`onmounted` asking for the focus exactly as the find field does. It is the
fourth appearance of the same upstream fault and the first time the answer was
to change the interface rather than the shell.

*Two fields must never both ask.* The page field and the find field are
siblings rather than one inside the other, so "innermost" cannot separate them
and would settle it by document order — which would hand ⌥⌘G's field to the
find bar. The find field asks only while the page field is not up. One rule,
two elements, and no third state saying which.

**Labels are read at open and dropped if they say nothing.** A document that
numbers its pages 1 to n has said exactly what the position already said, and
carrying that list means every lookup runs for no reason — the app decides the
same thing in `readLabels`, and here it is decided in `pdfium.rs`, at the one
place that reads all of them. What a reader types is read as a label first and
a position second, which is the order that makes a number off an index find
what the index meant: in `fixture::links_pdf`, "3" is the label of page six
*and* the position of page three, and page six is what it opens.

*One consequence for the harness.* `state().page` used to come off a pill
reading "4 / 400" and now comes off the field, which for a document with
labels says "vii" and is not a number at all. That is the interface being
right: a reader in the front matter of a book is on page vii, and nowhere on
screen says it is also the seventh thing in the file. `state().label` is what
the field says, and is the assertion to write for a document that numbers its
own pages.

**One bug fixed on the way, and it was in the fixtures.** `contents_pdf`,
`prose_pdf` and now `links_pdf` each wrote a temporary file named for the
*process* and renamed it into place — and `cargo test` runs its tests as
threads of one process, so two tests wanting a fixture neither had yet wrote
the same temporary and both renamed it: the first won and the second failed
with `NotFound` on a file it had just written. It had never fired because no
two tests had ever raced for the same new fixture. One `written()` now, with a
counter in the name.

*What is still missing from this item*: nothing follows a link with the
keyboard, because there is no focus ring to walk and `tabindex` is not honoured
in Blitz's focus walk (the same fault as item 3's). The links carry
`role="link"` and a name saying where they go, which is what a screen reader
needs and what the app had to add for the same reason — a bare rectangle over
printed words has no text of its own, and a page of them otherwise reads as
"link, link, link".

### 6. Trimming the margins, turning the page, and one page at a time — done but for presenting

Three of the four things in this item. **Presenting is the fourth and is the
window's rather than the page's** — full screen with nothing on it — which is
the one category `PROGRESS.md` has said from Phase 2 cannot be tested here at
all, and it waits for the multi-window work of item 9 where the rest of the
window lives.

**Trimming is `measureCrop` and `inkBox` from `viewer.ts`, and the whole of the
machinery around them is gone.** In the app it is an `async` method with a
`cropping` generation counter, a check after every `await` that the document
has not been closed and the run has not been superseded, and a `void` call at
three sites — because eight page renders in a browser are eight trips through
pdf.js's worker and cannot be waited for. Here eight pages at a hundred and
sixty pixels wide is under five milliseconds in the same call, so a toggle
measures and lays out before it returns. There is no run to supersede and no
state to be stale. `src/crop.rs` is the module and it is 190 lines against the
app's 130 plus the counter threaded through the viewer.

The constants come across unchanged and so do the arguments for them: eight
pages sampled because the shapes that vary are the front matter, the plates
and the index; the union rather than a per-page crop, because a per-page crop
changes the scale from page to page and in continuous scrolling that is a
document that breathes as you read it; `INK` at 235, which is the same
threshold `WHITE_POINT` recolours by, so a hairline printed at 90% white is
paper to both; and never more than a third off any one side, because a page
whose margins measure wider than that is more likely to be a page this has
misread and the cost of being wrong is a reader who cannot see the top line.

*The switch is remembered and the measurement is not.* `trim_margins` is
already a key in the app's own `settings.rs`, which this crate mounts, so
there was nothing to add: a run that had it on measures *this* document rather
than putting back the last one's rectangle. And a document with nothing to
trim keeps the switch and says so, which is the difference between "off" and
"on, and there was nothing there" — the chip reads Trimmed either way and the
notice line is what tells them apart.

**Rotation is four lines of arithmetic and a rectangle that turns with it.**
The crop is a rectangle on the page as the reader sees it, so a quarter
clockwise takes `(x, y, w, h)` to `(1 − y − h, x, h, w)` — turning it is exact
and free, where measuring it again would be eight renders for an answer
already in hand. Nothing is written down: a rotation is a way of looking
rather than a property of the file, which is what `viewer.ts` says of it and
what Preview, Acrobat and Sumatra all do.

*And no cache is thrown away, which is where this parts company with the app.*
There `rotate()` clears the link cache, the note cache and the markup cache,
because all three hold **fractions of a turned page** — the app's link layer
is a DOM overlay sized in percentages, so it has to know the shape it is a
percentage of. Here every rectangle stays in the page's own unturned points
and one function does the turning where it is drawn: `Layout::place_on`, the
single place a link, a match or a mark meets the rotation and the crop. It
replaced three multiplications by a scale that were spelled out in three
places, so the port came out shorter than what it replaced *and* gained a
feature.

**Paged mode made the app's sparse-array trap into a type.** `AGENTS.md` calls
that array "a genuine trap — two binary searches, `trackCurrentPage`,
`pointAt` and `mount` all have to know about it — and every one of them did,
each with a comment saying why", and calls that "the correct amount of defence
for the shape". In Rust the shape defends itself: `boxes` is
`Vec<Option<PageBox>>`, `box_of` was already returning an `Option` for the
out-of-range case, and nothing can read a box without answering the question.
The five comments are still there, because the *reasoning* is what was paid
for, but none of them is load-bearing any more.

*There is no key for it and no chip, on purpose.* The brief calls continuous
scrolling a strong default that may only ever change if the reader explicitly
opts into it, and says a shortcut for it would be a thing to hit by accident.
So the whole interface is a line in `settings.toml`, exactly as the app has
it — and `tests/paged.rs` presses every chord in the keymap at a paged reader
to hold it to that, which is the only way to make a claim about a keymap
rather than about the twenty keys somebody thought of.

*What a page turn is, and it is not a scroll.* One page is laid out, so
arriving at a page starts at the top of it; scrolling past the bottom of a
page turns it, and past the top turns back **to the bottom of the page
arrived at**, because that is where the reader was reading. Everything that
moves a reader now goes through one door — `Viewer::go_to` — so the history,
a link, a heading, a typed page number and a search result all turn the page
in paged mode without any of them knowing which mode is on. That door did not
exist before this item and four call sites were each doing
`scroll_target` then `scroll_to` by hand.

**The harness grew one thing: `Options.settings`**, which writes a real
`settings.toml` through the app's own `set_many` before the reader opens.
`openApp({ settings })` is the same trick in the app's harness and it is here
for the same reason — some of what this reader does is deliberately not
reachable by pressing anything.

### 7. The library — done, and it cost one thread

Where you were in each document, what the document calls itself, and what was
open when the reader put it down. The marks came in item 3 and the history in
item 5; this is the rest of `library.rs`, and it is the fourth of the app's
modules to be used rather than the fourth to be ported — it has been mounted
since item 3 and needed no change for any of this either.

**A position moves sixty times a second, and that is the whole of what made
this interesting.** Everything else this reader remembers is chosen a few
times a session: a theme, a zoom, a fit mode, the panel. The scroll offset
changes on every wheel event, and every change is a read-modify-write of the
whole of `library.toml`. `AGENTS.md` describes exactly what that costs on the
thread drawing the window — "a whole-file rewrite of `library.toml` was
landing in the middle of the one gesture this app exists to make smooth" — and
`store.rs` had a comment since item 1 saying that when the library landed, this
is where the thread would go. It went there.

`Scribe` is one thread, asleep on a channel. `Store::remember` sends it a
place; it keeps the latest one *per document* and writes when nothing new has
arrived for 700ms, which is `onScroll`'s own `setTimeout` in `main.ts` turned
inside out. So a reader scrolling through a chapter costs one write at the end
of it rather than four hundred, and the cost on the thread that is scrolling is
a channel send.

*Per document, and that is not fastidiousness.* The thread is the process's and
`cargo test` runs its tests in parallel, so a single pending slot would have
one test's position quietly replacing another's — intermittently, by timing,
which is the worst shape a test failure comes in. There is one window here, so
in the real binary the map has one entry and the keying costs nothing.

*And `flush()` is the other half of the same design.* A place is written when
the scrolling stops, and quitting is the one way to stop scrolling that does not
wait — so `main.rs` flushes once the event loop has returned, which is the same
call `savePosition` is awaited for on the app's way out. It is also what a test
calls instead of sleeping: `tests/library.rs` opens a reader, scrolls it,
flushes, and opens a *second* reader over the same directory, which is what "the
next time you open it" means when there is no window to close.

**Restoring is held rather than done.** `Viewer::new` runs before anything is
mounted, so the layout has a viewport of 0×0 and every page in it is zero high:
a place turned into a scroll offset there is turned back into page one the
moment the window says how big it is. So `restore` keeps it as what it is — a
page and a fraction of it — and the first `resize` spends it, through `go_to`
rather than through `scroll_target` because in paged mode arriving at a page is
a relayout. `remember_place` says nothing while one is pending, or the
relayouts on the way to the first frame would record page one over the place
being restored. Both of those were bugs before they were comments.

**`2310.06825v3.pdf` is not a name.** The document usually knows better, so
`title()` is the renderer seam's seventh question and `worth_calling` is
`main.ts`'s own judgement ported: a title under four characters or over two
hundred, the file name over again, anything beginning "untitled" or "Microsoft
Word -", anything ending in a document suffix — each of those is *worse* than
the file name, because it looks deliberate. It decides what the toolbar says,
what the window is called and what goes in the library, and it is asked once,
at open.

*One thing the port improved by accident.* In the app this is `adoptDocumentTitle`,
an async method that runs after the document has been parsed and rewrites the
toolbar when it lands — because pdf.js cannot answer until then. pdfium answers
at open, so the title is settled before the first frame and the toolbar is never
briefly wrong. The `.title` slot in the toolbar used to say "400 pages", which
the pill beside it already said.

*The one deviation from the app is a rule kept rather than a rule broken.*
`/^untitled\b/i` rejects any title *beginning* with the word, so "Untitled
Letters" — a real book — falls back to its file name. Carried across as
written: "Untitled document" and "Untitled 1" are what producers actually emit,
the cost is a file name instead of a name, and a port that quietly disagrees
with the app about a judgement is the drift this whole experiment is arranged
to avoid. The test says so in as many words.

**What was open comes back, and a document that is gone does not.**
`set_open` is written at open — one path, because there is one window; the
file has been a list since the app had two, and `one_or_many` in `library.rs`
is what makes both readable. `store::reopening` is what `main.rs` asks before
there is a window to hold a `Store`, and it does the two things the app's
`bootstrap` does in the same order: `prune`, so a document that has been moved
or deleted is not reopened and failed on at every launch for ever, and
`reopen_last_document`, which is asked *there* rather than left to the caller
for the app's own reason — two sides that each assume the other checked it are
two sides that disagree about whether the window has anything in it.

*What is deliberately not built is the shelf.* `library.toml` holds
twenty-four recently-read documents with their titles and where you were in
each, and there is nowhere here to show them: the app has a start screen and
this reader always has a document open. Building one would be inventing an
interface rather than porting it. The list is kept and pruned correctly, which
is what the day there is a start screen needs.

*And the write is still a whole file.* `remember` re-reads and rewrites
`library.toml` for every position it records — which is the app's design and
costs nothing at one write per pause, and would be the first thing to look at
if the recents list ever grew a thumbnail.

### What is not built

No text layer, no selection, no markup, no settings window, no Keyboard page,
no watchers, one window — and of the library, everything but the markup
journal, which waits for the item that is about markup. Presenting is the one
part of item 6 still outstanding, and it is a window rather than a page.

---

## Three things to carry forward

1. **Write the test with the feature.** The harness is a quarter-second for
   ten tests. The excuse not to is gone.
2. **The CPU path is real code, not a test fixture.** A widget added in Phase 3
   that draws through wgpu needs its `Software` half, or the screenshots
   quietly stop covering it.
3. **Nothing here has run on Windows or Linux.** The harness is the first part
   of this experiment that *could* run on either without a screen — a CI job
   running `cargo test` on three platforms would exercise Stylo, Parley,
   fontique and the whole reader on engines this is not developed on, and none
   of it needs a GPU. That is a small job and it is not done. Full screen,
   window dragging, the traffic lights and multi-window are the window's rather
   than the page's and cannot be tested here at all, exactly as the app's own
   harness says of the same list.

## Five things worth raising upstream, and only the last is blocking

- `vello`'s `BufferSizes` sized from the scene rather than from paris-30k. The
  comment in the source already says it should be. A tenth of every one of
  those constants would do for a reader, and it is not a fault only a PDF
  reader has.
- `PdfBitmap::as_raw_bytes` named as the copy it is. A function that looks like
  a view and allocates 24MB is a trap anybody using `pdfium-render` for a
  reader will fall into.
- A click clearing the focus onto `<html>`, with no way for a component to take
  it back. Either half alone is defensible, but together they mean an
  application whose shortcuts live on its own root stops answering them the
  first time anybody clicks anything. See Phase 3 item 3 — and item 4, where
  the same fault turns up a third time because a key can destroy the node that
  had the focus, and item 5, where it turns up a fourth and decided the shape
  of the page field: an element that stops asking for the keyboard while still
  holding the focus is the same dead keyboard, and the only reliable way to
  make it let go is to stop existing. `tabindex` honoured in the focus walk,
  which is what a browser does, would answer all four.
- Hit-testing that does not clip on `overflow: hidden`, so a node scrolled far
  out of its container is still clickable where its box says it is, over
  whatever is drawn there. Painting gets this right; only the hit test does
  not. See Phase 3 item 4.

**And the blocking one is IME**, which is not a fault but an absence: there
are no composition events, so the find field this phase built cannot take
composed input and a reader writing CJK cannot search. It is the one item on
either of these lists that a decision has to be made about rather than worked
around, and the assessment says so too.
