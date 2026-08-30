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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyrender::PaintScene;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::Document as _;
use blitz_test_harness::{Harness, HarnessOptions};
use dioxus::prelude::VirtualDom;
use dioxus_core::{provide_context, ScopeId};
use dioxus_native::DioxusDocument;
use keyboard_types::{Code, Key, Modifiers};
use peniko::kurbo::Rect;
use peniko::{Color, Fill};

use crate::app::{Config, Handle, Reader as ReaderComponent, ReaderProps, Screen};
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
            config: scratch_config(),
        }
    }
}

/// What the interface says about itself, read off the interface.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// The page the reader is on, off the pill in the toolbar.
    pub page: usize,
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
}

impl Reader {
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

    pub fn open_with(path: &str, options: Options) -> Self {
        let document = render::open(path).unwrap_or_else(|err| panic!("{err}"));
        Self::over(document, options)
    }

    /// The same, over a document already open — which is how a test opens one
    /// document and drives several readers over it.
    pub fn over(document: Arc<dyn PageSource>, options: Options) -> Self {
        write_keys(&options.config, &options.keys);
        // Corrected in `Viewer::new` during the first render, before anything
        // is painted. See `main.rs`, which does the same.
        let chosen = Chosen::new(palette::FALLBACK);
        let config = Config {
            dir: options.config.clone(),
            theme: options.theme,
        };
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
        let (width, height, scale) = (options.width as f64, options.height as f64, options.scale as f64);
        vdom.in_scope(ScopeId::ROOT, move || {
            provide_context(Screen::fixed(width, height, scale));
        });

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
        self.harness.press_with(key, modifiers);
        self.settle();
    }

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

        let mut down = key_event(key.clone(), KeyState::Pressed, modifiers);
        down.code = code;
        let mut up = key_event(key, KeyState::Released, modifiers);
        up.code = code;
        self.harness.dispatch(UiEvent::KeyDown(down));
        self.harness.dispatch(UiEvent::KeyUp(up));
        self.harness.pump();
        self.settle();
    }

    /// Turn the wheel over the document. Positive reads forwards, which is the
    /// direction the reader thinks in and the opposite of the sign winit uses.
    pub fn wheel(&mut self, by: f64) {
        let (x, y) = (self.width as f32 / 2.0, self.height as f32 / 2.0);
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
        self.settle();
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
        self.settle();
    }

    /// What the interface says about itself.
    pub fn state(&self) -> State {
        let pill = self.harness.text_content(".pill");
        let (page, pages) = pill
            .split_once('/')
            .map(|(page, pages)| {
                (
                    page.trim().parse().unwrap_or(0),
                    pages.trim().parse().unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));
        let chips = self.harness.query_all(".chip");
        let text_of = |nth: usize| {
            chips
                .get(nth)
                .and_then(|node| self.harness.base().get_node(*node).map(|n| n.text_content()))
                .unwrap_or_default()
        };
        let mounted = self
            .harness
            .query_all(".page")
            .into_iter()
            .filter_map(|node| {
                let doc = self.harness.base();
                let node = doc.get_node(node)?;
                // By name rather than through `local_name!`, which only knows
                // the atoms the HTML spec has: `data-page` is ours.
                let attrs = node.attrs()?;
                attrs
                    .iter()
                    .find(|attr| &*attr.name.local == "data-page")?
                    .value
                    .parse()
                    .ok()
            })
            .collect();
        State {
            page,
            pages,
            // The first chip is the fit mode and the last is the theme; see
            // the toolbar in `app.rs`.
            zoom: text_of(0),
            theme: text_of(3),
            notice: self.harness.text_content(".notice"),
            scroll: self
                .harness
                .attr(".pages", "data-scroll")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0),
            mounted,
        }
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

/// "ArrowDown" is a named key and "j" is a character. `keyboard_types` parses
/// the named ones and refuses everything else, which is exactly the split.
fn parse_key(key: &str) -> Key {
    if key.chars().count() == 1 {
        return Key::Character(key.to_string());
    }
    key.parse()
        .unwrap_or_else(|_| panic!("no key called {key:?}"))
}
