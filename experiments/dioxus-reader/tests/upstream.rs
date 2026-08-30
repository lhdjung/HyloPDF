//! Two faults that belong to somebody else, kept as the smallest thing that
//! shows each — to be sent upstream, and to say so the day either is fixed.
//!
//! The first runs with the suite, because it catches the panic it is about and
//! therefore *passes while the bug is there*: the day it fails is the day the
//! workaround it names can go. The second aborts the process rather than
//! panicking, which is not something to do to a test run, so it is
//! `#[ignore]`d:
//!
//! ```text
//! cargo test --test upstream -- --ignored
//! ```

use blitz_test_harness::Harness;
use dioxus::prelude::*;

/// **Stylo panics when a stylesheet changes while anything is hovered.**
///
/// `StylesheetInvalidationSet::invalidation_kind_for` calls `each_class` on
/// every element *snapshot* it finds while walking the tree, and
/// `ServoElementSnapshot::each_class` goes through `get_attr`, which is
/// `self.attrs.as_ref().unwrap()`. Blitz takes a cheap snapshot for a state
/// change alone — `snapshot_node_state_only`, which is what a hover or a press
/// produces — and that snapshot has `attrs: None`.
///
/// So: hover a button, change a stylesheet, and the process panics from a
/// stack with nothing of the application in it. It is a plain click on a
/// button that changes the theme, which is how this was found: the reader's
/// own `t` shortcut was fine and the *button beside it* was not.
///
/// Either side could fix it. Stylo's own element-wrapper path guards with
/// `has_attrs()` before reaching for them and this path does not; equally,
/// Blitz could fill the attributes in when it upgrades a state-only snapshot.
/// Reported against `stylo 0.20.0` and `blitz-dom 0.3.0-beta.2`.
///
/// The reader works around it by never rewriting its stylesheet: the theme is
/// a set of custom properties on the root, and an attribute change is a
/// snapshot that *does* carry attributes. See `styles.rs`.
#[test]
fn a_stylesheet_that_changes_under_a_hovered_node() {
    #[component]
    fn Themed() -> Element {
        let mut dark = use_signal(|| false);
        // Two things in this sheet are load-bearing and neither is obvious.
        // The `:hover` rule is what makes Blitz take a snapshot at all — it
        // snapshots a state change only when some rule depends on the state
        // bits, which is the check at `document.rs`'s `snapshot_node_impl`.
        // And the *class* selector is what makes Stylo ask that snapshot for
        // its classes: the walk skips that branch when the changed sheet has
        // no class selectors in it. Drop either and this test passes and says
        // nothing.
        let sheet = if dark() {
            ".chip { color: #ffffff; } .chip:hover { color: #eeeeee; }"
        } else {
            ".chip { color: #000000; } .chip:hover { color: #111111; }"
        };
        rsx! {
            style { "{sheet}" }
            button {
                class: "chip",
                onclick: move |_| dark.set(!dark()),
                "theme"
            }
        }
    }

    // The click is two things at once: the pointer lands on the button, which
    // is a state-only snapshot, and the handler rewrites the stylesheet.
    //
    // Caught rather than left to fly, so that this test *passes while the bug
    // is there* and fails the day it is fixed — which is the day the
    // workaround in `styles.rs` can go.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut harness = Harness::from_component(Themed);
        harness.click(".chip");
    }));
    std::panic::set_hook(hook);
    assert!(
        outcome.is_err(),
        "no panic: this is fixed upstream, and `styles.rs` can stop working \
         around it"
    );
}

/// **`pdfium-render`'s `thread_safe` feature does not serialise anything.**
///
/// It is two `unsafe impl`s — `Send` and `Sync` for `Pdfium` — and a `Send +
/// Sync` bound on the bindings accessor. pdfium itself has process-wide state
/// and no locking of its own, so two threads inside it abort the process:
/// `SIGABRT`, exit 134, no panic, no message, no stack.
///
/// This is not a bug so much as a name that promises something it does not
/// deliver, and the cost of believing it is a test binary that vanishes.
/// `pdfium.rs` takes a process-wide lock in front of every call, which is what
/// the feature's name suggests is already happening.
///
/// **With the lock in place this passes**, which is what makes it a
/// regression test rather than a demonstration: take the lock out of
/// `pdfium.rs` and it aborts. It stays `#[ignore]`d for exactly that reason —
/// a failure here is not a failure, it is the whole binary going away, and
/// that is not a thing to have happen in a run somebody is reading.
#[test]
#[ignore = "aborts the process rather than failing"]
fn pdfium_is_not_thread_safe() {
    use dioxus_reader::render;
    let path = format!(
        "{}/../../tests/fixtures/book.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                let document = render::open(&path).unwrap();
                for page in 0..4 {
                    document
                        .render(page, 400, 500, &mut |_bitmap| {})
                        .unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
