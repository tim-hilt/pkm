//! The Scene: the Elements and files of the open Drawing, read from and
//! written to `.excalidraw` bytes (ADR-0003, ADR-0009).
//!
//! A [`Scene`] keeps the whole document as the JSON it came from. Elements are
//! read through typed views ([`Element`]); everything the core does not
//! interpret (unknown Element types and properties, the `files` map, other
//! top-level members) is simply never touched, so it is written back
//! verbatim and in its original order. The only changes made on load are the
//! defaults for missing fields (see [`crate::restore`]), so files written by
//! excalidraw.com round-trip byte for byte.
//!
//! Saving always uses excalidraw.com's own layout: `JSON.stringify(data, null,
//! 2)` without a trailing newline.

use std::fmt;
use std::str::Utf8Error;

use crate::json::{self, Object, ParseError, Value};
use crate::restore;

/// The kinds of Element the core understands. Elements of any other type
/// (frames, embeds, types from newer Excalidraw versions) stay in the Scene
/// as opaque JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Image,
}

impl ElementType {
    /// The type named by an Element's `type` property, if the core knows it.
    pub fn from_name(name: &str) -> Option<ElementType> {
        Some(match name {
            "rectangle" => ElementType::Rectangle,
            "diamond" => ElementType::Diamond,
            "ellipse" => ElementType::Ellipse,
            "line" => ElementType::Line,
            "arrow" => ElementType::Arrow,
            "freedraw" => ElementType::Freedraw,
            "text" => ElementType::Text,
            "image" => ElementType::Image,
            _ => return None,
        })
    }

    /// Lines and arrows: Elements defined by a list of points.
    pub fn is_linear(self) -> bool {
        matches!(self, ElementType::Line | ElementType::Arrow)
    }
}

/// How a stroke is drawn: the Element's `strokeStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

/// How the background of a closed Element is filled: its `fillStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillPattern {
    Hachure,
    CrossHatch,
    Zigzag,
    Solid,
}

/// What is drawn at an end of an arrow: its `startArrowhead` or
/// `endArrowhead`. An arrow without one has `None` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrowhead {
    Arrow,
    Bar,
    Circle,
    CircleOutline,
    Triangle,
    TriangleOutline,
    Diamond,
    DiamondOutline,
    CardinalityOne,
    CardinalityMany,
    CardinalityOneOrMany,
    CardinalityExactlyOne,
    CardinalityZeroOrOne,
    CardinalityZeroOrMany,
    /// A name this version does not know; drawn as a small open arrowhead.
    Other,
}

impl Arrowhead {
    /// The arrowhead named in a file. Excalidraw's older names are
    /// translated, as its `normalizeArrowhead` does.
    fn from_name(name: &str) -> Arrowhead {
        match name {
            "arrow" => Arrowhead::Arrow,
            "bar" => Arrowhead::Bar,
            "circle" | "dot" => Arrowhead::Circle,
            "circle_outline" => Arrowhead::CircleOutline,
            "triangle" => Arrowhead::Triangle,
            "triangle_outline" => Arrowhead::TriangleOutline,
            "diamond" => Arrowhead::Diamond,
            "diamond_outline" => Arrowhead::DiamondOutline,
            "cardinality_one" | "crowfoot_one" => Arrowhead::CardinalityOne,
            "cardinality_many" | "crowfoot_many" => Arrowhead::CardinalityMany,
            "cardinality_one_or_many" | "crowfoot_one_or_many" => Arrowhead::CardinalityOneOrMany,
            "cardinality_exactly_one" => Arrowhead::CardinalityExactlyOne,
            "cardinality_zero_or_one" => Arrowhead::CardinalityZeroOrOne,
            "cardinality_zero_or_many" => Arrowhead::CardinalityZeroOrMany,
            _ => Arrowhead::Other,
        }
    }
}

/// Rounded corners of an Element: its `roundness` property. Elements with
/// sharp corners have none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Roundness {
    /// Type 1, from old files; behaves like [`Roundness::Proportional`].
    Legacy,
    /// Type 2: a quarter of the side (diamonds and linear Elements).
    Proportional,
    /// Type 3: a fixed radius (32 by default) that shrinks on small shapes
    /// (rectangles).
    Adaptive { radius: Option<f64> },
    /// A type this version does not know; its corners are not rounded, but
    /// the shape is still drawn the rounded way.
    Other,
}

/// Why a file could not be loaded as a Drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The bytes are not UTF-8 text.
    NotText(Utf8Error),
    /// The text is not valid JSON.
    InvalidJson(ParseError),
    /// Valid JSON, but not an Excalidraw Drawing; says what is wrong.
    NotADrawing(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NotText(e) => write!(f, "the file is not UTF-8 text: {e}"),
            LoadError::InvalidJson(e) => e.fmt(f),
            LoadError::NotADrawing(why) => write!(f, "not an Excalidraw Drawing: {why}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The open Drawing's content.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// The document: always an object with an `elements` array of objects.
    root: Object,
}

impl Scene {
    /// A Scene without Elements, with the members excalidraw.com exports.
    pub fn empty() -> Scene {
        let mut app_state = Object::new();
        app_state.insert("gridSize".into(), Value::Null);
        app_state.insert(
            "viewBackgroundColor".into(),
            Value::String("#ffffff".into()),
        );
        let mut root = Object::new();
        root.insert("type".into(), Value::String("excalidraw".into()));
        root.insert("version".into(), Value::Number(2.0));
        root.insert("source".into(), Value::String("WorkingTitle".into()));
        root.insert("elements".into(), Value::Array(Vec::new()));
        root.insert("appState".into(), Value::Object(app_state));
        root.insert("files".into(), Value::Object(Object::new()));
        Scene { root }
    }

    /// Reads the bytes of a `.excalidraw` file, giving every Element of a
    /// known type Excalidraw's defaults for its missing fields.
    pub fn from_bytes(bytes: &[u8]) -> Result<Scene, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(LoadError::NotText)?;
        let Value::Object(mut root) = json::parse(text).map_err(LoadError::InvalidJson)? else {
            return Err(not_a_drawing("the document is not an object"));
        };
        if root.get("type").and_then(Value::as_str) != Some("excalidraw") {
            return Err(not_a_drawing("`type` is not \"excalidraw\""));
        }
        if root.get("elements").is_none() {
            root.insert("elements".into(), Value::Array(Vec::new()));
        }
        let Some(elements) = root.get_mut("elements").and_then(Value::as_array_mut) else {
            return Err(not_a_drawing("`elements` is not an array"));
        };
        for (index, element) in elements.iter_mut().enumerate() {
            let Some(element) = element.as_object_mut() else {
                return Err(not_a_drawing(&format!("element {index} is not an object")));
            };
            if let Some(kind) = type_of(element) {
                restore::fill_missing(element, kind);
            }
        }
        Ok(Scene { root })
    }

    /// The `.excalidraw` file content, in excalidraw.com's layout.
    pub fn to_json(&self) -> String {
        self.root.to_json_pretty()
    }

    /// The Elements of a type the core understands, deleted ones included, in
    /// z-order (file order).
    pub fn elements(&self) -> impl Iterator<Item = Element<'_>> {
        self.element_values()
            .iter()
            .enumerate()
            .filter_map(|(handle, value)| {
                let object = value.as_object()?;
                Some(Element {
                    handle: u32::try_from(handle).ok()?,
                    kind: type_of(object)?,
                    object,
                })
            })
    }

    /// The colour of the Canvas as written in the file (a CSS colour), white
    /// if the file has none.
    pub fn view_background_color(&self) -> &str {
        self.root
            .get("appState")
            .and_then(Value::as_object)
            .and_then(|app_state| app_state.get("viewBackgroundColor"))
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
    }

    fn element_values(&self) -> &[Value] {
        self.root
            .get("elements")
            .and_then(Value::as_array)
            .unwrap_or_default()
    }
}

fn not_a_drawing(why: &str) -> LoadError {
    LoadError::NotADrawing(why.to_owned())
}

/// A point of a path: two finite numbers.
fn finite_pair(point: &Value) -> Option<(f64, f64)> {
    let [x, y] = point.as_array()? else {
        return None;
    };
    let (x, y) = (x.as_f64()?, y.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn type_of(element: &Object) -> Option<ElementType> {
    ElementType::from_name(element.get("type")?.as_str()?)
}

/// Read-only view of one Element of a known type.
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    handle: u32,
    kind: ElementType,
    object: &'a Object,
}

impl Element<'_> {
    /// Identifies the Element within the Scene for as long as the Scene's
    /// Elements are not added or removed: its position in the file.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    pub fn kind(&self) -> ElementType {
        self.kind
    }

    pub fn is_deleted(&self) -> bool {
        self.object
            .get("isDeleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn x(&self) -> f64 {
        self.number("x", 0.0)
    }

    pub fn y(&self) -> f64 {
        self.number("y", 0.0)
    }

    pub fn width(&self) -> f64 {
        self.number("width", 0.0)
    }

    pub fn height(&self) -> f64 {
        self.number("height", 0.0)
    }

    /// Rotation about the Element's centre in radians, clockwise.
    pub fn angle(&self) -> f64 {
        self.number("angle", 0.0)
    }

    /// The seed of the Element's hand-drawn jitter; any number, as in the file.
    pub fn seed(&self) -> f64 {
        self.number("seed", 1.0)
    }

    /// The stroke colour as written in the file (a CSS colour, usually hex).
    pub fn stroke_color(&self) -> &str {
        self.object
            .get("strokeColor")
            .and_then(Value::as_str)
            .unwrap_or("#1e1e1e")
    }

    pub fn stroke_width(&self) -> f64 {
        self.number("strokeWidth", 2.0)
    }

    pub fn line_style(&self) -> LineStyle {
        match self.object.get("strokeStyle").and_then(Value::as_str) {
            Some("dashed") => LineStyle::Dashed,
            Some("dotted") => LineStyle::Dotted,
            _ => LineStyle::Solid,
        }
    }

    /// The background colour as written in the file (a CSS colour, usually
    /// hex), `transparent` if the Element has none.
    pub fn background_color(&self) -> &str {
        self.object
            .get("backgroundColor")
            .and_then(Value::as_str)
            .unwrap_or("transparent")
    }

    /// How the background is filled. Like rough.js, anything unknown is
    /// hachure.
    pub fn fill_pattern(&self) -> FillPattern {
        match self.object.get("fillStyle").and_then(Value::as_str) {
            Some("solid") => FillPattern::Solid,
            Some("cross-hatch") => FillPattern::CrossHatch,
            Some("zigzag") => FillPattern::Zigzag,
            _ => FillPattern::Hachure,
        }
    }

    /// How sketchy the Element is drawn: 0 (architect), 1 (artist) or 2
    /// (cartoonist).
    pub fn roughness(&self) -> f64 {
        self.number("roughness", 1.0)
    }

    /// The Element's rounded corners, if it has any.
    pub fn roundness(&self) -> Option<Roundness> {
        let roundness = self.object.get("roundness")?.as_object()?;
        Some(match roundness.get("type").and_then(Value::as_f64) {
            Some(1.0) => Roundness::Legacy,
            Some(2.0) => Roundness::Proportional,
            Some(3.0) => Roundness::Adaptive {
                radius: roundness.get("value").and_then(Value::as_f64),
            },
            _ => Roundness::Other,
        })
    }

    /// The points of a line, arrow or freedraw stroke in the Element's own
    /// frame, which starts at its `x` and `y`. Points that are not number
    /// pairs are left out; a line or arrow without two valid points goes from
    /// the origin to `(width, height)`, as Excalidraw restores it. Empty for
    /// other types.
    pub fn points(&self) -> Vec<(f64, f64)> {
        let valid = self.valid_points();
        match self.kind {
            ElementType::Freedraw => valid,
            kind if kind.is_linear() && valid.len() < 2 => {
                vec![(0.0, 0.0), (self.width(), self.height())]
            }
            kind if kind.is_linear() => valid,
            _ => Vec::new(),
        }
    }

    fn valid_points(&self) -> Vec<(f64, f64)> {
        self.raw_points().iter().filter_map(finite_pair).collect()
    }

    fn raw_points(&self) -> &[Value] {
        self.object
            .get("points")
            .and_then(Value::as_array)
            .unwrap_or_default()
    }

    /// The pressure of each point of a freedraw stroke, in the order of
    /// [`Element::points`], as Excalidraw restores them: a point the file
    /// gives no pressure for has none here (the list is then shorter than
    /// the points), and a pressure that is not a number is `0.5`.
    pub(crate) fn pressures(&self) -> Vec<f64> {
        let given = self
            .object
            .get("pressures")
            .and_then(Value::as_array)
            .unwrap_or_default();
        self.raw_points()
            .iter()
            .enumerate()
            .filter(|(_, point)| finite_pair(point).is_some())
            .filter_map(|(index, _)| given.get(index))
            .map(|pressure| pressure.as_f64().filter(|p| p.is_finite()).unwrap_or(0.5))
            .collect()
    }

    /// Whether the pressure of a freedraw stroke is made up from the speed
    /// of the pointer instead of being measured. Excalidraw reads a file
    /// that does not say as measured.
    pub(crate) fn simulates_pressure(&self) -> bool {
        self.object
            .get("simulatePressure")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// How much a freedraw stroke smooths out the movement of the pointer,
    /// `0.0..=1.0`; Excalidraw's `0.5` if the file has no usable value.
    pub(crate) fn streamline(&self) -> f64 {
        self.object
            .get("strokeOptions")
            .and_then(Value::as_object)
            .and_then(|options| options.get("streamline"))
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite())
            .unwrap_or(0.5)
    }

    /// What an arrow has at its first point; `None` for lines and arrows
    /// without.
    pub fn start_arrowhead(&self) -> Option<Arrowhead> {
        if self.kind != ElementType::Arrow {
            return None;
        }
        self.arrowhead("startArrowhead", None)
    }

    /// What an arrow has at its last point. Arrows from old files that do not
    /// say have an arrow, as Excalidraw restores them.
    pub fn end_arrowhead(&self) -> Option<Arrowhead> {
        if self.kind != ElementType::Arrow {
            return None;
        }
        self.arrowhead("endArrowhead", Some(Arrowhead::Arrow))
    }

    fn arrowhead(&self, key: &str, missing: Option<Arrowhead>) -> Option<Arrowhead> {
        match self.object.get(key) {
            None => missing,
            Some(Value::String(name)) => Some(Arrowhead::from_name(name)),
            Some(_) => None,
        }
    }

    /// Whether the Element is an arrow that Excalidraw routes at right angles.
    /// The core keeps its points as they are in the file and draws them
    /// without routing.
    pub fn is_elbow_arrow(&self) -> bool {
        self.kind == ElementType::Arrow
            && self.object.get("elbowed").and_then(Value::as_bool) == Some(true)
    }

    /// `0.0` (invisible) to `1.0`.
    pub fn opacity(&self) -> f64 {
        (self.number("opacity", 100.0) / 100.0).clamp(0.0, 1.0)
    }

    /// Changes whenever the Element does; `0` if the file has no usable one.
    pub fn version(&self) -> u32 {
        // Saturating float-to-int conversion is exactly what is wanted.
        self.number("version", 0.0) as u32
    }

    fn number(&self, key: &str, default: f64) -> f64 {
        match self.object.get(key).and_then(Value::as_f64) {
            Some(n) if n.is_finite() => n,
            _ => default,
        }
    }
}
