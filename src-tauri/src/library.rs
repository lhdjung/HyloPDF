//! Where you were in each document, and what you had open last.
//!
//! This is reading history rather than configuration, so it lives in its own
//! file and never mixes with settings.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const LIMIT: usize = 24;

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
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = path(dir).with_extension("toml.tmp");
    fs::write(&temp, body).map_err(|e| e.to_string())?;
    fs::rename(&temp, path(dir)).map_err(|e| e.to_string())
}

/// Move a document to the front of the list, keeping the position already
/// recorded for it.
pub fn touch(dir: &Path, file: &str, title: &str, now: i64) -> Result<Library, String> {
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
    let mut library = load(dir);
    take(&mut library, file);
    save(dir, &library)?;
    Ok(library)
}

/// Drop entries whose file is gone, so the recents list stays honest.
pub fn prune(library: &Library) -> Library {
    Library {
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
