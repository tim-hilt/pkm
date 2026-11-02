use crate::geometry::{Geometry, StrokeStyle};
use crate::scene::{Element, LoadError, Scene};

/// The core's public interface: owns the open Scene and everything that edits
/// it. Tools, Selection, history and viewport join as the features that need
/// them land.
#[derive(Debug)]
pub struct Editor {
    scene: Scene,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// An editor with an empty Scene.
    pub fn new() -> Self {
        Self {
            scene: Scene::empty(),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes`, the content of a
    /// `.excalidraw` file. If the bytes are not a Drawing the Scene is kept.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        self.scene = Scene::from_bytes(bytes)?;
        Ok(())
    }

    /// The Scene as the content of a `.excalidraw` file. Deleted Elements and
    /// everything the core does not understand are included.
    pub fn save(&self) -> Vec<u8> {
        self.scene.to_json().into_bytes()
    }

    /// Number of Elements in the Scene that are shown: of a supported type
    /// and not deleted.
    pub fn element_count(&self) -> u64 {
        self.shown().count() as u64
    }

    /// The paths to draw for the Scene, in z-order.
    ///
    /// For now every shown Element is drawn as a plain box around its bounds;
    /// the real shapes replace this as the generators land.
    pub fn geometry(&self) -> Geometry {
        let mut geometry = Geometry::new();
        for element in self.shown() {
            let style = StrokeStyle {
                color: 0x1e1e_1eff,
                width: 2.0,
                opacity: element.opacity() as f32,
            };
            let [first, rest @ ..] = bounds_outline(&element);
            let mut path = geometry.stroke(element.handle(), element.version(), style);
            path.move_to(first.0, first.1);
            for (x, y) in rest {
                path.line_to(x, y);
            }
            path.line_to(first.0, first.1);
        }
        geometry
    }

    fn shown(&self) -> impl Iterator<Item = Element<'_>> {
        self.scene.elements().filter(|e| !e.is_deleted())
    }
}

/// The corners of the Element's bounds, clockwise from the top left, turned
/// by its angle about its centre.
fn bounds_outline(element: &Element) -> [(f32, f32); 4] {
    // Excalidraw normalises negative sizes on load; do not rely on it.
    let (x0, x1) = ordered(element.x(), element.x() + element.width());
    let (y0, y1) = ordered(element.y(), element.y() + element.height());
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (sin, cos) = element.angle().sin_cos();
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
        let (dx, dy) = (x - cx, y - cy);
        (
            (cx + dx * cos - dy * sin) as f32,
            (cy + dx * sin + dy * cos) as f32,
        )
    })
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}
