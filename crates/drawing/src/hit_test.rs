//! Which Element a point on the Canvas hits (`collision.ts` and
//! `App.getElementAtPosition` at the pinned commit), and which Elements a
//! step of the eraser touches (`eraserTest` in `eraser/index.ts`).
//!
//! Like Excalidraw, a point hits an Element if it is within a *threshold* of
//! the Element's outline, or, for Elements that can be grabbed by their
//! inside, if it is inside. A hollow rectangle is therefore selected by its
//! stroke, a filled one anywhere. The threshold grows with the Element's
//! stroke and shrinks with zoom, and is larger for fingers.
//!
//! Shapes are tested as clean geometry (the rounded rectangle, not the
//! sketchy lines drawn for it), flattened to polygons fine enough to be
//! within a fraction of a unit of the true shape. Lines and arrows turn about
//! the middle of the box around their points rather than around that of their
//! sketched stroke, which differs by the sketch's jitter.

use std::collections::HashMap;
use std::f64::consts::TAU;

use crate::color::parse_color;
use crate::frame::Frame;
use crate::freedraw;
use crate::input::PointerKind;
use crate::linear;
use crate::path::{Point, Segment, is_loop, to_cubics};
use crate::rough::Op;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use crate::shape::{diamond_corners, rounded_diamond, rounded_rectangle};
use crate::text;

/// Excalidraw's `DEFAULT_COLLISION_THRESHOLD` (8, less an epsilon) times the
/// 0.85 of `getElementHitThreshold`: the reach of a mouse in view points.
const POINTER_REACH: f64 = 6.8;
/// Half of the 44 points Apple recommends for touch targets.
const FINGER_REACH: f64 = 22.0;

/// How far from an Element's outline a pointer still hits it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Reach {
    /// In view points: the same on screen at any zoom.
    view_points: f64,
    zoom: f64,
}

impl Reach {
    pub fn new(kind: PointerKind, zoom: f64) -> Self {
        let view_points = match kind {
            PointerKind::Finger => FINGER_REACH,
            PointerKind::Pencil | PointerKind::Mouse => POINTER_REACH,
        };
        Self { view_points, zoom }
    }

    /// In Scene units: Excalidraw's `getElementHitThreshold`.
    fn of(&self, element: &Element) -> f64 {
        (element.stroke_width() / 2.0 + 0.1).max(self.view_points / self.zoom)
    }
}

/// The topmost Element hit by `point` (in the Scene), if any.
///
/// Locked Elements, deleted ones and labels (which belong to their
/// container) are not hit: a point on a label hits the container.
pub(crate) fn topmost<'a>(
    scene: &'a Scene,
    selection: &Selection,
    point: Point,
    reach: Reach,
) -> Option<Element<'a>> {
    // Only the two topmost hits matter.
    let (mut below, mut top) = (None, None);
    // Made when the first container with a label is reached: most Scenes
    // have none, or few.
    let mut labels: Option<HashMap<&str, Element>> = None;
    for element in scene.elements().filter(is_selectable) {
        let threshold = reach.of(&element);
        // The box of a selected Element is part of it, so that a hollow
        // Element does not lose its selection to a tap on its inside.
        let in_box = selection.contains(element.id())
            && has_bounding_box(&element)
            && Frame::of(&element).contains_turned(point, element.angle(), threshold);
        // The label of a container is part of it, wherever it sits
        // (Excalidraw's `hitElementBoundText`): an arrow's may be well off
        // its line.
        let on_label = element.has_bound_text()
            && labels
                .get_or_insert_with(|| text::labels(scene))
                .get(element.id())
                .is_some_and(|label| Frame::of(label).contains_turned(point, label.angle(), 0.0));
        if in_box || on_label || hits_itself(&element, point, threshold) {
            (below, top) = (top, Some(element));
        }
    }
    match (below, top) {
        // With several hits, the topmost must be hit properly, not just
        // barely: a tap next to a hollow Element in front of another goes
        // to the one behind.
        (Some(below), Some(top)) => {
            if hits_itself(&top, point, reach.of(&top) / 2.0) {
                Some(top)
            } else {
                Some(below)
            }
        }
        (_, top) => top,
    }
}

/// Every Element hit by `point` (in the Scene), topmost last: what a tap of
/// the eraser deletes (`App.getElementsAtPosition`).
pub(crate) fn all_at<'a>(
    scene: &'a Scene,
    point: Point,
    reach: Reach,
) -> impl Iterator<Item = Element<'a>> {
    scene
        .elements()
        .filter(is_selectable)
        .filter(move |element| hits_itself(element, point, reach.of(element)))
}

/// Whether the Element can be selected at all: shown, unlocked, and not a
/// label, which belongs to its container.
pub(crate) fn is_selectable(element: &Element) -> bool {
    !element.is_deleted() && !element.is_locked() && element.container_id().is_none()
}

/// Whether the Element shows a box with handles when selected. Lines and
/// arrows show their points instead (as on excalidraw.com's touch devices).
fn has_bounding_box(element: &Element) -> bool {
    !element.kind().is_linear()
}

fn hits_itself(element: &Element, point: Point, threshold: f64) -> bool {
    let frame = Frame::of(element);
    // Ink reaches past the points of a freedraw stroke.
    let margin = match element.kind() {
        ElementType::Freedraw => threshold + freedraw::ink_reach(element),
        _ => threshold,
    };
    if !frame.contains_turned(point, element.angle(), margin) {
        return false;
    }
    let point = frame.unturned(point, element.angle());
    let inside_counts = grabbable_inside(element);
    let moved = |points: Vec<Point>| in_scene(element, points);

    match element.kind() {
        ElementType::Line | ElementType::Arrow => {
            let path = moved(linear_path(element));
            near(point, &path, false, threshold) || (inside_counts && inside(point, &path))
        }
        ElementType::Freedraw => {
            let ink = moved(freedraw::outline(element));
            // The loop a stroke makes, not the ink around it, is what its
            // background fills.
            let loop_inside = inside_counts && inside(point, &moved(element.points()));
            winding(point, &ink) != 0 || near(point, &ink, true, threshold) || loop_inside
        }
        _ => {
            let polygon = moved(closed_shape(element));
            near(point, &polygon, true, threshold) || (inside_counts && inside(point, &polygon))
        }
    }
}

/// `points` of the Element's own frame, with its top left corner at the
/// origin, in the Scene (not turned).
fn in_scene(element: &Element, points: Vec<Point>) -> Vec<Point> {
    let (x, y) = (element.x(), element.y());
    points
        .into_iter()
        .map(|(px, py)| (x + px, y + py))
        .collect()
}

/// Excalidraw's `shouldTestInside`: whether the inside of the Element is
/// part of it. That is so for Elements with a visible background, for texts
/// and images, and for containers of a label; but never for arrows, and for
/// lines and freedraw strokes only if they close a loop.
fn grabbable_inside(element: &Element) -> bool {
    let filled = parse_color(element.background_color()) & 0xff != 0;
    let kind = element.kind();
    let draggable = match kind {
        ElementType::Text | ElementType::Image => true,
        _ => filled || element.has_bound_text(),
    };
    match kind {
        ElementType::Arrow => false,
        ElementType::Line | ElementType::Freedraw => draggable && is_loop(&element.points()),
        _ => draggable,
    }
}

// --- Eraser ---------------------------------------------------------------------

/// How far from the ink of a freedraw stroke the eraser still touches it:
/// the larger of 2.25 Scene units and 5 view points (Excalidraw's values).
const FREEDRAW_ERASE_MIN: f64 = 2.25;
const FREEDRAW_ERASE_VIEW: f64 = 5.0;
/// How far from a freedraw stroke a step of the eraser is looked for, as
/// Excalidraw does before it measures.
const FREEDRAW_ERASE_NEAR: f64 = 15.0;

/// Whether the step of the eraser from `from` to `to` (in the Scene) touches
/// the Element, in a view at `zoom` (Excalidraw's `eraserTest`).
///
/// Shapes are touched by crossing their outline, or, for those that can be
/// grabbed by their inside, by the step ending in it; the eraser passing
/// through a hollow shape without touching its outline leaves it alone. Lines
/// and arrows are touched by passing within a tolerance of their path, and
/// freedraw strokes of the ink around it.
pub(crate) fn crossed_by(element: &Element, from: Point, to: Point, zoom: f64) -> bool {
    let frame = Frame::of(element);
    let angle = element.angle();
    let step = (frame.unturned(from, angle), frame.unturned(to, angle));
    let kind = element.kind();
    let stroke = element.stroke_width();

    // Cheap rejection first: the boxes of the step and the Element, both
    // widened by the tolerance, must overlap.
    let near = match kind {
        ElementType::Freedraw => FREEDRAW_ERASE_NEAR.max(freedraw::ink_reach(element)),
        _ => stroke / 2.0,
    };
    let overlap =
        |a: f64, b: f64, min: f64, max: f64| a.min(b) - near <= max && a.max(b) + near >= min;
    if !(overlap(step.0.0, step.1.0, frame.min.0, frame.max.0)
        && overlap(step.0.1, step.1.1, frame.min.1, frame.max.1))
    {
        return false;
    }

    let moved = |points: Vec<Point>| in_scene(element, points);
    let inside_counts = grabbable_inside(element);
    match kind {
        ElementType::Line | ElementType::Arrow => {
            let path = moved(linear_path(element));
            let tolerance = stroke.max(stroke * 2.0 / zoom);
            let touched = path_distance(step, &path, false) <= tolerance;
            touched || (inside_counts && inside(step.1, &path))
        }
        ElementType::Freedraw => {
            let ink = moved(freedraw::outline(element));
            let tolerance = FREEDRAW_ERASE_MIN.max(FREEDRAW_ERASE_VIEW / zoom);
            let loop_inside = inside_counts && inside(step.1, &moved(element.points()));
            path_distance(step, &ink, true) <= tolerance
                || winding(step.0, &ink) != 0
                || loop_inside
        }
        _ => {
            let polygon = moved(closed_shape(element));
            path_distance(step, &polygon, true) <= 0.0
                || (inside_counts && inside(step.1, &polygon))
        }
    }
}

/// How far the segment `step` is from the path through `points`, closed or
/// not; `0` where they cross.
fn path_distance(step: (Point, Point), points: &[Point], closed: bool) -> f64 {
    match points {
        [] => f64::INFINITY,
        [only] => distance_to_segment(*only, step.0, step.1),
        _ => edges(points, closed)
            .map(|edge| segments_distance(step, edge))
            .fold(f64::INFINITY, f64::min),
    }
}

/// The distance between two segments: `0` if they cross or touch, else that of
/// the closest pair of endpoints and segments.
fn segments_distance(a: (Point, Point), b: (Point, Point)) -> f64 {
    let side = |(ox, oy): Point, (px, py): Point, (qx, qy): Point| {
        (px - ox) * (qy - oy) - (py - oy) * (qx - ox)
    };
    let crossing = side(b.0, b.1, a.0) * side(b.0, b.1, a.1) < 0.0
        && side(a.0, a.1, b.0) * side(a.0, a.1, b.1) < 0.0;
    if crossing {
        return 0.0;
    }
    [
        distance_to_segment(a.0, b.0, b.1),
        distance_to_segment(a.1, b.0, b.1),
        distance_to_segment(b.0, a.0, a.1),
        distance_to_segment(b.1, a.0, a.1),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

// --- Shapes ----------------------------------------------------------------------

/// The outline of a rectangle, diamond, ellipse, text or image, in the
/// Element's own frame (its top left corner at the origin).
fn closed_shape(element: &Element) -> Vec<Point> {
    let (w, h) = (element.width(), element.height());
    match (element.kind(), element.roundness()) {
        (ElementType::Ellipse, _) => ellipse(w / 2.0, h / 2.0),
        (ElementType::Diamond, Some(roundness)) => flatten(&rounded_diamond(w, h, roundness)),
        (ElementType::Diamond, None) => diamond_corners(w, h).to_vec(),
        (_, Some(roundness)) => flatten(&rounded_rectangle(w, h, roundness)),
        (_, None) => vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
    }
}

/// The path of a line or arrow in the Element's own frame: through its
/// points, as the smooth curve it is drawn as if it has roundness (and three
/// points to bend at), see [`linear::exact_curve`].
fn linear_path(element: &Element) -> Vec<Point> {
    let Some(ops) = linear::exact_curve(element) else {
        return element.points();
    };
    let segments: Vec<Segment> = ops
        .into_iter()
        .map(|op| match op {
            Op::Move(p) => Segment::Move(p),
            Op::Line(p) => Segment::Line(p),
            Op::Cubic(c1, c2, end) => Segment::Cubic(c1, c2, end),
        })
        .collect();
    flatten(&segments)
}

/// The polygon of an ellipse with radii `rx` and `ry` whose box has its top
/// left corner at the origin: within a quarter unit of the curve.
fn ellipse(rx: f64, ry: f64) -> Vec<Point> {
    // A chord of angle `a` on a circle of radius `r` is `r * a^2 / 8` off.
    let r = rx.abs().max(ry.abs());
    let chords = (TAU * (r / 2.0).sqrt()).ceil().clamp(16.0, 400.0) as usize;
    (0..chords)
        .map(|i| {
            let angle = TAU * i as f64 / chords as f64;
            (rx + rx * angle.cos(), ry + ry * angle.sin())
        })
        .collect()
}

/// A chord of a curve is at most this long (but a curve has at least
/// [`MIN_CHORDS`]): a quarter-circle corner of the largest radius (32) is
/// then within 0.15 of its chords.
const MAX_CHORD: f64 = 8.0;
const MIN_CHORDS: usize = 8;

/// The points along a path of a single subpath (one `Move` first), curves
/// cut into chords.
fn flatten(segments: &[Segment]) -> Vec<Point> {
    let mut points: Vec<Point> = Vec::new();
    for segment in to_cubics(segments) {
        match segment {
            Segment::Move(p) | Segment::Line(p) => points.push(p),
            Segment::Cubic(c1, c2, end) => {
                let start = points.last().copied().unwrap_or(end);
                let length = [(start, c1), (c1, c2), (c2, end)]
                    .iter()
                    .map(|&((ax, ay), (bx, by))| (bx - ax).hypot(by - ay))
                    .sum::<f64>();
                let chords = ((length / MAX_CHORD).ceil() as usize).clamp(MIN_CHORDS, 256);
                for step in 1..=chords {
                    let t = step as f64 / chords as f64;
                    points.push(cubic_at([start, c1, c2, end], t));
                }
            }
            Segment::Quad(..) => unreachable!("to_cubics removes quadratic curves"),
        }
    }
    points
}

fn cubic_at([p0, p1, p2, p3]: [Point; 4], t: f64) -> Point {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

// --- Point and path --------------------------------------------------------------

/// The edges of a path through `points`, with one back to the start if
/// `closed`.
fn edges(points: &[Point], closed: bool) -> impl Iterator<Item = (Point, Point)> {
    let wrap = closed && points.len() > 1;
    points
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .chain(wrap.then(|| (points[points.len() - 1], points[0])))
}

/// Whether `point` is within `threshold` of the path.
fn near(point: Point, points: &[Point], closed: bool, threshold: f64) -> bool {
    match points {
        [] => false,
        // A path of one point is a dot.
        [only] => distance_to_segment(point, *only, *only) <= threshold,
        _ => edges(points, closed).any(|(a, b)| distance_to_segment(point, a, b) <= threshold),
    }
}

fn distance_to_segment((px, py): Point, (ax, ay): Point, (bx, by): Point) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / length_squared).clamp(0.0, 1.0)
    };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// Whether `point` is inside the polygon, by the even-odd rule: a ray to the
/// right crosses its edges an odd number of times.
fn inside((px, py): Point, polygon: &[Point]) -> bool {
    edges(polygon, true)
        .filter(|&((ax, ay), (bx, by))| {
            (ay > py) != (by > py) && px < ax + (py - ay) / (by - ay) * (bx - ax)
        })
        .count()
        % 2
        == 1
}

/// How often the polygon winds around `point`; not zero where ink is.
fn winding((px, py): Point, polygon: &[Point]) -> i32 {
    edges(polygon, true)
        .filter(|&((ax, ay), (bx, by))| {
            (ay > py) != (by > py) && px < ax + (py - ay) / (by - ay) * (bx - ax)
        })
        .map(|((_, ay), (_, by))| if by > ay { 1 } else { -1 })
        .sum()
}
