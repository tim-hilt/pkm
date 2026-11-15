//! What Excalidraw draws for an Element (`shape.ts` at the pinned commit),
//! as geometry: the Element's style turned into rough.js options, its shape
//! into a fill and strokes (ADR-0004), placed in the Scene.

use crate::color::parse_color;
use crate::freedraw;
use crate::geometry::{Dash, FillRule, FillStyle, Geometry, PartWriter, StrokeStyle};
use crate::linear;
use crate::path::{Point, Segment};
use crate::rough::{Drawing, Fill, FillOps, Hatch, Op, Pattern, Sketcher, Style};
use crate::scene::{Element, ElementType, FillPattern, LineStyle, Roundness};

/// Adds the fill and the strokes of `element` to `geometry`. `background` is
/// the colour of the Canvas, which some arrowheads are filled with. Adds
/// nothing for Elements that are not drawn as rough.js or perfect-freehand
/// shapes (text, images, and types the app does not support).
pub(crate) fn draw(element: &Element, background: &str, geometry: &mut Geometry) {
    match element.kind() {
        ElementType::Rectangle | ElementType::Diamond | ElementType::Ellipse => {
            draw_closed(element, geometry);
        }
        ElementType::Line | ElementType::Arrow => {
            linear::draw(element, parse_color(background), geometry);
        }
        ElementType::Freedraw => freedraw::draw(element, geometry),
        _ => {}
    }
}

fn draw_closed(element: &Element, geometry: &mut Geometry) {
    let (width, height) = (element.width(), element.height());
    let mut sketcher = Sketcher::new(element.seed(), style_of(element, (width, height)));
    // rough.js fills rectangles and ellipses by the non-zero rule, and the
    // shapes it makes with a polygon or a path by the even-odd rule.
    let (drawing, rule) = match element.kind() {
        ElementType::Rectangle => match element.roundness() {
            Some(roundness) => (
                sketcher.path(&rounded_rectangle(width, height, roundness)),
                FillRule::EvenOdd,
            ),
            None => (
                sketcher.polygon(&[(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)]),
                FillRule::NonZero,
            ),
        },
        ElementType::Diamond => match element.roundness() {
            Some(roundness) => (
                sketcher.path(&rounded_diamond(width, height, roundness)),
                FillRule::EvenOdd,
            ),
            None => (
                sketcher.polygon(&diamond_corners(width, height)),
                FillRule::EvenOdd,
            ),
        },
        _ => (
            sketcher.ellipse(width / 2.0, height / 2.0, width, height),
            FillRule::NonZero,
        ),
    };

    let paint = Paint {
        stroke: StrokeStyle {
            color: parse_color(element.stroke_color()),
            width: stroke_width(element) as f32,
            opacity: element.opacity() as f32,
            dash: dash_of(element),
        },
        fill_color: parse_color(element.background_color()),
        fill_rule: rule,
        hatch_width: fill_weight(element) as f32,
    };
    emit(
        geometry,
        &Target::of(element, Placement::of(element)),
        &drawing,
        &paint,
    );
}

/// How the parts of one drawable (one rough.js `Drawable`) are painted.
pub(crate) struct Paint {
    /// The outline.
    pub stroke: StrokeStyle,
    /// The colour of the fill, however it is filled.
    pub fill_color: u32,
    pub fill_rule: FillRule,
    /// Hatching is stroked, this wide.
    pub hatch_width: f32,
}

/// Where the parts of an Element go.
pub(crate) struct Target {
    handle: u32,
    version: u32,
    placement: Placement,
}

impl Target {
    pub(crate) fn of(element: &Element, placement: Placement) -> Self {
        Self {
            handle: element.handle(),
            version: element.version(),
            placement,
        }
    }
}

/// Adds the parts for `drawing` to `geometry`: its fill, then its outline.
pub(crate) fn emit(geometry: &mut Geometry, target: &Target, drawing: &Drawing, paint: &Paint) {
    emit_fill(geometry, target, drawing, paint);
    let Target {
        handle,
        version,
        ref placement,
    } = *target;
    write_ops(
        &mut geometry.stroke(handle, version, paint.stroke),
        &drawing.outline,
        placement,
    );
}

/// Adds the part for the fill of `drawing` to `geometry`, if it has one.
pub(crate) fn emit_fill(
    geometry: &mut Geometry,
    target: &Target,
    drawing: &Drawing,
    paint: &Paint,
) {
    let Target {
        handle,
        version,
        ref placement,
    } = *target;
    let opacity = paint.stroke.opacity;
    match &drawing.fill {
        None => {}
        Some(FillOps::Area(ops)) => {
            let style = FillStyle {
                color: paint.fill_color,
                opacity,
                rule: paint.fill_rule,
            };
            write_ops(&mut geometry.fill(handle, version, style), ops, placement);
        }
        Some(FillOps::Lines(ops)) => {
            let style = StrokeStyle {
                color: paint.fill_color,
                width: paint.hatch_width,
                opacity,
                dash: None,
            };
            write_ops(&mut geometry.stroke(handle, version, style), ops, placement);
        }
    }
}

/// Writes `ops`, placed in the Scene, to `path`.
pub(crate) fn write_ops(path: &mut PartWriter, ops: &[Op], placement: &Placement) {
    for &op in ops {
        write(path, op, placement);
    }
}

// --- Options (generateRoughOptions) --------------------------------------

pub(crate) fn style_of(element: &Element, size: (f64, f64)) -> Style {
    style_with_fill(element, size, fill_of(element))
}

/// [`style_of`] with a `fill` that the caller has already worked out, or knows
/// it does not need.
pub(crate) fn style_with_fill(element: &Element, size: (f64, f64), fill: Option<Fill>) -> Style {
    Style {
        roughness: adjusted_roughness(element, size),
        // Rounded corners are one continuous path, which must stay connected;
        // otherwise only the cartoonist level jitters the corners.
        preserve_vertices: element.roundness().is_some() && is_path_shape(element)
            || element.roughness() < 2.0,
        // Dashes and dots would overlay each other if drawn twice.
        multi_stroke: element.line_style() == LineStyle::Solid,
        curve_fitting: if element.kind() == ElementType::Ellipse {
            1.0
        } else {
            0.95
        },
        fill,
    }
}

/// How the background is filled; `None` for a transparent one.
pub(crate) fn fill_of(element: &Element) -> Option<Fill> {
    if parse_color(element.background_color()) & 0xff == 0 {
        return None;
    }
    // Hatch lines are as far apart as four stroke widths, whatever the
    // dashes do to the stroke.
    let hatch = |pattern| {
        Fill::Hatch(Hatch {
            pattern,
            gap: element.stroke_width() * 4.0,
        })
    };
    Some(match element.fill_pattern() {
        FillPattern::Solid => Fill::Solid,
        FillPattern::Hachure => hatch(Pattern::Hachure),
        FillPattern::CrossHatch => hatch(Pattern::CrossHatch),
        FillPattern::Zigzag => hatch(Pattern::Zigzag),
    })
}

/// Hatch lines are half as thick as the stroke, dashes or not.
pub(crate) fn fill_weight(element: &Element) -> f64 {
    element.stroke_width() / 2.0
}

/// Rectangles and diamonds with rounded corners are drawn as a path.
fn is_path_shape(element: &Element) -> bool {
    matches!(
        element.kind(),
        ElementType::Rectangle | ElementType::Diamond
    )
}

/// Small shapes get less jitter, or the sketch would swallow them.
fn adjusted_roughness(element: &Element, (width, height): (f64, f64)) -> f64 {
    let roughness = element.roughness();
    let (max_size, min_size) = (width.max(height), width.min(height));
    // Excalidraw's `canChangeRoundness`: lines too, but not arrows.
    let can_round = is_path_shape(element) || element.kind() == ElementType::Line;

    if (min_size >= 20.0 && max_size >= 50.0)
        || (min_size >= 15.0 && element.roundness().is_some() && can_round)
        // Relatively long lines and arrows.
        || (element.kind().is_linear() && max_size >= 50.0)
    {
        return roughness;
    }
    (roughness / if max_size < 10.0 { 3.0 } else { 2.0 }).min(2.5)
}

/// Dashes and dots are drawn a bit wider to look as heavy as solid lines,
/// which are drawn twice.
pub(crate) fn stroke_width(element: &Element) -> f64 {
    match element.line_style() {
        LineStyle::Solid => element.stroke_width(),
        LineStyle::Dashed | LineStyle::Dotted => element.stroke_width() + 0.5,
    }
}

pub(crate) fn dash_of(element: &Element) -> Option<Dash> {
    let width = element.stroke_width();
    match element.line_style() {
        LineStyle::Solid => None,
        LineStyle::Dashed => Some(Dash {
            on: 8.0,
            off: (8.0 + width) as f32,
        }),
        LineStyle::Dotted => Some(Dash {
            on: 1.5,
            off: (6.0 + width) as f32,
        }),
    }
}

// --- Shapes --------------------------------------------------------------

/// How far corners are rounded off a side of length `side`.
pub(crate) fn corner_radius(side: f64, roundness: Roundness) -> f64 {
    const PROPORTION: f64 = 0.25;
    const DEFAULT_RADIUS: f64 = 32.0;
    match roundness {
        Roundness::Legacy | Roundness::Proportional => side * PROPORTION,
        Roundness::Adaptive { radius } => {
            let radius = radius.unwrap_or(DEFAULT_RADIUS);
            if side <= radius / PROPORTION {
                side * PROPORTION
            } else {
                radius
            }
        }
        Roundness::Other => 0.0,
    }
}

/// A rectangle whose corners are quadratic curves, starting after the top
/// left corner and going clockwise.
pub(crate) fn rounded_rectangle(w: f64, h: f64, roundness: Roundness) -> [Segment; 9] {
    let r = corner_radius(w.min(h), roundness);
    [
        Segment::Move((r, 0.0)),
        Segment::Line((w - r, 0.0)),
        Segment::Quad((w, 0.0), (w, r)),
        Segment::Line((w, h - r)),
        Segment::Quad((w, h), (w - r, h)),
        Segment::Line((r, h)),
        Segment::Quad((0.0, h), (0.0, h - r)),
        Segment::Line((0.0, r)),
        Segment::Quad((0.0, 0.0), (r, 0.0)),
    ]
}

/// Top, right, bottom and left corner of a diamond. Excalidraw nudges the
/// side corners one unit below the middle so that none of rough.js' numbers
/// is zero.
pub(crate) fn diamond_corners(w: f64, h: f64) -> [Point; 4] {
    let top_x = (w / 2.0).floor() + 1.0;
    let side_y = (h / 2.0).floor() + 1.0;
    [(top_x, 0.0), (w, side_y), (top_x, h), (0.0, side_y)]
}

/// A diamond whose corners are rounded with cubic curves.
pub(crate) fn rounded_diamond(w: f64, h: f64, roundness: Roundness) -> [Segment; 9] {
    let [
        (top_x, top_y),
        (right_x, right_y),
        (bottom_x, bottom_y),
        (left_x, left_y),
    ] = diamond_corners(w, h);
    let vertical = corner_radius((top_x - left_x).abs(), roundness);
    let horizontal = corner_radius((right_y - top_y).abs(), roundness);
    // A corner: the curve touches the tip twice, so it is a rounded point.
    let corner = |tip: Point, (dx, dy): Point| Segment::Cubic(tip, tip, (tip.0 + dx, tip.1 + dy));
    [
        Segment::Move((top_x + vertical, top_y + horizontal)),
        Segment::Line((right_x - vertical, right_y - horizontal)),
        corner((right_x, right_y), (-vertical, horizontal)),
        Segment::Line((bottom_x + vertical, bottom_y - horizontal)),
        corner((bottom_x, bottom_y), (-vertical, -horizontal)),
        Segment::Line((left_x + vertical, left_y + horizontal)),
        corner((left_x, left_y), (vertical, -horizontal)),
        Segment::Line((top_x - vertical, top_y + horizontal)),
        corner((top_x, top_y), (vertical, horizontal)),
    ]
}

// --- Placement -----------------------------------------------------------

/// Moves points from the Element's own frame into the Scene: by the Element's
/// position, turned about a centre by its angle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    origin: Point,
    centre: Point,
    sin_cos: Option<(f64, f64)>,
}

impl Placement {
    /// For Elements that turn about the middle of their box.
    fn of(element: &Element) -> Self {
        let origin = (element.x(), element.y());
        Self::about(
            element,
            (
                origin.0 + element.width() / 2.0,
                origin.1 + element.height() / 2.0,
            ),
        )
    }

    /// For Elements that turn about `centre`, given in the Scene.
    pub(crate) fn about(element: &Element, centre: Point) -> Self {
        let angle = element.angle();
        Self {
            origin: (element.x(), element.y()),
            centre,
            sin_cos: (angle != 0.0).then(|| angle.sin_cos()),
        }
    }

    fn apply(&self, (x, y): Point) -> (f32, f32) {
        let (x, y) = (self.origin.0 + x, self.origin.1 + y);
        let Some((sin, cos)) = self.sin_cos else {
            return (x as f32, y as f32);
        };
        let (dx, dy) = (x - self.centre.0, y - self.centre.1);
        (
            (self.centre.0 + dx * cos - dy * sin) as f32,
            (self.centre.1 + dx * sin + dy * cos) as f32,
        )
    }
}

fn write(path: &mut PartWriter, op: Op, placement: &Placement) {
    match op {
        Op::Move(p) => {
            let (x, y) = placement.apply(p);
            path.move_to(x, y);
        }
        Op::Line(p) => {
            let (x, y) = placement.apply(p);
            path.line_to(x, y);
        }
        Op::Cubic(c1, c2, p) => {
            let (x1, y1) = placement.apply(c1);
            let (x2, y2) = placement.apply(c2);
            let (x, y) = placement.apply(p);
            path.cubic_to(x1, y1, x2, y2, x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_corners_are_32_but_at_most_a_quarter_of_a_small_side() {
        let adaptive = Roundness::Adaptive { radius: None };
        assert_eq!(corner_radius(60.0, adaptive), 15.0);
        assert_eq!(corner_radius(128.0, adaptive), 32.0);
        assert_eq!(corner_radius(300.0, adaptive), 32.0);
        assert_eq!(corner_radius(60.0, Roundness::Proportional), 15.0);
        assert_eq!(corner_radius(60.0, Roundness::Other), 0.0);
    }

    mod fills {
        //! Fills of rectangles, diamonds and ellipses (ADR-0004), tested through
        //! `Editor::geometry`.
        //!
        //! Parity with excalidraw.com is checked against the golden fixtures: rough.js
        //! gives every shape a list of op sets (the fill, then the outline), and the
        //! core must emit one part for each, in that order. The style tests use
        //! literals taken from Excalidraw's and rough.js' sources, never values
        //! recomputed from the code under test.

        use crate::test_support as common;

        use crate::{Geometry, PartKind, Verb};
        use common::{assert_rough_elements_match_golden, editor_with_element, ops};

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

        const SHAPE: &str = r##""id":"a","type":"rectangle","width":100,"height":60,"seed":5,"backgroundColor":"#a5d8ff""##;

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
                let editor = editor_with_element(&format!(
                    r#"{SHAPE},"fillStyle":"{pattern}","x":10,"y":20"#
                ));
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
    }

    mod hand_drawn_shapes {
        //! Hand-drawn rectangles, diamonds and ellipses (ADR-0004), tested through
        //! `Editor::geometry`.
        //!
        //! Parity with excalidraw.com is checked against the golden fixtures, which
        //! hold what rough.js produced for the same seeds. The style tests use
        //! literals taken from Excalidraw's source (`shape.ts` at the pinned commit),
        //! never values recomputed from the code under test.

        use crate::test_support as common;

        use std::f64::consts::FRAC_PI_2;

        use crate::{Editor, Geometry, Part, Verb};
        use common::{Fixture, assert_rough_elements_match_golden, editor_with_element, ops};

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
    }
}
