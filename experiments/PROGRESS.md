# The Dioxus Native experiment: where it stands

`brief.md` is the ask and `dioxus-assessment.md` is the plan. This file is what
building it actually found — Phases 0 to 3 — and it is the only status file:
the four it replaces (`FINDINGS.md`, `PHASE1.md`, `FLOOR.md`, `PHASE2.md`) each
opened by correcting the one before, which is three quarters of a document to
read before reaching a true sentence. They are in git at `d2b0370` if the
working is ever wanted.

**The experiment is passing its gates and is not blocked on anything
upstream.** Two upstream faults were found and both are worked around in this
tree, with a test each that will fail the day they are fixed.

```
cd dioxus-reader
cargo run --release -- book.pdf              # read it
cargo run --release -- book.pdf --theme 4    # …in the fifth theme in the list
cargo run --release -- book.pdf --measure 60 # read it, and say what it cost
cargo run --release -- book.pdf --quit 5     # open, sit still, report, close
cargo test                                   # 55 tests, about nine seconds
cargo test -- --ignored                      # the one that aborts on purpose
```

Wheel scrolls. `j`/`k` and the arrows move a line, `d`/`u` half a screen, space
and Page Up/Down a screen, Home/End the ends, `n`/`p` a page, `+`/`-` zoom, `0`
fit width, `9` fit page, `s` spreads, `t` the next theme. The theme, the zoom,
the fit and the spread are still there the next time it opens.

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

The Tauri column is the installed app on the same document measured the same
way, summed over its four processes. **The assessment's Phase 1 gate — under
150MB against 346MB — is met**: 144MB against 373MB, a factor of 2.6, for
twice the binary. That is the trade the brief's goal 2 permits, and it is paid.

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
| `tests/cost.rs` | the memory assertion |
| `tests/upstream.rs` | the two faults above, as the smallest thing that shows each |
| `tests/recolor.rs` | the shader against the reference |
| `src/layout.rs` | eleven tests on the ported layout |
| `src/theme.rs`, `src/settings.rs` | seventeen, and they are the app's own — see Phase 3 |
| `src/store.rs`, `src/palette.rs` | the layer between them and the reader |

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

*Two things deliberately not done yet.* Writes happen on whichever thread
asked; the app moved these off the main thread because `remember_position`
fires on every pause in a scroll, and nothing here does that until the library
lands (item 7). And there is no settings *window* and no theme editor — those
are interface, and the next item is the one that makes an interface possible.

### What is not built

No sidebar, no search, no outline, no links, no text layer, no selection, no
markup, no settings window, no library, no watchers, one window. Two things in
the assessment's Phase 1 scope that are also still absent: `measureCrop` (trim
margins) and paged mode, both of which are layout and both of which the ported
`Layout` has room for.

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

## Two things worth raising upstream, neither blocking

- `vello`'s `BufferSizes` sized from the scene rather than from paris-30k. The
  comment in the source already says it should be. A tenth of every one of
  those constants would do for a reader, and it is not a fault only a PDF
  reader has.
- `PdfBitmap::as_raw_bytes` named as the copy it is. A function that looks like
  a view and allocates 24MB is a trap anybody using `pdfium-render` for a
  reader will fall into.
