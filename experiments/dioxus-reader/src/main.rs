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

use std::rc::Rc;
use std::sync::Arc;

use dioxus_reader::app::Config;
use dioxus_reader::app::CHROME;
use dioxus_reader::emit::{AppHandle, Emitter, Exchange};
use dioxus_reader::session::Session;
use dioxus_reader::shell::{Remote, Shell};
use dioxus_reader::windows::Desk;
use dioxus_reader::{render, stats, store, watch};

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
    // One reader at a time, and a second launch hands its document to the one
    // that is running rather than becoming a second one. A measuring run is
    // exempt: the numbers are taken by launching this binary repeatedly, and
    // a run that quietly handed its work to a reader somebody left open would
    // measure nothing and say it had. See `single.rs`.
    let door = if measuring {
        dioxus_reader::single::Claim::Alone
    } else {
        dioxus_reader::single::claim(&config.dir, named.as_deref())
    };
    if matches!(door, dioxus_reader::single::Claim::Second) {
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
    let path = session.first().cloned().unwrap_or_else(|| {
        format!(
            "{}/../../tests/fixtures/book.pdf",
            env!("CARGO_MANIFEST_DIR")
        )
    });

    // Opened once here for the message below and then dropped: the window
    // opens it again through `Session::window`, which is the one path a
    // window's document comes down. Two opens of the same file cost the same
    // milliseconds twice and buy a launch that says what went wrong before a
    // window exists to say it in.
    match render::open(&path) {
        Ok(document) => println!(
            "reader: {} pages in {path}, opened in {:.0}ms | {:.0}MB resident before any window",
            document.pages(),
            document.opened_in(),
            stats::rss_mb(),
        ),
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
    }

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
        dioxus_reader::config::themes_dir(),
    ));
    let session_maker = Rc::new(Session {
        desk: desk.clone(),
        exchange: exchange.clone(),
        watching: watching.clone(),
        dir: config.dir.clone(),
        theme: config.theme,
        size: (window_width, window_height),
        remote: windows.remote(),
    });

    // The launch window, and then the rest of the last session beside it.
    // They are queued rather than made: a window can only be built from
    // inside a winit callback, and `can_create_surfaces` is the first one.
    // Each is placed as it is made, so the second cascades off the first —
    // the app has to remember the spots instead, because showing a window on
    // macOS moves it and its windows are shown later.
    if let Some(spec) = session_maker.window(&path) {
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
        shell.on_resized(move |label| {
            let _ = handle.emit_to(label, "window-resized", ());
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
    if let dioxus_reader::single::Claim::First(listener) = door {
        dioxus_reader::single::serve(listener, windows.remote());
    }
    #[cfg(target_os = "macos")]
    dioxus_reader::dock::install(windows.remote());

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
    // The socket goes with the process it stood for.
    if !measuring {
        dioxus_reader::single::release(&config.dir);
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
