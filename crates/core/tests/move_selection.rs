//! Moving the Selection by dragging (ADR-0006), tested at the Editor seam
//! with recorded pointer sequences (see `select.rs` for their format): pointer
//! events in, Selection and saved Drawing out. Expected positions are worked
//! out by hand in the comments.

use workingtitle_core::Editor;
use workingtitle_core::json::{Value, parse};

mod common;
use common::{editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50.
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// A member of the Element `id` as saved.
fn saved(editor: &Editor, id: &str, key: &str) -> f64 {
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    let element = drawing
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap();
    element.get(key).and_then(Value::as_f64).unwrap()
}

fn position(editor: &Editor, id: &str) -> (f64, f64) {
    (saved(editor, id, "x"), saved(editor, id, "y"))
}

#[test]
fn dragging_a_selected_element_moves_it() {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 120
         1.2 move 1 mouse 180 115
         1.3 up 1 mouse 180 115",
    );

    // The pointer travelled (+30, -10).
    assert_eq!(position(&editor, "a"), (130.0, 90.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn dragging_an_unselected_element_selects_and_moves_it() {
    let mut editor = editor_with(&[RECTANGLE]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 160 125
         1.2 up 1 mouse 160 125",
    );

    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(position(&editor, "a"), (110.0, 100.0));
}

#[test]
fn dragging_an_unselected_element_replaces_the_selection() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = editor_with(&[RECTANGLE, other]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    replay(
        &mut editor,
        "1.0 down 1 mouse 350 125
         1.1 move 1 mouse 350 175
         1.2 up 1 mouse 350 175",
    );

    assert_eq!(editor.selection(), ["b"]);
    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    assert_eq!(position(&editor, "b"), (300.0, 150.0));
}

#[test]
fn a_selected_hollow_element_is_dragged_by_its_inside() {
    let hollow = r#"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "backgroundColor":"transparent","roundness":null}"#;
    let mut editor = editor_with(&[hollow]);
    tap(&mut editor, "mouse", (100.0, 125.0), 0.0);

    // The middle is 25 from the nearest edge, far out of reach of the stroke.
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 150 155
         1.2 up 1 mouse 150 155",
    );

    assert_eq!(position(&editor, "a"), (100.0, 130.0));
}

#[test]
fn every_step_of_a_move_makes_a_new_version_of_the_element() {
    let mut editor = editor_with(&[RECTANGLE]);
    assert_eq!(saved(&editor, "a", "version"), 3.0);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 160 125
         1.2 move 1 mouse 170 125
         1.3 up 1 mouse 170 125",
    );

    // Two steps, two versions.
    assert_eq!(saved(&editor, "a", "version"), 5.0);
    assert_ne!(saved(&editor, "a", "versionNonce"), 7.0);
}

#[test]
fn a_move_changes_the_revisions_clients_redraw_on() {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    let (scene, overlay) = (editor.revision(), editor.overlay_revision());

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 160 125",
    );

    assert_ne!(editor.revision(), scene);
    assert_ne!(editor.overlay_revision(), overlay);
}

#[test]
fn nothing_moves_before_the_pointer_leaves_the_taps_slop() {
    let mut editor = editor_with(&[RECTANGLE]);
    let revision = editor.revision();

    // A finger may wander 10 points while tapping.
    replay(
        &mut editor,
        "1.0 down 1 finger 150 125
         1.1 move 1 finger 156 130
         1.2 up 1 finger 156 130",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    assert_eq!(saved(&editor, "a", "version"), 3.0);
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_move_follows_the_pointer_not_the_steps_it_took() {
    let mut editor = editor_with(&[RECTANGLE]);

    // Away, and back to where it went down.
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 250 225
         1.2 move 1 mouse 150 125
         1.3 up 1 mouse 150 125",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
}

#[test]
fn a_finger_drags_by_the_distance_it_travelled_from_where_it_landed() {
    let mut editor = editor_with(&[RECTANGLE]);

    // The first 20 points of the drag are not lost to the tap's slop.
    replay(
        &mut editor,
        "1.0 down 1 finger 150 125
         1.1 move 1 finger 170 125
         1.2 move 1 finger 190 125
         1.3 up 1 finger 190 125",
    );

    assert_eq!(position(&editor, "a"), (140.0, 100.0));
}

#[test]
fn a_drag_covers_scene_units_whatever_the_zoom() {
    let mut editor = editor_with(&[RECTANGLE]);
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);

    // The rectangle's middle (150, 125) is at (300, 250) points.
    replay(
        &mut editor,
        "1.0 down 1 mouse 300 250
         1.1 move 1 mouse 340 270
         1.2 up 1 mouse 340 270",
    );

    // 40 and 20 points are 20 and 10 Scene units.
    assert_eq!(position(&editor, "a"), (120.0, 110.0));
}

#[test]
fn dragging_empty_canvas_moves_nothing_and_keeps_the_selection() {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    let revision = editor.revision();

    replay(
        &mut editor,
        "1.0 down 1 mouse 500 500
         1.1 move 1 mouse 560 500
         1.2 up 1 mouse 560 500",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(editor.revision(), revision);
}

#[test]
fn only_the_selection_tool_moves() {
    use workingtitle_core::Tool;
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_tool(Tool::Hand);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 200 125
         1.2 up 1 mouse 200 125",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
}

#[test]
fn a_cancelled_drag_puts_the_element_back() {
    let mut editor = editor_with(&[RECTANGLE]);

    replay(
        &mut editor,
        "1.0 down 1 finger 150 125
         1.1 move 1 finger 200 125
         1.2 cancel 1 finger 200 125",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    // Versions only ever go up: the way back is a change too.
    assert_eq!(saved(&editor, "a", "version"), 5.0);
}

#[test]
fn a_second_finger_ends_the_move_and_puts_the_element_back() {
    let mut editor = editor_with(&[RECTANGLE]);

    replay(
        &mut editor,
        "1.0 down 1 finger 150 125
         1.1 move 1 finger 200 125
         1.2 down 2 finger 500 500
         1.3 move 2 finger 520 500
         1.4 move 1 finger 250 125
         1.5 up 2 finger 520 500
         1.6 up 1 finger 250 125",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
}

#[test]
fn a_tool_change_during_a_drag_puts_the_element_back() {
    use workingtitle_core::Tool;
    let mut editor = editor_with(&[RECTANGLE]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 200 125",
    );
    editor.set_tool(Tool::Hand);

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
}

#[test]
fn loading_a_drawing_during_a_drag_leaves_the_new_scene_alone() {
    let mut editor = editor_with(&[RECTANGLE]);
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 200 125",
    );
    let other = editor_with(&[RECTANGLE]).save();

    editor.load(&other).unwrap();
    replay(
        &mut editor,
        "1.2 move 1 mouse 300 125\n1.3 up 1 mouse 300 125",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
}

#[test]
fn a_label_moves_with_its_container() {
    let container = r#"{"id":"box","type":"rectangle","x":0,"y":0,"width":200,"height":100,
        "backgroundColor":"transparent","roundness":null,
        "boundElements":[{"type":"text","id":"label"}]}"#;
    let label = r#"{"id":"label","type":"text","x":50,"y":40,"width":100,"height":25,
        "text":"hi","originalText":"hi","fontSize":20,"containerId":"box"}"#;
    let other = r#"{"id":"other","type":"rectangle","x":500,"y":500,"width":10,"height":10,
        "backgroundColor":"transparent","roundness":null}"#;
    let mut editor = editor_with(&[container, label, other]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 0 50
         1.1 move 1 mouse 10 70
         1.2 up 1 mouse 10 70",
    );

    assert_eq!(position(&editor, "box"), (10.0, 20.0));
    assert_eq!(position(&editor, "label"), (60.0, 60.0));
    assert_eq!(position(&editor, "other"), (500.0, 500.0));
}

#[test]
fn a_line_moves_with_its_position_and_keeps_its_points() {
    let line = r#"{"id":"l","type":"line","x":10,"y":10,"width":100,"height":0,
        "points":[[0,0],[100,0]],"roundness":null}"#;
    let mut editor = editor_with(&[line]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 60 10
         1.1 move 1 mouse 60 40
         1.2 up 1 mouse 60 40",
    );

    // The points are relative to the position and stay as they are.
    assert_eq!(position(&editor, "l"), (10.0, 40.0));
    assert_eq!(saved(&editor, "l", "width"), 100.0);
}

#[test]
fn shift_keeps_a_move_to_the_axis_the_pointer_went_further_along() {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    // (+30, -10): along x. Then (+5, +40): along y, as on excalidraw.com,
    // where shift may be pressed and released during the drag.
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 180 115 shift",
    );
    assert_eq!(position(&editor, "a"), (130.0, 100.0));
    replay(&mut editor, "1.2 move 1 mouse 155 165 shift");
    assert_eq!(position(&editor, "a"), (100.0, 140.0));
    replay(
        &mut editor,
        "1.3 move 1 mouse 155 165
         1.4 up 1 mouse 155 165",
    );
    assert_eq!(position(&editor, "a"), (105.0, 140.0));
}

#[test]
fn a_drag_between_the_elements_of_a_multi_selection_moves_it() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = editor_with(&[RECTANGLE, other]);
    editor.select_all();

    // From (250, 125), between a (to x = 200) and b (from x = 300): inside
    // the box around both (Excalidraw's
    // `isHittingCommonBoundingBoxOfSelectedElements`), so it moves them
    // rather than box-selecting.
    replay(
        &mut editor,
        "1.0 down 1 mouse 250 125
         1.1 move 1 mouse 250 145
         1.2 up 1 mouse 250 145",
    );

    assert_eq!(editor.selection(), ["a", "b"]);
    assert_eq!(position(&editor, "a"), (100.0, 120.0));
    assert_eq!(position(&editor, "b"), (300.0, 120.0));
}

#[test]
fn a_drag_just_outside_the_box_of_a_multi_selection_box_selects() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = editor_with(&[RECTANGLE, other]);
    editor.select_all();

    // The box reaches from y = 100 to 150, plus 4 of padding and 8 of
    // threshold: 170 is outside it.
    replay(
        &mut editor,
        "1.0 down 1 mouse 250 170
         1.1 move 1 mouse 250 200
         1.2 up 1 mouse 250 200",
    );

    assert_eq!(position(&editor, "a"), (100.0, 100.0));
    assert_eq!(position(&editor, "b"), (300.0, 100.0));
}
