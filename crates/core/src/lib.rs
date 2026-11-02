//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

mod editor;
pub mod geometry;
pub mod input;
pub mod json;
mod navigation;
mod restore;
pub mod scene;
mod viewport;

pub use editor::Editor;
pub use geometry::{Geometry, Part, PartKind, PartWriter, StrokeStyle, Verb};
pub use input::{MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
pub use scene::LoadError;
pub use viewport::{MAX_ZOOM, MIN_ZOOM, Viewport};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
