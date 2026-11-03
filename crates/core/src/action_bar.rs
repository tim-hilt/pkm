//! The Action Bar: the small floating bar next to a non-empty Selection that
//! offers duplicate, delete and more, in place of a context menu (ADR-0006).
//!
//! The core decides whether the bar is shown and where; clients draw it. The
//! anchor is in view coordinates, so that it follows the Selection through
//! panning and zooming without the client doing any geometry: it is read
//! with the viewport after every input, like the viewport itself.

use crate::frame::Frame;
use crate::path::Point;
use crate::scene::{Element, Scene};
use crate::selection::Selection;
use crate::viewport::Viewport;

/// How far in view points the anchor keeps below the Selection: clear of the
/// transform handles, which a finger's 44-point grips reach 28 points
/// beyond the box.
const GAP: f64 = 32.0;

/// Where the Action Bar is shown: the middle of its top edge, in view
/// coordinates (points from the Canvas's top-left corner, y down). Clients
/// keep the bar inside the view themselves, as only they know its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionBar {
    pub x: f64,
    pub y: f64,
}

impl ActionBar {
    /// The bar for `selection` as seen through `viewport`: below the middle
    /// of the bounds of its shown Elements. `None` if it selects none.
    pub(crate) fn of(scene: &Scene, selection: &Selection, viewport: Viewport) -> Option<Self> {
        let frame = scene
            .elements()
            .filter(|e| !e.is_deleted() && selection.contains(e.id()))
            .map(|e: Element| Frame::shown(&e))
            .reduce(Frame::union)?;
        let (x, y): Point = viewport.view_at((frame.centre().0, frame.max.1));
        Some(Self { x, y: y + GAP })
    }
}
