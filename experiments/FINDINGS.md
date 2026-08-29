# Phase 0: what the four spikes answered

`ai-markdown/dioxus-assessment.md` names four questions that can kill the
Dioxus Native experiment, and says to answer them in a scratch crate before
writing anything that looks like the app. This is that crate, and these are the
answers. Everything below was run on this machine — macOS 15 on Apple silicon,
`dioxus-native 0.7.10`, `blitz-shell 0.2.3`, `vello 0.6`, `wgpu 26.0.1`,
pdfium `chromium/8021`.

**All four gates pass.** Two of them pass with a condition attached, and one
finding that no gate asked about is the most important thing here.

```
cargo run --bin windows -- --auto 3      # three windows, made from a thread
cargo run --bin pages   -- --pages 20    # a document, drawn by pdfium
cargo run --bin chrome  -- --menu theme  # the toolbar, a menu, the notice line
cargo test --test recolor                # the shader against the reference
cargo run --bin probe                    # the DOM, with no window in front of it
```

---

## 1. Two windows — passes

Three windows, cascaded 32 points apart, each with its own `VirtualDom`, its
own renderer and its own surface; the second and third asked for from another
thread while the first was up; closing the last one ends the app. A screenshot
of all three is what this claim rests on, and each of them draws its own
number.

**`DioxusNativeApplication::add_window` cannot do this**, which the assessment
half-suspected. It pushes onto `BlitzApplication::pending_windows`, which is
drained in `resumed()` and nowhere else — so a window asked for at runtime is
never built — and the Dioxus half of the setup (the renderer context that
`use_wgpu` is found through, and `initial_build()`) is done by `launch` for the
one window it makes and never again. A window added that way comes up empty and
stays empty.

So `src/shell.rs` is our own `ApplicationHandler` over `BlitzApplication`,
whose fields are public. It is about two hundred lines and it is the shape the
real app would keep: a window is a `WindowSpec` (attributes, a position, a
`VirtualDom`) pushed onto a queue plus a wake-up through the event loop proxy,
because a window can only be created inside a winit callback and a `VirtualDom`
cannot cross a channel that wants `Send`.

Three things a shell of our own has to bring that `launch` brings for you:

- the **navigation provider** (`link_handler.rs` is private) — six lines,
  restated in `src/nav.rs`, and it is the same rule `onExternalLink` follows;
- the **HTML parser provider**, or `dangerous_inner_html` silently does
  nothing, which is how every icon in the chrome spike came out blank;
- the **net provider** (`assets.rs` is private) and the **tokio runtime**
  `launch` enters. Neither is needed for a reader that fetches nothing, but
  `use_future` with a timer is not available without the runtime.

**Two macOS window faults, both already known to this codebase in another
form.**

*`WindowAttributes::with_position` is not honoured.* Asked for three windows at
logical y 560, 592 and 624, winit put all three at physical y 972 and got the x
right. `set_outer_position` immediately after `View::init` puts them where they
belong. This is exactly what `Placements` in `lib.rs` exists for, and the
answer is the same: set the position again after the window exists.

*A window must be resumed exactly once.* `View::resume` builds a fresh
`vello::Renderer`, and every texture a custom paint source registered with the
old one is orphaned; the next frame that hands back a cached texture kills the
process with `Tried to draw an invalid empty image (id: 1). Maybe it was
registered to a different renderer, or unregistered before this render was
submitted.` Our shell resumed a window itself and then let
`BlitzApplication::resumed` resume it again. **`dioxus_native::launch` has the
same shape** — it inserts its window and then calls `inner.resumed()` — so any
`use_wgpu` source that caches a texture across frames will hit this through the
documented path too. A source must also drop its cached textures in `resume()`,
which is the honest fix on the source's side.

Not answered here: Windows and Linux. The assessment's gate says all three
platforms, and this spike has only run on one.

## 2. A page on the screen — passes, with a condition

A page of `tests/fixtures/book.pdf`, drawn by pdfium into a `wgpu::Texture`
and composited by Vello, at 1600×2070 device pixels. Twenty pages mount in a
scrolling column. Nothing crosses a process boundary, which was the whole
question.

| | debug | release |
| --- | ---: | ---: |
| pdfium, per page at 10.1MP | 3.4ms | 4.8-7.6ms |
| BGRA→RGBA swizzle on the CPU, 13MP page | 220ms | 3.3ms |
| texture upload, 13MB | 3.4ms | 5.7ms |
| RSS, 20 pages mounted, one drawn | 152MB | 69-83MB |
| RSS, 12 small pages, three drawn | — | 103MB |
| binary | — | 11MB + 7.2MB of `libpdfium.dylib` |

Three things worth knowing before Phase 1 leans on any of it.

**Blitz only paints what is on screen, and a paint source is only asked to
render when its canvas is painted.** Twenty mounted pages, one `render()` call:
the nineteen below the fold cost nothing but a DOM node. That is `mount()` and
`OVERSCAN` in `viewer.ts` given away for free — though the LRU is still ours to
write, because a source that has drawn keeps its texture until something tells
it not to.

**A `<canvas>` must be `display: block`.** With the default display it is an
inline non-replaced box, width and height are ignored, it lays out at 0×0 and
is never painted — no error, no warning, a blank page. This cost an hour;
`cargo run --bin probe` is what found it and is the reason the probe exists.

**And the condition: one canvas anywhere in the document makes the whole
document animate for ever.** `load_custom_paint_src` sets `has_canvas`,
`is_animating()` is `has_canvas | has_active_animations`, and `View::redraw`
requests another frame whenever the document is animating. Measured: a steady
60 frames a second with nothing moving and nobody touching it, at 52-62% of a
core in release. The brief says "no animations unless the user takes an
action"; this is the opposite, and it is a battery cost on a device somebody
reads on for an hour.

There are three ways out and Phase 1 has to pick one: patch blitz-dom so a
canvas only animates when its source says it has changed (the right fix, and
upstreamable); keep the pages out of the DOM entirely and draw them in one
custom paint source that owns the whole viewport (which is a bigger change and
takes the layout with it); or accept it. The first is small — `has_canvas`
would become something a source can clear.

Also measured, and not yet paid for: `register_texture` copies the texture into
Vello's image atlas **at the start of every frame**. At 60fps with one 13MB page
resident that is 780MB/s of GPU-to-GPU copying for a page that has not changed.
It does not show up as CPU and it is the second reason to make the redraw
conditional.

## 3. The shader — passes

`recolorByPixel` from `themes.ts` is ported twice: to Rust in
`src/recolor.rs`, faithfully enough to include `Uint8ClampedArray`'s
round-half-to-even, and to WGSL in `src/recolor.wgsl`. `cargo test --test
recolor` runs both over a fixture that hits every branch — the greys of a page
of type, saturated plot colours, the pale washes above the white point, the
near-neutrals either side of the colour floor — and holds them to **one level
out of 255**, which is the tolerance `recolor.test.mjs` already holds the
app's two paths to. It passes with colour keeping on and off.

The test runs headless: an adapter, a device, a compute pass and a readback,
no window and no document. That is the shape the Phase 2 harness wants.

Two notes for whoever writes the real one. `target` is a reserved word in WGSL.
And the uniform block is two `vec4`s with the keep-colour flag in the ink's
`w`, rather than two `vec3`s and a float, because there is then one possible
layout rather than a std140 rule to be right about — the `vec3` version
compiled, ran, and silently read the flag as zero.

## 4. Chrome that looks right — passes

The toolbar, a popover menu with swatches and a current item, the page field
and the notice line, rebuilt from `styles.css` with the four unsupported
properties taken out. Side by side with the app it is recognisably the same
window.

- **`position: fixed` is gone and nothing misses it.** The root is a flex
  column — toolbar, viewer, notice — so nothing is over a scrolling body. The
  popover is a child of the root with `position: absolute` and coordinates
  worked out by hand, which is what `showPopover` already does.
- **`overflow: auto` → `overflow: scroll`**, with `scrollbar-width: thin`.
- **Icons are SVG with the colour written into the attributes.** CSS does not
  reach inside an SVG here, so `stroke: currentColor` paints nothing. Baking
  the colour works and looks right; in the app it is a memo keyed by icon and
  theme colour.
- **`text-overflow: ellipsis` is missing, and so, it turns out, is
  `white-space: nowrap`.** The long document title wrapped onto two lines
  rather than being cut off. Measured in the probe: the same sentence in a
  120px box comes out 62px tall with `nowrap` and `overflow: hidden`, and 63px
  tall without them — so the line is wrapping either way and the `mask-image`
  fade the assessment recommends never gets a chance to be seen. This is the
  one gap in the list that came out worse than it was written down.
  Truncating in Rust — measure the text, cut it, add a real "…" — is the
  fallback that does not depend on Parley, and it is what the sidebar, the
  menus and the toolbar will all need.

---

## What this changes about the plan

Nothing in Phase 0 says stop. The order of Phase 1 should change in two small
ways.

1. **Decide the redraw question first**, before the layout port. It is a
   one-line property in blitz-dom and everything downstream — the frame budget,
   the battery claim, whether the atlas copy matters — hangs off it.
2. **Keep the probe.** `cargo run --bin probe` builds a document and reads it
   back with no GPU and no window, and it found in seconds what a screenshot
   could only say was "blank". It is the seed of the Phase 2 harness and it
   should grow into it rather than being replaced by it.

Two things this phase did not answer and should not be assumed: nothing has
run on Windows or Linux, and no measurement here involved scrolling a real
document with pages coming and going, which is where the LRU and the memory
table of Phase 1 actually live.
