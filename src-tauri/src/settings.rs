//! Settings live in one flat, hand-editable TOML table.
//!
//! Two properties matter here and shape the whole module:
//!
//! * every setting survives a restart, and
//! * settings are independent — writing one never rewrites another.
//!
//! So the file is a map of scalars, and a write is a read-modify-write of a
//! single key. Keys the running version does not know about are carried
//! through untouched instead of being dropped.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub type Settings = BTreeMap<String, Value>;

/// Every setting HyloPDF knows, with its default. This list is also the
/// whitelist: a write to an unknown key is refused, so a typo in a command
/// cannot quietly create a setting that nothing reads.
pub fn defaults() -> Settings {
    let mut s = Settings::new();
    // Reading
    s.insert("theme".into(), json!(super::theme::DEFAULT_LIGHT));
    s.insert("light_theme".into(), json!(super::theme::DEFAULT_LIGHT));
    s.insert("dark_theme".into(), json!(super::theme::DEFAULT_DARK));
    // Continuous scrolling is the default and stays the default: the UI only
    // changes this when the reader picks another mode by hand, and no keyboard
    // shortcut is bound to it.
    s.insert("scroll_mode".into(), json!("continuous"));
    s.insert("fit_mode".into(), json!("width"));
    s.insert("zoom".into(), json!(1.0));
    s.insert("page_gap".into(), json!(16));
    s.insert("recolor_images".into(), json!(true));
    s.insert("remember_position".into(), json!(true));
    // Chrome
    s.insert("show_toolbar".into(), json!(true));
    s.insert("show_sidebar".into(), json!(false));
    s.insert("sidebar_width".into(), json!(232));
    s.insert("fullscreen".into(), json!(false));
    // Window
    s.insert("window_width".into(), json!(1280.0));
    s.insert("window_height".into(), json!(860.0));
    s.insert("window_x".into(), Value::Null);
    s.insert("window_y".into(), Value::Null);
    s.insert("window_maximized".into(), json!(true));
    s
}

fn path(dir: &Path) -> PathBuf {
    dir.join("settings.toml")
}

fn from_toml(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        other => json!(other.to_string()),
    }
}

fn to_toml(value: &Value) -> Option<toml::Value> {
    match value {
        Value::String(s) => Some(toml::Value::String(s.clone())),
        Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(toml::Value::Integer(i))
            } else {
                n.as_f64().map(toml::Value::Float)
            }
        }
        _ => None,
    }
}

/// Stored values layered over the defaults, so a half-written or partial file
/// still yields a complete, usable set.
pub fn load(dir: &Path) -> Settings {
    let mut settings = defaults();
    let Ok(body) = fs::read_to_string(path(dir)) else {
        return settings;
    };
    let Ok(table) = body.parse::<toml::Table>() else {
        return settings;
    };
    for (key, value) in table {
        settings.insert(key, from_toml(value));
    }
    settings
}

fn write(dir: &Path, settings: &Settings) -> Result<(), String> {
    let mut table = toml::Table::new();
    for (key, value) in settings {
        if let Some(scalar) = to_toml(value) {
            table.insert(key.clone(), scalar);
        }
    }
    let body = format!(
        "# HyloPDF settings. Edited by the app, but yours to edit too.\n\n{}",
        toml::to_string_pretty(&table).map_err(|e| e.to_string())?
    );

    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Write beside the target and rename, so a crash mid-write cannot leave a
    // truncated settings file behind.
    let temp = path(dir).with_extension("toml.tmp");
    fs::write(&temp, body).map_err(|e| e.to_string())?;
    fs::rename(&temp, path(dir)).map_err(|e| e.to_string())
}

/// Whether a value is the kind of thing a setting holds, judged against that
/// setting's default.
///
/// The window's position is the one setting with no sensible default, so it
/// defaults to null and its real shape cannot be read off that. Null is legal
/// there and nowhere else: it is how "the window has never been placed" is
/// written down, and anything else offered for it still has to be a number.
fn same_shape(default: &Value, value: &Value) -> bool {
    match (default, value) {
        (Value::Null, Value::Null) => true,
        (Value::Null, other) => other.is_number(),
        (Value::Bool(_), Value::Bool(_)) => true,
        (Value::Number(_), Value::Number(_)) => true,
        (Value::String(_), Value::String(_)) => true,
        _ => false,
    }
}

/// Change exactly one setting. Everything else in the file is read back and
/// written out as it was, including keys this version does not know.
pub fn set(dir: &Path, key: &str, value: Value) -> Result<Settings, String> {
    let known = defaults();
    let Some(default) = known.get(key) else {
        return Err(format!("Unknown setting: {key}"));
    };
    if !same_shape(default, &value) {
        return Err(format!("Setting {key} does not take that kind of value."));
    }

    let mut settings = load(dir);
    settings.insert(key.to_string(), value);
    write(dir, &settings)?;
    Ok(settings)
}

/// Several settings at once, for the one case where that is honest: the window
/// geometry saved on quit, which is a single observation of a single window.
pub fn set_many(dir: &Path, entries: Vec<(String, Value)>) -> Result<Settings, String> {
    let known = defaults();
    let mut settings = load(dir);
    for (key, value) in entries {
        let Some(default) = known.get(&key) else {
            continue;
        };
        if same_shape(default, &value) {
            settings.insert(key, value);
        }
    }
    write(dir, &settings)?;
    Ok(settings)
}
