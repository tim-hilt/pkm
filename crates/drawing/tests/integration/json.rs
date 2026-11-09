//! Behaviour of the core's own JSON (ADR-0009), tested only through its public
//! interface: `parse`, `Value::to_json` and `Value::to_json_pretty`.
//!
//! Expected strings are literals taken from JavaScript's `JSON.stringify`
//! (the spec for what excalidraw.com writes), never recomputed by the code.

use pkm_drawing::json::{Value, parse};

/// Parses `input` and serializes it compactly again.
fn round_trip(input: &str) -> String {
    parse(input).expect("valid JSON").to_json()
}

// --- Key order ------------------------------------------------------------

#[test]
fn object_keys_keep_insertion_order() {
    let input = r#"{"z":1,"a":2,"m":{"y":true,"b":false},"10":3,"2":4}"#;
    assert_eq!(round_trip(input), input);
}

#[test]
fn duplicate_key_keeps_first_position_and_last_value_like_javascript() {
    // JSON.stringify(JSON.parse('{"a":1,"b":2,"a":3}')) === '{"a":3,"b":2}'
    assert_eq!(round_trip(r#"{"a":1,"b":2,"a":3}"#), r#"{"a":3,"b":2}"#);
}

#[test]
fn object_lookup_finds_values_by_key() {
    let value = parse(r#"{"type":"rectangle","x":1.5,"locked":false,"tags":[null]}"#).unwrap();
    assert_eq!(value.get("type").and_then(Value::as_str), Some("rectangle"));
    assert_eq!(value.get("x").and_then(Value::as_f64), Some(1.5));
    assert_eq!(value.get("locked").and_then(Value::as_bool), Some(false));
    assert_eq!(
        value
            .get("tags")
            .and_then(Value::as_array)
            .map(<[Value]>::len),
        Some(1)
    );
    assert_eq!(value.get("missing"), None);
}

// --- Numbers --------------------------------------------------------------

#[test]
fn numbers_serialize_like_javascript() {
    // (input, JSON.stringify(JSON.parse(input)))
    let cases = [
        ("0", "0"),
        ("-0", "0"),
        ("1.0", "1"),
        ("100", "100"),
        ("1E2", "100"),
        ("-12.50", "-12.5"),
        ("4.35", "4.35"),
        ("0.30000000000000004", "0.30000000000000004"),
        ("0.1", "0.1"),
        ("123.45000000000002", "123.45000000000002"),
        ("0.000001", "0.000001"),
        ("0.0000001", "1e-7"),
        ("1.5e-7", "1.5e-7"),
        ("-1.5e-10", "-1.5e-10"),
        ("100000000000000000000", "100000000000000000000"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
        ("1.5e+25", "1.5e+25"),
        ("-1e300", "-1e+300"),
        ("123456789012345678901", "123456789012345680000"),
        ("9007199254740993", "9007199254740992"),
        ("5e-324", "5e-324"),
        ("1.7976931348623157e308", "1.7976931348623157e+308"),
        ("1730380000000", "1730380000000"),
    ];
    for (input, expected) in cases {
        assert_eq!(round_trip(input), expected, "number {input}");
    }
}

#[test]
fn exact_ties_between_shortest_candidates_pick_the_even_digit_like_javascript() {
    // Both candidates are equally close; Rust alone would round up.
    for (input, expected) in [
        ("14611050304372.0625", "14611050304372.062"),
        ("-2045733348428895.25", "-2045733348428895.2"),
        ("1000000000.00390625", "1000000000.0039062"),
    ] {
        assert_eq!(round_trip(input), expected, "number {input}");
    }
}

#[test]
fn numbers_out_of_f64_range_become_null_like_javascript() {
    // JSON.stringify(JSON.parse("1e999")) === "null" (Infinity has no JSON form)
    assert_eq!(round_trip("[1e999,-1e999]"), "[null,null]");
}

// --- Strings --------------------------------------------------------------

#[test]
fn all_simple_escapes_are_decoded() {
    let value = parse(r#""\" \\ \/ \b \f \n \r \t""#).unwrap();
    assert_eq!(value.as_str(), Some("\" \\ / \u{8} \u{c} \n \r \t"));
}

#[test]
fn unicode_escapes_are_decoded() {
    assert_eq!(
        parse(r#""\u0041\u00e9\u65E5""#).unwrap().as_str(),
        Some("Aé日")
    );
}

#[test]
fn surrogate_pair_escapes_combine_into_one_character() {
    assert_eq!(parse(r#""\ud83d\ude00""#).unwrap().as_str(), Some("😀"));
    assert_eq!(
        parse(r#""\uD834\uDD1E""#).unwrap().as_str(),
        Some("\u{1d11e}")
    );
}

#[test]
fn raw_unicode_passes_through_unescaped() {
    let input = "\"Héllo 日本語 😀 \u{1d11e} \u{2028}\"";
    assert_eq!(round_trip(input), input);
}

#[test]
fn serializer_escapes_like_json_stringify() {
    // JSON.stringify("\"\\/\b\f\n\r\t\u0001\u001f\u007f") — "/" and DEL stay raw.
    let value = parse(r#""\"\\\/\b\f\n\r\t\u0001\u001F\u007f""#).unwrap();
    assert_eq!(
        value.to_json(),
        "\"\\\"\\\\/\\b\\f\\n\\r\\t\\u0001\\u001f\u{7f}\""
    );
}

#[test]
fn object_keys_are_escaped_like_strings() {
    let input = r#"{"a\"b\n":1,"日本":2}"#;
    assert_eq!(round_trip(input), input);
}

// --- Whole values & formatting -------------------------------------------

#[test]
fn every_value_kind_round_trips() {
    let input = r#"{"n":null,"t":true,"f":false,"i":-3,"s":"x","a":[1,[2,[]],{}],"o":{"k":{}}}"#;
    assert_eq!(round_trip(input), input);
}

#[test]
fn insignificant_whitespace_is_dropped_in_compact_output() {
    assert_eq!(
        round_trip(" \t\r\n{ \"a\" :\n[ 1 , 2 ] }\n"),
        r#"{"a":[1,2]}"#
    );
}

#[test]
fn pretty_output_matches_json_stringify_with_indent_2() {
    // JSON.stringify({a:[1,{b:null}],c:{},d:[],e:"x"}, null, 2)
    let expected = "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": {},\n  \"d\": [],\n  \"e\": \"x\"\n}";
    let value = parse(r#"{"a":[1,{"b":null}],"c":{},"d":[],"e":"x"}"#).unwrap();
    assert_eq!(value.to_json_pretty(), expected);
}

#[test]
fn scalar_and_empty_documents_round_trip() {
    for input in ["null", "true", "0", r#""s""#, "[]", "{}"] {
        assert_eq!(round_trip(input), input);
        assert_eq!(parse(input).unwrap().to_json_pretty(), input);
    }
}

// --- Real excalidraw.com export -------------------------------------------

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

// --- Rejection with position ----------------------------------------------

/// Parses invalid input and returns (byte offset, line, column).
fn error_position(input: &str) -> (usize, usize, usize) {
    let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
    assert!(!error.message.is_empty());
    (error.offset, error.line, error.column)
}

#[test]
fn error_reports_offset_line_and_column() {
    assert_eq!(error_position(r#"{"a": }"#), (6, 1, 7));
    assert_eq!(error_position("{\n  \"a\": 1,\n  \"b\" 2\n}"), (18, 3, 7));
}

#[test]
fn error_column_counts_characters_not_bytes() {
    // "é" is two bytes but one column.
    assert_eq!(error_position("[\"é\" 1]"), (6, 1, 6));
}

#[test]
fn error_display_names_the_position_and_the_problem() {
    let error = parse(r#"{"a": }"#).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("line 1, column 7"), "{text}");
    assert!(text.contains(&error.message), "{text}");
}

#[test]
fn syntax_errors_are_rejected_at_the_offending_character() {
    let cases: [(&str, usize); 24] = [
        ("", 0),            // empty input
        ("   ", 3),         // only whitespace
        ("[1,]", 3),        // trailing comma in array
        (r#"{"a":1,}"#, 7), // trailing comma in object
        ("[1 2]", 3),       // missing comma
        (r#"{"a" 1}"#, 5),  // missing colon
        (r#"{a:1}"#, 1),    // unquoted key
        ("{'a':1}", 1),     // single quotes
        ("[1] x", 4),       // trailing data
        ("[1", 2),          // unterminated array
        (r#"{"a":1"#, 6),   // unterminated object
        (r#""abc"#, 4),     // unterminated string
        ("\"a\nb\"", 2),    // raw newline in string
        (r#""\x""#, 2),     // unknown escape
        (r#""\u12G4""#, 5), // bad hex digit
        ("01", 1),          // leading zero
        ("+1", 0),          // plus sign
        (".5", 0),          // missing integer part
        ("1.", 2),          // missing fraction digits
        ("1e", 2),          // missing exponent digits
        ("NaN", 0),         // not JSON
        ("tru", 0),         // truncated literal
        ("// c\n1", 0),     // comments
        ("\u{feff}1", 0),   // BOM
    ];
    for (input, offset) in cases {
        let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
        assert_eq!(error.offset, offset, "offset for {input:?}: {error}");
    }
}

#[test]
fn unpaired_surrogate_escapes_are_rejected() {
    // Rust strings cannot hold lone surrogates; reject rather than corrupt.
    for input in [
        r#""\ud83d""#,
        r#""\ude00""#,
        r#""\ud83dx""#,
        r#""\ud83d\u0041""#,
    ] {
        let error = parse(input).expect_err(&format!("{input:?} must be rejected"));
        assert!(error.message.contains("surrogate"), "{error}");
    }
}

#[test]
fn excessive_nesting_is_rejected_instead_of_overflowing_the_stack() {
    let deep = "[".repeat(100_000);
    let error = parse(&deep).unwrap_err();
    assert!(error.message.contains("nesting"), "{error}");
}

#[test]
fn reasonable_nesting_is_accepted() {
    let input = format!("{}{}", "[".repeat(200), "]".repeat(200));
    assert_eq!(round_trip(&input), input);
}
