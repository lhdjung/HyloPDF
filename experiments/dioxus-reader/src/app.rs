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
//! handler `main.ts` has, including for a page, which cannot be focused. The
//! winit route would be `use_window_event`, and it is closed to us: it takes
//! its `WindowEventHandlers` out of a context that only `dioxus_native`'s own
//! application provides, and the type is private, so a shell of our own cannot
//! provide one. That is a third thing on the list `FINDINGS.md` keeps of what
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
//! fling, which is a real loss and is written down in `FINDINGS.md` rather
//! than papered over: a scrollbar we would have to draw, and momentum arrives
//! from the trackpad in the event stream regardless.

use std::sync::Arc;

use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use dioxus_native::{use_window, CustomWidgetAttr};

use crate::layout::{Anchor, Fit, Layout, Size, Spread};
use crate::page::{Chosen, PageWidget};
use crate::render::PageSource;
use crate::theme::{Theme, THEMES};

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

/// The chrome above and below the document, which the viewport is the window
/// minus. The app measures this off the elements; here it is stated, because
/// there is no `ResizeObserver` and no `get_client_rect` that can be called
/// safely from an event — see `resize_from_window` below.
pub const CHROME: f64 = 46.0 + 30.0 + 2.0;

/// How far one press of an arrow moves the page.
const LINE: f64 = 60.0;

/// The zoom ladder, in the app's own steps.
const ZOOMS: [f64; 13] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0,
];

/// Everything the reader is looking at, and everything that changes it.
pub struct Viewer {
    pub document: Arc<dyn PageSource>,
    pub layout: Layout,
    pub scroll_top: f64,
    pub theme: usize,
    chosen: Chosen,
    /// One line at the bottom of the window, which is `notice()` in `ui.ts`.
    pub notice: String,
    /// Bumped whenever every page has to be drawn again. It is not in the
    /// texture's key — the widget compares sizes and themes itself — but the
    /// components have to be told that something they cannot see has moved.
    pub generation: u64,
}

impl Viewer {
    pub fn new(document: Arc<dyn PageSource>, chosen: Chosen) -> Self {
        let sizes = (0..document.pages())
            .map(|index| document.size_of(index))
            .collect();
        Viewer {
            layout: Layout::new(sizes),
            scroll_top: 0.0,
            theme: 0,
            notice: String::new(),
            generation: 0,
            chosen,
            document,
        }
    }

    pub fn page(&self) -> usize {
        self.layout.page_at(self.scroll_top)
    }

    pub fn pages(&self) -> usize {
        self.layout.pages()
    }

    pub fn theme(&self) -> Theme {
        THEMES[self.theme]
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
    }

    pub fn set_spread(&mut self, spread: Spread) {
        self.keeping_place(|layout| layout.spread = spread);
    }

    pub fn set_theme(&mut self, theme: usize) {
        self.theme = theme.min(THEMES.len() - 1);
        self.chosen.set(self.theme());
        self.generation += 1;
    }

    pub fn toggle_theme(&mut self) {
        self.theme = (self.theme + 1) % THEMES.len();
        // Every mounted page reads this on its next paint, and the next paint
        // is the frame this change causes. A page already on the GPU is
        // recoloured by a compute pass over it rather than drawn again, which
        // is the whole difference from `keyFor()` carrying the theme.
        self.chosen.set(self.theme());
        self.notice = self.theme().name.to_string();
        self.generation += 1;
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
pub fn Reader(document: Handle, chosen: Chosen, theme: usize) -> Element {
    let mut viewer = use_signal(|| {
        let mut viewer = Viewer::new(document.0.clone(), chosen.clone());
        viewer.set_theme(theme);
        viewer
    });
    let window = use_window();

    // The viewport, taken from the window rather than from the element.
    //
    // `get_client_rect` is the obvious way and it panics: a `MountedData` call
    // borrows the document, and every place a component can call one from — a
    // DOM event handler, a mounted handler — is already inside a borrow of it.
    // The window is the one measurement that costs nothing to ask for, and the
    // chrome above and below the document is a number this file knows. The
    // scroll event carries the real client size, so the first scroll corrects
    // whatever this got wrong.
    let resize_from_window = {
        let window = window.clone();
        move |mut viewer: Signal<Viewer>| {
            let size = window.surface_size();
            let scale = window.scale_factor();
            let width = size.width as f64 / scale;
            let height = size.height as f64 / scale - CHROME;
            viewer.write().resize(width, height.max(120.0));
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

    // Every key the reader can press, in one place. The order of the arms
    // does not decide which one answers — that was the fault the app's own
    // keyboard was rewritten to remove — because a key is matched once.
    let on_key = move |event: KeyboardEvent| {
        let shift = event.modifiers().shift();
        let screen = viewer.read().layout.viewport.height;
        let go = move |to: f64| scroll_to(to);
        let by = move |delta: f64| {
            let to = viewer.read().scroll_by(delta);
            go(to);
        };
        match event.key() {
            Key::ArrowDown => by(LINE),
            Key::ArrowUp => by(-LINE),
            Key::PageDown => by(screen * 0.92),
            Key::PageUp => by(-screen * 0.92),
            Key::Home => go(0.0),
            Key::End => {
                let to = viewer.read().layout.max_scroll();
                go(to);
            }
            Key::Character(ref pressed) => match pressed.as_str() {
                " " if shift => by(-screen * 0.92),
                " " => by(screen * 0.92),
                "j" => by(LINE),
                "k" => by(-LINE),
                "d" => by(screen / 2.0),
                "u" => by(-screen / 2.0),
                "n" => {
                    let to = {
                        let held = viewer.read();
                        held.page_target(held.page() + 1)
                    };
                    go(to);
                }
                "p" => {
                    let to = {
                        let held = viewer.read();
                        held.page_target(held.page().saturating_sub(1).max(1))
                    };
                    go(to);
                }
                "t" => viewer.write().toggle_theme(),
                "0" => viewer.write().set_fit(Fit::Width),
                "9" => viewer.write().set_fit(Fit::Page),
                "s" => {
                    let spread = if viewer.read().layout.spread == Spread::Single {
                        Spread::Cover
                    } else {
                        Spread::Single
                    };
                    viewer.write().set_spread(spread);
                }
                "+" | "=" => viewer.write().zoom(true),
                "-" => viewer.write().zoom(false),
                _ => {}
            },
            _ => {}
        }
    };

    let held = viewer.read();
    let scroll_top = held.scroll_top;
    let wearing = held.theme();
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

    rsx! {
        style { {crate::styles::sheet(&wearing)} }
        div {
            class: "root",
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
                button { class: "chip", onclick: move |_| viewer.write().toggle_theme(), "{wearing.name}" }
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
                    for (index, top, left, width, height) in boxes {
                        Page {
                            // What `keyFor()` is: the page, the size it is
                            // drawn at, and the theme it is wearing. A change
                            // to any of them is a different node, which is
                            // what gives the old texture back — see `page.rs`.
                            key: "{index}:{width}x{height}:{wearing.name}",
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
