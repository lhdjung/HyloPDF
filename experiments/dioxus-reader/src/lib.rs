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
//! What is not built: no selection, no markup, no settings *window*, no
//! Keyboard page, no watchers, one window.
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
//! that had to move off the thread drawing the window lives.
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
//! themes on disk, five about `keys.toml`, eight about the library. Those are
//! not this crate's tests and this crate did not write them; they are the port
//! working.
//!
//! [`keys`] is the third of them and the most interesting, because its other
//! half is TypeScript. In the app, `keys.rs` owns the *file* and `keys.ts`
//! owns the meaning of a line, and the split had to be argued for across a
//! bridge; here `keys.rs` is mounted unchanged and [`keymap`] is `keys.ts`
//! ported beside it, so the same seam holds between two Rust modules. Nothing
//! about it had to move.
//!
//! [`library`] is the fourth, and it came across for the sidebar's sake: a
//! mark is a pin in a page and the pins have to be somewhere the next run can
//! read them. Where the reader was, what the document calls itself and what
//! was open last are read and written through it now as well — `remember`,
//! `touch`, `set_open` and `prune` — so the only part of the file with no
//! caller here is the markup journal, which waits for the item of Phase 3
//! that is about markup. It needed no change either.

pub mod app;
pub mod config;
pub mod crop;
/// Documents written by hand for the tests. Not in the binary either.
#[cfg(feature = "harness")]
pub mod fixture;
pub mod gpu;
/// Phase 2, and it is not in the binary: see the `harness` feature.
#[cfg(feature = "harness")]
pub mod harness;
pub mod keymap;
pub mod layout;
pub mod nav;
pub mod page;
pub mod palette;
pub mod pdfium;
pub mod recolor;
pub mod render;
pub mod search;
pub mod shell;
pub mod sidebar;
pub mod stats;
pub mod store;
pub mod styles;

// The app's own, unchanged. See the module comment above.
#[path = "../../../src-tauri/src/keys.rs"]
pub mod keys;
#[path = "../../../src-tauri/src/library.rs"]
pub mod library;
#[path = "../../../src-tauri/src/settings.rs"]
pub mod settings;
#[path = "../../../src-tauri/src/theme.rs"]
pub mod theme;

// `theme.rs` and `settings.rs` both write through this, and in the app it
// lives in `lib.rs` beside the commands. Re-exported at the crate root under
// the name they use, which is the whole of what mounting them costs.
pub use config::atomic_write;
