//! excalidraw.com's own exports survive `pkm_base::json` byte for byte.

use pkm_base::json::{Value, parse};

const MINIFIED: &str = include_str!("../fixtures/drawing.min.excalidraw");
const PRETTY: &str = include_str!("../fixtures/drawing.pretty.excalidraw");

#[test]
fn minified_export_reproduces_the_original_bytes() {
    assert_eq!(parse(MINIFIED).unwrap().to_json(), MINIFIED);
}

#[test]
fn pretty_export_reproduces_the_original_bytes() {
    // excalidraw.com itself saves with `JSON.stringify(data, null, 2)`.
    assert_eq!(parse(PRETTY).unwrap().to_json_pretty(), PRETTY);
}

#[test]
fn pretty_and_minified_exports_parse_to_the_same_value() {
    assert_eq!(parse(PRETTY).unwrap(), parse(MINIFIED).unwrap());
}

#[test]
fn unknown_element_types_and_properties_survive() {
    let scene = parse(MINIFIED).unwrap();
    let elements = scene.get("elements").and_then(Value::as_array).unwrap();
    let unknown = elements.last().unwrap();
    assert_eq!(
        unknown.get("type").and_then(Value::as_str),
        Some("futuristic-widget")
    );
    assert!(unknown.get("customProp").is_some());
}
