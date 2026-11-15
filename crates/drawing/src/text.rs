//! What Excalidraw draws for a text Element (`renderElement.ts` and
//! `textElement.ts` at the pinned commit), as geometry: positioned lines of
//! text. The core never lays out glyphs (ADR-0008); it only decides where each
//! line starts, and where a container's label wraps, using the client's
//! [`TextMeasurer`].

use std::cell::OnceCell;
use std::collections::HashMap;

use crate::color::parse_color;
use crate::geometry::{Geometry, Hole, TextAlign, TextStyle};
use crate::linear;
use crate::scene::{Element, ElementType, Scene, VerticalAlign};
use crate::wrap;
use pkm_base::json::Value;

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

/// Where the `arrow` that `label` is bound to is cut so that it does not run
/// through the label: the label's box with padding around, as Excalidraw
/// clips arrows (`renderElement`). `None` if the label shows nothing.
pub(crate) fn arrow_label_hole(
    label: &Element,
    arrow: &Element,
    measurer: Option<&dyn TextMeasurer>,
) -> Option<Hole> {
    let block = layout(label, Some(arrow), measurer);
    if block.lines.iter().all(String::is_empty) {
        return None;
    }
    Some(Hole {
        x: (block.x - BOUND_TEXT_PADDING) as f32,
        y: (block.y - BOUND_TEXT_PADDING) as f32,
        width: (block.width + 2.0 * BOUND_TEXT_PADDING) as f32,
        height: (block.height + 2.0 * BOUND_TEXT_PADDING) as f32,
    })
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

#[cfg(test)]
mod tests {
    mod arrow_labels {
        //! Labels on arrows (ADR-0006), tested at the Editor seam like those of
        //! shapes (`bound_text.rs`). The only test double is a `TextMeasurer` with
        //! fixed character widths (half the font size).
        //!
        //! Positions and wrapping are checked against labels that Excalidraw itself
        //! laid out (`text.excalidraw`, made by the golden fixture generator); the
        //! other expected values are worked out by hand from Excalidraw's rules
        //! (`getBoundTextMaxWidth`, `LinearElementEditor.getBoundTextElementCenter`):
        //! at font size 20 a character is 10 wide and a line 25 high; the lines of an
        //! arrow's label are at most 0.7 of its width wide, but no less than 11 font
        //! sizes (220); the label is centred on the middle of the arrow's path.

        use std::sync::Arc;

        use crate::{Editor, TextAlign, TextEditor};
        use pkm_base::json::Value;

        use crate::test_support as common;
        use common::{FixedWidth, editor_with, saved_elements, tap};

        fn scene(elements: &[&str]) -> Editor {
            let mut editor = editor_with(elements);
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor
        }

        /// An arrow at (100, 100) through `points`, `width` wide, as saved by
        /// Excalidraw (`roundness` as given).
        fn arrow_through(kind: &str, width: f64, points: &str, roundness: &str) -> String {
            format!(
                r##"{{"id":"a","type":"{kind}","x":100,"y":100,"width":{width},"height":0,"points":{points},
        "roundness":{roundness},"groupIds":[],"version":3,"versionNonce":7}}"##
            )
        }

        /// A straight arrow from (100, 100) to (400, 100); its middle is (250, 100).
        fn arrow() -> String {
            arrow_through("arrow", 300.0, "[[0,0],[300,0]]", "null")
        }

        const ON_ARROW: (f64, f64) = (150.0, 100.0);

        fn double_tap(editor: &mut Editor, at: (f64, f64)) {
            tap(editor, "finger", at, 10.0);
            tap(editor, "finger", at, 10.2);
        }

        fn open(editor: &Editor) -> TextEditor {
            editor.text_editor().expect("a text editor is open")
        }

        fn saved(editor: &Editor, id: &str) -> Value {
            saved_elements(editor)
                .into_iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
                .unwrap_or_else(|| panic!("no element {id}"))
        }

        fn label(editor: &Editor) -> Value {
            let labels: Vec<Value> = saved_elements(editor)
                .into_iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
                .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
                .collect();
            assert_eq!(labels.len(), 1, "{labels:?}");
            labels[0].clone()
        }

        fn number(element: &Value, key: &str) -> f64 {
            common::num(element, key)
        }

        /// The middle of the box of the open editor.
        fn middle(shown: &TextEditor) -> (f64, f64) {
            (shown.x + shown.width / 2.0, shown.y + shown.height / 2.0)
        }

        // --- As Excalidraw lays them out ----------------------------------------------------

        #[test]
        fn arrow_labels_are_wrapped_and_placed_as_excalidraw_does() {
            // The labels of `text.excalidraw` where Excalidraw put them: the same
            // texts, unbound, at the position and with the lines the file saves.
            let fixture = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/golden/text.excalidraw"),
            )
            .unwrap();
            let unbound = fixture.replace(
                r#""containerId": "label-arrow-"#,
                r#""containerId": null, "was": "label-arrow-"#,
            );
            assert_ne!(unbound, fixture);
            let load = |json: &str| {
                let mut editor = Editor::new();
                editor.load(json.as_bytes()).unwrap();
                editor.set_text_measurer(Arc::new(FixedWidth));
                editor.geometry()
            };
            let (labelled, expected) = (load(&fixture), load(&unbound));
            let lines = |geometry: &crate::Geometry| -> Vec<(String, f32, f32)> {
                geometry
                    .text_lines()
                    .iter()
                    .map(|l| (geometry.line_text(l).to_owned(), l.x, l.y))
                    .collect()
            };
            let (actual, expected) = (lines(&labelled), lines(&expected));

            assert_eq!(actual.len(), expected.len());
            for (a, e) in actual.iter().zip(&expected) {
                assert_eq!(a.0, e.0);
                assert!(
                    (a.1 - e.1).abs() < 1e-3 && (a.2 - e.2).abs() < 1e-3,
                    "{a:?} != {e:?}"
                );
            }
        }

        // --- Opening ----------------------------------------------------------------------

        #[test]
        fn a_double_tap_on_an_arrow_opens_a_new_label_at_the_middle_of_its_path() {
            let mut editor = scene(&[&arrow()]);

            double_tap(&mut editor, ON_ARROW);

            let shown = open(&editor);
            assert_eq!(shown.text, "");
            assert_eq!(shown.align, TextAlign::Center);
            assert!(shown.fixed_width);
            // 0.7 * 300 = 210 is less than 11 font sizes, so 220 wide; one line high,
            // centred on (250, 100).
            assert_eq!((shown.x, shown.width), (140.0, 220.0));
            assert_eq!((shown.y, shown.height), (87.5, 25.0));
            assert_eq!(shown.angle, 0.0);
        }

        #[test]
        fn a_long_arrow_gives_its_label_seven_tenths_of_its_width() {
            let mut editor = scene(&[&arrow_through("arrow", 600.0, "[[0,0],[600,0]]", "null")]);

            double_tap(&mut editor, ON_ARROW);

            assert_eq!(open(&editor).width, 420.0);
        }

        #[test]
        fn enter_with_an_arrow_selected_opens_its_label() {
            let mut editor = scene(&[&arrow()]);
            tap(&mut editor, "finger", ON_ARROW, 10.0);
            assert_eq!(editor.selection(), ["a"]);

            editor.edit_selected_text();

            assert_eq!(middle(&open(&editor)), (250.0, 100.0));
        }

        #[test]
        fn a_label_that_is_there_is_edited() {
            let mut editor = scene(&[
                r##"{"id":"a","type":"arrow","x":100,"y":100,"width":300,"height":0,"points":[[0,0],[300,0]],
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
                r##"{"id":"t","type":"text","containerId":"a","fontSize":20,"text":"from a file",
            "originalText":"from a file","x":0,"y":0,"width":110,"height":25,"lineHeight":1.25,
            "textAlign":"center","verticalAlign":"middle","version":3,"versionNonce":7}"##,
            ]);

            double_tap(&mut editor, ON_ARROW);

            let shown = open(&editor);
            assert_eq!(shown.text, "from a file");
            assert_eq!(middle(&shown), (250.0, 100.0));
        }

        #[test]
        fn a_line_takes_no_label() {
            let mut editor = scene(&[&arrow_through("line", 300.0, "[[0,0],[300,0]]", "null")]);

            double_tap(&mut editor, ON_ARROW);

            assert!(editor.text_editor().is_none());
        }

        // --- Where the label sits -----------------------------------------------------------

        #[test]
        fn the_label_of_an_arrow_with_an_odd_number_of_points_is_on_the_middle_one() {
            let bent = arrow_through("arrow", 200.0, "[[0,0],[100,100],[200,0]]", "null");
            let mut editor = scene(&[&bent]);

            double_tap(&mut editor, (150.0, 150.0));

            // The point (100, 100) of the arrow, which is at (100, 100).
            assert_eq!(middle(&open(&editor)), (200.0, 200.0));
        }

        #[test]
        fn the_label_of_an_arrow_with_an_even_number_of_points_is_in_the_middle_of_its_middle_segment()
         {
            let elbow = arrow_through(
                "arrow",
                200.0,
                "[[0,0],[100,0],[100,100],[200,100]]",
                "null",
            );
            let mut editor = scene(&[&elbow]);

            double_tap(&mut editor, (150.0, 100.0));

            // Halfway between (100, 0) and (100, 100) of the arrow.
            assert_eq!(middle(&open(&editor)), (200.0, 150.0));
        }

        #[test]
        fn the_label_of_a_rounded_arrow_is_halfway_along_its_curve() {
            // Symmetric about x = 250, so the middle of its length is there; the
            // inner points are at y = 0 and the curve between them bulges above.
            let rounded = arrow_through(
                "arrow",
                300.0,
                "[[0,0],[100,-100],[200,-100],[300,0]]",
                r#"{"type":2}"#,
            );
            let mut editor = scene(&[&rounded]);
            tap(&mut editor, "finger", (100.0, 100.0), 10.0);
            editor.edit_selected_text();

            let (x, y) = middle(&open(&editor));
            assert!((x - 250.0).abs() < 1e-6, "{x}");
            assert!(y < 0.0 && y > -50.0, "{y}");
        }

        #[test]
        fn a_turned_arrow_has_its_label_turned_about_its_middle_but_upright() {
            // The arrow runs (100, 100) to (400, 100), and a quarter turn about its
            // middle (250, 100) makes it run up from (250, 250) to (250, -50). The
            // label stays upright.
            let turned = arrow().replace(
                r#""roundness""#,
                r#""roughness":0,"angle":1.5707963267948966,"roundness""#,
            );
            let mut editor = scene(&[&turned]);
            tap(&mut editor, "finger", (250.0, 0.0), 10.0);
            assert_eq!(editor.selection(), ["a"]);
            editor.edit_selected_text();

            let shown = open(&editor);
            let (x, y) = middle(&shown);
            assert!(
                (x - 250.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6,
                "{x} {y}"
            );
            assert_eq!(shown.angle, 0.0);
            assert_eq!(shown.angle, 0.0);
        }

        // --- Typing -----------------------------------------------------------------------

        #[test]
        fn the_label_wraps_to_the_arrow_label_width_and_the_arrow_does_not_grow() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);

            // 22 characters make a line of 220: "aaaaaaaaaa bbbbbbbbbb" fits (21),
            // "cccc" does not.
            editor.set_editing_text("aaaaaaaaaa bbbbbbbbbb cccc");
            editor.commit_text();

            let text = label(&editor);
            assert_eq!(common::text(&text, "text"), "aaaaaaaaaa bbbbbbbbbb\ncccc");
            assert_eq!(number(&text, "width"), 210.0);
            assert_eq!(number(&text, "height"), 50.0);
            // Centred on (250, 100).
            assert_eq!(number(&text, "x"), 145.0);
            assert_eq!(number(&text, "y"), 75.0);
            assert_eq!(number(&text, "angle"), 0.0);
            let arrow = saved(&editor, "a");
            assert_eq!(
                (number(&arrow, "width"), number(&arrow, "height")),
                (300.0, 0.0)
            );
        }

        #[test]
        fn committing_binds_the_label_to_the_arrow_and_selects_it() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.commit_text();

            let id = common::text(&label(&editor), "id").to_owned();
            assert_eq!(common::text(&label(&editor), "containerId"), "a");
            let bound = saved(&editor, "a");
            let bound = bound
                .get("boundElements")
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(common::text(&bound[0], "id"), id);
            assert_eq!(editor.selection(), ["a"]);
            assert_eq!(editor.geometry().text_lines().len(), 1);
        }

        #[test]
        fn the_label_moves_with_the_arrow() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.commit_text();
            let before = editor.geometry().text_lines()[0].x;

            common::replay(
                &mut editor,
                "20 down 1 finger 150 100\n20.1 move 1 finger 170 140\n20.2 move 1 finger 200 160\n20.3 up 1 finger 200 160",
            );

            assert_eq!(editor.geometry().text_lines()[0].x, before + 50.0);
            assert_eq!(number(&label(&editor), "x"), 250.0 - 25.0 + 50.0);
        }

        #[test]
        fn the_drawn_label_follows_a_changed_path() {
            // An arrow from the file whose label was saved where the arrow used to
            // be: it is drawn at the middle of the path as it is now.
            let editor = scene(&[
                r##"{"id":"a","type":"arrow","x":100,"y":100,"width":300,"height":0,"points":[[0,0],[300,0]],
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
                r##"{"id":"t","type":"text","containerId":"a","fontSize":20,"text":"x",
            "originalText":"x","x":0,"y":0,"width":10,"height":25,"lineHeight":1.25,
            "textAlign":"center","verticalAlign":"middle","version":3,"versionNonce":7}"##,
            ]);

            // The anchor of a centred line is the middle of the label.
            assert_eq!(editor.geometry().text_lines()[0].x, 250.0);
        }

        // --- The gap in the arrow -----------------------------------------------------------

        /// The holes of the parts of `editor`'s geometry, as `(x, y, width, height)`.
        fn holes(editor: &Editor) -> Vec<(f32, f32, f32, f32)> {
            editor
                .geometry()
                .parts()
                .iter()
                .map(|p| (p.hole_x, p.hole_y, p.hole_width, p.hole_height))
                .collect()
        }

        #[test]
        fn an_arrow_is_cut_where_its_label_is() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.commit_text();

            // "Hello" is 50 by 25, centred on (250, 100), with 5 of padding around
            // (`BOUND_TEXT_PADDING`), as Excalidraw clips the arrow.
            let holes = holes(&editor);
            assert!(!holes.is_empty());
            assert!(
                holes.iter().all(|&h| h == (220.0, 82.5, 60.0, 35.0)),
                "{holes:?}"
            );
        }

        #[test]
        fn an_arrow_without_a_label_is_not_cut() {
            let editor = scene(&[&arrow()]);

            let holes = holes(&editor);
            assert!(!holes.is_empty());
            assert!(holes.iter().all(|h| h.2 == 0.0 && h.3 == 0.0), "{holes:?}");
        }

        #[test]
        fn the_cut_follows_an_edited_label() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.commit_text();
            let before = editor.geometry().parts()[0].version;

            editor.edit_selected_text();
            editor.set_editing_text("Hi");
            editor.commit_text();

            // Clients cache the arrow by version: it must change with the hole.
            assert_ne!(editor.geometry().parts()[0].version, before);
            assert_eq!(holes(&editor)[0], (235.0, 82.5, 30.0, 35.0));
        }

        // --- Undo, cancel, empty ------------------------------------------------------------

        #[test]
        fn creating_an_arrow_label_is_one_undo_step() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.set_editing_text("Hello world");
            editor.commit_text();
            assert!(editor.can_undo());

            editor.undo();

            assert_eq!(editor.geometry().text_lines().len(), 0);
            let bound = saved(&editor, "a");
            let bound = bound.get("boundElements");
            assert!(
                bound.is_none()
                    || bound
                        .and_then(Value::as_array)
                        .is_some_and(<[Value]>::is_empty),
                "{bound:?}"
            );

            editor.redo();
            assert_eq!(common::text(&label(&editor), "text"), "Hello world");
        }

        #[test]
        fn cancelling_a_new_arrow_label_leaves_the_arrow_as_it_was() {
            let mut editor = scene(&[&arrow()]);
            let before = saved(&editor, "a");
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hel");

            editor.cancel_text();

            assert_eq!(saved_elements(&editor).len(), 1);
            assert_eq!(
                saved(&editor, "a").get("boundElements"),
                before.get("boundElements")
            );
        }

        #[test]
        fn an_empty_arrow_label_is_not_kept() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("  ");
            editor.commit_text();

            assert_eq!(saved_elements(&editor).len(), 1);
            assert!(!editor.can_undo());
        }

        #[test]
        fn emptying_an_arrow_label_deletes_it_and_undo_brings_it_back() {
            let mut editor = scene(&[&arrow()]);
            double_tap(&mut editor, ON_ARROW);
            editor.set_editing_text("Hello");
            editor.commit_text();

            editor.edit_selected_text();
            assert_eq!(open(&editor).text, "Hello");
            editor.set_editing_text("");
            editor.commit_text();

            assert_eq!(editor.geometry().text_lines().len(), 0);
            editor.undo();
            assert_eq!(common::text(&label(&editor), "text"), "Hello");
        }
    }

    mod bound_text {
        //! Text inside shapes (ADR-0006), tested at the Editor seam: taps and
        //! commands in, the editor the client shows over the Canvas, the saved
        //! Drawing and the geometry out. The only test double is a `TextMeasurer`
        //! with fixed character widths (half the font size).
        //!
        //! Expected values are worked out by hand from Excalidraw's rules for bound
        //! text (`textElement.ts`): at font size 20 a character is 10 wide and a line
        //! 25 high; the text keeps 5 units of padding inside its container.

        use std::sync::Arc;

        use crate::{Editor, FontSize, Property, TextAlign, TextEditor, Tool};
        use pkm_base::json::Value;

        use crate::test_support as common;
        use common::{FixedWidth, editor_with, saved_elements, tap};

        /// An editor whose Scene is the given Elements, with a measurer.
        fn scene(elements: &[&str]) -> Editor {
            let mut editor = editor_with(elements);
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor
        }

        /// A shape of `kind` at (100, 100), 200 x 100, with its middle at (200, 150).
        fn shape(kind: &str) -> String {
            narrow(kind, 200.0)
        }

        /// A shape of `kind` at (100, 100), `width` x 100.
        fn narrow(kind: &str, width: f64) -> String {
            format!(
                r##"{{"id":"s","type":"{kind}","x":100,"y":100,"width":{width},"height":100,
        "backgroundColor":"transparent","roundness":null,"groupIds":["g"],"version":3,"versionNonce":7}}"##
            )
        }

        fn scene_of(kind: &str) -> Editor {
            scene(&[&shape(kind)])
        }

        const MIDDLE: (f64, f64) = (200.0, 150.0);

        fn double_tap(editor: &mut Editor, at: (f64, f64)) {
            double_tap_at(editor, at, 10.0);
        }

        fn double_tap_at(editor: &mut Editor, at: (f64, f64), time: f64) {
            tap(editor, "finger", at, time);
            tap(editor, "finger", at, time + 0.2);
        }

        fn open(editor: &Editor) -> TextEditor {
            editor.text_editor().expect("a text editor is open")
        }

        /// The Element `id` as saved.
        fn saved(editor: &Editor, id: &str) -> Value {
            saved_elements(editor)
                .into_iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
                .unwrap_or_else(|| panic!("no element {id}"))
        }

        /// The one text of the Scene that is not deleted.
        fn label(editor: &Editor) -> Value {
            let labels: Vec<Value> = saved_elements(editor)
                .into_iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
                .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
                .collect();
            assert_eq!(labels.len(), 1, "{labels:?}");
            labels[0].clone()
        }

        fn number(element: &Value, key: &str) -> f64 {
            element
                .get(key)
                .and_then(Value::as_f64)
                .unwrap_or_else(|| panic!("no number {key}"))
        }

        fn string<'a>(element: &'a Value, key: &str) -> &'a str {
            element
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("no string {key}"))
        }

        // --- Creating --------------------------------------------------------------------

        #[test]
        fn a_double_tap_on_a_shape_opens_the_editor_in_it() {
            for kind in ["rectangle", "diamond", "ellipse"] {
                let mut editor = scene_of(kind);

                double_tap(&mut editor, MIDDLE);

                let shown = open(&editor);
                assert_eq!(shown.text, "", "{kind}");
                assert_eq!(shown.align, TextAlign::Center, "{kind}");
                assert!(shown.fixed_width, "{kind}");
            }
        }

        #[test]
        fn the_editor_of_a_label_is_the_width_the_text_wraps_to_centred_in_the_shape() {
            // The width of a rectangle less 5 on both sides; the first line is
            // centred vertically in the same way.
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            let shown = open(&editor);
            assert_eq!((shown.x, shown.width), (105.0, 190.0));
            assert_eq!((shown.y, shown.height), (137.5, 25.0));

            // The largest rectangle inside an ellipse is 2 / sqrt(2) = 1.414 times
            // the radius: round(200 / 2 * 1.4142) = 141, less padding 131.
            let mut editor = scene_of("ellipse");
            double_tap(&mut editor, MIDDLE);
            let shown = open(&editor);
            assert_eq!(shown.width, 131.0);
            // It starts 5 in from the left of that rectangle, which is
            // 100 * (1 - 1 / sqrt(2)) = 29.29 in from the left of the ellipse.
            assert!((shown.x - 134.2893).abs() < 1e-3, "{}", shown.x);

            // And inside a diamond half the width: 100 less padding.
            let mut editor = scene_of("diamond");
            double_tap(&mut editor, MIDDLE);
            let shown = open(&editor);
            assert_eq!(shown.width, 90.0);
            assert_eq!(shown.x, 155.0);
        }

        #[test]
        fn committing_binds_a_label_to_the_shape() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            editor.commit_text();

            assert!(editor.text_editor().is_none());
            let label = label(&editor);
            assert_eq!(string(&label, "containerId"), "s");
            assert_eq!(string(&label, "text"), "Hello");
            assert_eq!(string(&label, "originalText"), "Hello");
            assert_eq!(string(&label, "textAlign"), "center");
            assert_eq!(string(&label, "verticalAlign"), "middle");
            assert_eq!(number(&label, "fontSize"), 20.0);
            // 50 x 25 in the middle of the shape.
            assert_eq!(
                (number(&label, "width"), number(&label, "height")),
                (50.0, 25.0)
            );
            assert_eq!((number(&label, "x"), number(&label, "y")), (175.0, 137.5));
            // The label is in the group of the shape.
            let groups = label.get("groupIds").and_then(Value::as_array).unwrap();
            assert_eq!(groups, [Value::String("g".into())]);

            // The shape lists it.
            let bound = saved(&editor, "s");
            let bound = bound
                .get("boundElements")
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(bound.len(), 1);
            assert_eq!(string(&bound[0], "type"), "text");
            assert_eq!(string(&bound[0], "id"), string(&label, "id"));
        }

        #[test]
        fn a_drawing_saved_while_a_new_label_is_edited_has_it_bound_both_ways() {
            // As Excalidraw lists it when the editing begins: autosave may write the
            // Drawing at any time, and leaving the app may end with that save.
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hel");

            let id = string(&label(&editor), "id").to_owned();
            let shape = saved(&editor, "s");
            let bound = shape
                .get("boundElements")
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(bound.len(), 1);
            assert_eq!(string(&bound[0], "id"), id);

            // Cancelling, or leaving it empty, takes it off again.
            editor.cancel_text();
            let shape = saved(&editor, "s");
            let before = saved(&scene_of("rectangle"), "s");
            assert_eq!(shape.get("boundElements"), before.get("boundElements"));
        }

        #[test]
        fn the_label_is_right_above_its_shape_and_the_shape_is_selected() {
            let mut editor = scene(&[
                &shape("rectangle"),
                r##"{"id":"o","type":"rectangle","x":500,"y":500,"width":10,"height":10}"##,
            ]);
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            editor.commit_text();

            let ids: Vec<String> = saved_elements(&editor)
                .iter()
                .map(|e| string(e, "id").to_owned())
                .collect();
            assert_eq!(ids[0], "s");
            assert_eq!(ids[2], "o");
            assert_eq!(editor.selection(), ["s"]);
        }

        #[test]
        fn the_label_is_drawn_and_not_while_it_is_edited() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            assert_eq!(editor.geometry().text_lines().len(), 0);

            editor.commit_text();
            assert_eq!(editor.geometry().text_lines().len(), 1);
        }

        #[test]
        fn creating_a_label_is_one_undo_step() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            editor.set_editing_text("Hello world");
            editor.commit_text();
            assert!(editor.can_undo());

            editor.undo();

            assert!(!editor.can_undo());
            assert_eq!(editor.geometry().text_lines().len(), 0);
            let shape = saved(&editor, "s");
            let bound = shape.get("boundElements");
            assert!(
                bound.is_none()
                    || bound
                        .and_then(Value::as_array)
                        .is_some_and(|b| b.is_empty()),
                "{bound:?}"
            );

            editor.redo();
            assert_eq!(string(&label(&editor), "text"), "Hello world");
            assert_eq!(editor.geometry().text_lines().len(), 1);
        }

        #[test]
        fn an_empty_label_is_not_kept() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("  ");
            editor.commit_text();

            assert_eq!(saved_elements(&editor).len(), 1);
            assert!(!editor.can_undo());
            assert_eq!(editor.selection(), ["s"]);
        }

        #[test]
        fn cancelling_keeps_nothing_of_the_label() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            editor.cancel_text();

            assert_eq!(saved_elements(&editor).len(), 1);
            assert!(!editor.can_undo());
        }

        #[test]
        fn a_tap_elsewhere_commits_the_label() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");

            tap(&mut editor, "finger", (600.0, 600.0), 20.0);

            assert!(editor.text_editor().is_none());
            assert_eq!(string(&label(&editor), "text"), "Hello");
        }

        #[test]
        fn a_double_tap_on_the_empty_inside_of_a_hollow_shape_edits_too() {
            // The stroke is far from the tap: the inside of a transparent shape
            // counts as the shape for text.
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, (150.0, 130.0));
            assert!(editor.text_editor().is_some());
        }

        #[test]
        fn a_double_tap_beside_the_shape_makes_a_free_text_not_a_label() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, (600.0, 600.0));
            editor.set_editing_text("Hi");
            editor.commit_text();
            assert_eq!(label(&editor).get("containerId"), Some(&Value::Null));
        }

        #[test]
        fn the_tool_stays_the_selection_tool() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            assert_eq!(editor.tool(), Tool::Selection);
        }

        // --- Wrapping and alignment -------------------------------------------------------

        /// The lines of the one label as saved.
        fn lines(editor: &Editor) -> Vec<String> {
            string(&label(editor), "text")
                .split('\n')
                .map(str::to_owned)
                .collect()
        }

        /// An editor with `typed` typed into a label of a shape of `kind`, `width`
        /// wide.
        fn labelled(kind: &str, width: f64, typed: &str) -> Editor {
            let mut editor = scene(&[&narrow(kind, width)]);
            double_tap(&mut editor, (100.0 + width / 2.0, 150.0));
            editor.set_editing_text(typed);
            editor.commit_text();
            editor
        }

        #[test]
        fn a_label_wraps_to_the_width_of_the_shape_less_padding() {
            // 100 wide, 90 inside: nine characters of 10.
            let editor = labelled("rectangle", 100.0, "hello world foo");
            assert_eq!(lines(&editor), ["hello", "world foo"]);
            let label = label(&editor);
            assert_eq!(
                (number(&label, "width"), number(&label, "height")),
                (90.0, 50.0)
            );
            // Its original text is what was typed.
            assert_eq!(string(&label, "originalText"), "hello world foo");
        }

        #[test]
        fn a_label_in_an_ellipse_or_diamond_wraps_to_the_rectangle_inside_it() {
            // 131 inside the ellipse of width 200: thirteen characters.
            let editor = labelled("ellipse", 200.0, "hello wonder ful");
            assert_eq!(lines(&editor), ["hello wonder", "ful"]);
            // 90 inside the diamond: nine.
            let editor = labelled("diamond", 200.0, "hello wonder ful");
            assert_eq!(lines(&editor), ["hello", "wonder", "ful"]);
            // And 190 in the rectangle: all of it.
            let editor = labelled("rectangle", 200.0, "hello wonder ful");
            assert_eq!(lines(&editor), ["hello wonder ful"]);
        }

        #[test]
        fn a_label_is_centred_in_the_shape_as_it_grows() {
            let editor = labelled("rectangle", 100.0, "hello world foo");
            let label = label(&editor);
            // 90 x 50 in the 90 x 90 inside of the shape.
            assert_eq!((number(&label, "x"), number(&label, "y")), (105.0, 125.0));
        }

        #[test]
        fn the_editor_follows_the_label_as_it_is_typed() {
            let mut editor = scene(&[&narrow("rectangle", 100.0)]);
            double_tap(&mut editor, (150.0, 150.0));
            editor.set_editing_text("hello world foo");

            let shown = open(&editor);
            assert_eq!(shown.text, "hello world foo");
            // The text view is as wide as the lines may be and as high as they are.
            assert_eq!((shown.x, shown.width), (105.0, 90.0));
            assert_eq!((shown.y, shown.height), (125.0, 50.0));
        }

        #[test]
        fn the_editor_of_a_label_follows_the_viewport() {
            let mut editor = scene_of("rectangle");
            editor.set_view_size(800.0, 600.0);
            editor.zoom_in();
            let zoom = editor.viewport().zoom();
            let (ox, oy) = editor.viewport().origin();
            double_tap(
                &mut editor,
                ((MIDDLE.0 - ox) * zoom, (MIDDLE.1 - oy) * zoom),
            );
            let shown = open(&editor);
            assert!((shown.width - 190.0 * zoom).abs() < 1e-9);
            assert!((shown.font_size - 20.0 * zoom).abs() < 1e-9);
        }

        #[test]
        fn a_left_aligned_label_starts_at_the_left_of_the_shape() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");
            editor.set_property(Property::TextAlign(TextAlign::Left));

            // The editor is the full width, the text is aligned in it.
            let shown = open(&editor);
            assert_eq!(shown.align, TextAlign::Left);
            assert_eq!((shown.x, shown.width), (105.0, 190.0));
            editor.commit_text();
            assert_eq!(number(&label(&editor), "x"), 105.0);
        }

        #[test]
        fn a_label_can_be_aligned_and_resized_while_it_is_edited() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello");

            editor.set_property(Property::TextAlign(TextAlign::Right));
            editor.set_property(Property::FontSize(FontSize::Large));
            editor.commit_text();

            let label = label(&editor);
            assert_eq!(string(&label, "textAlign"), "right");
            assert_eq!(number(&label, "fontSize"), 28.0);
            // Five characters of 14, right at the inside of the shape.
            assert_eq!(
                (number(&label, "width"), number(&label, "height")),
                (70.0, 35.0)
            );
            assert_eq!((number(&label, "x"), number(&label, "y")), (225.0, 132.5));
        }

        // --- The shape grows with the text ---------------------------------------------------

        fn height_of(editor: &Editor, id: &str) -> f64 {
            number(&saved(editor, id), "height")
        }

        #[test]
        fn the_shape_grows_to_hold_more_lines_than_fit() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);

            // Three lines are 75 high, more than the 90 the shape has inside; five
            // are 125.
            editor.set_editing_text("a\nb\nc");
            assert_eq!(height_of(&editor, "s"), 100.0);
            editor.set_editing_text("a\nb\nc\nd\ne");
            // 125 + 2 * 5 padding.
            assert_eq!(height_of(&editor, "s"), 135.0);
            assert_eq!(number(&saved(&editor, "s"), "y"), 100.0);
            // And the text stays in the middle of it.
            assert_eq!(open(&editor).y, 100.0 + 5.0 + (125.0 - 125.0) / 2.0);
        }

        #[test]
        fn the_shape_shrinks_again_as_text_is_taken_away_but_not_below_where_it_began() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");
            assert_eq!(height_of(&editor, "s"), 135.0);

            editor.set_editing_text("a\nb\nc\nd");
            assert_eq!(height_of(&editor, "s"), 110.0);
            editor.set_editing_text("a");
            assert_eq!(height_of(&editor, "s"), 100.0);
        }

        #[test]
        fn an_ellipse_and_a_diamond_grow_by_the_rectangle_that_fits_in_them() {
            // Ellipse: round((125 + 10) / sqrt(2) * 2) = 191.
            let mut editor = scene_of("ellipse");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");
            assert_eq!(height_of(&editor, "s"), 191.0);

            // Diamond: twice the 125 + 10.
            let mut editor = scene_of("diamond");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");
            assert_eq!(height_of(&editor, "s"), 270.0);
        }

        #[test]
        fn growing_and_labelling_are_one_undo_step_and_cancelling_gives_the_height_back() {
            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");
            editor.cancel_text();
            assert_eq!(height_of(&editor, "s"), 100.0);
            assert_eq!(saved_elements(&editor).len(), 1);

            let mut editor = scene_of("rectangle");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");
            editor.commit_text();
            assert_eq!(height_of(&editor, "s"), 135.0);
            editor.undo();
            assert_eq!(height_of(&editor, "s"), 100.0);
            editor.redo();
            assert_eq!(height_of(&editor, "s"), 135.0);
        }

        #[test]
        fn arrows_bound_to_the_shape_follow_it_as_it_grows() {
            let mut editor = scene(&[
                &shape("rectangle").replace(
                    r#""version":3"#,
                    r#""boundElements":[{"type":"arrow","id":"a"}],"version":3"#,
                ),
                r##"{"id":"a","type":"arrow","x":200,"y":300,"width":0,"height":94,
            "points":[[0,0],[0,-94]],"startBinding":null,
            "endBinding":{"elementId":"s","focus":0,"gap":5,"fixedPoint":[0.5,1],"mode":"orbit"},
            "version":3,"versionNonce":7}"##,
            ]);
            let before = saved(&editor, "a");
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("a\nb\nc\nd\ne");

            // The bottom of the shape moved 35 down, and its arrow end with it.
            let after = saved(&editor, "a");
            assert_ne!(before, after);
            let end = |arrow: &Value| {
                let points = common::saved_points(arrow);
                let last = points[points.len() - 1];
                (number(arrow, "x") + last.0, number(arrow, "y") + last.1)
            };
            assert!(
                (end(&after).1 - end(&before).1 - 35.0).abs() < 1e-6,
                "{before:?} {after:?}"
            );

            editor.commit_text();
            editor.undo();
            assert_eq!(saved(&editor, "a").get("points"), before.get("points"));
        }

        // --- Styling a selected shape with a label ----------------------------------------------
        //
        // As on excalidraw.com (`changeProperty(..., includeBoundText)`), the
        // properties panel reaches the label of a selected shape: its colour,
        // opacity, font size and alignment.

        #[test]
        fn stroke_colour_and_opacity_of_a_selected_shape_reach_its_label() {
            let mut editor = labelled("rectangle", 200.0, "Hello");
            assert_eq!(editor.selection(), ["s"]);

            editor.set_property(Property::StrokeColor("#e03131".into()));
            editor.set_property(Property::Opacity(40.0));

            let label = label(&editor);
            assert_eq!(string(&label, "strokeColor"), "#e03131");
            assert_eq!(number(&label, "opacity"), 40.0);
            assert_eq!(string(&saved(&editor, "s"), "strokeColor"), "#e03131");

            editor.undo();
            assert_eq!(number(&self::label(&editor), "opacity"), 100.0);
        }

        #[test]
        fn the_panel_offers_the_text_options_of_the_label_of_a_selected_shape() {
            let editor = labelled("rectangle", 200.0, "Hello");
            let shown = editor.properties();
            assert!(shown.sections.text);
            assert_eq!(shown.font_size, Some(FontSize::Medium));
            assert_eq!(shown.text_align, Some(TextAlign::Center));
        }

        #[test]
        fn a_larger_font_grows_the_selected_shape_to_hold_its_label_in_one_undo_step() {
            let mut editor = labelled("rectangle", 200.0, "a\nb\nc");
            assert_eq!(height_of(&editor, "s"), 100.0);

            editor.set_property(Property::FontSize(FontSize::ExtraLarge));

            // Three lines of 36 * 1.25, and 5 of padding above and below.
            let label = label(&editor);
            assert_eq!(number(&label, "fontSize"), 36.0);
            assert_eq!(number(&label, "height"), 135.0);
            assert_eq!(height_of(&editor, "s"), 145.0);
            assert_eq!(number(&label, "y"), 105.0);

            editor.undo();
            assert_eq!(height_of(&editor, "s"), 100.0);
            assert_eq!(number(&self::label(&editor), "fontSize"), 20.0);
        }

        #[test]
        fn aligning_the_label_of_a_selected_shape_moves_it_within_the_shape() {
            let mut editor = labelled("rectangle", 200.0, "Hello");
            editor.set_property(Property::TextAlign(TextAlign::Left));
            let label = label(&editor);
            assert_eq!(string(&label, "textAlign"), "left");
            assert_eq!(number(&label, "x"), 105.0);
        }

        // --- Editing a label that is there ------------------------------------------------------

        /// A rectangle at (100, 100), 200 x 100, with the label `hi`, as a file from
        /// excalidraw.com has it.
        const FILE: [&str; 2] = [
            r##"{"id":"s","type":"rectangle","x":100,"y":100,"width":200,"height":100,
        "angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
        "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],
        "frameId":null,"index":"a0","roundness":{"type":3},"seed":1,"version":4,
        "versionNonce":2,"isDeleted":false,"boundElements":[{"type":"text","id":"t"}],
        "updated":1,"link":null,"locked":false}"##,
            r##"{"id":"t","type":"text","x":175,"y":137.5,"width":50,"height":25,"angle":0,
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
        "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],
        "frameId":null,"index":"a1","roundness":null,"seed":3,"version":2,"versionNonce":4,
        "isDeleted":false,"boundElements":null,"updated":1,"link":null,"locked":false,
        "text":"Hello","fontSize":20,"fontFamily":5,"textAlign":"center",
        "verticalAlign":"middle","containerId":"s","originalText":"Hello","autoResize":true,
        "lineHeight":1.25}"##,
        ];

        #[test]
        fn a_double_tap_on_a_shape_with_a_label_edits_the_label() {
            let mut editor = scene(&FILE);

            double_tap(&mut editor, MIDDLE);

            assert_eq!(open(&editor).text, "Hello");
            // Not a second label.
            editor.commit_text();
            assert_eq!(saved_elements(&editor).len(), 2);
        }

        #[test]
        fn a_label_that_is_not_changed_leaves_the_file_as_it_was() {
            let mut editor = scene(&FILE);
            let before = editor.save();

            double_tap(&mut editor, MIDDLE);
            editor.commit_text();

            assert_eq!(editor.save(), before);
            assert!(!editor.can_undo());
        }

        #[test]
        fn a_label_from_a_file_is_edited_and_saved_as_excalidraw_saves_it() {
            let mut editor = scene(&FILE);
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hello world, how are you doing");
            editor.commit_text();

            let label = saved(&editor, "t");
            // 190 inside: nineteen characters, and "Hello world, how are" has twenty.
            assert_eq!(string(&label, "text"), "Hello world, how\nare you doing");
            assert_eq!(
                string(&label, "originalText"),
                "Hello world, how are you doing"
            );
            assert_eq!(string(&label, "containerId"), "s");
            assert_eq!(number(&label, "seed"), 3.0);
            // The shape still lists it once.
            let shape = saved(&editor, "s");
            assert_eq!(
                shape
                    .get("boundElements")
                    .and_then(Value::as_array)
                    .unwrap()
                    .len(),
                1
            );
            // Reloading gives the same lines.
            let mut again = scene(&[]);
            again.load(&editor.save()).unwrap();
            assert_eq!(again.save(), editor.save());
        }

        #[test]
        fn emptying_a_label_deletes_it_and_the_shape_stops_listing_it() {
            let mut editor = scene(&FILE);
            double_tap(&mut editor, MIDDLE);
            editor.set_editing_text("");
            editor.commit_text();

            assert_eq!(
                saved(&editor, "t").get("isDeleted"),
                Some(&Value::Bool(true))
            );
            let shape = saved(&editor, "s");
            assert_eq!(
                shape
                    .get("boundElements")
                    .and_then(Value::as_array)
                    .map(<[Value]>::len),
                Some(0)
            );
            assert_eq!(editor.geometry().text_lines().len(), 0);

            editor.undo();
            assert_eq!(string(&label(&editor), "text"), "Hello");
            assert_eq!(
                saved(&editor, "s")
                    .get("boundElements")
                    .and_then(Value::as_array)
                    .map(<[Value]>::len),
                Some(1)
            );
        }

        // --- Enter ------------------------------------------------------------------------------

        #[test]
        fn enter_on_a_selected_shape_edits_its_label() {
            let mut editor = scene_of("ellipse");
            tap(&mut editor, "mouse", (100.0, 150.0), 0.0);
            assert_eq!(editor.selection(), ["s"]);

            editor.edit_selected_text();

            assert_eq!(open(&editor).text, "");
            editor.set_editing_text("Hi");
            editor.commit_text();
            assert_eq!(string(&label(&editor), "containerId"), "s");
            assert_eq!(editor.selection(), ["s"]);
        }

        #[test]
        fn enter_on_a_selected_shape_with_a_label_edits_that_label() {
            let mut editor = scene(&FILE);
            tap(&mut editor, "mouse", (101.0, 150.0), 0.0);
            assert_eq!(editor.selection(), ["s"]);

            editor.edit_selected_text();

            assert_eq!(open(&editor).text, "Hello");
        }

        #[test]
        fn enter_on_a_selected_text_edits_it() {
            let mut editor = scene(&[
                r##"{"id":"t","type":"text","x":100,"y":50,"width":50,"height":25,"fontSize":20,
        "fontFamily":5,"lineHeight":1.25,"textAlign":"left","verticalAlign":"top",
        "text":"Hello","originalText":"Hello","containerId":null,"autoResize":true}"##,
            ]);
            tap(&mut editor, "mouse", (120.0, 60.0), 0.0);

            editor.edit_selected_text();

            assert_eq!(open(&editor).text, "Hello");
        }

        #[test]
        fn enter_does_nothing_without_exactly_one_shape_or_text_selected() {
            let mut editor = scene(&[
                &shape("rectangle"),
                r##"{"id":"o","type":"rectangle","x":400,"y":100,"width":50,"height":50,
        "backgroundColor":"#ffc9c9","fillStyle":"solid"}"##,
                r##"{"id":"l","type":"line","x":500,"y":100,"width":50,"height":50,"points":[[0,0],[50,50]]}"##,
            ]);
            editor.edit_selected_text();
            assert!(editor.text_editor().is_none());

            editor.select_all();
            editor.edit_selected_text();
            assert!(editor.text_editor().is_none());

            tap(&mut editor, "mouse", (500.0, 100.0), 5.0);
            editor.edit_selected_text();
            assert!(editor.text_editor().is_none());
        }

        // --- Touch --------------------------------------------------------------------------------

        #[test]
        fn a_double_tap_goes_into_a_selected_group_before_it_edits_a_label() {
            let mut editor = scene(&[
                &shape("rectangle"),
                r##"{"id":"o","type":"rectangle","x":400,"y":100,"width":50,"height":50,
        "backgroundColor":"#ffc9c9","fillStyle":"solid","groupIds":["g"]}"##,
            ]);
            let edge = (150.0, 100.0);
            tap(&mut editor, "finger", edge, 0.0);
            assert_eq!(editor.selection().len(), 2);

            double_tap_at(&mut editor, edge, 5.0);
            assert!(editor.text_editor().is_none());
            assert_eq!(editor.selection(), ["s"]);

            // A double-tap on the Element that is alone now edits it.
            double_tap_at(&mut editor, edge, 10.0);
            assert!(editor.text_editor().is_some());
        }

        #[test]
        fn a_single_tap_on_a_shape_only_selects_it() {
            let mut editor = scene_of("rectangle");
            tap(&mut editor, "finger", MIDDLE, 0.0);
            assert!(editor.text_editor().is_none());
        }

        #[test]
        fn without_a_text_measurer_no_label_is_edited() {
            let mut editor = editor_with(&[&shape("rectangle")]);
            double_tap(&mut editor, MIDDLE);
            assert!(editor.text_editor().is_none());
        }

        // --- Resizing the shape ----------------------------------------------------------------------

        #[test]
        fn resizing_the_shape_wraps_the_label_to_it_again() {
            let mut editor = labelled("rectangle", 200.0, "hello world foo bar");
            assert_eq!(lines(&editor), ["hello world foo bar"]);
            tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
            assert_eq!(editor.selection(), ["s"]);

            // The south east handle is 4 outside the corner, and keeps that distance
            // to it: the shape is 150 wide, 140 inside, fourteen characters.
            common::replay(
                &mut editor,
                "30 down 1 mouse 304 204
         30.1 move 1 mouse 280 204
         30.2 move 1 mouse 254 204
         30.3 up 1 mouse 254 204",
            );

            assert_eq!(number(&saved(&editor, "s"), "width"), 150.0);
            assert_eq!(lines(&editor), ["hello world", "foo bar"]);
            let label = label(&editor);
            // Two lines of the longest, 11 characters, in the middle of the 100 high
            // shape and of its 140 inside.
            assert_eq!(
                (number(&label, "width"), number(&label, "height")),
                (110.0, 50.0)
            );
            assert_eq!((number(&label, "x"), number(&label, "y")), (120.0, 125.0));
            // What was typed is kept for wrapping it again.
            assert_eq!(string(&label, "originalText"), "hello world foo bar");

            // And wider again, it is one line.
            common::replay(
                &mut editor,
                "40 down 1 mouse 254 204
         40.1 move 1 mouse 280 204
         40.2 move 1 mouse 304 204
         40.3 up 1 mouse 304 204",
            );
            assert_eq!(lines(&editor), ["hello world foo bar"]);
        }

        #[test]
        fn a_shape_made_too_narrow_for_its_label_grows_down_to_hold_the_lines() {
            let mut editor = labelled("rectangle", 200.0, "aaaa bbbb cccc dddd");
            tap(&mut editor, "mouse", (150.0, 100.0), 20.0);

            // 60 wide, 50 inside: a word a line, four lines of 25 are 100 high, more
            // than the 90 inside the shape (`handleBindTextResize`). The top stays.
            common::replay(
                &mut editor,
                "30 down 1 mouse 304 204
         30.1 move 1 mouse 200 204
         30.2 move 1 mouse 164 204
         30.3 up 1 mouse 164 204",
            );

            let shape = saved(&editor, "s");
            assert_eq!(number(&shape, "width"), 60.0);
            assert_eq!(
                (number(&shape, "y"), number(&shape, "height")),
                (100.0, 110.0)
            );
            assert_eq!(lines(&editor), ["aaaa", "bbbb", "cccc", "dddd"]);
            assert_eq!(number(&label(&editor), "y"), 105.0);
        }

        #[test]
        fn a_shape_made_too_low_for_its_label_keeps_the_height_of_the_lines() {
            // Two lines of 25 in a shape 100 x 100.
            let resized = |handle: (f64, f64), to: (f64, f64)| {
                let mut editor = labelled("rectangle", 100.0, "hello world foo");
                tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
                common::replay(
                    &mut editor,
                    &format!(
                        "30 down 1 mouse {} {}
                 30.1 move 1 mouse {} {}
                 30.2 up 1 mouse {} {}",
                        handle.0, handle.1, to.0, to.1, to.0, to.1
                    ),
                );
                let shape = saved(&editor, "s");
                (number(&shape, "y"), number(&shape, "height"))
            };

            // Pulled up by the bottom to 40 high, it is 50 + 2 * 5 padding high, from
            // its top.
            assert_eq!(resized((150.0, 204.0), (150.0, 144.0)), (100.0, 60.0));
            // Pushed down by the top, from its bottom.
            assert_eq!(resized((150.0, 96.0), (150.0, 156.0)), (140.0, 60.0));
        }

        #[test]
        fn a_label_is_centred_vertically_whatever_the_height_of_the_shape() {
            let mut editor = labelled("rectangle", 200.0, "Hi");
            let label = label(&editor);
            assert_eq!(string(&label, "verticalAlign"), "middle");
            // 25 high in the 90 inside of a shape that is 100 high.
            assert_eq!(number(&label, "y"), 137.5);

            tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
            // Dragging the south handle down 100 makes the shape 200 high.
            common::replay(
                &mut editor,
                "30 down 1 mouse 200 204
         30.1 move 1 mouse 200 254
         30.2 move 1 mouse 200 304
         30.3 up 1 mouse 200 304",
            );
            assert_eq!(number(&saved(&editor, "s"), "height"), 200.0);
            assert_eq!(number(&common::saved_elements(&editor)[1], "y"), 187.5);
        }

        // --- Excalidraw's own labels ----------------------------------------------------------------------

        #[test]
        fn editing_lays_a_label_out_as_excalidraw_did_in_its_own_fixture() {
            // `text.excalidraw` was made by Excalidraw's own code with the same fixed
            // character widths: every label in it sits as Excalidraw laid it out, in
            // rectangles, ellipses and diamonds, turned or not, at every alignment.
            let mut editor = common::editor_with_fixture("text");
            editor.set_text_measurer(Arc::new(FixedWidth));
            let originals = saved_elements(&editor);
            let labels: Vec<&Value> = originals
                .iter()
                .filter(|e| e.get("containerId").and_then(Value::as_str).is_some())
                .collect();
            assert!(labels.len() >= 10, "{} labels", labels.len());

            for original in labels {
                let container = string(original, "containerId");
                let c = saved(&editor, container);
                // Double-tap the middle of the container: it is the label's.
                let middle = (
                    number(&c, "x") + number(&c, "width") / 2.0,
                    number(&c, "y") + number(&c, "height") / 2.0,
                );
                let mut fresh = common::editor_with_fixture("text");
                fresh.set_text_measurer(Arc::new(FixedWidth));
                double_tap(&mut fresh, middle);
                let typed = string(original, "originalText").to_owned();
                fresh.set_editing_text("changed");
                fresh.set_editing_text(&typed);
                fresh.commit_text();

                let now = saved(&fresh, string(original, "id"));
                let id = string(original, "id");
                assert_eq!(string(&now, "text"), string(original, "text"), "{id}");
                for key in ["x", "y", "width", "height"] {
                    let (a, b) = (number(&now, key), number(original, key));
                    assert!((a - b).abs() < 1e-3, "{id}.{key}: {a} != {b}");
                }
                let c_now = saved(&fresh, container);
                assert_eq!(
                    number(&c_now, "height"),
                    number(&c, "height"),
                    "{container}"
                );
            }
        }

        // --- The text Tool on shapes ---------------------------------------------------------

        fn text_tap(editor: &mut Editor, at: (f64, f64)) {
            editor.set_tool(Tool::Text);
            tap(editor, "finger", at, 50.0);
        }

        fn live_texts(editor: &Editor) -> usize {
            saved_elements(editor)
                .iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
                .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
                .count()
        }

        #[test]
        fn a_text_tool_tap_near_the_middle_of_a_shape_makes_its_label() {
            for kind in ["rectangle", "diamond", "ellipse"] {
                let mut editor = scene_of(kind);
                text_tap(&mut editor, (MIDDLE.0 + 20.0, MIDDLE.1 - 20.0));
                let shown = open(&editor);
                assert!(shown.fixed_width, "{kind}");
                assert_eq!(shown.align, TextAlign::Center, "{kind}");
                editor.set_editing_text("Hi");
                editor.commit_text();
                assert_eq!(string(&label(&editor), "containerId"), "s", "{kind}");
            }
        }

        #[test]
        fn a_text_tool_tap_on_a_label_edits_it() {
            let mut editor = labelled("rectangle", 200.0, "Hi");
            text_tap(&mut editor, MIDDLE);
            assert_eq!(open(&editor).text, "Hi");
            editor.set_editing_text("Hello");
            editor.commit_text();
            assert_eq!(live_texts(&editor), 1);
            assert_eq!(string(&label(&editor), "text"), "Hello");
        }

        #[test]
        fn a_text_tool_tap_in_a_shape_but_away_from_the_middle_makes_a_free_text() {
            let mut editor = scene_of("rectangle");
            text_tap(&mut editor, (110.0, 110.0));
            assert!(!open(&editor).fixed_width);
            editor.set_editing_text("Free");
            editor.commit_text();
            assert!(
                label(&editor)
                    .get("containerId")
                    .is_none_or(|v| v.as_str().is_none())
            );
        }

        #[test]
        fn a_text_tool_tap_on_a_labelled_shape_away_from_its_label_makes_a_free_text() {
            let mut editor = labelled("rectangle", 200.0, "Hi");
            text_tap(&mut editor, (110.0, 110.0));
            assert_eq!(open(&editor).text, "");
            assert!(!open(&editor).fixed_width);
        }

        #[test]
        fn a_text_tool_tap_on_empty_canvas_makes_a_free_text() {
            let mut editor = scene_of("rectangle");
            text_tap(&mut editor, (600.0, 600.0));
            assert!(!open(&editor).fixed_width);
        }

        #[test]
        fn the_middle_snap_is_30_view_points_whatever_the_zoom() {
            // Whether a text-Tool tap `dx` view points right of the middle, zoomed
            // in, makes a label.
            let makes_label = |dx: f64| {
                let mut editor = scene_of("rectangle");
                for _ in 0..10 {
                    editor.zoom_in();
                }
                let view = editor.viewport();
                let (ox, oy) = view.origin();
                let middle = ((MIDDLE.0 - ox) * view.zoom(), (MIDDLE.1 - oy) * view.zoom());
                text_tap(&mut editor, (middle.0 + dx, middle.1));
                open(&editor).fixed_width
            };
            assert!(makes_label(25.0));
            assert!(!makes_label(35.0));
        }

        #[test]
        fn text_tool_labels_undo_cancel_and_vanish_when_empty_as_double_tap_ones_do() {
            // Undo.
            let mut editor = scene_of("rectangle");
            text_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hi");
            editor.commit_text();
            editor.undo();
            assert_eq!(live_texts(&editor), 0);

            // Cancel.
            let mut editor = scene_of("rectangle");
            text_tap(&mut editor, MIDDLE);
            editor.set_editing_text("Hi");
            editor.cancel_text();
            assert_eq!(live_texts(&editor), 0);

            // Emptying a label deletes it.
            let mut editor = labelled("rectangle", 200.0, "Hi");
            text_tap(&mut editor, MIDDLE);
            editor.set_editing_text("  ");
            editor.commit_text();
            assert_eq!(live_texts(&editor), 0);
        }
    }

    mod text {
        //! Text Elements (ADR-0008), tested through `Editor::geometry`: the core
        //! describes text as positioned lines and never lays out glyphs. Core tests
        //! use a fake `TextMeasurer` with fixed character widths.

        use crate::test_support as common;

        use std::sync::Arc;

        use crate::{Editor, Geometry, TextAlign, TextLine, TextMeasurer};
        use common::{FixedWidth, editor_with_element, editor_with_fixture};

        fn measured(element: &str) -> Editor {
            let mut editor = editor_with_element(element);
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor
        }

        /// The lines of `geometry` as `(text, x, y)`.
        fn lines(geometry: &Geometry) -> Vec<(&str, f32, f32)> {
            geometry
                .text_lines()
                .iter()
                .map(|line| (geometry.line_text(line), line.x, line.y))
                .collect()
        }

        fn only_line(geometry: &Geometry) -> TextLine {
            assert_eq!(geometry.text_lines().len(), 1, "{:?}", lines(geometry));
            geometry.text_lines()[0]
        }

        const HELLO: &str = r##""id":"t","type":"text","x":100,"y":50,"width":50,"height":25,
    "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
    "textAlign":"left","verticalAlign":"top","strokeColor":"#e03131","opacity":50"##;

        // --- Unbound text -----------------------------------------------------------

        #[test]
        fn a_text_element_is_described_as_a_line_of_text_not_as_paths() {
            let geometry = editor_with_element(HELLO).geometry();

            assert!(geometry.parts().is_empty(), "no box around text");
            let line = only_line(&geometry);
            assert_eq!(geometry.line_text(&line), "Hello");
            assert_eq!(line.font_size, 20.0);
            assert_eq!(line.font_family, 5);
            assert_eq!(line.align, TextAlign::Left as u32);
            assert_eq!(line.color, 0xe03131ff);
            assert_eq!(line.opacity, 0.5);
        }

        #[test]
        fn the_line_sits_on_the_baseline_of_its_box() {
            // Excalifont: ascender 886 and descender -374 per 1000 units, so 17.72
            // above and 7.48 below the baseline at 20 pt. A line height of 25 leaves
            // (25 - 17.72 - 7.48) / 2 = -0.1 around them: the baseline is 17.62
            // below the top.
            let geometry = editor_with_element(HELLO).geometry();

            let line = only_line(&geometry);
            assert_eq!(line.x, 100.0);
            assert!((line.y - (50.0 + 17.62)).abs() < 1e-4, "{}", line.y);
        }

        #[test]
        fn every_line_of_a_text_is_positioned_one_line_height_below_the_last() {
            let geometry = editor_with_element(
                r#""id":"t","type":"text","x":0,"y":0,"width":60,"height":75,
        "text":"one\ntwo\r\nthree","fontSize":20,"lineHeight":1.25"#,
            )
            .geometry();

            let ys: Vec<f32> = geometry.text_lines().iter().map(|l| l.y).collect();
            let texts: Vec<&str> = lines(&geometry).iter().map(|l| l.0).collect();
            assert_eq!(texts, ["one", "two", "three"]);
            assert!((ys[1] - ys[0] - 25.0).abs() < 1e-4);
            assert!((ys[2] - ys[1] - 25.0).abs() < 1e-4);
        }

        #[test]
        fn alignment_anchors_the_lines_at_the_left_middle_or_right_of_the_box() {
            let anchor = |align: &str| {
                let geometry = editor_with_element(&format!(
                    r#""id":"t","type":"text","x":100,"y":0,"width":80,"height":25,
            "text":"Hi","fontSize":20,"textAlign":"{align}""#
                ))
                .geometry();
                let line = only_line(&geometry);
                (line.x, line.align)
            };

            assert_eq!(anchor("left"), (100.0, TextAlign::Left as u32));
            assert_eq!(anchor("center"), (140.0, TextAlign::Center as u32));
            assert_eq!(anchor("right"), (180.0, TextAlign::Right as u32));
        }

        #[test]
        fn text_is_drawn_in_z_order_between_the_paths_around_it() {
            let mut editor = Editor::new();
            editor
                .load(
                    br#"{"type":"excalidraw","elements":[
            {"id":"a","type":"rectangle","width":10,"height":10},
            {"id":"t","type":"text","text":"x","width":10,"height":25},
            {"id":"b","type":"rectangle","width":10,"height":10}]}"#,
                )
                .unwrap();
            let geometry = editor.geometry();

            let line = only_line(&geometry);
            let before = geometry.parts()[..line.part_index as usize].to_vec();
            assert!(!before.is_empty() && before.iter().all(|p| p.element == 0));
            assert!(
                geometry.parts()[line.part_index as usize..]
                    .iter()
                    .all(|p| p.element == 2)
            );
            assert_eq!(line.element, 1);
        }

        #[test]
        fn a_rotated_text_turns_about_the_middle_of_its_box() {
            // The box is 40 wide and 25 high around (120, 12.5); turned a quarter
            // clockwise, its left edge is at the top.
            let geometry = editor_with_element(
                r#""id":"t","type":"text","x":100,"y":0,"width":40,"height":25,
        "text":"Hi","fontSize":20,"lineHeight":1.25,"angle":1.5707963267948966"#,
            )
            .geometry();

            let line = only_line(&geometry);
            assert!((line.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
            // The unrotated anchor (100, 17.62) is 20 left of and 5.12 below the
            // middle; a quarter clockwise puts it 5.12 left of and 20 above it.
            assert!((line.x - (120.0 - 5.12)).abs() < 1e-3, "{}", line.x);
            assert!((line.y - (12.5 - 20.0)).abs() < 1e-3, "{}", line.y);
        }

        #[test]
        fn deleted_and_empty_texts_draw_nothing() {
            for element in [
                r#""id":"t","type":"text","text":"gone","isDeleted":true"#,
                r#""id":"t","type":"text","text":"""#,
            ] {
                assert!(
                    editor_with_element(element)
                        .geometry()
                        .text_lines()
                        .is_empty()
                );
            }
        }

        #[test]
        fn other_font_families_are_drawn_as_excalifont_but_keep_their_family() {
            let editor = editor_with_element(
                r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":23,
        "text":"Hi","fontSize":20,"fontFamily":2,"lineHeight":1.15"#,
            );
            let line = only_line(&editor.geometry());
            assert_eq!(line.font_family, 2);
            assert!(
                String::from_utf8(editor.save())
                    .unwrap()
                    .contains(r#""fontFamily": 2"#)
            );
        }

        // --- Wrapping and labels ----------------------------------------------------

        /// A rectangle `id` of the given width and height with the label `text`
        /// (typed as `original`, saved as `text`).
        fn labelled(container: &str, text: &str, original: &str, label: &str) -> Editor {
            let mut editor = Editor::new();
            let drawing = format!(
                r#"{{"type":"excalidraw","elements":[
        {{"id":"box","type":"rectangle",{container},"boundElements":[{{"type":"text","id":"t"}}]}},
        {{"id":"t","type":"text","containerId":"box","fontSize":20,"lineHeight":1.25,
          "text":"{text}","originalText":"{original}",{label}}}]}}"#
            );
            editor.load(drawing.as_bytes()).unwrap();
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor
        }

        fn texts(geometry: &Geometry) -> Vec<&str> {
            lines(geometry).iter().map(|l| l.0).collect()
        }

        #[test]
        fn a_label_wraps_to_the_width_of_its_container_less_padding() {
            // 100 wide less 5 padding on each side leaves 90: nine characters of 10.
            let editor = labelled(
                r#""x":0,"y":0,"width":100,"height":200"#,
                "aaaa bbbb cccc",
                "aaaa bbbb cccc",
                r#""x":5,"y":5,"width":90,"height":25,"textAlign":"left","verticalAlign":"top""#,
            );

            assert_eq!(texts(&editor.geometry()), ["aaaa bbbb", "cccc"]);
        }

        #[test]
        fn a_label_wraps_again_when_the_container_is_not_the_size_it_was_wrapped_for() {
            // Saved for a wide container, now 60 wide: six characters fit.
            let editor = labelled(
                r#""x":0,"y":0,"width":70,"height":200"#,
                "aaaa bbbb cccc",
                "aaaa bbbb cccc",
                r#""x":5,"y":5,"width":130,"height":25,"textAlign":"left","verticalAlign":"top""#,
            );

            assert_eq!(texts(&editor.geometry()), ["aaaa", "bbbb", "cccc"]);
        }

        #[test]
        fn without_a_measurer_the_lines_are_the_ones_saved_in_the_file() {
            let mut editor = labelled(
                r#""x":0,"y":0,"width":100,"height":200"#,
                "aaaa bbbb\\ncccc",
                "aaaa bbbb cccc",
                r#""x":5,"y":5,"width":90,"height":50,"textAlign":"left","verticalAlign":"top""#,
            );
            let mut unmeasured = Editor::new();
            unmeasured.load(&editor.save()).unwrap();
            assert_eq!(texts(&unmeasured.geometry()), ["aaaa bbbb", "cccc"]);

            // The measurer is a choice of the client; replacing it changes the lines.
            struct Narrow;
            impl TextMeasurer for Narrow {
                fn line_width(&self, text: &str, _: f64) -> f64 {
                    text.chars().count() as f64 * 20.0
                }
            }
            editor.set_text_measurer(Arc::new(Narrow));
            assert_eq!(texts(&editor.geometry()), ["aaaa", "bbbb", "cccc"]);
        }

        #[test]
        fn a_label_is_placed_in_its_container_by_its_alignment() {
            // Container 200 x 100 at (10, 20); the label is one line of "ab", 20 wide
            // and 25 high, in the padded area (15, 25) to (205, 115).
            let place = |align: &str, vertical: &str| {
                let editor = labelled(
                    r#""x":10,"y":20,"width":200,"height":100"#,
                    "ab",
                    "ab",
                    &format!(
                        r#""x":0,"y":0,"width":20,"height":25,"textAlign":"{align}","verticalAlign":"{vertical}""#
                    ),
                );
                let geometry = editor.geometry();
                let line = only_line(&geometry);
                // The baseline is 17.62 below the top of the line.
                (line.x, line.y - 17.62)
            };
            let near = |(x, y): (f32, f32), (ex, ey): (f32, f32)| {
                assert!(
                    (x - ex).abs() < 1e-3 && (y - ey).abs() < 1e-3,
                    "{x}, {y} != {ex}, {ey}"
                );
            };

            near(place("left", "top"), (15.0, 25.0));
            // The anchor of a line is its start, middle or end, by alignment.
            near(place("center", "middle"), (15.0 + 95.0, 25.0 + 45.0 - 12.5));
            near(place("right", "bottom"), (15.0 + 190.0, 25.0 + 90.0 - 25.0));
        }

        #[test]
        fn a_label_in_an_ellipse_or_diamond_uses_the_rectangle_inscribed_in_it() {
            let width = |kind: &str| {
                let mut editor = Editor::new();
                let drawing = format!(
                    r#"{{"type":"excalidraw","elements":[
            {{"id":"box","type":"{kind}","x":0,"y":0,"width":200,"height":200}},
            {{"id":"t","type":"text","containerId":"box","fontSize":20,"text":"x",
              "originalText":"aaa bbb ccc ddd eee","x":0,"y":0,"width":10,"height":25}}]}}"#
                );
                editor.load(drawing.as_bytes()).unwrap();
                editor.set_text_measurer(Arc::new(FixedWidth));
                // Characters of the longest line.
                texts(&editor.geometry())
                    .iter()
                    .map(|l| l.len())
                    .max()
                    .unwrap()
            };

            // Ellipse: round(200 / 2 * sqrt 2) - 10 = 131 -> 13 characters, so
            // "aaa bbb ccc" fits and "aaa bbb ccc ddd" does not.
            assert_eq!(width("ellipse"), 11);
            // Diamond: round(200 / 2) - 10 = 90 -> 9 characters: "aaa bbb".
            assert_eq!(width("diamond"), 7);
        }

        #[test]
        fn text_bound_to_an_arrow_stays_where_the_file_has_it() {
            let mut editor = Editor::new();
            editor
                .load(
                    br#"{"type":"excalidraw","elements":[
            {"id":"a","type":"arrow","x":0,"y":0,"width":200,"height":0,"points":[[0,0],[200,0]],
             "boundElements":[{"type":"text","id":"t"}]},
            {"id":"t","type":"text","containerId":"a","fontSize":20,"text":"on the arrow",
             "originalText":"on the arrow","x":40,"y":-12,"width":120,"height":25}]}"#,
                )
                .unwrap();
            editor.set_text_measurer(Arc::new(FixedWidth));
            let geometry = editor.geometry();

            assert_eq!(texts(&geometry), ["on the arrow"]);
            assert_eq!(geometry.text_lines()[0].x, 40.0);
        }

        #[test]
        fn a_text_with_a_width_of_its_own_wraps_to_it() {
            let editor = measured(
                r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"autoResize":false,
        "text":"x","originalText":"one two three","fontSize":20,"lineHeight":1.25"#,
            );

            assert_eq!(texts(&editor.geometry()), ["one", "two", "three"]);
        }

        #[test]
        fn words_wrap_after_hyphens_and_break_when_longer_than_the_line() {
            let wrapped = |text: &str, width: u32| {
                let editor = measured(&format!(
                    r#""id":"t","type":"text","x":0,"y":0,"width":{width},"height":25,"autoResize":false,
            "text":"x","originalText":"{text}","fontSize":20,"lineHeight":1.25"#
                ));
                texts(&editor.geometry()).join("|")
            };

            assert_eq!(wrapped("well-known state-of", 100), "well-known|state-of");
            assert_eq!(wrapped("well-known", 60), "well-|known");
            assert_eq!(wrapped("abcdefghijkl", 50), "abcde|fghij|kl");
            // Spaces at a break are dropped; at the very end of the text those that
            // still fit stay.
            assert_eq!(wrapped("ab  cd", 40), "ab|cd");
            assert_eq!(wrapped("ab  ", 30), "ab ");
            // Hard breaks stay.
            assert_eq!(wrapped("ab\\n\\ncd", 100), "ab||cd");
        }

        #[test]
        fn cjk_text_wraps_between_any_two_characters() {
            let editor = measured(
                r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"autoResize":false,
        "text":"x","originalText":"日本語のテキスト","fontSize":20,"lineHeight":1.25"#,
            );

            assert_eq!(texts(&editor.geometry()), ["日本語のテ", "キスト"]);
        }

        #[test]
        fn text_wraps_exactly_like_excalidraw_wraps_it() {
            // `text.excalidraw` was made by Excalidraw's own code with the same fixed
            // character widths: the file's lines and positions are what it laid out.
            let unmeasured = editor_with_fixture("text").geometry();
            let mut measuring = editor_with_fixture("text");
            measuring.set_text_measurer(Arc::new(FixedWidth));
            let measured = measuring.geometry();

            let (expected, actual) = (lines(&unmeasured), lines(&measured));
            assert!(expected.len() > 60, "{} lines", expected.len());
            assert_eq!(actual.len(), expected.len());
            for (e, a) in expected.iter().zip(&actual) {
                assert_eq!(a.0, e.0);
                assert!(
                    (a.1 - e.1).abs() < 1e-3 && (a.2 - e.2).abs() < 1e-3,
                    "{a:?} != {e:?}"
                );
            }
            for (e, a) in unmeasured.text_lines().iter().zip(measured.text_lines()) {
                assert_eq!((e.element, e.align, e.angle), (a.element, a.align, a.angle));
            }
        }
    }
}
