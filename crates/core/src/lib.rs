//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

mod action_bar;
mod arrowhead;
mod base64;
mod binding;
mod box_select;
mod clipboard;
mod color;
mod delete;
mod draw_freedraw;
mod draw_linear;
mod draw_shape;
mod duplicate;
mod edit_points;
mod editor;
mod erase;
mod fractional_index;
mod frame;
mod freedraw;
mod freehand;
pub mod geometry;
mod group;
mod hachure;
mod history;
mod hit_test;
mod image;
mod index_sync;
pub mod input;
mod insert_image;
pub mod json;
mod laser;
mod linear;
mod move_selection;
mod navigation;
mod outline;
mod overlay;
mod path;
mod pencil_double_tap;
mod pencil_only;
mod properties;
mod random;
mod restore;
mod rough;
pub mod scene;
mod selection;
mod shape;
mod shortcut;
mod style;
mod tap;
mod text;
mod text_editing;
mod tool;
mod transform;
mod transform_handles;
mod viewport;
mod wrap;
mod z_order;

pub use action_bar::ActionBar;
pub use editor::{Editor, Export, ExportScope};
pub use geometry::{
    Dash, FillRule, FillStyle, Geometry, Image, ImageStatus, Part, PartKind, PartWriter,
    StrokeStyle, TextAlign, TextLine, TextStyle, Verb,
};
pub use input::{MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
pub use insert_image::ImageFile;
pub use pencil_double_tap::PencilDoubleTap;
pub use properties::{Properties, Property};
pub use scene::LoadError;
pub use shortcut::{Command, Shortcut, shortcuts};
pub use style::{Edges, FontSize, Sloppiness, StrokeWidth};
pub use text::TextMeasurer;
pub use text_editing::TextEditor;
pub use tool::Tool;
pub use viewport::{MAX_ZOOM, MIN_ZOOM, Viewport};
pub use z_order::ZOrder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
