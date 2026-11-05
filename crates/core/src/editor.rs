use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::action_bar::ActionBar;
use crate::binding::{self, End};
use crate::box_select::BoxSelect;
use crate::clipboard;
use crate::delete;
use crate::draw_freedraw::StrokeDrawing;
use crate::draw_linear::{LinearDrawing, Update};
use crate::draw_shape::{Outcome, ShapeGesture};
use crate::duplicate;
use crate::edit_points::{self, Grab, PointDrag, PointHandles, PointSelection};
use crate::erase::Erasing;
use crate::frame::Frame;
use crate::geometry::Geometry;
use crate::group;
use crate::history::{self, History, Step};
use crate::hit_test::{self, Reach};
use crate::input::{MagnifyEvent, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
use crate::insert_image::{self, ImageFile, Placement};
use crate::move_selection::MoveGesture;
use crate::navigation::{Bounds, Navigation};
use crate::overlay;
use crate::path::Point;
use crate::pencil_double_tap::PencilDoubleTap;
use crate::pencil_only::PencilOnly;
use crate::properties::{self, Properties, Property};
use crate::random::Entropy;
use crate::scene::{Element, ElementType, LoadError, Scene};
use crate::selection::Selection;
use crate::shortcut::Command;
use crate::style::Style;
use crate::tap::{Origin, Recognized, Tap, TapRecognizer};
use crate::text::{self, Containers, TextMeasurer};
use crate::text_editing::{TextEditing, TextEditor};
use crate::tool::Tool;
use crate::transform::TransformGesture;
use crate::transform_handles::{Handle, TransformBox};
use crate::viewport::Viewport;
use crate::z_order::{self, ZOrder};
use crate::{image, shape};

/// Hands out [`Geometry::generation`]s; unique across all editors.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Excalidraw's `ELEMENT_READY_TO_ERASE_OPACITY` (20 %): how much of their
/// opacity Elements keep while the eraser is about to delete them.
const READY_TO_ERASE_OPACITY: f32 = 0.2;

/// Scene units kept free around an export, like Excalidraw's default
/// `exportPadding`.
const EXPORT_PADDING: f64 = 10.0;

/// What to export as an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    /// Every shown Element.
    Scene,
    /// The Selection, with the labels of selected containers.
    Selection,
}

/// An image export for the client to rasterise (ADR-0008): the geometry of
/// what is exported and the rectangle of the Scene, in Scene units, that the
/// image shows.
#[derive(Debug)]
pub struct Export {
    pub geometry: Geometry,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

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
    /// The Tool before the current one, for the Pencil double-tap.
    previous_tool: Tool,
    /// Whether the Tool stays after drawing instead of returning to
    /// selection.
    tool_lock: bool,
    /// What new Elements look like.
    style: Style,
    entropy: Entropy,
    /// The shape being drawn, if the pointer of a shape Tool is down.
    shape: Option<ShapeGesture>,
    /// The line or arrow being drawn, from its first press until it is
    /// finished: with tapping that is longer than a pointer is down.
    linear: Option<LinearDrawing>,
    /// The freehand stroke being drawn, if the pointer of the freedraw Tool is
    /// down.
    stroke: Option<StrokeDrawing>,
    /// The erase in progress, if the pointer of the eraser Tool is down.
    erasing: Option<Erasing>,
    selection: Selection,
    /// The group that was gone into (double-tapped), see [`crate::group`]:
    /// always one that every selected Element is in.
    editing_group: Option<String>,
    /// The group that was gone into when the drag in progress began, which
    /// the drag leaves if it selects outside it, and a drag that is called off
    /// goes back into.
    drag_editing: Option<String>,
    taps: TapRecognizer,
    /// What can be undone and redone; never saved (see [`crate::history`]).
    history: History,
    /// The Selection being moved, if the pointer of a drag on it is down.
    moving: Option<MoveGesture>,
    /// The Selection being resized or rotated, if the pointer of a drag on a
    /// transform handle is down.
    transforming: Option<TransformGesture>,
    /// The points of the selected line or arrow being moved, if the pointer
    /// of a drag on one of its handles is down.
    point_dragging: Option<PointDrag>,
    /// The points of the selected line or arrow that are selected, see
    /// [`Editor::selected_points`].
    points: PointSelection,
    /// The box being dragged over empty Canvas, if the pointer of such a drag
    /// is down.
    box_selecting: Option<BoxSelect>,
    /// Whether fingers are held to selecting, panning and zooming, see
    /// [`Editor::pencil_only_drawing`].
    pencil_only: PencilOnly,
    /// The kind of the last pointer seen, which the handles of the overlay
    /// are sized for.
    last_pointer: PointerKind,
    /// Changes whenever the overlay may, see [`Editor::overlay_revision`].
    overlay_revision: u64,
    /// The zoom the overlay was last made for.
    overlay_zoom: f64,
    /// The shape an arrow end that is being put somewhere is over, which the
    /// overlay highlights.
    highlight: Option<String>,
    measurer: Option<Measurer>,
    /// The text being edited, if the client shows a text view for one; see
    /// [`Editor::text_editor`].
    text: Option<TextEditing>,
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
            previous_tool: Tool::default(),
            tool_lock: false,
            style: Style::default(),
            entropy: Entropy::from_system(),
            shape: None,
            linear: None,
            stroke: None,
            erasing: None,
            selection: Selection::default(),
            editing_group: None,
            drag_editing: None,
            taps: TapRecognizer::default(),
            history: History::default(),
            moving: None,
            transforming: None,
            point_dragging: None,
            points: PointSelection::default(),
            box_selecting: None,
            pencil_only: PencilOnly::default(),
            last_pointer: PointerKind::Finger,
            overlay_revision: 0,
            overlay_zoom: 1.0,
            highlight: None,
            measurer: None,
            text: None,
        }
    }

    /// Replaces the Scene with the Drawing in `bytes`, the content of a
    /// `.excalidraw` file. If the bytes are not a Drawing the Scene is kept.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        self.scene = Scene::from_bytes(bytes, &mut self.entropy)?;
        if self.selection.clear() {
            self.overlay_revision += 1;
        }
        self.editing_group = None;
        self.taps.cancel();
        // Whatever was being drawn, moved or transformed belonged to the
        // Scene that is gone, and so did what could be undone.
        self.shape = None;
        self.linear = None;
        self.stroke = None;
        self.erasing = None;
        self.moving = None;
        self.transforming = None;
        self.point_dragging = None;
        self.points.clear();
        self.box_selecting = None;
        self.text = None;
        self.history.clear();
        self.sync_highlight();
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
        self.geometry_of(self.generation, |_| true)
    }

    /// What a PNG export of `scope` shows: its geometry and the Scene
    /// rectangle to rasterise, which is the Elements' bounds plus
    /// Excalidraw's export padding. `None` if there is nothing to export.
    pub fn export(&self, scope: ExportScope) -> Option<Export> {
        let included = |element: &Element| match scope {
            ExportScope::Scene => true,
            ExportScope::Selection => self.selection.covers(element),
        };
        let bounds = bounds_of(self.shown().filter(|e| included(e)))?;
        Some(Export {
            geometry: self.geometry_of(next_generation(), included),
            x: bounds.min_x - EXPORT_PADDING,
            y: bounds.min_y - EXPORT_PADDING,
            width: bounds.max_x - bounds.min_x + 2.0 * EXPORT_PADDING,
            height: bounds.max_y - bounds.min_y + 2.0 * EXPORT_PADDING,
        })
    }

    /// The geometry of the shown Elements for which `include` holds.
    fn geometry_of(&self, generation: u64, include: impl Fn(&Element) -> bool) -> Geometry {
        let mut geometry = Geometry::with_generation(generation);
        let background = self.scene.view_background_color();
        let containers = Containers::new(&self.scene);
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
        let editing = self.text.as_ref().map(TextEditing::handle);
        for element in self.shown().filter(|e| include(e)) {
            // The text view shows the text that is being edited.
            if editing == Some(element.handle()) {
                continue;
            }
            let mark = geometry.mark();
            if element.kind() == ElementType::Text {
                text::draw(&element, &containers, measurer, &mut geometry);
            } else if element.kind() == ElementType::Image {
                image::draw(&element, &mut geometry);
            } else {
                shape::draw(&element, background, &mut geometry);
            }
            if self
                .erasing
                .as_ref()
                .is_some_and(|erasing| erasing.is_pending(element.id()))
            {
                geometry.fade(mark, READY_TO_ERASE_OPACITY);
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

    /// Shows the Scene from `origin` (the Scene point at the Canvas's top-left
    /// corner) at `zoom`, limited to the allowed range: how a client brings
    /// back the viewport a Drawing was left at. A value that is not a finite
    /// number leaves the viewport as it is. Not part of the Scene, so nothing
    /// to save or undo.
    pub fn set_viewport(&mut self, origin: (f64, f64), zoom: f64) {
        self.navigation.set_viewport(origin, zoom);
        self.navigated();
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
    /// with a pointer still down stops, and a shape it was drawing is gone. A
    /// line or arrow that is waiting for its next point is finished as it is.
    pub fn set_tool(&mut self, tool: Tool) {
        if tool != self.tool {
            self.commit_text();
            self.navigation.cancel_drag();
            self.taps.cancel();
            self.drop_shape();
            self.drop_stroke();
            self.drop_erase();
            self.end_linear();
            self.drop_move();
            self.drop_point_drag();
            self.drop_box_select();
            self.assign_tool(tool);
            self.sync_highlight();
        }
    }

    /// Makes `tool` the active Tool, remembering the one it replaces.
    fn assign_tool(&mut self, tool: Tool) {
        if tool != self.tool {
            self.previous_tool = self.tool;
            self.tool = tool;
        }
    }

    /// The Pencil was double-tapped; `action` is what the user set for that
    /// in the system settings (see [`crate::pencil_double_tap`]). Switches
    /// the Tool like [`Editor::set_tool`] does, so that a stroke in progress
    /// ends, and the Tool lock stays as it is.
    pub fn pencil_double_tap(&mut self, action: PencilDoubleTap) {
        if let Some(tool) = action.target(self.tool, self.previous_tool) {
            self.set_tool(tool);
        }
    }

    /// Whether Pencil-only drawing is on: the Pencil then uses the current
    /// Tool while fingers only select (a tap selects, whatever the Tool),
    /// pan (a one-finger drag, anywhere) and zoom, and never create
    /// Elements; a Pencil drag on empty Canvas box-selects (with the
    /// selection Tool). Mouse input is not affected. It turns on by itself
    /// with the first Pencil input, unless [`Editor::set_pencil_only_drawing`]
    /// has been called. Clients store it, see [`crate::pencil_only`], and
    /// read it after every input, which may have turned it on.
    pub fn pencil_only_drawing(&self) -> bool {
        self.pencil_only.is_on()
    }

    /// Switches Pencil-only drawing on or off: what the user chose in
    /// settings, or the setting stored earlier. Pencil input no longer turns
    /// it on by itself afterwards. Meant for use between gestures: it does
    /// not call off a drag that is in progress.
    pub fn set_pencil_only_drawing(&mut self, on: bool) {
        self.pencil_only.choose(on);
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

    /// The text view to show over the Canvas, if a text is being edited, with
    /// where it is, what it says and how it looks (see
    /// [`crate::text_editing`]). A tap with the text Tool opens it at the tap
    /// (or on the text that is there), and so does a double-tap (a
    /// double-click) on a text; a double-tap on a rectangle, diamond or
    /// ellipse opens it on the label of the shape, as does
    /// [`Editor::edit_selected_text`]. It closes when the text is committed
    /// or cancelled, or by a tap elsewhere, which commits. The position is in view
    /// coordinates: clients read it with the viewport after every input and
    /// command. Needs a [`Editor::set_text_measurer`]: without one no text
    /// can be edited.
    pub fn text_editor(&self) -> Option<TextEditor> {
        self.text
            .as_ref()?
            .editor(&self.scene, self.navigation.viewport())
    }

    /// Tells the core what the text view says now: the text is laid out
    /// again, and the editor follows it. Clients call it on every change of
    /// the text view. Does nothing unless a text is being edited.
    pub fn set_editing_text(&mut self, text: &str) {
        let (Some(editing), Some(measurer)) = (self.text.as_mut(), self.measurer.as_ref()) else {
            return;
        };
        if editing.set_text(&mut self.scene, &mut self.entropy, text, &*measurer.0) {
            self.changed(false);
        }
    }

    /// Ends the editing of the text and keeps it: a text that is new, or has
    /// been changed, is one undo step, and becomes the Selection (for a
    /// label, its shape does). A text with
    /// nothing in it but blanks is not kept: one that is new is never made,
    /// one that was there is deleted. Does nothing unless a text is being
    /// edited.
    pub fn commit_text(&mut self) {
        let Some(editing) = self.text.take() else {
            return;
        };
        let id = editing.id().to_owned();
        let container = editing.container_id().map(str::to_owned);
        let step = editing.finish(&mut self.scene, &mut self.entropy);
        let kept = self
            .scene
            .element_by_id(&id)
            .is_some_and(|element| !element.is_deleted());
        // The shape of a label is selected after it, kept or not; a text of
        // its own only if it is kept. With the Tool lock the text Tool waits
        // for the next text.
        let selected = container.or_else(|| kept.then_some(id));
        if let Some(selected) = selected.filter(|_| !self.tool_lock) {
            self.selection.select([selected]);
        }
        if let Some(step) = step {
            self.history.record(step, &self.scene, &self.selection);
        }
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// Ends the editing of the text and keeps it as it was before: a text
    /// that is new is never made. Does nothing unless a text is being edited.
    pub fn cancel_text(&mut self) {
        if let Some(editing) = self.text.take() {
            editing.cancel(&mut self.scene, &mut self.entropy);
            self.overlay_revision += 1;
            self.changed(true);
        }
    }

    /// Opens the text view on the text `existing` if there is one, else on a
    /// new text at `at` in the Scene.
    fn begin_text(&mut self, existing: Option<String>, at: Point) {
        let Some(measurer) = self.measurer.as_ref() else {
            return;
        };
        if let Some(id) = existing.filter(|id| self.scene.element_by_id(id).is_some()) {
            return self.begin_existing_text(&id);
        }
        let editing = TextEditing::begin_new(
            Step::begin(&self.selection),
            &mut self.scene,
            &mut self.entropy,
            &self.style,
            at,
            &*measurer.0,
        );
        self.open_text(editing);
    }

    /// Opens the text view on the text of its own `id`.
    fn begin_existing_text(&mut self, id: &str) {
        if self.measurer.is_none() {
            return;
        }
        let Some(element) = self.scene.element_by_id(id) else {
            return;
        };
        let step = Step::begin(&self.selection);
        let editing = TextEditing::begin_existing(step, &self.scene, &element, None);
        self.open_text(editing);
    }

    /// Opens the text view on the label of the shape `container_id`, or on a
    /// new one if it has none. Does nothing if the shape cannot hold text.
    fn begin_label(&mut self, container_id: &str) {
        let Some(measurer) = self.measurer.as_ref() else {
            return;
        };
        let holds_text = self
            .scene
            .element_by_id(container_id)
            .is_some_and(|e| text::takes_label(e.kind()) && !e.is_deleted());
        if !holds_text {
            return;
        }
        let step = Step::begin(&self.selection);
        let editing = match text::label_of(&self.scene, container_id) {
            Some(label) => {
                let container = self.scene.element_by_id(container_id);
                Some(TextEditing::begin_existing(
                    step,
                    &self.scene,
                    &label,
                    container.as_ref(),
                ))
            }
            None => TextEditing::begin_label(
                step,
                &mut self.scene,
                &mut self.entropy,
                &self.style,
                container_id,
                &*measurer.0,
            ),
        };
        if let Some(editing) = editing {
            self.open_text(editing);
        }
    }

    /// Shows the text view for `editing`. The Selection is empty while a
    /// text is edited, and the Tool returns to selection unless it is
    /// locked.
    fn open_text(&mut self, editing: TextEditing) {
        self.text = Some(editing);
        self.selection.clear();
        self.editing_group = None;
        self.sync_points();
        if !self.tool_lock {
            self.assign_tool(Tool::Selection);
        }
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// The command to edit the text of the Selection (Enter on a Mac): the
    /// text of a selected text, or the label of a selected rectangle,
    /// diamond or ellipse, which is made if it has none. Does nothing for any
    /// other Selection, and while a text is edited.
    pub fn edit_selected_text(&mut self) {
        if self.text.is_some() || self.is_gesturing() {
            return;
        }
        let [id] = self.selection.ids() else {
            return;
        };
        let id = id.clone();
        if self.is_free_text(&id) {
            self.begin_existing_text(&id);
        } else {
            self.begin_label(&id);
        }
    }

    /// Whether the Element `id` is a text of its own: one that is not the
    /// label of a container, which is edited with it.
    fn is_free_text(&self, id: &str) -> bool {
        self.scene
            .element_by_id(id)
            .is_some_and(|e| e.kind() == ElementType::Text && e.container_id().is_none())
    }

    /// Follows the pointer for a tap that opens the text view (with the text
    /// Tool, on a text if there is one at the tap) or, if it is open, closes
    /// it, keeping the text.
    fn follow_text_tap(&mut self, event: PointerEvent) {
        let Some(Recognized::Tap(tap)) = self.taps.track(event) else {
            return;
        };
        if self.text.is_some() {
            self.commit_text();
            return;
        }
        let at = self.navigation.viewport().scene_at(tap.x, tap.y);
        let existing = self
            .hit(tap.x, tap.y, tap.kind)
            .filter(|id| self.is_free_text(id));
        self.begin_text(existing, at);
    }

    /// A step of a finger, Pencil or mouse. Two fingers pan and zoom the
    /// viewport, and so does one pointer with the hand Tool (see
    /// [`crate::navigation`]). With a shape Tool, one pointer dragging draws
    /// the shape (see [`crate::draw_shape`]), and with the line or arrow Tool
    /// it draws a line, by dragging or by tapping its points one by one (see
    /// [`crate::draw_linear`]), and with the freedraw Tool it draws a stroke
    /// through the points the pointer passes (see [`crate::draw_freedraw`]),
    /// and with the eraser Tool it previews the Elements the pointer passes
    /// and deletes them when it lifts (see [`crate::erase`]), and with the
    /// text Tool a tap opens the text view (see [`crate::text_editing`]). With the selection Tool a tap selects the
    /// Element it hits or clears the Selection (see [`Editor::selection`])
    /// while a drag on an Element moves the Selection (see
    /// [`crate::move_selection`]), a drag on a handle of it resizes, rotates
    /// or, for a line or arrow, edits its points (see [`crate::edit_points`]),
    /// and a drag on empty Canvas box-selects (see [`crate::box_select`]).
    /// Shift adds to the Selection, or takes from it, instead of replacing it.
    /// Arrows bind to the shapes their ends are put on and follow them when
    /// they move or change (see [`crate::binding`]). In Pencil-only drawing
    /// (see [`Editor::pencil_only_drawing`]) none of this is done by fingers,
    /// which tap to select and drag to pan.
    pub fn pointer(&mut self, event: PointerEvent) {
        self.last_pointer = event.kind;
        self.pencil_only.observe(event.kind);
        // A finger in Pencil-only drawing: it pans, and only selects.
        let finger = self.pencil_only.restricts(event.kind);
        let tool = self.tool;
        self.navigation
            .pointer(event, self.tool == Tool::Hand || finger);
        self.navigated();
        if !finger {
            self.draw_shape(event);
            self.draw_linear(event);
            self.draw_stroke(event);
            self.erase(event);
        }
        // An event that finished a drawing, and so went back to selection,
        // was the drawing's: it is no tap of the selection Tool.
        let finished_drawing = tool != Tool::Selection && self.tool == Tool::Selection;
        if !finished_drawing {
            self.select(event);
        }
        self.sync_editing();
        self.sync_points();
        self.sync_highlight();
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
        self.finish_placing();
        if !self.is_gesturing() {
            let selection = self.history.undo(&mut self.scene, &mut self.entropy);
            self.restored(selection);
        }
    }

    /// Does again what was last undone, with the Selection it had then.
    /// Anything new done since undoing has made that history; see
    /// [`Editor::undo`].
    pub fn redo(&mut self) {
        self.finish_placing();
        if !self.is_gesturing() {
            let selection = self.history.redo(&mut self.scene, &mut self.entropy);
            self.restored(selection);
        }
    }

    /// Whether a pointer is down making a change that is not finished.
    fn is_gesturing(&self) -> bool {
        self.shape.is_some()
            || self.stroke.is_some()
            || self.erasing.is_some()
            || self.linear.as_ref().is_some_and(|l| !l.is_placing())
            || self.moving.is_some()
            || self.transforming.is_some()
            || self.point_dragging.is_some()
            || self.box_selecting.is_some()
    }

    /// Shows the Scene as undo or redo left it, with `selection` selected
    /// as far as its Elements are still there. Nothing to do for `None`.
    fn restored(&mut self, selection: Option<Selection>) {
        let Some(selection) = selection else {
            return;
        };
        self.taps.cancel();
        self.selection = history::without_gone(selection, &self.scene);
        self.sync_editing();
        // The points may have been added or taken away: no index is right.
        self.points.clear();
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// What the properties panel shows: the values of the Selection's
    /// Elements (`None` where they differ), or, with nothing selected, the
    /// style of the next Element. See [`crate::properties`].
    pub fn properties(&self) -> Properties {
        // The text being edited is what the panel is for.
        if let Some(editing) = &self.text
            && let Some(element) = self.scene.element_by_id(editing.id())
        {
            return Properties::of_selection(&[element]);
        }
        if self.selection.is_empty() {
            return Properties {
                has_text_options: self.tool == Tool::Text,
                ..Properties::of_style(&self.style)
            };
        }
        let selected: Vec<Element> = self
            .selection
            .ids()
            .iter()
            .filter_map(|id| self.scene.element_by_id(id))
            .collect();
        Properties::of_selection(&selected)
    }

    /// Sets `property` on every Element of the Selection that has it, as one
    /// undo step, and makes it the style of the Elements drawn next, as on
    /// excalidraw.com. With nothing selected it only does the latter. Does
    /// nothing while a pointer is drawing or moving.
    pub fn set_property(&mut self, property: Property) {
        // A text that is being edited takes the property as part of its
        // editing: the text view stays.
        if let Some(editing) = self.text.as_mut()
            && let Some(measurer) = self.measurer.as_ref()
        {
            self.style.set(&property);
            if editing.apply(&mut self.scene, &mut self.entropy, &property, &*measurer.0) {
                self.changed(false);
            }
            return;
        }
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        self.style.set(&property);
        let mut step = Step::begin(&self.selection);
        let versions = self.selected_text_versions();
        let mut changed = properties::apply(
            &mut self.scene,
            &mut self.entropy,
            self.selection.ids(),
            &property,
            |scene, id| step.touch(scene, id),
        );
        if matches!(property, Property::FontSize(_) | Property::TextAlign(_)) {
            changed |= self.refit_changed_texts(&versions);
        }
        if changed {
            self.history.record(step, &self.scene, &self.selection);
            self.changed(false);
        }
    }

    /// The ids and versions of the selected texts.
    fn selected_text_versions(&self) -> Vec<(String, u32)> {
        self.selection
            .ids()
            .iter()
            .filter_map(|id| self.scene.element_by_id(id))
            .filter(|element| element.kind() == ElementType::Text)
            .map(|element| (element.id().to_owned(), element.version()))
            .collect()
    }

    /// Lays out the texts again that are not the versions they were (and so
    /// are already part of the step): a change of their font size or
    /// alignment may change how big they are and where. Returns whether any
    /// changed.
    fn refit_changed_texts(&mut self, versions: &[(String, u32)]) -> bool {
        let Some(measurer) = self.measurer.as_ref() else {
            return false;
        };
        let containers = Containers::new(&self.scene);
        let refits: Vec<_> = self
            .selection
            .ids()
            .iter()
            .filter_map(|id| self.scene.element_by_id(id))
            .filter(|element| {
                versions
                    .iter()
                    .any(|(id, version)| id == element.id() && *version != element.version())
            })
            .filter_map(|element| {
                let members =
                    text::refit(&element, containers.of(&element).as_ref(), &*measurer.0)?;
                Some((element.handle(), members))
            })
            .collect();
        let changed = !refits.is_empty();
        for (handle, members) in refits {
            self.scene.update(handle, self.entropy.integer(), members);
        }
        changed
    }

    /// The ids of the selected Elements, in the order they were selected.
    pub fn selection(&self) -> &[String] {
        self.selection.ids()
    }

    /// Where the Action Bar is shown, or `None` if it is not: it is shown
    /// next to a non-empty Selection, except while a pointer is dragging it,
    /// or drawing, or box-selecting. The anchor is in view coordinates, so it
    /// follows the Selection through panning and zooming: clients read it
    /// with the viewport after every input and command. See
    /// [`crate::action_bar`].
    pub fn action_bar(&self) -> Option<ActionBar> {
        if self.is_gesturing() {
            return None;
        }
        ActionBar::of(
            &self.scene,
            &self.selection,
            self.editing_group.as_deref(),
            self.navigation.viewport(),
        )
    }

    /// Duplicates the Selection (the `duplicate` command): each Element gets
    /// a copy 10 units right and down of it, as on excalidraw.com, and the
    /// copies become the Selection. One undo step. Does nothing with nothing
    /// selected, or while a pointer is drawing or moving. See
    /// [`crate::duplicate`].
    pub fn duplicate(&mut self) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        let copies = duplicate::duplicate(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        );
        if copies.is_empty() {
            return;
        }
        self.selection = copies;
        self.sync_editing();
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// The text to put on the clipboard for copying the Selection: Excalidraw's
    /// clipboard JSON (`excalidraw/clipboard`), with the labels of selected
    /// containers and the files of selected images, which excalidraw.com
    /// pastes. `None` with nothing selected. See [`crate::clipboard`].
    pub fn copy_selection(&self) -> Option<String> {
        clipboard::copy(&self.scene, &self.selection)
    }

    /// Copies the Selection (see [`Editor::copy_selection`]) and deletes it
    /// (see [`Editor::delete_selection`]), which is one undo step. `None`,
    /// and nothing deleted, with nothing selected or while a pointer is
    /// drawing or moving.
    pub fn cut_selection(&mut self) -> Option<String> {
        self.finish_placing();
        if self.is_gesturing() {
            return None;
        }
        let copied = self.copy_selection()?;
        self.delete_selection();
        Some(copied)
    }

    /// Pastes `text` in the middle of the view. See [`Editor::paste_at`].
    pub fn paste(&mut self, text: &str) -> bool {
        let (width, height) = self.navigation.view_size();
        self.paste_at(text, width / 2.0, height / 2.0)
    }

    /// Pastes `text`, if it is Excalidraw's clipboard JSON (from
    /// [`Editor::copy_selection`] or excalidraw.com): its Elements are added
    /// with new ids, with the middle of them at the point `(x, y)` of the
    /// view. They become the Selection and the Tool returns to selection.
    /// One undo step. Returns whether anything was pasted: `false` for any
    /// other text, for the client to treat as it likes, and while a pointer
    /// is drawing or moving. See [`crate::clipboard`].
    pub fn paste_at(&mut self, text: &str, x: f64, y: f64) -> bool {
        self.finish_placing();
        if self.is_gesturing() {
            return false;
        }
        let centre = self.navigation.viewport().scene_at(x, y);
        let mut step = Step::begin(&self.selection);
        let Some(ids) =
            clipboard::paste(&mut self.scene, &mut self.entropy, &mut step, text, centre)
        else {
            return false;
        };
        self.set_tool(Tool::Selection);
        self.editing_group = None;
        self.selection.select(ids);
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed(true);
        true
    }

    /// Puts the `images` (pasted or dropped; see [`crate::insert_image`]) in
    /// the middle of the view. See [`Editor::insert_images_at`].
    pub fn insert_images(&mut self, images: Vec<ImageFile>) {
        let (width, height) = self.navigation.view_size();
        self.insert_images_at(images, width / 2.0, height / 2.0);
    }

    /// Puts the `images` as Elements around the point `(x, y)` of the view
    /// (where they were dropped): their files are stored in the Drawing,
    /// they become the Selection and the Tool returns to selection. One undo
    /// step. Images that have no pixels or bytes are skipped. Does nothing
    /// while a pointer is drawing or moving.
    pub fn insert_images_at(&mut self, images: Vec<ImageFile>, x: f64, y: f64) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let viewport = self.navigation.viewport();
        let placement = Placement {
            view_height: self.navigation.view_size().1,
            zoom: viewport.zoom(),
            centre: viewport.scene_at(x, y),
        };
        let mut step = Step::begin(&self.selection);
        let ids = insert_image::insert(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.style,
            &placement,
            &images,
        );
        if ids.is_empty() {
            return;
        }
        self.set_tool(Tool::Selection);
        self.editing_group = None;
        self.selection.select(ids);
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// Deletes the Selection (the `delete` command), and with it the labels
    /// of its containers; arrows bound to what is deleted are unbound. One
    /// undo step, which selects what was deleted again. Does nothing with
    /// nothing selected, or while a pointer is drawing or moving. See
    /// [`crate::delete`].
    pub fn delete_selection(&mut self) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        delete::delete(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
        );
        self.selection.clear();
        self.sync_editing();
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed(false);
    }

    /// Moves the Selection in z-order (the `bring forward`, `send backward`,
    /// `bring to front` and `send to back` commands), as Excalidraw does: see
    /// [`crate::z_order`]. One undo step. Does nothing with nothing selected,
    /// while a pointer is drawing or moving, or if the Selection is already
    /// as far as it goes.
    pub fn move_in_z_order(&mut self, how: ZOrder) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        if !z_order::move_selection(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            how,
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        // The handles of the Elements now stand for others.
        self.changed(true);
    }

    /// Puts the selected Elements into one new group (the `group` command),
    /// as Excalidraw does: see [`crate::group`]. Tapping any of them then
    /// selects them all. One undo step. Does nothing unless two or more
    /// Elements are selected and not all of them in a group already (see
    /// [`ActionBar::can_group`]), or while a pointer is drawing or moving.
    pub fn group(&mut self) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        if !group::group(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        // The Elements moved up: the handles of some now stand for others.
        self.changed(true);
    }

    /// Dissolves the groups that are selected (the `ungroup` command): the
    /// Elements stay selected, now without them. Groups inside and outside
    /// those stay. One undo step. Does nothing unless a group is selected
    /// (see [`ActionBar::can_ungroup`]), or while a pointer is drawing or
    /// moving.
    pub fn ungroup(&mut self) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        if !group::ungroup(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        self.changed(false);
    }

    /// Carries out `command`, e.g. from a keyboard shortcut (see
    /// [`crate::shortcuts`]), exactly as the matching method does.
    pub fn perform(&mut self, command: Command) {
        match command {
            Command::SetTool(tool) => self.set_tool(tool),
            Command::ToggleToolLock => self.set_tool_lock(!self.tool_lock),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Duplicate => self.duplicate(),
            Command::Delete => self.delete_selection(),
            Command::Group => self.group(),
            Command::Ungroup => self.ungroup(),
            Command::MoveInZOrder(how) => self.move_in_z_order(how),
            Command::SelectAll => self.select_all(),
            Command::ZoomIn => self.zoom_in(),
            Command::ZoomOut => self.zoom_out(),
            Command::ResetZoom => self.reset_zoom(),
            Command::ZoomToFit => self.zoom_to_fit(),
        }
    }

    /// Selects every shown, unlocked Element of the Scene (the `select all`
    /// command).
    pub fn select_all(&mut self) {
        self.commit_text();
        let ids: Vec<String> = self
            .shown()
            .filter(|e| !e.is_locked())
            .map(|e| e.id().to_owned())
            .collect();
        if self.selection.select(ids) {
            self.overlay_revision += 1;
        }
        self.editing_group = None;
        self.sync_points();
    }

    /// Selects the Element a tap hits, or clears the Selection if the tap
    /// was a deliberate one on empty Canvas (see [`crate::tap`]); moves the
    /// Selection with a drag that began on an Element, and box-selects with
    /// one that began on empty Canvas.
    ///
    /// Only the selection Tool taps and drags, and never with two fingers
    /// down: they navigate, and must not disturb the Selection, nor leave a
    /// move half done. A finger in Pencil-only drawing taps like the
    /// selection Tool does whatever the Tool is, but its drags are the
    /// pan's, see [`crate::pencil_only`].
    fn select(&mut self, event: PointerEvent) {
        let finger = self.pencil_only.restricts(event.kind);
        let tool = if finger { Tool::Selection } else { self.tool };
        let tapping = matches!(tool, Tool::Selection | Tool::Text);
        if !tapping || self.navigation.is_pinching() {
            self.taps.cancel();
            self.drop_move();
            self.drop_transform();
            self.drop_point_drag();
            self.drop_box_select();
            return;
        }
        if tool == Tool::Text || self.text.is_some() {
            self.follow_text_tap(event);
            return;
        }
        self.follow_move(event);
        self.follow_transform(event);
        self.follow_point_drag(event);
        self.follow_box_select(event);
        match self.taps.track(event) {
            Some(Recognized::Tap(tap)) => self.tap(tap, event.modifiers.shift),
            Some(Recognized::DragBegan(origin)) if !finger => self.begin_drag(origin, event),
            Some(Recognized::DragBegan(_)) | None => {}
        }
    }

    /// With `shift`, a tap on an Element adds it to the Selection or, if it
    /// is in it, takes it out, and a tap on empty Canvas leaves the Selection
    /// alone.
    fn tap(&mut self, tap: Tap, shift: bool) {
        // Before the handles: they are as big as a small text, which would
        // otherwise never be edited by a finger.
        if tap.double && !shift && self.edit_text_tapped(tap) {
            return;
        }
        // A tap on a handle is a grab that never dragged: it neither selects
        // nor clears the Selection. On a point handle it selects the point.
        if let Some((handles, grab)) = self.grabbed_point(tap.x, tap.y, tap.kind) {
            if let Grab::Point(index) = grab {
                if shift {
                    self.points.toggle(handles.id(), index);
                } else {
                    self.points.select_only(handles.id(), index);
                }
                self.overlay_revision += 1;
            }
            return;
        }
        if self.grabbed_handle(tap.x, tap.y, tap.kind).is_some() {
            return;
        }
        let hit = self.hit(tap.x, tap.y, tap.kind);
        let before = self.selection.clone();
        match hit {
            // A double-tap on an Element of a selected group goes into it.
            Some(id) if tap.double && !shift && self.enter_group(&id) => {}
            Some(id) if shift => {
                // Tapping outside the group that was gone into leaves it.
                self.editing_group = self.editing_around(&id);
                let unit = self.unit_of(&id);
                self.selection.toggle_all(&unit);
            }
            Some(id) => {
                self.select_unit(&id, false);
            }
            None if tap.is_short() && !shift => {
                self.selection.clear();
                self.editing_group = None;
            }
            None => {}
        }
        if self.selection != before {
            self.overlay_revision += 1;
        }
    }

    /// Opens the text view on what a double-tap hit, unless the double-tap
    /// goes into a group instead: on a text of its own, or on the label of a
    /// shape (made if it has none; the inside of a hollow shape counts as the
    /// shape). Returns whether it did.
    fn edit_text_tapped(&mut self, tap: Tap) -> bool {
        let hit = self.hit(tap.x, tap.y, tap.kind);
        let target = match hit {
            Some(id) => Some(id),
            None => {
                let at = self.navigation.viewport().scene_at(tap.x, tap.y);
                self.label_container_at(at)
            }
        };
        let Some(id) = target else {
            return false;
        };
        let editable = self.is_free_text(&id)
            || self
                .scene
                .element_by_id(&id)
                .is_some_and(|e| text::takes_label(e.kind()));
        let goes_into_group = group::entered(
            &self.scene,
            &self.selection,
            &id,
            self.editing_group.as_deref(),
        )
        .is_some();
        if !editable || goes_into_group {
            return false;
        }
        if self.is_free_text(&id) {
            self.begin_existing_text(&id);
        } else {
            self.begin_label(&id);
        }
        self.text.is_some()
    }

    /// The topmost shape that can hold a label with `at` (in the Scene) in
    /// its box, hollow or not.
    fn label_container_at(&self, at: Point) -> Option<String> {
        self.scene
            .elements()
            .filter(hit_test::is_selectable)
            .filter(|e| text::takes_label(e.kind()))
            .filter(|e| Frame::of(e).contains_turned(at, e.angle(), 0.0))
            .last()
            .map(|e| e.id().to_owned())
    }

    /// The group that was gone into, if it is one the Element `id` is in:
    /// selecting anything else leaves it.
    fn editing_around(&self, id: &str) -> Option<String> {
        let editing = self.editing_group.as_deref()?;
        let element = self.scene.element_by_id(id)?;
        element.is_in_group(editing).then(|| editing.to_owned())
    }

    /// What selecting the Element `id` selects: it, and its group (see
    /// [`crate::group`]).
    fn unit_of(&self, id: &str) -> Vec<String> {
        group::expand(&self.scene, &[id.to_owned()], self.editing_group.as_deref())
    }

    /// Selects what selecting the Element `id` selects, in place of the
    /// Selection or, if `additive`, in addition to it. Selecting outside the
    /// group that was gone into leaves it. Returns whether the Selection
    /// changed.
    fn select_unit(&mut self, id: &str, additive: bool) -> bool {
        self.editing_group = self.editing_around(id);
        let unit = self.unit_of(id);
        if additive {
            let kept = self.selection.ids().iter().cloned();
            self.selection.select(kept.chain(unit).collect::<Vec<_>>())
        } else {
            self.selection.select(unit)
        }
    }

    /// Goes into the selected group the Element `id` is in, if there is one,
    /// and selects the Element (or its group inside the group that was gone
    /// into). Returns whether it did.
    fn enter_group(&mut self, id: &str) -> bool {
        let Some(entered) = group::entered(
            &self.scene,
            &self.selection,
            id,
            self.editing_group.as_deref(),
        ) else {
            return false;
        };
        self.editing_group = Some(entered);
        let unit = self.unit_of(id);
        self.selection.select(unit);
        true
    }

    /// Leaves the group that was gone into unless every selected Element is
    /// still in it.
    fn sync_editing(&mut self) {
        let Some(editing) = self.editing_group.as_deref() else {
            return;
        };
        let inside = !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                self.scene
                    .element_by_id(id)
                    .is_some_and(|e| e.is_in_group(editing))
            });
        if !inside {
            self.editing_group = None;
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

    /// The transform handle a pointer of `kind` that went down at the view
    /// point `(x, y)` takes hold of, if any.
    fn grabbed_handle(&self, x: f64, y: f64, kind: PointerKind) -> Option<(TransformBox, Handle)> {
        let viewport = self.navigation.viewport();
        let transform_box = TransformBox::of(&self.scene, &self.selection)?;
        let handle = transform_box.grab(viewport.scene_at(x, y), kind, viewport.zoom())?;
        Some((transform_box, handle))
    }

    /// The handles of the points of the selected line or arrow, if there is
    /// one, for a pointer of `kind`.
    fn point_handles(&self, kind: PointerKind) -> Option<PointHandles> {
        let zoom = self.navigation.viewport().zoom();
        PointHandles::of(&self.scene, &self.selection, kind, zoom)
    }

    /// The point handles, and which of them, a pointer of `kind` that went
    /// down at the view point `(x, y)` takes hold of, if any.
    fn grabbed_point(&self, x: f64, y: f64, kind: PointerKind) -> Option<(PointHandles, Grab)> {
        let viewport = self.navigation.viewport();
        let handles = self.point_handles(kind)?;
        let grab = handles.grab(viewport.scene_at(x, y), kind, viewport.zoom())?;
        Some((handles, grab))
    }

    /// Starts moving points of the selected line or arrow if the drag began
    /// on a handle of one: the point is selected, if it is not yet, and moves
    /// with the other selected points; the middle of a segment gets a point
    /// that moves (see [`PointHandles::drag`]).
    fn begin_point_drag(&mut self, origin: Origin, event: PointerEvent) {
        let Some((handles, grab)) = self.grabbed_point(origin.x, origin.y, origin.kind) else {
            return;
        };
        self.overlay_revision += 1;
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        self.point_dragging = Some(handles.drag(
            Step::begin(&self.selection),
            &self.scene,
            event.id,
            start,
            &mut self.points,
            grab,
            event.modifiers.shift,
            binding::reach(origin.kind, self.navigation.viewport().zoom()),
        ));
        self.follow_point_drag(event);
    }

    /// Takes the points along with the pointer that moves them.
    fn follow_point_drag(&mut self, event: PointerEvent) {
        let Some(dragging) = self
            .point_dragging
            .as_mut()
            .filter(|d| d.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if dragging.drag_to(&mut self.scene, &mut self.entropy, to) {
                    self.changed(false);
                }
            }
            PointerPhase::Up => {
                if let Some(dragging) = self.point_dragging.take() {
                    self.history
                        .record(dragging.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_point_drag(),
        }
    }

    /// Puts the points being dragged, if any, back where they were, and
    /// selects again the points that were selected before the drag.
    fn drop_point_drag(&mut self) {
        if let Some(dragging) = self.point_dragging.take() {
            self.points = dragging.cancel(&mut self.scene, &mut self.entropy);
            self.overlay_revision += 1;
            self.changed(false);
        }
    }

    /// The indices of the points of the selected line or arrow that are
    /// selected: tapping a point selects it, shift adds to them, and
    /// dragging one moves them all. Empty unless a line or arrow is the only
    /// Element selected.
    pub fn selected_points(&self) -> &[usize] {
        self.points.indices()
    }

    /// Deletes the selected points of the selected line or arrow (the
    /// `delete` command when points are selected, which then takes the place
    /// of deleting the Element). A line or arrow keeps at least two points: if
    /// fewer would be left, it is deleted. The point before the first one
    /// deleted is selected next. Does nothing if no point is selected.
    pub fn delete_selected_points(&mut self) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let Some(handles) = self.point_handles(self.last_pointer) else {
            return;
        };
        let deleted = self.points.indices().to_vec();
        let Some(&first) = deleted.iter().min() else {
            return;
        };
        let mut step = Step::begin(&self.selection);
        step.touch(&self.scene, handles.id());
        let remaining = handles.without(&deleted);
        let last = handles.points().len() - 1;
        if remaining.len() < 2 {
            binding::unbind_all(&mut self.scene, &mut self.entropy, &mut step, handles.id());
            self.scene
                .mark_deleted(handles.id(), self.entropy.integer());
            self.selection.clear();
            self.points.clear();
        } else {
            // The point next to an end that is deleted becomes the end, and
            // is not bound to anything.
            for (index, end) in [(0, End::Start), (last, End::End)] {
                if deleted.contains(&index) {
                    binding::unbind(
                        &mut self.scene,
                        &mut self.entropy,
                        &mut step,
                        handles.id(),
                        end,
                    );
                }
            }
            edit_points::reshape(
                &mut self.scene,
                &mut self.entropy,
                handles.handle(),
                &remaining,
            );
            binding::follow(&mut self.scene, &mut self.entropy, handles.id());
            self.points
                .select_only(handles.id(), first.saturating_sub(1));
        }
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// Makes the highlight the shape an arrow end is over, if one is being put
    /// somewhere.
    fn sync_highlight(&mut self) {
        let current = self
            .linear
            .as_ref()
            .and_then(LinearDrawing::highlight)
            .or_else(|| self.point_dragging.as_ref().and_then(PointDrag::highlight))
            .map(str::to_owned);
        if current != self.highlight {
            self.highlight = current;
            self.overlay_revision += 1;
        }
    }

    /// Forgets the selected points that the Selection no longer has.
    fn sync_points(&mut self) {
        if self.points.retain_existing(&self.scene, &self.selection) {
            self.overlay_revision += 1;
        }
    }

    /// Starts resizing or rotating the Selection if the drag began on a
    /// transform handle of it.
    fn begin_transform(&mut self, origin: Origin, event: PointerEvent) {
        let Some((transform_box, handle)) = self.grabbed_handle(origin.x, origin.y, origin.kind)
        else {
            return;
        };
        let viewport = self.navigation.viewport();
        let start = viewport.scene_at(origin.x, origin.y);
        self.transforming = Some(TransformGesture::begin(
            Step::begin(&self.selection),
            &self.scene,
            &self.selection,
            event.id,
            transform_box,
            handle,
            start,
        ));
        self.follow_transform(event);
    }

    /// Takes the Selection along with the handle its pointer holds.
    fn follow_transform(&mut self, event: PointerEvent) {
        let Some(transforming) = self
            .transforming
            .as_mut()
            .filter(|t| t.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if transforming.drag_to(
                    &mut self.scene,
                    &mut self.entropy,
                    to,
                    event.modifiers,
                    self.measurer.as_ref().map(|m| m.0.as_ref()),
                ) {
                    self.changed(false);
                }
            }
            PointerPhase::Up => {
                if let Some(transforming) = self.transforming.take() {
                    self.history
                        .record(transforming.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_transform(),
        }
    }

    /// Puts the Selection being transformed, if any, back as it was.
    fn drop_transform(&mut self) {
        if let Some(transforming) = self.transforming.take() {
            transforming.cancel(&mut self.scene, &mut self.entropy);
            self.changed(false);
        }
    }

    /// Starts what a drag of the selection Tool is for, by where it began: on
    /// a transform handle it resizes or rotates the Selection, in the box
    /// around a Selection of several Elements or on an Element it moves, and
    /// on empty Canvas it box-selects. `event` is the step that turned the
    /// press into a drag.
    fn begin_drag(&mut self, origin: Origin, event: PointerEvent) {
        self.begin_transform(origin, event);
        if self.transforming.is_some() {
            return;
        }
        self.begin_point_drag(origin, event);
        if self.point_dragging.is_some() {
            return;
        }
        if self.in_selection_box(origin) {
            return self.begin_move(None, origin, event);
        }
        match self.hit(origin.x, origin.y, origin.kind) {
            Some(id) => self.begin_move(Some(&id), origin, event),
            None => self.begin_box_select(origin, event),
        }
    }

    /// Whether a press at `origin` is in the box around a Selection of
    /// several Elements, where a drag moves them all, whatever is under it
    /// (Excalidraw's `isHittingCommonBoundingBoxOfSelectedElements`). A
    /// single Element's box is part of hitting it, see
    /// [`hit_test::topmost`].
    fn in_selection_box(&self, origin: Origin) -> bool {
        if self.selection.ids().len() < 2 {
            return false;
        }
        let viewport = self.navigation.viewport();
        TransformBox::of(&self.scene, &self.selection).is_some_and(|transform_box| {
            transform_box.encloses(viewport.scene_at(origin.x, origin.y), viewport.zoom())
        })
    }

    /// Starts moving the Selection by a drag that began on the Element `on`,
    /// if any: on the Selection itself, or else on an Element that becomes
    /// the Selection, or with shift joins it.
    fn begin_move(&mut self, on: Option<&str>, origin: Origin, event: PointerEvent) {
        // Undoing the move also undoes the selecting it began with.
        let step = Step::begin(&self.selection);
        self.drag_editing = self.editing_group.clone();
        let changed = match on {
            Some(id) if !self.selection.contains(id) => self.select_unit(id, event.modifiers.shift),
            _ => false,
        };
        if changed {
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
                let lock_axis = event.modifiers.shift;
                if moving.drag_to(&mut self.scene, &mut self.entropy, to, lock_axis) {
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

    /// Puts the Selection being moved, if any, back where it was, and selects
    /// again what was selected before the drag began.
    fn drop_move(&mut self) {
        if let Some(moving) = self.moving.take() {
            let selection = moving.cancel(&mut self.scene, &mut self.entropy);
            self.show_selection(selection);
            self.editing_group = self.drag_editing.take();
            self.changed(false);
        }
    }

    /// Starts a box at where the drag began on empty Canvas; with shift it
    /// adds to the Selection instead of replacing it.
    fn begin_box_select(&mut self, origin: Origin, event: PointerEvent) {
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        self.drag_editing = self.editing_group.clone();
        self.box_selecting = Some(BoxSelect::begin(
            event.id,
            start,
            &self.selection,
            self.editing_group.as_deref(),
            event.modifiers.shift,
        ));
        self.follow_box_select(event);
    }

    /// Stretches the box to the pointer that draws it, and selects what it
    /// encloses.
    fn follow_box_select(&mut self, event: PointerEvent) {
        let Some(boxing) = self
            .box_selecting
            .as_mut()
            .filter(|b| b.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if boxing.drag_to(to) {
                    let selection = boxing.selection(&self.scene);
                    self.show_selection(selection);
                    self.overlay_revision += 1;
                }
            }
            PointerPhase::Up => {
                self.box_selecting = None;
                self.overlay_revision += 1;
            }
            PointerPhase::Cancel => self.drop_box_select(),
        }
    }

    /// Takes the box being dragged, if any, away, and the Selection back to
    /// what it was before it.
    fn drop_box_select(&mut self) {
        if let Some(boxing) = self.box_selecting.take() {
            self.show_selection(boxing.cancel());
            self.editing_group = self.drag_editing.take();
            self.overlay_revision += 1;
        }
    }

    fn show_selection(&mut self, selection: Selection) {
        if self.selection != selection {
            self.selection = selection;
            self.overlay_revision += 1;
        }
    }

    /// What clients draw over the Scene: a frame around each selected
    /// Element, the transform handles of the Selection (or, while one is
    /// being dragged, the box-select rectangle instead; or, for a line or
    /// arrow that is alone in the Selection, handles on its points), and a
    /// highlight around the shape an end of an arrow that is being put
    /// somewhere would bind to, in Scene coordinates,
    /// as geometry in the same format as [`Editor::geometry`] (strokes and the
    /// fills of handles and box; no text or images). Clients draw it after the
    /// Scene and do not cache it by Element: its strokes are as wide and as far
    /// from the Elements as they are because of the zoom it was made for.
    pub fn overlay(&self) -> Geometry {
        let zoom = self.navigation.viewport().zoom();
        let selected = self.shown().filter(|e| self.selection.contains(e.id()));
        let mut geometry = overlay::selection(selected, zoom);
        if let Some(boxing) = &self.box_selecting {
            overlay::box_select(&mut geometry, &boxing.bounds(), zoom);
        } else if let Some(transform_box) = TransformBox::of(&self.scene, &self.selection) {
            overlay::transform_handles(&mut geometry, &transform_box, self.last_pointer, zoom);
        } else if let Some(handles) = self.point_handles(self.last_pointer) {
            let selected = self.points.indices();
            overlay::point_handles(&mut geometry, &handles, selected, self.last_pointer, zoom);
        }
        if let Some(outline) = self
            .highlight
            .as_deref()
            .and_then(|id| binding::highlight(&self.scene, id))
        {
            overlay::binding_highlight(&mut geometry, &outline, zoom);
        }
        geometry
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
                let id = shape.id().to_owned();
                match shape.finish(&mut self.scene) {
                    Outcome::Drawn(step) => {
                        // As on excalidraw.com the new shape is selected, to
                        // be adjusted right away, and the Tool returns to
                        // selection; with the Tool lock neither, so that the
                        // next shape can follow.
                        let selected = (!self.tool_lock).then_some(id);
                        if self.selection.select(selected) {
                            self.overlay_revision += 1;
                        }
                        self.history.record(step, &self.scene, &self.selection);
                        if !self.tool_lock {
                            self.assign_tool(Tool::Selection);
                        }
                    }
                    Outcome::Dropped => self.changed(true),
                }
            }
            PointerPhase::Cancel if drawing_pointer == Some(event.id) => self.drop_shape(),
            _ => {}
        }
    }

    /// Follows the pointer with the line or arrow being drawn, or starts one
    /// (see [`crate::draw_linear`]).
    fn draw_linear(&mut self, event: PointerEvent) {
        let zoom = self.navigation.viewport().zoom();
        // Two fingers navigate: they place no points, and a line that one of
        // them began is gone. A line that is waiting for its next point
        // waits on.
        if self.navigation.is_pinching() {
            if let Some(linear) = self.linear.as_mut() {
                let update = linear.interrupt(&mut self.scene, &mut self.entropy);
                self.linear_updated(update);
            }
            return;
        }
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        if let Some(linear) = self.linear.as_mut() {
            let update = linear.follow(&mut self.scene, &mut self.entropy, event, at, zoom);
            self.linear_updated(update);
        } else if event.phase == PointerPhase::Down
            && let Some(kind) = self.tool.linear()
        {
            self.linear = Some(LinearDrawing::begin(
                Step::begin(&self.selection),
                &mut self.scene,
                &mut self.entropy,
                &self.style,
                kind,
                event.id,
                event.kind,
                at,
                zoom,
            ));
            self.changed(true);
        }
    }

    /// Does what the line being drawn needs after a step of a pointer.
    fn linear_updated(&mut self, update: Update) {
        match update {
            Update::Unchanged => {}
            Update::Changed => self.changed(false),
            Update::Finished => self.finish_linear(),
            Update::Abandoned => self.drop_linear(),
        }
    }

    /// Finishes the line being drawn, if any, with the points it has: it
    /// stays in the Scene and is the latest thing that can be undone. Nothing
    /// is left of a line without two points. Returns whether there is a line.
    fn end_linear(&mut self) -> bool {
        let Some(linear) = self.linear.take() else {
            return false;
        };
        let zoom = self.navigation.viewport().zoom();
        match linear.finish(&mut self.scene, &mut self.entropy, zoom) {
            Outcome::Drawn(step) => {
                self.history.record(step, &self.scene, &self.selection);
                self.changed(false);
                true
            }
            Outcome::Dropped => {
                self.changed(true);
                false
            }
        }
    }

    /// Finishes the line being drawn, and with it the Tool's work: it returns
    /// to selection unless locked.
    fn finish_linear(&mut self) {
        if self.end_linear() && !self.tool_lock {
            self.assign_tool(Tool::Selection);
        }
    }

    /// Finishes the text being edited, and the line being drawn if it is
    /// waiting for further points: what commands on the Scene do first.
    fn finish_placing(&mut self) {
        self.commit_text();
        if self.linear.as_ref().is_some_and(LinearDrawing::is_placing) {
            self.finish_linear();
            self.sync_highlight();
        }
    }

    /// Takes the line being drawn, if any, out of the Scene again.
    fn drop_linear(&mut self) {
        if let Some(linear) = self.linear.take() {
            linear.cancel(&mut self.scene, &mut self.entropy);
            self.changed(true);
        }
    }

    /// Follows the pointer with the stroke being drawn, or starts one (see
    /// [`crate::draw_freedraw`]). The Tool stays after the stroke whatever the
    /// Tool lock says.
    fn draw_stroke(&mut self, event: PointerEvent) {
        // Two fingers navigate, and a stroke one of them began ends: a short
        // one was only the finger landing, a longer one is kept as drawn.
        if self.navigation.is_pinching() {
            if let Some(stroke) = self.stroke.take() {
                if stroke.is_spike() {
                    stroke.cancel(&mut self.scene);
                    self.changed(true);
                } else {
                    self.history
                        .record(stroke.finish(), &self.scene, &self.selection);
                }
            }
            return;
        }
        let drawing_pointer = self.stroke.as_ref().map(StrokeDrawing::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        match event.phase {
            PointerPhase::Down if drawing_pointer.is_none() && self.tool == Tool::Freedraw => {
                self.stroke = Some(StrokeDrawing::begin(
                    Step::begin(&self.selection),
                    &mut self.scene,
                    &mut self.entropy,
                    &self.style,
                    event,
                    at,
                ));
                self.changed(true);
            }
            PointerPhase::Move if drawing_pointer == Some(event.id) => {
                let moved = self.stroke.as_mut().is_some_and(|stroke| {
                    stroke.extend(&mut self.scene, &mut self.entropy, event, at)
                });
                if moved {
                    self.changed(false);
                }
            }
            PointerPhase::Up if drawing_pointer == Some(event.id) => {
                if let Some(stroke) = self.stroke.take() {
                    let step = stroke.lift(&mut self.scene, &mut self.entropy, event, at);
                    self.history.record(step, &self.scene, &self.selection);
                    self.changed(false);
                }
            }
            PointerPhase::Cancel if drawing_pointer == Some(event.id) => self.drop_stroke(),
            _ => {}
        }
    }

    /// Follows the pointer with the erase, or starts one (see
    /// [`crate::erase`]). The Tool stays after the erase whatever the Tool
    /// lock says.
    fn erase(&mut self, event: PointerEvent) {
        // Two fingers navigate, and an erase one of them began is called off:
        // nothing is deleted by a finger that was only landing.
        if self.navigation.is_pinching() {
            self.drop_erase();
            return;
        }
        let erasing_pointer = self.erasing.as_ref().map(Erasing::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        let zoom = self.navigation.viewport().zoom();
        match event.phase {
            PointerPhase::Down if erasing_pointer.is_none() && self.tool == Tool::Eraser => {
                self.erasing = Some(Erasing::begin(Step::begin(&self.selection), event, at));
            }
            PointerPhase::Move if erasing_pointer == Some(event.id) => {
                let previewing = self
                    .erasing
                    .as_mut()
                    .is_some_and(|erasing| erasing.drag_to(&self.scene, at, zoom));
                if previewing {
                    self.changed(false);
                }
            }
            PointerPhase::Up if erasing_pointer == Some(event.id) => {
                if let Some(erasing) = self.erasing.take() {
                    let step = erasing.finish(&mut self.scene, &mut self.entropy, at, zoom);
                    // What was erased cannot stay selected.
                    let remaining = history::without_gone(self.selection.clone(), &self.scene);
                    self.show_selection(remaining);
                    self.history.record(step, &self.scene, &self.selection);
                    self.changed(false);
                }
            }
            PointerPhase::Cancel if erasing_pointer == Some(event.id) => self.drop_erase(),
            _ => {}
        }
    }

    /// Calls off the erase in progress, if any: what was previewed as about
    /// to be erased is shown as it is again.
    fn drop_erase(&mut self) {
        if self.erasing.take().is_some() {
            self.changed(false);
        }
    }

    /// Takes the stroke being drawn, if any, out of the Scene again.
    fn drop_stroke(&mut self) {
        if let Some(stroke) = self.stroke.take() {
            stroke.cancel(&mut self.scene);
            self.changed(true);
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
        bounds_of(self.shown())
    }

    fn shown(&self) -> impl Iterator<Item = Element<'_>> {
        self.scene.elements().filter(|e| !e.is_deleted())
    }
}

/// The smallest rectangle containing `elements` as they are shown (see
/// [`Frame::shown`]), or `None` if there are none.
fn bounds_of<'a>(elements: impl Iterator<Item = Element<'a>>) -> Option<Bounds> {
    elements
        .map(|element| Frame::shown(&element))
        .reduce(Frame::union)
        .map(|frame| Bounds {
            min_x: frame.min.0,
            min_y: frame.min.1,
            max_x: frame.max.0,
            max_y: frame.max.1,
        })
}
