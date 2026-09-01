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
use crate::render::{Heading, Link, PageSource, PageText, Rect, Target};
use crate::search::{Options as Find, Search};
use crate::select::{Selection, Spot};
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
    /// Whether to watch the themes directory and the open document for
    /// changes made by somebody else — see [`crate::watch`], which is the
    /// app's own file mounted here.
    ///
    /// **Only the harness reads this now.** The binary provides one
    /// `Arc<Watching>` for the whole process as a context — there is one
    /// themes directory and one document per window, which is the shape
    /// `watch.rs` already has — and a window that is handed one uses it
    /// whatever this says. What is left is the case with no context: off, and
    /// the reason is a thread. `Watching` has no way to stop, so the thread
    /// outlives the handle; one per process is nothing and one per test is a
    /// hundred threads and a hundred file-system watches on a `cargo test`.
    /// So a test posts news into the mailbox itself, which is the
    /// deterministic thing to do anyway, and the one test that wants the real
    /// watcher asks for it.
    pub watch: bool,
    /// What this window is called — `main`, then `reader-1`, and so on.
    ///
    /// It is the name `watch.rs` reports a rewritten document to, and the
    /// name [`crate::emit::Exchange`] routes that report by, which is the
    /// whole of why a window needs one. See [`crate::windows`].
    pub window: String,
}

impl Config {
    /// The reader's own directory. See [`crate::config::config_dir`]: it is
    /// deliberately not the installed app's.
    pub fn here() -> Config {
        Config {
            dir: crate::config::config_dir(),
            theme: None,
            watch: true,
            window: crate::windows::MAIN.to_string(),
        }
    }

    pub fn at(dir: impl Into<std::path::PathBuf>) -> Config {
        Config {
            dir: dir.into(),
            theme: None,
            watch: false,
            window: crate::windows::MAIN.to_string(),
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

/// The toolbar's height, the notice line's, and the hairline between them and
/// the document.
///
/// The app measures these off the elements; here they are stated, because
/// there is no `ResizeObserver` and no `get_client_rect` that can be called
/// safely from an event — see `resize_from_window` below. They are three
/// numbers rather than one because either of the first two can be taken away
/// now: ⌘T puts the toolbar down, and presenting puts everything down. See
/// [`Viewer::chrome`].
pub const TOOLBAR: f64 = 46.0;
const NOTICE: f64 = 30.0;
const HAIRLINE: f64 = 2.0;

/// What the chrome costs with all of it on screen, which is what a window
/// opens with.
pub const CHROME: f64 = TOOLBAR + NOTICE + HAIRLINE;

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

/// How many pages of text are kept for the sake of a selection.
///
/// A page is about a hundred kilobytes of characters and boxes, and a sweep
/// is over the pages on screen — which at any zoom this reader offers is at
/// most a spread and its neighbours. Eight is that with room either side, and
/// it is 800KB against the 23MB two page textures already cost. See
/// [`Viewer::texts`], which is the one cache in this file that is bounded
/// where the links beside it are not.
const TEXT_CACHE: usize = 8;

/// How long after a press a second one in the same place is the same gesture.
///
/// Blitz's own number for the text fields it owns, restated here because a
/// page cannot be told about a double click and has to count one — see
/// [`Viewer::begin_sweep`]. Two windows disagreeing about what a double click
/// is would be worse than either answer.
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(500);

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
            if url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("mailto:")
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

/// Where a copied passage goes.
///
/// A context holding one closure, for the reason [`Away`] is one: the thing it
/// stands for does not exist in a test, and a suite that took the real one
/// would empty the clipboard of whoever is running `cargo test` — which is a
/// worse trespass than opening a browser window, because it takes something
/// away rather than adding something.
///
/// **The app has no equivalent and needs none**, which is the whole of why
/// this is here. There, ⌘C is the webview's own: the browser owns the
/// selection, so it owns copying it, and `main.ts` reaches for the clipboard
/// only for the one thing the browser cannot do for itself — a quote with its
/// page number attached. Here the selection is the reader's own
/// ([`crate::select`]), so copying it is too, and the clipboard is a door in
/// the shell like every other door in this crate.
#[derive(Clone)]
pub struct Clip(Rc<dyn Fn(&str)>);

impl PartialEq for Clip {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Clip {
    pub fn new(put: impl Fn(&str) + 'static) -> Self {
        Clip(Rc::new(put))
    }

    /// The default: the system's clipboard, through the shell provider Blitz
    /// already hands every window. `None` for a shell that provided none,
    /// which says so once rather than silently copying nothing — a reader who
    /// presses ⌘C and pastes what they copied an hour ago has no way to tell
    /// that from a key that is not bound.
    pub fn to_the_system(shell: Option<Arc<dyn blitz_traits::shell::ShellProvider>>) -> Self {
        Clip::new(move |text| match &shell {
            Some(shell) => {
                if shell.set_clipboard_text(text.to_string()).is_err() {
                    eprintln!("the clipboard refused {} characters", text.len());
                }
            }
            None => eprintln!("there is no clipboard to copy into"),
        })
    }

    pub fn put(&self, text: &str) {
        (self.0)(text);
    }
}

/// Which document the reader chose, when they were asked.
///
/// A context holding one closure, for the reason [`Clip`] is one: the thing it
/// stands for is a modal window belonging to the operating system, and a test
/// that opened one would sit there until somebody clicked it. `blitz-shell`
/// already carries the picker — `open_file_dialog` on the shell provider,
/// behind its `file-dialog` feature, which is `rfd` — so this is a door in the
/// shell like the clipboard beside it rather than a dependency of its own.
///
/// `None` means the reader cancelled, which is a different answer from a path
/// that could not be opened and is the one case that says nothing at all.
#[derive(Clone)]
pub struct Pick(Rc<dyn Fn() -> Option<String>>);

impl PartialEq for Pick {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Pick {
    pub fn new(choose: impl Fn() -> Option<String> + 'static) -> Self {
        Pick(Rc::new(choose))
    }

    /// The default: the system's own picker, filtered to PDFs.
    ///
    /// It blocks the thread it is called on, which is the thread drawing the
    /// window — and that is right rather than a compromise: the picker is
    /// modal, so there is nothing behind it for a frame to show. It is also
    /// the only shape `ShellProvider` offers.
    pub fn from_the_system(shell: Option<Arc<dyn blitz_traits::shell::ShellProvider>>) -> Self {
        Pick::new(move || {
            // Said once rather than silently choosing nothing, which is
            // `Clip`'s rule and the same reason: a reader who presses ⌘O and
            // gets no window cannot tell a shell that provided no picker from
            // a picker they cancelled.
            let shell = shell.as_ref().or_else(|| {
                eprintln!("there is no picker to choose a document with");
                None
            })?;
            let filter = blitz_traits::shell::FileDialogFilter {
                name: "PDF".to_string(),
                extensions: vec!["pdf".to_string()],
            };
            shell
                .open_file_dialog(false, Some(filter))
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned())
        })
    }

    pub fn choose(&self) -> Option<String> {
        (self.0)()
    }
}

/// What the *window* can be asked to do, which is not the page's business.
///
/// A context holding one closure, for the reason [`Screen`] and [`Away`] are
/// contexts: the thing it stands for does not exist in a test. A shell answers
/// these against winit; the harness writes them down, which is how "⌘N asks
/// for a window" is a test rather than a thing somebody checked once by hand.
///
/// It is deliberately one closure and an enum rather than five closures. The
/// asks are a small closed set, they are all "tell whoever owns the window",
/// and a test that wants to know what the reader asked for wants the list in
/// order — which is a `Vec<Ask>` and not five counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A window of its own. The document is chosen by whoever answers, which
    /// in this reader means a file picker: there is no start screen here, so
    /// there is no such thing as a window with nothing in it. See
    /// [`crate::windows::Desk::hand_over`].
    NewWindow,
    /// This window, closed. On the last window that ends the app, which is
    /// how most people quit it.
    Close,
    /// All of them, and the app with them.
    Quit,
    /// This document, in a window of its own — ⇧⌘O, and the menu item under
    /// it. Answered by `hand_over`, so a document already open somewhere is
    /// brought forward rather than opened twice.
    NewWindowOn(String),
    /// This window is showing a different document now: where it is, and what
    /// to call it.
    ///
    /// Three things outside the reader have to be told and none of them is
    /// the reader's to reach. The desk, which is what the restore list is
    /// read from, and the watch, which is following the file that was open a
    /// moment ago — both belong to the process. And the window's own title,
    /// which belongs to winit. The name travels with the path because the
    /// reader has already worked it out (`store::called`, which is
    /// `worth_calling` deciding between a document's `/Title` and its file
    /// name) and asking pdfium again would mean opening the file again.
    Showing { path: String, title: String },
    /// Full screen, on or off. Presenting is this and the chrome taken away,
    /// and the second half is the page's own — see [`Viewer::present`].
    FullScreen(bool),
}

#[derive(Clone)]
pub struct Frame(Rc<dyn Fn(Ask)>);

impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Frame {
    pub fn new(ask: impl Fn(Ask) + 'static) -> Self {
        Frame(Rc::new(ask))
    }

    /// What a window with nobody listening does, which is say so once. A
    /// reader who presses ⌘N and gets silence has no way to tell a shortcut
    /// that did nothing from one that is not bound.
    pub fn unanswered() -> Self {
        Frame::new(|ask| eprintln!("nothing is listening for {ask:?}"))
    }

    pub fn ask(&self, ask: Ask) {
        (self.0)(ask);
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

/// The toolbar's three menus.
///
/// **This is the piece the port had been doing without, and the chips were
/// standing in for it.** Fourteen themes reached by pressing `t` fourteen
/// times, three spread modes on `s`, and a fit chip that could only ever mean
/// *fit width* are all the same shape: a list of choices with no room to show
/// itself. `keymap::EXTRA`'s first two entries exist because of it and say so
/// in their own comment — "this reader has no menus yet".
///
/// Three rather than one, because that is where the app's are and what they
/// are about: what document is open, what it looks like, and how it is laid
/// out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    /// Under the title: open, open beside, a window, close.
    Document,
    /// Under the theme's name: every theme installed, the one in use ticked.
    Theme,
    /// Under the zoom: fit, spread, rotation, margins.
    View,
}

impl Menu {
    /// What the button that opens it is called, which is also what the menu
    /// is called to a screen reader.
    pub fn label(self) -> &'static str {
        match self {
            Menu::Document => "Document",
            Menu::Theme => "Theme",
            Menu::View => "View",
        }
    }
}

/// Everything the reader is looking at, and everything that changes it.
pub struct Viewer {
    pub document: Arc<dyn PageSource>,
    pub layout: Layout,
    pub scroll_top: f64,
    /// Where the window sits across the document, as the fraction of the
    /// content its middle is over. Half, always, until somebody pans.
    ///
    /// **A fraction rather than an offset, and that is what makes the page
    /// stay centred.** `#pages` in the app is `margin: 0 auto` inside a
    /// `#viewer` that is `overflow: auto`, so a page narrower than the window
    /// is centred by the box model and a page wider than it scrolls. Here the
    /// pages are placed absolutely against a box `layout.rs` sizes, so both
    /// halves are arithmetic — and a stored *offset* would have to be
    /// recomputed at every one of the dozen places that relay the document
    /// out. A fraction needs recomputing nowhere: [`Viewer::scroll_left`]
    /// resolves it against whatever the content is now, so a page that has
    /// just become wider than the window arrives with its middle in the
    /// middle, which is the answer for a zoom step and the answer for a
    /// window made narrower.
    across: f64,
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
    /// Which toolbar menu is down, if any. `None` almost always.
    ///
    /// **One at a time, and the state is the reader's rather than the
    /// button's**, which is `showPopover` in `ui.ts` and the same reason:
    /// opening a second menu has to close the first, and nothing that lives
    /// inside one menu can know about another. Escape closes it, and so does
    /// a press anywhere the menu is not — see the root's `onmousedown` and
    /// [`Menu`].
    pub menu: Option<Menu>,
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
    /// The text of the pages the pointer has touched, and where every
    /// character of it is.
    ///
    /// Behind a `RefCell` for the reason the links are, and **bounded where
    /// the links are not**, because the two are not the same size at all: a
    /// link is a rectangle and a target, and a page of text is a `char` and a
    /// `Rect` per character — about thirty-six bytes each, so a page of
    /// typeset mathematics is a hundred kilobytes and a four-hundred-page book
    /// read end to end would be forty megabytes. That is a quarter of what
    /// this whole reader costs, held for a feature nobody may have used.
    ///
    /// [`TEXT_CACHE`] pages, oldest out first, which is the same shape the
    /// search's index has and a two-hundredth of the size: a sweep is over the
    /// pages on screen, and the pages on screen are a handful. The search
    /// keeps its own copy while the find bar is up and gives it back when the
    /// bar goes ([`Search::forget`]), and the two do not share one — a
    /// selection outlives the find bar, and a cache that emptied when
    /// somebody closed a search would be a selection that stopped being
    /// copyable for no reason the reader could see.
    texts: RefCell<Vec<(usize, Rc<PageText>)>>,
    /// What the reader has swept over, or nothing.
    ///
    /// One selection for the document rather than one per page: a sweep that
    /// runs off the bottom of a page carries on down the next one, which is
    /// what continuous scrolling is for. See [`crate::select`].
    selection: Option<Selection>,
    /// Where the content's top left is, in the coordinates a mouse event
    /// arrives in, worked out from the press that began the sweep.
    ///
    /// **Not asked of the DOM, because it cannot be asked from here.** A
    /// `MountedData` call borrows the document and every place a component can
    /// call one from is already inside a borrow of it — the same wall
    /// `Screen` is here for. What is available is that the press arrives with
    /// both its client coordinates and its coordinates within the page it
    /// landed on, and the layout knows where that page is: subtract, and the
    /// origin falls out. It is worked out once per sweep and the scroll
    /// offset is added back on every move, so scrolling mid-sweep — which is
    /// what a reader dragging to the bottom of the window does — extends the
    /// selection through the text that scrolls past rather than through the
    /// pixels it happens to be over.
    ///
    /// `None` outside a sweep, which is what root's `onmousemove` checks
    /// before touching the signal at all — see the sidebar's `resize_from`,
    /// which is the same shape for the same reason.
    sweep_from: Option<(f64, f64)>,
    /// When and where the pointer last went down on a page, which is the whole
    /// of what tells a second click from a first one. See
    /// [`Viewer::begin_sweep`].
    pressed: Option<(std::time::Instant, f64, f64)>,
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
    /// and what has been typed when somebody is.
    ///
    /// **It arrives holding the page it is on, and the first thing typed
    /// replaces the lot** — which is what selecting the contents comes to for
    /// anybody who then types, and it is what the app does
    /// (`el.pageNumber.select()`). It used to arrive empty, because
    /// `select()` has no equivalent here: parley will select all when it is
    /// *asked by a keystroke* and there is no imperative door onto it from a
    /// component. So the selection is emulated one level up — `page_fresh`
    /// below is the "everything in here is selected" state, and the keydown
    /// handler in `Reader` is where it is spent. The reason the empty field
    /// was wrong is not that it was harder to use: it is that the number
    /// disappearing is the reader losing the one thing the field was showing
    /// them.
    pub typing_page: bool,
    pub page_typed: String,
    /// Whether what is in the field is the page it opened on rather than
    /// anything the reader has typed — the emulated "all of it is selected".
    /// See [`Viewer::typing_page`].
    pub page_fresh: bool,
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
    /// How tall the window is, which is not the same as how tall the document
    /// area is: the difference is the chrome, and the chrome comes and goes.
    /// See [`Viewer::chrome`] and [`Viewer::fit_window`].
    window_height: f64,
    /// Whether the toolbar is on screen. A setting, so a reader who reads
    /// without one gets none next time.
    pub toolbar: bool,
    /// Full screen, as this reader last asked for it. The window is the thing
    /// that is actually in it, and this is the asking — see [`Frame`]. It is
    /// deliberately not a setting: the app remembers it because the window
    /// state is Tauri's to restore, and a reader who quit in full screen and
    /// launched into it again with no chrome would have nothing to press.
    pub full_screen: bool,
    /// Presenting: full screen with nothing else on it. Full screen plus the
    /// chrome away, held apart from both so that leaving one puts the other
    /// back the way it was.
    pub presenting: bool,
    /// Where the last run left the reader, waiting for a window to put it
    /// back in.
    ///
    /// **A held place rather than a scroll offset, because there is no
    /// viewport yet.** `Viewer::new` runs before anything is mounted, so the
    /// layout has a viewport of 0×0 and every page in it is zero high — a
    /// place turned into an offset there is turned back into page one the
    /// moment the window says how big it is. So it is kept as what it is, a
    /// page and a fraction of it, and [`Viewer::resize`] spends it on the
    /// first layout that has room in it.
    ///
    /// It is also what stops the restore from writing over itself:
    /// [`Viewer::remember_place`] says nothing while this is pending, or the
    /// relayouts on the way to the first frame would record page one over the
    /// place being restored.
    place: Option<Anchor>,
    /// Which draft of the document this is.
    ///
    /// **In the page's key, and it is the only thing that could be.** A page
    /// keeps its texture for as long as its key does not move — the page
    /// number, the size, the theme, the view — and a recompile changes none
    /// of those while changing every pixel. `generation` cannot do it: it is
    /// bumped by opening the sidebar, which must not throw a texture away.
    /// So a document replaced under the reader is a new number here, every
    /// mounted page is a new node, and Blitz releases the old textures
    /// between frames the way it does for a zoom.
    edition: u64,
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
            across: 0.5,
            sidebar_open: false,
            sidebar_width: 252.0,
            resize_from: None,
            tab: Tab::Contents,
            menu: None,
            thumb_scroll: 0.0,
            column: Column::default(),
            headings: document.outline(),
            labels: document.labels(),
            links: RefCell::new(HashMap::new()),
            texts: RefCell::new(Vec::new()),
            selection: None,
            sweep_from: None,
            pressed: None,
            past: Vec::new(),
            future: Vec::new(),
            typing_page: false,
            page_typed: String::new(),
            page_fresh: false,
            search: Search::new(),
            find_open: false,
            find_query: String::new(),
            highlight_all: true,
            scan: 0,
            revealed: false,
            trimming: false,
            window_width: 0.0,
            window_height: 0.0,
            toolbar: true,
            full_screen: false,
            presenting: false,
            place: None,
            edition: 0,
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
            let declared = viewer.document.title();
            viewer.place = viewer.store.opened(&path, &declared);
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
        self.layout.zoom = self
            .store
            .number("zoom")
            .clamp(ZOOMS[0], ZOOMS[ZOOMS.len() - 1]);
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
        self.toolbar = self.store.flag("show_toolbar");
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
        let settled = (self.layout.viewport.width - width).abs() < 0.5
            && (self.layout.viewport.height - height).abs() < 0.5;
        // A window that has not changed size still owes the reader their
        // place, so this is the one thing that gets past the early return.
        if settled && self.place.is_none() {
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
        // And where the last run left off, spent on the first window this
        // reader gets: a place is a page and a fraction of it, and turning
        // that into a scroll offset needs pages that have been laid out. It
        // goes through `go_to` rather than through `scroll_target` because in
        // paged mode arriving at a page is a relayout — see [`Viewer::go_to`].
        if let Some(place) = self.place.take() {
            self.go_to(place);
        }
    }

    /// How much of the window the document has: everything the panel is not
    /// standing on.
    fn document_width(&self) -> f64 {
        // Presenting takes the panel with the rest of the chrome, and it does
        // so here rather than by closing it: a reader who stops presenting
        // gets back the sidebar they had open, which is the whole difference
        // between hiding something and turning it off.
        let panel = if self.sidebar_open && !self.presenting {
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
    ///
    /// **The column of thumbnails is the exception, and it was wrong to
    /// treat it as one of the pages.** A thumbnail is a fifth of a page
    /// across and a twenty-fifth of one in area, so the whole visible column
    /// costs less to draw than a single page of the document does — and it is
    /// the thing directly under the pointer, so a panel whose pictures stay
    /// the size they were while its edge moves is the one place the deferral
    /// is visible as a fault rather than as a saving. `relay_column` is
    /// therefore live and the document's relayout is still deferred: the two
    /// halves of what was one decision, taken separately now.
    pub fn drag_sidebar(&mut self, client_x: f64) {
        let Some((start_x, start_width)) = self.resize_from else {
            return;
        };
        let width = (start_width + (client_x - start_x))
            .clamp(crate::sidebar::MIN_WIDTH, crate::sidebar::MAX_WIDTH);
        if width == self.sidebar_width {
            return;
        }
        self.sidebar_width = width;
        self.relay_column();
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

    /* --------------------------------------------------------- the window */

    /// What the chrome costs on screen right now.
    ///
    /// The notice line stays when the toolbar goes, deliberately: the message
    /// that says how to get the toolbar back is written on it, and a line that
    /// disappeared along with the thing it explains would be a joke at the
    /// reader's expense. Presenting is the case where everything goes, which
    /// is what presenting *is*.
    pub fn chrome(&self) -> f64 {
        if self.presenting {
            return 0.0;
        }
        let toolbar = if self.toolbar { TOOLBAR } else { 0.0 };
        toolbar + NOTICE + HAIRLINE
    }

    /// The window is this big. What the document gets is the rest.
    ///
    /// The one place the chrome is subtracted, which is why it is a method
    /// rather than a constant taken off at the call: the chrome comes and
    /// goes now, and a subtraction at the call site is a subtraction that
    /// only knows what was on screen when it was written.
    pub fn fit_window(&mut self, width: f64, height: f64) {
        self.window_height = height;
        self.resize(width, (height - self.chrome()).max(120.0));
    }

    /// The same window, with a different amount of it left for the document.
    fn refit(&mut self) {
        let (width, height) = (self.window_width, self.window_height);
        self.fit_window(width, height);
    }

    /// The toolbar, put away or brought back.
    ///
    /// The notice is the app's own and is the reason the notice line survives
    /// this: with the toolbar gone there is nothing on screen that says how to
    /// get it back, and the key that does it is whatever `keys.toml` says it
    /// is — so the message reads the keymap rather than stating a chord.
    pub fn toggle_toolbar(&mut self) {
        self.toolbar = !self.toolbar;
        self.store
            .set(vec![("show_toolbar".into(), json!(self.toolbar))]);
        if !self.toolbar {
            let key = self
                .keymap
                .by_action
                .get(&Action::Toolbar)
                .and_then(|chords| chords.first())
                .map(|chord| crate::keymap::shown(chord, crate::keymap::this_machine()));
            self.notice = match key {
                Some(key) => format!("Toolbar hidden, {key} brings it back"),
                // Unbound, which `keys.toml` can do: an empty list unbinds.
                // Then the sentence that names a key would be naming none.
                None => "Toolbar hidden".to_string(),
            };
        }
        self.refit();
        self.generation += 1;
    }

    /// Full screen, as this reader is asking for it. The window is what is
    /// actually in it — see [`Frame`] — and this is the half that is the
    /// page's: nothing changes shape, because full screen is a bigger window
    /// and a bigger window is a resize like any other.
    pub fn set_full_screen(&mut self, on: bool) {
        self.full_screen = on;
    }

    /// Presenting: full screen with nothing else on it.
    ///
    /// Answers what full screen should now be, which is the interesting part.
    /// Presenting turns it on; *stopping* presenting puts it back to whatever
    /// the reader had asked for themselves rather than turning it off — so
    /// somebody who was reading in full screen, presented, and then stopped is
    /// still in full screen, which is where they were.
    pub fn present(&mut self, on: bool) -> bool {
        self.presenting = on;
        self.notice = if on {
            "Presenting. Escape stops.".to_string()
        } else {
            String::new()
        };
        self.refit();
        self.generation += 1;
        on || self.full_screen
    }

    /// How to ask for this action from the keyboard, as it should be read —
    /// ⌘O on a Mac, Ctrl+O elsewhere — or nothing where it is not bound.
    ///
    /// **Read off the keymap rather than written beside the menu item**, for
    /// the reason the app's Keyboard page is drawn from the keymap: a menu
    /// that states its own shortcut cannot show a rebound one, and the
    /// hand-written table this replaces in the app had already drifted. A
    /// reader who unbinds ⌘O in `keys.toml` sees a menu item with no chord on
    /// it, which is true.
    pub fn chord_for(&self, action: Action) -> String {
        self.keymap
            .by_action
            .get(&action)
            .and_then(|bindings| bindings.first())
            .map(|binding| crate::keymap::describe_binding(binding, self.keymap.mac()))
            .unwrap_or_default()
    }

    /// Put a menu down, or take the one that is down away. Asking for the
    /// menu that is already open closes it, which is what clicking its own
    /// button means.
    pub fn show_menu(&mut self, menu: Menu) {
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
    }

    /// Whatever is down, put away. Answers whether there was anything, so
    /// that Escape can fall through to the next thing when there was not.
    pub fn close_menu(&mut self) -> bool {
        self.menu.take().is_some()
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
        if let Some(to) = self
            .column
            .reveal(self.page() - 1, self.thumb_scroll, height)
        {
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
                // A page turn where both sides sit at the top of their page
                // does not move `scroll_top` at all, so the write cannot be
                // left to `scroll_to` — and the page is the whole of what
                // there is to remember in this mode.
                self.remember_place();
            }
        }
        let target = self.layout.scroll_target(anchor);
        self.scroll_to(target);
    }

    /// The start and the end of the document, which are not the same thing as
    /// the top and the bottom of what is laid out.
    pub fn to_start(&mut self) {
        match self.layout.mode {
            Mode::Paged => self.go_to(Anchor {
                page: 1,
                offset: 0.0,
            }),
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
            self.layout
                .row_of(page - 1)
                .last()
                .copied()
                .unwrap_or(page - 1)
                + 2
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

    /* --------------------------------------------------------- selecting */

    /// A page's text, asked for once and kept — see [`Viewer::texts`].
    /// One-based, like everything that says "page" in this file.
    pub fn text_on(&self, page: usize) -> Rc<PageText> {
        if let Some(known) = self
            .texts
            .borrow()
            .iter()
            .find(|(at, _)| *at == page)
            .map(|(_, text)| text.clone())
        {
            return known;
        }
        let Some(index) = page.checked_sub(1) else {
            return Rc::new(PageText::default());
        };
        let text = Rc::new(self.document.text_of(index));
        let mut held = self.texts.borrow_mut();
        held.push((page, text.clone()));
        if held.len() > TEXT_CACHE {
            held.remove(0);
        }
        crate::stats::set(&crate::stats::TEXT_PAGES, held.len() as u64);
        text
    }

    /// The pointer went down on a page: where the sweep starts, and where the
    /// content is in the coordinates the pointer arrives in.
    ///
    /// `on` is the point within the page's own box, in CSS pixels from its top
    /// left, which is what the event carries; `client` is the same press said
    /// in the window's coordinates. The two together are the origin — see
    /// [`Viewer::sweep_from`].
    ///
    /// **A second press in the same place is a double click and takes the word
    /// under it**, which is counted here rather than heard about: `dblclick`
    /// is a default action of `pointerup`, and a default action never runs
    /// over a custom widget — see the comment on `onmousedown` in `Page`. The
    /// rule is Blitz's own, so that a page and a text field in the same window
    /// answer a double click the same way.
    pub fn begin_sweep(&mut self, page: usize, on: (f64, f64), client: (f64, f64)) {
        let Some(index) = page.checked_sub(1) else {
            return;
        };
        let Some(area) = self.layout.box_of(index) else {
            return;
        };
        self.sweep_from = Some((
            client.0 - on.0 - area.left,
            client.1 - on.1 - area.top + self.scroll_top,
        ));
        let again = self.pressed.is_some_and(|(when, x, y)| {
            when.elapsed() < DOUBLE_CLICK
                && (x - client.0).abs() <= 2.0
                && (y - client.1).abs() <= 2.0
        });
        self.pressed = Some((std::time::Instant::now(), client.0, client.1));
        if again {
            self.sweep_word(page, on);
            return;
        }
        let spot = self.spot_on(index, on.0, on.1);
        self.selection = Some(Selection::at(spot));
    }

    /// The pointer moved with the button down. A no-op outside a sweep, which
    /// is what lets this sit on the root and fire on every move in the window.
    pub fn sweep_to(&mut self, client: (f64, f64)) {
        let Some((left, top)) = self.sweep_from else {
            return;
        };
        let Some(mut sweep) = self.selection else {
            return;
        };
        let x = client.0 - left;
        let y = client.1 - top + self.scroll_top;
        let Some((index, on_x, on_y)) = self.layout.page_at_point(x, y) else {
            return;
        };
        let head = self.spot_on(index, on_x, on_y);
        if head == sweep.head {
            return;
        }
        sweep.head = head;
        self.selection = Some(sweep);
    }

    /// The pointer let go. A sweep that covered nothing is a click, and a
    /// click puts the selection down rather than leaving a caret nobody can
    /// see blinking in a document nobody can type into.
    pub fn end_sweep(&mut self) {
        self.sweep_from = None;
        if self.selection.is_some_and(|sweep| sweep.is_empty()) {
            self.selection = None;
        }
    }

    /// True while the pointer is down on a page.
    pub fn sweeping(&self) -> bool {
        self.sweep_from.is_some()
    }

    /// The word under a point, which is what a second click on it means.
    ///
    /// The anchor is left at the *start* of the word and the head at its end,
    /// so a reader who goes on dragging extends from the word rather than from
    /// wherever inside it they happened to press.
    pub fn sweep_word(&mut self, page: usize, on: (f64, f64)) {
        let Some(index) = page.checked_sub(1) else {
            return;
        };
        let (x, y) = self.layout.unplace_on(index, on.0, on.1);
        let text = self.text_on(index + 1);
        let (from, to) = crate::select::words_around(&text, crate::select::caret_at(&text, x, y));
        if from == to {
            return;
        }
        self.selection = Some(Selection {
            anchor: Spot {
                page: index + 1,
                index: from,
            },
            head: Spot {
                page: index + 1,
                index: to,
            },
        });
    }

    /// Where a caret goes for a point in a page's box.
    fn spot_on(&self, index: usize, on_x: f64, on_y: f64) -> Spot {
        let (x, y) = self.layout.unplace_on(index, on_x, on_y);
        let text = self.text_on(index + 1);
        Spot {
            page: index + 1,
            index: crate::select::caret_at(&text, x, y),
        }
    }

    /// Everything on the page the reader is on, which is ⌘A.
    ///
    /// The *page* rather than the document, which is the app's own label for
    /// this key — "Select the text of this page" — and its own reasoning: a
    /// reader who means the whole document means a file, and what this gesture
    /// is actually for is taking a page of a paper into something else.
    pub fn select_page(&mut self) -> bool {
        let page = self.page();
        let text = self.text_on(page);
        if text.is_empty() {
            self.notice = "There is no text on this page to select.".into();
            return false;
        }
        self.selection = Some(Selection {
            anchor: Spot { page, index: 0 },
            head: Spot {
                page,
                index: text.chars.len(),
            },
        });
        true
    }

    /// Put the selection down. `false` when there was none, so that whatever
    /// asked can go on to the next thing Escape means.
    pub fn clear_selection(&mut self) -> bool {
        self.sweep_from = None;
        self.selection.take().is_some()
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|sweep| !sweep.is_empty())
    }

    /// What is selected on one mounted page, as rectangles in CSS pixels from
    /// the top left of its box — the space [`Viewer::highlights`] and
    /// [`Viewer::link_areas`] answer in.
    pub fn selected_areas(&self, page: usize) -> Vec<Rect> {
        let Some(sweep) = self.selection else {
            return Vec::new();
        };
        let Some(index) = page.checked_sub(1) else {
            return Vec::new();
        };
        if self.layout.box_of(index).is_none() {
            return Vec::new();
        }
        if !sweep.pages().contains(&page) {
            return Vec::new();
        }
        let text = self.text_on(page);
        let Some((from, to)) = sweep.range_on(page, text.chars.len()) else {
            return Vec::new();
        };
        text.quads(from, to)
            .into_iter()
            .map(|quad| self.layout.place_on(index, quad))
            .collect()
    }

    /// The selected words, in reading order, over as many pages as the sweep
    /// covers.
    pub fn selected_text(&self) -> String {
        let Some(sweep) = self.selection else {
            return String::new();
        };
        let mut out = String::new();
        for page in sweep.pages() {
            let text = self.text_on(page);
            let Some((from, to)) = sweep.range_on(page, text.chars.len()) else {
                continue;
            };
            let part = crate::select::quote(&text, from, to);
            if part.is_empty() {
                continue;
            }
            if !out.is_empty() {
                // A page break is a paragraph break, which is what a reader
                // pasting two pages of a paper into their notes means by it.
                out.push('\n');
            }
            out.push_str(&part);
        }
        out
    }

    /// The selected words with where they came from, which is ⌘⇧C.
    ///
    /// The app's own format and the app's own reason: copying a sentence out
    /// of a paper and then going back to find the page it was on is the small,
    /// constant tax of reading for work. The page is the one the selection
    /// *began* on rather than the one in the toolbar, because a selection that
    /// runs across a page boundary began on the page it began on.
    ///
    /// Returns the text to copy and the words for the notice line, or nothing
    /// when there is no selection.
    pub fn quoted(&self) -> Option<(String, String)> {
        let sweep = self.selection?;
        let quoted = self.selected_text();
        if quoted.is_empty() {
            return None;
        }
        let name = self.store.title().to_string();
        let where_from = format!(
            "{}p. {}",
            if name.is_empty() {
                String::new()
            } else {
                format!("{name}, ")
            },
            self.label(sweep.span().0.page)
        );
        Some((format!("“{quoted}” — {where_from}"), where_from))
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

    /// Put the reader in the page field, holding the page they are on with
    /// all of it selected. `focusPageNumber` in `main.ts`, which is
    /// `focus()` and then `select()`; see [`Viewer::typing_page`] for what
    /// stands in for the second half.
    pub fn open_page_field(&mut self) {
        self.typing_page = true;
        self.page_typed = self.label(self.page());
        self.page_fresh = true;
    }

    /// The reader typed into the field: whatever is in it now.
    pub fn type_page(&mut self, text: &str) {
        self.typing_page = true;
        self.page_typed = text.to_string();
        self.page_fresh = false;
    }


    /// Go where the field says, or say that it says nowhere.
    ///
    /// Either way the field stops being typed in and goes back to naming the
    /// page the reader is on, which is what the app does by putting the
    /// current label back into it.
    pub fn commit_page(&mut self) {
        let typed = std::mem::take(&mut self.page_typed);
        self.typing_page = false;
        // Nothing was typed: the field is holding the page it opened on, and
        // Enter on that means "never mind" rather than "go where I already
        // am" — which would otherwise put an entry in the history for a jump
        // that went nowhere.
        let untouched = std::mem::take(&mut self.page_fresh);
        if untouched || typed.trim().is_empty() {
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
        self.page_fresh = false;
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
        self.store.set(vec![(
            "search_highlight_all".into(),
            json!(self.highlight_all),
        )]);
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
        self.store
            .set(vec![("fit_mode".into(), json!(name_of(fit)))]);
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

    /* ------------------------------------- what changed on the disk */

    /// A theme file was written — by hand, by an LLM, or by this app.
    ///
    /// `themesChanged` in `main.ts`, and the whole of it: the set is replaced
    /// and whatever is in use is put back on from the new files, so that
    /// editing a theme in an editor beside the reader shows up in the reader.
    /// Nothing is written down — nobody chose a theme here, and an editor
    /// saving every few seconds must not be a rewrite of `settings.toml`
    /// every few seconds.
    ///
    /// A theme whose file has *gone* takes the reader somewhere else rather
    /// than leaving the colours of something that no longer exists on screen,
    /// and that one *is* remembered, because it is a choice being made on the
    /// reader's behalf and the next run has to know what it was.
    ///
    /// The app has a third case here and this crate does not have it yet: a
    /// theme being composed in the editor window is the live theme and has no
    /// id, so every save in the themes directory reads as "the theme you are
    /// reading in has been deleted". `isEditingTheme()` is the guard, the
    /// settings window is item 1's other half, and this is a line to come
    /// back to when it lands.
    pub fn themes_changed(&mut self, themes: Vec<crate::theme::Theme>) {
        let before = self.store.theme().clone();
        self.store.set_themes(themes);
        let still_there = self
            .store
            .themes()
            .iter()
            .any(|theme| theme.id == before.id);
        if still_there {
            // Unconditional, as the app's is: a theme that came back
            // unchanged costs one comparison in the widget and nothing else.
            self.chosen.set(self.store.palette());
            self.notice = self.store.complaint.clone().unwrap_or_default();
        } else if let Some(index) = self.store.replacement_for(&before) {
            let name = self.store.wear(index);
            self.chosen.set(self.store.palette());
            self.notice = format!("{} is gone. Now reading in {name}.", before.name);
        }
        self.generation += 1;
    }

    /// The open document was rewritten underneath the reader.
    ///
    /// A paper being recompiled by LaTeX is the case this exists for, and the
    /// reader is meant to stay exactly where they were reading. Where that is
    /// comes off the layout rather than out of the library, for the reason
    /// `main.ts` gives: the library has the last position *written down*, and
    /// this is the one moment the two can differ by a whole scroll.
    ///
    /// What has to go is everything read out of the old file — the outline,
    /// the labels, the links, the search index, the crop — and everything
    /// that points into it, which is the history. What stays is everything
    /// that is the reader's: the fit, the zoom, the spread, the rotation, the
    /// panel, the theme.
    ///
    /// Answers the token of the scan it restarted, when the find bar was up:
    /// the matches were positions in a document that no longer exists, and
    /// looking again is what somebody with the bar open is asking for. `None`
    /// when there is nothing to scan, which is [`Viewer::find`]'s convention
    /// and the same reason for it — a task is the caller's to spawn.
    pub fn document_changed(&mut self, path: &str) -> Option<u64> {
        if path != self.document.path() {
            return None;
        }
        let at = self.layout.anchor(self.scroll_top);
        let reopened = match crate::render::open(path) {
            Ok(document) => document,
            // A compiler that is still writing is what `whole()` in `watch.rs`
            // is there to rule out, so this is the genuinely broken file —
            // and the document already open is the better thing to be looking
            // at than an empty window.
            Err(refused) => {
                self.notice =
                    format!("The document changed on disk and could not be read: {refused}");
                return None;
            }
        };
        self.document = reopened;
        self.headings = self.document.outline();
        self.labels = self.document.labels();
        self.links.borrow_mut().clear();
        // And the text with them, along with whatever was selected: both are
        // indices into a document that no longer exists, and a selection kept
        // across a recompile is a highlight over words nobody chose. The
        // markup journal is where a passage *does* survive a rebuild, and it
        // survives as a quote to be looked up again rather than as a range —
        // see `findQuote` in the app, and item 11.
        self.texts.borrow_mut().clear();
        self.selection = None;
        self.sweep_from = None;
        self.past.clear();
        self.future.clear();
        let sizes = (0..self.document.pages())
            .map(|index| self.document.size_of(index))
            .collect();
        self.layout.replace_sizes(sizes);
        if self.trimming {
            // Measured again rather than kept: the margins are a fact about
            // the file, and this is a different file.
            self.measure_crop();
        }
        self.edition += 1;
        self.generation += 1;
        // Clamped by `go_to`, so a draft that lost its last chapter lands on
        // the end of what is left rather than nowhere.
        self.go_to(at);
        let restarted = if self.find_open {
            let query = self.find_query.clone();
            self.search.forget();
            self.find(&query)
        } else {
            None
        };
        let renamed = self.store.renamed(&self.document.title());
        self.notice = if renamed {
            format!(
                "Reloaded — the document changed on disk. Now called {}.",
                self.store.title()
            )
        } else {
            "Reloaded — the document changed on disk.".into()
        };
        restarted
    }

    /// A different document, in this window. ⌘O, and the menu item under it.
    ///
    /// **The app's ⌘O replaces the document in the window it was pressed in**
    /// — `openDialog` calls `this.open(path)` — and ⇧⌘O is the one that asks
    /// for a window. That is the split kept here, and it is worth saying why
    /// it survives the thing that changed everything else about windows in
    /// this port: there is no start screen, so there is no empty window, so
    /// [`crate::session::Session::another`] gives ⌘N a second window on the
    /// document already in front. None of that bears on ⌘O, which was never
    /// about empty windows — it is the reader saying *this one instead*.
    ///
    /// What has to go is everything read out of the old file and everything
    /// pointing into it, which is [`Viewer::document_changed`]'s list, and
    /// **what has to go with it here is the library entry**: a recompile is
    /// the same document and this is a different one, so the marks, the
    /// title and the remembered place all move. What stays is the reader's
    /// own — the fit, the zoom, the spread, the rotation, the panel, the
    /// theme — because those are settings and a setting is not a property of
    /// a document.
    ///
    /// Answers whether it opened, because the caller has bookkeeping of its
    /// own to do — see [`Ask::Showing`] — and a document that would not open
    /// leaves this window showing the one it had.
    ///
    /// The find bar is closed rather than searched again, which is where this
    /// parts company with `document_changed`: a query asked of a paper is not
    /// a query asked of the next book, and the matches were positions in a
    /// document nobody is looking at any more.
    pub fn open_here(&mut self, path: &str) -> bool {
        if path == self.document.path() {
            self.notice = "That document is already open here.".into();
            return false;
        }
        let opened = match crate::render::open(path) {
            Ok(document) => document,
            Err(refused) => {
                self.notice = format!("Could not open that document: {refused}");
                return false;
            }
        };
        // Where the reader was in the document being put down, written before
        // the store stops pointing at it. `remember` hands it to the scribe,
        // which keeps one place per document — so this cannot be skipped on
        // the grounds that the scroll has not moved since the last one.
        self.store.remember(self.layout.anchor(self.scroll_top));
        self.document = opened;
        let declared = self.document.title();
        let place = self.store.opened(path, &declared);
        self.headings = self.document.outline();
        self.labels = self.document.labels();
        self.links.borrow_mut().clear();
        self.texts.borrow_mut().clear();
        self.selection = None;
        self.sweep_from = None;
        self.past.clear();
        self.future.clear();
        self.search.forget();
        self.close_find();
        let sizes = (0..self.document.pages())
            .map(|index| self.document.size_of(index))
            .collect();
        self.layout.replace_sizes(sizes);
        if self.trimming {
            self.measure_crop();
        }
        // A document with no contents opens on its pages, which is what
        // `restore` does at startup and what `setDocument` does in the app:
        // the difference between a panel and an empty box.
        self.tab = if self.headings.is_empty() {
            Tab::Pages
        } else {
            Tab::Contents
        };
        self.edition += 1;
        self.generation += 1;
        self.scroll_top = 0.0;
        self.go_to(place.unwrap_or(crate::layout::Anchor { page: 1, offset: 0.0 }));
        self.relay_column();
        self.revealed = false;
        self.reveal_thumb();
        self.notice = String::new();
        true
    }

    /// The next theme in the list, which is what `t` is bound to.
    ///
    /// Fourteen themes is too many to cycle through and this is not the app's
    /// gesture — the Theme menu is, and it is built now (see [`Menu`]) — but
    /// it is the one keystroke that proves the whole list is loaded and
    /// wearable, which is what `the_whole_shipped_theme_set_is_wearable`
    /// presses.
    pub fn next_theme(&mut self) {
        let next = (self.store.theme_index() + 1) % self.store.themes().len().max(1);
        self.set_theme(next);
    }

    /// How far the document is scrolled across, in CSS pixels — nothing at
    /// all unless it is wider than the window. See [`Viewer::across`].
    pub fn scroll_left(&self) -> f64 {
        let room = self.layout.max_scroll_x();
        if room <= 0.0 {
            return 0.0;
        }
        (self.across * self.layout.content_width() - self.layout.viewport.width / 2.0)
            .clamp(0.0, room)
    }

    /// Move the document across under the window, which is what a trackpad's
    /// other axis and ⇧-wheel ask for. A no-op when there is nothing to move,
    /// and it puts the middle back in the middle when there stops being: a
    /// reader who zooms out to a page that fits should not find it off to one
    /// side because of where they had panned to.
    pub fn pan(&mut self, delta: f64) {
        let room = self.layout.max_scroll_x();
        if room <= 0.0 {
            self.across = 0.5;
            return;
        }
        if delta == 0.0 {
            return;
        }
        let to = (self.scroll_left() + delta).clamp(0.0, room);
        self.across = (to + self.layout.viewport.width / 2.0) / self.layout.content_width();
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
        self.remember_place();
        true
    }

    /// Note where the reader is, for the next run.
    ///
    /// Called from the two places the reader actually moves — this one and a
    /// page turned in paged mode, which does not go past [`Viewer::scroll_to`]
    /// when both sides of the turn sit at the top of their page. What it
    /// costs here is a channel send: the disk is the scribe's, and it writes
    /// once the scrolling has stopped. See [`Store::remember`].
    fn remember_place(&self) {
        // Not while the last run's place is still waiting for a window to be
        // put back in: the relayouts on the way to the first frame would
        // record page one over the place being restored.
        if self.place.is_some() {
            return;
        }
        self.store.remember(self.layout.anchor(self.scroll_top));
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
    /// What the reader has swept over, on this page, in the same space as the
    /// other two. See [`crate::select`].
    selected: Vec<Rect>,
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
    let mut viewer = use_signal(|| {
        let mut store = Store::at(&config.dir);
        if let Some(index) = config.theme {
            store.wear_for_now(index);
        }
        let mut viewer = Viewer::new(document.0.clone(), chosen.clone(), store);
        // **Sized before the first frame, not on mount.** It was laid out at
        // the default viewport and corrected by `onmounted`, which meant every
        // page in the window was drawn at one size, re-keyed, and drawn again
        // — a full round of pdfium renders and texture uploads thrown away on
        // every launch, and a frame in which *every* node in the document is
        // replaced at once. That frame is the one this file's `PageWidget`
        // comment is about: a texture registered while something else is being
        // unregistered is a texture Vello cannot find at submit, and the
        // `fresh` flag only moves the collision a frame along. Asking the
        // window how big it is before laying anything out costs nothing and
        // takes the collision away rather than dodging it.
        let (width, height, _scale) = screen.get();
        viewer.fit_window(width, height);
        viewer
    });
    // Where a link out of the document goes. See [`Away`]: the default is the
    // system browser, and a harness provides its own.
    let away =
        use_hook(|| dioxus_core::try_consume_context::<Away>().unwrap_or_else(Away::to_the_system));
    // And what the window itself can be asked to do. See [`Frame`]: the shell
    // answers these against winit, and a harness writes them down.
    let frame =
        use_hook(|| dioxus_core::try_consume_context::<Frame>().unwrap_or_else(Frame::unanswered));
    // And where a copied passage goes. See [`Clip`]: the default is the
    // system's clipboard through the shell provider Blitz hands every window,
    // and a harness provides its own so that `cargo test` does not empty
    // anybody's.
    let clip = use_hook(|| {
        dioxus_core::try_consume_context::<Clip>().unwrap_or_else(|| {
            Clip::to_the_system(dioxus_core::try_consume_context::<
                Arc<dyn blitz_traits::shell::ShellProvider>,
            >())
        })
    });
    // And which document the reader chooses when they are asked for one. See
    // [`Pick`]: the default is the system's picker through the same shell
    // provider, and a harness answers with a path of its own.
    let pick = use_hook(|| {
        dioxus_core::try_consume_context::<Pick>().unwrap_or_else(|| {
            Pick::from_the_system(dioxus_core::try_consume_context::<
                Arc<dyn blitz_traits::shell::ShellProvider>,
            >())
        })
    });

    let resize_from_window = {
        let screen = screen.clone();
        move |mut viewer: Signal<Viewer>| {
            let (width, height, _scale) = screen.get();
            viewer.write().fit_window(width, height);
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
    let on_key = {
        let frame = frame.clone();
        let clip = clip.clone();
        let pick = pick.clone();
        move |event: KeyboardEvent| {
            let (press, screen) = {
                let held = viewer.read();
                (
                    held.keymap
                        .press(&event.key(), event.code(), event.modifiers(), &held.pending),
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
                    perform(viewer, action, screen, &frame, &clip, &pick);
                }
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

    // Two files the reader is looking at are not the reader's to change: the
    // themes, and the document. Both are watched on the Rust side — by the
    // app's own `watch.rs`, mounted here — and both arrive as news in a
    // mailbox. See `crate::tauri`, which is how a file that says `use
    // tauri::…` compiles in a crate that has no Tauri in it.
    //
    // **The task is the whole of the wiring on this side.** It waits on the
    // mailbox, which is a real wait: the watcher thread wakes it, the wake
    // marks the task ready, and dioxus's own waker takes it from there to the
    // window. Nothing polls, and in the harness the same wake makes the next
    // `pump()` run it, which is what lets this be tested with no thread and
    // no window at all.
    let watching = use_hook(|| {
        // This window's mailbox, joined to the process's switchboard under
        // this window's name. Both come from whoever made the window; a
        // harness provides them and a window with neither watches nothing.
        let post = dioxus_core::try_consume_context::<crate::emit::Post>().unwrap_or_default();
        let exchange = dioxus_core::try_consume_context::<crate::emit::Exchange>();
        if let Some(exchange) = exchange.as_ref() {
            exchange.join(&config.window, post.clone());
        }
        // **One watcher for the process, not one per window.** It follows one
        // themes directory and a document per window, which is exactly the
        // shape `watch.rs` already has — `follow` counts what wants a
        // directory rather than unwatching it along with the document that
        // named it, because two papers being recompiled in the same folder is
        // the ordinary case. A watcher per window would be that many watches
        // on the same directory and that many copies of every theme reload.
        let shared = dioxus_core::try_consume_context::<Arc<crate::watch::Watching>>();
        let held = match (shared, config.watch) {
            (Some(watching), _) => {
                let path = viewer.read().document.path().to_string();
                if !path.is_empty() {
                    watching.document(&config.window, Some(&path));
                }
                Some(watching)
            }
            // Nobody made one, and this window wants one: the harness's own
            // case, and it is a watcher of one window's own.
            (None, true) => {
                let held = viewer.read();
                let (themes, path) = (
                    held.store.themes_dir().to_path_buf(),
                    held.document.path().to_string(),
                );
                drop(held);
                let exchange = exchange.clone().unwrap_or_else(|| {
                    let exchange = crate::emit::Exchange::new();
                    exchange.join(&config.window, post.clone());
                    exchange
                });
                let watching = Arc::new(crate::watch::start(
                    crate::emit::AppHandle::new(exchange),
                    themes,
                ));
                if !path.is_empty() {
                    watching.document(&config.window, Some(&path));
                }
                Some(watching)
            }
            (None, false) => None,
        };
        let listening = post.clone();
        let sizing = screen.clone();
        spawn(async move {
            loop {
                let news = listening.next().await;
                match news.event.as_str() {
                    "themes-changed" => {
                        if let Ok(themes) = serde_json::from_value(news.payload) {
                            viewer.write().themes_changed(themes);
                        }
                    }
                    "document-changed" => {
                        let path = news.payload.as_str().unwrap_or_default().to_string();
                        let restarted = viewer.write().document_changed(&path);
                        scan(restarted);
                    }
                    // The window changed size, which nothing else in this
                    // process will tell the layout — Blitz resizes its own
                    // viewport and asks for a redraw, and a redraw of a
                    // layout computed for the old window is the old layout.
                    // See `Shell::on_resized`, which is the other half.
                    "window-resized" => {
                        let (width, height, _scale) = sizing.get();
                        viewer.write().fit_window(width, height);
                    }
                    // Nothing else is emitted, and an unknown event is a
                    // version of this crate that has not caught up rather
                    // than something to report.
                    _ => {}
                }
            }
        });
        // Held for the life of the reader. Dropping it stops nothing — see
        // `Config::watch` — but this is where it will be asked to.
        held
    });
    // Read so that the handle is plainly alive rather than plainly unused.
    let _ = watching.is_some();

    let held = viewer.read();
    let scroll_top = held.scroll_top;
    // How far across the document sits, which is nothing unless it is wider
    // than the window. See [`Viewer::across`].
    let scroll_left = held.scroll_left();
    let wearing = held.palette();
    let theme_name = held.theme_name();
    let title = held.store.title().to_string();
    // Which draft of the document is being drawn — in every page's key, so
    // that a recompile replaces the nodes and the textures with them. See
    // `Viewer::edition`.
    let edition = held.edition;
    let mounted = held.layout.mounted(held.scroll_top);
    let content_width = held.layout.content_width();
    let content_height = held.layout.content_height();
    let pages = held.pages();
    let notice = held.notice.clone();
    let sidebar_open = held.sidebar_open;
    let find_open = held.find_open;
    let presenting = held.presenting;
    let toolbar_on = held.toolbar && !held.presenting;
    let find_query = held.find_query.clone();
    let find_count = held.find_count();
    let find_options = held.search.options();
    let highlight_all = held.highlight_all;
    let marked = held.store.is_marked(held.page());
    // What the menus need, read once with the rest of it. The theme list is
    // names and nothing else — a swatch would have to go through `parseColor`
    // to be honest about what the renderer can read, which is the app's own
    // rule and a thing to build when the theme editor is.
    let menu = held.menu;
    let themes: Vec<String> = held.store.themes().iter().map(|t| t.name.clone()).collect();
    let theme_index = held.store.theme_index();
    let fit = held.layout.fit;
    let spread = held.layout.spread;
    let rotation = held.layout.view().rotation;
    let key_open = held.chord_for(Action::Open);
    let key_new_window = held.chord_for(Action::NewWindow);
    let key_close = held.chord_for(Action::CloseWindow);
    let key_fit_width = held.chord_for(Action::FitWidth);
    let key_fit_page = held.chord_for(Action::FitPage);
    let key_actual = held.chord_for(Action::ActualSize);
    let key_rotate_right = held.chord_for(Action::RotateRight);
    let key_rotate_left = held.chord_for(Action::RotateLeft);
    let page_field = held.page_field();
    // How wide the page box is: the padding, the border, and the number in it,
    // with a floor so that page 1 of a pamphlet is not a slot. See the comment on
    // `.pill` below — Blitz cannot centre an input's text, so the box is made
    // to fit rather than the text made to sit in the middle of it.
    let page_box = (14.0 + 8.5 * page_field.chars().count() as f64).max(28.0);
    let typing_page = held.typing_page;
    // Whether the field is still showing all of its contents as selected. See
    // `.page-field.fresh` in `styles.rs`, which is what makes that visible.
    let page_fresh = held.page_fresh;
    // How wide the page box is: the padding, the border, and the number in it,
    // with a floor so that page 1 of a pamphlet is not a slot. See the comment on
    // `.pill` below — Blitz cannot centre an input's text, so the box is made
    // to fit rather than the text made to sit in the middle of it.
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
                selected: held.selected_areas(index + 1),
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
            // Presenting is a class rather than a pile of conditions: the
            // chrome is gone from the DOM either way, and what is left for
            // CSS is the ground the document sits on.
            class: if presenting { "root presenting" } else { "root" },
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
            // A press anywhere a menu is not puts the menu away, which is
            // `showPopover`'s rule in `ui.ts`. Three things stop propagation
            // rather than being tested for here: the menu itself, and each of
            // the three buttons a menu hangs off — a press that closed the
            // menu on its way down would leave the button's own click to open
            // it straight back up, and clicking the open menu's own button
            // would be the one gesture that did nothing.
            // And a press anywhere the page field is not puts *that* away,
            // which is the field's `blur` handler in `main.ts` — it abandons
            // what was typed and puts the current page's label back. The
            // field stops propagation itself, for the same reason the menu
            // buttons do: a press that closed it on the way down would leave
            // nothing to click.
            onmousedown: move |_| {
                let (menu, typing) = {
                    let held = viewer.read();
                    (held.menu.is_some(), held.typing_page)
                };
                if menu {
                    viewer.write().close_menu();
                }
                if typing {
                    viewer.write().cancel_page();
                }
            },
            onmousemove: move |event| {
                let (resizing, sweeping) = {
                    let held = viewer.read();
                    (held.resize_from.is_some(), held.sweeping())
                };
                if resizing {
                    let x = event.client_coordinates().x;
                    viewer.write().drag_sidebar(x);
                } else if sweeping {
                    // The pointer has left the page it went down on, most of
                    // the time: a sweep of two lines crosses into the gap
                    // between pages and a sweep down the margin is never over
                    // a page at all. So this works in the *content's* own
                    // coordinates rather than in any element's, which is what
                    // the origin recorded at the press is for.
                    let at = event.client_coordinates();
                    viewer.write().sweep_to((at.x, at.y));
                }
            },
            onmouseup: move |_| {
                let (resizing, sweeping) = {
                    let held = viewer.read();
                    (held.resize_from.is_some(), held.sweeping())
                };
                if resizing {
                    viewer.write().finish_resize_sidebar();
                }
                if sweeping {
                    viewer.write().end_sweep();
                }
            },
            if toolbar_on {
            div { class: "toolbar",
                button {
                    // Not `.sidebar`, which is the panel itself: a selector
                    // that matches the button *and* the thing the button
                    // opens is a test that cannot tell them apart.
                    class: if sidebar_open { "chip contents on" } else { "chip contents" },
                    onclick: move |_| viewer.write().toggle_sidebar(),
                    "Contents"
                }
                // What the document is called — its own `/Title` where that
                // is worth having, and the file's name where it is not, see
                // `store::worth_calling` — and the button the document's own
                // menu hangs off, which is where the app puts it too.
                // **Every menu hangs inside an anchor of its own now, and
                // that is the whole of where a menu appears.** They were one
                // layer pinned to the ends of the toolbar — the Document menu
                // to the left edge, the other two to the right — on the
                // reasoning that a measured offset would need keeping in step
                // by hand and there is no way to ask an element where it is
                // from here. Both halves of that were true and the conclusion
                // was wrong: an absolutely positioned child of a
                // `position: relative` wrapper needs no measurement at all,
                // and it is *the browser* that keeps it in step. The View
                // menu came down under the page field, three chips to the
                // right of the button that opened it.
                //
                // Out of the flow, so the 46px row is still 46px whatever is
                // hanging off it — which is what the layer was for and is not
                // a reason to have one.
                div { class: "anchor",
                    button {
                        class: if menu == Some(Menu::Document) { "chip title on" } else { "chip title" },
                        onmousedown: move |event| event.stop_propagation(),
                        onclick: move |_| viewer.write().show_menu(Menu::Document),
                        "{title}"
                    }
                    if menu == Some(Menu::Document) {
                        div { class: "menu document", role: "menu", "aria-label": "Document",
                            // A press inside a menu is not a press outside it: the root
                            // puts the menu away, and the item's own click comes after
                            // the press. This was on the layer these three used to share.
                            onmousedown: move |event| event.stop_propagation(),
                            button {
                                class: "menu-item",
                                onclick: {
                                    let pick = pick.clone();
                                    let frame = frame.clone();
                                    move |_| {
                                        viewer.write().close_menu();
                                        if let Some(path) = pick.choose() {
                                            if viewer.write().open_here(&path) {
                                                let title =
                                                    viewer.read().store.title().to_string();
                                                frame.ask(Ask::Showing { path, title });
                                            }
                                        }
                                    }
                                },
                                span { class: "menu-label", "Open document…" }
                                span { class: "menu-key", "{key_open}" }
                            }
                            // The two-documents-at-once route in one step,
                            // which is the app's own wording and its own
                            // reason: pick the second and it arrives beside
                            // the first rather than on top of it. A menu
                            // item and not a key, because it is not a key
                            // in `keys.ts` either.
                            button {
                                class: "menu-item",
                                onclick: {
                                    let pick = pick.clone();
                                    let frame = frame.clone();
                                    move |_| {
                                        viewer.write().close_menu();
                                        if let Some(path) = pick.choose() {
                                            frame.ask(Ask::NewWindowOn(path));
                                        }
                                    }
                                },
                                span { class: "menu-label", "Open in a new window…" }
                            }
                            div { class: "menu-rule" }
                            button {
                                class: "menu-item",
                                onclick: {
                                    let frame = frame.clone();
                                    move |_| {
                                        viewer.write().close_menu();
                                        frame.ask(Ask::NewWindow);
                                    }
                                },
                                span { class: "menu-label", "New window" }
                                span { class: "menu-key", "{key_new_window}" }
                            }
                            button {
                                class: "menu-item",
                                onclick: {
                                    let frame = frame.clone();
                                    move |_| {
                                        viewer.write().close_menu();
                                        frame.ask(Ask::Close);
                                    }
                                },
                                span { class: "menu-label", "Close window" }
                                span { class: "menu-key", "{key_close}" }
                            }
                        }
                    }
                }
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
                // Two buttons that were a cycle and are now a list. The chip
                // still *says* what is in force — the harness reads the zoom
                // and the theme off these two and a reader reads them the same
                // way — and what changed is that clicking one shows the
                // choices instead of stepping to the next of them.
                // The three of them in one sunk group, minus on the left and
                // plus on the right, which is `.zoom-group` in the app. Three
                // quiet words in a row of quiet words read as three more
                // labels; a stepper with the readout between its two ends
                // reads as one control, and it is the same three elements.
                div { class: "zoom-group",
                    button { class: "chip zoom-out", onclick: move |_| viewer.write().zoom(false), "−" }
                    div { class: "anchor",
                        button {
                            class: if menu == Some(Menu::View) { "chip fit on" } else { "chip fit" },
                            onmousedown: move |event| event.stop_propagation(),
                            onclick: move |_| viewer.write().show_menu(Menu::View),
                            "{zoom}"
                        }
                        if menu == Some(Menu::View) {
                            div { class: "menu view", role: "menu", "aria-label": "View",
                                // A press inside a menu is not a press outside it: the root
                                // puts the menu away, and the item's own click comes after
                                // the press. This was on the layer these three used to share.
                                onmousedown: move |event| event.stop_propagation(),
                                button {
                                    class: if fit == Fit::Width { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_fit(Fit::Width); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if fit == Fit::Width { "✓" } else { "" }} }
                                    span { class: "menu-label", "Fit width" }
                                    span { class: "menu-key", "{key_fit_width}" }
                                }
                                button {
                                    class: if fit == Fit::Page { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_fit(Fit::Page); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if fit == Fit::Page { "✓" } else { "" }} }
                                    span { class: "menu-label", "Fit page" }
                                    span { class: "menu-key", "{key_fit_page}" }
                                }
                                button {
                                    class: if fit == Fit::Actual { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().actual_size(); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if fit == Fit::Actual { "✓" } else { "" }} }
                                    span { class: "menu-label", "Actual size" }
                                    span { class: "menu-key", "{key_actual}" }
                                }
                                div { class: "menu-rule" }
                                button {
                                    class: if spread == Spread::Single { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Single); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Single { "✓" } else { "" }} }
                                    span { class: "menu-label", "One page" }
                                }
                                button {
                                    class: if spread == Spread::Two { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Two); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Two { "✓" } else { "" }} }
                                    span { class: "menu-label", "Two pages" }
                                }
                                button {
                                    class: if spread == Spread::Cover { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Cover); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Cover { "✓" } else { "" }} }
                                    span { class: "menu-label", "Two pages, cover alone" }
                                }
                                div { class: "menu-rule" }
                                button {
                                    class: "menu-item",
                                    onclick: move |_| { viewer.write().rotate(-1); viewer.write().close_menu(); },
                                    span { class: "menu-tick", "" }
                                    span { class: "menu-label", "Rotate left" }
                                    span { class: "menu-key", "{key_rotate_left}" }
                                }
                                button {
                                    class: "menu-item",
                                    onclick: move |_| { viewer.write().rotate(1); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if rotation != 0 { "•" } else { "" }} }
                                    span { class: "menu-label", "Rotate right" }
                                    span { class: "menu-key", "{key_rotate_right}" }
                                }
                            }
                        }
                    }
                    button { class: "chip zoom-in", onclick: move |_| viewer.write().zoom(true), "+" }
                }
                div { class: "anchor",
                    button {
                        class: if menu == Some(Menu::Theme) { "chip theme on" } else { "chip theme" },
                        onmousedown: move |event| event.stop_propagation(),
                        onclick: move |_| viewer.write().show_menu(Menu::Theme),
                        "{theme_name}"
                    }
                    if menu == Some(Menu::Theme) {
                        div { class: "menu theme", role: "menu", "aria-label": "Theme",
                            // A press inside a menu is not a press outside it: the root
                            // puts the menu away, and the item's own click comes after
                            // the press. This was on the layer these three used to share.
                            onmousedown: move |event| event.stop_propagation(),
                            for (index, name) in themes.iter().cloned().enumerate() {
                                button {
                                    key: "{index}:{name}",
                                    class: if index == theme_index { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| {
                                        viewer.write().set_theme(index);
                                        viewer.write().close_menu();
                                    },
                                    span { class: "menu-tick", {if index == theme_index { "✓" } else { "" }} }
                                    span { class: "menu-label", "{name}" }
                                }
                            }
                        }
                    }
                }
                // The page field, which is a field rather than a readout for
                // the app's own reason: stepping is fine for nudging and
                // hopeless for arriving, and a reader with a citation in
                // front of them has a number to type. What it shows is the
                // page's *label* — see [`Viewer::label`].
                // **The box is the width of the number in it, and that is a
                // workaround wearing a design's clothes.** Blitz lays a text
                // input's own text out through parley and never gives parley
                // an alignment — `create_text_editor` in
                // `blitz-dom/src/layout/construct.rs` copies the font size,
                // the line height and the brush and stops, and it calls
                // `editor.set_width(None)`, so there is no box to align
                // within either. `text-align: center` on an input does
                // nothing at all, which is what left the page number pinned
                // against the left wall of a box wide enough for four digits.
                //
                // Centring it is therefore not available; making the box the
                // size of what is in it is, and it is the better answer
                // anyway — the field grows as digits are typed and the number
                // never sits in a puddle of empty box. The button and the
                // field take the same width so that opening the field moves
                // nothing.
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
                        class: if page_fresh { "page-field fresh" } else { "page-field" },
                        style: "width: {page_box}px;",
                        r#type: "text",
                        value: "{page_field}",
                        // Not the root's business: see its `onmousedown`.
                        // A press inside the field is somebody putting the
                        // caret somewhere, not somebody leaving — and putting
                        // the caret somewhere is also the end of the
                        // "everything is selected" state, exactly as a click
                        // into a selected field is anywhere else. `oninput`
                        // above depends on that: it can only take the label
                        // off the front while the caret is still at the
                        // front.
                        onmousedown: move |event| {
                            event.stop_propagation();
                            if viewer.read().page_fresh {
                                viewer.write().page_fresh = false;
                            }
                        },
                        "aria-label": "Go to page",
                        "data-keyboard": "goto",
                        onmounted: move |event| {
                            let node = event.data();
                            let task = node.set_focus(true);
                            spawn(async move { let _ = task.await; });
                        },
                        // **This is the other half of the emulated
                        // select-all, and it is here rather than in the
                        // keydown because of where the caret ends up.**
                        // Cancelling the keystroke and writing the character
                        // into the value attribute works — Blitz's
                        // `set_text` replaces the editor's string — but
                        // `set_text` does not touch the *selection*, and a
                        // field that has just been built has its caret at
                        // offset 0. So the second digit landed in front of
                        // the first and "50" was typed as "05", which parses
                        // to page 5 and is the sort of fault that passes
                        // every test written in one digit.
                        //
                        // Letting the editor do its own insertion moves the
                        // caret for us. Fresh means the caret was at the
                        // front, so whatever arrived is at the front of the
                        // value, and taking the old label off the end leaves
                        // exactly what was typed — with the caret now behind
                        // it, where the next digit wants it.
                        oninput: move |event| {
                            let typed = event.value();
                            let held = viewer.read();
                            let (fresh, was) = (held.page_fresh, held.page_typed.clone());
                            drop(held);
                            if fresh {
                                let first = typed.strip_suffix(&was).unwrap_or(&typed);
                                let first = first.to_string();
                                viewer.write().type_page(&first);
                            } else {
                                viewer.write().type_page(&typed);
                            }
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
                                // **A menu is outermost and this field owns
                                // the keyboard, which is a contradiction only
                                // Blitz makes.** The app puts the ordering in
                                // one document-level capturing handler; here
                                // the keyboard belongs to the innermost
                                // element asking for it, so a field that has
                                // it has to defer to the menu itself. See
                                // `Action::Dismiss`, which is the same list in
                                // the same order for the case where no field
                                // has the keyboard at all.
                                Key::Escape => {
                                    event.stop_propagation();
                                    if !viewer.write().close_menu() {
                                        viewer.write().cancel_page();
                                    }
                                }
                                // Backspace on a field whose contents are
                                // all "selected" empties it, which is what
                                // Backspace on a real selection does. The
                                // editor's own would delete what is before
                                // the caret, and the caret is at the front,
                                // so without this it does nothing at all.
                                Key::Backspace | Key::Delete
                                    if plain && viewer.read().page_fresh =>
                                {
                                    event.stop_propagation();
                                    event.prevent_default();
                                    viewer.write().type_page("");
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
                        style: "width: {page_box}px;",
                        "aria-label": "Go to page",
                        onclick: move |_| viewer.write().open_page_field(),
                        "{page_field}"
                    }
                    }
                    span { class: "of", "/ {pages}" }
                }
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
                                // The menu first, for the reason the page
                                // field gives above.
                                Key::Escape => {
                                    event.stop_propagation();
                                    if !viewer.write().close_menu() {
                                        viewer.write().close_find();
                                    }
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
            if sidebar_open && !presenting {
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
                    let (across, down) = match event.delta() {
                        WheelDelta::Pixels(delta) => (delta.x, delta.y),
                        WheelDelta::Lines(delta) => (delta.x * LINE, delta.y * LINE),
                        WheelDelta::Pages(delta) => {
                            let height = viewer.read().layout.viewport.height;
                            (delta.x * height, delta.y * height)
                        }
                    };
                    // The other axis, which only ever has anything in it when
                    // the reader has zoomed past the width of the window.
                    // macOS turns ⇧-wheel into one of these before winit sees
                    // it, so this is both gestures.
                    if across != 0.0 {
                        viewer.write().pan(-across);
                    }
                    viewer.write().nudge(-down);
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
                            key: "{placed.index}:{placed.width}x{placed.height}:{theme_name}:{view_key}:{edition}",
                            document: Handle(document.clone()),
                            chosen: chosen.clone(),
                            index: placed.index,
                            top: placed.top - scroll_top,
                            left: placed.left - scroll_left,
                            width: placed.width,
                            height: placed.height,
                            hits: placed.hits,
                            links: placed.links,
                            selected: placed.selected,
                            view,
                            viewer,
                            away: away.clone(),
                        }
                    }
                }
            }
            }
            // The line the toolbar's own way back is written on, which is
            // why it outlives the toolbar. Presenting is the case where
            // nothing is on screen at all.
            if !presenting {
                div { class: "notice", "{notice}" }
            }
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
    /// What the reader has swept over, on this page.
    ///
    /// Rectangles like the matches and for the same reason, and painted in
    /// the theme's own selection colour — which is exactly what the theme
    /// says it is for. What this cannot do is the app's other half: there,
    /// `paintSelection` copies the pixels under each selected line off the
    /// page canvas and runs them back through the luminance ramp, so selected
    /// words come out as the theme's ink on the theme's selection ground. Here
    /// a translucent rectangle lies over the printed words and they keep the
    /// colour they were printed in. It is the honest version of the same
    /// statement and it is one shader short of the app's.
    selected: Vec<Rect>,
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
            // Where a sweep begins, and the whole of what a page hears from
            // the pointer.
            //
            // **`onclick` and `ondoubleclick` would never fire here**, which
            // is not obvious and cost an hour: a page is a custom widget, and
            // `handle_dom_event` in `blitz-dom` forwards an event whose target
            // is a widget straight to the widget and *returns* — so the
            // default action never runs, and `click` and `dblclick` are both
            // default actions of `pointerup`. Handlers still run, because the
            // handler phase is before the default action, which is why
            // `onmousedown` and the root's `onmouseup` work at all. So the
            // second click is counted here rather than heard about; see
            // [`Viewer::begin_sweep`], which is Blitz's own rule — half a
            // second and two pixels — restated where it can be reached.
            //
            // The rest of the sweep is on the root, because a pointer dragged
            // down a document leaves the page it started on within a line or
            // two and the root is the one ancestor that spans the window.
            onmousedown: move |event| {
                let on = event.element_coordinates();
                let client = event.client_coordinates();
                viewer.write().begin_sweep(
                    index + 1,
                    (on.x, on.y),
                    (client.x, client.y),
                );
            },
            object {
                "data": widget,
                // A widget laid out at 0×0 is a blank window with nothing to
                // say why, which is what `display: block` costs to avoid.
                style: "display: block; width: {width}px; height: {height}px;",
            }
            for (at, area) in selected.iter().enumerate() {
                div {
                    key: "s{at}",
                    class: "selected",
                    style: "position: absolute; top: {area.top}px; left: {area.left}px; width: {area.width}px; height: {area.height}px;",
                }
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
fn perform(
    mut viewer: Signal<Viewer>,
    action: Action,
    screen: f64,
    frame: &Frame,
    clip: &Clip,
    pick: &Pick,
) {
    // Every movement goes through the viewer rather than through an offset,
    // because in paged mode an offset is not where a reader ends up: the page
    // has to be turned first. See [`Viewer::nudge`] and [`Viewer::go_to`].
    fn by(mut viewer: Signal<Viewer>, delta: f64) {
        viewer.write().nudge(delta);
    }
    fn page(mut viewer: Signal<Viewer>, page: usize) {
        viewer
            .write()
            .go_to(crate::layout::Anchor { page, offset: 0.0 });
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
        // Escape, which is the way out of four things and says nothing when
        // there is nothing to leave — a key that answers with a complaint
        // about what it did not do is worse than one that does nothing.
        Action::Dismiss => {
            // Outward, in the order the reader arrived at them. The page
            // field first, because it is the thing they are most recently
            // inside: pressing Escape while typing a number means "not that
            // after all", not "close the find bar I opened a minute ago".
            // Escape typed *into* the field never reaches here — the field
            // stops it — so this is the case where the field is open and the
            // pointer took the focus elsewhere.
            // A menu is the outermost thing of all — it is over everything
            // else and it is the thing the pointer is inside — so it goes
            // first, ahead of the field somebody may have opened underneath
            // it. The app says the same: its document-level handler stands
            // down for `dismiss` alone while a menu is up.
            if viewer.write().close_menu() {
                return;
            }
            let (typing, finding, selected, presenting, full) = {
                let held = viewer.read();
                (
                    held.typing_page,
                    held.find_open,
                    held.has_selection(),
                    held.presenting,
                    held.full_screen,
                )
            };
            if typing {
                viewer.write().cancel_page();
            } else if finding {
                viewer.write().close_find();
            } else if selected {
                // Between the find bar and presenting, which is where a
                // selection sits in the same "outward, in the order the reader
                // arrived" order: it is a thing on the page, and full screen
                // and presenting are things the window is doing. A reader who
                // is presenting and has swept a sentence to point at means to
                // put the sentence down first.
                viewer.write().clear_selection();
            } else if presenting {
                let full = viewer.write().present(false);
                frame.ask(Ask::FullScreen(full));
            } else if full {
                viewer.write().set_full_screen(false);
                frame.ask(Ask::FullScreen(false));
            }
        }
        // The three a selection is for. `Copy` is this experiment's own —
        // see `keymap::EXTRA`: in the app ⌘C is the webview's, because the
        // browser owns the selection and therefore owns copying it.
        Action::SelectPage => {
            if viewer.write().select_page() {
                let words = viewer.read().selected_text().chars().count();
                viewer.write().notice = format!("{words} characters of this page selected.");
            }
        }
        Action::Copy => {
            let copied = viewer.read().selected_text();
            if copied.is_empty() {
                viewer.write().notice = "Select something first, and this copies it.".into();
            } else {
                clip.put(&copied);
                viewer.write().notice = "Copied.".into();
            }
        }
        Action::CopyQuote => {
            let quoted = viewer.read().quoted();
            let said = match quoted {
                Some((quote, where_from)) => {
                    clip.put(&quote);
                    format!("Copied, with {where_from}.")
                }
                None => "Select something first, and this copies it with its page number.".into(),
            };
            viewer.write().notice = said;
        }
        // The window's own three, which the page can only ask for. See
        // [`Frame`]: what answers is the shell in the app and a list in the
        // harness, and the reader's side is the same either way.
        // The picker, through `Pick` rather than through the shell directly,
        // because a modal window belonging to the operating system is the one
        // door in this crate a test must not be able to open. The other thing
        // a chosen document can mean — a window of its own — is a menu item
        // and not a key, which is the app's own arrangement: there is no
        // `open-new-window` in `keys.ts` either.
        Action::Open => {
            if let Some(path) = pick.choose() {
                if viewer.write().open_here(&path) {
                    let title = viewer.read().store.title().to_string();
                    frame.ask(Ask::Showing { path, title });
                }
            }
        }
        Action::NewWindow => frame.ask(Ask::NewWindow),
        Action::CloseWindow => frame.ask(Ask::Close),
        Action::Quit => frame.ask(Ask::Quit),
        Action::Toolbar => viewer.write().toggle_toolbar(),
        Action::Fullscreen => {
            let on = !viewer.read().full_screen;
            viewer.write().set_full_screen(on);
            frame.ask(Ask::FullScreen(on));
        }
        Action::Present => {
            let on = !viewer.read().presenting;
            let full = viewer.write().present(on);
            frame.ask(Ask::FullScreen(full));
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
