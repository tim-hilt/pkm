//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

mod arrowhead;
mod editor;
mod freedraw;
mod freehand;
pub mod geometry;
mod hachure;
pub mod input;
pub mod json;
mod linear;
mod navigation;
mod path;
mod restore;
mod rough;
pub mod scene;
mod shape;
mod text;
mod tool;
mod viewport;
mod wrap;

pub use editor::Editor;
pub use geometry::{
    Dash, FillRule, FillStyle, Geometry, Part, PartKind, PartWriter, StrokeStyle, TextAlign,
    TextLine, TextStyle, Verb,
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
