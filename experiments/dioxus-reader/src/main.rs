//! The reader, run.
//!
//! ```text
//! cargo run --release                          # the 400-page fixture
//! cargo run --release -- ~/paper.pdf           # a document of your own
//! cargo run --release -- --measure 60          # read it, and say what it cost
//! ```
//!
//! With no path it opens whatever was open when the reader was last put down,
//! and `tests/fixtures/book.pdf` from the app beside it when there was
//! nothing. `reopen_last_document = false` in `settings.toml` turns that off,
//! which is the app's own setting — and so does `--measure` or `--quit`,
//! because every number in `PROGRESS.md` was taken on that fixture and a
//! measuring run that quietly used a different document would not be
//! comparable with any of them. A path that is not there is said so plainly
//! rather than being handed to pdfium, which reports it as a Debug-printed
//! `io::Error`.
//!
//! `--measure N` scrolls through N screenfuls on its own and prints what the
//! session cost — pages drawn, milliseconds each, texture resident, RSS. That
//! is the table the whole proposal is judged on, and it is in the binary
//! rather than in a script so that the numbers come from the thing being
//! measured.

use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_native::{LogicalSize, WindowAttributes};
use dioxus_reader::app::CHROME;
use dioxus_reader::app::{Config, Handle, Reader, ReaderProps};
use dioxus_reader::page::Chosen;
use dioxus_reader::palette;
use dioxus_reader::shell::{Remote, Shell, WindowSpec};
use dioxus_reader::store;
use dioxus_reader::{render, stats};

/// The last part of a path, which is what a document is called when it does
/// not call itself anything.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str, fallback: usize| -> usize {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(fallback)
    };
    let measure = flag("--measure", 0);
    // `--quit N` ends the run after N seconds, which is how the floor — a
    // window with one page in it and nobody touching it — gets measured.
    let quit_after = flag("--quit", 0);
    // The window's size is a flag because the floor is the number this whole
    // experiment is judged against, and a GPU renderer's buffers scale with
    // the surface: "how much of the floor is the window" is answerable only by
    // asking for two of them.
    let window_width = flag("--width", 1100) as f64;
    let window_height = flag("--height", 900) as f64;
    // `--theme N` is a place in the theme list, and the list is fourteen long
    // rather than two now: it is read out of the app's own `themes/` files,
    // through the app's own loader. Absent means whatever the last run wore.
    let theme = args
        .iter()
        .position(|arg| arg == "--theme")
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse::<usize>().ok());
    let named = args
        .iter()
        .skip(1)
        .find(|arg| arg.ends_with(".pdf"))
        .cloned();
    let config = Config {
        theme,
        ..Config::here()
    };
    // A document named on the command line, else what was open last, else the
    // fixture. `reopening` is where the pruning and the setting are asked —
    // see `store::reopening`: a document that has been moved or deleted would
    // otherwise be reopened, and fail, on every launch for ever.
    //
    // **A measuring run does not restore**, and that is the one rule worth
    // stating. `--measure` and `--quit` exist to produce the table this whole
    // proposal is judged on, and every number in it was taken on the 400-page
    // fixture; a machine that had read something else would quietly measure
    // that instead and the columns would stop being comparable. Naming a path
    // still measures whatever is named, which is the deliberate version of the
    // same thing.
    let measuring = measure > 0 || quit_after > 0;
    let path = named
        .clone()
        .or_else(|| {
            if measuring {
                None
            } else {
                store::reopening(&config.dir)
            }
        })
        .unwrap_or_else(|| {
            format!(
                "{}/../../tests/fixtures/book.pdf",
                env!("CARGO_MANIFEST_DIR")
            )
        });

    let document: Arc<dyn render::PageSource> = match render::open(&path) {
        Ok(document) => document,
        Err(err) => {
            eprintln!("{err}");
            // The one mistake worth a second sentence, because the documented
            // invocation used to be `-- book.pdf` and the fixture is not in
            // the directory cargo is run from.
            if named.is_some() && !std::path::Path::new(&path).exists() {
                eprintln!(
                    "Run it with no path at all to open the 400-page fixture the numbers were taken on."
                );
            }
            std::process::exit(1);
        }
    };
    println!(
        "reader: {} pages in {path}, opened in {:.0}ms | {:.0}MB resident before any window",
        document.pages(),
        document.opened_in(),
        stats::rss_mb(),
    );

    let event_loop = blitz_shell::create_default_event_loop();
    let (proxy, queue) = blitz_shell::BlitzShellProxy::new(event_loop.create_proxy());
    let mut shell = Shell::new(proxy, queue);
    shell.trace = false;
    let windows = shell.windows();

    // Black on white until the reader's own theme is read, which happens in
    // `Viewer::new` during the first render — before anything is painted, so
    // this is never seen. It is deliberately not any theme's colours: a
    // half-applied theme is harder to diagnose than one that plainly did not
    // load.
    let chosen = Chosen::new(palette::FALLBACK);
    // The window wears the document's name, decided the same way the toolbar
    // decides it — see `store::worth_calling`. It is settled here rather than
    // changed once the reader has read the file because the window's title is
    // an attribute given to the builder: changing it afterwards is a call on
    // the winit window, which is item 9's, and there is nothing to gain by
    // waiting when pdfium answers at open.
    let name = file_name(&path);
    let declared = document.title();
    let called = if store::worth_calling(&declared, &name) {
        declared.trim().to_string()
    } else {
        name
    };
    let attributes = WindowAttributes::default()
        .with_title(format!("{called} — HyloPDF"))
        .with_surface_size(LogicalSize::new(window_width, window_height));
    let vdom = VirtualDom::new_with_props(
        Reader,
        ReaderProps {
            document: Handle(document),
            chosen,
            config,
        },
    );
    windows.open(WindowSpec::new(vdom, attributes));

    if measure > 0 {
        drive(windows.remote(), measure, window_height - CHROME);
    }
    if quit_after > 0 {
        let remote = windows.remote();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(quit_after as u64));
            println!("idle: {}", stats::line());
            remote.quit();
        });
    }

    event_loop.run_app(shell).unwrap();
    // Where the reader got to, if the scribe is still holding it. Everything
    // else this reader remembers is written as it changes; a position is
    // written when the scrolling stops, and quitting is the one way to stop
    // scrolling that does not wait. See `store::flush`.
    store::flush();
    println!("reader: {}", stats::line());
}

/// Read the document without anybody sitting in front of it.
///
/// A thread that sends the window made-up wheel events — the same ones winit
/// would send if somebody were pushing a trackpad — one screenful at a time,
/// with enough of a pause between them for the pages to actually be drawn.
/// Nothing is taken from the machine: the events go into the window through
/// the shell, not through the system, so this runs with the window behind
/// whatever the reader is doing.
fn drive(remote: Remote, screens: usize, screen: f64) {
    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};

    std::thread::spawn(move || {
        let pause = std::time::Duration::from_millis(120);
        // Where the pointer is decides which element a wheel is aimed at, and
        // nothing has moved it yet: the middle of the window is over the
        // document.
        std::thread::sleep(std::time::Duration::from_millis(600));
        remote.inject(WindowEvent::PointerMoved {
            device_id: None,
            position: PhysicalPosition::new(550.0, 500.0),
            primary: true,
            source: winit::event::PointerSource::Mouse,
        });
        std::thread::sleep(pause);

        for screenful in 1..=screens {
            remote.inject(WindowEvent::MouseWheel {
                device_id: None as Option<DeviceId>,
                delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -(screen * 0.9))),
                phase: TouchPhase::Moved,
            });
            std::thread::sleep(pause);
            if screenful % 10 == 0 {
                println!("measure: {screenful} screens | {}", stats::line());
            }
        }
        println!("measure: done | {}", stats::line());
        remote.quit();
    });
}
