# The floor, and what was actually in it

`PHASE1.md` ends on one number and one question. The number is that an empty
Blitz window costs about 110MB, as much as the whole of Tauri's app process,
and that the reader built on it settled at 238MB against Tauri's 346MB — a
third off, where the assessment had wanted better than half. The question is
what the 110MB is made of, and `PHASE1.md` says plainly that the answer decides
whether there is a Phase 3:

> If it is Metal's per-surface overhead, it does not get better, and the honest
> conclusion is that a GPU renderer's floor is comparable to a webview's —
> which is the one finding that would end this experiment.

It is not Metal's per-surface overhead. It is two things, both of them
avoidable, and one of them is ours.

**Where it stands now**, on one machine in one sitting, macOS 15 on Apple
silicon, release builds, a 1100×900 window at 2×, `tests/fixtures/book.pdf`
(400 pages of plain text). Every figure is a **physical footprint**, which is
what Activity Monitor shows and what the kernel charges against a memory limit;
see "The measurement was wrong" below for why the old table's numbers were not.

| | Tauri + pdf.js | Dioxus Native, as Phase 1 left it | …now |
| --- | ---: | ---: | ---: |
| document open, nobody scrolling | 373MB | 376MB | **144MB** |
| after ~60 screenfuls | 466MB | 796MB | **~200MB, settling to 140MB** |
| one page drawn | 27ms | 6.4ms | **3.2ms** |
| …uploaded | — | 4.9ms | 4.7ms |
| binary | 6.2MB | 11.9MB + 6.9MB pdfium | 12MB + 7.2MB pdfium |

The Tauri column is the installed app opened on the same document and measured
the same way, summed over its four processes — 34MB for the app, 315MB for the
web content process, 18MB for WebKit's GPU process and 7MB for its network
process. It is not the number in `AGENTS.md`, and the difference is the metric
rather than the app; see below.

**So the assessment's Phase 1 gate is met after all.** It asked for under 150MB
on 400 pages of plain text against 346MB today. On the metric that counts GPU
memory the comparison is 144MB against 373MB, and the memory win is a factor of
2.6 rather than a third.

---

## The measurement was wrong

`stats::rss_mb` shelled out to `ps -o rss`, and every memory number in
`PHASE1.md` and `FINDINGS.md` came from it. On macOS that is `resident_size`,
and **a GPU buffer is charged to a process's physical footprint and only partly
to its resident size.** The two disagree by a factor of three on exactly the
workload this experiment is about.

The clearest demonstration is the one that closes item 3 on Phase 0's list.
`PHASE1.md` measured `vello_hybrid` beside `vello` as the assessment asked, got
228MB against 234MB, and concluded:

> Three per cent, and pages draw correctly through both. That closes item 3 on
> Phase 0's list.

Measured as footprint, on an empty window with one frame drawn:

| renderer | rss | footprint |
| --- | ---: | ---: |
| `vello` | 95MB | **208MB** |
| `vello_hybrid` | 85MB | **19MB** |

Eleven times, in the number that matters, and invisible in the one that was
being read. The 3% was real and was measuring the wrong thing.

`stats.rs` reports both now, and `footprint_mb()` is what a session is
summarised on. It costs a few hundred milliseconds — `vmmap --summary` is the
only thing that answers without linking against mach — so it is read where a
session ends and never in a frame.

## The ablation

`dioxus-spike/src/bin/floor.rs` builds the stack one layer at a time and
measures after each, one process per stage, because a stage cannot be unbuilt:
a wgpu device that has existed has already made its allocator's arenas.

```
cargo run --release --bin floor -- --all
```

| stage | rss | footprint |
| --- | ---: | ---: |
| the process alone | 8.6MB | 1.8MB |
| + a winit window | 75.6MB | 15.7MB |
| + a wgpu instance, adapter and device | 81.3MB | 16.4MB |
| + a surface configured to the window | — | — |
| + `vello`, resumed, one empty frame | 95.0MB | **208.0MB** |
| + `vello_hybrid`, resumed, one empty frame | 84.9MB | **18.8MB** |

(The surface stage panics on the way out — `Failed to wait for GPU to come idle
before reconfiguring the Surface`, a wgpu 29 shutdown-order fault in the spike
rather than in anything above it — after reporting the same 12.7MB the device
stage does. It is left in because the numbers before the panic are the ones the
row wants, and fixing it would be work on a scaffold.)

Read the two bottom rows against the one above them. **Nothing in Blitz, in
Stylo, in Parley, in fontique's system-font enumeration or in winit costs
anything worth naming.** A window with a GPU device and a swapchain behind it is
16MB. What costs 190MB is the first frame Vello draws, and it costs the same
190MB whether the frame is empty or full.

## What Vello allocates before it draws anything

`vello_encoding`'s `BufferSizes::new` computes buffer sizes for the compute
pipeline, and seven of them are constants that do not depend on the scene:

```rust
// The following buffer sizes have been hand picked to accommodate the vello test scenes as
// well as paris-30k. These should instead get derived from the scene layout using
// reasonable heuristics.
let bin_data = BufferSize::new(1 << 18);
let tiles = BufferSize::new(1 << 21);
let lines = BufferSize::new(1 << 21);
let seg_counts = BufferSize::new(1 << 21);
let segments = BufferSize::new(1 << 21);
let blend_spill = BufferSize::new(1 << 20);
let ptcl = BufferSize::new(1 << 23);
```

| buffer | elements | element | bytes |
| --- | ---: | ---: | ---: |
| `lines` | 2,097,152 | `LineSoup`, 24B | 50.3MB |
| `segments` | 2,097,152 | `PathSegment`, 24B | 50.3MB |
| `ptcl` | 8,388,608 | `u32` | 33.6MB |
| `tiles` | 2,097,152 | `Tile`, 8B | 16.8MB |
| `seg_counts` | 2,097,152 | `SegmentCount`, 8B | 16.8MB |
| `blend_spill` | 1,048,576 | `u32` | 4.2MB |
| `bin_data` | 262,144 | `u32` | 1.0MB |
| | | | **173.0MB** |

173.0MB computed, 190MB measured at the first frame, and `vmmap` puts 179MB of
the `widget` spike's 227MB under *Owned physical footprint (unmapped)
(graphics)*. That is the floor, and it is a scene-independent constant sized
for a scene a hundred times more complex than a page of a book and a toolbar.

The comment in the source is the whole story: the sizes *should* be derived from
the scene and are not. There are two ways to act on it and this experiment takes
the cheap one.

**`vello_hybrid` is now the default and there is no case for the other one
here.** It splits the work CPU/GPU, allocates none of the above, is upstream's
own default, takes the same `wgpu::Texture` resources through the same
`try_register_custom_resource`, and draws pages correctly. It is a cargo feature
in `dioxus-reader` — `--no-default-features --features vello` gets the other
back — because the comparison has to stay runnable.

**Patching `BufferSizes` is the other way and it is an upstream conversation
rather than a fork.** For a reader the scene is a handful of rectangles, some
images and a toolbar's worth of text; a tenth of every one of those constants
would do, and the code says as much itself. That is worth raising with
linebender whatever this experiment decides, because it is not a fault that only
a PDF reader has.

## And 96MB of it was ours

With `vello`'s scratch out of the picture, the reader still sat at 240MB with
two pages mounted and 46MB of texture on the GPU. `vmmap` named the difference
without being asked twice:

```
MALLOC_LARGE (empty)   59e800000-59f800000   [ 16.0M  16.0M  16.0M ]
MALLOC_LARGE (empty)   59f800000-5a0800000   [ 16.0M  16.0M  16.0M ]
MALLOC_LARGE (empty)   5a0800000-5a1800000   [ 16.0M  16.0M  16.0M ]
MALLOC_LARGE (empty)   5a1800000-5a2fe8000   [ 23.9M  23.9M  23.9M ]
MALLOC_LARGE (empty)   5a3000000-5a47e8000   [ 23.9M  23.9M  23.9M ]
MALLOC_LARGE (empty)   5a4800000-5a5fe8000   [ 23.9M  23.9M  23.9M ]
```

Six regions, 120MB, every one of them **`(empty)`** — freed, and still charged
to the process, because macOS's allocator does not hand large blocks straight
back. 23.9M is exactly one page of `book.pdf` at this window size. There were
three of them because `pdfium.rs` made three copies of every page it drew:

```rust
let bitmap = page.render_with_config(&config)?;   // pdfium's own buffer
Ok(Bitmap { bgra: bitmap.as_raw_bytes().to_vec(), .. })
//                 ^ returns an owned Vec        ^ and copies it again
```

`PdfBitmap::as_raw_bytes` is not a view. It is `FPDFBitmap_GetBuffer_as_vec`,
which allocates and copies; the `.to_vec()` on the end of it allocates and
copies a second time. Three 24MB buffers alive per page, all three freed at the
end of the call, none of them returned.

**The renderer now draws into a buffer it keeps.** `PdfBitmap::from_bytes` wraps
a slice we own, so pdfium renders straight into it; `render` lends the bytes to
a callback for exactly as long as the upload takes, which is what turned
`Bitmap` from an owned `Vec` into a borrow. The buffer is resized only when the
page size changes — a zoom or a window resize, not a page turn — so a document
scrolled end to end allocates once. It lives behind the same lock the document
does, because pdfium is not thread safe and every render is already serialised
through it.

It is worth 96MB and 3.2ms a page:

| | before | after |
| --- | ---: | ---: |
| footprint, document open | 240MB | **144MB** |
| `MALLOC_LARGE` | 120MB, 6 regions | 24MB, 1 region |
| drawing one page | 6.4ms | **3.2ms** |

The drawing time is the more interesting half. Phase 1 reported 6.6ms a page and
attributed it to pdfium; half of it was memcpy.

**One thing that looked like the same fix and was not.** The themed texture is
made per page and the source texture is read once and dropped, so keeping and
reusing the source looked like the same 24MB saving one layer up. It is not:
holding one costs a permanent 24MB (144MB idle became 169MB) and changed the
mid-scroll figure not at all, because the pile during a scroll is the *themed*
textures, which wgpu cannot free until the submission that read them has
retired. It is reverted, and the comment in `gpu.rs` says why so that it is not
tried again.

## What is left in the 144MB

| what | | note |
| --- | ---: | --- |
| page textures | 46MB | two mounted pages, 23MB each. The mounting window's, and it does not grow with the book |
| the swapchain | 43MB | three IOSurfaces at 2200×1800. The window's, and it scales with it |
| the page buffer | 24MB | one page, reused. The scratch above |
| small allocations | 21MB | Rust, Stylo, Parley, pdfium's own structures |
| everything else | ~10MB | |

None of it is a mystery any more and none of it is a fixed cost of the stack.
The obvious remaining lever is the swapchain — three buffers where two would do
— and it is winit's and wgpu's rather than ours.

**Mid-scroll is the one number still worth chasing.** Reading 60 screenfuls
takes the footprint to about 200MB and the peak to 390MB, with `vmmap` showing
177-228MB of graphics memory against the 46-75MB the counters say is mounted.
That is themed textures dropped and not yet freed: wgpu retires a texture when
the submission that used it does, and during a fast scroll pages are replaced
faster than they retire. It settles within a second of the scroll stopping.
The fix, when it is worth making, is the one `viewer.ts` already has — a pool
of page-sized textures reused rather than a new one per page, which is
`pageCache` and `discard()` in a different register.

## Caveats, stated plainly

- **One machine, macOS, Apple silicon.** Nothing here has run on Windows or
  Linux, and the physical-footprint accounting is a macOS concept; the
  equivalent question elsewhere is a different tool and possibly a different
  answer. Vello's 173MB is not platform-specific — it is a constant in a Rust
  source file — but everything about how an allocator holds freed blocks is.
- **One document.** `book.pdf`, 400 pages of plain text. The assessment's memory
  table has four documents and three of them are not in this repository. A
  scanned volume is the one most likely to behave differently, because the page
  buffer is sized by the window rather than by the scan, and that is the shape
  the CPU-side fix helps most.
- **A window nothing can see is a window nothing redraws.** Two `--measure 60`
  runs reported 4 pages drawn and 11 paints while another two reported 34 and
  243, and the difference was whether something was in front of the window. The
  events still arrive — they are injected through the shell, not the system —
  and macOS stops the frames. A stalled paint count is the sign, and it is a
  property of the harness rather than of the reader.

## What this changes about the plan

1. **The kill switch does not fire.** `PHASE1.md` named the one finding that
   would end the experiment — that the floor belongs to the stack and does not
   get better — and it is the opposite of what happened: none of the floor
   belonged to the stack. 144MB against Tauri's 373MB on the same document,
   measured the same way, is the win the binary cost was supposed to buy.
2. **`vello_hybrid` is the renderer.** Not as a fallback for hardware without
   compute, which is how the assessment lists it, but as the default, for
   memory. `vello` stays behind a feature so the comparison stays runnable.
3. **Measure footprint, never RSS.** Every number in `PHASE1.md` and
   `FINDINGS.md` predating this file is understated where GPU memory is
   concerned, and the vello/hybrid comparison in `PHASE1.md` is wrong rather
   than merely imprecise. `stats::line()` reports both now.
4. **Phase 2, the harness, is next, unchanged.** The one thing this file adds to
   it: it needs a memory assertion, and `footprint_mb()` is what it should
   assert on. A regression like three copies of every page is exactly the shape
   a test catches and a reading session does not.
   *Built — `PHASE2.md`. The assertion is `tests/cost.rs`, and it is a growth
   bound rather than a ceiling: ten screenfuls to settle, forty more, and the
   footprint may not climb across them.*
5. **Two things to raise upstream, neither blocking.** `BufferSizes` sized from
   the scene rather than from paris-30k, and `PdfBitmap::as_raw_bytes` named as
   the copy it is — a function that looks like a view and allocates 24MB is a
   trap anybody using `pdfium-render` for a reader will fall into.
