//! What Excalidraw draws for lines and arrows (`shape.ts` at the pinned
//! commit): a straight, multi-point or curved stroke through the Element's
//! points, and for arrows the arrowheads on its ends (ADR-0004).
//!
//! Elbow arrows are drawn through their points as they are saved in the
//! file, without Excalidraw's routing and rounded corners (the core does not
//! route them).

use crate::arrowhead::{self, Arrow, End, Placed};
use crate::geometry::{Dash, FillRule, Geometry, StrokeStyle};
use crate::path::{Point, is_loop};
use crate::rough::{Drawing, FillOps, Op, Sketcher, Style};
use crate::scene::{Arrowhead, Element, ElementType, LineStyle};
use crate::shape::{
    Paint, Placement, Target, dash_of, emit, fill_of, fill_weight, parse_color, stroke_width,
    style_of,
};

/// Adds the strokes of the line or arrow `element` to `geometry`.
/// `background` is the colour of the Canvas, which some arrowheads are filled
/// with.
pub(crate) fn draw(element: &Element, background: u32, geometry: &mut Geometry) {
    let points = element.points();
    let is_arrow = element.kind() == ElementType::Arrow;
    let Sketch {
        drawing,
        style,
        curved,
    } = sketch(element, &points);

    let target = Target::of(element, placement(element, &drawing.outline));
    let stroke = StrokeStyle {
        color: parse_color(element.stroke_color()),
        width: stroke_width(element) as f32,
        opacity: element.opacity() as f32,
        dash: dash_of(element),
    };
    // Lines are filled by the even-odd rule, like every polygon and curve of
    // rough.js.
    let paint = Paint {
        stroke,
        fill_color: parse_color(element.background_color()),
        fill_rule: FillRule::EvenOdd,
        hatch_width: fill_weight(element) as f32,
    };
    emit(geometry, &target, &drawing, &paint);

    if is_arrow {
        let arrow = Arrow {
            points: &points,
            stroke_width: element.stroke_width(),
            curved,
            start: element.start_arrowhead(),
            end: element.end_arrowhead(),
        };
        let heads = Heads {
            element,
            style: Style {
                fill: None,
                ..style
            },
            stroke_color: stroke.color,
            background,
        };
        for (end, head) in [(End::Start, arrow.start), (End::End, arrow.end)] {
            let Some(head) = head else { continue };
            for piece in heads.pieces(&arrow, &drawing.outline, end, head) {
                emit(geometry, &target, &piece.drawing, &piece.paint);
            }
        }
    }
}

/// A line or arrow sketched through `points`, its own, as drawn.
struct Sketch {
    drawing: Drawing,
    style: Style,
    curved: bool,
}

fn sketch(element: &Element, points: &[Point]) -> Sketch {
    let curved = element.roundness().is_some() && !element.is_elbow_arrow();

    // Only a line that comes back to its start has an inside to fill.
    let fill = if element.kind() == ElementType::Line && is_loop(points) {
        fill_of(element)
    } else {
        None
    };
    let style = Style {
        fill,
        ..style_of(element, extent(points))
    };

    let mut sketcher = Sketcher::new(element.seed(), style);
    let drawing = if curved {
        sketcher.curve(points)
    } else if fill.is_some() {
        sketcher.polygon(points)
    } else {
        sketcher.linear_path(points)
    };
    Sketch {
        drawing,
        style,
        curved,
    }
}

/// The smooth curve through the points of a line or arrow that is rounded
/// and has three points or more to bend at, in the Element's own frame: a
/// move to the first point, then a cubic for each segment. Excalidraw gets the
/// curve from rough.js, without any of its jitter, and so does this. `None`
/// for Elements that are drawn straight.
pub(crate) fn exact_curve(element: &Element) -> Option<Vec<Op>> {
    let points = element.points();
    if element.roundness().is_none() || element.is_elbow_arrow() || points.len() < 3 {
        return None;
    }
    const EXACT: Style = Style {
        roughness: 0.0,
        preserve_vertices: true,
        multi_stroke: false,
        curve_fitting: 0.95,
        fill: None,
    };
    Some(Sketcher::new(element.seed(), EXACT).curve(&points).outline)
}

/// The corners (top left, bottom right) in the Scene of the box Excalidraw
/// takes for the line or arrow: around its sketched stroke, jitter included,
/// but not the arrowheads. `None` if the stroke has no curves.
pub(crate) fn drawn_bounds(element: &Element) -> Option<(Point, Point)> {
    let (x, y) = (element.x(), element.y());
    let ((x0, y0), (x1, y1)) = curve_bounds(&sketched_outline(element), |p| p)?;
    Some(((x + x0, y + y0), (x + x1, y + y1)))
}

/// The corners in the Scene of the smallest box around the line's or arrow's
/// sketched stroke turned by its angle about the middle of
/// [`drawn_bounds`] (Excalidraw's `getLinearElementRotatedBounds`): jitter
/// included, arrowheads not. `None` if the stroke has no curves.
pub(crate) fn shown_bounds(element: &Element) -> Option<(Point, Point)> {
    // Sketched once: this is asked for every line and arrow on each move of
    // a box-select.
    let outline = sketched_outline(element);
    let (x, y) = (element.x(), element.y());
    let ((x0, y0), (x1, y1)) = curve_bounds(&outline, |p| p)?;
    let angle = element.angle();
    if angle == 0.0 {
        return Some(((x + x0, y + y0), (x + x1, y + y1)));
    }
    let pivot = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (sin, cos) = angle.sin_cos();
    curve_bounds(&outline, |(px, py)| {
        let (dx, dy) = (px - pivot.0, py - pivot.1);
        (
            x + pivot.0 + dx * cos - dy * sin,
            y + pivot.1 + dx * sin + dy * cos,
        )
    })
}

fn sketched_outline(element: &Element) -> Vec<Op> {
    sketch(element, &element.points()).drawing.outline
}

/// Width and height of the box around `points`, `(0, 0)` for none: what
/// Excalidraw's `getSizeFromPoints` makes the size of a line or arrow.
pub(crate) fn extent(points: &[Point]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let (mut min, mut max) = (
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for &(x, y) in points {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    (max.0 - min.0, max.1 - min.1)
}

/// Lines and arrows turn about the middle of the box around their sketched
/// stroke (jitter included), not around the box of their points.
fn placement(element: &Element, outline: &[Op]) -> Placement {
    let centre = match curve_bounds(outline, |p| p) {
        Some(((x0, y0), (x1, y1))) => ((x0 + x1) / 2.0, (y0 + y1) / 2.0),
        None => (element.width() / 2.0, element.height() / 2.0),
    };
    Placement::about(element, (element.x() + centre.0, element.y() + centre.1))
}

/// The corners of the box around the curves of `ops`; straight ops are not
/// part of it, as in Excalidraw (the strokes of lines are curves).
fn curve_bounds(ops: &[Op], place: impl Fn(Point) -> Point) -> Option<(Point, Point)> {
    let mut current = (0.0, 0.0);
    let mut bounds: Option<(Point, Point)> = None;
    for &op in ops {
        match op {
            Op::Move(p) => current = p,
            Op::Line(_) => {}
            Op::Cubic(c1, c2, end) => {
                let (min, max) = cubic_bounds([current, c1, c2, end].map(&place));
                bounds = Some(match bounds {
                    None => (min, max),
                    Some((lo, hi)) => (
                        (lo.0.min(min.0), lo.1.min(min.1)),
                        (hi.0.max(max.0), hi.1.max(max.1)),
                    ),
                });
                current = end;
            }
        }
    }
    bounds
}

/// The box around one cubic Bézier curve: its end points and the extremes
/// along the curve in between.
fn cubic_bounds([p0, p1, p2, p3]: [Point; 4]) -> (Point, Point) {
    let along = |a: f64, b: f64, c: f64, d: f64| {
        let (lo, hi) = (a.min(d), a.max(d));
        extremes(a, b, c, d)
            .into_iter()
            .flatten()
            .fold((lo, hi), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    let (x0, x1) = along(p0.0, p1.0, p2.0, p3.0);
    let (y0, y1) = along(p0.1, p1.1, p2.1, p3.1);
    ((x0, y0), (x1, y1))
}

/// The values of one coordinate of a cubic Bézier curve where it turns around
/// (at most two).
fn extremes(p0: f64, p1: f64, p2: f64, p3: f64) -> [Option<f64>; 2] {
    let (i, j, k) = (p1 - p0, p2 - p1, p3 - p2);
    let (a, b, c) = (3.0 * i - 6.0 * j + 3.0 * k, 6.0 * j - 6.0 * i, 3.0 * i);
    let discriminant = b * b - 4.0 * c * a;
    if discriminant < 0.0 {
        return [None; 2];
    }
    let roots = if a == 0.0 {
        [-c / b; 2]
    } else {
        let root = discriminant.sqrt();
        [(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)]
    };
    roots.map(|t| {
        (0.0..=1.0).contains(&t).then(|| {
            let u = 1.0 - t;
            u.powi(3) * p0 + 3.0 * u.powi(2) * t * p1 + 3.0 * u * t.powi(2) * p2 + t.powi(3) * p3
        })
    })
}

// --- Arrowheads (getArrowheadShapes) ----------------------------------------

/// One drawable of an arrowhead.
struct Piece {
    drawing: Drawing,
    paint: Paint,
}

/// What the arrowheads of one arrow are drawn from.
struct Heads<'a, 'e> {
    element: &'a Element<'e>,
    /// The style of the arrow's stroke, without a fill.
    style: Style,
    stroke_color: u32,
    /// The colour of the Canvas: outline arrowheads are filled with it so
    /// that they hide what lies behind them.
    background: u32,
}

impl Heads<'_, '_> {
    /// The drawables of `head` on `end` of `arrow`, whose sketched stroke is
    /// `stroke`; none if it cannot be put there.
    fn pieces(&self, arrow: &Arrow, stroke: &[Op], end: End, head: Arrowhead) -> Vec<Piece> {
        use Arrowhead::*;
        let place =
            |part: Arrowhead, offset: f64| arrowhead::place(arrow, stroke, end, part, offset);
        let mut pieces = Vec::new();
        match head {
            Circle | CircleOutline => {
                let fill = if head == CircleOutline {
                    self.background
                } else {
                    self.stroke_color
                };
                pieces.extend(self.circle(place(head, 0.0), 1.0, fill));
            }
            Triangle | TriangleOutline | Diamond | DiamondOutline => {
                let fill = if matches!(head, TriangleOutline | DiamondOutline) {
                    self.background
                } else {
                    self.stroke_color
                };
                pieces.extend(self.polygon(place(head, 0.0), fill));
            }
            CardinalityOne => pieces.extend(self.bar(place(CardinalityOne, 0.0))),
            CardinalityMany => pieces.extend(self.lines_to_tip(place(CardinalityMany, 0.0))),
            CardinalityOneOrMany => {
                pieces.extend(self.lines_to_tip(place(CardinalityMany, 0.0)));
                let bar = place(CardinalityOne, arrowhead::ONE_OR_MANY_BAR_OFFSET);
                pieces.extend(self.bar(bar));
            }
            CardinalityExactlyOne => {
                let inner = place(CardinalityOne, arrowhead::INNER_BAR_OFFSET);
                pieces.extend(self.bar(inner));
                pieces.extend(self.bar(place(CardinalityOne, 0.0)));
            }
            CardinalityZeroOrOne => {
                let circle = place(CircleOutline, arrowhead::ZERO_CIRCLE_OFFSET);
                pieces.extend(self.circle(circle, arrowhead::ZERO_CIRCLE_SCALE, self.background));
                let inner = place(CardinalityOne, arrowhead::INNER_BAR_OFFSET);
                pieces.extend(self.bar(inner));
            }
            CardinalityZeroOrMany => {
                pieces.extend(self.lines_to_tip(place(CardinalityMany, 0.0)));
                let circle = place(CircleOutline, arrowhead::ZERO_CIRCLE_OFFSET);
                pieces.extend(self.circle(circle, arrowhead::ZERO_CIRCLE_SCALE, self.background));
            }
            Bar | Arrow | Other => pieces.extend(self.lines_to_tip(place(head, 0.0))),
        }
        pieces
    }

    /// Two strokes from the sides of the head to its tip: an open arrowhead,
    /// a bar (whose sides lie opposite each other) or a crow's foot.
    fn lines_to_tip(&self, placed: Option<Placed>) -> Vec<Piece> {
        let Some(Placed::Wedge { tip, left, right }) = placed else {
            return Vec::new();
        };
        [left, right]
            .into_iter()
            .map(|side| self.line(side, tip))
            .collect()
    }

    /// One stroke across the arrow: the bar of a cardinality marker.
    fn bar(&self, placed: Option<Placed>) -> Option<Piece> {
        let Some(Placed::Wedge { left, right, .. }) = placed else {
            return None;
        };
        Some(self.line(left, right))
    }

    /// A stroke between two points, sketched at most as roughly as an
    /// artist's. Dotted arrows keep dotted heads, with the dots closer
    /// together; every other arrow has solid ones.
    fn line(&self, from: Point, to: Point) -> Piece {
        let style = Style {
            roughness: self.style.roughness.min(1.0),
            ..self.style
        };
        let dash = (self.element.line_style() == LineStyle::Dotted).then(|| Dash {
            on: 1.5,
            off: (self.element.stroke_width() + 4.0) as f32,
        });
        let drawing = Sketcher::new(self.element.seed(), style).linear_path(&[from, to]);
        self.piece(drawing, dash, 0, FillRule::NonZero)
    }

    /// A small polygon filled with `fill`, sketched along its outline.
    fn polygon(&self, placed: Option<Placed>, fill: u32) -> Option<Piece> {
        let corners = match placed? {
            Placed::Wedge { tip, left, right } => vec![tip, left, right],
            Placed::Diamond {
                tip,
                left,
                back,
                right,
            } => vec![tip, left, back, right],
            Placed::Circle { .. } => return None,
        };
        let style = Style {
            roughness: self.style.roughness.min(1.0),
            // A second stroke would stray from the fill.
            multi_stroke: false,
            // The corners of the small polygon stay in place, only its edges
            // bow.
            preserve_vertices: true,
            ..self.style
        };
        let drawing = Sketcher::new(self.element.seed(), style).polygon(&corners);
        Some(self.piece(filled_along_outline(drawing), None, fill, FillRule::EvenOdd))
    }

    /// A circle of `scale` times the placed size, filled with `fill`.
    fn circle(&self, placed: Option<Placed>, scale: f64, fill: u32) -> Option<Piece> {
        let Placed::Circle { centre, diameter } = placed? else {
            return None;
        };
        let style = Style {
            roughness: self.style.roughness.min(0.5),
            multi_stroke: false,
            ..self.style
        };
        let diameter = diameter * scale;
        let drawing = Sketcher::new(self.element.seed(), style)
            .ellipse(centre.0, centre.1, diameter, diameter);
        Some(self.piece(filled_along_outline(drawing), None, fill, FillRule::NonZero))
    }

    fn piece(&self, drawing: Drawing, dash: Option<Dash>, fill: u32, rule: FillRule) -> Piece {
        Piece {
            drawing,
            paint: Paint {
                stroke: StrokeStyle {
                    color: self.stroke_color,
                    width: stroke_width(self.element) as f32,
                    opacity: self.element.opacity() as f32,
                    dash,
                },
                fill_color: fill,
                fill_rule: rule,
                hatch_width: 0.0,
            },
        }
    }
}

/// The drawing with its outline's strokes also as its fill, one continuous
/// shape. rough.js' own solid fill is jittered separately from the outline,
/// by up to 2 units, which shows on shapes as small as arrowheads.
fn filled_along_outline(drawing: Drawing) -> Drawing {
    let area = drawing
        .outline
        .iter()
        .enumerate()
        .map(|(index, &op)| match op {
            Op::Move(p) if index > 0 => Op::Line(p),
            other => other,
        })
        .collect();
    Drawing {
        fill: Some(FillOps::Area(area)),
        ..drawing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_around_a_curve_includes_where_it_bulges_past_its_end_points() {
        // From (0, 0) up to y = 100 and back: the curve peaks at 75 (the
        // value at the middle, 1/8 + 3/8 + 3/8 + 1/8 weights of 0, 100, 100,
        // 0), but never reaches its control points.
        let ops = [
            Op::Move((0.0, 0.0)),
            Op::Cubic((0.0, 100.0), (100.0, 100.0), (100.0, 0.0)),
        ];

        assert_eq!(curve_bounds(&ops, |p| p), Some(((0.0, 0.0), (100.0, 75.0))));
    }

    #[test]
    fn there_is_no_box_around_strokes_without_curves() {
        assert_eq!(curve_bounds(&[Op::Move((1.0, 2.0))], |p| p), None);
    }
}
