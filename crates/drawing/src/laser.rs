//! The outline of a stroke of constant width: the parts of
//! `@excalidraw/laser-pointer` that Excalidraw's
//! `getConstantWidthFreedrawOutline` uses (`state.ts` and `math.ts`).
//!
//! Excalidraw pins the pressure to 1 and simplifies nothing, so every point
//! is as wide as the size. The loops step an angle by accumulating, like the
//! original, so they stop at the same point.

use crate::path::Point;
use std::f64::consts::PI;

/// A turn sharper than this many degrees is a corner.
const CORNER_MAX_ANGLE: f64 = 75.0;
/// Above this speed the corner detection is half as strict.
const CORNER_SPEED: f64 = 35.0;
/// A stroke thinner than this has no outline.
const MIN_SIZE: f64 = 0.5;
/// Caps are only rounded when they are wider than this.
const MIN_CAP_SIZE: f64 = 0.1;
const CAP_STEP: f64 = PI / 16.0;

/// The polygon around a stroke through `points`, `size` away from them on
/// each side, after smoothing the points by `streamline`.
pub(crate) fn outline(points: &[Point], size: f64, streamline: f64) -> Vec<Point> {
    let points = streamlined(points, streamline);
    match points[..] {
        [] => Vec::new(),
        [only] => dot(only, size),
        [first, second] => capsule(first, second, size),
        _ => stroke(&points, size),
    }
}

/// The points the pointer follows: each one lags behind the input by
/// `streamline`. A point equal to the one before it is dropped.
fn streamlined(input: &[Point], streamline: f64) -> Vec<Point> {
    let mut points: Vec<Point> = Vec::with_capacity(input.len());
    let mut previous: Option<Point> = None;
    for &point in input {
        if previous == Some(point) {
            continue;
        }
        previous = Some(point);
        match points.last() {
            Some(&last) if streamline > 0.0 => points.push(lerp(last, point, 1.0 - streamline)),
            _ => points.push(point),
        }
    }
    points
}

fn dot(centre: Point, size: f64) -> Vec<Point> {
    if size < MIN_SIZE {
        return Vec::new();
    }
    let mut ring: Vec<Point> = angles(0.0, 2.0 * PI)
        .map(|theta| add(centre, scale(rotate((1.0, 0.0), theta), size)))
        .collect();
    ring.push((centre.0 + size, centre.1));
    ring
}

/// Two points: a half circle round each, one after the other.
fn capsule(from: Point, to: Point, size: f64) -> Vec<Point> {
    if size < MIN_SIZE {
        return Vec::new();
    }
    let start = angle(from, (from.0, from.1 - 100.0), to);
    let half = |centre: Point, begin: f64| {
        angles(begin, begin + PI)
            .map(move |theta| add(centre, scale(rotate((1.0, 0.0), theta), size)))
    };
    let mut ring: Vec<Point> = half(from, start).collect();
    ring.extend(half(to, PI + start));
    ring.push(ring[0]);
    ring
}

/// Three points or more.
fn stroke(points: &[Point], size: f64) -> Vec<Point> {
    let len = points.len();
    let mut forward: Vec<Point> = Vec::new();
    let mut backward: Vec<Point> = Vec::new();
    let mut previous_speed = 0.0;
    let mut visible_start = 0;

    for i in 1..len - 1 {
        let (p, c, n) = (points[i - 1], points[i], points[i + 1]);
        let d = distance(p, c);
        let speed = previous_speed + (d - previous_speed) * 0.2;

        if size == 0.0 {
            visible_start = i + 1;
            continue;
        }

        let dir_pc = normalize(sub(p, c));
        let dir_nc = normalize(sub(n, c));
        let p1_dir_pc = rotate(dir_pc, PI / 2.0);
        let p2_dir_pc = rotate(dir_pc, -PI / 2.0);
        let p1_dir_nc = rotate(dir_nc, PI / 2.0);
        let p2_dir_nc = rotate(dir_nc, -PI / 2.0);

        let p1_pc = add(c, scale(p1_dir_pc, size));
        let p2_pc = add(c, scale(p2_dir_pc, size));
        let p1_nc = add(c, scale(p1_dir_nc, size));
        let p2_nc = add(c, scale(p2_dir_nc, size));

        let forward_turn = add(p1_dir_pc, p2_dir_nc);
        let backward_turn = add(p2_dir_pc, p1_dir_nc);
        let corner_forward = add(c, scale(direction_or(forward_turn, dir_pc), size));
        let corner_backward = add(c, scale(direction_or(backward_turn, dir_nc), size));

        let c_angle = normalize_angle(angle(c, p, n));
        let corner_angle =
            CORNER_MAX_ANGLE / 180.0 * PI * if speed > CORNER_SPEED { 0.5 } else { 1.0 };

        if c_angle.abs() < corner_angle {
            let turn = normalize_angle(PI - c_angle).abs();
            if turn == 0.0 {
                continue;
            }
            let arc_up = || angles_by(0.0, turn, turn / 4.0);
            let arc_down = || descending(turn, turn / 4.0);
            if c_angle < 0.0 {
                backward.extend([p2_pc, corner_backward]);
                forward.extend(arc_up().map(|t| add(c, rotate(scale(p1_dir_pc, size), t))));
                backward.extend(arc_down().map(|t| add(c, rotate(scale(p1_dir_pc, size), t))));
                backward.extend([corner_backward, p1_nc]);
            } else {
                forward.extend([p1_pc, corner_forward]);
                backward.extend(arc_up().map(|t| add(c, rotate(scale(p1_dir_pc, -size), -t))));
                forward.extend(arc_down().map(|t| add(c, rotate(scale(p1_dir_pc, -size), -t))));
                forward.extend([corner_forward, p2_nc]);
            }
        } else {
            forward.push(corner_forward);
            backward.push(corner_backward);
        }
        previous_speed = speed;
    }

    if visible_start >= len - 2 {
        return Vec::new();
    }

    let first = points[visible_start];
    let second = points[visible_start + 1];
    let penultimate = points[len - 2];
    let ultimate = points[len - 1];

    let perpendicular_start = rotate(normalize(sub(second, first)), -PI / 2.0);
    let perpendicular_end = rotate(normalize(sub(penultimate, ultimate)), PI / 2.0);

    // The start cap is built by unshifting, so it runs backwards.
    let mut start_cap: Vec<Point> = Vec::new();
    if size > MIN_CAP_SIZE {
        start_cap.extend(
            angles(0.0, PI).map(|t| add(first, rotate(scale(perpendicular_start, size), -t))),
        );
        start_cap.push(add(first, scale(perpendicular_start, -size)));
        start_cap.reverse();
    } else {
        start_cap.push(first);
    }

    let mut end_cap: Vec<Point> = angles(0.0, 3.0 * PI)
        .map(|t| add(ultimate, rotate(scale(perpendicular_end, -size), -t)))
        .collect();
    end_cap.reverse();
    backward.reverse();

    let mut outline = start_cap.clone();
    outline.extend(forward);
    outline.extend(end_cap);
    outline.extend(backward);
    outline.push(start_cap[0]);
    outline
}

/// `a`, `a + step`, … up to and including `end`, summed up like a `for` loop.
fn angles(start: f64, end: f64) -> impl Iterator<Item = f64> {
    angles_by(start, end, CAP_STEP)
}

fn angles_by(start: f64, end: f64, step: f64) -> impl Iterator<Item = f64> {
    let mut theta = start;
    std::iter::from_fn(move || {
        (theta <= end).then(|| {
            let current = theta;
            theta += step;
            current
        })
    })
}

/// `from`, `from - step`, … down to and including 0.
fn descending(from: f64, step: f64) -> impl Iterator<Item = f64> {
    let mut theta = from;
    std::iter::from_fn(move || {
        (theta >= 0.0).then(|| {
            let current = theta;
            theta -= step;
            current
        })
    })
}

fn add(a: Point, b: Point) -> Point {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}

fn scale(a: Point, s: f64) -> Point {
    (a.0 * s, a.1 * s)
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    add(a, scale(sub(b, a), t))
}

fn magnitude(a: Point) -> f64 {
    a.0.hypot(a.1)
}

fn distance(a: Point, b: Point) -> f64 {
    magnitude(sub(b, a))
}

/// The unit vector along `a`; not a number for the zero vector.
fn normalize(a: Point) -> Point {
    let length = (a.0 * a.0 + a.1 * a.1).sqrt();
    (a.0 / length, a.1 / length)
}

/// The unit vector along `turn`, or along `fallback` if `turn` is zero.
fn direction_or(turn: Point, fallback: Point) -> Point {
    if magnitude(turn) == 0.0 {
        fallback
    } else {
        normalize(turn)
    }
}

fn rotate(a: Point, radians: f64) -> Point {
    let (sin, cos) = radians.sin_cos();
    (cos * a.0 - sin * a.1, sin * a.0 + cos * a.1)
}

/// The angle at `p` from the direction of `p1` to the direction of `p2`.
fn angle(p: Point, p1: Point, p2: Point) -> f64 {
    (p2.1 - p.1).atan2(p2.0 - p.0) - (p1.1 - p.1).atan2(p1.0 - p.0)
}

fn normalize_angle(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_points_have_no_outline() {
        assert_eq!(outline(&[], 2.8, 0.5), Vec::new());
    }

    #[test]
    fn a_stroke_too_thin_to_see_has_no_outline() {
        assert_eq!(outline(&[(0.0, 0.0)], 0.4, 0.5), Vec::new());
    }

    #[test]
    fn a_point_is_a_circle_round_it_closed_on_its_start() {
        let ring = outline(&[(5.0, 5.0)], 2.0, 0.5);

        assert_eq!(ring.len(), 33);
        assert_eq!(ring[0], (7.0, 5.0));
        assert_eq!(ring[32], (7.0, 5.0));
        assert!((ring[8].0 - 5.0).abs() < 1e-9 && (ring[8].1 - 7.0).abs() < 1e-9);
    }

    #[test]
    fn repeated_points_count_once() {
        let once = outline(&[(0.0, 0.0), (9.0, 0.0)], 1.0, 0.5);
        let twice = outline(&[(0.0, 0.0), (0.0, 0.0), (9.0, 0.0)], 1.0, 0.5);

        assert_eq!(once, twice);
    }
}
