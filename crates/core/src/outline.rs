//! The outline of a shape that arrows can bind to (`collision.ts` and
//! `binding.ts` at the pinned commit, redone as one distance function).
//!
//! Rectangles, diamonds, ellipses, images and free texts are bindable. Each is
//! described by its box, its angle and its form, and answers one question: how
//! far is a point from its outline, [`Outline::distance`]. Everything binding
//! needs follows from that:
//!
//! * which shape an arrow end is near enough to bind to,
//! * where the end sits when the arrow stops short of the shape (the points at
//!   a given distance from the outline, [`Outline::crossings`]),
//! * the highlight drawn around a shape that would be bound, and
//! * the fixed point, an arrow's anchor on the shape as fractions of its box.
//!
//! The three forms are convex, so their signed distance is a convex function:
//! along any line it has a single minimum, and the crossings of a distance are
//! found by searching either side of it. Roundness of diamonds is ignored; a
//! diamond binds as the sharp one.

use crate::frame::Frame;
use crate::path::Point;
use crate::scene::{Element, ElementType};
use crate::shape::corner_radius;

/// Excalidraw's `MIN_BINDABLE_SIZE`: below this a box has no interior to
/// anchor a fixed point in.
const MIN_BINDABLE_SIZE: f64 = 1.0;
/// Excalidraw's bound on how far outside the box a fixed point may lie, in
/// boxes.
const FIXED_POINT_BOUND: f64 = 10.0;
/// Excalidraw does not allow a ratio of exactly one half, so that an arrow
/// does not jump between headings by rounding error.
const HALF: f64 = 0.5;
const HALF_EPSILON: f64 = 0.0001;

#[derive(Debug, Clone, Copy)]
enum Form {
    /// With the radius of its rounded corners, 0 for sharp ones.
    Rectangle(f64),
    Diamond,
    Ellipse,
}

/// A bindable shape: where it is, its angle and its form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Outline {
    frame: Frame,
    angle: f64,
    form: Form,
}

impl Outline {
    /// The outline of `element`, if arrows can bind to it: a rectangle,
    /// diamond, ellipse or image, or a text that is not a label.
    pub fn of(element: &Element) -> Option<Self> {
        let form = match element.kind() {
            ElementType::Rectangle => {
                let side = element.width().abs().min(element.height().abs());
                Form::Rectangle(
                    element
                        .roundness()
                        .map_or(0.0, |roundness| corner_radius(side, roundness)),
                )
            }
            ElementType::Image => Form::Rectangle(0.0),
            ElementType::Text if element.container_id().is_none() => Form::Rectangle(0.0),
            ElementType::Diamond => Form::Diamond,
            ElementType::Ellipse => Form::Ellipse,
            _ => return None,
        };
        Some(Self {
            frame: Frame::of(element),
            angle: element.angle(),
            form,
        })
    }

    /// The area of the box.
    pub fn area(&self) -> f64 {
        self.frame.width() * self.frame.height()
    }

    /// The box, not turned.
    pub fn frame(&self) -> Frame {
        self.frame
    }

    /// How far `point` is from the outline: negative inside, positive
    /// outside.
    pub fn distance(&self, point: Point) -> f64 {
        let (x, y) = self.frame.unturned(point, self.angle);
        let (cx, cy) = self.frame.centre();
        let (px, py) = ((x - cx).abs(), (y - cy).abs());
        let (a, b) = (self.frame.width() / 2.0, self.frame.height() / 2.0);
        match self.form {
            Form::Rectangle(radius) => {
                let (qx, qy) = (px - a + radius, py - b + radius);
                qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
            }
            Form::Diamond => diamond_distance(px, py, a, b),
            Form::Ellipse => ellipse_distance(px, py, a, b),
        }
    }

    /// The points of the segment from `from` to `to` that are `offset` from
    /// the outline, in order from `from`: none, one or two.
    pub fn crossings(&self, from: Point, to: Point, offset: f64) -> Vec<Point> {
        let at = |t: f64| (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        let gap = |t: f64| self.distance(at(t)) - offset;
        // The distance along a line is convex: find its lowest point first.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..SEARCH_STEPS {
            let (m1, m2) = (low + (high - low) / 3.0, high - (high - low) / 3.0);
            if gap(m1) < gap(m2) {
                high = m2;
            } else {
                low = m1;
            }
        }
        let lowest = (low + high) / 2.0;
        if gap(lowest) > 0.0 {
            return Vec::new();
        }
        let mut found = Vec::new();
        if gap(0.0) > 0.0 {
            found.push(at(root(&gap, 0.0, lowest)));
        }
        if gap(1.0) > 0.0 {
            found.push(at(root(&gap, 1.0, lowest)));
        }
        found
    }

    /// A closed polygon around the shape at `offset` from its outline: what
    /// the highlight of a shape that arrows would bind to is drawn along.
    pub fn contour(&self, offset: f64) -> Vec<Point> {
        let centre = self.frame.centre();
        let reach = self.frame.width() + self.frame.height() + 2.0 * offset.abs() + 1.0;
        (0..CONTOUR_RAYS)
            .map(|i| {
                // Rays go out turned with the shape, so that the sides of a
                // box are reached head on.
                let direction =
                    std::f64::consts::TAU * f64::from(i) / f64::from(CONTOUR_RAYS) + self.angle;
                let (sin, cos) = direction.sin_cos();
                let end = (centre.0 + reach * cos, centre.1 + reach * sin);
                let gap = |t: f64| {
                    self.distance((
                        centre.0 + (end.0 - centre.0) * t,
                        centre.1 + (end.1 - centre.1) * t,
                    )) - offset
                };
                let t = root(&gap, 1.0, 0.0);
                (
                    centre.0 + (end.0 - centre.0) * t,
                    centre.1 + (end.1 - centre.1) * t,
                )
            })
            .collect()
    }

    /// Where `point` is on the box as fractions of its width and height,
    /// seen from the shape when not turned (Excalidraw's `fixedPoint`).
    pub fn fixed_point(&self, point: Point) -> (f64, f64) {
        let (x, y) = self.frame.unturned(point, self.angle);
        if self.frame.width() < MIN_BINDABLE_SIZE || self.frame.height() < MIN_BINDABLE_SIZE {
            return normalized((HALF, HALF));
        }
        normalized((
            (x - self.frame.min.0) / self.frame.width(),
            (y - self.frame.min.1) / self.frame.height(),
        ))
    }

    /// The point of the Scene at the fixed point `ratio` of the box.
    pub fn at_fixed_point(&self, ratio: (f64, f64)) -> Point {
        let (fx, fy) = normalized(ratio);
        let unturned = (
            self.frame.min.0 + self.frame.width() * fx,
            self.frame.min.1 + self.frame.height() * fy,
        );
        self.frame.turned(unturned, self.angle)
    }
}

/// Steps of the searches for a lowest point (a third of the interval is cut
/// off by each) and for a crossing (it is halved): far finer than any
/// coordinate.
const SEARCH_STEPS: usize = 80;
/// How many rays make a [`Outline::contour`]; a multiple of four so that the
/// sides of a box are reached head on.
const CONTOUR_RAYS: u32 = 64;

/// The `t` between `from` (where `f` is positive) and `to` (where it is not)
/// at which `f` is zero.
fn root(f: &dyn Fn(f64) -> f64, mut from: f64, mut to: f64) -> f64 {
    for _ in 0..SEARCH_STEPS {
        let middle = (from + to) / 2.0;
        if f(middle) > 0.0 {
            from = middle;
        } else {
            to = middle;
        }
    }
    (from + to) / 2.0
}

/// Excalidraw's `normalizeFixedPoint`: ratios stay near the box, and none is
/// exactly a half.
pub(crate) fn normalized((x, y): (f64, f64)) -> (f64, f64) {
    let ratio = |r: f64| {
        let r = if r.is_finite() { r } else { HALF };
        let r = r.clamp(-FIXED_POINT_BOUND, FIXED_POINT_BOUND);
        if (r - HALF).abs() < HALF_EPSILON {
            0.5001
        } else {
            r
        }
    };
    (ratio(x), ratio(y))
}

/// The distance of `(px, py)`, in the first quadrant, from a diamond with
/// half-diagonals `a` and `b`.
fn diamond_distance(px: f64, py: f64, a: f64, b: f64) -> f64 {
    let scale = a * a + b * b;
    if scale == 0.0 {
        return px.hypot(py);
    }
    // Inigo Quilez' rhombus: the nearest point on the edge from the corner
    // on the x axis to the one on the y axis.
    let h = (((a - 2.0 * px) * a - (b - 2.0 * py) * b) / scale).clamp(-1.0, 1.0);
    let nearest = (0.5 * a * (1.0 - h), 0.5 * b * (1.0 + h));
    let distance = (px - nearest.0).hypot(py - nearest.1);
    if px * b + py * a - a * b > 0.0 {
        distance
    } else {
        -distance
    }
}

/// The distance of `(px, py)`, in the first quadrant, from an ellipse with
/// radii `a` and `b`: the nearest point is found by iterating on the
/// evolute, which converges within a few rounds.
fn ellipse_distance(px: f64, py: f64, a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 {
        return px.hypot(py);
    }
    let (mut tx, mut ty) = (
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    );
    for _ in 0..8 {
        let (x, y) = (a * tx, b * ty);
        let ex = (a * a - b * b) * tx.powi(3) / a;
        let ey = (b * b - a * a) * ty.powi(3) / b;
        let (rx, ry) = (x - ex, y - ey);
        let (qx, qy) = (px - ex, py - ey);
        let (r, q) = (rx.hypot(ry), qx.hypot(qy));
        if q == 0.0 {
            break;
        }
        tx = ((qx * r / q + ex) / a).clamp(0.0, 1.0);
        ty = ((qy * r / q + ey) / b).clamp(0.0, 1.0);
        let t = tx.hypot(ty);
        if t == 0.0 {
            break;
        }
        tx /= t;
        ty /= t;
    }
    let distance = (px - a * tx).hypot(py - b * ty);
    if (px / a).powi(2) + (py / b).powi(2) > 1.0 {
        distance
    } else {
        -distance
    }
}
