use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::binding::{self, Attachments, End};
use crate::box_select::BoxSelect;
use crate::clipboard;
use crate::color::{dark_mode, parse_color};
use crate::context_menu::ContextMenu;
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
use crate::json::Value;
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

mod commands;
mod cursor;
mod drawing_tools;
mod geometry;
use geometry::{GeometryCache, bounds_of};
mod selection_tool;
mod text_entry;

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
    /// Changes whenever a Drawing is loaded (handles are reassigned) or dark
    /// mode is switched (colours change), see [`Geometry::generation`].
    generation: u64,
    /// What [`Editor::geometry`] generated for shapes, see `GeometryCache`.
    geometry_cache: RefCell<GeometryCache>,
    /// Colours are shown dark, see [`Editor::set_dark_mode`].
    dark_mode: bool,
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
            geometry_cache: RefCell::default(),
            dark_mode: false,
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
        self.generation = next_generation();
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
        self.changed();
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
        self.geometry_cache.get_mut().clear();
        self.changed();
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
    /// [`crate::image`]). Colours are the displayed ones, see
    /// [`Editor::set_dark_mode`].
    pub fn geometry(&self) -> Geometry {
        // The text view shows the text that is being edited.
        let editing = self.text.as_ref().map(TextEditing::handle);
        let geometry = self.geometry_of(self.generation, self.dark_mode, |e| {
            editing != Some(e.handle())
        });
        self.prune_geometry_cache();
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
    /// Any Tool but selection also clears the Selection and leaves the group
    /// that was gone into, and so does a pointer going down with one.
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
            if tool != Tool::Selection {
                self.clear_selection();
            }
            self.sync_highlight();
        }
    }

    /// Empties the Selection and leaves the group that was gone into, as
    /// excalidraw.com does when a drawing Tool is chosen or used. It is no
    /// undo step: a step begun afterwards records the empty Selection.
    fn clear_selection(&mut self) {
        if self.selection.clear() {
            self.overlay_revision += 1;
        }
        self.sync_points();
        self.editing_group = None;
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
            // A press that pans (space, a mouse button) selects nothing.
            if event.phase == PointerPhase::Down
                && self.tool != Tool::Selection
                && !self.navigation.suspends_tool()
            {
                self.clear_selection();
            }
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
        self.changed();
    }

    /// What the properties panel shows: the values of the Selection's
    /// Elements (`None` where they differ), or, with nothing selected, the
    /// style of the next Element. See [`crate::properties`].
    pub fn properties(&self) -> Properties {
        let tool = self.tool.element();
        // The text being edited is what the panel is for.
        if let Some(editing) = &self.text
            && let Some(element) = self.scene.element_by_id(editing.id())
        {
            return Properties::of_selection(&[element], tool, &self.style);
        }
        if self.selection.is_empty() {
            return Properties::of_style(&self.style, tool);
        }
        // With the labels of selected containers, whose text options the
        // panel offers, as excalidraw.com does.
        let covered: Vec<Element> = self.selection.covered(&self.scene).collect();
        Properties::of_selection(&covered, tool, &self.style)
    }

    /// Whether the chrome shows the properties panel (Excalidraw's
    /// `showSelectedShapeActions`): while a text is edited, a Tool that
    /// draws is active or something is selected. The selection, hand and
    /// eraser Tools have nothing to style on their own.
    pub fn shows_properties(&self) -> bool {
        self.text.is_some()
            || !matches!(self.tool, Tool::Selection | Tool::Hand | Tool::Eraser)
            || !self.selection.is_empty()
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
                self.changed();
            }
            return;
        }
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        self.style.set(&property);
        let mut step = Step::begin(&self.selection);
        // The labels of selected containers change with them, as with
        // Excalidraw's `includeBoundText`: their colour, opacity, font size
        // and alignment (they have none of the other properties).
        let targets: Vec<(String, ElementType, u32)> = self
            .selection
            .covered(&self.scene)
            .map(|e| (e.id().to_owned(), e.kind(), e.version()))
            .collect();
        let ids: Vec<String> = targets.iter().map(|(id, ..)| id.clone()).collect();
        let texts: Vec<(String, u32)> = targets
            .into_iter()
            .filter(|(_, kind, _)| *kind == ElementType::Text)
            .map(|(id, _, version)| (id, version))
            .collect();
        let refits = matches!(property, Property::FontSize(_) | Property::TextAlign(_));
        // A text that is laid out again may change size, and a label its
        // container: the arrows bound to them follow, as `changeFontSize`
        // has them do.
        let attachments = refits.then(|| {
            let mut changing: Vec<String> = texts.iter().map(|(id, _)| id.clone()).collect();
            changing.extend(self.label_containers(&texts));
            Attachments::begin(&mut step, &self.scene, changing)
        });
        let mut changed = properties::apply(
            &mut self.scene,
            &mut self.entropy,
            &ids,
            &property,
            |scene, id| step.touch(scene, id),
        );
        if let Some(attachments) = attachments {
            changed |= self.refit_changed_texts(&mut step, &texts);
            attachments.update(&mut self.scene, &mut self.entropy, &mut step);
        }
        if changed {
            self.history.record(step, &self.scene, &self.selection);
            self.changed();
        }
    }

    /// The ids of the shapes (not arrows) that the `texts` are labels of.
    fn label_containers(&self, texts: &[(String, u32)]) -> Vec<String> {
        let containers = Containers::new(&self.scene);
        texts
            .iter()
            .filter_map(|(id, _)| containers.of(&self.scene.element_by_id(id)?))
            .filter(|container| container.kind() != ElementType::Arrow)
            .map(|container| container.id().to_owned())
            .collect()
    }

    /// Lays out the `texts` (ids and the versions they were) again that are
    /// not the versions they were (and so are already part of `step`): a
    /// change of their font size or alignment may change how big they are
    /// and where. A label's shape grows to hold it, and never shrinks
    /// (Excalidraw's `redrawTextBoundingBox`). Returns whether any changed.
    fn refit_changed_texts(&mut self, step: &mut Step, texts: &[(String, u32)]) -> bool {
        let Some(measurer) = self.measurer.as_ref() else {
            return false;
        };
        let changed: Vec<String> = texts
            .iter()
            .filter(|(id, version)| {
                self.scene
                    .element_by_id(id)
                    .is_some_and(|text| text.version() != *version)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let containers = Containers::new(&self.scene);
        let grown: Vec<(String, u32, (f64, f64))> = changed
            .iter()
            .filter_map(|id| {
                let label = self.scene.element_by_id(id)?;
                let shape = containers
                    .of(&label)
                    .filter(|shape| shape.kind() != ElementType::Arrow)?;
                let size = text::container_size_for(&label, &shape, shape.height(), &*measurer.0);
                (size != (shape.width(), shape.height()))
                    .then(|| (shape.id().to_owned(), shape.handle(), size))
            })
            .collect();
        for (id, handle, (width, height)) in grown {
            step.touch(&self.scene, &id);
            let members = [
                ("width", Value::Number(width)),
                ("height", Value::Number(height)),
            ];
            self.scene.update(handle, self.entropy.integer(), members);
        }
        let containers = Containers::new(&self.scene);
        let refits: Vec<_> = changed
            .iter()
            .filter_map(|id| {
                let element = self.scene.element_by_id(id)?;
                let members =
                    text::refit(&element, containers.of(&element).as_ref(), &*measurer.0)?;
                Some((element.handle(), members))
            })
            .collect();
        let refitted = !refits.is_empty();
        for (handle, members) in refits {
            self.scene.update(handle, self.entropy.integer(), members);
        }
        refitted
    }

    /// The ids of the selected Elements, in the order they were selected.
    pub fn selection(&self) -> &[String] {
        self.selection.ids()
    }

    /// What clients draw over the Scene: a frame around each selected
    /// Element (a dashed one around each selected group and the group that
    /// was gone into instead), the transform handles of the Selection (or, while one is
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
        let colors = overlay::Colors::of(self.dark_mode);
        let mut geometry = overlay::selection(
            &self.scene,
            &self.selection,
            self.editing_group.as_deref(),
            zoom,
            colors,
        );
        if let Some(boxing) = &self.box_selecting {
            overlay::box_select(&mut geometry, &boxing.bounds(), zoom, colors);
        } else if let Some(transform_box) = TransformBox::of(&self.scene, &self.selection) {
            overlay::transform_handles(
                &mut geometry,
                &transform_box,
                self.last_pointer,
                zoom,
                colors,
            );
        } else if let Some(handles) = self.point_handles(self.last_pointer) {
            let selected = self.points.indices();
            overlay::point_handles(
                &mut geometry,
                &handles,
                selected,
                self.last_pointer,
                zoom,
                colors,
            );
        }
        if let Some(outline) = self
            .highlight
            .as_deref()
            .and_then(|id| binding::highlight(&self.scene, id))
        {
            overlay::binding_highlight(&mut geometry, &outline, zoom, colors);
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

    /// The client reports the space bar down (`true`) or up. A drag that
    /// begins while it is held pans whatever the Tool and pointer, see
    /// [`crate::navigation`]. Clients reset it when the window or app loses
    /// focus, and do not send it while the text view has focus.
    pub fn set_pan_key(&mut self, held: bool) {
        self.navigation.set_pan_key(held);
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

    /// Records that the Scene changed. [`Geometry::generation`] stays: handles
    /// are stable, so no cached `(handle, version)` can go stale.
    fn changed(&mut self) {
        self.revision += 1;
        // Only selected Elements have anything drawn over them.
        if !self.selection.is_empty() {
            self.overlay_revision += 1;
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
