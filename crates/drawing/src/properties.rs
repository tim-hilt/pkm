//! The properties panel's logic (ADR-0006): setting stroke colour,
//! background colour, fill style, stroke width, stroke style, sloppiness,
//! edges and opacity on the Selection and as the style of the next Element,
//! and what the panel shows for the Selection.
//!
//! Which Elements have which property, and the option sets, are
//! Excalidraw's (`comparisons.ts`, `actionProperties.tsx`): a change reaches
//! the Elements of the Selection that have the property, and the panel shows
//! the value those share.

use crate::color::parse_color;
use crate::draw_shape::{ADAPTIVE_RADIUS, PROPORTIONAL_RADIUS};
use crate::geometry::TextAlign;
use crate::scene::{Element, ElementType, FillPattern, LineStyle, Scene};
use crate::style::{Edges, FontSize, Sloppiness, StrokeWidth, Style, fill_name, line_name};
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;

/// A change to one property, as the panel offers them.
#[derive(Debug, Clone, PartialEq)]
pub enum Property {
    /// A CSS colour, usually hex.
    StrokeColor(String),
    /// A CSS colour, usually hex, or `transparent`.
    BackgroundColor(String),
    FillStyle(FillPattern),
    StrokeWidth(StrokeWidth),
    StrokeStyle(LineStyle),
    Sloppiness(Sloppiness),
    Edges(Edges),
    /// `0.0` (invisible) to `100.0`; anything outside is brought within.
    Opacity(f64),
    /// Of texts only.
    FontSize(FontSize),
    /// Of texts only.
    TextAlign(TextAlign),
}

/// What the properties panel shows: for each property the value of the
/// Selection, or of the style of the next Element if nothing is selected.
///
/// `None` is shown where the Elements of the Selection differ (mixed), or
/// where none of them has the property, or where the value is none of the
/// panel's options.
#[derive(Debug, Clone, PartialEq)]
pub struct Properties {
    pub stroke_color: Option<String>,
    pub background_color: Option<String>,
    pub fill_style: Option<FillPattern>,
    pub stroke_width: Option<StrokeWidth>,
    pub stroke_style: Option<LineStyle>,
    pub sloppiness: Option<Sloppiness>,
    pub edges: Option<Edges>,
    /// `0.0..=100.0`.
    pub opacity: Option<f64>,
    pub font_size: Option<FontSize>,
    pub text_align: Option<TextAlign>,
    /// Which sections the panel shows; the values of a section that applies
    /// are still `None` where they are mixed.
    pub sections: Sections,
}

/// The sections of the panel that apply, following Excalidraw's
/// `getShapeActionPredicates`: a section applies if it applies to the active
/// Tool (so that it can be set before drawing) or to any covered Element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sections {
    pub stroke_color: bool,
    pub background_color: bool,
    /// Only where the background is not transparent.
    pub fill_style: bool,
    pub stroke_width: bool,
    pub stroke_style: bool,
    pub sloppiness: bool,
    pub edges: bool,
    pub opacity: bool,
    /// Font size and alignment: for a text, a labelled shape (its label is
    /// covered) and the text Tool.
    pub text: bool,
}

impl Sections {
    /// The sections for the active Tool (the type of Element it draws, if
    /// any) and the `covered` Elements (the Selection with the labels of its
    /// containers, or the text that is edited). `next_background` is the
    /// background colour the Tool draws with.
    pub(crate) fn new(
        tool: Option<ElementType>,
        covered: &[Element],
        next_background: &str,
    ) -> Self {
        let any = |applies: fn(ElementType) -> bool| {
            tool.is_some_and(applies) || covered.iter().any(|e| applies(e.kind()))
        };
        Self {
            stroke_color: any(has_stroke_color),
            background_color: any(has_background),
            fill_style: tool.is_some_and(|t| has_background(t) && !is_transparent(next_background))
                || covered
                    .iter()
                    .any(|e| has_background(e.kind()) && !is_transparent(e.background_color())),
            stroke_width: any(has_stroke_width),
            stroke_style: any(has_stroke_style),
            sloppiness: any(has_stroke_style),
            edges: any(takes_edges),
            opacity: true,
            text: any(has_font),
        }
    }
}

/// Whether a CSS colour draws nothing (Excalidraw's `isTransparent`: its
/// alpha is zero).
fn is_transparent(color: &str) -> bool {
    parse_color(color) & 0xff == 0
}

impl Property {
    /// Whether Elements of type `kind` have this property.
    fn applies_to(&self, kind: ElementType) -> bool {
        match self {
            Property::StrokeColor(_) => has_stroke_color(kind),
            Property::BackgroundColor(_) | Property::FillStyle(_) => has_background(kind),
            Property::StrokeWidth(_) => has_stroke_width(kind),
            Property::StrokeStyle(_) | Property::Sloppiness(_) => has_stroke_style(kind),
            Property::Edges(_) => takes_edges(kind),
            Property::Opacity(_) => true,
            Property::FontSize(_) | Property::TextAlign(_) => kind == ElementType::Text,
        }
    }

    /// The members that make `element` have this property, or `None` if it
    /// has it already.
    fn members(
        &self,
        element: &Element,
        entropy: &mut Entropy,
    ) -> Option<Vec<(&'static str, Value)>> {
        let text = |s: &str| Value::String(s.to_owned());
        match self {
            Property::StrokeColor(color) => {
                (element.stroke_color() != color).then(|| vec![("strokeColor", text(color))])
            }
            Property::BackgroundColor(color) => (element.background_color() != color)
                .then(|| vec![("backgroundColor", text(color))]),
            Property::FillStyle(fill) => (element.fill_pattern() != *fill)
                .then(|| vec![("fillStyle", text(fill_name(*fill)))]),
            Property::StrokeWidth(width) => {
                let width = width.of(element.kind());
                (element.stroke_width() != width)
                    .then(|| vec![("strokeWidth", Value::Number(width))])
            }
            Property::StrokeStyle(line) => (element.line_style() != *line)
                .then(|| vec![("strokeStyle", text(line_name(*line)))]),
            // Like Excalidraw, a new look is a new seed: the same shape
            // drawn again by hand.
            Property::Sloppiness(sloppiness) => (element.roughness() != sloppiness.roughness())
                .then(|| {
                    vec![
                        ("seed", Value::Number(entropy.integer())),
                        ("roughness", Value::Number(sloppiness.roughness())),
                    ]
                }),
            Property::Edges(edges) => (edges_of(element) != Some(*edges)).then(|| {
                let roundness = match edges {
                    Edges::Sharp => Value::Null,
                    Edges::Round => roundness(rounded(element.kind())),
                };
                vec![("roundness", roundness)]
            }),
            Property::Opacity(opacity) => {
                let opacity = clamp_opacity(*opacity);
                (opacity_of(element) != opacity).then(|| vec![("opacity", Value::Number(opacity))])
            }
            Property::FontSize(size) => (element.font_size() != size.size())
                .then(|| vec![("fontSize", Value::Number(size.size()))]),
            Property::TextAlign(align) => (element.text_align() != *align)
                .then(|| vec![("textAlign", text(align_name(*align)))]),
        }
    }
}

impl Style {
    /// Makes `property` the style of new Elements.
    pub(crate) fn set(&mut self, property: &Property) {
        match property {
            Property::StrokeColor(color) => self.stroke_color.clone_from(color),
            Property::BackgroundColor(color) => self.background_color.clone_from(color),
            Property::FillStyle(fill) => self.fill = *fill,
            Property::StrokeWidth(width) => self.stroke_width = *width,
            Property::StrokeStyle(line) => self.stroke_style = *line,
            Property::Sloppiness(sloppiness) => self.sloppiness = *sloppiness,
            Property::Edges(edges) => self.edges = *edges,
            Property::Opacity(opacity) => self.opacity = clamp_opacity(*opacity),
            Property::FontSize(size) => self.font_size = *size,
            Property::TextAlign(align) => self.text_align = *align,
        }
    }
}

impl Properties {
    /// What the panel shows with nothing selected: the style of the next
    /// Element.
    pub(crate) fn of_style(style: &Style, tool: Option<ElementType>) -> Self {
        Self {
            stroke_color: Some(style.stroke_color.clone()),
            background_color: Some(style.background_color.clone()),
            fill_style: Some(style.fill),
            stroke_width: Some(style.stroke_width),
            stroke_style: Some(style.stroke_style),
            sloppiness: Some(style.sloppiness),
            edges: Some(style.edges),
            opacity: Some(style.opacity),
            font_size: Some(style.font_size),
            text_align: Some(style.text_align),
            sections: Sections::new(tool, &[], &style.background_color),
        }
    }

    /// What the panel shows for the Elements `selected`.
    pub(crate) fn of_selection(
        selected: &[Element],
        tool: Option<ElementType>,
        style: &Style,
    ) -> Self {
        Self {
            stroke_color: common(selected, has_stroke_color, |e| {
                Some(e.stroke_color().to_owned())
            }),
            background_color: common(selected, has_background, |e| {
                Some(e.background_color().to_owned())
            }),
            fill_style: common(selected, has_background, |e| Some(e.fill_pattern())),
            stroke_width: common(selected, has_stroke_width, |e| {
                StrokeWidth::from_width(e.kind(), e.stroke_width())
            }),
            stroke_style: common(selected, has_stroke_style, |e| Some(e.line_style())),
            sloppiness: common(selected, has_stroke_style, |e| {
                Sloppiness::from_roughness(e.roughness())
            }),
            edges: common(selected, takes_edges, edges_of),
            opacity: common(selected, |_| true, |e| Some(opacity_of(e))),
            font_size: common(selected, has_font, |e| FontSize::from_size(e.font_size())),
            text_align: common(selected, has_font, |e| Some(e.text_align())),
            sections: Sections::new(tool, selected, &style.background_color),
        }
    }
}

/// Sets `property` on the Elements of `scene` with `ids` that have it and do
/// not yet, each as a new version of itself. `touch` is told of each before
/// it changes. Returns whether any changed.
pub(crate) fn apply(
    scene: &mut Scene,
    entropy: &mut Entropy,
    ids: &[String],
    property: &Property,
    mut touch: impl FnMut(&Scene, &str),
) -> bool {
    let mut changed = false;
    for id in ids {
        let Some(element) = scene.element_by_id(id) else {
            continue;
        };
        if !property.applies_to(element.kind()) {
            continue;
        }
        let Some(members) = property.members(&element, entropy) else {
            continue;
        };
        let handle = element.handle();
        touch(scene, id);
        scene.update(handle, entropy.integer(), members);
        changed = true;
    }
    changed
}

/// The value of the `elements` that `applies` to, if there are some and they
/// share it.
fn common<T: PartialEq>(
    elements: &[Element],
    applies: fn(ElementType) -> bool,
    read: fn(&Element) -> Option<T>,
) -> Option<T> {
    let mut values = elements
        .iter()
        .filter(|e| applies(e.kind()))
        .map(|e| read(e));
    let first = values.next()??;
    values
        .all(|value| value.as_ref() == Some(&first))
        .then_some(first)
}

// Which Elements have which property (Excalidraw's `comparisons.ts`).

fn has_stroke_color(kind: ElementType) -> bool {
    kind != ElementType::Image
}

fn has_background(kind: ElementType) -> bool {
    use ElementType::*;
    matches!(kind, Rectangle | Ellipse | Diamond | Line | Freedraw)
}

fn has_stroke_width(kind: ElementType) -> bool {
    !matches!(kind, ElementType::Text | ElementType::Image)
}

fn has_stroke_style(kind: ElementType) -> bool {
    use ElementType::*;
    matches!(kind, Rectangle | Ellipse | Diamond | Line | Arrow)
}

fn has_font(kind: ElementType) -> bool {
    kind == ElementType::Text
}

/// Whether Elements of type `kind` have rounded or sharp edges (Excalidraw's
/// `canChangeRoundness`).
pub(crate) fn takes_edges(kind: ElementType) -> bool {
    matches!(
        kind,
        ElementType::Rectangle | ElementType::Diamond | ElementType::Line | ElementType::Image
    )
}

/// The type of rounded corners Elements of type `kind` have when their edges
/// are round: a fixed radius for rectangles and images, a proportion of the
/// size for the others.
fn rounded(kind: ElementType) -> f64 {
    match kind {
        ElementType::Rectangle | ElementType::Image => ADAPTIVE_RADIUS,
        _ => PROPORTIONAL_RADIUS,
    }
}

/// The `roundness` member of type `radius`.
pub(crate) fn roundness(radius: f64) -> Value {
    let mut roundness = Object::new();
    roundness.insert("type".into(), Value::Number(radius));
    Value::Object(roundness)
}

fn edges_of(element: &Element) -> Option<Edges> {
    takes_edges(element.kind()).then(|| match element.roundness() {
        None => Edges::Sharp,
        Some(_) => Edges::Round,
    })
}

/// The `opacity` as written, `0.0..=100.0`.
fn opacity_of(element: &Element) -> f64 {
    element
        .member("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(100.0)
}

/// The `textAlign` the file names `align` by.
pub(crate) fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn clamp_opacity(opacity: f64) -> f64 {
    if opacity.is_nan() {
        100.0
    } else {
        opacity.clamp(0.0, 100.0)
    }
}
