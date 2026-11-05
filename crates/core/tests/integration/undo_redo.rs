//! Undo and redo (ADR-0006), tested at the Editor seam with recorded pointer
//! sequences: pointer events and commands in, saved Drawing, Selection and
//! undo/redo availability out.

use workingtitle_core::json::{Value, parse};
use workingtitle_core::{Editor, Tool};

use crate::common;
use common::{editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50.
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// The `elements` of the saved Drawing.
fn saved(editor: &Editor) -> Vec<Value> {
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    drawing
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .to_vec()
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

/// The position of the Element `id`.
fn position(editor: &Editor, id: &str) -> (f64, f64) {
    let elements = saved(editor);
    let element = elements
        .iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap();
    (number(element, "x"), number(element, "y"))
}

/// Draws a rectangle from (10, 10) to (60, 40) and returns to selection.
fn draw_rectangle(editor: &mut Editor) {
    editor.set_tool(Tool::Rectangle);
    replay(
        editor,
        "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 up 1 mouse 60 40",
    );
}

/// Drags the Element at `from` by `(dx, dy)` with the mouse.
fn drag(editor: &mut Editor, from: (f64, f64), (dx, dy): (f64, f64)) {
    let (x, y) = from;
    replay(
        editor,
        &format!(
            "1.0 down 1 mouse {x} {y}
             1.1 move 1 mouse {} {}
             1.2 up 1 mouse {} {}",
            x + dx,
            y + dy,
            x + dx,
            y + dy
        ),
    );
}

#[test]
fn undoing_a_drawn_shape_takes_it_away_and_redoing_brings_it_back() {
    let mut editor = Editor::new();
    draw_rectangle(&mut editor);
    assert_eq!(editor.element_count(), 1);
    let drawn = saved(&editor);

    editor.undo();
    assert_eq!(editor.element_count(), 0);

    editor.redo();
    assert_eq!(editor.element_count(), 1);
    let redrawn = saved(&editor);
    assert_eq!(redrawn.len(), 1);
    for key in ["id", "x", "y", "width", "height", "seed"] {
        assert_eq!(redrawn[0].get(key), drawn[0].get(key), "{key}");
    }
}

#[test]
fn undoing_a_move_puts_the_element_back_and_redoing_moves_it_again() {
    let mut editor = editor_with(&[RECTANGLE]);
    // The whole drag is one step: it passes through (120, 90) on the way.
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 115
         1.2 move 1 mouse 180 105
         1.3 up 1 mouse 180 105",
    );
    assert_eq!(position(&editor, "a"), (130.0, 80.0));

    editor.undo();
    assert_eq!(position(&editor, "a"), (100.0, 100.0));

    editor.redo();
    assert_eq!(position(&editor, "a"), (130.0, 80.0));
}

#[test]
fn steps_are_undone_latest_first_and_redone_in_the_order_they_were_done() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    drag(&mut editor, (180.0, 125.0), (0.0, 40.0));
    assert_eq!(position(&editor, "a"), (130.0, 140.0));

    editor.undo();
    assert_eq!(position(&editor, "a"), (130.0, 100.0));
    editor.undo();
    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    editor.redo();
    assert_eq!(position(&editor, "a"), (130.0, 100.0));
    editor.redo();
    assert_eq!(position(&editor, "a"), (130.0, 140.0));
}

#[test]
fn undo_and_redo_are_available_exactly_when_there_is_something_to_do() {
    let mut editor = Editor::new();
    assert!(!editor.can_undo() && !editor.can_redo());

    draw_rectangle(&mut editor);
    assert!(editor.can_undo() && !editor.can_redo());

    editor.undo();
    assert!(!editor.can_undo() && editor.can_redo());

    editor.redo();
    assert!(editor.can_undo() && !editor.can_redo());
}

#[test]
fn undo_and_redo_with_nothing_to_do_change_nothing() {
    let mut editor = editor_with(&[RECTANGLE]);
    let (saved_before, revision) = (editor.save(), editor.revision());

    editor.undo();
    editor.redo();

    assert_eq!(editor.save(), saved_before);
    assert_eq!(editor.revision(), revision);
}

#[test]
fn doing_something_new_after_undo_forgets_what_could_be_redone() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    editor.undo();
    assert!(editor.can_redo());

    drag(&mut editor, (150.0, 125.0), (0.0, 40.0));

    assert!(!editor.can_redo());
    assert_eq!(position(&editor, "a"), (100.0, 140.0));
}

#[test]
fn selecting_alone_is_not_a_step_and_keeps_what_can_be_redone() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    editor.undo();

    tap(&mut editor, "mouse", (350.0, 350.0), 5.0);
    tap(&mut editor, "mouse", (150.0, 125.0), 6.0);

    assert!(editor.can_redo());
}

#[test]
fn a_move_dragged_back_to_where_it_began_is_not_a_step() {
    let mut editor = editor_with(&[RECTANGLE]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 190 125
         1.2 move 1 mouse 150 125
         1.3 up 1 mouse 150 125",
    );

    assert!(!editor.can_undo());
}

#[test]
fn a_cancelled_drawing_or_move_is_not_a_step() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 cancel 1 mouse 60 40",
    );
    assert!(!editor.can_undo());

    let mut editor = editor_with(&[RECTANGLE]);
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 190 125
         1.2 cancel 1 mouse 190 125",
    );
    assert!(!editor.can_undo());
}

#[test]
fn a_tap_with_a_shape_tool_draws_nothing_and_is_not_a_step() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Ellipse);

    tap(&mut editor, "mouse", (50.0, 50.0), 0.0);

    assert!(!editor.can_undo());
}

#[test]
fn undo_and_redo_make_new_versions_of_the_elements_and_redraw() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    let version = |editor: &Editor| number(&saved(editor)[0], "version");
    let (moved, revision) = (version(&editor), editor.revision());

    editor.undo();
    assert!(version(&editor) > moved, "clients cache by version");
    assert_ne!(editor.revision(), revision);

    let (undone, revision) = (version(&editor), editor.revision());
    editor.redo();
    assert!(version(&editor) > undone);
    assert_ne!(editor.revision(), revision);
}

#[test]
fn undoing_a_creation_keeps_the_element_in_the_drawing_as_deleted() {
    let mut editor = Editor::new();
    draw_rectangle(&mut editor);

    editor.undo();

    let elements = saved(&editor);
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].get("isDeleted"), Some(&Value::Bool(true)));
    assert_eq!(editor.element_count(), 0);
}

#[test]
fn undoing_a_move_of_an_unselected_element_clears_the_selection_it_began_with() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    assert_eq!(editor.selection(), ["a"]);

    editor.undo();
    assert!(editor.selection().is_empty());

    editor.redo();
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn undoing_a_move_of_a_selected_element_keeps_the_selection() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = editor_with(&[RECTANGLE, other]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    drag(&mut editor, (150.0, 125.0), (0.0, 60.0));

    editor.undo();

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn undoing_the_creation_of_a_selected_element_leaves_nothing_selected() {
    let mut editor = Editor::new();
    draw_rectangle(&mut editor);
    // Select the new rectangle with a tap on its edge, then move it.
    tap(&mut editor, "mouse", (10.0, 25.0), 2.0);
    assert_eq!(editor.selection().len(), 1);
    drag(&mut editor, (10.0, 25.0), (50.0, 0.0));

    editor.undo();
    assert_eq!(editor.selection().len(), 1, "undoing the move");
    editor.undo();

    assert!(editor.selection().is_empty(), "the rectangle is gone");
}

#[test]
fn undoing_changes_what_clients_redraw_the_selection_frame_on() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    let overlay = editor.overlay_revision();

    editor.undo();

    assert_ne!(editor.overlay_revision(), overlay);
}

#[test]
fn nothing_is_undone_while_a_pointer_is_drawing_or_moving() {
    let mut editor = editor_with(&[RECTANGLE]);
    drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
    replay(
        &mut editor,
        "2.0 down 1 mouse 180 125
         2.1 move 1 mouse 200 125",
    );

    editor.undo();

    // The first move left it at (130, 100); this one is 20 further.
    assert_eq!(position(&editor, "a"), (150.0, 100.0), "the move goes on");
    replay(&mut editor, "2.2 up 1 mouse 200 125");
    assert_eq!(position(&editor, "a"), (150.0, 100.0));
    editor.undo();
    assert_eq!(position(&editor, "a"), (130.0, 100.0));
}

#[test]
fn the_history_is_not_saved_and_loading_a_drawing_starts_without_one() {
    let mut editor = Editor::new();
    draw_rectangle(&mut editor);
    editor.undo();
    draw_rectangle(&mut editor);
    editor.undo();
    let saved_bytes = editor.save();
    assert!(editor.can_redo());

    // The saved file knows nothing of undo or redo ...
    let text = String::from_utf8(saved_bytes.clone()).unwrap();
    assert!(!text.to_lowercase().contains("history"));
    assert!(!text.to_lowercase().contains("undo"));

    // ... so a new session starts with neither ...
    let mut reopened = Editor::new();
    reopened.load(&saved_bytes).unwrap();
    assert!(!reopened.can_undo() && !reopened.can_redo());
    reopened.undo();
    assert_eq!(reopened.save(), saved_bytes);

    // ... and so does the editor that loads another drawing.
    editor.load(&saved_bytes).unwrap();
    assert!(!editor.can_undo() && !editor.can_redo());
}

#[test]
fn drawing_several_shapes_is_undone_one_shape_at_a_time() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    editor.set_tool_lock(true);
    for y in [0.0, 100.0, 200.0] {
        replay(
            &mut editor,
            &format!(
                "0.0 down 1 mouse 0 {y}
                 0.1 move 1 mouse 50 {}
                 0.2 up 1 mouse 50 {}",
                y + 40.0,
                y + 40.0
            ),
        );
    }
    assert_eq!(editor.element_count(), 3);

    editor.undo();
    assert_eq!(editor.element_count(), 2);
    editor.undo();
    editor.undo();
    assert_eq!(editor.element_count(), 0);
    assert!(!editor.can_undo());
    editor.redo();
    editor.redo();
    assert_eq!(editor.element_count(), 2);
}

#[test]
fn undo_leaves_the_tool_and_the_viewport_alone() {
    let mut editor = Editor::new();
    draw_rectangle(&mut editor);
    editor.set_tool(Tool::Hand);
    editor.zoom_in();
    let zoom = editor.viewport().zoom();

    editor.undo();

    assert_eq!(editor.tool(), Tool::Hand);
    assert_eq!(editor.viewport().zoom(), zoom);
}
