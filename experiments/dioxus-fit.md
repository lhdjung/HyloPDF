# Is Dioxus Native a good fit for HyloPDF?

Written 2026-08-31, against `dioxus-experiment` at `b6b1cb3` plus its uncommitted
working tree, and against Blitz at `c6dec888` — the revision this tree's path
dependencies point at.

This was asked from the other side of the fence. `QRnew/dioxus-assessment.md`
says its migration "is a much better fit for QRnew than it is for the app the
reference documents describe," and the reference documents are `brief.md`,
`dioxus-assessment.md` and `PROGRESS.md` in this directory. So there are two
questions here and the first one is in the way of the second: **what is that
sentence measuring**, and **is Dioxus Native a good fit for HyloPDF**.

They have different answers, and the gap between them is the useful part.

---

## The short answer

**Yes, and the strongest evidence for it is that four fifths of it is built and
passing.** Phases 0, 1 and 2 are done, nine of the ten Phase 3 items are done,
241 tests run in about ninety seconds, and every gate the assessment set has
been met — 144MB against 373MB, 3.2ms a page against 27ms, five of the app's own
Rust modules compiled unchanged with forty-four of their own tests. "Is it a
fit" was a live question in the spring. It is now mostly a question that has
been answered by construction.

What is left is not a fit question at all. It is a **platform-coverage
question** — nothing here has run on Windows or Linux, and HyloPDF ships
installers for both — and a **maturity question**, which is a schedule risk
rather than a design one.

And one thing on the risk list is void: **IME, named in both documents as the
one blocking item, exists at the revision this tree is already pinned to.** See
Part 3.

---

# Part 1 — What that sentence is measuring

The QRnew document is comparing **how much friction each app meets on the way
across**, not how much either one gains by going. On every axis it names,
QRnew's number is smaller, and mostly by an order of magnitude.

| | QRnew | HyloPDF |
| --- | ---: | ---: |
| UI to be rewritten | 532 lines (`src/app.rs`) | ~11,980 lines of TypeScript, 2,060 of CSS |
| Core that moves unchanged | 1,722 lines, already extracted, untouched | ~2,450 lines, mounted by `#[path]`, untouched |
| Renderer change riding along | none — same `usvg`, same bytes | pdf.js → pdfium, a second migration |
| Test apparatus | none existed; the harness is pure gain | 17 files + a Playwright/WebKit runner + `api.ts`'s browser twin, all discarded |
| Binary | 8.7MB → 8.4MB | 6.2MB → 19MB |
| Crates in the lockfile | 634 → 623 | up |
| Known upstream faults met | 2 of 4 | 4 of 4, plus the CSS gap list |
| Custom Widget API needed | no | yes — and it is the newest thing in Blitz |
| A shell of our own | no | ~630 lines, off the documented path |
| Honest estimate | one to two weeks | three to five months |

Read down that column and the sentence is plainly true. QRnew's core already
emits an SVG that Blitz's own `usvg` parses, with every colour baked into
presentation attributes — a decision `draw.rs` made years ago so that exported
files would stand alone, which happens to be exactly what Blitz's one documented
SVG gap requires. Its file dialog is already `rfd` and its clipboard is already
`arboard`, which are the two crates Blitz's own feature flags pull. Nothing had
to be adapted, because nothing was misaligned to begin with.

## Which is not the same as "a better idea"

The axis the table does not have is **what each app is buying, and whether it
had another way to buy it.**

QRnew is buying 97MB of physical footprint, 0.00s of idle CPU where it had
0.03s, and a test suite it did not have. Real, and worth having. But QRnew works
today, `libcosmic` is production software, and if the migration had gone badly
the parked branch would have cost a fortnight. It is an **optional** move with a
**modest** payoff and an **easy** path.

HyloPDF is buying:

- 373MB → 144MB, a factor of 2.6, on the axis the brief exists to complain about;
- 27ms → 3.2ms a page, and a document open that stops being two orders of
  magnitude slower than it should be;
- **capabilities the current architecture cannot have at all.** pdf.js's
  `Annotation.save()` is not overridden by any markup subtype, so an annotation
  already in a file cannot be edited or deleted — and the journal,
  `annotation_id: null`, the rebuild-from-backup removal and the byte-truncation
  undo are all scaffolding around that one hole. Encrypted documents open rather
  than being refused. Selection stops being a pixel trick over a substitute-font
  text layer. Recolouring stops being 835 lines of blend-mode probing with a
  pixel-walk fallback and becomes one WGSL function.

And, unlike QRnew, **it had no alternative.** The brief's premise is the webview
memory floor, which Tauri cannot remove because it is Tauri. Keeping the bitmap
on the native side under Tauri was tried and killed on the `pdfium-prototype`
branch by 43ms a page of IPC. And of the non-webview Rust UI stacks, Blitz is
the only one that reads CSS — so egui, iced or Slint would each mean rewriting
the *product* rather than the *runtime*, discarding 2,060 lines that encode the
whole look the brief asks for.

**So: QRnew is an easy migration with an optional payoff. HyloPDF is a hard
migration with a necessary one.** "Better fit" is measuring the first half of
each of those sentences and not the second. Both apps can be, and are, right to
do it.

## One thing the comparison hides, and it runs HyloPDF's way

QRnew's port is *larger* than what it replaced: 532 lines of `app.rs` became
1,663 lines of `ui.rs` and 905 of `ui.css`, roughly five times over. That is not
waste. It is the bill for what `libcosmic` was supplying — a colour picker with
a saturation/value square, an about panel, a header bar, a context drawer,
tooltips and a theme — all of which become your own code the moment the toolkit
is gone. Blitz gives you a browser engine, not a widget set.

**HyloPDF has already paid that bill.** `dioxus-assessment.md` notes that
`<select>`, `<dialog>`, `<progress>` and `<meter>` are all unsupported and that
"the app uses none of them — every menu and the modal are hand-built already."
That was written as a lucky escape from a gap list. It is bigger than that: a
web app that already refused the platform's widgets has nothing to lose when the
platform's widgets stop existing, and the 11,980 lines are therefore an honest
count of work to *translate* rather than an underestimate of work to *reinvent*.
The one place it does not hold is `<input type="password">` for the encrypted-
document prompt, and that is one field.

---

# Part 2 — The fit, assessed on its own terms

## Three things that fit structurally, not by luck

**1. The bridge that killed the last experiment does not exist.** This is the
whole argument and it has been measured twice now. `AGENTS.md` records the
pdfium prototype failing on transport: 3.6ms of drawing and ~43ms of getting the
pixels into a canvas in a web content process, with all three ways out costed
and all three failing. Here the DOM, the page bitmaps and the renderer are one
process and a page goes to the GPU as a `wgpu::Texture` — 4.7ms of
`write_texture`. The objection recorded against "keep the bitmap on the native
side" was that the text layer, selection, links, outline and thumbnails would
have to follow the pixels out of the DOM. In Blitz they *are* DOM. The objection
was to a native layer under a webview and does not survive being a native engine
under the whole app.

**2. The seam rule paid, and it paid harder than the assessment claimed.**
`settings.rs`, `theme.rs`, `library.rs`, `keys.rs` and `watch.rs` — about 2,450
lines — are mounted by `#[path]` and compiled with no copy and no edit, with
forty-four of their own tests running unmodified. The assessment predicted one
change (`watch.rs`'s `emit_to`) and even that turned out to be on the other side
of the file: `extern crate self as tauri;` plus 130 lines of `emit.rs` supplying
the two names it imports. Five files, zero edits. That is a stronger result than
the plan asked for, and it is the reason "the runtime is rewritten, the app is
not" is a true sentence here rather than a slogan.

**3. The port keeps finding that the app's complexity was the webview's.**
`search.ts` is 540 lines and half of it is pdf.js's text layer rather than
searching. `sidebar.ts` is 699 lines and about half is a thumbnail cache that a
mounting window makes unnecessary. `themes.ts` is 835 lines and the recolouring
half is one shader. `viewer.ts`'s rotation clears three caches because its link
layer is sized in percentages of a turned page; here every rectangle stays in
the page's own unturned points and one function turns them, so a rotation throws
nothing away and the port came out *shorter than what it replaced while gaining
a feature.* That pattern has now repeated across six Phase 3 items. It is the
signature of a good fit rather than a lucky one: the new substrate keeps
removing the reason a piece of the old code existed.

## What does not fit, stated plainly

**The window story is off the documented path.** `shell.rs` owns
`BlitzApplication` directly and is written against public fields rather than a
supported API, because `DioxusNativeApplication::add_window` does not do what
its name says and `use_window_event` is closed to a shell of our own. It works —
two documents, the cascade, the Dock menu, one instance over a Unix socket, the
quit-versus-close rule — and item 9 found that most of the *rules* need no
window at all and are now `windows.rs` with fourteen tests. But it is the
highest-risk item in the tree and it has been shown to work on macOS and nowhere
else. Windows has no Unix socket; Apple Events need a bundle this experiment has
not got.

**Four upstream faults, all worked around, each with a test that fails the day
it is fixed.** A click clearing the focus onto `<html>` with no way to take it
back (four appearances, and it decided the shape of the page field). Hit-testing
that does not clip on `overflow: hidden`. A Stylo panic on a stylesheet mutation
during a state-only element snapshot. `PdfBitmap::as_raw_bytes` allocating 24MB
while looking like a view. None is a blocker; all four are the cost of alpha
software, and the shape to expect is "found in a day, worked around in a day."

**The test apparatus is rewritten, not ported.** That is the item most likely to
be underestimated by anyone reading the progress and not the plan. It is done,
and it is better than the thing it replaces on one axis — `screenshot()` means
rendering can be tested, which `npm test` never could — but "we replaced the
safety net first" is only comfortable in retrospect.

**The binary doubles.** 6.2MB against 12MB plus 7.2MB of pdfium. The brief
permits this explicitly as a price for memory, and the price is paid.

## The one thing genuinely still open

**Windows and Linux.** Nothing in this experiment has run on either, and the
README ships a `.deb`, an `.rpm`, an AppImage, an `.msi` and a `-setup.exe`.
Four separate risks live here and they are not the same risk:

| | what is unknown | how bad if it fails |
| --- | --- | --- |
| Stylo, Parley, fontique, Taffy | do they behave on engines this is not developed on | probably small; findable with no GPU |
| `vello_hybrid` on common Linux GPUs | is it smooth on an Intel iGPU | `vello_cpu` is built and is the fallback; if neither is smooth, **stop** |
| the shell of our own | winit's window lifecycle on three platforms | **stop** — multi-document is not negotiable |
| single instance | Unix socket has no Windows equivalent; a named pipe has no std type | contained; a known amount of work |

Only the third and fourth are structural. The first is the cheapest thing left
on the whole list to find out about, and it is not done — see Part 3.

---

# Part 3 — Four things the QRnew branch settles for this tree

QRnew's branch was built today against Blitz `c6dec888`, which is the identical
revision `experiments/dioxus-reader` points at. Everything here transfers.

## 1. IME exists, and it is not blocking any more

Both `dioxus-assessment.md` and `PROGRESS.md` name IME as **the** item requiring
a decision rather than a workaround: no composition events, so the find field
built in Phase 3 item 4 cannot take composed input and a reader writing CJK
cannot search. That is void.

Verified by reading the pinned revision:

- `packages/blitz-dom/src/events/ime.rs` — `handle_ime_event` takes the focused
  node, gets its `text_input_data_mut()`, and calls `apply_ime_event` through
  Parley. Any focused text input, which is what the find field and the settings
  fields are.
- `packages/blitz-shell/src/window.rs:638` routes winit's `WindowEvent::Ime`
  into it; `convert_events.rs:31` maps all four variants including
  `DeleteSurrounding`; `blitz-shell/src/lib.rs:125` reports the cursor area back
  to the compositor so the candidate window lands in the right place.
- QRnew's `composed_text_reaches_the_field` types 日本語 into a field by
  composition through `blitz-test-harness` and asserts on what comes out. It
  passes.

**One sharp edge, and it is winit's contract rather than a fault.**
`BlitzImeEvent::Commit` inserts at the selection *without clearing the composing
region first*, because winit sends an empty `Preedit` immediately before every
`Commit`. A test that omits that empty preedit gets "にほん日本語" and looks
like a Blitz bug. It is not.

So: strike IME from "five things worth raising upstream", strike it from "two
things still to decide", and write the test. It is an hour, and it retires the
only item on either list that a decision had to be made about.

## 2. The focus fault — where the upstream fix goes

This is the fault that has cost this tree four workarounds, and it is worth
filing with a precise pointer rather than a description. Confirmed still present
at `c6dec888`:

`handle_click` in `packages/blitz-dom/src/events/pointer.rs` walks up from the
target matching on `el.name.local` — checkbox, radio, summary, label, `<a href>`,
submit buttons, file inputs — and everything else falls through `_ => {}` to
`maybe_node_id = doc.nodes[node_id].parent`. If the walk reaches the root
unmatched, `generate_focus_events(doc, &mut |doc| doc.clear_focus(), …)`.

**The walk never consults `is_focussable()`, which already exists and already
knows the answer.** `packages/blitz-dom/src/node/element.rs:627`,
`flush_is_focussable`, honours `tabindex >= 0` and lists `button`, `input`,
`select`, `textarea`, `frame`, `iframe` and `summary` as focusable by default —
the exact browser rule. It is used by `focus_next_node` and `focus_prev_node`
and by nothing on the click path.

So the upstream request is one sentence: *consult `is_focussable()` in
`handle_click`'s ancestor walk.* It answers all four of this tree's appearances
and QRnew's one, and it is a change to a file that already contains the
predicate.

## 3. Move off the path dependency — it is an afternoon and it unblocks the CI job

`experiments/dioxus-reader/Cargo.toml` still points at `../../../blitz`. That
makes the clone a build dependency that is not in this repository, and a machine
without it gets `failed to load manifest for dependency blitz-dom` and nothing
else — `PROGRESS.md` says so and treats it as unavoidable until the next alpha
lands.

It is not unavoidable. QRnew is pinned to the same revision as a **git
dependency**, and a fresh checkout builds with nothing beside it:

```toml
[dependencies.dioxus-native]
git = "https://github.com/DioxusLabs/blitz"
rev = "c6dec888aca71fa72c9e5395e8da330ed84e8d9e"
```

Note what this also settles: `blitz-test-harness` is `publish = false` in
upstream's own manifest, so waiting for crates.io was never going to work — the
harness the whole Phase 2 argument rests on can never come from there.
Everything else is published as `0.3.0-beta.2`. A pinned git rev is the answer,
it is strictly better than the clone, and it is the thing standing between this
tree and the CI job in "three things to carry forward" #3.

**And that CI job is the highest-information-per-hour item left in the
experiment.** `cargo test` needs no GPU and no screen; running it on three
platforms exercises Stylo, Parley, fontique, Taffy and the whole reader on
engines this has never touched, and it is the cheapest possible answer to the
first row of the platform table above.

## 4. Three gap-list entries have gone stale

Checked against the pinned revision rather than against `blitz.is/status`:

**`pointer-events: none` works.** `node/node.rs:1299` in `hit_inner` skips
`PointerEvents::None` elements, landed in `85d072bc` (#465, June). This tree's
assessment already lists it ✅; QRnew's addendum says it is missing and builds
its colour-picker thumbs as background layers to avoid it. That workaround
appears to be unnecessary, and since the QRnew document recommends carrying it
to other Blitz apps, it is worth not carrying. One caveat: Blitz's version still
hit-tests *descendants* of a `pointer-events: none` element, where a browser
would not, so it is not exactly the CSS rule.

**`overflow: auto` no longer needs the workaround.** `stylo_taffy/src/convert.rs:271`
maps it to `taffy::Overflow::Scroll` with a TODO. So the five uses do not need
rewriting to `scroll` by hand; they already behave that way.

**Blitz has real scrollbars now** — `blitz-dom/src/node/scrollbar.rs`, from
`2e2330e7` ("Implement scrollbars end to end", #489) and `9ac084ae` ("Hook
scrollbar styles up to Stylo", #707), with hover state, drag targets and
`ScrollbarRef` resolved during hit-testing. Phase 1 called the scrollbar "the
largest single thing given up." **This is worth an hour of investigation and is
not a claim that it is solved**: the reader owns its scroll offset because
`MountedData::scroll` panics from inside any borrow, and the pages are placed
against that number rather than living in a scroll container, so a native
scrollbar has nothing to reflect. Whether the two can be reconciled — a real
scroll container holding a spacer of the document's height, with the mounting
window driven off the scroll event rather than off an owned number — is a
question nobody has asked yet.

**`position: fixed` and `sticky` are still absent**, and still silently: they
map to `Absolute` and `Relative` in the same file. The four workarounds stand.

## And one piece of housekeeping

`PROGRESS.md` is a commit behind its own tree. `src/select.rs` (392 lines) and
`tests/select.rs` (272) are untracked, `app.rs` has 521 uncommitted added lines,
and `blitz-shell`'s `clipboard` feature has been turned on for ⌘C — while "What
is not built" still reads "No text layer, no selection". Since that file is the
one place the numbers live, and it earned that role by replacing four documents
that each opened by correcting the one before, it is worth not letting it start
doing that again.

---

# Part 4 — What would still kill it

Unchanged from the assessment's own risk table, minus one row:

- **Vello unusable on ordinary Linux hardware.** `vello_hybrid` is the default
  and `vello_cpu` is built; if neither is smooth on an Intel iGPU, stop.
- **The shell of our own failing on Windows or Linux.** Multi-document is not
  negotiable, and this is written against public fields on an alpha API.
- **Time.** Three to five months was the estimate and roughly four fifths of it
  is spent. The remaining fifth is markup — which is where it stops being a port
  — plus the settings window, the Keyboard page and the start screen. The
  platform work is done, and so are the menus and the file picker (2026-09-01).
  That is not a small fifth.
- **What the fifth is made of, and why it kept being underestimated.** All of
  it is *interface*, and interface was never on the item list: the eleven items
  are the order the app's engine was built in. Every estimate taken off that
  list has therefore been an estimate of the engine. See the top of
  `PROGRESS.md`.
- **And a tail of the same thing that no item will ever name.** Four faults
  found by reading with it on 2026-09-01 — a zoomed page pinned to the left
  edge with the rest unreachable, undrawn pages flashing white, the page field
  emptying itself, a toolbar the same grey under every theme — were each a
  correct answer badly placed or badly coloured, and no test asks that
  question. Budget for finding them by using it, not by finishing the list.
- **Blitz's own pace.** Production readiness is "sometime in 2026" by the
  project's account and the WPT `css` subsuite is at 48%. Nothing here needs the
  other 52%, but the API moves and the cost is paid in the shell.

**No longer on this list: IME.** It was the only item that needed a decision
rather than a workaround, and there is nothing left to decide.

**Not on this list, and it should not be: the binary.** The assessment leaves it
as an open question. The brief's goal 2 answers it in advance — "a slightly
larger binary would be an acceptable price for improvements in the other
aspects, especially memory" — and 19MB for a 2.6× memory win is that trade at
the rate the brief set. The alternative it names, spending the same weeks
reducing memory inside Tauri, is worse than it looks: the last such pass already
happened and took 2521MB to 327MB, so the easy findings are gone, and what
remains has a webview floor under it that no amount of work removes.

---

# What has been done since this was written

Everything in Part 3 that was an instruction rather than an observation, plus
the housekeeping. Recorded here rather than by editing the parts above, because
what this document is *for* is the reasoning, and a recommendation that has been
acted on is worth reading beside its outcome.

- **The path dependency is gone.** Both crates — the reader and the Phase 0
  spike — take Blitz as a git dependency pinned to `c6dec888`, which is the
  revision the clone was already on. The suite passed on the other side of it
  unchanged. A fresh checkout of this repository now builds with nothing beside
  it, which is the sentence the whole rest of the list depended on.
- **The CI job exists**: `.github/workflows/experiment.yml`, `cargo test` on
  macOS, Linux and Windows, with pdfium downloaded per platform and the app's
  own fixture generated by the one Node script that has no dependencies. Two
  findings before it ever ran: the tree cross-checks for Windows from this
  machine (`--target x86_64-pc-windows-msvc`, standard library, no linker), and
  Linux cannot be cross-checked the same way because fontconfig wants a
  sysroot — its `RUST_FONTCONFIG_DLOPEN` escape changes the crate's API enough
  that `fontique` stops compiling. The runner is the answer, which is what this
  document said.
- **IME is struck**, exactly as Part 3 predicted and for the reason it gave.
  `Reader::compose` sends what an input method sends and `tests/ime.rs` is five
  tests: 日本語 into the field, a composed `résumé` found in the document, a
  preedit that is *not* searched for, the empty preedit before a commit
  asserted both ways round, and a composition that does not drive the document
  behind the field. Nothing in `app.rs` changed. The sharp edge this document
  named is real and is now written down where it fails if it moves.
- **The focus fault's pointer is in `PROGRESS.md`**, in the upstream list, with
  the file and the predicate named — ready to file, not filed. That is the one
  item on the list that needs somebody with an account.
- **`PROGRESS.md` is no longer a commit behind its own tree.** Item 10 is
  committed and written up, and so is all of the above.

One thing not in Part 3 came out of doing it: `tests/cost.rs` was a growth
bound that only bound anything on macOS, and CI runs on Linux — so
`footprint_mb()` now answers out of `/proc/self/status` there. And one thing is
recorded rather than fixed: a single `SIGSEGV` from the test binary, seen once
and not in thirty runs since. `PROGRESS.md` has the two candidates.

# Recommendation

**Continue, and do the cheap platform work before the expensive feature work.**

The order matters, because the two structural risks left are both about
platforms and both are currently un-probed while the remaining feature work is
large and well-understood. Finishing markup on macOS and *then* discovering the
shell does not hold on Windows would be the worst available sequence.

1. ~~**Move `dioxus-reader` to a git dependency on `c6dec888`.**~~ Done, and
   the spike with it.
2. ~~**Add the three-platform CI job on `cargo test`.**~~ Done. What is not
   done is *reading the first run*, which is the whole point of it: until that
   job has been green once, "nothing here has run on Windows or Linux" is still
   the true sentence.
3. ~~**Write the composed-input test and retire the IME decision.**~~ Done, in
   about the hour this predicted.
4. **File the focus fault upstream** with the `is_focussable()` pointer from
   Part 3. It is the fault this tree has paid for four times and the fix is one
   call in a file that already has the predicate. Written up and not filed:
   that step needs an account rather than a commit.
5. **Get the shell onto Windows and Linux**, in that order of doubt. The named
   pipe for single instance is a known amount of work; the window lifecycle is
   the unknown one.
6. **Then item 10, markup** — and rebuild it as it should have been rather than
   porting the scaffolding around a hole that is gone.
7. **Then Phase 4**, the decision, with the same shape as the pdfium write-up.

**Finally, on the sentence this document started with.** It is right, and the
right way to read it is as a compliment to how little QRnew had to change rather
than a doubt about HyloPDF. The two apps went through the same door for
different reasons. QRnew walked through because the door was open. HyloPDF is
carrying a PDF renderer, a window manager, a theme engine and eleven thousand
lines of interface through it, because every other door was measured and found
shut.

---

## How the checks in Part 3 were made

Blitz source read at the pinned revision, from
`~/.cargo/git/checkouts/blitz-66635cc3152d32bd/c6dec88` and from the clone at
`~/rust_projects/blitz`, both at `c6dec888`:

- IME — `packages/blitz-dom/src/events/ime.rs`,
  `packages/blitz-shell/src/{window.rs:638, convert_events.rs:31, lib.rs:125}`
- the focus walk — `packages/blitz-dom/src/events/pointer.rs`, `handle_click`;
  the predicate at `packages/blitz-dom/src/node/element.rs:627`
- `pointer-events` — `packages/blitz-dom/src/node/node.rs:1299`, `git log -S`
  gives `85d072bc` (#465), an ancestor of `c6dec888`
- `overflow`, `position` — `packages/stylo_taffy/src/convert.rs:258-271`
- scrollbars — `packages/blitz-dom/src/node/scrollbar.rs`, `git log` gives
  `2e2330e7` (#489) and `9ac084ae` (#707)

QRnew side: `~/rust_projects/QRnew` on branch `dioxus-native`,
`tests/interface.rs:470` (`composed_text_reaches_the_field`), `Cargo.toml`
(the git rev), `src/ui.rs` and `src/ui.css`, and `git show c6b4b55:src/app.rs`
for the 532-line figure.

Line counts in Part 1 are `wc -l` over `src/*.ts`, `src/*.css` and
`src-tauri/src/*.rs` in this repository as of this writing. Every performance
and memory figure is quoted from `PROGRESS.md` and was measured there, not here.

## Sources

- `brief.md`, `dioxus-assessment.md`, `PROGRESS.md` in this directory
- `AGENTS.md` in this repository, and the `pdfium-prototype` branch
- `~/rust_projects/QRnew/dioxus-assessment.md`, including the addendum written
  after its branch was built
- [Blitz status](https://blitz.is/status/css) and
  [the repository](https://github.com/DioxusLabs/blitz)
