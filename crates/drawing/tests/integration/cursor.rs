//! The mouse cursor over the Canvas (ADR-0006), tested at the Editor seam:
//! pointer events and the hovered view point in, `Editor::cursor` out. The
//! cursor tells what pressing there would do, as on excalidraw.com.

use pkm_drawing::{Cursor, Editor, Tool};

use crate::common;
use common::{editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50. Its handles are centred on
/// (96, 96), (204, 96), (204, 154) and (96, 154), the side handles on
/// (150, 96), (150, 154), (96, 125) and (204, 125), the rotation handle on
/// (150, 80).
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// The same rectangle turned by a quarter.
const TURNED: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "angle":1.5707963267948966,"strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,
    "version":3,"versionNonce":7}"##;

/// A filled rectangle at (300, 150), 100 x 100.
const OTHER: &str = r##"{"id":"b","type":"rectangle","x":300,"y":150,"width":100,"height":100,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// A line from (100, 300) to (200, 300).
const LINE: &str = r##"{"id":"l","type":"line","x":100,"y":300,"width":100,"height":0,
    "points":[[0,0],[100,0]],"strokeWidth":2,"roundness":null,"version":3,"versionNonce":7}"##;

fn selected(elements: &[&str], at: (f64, f64)) -> Editor {
    let mut editor = editor_with(elements);
    tap(&mut editor, "mouse", at, 0.0);
    assert!(!editor.selection().is_empty());
    editor
}

#[test]
fn empty_canvas_shows_the_arrow() {
    let editor = editor_with(&[RECTANGLE]);
    assert_eq!(editor.cursor(20.0, 20.0), Cursor::Arrow);
}

#[test]
fn an_element_that_a_drag_would_move_shows_the_move_cursor() {
    let editor = editor_with(&[RECTANGLE]);
    assert_eq!(editor.cursor(150.0, 125.0), Cursor::Move);
}

#[test]
fn the_inside_of_a_selected_hollow_element_shows_the_move_cursor() {
    let diamond = r##"{"id":"d","type":"diamond","x":100,"y":100,"width":140,"height":120,
        "strokeWidth":2,"roundness":null,"version":3,"versionNonce":7}"##;
    let mut editor = editor_with(&[diamond]);
    assert_eq!(editor.cursor(170.0, 160.0), Cursor::Arrow);
    tap(&mut editor, "mouse", (205.0, 130.0), 0.0);
    assert_eq!(editor.selection(), ["d"]);
    assert_eq!(editor.cursor(170.0, 160.0), Cursor::Move);
}

#[test]
fn a_locked_element_is_not_moved_and_shows_the_arrow() {
    let locked = RECTANGLE.replace(r#""id":"a""#, r#""id":"a","locked":true"#);
    let editor = editor_with(&[&locked]);
    assert_eq!(editor.cursor(150.0, 125.0), Cursor::Arrow);
}

#[test]
fn the_resize_handles_show_the_direction_they_resize_in() {
    let editor = selected(&[RECTANGLE], (150.0, 125.0));
    assert_eq!(editor.cursor(96.0, 96.0), Cursor::ResizeNorthWestSouthEast);
    assert_eq!(
        editor.cursor(204.0, 154.0),
        Cursor::ResizeNorthWestSouthEast
    );
    assert_eq!(editor.cursor(204.0, 96.0), Cursor::ResizeNorthEastSouthWest);
    assert_eq!(editor.cursor(96.0, 154.0), Cursor::ResizeNorthEastSouthWest);
    assert_eq!(editor.cursor(150.0, 96.0), Cursor::ResizeNorthSouth);
    assert_eq!(editor.cursor(150.0, 154.0), Cursor::ResizeNorthSouth);
    assert_eq!(editor.cursor(96.0, 125.0), Cursor::ResizeEastWest);
    assert_eq!(editor.cursor(204.0, 125.0), Cursor::ResizeEastWest);
}

#[test]
fn the_resize_cursors_turn_with_the_element() {
    // Turned by a quarter about its centre (150, 125): its top side runs
    // down the right, so the north handle is at (179, 125).
    let editor = selected(&[TURNED], (150.0, 125.0));
    assert_eq!(editor.cursor(179.0, 125.0), Cursor::ResizeEastWest);
    assert_eq!(editor.cursor(150.0, 71.0), Cursor::ResizeNorthSouth);
}

#[test]
fn the_rotation_handle_shows_the_open_hand() {
    let editor = selected(&[RECTANGLE], (150.0, 125.0));
    assert_eq!(editor.cursor(150.0, 80.0), Cursor::Grab);
}

#[test]
fn the_cursor_of_a_resize_stays_while_it_drags() {
    let mut editor = selected(&[RECTANGLE], (150.0, 125.0));
    replay(
        &mut editor,
        "1.0 down 1 mouse 204 154
         1.1 move 1 mouse 300 300",
    );
    assert_eq!(
        editor.cursor(300.0, 300.0),
        Cursor::ResizeNorthWestSouthEast
    );
}

#[test]
fn rotating_shows_the_closed_hand() {
    let mut editor = selected(&[RECTANGLE], (150.0, 125.0));
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 80
         1.1 move 1 mouse 300 125",
    );
    assert_eq!(editor.cursor(300.0, 125.0), Cursor::Grabbing);
}

#[test]
fn moving_shows_the_move_cursor_wherever_the_pointer_is() {
    let mut editor = editor_with(&[RECTANGLE]);
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 600 600",
    );
    assert_eq!(editor.cursor(600.0, 600.0), Cursor::Move);
}

#[test]
fn the_box_around_several_selected_elements_shows_the_move_cursor() {
    let mut editor = editor_with(&[RECTANGLE, OTHER]);
    editor.select_all();
    // Between the two rectangles, on neither.
    assert_eq!(editor.cursor(250.0, 140.0), Cursor::Move);
}

#[test]
fn the_point_handles_of_a_selected_line_show_the_pointing_hand() {
    let editor = selected(&[LINE], (150.0, 300.0));
    assert_eq!(editor.cursor(100.0, 300.0), Cursor::Pointer);
    assert_eq!(editor.cursor(150.0, 300.0), Cursor::Pointer);
}

#[test]
fn drawing_tools_show_the_crosshair() {
    let mut editor = editor_with(&[RECTANGLE]);
    for tool in [
        Tool::Rectangle,
        Tool::Diamond,
        Tool::Ellipse,
        Tool::Line,
        Tool::Arrow,
        Tool::Freedraw,
    ] {
        editor.set_tool(tool);
        assert_eq!(editor.cursor(150.0, 125.0), Cursor::Crosshair, "{tool:?}");
    }
}

#[test]
fn the_text_tool_shows_the_text_cursor() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Text);
    assert_eq!(editor.cursor(20.0, 20.0), Cursor::Text);
}

#[test]
fn the_eraser_shows_the_eraser() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Eraser);
    assert_eq!(editor.cursor(20.0, 20.0), Cursor::Eraser);
}

#[test]
fn the_hand_tool_shows_the_open_hand_and_the_closed_one_while_it_drags() {
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_tool(Tool::Hand);
    assert_eq!(editor.cursor(150.0, 125.0), Cursor::Grab);
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 200 200",
    );
    assert_eq!(editor.cursor(200.0, 200.0), Cursor::Grabbing);
}

#[test]
fn space_shows_the_open_hand_over_anything_and_the_closed_one_while_it_pans() {
    let mut editor = selected(&[RECTANGLE], (150.0, 125.0));
    editor.set_pan_key(true);
    assert_eq!(editor.cursor(204.0, 154.0), Cursor::Grab);
    replay(
        &mut editor,
        "1.0 down 1 mouse 204 154
         1.1 move 1 mouse 300 300",
    );
    assert_eq!(editor.cursor(300.0, 300.0), Cursor::Grabbing);
    replay(&mut editor, "1.2 up 1 mouse 300 300\nspace up");
    assert_eq!(editor.cursor(20.0, 20.0), Cursor::Arrow);
}

#[test]
fn a_right_button_pan_shows_the_closed_hand_once_it_pans() {
    let mut editor = editor_with(&[RECTANGLE]);
    replay(&mut editor, "1.0 down 1 mouse 20 20 right");
    // Not yet a pan: it may still be a right-click.
    assert_eq!(editor.cursor(20.0, 20.0), Cursor::Arrow);
    replay(&mut editor, "1.1 move 1 mouse 80 80 right");
    assert_eq!(editor.cursor(80.0, 80.0), Cursor::Grabbing);
}
