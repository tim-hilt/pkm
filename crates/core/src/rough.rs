//! The sketchy line generator of rough.js (MIT, v4.6.4), as far as Excalidraw
//! uses it for rectangles, diamonds and ellipses (ADR-0004).
//!
//! rough.js draws a shape as hand-jittered Bézier strokes, drawing every
//! stroke twice. All jitter comes from one seeded generator, so the same seed
//! gives the same sketch. To reproduce excalidraw.com the *order* in which
//! random numbers are drawn must be exactly rough.js' (including numbers that
//! are drawn and then multiplied by zero), and the arithmetic is kept in the
//! same shape, so that results agree to the last digits. The code is
//! otherwise organised the Rust way: a [`Sketcher`] owns the generator and
//! its style and turns shapes into [`Op`]s.
//!
//! A shape is drawn as its outline and, if it has a fill, a fill. rough.js
//! computes the outline first, so its jitter does not depend on the fill; the
//! fill then continues the same random sequence.

use std::f64::consts::PI;

use crate::hachure::{self, Line};
use crate::path::{Point, Segment, curve_polyline, points_on_path, to_cubics};

/// One drawing operation, in the coordinate frame of the shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Op {
    Move(Point),
    Line(Point),
    /// Control point, control point, end point.
    Cubic(Point, Point, Point),
}

/// A shape as rough.js draws it: first the fill, then the outline over it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Drawing {
    pub fill: Option<FillOps>,
    pub outline: Vec<Op>,
}

/// What rough.js draws to fill a shape.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FillOps {
    /// Lines that are stroked in the fill colour (hatching).
    Lines(Vec<Op>),
    /// An outline whose enclosed area is filled.
    Area(Vec<Op>),
}

/// How the inside of a shape is filled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Fill {
    Solid,
    Hatch(Hatch),
}

/// Parallel lines across a shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hatch {
    pub pattern: Pattern,
    /// Distance between neighbouring lines.
    pub gap: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pattern {
    /// Lines at -41 degrees.
    Hachure,
    /// Hachure lines and lines at right angles to them.
    CrossHatch,
    /// Lines at -41 degrees that bend back and forth between neighbours.
    Zigzag,
}

/// rough.js' `hachureAngle`, in degrees.
const HACHURE_ANGLE: f64 = -41.0;

/// rough.js' random number generator: a Lehmer / "minstd" generator
/// (multiplier 48271) on 32-bit integers, which JavaScript computes with
/// `Math.imul`.
#[derive(Debug, Clone)]
pub(crate) struct Random {
    state: i32,
}

impl Random {
    /// `seed` is converted like JavaScript's `Math.imul` converts its
    /// operands (ToInt32), so seeds beyond 32 bits wrap around.
    ///
    /// rough.js falls back to `Math.random()` for seed 0; that cannot be
    /// reproduced, so seed 0 here gives a constant stream of zeros.
    pub(crate) fn new(seed: f64) -> Self {
        Self {
            state: to_int32(seed),
        }
    }

    /// The next number in `0.0..1.0`.
    pub(crate) fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(48271);
        f64::from(self.state & 0x7fff_ffff) / 2_147_483_648.0
    }
}

/// JavaScript's ToInt32: truncate, then wrap modulo 2^32.
fn to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    // The remainder is exact and below 2^32, so the casts cannot lose more
    // than the wrapping that ToInt32 asks for.
    (n.trunc() % 4_294_967_296.0) as i64 as i32
}

/// How a shape is sketched. Everything else rough.js has as an option is
/// fixed to the defaults Excalidraw uses.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Style {
    /// `0` draws exact lines; larger values jitter more.
    pub roughness: f64,
    /// Keep the end points of lines where they are instead of jittering them.
    pub preserve_vertices: bool,
    /// Draw every stroke twice, as rough.js does by default.
    pub multi_stroke: bool,
    /// How well an ellipse's stroke fits it: `1` is exact, rough.js'
    /// default `0.95` lets the radii wobble a little.
    pub curve_fitting: f64,
    /// How the inside is filled, if at all.
    pub fill: Option<Fill>,
}

/// rough.js' `maxRandomnessOffset`.
const MAX_OFFSET: f64 = 2.0;
const BOWING: f64 = 1.0;
const CURVE_STEP_COUNT: f64 = 9.0;
/// How much rougher the single stroke of a solid fill is than the outline.
const FILL_SHAPE_ROUGHNESS_GAIN: f64 = 0.8;

/// Draws shapes with jitter from one seeded generator. Shapes drawn one after
/// the other continue the same random sequence.
#[derive(Debug, Clone)]
pub(crate) struct Sketcher {
    seed: f64,
    random: Random,
    style: Style,
}

impl Sketcher {
    pub(crate) fn new(seed: f64, style: Style) -> Self {
        Self {
            seed,
            random: Random::new(seed),
            style,
        }
    }

    /// Straight pieces from point to point, each drawn sketchily (rough.js
    /// `linearPath`; also its `line`, which is one piece). Nothing is drawn
    /// for fewer than two points.
    pub(crate) fn linear_path(&mut self, points: &[Point]) -> Drawing {
        let mut outline = Vec::new();
        for pair in points.windows(2) {
            self.double_line(pair[0], pair[1], &mut outline);
        }
        Drawing {
            fill: None,
            outline,
        }
    }

    /// A smooth curve through `points`, which are at least two for anything
    /// to be drawn; a curve of three or more is filled like a polygon along
    /// the curve, if the style has a fill (rough.js `curve`).
    pub(crate) fn curve(&mut self, points: &[Point]) -> Drawing {
        let roughness = self.style.roughness;
        let mut outline = self.curve_stroke(points, 1.0 + roughness * 0.2);
        if self.style.multi_stroke {
            // The second stroke has a random sequence of its own, from the
            // next seed (rough.js `cloneOptionsAlterSeed`).
            let seed = if self.seed == 0.0 {
                0.0
            } else {
                self.seed + 1.0
            };
            let first = std::mem::replace(&mut self.random, Random::new(seed));
            let second = self.curve_stroke(points, 1.5 * (1.0 + roughness * 0.22));
            self.random = first;
            outline.extend(second);
        }

        let fill = match self.style.fill {
            Some(fill) if points.len() >= 3 => Some(match fill {
                Fill::Solid => {
                    // One continuous, rougher stroke; the sequence goes on
                    // where the outline's first stroke left it.
                    let style = self.fill_shape_style();
                    let ops = self.with_style(style, |this| {
                        this.curve_stroke(points, 1.0 + style.roughness * 0.2)
                    });
                    FillOps::Area(join_pieces(ops))
                }
                Fill::Hatch(hatch) => {
                    let mut polygon = vec![curve_polyline(points, 10.0, (1.0 + roughness) / 2.0)];
                    FillOps::Lines(self.hatch(&mut polygon, hatch))
                }
            }),
            _ => None,
        };
        Drawing { fill, outline }
    }

    /// One pass along a curve, every point nudged by up to `offset` (rough.js
    /// `_curveWithOffset`).
    fn curve_stroke(&mut self, points: &[Point], offset: f64) -> Vec<Op> {
        let mut nudged = Vec::with_capacity(points.len() + 2);
        let nudge = |this: &mut Self, (x, y): Point| {
            let x = x + this.jitter(offset);
            (x, y + this.jitter(offset))
        };
        // The first and last points are doubled, so that the curve runs
        // through them.
        if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            nudged.push(nudge(self, first));
            nudged.push(nudge(self, first));
            for (i, &point) in points.iter().enumerate().skip(1) {
                nudged.push(nudge(self, point));
                if i == points.len() - 1 {
                    nudged.push(nudge(self, last));
                }
            }
        }
        match nudged[..] {
            [a, b] => {
                let mut ops = Vec::new();
                self.double_line(a, b, &mut ops);
                ops
            }
            _ if nudged.len() > 3 => curve(&nudged),
            _ => Vec::new(),
        }
    }

    /// A closed polygon through `points` (rough.js `polygon`; also its
    /// `rectangle`, which is one).
    pub(crate) fn polygon(&mut self, points: &[Point]) -> Drawing {
        let mut outline = Vec::new();
        if points.len() > 2 {
            for pair in points.windows(2) {
                self.double_line(pair[0], pair[1], &mut outline);
            }
            self.double_line(points[points.len() - 1], points[0], &mut outline);
        }
        let fill = self.style.fill.map(|fill| match fill {
            Fill::Solid => FillOps::Area(self.solid_polygons(&[points.to_vec()])),
            Fill::Hatch(hatch) => FillOps::Lines(self.hatch(&mut [points.to_vec()], hatch)),
        });
        Drawing { fill, outline }
    }

    /// An ellipse with the given centre and full width and height
    /// (rough.js `ellipse`).
    pub(crate) fn ellipse(&mut self, cx: f64, cy: f64, width: f64, height: f64) -> Drawing {
        let radius_mean = ((width / 2.0).powi(2) + (height / 2.0).powi(2)) / 2.0;
        let psq = (PI * 2.0 * radius_mean.sqrt()).sqrt();
        let steps = CURVE_STEP_COUNT
            .max(CURVE_STEP_COUNT / 200f64.sqrt() * psq)
            .ceil();
        let increment = PI * 2.0 / steps;
        let wobble = 1.0 - self.style.curve_fitting;
        let mut rx = (width / 2.0).abs();
        rx += self.jitter(rx * wobble);
        let mut ry = (height / 2.0).abs();
        ry += self.jitter(ry * wobble);

        let (outline, mut core) = self.sketched_ellipse(increment, (cx, cy), (rx, ry));
        let fill = self.style.fill.map(|fill| match fill {
            // A solid ellipse is sketched a second time, with fresh jitter,
            // and that sketch is the fill.
            Fill::Solid => FillOps::Area(self.sketched_ellipse(increment, (cx, cy), (rx, ry)).0),
            Fill::Hatch(hatch) => {
                FillOps::Lines(self.hatch(std::slice::from_mut(&mut core), hatch))
            }
        });
        Drawing { fill, outline }
    }

    /// The ops of one hand-drawn ellipse and the points it passes through
    /// (rough.js `ellipseWithParams`).
    fn sketched_ellipse(
        &mut self,
        increment: f64,
        centre: Point,
        radii: Point,
    ) -> (Vec<Op>, Vec<Point>) {
        let reach = self.offset(0.4, 1.0, 1.0);
        let overlap = increment * self.offset(0.1, reach, 1.0);
        let (outline, core) = self.ellipse_points(increment, centre, radii, 1.0, overlap);
        let mut ops = curve(&outline);
        if self.style.multi_stroke && self.style.roughness != 0.0 {
            let (second, _) = self.ellipse_points(increment, centre, radii, 1.5, 0.0);
            ops.extend(curve(&second));
        }
        (ops, core)
    }

    /// A path of straight and curved pieces, each drawn sketchily (rough.js
    /// `path`, which Excalidraw uses for rounded corners). The path is not
    /// closed for you: Excalidraw's paths end where they begin.
    pub(crate) fn path(&mut self, segments: &[Segment]) -> Drawing {
        let segments = to_cubics(segments);
        let outline = self.path_ops(&segments);
        let fill = self.style.fill.map(|fill| {
            // Polylines along the path, for everything but a solid fill of a
            // single closed curve.
            let distance = (1.0 + self.style.roughness) / 2.0;
            let mut polygons = points_on_path(&segments, distance);
            match fill {
                Fill::Solid if polygons.len() == 1 => {
                    // Sketched once and as one continuous path, a bit rougher.
                    let style = self.fill_shape_style();
                    let ops = self.with_style(style, |this| this.path_ops(&segments));
                    FillOps::Area(join_pieces(ops))
                }
                Fill::Solid => FillOps::Area(self.solid_polygons(&polygons)),
                Fill::Hatch(hatch) => FillOps::Lines(self.hatch(&mut polygons, hatch)),
            }
        });
        Drawing { fill, outline }
    }

    /// The strokes along a path without cubic curves left out (rough.js
    /// `svgPath`).
    fn path_ops(&mut self, segments: &[Segment]) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut current = (0.0, 0.0);
        for &segment in segments {
            match segment {
                Segment::Move(p) => current = p,
                Segment::Line(p) => {
                    self.double_line(current, p, &mut ops);
                    current = p;
                }
                Segment::Cubic(c1, c2, end) => {
                    self.bezier_to(c1, c2, end, current, &mut ops);
                    current = end;
                }
                Segment::Quad(..) => unreachable!("to_cubics removes quadratic curves"),
            }
        }
        ops
    }

    /// The style of the single stroke that a solid fill of a curve or path
    /// is sketched as: rougher than the outline, and drawn once (rough.js
    /// `fillShapeRoughnessGain`).
    fn fill_shape_style(&self) -> Style {
        let roughness = self.style.roughness;
        Style {
            roughness: if roughness == 0.0 {
                0.0
            } else {
                roughness + FILL_SHAPE_ROUGHNESS_GAIN
            },
            multi_stroke: false,
            ..self.style
        }
    }

    /// Runs `draw` with `style` instead of the sketcher's own.
    fn with_style<T>(&mut self, style: Style, draw: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.style, style);
        let result = draw(self);
        self.style = saved;
        result
    }

    // --- Fills --------------------------------------------------------------

    /// The outline of each polygon, every point nudged a little: a fill that
    /// is drawn solid still looks hand-made (rough.js `solidFillPolygon`).
    fn solid_polygons(&mut self, polygons: &[Vec<Point>]) -> Vec<Op> {
        let mut ops = Vec::new();
        for points in polygons.iter().filter(|points| points.len() > 2) {
            for (i, &(x, y)) in points.iter().enumerate() {
                let moved = (x + self.jitter(MAX_OFFSET), y + self.jitter(MAX_OFFSET));
                ops.push(if i == 0 {
                    Op::Move(moved)
                } else {
                    Op::Line(moved)
                });
            }
        }
        ops
    }

    /// Hatching across `polygons` (rough.js `patternFillPolygons`). The
    /// polygons are turned about the origin and back on the way, see
    /// [`hachure::hachure_lines`].
    fn hatch(&mut self, polygons: &mut [Vec<Point>], hatch: Hatch) -> Vec<Op> {
        let gap = hatch.gap.max(0.1);
        match hatch.pattern {
            Pattern::Hachure => {
                let lines = self.hatch_lines(polygons, gap, HACHURE_ANGLE);
                self.fill_strokes(&lines)
            }
            Pattern::CrossHatch => {
                let lines = self.hatch_lines(polygons, gap, HACHURE_ANGLE);
                let mut ops = self.fill_strokes(&lines);
                let across = self.hatch_lines(polygons, gap, HACHURE_ANGLE + 90.0);
                ops.extend(self.fill_strokes(&across));
                ops
            }
            Pattern::Zigzag => {
                let lines = self.hatch_lines(polygons, gap, HACHURE_ANGLE);
                let angle = PI / 180.0 * HACHURE_ANGLE;
                let (dx, dy) = (gap * 0.5 * angle.cos(), gap * 0.5 * angle.sin());
                // Every line becomes two that meet at its end.
                let mut bent = Vec::new();
                for [p1, p2] in lines {
                    if (p1.0 - p2.0).hypot(p1.1 - p2.1) != 0.0 {
                        bent.push([(p1.0 - dx, p1.1 + dy), p2]);
                        bent.push([(p1.0 + dx, p1.1 - dy), p2]);
                    }
                }
                self.fill_strokes(&bent)
            }
        }
    }

    /// The hatch lines at `angle` degrees. Rough shapes sometimes skip lines:
    /// rough.js decides with one random number (rough.js
    /// `polygonHachureLines`).
    fn hatch_lines(&mut self, polygons: &mut [Vec<Point>], gap: f64, angle: f64) -> Vec<Line> {
        let mut step = 1.0;
        if self.style.roughness >= 1.0 && self.random.next() > 0.7 {
            step = gap;
        }
        hachure::hachure_lines(polygons, gap, angle + 90.0, step)
    }

    /// Each line, drawn twice, however the outline is drawn.
    fn fill_strokes(&mut self, lines: &[Line]) -> Vec<Op> {
        let mut ops = Vec::new();
        for &[from, to] in lines {
            self.double_line_with(from, to, true, &mut ops);
        }
        ops
    }

    // --- Random numbers ---------------------------------------------------

    /// rough.js `_offset`: a random number in `min..max`, scaled by the
    /// roughness.
    fn offset(&mut self, min: f64, max: f64, gain: f64) -> f64 {
        self.style.roughness * gain * (self.random.next() * (max - min) + min)
    }

    /// rough.js `_offsetOpt`: a random number in `-x..x`.
    fn jitter(&mut self, x: f64) -> f64 {
        self.offset(-x, x, 1.0)
    }

    fn jitter_with_gain(&mut self, x: f64, gain: f64) -> f64 {
        self.offset(-x, x, gain)
    }

    // --- Strokes ----------------------------------------------------------

    /// A line drawn once, and once more over it unless multi-stroke is off.
    fn double_line(&mut self, from: Point, to: Point, ops: &mut Vec<Op>) {
        self.double_line_with(from, to, self.style.multi_stroke, ops);
    }

    fn double_line_with(&mut self, from: Point, to: Point, twice: bool, ops: &mut Vec<Op>) {
        self.line(from, to, false, ops);
        if twice {
            self.line(from, to, true, ops);
        }
    }

    /// One slightly bent, jittered stroke (rough.js `_line`). The overlay
    /// stroke uses half the jitter.
    fn line(&mut self, (x1, y1): Point, (x2, y2): Point, overlay: bool, ops: &mut Vec<Op>) {
        let length_sq = (x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2);
        let length = length_sq.sqrt();
        // Long lines are jittered less.
        let gain = if length < 200.0 {
            1.0
        } else if length > 500.0 {
            0.4
        } else {
            -0.0016668 * length + 1.233334
        };

        let mut offset = MAX_OFFSET;
        if offset * offset * 100.0 > length_sq {
            offset = length / 10.0;
        }
        let spread = if overlay { offset / 2.0 } else { offset };

        let diverge = 0.2 + self.random.next() * 0.2;
        let mut mid_x = BOWING * MAX_OFFSET * (y2 - y1) / 200.0;
        let mut mid_y = BOWING * MAX_OFFSET * (x1 - x2) / 200.0;
        mid_x = self.jitter_with_gain(mid_x, gain);
        mid_y = self.jitter_with_gain(mid_y, gain);

        let keep = self.style.preserve_vertices;
        let vertex = |this: &mut Self, v: f64| {
            if keep {
                v
            } else {
                v + this.jitter_with_gain(spread, gain)
            }
        };

        let start_x = vertex(self, x1);
        let start_y = vertex(self, y1);
        ops.push(Op::Move((start_x, start_y)));

        let c1x = mid_x + x1 + (x2 - x1) * diverge + self.jitter_with_gain(spread, gain);
        let c1y = mid_y + y1 + (y2 - y1) * diverge + self.jitter_with_gain(spread, gain);
        let c2x = mid_x + x1 + 2.0 * (x2 - x1) * diverge + self.jitter_with_gain(spread, gain);
        let c2y = mid_y + y1 + 2.0 * (y2 - y1) * diverge + self.jitter_with_gain(spread, gain);
        let end_x = vertex(self, x2);
        let end_y = vertex(self, y2);
        ops.push(Op::Cubic((c1x, c1y), (c2x, c2y), (end_x, end_y)));
    }

    /// A cubic curve drawn once or twice with jittered control points
    /// (rough.js `_bezierTo`).
    fn bezier_to(&mut self, c1: Point, c2: Point, end: Point, from: Point, ops: &mut Vec<Op>) {
        let spreads = [MAX_OFFSET, MAX_OFFSET + 0.3];
        let strokes = if self.style.multi_stroke { 2 } else { 1 };
        let keep = self.style.preserve_vertices;
        for (i, &spread) in spreads.iter().enumerate().take(strokes) {
            if i == 0 || keep {
                ops.push(Op::Move(from));
            } else {
                let x = from.0 + self.jitter(spreads[0]);
                let y = from.1 + self.jitter(spreads[0]);
                ops.push(Op::Move((x, y)));
            }
            // The end point is drawn first, as in rough.js.
            let end = if keep {
                end
            } else {
                let x = end.0 + self.jitter(spread);
                let y = end.1 + self.jitter(spread);
                (x, y)
            };
            let c1x = c1.0 + self.jitter(spread);
            let c1y = c1.1 + self.jitter(spread);
            let c2x = c2.0 + self.jitter(spread);
            let c2y = c2.1 + self.jitter(spread);
            ops.push(Op::Cubic((c1x, c1y), (c2x, c2y), end));
        }
    }

    /// The points an ellipse stroke runs through, including the lead-in and
    /// the overlapping lead-out that make it look hand-drawn
    /// (rough.js `_computeEllipsePoints`).
    fn ellipse_points(
        &mut self,
        increment: f64,
        (cx, cy): Point,
        (rx, ry): Point,
        offset: f64,
        overlap: f64,
    ) -> (Vec<Point>, Vec<Point>) {
        let mut points = Vec::new();
        // The points on the ellipse itself, without lead-in and lead-out.
        let mut core = Vec::new();
        if self.style.roughness == 0.0 {
            let increment = increment / 4.0;
            points.push((cx + rx * (-increment).cos(), cy + ry * (-increment).sin()));
            let mut angle = 0.0;
            while angle <= PI * 2.0 {
                let p = (cx + rx * angle.cos(), cy + ry * angle.sin());
                core.push(p);
                points.push(p);
                angle += increment;
            }
            points.push((cx + rx * 0f64.cos(), cy + ry * 0f64.sin()));
            points.push((cx + rx * increment.cos(), cy + ry * increment.sin()));
        } else {
            let start = self.jitter(0.5) - PI / 2.0;
            let on_ellipse = |this: &mut Self, scale: f64, angle: f64| {
                let x = this.jitter(offset) + cx + scale * rx * angle.cos();
                let y = this.jitter(offset) + cy + scale * ry * angle.sin();
                (x, y)
            };
            points.push(on_ellipse(self, 0.9, start - increment));
            let end = PI * 2.0 + start - 0.01;
            let mut angle = start;
            while angle < end {
                let p = on_ellipse(self, 1.0, angle);
                core.push(p);
                points.push(p);
                angle += increment;
            }
            points.push(on_ellipse(self, 1.0, start + PI * 2.0 + overlap * 0.5));
            points.push(on_ellipse(self, 0.98, start + overlap));
            points.push(on_ellipse(self, 0.9, start + overlap * 0.5));
        }
        (points, core)
    }
}

/// Keeps only the first move: the pieces of the shape join up (rough.js
/// `_mergedShape`).
pub(crate) fn join_pieces(mut ops: Vec<Op>) -> Vec<Op> {
    let mut index = 0;
    ops.retain(|op| {
        index += 1;
        index == 1 || !matches!(op, Op::Move(_))
    });
    ops
}

/// A smooth curve through `points`, drawn as one cubic per interior pair
/// (rough.js `_curve`, a Catmull-Rom spline). The first and last points only
/// shape the ends and are not on the curve.
fn curve(points: &[Point]) -> Vec<Op> {
    debug_assert!(points.len() > 3, "callers pass at least four points");
    let mut ops = vec![Op::Move(points[1])];
    for i in 1..points.len() - 2 {
        let (before, here, next, after) = (points[i - 1], points[i], points[i + 1], points[i + 2]);
        ops.push(Op::Cubic(
            (
                here.0 + (next.0 - before.0) / 6.0,
                here.1 + (next.1 - before.1) / 6.0,
            ),
            (
                next.0 + (here.0 - after.0) / 6.0,
                next.1 + (here.1 - after.1) / 6.0,
            ),
            next,
        ));
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_follows_the_lehmer_sequence_rough_js_computes_with_imul() {
        // 48271 * 1 = 48271; 48271^2 = 2330089441 wraps to -1964877855 in
        // int32, whose low 31 bits are 182605793.
        let mut random = Random::new(1.0);
        assert_eq!(random.next(), 48271.0 / 2147483648.0);
        assert_eq!(random.next(), 182_605_793.0 / 2147483648.0);
    }

    #[test]
    fn seeds_are_converted_like_toint32() {
        assert_eq!(to_int32(4_294_967_303.0), 7);
        assert_eq!(to_int32(2_147_483_648.0), i32::MIN);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(f64::NAN), 0);
    }
}
