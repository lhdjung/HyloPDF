//! What the document actually contains, with no window in front of it.
//!
//! A canvas that never asks its paint source to draw is either not a canvas as
//! far as blitz-dom is concerned, or is one that nothing paints. This tells the
//! two apart by building the document and reading the node back.
//!
//! It is also the first piece of the harness the assessment asks for in Phase
//! 2: a `DioxusDocument` built in process, driven, and inspected, with no GPU
//! and no window.

use blitz_dom::DocumentConfig;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native::DioxusDocument;

fn main() {
    let vdom = VirtualDom::new(App);
    let mut doc = DioxusDocument::new(vdom, DocumentConfig::default());
    doc.set_viewport(Viewport::new(800, 600, 1.0, ColorScheme::Light));
    doc.initial_build();
    doc.resolve(0.0);

    let count = doc.tree().len();
    println!("{count} nodes");
    for (id, node) in doc.tree().iter() {
        let Some(element) = node.element_data() else {
            continue;
        };
        let name = element.name.local.as_ref();
        if let Some(named) = element
            .attrs()
            .iter()
            .find(|a| a.name.local.as_ref() == "id")
        {
            println!("#{}: <{name}> {:?}", named.value, node.final_layout.size);
        }
        if name != "canvas" {
            continue;
        }
        let attrs: Vec<String> = element
            .attrs()
            .iter()
            .map(|a| format!("{}={:?}", a.name.local, a.value))
            .collect();
        println!(
            "node {id}: <{name}> {:?} attrs [{}] special {}",
            node.final_layout.size,
            attrs.join(", "),
            match element.canvas_data() {
                Some(data) => format!("canvas source {}", data.custom_paint_source_id),
                None => "none".to_string(),
            }
        );
    }
}

#[component]
fn App() -> Element {
    rsx! {
        canvas { "src": "7", style: "display: block; width: 100px; height: 100px;" }
        // Does a line that says it must not wrap, wrap? In the chrome spike
        // the document title came out on two lines, and `text-overflow` being
        // missing does not explain that on its own.
        div {
            id: "nowrap",
            style: "width: 120px; overflow: hidden; white-space: nowrap; font-size: 13px;",
            "A rather long document title that has to be cut off somewhere.pdf"
        }
        div {
            id: "wrap",
            style: "width: 120px; font-size: 13px;",
            "A rather long document title that has to be cut off somewhere.pdf"
        }
    }
}
