//! A plain PDF of however many pages you ask for.
//!
//! ```text
//! cargo run --example fixture -- smoke.pdf 5
//! ```
//!
//! This exists for the packaging job in `.github/workflows/bundle.yml`, which
//! needs a document to open with what it has just installed and has no
//! `cargo test` to write one. It was a Node script; the script went with the
//! port and the workflow went on calling it, which is how every bundle job
//! came to fail on a missing module. `fixture::draft` is what the suite's own
//! recompile tests use, so there is one writer and not two.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: fixture <path> [pages]");
    let pages = args
        .next()
        .map_or(5, |count| count.parse().expect("a number of pages"));
    hylopdf::fixture::draft(std::path::Path::new(&path), pages);
    println!("wrote {path}, {pages} pages");
}
