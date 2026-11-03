use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::action_bar::ActionBar;
use crate::binding::{self, End};
use crate::box_select::BoxSelect;
use crate::delete;
use crate::draw_freedraw::StrokeDrawing;
use crate::draw_linear::{LinearDrawing, Update};
use crate::draw_shape::{Outcome, ShapeGesture};
use crate::duplicate;
use crate::edit_points::{self, Grab, PointDrag, PointHandles, PointSelection};
use crate::erase::Erasing;
use crate::geometry::Geometry;
use crate::history::{self, History, Step};
use crate::hit_test::{self, Reach};
use crate::input::{MagnifyEvent, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
use crate::move_selection::MoveGesture;
use crate::navigation::{Bounds, Navigation};
use crate::overlay;
use crate::properties::{self, Properties, Property};
use crate::random::Entropy;
use crate::scene::{Element, ElementType, LoadError, Scene};
use crate::selection::Selection;
use crate::style::Style;
use crate::tap::{Origin, Recognized, Tap, TapRecognizer};
use crate::text::{self, Containers, TextMeasurer};
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
    /// The line or arrow being drawn, from its first press until it is
    /// finished: with tapping that is longer than a pointer is down.
    linear: Option<LinearDrawing>,
    /// The freehand stroke being drawn, if the pointer of the freedraw Tool is
    /// down.
    stroke: Option<StrokeDrawing>,
    /// The erase in progress, if the pointer of the eraser Tool is down.
    erasing: Option<Erasing>,
    selection: Selection,
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
            linear: None,
            stroke: None,
            erasing: None,
            selection: Selection::default(),
            taps: TapRecognizer::default(),
            history: History::default(),
            moving: None,
            transforming: None,
            point_dragging: None,
            points: PointSelection::default(),
            box_selecting: None,
            last_pointer: PointerKind::Finger,
            overlay_revision: 0,
            overlay_zoom: 1.0,
            highlight: None,
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
        let mut geometry = Geometry::with_generation(self.generation);
        let background = self.scene.view_background_color();
        let containers = Containers::new(&self.scene);
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
        for element in self.shown() {
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
            self.navigation.cancel_drag();
            self.taps.cancel();
            self.drop_shape();
            self.drop_stroke();
            self.drop_erase();
            self.end_linear();
            self.drop_move();
            self.drop_point_drag();
            self.drop_box_select();
            self.tool = tool;
            self.sync_highlight();
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
    /// the shape (see [`crate::draw_shape`]), and with the line or arrow Tool
    /// it draws a line, by dragging or by tapping its points one by one (see
    /// [`crate::draw_linear`]), and with the freedraw Tool it draws a stroke
    /// through the points the pointer passes (see [`crate::draw_freedraw`]),
    /// and with the eraser Tool it previews the Elements the pointer passes
    /// and deletes them when it lifts (see [`crate::erase`]). With the selection Tool a tap selects the
    /// Element it hits or clears the Selection (see [`Editor::selection`])
    /// while a drag on an Element moves the Selection (see
    /// [`crate::move_selection`]), a drag on a handle of it resizes, rotates
    /// or, for a line or arrow, edits its points (see [`crate::edit_points`]),
    /// and a drag on empty Canvas box-selects (see [`crate::box_select`]).
    /// Shift adds to the Selection, or takes from it, instead of replacing it.
    /// Arrows bind to the shapes their ends are put on and follow them when
    /// they move or change (see [`crate::binding`]).
    pub fn pointer(&mut self, event: PointerEvent) {
        self.last_pointer = event.kind;
        let tool = self.tool;
        self.navigation.pointer(event, self.tool == Tool::Hand);
        self.navigated();
        self.draw_shape(event);
        self.draw_linear(event);
        self.draw_stroke(event);
        self.erase(event);
        // An event that finished a drawing, and so went back to selection, was
        // the drawing's: it is not a tap that selects.
        if tool == Tool::Selection || self.tool != Tool::Selection {
            self.select(event);
        }
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
        // The points may have been added or taken away: no index is right.
        self.points.clear();
        self.overlay_revision += 1;
        self.changed(true);
    }

    /// What the properties panel shows: the values of the Selection's
    /// Elements (`None` where they differ), or, with nothing selected, the
    /// style of the next Element. See [`crate::properties`].
    pub fn properties(&self) -> Properties {
        if self.selection.is_empty() {
            return Properties::of_style(&self.style);
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
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        self.style.set(&property);
        let mut step = Step::begin(&self.selection);
        let changed = properties::apply(
            &mut self.scene,
            &mut self.entropy,
            self.selection.ids(),
            &property,
            |scene, id| step.touch(scene, id),
        );
        if changed {
            self.history.record(step, &self.scene, &self.selection);
            self.changed(false);
        }
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
        ActionBar::of(&self.scene, &self.selection, self.navigation.viewport())
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
        );
        if copies.is_empty() {
            return;
        }
        self.selection = copies;
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

    /// Selects every shown, unlocked Element of the Scene (the `select all`
    /// command).
    pub fn select_all(&mut self) {
        let ids: Vec<String> = self
            .shown()
            .filter(|e| !e.is_locked())
            .map(|e| e.id().to_owned())
            .collect();
        if self.selection.select(ids) {
            self.overlay_revision += 1;
        }
        self.sync_points();
    }

    /// Selects the Element a tap hits, or clears the Selection if the tap
    /// was a deliberate one on empty Canvas (see [`crate::tap`]); moves the
    /// Selection with a drag that began on an Element, and box-selects with
    /// one that began on empty Canvas.
    ///
    /// Only the selection Tool taps and drags, and never with two fingers
    /// down: they navigate, and must not disturb the Selection, nor leave a
    /// move half done.
    fn select(&mut self, event: PointerEvent) {
        if self.tool != Tool::Selection || self.navigation.is_pinching() {
            self.taps.cancel();
            self.drop_move();
            self.drop_transform();
            self.drop_point_drag();
            self.drop_box_select();
            return;
        }
        self.follow_move(event);
        self.follow_transform(event);
        self.follow_point_drag(event);
        self.follow_box_select(event);
        match self.taps.track(event) {
            Some(Recognized::Tap(tap)) => self.tap(tap, event.modifiers.shift),
            Some(Recognized::DragBegan(origin)) => self.begin_drag(origin, event),
            None => {}
        }
    }

    /// With `shift`, a tap on an Element adds it to the Selection or, if it
    /// is in it, takes it out, and a tap on empty Canvas leaves the Selection
    /// alone.
    fn tap(&mut self, tap: Tap, shift: bool) {
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
            Some(id) if shift => self.selection.toggle(&id),
            Some(id) => {
                self.selection.select_only(&id);
            }
            None if tap.is_short() && !shift => {
                self.selection.clear();
            }
            None => {}
        }
        if self.selection != before {
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
    /// that moves.
    fn begin_point_drag(&mut self, origin: Origin, event: PointerEvent) {
        let Some((handles, grab)) = self.grabbed_point(origin.x, origin.y, origin.kind) else {
            return;
        };
        let id = handles.id().to_owned();
        match grab {
            Grab::Point(index) if self.points.contains(index) => {}
            Grab::Point(index) if event.modifiers.shift => self.points.add(&id, index),
            Grab::Point(index) | Grab::Middle(index) => self.points.select_only(&id, index),
        }
        self.overlay_revision += 1;
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        let moving = self.points.indices().to_vec();
        self.point_dragging = Some(handles.drag(
            Step::begin(&self.selection),
            &self.scene,
            event.id,
            start,
            moving,
            grab,
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

    /// Puts the points being dragged, if any, back where they were.
    fn drop_point_drag(&mut self) {
        if let Some(dragging) = self.point_dragging.take() {
            dragging.cancel(&mut self.scene, &mut self.entropy);
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
                    event.modifiers.shift,
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
    /// a transform handle it resizes or rotates the Selection, on an Element
    /// it moves, and on empty Canvas it box-selects. `event` is the step that
    /// turned the press into a drag.
    fn begin_drag(&mut self, origin: Origin, event: PointerEvent) {
        self.begin_transform(origin, event);
        if self.transforming.is_some() {
            return;
        }
        self.begin_point_drag(origin, event);
        if self.point_dragging.is_some() {
            return;
        }
        match self.hit(origin.x, origin.y, origin.kind) {
            Some(id) => self.begin_move(&id, origin, event),
            None => self.begin_box_select(origin, event),
        }
    }

    /// Starts moving the Selection by a drag that began on the Element `id`:
    /// on the Selection itself, or else on an Element that becomes the
    /// Selection, or with shift joins it.
    fn begin_move(&mut self, id: &str, origin: Origin, event: PointerEvent) {
        // Undoing the move also undoes the selecting it began with.
        let step = Step::begin(&self.selection);
        let changed = if self.selection.contains(id) {
            false
        } else if event.modifiers.shift {
            self.selection.add(id)
        } else {
            self.selection.select_only(id)
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

    /// Puts the Selection being moved, if any, back where it was, and selects
    /// again what was selected before the drag began.
    fn drop_move(&mut self) {
        if let Some(moving) = self.moving.take() {
            let selection = moving.cancel(&mut self.scene, &mut self.entropy);
            self.show_selection(selection);
            self.changed(false);
        }
    }

    /// Starts a box at where the drag began on empty Canvas; with shift it
    /// adds to the Selection instead of replacing it.
    fn begin_box_select(&mut self, origin: Origin, event: PointerEvent) {
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        self.box_selecting = Some(BoxSelect::begin(
            event.id,
            start,
            &self.selection,
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
            self.tool = Tool::Selection;
        }
    }

    /// Finishes the line being drawn if it is waiting for further points.
    fn finish_placing(&mut self) {
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
