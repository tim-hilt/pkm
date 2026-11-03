//! Editing the points of a selected line or arrow (ADR-0006), tested at the
//! Editor seam with recorded pointer sequences (see `select.rs` for their
//! format): pointer events in, saved JSON and overlay out. Expected
//! coordinates are worked out by hand in the comments.
//!
//! A line or arrow that is alone in the Selection shows a handle on each of
//! its points. Dragging one moves it, dragging the handle at the middle of a
//! segment adds a point there, tapping selects a point, and deleting removes
//! the selected ones.

use workingtitle_core::json::Value;
use workingtitle_core::{Editor, PartKind};

mod common;
use common::{editor_with, ops, replay, saved_elements, saved_points, tap};

const SELECTION_COLOR: u32 = 0x6965_dbff;
const WHITE: u32 = 0xffff_ffff;

/// A line `id` at `(x, y)` through `points`, sharp unless `curved`.
fn line_json(id: &str, (x, y): (f64, f64), points: &str, extra: &str) -> String {
    format!(
        r#"{{"id":"{id}","type":"line","x":{x},"y":{y},"width":100,"height":100,
        "points":{points},"roughness":0,"roundness":null,"strokeWidth":2{extra}}}"#
    )
}

/// The line `a` of three points: displayed at (50, 50), (150, 50) and
/// (150, 150), with the middles of its segments at (100, 50) and (150, 100).
fn corner() -> String {
    line_json("a", (50.0, 50.0), "[[0,0],[100,0],[100,100]]", "")
}

/// An editor with `elements`, the first of which is selected by tapping its
/// first segment with `kind`, which the handles are made for.
fn selected(kind: &str, elements: &[&str]) -> Editor {
    let mut editor = editor_with(elements);
    tap(&mut editor, kind, (100.0, 50.0), 0.0);
    assert_eq!(editor.selection(), ["a"], "tapping the line selects it");
    editor
}

type Position = (f64, f64);

/// A handle of the overlay: a square with its centre, side and fill colour.
#[derive(Debug, Clone, Copy)]
struct Square {
    x: f64,
    y: f64,
    side: f64,
    color: u32,
}

/// The overlay's filled squares that belong to no Element, which are the
/// handles.
fn handles(editor: &Editor) -> Vec<Square> {
    let overlay = editor.overlay();
    overlay
        .parts()
        .iter()
        .filter(|p| p.kind == PartKind::Fill as u32 && p.element == u32::MAX)
        .map(|part| {
            let coords: Vec<f32> = ops(&overlay, part)
                .into_iter()
                .flat_map(|(_, c)| c)
                .collect();
            let xs: Vec<f64> = coords.iter().step_by(2).map(|&c| f64::from(c)).collect();
            let ys: Vec<f64> = coords
                .iter()
                .skip(1)
                .step_by(2)
                .map(|&c| f64::from(c))
                .collect();
            let (x0, x1) = (min(&xs), max(&xs));
            let (y0, y1) = (min(&ys), max(&ys));
            Square {
                x: (x0 + x1) / 2.0,
                y: (y0 + y1) / 2.0,
                side: x1 - x0,
                color: part.color,
            }
        })
        .collect()
}

fn min(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// The handles of the points, which are bigger than those of segments'
/// middles, and the latter, each as `(x, y)` rounded to a hundredth.
fn point_and_middle_handles(editor: &Editor) -> (Vec<Position>, Vec<Position>) {
    let all = handles(editor);
    let biggest = all.iter().map(|s| s.side).fold(0.0, f64::max);
    let round = |s: &Square| ((s.x * 100.0).round() / 100.0, (s.y * 100.0).round() / 100.0);
    let points = all
        .iter()
        .filter(|s| s.side == biggest)
        .map(round)
        .collect();
    let middles = all.iter().filter(|s| s.side < biggest).map(round).collect();
    (points, middles)
}

/// A drag of `kind` from `from` to `to`, in steps, ending at `to`.
fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
    replay(
        editor,
        &format!(
            "1.00 down 1 {kind} {} {}\n1.05 move 1 {kind} {} {}\n1.10 move 1 {kind} {} {}\n1.15 up 1 {kind} {} {}",
            from.0,
            from.1,
            (from.0 + to.0) / 2.0,
            (from.1 + to.1) / 2.0,
            to.0,
            to.1,
            to.0,
            to.1
        ),
    );
}

fn line(editor: &Editor) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some("a"))
        .unwrap()
}

fn frame(element: &Value) -> (f64, f64, f64, f64) {
    let n = |key: &str| element.get(key).and_then(Value::as_f64).unwrap();
    (n("x"), n("y"), n("width"), n("height"))
}

// --- Handles -------------------------------------------------------------------

#[test]
fn a_selected_line_shows_a_handle_on_each_point_and_in_the_middle_of_each_segment() {
    let editor = selected("mouse", &[&corner()]);

    let (points, middles) = point_and_middle_handles(&editor);

    assert_eq!(points, [(50.0, 50.0), (150.0, 50.0), (150.0, 150.0)]);
    assert_eq!(middles, [(100.0, 50.0), (150.0, 100.0)]);
}

#[test]
fn the_handles_of_a_mouse_are_small_and_those_of_a_finger_touch_sized() {
    let long = line_json("a", (50.0, 50.0), "[[0,0],[200,0],[200,200]]", "");
    let mouse = selected("mouse", &[&long]);
    let finger = selected("finger", &[&long]);

    let sides = |editor: &Editor| {
        let mut sides: Vec<f64> = handles(editor).iter().map(|s| s.side).collect();
        sides.sort_by(f64::total_cmp);
        sides.dedup();
        sides
    };

    // Points are 8 view points wide for a mouse and 28 for a finger; the
    // handles in the middle of segments are half as big.
    assert_eq!(sides(&mouse), [4.0, 8.0]);
    assert_eq!(sides(&finger), [14.0, 28.0]);
}

#[test]
fn handles_are_as_big_on_screen_at_any_zoom() {
    let mut editor = selected("mouse", &[&corner()]);
    editor.zoom_in();
    editor.zoom_in();
    let zoom = editor.viewport().zoom();
    assert!(zoom > 1.0);

    let side = handles(&editor).iter().map(|s| s.side).fold(0.0, f64::max);

    assert!((side * zoom - 8.0).abs() < 1e-3, "{side} at {zoom}");
}

#[test]
fn a_segment_too_short_on_screen_has_no_handle_in_its_middle() {
    // Segments of 20 and 100: handles in the middle only for those that are
    // four point handles (32 view points) long or more.
    let line = line_json("a", (50.0, 50.0), "[[0,0],[20,0],[120,0]]", "");
    let editor = selected("mouse", &[&line]);

    let (_, middles) = point_and_middle_handles(&editor);

    assert_eq!(middles, [(120.0, 50.0)]);
}

#[test]
fn the_middle_of_a_curved_segment_is_on_the_curve() {
    // A curve through (0, 0), (100, 100) and (200, 0). Its first segment
    // is the cubic with controls (16.67, 16.67) and (66.67, 100), whose
    // middle, (p0 + 3 c1 + 3 c2 + p1) / 8, is (43.75, 56.25).
    let curved = r#"{"id":"a","type":"line","x":0,"y":0,"width":200,"height":100,
        "points":[[0,0],[100,100],[200,0]],"roughness":0,"roundness":{"type":2}}"#;
    let mut editor = editor_with(&[curved]);
    tap(&mut editor, "mouse", (43.75, 56.25), 0.0);
    assert_eq!(editor.selection(), ["a"]);

    let (_, middles) = point_and_middle_handles(&editor);

    assert_eq!(middles[0], (43.75, 56.25));
}

#[test]
fn only_a_line_that_is_alone_in_the_selection_has_point_handles() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":300,"width":20,"height":20,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = selected("mouse", &[&corner(), other]);
    assert!(!handles(&editor).is_empty());

    editor.select_all();

    // The box around everything has handles of its own: eight to resize and
    // one to rotate, none of them on the points.
    let (points, _) = point_and_middle_handles(&editor);
    assert_eq!(points.len(), 9);
    assert!(!points.contains(&(150.0, 50.0)));
}

#[test]
fn a_line_that_is_not_selected_has_no_handles() {
    let editor = editor_with(&[&corner()]);

    assert!(handles(&editor).is_empty());
}

#[test]
fn an_elbow_arrow_has_no_point_handles() {
    // The core keeps its points as they are, and would not route it again.
    let elbow = r#"{"id":"a","type":"arrow","x":50,"y":50,"width":100,"height":100,
        "points":[[0,0],[100,0],[100,100]],"roughness":0,"roundness":null,"elbowed":true}"#;
    let editor = selected("mouse", &[elbow]);

    assert!(handles(&editor).is_empty());
}

// --- Selecting points -----------------------------------------------------------

#[test]
fn tapping_a_point_selects_it_and_it_looks_selected() {
    let mut editor = selected("mouse", &[&corner()]);
    assert!(editor.selected_points().is_empty());

    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

    assert_eq!(editor.selected_points(), [1]);
    assert_eq!(editor.selection(), ["a"], "the line stays selected");
    let at = |x: f64, y: f64| {
        handles(&editor)
            .into_iter()
            .find(|s| (s.x - x).abs() < 1e-3 && (s.y - y).abs() < 1e-3 && s.side == 8.0)
            .unwrap()
    };
    assert_eq!(at(150.0, 50.0).color, SELECTION_COLOR);
    assert_eq!(at(50.0, 50.0).color, WHITE);
}

#[test]
fn tapping_another_point_selects_that_one_instead_and_shift_adds_to_them() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

    tap(&mut editor, "mouse", (150.0, 150.0), 2.0);
    assert_eq!(editor.selected_points(), [2]);

    replay(
        &mut editor,
        "3 down 1 mouse 50 50 shift\n3.05 up 1 mouse 50 50 shift",
    );
    assert_eq!(editor.selected_points(), [2, 0]);

    // Shift on a selected point takes it out again.
    replay(
        &mut editor,
        "4 down 1 mouse 150 150 shift\n4.05 up 1 mouse 150 150 shift",
    );
    assert_eq!(editor.selected_points(), [0]);
}

#[test]
fn tapping_a_handle_does_not_deselect_the_line_even_off_the_line() {
    let mut editor = selected("finger", &[&corner()]);

    // 15 away from the point at (50, 50), inside the finger's grip.
    tap(&mut editor, "finger", (35.0, 50.0), 1.0);

    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(editor.selected_points(), [0]);
}

#[test]
fn tapping_empty_canvas_clears_the_selection_and_its_points() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

    tap(&mut editor, "mouse", (500.0, 500.0), 2.0);

    assert!(editor.selection().is_empty());
    assert!(editor.selected_points().is_empty());
    // Selecting the line again starts without a point selected.
    tap(&mut editor, "mouse", (100.0, 50.0), 3.0);
    assert!(editor.selected_points().is_empty());
}

#[test]
fn tapping_the_middle_of_a_segment_changes_nothing() {
    let mut editor = selected("mouse", &[&corner()]);
    let before = editor.save();

    tap(&mut editor, "mouse", (150.0, 100.0), 1.0);

    assert_eq!(editor.save(), before);
    assert_eq!(editor.selection(), ["a"]);
    assert!(editor.selected_points().is_empty());
}

// --- Moving points ---------------------------------------------------------------

#[test]
fn dragging_a_point_moves_it() {
    let mut editor = selected("mouse", &[&corner()]);

    drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

    // The second point is now at (170, 80): (120, 30) from the line's
    // position, and the others have not moved.
    let moved = line(&editor);
    assert_eq!(
        saved_points(&moved),
        [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
    );
    assert_eq!(frame(&moved), (50.0, 50.0, 120.0, 100.0));
    assert_eq!(
        editor.selected_points(),
        [1],
        "the dragged point is selected"
    );
}

#[test]
fn dragging_the_first_point_moves_the_line_s_position_with_it() {
    let mut editor = selected("mouse", &[&corner()]);

    drag(&mut editor, "mouse", (50.0, 50.0), (30.0, 40.0));

    // The first point is at (30, 40) and so is the position of the line; the
    // others, still at (150, 50) and (150, 150), are relative to it.
    let moved = line(&editor);
    assert_eq!(
        saved_points(&moved),
        [(0.0, 0.0), (120.0, 10.0), (120.0, 110.0)]
    );
    assert_eq!(frame(&moved), (30.0, 40.0, 120.0, 110.0));
}

#[test]
fn a_point_follows_how_far_the_pointer_travelled_not_where_it_is() {
    let mut editor = selected("finger", &[&corner()]);

    // The finger took hold at (165, 60), 15 and 10 from the point at
    // (150, 50), and moves by (20, 30).
    drag(&mut editor, "finger", (165.0, 60.0), (185.0, 90.0));

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
    );
}

#[test]
fn dragging_moves_all_selected_points_together() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
    replay(
        &mut editor,
        "2 down 1 mouse 150 150 shift\n2.05 up 1 mouse 150 150 shift",
    );
    assert_eq!(editor.selected_points(), [1, 2]);

    drag(&mut editor, "mouse", (150.0, 150.0), (160.0, 170.0));

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (110.0, 20.0), (110.0, 120.0)]
    );
}

#[test]
fn dragging_the_line_between_its_points_still_moves_it() {
    let mut editor = selected("mouse", &[&corner()]);

    // (120, 50) is on the first segment, away from the handles.
    drag(&mut editor, "mouse", (120.0, 50.0), (130.0, 70.0));

    assert_eq!(frame(&line(&editor)), (60.0, 70.0, 100.0, 100.0));
    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
}

#[test]
fn a_dragged_point_updates_the_overlay() {
    let mut editor = selected("mouse", &[&corner()]);
    let before = editor.overlay_revision();

    drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

    assert_ne!(editor.overlay_revision(), before);
    let (points, _) = point_and_middle_handles(&editor);
    assert_eq!(points, [(50.0, 50.0), (170.0, 80.0), (150.0, 150.0)]);
}

#[test]
fn a_turned_line_keeps_its_other_points_where_they_are_when_one_is_moved() {
    // A line from (0, 0) to (100, 0), turned a quarter turn about the middle
    // of its points, (50, 0): it is displayed from (50, -50) to (50, 50).
    let turned = r#"{"id":"a","type":"line","x":0,"y":0,"width":100,"height":0,
        "angle":1.5707963267948966,"points":[[0,0],[100,0]],"roughness":0,"roundness":null}"#;
    let mut editor = editor_with(&[turned]);
    tap(&mut editor, "mouse", (50.0, 0.0), 0.0);
    let (points, _) = point_and_middle_handles(&editor);
    assert_eq!(points.len(), 2);
    assert!((points[0].0 - 50.0).abs() < 1e-2 && (points[0].1 + 50.0).abs() < 1e-2);
    assert!((points[1].0 - 50.0).abs() < 1e-2 && (points[1].1 - 50.0).abs() < 1e-2);

    // Drag the lower end down by 50.
    drag(&mut editor, "mouse", (50.0, 50.0), (50.0, 100.0));

    let (points, _) = point_and_middle_handles(&editor);
    assert!(
        (points[0].0 - 50.0).abs() < 1e-2 && (points[0].1 + 50.0).abs() < 1e-2,
        "{points:?}"
    );
    assert!(
        (points[1].0 - 50.0).abs() < 1e-2 && (points[1].1 - 100.0).abs() < 1e-2,
        "{points:?}"
    );
}

// --- Adding points ----------------------------------------------------------------

#[test]
fn dragging_the_middle_of_a_segment_adds_a_point_there() {
    let mut editor = selected("mouse", &[&corner()]);

    drag(&mut editor, "mouse", (100.0, 50.0), (100.0, 20.0));

    // A point between the first and second, now at (100, 20), which is
    // (50, -30) from the line's position.
    let edited = line(&editor);
    assert_eq!(
        saved_points(&edited),
        [(0.0, 0.0), (50.0, -30.0), (100.0, 0.0), (100.0, 100.0)]
    );
    assert_eq!(frame(&edited), (50.0, 50.0, 100.0, 130.0));
    assert_eq!(editor.selected_points(), [1], "the new point is selected");
}

#[test]
fn a_two_point_line_gets_a_third_point_by_dragging_its_middle() {
    let two = line_json("a", (0.0, 0.0), "[[0,0],[200,0]]", "");
    let mut editor = editor_with(&[&two]);
    tap(&mut editor, "finger", (50.0, 0.0), 0.0);

    drag(&mut editor, "finger", (100.0, 0.0), (100.0, 80.0));

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 80.0), (200.0, 0.0)]
    );
}

#[test]
fn undoing_an_added_point_takes_it_away_again() {
    let mut editor = selected("mouse", &[&corner()]);
    drag(&mut editor, "mouse", (100.0, 50.0), (100.0, 20.0));

    editor.undo();

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
    assert!(editor.selected_points().is_empty(), "the point is gone");
}

// --- Deleting points ----------------------------------------------------------------

#[test]
fn deleting_the_selected_point_removes_it() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);

    editor.delete_selected_points();

    assert_eq!(saved_points(&line(&editor)), [(0.0, 0.0), (100.0, 100.0)]);
    assert_eq!(frame(&line(&editor)), (50.0, 50.0, 100.0, 100.0));
    assert_eq!(
        editor.selected_points(),
        [0],
        "the point before is selected"
    );
}

#[test]
fn deleting_the_first_point_moves_the_position_to_the_next() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (50.0, 50.0), 1.0);

    editor.delete_selected_points();

    // The points left are at (150, 50) and (150, 150).
    let left = line(&editor);
    assert_eq!(saved_points(&left), [(0.0, 0.0), (0.0, 100.0)]);
    assert_eq!((frame(&left).0, frame(&left).1), (150.0, 50.0));
}

#[test]
fn deleting_leaves_a_line_of_two_points_or_nothing() {
    let two = line_json("a", (50.0, 50.0), "[[0,0],[100,0]]", "");
    let mut editor = editor_with(&[&two]);
    tap(&mut editor, "mouse", (100.0, 50.0), 0.0);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
    assert_eq!(editor.selected_points(), [1]);

    editor.delete_selected_points();

    // A line needs two points: with one gone there is nothing left.
    assert_eq!(editor.element_count(), 0);
    assert!(editor.selection().is_empty());
    assert!(editor.selected_points().is_empty());
}

#[test]
fn deleting_without_a_selected_point_does_nothing() {
    let mut editor = selected("mouse", &[&corner()]);
    let before = editor.save();

    editor.delete_selected_points();

    assert_eq!(editor.save(), before);
    assert!(!editor.can_undo());
}

#[test]
fn undoing_a_deleted_point_brings_it_back() {
    let mut editor = selected("mouse", &[&corner()]);
    tap(&mut editor, "mouse", (150.0, 50.0), 1.0);
    editor.delete_selected_points();

    editor.undo();

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
    editor.redo();
    assert_eq!(saved_points(&line(&editor)), [(0.0, 0.0), (100.0, 100.0)]);
}

// --- History and interruptions -----------------------------------------------------

#[test]
fn a_whole_drag_of_a_point_is_one_undo_step() {
    let mut editor = selected("mouse", &[&corner()]);

    drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));
    editor.undo();

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (120.0, 30.0), (100.0, 100.0)]
    );
}

#[test]
fn a_second_finger_while_dragging_a_point_puts_it_back() {
    let mut editor = selected("finger", &[&corner()]);

    replay(
        &mut editor,
        "1 down 1 finger 150 50\n1.1 move 1 finger 170 70\n1.2 move 1 finger 180 80\n1.3 down 2 finger 400 400",
    );

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn cancelling_a_drag_of_a_point_puts_it_back() {
    let mut editor = selected("pencil", &[&corner()]);

    replay(
        &mut editor,
        "1 down 1 pencil 150 50\n1.1 move 1 pencil 170 70\n1.2 move 1 pencil 180 80\n1.3 cancel 1 pencil 180 80",
    );

    assert_eq!(
        saved_points(&line(&editor)),
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
    );
    assert!(!editor.can_undo());
}

#[test]
fn undo_does_nothing_while_a_point_is_being_dragged() {
    let mut editor = selected("mouse", &[&corner()]);
    drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));
    replay(
        &mut editor,
        "2 down 1 mouse 150 150\n2.1 move 1 mouse 170 170\n2.2 move 1 mouse 180 180",
    );

    editor.undo();

    let (points, _) = point_and_middle_handles(&editor);
    assert_eq!(points[2], (180.0, 180.0));
}

#[test]
fn the_geometry_of_an_edited_line_changes_with_its_points() {
    let mut editor = selected("mouse", &[&corner()]);
    let before = editor.revision();

    drag(&mut editor, "mouse", (150.0, 50.0), (170.0, 80.0));

    assert_ne!(editor.revision(), before);
}
