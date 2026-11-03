//! What Excalidraw draws for a freedraw stroke (`shape.ts` at the pinned
//! commit): the outline perfect-freehand makes around the points, filled in
//! the stroke colour, and for a closed stroke a sketched fill of its
//! background (ADR-0004).
//!
//! Strokes of constant width (`strokeOptions.variability` `"constant"`, which
//! Excalidraw draws with its laser pointer geometry) are drawn with variable
//! width like the others.

use crate::freehand::{self, Options, Sample};
use crate::geometry::{FillRule, FillStyle, Geometry, StrokeStyle};
use crate::path::{Point, Segment, is_loop, simplify, to_cubics};
use crate::rough::{Op, Sketcher, Style};
use crate::scene::Element;
use crate::shape::{
    Paint, Placement, Target, emit_fill, fill_of, fill_weight, parse_color, stroke_width, style_of,
    write_ops,
};

/// A stroke is this many times as wide as the Element's stroke width at full
/// pressure. Tuned by eye in Excalidraw.
const SIZE_FACTOR: f64 = 4.25;
const THINNING: f64 = 0.6;
const SMOOTHING: f64 = 0.5;
/// How far the simplified path the fill follows may stray from the points.
const FILL_SIMPLIFICATION: f64 = 0.75;

/// Adds the fill (if the stroke is a loop) and the stroke of the freedraw
/// `element` to `geometry`.
pub(crate) fn draw(element: &Element, geometry: &mut Geometry) {
    let points = element.points();
    let placement = Placement::about(element, centre(element, &points));
    let target = Target::of(element, placement);
    let color = parse_color(element.stroke_color());

    if is_loop(&points) {
        let path = simplify(&points, FILL_SIMPLIFICATION);
        // Excalidraw draws the curve without its outline, so only the fill
        // is used; the outline only advances the random sequence.
        let style = Style {
            fill: fill_of(element),
            ..style_of(element, (element.width(), element.height()))
        };
        let drawing = Sketcher::new(element.seed(), style).curve(&path);
        let paint = Paint {
            stroke: StrokeStyle {
                color,
                width: stroke_width(element) as f32,
                opacity: element.opacity() as f32,
                dash: None,
            },
            fill_color: parse_color(element.background_color()),
            fill_rule: FillRule::EvenOdd,
            hatch_width: fill_weight(element) as f32,
        };
        emit_fill(geometry, &target, &drawing, &paint);
    }

    let ops = outline_path(&stroke_outline(element, &points));
    if ops.is_empty() {
        return;
    }
    let style = FillStyle {
        color,
        opacity: element.opacity() as f32,
        rule: FillRule::NonZero,
    };
    let mut path = geometry.fill(element.handle(), element.version(), style);
    write_ops(&mut path, &ops, &placement);
}

/// The middle of the box around the points, in the Scene: strokes turn about
/// it, whatever the Element's own width and height say.
fn centre(element: &Element, points: &[Point]) -> Point {
    let Some(&first) = points.first() else {
        return (element.x(), element.y());
    };
    let (min, max) = points.iter().fold((first, first), |(min, max), &(x, y)| {
        ((min.0.min(x), min.1.min(y)), (max.0.max(x), max.1.max(y)))
    });
    (
        element.x() + (min.0 + max.0) / 2.0,
        element.y() + (min.1 + max.1) / 2.0,
    )
}

/// The polygon around the stroke, in the Element's frame.
fn stroke_outline(element: &Element, points: &[Point]) -> Vec<Point> {
    let simulate_pressure = element.simulates_pressure();
    let pressures = element.pressures();
    let samples: Vec<Sample> = if simulate_pressure {
        points
            .iter()
            .map(|&point| Sample {
                point,
                pressure: None,
            })
            .collect()
    } else if points.is_empty() {
        // Excalidraw draws a dot where a stroke has no points at all.
        vec![Sample {
            point: (0.0, 0.0),
            pressure: Some(0.5),
        }]
    } else {
        points
            .iter()
            .enumerate()
            .map(|(i, &point)| Sample {
                point,
                pressure: pressures.get(i).copied(),
            })
            .collect()
    };
    let options = Options {
        size: element.stroke_width() * SIZE_FACTOR,
        thinning: THINNING,
        smoothing: SMOOTHING,
        streamline: element.streamline(),
        simulate_pressure,
        // Ease out sine: https://easings.net/#easeOutSine
        easing: |t| (t * std::f64::consts::FRAC_PI_2).sin(),
    };
    freehand::outline(&samples, &options)
}

/// The closed shape through `outline` that Excalidraw fills: curves that
/// bend towards each point and end halfway to the next (`getSvgPathFromStroke`).
fn outline_path(outline: &[Point]) -> Vec<Op> {
    let Some(&first) = outline.first() else {
        return Vec::new();
    };
    let middle = |a: Point, b: Point| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let mut segments = vec![Segment::Move(first)];
    for (i, &point) in outline.iter().enumerate() {
        let next = outline[(i + 1) % outline.len()];
        segments.push(Segment::Quad(point, middle(point, next)));
    }
    segments.push(Segment::Line(first));

    to_cubics(&segments)
        .into_iter()
        .map(|segment| match segment {
            Segment::Move(p) => Op::Move(p),
            Segment::Line(p) => Op::Line(p),
            Segment::Cubic(c1, c2, p) => Op::Cubic(c1, c2, p),
            Segment::Quad(..) => unreachable!("to_cubics leaves no quadratic curves"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outline_is_a_closed_chain_of_curves_ending_at_the_midpoints() {
        let path = outline_path(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)]);

        assert_eq!(path.len(), 5);
        assert_eq!(path[0], Op::Move((0.0, 0.0)));
        assert!(matches!(path[1], Op::Cubic(_, _, (2.0, 0.0))));
        assert!(matches!(path[2], Op::Cubic(_, _, (4.0, 2.0))));
        // The last curve ends halfway back to the start, then a line closes.
        assert!(matches!(path[3], Op::Cubic(_, _, (2.0, 2.0))));
        assert_eq!(path[4], Op::Line((0.0, 0.0)));
    }

    #[test]
    fn there_is_no_outline_without_points() {
        assert_eq!(outline_path(&[]), Vec::new());
    }
}
