//! Drawing lines and arrows (ADR-0006), tested at the Editor seam with
//! recorded pointer sequences (see `select.rs` for their format): pointer
//! events in, saved JSON, Tool and Selection out. Expected coordinates are
//! worked out by hand in the comments.
//!
//! Dragging draws a two-point line. A tap starts a multi-point one instead:
//! every further tap adds a point, and tapping the last point again (or
//! switching Tool, or for lines closing the loop) finishes it, as on
//! excalidraw.com.

use pkm_base::json::Value;
use pkm_drawing::{Editor, Tool};

use crate::common;
use common::{replay, saved_elements, saved_points, tap};

fn editor_with(tool: Tool) -> Editor {
    let mut editor = Editor::new();
    editor.set_tool(tool);
    editor
}

/// A drag of `kind` from `from` to `to` in two steps, with `modifiers`
/// (`"shift"` or empty) held while it moves.
fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64), modifiers: &str) {
    replay(
        editor,
        &format!(
            "0.00 down 1 {kind} {} {}\n0.05 move 1 {kind} {} {} {modifiers}\n0.10 move 1 {kind} {} {} {modifiers}\n0.15 up 1 {kind} {} {} {modifiers}",
            from.0,
            from.1,
            (from.0 + to.0) / 2.0,
            (from.1 + to.1) / 2.0,
            to.0,
            to.1,
            to.0,
            to.1,
        ),
    );
}

fn text<'a>(element: &'a Value, key: &str) -> &'a str {
    element.get(key).and_then(Value::as_str).unwrap()
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

// --- Dragging ----------------------------------------------------------------

#[test]
fn dragging_with_the_line_tool_draws_a_two_point_line() {
    for kind in ["mouse", "finger", "pencil"] {
        let mut editor = editor_with(Tool::Line);

        drag(&mut editor, kind, (10.0, 20.0), (110.0, 70.0), "");

        let elements = saved_elements(&editor);
        assert_eq!(elements.len(), 1, "{kind}");
        assert_eq!(text(&elements[0], "type"), "line");
        assert_eq!(
            (number(&elements[0], "x"), number(&elements[0], "y")),
            (10.0, 20.0)
        );
        assert_eq!(saved_points(&elements[0]), [(0.0, 0.0), (100.0, 50.0)]);
        assert_eq!(
            (
                number(&elements[0], "width"),
                number(&elements[0], "height")
            ),
            (100.0, 50.0)
        );
        assert_eq!(editor.element_count(), 1);
    }
}

#[test]
fn dragging_with_the_arrow_tool_draws_an_arrow_with_an_arrowhead() {
    let mut editor = editor_with(Tool::Arrow);

    drag(&mut editor, "mouse", (0.0, 0.0), (80.0, 0.0), "");

    let arrow = &saved_elements(&editor)[0];
    assert_eq!(text(arrow, "type"), "arrow");
    assert_eq!(saved_points(arrow), [(0.0, 0.0), (80.0, 0.0)]);
    assert_eq!(arrow.get("startArrowhead"), Some(&Value::Null));
    assert_eq!(text(arrow, "endArrowhead"), "arrow");
    assert_eq!(arrow.get("elbowed"), Some(&Value::Bool(false)));
}

#[test]
fn dragging_up_and_left_keeps_the_first_point_where_the_pointer_went_down() {
    let mut editor = editor_with(Tool::Line);

    drag(&mut editor, "mouse", (110.0, 70.0), (10.0, 20.0), "");

    let line = &saved_elements(&editor)[0];
    assert_eq!((number(line, "x"), number(line, "y")), (110.0, 70.0));
    assert_eq!(saved_points(line), [(0.0, 0.0), (-100.0, -50.0)]);
    // Sizes stay positive.
    assert_eq!(
        (number(line, "width"), number(line, "height")),
        (100.0, 50.0)
    );
}

#[test]
fn shift_snaps_the_angle_of_a_dragged_line_to_fifteen_degrees() {
    let mut editor = editor_with(Tool::Line);

    // From (0, 0) to (100, 10) the angle is 5.7 degrees, which snaps to 0;
    // the length, hypot(100, 10) = 100.4988, is kept.
    drag(&mut editor, "mouse", (0.0, 0.0), (100.0, 10.0), "shift");

    let points = saved_points(&saved_elements(&editor)[0]);
    assert!((points[1].0 - 100.4988).abs() < 1e-3, "{points:?}");
    assert!(points[1].1.abs() < 1e-9, "{points:?}");
}

#[test]
fn a_new_line_looks_like_one_excalidraw_creates() {
    let mut editor = editor_with(Tool::Line);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
    editor.set_tool(Tool::Arrow);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

    let elements = saved_elements(&editor);
    let keys = |e: &Value| -> Vec<String> {
        e.as_object()
            .unwrap()
            .iter()
            .map(|(k, _)| k.to_owned())
            .collect()
    };
    // `newLinearElement` and `newArrowElement`: the members of every Element,
    // then the points, bindings and arrowheads, and the one that tells the
    // two apart.
    let common = [
        "id",
        "type",
        "x",
        "y",
        "width",
        "height",
        "angle",
        "strokeColor",
        "backgroundColor",
        "fillStyle",
        "strokeWidth",
        "strokeStyle",
        "roughness",
        "opacity",
        "groupIds",
        "frameId",
        "index",
        "roundness",
        "seed",
        "version",
        "versionNonce",
        "isDeleted",
        "boundElements",
        "updated",
        "created",
        "link",
        "locked",
        "points",
        "startBinding",
        "endBinding",
        "startArrowhead",
        "endArrowhead",
    ];
    assert_eq!(keys(&elements[0]), [&common[..], &["polygon"]].concat());
    assert_eq!(keys(&elements[1]), [&common[..], &["elbowed"]].concat());
    // Lines and arrows are rounded unless the user chooses sharp.
    let roundness = elements[0].get("roundness").unwrap();
    assert_eq!(roundness.get("type").and_then(Value::as_f64), Some(2.0));
    assert_eq!(elements[0].get("polygon"), Some(&Value::Bool(false)));
}

#[test]
fn lines_get_their_own_seeds() {
    let mut editor = editor_with(Tool::Line);
    editor.set_tool_lock(true);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

    let elements = saved_elements(&editor);
    assert_ne!(number(&elements[0], "seed"), number(&elements[1], "seed"));
}

#[test]
fn a_dragged_line_is_drawn_and_survives_saving_and_loading() {
    let mut editor = editor_with(Tool::Arrow);
    drag(&mut editor, "mouse", (0.0, 0.0), (80.0, 40.0), "");
    assert!(!editor.geometry().parts().is_empty());

    let mut again = Editor::new();
    again.load(&editor.save()).unwrap();

    assert_eq!(again.save(), editor.save());
    assert_eq!(again.element_count(), 1);
}

// --- The Tool afterwards -------------------------------------------------------

#[test]
fn the_tool_returns_to_selection_after_a_dragged_line_unless_locked() {
    let mut editor = editor_with(Tool::Line);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
    assert_eq!(editor.tool(), Tool::Selection);

    editor.set_tool(Tool::Arrow);
    editor.set_tool_lock(true);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
    drag(&mut editor, "mouse", (0.0, 60.0), (50.0, 90.0), "");
    assert_eq!(editor.tool(), Tool::Arrow);
    assert_eq!(saved_elements(&editor).len(), 3);
}

// --- Tapping: multi-point lines -------------------------------------------------

#[test]
fn tapping_places_the_points_and_tapping_the_last_one_again_finishes() {
    let mut editor = editor_with(Tool::Line);

    tap(&mut editor, "mouse", (10.0, 10.0), 0.0);
    tap(&mut editor, "mouse", (60.0, 10.0), 1.0);
    tap(&mut editor, "mouse", (60.0, 60.0), 2.0);
    assert_eq!(editor.tool(), Tool::Line, "still placing points");
    tap(&mut editor, "mouse", (60.0, 60.0), 3.0);

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    assert_eq!(
        saved_points(&elements[0]),
        [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]
    );
    assert_eq!(editor.tool(), Tool::Selection);
    assert!(
        editor.selection().is_empty(),
        "finishing taps select nothing"
    );
}

#[test]
fn tapping_places_the_points_of_an_arrow_too() {
    let mut editor = editor_with(Tool::Arrow);

    tap(&mut editor, "finger", (0.0, 0.0), 0.0);
    tap(&mut editor, "finger", (100.0, 0.0), 1.0);
    tap(&mut editor, "finger", (100.0, 100.0), 2.0);
    tap(&mut editor, "finger", (100.0, 100.0), 3.0);

    let arrow = &saved_elements(&editor)[0];
    assert_eq!(text(arrow, "type"), "arrow");
    assert_eq!(
        saved_points(arrow),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
}

#[test]
fn the_zone_in_which_tapping_the_last_point_finishes_is_larger_for_fingers() {
    // A mouse tap 15 away from the last point places a point there...
    let mut mouse = editor_with(Tool::Line);
    tap(&mut mouse, "mouse", (0.0, 0.0), 0.0);
    tap(&mut mouse, "mouse", (100.0, 0.0), 1.0);
    tap(&mut mouse, "mouse", (115.0, 0.0), 2.0);
    assert_eq!(mouse.tool(), Tool::Line);
    mouse.set_tool(Tool::Hand);
    assert_eq!(
        saved_points(&saved_elements(&mouse)[0]),
        [(0.0, 0.0), (100.0, 0.0), (115.0, 0.0)]
    );

    // ...a finger's, which is a lot less precise, finishes the line.
    let mut finger = editor_with(Tool::Line);
    tap(&mut finger, "finger", (0.0, 0.0), 0.0);
    tap(&mut finger, "finger", (100.0, 0.0), 1.0);
    tap(&mut finger, "finger", (115.0, 0.0), 2.0);
    assert_eq!(finger.tool(), Tool::Selection);
    assert_eq!(
        saved_points(&saved_elements(&finger)[0]),
        [(0.0, 0.0), (100.0, 0.0)]
    );
}

#[test]
fn the_zone_follows_the_zoom_so_that_it_is_the_same_on_screen() {
    let mut editor = editor_with(Tool::Line);
    // Pinch to a zoom of 2: the finger zone is then 11 units in the Scene.
    replay(
        &mut editor,
        "0 down 1 finger 10 10\n0.1 down 2 finger 50 10\n0.2 move 2 finger 90 10\n0.3 up 2 finger 90 10\n0.4 up 1 finger 10 10",
    );
    let zoom = editor.viewport().zoom();
    assert!(zoom > 1.5, "{zoom}");

    tap(&mut editor, "finger", (0.0, 0.0), 1.0);
    tap(&mut editor, "finger", (100.0, 0.0), 2.0);
    // 30 view points from the last point, which is 15 units of the Scene:
    // outside the zone of 22 view points (11 units), though it would be
    // inside one of 22 units.
    tap(&mut editor, "finger", (100.0 + 30.0, 0.0), 3.0);
    assert_eq!(editor.tool(), Tool::Line);

    // 20 view points away (10 units) is inside.
    tap(&mut editor, "finger", (130.0 + 20.0, 0.0), 4.0);
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn closing_the_loop_of_a_line_finishes_it_as_a_polygon() {
    let mut editor = editor_with(Tool::Line);

    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    tap(&mut editor, "mouse", (100.0, 0.0), 1.0);
    tap(&mut editor, "mouse", (100.0, 100.0), 2.0);
    tap(&mut editor, "mouse", (3.0, 4.0), 3.0);

    let line = &saved_elements(&editor)[0];
    // The last point is put exactly onto the first.
    assert_eq!(
        saved_points(line),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 0.0)]
    );
    assert_eq!(line.get("polygon"), Some(&Value::Bool(true)));
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn an_arrow_does_not_close_a_loop() {
    let mut editor = editor_with(Tool::Arrow);

    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    tap(&mut editor, "mouse", (100.0, 0.0), 1.0);
    tap(&mut editor, "mouse", (100.0, 100.0), 2.0);
    tap(&mut editor, "mouse", (3.0, 4.0), 3.0);

    assert_eq!(editor.tool(), Tool::Arrow, "still placing points");
}

#[test]
fn switching_the_tool_finishes_the_line_without_the_point_being_placed() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    tap(&mut editor, "mouse", (50.0, 0.0), 1.0);
    // The mouse hovers: the next point follows it but is not placed yet.
    replay(&mut editor, "2 move 9 mouse 50 50");
    assert_eq!(
        saved_points(&saved_elements(&editor)[0]),
        [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]
    );

    editor.set_tool(Tool::Hand);

    assert_eq!(
        saved_points(&saved_elements(&editor)[0]),
        [(0.0, 0.0), (50.0, 0.0)]
    );
}

#[test]
fn a_line_with_a_single_point_is_nothing() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);

    editor.set_tool(Tool::Selection);

    assert!(saved_elements(&editor).is_empty());
    assert_eq!(editor.element_count(), 0);
    assert!(!editor.can_undo());
}

#[test]
fn hovering_makes_the_next_point_appear_only_beyond_the_zone_of_the_last() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);

    replay(&mut editor, "1 move 9 mouse 5 0");
    assert_eq!(saved_points(&saved_elements(&editor)[0]).len(), 1);

    replay(&mut editor, "1.1 move 9 mouse 30 0");
    assert_eq!(
        saved_points(&saved_elements(&editor)[0]),
        [(0.0, 0.0), (30.0, 0.0)]
    );

    replay(&mut editor, "1.2 move 9 mouse 2 0");
    assert_eq!(saved_points(&saved_elements(&editor)[0]).len(), 1);
}

#[test]
fn shift_snaps_the_point_being_placed() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    tap(&mut editor, "mouse", (100.0, 0.0), 1.0);

    // From (100, 0) towards (200, 10): 5.7 degrees, snapped to 0.
    replay(
        &mut editor,
        "2 down 1 mouse 200 10 shift\n2.1 up 1 mouse 200 10 shift",
    );
    editor.set_tool(Tool::Hand);

    let points = saved_points(&saved_elements(&editor)[0]);
    assert_eq!(points.len(), 3);
    // 100 along, then hypot(100, 10) = 100.4988 more.
    assert!((points[2].0 - 200.4988).abs() < 1e-3, "{points:?}");
    assert_eq!(points[2].1, 0.0);
}

// --- Interruptions ------------------------------------------------------------------

#[test]
fn a_second_finger_while_dragging_takes_the_line_back() {
    let mut editor = editor_with(Tool::Line);

    replay(
        &mut editor,
        "0 down 1 finger 10 10\n0.1 move 1 finger 80 10\n0.2 down 2 finger 200 200",
    );

    assert_eq!(editor.element_count(), 0);
    assert!(saved_elements(&editor).is_empty());
    assert_eq!(editor.tool(), Tool::Line);
}

#[test]
fn cancelling_the_pointer_while_dragging_takes_the_line_back() {
    let mut editor = editor_with(Tool::Arrow);

    replay(
        &mut editor,
        "0 down 1 pencil 10 10\n0.1 move 1 pencil 80 10\n0.2 cancel 1 pencil 80 10",
    );

    assert!(saved_elements(&editor).is_empty());
    assert!(!editor.can_undo());
}

#[test]
fn two_finger_gestures_between_taps_place_no_points() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "finger", (0.0, 0.0), 0.0);

    // Two fingers land and lift again without going anywhere.
    replay(
        &mut editor,
        "1 down 1 finger 100 100\n1.1 down 2 finger 200 100\n1.2 up 2 finger 200 100\n1.3 up 1 finger 100 100",
    );
    assert_eq!(editor.tool(), Tool::Line);
    tap(&mut editor, "finger", (50.0, 0.0), 2.0);
    editor.set_tool(Tool::Hand);

    let line = &saved_elements(&editor)[0];
    assert_eq!(number(line, "x"), 0.0);
    assert_eq!(saved_points(line), [(0.0, 0.0), (50.0, 0.0)]);
}

// --- History -----------------------------------------------------------------------

#[test]
fn a_drawn_line_is_one_undo_step() {
    let mut editor = editor_with(Tool::Line);
    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
    assert!(editor.can_undo());

    editor.undo();
    assert_eq!(editor.element_count(), 0);
    editor.redo();
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn a_multi_point_line_is_one_undo_step_too() {
    let mut editor = editor_with(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    tap(&mut editor, "mouse", (50.0, 0.0), 1.0);
    tap(&mut editor, "mouse", (50.0, 50.0), 2.0);
    tap(&mut editor, "mouse", (50.0, 50.0), 3.0);

    editor.undo();

    assert_eq!(editor.element_count(), 0);
    assert!(!editor.can_undo());
}

#[test]
fn undoing_while_placing_points_finishes_the_line_first() {
    let mut editor = editor_with(Tool::Line);
    drag(&mut editor, "mouse", (0.0, 100.0), (50.0, 100.0), "");
    editor.set_tool(Tool::Line);
    tap(&mut editor, "mouse", (0.0, 0.0), 1.0);
    tap(&mut editor, "mouse", (50.0, 0.0), 2.0);

    editor.undo();

    // The line being placed was finished, which made it the latest step, and
    // is now undone; the first one stays.
    assert_eq!(editor.element_count(), 1);
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn drawing_lines_does_not_change_the_selection() {
    let mut editor = editor_with(Tool::Line);

    drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

    assert!(editor.selection().is_empty());
}
