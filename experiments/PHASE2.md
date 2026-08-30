# Phase 2: a harness, before the app grows

`dioxus-assessment.md` puts this before Phase 3 and gives the reason in one
sentence: *the alternative is writing 10,000 lines with no net and finding out
at the end.* `PHASE1.md` is the reader it drives and `FLOOR.md` is the memory
work that ends with "the harness needs a memory assertion". Both of those are
now true statements about the tree rather than plans.

```
cargo test                       # 31 tests, about nine seconds
cargo test -- --ignored          # the one that aborts the process on purpose
cargo run --release -- book.pdf  # the reader, unchanged
```

**The harness found two bugs on its first run, and one of them was a crash in
the shipping app.** That is the whole argument for the phase, made sooner than
expected — see "What it caught" below.

---

## What it is

`src/harness.rs`, behind a `harness` feature that `cargo test` turns on and
`cargo build` leaves off. The release binary is 12MB with the harness in the
tree and 12MB without it, which is the point of the feature.

```rust
use dioxus_reader::harness::{Options, Reader};

let mut reader = Reader::open(&Reader::book());
reader.press("j");                    // and "ArrowDown", "Home", " ", "+"
reader.wheel_screen();                // a screenful, the way a trackpad sends it
reader.click_nth(".chip", 3);         // the fourth chip in the toolbar
let state = reader.state();           // page, pages, zoom, theme, notice, scroll, mounted
let shot = reader.screenshot();       // the whole window, RGBA, rasterised on the CPU
shot.save("/tmp/page.png");
```

There is no window, no GPU, no compositor and no PDF worker in another
process. `Reader::open` is about 40ms and the whole reader suite is a quarter
of a second.

**Most of it is upstream's.** `blitz-test-harness` — which did not exist when
the assessment was written — builds a `DioxusDocument`, resolves style and
layout against a stated viewport, and synthesises pointer, wheel, key and IME
events through the real event pipeline. It also has the DOM inspection the
assertions are written against: `query_all`, `layout_rect`, `text_content`,
`hit`, `dom_string`. So Phase 2 turned out to be three things rather than a
harness from nothing:

*A reader to drive*, which is the document, the theme, the viewport and the
contexts the components expect, in one call.

*`state()`*, which reads the interface the way somebody looking at it would —
the page number off the pill, the zoom off its chip, the theme off the button
that changes it. It is deliberate that these come off the DOM rather than out
of the `Viewer`: a test that reaches past the interface cannot tell you the
interface is wired up, and it was the *wiring* that was broken both times
something was broken.

*`screenshot()`*, which is the half the app's own harness never had.

## The three things that had to change in the reader itself

Each of them is smaller than the test it makes possible, and each is an
improvement on its own account. That is worth saying plainly, because "the
tests made me change the code" is usually the opposite of a recommendation.

**1. A page can be drawn without a GPU.** `blitz_paint::paint_scene` runs every
custom widget's `paint` with the `RenderContext` the scene is being built for,
and a headless scene is being built for `vello_cpu`, where
`renderer_specific_context()` holds no `DeviceHandle`. Before this, `PageWidget`
printed a line to stderr and returned an empty scene — so a screenshot test
would have passed by photographing nothing.

`Software` in `page.rs` is the other path: pdfium's BGRA is swizzled and run
through `recolor_cpu` — the same reference implementation `recolor.rs` holds
the shader to — into a `peniko::ImageData` the widget fills with. It costs a
copy the GPU path does not, because an `ImageData` owns its bytes and the
reference ramp reads RGBA, and it re-renders on a theme change where the GPU
path re-runs a compute pass over the copy already uploaded.

It is also the thing the assessment's risk table asks for and nobody had built:
*"Vello unusable on common Linux hardware → ship `vello_hybrid` by default,
`vello_cpu` as fallback"*. A `vello_cpu` fallback whose pages are the one thing
it cannot draw is not a fallback.

**2. The window is asked for a number, not for itself.** `Reader` called
`use_window()` and took its viewport off the winit window. A headless test
cannot provide an `Arc<dyn winit::Window>` — it is thirty-odd methods about a
thing that does not exist — so the component asks a `Screen` for `(width,
height, scale)`, `shell.rs` answers it out of the real window, and the harness
answers it out of two numbers. A component reaching into winit is a component
that knows what it is running under, which the whole argument for narrow seams
says it should not.

**3. Two `data-` attributes.** Where the reader is scrolled to, and which page
each `.page` node is. Everything else `state()` reports is text somebody could
read off the screen; these two have no pixels of their own, and the mounting
window — the single most load-bearing thing in `layout.rs` — is otherwise
invisible from outside.

## What it caught

**A click on the theme button crashed the app.** Not a headless artefact: a
panic inside Stylo, from a stack with nothing of this app in it, on a gesture
anybody would make. Pressing `t` for the same action was fine, and that is why
it had not been found — Phase 1 checked the keyboard in the real app and the
button beside it takes a different path through style invalidation.

The mechanism, because it will catch somebody else:

- A `<style>` element whose text changes is a **stylesheet mutation**, and
  Stylo answers one by walking the tree with `StylesheetInvalidationSet`.
- That walk calls `each_class` on any element **snapshot** it finds on the way,
  and `ServoElementSnapshot::each_class` goes through `get_attr`, which is
  `self.attrs.as_ref().unwrap()`.
- Blitz takes a **state-only** snapshot for a hover or a press
  (`snapshot_node_state_only`, "cheaper … as it does not capture attributes"),
  and that snapshot has `attrs: None`.

So a click is two things at once — the pointer lands on a button, which is a
snapshot, and the handler rewrites the stylesheet — and the second walks over
the first. Two further conditions have to hold and neither is obvious, which is
why the first three attempts at a minimal reproduction all passed: the changed
sheet must contain a **class selector** (the walk skips the branch otherwise),
and some rule must depend on the **state bits** (Blitz does not snapshot
otherwise). `tests/upstream.rs` is the twenty-line reproduction with both, and
it catches the panic rather than letting it fly, so it *passes while the bug is
there* and fails the day it is fixed.

Either side could fix it: Stylo's element-wrapper path guards with
`has_attrs()` before reaching for them and this path does not, and equally
Blitz could fill the attributes in. Against `stylo 0.20.0` and `blitz-dom
0.3.0-beta.2`.

**The reader no longer rewrites its stylesheet, and that is a better design
anyway.** The theme was interpolated into the sheet, so every theme change
re-parsed 60 lines of CSS; it is now ten custom properties in the root's
`style` attribute, and a theme change re-resolves variables. An attribute
change is a snapshot that *does* carry attributes, so the crash cannot happen.
Stylo has had custom properties all along and Phase 1 chose interpolation to
make the derivation from five colours obvious — that argument is now in
`variables()` in `styles.rs`, where it reads better.

**`pdfium-render`'s `thread_safe` feature does not serialise anything.** It is
two `unsafe impl`s — `Send` and `Sync` for `Pdfium` — and a bound on the
bindings accessor. pdfium itself has process-wide state and no locking, so two
threads inside it abort the process: `SIGABRT`, exit 134, no panic, no message,
no stack, which is the C++ `CHECK` failure `FLOOR.md` describes from the
`page://` work.

It was invisible while there was one document on one thread. It arrived the
moment there was a test suite: `cargo test` runs test functions in parallel,
four of them opened four documents, and the binary vanished. `pdfium.rs` takes
a process-wide lock in front of every call now — the library's lock, not the
document's, because a per-document lock is exactly what was already there and
exactly what does not help. It costs nothing measurable (a page is 2.5ms and
the lock is uncontended) and it is the thing to remember if pages are ever
drawn off the main thread.

## What is tested

| file | what it holds |
| --- | --- |
| `tests/reader.rs` | the interface: opening, the wheel, ten keys, the mounting window, fit and zoom, keeping your place through a zoom, the toolbar, spreads, a window of another size |
| `tests/paint.rs` | the pixels: a page where the layout puts it, ink on it, a recolouring theme reaching the page and the chrome, the ink surviving the theme, the picture changing when you scroll |
| `tests/cost.rs` | the memory assertion `FLOOR.md` asked for |
| `tests/upstream.rs` | the two faults above, as the smallest thing that shows each |
| `tests/recolor.rs` | the shader against the reference (Phase 0's, unchanged) |
| `src/layout.rs` | eleven tests on the ported layout (Phase 1's, unchanged) |

Thirty-one tests, about nine seconds, no dev server and no browser.

**The memory test is a growth bound, not a ceiling.** What a process costs to
start depends on the machine, the allocator and how many fonts are installed,
and none of that is what a leak looks like. So: ten screenfuls to reach a
steady state, then forty more, and the footprint may not climb by more than
60MB across them — it climbs by zero, and the whole session sits at 30MB
because the CPU path holds two pages of `ImageData` and nothing else. The
regression it exists to catch is the one that cost 96MB and went unnoticed
through the whole of Phase 1: three copies of every page drawn, freed and not
handed back. It also asserts the counters directly, which is the cheaper half —
at most four pages mounted, holding under 40MB.

**There are no reference PNGs, and that is a decision.** The assessment expects
`screenshot()` to be compared against one, on the grounds that a software
rasteriser "produces the same pixels on every machine". The rasteriser does;
the *fonts* do not. The toolbar is drawn in whatever `ui-sans-serif` resolves
to, which is a different file on a Mac, in a container and on whatever a
contributor is using, and pdfium's own text rendering moves between versions.
A byte-comparison would therefore fail everywhere but here, and the usual
answer — a tolerance — turns the test into "the picture is roughly the same
shape", which is a slow way of asserting nothing.

So the pixel tests assert *measurable properties*: paper is paper and the
ground beside it is not, a band that should hold text is not uniform, a
recolouring theme moves the mean of the page below 80 while the light one
leaves it above 200, the toolbar wears the theme's own paper to within a level
or two, four screenfuls later the window is a different picture. Each of those
is a sentence somebody could check by looking, which is the right bar for a
test that stands in for looking. A reference-image test becomes possible the
day the harness ships its own font, and that is a Phase 3 note rather than a
gap here.

## What Phase 2 does not do, and why

*The assessment lists `search`, `keys`, `theme` and `settings` tests to port.*
Three of those four subsystems do not exist in this crate yet — they are Phase
3, items 1, 2 and 4. `theme.rs` has its test already, `layout.rs` has eleven,
and the rest port when the thing they test is written. What Phase 2 owed them
was the surface to port onto, and that is what `harness.rs` is.

*Full screen, window dragging, the traffic lights and multi-window* are the
window's rather than the page's and cannot be tested here, exactly as the app's
own harness says of the same list. Nothing has changed about that.

*Nothing here has run on Windows or Linux.* The harness is the first part of
this experiment that *could* run on either without a screen, which is the
interesting half — a CI job that runs `cargo test` on three platforms would
exercise Stylo, Parley, fontique and the whole reader on engines this is not
developed on, and none of it needs a GPU. That is a small job and it is not
done.

## Where this leaves the experiment

The gate was passed in `FLOOR.md` and nothing here moves it: the real app still
opens the same document at **144MB of footprint against Tauri's 373MB**, draws
a page in **2.5ms**, and ships as **12MB plus 7.2MB of pdfium**. What Phase 2
changes is the confidence with which the next ten thousand lines can be
written, and it has already paid for itself twice: a crash on a mouse click,
and a library whose thread-safety feature is a name.

Phase 3 is next, in the order the assessment gives — themes and settings
first, then the keyboard, then the sidebar. Three things to carry into it:

1. **Write the test with the feature.** The harness is cheap enough (a
   quarter-second for ten tests) that the excuse not to is gone.
2. **`data-` attributes are the seam for state that has no pixels.** Two so
   far. If a third is needed for something the interface *does* show, prefer
   asserting on what it shows.
3. **The CPU path is real code now, not a test fixture.** A widget added in
   Phase 3 that draws through wgpu needs its `Software` half, or the
   screenshots quietly stop covering it.
