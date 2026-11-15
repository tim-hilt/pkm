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
    /// Align: two or more groups or Elements are selected (see
    /// [`crate::Editor::can_align`]).
    pub align: bool,
    /// Distribute, within Align: three or more are.
    pub distribute: bool,
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
            align: false,
            distribute: false,
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

#[cfg(test)]
mod tests {
    //! The properties panel's seam (ADR-0006): style commands in, the Selection's
    //! saved Elements, the values the panel shows and the style of the next
    //! Element out. Option values are literals from Excalidraw's own panel
    //! (stroke widths 1, 2, 4; roughness 0, 1, 2; roundness types 2 and 3).

    use crate::scene::{FillPattern, LineStyle};
    use crate::{Edges, Editor, Property, Sections, Sloppiness, StrokeWidth, Tool};
    use pkm_base::json::Value;

    use crate::test_support as common;
    use common::{editor_with, replay, saved_elements, tap};

    const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,
    "strokeStyle":"solid","roughness":1,"opacity":100,"roundness":{"type":3},
    "version":3,"versionNonce":7}"##;

    const ELLIPSE: &str = r##"{"id":"b","type":"ellipse","x":300,"y":100,"width":100,"height":50,
    "strokeColor":"#e03131","backgroundColor":"#ffc9c9","fillStyle":"hachure","strokeWidth":4,
    "strokeStyle":"dashed","roughness":2,"opacity":50,"roundness":null,
    "version":3,"versionNonce":7}"##;

    fn element(editor: &Editor, id: &str) -> Value {
        saved_elements(editor)
            .into_iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap()
    }

    fn text_of(editor: &Editor, id: &str, key: &str) -> String {
        element(editor, id)
            .get(key)
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    }

    fn number_of(editor: &Editor, id: &str, key: &str) -> f64 {
        element(editor, id)
            .get(key)
            .and_then(Value::as_f64)
            .unwrap()
    }

    /// Both Elements selected.
    fn both() -> Editor {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        editor.select_all();
        editor
    }

    /// Draws a rectangle from (10, 10) to (60, 40) and returns to selection.
    fn draw_rectangle(editor: &mut Editor) {
        editor.set_tool(Tool::Rectangle);
        replay(
            editor,
            "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 up 1 mouse 60 40",
        );
    }

    // --- Applying to the Selection ----------------------------------------------

    #[test]
    fn stroke_colour_applies_to_every_element_in_the_selection() {
        let mut editor = both();
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert_eq!(text_of(&editor, "a", "strokeColor"), "#1971c2");
        assert_eq!(text_of(&editor, "b", "strokeColor"), "#1971c2");
    }

    #[test]
    fn only_selected_elements_change() {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        tap(&mut editor, "mouse", (100.0, 125.0), 1.0);
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert_eq!(text_of(&editor, "a", "strokeColor"), "#1971c2");
        assert_eq!(text_of(&editor, "b", "strokeColor"), "#e03131");
    }

    #[test]
    fn every_listed_property_is_written_the_way_excalidraw_saves_it() {
        let mut editor = both();
        editor.set_property(Property::BackgroundColor("#a5d8ff".into()));
        editor.set_property(Property::FillStyle(FillPattern::CrossHatch));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Thin));
        editor.set_property(Property::StrokeStyle(LineStyle::Dotted));
        editor.set_property(Property::Sloppiness(Sloppiness::Architect));
        editor.set_property(Property::Opacity(30.0));
        for id in ["a", "b"] {
            assert_eq!(text_of(&editor, id, "backgroundColor"), "#a5d8ff");
            assert_eq!(text_of(&editor, id, "fillStyle"), "cross-hatch");
            assert_eq!(number_of(&editor, id, "strokeWidth"), 1.0);
            assert_eq!(text_of(&editor, id, "strokeStyle"), "dotted");
            assert_eq!(number_of(&editor, id, "roughness"), 0.0);
            assert_eq!(number_of(&editor, id, "opacity"), 30.0);
        }
    }

    #[test]
    fn the_other_options_of_each_property_are_written_too() {
        let mut editor = both();
        editor.set_property(Property::FillStyle(FillPattern::Hachure));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Bold));
        editor.set_property(Property::StrokeStyle(LineStyle::Dashed));
        editor.set_property(Property::Sloppiness(Sloppiness::Cartoonist));
        assert_eq!(text_of(&editor, "a", "fillStyle"), "hachure");
        assert_eq!(number_of(&editor, "a", "strokeWidth"), 4.0);
        assert_eq!(text_of(&editor, "a", "strokeStyle"), "dashed");
        assert_eq!(number_of(&editor, "a", "roughness"), 2.0);

        editor.set_property(Property::FillStyle(FillPattern::Solid));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Medium));
        editor.set_property(Property::StrokeStyle(LineStyle::Solid));
        editor.set_property(Property::Sloppiness(Sloppiness::Artist));
        assert_eq!(text_of(&editor, "a", "fillStyle"), "solid");
        assert_eq!(number_of(&editor, "a", "strokeWidth"), 2.0);
        assert_eq!(text_of(&editor, "a", "strokeStyle"), "solid");
        assert_eq!(number_of(&editor, "a", "roughness"), 1.0);
    }

    #[test]
    fn sharp_edges_take_the_roundness_away_and_round_ones_give_the_type_of_the_shape() {
        let diamond = r#"{"id":"d","type":"diamond","x":0,"y":0,"width":50,"height":50,
        "roundness":{"type":2},"version":1,"versionNonce":1}"#;
        let mut editor = editor_with(&[RECTANGLE, diamond]);
        editor.select_all();

        editor.set_property(Property::Edges(Edges::Sharp));
        assert_eq!(element(&editor, "a").get("roundness"), Some(&Value::Null));
        assert_eq!(element(&editor, "d").get("roundness"), Some(&Value::Null));

        editor.set_property(Property::Edges(Edges::Round));
        // Rectangles take Excalidraw's adaptive radius, diamonds the proportional.
        let type_of = |id| {
            element(&editor, id)
                .get("roundness")
                .and_then(|r| r.get("type"))
                .and_then(Value::as_f64)
        };
        assert_eq!(type_of("a"), Some(3.0));
        assert_eq!(type_of("d"), Some(2.0));
    }

    #[test]
    fn a_new_sloppiness_gives_the_shape_a_new_seed() {
        let mut editor = both();
        let before = number_of(&editor, "a", "seed");
        editor.set_property(Property::Sloppiness(Sloppiness::Cartoonist));
        assert_ne!(number_of(&editor, "a", "seed"), before);
    }

    #[test]
    fn opacity_is_kept_within_zero_and_a_hundred() {
        let mut editor = both();
        editor.set_property(Property::Opacity(250.0));
        assert_eq!(number_of(&editor, "a", "opacity"), 100.0);
        editor.set_property(Property::Opacity(-5.0));
        assert_eq!(number_of(&editor, "a", "opacity"), 0.0);
    }

    #[test]
    fn properties_an_element_does_not_have_are_left_alone() {
        let text = r##"{"id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"text":"hi",
        "originalText":"hi","strokeColor":"#1e1e1e","opacity":100,"version":1,"versionNonce":1}"##;
        let mut editor = editor_with(&[text]);
        editor.select_all();
        let fill = text_of(&editor, "t", "fillStyle");
        editor.set_property(Property::StrokeColor("#2f9e44".into()));
        editor.set_property(Property::FillStyle(FillPattern::Zigzag));
        editor.set_property(Property::Opacity(40.0));
        assert_eq!(text_of(&editor, "t", "strokeColor"), "#2f9e44");
        assert_eq!(number_of(&editor, "t", "opacity"), 40.0);
        assert_eq!(text_of(&editor, "t", "fillStyle"), fill);
    }

    // --- What the panel shows ---------------------------------------------------

    #[test]
    fn the_panel_shows_the_value_the_selected_elements_share() {
        let mut editor = both();
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        let shown = editor.properties();
        assert_eq!(shown.stroke_color.as_deref(), Some("#1971c2"));
    }

    #[test]
    fn the_panel_shows_mixed_values_as_none() {
        let editor = both();
        let shown = editor.properties();
        assert_eq!(shown.stroke_color, None);
        assert_eq!(shown.background_color, None);
        assert_eq!(shown.fill_style, None);
        assert_eq!(shown.stroke_width, None);
        assert_eq!(shown.stroke_style, None);
        assert_eq!(shown.sloppiness, None);
        assert_eq!(shown.opacity, None);
    }

    #[test]
    fn a_single_selected_element_shows_all_its_values() {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        tap(&mut editor, "mouse", (300.0, 125.0), 1.0);
        // Tapped inside: a hachure-filled ellipse is hit by its fill.
        let shown = editor.properties();
        assert_eq!(shown.stroke_color.as_deref(), Some("#e03131"));
        assert_eq!(shown.background_color.as_deref(), Some("#ffc9c9"));
        assert_eq!(shown.fill_style, Some(FillPattern::Hachure));
        assert_eq!(shown.stroke_width, Some(StrokeWidth::Bold));
        assert_eq!(shown.stroke_style, Some(LineStyle::Dashed));
        assert_eq!(shown.sloppiness, Some(Sloppiness::Cartoonist));
        assert_eq!(shown.opacity, Some(50.0));
    }

    #[test]
    fn edges_are_shown_for_the_elements_that_have_them() {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        tap(&mut editor, "mouse", (100.0, 125.0), 1.0);
        assert_eq!(editor.properties().edges, Some(Edges::Round));
        editor.set_property(Property::Edges(Edges::Sharp));
        assert_eq!(editor.properties().edges, Some(Edges::Sharp));
    }

    #[test]
    fn the_panel_follows_the_selection() {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        tap(&mut editor, "mouse", (100.0, 125.0), 1.0);
        assert_eq!(editor.properties().stroke_color.as_deref(), Some("#1e1e1e"));
        editor.select_all();
        assert_eq!(editor.properties().stroke_color, None);
    }

    // --- With an empty Selection: the style of the next Element -----------------

    #[test]
    fn with_nothing_selected_the_panel_shows_excalidraws_defaults() {
        let editor = Editor::new();
        let shown = editor.properties();
        assert_eq!(shown.stroke_color.as_deref(), Some("#1e1e1e"));
        assert_eq!(shown.background_color.as_deref(), Some("transparent"));
        assert_eq!(shown.fill_style, Some(FillPattern::Solid));
        assert_eq!(shown.stroke_width, Some(StrokeWidth::Medium));
        assert_eq!(shown.stroke_style, Some(LineStyle::Solid));
        assert_eq!(shown.sloppiness, Some(Sloppiness::Artist));
        assert_eq!(shown.edges, Some(Edges::Round));
        assert_eq!(shown.opacity, Some(100.0));
    }

    #[test]
    fn with_nothing_selected_a_change_is_the_style_of_the_next_element() {
        let mut editor = Editor::new();
        editor.set_property(Property::StrokeColor("#e03131".into()));
        editor.set_property(Property::BackgroundColor("#ffc9c9".into()));
        editor.set_property(Property::FillStyle(FillPattern::Zigzag));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Bold));
        editor.set_property(Property::StrokeStyle(LineStyle::Dashed));
        editor.set_property(Property::Sloppiness(Sloppiness::Cartoonist));
        editor.set_property(Property::Edges(Edges::Sharp));
        editor.set_property(Property::Opacity(60.0));
        assert_eq!(editor.properties().stroke_color.as_deref(), Some("#e03131"));

        draw_rectangle(&mut editor);
        let drawn = saved_elements(&editor).remove(0);
        let text = |key| drawn.get(key).and_then(Value::as_str).unwrap().to_owned();
        let number = |key| drawn.get(key).and_then(Value::as_f64).unwrap();
        assert_eq!(text("strokeColor"), "#e03131");
        assert_eq!(text("backgroundColor"), "#ffc9c9");
        assert_eq!(text("fillStyle"), "zigzag");
        assert_eq!(number("strokeWidth"), 4.0);
        assert_eq!(text("strokeStyle"), "dashed");
        assert_eq!(number("roughness"), 2.0);
        assert_eq!(drawn.get("roundness"), Some(&Value::Null));
        assert_eq!(number("opacity"), 60.0);
    }

    #[test]
    fn changing_a_selection_also_sets_the_style_of_the_next_element() {
        // As on excalidraw.com: the style last chosen is used for the next one.
        let mut editor = editor_with(&[RECTANGLE]);
        editor.select_all();
        editor.set_property(Property::StrokeColor("#2f9e44".into()));
        editor.select_all();
        tap(&mut editor, "mouse", (500.0, 500.0), 5.0); // clears the Selection
        assert_eq!(editor.properties().stroke_color.as_deref(), Some("#2f9e44"));
    }

    #[test]
    fn a_freedraw_stroke_is_half_as_wide_for_the_same_option() {
        let mut editor = Editor::new();
        editor.set_property(Property::StrokeWidth(StrokeWidth::Bold));
        editor.set_tool(Tool::Freedraw);
        replay(
            &mut editor,
            "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 up 1 mouse 60 40",
        );
        editor.select_all();
        assert_eq!(number_of_only(&editor, "strokeWidth"), 2.0);
        // The panel still shows the option, not the width.
        assert_eq!(editor.properties().stroke_width, Some(StrokeWidth::Bold));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Thin));
        assert_eq!(number_of_only(&editor, "strokeWidth"), 0.5);
    }

    fn number_of_only(editor: &Editor, key: &str) -> f64 {
        saved_elements(editor)[0]
            .get(key)
            .and_then(Value::as_f64)
            .unwrap()
    }

    // --- Undo ---------------------------------------------------------------------

    #[test]
    fn each_change_is_one_undo_step_for_the_whole_selection() {
        let mut editor = both();
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        editor.set_property(Property::Opacity(20.0));

        editor.undo();
        assert_eq!(number_of(&editor, "a", "opacity"), 100.0);
        assert_eq!(number_of(&editor, "b", "opacity"), 50.0);
        assert_eq!(text_of(&editor, "a", "strokeColor"), "#1971c2");

        editor.undo();
        assert_eq!(text_of(&editor, "a", "strokeColor"), "#1e1e1e");
        assert_eq!(text_of(&editor, "b", "strokeColor"), "#e03131");
        assert!(!editor.can_undo());

        editor.redo();
        editor.redo();
        assert_eq!(text_of(&editor, "b", "strokeColor"), "#1971c2");
        assert_eq!(number_of(&editor, "b", "opacity"), 20.0);
    }

    #[test]
    fn undo_brings_the_selection_and_the_panel_back_with_it() {
        let mut editor = both();
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert_eq!(editor.selection().len(), 2);
        editor.undo();
        assert_eq!(editor.selection().len(), 2);
        assert_eq!(editor.properties().stroke_color, None);
    }

    #[test]
    fn choosing_what_is_already_there_is_not_an_undo_step() {
        let mut editor = both();
        editor.set_property(Property::Opacity(100.0)); // "b" differs, "a" does not
        editor.undo();
        assert_eq!(number_of(&editor, "b", "opacity"), 50.0);
        assert!(!editor.can_undo());

        editor.set_property(Property::StrokeColor("#1e1e1e".into()));
        editor.set_property(Property::StrokeColor("#1e1e1e".into()));
        editor.undo();
        assert_eq!(text_of(&editor, "b", "strokeColor"), "#e03131");
        assert!(!editor.can_undo());
    }

    #[test]
    fn with_nothing_selected_there_is_nothing_to_undo() {
        let mut editor = editor_with(&[RECTANGLE]);
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert!(!editor.can_undo());
        assert_eq!(text_of(&editor, "a", "strokeColor"), "#1e1e1e");
    }

    #[test]
    fn a_change_makes_a_new_version_of_the_elements_it_changed() {
        let mut editor = editor_with(&[RECTANGLE, ELLIPSE]);
        editor.select_all();
        let revision = editor.revision();
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert!(editor.revision() > revision, "clients must redraw");
        assert_eq!(number_of(&editor, "a", "version"), 4.0);
    }

    // --- Whether the panel is shown ---------------------------------------------

    #[test]
    fn the_panel_is_shown_for_a_drawing_tool_or_a_selection() {
        // Excalidraw's `showSelectedShapeActions`: a Tool that draws, a text
        // being edited or a Selection; not for selection, hand or eraser alone.
        let mut editor = editor_with(&[RECTANGLE]);
        for tool in [Tool::Selection, Tool::Hand, Tool::Eraser] {
            editor.set_tool(tool);
            assert!(!editor.shows_properties(), "{tool:?}");
        }
        for tool in [
            Tool::Rectangle,
            Tool::Diamond,
            Tool::Ellipse,
            Tool::Arrow,
            Tool::Line,
            Tool::Freedraw,
            Tool::Text,
        ] {
            editor.set_tool(tool);
            assert!(editor.shows_properties(), "{tool:?}");
        }
        editor.set_tool(Tool::Selection);
        editor.select_all();
        assert!(editor.shows_properties());

        let mut editor = Editor::new();
        editor.set_text_measurer(std::sync::Arc::new(common::FixedWidth));
        editor.set_tool(Tool::Text);
        tap(&mut editor, "mouse", (500.0, 500.0), 0.0);
        assert!(editor.text_editor().is_some());
        assert!(editor.shows_properties());
        editor.commit_text(); // an empty text is dropped
        assert!(!editor.shows_properties());
    }

    // --- Which sections apply (Excalidraw's `getShapeActionPredicates`) ---------------

    const NONE: Sections = Sections {
        stroke_color: false,
        background_color: false,
        fill_style: false,
        stroke_width: false,
        stroke_style: false,
        sloppiness: false,
        edges: false,
        opacity: true,
        text: false,
        align: false,
        distribute: false,
    };

    fn element_json(kind: &str, extra: &str) -> String {
        format!(
            r##"{{"id":"e","type":"{kind}","x":100,"y":100,"width":100,"height":50,
        "backgroundColor":"transparent","strokeWidth":2,"roundness":null,
        "fontSize":20,"fontFamily":5,"text":"Hi","originalText":"Hi","lineHeight":1.25,
        "points":[[0,0],[100,50]],{extra}"version":3,"versionNonce":7}}"##
        )
    }

    fn sections_of_selected(kind: &str, extra: &str) -> Sections {
        let mut editor = editor_with(&[&element_json(kind, extra)]);
        editor.select_all();
        editor.properties().sections
    }

    #[test]
    fn a_text_has_no_fill_stroke_width_style_sloppiness_or_edges() {
        assert_eq!(
            sections_of_selected("text", ""),
            Sections {
                stroke_color: true,
                text: true,
                ..NONE
            }
        );
    }

    #[test]
    fn an_image_has_edges_and_opacity_only() {
        assert_eq!(
            sections_of_selected("image", ""),
            Sections {
                edges: true,
                ..NONE
            }
        );
    }

    #[test]
    fn a_filled_ellipse_has_everything_but_edges_and_text() {
        assert_eq!(
            sections_of_selected("ellipse", r##""backgroundColor":"#ffc9c9","##),
            Sections {
                stroke_color: true,
                background_color: true,
                fill_style: true,
                stroke_width: true,
                stroke_style: true,
                sloppiness: true,
                ..NONE
            }
        );
    }

    #[test]
    fn the_fill_style_is_offered_only_for_a_background_that_is_not_transparent() {
        for (color, offered) in [
            ("transparent", false),
            ("#ffffff00", false),
            ("#ffc9c9", true),
        ] {
            let sections =
                sections_of_selected("rectangle", &format!(r#""backgroundColor":"{color}","#));
            assert!(sections.background_color);
            assert_eq!(sections.fill_style, offered, "{color}");
        }
    }

    #[test]
    fn a_rectangle_and_a_line_have_edges_but_an_arrow_does_not() {
        assert!(sections_of_selected("rectangle", "").edges);
        assert!(sections_of_selected("diamond", "").edges);
        assert!(sections_of_selected("line", "").edges);
        assert!(!sections_of_selected("arrow", "").edges);
        assert!(!sections_of_selected("ellipse", "").edges);
    }

    #[test]
    fn a_line_has_a_background_but_an_arrow_has_none() {
        assert!(sections_of_selected("line", "").background_color);
        assert!(!sections_of_selected("arrow", "").background_color);
    }

    #[test]
    fn a_freedraw_stroke_has_width_and_background_but_no_style_or_sloppiness() {
        assert_eq!(
            sections_of_selected("freedraw", ""),
            Sections {
                stroke_color: true,
                background_color: true,
                stroke_width: true,
                ..NONE
            }
        );
    }

    #[test]
    fn a_mixed_selection_has_the_sections_of_any_of_its_elements() {
        let text = element_json("text", "").replace(r#""id":"e""#, r#""id":"t""#);
        let image = element_json("image", "");
        let mut editor = editor_with(&[&text, &image]);
        editor.select_all();
        assert_eq!(
            editor.properties().sections,
            Sections {
                stroke_color: true,
                edges: true,
                text: true,
                align: true,
                ..NONE
            }
        );
    }

    #[test]
    fn a_section_that_applies_but_differs_is_shown_mixed() {
        let editor = both();
        let shown = editor.properties();
        assert!(shown.sections.stroke_width);
        assert_eq!(shown.stroke_width, None);
    }

    #[test]
    fn with_nothing_selected_the_sections_are_those_of_the_tool() {
        let mut editor = editor_with(&[]);
        let sections = |editor: &mut Editor, tool| {
            editor.set_tool(tool);
            editor.properties().sections
        };
        let with_everything = Sections {
            stroke_color: true,
            stroke_width: true,
            stroke_style: true,
            sloppiness: true,
            ..NONE
        };
        // The default background is transparent: no fill style yet.
        assert_eq!(
            sections(&mut editor, Tool::Rectangle),
            Sections {
                background_color: true,
                edges: true,
                ..with_everything
            }
        );
        assert_eq!(
            sections(&mut editor, Tool::Ellipse),
            Sections {
                background_color: true,
                ..with_everything
            }
        );
        assert_eq!(
            sections(&mut editor, Tool::Diamond),
            Sections {
                background_color: true,
                edges: true,
                ..with_everything
            }
        );
        assert_eq!(
            sections(&mut editor, Tool::Line),
            Sections {
                background_color: true,
                edges: true,
                ..with_everything
            }
        );
        assert_eq!(sections(&mut editor, Tool::Arrow), with_everything);
        assert_eq!(
            sections(&mut editor, Tool::Freedraw),
            Sections {
                stroke_color: true,
                background_color: true,
                stroke_width: true,
                ..NONE
            }
        );
        assert_eq!(
            sections(&mut editor, Tool::Text),
            Sections {
                stroke_color: true,
                text: true,
                ..NONE
            }
        );
        assert_eq!(sections(&mut editor, Tool::Selection), NONE);
    }

    #[test]
    fn the_tool_offers_the_fill_style_once_the_next_background_is_not_transparent() {
        let mut editor = editor_with(&[]);
        editor.set_tool(Tool::Rectangle);
        assert!(!editor.properties().sections.fill_style);
        editor.set_property(Property::BackgroundColor("#ffc9c9".into()));
        assert!(editor.properties().sections.fill_style);
        editor.set_tool(Tool::Arrow);
        assert!(!editor.properties().sections.fill_style);
    }
}
