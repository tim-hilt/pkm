use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::draw_shape::{Outcome, ShapeGesture};
use crate::geometry::Geometry;
use crate::input::{MagnifyEvent, PointerEvent, PointerPhase, ScrollEvent};
use crate::navigation::{Bounds, Navigation};
use crate::random::Entropy;
use crate::scene::{Element, ElementType, LoadError, Scene};
use crate::style::Style;
use crate::text::{self, Containers, TextMeasurer};
use crate::tool::Tool;
use crate::viewport::Viewport;
use crate::{image, shape};

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
    /// Changes whenever anything shown on the Canvas does, see
    /// [`Editor::revision`].
    revision: u64,
    navigation: Navigation,
    tool: Tool,
    /// Whether the Tool stays after drawing instead of returning to
    /// selection.
    tool_lock: bool,
    /// What new Elements look like.
    style: Style,
    entropy: Entropy,
    /// The shape being drawn, if the pointer of a shape Tool is down.
    shape: Option<ShapeGesture>,
    measurer: Option<Measurer>,
}

/// The client's [`TextMeasurer`].
struct Measurer(Arc<dyn TextMeasurer>);

impl fmt::Debug for Measurer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextMeasurer")
    }
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
            revision: 0,
            navigation: Navigation::default(),
            tool: Tool::default(),
            tool_lock: false,
            style: Style::default(),
            entropy: Entropy::from_system(),
            shape: None,
            measurer: None,
        }
    }

    /// Replaces the Scene with the Drawing in `bytes`, the content of a
    /// `.excalidraw` file. If the bytes are not a Drawing the Scene is kept.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        self.scene = Scene::from_bytes(bytes)?;
        // Whatever was being drawn belonged to the Scene that is gone.
        self.shape = None;
        self.changed(true);
        Ok(())
    }

    /// The Scene as the content of a `.excalidraw` file. Deleted Elements and
    /// everything the core does not understand are included.
    pub fn save(&self) -> Vec<u8> {
        self.scene.to_json().into_bytes()
    }

    /// Hands the core the client's way to measure text, which it needs to wrap
    /// the text of containers. Without one the lines are those Excalidraw saved
    /// in the file. Replaces any earlier measurer.
    pub fn set_text_measurer(&mut self, measurer: Arc<dyn TextMeasurer>) {
        self.measurer = Some(Measurer(measurer));
        // What was drawn for texts may differ now.
        self.changed(true);
    }

    /// Tells whether the Scene changed: it differs after every input or
    /// command that added, removed or edited an Element, or replaced the
    /// Scene, and only then. Clients compare it with the one they last drew
    /// and ask for [`Editor::geometry`] again if it differs.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The bytes of the file `id` of the Scene (an image, as stored; see
    /// [`Scene::file_bytes`]), for clients to decode when [`Editor::geometry`]
    /// names it. `None` if the Scene has no such file.
    pub fn file_bytes(&self, id: &str) -> Option<Vec<u8>> {
        self.scene.file_bytes(id)
    }

    /// Number of Elements in the Scene that are shown: of a supported type
    /// and not deleted.
    pub fn element_count(&self) -> u64 {
        self.shown().count() as u64
    }

    /// The paths and lines of text to draw for the Scene, in z-order.
    ///
    /// Rectangles, diamonds, ellipses, lines and arrows are drawn hand-drawn
    /// like on excalidraw.com, with the background of closed ones filled by
    /// hachure, cross-hatch, zigzag or solid (see [`crate::shape`]), and
    /// freedraw strokes as the filled outline of their pressure-sensitive
    /// path. Text is not made of paths but described as lines of text for the
    /// client to set in Excalifont (see [`crate::text`]). Images are described
    /// by file id, placement, crop and scale for the client to draw (see
    /// [`crate::image`]).
    pub fn geometry(&self) -> Geometry {
        let mut geometry = Geometry::with_generation(self.generation);
        let background = self.scene.view_background_color();
        let containers = Containers::new(&self.scene);
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
        for element in self.shown() {
            if element.kind() == ElementType::Text {
                text::draw(&element, &containers, measurer, &mut geometry);
            } else if element.kind() == ElementType::Image {
                image::draw(&element, &mut geometry);
            } else {
                shape::draw(&element, background, &mut geometry);
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
    /// with a pointer still down stops, and a shape it was drawing is gone.
    pub fn set_tool(&mut self, tool: Tool) {
        if tool != self.tool {
            self.navigation.cancel_drag();
            self.drop_shape();
            self.tool = tool;
        }
    }

    /// Whether the Tool lock is on: the Tool then stays after drawing a shape
    /// instead of returning to selection.
    pub fn tool_lock(&self) -> bool {
        self.tool_lock
    }

    /// Turns the Tool lock on or off. As on excalidraw.com, turning it off
    /// also returns to selection: the lock was what kept the Tool.
    pub fn set_tool_lock(&mut self, locked: bool) {
        if self.tool_lock && !locked {
            self.set_tool(Tool::Selection);
        }
        self.tool_lock = locked;
    }

    /// A step of a finger, Pencil or mouse. Two fingers pan and zoom the
    /// viewport, and so does one pointer with the hand Tool (see
    /// [`crate::navigation`]). With a shape Tool, one pointer dragging draws
    /// the shape (see [`crate::draw_shape`]).
    pub fn pointer(&mut self, event: PointerEvent) {
        self.navigation.pointer(event, self.tool == Tool::Hand);
        self.draw_shape(event);
    }

    /// A scroll wheel or trackpad scroll (Mac): pans, or zooms with ctrl/cmd.
    pub fn scroll(&mut self, event: ScrollEvent) {
        self.navigation.scroll(event);
    }

    /// A trackpad pinch (Mac): zooms around the pinch centre.
    pub fn magnify(&mut self, event: MagnifyEvent) {
        self.navigation.magnify(event);
    }

    /// Follows the pointer with the shape being drawn, or starts one.
    fn draw_shape(&mut self, event: PointerEvent) {
        // Two fingers navigate; they never draw, and a drawing one of them
        // began turns into the gesture.
        if self.navigation.is_pinching() {
            self.drop_shape();
            return;
        }
        let drawing_pointer = self.shape.as_ref().map(ShapeGesture::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        match event.phase {
            PointerPhase::Down if drawing_pointer.is_none() => {
                if let Some(kind) = self.tool.shape() {
                    self.shape = Some(ShapeGesture::begin(
                        &mut self.scene,
                        &mut self.entropy,
                        &self.style,
                        kind,
                        event.id,
                        at,
                    ));
                    self.changed(true);
                }
            }
            PointerPhase::Move if drawing_pointer == Some(event.id) => {
                let square = event.modifiers.shift;
                let moved = self.shape.as_mut().is_some_and(|shape| {
                    shape.drag_to(&mut self.scene, &mut self.entropy, at, square)
                });
                if moved {
                    self.changed(false);
                }
            }
            PointerPhase::Up if drawing_pointer == Some(event.id) => {
                let Some(shape) = self.shape.take() else {
                    return;
                };
                match shape.finish(&mut self.scene) {
                    Outcome::Drawn if !self.tool_lock => self.tool = Tool::Selection,
                    Outcome::Drawn => {}
                    Outcome::Dropped => self.changed(true),
                }
            }
            PointerPhase::Cancel if drawing_pointer == Some(event.id) => self.drop_shape(),
            _ => {}
        }
    }

    /// Takes the shape being drawn, if any, out of the Scene again.
    fn drop_shape(&mut self) {
        if let Some(shape) = self.shape.take() {
            shape.cancel(&mut self.scene);
            self.changed(true);
        }
    }

    /// Records that the Scene changed. `restructured` if Elements were added
    /// or removed, which may hand their handles to other Elements.
    fn changed(&mut self, restructured: bool) {
        self.revision += 1;
        if restructured {
            self.generation = next_generation();
        }
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
