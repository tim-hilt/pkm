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
