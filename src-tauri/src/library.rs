//! Where you were in each document, and what you had open last.
//!
//! This is reading history rather than configuration, so it lives in its own
//! file and never mixes with settings.
//!
//! Like settings, every change here is a read-modify-write of the whole file,
//! and `remember` fires on every pause in a scroll — so it is the one most
//! likely to meet another. `LOCK` serialises them.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::atomic_write;

const LIMIT: usize = 24;

/// How many windows' worth of "what was open" is worth writing down. Nobody
/// reads eight documents at once; the cap is here so that a label left behind
/// by a closed window cannot make the list grow without end.
const OPEN_LIMIT: usize = 8;

static LOCK: Mutex<()> = Mutex::new(());

/// A place the reader put a pin in.
///
/// One per page, deliberately: a mark is "come back here", and a page is the
/// unit somebody means by "here". Keying them by page is what lets the whole
/// feature work without ids — marking a page that is already marked takes the
/// mark off again, which is the same gesture doing the same thing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mark {
    pub page: u32,
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default = "one")]
    pub page: u32,
    /// How far into that page the viewport sat, 0.0 to 1.0. Together with the
    /// page number this restores the exact scroll offset at any zoom level.
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub opened_at: i64,
    /// Last, and it has to be: these serialise as an array of tables, and TOML
    /// puts every plain key of a parent before its tables. Above `opened_at`,
    /// the entry's own keys would land inside the last mark.
    #[serde(default)]
    pub marks: Vec<Mark>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    /// What was open when the app was last put down, so it can be picked up
    /// again — one path per window, in the order the windows claimed one.
    /// Empty when the reader closed the documents themselves: that is them
    /// saying they have finished, and reopening would be the app arguing.
    ///
    /// A list rather than a path because there is more than one window now.
    /// It was a path, and a file written by that version still reads: see
    /// `one_or_many`. Only the list is ever written.
    ///
    /// Kept here rather than in `settings.toml` because it is not a setting —
    /// it is the other half of what the first line of this file claims to
    /// know. Serialised before `file`, because a TOML table has to come after
    /// every plain key of its parent or it swallows them — and an array of
    /// strings is a plain key, so this still holds.
    #[serde(default, deserialize_with = "one_or_many")]
    pub open: Vec<String>,
    #[serde(default, rename = "file")]
    pub files: Vec<Entry>,
}

/// `open` used to be one path and is now a list of them. A `library.toml`
/// written by the version before this one says `open = "..."`, and reading it
/// as a list of one is the whole of what compatibility costs here — the same
/// answer `theme.rs` gives to a renamed key, and for the same reason: a
/// downgrade that silently eats where you were is worse than a line of code.
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Shape::deserialize(deserializer)? {
        // The old file wrote an empty string for "nothing was open".
        Shape::One(one) if one.is_empty() => Vec::new(),
        Shape::One(one) => vec![one],
        Shape::Many(many) => many,
    })
}

fn path(dir: &Path) -> PathBuf {
    dir.join("library.toml")
}

pub fn load(dir: &Path) -> Library {
    fs::read_to_string(path(dir))
        .ok()
        .and_then(|body| toml::from_str(&body).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, library: &Library) -> Result<(), String> {
    let body = toml::to_string_pretty(library).map_err(|e| e.to_string())?;
    atomic_write(&path(dir), body.as_bytes())
}

/// Move a document to the front of the list, keeping the position already
/// recorded for it.
pub fn touch(dir: &Path, file: &str, title: &str, now: i64) -> Result<Library, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let mut entry = take(&mut library, file).unwrap_or_else(|| Entry {
        path: file.to_string(),
        page: 1,
        ..Entry::default()
    });
    if !title.is_empty() {
        entry.title = title.to_string();
    }
    entry.opened_at = now;
    library.files.insert(0, entry);
    library.files.truncate(LIMIT);
    save(dir, &library)?;
    Ok(library)
}

pub fn remember(dir: &Path, file: &str, page: u32, offset: f64) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let Some(entry) = library.files.iter_mut().find(|e| e.path == file) else {
        return Ok(());
    };
    if entry.page == page && (entry.offset - offset).abs() < 0.0005 {
        return Ok(());
    }
    entry.page = page;
    entry.offset = offset;
    save(dir, &library)
}

pub fn forget(dir: &Path, file: &str) -> Result<Library, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    take(&mut library, file);
    save(dir, &library)?;
    Ok(library)
}

/// Put a pin in a page, or take one out. The same call does both: a page that
/// is already marked is unmarked, which is what pressing the same key twice
/// means everywhere else.
///
/// Returns whether the page ended up marked, and the marks as they now stand.
pub fn toggle_mark(
    dir: &Path,
    file: &str,
    page: u32,
    offset: f64,
    title: &str,
    now: i64,
) -> Result<(bool, Vec<Mark>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let Some(entry) = library.files.iter_mut().find(|e| e.path == file) else {
        return Err("That document is not in the library.".into());
    };
    let marked = match entry.marks.iter().position(|mark| mark.page == page) {
        Some(at) => {
            entry.marks.remove(at);
            false
        }
        None => {
            entry.marks.push(Mark {
                page,
                offset,
                title: title.to_string(),
                at: now,
            });
            entry.marks.sort_by_key(|mark| mark.page);
            true
        }
    };
    let marks = entry.marks.clone();
    save(dir, &library)?;
    Ok((marked, marks))
}

/// Give a document the name it calls itself, once the frontend has read it out
/// of the file. A file named `2310.06825v3.pdf` says nothing on a shelf.
pub fn retitle(dir: &Path, file: &str, title: &str) -> Result<Library, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let Some(entry) = library.files.iter_mut().find(|e| e.path == file) else {
        return Ok(library);
    };
    if entry.title == title {
        return Ok(library);
    }
    entry.title = title.to_string();
    save(dir, &library)?;
    Ok(library)
}

/// Note what is open now, so the next launch can pick it up. An empty list
/// means nothing is: the reader closed what they had, and that is a decision
/// to respect.
///
/// The caller holds the live answer — one path per window — and passes the
/// whole of it, because the file is the record of what the app is holding
/// rather than a log of what each window did.
pub fn set_open(dir: &Path, files: &[String]) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let mut wanted: Vec<String> = Vec::new();
    for file in files {
        // The same document open in two windows is one document to reopen.
        if !file.is_empty() && !wanted.contains(file) {
            wanted.push(file.clone());
        }
    }
    wanted.truncate(OPEN_LIMIT);
    if library.open == wanted {
        return Ok(());
    }
    library.open = wanted;
    save(dir, &library)
}

/// Drop entries whose file is gone, so the recents list stays honest — and
/// with them a remembered document that is no longer there.
pub fn prune(library: &Library) -> Library {
    Library {
        open: library
            .open
            .iter()
            .filter(|file| Path::new(file).is_file())
            .cloned()
            .collect(),
        files: library
            .files
            .iter()
            .filter(|entry| Path::new(&entry.path).exists())
            .cloned()
            .collect(),
    }
}

fn take(library: &mut Library, file: &str) -> Option<Entry> {
    let index = library.files.iter().position(|e| e.path == file)?;
    Some(library.files.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hylopdf-lib-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn what_was_open_survives_and_shares_the_file_with_the_entries() {
        let dir = scratch("open");
        let doc = dir.join("paper.pdf");
        fs::write(&doc, b"%PDF-1.4\n%%EOF\n").expect("document");
        let doc = doc.to_string_lossy().to_string();

        touch(&dir, &doc, "paper.pdf", 100).expect("touch");
        remember(&dir, &doc, 12, 0.25).expect("remember");
        set_open(&dir, std::slice::from_ref(&doc)).expect("set open");

        // The one thing this file's shape can get wrong: `open` is a plain key
        // and `file` is an array of tables, and TOML puts every plain key of a
        // parent before its tables. Written the other way round, `open` lands
        // inside the last entry and comes back empty. An array of strings is
        // still a plain key, so widening it did not move that line.
        let back = load(&dir);
        assert_eq!(back.open, vec![doc.clone()]);
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].page, 12);

        set_open(&dir, &[]).expect("clear open");
        assert!(load(&dir).open.is_empty());
        assert_eq!(load(&dir).files.len(), 1, "clearing it kept the history");
    }

    #[test]
    fn two_windows_worth_of_open_documents_round_trip() {
        let dir = scratch("open-many");
        let one = dir.join("one.pdf");
        let two = dir.join("two.pdf");
        fs::write(&one, b"%PDF-1.4\n%%EOF\n").expect("document");
        fs::write(&two, b"%PDF-1.4\n%%EOF\n").expect("document");
        let one = one.to_string_lossy().to_string();
        let two = two.to_string_lossy().to_string();

        // The same document in two windows is one document to reopen, and the
        // order the windows claimed them in is the order they come back.
        set_open(&dir, &[one.clone(), two.clone(), one.clone()]).expect("set open");
        assert_eq!(load(&dir).open, vec![one.clone(), two.clone()]);

        // And the entries still read back, which is the plain-key-before-tables
        // rule again.
        touch(&dir, &one, "one.pdf", 100).expect("touch");
        assert_eq!(load(&dir).open, vec![one.clone(), two.clone()]);
        assert_eq!(load(&dir).files.len(), 1);
    }

    #[test]
    fn a_library_written_before_there_were_two_windows_still_reads() {
        let dir = scratch("open-old");
        let doc = dir.join("paper.pdf");
        fs::write(&doc, b"%PDF-1.4\n%%EOF\n").expect("document");
        let doc = doc.to_string_lossy().to_string();

        // What the version before this one wrote: `open` as a bare path.
        let body = format!("open = {doc:?}\n\n[[file]]\npath = {doc:?}\npage = 7\n");
        fs::write(path(&dir), body).expect("old library");

        let back = load(&dir);
        assert_eq!(back.open, vec![doc.clone()], "a path is a list of one");
        assert_eq!(back.files[0].page, 7);

        // And an empty string is what that version wrote for "nothing open".
        fs::write(path(&dir), "open = \"\"\n").expect("old library");
        assert!(load(&dir).open.is_empty());
    }

    #[test]
    fn a_mark_is_a_toggle_and_survives_the_file() {
        let dir = scratch("marks");
        let doc = dir.join("book.pdf");
        fs::write(&doc, b"%PDF-1.4\n%%EOF\n").expect("document");
        let doc = doc.to_string_lossy().to_string();
        touch(&dir, &doc, "book.pdf", 100).expect("touch");

        let (marked, marks) = toggle_mark(&dir, &doc, 12, 0.25, "Chapter 2", 100).expect("mark");
        assert!(marked);
        assert_eq!(marks.len(), 1);

        // Out of order in, in order out.
        toggle_mark(&dir, &doc, 4, 0.0, "Chapter 1", 101).expect("mark");
        let pages: Vec<u32> = load(&dir).files[0].marks.iter().map(|m| m.page).collect();
        assert_eq!(pages, vec![4, 12]);

        // The same page again takes the pin out rather than adding a second.
        let (marked, marks) = toggle_mark(&dir, &doc, 12, 0.25, "Chapter 2", 102).expect("unmark");
        assert!(!marked);
        assert_eq!(marks.len(), 1);

        // And the entry's own keys still read back: `marks` is an array of
        // tables, and TOML puts every plain key of a parent before its tables,
        // so anything below it in the struct would land inside the last mark.
        let back = load(&dir);
        assert_eq!(back.files[0].page, 1);
        assert_eq!(back.files[0].title, "book.pdf");
        assert_eq!(back.files[0].opened_at, 100);
        assert_eq!(back.files[0].marks[0].title, "Chapter 1");
    }

    #[test]
    fn a_document_that_is_gone_is_not_reopened() {
        let dir = scratch("gone");
        let missing = dir.join("vanished.pdf").to_string_lossy().to_string();
        touch(&dir, &missing, "vanished.pdf", 100).expect("touch");
        set_open(&dir, std::slice::from_ref(&missing)).expect("set open");

        let pruned = prune(&load(&dir));
        assert!(
            pruned.open.is_empty(),
            "a launch would have failed on it every time"
        );
        assert!(pruned.files.is_empty());
    }
}
