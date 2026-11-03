//! The box of an Element, and what to do with it.

use crate::linear;
use crate::path::Point;
use crate::scene::{Element, ElementType};

/// The box of an Element: where it is when not turned. Selection frames,
/// hit tests and turning all work from it (Excalidraw's
/// `getElementAbsoluteCoords`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub min: Point,
    pub max: Point,
}

impl Frame {
    pub fn of(element: &Element) -> Self {
        if matches!(element.kind(), ElementType::Freedraw) || element.kind().is_linear() {
            let points = element.points();
            let (x, y) = (element.x(), element.y());
            return points.iter().fold(
                Frame {
                    min: (x, y),
                    max: (x, y),
                },
                |frame, &(px, py)| Frame {
                    min: (frame.min.0.min(x + px), frame.min.1.min(y + py)),
                    max: (frame.max.0.max(x + px), frame.max.1.max(y + py)),
                },
            );
        }
        // Excalidraw normalises negative sizes on load; do not rely on it.
        let (x1, x2) = ordered(element.x(), element.x() + element.width());
        let (y1, y2) = ordered(element.y(), element.y() + element.height());
        Self {
            min: (x1, y1),
            max: (x2, y2),
        }
    }

    /// Like [`Frame::of`], but the box Excalidraw draws a selection frame
    /// around: for lines and arrows that of the sketched stroke, jitter
    /// included. Dearer than `of`, which is close enough for hit tests.
    pub fn drawn(element: &Element) -> Self {
        if element.kind().is_linear()
            && let Some((min, max)) = linear::drawn_bounds(element)
        {
            return Frame { min, max };
        }
        Self::of(element)
    }

    pub fn centre(&self) -> Point {
        (
            (self.min.0 + self.max.0) / 2.0,
            (self.min.1 + self.max.1) / 2.0,
        )
    }

    /// Where `point` is relative to the Element if it were not turned by
    /// `angle` (about the centre of the box).
    pub fn unturned(&self, point: Point, angle: f64) -> Point {
        if angle == 0.0 {
            return point;
        }
        let (sin, cos) = (-angle).sin_cos();
        let centre = self.centre();
        let (dx, dy) = (point.0 - centre.0, point.1 - centre.1);
        (
            centre.0 + dx * cos - dy * sin,
            centre.1 + dx * sin + dy * cos,
        )
    }

    /// Whether `point` is in the box turned by `angle`, or at most `margin`
    /// outside it.
    pub fn contains_turned(&self, point: Point, angle: f64, margin: f64) -> bool {
        let (x, y) = self.unturned(point, angle);
        (self.min.0 - margin..=self.max.0 + margin).contains(&x)
            && (self.min.1 - margin..=self.max.1 + margin).contains(&y)
    }
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}
