mod library;
mod settings;
mod theme;
mod watch;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Replace a file's contents without ever leaving a half-written one behind:
/// write beside the target, then rename over it, which is atomic on every
/// system we ship to.
///
/// The temp file is named for this process and this write. Sharing one temp
/// path — which is what `with_extension("toml.tmp")` gave us — meant two
/// writers could overwrite each other's staging file and then rename the wrong
/// bytes into place, or find it already gone and fail. The locks in `settings`
/// and `library` make that unreachable within one process; the unique name
/// makes it unreachable full stop.
pub(crate) fn atomic_write(target: &Path, body: &[u8]) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let dir = target
        .parent()
        .ok_or("That path has no folder to write into.")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let stem = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let ticket = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(".{stem}.{}.{ticket}.tmp", std::process::id()));

    if let Err(e) = std::fs::write(&temp, body) {
        return Err(e.to_string());
    }
    std::fs::rename(&temp, target).map_err(|e| {
        // A failed rename leaves the staging file behind; it is ours and
        // nobody else's, so cleaning it up cannot take anything with it.
        let _ = std::fs::remove_file(&temp);
        e.to_string()
    })
}

/// Resolved once at startup: the config directory and the themes directory
/// inside it.
struct Paths {
    config: PathBuf,
    themes: PathBuf,
}

/// A file waiting to be opened — from the command line, or from the OS asking
/// us to open it while we were still starting up. Once the interface is up it
/// takes documents by event instead, so `ready` decides which route applies.
#[derive(Default)]
struct Pending {
    file: Mutex<Option<String>>,
    listening: AtomicBool,
}

/// The document currently open, held open.
///
/// pdf.js reads a document in pieces — it asks for the cross-reference table,
/// then the pages it actually needs — so the file is opened once and kept,
/// rather than opened and closed for every range. Only the path recorded here
/// can be read, which keeps `read_range` a way of reading the open document
/// rather than a way of reading any file on the disk.
#[derive(Default)]
struct OpenFile(Mutex<Option<(String, File)>>);

impl OpenFile {
    /// Open a document for reading and report its size.
    fn begin(&self, path: &str) -> Result<u64, String> {
        let file =
            File::open(path).map_err(|e| format!("Could not read {}: {e}", file_name(path)))?;
        let length = file
            .metadata()
            .map_err(|e| format!("Could not measure {}: {e}", file_name(path)))?
            .len();
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some((path.to_string(), file));
        Ok(length)
    }

    /// Bytes `[start, start + length)` of the open document.
    fn range(&self, path: &str, start: u64, length: u64) -> Result<Vec<u8>, String> {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some((open, file)) = slot.as_mut() else {
            return Err("No document is open.".into());
        };
        if open != path {
            return Err("That is not the document that is open.".into());
        }
        // A document is never larger than the disk it sits on, and a request
        // for more than that is a bug rather than a big file.
        let length = length.min(64 * 1024 * 1024) as usize;
        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("Could not seek: {e}"))?;
        let mut buffer = vec![0u8; length];
        let mut filled = 0;
        while filled < length {
            match file.read(&mut buffer[filled..]) {
                Ok(0) => break, // end of file: a short read is the honest answer
                Ok(n) => filled += n,
                Err(e) => return Err(format!("Could not read: {e}")),
            }
        }
        buffer.truncate(filled);
        Ok(buffer)
    }

    fn close(&self) {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *slot = None;
    }
}

#[derive(Serialize)]
struct Bootstrap {
    settings: settings::Settings,
    themes: Vec<theme::Theme>,
    library: Vec<library::Entry>,
    config_dir: String,
    themes_dir: String,
}

#[derive(Serialize)]
struct Opened {
    path: String,
    name: String,
    page: u32,
    offset: f64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn file_name(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Every command that touches the disk is `async`, and that is the whole
/// reason for the keyword here — none of them await anything.
///
/// A synchronous Tauri command runs on the thread that received the IPC
/// message, which is the main thread: the one drawing the window. Reading a
/// document, or rewriting `settings.toml`, would stop the app dead for as long
/// as the disk took. `remember_position` alone fires on every pause in a
/// scroll, so that stall would land squarely in the middle of the one gesture
/// this app exists to make smooth. Marked `async`, the body is handed to the
/// runtime's thread pool instead and the window keeps painting.
///
/// The price is that two of them can now genuinely run at once, which is why
/// `settings` and `library` hold a lock across read-modify-write.
#[tauri::command]
async fn bootstrap(paths: State<'_, Paths>) -> Result<Bootstrap, String> {
    let stored = library::load(&paths.config);
    Ok(Bootstrap {
        settings: settings::load(&paths.config),
        themes: theme::load_all(&paths.themes),
        library: library::prune(&stored).files,
        config_dir: paths.config.to_string_lossy().to_string(),
        themes_dir: paths.themes.to_string_lossy().to_string(),
    })
}

/// Settings, written as a group.
///
/// Every write is still one key changing one entry; what arrives together is
/// simply written together. The interface changes settings in pairs more often
/// than not — a theme and the light or dark slot it fills, a zoom and its fit
/// mode — and sending those as two commands meant two whole-file rewrites that
/// each had to re-read what the other had just done.
#[tauri::command]
async fn set_settings(
    paths: State<'_, Paths>,
    entries: Vec<(String, Value)>,
) -> Result<settings::Settings, String> {
    settings::set_many(&paths.config, entries)
}

/// The window's geometry is one observation of one window, so it is written in
/// one go, like everything else.
// Async for a second reason as well as the one above: the window getters below
// hand their work to the main thread and wait for it, which would deadlock a
// command already running there. The same goes for `ready`.
#[tauri::command]
async fn save_window_state(
    window: WebviewWindow,
    paths: State<'_, Paths>,
) -> Result<settings::Settings, String> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    let maximized = window.is_maximized().unwrap_or(false);
    let fullscreen = window.is_fullscreen().unwrap_or(false);
    entries.push(("window_maximized".into(), Value::from(maximized)));
    entries.push(("fullscreen".into(), Value::from(fullscreen)));

    // A maximized or fullscreen window would otherwise overwrite the size the
    // reader chose, and they would never get it back on restore.
    if !maximized && !fullscreen {
        let scale = window.scale_factor().unwrap_or(1.0);
        if let Ok(size) = window.inner_size() {
            let size = size.to_logical::<f64>(scale);
            entries.push(("window_width".into(), Value::from(size.width)));
            entries.push(("window_height".into(), Value::from(size.height)));
        }
        if let Ok(position) = window.outer_position() {
            let position = position.to_logical::<f64>(scale);
            entries.push(("window_x".into(), Value::from(position.x)));
            entries.push(("window_y".into(), Value::from(position.y)));
        }
    }
    settings::set_many(&paths.config, entries)
}

#[tauri::command]
async fn list_themes(paths: State<'_, Paths>) -> Result<Vec<theme::Theme>, String> {
    Ok(theme::load_all(&paths.themes))
}

#[tauri::command]
async fn save_theme(paths: State<'_, Paths>, theme: theme::Theme) -> Result<theme::Theme, String> {
    theme::save(&paths.themes, &theme)
}

#[tauri::command]
async fn delete_theme(paths: State<'_, Paths>, id: String) -> Result<Vec<theme::Theme>, String> {
    theme::delete(&paths.themes, &id)?;
    Ok(theme::load_all(&paths.themes))
}

#[tauri::command]
async fn pick_pdf(app: AppHandle) -> Option<String> {
    // A blocking dialog is fine here: async commands never run on the main
    // thread, which is the one thing the file picker cannot tolerate.
    app.dialog()
        .file()
        .add_filter("PDF document", &["pdf"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
async fn open_document(paths: State<'_, Paths>, path: String) -> Result<Opened, String> {
    if !PathBuf::from(&path).is_file() {
        return Err(format!("{} is no longer there.", file_name(&path)));
    }
    let name = file_name(&path);
    let updated = library::touch(&paths.config, &path, &name, now())?;
    let entry = updated.files.iter().find(|e| e.path == path);
    Ok(Opened {
        path: path.clone(),
        name,
        page: entry.map(|e| e.page).unwrap_or(1),
        offset: entry.map(|e| e.offset).unwrap_or(0.0),
    })
}

/// Open a document for reading and report how long it is.
///
/// Nothing is read here. pdf.js is given the length and asks for the pieces it
/// needs — the cross-reference table at the end, then the pages actually being
/// looked at — through `read_range`. Handing it the whole file instead meant
/// three copies of every document in memory at once: the buffer read here, the
/// array it became on the way through the bridge, and the copy the pdf.js
/// worker keeps. A five hundred megabyte scan cost well over a gigabyte before
/// a single page was drawn, and every one of those bytes was read before the
/// first one was shown.
#[tauri::command]
async fn open_for_reading(
    open: State<'_, OpenFile>,
    watching: State<'_, watch::Watching>,
    path: String,
) -> Result<u64, String> {
    let length = open.begin(&path)?;
    // Only a document that opened is worth following, and this is also where
    // a second document displaces the first: nothing closes in between.
    watching.document(Some(&path));
    Ok(length)
}

/// A slice of the open document. Returned raw rather than as JSON, so the
/// bytes do not get base64'd through the IPC bridge.
#[tauri::command]
async fn read_range(
    open: State<'_, OpenFile>,
    path: String,
    start: u64,
    length: u64,
) -> Result<tauri::ipc::Response, String> {
    open.range(&path, start, length)
        .map(tauri::ipc::Response::new)
}

/// Let go of the open document, so the handle does not outlive the reading.
#[tauri::command]
async fn close_document(
    open: State<'_, OpenFile>,
    watching: State<'_, watch::Watching>,
) -> Result<(), String> {
    open.close();
    watching.document(None);
    Ok(())
}

#[tauri::command]
async fn remember_position(
    paths: State<'_, Paths>,
    path: String,
    page: u32,
    offset: f64,
) -> Result<(), String> {
    library::remember(&paths.config, &path, page, offset)
}

#[tauri::command]
async fn forget_document(
    paths: State<'_, Paths>,
    path: String,
) -> Result<Vec<library::Entry>, String> {
    library::forget(&paths.config, &path).map(|library| library.files)
}

/// A link from inside a document, handed to whatever the system uses for web
/// pages.
///
/// Only web addresses and mail are allowed through. A PDF may point at a page;
/// it may not point at a program, and it never gets near a shell — the address
/// is passed as a single argument.
#[tauri::command]
async fn open_link(url: String) -> Result<(), String> {
    let scheme_ok = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| url.len() > scheme.len() && url.starts_with(scheme));
    if !scheme_ok || url.chars().any(char::is_control) {
        return Err("That link does not point at a web page.".into());
    }

    let command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&url);
        c
    } else if cfg!(target_os = "windows") {
        // Not `start`, which would send the address through the shell.
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler").arg(&url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&url);
        c
    };

    match wait_for(command).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Nothing here knows how to open that link.".into()),
        Err(e) => Err(format!("Could not open the link: {e}")),
    }
}

/// Start a program and wait for it to hand off, on a thread that is allowed to
/// sit still.
///
/// `status()` blocks until the child exits. Called straight from an async
/// command that would tie up one of the runtime's few worker threads for as
/// long as the launcher took, and the launchers here are the slowest programs
/// on the system to start cold. `spawn_blocking` is where waiting belongs.
async fn wait_for(mut command: std::process::Command) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        command
            .status()
            .map(|status| status.success())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Show a document where it lives, selected, in whatever this system uses to
/// browse files.
///
/// Async for the same reason as `open_link`: waiting for the file manager to
/// start, and reaping it afterwards, has no business on the main thread.
#[tauri::command]
async fn reveal_document(path: String) -> Result<(), String> {
    let file = PathBuf::from(&path);
    if !file.exists() {
        return Err(format!("{} is no longer there.", file_name(&path)));
    }

    #[cfg(target_os = "macos")]
    let command = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&file);
        c
    };

    #[cfg(target_os = "windows")]
    let command = {
        // `/select,` takes the path as one argument; nothing goes through a
        // shell.
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(format!("/select,{}", file.display()));
        c
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = {
        // The freedesktop file managers answer this; the fallback below opens
        // the folder for the ones that do not.
        let uri = format!("file://{}", file.display());
        let mut ask = std::process::Command::new("dbus-send");
        ask.args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ]);
        if wait_for(ask).await.unwrap_or(false) {
            return Ok(());
        }
        let mut c = std::process::Command::new("xdg-open");
        c.arg(file.parent().unwrap_or(&file));
        c
    };

    match wait_for(command).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Nothing here knows how to show that file.".into()),
        Err(e) => Err(format!("Could not show the file: {e}")),
    }
}

/// Console output from the webview during development, where there is no
/// terminal attached to it.
#[tauri::command]
fn log(message: String) {
    eprintln!("[webview] {message}");
}

/// Called once the interface is listening and ready to paint. Showing the
/// window only then means no white flash before a dark theme arrives.
///
/// Returns the document the app was started with, if there was one: by now the
/// frontend is listening, so anything arriving later comes through as an event.
#[tauri::command]
async fn ready(window: WebviewWindow, pending: State<'_, Pending>) -> Result<Option<String>, ()> {
    let _ = window.show();
    let _ = window.set_focus();
    pending.listening.store(true, Ordering::SeqCst);
    Ok(pending.file.lock().ok().and_then(|mut file| file.take()))
}

/// Show or hide the close, minimise and zoom buttons.
///
/// With the title bar overlaid on the document, those three are the last of
/// the app's own furniture left on screen once the toolbar is put away — so
/// putting the toolbar away should take them with it. Nothing in Tauri reaches
/// them, so this goes to the NSWindow itself. Elsewhere the window has a real
/// title bar of its own and there is nothing to do.
#[tauri::command]
fn set_titlebar_buttons(window: WebviewWindow, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        // AppKit only, and only from the main thread.
        let _ = window.clone().run_on_main_thread(move || {
            use objc2::runtime::{AnyObject, Bool};
            let Ok(handle) = window.ns_window() else {
                return;
            };
            let ns_window = handle as *mut AnyObject;
            // NSWindowButton: close, miniaturise, zoom.
            for button in 0usize..3 {
                unsafe {
                    let button: *mut AnyObject =
                        objc2::msg_send![ns_window, standardWindowButton: button];
                    if !button.is_null() {
                        let _: () = objc2::msg_send![button, setHidden: Bool::new(!visible)];
                    }
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, visible);
    }
}

fn restore_window(window: &WebviewWindow, stored: &settings::Settings) {
    let number = |key: &str| stored.get(key).and_then(|v| v.as_f64());

    if let (Some(width), Some(height)) = (number("window_width"), number("window_height")) {
        let _ = window.set_size(tauri::LogicalSize::new(width.max(480.0), height.max(360.0)));
    }
    if let (Some(x), Some(y)) = (number("window_x"), number("window_y")) {
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        // A screen may have been unplugged since; pull the window back on.
        if let Ok(false) = is_on_screen(window) {
            let _ = window.center();
        }
    } else {
        let _ = window.center();
    }
    if stored
        .get("window_maximized")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let _ = window.maximize();
    }
    if stored
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let _ = window.set_fullscreen(true);
    }
}

fn is_on_screen(window: &WebviewWindow) -> Result<bool, tauri::Error> {
    let position = window.outer_position()?;
    let size = window.outer_size()?;
    for monitor in window.available_monitors()? {
        let area = monitor.position();
        let bounds = monitor.size();
        let overlaps_x =
            position.x < area.x + bounds.width as i32 && position.x + size.width as i32 > area.x;
        let overlaps_y =
            position.y < area.y + bounds.height as i32 && position.y + size.height as i32 > area.y;
        if overlaps_x && overlaps_y {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The document named on the command line, if there was one.
///
/// Any argument that is not a flag and names a file that exists. Deliberately
/// not "ends in .pdf": on Linux a file is what its contents say it is and an
/// extension is optional, so filtering on the name meant `hylopdf ./report`
/// opened nothing and said nothing about why. Let the parser be the judge —
/// it already reports a document it cannot read, and it does it properly.
fn first_document_argument() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-') && Path::new(arg).is_file())
}

/// The same question asked of a second instance's arguments, which arrive as a
/// list rather than from the environment.
fn document_among(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-') && Path::new(arg).is_file())
        .cloned()
}

/// A document handed to us by the OS: sent straight through if the interface
/// is up, stashed for it to collect at boot if it is not.
fn hand_over(app: &AppHandle, path: String) {
    let Some(pending) = app.try_state::<Pending>() else {
        return;
    };
    if pending.listening.load(Ordering::SeqCst) {
        let _ = app.emit("open-document", &path);
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
        return;
    }
    let Ok(mut slot) = pending.file.lock() else {
        return;
    };
    *slot = Some(path);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // One HyloPDF at a time.
    //
    // `RunEvent::Opened` below is an Apple Events mechanism and fires on macOS
    // alone. Everywhere else the system answers "open this PDF" by launching
    // the whole app again with the path in its arguments — so double-clicking
    // three documents gave three windows, three pdf.js runtimes, and three
    // processes writing over each other's `settings.toml`, which no lock
    // inside one of them can help with. A second instance now hands its
    // document to the first and stands down.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(
            |app: &AppHandle, args: Vec<String>, _cwd: String| {
                if let Some(path) = document_among(&args) {
                    hand_over(app, path);
                } else if let Some(window) = app.get_webview_window("main") {
                    // Started again with nothing to open: the reader is looking
                    // for the window they already have.
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            },
        ));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            set_settings,
            save_window_state,
            list_themes,
            save_theme,
            delete_theme,
            pick_pdf,
            open_document,
            open_for_reading,
            read_range,
            close_document,
            remember_position,
            forget_document,
            open_link,
            reveal_document,
            ready,
            set_titlebar_buttons,
            log,
        ])
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let themes = config.join("themes");
            std::fs::create_dir_all(&config).ok();
            theme::install_built_ins(&themes);

            let stored = settings::load(&config);
            // Started after the shipped themes are written, so that writing
            // them is not itself the first thing it reports.
            app.manage(watch::start(app.handle().clone(), themes.clone()));
            app.manage(Paths { config, themes });
            app.manage(OpenFile::default());
            app.manage(Pending {
                file: Mutex::new(first_document_argument()),
                listening: AtomicBool::new(false),
            });

            if let Some(window) = app.get_webview_window("main") {
                restore_window(&window, &stored);

                // If the frontend never reports in, show the window anyway
                // rather than leaving the reader with nothing.
                let handle = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let _ = handle.show();
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HyloPDF")
        .run(|app, event| {
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        hand_over(app, path.to_string_lossy().to_string());
                    }
                }
            }
        });
}
