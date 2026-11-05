//! Hand-drawn rectangles, diamonds and ellipses (ADR-0004), tested through
//! `Editor::geometry`.
//!
//! Parity with excalidraw.com is checked against the golden fixtures, which
//! hold what rough.js produced for the same seeds. The style tests use
//! literals taken from Excalidraw's source (`shape.ts` at the pinned commit),
//! never values recomputed from the code under test.

mod common;

use std::f64::consts::FRAC_PI_2;

use common::{Fixture, assert_rough_elements_match_golden, editor_with_element, ops};
use workingtitle_core::{Editor, Geometry, Part, Verb};

// --- Parity with rough.js ---------------------------------------------------

#[test]
fn shapes_match_the_golden_output_for_every_roughness_stroke_style_and_roundness() {
    let checked = assert_rough_elements_match_golden("shapes", |_| true);

    // Every type x roughness x stroke style x roundness, small and big ones.
    assert_eq!(checked, Fixture::load("shapes").elements().len());
    assert!(checked >= 51, "only {checked} elements checked");
}

#[test]
fn the_same_seed_gives_the_same_strokes_and_another_seed_another_sketch() {
    let drawing = |seed: u32| {
        editor_with_element(&format!(
            r#""id":"a","type":"rectangle","width":100,"height":60,"seed":{seed}"#
        ))
        .geometry()
    };
    let (a, b, c) = (drawing(7), drawing(7), drawing(8));

    assert_eq!(a.coords(), b.coords());
    assert_ne!(a.coords(), c.coords());
}

#[test]
fn seeds_beyond_32_bits_wrap_like_javascripts_imul() {
    // Math.imul converts its operands to int32, so 2^32 + 7 behaves as 7.
    let drawing = |seed: u64| {
        editor_with_element(&format!(
            r#""id":"a","type":"ellipse","width":100,"height":60,"seed":{seed}"#
        ))
        .geometry()
    };

    assert_eq!(drawing(7).coords(), drawing((1 << 32) + 7).coords());
}

// --- Style ------------------------------------------------------------------

fn only_stroke(editor: &Editor) -> (Geometry, Part) {
    let geometry = editor.geometry();
    let [part] = geometry.parts() else {
        panic!("one part")
    };
    let part = *part;
    (geometry, part)
}

#[test]
fn solid_strokes_use_the_elements_width_and_colour() {
    let editor = editor_with_element(
        r##""id":"a","type":"rectangle","width":100,"height":60,"strokeWidth":4,"strokeColor":"#e03131""##,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.width, 4.0);
    assert_eq!(part.color, 0xe031_31ff);
    assert_eq!((part.dash_on, part.dash_off), (0.0, 0.0));
}

#[test]
fn css_colour_names_and_functions_draw_in_that_colour() {
    // Values from tinycolor2 1.6.0 (`toHex8`), the reader of Excalidraw's colour input.
    let editor = editor_with_element(
        r#""id":"a","type":"rectangle","width":100,"height":60,"strokeColor":"rebeccapurple","backgroundColor":"rgb(255, 0, 0)","fillStyle":"solid""#,
    );
    let geometry = editor.geometry();
    let colours: Vec<u32> = geometry.parts().iter().map(|part| part.color).collect();

    assert!(colours.contains(&0x6633_99ff), "stroke: {colours:x?}");
    assert!(colours.contains(&0xff00_00ff), "fill: {colours:x?}");
}

#[test]
fn an_unknown_colour_string_draws_in_the_default_ink() {
    let editor = editor_with_element(
        r#""id":"a","type":"rectangle","width":100,"height":60,"strokeColor":"not a colour""#,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.color, 0x1e1e_1eff);
}

#[test]
fn dashed_strokes_are_slightly_wider_with_8_on_and_8_plus_width_off() {
    // shape.ts: dash [8, 8 + strokeWidth], strokeWidth + 0.5.
    let editor = editor_with_element(
        r#""id":"a","type":"rectangle","width":100,"height":60,"strokeStyle":"dashed""#,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.width, 2.5);
    assert_eq!((part.dash_on, part.dash_off), (8.0, 10.0));
}

#[test]
fn dotted_strokes_are_slightly_wider_with_1_5_on_and_6_plus_width_off() {
    // shape.ts: dash [1.5, 6 + strokeWidth], strokeWidth + 0.5.
    let editor = editor_with_element(
        r#""id":"a","type":"ellipse","width":100,"height":60,"strokeStyle":"dotted""#,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.width, 2.5);
    assert_eq!((part.dash_on, part.dash_off), (1.5, 8.0));
}

#[test]
fn opacity_and_version_come_from_the_element() {
    let editor = editor_with_element(
        r#""id":"a","type":"diamond","width":100,"height":60,"opacity":30,"version":12"#,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.opacity, 0.3);
    assert_eq!(part.version, 12);
}

#[test]
fn a_transparent_stroke_colour_is_invisible() {
    let editor = editor_with_element(
        r#""id":"a","type":"rectangle","width":100,"height":60,"strokeColor":"transparent""#,
    );
    let (_, part) = only_stroke(&editor);

    assert_eq!(part.color & 0xff, 0);
}

#[test]
fn architect_sloppiness_draws_exact_straight_edges() {
    // roughness 0: the box of the rectangle, every stroke exactly on its edge.
    let editor = editor_with_element(
        r#""id":"a","type":"rectangle","x":10,"y":20,"width":100,"height":60,"roughness":0"#,
    );
    let (geometry, part) = only_stroke(&editor);

    for (_, p) in ops(&geometry, &part) {
        for xy in p.chunks(2) {
            assert!((10.0..=110.0).contains(&xy[0]) && (20.0..=80.0).contains(&xy[1]));
        }
    }
    // First op: the top left corner.
    assert_eq!(ops(&geometry, &part)[0], (Verb::Move, vec![10.0, 20.0]));
}

// --- Placement --------------------------------------------------------------

#[test]
fn a_rotated_element_turns_about_its_centre() {
    // Centre (50, 30) before moving by (x, y) = (0, 0). A quarter turn
    // clockwise (y points down) takes the architect rectangle's corner (0, 0)
    // to (80, -20): the offset (-50, -30) becomes (30, -50).
    let editor = editor_with_element(&format!(
        r#""id":"a","type":"rectangle","width":100,"height":60,"roughness":0,"angle":{FRAC_PI_2}"#
    ));
    let (geometry, part) = only_stroke(&editor);

    let (verb, first) = &ops(&geometry, &part)[0];
    assert_eq!(*verb, Verb::Move);
    assert!((first[0] - 80.0).abs() < 1e-3 && (first[1] + 20.0).abs() < 1e-3);
}

// --- Client caching ---------------------------------------------------------

#[test]
fn geometry_says_when_handles_stop_meaning_the_same_element() {
    // Clients cache per (element handle, version). Handles are positions in
    // the file, so loading another Drawing makes every cached entry wrong.
    let drawing = r#"{"type":"excalidraw","version":2,"elements":[
        {"id":"a","type":"rectangle","width":10,"height":10}]}"#;
    let mut editor = Editor::new();
    editor.load(drawing.as_bytes()).unwrap();

    let before = editor.geometry().generation();
    assert_eq!(editor.geometry().generation(), before, "unchanged scene");

    editor.load(drawing.as_bytes()).unwrap();
    assert_ne!(editor.geometry().generation(), before, "loaded again");
    assert_ne!(
        Editor::new().geometry().generation(),
        editor.geometry().generation(),
        "other editors never share one"
    );
}
