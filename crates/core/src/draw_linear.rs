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
use crate::json::{Object, Value};
use crate::path::Point;
use crate::random::Entropy;
use crate::scene::{ElementType, Scene};
use crate::style::Style;
use crate::tap::slop;
use crate::transform::SHIFT_LOCKING_ANGLE;

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
        PROPORTIONAL_RADIUS,
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
