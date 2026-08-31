//! The three names `watch.rs` reaches for, supplied by this crate.
//!
//! **This module exists so that the app's `watch.rs` compiles here with no
//! change at all.** That file opens with `use tauri::{AppHandle, Emitter};`,
//! and the first segment of a `use` path is looked up in the extern prelude —
//! so making it resolve means putting something called `tauri` in there.
//! `extern crate self as tauri;` in `lib.rs` is that, in one line: this crate
//! answers to its own name and to `tauri`, [`AppHandle`] and [`Emitter`] are
//! re-exported at its root, and the app's file finds what it asked for. One
//! file, two meanings of `tauri`, and nothing in between them.
//!
//! (The obvious version of this is a module named `tauri`, and it does not
//! work: a `use` path anchored on a bare identifier does not see the crate
//! root's modules, only crates. The compiler says as much and suggests
//! `crate::tauri`, which is the one thing that cannot be written here,
//! because what is being avoided is editing the file.)
//!
//! It is worth being plain about what that does and does not prove. The
//! assessment predicted this file would need one change — "`emit_to(window,
//! …)` becomes an `EventLoopProxy::send_event`" — and it was nearly right:
//! the change is real, and it is *here* rather than there. What is proved is
//! the useful half. Everything `watch.rs` knows — that a change is a burst,
//! that a theme reload is decided by comparing themes rather than by a file
//! having moved, that a document is not believed until it ends the way a PDF
//! ends, that the watch goes on the directory because a compiler renames over
//! the file — is about the disk, and none of it is about Tauri. The Tauri in
//! it is two names and two method calls.
//!
//! The signatures are Tauri's, not ours, because that is the constraint: a
//! shim that took a nicer argument would be a shim the app's file does not
//! compile against, which is the whole of what is being tested. The one cost
//! that comes with them is [`Emitter::emit`]'s `S: Serialize` bound — a
//! payload arrives here as a `serde_json::Value` rather than as the
//! `Vec<Theme>` it started as, which is the bridge's serialisation surviving
//! in a build that has no bridge. It fires when somebody saves a theme file,
//! and it is fourteen themes of five colours. A `watch.rs` genuinely rewritten
//! for this crate would hand the vector over; this one is not rewritten, and
//! that is the point.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use serde::Serialize;

/// What a window's frontend is told, and by whom.
///
/// The shape is the bridge's, kept deliberately: an event has a name, a
/// payload, and either a target or everybody. There is one window in this
/// crate and `target` is therefore always ignored — but it is carried rather
/// than dropped, because `emit_to` naming one window is the whole reason
/// `watch.rs` follows a document *per window*, and item 9 is where that stops
/// being theoretical.
#[derive(Clone, Debug, PartialEq)]
pub struct News {
    pub event: String,
    pub target: Option<String>,
    pub payload: serde_json::Value,
}

/// Where news waits until somebody reads it.
///
/// **A mailbox with a waker in it, and the waker is the load-bearing half.**
/// The watcher is a thread and the reader is a Dioxus task, so what is needed
/// between them is not a channel but a way for the thread to say "poll me" to
/// a task it cannot see. That is exactly what a `Waker` is for, and the chain
/// it starts is already built: waking a task marks it ready, which wakes the
/// virtual DOM, which — through the waker `View::poll` built out of the shell
/// proxy — puts an event on the winit loop, which polls the document. So a
/// theme saved in an editor reaches the screen without anything anywhere
/// polling a clock.
///
/// In the harness the same wake marks the task ready and the next `pump()`
/// runs it, which is why a test can drive this with no window and no thread.
#[derive(Clone, Default)]
pub struct Post(Arc<Mutex<Mailbox>>);

#[derive(Default)]
struct Mailbox {
    waiting: VecDeque<News>,
    waker: Option<Waker>,
}

impl Post {
    pub fn new() -> Post {
        Post::default()
    }

    /// Leave news, and wake whoever is waiting for it.
    pub fn send(&self, news: News) {
        let waker = {
            let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
            held.waiting.push_back(news);
            held.waker.take()
        };
        // Outside the lock: a waker may run the task inline, and a task that
        // reads the mailbox while the lock is held is a deadlock.
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// The next piece of news, whenever it comes.
    pub fn next(&self) -> Next {
        Next(self.clone())
    }

    /// Whatever is waiting, without waiting. What a test reads when it would
    /// rather assert on the news than on what the reader did with it.
    pub fn take(&self) -> Option<News> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiting
            .pop_front()
    }
}

pub struct Next(Post);

impl std::future::Future for Next {
    type Output = News;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<News> {
        let mut held = self.0 .0.lock().unwrap_or_else(|e| e.into_inner());
        match held.waiting.pop_front() {
            Some(news) => Poll::Ready(news),
            None => {
                // Replaced rather than kept: a task can be polled by a
                // different waker than the one it registered last time, and
                // waking the stale one wakes nothing.
                held.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// What `watch.rs` is handed, and the whole of what it does with it is emit.
///
/// Tauri's is a handle on the running application. Here it is a handle on one
/// mailbox, which is the only part of an application that file ever reaches
/// for.
#[derive(Clone)]
pub struct AppHandle(Post);

impl AppHandle {
    pub fn new(post: Post) -> AppHandle {
        AppHandle(post)
    }
}

/// Who an event is for. Tauri's `emit_to` takes `impl Into<EventTarget>` and
/// `watch.rs` passes a `&str`, so that is the conversion this needs.
pub struct EventTarget(String);

impl From<&str> for EventTarget {
    fn from(label: &str) -> EventTarget {
        EventTarget(label.to_string())
    }
}

impl From<String> for EventTarget {
    fn from(label: String) -> EventTarget {
        EventTarget(label)
    }
}

/// A payload that could not be turned into JSON, which is the only way any of
/// this fails. Nothing acts on it — `watch.rs` writes `let _ = app.emit(…)`,
/// as it must, because there is nowhere on a watcher thread to report an
/// error to.
#[derive(Debug)]
pub struct Undeliverable;

pub type Result<T> = std::result::Result<T, Undeliverable>;

/// Tauri's trait, with the two methods `watch.rs` calls.
pub trait Emitter {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()>;
    fn emit_to<I: Into<EventTarget>, S: Serialize + Clone>(
        &self,
        target: I,
        event: &str,
        payload: S,
    ) -> Result<()>;
}

impl Emitter for AppHandle {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()> {
        self.deliver(event, None, payload)
    }

    fn emit_to<I: Into<EventTarget>, S: Serialize + Clone>(
        &self,
        target: I,
        event: &str,
        payload: S,
    ) -> Result<()> {
        self.deliver(event, Some(target.into().0), payload)
    }
}

impl AppHandle {
    fn deliver<S: Serialize>(&self, event: &str, target: Option<String>, payload: S) -> Result<()> {
        let payload = serde_json::to_value(payload).map_err(|_| Undeliverable)?;
        self.0.send(News {
            event: event.to_string(),
            target,
            payload,
        });
        Ok(())
    }
}
