//! What the reader remembers between runs: the settings table, and the themes
//! it is choosing from.
//!
//! [`crate::settings`] and [`crate::theme`] are the app's own modules, mounted
//! into this crate unchanged, and they are deliberately about the disk and
//! nothing else — a settings table is a map of scalars, a theme is a file.
//! This is the layer above them that the reader talks to: which theme is in
//! use, what it resolves to, and a way to change a setting that writes it down
//! without the caller thinking about it.
//!
//! **There is no bridge here, and that is the whole of what this replaces.**
//! In the app the same work is `api.ts` (898 lines, deleted by this design),
//! thirty-three `#[tauri::command]`s, a browser twin of every one of them, and
//! `settings.test.mjs` existing solely because the table is written out three
//! times. Here a component calls a method. The settings table is stated once,
//! in the file the app states it in, and there is no second copy to drift.
//!
//! *One write is off the main thread, and only one needs to be.* `set_many`
//! is a read-modify-write of a small TOML file and happens on whichever
//! thread asked, which is fine for a theme or a zoom — a reader changes those
//! a handful of times a session. Where the reader *is* moves sixty times a
//! second, and the app moved that one to an `async` command for exactly that
//! reason: a whole-file rewrite of `library.toml` was landing in the middle of
//! the one gesture this app exists to make smooth. `Scribe` is that here —
//! one thread, one pending place per document, written when the scrolling
//! stops. The lock inside `library.rs` is what makes it safe, and it was
//! already there.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

/// What a mark kept beside the document is written down as being: the app's
/// own `MARKUP_OPACITY`, so a journal this reader writes and the app reads
/// says what the app would have said. Nothing here draws with it — a mark in
/// the file is drawn by pdfium out of the appearance stream pdfium generates.
const MARKUP_OPACITY: f64 = 0.35;

use crate::keys;
use crate::layout::Anchor;
use crate::library::{self, Highlight, Mark};
use crate::palette::{self, Palette};
use crate::settings::{self, Settings};
use crate::theme;

/// How long the scrolling has to stop for before where the reader is gets
/// written down. `onScroll` in `main.ts` is the same number, spelled
/// `setTimeout(… , 700)`.
const SETTLE: Duration = Duration::from_millis(700);

/// The one thread that writes down where the reader is.
///
/// **The only write in this crate that needed moving, and the reason is the
/// rate.** A theme is chosen a few times a session and a zoom a few dozen; the
/// scroll offset changes on every wheel event, and every change is a
/// read-modify-write of the whole of `library.toml`. Done on the thread that
/// draws the window, that is the app's own bug — `AGENTS.md` describes a
/// whole-file rewrite landing in the middle of the one gesture this app exists
/// to make smooth — and this crate would have had it the moment the position
/// was remembered at all.
///
/// Two things it does, and they are separable but not usefully so. It moves
/// the write off the thread that is scrolling, and it *coalesces*: a place
/// arriving while another is pending replaces it, and nothing is written until
/// [`SETTLE`] has passed with nothing new. So a reader scrolling through a
/// chapter costs one write at the end of it rather than four hundred.
///
/// **Pending places are keyed by document**, which matters for one reason
/// only: `cargo test` runs its tests in parallel and this thread is the
/// process's. A single pending slot would have one test's position quietly
/// replacing another's, intermittently and by timing, which is the worst
/// shape a test failure comes in.
struct Scribe {
    jobs: Sender<Job>,
}

enum Job {
    /// Where the reader is in one document, superseding whatever was pending
    /// for it.
    Place {
        dir: PathBuf,
        file: String,
        page: u32,
        offset: f64,
    },
    /// Write everything pending now and say when it is done. What quitting
    /// asks for, and what a test asks for instead of sleeping.
    Flush(Sender<()>),
}

impl Scribe {
    /// The process's, made on first use. There is no way to stop it and
    /// nothing that would want to: it is asleep on a channel except when
    /// there is something to write.
    fn get() -> &'static Scribe {
        static SCRIBE: OnceLock<Scribe> = OnceLock::new();
        SCRIBE.get_or_init(|| {
            let (jobs, inbox) = mpsc::channel();
            std::thread::Builder::new()
                .name("hylopdf-library".into())
                .spawn(move || run(inbox))
                .expect("a thread to write the library on");
            Scribe { jobs }
        })
    }
}

/// Wait, coalesce, write.
///
/// The shape is `clearTimeout` and `setTimeout` from `main.ts` turned inside
/// out: with nothing pending this blocks for ever, and with something pending
/// it waits [`SETTLE`] for a newer answer and writes when none comes. A place
/// arriving in the meantime restarts the wait, which is what makes a
/// continuous scroll cost one write — and means, exactly as in the app, that
/// a scroll which never pauses is not written down until something asks for a
/// flush.
fn run(inbox: Receiver<Job>) {
    let mut pending: BTreeMap<(PathBuf, String), (u32, f64)> = BTreeMap::new();
    loop {
        let job = if pending.is_empty() {
            inbox.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            inbox.recv_timeout(SETTLE)
        };
        match job {
            Ok(Job::Place {
                dir,
                file,
                page,
                offset,
            }) => {
                pending.insert((dir, file), (page, offset));
            }
            Ok(Job::Flush(done)) => {
                write_out(&mut pending);
                // The sender may be gone — a test that stopped waiting — and
                // that is not this thread's problem.
                let _ = done.send(());
            }
            Err(RecvTimeoutError::Timeout) => write_out(&mut pending),
            Err(RecvTimeoutError::Disconnected) => {
                write_out(&mut pending);
                return;
            }
        }
    }
}

fn write_out(pending: &mut BTreeMap<(PathBuf, String), (u32, f64)>) {
    for ((dir, file), (page, offset)) in std::mem::take(pending) {
        // A library that cannot be written is a reader who loses their place,
        // which is worth nothing at all on a thread with nowhere to say it.
        // The notice for that case is raised at open, where the same file is
        // written by `touch` and somebody is looking at the screen.
        let _ = library::remember(&dir, &file, page, offset);
    }
}

/// Write down everything the scribe is holding, and wait for it.
///
/// Called on the way out — `main.rs`, once the event loop has returned — and
/// by any test that wants to reopen a reader and find its place kept. Without
/// it a run that ends while somebody is still scrolling loses the last
/// seven hundred milliseconds of reading, which is a page.
pub fn flush() {
    let (done, wait) = mpsc::channel();
    if Scribe::get().jobs.send(Job::Flush(done)).is_ok() {
        // Two seconds is not a timeout anybody should reach; it is here so
        // that a thread which has somehow died cannot hold up a quit.
        let _ = wait.recv_timeout(Duration::from_secs(2));
    }
}

/// Whether a document's own `/Title` is worth calling it by.
///
/// `worthCalling` in `main.ts`, and every line of it is a fact about what
/// producers actually write rather than about this app. A great many PDFs
/// carry a title filled in by the program that made them and not by anybody:
/// the file name again, the file name of the *source* — "Microsoft Word -
/// report.doc" — or the word "untitled". Each of those is worse than the file
/// name, because it looks deliberate. Anything that fails leaves the file name
/// alone, which is what it was before.
pub fn worth_calling(title: &str, file_name: &str) -> bool {
    let title = title.trim();
    if title.chars().count() < 4 || title.chars().count() > 200 {
        return false;
    }
    let folded = title.to_lowercase();
    let name = file_name.to_lowercase();
    let stem = name.strip_suffix(".pdf").unwrap_or(&name);
    if folded == stem || folded == name {
        return false;
    }
    if folded.starts_with("untitled")
        && !folded[8..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    {
        return false;
    }
    if folded.starts_with("microsoft word -") {
        return false;
    }
    if let Some(rest) = folded.strip_prefix("document") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    // A title that is a file name is a file name, whatever file it names.
    const SUFFIXES: &[&str] = &[
        ".pdf", ".doc", ".docx", ".tex", ".indd", ".ppt", ".pptx", ".odt", ".rtf", ".ps", ".dvi",
    ];
    if SUFFIXES.iter().any(|suffix| folded.ends_with(suffix)) {
        return false;
    }
    true
}

/// What to call a document: its own `/Title` where that is worth having, and
/// the file's name where it is not.
///
/// A function rather than a method for the reason [`reopening`] is one: a
/// window is given its title before there is a [`Store`] to ask, because a
/// window's title is an attribute handed to the builder. `Store::opened`
/// decides the same thing the same way, and this is the deciding.
pub fn called(path: &str, declared: &str) -> String {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    if worth_calling(declared, &name) {
        declared.trim().to_string()
    } else {
        name
    }
}

/// What was open when the reader was last put down, if it is still there and
/// the reader wants it back.
///
/// Read before there is a window, which is why it is a function rather than a
/// method: `main.rs` has to know what to open before it can make the thing
/// that would hold a [`Store`].
///
/// `prune` is the app's own and is what keeps this honest — a document that
/// has been moved or deleted would otherwise be reopened, and fail, on every
/// launch for ever. `reopen_last_document` is the app's own setting too, and
/// is asked here rather than left to the caller for the reason `bootstrap`
/// gives in `lib.rs`: two sides that each assume the other checked it are two
/// sides that disagree about whether the window has anything in it.
pub fn reopening(dir: &Path) -> Option<String> {
    reopening_all(dir).into_iter().next()
}

/// The whole of it: one path per window that was open, in the order the
/// windows were made.
///
/// `library.open` has been a list since the app had two windows, and this is
/// where that stops being a list of one. The first is the launch window's and
/// the rest are windows of their own — see `main.rs`.
pub fn reopening_all(dir: &Path) -> Vec<String> {
    let settings = settings::load(dir);
    let wanted = settings
        .get("reopen_last_document")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !wanted {
        return Vec::new();
    }
    library::prune(&library::load(dir)).open
}

pub struct Store {
    dir: PathBuf,
    themes_dir: PathBuf,
    settings: Settings,
    themes: Vec<theme::Theme>,
    /// A theme chosen for this run and not written down, which is what
    /// `--theme` is. A flag that quietly rewrote a setting would be a flag
    /// that changes what the *next* run does, which is not what a flag means.
    for_now: Option<usize>,
    /// Colours a theme names that the renderer cannot read, if any — raised
    /// once, as the app's `unreadableColors` notice is.
    pub complaint: Option<String>,
    /// The document this reader has open, as the library names one: the path,
    /// as given. Empty until [`Store::opened`] is called, which is what puts
    /// the document into `library.toml` and is the only reason a mark has
    /// anywhere to go.
    file: String,
    /// The pins in that document, kept in memory so that drawing the sidebar
    /// is not a read of a file per frame. The file is still the record —
    /// every change here is written through, and the write is what the next
    /// run reads.
    marks: Vec<Mark>,
    /// Markup kept beside the document because it could not go into it. See
    /// [`Store::journal`].
    journal: Vec<Highlight>,
    /// What the document is called on the shelf: its own `/Title` where that
    /// is worth having, and the file's name where it is not. Decided once, at
    /// open, by [`worth_calling`].
    title: String,
}

impl Store {
    /// The reader's own directory, with the shipped themes written into it.
    pub fn open() -> Store {
        Store::at(&crate::config::config_dir())
    }

    /// One stated directory, which is what a test has and what
    /// `HYLOPDF_CONFIG` gives a run.
    pub fn at(dir: &Path) -> Store {
        let themes_dir = dir.join("themes");
        // On every run, so that a shipped theme whose colours change reaches a
        // machine that already has the old one. See the comment on the
        // function: a built-in edited in place is overwritten, deliberately,
        // and every shipped file carries a banner saying so.
        theme::install_built_ins(&themes_dir);
        let themes = theme::load_all(&themes_dir);
        // Once, and then never again: unlike a shipped theme this file is the
        // reader's from the moment it exists, and every line of the template
        // is a comment. `keys::install` is the app's own and says why.
        keys::install(dir);
        let mut store = Store {
            settings: settings::load(dir),
            dir: dir.to_path_buf(),
            themes_dir,
            themes,
            for_now: None,
            complaint: None,
            file: String::new(),
            marks: Vec::new(),
            journal: Vec::new(),
            title: String::new(),
        };
        store.complaint = store.unreadable();
        store
    }

    /// What `keys.toml` says, and the lines of it that were not usable.
    ///
    /// Read rather than held, because the one thing that will want it twice
    /// is a Reload button — the app has one on its Keyboard page, for the
    /// reason `keys.rs` gives: this directory is written to several times a
    /// minute while somebody is scrolling, so a watcher over it would be
    /// answering its own writes.
    ///
    /// The bindings are carried across as written. What an action *name*
    /// means, and whether a chord can be read at all, is
    /// [`crate::keymap`]'s — which is the same split the app has across its
    /// bridge, and it did not have to move to get here.
    pub fn keyboard(&self) -> keys::Keys {
        keys::load(&self.dir)
    }

    pub fn themes(&self) -> &[theme::Theme] {
        &self.themes
    }

    /// The themes directory, which the settings window and the watcher will
    /// both want and neither exists yet.
    pub fn themes_dir(&self) -> &Path {
        &self.themes_dir
    }

    /// Where `settings.toml` and `keys.toml` live — the About page names it,
    /// because a reader who is told their settings are a plain file is owed
    /// the path to it.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the theme in use sits in [`Store::themes`].
    ///
    /// A theme is remembered by **id**, not by position: the list changes when
    /// somebody adds a file to the directory, and a position would then mean a
    /// different theme than it did yesterday. A id naming nothing falls back
    /// to the default light theme rather than to nothing, which is what makes
    /// deleting the theme you are wearing survivable.
    pub fn theme_index(&self) -> usize {
        if let Some(index) = self.for_now.filter(|&index| index < self.themes.len()) {
            return index;
        }
        let wanted = self.text("theme");
        self.themes
            .iter()
            .position(|theme| theme.id == wanted)
            .or_else(|| {
                self.themes
                    .iter()
                    .position(|theme| theme.id == theme::DEFAULT_LIGHT)
            })
            .unwrap_or(0)
    }

    pub fn theme(&self) -> &theme::Theme {
        &self.themes[self.theme_index()]
    }

    /// The theme in use, as colours. `recolor_images` is the setting that says
    /// whether a pixel with a colour of its own keeps it, which is why it is
    /// read here rather than off the theme.
    pub fn palette(&self) -> Palette {
        palette::resolve(self.theme(), self.flag("recolor_images"))
    }

    /// Wear a theme, by its place in the list.
    ///
    /// Two settings move together, which is what `set_many` is for: the theme
    /// in use, and the light or dark slot it fills — so that following the
    /// machine's appearance later comes back to the theme somebody chose for
    /// that half rather than to the shipped default. Which slot it fills is
    /// read off the theme's own paper, because that is the only thing that
    /// actually makes a theme dark.
    pub fn wear(&mut self, index: usize) -> String {
        let Some(theme) = self.themes.get(index) else {
            return String::new();
        };
        // Choosing one by hand is the reader deciding, which outranks a flag
        // and is written down.
        self.for_now = None;
        let (id, name) = (theme.id.clone(), theme.name.clone());
        let slot = if self.is_dark(theme) {
            "dark_theme"
        } else {
            "light_theme"
        };
        self.set(vec![("theme".into(), json!(id)), (slot.into(), json!(id))]);
        self.complaint = self.unreadable();
        name
    }

    /// The themes, again, because one of the files changed.
    ///
    /// The whole set arrives rather than a filename — that is what
    /// `themes-changed` carries, and fourteen themes of five colours is
    /// cheaper to send than to ask for. Nothing is written down: nobody chose
    /// a theme here, and an editor saving a file every few seconds must not
    /// be a rewrite of `settings.toml` every few seconds.
    pub fn set_themes(&mut self, themes: Vec<theme::Theme>) {
        self.themes = themes;
        self.complaint = self.unreadable();
    }

    /// What to wear instead of a theme whose file has gone.
    ///
    /// `replacementFor` in `main.ts`, in order: the theme remembered for that
    /// half of the pair, else anything of the same darkness, else whatever is
    /// left. The point of the order is that somebody who was reading in a
    /// dark theme is not put into a light one because a file was deleted.
    pub fn replacement_for(&self, gone: &theme::Theme) -> Option<usize> {
        let dark = self.is_dark(gone);
        let remembered = self.text(if dark { "dark_theme" } else { "light_theme" });
        let left = || {
            self.themes
                .iter()
                .enumerate()
                .filter(|(_, theme)| theme.id != gone.id)
        };
        left()
            .find(|(_, theme)| theme.id == remembered)
            .or_else(|| left().find(|(_, theme)| self.is_dark(theme) == dark))
            .or_else(|| left().next())
            .map(|(index, _)| index)
    }

    /// Wear a theme for this run only. See `for_now`.
    pub fn wear_for_now(&mut self, index: usize) {
        self.for_now = Some(index);
        self.complaint = self.unreadable();
    }

    /// Whether a theme is the dark one of the pair, judged by its paper.
    fn is_dark(&self, theme: &theme::Theme) -> bool {
        let paper = palette::read_colour(&theme.background).unwrap_or(palette::FALLBACK.background);
        let luma = 0.2126 * paper[0] as f64 + 0.7152 * paper[1] as f64 + 0.0722 * paper[2] as f64;
        luma < 128.0
    }

    fn unreadable(&self) -> Option<String> {
        let theme = self.theme();
        let bad = palette::unreadable(theme);
        if bad.is_empty() {
            return None;
        }
        Some(format!(
            "{} names {} the renderer cannot read — colours are #abc, #abcd, #aabbcc or #aabbccdd",
            theme.name,
            bad.join(" and "),
        ))
    }

    /* ------------------------------------------------------- the library */

    /// Say which document this reader has open, and read back what is already
    /// known about it: what to call it, and where the last run left off.
    ///
    /// `touch` is the app's own, and it does two things at once: it moves the
    /// document to the front of the recently-read list, and it *makes an
    /// entry* if there is none. The second is why this is called on open and
    /// not left until somebody marks a page — `toggle_mark` refuses a
    /// document that is not in the library, which is the right answer to a
    /// stale path and the wrong one to a document that was opened a moment
    /// ago.
    ///
    /// `declared` is what the document calls itself, as written — see
    /// [`crate::render::PageSource::title`]. Whether it is worth using is
    /// [`worth_calling`]'s to say, and it is asked *here* rather than at the
    /// renderer because it is the one place that also has the file name to
    /// weigh it against. The app asks the same question a moment later, in
    /// `adoptDocumentTitle`, because pdf.js cannot answer it until the
    /// document has been parsed; pdfium answers it at open, so the toolbar is
    /// never briefly wrong.
    ///
    /// Answers where the reader was, which is `None` for a document that has
    /// not been read before **and for a reader who has turned remembering
    /// off**. That switch is asked here rather than at the caller for the
    /// reason `bootstrap` gives in the app's `lib.rs`: a position handed over
    /// regardless, with the caller expected to ignore it, is two sides that
    /// eventually disagree about whether there was one.
    pub fn opened(&mut self, path: &str, declared: &str) -> Option<Anchor> {
        self.title = called(path, declared);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        self.file = path.to_string();
        let title = self.title.clone();
        let mut place = None;
        match library::touch(&self.dir, path, &title, now) {
            Ok(library) => {
                if let Some(entry) = library.files.iter().find(|entry| entry.path == path) {
                    self.marks = entry.marks.clone();
                    self.journal = entry.highlights.clone();
                    place = Some(Anchor {
                        page: entry.page.max(1) as usize,
                        offset: entry.offset,
                    });
                }
            }
            // A library that cannot be written is a reader who loses their
            // marks and their place at the end of the session, which is worth
            // a line at the bottom of the window and is not worth refusing to
            // open a document over.
            Err(refused) => self.complaint = Some(refused),
        }
        // What is open now is deliberately *not* written here. It is one
        // entry per window and a `Store` is one window's, so a store that
        // wrote the list would write a list of one and take the other windows
        // out of it — which is exactly what happened, and it took a session
        // of three windows down to whichever rendered last. Whoever makes a
        // window records what it shows: `Session::window` in the app, and the
        // harness for a reader that has no window at all.
        if !self.flag("remember_position") {
            return None;
        }
        place.filter(|at| at.page > 1 || at.offset > 0.0)
    }

    /// What to call this document: its own title where that is worth having,
    /// and the file's name where it is not.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The document was rewritten, and a rewritten document may call itself
    /// something else — a paper whose `\title{}` changed between two runs of
    /// LaTeX is the ordinary case. Answers whether the name moved.
    ///
    /// `retitle` is the app's own and writes only when there is a difference,
    /// which is what makes it safe to ask on every reload.
    pub fn renamed(&mut self, declared: &str) -> bool {
        if self.file.is_empty() {
            return false;
        }
        let name = std::path::Path::new(&self.file)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let now = if worth_calling(declared, &name) {
            declared.trim().to_string()
        } else {
            name
        };
        if now == self.title {
            return false;
        }
        self.title = now;
        let _ = library::retitle(&self.dir, &self.file, &self.title);
        true
    }

    /// Write down where the reader is, eventually.
    ///
    /// **Eventually is the whole of the design.** This is called on every
    /// change of the scroll offset, which is every wheel event; what it does
    /// is hand a place to `Scribe`, which keeps one per document and writes
    /// when the scrolling has stopped. Nothing here touches the disk, so the
    /// cost on the thread drawing the window is a channel send.
    pub fn remember(&self, at: Anchor) {
        if self.file.is_empty() || !self.flag("remember_position") {
            return;
        }
        let _ = Scribe::get().jobs.send(Job::Place {
            dir: self.dir.clone(),
            file: self.file.clone(),
            page: at.page as u32,
            offset: at.offset,
        });
    }

    /// The pages the reader has put a pin in, by page number, in page order.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    pub fn is_marked(&self, page: usize) -> bool {
        self.marks.iter().any(|mark| mark.page as usize == page)
    }

    /// Put a pin in a page, or take it out again — the same gesture doing the
    /// same thing, which is what makes the feature work without ids. Answers
    /// whether the page is marked now.
    ///
    /// `title` is what the row in the sidebar says. The app names a mark for
    /// the section it falls in, which it reads off the outline it has already
    /// walked; that is the caller's to work out, because the outline belongs
    /// to the document and this belongs to the disk.
    pub fn toggle_mark(&mut self, page: usize, title: &str) -> bool {
        if self.file.is_empty() {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        match library::toggle_mark(&self.dir, &self.file, page as u32, 0.0, title, now) {
            Ok((marked, marks)) => {
                self.marks = marks;
                marked
            }
            Err(_) => false,
        }
    }

    /* --------------------------------------------------- the markup journal */

    /// Markup this reader is keeping *beside* the document rather than in it.
    ///
    /// **The journal is never the authority**, which is the app's rule and the
    /// reason `library.rs` says so at the top of `Highlight`: a mark that is
    /// in the file is read out of the file, and this list holds only what a
    /// file cannot carry — markup on a document that could not be written.
    /// Those are the entries the app holds with `annotation_id: null`, and
    /// they are the only ones this reader ever puts here.
    ///
    /// The shape on disk is the app's exactly — the same `library.toml`, the
    /// same eight numbers a run, in the page's own PDF space counting from the
    /// bottom — because `library.rs` is the app's file mounted here rather
    /// than a copy, and a journal one of them writes is a journal the other
    /// reads.
    pub fn journal(&self) -> &[Highlight] {
        &self.journal
    }

    /// Keep one mark beside the document, because it could not go in.
    /// Answers its id, which is what takes it out again.
    pub fn keep_markup(&mut self, page: usize, quads: &[f64], color: &str, quote: &str) -> String {
        let id = format!(
            "{:x}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0),
            self.journal.len()
        );
        if self.file.is_empty() {
            return id;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        let highlight = Highlight {
            id: id.clone(),
            page: page as u32,
            quads: quads.to_vec(),
            color: color.to_string(),
            opacity: MARKUP_OPACITY,
            style: library::HighlightStyle::Highlight,
            quote: quote.to_string(),
            at: now,
            annotation_id: None,
        };
        match library::add_highlight(&self.dir, &self.file, highlight) {
            Ok(highlights) => self.journal = highlights,
            Err(refused) => self.complaint = Some(refused),
        }
        id
    }

    /// Replace the whole journal with what the file itself says, plus
    /// whatever the file could not carry.
    ///
    /// **The journal is a cache and a recovery log, never an authority** —
    /// `library.rs` says so above `Highlight` and this is what makes it true:
    /// everything that was here is discarded in favour of what was just read
    /// out of the document. What the caller keeps is its own business, and
    /// the only things it ever keeps are the two the file cannot say.
    pub fn set_journal(&mut self, highlights: Vec<Highlight>) {
        if self.file.is_empty() {
            return;
        }
        if library::set_highlights(&self.dir, &self.file, highlights.clone()).is_ok() {
            self.journal = highlights;
        }
    }

    /// One entry of the journal, as this reader writes them.
    pub fn markup_entry(
        page: usize,
        quads: Vec<f64>,
        color: &str,
        quote: &str,
        annotation: Option<String>,
    ) -> Highlight {
        Highlight {
            id: format!("{page}-{}-{}", color, quote.len()),
            page: page as u32,
            quads,
            color: color.to_string(),
            opacity: MARKUP_OPACITY,
            style: library::HighlightStyle::Highlight,
            quote: quote.to_string(),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs() as i64)
                .unwrap_or(0),
            annotation_id: annotation,
        }
    }

    /// Take one out of the journal by the id it was given.
    pub fn drop_markup(&mut self, id: &str) {
        if self.file.is_empty() {
            return;
        }
        if let Ok(highlights) = library::remove_highlight(&self.dir, &self.file, id) {
            self.journal = highlights;
        }
    }

    /// Change settings and write them down.
    ///
    /// A group rather than a key, because settings almost never move alone —
    /// a theme with the slot it fills, a zoom with its fit mode — and one call
    /// per key means two whole-file rewrites per change, each re-reading what
    /// the other has just done. That is `App.set` and `flushSettings` in
    /// `main.ts`, and here it is the signature.
    ///
    /// Anything refused is reported by `set_many` and dropped here: a caller
    /// in this crate passing an unknown key is a bug in this crate rather than
    /// something a reader can act on, and the settings in the same group that
    /// were fine have already landed.
    pub fn set(&mut self, entries: Vec<(String, Value)>) {
        for (key, value) in &entries {
            self.settings.insert(key.clone(), value.clone());
        }
        if let Err(refused) = settings::set_many(&self.dir, entries) {
            debug_assert!(false, "settings refused: {refused}");
            // And take the refused ones back out of memory, so that what is
            // held and what is on disk agree.
            self.settings = settings::load(&self.dir);
        }
    }

    pub fn text(&self, key: &str) -> String {
        self.settings
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub fn flag(&self, key: &str) -> bool {
        self.settings
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn number(&self, key: &str) -> f64 {
        self.settings
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hylopdf-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The reader gets the app's fourteen themes, from the app's own files,
    /// with the Hylo family first — which is what the `order` in each shipped
    /// file is for and the one thing a directory cannot say.
    #[test]
    fn the_shipped_themes_are_there_and_in_their_stated_order() {
        let dir = scratch("themes");
        let store = Store::at(&dir);
        assert_eq!(store.themes().len(), theme::BUILT_IN.len());
        assert!(store.themes().len() >= 14, "{}", store.themes().len());
        assert_eq!(store.themes()[0].id, theme::DEFAULT_LIGHT);
        assert_eq!(store.themes()[1].id, theme::DEFAULT_DARK);
        assert!(store.themes().iter().all(|theme| theme.built_in));
        assert!(store.complaint.is_none(), "{:?}", store.complaint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A theme is remembered by id and survives the run that chose it.
    #[test]
    fn the_theme_is_remembered() {
        let dir = scratch("wear");
        let dark = {
            let mut store = Store::at(&dir);
            let dark = store
                .themes()
                .iter()
                .position(|theme| theme.id == theme::DEFAULT_DARK)
                .expect("Hylo Dark ships");
            store.wear(dark);
            // Read off the file rather than restated here. A colour written
            // twice is a colour that drifts, which is the whole reason the
            // shipped set is a directory and not a list.
            assert_eq!(
                store.palette().background,
                palette::read_colour(&store.theme().background).expect("hex"),
            );
            dark
        };

        let reopened = Store::at(&dir);
        assert_eq!(reopened.theme_index(), dark);
        assert_eq!(reopened.theme().id, theme::DEFAULT_DARK);
        // And the slot it fills was written with it, so that following the
        // system later comes back to this rather than to the shipped default.
        assert_eq!(reopened.text("dark_theme"), theme::DEFAULT_DARK);
        assert_eq!(reopened.text("light_theme"), theme::DEFAULT_LIGHT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The theme in use is a name in a file, and a file can name a theme that
    /// is not there — because it was deleted, or because it was written by
    /// hand. Falling back to the default beats falling back to nothing.
    #[test]
    fn a_theme_that_is_gone_falls_back_rather_than_failing() {
        let dir = scratch("gone");
        let mut store = Store::at(&dir);
        store.set(vec![("theme".into(), json!("no-such-theme"))]);
        let reopened = Store::at(&dir);
        assert_eq!(reopened.theme().id, theme::DEFAULT_LIGHT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A theme somebody wrote by hand is listed after the shipped ones and is
    /// wearable, which is the whole argument for themes being files.
    #[test]
    fn a_hand_written_theme_can_be_worn() {
        let dir = scratch("hand");
        Store::at(&dir);
        std::fs::write(
            dir.join("themes/Mine.toml"),
            "name = \"Mine\"\ntext = \"#102030\"\nbackground = \"#fefefe\"\n",
        )
        .expect("write a theme");

        let mut store = Store::at(&dir);
        let mine = store
            .themes()
            .iter()
            .position(|theme| theme.name == "Mine")
            .expect("listed");
        assert!(
            mine >= theme::BUILT_IN.len(),
            "listed after the shipped set"
        );
        store.wear(mine);
        assert_eq!(store.palette().text, [0x10, 0x20, 0x30]);
        // It is light, so it filled the light slot.
        assert_eq!(store.text("light_theme"), "Mine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The judgement `worth_calling` makes, in the cases that made it exist.
    ///
    /// Every "no" here is a string a real producer writes into a real file,
    /// which is why the test is a list rather than an argument: the rule is
    /// not derivable, it is observed.
    #[test]
    fn a_name_worth_having_is_told_from_one_that_is_not() {
        for (title, file) in [
            ("The Structure of Scientific Revolutions", "kuhn.pdf"),
            ("Attention Is All You Need", "1706.03762v7.pdf"),
        ] {
            assert!(worth_calling(title, file), "{title:?}");
        }
        for (title, file) in [
            // Filled in by the program that made it, not by anybody.
            ("Microsoft Word - report.doc", "report.pdf"),
            ("untitled", "notes.pdf"),
            ("Untitled", "notes.pdf"),
            ("Document1", "notes.pdf"),
            // A title that is a file name is a file name, whatever file it
            // names.
            ("thesis.tex", "thesis.pdf"),
            ("scan_0001.PDF", "scan_0001.pdf"),
            // The file name over again, with and without its suffix.
            ("kuhn", "kuhn.pdf"),
            ("Kuhn.pdf", "kuhn.pdf"),
            // Too short to be a title, and long enough to be a page.
            ("Abc", "paper.pdf"),
            (&"x".repeat(201), "paper.pdf"),
        ] {
            assert!(!worth_calling(title, file), "{title:?}");
        }
        // The app rejects anything *beginning* with the word, not only the
        // word alone — `/^untitled\b/i` — so "Untitled Letters", which is a
        // real book, falls back to its file name. Carried across as written
        // rather than improved on: "Untitled document" and "Untitled 1" are
        // what producers actually emit, the cost of the rule is a file name
        // instead of a name, and a port that quietly disagrees with the app
        // about a judgement is the drift this experiment exists to avoid.
        assert!(!worth_calling("Untitled Letters", "letters.pdf"));
    }

    /// And one that names a colour the renderer cannot read says so, rather
    /// than silently rendering the fallback.
    #[test]
    fn an_unreadable_theme_is_complained_about() {
        let dir = scratch("unreadable");
        Store::at(&dir);
        std::fs::write(
            dir.join("themes/Wrong.toml"),
            "name = \"Wrong\"\ntext = \"steelblue\"\nbackground = \"#fff\"\n",
        )
        .expect("write a theme");

        let mut store = Store::at(&dir);
        let wrong = store
            .themes()
            .iter()
            .position(|theme| theme.name == "Wrong")
            .expect("listed");
        store.wear(wrong);
        let said = store.complaint.clone().expect("a notice");
        assert!(said.contains("Wrong") && said.contains("text"), "{said}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
