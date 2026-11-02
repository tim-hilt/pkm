//! Excalidraw's defaults for fields a file leaves out.
//!
//! Mirrors `restoreElement` in Excalidraw's `data/restore.ts` at the pinned
//! commit (`8bcdca4`), for the Element types the core understands. A field is
//! only added when the Element has no such member at all; present values, even
//! odd ones (`"opacity": 0`, `"boundElements": null`), are left alone so that
//! files written by excalidraw.com come back unchanged.
//!
//! Not restored, because the core has no use for it yet or cannot do it:
//! `id` (Excalidraw invents a random one) and `updated` (the current time),
//! legacy-property migrations (`strokeSharpness`, `font`, `boundElementIds`),
//! repairs of invalid values and binding repair.

use crate::json::{self, Object, Value};
use crate::scene::ElementType;

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

/// Adds each `(key, JSON)` default the element lacks.
fn fill(element: &mut Object, defaults: &[(&str, &str)]) {
    for &(key, default) in defaults {
        if element.get(key).is_none() {
            let value = json::parse(default).expect("defaults are valid JSON");
            element.insert(key.to_owned(), value);
        }
    }
}
