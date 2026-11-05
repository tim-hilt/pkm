//! Excalidraw's repairs of the Elements of a file that is opened.
//!
//! Mirrors `restoreElements` and `restoreElement` in Excalidraw's
//! `data/restore.ts` at the pinned commit (`8bcdca4`), for the Element types
//! the core understands; Elements of other types are kept as they are. A
//! field is only added when the Element has no such member at all; present
//! values, even odd ones (`"opacity": 0`, `"boundElements": null`), are left
//! alone so that files written by excalidraw.com come back unchanged. The
//! readers in [`crate::scene::Element`] cope with invalid values instead.
//!
//! Old files' properties are migrated first ([`migrate_legacy`]):
//! `strokeSharpness`, `boundElementIds` and a text's `font`.
//!
//! Not restored: `updated` (the current time; nothing in the core reads it),
//! repairs of invalid values and binding repair (the core tolerates bindings
//! to Elements that are not there).

use std::collections::HashSet;

use crate::json::{self, Object, Value};
use crate::linear;
use crate::random::Entropy;
use crate::scene::{Element, ElementType};

/// Excalidraw's `MAX_LINEAR_PX`: lines and arrows wider or taller than this
/// would freeze the editor (a dashed one is millions of dashes), so they are
/// deleted on load.
const MAX_LINEAR_PX: f64 = 75_000.0;

/// Repairs the Elements of a file that is opened, in file order: every
/// Element of a known type gets Excalidraw's defaults for its missing fields
/// (see [`fill_missing`]), an id of its own if its id is missing, empty, not
/// a string or that of an earlier Element, and is deleted if it is a line too
/// large to draw. Elements of other types are not touched, but their ids
/// count as taken.
pub(crate) fn restore_elements(elements: &mut [Value], entropy: &mut Entropy) {
    let mut taken = HashSet::new();
    for element in elements.iter_mut().filter_map(Value::as_object_mut) {
        let id = element
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = element
            .get("type")
            .and_then(Value::as_str)
            .and_then(ElementType::from_name);
        let Some(kind) = kind else {
            taken.insert(id.to_owned());
            continue;
        };
        if id.is_empty() || !taken.insert(id.to_owned()) {
            // A new id could clash with a later one only by a 126-bit chance.
            let id = entropy.id();
            taken.insert(id.clone());
            element.insert("id".into(), Value::String(id));
        }
        migrate_legacy(element, kind);
        fill_missing(element, kind);
        if kind.is_linear() {
            delete_if_oversized(element);
        }
    }
}

/// Excalidraw's `ROUNDNESS` types that old files' `strokeSharpness: "round"`
/// becomes: the legacy radius where the type now has an adaptive one.
const ROUNDNESS_LEGACY: f64 = 1.0;
const ROUNDNESS_PROPORTIONAL_RADIUS: f64 = 2.0;

/// Replaces the properties of files from before 2023 by what Excalidraw's
/// `restoreElement` makes of them. The legacy properties are dropped, except
/// a text's `font`, which Excalidraw keeps too.
fn migrate_legacy(element: &mut Object, kind: ElementType) {
    if element.get("strokeSharpness").is_some() {
        let round = element.get("strokeSharpness").and_then(Value::as_str) == Some("round");
        if round && !is_truthy(element.get("roundness")) {
            let adaptive = matches!(kind, ElementType::Rectangle | ElementType::Image);
            let ty = if adaptive {
                ROUNDNESS_LEGACY
            } else {
                ROUNDNESS_PROPORTIONAL_RADIUS
            };
            let mut roundness = Object::new();
            roundness.insert("type".into(), Value::Number(ty));
            element.insert("roundness".into(), Value::Object(roundness));
        }
        element.remove("strokeSharpness");
    }
    if let Some(ids) = element.get("boundElementIds") {
        if let Value::Array(ids) = ids {
            let bound = ids.iter().map(|id| {
                let mut bound = Object::new();
                bound.insert("type".into(), Value::String("arrow".into()));
                bound.insert("id".into(), id.clone());
                Value::Object(bound)
            });
            let bound = Value::Array(bound.collect());
            element.insert("boundElements".into(), bound);
        }
        element.remove("boundElementIds");
    }
    if kind == ElementType::Text {
        migrate_font(element);
    }
}

/// JavaScript truthiness of an Element member.
fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => *n != 0.0 && !n.is_nan(),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A text's `font` (`"20px Virgil"`) becomes `fontSize` and `fontFamily`,
/// which it overrides; a size that is no number is the default, and a family
/// that is not a name in `FONT_FAMILY` (as one word) is Excalifont.
fn migrate_font(text: &mut Object) {
    let Some(font) = text.get("font").and_then(Value::as_str) else {
        return;
    };
    let mut parts = font.split(' ');
    let size = parts
        .next()
        .and_then(parse_float)
        .filter(|size| size.is_finite())
        .unwrap_or(DEFAULT_FONT_SIZE);
    let family = parts.next().and_then(font_family_by_name);
    text.insert("fontSize".into(), Value::Number(size));
    let family = family.unwrap_or(DEFAULT_FONT_FAMILY);
    text.insert("fontFamily".into(), Value::Number(family));
}

/// `FONT_FAMILY[name]`, for the names that are one word.
fn font_family_by_name(name: &str) -> Option<f64> {
    Some(match name {
        "Virgil" => 1.0,
        "Helvetica" => 2.0,
        "Cascadia" => 3.0,
        "Excalifont" => 5.0,
        "Nunito" => 6.0,
        "Assistant" => 10.0,
        _ => return None,
    })
}

/// JavaScript's `parseFloat` for decimal numbers: the longest numeric prefix
/// after leading whitespace.
fn parse_float(text: &str) -> Option<f64> {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let int = digits(end);
    end += int;
    let mut fraction = 0;
    if bytes.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        if int > 0 || fraction > 0 {
            end += 1 + fraction;
        }
    }
    if int == 0 && fraction == 0 {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    text[..end].parse().ok()
}

/// Members every Element has, in Excalidraw's order, as JSON text.
const COMMON: &[(&str, &str)] = &[
    ("version", "1"),
    ("versionNonce", "0"),
    ("index", "null"),
    ("isDeleted", "false"),
    ("fillStyle", r#""solid""#),
    ("strokeWidth", "2"),
    ("strokeStyle", r#""solid""#),
    ("roughness", "1"),
    ("opacity", "100"),
    ("angle", "0"),
    ("x", "0"),
    ("y", "0"),
    ("strokeColor", r##""#1e1e1e""##),
    ("backgroundColor", r#""transparent""#),
    ("width", "0"),
    ("height", "0"),
    ("seed", "1"),
    ("groupIds", "[]"),
    ("frameId", "null"),
    ("roundness", "null"),
    ("boundElements", "[]"),
    ("created", "null"),
    ("link", "null"),
    ("locked", "false"),
];

const DEFAULT_FONT_SIZE: f64 = 20.0;
/// Excalifont.
const DEFAULT_FONT_FAMILY: f64 = 5.0;

/// Adds Excalidraw's default for every missing member of `element`.
pub(crate) fn fill_missing(element: &mut Object, kind: ElementType) {
    fill(element, COMMON);
    match kind {
        ElementType::Rectangle | ElementType::Diamond | ElementType::Ellipse => {}
        ElementType::Text => fill_text(element),
        ElementType::Freedraw => fill(
            element,
            &[
                ("points", "[]"),
                ("pressures", "[]"),
                (
                    "strokeOptions",
                    r#"{"variability":"variable","streamline":0.5}"#,
                ),
            ],
        ),
        ElementType::Image => fill(
            element,
            &[
                ("status", r#""pending""#),
                ("scale", "[1,1]"),
                ("crop", "null"),
            ],
        ),
        ElementType::Line | ElementType::Arrow => fill_linear(element, kind),
    }
}

fn fill_text(element: &mut Object) {
    fill(
        element,
        &[
            ("fontSize", "20"),
            ("fontFamily", "5"),
            ("text", r#""""#),
            ("textAlign", r#""left""#),
            ("verticalAlign", r#""top""#),
            ("containerId", "null"),
        ],
    );
    if element.get("originalText").is_none() {
        let text = element.get("text").cloned().unwrap_or(Value::Null);
        element.insert("originalText".into(), text);
    }
    fill(element, &[("autoResize", "true")]);
    if element.get("lineHeight").is_none() {
        let line_height = line_height(element);
        element.insert("lineHeight".into(), Value::Number(line_height));
    }
    fill(
        element,
        &[("labelPosition", "null"), ("baseFontSize", "null")],
    );
}

/// The line height of a text Element that has none: detected from its height
/// (as old files need), else the font's own.
fn line_height(text: &Object) -> f64 {
    let finite = |key: &str| {
        text.get(key)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
    };
    let font_size = finite("fontSize").unwrap_or(DEFAULT_FONT_SIZE);
    let lines = text
        .get("text")
        .and_then(Value::as_str)
        .map_or(1, line_count);
    let detected = finite("height")
        .filter(|&height| height != 0.0)
        .map(|height| height / lines as f64 / font_size);
    match detected {
        Some(detected) if detected.is_finite() && detected > 0.0 => detected,
        _ => font_line_height(finite("fontFamily").unwrap_or(DEFAULT_FONT_FAMILY)),
    }
}

/// Lines in `text`, where `\r\n`, `\n` and `\r` all end one.
fn line_count(text: &str) -> usize {
    let mut lines = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines += 1;
            }
            '\n' => lines += 1,
            _ => {}
        }
    }
    lines
}

/// `FONT_METADATA[family].metrics.lineHeight`; unknown families use
/// Excalifont's.
fn font_line_height(family: f64) -> f64 {
    match family as u32 {
        2 | 7 | 9 => 1.15, // Helvetica, Lilita One, Liberation Sans
        3 => 1.2,          // Cascadia
        _ => 1.25,         // Excalifont, Virgil, Nunito, Comic Shanns, Assistant
    }
}

fn fill_linear(element: &mut Object, kind: ElementType) {
    if element.get("points").is_none() {
        let size = |key: &str| {
            element
                .get(key)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
                .unwrap_or(0.0)
        };
        let end = Value::Array(vec![
            Value::Number(size("width")),
            Value::Number(size("height")),
        ]);
        let origin = Value::Array(vec![Value::Number(0.0), Value::Number(0.0)]);
        element.insert("points".into(), Value::Array(vec![origin, end]));
    }
    fill(
        element,
        &[
            ("startBinding", "null"),
            ("endBinding", "null"),
            ("startArrowhead", "null"),
            (
                "endArrowhead",
                if kind == ElementType::Arrow {
                    r#""arrow""#
                } else {
                    "null"
                },
            ),
        ],
    );
    if kind == ElementType::Line {
        fill(element, &[("polygon", "false")]);
    }
}

/// Excalidraw's `handleOversizedLinearElements` for a line or arrow, whose
/// size is that of its points.
fn delete_if_oversized(element: &mut Object) {
    let Some(points) = Element::view(0, element).map(|line| line.points()) else {
        return;
    };
    let (width, height) = linear::extent(&points);
    if width <= MAX_LINEAR_PX && height <= MAX_LINEAR_PX {
        return;
    }
    for (key, value) in [
        ("x", "0"),
        ("y", "0"),
        ("width", "100"),
        ("height", "100"),
        ("points", "[[0,0],[100,100]]"),
        ("isDeleted", "true"),
    ] {
        element.insert(key.into(), json::parse(value).expect("valid JSON"));
    }
}

/// Adds each `(key, JSON)` default the element lacks.
fn fill(element: &mut Object, defaults: &[(&str, &str)]) {
    for &(key, default) in defaults {
        if element.get(key).is_none() {
            let value = json::parse(default).expect("defaults are valid JSON");
            element.insert(key.to_owned(), value);
        }
    }
}
