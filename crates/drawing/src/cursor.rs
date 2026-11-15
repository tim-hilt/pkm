//! The mouse cursor over the Canvas: what pressing where it is would do, as
//! on excalidraw.com (`CURSOR_TYPE` and `getCursorForResizingElement`). The
//! core decides it (ADR-0006); clients only map each kind to their platform's
//! cursor.

use std::f64::consts::{FRAC_PI_4, PI};

use crate::transform_handles::Handle;

/// A kind of mouse cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Cursor {
    /// Nothing in particular happens: a press selects or box-selects.
    #[default]
    Arrow,
    /// A press starts drawing.
    Crosshair,
    /// A press places or edits text.
    Text,
    /// A drag moves what is under the pointer, or is moving it.
    Move,
    /// A drag pans the Canvas, or rotates the Selection.
    Grab,
    /// The Canvas is being panned, or the Selection rotated.
    Grabbing,
    /// A press takes hold of a point of a line or arrow.
    Pointer,
    /// A drag erases.
    Eraser,
    /// A drag resizes up and down.
    ResizeNorthSouth,
    /// A drag resizes left and right.
    ResizeEastWest,
    /// A drag resizes along the diagonal from top left to bottom right.
    ResizeNorthWestSouthEast,
    /// A drag resizes along the diagonal from top right to bottom left.
    ResizeNorthEastSouthWest,
}

impl Cursor {
    /// The cursor over `handle` of a box turned by `angle` (clockwise, in
    /// radians): the resize arrows point the way the handle drags, to the
    /// nearest of the four directions there are cursors for.
    pub(crate) fn of_handle(handle: Handle, angle: f64) -> Self {
        // The direction of the handle's drag in the unturned box, clockwise
        // from pointing right; a resize goes both ways, so modulo a half turn.
        let direction = match handle {
            Handle::Rotation => return Cursor::Grab,
            Handle::E | Handle::W => 0.0,
            Handle::NW | Handle::SE => FRAC_PI_4,
            Handle::N | Handle::S => 2.0 * FRAC_PI_4,
            Handle::NE | Handle::SW => 3.0 * FRAC_PI_4,
        };
        let turned = (direction + angle).rem_euclid(PI);
        match (turned / FRAC_PI_4).round() as u8 % 4 {
            0 => Cursor::ResizeEastWest,
            1 => Cursor::ResizeNorthWestSouthEast,
            2 => Cursor::ResizeNorthSouth,
            _ => Cursor::ResizeNorthEastSouthWest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_eighth_turn_turns_the_cursor_to_the_next_direction() {
        assert_eq!(
            Cursor::of_handle(Handle::E, FRAC_PI_4),
            Cursor::ResizeNorthWestSouthEast
        );
        assert_eq!(
            Cursor::of_handle(Handle::N, FRAC_PI_4),
            Cursor::ResizeNorthEastSouthWest
        );
        assert_eq!(
            Cursor::of_handle(Handle::NE, FRAC_PI_4),
            Cursor::ResizeEastWest
        );
    }

    #[test]
    fn a_turn_backwards_or_by_more_than_a_half_turn_wraps_around() {
        assert_eq!(
            Cursor::of_handle(Handle::E, -FRAC_PI_4),
            Cursor::ResizeNorthEastSouthWest
        );
        assert_eq!(
            Cursor::of_handle(Handle::S, 3.0 * PI),
            Cursor::ResizeNorthSouth
        );
    }

    mod cursor {
        //! The mouse cursor over the Canvas (ADR-0006), tested at the Editor seam:
        //! pointer events and the hovered view point in, `Editor::cursor` out. The
        //! cursor tells what pressing there would do, as on excalidraw.com.

        use crate::{Cursor, Editor, Tool};

        use crate::test_support as common;
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
    }
}
