//! Arrows attach to shapes (ADR-0006), tested at the Editor seam with recorded
//! pointer sequences (see `select.rs` for their format): pointer events in,
//! saved JSON and overlay out. Expected coordinates are worked out by hand in
//! the comments.
//!
//! The data is Excalidraw's own (pinned commit `8bcdca4`): an arrow's
//! `startBinding` / `endBinding` name the shape with a `fixedPoint` (where on
//! the shape's box the arrow aims, as fractions of its width and height) and a
//! `mode`, and the shape lists the arrow in its `boundElements`. In `orbit`
//! mode the arrow stops short of the shape at a gap of 5 plus half the shape's
//! stroke width, so a shape with a stroke width of 2 has a gap of 6.

use pkm_base::json::Value;
use pkm_drawing::{Editor, PartKind, Tool};

use crate::common;
use common::{editor_with, ops, replay, saved_elements, saved_points, tap};

/// A rectangle from (100, 0) to (200, 100), without rounded corners.
const BOX: &str = r#"{"id":"box","type":"rectangle","x":100,"y":0,"width":100,"height":100,
    "strokeWidth":2,"roundness":null,"version":3,"versionNonce":7}"#;

/// A rectangle from (300, 0) to (400, 100).
const OTHER: &str = r#"{"id":"other","type":"rectangle","x":300,"y":0,"width":100,"height":100,
    "strokeWidth":2,"roundness":null}"#;

/// A drag of `kind` from `from` to `to` in two steps.
fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
    replay(
        editor,
        &format!(
            "0.00 down 1 {kind} {} {}\n0.05 move 1 {kind} {} {}\n0.10 move 1 {kind} {} {}\n0.15 up 1 {kind} {} {}",
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

/// An editor with `elements` and the arrow Tool, in which an arrow has been
/// dragged from `from` to `to`.
fn with_arrow(elements: &[&str], from: (f64, f64), to: (f64, f64)) -> Editor {
    let mut editor = editor_with(elements);
    editor.set_tool(Tool::Arrow);
    drag(&mut editor, "mouse", from, to);
    editor
}

fn element(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no element {id}"))
}

/// The arrow of a Scene that has one.
fn arrow(editor: &Editor) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("type").and_then(Value::as_str) == Some("arrow"))
        .expect("an arrow")
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap().to_owned()
}

/// What an arrow's `startBinding` or `endBinding` says: the id of the shape,
/// the mode and the fixed point; `None` for an end that is not bound.
fn binding(arrow: &Value, key: &str) -> Option<(String, String, (f64, f64))> {
    let binding = arrow.get(key).unwrap();
    if *binding == Value::Null {
        return None;
    }
    let fixed = binding.get("fixedPoint").and_then(Value::as_array).unwrap();
    Some((
        text(binding, "elementId"),
        text(binding, "mode"),
        (fixed[0].as_f64().unwrap(), fixed[1].as_f64().unwrap()),
    ))
}

/// The ids of the arrows a shape lists in its `boundElements`.
fn bound_arrows(shape: &Value) -> Vec<String> {
    shape
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|b| {
            assert_eq!(text(b, "type"), "arrow");
            text(b, "id")
        })
        .collect()
}

fn assert_near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} is not {expected}"
    );
}

fn assert_points(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert_near(a.0, e.0, &format!("{actual:?} x"));
        assert_near(a.1, e.1, &format!("{actual:?} y"));
    }
}

// --- Binding while drawing ---------------------------------------------------------

#[test]
fn an_arrow_drawn_onto_a_shape_binds_to_it() {
    // The arrow runs along y = 40 and ends inside the box at (120, 40), a
    // fifth of its width and two fifths of its height from its corner.
    let editor = with_arrow(&[BOX], (0.0, 40.0), (120.0, 40.0));

    let arrow = arrow(&editor);
    let (shape, mode, (fx, fy)) = binding(&arrow, "endBinding").expect("bound at its end");
    assert_eq!((shape.as_str(), mode.as_str()), ("box", "orbit"));
    assert_near(fx, 0.2, "fixedPoint x");
    assert_near(fy, 0.4, "fixedPoint y");
    assert_eq!(binding(&arrow, "startBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), [text(&arrow, "id")]);
    // It stops at the gap of 6 short of the box's left side.
    assert_points(&saved_points(&arrow), &[(0.0, 0.0), (94.0, 0.0)]);
}

#[test]
fn an_arrow_between_two_shapes_binds_at_both_ends() {
    // From (150, 40) inside the box to (320, 40) inside the other one. The
    // arrow aims at both of them along y = 40 and stops short of each, at
    // x = 206 of the box's right side (200 + 6) and at x = 294 of the other's
    // left side (300 - 6).
    let editor = with_arrow(&[BOX, OTHER], (150.0, 40.0), (320.0, 40.0));

    let arrow = arrow(&editor);
    let (start, start_mode, (sx, sy)) = binding(&arrow, "startBinding").expect("bound at start");
    let (end, end_mode, (ex, ey)) = binding(&arrow, "endBinding").expect("bound at end");
    assert_eq!(
        (start.as_str(), start_mode.as_str()),
        ("box", "orbit"),
        "start"
    );
    assert_eq!((end.as_str(), end_mode.as_str()), ("other", "orbit"), "end");
    assert_near(sx, 0.5001, "start x (a half is nudged)");
    assert_near(sy, 0.4, "start y");
    assert_near(ex, 0.2, "end x");
    assert_near(ey, 0.4, "end y");
    assert_eq!(bound_arrows(&element(&editor, "box")), [text(&arrow, "id")]);
    assert_eq!(
        bound_arrows(&element(&editor, "other")),
        [text(&arrow, "id")]
    );
    assert_near(arrow.get("x").and_then(Value::as_f64).unwrap(), 206.0, "x");
    assert_points(&saved_points(&arrow), &[(0.0, 0.0), (88.0, 0.0)]);
}

#[test]
fn an_arrow_ending_near_a_shape_binds_within_reach() {
    // 10 to the left of the box, which is within the 15 units of reach.
    let editor = with_arrow(&[BOX], (0.0, 40.0), (90.0, 40.0));

    let (shape, _, (fx, fy)) = binding(&arrow(&editor), "endBinding").expect("bound");
    assert_eq!(shape, "box");
    assert_near(fx, -0.1, "fixedPoint x lies outside the box");
    assert_near(fy, 0.4, "fixedPoint y");
}

#[test]
fn an_arrow_ending_far_from_shapes_stays_unbound() {
    // 30 to the left of the box.
    let editor = with_arrow(&[BOX], (0.0, 40.0), (70.0, 40.0));

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    assert_eq!(binding(&arrow, "startBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_points(&saved_points(&arrow), &[(0.0, 0.0), (70.0, 0.0)]);
}

#[test]
fn a_finger_reaches_further_to_a_shape_than_a_mouse() {
    // 20 to the left of the box: out of reach of a mouse, within the 22 view
    // points of a finger.
    let bound = |kind: &str| {
        let mut editor = editor_with(&[BOX]);
        editor.set_tool(Tool::Arrow);
        drag(&mut editor, kind, (0.0, 40.0), (80.0, 40.0));
        binding(&arrow(&editor), "endBinding").is_some()
    };

    assert!(!bound("mouse"));
    assert!(bound("finger"));
}

#[test]
fn a_line_never_binds() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Line);
    drag(&mut editor, "mouse", (0.0, 40.0), (120.0, 40.0));

    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    let line = &saved_elements(&editor)[1];
    assert_points(&saved_points(line), &[(0.0, 0.0), (120.0, 0.0)]);
}

#[test]
fn an_arrow_binds_to_ellipses_and_diamonds_at_their_outline() {
    // The ellipse is 100 wide and 100 high around (150, 50): a circle of
    // radius 50, gap 6. An arrow along y = 50 from the left stops at
    // x = 150 - 56 = 94.
    let ellipse = r#"{"id":"e","type":"ellipse","x":100,"y":0,"width":100,"height":100,
        "strokeWidth":2,"roundness":null}"#;
    let editor = with_arrow(&[ellipse], (0.0, 50.0), (140.0, 50.0));
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "e");
    let points = saved_points(&arrow(&editor));
    assert!((points[1].0 - 94.0).abs() < 1e-3, "{points:?}");

    // The diamond has its corners at (150, 0), (200, 50), (150, 100) and
    // (100, 50). The outline at a distance of 6 runs round its left corner
    // in an arc, so an arrow along y = 50 stops 6 short of it, at x = 94.
    let diamond = r#"{"id":"d","type":"diamond","x":100,"y":0,"width":100,"height":100,
        "strokeWidth":2,"roundness":null}"#;
    let editor = with_arrow(&[diamond], (0.0, 50.0), (140.0, 50.0));
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "d");
    let points = saved_points(&arrow(&editor));
    assert!((points[1].0 - 94.0).abs() < 1e-3, "{points:?}");
}

#[test]
fn the_last_point_of_an_arrow_placed_point_by_point_binds_and_is_tapped_where_shown() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);

    tap(&mut editor, "finger", (0.0, 40.0), 0.0);
    tap(&mut editor, "finger", (50.0, 100.0), 1.0);
    tap(&mut editor, "finger", (120.0, 40.0), 2.0);

    // The last point is aimed at (0.2, 0.4) of the box from the point before
    // it, (50, 100), and so stops at the offset left side after 26 of the 70
    // across.
    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "startBinding"), None);
    assert_eq!(binding(&arrow, "endBinding").unwrap().0, "box");
    assert_points(
        &shown(&editor),
        &[
            (0.0, 40.0),
            (50.0, 100.0),
            (94.0, 40.0 + 60.0 * 26.0 / 70.0),
        ],
    );
    assert_eq!(editor.tool(), Tool::Arrow, "still placing points");

    // Tapping the end where it is shown finishes the arrow.
    tap(&mut editor, "finger", (94.0, 62.0), 3.0);

    assert_eq!(editor.tool(), Tool::Selection);
    assert_eq!(saved_points(&self::arrow(&editor)).len(), 3);
}

#[test]
fn a_point_that_is_not_last_any_more_is_not_bound() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);
    tap(&mut editor, "finger", (0.0, 40.0), 0.0);
    tap(&mut editor, "finger", (120.0, 40.0), 1.0);
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");

    // Another point is placed beyond the box.
    tap(&mut editor, "finger", (250.0, 150.0), 2.0);

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_points(
        &saved_points(&arrow),
        &[(0.0, 0.0), (120.0, 0.0), (250.0, 110.0)],
    );
}

#[test]
fn a_hovering_mouse_highlights_the_shape_the_next_point_would_bind_to() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);
    tap(&mut editor, "mouse", (0.0, 40.0), 0.0);
    assert_eq!(highlight(&editor), None);

    replay(&mut editor, "1 move 9 mouse 120 40");

    assert_box(highlight(&editor), (94.0, -6.0, 206.0, 106.0));
}

// --- The highlight ------------------------------------------------------------

const HIGHLIGHT_COLOR: u32 = 0x1971_c2ff;

/// The box around the highlight in the overlay: the strokes that belong to no
/// Element and are blue, those of the shape an arrow end is over.
fn highlight(editor: &Editor) -> Option<(f64, f64, f64, f64)> {
    let overlay = editor.overlay();
    let coords: Vec<(f64, f64)> = overlay
        .parts()
        .iter()
        .filter(|p| p.kind == PartKind::Stroke as u32 && p.color == HIGHLIGHT_COLOR)
        .flat_map(|part| ops(&overlay, part))
        .flat_map(|(_, c)| {
            c.chunks(2)
                .map(|p| (f64::from(p[0]), f64::from(p[1])))
                .collect::<Vec<_>>()
        })
        .collect();
    if coords.is_empty() {
        return None;
    }
    let fold = |f: fn(f64, f64) -> f64, pick: fn(&(f64, f64)) -> f64, start: f64| {
        coords.iter().map(pick).fold(start, f)
    };
    Some((
        fold(f64::min, |p| p.0, f64::INFINITY),
        fold(f64::min, |p| p.1, f64::INFINITY),
        fold(f64::max, |p| p.0, f64::NEG_INFINITY),
        fold(f64::max, |p| p.1, f64::NEG_INFINITY),
    ))
}

fn assert_box(actual: Option<(f64, f64, f64, f64)>, expected: (f64, f64, f64, f64)) {
    let actual = actual.expect("a highlight");
    for (a, e) in [
        (actual.0, expected.0),
        (actual.1, expected.1),
        (actual.2, expected.2),
        (actual.3, expected.3),
    ] {
        assert!((a - e).abs() < 1e-3, "{actual:?} is not {expected:?}");
    }
}

#[test]
fn a_shape_an_arrow_end_is_over_is_highlighted_around_its_outline() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);
    replay(&mut editor, "0.00 down 1 mouse 0 40");
    assert_eq!(highlight(&editor), None, "nothing is near the start");
    let before = editor.overlay_revision();

    replay(&mut editor, "0.10 move 1 mouse 120 40");

    // Around the box at the gap of 6: from (94, -6) to (206, 106).
    assert_box(highlight(&editor), (94.0, -6.0, 206.0, 106.0));
    assert_ne!(editor.overlay_revision(), before);
}

#[test]
fn the_highlight_goes_when_the_end_leaves_the_shape_or_the_arrow_is_finished() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);
    replay(
        &mut editor,
        "0.00 down 1 mouse 0 40\n0.10 move 1 mouse 120 40",
    );
    assert!(highlight(&editor).is_some());
    let before = editor.overlay_revision();

    replay(&mut editor, "0.20 move 1 mouse 40 40");
    assert_eq!(highlight(&editor), None, "moved away");
    assert_ne!(editor.overlay_revision(), before);

    replay(
        &mut editor,
        "0.30 move 1 mouse 120 40\n0.40 up 1 mouse 120 40",
    );
    assert_eq!(highlight(&editor), None, "finished");
}

#[test]
fn the_highlight_follows_the_start_of_the_arrow_too() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);

    replay(&mut editor, "0.00 down 1 mouse 150 50");

    assert_box(highlight(&editor), (94.0, -6.0, 206.0, 106.0));
}

#[test]
fn a_turned_shape_is_highlighted_turned() {
    // The box turned a quarter turn about its middle (150, 50) is the same
    // box; a wide one turned is tall.
    let wide = r#"{"id":"w","type":"rectangle","x":100,"y":0,"width":200,"height":40,
        "angle":1.5707963267948966,"strokeWidth":2,"roundness":null}"#;
    let mut editor = editor_with(&[wide]);
    editor.set_tool(Tool::Arrow);

    // The middle of the box is (200, 20); turned it spans x from 180 to 220 and
    // y from -80 to 120.
    replay(&mut editor, "0.00 down 1 mouse 200 20");

    assert_box(highlight(&editor), (174.0, -86.0, 226.0, 126.0));
}

// --- Following the shapes ----------------------------------------------------------

/// The box with an arrow from (0, 40) bound to it at (120, 40): the arrow's
/// end is at (94, 40), its fixed point on the box is (0.2, 0.4).
fn bound_to_box() -> Editor {
    with_arrow(&[BOX, OTHER], (0.0, 40.0), (120.0, 40.0))
}

/// Where the arrow's points are in the Scene.
fn shown(editor: &Editor) -> Vec<(f64, f64)> {
    let arrow = arrow(editor);
    let (x, y) = (
        arrow.get("x").and_then(Value::as_f64).unwrap(),
        arrow.get("y").and_then(Value::as_f64).unwrap(),
    );
    saved_points(&arrow)
        .into_iter()
        .map(|(px, py)| (x + px, y + py))
        .collect()
}

#[test]
fn a_bound_arrow_follows_a_moved_shape() {
    let mut editor = bound_to_box();

    // Drag the box down by 20 by its right side.
    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );

    // The arrow still aims at (0.2, 0.4) of the box, now at (120, 60), from
    // its start at (0, 40): its line meets the box's offset left side, x = 94,
    // after 94 of the 120 across, at y = 40 + 20 * 94 / 120.
    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 40.0 + 20.0 * 94.0 / 120.0)],
    );
    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding").unwrap().0, "box");
    assert_eq!(bound_arrows(&element(&editor, "box")), [text(&arrow, "id")]);
}

#[test]
fn a_bound_arrow_follows_while_the_shape_is_dragged() {
    let mut editor = bound_to_box();

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90",
    );

    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 40.0 + 20.0 * 94.0 / 120.0)],
    );
}

#[test]
fn a_bound_start_follows_a_moved_shape_too() {
    // An arrow from the box to the other shape, both bound; the box moves up
    // by 30. The arrow starts at the box's fixed point (0.5001, 0.4), which is
    // now at (150.01, 10), and aims at the other's (320, 40).
    let mut editor = with_arrow(&[BOX, OTHER], (150.0, 40.0), (320.0, 40.0));

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 55\n1.2 move 1 mouse 200 40\n1.3 up 1 mouse 200 40",
    );

    let points = shown(&editor);
    // The start is on the box's offset right side, x = 206, and the end on the
    // other's offset left side, x = 294, both on the line between the fixed
    // points (150.01, 10) and (320, 40).
    let slope = 30.0 / (320.0 - 150.01);
    assert_points(
        &points,
        &[
            (206.0, 10.0 + slope * (206.0 - 150.01)),
            (294.0, 10.0 + slope * (294.0 - 150.01)),
        ],
    );
}

#[test]
fn an_arrow_moved_with_its_shape_stays_bound_and_keeps_its_shape() {
    let mut editor = bound_to_box();
    editor.select_all();

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 230 70\n1.2 move 1 mouse 250 70\n1.3 up 1 mouse 250 70",
    );

    assert_points(&shown(&editor), &[(50.0, 40.0), (144.0, 40.0)]);
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
}

#[test]
fn an_arrow_moved_without_its_shape_is_unbound() {
    let mut editor = bound_to_box();

    // Drag the arrow itself, by its shaft, up by 30.
    replay(
        &mut editor,
        "1.0 down 1 mouse 40 40\n1.1 move 1 mouse 40 25\n1.2 move 1 mouse 40 10\n1.3 up 1 mouse 40 10",
    );

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_points(&shown(&editor), &[(0.0, 10.0), (94.0, 10.0)]);
}

#[test]
fn cancelling_a_move_puts_shape_arrow_and_bindings_back() {
    let mut editor = bound_to_box();
    let before = editor.save();

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 cancel 1 mouse 200 90",
    );

    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
    assert_eq!(element(&editor, "box").get("y"), Some(&Value::Number(0.0)));
    assert_eq!(
        bound_arrows(&element(&editor, "box")).len(),
        1,
        "{}",
        String::from_utf8_lossy(&before)
    );
}

#[test]
fn undoing_a_move_puts_the_arrow_back_too() {
    let mut editor = bound_to_box();

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );
    editor.undo();

    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
}

#[test]
fn undoing_the_unbinding_move_of_an_arrow_binds_it_again() {
    let mut editor = bound_to_box();
    replay(
        &mut editor,
        "1.0 down 1 mouse 40 40\n1.1 move 1 mouse 40 25\n1.2 move 1 mouse 40 10\n1.3 up 1 mouse 40 10",
    );

    editor.undo();

    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
}

/// The box selected, by a tap on its right side, in a Scene with the arrow
/// of [`bound_to_box`]. The box's selection frame is 4 out, so its corner
/// handles are at (96, -4), (204, -4), (204, 104) and (96, 104), and the
/// rotation handle at (150, -20).
fn box_selected() -> Editor {
    let mut editor = bound_to_box();
    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.05 up 1 mouse 200 70",
    );
    assert_eq!(editor.selection(), ["box"]);
    editor
}

#[test]
fn a_bound_arrow_follows_a_resized_shape() {
    let mut editor = box_selected();

    // The south-east handle goes from (204, 104) to (204, 204): the box is
    // now 100 wide and 200 high, and the arrow's fixed point (0.2, 0.4) is at
    // (120, 80). From the start (0, 40) the arrow reaches x = 94 at
    // y = 40 + 40 * 94 / 120.
    replay(
        &mut editor,
        "2.0 down 1 mouse 204 104\n2.1 move 1 mouse 204 150\n2.2 move 1 mouse 204 204\n2.3 up 1 mouse 204 204",
    );

    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 40.0 + 40.0 * 94.0 / 120.0)],
    );
}

#[test]
fn a_bound_arrow_follows_a_turned_shape() {
    let mut editor = box_selected();

    // A quarter turn clockwise about the middle (150, 50) takes the fixed
    // point at (120, 40), which is (-30, -10) from it, to (150 + 10,
    // 50 - 30) = (160, 20). The turned box is the same square, so the arrow
    // from (0, 40) to it reaches the offset left side at x = 94,
    // y = 20 + 20 * 66 / 160.
    replay(
        &mut editor,
        "2.0 down 1 mouse 150 -20\n2.1 move 1 mouse 200 5\n2.2 move 1 mouse 250 50\n2.3 up 1 mouse 250 50",
    );

    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 20.0 + 20.0 * 66.0 / 160.0)],
    );
}

#[test]
fn undoing_a_resize_puts_the_arrow_back() {
    let mut editor = box_selected();
    replay(
        &mut editor,
        "2.0 down 1 mouse 204 104\n2.1 move 1 mouse 204 150\n2.2 move 1 mouse 204 204\n2.3 up 1 mouse 204 204",
    );

    editor.undo();

    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
}

// --- Editing the ends ----------------------------------------------------------------

/// The arrow of [`bound_to_box`] selected by a tap on its shaft: it shows
/// handles on its points, (0, 40) and (94, 40), and in the middle at (47, 40).
fn arrow_selected() -> Editor {
    let mut editor = bound_to_box();
    replay(&mut editor, "1.0 down 1 mouse 40 40\n1.05 up 1 mouse 40 40");
    assert_eq!(editor.selection(), [text(&arrow(&editor), "id")]);
    editor
}

/// Drags with a mouse from `from` to `to` in two steps, without lifting.
fn hold_drag(editor: &mut Editor, from: (f64, f64), to: (f64, f64)) {
    replay(
        editor,
        &format!(
            "2.0 down 1 mouse {} {}\n2.1 move 1 mouse {} {}\n2.2 move 1 mouse {} {}",
            from.0,
            from.1,
            (from.0 + to.0) / 2.0,
            (from.1 + to.1) / 2.0,
            to.0,
            to.1
        ),
    );
}

fn lift(editor: &mut Editor, at: (f64, f64)) {
    replay(editor, &format!("2.3 up 1 mouse {} {}", at.0, at.1));
}

#[test]
fn dragging_a_bound_end_away_from_its_shape_unbinds_it() {
    let mut editor = arrow_selected();

    hold_drag(&mut editor, (94.0, 40.0), (60.0, 150.0));
    lift(&mut editor, (60.0, 150.0));

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_points(&shown(&editor), &[(0.0, 40.0), (60.0, 150.0)]);
}

#[test]
fn dragging_a_bound_end_onto_another_shape_rebinds_it() {
    let mut editor = arrow_selected();

    // To (320, 40) inside the other shape: the arrow stops at its offset left
    // side, x = 294.
    hold_drag(&mut editor, (94.0, 40.0), (320.0, 40.0));
    lift(&mut editor, (320.0, 40.0));

    let arrow = arrow(&editor);
    let (shape, mode, (fx, fy)) = binding(&arrow, "endBinding").unwrap();
    assert_eq!((shape.as_str(), mode.as_str()), ("other", "orbit"));
    assert_near(fx, 0.2, "fixedPoint x");
    assert_near(fy, 0.4, "fixedPoint y");
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_eq!(
        bound_arrows(&element(&editor, "other")),
        [text(&arrow, "id")]
    );
    assert_points(&shown(&editor), &[(0.0, 40.0), (294.0, 40.0)]);
}

#[test]
fn dragging_a_bound_end_over_the_same_shape_aims_it_elsewhere() {
    let mut editor = arrow_selected();

    // To (150, 80), which is (0.5001, 0.8) of the box, as a half is nudged:
    // the arrow from (0, 40) to (150.01, 80) meets the offset left side after
    // 94 of the 150.01 across, at y = 40 + 40 * 94 / 150.01.
    hold_drag(&mut editor, (94.0, 40.0), (150.0, 80.0));
    lift(&mut editor, (150.0, 80.0));

    let (shape, _, (fx, fy)) = binding(&arrow(&editor), "endBinding").unwrap();
    assert_eq!(shape, "box");
    assert_near(fx, 0.5001, "fixedPoint x");
    assert_near(fy, 0.8, "fixedPoint y");
    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 40.0 + 40.0 * 94.0 / 150.01)],
    );
    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
}

#[test]
fn dragging_a_free_start_onto_a_shape_binds_it() {
    // An arrow from (0, 40) to (60, 40), selected; its start is dragged to
    // (120, 40) inside the box.
    let mut editor = with_arrow(&[BOX], (0.0, 40.0), (50.0, 40.0));
    replay(&mut editor, "1.0 down 1 mouse 25 40\n1.05 up 1 mouse 25 40");

    hold_drag(&mut editor, (0.0, 40.0), (120.0, 40.0));
    lift(&mut editor, (120.0, 40.0));

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    let (shape, _, (fx, fy)) = binding(&arrow, "startBinding").expect("bound at start");
    assert_eq!(shape, "box");
    assert_near(fx, 0.2, "fixedPoint x");
    assert_near(fy, 0.4, "fixedPoint y");
    // Aiming from inside the box at the end (50, 40): out through its left side.
    assert_points(&shown(&editor), &[(94.0, 40.0), (50.0, 40.0)]);
}

#[test]
fn the_shape_a_dragged_end_is_over_is_highlighted() {
    let mut editor = arrow_selected();
    assert_eq!(highlight(&editor), None);

    hold_drag(&mut editor, (94.0, 40.0), (320.0, 40.0));
    assert_box(highlight(&editor), (294.0, -6.0, 406.0, 106.0));

    replay(&mut editor, "2.25 move 1 mouse 250 150");
    assert_eq!(highlight(&editor), None);
}

#[test]
fn undoing_a_dragged_end_restores_its_binding() {
    let mut editor = arrow_selected();
    hold_drag(&mut editor, (94.0, 40.0), (320.0, 40.0));
    lift(&mut editor, (320.0, 40.0));

    editor.undo();

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding").unwrap().0, "box");
    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
    assert_eq!(
        bound_arrows(&element(&editor, "other")),
        Vec::<String>::new()
    );
    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
}

#[test]
fn cancelling_a_dragged_end_restores_its_binding() {
    let mut editor = arrow_selected();

    hold_drag(&mut editor, (94.0, 40.0), (60.0, 150.0));
    replay(&mut editor, "2.3 cancel 1 mouse 60 150");

    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
    assert_eq!(highlight(&editor), None);
}

// --- Files of excalidraw.com ----------------------------------------------------

/// The box and an arrow as excalidraw.com saves a bound arrow: it starts free
/// at (0, 40) and ends bound to the box at (0.2, 0.4) in `orbit` mode.
fn excalidraw_arrow(extra: &str, end_binding: &str) -> String {
    format!(
        r#"{{"id":"arrow","type":"arrow","x":0,"y":40,"width":94,"height":0,
        "points":[[0,0],[94,0]],"strokeWidth":2,"roundness":{{"type":2}},
        "startBinding":null,"endBinding":{end_binding},
        "startArrowhead":null,"endArrowhead":"arrow","elbowed":false{extra}}}"#
    )
}

const ORBIT_END: &str = r#"{"elementId":"box","mode":"orbit","fixedPoint":[0.2,0.4]}"#;

fn bound_box() -> String {
    BOX.replace(
        r#""versionNonce":7"#,
        r#""versionNonce":7,"boundElements":[{"id":"arrow","type":"arrow"}]"#,
    )
}

#[test]
fn a_file_with_bound_arrows_is_saved_as_it_was_loaded() {
    let arrow = excalidraw_arrow("", ORBIT_END);
    let editor = editor_with(&[&bound_box(), &arrow]);
    let saved = editor.save();

    let mut again = Editor::new();
    again.load(&saved).unwrap();

    assert_eq!(again.save(), saved);
    assert_eq!(
        binding(&self::arrow(&editor), "endBinding").unwrap().0,
        "box"
    );
}

#[test]
fn an_arrow_bound_in_a_file_follows_a_moved_shape() {
    let arrow = excalidraw_arrow(r#","customData":{"keep":"me"}"#, ORBIT_END);
    let mut editor = editor_with(&[&bound_box(), &arrow]);
    let keys = |e: &Value| -> Vec<String> {
        e.as_object()
            .unwrap()
            .iter()
            .map(|(k, _)| k.to_owned())
            .collect()
    };
    let keys_before = keys(&element(&editor, "arrow"));

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );

    assert_points(
        &shown(&editor),
        &[(0.0, 40.0), (94.0, 40.0 + 20.0 * 94.0 / 120.0)],
    );
    let moved = self::arrow(&editor);
    // What the core does not know of stays, in the order of the file.
    assert_eq!(
        moved.get("customData").and_then(|c| c.get("keep")),
        Some(&Value::String("me".into()))
    );
    assert_eq!(keys(&moved), keys_before);
    assert_eq!(binding(&moved, "endBinding").unwrap().2, (0.2, 0.4));
    assert_eq!(bound_arrows(&element(&editor, "box")), ["arrow"]);
}

#[test]
fn an_arrow_bound_inside_a_shape_ends_at_the_fixed_point() {
    let inside = r#"{"elementId":"box","mode":"inside","fixedPoint":[0.2,0.4]}"#;
    let arrow = excalidraw_arrow("", inside);
    let mut editor = editor_with(&[&bound_box(), &arrow]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );

    // Exactly at (0.2, 0.4) of the box, which is now at (100, 20).
    assert_points(&shown(&editor), &[(0.0, 40.0), (120.0, 60.0)]);
}

#[test]
fn a_binding_of_an_older_file_without_a_fixed_point_is_kept_as_it_is() {
    let legacy = r#"{"elementId":"box","focus":0.1,"gap":5}"#;
    let arrow = excalidraw_arrow("", legacy);
    let mut editor = editor_with(&[&bound_box(), &arrow]);
    let before = element(&editor, "arrow");

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );

    let after = element(&editor, "arrow");
    assert_eq!(after.get("endBinding"), before.get("endBinding"));
    assert_eq!(saved_points(&after), saved_points(&before));
}

#[test]
fn a_binding_to_a_missing_shape_does_no_harm() {
    let ghost = r#"{"elementId":"ghost","mode":"orbit","fixedPoint":[0.2,0.4]}"#;
    let arrow = excalidraw_arrow("", ghost);
    let mut editor = editor_with(&[BOX, &arrow]);

    replay(
        &mut editor,
        "1.0 down 1 mouse 200 70\n1.1 move 1 mouse 200 80\n1.2 move 1 mouse 200 90\n1.3 up 1 mouse 200 90",
    );

    assert_points(&shown(&editor), &[(0.0, 40.0), (94.0, 40.0)]);
}

// --- Deleting points -------------------------------------------------------------

#[test]
fn deleting_the_bound_end_of_an_arrow_unbinds_it() {
    let mut editor = editor_with(&[BOX]);
    editor.set_tool(Tool::Arrow);
    tap(&mut editor, "mouse", (0.0, 40.0), 0.0);
    tap(&mut editor, "mouse", (50.0, 100.0), 1.0);
    tap(&mut editor, "mouse", (120.0, 40.0), 2.0);
    tap(&mut editor, "mouse", (94.0, 62.0), 3.0);
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
    // Select the arrow by its first segment, then its end by its handle.
    tap(&mut editor, "mouse", (25.0, 70.0), 4.0);
    tap(&mut editor, "mouse", (94.0, 62.286), 5.0);
    assert_eq!(editor.selected_points(), [2]);

    editor.delete_selection();

    let arrow = arrow(&editor);
    assert_eq!(binding(&arrow, "endBinding"), None);
    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_points(&shown(&editor), &[(0.0, 40.0), (50.0, 100.0)]);
}

#[test]
fn deleting_an_arrow_by_its_points_frees_the_shapes() {
    let mut editor = bound_to_box();
    replay(&mut editor, "1.0 down 1 mouse 40 40\n1.05 up 1 mouse 40 40");
    tap(&mut editor, "mouse", (94.0, 40.0), 2.0);
    assert_eq!(editor.selected_points(), [1]);

    // An arrow keeps two points at least: it goes.
    editor.delete_selection();

    assert_eq!(bound_arrows(&element(&editor, "box")), Vec::<String>::new());
    assert_eq!(editor.selection(), Vec::<String>::new());

    editor.undo();

    assert_eq!(bound_arrows(&element(&editor, "box")).len(), 1);
    assert_eq!(binding(&arrow(&editor), "endBinding").unwrap().0, "box");
}

// --- Properties -----------------------------------------------------------------

#[test]
fn a_bound_arrow_follows_a_text_whose_font_size_changes() {
    struct FixedWidth;
    impl pkm_drawing::TextMeasurer for FixedWidth {
        fn line_width(&self, text: &str, font_size: f64) -> f64 {
            text.chars().count() as f64 * font_size / 2.0
        }
    }
    // Five characters of 10, one line of 25: from (100, 0) to (150, 25).
    let hello = r#"{"id":"t","type":"text","x":100,"y":0,"width":50,"height":25,
        "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
        "textAlign":"left","verticalAlign":"top","containerId":null,"autoResize":true,
        "boundElements":[{"id":"arrow","type":"arrow"}]}"#;
    let corner = r#"{"elementId":"t","mode":"inside","fixedPoint":[1,1]}"#;
    let arrow = excalidraw_arrow("", corner).replace("[94,0]", "[150,-15]");
    let mut editor = editor_with(&[hello, &arrow]);
    editor.set_text_measurer(std::sync::Arc::new(FixedWidth));
    tap(&mut editor, "mouse", (120.0, 5.0), 0.0);
    assert_eq!(editor.selection(), ["t"]);

    editor.set_property(pkm_drawing::Property::FontSize(
        pkm_drawing::FontSize::Large,
    ));

    // Five characters of 14, one line of 35: the corner is at (170, 35).
    assert_points(&shown(&editor), &[(0.0, 40.0), (170.0, 35.0)]);
    editor.undo();
    assert_points(&shown(&editor), &[(0.0, 40.0), (150.0, 25.0)]);
}
