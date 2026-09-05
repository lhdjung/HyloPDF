//! The reader, run.
//!
//! ```text
//! cargo run --release                          # what you were reading last
//! cargo run --release -- ~/paper.pdf           # a document of your own
//! cargo run --release -- --measure 60          # read it, and say what it cost
//! ```
//!
//! With no path it opens whatever was open when the reader was last put down,
//! and **the start screen** when there was nothing. `reopen_last_document =
//! false` in `settings.toml` turns the restoring off, which is the app's own
//! setting — and so does `--measure` or `--quit`, which additionally open
//! `tests/fixtures/book.pdf` from the app beside it, because every number in
//! `PROGRESS.md` was taken on that fixture and a measuring run that quietly
//! used a different document, or none, would not be comparable with any of
//! them. A path that is not there is said so plainly rather than being handed
//! to pdfium, which reports it as a Debug-printed `io::Error`.
//!
//! `--measure N` scrolls through N screenfuls on its own and prints what the
//! session cost — pages drawn, milliseconds each, texture resident, RSS. That
//! is the table the whole proposal is judged on, and it is in the binary
//! rather than in a script so that the numbers come from the thing being
//! measured.

use std::rc::Rc;
use std::sync::Arc;

use hylopdf::app::Config;
use hylopdf::app::CHROME;
use hylopdf::emit::{AppHandle, Emitter, Exchange};
use hylopdf::session::Session;
use hylopdf::shell::{Remote, Shell};
use hylopdf::windows::Desk;
use hylopdf::{render, stats, store, watch};

fn main() {
    // Before a document exists, which is what this has to be. See its own
    // comment, and `body` in `styles.rs` for what it buys.
    hylopdf::styles::use_variable_fonts();
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
    // **The window's size is the app's setting, not a number in this file.**
    // It was 1100×900 and never remembered, and that is most of what a reader
    // comparing the two sees as "everything is too small": the app opens at
    // 1280×860 *maximized* (`settings.rs`), so its toolbar has room for the
    // document's name and this one squeezed the name to three letters. The
    // flags still win, because they are what a measuring run asks with — the
    // floor is measured at two sizes on purpose — and a run that quietly
    // adopted whatever size somebody had left the window at would not be
    // comparable with the table in `PROGRESS.md`.
    let remembered = hylopdf::settings::load(&hylopdf::config::config_dir());
    let setting = |key: &str, fallback: f64| -> f64 {
        remembered
            .get(key)
            .and_then(|value| value.as_f64())
            .unwrap_or(fallback)
    };
    let given = |name: &str| args.iter().any(|arg| arg == name);
    let window_width = if given("--width") {
        flag("--width", 1100) as f64
    } else {
        setting("window_width", 1280.0)
    };
    let window_height = if given("--height") {
        flag("--height", 900) as f64
    } else {
        setting("window_height", 860.0)
    };
    // A measuring run is never maximized: the whole point of `--width` is to
    // ask what the floor costs at a named size.
    let window_maximized = !given("--width")
        && !given("--height")
        && remembered
            .get("window_maximized")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
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
    // One reader at a time, and a second launch hands its document to the one
    // that is running rather than becoming a second one. A measuring run is
    // exempt: the numbers are taken by launching this binary repeatedly, and
    // a run that quietly handed its work to a reader somebody left open would
    // measure nothing and say it had. See `single.rs`.
    let door = if measuring {
        hylopdf::single::Claim::Alone
    } else {
        hylopdf::single::claim(&config.dir, named.as_deref())
    };
    if matches!(door, hylopdf::single::Claim::Second) {
        // Quietly and successfully: the document is on its way to a window
        // that already exists, which is what was asked for.
        return;
    }

    // One path per window that was open, in the order the windows were made.
    // A document named on the command line is the launch window's and nothing
    // is restored beside it, which is what naming one means.
    let session: Vec<String> = match (&named, measuring) {
        (Some(path), _) => vec![path.clone()],
        (None, true) => Vec::new(),
        (None, false) => store::reopening_all(&config.dir),
    };
    // **Nothing to open is now the start screen rather than the fixture.**
    // It was the fixture because there was nowhere else for a window with
    // nothing in it to go — a launch on a machine that had never read
    // anything opened a 400-page test document nobody asked for, which is a
    // strange first impression for a reader to make. A measuring run is the
    // exception and keeps the fixture, for the reason it is exempt from the
    // restore above: every number in `PROGRESS.md` was taken on it, and a
    // measuring run of an empty window would measure nothing and say it had.
    let fixture = || {
        format!(
            "{}/tests/fixtures/book.pdf",
            env!("CARGO_MANIFEST_DIR")
        )
    };
    let path = match (session.first(), measuring) {
        (Some(path), _) => Some(path.clone()),
        (None, true) => Some(fixture()),
        (None, false) => None,
    };

    // Opened once here for the message below and then dropped: the window
    // opens it again through `Session::window`, which is the one path a
    // window's document comes down. Two opens of the same file cost the same
    // milliseconds twice and buy a launch that says what went wrong before a
    // window exists to say it in.
    match path.as_deref().map(render::open) {
        Some(Ok(document)) => println!(
            "reader: {} pages in {}, opened in {:.0}ms | {:.0}MB resident before any window",
            document.pages(),
            path.as_deref().unwrap_or_default(),
            document.opened_in(),
            stats::rss_mb(),
        ),
        None => println!(
            "reader: nothing to open — the start screen | {:.0}MB resident before any window",
            stats::rss_mb(),
        ),
        // A locked document is not a launch that failed: the window comes up
        // and asks. See `Session::window_on`.
        Some(Err(render::Refusal::Locked)) => println!(
            "reader: {} is locked — the window will ask for the password | {:.0}MB resident before any window",
            path.as_deref().unwrap_or_default(),
            stats::rss_mb(),
        ),
        Some(Err(err)) => {
            eprintln!("{err}");
            // The one mistake worth a second sentence, because the documented
            // invocation used to be `-- book.pdf` and the fixture is not in
            // the directory cargo is run from.
            let named_missing = named.is_some()
                && path
                    .as_deref()
                    .is_some_and(|path| !std::path::Path::new(path).exists());
            if named_missing {
                eprintln!(
                    "Run it with no path at all to open whatever you were reading last."
                );
            }
            std::process::exit(1);
        }
    }

    // Where the launch window's size waits until the app goes. See the
    // `on_resized` hook below for why it is held rather than written.
    let geometry: Arc<std::sync::Mutex<Option<(f64, f64, bool)>>> =
        Arc::new(std::sync::Mutex::new(None));

    let event_loop = blitz_shell::create_default_event_loop();
    let (proxy, queue) = blitz_shell::BlitzShellProxy::new(event_loop.create_proxy());
    let mut shell = Shell::new(proxy, queue);
    // What the shell says about the windows it makes and closes. Off, because
    // it is a line per window on a run nobody asked to debug — and on with
    // `HYLOPDF_TRACE=1`, which is how "did the second window actually land
    // where it was told to" is answered from a terminal rather than with a
    // ruler on the screen.
    shell.trace = std::env::var_os("HYLOPDF_TRACE").is_some();
    let windows = shell.windows();

    // What the process holds and every window shares: who is showing what,
    // where news goes, and one watcher over the themes directory and every
    // open document. See `session.rs`.
    let desk = Desk::new();
    let exchange = Exchange::new();
    let watching = Arc::new(watch::start(
        AppHandle::new(exchange.clone()),
        hylopdf::config::themes_dir(),
    ));
    let session_maker = Rc::new(Session {
        desk: desk.clone(),
        exchange: exchange.clone(),
        watching: watching.clone(),
        dir: config.dir.clone(),
        theme: config.theme,
        size: (window_width, window_height),
        maximized: window_maximized,
        remote: windows.remote(),
    });

    // The launch window, and then the rest of the last session beside it.
    // They are queued rather than made: a window can only be built from
    // inside a winit callback, and `can_create_surfaces` is the first one.
    // Each is placed as it is made, so the second cascades off the first —
    // the app has to remember the spots instead, because showing a window on
    // macOS moves it and its windows are shown later.
    let launch = match path.as_deref() {
        Some(path) => session_maker.window(path),
        None => session_maker.empty_window(),
    };
    if let Some(spec) = launch {
        windows.open(spec);
    }
    for beside in session.iter().skip(1) {
        if let Some(spec) = session_maker.window(beside) {
            windows.open(spec);
        }
    }

    // Where a window comes from when one is asked for by path alone: the Dock
    // menu, a second launch, ⌘N. See `Session::hand_over`.
    {
        let session = session_maker.clone();
        shell.on_request(move |path| match path {
            Some(path) => session.hand_over(&path),
            None => session.another(),
        });
    }
    {
        let session = session_maker.clone();
        shell.on_close(move |label| session.tidy(label));
    }
    {
        // ⌘O: the window kept its identity and changed what is in it, which
        // is the one thing no other path does — every other document arriving
        // in this app arrives with a window of its own.
        let session = session_maker.clone();
        shell.on_swap(move |label, path| session.showing(label, path));
    }
    {
        let desk = desk.clone();
        shell.on_focus(move |label| desk.focused(label.as_deref()));
    }
    {
        // The window changed size, and the document's layout is the one thing
        // in it that will not hear about that on its own — see
        // `Shell::on_resized`. It goes down the mailbox rather than into the
        // window because a component is the only thing that can read the
        // signal, and news is how a component is reached.
        let handle = AppHandle::new(exchange.clone());
        let geometry = geometry.clone();
        shell.on_resized(move |label, width, height, maximized| {
            let _ = handle.emit_to(label, "window-resized", ());
            // **Geometry belongs to the launch window**, which is the app's
            // own rule and the app's own reason: there is one remembered size
            // and there are several windows, and letting whichever moved last
            // own it makes the number creep, because what it reads back are
            // windows that were themselves cascaded off it. Held rather than
            // written — a drag is a hundred of these, and each write is a
            // whole file — and put down once, on the way out.
            if label == "main" {
                *geometry.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((width, height, maximized));
            }
        });
    }
    {
        // Two fingers on the trackpad, which macOS reports as a gesture rather
        // than as a modified wheel — so an application that listens only for
        // ⌃-wheel does not zoom at all. See `Shell::on_pinch`.
        let handle = AppHandle::new(exchange.clone());
        shell.on_pinch(move |label, delta| {
            let _ = handle.emit_to(label, "pinched", delta);
        });
    }
    {
        // The machine went light or dark. Same shape as the resize above and
        // for the same reason — the event says only that there is a new
        // answer, and the reader asks the window for it. See
        // `Shell::on_theme`.
        let handle = AppHandle::new(exchange.clone());
        shell.on_theme(move |label| {
            let _ = handle.emit_to(label, "appearance-changed", ());
        });
    }
    {
        // A document dragged over a window and let go on it. Everything about
        // it is the window's to answer — the hint it shows, and opening the
        // file through the same `open_here` that ⌘O uses — so all this does is
        // carry winit's word down the mailbox, which is the shape of every
        // other line in this block.
        let handle = AppHandle::new(exchange.clone());
        shell.on_drop(move |label, drag| {
            let _ = match drag {
                hylopdf::shell::Drag::Over(takeable) => {
                    handle.emit_to(label, "drag-over", takeable)
                }
                hylopdf::shell::Drag::Left => handle.emit_to(label, "drag-left", ()),
                hylopdf::shell::Drag::Refused => {
                    handle.emit_to(label, "drag-refused", ())
                }
                hylopdf::shell::Drag::Drop(path) => {
                    handle.emit_to(label, "open-document", path)
                }
            };
        });
    }
    {
        // Raised before the first window of a quit goes, which is the whole
        // of what tells a window closed by the reader from a window closed
        // because the app is going. See `windows::Desk::closing`.
        let desk = desk.clone();
        shell.on_quit(move || desk.leaving());
    }

    // The door, answered for as long as the process lives, and the Dock's own
    // "New Window" beside it — the one route to a second window that does not
    // need this reader to be in front already, which is exactly the moment
    // somebody wants one.
    #[cfg(unix)]
    if let hylopdf::single::Claim::First(listener) = door {
        hylopdf::single::serve(listener, windows.remote());
    }
    #[cfg(target_os = "macos")]
    hylopdf::dock::install(windows.remote());

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
    // How big the window was when the reader put it down, which is how big it
    // comes back. Written here rather than as it changes for the reason above,
    // and not at all for a measuring run, whose size came from a flag.
    if !measuring {
        if let Some((width, height, maximized)) =
            *geometry.lock().unwrap_or_else(|e| e.into_inner())
        {
            let _ = hylopdf::settings::set_many(
                &config.dir,
                vec![
                    ("window_width".into(), serde_json::json!(width)),
                    ("window_height".into(), serde_json::json!(height)),
                    ("window_maximized".into(), serde_json::json!(maximized)),
                ],
            );
        }
    }
    // The socket goes with the process it stood for.
    if !measuring {
        hylopdf::single::release(&config.dir);
    }
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
