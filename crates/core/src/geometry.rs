//! Geometry output: what the core tells clients to draw (ADR-0008).
//!
//! The core describes Elements as resolution-independent paths in Scene
//! coordinates and never tessellates or rasterises them. Clients cache the
//! paths and apply the viewport themselves, so panning and zooming never
//! reach the core.
//!
//! # Zero-copy format
//!
//! [`Geometry`] is flat, contiguous buffers that clients read in place through
//! raw pointers (ADR-0007); nothing is serialised:
//!
//! * [`Geometry::parts`]: one fixed-size [`Part`] record per drawable path,
//!   carrying its style and the ranges of its ops in the other two buffers.
//! * [`Geometry::verbs`]: one byte per op, a [`Verb`] number.
//! * [`Geometry::coords`]: the `f32` operands of all ops back to back; a verb
//!   takes [`Verb::coord_count`] of them.
//!
//! Text is not made of paths: clients lay out glyphs (ADR-0008). Two more
//! buffers describe it:
//!
//! * [`Geometry::text_lines`]: one fixed-size [`TextLine`] record per line of
//!   text, carrying its font, colour and position, and the byte range of its
//!   characters in the last buffer. Lines are interleaved with the parts in
//!   z-order by [`TextLine::part_index`].
//! * [`Geometry::text`]: the characters of all lines back to back, as UTF-8.
//!
//! Images are not made of paths either: they are pixels the client decodes
//! (the core never does, ADR-0003). Two more buffers describe them:
//!
//! * [`Geometry::images`]: one fixed-size [`Image`] record per image,
//!   carrying its placement, crop, scale and style, and the byte range of its
//!   file id in the last buffer. Images are interleaved with the parts in
//!   z-order by [`Image::part_index`], just like text lines.
//! * [`Geometry::file_ids`]: the file ids of all images back to back, as
//!   UTF-8. The bytes of the file itself come from the editor, by id.
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

/// Where a line of text sits relative to its [`TextLine::x`]. Numbers are
/// part of the client contract.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// The line starts at `x`.
    Left = 0,
    /// The line is centred on `x`.
    Center = 1,
    /// The line ends at `x`.
    Right = 2,
}

/// Resolved style of a line of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Colour as `0xRRGGBBAA`.
    pub color: u32,
    /// See [`StrokeStyle::opacity`].
    pub opacity: f32,
    /// The Element's `fontFamily`. Clients draw every family with Excalifont
    /// (the only bundled font); the number is carried along so nothing is lost.
    pub font_family: u32,
    /// Font size in Scene units.
    pub font_size: f32,
    pub align: TextAlign,
}

/// One line of text to draw, set in Excalifont: its style and where its
/// characters live in [`Geometry::text`].
///
/// Like parts, lines of one Element share `element` and `version`, and
/// clients key their caches on them.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLine {
    /// Core-assigned handle of the Element this line belongs to.
    pub element: u32,
    /// Changes whenever the lines of the Element do, which includes changes
    /// of the container a text is bound to.
    pub version: u32,
    /// The number of parts that come before this line in z-order: draw it
    /// after the parts before `part_index` and before the part at it.
    pub part_index: u32,
    /// Colour as `0xRRGGBBAA`.
    pub color: u32,
    /// See [`StrokeStyle::opacity`].
    pub opacity: f32,
    /// Font size in Scene units; glyphs scale with it from their design size.
    pub font_size: f32,
    /// The Element's `fontFamily`; see [`TextStyle::font_family`].
    pub font_family: u32,
    /// A [`TextAlign`] number.
    pub align: u32,
    /// Where the line is anchored on its baseline, in Scene coordinates
    /// (already turned by `angle`): at its start, middle or end, see `align`.
    pub x: f32,
    pub y: f32,
    /// Clockwise rotation in radians about `(x, y)`.
    pub angle: f32,
    /// Index of the line's first byte in [`Geometry::text`].
    pub text_start: u32,
    /// Number of bytes (UTF-8) of the line, which has no line breaks.
    pub text_len: u32,
}

/// How far the file of an image is known to have got, from its `status`.
/// Numbers are part of the client contract.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageStatus {
    /// Not stored yet (or the file does not say).
    Pending = 0,
    Saved = 1,
    /// Loading the file failed; clients show the error placeholder instead of
    /// the image if they have no pixels for it.
    Error = 2,
}

/// One image to draw: the part of a file shown in a box, and its style.
///
/// The box is `width` x `height` with its top left corner at `(x, y)`. To
/// draw it, turn it clockwise by `angle` about its centre, then mirror it
/// about its centre by `scale_x` and `scale_y` (each `1` or `-1`), as
/// Excalidraw does. The picture fills the box from `(u0, v0)` (top left) to
/// `(u1, v1)` (bottom right) of the file, in fractions of its width and
/// height; a mirrored image has the mirrored picture in the same box.
///
/// Like parts, images of one Element share `element` and `version`, and
/// clients key their caches on them. What the image shows besides this
/// record is the file named by the file id, so a cache of pixels is keyed on
/// the file id alone.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Image {
    /// Core-assigned handle of the Element this image belongs to.
    pub element: u32,
    pub version: u32,
    /// The number of parts that come before this image in z-order: draw it
    /// after the parts before `part_index` and before the part at it.
    pub part_index: u32,
    /// Index of the file id's first byte in [`Geometry::file_ids`]. The id is
    /// empty for an image that has no file.
    pub file_id_start: u32,
    /// Number of bytes (UTF-8) of the file id.
    pub file_id_len: u32,
    /// An [`ImageStatus`] number.
    pub status: u32,
    /// See [`StrokeStyle::opacity`].
    pub opacity: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Clockwise rotation in radians about the box's centre.
    pub angle: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    /// Radius in Scene units of the box's rounded corners; `0` for sharp ones.
    /// The picture is cut off outside the rounded box.
    pub corner_radius: f32,
}

/// Paths, text and images to draw, in z-order (later ones are drawn over earlier ones).
#[derive(Debug, Default)]
pub struct Geometry {
    generation: u64,
    parts: Vec<Part>,
    verbs: Vec<u8>,
    coords: Vec<f32>,
    text_lines: Vec<TextLine>,
    text: String,
    images: Vec<Image>,
    file_ids: String,
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

    /// Appends a line of text for `element` at `version`, anchored at
    /// `(x, y)` on its baseline and turned by `angle` about that point.
    pub fn text_line(
        &mut self,
        element: u32,
        version: u32,
        style: TextStyle,
        line: &str,
        (x, y): (f32, f32),
        angle: f32,
    ) {
        let text_start = index(self.text.len());
        self.text.push_str(line);
        self.text_lines.push(TextLine {
            element,
            version,
            part_index: index(self.parts.len()),
            color: style.color,
            opacity: style.opacity,
            font_size: style.font_size,
            font_family: style.font_family,
            align: style.align as u32,
            x,
            y,
            angle,
            text_start,
            text_len: index(line.len()),
        });
    }

    pub fn text_lines(&self) -> &[TextLine] {
        &self.text_lines
    }

    /// The characters of all lines; see [`TextLine::text_start`].
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The characters of `line`.
    pub fn line_text(&self, line: &TextLine) -> &str {
        let start = line.text_start as usize;
        &self.text[start..start + line.text_len as usize]
    }

    /// Appends an image of file `file_id` (empty if it has none), drawn
    /// after the parts added so far. The record's `part_index`,
    /// `file_id_start` and `file_id_len` are filled in here; pass `0`.
    pub fn image(&mut self, file_id: &str, image: Image) {
        self.images.push(Image {
            part_index: index(self.parts.len()),
            file_id_start: index(self.file_ids.len()),
            file_id_len: index(file_id.len()),
            ..image
        });
        self.file_ids.push_str(file_id);
    }

    pub fn images(&self) -> &[Image] {
        &self.images
    }

    /// The ids of the files of all images; see [`Image::file_id_start`].
    pub fn file_ids(&self) -> &str {
        &self.file_ids
    }

    /// The id of the file `image` shows.
    pub fn image_file_id(&self, image: &Image) -> &str {
        let start = image.file_id_start as usize;
        &self.file_ids[start..start + image.file_id_len as usize]
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
