//! What a reading session costs, counted where it happens.
//!
//! The app's own memory table was made by running the real thing and watching
//! it from outside. That works and it says nothing about *which* of the three
//! places that hold a page is holding it, which is the fault that let the
//! thumbnail column leak for as long as it did. So the counters live in the
//! code that allocates, and `--measure` prints them beside the process's own
//! resident size.

use std::sync::atomic::{AtomicU64, Ordering};

/// Every call to `Widget::paint`, whether it drew anything or not. Under the
/// old canvas API this was the frame counter and it never stopped climbing;
/// the number to look at now is whether it stops.
pub static PAINTS: AtomicU64 = AtomicU64::new(0);
/// Pages drawn by the renderer, which should settle at one per mounted page.
pub static DRAWN: AtomicU64 = AtomicU64::new(0);
/// Pages recoloured without being drawn again — what a theme change costs.
pub static REPAINTED: AtomicU64 = AtomicU64::new(0);
/// Microseconds, because a page is single milliseconds and an average of
/// milliseconds in integers is a lie.
pub static DREW_US: AtomicU64 = AtomicU64::new(0);
pub static UPLOADED_US: AtomicU64 = AtomicU64::new(0);
/// Bytes of texture alive on the GPU, source and painted copies both.
pub static RESIDENT: AtomicU64 = AtomicU64::new(0);
/// Pages in the document right now — the mounting window, observed rather
/// than asserted.
pub static MOUNTED: AtomicU64 = AtomicU64::new(0);

pub fn add(counter: &AtomicU64, by: u64) {
    counter.fetch_add(by, Ordering::Relaxed);
}

pub fn sub(counter: &AtomicU64, by: u64) {
    counter.fetch_sub(by.min(counter.load(Ordering::Relaxed)), Ordering::Relaxed);
}

pub fn get(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

/// This process's resident size in megabytes.
///
/// Read from the platform rather than counted, because the counters above are
/// exactly the numbers that cannot see a leak they do not know about — which
/// is how the thumbnail column stayed invisible in the app's own accounting.
pub fn rss_mb() -> f64 {
    #[cfg(unix)]
    {
        let pid = std::process::id();
        if let Ok(out) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
        {
            if let Ok(text) = String::from_utf8(out.stdout) {
                if let Ok(kb) = text.trim().parse::<f64>() {
                    return kb / 1024.0;
                }
            }
        }
    }
    0.0
}

/// One line describing where the session stands.
pub fn line() -> String {
    let drawn = get(&DRAWN).max(1);
    format!(
        "{} mounted | {} drawn, {:.1}ms each, {:.1}ms uploading | {} recoloured in place | \
         {:.0}MB of texture | {:.0}MB resident | {} paints",
        get(&MOUNTED),
        get(&DRAWN),
        get(&DREW_US) as f64 / 1000.0 / drawn as f64,
        get(&UPLOADED_US) as f64 / 1000.0 / drawn as f64,
        get(&REPAINTED),
        get(&RESIDENT) as f64 / 1e6,
        rss_mb(),
        get(&PAINTS),
    )
}
