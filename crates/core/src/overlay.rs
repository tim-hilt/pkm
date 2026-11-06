//! Overlay output: what clients draw over the Scene to show the state of
//! editing. It uses the same geometry vocabulary as the Scene itself, in
//! Scene coordinates.
//!
//! Overlay strokes are as thin and as far from what they surround on screen
//! at any zoom, so they are made for one zoom; clients ask again when it
//! changes (see `Editor::overlay_revision`).

use crate::edit_points::PointHandles;
use std::collections::HashSet;

use crate::frame::Frame;
use crate::geometry::{Dash, FillRule, FillStyle, Geometry, PartWriter, StrokeStyle};
use crate::group;
use crate::input::PointerKind;
use crate::path::Point;
use crate::scene::{ElementType, Scene};
use crate::selection::Selection;
use crate::transform_handles::{Grip, TransformBox};

/// Excalidraw's selection colour (`--color-selection`), opaque.
const SELECTION_COLOR: u32 = 0x6965_dbff;
/// Excalidraw's `DEFAULT_TRANSFORM_HANDLE_SPACING * 2`: how far in view
/// points a selection frame keeps from its Element.
const FRAME_PADDING: f64 = 4.0;
/// The line width of a selection frame in view points.
const FRAME_WIDTH: f64 = 1.0;

/// Excalidraw's `groupSelectionColor` (black in the light theme).
const GROUP_COLOR: u32 = 0x0000_00ff;
/// The dash of a group frame in view points: `8` drawn, `4` skipped.
const GROUP_DASH: (f64, f64) = (8.0, 4.0);

/// The selection frames of a Selection, for a view at `zoom`: a rectangle
/// around each selected Element, turned with it, except those that are
/// selected as part of a group; and a dashed one around the common bounds
/// of each selected group and of the group that was gone into (`editing`),
/// as Excalidraw draws them (`renderInteractiveScene`).
pub(crate) fn selection(
    scene: &Scene,
    selection: &Selection,
    editing: Option<&str>,
    zoom: f64,
) -> Geometry {
    let groups = group::selected_members(scene, selection, editing);
    let in_group: HashSet<&str> = groups.iter().flatten().copied().collect();
    let mut geometry = Geometry::new();
    let singles = scene
        .elements()
        .filter(|e| !e.is_deleted() && selection.contains(e.id()) && !in_group.contains(e.id()));
    for element in singles {
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
    let entered = editing.map(|group| group::members(scene, group));
    let selected = groups.iter().map(|ids| {
        scene
            .elements()
            .filter(|e| ids.contains(&e.id()))
            .collect::<Vec<_>>()
    });
    for members in selected.chain(entered) {
        if let Some(bounds) = members.iter().map(Frame::shown).reduce(Frame::union) {
            group_frame(&mut geometry, &bounds, zoom);
        }
    }
    geometry
}

/// A dashed frame around `bounds`, not turned.
fn group_frame(geometry: &mut Geometry, bounds: &Frame, zoom: f64) {
    let style = StrokeStyle {
        color: GROUP_COLOR,
        width: (FRAME_WIDTH / zoom) as f32,
        opacity: 1.0,
        dash: Some(Dash {
            on: (GROUP_DASH.0 / zoom) as f32,
            off: (GROUP_DASH.1 / zoom) as f32,
        }),
    };
    let corners = corners(bounds, FRAME_PADDING / zoom, 0.0);
    trace(&mut geometry.stroke(NO_ELEMENT, 0, style), corners);
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

/// The colour of the highlight of a shape an arrow would bind to: a blue that
/// sets it apart from the selection colour (Excalidraw's `#1971c2`).
const HIGHLIGHT_COLOR: u32 = 0x1971_c2ff;
/// The line width of the highlight of a shape in view points: thicker than
/// a selection frame, as it marks where an arrow would bind.
const HIGHLIGHT_WIDTH: f64 = 3.0;
const HIGHLIGHT_OPACITY: f32 = 0.6;

/// The highlight of the shape an arrow end would bind to: its `outline`, a
/// closed polygon in the Scene.
pub(crate) fn binding_highlight(geometry: &mut Geometry, outline: &[Point], zoom: f64) {
    let Some(&(x, y)) = outline.first() else {
        return;
    };
    let mut stroke = geometry.stroke(
        NO_ELEMENT,
        0,
        StrokeStyle {
            color: HIGHLIGHT_COLOR,
            width: (HIGHLIGHT_WIDTH / zoom) as f32,
            opacity: HIGHLIGHT_OPACITY,
            dash: None,
        },
    );
    stroke.move_to(x as f32, y as f32);
    for &(x, y) in outline.iter().skip(1).chain(outline.first()) {
        stroke.line_to(x as f32, y as f32);
    }
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
