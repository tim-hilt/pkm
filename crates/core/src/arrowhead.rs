//! Where the arrowheads of an arrow go and how big they are, as Excalidraw
//! works it out (`getArrowheadPoints` and its helpers in `bounds.ts` at the
//! pinned commit).
//!
//! An arrowhead sits on the end of the arrow's *sketched* stroke, not on its
//! points, so the jitter of the sketch moves it too. It points along the last
//! stretch of that stroke and shrinks when the last segment of the arrow is
//! too short to carry it. The arithmetic keeps the shape of the TypeScript so
//! that results agree with excalidraw.com's.

use std::f64::consts::PI;

use crate::path::Point;
use crate::rough::Op;
use crate::scene::Arrowhead;

/// Which end of an arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Start,
    End,
}

/// What the arrowheads of an arrow depend on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Arrow<'a> {
    pub points: &'a [Point],
    pub stroke_width: f64,
    /// Whether the arrow is drawn as a curve through its points. Heads of
    /// curved arrows never grow beyond the last segment, which the curve
    /// bends away from.
    pub curved: bool,
    pub start: Option<Arrowhead>,
    pub end: Option<Arrowhead>,
}

/// The corners of an arrowhead, placed on an arrow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Placed {
    Circle {
        centre: Point,
        diameter: f64,
    },
    /// A point with two more behind it: the corners of a triangle, the ends
    /// of the lines of an open arrowhead or a bar, and the feet of a crow's
    /// foot (whose `tip` is where the feet join).
    Wedge {
        tip: Point,
        left: Point,
        right: Point,
    },
    Diamond {
        tip: Point,
        left: Point,
        back: Point,
        right: Point,
    },
}

// Layout of the parts of composite cardinality markers, in multiples of the
// part's size (negative offsets move a part towards the tip).
pub(crate) const ONE_OR_MANY_BAR_OFFSET: f64 = -0.25;
pub(crate) const INNER_BAR_OFFSET: f64 = -0.5;
pub(crate) const ZERO_CIRCLE_OFFSET: f64 = 1.5;
pub(crate) const ZERO_CIRCLE_SCALE: f64 = 0.8;

const CARDINALITY_MARKER_SIZE: f64 = 20.0;
const CROWFOOT_SIZE: f64 = 15.0;
const ARROW_ANGLE: f64 = 20.0;
/// Scaled-down open arrowheads open up to this angle (in degrees), so that
/// they do not collapse into the line, reaching it at this size.
const ARROW_WIDEST_ANGLE: f64 = 30.0;
const ARROW_WIDEST_SIZE: f64 = 10.0;

/// The size an arrowhead is drawn at, if there is room.
fn size_of(head: Arrowhead) -> f64 {
    match head {
        Arrowhead::Arrow => 25.0,
        Arrowhead::Diamond | Arrowhead::DiamondOutline => 12.0,
        Arrowhead::CardinalityMany
        | Arrowhead::CardinalityOneOrMany
        | Arrowhead::CardinalityZeroOrMany => CROWFOOT_SIZE,
        Arrowhead::CardinalityOne
        | Arrowhead::CardinalityExactlyOne
        | Arrowhead::CardinalityZeroOrOne => CARDINALITY_MARKER_SIZE,
        _ => 15.0,
    }
}

/// How far the sides of an arrowhead open, in degrees, if it is drawn at
/// `size`.
fn angle_of(head: Arrowhead, size: f64) -> f64 {
    match head {
        Arrowhead::Bar => 90.0,
        Arrowhead::Arrow => {
            let full = size_of(head);
            let shrinkage = ((full - size) / (full - ARROW_WIDEST_SIZE)).clamp(0.0, 1.0);
            ARROW_ANGLE + (ARROW_WIDEST_ANGLE - ARROW_ANGLE) * shrinkage
        }
        _ => 25.0,
    }
}

fn is_cardinality(head: Arrowhead) -> bool {
    matches!(
        head,
        Arrowhead::CardinalityOne
            | Arrowhead::CardinalityMany
            | Arrowhead::CardinalityOneOrMany
            | Arrowhead::CardinalityExactlyOne
            | Arrowhead::CardinalityZeroOrOne
            | Arrowhead::CardinalityZeroOrMany
    )
}

/// Where `head` goes on the `end` of `arrow` (`offset` is how far back from
/// the tip, in multiples of its size; negative is towards the tip), given the
/// ops of the arrow's sketched stroke. `None` if there is nothing to put it
/// on.
pub(crate) fn place(
    arrow: &Arrow,
    stroke: &[Op],
    end: End,
    head: Arrowhead,
    offset: f64,
) -> Option<Placed> {
    let (p0, p1, p2, p3) = last_curve(stroke, end)?;

    // The curve is evaluated from its far end: 0.3 along from the arrow's
    // tip is the point the head looks back to. The value is Excalidraw's,
    // "chosen arbitrarily".
    let at = |t: f64, pick: fn(Point) -> f64| {
        (1.0 - t).powi(3) * pick(p3)
            + 3.0 * t * (1.0 - t).powi(2) * pick(p2)
            + 3.0 * t.powi(2) * (1.0 - t) * pick(p1)
            + pick(p0) * t.powi(3)
    };
    let (x2, y2) = if end == End::Start { p0 } else { p3 };
    let (x1, y1) = (at(0.3, |p| p.0), at(0.3, |p| p.1));
    let distance = (x2 - x1).hypot(y2 - y1);
    let (nx, ny) = ((x2 - x1) / distance, (y2 - y1) / distance);

    let size = size_of(head);
    let length = last_segment_length(arrow.points, end);
    // Shrink the head so that it does not outgrow the last segment, unless
    // the geometry around the corner leaves room for more.
    let length_multiplier = if matches!(head, Arrowhead::Diamond | Arrowhead::DiamondOutline) {
        0.25
    } else {
        0.5
    };
    let mut fitted = size.min(length * length_multiplier);
    if fitted < size {
        fitted = size.min(fitted.max(size_beyond_last_segment(arrow, end, head)));
    }

    let (tx, ty) = (x2 - nx * fitted * offset, y2 - ny * fitted * offset);
    let (xs, ys) = (tx - nx * fitted, ty - ny * fitted);

    if matches!(head, Arrowhead::Circle | Arrowhead::CircleOutline) {
        let diameter = (ys - ty).hypot(xs - tx) + arrow.stroke_width - 2.0;
        return finite(Placed::Circle {
            centre: (tx, ty),
            diameter,
        });
    }

    let angle = angle_of(head, fitted);

    if matches!(
        head,
        Arrowhead::CardinalityMany | Arrowhead::CardinalityOneOrMany
    ) {
        // The feet spread from where the head would end, and join at the tip.
        let left = rotate((tx, ty), (xs, ys), (-angle).to_radians());
        let right = rotate((tx, ty), (xs, ys), angle.to_radians());
        return finite(Placed::Wedge {
            tip: (xs, ys),
            left,
            right,
        });
    }

    let left = rotate((xs, ys), (tx, ty), -angle * PI / 180.0);
    let right = rotate((xs, ys), (tx, ty), angle.to_radians());

    if matches!(head, Arrowhead::Diamond | Arrowhead::DiamondOutline) {
        // The corner opposite the tip, in line with the last segment.
        let (px, py) = neighbour(arrow.points, end);
        let back = match end {
            End::Start => rotate((tx + fitted * 2.0, ty), (tx, ty), (py - ty).atan2(px - tx)),
            End::End => rotate((tx - fitted * 2.0, ty), (tx, ty), (ty - py).atan2(tx - px)),
        };
        return finite(Placed::Diamond {
            tip: (tx, ty),
            left,
            back,
            right,
        });
    }

    finite(Placed::Wedge {
        tip: (tx, ty),
        left,
        right,
    })
}

/// `placed` unless a number in it is not one (an arrow with no length has no
/// direction to put a head on).
fn finite(placed: Placed) -> Option<Placed> {
    let point = |(x, y): Point| x.is_finite() && y.is_finite();
    let ok = match placed {
        Placed::Circle { centre, diameter } => point(centre) && diameter.is_finite(),
        Placed::Wedge { tip, left, right } => point(tip) && point(left) && point(right),
        Placed::Diamond {
            tip,
            left,
            back,
            right,
        } => point(tip) && point(left) && point(back) && point(right),
    };
    ok.then_some(placed)
}

/// The cubic that ends the sketched stroke at `end` (or the first one, at the
/// start), as its four points.
fn last_curve(stroke: &[Op], end: End) -> Option<(Point, Point, Point, Point)> {
    let index = match end {
        End::Start => 1,
        End::End => stroke.len().checked_sub(1)?,
    };
    let Op::Cubic(c1, c2, to) = *stroke.get(index)? else {
        return None;
    };
    // It starts where the op before it ended.
    let from = match *stroke.get(index.checked_sub(1)?)? {
        Op::Move(p) | Op::Line(p) | Op::Cubic(_, _, p) => p,
    };
    Some((from, c1, c2, to))
}

/// The point next to the one at `end`; the origin if there is only one point.
fn neighbour(points: &[Point], end: End) -> Point {
    let index = match end {
        End::Start => 1,
        End::End => points.len().wrapping_sub(2),
    };
    points.get(index).copied().unwrap_or((0.0, 0.0))
}

/// The length of the arrow's last segment at `end`.
fn last_segment_length(points: &[Point], end: End) -> f64 {
    let tip = match end {
        End::Start => points.first(),
        End::End => points.last(),
    };
    let (cx, cy) = tip.copied().unwrap_or((0.0, 0.0));
    let (px, py) = neighbour(points, end);
    (cx - px).hypot(cy - py)
}

/// The last segment of an arrow at an end, and how much the arrow turns into
/// it from the segment before.
#[derive(Debug, Clone, Copy)]
struct Corner {
    length: f64,
    prev_length: f64,
    /// In radians; `0` if the arrow continues straight.
    turn: f64,
}

/// `None` if the arrow has no segment before the last, or one of them has no
/// length.
fn corner_at(points: &[Point], end: End) -> Option<Corner> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let [tip, corner, prev] = match end {
        End::Start => [points[0], points[1], points[2]],
        End::End => [points[n - 1], points[n - 2], points[n - 3]],
    };
    let length = (tip.0 - corner.0).hypot(tip.1 - corner.1);
    let prev_length = (corner.0 - prev.0).hypot(corner.1 - prev.1);
    if length == 0.0 || prev_length == 0.0 {
        return None;
    }
    let dot = (tip.0 - corner.0) * (corner.0 - prev.0) + (tip.1 - corner.1) * (corner.1 - prev.1);
    Some(Corner {
        length,
        prev_length,
        turn: (dot / (length * prev_length)).clamp(-1.0, 1.0).acos(),
    })
}

/// Size up to which an arrowhead reaching back past the last segment keeps its
/// sides (at `side_angle` degrees to the segment) `clearance` away from the
/// segment before.
fn sides_clear_size(corner: &Corner, side_angle: f64, clearance: f64) -> f64 {
    let angle = side_angle.to_radians();
    // The segment before stays between the sides.
    if corner.turn <= angle {
        return f64::INFINITY;
    }
    // In the tip-corner-side-end triangle, the side's end is
    // `length * sin(turn)` from the previous segment at size 0, and every
    // unit of size takes it `sin(turn - angle)` closer.
    (corner.length * corner.turn.sin() - clearance) / (corner.turn - angle).sin()
}

/// The scale (at most 1) at which all parts of a cardinality `marker` fit on
/// the last segment: the crow's foot reaches back at most to the corner, the
/// circle stays clear of it, and the bars' ends stay closer to the arrow than
/// to the previous segment.
fn cardinality_scale(marker: Arrowhead, corner: &Corner, stroke_width: f64) -> f64 {
    let bar = |offset: f64| {
        let angle = angle_of(Arrowhead::CardinalityOne, CARDINALITY_MARKER_SIZE).to_radians();
        corner.length
            / (CARDINALITY_MARKER_SIZE
                * (offset + angle.cos() + angle.sin() * (corner.turn / 2.0).tan()))
    };
    let crows_foot = || corner.length / CROWFOOT_SIZE;
    let circle = || {
        (corner.length - ((stroke_width - 2.0) * ZERO_CIRCLE_SCALE) / 2.0 - stroke_width)
            / (size_of(Arrowhead::CircleOutline) * (ZERO_CIRCLE_OFFSET + ZERO_CIRCLE_SCALE / 2.0))
    };
    match marker {
        Arrowhead::CardinalityOne => bar(0.0).min(1.0),
        Arrowhead::CardinalityMany => crows_foot().min(1.0),
        Arrowhead::CardinalityOneOrMany => crows_foot().min(bar(ONE_OR_MANY_BAR_OFFSET)).min(1.0),
        Arrowhead::CardinalityExactlyOne => bar(INNER_BAR_OFFSET).min(bar(0.0)).min(1.0),
        Arrowhead::CardinalityZeroOrOne => circle().min(bar(INNER_BAR_OFFSET)).min(1.0),
        Arrowhead::CardinalityZeroOrMany => crows_foot().min(circle()).min(1.0),
        _ => 1.0,
    }
}

/// The size `head`, or a part of a composite marker, may grow to when the
/// last segment of the arrow is too short to fit it, by using up the whole
/// segment or reaching back past its start. `0` if it should not grow.
fn size_beyond_last_segment(arrow: &Arrow, end: End, head: Arrowhead) -> f64 {
    let Some(corner) = corner_at(arrow.points, end) else {
        return 0.0;
    };
    let Corner {
        length,
        prev_length,
        turn,
    } = corner;
    // Do not take up more than half of the last two segments.
    let max_size = (length + prev_length) / 2.0;
    let marker = match end {
        End::Start => arrow.start,
        End::End => arrow.end,
    };

    // All parts of a cardinality marker scale together, and the marker has to
    // stay on the last segment to be read along it. The end of a curved
    // arrow bends away from a longer head, and its direction is too
    // unreliable for a wider one.
    if let Some(marker) = marker.filter(|&marker| is_cardinality(marker)) {
        return if arrow.curved {
            0.0
        } else {
            size_of(head) * cardinality_scale(marker, &corner, arrow.stroke_width)
        };
    }
    // Circles have no direction, the previous segment can run into them.
    if matches!(head, Arrowhead::Circle | Arrowhead::CircleOutline) {
        return max_size;
    }
    if arrow.curved {
        return 0.0;
    }
    match head {
        // Keep the bar's ends closer to the tip than to the previous
        // segment, which (other than when going straight) runs at an angle
        // to the bar.
        Arrowhead::Bar => max_size.min(length / (turn / 2.0).tan()),
        // The diamond can reach back to the corner (at twice its size); past
        // it the previous segment would cut through the diamond.
        Arrowhead::Diamond | Arrowhead::DiamondOutline => length / 2.0,
        // The triangle's base may sit on the previous segment.
        Arrowhead::Triangle | Arrowhead::TriangleOutline => max_size.min(sides_clear_size(
            &corner,
            angle_of(head, size_of(head)),
            0.0,
        )),
        // Only grow as much as legibility needs: a full-size head past a
        // sharp turn looks like an outline triangle. Keep the open sides'
        // strokes off the previous segment's; smaller heads are wider, so
        // check the angles at both ends of the range.
        Arrowhead::Arrow => {
            let legible = size_of(head) / 2.0;
            legible
                .min(max_size)
                .min(sides_clear_size(
                    &corner,
                    angle_of(head, legible),
                    arrow.stroke_width,
                ))
                .min(sides_clear_size(
                    &corner,
                    angle_of(head, 0.0),
                    arrow.stroke_width,
                ))
        }
        _ => 0.0,
    }
}

/// `point` turned clockwise about `centre` by `angle` radians.
fn rotate(point: Point, centre: Point, angle: f64) -> Point {
    if angle == 0.0 {
        return point;
    }
    let (x, y) = (point.0 - centre.0, point.1 - centre.1);
    (
        x * angle.cos() - y * angle.sin() + centre.0,
        x * angle.sin() + y * angle.cos() + centre.1,
    )
}
