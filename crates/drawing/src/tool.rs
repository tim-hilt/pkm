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
    /// Dragging draws a line of two points, tapping places the points of a
    /// longer one, see [`crate::draw_linear`].
    Line,
    /// The same, with an arrowhead at the end.
    Arrow,
    /// Dragging draws a freehand stroke, see [`crate::draw_freedraw`]. The
    /// Tool returns to selection after each stroke, unless the Tool lock is on.
    Freedraw,
    /// Dragging previews the Elements the pointer passes over and deletes
    /// them on release, see [`crate::erase`]. The Tool returns to selection
    /// after each erase, unless the Tool lock is on.
    Eraser,
    /// Tapping opens the text editor at the tap, or on the text that is
    /// there; see [`crate::text_editing`].
    Text,
}

impl Tool {
    /// The type of Element this Tool draws, if it draws any.
    pub(crate) fn element(self) -> Option<ElementType> {
        match self {
            Tool::Freedraw => Some(ElementType::Freedraw),
            Tool::Text => Some(ElementType::Text),
            _ => self.shape().or(self.linear()),
        }
    }

    /// The type of Element dragging with this Tool draws, if it draws shapes.
    pub(crate) fn shape(self) -> Option<ElementType> {
        match self {
            Tool::Rectangle => Some(ElementType::Rectangle),
            Tool::Diamond => Some(ElementType::Diamond),
            Tool::Ellipse => Some(ElementType::Ellipse),
            Tool::Selection
            | Tool::Hand
            | Tool::Line
            | Tool::Arrow
            | Tool::Freedraw
            | Tool::Eraser
            | Tool::Text => None,
        }
    }

    /// The type of Element drawing with this Tool makes, if it draws lines
    /// or arrows.
    pub(crate) fn linear(self) -> Option<ElementType> {
        match self {
            Tool::Line => Some(ElementType::Line),
            Tool::Arrow => Some(ElementType::Arrow),
            _ => None,
        }
    }
}
