//! What Excalidraw draws for a text Element (`renderElement.ts` and
//! `textElement.ts` at the pinned commit), as geometry: positioned lines of
//! text. The core never lays out glyphs (ADR-0008); it only decides where each
//! line starts, and where a container's label wraps, using the client's
//! [`TextMeasurer`].

use std::cell::OnceCell;
use std::collections::HashMap;

use crate::color::parse_color;
use crate::geometry::{Geometry, TextAlign, TextStyle};
use crate::json::Value;
use crate::linear;
use crate::scene::{Element, ElementType, Scene, VerticalAlign};
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
/// How much of an arrow's width the lines of its label may take.
const ARROW_LABEL_WIDTH_FRACTION: f64 = 0.7;
/// The least an arrow's label may be wide, in font sizes.
const ARROW_LABEL_MIN_WIDTH_IN_FONT_SIZES: f64 = 11.0;

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
    pub(crate) fn of(&self, text: &Element) -> Option<Element<'a>> {
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

/// Whether an Element of `kind` can have a text created in it by the user:
/// shapes and arrows, but not lines (`isTextBindableContainer`).
pub(crate) fn takes_label(kind: ElementType) -> bool {
    matches!(
        kind,
        ElementType::Rectangle | ElementType::Diamond | ElementType::Ellipse | ElementType::Arrow
    )
}

/// The text bound to the container `container_id` as its label, if it has
/// one that is shown.
pub(crate) fn label_of<'a>(scene: &'a Scene, container_id: &str) -> Option<Element<'a>> {
    scene.elements().find(|e| {
        e.kind() == ElementType::Text && !e.is_deleted() && e.container_id() == Some(container_id)
    })
}

/// The label of every container that has one, by the container's id, for
/// looking up many: a label is part of its container for tapping it
/// (`hitElementBoundText`) and, for an arrow, for box-selecting it.
pub(crate) fn labels<'a>(scene: &'a Scene) -> HashMap<&'a str, Element<'a>> {
    scene
        .elements()
        .filter(|e| e.kind() == ElementType::Text && !e.is_deleted())
        .filter_map(|e| e.container_id().map(|container| (container, e)))
        .collect()
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
/// the lines are the ones saved in the file. The label of an arrow sits at
/// the middle of its path (`getBoundTextElementPosition`).
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
    let version = version(element, container.as_ref());

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

/// The version `text` is drawn with: lines change with the container as well
/// as with their own Element.
pub(crate) fn version(text: &Element, container: Option<&Element>) -> u32 {
    container.map_or(text.version(), |c| {
        text.version()
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(c.version())
    })
}

fn layout(
    element: &Element,
    container: Option<&Element>,
    measurer: Option<&dyn TextMeasurer>,
) -> Block {
    let mut saved = Block {
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
    // The file's position of an arrow's label is not what is drawn: the
    // arrow may have moved or bent since.
    if let Some(arrow) = container
        .as_ref()
        .filter(|c| c.kind() == ElementType::Arrow)
    {
        (saved.x, saved.y) = arrow_label_origin(arrow, (saved.width, saved.height));
        saved.angle = 0.0;
    }
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
        Some(container) => {
            let lines = wrapped(bound_max_width(container, element));
            let width = wrap::widest(&lines, font_size, measurer);
            let height = lines.len() as f64 * line_height;
            let (x, y) = label_origin(element, container, (width, height));
            Block {
                lines,
                x,
                y,
                width,
                height,
                angle: label_angle(container),
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

/// How wide the lines of the `label` of `container` may be
/// (`getBoundTextMaxWidth`): for a shape the width of the rectangle that fits
/// in it, less padding; for an arrow a fraction of its width, but at least
/// as wide as a few characters of the label.
pub(crate) fn bound_max_width(container: &Element, label: &Element) -> f64 {
    match container.kind() {
        ElementType::Arrow => arrow_label_width(container.width(), label.font_size()),
        kind => label_max_width(kind, container.width()),
    }
}

/// How wide the lines of the label of an arrow `width` wide set at
/// `font_size` may be.
pub(crate) fn arrow_label_width(width: f64, font_size: f64) -> f64 {
    (ARROW_LABEL_WIDTH_FRACTION * width).max(font_size * ARROW_LABEL_MIN_WIDTH_IN_FONT_SIZES)
}

/// Where the top left of a label of `size` goes in `container`: at the
/// middle of an arrow's path, else in the padded rectangle of a shape.
fn label_origin(label: &Element, container: &Element, size: (f64, f64)) -> (f64, f64) {
    if container.kind() == ElementType::Arrow {
        arrow_label_origin(container, size)
    } else {
        bound_position(label, container, size)
    }
}

/// Where the top left of a label of `(width, height)` goes on `arrow`: the
/// label is centred on the middle of the path.
fn arrow_label_origin(arrow: &Element, (width, height): (f64, f64)) -> (f64, f64) {
    let (x, y) = linear::label_anchor(arrow);
    (x - width / 2.0, y - height / 2.0)
}

/// How far the label of `container` is turned: with a shape, but never with a
/// line or arrow.
pub(crate) fn label_angle(container: &Element) -> f64 {
    if container.kind().is_linear() {
        0.0
    } else {
        container.angle()
    }
}

/// How wide the lines of a label may be in a container of `kind` and `width`.
pub(crate) fn label_max_width(kind: ElementType, width: f64) -> f64 {
    inscribed(kind, width) - BOUND_TEXT_PADDING * 2.0
}

/// How high a label may be (`getBoundTextMaxHeight`).
pub(crate) fn bound_max_height(container: &Element) -> f64 {
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
    let (max_width, max_height) = (
        label_max_width(container.kind(), container.width()),
        bound_max_height(container),
    );

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

/// What a text Element with typed text has to save, laid out as Excalidraw
/// does while it is edited (`refreshTextDimensions`).
pub(crate) struct Typed {
    /// The lines joined by `\n`.
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The text `typed` in the unbound text `element`: a text that grows with its
/// content stays one line per line break and is as wide as the widest, one of
/// fixed width wraps to it. It stays where its alignment anchors it as it
/// grows or shrinks: a left aligned text at its left edge, a centred one at
/// its middle, a right aligned one at its right edge, all at their top
/// (`getAdjustedDimensions`), also when turned.
pub(crate) fn fit_typed(element: &Element, typed: &str, measurer: &dyn TextMeasurer) -> Typed {
    let font_size = element.font_size();
    let typed = normalized(typed);
    let (lines, width) = if element.auto_resize() {
        let lines: Vec<String> = typed.split('\n').map(str::to_owned).collect();
        let width = wrap::widest(&lines, font_size, measurer);
        (lines, width)
    } else {
        let lines = wrap::wrap_text(&typed, font_size, element.width(), measurer);
        (lines, element.width())
    };
    let height = lines.len() as f64 * font_size * element.line_height();
    let (x, y) = anchored_origin(element, (width, height));
    Typed {
        text: lines.join("\n"),
        x,
        y,
        width,
        height,
    }
}

/// The members a text has to save to be as `fit` says, if they are not what
/// it has.
pub(crate) fn members_of(element: &Element, fit: Typed) -> Option<Vec<(&'static str, Value)>> {
    let same = element.text() == fit.text
        && (element.x(), element.y()) == (fit.x, fit.y)
        && (element.width(), element.height()) == (fit.width, fit.height);
    (!same).then(|| {
        vec![
            ("text", Value::String(fit.text)),
            ("x", Value::Number(fit.x)),
            ("y", Value::Number(fit.y)),
            ("width", Value::Number(fit.width)),
            ("height", Value::Number(fit.height)),
        ]
    })
}

/// The members the text `element` has to save to fit its font size and
/// alignment again, if it has to save any (Excalidraw's
/// `redrawTextBoundingBox`): a label sits in its `container` again, any other
/// text is laid out as typed.
pub(crate) fn refit(
    element: &Element,
    container: Option<&Element>,
    measurer: &dyn TextMeasurer,
) -> Option<Vec<(&'static str, Value)>> {
    let fit = match container {
        Some(container) => {
            let fit = fit_label(element, container, Some(measurer));
            Typed {
                text: fit.text.unwrap_or_else(|| element.text().to_owned()),
                x: fit.x,
                y: fit.y,
                width: fit.width,
                height: fit.height,
            }
        }
        None => fit_typed(element, element.original_text(), measurer),
    };
    members_of(element, fit)
}

/// Where the top left of `element` goes when it becomes `(width, height)`,
/// for the point of its top edge its alignment anchors to stay where it is.
fn anchored_origin(element: &Element, (width, height): (f64, f64)) -> (f64, f64) {
    let ratio = anchor_ratio(element.text_align());
    let (old_width, old_height) = (element.width(), element.height());
    let (sin, cos) = element.angle().sin_cos();
    // From the middle of a box of the given size to its anchor, turned.
    let to_anchor = |width: f64, height: f64| {
        let (dx, dy) = (width * (ratio - 0.5), -height / 2.0);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    };
    let (old_to, new_to) = (to_anchor(old_width, old_height), to_anchor(width, height));
    let old_middle = (
        element.x() + old_width / 2.0,
        element.y() + old_height / 2.0,
    );
    let middle = (
        old_middle.0 + old_to.0 - new_to.0,
        old_middle.1 + old_to.1 - new_to.1,
    );
    (middle.0 - width / 2.0, middle.1 - height / 2.0)
}

/// How far along the width of a text its alignment anchors it.
pub(crate) fn anchor_ratio(align: TextAlign) -> f64 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.0,
    }
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
            let lines = set_in_lines(label, bound_max_width(container, label), measurer);
            (Some(lines.text), lines.width, lines.height)
        }
        None => (None, label.width(), label.height()),
    };
    let (x, y) = label_origin(label, container, (width, height));
    LabelFit {
        text,
        x,
        y,
        width,
        height,
    }
}

/// The size a container of `kind` needs to hold a label of `dimension` wide
/// or high (Excalidraw's `computeContainerDimensionForBoundText`).
fn container_dimension(kind: ElementType, dimension: f64) -> f64 {
    let dimension = dimension.ceil();
    let padding = BOUND_TEXT_PADDING * 2.0;
    match kind {
        ElementType::Ellipse => ((dimension + padding) / std::f64::consts::SQRT_2 * 2.0).round(),
        ElementType::Diamond => 2.0 * (dimension + padding),
        _ => dimension + padding,
    }
}

/// How big `container` has to be for its `label` as it is typed
/// (`redrawTextBoundingBox` and the wysiwyg editor of Excalidraw): it grows
/// down to hold more lines than fit and shrinks again as lines are taken
/// away, but not below `original_height`, the height it had when the editing
/// began; it widens for a line that cannot be broken to fit.
pub(crate) fn container_size_for(
    label: &Element,
    container: &Element,
    original_height: f64,
    measurer: &dyn TextMeasurer,
) -> (f64, f64) {
    let kind = container.kind();
    let max_width = bound_max_width(container, label);
    let lines = set_in_lines(label, max_width, measurer);
    let (mut width, mut height) = (container.width(), container.height());
    let max_height = bound_max_height(container);
    if lines.height > max_height {
        height = container_dimension(kind, lines.height);
    } else if height > original_height && lines.height < max_height {
        height = container_dimension(kind, lines.height).max(original_height);
    }
    if lines.width > max_width {
        width = container_dimension(kind, lines.width);
    }
    (width, height)
}

/// How high a container of `kind` that is to be `(width, height)` has to be
/// for its `label` set at `font_size`: `height`, unless the lines need more
/// (Excalidraw's `handleBindTextResize`). With a `measurer` the lines wrap to
/// the width again; without, they stay as they are.
pub(crate) fn height_to_hold(
    label: &Element,
    kind: ElementType,
    (width, height): (f64, f64),
    font_size: f64,
    measurer: Option<&dyn TextMeasurer>,
) -> f64 {
    let lines_height = match measurer {
        Some(measurer) => {
            let lines = wrap::wrap_text(
                &normalized(label.original_text()),
                font_size,
                label_max_width(kind, width),
                measurer,
            );
            lines.len() as f64 * font_size * label.line_height()
        }
        None => label.height() * font_size / label.font_size(),
    };
    if lines_height > inscribed(kind, height) - BOUND_TEXT_PADDING * 2.0 {
        container_dimension(kind, lines_height)
    } else {
        height
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
