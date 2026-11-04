//! Fills of rectangles, diamonds and ellipses (ADR-0004), tested through
//! `Editor::geometry`.
//!
//! Parity with excalidraw.com is checked against the golden fixtures: rough.js
//! gives every shape a list of op sets (the fill, then the outline), and the
//! core must emit one part for each, in that order. The style tests use
//! literals taken from Excalidraw's and rough.js' sources, never values
//! recomputed from the code under test.

mod common;

use common::{assert_rough_elements_match_golden, editor_with_element, ops};
use workingtitle_core::{Geometry, PartKind, Verb};

// --- Parity with rough.js ---------------------------------------------------

#[test]
fn every_fill_style_matches_the_golden_output_for_every_shape_roughness_and_stroke_width() {
    let checked = assert_rough_elements_match_golden("fills", |_| true);
    // 3 shapes x 4 fill styles x 3 roughnesses, rounded shapes, stroke widths.
    assert!(checked >= 44, "only {checked} elements checked");
}

// --- Style ------------------------------------------------------------------

fn kinds(geometry: &Geometry) -> Vec<u32> {
    geometry.parts().iter().map(|p| p.kind).collect()
}

const SHAPE: &str =
    r##""id":"a","type":"rectangle","width":100,"height":60,"seed":5,"backgroundColor":"#a5d8ff""##;

#[test]
fn an_element_without_a_background_colour_has_no_fill() {
    for style in ["solid", "hachure", "cross-hatch", "zigzag"] {
        let editor = editor_with_element(&format!(
            r#""id":"a","type":"ellipse","width":100,"height":60,"fillStyle":"{style}","backgroundColor":"transparent""#
        ));

        assert_eq!(
            kinds(&editor.geometry()),
            [PartKind::Stroke as u32],
            "{style}"
        );
    }
}

#[test]
fn a_fully_transparent_background_colour_has_no_fill_either() {
    let editor = editor_with_element(
        r##""id":"a","type":"rectangle","width":100,"height":60,"backgroundColor":"#a5d8ff00""##,
    );

    assert_eq!(kinds(&editor.geometry()), [PartKind::Stroke as u32]);
}

#[test]
fn the_fill_is_drawn_first_so_the_outline_lies_over_it() {
    for (style, fill_kind) in [
        ("solid", PartKind::Fill),
        ("hachure", PartKind::Stroke),
        ("cross-hatch", PartKind::Stroke),
        ("zigzag", PartKind::Stroke),
    ] {
        let editor = editor_with_element(&format!(r#"{SHAPE},"fillStyle":"{style}""#));
        let geometry = editor.geometry();

        let [fill, outline] = geometry.parts() else {
            panic!("{style}: a fill and an outline")
        };
        assert_eq!(fill.kind, fill_kind as u32, "{style}");
        assert_eq!(outline.kind, PartKind::Stroke as u32, "{style}");
        assert_eq!(outline.color, 0x1e1e_1eff, "{style}: stroke colour");
        assert_eq!(fill.color, 0xa5d8_ffff, "{style}: background colour");
    }
}

#[test]
fn background_colour_with_alpha_keeps_its_alpha() {
    let editor = editor_with_element(&format!(
        r##"{SHAPE},"fillStyle":"solid","backgroundColor":"#a5d8ff80""##
    ));

    assert_eq!(editor.geometry().parts()[0].color, 0xa5d8_ff80);
}

#[test]
fn fill_and_outline_share_the_elements_opacity_and_version() {
    let editor = editor_with_element(&format!(
        r#"{SHAPE},"fillStyle":"solid","opacity":40,"version":7"#
    ));
    let geometry = editor.geometry();

    for part in geometry.parts() {
        assert_eq!(part.opacity, 0.4);
        assert_eq!(part.version, 7);
        assert_eq!(part.element, 0);
    }
}

#[test]
fn hatch_lines_are_half_as_thick_as_the_stroke_and_never_dashed() {
    // shape.ts: fillWeight is strokeWidth / 2, whatever the stroke style.
    let editor = editor_with_element(&format!(
        r#"{SHAPE},"fillStyle":"hachure","strokeWidth":4,"strokeStyle":"dashed""#
    ));
    let geometry = editor.geometry();

    let hatch = geometry.parts()[0];
    assert_eq!(hatch.width, 2.0);
    assert_eq!((hatch.dash_on, hatch.dash_off), (0.0, 0.0));
    // The outline still gets its dashes and the extra half unit.
    let outline = geometry.parts()[1];
    assert_eq!(outline.width, 4.5);
    assert_eq!((outline.dash_on, outline.dash_off), (8.0, 12.0));
}

#[test]
fn hatch_lines_are_four_stroke_widths_apart() {
    // shape.ts: hachureGap is strokeWidth * 4. Architect lines on a 100 x 60
    // rectangle at -41 degrees: neighbouring lines start 8 units apart
    // measured across the lines, so there are about 100 / 8 of them per unit
    // length of the slanted direction; thicker strokes thin them out.
    let lines = |stroke_width: u32| {
        let editor = editor_with_element(&format!(
            r#"{SHAPE},"roughness":0,"fillStyle":"hachure","strokeWidth":{stroke_width}"#
        ));
        let geometry = editor.geometry();
        let hatch = geometry.parts()[0];
        // Every line is drawn twice: a move and a curve each.
        hatch.verb_count / 4
    };

    let (thin, thick) = (lines(1), lines(4));
    assert!(
        thin > 3 * thick,
        "{thin} lines at width 1, {thick} at width 4"
    );
}

#[test]
fn a_rotated_element_turns_its_fill_with_it() {
    // A quarter turn about the centre (50, 30) takes the architect rectangle's
    // top left corner (0, 0) to (80, -20); the solid fill starts there too.
    let editor = editor_with_element(&format!(
        r#"{SHAPE},"roughness":0,"fillStyle":"solid","angle":{}"#,
        std::f64::consts::FRAC_PI_2
    ));
    let geometry = editor.geometry();

    let (verb, first) = &ops(&geometry, &geometry.parts()[0])[0];
    assert_eq!(*verb, Verb::Move);
    assert!((first[0] - 80.0).abs() < 1e-3 && (first[1] + 20.0).abs() < 1e-3);
}

#[test]
fn a_solid_fill_is_one_closed_shape_through_the_corners() {
    // Architect rectangle: the fill goes through all four corners, in order.
    let editor = editor_with_element(&format!(
        r#"{SHAPE},"roughness":0,"fillStyle":"solid","x":10,"y":20"#
    ));
    let geometry = editor.geometry();

    assert_eq!(
        ops(&geometry, &geometry.parts()[0]),
        [
            (Verb::Move, vec![10.0, 20.0]),
            (Verb::Line, vec![110.0, 20.0]),
            (Verb::Line, vec![110.0, 80.0]),
            (Verb::Line, vec![10.0, 80.0]),
        ]
    );
}

#[test]
fn hatching_stays_inside_the_shape() {
    // Rough lines stray a few units over the edge, no further.
    for pattern in ["hachure", "cross-hatch", "zigzag"] {
        let editor =
            editor_with_element(&format!(r#"{SHAPE},"fillStyle":"{pattern}","x":10,"y":20"#));
        let geometry = editor.geometry();

        for (_, operands) in ops(&geometry, &geometry.parts()[0]) {
            for xy in operands.chunks(2) {
                assert!(
                    (5.0..=115.0).contains(&xy[0]) && (15.0..=85.0).contains(&xy[1]),
                    "{pattern}: ({}, {})",
                    xy[0],
                    xy[1]
                );
            }
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_fill() {
    let drawing = |seed: u32| {
        editor_with_element(&format!(
            r#"{SHAPE},"fillStyle":"cross-hatch","seed":{seed}"#
        ))
        .geometry()
    };
    let (a, b, c) = (drawing(3), drawing(3), drawing(4));

    assert_eq!(a.coords(), b.coords());
    assert_ne!(a.coords(), c.coords());
}

#[test]
fn a_fill_does_not_change_the_outline() {
    // rough.js sketches the outline first, so the fill cannot move it.
    let outline_of = |extra: &str| {
        let editor = editor_with_element(&format!(r#"{SHAPE}{extra}"#));
        let geometry = editor.geometry();
        let last = *geometry.parts().last().unwrap();
        ops(&geometry, &last)
    };

    let plain = outline_of(r#","backgroundColor":"transparent""#);
    for style in ["solid", "hachure", "cross-hatch", "zigzag"] {
        assert_eq!(
            plain,
            outline_of(&format!(r#","fillStyle":"{style}""#)),
            "{style}"
        );
    }
}
