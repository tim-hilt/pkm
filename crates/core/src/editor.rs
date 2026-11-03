use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::draw_shape::{Outcome, ShapeGesture};
use crate::geometry::Geometry;
use crate::history::{self, History, Step};
use crate::hit_test::{self, Reach};
use crate::input::{MagnifyEvent, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
use crate::move_selection::MoveGesture;
use crate::navigation::{Bounds, Navigation};
use crate::overlay;
use crate::random::Entropy;
use crate::scene::{Element, ElementType, LoadError, Scene};
use crate::selection::Selection;
use crate::style::Style;
use crate::tap::{Origin, Recognized, Tap, TapRecognizer};
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
/// it, the Selection, the undo history of the session and the viewport onto
/// it.
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
    selection: Selection,
    taps: TapRecognizer,
    /// What can be undone and redone; never saved (see [`crate::history`]).
    history: History,
    /// The Selection being moved, if the pointer of a drag on it is down.
    moving: Option<MoveGesture>,
    /// Changes whenever the overlay may, see [`Editor::overlay_revision`].
    overlay_revision: u64,
    /// The zoom the overlay was last made for.
    overlay_zoom: f64,
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
            selection: Selection::default(),
            taps: TapRecognizer::default(),
            history: History::default(),
            moving: None,
            overlay_revision: 0,
            overlay_zoom: 1.0,
            measurer: None,
        }
    }

    /// Replaces the Scene with the Drawing in `bytes`, the content of a
    /// `.excalidraw` file. If the bytes are not a Drawing the Scene is kept.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        self.scene = Scene::from_bytes(bytes)?;
        if self.selection.clear() {
            self.overlay_revision += 1;
        }
        self.taps.cancel();
        // Whatever was being drawn or moved belonged to the Scene that is
        // gone, and so did what could be undone.
        self.shape = None;
        self.moving = None;
        self.history.clear();
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
        self.navigated();
    }

    /// Zooms out one step (10 percentage points) around the middle of the view.
    pub fn zoom_out(&mut self) {
        self.navigation.zoom_out();
        self.navigated();
    }

    /// Goes back to 100 % around the middle of the view.
    pub fn reset_zoom(&mut self) {
        self.navigation.reset_zoom();
        self.navigated();
    }

    /// Frames all shown Elements; see [`crate::navigation`]. Does nothing
    /// for an empty Scene.
    pub fn zoom_to_fit(&mut self) {
        if let Some(bounds) = self.scene_bounds() {
            self.navigation.fit(bounds);
            self.navigated();
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
            self.taps.cancel();
            self.drop_shape();
            self.drop_move();
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
    /// the shape (see [`crate::draw_shape`]), and with the selection Tool a tap
    /// selects the Element it hits or clears the Selection (see
    /// [`Editor::selection`]) while a drag on an Element moves the Selection
    /// (see [`crate::move_selection`]).
    pub fn pointer(&mut self, event: PointerEvent) {
        self.navigation.pointer(event, self.tool == Tool::Hand);
        self.navigated();
        self.draw_shape(event);
        self.select(event);
    }

    /// Whether there is something to undo: the buttons for it are enabled
    /// exactly then. Changes after any input or command, so clients read it
    /// with the other state of the chrome.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether there is something to redo; see [`Editor::can_undo`].
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Takes back the latest change to the Scene: a drawn shape, a move. The
    /// Selection becomes what it was before that change. Does nothing if
    /// there is nothing to undo, or while a pointer is drawing or moving.
    pub fn undo(&mut self) {
        if !self.is_gesturing() {
            let selection = self.history.undo(&mut self.scene, &mut self.entropy);
            self.restored(selection);
        }
    }

    /// Does again what was last undone, with the Selection it had then.
    /// Anything new done since undoing has made that history; see
    /// [`Editor::undo`].
    pub fn redo(&mut self) {
        if !self.is_gesturing() {
            let selection = self.history.redo(&mut self.scene, &mut self.entropy);
            self.restored(selection);
        }
    }

    /// Whether a pointer is down making a change that is not finished.
    fn is_gesturing(&self) -> bool {
        self.shape.is_some() || self.moving.is_some()
    }

    /// Shows the Scene as undo or redo left it, with `selection` selected
    /// as far as its Elements are still there. Nothing to do for `None`.
    fn restored(&mut self, selection: Option<Selection>) {
        let Some(selection) = selection else {
            return;
        };
        self.taps.cancel();
        self.selection = history::without_gone(selection, &self.scene);
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// The ids of the selected Elements, in the order they were selected.
    pub fn selection(&self) -> &[String] {
        self.selection.ids()
    }

    /// Selects the Element a tap hits, or clears the Selection if the tap
    /// was a deliberate one on empty Canvas (see [`crate::tap`]); moves the
    /// Selection with a drag that began on an Element.
    ///
    /// Only the selection Tool taps and drags, and never with two fingers
    /// down: they navigate, and must not disturb the Selection, nor leave a
    /// move half done.
    fn select(&mut self, event: PointerEvent) {
        if self.tool != Tool::Selection || self.navigation.is_pinching() {
            self.taps.cancel();
            self.drop_move();
            return;
        }
        self.follow_move(event);
        match self.taps.track(event) {
            Some(Recognized::Tap(tap)) => self.tap(tap),
            Some(Recognized::DragBegan(origin)) => self.begin_move(origin, event),
            None => {}
        }
    }

    fn tap(&mut self, tap: Tap) {
        let hit = self.hit(tap.x, tap.y, tap.kind);
        let changed = match hit {
            Some(id) => self.selection.select_only(&id),
            None if tap.is_short() => self.selection.clear(),
            None => false,
        };
        if changed {
            self.overlay_revision += 1;
        }
    }

    /// The id of the topmost Element at the view point `(x, y)`, if any.
    fn hit(&self, x: f64, y: f64, kind: PointerKind) -> Option<String> {
        let viewport = self.navigation.viewport();
        let reach = Reach::new(kind, viewport.zoom());
        let point = viewport.scene_at(x, y);
        hit_test::topmost(&self.scene, &self.selection, point, reach)
            .map(|element| element.id().to_owned())
    }

    /// Starts moving the Selection if the drag began on an Element: on the
    /// Selection itself, or else on an Element that becomes the Selection.
    /// `event` is the step that turned the press into a drag.
    fn begin_move(&mut self, origin: Origin, event: PointerEvent) {
        let Some(id) = self.hit(origin.x, origin.y, origin.kind) else {
            return;
        };
        // Undoing the move also undoes the selecting it began with.
        let step = Step::begin(&self.selection);
        if self.selection.select_only(&id) {
            self.overlay_revision += 1;
        }
        let viewport = self.navigation.viewport();
        let start = viewport.scene_at(origin.x, origin.y);
        self.moving = Some(MoveGesture::begin(
            step,
            &self.scene,
            &self.selection,
            event.id,
            start,
        ));
        self.follow_move(event);
    }

    /// Takes the Selection along with the pointer that moves it.
    fn follow_move(&mut self, event: PointerEvent) {
        let Some(moving) = self.moving.as_mut().filter(|m| m.pointer() == event.id) else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if moving.drag_to(&mut self.scene, &mut self.entropy, to) {
                    self.changed(false);
                }
            }
            PointerPhase::Up => {
                if let Some(moving) = self.moving.take() {
                    self.history
                        .record(moving.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_move(),
        }
    }

    /// Puts the Selection being moved, if any, back where it was.
    fn drop_move(&mut self) {
        if let Some(moving) = self.moving.take() {
            moving.cancel(&mut self.scene, &mut self.entropy);
            self.changed(false);
        }
    }

    /// What clients draw over the Scene: a frame around each selected
    /// Element, in Scene coordinates, as geometry in the same format as
    /// [`Editor::geometry`] (strokes only; no text or images). Clients draw
    /// it after the Scene and do not cache it by Element: its strokes are as
    /// wide and as far from the Elements as they are because of the zoom it
    /// was made for.
    pub fn overlay(&self) -> Geometry {
        let selected = self.shown().filter(|e| self.selection.contains(e.id()));
        overlay::selection(selected, self.navigation.viewport().zoom())
    }

    /// Tells whether [`Editor::overlay`] may differ from before: it changes
    /// whenever the Selection, a selected Element or the zoom does, and
    /// never for a pan. Clients compare it with the one they last drew
    /// with, like [`Editor::revision`].
    pub fn overlay_revision(&self) -> u64 {
        self.overlay_revision
    }

    /// Notes that the viewport may have changed: the overlay depends on its
    /// zoom.
    fn navigated(&mut self) {
        let zoom = self.navigation.viewport().zoom();
        if zoom != self.overlay_zoom {
            self.overlay_zoom = zoom;
            self.overlay_revision += 1;
        }
    }

    /// A scroll wheel or trackpad scroll (Mac): pans, or zooms with ctrl/cmd.
    pub fn scroll(&mut self, event: ScrollEvent) {
        self.navigation.scroll(event);
        self.navigated();
    }

    /// A trackpad pinch (Mac): zooms around the pinch centre.
    pub fn magnify(&mut self, event: MagnifyEvent) {
        self.navigation.magnify(event);
        self.navigated();
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
                        Step::begin(&self.selection),
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
                    Outcome::Drawn(step) => {
                        self.history.record(step, &self.scene, &self.selection);
                        if !self.tool_lock {
                            self.tool = Tool::Selection;
                        }
                    }
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
        // Only selected Elements have anything drawn over them.
        if !self.selection.is_empty() {
            self.overlay_revision += 1;
        }
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
