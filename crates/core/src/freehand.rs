//! Pressure-sensitive stroke outlines (perfect-freehand 1.2.0, MIT), used for
//! freedraw (ADR-0004).
//!
//! An idiomatic port of the parts Excalidraw uses: strokes that end where
//! their last point is (`last`), with round caps, and no tapering. The
//! arithmetic keeps the order of the JavaScript, including its quirks, so
//! that the outline is the one excalidraw.com computes.

use crate::path::Point;

/// How quickly simulated pressure follows the speed of the pointer.
const RATE_OF_PRESSURE_CHANGE: f64 = 0.275;
/// Caps are half turns of a little more than π, so that they overlap the
/// stroke slightly.
const FIXED_PI: f64 = std::f64::consts::PI + 0.0001;
/// A stroke point this close to the end (along the stroke) is left out, so
/// that the last piece does not wiggle.
const END_CLEARANCE: f64 = 3.0;

/// One point of the pointer's path and, if it was measured, its pressure
/// (`0.0..=1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sample {
    pub point: Point,
    pub pressure: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Diameter of the stroke at full pressure, before `easing`.
    pub size: f64,
    /// How much pressure changes the width: `0` for a constant width.
    pub thinning: f64,
    /// How much the sides of the outline are smoothed.
    pub smoothing: f64,
    /// How much the path is smoothed before the outline is made.
    pub streamline: f64,
    /// Make pressure up from the speed of the pointer instead of using the
    /// pressure of the samples.
    pub simulate_pressure: bool,
    /// Maps pressure to the share of `size` that is the radius.
    pub easing: fn(f64) -> f64,
}

/// A smoothed point of the path that the outline is made around.
#[derive(Debug, Clone, Copy)]
struct StrokePoint {
    point: Point,
    pressure: f64,
    /// Unit vector from this point back to the one before.
    vector: Point,
    /// Distance from the point before.
    distance: f64,
    /// Distance from the start along the stroke.
    running_length: f64,
}

/// The closed polygon around the stroke through `samples`; empty if there
/// is nothing to draw.
pub(crate) fn outline(samples: &[Sample], options: &Options) -> Vec<Point> {
    outline_of(&stroke_points(samples, options), options)
}

// --- Smoothing --------------------------------------------------------------

fn stroke_points(samples: &[Sample], options: &Options) -> Vec<StrokePoint> {
    let Some(&first) = samples.first() else {
        return Vec::new();
    };
    let mut samples = samples.to_vec();
    match samples[..] {
        [_, last] => {
            // Two points are spread into five, which have no pressure of
            // their own (the pressure of `last` is lost).
            samples.truncate(1);
            samples.extend((1..5).map(|i| Sample {
                point: lerp(first.point, last.point, f64::from(i) / 4.0),
                pressure: None,
            }));
        }
        [_] => samples.push(Sample {
            point: add(first.point, (1.0, 1.0)),
            pressure: first.pressure,
        }),
        _ => {}
    }

    let t = 0.15 + (1.0 - options.streamline) * 0.85;
    let mut points = vec![StrokePoint {
        point: first.point,
        pressure: first.pressure.filter(|p| *p >= 0.0).unwrap_or(0.25),
        vector: (1.0, 1.0),
        distance: 0.0,
        running_length: 0.0,
    }];
    let mut reached_minimum_length = false;
    let mut running_length = 0.0;
    let last_index = samples.len() - 1;

    for (i, sample) in samples.iter().enumerate().skip(1) {
        let previous = *points.last().expect("the first point");
        // The end of the stroke is where the pointer ended, not a smoothed
        // point on the way.
        let point = if i == last_index {
            sample.point
        } else {
            lerp(previous.point, sample.point, t)
        };
        if point == previous.point {
            continue;
        }
        let distance = dist(point, previous.point);
        running_length += distance;
        // Points within the first `size` of the stroke are dropped, so that
        // the jitter of a pointer that is just touching down is not drawn.
        if i < last_index && !reached_minimum_length {
            if running_length < options.size {
                continue;
            }
            reached_minimum_length = true;
        }
        points.push(StrokePoint {
            point,
            pressure: sample.pressure.filter(|p| *p >= 0.0).unwrap_or(0.5),
            vector: unit(sub(previous.point, point)),
            distance,
            running_length,
        });
    }
    points[0].vector = points.get(1).map_or((0.0, 0.0), |p| p.vector);
    points
}

// --- Outline ----------------------------------------------------------------

fn outline_of(points: &[StrokePoint], options: &Options) -> Vec<Point> {
    let Options {
        size,
        thinning,
        smoothing,
        simulate_pressure,
        easing,
        ..
    } = *options;
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    if size <= 0.0 {
        return Vec::new();
    }

    let total_length = last.running_length;
    let min_distance_sq = (size * smoothing).powi(2);
    let radius_of = |pressure| size * easing(0.5 - thinning * (0.5 - pressure));
    let simulated = |previous: f64, point: &StrokePoint| {
        let speed = (point.distance / size).min(1.0);
        let slowness = (1.0 - speed).min(1.0);
        (previous + (slowness - previous) * (speed * RATE_OF_PRESSURE_CHANGE)).min(1.0)
    };

    // The pressure that simulation starts from: the mean over the first ten
    // points, each following the speed.
    let mut previous_pressure = points.iter().take(10).fold(first.pressure, |acc, point| {
        let pressure = if simulate_pressure {
            simulated(acc, point)
        } else {
            point.pressure
        };
        (acc + pressure) / 2.0
    });

    let mut radius = radius_of(last.pressure);
    let mut first_radius = None;
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut previous_vector = first.vector;
    let mut last_left = first.point;
    let mut last_right = first.point;
    let mut previous_was_sharp = false;

    for (i, point) in points.iter().enumerate() {
        let is_last = i == points.len() - 1;
        if !is_last && total_length - point.running_length < END_CLEARANCE {
            continue;
        }

        let mut pressure = point.pressure;
        if thinning != 0.0 {
            if simulate_pressure {
                pressure = simulated(previous_pressure, point);
            }
            radius = radius_of(pressure);
        } else {
            radius = size / 2.0;
        }
        first_radius.get_or_insert(radius);
        radius = radius.max(0.01);

        let next_vector = points.get(i + 1).unwrap_or(point).vector;
        let next_dot = if is_last {
            1.0
        } else {
            dot(point.vector, next_vector)
        };
        let previous_dot = dot(point.vector, previous_vector);
        let is_sharp = previous_dot < 0.0 && !previous_was_sharp;
        let next_is_sharp = next_dot < 0.0;

        // A sharp turn gets a round join: half a turn of points around it.
        if is_sharp || next_is_sharp {
            let offset = mul(perpendicular(previous_vector), radius);
            let step = 1.0 / 13.0;
            let mut t = 0.0;
            while t <= 1.0 {
                last_left = rotate(sub(point.point, offset), point.point, FIXED_PI * t);
                left.push(last_left);
                last_right = rotate(add(point.point, offset), point.point, FIXED_PI * -t);
                right.push(last_right);
                t += step;
            }
            if next_is_sharp {
                previous_was_sharp = true;
            }
            continue;
        }
        previous_was_sharp = false;

        if is_last {
            let offset = mul(perpendicular(point.vector), radius);
            left.push(sub(point.point, offset));
            right.push(add(point.point, offset));
            continue;
        }

        let offset = mul(
            perpendicular(lerp(next_vector, point.vector, next_dot)),
            radius,
        );
        let on_left = sub(point.point, offset);
        if i <= 1 || dist_sq(last_left, on_left) > min_distance_sq {
            left.push(on_left);
            last_left = on_left;
        }
        let on_right = add(point.point, offset);
        if i <= 1 || dist_sq(last_right, on_right) > min_distance_sq {
            right.push(on_right);
            last_right = on_right;
        }
        previous_pressure = pressure;
        previous_vector = point.vector;
    }

    let start = first.point;
    if points.len() == 1 {
        // A dot: a full circle around the point.
        let from = add(
            start,
            mul(
                unit(perpendicular(sub(start, add(start, (1.0, 1.0))))),
                -first_radius.unwrap_or(radius),
            ),
        );
        let step = 1.0 / 13.0;
        let mut circle = Vec::new();
        let mut t = step;
        while t <= 1.0 {
            circle.push(rotate(from, start, FIXED_PI * 2.0 * t));
            t += step;
        }
        return circle;
    }

    let end = last.point;
    let mut start_cap = Vec::new();
    let step = 1.0 / 13.0;
    let mut t = step;
    while t <= 1.0 {
        start_cap.push(rotate(right[0], start, FIXED_PI * t));
        t += step;
    }
    let direction = perpendicular(neg(last.vector));
    let from = add(end, mul(direction, radius));
    let mut end_cap = Vec::new();
    let step = 1.0 / 29.0;
    let mut t = step;
    while t < 1.0 {
        end_cap.push(rotate(from, end, FIXED_PI * 3.0 * t));
        t += step;
    }

    right.reverse();
    [left, end_cap, right, start_cap].concat()
}

// --- Vectors ----------------------------------------------------------------

fn neg((x, y): Point) -> Point {
    (-x, -y)
}

fn add(a: Point, b: Point) -> Point {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}

fn mul((x, y): Point, factor: f64) -> Point {
    (x * factor, y * factor)
}

/// The vector turned a quarter turn (the perpendicular of `(x, y)` is
/// `(y, -x)`).
fn perpendicular((x, y): Point) -> Point {
    (y, -x)
}

fn dot(a: Point, b: Point) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn unit((x, y): Point) -> Point {
    let length = x.hypot(y);
    (x / length, y / length)
}

fn dist(a: Point, b: Point) -> f64 {
    (a.1 - b.1).hypot(a.0 - b.0)
}

fn dist_sq(a: Point, b: Point) -> f64 {
    let (dx, dy) = sub(a, b);
    dx * dx + dy * dy
}

/// `from + (to - from) * t`.
fn lerp(from: Point, to: Point, t: f64) -> Point {
    add(from, mul(sub(to, from), t))
}

/// `point` turned by `angle` about `centre`.
fn rotate(point: Point, centre: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    let (px, py) = sub(point, centre);
    (
        px * cos - py * sin + centre.0,
        px * sin + py * cos + centre.1,
    )
}
