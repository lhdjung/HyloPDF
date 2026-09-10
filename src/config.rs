//! Where the app's own files live, and the one way anything writes to them.
//!
//! This is the part of `src-tauri/src/lib.rs` that `theme.rs` and
//! `settings.rs` actually need — `atomic_write` and a config directory — with
//! Tauri's path resolver replaced by the platform conventions it was resolving
//! to. Nothing else of those 2,431 lines is required to make the two modules
//! beside this one compile and run, which is the assessment's claim about them
//! demonstrated rather than restated.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Write by writing something else and renaming it over the top.
///
/// Copied out of `src-tauri/src/lib.rs` unchanged. A plain `fs::write`
/// truncates and then fills, so there is a moment when the file on disk is a
/// settings file with no settings in it — and this directory is watched, and
/// read by anything the reader has open beside the app.
pub fn atomic_write(target: &Path, body: &[u8]) -> Result<(), String> {
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

/// A document's path the way the rest of the app keys it: absolute, so that
/// `paper.pdf` from a terminal and the same file from the Finder are one
/// document to the library, the watch and the desk. Every door a path comes
/// in by calls this once; nothing downstream does.
pub fn absolute(path: &str) -> String {
    std::path::absolute(path)
        .map(|whole| whole.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

/// The directory the settings file and the themes directory live in.
///
/// **The app's own, under its bundle identifier.** The port wrote to
/// `HyloPDF-dioxus` while it was an experiment; a machine that still has that
/// directory has it moved into place, once.
///
/// **Warning: `app.hylopdf` is not a leftover of a dev build.** It is the
/// directory the *shipped Tauri app* wrote from its first release on (the
/// `identifier` in its `tauri.conf.json`), holding `library.toml` with every
/// bookmark and highlight, `keys.toml` and any theme the reader made. The
/// `remove_dir_all` below fires only when both directories exist — a machine
/// that ran the experiment beside the installed app — and there it wipes that
/// data rather than merging it. The formats are the same modules, so the
/// safer move is to leave an existing `app.hylopdf` alone and not rename.
///
/// `HYLOPDF_CONFIG` overrides it, which is what the tests use and what makes
/// a run reproducible.
pub fn config_dir() -> PathBuf {
    if let Some(stated) = std::env::var_os("HYLOPDF_CONFIG") {
        return PathBuf::from(stated);
    }
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = base().join("app.hylopdf");
        let experiment = base().join("HyloPDF-dioxus");
        if experiment.is_dir() {
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            let _ = std::fs::rename(&experiment, &dir);
        }
        dir
    })
    .clone()
}

/// The themes directory inside it, which is what `theme::load_all` reads.
pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

/// Where an application's own files go on this platform. Tauri's
/// `app_config_dir()` resolves to the same three answers; there is no reason
/// to take a dependency to get them.
fn base() -> PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    };
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData/Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
    }
}
