//! Taking in a Drawing that Sync rewrote while it is open
//! (`Editor::merge_synced`), tested at the Editor seam: bytes in, saved
//! Drawing, Selection, undo availability and viewport out. Expected values
//! are literals worked out from Excalidraw's `version` rule.

use std::sync::Arc;

use pkm_drawing::json::Value;
use pkm_drawing::{Editor, Tool};

use crate::common;
use common::{FixedWidth, editor_with, num, replay, saved_elements, tap};

/// A rectangle at (100, 100), 100 x 50, version 3.
const A: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "roundness":null,"strokeWidth":2,"backgroundColor":"#ffc9c9","version":3,"versionNonce":7,"index":"a0","isDeleted":false}"##;

/// A rectangle at (400, 400), 100 x 50, version 3.
const B: &str = r##"{"id":"b","type":"rectangle","x":400,"y":400,"width":100,"height":50,
    "roundness":null,"strokeWidth":2,"backgroundColor":"#ffc9c9","version":3,"versionNonce":8,"index":"a1","isDeleted":false}"##;

fn drawing(elements: &[&str]) -> Vec<u8> {
    format!(
        r#"{{"type":"excalidraw","version":2,"elements":[{}]}}"#,
        elements.join(",")
    )
    .into_bytes()
}

/// `element` as edited elsewhere: version 9 with `from` replaced by `to`.
fn edited(element: &str, from: &str, to: &str) -> String {
    assert!(element.contains(from), "{from}");
    element
        .replace(r#""version":3"#, r#""version":9"#)
        .replace(from, to)
}

fn element(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn x_of(editor: &Editor, id: &str) -> f64 {
    num(&element(editor, id), "x")
}

fn is_deleted(editor: &Editor, id: &str) -> bool {
    element(editor, id)
        .get("isDeleted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Drags the rectangle `A` by 20 to the right with the mouse.
fn drag_a(editor: &mut Editor) {
    replay(
        editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 125
         1.2 up 1 mouse 180 125",
    );
}

#[test]
fn changes_from_elsewhere_appear_in_the_scene() {
    let mut editor = editor_with(&[A]);
    let remote = drawing(&[A, &edited(B, r#""x":400"#, r#""x":450"#)]);
    editor.merge_synced(&remote).unwrap();

    assert_eq!(editor.element_count(), 2);
    assert_eq!(x_of(&editor, "b"), 450.0);
    assert_eq!(x_of(&editor, "a"), 100.0);
}

#[test]
fn a_change_of_an_element_the_user_changed_too_goes_by_version() {
    let mut editor = editor_with(&[A]);
    drag_a(&mut editor); // a is version 4 at x = 120
    let remote = drawing(&[&edited(A, r#""x":100"#, r#""x":500"#)]); // version 9
    editor.merge_synced(&remote).unwrap();
    assert_eq!(x_of(&editor, "a"), 500.0);
}

#[test]
fn merging_is_not_an_undo_step() {
    let mut editor = editor_with(&[A]);
    drag_a(&mut editor);
    editor.merge_synced(&drawing(&[A, B])).unwrap();

    // Undo takes back the user's drag, not the arrival of `b`.
    editor.undo();
    assert_eq!(x_of(&editor, "a"), 100.0);
    assert!(!is_deleted(&editor, "b"));
    assert!(!editor.can_undo());
    assert!(editor.can_redo());
}

#[test]
fn merging_leaves_redo_as_it_was() {
    let mut editor = editor_with(&[A]);
    drag_a(&mut editor);
    editor.undo();
    editor.merge_synced(&drawing(&[A, B])).unwrap();

    assert!(editor.can_redo());
    editor.redo();
    assert_eq!(x_of(&editor, "a"), 120.0);
    assert!(!is_deleted(&editor, "b"));
}

#[test]
fn the_selection_stays_less_the_elements_deleted_elsewhere() {
    let mut editor = editor_with(&[A, B]);
    tap(&mut editor, "mouse", (150.0, 125.0), 1.0);
    assert_eq!(editor.selection(), ["a"]);

    editor
        .merge_synced(&drawing(&[A, &edited(B, r#""x":400"#, r#""x":450"#)]))
        .unwrap();
    assert_eq!(editor.selection(), ["a"]);

    editor
        .merge_synced(&drawing(&[
            &edited(A, r#""isDeleted":false"#, r#""isDeleted":true"#),
            B,
        ]))
        .unwrap();
    assert!(editor.selection().is_empty());
}

#[test]
fn the_viewport_stays() {
    let mut editor = editor_with(&[A]);
    editor.set_viewport((-30.0, 20.0), 2.0);
    let before = editor.viewport();
    editor.merge_synced(&drawing(&[A, B])).unwrap();
    assert_eq!(editor.viewport(), before);
}

#[test]
fn the_revision_changes_so_clients_draw_again() {
    let mut editor = editor_with(&[A]);
    let before = editor.revision();
    editor.merge_synced(&drawing(&[A, B])).unwrap();
    assert_ne!(editor.revision(), before);
}

#[test]
fn bytes_that_add_nothing_change_nothing() {
    let mut editor = Editor::new();
    let file = format!(
        r#"{{"type":"excalidraw","version":2,"elements":[{A}],"appState":{{}},"files":{{}}}}"#
    );
    editor.load(file.as_bytes()).unwrap();
    let before = editor.revision();
    // What the Drawing's own file says.
    let own = editor.save();
    editor.merge_synced(&own).unwrap();
    // Twice over, as Sync may rewrite a file with what it has already.
    editor.merge_synced(&own).unwrap();
    assert_eq!(editor.revision(), before);
}

#[test]
fn bytes_that_are_not_a_drawing_are_refused_and_the_scene_kept() {
    let mut editor = editor_with(&[A]);
    assert!(editor.merge_synced(b"{\"type\":\"nope\"}").is_err());
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn an_element_being_dragged_keeps_the_local_version_until_the_drag_ends() {
    let mut editor = editor_with(&[A, B]);
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 125",
    );
    assert_eq!(x_of(&editor, "a"), 120.0);

    let remote = drawing(&[
        &edited(A, r#""x":100"#, r#""x":500"#),
        &edited(B, r#""x":400"#, r#""x":450"#),
    ]);
    editor.merge_synced(&remote).unwrap();
    assert_eq!(x_of(&editor, "a"), 120.0, "dragged element keeps local");
    assert_eq!(x_of(&editor, "b"), 450.0, "others take the remote");

    // The drag goes on where the user's finger is.
    replay(
        &mut editor,
        "1.2 move 1 mouse 180 125
         1.3 up 1 mouse 180 125",
    );
    assert_eq!(x_of(&editor, "a"), 130.0);

    // Once it ended the normal version rule applies.
    editor.merge_synced(&remote).unwrap();
    assert_eq!(x_of(&editor, "a"), 500.0);
}

#[test]
fn a_shape_being_drawn_is_not_taken_away_by_a_drawing_without_it() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40",
    );
    editor.merge_synced(&drawing(&[B])).unwrap();
    replay(&mut editor, "0.2 up 1 mouse 60 40");
    assert_eq!(editor.element_count(), 2);
    assert_eq!(x_of(&editor, "b"), 400.0);
}

#[test]
fn the_open_text_view_and_its_text_stay() {
    let mut editor = Editor::new();
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor.set_tool(Tool::Text);
    tap(&mut editor, "finger", (50.0, 50.0), 1.0);
    editor.set_editing_text("hello");

    editor.merge_synced(&drawing(&[A])).unwrap();

    let view = editor.text_editor().expect("the text view stays open");
    assert_eq!(view.text, "hello");
    editor.set_editing_text("hello!");
    assert_eq!(editor.text_editor().unwrap().text, "hello!");
    assert_eq!(editor.element_count(), 2);
}
