//! The reader, as components and one piece of state.
//!
//! `main.ts` holds an `App` object with the whole interface hanging off it and
//! about thirty methods that change it; the shape here is the same and the
//! wiring is not. A signal holding one `Viewer` is what the `App` object was,
//! and the DOM that used to be built by hand with `document.createElement` is
//! an `rsx!` block that is rebuilt from the state whenever the state moves.
//!
//! Three things about it are Blitz's doing rather than choices:
//!
//! *Nothing is `position: fixed`*, because Blitz has no such thing. The root
//! is a flex column — toolbar, viewer, notice — so nothing is over a scrolling
//! body and nothing needs to be. The assessment expected this to be a
//! workaround and it is an improvement.
//!
//! *The keyboard is one handler on the root, and that is enough.* Blitz sends
//! a key to the focused node and falls back to the root element when there is
//! none, and DOM events bubble — so a `keydown` on the root is the app-level
//! handler `main.ts` has, including for a page, which cannot be focused. What
//! it does with the key is not this file's: an event becomes a chord and a
//! chord is looked up in [`crate::keymap`], which is `keys.ts` ported, over
//! `keys.toml`, which is the app's own `keys.rs` mounted. What is left here is
//! `perform` at the bottom — one arm per action, and the arms that are missing
//! are the list of what Phase 3 has left to build. The
//! winit route would be `use_window_event`, and it is closed to us: it takes
//! its `WindowEventHandlers` out of a context that only `dioxus_native`'s own
//! application provides, and the type is private, so a shell of our own cannot
//! provide one. That is a third thing on the list `PROGRESS.md` keeps of what
//! owning the window costs.
//!
//! *A page is a `<div>` with an `<object>` in it*, absolutely positioned
//! against the viewer itself at where-it-is minus where-the-reader-is. Blitz
//! has no `position: static`, so an absolutely positioned node is placed
//! against its immediate parent — which is what this layout wants anyway.
//!
//! *And the scrolling is ours, not the engine's.* The obvious shape is
//! `overflow: scroll` on the viewer and Blitz's own scroller underneath it,
//! and it does not survive contact with the second half of scrolling, which is
//! the app moving the document itself — a page jump, Home, a zoom that has to
//! keep the reader's place. Every one of those goes through `MountedData`, and
//! **every `MountedData` call borrows the document while the document is
//! already borrowed**: a DOM event handler runs inside `EventDriver`'s borrow,
//! and a mounted handler inside `flush_queued_mounted_events`'s, so `scroll`
//! and `get_client_rect` both panic with "RefCell already borrowed" rather
//! than failing. (`NodeHandle::try_doc` exists and says as much in its own doc
//! comment; the safe methods are not the ones a reader needs.)
//!
//! So the scroll offset is a number in this file, the wheel moves it, and the
//! pages are placed against it. That is what `viewer.ts` does in all but the
//! last step anyway — it computes every position itself and lets the browser
//! hold one number. What is lost is the scrollbar and the platform's own
//! fling, which is a real loss and is written down in `PROGRESS.md` rather
//! than papered over: a scrollbar we would have to draw, and momentum arrives
//! from the trackpad in the event stream regardless.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use dioxus_native::CustomWidgetAttr;
use serde_json::json;

use crate::keymap::{Action, Keymap, Press};
use crate::layout::{Anchor, Fit, Layout, Mode, Size, Spread};
use crate::page::{Chosen, PageWidget};
use crate::palette::Palette;
use crate::render::{Heading, Link, PageSource, Rect, Target};
use crate::search::{Options as Find, Search};
use crate::sidebar::{Column, Sidebar, Tab};
use crate::store::Store;

/// An open document, wrapped so that it can be a component's prop.
///
/// Two handles to the same open document are the same document, and no two
/// documents are ever equal: Dioxus wants this to decide whether a component's
/// props have changed, and there is nothing in an open file to compare.
#[derive(Clone)]
pub struct Handle(pub Arc<dyn PageSource>);

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Where the reader's settings and themes live, and what to wear for this run.
///
/// A path rather than an open [`Store`], because a component's props have to
/// be `Clone` and `PartialEq` and a settings table is neither — and because
/// the store is this window's, made where the window's state is made.
#[derive(Clone, PartialEq)]
pub struct Config {
    pub dir: std::path::PathBuf,
    /// `--theme N`, which chooses for this run without writing it down.
    pub theme: Option<usize>,
}

impl Config {
    /// The reader's own directory. See [`crate::config::config_dir`]: it is
    /// deliberately not the installed app's.
    pub fn here() -> Config {
        Config {
            dir: crate::config::config_dir(),
            theme: None,
        }
    }

    pub fn at(dir: impl Into<std::path::PathBuf>) -> Config {
        Config {
            dir: dir.into(),
            theme: None,
        }
    }
}

/// How big the window is, asked of whatever knows.
///
/// This used to be `use_window()`, which consumes an `Arc<dyn winit::Window>`
/// from a context — and that is the one thing a headless test cannot provide:
/// `dyn Window` is thirty-odd methods about a thing that does not exist. So
/// the component asks for a number instead, the shell answers it out of the
/// real window, and the harness answers it out of its own viewport. Width and
/// height in logical pixels, and the scale factor beside them.
///
/// It is also the more honest shape. A component reaching into winit is a
/// component that knows what it is running under, and the whole argument for
/// keeping the seams narrow — `api.ts` in the app, `render.rs` here — says it
/// should not.
#[derive(Clone)]
pub struct Screen(Rc<dyn Fn() -> (f64, f64, f64)>);

impl PartialEq for Screen {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Screen {
    pub fn new(size: impl Fn() -> (f64, f64, f64) + 'static) -> Self {
        Screen(Rc::new(size))
    }

    /// A window of a fixed size, which is what a test has.
    pub fn fixed(width: f64, height: f64, scale: f64) -> Self {
        Screen::new(move || (width, height, scale))
    }

    pub fn get(&self) -> (f64, f64, f64) {
        (self.0)()
    }
}

/// The chrome above and below the document, which the viewport is the window
/// minus. The app measures this off the elements; here it is stated, because
/// there is no `ResizeObserver` and no `get_client_rect` that can be called
/// safely from an event — see `resize_from_window` below.
pub const CHROME: f64 = 46.0 + 30.0 + 2.0;

/// How far one press of an arrow moves the page.
const LINE: f64 = 60.0;

/// What a screen keeps of itself when a screen is scrolled: the last lines of
/// the old screen are the first lines of the new one, which is how somebody
/// reading a paragraph across the join does not lose it. `scrollByViewport` in
/// `viewer.ts` is the same number, and half a screen is half of *this* rather
/// than half of the window — otherwise `d` twice and Space once land in two
/// different places, which is exactly the sort of thing a reader notices and
/// cannot name.
const OVERLAP: f64 = 60.0;

/// How many places back a reader can step.
///
/// The app's number, and its reasoning: deep enough to walk out of a chain of
/// cross-references, shallow enough that it is a history rather than a log.
const HISTORY_LIMIT: usize = 50;

/// How much of the window a match is brought to when the reader is taken to
/// one: a third down, so there is something above it to read into.
const REVEAL: f64 = 0.3;

/// The zoom ladder, in the app's own steps.
const ZOOMS: [f64; 13] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0,
];

/// What opening a link outside the document does.
///
/// A context rather than a call, for the reason [`Screen`] is one: the thing
/// it stands for does not exist in a test. `webbrowser::open` is the right
/// answer in the app and is the *default* here, so a shell that provides
/// nothing still opens links — but a harness that provides its own can watch
/// where a link would have gone without a browser window arriving on somebody
/// else's screen halfway through `cargo test`.
///
/// It is the same door `nav.rs` already is for a `<a href>` inside the
/// chrome; a document's own links do not go through the DOM at all, so they
/// need their own way out.
#[derive(Clone)]
pub struct Away(Rc<dyn Fn(&str)>);

impl PartialEq for Away {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Away {
    pub fn new(open: impl Fn(&str) + 'static) -> Self {
        Away(Rc::new(open))
    }

    /// The default: hand the address to the system, with the same three
    /// schemes `nav.rs` allows and for the same reason — a `file:` or a
    /// `javascript:` in somebody's document is not a thing this app opens
    /// because the document asked.
    pub fn to_the_system() -> Self {
        Away::new(|url| {
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")
            {
                if let Err(err) = webbrowser::open(url) {
                    eprintln!("could not open {url}: {err}");
                }
            }
        })
    }

    pub fn open(&self, url: &str) {
        (self.0)(url);
    }
}

/// A fit mode, spelled the way `settings.toml` spells it. One function rather
/// than two match arms in two places, because the value written and the value
/// read back have to be the same word and nothing else checks that they are.
fn name_of(fit: Fit) -> &'static str {
    match fit {
        Fit::Width => "width",
        Fit::Page => "page",
        Fit::Actual => "actual",
    }
}

/// Everything the reader is looking at, and everything that changes it.
pub struct Viewer {
    pub document: Arc<dyn PageSource>,
    pub layout: Layout,
    pub scroll_top: f64,
    /// The settings table and the themes, which is everything this reader
    /// remembers between runs. It is not a signal and does not need to be:
    /// every change to it goes through a method here, and this whole struct
    /// is behind one.
    pub store: Store,
    chosen: Chosen,
    /// One line at the bottom of the window, which is `notice()` in `ui.ts`.
    pub notice: String,
    /// Every key the reader can press, and what it asks for. Built once from
    /// the defaults with `keys.toml` over the top; see [`crate::keymap`].
    pub keymap: Keymap,
    /// The first half of a sequence, waiting to find out what follows it —
    /// `g`, on its way to `g g`. Empty almost always.
    pending: String,
    /// Bumped whenever every page has to be drawn again. It is not in the
    /// texture's key — the widget compares sizes and themes itself — but the
    /// components have to be told that something they cannot see has moved.
    pub generation: u64,
    /// The panel on the left, and what it is showing. All three are settings,
    /// so a reader who reads with the contents open gets them back.
    pub sidebar_open: bool,
    pub sidebar_width: f64,
    /// The pointer's `client_x` and the panel's width when the edge was
    /// picked up — everything `drag_sidebar` needs and nothing it has to ask
    /// the DOM for. `None` outside a drag, which is most of the time and is
    /// what root's `onmousemove` checks before touching the signal at all.
    resize_from: Option<(f64, f64)>,
    pub tab: Tab,
    /// Where the thumbnail column has been scrolled to. Ours for the reason
    /// the document's own scroll offset is ours — see the module comment.
    pub thumb_scroll: f64,
    /// Where every thumbnail sits, for the panel at its current width.
    pub column: Column,
    /// The document's own table of contents, read once when it was opened.
    pub headings: Vec<Heading>,
    /// What the document calls its own pages, or empty when it calls them 1
    /// to n — see [`crate::render::PageSource::labels`]. Read once at open,
    /// like the outline, and for the same reason: it decides what the toolbar
    /// *says*, and a number that arrives late is a number that was wrong.
    labels: Vec<String>,
    /// The links on each page that has been asked about, kept.
    ///
    /// **The one piece of state here behind a `RefCell`**, because it is the
    /// one that has to be filled while the reader is being *read*: a page's
    /// links are wanted by the render that mounts it, and a render holds a
    /// `read()` of this whole struct. Filling it from the outside instead
    /// would mean every method that changes which pages are mounted
    /// remembering to — `scroll_to`, `resize`, `set_fit`, `zoom`,
    /// `set_spread`, `go_to_page` — and the failure of forgetting one is a
    /// page whose cross-references quietly do nothing.
    ///
    /// It is not trimmed. A link is a rectangle and a target; a book of four
    /// hundred pages of mathematics is a few hundred kilobytes of them, which
    /// is a fiftieth of one page's texture, and the whole point of the caches
    /// this experiment does keep an eye on is that they hold *pixels*.
    links: RefCell<HashMap<usize, Rc<Vec<Link>>>>,
    /// Where the reader has jumped from, and where they came back from.
    ///
    /// The distinction the app draws and the reason both lists exist: moving
    /// *through* a document leaves no trace — scrolling, turning a page,
    /// stepping through search results — and jumping *across* it does.
    /// Following a cross-reference, picking a chapter out of the contents and
    /// typing a page number are the moves that leave a reader stranded, and
    /// they are exactly the three that go through [`Viewer::jump_to`].
    past: Vec<Anchor>,
    future: Vec<Anchor>,
    /// Whether the reader is typing a page number into the field in the
    /// toolbar, and what they have typed.
    ///
    /// The field shows the current page's label when nobody is typing in it,
    /// and what has been typed when somebody is. Two states rather than one
    /// because the app selects the field's contents when it is focused and
    /// there is no way to say that here — so arriving *empties* it instead,
    /// which comes to the same thing for anybody who then types.
    pub typing_page: bool,
    pub page_typed: String,
    /// The index, the matches, and where the reader is in them. See
    /// [`crate::search`]: it knows nothing about this struct, which is what
    /// lets the whole of it be tested with no document and no window.
    pub search: Search,
    /// Whether the find bar is up. The index is put down when it goes — see
    /// [`Search::forget`] — so this is a memory decision as well as a
    /// visible one.
    pub find_open: bool,
    /// What is in the field, which is not the same as what has been searched
    /// for: the field is ahead of the scan by however long a keystroke takes
    /// to reach it.
    pub find_query: String,
    /// Whether every match is painted or only the one the reader is on. The
    /// third search switch, and the one that changes nothing about what is
    /// found — which is why it lives here and not in [`crate::search`].
    pub highlight_all: bool,
    /// Which scan is running. A keystroke starts a new one and the task
    /// driving the old one stops at its next slice, which is what `run` in
    /// `search.ts` does with the same two lines.
    scan: u64,
    /// Whether this search has already taken the reader to its first match.
    /// Once, and only for the first result to arrive — after that the reader
    /// is moved by asking, not by the scan catching up.
    revealed: bool,
    /// Whether the reader has asked for the margins to come off. Not the
    /// same question as whether any came off — see [`Viewer::trimmed`].
    trimming: bool,
    /// How wide the window is, which is not how wide the document is: the
    /// panel takes its share first. Kept because opening the panel has to
    /// relay out against the same window.
    window_width: f64,
}

impl Viewer {
    pub fn new(document: Arc<dyn PageSource>, chosen: Chosen, store: Store) -> Self {
        let sizes = (0..document.pages())
            .map(|index| document.size_of(index))
            .collect();
        // The reader's file over the defaults, and its complaints in front of
        // the keymap's own: `keys.rs` reports the shapes TOML can describe and
        // this side cannot use, `keymap` reports the chords and the action
        // names — and to a reader looking at one line at the bottom of a
        // window they are all just things wrong with `keys.toml`.
        let file = store.keyboard();
        let mut keymap = Keymap::build(crate::keymap::this_machine(), &file.bindings);
        keymap.problems = file
            .problems
            .into_iter()
            .chain(keymap.problems.drain(..))
            .collect();
        let mut viewer = Viewer {
            layout: Layout::new(sizes),
            scroll_top: 0.0,
            sidebar_open: false,
            sidebar_width: 252.0,
            resize_from: None,
            tab: Tab::Contents,
            thumb_scroll: 0.0,
            column: Column::default(),
            headings: document.outline(),
            labels: document.labels(),
            links: RefCell::new(HashMap::new()),
            past: Vec::new(),
            future: Vec::new(),
            typing_page: false,
            page_typed: String::new(),
            search: Search::new(),
            find_open: false,
            find_query: String::new(),
            highlight_all: true,
            scan: 0,
            revealed: false,
            trimming: false,
            window_width: 0.0,
            notice: String::new(),
            keymap,
            pending: String::new(),
            generation: 0,
            chosen,
            document,
            store,
        };
        // The document goes into `library.toml` before anything is restored,
        // because that is what gives a mark somewhere to live — see
        // `Store::opened`.
        let path = viewer.document.path().to_string();
        if !path.is_empty() {
            viewer.store.opened(&path);
        }
        viewer.restore();
        viewer
    }

    /// Put back what the last run left, which is the brief's first promise
    /// about settings: they survive, and they are independent of each other.
    ///
    /// Read once, at the start, and then never again — the settings table is
    /// this window's copy from here on, which is the same thing the app says
    /// about two windows and a setting changed in one of them.
    fn restore(&mut self) {
        self.layout.fit = match self.store.text("fit_mode").as_str() {
            "page" => Fit::Page,
            "actual" => Fit::Actual,
            _ => Fit::Width,
        };
        // A zoom out of the range the ladder covers is a zoom nothing can step
        // away from, and `settings.rs` only promises the value is a number.
        self.layout.zoom = self.store.number("zoom").clamp(ZOOMS[0], ZOOMS[ZOOMS.len() - 1]);
        self.layout.spread = match self.store.text("spread_mode").as_str() {
            "two" => Spread::Two,
            "cover" => Spread::Cover,
            _ => Spread::Single,
        };
        self.layout.gap = self.store.number("page_gap");
        // Continuous unless the file says otherwise, and the file is the only
        // way to say otherwise: see [`Mode`].
        self.layout.mode = match self.store.text("scroll_mode").as_str() {
            "paged" => Mode::Paged,
            _ => Mode::Continuous,
        };
        self.sidebar_open = self.store.flag("show_sidebar");
        // The switch is a setting and the crop is not, so a run that had it on
        // measures this document rather than putting back the last one's
        // rectangle. It is deferred to the end of `restore` because measuring
        // draws eight pages and the layout has not been built yet.
        self.trimming = self.store.flag("trim_margins");
        // Where a match is looked for is a way of reading rather than a
        // property of a document, so these outlive the find bar they are set
        // from and the session they were set in — which is the comment
        // `settings.rs` already carries above the three of them.
        self.highlight_all = self.store.flag("search_highlight_all");
        self.search.set_options(Find {
            match_case: self.store.flag("search_match_case"),
            whole_words: self.store.flag("search_whole_words"),
        });
        self.sidebar_width = self
            .store
            .number("sidebar_width")
            .clamp(crate::sidebar::MIN_WIDTH, crate::sidebar::MAX_WIDTH);
        // Which tab is showing is not a setting — the app has none either,
        // and inventing one here would mean adding a key to the app's own
        // `settings.rs`, which is the file this crate mounts rather than
        // edits. A document with no contents opens on the pages, which is
        // what `setDocument` does and is the difference between a panel and
        // an empty box.
        if self.headings.is_empty() {
            self.tab = Tab::Pages;
        }
        self.relay_column();
        self.layout.relayout();
        if self.trimming {
            self.measure_crop();
        }
        self.chosen.set(self.store.palette());
        // A theme naming a colour that cannot be read is the one thing here
        // worth a sentence on the screen, and `store` has already worked out
        // whether there is one.
        // Two things can be wrong with the reader's files at startup and
        // there is one line to say so in. The theme wins, because it is about
        // what is on the screen right now; the keyboard's is still there on
        // the next keystroke that does nothing.
        if let Some(said) = self
            .store
            .complaint
            .clone()
            .or_else(|| self.keymap.complaint())
        {
            self.notice = said;
        }
    }

    pub fn page(&self) -> usize {
        self.layout.page_at(self.scroll_top)
    }

    pub fn pages(&self) -> usize {
        self.layout.pages()
    }

    /// The theme in use, as colours. The name beside it is the theme's, and
    /// the toolbar shows that rather than this.
    pub fn palette(&self) -> Palette {
        self.store.palette()
    }

    pub fn theme_name(&self) -> String {
        self.store.theme().name.clone()
    }

    /// The window changed size, or the sidebar did. Everything below the
    /// layout is a function of this, so it is the one entry point that both
    /// relays out and puts the reader back where they were.
    ///
    /// The width handed in is the *window's*; what the document gets is that
    /// minus the panel, which is why opening the sidebar is a resize and not
    /// merely something appearing beside the page.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.window_width = width;
        let width = self.document_width();
        if (self.layout.viewport.width - width).abs() < 0.5
            && (self.layout.viewport.height - height).abs() < 0.5
        {
            return;
        }
        let anchor = self.layout.anchor(self.scroll_top);
        self.layout.viewport = Size { width, height };
        self.layout.relayout();
        self.scroll_top = self.layout.scroll_target(anchor);
        // The column is laid out for a panel of a width and clamped against a
        // panel of a height, and this is where both of those move.
        self.relay_column();
        self.reveal_thumb();
    }

    /// How much of the window the document has: everything the panel is not
    /// standing on.
    fn document_width(&self) -> f64 {
        let panel = if self.sidebar_open {
            self.sidebar_width
        } else {
            0.0
        };
        (self.window_width - panel).max(120.0)
    }

    /* ------------------------------------------------------- the sidebar */

    /// Open or shut the panel, and remember which.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
        self.store
            .set(vec![("show_sidebar".into(), json!(self.sidebar_open))]);
        // The document is now a different width, and the reader should be
        // looking at the same words afterwards — which is exactly what
        // `resize` promises, so this goes through it rather than around it.
        let (width, height) = (self.window_width, self.layout.viewport.height);
        self.layout.viewport.width = -1.0;
        self.resize(width, height);
        self.generation += 1;
        if self.sidebar_open {
            self.reveal_thumb();
        }
    }

    /// The panel's edge has been picked up, at this `client_x`.
    pub fn start_resize_sidebar(&mut self, client_x: f64) {
        self.resize_from = Some((client_x, self.sidebar_width));
    }

    /// The pointer has moved to `client_x`. A no-op outside a drag, which is
    /// what lets `onmousemove` sit on the root and fire on every move in the
    /// window without a drag costing a render it did not ask for.
    ///
    /// **Only the panel's own width moves here — the document does not.**
    /// `sidebar_width` is in `PageWidget`'s key alongside the page and the
    /// theme (see `page.rs`), so relaying the document out at every width a
    /// drag passes through is a fresh pdfium render and texture upload for
    /// every mounted page, every frame, with nothing to show while either
    /// runs: exactly the white flicker a reader sees. `.sidebar`'s own width
    /// is a plain style attribute, so the boundary line still follows the
    /// pointer — flexbox gives the `.viewer` box the room it has left for
    /// free — and `finish_resize_sidebar` is the one relayout the drag
    /// deferred, once, at wherever it landed.
    pub fn drag_sidebar(&mut self, client_x: f64) {
        let Some((start_x, start_width)) = self.resize_from else {
            return;
        };
        let width = (start_width + (client_x - start_x))
            .clamp(crate::sidebar::MIN_WIDTH, crate::sidebar::MAX_WIDTH);
        self.sidebar_width = width;
    }

    /// The pointer let go: the one relayout the drag deferred, and the one
    /// write — same reasoning as `toggle_sidebar`, and there is no debounced
    /// write to reach for the way `main.ts`'s `setSoon` is for a zoom moving
    /// under a pinch.
    pub fn finish_resize_sidebar(&mut self) {
        if self.resize_from.take().is_some() {
            let (window_width, height) = (self.window_width, self.layout.viewport.height);
            self.layout.viewport.width = -1.0;
            self.resize(window_width, height);
            // A whole number, because `same_shape` in `settings.rs` holds
            // `sidebar_width` to the shape its default is — a distance in
            // pixels — and a drag's arithmetic is not.
            self.store.set(vec![(
                "sidebar_width".into(),
                json!(self.sidebar_width.round() as i64),
            )]);
        }
    }

    pub fn show_tab(&mut self, tab: Tab) {
        self.tab = tab;
        if tab == Tab::Pages {
            self.reveal_thumb();
        }
    }

    /// Lay the thumbnail column out for the panel as it stands.
    fn relay_column(&mut self) {
        let sizes: Vec<Size> = (0..self.layout.pages())
            .map(|index| self.layout.size_of(index))
            .collect();
        self.column = Column::new(&sizes, self.sidebar_width);
        self.thumb_scroll = self
            .thumb_scroll
            .clamp(0.0, self.column.max_scroll(self.layout.viewport.height));
    }

    pub fn scroll_thumbs(&mut self, delta: f64) {
        let height = self.layout.viewport.height;
        self.thumb_scroll = (self.thumb_scroll + delta).clamp(0.0, self.column.max_scroll(height));
    }

    /// Bring the page the reader is on into the column, if it is not there
    /// already. See `Column::reveal`: doing nothing is most of the behaviour.
    fn reveal_thumb(&mut self) {
        if !self.sidebar_open || self.tab != Tab::Pages {
            return;
        }
        let height = self.layout.viewport.height;
        if let Some(to) = self.column.reveal(self.page() - 1, self.thumb_scroll, height) {
            self.thumb_scroll = to;
        }
    }

    /// Go to a page, one-based — what a row in either list does when it is
    /// clicked, and a *jump*: see [`Viewer::jump_to`].
    pub fn go_to_page(&mut self, page: usize) {
        self.jump_to(page, 0.0);
    }

    /// Land at a place in the document, turning the page first if that is
    /// what landing means here.
    ///
    /// **The one place the two scroll modes actually differ to a caller.** In
    /// continuous mode a page is somewhere to scroll to; in paged mode it is
    /// the only page laid out, so arriving at it is a relayout and the scroll
    /// offset starts again from the top of it. Everything that moves the
    /// reader goes through this — the history, a link, a heading, a match, a
    /// typed page number — so none of them has to know which mode is on.
    pub fn go_to(&mut self, anchor: Anchor) {
        if self.layout.mode == Mode::Paged {
            let page = anchor.page.clamp(1, self.pages().max(1));
            if page != self.layout.current {
                self.layout.current = page;
                self.layout.relayout();
                self.generation += 1;
                self.scroll_top = 0.0;
                self.reveal_thumb();
            }
        }
        let target = self.layout.scroll_target(anchor);
        self.scroll_to(target);
    }

    /// The start and the end of the document, which are not the same thing as
    /// the top and the bottom of what is laid out.
    pub fn to_start(&mut self) {
        match self.layout.mode {
            Mode::Paged => self.go_to(Anchor { page: 1, offset: 0.0 }),
            Mode::Continuous => {
                self.scroll_to(0.0);
            }
        }
    }

    pub fn to_end(&mut self) {
        match self.layout.mode {
            Mode::Paged => {
                let last = self.pages().max(1);
                self.go_to(Anchor {
                    page: last,
                    offset: 0.0,
                });
                let bottom = self.layout.max_scroll();
                self.scroll_to(bottom);
            }
            Mode::Continuous => {
                let bottom = self.layout.max_scroll();
                self.scroll_to(bottom);
            }
        }
    }

    /// Move by a distance, and turn the page when there is nowhere left to
    /// move.
    ///
    /// `scrollByViewport` in `viewer.ts` does this for a screen and `onWheel`
    /// for a gesture; here it is one function because a key and a wheel ask
    /// the same question. In paged mode the strip holds exactly one page, so
    /// a page that fits the window cannot be scrolled at all and a taller one
    /// stops dead at its own bottom edge — either way the reader pushes and
    /// nothing happens, which is the one gesture everybody tries first.
    pub fn nudge(&mut self, delta: f64) {
        if self.layout.mode == Mode::Continuous || delta == 0.0 {
            let to = self.scroll_by(delta);
            self.scroll_to(to);
            return;
        }
        let room = self.layout.max_scroll();
        let at_edge = if delta > 0.0 {
            self.scroll_top >= room - 1.0
        } else {
            self.scroll_top <= 1.0
        };
        if !at_edge {
            let to = self.scroll_by(delta);
            self.scroll_to(to);
            return;
        }
        let page = self.layout.current;
        let next = if delta > 0.0 {
            self.layout.row_of(page - 1).last().copied().unwrap_or(page - 1) + 2
        } else {
            page.saturating_sub(1)
        };
        if next < 1 || next > self.pages() {
            return;
        }
        self.go_to(Anchor {
            page: next,
            offset: 0.0,
        });
        // Backwards is the *bottom* of the page arrived at, because that is
        // where the reader was reading: turning back to the top of the
        // previous page skips everything they were about to re-read.
        if delta < 0.0 {
            let bottom = self.layout.max_scroll();
            self.scroll_to(bottom);
        }
    }

    /// One page at a time, or continuous.
    ///
    /// A setting and nothing else — there is no shortcut and no chip, which
    /// is the brief's own instruction: continuous is a strong default and a
    /// key that leaves it by accident is the failure it names. See [`Mode`].
    pub fn set_scroll_mode(&mut self, mode: Mode) {
        if mode == self.layout.mode {
            return;
        }
        let here = self.layout.anchor(self.scroll_top);
        self.layout.mode = mode;
        self.layout.current = here.page;
        self.layout.relayout();
        self.scroll_top = self.layout.scroll_target(here);
        self.generation += 1;
        self.store.set(vec![(
            "scroll_mode".into(),
            json!(match mode {
                Mode::Paged => "paged",
                Mode::Continuous => "continuous",
            }),
        )]);
    }

    /* ------------------------------------------------------ where you were */

    /// Go somewhere the reader asked to go, remembering where they were.
    ///
    /// `jumpTo` in `viewer.ts`, and the distinction it draws is the whole of
    /// why there is a history at all: the citation on page 12 that lands on
    /// page 190 is what this exists for, and the twenty keystrokes of
    /// scrolling that got the reader to page 12 are not.
    ///
    /// A jump that lands where the reader already is is not a jump. Without
    /// that test, pressing Home twice files the first page away as somewhere
    /// worth returning to, and a page number typed twice fills the history
    /// with copies of one place.
    pub fn jump_to(&mut self, page: usize, offset: f64) {
        let from = self.layout.anchor(self.scroll_top);
        let to = page.clamp(1, self.pages().max(1));
        if to == from.page && (offset - from.offset).abs() < 0.01 {
            self.go_to(Anchor { page: to, offset });
            return;
        }
        self.past.push(from);
        if self.past.len() > HISTORY_LIMIT {
            self.past.remove(0);
        }
        // A jump made after stepping back throws away what was ahead, which
        // is what every back button does and what nobody is surprised by.
        self.future.clear();
        self.go_to(Anchor { page: to, offset });
    }

    /// Back to where the last jump started, or forward again.
    ///
    /// `false` when there is nowhere to go, so that the caller can say so:
    /// silence at the end of the history is indistinguishable from a key that
    /// does not work.
    pub fn go_back(&mut self) -> bool {
        let Some(place) = self.past.pop() else {
            return false;
        };
        self.future.push(self.layout.anchor(self.scroll_top));
        self.go_to(place);
        true
    }

    pub fn go_forward(&mut self) -> bool {
        let Some(place) = self.future.pop() else {
            return false;
        };
        self.past.push(self.layout.anchor(self.scroll_top));
        self.go_to(place);
        true
    }

    /* ------------------------------------------------------------- links */

    /// The links on a page, asked for once and kept. See [`Viewer::links`].
    pub fn links_on(&self, index: usize) -> Rc<Vec<Link>> {
        if let Some(known) = self.links.borrow().get(&index) {
            return known.clone();
        }
        let links = Rc::new(self.document.links_of(index));
        self.links.borrow_mut().insert(index, links.clone());
        links
    }

    /// The links on a mounted page, as rectangles in CSS pixels from the top
    /// left of its box — which is the space the page's own overlay is laid
    /// out in, and the same one [`Viewer::highlights`] answers in.
    pub fn link_areas(&self, page: usize) -> Vec<(Rect, Target)> {
        let Some(index) = page.checked_sub(1) else {
            return Vec::new();
        };
        if self.layout.box_of(index).is_none() {
            return Vec::new();
        }
        self.links_on(index)
            .iter()
            .map(|link| (self.layout.place_on(index, link.rect), link.target.clone()))
            .collect()
    }

    /// Follow a link, and say what the window has to do about it.
    ///
    /// A place in this document is a jump and is done here. An address is not
    /// this struct's to open — there is no browser in a `Viewer` and there is
    /// none in the harness either — so it is handed back, and whoever owns the
    /// window decides what opening a link means. That is `onExternalLink` in
    /// `main.ts`, which the app's own comment calls "the only thing allowed to
    /// decide", one layer further out.
    pub fn follow(&mut self, target: &Target) -> Option<String> {
        match target {
            Target::Place { page, offset } => {
                self.jump_to(*page, *offset);
                None
            }
            Target::Away(url) => {
                self.notice = format!("Opened {url}");
                Some(url.clone())
            }
        }
    }

    /* ------------------------------------------------- what a page is called */

    /// Whether this document numbers its pages its own way.
    pub fn has_labels(&self) -> bool {
        !self.labels.is_empty()
    }

    /// What to call a page, one-based, when showing it to a reader.
    pub fn label(&self, page: usize) -> String {
        match self.labels.get(page.wrapping_sub(1)) {
            Some(label) if !label.is_empty() => label.clone(),
            _ => page.to_string(),
        }
    }

    /// The page a reader means by what they typed.
    ///
    /// A label first, because that is what is printed on the page and what an
    /// index cites; the position in the file second, so that "page 7" still
    /// finds something in a document whose seventh page is called "vii" — and
    /// because there is otherwise no way at all to reach a page whose label is
    /// blank. `pageForLabel` in `viewer.ts`, in that order and for that
    /// reason.
    pub fn page_for_label(&self, text: &str) -> Option<usize> {
        let wanted = text.trim();
        if wanted.is_empty() {
            return None;
        }
        let folded = wanted.to_lowercase();
        if let Some(at) = self
            .labels
            .iter()
            .position(|label| label.to_lowercase() == folded)
        {
            return Some(at + 1);
        }
        let number: usize = wanted.parse().ok()?;
        (number >= 1 && number <= self.pages()).then_some(number)
    }

    /// What the field in the toolbar has in it.
    pub fn page_field(&self) -> String {
        if self.typing_page {
            self.page_typed.clone()
        } else {
            self.label(self.page())
        }
    }

    /// Put the reader in the page field, empty. `focusPageNumber` in
    /// `main.ts`, which selects the field's contents instead — see
    /// [`Viewer::typing_page`] for why emptying it is the same gesture here.
    pub fn open_page_field(&mut self) {
        self.typing_page = true;
        self.page_typed.clear();
    }

    pub fn type_page(&mut self, text: &str) {
        self.typing_page = true;
        self.page_typed = text.to_string();
    }

    /// Go where the field says, or say that it says nowhere.
    ///
    /// Either way the field stops being typed in and goes back to naming the
    /// page the reader is on, which is what the app does by putting the
    /// current label back into it.
    pub fn commit_page(&mut self) {
        let typed = std::mem::take(&mut self.page_typed);
        self.typing_page = false;
        if typed.trim().is_empty() {
            return;
        }
        match self.page_for_label(&typed) {
            Some(page) => self.go_to_page(page),
            None => {
                self.notice = format!("There is no page {} in this document", typed.trim());
            }
        }
    }

    pub fn cancel_page(&mut self) {
        self.typing_page = false;
        self.page_typed.clear();
    }

    /// What a mark on this page would be called: the section it falls in.
    ///
    /// `sectionFor` in `sidebar.ts`, and for its reason — a mark named for
    /// the chapter it sits in is worth a great deal more than one named
    /// "Page 214", and the outline has already been walked.
    pub fn section_for(&self, page: usize) -> String {
        crate::sidebar::heading_for(&self.headings, page)
            .map(|at| self.headings[at].title.clone())
            .unwrap_or_default()
    }

    /// Put a pin in a page or take it out — the same gesture doing the same
    /// thing, which is what `toggle_mark` is.
    pub fn mark_page(&mut self, page: usize) {
        let title = self.section_for(page);
        let marked = self.store.toggle_mark(page, &title);
        self.notice = if marked {
            format!("Marked page {page}")
        } else {
            format!("Took the mark off page {page}")
        };
        // The panel opens on the pages when a document has no contents, and a
        // document with a mark in it has something to show there after all —
        // which is the rule `showMarks` follows, and the reason it is that
        // narrow: taking the reader off the thumbnails they were looking at,
        // for a list they can see is there, is the panel arguing.
        if marked && self.sidebar_open && self.tab == Tab::Pages && self.headings.is_empty() {
            self.tab = Tab::Contents;
        }
    }

    /* ------------------------------------------------------- the search */

    /// Put the find bar up. Nothing is searched for until something is typed.
    pub fn open_find(&mut self) {
        self.find_open = true;
        if self.sidebar_open && !self.search.query().is_empty() {
            self.tab = Tab::Results;
        }
    }

    /// Take the find bar down, and the index with it.
    ///
    /// **The index goes when the bar does**, which is the app's policy and its
    /// reasoning: every page ever scanned is kept, so a long book costs tens
    /// of megabytes for as long as it is open — a fair trade while somebody is
    /// searching and no trade at all once they have stopped. Reopening rescans
    /// and that is under half a second. See [`Search::forget`].
    pub fn close_find(&mut self) {
        self.find_open = false;
        self.find_query.clear();
        self.search.forget();
        self.scan += 1;
        if self.tab == Tab::Results {
            self.tab = if self.headings.is_empty() {
                Tab::Pages
            } else {
                Tab::Contents
            };
        }
    }

    /// Look for what is in the field. Returns the token of the scan it
    /// started, or `None` when there is nothing to scan — which is what the
    /// caller needs to know before spawning a task to drive it.
    pub fn find(&mut self, query: &str) -> Option<u64> {
        self.find_query = query.to_string();
        self.scan += 1;
        self.revealed = false;
        let (page, pages) = (self.page(), self.pages());
        if !self.search.find(query, page, pages) {
            return None;
        }
        if self.sidebar_open {
            self.tab = Tab::Results;
        }
        Some(self.scan)
    }

    /// Read pages until the slice is up. Returns whether there is more to do.
    ///
    /// The whole of the streaming, and it is here rather than in
    /// [`crate::search`] because it is the only part that needs a clock and a
    /// document. A slice is [`crate::search::SLICE_MS`] and the reason there
    /// is one at all is a book, not a page: pdfium reads a page of the
    /// 400-page fixture in 0.18ms and a page of a 376-page book of typeset
    /// mathematics in 1.3ms, so the cost worth hiding is the 498ms the whole
    /// of that book takes rather than anything a single page does.
    pub fn scan_slice(&mut self, token: u64) -> bool {
        if token != self.scan {
            return false;
        }
        let began = std::time::Instant::now();
        while let Some(page) = self.search.wants() {
            let document = self.document.clone();
            self.search.feed(page, || document.text_of(page - 1));
            if began.elapsed().as_secs_f64() * 1000.0 > crate::search::SLICE_MS {
                break;
            }
        }
        self.search.publish();
        // The first result to arrive is the one the reader is taken to, and
        // only the first: after that they are moved by asking rather than by
        // the scan catching up with them somewhere else in the book.
        if !self.revealed && self.search.current().is_some() {
            self.revealed = true;
            self.reveal_match();
        }
        self.search.wants().is_some()
    }

    /// Move to the next match, or the one before, and go there.
    pub fn step_match(&mut self, forwards: bool) {
        if self.search.matches().is_empty() {
            self.notice = if self.search.state().textless {
                "There is no text in this document to search".into()
            } else {
                "No matches".into()
            };
            return;
        }
        self.search.step(forwards);
        self.reveal_match();
    }

    /// Go to one result by its place in the list — a row of the Results tab.
    pub fn go_to_result(&mut self, at: usize) {
        self.search.go_to(at);
        self.reveal_match();
    }

    /// Bring the match the reader is on into view.
    ///
    /// A match is a range of characters and a character knows its box, so this
    /// is arithmetic: the top of the page, plus where on the page the match
    /// is, less a third of a screen so that there is something above it to
    /// read into. The app measures a DOM range against a text layer to reach
    /// the same number.
    pub fn reveal_match(&mut self) {
        let Some(hit) = self.search.current() else {
            return;
        };
        let Some(page) = self.layout.box_of(hit.page - 1) else {
            // Paged mode lays out one page and leaves the rest of `boxes`
            // empty, so a match anywhere else is a page to turn to rather
            // than a place on one. `revealMatch` in `viewer.ts` says the same
            // thing: the other pages have no place on the strip at all.
            self.go_to(Anchor {
                page: hit.page,
                offset: 0.0,
            });
            return;
        };
        // Where the match lands on the *page as it is being shown*, which a
        // quad in the page's own points is not once the reader has turned or
        // trimmed it. One call rather than a multiplication by the scale —
        // see [`Layout::place_on`].
        let top = self
            .search
            .quads_on(hit.page)
            .into_iter()
            .filter(|(_, current)| *current)
            .map(|(quad, _)| self.layout.place_on(hit.page - 1, quad).top)
            .fold(f64::INFINITY, f64::min);
        let target = if top.is_finite() {
            page.top + top - self.layout.viewport.height * REVEAL
        } else {
            // A match nothing drew — pdfium generates characters the printer
            // never put on the page — is still on a page.
            page.top
        };
        self.scroll_to(target);
    }

    /// Change one of the two switches that decide what is found, and look
    /// again with it.
    ///
    /// The extracted text stays: only the fold and the boundary test depend on
    /// these, so a rescan after this asks the renderer for nothing — which is
    /// what `changing_the_case_setting_does_not_go_back_to_the_renderer` in
    /// `search.rs` holds it to.
    pub fn set_find_options(&mut self, options: Find) -> Option<u64> {
        self.search.set_options(options);
        self.store.set(vec![
            ("search_match_case".into(), json!(options.match_case)),
            ("search_whole_words".into(), json!(options.whole_words)),
        ]);
        let query = self.find_query.clone();
        self.find(&query)
    }

    /// Paint every match, or only the one the reader is on.
    pub fn toggle_highlight_all(&mut self) {
        self.highlight_all = !self.highlight_all;
        self.store
            .set(vec![("search_highlight_all".into(), json!(self.highlight_all))]);
    }

    /// What to paint over one page, in CSS pixels from its top left.
    ///
    /// `(rectangle, is the one the reader is on)`. Empty when the find bar is
    /// down, because a highlight that outlives the bar that made it is a mark
    /// on the page nobody asked for.
    pub fn highlights(&self, page: usize) -> Vec<(Rect, bool)> {
        if !self.find_open {
            return Vec::new();
        }
        if self.layout.box_of(page - 1).is_none() {
            return Vec::new();
        }
        self.search
            .quads_on(page)
            .into_iter()
            .filter(|(_, current)| self.highlight_all || *current)
            .map(|(quad, current)| (self.layout.place_on(page - 1, quad), current))
            .collect()
    }

    /// What the find bar says beside the field: where the reader is in the
    /// matches, or why there are none.
    pub fn find_count(&self) -> String {
        let state = self.search.state();
        if state.query.trim().is_empty() {
            return String::new();
        }
        if state.total == 0 {
            return if state.scanning {
                "Searching…".into()
            } else if state.textless {
                "No text to search".into()
            } else {
                "None".into()
            };
        }
        let at = state.at.map(|at| at + 1).unwrap_or(0);
        let more = if state.capped {
            "+"
        } else if state.scanning {
            "…"
        } else {
            ""
        };
        format!("{at} of {}{more}", state.total)
    }

    /// Relay out around the page the reader is on, which is what every change
    /// of fit, zoom or spread has to do.
    fn keeping_place(&mut self, change: impl FnOnce(&mut Layout)) {
        let anchor = self.layout.anchor(self.scroll_top);
        change(&mut self.layout);
        self.layout.relayout();
        self.scroll_top = self.layout.scroll_target(anchor);
        self.generation += 1;
    }

    pub fn set_fit(&mut self, fit: Fit) {
        self.keeping_place(|layout| layout.fit = fit);
        self.notice = match fit {
            Fit::Width => "Fit width".into(),
            Fit::Page => "Fit page".into(),
            Fit::Actual => "Actual size".into(),
        };
        self.store.set(vec![("fit_mode".into(), json!(name_of(fit)))]);
    }

    /// Actual size, which is a fit mode *and* a zoom of 1.
    ///
    /// The pair that never moves alone, and the reason `store.set` takes a
    /// group: written one at a time, a zoom of 1 lands under a fit mode that
    /// ignores it and the next run comes back fitted to the width.
    pub fn actual_size(&mut self) {
        self.keeping_place(|layout| {
            layout.fit = Fit::Actual;
            layout.zoom = 1.0;
        });
        self.notice = "Actual size".into();
        self.store.set(vec![
            ("zoom".into(), json!(1.0)),
            ("fit_mode".into(), json!(name_of(Fit::Actual))),
        ]);
    }

    pub fn zoom(&mut self, closer: bool) {
        let current = if self.layout.fit == Fit::Actual {
            self.layout.zoom
        } else {
            // Leaving a fit mode starts from where the fit had got to, so the
            // first press changes the size by one step rather than jumping.
            self.layout
                .box_of(self.page() - 1)
                .map(|page| page.scale / crate::layout::PDF_TO_CSS_UNITS)
                .unwrap_or(1.0)
        };
        let next = if closer {
            ZOOMS.iter().copied().find(|&step| step > current + 0.001)
        } else {
            ZOOMS
                .iter()
                .copied()
                .rev()
                .find(|&step| step < current - 0.001)
        };
        let Some(next) = next else { return };
        self.keeping_place(|layout| {
            layout.fit = Fit::Actual;
            layout.zoom = next;
        });
        self.notice = format!("{:.0}%", next * 100.0);
        // The pair that never moves alone, and the reason `set` takes a group:
        // a zoom without the fit mode it left comes back as a fit width that
        // ignores it.
        self.store.set(vec![
            ("zoom".into(), json!(next)),
            ("fit_mode".into(), json!(name_of(Fit::Actual))),
        ]);
    }

    /// Take the margins off, or put them back.
    ///
    /// The switch is remembered and the measurement is not: `trim_margins` is
    /// a setting in the app's own `settings.rs`, which this crate mounts, and
    /// the crop it produces is a fact about a document rather than about the
    /// reader. So a run that opens a different document measures that one.
    ///
    /// **Left on when there is nothing to trim.** A document with no margins
    /// to speak of, or one this misread, keeps the switch and loses nothing —
    /// [`Viewer::trimmed`] is how the interface says which of the two
    /// happened, and it is the difference between "off" and "on, and there
    /// was nothing there".
    pub fn set_trim(&mut self, on: bool) {
        self.trimming = on;
        self.store.set(vec![("trim_margins".into(), json!(on))]);
        if !on {
            self.keeping_place(|layout| layout.crop = None);
            self.notice = "Margins put back".into();
            return;
        }
        self.measure_crop();
        self.notice = if self.trimmed() {
            "Margins trimmed".into()
        } else {
            "There are no margins to trim on this document".into()
        };
    }

    pub fn trims_margins(&self) -> bool {
        self.trimming
    }

    /// Whether anything was actually found to trim.
    pub fn trimmed(&self) -> bool {
        self.layout.crop.is_some()
    }

    /// Measure the document and lay it out again against what was found.
    ///
    /// The crop is measured on the page as the *document* has it and then
    /// turned to match the page as the reader has it, which is the same order
    /// [`Layout::turn`] keeps: measuring after a rotation would ask pdfium to
    /// draw eight pages sideways for an answer that is one transposition away
    /// from the one already in hand.
    fn measure_crop(&mut self) {
        let mut crop = crate::crop::measure(&self.document);
        let mut turns = (self.layout.rotation / 90) % 4;
        while turns > 0 {
            crop = crop.map(crate::layout::Crop::turned);
            turns -= 1;
        }
        self.keeping_place(|layout| layout.crop = crop);
    }

    /// Turn the document a quarter at a time.
    ///
    /// Nothing is written down: a rotation is a way of looking rather than a
    /// property of the file, which is what `viewer.ts` says of it and what
    /// Preview, Acrobat and Sumatra all do.
    ///
    /// **And no cache is thrown away**, which is where this parts company
    /// with the app. There `rotate()` clears the links, the notes and the
    /// markup, because all three are held as fractions of a page that has
    /// just changed shape. Here they are held in the page's own unturned
    /// points and [`Layout::place_on`] does the turning where they are drawn,
    /// so a rotation costs a relayout and the pages that were on screen.
    pub fn rotate(&mut self, quarter_turns: i32) {
        self.keeping_place(|layout| layout.turn(quarter_turns));
        self.notice = match self.layout.rotation {
            0 => "Upright".into(),
            degrees => format!("Turned {degrees}°"),
        };
    }

    pub fn set_spread(&mut self, spread: Spread) {
        self.keeping_place(|layout| layout.spread = spread);
        self.store.set(vec![(
            "spread_mode".into(),
            json!(match spread {
                Spread::Single => "single",
                Spread::Two => "two",
                Spread::Cover => "cover",
            }),
        )]);
    }

    /// Wear the theme at `index` in the list, and remember it.
    pub fn set_theme(&mut self, index: usize) {
        let name = self.store.wear(index);
        if name.is_empty() {
            return;
        }
        // Every mounted page reads this on its next paint, and the next paint
        // is the frame this change causes. A page already on the GPU is
        // recoloured by a compute pass over it rather than drawn again, which
        // is the whole difference from `keyFor()` carrying the theme.
        self.chosen.set(self.store.palette());
        self.notice = self.store.complaint.clone().unwrap_or(name);
        self.generation += 1;
    }

    /// The next theme in the list, which is what `t` is bound to.
    ///
    /// Fourteen themes is too many to cycle through in the real app and this
    /// is not the real app's gesture — the menu is Phase 3's — but it is the
    /// one keystroke that proves the whole list is loaded and wearable.
    pub fn next_theme(&mut self) {
        let next = (self.store.theme_index() + 1) % self.store.themes().len().max(1);
        self.set_theme(next);
    }

    /// Where the reader would end up, clamped, in CSS pixels.
    pub fn scroll_by(&self, delta: f64) -> f64 {
        (self.scroll_top + delta).clamp(0.0, self.layout.max_scroll())
    }

    /// Move the document under the window. The one place `scroll_top` changes.
    pub fn scroll_to(&mut self, top: f64) -> bool {
        let to = top.clamp(0.0, self.layout.max_scroll());
        if (to - self.scroll_top).abs() < 0.01 {
            return false;
        }
        self.scroll_top = to;
        // The column follows the document, and only when it has to — see
        // `Column::reveal`. `setPage` in `sidebar.ts` is this call.
        self.reveal_thumb();
        true
    }

    pub fn page_target(&self, page: usize) -> f64 {
        self.layout.scroll_target(Anchor {
            page: page.clamp(1, self.pages().max(1)),
            offset: 0.0,
        })
    }
}

/// One mounted page as the `rsx!` block needs it: which page, where its box
/// is, and the two overlays that go on top of it.
///
/// A struct rather than a tuple since links joined the highlights and made it
/// seven things. The tuple was already at clippy's limit and reading the
/// fourth `f64` in a row off its position was the sort of thing that is
/// correct until somebody inserts a field.
struct Placed {
    index: usize,
    top: f64,
    left: f64,
    width: f64,
    height: f64,
    hits: Vec<(Rect, bool)>,
    links: Vec<(Rect, Target)>,
}

/// The element that wants the keyboard, as a selector.
///
/// **A click takes the focus away from the reader, and the reader cannot take
/// it back.** Blitz walks up from whatever was clicked looking for something
/// it knows how to focus — a text input, a checkbox, a summary, a link — and
/// a plain `<button>` is not on that list, so the focus ends up on `<html>`.
/// A key with nothing focused goes to `<html>` too, which is above anything a
/// component can put a handler on, so from the first click onwards every
/// shortcut in this reader did nothing. It was invisible for two phases
/// because no test had ever pressed a key *after* clicking something.
///
/// And it cannot be answered from inside the reader. The one way to ask for
/// the focus is `MountedData::set_focus`, which takes `doc_mut()` the moment
/// it is called — and every place a component could call it from is already
/// inside a borrow of the document, including a task spawned from one, which
/// is polled inside that borrow as well. It panics with "RefCell already
/// borrowed" from a stack naming neither.
///
/// So the element that wants the keyboard says so, and whoever owns the
/// window hands it back: `shell.rs` after a click in the real app, and the
/// harness after a synthesised one. That is the same division the app makes —
/// `reclaimKeyboard()` in `main.ts` is the window's answer to the same
/// problem, because a full-screen change costs the page its keyboard there in
/// exactly this way.
pub const KEYBOARD: &str = "[data-keyboard]";

/// Give the keyboard back to the element that asked for it, unless something
/// inside it has taken the focus.
///
/// The condition is the whole of the policy: focus that lands *inside* the
/// reader belongs to whatever took it — a field in the find bar, when there
/// is one — and focus that lands anywhere else is focus nobody wanted, which
/// is what a click on a button leaves behind.
pub fn give_keyboard_back(doc: &mut blitz_dom::BaseDocument) {
    // **The innermost element that asks for it wins**, which is why this is
    // `query_selector_all` and takes the last. The reader's root always asks;
    // the find bar's field asks as well while it is up, and it is a
    // descendant, so it comes later in document order. Without that rule a
    // click on "Match case" would hand the keyboard back to the root and the
    // next thing typed would scroll the document instead of changing the
    // query — which is the same complaint as the bug this whole function
    // exists for, one level in.
    let Ok(wants) = doc.query_selector_all(KEYBOARD) else {
        return;
    };
    let Some(wants) = wants.last().copied() else {
        return;
    };
    if let Some(focused) = doc.get_focussed_node_id() {
        let mut node = Some(focused);
        while let Some(id) = node {
            if id == wants {
                return;
            }
            node = doc.get_node(id).and_then(|node| node.parent);
        }
    }
    doc.set_focus_to(wants);
}

/// The whole window.
///
#[component]
pub fn Reader(document: Handle, chosen: Chosen, config: Config) -> Element {
    let mut viewer = use_signal(|| {
        let mut store = Store::at(&config.dir);
        if let Some(index) = config.theme {
            store.wear_for_now(index);
        }
        Viewer::new(document.0.clone(), chosen.clone(), store)
    });
    // The viewport, taken from the window rather than from the element.
    //
    // `get_client_rect` is the obvious way and it panics: a `MountedData` call
    // borrows the document, and every place a component can call one from — a
    // DOM event handler, a mounted handler — is already inside a borrow of it.
    // The window is the one measurement that costs nothing to ask for, and the
    // chrome above and below the document is a number this file knows. The
    // scroll event carries the real client size, so the first scroll corrects
    // whatever this got wrong.
    let screen = use_hook(|| {
        dioxus_core::try_consume_context::<Screen>()
            // Nothing provided one, which is a shell that forgot rather than a
            // situation to cope with — so the number is stated rather than
            // guessed, and it is the one `main.rs` defaults to.
            .unwrap_or_else(|| Screen::fixed(1100.0, 900.0, 1.0))
    });
    // Where a link out of the document goes. See [`Away`]: the default is the
    // system browser, and a harness provides its own.
    let away = use_hook(|| {
        dioxus_core::try_consume_context::<Away>().unwrap_or_else(Away::to_the_system)
    });

    let resize_from_window = {
        let screen = screen.clone();
        move |mut viewer: Signal<Viewer>| {
            let (width, height, _scale) = screen.get();
            viewer.write().resize(width, (height - CHROME).max(120.0));
        }
    };

    // Every key the reader can press is an *action*, and a chord is only a
    // way of asking for one. What was here before was a `match` on
    // `event.key()` — which is the shape the app spent a rewrite getting out
    // of, and it could not express ⌘0 at all: a modifier was something an arm
    // had to remember to check, so the arms that did not check quietly
    // answered chords nobody had pressed.
    //
    // Now the event is turned into a chord and the chord is looked up. The
    // table is `keymap.rs` and the file over the top of it is `keys.toml`,
    // both of them the app's own — see `crate::keymap`. What is left here is
    // this: work out what was asked for, and do it.
    let on_key = move |event: KeyboardEvent| {
        let (press, screen) = {
            let held = viewer.read();
            (
                held.keymap.press(
                    &event.key(),
                    event.code(),
                    event.modifiers(),
                    &held.pending,
                ),
                held.layout.viewport.height,
            )
        };
        match press {
            // `g`, on its way to `g g`. A sequence half pressed and then
            // abandoned is dropped by the next chord that continues nothing,
            // rather than by a timer: there is no `setTimeout` here, and the
            // app's 1200ms one is a nicety rather than the behaviour.
            Press::Wait(prefix) => viewer.write().pending = prefix,
            Press::Nothing => viewer.write().pending.clear(),
            Press::Act(action) => {
                viewer.write().pending.clear();
                perform(viewer, action, screen);
            }
        }
    };

    // What drives a scan: a task per search, stopped by its token going stale.
    // There is no timer and no `setTimeout` — a slice yields to the event
    // loop, is woken by dioxus's own scheduler, and comes back on the next
    // turn. See `Breathe` at the bottom of this file.
    let scan = move |token: Option<u64>| {
        let Some(token) = token else { return };
        let mut viewer = viewer;
        spawn(async move {
            while viewer.write().scan_slice(token) {
                Breathe::once().await;
            }
        });
    };

    let held = viewer.read();
    let scroll_top = held.scroll_top;
    let wearing = held.palette();
    let theme_name = held.theme_name();
    let mounted = held.layout.mounted(held.scroll_top);
    let content_width = held.layout.content_width();
    let content_height = held.layout.content_height();
    let pages = held.pages();
    let notice = held.notice.clone();
    let sidebar_open = held.sidebar_open;
    let find_open = held.find_open;
    let find_query = held.find_query.clone();
    let find_count = held.find_count();
    let find_options = held.search.options();
    let highlight_all = held.highlight_all;
    let marked = held.store.is_marked(held.page());
    let page_field = held.page_field();
    let typing_page = held.typing_page;
    let zoom = match held.layout.fit {
        Fit::Width => "Fit width".to_string(),
        Fit::Page => "Fit page".to_string(),
        Fit::Actual => format!("{:.0}%", held.layout.zoom * 100.0),
    };
    let boxes: Vec<Placed> = mounted
        .iter()
        .filter_map(|&index| {
            held.layout.box_of(index).map(|page| Placed {
                index,
                top: page.top,
                left: page.left,
                width: page.width,
                height: page.height,
                hits: held.highlights(index + 1),
                links: held.link_areas(index + 1),
            })
        })
        .collect();
    let document = held.document.clone();
    // How every page is drawn, and the string that says so in a key. A turn
    // of 180° leaves a page exactly the shape it was, so the box's size
    // cannot stand in for this: the pixels differ and nothing else would say
    // so. See `page.rs` on why the key is what invalidates a texture.
    let view = held.layout.view();
    let view_key = match view.crop {
        Some(crop) => format!(
            "{}|{:.3},{:.3},{:.3},{:.3}",
            view.rotation, crop.x, crop.y, crop.width, crop.height
        ),
        None => format!("{}|whole", view.rotation),
    };
    let trimming = held.trims_margins();
    let chosen = chosen.clone();
    drop(held);

    let variables = crate::styles::variables(&wearing);

    rsx! {
        // Never rewritten. See `styles.rs`: the theme is on the root as
        // variables, because a stylesheet that changes while something is
        // hovered takes Stylo down.
        style { {crate::styles::SHEET} }
        div {
            class: "root",
            style: "{variables}",
            tabindex: 0,
            onkeydown: on_key,
            // A key with nothing focused goes to the *root element*, which is
            // `<html>` — above anything a component can put a handler on. So
            // the reader's own root takes the focus as soon as it exists, and
            // every key arrives here.
            onmounted: move |event| {
                let node = event.data();
                let task = node.set_focus(true);
                spawn(async move {
                    let _ = task.await;
                });
            },
            // And says so, for whoever owns the window: see [`KEYBOARD`] and
            // `give_keyboard_back`. A click takes the focus away from here
            // and a component cannot take it back.
            "data-keyboard": "reader",
            // The sidebar's resize handle starts a drag but cannot track it:
            // dragging widens the panel, which moves the pointer out from
            // under whatever element it started on. These sit on the root
            // instead — DOM events bubble, and the root is the one ancestor
            // that spans the whole window regardless of which side the
            // pointer ends up over. `drag_sidebar` is a no-op with nothing to
            // drag, so an ordinary mouse move elsewhere is one `read` and
            // nothing more — `write` marks the signal dirty on every call
            // whether or not anything changed, and every move in the window
            // reaches here.
            onmousemove: move |event| {
                if viewer.read().resize_from.is_none() {
                    return;
                }
                let x = event.client_coordinates().x;
                viewer.write().drag_sidebar(x);
            },
            onmouseup: move |_| {
                if viewer.read().resize_from.is_some() {
                    viewer.write().finish_resize_sidebar();
                }
            },
            div { class: "toolbar",
                button {
                    // Not `.sidebar`, which is the panel itself: a selector
                    // that matches the button *and* the thing the button
                    // opens is a test that cannot tell them apart.
                    class: if sidebar_open { "chip contents on" } else { "chip contents" },
                    onclick: move |_| viewer.write().toggle_sidebar(),
                    "Contents"
                }
                div { class: "title", "{pages} pages" }
                div { class: "spacer" }
                button {
                    class: if marked { "chip mark on" } else { "chip mark" },
                    onclick: move |_| { let page = viewer.read().page(); viewer.write().mark_page(page); },
                    if marked { "Marked" } else { "Mark" }
                }
                button {
                    // On means the reader asked, not that anything was found:
                    // a document with nothing to trim keeps the switch and
                    // says so on the notice line. See [`Viewer::set_trim`].
                    class: if trimming { "chip trim on" } else { "chip trim" },
                    onclick: move |_| { let on = viewer.read().trims_margins(); viewer.write().set_trim(!on); },
                    if trimming { "Trimmed" } else { "Trim" }
                }
                button { class: "chip fit", onclick: move |_| viewer.write().set_fit(Fit::Width), "{zoom}" }
                button { class: "chip zoom-out", onclick: move |_| viewer.write().zoom(false), "−" }
                button { class: "chip zoom-in", onclick: move |_| viewer.write().zoom(true), "+" }
                button { class: "chip theme", onclick: move |_| viewer.write().next_theme(), "{theme_name}" }
                // The page field, which is a field rather than a readout for
                // the app's own reason: stepping is fine for nudging and
                // hopeless for arriving, and a reader with a citation in
                // front of them has a number to type. What it shows is the
                // page's *label* — see [`Viewer::label`].
                div { class: "pill",
                    // **A readout that becomes a field, rather than a field
                    // that is always one**, which is where this parts company
                    // with the app — and it is Blitz's focus rule that decides
                    // it, not taste. The keyboard is handed back to the
                    // innermost element asking for it (see
                    // [`give_keyboard_back`]), so a field that is always in
                    // the toolbar either always asks — and then every
                    // keystroke in the reader goes into it — or stops asking
                    // while still holding the focus, which is the same dead
                    // keyboard one level along. The find bar has neither
                    // problem because its field *stops existing* when the bar
                    // closes, and the focus goes with it. This is that
                    // mechanism, borrowed.
                    if typing_page {
                    input {
                        class: "page-field",
                        r#type: "text",
                        value: "{page_field}",
                        "aria-label": "Go to page",
                        "data-keyboard": "goto",
                        onmounted: move |event| {
                            let node = event.data();
                            let task = node.set_focus(true);
                            spawn(async move { let _ = task.await; });
                        },
                        oninput: move |event| {
                            let typed = event.value();
                            viewer.write().type_page(&typed);
                        },
                        // The same two rules the find field has, and for the
                        // same two reasons: a plain key typed here would
                        // otherwise bubble to the root and scroll the
                        // document, and Blitz applies a keystroke to a focused
                        // field whatever modifier is held down.
                        onkeydown: move |event| {
                            let key = event.key();
                            let modifiers = event.modifiers();
                            let plain = !modifiers.meta() && !modifiers.ctrl() && !modifiers.alt();
                            match key {
                                Key::Enter => {
                                    event.stop_propagation();
                                    viewer.write().commit_page();
                                }
                                Key::Escape => {
                                    event.stop_propagation();
                                    viewer.write().cancel_page();
                                }
                                _ if plain => event.stop_propagation(),
                                Key::Character(ref typed)
                                    if matches!(typed.as_str(), "a" | "c" | "v" | "x" | "z") => {}
                                _ => event.prevent_default(),
                            }
                        },
                    }
                    } else {
                    button {
                        class: "page-now",
                        "aria-label": "Go to page",
                        onclick: move |_| viewer.write().open_page_field(),
                        "{page_field}"
                    }
                    }
                    span { class: "of", "/ {pages}" }
                }
            }
            // Not a popover and not over anything: the root is a flex column,
            // so the bar is a row in it and the document is what gets shorter.
            // In the app it is `position: fixed` with a list of the places the
            // pointer may go without dismissing it; here there is nothing to
            // dismiss it from.
            if find_open {
                div { class: "findbar",
                    input {
                        class: "find-field",
                        r#type: "text",
                        value: "{find_query}",
                        placeholder: "Search this document",
                        // While the bar is up, this is the element that wants
                        // the keyboard — and it is inside the one that
                        // otherwise does, which is what makes the rule in
                        // `give_keyboard_back` "the innermost one asking".
                        //
                        // Unless the page field is up, which is the one case
                        // where two of them would ask at once: they are
                        // siblings rather than one inside the other, so
                        // "innermost" cannot separate them and would settle it
                        // by document order — which would hand ⌥⌘G's field to
                        // the find bar. Two fields never both ask.
                        "data-keyboard": if typing_page { None } else { Some("find") },
                        onmounted: move |event| {
                            let node = event.data();
                            let task = node.set_focus(true);
                            spawn(async move { let _ = task.await; });
                        },
                        oninput: move |event| {
                            let typed = event.value();
                            let token = viewer.write().find(&typed);
                            scan(token);
                        },
                        // Every key typed here also bubbles to the root, and
                        // the root turns keys into actions — so without this,
                        // typing "just" into the field scrolls the document
                        // four times on the way. What the field lets past is
                        // a chord with a modifier on it: ⌘+ still zooms while
                        // somebody is searching, exactly as it does in the
                        // app.
                        onkeydown: move |event| {
                            let key = event.key();
                            let modifiers = event.modifiers();
                            let plain = !modifiers.meta() && !modifiers.ctrl() && !modifiers.alt();
                            match key {
                                // Enter is the find bar's own, and is not in
                                // `keys.toml`: it means "the next one" here
                                // and nothing anywhere else.
                                Key::Enter => {
                                    event.stop_propagation();
                                    viewer.write().step_match(!modifiers.shift());
                                }
                                Key::Escape => {
                                    event.stop_propagation();
                                    viewer.write().close_find();
                                }
                                _ if plain => event.stop_propagation(),
                                // A chord with a modifier on it is not
                                // typing — and Blitz applies the keystroke to
                                // a focused field whatever is held down, so
                                // ⌘G stepped to the next match *and* put a
                                // "g" in the query, which started a search
                                // for something nobody typed. What the field
                                // keeps is what a text field owns; everything
                                // else is prevented here and answered on the
                                // root, where the keymap is.
                                Key::Character(ref typed)
                                    if matches!(typed.as_str(), "a" | "c" | "v" | "x" | "z") => {}
                                _ => event.prevent_default(),
                            }
                        },
                    }
                    div { class: "find-count", "{find_count}" }
                    button {
                        class: "chip find-previous",
                        "aria-label": "Previous match",
                        onclick: move |_| viewer.write().step_match(false),
                        "‹"
                    }
                    button {
                        class: "chip find-next",
                        "aria-label": "Next match",
                        onclick: move |_| viewer.write().step_match(true),
                        "›"
                    }
                    button {
                        class: if find_options.match_case { "chip find-case on" } else { "chip find-case" },
                        onclick: move |_| {
                            let token = viewer.write().set_find_options(crate::search::Options {
                                match_case: !find_options.match_case,
                                whole_words: find_options.whole_words,
                            });
                            scan(token);
                        },
                        "Match case"
                    }
                    button {
                        class: if find_options.whole_words { "chip find-words on" } else { "chip find-words" },
                        onclick: move |_| {
                            let token = viewer.write().set_find_options(crate::search::Options {
                                match_case: find_options.match_case,
                                whole_words: !find_options.whole_words,
                            });
                            scan(token);
                        },
                        "Whole words"
                    }
                    button {
                        class: if highlight_all { "chip find-all on" } else { "chip find-all" },
                        onclick: move |_| viewer.write().toggle_highlight_all(),
                        "Highlight all"
                    }
                    button {
                        class: "chip find-close",
                        onclick: move |_| viewer.write().close_find(),
                        "Done"
                    }
                }
            }
            div { class: "body",
            if sidebar_open {
                Sidebar {
                    viewer,
                    document: Handle(document.clone()),
                    chosen: chosen.clone(),
                }
            }
            div {
                class: "viewer",
                onmounted: move |_| resize_from_window(viewer),
                onwheel: move |event| {
                    // A trackpad sends pixels and a mouse sends lines; both
                    // arrive here, and a line is what the app calls a line.
                    // The sign is the platform's, not the DOM's: winit hands
                    // over a negative y for reading forwards and Blitz's own
                    // scroller negates it, so this negates it too rather than
                    // scrolling the opposite way from every other window on
                    // the machine.
                    let delta = -match event.delta() {
                        WheelDelta::Pixels(delta) => delta.y,
                        WheelDelta::Lines(delta) => delta.y * LINE,
                        WheelDelta::Pages(delta) => delta.y * viewer.read().layout.viewport.height,
                    };
                    viewer.write().nudge(delta);
                },
                div {
                    class: "pages",
                    style: "width: {content_width}px; height: {content_height}px;",
                    // The one thing the reader can see and the DOM cannot
                    // say. Everything else the harness asserts on is text in
                    // the toolbar, which is better because it is what somebody
                    // reading the screen would check; a scroll offset is a
                    // number with no pixels of its own.
                    "data-scroll": "{scroll_top}",
                    for placed in boxes {
                        Page {
                            // What `keyFor()` is: the page, the size it is
                            // drawn at, and the theme it is wearing. A change
                            // to any of them is a different node, which is
                            // what gives the old texture back — see `page.rs`.
                            key: "{placed.index}:{placed.width}x{placed.height}:{theme_name}:{view_key}",
                            document: Handle(document.clone()),
                            chosen: chosen.clone(),
                            index: placed.index,
                            top: placed.top - scroll_top,
                            left: placed.left,
                            width: placed.width,
                            height: placed.height,
                            hits: placed.hits,
                            links: placed.links,
                            view,
                            viewer,
                            away: away.clone(),
                        }
                    }
                }
            }
            }
            div { class: "notice", "{notice}" }
        }
    }
}

/// One page, in its place.
#[component]
fn Page(
    document: Handle,
    chosen: Chosen,
    index: usize,
    top: f64,
    left: f64,
    width: f64,
    height: f64,
    /// Where the matches on this page are, in CSS pixels from its top left,
    /// and which of them the reader is on.
    ///
    /// **These are nodes, not pixels, and that is the whole of the port.**
    /// `paintSelection` and the highlight painting in `viewer.ts` copy the
    /// page canvas, run the copy through a luminance ramp and lay it back
    /// down, because a `::selection` colour puts pdf.js's text layer on
    /// screen and a page's bold type comes back regular. There is no text
    /// layer here and nothing to put on screen: a match is a rectangle in
    /// PDF points, so it is a `div` over the page in the theme's own
    /// selection colours, and the glyphs underneath it are the ones pdfium
    /// drew.
    hits: Vec<(Rect, bool)>,
    /// The document's own links on this page, in the same space as `hits`.
    ///
    /// A node each, for the reason the highlights are nodes: there is no text
    /// layer here to hang an anchor off, and a rectangle is what a link
    /// actually is in the file.
    links: Vec<(Rect, Target)>,
    /// How this page is turned and how much of it is drawn. In the key as
    /// well, which is what gives the old texture back — see [`PageWidget`].
    view: crate::layout::View,
    /// Following a link is a jump, and a jump is the viewer's.
    viewer: Signal<Viewer>,
    /// …unless it leads out of the document, which is the window's. See
    /// [`Away`].
    away: Away,
) -> Element {
    // The widget is handed over the first time the attribute is set, so
    // `use_hook` is what keeps a re-render from building a second one — and
    // what makes a page that merely moved keep the texture it has.
    let widget = use_hook(|| {
        let shell = dioxus_core::try_consume_context::<
            std::sync::Arc<dyn blitz_traits::shell::ShellProvider>,
        >();
        CustomWidgetAttr::new(PageWidget::new(
            document.0.clone(),
            index,
            view,
            chosen.clone(),
            shell,
        ))
    });

    rsx! {
        div {
            class: "page",
            // Which page this is. The mounting window is the single most
            // load-bearing thing in `layout.rs` and it is invisible from
            // outside without this.
            "data-page": "{index + 1}",
            style: "position: absolute; top: {top}px; left: {left}px; width: {width}px; height: {height}px;",
            object {
                "data": widget,
                // A widget laid out at 0×0 is a blank window with nothing to
                // say why, which is what `display: block` costs to avoid.
                style: "display: block; width: {width}px; height: {height}px;",
            }
            for (at, (quad, current)) in hits.iter().enumerate() {
                div {
                    key: "{at}",
                    class: if *current { "hit current" } else { "hit" },
                    style: "position: absolute; top: {quad.top}px; left: {quad.left}px; width: {quad.width}px; height: {quad.height}px;",
                }
            }
            for (at, (area, target)) in links.iter().enumerate() {
                div {
                    key: "l{at}",
                    class: "link",
                    // Deliberately not an `<a href>`, which is the app's own
                    // decision made again for a different reason. There it is
                    // that an anchor carrying the address navigates on a
                    // middle click, which never reaches the click handler, so
                    // the webview left the app and took the document with it.
                    // Here it is that an `href` would go through `nav.rs`,
                    // which is the chrome's door and knows nothing about
                    // pages: an internal link would find no scheme it allows
                    // and do nothing at all.
                    role: "link",
                    // A name, because the element has no text of its own: it
                    // is a bare rectangle over printed words, and there is no
                    // text layer here for it to reach them through. Without
                    // one a page of cross-references reads as "link, link,
                    // link" — the app's own finding, and the fix is cheaper
                    // here because the destination is already resolved.
                    "aria-label": match target {
                        Target::Away(url) => url.clone(),
                        Target::Place { page, .. } => format!("Page {page} of this document"),
                    },
                    style: "position: absolute; top: {area.top}px; left: {area.left}px; width: {area.width}px; height: {area.height}px;",
                    onclick: {
                        let target = target.clone();
                        let away = away.clone();
                        move |_| {
                            if let Some(url) = viewer.write().follow(&target) {
                                away.open(&url);
                            }
                        }
                    },
                }
            }
        }
    }
}

/// One handler per action, and a dispatch of about thirty lines — which is
/// what `main.ts` has, and for the same reason: the table decides *which*
/// action, so nothing here has to know anything about keys.
///
/// **The arms that are missing are the interesting half.** Every action in
/// the app's table is carried across whether or not this reader can do it, so
/// a key bound to something unbuilt says so rather than doing nothing —
/// which turns the keyboard into a live list of what Phase 3 has left. It is
/// also the honest answer to a reader who presses ⌘P: printing is not there
/// yet, and silence would be indistinguishable from a broken keymap.
fn perform(mut viewer: Signal<Viewer>, action: Action, screen: f64) {
    // Every movement goes through the viewer rather than through an offset,
    // because in paged mode an offset is not where a reader ends up: the page
    // has to be turned first. See [`Viewer::nudge`] and [`Viewer::go_to`].
    fn by(mut viewer: Signal<Viewer>, delta: f64) {
        viewer.write().nudge(delta);
    }
    fn page(mut viewer: Signal<Viewer>, page: usize) {
        viewer.write().go_to(crate::layout::Anchor {
            page,
            offset: 0.0,
        });
    }

    match action {
        Action::ScrollDown => by(viewer, LINE),
        Action::ScrollUp => by(viewer, -LINE),
        Action::HalfScreenDown => by(viewer, (screen - OVERLAP) / 2.0),
        Action::HalfScreenUp => by(viewer, -(screen - OVERLAP) / 2.0),
        Action::ScreenDown => by(viewer, screen - OVERLAP),
        Action::ScreenUp => by(viewer, -(screen - OVERLAP)),
        Action::FirstPage => viewer.write().to_start(),
        Action::LastPage => viewer.write().to_end(),
        Action::NextPage => {
            let next = viewer.read().page() + 1;
            page(viewer, next);
        }
        Action::PreviousPage => {
            let previous = viewer.read().page().saturating_sub(1).max(1);
            page(viewer, previous);
        }
        Action::ZoomIn => viewer.write().zoom(true),
        Action::ZoomOut => viewer.write().zoom(false),
        Action::FitWidth => viewer.write().set_fit(Fit::Width),
        Action::FitPage => viewer.write().set_fit(Fit::Page),
        Action::ActualSize => viewer.write().actual_size(),
        Action::RotateRight => viewer.write().rotate(1),
        Action::RotateLeft => viewer.write().rotate(-1),
        Action::NextTheme => viewer.write().next_theme(),
        Action::Sidebar => viewer.write().toggle_sidebar(),
        Action::Mark => {
            let page = viewer.read().page();
            viewer.write().mark_page(page);
        }
        Action::GoToPage => viewer.write().open_page_field(),
        // Silence would be the wrong answer at the end of the history: the
        // reader pressed a key and has no way to tell a shortcut that did
        // nothing from one that is not bound. The app says the same two
        // sentences.
        Action::Back => {
            if !viewer.write().go_back() {
                viewer.write().notice = "Nowhere further back".to_string();
            }
        }
        Action::Forward => {
            if !viewer.write().go_forward() {
                viewer.write().notice = "Nowhere further forward".to_string();
            }
        }
        Action::Find => {
            viewer.write().open_find();
        }
        Action::FindNext => viewer.write().step_match(true),
        Action::FindPrevious => viewer.write().step_match(false),
        // Escape, which in the app leaves full screen and stops presenting as
        // well. There is neither here yet, so it is the find bar's way out and
        // says nothing when there is nothing to close — a key that answers
        // with a complaint about what it did not do is worse than one that
        // does nothing.
        Action::Dismiss => {
            // The page field first, because it is the thing the reader is
            // most recently inside: pressing Escape while typing a number
            // means "not that after all", not "close the find bar I opened a
            // minute ago". Escape typed *into* the field never reaches here —
            // the field stops it — so this is the case where the field is
            // open and the pointer took the focus elsewhere.
            if viewer.read().typing_page {
                viewer.write().cancel_page();
            } else if viewer.read().find_open {
                viewer.write().close_find();
            }
        }
        Action::Spread => {
            let next = if viewer.read().layout.spread == Spread::Single {
                Spread::Cover
            } else {
                Spread::Single
            };
            viewer.write().set_spread(next);
        }
        not_built => {
            let said = format!("{} is not built yet", crate::keymap::label(not_built));
            viewer.write().notice = said;
        }
    }
}

/// One turn of the event loop, awaited.
///
/// `breathe()` in `search.ts` is `setTimeout(resolve, 0)` and this is the same
/// thing said in Rust: wake the task immediately and return `Pending`, so the
/// scheduler puts it back in the queue and whoever is driving the document —
/// the shell in the real app, `pump()` in the harness — gets a turn first.
///
/// It is a macrotask there and a wake here for the same reason: awaiting a
/// promise alone would keep the browser out of the loop, and returning
/// `Ready` alone would keep the window out of it.
struct Breathe(bool);

impl Breathe {
    fn once() -> Breathe {
        Breathe(false)
    }
}

impl std::future::Future for Breathe {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            return std::task::Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}
