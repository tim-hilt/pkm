//! Resizing and rotating the Selection by its transform handles (ADR-0006),
//! tested at the Editor seam with recorded pointer sequences (see `select.rs`
//! for their format): pointer events in, saved Drawing and overlay out.
//! Expected values are worked out by hand in the comments, from Excalidraw's
//! rules for resizing and rotating (`resizeElements.ts`).

use workingtitle_core::json::{Value, parse};
use workingtitle_core::{Editor, PartKind};

mod common;
use common::{FixedWidth, editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50. Its selection frame is 4
/// units out, so its handles are centred on (96, 96), (204, 96), (204, 154)
/// and (96, 154), and the rotation handle 16 above the middle of the top
/// side, at (150, 80).
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

/// `(x, y, width, height)` of the Element `id`.
fn frame(editor: &Editor, id: &str) -> (f64, f64, f64, f64) {
    (
        saved(editor, id, "x"),
        saved(editor, id, "y"),
        saved(editor, id, "width"),
        saved(editor, id, "height"),
    )
}

/// Presses at `from`, drags to `to` and lifts there.
fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
    let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    replay(
        editor,
        &format!(
            "1.0 down 1 {kind} {} {}
             1.1 move 1 {kind} {} {}
             1.2 move 1 {kind} {} {}
             1.3 up 1 {kind} {} {}",
            from.0, from.1, mid.0, mid.1, to.0, to.1, to.0, to.1
        ),
    );
}

/// An editor with the rectangle selected.
fn selected_rectangle() -> Editor {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn dragging_the_south_east_handle_resizes_the_selected_element() {
    let mut editor = selected_rectangle();

    // The pointer takes hold 4 units beyond the corner and keeps that
    // distance, so the corner ends at (250, 180).
    drag(&mut editor, "mouse", (204.0, 154.0), (254.0, 184.0));

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 80.0));
    assert_eq!(editor.selection(), ["a"]);
}

/// Presses at `from`, drags to `to` and lifts there, with `shift` held.
fn held_drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64), shift: bool) {
    let hold = if shift { " shift" } else { "" };
    let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    replay(
        editor,
        &format!(
            "2.0 down 1 {kind} {} {}{hold}
             2.1 move 1 {kind} {} {}{hold}
             2.2 move 1 {kind} {} {}{hold}
             2.3 up 1 {kind} {} {}{hold}",
            from.0, from.1, mid.0, mid.1, to.0, to.1, to.0, to.1
        ),
    );
}

/// The centre of each handle of [`RECTANGLE`] for a mouse at zoom 1: just
/// beyond the selection frame, which keeps 4 units from the box, plus
/// Excalidraw's 2 of spacing; the rotation handle 16 further above.
const NE: (f64, f64) = (206.0, 94.0);
const SE: (f64, f64) = (206.0, 156.0);
const W: (f64, f64) = (94.0, 125.0);
const S: (f64, f64) = (150.0, 156.0);
const E: (f64, f64) = (206.0, 125.0);
const ROTATION: (f64, f64) = (150.0, 78.0);

#[test]
fn dragging_the_north_east_handle_keeps_the_opposite_corner() {
    let mut editor = selected_rectangle();

    // The pointer takes hold 6 beyond the corner and keeps that distance,
    // so the dragged corner ends at (210, 70) and the box grows left and
    // down of it.
    held_drag(&mut editor, "mouse", NE, (216.0, 64.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 70.0, 110.0, 80.0));
}

#[test]
fn dragging_the_west_handle_keeps_the_opposite_side() {
    let mut editor = selected_rectangle();

    held_drag(&mut editor, "mouse", W, (74.0, 145.0), false);

    assert_eq!(frame(&editor, "a"), (80.0, 100.0, 120.0, 50.0));
}

#[test]
fn dragging_a_side_handle_across_the_box_mirrors_the_element() {
    let mut editor = selected_rectangle();

    // The south side is dragged up past the north one: the Element is
    // mirrored, and the dragged side ends where the pointer is.
    held_drag(&mut editor, "mouse", S, (150.0, 46.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 40.0, 100.0, 60.0));
}

#[test]
fn shift_keeps_the_ratio_of_the_sides_from_a_corner() {
    let mut editor = selected_rectangle();

    // The pointer asks for 150 x 60; the wider side wins, so both sides
    // grow by half again.
    held_drag(&mut editor, "mouse", SE, (256.0, 166.0), true);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 75.0));
}

#[test]
fn shift_keeps_the_ratio_of_the_sides_from_a_side() {
    let mut editor = selected_rectangle();

    // The east side is dragged out by half; the height follows it, and the
    // Element stays centred on the side it keeps.
    held_drag(&mut editor, "mouse", E, (256.0, 145.0), true);

    assert_eq!(frame(&editor, "a"), (100.0, 87.5, 150.0, 75.0));
}

#[test]
fn undoing_puts_the_resized_element_back() {
    let mut editor = selected_rectangle();

    held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);
    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));

    editor.undo();
    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn cancelling_a_resize_puts_the_element_back() {
    let mut editor = selected_rectangle();

    replay(
        &mut editor,
        "3.0 down 1 mouse 206 156
         3.1 move 1 mouse 256 206
         3.2 cancel 1 mouse 256 206",
    );

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_tap_on_a_handle_leaves_the_selection_alone() {
    let mut editor = selected_rectangle();

    // A press on the rotation handle that never drags is neither a
    // selection nor a clearing of it.
    tap(&mut editor, "mouse", ROTATION, 4.0);

    assert_eq!(editor.selection(), ["a"]);
}

/// The `key` of the Element `id` as saved, within a tolerance.
fn nearly(editor: &Editor, id: &str, key: &str, expected: f64) {
    let actual = saved(editor, id, key);
    assert!(
        (actual - expected).abs() < 1e-9,
        "{key} is {actual}, expected {expected}"
    );
}

#[test]
fn dragging_the_rotation_handle_turns_the_element() {
    let mut editor = selected_rectangle();

    // The pointer ends level with the middle of the box, to its right: a
    // quarter turn clockwise, and the box stays where it is.
    held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

    nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn shift_snaps_the_rotation_to_steps_of_15_degrees() {
    let mut editor = selected_rectangle();

    // The pointer is 140° clockwise from straight above the middle
    // (150, 125); snapped, the turn becomes the nearest step of 15°, 135°.
    let (sin, cos) = 140f64.to_radians().sin_cos();
    let to = (150.0 + 100.0 * sin, 125.0 - 100.0 * cos);
    held_drag(&mut editor, "mouse", ROTATION, to, true);

    nearly(&editor, "a", "angle", 3.0 * std::f64::consts::FRAC_PI_4);
}

/// A filled rectangle at (300, 150), 100 x 100.
const BIGGER_RECTANGLE: &str = r##"{"id":"b","type":"rectangle","x":300,"y":150,"width":100,"height":100,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// An editor with both rectangles selected.
fn selected_rectangles() -> Editor {
    let mut editor = editor_with(&[RECTANGLE, BIGGER_RECTANGLE]);
    editor.select_all();
    assert_eq!(editor.selection(), ["a", "b"]);
    editor
}

#[test]
fn dragging_a_handle_resizes_the_selection_as_a_unit() {
    let mut editor = selected_rectangles();

    // The box of the Selection is 100..400 by 100..250. The pointer asks
    // for 450 x 300, so everything is scaled by 1.5 across and 2 down from
    // the corner opposite the handle.
    held_drag(&mut editor, "mouse", (406.0, 256.0), (556.0, 406.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
    assert_eq!(frame(&editor, "b"), (400.0, 200.0, 150.0, 200.0));
}

#[test]
fn dragging_a_side_handle_stretches_the_selection_across_only() {
    let mut editor = selected_rectangles();

    held_drag(&mut editor, "mouse", (406.0, 175.0), (556.0, 175.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 50.0));
    assert_eq!(frame(&editor, "b"), (400.0, 150.0, 150.0, 100.0));
}

#[test]
fn dragging_a_handle_across_the_selection_mirrors_it() {
    let mut editor = selected_rectangles();

    // The west handle is dragged 600 to the right, past the east side of
    // the box: what was left of it is now right of it, at the same size.
    held_drag(&mut editor, "mouse", (94.0, 175.0), (694.0, 175.0), false);

    assert_eq!(frame(&editor, "a"), (600.0, 100.0, 100.0, 50.0));
    assert_eq!(frame(&editor, "b"), (400.0, 150.0, 100.0, 100.0));
}

#[test]
fn dragging_the_rotation_handle_turns_the_selection_as_a_unit() {
    let mut editor = selected_rectangles();

    // The pointer ends level with the middle of the box, to its right: a
    // quarter turn about the middle of the box, each Element keeping its
    // own angle on top of it.
    held_drag(&mut editor, "mouse", (250.0, 78.0), (450.0, 175.0), false);

    nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
    assert_eq!(frame(&editor, "a"), (250.0, 50.0, 100.0, 50.0));
    nearly(&editor, "b", "angle", std::f64::consts::FRAC_PI_2);
    assert_eq!(frame(&editor, "b"), (175.0, 225.0, 100.0, 100.0));
}

// --- Each kind of Element follows its own rules -----------------------------

/// The points of the Element `id` as saved.
fn saved_points(editor: &Editor, id: &str) -> Vec<(f64, f64)> {
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    let element = drawing
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap();
    element
        .get("points")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|p| {
            let p = p.as_array().unwrap();
            (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())
        })
        .collect()
}

/// A freedraw stroke from (100, 100) to (200, 150).
const STROKE: &str = r##"{"id":"a","type":"freedraw","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"pressures":[0.5,0.5],"version":3,"versionNonce":7}"##;

#[test]
fn resizing_a_freedraw_stroke_rescales_its_points() {
    let mut editor = editor_with(&[STROKE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
    assert_eq!(saved_points(&editor, "a"), [(0.0, 0.0), (150.0, 100.0)]);
}

/// A text of one line of `text` in the size `font_size`, at (100, 100),
/// wide enough that the handles on its sides are offered.
fn text_element(text: &str, font_size: f64) -> String {
    let chars = text.chars().count() as f64;
    format!(
        r#"{{"id":"a","type":"text","x":100,"y":100,"width":{},"height":{},
        "fontSize":{font_size},"lineHeight":1.25,"text":"{text}","originalText":"{text}","version":3,"versionNonce":7}}"#,
        chars * font_size / 2.0,
        font_size * 1.25
    )
}

fn selected_text(text: &str, font_size: f64) -> Editor {
    let mut editor = editor_with(&[&text_element(text, font_size)]);
    editor.set_text_measurer(std::sync::Arc::new(FixedWidth));
    tap(&mut editor, "mouse", (115.0, 112.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn resizing_a_text_from_a_corner_scales_its_font() {
    let mut editor = selected_text("abc", 20.0);

    // The pointer asks for 80 x 75; a corner keeps the ratio of the sides,
    // so the text becomes 90 wide and three times as tall and its font
    // three times as big.
    held_drag(&mut editor, "mouse", (136.0, 131.0), (186.0, 181.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 90.0, 75.0));
    nearly(&editor, "a", "fontSize", 60.0);
}

#[test]
fn resizing_a_text_from_a_side_wraps_its_lines_again() {
    let mut editor = selected_text("ab cd", 40.0);

    // The east side is dragged in to 50 from the left edge: the two words
    // no longer fit on one line, and the text takes two lines of the same
    // font.
    held_drag(&mut editor, "mouse", (206.0, 125.0), (156.0, 125.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 50.0, 100.0));
    nearly(&editor, "a", "fontSize", 40.0);
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    let text = drawing.get("elements").and_then(Value::as_array).unwrap()[0]
        .get("text")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    assert_eq!(text, "ab\ncd");
}

/// An image at (100, 100), 100 x 50.
const IMAGE: &str = r##"{"id":"a","type":"image","x":100,"y":100,"width":100,"height":50,
    "fileId":"f","version":3,"versionNonce":7}"##;

fn selected_image() -> Editor {
    let mut editor = editor_with(&[IMAGE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn an_image_keeps_the_ratio_of_its_sides_of_itself() {
    let mut editor = selected_image();

    // The pointer asks for 150 x 60; the image keeps its sides' ratio
    // without Shift, so both grow by half.
    held_drag(&mut editor, "mouse", SE, (256.0, 166.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 75.0));
}

#[test]
fn shift_frees_an_image_to_lose_the_ratio_of_its_sides() {
    let mut editor = selected_image();

    held_drag(&mut editor, "mouse", SE, (256.0, 166.0), true);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 60.0));
}

#[test]
fn mirroring_an_image_mirrors_which_way_it_faces() {
    let mut editor = selected_image();

    // The corner is dragged past the opposite one: the image is mirrored,
    // and its `scale` says so, as excalidraw.com reads it.
    held_drag(&mut editor, "mouse", SE, (46.0, 46.0), true);

    assert_eq!(frame(&editor, "a"), (40.0, 40.0, 60.0, 60.0));
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    let scale = drawing.get("elements").and_then(Value::as_array).unwrap()[0]
        .get("scale")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|f| f.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(scale, [-1.0, -1.0]);
}

/// A rectangle at (100, 100), 100 x 50, with the label `hi` in its middle.
fn labelled_rectangle() -> Editor {
    let mut editor = editor_with(&[
        r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
        r##"{"id":"t","type":"text","containerId":"a","x":105,"y":112.5,"width":20,"height":25,
            "fontSize":20,"lineHeight":1.25,"textAlign":"center","verticalAlign":"middle",
            "text":"hi","originalText":"hi","version":3,"versionNonce":7}"##,
    ]);
    editor.set_text_measurer(std::sync::Arc::new(FixedWidth));
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn the_label_of_a_resized_container_sits_in_it_again() {
    let mut editor = labelled_rectangle();

    held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
    // The label keeps its font and is centred in the container it sits in,
    // as Excalidraw's own tests expect of a resized container.
    let (width, height) = (saved(&editor, "t", "width"), saved(&editor, "t", "height"));
    assert_eq!((width, height), (20.0, 25.0));
    nearly(&editor, "t", "x", 165.0);
    nearly(&editor, "t", "y", 137.5);
    nearly(&editor, "t", "fontSize", 20.0);
}

#[test]
fn the_label_of_a_turned_container_turns_with_it() {
    let mut editor = labelled_rectangle();

    held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

    nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
    nearly(&editor, "t", "angle", std::f64::consts::FRAC_PI_2);
    // The label stays in the middle of the container.
    nearly(&editor, "t", "x", 140.0);
    nearly(&editor, "t", "y", 112.5);
}

/// An arrow from (100, 100) to (200, 150).
const ARROW: &str = r##"{"id":"a","type":"arrow","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"version":3,"versionNonce":7}"##;

#[test]
fn a_line_or_arrow_alone_has_handles_on_its_points_instead_of_transform_handles() {
    let mut editor = editor_with(&[ARROW]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    // A lone line or arrow is edited at its points (see `edit_points.rs`): a
    // handle on each of its two points and one in the middle of its segment,
    // but no eight to resize and one to rotate.
    let overlay = editor.overlay();
    let handles = overlay
        .parts()
        .iter()
        .filter(|p| p.element == u32::MAX && p.kind == PartKind::Fill as u32)
        .count();
    assert_eq!(handles, 3);
}

#[test]
fn a_finger_takes_hold_of_a_handle_from_farther_away() {
    let mut editor = selected_rectangle();

    // Apple's 44 points of grip for a finger reach 22 beyond the handle's
    // centre; a mouse's 8 points would not take hold here at all.
    held_drag(&mut editor, "finger", (226.0, 176.0), (266.0, 216.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 140.0, 90.0));
}

/// The sides of the squares the overlay draws for the handles.
fn handle_sizes(editor: &Editor) -> Vec<f64> {
    let overlay = editor.overlay();
    overlay
        .parts()
        .iter()
        .filter(|p| p.element == u32::MAX && p.kind == PartKind::Fill as u32)
        .map(|p| {
            let coords: Vec<_> = common::ops(&overlay, p)
                .iter()
                .flat_map(|(_, c)| c.iter().copied())
                .collect();
            let (mut least_x, mut greatest_x) = (f32::INFINITY, f32::NEG_INFINITY);
            let (mut least_y, mut greatest_y) = (f32::INFINITY, f32::NEG_INFINITY);
            for pair in coords.chunks(2) {
                least_x = least_x.min(pair[0]);
                greatest_x = greatest_x.max(pair[0]);
                least_y = least_y.min(pair[1]);
                greatest_y = greatest_y.max(pair[1]);
            }
            (greatest_x - least_x).max(greatest_y - least_y) as f64
        })
        .collect()
}

#[test]
fn the_handles_are_larger_for_a_finger_than_for_a_mouse() {
    let mut editor = editor_with(&[RECTANGLE]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    let mouse_sizes = handle_sizes(&editor);
    assert!(
        mouse_sizes.iter().all(|size| (size - 8.0).abs() < 1e-3),
        "{mouse_sizes:?}"
    );

    tap(&mut editor, "finger", (150.0, 125.0), 1.0);
    let finger_sizes = handle_sizes(&editor);
    assert!(
        finger_sizes.iter().all(|size| (size - 28.0).abs() < 1e-3),
        "{finger_sizes:?}"
    );
}

/// A text of five characters, 100 wide and one line high, at (300, 150).
const SELECTED_TEXT_IN_MULTIPLE: &str = r##"{"id":"b","type":"text","x":300,"y":150,"width":100,"height":25,
    "fontSize":20,"lineHeight":1.25,"text":"abcde","originalText":"abcde","version":3,"versionNonce":7}"##;

#[test]
fn the_selection_of_a_text_keeps_the_ratio_of_the_sides() {
    let mut editor = editor_with(&[RECTANGLE, SELECTED_TEXT_IN_MULTIPLE]);
    editor.select_all();

    // The box of the Selection is 100..400 by 100..175. The pointer asks
    // for 450 x 150; a text in the Selection keeps the ratio of the sides,
    // so both take the taller scale, of 2.
    held_drag(&mut editor, "mouse", (406.0, 181.0), (556.0, 256.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 200.0, 100.0));
    assert_eq!(frame(&editor, "b"), (500.0, 200.0, 200.0, 50.0));
    nearly(&editor, "b", "fontSize", 40.0);
}

#[test]
fn resizing_a_turned_element_follows_the_pointer_in_its_own_frame() {
    // The rectangle of the first test, a quarter turn clockwise about its
    // middle: its south-east handle sits at (119, 181), below and left of
    // its middle on the screen.
    let turned = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "angle":1.5707963267948966,"strokeWidth":2,"backgroundColor":"#ffc9c9",
        "roundness":null,"version":3,"versionNonce":7}"##;
    let mut editor = editor_with(&[turned]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    // The handle is dragged down the screen, which is across in the
    // Element's own frame: the box grows across by the 30 the pointer
    // travelled, and the Element keeps its turn.
    held_drag(&mut editor, "mouse", (119.0, 181.0), (119.0, 211.0), false);

    assert_eq!(frame(&editor, "a"), (85.0, 115.0, 130.0, 50.0));
    nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
}

#[test]
fn a_diamond_and_an_ellipse_resize_like_a_rectangle() {
    let mut editor = editor_with(&[
        r##"{"id":"a","type":"diamond","x":100,"y":100,"width":100,"height":50,
            "version":3,"versionNonce":7}"##,
        r##"{"id":"b","type":"ellipse","x":300,"y":100,"width":100,"height":50,
            "version":3,"versionNonce":7}"##,
    ]);
    editor.select_all();

    // The box of the Selection is 100..400 by 100..150; the pointer asks
    // for 450 x 150, so everything is scaled by 1.5 across and 3 down.
    held_drag(&mut editor, "mouse", (406.0, 156.0), (556.0, 256.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 150.0));
    assert_eq!(frame(&editor, "b"), (400.0, 100.0, 150.0, 150.0));
}

/// An arrow from (100, 100) to (200, 150), selected with the big rectangle.
const ARROW_IN_MULTIPLE: &str = r##"{"id":"a","type":"arrow","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"version":3,"versionNonce":7}"##;

#[test]
fn resizing_the_selection_rescales_the_points_of_lines_and_arrows() {
    let mut editor = editor_with(&[ARROW_IN_MULTIPLE, BIGGER_RECTANGLE]);
    editor.select_all();

    // The box of the Selection is 100..400 by 100..250; the pointer asks
    // for 450 x 300. The arrow's points scale with it.
    held_drag(&mut editor, "mouse", (406.0, 256.0), (556.0, 406.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
    assert_eq!(saved_points(&editor, "a"), [(0.0, 0.0), (150.0, 100.0)]);
    assert_eq!(frame(&editor, "b"), (400.0, 200.0, 150.0, 200.0));
}

// --- Turned Elements, strokes, groups, the least font, from the centre -------

/// `offset` from `centre`, turned by `angle` (clockwise on the screen).
fn turned_from(centre: (f64, f64), angle: f64, offset: (f64, f64)) -> (f64, f64) {
    let (sin, cos) = angle.sin_cos();
    (
        centre.0 + offset.0 * cos - offset.1 * sin,
        centre.1 + offset.0 * sin + offset.1 * cos,
    )
}

#[test]
fn resizing_a_turned_element_from_any_corner_keeps_the_opposite_corner() {
    // The rectangle of the first test, turned by half a radian about its
    // middle (150, 125).
    let angle = 0.5;
    let turned = format!(
        r##"{{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "angle":{angle},"strokeWidth":2,"backgroundColor":"#ffc9c9",
        "roundness":null,"version":3,"versionNonce":7}}"##
    );
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let mut editor = editor_with(&[&turned]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        let opposite = turned_from((150.0, 125.0), angle, (-sx * 50.0, -sy * 25.0));

        // The handle is 6 beyond the corner in the Element's own frame, and
        // is dragged 15 out across and 10 out down in that frame.
        let handle = turned_from((150.0, 125.0), angle, (sx * 56.0, sy * 31.0));
        let delta = turned_from((0.0, 0.0), angle, (sx * 15.0, sy * 10.0));
        let to = (handle.0 + delta.0, handle.1 + delta.1);
        held_drag(&mut editor, "mouse", handle, to, false);

        let (x, y, width, height) = frame(&editor, "a");
        assert!(
            (width - 115.0).abs() < 1e-9 && (height - 60.0).abs() < 1e-9,
            "handle ({sx}, {sy}): {width} x {height}"
        );
        let centre = (x + width / 2.0, y + height / 2.0);
        let kept = turned_from(centre, angle, (-sx * width / 2.0, -sy * height / 2.0));
        assert!(
            (kept.0 - opposite.0).abs() < 1e-9 && (kept.1 - opposite.1).abs() < 1e-9,
            "handle ({sx}, {sy}): the opposite corner moved from {opposite:?} to {kept:?}"
        );
    }
}

#[test]
fn resizing_a_stroke_that_reaches_back_past_its_start_keeps_the_opposite_corner() {
    // A stroke from (100, 100) back to (50, 80) and on to (150, 130): its
    // box is 50..150 by 80..130, left of and above its first point.
    let mut editor = editor_with(&[
        r##"{"id":"a","type":"freedraw","x":100,"y":100,"width":100,"height":50,
        "points":[[0,0],[-50,-20],[50,30]],"pressures":[0.5,0.5,0.5],"version":3,"versionNonce":7}"##,
    ]);
    editor.select_all();

    // The south-east corner goes from (150, 130) to (200, 180); the north-west
    // one stays at (50, 80). The points scale by 1.5 across and 2 down and
    // keep their least coordinates, so the first point is still (100, 100).
    held_drag(&mut editor, "mouse", (156.0, 136.0), (206.0, 186.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
    assert_eq!(
        saved_points(&editor, "a"),
        [(25.0, 20.0), (-50.0, -20.0), (100.0, 80.0)]
    );
}

#[test]
fn a_group_keeps_the_ratio_of_its_sides() {
    let grouped = |element: &str| element.replacen("\"x\"", "\"groupIds\":[\"g\"],\"x\"", 1);
    let mut editor = editor_with(&[&grouped(RECTANGLE), &grouped(BIGGER_RECTANGLE)]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a", "b"]);

    // The box is 100..400 by 100..250; the east side is dragged out by half
    // again, and a group grows down by as much as across (Excalidraw's
    // `isInGroup`), about the middle of the west side.
    held_drag(&mut editor, "mouse", (406.0, 175.0), (556.0, 175.0), false);

    assert_eq!(frame(&editor, "a"), (100.0, 62.5, 150.0, 75.0));
    assert_eq!(frame(&editor, "b"), (400.0, 137.5, 150.0, 150.0));
}

#[test]
fn a_selection_whose_text_would_get_too_small_a_font_is_not_resized() {
    // A text of a 2-unit font, 5 x 2.5 at (300, 150), selected with the
    // rectangle: the box is 100..305 by 100..152.5.
    let mut editor = editor_with(&[
        RECTANGLE,
        r##"{"id":"b","type":"text","x":300,"y":150,"width":5,"height":2.5,
        "fontSize":2,"lineHeight":1.25,"text":"abcde","originalText":"abcde","version":3,"versionNonce":7}"##,
    ]);
    editor.select_all();

    // Shrunk to two fifths, the text's font would be 0.8, under Excalidraw's
    // least of 1: nothing is resized, the rectangle neither.
    replay(
        &mut editor,
        "4.0 down 1 mouse 311 158.5
         4.1 move 1 mouse 188 127
         4.2 up 1 mouse 188 127",
    );

    assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
    nearly(&editor, "b", "fontSize", 2.0);
    assert!(!editor.can_undo());
}

#[test]
fn alt_resizes_an_element_about_its_middle() {
    let mut editor = selected_rectangle();

    // The corner is dragged out by 25 x 15; with Alt the opposite corner
    // moves out as far, so the Element grows by twice that about its middle.
    replay(
        &mut editor,
        "5.0 down 1 mouse 206 156 alt
         5.1 move 1 mouse 231 171 alt
         5.2 up 1 mouse 231 171 alt",
    );

    assert_eq!(frame(&editor, "a"), (75.0, 85.0, 150.0, 80.0));
}

#[test]
fn alt_resizes_the_selection_about_its_middle() {
    let mut editor = selected_rectangles();

    // The box is 100..400 by 100..250, its middle (250, 175). The corner is
    // dragged to (475, 287.5): with Alt, twice as far from the middle, so
    // everything is scaled by 1.5 about the middle.
    replay(
        &mut editor,
        "5.0 down 1 mouse 406 256 alt
         5.1 move 1 mouse 481 293.5 alt
         5.2 up 1 mouse 481 293.5 alt",
    );

    assert_eq!(frame(&editor, "a"), (25.0, 62.5, 150.0, 75.0));
    assert_eq!(frame(&editor, "b"), (325.0, 137.5, 150.0, 150.0));
}

#[test]
fn a_rotation_is_one_undo_step() {
    let mut editor = selected_rectangle();
    held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

    editor.undo();
    nearly(&editor, "a", "angle", 0.0);
    assert_eq!(editor.selection(), ["a"]);

    editor.redo();
    nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
    assert!(!editor.can_redo());
}
