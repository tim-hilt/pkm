//! The viewport: which part of the Scene the Canvas shows (ADR-0006).
//!
//! The core owns it so that every client pans and zooms identically. Clients
//! read it after each input and apply it as a transform to cached geometry
//! (ADR-0007), so navigating never asks the core for geometry again.

/// Excalidraw's zoom limits (`MIN_ZOOM`, `MAX_ZOOM`).
pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 30.0;

/// The Scene point at the Canvas's top-left corner and the zoom: how many
/// view points one Scene unit covers. Zoom is always within
/// [`MIN_ZOOM`]`..=`[`MAX_ZOOM`].
///
/// View coordinates are points from the Canvas's top-left corner, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    x: f64,
    y: f64,
    zoom: f64,
}

impl Default for Viewport {
    /// Scene origin at the top-left corner, unzoomed.
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// The Scene point shown at the Canvas's top-left corner.
    pub fn origin(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// The Scene point shown at the view point `(x, y)`.
    pub(crate) fn scene_at(&self, x: f64, y: f64) -> (f64, f64) {
        (self.x + x / self.zoom, self.y + y / self.zoom)
    }

    /// Where the Scene point `(x, y)` is shown, in view coordinates: the
    /// inverse of [`Viewport::scene_at`].
    pub(crate) fn view_at(&self, (x, y): (f64, f64)) -> (f64, f64) {
        ((x - self.x) * self.zoom, (y - self.y) * self.zoom)
    }

    /// Shifts the view by `(dx, dy)` view points: the Scene appears to move
    /// by the same amount in the opposite direction.
    pub(crate) fn scroll_by(&mut self, dx: f64, dy: f64) {
        self.x += dx / self.zoom;
        self.y += dy / self.zoom;
    }

    /// Zooms to `zoom` (limited to the allowed range) such that the Scene
    /// point `scene` is shown at the view point `view`.
    ///
    /// This one operation is every pan/zoom gesture: a pinch pins the Scene
    /// point first grabbed to wherever the fingers' centre is now.
    pub(crate) fn pin(&mut self, scene: (f64, f64), view: (f64, f64), zoom: f64) {
        // `clamp` passes NaN through, and a NaN zoom would never recover.
        if zoom.is_nan() {
            return;
        }
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.x = scene.0 - view.0 / self.zoom;
        self.y = scene.1 - view.1 / self.zoom;
    }

    /// Zooms to `zoom` (limited to the allowed range) keeping the Scene under
    /// the view point `(x, y)` in place. Does nothing if the zoom would not
    /// change, so hitting a limit leaves the viewport exactly as it was.
    pub(crate) fn zoom_at(&mut self, x: f64, y: f64, zoom: f64) {
        if zoom.clamp(MIN_ZOOM, MAX_ZOOM) == self.zoom {
            return;
        }
        self.pin(self.scene_at(x, y), (x, y), zoom);
    }
}
