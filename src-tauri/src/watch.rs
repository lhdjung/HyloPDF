//! What changes on the disk while the app is running.
//!
//! Two of the files the reader is looking at are not the app's to change. A
//! theme is TOML precisely so that somebody — or something — can open it in an
//! editor, and until now an edit was only seen at the next launch, which is a
//! poor way to pick a colour. The document is often a paper being recompiled
//! by LaTeX under the reader, and reopening it by hand to see the new draft is
//! the same poor way.
//!
//! Both are news the file system already has, and asking for it belongs here
//! rather than on the other side of the bridge: the work is on the disk, the
//! payload is a filename, and nothing crosses that is bigger than the answer.
//!
//! One thread owns one watcher and both subjects. Three things shape it.
//!
//! *A change is a burst, not an event.* A save from an editor is three or four
//! events, an atomic write is a create and a rename, and a LaTeX run is
//! hundreds over several seconds. Everything is collected until the file
//! system has been quiet for `SETTLE` and then acted on once.
//!
//! *A theme reload is decided by what the files say, not by what moved.* The
//! app writes into this directory itself — the shipped themes on every run, a
//! saved theme, the staging file `atomic_write` renames over — so an event is
//! not news. The themes are loaded and compared against the last set handed
//! over, and nothing is emitted unless they actually differ.
//!
//! *A document is believed only once it is whole.* A compiler writes its
//! output across the whole of a run, and what is on the disk in the middle of
//! one starts like a PDF and stops mid-sentence. Handing that over would take
//! the reader's document away and leave them on the start screen, so the file
//! has to end the way a PDF ends, and hold still, before anyone hears about
//! it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use notify::{EventKind, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::theme;

/// How long the file system has to be quiet before a burst counts as one
/// change.
const SETTLE: Duration = Duration::from_millis(250);

/// And how long a document has to hold its size before it is believed.
const STEADY: Duration = Duration::from_millis(150);

/// How much of the end of a document to read looking for `%%EOF`. Generous:
/// the marker is the last line, but nothing forbids whitespace after it.
const TAIL: u64 = 1024;

/// The handle the rest of the app holds. Its only verb is "follow this
/// document instead", which `open_for_reading` and `close_document` say
/// between them.
pub struct Watching(Mutex<Sender<Signal>>);

impl Watching {
    /// Follow this document, or stop following one.
    pub fn document(&self, path: Option<&str>) {
        let signal = Signal::Follow(path.map(PathBuf::from));
        if let Ok(sender) = self.0.lock() {
            let _ = sender.send(signal);
        }
    }
}

enum Signal {
    Touched(Vec<PathBuf>),
    Follow(Option<PathBuf>),
}

/// The document being followed, the directory the watch is actually on, and
/// what the file looked like when it was last whole.
struct Followed {
    path: PathBuf,
    dir: PathBuf,
    mark: Option<(u64, SystemTime)>,
}

/// Start watching the themes directory. The document half stays idle until
/// something is opened.
///
/// A watcher that cannot be created, or a directory that cannot be watched, is
/// not worth a message: the app then behaves exactly as it did before any of
/// this, which is to notice at the next launch.
pub fn start(app: AppHandle, themes: PathBuf) -> Watching {
    let (sender, receiver) = mpsc::channel();
    let events = sender.clone();

    std::thread::spawn(move || {
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if let Ok(event) = result {
                // Reading a file is not changing it, and on macOS the reads
                // are ours: every page of the open document comes through
                // `read_range`.
                if !matches!(event.kind, EventKind::Access(_)) {
                    let _ = events.send(Signal::Touched(event.paths));
                }
            }
        });
        let Ok(mut watcher) = watcher else { return };
        let _ = watcher.watch(&themes, RecursiveMode::NonRecursive);
        run(app, themes, receiver, &mut watcher);
    });

    Watching(Mutex::new(sender))
}

fn run(app: AppHandle, themes: PathBuf, receiver: Receiver<Signal>, watcher: &mut dyn Watcher) {
    // What the frontend already has. Compared against, never emitted blindly.
    let mut known = theme::load_all(&themes);
    let mut document: Option<Followed> = None;

    while let Ok(first) = receiver.recv() {
        let mut touched: Vec<PathBuf> = Vec::new();
        let mut pending = Some(first);

        // Collect until things go quiet. `Follow` is handled as it arrives —
        // a document opened in the middle of a burst is still the document to
        // watch when the burst ends.
        loop {
            match pending.take() {
                Some(Signal::Touched(paths)) => touched.extend(paths),
                Some(Signal::Follow(next)) => follow(watcher, &themes, &mut document, next),
                None => {}
            }
            match receiver.recv_timeout(SETTLE) {
                Ok(next) => pending = Some(next),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        if touched
            .iter()
            .any(|path| path.parent() == Some(themes.as_path()))
        {
            let current = theme::load_all(&themes);
            if current != known {
                known = current;
                let _ = app.emit("themes-changed", &known);
            }
        }

        if let Some(followed) = document.as_mut() {
            if touched.iter().any(|path| path == &followed.path) {
                if let Some(mark) = whole(&followed.path) {
                    if Some(mark) != followed.mark {
                        followed.mark = Some(mark);
                        let path = followed.path.to_string_lossy().to_string();
                        let _ = app.emit("document-changed", path);
                    }
                }
            }
        }
    }
}

/// Point the document watch somewhere else, or nowhere.
///
/// The watch goes on the directory rather than on the file. A document is
/// replaced by writing another one beside it and renaming it over the top —
/// which is what `atomic_write` does here and what most compilers do — and a
/// watch on a file follows the file it was opened on rather than the name,
/// so it would go on watching something nobody can see any more.
fn follow(
    watcher: &mut dyn Watcher,
    themes: &Path,
    held: &mut Option<Followed>,
    next: Option<PathBuf>,
) {
    // Being pointed at the document already being followed is not a change of
    // subject, and treating it as one is worse than doing nothing. It is also
    // the ordinary case: a reload goes back through `open_for_reading`, which
    // says "follow this" about the document it has just reopened. Remaking the
    // watch would lose whatever landed in the gap, and — the part that bites —
    // the baseline would be retaken from what is on the disk *now* rather than
    // from the version just handed over, so a draft that arrived during the
    // reload would match its own mark and never be reported at all.
    if let (Some(current), Some(next)) = (held.as_ref(), next.as_ref()) {
        if &current.path == next {
            return;
        }
    }

    if let Some(old) = held.take() {
        // The themes directory is watched for its own sake, and must not be
        // dropped along with a document that happened to be sitting in it.
        if old.dir != themes {
            let _ = watcher.unwatch(&old.dir);
        }
    }
    let Some(path) = next else { return };
    let Some(dir) = path.parent().map(Path::to_path_buf) else {
        return;
    };
    if dir != themes && watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    // What it looks like now is the baseline: opening a document is not a
    // reason to reload it.
    *held = Some(Followed {
        mark: whole(&path),
        path,
        dir,
    });
}

/// A document's size and time, if what is on the disk is a whole PDF.
///
/// Cheap on purpose — five bytes at the front, a kilobyte at the back, and one
/// look at the size again after a pause. None of it proves the document is
/// readable; all of it rules out the case that actually happens, which is
/// catching a compiler halfway through writing one.
fn whole(path: &Path) -> Option<(u64, SystemTime)> {
    let mark = identity(path)?;
    let mut file = File::open(path).ok()?;

    let mut head = [0u8; 5];
    file.read_exact(&mut head).ok()?;
    if &head != b"%PDF-" {
        return None;
    }

    file.seek(SeekFrom::Start(mark.0.saturating_sub(TAIL)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    if !tail.windows(5).any(|window| window == b"%%EOF") {
        return None;
    }

    // Still being written to? Then this is a moment in the middle of a run
    // that happens to end in `%%EOF`, and the next burst will ask again.
    std::thread::sleep(STEADY);
    if identity(path)? != mark {
        return None;
    }
    Some(mark)
}

fn identity(path: &Path) -> Option<(u64, SystemTime)> {
    let data = std::fs::metadata(path).ok()?;
    let length = data.len();
    if length == 0 {
        return None;
    }
    Some((length, data.modified().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate that stands between the reader and a compiler's half-written
    /// output. Everything else here is the file system's word for it; this is
    /// the one judgement the module makes on its own.
    fn scratch(name: &str, body: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hylopdf-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("scratch file");
        path
    }

    #[test]
    fn a_finished_document_is_whole() {
        let path = scratch("done.pdf", b"%PDF-1.7\n... objects ...\ntrailer\n%%EOF\n");
        assert!(whole(&path).is_some());
    }

    #[test]
    fn a_document_still_being_written_is_not() {
        // What pdflatex leaves on the disk in the middle of a run: the header
        // is there from the first write, and the marker that ends a PDF is the
        // last thing to arrive.
        let path = scratch("running.pdf", b"%PDF-1.7\n... objects ...\n");
        assert!(whole(&path).is_none());
    }

    #[test]
    fn an_empty_file_is_not() {
        let path = scratch("empty.pdf", b"");
        assert!(whole(&path).is_none());
    }

    #[test]
    fn something_that_is_not_a_pdf_is_not() {
        let path = scratch("notes.txt", b"%%EOF is in here but this is not a document");
        assert!(whole(&path).is_none());
    }

    /// A long document must not be judged by its first kilobyte: the marker is
    /// at the end, and only the end is read.
    #[test]
    fn the_marker_is_looked_for_at_the_end() {
        let mut body = b"%PDF-1.7\n".to_vec();
        body.extend(std::iter::repeat(b'x').take(64 * 1024));
        body.extend_from_slice(b"\n%%EOF\n");
        let path = scratch("long.pdf", &body);
        assert!(whole(&path).is_some());
    }

    /// A watcher that does nothing but say what it was asked to do, so that
    /// `follow` can be checked without a file system underneath it.
    #[derive(Default)]
    struct Recorded {
        watched: Vec<PathBuf>,
        unwatched: Vec<PathBuf>,
    }

    impl Watcher for Recorded {
        fn new<F: notify::EventHandler>(_: F, _: notify::Config) -> notify::Result<Self> {
            unreachable!("built directly, never through the trait")
        }

        fn watch(&mut self, path: &Path, _: RecursiveMode) -> notify::Result<()> {
            self.watched.push(path.to_path_buf());
            Ok(())
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            self.unwatched.push(path.to_path_buf());
            Ok(())
        }

        fn kind() -> notify::WatcherKind {
            notify::WatcherKind::NullWatcher
        }
    }

    /// Every reload says "follow this" about the document it has just reopened,
    /// and that must change nothing. Remaking the watch would lose whatever
    /// landed in the gap, and retaking the baseline would swallow a draft that
    /// arrived during the reload — the next burst would find the file matching
    /// its own mark and report nothing.
    #[test]
    fn reopening_the_same_document_leaves_the_watch_alone() {
        let path = scratch("reopened.pdf", b"%PDF-1.7\ntrailer\n%%EOF\n");
        let dir = path.parent().expect("a parent").to_path_buf();
        let themes = dir.join("themes");
        let mut watcher = Recorded::default();
        let mut held = None;

        follow(&mut watcher, &themes, &mut held, Some(path.clone()));
        let first = held.as_ref().expect("followed").mark;
        assert_eq!(watcher.watched, vec![dir.clone()]);

        // A new draft lands, and the reload it causes comes back through here.
        std::fs::write(&path, b"%PDF-1.7\nrather longer than it was\n%%EOF\n").expect("rewrite");
        follow(&mut watcher, &themes, &mut held, Some(path.clone()));

        assert!(watcher.unwatched.is_empty(), "the watch was remade");
        assert_eq!(watcher.watched, vec![dir]);
        assert_eq!(
            held.expect("still followed").mark,
            first,
            "the baseline moved"
        );
    }

    /// A different document, though, is a different subject.
    #[test]
    fn another_document_moves_the_watch() {
        let first = scratch("first.pdf", b"%PDF-1.7\n%%EOF\n");
        let dir = first.parent().expect("a parent").to_path_buf();
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("second directory");
        let second = elsewhere.join("second.pdf");
        std::fs::write(&second, b"%PDF-1.7\n%%EOF\n").expect("second document");

        let themes = dir.join("themes");
        let mut watcher = Recorded::default();
        let mut held = None;

        follow(&mut watcher, &themes, &mut held, Some(first));
        follow(&mut watcher, &themes, &mut held, Some(second.clone()));

        assert_eq!(watcher.unwatched, vec![dir.clone()]);
        assert_eq!(watcher.watched, vec![dir, elsewhere]);
        assert_eq!(held.expect("followed").path, second);
    }
}
