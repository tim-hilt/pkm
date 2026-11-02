//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

pub mod geometry;
pub mod json;

pub use geometry::{Geometry, Part, PartKind, PartWriter, StrokeStyle, Verb};

/// The core's public interface: owns the open Scene and everything that edits
/// it. For now only an empty Scene exists; Tools, Selection, history and
/// viewport join as the features that need them land.
#[derive(Debug, Default)]
pub struct Editor {}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of Elements in the Scene.
    pub fn element_count(&self) -> u64 {
        0
    }

    /// The paths to draw for the Scene, in z-order.
    ///
    /// Tracer-bullet stand-in: Scenes cannot be loaded yet, so this ignores
    /// the Scene and always describes one hard-coded rounded rectangle.
    pub fn geometry(&self) -> Geometry {
        let mut geometry = Geometry::new();
        let style = StrokeStyle {
            color: 0x1e1e_1eff,
            width: 2.0,
            opacity: 1.0,
        };
        rounded_rectangle(
            &mut geometry.stroke(0, 1, style),
            50.0,
            100.0,
            280.0,
            180.0,
            32.0,
        );
        geometry
    }
}

/// Control-point distance, as a fraction of the radius, at which a cubic
/// Bézier best approximates a quarter circle: 4/3 · (√2 − 1).
const QUARTER_CIRCLE_HANDLE: f32 = 0.552_284_8;

/// Writes the outline of a rectangle with corner `radius`, clockwise from the
/// end of the top-left corner back to the start of it, so it is closed.
fn rounded_rectangle(path: &mut PartWriter, x: f32, y: f32, w: f32, h: f32, radius: f32) {
    let (right, bottom) = (x + w, y + h);
    let handle = radius * QUARTER_CIRCLE_HANDLE;
    path.move_to(x + radius, y);
    path.line_to(right - radius, y);
    path.cubic_to(
        right - radius + handle,
        y,
        right,
        y + radius - handle,
        right,
        y + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + handle,
        right - radius + handle,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(x + radius, bottom);
    path.cubic_to(
        x + radius - handle,
        bottom,
        x,
        bottom - radius + handle,
        x,
        bottom - radius,
    );
    path.line_to(x, y + radius);
    path.cubic_to(
        x,
        y + radius - handle,
        x + radius - handle,
        y,
        x + radius,
        y,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
