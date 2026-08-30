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

use std::sync::Arc;

use std::rc::Rc;

use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use dioxus_native::CustomWidgetAttr;
use serde_json::json;

use crate::keymap::{Action, Keymap, Press};
use crate::layout::{Anchor, Fit, Layout, Size, Spread};
use crate::page::{Chosen, PageWidget};
use crate::palette::Palette;
use crate::render::PageSource;
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

/// The zoom ladder, in the app's own steps.
const ZOOMS: [f64; 13] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0,
];

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
            notice: String::new(),
            keymap,
            pending: String::new(),
            generation: 0,
            chosen,
            document,
            store,
        };
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
        self.layout.relayout();
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
    pub fn resize(&mut self, width: f64, height: f64) {
        if (self.layout.viewport.width - width).abs() < 0.5
            && (self.layout.viewport.height - height).abs() < 0.5
        {
            return;
        }
        let anchor = self.layout.anchor(self.scroll_top);
        self.layout.viewport = Size { width, height };
        self.layout.relayout();
        self.scroll_top = self.layout.scroll_target(anchor);
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
        true
    }

    pub fn page_target(&self, page: usize) -> f64 {
        self.layout.scroll_target(Anchor {
            page: page.clamp(1, self.pages().max(1)),
            offset: 0.0,
        })
    }
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
    let resize_from_window = {
        let screen = screen.clone();
        move |mut viewer: Signal<Viewer>| {
            let (width, height, _scale) = screen.get();
            viewer.write().resize(width, (height - CHROME).max(120.0));
        }
    };

    // Going somewhere is a number changing. Nothing is asked of the engine.
    // The signal is copied into the closure rather than captured by reference,
    // which is what keeps this an `Fn` and lets the key handler build small
    // closures of its own out of it.
    let scroll_to = move |top: f64| {
        let mut viewer = viewer;
        viewer.write().scroll_to(top);
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

    let held = viewer.read();
    let scroll_top = held.scroll_top;
    let wearing = held.palette();
    let theme_name = held.theme_name();
    let mounted = held.layout.mounted(held.scroll_top);
    let content_width = held.layout.content_width();
    let content_height = held.layout.content_height();
    let page = held.page();
    let pages = held.pages();
    let notice = held.notice.clone();
    let zoom = match held.layout.fit {
        Fit::Width => "Fit width".to_string(),
        Fit::Page => "Fit page".to_string(),
        Fit::Actual => format!("{:.0}%", held.layout.zoom * 100.0),
    };
    let boxes: Vec<(usize, f64, f64, f64, f64)> = mounted
        .iter()
        .filter_map(|&index| {
            held.layout
                .box_of(index)
                .map(|page| (index, page.top, page.left, page.width, page.height))
        })
        .collect();
    let document = held.document.clone();
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
            div { class: "toolbar",
                div { class: "title", "{pages} pages" }
                div { class: "spacer" }
                button { class: "chip", onclick: move |_| viewer.write().set_fit(Fit::Width), "{zoom}" }
                button { class: "chip", onclick: move |_| viewer.write().zoom(false), "−" }
                button { class: "chip", onclick: move |_| viewer.write().zoom(true), "+" }
                button { class: "chip", onclick: move |_| viewer.write().next_theme(), "{theme_name}" }
                div { class: "pill", "{page} / {pages}" }
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
                    let to = viewer.read().scroll_by(delta);
                    scroll_to(to);
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
                    for (index, top, left, width, height) in boxes {
                        Page {
                            // What `keyFor()` is: the page, the size it is
                            // drawn at, and the theme it is wearing. A change
                            // to any of them is a different node, which is
                            // what gives the old texture back — see `page.rs`.
                            key: "{index}:{width}x{height}:{theme_name}",
                            document: Handle(document.clone()),
                            chosen: chosen.clone(),
                            index,
                            top: top - scroll_top,
                            left,
                            width,
                            height,
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
/// also the honest answer to a reader who presses ⌘F: search is not there
/// yet, and silence would be indistinguishable from a broken keymap.
fn perform(mut viewer: Signal<Viewer>, action: Action, screen: f64) {
    fn by(mut viewer: Signal<Viewer>, delta: f64) {
        let to = viewer.read().scroll_by(delta);
        viewer.write().scroll_to(to);
    }
    fn to(mut viewer: Signal<Viewer>, top: f64) {
        viewer.write().scroll_to(top);
    }
    fn page(mut viewer: Signal<Viewer>, page: usize) {
        let target = viewer.read().page_target(page);
        viewer.write().scroll_to(target);
    }

    match action {
        Action::ScrollDown => by(viewer, LINE),
        Action::ScrollUp => by(viewer, -LINE),
        Action::HalfScreenDown => by(viewer, (screen - OVERLAP) / 2.0),
        Action::HalfScreenUp => by(viewer, -(screen - OVERLAP) / 2.0),
        Action::ScreenDown => by(viewer, screen - OVERLAP),
        Action::ScreenUp => by(viewer, -(screen - OVERLAP)),
        Action::FirstPage => to(viewer, 0.0),
        Action::LastPage => {
            let bottom = viewer.read().layout.max_scroll();
            to(viewer, bottom);
        }
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
        Action::NextTheme => viewer.write().next_theme(),
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
