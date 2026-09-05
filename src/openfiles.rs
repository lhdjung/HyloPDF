//! Documents opened from the Finder, which do not arrive as arguments.
//!
//! **This is the hole `single.rs` names.** Every other way a document reaches
//! this reader is a launch with a path in `argv` — a terminal, `open -a`, "Open
//! with" on Linux, a second launch handing its path down the socket. The Finder
//! is not that: it activates the application and *then* sends it an Apple
//! Event, `'aevt'`/`'odoc'`. An application that does not answer it launches,
//! shows the start screen, and lets macOS report that it "cannot open files in
//! the PDF document format" — which is what somebody who has just made this
//! their default PDF reader sees, every time.
//!
//! **It goes to `NSAppleEventManager` rather than to a delegate**, and that is
//! measured rather than chosen: AppKit's own `'odoc'` handler forwards to
//! `application:openURLs:` on `NSApp`'s delegate, and winit sets no delegate at
//! all — `[NSApp delegate]` is nil for the life of the process, so there is
//! nothing to add the method to and nothing for AppKit to forward to. Taking
//! the event class directly is the one route that does not depend on somebody
//! else's object, and it costs the descriptor-walking below.
//!
//! **Armed from `can_create_surfaces`, not from `main`.** `NSApplication`
//! installs its own `'odoc'` handler while it finishes launching, so a handler
//! set before the event loop starts would be the one that gets replaced.
//! `can_create_surfaces` is the first callback after that, and the Finder's
//! event is queued behind it even on a cold launch — so the document arrives
//! after the window that will take it, which is what
//! [`crate::session::Session::hand_over`] wants: a start screen with nothing in
//! it is filled rather than displaced.

use std::ffi::{c_char, CStr};
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
use objc2::{msg_send, sel, ClassType};

use crate::shell::Remote;

/// How the handler reaches the shell. AppKit calls the method below with no
/// context of its own, so the only way to hand it anything is a static — the
/// same arrangement, and the same reason, as `dock.rs`.
static SHELL: OnceLock<Remote> = OnceLock::new();

/// The four-character codes this needs, which have no Rust binding here:
/// `'aevt'`, `'odoc'`, `'----'` (the direct object) and `'furl'` (a file URL).
const CLASS_APPLE_EVENT: u32 = u32::from_be_bytes(*b"aevt");
const EVENT_OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
const KEY_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const TYPE_FILE_URL: u32 = u32::from_be_bytes(*b"furl");

fn tracing() -> bool {
    std::env::var_os("HYLOPDF_TRACE").is_some()
}

/// One item of the event's list, as a POSIX path.
///
/// The descriptor carries a URL rather than a path — `file:///Users/…/a%20b.pdf`
/// — so it is coerced to `'furl'`, read as bytes, and handed to `NSURL`, whose
/// `path` is the decoded answer. Doing the decoding here instead would be a
/// second percent-decoder in a codebase that already links the system's.
unsafe fn path_of(item: *mut AnyObject) -> Option<String> {
    unsafe {
        let url_desc: *mut AnyObject = msg_send![item, coerceToDescriptorType: TYPE_FILE_URL];
        if url_desc.is_null() {
            return None;
        }
        let data: *mut AnyObject = msg_send![url_desc, data];
        if data.is_null() {
            return None;
        }
        let bytes: *const u8 = msg_send![data, bytes];
        let length: usize = msg_send![data, length];
        if bytes.is_null() || length == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(bytes, length);
        let text = std::str::from_utf8(text).ok()?;
        let ns_string: *mut AnyObject = msg_send![
            AnyClass::get(c"NSString").expect("NSString"),
            stringWithUTF8String: std::ffi::CString::new(text).ok()?.as_ptr()
        ];
        let url: *mut AnyObject = msg_send![
            AnyClass::get(c"NSURL").expect("NSURL"),
            URLWithString: ns_string
        ];
        if url.is_null() {
            return None;
        }
        let path: *mut AnyObject = msg_send![url, path];
        if path.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![path, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

/// `-[… handleOpen:withReplyEvent:]`. One event carries every document of a
/// multiple selection, so this is a list and not a path.
extern "C" fn handle_open(
    _this: *mut AnyObject,
    _cmd: Sel,
    event: *mut AnyObject,
    _reply: *mut AnyObject,
) {
    let Some(shell) = SHELL.get() else { return };
    unsafe {
        let list: *mut AnyObject = msg_send![event, paramDescriptorForKeyword: KEY_DIRECT_OBJECT];
        if list.is_null() {
            return;
        }
        // Apple Event lists are indexed from one.
        let count: i32 = msg_send![list, numberOfItems];
        for index in 1..=count {
            let item: *mut AnyObject = msg_send![list, descriptorAtIndex: index];
            if item.is_null() {
                continue;
            }
            if let Some(path) = path_of(item) {
                if tracing() {
                    eprintln!("openfiles: {path}");
                }
                shell.request(Some(path));
            }
        }
    }
}

/// Take `'odoc'` for this process, once.
pub fn install(shell: Remote) {
    if SHELL.set(shell).is_err() {
        return;
    }
    unsafe {
        let Some(mut builder) = ClassBuilder::new(c"HyloPDFOpenFiles", NSObject::class()) else {
            return;
        };
        builder.add_method(
            sel!(handleOpen:withReplyEvent:),
            handle_open as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject),
        );
        let class = builder.register();
        // Never released: it is the handler for as long as the process lives,
        // and `NSAppleEventManager` does not retain it.
        let target: *mut AnyObject = msg_send![class, new];

        let manager: *mut AnyObject = msg_send![
            AnyClass::get(c"NSAppleEventManager").expect("NSAppleEventManager"),
            sharedAppleEventManager
        ];
        let _: () = msg_send![
            manager,
            setEventHandler: target,
            andSelector: sel!(handleOpen:withReplyEvent:),
            forEventClass: CLASS_APPLE_EVENT,
            andEventID: EVENT_OPEN_DOCUMENTS,
        ];
        if tracing() {
            eprintln!("openfiles: armed on 'aevt'/'odoc'");
        }
    }
}
