# Phase 0: what the four spikes answered

**Every memory figure in this file is `ps -o rss`, which on macOS does not see
GPU memory.** `FLOOR.md` is the correction and it is large: the 112MB floor
below is 227MB by the measure that counts, 173MB of it is a constant in
`vello`'s buffer sizing rather than anything Blitz does, and through
`vello_hybrid` the same window is 18.8MB. Item 3 of "What this changes about
the plan" at the foot of this file asked for exactly that comparison; it was
made in `PHASE1.md` in the wrong unit and remade in `FLOOR.md` in the right one.

**Phase 1 has since been built and measured** — `dioxus-reader/`, written up in
`PHASE1.md` beside this file. Where the two disagree, that one is later and was
measured on a running reader rather than on a spike. The three things it
changes here: the mounting window and the LRU are ported and the per-page cost
is settled (25MB and 12ms a page, two pages mounted); `vello_hybrid` was
measured beside `vello` and is within 3% of it, which closes item 3 below; and
the 112MB floor this file could not explain is now 105-111MB measured in
release on the same two binaries, and is the number the whole proposal turns
on.

`dioxus-assessment.md` names four questions that can kill the Dioxus Native
experiment, and says to answer them in a scratch crate before writing anything
that looks like the app. This is that crate, and these are the answers.

**Everything below was re-run on Blitz `main`.** The first pass was built on
the published `dioxus-native 0.7.10` / `blitz-shell 0.2.3`, and its headline
finding — that a page on the screen makes the whole document redraw at 60fps
for ever — was reported here as needing a one-line patch to blitz-dom. It does
not. Upstream had already removed the API that caused it. See "The redraw
question, and the patch that was not needed" below; it is the most important
thing in this file and it is the one thing the first pass got wrong.

Run on macOS 15 on Apple silicon, against a clone of
`github.com/DioxusLabs/blitz` at `64eb2785` (`0.3.0-beta.2`), which is
`dioxus-native 0.8.0-alpha.1` as published. `wgpu 29`, `winit 0.31.0-beta.2`,
`anyrender 0.13` / `anyrender_vello 0.14`, pdfium `chromium/8021`.

**All four gates pass.**

```
cargo run --bin windows -- --auto 3      # three windows, made from a thread
cargo run --bin pages   -- --pages 20    # a document, drawn by pdfium
cargo run --bin chrome  -- --menu theme  # the toolbar, a menu, the notice line
cargo run --bin widget                   # one widget, and the frames it costs
cargo test --test recolor                # the shader against the reference
cargo run --bin probe                    # the DOM, with no window in front of it
```

`libpdfium.dylib` is not committed: `vendor/lib/` is filled from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
(`pdfium-mac-arm64.tgz`, this build `chromium/8021`), or pointed at with
`SPIKE_PDFIUM`.

The crate takes Blitz through **path dependencies into a clone beside this
repository** (`../../../blitz`), because the Custom Widget API this now rests
on is on `main` and only partly on crates.io: `dioxus-native 0.8.0-alpha.1`
exports `Widget` but not `CustomWidgetAttr`, which is how a widget is attached
from Dioxus. Move the paths to a git dependency when the next alpha lands.

---

## The redraw question, and the patch that was not needed

The first pass measured a steady 60 frames a second with nothing moving and
nobody touching the machine, at 52-62% of a core, for as long as one page was
on the screen. The cause was real and is still in the tree: `<canvas src="…">`
sets `has_canvas` on the document, `is_animating()` is `has_canvas |
has_active_animations | …`, and `View::redraw` asks for another frame whenever
the document is animating. This file said the fix was "a one-line property in
blitz-dom" and "upstreamable".

It was neither, for two reasons.

*A one-line fix would have been wrong.* Dropping `has_canvas` from
`is_animating` stops the idle frames and also stops a canvas that genuinely
animates from ever being redrawn, with no way to opt back in — because a
`CustomPaintSource` has no way to say "I have changed". That trait lives in
`anyrender_vello`, which is a different repository, so the honest version of
the fix was never one line and never only Blitz's.

*And it was already done, by a different route.* Blitz `main` removed
`use_wgpu` and the canvas paint source entirely in "New Custom Widget API
(#425)". The replacement is `blitz_dom::Widget`: a trait with the same three
lifecycle moments (`can_create_surfaces`, `destroy_surfaces`, `paint`) and a
fourth that is the whole answer —

```rust
/// Whether the widget currently requires redraws (e.g. because it is animating).
fn requires_redraw(&self) -> bool { false }
```

— which `Document::is_animating()` consults per widget. A page is not an
animation, so it says no, and the document goes quiet. `cargo run --bin
widget` is the control, and it is measured both ways on the same binary:

| `requires_redraw` | paints in ~5.4s |
| --- | ---: |
| `false` (the default) | **2, then nothing** |
| `true` (`--animate`) | 320, i.e. 60/s |

`cargo run --bin probe` says the same thing with no window and no GPU at all:
a document with a widget in it reports `animating: false`. And the page spike,
with twenty pages mounted, settles at **0 widget paints per second** and stays
there.

So: no fork of Blitz, no patch, and the item at the top of "what this changes
about the plan" is closed. What replaced it is a different cost, below.

**The new cost, which is the price of the same API.**
`build_custom_widget_scenes` in blitz-paint calls `paint()` on **every** widget
in the document each frame, not only the ones on screen — where a canvas paint
source was asked to render only when its canvas was painted. Measured: twenty
pages mounted, all twenty drawn by pdfium and resident on the GPU, 265MB of
texture, where the canvas version drew one. So this line from the first pass —

> Twenty mounted pages, one `render()` call: the nineteen below the fold cost
> nothing but a DOM node. That is `mount()` and `OVERSCAN` in `viewer.ts` given
> away for free

— is **withdrawn**. `mount()` and `OVERSCAN` are ours to write after all: a
page that is not near the viewport must not be in the DOM. That was always
true of the LRU; it is now true of the mounting window as well. It is the same
code `viewer.ts` already has, so it is a port and not an invention, but Phase 1
has to do it before the memory table means anything.

---

## 1. Two windows — passes

Three windows, cascaded 32 points apart, each with its own `VirtualDom`, its
own renderer and its own surface; the second and third asked for from another
thread while the first was up; closing the last one ends the app.

**`DioxusNativeApplication::add_window` still cannot do this.** It pushes onto
`BlitzApplication::pending_windows`, which is drained in `can_create_surfaces()`
and nowhere else — so a window asked for at runtime is never built — and the
Dioxus half of the setup (the contexts, and `initial_build()`) is done by
`launch` for the one window it makes and never again. A window added that way
comes up empty and stays empty. `src/shell.rs` is our own `ApplicationHandler`
over `BlitzApplication`, whose fields are public, and it is the shape the real
app would keep.

What changed on `main`, and what a shell of our own now has to know:

- **`resumed()` is no longer where windows are born; `can_create_surfaces()`
  is**, and `destroy_surfaces()` is its opposite. A window made after that
  first callback has to be resumed by hand, exactly as before.
- **Resuming is two steps.** `View::resume` starts the renderer and the
  renderer answers with `BlitzShellEvent::ResumeReady`, which
  `BlitzApplication` turns into `complete_resume` — so a view has to be in
  `inner.windows` before the event is drained, or the first frame never lands.
- **Events arrive on a channel, not on the winit proxy.**
  `BlitzShellProxy::new(event_loop.create_proxy())` hands back a sender and a
  `Receiver`, `proxy_wake_up` is where they are drained, and an embedder's own
  events are `BlitzShellEvent::Embedder` payloads taken out of that same queue.
- The **navigation provider** and the **HTML parser provider** are still
  private to `dioxus-native` and still have to be restated (`src/nav.rs`, six
  lines) or `dangerous_inner_html` silently does nothing.
- **`blitz-shell` needs its `custom-widget` feature turned on** even though
  `dioxus-native` turns it on for blitz-dom and blitz-paint. Without it,
  `View::redraw` never unregisters the resources of a widget whose node was
  dropped, and `complete_resume` never calls `can_create_surfaces` on the
  widgets already in the document. Nothing fails loudly; textures just leak.

**The macOS window fault is smaller than it was and has not gone.**
`WindowAttributes::with_position` now gets the x right and the y wrong: three
windows asked for logical (120,120), (152,152), (184,184) came up at physical
(240,176), (304,240), (368,304) — 64 physical pixels high, consistently, which
is a title bar. `set_outer_position` immediately after `View::init` puts them
where they belong. This is what `Placements` in `lib.rs` exists for and the
answer is unchanged: set the position again after the window exists.

Not answered here: Windows and Linux. The assessment's gate says all three
platforms, and this spike has only run on one.

## 2. A page on the screen — passes

Pages of `tests/fixtures/book.pdf` drawn by pdfium into a `wgpu::Texture`,
registered as an anyrender resource and composited by Vello. Nothing crosses a
process boundary, which was the whole question.

Release build, 900×900 window at 2×, `book.pdf`:

| | at 3.3MP a page | at 10.1MP a page |
| --- | ---: | ---: |
| pdfium, per page | 0.9ms | 4.1ms |
| BGRA→RGBA swizzle on the CPU | 1.6ms | 5.1ms |
| texture upload | 1.7ms | 5.6ms |
| texture resident, per page | 13MB | 41MB |

pdfium on its own, with no window at all (`cargo run --release --bin render`):
**3.3ms a page at 10.1MP**, and a 400-page document opens in 154ms.

The swizzle is the one number that is wildly worse in a debug build — 222ms a
page against 1.6ms — which is worth knowing before anybody measures the wrong
binary. It also should not exist in the real app: the recolouring compute pass
reads every pixel anyway and can swap two channels for nothing.

**Memory, and a floor that is higher than it was.** Peak RSS:

| what | RSS |
| --- | ---: |
| one widget, one 640×480 texture, no pdfium | 112MB |
| 1 page mounted and drawn | 162MB |
| 3 pages | 198MB |
| 3 pages at 10.1MP | 234MB |
| 20 pages (all of them drawn — see above) | 403MB |

The 112MB is the floor of an empty wgpu + Vello window on this machine, and it
is the number to argue with: the whole proposal is that a native stack beats
the webview's floor, and Tauri's whole app was measured at 182MB for a real
reading session. The first pass reported 69-83MB for a window with twenty
pages mounted and one drawn; the same shape now costs about twice that. What
changed between the two is a renderer version, a wgpu version and a Blitz
version all at once, so this is a measurement and not yet an explanation.
**Phase 1's memory gate should be run against `vello_hybrid` as well as
`vello` before anything is concluded from it**, because that is now the
default renderer upstream and it is the cheaper one by design.

Two things from the first pass still hold: **a `<canvas>` must be `display:
block`** (an `<object>` carrying a widget wants the same, and `probe` is what
tells you it is laid out at 0×0 rather than leaving you with a blank window);
and a resource registered against a renderer does not survive that renderer
being rebuilt — which is now the widget's own business, in `destroy_surfaces`,
where the trait puts it.

## 3. The shader — passes

`recolorByPixel` from `themes.ts` is ported twice: to Rust in `src/recolor.rs`,
faithfully enough to include `Uint8ClampedArray`'s round-half-to-even, and to
WGSL in `src/recolor.wgsl`. `cargo test --test recolor` runs both over a
fixture that hits every branch — the greys of a page of type, saturated plot
colours, the pale washes above the white point, the near-neutrals either side
of the colour floor — and holds them to **one level out of 255**, which is the
tolerance `recolor.test.mjs` already holds the app's two paths to. It passes
with colour keeping on and off, and it passes unchanged on wgpu 29 after three
mechanical API edits (`InstanceDescriptor::new_without_display_handle`,
`DeviceDescriptor::experimental_features`, `PollType::Wait { .. }`).

The test runs headless: an adapter, a device, a compute pass and a readback, no
window and no document. That is the shape the Phase 2 harness wants.

Two notes for whoever writes the real one. `target` is a reserved word in WGSL.
And the uniform block is two `vec4`s with the keep-colour flag in the ink's
`w`, rather than two `vec3`s and a float, because there is then one possible
layout rather than a std140 rule to be right about — the `vec3` version
compiled, ran, and silently read the flag as zero.

## 4. Chrome that looks right — passes

The toolbar, a popover menu with swatches and a current item, the page field
and the notice line, rebuilt from `styles.css` with the unsupported properties
taken out. Side by side with the app it is recognisably the same window.

- **`position: fixed` is gone and nothing misses it.** The root is a flex
  column — toolbar, viewer, notice — so nothing is over a scrolling body. The
  popover is a child of the root with `position: absolute` and coordinates
  worked out by hand, which is what `showPopover` already does.
- **`overflow: auto` → `overflow: scroll`**, with `scrollbar-width: thin`.
- **Icons are SVG with the colour written into the attributes.** CSS does not
  reach inside an SVG here, so `stroke: currentColor` paints nothing. Baking
  the colour works and looks right; in the app it is a memo keyed by icon and
  theme colour. There is now a *second* way to get a toolbar of blank icons,
  and it looks identical to the first: **the `svg` feature has to be on.** It
  is on by default in `dioxus-native` and off in a `default-features = false`
  build, which is what this crate does, and the failure is silent.
- **`text-overflow: ellipsis` is still missing (Parley #304), but
  `white-space: nowrap` now works** — and that is a change from the first
  pass, which found the long document title wrapping onto two lines whatever
  it was told. Measured in the probe: the same sentence in a 120px box is
  **16px tall with `nowrap` and 62px without it**. So the `mask-image` fade
  this file recommended and could not use is now visible in the real thing —
  the title runs to the edge of its box and fades out, on one line. Measuring
  and truncating in Rust stays the fallback for a place that really wants a
  literal "…", but nothing in the chrome needs it.

---

## What this changes about the plan

1. **The redraw question is answered and costs nothing to act on**: use
   `blitz_dom::Widget`, not the canvas paint source, and leave
   `requires_redraw` alone. The layout port can go first after all.
2. **Port `mount()`/`OVERSCAN` early, not late.** Every widget in the document
   is painted every frame, so an unmounted page has to be genuinely absent
   from the DOM, and until that exists every memory number is measuring the
   wrong thing.
3. **Measure `vello_hybrid` beside `vello` in Phase 1.** It is upstream's
   default now, and the 112MB floor measured here is the number the whole
   proposal has to beat.
4. **Keep the probe.** `cargo run --bin probe` builds a document and reads it
   back with no GPU and no window. It found `display: block` in an afternoon
   that a screenshot could only call "blank"; this time it answered both the
   animation question and the `nowrap` question in one run. It is the seed of
   the Phase 2 harness and it should grow into it rather than being replaced.

Two things this phase did not answer and should not be assumed: nothing has run
on Windows or Linux, and no measurement here involved scrolling a real document
with pages coming and going, which is where the LRU, the mounting window and
the memory table of Phase 1 actually live.
