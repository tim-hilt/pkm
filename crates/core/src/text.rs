//! What Excalidraw draws for a text Element (`renderElement.ts` and
//! `textElement.ts` at the pinned commit), as geometry: positioned lines of
//! text. The core never lays out glyphs (ADR-0008); it only decides where each
//! line starts, and where a container's label wraps, using the client's
//! [`TextMeasurer`].

use std::cell::OnceCell;
use std::collections::HashMap;

use crate::geometry::{Geometry, TextAlign, TextStyle};
use crate::scene::{Element, ElementType, Scene, VerticalAlign};
use crate::shape::parse_color;
use crate::wrap;

/// Measures text for the core; supplied by the client, which owns the font.
///
/// Every font family is drawn with Excalifont, the only bundled font, so the
/// font is given by its size alone.
pub trait TextMeasurer: Send + Sync {
    /// The advance width in Scene units of `text`, a single line, set in
    /// Excalifont at `font_size`.
    fn line_width(&self, text: &str, font_size: f64) -> f64;
}

/// Padding between a container and the text bound to it.
const BOUND_TEXT_PADDING: f64 = 5.0;

/// The Elements a text can be bound to, found by `id` the first time one is
/// asked for: most Scenes have no labels.
pub(crate) struct Containers<'a> {
    scene: &'a Scene,
    by_id: OnceCell<HashMap<&'a str, Element<'a>>>,
}

impl<'a> Containers<'a> {
    pub(crate) fn new(scene: &'a Scene) -> Self {
        Self {
            scene,
            by_id: OnceCell::new(),
        }
    }

    /// The Element `text` is a label of, if it has one that is shown and can
    /// hold text.
    fn of(&self, text: &Element) -> Option<Element<'a>> {
        let id = text.container_id()?;
        let by_id = self
            .by_id
            .get_or_init(|| self.scene.elements().map(|e| (e.id(), e)).collect());
        let container = *by_id.get(id)?;
        let holds_text = matches!(
            container.kind(),
            ElementType::Rectangle
                | ElementType::Diamond
                | ElementType::Ellipse
                | ElementType::Arrow
        );
        (holds_text && !container.is_deleted()).then_some(container)
    }
}

/// Where a text is and what it says: its lines in a box.
struct Block {
    lines: Vec<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    /// Clockwise about the middle of the box.
    angle: f64,
}

/// Adds the lines of the text `element` to `geometry`.
///
/// With a `measurer`, a label wraps to its container and sits in it as
/// Excalidraw would lay it out now (`redrawTextBoundingBox`), and so does a
/// text of fixed width; without, and for text that is as wide as it is long,
/// the lines are the ones saved in the file. The text of an arrow's label is
/// where the file has it.
pub(crate) fn draw(
    element: &Element,
    containers: &Containers,
    measurer: Option<&dyn TextMeasurer>,
    geometry: &mut Geometry,
) {
    let container = containers.of(element);
    let block = layout(element, container.as_ref(), measurer);
    if block.lines.iter().all(String::is_empty) {
        return;
    }

    let font_size = element.font_size();
    let line_height = font_size * element.line_height();
    let baseline = baseline_offset(element.font_family(), font_size, line_height);
    let align = element.text_align();
    let style = TextStyle {
        color: parse_color(element.stroke_color()),
        opacity: element.opacity() as f32,
        font_family: element.font_family(),
        font_size: font_size as f32,
        align,
    };
    // Lines change with the container as well as with their own Element.
    let version = container.map_or(element.version(), |c| {
        element
            .version()
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(c.version())
    });

    let anchor_x = block.x
        + match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => block.width / 2.0,
            TextAlign::Right => block.width,
        };
    let centre = (block.x + block.width / 2.0, block.y + block.height / 2.0);
    let (sin, cos) = block.angle.sin_cos();
    for (index, line) in block.lines.iter().enumerate() {
        let (dx, dy) = (
            anchor_x - centre.0,
            block.y + index as f64 * line_height + baseline - centre.1,
        );
        let at = (
            centre.0 + dx * cos - dy * sin,
            centre.1 + dx * sin + dy * cos,
        );
        geometry.text_line(
            element.handle(),
            version,
            style,
            line,
            (at.0 as f32, at.1 as f32),
            block.angle as f32,
        );
    }
}

fn layout(
    element: &Element,
    container: Option<&Element>,
    measurer: Option<&dyn TextMeasurer>,
) -> Block {
    let saved = Block {
        lines: normalized(element.text())
            .split('\n')
            .map(str::to_owned)
            .collect(),
        x: element.x(),
        y: element.y(),
        width: element.width(),
        height: element.height(),
        angle: element.angle(),
    };
    let Some(measurer) = measurer else {
        return saved;
    };
    let font_size = element.font_size();
    let line_height = font_size * element.line_height();
    let wrapped = |max_width: f64| {
        wrap::wrap_text(
            &normalized(element.original_text()),
            font_size,
            max_width,
            measurer,
        )
    };

    match container {
        Some(container) if container.kind() == ElementType::Arrow => saved,
        Some(container) => {
            let lines = wrapped(bound_max_width(container));
            let width = wrap::widest(&lines, font_size, measurer);
            let height = lines.len() as f64 * line_height;
            let (x, y) = bound_position(element, container, (width, height));
            Block {
                lines,
                x,
                y,
                width,
                height,
                angle: container.angle(),
            }
        }
        None if !element.auto_resize() => {
            let lines = wrapped(element.width());
            Block {
                height: lines.len() as f64 * line_height,
                lines,
                ..saved
            }
        }
        None => saved,
    }
}

/// How wide the lines of a label may be (`getBoundTextMaxWidth`): the width
/// of the rectangle that fits in the container, less padding.
fn bound_max_width(container: &Element) -> f64 {
    label_max_width(container.kind(), container.width())
}

/// How wide the lines of a label may be in a container of `kind` and `width`.
pub(crate) fn label_max_width(kind: ElementType, width: f64) -> f64 {
    inscribed(kind, width) - BOUND_TEXT_PADDING * 2.0
}

/// How high a label may be (`getBoundTextMaxHeight`).
fn bound_max_height(container: &Element) -> f64 {
    inscribed(container.kind(), container.height()) - BOUND_TEXT_PADDING * 2.0
}

/// The side of the largest rectangle inside a container of `kind` with
/// `side`, which is as wide or high as the container.
fn inscribed(kind: ElementType, side: f64) -> f64 {
    match kind {
        ElementType::Ellipse => (side / 2.0 * std::f64::consts::SQRT_2).round(),
        ElementType::Diamond => (side / 2.0).round(),
        _ => side,
    }
}

/// Where the top left of a label of `size` goes (`computeBoundTextPosition`):
/// in the padded rectangle inside the container as its alignment says, then
/// turned with the container.
fn bound_position(text: &Element, container: &Element, (width, height): (f64, f64)) -> (f64, f64) {
    // The padded rectangle inside the container.
    let (mut left, mut top) = (
        container.x() + BOUND_TEXT_PADDING,
        container.y() + BOUND_TEXT_PADDING,
    );
    match container.kind() {
        ElementType::Ellipse => {
            let inset = 1.0 - std::f64::consts::FRAC_1_SQRT_2;
            left += container.width() / 2.0 * inset;
            top += container.height() / 2.0 * inset;
        }
        ElementType::Diamond => {
            left += container.width() / 4.0;
            top += container.height() / 4.0;
        }
        _ => {}
    }
    let (max_width, max_height) = (bound_max_width(container), bound_max_height(container));

    let y = top
        + match text.vertical_align() {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Middle => max_height / 2.0 - height / 2.0,
            VerticalAlign::Bottom => max_height - height,
        };
    let x = left
        + match text.text_align() {
            TextAlign::Left => 0.0,
            TextAlign::Center => max_width / 2.0 - width / 2.0,
            TextAlign::Right => max_width - width,
        };

    let angle = container.angle();
    if angle == 0.0 {
        return (x, y);
    }
    let pivot = (left + max_width / 2.0, top + max_height / 2.0);
    let (dx, dy) = (x + width / 2.0 - pivot.0, y + height / 2.0 - pivot.1);
    let (sin, cos) = angle.sin_cos();
    (
        pivot.0 + dx * cos - dy * sin - width / 2.0,
        pivot.1 + dx * sin + dy * cos - height / 2.0,
    )
}

/// A text set in lines.
pub(crate) struct Lines {
    /// The lines joined by `\n`, as a text Element saves them.
    pub text: String,
    /// The width of the widest line.
    pub width: f64,
    pub height: f64,
}

/// The text of `element` as typed, set in lines at most `max_width` wide.
pub(crate) fn set_in_lines(
    element: &Element,
    max_width: f64,
    measurer: &dyn TextMeasurer,
) -> Lines {
    let font_size = element.font_size();
    let lines = wrap::wrap_text(
        &normalized(element.original_text()),
        font_size,
        max_width,
        measurer,
    );
    Lines {
        width: wrap::widest(&lines, font_size, measurer),
        height: lines.len() as f64 * font_size * element.line_height(),
        text: lines.join("\n"),
    }
}

/// The members a label has to save to sit in its container as the container
/// is now (Excalidraw's `handleBindTextResize`).
pub(crate) struct LabelFit {
    /// The wrapped text, if the lines were laid out again.
    pub text: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where `label` sits in the shape `container`, and how big it is there. With
/// a `measurer` the text is wrapped to the container again; without, the
/// label keeps the size it was saved with.
pub(crate) fn fit_label(
    label: &Element,
    container: &Element,
    measurer: Option<&dyn TextMeasurer>,
) -> LabelFit {
    let (text, width, height) = match measurer {
        Some(measurer) => {
            let lines = set_in_lines(label, bound_max_width(container), measurer);
            (Some(lines.text), lines.width, lines.height)
        }
        None => (None, label.width(), label.height()),
    };
    let (x, y) = bound_position(label, container, (width, height));
    LabelFit {
        text,
        x,
        y,
        width,
        height,
    }
}

/// How small a container of `label` may get: one character wide and one line
/// high (Excalidraw's `getApproxMinLineWidth` and `getApproxMinLineHeight`).
pub(crate) fn min_container_size(label: &Element, measurer: &dyn TextMeasurer) -> (f64, f64) {
    let font_size = label.font_size();
    let widest_char = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        .chars()
        .map(|c| measurer.line_width(c.encode_utf8(&mut [0; 4]), font_size))
        .fold(0.0, f64::max);
    (
        widest_char + BOUND_TEXT_PADDING * 2.0,
        font_size * label.line_height() + BOUND_TEXT_PADDING * 2.0,
    )
}

/// How narrow a text of fixed width may be made (Excalidraw's
/// `getMinTextElementWidth`).
pub(crate) fn min_text_width(font_size: f64, measurer: &dyn TextMeasurer) -> f64 {
    measurer.line_width(" ", font_size) + BOUND_TEXT_PADDING * 2.0
}

/// Line breaks as `\n`, and tabs as the eight spaces Excalidraw shows them as.
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "        ")
}

/// How far below the top of its line the baseline of a text is
/// (`getVerticalOffset`): the glyphs are centred in the line as the font's
/// own ascender and descender say.
fn baseline_offset(font_family: u32, font_size: f64, line_height: f64) -> f64 {
    let (units_per_em, ascender, descender) = metrics(font_family);
    let em = font_size / units_per_em;
    let gap = (line_height - em * ascender + em * descender) / 2.0;
    em * ascender + gap
}

/// `(unitsPerEm, ascender, descender)` of the font family as Excalidraw
/// knows it (`FONT_METADATA`); the unknown use Excalifont's.
fn metrics(font_family: u32) -> (f64, f64, f64) {
    match font_family {
        6 => (1000.0, 1011.0, -353.0),  // Nunito
        7 => (1000.0, 923.0, -220.0),   // Lilita One
        8 => (1000.0, 750.0, -250.0),   // Comic Shanns
        2 => (2048.0, 1577.0, -471.0),  // Helvetica
        3 => (2048.0, 1900.0, -480.0),  // Cascadia
        9 => (2048.0, 1854.0, -434.0),  // Liberation Sans
        10 => (2048.0, 1021.0, -287.0), // Assistant
        _ => (1000.0, 886.0, -374.0),   // Excalifont, Virgil
    }
}
