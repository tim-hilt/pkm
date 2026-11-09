//! Lines and arrows (ADR-0004), tested through `Editor::geometry`.
//!
//! Parity with excalidraw.com is checked against the golden fixtures: rough.js
//! gives every Element a list of drawables, each a list of op sets (the fill,
//! then the outline), and the core must emit one part for each op set, in that
//! order. The remaining tests use literals taken from Excalidraw's and
//! rough.js' sources, never values recomputed from the code under test.

use crate::common;

use common::{assert_rough_elements_match_golden, editor_with_element, ops, text};
use pkm_drawing::json::Value;
use pkm_drawing::{PartKind, Verb};

fn is_a(kind: &'static str) -> impl Fn(&Value) -> bool {
    move |element| text(element, "type") == kind
}

// --- Parity with rough.js ---------------------------------------------------

#[test]
fn lines_match_the_golden_output_straight_multi_point_curved_and_closed() {
    let checked = assert_rough_elements_match_golden("lines-arrows", is_a("line"));

    assert!(checked >= 12, "only {checked} lines checked");
}

#[test]
fn arrows_match_the_golden_output_with_every_arrowhead_at_either_end() {
    let checked = assert_rough_elements_match_golden("lines-arrows", is_a("arrow"));

    assert!(checked >= 45, "only {checked} arrows checked");
}

#[test]
fn arrowheads_shrink_and_keep_clear_of_a_turn_on_a_short_last_segment() {
    let checked = assert_rough_elements_match_golden("arrowhead-turns", is_a("arrow"));

    assert_eq!(checked, 42);
}

#[test]
fn a_line_is_drawn_as_its_points_with_nothing_else() {
    // Architect line: exact strokes, drawn twice, between the points.
    let editor = editor_with_element(
        r#""id":"a","type":"line","x":10,"y":20,"width":50,"height":30,"roughness":0,
        "points":[[0,0],[50,30]]"#,
    );
    let geometry = editor.geometry();

    let [outline] = geometry.parts() else {
        panic!("a plain line is one stroke")
    };
    assert_eq!(outline.kind, PartKind::Stroke as u32);
    let verbs: Vec<_> = ops(&geometry, outline).iter().map(|(v, _)| *v).collect();
    assert_eq!(verbs, [Verb::Move, Verb::Cubic, Verb::Move, Verb::Cubic]);
}

// --- Placement and style ----------------------------------------------------

/// Architect (roughness 0) arrows have exact strokes.
const ARROW: &str = r#""id":"a","type":"arrow","x":10,"y":20,"width":100,"height":0,
    "roughness":0,"points":[[0,0],[100,0]]"#;

fn first_move(geometry: &pkm_drawing::Geometry, part: usize) -> (f32, f32) {
    let (verb, operands) = &ops(geometry, &geometry.parts()[part])[0];
    assert_eq!(*verb, Verb::Move);
    (operands[0], operands[1])
}

#[test]
fn a_rotated_line_turns_about_the_middle_of_its_stroke() {
    // A quarter turn about the middle (50, 0) of the line from (0, 0) to
    // (100, 0) takes its start to (50, -50).
    let editor = editor_with_element(&format!(
        r#""id":"a","type":"line","width":100,"height":0,"roughness":0,
        "points":[[0,0],[100,0]],"angle":{}"#,
        std::f64::consts::FRAC_PI_2
    ));
    let geometry = editor.geometry();

    let (x, y) = first_move(&geometry, 0);
    assert!(
        (x - 50.0).abs() < 1e-3 && (y + 50.0).abs() < 1e-3,
        "({x}, {y})"
    );
}

#[test]
fn a_rotated_arrow_turns_its_arrowhead_with_it() {
    let editor = editor_with_element(&format!(
        r#"{ARROW},"endArrowhead":"bar","angle":{}"#,
        std::f64::consts::PI
    ));
    let geometry = editor.geometry();

    // Half a turn about the middle (60, 20) takes the start (10, 20) to
    // (110, 20); the bar stands across the other end, now at x = 10.
    let (x, y) = first_move(&geometry, 0);
    assert!(
        (x - 110.0).abs() < 1e-3 && (y - 20.0).abs() < 1e-3,
        "({x}, {y})"
    );
    for part in 1..geometry.parts().len() {
        let (x, _) = first_move(&geometry, part);
        assert!((x - 10.0).abs() < 1.0, "bar at x = {x}");
    }
}

#[test]
fn an_arrow_has_an_arrowhead_at_its_end_unless_the_file_says_otherwise() {
    let count = |extra: &str| {
        editor_with_element(&format!("{ARROW}{extra}"))
            .geometry()
            .parts()
            .len()
    };

    // The line, and the two strokes of an open arrowhead.
    assert_eq!(count(""), 3);
    assert_eq!(count(r#","endArrowhead":null"#), 1);
    assert_eq!(count(r#","startArrowhead":"arrow""#), 5);
    // A line has no arrowheads, whatever the file says.
    let line = r#""id":"a","type":"line","width":100,"height":0,"points":[[0,0],[100,0]],
        "endArrowhead":"arrow""#;
    assert_eq!(editor_with_element(line).geometry().parts().len(), 1);
}

#[test]
fn excalidraws_older_arrowhead_names_still_work() {
    let parts = |name: &str| {
        editor_with_element(&format!(r#"{ARROW},"endArrowhead":"{name}""#))
            .geometry()
            .parts()
            .len()
    };

    // "dot" is the circle (a filled outline), "crowfoot_one" the bar.
    assert_eq!(parts("dot"), parts("circle"));
    assert_eq!(parts("crowfoot_one"), parts("cardinality_one"));
    assert_eq!(parts("crowfoot_many"), parts("cardinality_many"));
}

#[test]
fn an_unknown_arrowhead_is_drawn_as_a_small_open_arrowhead() {
    let parts =
        |name: &str| editor_with_element(&format!(r#"{ARROW},"endArrowhead":"{name}""#)).geometry();

    assert_eq!(
        parts("from_the_future").parts().len(),
        parts("bar").parts().len()
    );
}

#[test]
fn outline_arrowheads_are_filled_with_the_colour_of_the_canvas() {
    let drawing = format!(
        r##"{{"type":"excalidraw","version":2,"appState":{{"viewBackgroundColor":"#112233"}},
        "elements":[{{{ARROW},"endArrowhead":"triangle_outline","strokeColor":"#e03131"}}]}}"##
    );
    let mut editor = pkm_drawing::Editor::new();
    editor.load(drawing.as_bytes()).unwrap();
    let geometry = editor.geometry();

    // The line, then the head: its fill and its outline.
    let [_, fill, outline] = geometry.parts() else {
        panic!("a line and a head of two parts")
    };
    assert_eq!(fill.kind, PartKind::Fill as u32);
    assert_eq!(fill.color, 0x1122_33ff);
    assert_eq!(outline.color, 0xe031_31ff);
}

#[test]
fn solid_arrowheads_are_filled_with_the_stroke_colour() {
    let editor = editor_with_element(&format!(
        r##"{ARROW},"endArrowhead":"diamond","strokeColor":"#e03131""##
    ));
    let geometry = editor.geometry();

    let fill = geometry.parts()[1];
    assert_eq!(
        (fill.kind, fill.color),
        (PartKind::Fill as u32, 0xe031_31ff)
    );
}

#[test]
fn every_part_of_an_arrow_shares_its_opacity_and_version() {
    let editor = editor_with_element(&format!(
        r#"{ARROW},"endArrowhead":"circle","startArrowhead":"bar","opacity":40,"version":7"#
    ));
    let geometry = editor.geometry();

    assert!(geometry.parts().len() > 3);
    for part in geometry.parts() {
        assert_eq!((part.opacity, part.version, part.element), (0.4, 7, 0));
    }
}

#[test]
fn an_arrow_without_length_has_no_arrowhead_and_no_invalid_numbers() {
    let editor = editor_with_element(
        r#""id":"a","type":"arrow","x":5,"y":5,"width":0,"height":0,"points":[[0,0],[0,0]],
        "startArrowhead":"triangle","endArrowhead":"circle""#,
    );
    let geometry = editor.geometry();

    assert_eq!(geometry.parts().len(), 1, "only the stroke");
    assert!(geometry.coords().iter().all(|n| n.is_finite()));
}

#[test]
fn a_line_without_points_goes_from_the_origin_to_its_size() {
    let editor = editor_with_element(
        r#""id":"a","type":"line","x":10,"y":20,"width":30,"height":40,"roughness":0"#,
    );
    let geometry = editor.geometry();

    let operations = ops(&geometry, &geometry.parts()[0]);
    assert_eq!(operations[0], (Verb::Move, vec![10.0, 20.0]));
    assert_eq!(operations[1].1[4..], [40.0, 60.0]);
}

#[test]
fn dashes_and_dots_apply_to_the_line_but_not_to_solid_arrowheads() {
    let editor = editor_with_element(&format!(
        r#"{ARROW},"endArrowhead":"triangle","strokeStyle":"dashed""#
    ));
    let geometry = editor.geometry();

    let dashes: Vec<_> = geometry
        .parts()
        .iter()
        .map(|p| (p.dash_on, p.dash_off))
        .collect();
    assert_eq!(dashes, [(8.0, 10.0), (0.0, 0.0), (0.0, 0.0)]);
}

// --- Elbow arrows -----------------------------------------------------------

const ELBOW: &str = r#""id":"a","type":"arrow","x":0,"y":0,"width":80,"height":60,
    "roughness":0,"points":[[0,0],[80,0],[80,60]]"#;

#[test]
fn an_elbow_arrow_is_drawn_through_its_points_without_routing_or_rounding() {
    let elbow = editor_with_element(&format!(
        r#"{ELBOW},"elbowed":true,"roundness":{{"type":2}},"fixedSegments":null,"startIsSpecial":false"#
    ))
    .geometry();
    let sharp = editor_with_element(ELBOW).geometry();

    assert_eq!(elbow.coords(), sharp.coords());
    assert_eq!(elbow.verbs(), sharp.verbs());
    // The corner is a corner: the stroke arrives at (80, 0).
    assert!(
        ops(&elbow, &elbow.parts()[0])
            .iter()
            .any(|(_, operands)| operands.ends_with(&[80.0, 0.0]))
    );
}

#[test]
fn an_elbow_arrow_survives_open_and_save_unchanged() {
    let drawing = r#"{
  "type": "excalidraw",
  "version": 2,
  "elements": [
    {
      "id": "a",
      "type": "arrow",
      "x": 0,
      "y": 0,
      "width": 80,
      "height": 60,
      "points": [
        [
          0,
          0
        ],
        [
          80,
          0
        ],
        [
          80,
          60
        ]
      ],
      "elbowed": true,
      "fixedSegments": [
        {
          "index": 1
        }
      ],
      "startIsSpecial": false,
      "endIsSpecial": true
    }
  ]
}"#;
    let mut editor = pkm_drawing::Editor::new();
    editor.load(drawing.as_bytes()).unwrap();
    let saved = String::from_utf8(editor.save()).unwrap();

    for property in [
        "\"elbowed\": true",
        "\"fixedSegments\": [",
        "\"endIsSpecial\": true",
    ] {
        assert!(saved.contains(property), "{property} was lost");
    }
}
