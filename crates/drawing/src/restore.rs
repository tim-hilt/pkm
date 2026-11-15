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

use crate::linear;
use crate::scene::{Element, ElementType};
use pkm_base::json::{self, Object, Value};
use pkm_base::random::Entropy;

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

#[cfg(test)]
mod tests {
    //! Files with extreme or invalid Element values must not make the core do
    //! unbounded work (issue 55), tested at the Editor seam.
    //!
    //! The bounded cases assert how much geometry comes out, with sizes chosen so
    //! that code without the bounds makes a lot of it quickly instead of running
    //! out of memory: they are safe to run on any machine. The full probe Drawing
    //! is only meant for code that has the bounds.

    use crate::{Editor, ExportScope};

    fn editor_with(elements: &str) -> Editor {
        let json =
            format!(r#"{{"type":"excalidraw","version":2,"elements":[{elements}],"files":{{}}}}"#);
        let mut editor = Editor::new();
        editor.load(json.as_bytes()).unwrap();
        editor
    }

    /// Every call that turns Element values into work.
    fn use_all(editor: &mut Editor) -> usize {
        let drawn = editor.geometry().coords().len();
        let exported = editor.export(ExportScope::Scene);
        editor.select_all();
        let _ = editor.overlay();
        let _ = editor.properties();
        let _ = editor.copy_selection();
        editor.duplicate();
        editor.undo();
        editor.delete_selection();
        drawn + exported.map_or(0, |e| e.geometry.coords().len())
    }

    #[test]
    fn a_tall_hatched_rectangle_is_not_filled_with_hundreds_of_thousands_of_lines() {
        // 300,000 scan lines, a hatch line every 8th.
        let mut editor = editor_with(
            r##"{"id":"a","type":"rectangle","width":50,"height":300000,"strokeWidth":2,
        "backgroundColor":"#ff0000","fillStyle":"hachure","roundness":null}"##,
        );

        assert!(use_all(&mut editor) < 100_000);
    }

    #[test]
    fn a_huge_ellipse_is_drawn_in_a_bounded_number_of_steps() {
        // Unbounded, 1e10 would take over a hundred thousand steps.
        let mut editor = editor_with(r#"{"id":"a","type":"ellipse","width":1e10,"height":1e10}"#);

        assert!(use_all(&mut editor) < 300_000);
    }

    #[test]
    fn extreme_ellipses_and_fills_finish() {
        let mut editor = editor_with(
            r##"{"id":"a","type":"ellipse","width":1e300,"height":1e300,"x":-1e300},
        {"id":"b","type":"ellipse","width":1e300,"height":1e300,"backgroundColor":"#f00","fillStyle":"zigzag"},
        {"id":"c","type":"rectangle","width":1e300,"height":1e300,"backgroundColor":"#f00","fillStyle":"cross-hatch"},
        {"id":"d","type":"diamond","width":1e300,"height":1e300,"roundness":{"type":2},"backgroundColor":"#f00"},
        {"id":"e","type":"rectangle","width":1e300,"height":1e300,"roundness":{"type":3},"strokeStyle":"dotted"},
        {"id":"f","type":"line","points":[[0,0],[1e300,1e300],[5,5]],"strokeStyle":"dashed","roughness":99},
        {"id":"g","type":"freedraw","points":[[0,0],[1e300,1e300],[5,5]],"pressures":[1,1,1],"strokeWidth":1e300}"##,
        );

        use_all(&mut editor);
    }

    /// The Drawing of the review probe: every kind of invalid value at once.
    #[test]
    fn the_probe_drawing_finishes() {
        let mut editor = editor_with(
            r#"
  {"id":"a","type":"rectangle","x":"10","y":null,"width":-5,"height":1e999,"angle":"x",
   "strokeWidth":-1,"roughness":99,"opacity":"50","seed":-3.5,"roundness":{"type":"x"},
   "boundElements":[{"id":"gone","type":"text"},7],"groupIds":"g","fillStyle":5},
  {"id":"b","type":"diamond","width":0,"height":0,"strokeStyle":["dashed"]},
  {"id":"c","type":"line","points":[],"strokeStyle":"dotted"},
  {"id":"d","type":"arrow","points":[[0,0],["x",1],[1e999,0]],
   "startBinding":{"elementId":"gone"},"endBinding":{"elementId":"a","fixedPoint":"no"},
   "startArrowhead":7,"endArrowhead":"unknown"},
  {"id":"e","type":"freedraw","points":[[1,1]],"pressures":[2,"x"]},
  {"id":"f","type":"freedraw","points":"none","pressures":null},
  {"id":"g","type":"text","text":null,"fontSize":0,"lineHeight":-1,"containerId":"gone"},
  {"id":"h","type":"text","text":"x","containerId":"a","fontFamily":"Virgil"},
  {"id":"i","type":"image","fileId":"missing","scale":[0,"x"],"crop":{"naturalWidth":0}},
  {"id":"j","type":"ellipse","width":1e300,"height":1e300,"x":-1e300}"#,
        );

        use_all(&mut editor);
    }
}
