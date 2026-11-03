//! Drawing a rectangle, diamond or ellipse by dragging (ADR-0006).
//!
//! Like on excalidraw.com the Element is added to the Scene when the pointer
//! goes down, at zero size, and follows the pointer from then on. When the
//! pointer lifts it stays, unless it never grew, in which case it was a tap
//! and nothing is drawn.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::history::Step;
use crate::json::{Object, Value};
use crate::random::Entropy;
use crate::scene::{ElementType, Scene};
use crate::style::Style;

/// Excalidraw's `ROUNDNESS`: rounded corners of rectangles take a fixed
/// radius (the Element's size permitting), those of the other shapes a
/// proportion of their size.
const ADAPTIVE_RADIUS: f64 = 3.0;
pub(crate) const PROPORTIONAL_RADIUS: f64 = 2.0;

/// A shape being drawn by one pointer.
#[derive(Debug)]
pub(crate) struct ShapeGesture {
    pointer: u64,
    handle: u32,
    /// Where the pointer went down, in Scene coordinates.
    origin: (f64, f64),
    /// Where the shape is now: `(x, y, width, height)`.
    frame: (f64, f64, f64, f64),
    step: Step,
}

/// How a shape gesture ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// There is a new shape in the Scene; the step is what undoes it.
    Drawn(Step),
    /// The shape was taken out of the Scene again.
    Dropped,
}

impl ShapeGesture {
    /// Starts drawing a `kind` shape with `pointer` going down at `origin`,
    /// as the action of `step`.
    pub fn begin(
        mut step: Step,
        scene: &mut Scene,
        entropy: &mut Entropy,
        style: &Style,
        kind: ElementType,
        pointer: u64,
        origin: (f64, f64),
    ) -> Self {
        let shape = new_shape(kind, entropy, style, origin);
        if let Some(id) = shape.get("id").and_then(Value::as_str) {
            step.create(id);
        }
        let handle = scene.push(shape);
        Self {
            pointer,
            handle,
            origin,
            frame: (origin.0, origin.1, 0.0, 0.0),
            step,
        }
    }

    /// The pointer that draws this shape.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Stretches the shape from where the pointer went down to `to`, as a
    /// square if `square` is set. Returns whether the shape changed.
    pub fn drag_to(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        to: (f64, f64),
        square: bool,
    ) -> bool {
        let frame = spanned(self.origin, to, square);
        if frame == self.frame {
            return false;
        }
        scene.set_frame(self.handle, frame, entropy.integer());
        self.frame = frame;
        true
    }

    /// Ends the gesture with the pointer lifting.
    pub fn finish(self, scene: &mut Scene) -> Outcome {
        // Excalidraw's `isInvisiblySmallElement`: a tap, not a shape.
        let (_, _, width, height) = self.frame;
        if width == 0.0 && height == 0.0 {
            self.cancel(scene);
            Outcome::Dropped
        } else {
            Outcome::Drawn(self.step)
        }
    }

    /// Ends the gesture without a shape.
    pub fn cancel(self, scene: &mut Scene) {
        scene.remove(self.handle);
    }
}

/// The frame of a shape dragged from `origin` to `to`: any two opposite
/// corners work, so the size is always positive. A square takes the longer
/// side (Excalidraw's `getPerfectElementSize`) and grows towards the pointer.
fn spanned(origin: (f64, f64), to: (f64, f64), square: bool) -> (f64, f64, f64, f64) {
    let (mut width, mut height) = ((to.0 - origin.0).abs(), (to.1 - origin.1).abs());
    if square {
        width = width.max(height);
        height = width;
    }
    let x = if to.0 < origin.0 {
        origin.0 - width
    } else {
        origin.0
    };
    let y = if to.1 < origin.1 {
        origin.1 - height
    } else {
        origin.1
    };
    (x, y, width, height)
}

/// A shape at `origin` without size; see [`new_element`].
fn new_shape(
    kind: ElementType,
    entropy: &mut Entropy,
    style: &Style,
    origin: (f64, f64),
) -> Object {
    let (type_name, roundness) = match kind {
        ElementType::Diamond => ("diamond", PROPORTIONAL_RADIUS),
        ElementType::Ellipse => ("ellipse", PROPORTIONAL_RADIUS),
        _ => ("rectangle", ADAPTIVE_RADIUS),
    };
    new_element(type_name, roundness, entropy, style, origin)
}

/// An Element of `type_name` at `origin` without size, with all the members
/// and in the order of one Excalidraw creates (`newElement`), so that
/// excalidraw.com reads it as its own: `roundness` is the type of its rounded
/// corners. Members of the type itself are for the caller to add. The
/// fractional `index` is left for Excalidraw to assign, which it does for
/// every Element that has none when loading.
pub(crate) fn new_element(
    type_name: &str,
    roundness: f64,
    entropy: &mut Entropy,
    style: &Style,
    (x, y): (f64, f64),
) -> Object {
    let text = |s: &str| Value::String(s.to_owned());
    let number = Value::Number;
    let now = now_ms();

    let mut rounding = Object::new();
    rounding.insert("type".into(), number(roundness));

    let mut shape = Object::new();
    for (key, value) in [
        ("id", Value::String(entropy.id())),
        ("type", text(type_name)),
        ("x", number(x)),
        ("y", number(y)),
        ("width", number(0.0)),
        ("height", number(0.0)),
        ("angle", number(0.0)),
        ("strokeColor", text(&style.stroke_color)),
        ("backgroundColor", text(&style.background_color)),
        ("fillStyle", text(style.fill_style)),
        ("strokeWidth", number(style.stroke_width)),
        ("strokeStyle", text(style.stroke_style)),
        ("roughness", number(style.roughness)),
        ("opacity", number(style.opacity)),
        ("groupIds", Value::Array(Vec::new())),
        ("frameId", Value::Null),
        ("index", Value::Null),
        ("roundness", Value::Object(rounding)),
        ("seed", number(entropy.integer())),
        ("version", number(1.0)),
        ("versionNonce", number(entropy.integer())),
        ("isDeleted", Value::Bool(false)),
        ("boundElements", Value::Null),
        ("updated", number(now)),
        ("created", number(now)),
        ("link", Value::Null),
        ("locked", Value::Bool(false)),
    ] {
        shape.insert(key.into(), value);
    }
    shape
}

/// Milliseconds since 1970, as Excalidraw's `updated` and `created`.
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_millis() as f64)
}
