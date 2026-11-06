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
}
