//! Overlay output: what clients draw over the Scene to show the state of
//! editing. It uses the same geometry vocabulary as the Scene itself, in
//! Scene coordinates.
//!
//! Overlay strokes are as thin and as far from what they surround on screen
//! at any zoom, so they are made for one zoom; clients ask again when it
//! changes (see `Editor::overlay_revision`).

use crate::frame::Frame;
use crate::geometry::{Geometry, StrokeStyle};
use crate::path::Point;
use crate::scene::{Element, ElementType};

/// Excalidraw's selection colour (`--color-selection`), opaque.
const SELECTION_COLOR: u32 = 0x6965_dbff;
/// Excalidraw's `DEFAULT_TRANSFORM_HANDLE_SPACING * 2`: how far in view
/// points a selection frame keeps from its Element.
const FRAME_PADDING: f64 = 4.0;
/// The line width of a selection frame in view points.
const FRAME_WIDTH: f64 = 1.0;

/// The selection frame of every Element in `selected`, for a view at `zoom`:
/// a rectangle around the Element, turned with it.
pub(crate) fn selection<'a>(selected: impl Iterator<Item = Element<'a>>, zoom: f64) -> Geometry {
    let mut geometry = Geometry::new();
    for element in selected {
        let frame = Frame::drawn(&element);
        // Images are framed on their edges, everything else a little away.
        let padding = match element.kind() {
            ElementType::Image => 0.0,
            _ => FRAME_PADDING / zoom,
        };
        let style = StrokeStyle {
            color: SELECTION_COLOR,
            width: (FRAME_WIDTH / zoom) as f32,
            opacity: 1.0,
            dash: None,
        };
        let corners = corners(&frame, padding, element.angle());
        let mut path = geometry.stroke(element.handle(), element.version(), style);
        let [first, rest @ ..] = corners;
        path.move_to(first.0, first.1);
        for (x, y) in rest.into_iter().chain([first]) {
            path.line_to(x, y);
        }
    }
    geometry
}

/// The corners of `frame` grown by `padding`, clockwise from the top left,
/// turned by `angle` about its centre.
fn corners(frame: &Frame, padding: f64, angle: f64) -> [(f32, f32); 4] {
    let (x0, y0) = (frame.min.0 - padding, frame.min.1 - padding);
    let (x1, y1) = (frame.max.0 + padding, frame.max.1 + padding);
    let (cx, cy) = frame.centre();
    let (sin, cos) = angle.sin_cos();
    let turned = |(x, y): Point| {
        let (dx, dy) = (x - cx, y - cy);
        (
            (cx + dx * cos - dy * sin) as f32,
            (cy + dx * sin + dy * cos) as f32,
        )
    };
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(turned)
}
