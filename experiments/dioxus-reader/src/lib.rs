//! The Dioxus Native experiment: a reader that reads, and the app's own Rust
//! underneath it.
//!
//! `experiments/dioxus-assessment.md` is the plan and `experiments/PROGRESS.md`
//! is what building it found — the status, the numbers, the rules the port
//! turned up, and the upstream faults it is written around.
//!
//! Two things about this crate are worth knowing before changing it.
//!
//! **Five of the app's own modules are mounted by `#[path]`, not copied** —
//! [`theme`], [`settings`], [`keys`], [`library`] and [`watch`] are
//! `src-tauri/src/`'s files, compiled here with nothing removed, and their
//! forty-four tests run with them. A copy would go stale, and a stale copy is
//! invisible: the file is right and what is on screen is the copy. The day one
//! of them grows a Tauri dependency, this crate stops compiling and says which
//! line did it.
//!
//! `watch.rs` costs three lines for that, at the bottom of this file: it says
//! `use tauri::{AppHandle, Emitter};`, so the crate answers to `tauri` as well
//! as to its own name and puts those two at its root. They live in [`emit`],
//! and they have Tauri's signatures rather than nicer ones because a shim
//! taking a nicer argument is a shim the app's file does not compile against,
//! which is the whole of what is being tested.
//!
//! **Phase 2 is [`harness`]** — the reader driven with no window and no
//! screen, behind the `harness` feature, which `cargo test` turns on and
//! `cargo build` does not.

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
// `src-tauri` pins an older `rust-version`, so clippy does not suggest
// `iter::repeat_n` there. Allowed rather than fixed: holding a file this crate
// does not own to a standard its own crate does not set is how a mounted
// module starts being edited.
#[allow(clippy::manual_repeat_n)]
#[path = "../../../src-tauri/src/watch.rs"]
pub mod watch;

// `theme.rs` and `settings.rs` write through this, and reach it at the crate
// root under the name they use in the app.
pub use config::atomic_write;

// `watch.rs` says `use tauri::{AppHandle, Emitter};`. See the module comment.
extern crate self as tauri;
pub use emit::{AppHandle, Emitter};
