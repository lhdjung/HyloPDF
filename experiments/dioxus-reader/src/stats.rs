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
/// Pages of *text* held for the sake of a selection — see `Viewer::texts`.
///
/// The newest count rather than a total, so it is a level and not a tally:
/// what it says is how full that cache was the last time anything filled it,
/// and the only thing worth asserting about it is that it stays under its cap.
/// A page of text is a hundred kilobytes, which is small next to a texture and
/// large next to nothing — it is here because an unbounded cache of them was
/// the obvious way to write it and would have cost 40MB on a book read end to
/// end.
pub static TEXT_PAGES: AtomicU64 = AtomicU64::new(0);

pub fn add(counter: &AtomicU64, by: u64) {
    counter.fetch_add(by, Ordering::Relaxed);
}

pub fn sub(counter: &AtomicU64, by: u64) {
    counter.fetch_sub(by.min(counter.load(Ordering::Relaxed)), Ordering::Relaxed);
}

/// A level rather than a tally: the counter is *told* what it is now. See
/// [`TEXT_PAGES`], which is the one counter here that is not cumulative.
pub fn set(counter: &AtomicU64, to: u64) {
    counter.store(to, Ordering::Relaxed);
}

pub fn get(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

/// This process's resident size in megabytes.
///
/// Read from the platform rather than counted, because the counters above are
/// exactly the numbers that cannot see a leak they do not know about — which
/// is how the thumbnail column stayed invisible in the app's own accounting.
///
/// **This is not what a Mac means by memory, and Phase 1's table was wrong to
/// stop here.** A GPU buffer is charged to the process's *physical footprint*
/// and only partly to its resident size, so a renderer that allocates 173MB of
/// scratch on the device moves this number by single megabytes. See
/// [`footprint_mb`], which is what Activity Monitor shows and what the kernel
/// charges against a memory limit.
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

/// This process's physical footprint and its peak, in megabytes.
///
/// The number GPU allocations actually land in. `vmmap --summary` is the only
/// thing that reports it without linking against mach; it costs a few hundred
/// milliseconds, so it is read where a session is summarised and never in a
/// frame.
#[cfg(target_os = "macos")]
pub fn footprint_mb() -> (f64, f64) {
    let pid = std::process::id().to_string();
    let Ok(out) = std::process::Command::new("vmmap")
        .args(["--summary", &pid])
        .output()
    else {
        return (0.0, 0.0);
    };
    let (mut settled, mut peak) = (0.0, 0.0);
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        // "227.3M", "1.8G", "996K" — vmmap's own spelling.
        let text = value.trim();
        let (number, scale) = match text.chars().last() {
            Some('K') => (&text[..text.len() - 1], 1.0 / 1024.0),
            Some('M') => (&text[..text.len() - 1], 1.0),
            Some('G') => (&text[..text.len() - 1], 1024.0),
            _ => (text, 1.0 / (1024.0 * 1024.0)),
        };
        let parsed = number.trim().parse::<f64>().unwrap_or(0.0) * scale;
        match label.trim() {
            "Physical footprint" => settled = parsed,
            "Physical footprint (peak)" => peak = parsed,
            _ => {}
        }
    }
    (settled, peak)
}

#[cfg(not(target_os = "macos"))]
pub fn footprint_mb() -> (f64, f64) {
    (0.0, 0.0)
}

/// One line describing where the session stands.
pub fn line() -> String {
    let drawn = get(&DRAWN).max(1);
    let footprint = footprint_mb();
    format!(
        "{} mounted | {} drawn, {:.1}ms each, {:.1}ms uploading | {} recoloured in place | \
         {:.0}MB of texture | {:.0}MB resident, {:.0}MB footprint (peak {:.0}MB) | {} paints",
        get(&MOUNTED),
        get(&DRAWN),
        get(&DREW_US) as f64 / 1000.0 / drawn as f64,
        get(&UPLOADED_US) as f64 / 1000.0 / drawn as f64,
        get(&REPAINTED),
        get(&RESIDENT) as f64 / 1e6,
        rss_mb(),
        footprint.0,
        footprint.1,
        get(&PAINTS),
    )
}
