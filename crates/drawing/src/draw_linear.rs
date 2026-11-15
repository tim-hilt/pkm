//! Drawing a line or arrow with the line and arrow Tools (ADR-0006), as
//! Excalidraw does.
//!
//! The Element is added to the Scene when the pointer goes down, with a single
//! point, and what the pointer does next decides what it becomes:
//!
//! * Dragging stretches it to a line of two points, which is finished when the
//!   pointer lifts. Shift snaps its angle to multiples of 15 degrees.
//! * A tap leaves the first point and lets the user place the others one by
//!   one: every further press adds a point where it lifts (a pointer that
//!   hovers, like a mouse, shows the next point following it), until the line
//!   is finished by pressing on the last point again, by closing the loop of a
//!   line on its first point, or by whatever the editor does to end it (the
//!   Tool changes). A touch screen has no hovering, so for it a point is
//!   exactly where a finger tapped.
//!
//! What counts as "on" a point is a zone around it, in view points so that it
//! feels the same at any zoom, and wider for fingers.
//!
//! The ends of an arrow bind to the shapes they are put on (see
//! [`crate::binding`]): the first where the pointer goes down, the last
//! wherever the line currently ends. The shape the last point is on is the one
//! to highlight.

use crate::binding::{self, End};
use crate::draw_shape::{Outcome, PROPORTIONAL_RADIUS, new_element};
use crate::edit_points;
use crate::history::Step;
use crate::input::{PointerEvent, PointerKind, PointerPhase};
use crate::path::Point;
use crate::scene::{ElementType, Scene};
use crate::style::Style;
use crate::tap::slop;
use crate::transform::SHIFT_LOCKING_ANGLE;
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;

/// Excalidraw's `LINE_CONFIRM_THRESHOLD`: how near a mouse or Pencil must be
/// to a point to be on it, in view points.
const PRECISE_ZONE: f64 = 8.0;
/// The same for a finger: half of the 44 points Apple recommends for touch
/// targets.
const FINGER_ZONE: f64 = 22.0;

/// How near in view points a pointer of `kind` must be to a point to be on it.
fn zone(kind: PointerKind) -> f64 {
    match kind {
        PointerKind::Finger => FINGER_ZONE,
        PointerKind::Pencil | PointerKind::Mouse => PRECISE_ZONE,
    }
}

/// Where the line is in being drawn.
#[derive(Debug)]
enum Stage {
    /// The first pointer is down and has not yet moved away (a tap) or has
    /// (a drag).
    First { pointer: u64, dragged: bool },
    /// The first point is placed; the others are placed by presses. `press`
    /// is the pointer that is down, if one is.
    Placing { press: Option<u64> },
}

/// What a step of a pointer did to the line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Update {
    /// Nothing the Scene shows changed.
    Unchanged,
    Changed,
    /// The line is complete; the editor finishes it.
    Finished,
    /// The line is abandoned; the editor takes it back.
    Abandoned,
}

/// A line or arrow being drawn.
#[derive(Debug)]
pub(crate) struct LinearDrawing {
    handle: u32,
    id: String,
    kind: ElementType,
    /// The zoom of the view at the last step, which decides how far from a
    /// shape an arrow still binds to it.
    zoom: f64,
    /// The shape an end of the arrow is put on, if any.
    highlight: Option<String>,
    /// Where the last placed point of an arrow is shown, if that is not
    /// where it was placed: an end bound to a shape stops short of it.
    placed_end: Option<Point>,
    /// The kind of the pointer that went down last, which decides how
    /// precise the user is.
    pointer_kind: PointerKind,
    /// The points placed so far, in Scene coordinates.
    points: Vec<Point>,
    /// The point that follows the pointer and is not placed yet.
    preview: Option<Point>,
    stage: Stage,
    step: Step,
}

impl LinearDrawing {
    /// Starts drawing a `kind` line with `pointer` going down at `origin` in a
    /// view at `zoom`, as the action of `step`.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        mut step: Step,
        scene: &mut Scene,
        entropy: &mut Entropy,
        style: &Style,
        kind: ElementType,
        pointer: u64,
        pointer_kind: PointerKind,
        origin: Point,
        zoom: f64,
    ) -> Self {
        let element = new_linear(kind, entropy, style, origin);
        let id = element
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        step.create(&id);
        let handle = scene.push(element);
        let mut highlight = None;
        if kind == ElementType::Arrow {
            highlight = binding::bind_end(
                scene,
                entropy,
                &mut step,
                &id,
                End::Start,
                origin,
                binding::reach(pointer_kind, zoom),
            );
        }
        Self {
            handle,
            id,
            zoom,
            highlight,
            placed_end: None,
            kind,
            pointer_kind,
            points: vec![origin],
            preview: None,
            stage: Stage::First {
                pointer,
                dragged: false,
            },
            step,
        }
    }

    /// The handle of the line being drawn.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// The id of the line being drawn.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The shape an end of the arrow is on, which is highlighted.
    pub fn highlight(&self) -> Option<&str> {
        self.highlight.as_deref()
    }

    /// Whether the first press is over and the points are being placed one
    /// by one, which goes on between presses.
    pub fn is_placing(&self) -> bool {
        matches!(self.stage, Stage::Placing { .. })
    }

    /// A second finger landed: whatever the pointer that is down was going to
    /// do is not done. A line that is being placed keeps its points and
    /// forgets the one that was about to be added.
    pub fn interrupt(&mut self, scene: &mut Scene, entropy: &mut Entropy) -> Update {
        match &mut self.stage {
            Stage::First { .. } => Update::Abandoned,
            Stage::Placing { press } => {
                let had = press.take().is_some() || self.preview.is_some();
                self.preview = None;
                if had {
                    self.write(scene, entropy);
                }
                Update::Unchanged
            }
        }
    }

    /// Follows `event`, whose position in the Scene is `at`, in a view at
    /// `zoom`.
    pub fn follow(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        event: PointerEvent,
        at: Point,
        zoom: f64,
    ) -> Update {
        if event.phase == PointerPhase::Down {
            self.pointer_kind = event.kind;
        }
        self.zoom = zoom;
        let update = match self.stage {
            Stage::First { pointer, dragged } => {
                if event.id != pointer {
                    return Update::Unchanged;
                }
                self.follow_first(event, at, zoom, dragged)
            }
            Stage::Placing { press } => self.follow_placing(event, at, zoom, press),
        };
        if update == Update::Changed {
            self.write(scene, entropy);
        }
        update
    }

    /// A pointer hovers at `at` (in the Scene) with shift held or not, in a
    /// view at `zoom`: while points are placed and none is pressed, the next
    /// one follows it.
    pub fn hover(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        zoom: f64,
        shift: bool,
    ) -> Update {
        if !matches!(self.stage, Stage::Placing { press: None }) {
            return Update::Unchanged;
        }
        self.zoom = zoom;
        let update = self.aim(at, zoom, shift);
        if update == Update::Changed {
            self.write(scene, entropy);
        }
        update
    }

    /// The first pointer: a tap or a drag.
    fn follow_first(&mut self, event: PointerEvent, at: Point, zoom: f64, dragged: bool) -> Update {
        let origin = self.points[0];
        match event.phase {
            PointerPhase::Down => Update::Unchanged,
            PointerPhase::Move => {
                let away = distance(at, origin) * zoom > slop(self.pointer_kind);
                if !(dragged || away) {
                    return Update::Unchanged;
                }
                if let Stage::First { dragged, .. } = &mut self.stage {
                    *dragged = true;
                }
                self.preview = Some(aimed(origin, at, event.modifiers.shift));
                Update::Changed
            }
            PointerPhase::Up if dragged => {
                if let Some(end) = self.preview.take() {
                    self.points.push(end);
                }
                Update::Finished
            }
            PointerPhase::Up => {
                self.stage = Stage::Placing { press: None };
                Update::Unchanged
            }
            PointerPhase::Cancel => Update::Abandoned,
        }
    }

    /// Placing points: presses add them, pointers that hover show the next.
    fn follow_placing(
        &mut self,
        event: PointerEvent,
        at: Point,
        zoom: f64,
        press: Option<u64>,
    ) -> Update {
        match event.phase {
            PointerPhase::Down if press.is_some() => Update::Unchanged,
            PointerPhase::Down => {
                let reach = zone(event.kind) / zoom;
                let (first, last) = (self.points[0], self.last_point());
                let on_first = self.kind == ElementType::Line && distance(at, first) <= reach;
                if self.points.len() >= 2 && (on_first || distance(at, last) < reach) {
                    // Pressing the last point finishes the line; pressing
                    // the first closes the loop of a line, which the finish
                    // makes exact.
                    if on_first {
                        self.points.push(at);
                    }
                    self.preview = None;
                    return Update::Finished;
                }
                self.stage = Stage::Placing {
                    press: Some(event.id),
                };
                self.aim(at, zoom, event.modifiers.shift)
            }
            PointerPhase::Move if press.is_none() || press == Some(event.id) => {
                self.aim(at, zoom, event.modifiers.shift)
            }
            PointerPhase::Up if press == Some(event.id) => {
                self.stage = Stage::Placing { press: None };
                if let Some(point) = self.preview.take() {
                    self.points.push(point);
                }
                Update::Changed
            }
            PointerPhase::Cancel if press == Some(event.id) => {
                self.stage = Stage::Placing { press: None };
                self.preview = None;
                Update::Changed
            }
            _ => Update::Unchanged,
        }
    }

    /// Where the last placed point is shown, which is where the user finds it.
    fn last_point(&self) -> Point {
        self.placed_end
            .unwrap_or(self.points[self.points.len() - 1])
    }

    /// Lets the next point follow the pointer at `at`, once it is outside the
    /// zone of the last one.
    fn aim(&mut self, at: Point, zoom: f64, shift: bool) -> Update {
        let last = self.last_point();
        let target = aimed(last, at, shift);
        let next = (distance(target, last) * zoom >= zone(self.pointer_kind)).then_some(target);
        if next == self.preview {
            return Update::Unchanged;
        }
        self.preview = next;
        Update::Changed
    }

    /// Ends the drawing with the line as it is. A line that closes its loop
    /// becomes a polygon with its last point exactly on its first, so that it
    /// stays closed at any scale. A line of fewer than two points is nothing.
    /// `zoom` is that of the view.
    pub fn finish(mut self, scene: &mut Scene, entropy: &mut Entropy, zoom: f64) -> Outcome {
        self.preview = None;
        if self.points.len() < 2 {
            self.cancel(scene, entropy);
            return Outcome::Dropped;
        }
        let closes = self.kind == ElementType::Line
            && self.points.len() >= 3
            && distance(self.points[0], self.points[self.points.len() - 1])
                <= zone(self.pointer_kind) / zoom;
        if closes {
            let first = self.points[0];
            if let Some(last) = self.points.last_mut() {
                *last = first;
            }
        }
        self.write(scene, entropy);
        if closes {
            scene.update(
                self.handle,
                entropy.integer(),
                [("polygon", Value::Bool(true))],
            );
        }
        Outcome::Drawn(self.step)
    }

    /// Ends the drawing without a line. The shapes it was bound to are not
    /// bound to it any more.
    pub fn cancel(mut self, scene: &mut Scene, entropy: &mut Entropy) {
        binding::unbind_all(scene, entropy, &mut self.step, &self.id);
        scene.remove(self.handle);
    }

    /// Makes the Element have the points placed, and the one following the
    /// pointer. The first is the Element's position, and the others are
    /// relative to it.
    fn write(&mut self, scene: &mut Scene, entropy: &mut Entropy) {
        let all: Vec<Point> = self.points.iter().chain(&self.preview).copied().collect();
        let origin = all[0];
        let relative: Vec<Point> = all
            .iter()
            .map(|&(x, y)| (x - origin.0, y - origin.1))
            .collect();
        scene.set_points(self.handle, origin, &relative, entropy.integer());
        if self.kind == ElementType::Arrow && all.len() >= 2 {
            self.bind_last(scene, entropy, all[all.len() - 1]);
        }
    }

    /// Binds the end of the arrow, which is at `at`, to the shape that is
    /// there, or unbinds it, and then puts the bound ends where they belong.
    /// The first end stays bound as it was when the pointer went down.
    fn bind_last(&mut self, scene: &mut Scene, entropy: &mut Entropy, at: Point) {
        let reach = binding::reach(self.pointer_kind, self.zoom);
        self.highlight = binding::bind_end(
            scene,
            entropy,
            &mut self.step,
            &self.id,
            End::End,
            at,
            reach,
        );
        binding::follow(scene, entropy, &self.id);
        // The end of the arrow is the last placed point unless a point is
        // still following the pointer.
        if self.preview.is_none() {
            self.placed_end = scene
                .element_by_id(&self.id)
                .and_then(|arrow| edit_points::shown_points(&arrow).pop());
        }
    }
}

fn distance(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// `to`, or with `snap` the point at the same distance from `from` whose
/// direction is the nearest multiple of 15 degrees (Excalidraw's
/// `getLockedLinearCursorAlignSize`).
fn aimed(from: Point, to: Point, snap: bool) -> Point {
    if !snap {
        return to;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let angle = dy.atan2(dx);
    let locked = (angle / SHIFT_LOCKING_ANGLE).round() * SHIFT_LOCKING_ANGLE;
    let length = dx.hypot(dy);
    // What should be exactly on an axis is not off by rounding.
    let clean = |n: f64| if n.abs() < 1e-9 { 0.0 } else { n };
    (
        from.0 + clean(length * locked.cos()),
        from.1 + clean(length * locked.sin()),
    )
}

/// A line or arrow of one point at `origin`: the members of one Excalidraw
/// creates (`newLinearElement`, `newArrowElement`). The arrowheads are those
/// of its defaults; the properties panel will change them.
fn new_linear(kind: ElementType, entropy: &mut Entropy, style: &Style, origin: Point) -> Object {
    let is_arrow = kind == ElementType::Arrow;
    let mut element = new_element(
        if is_arrow { "arrow" } else { "line" },
        Some(PROPORTIONAL_RADIUS),
        entropy,
        style,
        origin,
    );
    let origin_point = Value::Array(vec![Value::Number(0.0), Value::Number(0.0)]);
    element.insert("points".into(), Value::Array(vec![origin_point]));
    element.insert("startBinding".into(), Value::Null);
    element.insert("endBinding".into(), Value::Null);
    element.insert("startArrowhead".into(), Value::Null);
    element.insert(
        "endArrowhead".into(),
        if is_arrow {
            Value::String("arrow".into())
        } else {
            Value::Null
        },
    );
    if is_arrow {
        element.insert("elbowed".into(), Value::Bool(false));
    } else {
        element.insert("polygon".into(), Value::Bool(false));
    }
    element
}

#[cfg(test)]
mod tests {
    //! Drawing lines and arrows (ADR-0006), tested at the Editor seam with
    //! recorded pointer sequences (see `select.rs` for their format): pointer
    //! events in, saved JSON, Tool and Selection out. Expected coordinates are
    //! worked out by hand in the comments.
    //!
    //! Dragging draws a two-point line. A tap starts a multi-point one instead:
    //! every further tap adds a point, and tapping the last point again (or
    //! switching Tool, or for lines closing the loop) finishes it, as on
    //! excalidraw.com.

    use crate::input::Modifiers;
    use crate::{Editor, PointerKind, Tool};
    use pkm_base::json::Value;

    use crate::test_support as common;
    use common::{replay, saved_elements, saved_points, tap};

    fn editor_with(tool: Tool) -> Editor {
        let mut editor = Editor::new();
        editor.set_tool(tool);
        editor
    }

    /// A drag of `kind` from `from` to `to` in two steps, with `modifiers`
    /// (`"shift"` or empty) held while it moves.
    fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64), modifiers: &str) {
        replay(
            editor,
            &format!(
                "0.00 down 1 {kind} {} {}\n0.05 move 1 {kind} {} {} {modifiers}\n0.10 move 1 {kind} {} {} {modifiers}\n0.15 up 1 {kind} {} {} {modifiers}",
                from.0,
                from.1,
                (from.0 + to.0) / 2.0,
                (from.1 + to.1) / 2.0,
                to.0,
                to.1,
                to.0,
                to.1,
            ),
        );
    }

    fn text<'a>(element: &'a Value, key: &str) -> &'a str {
        element.get(key).and_then(Value::as_str).unwrap()
    }

    fn number(element: &Value, key: &str) -> f64 {
        element.get(key).and_then(Value::as_f64).unwrap()
    }

    // --- Dragging ----------------------------------------------------------------

    #[test]
    fn dragging_with_the_line_tool_draws_a_two_point_line() {
        for kind in ["mouse", "finger", "pencil"] {
            let mut editor = editor_with(Tool::Line);

            drag(&mut editor, kind, (10.0, 20.0), (110.0, 70.0), "");

            let elements = saved_elements(&editor);
            assert_eq!(elements.len(), 1, "{kind}");
            assert_eq!(text(&elements[0], "type"), "line");
            assert_eq!(
                (number(&elements[0], "x"), number(&elements[0], "y")),
                (10.0, 20.0)
            );
            assert_eq!(saved_points(&elements[0]), [(0.0, 0.0), (100.0, 50.0)]);
            assert_eq!(
                (
                    number(&elements[0], "width"),
                    number(&elements[0], "height")
                ),
                (100.0, 50.0)
            );
            assert_eq!(editor.element_count(), 1);
        }
    }

    #[test]
    fn dragging_with_the_arrow_tool_draws_an_arrow_with_an_arrowhead() {
        let mut editor = editor_with(Tool::Arrow);

        drag(&mut editor, "mouse", (0.0, 0.0), (80.0, 0.0), "");

        let arrow = &saved_elements(&editor)[0];
        assert_eq!(text(arrow, "type"), "arrow");
        assert_eq!(saved_points(arrow), [(0.0, 0.0), (80.0, 0.0)]);
        assert_eq!(arrow.get("startArrowhead"), Some(&Value::Null));
        assert_eq!(text(arrow, "endArrowhead"), "arrow");
        assert_eq!(arrow.get("elbowed"), Some(&Value::Bool(false)));
    }

    #[test]
    fn dragging_up_and_left_keeps_the_first_point_where_the_pointer_went_down() {
        let mut editor = editor_with(Tool::Line);

        drag(&mut editor, "mouse", (110.0, 70.0), (10.0, 20.0), "");

        let line = &saved_elements(&editor)[0];
        assert_eq!((number(line, "x"), number(line, "y")), (110.0, 70.0));
        assert_eq!(saved_points(line), [(0.0, 0.0), (-100.0, -50.0)]);
        // Sizes stay positive.
        assert_eq!(
            (number(line, "width"), number(line, "height")),
            (100.0, 50.0)
        );
    }

    #[test]
    fn shift_snaps_the_angle_of_a_dragged_line_to_fifteen_degrees() {
        let mut editor = editor_with(Tool::Line);

        // From (0, 0) to (100, 10) the angle is 5.7 degrees, which snaps to 0;
        // the length, hypot(100, 10) = 100.4988, is kept.
        drag(&mut editor, "mouse", (0.0, 0.0), (100.0, 10.0), "shift");

        let points = saved_points(&saved_elements(&editor)[0]);
        assert!((points[1].0 - 100.4988).abs() < 1e-3, "{points:?}");
        assert!(points[1].1.abs() < 1e-9, "{points:?}");
    }

    #[test]
    fn a_new_line_looks_like_one_excalidraw_creates() {
        let mut editor = editor_with(Tool::Line);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
        editor.set_tool(Tool::Arrow);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

        let elements = saved_elements(&editor);
        let keys = |e: &Value| -> Vec<String> {
            e.as_object()
                .unwrap()
                .iter()
                .map(|(k, _)| k.to_owned())
                .collect()
        };
        // `newLinearElement` and `newArrowElement`: the members of every Element,
        // then the points, bindings and arrowheads, and the one that tells the
        // two apart.
        let common = [
            "id",
            "type",
            "x",
            "y",
            "width",
            "height",
            "angle",
            "strokeColor",
            "backgroundColor",
            "fillStyle",
            "strokeWidth",
            "strokeStyle",
            "roughness",
            "opacity",
            "groupIds",
            "frameId",
            "index",
            "roundness",
            "seed",
            "version",
            "versionNonce",
            "isDeleted",
            "boundElements",
            "updated",
            "created",
            "link",
            "locked",
            "points",
            "startBinding",
            "endBinding",
            "startArrowhead",
            "endArrowhead",
        ];
        assert_eq!(keys(&elements[0]), [&common[..], &["polygon"]].concat());
        assert_eq!(keys(&elements[1]), [&common[..], &["elbowed"]].concat());
        // Lines and arrows are rounded unless the user chooses sharp.
        let roundness = elements[0].get("roundness").unwrap();
        assert_eq!(roundness.get("type").and_then(Value::as_f64), Some(2.0));
        assert_eq!(elements[0].get("polygon"), Some(&Value::Bool(false)));
    }

    #[test]
    fn lines_get_their_own_seeds() {
        let mut editor = editor_with(Tool::Line);
        editor.set_tool_lock(true);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

        let elements = saved_elements(&editor);
        assert_ne!(number(&elements[0], "seed"), number(&elements[1], "seed"));
    }

    #[test]
    fn a_dragged_line_is_drawn_and_survives_saving_and_loading() {
        let mut editor = editor_with(Tool::Arrow);
        drag(&mut editor, "mouse", (0.0, 0.0), (80.0, 40.0), "");
        assert!(!editor.geometry().parts().is_empty());

        let mut again = Editor::new();
        again.load(&editor.save()).unwrap();

        assert_eq!(again.save(), editor.save());
        assert_eq!(again.element_count(), 1);
    }

    // --- The Tool afterwards -------------------------------------------------------

    #[test]
    fn the_tool_returns_to_selection_after_a_dragged_line_unless_locked() {
        let mut editor = editor_with(Tool::Line);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
        assert_eq!(editor.tool(), Tool::Selection);

        editor.set_tool(Tool::Arrow);
        editor.set_tool_lock(true);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
        drag(&mut editor, "mouse", (0.0, 60.0), (50.0, 90.0), "");
        assert_eq!(editor.tool(), Tool::Arrow);
        assert_eq!(saved_elements(&editor).len(), 3);
    }

    // --- Tapping: multi-point lines -------------------------------------------------

    #[test]
    fn tapping_places_the_points_and_tapping_the_last_one_again_finishes() {
        let mut editor = editor_with(Tool::Line);

        tap(&mut editor, "mouse", (10.0, 10.0), 0.0);
        tap(&mut editor, "mouse", (60.0, 10.0), 1.0);
        tap(&mut editor, "mouse", (60.0, 60.0), 2.0);
        assert_eq!(editor.tool(), Tool::Line, "still placing points");
        tap(&mut editor, "mouse", (60.0, 60.0), 3.0);

        let elements = saved_elements(&editor);
        assert_eq!(elements.len(), 1);
        assert_eq!(
            saved_points(&elements[0]),
            [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]
        );
        assert_eq!(editor.tool(), Tool::Selection);
        assert_eq!(editor.selection().len(), 1, "the new line is selected");
    }

    #[test]
    fn tapping_places_the_points_of_an_arrow_too() {
        let mut editor = editor_with(Tool::Arrow);

        tap(&mut editor, "finger", (0.0, 0.0), 0.0);
        tap(&mut editor, "finger", (100.0, 0.0), 1.0);
        tap(&mut editor, "finger", (100.0, 100.0), 2.0);
        tap(&mut editor, "finger", (100.0, 100.0), 3.0);

        let arrow = &saved_elements(&editor)[0];
        assert_eq!(text(arrow, "type"), "arrow");
        assert_eq!(
            saved_points(arrow),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]
        );
    }

    #[test]
    fn the_zone_in_which_tapping_the_last_point_finishes_is_larger_for_fingers() {
        // A mouse tap 15 away from the last point places a point there...
        let mut mouse = editor_with(Tool::Line);
        tap(&mut mouse, "mouse", (0.0, 0.0), 0.0);
        tap(&mut mouse, "mouse", (100.0, 0.0), 1.0);
        tap(&mut mouse, "mouse", (115.0, 0.0), 2.0);
        assert_eq!(mouse.tool(), Tool::Line);
        mouse.set_tool(Tool::Hand);
        assert_eq!(
            saved_points(&saved_elements(&mouse)[0]),
            [(0.0, 0.0), (100.0, 0.0), (115.0, 0.0)]
        );

        // ...a finger's, which is a lot less precise, finishes the line.
        let mut finger = editor_with(Tool::Line);
        tap(&mut finger, "finger", (0.0, 0.0), 0.0);
        tap(&mut finger, "finger", (100.0, 0.0), 1.0);
        tap(&mut finger, "finger", (115.0, 0.0), 2.0);
        assert_eq!(finger.tool(), Tool::Selection);
        assert_eq!(
            saved_points(&saved_elements(&finger)[0]),
            [(0.0, 0.0), (100.0, 0.0)]
        );
    }

    #[test]
    fn the_zone_follows_the_zoom_so_that_it_is_the_same_on_screen() {
        let mut editor = editor_with(Tool::Line);
        // Pinch to a zoom of 2: the finger zone is then 11 units in the Scene.
        replay(
            &mut editor,
            "0 down 1 finger 10 10\n0.1 down 2 finger 50 10\n0.2 move 2 finger 90 10\n0.3 up 2 finger 90 10\n0.4 up 1 finger 10 10",
        );
        let zoom = editor.viewport().zoom();
        assert!(zoom > 1.5, "{zoom}");

        tap(&mut editor, "finger", (0.0, 0.0), 1.0);
        tap(&mut editor, "finger", (100.0, 0.0), 2.0);
        // 30 view points from the last point, which is 15 units of the Scene:
        // outside the zone of 22 view points (11 units), though it would be
        // inside one of 22 units.
        tap(&mut editor, "finger", (100.0 + 30.0, 0.0), 3.0);
        assert_eq!(editor.tool(), Tool::Line);

        // 20 view points away (10 units) is inside.
        tap(&mut editor, "finger", (130.0 + 20.0, 0.0), 4.0);
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn closing_the_loop_of_a_line_finishes_it_as_a_polygon() {
        let mut editor = editor_with(Tool::Line);

        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        tap(&mut editor, "mouse", (100.0, 0.0), 1.0);
        tap(&mut editor, "mouse", (100.0, 100.0), 2.0);
        tap(&mut editor, "mouse", (3.0, 4.0), 3.0);

        let line = &saved_elements(&editor)[0];
        // The last point is put exactly onto the first.
        assert_eq!(
            saved_points(line),
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 0.0)]
        );
        assert_eq!(line.get("polygon"), Some(&Value::Bool(true)));
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn an_arrow_does_not_close_a_loop() {
        let mut editor = editor_with(Tool::Arrow);

        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        tap(&mut editor, "mouse", (100.0, 0.0), 1.0);
        tap(&mut editor, "mouse", (100.0, 100.0), 2.0);
        tap(&mut editor, "mouse", (3.0, 4.0), 3.0);

        assert_eq!(editor.tool(), Tool::Arrow, "still placing points");
    }

    #[test]
    fn switching_the_tool_finishes_the_line_without_the_point_being_placed() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        tap(&mut editor, "mouse", (50.0, 0.0), 1.0);
        // The mouse hovers: the next point follows it but is not placed yet.
        replay(&mut editor, "2 move 9 mouse 50 50");
        assert_eq!(
            saved_points(&saved_elements(&editor)[0]),
            [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]
        );

        editor.set_tool(Tool::Hand);

        assert_eq!(
            saved_points(&saved_elements(&editor)[0]),
            [(0.0, 0.0), (50.0, 0.0)]
        );
    }

    #[test]
    fn a_line_with_a_single_point_is_nothing() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);

        editor.set_tool(Tool::Selection);

        assert!(saved_elements(&editor).is_empty());
        assert_eq!(editor.element_count(), 0);
        assert!(!editor.can_undo());
    }

    #[test]
    fn hovering_makes_the_next_point_appear_only_beyond_the_zone_of_the_last() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);

        let none = Modifiers::default();
        editor.hover(5.0, 0.0, PointerKind::Mouse, none);
        assert_eq!(saved_points(&saved_elements(&editor)[0]).len(), 1);

        editor.hover(30.0, 0.0, PointerKind::Mouse, none);
        assert_eq!(
            saved_points(&saved_elements(&editor)[0]),
            [(0.0, 0.0), (30.0, 0.0)]
        );

        editor.hover(2.0, 0.0, PointerKind::Mouse, none);
        assert_eq!(saved_points(&saved_elements(&editor)[0]).len(), 1);
    }

    #[test]
    fn hovering_leaves_a_point_being_pressed_alone() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        replay(&mut editor, "1 down 3 pencil 50 0");

        editor.hover(100.0, 100.0, PointerKind::Mouse, Modifiers::default());

        assert_eq!(
            saved_points(&saved_elements(&editor)[0]),
            [(0.0, 0.0), (50.0, 0.0)]
        );
    }

    #[test]
    fn shift_snaps_the_hovering_next_point() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);

        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        editor.hover(100.0, 10.0, PointerKind::Mouse, shift);

        let points = saved_points(&saved_elements(&editor)[0]);
        assert_eq!(points[1].1, 0.0, "{points:?}");
    }

    #[test]
    fn shift_snaps_the_point_being_placed() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        tap(&mut editor, "mouse", (100.0, 0.0), 1.0);

        // From (100, 0) towards (200, 10): 5.7 degrees, snapped to 0.
        replay(
            &mut editor,
            "2 down 1 mouse 200 10 shift\n2.1 up 1 mouse 200 10 shift",
        );
        editor.set_tool(Tool::Hand);

        let points = saved_points(&saved_elements(&editor)[0]);
        assert_eq!(points.len(), 3);
        // 100 along, then hypot(100, 10) = 100.4988 more.
        assert!((points[2].0 - 200.4988).abs() < 1e-3, "{points:?}");
        assert_eq!(points[2].1, 0.0);
    }

    // --- Interruptions ------------------------------------------------------------------

    #[test]
    fn a_second_finger_while_dragging_takes_the_line_back() {
        let mut editor = editor_with(Tool::Line);

        replay(
            &mut editor,
            "0 down 1 finger 10 10\n0.1 move 1 finger 80 10\n0.2 down 2 finger 200 200",
        );

        assert_eq!(editor.element_count(), 0);
        assert!(saved_elements(&editor).is_empty());
        assert_eq!(editor.tool(), Tool::Line);
    }

    #[test]
    fn cancelling_the_pointer_while_dragging_takes_the_line_back() {
        let mut editor = editor_with(Tool::Arrow);

        replay(
            &mut editor,
            "0 down 1 pencil 10 10\n0.1 move 1 pencil 80 10\n0.2 cancel 1 pencil 80 10",
        );

        assert!(saved_elements(&editor).is_empty());
        assert!(!editor.can_undo());
    }

    #[test]
    fn two_finger_gestures_between_taps_place_no_points() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "finger", (0.0, 0.0), 0.0);

        // Two fingers land and lift again without going anywhere.
        replay(
            &mut editor,
            "1 down 1 finger 100 100\n1.1 down 2 finger 200 100\n1.2 up 2 finger 200 100\n1.3 up 1 finger 100 100",
        );
        assert_eq!(editor.tool(), Tool::Line);
        tap(&mut editor, "finger", (50.0, 0.0), 2.0);
        editor.set_tool(Tool::Hand);

        let line = &saved_elements(&editor)[0];
        assert_eq!(number(line, "x"), 0.0);
        assert_eq!(saved_points(line), [(0.0, 0.0), (50.0, 0.0)]);
    }

    // --- History -----------------------------------------------------------------------

    #[test]
    fn a_drawn_line_is_one_undo_step() {
        let mut editor = editor_with(Tool::Line);
        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");
        assert!(editor.can_undo());

        editor.undo();
        assert_eq!(editor.element_count(), 0);
        editor.redo();
        assert_eq!(editor.element_count(), 1);
    }

    #[test]
    fn a_multi_point_line_is_one_undo_step_too() {
        let mut editor = editor_with(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
        tap(&mut editor, "mouse", (50.0, 0.0), 1.0);
        tap(&mut editor, "mouse", (50.0, 50.0), 2.0);
        tap(&mut editor, "mouse", (50.0, 50.0), 3.0);

        editor.undo();

        assert_eq!(editor.element_count(), 0);
        assert!(!editor.can_undo());
    }

    #[test]
    fn undoing_while_placing_points_finishes_the_line_first() {
        let mut editor = editor_with(Tool::Line);
        drag(&mut editor, "mouse", (0.0, 100.0), (50.0, 100.0), "");
        editor.set_tool(Tool::Line);
        tap(&mut editor, "mouse", (0.0, 0.0), 1.0);
        tap(&mut editor, "mouse", (50.0, 0.0), 2.0);

        editor.undo();

        // The line being placed was finished, which made it the latest step, and
        // is now undone; the first one stays.
        assert_eq!(editor.element_count(), 1);
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn a_new_line_or_arrow_is_selected_to_be_adjusted() {
        for tool in [Tool::Line, Tool::Arrow] {
            let mut editor = editor_with(tool);

            drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

            let id = text(&saved_elements(&editor)[0], "id").to_owned();
            assert_eq!(editor.selection(), [id], "{tool:?}");
        }
    }

    #[test]
    fn with_the_tool_lock_a_new_arrow_is_not_selected() {
        let mut editor = editor_with(Tool::Arrow);
        editor.set_tool_lock(true);

        drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

        assert!(editor.selection().is_empty());
    }

    #[test]
    fn a_double_tap_beside_a_selected_arrow_makes_a_text() {
        for kind in ["mouse", "finger"] {
            let mut editor = editor_with(Tool::Arrow);
            editor.set_text_measurer(std::sync::Arc::new(common::FixedWidth));
            drag(&mut editor, "mouse", (0.0, 0.0), (50.0, 50.0), "");

            tap(&mut editor, kind, (300.0, 300.0), 10.0);
            tap(&mut editor, kind, (300.0, 300.0), 10.1);

            assert!(editor.text_editor().is_some(), "{kind}");
        }
    }
}
