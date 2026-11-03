//! Overlay output: what clients draw over the Scene to show the state of
//! editing. It uses the same geometry vocabulary as the Scene itself, in
//! Scene coordinates.
//!
//! Overlay strokes are as thin and as far from what they surround on screen
//! at any zoom, so they are made for one zoom; clients ask again when it
//! changes (see `Editor::overlay_revision`).

use crate::edit_points::PointHandles;
use crate::frame::Frame;
use crate::geometry::{FillRule, FillStyle, Geometry, PartWriter, StrokeStyle};
use crate::input::PointerKind;
use crate::path::Point;
use crate::scene::{Element, ElementType};
use crate::transform_handles::{Grip, TransformBox};

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
        trace(
            &mut geometry.stroke(element.handle(), element.version(), style),
            corners,
        );
    }
    geometry
}

/// Excalidraw's handle colour: white squares with the selection colour
/// around them.
const HANDLE_COLOR: u32 = 0xffff_ffff;
/// What overlay parts that belong to no Element (transform handles, the
/// box-select rectangle) name in its place: clients do not cache the overlay
/// by Element (see [`Editor::overlay`]).
const NO_ELEMENT: u32 = u32::MAX;

/// The transform handles of a Selection: a white square with the selection
/// colour around it at every handle's centre, as big as the pointer's grip
/// makes it, turned with the box (Excalidraw's transform handles).
pub(crate) fn transform_handles(
    geometry: &mut Geometry,
    transform_box: &TransformBox,
    kind: PointerKind,
    zoom: f64,
) {
    let half = Grip::of(kind).visual / zoom / 2.0;
    let (sin, cos) = transform_box.angle().sin_cos();
    for (_, (cx, cy)) in transform_box.handles(zoom) {
        let corners =
            [(-half, -half), (half, -half), (half, half), (-half, half)].map(|(dx, dy)| {
                (
                    (cx + dx * cos - dy * sin) as f32,
                    (cy + dx * sin + dy * cos) as f32,
                )
            });
        framed(geometry, corners, HANDLE_COLOR, zoom);
    }
}

/// `corners` filled with `fill` and framed in the selection colour, one view
/// point wide at any zoom: what a handle looks like.
fn framed(geometry: &mut Geometry, corners: [(f32, f32); 4], fill: u32, zoom: f64) {
    // A writer holds the geometry, so the fill is finished before the stroke
    // around it begins.
    {
        let mut fill = geometry.fill(
            NO_ELEMENT,
            0,
            FillStyle {
                color: fill,
                opacity: 1.0,
                rule: FillRule::NonZero,
            },
        );
        trace(&mut fill, corners);
    }
    let mut stroke = geometry.stroke(
        NO_ELEMENT,
        0,
        StrokeStyle {
            color: SELECTION_COLOR,
            width: (FRAME_WIDTH / zoom) as f32,
            opacity: 1.0,
            dash: None,
        },
    );
    trace(&mut stroke, corners);
}

/// The handles of the points of the line or arrow that is alone in the
/// Selection: a square on every point, filled with the selection colour for
/// the `selected` ones, and a smaller one in the middle of segments that are
/// long enough. As big as the pointer's grip makes them.
pub(crate) fn point_handles(
    geometry: &mut Geometry,
    handles: &PointHandles,
    selected: &[usize],
    kind: PointerKind,
    zoom: f64,
) {
    let half = Grip::of(kind).visual / zoom / 2.0;
    for (index, &at) in handles.points().iter().enumerate() {
        let fill = if selected.contains(&index) {
            SELECTION_COLOR
        } else {
            HANDLE_COLOR
        };
        framed(geometry, square(at, half), fill, zoom);
    }
    for at in handles.middles() {
        framed(geometry, square(at, half / 2.0), HANDLE_COLOR, zoom);
    }
}

/// The corners of the square around `centre` that reaches `half` to each
/// side.
fn square((cx, cy): Point, half: f64) -> [(f32, f32); 4] {
    [(-half, -half), (half, -half), (half, half), (-half, half)]
        .map(|(dx, dy)| ((cx + dx) as f32, (cy + dy) as f32))
}

/// Excalidraw's box-select fill (`rgba(0, 0, 200, 0.04)`).
const BOX_FILL: u32 = 0x0000_c8ff;
const BOX_FILL_OPACITY: f32 = 0.04;

/// The rectangle of a box-select drag over `bounds`: faintly filled, framed
/// in the selection colour, one view point wide at any zoom.
pub(crate) fn box_select(geometry: &mut Geometry, bounds: &Frame, zoom: f64) {
    let corners = corners(bounds, 0.0, 0.0);
    {
        let mut fill = geometry.fill(
            NO_ELEMENT,
            0,
            FillStyle {
                color: BOX_FILL,
                opacity: BOX_FILL_OPACITY,
                rule: FillRule::NonZero,
            },
        );
        trace(&mut fill, corners);
    }
    let mut stroke = geometry.stroke(
        NO_ELEMENT,
        0,
        StrokeStyle {
            color: SELECTION_COLOR,
            width: (FRAME_WIDTH / zoom) as f32,
            opacity: 1.0,
            dash: None,
        },
    );
    trace(&mut stroke, corners);
}

/// A closed chain through `corners`.
fn trace(path: &mut PartWriter, corners: [(f32, f32); 4]) {
    let [first, rest @ ..] = corners;
    path.move_to(first.0, first.1);
    for (x, y) in rest.into_iter().chain([first]) {
        path.line_to(x, y);
    }
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
