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

/// What handing the document to something that prints does.
///
/// A context holding one closure, for the reason [`Away`] is one, and it is
/// the same answer the app arrives at from the other side: this application
/// does not print, it hands the file to one that does. `print_document` in
/// the app's `lib.rs` is `open -a Preview` on macOS, Edge by absolute path on
/// Windows and `xdg-open` on Linux, and the reasoning under those choices is
/// worth having rather than restating — the point of naming a program is that
/// it is **not us**, because the system's default handler for a PDF may well
/// be this reader, and handing a document to ourselves to print it is a loop.
///
/// A test must not be able to open Preview, which is why this is a door at
/// all: `cargo test` printing four hundred pages would be a worse trespass
/// than [`Clip`]'s.
/// What handing a document over came to: nothing, or a sentence for the
/// notice line.
type Handover = Rc<dyn Fn(&str) -> Result<(), String>>;

#[derive(Clone)]
pub struct Printer(Handover);

impl PartialEq for Printer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Printer {
    pub fn new(print: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        Printer(Rc::new(print))
    }

    /// The default: the platform's own, exactly as the app names them.
    pub fn to_the_system() -> Self {
        Printer::new(|path| {
            let file = std::path::PathBuf::from(path);
            if !file.exists() {
                let name = file
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string());
                return Err(format!("{name} is no longer there."));
            }

            #[cfg(target_os = "macos")]
            let mut command = {
                let mut command = std::process::Command::new("open");
                command.arg("-a").arg("Preview").arg(&file);
                command
            };

            // Edge by absolute path, because it is not on `PATH` and because
            // naming it is the whole point: `ShellExec_RunDLL` alone opens the
            // file with the *default* handler, which after installing this
            // reader may be this reader.
            #[cfg(target_os = "windows")]
            let mut command = {
                let edge = std::env::var("ProgramFiles(x86)")
                    .or_else(|_| std::env::var("ProgramFiles"))
                    .map(|root| {
                        std::path::PathBuf::from(root)
                            .join(r"Microsoft\Edge\Application\msedge.exe")
                    })
                    .ok()
                    .filter(|path| path.exists());
                match edge {
                    Some(edge) => {
                        let mut command = std::process::Command::new(edge);
                        command.arg(&file);
                        command
                    }
                    None => {
                        let mut command = std::process::Command::new("rundll32.exe");
                        command.arg("shell32.dll,ShellExec_RunDLL").arg(&file);
                        command
                    }
                }
            };

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let mut command = {
                let mut command = std::process::Command::new("xdg-open");
                command.arg(&file);
                command
            };

            command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("Could not hand the document over to print: {err}"))
        })
    }

    pub fn print(&self, path: &str) -> Result<(), String> {
        (self.0)(path)
    }
}

/// The document, shown where it lives — "Show in Finder", and its two other
/// names on the two other platforms.
///
/// A door of its own beside [`Printer`] and for the same reason: it hands a
/// path to a program outside this process, which is exactly what a test must
/// not do. `revealDocument` in `api.ts`.
#[derive(Clone)]
pub struct Reveal(Handover);

impl PartialEq for Reveal {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Reveal {
    pub fn new(show: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        Reveal(Rc::new(show))
    }

    pub fn to_the_system() -> Self {
        Reveal::new(|path| {
            let file = std::path::PathBuf::from(path);
            if !file.exists() {
                return Err(format!("{} is no longer there.", where_it_lives(&file)));
            }

            #[cfg(target_os = "macos")]
            let mut command = {
                let mut command = std::process::Command::new("open");
                command.arg("-R").arg(&file);
                command
            };

            #[cfg(target_os = "windows")]
            let mut command = {
                let mut command = std::process::Command::new("explorer.exe");
                command.arg(format!("/select,{}", file.display()));
                command
            };

            // Nothing on Linux selects a file portably, so the folder it is in
            // is what opens. Saying the folder is the whole of what the item
            // promises anywhere.
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let mut command = {
                let mut command = std::process::Command::new("xdg-open");
                command.arg(file.parent().unwrap_or(&file));
                command
            };

            command
                .spawn()
                .map(|_| ())
                .map_err(|err| format!("Could not show the document: {err}"))
        })
    }

    pub fn show(&self, path: &str) -> Result<(), String> {
        (self.0)(path)
    }
}

fn where_it_lives(file: &std::path::Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string())
}

/// What the file manager is called here. `fileManagerName` in `api.ts`.
pub fn file_manager_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Finder"
    } else if cfg!(target_os = "windows") {
        "File Explorer"
    } else {
        "the file manager"
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

/// Whether the machine is in dark mode, asked of whatever knows.
///
/// [`Screen`]'s sibling, and for the same reason: a component that asks winit
/// what the system appearance is cannot be built without winit, and the
/// harness has no window. The shell answers it out of `Window::theme()`, the
/// harness out of a cell a test can set, and nothing else answers it at all.
///
/// It answers `Option<bool>` where the app's `matchMedia` answers `bool`,
/// because winit says `Option<Theme>` and the absence is real: a platform
/// that does not report an appearance must leave the reader wearing what they
/// chose rather than be read as "light". See [`crate::store::Store::outside`].
#[derive(Clone)]
pub struct Appearance(Rc<dyn Fn() -> Option<bool>>);

impl PartialEq for Appearance {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Appearance {
    pub fn new(dark: impl Fn() -> Option<bool> + 'static) -> Self {
        Appearance(Rc::new(dark))
    }

    /// A machine that will not say, which is what a shell that has not
    /// provided one leaves behind and what most tests want.
    pub fn unknown() -> Self {
        Appearance::new(|| None)
    }

    pub fn get(&self) -> Option<bool> {
        (self.0)()
    }
}

/// Said when a theme chosen by hand has taken the reader off following the
/// machine. It names the window the switch is in, because a switch that moved
/// without being touched is one the reader has to be able to find.
pub const FOLLOWING_OFF: &str =
    "No longer following the system's light and dark. Settings has the switch.";

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
/// The toolbar's own bottom border, and the only hairline left: the notice
/// line used to be a row of the flex column under it and had one too.
const HAIRLINE: f64 = 1.0;

/// What the chrome costs with all of it on screen, which is what a window
/// opens with.
pub const CHROME: f64 = TOOLBAR + HAIRLINE;

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
/// How long a message stays on the notice line. `ui.notice` in the app.
const NOTICE_LASTS: std::time::Duration = std::time::Duration::from_millis(4200);

/// How wide the strip at the right edge of a note that is a passage rather
/// than a marker. See [`crate::render::Note`].
const NOTE_EDGE: f64 = 14.0;

/// How far down the window the peek handle stays once it is down. The app's
/// own 110px: the handle sits lower in full screen and the hand has to travel
/// to reach it, so it must not go away while it is being reached for.
const PEEK_KEEP: f64 = 110.0;

/// How long the page pill stays up after a scroll. `flashPill` in `main.ts`.
const PILL_LASTS: std::time::Duration = std::time::Duration::from_millis(1100);

const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(500);

/// How long a zoom gesture goes on being one after the last event of it.
///
/// A trackpad's magnification events arrive about a frame apart and a
/// ⌃-wheel's rather further, so this is the gap that says the fingers have
/// stopped rather than paused — long enough that a slow drag is not cut into
/// three gestures, short enough that the page comes back sharp before the
/// reader has read a line of it. See [`Viewer::settle_zoom`].
const ZOOM_SETTLES: std::time::Duration = std::time::Duration::from_millis(180);

/// The zoom ladder, in the app's own steps.
/// `ZOOM_LADDER` in `main.ts`, and the same sixteen steps: three of them —
/// 175%, 250% and 600% — had been dropped on the way across, so ⌘+ walked a
/// shorter ladder here and the top of it was 400%.
const ZOOMS: [f64; 16] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 6.0,
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
/// **It asks and does not answer, and that is a crash rather than a taste.**
/// `rfd` shows an `NSOpenPanel` and runs it *modally*, which spins a nested
/// run loop — and a nested run loop delivers events to winit, which is already
/// inside `EventHandler::handle` because a click on a menu item is what got us
/// here. That handler holds a `RefMut` on itself for exactly this reason and
/// panics on re-entry: `tried to handle event while another event is currently
/// being handled`, from a stack with nothing of this app in it. So the picker
/// is opened from a thread of its own, where `rfd` dispatches it back onto the
/// main queue — which the main thread reaches *after* the click has been
/// handled and the handler is free. The answer comes back the way every other
/// answer from a thread comes back in this reader, as news in the mailbox, and
/// is handled beside the document dropped on the window and the document
/// handed over by a second launch, both of which were already that shape.
///
/// A picker the reader closed sends nothing, which is what cancelling means.
#[derive(Clone)]
pub struct Pick(Rc<dyn Fn(Opening)>);

/// Which door a chosen document goes through: this window, or one of its own.
///
/// The two are a menu item apart in the interface and a whole window apart
/// underneath, and the choice has to travel with the ask because by the time
/// the answer arrives the menu it was chosen from is long shut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// Here, replacing what this window is showing. ⌘O and the start screen.
    Here,
    /// Beside it, in a window of its own — the app's "Open document in new
    /// window…", which is the two-documents-at-once route in one step.
    Beside,
}

impl Opening {
    /// The news a chosen document arrives as. Both are handled in `Reader`'s
    /// one mailbox task; `open-document` is the same event a drop and a second
    /// launch already send, and a picked document is the same thing happening
    /// for a different reason.
    fn event(self) -> &'static str {
        match self {
            Opening::Here => "open-document",
            Opening::Beside => "open-document-beside",
        }
    }
}

impl PartialEq for Pick {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Pick {
    pub fn new(choose: impl Fn(Opening) + 'static) -> Self {
        Pick(Rc::new(choose))
    }

    /// The default: the system's own picker, filtered to PDFs, opened on a
    /// thread and answered into the mailbox.
    pub fn from_the_system(
        shell: Option<Arc<dyn blitz_traits::shell::ShellProvider>>,
        post: crate::emit::Post,
    ) -> Self {
        Pick::new(move |opening| {
            // Said once rather than silently choosing nothing, which is
            // `Clip`'s rule and the same reason: a reader who presses ⌘O and
            // gets no window cannot tell a shell that provided no picker from
            // a picker they cancelled.
            let Some(shell) = shell.clone() else {
                eprintln!("there is no picker to choose a document with");
                return;
            };
            let post = post.clone();
            std::thread::spawn(move || {
                let filter = blitz_traits::shell::FileDialogFilter {
                    name: "PDF".to_string(),
                    extensions: vec!["pdf".to_string()],
                };
                let chosen = shell
                    .open_file_dialog(false, Some(filter))
                    .into_iter()
                    .next();
                if let Some(path) = chosen {
                    post.send(crate::emit::News {
                        event: opening.event().to_string(),
                        target: None,
                        payload: serde_json::Value::String(path.to_string_lossy().into_owned()),
                    });
                }
            });
        })
    }

    /// Ask for a document. What comes back, comes back later.
    pub fn ask(&self, opening: Opening) {
        (self.0)(opening);
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

/// What a mark says it was made by, which is what every other reader shows in
/// the margin beside it. The app writes nothing here, because pdf.js's
/// annotation editor does not offer to; this reader is writing the annotation
/// itself and there is no reason to leave it anonymous.
const AUTHOR: &str = "HyloPDF";

/// How tall a signature is dropped, in the page's own points.
///
/// A fixed height rather than a fraction of the page, and the reason is that a
/// signature is a fact about a hand and not about the paper: a name written on
/// a postcard and the same name written on a poster are the same size. Forty
/// points is about 14mm, which is what a signature on a printed form measures.
/// Every other size question in this reader is the page's; this one is not.
const HAND_HEIGHT: f64 = 40.0;

/// The drawing pad in the Sign window, in CSS pixels.
///
/// Written down rather than measured, and that is the one awkward thing about
/// the pad: a stroke arrives as a point inside the element, and turning it into
/// the unit box needs the element's size — which the handler cannot ask for.
/// Blitz gives an event its coordinates and not its target's box, so the pad
/// is sized from these on the element itself rather than in `styles.rs` — the
/// sheet is a `const &str` and cannot interpolate, and two numbers that have to
/// agree is exactly the copy this codebase keeps warning about.
pub const PAD_WIDTH: f64 = 440.0;
pub const PAD_HEIGHT: f64 = 150.0;

/// One row of the markup list, wherever the mark itself is being kept.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkRow {
    pub page: usize,
    pub color: String,
    /// The words under it — read off the page for a mark in the document,
    /// and remembered for one the journal is holding, because a mark that is
    /// beside a document may be beside a document that has been rewritten.
    pub quote: String,
    pub key: MarkKey,
}

/// How to find a mark again in order to take it out.
///
/// **The two halves of this enum are the whole of what item 11 found.** A
/// mark in the file is named by where it sits and is removed by removing it;
/// a mark beside the file is named by an id this reader made up and is
/// removed from a TOML table. In the app every mark is the second kind for
/// the purposes of removal, because the first kind cannot be removed at all —
/// which is why its journal needs a durable id and this one does not.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkKey {
    /// In the document: the page, and where among that page's annotations.
    /// Good for as long as the document is the one it was read from, which
    /// is until the next write — and every write here is followed by a
    /// reopen and a re-read.
    InFile(usize, usize),
    /// Beside the document, in `library.toml`, by the id it was given.
    Beside(String),
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

/// Which page of the Settings window is open.
///
/// **A window in the flow rather than a window of the system's**, which is
/// what the app does too: `showWindow` in `ui.ts` is a scrim and a frame in
/// the same document, not a second webview. That matters more here than
/// there — a second winit window would be a second `Viewer` over a second
/// `Store`, and every setting changed in it would reach the reader on its
/// next launch. See `AGENTS.md` on exactly that staleness between two reader
/// windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Reading,
    Appearance,
    Window,
    Keyboard,
    About,
}

impl Pane {
    /// The five, in the order the nav column lists them.
    pub const ALL: [Pane; 5] = [
        Pane::Reading,
        Pane::Appearance,
        Pane::Window,
        Pane::Keyboard,
        Pane::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pane::Reading => "Reading",
            Pane::Appearance => "Appearance",
            Pane::Window => "Window",
            Pane::Keyboard => "Keyboard",
            Pane::About => "About",
        }
    }

    /// The icon beside it, by the app's own name for the drawing.
    pub fn icon(self) -> &'static str {
        match self {
            Pane::Reading => "book",
            Pane::Appearance => "theme",
            Pane::Window => "sidebar",
            Pane::Keyboard => "keyboard",
            Pane::About => "info",
        }
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
    /// Under the document's name: what can be done with the document that is
    /// open — mark it, highlight in it, print it, copy what it is called.
    Document,
    /// Under "Open…": every way to bring a *different* document onto the
    /// screen, and the shelf of what was read before. Kept apart from
    /// [`Menu::Document`] for the app's own reason — a menu opened by pressing
    /// the name of the paper you are reading should not be four items about
    /// papers you are not.
    Open,
    /// Under the theme's name: every theme installed, the one in use ticked.
    Theme,
    /// Under the zoom: the fit modes, a stepper and the presets — which is
    /// `showZoomMenu` in `main.ts`, item for item.
    View,
    /// Under the cog: the switches somebody reaches for while reading, and
    /// the way to the window that holds all of them. `showSettingsMenu`.
    Settings,
}

impl Menu {
    /// What the button that opens it is called, which is also what the menu
    /// is called to a screen reader.
    pub fn label(self) -> &'static str {
        match self {
            Menu::Document => "Document",
            Menu::Open => "Open",
            Menu::Theme => "Theme",
            Menu::View => "View",
            Menu::Settings => "Settings",
        }
    }
}

/// Everything the reader is looking at, and everything that changes it.
/// A document that will not open without a password, and what has been said
/// to it so far.
///
/// `ui.askForPassword` in the app, which is a window with a field in it and a
/// promise the load is waiting on. There is nothing to wait here: pdfium
/// answers at open, so a locked document is a piece of *state* — this window
/// is showing whatever it was showing, and there is a question over it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Locked {
    /// The document being asked about. Not opened, and not the document this
    /// window is showing: a reader who declines keeps what they had.
    pub path: String,
    /// What has been typed, in the clear. What is on screen is bullets — see
    /// [`Viewer::type_password`].
    pub typed: String,
    /// Whether an answer has already been given and refused, which is the
    /// difference between the two sentences over the field.
    pub wrong: bool,
}

/// The Sign window: what is drawn on the pad, and what it will be called.
///
/// **The pad is the one place in this reader where the pointer draws.**
/// Everything else it does is choosing — a word, a page, a colour — and this
/// is the one gesture whose whole content is the path the pointer took. So the
/// points are kept as they arrive, in the pad's own space, and normalised into
/// the unit box only when the pad's size is in hand; see [`Viewer::draw_to`].
///
/// The strokes are *not* a `Signature` while they are being drawn, deliberately:
/// a signature is a thing on disk in a unit box, and these are pixels on a pad.
/// The conversion happens once, at the moment it is kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Signing {
    /// What the reader is calling it. A name is asked for because several are
    /// the ordinary case — a full name and a set of initials — and a list of
    /// rows all called the same thing is a list nobody can choose from.
    pub name: String,
    /// The strokes so far, in the pad's own pixels from its top left.
    pub strokes: Vec<Vec<[f64; 2]>>,
    /// Whether the pointer is down, which is what makes a move add a point to
    /// the last stroke rather than start one.
    pub drawing: bool,
    /// How big the pad was when it was last drawn on, so that the strokes can
    /// be put into the unit box without asking the DOM. Written by the pad's
    /// own press handler, which is the one place the size is known.
    pub pad: (f64, f64),
    /// Where the pad's top left corner is in the window, worked out at the
    /// press from the difference between the two coordinate systems the event
    /// carries. See [`Viewer::draw_from`].
    pub origin: (f64, f64),
}

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
    /// A document is being dragged over this window, and whether it is one
    /// this reader would open — `#drop-hint` in the app, arriving from winit
    /// rather than from the DOM. `None` is nothing over the window, which is
    /// almost always.
    pub dragging: Option<bool>,
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
    /// **Whether the panel on screen is one the search borrowed.** A count in
    /// the find bar is the answer to "is it in here" and the list behind it is
    /// the answer to "which one did I mean" — the app makes the count the way
    /// through to the list, and this reader does too. What the app does not
    /// have is a way back: it opens the panel and leaves it open, because
    /// closing something the reader can see is the sort of tidying that loses
    /// people their place. Here the panel came up on its own, so it goes away
    /// on its own — one Escape takes the bar down and the panel with it, and
    /// only when the panel was shut to begin with. A reader who was already
    /// reading with the contents open keeps them.
    results_borrowed: bool,
    /// The page pill: whether it is up, and which flash put it there.
    ///
    /// A token rather than a timer, exactly as the notice line is done — see
    /// the `use_effect` in [`Reader`]: the thread that will take the pill down
    /// carries the number it was started for, so a scroll during the second
    /// it is up keeps it up rather than having it vanish on the first one's
    /// clock.
    pill_up: bool,
    pill_token: u64,
    /// Whether the handle that gives the toolbar back is down. See
    /// [`Viewer::reach_for_toolbar`].
    peek: bool,
    /// Whether this search has already put its results on screen. Once per
    /// search, like [`Viewer::revealed`] beside it and for the same reason:
    /// a scan is many slices and the reader must be able to close what one of
    /// them opened. See [`Viewer::show_the_matches`].
    offered_results: bool,
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
    /// Which page of Settings is up, if any. See [`Pane`].
    pub pane: Option<Pane>,
    /// And which page it was on when it was last put away, so that reopening
    /// it comes back to where the reader was.
    pane_last: Pane,
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
    /// And the notes on it, kept the same way and for the same reason: a
    /// document that carries none — which is most of them — asks pdfium once
    /// per page and gets an empty list it can hold on to.
    notes: RefCell<HashMap<usize, Rc<Vec<crate::render::Note>>>>,
    /// The note the reader has opened, if any. See the note window in
    /// [`Reader`] — `showNote` in `main.ts`.
    pub note_open: Option<(usize, crate::render::Note)>,
    /// The document waiting on a password, if one is. `ui.askForPassword` in
    /// the app, and see [`Locked`].
    pub locked: Option<Locked>,
    /// Whether the Information window is up. `showDocumentDetails` in
    /// `main.ts`: what the document says about itself, which nothing else in
    /// this reader shows.
    pub details_open: bool,
    /// The theme being written, if one is. `editing` in `settings.ts`: a
    /// draft is installed as the live theme while it is being made, which is
    /// how the app around you becomes the preview.
    pub editing: Option<crate::theme::Theme>,
    /// The Sign window, when it is up. See [`Signing`], and [`crate::sign`]
    /// for what the word does and does not mean here.
    pub signing: Option<Signing>,
    /// A signature chosen and waiting for somewhere to go.
    ///
    /// **Signing is two gestures, and this is the gap between them.** Choosing
    /// which name to sign with and choosing where on the page it goes are
    /// different questions, and a window that answered both would have to
    /// contain the page. So the window closes, this is armed, the pointer
    /// becomes the thing that answers the second question, and one click on
    /// a page puts it there. Escape disarms it, like everything else that
    /// waits.
    pub placing: Option<crate::sign::Signature>,
    /// Whether the reader has been told, for this document, that signing it
    /// rewrites the file and breaks the cryptographic signature it carries.
    /// Once, and before it happens — see [`crate::sign::BREAKS_A_SIGNATURE`].
    said_rewrites: bool,
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
    /// Every highlight in the document, read out of the file at open and
    /// again after every reload. See [`crate::markup`].
    ///
    /// **The file is the record and this is a reading of it**, which is why
    /// there is nothing to keep in step: a mark is written, the document is
    /// reopened through the path a recompile already uses, and this is filled
    /// from what came back. The app has a journal that has to be reconciled
    /// against the file on every open because its writes and its reads are on
    /// opposite sides of a bridge; here they are the same call.
    pub markup: Vec<crate::markup::Mark>,
    /// Where a mark can go on this document, asked once when it opened.
    standing: crate::markup::Standing,
    /// Whether the reader has been told about the standing yet. Once per
    /// document, as the app says it: a sentence repeated on every gesture is
    /// a sentence nobody reads.
    said_standing: bool,
    /// The colour popover: which page it is over and where on that page, in
    /// the same space as a mark's own quads.
    ///
    /// `None` almost always. It opens where the selection ends rather than
    /// off a toolbar button, because nothing in the toolbar is what it is
    /// about — which is `showMarkupPopover` in `main.ts` making a throwaway
    /// element over the selection's own rectangle, said in a reader that can
    /// simply put the node on the page.
    pub markup_at: Option<(usize, Rect)>,
    /// The mark the pointer was last clicked on, and what to say about it:
    /// which page, where on it, what colour it is and how to take it out.
    ///
    /// **The whole of what "I cannot remove a highlight" turned out to
    /// be.** Removal was built, tested and reachable — from a ten-pixel × on
    /// a row in the Contents panel, behind a tab the panel does not open on.
    /// A mark is a thing on a page, so the way to take it off is on the page:
    /// click it and it says so. Nothing about the removal itself changed.
    pub mark_open: Option<(usize, Rect, MarkKey, String)>,
    /// Where the last press landed, in the page's own space, so that letting
    /// go without having swept anywhere can ask what is under it.
    pressed_on: Option<(usize, f64, f64)>,
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
    /// The scale a zoom gesture began at, while one is under way.
    ///
    /// A pinch and a ⌃-wheel are a stream rather than a step — see
    /// [`Viewer::zoom_by`] — and this is what makes the stream one gesture:
    /// every page keeps the texture it was drawn at when the fingers went
    /// down and is stretched to whatever the layout says this frame. The
    /// number is the scale to divide by to get back to that size, which is
    /// what the component key needs so that nothing is re-keyed either. See
    /// [`crate::page::Chosen::holding`].
    zoom_from: Option<f64>,
    /// Which gesture the settle timer is for; see [`Viewer::settle_zoom`].
    zoom_token: u64,
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
            results_borrowed: false,
            offered_results: false,
            note_open: None,
            locked: None,
            details_open: false,
            editing: None,
            signing: None,
            placing: None,
            said_rewrites: false,
            pill_up: false,
            pill_token: 0,
            peek: false,
            sidebar_width: 252.0,
            resize_from: None,
            tab: Tab::Contents,
            menu: None,
            pane: None,
            pane_last: Pane::Reading,
            thumb_scroll: 0.0,
            column: Column::default(),
            headings: document.outline(),
            labels: document.labels(),
            links: RefCell::new(HashMap::new()),
            notes: RefCell::new(HashMap::new()),
            texts: RefCell::new(Vec::new()),
            selection: None,
            markup: Vec::new(),
            standing: crate::markup::Standing::default(),
            said_standing: false,
            markup_at: None,
            mark_open: None,
            pressed_on: None,
            sweep_from: None,
            pressed: None,
            zoom_from: None,
            zoom_token: 0,
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
            dragging: None,
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
        viewer.read_markup();
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
        // A panel the reader has now taken hold of is theirs, whoever opened
        // it: closing the find bar must not take away something that was
        // asked for in between. See `results_borrowed`.
        self.results_borrowed = false;
        self.set_sidebar(!self.sidebar_open, true);
    }

    /// The panel, opened or shut — and whether that is a fact about the
    /// reader or only about what is on screen for the moment.
    ///
    /// `remember` is the whole of the difference: a panel the reader opened is
    /// a setting and comes back next time, and a panel the search borrowed is
    /// neither.
    fn set_sidebar(&mut self, open: bool, remember: bool) {
        if self.sidebar_open == open {
            return;
        }
        self.sidebar_open = open;
        if remember {
            self.store.set(vec![("show_sidebar".into(), json!(open))]);
        }
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
    /// **Only the toolbar costs anything now.** The notice used to be a row
    /// of the flex column under it, so it took 30px off the document whether
    /// or not it had anything to say — which meant a window that had once
    /// been told a zoom percentage wore that percentage along its bottom edge
    /// for the rest of the session. It is a pill over the document now, the
    /// way the app has always had it, and it goes away by itself. See
    /// [`Viewer::notice`] and the "notice-timeout" arm in [`Reader`].
    ///
    /// It still outlives the toolbar, which is the part that was right: the
    /// message saying how to get the toolbar back is written on it, and a
    /// line that disappeared along with the thing it explains would be a joke
    /// at the reader's expense. Presenting is the case where everything goes,
    /// which is what presenting *is*.
    pub fn chrome(&self) -> f64 {
        if self.presenting || !self.toolbar {
            return 0.0;
        }
        TOOLBAR + HAIRLINE
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
        let bindings = self.keymap.by_action.get(&action);
        let shown = bindings.and_then(|bindings| {
            // **A bare function key is the system's on a Mac.** F11 is
            // Mission Control and F1 is the brightness, so a menu offering
            // either as the way to do something is offering a key that will
            // not arrive. Where an action has a chord as well, that is the one
            // to name — which is what `FULLSCREEN_KEYS` in `main.ts` hard-codes
            // for exactly this row, said here as the rule behind it.
            if self.keymap.mac() {
                if let Some(chord) = bindings.iter().find(|binding| binding.contains("mod+")) {
                    return Some(chord);
                }
            }
            bindings.first()
        });
        shown
            .map(|binding| crate::keymap::describe_binding(binding, self.keymap.mac()))
            .unwrap_or_default()
    }

    /// Put a menu down, or take the one that is down away. Asking for the
    /// menu that is already open closes it, which is what clicking its own
    /// button means.
    pub fn show_menu(&mut self, menu: Menu) {
        // One thing down at a time: the swatches are a menu in everything but
        // name, and two of them open at once is the thing `showPopover` in
        // `ui.ts` exists to prevent.
        self.markup_at = None;
        // **And the find bar is one of the things that go down.** `wire()` in
        // `main.ts` wraps every control in the bar that opens something of its
        // own in `opens(…)`, which closes the search first, and its reason is
        // the reason: two panels claiming the same corner of the screen, one
        // of them still holding the keyboard, is not a place anybody meant to
        // be. The five menus are all of them; the chips that merely move
        // around the document — the page arrows, the two rotations, the zoom
        // steppers — leave the bar alone, there and here.
        self.close_find();
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
    }

    /* ------------------------------------------------------- the settings */

    /// Put the Settings window up, on the page it was last left on.
    ///
    /// Coming back to the same page is what a window with a nav column is
    /// expected to do, and it is `currentPage` in `settings.ts` — module
    /// scope there, a field here, which is the same lifetime said in Rust:
    /// as long as the reader, not as long as the window.
    pub fn open_settings(&mut self) {
        self.close_menu();
        self.pane = Some(self.pane_last);
    }

    pub fn close_settings(&mut self) -> bool {
        if let Some(pane) = self.pane.take() {
            self.pane_last = pane;
            return true;
        }
        false
    }

    pub fn show_pane(&mut self, pane: Pane) {
        self.pane = Some(pane);
        self.pane_last = pane;
    }

    /// A flag straight through to the settings file, for the switches whose
    /// whole effect is that they are written down — `remember_position` is
    /// read at the next open, `reopen_last_document` by the next launch.
    pub fn set_flag(&mut self, key: &str, on: bool) {
        self.store.set(vec![(key.to_string(), json!(on))]);
    }

    /// The gap between one page and the next, which is a distance on the
    /// screen and therefore a relayout.
    pub fn set_page_gap(&mut self, gap: f64) {
        let gap = gap.clamp(0.0, 64.0).round();
        if gap == self.layout.gap {
            return;
        }
        self.keeping_place(|layout| layout.gap = gap);
        self.store.set(vec![("page_gap".into(), json!(gap as i64))]);
    }

    /// The panel's width, set from the field rather than dragged. Goes through
    /// the same relayout the drag's own ending does — see
    /// [`Viewer::finish_resize_sidebar`], and the comment there about why a
    /// whole number.
    pub fn set_sidebar_width(&mut self, width: f64) {
        let width = width
            .clamp(crate::sidebar::MIN_WIDTH, crate::sidebar::MAX_WIDTH)
            .round();
        if width == self.sidebar_width {
            return;
        }
        self.sidebar_width = width;
        let (window_width, height) = (self.window_width, self.layout.viewport.height);
        self.layout.viewport.width = -1.0;
        self.resize(window_width, height);
        self.store
            .set(vec![("sidebar_width".into(), json!(width as i64))]);
    }

    /// What the page on screen is actually drawn at, as a percentage.
    ///
    /// **Not the remembered zoom**, which in a fit mode is a number nobody is
    /// looking at: `zoomPercent` in `viewer.ts` exists for the same reason and
    /// is read in the same place — the stepper in the zoom menu starts from
    /// what is on the screen, because that is what somebody types over.
    pub fn zoom_percent(&self) -> f64 {
        self.layout
            .box_of(self.page().saturating_sub(1))
            .map(|page| page.scale / crate::layout::PDF_TO_CSS_UNITS * 100.0)
            .unwrap_or(self.layout.zoom * 100.0)
    }

    /// A fixed zoom, typed rather than stepped. The pair that never moves
    /// alone — see [`Viewer::zoom`].
    pub fn set_zoom(&mut self, zoom: f64) {
        // A zoom asked for outright ends any gesture that was running, or the
        // pages would hold a stale texture until its timer came round.
        self.zoom_token += 1;
        self.zoom_from = None;
        self.chosen.hold(false);
        let zoom = zoom.clamp(0.25, 6.0);
        self.keeping_place(|layout| {
            layout.fit = Fit::Actual;
            layout.zoom = zoom;
        });
        self.store.set(vec![
            ("zoom".into(), json!(zoom)),
            ("fit_mode".into(), json!(name_of(Fit::Actual))),
        ]);
    }

    /// Zoom by a proportion of where we are, which is what a pinch asks for.
    ///
    /// **A pinch and a ⌃-wheel cannot be steps on the ladder.** A trackpad
    /// sends a stream of small events and a mouse sends one large one, so
    /// stepping on each took 125% to 400% in one gesture — the app's own note,
    /// and its answer: each event is a proportion, and the proportions are
    /// applied as they arrive. Leaving a fit mode starts from the size the fit
    /// had reached, so the first squeeze changes the page by a little rather
    /// than jumping to whatever the remembered zoom was.
    ///
    /// The setting is written through `set_soon`, because a pinch produces one
    /// of these a frame and the file only needs the one it ends on.
    pub fn zoom_by(&mut self, factor: f64) {
        let current = if self.layout.fit == Fit::Actual {
            self.layout.zoom
        } else {
            self.layout
                .box_of(self.page() - 1)
                .map(|page| page.scale / crate::layout::PDF_TO_CSS_UNITS)
                .unwrap_or(1.0)
        };
        let next = (current * factor).clamp(0.25, 6.0);
        if (next - current).abs() < 0.0005 {
            return;
        }
        // **The gesture begins here and is what keeps the document on the
        // screen while it lasts.** Every page holds the texture it already
        // has and is stretched to the size the layout is asking for, so the
        // words grow under the reader's fingers instead of the document going
        // blank until they stop. `zoom_from` is the scale that was on when
        // the fingers went down, which is what the page key divides by to
        // stay the same key for the whole gesture — see
        // [`crate::page::Chosen::holding`] for what it costs and why the
        // alternative is not an option.
        if self.zoom_from.is_none() {
            self.zoom_from = Some(current);
            self.chosen.hold(true);
        }
        self.zoom_token += 1;
        self.keeping_place(|layout| {
            layout.fit = Fit::Actual;
            layout.zoom = next;
        });
        self.notice = format!("{:.0}%", next * 100.0);
        self.store.set_soon(vec![
            ("zoom".into(), json!(next)),
            ("fit_mode".into(), json!(name_of(Fit::Actual))),
        ]);
    }

    /// Which gesture is running, for the timer that ends it.
    pub fn zoom_gesture(&self) -> Option<u64> {
        self.zoom_from.map(|_| self.zoom_token)
    }

    /// How much smaller than the box a page is currently drawn, while a zoom
    /// gesture is under way: 1.0 when there is none.
    ///
    /// The key every page is built with is multiplied by this, so it is the
    /// size the page had when the gesture began — a number that does not move
    /// while the gesture does, which is the whole point of it.
    pub fn zoom_held_at(&self) -> f64 {
        match (self.zoom_from, self.layout.fit) {
            (Some(from), Fit::Actual) if self.layout.zoom > 0.0 => from / self.layout.zoom,
            _ => 1.0,
        }
    }

    /// The fingers stopped. Draw the pages again, at the size they are now.
    ///
    /// Guarded by the token for the reason [`Viewer::unflash_pill`] is: a
    /// gesture that went on past the settle is a second timer, and the first
    /// one must not end it.
    pub fn settle_zoom(&mut self, token: u64) {
        if self.zoom_token != token {
            return;
        }
        self.zoom_from = None;
        self.chosen.hold(false);
    }

    /// Whether a picture on a recoloured page is recoloured with it.
    ///
    /// It is in the *palette* rather than beside it — `resolve` takes it — so
    /// changing it is a new palette and every drawn page is stale. See
    /// `store.rs`, where the same flag is read.
    pub fn set_recolor_images(&mut self, on: bool) {
        self.store.set(vec![("recolor_images".into(), json!(on))]);
        self.generation += 1;
    }

    /// …and what it is set to, which the settings menu shows.
    pub fn recolor_images(&self) -> bool {
        self.store.flag("recolor_images")
    }

    /// Whether the page pill appears while the reader scrolls with the
    /// toolbar away. See the pill in `Reader` — `show_page_pill` in the app.
    pub fn page_pill(&self) -> bool {
        self.store.flag("show_page_pill")
    }

    pub fn set_page_pill(&mut self, on: bool) {
        self.store.set(vec![("show_page_pill".into(), json!(on))]);
    }

    /// Read `keys.toml` again, exactly as the launch did.
    ///
    /// A button rather than a watcher, and that is the app's reasoning
    /// unchanged: the config directory is written to several times a minute
    /// while somebody is scrolling — `remember_position` alone — so a watch on
    /// it would be answering its own writes. See `Store::keyboard`.
    pub fn reload_keys(&mut self) {
        let file = self.store.keyboard();
        let mut keymap = Keymap::build(crate::keymap::this_machine(), &file.bindings);
        keymap.problems = file
            .problems
            .into_iter()
            .chain(keymap.problems.drain(..))
            .collect();
        // The page redraws either way, so the line is what says it happened:
        // a file with nothing wrong in it redraws to exactly what was there.
        self.notice = match keymap.problems.len() {
            0 => "Keys reloaded.".to_string(),
            one => format!("Keys reloaded. {one} could not be used — below."),
        };
        self.keymap = keymap;
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

    /* ------------------------------------------------------------- notes */

    /// The notes on a page, asked for once and kept.
    pub fn notes_on(&self, index: usize) -> Rc<Vec<crate::render::Note>> {
        if let Some(known) = self.notes.borrow().get(&index) {
            return known.clone();
        }
        let notes = Rc::new(self.document.notes_of(index));
        self.notes.borrow_mut().insert(index, notes.clone());
        notes
    }

    /// The notes on a mounted page, placed in the same space as the links.
    pub fn note_areas(&self, page: usize) -> Vec<(Rect, crate::render::Note)> {
        let Some(index) = page.checked_sub(1) else {
            return Vec::new();
        };
        if self.layout.box_of(index).is_none() {
            return Vec::new();
        }
        self.notes_on(index)
            .iter()
            .map(|note| (self.layout.place_on(index, note.rect), note.clone()))
            .collect()
    }

    /// Open one, which is a window rather than a tooltip: a note can be a
    /// paragraph, and `title` is a tooltip's whole vocabulary.
    pub fn open_note(&mut self, page: usize, note: crate::render::Note) {
        self.note_open = Some((page, note));
    }

    pub fn close_note(&mut self) -> bool {
        self.note_open.take().is_some()
    }

    /// What has been typed into the password field so far.
    ///
    /// Kept here rather than in the field because the field does not hold it:
    /// Blitz renders `type="password"` as plain text, so what is on screen is
    /// a row of bullets and this is the string they stand for. See the field
    /// in [`Reader`], which is where the two are kept in step.
    pub fn type_password(&mut self, typed: &str) {
        if let Some(locked) = self.locked.as_mut() {
            locked.typed = typed.to_string();
        }
    }

    /// "Not now": the question is withdrawn and the reader is left with
    /// whatever they had — which at launch is the start screen.
    ///
    /// **Declining is not answering with an empty password**, which is the
    /// app's own hard-won note about pdf.js: a reader who has decided not to
    /// open this document must not be asked again on their way out of the
    /// question.
    pub fn stop_unlocking(&mut self) -> bool {
        self.locked.take().is_some()
    }

    /// The reader has answered. Answers whether the document opened, because
    /// the caller has the same bookkeeping to do as [`Self::open_here`]'s.
    pub fn unlock(&mut self) -> bool {
        let Some(locked) = self.locked.clone() else {
            return false;
        };
        self.open_here_with(&locked.path, Some(&locked.typed))
    }

    /// What the document says about itself, in the app's own order — see
    /// [`crate::render::PageSource::details`]. The name and the file are the
    /// reader's rather than the document's, so they are added here.
    pub fn details(&self) -> Vec<(String, String)> {
        let mut rows = self.document.details();
        if !self.document.path().is_empty() {
            rows.push(("File".into(), self.document.path().to_string()));
        }
        rows
    }

    /* --------------------------------------------------------- the editor */

    /// Begin a theme: a new one, or a copy of the one being worn.
    ///
    /// **The draft is installed as the live theme**, which is what makes the
    /// window around it the preview — `edit.preview` in `settings.ts` does
    /// the same. A built-in kept its own id here would be overwritten on
    /// save, so a copy is given an empty id and `theme::save` mints one.
    pub fn begin_theme(&mut self, from: Option<crate::theme::Theme>) {
        let draft = match from {
            Some(theme) if theme.built_in => crate::theme::Theme {
                id: String::new(),
                name: format!("{} copy", theme.name),
                built_in: false,
                ..theme
            },
            Some(theme) => theme,
            None => {
                let worn = self.store.theme().clone();
                crate::theme::Theme {
                    id: String::new(),
                    name: "New theme".into(),
                    built_in: false,
                    ..worn
                }
            }
        };
        self.editing = Some(draft);
        self.preview_draft();
    }

    /// What is in the draft, worn without being remembered — `wear_for_now`,
    /// which is the same door the harness's theme override uses.
    pub fn preview_draft(&mut self) {
        let Some(draft) = self.editing.clone() else {
            return;
        };
        let mut themes = self.store.themes().to_vec();
        // The draft stands in for the theme it is a version of, or is added
        // to the end when it is a new one.
        match themes
            .iter()
            .position(|theme| theme.id == draft.id && !draft.id.is_empty())
        {
            Some(at) => themes[at] = draft,
            None => themes.push(draft),
        }
        let at = themes.len() - 1;
        let at = self
            .editing
            .as_ref()
            .and_then(|draft| {
                themes
                    .iter()
                    .position(|theme| theme.id == draft.id && !draft.id.is_empty())
            })
            .unwrap_or(at);
        self.store.set_themes(themes);
        self.store.wear_for_now(at);
        self.chosen.set(self.store.palette());
        self.generation += 1;
    }

    /// Change one field of the draft and show it. The field's name is the
    /// theme file's own, which is what keeps this one function rather than
    /// seven.
    pub fn draft_set(&mut self, field: &str, value: String) {
        let Some(draft) = self.editing.as_mut() else {
            return;
        };
        let some = |value: String| (!value.trim().is_empty()).then_some(value);
        match field {
            "name" => draft.name = value,
            "text" => draft.text = value,
            "background" => draft.background = value,
            "accent" => draft.accent = some(value),
            "link" => draft.link = some(value),
            "selection_area" => draft.selection_area = some(value),
            "selection_text" => draft.selection_text = some(value),
            _ => return,
        }
        self.preview_draft();
    }

    pub fn draft_recolor(&mut self, on: bool) {
        if let Some(draft) = self.editing.as_mut() {
            draft.recolor = on;
        }
        self.preview_draft();
    }

    /// Put the draft down and go back to what was on before it.
    pub fn cancel_theme(&mut self) {
        self.editing = None;
        self.reload_themes();
    }

    /// Write the draft to its file, wear it for real, and close the editor.
    pub fn save_theme(&mut self) {
        let Some(draft) = self.editing.clone() else {
            return;
        };
        let dir = self.store.themes_dir().to_path_buf();
        match crate::theme::save(&dir, &draft) {
            Ok(saved) => {
                self.editing = None;
                self.reload_themes();
                let at = self
                    .store
                    .themes()
                    .iter()
                    .position(|theme| theme.id == saved.id);
                if let Some(at) = at {
                    self.set_theme(at);
                }
                self.notice = format!("Saved {}.", saved.name);
            }
            Err(said) => self.notice = said,
        }
    }

    /// Delete the theme being edited, which is only ever one that is on disk.
    pub fn delete_theme(&mut self) {
        let Some(draft) = self.editing.clone() else {
            return;
        };
        if draft.id.trim().is_empty() {
            self.cancel_theme();
            return;
        }
        let dir = self.store.themes_dir().to_path_buf();
        match crate::theme::delete(&dir, &draft.id) {
            Ok(()) => {
                self.editing = None;
                self.reload_themes();
                let replacement = self.store.replacement_for(&draft).unwrap_or(0);
                self.set_theme(replacement);
                self.notice = format!("Deleted {}.", draft.name);
            }
            Err(said) => self.notice = said,
        }
    }

    /// The themes directory, read again. What every one of the four above
    /// ends with, because each has changed what is in it or what is worn.
    fn reload_themes(&mut self) {
        let dir = self.store.themes_dir().to_path_buf();
        self.store.set_themes(crate::theme::load_all(&dir));
        let worn = self.store.theme_index();
        self.store.wear_for_now(worn);
        self.chosen.set(self.store.palette());
        self.generation += 1;
    }

    pub fn open_details(&mut self) {
        self.details_open = true;
    }

    pub fn close_details(&mut self) -> bool {
        std::mem::take(&mut self.details_open)
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
        // Where the press landed on the page, for [`Viewer::end_sweep`] to
        // ask what is under it when nothing was swept.
        self.pressed_on = Some((page, on.0, on.1));
        // A press anywhere puts away whatever the last one opened.
        self.mark_open = None;
        if again {
            self.sweep_word(page, on);
            return;
        }
        let spot = self.spot_on(index, on.0, on.1);
        self.selection = Some(Selection::at(spot));
        // A new sweep is a new passage; the swatches offered for the last one
        // go with it.
        self.markup_at = None;
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
        // **A click on a mark is a question about that mark**, and it is
        // asked here rather than on the press for one reason: a press is
        // also where a sweep begins, and a passage that is already marked is
        // exactly the passage a reader is most likely to want to select and
        // copy. So the mark answers only when nothing was swept — which is
        // what a click is. See [`Viewer::mark_open`].
        if self.selection.is_none() {
            if let Some((page, x, y)) = self.pressed_on {
                self.mark_open = self.mark_under(page, x, y);
            }
        }
        self.pressed_on = None;
    }

    /// The mark under a point on a page, if there is one, ready to be shown.
    ///
    /// A mark's quads are in the page's own points from its top left, which
    /// is the space `place_on` takes — the same trip `note_areas` and
    /// `highlights` make. The rectangle handed back is the one that was hit,
    /// so the popover opens under the line that was clicked rather than under
    /// the first line of a mark that runs over three.
    fn mark_under(&self, page: usize, x: f64, y: f64) -> Option<(usize, Rect, MarkKey, String)> {
        let index = page.checked_sub(1)?;
        self.layout.box_of(index)?;
        self.markup
            .iter()
            .filter(|mark| mark.page == page)
            .find_map(|mark| {
                mark.quads
                    .iter()
                    .map(|quad| self.layout.place_on(index, *quad))
                    .find(|area| {
                        x >= area.left
                            && x <= area.left + area.width
                            && y >= area.top
                            && y <= area.top + area.height
                    })
                    .map(|area| {
                        (
                            page,
                            area,
                            MarkKey::InFile(mark.page, mark.index),
                            mark.color.clone(),
                        )
                    })
            })
    }

    /// Put the mark's own popover away. `false` when it was not up, which is
    /// what lets Escape go on to the next thing it means — the same shape
    /// [`Viewer::close_markup`] has.
    pub fn close_mark(&mut self) -> bool {
        self.mark_open.take().is_some()
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
        self.markup_at = None;
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

    /* --------------------------------------------------------------- ink */

    /// Open the Sign window, or say why it cannot be.
    ///
    /// The refusals are asked *here* rather than at the moment the signature
    /// is placed, which is the opposite of what markup does and is right for
    /// the opposite reason: a mark is one gesture, so it may as well try and
    /// report; signing is a window, a drawing and a click, and finding out at
    /// the end of all three that the file is read-only is the reader's time
    /// spent on nothing.
    pub fn open_signing(&mut self) -> bool {
        if self.empty() {
            return false;
        }
        let standing = crate::sign::standing(
            self.document.path(),
            self.document.encrypted(),
        );
        if !standing.into_file {
            self.notice = format!("{} — so it cannot be signed.", standing.refused);
            return false;
        }
        self.menu = None;
        self.signing = Some(Signing {
            // The pad opens empty and the name opens empty with it. A default
            // of "Signature" is what `sign::save` falls back to, and putting
            // it in the field would mean a reader who types their own name has
            // to clear somebody else's word out of the way first.
            ..Default::default()
        });
        true
    }

    /// Take it down. `false` when it was not up, which is what lets Escape go
    /// on to the next thing it means.
    pub fn close_signing(&mut self) -> bool {
        self.signing.take().is_some()
    }

    /// Where the signatures are kept, which is the config directory this
    /// reader was given rather than the ambient one. See [`crate::sign::dir`].
    pub fn signatures(&self) -> Vec<crate::sign::Signature> {
        crate::sign::load_all(self.store.dir())
    }

    /// A press on the pad: begin a stroke, and note where the pad is.
    ///
    /// `on` is where the press landed inside the pad and `client` is where it
    /// landed in the window; the difference between them is the pad's top left
    /// corner, which is the one thing a later move needs and cannot ask for.
    /// `begin_sweep` records the same number for the same reason.
    pub fn draw_from(&mut self, on: (f64, f64), client: (f64, f64), pad: (f64, f64)) {
        let Some(signing) = self.signing.as_mut() else {
            return;
        };
        signing.pad = pad;
        signing.origin = (client.0 - on.0, client.1 - on.1);
        signing.drawing = true;
        signing.strokes.push(vec![[on.0, on.1]]);
    }

    /// The pointer moved in the window with the button down, while a stroke is
    /// running. Taken into the pad's own space and clamped to it: a hand that
    /// runs off the edge should stop at the edge rather than write a signature
    /// whose box is the whole window.
    pub fn draw_on_pad(&mut self, client: (f64, f64)) {
        let Some((origin, pad)) = self
            .signing
            .as_ref()
            .filter(|signing| signing.drawing)
            .map(|signing| (signing.origin, signing.pad))
        else {
            return;
        };
        self.draw_to((
            (client.0 - origin.0).clamp(0.0, pad.0),
            (client.1 - origin.1).clamp(0.0, pad.1),
        ));
    }

    /// One point onto the stroke that is running, in the pad's own space.
    pub fn draw_to(&mut self, at: (f64, f64)) {
        let Some(signing) = self.signing.as_mut() else {
            return;
        };
        if !signing.drawing {
            return;
        }
        let Some(stroke) = signing.strokes.last_mut() else {
            return;
        };
        // A point that has not moved is a point that says nothing, and a
        // signature drawn slowly would otherwise be a thousand of them. Half a
        // pixel is below what anybody can see and above what a jittering
        // trackpad reports while a finger rests.
        if stroke
            .last()
            .is_some_and(|last| (last[0] - at.0).abs() < 0.5 && (last[1] - at.1).abs() < 0.5)
        {
            return;
        }
        stroke.push([at.0, at.1]);
    }

    /// And the button let go. A stroke of one point is kept: a dot over an i
    /// is a stroke of one point, and [`crate::sign::place`] knows what to do
    /// with one.
    pub fn draw_done(&mut self) {
        if let Some(signing) = self.signing.as_mut() {
            signing.drawing = false;
        }
    }

    /// Throw away what is on the pad, leaving the window up. The thing anybody
    /// wants after the first attempt at signing with a trackpad.
    pub fn clear_pad(&mut self) {
        if let Some(signing) = self.signing.as_mut() {
            signing.strokes.clear();
            signing.drawing = false;
        }
    }

    /// Keep what is on the pad, and answer whether it was kept.
    ///
    /// **The strokes go across as drawn**, in the pad's own pixels, and
    /// `sign::save` normalises them. That is one division rather than two, and
    /// the two were the bug: dividing x by the pad's width and y by its height
    /// scales the axes differently, so a name written across a pad three times
    /// wider than it is tall arrived a third as wide as it was drawn. A pad
    /// pixel is square, so handing over pixels loses nothing — see
    /// [`crate::sign::Signature::trimmed`], which is the one place a signature
    /// is ever rescaled.
    pub fn keep_signature(&mut self) -> bool {
        let Some(signing) = self.signing.clone() else {
            return false;
        };
        if signing.strokes.iter().all(|stroke| stroke.is_empty()) {
            self.notice = "Draw a signature first, and this keeps it.".into();
            return false;
        }
        let drawn = crate::sign::Signature {
            name: signing.name.trim().to_string(),
            id: String::new(),
            strokes: signing.strokes.clone(),
        };
        match crate::sign::save(self.store.dir(), &drawn) {
            Ok(stored) => {
                self.notice = format!("Kept {}.", stored.name);
                // The pad is cleared rather than the window closed: keeping a
                // signature and using one are two things, and a reader who has
                // just drawn one very often wants to draw the initials too.
                self.clear_pad();
                if let Some(signing) = self.signing.as_mut() {
                    signing.name.clear();
                }
                true
            }
            Err(why) => {
                self.notice = why;
                false
            }
        }
    }

    /// Take one off the list, and off the disk.
    pub fn forget_signature(&mut self, id: &str) {
        if let Err(why) = crate::sign::forget(self.store.dir(), id) {
            self.notice = why;
        }
    }

    /// Every signature already in the document this window is showing.
    pub fn signed_here(&self) -> Vec<crate::sign::Placed> {
        self.document.signatures()
    }

    /// **Take a signature back out of the document.**
    ///
    /// The assessment that led to this feature named exactly one caveat worth
    /// deciding before shipping rather than after — *it cannot be removed
    /// afterwards* — and that was written about the app, where
    /// `Annotation.save()` is not overridden by any subtype and nothing
    /// already in a file can come out through `saveDocument()` at all. Here it
    /// is `FPDFPage_RemoveAnnot`, which is the same one call a highlight comes
    /// out through, so the caveat does not apply and there is no reason to
    /// ship the feature without it. A signature somebody put on the wrong page
    /// is the ordinary case, not the corner.
    ///
    /// Answers the scan to restart, as everything that reopens the document
    /// does.
    pub fn unsign(&mut self, page: usize, index: usize) -> Option<u64> {
        let path = self.document.path().to_string();
        self.document.release();
        let taken = crate::markup::remove(&path, page, index);
        let restarted = self.reopen(&path);
        match taken {
            Ok(()) => self.notice = format!("Signature taken off page {page}."),
            Err(refused) => self.notice = refused,
        }
        restarted
    }

    /// Choose a signature and go looking for somewhere to put it.
    ///
    /// The window closes, because the next question is *where* and the answer
    /// to it is on the page the window is covering.
    pub fn sign_with(&mut self, signature: crate::sign::Signature) {
        self.signing = None;
        self.placing = Some(signature);
        self.notice = "Click on the page where the signature should go.".into();
    }

    /// Put it down again unsigned. `false` when nothing was armed, which is
    /// what lets Escape go on to the next thing it means.
    pub fn put_down(&mut self) -> bool {
        if self.placing.take().is_some() {
            self.notice = "Signing cancelled.".into();
            true
        } else {
            false
        }
    }

    /// **The click that signs.** `on` is where the pointer landed on the page,
    /// in the screen pixels a page is laid out in; `unplace_on` takes it the
    /// rest of the way, through the crop and the rotation, into the page's own
    /// points.
    ///
    /// The point is where the *middle of the left edge* of the signature goes,
    /// not its top left, because what a reader is aiming at when they click on
    /// a line is the line — so the signature sits on it rather than hanging
    /// below it.
    ///
    /// Answers the scan to restart, when the find bar was up, which is the
    /// convention [`Viewer::document_changed`] sets for everything that
    /// reopens the document underneath the reader.
    pub fn sign_at(&mut self, page: usize, on: (f64, f64)) -> Option<u64> {
        let signature = self.placing.take()?;
        let index = page.checked_sub(1)?;
        let (x, y) = self.layout.unplace_on(index, on.0, on.1);
        let height = HAND_HEIGHT;
        let at = Rect {
            left: x,
            top: y - height / 2.0,
            width: 0.0,
            height,
        };
        let path = self.document.path().to_string();

        // Said once, before it happens, and only for the document that has
        // something to lose. See `sign::BREAKS_A_SIGNATURE`.
        let warning = if self.standing.signed && !self.said_rewrites {
            self.said_rewrites = true;
            format!(" {}", crate::sign::BREAKS_A_SIGNATURE)
        } else {
            String::new()
        };

        // Let go of the file before writing it and reopen whatever happens —
        // `mark_selection`'s own rule, and see
        // [`crate::render::PageSource::release`] for why it is not optional.
        self.document.release();
        let written = crate::sign::place(&path, page, at, &signature, crate::sign::INK);
        let restarted = self.reopen(&path);
        match written {
            Ok(()) => self.notice = format!("Signed on page {page}.{warning}"),
            // Nothing is kept beside the document here, which is where this
            // parts company with a mark. A highlight kept in the journal is
            // still a passage the reader marked and can be shown to them; a
            // signature that did not go into the file is not a signature at
            // all, and a list of names this reader had failed to write would
            // be a promise it cannot keep.
            Err(refused) => self.notice = refused,
        }
        restarted
    }

    /* ------------------------------------------------------------- markup */

    /// Read the document's own markup, ask the disk where a new mark could
    /// go, and bring the journal into line with both. Called at open and
    /// after every reload.
    fn read_markup(&mut self) {
        self.markup = self.document.markup();
        let path = self.document.path().to_string();
        self.standing = if path.is_empty() {
            crate::markup::Standing::default()
        } else {
            crate::markup::standing(&path, self.document.encrypted())
        };
        self.sync_journal();
    }

    /// The journal, rebuilt from what the file says.
    ///
    /// **This is where a recompile is survived**, and it is the one job the
    /// journal has that a document cannot do for itself. A paper rebuilt by
    /// LaTeX is a new file: every annotation in the old one went with it, and
    /// the words are usually still there. So each mark in the document is
    /// written down here with the passage it covers, and a reload that finds
    /// the annotations gone finds the quotes still written down — which is
    /// what [`Viewer::restore_markup`] then looks up again.
    ///
    /// The rule is the app's `syncMarkup`, said in this reader's terms:
    /// **everything is thrown away and rebuilt from the file**, and what
    /// survives is only what the file cannot carry — a mark beside a document
    /// that could not be written, and a mark a rebuilt document lost.
    ///
    /// A mark is the same mark as before when its colour and its words are
    /// the same. Not its page and not its index: a rebuilt paper moves a
    /// passage down the document, which is precisely the case this exists
    /// for, and an index shifts every time an earlier annotation is added or
    /// taken away. The folded quote is what `findQuote` matches on in the app
    /// and it is what a reader would recognise.
    fn sync_journal(&mut self) {
        let inside: Vec<(String, String, crate::markup::Mark)> = self
            .markup
            .iter()
            .map(|mark| {
                let quote = crate::markup::quote_under(&self.text_on(mark.page), &mark.quads);
                (mark.color.to_lowercase(), folded(&quote), mark.clone())
            })
            .collect();
        let mut next = Vec::new();
        for held in self.store.journal() {
            let known = inside.iter().any(|(colour, quote, _)| {
                *colour == held.color.to_lowercase() && *quote == folded(&held.quote)
            });
            if known {
                // In the file, so the file's own entry below is the one to
                // keep — this copy is last time's reading of it.
                continue;
            }
            // Not in the file. Either it never was, or it went with a
            // rebuild, and **`annotation_id` is what says so**: `None` is the
            // app's own mark for a highlight the document is not carrying,
            // and it is what the panel reads to know which rows to list and
            // which passages it can offer to put back.
            let mut lost = held.clone();
            lost.annotation_id = None;
            next.push(lost);
        }
        for (_, _, mark) in &inside {
            let height = self.document.size_of(mark.page.saturating_sub(1)).height;
            let quote = crate::markup::quote_under(&self.text_on(mark.page), &mark.quads);
            next.push(crate::store::Store::markup_entry(
                mark.page,
                crate::markup::flat(&mark.quads, height),
                &mark.color,
                &quote,
                Some(format!("{}:{}", mark.page, mark.index)),
            ));
        }
        self.store.set_journal(next);
    }

    /// The marks the journal is holding that are not in the document: the
    /// ones a rebuild lost, and the ones a document that cannot be written
    /// never took.
    pub fn markup_adrift(&self) -> Vec<&crate::library::Highlight> {
        self.store
            .journal()
            .iter()
            .filter(|held| held.annotation_id.is_none())
            .collect()
    }

    /// How many of those could be put back, which is what the offer says.
    pub fn restorable(&self) -> usize {
        if !self.standing.into_file {
            return 0;
        }
        self.markup_adrift()
            .iter()
            .filter(|held| !held.quote.trim().is_empty())
            .count()
    }

    /// Look the lost passages up again and write back the ones that are
    /// still there.
    ///
    /// **A guess, and never a thing that happens on its own.** It is a button
    /// in the panel because re-anchoring is a guess however good a one, and
    /// this reader does not write to somebody's file without being asked —
    /// which is the app's own sentence about the same button.
    ///
    /// The lookup starts on the page the passage used to be on and works
    /// outwards, because a rebuilt paper usually moves a passage by a page
    /// or two rather than to the other end of the book. What it does not
    /// find is left in the journal and counted out loud: a passage that was
    /// rewritten is not a passage that moved.
    pub fn restore_markup(&mut self) -> Option<u64> {
        let path = self.document.path().to_string();
        let wanted: Vec<crate::library::Highlight> = self
            .markup_adrift()
            .into_iter()
            .filter(|held| !held.quote.trim().is_empty())
            .cloned()
            .collect();
        if wanted.is_empty() || !self.standing.into_file {
            return None;
        }
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for held in &wanted {
            match self.find_quote(held.page as usize, &held.quote) {
                Some((page, quads)) => found.push((held.clone(), page, quads)),
                None => missing.push(held.clone()),
            }
        }
        if found.is_empty() {
            self.notice = format!(
                "{} could not be found in this document.",
                said_of(missing.len(), "passage", "passages"),
            );
            return None;
        }
        // The journal is written *before* the file, so that the reload the
        // write causes does not put back what is about to be put back. The
        // app's `restoreMarkup` does the same, for the same reason.
        let keeping: Vec<crate::library::Highlight> = self
            .store
            .journal()
            .iter()
            .filter(|held| {
                held.annotation_id.is_some() || missing.iter().any(|lost| lost.id == held.id)
            })
            .cloned()
            .collect();
        self.store.set_journal(keeping);
        self.document.release();
        let mut wrote = 0;
        let mut refused = String::new();
        for (held, page, quads) in &found {
            match crate::markup::add(&path, &[(*page, quads.clone())], &held.color, AUTHOR) {
                Ok(()) => wrote += 1,
                Err(why) => refused = why,
            }
        }
        let restarted = self.reopen(&path);
        self.notice = if missing.is_empty() {
            format!("{} put back.", said_of(wrote, "passage", "passages"))
        } else {
            format!(
                "{} put back. {} could not be found in this document.",
                said_of(wrote, "passage", "passages"),
                said_of(missing.len(), "passage", "passages"),
            )
        };
        if !refused.is_empty() {
            self.notice = refused;
        }
        restarted
    }

    /// Where a remembered passage is now, starting from the page it used to
    /// be on and working outwards.
    ///
    /// Through [`crate::search::fold`], which is the one thing this borrows
    /// from the search and is what makes it work at all: a passage that moved
    /// has very often been re-typeset on the way, so the ligatures and the
    /// soft hyphens are not the ones it had. The app's `findQuote` reaches
    /// for the same function for the same reason.
    fn find_quote(&self, was_on: usize, quote: &str) -> Option<(usize, Vec<Rect>)> {
        let wanted = folded(quote);
        if wanted.is_empty() {
            return None;
        }
        let pages = self.document.pages();
        let order = std::iter::once(was_on.clamp(1, pages.max(1)))
            .chain((1..=pages).filter(|page| *page != was_on));
        for page in order {
            let text = self.text_on(page);
            if text.chars.is_empty() {
                continue;
            }
            let folded = crate::search::fold(&text.chars, false);
            // Whitespace is flattened on both sides before the comparison,
            // because a passage that moved has very often been re-broken as
            // well as re-set: the quote was written down with single spaces
            // and the page it is now on may break it across a line. Each
            // character of the flattened text remembers where in the folded
            // text it came from, so the answer is still a range of the page's
            // own characters — the same trick `fold` itself plays one level
            // down.
            let mut flat = String::with_capacity(folded.text.len());
            let mut back = Vec::with_capacity(folded.text.len());
            for (at, &character) in folded.text.iter().enumerate() {
                if character.is_whitespace() {
                    if flat.ends_with(' ') || flat.is_empty() {
                        continue;
                    }
                    flat.push(' ');
                } else {
                    flat.push(character);
                }
                back.push(at);
            }
            let Some(at) = flat.find(&wanted) else {
                continue;
            };
            // Byte offset into character offset, which is what `back` — and
            // through it `origin` — is indexed by.
            let from = flat[..at].chars().count();
            let to = from + wanted.chars().count();
            let (start, end) = (
                *folded.origin.get(*back.get(from)?)?,
                back.get(to)
                    .and_then(|at| folded.origin.get(*at))
                    .copied()
                    .unwrap_or(text.chars.len()),
            );
            let quads = text.quads(start, end);
            if !quads.is_empty() {
                return Some((page, quads));
            }
        }
        None
    }

    /// Every mark the panel lists: the document's own first, in reading
    /// order, and then whatever the journal is holding.
    ///
    /// The two are one list because to a reader they are one thing — a
    /// passage they marked — and they are told apart by a word on the row
    /// rather than by a section of their own. Which is the app's answer as
    /// well: `showHighlights` marks the ones that are not in the document
    /// rather than putting them elsewhere.
    pub fn markup_rows(&self) -> Vec<MarkRow> {
        let mut rows: Vec<MarkRow> = self
            .markup
            .iter()
            .map(|mark| MarkRow {
                page: mark.page,
                color: mark.color.clone(),
                quote: crate::markup::quote_under(&self.text_on(mark.page), &mark.quads),
                key: MarkKey::InFile(mark.page, mark.index),
            })
            .collect();
        rows.sort_by(|a, b| {
            let (first, second) = (
                self.markup
                    .iter()
                    .find(|mark| MarkKey::InFile(mark.page, mark.index) == a.key)
                    .map(|mark| (mark.page, mark.begins())),
                self.markup
                    .iter()
                    .find(|mark| MarkKey::InFile(mark.page, mark.index) == b.key)
                    .map(|mark| (mark.page, mark.begins())),
            );
            first
                .partial_cmp(&second)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // Only the ones the file is not already showing: the journal mirrors
        // every mark in the document as well, so that a rebuild has the
        // quotes to look up — see [`Viewer::sync_journal`] — and listing both
        // copies would list every mark twice.
        rows.extend(self.markup_adrift().into_iter().map(|held| MarkRow {
            page: held.page as usize,
            color: held.color.clone(),
            quote: held.quote.clone(),
            key: MarkKey::Beside(held.id.clone()),
        }));
        rows
    }

    /// Put the colour popover up over the selection, or say why not.
    ///
    /// Answers whether it opened, so that the gesture that opens it by
    /// itself — letting go of a sweep — can stay silent while the key says
    /// something.
    pub fn open_markup(&mut self) -> bool {
        let Some(sweep) = self.selection.filter(|sweep| !sweep.is_empty()) else {
            return false;
        };
        // Where the sweep *ends*, which is where the reader's pointer is and
        // is what the app anchors to as well — the last rectangle of the
        // range, not the first.
        let (_, last) = sweep.span();
        let areas = self.selected_areas(last.page);
        let Some(area) = areas.last().copied() else {
            return false;
        };
        self.menu = None;
        self.markup_at = Some((last.page, area));
        true
    }

    /// Take it down again. `false` when it was not up, which is what lets
    /// Escape go on to the next thing it means.
    pub fn close_markup(&mut self) -> bool {
        self.markup_at.take().is_some()
    }

    /// The six colours the popover offers, in the order the swatches show
    /// them.
    ///
    /// Six independent settings rather than a list, because that is what
    /// `settings.rs` has — the table has no list type and a palette is not
    /// worth adding one for. The file this crate mounts is the app's, so
    /// these are the app's six keys and a reader who edits `markup_color_3`
    /// changes the third swatch in both.
    pub fn markup_colors(&self) -> Vec<String> {
        (1..=6)
            .map(|at| self.store.text(&format!("markup_color_{at}")))
            .filter(|colour| crate::markup::read_color(colour).is_some())
            .collect()
    }

    /// Mark what is selected, in this colour.
    ///
    /// The whole gesture, in the order it happens: the quads come off the
    /// selection, the document is let go of, the write goes in, and the
    /// document is reopened through the same path a recompile uses. There is
    /// no pending layer and no markup revision in any cache key, for the
    /// reason the app gives in `AGENTS.md`: saving immediately makes the
    /// deferred machinery unnecessary rather than merely delayed, and a
    /// reload rebuilds every cache there is.
    ///
    /// Answers the scan to restart, when the find bar was up — see
    /// [`Viewer::document_changed`], which is where that convention comes
    /// from.
    pub fn mark_selection(&mut self, color: &str) -> Option<u64> {
        self.markup_at = None;
        let Some(sweep) = self.selection.filter(|sweep| !sweep.is_empty()) else {
            // Two different sentences, and the difference is the point. A
            // scan has no text in it at all, so there is nothing this gesture
            // could ever mark and no amount of selecting will help.
            self.notice = if self.document.text_of(self.page() - 1).is_empty() {
                "There is no text in this document to mark.".into()
            } else {
                "Select something first, and this marks it.".into()
            };
            return None;
        };
        let runs: Vec<(usize, Vec<Rect>)> = sweep
            .pages()
            .filter_map(|page| {
                let text = self.text_on(page);
                let (from, to) = sweep.range_on(page, text.chars.len())?;
                let quads = text.quads(from, to);
                (!quads.is_empty()).then_some((page, quads))
            })
            .collect();
        if runs.is_empty() {
            self.notice = "There is nothing there to mark.".into();
            return None;
        }
        let quote = self.selected_text();
        let path = self.document.path().to_string();
        if !self.standing.into_file {
            return self.keep_beside(&runs, color, &quote);
        }
        // Signed, and said once: it is their document, and a rewrite is
        // exactly the thing a signature is there to detect. Asked rather than
        // refused, which is the app's decision made again.
        let warning = if self.standing.signed && !self.said_standing {
            self.said_standing = true;
            " This document is signed, and marking it breaks the signature."
        } else {
            ""
        };
        // **Let go of the file before writing it, and reopen whatever
        // happens.** See [`crate::render::PageSource::release`]: pdfium keeps
        // the document's file open for as long as the document lives, and on
        // Windows nothing can rename over it or truncate it while it does.
        // The reopen is unconditional because a released document draws
        // nothing — so a write that fails must still leave the reader looking
        // at their document.
        self.document.release();
        let written = crate::markup::add(&path, &runs, color, AUTHOR);
        self.selection = None;
        let restarted = self.reopen(&path);
        self.show_markup_panel();
        match written {
            Ok(()) => self.notice = format!("Marked.{warning}"),
            Err(refused) => {
                // The file is as it was, so there is nothing to put back. The
                // mark is kept beside the document instead, which is the
                // answer a read-only file gets and for the same reason: a
                // passage the reader marked is not lost because the disk said
                // no.
                self.keep_beside(&runs, color, &quote);
                self.notice = format!("{refused} The mark is kept beside the document.");
            }
        }
        restarted
    }

    /// Keep a mark beside the document rather than in it, and say so once.
    fn keep_beside(
        &mut self,
        runs: &[(usize, Vec<Rect>)],
        color: &str,
        quote: &str,
    ) -> Option<u64> {
        for (page, quads) in runs {
            let height = self.document.size_of(page.saturating_sub(1)).height;
            self.store
                .keep_markup(*page, &crate::markup::flat(quads, height), color, quote);
        }
        self.selection = None;
        self.show_markup_panel();
        let why = self.standing.refused.clone();
        self.notice = if self.said_standing || why.is_empty() {
            "Marked, beside the document.".into()
        } else {
            self.said_standing = true;
            format!("Marked — but {why}, so it is kept beside the document rather than in it.")
        };
        None
    }

    /// A document with a passage marked in it has something to show in the
    /// Contents panel after all, which is [`Viewer::mark_page`]'s own rule
    /// and is as narrow here as it is there: only when the panel is already
    /// open, and only when the tab it is on has nothing on it. Taking a
    /// reader off the thumbnails they were looking at, for a list they can
    /// see is there, is the panel arguing.
    fn show_markup_panel(&mut self) {
        if self.sidebar_open && self.tab == Tab::Pages && self.headings.is_empty() {
            self.tab = Tab::Contents;
        }
    }

    /// Take one mark out, wherever it is being kept.
    ///
    /// **A mark in the document comes out of the document**, which is the
    /// sentence this whole item is about: `FPDFPage_RemoveAnnot`, a reopen,
    /// and it is gone from the file for every reader of it. The app cannot
    /// say that — see [`crate::markup`].
    pub fn remove_markup(&mut self, key: &MarkKey) -> Option<u64> {
        match key {
            MarkKey::Beside(id) => {
                self.store.drop_markup(id);
                self.notice = "Mark removed.".into();
                None
            }
            MarkKey::InFile(page, index) => {
                let path = self.document.path().to_string();
                // **The journal has to be told first**, or the reload cannot
                // tell "the reader took this off" from "a rebuild lost it" —
                // the mark is gone from the file either way, and the second
                // reading offers it straight back. The app hit exactly this
                // and fixed it exactly here, ahead of the write rather than
                // after it.
                let entry = self
                    .markup
                    .iter()
                    .find(|mark| mark.page == *page && mark.index == *index)
                    .map(|mark| {
                        (
                            mark.color.to_lowercase(),
                            folded(&crate::markup::quote_under(
                                &self.text_on(mark.page),
                                &mark.quads,
                            )),
                        )
                    });
                if let Some((colour, quote)) = entry {
                    let keeping: Vec<crate::library::Highlight> = self
                        .store
                        .journal()
                        .iter()
                        .filter(|held| {
                            held.color.to_lowercase() != colour || folded(&held.quote) != quote
                        })
                        .cloned()
                        .collect();
                    self.store.set_journal(keeping);
                }
                self.document.release();
                let taken = crate::markup::remove(&path, *page, *index);
                let restarted = self.reopen(&path);
                self.notice = match taken {
                    Ok(()) => "Mark removed.".into(),
                    Err(refused) => refused,
                };
                restarted
            }
        }
    }

    /* ------------------------------------------------- what a page is called */

    /// Whether this document numbers its pages its own way.
    pub fn has_labels(&self) -> bool {
        !self.labels.is_empty()
    }

    /* ---------------------------------------------------- the toolbar peek */

    /// **With the toolbar away, the top edge of the window stands in for
    /// it.** `wireToolbarPeek` in `main.ts`: a handle drops in when the
    /// pointer arrives at the edge and puts the bar back, and nothing is on
    /// screen until somebody reaches for it. Without this the only way back
    /// was the key the notice names, which is a sentence that has to be read
    /// and remembered.
    ///
    /// In full screen the top of the window is not reliably ours — reaching
    /// for it slides the system's own bars over that band — so the handle
    /// answers from further down and sits below them, which is the app's
    /// reasoning and its two numbers.
    pub fn reach_for_toolbar(&mut self, y: f64) {
        if self.toolbar || self.presenting {
            self.peek = false;
            return;
        }
        let reach = if self.full_screen { 46.0 } else { 8.0 };
        if y <= reach {
            self.peek = true;
        } else if y > PEEK_KEEP {
            // Going away while it is being reached for is the one thing it
            // must not do, so it stays for a good way below where it appears.
            self.peek = false;
        }
    }

    /// Whether a move at this height would change anything, asked before the
    /// signal is written to: `onmousemove` fires on every move in the window
    /// and a write is a render. The same guard `resize_from` gets.
    pub fn peek_changes(&self, y: f64) -> bool {
        if self.toolbar || self.presenting {
            return self.peek;
        }
        let reach = if self.full_screen { 46.0 } else { 8.0 };
        (y <= reach) != self.peek && (y <= reach || y > PEEK_KEEP)
    }

    pub fn peeking(&self) -> bool {
        self.peek
    }

    /* ------------------------------------------------------- the page pill */

    /// The reader scrolled: put the pill up, if there is any reason to.
    ///
    /// `onScroll` in `main.ts`, and its two conditions: **only while the
    /// toolbar is away**, because with the bar up the same number is already
    /// on screen, and only if the reader wants it. Returns the token of the
    /// flash, which is what the thread that takes it down carries.
    pub fn flash_pill(&mut self) -> Option<u64> {
        if self.toolbar || self.presenting || self.empty() || !self.page_pill() {
            return None;
        }
        self.pill_token += 1;
        self.pill_up = true;
        Some(self.pill_token)
    }

    /// …and the end of that second, if nothing has happened since.
    pub fn unflash_pill(&mut self, token: u64) {
        if self.pill_token == token {
            self.pill_up = false;
        }
    }

    pub fn pill_shown(&self) -> bool {
        self.pill_up
    }

    /// What the pill says: the page, and how many there are. A document that
    /// numbers its own pages says both — "iii (3 of 400)" — because the label
    /// is what is printed on the page and the position is what says how far
    /// through it is.
    pub fn pill_text(&self) -> String {
        let (page, pages) = (self.page(), self.pages());
        if pages == 0 {
            return String::new();
        }
        if self.has_labels() {
            format!("{} ({page} of {pages})", self.label(page))
        } else {
            format!("{page} of {pages}")
        }
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
        if let Some(page) = self.page_for_label(&typed) {
            self.go_to_page(page);
            return;
        }
        // **A number past the end is the last page, not a complaint.** This
        // is ⌘9 in a browser with four tabs open: somebody who asks for page
        // 900 of an 800-page book has asked to go as far as it goes, and the
        // window they typed into is gone by the time the notice arrives, so
        // the sentence is all they get for it. Only text that is neither a
        // label nor a number is worth a word — "xii" in a document numbered
        // 1, 2, 3 is a reader looking at the wrong book, and there is nowhere
        // to clamp it to.
        if let Ok(number) = typed.trim().parse::<usize>() {
            let pages = self.pages();
            if pages > 0 {
                self.go_to_page(number.clamp(1, pages));
            }
            return;
        }
        self.notice = format!("There is no page {} in this document", typed.trim());
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
        // No page to put a pin in. `open_find`'s note again, and this is the
        // third of the four keys the app leaves unflagged that are plainly
        // about a document — `mark`, `find`, `find-next`, `find-previous`.
        // Without it ⌘⇧B on the start screen says "Marked page 0", and says it
        // about the document whose entry the store has just let go of.
        if self.empty() {
            return;
        }
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
        // Nothing to search. **This is one line more than the app has**, and
        // it is a deliberate difference rather than an oversight either way:
        // `find` is not `needsDocument` in `keys.ts`, so ⌘F on the app's own
        // start screen puts up a bar that will never find anything, with a
        // placeholder reading "Search this document" over a window that has
        // none. The flag is left agreeing with the app — `tests/keys.rs`
        // checks every one of them — because that table is a port and this is
        // a judgement about one key.
        if self.empty() {
            return;
        }
        // The colour popover is about a passage, and somebody opening the
        // find bar has moved on from it. Closing it here rather than leaving
        // Escape to do it keeps that key's order the one it claims to be:
        // outward, in the order the reader arrived — and the bar was arrived
        // at second.
        self.markup_at = None;
        self.find_open = true;
        if self.sidebar_open && !self.search.query().is_empty() {
            self.tab = Tab::Results;
        }
    }

    /// Show the list behind the count.
    ///
    /// **The count in the find bar is the way through to the results**, which
    /// is `el.findStatus`'s click handler in `main.ts` and the reasoning it
    /// carries: "3 of 128" answers *is it in here* and not *which one did I
    /// mean*, and the second question is the one somebody searching a long
    /// document is usually asking. An empty count is not a way to anything, so
    /// it does nothing and does not look pressable either.
    pub fn show_results(&mut self) {
        if self.search.state().total == 0 {
            return;
        }
        if !self.sidebar_open {
            self.set_sidebar(true, false);
            self.results_borrowed = true;
        }
        self.tab = Tab::Results;
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
        // A panel that came up to hold the results goes back down with them,
        // so that one Escape undoes the whole of what one search did. See
        // `results_borrowed`, which is what keeps this from shutting a panel
        // the reader had open before any of this started.
        if self.results_borrowed {
            self.results_borrowed = false;
            self.set_sidebar(false, false);
        }
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
        self.offered_results = false;
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
        self.show_the_matches();
        self.search.wants().is_some()
    }

    /// **A search puts its results in the panel, as soon as there are any.**
    ///
    /// The count in the bar answers *is it in here* and the list answers
    /// *which one did I mean*, and the second question is the one somebody
    /// searching a book is usually asking — so the list is not kept behind a
    /// button here. It arrives with the first match rather than with the bar,
    /// which is what keeps a search for something that is not in the document
    /// from opening a panel to say so; [`Viewer::show_results`] is the same
    /// door, opened by hand, for the reader who shut the panel and wants it
    /// back.
    ///
    /// The panel is *borrowed*: it goes back down with the bar that opened
    /// it, and one the reader had open before any of this is one they keep.
    /// See [`Viewer::close_find`].
    fn show_the_matches(&mut self) {
        // Once per search, and that is what the flag is for rather than
        // tidiness: a scan is dozens of slices, and a panel reopened on every
        // one of them is a panel the reader cannot close while the book is
        // still being read.
        if self.offered_results || !self.find_open || self.search.state().total == 0 {
            return;
        }
        self.offered_results = true;
        if !self.sidebar_open {
            self.set_sidebar(true, false);
            self.results_borrowed = true;
        }
        self.tab = Tab::Results;
    }

    /// Move to the next match, or the one before, and go there.
    pub fn step_match(&mut self, forwards: bool) {
        // Nothing to step through, and nothing to say about it: "No matches"
        // on a start screen is an answer to a question nobody asked. The
        // second half of `open_find`'s note — the app leaves ⌘G, ⌘⇧G and ⌘F
        // unflagged, and all three of them are about a document.
        if self.empty() {
            return;
        }
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
        // See `set_zoom`: choosing a fit mode ends a gesture too.
        self.zoom_token += 1;
        self.zoom_from = None;
        self.chosen.hold(false);
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
        let worn = self.store.wear(index);
        if worn.name.is_empty() {
            return;
        }
        // Every mounted page reads this on its next paint, and the next paint
        // is the frame this change causes. A page already on the GPU is
        // recoloured by a compute pass over it rather than drawn again, which
        // is the whole difference from `keyFor()` carrying the theme.
        self.chosen.set(self.store.palette());
        // Three things could be said and one line says one of them, so they
        // are in the order of how much the reader needs to know: a colour the
        // renderer cannot read is a theme that is not going to look like
        // itself; having just been taken off following the machine is a
        // switch that has moved without being touched; and otherwise the name
        // of what is now being read in.
        self.notice = match (self.store.complaint.clone(), worn.stopped_following) {
            (Some(complaint), _) => complaint,
            (None, true) => FOLLOWING_OFF.into(),
            (None, false) => worn.name,
        };
        self.generation += 1;
    }

    /// ⌘D, and the switch on the Appearance page.
    ///
    /// The theme moves to the other half of the pair the reader has already
    /// chosen — see [`Store::other_half`] — and going through `set_theme`
    /// rather than around it is what makes the keystroke stop the app
    /// following the machine, which is right: pressing ⌘D at noon is a reader
    /// saying they want the dark theme *now*, and following would take it
    /// away again at the machine's next word.
    pub fn toggle_dark(&mut self) {
        self.set_dark(!self.store.dark_now());
    }

    pub fn set_dark(&mut self, on: bool) {
        match self.store.other_half(on) {
            Some(index) => self.set_theme(index),
            // It takes deleting half the themes directory to reach this, and
            // saying nothing would read as a broken key.
            None => {
                self.notice = format!(
                    "There is no {} theme in your themes folder.",
                    if on { "dark" } else { "light" }
                );
            }
        }
    }

    /// The machine's light or dark, at startup and whenever it changes.
    ///
    /// `followSystemTheme` in `main.ts`, and called at startup for the reason
    /// that file gives: the machine can have changed its mind while the app
    /// was shut. What is *not* the app's is `None` — a webview always answers
    /// this question and a window does not, so a platform that will not say
    /// leaves the reader wearing what they chose. See [`Store::outside`].
    pub fn follow_system(&mut self, dark: Option<bool>) {
        self.store.set_outside(dark);
        if let Some(index) = self.store.following() {
            self.set_theme(index);
        }
    }

    /// The switch itself. Turning it on takes effect at once rather than at
    /// the machine's next change, because a switch that says "follow the
    /// system" and leaves a light theme up on a dark machine has not been
    /// believed by anybody.
    pub fn set_follow_system(&mut self, on: bool) {
        self.store
            .set(vec![("follow_system_theme".into(), serde_json::json!(on))]);
        if on {
            if let Some(index) = self.store.following() {
                self.set_theme(index);
            }
        }
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
            let worn = self.store.wear(index);
            self.chosen.set(self.store.palette());
            self.notice = format!("{} is gone. Now reading in {}.", before.name, worn.name);
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
        let restarted = self.reopen(path);
        // Still asked, because asking is what *renames* the document — the
        // toolbar takes its title from what this writes. What is no longer
        // done with the answer is announce it. A reader watching a paper
        // recompile sees the page redraw and the title change, which is the
        // whole of the news; a line saying "Reloaded — the document changed
        // on disk" tells somebody who did not know what a reload is that
        // something they did not do has happened to their file, which is a
        // sentence that can only worry them. See `reopen`, which had already
        // reached this conclusion for the other caller.
        let _ = self.store.renamed(&self.document.title());
        restarted
    }

    /// The document on disk, read again, with the reader left where they
    /// were — and nothing said about it.
    ///
    /// Split out of [`Viewer::document_changed`] because there are now two
    /// reasons to do this and they have different things to say afterwards:
    /// a compiler rewrote the paper, or this reader just marked a passage in
    /// it. The work is identical, and the sentence is not — "Reloaded" is the
    /// wrong answer to a reader who pressed a colour.
    fn reopen(&mut self, path: &str) -> Option<u64> {
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
        self.read_markup();
        self.links.borrow_mut().clear();
        self.notes.borrow_mut().clear();
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
        if self.find_open {
            let query = self.find_query.clone();
            self.search.forget();
            self.find(&query)
        } else {
            None
        }
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
        self.open_here_with(path, None)
    }

    /// The same, with the password for a document that wants one.
    ///
    /// **Locked is not an error here, it is a question**, and this is the one
    /// place that turns it into one: a document that will not open without a
    /// password puts [`Locked`] up and leaves this window showing whatever it
    /// had, exactly as any other refusal does. The reader answers, the answer
    /// comes back through [`Self::unlock`], and this runs again with it.
    ///
    /// Whether the sentence over the field says "it needs a password" or
    /// "that one was not right" is decided here rather than by pdfium, which
    /// reports both as `FPDF_ERR_PASSWORD`: the difference is whether this
    /// call supplied one.
    fn open_here_with(&mut self, path: &str, password: Option<&str>) -> bool {
        if path == self.document.path() {
            self.notice = "That document is already open here.".into();
            return false;
        }
        let opened = match crate::render::open_with(path, password) {
            Ok(document) => document,
            Err(crate::render::Refusal::Locked) => {
                self.locked = Some(Locked {
                    path: path.to_string(),
                    typed: String::new(),
                    wrong: password.is_some(),
                });
                return false;
            }
            Err(refused) => {
                self.locked = None;
                self.notice = format!("Could not open that document: {refused}");
                return false;
            }
        };
        self.locked = None;
        // Where the reader was in the document being put down, written before
        // the store stops pointing at it. `remember` hands it to the scribe,
        // which keeps one place per document — so this cannot be skipped on
        // the grounds that the scroll has not moved since the last one.
        self.store.remember(self.layout.anchor(self.scroll_top));
        let declared = opened.title();
        self.document = opened;
        let place = self.store.opened(path, &declared);
        self.take_up(place);
        true
    }

    /// The document is put down and this window is showing none.
    ///
    /// **The gesture the app calls Close**, and it is not the same as closing
    /// the window: the window stays, the toolbar keeps the things that are
    /// not about a document, and what is in front of the reader is the start
    /// screen. In the app it is also the one gesture that empties the restore
    /// list, which is `AGENTS.md`'s own distinction — a window that goes
    /// because the app is quitting was open at the end, and a document the
    /// reader put down is one they have finished with. Here that is the
    /// caller's to say, because the list belongs to the process (see
    /// [`Ask::Showing`] with an empty path).
    ///
    /// Everything [`Viewer::open_here`] clears is cleared, for the same
    /// reasons, and the document put in its place is [`crate::render::Nothing`]
    /// — see there for why this is a document of no pages rather than no
    /// document at all.
    pub fn close_document(&mut self) {
        if self.empty() {
            return;
        }
        // Where they got to, written while the store still points at the file
        // it is about. Exactly as `open_here` does it, and for the same
        // reason: this is the last moment either half is true.
        self.store.remember(self.layout.anchor(self.scroll_top));
        // **And written now rather than eventually**, which is the one place
        // in this reader that waits for the scribe. Everywhere else the delay
        // is the whole design — a place arrives on every wheel event and the
        // last one wins — but the screen this is about to put up says, on its
        // own first row, where the reader stopped in the document they are
        // putting down. Reading that off the file before the file has caught
        // up shows the page they were on the last time they *opened* it, and
        // the number is stale exactly when it is most looked at.
        crate::store::flush();
        self.document = crate::render::nothing();
        self.store.closed();
        self.take_up(None);
        // Nothing to say. The screen that arrives says what it is.
        self.notice = String::new();
    }

    /// Whether this window has a document in it.
    ///
    /// One predicate, asked in the two places it decides something: what the
    /// toolbar carries, and whether the body is the document or the start
    /// screen. Everything else in this file goes on being written for a
    /// document, because a document of no pages answers every question
    /// already — see [`crate::render::Nothing`].
    pub fn empty(&self) -> bool {
        self.document.pages() == 0
    }

    /// The last few documents read, for the start screen and the Open menu,
    /// with the one already open left out.
    ///
    /// Left out because reopening it here would be a no-op, which is the app's
    /// own reasoning about the same list: opening it in a *second* window is
    /// what its own title menu is for. Six, which is the app's number for the
    /// start screen; the menu takes what it wants of them.
    pub fn recents(&self) -> Vec<crate::store::Recent> {
        let open = self.document.path();
        self.store
            .recents()
            .into_iter()
            .filter(|entry| entry.path != open)
            .take(6)
            .collect()
    }

    /// Take a document off the recently-read list, and off the shelf.
    pub fn forget(&mut self, path: &str) {
        self.store.forget(path);
        self.generation += 1;
    }

    /// Everything a window has to put down when the document under it changes,
    /// and everything it has to pick up again.
    ///
    /// Shared by [`Viewer::open_here`] and [`Viewer::close_document`] because
    /// the list is identical and the list is the part that is easy to get
    /// wrong — a cache left pointing into the document that was there a moment
    /// ago is a rectangle drawn over the wrong page, and it took closing a
    /// document to notice that the two paths had to agree.
    fn take_up(&mut self, place: Option<crate::layout::Anchor>) {
        self.headings = self.document.outline();
        self.labels = self.document.labels();
        // A different document has different markup, and its own answer to
        // whether it can be written — and `said_standing` goes with it,
        // because "said once" means once per document.
        self.read_markup();
        self.said_standing = false;
        self.markup_at = None;
        self.links.borrow_mut().clear();
        self.notes.borrow_mut().clear();
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
        self.go_to(place.unwrap_or(crate::layout::Anchor {
            page: 1,
            offset: 0.0,
        }));
        self.relay_column();
        self.revealed = false;
        self.reveal_thumb();
        self.notice = String::new();
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
    /// The notes somebody else left on this page. See [`crate::render::Note`].
    notes: Vec<(Rect, crate::render::Note)>,
    /// What the reader has swept over, on this page, in the same space as the
    /// other two. See [`crate::select`].
    selected: Vec<Rect>,
    /// Where the colour popover goes, when it is over this page. See
    /// [`Viewer::markup_at`].
    swatches: Option<Rect>,
    /// The size the page's texture is keyed on, which is its box except under
    /// a zoom gesture. See [`Viewer::zoom_held_at`].
    drawn: (f64, f64),
    /// The mark the reader clicked on, when it is on this page. See
    /// [`Viewer::mark_open`].
    mark: Option<(Rect, MarkKey, String)>,
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
pub fn Reader(
    document: Handle,
    chosen: Chosen,
    config: Config,
    /// A document this window is to ask for the password to, if there is one.
    ///
    /// A prop rather than a message down the mailbox because it is true
    /// *before* the first frame: a window made on a locked document has never
    /// had anything else in it, and a question posted to a window that has not
    /// rendered yet is a question about arrival order. See `Session::window_on`.
    #[props(default)]
    asking: Option<String>,
) -> Element {
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
    // …and what it says about light and dark, which is asked in the same
    // breath and for the same reason. See [`Appearance`].
    let appearance = use_hook(|| {
        dioxus_core::try_consume_context::<Appearance>().unwrap_or_else(Appearance::unknown)
    });
    let mut viewer = use_signal(|| {
        let mut store = Store::at(&config.dir);
        if let Some(index) = config.theme {
            store.wear_for_now(index);
        }
        let mut viewer = Viewer::new(document.0.clone(), chosen.clone(), store);
        // **Before the first frame, like the viewport above it**, and for the
        // reader's sake rather than the renderer's: a machine in dark mode
        // must never see a white page on the way in, which is the sentence
        // `followSystemTheme` in `main.ts` is written under. It costs one
        // question of the window and, on the ordinary launch where nothing
        // has changed, nothing else.
        viewer.follow_system(appearance.get());
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
        // And the question, if this window was made to ask one.
        if let Some(path) = asking.clone() {
            viewer.locked = Some(Locked {
                path,
                typed: String::new(),
                wrong: false,
            });
        }
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
    // And what hands the document to something that prints. See [`Printer`]:
    // the default is the platform's own program, and a harness writes the
    // path down instead of opening one.
    let printer = use_hook(|| {
        dioxus_core::try_consume_context::<Printer>().unwrap_or_else(Printer::to_the_system)
    });
    // …and what shows the document where it lives. See [`Reveal`]: the default
    // asks the platform's file manager, and a harness writes the path down.
    let reveal = use_hook(|| {
        dioxus_core::try_consume_context::<Reveal>().unwrap_or_else(Reveal::to_the_system)
    });
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
            Pick::from_the_system(
                dioxus_core::try_consume_context::<Arc<dyn blitz_traits::shell::ShellProvider>>(),
                dioxus_core::try_consume_context::<crate::emit::Post>().unwrap_or_default(),
            )
        })
    });

    // The window, for the switch in the settings menu — the keyboard's own
    // copy is moved into `on_key` below, and asking the window to go full
    // screen is the same `Ask` either way.
    let full_screen_frame = frame.clone();

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
        let printer = printer.clone();
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
                    // **An action about a document does nothing when there is
                    // none.** `keys.ts` has carried `needsDocument` since the
                    // keyboard was ported and this reader has carried
                    // `needs_document` beside it, unread, because there was no
                    // window without a document in it to read it for. The
                    // start screen is what makes it mean something: without
                    // this, `j` on the start screen scrolls a layout of no
                    // pages, ⌘F puts up a find bar over nothing, and ⌘⇧H
                    // offers to highlight a selection that cannot exist.
                    if crate::keymap::needs_document(action) && viewer.read().empty() {
                        return;
                    }
                    perform(viewer, action, screen, &frame, &clip, &pick, &printer);
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
    let (watching, notifying) = use_hook(|| {
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
        let watching_appearance = appearance.clone();
        let opening = frame.clone();
        spawn(async move {
            loop {
                let news = listening.next().await;
                match news.event.as_str() {
                    // Four seconds after something was said, said by a
                    // thread of its own. It clears the line only if the line
                    // still carries what it was started for.
                    "notice-timeout" => {
                        let said = news.payload.as_str().unwrap_or_default();
                        if viewer.read().notice == said {
                            viewer.write().notice.clear();
                        }
                    }
                    // A second after the reader stopped scrolling, and only
                    // if nothing has scrolled since — see `Viewer::flash_pill`.
                    "pill-timeout" => {
                        if let Some(token) = news.payload.as_u64() {
                            viewer.write().unflash_pill(token);
                        }
                    }
                    // The fingers stopped moving. See [`Viewer::settle_zoom`].
                    "zoom-settled" => {
                        if let Some(token) = news.payload.as_u64() {
                            viewer.write().settle_zoom(token);
                        }
                    }
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
                    // A document is over the window, and whether it is one
                    // this reader would open. Both answers are worth having:
                    // a hint that says "drop to open" over a folder is a
                    // promise nothing keeps.
                    "drag-over" => {
                        let takeable = news.payload.as_bool().unwrap_or(true);
                        viewer.write().dragging = Some(takeable);
                    }
                    "drag-left" => viewer.write().dragging = None,
                    // Let go on something that is not a document. The app's
                    // own sentence, said on the app's own line.
                    "drag-refused" => {
                        let mut held = viewer.write();
                        held.dragging = None;
                        held.notice = "That is not a PDF.".into();
                    }
                    // A document handed to this window by the process: a
                    // second launch, "Open with", a double-click in the
                    // Finder. It arrives here rather than at a new window
                    // because this window is showing nothing — see
                    // `Desk::hand_over` — and the bookkeeping afterwards is
                    // ⌘O's own, because this is ⌘O with somebody else
                    // choosing the file.
                    "open-document" => {
                        let path = news.payload.as_str().unwrap_or_default().to_string();
                        viewer.write().dragging = None;
                        if !path.is_empty() && viewer.write().open_here(&path) {
                            let title = viewer.read().store.title().to_string();
                            opening.ask(Ask::Showing { path, title });
                        }
                    }
                    // The same document, through the other door: the picker
                    // was opened by "Open document in new window…", so what
                    // it chose goes beside this window rather than into it.
                    // See `Pick` — a picker cannot answer where it was asked,
                    // so which door it was is carried in the event's name.
                    "open-document-beside" => {
                        let path = news.payload.as_str().unwrap_or_default().to_string();
                        if !path.is_empty() {
                            opening.ask(Ask::NewWindowOn(path));
                        }
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
                    // The machine went light or dark while the reader was
                    // reading. Nothing carries the answer — the event says
                    // only that there is a new one, exactly as a resize does,
                    // and it is asked of the window. See `Shell::on_theme`.
                    // Two fingers, moving apart or together. macOS gives the
                    // change since the last event as a fraction, so the
                    // gesture's whole scale is the product of them and each
                    // one is a proportion to zoom by. See `Viewer::zoom_by`.
                    "pinched" => {
                        if let Some(delta) = news.payload.as_f64() {
                            viewer.write().zoom_by(1.0 + delta);
                        }
                    }
                    "appearance-changed" => {
                        viewer.write().follow_system(watching_appearance.get());
                    }
                    // Nothing else is emitted, and an unknown event is a
                    // version of this crate that has not caught up rather
                    // than something to report.
                    _ => {}
                }
            }
        });
        // Held for the life of the reader. Dropping it stops nothing — see
        // `Config::watch` — but this is where it will be asked to. The
        // mailbox comes back out with it because the notice timer below
        // wants to post into it, and a hook cannot be declared inside
        // another hook's initialiser.
        (held, post)
    });
    // Read so that the handle is plainly alive rather than plainly unused.
    let _ = watching.is_some();

    // **The notice puts itself away after four seconds**, which is what
    // `ui.notice` in the app does with a `setTimeout` and what this line had
    // no way of doing: it was a row of the window with the last thing said
    // still on it, so a zoom percentage stayed along the bottom edge until
    // something else was said.
    //
    // A thread rather than a timer, because there is no runtime here to hold
    // one — nothing in this reader is async except the mailbox. It sleeps and
    // posts, which is the same door `watch.rs` uses to reach a window from a
    // thread of its own, and the "notice-timeout" arm above throws the
    // message away if what is on the line is no longer the message this timer
    // was started for. So a second notice does not vanish with the first
    // one's four seconds — and a message said twice in a row keeps the first
    // timer rather than restarting it, which is the one case this is
    // imprecise about and costs at most four seconds of a sentence that is
    // still true.
    {
        let notifying = notifying.clone();
        let mut last = String::new();
        use_effect(move || {
            let said = viewer.read().notice.clone();
            if said == last {
                return;
            }
            last = said.clone();
            if said.is_empty() {
                return;
            }
            crate::emit::after(
                NOTICE_LASTS,
                notifying.clone(),
                crate::emit::News {
                    event: "notice-timeout".into(),
                    target: None,
                    payload: serde_json::Value::String(said),
                },
            );
        });
    }

    // **And the pill puts itself away after a second**, the same way and for
    // the same reason: `flashPill` in the app is a `setTimeout` of 1100ms,
    // restarted by every scroll. The effect watches the scroll offset, so a
    // wheel, a key, a jump and a link all flash it without one of them having
    // to remember to.
    {
        let notifying = notifying.clone();
        let mut last = f64::NAN;
        use_effect(move || {
            let now = viewer.read().scroll_top;
            if now == last {
                return;
            }
            last = now;
            let Some(token) = viewer.write().flash_pill() else {
                return;
            };
            crate::emit::after(
                PILL_LASTS,
                notifying.clone(),
                crate::emit::News {
                    event: "pill-timeout".into(),
                    target: None,
                    payload: serde_json::Value::from(token),
                },
            );
        });
    }

    // **And a zoom gesture ends when the fingers stop**, which is the same
    // shape again and for the same reason: a pinch is a stream of events with
    // no end in it, so what ends it is the gap after the last one. Until then
    // every page is stretched rather than redrawn — see
    // [`crate::page::Chosen::holding`] — and this is what puts them back
    // sharp.
    {
        let notifying = notifying.clone();
        let mut last = 0u64;
        use_effect(move || {
            let Some(token) = viewer.read().zoom_gesture() else {
                return;
            };
            if token == last {
                return;
            }
            last = token;
            crate::emit::after(
                ZOOM_SETTLES,
                notifying.clone(),
                crate::emit::News {
                    event: "zoom-settled".into(),
                    target: None,
                    payload: serde_json::Value::from(token),
                },
            );
        });
    }

    let held = viewer.read();
    let scroll_top = held.scroll_top;
    // How far across the document sits, which is nothing unless it is wider
    // than the window. See [`Viewer::across`].
    let scroll_left = held.scroll_left();
    let wearing = held.palette();
    let theme_name = held.theme_name();
    // Which draft of the document is being drawn — in every page's key, so
    // that a recompile replaces the nodes and the textures with them. See
    // `Viewer::edition`.
    let edition = held.edition;
    let mounted = held.layout.mounted(held.scroll_top);
    let content_width = held.layout.content_width();
    let content_height = held.layout.content_height();
    let pages = held.pages();
    let notice = held.notice.clone();
    // Read once for the whole render rather than per page: six strings out of
    // the settings table, and the great majority of renders draw no popover
    // at all.
    let markup_colours = held.markup_colors();
    let sidebar_open = held.sidebar_open;
    let find_open = held.find_open;
    let presenting = held.presenting;
    let toolbar_on = held.toolbar && !held.presenting;
    // Where the find bar hangs: under the toolbar, or up at the window's edge
    // when there is no toolbar to hang under. `styles.css` says
    // `calc(var(--toolbar-height) + 12px)` and `12px` in two rules; this is
    // the same two numbers with the selector done in Rust.
    let find_top = if toolbar_on { 58 } else { 12 };
    // The pill, and what it says. See [`Viewer::flash_pill`].
    // The note the reader has opened, and what its page is called — the label
    // rather than the position, which is what `showNote` says too.
    let worn_built_in = held.store.theme().built_in;
    let note_open = held.note_open.clone();
    let locked = held.locked.clone();
    // The bullets the password field shows, counted here because a format
    // hole in `rsx!` cannot hold a string literal of its own. See the field.
    let locked_shown = locked
        .as_ref()
        .map(|locked| "\u{2022}".repeat(locked.typed.chars().count()))
        .unwrap_or_default();
    let details_open = held.details_open;
    let details_rows = if details_open {
        held.details()
    } else {
        Vec::new()
    };
    // The Sign window, and the signatures already kept. The list is read off
    // the disk, so it is read while the window is open and not otherwise —
    // the same bargain the recents shelf strikes one paragraph down.
    let signing = held.signing.clone();
    let kept = if signing.is_some() {
        held.signatures()
    } else {
        Vec::new()
    };
    // And what is already on the document, which is read from the file rather
    // than remembered — see `PageSource::signatures`.
    let signed_here = if signing.is_some() {
        held.signed_here()
    } else {
        Vec::new()
    };
    // Whether a signature is looking for somewhere to go, which changes what
    // a click on a page means and what the pointer looks like over one.
    let placing = held.placing.is_some();
    // Asked before the list is consumed by the rows below it, which is the
    // only reason it is a variable.
    let nothing_kept = kept.is_empty();
    let note_page = note_open
        .as_ref()
        .map(|(page, _)| held.label(*page))
        .unwrap_or_default();
    let peeking = held.peeking();
    let pill_up = held.pill_shown();
    let pill_text = held.pill_text();
    // Whether this window has a document in it, which decides two things: what
    // the toolbar carries, and whether the body is the document or the start
    // screen. See [`Viewer::empty`].
    let empty = held.empty();
    // A document over the window, if there is one. See [`Viewer::dragging`].
    let dragging = held.dragging;
    // What the button the Document menu hangs off is called: the document's
    // name, or the app's own "Open…" when there is none.
    // The shelf, read once for this render. It is a file on the disk, so it is
    // read when the Document menu is open and not otherwise — a menu that is
    // shut costs nothing, which is the same bargain every other menu in this
    // bar strikes.
    let recents = if held.menu == Some(Menu::Document) {
        held.recents()
    } else {
        Vec::new()
    };
    // The title button exists only where there is a document, so this is the
    // document's name and nothing else. It used to double as the "Open…"
    // button for a window with none — one button rather than two, on the
    // reasoning that the menu behind it was one menu. It is two menus now, and
    // Open… is a button of its own that is there whether or not anything is
    // open, which is what the app does and is the plainer answer.
    let shelf_name = held.store.title().to_string();
    // **Whether the name has run out of box**, which is what decides the fade
    // over its last twenty-four pixels — see `.chip.title.clipped`. Blitz has
    // no `text-overflow: ellipsis`, so the fade stands in for one, and a fade
    // drawn unconditionally is a fade over every name that fits: `book.pdf`
    // went pale over a third of its width. The box is `max-width: 276px` less
    // sixteen of padding, and thirty-four characters is what fills it — the
    // app's own `34ch`, which is the same number counted rather than measured.
    // Erring long is the safe direction: a name a character or two past the
    // cap is cut without a fade, which is what every reader has seen from a
    // narrow column; a name inside the cap is never faded, which was the
    // complaint.
    let name_clipped = shelf_name.chars().count() > 34;
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
    // Each theme's name, the two colours its swatch is drawn in, and whether
    // it is one the reader wrote. Read through `palette` rather than handed
    // raw to CSS: a swatch that shows a colour the renderer cannot read is
    // the one place in the app meant to show what you are about to get
    // lying about it — the app's own finding, in `ui.swatch`.
    let theme_rows: Vec<(String, String, String, bool)> = held
        .store
        .themes()
        .iter()
        .map(|theme| {
            let ink = crate::palette::read_colour(&theme.text).unwrap_or([0, 0, 0]);
            let paper = if theme.recolor {
                crate::palette::read_colour(&theme.background).unwrap_or([255, 255, 255])
            } else {
                [255, 255, 255]
            };
            (
                theme.name.clone(),
                crate::palette::hex(ink),
                crate::palette::hex(paper),
                !theme.built_in,
            )
        })
        .collect();
    let dark_now = held.store.dark_now();
    let following = held.store.flag("follow_system_theme");
    let key_dark = held.chord_for(Action::Dark);
    let theme_index = held.store.theme_index();
    let fit = held.layout.fit;
    let spread = held.layout.spread;
    let key_open = held.chord_for(Action::Open);
    let key_new_window = held.chord_for(Action::NewWindow);
    let key_mark = held.chord_for(Action::Mark);
    let key_print = held.chord_for(Action::Print);
    let key_fit_width = held.chord_for(Action::FitWidth);
    let key_fit_page = held.chord_for(Action::FitPage);
    let key_actual = held.chord_for(Action::ActualSize);
    // The settings menu names three more keys, for the same reason every
    // other menu item here names one: read off the keymap, so a rebound key
    // is the key the menu shows.
    let key_toolbar = held.chord_for(Action::Toolbar);
    let key_fullscreen = held.chord_for(Action::Fullscreen);
    let key_settings = held.chord_for(Action::Settings);
    let key_rotate_left = held.chord_for(Action::RotateLeft);
    let key_rotate_right = held.chord_for(Action::RotateRight);
    let full_screen = held.full_screen;
    let scroll_mode = held.layout.mode;
    let recolor_images = held.recolor_images();
    let page_pill = held.page_pill();
    let page_field = held.page_field();
    // How wide the page box is: the padding, the border, and the number in it,
    // with a floor so that page 1 of a pamphlet is not a slot. See the comment on
    // `.pill` below — Blitz cannot centre an input's text, so the box is made
    // to fit rather than the text made to sit in the middle of it.
    //
    // **The floor is the app's own width**, not the smallest box a digit will
    // sit in. `.page-jump input` is `width: 44px` whatever is in it — four
    // digits fit and one digit is centred in the same box — and a floor of
    // twenty-eight made page 1 of any document a slot half that size beside a
    // count that was not shrinking with it. Growing past 44 is still this
    // reader's own answer to the centring it cannot do, and it only happens
    // at four digits.
    let page_box = (14.0 + 9.1 * page_field.chars().count() as f64).max(44.0);
    // What an icon is drawn in, and why it is a string rather than a class.
    // An inline `<svg>` here is handed to usvg with no cascade behind it — see
    // [`Icon`] — so the shade a chip's label resolves to has to be passed down
    // beside it. Two of them, because a chip whose thing is in force is the
    // accent and every other chip is the quiet shade.
    let ink = crate::palette::hex(wearing.muted());
    let ink_on = crate::palette::hex(wearing.accent);
    // The third shade: what a tick that is *not* ticked is drawn in, and the
    // magnifier at the head of the find bar. `.find-option svg` is
    // `opacity: 0.28` in the app, which is the same thing said in the one way
    // an icon with no cascade behind it can be told.
    let faint = crate::palette::hex(wearing.faint());
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
    // What the zoom menu needs, read while the reader is held: the remembered
    // zoom, which is what the presets tick against, and what is actually on
    // screen, which is where the stepper starts. In a fit mode those are
    // different numbers — see [`Viewer::zoom_percent`].
    let zoom_now = held.layout.zoom * 100.0;
    let shown_percent = held.zoom_percent().round();
    // "Actual size" is a fit mode *and* a zoom of 1, so it is ticked only when
    // both are true — `showZoomMenu` asks the same two questions.
    let actual_100 = held.layout.fit == Fit::Actual && (zoom_now - 100.0).abs() < 0.5;
    // What a page is *drawn* at, against what it is *shown* at. The two are
    // the same number except under a zoom gesture, where the drawn size is
    // frozen at whatever it was when the fingers went down — see
    // [`Viewer::zoom_held_at`] and [`crate::page::Chosen::holding`].
    let held_at = held.zoom_held_at();
    let boxes: Vec<Placed> = mounted
        .iter()
        .filter_map(|&index| {
            held.layout.box_of(index).map(|page| Placed {
                index,
                top: page.top,
                left: page.left,
                width: page.width,
                height: page.height,
                drawn: (
                    (page.width * held_at).round(),
                    (page.height * held_at).round(),
                ),
                hits: held.highlights(index + 1),
                links: held.link_areas(index + 1),
                notes: held.note_areas(index + 1),
                selected: held.selected_areas(index + 1),
                swatches: held
                    .markup_at
                    .filter(|(page, _)| *page == index + 1)
                    .map(|(_, area)| area),
                mark: held
                    .mark_open
                    .as_ref()
                    .filter(|(page, ..)| *page == index + 1)
                    .map(|(_, area, key, colour)| (*area, key.clone(), colour.clone())),
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
            class: match (presenting, placing) {
                // A signature waiting for somewhere to go changes what a click
                // on the page means, so it changes what the pointer looks like
                // over one. Nothing else in this reader arms a click, and a
                // mode with nothing on screen saying so is a mode a reader
                // discovers by signing something they did not mean to.
                (true, _) => "root presenting",
                (false, true) => "root placing",
                (false, false) => "root",
            },
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
            // And a press past the find bar puts *that* away, which is
            // `onFindOutside` in `main.ts`: "reaching past the bar puts it
            // away, the way the Theme and Settings menus do. Anything below
            // the toolbar is somewhere else — the document, the contents, a
            // link — and going there is done with the search, whether or not
            // it was said out loud."
            //
            // The app spells its exceptions as a selector — `FIND_KEEPS_OPEN`,
            // the bar itself, the top strip, the popovers, the windows and the
            // list of results — and a handler here has no `closest` to ask
            // with. So the top strip is asked for by *height*, which is the
            // whole of what that half of the selector means, and the other
            // four stop the press themselves the way the menus already do. The
            // list of results is the one worth naming: it is this search seen
            // larger, so picking a line out of it must not close the thing
            // that found it — `sidebar.rs` stops the press over the Results
            // panel and its tab.
            onmousedown: move |event| {
                let (menu, typing, find, strip) = {
                    let held = viewer.read();
                    (
                        held.menu.is_some(),
                        held.typing_page,
                        held.find_open,
                        held.chrome(),
                    )
                };
                if menu {
                    viewer.write().close_menu();
                }
                if typing {
                    viewer.write().cancel_page();
                }
                // `chrome()` is nought with the bar away or while presenting,
                // which is the right answer both times: the find card comes up
                // to meet the window's edge and there is no strip to be inside
                // of, so every press that reaches the root is past it. That is
                // `#shell[data-toolbar="hidden"]` in the app saying the same.
                if find && event.client_coordinates().y > strip {
                    viewer.write().close_find();
                }
            },
            onmousemove: move |event| {
                let (resizing, sweeping, drawing) = {
                    let held = viewer.read();
                    (
                        held.resize_from.is_some(),
                        held.sweeping(),
                        held.signing.as_ref().is_some_and(|pad| pad.drawing),
                    )
                };
                // **A hand signing a name leaves the pad**, which is the whole
                // reason this is here and not on the pad itself. The point is
                // taken in the pad's own space, which means subtracting where
                // the pad is — and a handler cannot ask an element where it
                // is, so it is remembered at the press. See
                // [`Viewer::draw_from`].
                if drawing {
                    let at = event.client_coordinates();
                    viewer.write().draw_on_pad((at.x, at.y));
                    return;
                }
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
                } else {
                    // The top edge of the window, reached for. See
                    // [`Viewer::reach_for_toolbar`] — it does nothing at all
                    // while the toolbar is up, which is almost always.
                    let y = event.client_coordinates().y;
                    if viewer.read().peek_changes(y) {
                        viewer.write().reach_for_toolbar(y);
                    }
                }
            },
            onmouseup: move |_| {
                let (resizing, sweeping) = {
                    let held = viewer.read();
                    (held.resize_from.is_some(), held.sweeping())
                };
                viewer.write().draw_done();
                if resizing {
                    viewer.write().finish_resize_sidebar();
                }
                if sweeping {
                    viewer.write().end_sweep();
                    // **A sweep that covered something offers to mark it.**
                    // The app's own hard-won lesson, and the reason it is
                    // worth repeating here: the colour popover was reachable
                    // only by ⌘⇧H, so nothing on screen ever pointed at
                    // highlighting and nobody found the feature after it was
                    // built. Letting go of a selection is the moment the
                    // reader is looking at the passage, which is what
                    // `markup-assessment.md` describes the gesture as all
                    // along.
                    viewer.write().open_markup();
                }
            },
            if toolbar_on {
            div { class: "toolbar",
                // **Everything in this bar that is about a document is gone
                // when there is none**, which is `#shell[data-empty="true"]`
                // in the app's stylesheet doing the same thing by selector.
                // What is left is the two things that are still true of a
                // window with nothing in it: how to put something in it, and
                // what it looks like.
                //
                // **Three groups, and the middle one is why.** This was one
                // flat row with a spacer in it, so the page readout ended up
                // wherever the row ran out of chips — which was the far right,
                // beside the cog, and a page counter at the edge of the window
                // is a page counter nobody looks at. The app puts navigation in
                // the middle of the bar and this is that, group for group:
                // `.bar-left` gives way because it holds a title and a title
                // has somewhere to shrink to, `.bar-center` never gives way,
                // and `.bar-right` grows against the left so the two sides
                // share the slack and the middle stays near the middle.
                div { class: "bar-group bar-left",
                    if !empty {
                    button {
                        // Not `.sidebar`, which is the panel itself: a selector
                        // that matches the button *and* the thing the button
                        // opens is a test that cannot tell them apart.
                        class: if sidebar_open { "chip contents on" } else { "chip contents" },
                        // Contents is `opens(…)` in `main.ts` for the same
                        // reason the five menus are — see `show_menu`. The
                        // *keyboard* action is not, there or here: a shortcut
                        // asked for the panel and said nothing about the
                        // search.
                        onclick: move |_| {
                            viewer.write().close_find();
                            viewer.write().toggle_sidebar();
                        },
                        Icon { name: "contents", stroke: if sidebar_open { ink_on.clone() } else { ink.clone() } }
                        "Contents"
                    }
                    }
                    // **The way to another document, which is not the same
                    // question as what can be done with this one.** Both used
                    // to hang off the title, which is the shape the app had
                    // before it split them and is a strange one once you say
                    // it out loud: a menu opened by pressing the name of the
                    // paper you are reading, four of whose items are about
                    // papers you are not. The split is the app's own and its
                    // reasoning is worth keeping — the picker, a second
                    // window and the shelf all answer "open something"; a
                    // mark, a highlight and a print belong to the document
                    // already on screen.
                    //
                    // **Every menu hangs inside an anchor of its own, and
                    // that is the whole of where a menu appears.** They were
                    // one layer pinned to the ends of the toolbar — the
                    // Document menu to the left edge, the other two to the
                    // right — on the reasoning that a measured offset would
                    // need keeping in step by hand and there is no way to ask
                    // an element where it is from here. Both halves of that
                    // were true and the conclusion was wrong: an absolutely
                    // positioned child of a `position: relative` wrapper needs
                    // no measurement at all, and it is *the browser* that
                    // keeps it in step.
                    //
                    // Out of the flow, so the 46px row is still 46px whatever
                    // is hanging off it — which is what the layer was for and
                    // is not a reason to have one.
                    div { class: "anchor",
                        button {
                            class: if menu == Some(Menu::Open) { "chip open on" } else { "chip open" },
                            onmousedown: move |event| event.stop_propagation(),
                            onclick: move |_| viewer.write().show_menu(Menu::Open),
                            Icon {
                                name: "folder",
                                stroke: if menu == Some(Menu::Open) { ink_on.clone() } else { ink.clone() },
                            }
                            "Open…"
                        }
                        if menu == Some(Menu::Open) {
                            div { class: "menu open", role: "menu", "aria-label": "Open",
                                // A press inside a menu is not a press outside it: the root
                                // puts the menu away, and the item's own click comes after
                                // the press. This was on the layer these three used to share.
                                onmousedown: move |event| event.stop_propagation(),
                                button {
                                    class: "menu-item",
                                    "data-item": "open",
                                    onclick: {
                                        let pick = pick.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            pick.ask(Opening::Here);
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "folder", stroke: ink.clone() }
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
                                        move |_| {
                                            viewer.write().close_menu();
                                            pick.ask(Opening::Beside);
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "window", stroke: ink.clone() }
                                    span { class: "menu-label", "Open document in new window…" }
                                }
                                button {
                                    class: "menu-item",
                                    onclick: {
                                        let frame = frame.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            frame.ask(Ask::NewWindow);
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "window", stroke: ink.clone() }
                                    span { class: "menu-label", "New window" }
                                    span { class: "menu-key", "{key_new_window}" }
                                }
                                // And the shelf, which is the app's own last
                                // section of this menu. It is the same list
                                // the start screen shows and it is here for
                                // the reader who has a document open: the
                                // start screen is unreachable without first
                                // putting that one down, and "the paper I was
                                // reading yesterday" should not cost a trip
                                // through the file picker to find again.
                                if !recents.is_empty() {
                                    div { class: "menu-rule" }
                                    div { class: "menu-section", "Recently read" }
                                    for entry in recents.iter().cloned() {
                                        button {
                                            key: "{entry.path}",
                                            class: "menu-item",
                                            "data-item": "recent",
                                            onclick: {
                                                let frame = frame.clone();
                                                let path = entry.path.clone();
                                                move |_| {
                                                    let path = path.clone();
                                                    viewer.write().close_menu();
                                                    if viewer.write().open_here(&path) {
                                                        let title = viewer
                                                            .read()
                                                            .store
                                                            .title()
                                                            .to_string();
                                                        frame.ask(Ask::Showing { path, title });
                                                    }
                                                }
                                            },
                                            // A drawing rather than the empty tick
                                            // column every other item carries: the
                                            // app's own `ui.menuItem({icon:
                                            // "document"})`, and it is what makes
                                            // this section read as a shelf rather
                                            // than as four more commands.
                                            span { class: "menu-tick",
                                                Icon { name: "document", stroke: crate::palette::hex(wearing.faint()) }
                                            }
                                            Icon { name: "document", stroke: ink.clone() }
                                    span { class: "menu-label", "{entry.title}" }
                                            span { class: "menu-key", "p. {entry.page}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // The two window verbs, in the bar only while there is
                    // nothing else in it. This is the app's own rule — see
                    // `#shell:not([data-empty="true"]) #new-window` in
                    // `styles.css` — and the reason is that a window with a
                    // document in it has better things to spend the room on,
                    // while a window with none has ⌘N and ⌘W and no way of
                    // knowing it.
                    if empty {
                    button {
                        class: "chip new-window",
                        onclick: {
                            let frame = frame.clone();
                            move |_| frame.ask(Ask::NewWindow)
                        },
                        Icon { name: "window", stroke: ink.clone() }
                        "New window"
                    }
                    button {
                        class: "chip close-window",
                        onclick: {
                            let frame = frame.clone();
                            move |_| frame.ask(Ask::Close)
                        },
                        Icon { name: "close", stroke: ink.clone() }
                        "Close window"
                    }
                    }
                    if !empty {
                    // The gesture the app calls Close, and the one place a
                    // document can be put down without the window going with
                    // it. A button rather than a menu item, which is where
                    // the app keeps it: it was under the title here, and a
                    // reader looking for how to put a document down does not
                    // look inside a menu named after the document.
                    button {
                        class: "chip close-doc",
                        "data-item": "close-document",
                        onclick: {
                            let frame = frame.clone();
                            move |_| {
                                viewer.write().close_menu();
                                viewer.write().close_document();
                                // The desk, the restore list and the document
                                // watch all belong to the process, and an
                                // empty path is how this window says it is
                                // showing none. See [`Ask::Showing`].
                                frame.ask(Ask::Showing {
                                    path: String::new(),
                                    title: String::new(),
                                });
                            }
                        },
                        Icon { name: "close", stroke: ink.clone() }
                        "Close"
                    }
                    // What the document is called — its own `/Title` where
                    // that is worth having, and the file's name where it is
                    // not, see `store::worth_calling` — and the button the
                    // document's own menu hangs off, which is where the app
                    // puts it too.
                    // The extra air the app puts in front of the name lives on
                    // the anchor rather than on the button, so that the menu
                    // still comes down flush with the button it belongs to.
                    div { class: "anchor titled",
                        button {
                            class: match (menu == Some(Menu::Document), name_clipped) {
                                (true, true) => "chip title on clipped",
                                (true, false) => "chip title on",
                                (false, true) => "chip title clipped",
                                (false, false) => "chip title",
                            },
                            onmousedown: move |event| event.stop_propagation(),
                            onclick: move |_| viewer.write().show_menu(Menu::Document),
                            // **No icon**, which is `#doc-title` in the app's
                            // own `index.html`: this is the one thing in the
                            // bar that is not a verb, and an icon in front of
                            // a file name says nothing the name does not. It
                            // also cost the name twenty-three pixels in a bar
                            // that has none to spare — the name is the first
                            // thing squeezed when the bar runs out of room,
                            // so what the icon was taking came straight out
                            // of what the reader can read.
                            //
                            // The colour is named here for the reason the
                            // zoom readout's is: there is no icon on this
                            // button any more, so nothing about it changes
                            // when the theme does, and Blitz settles the
                            // colour of a text run when it builds the run.
                            style: if menu == Some(Menu::Document) { "color: {ink_on}" } else { "color: {crate::palette::hex(wearing.faint())}" },
                            "{shelf_name}"
                        }
                        if menu == Some(Menu::Document) {
                            div { class: "menu document", role: "menu", "aria-label": "Document",
                                onmousedown: move |event| event.stop_propagation(),
                                // Where the document lives, which is the app's
                                // own first item — and the one thing in this
                                // menu that is about the file rather than
                                // about what is in it.
                                button {
                                    class: "menu-item",
                                    "data-item": "reveal",
                                    onclick: {
                                        let reveal = reveal.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            let path = viewer.read().document.path().to_string();
                                            if path.is_empty() {
                                                return;
                                            }
                                            if let Err(said) = reveal.show(&path) {
                                                viewer.write().notice = said;
                                            }
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "folder", stroke: ink.clone() }
                                    span { class: "menu-label", "Show in {crate::app::file_manager_name()}" }
                                }
                                // The page marked, which used to be a chip in
                                // the bar and is not one in the app's: a mark
                                // is set once and read from the Contents
                                // panel, so a permanent button for it is a
                                // permanent button for something nobody
                                // presses twice in an hour. It ticks, which
                                // is what the chip's "on" state was saying.
                                button {
                                    class: "menu-item",
                                    "data-item": "mark",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        let page = viewer.read().page();
                                        viewer.write().mark_page(page);
                                    },
                                    span { class: "menu-tick", {if marked { "✓" } else { "" }} }
                                    Icon { name: "mark", stroke: ink.clone() }
                                    span { class: "menu-label", "Mark this page" }
                                    span { class: "menu-key", "{key_mark}" }
                                }
                                // **The one item in this menu the app has
                                // no counterpart for.** Signing is not
                                // parity — see [`crate::sign`], and
                                // `signing-assessment.md` for the two things
                                // the word means and which of them this is.
                                // It sits beside Mark this page because the
                                // two are the same kind of gesture: something
                                // done *to* the document, as against the
                                // three below, which are ways of taking it
                                // somewhere else.
                                button {
                                    class: "menu-item",
                                    "data-item": "sign",
                                    onclick: move |_| { viewer.write().open_signing(); },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "sign", stroke: ink.clone() }
                                    span { class: "menu-label", "Sign…" }
                                }
                                // Printing prints nothing: the document goes
                                // to a program that does. See [`Printer`].
                                button {
                                    class: "menu-item",
                                    "data-item": "print",
                                    onclick: {
                                        let printer = printer.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            let path = viewer.read().document.path().to_string();
                                            if path.is_empty() {
                                                return;
                                            }
                                            if let Err(said) = printer.print(&path) {
                                                viewer.write().notice = said;
                                            }
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "print", stroke: ink.clone() }
                                    span { class: "menu-label", "Print…" }
                                    span { class: "menu-key", "{key_print}" }
                                }
                                // Two ways of taking the document with you,
                                // which is the app's own pair. The name is
                                // what the toolbar shows; the path is what
                                // another program will want.
                                button {
                                    class: "menu-item",
                                    "data-item": "copy-name",
                                    onclick: {
                                        let clip = clip.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            let name = viewer.read().store.title().to_string();
                                            clip.put(&name);
                                            viewer.write().notice = "Name copied.".into();
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "copy", stroke: ink.clone() }
                                    span { class: "menu-label", "Copy name" }
                                }
                                button {
                                    class: "menu-item",
                                    "data-item": "copy-path",
                                    onclick: {
                                        let clip = clip.clone();
                                        move |_| {
                                            viewer.write().close_menu();
                                            let path = viewer.read().document.path().to_string();
                                            clip.put(&path);
                                            viewer.write().notice = "Path copied.".into();
                                        }
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "copy", stroke: ink.clone() }
                                    span { class: "menu-label", "Copy path" }
                                }
                                div { class: "menu-rule" }
                                // What the document says about itself. Last,
                                // and behind a rule, because it is the one
                                // item here that opens something rather than
                                // doing something.
                                button {
                                    class: "menu-item",
                                    "data-item": "information",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        viewer.write().open_details();
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "info", stroke: ink.clone() }
                                    span { class: "menu-label", "Information" }
                                }
                            }
                        }
                    }
                    }
                }
                // The middle of the bar, and the only thing in it: where you
                // are, with a step either side of it. The two buttons are the
                // app's `#prev-page` and `#next-page` — a page is turned by a
                // key or by scrolling far more often than by pressing an
                // arrow, but the pair is what makes the number between them
                // read as a control rather than as a readout.
                div { class: "bar-group bar-center",
                    if !empty {
                    button {
                        class: "chip page-previous",
                        "aria-label": "Previous page",
                        onclick: move |_| {
                            let previous = viewer.read().page().saturating_sub(1).max(1);
                            viewer.write().go_to_page(previous);
                        },
                        Icon { name: "up", stroke: ink.clone() }
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
                                let plain = crate::keymap::plain(modifiers);
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
                            // The colour is written out beside the width for
                            // the reason the zoom readout's is: this is a
                            // label that does not change when the theme does,
                            // and Blitz was leaving it in the ink of the theme
                            // before — which on a dark theme after a light one
                            // is a page number nobody can see. See
                            // `.chip.fit` above and `PROGRESS.md`.
                            style: "width: {page_box}px; color: {crate::palette::hex(wearing.text)};",
                            "aria-label": "Go to page",
                            onclick: move |_| viewer.write().open_page_field(),
                            "{page_field}"
                        }
                        }
                        span { class: "of", "of {pages}" }
                    }
                    button {
                        class: "chip page-next",
                        "aria-label": "Next page",
                        onclick: move |_| {
                            let next = viewer.read().page() + 1;
                            viewer.write().go_to_page(next);
                        },
                        Icon { name: "down", stroke: ink.clone() }
                    }
                    }
                }
                div { class: "bar-group bar-right",
                    if !empty {
                    // The app's `#find`, which this bar did not have: ⌘F was
                    // the only way in, and a shortcut is not a way in for
                    // somebody who does not already know it is there.
                    button {
                        class: if find_open { "chip find on" } else { "chip find" },
                        onclick: move |_| viewer.write().open_find(),
                        Icon { name: "search", stroke: if find_open { ink_on.clone() } else { ink.clone() } }
                        "Search"
                    }
                    // **Left and Right, in the bar.** `#rotate-left` and
                    // `#rotate-right` in `index.html`, beside Search and
                    // before the zoom — they were items in the View menu here,
                    // which is a place to go looking for something you do
                    // twice in a row while reading a scan that came in
                    // sideways.
                    button {
                        class: "chip rotate-left",
                        title: "Turn the page left — {key_rotate_left}",
                        onclick: move |_| viewer.write().rotate(-1),
                        Icon { name: "rotateLeft", stroke: ink.clone() }
                        "Left"
                    }
                    button {
                        class: "chip rotate-right",
                        title: "Turn the page right — {key_rotate_right}",
                        onclick: move |_| viewer.write().rotate(1),
                        Icon { name: "rotateRight", stroke: ink.clone() }
                        "Right"
                    }
                    }
                    if !empty {
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
                        button {
                            class: "chip zoom-out",
                            "aria-label": "Zoom out",
                            onclick: move |_| viewer.write().zoom(false),
                            Icon { name: "minus", stroke: ink.clone() }
                        }
                        div { class: "anchor",
                            button {
                                class: if menu == Some(Menu::View) { "chip fit on" } else { "chip fit" },
                                // **The one label in the bar with no icon
                                // beside it, and the only one that kept the
                                // last theme's colour.** Blitz rebuilds an
                                // element's inline layout — which is where the
                                // colour of its text is settled — when
                                // something about that element or its children
                                // is mutated, and a change to a custom
                                // property on the root is neither. Every other
                                // chip has an `Icon` whose `stroke` is the
                                // theme's, so every other chip is mutated and
                                // comes out right; this one changed colour on
                                // the next zoom step, which is when its text
                                // changed. Naming the colour here is the same
                                // answer the icons already carry rather than a
                                // second one. See `PROGRESS.md`.
                                style: if menu == Some(Menu::View) { "color: {ink_on}" } else { "color: {ink}" },
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
                                    // **`showZoomMenu` in `main.ts`, item for
                                    // item.** It had grown spread and rotation
                                    // here, which are not what a reader
                                    // pressing the zoom is asking about —
                                    // spread is in the settings menu next door
                                    // and rotation is two buttons in the bar,
                                    // which is where the app keeps them. What
                                    // it was missing is the half that makes
                                    // this a zoom menu at all: a number to
                                    // type and the presets under it.
                                    //
                                    // Nothing in here puts the menu away: a
                                    // zoom is something you try on, like a
                                    // theme, so the ticks move and the list
                                    // stays.
                                    button {
                                        class: if fit == Fit::Width { "menu-item on" } else { "menu-item" },
                                        onclick: move |_| viewer.write().set_fit(Fit::Width),
                                        span { class: "menu-tick", {if fit == Fit::Width { "✓" } else { "" }} }
                                        span { class: "menu-label", "Fit width" }
                                        span { class: "menu-key", "{key_fit_width}" }
                                    }
                                    button {
                                        class: if fit == Fit::Page { "menu-item on" } else { "menu-item" },
                                        onclick: move |_| viewer.write().set_fit(Fit::Page),
                                        span { class: "menu-tick", {if fit == Fit::Page { "✓" } else { "" }} }
                                        span { class: "menu-label", "Fit page" }
                                        span { class: "menu-key", "{key_fit_page}" }
                                    }
                                    button {
                                        class: if actual_100 { "menu-item on" } else { "menu-item" },
                                        onclick: move |_| viewer.write().actual_size(),
                                        span { class: "menu-tick", {if actual_100 { "✓" } else { "" }} }
                                        Icon { name: "actualSize", stroke: ink.clone() }
                                    span { class: "menu-label", "Actual size" }
                                        span { class: "menu-key", "{key_actual}" }
                                    }
                                    div { class: "menu-rule" }
                                    // The rest of the ladder, for the sizes
                                    // the presets below do not name. It starts
                                    // from what is on the screen rather than
                                    // from the remembered zoom, because in a
                                    // fit mode those are different numbers and
                                    // the one being looked at is the one to
                                    // type over.
                                    div { class: "menu-row",
                                        label { class: "menu-row-label", "Zoom to" }
                                        crate::prefs::Stepper {
                                            viewer,
                                            value: shown_percent,
                                            min: 25.0,
                                            max: 600.0,
                                            step: 25.0,
                                            unit: "%".to_string(),
                                            onchange: move |value: f64| viewer.write().set_zoom(value / 100.0),
                                        }
                                    }
                                    for percent in [50.0_f64, 75.0, 100.0, 125.0, 150.0, 200.0, 300.0] {
                                        {
                                            let on = fit == Fit::Actual && (zoom_now - percent).abs() < 0.5;
                                            rsx! {
                                                button {
                                                    key: "{percent}",
                                                    class: if on { "menu-item on" } else { "menu-item" },
                                                    onclick: move |_| viewer.write().set_zoom(percent / 100.0),
                                                    span { class: "menu-tick", {if on { "✓" } else { "" }} }
                                                    span { class: "menu-label", "{percent:.0}%" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            class: "chip zoom-in",
                            "aria-label": "Zoom in",
                            onclick: move |_| viewer.write().zoom(true),
                            Icon { name: "plus", stroke: ink.clone() }
                        }
                    }
                    }
                    div { class: "anchor",
                        button {
                            class: if menu == Some(Menu::Theme) { "chip theme on" } else { "chip theme" },
                            onmousedown: move |event| event.stop_propagation(),
                            onclick: move |_| viewer.write().show_menu(Menu::Theme),
                            // **"Theme", not the theme's name**, which is
                            // `#theme` in the app's own `index.html` and is
                            // the rule the bar beside it follows: a button
                            // says what pressing it does. The name of the
                            // theme in force is a tick in the menu, where the
                            // fourteen it is one of are. The harness still
                            // reads it — off `data-theme`, which is the
                            // reader's own account of what it is wearing and
                            // is not a label anybody has to look at.
                            "data-theme": "{theme_name}",
                            Icon { name: "theme", stroke: if menu == Some(Menu::Theme) { ink_on.clone() } else { ink.clone() } }
                            "Theme"
                        }
                        if menu == Some(Menu::Theme) {
                            div { class: "menu theme", role: "menu", "aria-label": "Theme",
                                // A press inside a menu is not a press outside it: the root
                                // puts the menu away, and the item's own click comes after
                                // the press. This was on the layer these three used to share.
                                onmousedown: move |event| event.stop_propagation(),
                                // **The two switches above the list**, which is
                                // `showThemeMenu` in `main.ts`: dark mode is a
                                // move between the pair the reader has chosen
                                // and following the machine is the thing that
                                // does it for them, and both belong where the
                                // themes are. They were on the Appearance page
                                // alone here, which is a window away.
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Dark mode" }
                                    span { class: "menu-row-note", "{key_dark}" }
                                    crate::prefs::Toggle {
                                        on: dark_now,
                                        onchange: move |on: bool| viewer.write().set_dark(on),
                                    }
                                }
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Light or dark follow system" }
                                    crate::prefs::Toggle {
                                        on: following,
                                        onchange: move |on: bool| viewer.write().set_follow_system(on),
                                    }
                                }
                                div { class: "menu-rule" }
                                div { class: "menu-section", "Themes" }
                                // Nothing in here that only changes an
                                // appearance puts the menu away: a theme is
                                // something you try on, so the tick moves and
                                // the list stays. The app says the same in the
                                // same place.
                                for (index, theme) in theme_rows.iter().cloned().enumerate() {
                                    {
                                        let (name, ink, paper, mine) = theme;
                                        rsx! {
                                            button {
                                                key: "{index}:{name}",
                                                class: if index == theme_index { "menu-item on" } else { "menu-item" },
                                                onclick: move |_| viewer.write().set_theme(index),
                                                span { class: "menu-tick", {if index == theme_index { "✓" } else { "" }} }
                                                // Two letters of the theme, in
                                                // the theme's own colours —
                                                // `ui.swatch`, and read through
                                                // `parseColor` for its reason:
                                                // a swatch that hands a raw
                                                // string to CSS shows a colour
                                                // the renderer cannot read.
                                                span {
                                                    class: "swatch",
                                                    style: "background: {paper}; color: {ink};",
                                                    "A"
                                                }
                                                span { class: "menu-label", "{name}" }
                                                if mine { span { class: "menu-key", "Yours" } }
                                            }
                                        }
                                    }
                                }
                                // The three at the foot of `showThemeMenu`,
                                // and they are the whole of the way in to the
                                // theme editor from the bar. A built-in is
                                // copied rather than edited, which is the
                                // app's wording and its reason: a shipped
                                // theme is written back on every run.
                                div { class: "menu-rule" }
                                button {
                                    class: "menu-item",
                                    "data-item": "new-theme",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        viewer.write().begin_theme(None);
                                        viewer.write().show_pane(Pane::Appearance);
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "plusCircle", stroke: ink.clone() }
                                    span { class: "menu-label", "New theme…" }
                                }
                                button {
                                    class: "menu-item",
                                    "data-item": "edit-theme",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        let worn = viewer.read().store.theme().clone();
                                        viewer.write().begin_theme(Some(worn));
                                        viewer.write().show_pane(Pane::Appearance);
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "edit", stroke: ink.clone() }
                                    span { class: "menu-label",
                                        {if worn_built_in {
                                            "Make a copy of this theme…"
                                        } else {
                                            "Edit this theme…"
                                        }}
                                    }
                                }
                                if !worn_built_in {
                                    button {
                                        class: "menu-item",
                                        "data-item": "delete-theme",
                                        onclick: move |_| {
                                            viewer.write().close_menu();
                                            let worn = viewer.read().store.theme().clone();
                                            viewer.write().begin_theme(Some(worn));
                                            viewer.write().delete_theme();
                                        },
                                        span { class: "menu-tick", "" }
                                        Icon { name: "trash", stroke: ink.clone() }
                                        span { class: "menu-label", "Delete this theme" }
                                    }
                                }
                                div { class: "menu-rule" }
                                button {
                                    class: "menu-item",
                                    "data-item": "appearance-settings",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        viewer.write().show_pane(Pane::Appearance);
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "settings", stroke: ink.clone() }
                                    span { class: "menu-label", "All appearance settings…" }
                                }
                            }
                        }
                    }
                    // **The cog opens a menu, not the window.**
                    // `showSettingsMenu` in `main.ts`: the four or five
                    // switches somebody reaches for while reading — the
                    // toolbar, full screen, how the pages come, whether a
                    // picture takes the theme — and "All settings…" at the
                    // bottom for the rest. Going straight to the window meant
                    // a window over the document for a switch that is one
                    // press.
                    div { class: "anchor",
                        button {
                            class: if menu == Some(Menu::Settings) { "chip settings on" } else { "chip settings" },
                            onmousedown: move |event| event.stop_propagation(),
                            onclick: move |_| viewer.write().show_menu(Menu::Settings),
                            Icon { name: "settings", stroke: if menu == Some(Menu::Settings) { ink_on.clone() } else { ink.clone() } }
                            "Settings"
                        }
                        if menu == Some(Menu::Settings) {
                            div { class: "menu settings", role: "menu", "aria-label": "Settings",
                                onmousedown: move |event| event.stop_propagation(),
                                div { class: "menu-section", "Window" }
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Show toolbar" }
                                    span { class: "menu-row-note", "{key_toolbar}" }
                                    // And then leave: this menu hangs off a
                                    // button in the toolbar, so turning the
                                    // toolbar off leaves it anchored to
                                    // nothing. The app closes it for the same
                                    // reason and says so in the same words.
                                    crate::prefs::Toggle {
                                        on: toolbar_on,
                                        onchange: move |_| {
                                            viewer.write().toggle_toolbar();
                                            viewer.write().close_menu();
                                        },
                                    }
                                }
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Full screen" }
                                    span { class: "menu-row-note", "{key_fullscreen}" }
                                    crate::prefs::Toggle {
                                        on: full_screen,
                                        onchange: move |on: bool| {
                                            viewer.write().set_full_screen(on);
                                            full_screen_frame.ask(Ask::FullScreen(on));
                                        },
                                    }
                                }
                                div { class: "menu-rule" }
                                div { class: "menu-section", "Reading" }
                                button {
                                    class: if scroll_mode == crate::layout::Mode::Continuous { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| {
                                        viewer.write().set_scroll_mode(crate::layout::Mode::Continuous);
                                        viewer.write().close_menu();
                                    },
                                    span { class: "menu-tick", {if scroll_mode == crate::layout::Mode::Continuous { "✓" } else { "" }} }
                                    span { class: "menu-label", "Continuous scrolling" }
                                    span { class: "menu-key", "Default" }
                                }
                                button {
                                    class: if scroll_mode == crate::layout::Mode::Paged { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| {
                                        viewer.write().set_scroll_mode(crate::layout::Mode::Paged);
                                        viewer.write().close_menu();
                                    },
                                    span { class: "menu-tick", {if scroll_mode == crate::layout::Mode::Paged { "✓" } else { "" }} }
                                    span { class: "menu-label", "One page at a time" }
                                }
                                div { class: "menu-rule" }
                                div { class: "menu-section", "Pages side by side" }
                                button {
                                    class: if spread == Spread::Single { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Single); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Single { "✓" } else { "" }} }
                                    span { class: "menu-label", "One page across" }
                                    span { class: "menu-key", "Default" }
                                }
                                button {
                                    class: if spread == Spread::Two { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Two); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Two { "✓" } else { "" }} }
                                    span { class: "menu-label", "Two side by side" }
                                }
                                button {
                                    class: if spread == Spread::Cover { "menu-item on" } else { "menu-item" },
                                    onclick: move |_| { viewer.write().set_spread(Spread::Cover); viewer.write().close_menu(); },
                                    span { class: "menu-tick", {if spread == Spread::Cover { "✓" } else { "" }} }
                                    span { class: "menu-label", "Two, cover alone" }
                                }
                                div { class: "menu-rule" }
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Recolour pictures too" }
                                    span { class: "menu-row-note", "Off leaves them as printed." }
                                    crate::prefs::Toggle {
                                        on: recolor_images,
                                        onchange: move |on: bool| viewer.write().set_recolor_images(on),
                                    }
                                }
                                div { class: "menu-row",
                                    label { class: "menu-row-label", "Show page count while scrolling" }
                                    span { class: "menu-row-note", "Only when the toolbar is hidden." }
                                    crate::prefs::Toggle {
                                        on: page_pill,
                                        onchange: move |on: bool| viewer.write().set_page_pill(on),
                                    }
                                }
                                div { class: "menu-rule" }
                                button {
                                    class: "menu-item",
                                    onclick: move |_| {
                                        viewer.write().close_menu();
                                        viewer.write().open_settings();
                                    },
                                    span { class: "menu-tick", "" }
                                    Icon { name: "settings", stroke: ink.clone() }
                                    span { class: "menu-label", "All settings…" }
                                    span { class: "menu-key", "{key_settings}" }
                                }
                            }
                        }
                    }
                }
            }
            }
            // **The app's own card, under the toolbar at the right.** It was
            // a row of the flex column, which is simpler and is not what a
            // find bar is: it took forty pixels off the document for as long
            // as it was up, so opening it moved the page being read. Two
            // rows, `.find-row` and `.find-options`, exactly as `index.html`
            // has them — and `top` in a style rather than the app's
            // `#shell[data-toolbar="hidden"]`, because the bar comes up to
            // meet the window's edge when the toolbar is away and there is no
            // shell attribute here to hang a selector off.
            if find_open {
                div { class: "find-bar", style: "top: {find_top}px;",
                // `#find-bar` is the first name in `FIND_KEEPS_OPEN`, and the
                // card sits *below* the toolbar — so without this the root's
                // "a press past the bar puts it away" would fire on the bar's
                // own switches, and Highlight all would close the search it
                // was about to change. Said once here rather than on each of
                // the eight controls inside it.
                onmousedown: move |event| event.stop_propagation(),
                div { class: "find-row",
                    span { class: "find-icon", Icon { name: "search", stroke: faint.clone() } }
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
                            let plain = crate::keymap::plain(modifiers);
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
                    // The count, and the way to the list behind it — see
                    // [`Viewer::show_results`]. A button rather than a
                    // readout, and one that only looks pressable when it has
                    // something to show, which is `.find-status:not(:empty)`
                    // in the app said with a class because Blitz has no `:empty`.
                    button {
                        class: if find_count.is_empty() { "find-count" } else { "find-count ready" },
                        "aria-label": "Show every match",
                        onmousedown: move |event| event.stop_propagation(),
                        onclick: move |_| viewer.write().show_results(),
                        "{find_count}"
                    }
                    // Up and down, which is what the app draws: the matches
                    // are a place in the document rather than a list to walk
                    // left and right along.
                    button {
                        class: "chip icon-only find-previous",
                        "aria-label": "Previous match",
                        onclick: move |_| viewer.write().step_match(false),
                        Icon { name: "up", stroke: ink.clone() }
                    }
                    button {
                        class: "chip icon-only find-next",
                        "aria-label": "Next match",
                        onclick: move |_| viewer.write().step_match(true),
                        Icon { name: "down", stroke: ink.clone() }
                    }
                    // A cross, not a word: closing the find bar finishes
                    // nothing, it puts a thing away.
                    button {
                        class: "chip icon-only find-close",
                        "aria-label": "Close search",
                        onclick: move |_| viewer.write().close_find(),
                        Icon { name: "close", stroke: ink.clone() }
                    }
                }
                // **The three switches, in the app's own order and under the
                // field they belong to.** `#find-highlight`, `#find-case`,
                // `#find-words` in `index.html`: two of them change what is
                // found and the first changes only how much of it is painted,
                // which is the one a reader reaches for most. Each wears a
                // tick whether it is on or not, so turning one on does not
                // shuffle the other two sideways under the pointer.
                div { class: "find-options",
                    button {
                        class: if highlight_all { "find-option find-all on" } else { "find-option find-all" },
                        onclick: move |_| viewer.write().toggle_highlight_all(),
                        Icon { name: "check", stroke: if highlight_all { ink_on.clone() } else { faint.clone() } }
                        "Highlight all"
                    }
                    button {
                        class: if find_options.match_case { "find-option find-case on" } else { "find-option find-case" },
                        onclick: move |_| {
                            let token = viewer.write().set_find_options(crate::search::Options {
                                match_case: !find_options.match_case,
                                whole_words: find_options.whole_words,
                            });
                            scan(token);
                        },
                        Icon { name: "check", stroke: if find_options.match_case { ink_on.clone() } else { faint.clone() } }
                        "Match case"
                    }
                    button {
                        class: if find_options.whole_words { "find-option find-words on" } else { "find-option find-words" },
                        onclick: move |_| {
                            let token = viewer.write().set_find_options(crate::search::Options {
                                match_case: find_options.match_case,
                                whole_words: !find_options.whole_words,
                            });
                            scan(token);
                        },
                        Icon { name: "check", stroke: if find_options.whole_words { ink_on.clone() } else { faint.clone() } }
                        "Whole words"
                    }
                }
                }
            }
            div { class: "body",
            if empty {
                Start { viewer, pick: pick.clone(), frame: frame.clone() }
            }
            if sidebar_open && !presenting && !empty {
                Sidebar {
                    viewer,
                    document: Handle(document.clone()),
                    chosen: chosen.clone(),
                }
            }
            if !empty {
            div {
                class: "viewer",
                onmounted: move |_| resize_from_window(viewer),
                onwheel: move |event| {
                    // **⌃-wheel and ⌘-wheel are zoom, everywhere else and
                    // here.** A mouse with no pinch to offer says it this way,
                    // and so does a trackpad on the platforms where winit
                    // reports a pinch as a modified wheel rather than as a
                    // gesture of its own. The factor is exponential in the
                    // distance so that a fast flick and a slow drag arrive at
                    // the same place per pixel; 320 is the app's own constant.
                    if crate::keymap::command(event.modifiers())
                        || event.modifiers().ctrl()
                    {
                        let down = match event.delta() {
                            WheelDelta::Pixels(delta) => delta.y,
                            WheelDelta::Lines(delta) => delta.y * LINE,
                            WheelDelta::Pages(delta) => {
                                delta.y * viewer.read().layout.viewport.height
                            }
                        };
                        let capped = down.clamp(-60.0, 60.0);
                        viewer.write().zoom_by((capped / 320.0).exp());
                        return;
                    }
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
                            key: "{placed.index}:{placed.drawn.0}x{placed.drawn.1}:{theme_name}:{view_key}:{edition}",
                            document: Handle(document.clone()),
                            chosen: chosen.clone(),
                            index: placed.index,
                            top: placed.top - scroll_top,
                            left: placed.left - scroll_left,
                            width: placed.width,
                            height: placed.height,
                            hits: placed.hits,
                            links: placed.links,
                            notes: placed.notes,
                            selected: placed.selected,
                            swatches: placed.swatches,
                            mark: placed.mark,
                            drawn: placed.drawn,
                            colours: markup_colours.clone(),
                            view,
                            viewer,
                            away: away.clone(),
                        }
                    }
                }
            }
            }
            }
            // **The way back to a toolbar that is not there.** `#toolbar-peek`
            // in the app: nothing is on screen until somebody reaches for the
            // top edge, and then a handle drops in and puts the bar back. The
            // notice that names ⌘T is four seconds long and this is not, which
            // is the difference between a way back and having been told one.
            if !toolbar_on && !presenting && peeking {
                div { class: "peek-line",
                    button {
                        class: if full_screen { "toolbar-peek clear" } else { "toolbar-peek" },
                        onclick: move |_| viewer.write().toggle_toolbar(),
                        Icon { name: "down", stroke: ink.clone() }
                        "Show toolbar"
                    }
                }
            }
            // Where the reader is, while they scroll with the toolbar away.
            // `#page-pill` in the app, and the same two conditions on it —
            // see [`Viewer::flash_pill`].
            if pill_up && !presenting {
                div { class: "pill-line",
                    div { class: "page-pill", "{pill_text}" }
                }
            }
            // The line the toolbar's own way back is written on, which is
            // why it outlives the toolbar. Presenting is the case where
            // nothing is on screen at all.
            if !presenting && !notice.is_empty() {
                // Over the document and centred near its lower edge, which is
                // the app's own `.notice`. Two elements rather than one
                // because centring is done by the outer row: the app reaches
                // the middle with `left: 50%` and a `translateX(-50%)`, and a
                // transform is not something to lean on in Blitz when a flex
                // row does it with no transform at all.
                div { class: "notice-line",
                    div { class: "notice", "{notice}" }
                }
            }
            // What a document being dragged over the window looks like. Over
            // everything but Settings, and outside `.body` so that it covers
            // the panel and the toolbar too: the promise is "anywhere in this
            // window", which is the app's own wording and has to be true of
            // the whole window.
            if let Some(takeable) = dragging {
                div { class: if takeable { "drop-hint" } else { "drop-hint refused" },
                    span { class: "drop-hint-word",
                        {if takeable { "Drop to open" } else { "That is not a PDF" }}
                    }
                }
            }
            // A note, opened. `showNote` in `main.ts` is a window for the
            // reason this one is: a note can be a paragraph, and a tooltip's
            // whole vocabulary is one line that goes away when the pointer
            // does. The sentence at the foot of it is the app's own and is
            // the honest half — this reader shows the notes a document
            // carries and has no way to write one.
            if let Some((_page, note)) = note_open {
                div {
                    class: "window-scrim",
                    onmousedown: move |event| {
                        event.stop_propagation();
                        viewer.write().close_note();
                    },
                    div {
                        class: "window note-window",
                        role: "dialog",
                        "aria-modal": "true",
                        "aria-label": "Note",
                        onmousedown: move |event| event.stop_propagation(),
                        div { class: "window-bar",
                            span { class: "window-title",
                                {if note.by.is_empty() { "Note".to_string() } else { note.by.clone() }}
                            }
                            button {
                                class: "chip window-close",
                                "aria-label": "Close",
                                onclick: move |_| { viewer.write().close_note(); },
                                Icon { name: "close", stroke: ink.clone() }
                            }
                        }
                        div { class: "note-body",
                            p { class: "note-where", "On page {note_page}." }
                            p { class: "note-text", "{note.text}" }
                            p { class: "note-said",
                                "HyloPDF shows the notes a document already carries. It does not write them."
                            }
                        }
                    }
                }
            }
            // What the document says about itself. `showDocumentDetails` in
            // `main.ts`, field for field and in its order — and a window
            // rather than a panel for the reason the note beside it is one:
            // it is read once and dismissed.
            if details_open {
                div {
                    class: "window-scrim",
                    onmousedown: move |event| {
                        event.stop_propagation();
                        viewer.write().close_details();
                    },
                    div {
                        class: "window details-window",
                        role: "dialog",
                        "aria-modal": "true",
                        "aria-label": "Document",
                        onmousedown: move |event| event.stop_propagation(),
                        div { class: "window-bar",
                            span { class: "window-title", "Document" }
                            button {
                                class: "chip window-close",
                                "aria-label": "Close",
                                onclick: move |_| { viewer.write().close_details(); },
                                Icon { name: "close", stroke: ink.clone() }
                            }
                        }
                        div { class: "note-body",
                            p { class: "details-name", "{shelf_name}" }
                            for (label, value) in details_rows {
                                div { class: "details-row", key: "{label}",
                                    span { class: "details-label", "{label}" }
                                    span { class: "details-value", "{value}" }
                                }
                            }
                        }
                    }
                }
            }
            // **The Sign window**, which has no counterpart in the app: see
            // [`crate::sign`]. A window rather than a panel for the reason the
            // two above are windows — it is opened on purpose, answered, and
            // dismissed — and it is the only one in this reader with a
            // *drawing surface* in it.
            if let Some(pad) = signing {
                div {
                    class: "window-scrim",
                    onmousedown: move |event| {
                        event.stop_propagation();
                        viewer.write().close_signing();
                    },
                    div {
                        class: "window sign-window",
                        role: "dialog",
                        "aria-modal": "true",
                        "aria-label": "Sign this document",
                        onmousedown: move |event| event.stop_propagation(),
                        div { class: "window-bar",
                            span { class: "window-title", "Sign this document" }
                            button {
                                class: "chip window-close",
                                "aria-label": "Close",
                                onclick: move |_| { viewer.write().close_signing(); },
                                Icon { name: "close", stroke: ink.clone() }
                            }
                        }
                        div { class: "sign-body",
                            // **The sentence that keeps this honest**, and it
                            // is the first thing in the window rather than a
                            // footnote under it. A reader who wanted the other
                            // kind of signing should find that out before they
                            // have drawn anything.
                            p { class: "pane-lede",
                                "Ink on the page, the way a pen is. It is written into the document and any reader can see it — and it is not a digital signature: it proves nothing about who signed or whether the file has changed since."
                            }
                            // **What is already on this document**, first,
                            // because a reader who opens this window on a
                            // document they have signed is at least as likely
                            // to be here to take one off as to add another.
                            if !signed_here.is_empty() {
                                h3 { class: "pane-group", "On this document" }
                                div { class: "sign-list",
                                    for placed in signed_here {
                                        div {
                                            key: "{placed.page}-{placed.index}",
                                            class: "sign-row",
                                            span { class: "sign-placed",
                                                span { class: "sign-name",
                                                    {if placed.by.is_empty() {
                                                        "Ink".to_string()
                                                    } else {
                                                        placed.by.clone()
                                                    }}
                                                }
                                                span { class: "sign-where", "page {placed.page}" }
                                            }
                                            button {
                                                class: "sign-forget",
                                                "aria-label": "Take this signature off the document",
                                                onclick: {
                                                    let (page, index) = (placed.page, placed.index);
                                                    move |_| { viewer.write().unsign(page, index); }
                                                },
                                                Icon { name: "trash", stroke: ink.clone() }
                                            }
                                        }
                                    }
                                }
                            }
                            if !nothing_kept {
                                h3 { class: "pane-group", "Kept" }
                                div { class: "sign-list",
                                    for entry in kept {
                                        div { key: "{entry.id}", class: "sign-row",
                                            button {
                                                class: "sign-use",
                                                onclick: {
                                                    let entry = entry.clone();
                                                    move |_| viewer.write().sign_with(entry.clone())
                                                },
                                                // The signature itself, drawn
                                                // from the strokes on disk —
                                                // which is the only honest
                                                // preview there is, and it is
                                                // the same arithmetic
                                                // `sign::place` does onto a
                                                // page.
                                                Scrawl { signature: entry.clone(), width: 132.0, height: 44.0 }
                                                span { class: "sign-name", "{entry.name}" }
                                            }
                                            button {
                                                class: "sign-forget",
                                                "aria-label": "Delete this signature",
                                                onclick: {
                                                    let id = entry.id.clone();
                                                    move |_| viewer.write().forget_signature(&id)
                                                },
                                                Icon { name: "trash", stroke: ink.clone() }
                                            }
                                        }
                                    }
                                }
                            }
                            h3 { class: "pane-group",
                                {if nothing_kept { "Draw one" } else { "Or draw another" }}
                            }
                            // **The pad.** A press begins a stroke; the moves
                            // and the release are heard on the root, for the
                            // reason a sweep down the document is — a hand
                            // signing a name leaves the box it started in more
                            // often than not, and the root is the one ancestor
                            // that spans the window.
                            div {
                                class: "sign-pad",
                                // **The size is written here and nowhere
                                // else.** The stylesheet is a `const &str` and
                                // cannot interpolate, so a pad sized in CSS
                                // and read in Rust would be two numbers that
                                // have to agree — and the handler's arithmetic
                                // is wrong by exactly their difference, which
                                // shows up as a signature drawn slightly off
                                // the hand. One source, on the element.
                                style: "width: {PAD_WIDTH}px; height: {PAD_HEIGHT}px;",
                                onmousedown: move |event| {
                                    event.stop_propagation();
                                    let on = event.element_coordinates();
                                    let client = event.client_coordinates();
                                    viewer.write().draw_from(
                                        (on.x, on.y),
                                        (client.x, client.y),
                                        (PAD_WIDTH, PAD_HEIGHT),
                                    );
                                },
                                Scrawl {
                                    signature: crate::sign::Signature {
                                        name: String::new(),
                                        id: String::new(),
                                        // In the pad's own pixels, which is
                                        // what `literal` below draws in.
                                        strokes: pad.strokes.clone(),
                                    },
                                    width: PAD_WIDTH,
                                    height: PAD_HEIGHT,
                                    // The pad draws what was drawn, at the place
                                    // it was drawn — not stretched to its own
                                    // extent, which is what the saved ones are
                                    // shown at and would make the ink jump
                                    // under the hand drawing it.
                                    literal: true,
                                }
                                if pad.strokes.iter().all(|stroke| stroke.is_empty()) {
                                    span { class: "sign-pad-hint", "Draw your name here" }
                                }
                            }
                            crate::prefs::Field { label: "Name",
                                crate::prefs::TextField {
                                    value: pad.name.clone(),
                                    onchange: move |value| {
                                        if let Some(signing) = viewer.write().signing.as_mut() {
                                            signing.name = value;
                                        }
                                    },
                                }
                            }
                            div { class: "pane-actions",
                                button {
                                    class: "chip action",
                                    onclick: move |_| viewer.write().clear_pad(),
                                    "Clear"
                                }
                                button {
                                    class: "chip action primary",
                                    onclick: move |_| { viewer.write().keep_signature(); },
                                    "Keep this signature"
                                }
                            }
                        }
                    }
                }
            }
            // A document that will not open without a password.
            // `ui.askForPassword` in the app, and it is a window there for the
            // reason the two above are: it is the only thing on screen worth
            // attending to, and the answer decides whether there is a document
            // at all.
            if let Some(asking) = locked {
                div {
                    class: "window-scrim",
                    // A press outside is not "not now": the app's own window
                    // has no light dismiss either, and a question with a field
                    // in it is not something to lose by clicking beside it.
                    onmousedown: move |event| event.stop_propagation(),
                    div {
                        class: "window ask-window",
                        role: "dialog",
                        "aria-modal": "true",
                        "aria-label": "This document is locked",
                        onmousedown: move |event| event.stop_propagation(),
                        div { class: "window-bar",
                            span { class: "window-title", "This document is locked" }
                            button {
                                class: "chip window-close",
                                "aria-label": "Close",
                                onclick: move |_| { viewer.write().stop_unlocking(); },
                                Icon { name: "close", stroke: ink.clone() }
                            }
                        }
                        div { class: "ask-body",
                            p { class: "pane-lede",
                                {if asking.wrong {
                                    "That password was not right. Try again."
                                } else {
                                    "It needs a password before it can be opened."
                                }}
                            }
                            // **What is on screen is bullets and what is in
                            // the field is the password.** Blitz reads
                            // `type="password"` — it builds a text editor for
                            // it and gives it the right accessibility role —
                            // and it does not *mask* it, so a field left to
                            // itself would show somebody's password to the
                            // room.
                            //
                            // So the ink is taken away and the bullets are
                            // drawn over the top: `color: transparent` with a
                            // `caret-color` of its own, and a span above it
                            // holding one bullet a character. The attribute is
                            // still `password`, because the accessibility role
                            // is worth having and because the day Blitz masks
                            // it this becomes a bullet under a bullet rather
                            // than a fault.
                            //
                            // **The other way round was tried and is the one
                            // to know about.** Putting the bullets in the
                            // field's own `value` and keeping the password
                            // beside it works until somebody presses a key
                            // twice: `set_text` in `blitz-dom` only touches
                            // the editor when the string it is given differs
                            // from the one it holds, and setting it collapses
                            // the selection to the front — so a masked field
                            // has its caret thrown to offset 0 after every
                            // keystroke and "hylo" is typed in as "olyh". The
                            // page field beside it never sees this because
                            // what it writes back is what was typed, so the
                            // guard skips and the caret stays. And Backspace
                            // could not be intercepted to work around it: on
                            // macOS it is not a keystroke at all but a
                            // `doCommandBySelector:` the editor answers
                            // directly, which no handler here can decline.
                            //
                            // The cost is that the password is in the DOM, in
                            // this window, while the question is up — which is
                            // where a browser keeps it too.
                            span { class: "ask-field-wrap",
                                input {
                                    class: "text-field ask-field",
                                    r#type: "password",
                                    value: "{asking.typed}",
                                    "aria-label": "Password",
                                    "data-keyboard": "password",
                                    onmounted: move |event| {
                                        let node = event.data();
                                        let task = node.set_focus(true);
                                        spawn(async move { let _ = task.await; });
                                    },
                                    oninput: move |event| {
                                        viewer.write().type_password(&event.value());
                                    },
                                    // The same two rules every field in this
                                    // file has — a plain key would otherwise
                                    // scroll the document behind the window —
                                    // plus the two this one is for.
                                    onkeydown: {
                                        let frame = frame.clone();
                                        move |event: KeyboardEvent| {
                                            let plain = !event.modifiers().meta()
                                                && !event.modifiers().ctrl()
                                                && !event.modifiers().alt();
                                            match event.key() {
                                                Key::Enter => {
                                                    event.stop_propagation();
                                                    event.prevent_default();
                                                    if viewer.write().unlock() {
                                                        let path = viewer
                                                            .read()
                                                            .document
                                                            .path()
                                                            .to_string();
                                                        let title = viewer
                                                            .read()
                                                            .store
                                                            .title()
                                                            .to_string();
                                                        frame.ask(Ask::Showing { path, title });
                                                    }
                                                }
                                                Key::Escape => {
                                                    event.stop_propagation();
                                                    viewer.write().stop_unlocking();
                                                }
                                                _ if plain => event.stop_propagation(),
                                                Key::Character(ref typed)
                                                    if matches!(
                                                        typed.as_str(),
                                                        "a" | "c" | "v" | "x" | "z"
                                                    ) => {}
                                                _ => event.prevent_default(),
                                            }
                                        }
                                    },
                                }
                                span { class: "ask-bullets", "{locked_shown}" }
                            }
                            div { class: "pane-actions ask-actions",
                                button {
                                    class: "chip action",
                                    "data-item": "not-now",
                                    onclick: move |_| { viewer.write().stop_unlocking(); },
                                    "Not now"
                                }
                                button {
                                    class: "chip action primary",
                                    "data-item": "unlock",
                                    onclick: {
                                        let frame = frame.clone();
                                        move |_| {
                                            if viewer.write().unlock() {
                                                let path =
                                                    viewer.read().document.path().to_string();
                                                let title =
                                                    viewer.read().store.title().to_string();
                                                frame.ask(Ask::Showing { path, title });
                                            }
                                        }
                                    },
                                    "Open"
                                }
                            }
                        }
                    }
                }
            }
            // And Settings, over all of it. Last in the root so that it is
            // last in paint order too — Blitz paints by the rules, and a
            // scrim that comes before the document is a scrim behind it.
            crate::prefs::Settings { viewer, frame: frame.clone() }
        }
    }
}

/// The window with nothing in it: what a reader sees before there is a
/// document, and after they have put one down.
///
/// **The largest piece of interface this reader did not have**, and its
/// absence had reached into three other places before it was built. ⌘N opened
/// a second window on the document already in front of somebody, because
/// there was nowhere else for a new window to land. A document handed over by
/// the system could never fill an idle window, because no window was ever
/// idle — `Handover::Fill` in `windows.rs` carried a comment saying it was
/// unreachable *until there is a start screen*. And there was no way to close
/// a document at all, only to close the window it was in. All three are
/// answered by there being something to show.
///
/// It is the app's own screen, item for item: the name, one line under it,
/// the button that opens a document, the last six read with the page each was
/// left on, and the sentence saying a document can simply be dropped on the
/// window. The one thing that is not the app's is where it sits — the app
/// lays it over the document region and shows it with a `[data-empty]`
/// selector, which is a webview arrangement for a webview reason (the viewer
/// stays in the DOM and keeps its scroll). Here it *replaces* the document
/// region, which is the same picture and one fewer thing on screen.
#[component]
fn Start(viewer: Signal<Viewer>, pick: Pick, frame: Frame) -> Element {
    // Read once for the render, like everything else the toolbar reads: this
    // is a file on the disk, and the alternative is reading it per row.
    let recents = viewer.read().recents();
    let ink = crate::palette::hex(viewer.read().palette().muted());
    let open = {
        let pick = pick.clone();
        move |_| pick.ask(Opening::Here)
    };
    rsx! {
        div { class: "start",
            div { class: "start-inner",
                h1 { class: "start-name", "HyloPDF" }
                p { class: "start-sub", "A calm place to read." }
                button {
                    class: "start-open",
                    onclick: open,
                    Icon { name: "folder", stroke: crate::palette::hex(viewer.read().palette().background) }
                    "Open a document"
                }
                if !recents.is_empty() {
                    div { class: "recents",
                        div { class: "recents-title", "Recently read" }
                        for entry in recents {
                            div {
                                key: "{entry.path}",
                                class: "recent",
                                button {
                                    class: "recent-open",
                                    // The whole row opens it. The × beside it
                                    // is a button of its own rather than a
                                    // click this one has to tell apart by
                                    // where it landed.
                                    onclick: {
                                        let path = entry.path.clone();
                                        let frame = frame.clone();
                                        move |_| {
                                            let path = path.clone();
                                            if viewer.write().open_here(&path) {
                                                let title = viewer.read().store.title().to_string();
                                                frame.ask(Ask::Showing { path, title });
                                            }
                                        }
                                    },
                                    Icon { name: "document", stroke: ink.clone() }
                                    span { class: "recent-name", "{entry.title}" }
                                    // Where they stopped, in a column of its
                                    // own — on every row rather than only the
                                    // ones past page one, so the list has a
                                    // straight edge. The app's own reasoning,
                                    // and its own words.
                                    span { class: "recent-page", "p. {entry.page}" }
                                }
                                button {
                                    class: "recent-forget",
                                    "aria-label": "Remove from this list",
                                    onclick: {
                                        let path = entry.path.clone();
                                        move |_| viewer.write().forget(&path)
                                    },
                                    Icon { name: "close", stroke: ink.clone() }
                                }
                            }
                        }
                    }
                }
                p { class: "start-hint", "Or drop a PDF anywhere in this window" }
            }
        }
    }
}

/// A signature, drawn.
///
/// **The only thing in this reader that draws a document's own ink**, and it
/// draws it the same way the page will: an SVG `polyline` per stroke, in the
/// same ballpoint blue, with round caps and joins so that a name looks written
/// rather than plotted.
///
/// `literal` is the difference between the pad and the list. A signature in the
/// list is shown stretched to the box it is given, which is what it will look
/// like on the page — `sign::place` scales it to the height it is dropped at
/// and the strokes on disk are already trimmed to their own extent. The pad
/// must not do that: stretching what is being drawn as it is drawn means the
/// ink moves out from under the hand, and the second stroke of a name lands
/// somewhere the first one has just been dragged away from.
#[component]
pub(crate) fn Scrawl(
    signature: crate::sign::Signature,
    width: f64,
    height: f64,
    #[props(default)] literal: bool,
) -> Element {
    // Fitted into the box, or taken as written. **One scale either way**, which
    // is the same rule `sign::place` follows and for the same reason: the
    // strokes are height-normalised, so x and y are in one unit and scaling
    // them differently is what turns a name into a scribble.
    let aspect = signature.aspect();
    let (scale, drawn_width, drawn_height) = if literal {
        // The pad's own space, where a point is already a pixel.
        (1.0, width, height)
    } else {
        // Whichever of the two constraints binds first, so a wide signature in
        // a short box is fitted across rather than clipped.
        let fitted = height.min(width / aspect.max(f64::EPSILON));
        (fitted, fitted * aspect, fitted)
    };
    let across = (width - drawn_width) / 2.0;
    let down = (height - drawn_height) / 2.0;
    let lines: Vec<String> = signature
        .strokes
        .iter()
        .filter(|stroke| !stroke.is_empty())
        .map(|stroke| {
            stroke
                .iter()
                .map(|point| {
                    format!(
                        "{:.2},{:.2}",
                        across + point[0] * scale,
                        down + point[1] * scale
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    rsx! {
        svg {
            class: "scrawl",
            width: "{width}",
            height: "{height}",
            view_box: "0 0 {width} {height}",
            for (at, points) in lines.iter().enumerate() {
                polyline {
                    key: "{at}",
                    points: "{points}",
                    fill: "none",
                    stroke: crate::sign::INK,
                    "stroke-width": "2",
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                }
            }
        }
    }
}

/// One icon, at the size the chrome wants it.
///
/// **The colour is an attribute rather than `currentColor`, and that is
/// Blitz's shape rather than a choice.** An inline `<svg>` is not laid out as
/// elements here: `construct.rs` takes the subtree's `outer_html` and hands it
/// to usvg, which has no CSS cascade and no idea what `color` the button it
/// sits in resolved to. So the theme's own shade goes in as `stroke`, and
/// what a browser gets for free — an icon that follows its label through
/// hover and `on` — has to be passed down.
#[component]
pub(crate) fn Icon(name: &'static str, #[props(default)] stroke: Option<String>) -> Element {
    // A name nothing draws is nothing drawn, rather than a panic: the table is
    // a copy of `icons.ts` and `tests/icons.rs` is what says the two agree.
    let Some(body) = crate::icons::path(name) else {
        return rsx! {};
    };
    let stroke = stroke.unwrap_or_else(|| "currentColor".to_string());
    rsx! {
        svg {
            class: "icon",
            view_box: "0 0 24 24",
            width: "16",
            height: "16",
            fill: "none",
            stroke: "{stroke}",
            // And `color`, because two of these icons fill part of themselves
            // with `currentColor` — the theme circle's dark half, and the
            // cog's centre. usvg resolves that against the `color` property on
            // the element or an ancestor and falls back to black, which on a
            // dark theme is a hole in the middle of the icon.
            color: "{stroke}",
            stroke_width: "1.7",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            dangerous_inner_html: "{body}",
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
    /// The notes on this page, in the same space as the links.
    notes: Vec<(Rect, crate::render::Note)>,
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
    /// Where the colour popover goes on this page, when it is on this page.
    ///
    /// It hangs off the page rather than off the window because that is the
    /// space it is positioned in — under the last line of the selection,
    /// which is a rectangle in the page's own box. The app makes a throwaway
    /// element over the same rectangle and hands it to `showPopover`, for
    /// want of anywhere else to put one.
    swatches: Option<Rect>,
    /// The mark the reader clicked on, when it is on this page: the line they
    /// hit, how to take it out, and the colour it is drawn in. See
    /// [`Viewer::mark_open`].
    mark: Option<(Rect, MarkKey, String)>,
    /// The size this page's texture is drawn at, which is its box except
    /// under a zoom gesture. See [`Viewer::zoom_held_at`].
    drawn: (f64, f64),
    /// The colours it offers. See [`Viewer::markup_colors`].
    colours: Vec<String>,
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
            // The size the texture under this page is drawn at, which is the
            // box's own except while a zoom gesture is running — see
            // [`Viewer::zoom_held_at`]. On the page for the reason
            // `data-page` is: it is the one thing about a page that nothing
            // else in the DOM says, and a test that could not read it would
            // have to photograph the difference between sharp and stretched.
            "data-drawn": "{drawn.0}x{drawn.1}",
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
                // **A signature waiting for somewhere to go takes this press
                // instead of the sweep.** Signing is the one gesture in this
                // reader that turns a click on a page into a write, so it has
                // to be the first thing a press is asked about — a sweep
                // begun here would put the selection down over the very page
                // the reader is aiming at.
                if viewer.read().placing.is_some() {
                    viewer.write().sign_at(index + 1, (on.x, on.y));
                    return;
                }
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
            if let Some(area) = swatches {
                div {
                    class: "markup-popover",
                    // **A press on the swatches must not reach the page**, or
                    // it begins a sweep of its own and puts down the very
                    // selection it is there to mark. The app has the same
                    // problem and answers it the other way round — it
                    // captures the selection when the popover opens and hands
                    // it to the swatches, because in a webview the browser
                    // collapses the selection before any handler runs and
                    // there is nothing to stop. Here the selection is the
                    // reader's own, so stopping the press is enough, and it
                    // is what the menus one layer up already do.
                    onmousedown: move |event| event.stop_propagation(),
                    // Under the line it is about, which is where the app puts
                    // it — and it took a fix there to get it: the anchor
                    // element had no height, so `getBoundingClientRect` put
                    // the swatches straight over the words they were about.
                    // Here the rectangle is the line's own, so the offset is
                    // simply its height.
                    style: "position: absolute; top: {area.top + area.height + 8.0}px; left: {area.left}px;",
                    for colour in colours.iter() {
                        button {
                            key: "{colour}",
                            class: "markup-swatch",
                            "data-colour": "{colour}",
                            "aria-label": "Mark in {colour}",
                            style: "background: {colour};",
                            onclick: {
                                let colour = colour.clone();
                                move |_| {
                                    let restarted = viewer.write().mark_selection(&colour);
                                    rescan(viewer, restarted);
                                }
                            },
                        }
                    }
                }
            }
            // **A mark clicked on says how to take it off.** Removal has
            // worked since the day markup landed — `FPDFPage_RemoveAnnot`,
            // eleven lines, and gone from the file for every reader of it —
            // and it was reachable only from a × the width of a full stop, on
            // a row in a panel that does not open on the tab the row is on.
            // A reader who marked a passage and wanted it gone had nothing to
            // click. See [`Viewer::mark_open`], and `end_sweep`, which is
            // what decides that a press was a click rather than a sweep.
            if let Some((area, key, colour)) = mark {
                div {
                    class: "mark-popover",
                    // The same rule the swatches have, and for the same
                    // reason: a press in here must not reach the page and
                    // begin a sweep of its own — which would take this very
                    // popover down again on the way.
                    onmousedown: move |event| event.stop_propagation(),
                    style: "position: absolute; top: {area.top + area.height + 8.0}px; left: {area.left}px;",
                    span { class: "mark-dot", style: "background: {colour};" }
                    button {
                        class: "mark-remove",
                        onclick: move |_| {
                            let restarted = viewer.write().remove_markup(&key);
                            viewer.write().close_mark();
                            rescan(viewer, restarted);
                        },
                        "Remove highlight"
                    }
                }
            }
            // **The notes a document already carries, made readable.** pdfium
            // paints an annotation's own appearance into the page, so a sticky
            // note arrives as the little icon it was drawn as — and the words
            // behind it live in the annotation, which nothing was reading. So
            // the icon sat there looking like a button and was not one, which
            // is the app's own sentence about the same fault.
            for (at, (area, note)) in notes.iter().enumerate() {
                {
                    let opening = note.clone();
                    // A marker is pressable all over; a comment over a
                    // highlighted sentence answers on a strip at its right
                    // edge, because covering the sentence would put the words
                    // out of reach of a pointer that wants to select them.
                    let (left, width) = if note.icon {
                        (area.left, area.width)
                    } else {
                        (area.left + area.width, NOTE_EDGE)
                    };
                    let said = if note.by.is_empty() {
                        format!("Note. {}", note.text)
                    } else {
                        format!("Note. {}: {}", note.by, note.text)
                    };
                    rsx! {
                        div {
                            key: "n{at}",
                            class: if note.icon { "note-spot" } else { "note-edge" },
                            role: "button",
                            "aria-label": "{said}",
                            title: "{said}",
                            style: "position: absolute; top: {area.top}px; left: {left}px; width: {width}px; height: {area.height}px;",
                            onclick: move |_| viewer.write().open_note(index + 1, opening.clone()),
                        }
                    }
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

/// The same text, in the form a passage is looked up by. See
/// [`crate::search::fold`].
fn folded(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    crate::search::fold(&chars, false)
        .text
        .into_iter()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "One passage" and "three passages", which is a sentence rather than a
/// count followed by a noun.
fn said_of(many: usize, one: &str, more: &str) -> String {
    if many == 1 {
        format!("1 {one}")
    } else {
        format!("{many} {more}")
    }
}

/// Drive a scan that something restarted, from anywhere in the component.
///
/// The mailbox has a closure of exactly this shape and cannot lend it out —
/// it is spawned inside the task that owns the news — so this is that closure
/// said once more, for the two gestures that reload the document without any
/// news arriving: marking a passage and taking a mark off. See
/// [`Viewer::reopen`], which is what hands back the token.
pub(crate) fn rescan(mut viewer: Signal<Viewer>, token: Option<u64>) {
    let Some(token) = token else { return };
    spawn(async move {
        while viewer.write().scan_slice(token) {
            Breathe::once().await;
        }
    });
}

/// One handler per action, and a dispatch of about thirty lines — which is
/// what `main.ts` has, and for the same reason: the table decides *which*
/// action, so nothing here has to know anything about keys.
///
/// **There are no arms missing, and that is new.** Every action in the app's
/// table is carried across whether or not this reader can do it, and for most
/// of Phase 3 the ones it could not do fell through to a catch-all saying so
/// — which turned the keyboard into a live list of what was left. The list is
/// empty: all forty-three of the app's actions answer here, and the three
/// that took longest than the rest were dark mode, help and print, which are
/// the three that are about something outside the document. The catch-all is
/// gone with them, so an action added to [`crate::keymap`] and not handled
/// here is now a compile error rather than a sentence in the notice line.
fn perform(
    mut viewer: Signal<Viewer>,
    action: Action,
    screen: f64,
    frame: &Frame,
    clip: &Clip,
    pick: &Pick,
    printer: &Printer,
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
        Action::Dark => viewer.write().toggle_dark(),
        // F1 and ⌘/, and the app's own answer to both: the Keyboard page,
        // which is the list of everything this reader answers to and is drawn
        // from the keymap rather than from a list of its own. "Help" behind a
        // cog is a strange place to keep the answer to "what can this thing
        // do", which is why it is a key at all.
        Action::Help => viewer.write().show_pane(Pane::Keyboard),
        // ⌘P, and it prints nothing: the document is handed to a program that
        // does. See [`Printer`].
        Action::Print => {
            let path = viewer.read().document.path().to_string();
            if path.is_empty() {
                return;
            }
            if let Err(said) = printer.print(&path) {
                viewer.write().notice = said;
            }
        }
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
            // The colour popover, which is a menu in everything but name and
            // sits in the same place in this list.
            if viewer.write().close_markup() {
                return;
            }
            // …and the one a mark clicked on puts up, which is the same kind
            // of thing in the same place. See [`Viewer::mark_open`].
            if viewer.write().close_mark() {
                return;
            }
            // Settings next, and above everything below it: it is a window
            // over the reader, and Escape inside a window means that window.
            // Below the menus because a menu opened *from* Settings is inside
            // it, which is the same "outward, in the order the reader arrived"
            // this whole list is.
            if viewer.write().close_settings() {
                return;
            }
            // And a note, which is a window of the same kind one line down:
            // it is over the reader, and Escape inside a window means that
            // window.
            if viewer.write().close_note() {
                return;
            }
            // The Information window, which is the note's neighbour in every
            // other respect and was missing from this list: Escape closed the
            // note beside it and left this one up. `showDocumentDetails` puts
            // up the same kind of window and the app's modal closes on Escape
            // whatever is in it.
            if viewer.write().close_details() {
                return;
            }
            // The Sign window, and then a signature that is looking for
            // somewhere to go. Two arms rather than one, and in this order,
            // because they are two states and the reader is in one of them:
            // the window is over the page, so Escape means the window; and a
            // signature waiting to be placed has no window at all, so Escape
            // there is the only way to put it down without signing something.
            if viewer.write().close_signing() {
                return;
            }
            if viewer.write().put_down() {
                return;
            }
            // And the password window. Below the rest because a reader inside
            // it is inside a field, so this arm is only reached when the
            // pointer has taken the keyboard somewhere else — the field's own
            // Escape is what usually answers. Withdrawing the question is not
            // answering it with an empty password: see
            // [`Viewer::stop_unlocking`].
            if viewer.write().stop_unlocking() {
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
        Action::Open => pick.ask(Opening::Here),
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
        Action::Settings => viewer.write().open_settings(),
        // ⌘⇧H. The key opens the popover rather than marking in the last
        // colour used, which is the app's arrangement: a colour is a decision
        // and a keystroke that made one silently would be a keystroke nobody
        // could undo.
        Action::Markup => {
            if !viewer.write().open_markup() {
                let held = viewer.read();
                let nothing = held.document.text_of(held.page() - 1).is_empty();
                drop(held);
                viewer.write().notice = if nothing {
                    "There is no text in this document to mark.".into()
                } else {
                    "Select something first, and this marks it.".into()
                };
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
