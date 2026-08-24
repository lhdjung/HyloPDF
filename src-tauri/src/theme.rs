//! Themes are plain TOML files, one per theme, so that a theme can be written
//! by hand (or by an LLM) without touching the app.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_write;

/// The themes that ship with HyloPDF. They are written into the user's theme
/// directory on first run so that they are visible, readable and copyable, but
/// the embedded copies stay authoritative if a file is missing or unreadable.
pub const BUILT_IN: &[(&str, &str)] = &[
    ("hylo-light", include_str!("../themes/hylo-light.toml")),
    ("hylo-dark", include_str!("../themes/hylo-dark.toml")),
    ("glamour", include_str!("../themes/glamour.toml")),
    ("dracula", include_str!("../themes/dracula.toml")),
    ("gruvbox", include_str!("../themes/gruvbox.toml")),
    ("sepia", include_str!("../themes/sepia.toml")),
    (
        "high-contrast",
        include_str!("../themes/high-contrast.toml"),
    ),
];

pub const DEFAULT_LIGHT: &str = "hylo-light";
pub const DEFAULT_DARK: &str = "hylo-dark";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Slug, taken from the file name. Not stored in the file itself.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub text: String,
    pub background: String,
    #[serde(default)]
    pub accent: Option<String>,
    /// The colour links are tinted with while the document is being recoloured.
    /// Absent means "use the accent".
    #[serde(default)]
    pub link: Option<String>,
    /// The colour behind selected text. Absent means "derive it from the
    /// accent", which is what every theme did before this was settable and is
    /// still the right answer for most of them.
    #[serde(default)]
    pub selection: Option<String>,
    /// The colour selected text itself is drawn in. Absent means "derive it
    /// from the colour behind it", which is what most themes want: the two
    /// only ever appear together, so one of them can always answer for both.
    #[serde(default)]
    pub selection_text: Option<String>,
    /// When false the document keeps its own colors and only the app chrome is
    /// themed. Used by Hylo Light.
    #[serde(default = "yes")]
    pub recolor: bool,
    /// Set by the loader, not by the file.
    #[serde(default)]
    pub built_in: bool,
}

fn yes() -> bool {
    true
}

/// What actually gets written to disk when a user saves a theme.
#[derive(Debug, Serialize)]
struct ThemeFile<'a> {
    name: &'a str,
    text: &'a str,
    background: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_text: &'a Option<String>,
    recolor: bool,
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        "theme".into()
    } else {
        slug
    }
}

fn parse(id: &str, source: &str, built_in: bool) -> Option<Theme> {
    let mut theme: Theme = toml::from_str(source).ok()?;
    theme.id = id.to_string();
    theme.built_in = built_in;
    Some(theme)
}

fn is_built_in(id: &str) -> bool {
    BUILT_IN.iter().any(|(name, _)| *name == id)
}

/// The banner every shipped theme file carries.
///
/// These files are rewritten on every run, so an edit made in place disappears
/// at the next launch. That is deliberate — the shipped set is the app's to
/// define, and a built-in that could drift would make "Hylo Dark" mean
/// something different on every machine. But a file that silently undoes your
/// work and says nothing about it is a trap, and the whole point of keeping
/// themes as plain text is that someone can open one and get somewhere. So the
/// file says what it is and where to put a copy.
const BANNER: &str = "\
# This file ships with HyloPDF and is rewritten every time the app starts.
# Edit it and your changes will be gone at the next launch.
#
# To make it yours: copy it to a new name in this folder — any name but the
# ones the shipped themes use — change the `name` inside, and it will appear in
# the theme list alongside these. The app does the same thing when you press
# \"Make a copy of this theme\".

";

fn shipped(source: &str) -> String {
    format!("{BANNER}{source}")
}

/// Write the shipped themes out on every run, so that a built-in whose colours
/// change in the app changes on disk too, rather than the first install of it
/// sitting there forever. Editing a built-in through the app already saves a
/// copy under an id of its own, so nothing a reader made is at stake; a
/// built-in file hand-edited in place is overwritten, deliberately — and the
/// banner on top of it says so, so nobody finds that out the hard way.
pub fn install_built_ins(dir: &Path) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    for (id, source) in BUILT_IN {
        let path = dir.join(format!("{id}.toml"));
        let wanted = shipped(source);
        // Only when it differs: no reason to touch a file that already says
        // exactly this.
        let on_disk = fs::read_to_string(&path).unwrap_or_default();
        if on_disk != wanted {
            // Through `atomic_write` like every other write in this crate. A
            // plain `fs::write` truncates and then fills, so there is a moment
            // when the file on disk is a shipped theme with no colours in it —
            // and this directory is watched, and read by anything the reader
            // has open beside the app. Rewriting seven files at every launch
            // is seven chances at that moment.
            let _ = atomic_write(&path, wanted.as_bytes());
        }
    }
}

/// All themes, built-ins first and in the order they are declared above, then
/// the user's own in alphabetical order.
pub fn load_all(dir: &Path) -> Vec<Theme> {
    let mut themes: Vec<Theme> = Vec::new();

    for (id, embedded) in BUILT_IN {
        let from_disk = fs::read_to_string(dir.join(format!("{id}.toml")))
            .ok()
            .and_then(|source| parse(id, &source, true));
        if let Some(theme) = from_disk.or_else(|| parse(id, embedded, true)) {
            themes.push(theme);
        }
    }

    let mut custom: Vec<Theme> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if is_built_in(id) {
                continue;
            }
            if let Some(theme) = fs::read_to_string(&path)
                .ok()
                .and_then(|source| parse(id, &source, false))
            {
                custom.push(theme);
            }
        }
    }
    custom.sort_by_key(|theme| theme.name.to_lowercase());
    themes.append(&mut custom);
    themes
}

pub fn save(dir: &Path, theme: &Theme) -> Result<Theme, String> {
    if theme.name.trim().is_empty() {
        return Err("A theme needs a name.".into());
    }
    let mut id = if theme.id.trim().is_empty() {
        // A new theme never lands on top of one that is already there.
        unique_id(dir, &slugify(&theme.name))
    } else {
        slugify(&theme.id)
    };
    if is_built_in(&id) {
        // Editing a built-in makes a copy rather than shadowing the original.
        id = unique_id(dir, &format!("{id}-custom"));
    }

    let stored = ThemeFile {
        name: theme.name.trim(),
        text: &theme.text,
        background: &theme.background,
        accent: &theme.accent,
        link: &theme.link,
        selection: &theme.selection,
        selection_text: &theme.selection_text,
        recolor: theme.recolor,
    };
    let body = toml::to_string_pretty(&stored).map_err(|e| e.to_string())?;

    atomic_write(&path_for(dir, &id), body.as_bytes())?;

    let mut saved = theme.clone();
    saved.id = id;
    saved.built_in = false;
    Ok(saved)
}

pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    if is_built_in(id) {
        return Err("Built-in themes cannot be deleted.".into());
    }
    let path = path_for(dir, &slugify(id));
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn path_for(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.toml"))
}

fn unique_id(dir: &Path, base: &str) -> String {
    if !path_for(dir, base).exists() && !is_built_in(base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !path_for(dir, &candidate).exists() && !is_built_in(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}
