//! [`Editor::cursor`]: the mouse cursor over the Canvas.

use super::*;
use crate::cursor::Cursor;

impl Editor {
    /// The cursor for a mouse at the view point `(x, y)`: what pressing there
    /// would do with the Tool and Selection as they are, or what the drag in
    /// progress does. Clients ask on every mouse move and after every input
    /// or command, and map it to their platform's cursor.
    pub fn cursor(&self, x: f64, y: f64) -> Cursor {
        if self.navigation.is_dragging() {
            return Cursor::Grabbing;
        }
        if let Some(transforming) = &self.transforming {
            return match Cursor::of_handle(transforming.handle(), transforming.angle()) {
                Cursor::Grab => Cursor::Grabbing,
                resize => resize,
            };
        }
        if self.moving.is_some() {
            return Cursor::Move;
        }
        if self.point_dragging.is_some() {
            return Cursor::Pointer;
        }
        if self.box_selecting.is_some() {
            return Cursor::Arrow;
        }
        if self.navigation.pan_key() {
            return Cursor::Grab;
        }
        match self.tool {
            Tool::Selection => self.selection_cursor(x, y),
            Tool::Hand => Cursor::Grab,
            Tool::Text => Cursor::Text,
            Tool::Eraser => Cursor::Eraser,
            Tool::Rectangle
            | Tool::Diamond
            | Tool::Ellipse
            | Tool::Line
            | Tool::Arrow
            | Tool::Freedraw => Cursor::Crosshair,
        }
    }

    /// What a drag of the selection Tool from `(x, y)` would do, in the order
    /// [`Editor::begin_drag`] tries it.
    fn selection_cursor(&self, x: f64, y: f64) -> Cursor {
        let kind = PointerKind::Mouse;
        let transform_box = TransformBox::of(&self.scene, &self.selection);
        let viewport = self.navigation.viewport();
        let at = viewport.scene_at(x, y);
        if let Some(transform_box) = transform_box
            && let Some(handle) = transform_box.grab(at, kind, viewport.zoom())
        {
            return Cursor::of_handle(handle, transform_box.angle());
        }
        if self.grabbed_point(x, y, kind).is_some() {
            return Cursor::Pointer;
        }
        if self.in_selection_box(transform_box, x, y) || self.hit(x, y, kind).is_some() {
            return Cursor::Move;
        }
        Cursor::Arrow
    }
}
