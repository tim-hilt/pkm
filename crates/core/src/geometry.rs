//! Geometry output: what the core tells clients to draw (ADR-0008).
//!
//! The core describes Elements as resolution-independent paths in Scene
//! coordinates and never tessellates or rasterises them. Clients cache the
//! paths and apply the viewport themselves, so panning and zooming never
//! reach the core.
//!
//! # Zero-copy format
//!
//! [`Geometry`] is three flat, contiguous buffers that clients read in place
//! through raw pointers (ADR-0007); nothing is serialised:
//!
//! * [`Geometry::parts`]: one fixed-size [`Part`] record per drawable path,
//!   carrying its style and the ranges of its ops in the other two buffers.
//! * [`Geometry::verbs`]: one byte per op, a [`Verb`] number.
//! * [`Geometry::coords`]: the `f32` operands of all ops back to back; a verb
//!   takes [`Verb::coord_count`] of them.
//!
//! Records are `#[repr(C)]` with only 4-byte fields, so clients can mirror
//! them without padding surprises. Numbers are in the host's byte order.

/// What a [`Part`] is drawn as. Numbers are part of the client contract.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// Outline of constant width with round caps and joins.
    Stroke = 0,
}

/// One path operation. Numbers are part of the client contract.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Starts a new subpath at `(x, y)`.
    Move = 0,
    /// Straight line to `(x, y)`.
    Line = 1,
    /// Cubic Bézier to `(x, y)` with control points `(x1, y1)`, `(x2, y2)`,
    /// operands in the order `x1 y1 x2 y2 x y`.
    Cubic = 2,
}

impl Verb {
    /// The verb with number `byte`, if there is one.
    pub fn from_byte(byte: u8) -> Option<Verb> {
        match byte {
            0 => Some(Verb::Move),
            1 => Some(Verb::Line),
            2 => Some(Verb::Cubic),
            _ => None,
        }
    }

    /// How many `f32` operands the verb consumes from [`Geometry::coords`].
    pub fn coord_count(self) -> usize {
        match self {
            Verb::Move | Verb::Line => 2,
            Verb::Cubic => 6,
        }
    }
}

/// Resolved style of a stroked path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    /// Colour as `0xRRGGBBAA`.
    pub color: u32,
    /// Line width in Scene units.
    pub width: f32,
    /// Element opacity, `0.0` (invisible) to `1.0`; independent of the
    /// colour's own alpha.
    pub opacity: f32,
}

/// One drawable path: its style and where its ops live in the buffers.
///
/// Parts of one Element share `element` and `version`; clients key their
/// caches on `(element, version)` and rebuild when the version changes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    /// Core-assigned handle of the Element this part belongs to.
    pub element: u32,
    /// Element version; changes whenever the Element's geometry does.
    pub version: u32,
    /// A [`PartKind`] number.
    pub kind: u32,
    /// Colour as `0xRRGGBBAA`.
    pub color: u32,
    /// See [`StrokeStyle::opacity`].
    pub opacity: f32,
    /// Line width in Scene units.
    pub width: f32,
    /// Index of this part's first verb in [`Geometry::verbs`].
    pub verb_start: u32,
    /// Number of verbs; the part's operands start at `coord_start`.
    pub verb_count: u32,
    /// Index of this part's first operand in [`Geometry::coords`].
    pub coord_start: u32,
}

/// Paths to draw, in z-order (later parts are drawn over earlier ones).
#[derive(Debug, Default)]
pub struct Geometry {
    parts: Vec<Part>,
    verbs: Vec<u8>,
    coords: Vec<f32>,
}

impl Geometry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an empty stroked part for `element` at `version` and returns
    /// the writer that fills it with ops. Parts cannot be reopened, so
    /// buffers only ever grow at their end.
    pub fn stroke(&mut self, element: u32, version: u32, style: StrokeStyle) -> PartWriter<'_> {
        self.parts.push(Part {
            element,
            version,
            kind: PartKind::Stroke as u32,
            color: style.color,
            opacity: style.opacity,
            width: style.width,
            verb_start: index(self.verbs.len()),
            verb_count: 0,
            coord_start: index(self.coords.len()),
        });
        PartWriter { geometry: self }
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    pub fn verbs(&self) -> &[u8] {
        &self.verbs
    }

    pub fn coords(&self) -> &[f32] {
        &self.coords
    }
}

/// Appends ops to the part it was created for by [`Geometry::stroke`].
pub struct PartWriter<'a> {
    geometry: &'a mut Geometry,
}

impl PartWriter<'_> {
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.push(Verb::Move, &[x, y]);
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.push(Verb::Line, &[x, y]);
    }

    /// Cubic Bézier to `(x, y)` with control points `(x1, y1)` and `(x2, y2)`.
    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.push(Verb::Cubic, &[x1, y1, x2, y2, x, y]);
    }

    fn push(&mut self, verb: Verb, operands: &[f32]) {
        debug_assert_eq!(operands.len(), verb.coord_count());
        let geometry = &mut *self.geometry;
        geometry.verbs.push(verb as u8);
        geometry.coords.extend_from_slice(operands);
        // A writer only exists right after its part was pushed.
        geometry.parts.last_mut().expect("writer's part").verb_count += 1;
    }
}

/// Buffer positions are `u32` in the format; a Scene never gets near 4 G ops.
fn index(len: usize) -> u32 {
    u32::try_from(len).expect("geometry buffer exceeds u32 range")
}
