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
//! Only the current file format is supported: properties of older formats
//! (`strokeSharpness`, `boundElementIds`, a text's `font`) are not migrated.
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
        fill_missing(element, kind);
        if kind.is_linear() {
            delete_if_oversized(element);
        }
    }
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
        let family = element.get("fontFamily").and_then(Value::as_f64);
        let line_height = font_line_height(family.unwrap_or(DEFAULT_FONT_FAMILY));
        element.insert("lineHeight".into(), Value::Number(line_height));
    }
    fill(
        element,
        &[("labelPosition", "null"), ("baseFontSize", "null")],
    );
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
