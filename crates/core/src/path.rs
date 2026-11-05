//! Paths as SVG describes them, and their conversion to polylines: what the
//! rough.js generators need to draw rounded shapes and to fill shapes
//! (rough.js' `path-data-parser`, `points-on-curve` and `points-on-path`,
//! MIT).
//!
//! The arithmetic keeps the shape of the JavaScript, because the polylines
//! feed the fill patterns and must agree with excalidraw.com's.

pub(crate) type Point = (f64, f64);

/// A piece of an SVG-like path, absolute coordinates only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Segment {
    Move(Point),
    Line(Point),
    /// Control point, end point.
    Quad(Point, Point),
    /// Control point, control point, end point.
    Cubic(Point, Point, Point),
}

/// The same path with quadratic curves turned into cubic ones, so that only
/// moves, lines and cubics remain (SVG `normalize`).
pub(crate) fn to_cubics(segments: &[Segment]) -> Vec<Segment> {
    let mut current = (0.0, 0.0);
    segments
        .iter()
        .map(|&segment| {
            let normalized = match segment {
                Segment::Quad(control, end) => Segment::Cubic(
                    (
                        current.0 + 2.0 * (control.0 - current.0) / 3.0,
                        current.1 + 2.0 * (control.1 - current.1) / 3.0,
                    ),
                    (
                        end.0 + 2.0 * (control.0 - end.0) / 3.0,
                        end.1 + 2.0 * (control.1 - end.1) / 3.0,
                    ),
                    end,
                ),
                other => other,
            };
            current = match segment {
                Segment::Move(p)
                | Segment::Line(p)
                | Segment::Quad(_, p)
                | Segment::Cubic(_, _, p) => p,
            };
            normalized
        })
        .collect()
}

/// Flattens a path (already free of quadratic curves, see [`to_cubics`]) into
/// one polyline per subpath. Curves are cut into chords within `1` unit of the
/// curve, then every polyline is simplified to within `distance` of itself
/// (rough.js `pointsOnPath(d, 1, distance)`).
pub(crate) fn points_on_path(segments: &[Segment], distance: f64) -> Vec<Vec<Point>> {
    const TOLERANCE: f64 = 1.0;
    let mut sets = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut start = (0.0, 0.0);
    let mut pending_curve: Vec<Point> = Vec::new();

    fn flush_curve(current: &mut Vec<Point>, pending: &mut Vec<Point>) {
        if pending.len() >= 4 {
            current.extend(points_on_bezier_curves(pending, TOLERANCE));
        }
        pending.clear();
    }

    for &segment in segments {
        match segment {
            Segment::Move(p) => {
                flush_curve(&mut current, &mut pending_curve);
                if !current.is_empty() {
                    sets.push(std::mem::take(&mut current));
                }
                start = p;
                current.push(start);
            }
            Segment::Line(p) => {
                flush_curve(&mut current, &mut pending_curve);
                current.push(p);
            }
            Segment::Cubic(c1, c2, p) => {
                if pending_curve.is_empty() {
                    pending_curve.push(current.last().copied().unwrap_or(start));
                }
                pending_curve.extend([c1, c2, p]);
            }
            Segment::Quad(..) => unreachable!("to_cubics removes quadratic curves"),
        }
    }
    flush_curve(&mut current, &mut pending_curve);
    if !current.is_empty() {
        sets.push(current);
    }

    sets.iter()
        .map(|set| simplify(set, distance))
        .filter(|set| !set.is_empty())
        .collect()
}

/// A polyline along the smooth curve through at least three `points`: the
/// curve is cut into chords within `tolerance` of it, then the polyline is
/// simplified to within `distance` of itself (points-on-curve
/// `pointsOnBezierCurves(curveToBezier(points), tolerance, distance)`).
pub(crate) fn curve_polyline(points: &[Point], tolerance: f64, distance: f64) -> Vec<Point> {
    let polyline = points_on_bezier_curves(&curve_to_bezier(points), tolerance);
    if distance > 0.0 {
        simplify(&polyline, distance)
    } else {
        polyline
    }
}

/// The Bézier chain of a Catmull-Rom spline through `points` (at least
/// three): start point, then two control points and an end point per curve.
fn curve_to_bezier(points: &[Point]) -> Vec<Point> {
    if let [a, b, c] = *points {
        return vec![a, b, c, c];
    }
    // The ends are doubled, so that the curve runs through them.
    let mut padded = vec![points[0], points[0]];
    padded.extend_from_slice(&points[1..]);
    padded.push(points[points.len() - 1]);

    let mut chain = vec![padded[0]];
    for i in 1..padded.len() - 2 {
        let (before, here, next, after) = (padded[i - 1], padded[i], padded[i + 1], padded[i + 2]);
        chain.push((
            here.0 + (next.0 - before.0) / 6.0,
            here.1 + (next.1 - before.1) / 6.0,
        ));
        chain.push((
            next.0 + (here.0 - after.0) / 6.0,
            next.1 + (here.1 - after.1) / 6.0,
        ));
        chain.push(next);
    }
    chain
}

/// Points on the chain of cubic Béziers through `points` (start point, then
/// two control points and an end point per curve), by subdividing until each
/// piece is flat to within `tolerance`.
fn points_on_bezier_curves(points: &[Point], tolerance: f64) -> Vec<Point> {
    let mut out = Vec::new();
    for curve in 0..(points.len() - 1) / 3 {
        let offset = curve * 3;
        let piece = [
            points[offset],
            points[offset + 1],
            points[offset + 2],
            points[offset + 3],
        ];
        split_until_flat(piece, tolerance, MAX_SPLITS, &mut out);
    }
    out
}

/// How often one curve is halved at most. Each halving cuts the distance to
/// the chord to a quarter, so ten reach any curve of a sane size; the limit
/// only stops curves whose coordinates are huge or not numbers, which would
/// never get flat (and would exhaust the stack and memory).
const MAX_SPLITS: u32 = 10;

fn split_until_flat(curve: [Point; 4], tolerance: f64, splits_left: u32, out: &mut Vec<Point>) {
    if splits_left == 0 || flatness(&curve) < tolerance {
        let start = curve[0];
        match out.last() {
            Some(&last) => {
                if distance(last, start) > 1.0 {
                    out.push(start);
                }
            }
            None => out.push(start),
        }
        out.push(curve[3]);
    } else {
        let [p1, p2, p3, p4] = curve;
        let (q1, q2, q3) = (lerp(p1, p2), lerp(p2, p3), lerp(p3, p4));
        let (r1, r2) = (lerp(q1, q2), lerp(q2, q3));
        let middle = lerp(r1, r2);
        split_until_flat([p1, q1, r1, middle], tolerance, splits_left - 1, out);
        split_until_flat([middle, r2, q3, p4], tolerance, splits_left - 1, out);
    }
}

/// How far a cubic strays from its chord, up to a constant factor and
/// squared (the bound by Sean Tracey that points-on-curve uses).
fn flatness(&[p1, p2, p3, p4]: &[Point; 4]) -> f64 {
    let mut ux = 3.0 * p2.0 - 2.0 * p1.0 - p4.0;
    ux *= ux;
    let mut uy = 3.0 * p2.1 - 2.0 * p1.1 - p4.1;
    uy *= uy;
    let mut vx = 3.0 * p3.0 - 2.0 * p4.0 - p1.0;
    vx *= vx;
    let mut vy = 3.0 * p3.1 - 2.0 * p4.1 - p1.1;
    vy *= vy;
    if ux < vx {
        ux = vx;
    }
    if uy < vy {
        uy = vy;
    }
    ux + uy
}

/// Ramer-Douglas-Peucker: drops points that lie within `epsilon` of the line
/// between their neighbours that are kept.
pub(crate) fn simplify(points: &[Point], epsilon: f64) -> Vec<Point> {
    let mut out = Vec::new();
    if !points.is_empty() {
        simplify_range(points, 0, points.len(), epsilon, &mut out);
    }
    out
}

fn simplify_range(points: &[Point], start: usize, end: usize, epsilon: f64, out: &mut Vec<Point>) {
    let (first, last) = (points[start], points[end - 1]);
    let mut max_distance_sq = 0.0;
    // As in the original, the index to split at starts out as 1 and not as
    // `start + 1`; it is only used when some point is farther than `epsilon`.
    let mut split = 1;
    for (i, &p) in points.iter().enumerate().take(end - 1).skip(start + 1) {
        let d = distance_to_segment_sq(p, first, last);
        if d > max_distance_sq {
            max_distance_sq = d;
            split = i;
        }
    }
    if max_distance_sq.sqrt() > epsilon {
        simplify_range(points, start, split + 1, epsilon, out);
        simplify_range(points, split, end, epsilon, out);
    } else {
        if out.is_empty() {
            out.push(first);
        }
        out.push(last);
    }
}

fn distance_to_segment_sq(p: Point, v: Point, w: Point) -> f64 {
    let l2 = distance_sq(v, w);
    if l2 == 0.0 {
        return distance_sq(p, v);
    }
    let t = (((p.0 - v.0) * (w.0 - v.0) + (p.1 - v.1) * (w.1 - v.1)) / l2).clamp(0.0, 1.0);
    distance_sq(p, (v.0 + (w.0 - v.0) * t, v.1 + (w.1 - v.1) * t))
}

fn distance_sq(a: Point, b: Point) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn distance(a: Point, b: Point) -> f64 {
    distance_sq(a, b).sqrt()
}

fn lerp(a: Point, b: Point) -> Point {
    (a.0 + (b.0 - a.0) * 0.5, a.1 + (b.1 - a.1) * 0.5)
}

/// How close the end of a line must come to its start to close a loop that
/// can be filled.
const LOOP_REACH: f64 = 8.0;

/// Whether `points` start and end at (nearly) the same place.
pub(crate) fn is_loop(points: &[Point]) -> bool {
    match (points.first(), points.last()) {
        (Some(first), Some(last)) if points.len() >= 3 => {
            (first.0 - last.0).hypot(first.1 - last.1) <= LOOP_REACH
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lines_that_return_to_their_start_are_loops() {
        assert!(is_loop(&[
            (0.0, 0.0),
            (50.0, 0.0),
            (50.0, 50.0),
            (3.0, 4.0)
        ]));
        assert!(!is_loop(&[
            (0.0, 0.0),
            (50.0, 0.0),
            (50.0, 50.0),
            (9.0, 0.0)
        ]));
        assert!(!is_loop(&[(0.0, 0.0), (0.0, 0.0)]), "needs three points");
    }

    #[test]
    fn quadratic_curves_become_cubics_with_controls_two_thirds_along() {
        let path = [
            Segment::Move((0.0, 0.0)),
            Segment::Quad((30.0, 0.0), (30.0, 30.0)),
        ];

        assert_eq!(
            to_cubics(&path),
            [
                Segment::Move((0.0, 0.0)),
                Segment::Cubic((20.0, 0.0), (30.0, 10.0), (30.0, 30.0)),
            ]
        );
    }

    #[test]
    fn straight_pieces_stay_corners_when_simplified() {
        let square = [
            Segment::Move((0.0, 0.0)),
            Segment::Line((10.0, 0.0)),
            Segment::Line((10.0, 10.0)),
            Segment::Line((0.0, 10.0)),
            Segment::Line((0.0, 0.0)),
        ];

        assert_eq!(
            points_on_path(&square, 0.5),
            [vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0)
            ]]
        );
    }

    #[test]
    fn points_on_a_straight_run_are_dropped() {
        let line = [
            Segment::Move((0.0, 0.0)),
            Segment::Line((5.0, 0.0)),
            Segment::Line((10.0, 0.0)),
        ];

        assert_eq!(points_on_path(&line, 0.5), [vec![(0.0, 0.0), (10.0, 0.0)]]);
    }

    #[test]
    fn a_curve_is_cut_into_chords_that_stay_close_to_it() {
        // A quarter circle of radius 100 as one cubic.
        let k = 0.5522847498 * 100.0;
        let quarter = [
            Segment::Move((100.0, 0.0)),
            Segment::Cubic((100.0, k), (k, 100.0), (0.0, 100.0)),
        ];

        let [set] = points_on_path(&quarter, 0.5).try_into().unwrap();

        assert_eq!(set.first(), Some(&(100.0, 0.0)));
        assert_eq!(set.last(), Some(&(0.0, 100.0)));
        assert!(set.len() > 4, "a round curve needs several chords");
        for (x, y) in set {
            let radius = x.hypot(y);
            assert!((radius - 100.0).abs() < 1.5, "({x}, {y}) is off the circle");
        }
    }

    #[test]
    fn every_move_starts_another_polyline() {
        let two = [
            Segment::Move((0.0, 0.0)),
            Segment::Line((10.0, 0.0)),
            Segment::Move((0.0, 5.0)),
            Segment::Line((10.0, 5.0)),
        ];

        assert_eq!(
            points_on_path(&two, 0.5),
            [vec![(0.0, 0.0), (10.0, 0.0)], vec![(0.0, 5.0), (10.0, 5.0)]]
        );
    }

    #[test]
    fn a_curve_of_absurd_size_is_cut_into_a_bounded_number_of_chords() {
        // Cut until flat, this one would need thousands of chords.
        let polyline = curve_polyline(&[(0.0, 0.0), (1e5, 1e5), (2e5, 0.0)], 0.15, 0.0);

        assert!(
            polyline.len() <= (1 << MAX_SPLITS) + 1,
            "{}",
            polyline.len()
        );
    }
}
