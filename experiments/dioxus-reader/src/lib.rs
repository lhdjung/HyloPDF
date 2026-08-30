//! Phase 1 of the Dioxus Native experiment: a reader that reads.
//!
//! `experiments/dioxus-assessment.md` is the plan and `experiments/FINDINGS.md`
//! is what Phase 0 answered. This crate is the next step: open a document,
//! scroll it, fit it, zoom it, put a theme on it — with the layout ported from
//! `viewer.ts` rather than reinvented, and with the renderer behind one trait
//! from the first line.
//!
//! What it is not: no sidebar, no search, no settings window, no markup, one
//! window. Those are Phase 3, and the point of leaving them out is that the
//! memory and speed numbers this produces are about the thing being proposed
//! rather than about a half-built app.

pub mod app;
pub mod gpu;
pub mod layout;
pub mod nav;
pub mod page;
pub mod pdfium;
pub mod recolor;
pub mod render;
pub mod shell;
pub mod stats;
pub mod styles;
pub mod theme;
