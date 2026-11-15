//! Creating and editing text (ADR-0006), as Excalidraw does with its
//! `textarea` (`textWysiwyg`): the client shows a native text view over the
//! Canvas where the core says, with the text and style it says, and tells the
//! core what is typed. The core keeps the text Element in step, so that the
//! text view and the Element always agree on where the text is and how big.
//!
//! Like on excalidraw.com the Element is in the Scene while it is edited, but
//! not drawn: the text view shows it. Committing makes the edit one undo step
//! (creating the text, or changing it); an empty text is not kept.
//!
//! A text can also be the *label* of a rectangle, diamond or ellipse (its
//! container): it is created and edited with the shape, wraps to the width
//! the shape leaves it, is centred in it and lists in the shape's
//! `boundElements`. The shape grows with the text, as on excalidraw.com, and
//! the arrows bound to it follow. An arrow takes a label too, at the middle
//! of its path; that wraps to a fraction of the arrow's width and does not
//! make the arrow grow.

use crate::binding::Attachments;
use crate::color::parse_color;
use crate::draw_shape::new_element;
use crate::geometry::TextAlign;
use crate::history::Step;
use crate::linear;
use crate::path::Point;
use crate::properties::{self, Property, align_name};
use crate::scene::{Element, ElementType, Scene};
use crate::style::Style;
use crate::text::{self, TextMeasurer};
use crate::viewport::Viewport;
use crate::wrap;
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;

/// Excalifont, the font family of every new text.
const FONT_FAMILY: f64 = 5.0;
/// The line height of Excalifont, relative to the font size.
const LINE_HEIGHT: f64 = 1.25;

/// The text view a client shows over the Canvas while a text is edited: where
/// it is, what it says and how it looks. In view coordinates, so it follows
/// the Canvas through panning and zooming: clients read it with the viewport
/// after every input and command.
///
/// The view is the box of the text, turned by `angle` about its middle. Text
/// that grows with its content is not wrapped; the box is as wide as the
/// widest line. Text of `fixed_width` wraps to the width of the box.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditor {
    /// The text as typed: what the text view starts with.
    pub text: String,
    /// The top left corner of the box, in points from the Canvas's top left.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The font size in view points, which is the Scene's times the zoom.
    pub font_size: f64,
    /// The height of a line relative to the font size.
    pub line_height: f64,
    /// The Element's `fontFamily`. Every family is drawn in Excalifont.
    pub font_family: u32,
    pub align: TextAlign,
    /// `0xRRGGBBAA`.
    pub color: u32,
    /// `0.0..=1.0`.
    pub opacity: f64,
    /// Clockwise, in radians.
    pub angle: f64,
    pub zoom: f64,
    /// Whether the text wraps to the width of the box instead of growing.
    pub fixed_width: bool,
}

/// A text being edited: an Element in the Scene, as a step that undoes it.
#[derive(Debug)]
pub(crate) struct TextEditing {
    handle: u32,
    id: String,
    /// Whether the text was there before the editing began.
    existing: bool,
    /// The shape the text is the label of.
    container: Option<Container>,
    step: Step,
}

/// The shape a label is in, as far as editing the label changes it.
#[derive(Debug)]
struct Container {
    handle: u32,
    id: String,
    /// How high it was when the editing began (or the font last changed): it
    /// shrinks back to it as text is taken away.
    original_height: f64,
    /// The arrows bound to it, which follow it as it grows.
    attachments: Attachments,
}

impl TextEditing {
    /// Starts a new text at `at` in the Scene, centred on it as Excalidraw
    /// does, as the action of `step`. It is empty and has the style of new
    /// Elements, and is anchored at `at` as its alignment says.
    pub fn begin_new(
        mut step: Step,
        scene: &mut Scene,
        entropy: &mut Entropy,
        style: &Style,
        at: Point,
        measurer: &dyn TextMeasurer,
    ) -> Self {
        let size = blank_size(style, measurer);
        let origin = (
            at.0 - text::anchor_ratio(style.text_align) * size.0,
            at.1 - size.1 / 2.0,
        );
        let element = blank_text(entropy, style, origin, size);
        let id = id_of(&element);
        step.create(&id);
        let handle = scene.push(element);
        Self {
            handle,
            id,
            existing: false,
            container: None,
            step,
        }
    }

    /// Starts a new text as the label of the shape `container_id`, centred
    /// in it, as the action of `step`. It is empty and has the style of new
    /// Elements, in the group of the shape, right above it. `None` if there
    /// is no such shape.
    pub fn begin_label(
        mut step: Step,
        scene: &mut Scene,
        entropy: &mut Entropy,
        style: &Style,
        container_id: &str,
        measurer: &dyn TextMeasurer,
    ) -> Option<Self> {
        let container = scene.element_by_id(container_id)?;
        let (origin, angle) = (
            (container.x(), container.y()),
            text::label_angle(&container),
        );
        let groups = container
            .member("groupIds")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let container = Container::begin(&mut step, scene, &container);

        let mut element = blank_text(entropy, style, origin, blank_size(style, measurer));
        let text = |s: &str| Value::String(s.to_owned());
        for (key, value) in [
            ("textAlign", text("center")),
            ("verticalAlign", text("middle")),
            ("containerId", text(container_id)),
            ("angle", Value::Number(angle)),
            ("groupIds", groups),
        ] {
            element.insert(key.into(), value);
        }
        let id = id_of(&element);
        step.create(&id);
        let handle = scene.insert_after(container_id, element);
        // Listed right away, as Excalidraw does, so that a Drawing saved while
        // the label is edited is whole; cancelling takes it off again.
        list_label(scene, entropy, container_id, &id, true);
        let mut editing = Self {
            handle,
            id,
            existing: false,
            container: Some(container),
            step,
        };
        editing.lay_out_label(scene, entropy, measurer);
        Some(editing)
    }

    /// Starts editing the text `element` that is there, as the action of
    /// `step`. If it is the label of `container`, that is edited with it.
    pub fn begin_existing(
        mut step: Step,
        scene: &Scene,
        element: &Element,
        container: Option<&Element>,
    ) -> Self {
        step.touch(scene, element.id());
        let container = container.map(|container| Container::begin(&mut step, scene, container));
        Self {
            handle: element.handle(),
            id: element.id().to_owned(),
            existing: true,
            container,
            step,
        }
    }

    /// The handles of everything the editing changes: the text and the
    /// shape it labels.
    pub fn handles(&self) -> Vec<u32> {
        std::iter::once(self.handle)
            .chain(self.container.as_ref().map(|c| c.handle))
            .collect()
    }

    /// The handle of the Element being edited.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// The id of the Element being edited.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The id of the shape the text is the label of, if it is one.
    pub fn container_id(&self) -> Option<&str> {
        self.container.as_ref().map(|c| c.id.as_str())
    }

    /// Makes `typed` the text, and lays it out again. Returns whether
    /// anything changed.
    pub fn set_text(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        typed: &str,
        measurer: &dyn TextMeasurer,
    ) -> bool {
        let Some(element) = scene.element(self.handle) else {
            return false;
        };
        if element.original_text() == typed {
            return false;
        }
        if self.container.is_some() {
            let members = [("originalText", Value::String(typed.to_owned()))];
            scene.update(self.handle, entropy.integer(), members);
            self.lay_out_label(scene, entropy, measurer);
            return true;
        }
        let fit = text::fit_typed(&element, typed, measurer);
        let members = [
            ("originalText", Value::String(typed.to_owned())),
            ("text", Value::String(fit.text)),
            ("x", Value::Number(fit.x)),
            ("y", Value::Number(fit.y)),
            ("width", Value::Number(fit.width)),
            ("height", Value::Number(fit.height)),
        ];
        scene.update(self.handle, entropy.integer(), members);
        true
    }

    /// Sets `property` on the text, and lays it out again if it has to be.
    /// Returns whether anything changed.
    pub fn apply(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        property: &Property,
        measurer: &dyn TextMeasurer,
    ) -> bool {
        let step = &mut self.step;
        let changed = properties::apply(
            scene,
            entropy,
            std::slice::from_ref(&self.id),
            property,
            |scene, id| step.touch(scene, id),
        );
        if !changed {
            return false;
        }
        if let Some(container) = self.container.as_mut() {
            // A new font is a new text to hold: the shape is as high as it
            // is now to begin with.
            if let Some(shape) = scene.element(container.handle) {
                container.original_height = shape.height();
            }
            self.lay_out_label(scene, entropy, measurer);
        } else if let Some(element) = scene.element(self.handle)
            && let Some(members) = text::refit(&element, None, measurer)
        {
            scene.update(self.handle, entropy.integer(), members);
        }
        true
    }

    /// Makes the container the size its label needs, and puts the label in
    /// it, wrapped to it, with the arrows bound to it following.
    fn lay_out_label(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        measurer: &dyn TextMeasurer,
    ) {
        let Some(container) = self.container.as_ref() else {
            return;
        };
        // An arrow does not grow with its label.
        let size = if scene
            .element(container.handle)
            .is_some_and(|c| c.kind() == ElementType::Arrow)
        {
            None
        } else {
            let (Some(label), Some(shape)) =
                (scene.element(self.handle), scene.element(container.handle))
            else {
                return;
            };
            Some(text::container_size_for(
                &label,
                &shape,
                container.original_height,
                measurer,
            ))
        };
        if let Some(size) = size
            && let Some(shape) = scene.element(container.handle)
            && (shape.width(), shape.height()) != size
        {
            let members = [
                ("width", Value::Number(size.0)),
                ("height", Value::Number(size.1)),
            ];
            scene.update(container.handle, entropy.integer(), members);
        }
        let fit = {
            let (Some(label), Some(shape)) =
                (scene.element(self.handle), scene.element(container.handle))
            else {
                return;
            };
            text::fit_label(&label, &shape, Some(measurer))
        };
        let mut members = vec![
            ("x", Value::Number(fit.x)),
            ("y", Value::Number(fit.y)),
            ("width", Value::Number(fit.width)),
            ("height", Value::Number(fit.height)),
        ];
        if let Some(lines) = fit.text {
            members.push(("text", Value::String(lines)));
        }
        scene.update(self.handle, entropy.integer(), members);
        if let Some(container) = self.container.as_ref() {
            container.attachments.update(scene, entropy, &mut self.step);
        }
    }

    /// What the client shows for the text, seen through `viewport`.
    pub fn editor(&self, scene: &Scene, viewport: Viewport) -> Option<TextEditor> {
        let element = scene.element(self.handle)?;
        let zoom = viewport.zoom();
        let (x, y, width, height) = match self
            .container
            .as_ref()
            .and_then(|c| scene.element(c.handle))
        {
            Some(shape) => label_box(&element, &shape),
            None => (element.x(), element.y(), element.width(), element.height()),
        };
        let (x, y) = viewport.view_at((x, y));
        Some(TextEditor {
            text: element.original_text().to_owned(),
            x,
            y,
            width: width * zoom,
            height: height * zoom,
            font_size: element.font_size() * zoom,
            line_height: element.line_height(),
            font_family: element.font_family(),
            align: element.text_align(),
            color: parse_color(element.stroke_color()),
            opacity: element.opacity(),
            angle: element.angle(),
            zoom,
            fixed_width: self.container.is_some() || !element.auto_resize(),
        })
    }

    /// Ends the editing with the text as it is. If there is something in it
    /// the Element stays, and the step is what undoes the editing; an empty
    /// text is not kept, which for one that was there before deletes it (and
    /// takes it off its shape).
    pub fn finish(self, scene: &mut Scene, entropy: &mut Entropy) -> Option<Step> {
        let blank = scene
            .element(self.handle)
            .is_none_or(|element| element.original_text().trim().is_empty());
        match (blank, self.existing) {
            (false, _) => Some(self.step),
            (true, true) => {
                scene.mark_deleted(&self.id, entropy.integer());
                if let Some(container) = &self.container {
                    list_label(scene, entropy, &container.id, &self.id, false);
                }
                Some(self.step)
            }
            (true, false) => {
                self.cancel(scene, entropy);
                None
            }
        }
    }

    /// Ends the editing without any change to the text.
    pub fn cancel(self, scene: &mut Scene, entropy: &mut Entropy) {
        let handle = self.handle;
        let existing = self.existing;
        // The shape may have grown for the text.
        self.step.revert(scene, entropy);
        if !existing {
            scene.remove(handle);
        }
    }
}

impl Container {
    /// Notes the shape `element` as touched by `step`, which edits a label in
    /// it.
    fn begin(step: &mut Step, scene: &Scene, element: &Element) -> Self {
        step.touch(scene, element.id());
        Self {
            handle: element.handle(),
            id: element.id().to_owned(),
            original_height: element.height(),
            attachments: Attachments::begin(step, scene, vec![element.id().to_owned()]),
        }
    }
}

/// The size of a text with nothing in it: as wide as a space and a line high.
fn blank_size(style: &Style, measurer: &dyn TextMeasurer) -> (f64, f64) {
    let font_size = style.font_size.size();
    (
        wrap::widest(&[String::new()], font_size, measurer),
        font_size * LINE_HEIGHT,
    )
}

/// A new text Element of `size` at `origin`, empty and in the style of new
/// Elements, aligned and bound to nothing yet.
fn blank_text(
    entropy: &mut Entropy,
    style: &Style,
    origin: Point,
    (width, height): (f64, f64),
) -> Object {
    let mut element = new_element("text", None, entropy, style, origin);
    let text = |s: &str| Value::String(s.to_owned());
    for (key, value) in [
        ("width", Value::Number(width)),
        ("height", Value::Number(height)),
        ("text", text("")),
        ("fontSize", Value::Number(style.font_size.size())),
        ("baseFontSize", Value::Null),
        ("fontFamily", Value::Number(FONT_FAMILY)),
        ("textAlign", text(align_name(style.text_align))),
        ("verticalAlign", text("top")),
        ("containerId", Value::Null),
        ("originalText", text("")),
        ("autoResize", Value::Bool(true)),
        ("lineHeight", Value::Number(LINE_HEIGHT)),
        ("labelPosition", Value::Null),
    ] {
        element.insert(key.into(), value);
    }
    element
}

fn id_of(element: &Object) -> String {
    element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Makes the `boundElements` of the shape `container` list the text `label`
/// or not.
fn list_label(
    scene: &mut Scene,
    entropy: &mut Entropy,
    container: &str,
    label: &str,
    listed: bool,
) {
    let Some(shape) = scene.element_by_id(container) else {
        return;
    };
    let mut bound: Vec<Value> = shape
        .member("boundElements")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .to_vec();
    let is_label = |entry: &Value| entry.get("id").and_then(Value::as_str) == Some(label);
    if bound.iter().any(is_label) == listed {
        return;
    }
    if listed {
        let mut entry = Object::new();
        entry.insert("type".into(), Value::String("text".into()));
        entry.insert("id".into(), Value::String(label.to_owned()));
        bound.push(Value::Object(entry));
    } else {
        bound.retain(|entry| !is_label(entry));
    }
    let handle = shape.handle();
    scene.update(
        handle,
        entropy.integer(),
        [("boundElements", Value::Array(bound))],
    );
}

/// The box a text view of the label `label` of `container` fills, as `(x, y,
/// width, height)` in the Scene: as wide as lines may be in the container and
/// as high as the text, with the lines aligned in it as the label is. The
/// label of a shape sits in it by its alignment, so its middle is off the
/// box's by whatever the lines leave over, along the (turned) top of the
/// container; that of an arrow is centred on the box.
fn label_box(label: &Element, container: &Element) -> (f64, f64, f64, f64) {
    let width = text::bound_max_width(container, label);
    let height = label.height();
    let off = match label.text_align() {
        _ if container.kind() == ElementType::Arrow => 0.0,
        TextAlign::Left => (width - label.width()) / 2.0,
        TextAlign::Center => 0.0,
        TextAlign::Right => (label.width() - width) / 2.0,
    };
    let (sin, cos) = text::label_angle(container).sin_cos();
    // What a file saves of an arrow's label may be where the arrow was.
    let middle = if container.kind() == ElementType::Arrow {
        linear::label_anchor(container)
    } else {
        (
            label.x() + label.width() / 2.0 + off * cos,
            label.y() + label.height() / 2.0 + off * sin,
        )
    };
    (
        middle.0 - width / 2.0,
        middle.1 - height / 2.0,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    //! Creating and editing text (ADR-0006), tested at the Editor seam: taps and
    //! commands in, the editor the client shows over the Canvas, the saved
    //! Drawing and the geometry out. The only test double is a `TextMeasurer`
    //! with fixed character widths (half the font size).
    //!
    //! Expected values are worked out by hand: at font size 20 a character is 10
    //! wide and a line 25 high (line height 1.25), at 28 they are 14 and 35.

    use std::sync::Arc;

    use crate::scene::LineStyle;
    use crate::{Editor, FontSize, Property, TextAlign, TextEditor, Tool};
    use pkm_base::json::Value;

    use crate::test_support as common;
    use common::{FixedWidth, editor_with, replay, saved_elements, tap};

    fn measured(mut editor: Editor) -> Editor {
        editor.set_text_measurer(Arc::new(FixedWidth));
        editor
    }

    fn empty() -> Editor {
        measured(Editor::new())
    }

    /// An editor with the text Tool active.
    fn with_text_tool() -> Editor {
        let mut editor = empty();
        editor.set_tool(Tool::Text);
        editor
    }

    /// The editor after a tap at `(x, y)` with the text Tool, and `text` typed.
    fn typing(text: &str, (x, y): (f64, f64)) -> Editor {
        let mut editor = with_text_tool();
        tap(&mut editor, "finger", (x, y), 1.0);
        editor.set_editing_text(text);
        editor
    }

    fn open(editor: &Editor) -> TextEditor {
        editor.text_editor().expect("a text editor is open")
    }

    fn texts(editor: &Editor) -> Vec<Value> {
        saved_elements(editor)
            .into_iter()
            .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
            .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
            .collect()
    }

    fn only_text(editor: &Editor) -> Value {
        let texts = texts(editor);
        assert_eq!(texts.len(), 1, "{texts:?}");
        texts[0].clone()
    }

    fn number(element: &Value, key: &str) -> f64 {
        element.get(key).and_then(Value::as_f64).unwrap()
    }

    fn string<'a>(element: &'a Value, key: &str) -> &'a str {
        element.get(key).and_then(Value::as_str).unwrap()
    }

    /// The lines of the Scene's geometry as drawn.
    fn drawn(editor: &Editor) -> Vec<String> {
        let geometry = editor.geometry();
        geometry
            .text_lines()
            .iter()
            .map(|line| geometry.line_text(line).to_owned())
            .collect()
    }

    // --- Creating --------------------------------------------------------------------

    #[test]
    fn tapping_with_the_text_tool_opens_the_editor_at_the_tap() {
        let mut editor = with_text_tool();

        tap(&mut editor, "finger", (100.0, 200.0), 1.0);

        let shown = open(&editor);
        assert_eq!(shown.text, "");
        // The first line is centred on the tap: 12.5 above it.
        assert_eq!((shown.x, shown.y), (100.0, 187.5));
        assert_eq!(shown.font_size, 20.0);
        assert_eq!(shown.align, TextAlign::Left);
    }

    #[test]
    fn the_tool_returns_to_selection_once_the_editor_is_open() {
        let mut editor = with_text_tool();
        tap(&mut editor, "finger", (100.0, 200.0), 1.0);
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn a_tool_lock_keeps_the_text_tool() {
        let mut editor = with_text_tool();
        editor.set_tool_lock(true);
        tap(&mut editor, "finger", (100.0, 200.0), 1.0);
        assert_eq!(editor.tool(), Tool::Text);
        assert!(editor.text_editor().is_some());
    }

    #[test]
    fn committing_creates_the_text_element() {
        let mut editor = typing("Hello", (100.0, 200.0));

        editor.commit_text();

        assert!(editor.text_editor().is_none());
        let text = only_text(&editor);
        assert_eq!(string(&text, "text"), "Hello");
        assert_eq!(string(&text, "originalText"), "Hello");
        assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 187.5));
        assert_eq!(
            (number(&text, "width"), number(&text, "height")),
            (50.0, 25.0)
        );
        assert_eq!(number(&text, "fontSize"), 20.0);
        assert_eq!(number(&text, "fontFamily"), 5.0);
        assert_eq!(number(&text, "lineHeight"), 1.25);
        assert_eq!(string(&text, "textAlign"), "left");
        assert_eq!(string(&text, "verticalAlign"), "top");
        assert_eq!(string(&text, "strokeColor"), "#1e1e1e");
        assert_eq!(text.get("containerId"), Some(&Value::Null));
        assert_eq!(text.get("autoResize"), Some(&Value::Bool(true)));
    }

    #[test]
    fn the_new_text_is_selected() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.commit_text();
        assert_eq!(editor.selection().len(), 1);
    }

    #[test]
    fn empty_text_cancels() {
        let mut editor = typing("", (100.0, 200.0));
        editor.commit_text();
        assert!(editor.text_editor().is_none());
        assert!(texts(&editor).is_empty());
        assert!(!editor.can_undo());
    }

    #[test]
    fn text_of_only_spaces_and_line_breaks_cancels() {
        let mut editor = typing("  \n ", (100.0, 200.0));
        editor.commit_text();
        assert!(texts(&editor).is_empty());
    }

    #[test]
    fn cancelling_discards_what_was_typed() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.cancel_text();
        assert!(editor.text_editor().is_none());
        assert!(texts(&editor).is_empty());
    }

    #[test]
    fn the_editor_follows_the_text_as_it_is_typed() {
        let mut editor = typing("Hello\nWorld!", (100.0, 200.0));

        let shown = open(&editor);
        assert_eq!(shown.text, "Hello\nWorld!");
        assert_eq!((shown.width, shown.height), (60.0, 50.0));
        assert_eq!((shown.x, shown.y), (100.0, 187.5));

        editor.set_editing_text("Hi");
        let shown = open(&editor);
        assert_eq!((shown.width, shown.height), (20.0, 25.0));
    }

    #[test]
    fn the_text_is_not_drawn_while_the_editor_shows_it() {
        let mut editor = typing("Hello", (100.0, 200.0));
        assert_eq!(drawn(&editor), Vec::<String>::new());

        editor.commit_text();
        assert_eq!(drawn(&editor), ["Hello"]);
    }

    #[test]
    fn creating_text_is_one_undo_step() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.set_editing_text("Hello world");
        editor.commit_text();
        assert!(editor.can_undo());

        editor.undo();
        assert!(texts(&editor).is_empty());
        assert!(editor.selection().is_empty());
        assert!(!editor.can_undo());

        editor.redo();
        assert_eq!(string(&only_text(&editor), "text"), "Hello world");
    }

    #[test]
    fn a_tap_elsewhere_commits_the_text() {
        let mut editor = typing("Hello", (100.0, 200.0));

        tap(&mut editor, "finger", (400.0, 400.0), 2.0);

        assert!(editor.text_editor().is_none());
        assert_eq!(string(&only_text(&editor), "text"), "Hello");
    }

    #[test]
    fn two_fingers_panning_do_not_commit_the_text() {
        let mut editor = typing("Hello", (100.0, 200.0));

        replay(
            &mut editor,
            "2.0 down 1 finger 300 300
         2.0 down 2 finger 340 300
         2.1 move 1 finger 300 340
         2.1 move 2 finger 340 340
         2.2 up 1 finger 300 340
         2.2 up 2 finger 340 340",
        );

        assert!(editor.text_editor().is_some());
    }

    #[test]
    fn changing_the_tool_commits_the_text() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.set_tool(Tool::Rectangle);
        assert!(editor.text_editor().is_none());
        assert_eq!(string(&only_text(&editor), "text"), "Hello");
    }

    #[test]
    fn commands_commit_the_text_first() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.undo();
        assert!(editor.text_editor().is_none());
        // The commit was the latest change: undoing took it back.
        assert!(texts(&editor).is_empty());
    }

    #[test]
    fn a_tap_with_the_text_tool_on_text_edits_it() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor.commit_text();
        editor.set_tool(Tool::Text);

        tap(&mut editor, "finger", (120.0, 200.0), 5.0);

        assert_eq!(open(&editor).text, "Hello");
        assert_eq!(texts(&editor).len(), 1);
    }

    #[test]
    fn no_editor_opens_without_a_measurer() {
        let mut editor = Editor::new();
        editor.set_tool(Tool::Text);
        tap(&mut editor, "finger", (100.0, 200.0), 1.0);
        assert!(editor.text_editor().is_none());
    }

    // --- Editing ---------------------------------------------------------------------

    const HELLO: &str = r##"{"id":"t","type":"text","x":100,"y":50,"width":50,"height":25,
    "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
    "textAlign":"left","verticalAlign":"top","strokeColor":"#e03131","opacity":100,
    "containerId":null,"autoResize":true,"version":3,"versionNonce":7}"##;

    fn with_hello() -> Editor {
        measured(editor_with(&[HELLO]))
    }

    /// Double-taps the text at `(x, y)`.
    fn double_tap(editor: &mut Editor, (x, y): (f64, f64)) {
        tap(editor, "finger", (x, y), 10.0);
        tap(editor, "finger", (x, y), 10.2);
    }

    #[test]
    fn a_double_tap_on_text_opens_the_editor_on_it() {
        let mut editor = with_hello();

        double_tap(&mut editor, (120.0, 60.0));

        let shown = open(&editor);
        assert_eq!(shown.text, "Hello");
        assert_eq!((shown.x, shown.y), (100.0, 50.0));
        assert_eq!((shown.width, shown.height), (50.0, 25.0));
    }

    #[test]
    fn a_double_tap_on_empty_canvas_opens_the_editor_at_the_tap() {
        let mut editor = empty();

        double_tap(&mut editor, (100.0, 200.0));

        let shown = open(&editor);
        assert_eq!(shown.text, "");
        assert_eq!((shown.x, shown.y), (100.0, 187.5));
        assert_eq!(editor.tool(), Tool::Selection);

        editor.set_editing_text("Hello");
        editor.commit_text();
        assert_eq!(string(&only_text(&editor), "text"), "Hello");
        assert_eq!(editor.selection().len(), 1);
    }

    #[test]
    fn a_double_click_with_a_mouse_on_empty_canvas_makes_a_text() {
        let mut editor = empty();
        tap(&mut editor, "mouse", (100.0, 200.0), 10.0);
        tap(&mut editor, "mouse", (100.0, 200.0), 10.1);
        assert!(editor.text_editor().is_some());
    }

    #[test]
    fn a_single_tap_on_empty_canvas_makes_no_text() {
        let mut editor = empty();
        tap(&mut editor, "finger", (100.0, 200.0), 10.0);
        assert!(editor.text_editor().is_none());
    }

    #[test]
    fn a_double_tap_on_empty_canvas_makes_one_text_not_two() {
        let mut editor = empty();
        double_tap(&mut editor, (100.0, 200.0));
        editor.set_editing_text("A");
        editor.commit_text();
        // A third tap soon after starts a new double-tap; it makes nothing alone.
        tap(&mut editor, "finger", (400.0, 400.0), 10.4);
        assert!(editor.text_editor().is_none());
        assert_eq!(texts(&editor).len(), 1);
    }

    const LINE: &str = r#"{"id":"l","type":"line","x":100,"y":300,"width":100,"height":0,
    "points":[[0,0],[100,0]],"strokeWidth":2,"version":3,"versionNonce":7}"#;

    const STROKE: &str = r#"{"id":"f","type":"freedraw","x":100,"y":300,"width":100,"height":0,
    "points":[[0,0],[50,0],[100,0]],"pressures":[],"simulatePressure":true,
    "strokeWidth":2,"version":3,"versionNonce":7}"#;

    #[test]
    fn a_double_tap_on_a_stroke_makes_a_text_at_the_tap() {
        let mut editor = measured(editor_with(&[STROKE]));

        double_tap(&mut editor, (150.0, 300.0));

        let shown = open(&editor);
        assert_eq!((shown.x, shown.y), (150.0, 287.5));
        editor.set_editing_text("Hi");
        editor.commit_text();
        assert_eq!(string(&only_text(&editor), "text"), "Hi");
    }

    #[test]
    fn a_double_tap_on_the_only_selected_line_makes_no_text() {
        // Excalidraw edits the points of that line instead.
        let mut editor = measured(editor_with(&[LINE]));
        double_tap(&mut editor, (150.0, 300.0));
        assert!(editor.text_editor().is_none());
    }

    #[test]
    fn a_double_tap_on_an_image_makes_no_text() {
        // Excalidraw crops it instead.
        let image = r#"{"id":"i","type":"image","x":100,"y":300,"width":100,"height":50,
        "fileId":"f1","status":"saved","scale":[1,1],"version":3,"versionNonce":7}"#;
        let mut editor = measured(editor_with(&[image]));
        double_tap(&mut editor, (150.0, 325.0));
        assert!(editor.text_editor().is_none());
    }

    #[test]
    fn a_single_tap_on_text_only_selects_it() {
        let mut editor = with_hello();
        tap(&mut editor, "finger", (120.0, 60.0), 10.0);
        assert!(editor.text_editor().is_none());
        assert_eq!(editor.selection(), ["t"]);
    }

    #[test]
    fn a_double_click_with_a_mouse_edits_too() {
        let mut editor = with_hello();
        tap(&mut editor, "mouse", (120.0, 60.0), 10.0);
        tap(&mut editor, "mouse", (120.0, 60.0), 10.1);
        assert_eq!(open(&editor).text, "Hello");
    }

    #[test]
    fn the_edited_text_is_not_drawn_under_the_editor() {
        let mut editor = with_hello();
        assert_eq!(drawn(&editor), ["Hello"]);

        double_tap(&mut editor, (120.0, 60.0));
        assert_eq!(drawn(&editor), Vec::<String>::new());

        editor.commit_text();
        assert_eq!(drawn(&editor), ["Hello"]);
    }

    #[test]
    fn committing_saves_the_new_text_and_its_size() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));

        editor.set_editing_text("Hello world\nagain");
        editor.commit_text();

        let text = only_text(&editor);
        assert_eq!(string(&text, "text"), "Hello world\nagain");
        assert_eq!(string(&text, "originalText"), "Hello world\nagain");
        assert_eq!(
            (number(&text, "width"), number(&text, "height")),
            (110.0, 50.0)
        );
        assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 50.0));
        assert_eq!(editor.selection(), ["t"]);
    }

    #[test]
    fn editing_is_one_undo_step() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));
        editor.set_editing_text("Hi");
        editor.set_editing_text("Hi there");
        editor.commit_text();

        editor.undo();
        let text = only_text(&editor);
        assert_eq!(string(&text, "text"), "Hello");
        assert_eq!(number(&text, "width"), 50.0);

        editor.redo();
        assert_eq!(string(&only_text(&editor), "text"), "Hi there");
    }

    #[test]
    fn committing_without_a_change_is_no_undo_step() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));
        editor.commit_text();
        assert!(!editor.can_undo());
    }

    #[test]
    fn cancelling_keeps_the_text_as_it_was() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));
        editor.set_editing_text("Something else");
        editor.cancel_text();
        assert_eq!(string(&only_text(&editor), "text"), "Hello");
        assert!(!editor.can_undo());
    }

    #[test]
    fn emptying_a_text_deletes_it_and_undo_brings_it_back() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));
        editor.set_editing_text("");
        editor.commit_text();
        assert!(texts(&editor).is_empty());
        assert!(editor.selection().is_empty());

        editor.undo();
        assert_eq!(string(&only_text(&editor), "text"), "Hello");
    }

    #[test]
    fn a_double_tap_into_a_selected_group_still_goes_into_the_group() {
        let second = HELLO
            .replace(r#""id":"t""#, r#""id":"u""#)
            .replace("100,\"y\"", "300,\"y\"");
        let first = HELLO.replace(r#""type":"text""#, r#""type":"text","groupIds":["g"]"#);
        let second = second.replace(r#""type":"text""#, r#""type":"text","groupIds":["g"]"#);
        let mut editor = measured(editor_with(&[&first, &second]));

        double_tap(&mut editor, (120.0, 60.0));

        assert!(editor.text_editor().is_none());
    }

    #[test]
    fn text_of_a_fixed_width_wraps_to_it_while_it_is_edited() {
        let fixed = HELLO
            .replace(r#""width":50"#, r#""width":60"#)
            .replace(r#""autoResize":true"#, r#""autoResize":false"#);
        let mut editor = measured(editor_with(&[&fixed]));
        double_tap(&mut editor, (120.0, 60.0));
        assert!(open(&editor).fixed_width);

        editor.set_editing_text("Hello there");

        let shown = open(&editor);
        assert_eq!((shown.width, shown.height), (60.0, 50.0));
        assert_eq!(shown.text, "Hello there");
        editor.commit_text();
        let text = only_text(&editor);
        assert_eq!(string(&text, "text"), "Hello\nthere");
        assert_eq!(string(&text, "originalText"), "Hello there");
    }

    #[test]
    fn the_text_tool_creates_a_text_where_another_element_is() {
        let rectangle = r##"{"id":"r","type":"rectangle","x":50,"y":50,"width":200,"height":100,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,
        "version":3,"versionNonce":7}"##;
        let mut editor = measured(editor_with(&[rectangle]));
        editor.set_tool(Tool::Text);

        tap(&mut editor, "finger", (150.0, 100.0), 1.0);
        editor.set_editing_text("Label");
        editor.commit_text();

        assert_eq!(string(&only_text(&editor), "text"), "Label");
        assert_eq!(saved_elements(&editor).len(), 2);
    }

    #[test]
    fn with_the_tool_lock_the_text_tool_stays_for_the_next_text() {
        let mut editor = with_text_tool();
        editor.set_tool_lock(true);
        tap(&mut editor, "finger", (100.0, 200.0), 1.0);
        editor.set_editing_text("One");
        editor.commit_text();
        assert_eq!(editor.tool(), Tool::Text);
        assert!(editor.selection().is_empty());

        tap(&mut editor, "finger", (100.0, 300.0), 2.0);
        editor.set_editing_text("Two");
        editor.commit_text();
        assert_eq!(texts(&editor).len(), 2);
    }

    #[test]
    fn loading_a_drawing_closes_the_editor() {
        let mut editor = typing("Hello", (100.0, 200.0));
        editor
            .load(br#"{"type":"excalidraw","version":2,"elements":[]}"#)
            .unwrap();
        assert!(editor.text_editor().is_none());
    }

    // --- Where the editor is ----------------------------------------------------------

    #[test]
    fn the_editor_follows_panning_and_zooming() {
        let mut editor = with_hello();
        editor.set_view_size(800.0, 600.0);
        double_tap(&mut editor, (120.0, 60.0));
        assert_eq!(open(&editor).zoom, 1.0);

        editor.zoom_in();
        // Zooming acts around the middle of the view, (400, 300): the point
        // (100, 50) of the Scene moves away from it with the zoom of 1.1.
        let shown = open(&editor);
        assert_eq!(shown.zoom, 1.1);
        assert!(
            (shown.x - (400.0 + (100.0 - 400.0) * 1.1)).abs() < 1e-9,
            "{}",
            shown.x
        );
        assert!(
            (shown.y - (300.0 + (50.0 - 300.0) * 1.1)).abs() < 1e-9,
            "{}",
            shown.y
        );
        assert!((shown.font_size - 22.0).abs() < 1e-9);
        assert!((shown.width - 55.0).abs() < 1e-9);
        assert!((shown.height - 27.5).abs() < 1e-9);
    }

    #[test]
    fn the_editor_has_the_style_of_the_text() {
        let mut editor = with_hello();
        double_tap(&mut editor, (120.0, 60.0));
        let shown = open(&editor);
        assert_eq!(shown.color, 0xe03131ff);
        assert_eq!(shown.opacity, 1.0);
        assert_eq!(shown.font_family, 5);
        assert_eq!(shown.angle, 0.0);
    }

    #[test]
    fn a_turned_text_is_edited_turned() {
        let turned = HELLO.replace(r#""text":"Hello""#, r#""angle":0.5,"text":"Hello""#);
        let mut editor = measured(editor_with(&[&turned]));
        double_tap(&mut editor, (125.0, 62.5));
        assert_eq!(open(&editor).angle, 0.5);
    }

    // --- Alignment keeps the text where it is anchored -----------------------------------

    #[test]
    fn centred_text_grows_to_both_sides() {
        let mut editor = with_text_tool();
        editor.set_property(Property::TextAlign(TextAlign::Center));
        tap(&mut editor, "finger", (200.0, 100.0), 1.0);

        editor.set_editing_text("Hello");
        editor.commit_text();

        // A space is 10 wide, so the empty text is centred on the tap with its
        // left edge at 195; five characters are 50 wide: 175.
        let text = only_text(&editor);
        assert_eq!(string(&text, "textAlign"), "center");
        assert_eq!((number(&text, "x"), number(&text, "width")), (175.0, 50.0));
    }

    #[test]
    fn right_aligned_text_grows_to_the_left() {
        let mut editor = with_text_tool();
        editor.set_property(Property::TextAlign(TextAlign::Right));
        tap(&mut editor, "finger", (200.0, 100.0), 1.0);

        editor.set_editing_text("Hello");
        editor.commit_text();

        let text = only_text(&editor);
        assert_eq!((number(&text, "x"), number(&text, "width")), (150.0, 50.0));
    }

    // --- Font size and alignment ----------------------------------------------------------

    #[test]
    fn the_font_size_of_the_next_text_is_the_chosen_one() {
        let mut editor = with_text_tool();
        editor.set_property(Property::FontSize(FontSize::Large));
        tap(&mut editor, "finger", (100.0, 200.0), 1.0);
        assert_eq!(open(&editor).font_size, 28.0);

        editor.set_editing_text("Hi");
        editor.commit_text();

        let text = only_text(&editor);
        assert_eq!(number(&text, "fontSize"), 28.0);
        assert_eq!(
            (number(&text, "width"), number(&text, "height")),
            (28.0, 35.0)
        );
    }

    #[test]
    fn the_panel_shows_the_font_size_and_alignment_of_the_next_text() {
        let mut editor = empty();
        assert_eq!(editor.properties().font_size, Some(FontSize::Medium));
        assert_eq!(editor.properties().text_align, Some(TextAlign::Left));
        editor.set_property(Property::FontSize(FontSize::ExtraLarge));
        editor.set_property(Property::TextAlign(TextAlign::Right));
        assert_eq!(editor.properties().font_size, Some(FontSize::ExtraLarge));
        assert_eq!(editor.properties().text_align, Some(TextAlign::Right));
    }

    #[test]
    fn the_panel_offers_font_size_and_alignment_for_texts_only() {
        let rectangle = r##"{"id":"r","type":"rectangle","x":300,"y":300,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,
        "version":3,"versionNonce":7}"##;
        let mut editor = measured(editor_with(&[HELLO, rectangle]));
        // Nothing selected: for the next Element, which is a text with the text Tool.
        assert!(!editor.properties().sections.text);
        editor.set_tool(Tool::Text);
        assert!(editor.properties().sections.text);
        editor.set_tool(Tool::Selection);

        tap(&mut editor, "finger", (320.0, 300.0), 1.0);
        assert_eq!(editor.selection(), ["r"]);
        assert!(!editor.properties().sections.text);

        editor.select_all();
        assert!(editor.properties().sections.text);
    }

    #[test]
    fn the_panel_offers_font_size_and_alignment_while_a_text_is_edited() {
        let editor = typing("Hi", (100.0, 200.0));
        assert!(editor.properties().sections.text);
    }

    #[test]
    fn the_panel_shows_the_font_size_and_alignment_of_the_selected_text() {
        let mut editor = with_hello();
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);
        assert_eq!(editor.properties().font_size, Some(FontSize::Medium));
        assert_eq!(editor.properties().text_align, Some(TextAlign::Left));
    }

    #[test]
    fn a_font_size_that_is_none_of_the_options_shows_none_chosen() {
        let odd = HELLO.replace(r#""fontSize":20"#, r#""fontSize":21"#);
        let mut editor = measured(editor_with(&[&odd]));
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);
        assert_eq!(editor.properties().font_size, None);
    }

    #[test]
    fn the_font_size_applies_to_the_selected_text_and_resizes_it() {
        let mut editor = with_hello();
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);

        editor.set_property(Property::FontSize(FontSize::Large));

        let text = only_text(&editor);
        assert_eq!(number(&text, "fontSize"), 28.0);
        // Five characters of 14, one line of 35; anchored at the left and top.
        assert_eq!(
            (number(&text, "width"), number(&text, "height")),
            (70.0, 35.0)
        );
        assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 50.0));
        assert_eq!(editor.properties().font_size, Some(FontSize::Large));
    }

    #[test]
    fn the_font_size_of_a_centred_text_grows_about_its_middle() {
        let centred = HELLO.replace(r#""textAlign":"left""#, r#""textAlign":"center""#);
        let mut editor = measured(editor_with(&[&centred]));
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);

        editor.set_property(Property::FontSize(FontSize::Large));

        // 50 wide at 100 becomes 70 wide: 10 further left.
        assert_eq!(number(&only_text(&editor), "x"), 90.0);
    }

    #[test]
    fn the_font_size_is_one_undo_step() {
        let mut editor = with_hello();
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);
        editor.set_property(Property::FontSize(FontSize::Large));

        editor.undo();

        let text = only_text(&editor);
        assert_eq!(number(&text, "fontSize"), 20.0);
        assert_eq!(number(&text, "width"), 50.0);
    }

    #[test]
    fn the_alignment_applies_to_the_selected_text() {
        let mut editor = with_hello();
        tap(&mut editor, "finger", (120.0, 60.0), 1.0);

        editor.set_property(Property::TextAlign(TextAlign::Right));

        assert_eq!(string(&only_text(&editor), "textAlign"), "right");
        assert_eq!(editor.properties().text_align, Some(TextAlign::Right));
        let geometry = editor.geometry();
        assert_eq!(geometry.text_lines()[0].align, TextAlign::Right as u32);
    }

    #[test]
    fn font_size_and_alignment_leave_other_elements_alone() {
        let rectangle = r##"{"id":"r","type":"rectangle","x":300,"y":300,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,
        "version":3,"versionNonce":7}"##;
        let mut editor = measured(editor_with(&[HELLO, rectangle]));
        editor.select_all();
        assert_eq!(editor.properties().font_size, Some(FontSize::Medium));

        editor.set_property(Property::FontSize(FontSize::Small));

        let rectangle = saved_elements(&editor)
            .into_iter()
            .find(|e| string(e, "id") == "r")
            .unwrap();
        assert_eq!(rectangle.get("fontSize"), None);
        assert_eq!(number(&only_text(&editor), "fontSize"), 16.0);
    }

    #[test]
    fn the_font_size_applies_to_the_text_being_edited() {
        let mut editor = typing("Hi", (100.0, 200.0));

        editor.set_property(Property::FontSize(FontSize::ExtraLarge));

        let shown = open(&editor);
        assert_eq!(shown.font_size, 36.0);
        assert_eq!((shown.width, shown.height), (36.0, 45.0));
        // The editor stays open: the panel is a change of the text, not an end.
        editor.commit_text();
        assert_eq!(number(&only_text(&editor), "fontSize"), 36.0);
        // And the next text has the size too.
        assert_eq!(editor.properties().font_size, Some(FontSize::ExtraLarge));
    }

    #[test]
    fn the_alignment_applies_to_the_text_being_edited() {
        let mut editor = typing("Hi", (100.0, 200.0));
        editor.set_property(Property::TextAlign(TextAlign::Center));
        assert_eq!(open(&editor).align, TextAlign::Center);
        editor.commit_text();
        assert_eq!(string(&only_text(&editor), "textAlign"), "center");
    }

    #[test]
    fn other_properties_apply_to_the_text_being_edited_too() {
        let mut editor = typing("Hi", (100.0, 200.0));
        editor.set_property(Property::StrokeColor("#1971c2".into()));
        assert_eq!(open(&editor).color, 0x1971c2ff);
        editor.commit_text();
        assert_eq!(string(&only_text(&editor), "strokeColor"), "#1971c2");
    }

    #[test]
    fn properties_that_text_does_not_have_leave_it_alone() {
        let mut editor = typing("Hi", (100.0, 200.0));
        editor.set_property(Property::StrokeStyle(LineStyle::Dashed));
        editor.commit_text();
        assert_eq!(string(&only_text(&editor), "strokeStyle"), "solid");
    }
}
