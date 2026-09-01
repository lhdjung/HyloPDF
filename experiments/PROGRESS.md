# The Dioxus Native experiment: where it stands

`brief.md` is the ask and `dioxus-assessment.md` is the plan. This file is what
building it actually found — Phases 0 to 3 — and it is the only status file:
the four it replaces (`FINDINGS.md`, `PHASE1.md`, `FLOOR.md`, `PHASE2.md`) each
opened by correcting the one before, which is three quarters of a document to
read before reaching a true sentence. They are in git at `d2b0370` if the
working is ever wanted.

**The experiment is passing its gates and is no longer blocked on anything
upstream.** IME was the one item that needed a decision rather than a
workaround, and it is struck: composition events exist at the revision this
tree is pinned to, the find bar takes 日本語, and nothing in the reader had to
change for it — see "The platform work" near the end of this file. Five
upstream *faults* were found and all five are worked around here, with a test
each that will fail the day they are fixed.

## What "done" has meant here, and what it has not

Every item below is done in the sense its own entry claims, and the tests say
so. The list still does not add up to an app anybody would use instead of the
Tauri one, and the reason is the list. **It is the order the app was built in,
taken from the commit history — and that order is the engine.** Layout,
rendering, search, links, the library, windows, selection. The *interface* the
engine is reached through was never an item, so its absence never showed up as
an unfinished one: no menus, no way to open a second document, no settings
window, no Keyboard page. Progress against each item was real and progress
towards parity was being overstated by about the size of the thing nobody was
counting.

Two smaller versions of the same fault, both now fixed and both worth
recognising by shape. The toolbar's theme and fit chips *cycled*, because a
list needs a menu and there were no menus — so "themes work" was true and
choosing one was fourteen keystrokes. And dragging the sidebar deliberately
deferred the relayout, which was right for the document and wrong for the
thumbnails directly under the pointer; the entry for it argued the saving and
never asked which half the reader was looking at.

**The four grievances after those are the same shape again**, and they are
the clearest statement of it: a page wider than the window pinned to the left
of it with the rest unreachable, every undrawn page flashing white on a dark
theme, the page field emptying itself, and a toolbar that wore the same grey
under all fourteen themes. Every one of those was a *correct answer* — the
layout centred what fitted, the renderer drew the right pixels, the field took
the right number, the shade was derived from the theme — placed, coloured or
timed in a way nobody would sit in front of. No test asks that question, so
the only thing that finds them is reading with it. See "Four grievances from
reading with it" and the five after it, below.

**And the round that fixed those four did not make the reader look right**,
which is the sharpest version of the lesson so far. Three of the five that came
back had been sitting underneath the fixes, and one of them was the *cause* of
a fault the first round had answered somewhere else: the window's size never
reached the layout at all, so the centring that round added was correct and
never seen. A fix verified in a harness whose window is one size for the life
of the test is a fix verified against the wrong window. Reading with it is not
a step at the end; it is the only instrument that has ever found any of
these.

**So the measure to use is the app beside it, not this file.** What follows is
where that comparison actually stands.

```
cd dioxus-reader
cargo run --release                          # the 400-page fixture
cargo run --release -- ~/paper.pdf           # a document of your own
cargo run --release -- --theme 4             # …in the fifth theme in the list
cargo run --release -- --measure 60          # read it, and say what it cost
cargo run --release -- --quit 5              # open, sit still, report, close
cargo test                                   # 329 tests, about a minute and a half
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
⌘R and ⌘L turn the page a quarter each way. **Words can be swept with the
pointer** — a second click takes the word under it — and ⌘A takes the page,
⌘C copies what is selected, ⌘⇧C copies it with the page it came from.
`s` spreads
and `t` the next theme are this experiment's own and are not in the app, and so
is ⌘C: see item 10, where a key the app never had to write down is the clearest
thing found so far that leaving the webview costs. Any
of them can be rebound in `keys.toml`; a key bound to something not built yet
says so on the notice line. The theme, the zoom, the fit, the spread, the
sidebar, the trim, the marks and **the page you had got to** are all still
there the next time it opens.

**Two of the files it reads are watched.** A theme edited in an editor beside
the reader is worn as soon as it is saved, and a paper recompiled by LaTeX
underneath it is reopened at the page you were on. Both are the app's own
`watch.rs`, compiled here unchanged — see Phase 3 item 8.

**There are three menus in the toolbar**, off the document's name, the zoom
and the theme: open a document or a window, the fit and the spread and the
rotations, and all fourteen themes with the one in use ticked. ⌘O opens a
different document in this window and the first item of the document menu is
the same thing; "Open in a new window…" beside it is the app's own wording for
the app's own gesture. Each menu item shows the key that asks for the same
thing, read off the keymap rather than written beside it, so a rebound key is
what it shows. See "The menus, and opening a document" below.

**One thing is a setting and nothing else.** One page at a time is
`scroll_mode = "paged"` in `settings.toml`, with no key and no chip, which is
the brief's own instruction about it. Trimming the margins is the chip marked
Trim. See Phase 3 item 6.

**Every key in the app's table answers.** ⌘D is dark mode — the other half of
the pair the reader chose, not a default — and the machine's own light and
dark is followed until the reader says otherwise. ⌘P hands the document to a
program that prints and F1 is the Keyboard page. See "After Phase 3" below,
which is also where the `SIGSEGV` this file used to record as unexplained is
explained.

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
pointed at with `SPIKE_PDFIUM` — the reader reads `HYLO_PDFIUM` and falls back
to the spike's copy. Blitz comes in as a **git dependency pinned to
`c6dec888`**, because the Custom Widget API this rests on is on `main` and only
partly on crates.io.

**It used to be a path dependency into a clone beside this repository**
(`../../../blitz`), and that clone was a build dependency that is not in this
repository: a machine without it got `failed to load manifest for dependency
blitz-dom` and nothing else, which names a path rather than saying what to do
about it. This file called moving off that "when the next alpha lands", and
waiting was never going to work — `blitz-test-harness` is `publish = false` in
upstream's own manifest, so the harness the whole Phase 2 argument rests on can
never come from crates.io. A pinned revision is the answer instead, both crates
take it, and **a fresh checkout now builds with nothing beside it**. That is
what made the CI job below possible; it is the only thing that ever stood in
its way.

Two things worth knowing from the move onto `main` that preceded it: the API
this rests on has not moved since `64eb2785`, and **all five of the upstream
faults in `tests/upstream.rs` are still faults** — every one of those tests
still passes, and they are written to pass while the bug is there and fail the
day it is fixed.

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

**And a lock in front of every call is not a lock in front of every call**, as
a `SIGSEGV` a fortnight later showed: `FPDF_CloseDocument` is reached through
`PdfDocument`'s own `Drop`, which runs wherever the last owner dies and takes
no lock, and what it corrupts is a process-wide map in pdfium rather than the
document being closed. See "After Phase 3" below for the crash report that
names it and the four-line fix. The general form is worth keeping in front of
this paragraph: **a `Drop` is a call site, and it is the one call site that
does not appear at the place it happens.**

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
| `tests/watch.rs` | what changes on the disk: a theme edited and a theme deleted, a document recompiled and one that got shorter, news about somebody else's document, a rebuild that renames the paper — and one test with a real watcher and a real file behind it |
| `tests/windows.rs` | the window, asked for rather than made: a second one, closing against quitting, full screen and the way out of it, presenting taking the chrome and the panel and giving them back, and the toolbar naming its own way back |
| `tests/select.rs` | sweeping words: what a sweep covers and what it reads as, a sweep backwards and one across two pages, a click that is not a selection, a second click taking a word, the page turned under the pointer, ⌘A, ⌘C, ⌘⇧C, Escape's order, a recompile putting it down, and the cap on the pages of text kept |
| `tests/ime.rs` | composed input: a word from a candidate window reaching the field, one that is in the document being found, a preedit that is not searched for, the empty preedit before a commit, and a composition that does not drive the document |
| `tests/cost.rs` | the memory assertion |
| `tests/upstream.rs` | the five faults above, as the smallest thing that shows each |
| `tests/recolor.rs` | the shader against the reference |
| `src/layout.rs` | fourteen tests on the ported layout, three of them on the turn, the crop and where a rectangle lands under both |
| `src/theme.rs`, `src/settings.rs`, `src/keys.rs`, `src/library.rs`, `src/watch.rs` | forty-four, and they are the app's own — see Phase 3 |
| `src/sidebar.rs` | four on the thumbnail column's geometry |
| `src/stats.rs` | two on the two lines of `/proc/self/status` the memory test reads on Linux |
| `src/crop.rs` | seven: the ink box, the padding, the clamp, the refusals, and the sample |
| `src/windows.rs` | fourteen on the rules the app cannot test at all: which window a document goes to, what a window going means, and where the next one lands |
| `src/emit.rs` | four on the switchboard: news for one window, news for all of them, and a window that has gone |
| `src/search.rs` | eighteen: the fold, the origin map, whole words, the scan order, stepping, the cap, and the quads a match becomes |
| `src/select.rs` | ten on where a caret lands, what two of them cover, what a word is, and what a range reads as |
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
being a port. It came out as eleven rather than ten: markup needs something to
mark, and selecting words is a whole item — see item 10, which is the first
place in the port where the webview turned out to be doing something worth
having.

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

Three actions are this experiment's and are in a list of their own so that the
app's table stays exactly the app's: `t` for the next theme and `s` for
spreads, both of which were built because there were no menus, and ⌘C, which
is there because the selection is this reader's own rather than a webview's —
see item 10. The test that holds the table against the shipped `keys.toml`
asserts none of them is in it.

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

*The `#ffffff` in that paragraph was the other half of the same complaint and
is gone.* Deferring the relayout stopped the drag causing the flash; it left
every other re-key — a zoom step, a jump, a theme, a turn — causing it, and
those cannot be deferred, because the new size is the whole point of them.
`.page` is `var(--page)` now, which is the theme's paper under a recolouring
theme. See "Four grievances from reading with it" near the end of this file.

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

### 8. The watchers — done, and the app's file was mounted rather than ported

The themes directory and the open document are the two files this reader is
looking at that are not its own. A theme is TOML precisely so that somebody —
or something asked on their behalf — can open it in an editor, and until this
item an edit was seen at the next launch. A document is very often a paper
being recompiled by LaTeX under the reader, and reopening it by hand to see the
new draft is the same poor way round.

**`watch.rs` is the fifth of the app's modules to be mounted, and the
assessment said it would be the first to need a change.** It listed the file
under "survives nearly unchanged" with one line against it: "`emit_to(window,
…)` becomes an `EventLoopProxy::send_event`". That was nearly right, and the
"nearly" is the interesting half — the change is real and it is on *this* side
of the file rather than in it. Not one line of `watch.rs` was touched.

What made that possible is that the Tauri in it is two names. The file opens
`use tauri::{AppHandle, Emitter};`, calls `app.emit` twice, and is otherwise
about the disk from top to bottom: a change is a burst and not an event, a
theme reload is decided by comparing the themes rather than by something having
moved, a document is not believed until it ends the way a PDF ends, and the
watch goes on the *directory* because a compiler replaces a file by renaming
another one over it. So this crate supplies the two names. `extern crate self
as tauri;` in `lib.rs` puts the crate in its own extern prelude under that
name, `AppHandle` and `Emitter` are re-exported at its root, and `src/emit.rs`
is where they actually live — about a hundred and thirty lines, most of it
comment.

*The obvious version of the trick is a module called `tauri`, and it does not
work.* A `use` path anchored on a bare identifier is looked up in the extern
prelude and not among the crate root's modules; the compiler says so and helpfully
suggests `crate::tauri::…`, which is the one thing that cannot be written here,
because what is being avoided is editing the file.

*What the signatures cost.* They are Tauri's, because a shim taking a nicer
argument would be a shim the app's file does not compile against — which is
the whole of what is being tested. `Emitter::emit` therefore wants `S:
Serialize`, so the fourteen themes arrive as a `serde_json::Value` and are
deserialised on the other side. That is the bridge's serialisation surviving in
a build with no bridge, it is the only place in this crate that still does it,
and it happens when somebody saves a theme file. A `watch.rs` genuinely
rewritten for this crate would hand the vector over; this one is not rewritten,
and that is the point.

**Fourteen tests came with the file**, which is the fifth time that has
happened and is beginning to be the strongest single argument in this
experiment: the burst settling, a document that is only half written, a rewrite
of the same length inside one tick of a coarse clock, a window's own write
absorbed before it is reported, two windows sharing one folder. None of them
needed a line of change, and one of them — `a_second_window_reading_the_same_folder_keeps_the_watch`
— is about a situation this crate cannot have yet.

**The wire, on this side, is a waker.** The watcher is a thread and the reader
is a Dioxus task, so what is needed between them is not a channel but a way for
a thread to say "poll me" to a task it cannot see. `Post` in `emit.rs` is a
mailbox with a `Waker` in it, and the chain it starts already existed: waking
the task marks it ready, which wakes the virtual DOM, which — through the waker
`View::poll` builds out of the shell proxy — puts an event on the winit loop,
which polls the document. So a theme saved in an editor reaches the screen with
nothing anywhere polling a clock. In the harness the same wake marks the same
task ready and the next `pump()` runs it, which is why the whole feature can be
tested with no window and no thread.

That is the answer to a question the assessment left open and `PROGRESS.md`
has been keeping a list of: what owning the window costs. Here it cost nothing
— the shell needed no new event, and `Windows`/`Remote` were not touched.

**What the reader does about it.** `themes_changed` is `themesChanged` in
`main.ts`: the set is replaced, whatever is in use is put back on from the new
files, and nothing is written down, because nobody chose a theme and an editor
saving every few seconds must not be a rewrite of `settings.toml` every few
seconds. A theme whose file has *gone* hands the reader to another one — the
one remembered for that half of the light/dark pair, else anything of the same
darkness, else whatever is left — and *that* is written down, because it is a
choice made on the reader's behalf.

*A shipped theme cannot go, and finding that out took a test.* `load_all` falls
back to the copy embedded in the binary, so deleting `hylo-light.toml` deletes
nothing a reader can see — it is reinstated on the next run and answered from
memory in the meantime. The theme that can actually vanish is the one that only
ever existed as a file, which is the one this is for.

*One case the app has and this does not.* A theme being composed in the editor
window is the live theme and has no id, so every save in the directory reads as
"the theme you are reading in has been deleted"; `isEditingTheme()` is the
app's guard. There is no settings window here yet — item 1's other half — and
the line is marked in `themes_changed` for the day there is.

**`document_changed` is a reload that keeps the reader's place**, and where
they are comes off the layout rather than out of the library, for `main.ts`'s
reason: the library has the last position *written down*, and this is the one
moment the two can differ by a whole scroll. What goes is everything read out
of the old file — the outline, the labels, the links, the search index, the
crop — and everything that points into it, which is the history. What stays is
everything that is the reader's: the fit, the zoom, the spread, the rotation,
the panel, the theme. A draft that lost its last chapter lands on the end of
what is left, because `go_to` clamps and `Layout::replace_sizes` clamps
`current` — paged mode lays out `current` and nothing else, so a page number
past the end is a window with nothing in it.

*And it needed one new number.* A page keeps its texture for as long as its key
does not move — the page, the size, the theme, the view — and a recompile
changes none of those while changing every pixel. `generation` could not do it:
it is bumped by opening the sidebar, which must not throw a texture away. So
`edition` counts drafts, it is in the key, and Blitz releases the old textures
between frames the way it does for a zoom.

*A rebuilt paper may also call itself something else*, so the title is asked
again and `library::retitle` — the app's own, which writes only when there is a
difference — records it. That is the seventh question of the renderer seam
earning its keep a second time.

**Eight tests of the reader's half, and one of them has a file system in it.**
The other seven post the news through the mailbox the reader really listens on,
which is deterministic and is how the app's own suite tests this side. The
eighth deletes and rewrites a real theme file beside a reader with the real
watcher running, and it is the only test in this suite that waits on a clock —
because the thing being waited for is a clock. Two things about it are worth
keeping:

*The watch is set up on a thread and nothing says when it is up.* A save made
before that produces no event at all, and no amount of waiting afterwards
conjures one. So the test saves again — which is what an editor with the file
open does anyway — with a pause between saves longer than the watcher's own
settle window, or the burst never ends and nothing is ever reported.

*And a test that writes into `/tmp` on macOS must canonicalise it.* `/var` is a
symlink to `/private/var`; the file system reports the real path and the
watcher decides an event is about the themes directory by comparing the event's
parent with the directory it was given, so a directory named through the link
never matches and nothing is ever reported. Nothing a reader has is behind a
link. This cost an hour and is the sort of thing that only ever bites a test.

**One thing that is off in the harness on purpose: the watcher itself.**
`Watching` has no way to stop — the sender the notify callback holds keeps the
receiver alive, so the thread outlives the handle — which is nothing at one per
process and is a hundred threads and a hundred file-system watches on a `cargo
test`. `Config::watch` is therefore true in the binary and false in the
harness, and the one test that wants the real thing asks for it. The day
`watch.rs` learns to stop is the day that flag can go.

### 9. Two documents at once, and presenting — done

Everything in this item is the *window's* rather than the page's, which is why
it is last of the nine and why `PROGRESS.md` has been saying since Phase 2 that
this category cannot be tested here at all. Most of that turned out to be
wrong, and the reason is the most useful thing the item found.

**The app's window story is two things wearing one coat: rules, and windows.**
`AGENTS.md` is plain about the cost of that — "None of this can be tested in
the harness, which has no Rust behind it and no windows", followed by a list of
things somebody checked by hand in a running app: two documents handed over,
three windows restored from a session, ⌘N, a window closed, "New Window" from
the Dock. Every one of those is a *rule* — which window gets a document, what a
window going means, where the next one goes — and not one of them needs a
window to be true. They are untestable there because they are written against
`AppHandle`, `State<'_, …>` and `WebviewWindow`, so asking "what would happen
if a second file arrived now" means having an application running to ask.

So they are separated here. **`windows.rs` is `OpenDocuments`, `Placements`,
`Exiting` and the deciding half of `hand_over`, with every mention of a window
taken out**, and it has fourteen tests: that nothing is ever displaced, that a
document already open comes to the front rather than opening twice, that
quitting forgets nothing while closing a window forgets that window, that
closing the *last* window writes nothing because no flag can tell "finished
with this" from "goodbye" there, and that three windows closed one at a time
come back as the third alone. `session.rs` is the half that makes windows and
cannot be tested, and it is eighty lines.

**The reader's own side goes through one door and the door is written down.**
`Frame` is a context holding one closure — the shape `Screen` and `Away`
already had, for the same reason: the thing it stands for does not exist in a
test. A key asks for `Ask::NewWindow`, `Close`, `Quit` or `FullScreen(bool)`,
the shell answers each against winit, and the harness appends it to a list. So
"⌘N asks for a window", "⌘W and ⌘Q are not the same ask" and "Escape leaves
full screen" are tests — and the last of those is specifically called out in
`AGENTS.md` as a real-app check, because a browser in full screen keeps the
Escape key and the app's harness is a browser.

**What is left for the real app is one file, and it is `shell.rs`.**

#### What the port lost

Most of `spawn_window`, and each thing it lost says something about what the
app's version is actually for.

*There is no `Placements` map.* In the app a window is built with a position,
then *shown*, and showing it on macOS moves it onto the launch window's frame —
so the spot has to be remembered and applied again afterwards, and windows
still coming up have to be counted as taken or a restored session lands three
windows in one place. Here a window is made, positioned and drawn in one turn
of the main thread with nothing on screen in between, so the windows that exist
are the whole of what is taken and the cascade is a pure function of where they
are. `cascade` is the app's arithmetic exactly, including the bounded walk.

*There is no `Pending`, no `ready` and no "is the frontend listening yet".*
Those exist because a Tauri window and its interface are two processes that
have to shake hands: a document arriving before the webview reports in has to
be held somewhere. Here the virtual DOM is built before the window is on
screen, so a window is *made for* a document and there is nothing to hold.

*There is no `visible(false)` and no three-second safety net thread.* The app
hides a new window until its frontend says it has painted, so a dark theme
never flashes white, and spawns a thread to show it anyway if that never
happens. The equivalent here is that the theme is read during the first render,
before anything is painted at all.

*And there is no capability list.* `capabilities/default.json` naming
`["main", "reader-*"]` is a Tauri fact; a window outside it gets no permissions
and fails as a webview that never reports in.

#### The one thing that has no equivalent, and it is the start screen

`Desk::hand_over` has three answers — bring that window forward, fill an empty
window, make one — and **the middle one is unreachable in this reader**. The
app can have a window with nothing in it because it has something to show in
one; item 7 already recorded that this reader does not, because "there is
nowhere to show a recently-read list in a reader that always has a document
open". So a window is made for a document and never before one.

That decides ⌘N, which in the app is an empty window. Here it is **a second
window on what the front one is reading** — which is not a compromise: two
places in one book at once is a thing readers want, and the app's own "Open in
a new window…" is the picker version of the same gesture. The picker itself is
a door of its own (`rfd`, in the assessment's table); it was built with the
menus — see "The menus, and opening a document" — and ⌘N is unchanged by it,
because ⌘N was never the gesture that wanted one. The `Fill` arm is kept
because the rule is right and
because a window whose document failed to open is that case arriving by the
back door.

#### A mailbox became a switchboard, and `watch.rs` did not notice

Item 8 mounted the app's `watch.rs` unchanged by supplying the two names it
imports, and said the cost of one window was nothing. This is the bill.
`watch.rs` reports a rewritten document with `emit_to(label, …)` and the themes
with `emit`, so with more than one window the handle has to *route*:
`emit::Exchange` is a label-to-mailbox table, a window joins when it is made
and leaves when it is destroyed, and `emit` with no target goes to everybody.
Forty lines, four tests, and not one line of the app's file.

Leaving matters as much as joining: news for a window that has gone piles up in
a mailbox nobody reads, and that mailbox holds a `Waker` into a virtual DOM
that no longer exists.

The watcher itself is one per *process*, provided as a context — which is the
shape `watch.rs` already had, since `follow` counts what wants a directory
rather than unwatching it along with the document that named it. A watcher per
window would be that many watches on one themes directory and that many copies
of every theme reload.

#### The store was writing the open list and had to stop

`Store::opened` ended with `library::set_open(&dir, &[self.file])`, with a
comment saying "one path because there is one window". A `Store` is one
window's, so with three windows whichever rendered last wrote a list of one and
took the other two out of it. The rule that replaced it is the one the app has:
**whoever makes a window records what it shows** — `Session::window` in the
binary, and the harness for a reader that has no window at all.

#### Everything a window is asked to do is an event, even when it needn't be

Closing a window and putting one in full screen are both reached from a Dioxus
event handler, which runs inside `View::handle_winit_event` — inside a borrow
of the document and inside the shell's own borrow of the window map. Taking the
window out of that map from in there cannot be written. So every ask is posted
to the shell proxy and answered on the next turn, where nothing is borrowed: it
costs a frame nobody can see and it makes every window verb one shape. The Dock
menu, which in the app has to spawn a thread because it is invoked on the main
thread and `spawn_window` asks questions only the main thread can answer,
needed no special case at all — it posts the same event everything else does.

#### Presenting, and why the chrome is a method

Presenting is full screen with nothing else on it, and it is the last part of
item 6. The chrome is `TOOLBAR + NOTICE + HAIRLINE` and was a constant; it is
`Viewer::chrome()` now, because either of the first two can be taken away and a
subtraction at the call site only knows what was on screen when it was written.
⌘T puts the toolbar down and presenting puts everything down — but *not* the
notice line when only the toolbar goes, because the message saying how to get
the toolbar back is written on it, and it names whatever key `keys.toml` says
rather than a chord this file states. The panel is hidden rather than closed,
so stopping puts back what was open.

Full screen and presenting are two switches, not one: a reader who was in full
screen, presented, and then stopped is still in full screen, which is where
they were. Escape leaves them in the order the reader arrived — page field,
find bar, presenting, full screen.

#### One instance is a socket, and binding it is the claim

The app uses `tauri-plugin-single-instance` and needs it for a reason
`AGENTS.md` states: three double-clicked documents are three launches, and
three processes writing over each other's `settings.toml` is a race no lock
inside one of them can help with. `single.rs` is a Unix socket, and it is
thirty lines because the socket does both jobs — **binding it is the claim and
connecting to it is how the document gets across**. A lock file would need a
second channel beside it, and that channel would be this. It connects before it
looks, because a socket file proves nothing: a process killed outright leaves
one behind.

Two things it cannot do. Windows wants a named pipe and there is no std type
for one, so a second launch there is still a second process — the same state
this experiment has been in since Phase 0, and honest about. And **Apple Events
are out of reach, because there is no application bundle**: `RunEvent::Opened`
is how macOS tells an app that is already running to open a document, and
before any `NSApplicationDelegate` there has to be an `Info.plist` saying this
program opens PDFs. Every other route a document arrives by — a second launch
with an argument, "Open with" on Linux, the command line — is a launch with an
argument, and a launch with an argument comes through the socket.

#### What it cost to find: a crash that was not multi-window's fault

The first run of two windows died on the third frame with
`MissingTextureBinding(TextureId(4))` from inside Vello's atlas upload, about
two runs in three. It was not about two windows: **one window, made after the
event loop had started, dies exactly the same way** — which is a path that had
existed in `shell.rs` since Phase 0 and had never once been taken, because
until this item every window was made before the loop began.

The cause is the one `page.rs` already has a long comment about, one level
along. A page's texture is registered on one frame and drawn from the next
(`fresh`), because registering and drawing in the same frame breaks when
something else is unregistered in that frame — and something else is
unregistered whenever every page in the document is replaced at once. That
happened on every launch: the viewer was laid out at a default viewport and
corrected by `onmounted`, so the first frame drew every page at the wrong size
and the second re-keyed all of them. The `fresh` flag moved the collision one
frame along rather than removing it, and a window made late enough for its
frames to land differently found it again.

The fix is a line and it is worth more than the crash: **the viewer is sized
from the window before the first frame rather than on mount**. There is nothing
to re-key, so nothing is unregistered while something else is being registered
— and a full round of pdfium renders and texture uploads is no longer drawn and
thrown away on every launch. Five runs for five, both paths.

#### What was checked in the real app

A launch, a second launch on another document (the socket handed it over in
9ms, the window cascaded to exactly the position it asked for, and both entries
landed in `library.toml`), a third launch on a document already open (no window
made, no entry added, the window brought forward), a second launch with no
document at all (⌘N's own path: a second window on the same book, and
`set_open` deduplicating it to one entry), and a quit (`CloseRequested` for
every window, `tidy` for each, and the session list surviving, which is the
whole of what `Exiting` is for).

**What could not be checked is anything needing a keystroke.** Synthetic keys
through System Events reach nothing in this binary — it is not a bundle, and
`AGENTS.md` already records that driving the real app this way is unreliable —
so ⌘N, ⌘T, presenting and closing a window from the keyboard were exercised
through the harness and through the socket rather than through the keyboard.
The shell's own answers to those asks — `set_fullscreen`, `focus_window`,
`CloseRequested` — are one file and the last two are covered by the launches
above.

### 10. Selecting text — done, and it is the first thing the webview was doing for us

Markup needs something to mark, and this reader had nothing: `PROGRESS.md` has
said "no text layer, no selection" since Phase 1. So item 10 is selecting
words, and item 11 is what the plan calls item 10.

**`select.rs` is the file the app does not have, and the reason it does not is
that a webview comes with one.** In the app, selecting is the browser's: pdf.js
lays a text layer over every page — spans that exist to be selected rather than
seen — and `paintSelection` then spends a hundred lines undoing the damage,
because a `::selection` colour puts those spans on screen and a page's bold
type comes back regular, its mathematics as boxes, every letter shifted as its
line is stretched to the width the printer used. It copies the pixels under
each selected line off the page canvas, runs them through the same luminance
ramp that recolours a page, and lays them back down.

Here there is no text layer, so there is nothing to hide. pdfium answers per
character — `PageText` is characters and their boxes, indexed together — so a
selection is **two indices**, what it covers is a range of characters, and what
it looks like is `PageText::quads`, which the search has been drawing since
item 4. The glyphs under it are the ones pdfium drew, because nothing is drawn
over them but a translucent rectangle in the theme's own `selection_area`.
`select.rs` is 240 lines including its tests; `paintSelection`, `joinRuns` and
the text layer it was written against are rather more than that.

What that costs is what a text layer buys, and it is worth naming: **no
keyboard selection, no idea what a word is until `words_around` guesses, and
nothing about right-to-left or vertical text** — a selection here is a range of
indices in the document's own order, which is the order pdfium reports and
usually but not always the order somebody would sweep. The app inherits the
browser's answers to all three. This is the first place in the whole port where
the webview was doing something worth having.

#### The pointer is the one thing that arrives in the wrong space

Everything else in this reader starts life in the page's own unturned points —
a link's area, a match's quad, a character's cell — and goes *out* through
`place_on` once, on its way to the screen. A press starts on the screen and has
to come back the other way, through the same crop and the same rotation, which
is `Layout::unplace_on`: `place_on` inverted, rather than a search for the
rectangle the point is in. The search was the other way to write it and it has
no answer for most of a page — a character's box is eight points wide, and a
point in the gap between two words or in the leading between two lines is in
none of them. Inverting the transform gives every point an answer and leaves
*which character* to `caret_at`, which has the whole page in hand.

`caret_at` is a browser's rule and is the one nobody notices when it is right:
the nearest line, then the nearest character on it, then whichever side of that
character the point actually fell — so a click past the end of a line lands
after its last character rather than at the start of the next one, and a sweep
that runs off the bottom of the page selects to the end of it. Vertical
distance is weighted a thousand to one against horizontal, which is what makes
a sweep leaving the right edge carry on along the line rather than jumping to
whatever is directly below. `page_at_point` does the same thing one level up
and never returns "no page": a sweep into the gutter of a spread, into the grey
either side, or past the last page is still a sweep, so the nearest page is
chosen and the point is clamped into it.

**Where the content is, is worked out from the press itself.** A `MountedData`
call borrows the document and every place a component can call one from is
already inside a borrow of it — the same wall `Screen` exists for — so the
viewer cannot ask the DOM where it is. It does not have to: the press arrives
carrying both its client coordinates and its coordinates within the page it
landed on, and the layout knows where that page is, so subtracting gives the
origin. It is taken once per sweep and the scroll offset is added on every
move, which is what makes scrolling mid-sweep extend the selection through the
text that goes past rather than through the pixels the pointer is over.

Because the selection is characters and not rectangles, **it survives a zoom, a
turn, a trim and a spread with nothing recomputed** — the same property item 6
found for links and matches, arriving for free a third time.

#### A page never hears a click, and that is a fault worth having found

`onclick` and `ondoubleclick` on a page do nothing at all, ever. A page is a
custom widget, and `handle_dom_event` in `blitz-dom` forwards an event whose
target is a widget straight to the widget and then **returns**, before the
match that runs default actions — and `click` is the default action of
`pointerup`, `dblclick` the default action of `click`. Handlers still run,
because the handler phase is before the default action, which is why
`onmousedown` and the root's `onmouseup` work and why this took an hour to see:
the pointer is plainly reaching the node, and the two events that never arrive
are the two that would say a *gesture* happened.

The two it takes away are precisely the two a widget cannot generate for
itself. A click is not a pointerup — it is a press and a release on the same
node — and a double click is two of those within half a second and two pixels.
So `begin_sweep` counts the second press itself, with Blitz's own numbers, so
that a page and a text field in the same window answer a double click the same
way. It is the fifth entry in `tests/upstream.rs` and the fourth that runs with
the suite: a widget, a click, and an assertion that the handler heard nothing,
which will fail the day it is fixed.

#### Copying, and the key the app never needed

⌘C is not in the app's table because it is not the app's key: the webview owns
the selection, so it owns copying it, and `main.ts` reaches for the clipboard
only for ⌘⇧C — a quote with its page number attached, which is the one thing a
browser will not do for itself. Here the selection is the reader's own, so
plain copying has to be an action like everything else. `keymap::EXTRA` now has
three entries rather than two, and this one is different in kind from the other
two: `t` and `s` were built because there were no menus and would go away on a
merge, and `copy` would have to *join* the app's table. It is the clearest
thing this port has found that leaving the webview costs — a key nobody ever
had to write down.

⌘⇧C is the app's own format and its own reasoning, carried across: the page is
the one the selection *began* on rather than the one in the toolbar, because a
selection that runs across a page boundary began where it began. ⌘A is "select
the text of this page", which is the app's label and the app's decision — a
reader who means the whole document means a file.

`Clip` is a context holding one closure, which is `Away` and `Frame` for the
third time: the real one is the machine's clipboard through the shell provider
Blitz hands every window, and the harness provides its own. A suite that took
the real one would empty the clipboard of whoever is running `cargo test`,
which is a worse trespass than opening a browser window because it takes
something away rather than adding something.

**The clipboard costs 96 bytes.** `blitz-shell`'s `clipboard` feature was off —
the trait method is on `ShellProvider` either way and it is the *implementation*
that is behind the feature, so every copy would have silently returned `Err`
and the reader would have pasted whatever it had an hour ago. Turning it on
took the release binary from 13,054,096 to 13,054,192 bytes, which is arboard
reduced to a few calls into `NSPasteboard` by LTO. `file-dialog` is the other
half of that default set; it was turned on when ⌘O was built, and it costs
rather more — about 1MB, because `rfd` is a real dependency and not a few
calls into a system object.

#### What is tested, and how

Eighteen tests in `tests/select.rs`, and the interesting part is how they ask.
**What is selected is asked by copying it**, because that is the only way a
reader can find out too — the rectangles on the page carry no text, and a test
that reached into the viewer would be asserting on a field rather than on the
reader. So `selected()` presses ⌘C and reads the harness's clipboard, which
exercises the whole path every time: a sweep of three pointer events, the
caret arithmetic, the quote, and the door out.

The fixture is `prose_pdf`, six pages of one line each, whose text is a
constant in `fixture.rs` — so "the sweep covered the line" is an assertion
about the document rather than about whatever came back. A sweep backwards
covers the same words; a click is not a selection; a sweep below the line
reaches the end of it; a sweep from page one to page two selects on both; a
turn of the page selects what is under the pointer, which is the case that says
`unplace_on` really is `place_on` backwards; Escape puts the selection down
*after* the find bar and *before* presenting, which is the same "outward, in
the order the reader arrived" rule item 6 established. And a recompile puts
the selection down, because a selection is indices into a document and a paper
rebuilt by LaTeX is a different document — markup is the case where a passage
*does* survive a rebuild, and it survives as a quote to be looked up again
rather than as a range.

And the cache is asserted rather than assumed. `Viewer::texts` is the one cache
in `app.rs` that is bounded where the links beside it are not, because a page of
text is a `char` and a `Rect` per character — about thirty-six bytes each, so a
four-hundred-page book read end to end would be 40MB, a quarter of what this
whole reader costs, held for a feature nobody may have used. Eight pages,
oldest out first, and `stats::TEXT_PAGES` is what a test reads to say so.

#### What was checked in the real app

That it still starts, draws and quits, and that the shell provider really does
reach `Clip` — the one line no test covers, checked with a print and then taken
out again. **The clipboard itself was not exercised in the real app**, because
copying means writing to the machine's own clipboard and taking somebody's
clipboard away is not a thing to do without being asked. Everything above it —
the sweep, the caret, the quote, the page number — is exercised by the harness
against the real event path.

### The platform work, which was supposed to come after markup

`dioxus-fit.md` was written from the other side of the fence — against QRnew's
own migration, built the same week onto the same Blitz revision — and its
recommendation was to reverse the order this file had been working in: **do the
cheap platform work before the expensive feature work**, because the two
structural risks left are both about platforms and both were un-probed, while
the feature work remaining is large and well understood. Finishing markup on
macOS and *then* finding out the shell does not hold on Windows is the worst
available sequence. So markup waited, and this is what came first.

#### Blitz is a pinned git revision now, and a fresh checkout builds

Both crates, at `c6dec888`, which is the revision the clone was already sitting
on. The whole change is seven lines of `Cargo.toml` in the reader and six in
the spike; the lockfiles moved and nothing else did, and all 270 tests passed on
the other side of it without a rebuild of anything but the dependency graph.

The reason this was worth doing first is that it is the only thing that stood
between this tree and a machine other than this one. See Phase 0 above for what
it replaces.

#### And there is a CI job on three platforms

`.github/workflows/experiment.yml`. It is the third of the three things to
carry forward, which has been on that list since Phase 2 and is the item
`dioxus-fit.md` calls the highest information per hour left in the experiment:
`cargo test` needs no GPU, no screen and no compositor, so the whole reader —
Stylo, Parley, fontique, Taffy, the layout, the shader's CPU twin, pdfium and
every one of the app's five mounted modules — can be run on Windows and Linux
for the price of a runner. **Nothing in this experiment had ever run on
either.**

It is a workflow of its own rather than a job in `ci.yml` because the two share
nothing: that one is Node and Tauri and a webview. Four things it needs, and
each was a small discovery:

- **pdfium is downloaded per platform**, from the same `chromium/8021` release
  `vendor/lib` was filled from. The Windows archive keeps the DLL in `bin/` and
  its import library in `lib/`, so the directory `HYLO_PDFIUM` names is not the
  same on all three.
- **Linux needs `libfontconfig1-dev`**, because `yeslogic-fontconfig-sys` looks
  for fontconfig with pkg-config at build time and panics out of a build script
  without it — long before any test runs. Its own escape hatch,
  `RUST_FONTCONFIG_DLOPEN=1`, is not one here: it changes the crate's API
  surface enough that `fontique` stops compiling against it. That was found by
  trying it.
- **Node, for one file.** `Reader::book()` is the app's own 400-page fixture,
  which the app generates rather than commits, and `make-pdf.mjs` has no
  dependencies — no `npm ci`, one command. `src/fixture.rs`'s own documents are
  written in Rust precisely so that this is the *only* place the suite needs
  anything but cargo.
- **No `cargo fmt --check`**, unlike the app's Rust job, and deliberately: the
  keymap is one row per action so that it can be read against `keys.ts`, the
  macro above it is one line per arm, and rustfmt explodes both. Clippy runs,
  on one runner, and is clean.

What it cannot cover is the window — the shell, the cascade, full screen, the
Dock menu and the socket. Item 9 is what makes that a small hole rather than a
large one: the *rules* are in `windows.rs` with fourteen tests of their own, and
what is left needing a real window is the part that genuinely is a window.

**One thing was answered before the job ever ran.** `cargo check --all-targets
--target x86_64-pc-windows-msvc` compiles, from this machine, with the standard
library for the target and no linker: the `cfg(not(unix))` arm of `single.rs`,
`stats.rs` without `vmmap`, and everything under them. Linux cannot be
cross-checked the same way, because fontconfig wants a sysroot — which is the
entry above, and is why the runner is the answer.

#### IME exists, and it was never going to need a decision

This file has carried IME as **the** one item that needed a decision rather
than a workaround, on the strength of `dioxus-assessment.md`'s "IME does not
exist — no `compositionstart` / `update` / `end`". Both were right when they
were written and neither is now. `packages/blitz-dom/src/events/ime.rs` takes
the focused node's editor and applies the composition through Parley;
`blitz-shell` routes all four of winit's `WindowEvent::Ime` variants into it
and reports the cursor area back so the candidate window lands in the right
place.

What arrives is not a DOM `CompositionEvent` and never will be, and that is the
right answer rather than a shortfall: the DOM's composition events are a
*notification* that a composition is under way, and what a find bar wants is
the result. `Reader::compose` sends what an input method sends — a run of
preedits, then the word — and `tests/ime.rs` types 日本語 into the field by
composition, finds `résumé` in the document by composing it, and asserts that
the reader is taken to the page it is on. **Nothing in `app.rs` had to change**:
the find field is an ordinary `<input>` and a committed composition is an
`input` event like any other.

Two things came out of writing it that were not the point.

*A preedit is not a query, and here that is free.* Blitz answers a preedit with
a redraw and no `input` event, so the reader never searches for a half-typed
romaji. A browser **does** fire `input` mid-composition, with `isComposing` set
for the application to check — and `main.ts` does not check it, so the app runs
a scan of the whole document for every intermediate guess. That is the second
time the port has come out ahead by inheriting a stricter substrate.

*The empty preedit before a commit is winit's contract and not a nicety.*
Without it, the commit is inserted beside the composing region rather than in
place of it, and the field ends up holding にほん日本語 — which looks exactly
like a Blitz fault and is not one. `compose` sends it, and one test sends the
raw pair the other way round so that the contract is written down somewhere
that fails if it changes.

So: struck from the risk list, struck from "worth raising upstream", and struck
from the two documents that called it blocking. It cost an afternoon and five
tests.

#### The memory bound now binds on the platform CI runs on

`tests/cost.rs` is a growth bound, and `footprint_mb()` answered `0.0` anywhere
but macOS — so on the machine that will now run this on every push, the one
test written to catch the shape of regression that cost 96MB and went unnoticed
through the whole of Phase 1 was checking counters and nothing else.

Linux answers out of `/proc/self/status`: `VmRSS`, and `VmHWM` for the peak.
**That is RSS, in a file whose own heading says never to measure RSS**, and the
exception is exact rather than convenient. The rule is about a Mac and about
the GPU — a `wgpu` allocation is charged to the physical footprint and only
partly to the resident size, which is how `vello` and `vello_hybrid` were
measured at 3% apart when they are eleven times apart. Neither half holds here:
Linux has no separate footprint counter to prefer, and the one caller runs the
whole reader down the **CPU** path, where a page is an `ImageData` on the heap,
there is no device and no driver, and everything the process holds is resident
by construction.

The parsing is a function of a string with two tests on it, because the machine
this was written on cannot run the function that reads the file. Windows still
answers nothing: `GetProcessMemoryInfo` means a dependency for one number, and
the test already knows what to do with silence.

#### One thing seen once and not since — solved, later; see "After Phase 3"

**It was the first candidate below, and this section's own last paragraph
named the fix.** Left as written because the reasoning is what is worth
keeping: the crash came back a fortnight later, twice in six runs of the
suite, and macOS had written a crash report that ended the guessing in one
line. `FPDF_CloseDocument` from `Drop`, outside the lock, corrupting a
process-wide map in pdfium keyed by the document being closed. What follows is
what could honestly be said before that report existed.

A single `SIGSEGV` out of the test binary, on the first run after the IME tests
were written, with two of the five having passed. It has not come back in
twenty-seven runs of that binary and four of the whole suite, and a probe
written for the most likely cause — documents opened and dropped on eight
threads while others are read, which is the one thing pdfium's process-wide
lock does not cover, since `FPDF_CloseDocument` runs from `Drop` outside it —
did not reproduce it in twelve runs either.

It is recorded rather than fixed because the honest state is that it is not
understood. The other candidate is font fallback: those tests are the first
thing in this tree to shape Japanese, and first-time CJK fallback on several
threads at once goes through fontique and the platform's font machinery. If it
returns, it will most likely return on the CI job, which runs a cold process on
a cold machine every time — and the two places to look are a `Drop` for
`pdfium::Open` that takes `library()` before the document goes, and the font
context.

### The menus, and opening a document — done, and not on the list at all

Taken out of order for the reason at the top of this file: these were the two
things somebody comparing this with the app noticed first, and neither was an
item, so neither was ever going to be reached by working down the list.

**Three menus, in a layer of their own.** `Menu` in `app.rs` is which one is
down and the panels are a sibling of the toolbar rather than a child of it —
a menu inside a 46px row is a panel taller than its parent, and the layer is
out of the flow so the column above it is exactly what it was. What is in them
is the app's: the document's name carries open, open beside, a window and
close; the zoom carries the three fits, the three spreads and the two
rotations; the theme carries all fourteen. The chips still *say* what is in
force, which is what the harness reads off them and how a reader reads them
too — what changed is that clicking one shows the choices rather than stepping
to the next of them.

**Every menu item's chord is read off the keymap** (`Viewer::chord_for`),
never written beside the item. It is the reason the app's Keyboard page was
rewritten to be drawn from the keymap: a hand-written chord cannot show a
rebound one, and the table it replaced had already drifted. A reader who
unbinds ⌘O sees an item with no chord on it, which is true.

Three things about them are Blitz's rather than taste, and two are traps
already recorded one level away:

- *A menu needs a non-zero `z-index` to be **hit-tested*** ahead of what it is
  drawn over. Same fault as `.sidebar-resize`, and a menu that paints and
  cannot be clicked is worse than no menu.
- *A press inside a menu must not reach the root*, which is what dismisses it
  — and the three buttons a menu hangs off must stop propagation too, or the
  press closes the menu on the way down and the click opens it straight back
  up. Clicking an open menu's own button would then be the one gesture that
  did nothing.
- *Escape cannot be ordered from one place.* The app puts the whole dismissal
  order in one document-level capturing handler; here the keyboard belongs to
  the innermost element asking for it, so a field that has it has to defer to
  the menu itself. `Action::Dismiss` has the order for when no field has the
  keyboard, and the find field and the page field each check the menu first.

**⌘O opens a different document in this window**, which is what the app's ⌘O
does — `openDialog` calls `this.open(path)` — and ⇧⌘O is a menu item and not a
key there either. This is the one place where the port's "there is no empty
window" finding does *not* apply: ⌘N gives a second window on the document in
front because there is no start screen, and ⌘O was never about empty windows.
`Viewer::open_here` is `document_changed` plus the library entry, because a
recompile is the same document and this is a different one: the marks, the
title and the remembered place all move, and the fit, zoom, spread, rotation,
panel and theme stay, because those are settings.

**The picker is `blitz-shell`'s**, behind its `file-dialog` feature, which is
`rfd`. It costs 1MB — 12MB of binary to 13MB. It is reached through `Pick`, a
context holding one closure, for the reason `Clip` is one and one step further:
the real answer is a modal window belonging to the operating system, and a
suite that opened one would sit there until somebody clicked it. `tests/menus.rs`
answers with a path and tests everything downstream of the answer.

**Three things outside the window have to hear about a swap and none of them
is the window's**: the desk, which is what the restore list is read from; the
watch, which is following the file that was open a moment ago; and the
window's own title. `Ask::Showing { path, title }` is all three, answered by
`Shell::on_swap` and `Session::showing`. The name travels with the path
because the reader has already worked it out and asking pdfium again would
mean opening the file again.

**And the thumbnails follow the drag now.** The document's relayout is still
deferred to the end of a sidebar drag, and the entry that decided that
(`drag_sidebar`) was right about the document and never asked about the
column: a thumbnail is a twenty-fifth of a page in area and it is the thing
directly under the pointer. `relay_column` is live; the pages are not.

*Nine tests in `tests/menus.rs`, one more in `tests/sidebar.rs`.* Everything
here goes through Blitz's real event pipeline and real hit-testing, which is
what the harness is.

#### The one thing in this section a person still has to check

**The picker has never been seen to open.** `Pick`'s default calls
`ShellProvider::open_file_dialog`, and every test stubs it — correctly, since
the real one is a modal window. Driving the real app to check it did not work
either, and the reason is worth recording beside what `AGENTS.md` already says
about foreground testing: **plain keys sent by System Events reach this app and
modified chords do not.** `j` scrolled the document; ⌘O and ⌘B, sent the same
way with the window frontmost and clicked into, did nothing at all — the app
never saw them, which the diagnostic in `Pick` confirmed by never printing.
So: press ⌘O in a running reader by hand, once, before this is believed.

#### And a `MissingTextureBinding` seen once, on a cold binary

The launch immediately after a fresh `cargo build --release` died with
`MissingTextureBinding(TextureId(2))` — the fault item 9 recorded and fixed by
sizing the viewport from the window before the first frame. It has not come
back in nineteen launches since, including six with a focus change and a
screen capture thrown at the first seconds. The difference on the run that
died was that everything was cold: first render, first shader compile. That
would change the frame ordering the `fresh` flag depends on, which is exactly
the shape of the original fault. Recorded rather than fixed, because one
observation is not a diagnosis — and the place to look is `page.rs`'s `fresh`,
not anything added here.

### Four grievances from reading with it — done, and none of them was on the list either

The menus above came from the same place these did: reading with the thing.
All four are a correct answer badly placed, badly coloured, or out of reach,
which is the class of fault a suite that asks "does it work" will never raise.

**A page wider than the window was pinned to the left of it, with the rest
unreachable.** `#viewer` in the app is `overflow: auto` and `#pages` is
`margin: 0 auto` — a page narrower than the window is centred by the box model
and a wider one scrolls. Blitz has neither, and the pages here are placed
absolutely against a box `layout.rs` sizes, so both halves had to be
arithmetic and only the first half had been written. At 200% the page sat
twenty pixels from the left edge with a fifth of it off the screen and no
gesture that would reach it.

`Viewer::across` is the fix and the shape is the part worth keeping: **a
fraction, not an offset** — where the middle of the window sits across the
content, half by default. An offset would have to be recomputed at each of
the dozen places that relay the document out; a fraction is resolved against
whatever the content is now, so a page that has just become wider than the
window arrives with its middle in the middle, and a reader who zooms back out
to something that fits gets it centred rather than left where they had
panned. The other axis of a trackpad pans it, and ⇧-wheel with it — AppKit
turns the second into the first before winit sees either.

**Every undrawn page was white, whatever the theme.** `.page { background:
#ffffff }`, and a page whose texture has not arrived draws as that and nothing
else — so a zoom step, a jump, a theme change or a turn, all of which re-key
every mounted page, flashed white rectangles on a dark theme. `--page` is the
theme's paper under a recolouring theme and white under one that is not,
which is the colour the page is about to be. It does not make pdfium faster;
it makes the frame before pdfium answers the right colour, which is the whole
of what a reader was seeing.

**The page field opened empty.** The app selects the field's contents
(`el.pageNumber.select()`); this emptied it instead, and the entry for it said
that came to the same thing for anybody who then types. It does not — the
number vanishing is the reader losing the one thing the field was showing
them. There is no imperative door onto parley's selection (it will select all
when a keystroke asks and not otherwise), so it is emulated: the field opens
holding the page it is on, `page_fresh` is the "all of it is selected" state,
and the first thing typed replaces the lot.

*The interesting half is where that replacement had to go.* Cancelling the
keystroke and writing the character in through the `value` attribute works —
Blitz's `set_text` replaces the editor's string — but `set_text` does not
touch the *selection*, and a field just built has its caret at offset 0. So
the second digit landed in front of the first and "50" was typed as "05",
which parses to page 5 and passes every test written in one digit. Letting the
editor do its own insertion moves the caret; the replacement then happens in
`oninput`, where fresh means the caret was at the front and taking the old
label off the end of the new value leaves exactly what was typed. A click
inside the field ends the fresh state, which is both what a click into a
selected field does anywhere else and what keeps that arithmetic true.

And **a press anywhere else puts the field away**, which is the field's own
`blur` handler in `main.ts`. It had none, so the field held the keyboard until
Escape or Enter and a reader who clicked away was typing into something they
were no longer looking at.

**The toolbar wore a grey rather than the theme.** `--muted` was mixed halfway
between paper and ink, and halfway between any two colours is a mid-grey — so
Mark, Trim, the zoom and the two steppers came out very nearly the same under
all fourteen themes, which reads as the theme not having loaded. `--text-soft`
in `themes.ts` is `mix(text, bg, 0.26)`; `Palette::muted` is now that number
said from the other end, and `faint` with it. `tests/chrome.rs` asserts the
distance from the theme's own ink rather than a hex value, so it is a claim
about every theme rather than about one.

`tests/chrome.rs` is the nine tests for all of it.

### And five more, from reading with it again — four of which the first round had not touched

The round above fixed what it said it fixed and the reader still did not look
right, which is worth naming: **three of these five were sitting under the
fixes, and one of them was the *cause* of a fault the first round had answered
somewhere else.**

**The window's size never reached the layout.** The largest of them, and behind
two of the five complaints. Blitz answers `WindowEvent::SurfaceResized` by
moving its own viewport and asking for a redraw, and tells nobody — so the
chrome followed the window and `Viewer::layout` kept the viewport it was handed
when the window was mounted, at `onmounted`, once, for the life of the window.
A window opened at 1100 and dragged to 1600 laid its pages out for 1100 inside
a `.viewer` that was now 1600: the page centred in a `.pages` box narrower than
the window, which on screen is a page against the left of the screen, and Fit
width fitting a width the window no longer had.

**So the first round's centring was right and invisible.** `Viewer::across`
does centre the page, and every test of it passed, because a harness window
never changes size. Nothing in the suite had ever resized one — which is the
same shape as the menus and the four above: not a wrong answer, an answer
computed against something that had stopped being true.

The wire is the one every other piece of news uses. `Shell::on_resized` is
winit's half, `main.rs` turns it into an emit, and the `window-resized` arm in
the mailbox task refits from `Screen` — which in the app reads winit and in the
harness reads a `Cell` that `Reader::resize` sets. There is no `ResizeObserver`
here and `get_client_rect` cannot be called from inside an event, so news is
the only door. What is *not* measured is what a live drag costs: each step
re-keys every mounted page and that is a pdfium render each, the same as the
app's, and the placeholder is at least the theme's paper now rather than white.

**The panel's hairline was outside its width**, so the document was laid out
for a viewport one pixel wider than the box it was drawn into: every page flush
against the panel with its far edge a pixel over the window. `box-sizing:
border-box`, and it is the resize fault in miniature — the layout's idea of the
viewport and the viewport disagreeing.

**A menu came down at the end of the toolbar rather than under its button.**
The menus shipped as one layer pinned to the bar's two ends, on the reasoning
that a measured offset would need keeping in step by hand and there is no way
to ask an element where it is from here. Both halves of that were true and the
conclusion was wrong: an absolutely positioned child of a `position: relative`
wrapper needs no measurement, and it is the browser that keeps it in step. The
View menu — whose button sits between Trim and the theme — came down under the
page field, three chips to the right of what had been clicked. `.anchor` is the
wrapper; the panel is still out of the flow, so the 46px row is still 46px.

**`text-align` does nothing to a text input, and the page number sat against
the left wall of its box.** `create_text_editor` in Blitz copies the font size,
the line height and the brush into parley and stops — no alignment — and calls
`editor.set_width(None)`, so there is no box to align within either. Parley has
`set_alignment`; nothing calls it. Centring is therefore not available, so the
box is made to fit instead: `page_box` is the padding, the border and the
number, and the readout and the field take the same width so opening one moves
nothing. It is a workaround, and it is also the better answer — the number
never sits in a puddle of empty box.

*And the selection was invisible.* The emulated select-all from the round above
was real and nothing on screen said so, so the field opened looking like a
field somebody had clicked into and the first digit replacing all of it came as
a surprise. `.page-field.fresh` is the theme's own selection colours — the pair
a swept passage on the page is drawn in — and `--found-ink` is what that
needed. The platform's blue focus ring went with it, for the accent border the
app uses: it belonged to no theme here, and under Hylo Ember it was the one
cold thing on screen.

**The toolbar was still grey, and the accent was still the only colour in it.**
The round above set `--muted` to the app's own `--text-soft` and that was
correct and beside the point: every theme in this app names a near-monochrome
text colour — `#2f3237`, `#e9eaee`, `#f8f8f2` — so *any* shade of it is a grey,
in the app as much as here. What carries a theme in the bar is the accent, and
it was arriving as one bright word among the grey with nothing under it. So
`--accent-soft` (a fifth of the way from paper to accent) is the ground a chip
in force stands on, which is `.btn.on` in `styles.css` said exactly; and minus,
the readout and plus are one sunk `.zoom-group` rather than three more quiet
words in a row of quiet words.

`tests/chrome.rs` is fifteen tests now.

### The icons, which were the other half of the grey bar

Every button in the app's `index.html` carries a `data-icon` and none here did,
so the bar was a row of words where the app's is a row of small drawings with
words beside them. That is most of what "the toolbar is grey" was about, and no
amount of choosing a better grey answers it.

**Inline SVG works here, and not the way it looks like it works.** Blitz does
not lay an `<svg>` out as elements: `construct.rs` takes the subtree's
`outer_html`, injects an `xmlns` if it is missing, and hands the string to
usvg. That is why `dangerous_inner_html` is the right door — the paths from
`icons.ts` go in as the string they already are — and it is also why **an icon
cannot inherit its colour**. usvg parses its own document with no cascade
behind it, so `stroke="currentColor"` resolves to black on every theme, which
on Hylo Dark is an icon that is not there. The shade travels with the icon
instead: `Icon` takes a `stroke`, and `color` beside it, because two of these
drawings fill part of themselves with `currentColor` — the theme circle's dark
half and the cog's centre — and usvg resolves that against `color` and
otherwise against black.

The cost is the one thing a browser gives free: an icon does not follow its
label through `:hover`. It does follow the `on` state, because that is a state
the component knows about.

**`src/icons.rs` is a copy and `tests/icons.rs` is the gate.** The app's file
is TypeScript, so it cannot be mounted the way `theme.rs` and `settings.rs`
are; a copied *drawing* is exactly the kind of copy `AGENTS.md` warns about,
because both sides draw something and only one is ever looked at. The test
parses `src/icons.ts` and compares the fourteen shared names character for
character — the same trick `settings.test.mjs` plays on the settings table.
One icon is this reader's own and the test says so: `crop`, for the Trim chip,
which lives in the app's settings and has never needed a drawing there.

### The Settings window, which was item 1's other half

The oldest thing outstanding since Phase 3 began, and the last large piece of
interface — which is, per the top of this file, the category that was not on
the list in the first place.

**It is a window in the flow, not a window of the system's**, and the app is
the same: `showWindow` in `ui.ts` is a scrim and a frame in the same document.
That matters more here than there. A second winit window would be a second
`Viewer` over a second `Store`, and every setting changed in it would reach
the reader on its next launch — `AGENTS.md` describes exactly that staleness
between two reader windows, and it is tolerable between two documents and not
between a switch and the thing the switch is about. It also means the whole
window is testable in a harness that has no windows.

Five pages: **Reading** (progression, spread, the gap, trim, zoom, recolouring
pictures, and the three things that are remembered), **Appearance** (every
theme in the folder as a swatch of its three deciding colours, resolved
through `parseColor` first — a swatch that hands its raw string to the
renderer is the picker lying about the page), **Window**, **Keyboard**, and
**About**. `.field` is `ui.field`: the control on the name's line and the
sentence under both, which is most of why that window reads as prose rather
than as a form.

**The Keyboard page is drawn from the keymap and never from a list of its
own**, which is the app's own hard-won rule: its hand-written table had
already drifted, naming ⌘T twice and unable to know about a key the reader had
rebound. Every row is an action out of `keymap.rs` with whatever `keys.toml`
gave it, the file's complaints come first because a key that does nothing is
otherwise found out about by pressing it, and Reload is a button because that
directory is written to several times a minute while somebody is scrolling.

**The stepper is where Blitz charged for it.** A number that can be typed
needs two things this engine does not give: the caret starts at offset 0, so
Backspace does nothing and a typed digit goes *in front* — 20 with 3 typed
into it is 320, which clamps to the maximum — and there is no way to select
what is in a field. So the page field's emulation is here too: a `fresh` flag,
the first keystroke replacing the lot, the replacement done in `oninput`
where the editor has already moved the caret. What is new is that the field
holds its own text while it is being typed into, because a typed number is
clamped on the way out and echoing the clamped one back would rewrite the
editor under the caret. `set_text` is a no-op when the text already matches,
which is the whole reason the echo is free the rest of the time.

**And Escape had to be answered inside the field.** The keyboard goes to the
innermost element asking for it, and a stepper is that the moment the window
opens; every plain key has to be stopped there or it reaches the root and
scrolls the document behind the window — including the one key that closes the
thing the reader is looking at. That is the focus fault turning up a fifth
time. `tests/prefs.rs` is eight tests.

### 11. Markup — done, and it is the item where pdfium wins outright

The last item on the list, and the one the plan put last because it is where
the port stops being a port: everything above it is the app's behaviour said
in Rust, and this is a place where the two renderers disagree about what is
*possible*.

**A marked passage is a `/Subtype /Highlight` in the file**, with
`/QuadPoints`, `/C` and the appearance stream pdfium generates for it — the
specification's own annotation, the one Preview, Acrobat and Zotero read.
Sweep a sentence, let go, and six swatches come up under the line; press one
and the mark is in the document. `src/markup.rs` is 400 lines including its
doc comments and `tests/markup.rs` is sixteen tests.

#### Removal is one call, and in the app it is several hundred lines

`saveDocument()` in pdf.js writes an incremental update and no markup subtype
overrides `Annotation.save()`, so **an annotation already in the file cannot
be edited or deleted through it at all**. `AGENTS.md` says so in as many
words, and what the app does about it is the largest single piece of
machinery in the feature: keep a pristine copy of every document it has ever
written to, load it detached, replay every highlight but the one being
removed into it, work out which of them are the app's to replay and which are
baked into the backup already, refuse when the file carries markup neither
the backup nor the journal can account for — and a one-level byte-truncation
undo for the case even that cannot reach.

Here it is `FPDFPage_RemoveAnnot`, which `pdfium-render` wraps as
`delete_annotation`, and `markup::remove` is eleven lines. That is the whole
difference, and it is worth being precise about where it comes from: it is
not that pdfium is better software, it is that this reader *owns the
document* — pdfium hands over a mutable `PdfDocument` and a save, where
pdf.js hands over a read model and an annotation-storage side channel that
was built for its own editor.

**What pdfium charges for it, and it is a real charge.** `save_to_bytes` is
`FPDF_SaveAsCopy` with `flags = 0`, and `pdfium-render` does not expose the
flags — `FPDF_INCREMENTAL` is in its own bindings with a `TODO` above the
hard-coded zero. So where the app appends new objects and leaves every
original byte untouched, this re-serialises the document. For a paper that is
nothing; for a signed one it is the end of the signature, which is why the
reader is told once and asked rather than refused. `.hylopdf-original` is
kept beside the document the first time this reader ever writes into one —
the app's own file under the app's own name, kept here for the other reason.

#### Two faults found by writing it, and one of them was in the reader

**A highlight written with `PdfQuadPoints::from_rect` is invisible, and
round-trips perfectly.** The specification numbers a quad's corners
upper-left, upper-right, lower-left, lower-right, and pdfium reads them back
that way — `RectFromQuadPointsArray` takes its left and bottom from the third
point and its right and top from the second. `from_rect` instead *walks* the
rectangle: bottom-left, bottom-right, top-right, top-left. Written that way,
the annotation is in the file with the right colour, pdfium's own
`GenerateHighlightAP` builds it an appearance stream whose `/BBox` has its
left and right the same number, and nothing draws it. And `to_rect` takes the
minimum and maximum of the four points, so it undoes `from_rect` exactly: the
mark reads back correct through the library that wrote it, and only something
else opening the document ever finds out. That is why the test beside it
renders the page and looks at a pixel rather than re-reading the annotation.

**And red and blue were the wrong way round in every page this reader has
ever drawn.** `PdfRenderConfig::new()` turns `FPDF_REVERSE_BYTE_ORDER` on —
the crate's own source says why, and the reason is `image`'s `DynamicImage`
rather than anything about PDF — so a bitmap asked for as BGRA came back
RGBA. Both paths above it take pdfium at its word: the GPU uploads the buffer
as `Bgra8Unorm` and lets the sampler swizzle, and the software path swaps the
two channels by hand. Both were swapping an order that had already been
swapped.

It is invisible on almost everything this reader draws, which is why nine
phases of work and 313 tests never caught it: a page of black type on white
paper is the same picture either way, and so is every scan. The first thing
in the whole reader to put a *known* colour on a page is markup, and a
passage marked in `#ff0000` came back on screen as `#0000ff`. One line in
`pdfium.rs`, and it broke no test — which is the same evidence twice.

#### What the file cannot carry, and the journal that holds it

The journal is `library.toml` — the app's own file, through the app's own
`library.rs`, mounted here rather than copied, so a journal one of them
writes is a journal the other reads. The rule is `syncMarkup`'s: **everything
is thrown away and rebuilt from the file on every read**, and what survives
is only what a file cannot say.

*A document that cannot be written.* Asked of the disk before the gesture
rather than found out from a write that failed — `OpenOptions::new().write(true)`
is the only question whose answer is actually true, because permission bits,
a read-only volume, another owner and a sandbox all come back the same way.
The mark is kept beside the document with its quads and its quote, the row in
the panel says "beside the document", and the reader is told once.

*A document that was rebuilt.* A paper recompiled by LaTeX is a new file and
every annotation went with it — the case this whole app goes out of its way
to support everywhere else. So every mark in the document is written into the
journal with the passage it covers, and a reload that finds the annotations
gone finds the quotes still written down. "Put 1 passage back" appears in the
panel, looks each one up through `search::fold` — ligatures split, soft
hyphens dropped, whitespace flattened, because a passage that moved has very
often been re-set on the way — starting from the page it used to be on and
working outwards, and writes back what it finds. What it does not find is
left in the journal and counted out loud: a passage that was rewritten is not
a passage that moved. It is a button and never a thing that happens on its
own, which is the app's own sentence about the same button: re-anchoring is a
guess, however good a one.

*And a mark the reader took off is not a mark a rebuild lost.* Both are
"missing from the file", and telling them apart is a bug the app had and
fixed in exactly the same place: the journal is written **before** the file
is, so the reload cannot offer back what was just deliberately removed.

#### Three things this reader had to learn that the app never had to

**The file has to be let go of before it can be written.** pdfium reads a
page's objects when the page is asked for, so `FPDF_LoadDocument` keeps the
file open for the life of the document — the same lazy read the app gets out
of `read_range`, arrived at from the other end. On Windows nothing can rename
over a file held open that way, and nothing can truncate it either. So
`PageSource::release` exists: the write path lets go, writes, and reopens
through the path a recompile already uses, and the reopen is unconditional
because a released document draws nothing. The app needs none of this — the
handle it holds is Rust's own `File`, which shares writing and deletion, and
the bytes it writes came out of the worker rather than through that handle.

**A press on the swatches must not reach the page.** It would begin a sweep
of its own and put down the very selection it is there to mark. The app has
the same problem from the other side and answers it the other way round: in a
webview the browser collapses the selection before any handler runs, so there
is nothing to stop and `captureSelection` takes a copy on the way in. Here
the selection is the reader's own, so stopping the press is enough — which is
what the toolbar menus one layer up already do.

**A mark is drawn by pdfium, not by this reader.** Every other rectangle in
this app — a search hit, a selection, a link — is a node over the page,
because there is no text layer here and a rectangle is what those things
actually are. Markup is the exception and it is the right way round: a
highlight is *in the document*, `PdfRenderConfig` draws annotations by
default, and that means a document arriving with markup somebody else made
shows it too. The one cost is that a mark goes through the recolouring shader
with the page it is on, exactly as it does in the app.

#### What is not built, of markup

No underline, no strike-out, no squiggly: pdfium can write all three and they
are not offered, because a list that showed a mark this reader has no way to
make would be a list with a dead row in it. The app arrives at the same place
from the other side — `saveNewAnnotations` in `pdf.worker.mjs` has a case for
`HIGHLIGHT` and none for the other three. No area drag for scans, which is
the one thing `markup-assessment.md` still lists as unbuilt on both sides. No
note attached to a mark: `/Contents` on a highlight is a comment somebody
asked for, and the quote in the panel is read off the page instead — which
has the property that matters, in that it is right for markup this reader did
not make.

### What is not built

No theme editor. There is still no text *layer*, and there is not going to be
one: item 10 is what that was for.

**Phase 3 is complete.** Eleven items, and the last one came out ahead of the
thing it was porting.

---

## After Phase 3: dark mode, help, print — and the last empty arm

Three of the app's forty-three keyboard actions still answered "not built
yet", and the sentence was carried by a catch-all at the bottom of `perform`
that turned the keyboard into a live list of what was left. **The list is
empty and the catch-all is gone**, which is worth more than the three
features: an action added to `keymap.rs` and not handled in `app.rs` is now a
compile error rather than a sentence in the notice line.

The three were the three that are about something *outside* the document, and
that is why they were last rather than because they are hard.

### Dark mode, and the machine's own light and dark

The reader had fourteen themes, a menu to choose one from and a `t` that
cycled; what it did not have was the one gesture the brief asks for by name —
"dark mode that is easy to toggle (via UI or shortcuts)". Two settings for it
were already being written, because `Store::wear` has recorded `light_theme`
and `dark_theme` beside `theme` since Phase 3 item 1: which slot a theme fills
is read off its own paper, because that is the only thing that actually makes
a theme dark. Nothing read them back.

Now ⌘D does, and it moves between **the pair the reader chose** rather than
between two defaults — Sepia by day and Tokyo Night by night is the case the
two slots exist for, and it is a test.

**`follow_system_theme` was listed as needing a signal this reader does not
get. It does get one**: winit's `WindowEvent::ThemeChanged`, which macOS has
answered since winit 0.28, plus `Window::theme()` for the startup question.
Both go through `Appearance`, which is `Screen`'s sibling in every respect —
a context holding one closure, answered by the shell out of the real window
and by the harness out of a cell, because a component that asks winit what
the system appearance is cannot be built without winit and the harness has no
window. The event carries nothing: it says there is a new answer and the
reader asks, exactly as a resize does, so one place answers the question and
the startup path uses it too.

**The one place this is not a port is the shape of the answer.** The app's
`darkOutside()` is `matchMedia`, which always says light or dark because a
webview is a browser; `Window::theme()` is an `Option`, and the absence is
real. Read as "light" it would move every reader on a platform that does not
report an appearance to the light theme at every launch, and turn following
off the first time they chose a dark one. So `Store::outside` is
`Option<bool>` all the way down and every rule is written against `Some` —
which is a test of its own, because it is the half that is easy to write the
wrong way round.

Three rules came across unchanged and they are what make the switch feel like
a decision rather than a mode:

- Following is asked **before the first frame**, not after mounting, so a dark
  machine never sees a white page on the way in. It is the same call as the
  viewport question above it and for the same reason.
- Choosing a theme whose darkness disagrees with the machine **stops
  following**, and says so, and writes it down. Left following, the machine's
  next word would take the choice straight back off them. ⌘D is that same
  rule arriving through the same door, which is why `toggle_dark` goes
  *through* `set_theme` rather than around it.
- Choosing another theme of the darkness already in force says nothing about
  the machine and leaves the switch alone.

The two switches are on the Appearance page. The following one shows **the
setting, not the setting narrowed by whether it can do anything today** — a
control that reads back other than what is in the file is the picker lying
about the page, which is `AGENTS.md`'s rule about swatches one step along. The
sentence under it is where "this machine does not report an appearance" is
said.

One line more than the app's, deliberately: `other_half` checks that the slot
holds a theme of the darkness it is the slot *for*. A theme file whose paper
was edited from dark to light is still named by `dark_theme`, and the app
trusts the name — so ⌘D hands back a light theme, records it in the other
slot, and the pair repairs itself after having done nothing anybody could see.

### Help, and print

Help is the Keyboard page, which is the app's own answer and the reason that
page is a key at all: "Help" behind a cog is a strange place to keep the
answer to "what can this thing do". One line, now that the Settings window
exists.

Print prints nothing. It hands the document to a program that does —
`open -a Preview`, Edge by absolute path on Windows, `xdg-open` on Linux —
and the app's reasoning under those choices is the part worth having: the
point of *naming* a program is that it is **not us**, because the system's
default handler for a PDF may well be this reader, and handing a document to
ourselves to print it is a loop. It is a `Printer` context beside `Clip` and
`Pick`, for the reason those are contexts: `cargo test` must not be able to
open Preview on four hundred pages.

### And the `SIGSEGV` that was seen once is understood

`PROGRESS.md` has carried "seen once and not understood: a single `SIGSEGV`
from the test binary, not reproduced in thirty runs since", with two
candidates. It came back — twice in six full runs of the suite, in two
different test binaries, with no panic and no assertion — and this time macOS
had written a crash report, which named it outright:

```
__tree_remove(…)
CPDF_Document::~CPDF_Document()
FPDF_CloseDocument
<PdfDocument as Drop>::drop
drop_in_place<dioxus_reader::pdfium::Open>
drop_in_place<dioxus_reader::harness::Reader>
```

It was the first candidate, and the reason it took a report to see is that the
call is invisible in the source. **Nothing in this crate calls
`FPDF_CloseDocument`** — `PdfDocument`'s own `Drop` does, whenever the last
`Arc<dyn PageSource>` goes, on whatever thread that happens to be. Every other
call into pdfium in `pdfium.rs` is taken behind the process-wide lock, and
this one is not written down anywhere to be taken behind anything.

What it corrupts is not the document being closed. **pdfium keeps a
process-wide map of stock fonts keyed by `CPDF_Document*`**, and
`~CPDF_Document` erases its own entry from it; erase a node from a red-black
tree while another thread is inserting one and the tree is broken, after which
any thread that walks it dies. So the crash lands in a test that was opening a
document, caused by a test that was finishing one — which is exactly why it
looked random, moved between binaries, and would not reproduce alone.

The fix is an `impl Drop for Document` that takes the library lock and closes
the document inside it, which is `release()`'s one line again; the `Open` that
drops afterwards has nothing left to close. Eight consecutive clean runs of
the suite against two failures in the six before it — evidence rather than
proof, which is what a race allows.

The rule it leaves is general and worth carrying to anything wrapping a C
library behind a lock: **a `Drop` is a call site, and it is the one call site
that does not appear at the place it happens.** The second candidate in that
note — CJK font fallback on several threads — is not ruled out by this and is
also not needed to explain anything any more.

### And this one was checked in the real app, because it had to be

`Window::theme()` and `WindowEvent::ThemeChanged` are the window's, so the
harness proves the rules and proves nothing about the wire. Both halves were
run for real: the reader was left set to Sepia with following on, the machine
was in dark mode, and it launched wearing Hylo Dark — so the startup question
is answered before the first frame, and the answer is written down. Then the
machine was switched to light and back with the reader open, and it moved to
**Sepia** and back to Hylo Dark — not to Hylo Light, which is the pair doing
what the two slots are for. `follow_system_theme` stayed on throughout, which
is the other half: following the machine is not the reader overruling it.

`tests/prefs.rs` is 14 and `src/store.rs` has four of its own; 339 in total.

---

## Three things to carry forward

1. **Write the test with the feature.** The harness is a quarter-second for
   ten tests. The excuse not to is gone.
2. **The CPU path is real code, not a test fixture.** A widget added in Phase 3
   that draws through wgpu needs its `Software` half, or the screenshots
   quietly stop covering it.
3. **The CI job exists now, and until it has gone green nothing here has run
   on Windows or Linux.** `experiment.yml` runs `cargo test` on three runners,
   which needs no GPU and no screen and exercises Stylo, Parley, fontique,
   Taffy and the whole reader on engines this is not developed on. It was
   blocked on one thing — a path dependency into a clone — and that is gone.
   Window dragging and the traffic lights are the window's rather than the
   page's and cannot be tested there at all, though item 9 found that most of
   what was on this list *can* be, once the rules are separated from the
   windows they are about.

## Eight things worth raising upstream, and none of them is blocking

- `vello`'s `BufferSizes` sized from the scene rather than from paris-30k. The
  comment in the source already says it should be. A tenth of every one of
  those constants would do for a reader, and it is not a fault only a PDF
  reader has.
- `PdfBitmap::as_raw_bytes` named as the copy it is. A function that looks like
  a view and allocates 24MB is a trap anybody using `pdfium-render` for a
  reader will fall into.
- `PdfQuadPoints::from_rect` in `pdfium-render` numbering the corners the way
  the PDF specification does. It walks the rectangle — bottom-left,
  bottom-right, top-right, top-left — where 12.5.6.10 and pdfium's own
  `RectFromQuadPointsArray` want upper-left, upper-right, lower-left,
  lower-right. A text-markup annotation built with it is written, saved, read
  back correctly by the same crate, and drawn by nothing: the appearance
  stream pdfium generates has a `/BBox` of no width. It is `from_rect`, it is
  the obvious call, and its result is invisible from inside the library that
  made it. See Phase 3 item 11.
- `PdfDocument::save_to_writer` taking the flags. `FPDF_INCREMENTAL` and
  `FPDF_NO_INCREMENTAL` are both in `pdfium-render`'s own bindings and the
  flags word is hard-coded to zero, with a `TODO` above it from 2022 saying
  there is not a lot of information on what they do. There is one thing worth
  knowing about them and it decides a feature: an incremental save leaves the
  original bytes untouched, which is what a signature, a syncing folder and a
  document somebody else's software wrote all care about. It is one argument.
- `PdfRenderConfig::new()` defaulting `FPDF_REVERSE_BYTE_ORDER` to *on*. The
  crate's own comment says why — it is for `image`'s `DynamicImage` — and the
  cost is that a bitmap asked for as `BGRA` is not BGRA. Anything drawing into
  a buffer of its own, which is the whole point of `PdfBitmap::from_bytes`,
  gets a silent channel swap that is invisible on grey pages. Defaulting it
  off, or naming the format `RGBA` when it is set, would have made this
  impossible to get wrong. See Phase 3 item 11.
- A click clearing the focus onto `<html>`, with no way for a component to take
  it back. Either half alone is defensible, but together they mean an
  application whose shortcuts live on its own root stops answering them the
  first time anybody clicks anything. See Phase 3 item 3 — and item 4, where
  the same fault turns up a third time because a key can destroy the node that
  had the focus, and item 5, where it turns up a fourth and decided the shape
  of the page field: an element that stops asking for the keyboard while still
  holding the focus is the same dead keyboard, and the only reliable way to
  make it let go is to stop existing. `tabindex` honoured in the focus walk,
  which is what a browser does, would answer all four — and the fix has an
  address: `handle_click` in `packages/blitz-dom/src/events/pointer.rs` walks
  up from the target matching on `el.name.local` and clears the focus if the
  walk reaches the root unmatched, **without ever consulting `is_focussable()`**,
  which is `packages/blitz-dom/src/node/element.rs`'s own predicate, already
  honours `tabindex >= 0`, and is already used by `focus_next_node`. One call,
  in a file that has the answer in it.
- Hit-testing that does not clip on `overflow: hidden`, so a node scrolled far
  out of its container is still clickable where its box says it is, over
  whatever is drawn there. Painting gets this right; only the hit test does
  not. See Phase 3 item 4.
- A custom widget swallowing every default action, so `click` and `dblclick`
  never happen over one — `handle_dom_event` forwards the event to the widget
  and returns before the match that generates them. The two it takes away are
  exactly the two a widget cannot generate for itself, because a click is a
  press and a release on the same node rather than a pointerup. See Phase 3
  item 10.

**IME used to be an entry of its own and the only blocking one**, on the strength
of there being no composition events at all: a reader writing CJK could not
search. It is struck. Blitz applies a composition to the focused element's
editor through Parley and tells the application about the result, `blitz-shell`
routes all four of winit's IME variants into it, and `tests/ime.rs` types 日本語
into the find field and finds a composed word in a document. Nothing in this
reader had to change for it. See "The platform work" above — and note that what
arrives is not a `CompositionEvent` and does not need to be.
