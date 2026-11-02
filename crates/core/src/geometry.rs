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
    /// The area enclosed by the path, which clients treat as closed: every
    /// subpath ends with an implicit line back to its [`Verb::Move`]. Which
    /// points count as enclosed is decided by the part's [`FillRule`].
    Fill = 1,
}

/// Which points of the plane a [`PartKind::Fill`] covers when its path
/// crosses itself. Numbers are part of the client contract.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    /// A point is inside if the path winds around it at all; overlapping
    /// stretches stay filled.
    NonZero = 0,
    /// A point is inside if a ray from it crosses the path an odd number of
    /// times; overlapping stretches become holes.
    EvenOdd = 1,
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

/// A repeating dash pattern: `on` units drawn, then `off` units skipped, in
/// Scene units along the path. The pattern starts over at every
/// [`Verb::Move`], like a canvas' `setLineDash`; the round caps of the stroke
/// come on top of the `on` length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dash {
    pub on: f32,
    pub off: f32,
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
    /// `None` for a solid line.
    pub dash: Option<Dash>,
}

/// Resolved style of a filled path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillStyle {
    /// Colour as `0xRRGGBBAA`.
    pub color: u32,
    /// See [`StrokeStyle::opacity`].
    pub opacity: f32,
    pub rule: FillRule,
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
    /// Length of the drawn stretches of a dashed line, see [`Dash`]; `0` (with
    /// `dash_off` also `0`) for a solid line.
    pub dash_on: f32,
    /// Length of the gaps of a dashed line.
    pub dash_off: f32,
    /// A [`FillRule`] number; `0` for strokes, which have no use for it.
    pub fill_rule: u32,
}

/// Paths to draw, in z-order (later parts are drawn over earlier ones).
#[derive(Debug, Default)]
pub struct Geometry {
    generation: u64,
    parts: Vec<Part>,
    verbs: Vec<u8>,
    coords: Vec<f32>,
}

impl Geometry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty geometry of the Scene state `generation`; see
    /// [`Geometry::generation`].
    pub fn with_generation(generation: u64) -> Self {
        Self {
            generation,
            ..Self::default()
        }
    }

    /// Tells clients whether their per-Element caches, keyed by
    /// `(Part::element, Part::version)`, are still valid: it changes whenever
    /// an [`Part::element`] handle may stand for another Element than before
    /// (the Scene was replaced, Elements were added or removed). While it
    /// stays the same, an Element with an unchanged version has unchanged
    /// geometry.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Appends an empty stroked part for `element` at `version` and returns
    /// the writer that fills it with ops. Parts cannot be reopened, so
    /// buffers only ever grow at their end.
    pub fn stroke(&mut self, element: u32, version: u32, style: StrokeStyle) -> PartWriter<'_> {
        let dash = style.dash.unwrap_or(Dash { on: 0.0, off: 0.0 });
        self.begin(Part {
            kind: PartKind::Stroke as u32,
            color: style.color,
            opacity: style.opacity,
            width: style.width,
            dash_on: dash.on,
            dash_off: dash.off,
            ..self.empty_part(element, version)
        })
    }

    /// Like [`Geometry::stroke`] for a filled part.
    pub fn fill(&mut self, element: u32, version: u32, style: FillStyle) -> PartWriter<'_> {
        self.begin(Part {
            kind: PartKind::Fill as u32,
            color: style.color,
            opacity: style.opacity,
            fill_rule: style.rule as u32,
            ..self.empty_part(element, version)
        })
    }

    /// A part with no ops yet that would start at the buffers' ends.
    fn empty_part(&self, element: u32, version: u32) -> Part {
        Part {
            element,
            version,
            kind: PartKind::Stroke as u32,
            color: 0,
            opacity: 1.0,
            width: 0.0,
            verb_start: index(self.verbs.len()),
            verb_count: 0,
            coord_start: index(self.coords.len()),
            dash_on: 0.0,
            dash_off: 0.0,
            fill_rule: FillRule::NonZero as u32,
        }
    }

    fn begin(&mut self, part: Part) -> PartWriter<'_> {
        self.parts.push(part);
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

/// Appends ops to the part it was created for by [`Geometry::stroke`] or [`Geometry::fill`].
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
