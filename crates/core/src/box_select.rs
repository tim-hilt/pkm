//! Box-select: selecting Elements by dragging a rectangle over empty Canvas
//! (ADR-0006; `getElementsWithinSelection` at the pinned commit).
//!
//! As on excalidraw.com the box selects the Elements it *encloses*, not those
//! it merely touches, and the Selection follows the pointer while it drags:
//! Elements join as the box grows over them and leave as it shrinks back.
//! Unlike there, a box that encloses nothing leaves the Selection alone
//! instead of clearing it: a sloppy touch that happens to drag on empty
//! Canvas must not throw the Selection away (only a short tap does, see
//! [`crate::tap`]). With shift held the box adds to the Selection the drag
//! began with instead of replacing it.

use crate::frame::Frame;
use crate::group;
use crate::hit_test;
use crate::path::Point;
use crate::scene::Scene;
use crate::selection::Selection;

/// A box-select drag by one pointer.
#[derive(Debug)]
pub(crate) struct BoxSelect {
    pointer: u64,
    /// Where the pointer went down, in Scene coordinates; one corner.
    origin: Point,
    /// Where the pointer is now; the opposite corner.
    to: Point,
    /// The Selection when the drag began.
    before: Selection,
    /// Whether the box adds to `before` rather than replacing it.
    additive: bool,
    /// The group that was gone into when the drag began, if any: the box
    /// selects the Elements in it one by one, as a tap does.
    editing: Option<String>,
}

impl BoxSelect {
    /// Starts a box with `pointer`, which went down at `origin`, while
    /// `before` is selected and the group `editing` is gone into.
    pub fn begin(
        pointer: u64,
        origin: Point,
        before: &Selection,
        editing: Option<&str>,
        additive: bool,
    ) -> Self {
        Self {
            pointer,
            origin,
            to: origin,
            before: before.clone(),
            additive,
            editing: editing.map(str::to_owned),
        }
    }

    /// The pointer that draws the box.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Follows the pointer to `to`. Returns whether the box changed.
    pub fn drag_to(&mut self, to: Point) -> bool {
        let moved = to != self.to;
        self.to = to;
        moved
    }

    /// The box, in Scene coordinates.
    pub fn bounds(&self) -> Frame {
        let (a, b) = (self.origin, self.to);
        Frame {
            min: (a.0.min(b.0), a.1.min(b.1)),
            max: (a.0.max(b.0), a.1.max(b.1)),
        }
    }

    /// What the Selection is while the box is where it is: the Elements it
    /// encloses, and the other Elements of their groups.
    pub fn selection(&self, scene: &Scene) -> Selection {
        let bounds = self.bounds();
        let enclosed: Vec<String> = scene
            .elements()
            .filter(hit_test::is_selectable)
            .filter(|element| {
                // Elements are as big as they are drawn: as far as their
                // turned bounds and half of their stroke around them.
                let shown = Frame::shown(element);
                let stroke = element.stroke_width() / 2.0;
                bounds.min.0 <= shown.min.0 - stroke
                    && bounds.min.1 <= shown.min.1 - stroke
                    && bounds.max.0 >= shown.max.0 + stroke
                    && bounds.max.1 >= shown.max.1 + stroke
            })
            .map(|element| element.id().to_owned())
            .collect();
        let enclosed = group::expand(scene, &enclosed, self.editing.as_deref());

        let mut selection = Selection::default();
        match (self.additive, enclosed.is_empty()) {
            (true, _) => {
                selection.select(self.before.ids().iter().cloned().chain(enclosed));
            }
            (false, false) => {
                selection.select(enclosed);
            }
            (false, true) => return self.before.clone(),
        }
        selection
    }

    /// Abandons the box; the Selection goes back to what it was before it.
    pub fn cancel(self) -> Selection {
        self.before
    }
}
