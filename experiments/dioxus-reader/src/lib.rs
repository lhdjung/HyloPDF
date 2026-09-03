//! The Dioxus Native experiment: a reader that reads, and the app's own Rust
//! underneath it.
//!
//! `experiments/dioxus-assessment.md` is the plan and `experiments/PROGRESS.md`
//! is what the phases actually found. This crate is the reader: open a
//! document, scroll it, fit it, zoom it, put one of the app's fourteen themes
//! on it, and remember all of that between runs.
//!
//! **Phase 2 is here too, and it is [`harness`]** — the reader driven with no
//! window and no screen, which is what replaces the Playwright harness the app
//! tests through today. It is behind the `harness` feature, which `cargo test`
//! turns on and `cargo build` does not, so nothing it needs is in the binary.
//!
//! What is not built: nothing on the app's own list of things a reader can
//! ask for. The theme editor was the last of it and the password prompt was
//! the last of that — an encrypted document is asked about now rather than
//! refused, which is [`app::Locked`] and the window over it.
//!
//! **And one thing is built that the app has no counterpart for**, which is
//! the first time that has been true: [`sign`]. A reader draws their name
//! once, keeps it, and drops it onto a page as the specification's own
//! `/Ink` annotation. It is not parity and does not pretend to be — it is in
//! `tests/parity.rs` as a named exception, the way `keymap::EXTRA` names the
//! three keyboard actions this reader has and the app has not. It is also not
//! a *digital* signature and the window says so in its first sentence; see
//! `signing-assessment.md` for the two things that word means and why only one
//! of them is reachable from either renderer. The other one is *read* here,
//! though — [`sign::Seal`] is every `/Sig` the document already carries, and
//! its own comment is where the assessment's "signed by X, intact" is
//! corrected to the four facts that can honestly be had.
//!
//! Every one of the
//! app's forty-three keyboard actions answers here — the last three to arrive were dark mode,
//! help and print, which are the three that are about something outside the
//! document — so an action added to [`keymap`] and not handled in [`app`] is
//! a compile error rather than a sentence in the notice line.
//!
//! What *is* built, beyond opening a document and reading it: the document's
//! own links, with the page labels and the go-to field that belong to the same
//! question — where a document says its parts are — and the history that
//! following one needs. The sidebar is built —
//! [`sidebar`] — with the document's own contents, the reader's marks, a
//! column of thumbnails and the search results in it. [`search`] is built
//! too, and it is half the size of the app's because pdfium answers per
//! character: there is no text layer here and nothing measuring a DOM range
//! against one. The margins can be trimmed off ([`crop`]), the page can be
//! turned a quarter at a time, and the document can be read one page at a
//! time rather than continuously — the last of which is a line in
//! `settings.toml` and deliberately nothing else, because the brief says a
//! shortcut for it would be a thing to hit by accident. And what the reader
//! remembers between runs is no longer only settings: where they were in each
//! document, what each document calls itself, and which one was open last are
//! all in `library.toml` — see [`store`], which is also where the one write
//! that had to move off the thread drawing the window lives. Two of the files
//! it reads are watched, so a theme saved in an editor and a paper recompiled
//! by LaTeX both arrive without anybody asking for them. And words can be
//! swept with the pointer and copied — [`select`], which is the file the app
//! does not have because a webview comes with one, and the first place in this
//! port where that turned out to be worth having.
//!
//! # The app's own modules, compiled here unchanged
//!
//! The assessment's central claim about the Rust side is that roughly 2,450
//! lines of it port with no change at all, because nothing in them knows about
//! Tauri beyond an attribute. [`theme`] and [`settings`] are that claim tested
//! rather than asserted: they are `src-tauri/src/theme.rs` and
//! `src-tauri/src/settings.rs`, mounted here by path, with no copy in this
//! crate and nothing removed from them. Everything they need is
//! [`config::atomic_write`] and each other.
//!
//! A copy would have been the ordinary thing to do and it would have been
//! wrong, for the reason `AGENTS.md` gives about every other copy in this tree:
//! the copy goes stale, and a stale copy of a theme loader is invisible,
//! because the file is right and what is on screen is the copy. Mounting the
//! files means the experiment cannot drift from the app, and it means that the
//! day one of them grows a Tauri dependency, this crate stops compiling and
//! says which line did it — which is the signal worth having.
//!
//! Their own tests come with them, and `cargo test` runs them here: eleven
//! about the settings table, the write race and hand-edited files, six about
//! themes on disk, five about `keys.toml`, eight about the library, fourteen
//! about watching a directory. Those are not this crate's tests and this crate
//! did not write them; they are the port working.
//!
//! [`keys`] is the third of them and the most interesting, because its other
//! half is TypeScript. In the app, `keys.rs` owns the *file* and `keys.ts`
//! owns the meaning of a line, and the split had to be argued for across a
//! bridge; here `keys.rs` is mounted unchanged and [`keymap`] is `keys.ts`
//! ported beside it, so the same seam holds between two Rust modules. Nothing
//! about it had to move.
//!
//! And what changes on the disk while the reader is running — a theme edited
//! in an editor beside it, a paper recompiled underneath it — reaches the
//! screen without the reader asking: see [`watch`], which is the app's own
//! file, and [`emit`], which is the whole of what it took to mount it.
//!
//! [`library`] is the fourth, and it came across for the sidebar's sake: a
//! mark is a pin in a page and the pins have to be somewhere the next run can
//! read them. Where the reader was, what the document calls itself and what
//! was open last are read and written through it now as well — `remember`,
//! `touch`, `set_open` and `prune` — so the only part of the file with no
//! caller here is the markup journal, which waits for the item of Phase 3
//! that is about markup. It needed no change either.
//!
//! **There is more than one window now** — see [`windows`], which is the app's
//! own rules about which window a document goes to and what a window going
//! means, with every mention of a window taken out of them and a test against
//! each; [`session`], which is the half that actually makes one; and [`single`],
//! which is why a second launch hands its document to the reader that is
//! already running rather than becoming a second one. A window can also be put
//! into full screen with nothing on it at all, which is the last thing item 6
//! was waiting for.
//!
//! [`watch`] is the fifth, and it is the one this crate expected to have to
//! edit: it imports two names from Tauri and calls two methods on them, and
//! everything else in it is about the disk. So the names are supplied instead
//! — `extern crate self as tauri;` below, and [`emit`] — and the file is
//! mounted like the other four, with its fourteen tests. That is the whole of
//! the "`emit_to(window, …)` becomes an `EventLoopProxy::send_event`" the
//! assessment budgeted for, and it happened outside the file rather than
//! inside it.

pub mod app;
pub mod config;
pub mod crop;
/// The Dock's own menu, which is AppKit and exists nowhere else.
#[cfg(target_os = "macos")]
pub mod dock;
/// The three names the app's `watch.rs` reaches for, and the reason it can be
/// mounted rather than ported. See the module's own comment.
pub mod emit;
/// Documents written by hand for the tests. Not in the binary either.
#[cfg(feature = "harness")]
pub mod fixture;
pub mod gpu;
/// Phase 2, and it is not in the binary: see the `harness` feature.
#[cfg(feature = "harness")]
pub mod harness;
pub mod icons;
pub mod keymap;
pub mod layout;
pub mod markup;
pub mod nav;
pub mod page;
pub mod palette;
pub mod pdfium;
pub mod prefs;
pub mod recolor;
pub mod render;
pub mod search;
pub mod select;
pub mod session;
pub mod shell;
pub mod sidebar;
pub mod sign;
pub mod single;
pub mod stats;
pub mod steady;
pub mod store;
pub mod styles;
pub mod windows;

// The app's own, unchanged. See the module comment above.
#[path = "../../../src-tauri/src/keys.rs"]
pub mod keys;
#[path = "../../../src-tauri/src/library.rs"]
pub mod library;
#[path = "../../../src-tauri/src/settings.rs"]
pub mod settings;
#[path = "../../../src-tauri/src/theme.rs"]
pub mod theme;
// The one lint that fires here and not in the app, and it is a difference of
// standard rather than of code: `src-tauri` pins `rust-version = "1.77.2"`,
// which is older than `iter::repeat_n`, so clippy does not suggest it there.
// This crate has no such pin. Allowed rather than fixed, because holding a
// file this crate does not own to a standard its own crate does not set is
// how a mounted module starts being edited.
#[allow(clippy::manual_repeat_n)]
#[path = "../../../src-tauri/src/watch.rs"]
pub mod watch;

// `theme.rs` and `settings.rs` both write through this, and in the app it
// lives in `lib.rs` beside the commands. Re-exported at the crate root under
// the name they use, which is the whole of what mounting them costs.
pub use config::atomic_write;

// And `watch.rs` costs these three lines, which are the same trick one turn
// further: it says `use tauri::{AppHandle, Emitter};`, so this crate answers
// to `tauri` as well as to its own name and puts those two at its root. See
// [`emit`], which is where they actually live and why they have Tauri's
// signatures rather than nicer ones.
extern crate self as tauri;
pub use emit::{AppHandle, Emitter};
