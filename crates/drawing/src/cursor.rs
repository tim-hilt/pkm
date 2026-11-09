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
}
