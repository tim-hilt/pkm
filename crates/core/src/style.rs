//! The current style: what new Elements look like (Excalidraw's
//! `currentItem...` app state), and the option sets of the properties panel.

use crate::scene::{ElementType, FillPattern, LineStyle};

/// The stroke widths the properties panel offers (Excalidraw's `STROKE_WIDTH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeWidth {
    Thin,
    Medium,
    Bold,
}

impl StrokeWidth {
    /// The `strokeWidth` an Element of `kind` has for this option. Freedraw
    /// strokes are drawn half as wide as the other Elements for the same
    /// option (Excalidraw's `FREEDRAW_STROKE_WIDTH`).
    pub(crate) fn of(self, kind: ElementType) -> f64 {
        let width = match self {
            StrokeWidth::Thin => 1.0,
            StrokeWidth::Medium => 2.0,
            StrokeWidth::Bold => 4.0,
        };
        if kind == ElementType::Freedraw {
            width / 2.0
        } else {
            width
        }
    }

    /// The option an Element of `kind` with `width` has, if it has one.
    pub(crate) fn from_width(kind: ElementType, width: f64) -> Option<Self> {
        [StrokeWidth::Thin, StrokeWidth::Medium, StrokeWidth::Bold]
            .into_iter()
            .find(|option| option.of(kind) == width)
    }
}

/// How hand-drawn Elements look, as the `roughness` of Excalidraw's
/// "sloppiness" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sloppiness {
    Architect,
    Artist,
    Cartoonist,
}

impl Sloppiness {
    pub(crate) fn roughness(self) -> f64 {
        match self {
            Sloppiness::Architect => 0.0,
            Sloppiness::Artist => 1.0,
            Sloppiness::Cartoonist => 2.0,
        }
    }

    pub(crate) fn from_roughness(roughness: f64) -> Option<Self> {
        [
            Sloppiness::Architect,
            Sloppiness::Artist,
            Sloppiness::Cartoonist,
        ]
        .into_iter()
        .find(|option| option.roughness() == roughness)
    }
}

/// Whether corners are rounded: whether an Element has `roundness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edges {
    Sharp,
    Round,
}

/// Excalidraw's defaults (`DEFAULT_ELEMENT_PROPS`), changed by the properties
/// panel: what every new Element gets.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub stroke_color: String,
    pub background_color: String,
    pub fill: FillPattern,
    pub stroke_width: StrokeWidth,
    pub stroke_style: LineStyle,
    pub sloppiness: Sloppiness,
    pub edges: Edges,
    /// `0.0..=100.0`.
    pub opacity: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill: FillPattern::Solid,
            stroke_width: StrokeWidth::Medium,
            stroke_style: LineStyle::Solid,
            sloppiness: Sloppiness::Artist,
            edges: Edges::Round,
            opacity: 100.0,
        }
    }
}

/// The `fillStyle` the file names `fill` by.
pub(crate) fn fill_name(fill: FillPattern) -> &'static str {
    match fill {
        FillPattern::Hachure => "hachure",
        FillPattern::CrossHatch => "cross-hatch",
        FillPattern::Zigzag => "zigzag",
        FillPattern::Solid => "solid",
    }
}

/// The `strokeStyle` the file names `line` by.
pub(crate) fn line_name(line: LineStyle) -> &'static str {
    match line {
        LineStyle::Solid => "solid",
        LineStyle::Dashed => "dashed",
        LineStyle::Dotted => "dotted",
    }
}
