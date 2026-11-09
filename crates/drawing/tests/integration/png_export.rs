//! What a PNG export shows, tested through `Editor::export`: the Scene or
//! the Selection, framed with Excalidraw's padding. Clients rasterise it.

use std::collections::BTreeSet;

use std::sync::Arc;

use pkm_drawing::{Editor, ExportScope, Tool};

use crate::common;
use common::{FixedWidth, editor_with, tap};

fn rectangle(id: &str, x: f64) -> String {
    format!(
        r#"{{"id":"{id}","type":"rectangle","x":{x},"y":100,"width":100,"height":50,
        "backgroundColor":"transparent","roundness":null}}"#
    )
}

fn drawn_elements(export: &pkm_drawing::Export) -> BTreeSet<u32> {
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

#[test]
fn a_text_being_edited_is_exported_as_typed_so_far() {
    // The Canvas leaves it to the text view; the export has no text view.
    let mut editor = two_rectangles();
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor.set_tool(Tool::Text);
    tap(&mut editor, "mouse", (500.0, 300.0), 0.0);
    editor.set_editing_text("hello");
    assert!(editor.text_editor().is_some());

    let export = editor.export(ExportScope::Scene).unwrap();

    let lines = export.geometry.text_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(export.geometry.line_text(&lines[0]), "hello");
    assert!(export.x + export.width > 500.0 && export.y + export.height > 300.0);
}

#[test]
fn a_freedraw_stroke_is_framed_by_its_points() {
    let editor = editor_with(&[r#"{"id":"f","type":"freedraw","x":100,"y":100,
        "width":50,"height":40,"points":[[0,0],[-20,-30],[30,10]],
        "pressures":[],"simulatePressure":true}"#]);

    let export = editor.export(ExportScope::Scene).unwrap();

    assert_eq!((export.x, export.y), (70.0, 60.0));
    assert_eq!((export.width, export.height), (70.0, 60.0));
}

#[test]
fn a_line_with_negative_points_is_framed_by_its_points() {
    let editor = editor_with(&[r#"{"id":"l","type":"line","x":100,"y":100,
        "width":50,"height":40,"points":[[0,0],[-50,-40]],
        "roughness":0,"roundness":null}"#]);

    let export = editor.export(ExportScope::Scene).unwrap();

    assert_eq!((export.x, export.y), (40.0, 50.0));
    assert_eq!((export.width, export.height), (70.0, 60.0));
}

/// The PNG size, at scale 2, Excalidraw exports a visual parity fixture at.
fn exported_png_size(fixture: &str) -> (f64, f64) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/visual-parity/fixtures/"
    );
    let mut editor = Editor::new();
    editor
        .load(&std::fs::read(format!("{path}{fixture}.excalidraw")).unwrap())
        .unwrap();
    let export = editor.export(ExportScope::Scene).unwrap();
    ((export.width * 2.0).round(), (export.height * 2.0).round())
}

#[test]
fn freedraw_strokes_export_at_excalidraws_size() {
    assert_eq!(exported_png_size("freedraw"), (700.0, 700.0));
}

#[test]
fn lines_and_arrows_are_framed_by_their_sketched_stroke_like_excalidraw() {
    assert_eq!(exported_png_size("lines"), (1163.0, 1001.0));
}
