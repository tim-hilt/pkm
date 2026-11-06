//! Freedraw strokes (ADR-0004), tested through `Editor::geometry`.
//!
//! Parity with excalidraw.com is checked against the golden fixtures: the
//! outline perfect-freehand computes for a stroke must come out as the one
//! filled shape Excalidraw draws through it, and the fill of a closed stroke
//! as the rough.js curve Excalidraw sketches. The remaining tests use
//! literals worked out by hand from the sources of Excalidraw and
//! perfect-freehand.

use crate::common;

use common::{
    Fixture, NON_ZERO, Shape, assert_part_matches_set, editor_with_element, editor_with_fixture,
    num, opaque, ops, parts_of, text,
};
use excalidraw_native_core::json::Value;
use excalidraw_native_core::{Geometry, Part, PartKind, Verb};

/// Checks every freedraw Element of the `freedraw` fixture that `keep`
/// accepts against Excalidraw's output. Returns how many were checked.
fn assert_strokes_match_golden(keep: impl Fn(&Value) -> bool) -> usize {
    let editor = editor_with_fixture("freedraw");
    let fixture = Fixture::load("freedraw");
    let geometry = editor.geometry();
    let mut checked = 0;

    for (handle, element) in fixture.elements().iter().enumerate() {
        if !keep(element) {
            continue;
        }
        let id = text(element, "id");
        let position = (num(element, "x"), num(element, "y"));
        let mut shapes = fixture.shapes(id);
        let Some(Shape::Outline(outline)) = shapes.pop() else {
            panic!("{id}: the outline is last")
        };

        let parts = parts_of(&geometry, handle);
        let (stroke, fills) = parts.split_last().expect("a freedraw is drawn");
        assert_eq!(fills.len(), shapes.len(), "{id}: one fill per rough shape");
        let mut fills = fills.iter();
        for shape in &shapes {
            let Shape::Rough {
                shape,
                options,
                sets,
            } = shape
            else {
                panic!("{id}: only the outline is not rough")
            };
            assert_eq!(sets.len(), 1, "{id}: one set per fill");
            let part = fills.next().unwrap();
            assert_part_matches_set(&geometry, part, &sets[0], (shape, options), position, id);
        }

        assert_outline_part(&geometry, stroke, &outline, position, element);
        checked += 1;
    }
    checked
}

/// Checks that `part` fills the shape Excalidraw makes of `outline` (see
/// `getSvgPathFromStroke`): curves that start at the first point, bend
/// towards each point and end halfway to the next, then close the shape.
fn assert_outline_part(
    geometry: &Geometry,
    part: &Part,
    outline: &[[f64; 2]],
    (x, y): (f64, f64),
    element: &Value,
) {
    let id = text(element, "id");
    assert_eq!(
        part.kind,
        PartKind::Fill as u32,
        "{id}: the stroke is a fill"
    );
    assert_eq!(part.fill_rule, NON_ZERO, "{id}: fill rule");
    assert_eq!(part.color, opaque(text(element, "strokeColor")), "{id}");

    let n = outline.len();
    let at = |i: usize| (outline[i % n][0] + x, outline[i % n][1] + y);
    let middle = |i: usize| {
        let (a, b) = (at(i), at(i + 1));
        ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
    };
    // A curve with one control point `c` from `from` to `to`, as a cubic.
    let cubic = |from: (f64, f64), c: (f64, f64), to: (f64, f64)| {
        [
            from.0 + 2.0 * (c.0 - from.0) / 3.0,
            from.1 + 2.0 * (c.1 - from.1) / 3.0,
            to.0 + 2.0 * (c.0 - to.0) / 3.0,
            to.1 + 2.0 * (c.1 - to.1) / 3.0,
            to.0,
            to.1,
        ]
    };

    let mut expected = vec![(Verb::Move, vec![at(0).0, at(0).1])];
    let mut from = at(0);
    for i in 0..n {
        let to = middle(i);
        expected.push((Verb::Cubic, cubic(from, at(i), to).to_vec()));
        from = to;
    }
    expected.push((Verb::Line, vec![at(0).0, at(0).1]));

    let actual = ops(geometry, part);
    assert_eq!(actual.len(), expected.len(), "{id}: number of ops");
    for (i, ((verb, got), (want_verb, want))) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(verb, want_verb, "{id}: op {i}");
        for (k, (g, w)) in got.iter().zip(want).enumerate() {
            assert!(
                (f64::from(*g) - w).abs() < common::TOLERANCE,
                "{id}: op {i} operand {k}: {g} vs {w}"
            );
        }
    }
}

fn simulates_pressure(simulated: bool) -> impl Fn(&Value) -> bool {
    move |element| element.get("simulatePressure").and_then(Value::as_bool) == Some(simulated)
}

// --- Parity with perfect-freehand and rough.js ------------------------------

#[test]
fn strokes_with_simulated_pressure_match_the_golden_output() {
    let checked = assert_strokes_match_golden(simulates_pressure(true));

    assert!(checked >= 6, "only {checked} strokes checked");
}

#[test]
fn strokes_with_real_pressure_match_the_golden_output() {
    let checked = assert_strokes_match_golden(simulates_pressure(false));

    assert!(checked >= 4, "only {checked} strokes checked");
}

#[test]
fn every_golden_stroke_is_checked() {
    let checked = assert_strokes_match_golden(|_| true);

    assert_eq!(checked, Fixture::load("freedraw").elements().len());
}

// --- Placement and style ----------------------------------------------------

/// A horizontal stroke from (10, 20) to (30, 20), twice as wide as its path.
const STROKE: &str = r#""id":"a","type":"freedraw","x":10,"y":20,"width":20,"height":0,
    "points":[[0,0],[10,0],[20,0]],"pressures":[],"simulatePressure":true"#;

fn outline_coords(editor: &excalidraw_native_core::Editor) -> Vec<f32> {
    let geometry = editor.geometry();
    let [part] = geometry.parts() else {
        panic!("an open stroke is one part")
    };
    ops(&geometry, part)
        .into_iter()
        .flat_map(|(_, operands)| operands)
        .collect()
}

#[test]
fn a_stroke_turns_about_the_middle_of_its_points() {
    let upright = outline_coords(&editor_with_element(STROKE));
    let turned = outline_coords(&editor_with_element(&format!(
        r#"{STROKE},"angle":{}"#,
        std::f64::consts::PI
    )));

    // Half a turn about (20, 20), the middle of the points, mirrors every
    // coordinate there.
    assert_eq!(upright.len(), turned.len());
    for (pair, mirrored) in upright.chunks(2).zip(turned.chunks(2)) {
        assert!((pair[0] + mirrored[0] - 40.0).abs() < 1e-3, "x");
        assert!((pair[1] + mirrored[1] - 40.0).abs() < 1e-3, "y");
    }
}

#[test]
fn a_stroke_is_as_opaque_as_its_element() {
    let editor = editor_with_element(&format!(r#"{STROKE},"opacity":40"#));
    let geometry = editor.geometry();

    assert_eq!(geometry.parts()[0].opacity, 0.4);
}

#[test]
fn a_stroke_is_drawn_in_its_stroke_colour_whatever_the_background() {
    let editor = editor_with_element(&format!(
        r##"{STROKE},"strokeColor":"#e03131","backgroundColor":"#a5d8ff""##
    ));
    let geometry = editor.geometry();

    // The path is open, so there is nothing to fill with the background.
    let [part] = geometry.parts() else {
        panic!("one part")
    };
    assert_eq!(part.color, opaque("#e03131"));
}

#[test]
fn a_thicker_stroke_reaches_further_from_its_points() {
    let reach = |width: f64| {
        let editor = editor_with_element(&format!(r#"{STROKE},"strokeWidth":{width}"#));
        // Distance of the outline from the line y = 20 through the points.
        outline_coords(&editor)
            .chunks(2)
            .map(|p| (f64::from(p[1]) - 20.0).abs())
            .fold(0.0, f64::max)
    };

    // The ink reaches at most as far as a stroke is wide: 4.25 times the
    // Element's stroke width, and it scales with it.
    assert!(reach(1.0) <= 4.25 + 1e-3);
    assert!(reach(4.0) <= 17.0 + 1e-3);
    assert!(reach(4.0) > 3.0 * reach(1.0));
}

#[test]
fn a_stroke_without_points_draws_nothing() {
    let editor = editor_with_element(
        r#""id":"a","type":"freedraw","x":0,"y":0,"width":0,"height":0,
        "points":[],"pressures":[],"simulatePressure":true"#,
    );

    assert!(editor.geometry().parts().is_empty());
}

#[test]
fn a_stroke_from_excalidraw_com_with_missing_members_still_draws() {
    // Old files lack strokeOptions and may lack simulatePressure; Excalidraw
    // then draws with the pressures of the file.
    let editor = editor_with_element(
        r#""id":"a","type":"freedraw","x":0,"y":0,"width":10,"height":5,
        "points":[[0,0],[5,5],[10,0]],"pressures":[0.2,0.8,0.4]"#,
    );
    let geometry = editor.geometry();

    let [part] = geometry.parts() else {
        panic!("one part")
    };
    assert_eq!(part.kind, PartKind::Fill as u32);
    assert!(part.verb_count > 10);
}

#[test]
fn a_loop_is_filled_before_its_stroke_is_drawn() {
    let editor = editor_with_element(
        r##""id":"a","type":"freedraw","x":0,"y":0,"width":40,"height":40,
        "backgroundColor":"#ffc9c9","fillStyle":"solid","roughness":0,
        "points":[[0,0],[40,0],[40,40],[0,40],[0,0]],"pressures":[],"simulatePressure":true"##,
    );
    let geometry = editor.geometry();

    let kinds_and_colours: Vec<_> = geometry.parts().iter().map(|p| (p.kind, p.color)).collect();
    assert_eq!(
        kinds_and_colours,
        [
            (PartKind::Fill as u32, opaque("#ffc9c9")),
            (PartKind::Fill as u32, opaque("#1e1e1e")),
        ]
    );
}

// --- Constant width ---------------------------------------------------------

fn is_constant_width(element: &Value) -> bool {
    element
        .get("strokeOptions")
        .and_then(Value::as_object)
        .and_then(|options| options.get("variability"))
        .and_then(Value::as_str)
        == Some("constant")
}

#[test]
fn strokes_of_constant_width_match_the_golden_output() {
    // Straight, wavy, single point, thick, two points and a sharp corner.
    let checked = assert_strokes_match_golden(is_constant_width);

    assert!(checked >= 6, "only {checked} strokes checked");
}

/// A horizontal stroke through (10, 20) to (30, 20) of constant width.
const CONSTANT_STROKE: &str = r#""id":"a","type":"freedraw","x":10,"y":20,"width":20,"height":0,
    "points":[[0,0],[10,0],[20,0]],"pressures":[],"simulatePressure":true,
    "strokeOptions":{"variability":"constant","streamline":0.5}"#;

#[test]
fn a_stroke_of_constant_width_reaches_one_and_four_tenths_of_its_stroke_width() {
    let editor = editor_with_element(&format!(r#"{CONSTANT_STROKE},"strokeWidth":4"#));

    let reach = outline_coords(&editor)
        .chunks(2)
        .map(|p| (f64::from(p[1]) - 20.0).abs())
        .fold(0.0, f64::max);

    assert!((reach - 5.6).abs() < 1e-2, "reach {reach}");
}
