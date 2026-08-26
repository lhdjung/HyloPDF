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

static LOCK: Mutex<()> = Mutex::new(());

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
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    /// What was open when the app was last put down, so it can be picked up
    /// again. Empty when the reader closed the document themselves: that is
    /// them saying they have finished with it, and reopening it would be the
    /// app arguing.
    ///
    /// Kept here rather than in `settings.toml` because it is not a setting —
    /// it is the other half of what the first line of this file claims to
    /// know. Serialised before `file`, because a TOML table has to come after
    /// every plain key of its parent or it swallows them.
    #[serde(default)]
    pub open: String,
    #[serde(default, rename = "file")]
    pub files: Vec<Entry>,
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

/// Note what is open now, so the next launch can pick it up. `None` means
/// nothing is: the reader closed it, and that is a decision to respect.
pub fn set_open(dir: &Path, file: Option<&str>) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = load(dir);
    let wanted = file.unwrap_or_default();
    if library.open == wanted {
        return Ok(());
    }
    library.open = wanted.to_string();
    save(dir, &library)
}

/// Drop entries whose file is gone, so the recents list stays honest — and
/// with them a remembered document that is no longer there.
pub fn prune(library: &Library) -> Library {
    let open = if Path::new(&library.open).is_file() {
        library.open.clone()
    } else {
        String::new()
    };
    Library {
        open,
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
        set_open(&dir, Some(&doc)).expect("set open");

        // The one thing this file's shape can get wrong: `open` is a plain key
        // and `file` is an array of tables, and TOML puts every plain key of a
        // parent before its tables. Written the other way round, `open` lands
        // inside the last entry and comes back empty.
        let back = load(&dir);
        assert_eq!(back.open, doc);
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].page, 12);

        set_open(&dir, None).expect("clear open");
        assert_eq!(load(&dir).open, "");
        assert_eq!(load(&dir).files.len(), 1, "clearing it kept the history");
    }

    #[test]
    fn a_document_that_is_gone_is_not_reopened() {
        let dir = scratch("gone");
        let missing = dir.join("vanished.pdf").to_string_lossy().to_string();
        touch(&dir, &missing, "vanished.pdf", 100).expect("touch");
        set_open(&dir, Some(&missing)).expect("set open");

        let pruned = prune(&load(&dir));
        assert_eq!(pruned.open, "", "a launch would have failed on it every time");
        assert!(pruned.files.is_empty());
    }
}
