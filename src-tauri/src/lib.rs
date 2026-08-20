mod library;
mod settings;
mod theme;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

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

#[tauri::command]
fn bootstrap(paths: State<'_, Paths>) -> Bootstrap {
    let stored = library::load(&paths.config);
    Bootstrap {
        settings: settings::load(&paths.config),
        themes: theme::load_all(&paths.themes),
        library: library::prune(&stored).files,
        config_dir: paths.config.to_string_lossy().to_string(),
        themes_dir: paths.themes.to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn set_setting(
    paths: State<'_, Paths>,
    key: String,
    value: Value,
) -> Result<settings::Settings, String> {
    settings::set(&paths.config, &key, value)
}

/// The window's geometry is one observation of one window, so it is written in
/// one go. Everything else goes through `set_setting`, one key at a time.
// Async, and deliberately so: window getters below hand their work to the
// main thread and wait for it, which would deadlock a command already running
// there. The same goes for `ready`.
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
fn list_themes(paths: State<'_, Paths>) -> Vec<theme::Theme> {
    theme::load_all(&paths.themes)
}

#[tauri::command]
fn save_theme(paths: State<'_, Paths>, theme: theme::Theme) -> Result<theme::Theme, String> {
    theme::save(&paths.themes, &theme)
}

#[tauri::command]
fn delete_theme(paths: State<'_, Paths>, id: String) -> Result<Vec<theme::Theme>, String> {
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
fn open_document(paths: State<'_, Paths>, path: String) -> Result<Opened, String> {
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

/// The bytes of the document. Returned raw rather than as JSON, so a large PDF
/// does not get base64'd through the IPC bridge.
#[tauri::command]
fn read_document(path: String) -> Result<tauri::ipc::Response, String> {
    std::fs::read(&path)
        .map(tauri::ipc::Response::new)
        .map_err(|e| format!("Could not read {}: {e}", file_name(&path)))
}

#[tauri::command]
fn remember_position(
    paths: State<'_, Paths>,
    path: String,
    page: u32,
    offset: f64,
) -> Result<(), String> {
    library::remember(&paths.config, &path, page, offset)
}

#[tauri::command]
fn forget_document(paths: State<'_, Paths>, path: String) -> Result<Vec<library::Entry>, String> {
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

    let mut command = if cfg!(target_os = "macos") {
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

    // Async, so waiting for the launcher to hand off — and reaping it — never
    // touches the main thread.
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err("Nothing here knows how to open that link.".into()),
        Err(e) => Err(format!("Could not open the link: {e}")),
    }
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
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&file);
        c
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        // `/select,` takes the path as one argument; nothing goes through a
        // shell.
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(format!("/select,{}", file.display()));
        c
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        // The freedesktop file managers answer this; the fallback below opens
        // the folder for the ones that do not.
        let uri = format!("file://{}", file.display());
        let shown = std::process::Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:{uri}"),
                "string:",
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if shown {
            return Ok(());
        }
        let mut c = std::process::Command::new("xdg-open");
        c.arg(file.parent().unwrap_or(&file));
        c
    };

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err("Nothing here knows how to show that file.".into()),
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

fn first_pdf_argument() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-') && arg.to_lowercase().ends_with(".pdf"))
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            set_setting,
            save_window_state,
            list_themes,
            save_theme,
            delete_theme,
            pick_pdf,
            open_document,
            read_document,
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
            app.manage(Paths { config, themes });
            app.manage(Pending {
                file: Mutex::new(first_pdf_argument()),
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
