//! The golden fixtures (ADR-0004) are what the shape generators will be
//! compared against. These tests guard the fixtures themselves: that they are
//! complete, that every Element has output, and that the output is plausible
//! for its input. They read the fixtures only through the core's own JSON.
//!
//! Expected values are facts about rough.js / perfect-freehand (which set types
//! a fill style yields, that roughness 0 adds no jitter), never recomputed
//! from the fixture.

use crate::common;

use common::{Fixture, Op, Shape};
use excalidraw_native_core::json::Value;

const FIXTURES: [&str; 5] = [
    "shapes",
    "fills",
    "lines-arrows",
    "arrowhead-turns",
    "freedraw",
];

fn str_prop<'a>(element: &'a Value, key: &str) -> &'a str {
    element.get(key).and_then(Value::as_str).unwrap_or("")
}

fn num_prop(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

fn point_count(element: &Value) -> usize {
    element
        .get("points")
        .and_then(Value::as_array)
        .map_or(0, <[Value]>::len)
}

fn has_roundness(element: &Value) -> bool {
    !matches!(element.get("roundness"), None | Some(Value::Null))
}

fn is_filled(element: &Value) -> bool {
    str_prop(element, "backgroundColor") != "transparent"
}

/// True when some Element of `fixture` satisfies `matches`.
fn any(fixture: &Fixture, matches: impl Fn(&Value) -> bool) -> bool {
    fixture.elements().iter().any(matches)
}

/// All set types of an Element's rough shapes.
fn set_kinds(fixture: &Fixture, id: &str) -> Vec<String> {
    fixture
        .shapes(id)
        .into_iter()
        .flat_map(|shape| match shape {
            Shape::Rough { sets, .. } => sets.into_iter().map(|s| s.kind).collect(),
            Shape::Outline(_) => vec![],
        })
        .collect()
}

// --- Fixture integrity ----------------------------------------------------

#[test]
fn fixtures_record_the_upstream_versions_they_came_from() {
    for name in FIXTURES {
        let fixture = Fixture::load(name);
        assert_eq!(
            fixture.upstream("excalidraw"),
            Some("8bcdca4526867433f99fdfcb55a063668db0c9dc"),
            "{name}"
        );
        assert_eq!(fixture.upstream("roughjs"), Some("4.6.4"), "{name}");
        assert_eq!(
            fixture.upstream("perfect-freehand"),
            Some("1.2.0"),
            "{name}"
        );
    }
}

#[test]
fn every_element_has_golden_output_and_nothing_else_does() {
    for name in FIXTURES {
        let fixture = Fixture::load(name);
        let mut input: Vec<&str> = fixture
            .elements()
            .iter()
            .map(|e| str_prop(e, "id"))
            .collect();
        let mut golden = fixture.golden_ids();
        input.sort_unstable();
        golden.sort_unstable();
        assert!(!input.is_empty(), "{name} has no elements");
        assert_eq!(input, golden, "{name}");
    }
}

#[test]
fn every_element_yields_at_least_one_non_empty_shape() {
    for name in FIXTURES {
        let fixture = Fixture::load(name);
        for id in fixture.golden_ids() {
            let shapes = fixture.shapes(id);
            assert!(!shapes.is_empty(), "{name}/{id}");
            for shape in shapes {
                match shape {
                    Shape::Rough { sets, .. } => {
                        assert!(sets.iter().any(|s| !s.ops.is_empty()), "{name}/{id}")
                    }
                    Shape::Outline(points) => assert!(points.len() >= 3, "{name}/{id}"),
                }
            }
        }
    }
}

// --- Coverage -------------------------------------------------------------

#[test]
fn shapes_cover_every_roughness_stroke_style_and_roundness() {
    let fixture = Fixture::load("shapes");
    for ty in ["rectangle", "diamond", "ellipse"] {
        for roughness in [0.0, 1.0, 2.0] {
            for stroke_style in ["solid", "dashed", "dotted"] {
                for round in [false, true] {
                    // Ellipses have no corners to round.
                    if ty == "ellipse" && round {
                        continue;
                    }
                    assert!(
                        any(&fixture, |e| str_prop(e, "type") == ty
                            && num_prop(e, "roughness") == roughness
                            && str_prop(e, "strokeStyle") == stroke_style
                            && has_roundness(e) == round),
                        "missing {ty} roughness {roughness} {stroke_style} round={round}"
                    );
                }
            }
        }
    }
}

#[test]
fn fills_cover_every_fill_style_on_every_closed_shape() {
    let fixture = Fixture::load("fills");
    for ty in ["rectangle", "diamond", "ellipse"] {
        for fill_style in ["hachure", "cross-hatch", "zigzag", "solid"] {
            assert!(
                any(&fixture, |e| str_prop(e, "type") == ty
                    && str_prop(e, "fillStyle") == fill_style
                    && is_filled(e)),
                "missing {fill_style} {ty}"
            );
        }
    }
}

#[test]
fn fill_styles_yield_the_rough_set_types_that_rough_js_gives_them() {
    let fixture = Fixture::load("fills");
    for element in fixture.elements() {
        if !is_filled(element) {
            continue;
        }
        let id = str_prop(element, "id");
        let expected = match str_prop(element, "fillStyle") {
            "solid" => "fillPath",
            _ => "fillSketch",
        };
        assert!(
            set_kinds(&fixture, id).iter().any(|k| k == expected),
            "{id} should contain a {expected} set"
        );
    }
}

#[test]
fn lines_and_arrows_cover_straight_curved_multi_point_and_all_arrowheads() {
    let fixture = Fixture::load("lines-arrows");
    for ty in ["line", "arrow"] {
        let of_type = |e: &Value| str_prop(e, "type") == ty;
        assert!(
            any(&fixture, |e| of_type(e)
                && point_count(e) == 2
                && !has_roundness(e)),
            "straight {ty}"
        );
        assert!(
            any(&fixture, |e| of_type(e)
                && point_count(e) > 2
                && !has_roundness(e)),
            "multi-point {ty}"
        );
        assert!(
            any(&fixture, |e| of_type(e)
                && point_count(e) > 2
                && has_roundness(e)),
            "curved {ty}"
        );
    }

    let arrowheads = [
        "arrow",
        "bar",
        "circle",
        "circle_outline",
        "triangle",
        "triangle_outline",
        "diamond",
        "diamond_outline",
        "cardinality_one",
        "cardinality_many",
        "cardinality_one_or_many",
        "cardinality_exactly_one",
        "cardinality_zero_or_one",
        "cardinality_zero_or_many",
    ];
    for head in arrowheads {
        assert!(
            any(&fixture, |e| str_prop(e, "endArrowhead") == head),
            "end arrowhead {head}"
        );
        assert!(
            any(&fixture, |e| str_prop(e, "startArrowhead") == head),
            "start arrowhead {head}"
        );
    }
}

#[test]
fn freedraw_covers_real_and_simulated_pressure() {
    let fixture = Fixture::load("freedraw");
    for simulated in [true, false] {
        assert!(
            any(&fixture, |e| str_prop(e, "type") == "freedraw"
                && e.get("simulatePressure").and_then(Value::as_bool)
                    == Some(simulated)),
            "simulatePressure={simulated}"
        );
    }
}

// --- Plausibility of the output for its input -----------------------------

#[test]
fn architect_rectangle_has_no_jitter() {
    // roughness 0 means rough.js adds no offsets, so every control point of
    // the outline lies on or inside the rectangle's own box.
    let fixture = Fixture::load("shapes");
    let element = fixture.element("rectangle-r0-solid-sharp");
    let (w, h) = (num_prop(element, "width"), num_prop(element, "height"));

    for shape in fixture.shapes("rectangle-r0-solid-sharp") {
        let Shape::Rough { sets, .. } = shape else {
            panic!("rectangle yields rough shapes only")
        };
        for op in sets.iter().flat_map(|s| &s.ops) {
            for [x, y] in op.points() {
                assert!(
                    (0.0..=w).contains(&x) && (0.0..=h).contains(&y),
                    "({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn rough_shapes_start_each_stroke_with_a_move() {
    let fixture = Fixture::load("shapes");
    for id in fixture.golden_ids() {
        for shape in fixture.shapes(id) {
            let Shape::Rough { sets, .. } = shape else {
                continue;
            };
            for set in sets {
                assert!(matches!(set.ops.first(), Some(Op::Move(_))), "{id}");
            }
        }
    }
}

#[test]
fn only_closed_freedraw_strokes_get_a_fill_shape() {
    let fixture = Fixture::load("freedraw");
    let mut saw_filled = false;
    let mut saw_open = false;
    for element in fixture.elements() {
        let id = str_prop(element, "id");
        let shapes = fixture.shapes(id);
        let rough_count = shapes
            .iter()
            .filter(|s| matches!(s, Shape::Rough { .. }))
            .count();
        // The stroke outline is always last.
        assert!(matches!(shapes.last(), Some(Shape::Outline(_))), "{id}");
        match rough_count {
            0 => saw_open = true,
            1 => saw_filled = true,
            n => panic!("{id}: {n} fill shapes"),
        }
    }
    assert!(
        saw_filled && saw_open,
        "need both a closed and an open stroke"
    );
}
