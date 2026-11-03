//! Drawing a freehand stroke with the freedraw Tool (ADR-0004, ADR-0006), as
//! Excalidraw does.
//!
//! The stroke is added to the Scene when the pointer goes down, with a single
//! point, and grows by a point wherever the pointer goes, so that it is drawn
//! while it is being drawn. The point where the pointer lifts is the last.
//!
//! A Pencil reports how hard it presses, and the stroke keeps that pressure
//! for every point. Fingers and the mouse do not draw with pressure that
//! means anything (or none at all), so their strokes make it up from the
//! speed of the pointer instead; see [`crate::freehand`].

use crate::draw_shape::new_element;
use crate::history::Step;
use crate::input::{PointerEvent, PointerKind};
use crate::json::{Object, Value};
use crate::path::Point;
use crate::random::Entropy;
use crate::scene::Scene;
use crate::style::Style;

/// Excalidraw's `DEFAULT_STROKE_STREAMLINE`: how much a mouse stroke is
/// smoothed.
const MOUSE_STREAMLINE: f64 = 0.5;
/// Excalidraw's `DEFAULT_STROKE_STREAMLINE_PRECISE`: what the others are,
/// whose movements are less jittery.
const PRECISE_STREAMLINE: f64 = 0.2;

/// A stroke of fewer points than this, that a second finger interrupts, is a
/// spike of the first finger landing rather than something drawn; Excalidraw
/// discards it.
const SPIKE_POINTS: usize = 10;

/// A freehand stroke being drawn by one pointer.
#[derive(Debug)]
pub(crate) struct StrokeDrawing {
    pointer: u64,
    handle: u32,
    /// Where the pointer went down, in Scene coordinates: the position of the
    /// Element, which the points are relative to.
    origin: Point,
    /// Whether the stroke has the pressure the pointer reports; otherwise it
    /// is made up.
    measured: bool,
    /// How many points the stroke has.
    points: usize,
    /// The last point, relative to the origin.
    last: Point,
    /// The corners of the box around the points, relative to the origin.
    min: Point,
    max: Point,
    step: Step,
}

impl StrokeDrawing {
    /// Starts a stroke with `event`, the pointer going down at `origin` in
    /// the Scene, as the action of `step`.
    pub fn begin(
        mut step: Step,
        scene: &mut Scene,
        entropy: &mut Entropy,
        style: &Style,
        event: PointerEvent,
        origin: Point,
    ) -> Self {
        let measured = event.kind == PointerKind::Pencil;
        let mut stroke = new_element("freedraw", None, entropy, style, origin);
        // Excalidraw draws these half as wide as the other Elements for the
        // same setting.
        stroke.insert(
            "strokeWidth".into(),
            Value::Number(style.stroke_width / 2.0),
        );
        add_members(&mut stroke, event, measured);
        if let Some(id) = stroke.get("id").and_then(Value::as_str) {
            step.create(id);
        }
        let handle = scene.push(stroke);
        Self {
            pointer: event.id,
            handle,
            origin,
            measured,
            points: 1,
            last: (0.0, 0.0),
            min: (0.0, 0.0),
            max: (0.0, 0.0),
            step,
        }
    }

    /// The pointer that draws this stroke.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Whether the stroke is so short that it is more likely the mark of a
    /// finger landing than something drawn.
    pub fn is_spike(&self) -> bool {
        self.points < SPIKE_POINTS
    }

    /// Takes the stroke on to where the pointer of `event` is, at `at` in the
    /// Scene. Returns whether the stroke changed: a pointer that has not moved
    /// adds nothing.
    pub fn extend(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        event: PointerEvent,
        at: Point,
    ) -> bool {
        let point = self.relative(at);
        if point == self.last {
            return false;
        }
        self.add(scene, entropy, event, point);
        true
    }

    /// Ends the stroke with the pointer of `event` lifting at `at` in the
    /// Scene; that is the last point of it even if the pointer did not move.
    /// The stroke stays in the Scene, and the step is what undoes it.
    pub fn lift(
        mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        event: PointerEvent,
        at: Point,
    ) -> Step {
        let mut point = self.relative(at);
        // Excalidraw's way to keep a dot from being "invisibly small": the
        // end of a stroke that never left its start is a hair away.
        if point == (0.0, 0.0) {
            point = (DOT, DOT);
        }
        self.add(scene, entropy, event, point);
        self.step
    }

    /// Ends the stroke with the points it has.
    pub fn finish(self) -> Step {
        self.step
    }

    /// Ends the stroke without anything drawn.
    pub fn cancel(self, scene: &mut Scene) {
        scene.remove(self.handle);
    }

    fn relative(&self, at: Point) -> Point {
        (at.0 - self.origin.0, at.1 - self.origin.1)
    }

    fn add(&mut self, scene: &mut Scene, entropy: &mut Entropy, event: PointerEvent, point: Point) {
        self.points += 1;
        self.last = point;
        self.min = (self.min.0.min(point.0), self.min.1.min(point.1));
        self.max = (self.max.0.max(point.0), self.max.1.max(point.1));
        let pressure = self.measured.then(|| pressure_of(event));
        let size = (self.max.0 - self.min.0, self.max.1 - self.min.1);
        scene.push_stroke_point(self.handle, point, pressure, size, entropy.integer());
    }
}

/// How far the end of a dot is from its start, in Scene units.
const DOT: f64 = 0.0001;

/// The pressure of `event` as the device reported it: the shortest decimal
/// that reads back as the same `f32`, so that `0.4` is saved as `0.4`.
fn pressure_of(event: PointerEvent) -> f64 {
    event
        .pressure
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(event.pressure))
}

/// The members of a freedraw stroke beyond those of every Element, as
/// Excalidraw's `newFreeDrawElement` gives them, for a stroke that starts with
/// `event`.
fn add_members(stroke: &mut Object, event: PointerEvent, measured: bool) {
    let streamline = match event.kind {
        PointerKind::Mouse => MOUSE_STREAMLINE,
        PointerKind::Pencil | PointerKind::Finger => PRECISE_STREAMLINE,
    };
    let mut options = Object::new();
    options.insert("variability".into(), Value::String("variable".into()));
    options.insert("streamline".into(), Value::Number(streamline));

    let first = vec![Value::Number(0.0), Value::Number(0.0)];
    let pressures = if measured {
        vec![Value::Number(pressure_of(event))]
    } else {
        Vec::new()
    };
    stroke.insert("points".into(), Value::Array(vec![Value::Array(first)]));
    stroke.insert("pressures".into(), Value::Array(pressures));
    stroke.insert("simulatePressure".into(), Value::Bool(!measured));
    stroke.insert("strokeOptions".into(), Value::Object(options));
}
