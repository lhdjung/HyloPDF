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
//! *Writes are still off the main thread's critical path in the app and are
//! not here yet.* `set_many` is a read-modify-write of one small TOML file and
//! it happens on whichever thread asked; the app moved these to `async`
//! commands because `remember_position` fires on every pause in a scroll, and
//! nothing in this crate does that yet. When the library lands — Phase 3 item
//! 7 — this is where the thread goes, and the lock inside `settings.rs` is
//! already what makes that safe.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::palette::{self, Palette};
use crate::keys;
use crate::settings::{self, Settings};
use crate::theme;

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
        self.set(vec![
            ("theme".into(), json!(id)),
            (slot.into(), json!(id)),
        ]);
        self.complaint = self.unreadable();
        name
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
        assert!(mine >= theme::BUILT_IN.len(), "listed after the shipped set");
        store.wear(mine);
        assert_eq!(store.palette().text, [0x10, 0x20, 0x30]);
        // It is light, so it filled the light slot.
        assert_eq!(store.text("light_theme"), "Mine");
        let _ = std::fs::remove_dir_all(&dir);
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
