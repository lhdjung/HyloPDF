//! Where a window comes from, and what one gives back when it goes.
//!
//! This is `spawn_window` and the acting half of `hand_over` out of the app's
//! `lib.rs`. The *deciding* half is [`crate::windows`], which has no window in
//! it and is tested; this is the part that opens a file, builds a virtual DOM
//! and hands it to the shell, and it is the part a test cannot reach because
//! it makes a window.
//!
//! Three things about it are worth knowing before changing it.
//!
//! **A window is made for a document, never before one.** The app builds an
//! empty window and fills it, because it has a start screen to show in the
//! meantime; this reader has none — see item 7, "there is nowhere to show a
//! recently-read list in a reader that always has a document open" — so a
//! document that will not open produces no window at all rather than an empty
//! one, and ⌘N opens a second window on what the front one is reading.
//!
//! **Everything a window is told about is addressed to its label.** The label
//! goes into the [`Desk`], into [`crate::emit::Exchange`], and into
//! `watch.rs`'s per-window document watch, and those three are the whole of
//! what a window is to the rest of the process. Giving them back is
//! [`Session::tidy`], which is `tidy_after` under another name.
//!
//! **The reader's own `Store` is per window and is made inside it.** So two
//! windows have two copies of the settings table, and a setting changed in one
//! is not seen by the other until it is opened again — which is exactly what
//! `AGENTS.md` says about the app, and for the same reason. Themes are the
//! exception, because the watcher broadcasts them.

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::VirtualDom;
use dioxus_core::{provide_context, ScopeId};
use dioxus_native::{LogicalSize, WindowAttributes};

use crate::app::{Config, Handle, Reader, ReaderProps};
use crate::emit::{Exchange, Post};
use crate::page::Chosen;
use crate::shell::{Remote, WindowSpec};
use crate::windows::{Desk, Handover};
use crate::{palette, render, store};

/// Everything a window needs that is the *process's* rather than its own.
pub struct Session {
    pub desk: Desk,
    pub exchange: Exchange,
    /// One watcher, shared. See the hook in `app.rs`: a watcher per window
    /// would be that many watches on one themes directory and that many
    /// copies of every theme reload.
    pub watching: Arc<crate::watch::Watching>,
    pub dir: PathBuf,
    /// `--theme N`, which every window this run makes wears.
    pub theme: Option<usize>,
    pub size: (f64, f64),
    /// How a window in front is brought forward, which is the shell's to do
    /// and is asked for from here.
    pub remote: Remote,
}

impl Session {
    /// A window on this document, or nothing if it will not open.
    ///
    /// The claim on the document happens here, before the window exists, for
    /// the app's own reason: two files arriving in the same instant must not
    /// both be handed to the same window.
    pub fn window(&self, path: &str) -> Option<WindowSpec> {
        let document = match render::open(path) {
            Ok(document) => document,
            Err(refused) => {
                eprintln!("{refused}");
                return None;
            }
        };
        let label = self.desk.name();
        self.desk.set(&label, Some(path));
        // What the next launch comes back to, written as each window opens.
        let _ = crate::library::set_open(&self.dir, &self.desk.open());

        let post = Post::new();
        self.exchange.join(&label, post.clone());

        // The window wears the document's name, decided the way the toolbar
        // decides it — see `store::worth_calling`. It is settled here because
        // a window's title is an attribute given to the builder, and pdfium
        // answers at open, so there is nothing to gain by waiting.
        let called = store::called(path, &document.title());
        let attributes = WindowAttributes::default()
            .with_title(format!("{called} — HyloPDF"))
            .with_surface_size(LogicalSize::new(self.size.0, self.size.1));

        let config = Config {
            dir: self.dir.clone(),
            theme: self.theme,
            watch: true,
            window: label.clone(),
        };
        let vdom = VirtualDom::new_with_props(
            Reader,
            ReaderProps {
                document: Handle(document),
                // Black on white until this window reads the theme, which
                // happens during its first render and before anything is
                // painted. See `main.rs`.
                chosen: Chosen::new(palette::FALLBACK),
                config,
            },
        );
        let watching = self.watching.clone();
        let exchange = self.exchange.clone();
        vdom.in_scope(ScopeId::ROOT, move || {
            provide_context(post);
            provide_context(exchange);
            provide_context(watching);
        });
        Some(WindowSpec::new(label, vdom, attributes))
    }

    /// A window is showing a different document now, because the reader
    /// pressed ⌘O in it.
    ///
    /// The three things that belong to the process rather than to the window,
    /// in the order `Session::window` does them for a new one: who is showing
    /// what, what the next launch comes back to, and which file this window's
    /// watch is following. The window title is the fourth, and it is the
    /// window's own — there is nothing here that can reach it, and it is set
    /// where every other window attribute is.
    ///
    /// The document's own name is not asked for again here. It was read when
    /// the reader opened it and it is on the toolbar already; asking pdfium a
    /// second time would mean opening the file a second time to do it.
    pub fn showing(&self, label: &str, path: &str) {
        self.desk.set(label, Some(path));
        let _ = crate::library::set_open(&self.dir, &self.desk.open());
        self.watching.document(label, Some(path));
    }

    /// A document handed to us by the system — a second launch, "Open with",
    /// the command line — put where [`Desk::hand_over`] says it goes.
    ///
    /// `None` means no window is to be made, which covers both the document
    /// that is already open somewhere (that window comes forward instead) and
    /// the file that will not open.
    pub fn hand_over(&self, path: &str) -> Option<WindowSpec> {
        match self.desk.hand_over(path) {
            Handover::Front(label) => {
                self.remote.show(&label);
                None
            }
            // Unreachable in this reader and kept because the rule is right —
            // see [`Desk::hand_over`]. A window that exists and is showing
            // nothing has nothing to hand a document to *through*, so until
            // there is a start screen this is a window of its own as well.
            Handover::Fill(_) | Handover::Spawn => self.window(path),
        }
    }

    /// ⌘N, the Dock's "New Window", and a second launch with no document
    /// named: a window on whatever the front one is reading.
    ///
    /// **This is where the port stops being a port**, and the reason is the
    /// start screen. In the app a new window is an empty one, because there
    /// is something to show in an empty window; here there is not, so the
    /// choice is between a file picker and the document already in front of
    /// somebody — and the second is both the cheaper answer and a thing
    /// readers actually want, which is two places in one book at once. The
    /// picker is a door of its own (`rfd`, in the assessment's table) and
    /// belongs with the menus, which are not built.
    pub fn another(&self) -> Option<WindowSpec> {
        let showing = self
            .desk
            .front()
            .and_then(|label| self.desk.document_of(&label))
            .or_else(|| self.desk.open().into_iter().next())?;
        self.window(&showing)
    }

    /// What a window gives back when it goes: its entry in the library, its
    /// mailbox, and its place in the document watch.
    ///
    /// The write is the interesting one and the rule is [`Desk::closing`]'s:
    /// a window closed by the reader is a document they have finished with, a
    /// window closed because the app is quitting is not, and the last window
    /// cannot tell the two apart so it writes nothing.
    pub fn tidy(&self, label: &str) {
        self.watching.document(label, None);
        self.exchange.leave(label);
        if let Some(remaining) = self.desk.closing(label) {
            let _ = crate::library::set_open(&self.dir, &remaining);
        }
    }
}
