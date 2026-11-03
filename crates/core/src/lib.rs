//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

mod arrowhead;
mod base64;
mod box_select;
mod draw_shape;
mod editor;
mod frame;
mod freedraw;
mod freehand;
pub mod geometry;
mod hachure;
mod history;
mod hit_test;
mod image;
pub mod input;
pub mod json;
mod linear;
mod move_selection;
mod navigation;
mod overlay;
mod path;
mod random;
mod restore;
mod rough;
pub mod scene;
mod selection;
mod shape;
mod style;
mod tap;
mod text;
mod tool;
mod transform;
mod transform_handles;
mod viewport;
mod wrap;

pub use editor::Editor;
pub use geometry::{
    Dash, FillRule, FillStyle, Geometry, Image, ImageStatus, Part, PartKind, PartWriter,
    StrokeStyle, TextAlign, TextLine, TextStyle, Verb,
};
pub use input::{MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
pub use scene::LoadError;
pub use text::TextMeasurer;
pub use tool::Tool;
pub use viewport::{MAX_ZOOM, MIN_ZOOM, Viewport};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
