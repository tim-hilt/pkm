//! Creating and editing text (ADR-0006), as Excalidraw does with its
//! `textarea` (`textWysiwyg`): the client shows a native text view over the
//! Canvas where the core says, with the text and style it says, and tells the
//! core what is typed. The core keeps the text Element in step, so that the
//! text view and the Element always agree on where the text is and how big.
//!
//! Like on excalidraw.com the Element is in the Scene while it is edited, but
//! not drawn: the text view shows it. Committing makes the edit one undo step
//! (creating the text, or changing it); an empty text is not kept.

use crate::draw_shape::new_element;
use crate::geometry::TextAlign;
use crate::history::Step;
use crate::json::Value;
use crate::path::Point;
use crate::properties::{self, Property, align_name};
use crate::random::Entropy;
use crate::scene::{Element, Scene};
use crate::shape::parse_color;
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
    step: Step,
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
        let font_size = style.font_size.size();
        let width = wrap::widest(&[String::new()], font_size, measurer);
        let height = font_size * LINE_HEIGHT;
        let origin = (
            at.0 - text::anchor_ratio(style.text_align) * width,
            at.1 - height / 2.0,
        );
        let mut element = new_element("text", None, entropy, style, origin);
        let text = |s: &str| Value::String(s.to_owned());
        for (key, value) in [
            ("width", Value::Number(width)),
            ("height", Value::Number(height)),
            ("text", text("")),
            ("fontSize", Value::Number(font_size)),
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
        let id = element
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        step.create(&id);
        let handle = scene.push(element);
        Self {
            handle,
            id,
            existing: false,
            step,
        }
    }

    /// Starts editing the text `element` that is there, as the action of
    /// `step`.
    pub fn begin_existing(mut step: Step, scene: &Scene, element: &Element) -> Self {
        step.touch(scene, element.id());
        Self {
            handle: element.handle(),
            id: element.id().to_owned(),
            existing: true,
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
        if changed
            && let Some(element) = scene.element(self.handle)
            && let Some(members) = text::refit(&element, None, measurer)
        {
            scene.update(self.handle, entropy.integer(), members);
        }
        changed
    }

    /// What the client shows for the text, seen through `viewport`.
    pub fn editor(&self, scene: &Scene, viewport: Viewport) -> Option<TextEditor> {
        let element = scene.element(self.handle)?;
        let zoom = viewport.zoom();
        let (x, y) = viewport.view_at((element.x(), element.y()));
        Some(TextEditor {
            text: element.original_text().to_owned(),
            x,
            y,
            width: element.width() * zoom,
            height: element.height() * zoom,
            font_size: element.font_size() * zoom,
            line_height: element.line_height(),
            font_family: element.font_family(),
            align: element.text_align(),
            color: parse_color(element.stroke_color()),
            opacity: element.opacity(),
            angle: element.angle(),
            zoom,
            fixed_width: !element.auto_resize(),
        })
    }

    /// Ends the editing with the text as it is. If there is something in it
    /// the Element stays, and the step is what undoes the editing; an empty
    /// text is not kept, which for one that was there before deletes it.
    pub fn finish(self, scene: &mut Scene, entropy: &mut Entropy) -> Option<Step> {
        let blank = scene
            .element(self.handle)
            .is_none_or(|element| element.original_text().trim().is_empty());
        match (blank, self.existing) {
            (false, _) => Some(self.step),
            (true, true) => {
                scene.mark_deleted(&self.id, entropy.integer());
                Some(self.step)
            }
            (true, false) => {
                scene.remove(self.handle);
                None
            }
        }
    }

    /// Ends the editing without any change to the text.
    pub fn cancel(self, scene: &mut Scene, entropy: &mut Entropy) {
        if self.existing {
            self.step.revert(scene, entropy);
        } else {
            scene.remove(self.handle);
        }
    }
}
