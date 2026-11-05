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
//! the arrows bound to it follow.

use crate::binding::Attachments;
use crate::color::parse_color;
use crate::draw_shape::new_element;
use crate::geometry::TextAlign;
use crate::history::Step;
use crate::json::{Object, Value};
use crate::path::Point;
use crate::properties::{self, Property, align_name};
use crate::random::Entropy;
use crate::scene::{Element, Scene};
use crate::style::Style;
use crate::text::{self, TextMeasurer};
use crate::viewport::Viewport;
use crate::wrap;

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
        let (origin, angle) = ((container.x(), container.y()), container.angle());
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
        let size = {
            let (Some(label), Some(shape)) =
                (scene.element(self.handle), scene.element(container.handle))
            else {
                return;
            };
            text::container_size_for(&label, &shape, container.original_height, measurer)
        };
        if let Some(shape) = scene.element(container.handle)
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
/// label sits in it by its alignment, so its middle is off the box's by
/// whatever the lines leave over, along the (turned) top of the container.
fn label_box(label: &Element, container: &Element) -> (f64, f64, f64, f64) {
    let width = text::label_max_width(container.kind(), container.width());
    let height = label.height();
    let off = match label.text_align() {
        TextAlign::Left => (width - label.width()) / 2.0,
        TextAlign::Center => 0.0,
        TextAlign::Right => (label.width() - width) / 2.0,
    };
    let (sin, cos) = container.angle().sin_cos();
    let middle = (
        label.x() + label.width() / 2.0 + off * cos,
        label.y() + label.height() / 2.0 + off * sin,
    );
    (
        middle.0 - width / 2.0,
        middle.1 - height / 2.0,
        width,
        height,
    )
}
