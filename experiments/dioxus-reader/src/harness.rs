//! Phase 2: the reader, driven with no window and no screen.
//!
//! `scripts/ui-harness.mjs` in the app opens the interface in Playwright's
//! WebKit and offers `press`, `wheel`, `click`, `state()` and `screenshot()`
//! against it. It needs a dev server, a browser engine and a machine willing
//! to run one, and the two things it cannot do are the two the assessment
//! cares most about: it cannot test the rendering, and it cannot run where the
//! app actually runs.
//!
//! This is the replacement, and it is smaller than the thing it replaces
//! because most of it is upstream's. `blitz_test_harness::Harness` builds a
//! `DioxusDocument`, resolves style and layout against a stated viewport, and
//! synthesises pointer, wheel and key events through the *real* event
//! pipeline — no window, no GPU, no compositor. What this file adds is the
//! three things that are the reader's rather than Blitz's:
//!
//! *A reader to drive.* The document, the theme, the viewport and the
//! contexts the components expect, in one call.
//!
//! *`state()`*, which reads the interface the way somebody looking at it
//! would: the page number off the pill, the zoom off its chip, the theme off
//! the button that changes it. Two things are read from attributes instead,
//! because they have no pixels of their own — where the reader is scrolled to,
//! and which pages the mounting window is holding.
//!
//! *`screenshot()`*, which is the half the JS harness never had. Blitz paints
//! into an `anyrender::PaintScene`, and `vello_cpu` is a `PaintScene` that
//! rasterises on the CPU, deterministically, on any machine. Pages come out of
//! it too: `PageWidget` draws through `peniko::ImageData` when there is no
//! wgpu device behind the scene — see `Software` in `page.rs`, which is the
//! one piece of production code this file needed.
//!
//! ```no_run
//! use dioxus_reader::harness::Reader;
//! let mut reader = Reader::open("book.pdf");
//! reader.press("j");
//! assert!(reader.state().scroll > 0.0);
//! reader.save_png("/tmp/page.png");
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyrender::PaintScene;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::Document as _;
use blitz_test_harness::{Harness, HarnessOptions};
use blitz_traits::events::BlitzImeEvent;
use dioxus::prelude::VirtualDom;
use dioxus_core::{provide_context, ScopeId};
use dioxus_native::DioxusDocument;
use keyboard_types::{Code, Key, Modifiers};
use peniko::kurbo::Rect;
use peniko::{Color, Fill};

use crate::app::{Away, Config, Handle, Reader as ReaderComponent, ReaderProps, Screen};
use crate::page::Chosen;
use crate::palette;
use crate::render::{self, PageSource};

/// How the reader is opened. The defaults are a window a book is comfortable
/// in, at a density of one — a test wants the same pixels on every machine,
/// and a retina factor of two quadruples what the CPU rasteriser has to do.
pub struct Options {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// A place in the theme list, as `--theme` takes. `None` is whatever the
    /// settings say, which in a fresh directory is Hylo Light.
    pub theme: Option<usize>,
    /// `keys.toml`, as a table: action name against the keys it should
    /// answer to. Written into the config directory before the reader opens,
    /// so what is exercised is the real path — the app's own `keys::load`,
    /// reading a real file off a real disk — rather than a table handed
    /// straight to the keymap. `openApp({ keys: … })` in the app's harness is
    /// the same trick against the browser twin of the same loader.
    pub keys: BTreeMap<String, Vec<String>>,
    /// Settings written into the config directory before the reader opens,
    /// as key against value.
    ///
    /// The twin of `openApp({ settings })` in the app's own harness, and it
    /// exists for the same reason: some of what this reader does is not
    /// reachable by pressing anything. `scroll_mode` is the case in point —
    /// the brief says continuous scrolling may only ever change if the reader
    /// explicitly opts into it, so there is deliberately no key and no chip,
    /// and a line in `settings.toml` is the whole of the interface for it.
    ///
    /// Written through the app's own [`crate::settings::set_many`], so what a
    /// test exercises is the real loader reading a real file.
    pub settings: Vec<(String, serde_json::Value)>,
    /// Whether the real watcher runs behind this reader — the app's own
    /// `watch.rs`, on a thread, over the themes directory and the document.
    ///
    /// **Off by default, and a test should think before turning it on.** The
    /// watcher thread cannot be stopped (see [`crate::app::Config::watch`]),
    /// so every reader that starts one leaves one behind for the rest of the
    /// run, and what it then reports arrives at the speed of the file system
    /// rather than of the test. The deterministic way to test what the reader
    /// *does* about news is [`Reader::deliver`], which posts the same news
    /// the watcher would; this is for the one test that has to prove the
    /// watcher is really wired to it.
    pub watch: bool,
    /// What the picker answers with, in order — one path per Open, and
    /// nothing left means the reader cancelled.
    ///
    /// **The one door in this crate a test must not be able to open for
    /// real**: `Pick`'s default is the system's own file dialog, which is a
    /// modal window that would sit there until somebody clicked it. Same
    /// shape and same reason as [`crate::app::Clip`], one step further —
    /// a clipboard takes something away, a modal window takes the run.
    pub picks: Vec<String>,
    /// What the machine says about light and dark before the reader opens.
    ///
    /// `None` is a machine that will not say, which is winit's own `None` and
    /// this harness's default — so a test that does not care about appearance
    /// gets a reader that follows nothing, whatever `follow_system_theme`
    /// happens to be. `openApp({ appearance: "dark" })` in the app's harness
    /// is the twin, and it defaults to light rather than to nothing, because
    /// a browser has no third answer.
    pub appearance: Option<bool>,
    /// Where this reader's settings and themes live.
    ///
    /// **A directory of its own per reader, and that is not fastidiousness.**
    /// `cargo test` runs test functions in parallel and every one of them
    /// changes settings — a theme, a zoom, a fit mode — so a shared directory
    /// would have tests writing over each other's table and reading back
    /// somebody else's answer, intermittently and by timing. It is also what
    /// keeps a test run away from the reader's own settings, which are in the
    /// directory the real binary uses.
    pub config: PathBuf,
}

/// A directory nothing else in this process is using.
fn scratch_config() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "hylopdf-harness-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: 1100,
            height: 900,
            scale: 1.0,
            theme: None,
            keys: BTreeMap::new(),
            settings: Vec::new(),
            picks: Vec::new(),
            watch: false,
            appearance: None,
            config: scratch_config(),
        }
    }
}

/// What the interface says about itself, read off the interface.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// The page the reader is on, off the field in the toolbar.
    ///
    /// **0 for a document that numbers its pages its own way**, because what
    /// the field holds is then "vii" and not a number at all — which is the
    /// interface being right rather than this being wrong: a reader looking at
    /// the front matter of a book is on page vii, and there is nowhere on
    /// screen that says it is also the seventh thing in the file. `label` is
    /// what the field actually says, and is the assertion to write for a
    /// document with labels.
    pub page: usize,
    /// What the page field says, verbatim.
    pub label: String,
    pub pages: usize,
    /// "Fit width", "Fit page", "150%" — whatever the chip says.
    pub zoom: String,
    pub theme: String,
    pub notice: String,
    /// Where the document has been moved to, in CSS pixels.
    pub scroll: f64,
    /// Which pages are in the DOM, one-based and in order. This is the
    /// mounting window, observed from outside.
    pub mounted: Vec<usize>,
    /// Whether the panel on the left is open, and which tab it is showing —
    /// "contents", "pages", or nothing at all when it is shut.
    pub sidebar: Option<String>,
    /// The panel's own width, in CSS pixels — 0.0 when it is shut. Read off
    /// its layout rather than off `sidebar_width` in the state, because a
    /// drag test is asking what the reader would actually see.
    pub sidebar_width: f64,
    /// Which thumbnails are in the DOM, one-based and in order: the column's
    /// own mounting window, which is what replaces `THUMB_CACHE`.
    pub thumbs: Vec<usize>,
    /// What the find bar says beside the field — "3 of 12", "None",
    /// "Searching…" — or `None` when the bar is not up.
    ///
    /// Off the interface, like everything else here: it is the sentence
    /// somebody searching would read, not a field of the `Search`.
    pub find: Option<String>,
    /// What is in the field.
    pub query: String,
    /// How many match rectangles are painted over the mounted pages. Not the
    /// number of matches — that is in `find` — but the number the reader can
    /// actually see, which is what "Highlight all" changes and the only thing
    /// that says the highlighting reached the page.
    pub hits: usize,
    /// Which results the list is showing, by their place in the whole list.
    pub results: Vec<usize>,
    /// What the document is called, off the toolbar: its own `/Title` where
    /// that is worth having, and the file's name where it is not.
    pub title: String,
    /// Whether the window is showing the start screen rather than a document.
    pub empty: bool,
    /// The recently-read list on the start screen, as it reads: the name and
    /// the page, one string a row. Empty when there is no start screen, which
    /// is what the reader would say too.
    pub recents: Vec<String>,
    /// What the drop hint says while something is being dragged over the
    /// window, or nothing at all. See [`crate::app::Viewer::dragging`].
    pub dragging: Option<String>,
    /// Whether the toolbar is on screen. ⌘T puts it away and presenting takes
    /// it with everything else — see [`crate::app::Viewer::chrome`]. Nearly
    /// every other field here is read *off* the toolbar, so a test that hides
    /// it is asserting on an empty string unless it means to.
    /// Which toolbar menu is down — "document", "theme", "view" — and nothing
    /// when none is. Read off the panel's own class, which is what a reader
    /// sees, rather than off the state behind it.
    pub menu: Option<String>,
    pub toolbar: bool,
    /// Full screen with nothing else on it.
    pub presenting: bool,
}

/// A reader with no window, driven by hand.
pub struct Reader {
    pub harness: Harness<DioxusDocument>,
    /// Kept because the harness borrows nothing of it and a test may want to
    /// ask the document a question the interface does not answer.
    pub document: Arc<dyn PageSource>,
    /// The settings directory this reader was given, so that a test can open
    /// a second reader on the same one and check that something was
    /// remembered.
    pub config: PathBuf,
    width: u32,
    height: u32,
    scale: f64,
    /// The window's size as the reader itself asks for it. A `Cell` rather
    /// than the two numbers above because [`Screen`] is a closure the
    /// component holds and reads whenever it likes — which is the whole shape
    /// of the thing in the app, where it reads winit. See [`Reader::resize`].
    sizing: Rc<std::cell::Cell<(f64, f64, f64)>>,
    /// What the machine says about light and dark, read the same way and for
    /// the same reason. See [`Reader::set_appearance`].
    outside: Rc<std::cell::Cell<Option<bool>>>,
    opened: Rc<RefCell<Vec<String>>>,
    /// The mailbox the reader's watch task listens on. See [`Reader::deliver`].
    post: crate::emit::Post,
    /// Everything this reader has asked its window for, in order. See
    /// [`crate::app::Frame`]: there is no window here, so the asks are
    /// written down instead — which is how "⌘N asks for a window" and "Escape
    /// leaves full screen" are tests rather than things somebody checked once
    /// by hand in a running app.
    asks: Rc<RefCell<Vec<crate::app::Ask>>>,
    /// Everything this reader has copied, in order. See [`crate::app::Clip`]:
    /// the real one is the machine's clipboard, and a suite that took it would
    /// empty the clipboard of whoever is running `cargo test`.
    copied: Rc<RefCell<Vec<String>>>,
    /// Every document this reader handed over to print. See
    /// [`crate::app::Printer`].
    printed: Rc<RefCell<Vec<String>>>,
}

impl Reader {
    /// Tell this reader that something on the disk changed, exactly as the
    /// watcher would.
    ///
    /// The news is the news `watch.rs` emits, in the shape it emits it —
    /// `themes-changed` with the whole set, `document-changed` with a path —
    /// so what is being tested is the reader's half of a real wire and not a
    /// method called directly. What it skips is the file system: a test that
    /// wants the *watcher* tested asks for `watch: true` and writes a file.
    ///
    /// Pumps afterwards, because the task is woken rather than run: the wake
    /// marks it ready and the next turn of the loop is what runs it.
    pub fn deliver(&mut self, news: crate::emit::News) {
        self.post.send(news);
        self.settle();
    }

    /// A document dragged over the window, exactly as winit reports it —
    /// `true` for one this reader would open, `false` for anything else.
    ///
    /// The news `main.rs` turns `WindowEvent::DragEntered` into. What it
    /// skips is winit, which is the same line [`Reader::deliver`] draws for
    /// the watcher: there is no window here, and the half that is worth
    /// testing is what the reader does about the news.
    pub fn drag_over(&mut self, takeable: bool) {
        self.deliver(crate::emit::News {
            event: "drag-over".into(),
            target: None,
            payload: serde_json::Value::Bool(takeable),
        });
    }

    /// The drag left the window without anything being let go.
    pub fn drag_left(&mut self) {
        self.deliver(crate::emit::News {
            event: "drag-left".into(),
            target: None,
            payload: serde_json::Value::Null,
        });
    }

    /// Something was let go on the window that this reader will not open.
    pub fn drag_refused(&mut self) {
        self.deliver(crate::emit::News {
            event: "drag-refused".into(),
            target: None,
            payload: serde_json::Value::Null,
        });
    }

    /// A document let go on the window, or handed to it by the process —
    /// which are the same news, because they are the same thing happening.
    pub fn hand_over(&mut self, path: &str) {
        self.deliver(crate::emit::News {
            event: "open-document".into(),
            target: None,
            payload: serde_json::Value::String(path.to_string()),
        });
    }

    /// The whole theme set, again — what a saved theme file causes.
    pub fn themes_changed(&mut self, themes: &[crate::theme::Theme]) {
        self.deliver(crate::emit::News {
            event: "themes-changed".into(),
            target: None,
            payload: serde_json::to_value(themes).expect("themes are serialisable"),
        });
    }

    /// The window is a different size, exactly as dragging its edge makes it.
    ///
    /// Three things happen and all three are what winit and the shell do:
    /// Blitz's own viewport moves, so the chrome is laid out against the new
    /// window; the number the reader reads when it asks how big the window is
    /// moves with it; and the news goes down the mailbox, which is the half
    /// `Shell::on_resized` supplies in the app. Leave any one of them out and
    /// this tests something other than the fault it exists for.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.sizing.set((width as f64, height as f64, self.scale));
        self.harness.set_viewport_size(width, height);
        self.deliver(crate::emit::News {
            event: "window-resized".into(),
            target: Some(crate::windows::MAIN.into()),
            payload: serde_json::Value::Null,
        });
    }

    /// The machine went light or dark, exactly as changing it in System
    /// Settings does.
    ///
    /// Two halves, as [`Reader::resize`] has: the answer the reader gets when
    /// it asks moves, and the news goes down the mailbox — which is
    /// `Shell::on_theme` in the app. Leave the first out and the reader asks
    /// and hears the old answer; leave the second out and it never asks.
    pub fn set_appearance(&mut self, dark: Option<bool>) {
        self.outside.set(dark);
        self.deliver(crate::emit::News {
            event: "appearance-changed".into(),
            target: Some(crate::windows::MAIN.into()),
            payload: serde_json::Value::Null,
        });
    }

    /// The open document was rewritten — what a recompile causes.
    pub fn document_changed(&mut self, path: &str) {
        self.deliver(crate::emit::News {
            event: "document-changed".into(),
            target: Some(crate::windows::MAIN.into()),
            payload: serde_json::Value::String(path.to_string()),
        });
    }

    /// Turn the loop until `ready` says so, or until it plainly is not going
    /// to — with a real clock, because the thing being waited for is a real
    /// file system.
    ///
    /// The only test that needs this is the one with the real watcher behind
    /// it: `notify` reports when the platform tells it to, and the app's own
    /// `SETTLE` and `STEADY` are a quarter of a second between them. Every
    /// other test in this suite waits for a turn of the loop and no longer.
    ///
    /// Answers whether it happened, so that a test that gives up says what
    /// the reader was stuck on rather than that a timer went off.
    pub fn wait_until(&mut self, seconds: f64, ready: impl Fn(&mut Reader) -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
        loop {
            if ready(self) {
                return true;
            }
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.settle();
        }
    }

    /// Everything this reader has asked of the window it does not have.
    pub fn asks(&self) -> Vec<crate::app::Ask> {
        self.asks.borrow().clone()
    }

    /// Every address this reader has handed to the system, in order — see
    /// [`crate::app::Away`]. Empty unless a link out of the document has been
    /// followed, and nothing is ever actually opened.
    pub fn opened(&self) -> Vec<String> {
        self.opened.borrow().clone()
    }

    /// Everything this reader has copied, in order — see [`crate::app::Clip`].
    /// Nothing reaches the machine's own clipboard.
    /// Every document this reader handed to a program that prints, in order.
    /// Nothing was opened; see [`crate::app::Printer`].
    pub fn printed(&self) -> Vec<String> {
        self.printed.borrow().clone()
    }

    pub fn copied(&self) -> Vec<String> {
        self.copied.borrow().clone()
    }

    /// How big the window is, in CSS pixels. What a test needs to know how
    /// much of a page taller than the window is actually on screen.
    pub fn window(&self) -> (f64, f64) {
        (self.width as f64, self.height as f64)
    }

    /// Open a document with the default options.
    pub fn open(path: &str) -> Self {
        Self::open_with(path, Options::default())
    }

    /// The fixture the app's own test suite generates: 400 pages of plain
    /// text. It is the document every memory number in `PROGRESS.md` was taken
    /// on, which is the reason to reach for it here too.
    pub fn book() -> String {
        format!(
            "{}/../../tests/fixtures/book.pdf",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// A window with nothing in it — the start screen, as ⌘N gives it and as
    /// closing a document leaves it.
    ///
    /// Over [`crate::render::Nothing`], which is the same document the real
    /// empty window holds: there is no "no document" state to fake, and that
    /// is the whole reason the empty document exists.
    pub fn empty(options: Options) -> Self {
        Self::over(crate::render::nothing(), options)
    }

    pub fn open_with(path: &str, options: Options) -> Self {
        let document = render::open(path).unwrap_or_else(|err| panic!("{err}"));
        Self::over(document, options)
    }

    /// The same, over a document already open — which is how a test opens one
    /// document and drives several readers over it.
    pub fn over(document: Arc<dyn PageSource>, options: Options) -> Self {
        write_keys(&options.config, &options.keys);
        write_settings(&options.config, &options.settings);
        // Corrected in `Viewer::new` during the first render, before anything
        // is painted. See `main.rs`, which does the same.
        let chosen = Chosen::new(palette::FALLBACK);
        let config = Config {
            dir: options.config.clone(),
            theme: options.theme,
            watch: options.watch,
            window: crate::windows::MAIN.to_string(),
        };
        // The mailbox the reader listens on, made here so that a test can put
        // news in it. With `watch` off this is the only thing that ever does.
        let post = crate::emit::Post::new();
        let vdom = VirtualDom::new_with_props(
            ReaderComponent,
            ReaderProps {
                document: Handle(document.clone()),
                chosen,
                config,
            },
        );
        // What the shell provides out of the winit window, provided out of the
        // numbers instead. Nothing else the reader consumes is required: the
        // shell provider is asked for with `try_consume_context`, and without
        // one a page simply does not ask for a frame it is not going to get.
        let sizing = Rc::new(std::cell::Cell::new((
            options.width as f64,
            options.height as f64,
            options.scale as f64,
        )));
        let asked_size = sizing.clone();
        let outside = Rc::new(std::cell::Cell::new(options.appearance));
        let asked_appearance = outside.clone();
        let scale = options.scale as f64;
        // Where a link out of the document would have gone, written down
        // instead of opened. The default is the system browser and is right in
        // the app; a suite that took it would open a browser window on
        // whoever is running `cargo test`.
        let opened: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let recorder = opened.clone();
        let posting = post.clone();
        // And what this reader asked its window to do, which in the app is
        // answered against winit and here is a list.
        let asks: Rc<RefCell<Vec<crate::app::Ask>>> = Rc::new(RefCell::new(Vec::new()));
        let asked = asks.clone();
        // …and what it copied, for the same reason and with more at stake: a
        // clipboard is somebody's and taking it is worse than adding a window.
        let copied: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let copying = copied.clone();
        // …and what it handed to a program that prints, for the same reason
        // one step further again: a clipboard takes something away, a picker
        // takes the run, and printing takes somebody's paper.
        let printed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let printing = printed.clone();
        // …and what the picker will answer with, taken from the front of the
        // list one Open at a time.
        let picks: Rc<RefCell<std::collections::VecDeque<String>>> =
            Rc::new(RefCell::new(options.picks.iter().cloned().collect()));
        let picking = picks.clone();
        vdom.in_scope(ScopeId::ROOT, move || {
            provide_context(posting);
            provide_context(Screen::new(move || asked_size.get()));
            provide_context(crate::app::Appearance::new(move || asked_appearance.get()));
            provide_context(Away::new(move |url| {
                recorder.borrow_mut().push(url.to_string());
            }));
            provide_context(crate::app::Frame::new(move |ask| {
                asked.borrow_mut().push(ask);
            }));
            provide_context(crate::app::Clip::new(move |text| {
                copying.borrow_mut().push(text.to_string());
            }));
            provide_context(crate::app::Printer::new(move |path| {
                printing.borrow_mut().push(path.to_string());
                Ok(())
            }));
            provide_context(crate::app::Pick::new(move || {
                picking.borrow_mut().pop_front()
            }));
        });

        // What this window is showing, which is the one thing `Store::opened`
        // used to write and stopped writing when there was more than one
        // window: an entry per window is the process's business, not a
        // store's. `Session::window` does this in the app; here there is one
        // reader and it is this line. See `store::opened`.
        let path = document.path().to_string();
        if !path.is_empty() {
            let _ = crate::library::set_open(&options.config, std::slice::from_ref(&path));
        }

        let harness = Harness::from_vdom(
            vdom,
            HarnessOptions {
                width: options.width,
                height: options.height,
                scale: options.scale,
                ..Default::default()
            },
        );

        let mut reader = Reader {
            harness,
            document,
            config: options.config,
            width: options.width,
            height: options.height,
            scale,
            sizing,
            outside,
            opened,
            post,
            asks,
            copied,
            printed,
        };
        reader.focus_root();
        reader.settle();
        reader
    }

    /// Give the reader's root the keyboard.
    ///
    /// The component asks for it on mount, through `MountedData::set_focus`,
    /// and that is a round trip through a spawned task and the shell — which
    /// there is not one of here. So the harness does it directly, which is
    /// also the honest thing: what is being tested is that a key pressed at
    /// the reader does what it should, not that focus arrives by one route
    /// rather than another.
    fn focus_root(&mut self) {
        if let Some(root) = self.harness.query(".root") {
            self.harness.base_mut().set_focus_to(root);
        }
        self.harness.pump();
    }

    /// Write down where the reader is, now, rather than when the scrolling
    /// stops.
    ///
    /// A position is the one thing this reader writes on a thread of its own
    /// and after a pause — see `Scribe` in `store.rs` — so a test that opens
    /// a second reader on the same directory has to say when the first one has
    /// finished. It is the same call quitting makes, which is what makes this
    /// the real path rather than a hook for the tests.
    pub fn flush(&self) {
        crate::store::flush();
    }

    /// Let everything that is going to happen, happen.
    ///
    /// Nothing here waits on a clock: `pump` polls the virtual DOM and
    /// resolves style and layout, and the only reason to do it more than once
    /// is that one round of that can produce work for the next — a resize that
    /// relays out, a relayout that mounts a page. Three is empirically enough
    /// and costs microseconds; the alternative is a sleep, which is the thing
    /// the app's own test suite spent a day removing.
    pub fn settle(&mut self) {
        for _ in 0..3 {
            self.harness.pump();
        }
    }

    /// Press a key, by the name the DOM gives it: "j", "ArrowDown", "Home",
    /// " ". A single character is a character; anything longer is looked up.
    pub fn press(&mut self, key: &str) {
        self.press_with(key, Modifiers::default());
    }

    pub fn press_with(&mut self, key: &str, modifiers: Modifiers) {
        let key = parse_key(key);
        self.harness.press_with(key.clone(), modifiers);
        self.apple_binding(&key, modifiers);
        self.give_keyboard_back();
        self.settle();
    }

    /// What AppKit sends *instead of* an editing key, on a Mac.
    ///
    /// **Backspace is not a key there.** AppKit reads the editing keys against
    /// the standard key bindings and calls `doCommandBySelector:` with a name
    /// — `deleteBackward:` — which winit surfaces through a callback of its
    /// own and `blitz-shell` turns into `UiEvent::AppleStandardKeybinding`.
    /// `blitz-dom` knows this and says so: its `Key::Backspace` arm is
    /// `#[cfg(not(target_os = "macos"))]`, so on a Mac the keystroke alone
    /// deletes nothing at all.
    ///
    /// A harness has no AppKit, so it has to send the second half itself, and
    /// it must — the app's whole editing story is macOS-only otherwise, and
    /// the fault this exists to cover was exactly that: `Shell` did not
    /// forward the callback, so nothing anybody typed could be taken back
    /// out. See `ApplicationHandlerExtMacOS for Shell` in `shell.rs`.
    ///
    /// Only the two a reader of this app presses. Everything else in a text
    /// field here is a letter, an arrow or Escape, and those arrive as
    /// keystrokes on every platform.
    #[cfg(target_os = "macos")]
    fn apple_binding(&mut self, key: &Key, modifiers: Modifiers) {
        use blitz_traits::events::UiEvent;
        let command = match key {
            Key::Backspace if modifiers.alt() => "deleteWordBackward:",
            Key::Backspace => "deleteBackward:",
            Key::Delete if modifiers.alt() => "deleteWordForward:",
            Key::Delete => "deleteForward:",
            _ => return,
        };
        self.harness
            .dispatch(UiEvent::AppleStandardKeybinding(command.into()));
        self.harness.pump();
    }

    #[cfg(not(target_os = "macos"))]
    fn apple_binding(&mut self, _key: &Key, _modifiers: Modifiers) {}

    /// Press a chord, written the way `keys.toml` writes one: "mod+0",
    /// "shift+g", "alt+left", "g".
    ///
    /// This is the shape a test wants, because it is the shape the binding it
    /// is testing is written in — and it keeps the platform out of the test:
    /// `mod` is ⌘ here and Ctrl on the machine CI runs on, exactly as it is
    /// for the reader. `MOD` in the app's harness exists for the same reason.
    pub fn press_chord(&mut self, chord: &str) {
        let (key, code, modifiers) = spell_out(chord);
        self.press_coded(key, code, modifiers);
    }

    /// A keystroke with a *physical key* behind it as well as a character.
    ///
    /// The one case that needs it is the one the app found the hard way:
    /// Option is not a letter on a Mac, so ⌥⌘G arrives as ©, and what makes
    /// the chord readable is `event.code` still saying `KeyG`. Upstream's
    /// `key_event` sends `Code::Unidentified`, which is right for typing and
    /// cannot express this.
    pub fn press_coded(&mut self, key: Key, code: Code, modifiers: Modifiers) {
        use blitz_test_harness::key_event;
        use blitz_traits::events::{KeyState, UiEvent};

        let key_for_binding = key.clone();
        let mut down = key_event(key.clone(), KeyState::Pressed, modifiers);
        down.code = code;
        let mut up = key_event(key, KeyState::Released, modifiers);
        up.code = code;
        self.harness.dispatch(UiEvent::KeyDown(down));
        self.harness.dispatch(UiEvent::KeyUp(up));
        self.harness.pump();
        self.apple_binding(&key_for_binding, modifiers);
        self.give_keyboard_back();
        self.settle();
    }

    /// Type into whatever has the keyboard, one character at a time, the way
    /// somebody typing does — so the reader sees an `input` event per letter
    /// and starts a scan per letter, which is the shape the slicing exists
    /// for and the shape a test should exercise.
    pub fn type_text(&mut self, text: &str) {
        self.harness.type_text(text);
        self.settle();
    }

    /// Compose a word the way an input method does: a run of preedits, then
    /// the text itself.
    ///
    /// **This is what a reader writing Japanese, Chinese or Korean actually
    /// does**, and until this method existed nothing in either of this
    /// experiment's documents could say whether the find bar answered it.
    /// Both listed IME as the one item that needed a decision rather than a
    /// workaround, on the strength of there being no composition events. There
    /// are: `packages/blitz-dom/src/events/ime.rs` takes the focused node's
    /// editor and applies the event through Parley, and `blitz-shell` routes
    /// winit's four `WindowEvent::Ime` variants into it.
    ///
    /// `stages` is what the candidate window shows on the way — "に",
    /// "にほん", "にほんご" — and `text` is what is chosen. Every stage is a
    /// `Preedit`; the choice is an **empty preedit and then a `Commit`**,
    /// because that is winit's own contract: "right before this event winit
    /// will send empty [`Preedit`]". A test that leaves the empty one out
    /// composes on top of its own composing region and gets にほん日本語,
    /// which reads exactly like a Blitz fault and is not one.
    pub fn compose(&mut self, stages: &[&str], text: &str) {
        for stage in stages {
            self.preedit(stage);
        }
        self.preedit("");
        self.harness.ime(BlitzImeEvent::Commit(text.to_string()));
        self.settle();
    }

    /// The commit on its own, without the empty preedit `compose` sends
    /// first. Here so that a test can show what winit's contract is *for* —
    /// see `tests/ime.rs` — and for nothing else: a reader's input method
    /// always sends the empty one.
    pub fn commit(&mut self, text: &str) {
        self.harness.ime(BlitzImeEvent::Commit(text.to_string()));
        self.settle();
    }

    /// One stage of a composition, shown in the field and committed to
    /// nothing. An empty string clears the composing region.
    ///
    /// A preedit generates no `input` event — `apply_generated_text_input_event`
    /// answers `PreEditChange` with a redraw and nothing else — so the reader
    /// does not search for a half-typed romaji. A browser does fire `input`
    /// during a composition, with `isComposing` set for the application to
    /// check, and the app never checked it; here the right behaviour is what
    /// arrives.
    pub fn preedit(&mut self, stage: &str) {
        let cursor = Some((stage.len(), stage.len()));
        self.harness
            .ime(BlitzImeEvent::Preedit(stage.to_string(), cursor));
        self.settle();
    }

    /// Read pages until the scan has finished, or until it plainly is not
    /// going to.
    ///
    /// **The scan is a task and a task is polled by whoever drives the
    /// document**, which in the real app is the event loop being woken and
    /// here is `pump()`. So a test that wants the whole document searched
    /// pumps until the bar stops saying so — the same "wait for the
    /// condition, not for the clock" rule the app's own harness spent a day
    /// learning, with the clock replaced by a turn of the loop.
    pub fn scan_out(&mut self) {
        for _ in 0..4000 {
            let said = self.state().find.unwrap_or_default();
            if !said.ends_with('…') && said != "Searching…" {
                return;
            }
            self.harness.pump();
        }
    }

    /// Turn the wheel over the document. Positive reads forwards, which is the
    /// direction the reader thinks in and the opposite of the sign winit uses.
    pub fn wheel(&mut self, by: f64) {
        let (x, y) = (self.width as f32 / 2.0, self.height as f32 / 2.0);
        self.harness.wheel_at(x, y, 0.0, -by);
        self.settle();
    }

    /// Turn the wheel *across* the document, which on a Mac is two fingers
    /// sideways and ⇧-wheel both — AppKit turns the second into the first
    /// before winit sees it. Positive reads rightwards. Nothing moves unless
    /// the reader has zoomed past the width of the window.
    pub fn wheel_across(&mut self, by: f64) {
        let (x, y) = (self.width as f32 / 2.0, self.height as f32 / 2.0);
        self.harness.wheel_at(x, y, -by, 0.0);
        self.settle();
    }

    /// Turn the wheel over something in particular — the thumbnail column,
    /// which scrolls on its own and is not where the middle of the window is.
    pub fn wheel_over(&mut self, selector: &str, by: f64) {
        let (x, y) = self.harness.center_of(selector);
        self.harness.wheel_at(x, y, 0.0, -by);
        self.settle();
    }

    /// One screenful, which is what a reader means by turning the wheel.
    pub fn wheel_screen(&mut self) {
        let screen = self.height as f64 - crate::app::CHROME;
        self.wheel(screen * 0.9);
    }

    /// Click the first element matching a CSS selector — ".chip", ".pill".
    pub fn click(&mut self, selector: &str) {
        self.harness.click(selector);
        self.give_keyboard_back();
        self.settle();
    }

    /// Click a point in the window, which is what a test does when the thing
    /// to be clicked is the seventh row of a list rather than a selector.
    pub fn click_at(&mut self, x: f32, y: f32) {
        self.harness.click_at(x, y);
        self.give_keyboard_back();
        self.settle();
    }

    /// Pick up the panel's edge and carry it `by` pixels — negative narrows,
    /// positive widens. Three real events, the same three a pointer sends,
    /// because `drag_sidebar` reads its distance off the two ends of the
    /// drag and nothing shorter would exercise that.
    pub fn drag_sidebar_edge(&mut self, by: f32) {
        let (x, y) = self.harness.center_of(".sidebar-resize");
        self.harness.mouse_down_at(x, y);
        self.harness.move_mouse_to(x + by, y);
        self.harness.mouse_up_at(x + by, y);
        self.settle();
    }

    /// Sweep the pointer from one point in the window to another, which is
    /// what selecting text is.
    ///
    /// The same three events a pointer sends, because the selection is read
    /// off all three: the press decides where the content is (see
    /// `Viewer::sweep_from`), the moves extend it, and the release is what
    /// turns a sweep that covered nothing into no selection at all. The
    /// intermediate move is not decoration either — a sweep of one jump is a
    /// sweep no reader makes, and a bug that only appears on the second move
    /// would never be seen.
    pub fn sweep(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.harness.mouse_down_at(from.0, from.1);
        let middle = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        self.harness.move_mouse_to(middle.0, middle.1);
        self.harness.move_mouse_to(to.0, to.1);
        self.harness.mouse_up_at(to.0, to.1);
        self.give_keyboard_back();
        self.settle();
    }

    /// A sweep across one line of a page, given as fractions of that page's
    /// box: a test says "a fifth of the way down, from a tenth across to
    /// nine tenths" and does not have to know where the page is on screen.
    ///
    /// The page is found by its `data-page` attribute, which is the one thing
    /// the DOM says about a page that a reader could also check.
    pub fn sweep_page(&mut self, page: usize, from: (f32, f32), to: (f32, f32)) {
        let (from, to) = (self.point_on(page, from), self.point_on(page, to));
        self.sweep(from, to);
    }

    /// Two clicks in the same place, quickly enough to be one gesture.
    ///
    /// Blitz decides that from the clock and the distance — under half a
    /// second and within two pixels — so this is two ordinary clicks and no
    /// synthesised event. Which is worth having: what is being tested is that
    /// a reader double-clicking a word gets the word, not that a `dblclick`
    /// handler runs when one is posted.
    pub fn double_click_on(&mut self, page: usize, at: (f32, f32)) {
        let (x, y) = self.point_on(page, at);
        self.harness.click_at(x, y);
        self.harness.click_at(x, y);
        self.give_keyboard_back();
        self.settle();
    }

    /// Where a point given as fractions of a page's box is, in the window.
    ///
    /// The page is found by its `data-page` attribute, which is the one thing
    /// the DOM says about a page that a reader could also check — see the
    /// comment on it in `app.rs`, which is there because the mounting window
    /// is invisible from outside without it.
    pub fn point_on(&self, page: usize, at: (f32, f32)) -> (f32, f32) {
        let node = self
            .harness
            .query_all(&format!("[data-page='{page}']"))
            .first()
            .copied()
            .unwrap_or_else(|| panic!("page {page} is not mounted"));
        let rect = self.harness.layout_rect_of(node);
        (rect.x + rect.width * at.0, rect.y + rect.height * at.1)
    }

    /// What the shell does after a click *and after a key*, done here instead.
    ///
    /// A click clears the focus off the reader and onto `<html>`, and from
    /// then on every shortcut goes somewhere no component can hear — see
    /// [`crate::app::KEYBOARD`], which is the whole account of it. The real
    /// window answers this in `shell.rs`; a harness has no window, so it
    /// answers it here, in the same one line and through the same function.
    /// The same trick and the same reason as `focus_root` above.
    fn give_keyboard_back(&mut self) {
        crate::app::give_keyboard_back(&mut self.harness.doc.inner_mut());
        self.harness.pump();
    }

    /// The nth element matching a selector, clicked. The toolbar is four chips
    /// in a row and they are told apart by their order, exactly as somebody
    /// looking at them would.
    pub fn click_nth(&mut self, selector: &str, nth: usize) {
        let nodes = self.harness.query_all(selector);
        let node = nodes
            .get(nth)
            .unwrap_or_else(|| panic!("no {selector} number {nth}"));
        let rect = self.harness.layout_rect_of(*node);
        let (x, y) = rect.center();
        self.harness.click_at(x, y);
        self.give_keyboard_back();
        self.settle();
    }

    /// What an element says, or nothing when there is no such element.
    ///
    /// `text_content` panics on a selector that matches nothing, which is the
    /// right default for a test asserting on something it expects to be
    /// there — and the wrong one for [`Reader::state`], which reads eight
    /// things off a toolbar that can now be put away. A reader with no
    /// toolbar has no zoom on screen; that is a fact about the state, not a
    /// failure to read it.
    fn text(&self, selector: &str) -> String {
        match self.harness.query(selector) {
            Some(_) => self.harness.text_content(selector),
            None => String::new(),
        }
    }

    /// What the interface says about itself.
    pub fn state(&self) -> State {
        // The page off the field and the count off the span beside it. It was
        // one string in a pill until the field replaced the readout, and
        // reading a number out of an `<input>` is reading its editor rather
        // than its text: an input has no text content at all.
        let label = match self.harness.query(".page-field") {
            // Being typed into, and then what it says is what has been typed.
            Some(_) => self.field(".page-field"),
            None => self.text(".page-now"),
        };
        let page = label.trim().parse().unwrap_or(0);
        let pages = self
            .text(".of")
            .trim_start_matches('/')
            .trim()
            .parse()
            .unwrap_or(0);
        let mounted = self.numbered(".page", "data-page");
        let thumbs = self.numbered(".thumb", "data-thumb");
        let sidebar_node = self.harness.query(".sidebar");
        let sidebar = sidebar_node.map(|_| {
            for name in ["pages", "results"] {
                if self
                    .harness
                    .query(&format!(".tab.on[data-tab='{name}']"))
                    .is_some()
                {
                    return name.to_string();
                }
            }
            "contents".to_string()
        });
        let sidebar_width = sidebar_node
            .map(|node| self.harness.layout_rect_of(node).width as f64)
            .unwrap_or(0.0);
        State {
            page,
            label,
            pages,
            // By class rather than by position. They were read off the first
            // and the last of the chips, which was fine while there were four
            // and wrong the moment the sidebar added two more — and wrong
            // silently, because a chip is a chip.
            zoom: self.text(".chip.fit"),
            theme: self.text(".chip.theme"),
            notice: self.text(".notice"),
            // Guarded, because `attr` panics on a selector that matches
            // nothing and a window showing the start screen has no `.pages`
            // in it at all. Every other reader here goes through `text`,
            // which has been guarded all along for the same reason one level
            // down: the toolbar is not always on screen either.
            scroll: self
                .harness
                .query(".pages")
                .and_then(|_| self.harness.attr(".pages", "data-scroll"))
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0),
            mounted,
            sidebar,
            sidebar_width,
            thumbs,
            find: self
                .harness
                .query(".findbar")
                .map(|_| self.harness.text_content(".find-count")),
            query: self.field(".find-field"),
            hits: self.harness.query_all(".hit").len(),
            results: self.numbered(".result", "data-result"),
            title: self.text(".title"),
            menu: ["document", "open", "theme", "view"]
                .into_iter()
                .find(|which| self.harness.query(&format!(".menu.{which}")).is_some())
                .map(str::to_string),
            toolbar: self.harness.query(".toolbar").is_some(),
            presenting: self.harness.query(".root.presenting").is_some(),
            empty: self.harness.query(".start").is_some(),
            // Read as one string a row, the way somebody looking at the list
            // would read it: "book.pdf p. 12". Two fields would be two
            // parallel arrays and one assertion apiece; a row is the thing on
            // the screen.
            recents: self
                .text_all(".recent-open")
                .into_iter()
                .map(|row| row.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect(),
            dragging: self
                .harness
                .query(".drop-hint")
                .map(|_| self.text(".drop-hint-word")),
        }
    }

    /// What is in a text field, which is not in the DOM as text: an input
    /// holds an editor, and the editor holds the string. Empty when there is
    /// no such field, which is how "the find bar is not up" reads.
    fn field(&self, selector: &str) -> String {
        self.harness
            .query(selector)
            .and_then(|node| {
                let doc = self.harness.base();
                doc.get_node(node)?
                    .element_data()?
                    .text_input_data()
                    .map(|input| input.editor.raw_text().to_string())
            })
            .unwrap_or_default()
    }

    /// What every node matching `selector` says in one attribute, in document
    /// order — and an empty string for one that does not carry it.
    ///
    /// Public because a test outside this crate cannot walk the DOM itself:
    /// `blitz-dom` is not one of its dependencies and should not become one
    /// for the sake of reading an attribute. `tests/links.rs` picks a link out
    /// of a page by the name it tells a screen reader, which is the only thing
    /// on it that says where it goes.
    pub fn attribute_all(&self, selector: &str, attribute: &str) -> Vec<String> {
        self.harness
            .query_all(selector)
            .into_iter()
            .map(|node| {
                let doc = self.harness.base();
                doc.get_node(node)
                    .and_then(|node| node.attrs())
                    // By name rather than through `local_name!`, which only
                    // knows the atoms the HTML spec has: some of these are
                    // ours.
                    .and_then(|attrs| attrs.iter().find(|attr| &*attr.name.local == attribute))
                    .map(|attr| attr.value.to_string())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// What every node matching `selector` says, in document order.
    ///
    /// [`Reader::attribute_all`]'s twin, and public for the same reason: a
    /// test outside this crate cannot walk the DOM. The Settings window is
    /// what wants it — a switch has no text of its own, so which switch is
    /// which is read off the labels beside them, in order.
    pub fn text_all(&self, selector: &str) -> Vec<String> {
        self.harness
            .query_all(selector)
            .into_iter()
            .map(|node| {
                let doc = self.harness.base();
                doc.get_node(node)
                    .map(|node| node.text_content())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Every node matching `selector`, as the number its `attribute` carries,
    /// in document order. The two mounting windows are read this way — the
    /// document's pages and the sidebar's thumbnails — because neither has
    /// pixels that say which page it is.
    fn numbered(&self, selector: &str, attribute: &str) -> Vec<usize> {
        self.attribute_all(selector, attribute)
            .into_iter()
            .filter_map(|value| value.parse().ok())
            .collect()
    }

    /// The window, rasterised. RGBA8, `width * height * 4` bytes, top row
    /// first — the same buffer a PNG is written from.
    pub fn screenshot(&mut self) -> Shot {
        let width = (self.width as f64 * self.scale) as u32;
        let height = (self.height as f64 * self.scale) as u32;
        let scale = self.scale;
        let mut doc = self.harness.doc.inner_mut();
        let rgba = anyrender::render_to_buffer::<VelloCpuImageRenderer, _>(
            |scene| {
                // The page under the document, which the app paints in CSS and
                // a bare scene has no opinion about. White rather than the
                // theme's paper, so that a page failing to draw at all shows
                // as white on a dark theme rather than blending in.
                scene.fill(
                    Fill::NonZero,
                    Default::default(),
                    Color::WHITE,
                    Default::default(),
                    &Rect::new(0.0, 0.0, width as f64, height as f64),
                );
                blitz_paint::paint_scene(scene, &mut doc, scale, width, height, 0, 0);
            },
            width,
            height,
        );
        Shot {
            width,
            height,
            rgba,
        }
    }

    /// A screenshot, written where somebody can look at it.
    pub fn save_png(&mut self, path: &str) {
        self.screenshot().save(path);
    }
}

/// One rasterised window.
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Shot {
    /// The pixel at (x, y), as RGBA.
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is outside a {}x{} window",
            self.width,
            self.height
        );
        let at = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[at],
            self.rgba[at + 1],
            self.rgba[at + 2],
            self.rgba[at + 3],
        ]
    }

    /// How many of the pixels in a rectangle are not the given colour, as a
    /// fraction. This is what "there is something on the page" means when the
    /// question is asked of a rasteriser rather than of a person.
    pub fn unlike(&self, colour: [u8; 3], rect: (u32, u32, u32, u32)) -> f64 {
        let (x0, y0, x1, y1) = rect;
        let mut seen = 0u64;
        let mut different = 0u64;
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                let pixel = self.at(x, y);
                seen += 1;
                let far = (0..3)
                    .map(|c| (pixel[c] as i32 - colour[c] as i32).abs())
                    .max()
                    .unwrap_or(0);
                if far > 12 {
                    different += 1;
                }
            }
        }
        if seen == 0 {
            0.0
        } else {
            different as f64 / seen as f64
        }
    }

    /// The mean colour of a rectangle, which is how "this page got darker" is
    /// asked without caring which pixel did it.
    /// The first column of a band that has anything drawn on it, as an x in
    /// device pixels — and `None` for a band that is all one colour.
    ///
    /// What "anything" means is: unlike the band's own top-left pixel, which
    /// is the ground it is drawn on. That is how a reader tells an indent
    /// from a flush row, and it is the only way to measure one here: the rows
    /// are the width of the panel whatever their depth, so the indent is
    /// padding and the box does not move.
    pub fn leftmost_ink(&self, rect: (u32, u32, u32, u32)) -> Option<u32> {
        let (x0, y0, x1, y1) = rect;
        let ground = self.at(x0, y0);
        for x in x0..x1.min(self.width) {
            for y in y0..y1.min(self.height) {
                let pixel = self.at(x, y);
                let far = (0..3)
                    .map(|c| (pixel[c] as i32 - ground[c] as i32).abs())
                    .max()
                    .unwrap_or(0);
                if far > 12 {
                    return Some(x);
                }
            }
        }
        None
    }

    pub fn mean(&self, rect: (u32, u32, u32, u32)) -> [f64; 3] {
        let (x0, y0, x1, y1) = rect;
        let mut total = [0f64; 3];
        let mut seen = 0f64;
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                let pixel = self.at(x, y);
                for c in 0..3 {
                    total[c] += pixel[c] as f64;
                }
                seen += 1.0;
            }
        }
        if seen == 0.0 {
            return [0.0; 3];
        }
        [total[0] / seen, total[1] / seen, total[2] / seen]
    }

    pub fn save(&self, path: &str) {
        let file = std::fs::File::create(path).unwrap_or_else(|err| panic!("{path}: {err}"));
        let mut encoder = png::Encoder::new(file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&self.rgba)
            .unwrap();
    }
}

/// A chord as `keys.toml` writes it, taken apart into the event a keyboard
/// would have sent. The inverse of `keymap::parse_chord`, and deliberately in
/// the harness rather than beside it: nothing the reader ships needs to turn a
/// chord back into a keystroke.
fn spell_out(chord: &str) -> (Key, Code, Modifiers) {
    let mut rest = chord;
    let mut modifiers = Modifiers::default();
    while let Some(at) = rest.find('+') {
        // `+` is a key as well as a separator: `mod++` is a chord whose key
        // is the plus sign, and there is nothing before the last one to read
        // as a modifier.
        let (name, after) = rest.split_at(at);
        let flag = match name {
            "mod" => {
                if crate::keymap::this_machine() {
                    Modifiers::META
                } else {
                    Modifiers::CONTROL
                }
            }
            "ctrl" => Modifiers::CONTROL,
            "alt" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            _ => break,
        };
        modifiers |= flag;
        rest = &after[1..];
    }
    let key = match rest {
        "space" => Key::Character(" ".to_string()),
        "escape" => Key::Escape,
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "home" => Key::Home,
        "end" => Key::End,
        one if one.chars().count() == 1 => {
            // A shifted letter reaches the page as the capital, which is what
            // `chordsOf` reads: `shift+g` is G and not a shifted g.
            if modifiers.shift() && one.chars().all(|c| c.is_ascii_lowercase()) {
                Key::Character(one.to_uppercase())
            } else {
                Key::Character(one.to_string())
            }
        }
        named => named
            .parse()
            .unwrap_or_else(|_| panic!("no key called {named:?}")),
    };
    (key, Code::Unidentified, modifiers)
}

/// The reader's `keys.toml`, written before the reader reads it. Nothing is
/// written when there is nothing to say, so the ordinary test gets the
/// defaults through the same path a fresh install does.
fn write_keys(dir: &std::path::Path, keys: &BTreeMap<String, Vec<String>>) {
    if keys.is_empty() {
        return;
    }
    let mut body = String::new();
    for (action, chords) in keys {
        let quoted: Vec<String> = chords.iter().map(|c| format!("{c:?}")).collect();
        body.push_str(&format!("{action} = [{}]\n", quoted.join(", ")));
    }
    let _ = std::fs::create_dir_all(dir);
    std::fs::write(dir.join(crate::keys::FILE), body).expect("keys.toml");
}

/// The settings a test asked for, written the way the app writes them.
fn write_settings(dir: &std::path::Path, settings: &[(String, serde_json::Value)]) {
    if settings.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    crate::settings::set_many(dir, settings.to_vec()).expect("settings.toml");
}

/// "ArrowDown" is a named key and "j" is a character. `keyboard_types` parses
/// the named ones and refuses everything else, which is exactly the split.
fn parse_key(key: &str) -> Key {
    if key.chars().count() == 1 {
        return Key::Character(key.to_string());
    }
    key.parse()
        .unwrap_or_else(|_| panic!("no key called {key:?}"))
}
