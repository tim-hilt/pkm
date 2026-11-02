//! Reader for the golden fixtures (format documented in
//! `tests/fixtures/golden/README.md`), built only on the core's own JSON.
//!
//! Shared by several test binaries, hence not every item is used by each.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use workingtitle_core::json::{Object, Value, parse};
use workingtitle_core::{Editor, Geometry, Part, Verb};

/// An editor that has loaded golden fixture `<name>.excalidraw`.
pub fn editor_with_fixture(name: &str) -> Editor {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden")
        .join(format!("{name}.excalidraw"));
    let mut editor = Editor::new();
    editor.load(&fs::read(path).expect("fixture")).unwrap();
    editor
}

/// An editor whose Scene is one Element with the given JSON members.
pub fn editor_with_element(element: &str) -> Editor {
    let mut editor = Editor::new();
    let drawing = format!(r#"{{"type":"excalidraw","version":2,"elements":[{{{element}}}]}}"#);
    editor.load(drawing.as_bytes()).unwrap();
    editor
}

/// The ops of `part` as `(verb, operands)`.
pub fn ops(geometry: &Geometry, part: &Part) -> Vec<(Verb, Vec<f32>)> {
    let verbs = &geometry.verbs()[part.verb_start as usize..][..part.verb_count as usize];
    let mut coords = &geometry.coords()[part.coord_start as usize..];
    verbs
        .iter()
        .map(|&byte| {
            let verb = Verb::from_byte(byte).expect("known verb");
            let (taken, rest) = coords.split_at(verb.coord_count());
            coords = rest;
            (verb, taken.to_vec())
        })
        .collect()
}

/// One `.excalidraw` source file and the output Excalidraw generated for it.
pub struct Fixture {
    scene: Value,
    golden: Value,
}

/// One drawable of an Element, in z-order (fill before stroke).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A rough.js `Drawable`: `shape` is rough.js' name for it ("rectangle",
    /// "path", "curve", ...), `options` the rough.js options Excalidraw chose.
    Rough {
        shape: String,
        options: Value,
        sets: Vec<OpSet>,
    },
    /// A closed polygon outlining a freedraw stroke (perfect-freehand).
    Outline(Vec<[f64; 2]>),
}

/// A rough.js `OpSet`: ops that are stroked (`path`) or filled (`fillPath`,
/// `fillSketch`) together.
#[derive(Debug, Clone, PartialEq)]
pub struct OpSet {
    pub kind: String,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Move([f64; 2]),
    LineTo([f64; 2]),
    BCurveTo([f64; 6]),
}

impl Op {
    /// Every coordinate the op mentions, as (x, y) pairs.
    pub fn points(&self) -> Vec<[f64; 2]> {
        match *self {
            Op::Move(p) | Op::LineTo(p) => vec![p],
            Op::BCurveTo([a, b, c, d, e, f]) => vec![[a, b], [c, d], [e, f]],
        }
    }
}

impl Fixture {
    /// Loads `<name>.excalidraw` and `<name>.golden.json` from
    /// `tests/fixtures/golden`.
    pub fn load(name: &str) -> Fixture {
        let read = |file: String| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/golden")
                .join(&file);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            parse(&text).unwrap_or_else(|e| panic!("{file} is not valid JSON: {e:?}"))
        };
        Fixture {
            scene: read(format!("{name}.excalidraw")),
            golden: read(format!("{name}.golden.json")),
        }
    }

    /// The Elements of the source file, in file order.
    pub fn elements(&self) -> &[Value] {
        self.scene
            .get("elements")
            .and_then(Value::as_array)
            .expect("scene has an elements array")
    }

    /// The Element with `id`.
    pub fn element(&self, id: &str) -> &Value {
        self.elements()
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("no element {id}"))
    }

    /// The Excalidraw / rough.js / perfect-freehand versions that produced the
    /// golden output, e.g. `upstream("roughjs") == "4.6.4"`.
    pub fn upstream(&self, package: &str) -> Option<&str> {
        self.golden
            .get("upstream")?
            .get(package)
            .and_then(Value::as_str)
    }

    /// Ids of all Elements that have golden output.
    pub fn golden_ids(&self) -> Vec<&str> {
        self.golden_elements().iter().map(|(id, _)| id).collect()
    }

    /// The golden shapes of the Element with `id`.
    pub fn shapes(&self, id: &str) -> Vec<Shape> {
        let list = self
            .golden_elements()
            .get(id)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("no golden output for {id}"));
        list.iter().map(read_shape).collect()
    }

    fn golden_elements(&self) -> &Object {
        self.golden
            .get("elements")
            .and_then(Value::as_object)
            .expect("golden has an elements object")
    }
}

fn read_shape(value: &Value) -> Shape {
    match value.get("kind").and_then(Value::as_str) {
        Some("rough") => Shape::Rough {
            shape: str_field(value, "shape").to_owned(),
            options: value.get("options").expect("options").clone(),
            sets: array_field(value, "sets").iter().map(read_set).collect(),
        },
        Some("outline") => Shape::Outline(
            array_field(value, "points")
                .iter()
                .map(numbers::<2>)
                .collect(),
        ),
        other => panic!("unknown shape kind {other:?}"),
    }
}

fn read_set(value: &Value) -> OpSet {
    OpSet {
        kind: str_field(value, "type").to_owned(),
        ops: array_field(value, "ops")
            .iter()
            .map(|op| {
                let data = op.get("data").expect("op data");
                match str_field(op, "op") {
                    "move" => Op::Move(numbers(data)),
                    "lineTo" => Op::LineTo(numbers(data)),
                    "bcurveTo" => Op::BCurveTo(numbers(data)),
                    other => panic!("unknown op {other}"),
                }
            })
            .collect(),
    }
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string {key}"))
}

fn array_field<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing array {key}"))
}

/// Reads exactly `N` numbers; panics otherwise, which fails the fixture test.
fn numbers<const N: usize>(value: &Value) -> [f64; N] {
    let items = value.as_array().expect("array of numbers");
    assert_eq!(items.len(), N, "expected {N} numbers");
    std::array::from_fn(|i| items[i].as_f64().expect("number"))
}
