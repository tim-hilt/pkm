use crate::geometry::{Geometry, StrokeStyle};
use crate::input::{MagnifyEvent, PointerEvent, ScrollEvent};
use crate::navigation::Navigation;
use crate::scene::{Element, LoadError, Scene};
use crate::viewport::Viewport;

/// The core's public interface: owns the open Scene and everything that edits
/// it and the viewport onto it. Tools, Selection and history join as the
/// features that need them land.
#[derive(Debug)]
pub struct Editor {
    scene: Scene,
    navigation: Navigation,
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
            navigation: Navigation::default(),
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

    /// Which part of the Scene the Canvas shows. Clients read it after
    /// forwarding input and apply it to the geometry they already have: it
    /// never changes what [`Editor::geometry`] returns.
    pub fn viewport(&self) -> Viewport {
        self.navigation.viewport()
    }

    /// A step of a finger, Pencil or mouse. Two fingers pan and zoom the
    /// viewport (see [`crate::navigation`]).
    pub fn pointer(&mut self, event: PointerEvent) {
        self.navigation.pointer(event);
    }

    /// A scroll wheel or trackpad scroll (Mac): pans, or zooms with ctrl/cmd.
    pub fn scroll(&mut self, event: ScrollEvent) {
        self.navigation.scroll(event);
    }

    /// A trackpad pinch (Mac): zooms around the pinch centre.
    pub fn magnify(&mut self, event: MagnifyEvent) {
        self.navigation.magnify(event);
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
