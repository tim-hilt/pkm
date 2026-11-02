//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

mod editor;
pub mod geometry;
pub mod json;
mod restore;
pub mod scene;

pub use editor::Editor;
pub use geometry::{Geometry, Part, PartKind, PartWriter, StrokeStyle, Verb};
pub use scene::LoadError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
