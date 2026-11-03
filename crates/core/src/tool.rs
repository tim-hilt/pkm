//! The Tool: the active mode that determines what the next input does.

use crate::scene::ElementType;

/// Tools join as the features that need them land.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    /// Selects Elements by tapping them (the default), see [`crate::tap`].
    #[default]
    Selection,
    /// Pans the Canvas with a single pointer drag, whatever the pointer.
    Hand,
    /// Dragging draws a rectangle.
    Rectangle,
    /// Dragging draws a diamond.
    Diamond,
    /// Dragging draws an ellipse.
    Ellipse,
}

impl Tool {
    /// The type of Element dragging with this Tool draws, if it draws shapes.
    pub(crate) fn shape(self) -> Option<ElementType> {
        match self {
            Tool::Rectangle => Some(ElementType::Rectangle),
            Tool::Diamond => Some(ElementType::Diamond),
            Tool::Ellipse => Some(ElementType::Ellipse),
            Tool::Selection | Tool::Hand => None,
        }
    }
}
