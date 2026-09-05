//! One HyloPDF at a time, and how a second launch hands its document over.
//!
//! The app uses `tauri-plugin-single-instance`, and `AGENTS.md` says why it
//! has to have one: three double-clicked documents mean three launches, and
//! three processes writing over each other's `settings.toml` is a race no
//! lock inside one of them can help with. The same is true here and rather
//! more so — this reader keeps a library, a themes directory and a settings
//! table on disk, and holds a process-wide lock around pdfium.
//!
//! The assessment's table offers "`single-instance`, or a Unix socket / named
//! pipe". This is the socket, and it is about thirty lines because the socket
//! is doing both jobs at once: **binding it is the claim, and connecting to it
//! is how the document gets across**. A lock file would need a second channel
//! beside it to carry the path, and that channel would be this.
//!
//! **Unix only, deliberately.** Windows wants a named pipe and there is no
//! std type for one, so a second launch there is a second process — which is
//! the state this experiment has been in since Phase 0, and is honest about:
//! multi-window "has been shown to work on macOS and nowhere else". The
//! branch is where the port would go, not what it would look like.
//!
//! **What this cannot reach is Apple Events**, which is how macOS tells an
//! app that is *already running* to open a document. `RunEvent::Opened` in the
//! app is that, and it needs an `NSApplicationDelegate` — and, before any of
//! that, an application bundle with an `Info.plist` declaring that this
//! program opens PDFs. This experiment is a binary run from a terminal, so
//! there is nothing for the Finder to send an event to. Every other route a
//! document arrives by — a second `cargo run -- paper.pdf`, `open -a` on a
//! bundle, "Open with" on Linux — is a launch with an argument, and a launch
//! with an argument comes through here.

use std::path::{Path, PathBuf};

/// What a launch found.
pub enum Claim {
    /// This process is the one. Hold the listener and serve it.
    #[cfg(unix)]
    First(std::os::unix::net::UnixListener),
    /// Somebody else is; the document has been handed to them.
    Second,
    /// Nothing could be claimed and nothing is being served — the platform
    /// has no socket, or the socket could not be made. A second reader is
    /// worse than one and better than none.
    Alone,
}

fn socket_path(dir: &Path) -> PathBuf {
    dir.join("instance.sock")
}

/// Claim this process as the only one, or hand `path` to whoever already has.
///
/// `Second` means the caller should exit, quietly and successfully: the
/// document is on its way to a window that already exists, which is what the
/// reader asked for.
#[cfg(unix)]
pub fn claim(dir: &Path, path: Option<&str>) -> Claim {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};

    let socket = socket_path(dir);
    // Connect first, because a socket file that exists proves nothing: a
    // process killed with SIGKILL leaves one behind, and the only way to tell
    // a live one from a corpse is to try it.
    if let Ok(mut stream) = UnixStream::connect(&socket) {
        let _ = stream.write_all(path.unwrap_or("").as_bytes());
        return Claim::Second;
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::remove_file(&socket);
    match UnixListener::bind(&socket) {
        Ok(listener) => Claim::First(listener),
        // Two launches in the same instant: the other one bound between our
        // connect and our bind. Ask it again rather than giving up, because
        // the alternative is two readers with two libraries.
        Err(_) => match UnixStream::connect(&socket) {
            Ok(mut stream) => {
                let _ = stream.write_all(path.unwrap_or("").as_bytes());
                Claim::Second
            }
            Err(_) => Claim::Alone,
        },
    }
}

#[cfg(not(unix))]
pub fn claim(_dir: &Path, _path: Option<&str>) -> Claim {
    Claim::Alone
}

/// Answer the door for as long as the process lives.
///
/// A thread, because accepting blocks and the main thread is drawing. What
/// arrives is a path or nothing, and either way it goes to the shell as a
/// request for a window — `Session::hand_over` is what decides whether that
/// means a new window or bringing one forward.
#[cfg(unix)]
pub fn serve(listener: std::os::unix::net::UnixListener, shell: crate::shell::Remote) {
    use std::io::Read;

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut said = String::new();
            if stream.read_to_string(&mut said).is_err() {
                continue;
            }
            let said = said.trim();
            shell.request((!said.is_empty()).then(|| said.to_string()));
        }
    });
}

/// Take the socket away, so that the next launch is not left connecting to a
/// process that has gone.
///
/// A best effort and nothing more: a process that is killed outright leaves
/// the file behind, which is exactly why [`claim`] connects before it looks.
pub fn release(dir: &Path) {
    let _ = std::fs::remove_file(socket_path(dir));
}
