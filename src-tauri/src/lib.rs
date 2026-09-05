mod keys;
mod library;
mod settings;
mod theme;
mod watch;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
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

/// Where the pristine copy of a document goes the first time this app writes
/// into it. Beside the document rather than tucked away in the config
/// directory, because the point is that the reader can find it without
/// knowing this app keeps one.
fn original_backup_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.pdf");
    target.with_file_name(format!("{name}.hylopdf-original"))
}

/// Resolved once at startup: the config directory and the themes directory
/// inside it.
struct Paths {
    config: PathBuf,
    themes: PathBuf,
}

/// Files waiting to be opened — from the command line, from the OS asking us
/// to open one while we were still starting up, or seeded into a window this
/// process has just made. Once a window's interface is up it takes documents
/// by event instead, so `ready` decides which route applies.
///
/// Both maps are keyed by window label. A window that is still painting cannot
/// be sent an event, and with more than one window there is no longer a single
/// "the interface" to ask about.
#[derive(Default)]
struct Pending {
    file: Mutex<HashMap<String, String>>,
    listening: Mutex<HashSet<String>>,
}

impl Pending {
    fn hold(&self, window: &str, path: String) {
        let mut files = self.file.lock().unwrap_or_else(|e| e.into_inner());
        files.insert(window.to_string(), path);
    }

    fn is_listening(&self, window: &str) -> bool {
        let listening = self.listening.lock().unwrap_or_else(|e| e.into_inner());
        listening.contains(window)
    }

    /// Give back whatever was being held for a window that is gone — a
    /// document queued for it, or its place in `listening`. Without this both
    /// maps grow by one label for every window ever opened, for the life of
    /// the process: `ready` removes its own `file` entry once it has read it,
    /// but nothing removed `listening`, and a window whose build failed never
    /// called `ready` at all.
    fn forget(&self, window: &str) {
        let mut files = self.file.lock().unwrap_or_else(|e| e.into_inner());
        files.remove(window);
        let mut listening = self.listening.lock().unwrap_or_else(|e| e.into_inner());
        listening.remove(window);
    }
}

/// The document each window has open, held open.
///
/// pdf.js reads a document in pieces — it asks for the cross-reference table,
/// then the pages it actually needs — so the file is opened once and kept,
/// rather than opened and closed for every range. Only a path recorded here
/// can be read, which keeps `read_range` a way of reading the document the
/// asking window is showing rather than a way of reading any file on the disk.
///
/// Keyed by window label, and that keying is the whole of what a second window
/// costs on this side. There was one slot, and it was the reason two documents
/// at once could not work: the second window's `open_for_reading` replaced the
/// first window's handle, and every `read_range` from the first window after
/// that came back "That is not the document that is open" — in the middle of a
/// scroll, with no way to recover short of reopening.
#[derive(Default)]
struct OpenFiles(Mutex<HashMap<String, (String, File)>>);

impl OpenFiles {
    /// Open a document for reading, on behalf of a window, and report its size.
    fn begin(&self, window: &str, path: &str) -> Result<u64, String> {
        let file =
            File::open(path).map_err(|e| format!("Could not read {}: {e}", file_name(path)))?;
        let length = file
            .metadata()
            .map_err(|e| format!("Could not measure {}: {e}", file_name(path)))?
            .len();
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        held.insert(window.to_string(), (path.to_string(), file));
        Ok(length)
    }

    /// Bytes `[start, start + length)` of the document that window has open.
    fn range(&self, window: &str, path: &str, start: u64, length: u64) -> Result<Vec<u8>, String> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some((open, file)) = held.get_mut(window) else {
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

    /// Replace the document a window has open with `bytes` — an incremental
    /// update pdf.js produced, original bytes untouched and new objects
    /// appended — and report the new length.
    ///
    /// Locked the same way `range` is: one `Mutex` over every window's
    /// handle, so a read and a write for the same document cannot interleave
    /// mid-operation. Coarser than a per-document lock would be, and there is
    /// only one document worth writing at a time in practice, so this is the
    /// existing shape rather than a new one.
    ///
    /// The first write to a given document leaves `.hylopdf-original` beside
    /// it — untouched by every write after the first, because it exists to
    /// answer for all of them, not just the last one.
    fn write(&self, window: &str, path: &str, bytes: &[u8]) -> Result<u64, String> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some((open, _)) = held.get(window) else {
            return Err("No document is open.".into());
        };
        if open != path {
            return Err("That is not the document that is open.".into());
        }

        let target = Path::new(path);
        let backup = original_backup_path(target);
        if !backup.exists() {
            std::fs::copy(target, &backup).map_err(|e| format!("Could not back up: {e}"))?;
        }

        atomic_write(target, bytes)?;

        // The rename that just happened detached the old handle from the file
        // it used to point at — POSIX leaves an open descriptor reading
        // whatever inode it had, not whatever is at the path now — so every
        // read after this one needs a handle opened fresh against the new
        // file.
        let file = File::open(target).map_err(|e| format!("Could not reopen: {e}"))?;
        let length = file
            .metadata()
            .map_err(|e| format!("Could not measure: {e}"))?
            .len();
        held.insert(window.to_string(), (path.to_string(), file));
        Ok(length)
    }

    /// Undo the most recent `write` into this document, by truncating it back
    /// to the length it had before that write.
    ///
    /// This works — and does not need pdf.js's cooperation at all — because
    /// every write here is `saveDocument()`'s own incremental update: the
    /// bytes before `at_length` are the original file, untouched, and
    /// everything an annotation added sits after them. Dropping those bytes
    /// is not an edit to an object in the file, it is the file as it stood
    /// one write ago, which sidesteps the limit `markup-assessment.md` found
    /// in `saveDocument()` — that it cannot edit or delete an annotation
    /// already there — entirely: nothing is ever asked to delete anything,
    /// the trailing bytes are simply never written back.
    ///
    /// `expected_length` is checked first and the whole thing refused if it
    /// does not match what is on disk right now: if anything else touched the
    /// file since the write this is meant to undo — a second highlight, a
    /// recompile — the offset this call was given no longer names the
    /// boundary the caller thinks it does, and truncating to it would eat
    /// whatever landed after.
    fn revert(
        &self,
        window: &str,
        path: &str,
        expected_length: u64,
        at_length: u64,
    ) -> Result<(), String> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some((open, _)) = held.get(window) else {
            return Err("No document is open.".into());
        };
        if open != path {
            return Err("That is not the document that is open.".into());
        }

        let target = Path::new(path);
        let current = std::fs::metadata(target)
            .map(|meta| meta.len())
            .map_err(|e| format!("Could not measure {}: {e}", file_name(path)))?;
        if current != expected_length {
            return Err(
                "This document has changed since, so that mark can no longer be undone.".into(),
            );
        }

        let mut file =
            File::open(target).map_err(|e| format!("Could not read {}: {e}", file_name(path)))?;
        let mut kept = vec![0u8; at_length as usize];
        file.read_exact(&mut kept)
            .map_err(|e| format!("Could not read {}: {e}", file_name(path)))?;
        drop(file);

        atomic_write(target, &kept)?;

        let file = File::open(target).map_err(|e| format!("Could not reopen: {e}"))?;
        held.insert(window.to_string(), (path.to_string(), file));
        Ok(())
    }

    fn close(&self, window: &str) {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        held.remove(window);
    }

    /// Whether this window is reading a document. The one answer here that is
    /// nobody's bookkeeping: it is true exactly while the handle is open.
    fn holds(&self, window: &str) -> bool {
        let held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        held.contains_key(window)
    }

    /// Whether this window's open document is this one — the same question
    /// `range` and `write` ask before touching anything, pulled out for
    /// `original_document`, which reads a file beside it rather than the
    /// document itself.
    fn is_open(&self, window: &str, path: &str) -> bool {
        let held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        matches!(held.get(window), Some((open, _)) if open == path)
    }
}

/// What each window is showing, in the order the windows claimed a document.
///
/// This is the only source of two answers. The first is what `library.open` is
/// written from, so that a launch can put back every window that was open
/// rather than only the last one. The second is which window a document handed
/// over by the system should go to: a window not named here has nothing in it,
/// and `hand_over` claims it here the moment it picks it, so two files
/// double-clicked at once do not both land in the same empty window.
///
/// A window closing takes its entry out — closing a window is putting its
/// document down, and a document put down should not come back. See
/// `tidy_after`, which is where that happens and where the one case it cannot
/// mean that is handled: quitting. Putting a *document* down without the
/// window — the reader's own Close, which says `None` here — is remembered the
/// same way.
#[derive(Default)]
struct OpenDocuments(Mutex<Vec<(String, String)>>);

impl OpenDocuments {
    /// Record what a window is showing, and report the list as it now stands.
    fn set(&self, window: &str, path: Option<&str>) -> Vec<String> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match path {
            Some(path) => match held.iter_mut().find(|(label, _)| label == window) {
                Some(slot) => slot.1 = path.to_string(),
                None => held.push((window.to_string(), path.to_string())),
            },
            None => held.retain(|(label, _)| label != window),
        }
        held.iter().map(|(_, path)| path.clone()).collect()
    }

    /// Whether this window already has something to show, or is about to.
    fn taken(&self, window: &str) -> bool {
        let held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        held.iter().any(|(label, _)| label == window)
    }

    /// The window already showing this document, if one is.
    ///
    /// Asked before a document handed over by the system is opened at all: a
    /// file the reader already has open is one they want to *look* at, and
    /// opening a second copy of it beside the first is the one thing
    /// double-clicking it cannot mean.
    fn showing(&self, path: &str) -> Option<String> {
        let held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        held.iter()
            .find(|(_, open)| open == path)
            .map(|(label, _)| label.clone())
    }
}

/// Whether the app is on its way out.
///
/// A window going means two different things and they are told apart by
/// nothing else. Closed by the reader, it means they have finished with that
/// document. Closed because the app is quitting, it means nothing at all — the
/// document was open at the end, which is exactly what the next launch is
/// meant to put back. So everything that ends the app raises this first, and
/// `tidy_after` forgets nothing once it is up.
///
/// Three things can raise it: `quit_app`, which is how the app is left on the
/// platforms with no menu bar to put Quit in; `RunEvent::ExitRequested`; and
/// `RunEvent::Exit`, which on macOS is what ⌘Q arrives as — AppKit terminates
/// the process without closing the windows one at a time, so the flag is up
/// before any of them can be mistaken for a reader closing it.
#[derive(Default)]
struct Exiting(AtomicBool);

impl Exiting {
    fn now(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn under_way(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Serialize)]
struct Bootstrap {
    settings: settings::Settings,
    themes: Vec<theme::Theme>,
    library: Vec<library::Entry>,
    /// What this window should reopen, if anything. Only the window the app
    /// launched with is ever given one: every other window is either new, and
    /// so has nothing to come back to, or was made to hold one of the *other*
    /// documents that were open last, which reaches it through `ready` the
    /// same way a double-clicked file does.
    open_document: String,
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

/// Whether the reader asked to come back to what was open last.
///
/// One reading, in one place, because two sides of the app act on it: the
/// launch window's own document (`bootstrap`) and every other window the last
/// session had (`Restore`, in `setup`) — and `setup` also *claims* the launch
/// window in `OpenDocuments` on the strength of it. That claim was made
/// unconditionally, so with the setting off the launch window sat on the start
/// screen while this side believed it was holding a document: `idle_window`
/// skipped it, and the next file double-clicked opened a second window rather
/// than filling the empty one already on screen.
fn wants_reopening(settings: &settings::Settings) -> bool {
    settings
        .get("reopen_last_document")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
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
async fn bootstrap(window: WebviewWindow, paths: State<'_, Paths>) -> Result<Bootstrap, String> {
    let stored = library::prune(&library::load(&paths.config));
    let settings = settings::load(&paths.config);
    // Only the launch window has anything to come back to, and only when the
    // reader asked to come back to it. The setting is asked here rather than
    // left to the frontend: this used to hand over the document whatever it
    // said and rely on `main.ts` to ignore the answer, which meant the two
    // sides of the bridge disagreed about whether the launch window had
    // anything in it. See `reopening` in `setup`, which is the other half.
    let reopen = if window.label() == MAIN && wants_reopening(&settings) {
        stored.open.first().cloned().unwrap_or_default()
    } else {
        String::new()
    };
    Ok(Bootstrap {
        settings,
        themes: theme::load_all(&paths.themes),
        library: stored.files,
        open_document: reopen,
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
///
/// And only ever of the launch window. There is one remembered geometry and
/// there are several windows, so somebody has to own it; letting whichever
/// window last moved own it meant the number crept down and across a little
/// every session, because the windows it was reading back were themselves
/// cascaded off it. The rule is simple to say and it holds still: the window
/// HyloPDF opens with is the one whose size and place are remembered, and the
/// rest cascade off that one each time.
// Async for a second reason as well as the one above: the window getters below
// hand their work to the main thread and wait for it, which would deadlock a
// command already running there. The same goes for `ready`.
#[tauri::command]
async fn save_window_state(
    window: WebviewWindow,
    paths: State<'_, Paths>,
) -> Result<settings::Settings, String> {
    if window.label() != MAIN {
        return Ok(settings::load(&paths.config));
    }
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

/// The keys, read from `keys.toml`.
///
/// A door of its own rather than a field on `Bootstrap`, because it is asked
/// twice: once before the first keystroke, and again whenever the reader has
/// edited the file and pressed Reload. The themes directory is watched and
/// this file is not — a watch on the config directory would fire on every
/// `settings.toml` write the app makes itself, which is several a minute
/// while somebody is scrolling.
#[tauri::command]
async fn load_keys(paths: State<'_, Paths>) -> Result<keys::Keys, String> {
    Ok(keys::load(&paths.config))
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
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    watching: State<'_, watch::Watching>,
    path: String,
) -> Result<u64, String> {
    let length = open.begin(window.label(), &path)?;
    // Only a document that opened is worth following, and this is also where
    // a second document displaces the first *in this window*: nothing closes
    // in between, and no other window is touched.
    watching.document(window.label(), Some(&path));
    Ok(length)
}

/// A slice of the open document. Returned raw rather than as JSON, so the
/// bytes do not get base64'd through the IPC bridge.
#[tauri::command]
async fn read_range(
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    path: String,
    start: u64,
    length: u64,
) -> Result<tauri::ipc::Response, String> {
    open.range(window.label(), &path, start, length)
        .map(tauri::ipc::Response::new)
}

/// Write bytes pdf.js produced — an incremental update carrying a highlight —
/// over the document a window has open, and tell that window to reload the
/// same way it would for a change made outside the app.
///
/// The reload is deliberate rather than left to the watcher: `open.write`
/// already knows the write landed, so this fires `document-changed` for the
/// writing window itself, and `watching.wrote` tells the watcher that the
/// burst of file-system events the write is about to cause is not news — the
/// baseline moves right here, before the real burst arrives, rather than
/// racing it. A second window with the same document open is not touched by
/// either call and gets the ordinary reload once the watcher notices on its
/// own, which is correct: its transport really is stale.
#[tauri::command]
async fn write_document(
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    watching: State<'_, watch::Watching>,
    path: String,
    bytes: Vec<u8>,
) -> Result<u64, String> {
    let length = open.write(window.label(), &path, &bytes)?;
    watching.wrote(window.label(), Path::new(&path));
    let _ = window.emit_to(window.label(), "document-changed", &path);
    Ok(length)
}

/// Undo the most recent write into this document — see `OpenFiles::revert`
/// for how truncation stands in for an edit `saveDocument()` cannot make.
/// Reloads the writing window exactly the way `write_document` does, which is
/// what puts the highlight that write added back off the page.
#[tauri::command]
async fn revert_write(
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    watching: State<'_, watch::Watching>,
    path: String,
    expected_length: u64,
    at_length: u64,
) -> Result<(), String> {
    open.revert(window.label(), &path, expected_length, at_length)?;
    watching.wrote(window.label(), Path::new(&path));
    let _ = window.emit_to(window.label(), "document-changed", &path);
    Ok(())
}

/// The pristine copy of a document, from before this app ever wrote into it —
/// see `original_backup_path`. What `App.removeHighlight` rebuilds from:
/// `saveDocument()` cannot edit or delete an annotation already in the file
/// (see `markup-assessment.md`), so removing one this app wrote, at any point
/// rather than only right after writing it, means starting again from the
/// backup and replaying every highlight still wanted as a fresh write.
///
/// Errs when there is no backup — nothing has ever been written into this
/// document, so there is nothing of this app's own in it to remove either.
#[tauri::command]
async fn original_document(
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    path: String,
) -> Result<Vec<u8>, String> {
    if !open.is_open(window.label(), &path) {
        return Err("That is not the document that is open.".into());
    }
    let backup = original_backup_path(Path::new(&path));
    std::fs::read(&backup).map_err(|_| {
        "This document has never been marked, so there is nothing to rebuild from.".to_string()
    })
}

/// What standing this app has to write markup into a document — asked before
/// the reader marks anything rather than discovered by trying.
///
/// Two answers, and they are different in kind. `writable` is a fact about
/// the disk: false means an attempt would fail, so the gesture keeps the
/// markup in the journal instead and says so once. `cloud` is not a refusal
/// at all — a file in a syncing folder can be written perfectly well, and the
/// thing worth saying is that the service, not this app, decides which copy
/// wins if it is open on two machines at once.
#[derive(Debug, Clone, Serialize)]
pub struct Writability {
    writable: bool,
    /// Why not, in one line, ready to be shown. Empty when it is writable.
    reason: String,
    /// The syncing service whose folder this document sits in, named, when it
    /// sits in one.
    cloud: Option<String>,
    /// How long the document is. The frontend decides from this whether
    /// `saveDocument()` — which pulls the whole file into the worker — is a
    /// reasonable thing to do on every mark.
    size: u64,
}

/// Folders that mean a file is being synced somewhere else as well. Matched
/// against a whole path component, and by prefix within it, because these
/// arrive personalised: "OneDrive - Acme", "Dropbox (Personal)".
const SYNCED_FOLDERS: &[(&str, &str)] = &[
    ("Mobile Documents", "iCloud Drive"),
    ("com~apple~CloudDocs", "iCloud Drive"),
    ("Dropbox", "Dropbox"),
    ("Google Drive", "Google Drive"),
    ("GoogleDrive", "Google Drive"),
    ("My Drive", "Google Drive"),
    ("OneDrive", "OneDrive"),
    ("Nextcloud", "Nextcloud"),
    ("ownCloud", "ownCloud"),
    ("pCloud Drive", "pCloud"),
    ("Sync.com", "Sync.com"),
    ("Proton Drive", "Proton Drive"),
    ("Box Sync", "Box"),
];

/// The syncing service this path is inside, if it is inside one.
///
/// A guess by name, and it can only ever be a guess: a folder called Dropbox
/// that is nothing of the kind produces one extra line of notice, and a
/// service nobody here has heard of produces none. Both are the right way to
/// be wrong for something whose whole output is one sentence of warning.
fn cloud_service(path: &Path) -> Option<String> {
    for part in path.components() {
        let name = part.as_os_str().to_string_lossy();
        for (folder, service) in SYNCED_FOLDERS {
            if name.len() >= folder.len() && name[..folder.len()].eq_ignore_ascii_case(folder) {
                return Some((*service).to_string());
            }
        }
    }
    None
}

/// Whether the disk would refuse this write, and what to say if it does.
///
/// The file is asked by opening it for writing and closing it again, which is
/// the only question with an answer that is actually true: a read-only file,
/// a read-only volume, a file somebody else owns and a sandbox that has not
/// granted this path all come back the same way, and none of them can be read
/// off the permission bits alone. Nothing is written by the probe — no
/// `truncate`, no `append` — so a document that survives it is exactly as it
/// was.
///
/// The folder is asked separately and more cheaply, because an incremental
/// update is written *beside* the document and renamed over it (see
/// `atomic_write`), so a writable file in an unwritable folder is still a
/// write that cannot happen. That half is read off the permission bits and is
/// therefore the half that can be wrong; the write itself still fails safely
/// and reports, so this is a better message rather than the only guard.
fn refuses_writing(path: &Path) -> Option<String> {
    let name = file_name(&path.to_string_lossy());
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Some(format!("{name} is read only."));
        }
        Err(error) => return Some(format!("{name} cannot be written: {error}")),
    }
    let folder = path.parent()?;
    match std::fs::metadata(folder) {
        Ok(meta) if meta.permissions().readonly() => {
            Some(format!("The folder {name} is in is read only."))
        }
        _ => None,
    }
}

/// Ask the disk about a document before offering to write into it.
fn writability(path: &Path) -> Writability {
    let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let cloud = cloud_service(path);
    match refuses_writing(path) {
        Some(reason) => Writability {
            writable: false,
            reason,
            cloud,
            size,
        },
        None => Writability {
            writable: true,
            reason: String::new(),
            cloud,
            size,
        },
    }
}

/// Whether markup can be written into this document, and what to say about it
/// if it cannot. Asked once per open — see `App.readMarkupStanding`.
#[tauri::command]
async fn document_writability(path: String) -> Result<Writability, String> {
    Ok(writability(Path::new(&path)))
}

/// Let go of the open document, so the handle does not outlive the reading.
#[tauri::command]
async fn close_document(
    window: WebviewWindow,
    open: State<'_, OpenFiles>,
    watching: State<'_, watch::Watching>,
) -> Result<(), String> {
    open.close(window.label());
    watching.document(window.label(), None);
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

/// Note which document this window is showing, so the next launch can put the
/// windows back where they were. `None` when the reader has closed it, which
/// is them saying they are done with it.
///
/// The window says only what *it* is holding; the list written to
/// `library.toml` is every window's answer together. See `OpenDocuments`.
#[tauri::command]
async fn set_open_document(
    window: WebviewWindow,
    paths: State<'_, Paths>,
    open: State<'_, OpenDocuments>,
    path: Option<String>,
) -> Result<(), String> {
    let all = open.set(window.label(), path.as_deref());
    library::set_open(&paths.config, &all)
}

/// A second window, reading a second document.
///
/// Everything an open document needs is per-window already: the whole
/// interface is one `App` object inside one webview, so a window is a reader
/// with its own viewer, its own search index and its own sidebar for nothing
/// more than the cost of the webview. What was not per-window was on this
/// side — the file handle and the document watch — and both are keyed by
/// window label now.
///
/// What stays shared is what belongs to the app rather than to a window: the
/// settings, the themes and the library. That is also the reason this is a
/// second *window* and not a second process — one process is what keeps two
/// windows from writing over each other's `settings.toml`, which is what the
/// single-instance plugin has always been for.
#[tauri::command]
async fn new_window(app: AppHandle, path: Option<String>) -> Result<(), String> {
    spawn_window(&app, path)
}

/// Leave, off a Mac, where there is no menu bar to put Quit in.
///
/// Every window is closed rather than the process exited: a window's close
/// handler is where its position, its pending settings and its geometry are
/// written down, and `app.exit` would take the process out from under all
/// three. Closing the last window is what ends the app on those platforms
/// anyway.
#[tauri::command]
async fn quit_app(app: AppHandle) {
    // Raised before the first window goes: every one of them is about to
    // close, and none of those closes is the reader putting a document down.
    if let Some(exiting) = app.try_state::<Exiting>() {
        exiting.now();
    }
    for window in app.webview_windows().values() {
        let _ = window.close();
    }
}

/// Hand a document to whatever this system prints PDFs with.
///
/// HyloPDF does not print. Everything it would take to print well — a page
/// range, a paper size, a preview, a printer — is a dialog this app does not
/// have, and the routes that avoid writing one are all worse than they sound:
/// the webview on macOS answers `window.print()` by doing nothing at all, and
/// `lpr` and its cousins print immediately, to the default printer, with no
/// dialog and no way back. Sending four hundred pages to a printer nobody
/// chose is the one failure here that costs paper.
///
/// So this hands the file to a program that does print, and says so. Not to
/// the *print* verb, on any platform: where a system has one it prints
/// immediately, which is the failure above wearing a different hat. What is
/// wanted is the file open somewhere with a File menu in it.
///
/// Which program is named where it can be. On macOS that is Preview, rather
/// than "the default application", because once HyloPDF is installed the
/// default application for a PDF may well be HyloPDF, and handing it to
/// ourselves is a loop. Windows names Edge for the same reason — it ships with
/// every supported version, it prints PDFs properly, and it is somewhere else
/// — and falls back to the default handler where it has been removed. Linux
/// has no viewer that can be assumed, so `xdg-open` is all there is.
///
/// So the loop is still reachable on those two, and `hand_over` is where it
/// stops: a document already open comes to the front instead of opening a
/// second time. The reader gets their own window back rather than a duplicate.
#[tauri::command]
async fn print_document(path: String) -> Result<(), String> {
    let file = PathBuf::from(&path);
    if !file.exists() {
        return Err(format!("{} is no longer there.", file_name(&path)));
    }

    #[cfg(target_os = "macos")]
    let command = {
        let mut c = std::process::Command::new("open");
        c.arg("-a").arg("Preview").arg(&file);
        c
    };

    #[cfg(target_os = "windows")]
    let command = {
        // Edge by name, the way macOS names Preview: it ships with every
        // supported Windows, it prints a PDF properly, and — the point — it is
        // not us. `ShellExec_RunDLL` alone opens the file with whatever the
        // *default* handler is, which after installing this app may well be
        // this app.
        //
        // By absolute path rather than by name, because Edge is not on `PATH`.
        // Where it has been removed, the default handler is still better than
        // nothing; `hand_over` is what keeps that from becoming a loop.
        let edge = std::env::var("ProgramFiles(x86)")
            .or_else(|_| std::env::var("ProgramFiles"))
            .map(|root| PathBuf::from(root).join(r"Microsoft\Edge\Application\msedge.exe"))
            .ok()
            .filter(|path| path.exists());
        match edge {
            Some(edge) => {
                let mut c = std::process::Command::new(edge);
                c.arg(&file);
                c
            }
            None => {
                // The path as one argument, rather than through a shell.
                let mut c = std::process::Command::new("rundll32.exe");
                c.arg("shell32.dll,ShellExec_RunDLL").arg(&file);
                c
            }
        }
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&file);
        c
    };

    match wait_for(command).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Nothing here knows how to print that.".into()),
        Err(e) => Err(format!("Could not hand it over: {e}")),
    }
}

/// A pin in a page, or the same pin taken out. See `library::toggle_mark`.
#[derive(Serialize)]
struct Marked {
    marked: bool,
    marks: Vec<library::Mark>,
}

#[tauri::command]
async fn toggle_mark(
    paths: State<'_, Paths>,
    path: String,
    page: u32,
    offset: f64,
    title: String,
) -> Result<Marked, String> {
    let (marked, marks) = library::toggle_mark(&paths.config, &path, page, offset, &title, now())?;
    Ok(Marked { marked, marks })
}

/// Journal a highlight the reader just drew. See `library::add_highlight`.
#[tauri::command]
async fn add_highlight(
    paths: State<'_, Paths>,
    path: String,
    highlight: library::Highlight,
) -> Result<Vec<library::Highlight>, String> {
    library::add_highlight(&paths.config, &path, highlight)
}

/// Take a highlight out of the journal, by the id it was added with.
#[tauri::command]
async fn remove_highlight(
    paths: State<'_, Paths>,
    path: String,
    id: String,
) -> Result<Vec<library::Highlight>, String> {
    library::remove_highlight(&paths.config, &path, &id)
}

/// Replace a document's journaled highlights with what the file itself says,
/// once it has been read. See `library::set_highlights` for why this
/// discards rather than merges.
#[tauri::command]
async fn set_highlights(
    paths: State<'_, Paths>,
    path: String,
    highlights: Vec<library::Highlight>,
) -> Result<(), String> {
    library::set_highlights(&paths.config, &path, highlights)
}

/// The title the document gives itself, which the frontend reads out of the
/// file and this remembers for the recently-read list.
#[tauri::command]
async fn set_document_title(
    paths: State<'_, Paths>,
    path: String,
    title: String,
) -> Result<Vec<library::Entry>, String> {
    library::retitle(&paths.config, &path, &title).map(|library| library.files)
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

/// Open a file or folder with whatever this system opens it with by
/// default — a text editor for `keys.toml`, the file manager for the themes
/// folder. Distinct from `reveal_document`, which shows a file selected
/// inside its parent rather than opening it.
#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    let file = PathBuf::from(&path);
    if !file.exists() {
        return Err(format!("{} is no longer there.", file_name(&path)));
    }

    #[cfg(target_os = "macos")]
    let command = {
        let mut c = std::process::Command::new("open");
        c.arg(&file);
        c
    };

    #[cfg(target_os = "windows")]
    let command = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(&file);
        c
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&file);
        c
    };

    match wait_for(command).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Nothing here knows how to open that.".into()),
        Err(e) => Err(format!("Could not open it: {e}")),
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

/// The documents that were open in windows other than the launch window when
/// the app was last put down, waiting for a window each. Drained by the first
/// `ready`; see there for why it is not done in `setup`.
struct Restore(Mutex<Vec<String>>);

/// Called once the interface is listening and ready to paint. Showing the
/// window only then means no white flash before a dark theme arrives.
///
/// Returns the document that window was started with, if there was one: by now
/// its frontend is listening, so anything arriving later comes through as an
/// event.
#[tauri::command]
async fn ready(
    app: AppHandle,
    window: WebviewWindow,
    pending: State<'_, Pending>,
) -> Result<Option<String>, ()> {
    let _ = window.show();
    let _ = window.set_focus();
    place(&app, &window);
    let label = window.label().to_string();
    if let Ok(mut listening) = pending.listening.lock() {
        listening.insert(label.clone());
    }

    // The other windows the last session had open, made here rather than in
    // `setup`. A window built during `setup` comes out wrong on macOS: Tauri
    // reports it as visible and it is not on screen and not in the
    // accessibility tree, because it was made before the application had
    // finished launching. Made from here it is an ordinary window, which is
    // what the handover path has always produced. The first window to report
    // in drains the list, so it happens once.
    if let Some(restore) = app.try_state::<Restore>() {
        let waiting: Vec<String> = restore
            .0
            .lock()
            .map(|mut held| std::mem::take(&mut *held))
            .unwrap_or_default();
        for path in waiting {
            let _ = spawn_window(&app, Some(path));
        }
    }

    Ok(pending
        .file
        .lock()
        .ok()
        .and_then(|mut files| files.remove(&label)))
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

/// The smallest the window is allowed to be, and the same two numbers
/// `tauri.conf.json` gives it as `minWidth` and `minHeight`.
///
/// They used to be 480×360 here and 520×400 there. The window manager enforces
/// its own, so the smaller pair never did anything — it was dead code
/// describing an intention the app did not have. Two numbers in two files is
/// the arrangement Tauri leaves us with; the least that can be done is have
/// them agree and say where the other copy is.
const MIN_WIDTH: f64 = 520.0;
const MIN_HEIGHT: f64 = 400.0;

/// A "New Window" item on the icon in the Dock, above the standard ones.
///
/// The one route to a second window that does not need HyloPDF to be in front
/// already, which is exactly the moment somebody wants one: they are looking
/// at something else and want this document beside it. Firefox, Safari and
/// Preview all have it.
///
/// It is also the one place in this app that writes "New Window" in title
/// case. The Dock menu is the system's furniture and everything in it is
/// spelled the system's way; the app's own menus keep the sentence case they
/// use everywhere else.
///
/// Nothing in Tauri or in muda reaches the Dock menu, so this goes to AppKit
/// directly — the same as the traffic lights above, and for the same reason. A
/// menu item needs a *target*, and a target is an Objective-C object with a
/// selector on it, so one class is built at runtime, one instance of it is
/// made, and both are left alive for as long as the process is.
#[cfg(target_os = "macos")]
mod dock {
    use std::ffi::CStr;
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
    use objc2::{msg_send, sel, ClassType};
    use tauri::AppHandle;

    use super::spawn_window;

    /// The app, for the one method below, which AppKit calls with no context
    /// of its own.
    static APP: OnceLock<AppHandle> = OnceLock::new();

    unsafe fn string(text: &CStr) -> *mut AnyObject {
        let class = AnyClass::get(c"NSString").expect("NSString");
        unsafe { msg_send![class, stringWithUTF8String: text.as_ptr()] }
    }

    /// What the item does. Called on the main thread, which is why the window
    /// is made on another one: `spawn_window` asks the windows where they are,
    /// and every one of those questions is answered *by* the main thread.
    extern "C" fn new_window(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
        let Some(app) = APP.get().cloned() else {
            return;
        };
        std::thread::spawn(move || {
            let _ = spawn_window(&app, None);
        });
    }

    pub(super) fn install(app: &AppHandle) {
        if APP.set(app.clone()).is_err() {
            return;
        }
        unsafe {
            let Some(mut builder) = ClassBuilder::new(c"HyloPDFDock", NSObject::class()) else {
                return;
            };
            builder.add_method(
                sel!(hyloNewWindow:),
                new_window as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            let class = builder.register();
            // Never released: it is the target of a menu item that lives as
            // long as the application does.
            let target: *mut AnyObject = msg_send![class, new];

            let menu: *mut AnyObject = msg_send![AnyClass::get(c"NSMenu").expect("NSMenu"), new];
            let item: *mut AnyObject =
                msg_send![AnyClass::get(c"NSMenuItem").expect("NSMenuItem"), alloc];
            let item: *mut AnyObject = msg_send![
                item,
                initWithTitle: string(c"New Window"),
                action: sel!(hyloNewWindow:),
                keyEquivalent: string(c""),
            ];
            let _: () = msg_send![item, setTarget: target];
            let _: () = msg_send![menu, addItem: item];

            let shared: *mut AnyObject = msg_send![
                AnyClass::get(c"NSApplication").expect("NSApplication"),
                sharedApplication
            ];
            let _: () = msg_send![shared, setDockMenu: menu];
        }
    }
}

/// The window `tauri.conf.json` declares, and the one the app launches with.
/// It is the only window with a name of its own: every other is made by
/// `spawn_window` and named `reader-N`, which is also the pattern the
/// capability has to allow.
const MAIN: &str = "main";

/// How far a new window is stepped down and across from the one in front of
/// it, so that two windows are two windows rather than one with a stack
/// behind it.
const CASCADE: f64 = 28.0;

/// Where a window has been told to go, until it is up and can be put there.
///
/// The position handed to the builder does not survive on macOS. A window made
/// with one comes up where it was told to, and then *showing* it moves the
/// window onto the launch window's frame — so every window ends up exactly on
/// top of every other, which looks precisely like the app still only having
/// one. Setting it again right after `build` does not help, and neither does
/// setting it just before `show`, because `show` is the thing that does it. So
/// the place is worked out when the window is made, kept here, and applied by
/// `place` immediately after the window is shown, which is the first moment
/// that is the last word. Nothing is seen in between: the two happen in one
/// turn of the main thread.
#[derive(Default)]
struct Placements(Mutex<HashMap<String, (f64, f64)>>);

/// Where the window's top-left corner is, in the units settings are written in.
fn corner(window: &WebviewWindow) -> Option<(f64, f64)> {
    let scale = window.scale_factor().ok()?;
    let at = window.outer_position().ok()?.to_logical::<f64>(scale);
    Some((at.x, at.y))
}

/// Where to put a new window: one step down and across from the window in
/// front of it, and on again while that spot is taken.
///
/// Off the *front* window rather than off the remembered position, which is
/// what this did first and is worth saying why it did not work. Restoring
/// three windows makes them in one burst, so all three cascaded from the same
/// number and landed within a few pixels of each other — a stack that looks
/// exactly like one window, which is the failure this whole feature exists to
/// avoid. Stepping past what is already there is also what makes ⌘N four times
/// give four windows rather than four windows in one place.
///
/// `None` means there is nothing to cascade from, and the window is centred.
fn placement(app: &AppHandle, stored: &settings::Settings) -> Option<(f64, f64)> {
    let windows = app.webview_windows();
    // Where the windows are, and where the ones still coming up are going. A
    // window made a moment ago has not been put in its place yet, and two
    // windows made in the same breath — which is what restoring a session is —
    // would otherwise choose the same spot.
    let mut taken: Vec<(f64, f64)> = windows.values().filter_map(corner).collect();
    if let Some(placements) = app.try_state::<Placements>() {
        if let Ok(held) = placements.0.lock() {
            taken.extend(held.values().copied());
        }
    }
    let number = |key: &str| stored.get(key).and_then(|value| value.as_f64());
    let base = windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .and_then(corner)
        .or_else(|| windows.get(MAIN).and_then(corner))
        .or_else(|| Some((number("window_x")?, number("window_y")?)))?;

    let mut spot = (base.0 + CASCADE, base.1 + CASCADE);
    // Bounded: a screen this far down and across is a screen nobody has, and
    // an unbounded walk here would be an unbounded walk off the display.
    for _ in 0..16 {
        let clash = taken
            .iter()
            .any(|(x, y)| (x - spot.0).abs() < 2.0 && (y - spot.1).abs() < 2.0);
        if !clash {
            break;
        }
        spot = (spot.0 + CASCADE, spot.1 + CASCADE);
    }
    Some(spot)
}

static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

/// Make a window and, if there is a document for it, leave it where `ready`
/// will collect it.
///
/// The document is claimed in `OpenDocuments` before the window exists, which
/// is what stops a second file arriving in the same instant from being handed
/// to a window that is already spoken for.
fn spawn_window(app: &AppHandle, path: Option<String>) -> Result<(), String> {
    let label = format!("reader-{}", NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
    if let Some(path) = path.as_deref() {
        if let Some(pending) = app.try_state::<Pending>() {
            pending.hold(&label, path.to_string());
        }
        if let Some(open) = app.try_state::<OpenDocuments>() {
            open.set(&label, Some(path));
        }
    }

    let stored = app
        .try_state::<Paths>()
        .map(|paths| settings::load(&paths.config))
        .unwrap_or_default();
    let number = |key: &str| stored.get(key).and_then(|v| v.as_f64());
    let spot = placement(app, &stored);

    let mut builder = tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::default())
        .title("HyloPDF")
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .inner_size(
            number("window_width").unwrap_or(1280.0).max(MIN_WIDTH),
            number("window_height").unwrap_or(860.0).max(MIN_HEIGHT),
        )
        // Shown by `ready`, for the same reason the first window is: a
        // window that appears before the theme has been applied appears
        // white, whatever theme is about to arrive.
        .visible(false)
        .background_color(tauri::window::Color(0xf2, 0xf1, 0xed, 0xff));

    builder = match spot {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };

    // The document runs up under the title bar here as well; without this a
    // second window would have a native strip the first one does not.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }

    if let (Some(spot), Some(placements)) = (spot, app.try_state::<Placements>()) {
        if let Ok(mut held) = placements.0.lock() {
            held.insert(label.clone(), spot);
        }
    }

    let window = match builder.build() {
        Ok(window) => window,
        Err(e) => {
            // Everything above was claimed on the assumption the window would
            // exist to give it back — `tidy_after` never runs for a label
            // whose build failed, so the claims are undone here instead.
            // `OpenDocuments` is the one that matters: left in place, it is a
            // phantom window holding a document, counted against `OPEN_LIMIT`
            // and written into `library.toml` by the next `set_open`.
            if let Some(pending) = app.try_state::<Pending>() {
                pending.forget(&label);
            }
            if let Some(open) = app.try_state::<OpenDocuments>() {
                open.set(&label, None);
            }
            if let Some(placements) = app.try_state::<Placements>() {
                if let Ok(mut held) = placements.0.lock() {
                    held.remove(&label);
                }
            }
            return Err(e.to_string());
        }
    };
    tidy_after(&window);
    // The same safety net the launch window has: a frontend that never reports
    // in would otherwise leave a window that exists, holds a document, and
    // cannot be seen.
    let handle = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if !handle.is_visible().unwrap_or(false) {
            let _ = handle.show();
        }
    });
    Ok(())
}

/// Put a window where `spawn_window` decided it should go, now that it has
/// been shown. See `Placements` — showing it is what moves it, so this has to
/// come after, and nothing is on screen in between.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let label = window.label();
    let spot = app.try_state::<Placements>().and_then(|placements| {
        placements
            .0
            .lock()
            .ok()
            .and_then(|mut held| held.remove(label))
    });
    // The launch window has no placement: its geometry is `restore_window`'s,
    // and it must not be centred out of a maximised state here.
    let Some((x, y)) = spot else { return };
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    // The cascade walks; a window stepped off the end of the screen is a
    // window the reader has to go and find.
    if let Ok(false) = is_on_screen(window) {
        let _ = window.center();
    }
}

/// What a window has to give back when it goes: its file handle, its place in
/// the document watch, its entry in `OpenDocuments`, and whatever `Pending`
/// was still holding for it. None of them is the reader's business and none
/// can be asked for from the frontend, which by then is gone.
///
/// The entry is the interesting one, because a window going means two things.
/// A window the reader closed is a document they have finished with, and it
/// should not be back on the screen next launch — that was the complaint. A
/// window closing *because the app is quitting* means only that it was open at
/// the end, which is the whole of what the next launch is meant to put back.
/// `Exiting` is what tells them apart, and it is raised by everything that
/// ends the app before any window has gone.
///
/// One case is left that no flag can reach: closing the last window, which on
/// every platform ends the app, and which is how most people quit it. There is
/// no signal that separates "I have finished with this" from "goodbye" there,
/// so this never writes an *empty* list — a close can forget any window but
/// the last, and quitting with one document open still comes back to it. The
/// reader who means the other thing has Close, which empties the list from the
/// frontend and is the gesture that says so.
///
/// The write is made here on the main thread rather than handed to one of its
/// own, unlike every other write in this file: the windows of a quit close one
/// after another, and two threads racing over `library.toml` would leave
/// whichever finished last, not whichever knew most.
fn tidy_after(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::Destroyed) {
            return;
        }
        if let Some(open) = app.try_state::<OpenFiles>() {
            open.close(&label);
        }
        if let Some(watching) = app.try_state::<watch::Watching>() {
            watching.document(&label, None);
        }
        if let Some(pending) = app.try_state::<Pending>() {
            pending.forget(&label);
        }
        let leaving = app
            .try_state::<Exiting>()
            .is_some_and(|exiting| exiting.under_way());
        if leaving {
            return;
        }
        if let (Some(open), Some(paths)) =
            (app.try_state::<OpenDocuments>(), app.try_state::<Paths>())
        {
            let all = open.set(&label, None);
            if !all.is_empty() {
                let _ = library::set_open(&paths.config, &all);
            }
        }
    });
}

fn restore_window(window: &WebviewWindow, stored: &settings::Settings) {
    let number = |key: &str| stored.get(key).and_then(|v| v.as_f64());

    if let (Some(width), Some(height)) = (number("window_width"), number("window_height")) {
        let _ = window.set_size(tauri::LogicalSize::new(
            width.max(MIN_WIDTH),
            height.max(MIN_HEIGHT),
        ));
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

/// Caught by `hand_over` when the OS hands over a document before `setup` has
/// run — before `Pending` exists to hold it — and drained by `setup` once it
/// does. On macOS, `RunEvent::Opened` for a cold launch onto a document can
/// arrive that early: AppKit delivers the Apple Event as the app is still
/// coming up, ahead of the `setup` hook that creates `Pending`, so without
/// this the document a reader just double-clicked was silently dropped on the
/// floor and they landed on the start screen instead.
///
/// A list, and it has to be read as one. `RunEvent::Opened` carries `urls`,
/// plural — three files selected together and opened in one gesture are three
/// documents arriving before `setup` — and taking one of them and leaving the
/// rest in here was the same silent drop by a different door: two of the three
/// never appeared and nothing anywhere said why. See `first_and_rest`.
static EARLY_OPEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Which document the launch window takes, and which need a window each.
///
/// The command line wins the launch window where both are populated, which is
/// only ever a cold launch: a second instance's arguments are routed through
/// `hand_over` instead, so nothing else can have both, and the launch itself
/// named the document that started it. Everything left over is a document
/// somebody asked for and has to go somewhere, which is what `Restore` is for.
///
/// Split out from `setup` so that it can be tested at all — `setup` needs a
/// running application and this is one `match`.
fn first_and_rest(
    argument: Option<String>,
    mut early: Vec<String>,
) -> (Option<String>, Vec<String>) {
    match argument {
        Some(path) => (Some(path), early),
        None if early.is_empty() => (None, Vec::new()),
        // In arrival order: the first document of a selection is the one the
        // launch window opens on, which is the one a reader is looking for.
        None => {
            let first = early.remove(0);
            (Some(first), early)
        }
    }
}

/// A document handed to us by the OS — "Open with", the dock, a second launch
/// — sent to a window with nothing in it, or to a window made for it.
///
/// It used to displace whatever was on screen, because there was only ever one
/// window to displace, and that was the single worst thing this app did to
/// somebody: double-clicking a file closed the document they were reading, and
/// nothing about double-clicking a file says that. Now nothing is closed. A
/// window is idle if it is not named in `OpenDocuments`, and the one with the
/// keyboard is preferred, because that is the window the reader is looking at.
///
/// *A document already open is brought to the front rather than opened again.*
/// A second copy of a document beside the first is the one thing
/// double-clicking a file cannot mean — the reader is asking to look at it,
/// and it is already there. It is also what closes the loop `print_document`
/// can otherwise start: off macOS a document is handed to whatever the system
/// opens PDFs with, and on a machine where that is HyloPDF it comes straight
/// back here. What used to happen then was a second window on the same file,
/// with a second pdf.js runtime behind it; now the window the reader printed
/// from comes forward, which is the closest thing to the truth available.
///
/// Queued in `EARLY_OPEN` if `Pending` itself does not exist yet — on macOS an
/// Apple Event for a cold launch can arrive before `setup` has run.
fn hand_over(app: &AppHandle, path: String) {
    let Some(pending) = app.try_state::<Pending>() else {
        if let Ok(mut queue) = EARLY_OPEN.lock() {
            queue.push(path);
        }
        return;
    };
    if let Some(window) = already_showing(app, &path) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let Some(window) = idle_window(app) else {
        match spawn_window(app, Some(path.clone())) {
            Ok(()) => {}
            // A window could not be made. Whatever the reason, the one thing
            // that must not happen is the document going nowhere: somebody
            // double-clicked a file and the app came to the front with no
            // sign of it, which reads as the app being broken and gives them
            // nothing to do about it. So it goes into the window that is
            // there, which is what this did before there was more than one.
            Err(problem) => {
                eprintln!("could not make a window for {path}: {problem}");
                if let Some(window) = any_window(app) {
                    give(app, pending, &window, path);
                }
            }
        }
        return;
    };
    give(app, pending, &window, path);
}

/// Hand a document to a window: by event if its interface is up, and by
/// leaving it where `ready` will collect it if it is not.
fn give(app: &AppHandle, pending: State<'_, Pending>, window: &WebviewWindow, path: String) {
    let label = window.label().to_string();
    // Claimed before anything else can be told about it.
    if let Some(open) = app.try_state::<OpenDocuments>() {
        open.set(&label, Some(&path));
    }
    if pending.is_listening(&label) {
        let _ = app.emit_to(label.as_str(), "open-document", &path);
    } else {
        pending.hold(&label, path);
    }
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// The window this document is already open in, if it is open at all.
fn already_showing(app: &AppHandle, path: &str) -> Option<WebviewWindow> {
    let label = app.try_state::<OpenDocuments>()?.showing(path)?;
    app.get_webview_window(&label)
}

/// Any window at all, the one with the keyboard first. The last resort above.
fn any_window(app: &AppHandle) -> Option<WebviewWindow> {
    let windows = app.webview_windows();
    windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .or_else(|| windows.get(MAIN))
        .or_else(|| windows.values().next())
        .cloned()
}

/// A window with nothing to show — no document open, and none on its way. The
/// one that has the keyboard first.
///
/// Two questions rather than one, and the second is the safety net.
/// `OpenDocuments` is the answer the *frontend* gives, and it is given by a
/// call that nothing waits for; `OpenFiles` is the handle the window is
/// actually reading through, kept by the read path itself. If the first were
/// ever to be missed — a call that failed, a window that never finished
/// starting — a document handed over would be given to a window that already
/// has one, and the reader would either lose their place or, if that window
/// ignored it, see nothing happen at all. A window reading a file is never
/// idle, whatever the bookkeeping says.
fn idle_window(app: &AppHandle) -> Option<WebviewWindow> {
    let open = app.try_state::<OpenDocuments>()?;
    let reading = app.try_state::<OpenFiles>();
    let windows = app.webview_windows();
    let free = |window: &&WebviewWindow| {
        let label = window.label();
        !open.taken(label) && !reading.as_ref().is_some_and(|files| files.holds(label))
    };
    windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false) && free(window))
        .or_else(|| windows.values().find(free))
        .cloned()
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
                } else if let Some(window) = app
                    .get_webview_window(MAIN)
                    .or_else(|| app.webview_windows().values().next().cloned())
                {
                    // Started again with nothing to open: the reader is looking
                    // for a window they already have.
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
            load_keys,
            list_themes,
            save_theme,
            delete_theme,
            pick_pdf,
            open_document,
            open_for_reading,
            read_range,
            write_document,
            revert_write,
            original_document,
            document_writability,
            close_document,
            remember_position,
            set_open_document,
            new_window,
            quit_app,
            set_document_title,
            toggle_mark,
            add_highlight,
            remove_highlight,
            set_highlights,
            forget_document,
            open_link,
            open_path,
            reveal_document,
            print_document,
            ready,
            set_titlebar_buttons,
            log,
        ])
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let themes = config.join("themes");
            std::fs::create_dir_all(&config).ok();
            theme::install_built_ins(&themes);
            keys::install(&config);

            let stored = settings::load(&config);
            // Started after the shipped themes are written, so that writing
            // them is not itself the first thing it reports.
            app.manage(watch::start(app.handle().clone(), themes.clone()));
            // Empty when the reader would rather start fresh, so that nothing
            // below this line has to remember to ask again — including the
            // claim on the launch window, which is where forgetting cost most.
            let reopening = wants_reopening(&stored);
            let reopen = if reopening {
                library::prune(&library::load(&config)).open
            } else {
                Vec::new()
            };
            app.manage(Paths { config, themes });
            app.manage(OpenFiles::default());
            app.manage(OpenDocuments::default());
            app.manage(Exiting::default());
            app.manage(Placements::default());
            #[cfg(target_os = "macos")]
            dock::install(app.handle());
            // Every document that raced `setup` in as an Apple Event, in the
            // order they arrived, and emptied rather than sampled: what is
            // left in that static is left there for the life of the process.
            let early: Vec<String> = EARLY_OPEN
                .lock()
                .map(|mut queue| std::mem::take(&mut *queue))
                .unwrap_or_default();
            // One of them gets the launch window; the rest get a window each,
            // through `Restore` below. See `first_and_rest`.
            let (initial, extra) = first_and_rest(first_document_argument(), early);
            let pending = Pending::default();
            if let Some(path) = initial.clone() {
                pending.hold(MAIN, path);
            }
            app.manage(pending);

            // The launch window's own document is claimed here rather than
            // waiting for its frontend to report it: until it is, that window
            // reads as idle, and a second file double-clicked in the same
            // moment would be handed to it — over the top of the document the
            // launch was for.
            let opening = initial.clone().or_else(|| reopen.first().cloned());
            if let (Some(path), Some(open)) = (opening, app.try_state::<OpenDocuments>()) {
                open.set(MAIN, Some(&path));
            }

            if let Some(window) = app.get_webview_window(MAIN) {
                restore_window(&window, &stored);
                tidy_after(&window);

                // If the frontend never reports in, show the window anyway
                // rather than leaving the reader with nothing. Only if it is
                // still not up: `show` on a window that is already on screen
                // orders it to the front, and by three seconds in the session's
                // other windows have been restored — so this took the front off
                // whichever of them the reader had just been handed.
                let handle = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    if !handle.is_visible().unwrap_or(false) {
                        let _ = handle.show();
                    }
                });
            }

            // What needs a window of its own once the launch window is up.
            //
            // The documents this launch was *for* come first: a selection of
            // three files opened together is one document in the launch window
            // and two here. Then, only if the launch named no document at all,
            // the rest of the session that was open last — because opening a
            // file is not a request to reopen everything else as well.
            let mut more = extra;
            if reopening && initial.is_none() {
                more.extend(reopen.into_iter().skip(1));
            }
            app.manage(Restore(Mutex::new(more)));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HyloPDF")
        .run(|app, event| {
            // The app is leaving, so the windows about to be destroyed are not
            // windows the reader closed. See `Exiting`. Both events are watched
            // because they arrive in different orders: a quit from the menu bar
            // on macOS comes as `Exit` with no window events at all, and
            // `ExitRequested` is what everything else raises first.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                if let Some(exiting) = app.try_state::<Exiting>() {
                    exiting.now();
                }
            }

            // How macOS says "open this PDF": an Apple Event into the running
            // app rather than a second process. The variant exists on Apple
            // platforms alone — naming it anywhere else does not compile — so
            // it is matched there alone, and everywhere else the same job is
            // done by the single-instance plugin above.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        hand_over(app, path.to_string_lossy().to_string());
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cold launch onto three files double-clicked together is three Apple
    /// Events, all of them arriving before `setup` has run. One document can
    /// have the launch window; the other two have to come back out of the
    /// queue, or they are gone for the life of the process — which is what
    /// `EARLY_OPEN.pop()` did, silently, and to the *first* two of the three
    /// rather than the last.
    #[test]
    fn every_document_that_raced_setup_is_accounted_for() {
        let early = vec!["one.pdf".to_string(), "two.pdf".into(), "three.pdf".into()];
        let (first, rest) = first_and_rest(None, early);
        assert_eq!(
            first.as_deref(),
            Some("one.pdf"),
            "arrival order, not reverse"
        );
        assert_eq!(rest, vec!["two.pdf".to_string(), "three.pdf".into()]);
    }

    /// The command line named the document this launch was for, so it takes
    /// the launch window — and the ones that raced in still get windows.
    #[test]
    fn the_command_line_wins_the_launch_window_and_loses_nothing() {
        let (first, rest) = first_and_rest(Some("named.pdf".into()), vec!["raced.pdf".to_string()]);
        assert_eq!(first.as_deref(), Some("named.pdf"));
        assert_eq!(rest, vec!["raced.pdf".to_string()]);
    }

    /// The ordinary launch: nothing on the command line and nothing racing.
    #[test]
    fn an_empty_launch_asks_for_no_windows() {
        let (first, rest) = first_and_rest(None, Vec::new());
        assert!(first.is_none());
        assert!(rest.is_empty());
    }

    /// Two places act on this setting — `bootstrap`, for the launch window's
    /// own document, and `setup`, which claims that window in `OpenDocuments`
    /// on the strength of it. Both go through one function so they cannot
    /// disagree, and it has to fall the right way when the file says nothing.
    #[test]
    fn coming_back_to_what_was_open_is_a_setting_with_a_default() {
        assert!(
            settings::defaults().contains_key("reopen_last_document"),
            "the key `wants_reopening` reads has to be one the table knows, \
             or every file falls through to the default forever"
        );
        assert!(
            wants_reopening(&settings::defaults()),
            "on unless asked otherwise"
        );

        let mut off = settings::defaults();
        off.insert("reopen_last_document".into(), serde_json::json!(false));
        assert!(!wants_reopening(&off));

        // A hand-edited file can say something that is not a boolean at all.
        // `settings::load` drops it, so this should never be reached — and if
        // it ever is, the answer is the default rather than a panic.
        let mut nonsense = settings::defaults();
        nonsense.insert(
            "reopen_last_document".into(),
            serde_json::json!("yes please"),
        );
        assert!(wants_reopening(&nonsense));
    }
    /// A document handed over by the system when it is already open is a
    /// reader asking to look at it, not asking for a second copy of it beside
    /// the first — and it is how `print_document` comes back when the default
    /// PDF handler off macOS turns out to be this app.
    #[test]
    fn a_document_already_open_is_found_by_the_window_holding_it() {
        let open = OpenDocuments::default();
        open.set("main", Some("/papers/one.pdf"));
        open.set("reader-1", Some("/papers/two.pdf"));

        assert_eq!(open.showing("/papers/two.pdf").as_deref(), Some("reader-1"));
        assert_eq!(open.showing("/papers/one.pdf").as_deref(), Some("main"));
        assert!(open.showing("/papers/three.pdf").is_none());

        // Put down, and no longer open — the next hand-over of it wants an
        // idle window rather than the one that used to have it.
        open.set("reader-1", None);
        assert!(open.showing("/papers/two.pdf").is_none());

        // And a window that has moved on to something else does not answer
        // for what it used to hold.
        open.set("main", Some("/papers/three.pdf"));
        assert!(open.showing("/papers/one.pdf").is_none());
        assert_eq!(open.showing("/papers/three.pdf").as_deref(), Some("main"));
    }

    /// Its own directory per file, not shared across tests — `cargo test`
    /// runs this module's tests in parallel, and a directory holding more
    /// than one test's files means a scan for "no temp file survived" can
    /// catch a different test's write mid-flight.
    fn scratch_pdf(name: &str, body: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hylopdf-write-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("scratch file");
        path
    }

    /// The write door is asked before the gesture, not after the failure.
    /// A file the reader cannot write is the ordinary case this exists for:
    /// a paper in a folder somebody shared read-only, or a document opened
    /// off a mounted volume.
    #[test]
    fn a_read_only_document_says_so_rather_than_failing_later() {
        let path = scratch_pdf("locked.pdf", b"%PDF-1.7\n%%EOF\n");
        let ordinary = writability(&path);
        assert!(ordinary.writable, "{}", ordinary.reason);
        assert_eq!(ordinary.reason, "");
        assert_eq!(ordinary.size, 15);

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();

        let locked = writability(&path);
        // Root is allowed to write a read-only file, so a suite running as
        // root would find this writable and be right. Skip rather than fail:
        // the claim is about the probe, not about who is running it.
        if locked.writable {
            eprintln!("skipped: this user can write a read-only file");
        } else {
            assert_eq!(locked.reason, "locked.pdf is read only.");
        }

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
    }

    /// A document that is not there at all is not writable either, and the
    /// message says which document rather than which system call.
    #[test]
    fn a_document_that_is_gone_is_not_writable() {
        let missing = std::env::temp_dir().join(format!("hylopdf-gone-{}.pdf", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let answer = writability(&missing);
        assert!(!answer.writable);
        assert!(
            answer.reason.contains(".pdf"),
            "the reason should name the document: {}",
            answer.reason
        );
    }

    /// Syncing folders arrive personalised — "OneDrive - Acme", "Dropbox
    /// (Personal)" — so the match is a prefix within one whole component,
    /// and a folder that merely starts the same way somewhere in the middle
    /// of a name is not one.
    #[test]
    fn a_synced_folder_is_named_by_the_service_that_syncs_it() {
        let cases: &[(&str, Option<&str>)] = &[
            (
                "/Users/x/Dropbox (Personal)/papers/one.pdf",
                Some("Dropbox"),
            ),
            ("/Users/x/OneDrive - Acme/one.pdf", Some("OneDrive")),
            (
                "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/one.pdf",
                Some("iCloud Drive"),
            ),
            (
                "/Users/x/Google Drive/My Drive/one.pdf",
                Some("Google Drive"),
            ),
            ("/Users/x/Papers/one.pdf", None),
            // Not a synced folder: the name only contains one.
            ("/Users/x/Not Dropbox/one.pdf", None),
        ];
        for (path, service) in cases {
            assert_eq!(
                cloud_service(Path::new(path)).as_deref(),
                *service,
                "for {path}"
            );
        }
    }

    /// The write door's authorisation: it takes the same lock the read path
    /// does, which means the same question — is this the document *this*
    /// window has open — gets asked before a single byte moves.
    #[test]
    fn a_window_cannot_write_a_document_it_does_not_have_open() {
        let path = scratch_pdf("unopened.pdf", b"%PDF-1.7\n%%EOF\n");
        let open = OpenFiles::default();
        let err = open
            .write("main", &path.to_string_lossy(), b"anything")
            .unwrap_err();
        assert_eq!(err, "No document is open.");

        let elsewhere = scratch_pdf("elsewhere.pdf", b"%PDF-1.7\n%%EOF\n");
        open.begin("main", &elsewhere.to_string_lossy()).unwrap();
        let err = open
            .write("main", &path.to_string_lossy(), b"anything")
            .unwrap_err();
        assert_eq!(err, "That is not the document that is open.");
    }

    /// The write itself: atomic (no half-written file is ever visible under
    /// the target name), and the handle a window reads through afterwards
    /// sees the new bytes rather than the detached old inode.
    #[test]
    fn writing_replaces_the_file_and_the_handle_reading_it() {
        let path = scratch_pdf("write-me.pdf", b"%PDF-1.7\noriginal\n%%EOF\n");
        let open = OpenFiles::default();
        open.begin("main", &path.to_string_lossy()).unwrap();

        let new_bytes = b"%PDF-1.7\noriginal\nappended\n%%EOF\n";
        let length = open
            .write("main", &path.to_string_lossy(), new_bytes)
            .unwrap();
        assert_eq!(length, new_bytes.len() as u64);
        assert_eq!(std::fs::read(&path).unwrap(), new_bytes);

        // No staging file left behind beside it.
        let siblings: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(siblings.is_empty(), "a temp file survived the write");

        // And the handle this window reads through sees the new bytes, not
        // whatever the old, now-detached inode still holds.
        let read = open
            .range("main", &path.to_string_lossy(), 0, new_bytes.len() as u64)
            .unwrap();
        assert_eq!(read, new_bytes);
    }

    /// The first write to a document leaves a pristine copy beside it; a
    /// second write must not overwrite that copy with the document's
    /// already-modified state.
    #[test]
    fn only_the_first_write_creates_the_original_backup() {
        let path = scratch_pdf("backed-up.pdf", b"%PDF-1.7\nfirst draft\n%%EOF\n");
        let open = OpenFiles::default();
        open.begin("main", &path.to_string_lossy()).unwrap();

        open.write(
            "main",
            &path.to_string_lossy(),
            b"%PDF-1.7\nsecond draft\n%%EOF\n",
        )
        .unwrap();
        let backup = original_backup_path(&path);
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            b"%PDF-1.7\nfirst draft\n%%EOF\n"
        );

        open.write(
            "main",
            &path.to_string_lossy(),
            b"%PDF-1.7\nthird draft\n%%EOF\n",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            b"%PDF-1.7\nfirst draft\n%%EOF\n",
            "a later write clobbered the original backup"
        );
    }

    /// `original_document` reads through `is_open` first — the same door
    /// `range` and `write` use — so a window naming a document it does not
    /// have open cannot read the backup beside somebody else's, and the
    /// backup a first write left behind is exactly the original bytes.
    #[test]
    fn is_open_guards_the_backup_the_same_way_writing_does() {
        let path = scratch_pdf("removable.pdf", b"%PDF-1.7\nfirst draft\n%%EOF\n");
        let open = OpenFiles::default();
        assert!(!open.is_open("main", &path.to_string_lossy()));

        open.begin("main", &path.to_string_lossy()).unwrap();
        assert!(open.is_open("main", &path.to_string_lossy()));

        let elsewhere = scratch_pdf("elsewhere-removable.pdf", b"%PDF-1.7\n%%EOF\n");
        assert!(!open.is_open("main", &elsewhere.to_string_lossy()));

        open.write(
            "main",
            &path.to_string_lossy(),
            b"%PDF-1.7\nfirst draft\nappended\n%%EOF\n",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(original_backup_path(&path)).unwrap(),
            b"%PDF-1.7\nfirst draft\n%%EOF\n"
        );
    }

    /// Undoing a write is not an edit to the annotation it added — pdf.js has
    /// no way to make one — it is truncating the file back to the length it
    /// had before that write, which is where the write's own incremental
    /// update began.
    #[test]
    fn reverting_a_write_restores_the_bytes_from_before_it() {
        let original = b"%PDF-1.7\noriginal\n%%EOF\n";
        let path = scratch_pdf("undo-me.pdf", original);
        let open = OpenFiles::default();
        open.begin("main", &path.to_string_lossy()).unwrap();

        let marked = b"%PDF-1.7\noriginal\n%%EOF\nappended highlight";
        let after = open.write("main", &path.to_string_lossy(), marked).unwrap();
        assert_eq!(after, marked.len() as u64);

        open.revert(
            "main",
            &path.to_string_lossy(),
            after,
            original.len() as u64,
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);

        // No staging file left behind by the revert either.
        let siblings: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(siblings.is_empty(), "a temp file survived the revert");

        // And the handle reads the reverted bytes, not the detached old inode.
        let read = open
            .range("main", &path.to_string_lossy(), 0, original.len() as u64)
            .unwrap();
        assert_eq!(read, original);
    }

    /// A revert is refused, rather than acted on, the moment the file no
    /// longer matches what the caller thinks it is undoing — a second
    /// highlight since, or a recompile — because the length it was given
    /// would then cut into bytes that were never the ones this write added.
    #[test]
    fn a_revert_is_refused_once_the_file_has_moved_on() {
        let path = scratch_pdf("moved-on.pdf", b"%PDF-1.7\noriginal\n%%EOF\n");
        let open = OpenFiles::default();
        open.begin("main", &path.to_string_lossy()).unwrap();

        let marked = b"%PDF-1.7\noriginal\n%%EOF\nappended highlight";
        open.write("main", &path.to_string_lossy(), marked).unwrap();

        // A second write lands — from another highlight, or an external
        // recompile — before the first is undone.
        let moved_on = b"%PDF-1.7\noriginal\n%%EOF\nappended highlight\nand more";
        open.write("main", &path.to_string_lossy(), moved_on)
            .unwrap();

        let err = open
            .revert("main", &path.to_string_lossy(), marked.len() as u64, 9)
            .unwrap_err();
        assert_eq!(
            err,
            "This document has changed since, so that mark can no longer be undone."
        );
        // Refused, and untouched: still the second write's bytes.
        assert_eq!(std::fs::read(&path).unwrap(), moved_on);
    }
}
