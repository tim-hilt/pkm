//! What a PNG export shows, tested through `Editor::export`: the Scene or
//! the Selection, framed with Excalidraw's padding. Clients rasterise it.

use std::collections::BTreeSet;

use workingtitle_core::{Editor, ExportScope};

mod common;
use common::{editor_with, tap};

fn rectangle(id: &str, x: f64) -> String {
    format!(
        r#"{{"id":"{id}","type":"rectangle","x":{x},"y":100,"width":100,"height":50,
        "backgroundColor":"transparent","roundness":null}}"#
    )
}

fn drawn_elements(export: &workingtitle_core::Export) -> BTreeSet<u32> {
    export.geometry.parts().iter().map(|p| p.element).collect()
}

fn two_rectangles() -> Editor {
    let mut editor = editor_with(&[&rectangle("a", 100.0), &rectangle("b", 300.0)]);
    editor.set_view_size(800.0, 600.0);
    editor
}

#[test]
fn the_scene_is_exported_whole_with_excalidraws_padding() {
    let editor = two_rectangles();

    let export = editor.export(ExportScope::Scene).unwrap();

    assert_eq!((export.x, export.y), (90.0, 90.0));
    assert_eq!((export.width, export.height), (320.0, 70.0));
    assert_eq!(drawn_elements(&export).len(), 2);
}

#[test]
fn the_selection_is_exported_alone() {
    let mut editor = two_rectangles();
    tap(&mut editor, "mouse", (300.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["b"]);

    let export = editor.export(ExportScope::Selection).unwrap();

    assert_eq!((export.x, export.y), (290.0, 90.0));
    assert_eq!((export.width, export.height), (120.0, 70.0));
    assert_eq!(drawn_elements(&export).len(), 1);
}

#[test]
fn nothing_to_export_gives_none() {
    assert!(Editor::new().export(ExportScope::Scene).is_none());
    assert!(two_rectangles().export(ExportScope::Selection).is_none());
}
