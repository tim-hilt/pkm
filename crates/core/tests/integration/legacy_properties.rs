//! Legacy properties of old files are migrated on load as Excalidraw's
//! `restoreElement` does (issue 54). The expected values are what Excalidraw
//! itself made of the same Elements: `legacy.restored.json`, written by the
//! golden fixture generator from `legacy.excalidraw`.

use std::fs;
use std::path::PathBuf;

use workingtitle_core::Editor;
use workingtitle_core::json::{Value, parse};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The Elements of the saved Drawing by id.
fn saved_elements(original: &str) -> Vec<Value> {
    let mut editor = Editor::new();
    editor.load(original.as_bytes()).expect("the Drawing loads");
    let saved = String::from_utf8(editor.save()).unwrap();
    let drawing = parse(&saved).unwrap();
    drawing
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .to_vec()
}

fn with_id<'a>(elements: &'a [Value], id: &str) -> &'a Value {
    elements
        .iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no Element {id}"))
}

fn json(element: &Value, key: &str) -> String {
    element.get(key).map_or("absent".into(), Value::to_json)
}

/// The properties the migrations decide.
const MIGRATED: [&str; 8] = [
    "roundness",
    "strokeSharpness",
    "boundElements",
    "boundElementIds",
    "font",
    "fontSize",
    "fontFamily",
    "lineHeight",
];

#[test]
fn every_legacy_element_is_migrated_as_excalidraw_does() {
    let saved = saved_elements(&fixture("legacy.excalidraw"));
    let expected = parse(&fixture("legacy.restored.json")).unwrap();
    let expected = expected.as_object().unwrap();
    assert!(expected.len() >= 30, "the fixture covers the edge cases");

    for (id, upstream) in expected.iter() {
        let ours = with_id(&saved, id);
        for key in MIGRATED {
            assert_eq!(json(ours, key), json(upstream, key), "{id}: {key}");
        }
    }
    assert_eq!(saved.len(), expected.len());
}

#[test]
fn migrated_values_are_what_the_task_describes() {
    let saved = saved_elements(&fixture("legacy.excalidraw"));
    let value = |id: &str, key: &str| json(with_id(&saved, id), key);

    assert_eq!(value("rectangle-round", "roundness"), r#"{"type":1}"#);
    assert_eq!(value("ellipse-round", "roundness"), r#"{"type":2}"#);
    assert_eq!(value("rectangle-round", "strokeSharpness"), "absent");
    assert_eq!(value("rectangle-sharp", "roundness"), "null");
    assert_eq!(
        value("rectangle-round-has-roundness", "roundness"),
        r#"{"type":3}"#
    );
    assert_eq!(
        value("bound-ids", "boundElements"),
        r#"[{"type":"arrow","id":"a1"},{"type":"arrow","id":"a2"}]"#
    );
    assert_eq!(value("bound-ids", "boundElementIds"), "absent");
    assert_eq!(value("font-20px-Virgil", "fontSize"), "20");
    assert_eq!(value("font-20px-Virgil", "fontFamily"), "1");
    assert_eq!(value("font-28px-Helvetica", "fontSize"), "28");
    assert_eq!(value("font-28px-Helvetica", "fontFamily"), "2");
    assert_eq!(value("font-20px-Nonexistent", "fontFamily"), "5");
}

#[test]
fn unknown_properties_are_still_kept_verbatim() {
    let saved = saved_elements(
        r#"{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","strokeSharpness":"round","mine":{"k":[1,2]}}]}"#,
    );
    assert_eq!(json(&saved[0], "mine"), r#"{"k":[1,2]}"#);
}
