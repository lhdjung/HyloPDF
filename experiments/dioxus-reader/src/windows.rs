//! Which window is showing what, where the next one goes, and what a window
//! going means.
//!
//! This is `OpenDocuments`, `Placements`, `Exiting`, `placement()` and the
//! decision half of `hand_over()` out of the app's `lib.rs`, with every
//! mention of a webview taken out. What is left is bookkeeping and three
//! rules, and none of it knows what a window is — which is the point, because
//! **in the app none of this can be tested at all**. `AGENTS.md` says so in as
//! many words: "None of this can be tested in the harness, which has no Rust
//! behind it and no windows", and what stands in for a test there is a list of
//! things that were checked by hand in a running app. Here the rules are a
//! module with no window in it and the tests are at the bottom of the file.
//!
//! The three rules, in the order they were learned:
//!
//! *Nothing is ever displaced.* A document handed over by the system goes to
//! a window with nothing in it, or to a window made for it — never over the
//! top of what somebody is reading. That was the single worst thing the app
//! did to anybody before there was more than one window.
//!
//! *A document already open is brought to the front rather than opened
//! again.* A second copy of a paper beside the first is the one thing
//! double-clicking a file cannot mean.
//!
//! *A window going means two things, and only [`Desk::leaving`] tells them
//! apart.* Closed by the reader it is a document they have finished with;
//! closed because the app is going it means only that it was open at the end,
//! which is the whole of what the next launch puts back.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The window the app launches with, and the only one with a name of its own.
///
/// Every other is `reader-N`. The names matter for one reason and it is
/// `watch.rs`: a document is followed per window and reported by
/// `emit_to(label, …)`, so a label is how a recompiled paper finds the window
/// reading it. See [`crate::emit::Exchange`].
pub const MAIN: &str = "main";

/// How far a new window is stepped down and across from the one in front of
/// it, so that two windows are two windows rather than one with a stack
/// behind it. The app's number.
pub const CASCADE: f64 = 28.0;

/// What to do with a document somebody has handed us.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handover {
    /// It is already open in this window. Bring that window forward — the
    /// reader is asking to look at it, not for a second copy of it.
    Front(String),
    /// This window has nothing in it. Give it the document.
    Fill(String),
    /// Every window is busy. Make one.
    Spawn,
}

/// What each window is showing, which window is in front, and whether the app
/// is on its way out.
///
/// Shared rather than owned: the shell has it, and so does whatever answers
/// the door — the Dock menu item, the single-instance listener — both of
/// which are on threads of their own.
#[derive(Clone, Default)]
pub struct Desk(Arc<Inner>);

#[derive(Default)]
struct Inner {
    /// Window label to the document it has open, in the order the windows
    /// were made. The order is what `library.toml` records, so a session
    /// comes back in the order it was left.
    showing: Mutex<Vec<(String, String)>>,
    /// Which window has the keyboard, as winit last reported it.
    front: Mutex<Option<String>>,
    exiting: AtomicBool,
    next: AtomicU64,
}

impl Desk {
    pub fn new() -> Desk {
        Desk::default()
    }

    /// The name for the next window: `main` for the first, then `reader-1`.
    pub fn name(&self) -> String {
        match self.0.next.fetch_add(1, Ordering::Relaxed) {
            0 => MAIN.to_string(),
            n => format!("reader-{n}"),
        }
    }

    /// Record what a window is showing, and report the whole list as it now
    /// stands — which is what `library.toml` wants.
    pub fn set(&self, window: &str, path: Option<&str>) -> Vec<String> {
        let mut held = self.0.showing.lock().unwrap_or_else(|e| e.into_inner());
        match path {
            Some(path) => match held.iter_mut().find(|(label, _)| label == window) {
                Some(slot) => slot.1 = path.to_string(),
                None => held.push((window.to_string(), path.to_string())),
            },
            None => held.retain(|(label, _)| label != window),
        }
        held.iter().map(|(_, path)| path.clone()).collect()
    }

    /// What every window is showing, in the order they were made.
    pub fn open(&self) -> Vec<String> {
        let held = self.0.showing.lock().unwrap_or_else(|e| e.into_inner());
        held.iter().map(|(_, path)| path.clone()).collect()
    }

    /// What one window is showing.
    pub fn document_of(&self, window: &str) -> Option<String> {
        let held = self.0.showing.lock().unwrap_or_else(|e| e.into_inner());
        held.iter()
            .find(|(label, _)| label == window)
            .map(|(_, path)| path.clone())
    }

    pub fn count(&self) -> usize {
        self.0
            .showing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// Whichever window winit last said had the keyboard.
    pub fn focused(&self, window: Option<&str>) {
        let mut front = self.0.front.lock().unwrap_or_else(|e| e.into_inner());
        *front = window.map(str::to_string);
    }

    pub fn front(&self) -> Option<String> {
        self.0
            .front
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Where a document handed to us by the system should go.
    ///
    /// **The middle arm is unreachable in this reader, and that is a finding
    /// rather than an oversight.** `Fill` is for a window with nothing in it,
    /// and there is no such thing here: the app has a start screen, so ⌘N
    /// gives an empty window and a double-clicked file fills it; this reader
    /// has no start screen — see item 7, "there is nowhere to show a
    /// recently-read list in a reader that always has a document open" — so a
    /// window is made *for* a document and never before one. The arm is kept
    /// because the rule is right and the day a window can be empty is the day
    /// it is needed, and because a window whose document failed to open is
    /// exactly that case arriving by the back door.
    pub fn hand_over(&self, path: &str) -> Handover {
        let held = self.0.showing.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((label, _)) = held.iter().find(|(_, open)| open == path) {
            return Handover::Front(label.clone());
        }
        drop(held);
        match self.idle() {
            Some(label) => Handover::Fill(label),
            None => Handover::Spawn,
        }
    }

    /// A window with nothing in it, the one with the keyboard first.
    fn idle(&self) -> Option<String> {
        let held = self.0.showing.lock().unwrap_or_else(|e| e.into_inner());
        let taken = |label: &str| held.iter().any(|(known, _)| known == label);
        if let Some(front) = self.front() {
            if !taken(&front) {
                return Some(front);
            }
        }
        None
    }

    /// The app is going. Raised before the first window closes, by everything
    /// that ends the run.
    pub fn leaving(&self) {
        self.0.exiting.store(true, Ordering::Relaxed);
    }

    pub fn is_leaving(&self) -> bool {
        self.0.exiting.load(Ordering::Relaxed)
    }

    /// A window has gone. Answers the list to write down, or `None` for
    /// "write nothing".
    ///
    /// Two cases write nothing, and they are the whole of the rule. The app
    /// is quitting, in which case every window is about to go and the list as
    /// it stands is what the next launch is meant to put back. Or this was
    /// the last window — which on every platform ends the app and is how most
    /// people quit it, and where nothing separates "I have finished with
    /// this" from "goodbye". So a close can forget any window but the last,
    /// and quitting with one document open still comes back to it.
    pub fn closing(&self, window: &str) -> Option<Vec<String>> {
        if self.is_leaving() {
            return None;
        }
        let remaining = self.set(window, None);
        if remaining.is_empty() {
            return None;
        }
        Some(remaining)
    }
}

/// Where to put a new window: one step down and across from the window in
/// front of it, and on again while that spot is taken.
///
/// Off the *front* window rather than off the remembered position, which is
/// the app's own hard-won correction: restoring three windows makes them in
/// one burst, so all three cascaded off the same number and landed within a
/// few pixels of each other — a stack that looks exactly like one window,
/// which is the failure this whole feature exists to avoid.
///
/// `None` means there is nothing to cascade from, and the window is centred.
///
/// **What the app needs here and this does not is `Placements`.** There, a
/// window is built, then *shown*, and showing it on macOS moves it onto the
/// launch window's frame — so the spot has to be remembered and applied again
/// after the show, and a window made a moment ago has not been put in its
/// place yet, which is why the pending spots are counted as taken. Here a
/// window is made and positioned in one function with nothing on screen in
/// between, so the windows that exist are the whole of what is taken.
pub fn cascade(
    front: Option<(f64, f64)>,
    taken: &[(f64, f64)],
    remembered: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let base = front.or(remembered)?;
    let mut spot = (base.0 + CASCADE, base.1 + CASCADE);
    // Bounded: a screen this far down and across is a screen nobody has, and
    // an unbounded walk here would be an unbounded walk off the display.
    for _ in 0..16 {
        let clash = taken
            .iter()
            .any(|(x, y)| (x - spot.0).abs() < 2.0 && (y - spot.1).abs() < 2.0);
        if !clash {
            break;
        }
        spot = (spot.0 + CASCADE, spot.1 + CASCADE);
    }
    Some(spot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_are_named_the_way_the_app_names_them() {
        let desk = Desk::new();
        assert_eq!(desk.name(), "main");
        assert_eq!(desk.name(), "reader-1");
        assert_eq!(desk.name(), "reader-2");
    }

    #[test]
    fn a_document_already_open_comes_to_the_front() {
        let desk = Desk::new();
        desk.set("main", Some("/papers/one.pdf"));
        desk.set("reader-1", Some("/papers/two.pdf"));
        assert_eq!(
            desk.hand_over("/papers/two.pdf"),
            Handover::Front("reader-1".into())
        );
    }

    #[test]
    fn a_document_nothing_is_showing_gets_a_window_of_its_own() {
        let desk = Desk::new();
        desk.set("main", Some("/papers/one.pdf"));
        assert_eq!(desk.hand_over("/papers/three.pdf"), Handover::Spawn);
    }

    /// The arm this reader cannot reach on its own — see [`Desk::hand_over`].
    /// A window that exists and is showing nothing is what a failed open
    /// leaves behind, and the rule for it is the app's.
    #[test]
    fn a_window_with_nothing_in_it_is_filled_rather_than_displaced() {
        let desk = Desk::new();
        desk.set("main", Some("/papers/one.pdf"));
        desk.focused(Some("reader-1"));
        assert_eq!(
            desk.hand_over("/papers/two.pdf"),
            Handover::Fill("reader-1".into())
        );
    }

    #[test]
    fn nothing_is_ever_displaced() {
        let desk = Desk::new();
        desk.set("main", Some("/papers/one.pdf"));
        desk.focused(Some("main"));
        assert_eq!(desk.hand_over("/papers/two.pdf"), Handover::Spawn);
    }

    #[test]
    fn what_is_open_is_in_the_order_the_windows_were_made() {
        let desk = Desk::new();
        desk.set("main", Some("/a.pdf"));
        desk.set("reader-1", Some("/b.pdf"));
        desk.set("reader-2", Some("/c.pdf"));
        assert_eq!(desk.open(), vec!["/a.pdf", "/b.pdf", "/c.pdf"]);
        // A window that opens something else replaces its own entry rather
        // than adding one.
        desk.set("reader-1", Some("/d.pdf"));
        assert_eq!(desk.open(), vec!["/a.pdf", "/d.pdf", "/c.pdf"]);
    }

    #[test]
    fn a_window_the_reader_closed_is_forgotten() {
        let desk = Desk::new();
        desk.set("main", Some("/a.pdf"));
        desk.set("reader-1", Some("/b.pdf"));
        assert_eq!(desk.closing("reader-1"), Some(vec!["/a.pdf".to_string()]));
    }

    /// The case no flag can reach. Closing the last window ends the app on
    /// every platform, and there is nothing there to separate "finished with
    /// it" from "goodbye" — so the list is left alone and the next launch
    /// comes back to it.
    #[test]
    fn closing_the_last_window_writes_nothing() {
        let desk = Desk::new();
        desk.set("main", Some("/a.pdf"));
        assert_eq!(desk.closing("main"), None);
    }

    #[test]
    fn quitting_forgets_nothing() {
        let desk = Desk::new();
        desk.set("main", Some("/a.pdf"));
        desk.set("reader-1", Some("/b.pdf"));
        desk.leaving();
        assert_eq!(desk.closing("reader-1"), None);
        assert_eq!(desk.closing("main"), None);
        // And what was open at the end is still the whole of it.
        assert_eq!(desk.open(), vec!["/a.pdf", "/b.pdf"]);
    }

    /// Three windows closed one at a time come back as the third alone, which
    /// is what a reader who closed them means and as close to it as the
    /// platforms allow.
    #[test]
    fn three_windows_closed_one_at_a_time_come_back_as_one() {
        let desk = Desk::new();
        desk.set("main", Some("/a.pdf"));
        desk.set("reader-1", Some("/b.pdf"));
        desk.set("reader-2", Some("/c.pdf"));
        assert_eq!(
            desk.closing("main"),
            Some(vec!["/b.pdf".into(), "/c.pdf".into()])
        );
        assert_eq!(desk.closing("reader-1"), Some(vec!["/c.pdf".to_string()]));
        assert_eq!(desk.closing("reader-2"), None);
    }

    #[test]
    fn a_new_window_steps_off_the_one_in_front() {
        assert_eq!(
            cascade(Some((100.0, 100.0)), &[], None),
            Some((128.0, 128.0))
        );
    }

    #[test]
    fn and_on_again_while_the_spot_is_taken() {
        let taken = [(100.0, 100.0), (128.0, 128.0), (156.0, 156.0)];
        assert_eq!(
            cascade(Some((100.0, 100.0)), &taken, None),
            Some((184.0, 184.0))
        );
    }

    #[test]
    fn with_no_window_in_front_it_falls_back_to_the_remembered_place() {
        assert_eq!(cascade(None, &[], Some((40.0, 60.0))), Some((68.0, 88.0)));
        assert_eq!(cascade(None, &[], None), None);
    }

    /// A screen this far down and across is a screen nobody has. The walk is
    /// bounded, so a display full of windows lands on top of one rather than
    /// off the end of the world.
    #[test]
    fn the_walk_is_bounded() {
        let taken: Vec<(f64, f64)> = (0..40)
            .map(|n| (100.0 + n as f64 * CASCADE, 100.0 + n as f64 * CASCADE))
            .collect();
        let spot = cascade(Some((100.0, 100.0)), &taken, None).expect("a spot");
        assert!(spot.0 <= 100.0 + 17.0 * CASCADE, "walked off: {spot:?}");
    }
}
