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
//!
//! A read-modify-write is only atomic if nothing else is doing one at the same
//! time. These commands run off the main thread, and the interface writes
//! settings in pairs — a theme and the light or dark theme it stands for, a
//! zoom and the fit mode that goes with it — so two writes landing together is
//! the normal case rather than the unlucky one. `LOCK` serialises them, and
//! the temp file each write goes through is named for the writer, so two of
//! them can never be the same file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::atomic_write;

/// Held across read-modify-write, so one write never reads a file another is
/// half way through replacing.
static LOCK: Mutex<()> = Mutex::new(());

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
    // On, and on by default: an app that stays white while the machine around
    // it has gone dark at sunset is the one thing every reader now notices.
    // It is a switch rather than a mode — `theme` is still what is in use, and
    // this only says who gets to change it when the system changes its mind.
    // Choosing a theme that disagrees with the system turns it off, because
    // that choice is the reader saying they would rather decide themselves.
    s.insert("follow_system_theme".into(), json!(true));
    // Continuous scrolling is the default and stays the default: the UI only
    // changes this when the reader picks another mode by hand, and no keyboard
    // shortcut is bound to it.
    s.insert("scroll_mode".into(), json!("continuous"));
    // One page across by default. Two side by side is worth a great deal on a
    // wide screen and is wrong on a narrow one, and only the reader knows
    // which they have.
    s.insert("spread_mode".into(), json!("single"));
    s.insert("fit_mode".into(), json!("width"));
    s.insert("zoom".into(), json!(1.0));
    s.insert("page_gap".into(), json!(16));
    // Off, and off deliberately. Taking the margins away is the right answer
    // for a scanned book and for anything typeset with an inch of white down
    // each side, and it is a change to what a page looks like — so it is the
    // reader's to ask for rather than something they find has happened.
    s.insert("trim_margins".into(), json!(false));
    // On by default, and it was not always: recolouring used to flatten a page
    // onto two colours, which is the one thing it can do that makes a page
    // harder to read rather than easier — a photograph goes to mud, and a
    // chart whose series differ only in hue loses the difference. It keeps the
    // colours now, so a picture belongs in it. Off is for wanting a photograph
    // exactly as it was printed, and costs a figure drawn half in pictures and
    // half in lines the agreement between its halves.
    s.insert("recolor_images".into(), json!(true));
    s.insert("remember_position".into(), json!(true));
    // The other half of remembering where you stopped: come back to *what* you
    // were reading, not just to where in it. Only what was open when the app
    // went down — a document the reader closed themselves is one they have
    // finished with, and reopening it would be the app arguing.
    s.insert("reopen_last_document".into(), json!(true));
    // On by default: a page count shown briefly while scrolling is how a
    // reader with the toolbar hidden still knows where they are.
    s.insert("show_page_pill".into(), json!(true));
    // Search. Where a match is looked for is a way of reading, not a property
    // of a document, so these outlive the find bar they are set from and the
    // session they were set in.
    s.insert("search_highlight_all".into(), json!(true));
    s.insert("search_match_case".into(), json!(false));
    s.insert("search_whole_words".into(), json!(false));
    // Chrome
    s.insert("show_toolbar".into(), json!(true));
    s.insert("show_sidebar".into(), json!(false));
    // Wide enough for the three tabs the panel can carry — Contents, Pages
    // and, while a search is up, Results — without a word being shortened.
    s.insert("sidebar_width".into(), json!(252));
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
///
/// Every value is checked on the way in, against the same `same_shape` a write
/// is checked against. It was not, and that was the one hole in a module whose
/// whole subject is that a setting is a known key holding a known kind of
/// thing: `set_many` refused anything else, and then `load` layered whatever
/// the file happened to say straight over the defaults and handed it to the
/// frontend. This file's own header invites hand-editing, so `zoom = "big"`
/// is a thing a person can write — and it arrived in `viewer.setFit` typed as
/// a number, `scroll_mode = "sideways"` arrived at a switch with no arm for
/// it, and nothing anywhere had said no.
///
/// A value of the wrong shape is dropped rather than repaired, so the default
/// stands and the app is usable. Unknown keys are still carried through
/// untouched — those belong to a version that is not this one, and dropping
/// them is how a downgrade eats your settings.
pub fn load(dir: &Path) -> Settings {
    let known = defaults();
    let mut settings = known.clone();
    let Ok(body) = fs::read_to_string(path(dir)) else {
        return settings;
    };
    let Ok(table) = body.parse::<toml::Table>() else {
        return settings;
    };
    for (key, value) in table {
        let value = from_toml(value);
        match known.get(&key) {
            Some(default) if !same_shape(default, &value) => continue,
            _ => settings.insert(key, value),
        };
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

    atomic_write(&path(dir), body.as_bytes())
}

/// Whether a value is the kind of thing a setting holds, judged against that
/// setting's default.
///
/// The window's position is the one setting with no sensible default, so it
/// defaults to null and its real shape cannot be read off that. Null is legal
/// there and nowhere else: it is how "the window has never been placed" is
/// written down, and anything else offered for it still has to be a number.
///
/// A whole number is its own shape. `page_gap` and `sidebar_width` are
/// distances in pixels and are integers everywhere they are used; letting 16.5
/// through because "a number is a number" is the sort of thing that surfaces
/// later as a layout that is half a pixel out and nobody knowing why. A
/// setting whose default is a float — `zoom` — takes either, because a whole
/// number is a perfectly good zoom.
fn same_shape(default: &Value, value: &Value) -> bool {
    match (default, value) {
        (Value::Null, Value::Null) => true,
        (Value::Null, other) => other.is_number(),
        (Value::Bool(_), Value::Bool(_)) => true,
        (Value::Number(d), Value::Number(v)) if d.is_i64() || d.is_u64() => {
            v.is_i64() || v.is_u64()
        }
        (Value::Number(_), Value::Number(_)) => true,
        (Value::String(_), Value::String(_)) => true,
        _ => false,
    }
}

/// Several settings at once. Two cases want this: the window geometry saved on
/// quit, which is one observation of one window, and any group the interface
/// changes together — a theme and the light or dark slot it fills, a zoom and
/// the fit mode that goes with it. Writing those as one file write is both
/// cheaper and the only way they can never be seen half-applied.
///
/// Unknown keys and wrong-shaped values are reported rather than silently
/// dropped, so a typo in a command still surfaces; the rest are still written.
pub fn set_many(dir: &Path, entries: Vec<(String, Value)>) -> Result<Settings, String> {
    let known = defaults();
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = load(dir);
    let mut refused: Vec<String> = Vec::new();
    for (key, value) in entries {
        match known.get(&key) {
            Some(default) if same_shape(default, &value) => {
                settings.insert(key, value);
            }
            Some(_) => refused.push(format!("{key} does not take that kind of value")),
            None => refused.push(format!("unknown setting {key}")),
        }
    }
    write(dir, &settings)?;
    if refused.is_empty() {
        Ok(settings)
    } else {
        Err(refused.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape `set_settings` receives.
    ///
    /// The frontend sends `entries` as an array of two-element arrays, and
    /// this is the one thing about that command a type checker on either side
    /// cannot see: TypeScript says it sent tuples, serde says it wants tuples,
    /// and nothing checks that those two agree until a reader changes a
    /// setting and it silently does not stick.
    #[test]
    fn entries_deserialize_from_pairs() {
        let wire = r#"[["theme", "dracula"], ["zoom", 1.5], ["show_toolbar", false]]"#;
        let entries: Vec<(String, Value)> = serde_json::from_str(wire).expect("entries");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], ("theme".to_string(), json!("dracula")));
        assert_eq!(entries[1], ("zoom".to_string(), json!(1.5)));
        assert_eq!(entries[2], ("show_toolbar".to_string(), json!(false)));
    }

    #[test]
    fn a_group_is_written_together_and_read_back() {
        let dir = std::env::temp_dir().join(format!("hylopdf-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let after = set_many(
            &dir,
            vec![
                ("theme".into(), json!("dracula")),
                ("dark_theme".into(), json!("dracula")),
            ],
        )
        .expect("write");
        assert_eq!(after.get("theme"), Some(&json!("dracula")));

        // Both halves survive a reload, which is the point of writing them as
        // one file write rather than two.
        let reloaded = load(&dir);
        assert_eq!(reloaded.get("theme"), Some(&json!("dracula")));
        assert_eq!(reloaded.get("dark_theme"), Some(&json!("dracula")));
        // Untouched settings keep their defaults.
        assert_eq!(reloaded.get("scroll_mode"), Some(&json!("continuous")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_key_is_reported_and_the_rest_still_land() {
        let dir = std::env::temp_dir().join(format!("hylopdf-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let refused = set_many(
            &dir,
            vec![
                ("page_gap".into(), json!(20)),
                ("nonsense".into(), json!(1)),
                ("show_toolbar".into(), json!("not a boolean")),
            ],
        );
        let message = refused.expect_err("should report what it refused");
        assert!(message.contains("nonsense"), "{message}");
        assert!(message.contains("show_toolbar"), "{message}");

        let reloaded = load(&dir);
        assert_eq!(reloaded.get("page_gap"), Some(&json!(20)));
        assert_eq!(reloaded.get("show_toolbar"), Some(&json!(true)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hand-edited file is the one place a setting can be the wrong kind of
    /// thing, and the file's own header invites hand-editing. What reaches the
    /// frontend has to be what the frontend's types say it is.
    #[test]
    fn a_hand_edited_file_cannot_change_what_a_setting_is() {
        let dir = std::env::temp_dir().join(format!("hylopdf-shape-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.toml"),
            r#"
zoom = "big"
scroll_mode = 7
page_gap = 20
sidebar_width = 12.5
something_a_later_version_added = "kept"
"#,
        )
        .unwrap();

        let loaded = load(&dir);
        // Wrong kind: the default stands.
        assert_eq!(loaded.get("zoom"), Some(&json!(1.0)));
        assert_eq!(loaded.get("scroll_mode"), Some(&json!("continuous")));
        // A distance in pixels is a whole number of them, so the default
        // stands — taken from the table rather than written out again here,
        // which is a number that has moved once already.
        assert_eq!(loaded.get("sidebar_width"), defaults().get("sidebar_width"));
        // Right kind: taken.
        assert_eq!(loaded.get("page_gap"), Some(&json!(20)));
        // Not ours to judge, and dropping it is how a downgrade eats settings.
        assert_eq!(
            loaded.get("something_a_later_version_added"),
            Some(&json!("kept"))
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Concurrent writers must not lose each other's work. This is the failure
    /// the lock exists for, and it only became reachable when the commands
    /// stopped running on the main thread.
    #[test]
    fn concurrent_writes_do_not_lose_settings() {
        let dir = std::env::temp_dir().join(format!("hylopdf-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let keys = ["page_gap", "sidebar_width", "window_width", "window_height"];
        std::thread::scope(|scope| {
            for (n, key) in keys.iter().enumerate() {
                let dir = dir.clone();
                scope.spawn(move || {
                    for _ in 0..20 {
                        set_many(&dir, vec![((*key).into(), json!(100 + n as i64))]).unwrap();
                    }
                });
            }
        });

        let reloaded = load(&dir);
        for (n, key) in keys.iter().enumerate() {
            assert_eq!(
                reloaded.get(*key),
                Some(&json!(100 + n as i64)),
                "{key} was lost by another writer"
            );
        }
        // And nothing was left staged.
        let staged: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(staged.is_empty(), "temp files left behind: {staged:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
