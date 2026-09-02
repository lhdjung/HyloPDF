//! A window shell for Dioxus Native, because `launch()` only makes one window.
//!
//! `dioxus_native::launch` builds a `DioxusNativeApplication` around a single
//! `WindowConfig` and hands it to winit. `DioxusNativeApplication::add_window`
//! exists and is public, but it pushes onto `BlitzApplication::pending_windows`,
//! which is drained in `can_create_surfaces()` and nowhere else — and the
//! Dioxus half of the setup (the renderer and window contexts, and
//! `initial_build()`) happens only for the *one* window `launch` created. A
//! second window added that way comes up empty and stays empty.
//!
//! So the shell is ours. It owns `BlitzApplication` directly — its fields are
//! public — and does the per-window Dioxus setup itself. Everything a window
//! needs that `dioxus-native` keeps private (the net provider for `dioxus://`
//! assets, the navigation provider that opens a link in a browser) is small
//! enough to restate; see `nav.rs`.
//!
//! A window can only be created from inside a winit callback, because
//! `event_loop.create_window` wants the `&dyn ActiveEventLoop` that only a
//! callback has. So asking for a window is two things: a `WindowSpec` pushed
//! onto a queue, and a wake-up sent through the shell proxy. The spec carries
//! a `VirtualDom`, which is `!Send`, and the event carries nothing.
//!
//! **Everything a window is asked to do arrives as one of those events, even
//! the things that could be done on the spot.** Closing a window and putting
//! it in full screen are both reached from a Dioxus event handler, which runs
//! inside `View::handle_winit_event` — inside a borrow of the document and
//! inside the shell's own borrow of the window map. Taking the window out of
//! that map from in there cannot be written at all, so the ask is posted and
//! answered on the next turn, where nothing is borrowed. It costs a frame
//! nobody can see and it makes every window verb one shape.
//!
//! What this file deliberately does *not* know is what a window is *for*.
//! There is no document in it, no library and no settings: a window is a
//! virtual DOM, a label and a place, and what has to happen when one goes is
//! a closure somebody else set — see [`Shell::on_close`]. The bookkeeping is
//! `windows.rs` and the wiring is `main.rs`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use blitz_shell::{BlitzApplication, BlitzShellEvent, BlitzShellProxy, View, WindowConfig};
use dioxus_core::{provide_context, ScopeId, VirtualDom};
use dioxus_native::{DioxusDocument, DioxusNativeWindowRenderer, DocumentConfig};
use winit::application::ApplicationHandler;
use winit::dpi::Position;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowId};

/// What a window is made of, before it has one.
pub struct WindowSpec {
    /// What this window is called: `main`, then `reader-1`. The name is the
    /// shell's only interest in it — it is handed back to [`Shell::on_close`]
    /// and given to the window's own [`crate::app::Frame`] — and everything
    /// that name means is `windows.rs`'s.
    pub label: String,
    pub attributes: WindowAttributes,
    /// Where the window should be, applied *after* it is created as well as
    /// through the attributes.
    ///
    /// The app this is a spike for has a comment about exactly this: on macOS
    /// a window given a position by the builder is moved onto the launch
    /// window's frame when it is shown, so every window lands on top of every
    /// other. `Placements` in `lib.rs` puts it back. Whether winit has the
    /// same fault is one of the questions this spike answers, so the position
    /// is set twice and the second time is reported.
    pub position: Option<Position>,
    pub vdom: VirtualDom,
}

impl WindowSpec {
    pub fn new(label: impl Into<String>, vdom: VirtualDom, attributes: WindowAttributes) -> Self {
        Self {
            label: label.into(),
            attributes,
            position: None,
            vdom,
        }
    }

    pub fn at(mut self, position: impl Into<Position>) -> Self {
        self.position = Some(position.into());
        self
    }
}

/// The door a component opens a window through. Available as a context.
#[derive(Clone)]
pub struct Windows {
    queue: Rc<RefCell<Vec<WindowSpec>>>,
    proxy: BlitzShellProxy,
}

/// The wake-up. It carries nothing: the spec is in the queue, because a
/// `VirtualDom` cannot cross a channel that wants `Send + Sync`.
struct Spawn;

/// "Make a window, on this document" — the shape `hand_over` has in the real
/// app, where a document arrives from somewhere with no window of its own to
/// describe. A path and nothing else, so it can be sent from any thread: the
/// Dock menu's item and the single-instance listener are both on one.
///
/// `None` means "a window, and you choose the document", which is what ⌘N is
/// in a reader with no start screen — see [`crate::windows::Desk::hand_over`].
struct Wanted(Option<String>);

/// This window, closed, from inside one of its own event handlers.
struct CloseOne(WindowId);

/// This window is showing a different document now — the path and the name.
///
/// Deferred through the proxy like every other ask, and for the ordinary
/// reason: it arrives from a Dioxus handler, inside a borrow of the very
/// window it is about. What answers it is [`Shell::on_swap`], because the
/// shell does not know what a document is.
struct Swapped(WindowId, String, String);

/// Bring the window with this name forward — what a document that is already
/// open answers with, rather than opening a second copy of itself. See
/// [`crate::windows::Handover::Front`].
struct Show(String);

/// This window, in or out of full screen. Deferred for the reason above: the
/// ask comes from a Dioxus handler, and on macOS the answer is an animation
/// the window is in the middle of being borrowed for.
struct FullScreen(WindowId, bool);

/// Ask every window to close and the app to end.
struct Quit;

/// A window event, made up rather than received.
///
/// `View::handle_winit_event` is public, so a synthetic wheel or key can be
/// handed to a window exactly as winit would have handed it over — no OS
/// involvement, nothing taken from whoever is using the machine, and it works
/// with the window in the background. That is the whole of what
/// `scripts/ui-harness.mjs` does through Playwright today, and it is the seed
/// of what replaces it: `--measure` scrolls a document by sending wheels.
pub struct Inject(pub WindowEvent);

impl Windows {
    pub fn open(&self, spec: WindowSpec) {
        self.queue.borrow_mut().push(spec);
        self.proxy.send_event(BlitzShellEvent::embedder_event(Spawn));
    }

    /// Ask the shell's own factory for the next window, on this document.
    /// Unlike `open`, this needs nothing that is `!Send`, so it can be sent
    /// from another thread — which is what the Dock menu item does, and what
    /// a second launch of the app does through the single-instance socket.
    pub fn request(&self, path: Option<String>) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(Wanted(path)));
    }

    /// A handle that can cross threads, carrying only the proxy.
    pub fn remote(&self) -> Remote {
        Remote {
            proxy: self.proxy.clone(),
        }
    }

    pub fn quit(&self) {
        self.proxy.send_event(BlitzShellEvent::embedder_event(Quit));
    }
}

/// The `Send` half of [`Windows`]: it can ask for a window and it can end the
/// app, and it cannot carry a `VirtualDom`.
#[derive(Clone)]
pub struct Remote {
    proxy: BlitzShellProxy,
}

impl Remote {
    pub fn request(&self, path: Option<String>) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(Wanted(path)));
    }

    /// Bring a window forward by name.
    pub fn show(&self, label: &str) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(Show(label.to_string())));
    }

    /// Hand a made-up window event to whichever window is in front.
    pub fn inject(&self, event: WindowEvent) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(Inject(event)));
    }

    pub fn quit(&self) {
        self.proxy.send_event(BlitzShellEvent::embedder_event(Quit));
    }
}

/// Where a window comes from, asked by path. `None` back means no window
/// after all — a document that will not open.
type Factory = Box<dyn FnMut(Option<String>) -> Option<WindowSpec>>;

/// What a window gives back when it goes, by name. See [`Shell::on_close`].
type Tidy = Box<dyn FnMut(&str)>;

/// A window's name and the document it has swapped to. See [`Shell::on_swap`].
type Swap = Box<dyn FnMut(&str, &str)>;

/// A window's name, said again every time the window changes size. See
/// [`Shell::on_resized`].
type Resized = Box<dyn FnMut(&str, f64, f64, bool)>;
/// That two fingers moved apart or together on a window. See [`Shell::on_pinch`].
type Pinched = Box<dyn FnMut(&str, f64)>;

/// A window's name, said again every time the machine goes light or dark. See
/// [`Shell::on_theme`].
type Themed = Box<dyn FnMut(&str)>;

/// A file dragged over a window, or let go over one. See [`Shell::on_drop`].
type Dropped = Box<dyn FnMut(&str, Drag)>;

/// What is happening with a file over a window.
///
/// The paths winit hands over are filtered to documents this reader can open
/// before they get here, so `Over(true)` is "this will be caught" and
/// `Over(false)` is "this will not" — which is the difference between a hint
/// that means something and a hint that appears for every drag across the
/// screen. `Drop` carries the one path that is going to be opened, because a
/// reader dropping four files on one window means one document and three they
/// will have to drop again; the app takes the first the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drag {
    /// Over the window, and whether there is a document in it.
    Over(bool),
    /// Gone, without anything being let go.
    Left,
    /// Let go, on this document.
    Drop(String),
    /// Let go, on something this reader will not open. A case of its own
    /// rather than [`Drag::Left`] with the hint taken down, because the reader
    /// did something and deserves to be told why nothing happened — "That is
    /// not a PDF" is the app's own sentence for it.
    Refused,
}

/// Whether a dragged path is something this reader would open.
///
/// By extension and nothing else. The alternative is opening the file to find
/// out, and this runs on the thread drawing the window every time a pointer
/// carrying a file crosses it.
fn is_document(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

pub struct Shell {
    inner: BlitzApplication<DioxusNativeWindowRenderer>,
    proxy: BlitzShellProxy,
    windows: Windows,
    /// Where a window comes from when nobody handed one over: one place that
    /// makes them, which is what `spawn_window` is today. It answers `None`
    /// when there is to be no window after all — a document that will not
    /// open, or a picker the reader closed.
    factory: Option<Factory>,
    /// What each window is called, so that a `WindowId` arriving from winit
    /// can be handed to [`Shell::on_close`] as a name.
    labels: std::collections::HashMap<WindowId, String>,
    /// What to do when a window goes: give back its place in the library, its
    /// mailbox and its document watch. The shell knows none of that and does
    /// not want to — see the module comment.
    tidy: Option<Tidy>,
    /// What a window has swapped its document for, by name. See
    /// [`Shell::on_swap`].
    swap: Option<Swap>,
    /// Which window has the keyboard, reported as winit says so.
    focus: Option<Box<dyn FnMut(Option<String>)>>,
    /// That a window changed size. See [`Shell::on_resized`].
    resized: Option<Resized>,
    /// That two fingers moved apart or together on it.
    pinched: Option<Pinched>,
    /// That the machine went light or dark. See [`Shell::on_theme`].
    themed: Option<Themed>,
    /// That a document is being dragged over a window, or has been let go
    /// over one. See [`Shell::on_drop`].
    dropped: Option<Dropped>,
    /// Raised before the first window of a quit goes.
    leaving: Option<Box<dyn FnMut()>>,
    /// Whether winit has told us surfaces can be created yet. A window made
    /// before that is left for `BlitzApplication::can_create_surfaces` to
    /// bring up; one made after has to be brought up here, because nothing
    /// else will.
    ///
    /// Resuming a window *twice* is not harmless and cost an afternoon:
    /// `View::resume` builds a fresh renderer, and every resource a widget
    /// registered with the old one is orphaned — the next frame that hands
    /// back a cached texture dies with "Tried to draw an invalid empty image
    /// ... maybe it was registered to a different renderer". Blitz's own
    /// `Widget` trait now has the honest answer to this on the widget's side:
    /// `destroy_surfaces` is where a cached texture is dropped, and
    /// `can_create_surfaces` is where it comes back.
    started: bool,
    /// Reported once per window, so that "where did it actually land" is
    /// answerable from the terminal rather than from a ruler on the screen.
    pub trace: bool,
}

impl Shell {
    pub fn new(proxy: BlitzShellProxy, event_queue: Receiver<BlitzShellEvent>) -> Self {
        Self {
            inner: BlitzApplication::new(proxy.clone(), event_queue),
            windows: Windows {
                queue: Rc::new(RefCell::new(Vec::new())),
                proxy: proxy.clone(),
            },
            proxy,
            factory: None,
            labels: std::collections::HashMap::new(),
            tidy: None,
            swap: None,
            focus: None,
            resized: None,
            pinched: None,
            themed: None,
            dropped: None,
            leaving: None,
            started: false,
            trace: true,
        }
    }

    /// Say where a window comes from when one is asked for by path only.
    pub fn on_request(
        &mut self,
        factory: impl FnMut(Option<String>) -> Option<WindowSpec> + 'static,
    ) {
        self.factory = Some(Box::new(factory));
    }

    /// Say what a window has to give back when it goes.
    ///
    /// Called with the window's label, before the window is taken down —
    /// which is the app's `tidy_after` at one remove, and the remove is the
    /// point: there it is a Tauri window event handler and everything it
    /// touches is `State<'_, …>` off an `AppHandle`; here it is a closure and
    /// what it touches is `windows.rs`.
    pub fn on_close(&mut self, tidy: impl FnMut(&str) + 'static) {
        self.tidy = Some(Box::new(tidy));
    }

    /// Say what has to happen before the app goes: raising the flag that
    /// tells a window closing because of a quit from a window closed by the
    /// reader. See [`crate::windows::Desk::closing`].
    pub fn on_quit(&mut self, leaving: impl FnMut() + 'static) {
        self.leaving = Some(Box::new(leaving));
    }

    /// Say what happens when a window changes size.
    ///
    /// **Blitz answers a resize itself and tells nobody**, which is the whole
    /// of a fault a reader sees as two: `WindowEvent::SurfaceResized` sets the
    /// document's own viewport and asks for a redraw, so the *chrome* follows
    /// the window and the *document* does not — `Viewer::layout` keeps the
    /// viewport it was given when the window was mounted. A window opened at
    /// 1100 and then dragged wider leaves the pages laid out for 1100 inside a
    /// `.viewer` that is now 1600: the page sits left of centre because it is
    /// centred in a `.pages` box narrower than the window, and Fit width fits
    /// a width the window no longer has.
    ///
    /// There is no `ResizeObserver` here and `get_client_rect` cannot be
    /// called from inside an event — see `Reader`'s first comment — so the
    /// news comes the way every other piece of news does, through the window's
    /// mailbox. This is only the half winit can see; `main.rs` is where it is
    /// turned into an emit.
    pub fn on_resized(&mut self, resized: impl FnMut(&str, f64, f64, bool) + 'static) {
        self.resized = Some(Box::new(resized));
    }

    /// Say what happens when two fingers pinch on a window.
    pub fn on_pinch(&mut self, pinched: impl FnMut(&str, f64) + 'static) {
        self.pinched = Some(Box::new(pinched));
    }

    /// **Backspace is a question the window has to have asked to be told the
    /// answer to**, and nothing in this app had asked it.
    ///
    /// See [`ApplicationHandlerExtMacOS`] below for the first half: AppKit
    /// delivers the editing keys as `doCommandBySelector:` rather than as
    /// keystrokes, and that reaches a winit application through
    /// `standard_key_binding`, which this shell forwards. What forwarding it
    /// did not fix is that **winit only reads a keystroke against the
    /// standard key bindings when IME is enabled on the window** — `key_down`
    /// calls `interpretKeyEvents` inside `if ime_capabilities.is_some()`, and
    /// `interpretKeyEvents` is the only thing that ever calls
    /// `doCommandBySelector:`. With IME off, Backspace is a keystroke that
    /// blitz-dom deliberately ignores on this platform and nothing else at
    /// all.
    ///
    /// `blitz-dom` does mean to enable it: `Node::focus` asks the shell for
    /// IME when the node being focused is a text input. **It is asked one
    /// moment too early.** A text input's editor is built by
    /// `create_text_editor` during *layout construction*, and this app focuses
    /// its fields from `onmounted`, which runs before the first layout — so
    /// `text_input_data()` is still `None`, the request is not made, and the
    /// focus never moves away and back to make it again. The find bar, the
    /// go-to-page field and every field in the Settings window could be typed
    /// into and nothing could be taken back out.
    ///
    /// So the window is asked here instead, whenever the focus has moved and
    /// has landed on something being typed into. Nothing turns it off again:
    /// winit's `set_ime_allowed` returns early when IME is already on, so
    /// `ImeRequest::Disable` does nothing — which is upstream's, not ours, and
    /// means "enabled once" is the only state this can reach anyway.
    fn keep_ime_in_step(&mut self, window_id: WindowId) {
        use winit::window::{ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData};
        let Some(view) = self.inner.windows.get(&window_id) else {
            return;
        };
        let editing = {
            let doc = view.doc.inner();
            doc.get_focussed_node_id()
                .and_then(|id| doc.get_node(id))
                .and_then(|node| node.element_data())
                .is_some_and(|data| data.text_input_data().is_some())
        };
        if !editing {
            return;
        }
        if let Some(ask) = ImeEnableRequest::new(ImeCapabilities::new(), ImeRequestData::default()) {
            // `Err(AlreadyEnabled)` is the ordinary answer from the second
            // keystroke onwards, and is nothing to report.
            let _ = view.window.request_ime_update(ImeRequest::Enable(ask));
        }
    }

    /// Say what happens when the machine goes light or dark.
    ///
    /// The app gets this from `matchMedia`, which is a browser answering a
    /// question about the operating system. Here it is winit's
    /// `WindowEvent::ThemeChanged`, and it arrives per window rather than per
    /// process — every window is told, so every window's reader follows,
    /// which is the right shape anyway: the theme is a setting, and a setting
    /// changed in one window is not seen by another until it is opened again
    /// (`AGENTS.md` says so of the app, and this crate inherits it).
    ///
    /// Like a resize, the event carries no answer that is worth carrying: it
    /// says there is a new one, and the reader asks the window through
    /// [`crate::app::Appearance`]. That keeps one place answering the
    /// question — the startup path asks the same way, and a harness answers
    /// both from the same cell.
    pub fn on_theme(&mut self, themed: impl FnMut(&str) + 'static) {
        self.themed = Some(Box::new(themed));
    }

    /// Say what happens when a document is dragged onto a window.
    ///
    /// **The one gesture the start screen advertises and nothing answered.**
    /// "Or drop a PDF anywhere in this window" is the app's own last line of
    /// that screen, and it is a promise; in the app the webview makes it good
    /// through `dragover`/`drop`, and here there is no webview and no DOM
    /// event — winit reports it on the window, which is exactly the right
    /// place for it, because what is being dropped is a *file* and files are
    /// the Rust side's business in both applications.
    ///
    /// Three states rather than one, because the hint is half the gesture: a
    /// window that says nothing while a file is over it gives the reader no
    /// way to know it will be caught. [`Drag::Over`] carries what would be
    /// opened, so a folder or a `.txt` can be turned away before it is let
    /// go rather than after.
    pub fn on_drop(&mut self, dropped: impl FnMut(&str, Drag) + 'static) {
        self.dropped = Some(Box::new(dropped));
    }

    /// Say what happens when a window opens a different document in itself.
    ///
    /// Two things outside the window have to hear about it and neither is the
    /// window's: the desk, which is where the restore list is read from, and
    /// the watch, which is following the file that was open a moment ago.
    /// Both belong to the process — see `session.rs` — which is why this is a
    /// closure here rather than something the reader does for itself.
    pub fn on_swap(&mut self, swapped: impl FnMut(&str, &str) + 'static) {
        self.swap = Some(Box::new(swapped));
    }

    /// Say where "which window is in front" should be written down.
    pub fn on_focus(&mut self, tell: impl FnMut(Option<String>) + 'static) {
        self.focus = Some(Box::new(tell));
    }

    /// Where every window is, in logical pixels — what a new one cascades
    /// past. See [`crate::windows::cascade`].
    pub fn corners(&self) -> Vec<(f64, f64)> {
        self.inner
            .windows
            .values()
            .filter_map(|view| {
                let scale = view.window.scale_factor();
                let at = view.window.outer_position().ok()?.to_logical::<f64>(scale);
                Some((at.x, at.y))
            })
            .collect()
    }

    /// The handle to hand out before the loop starts.
    pub fn windows(&self) -> Windows {
        self.windows.clone()
    }

    /// One step down and across from the window in front, and on again while
    /// the spot is taken. `None` when there is no window to step off, which is
    /// the first one.
    fn next_spot(&self) -> Option<Position> {
        let corner = |view: &View<DioxusNativeWindowRenderer>| {
            let scale = view.window.scale_factor();
            let at = view.window.outer_position().ok()?.to_logical::<f64>(scale);
            Some((at.x, at.y))
        };
        let front = self
            .inner
            .windows
            .values()
            .find(|view| view.window.has_focus())
            .or_else(|| self.inner.windows.values().next())
            .and_then(corner);
        let (x, y) = crate::windows::cascade(front, &self.corners(), None)?;
        Some(winit::dpi::LogicalPosition::new(x, y).into())
    }

    fn drain(&mut self, event_loop: &dyn ActiveEventLoop) {
        let specs: Vec<WindowSpec> = self.windows.queue.borrow_mut().drain(..).collect();
        for spec in specs {
            self.open(event_loop, spec);
        }
    }

    fn open(&mut self, event_loop: &dyn ActiveEventLoop, spec: WindowSpec) {
        // One renderer per window: `DioxusNativeWindowRenderer` is an
        // `Rc<RefCell<VelloWindowRenderer>>` over one surface, and a surface
        // belongs to one window.
        let renderer = DioxusNativeWindowRenderer::new();

        let doc = DioxusDocument::new(
            spec.vdom,
            DocumentConfig {
                navigation_provider: Some(crate::nav::provider()),
                // Without this, `dangerous_inner_html` silently does nothing —
                // which is how every icon in the chrome spike came out blank
                // the first time. `dioxus_native::launch` passes it behind its
                // `html` feature; a shell that makes its own windows has to
                // pass it too.
                html_parser_provider: Some(std::sync::Arc::new(blitz_html::HtmlProvider)),
                ..Default::default()
            },
        );

        let config =
            WindowConfig::with_attributes(Box::new(doc) as _, renderer.clone(), spec.attributes);
        let mut view = View::init(config, event_loop, &self.proxy);
        self.labels.insert(view.window_id(), spec.label.clone());

        // The Dioxus half, which `BlitzApplication` knows nothing about.
        let windows = self.windows.clone();
        let winit_window = std::sync::Arc::clone(&view.window);
        let shell_provider = view.doc.inner().shell_provider.clone();
        // What `use_window()` was reached for and the only thing it was
        // reached for. See `Screen` in `app.rs`: a component that asks winit
        // how big it is cannot be built without winit, and the harness has no
        // window at all.
        let screen = {
            let window = std::sync::Arc::clone(&view.window);
            crate::app::Screen::new(move || {
                let size = window.surface_size();
                let scale = window.scale_factor();
                (
                    size.width as f64 / scale,
                    size.height as f64 / scale,
                    scale,
                )
            })
        };
        // What this window can be asked to do. Every one of them goes back
        // out through the proxy rather than being done here — see the module
        // comment: the ask arrives from inside a borrow of this very window.
        // What the machine says about light and dark. `None` where the
        // platform will not say, which winit allows and this reader answers
        // by leaving the theme alone. See [`crate::app::Appearance`].
        let appearance = {
            let window = std::sync::Arc::clone(&view.window);
            crate::app::Appearance::new(move || {
                window
                    .theme()
                    .map(|theme| theme == winit::window::Theme::Dark)
            })
        };
        let frame = {
            let proxy = self.proxy.clone();
            let id = view.window_id();
            crate::app::Frame::new(move |ask| {
                let event = match ask {
                    crate::app::Ask::NewWindow => BlitzShellEvent::embedder_event(Wanted(None)),
                    crate::app::Ask::Close => BlitzShellEvent::embedder_event(CloseOne(id)),
                    crate::app::Ask::Quit => BlitzShellEvent::embedder_event(Quit),
                    crate::app::Ask::FullScreen(on) => {
                        BlitzShellEvent::embedder_event(FullScreen(id, on))
                    }
                    // The picker's two answers. A window of its own goes
                    // through the same door the Dock menu and a second launch
                    // use, so a document already open is brought forward
                    // rather than opened twice.
                    crate::app::Ask::NewWindowOn(path) => {
                        BlitzShellEvent::embedder_event(Wanted(Some(path)))
                    }
                    crate::app::Ask::Showing { path, title } => {
                        BlitzShellEvent::embedder_event(Swapped(id, path, title))
                    }
                };
                proxy.send_event(event);
            })
        };
        let doc = view.downcast_doc_mut::<DioxusDocument>();
        doc.vdom.in_scope(ScopeId::ROOT, move || {
            provide_context(renderer);
            provide_context(windows);
            provide_context(winit_window);
            provide_context(shell_provider);
            provide_context(screen);
            provide_context(appearance);
            provide_context(frame);
        });
        doc.initial_build();

        // Where it goes: what the spec asked for, else one step down and
        // across from the window in front of it. The cascade is
        // `windows::cascade` and the argument for it is there; what is here
        // is that the shell is the only thing that knows where the windows
        // actually are, and it knows *now* rather than when the spec was
        // made. In the app this is a `Placements` map applied after the
        // window is shown, because showing it on macOS moves it onto the
        // launch window's frame — here the window is made, placed and drawn
        // in one turn and nothing is seen in between.
        let position = spec.position.or_else(|| self.next_spot());
        if let Some(position) = position {
            let before = view.window.outer_position().ok();
            view.window.set_outer_position(position);
            if self.trace {
                let after = view.window.outer_position().ok();
                eprintln!(
                    "shell: window {:?} asked for {:?}; before {:?}, after {:?}",
                    view.window_id(),
                    position,
                    before,
                    after
                );
            }
        }

        let id = view.window_id();
        self.inner.windows.insert(id, view);
        if self.started {
            // A window made after the first `can_create_surfaces` has to be
            // resumed here; the renderer answers with `ResumeReady`, which
            // `BlitzApplication` turns into `complete_resume` once the view is
            // in its map — which is why the insert above happens either way.
            if let Some(view) = self.inner.windows.get_mut(&id) {
                view.resume();
                view.request_redraw();
            }
        }
    }
}

/// **Backspace is not a key on a Mac.**
///
/// AppKit does not deliver the editing keys as keystrokes at all. It reads
/// them against the standard key bindings and calls `doCommandBySelector:`
/// with a name — `deleteBackward:`, `deleteWordBackward:`, `moveToBeginningOfLine:`
/// — and winit surfaces that as [`ApplicationHandlerExtMacOS::standard_key_binding`],
/// which is a *separate* callback from `window_event`. `blitz-dom` knows this
/// and says so: the `Key::Backspace` arm in `node/text.rs` is
/// `#[cfg(not(target_os = "macos"))]`, because on a Mac the command is the
/// only thing that ever arrives.
///
/// `BlitzApplication` implements the callback and returns itself from
/// `macos_handler`. This shell wraps it and implements `ApplicationHandler`
/// itself, so until now it answered `None` — winit's default — and every one
/// of those commands was dropped on the floor. What that looked like: a
/// reader could type into the find bar and could not take anything back out
/// of it, and the same in the go-to-page field and every field in the
/// settings window. Nothing else in the app was affected, because nothing
/// else in the app edits text.
#[cfg(target_os = "macos")]
impl winit::platform::macos::ApplicationHandlerExtMacOS for Shell {
    fn standard_key_binding(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        action: &str,
    ) {
        self.inner.standard_key_binding(event_loop, window_id, action);
    }
}

impl ApplicationHandler for Shell {
    /// See [`ApplicationHandlerExtMacOS`] above: without this, winit answers
    /// `None` and every editing key on a Mac is lost.
    #[cfg(target_os = "macos")]
    fn macos_handler(
        &mut self,
    ) -> Option<&mut dyn winit::platform::macos::ApplicationHandlerExtMacOS> {
        Some(self)
    }

    /// Where a window is actually born. Winit calls this once the platform can
    /// make surfaces, and again after a `destroy_surfaces`; everything queued
    /// so far is brought up by `BlitzApplication`, which resumes every window
    /// it holds.
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Winit calls this more than once — on macOS it comes again after the
        // window is on screen — and `BlitzApplication::can_create_surfaces`
        // resumes *every* window it holds each time. A second resume builds a
        // second renderer, and the first one's registered textures are still
        // in the window renderer's map: the next frame draws an image whose
        // override belongs to a renderer that no longer exists, which Vello
        // reports as "tried to draw an invalid empty image ... maybe it was
        // registered to a different renderer". It is a panic, it lands three
        // frames into every run, and the trail back to here is not short. So
        // the first call is the one that resumes; after it, a new window
        // resumes itself in `open`.
        self.drain(event_loop);
        if !self.started {
            self.inner.can_create_surfaces(event_loop);
            self.started = true;
        }
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.started = false;
        self.inner.destroy_surfaces(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            if self.trace {
                eprintln!(
                    "shell: closing {:?}, {} open",
                    window_id,
                    self.inner.windows.len()
                );
            }
            // Before `BlitzApplication` drops the window, because everything
            // this gives back is asked *of* the window: what it was showing,
            // and the document it was having watched. Afterwards there is a
            // `WindowId` and nothing to look it up in.
            if let Some(label) = self.labels.remove(&window_id) {
                if let Some(tidy) = self.tidy.as_mut() {
                    tidy(&label);
                }
            }
        }
        // Which window has the keyboard, which is what a new window cascades
        // off and what a handed-over document prefers. winit is the only
        // thing that knows, and it says so exactly once per change.
        if let WindowEvent::Focused(gained) = event {
            if let Some(tell) = self.focus.as_mut() {
                let label = gained
                    .then(|| self.labels.get(&window_id).cloned())
                    .flatten();
                tell(label);
            }
        }
        // A click clears the focus off the page, and from that moment every
        // keyboard shortcut goes to `<html>`, which is above anything a
        // component can put a handler on. Giving it back belongs to whoever
        // owns the window because a component cannot do it — the one call
        // that asks for the focus panics from inside an event handler. See
        // `app::KEYBOARD`, which is the whole account, and `harness.rs`,
        // which does this same one line for a window that does not exist.
        // A key as well as a click, because a key can take the focused node
        // away with it: Escape closes the find bar, the field it was typed
        // into stops existing, and the focus goes with it — after which every
        // shortcut in the reader is dead again, which is the same failure one
        // level along.
        let moved_focus = matches!(
            event,
            WindowEvent::PointerButton {
                state: ElementState::Released,
                ..
            } | WindowEvent::KeyboardInput { .. }
        );
        // A resize, which Blitz answers for the chrome and nobody answers for
        // the document. See [`Shell::on_resized`]. The scale factor counts as
        // one: a window dragged to a screen of a different density is the same
        // number of CSS pixels changing under the same layout.
        let resized = matches!(
            event,
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. }
        );
        // …and a two-finger pinch, which is how a trackpad asks to zoom and is
        // not a wheel: macOS reports it as a gesture of its own, so an
        // application listening only for ⌃-wheel hears the opening of the
        // gesture at best and usually nothing at all. Blitz has no DOM event
        // for it, so it goes down the mailbox like the resize.
        let pinched = match event {
            WindowEvent::PinchGesture { delta, .. } if delta.is_finite() && delta != 0.0 => {
                Some(delta)
            }
            _ => None,
        };
        // …and the machine going light or dark, which nothing else in this
        // process hears. See [`Shell::on_theme`].
        let themed = matches!(event, WindowEvent::ThemeChanged(_));
        // …and a document being dragged onto the window. Read *before* the
        // event is handed on, like the two above it, because `window_event`
        // takes the event by value. `DragMoved` is deliberately not answered:
        // it fires for every pixel the pointer travels and says nothing that
        // `DragEntered` has not already said, so answering it would be one
        // emit per frame for the whole of a drag.
        let dragging = match event {
            WindowEvent::DragEntered { ref paths, .. } => {
                Some(Drag::Over(paths.iter().any(|path| is_document(path))))
            }
            WindowEvent::DragLeft { .. } => Some(Drag::Left),
            WindowEvent::DragDropped { ref paths, .. } => paths
                .iter()
                .find(|path| is_document(path))
                .map(|path| Drag::Drop(path.to_string_lossy().into_owned()))
                .or(Some(Drag::Refused)),
            _ => None,
        };
        self.inner.window_event(event_loop, window_id, event);
        if resized {
            // The size goes with the news, because the one thing that wants
            // it outside this file is the setting that remembers it — and
            // `main.rs`, which is where that is written, has no window to
            // ask. Logical, not physical: a setting written in device pixels
            // comes back at half the size on the next screen.
            let geometry = self.inner.windows.get(&window_id).map(|view| {
                let scale = view.window.scale_factor();
                let size = view.window.surface_size();
                (
                    size.width as f64 / scale,
                    size.height as f64 / scale,
                    view.window.is_maximized(),
                )
            });
            if let (Some(label), Some((width, height, maximized)), Some(tell)) = (
                self.labels.get(&window_id).cloned(),
                geometry,
                self.resized.as_mut(),
            ) {
                tell(&label, width, height, maximized);
            }
        }
        if let Some(delta) = pinched {
            if let (Some(label), Some(tell)) =
                (self.labels.get(&window_id).cloned(), self.pinched.as_mut())
            {
                tell(&label, delta);
            }
        }
        if themed {
            if let (Some(label), Some(tell)) =
                (self.labels.get(&window_id).cloned(), self.themed.as_mut())
            {
                tell(&label);
            }
        }
        if let Some(drag) = dragging {
            if let (Some(label), Some(tell)) =
                (self.labels.get(&window_id).cloned(), self.dropped.as_mut())
            {
                tell(&label, drag);
            }
        }
        if moved_focus {
            if let Some(view) = self.inner.windows.get_mut(&window_id) {
                crate::app::give_keyboard_back(&mut view.doc.inner_mut());
                view.request_redraw();
            }
        }
        // Asked on every event rather than only where the focus moved: a
        // field's editor is not built until the layout after it is mounted,
        // and a layout happens on a redraw rather than on anything this sees
        // go past. See [`Shell::keep_ime_in_step`], which does nothing at all
        // unless something is being typed into.
        self.keep_ime_in_step(window_id);
    }

    /// Blitz's events arrive on a channel now and the proxy only says "there
    /// is something on it", so the shell drains the same queue the
    /// application would and takes its own three events out of it.
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.inner.event_queue.try_recv() {
            if let BlitzShellEvent::Embedder(ref payload) = event {
                if payload.downcast_ref::<Spawn>().is_some() {
                    self.drain(event_loop);
                    continue;
                }
                if let Some(wanted) = payload.downcast_ref::<Wanted>() {
                    // Taken out and put back rather than borrowed, because the
                    // factory is `FnMut` and what it does is make a window
                    // through this same shell.
                    if let Some(mut factory) = self.factory.take() {
                        let spec = factory(wanted.0.clone());
                        self.factory = Some(factory);
                        if let Some(spec) = spec {
                            self.open(event_loop, spec);
                        }
                    } else {
                        eprintln!("shell: asked for a window with no factory set");
                    }
                    continue;
                }
                if let Some(Show(label)) = payload.downcast_ref::<Show>() {
                    let id = self
                        .labels
                        .iter()
                        .find(|(_, known)| *known == label)
                        .map(|(id, _)| *id);
                    if let Some(view) = id.and_then(|id| self.inner.windows.get(&id)) {
                        view.window.set_minimized(false);
                        view.window.focus_window();
                    }
                    continue;
                }
                if let Some(Swapped(id, path, title)) = payload.downcast_ref::<Swapped>() {
                    // The window wears the document's name, spelled the way
                    // `Session::window` spells it for a window that was born
                    // on one.
                    if let Some(view) = self.inner.windows.get(id) {
                        // …and wears the application's own name when it is
                        // showing nothing, rather than " — HyloPDF" with a
                        // hole where a document used to be.
                        view.window.set_title(&if title.is_empty() {
                            "HyloPDF".to_string()
                        } else {
                            format!("{title} — HyloPDF")
                        });
                    }
                    if let (Some(label), Some(swap)) =
                        (self.labels.get(id).cloned(), self.swap.as_mut())
                    {
                        swap(&label, path);
                    }
                    continue;
                }
                if let Some(CloseOne(id)) = payload.downcast_ref::<CloseOne>() {
                    // The same path a click on the close button takes, rather
                    // than a second way of closing a window: everything that
                    // has to be given back is hung off `CloseRequested`.
                    self.window_event(event_loop, *id, WindowEvent::CloseRequested);
                    continue;
                }
                if let Some(FullScreen(id, on)) = payload.downcast_ref::<FullScreen>() {
                    if let Some(view) = self.inner.windows.get(id) {
                        view.window.set_fullscreen(
                            on.then_some(winit::monitor::Fullscreen::Borderless(None)),
                        );
                    }
                    continue;
                }
                if let Some(injected) = payload.downcast_ref::<Inject>() {
                    if let Some(view) = self.inner.windows.values_mut().next() {
                        view.handle_winit_event(injected.0.clone());
                    }
                    continue;
                }
                if payload.downcast_ref::<Quit>().is_some() {
                    if let Some(leaving) = self.leaving.as_mut() {
                        leaving();
                    }
                    // Every window closed through the door a window closes
                    // through, so that each of them gives back what it holds
                    // — and whoever raised the flag that says this is a quit
                    // has already done so, which is what stops the session
                    // being forgotten on the way out. See
                    // [`crate::windows::Desk::closing`].
                    let ids: Vec<WindowId> = self.inner.windows.keys().copied().collect();
                    for id in ids {
                        self.window_event(event_loop, id, WindowEvent::CloseRequested);
                    }
                    self.inner.windows.clear();
                    event_loop.exit();
                    continue;
                }
            }
            self.inner.handle_blitz_shell_event(event_loop, event);
        }
        // A field can arrive without a window event to announce it: ⌘F is a
        // keystroke, the bar it opens is rendered on the poll that follows,
        // and the field's editor is not built until the layout after that. So
        // the question is asked again once the queue is drained, and every
        // window is asked because a poll names none. See
        // [`Shell::keep_ime_in_step`].
        let ids: Vec<WindowId> = self.inner.windows.keys().copied().collect();
        for id in ids {
            self.keep_ime_in_step(id);
        }
    }
}
