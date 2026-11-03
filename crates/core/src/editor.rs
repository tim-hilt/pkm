use std::sync::atomic::{AtomicU64, Ordering};

use crate::geometry::{Geometry, StrokeStyle};
use crate::input::{MagnifyEvent, PointerEvent, ScrollEvent};
use crate::navigation::{Bounds, Navigation};
use crate::scene::{Element, LoadError, Scene};
use crate::shape;
use crate::tool::Tool;
use crate::viewport::Viewport;

/// Hands out [`Geometry::generation`]s; unique across all editors.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// The core's public interface: owns the open Scene and everything that edits
/// it and the viewport onto it. Tools, Selection and history join as the
/// features that need them land.
#[derive(Debug)]
pub struct Editor {
    scene: Scene,
    /// Changes whenever Element handles are reassigned, see
    /// [`Geometry::generation`].
    generation: u64,
    navigation: Navigation,
    tool: Tool,
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
            generation: next_generation(),
            navigation: Navigation::default(),
            tool: Tool::default(),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes`, the content of a
    /// `.excalidraw` file. If the bytes are not a Drawing the Scene is kept.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        self.scene = Scene::from_bytes(bytes)?;
        self.generation = next_generation();
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
    /// Rectangles, diamonds, ellipses, lines and arrows are drawn hand-drawn
    /// like on excalidraw.com, with the background of closed ones filled by
    /// hachure, cross-hatch, zigzag or solid (see [`crate::shape`]), and
    /// freedraw strokes as the filled outline of their pressure-sensitive
    /// path. The other types are drawn as a plain box around their bounds
    /// until their generators land.
    pub fn geometry(&self) -> Geometry {
        let mut geometry = Geometry::with_generation(self.generation);
        let background = self.scene.view_background_color();
        for element in self.shown() {
            if !shape::draw(&element, background, &mut geometry) {
                draw_bounds(&element, &mut geometry);
            }
        }
        geometry
    }

    /// Which part of the Scene the Canvas shows. Clients read it after
    /// forwarding input and apply it to the geometry they already have: it
    /// never changes what [`Editor::geometry`] returns.
    pub fn viewport(&self) -> Viewport {
        self.navigation.viewport()
    }

    /// Tells the core how big the Canvas is, in points. Clients report it on
    /// start and whenever it changes; zoom commands act around its middle.
    pub fn set_view_size(&mut self, width: f64, height: f64) {
        self.navigation.set_view_size(width, height);
    }

    /// Zooms in one step (10 percentage points) around the middle of the view.
    pub fn zoom_in(&mut self) {
        self.navigation.zoom_in();
    }

    /// Zooms out one step (10 percentage points) around the middle of the view.
    pub fn zoom_out(&mut self) {
        self.navigation.zoom_out();
    }

    /// Goes back to 100 % around the middle of the view.
    pub fn reset_zoom(&mut self) {
        self.navigation.reset_zoom();
    }

    /// Frames all shown Elements; see [`crate::navigation`]. Does nothing
    /// for an empty Scene.
    pub fn zoom_to_fit(&mut self) {
        if let Some(bounds) = self.scene_bounds() {
            self.navigation.fit(bounds);
        }
    }

    /// The active Tool.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Makes `tool` the active Tool; whatever the previous one was doing
    /// with a pointer still down stops.
    pub fn set_tool(&mut self, tool: Tool) {
        if tool != self.tool {
            self.navigation.cancel_drag();
            self.tool = tool;
        }
    }

    /// A step of a finger, Pencil or mouse. Two fingers pan and zoom the
    /// viewport, and so does one pointer with the hand Tool (see
    /// [`crate::navigation`]).
    pub fn pointer(&mut self, event: PointerEvent) {
        self.navigation.pointer(event, self.tool == Tool::Hand);
    }

    /// A scroll wheel or trackpad scroll (Mac): pans, or zooms with ctrl/cmd.
    pub fn scroll(&mut self, event: ScrollEvent) {
        self.navigation.scroll(event);
    }

    /// A trackpad pinch (Mac): zooms around the pinch centre.
    pub fn magnify(&mut self, event: MagnifyEvent) {
        self.navigation.magnify(event);
    }

    /// The smallest rectangle containing every shown Element, or `None` if
    /// there is none.
    fn scene_bounds(&self) -> Option<Bounds> {
        self.shown()
            .flat_map(|element| bounds_outline(&element))
            .fold(None, |bounds, (x, y)| {
                Some(match bounds {
                    None => Bounds {
                        min_x: x,
                        min_y: y,
                        max_x: x,
                        max_y: y,
                    },
                    Some(b) => Bounds {
                        min_x: b.min_x.min(x),
                        min_y: b.min_y.min(y),
                        max_x: b.max_x.max(x),
                        max_y: b.max_y.max(y),
                    },
                })
            })
    }

    fn shown(&self) -> impl Iterator<Item = Element<'_>> {
        self.scene.elements().filter(|e| !e.is_deleted())
    }
}

/// Draws a box around the Element's bounds.
fn draw_bounds(element: &Element, geometry: &mut Geometry) {
    let style = StrokeStyle {
        color: 0x1e1e_1eff,
        width: 2.0,
        opacity: element.opacity() as f32,
        dash: None,
    };
    let [first, rest @ ..] = bounds_outline(element);
    let mut path = geometry.stroke(element.handle(), element.version(), style);
    path.move_to(first.0 as f32, first.1 as f32);
    for (x, y) in rest {
        path.line_to(x as f32, y as f32);
    }
    path.line_to(first.0 as f32, first.1 as f32);
}

/// The corners of the Element's bounds, clockwise from the top left, turned
/// by its angle about its centre.
fn bounds_outline(element: &Element) -> [(f64, f64); 4] {
    // Excalidraw normalises negative sizes on load; do not rely on it.
    let (x0, x1) = ordered(element.x(), element.x() + element.width());
    let (y0, y1) = ordered(element.y(), element.y() + element.height());
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (sin, cos) = element.angle().sin_cos();
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
        let (dx, dy) = (x - cx, y - cy);
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    })
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}
