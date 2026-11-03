//! Resizing and rotating the Selection by its transform handles (ADR-0006),
//! as Excalidraw does (`resizeElements.ts` at the pinned commit).
//!
//! The Elements of the Selection are transformed as a unit: resizing scales
//! them all about the corner or side opposite the handle that is dragged,
//! rotating turns them all about the middle of the box. Each Element follows
//! the rules of its kind — the points of lines and freedraw strokes rescale,
//! a text's font scales and its lines wrap again, an image mirrors — and a
//! label follows the container it sits in. Every step is a new version of
//! each touched Element, as on excalidraw.com, which is what makes the
//! transform part of the saved Drawing.
//!
//! While a pointer resizes, Shift (on the Mac) keeps the ratio of the sides;
//! an image keeps it anyway and Shift frees it. While a pointer rotates,
//! Shift snaps the rotation to fifteenths of a turn.
//!
//! A line or arrow alone has no transform handles: it is edited at its
//! points instead (see [`crate::edit_points`]). An arrow routed at right angles is
//! transformed like any other line here, points and all, where Excalidraw
//! re-routes it; the core keeps its points as the file has them.

use crate::frame::Frame;
use crate::history::Step;
use crate::json::{Object, Value};
use crate::path::Point;
use crate::random::Entropy;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use crate::text::{self, TextMeasurer};
use crate::transform_handles::{Handle, TransformBox};

/// Excalidraw's `SHIFT_LOCKING_ANGLE`: the angle a snapped rotation is a
/// multiple of.
pub(crate) const SHIFT_LOCKING_ANGLE: f64 = std::f64::consts::PI / 12.0;
/// Excalidraw's `MIN_FONT_SIZE`: a resize that would make a font smaller
/// than this does not happen.
const MIN_FONT_SIZE: f64 = 1.0;
const TWO_PI: f64 = std::f64::consts::TAU;

/// An Element the gesture transforms, as it was when the gesture began
/// (Excalidraw's `originalElements`). Resizing follows the pointer from the
/// Elements as they began, not as the last step left them, so dragging back
/// undoes the steps before.
#[derive(Debug)]
struct Tracked {
    handle: u32,
    snapshot: Object,
}

/// A label that follows the container it sits in.
#[derive(Debug)]
struct Label {
    handle: u32,
    container: u32,
    snapshot: Object,
}

impl Tracked {
    /// The Element as it began.
    fn element(&self) -> Option<Element<'_>> {
        Element::view(self.handle, &self.snapshot)
    }
}

impl Label {
    /// The label as it began.
    fn element(&self) -> Option<Element<'_>> {
        Element::view(self.handle, &self.snapshot)
    }

    /// The Element `id` of the label, if the file gave it one.
    fn id(&self) -> Option<&str> {
        self.snapshot.get("id").and_then(Value::as_str)
    }
}

/// A resize or rotation of the Selection by one pointer.
#[derive(Debug)]
pub(crate) struct TransformGesture {
    pointer: u64,
    handle: Handle,
    /// The box the Selection is transformed as, as it was when the gesture
    /// began.
    transform_box: TransformBox,
    /// How far the pointer that took hold of `handle` was from the part of
    /// the box it stands for; the transform follows the pointer less that
    /// much, so that the box does not jump to it.
    offset: Point,
    /// The Elements the gesture transforms: those of the Selection, labels
    /// excepted, each with how it began.
    tracked: Vec<Tracked>,
    /// The labels of the tracked Elements, each with how it began.
    labels: Vec<Label>,
    /// The effective pointer of the last step, to tell a pointer that moved
    /// from one that is where it was.
    applied: Option<Point>,
    /// The angle of the last rotation, to tell a pointer that moved from one
    /// that is where it was.
    turned: Option<f64>,
    step: Step,
}

impl TransformGesture {
    /// Begins transforming the Selection with `pointer`, which went down
    /// holding `handle` of `transform_box` at `origin`. The transform is the
    /// action of `step`.
    pub fn begin(
        mut step: Step,
        scene: &Scene,
        selection: &Selection,
        pointer: u64,
        transform_box: TransformBox,
        handle: Handle,
        origin: Point,
    ) -> Self {
        let mut tracked = Vec::new();
        for element in crate::transform_handles::targets(scene, selection) {
            if let Some(snapshot) = scene.snapshot(element.id()) {
                step.touch(scene, element.id());
                tracked.push(Tracked {
                    handle: element.handle(),
                    snapshot,
                });
            }
        }
        let mut labels = Vec::new();
        for element in scene.elements() {
            let Some(container) = element.container_id() else {
                continue;
            };
            let transforms = tracked
                .iter()
                .filter_map(Tracked::element)
                .any(|e| e.id() == container);
            if element.is_deleted() || !transforms {
                continue;
            }
            if let Some(snapshot) = scene.snapshot(element.id()) {
                step.touch(scene, element.id());
                labels.push(Label {
                    handle: element.handle(),
                    container: container_of(&tracked, container),
                    snapshot,
                });
            }
        }
        Self {
            pointer,
            handle,
            offset: transform_box.grab_offset(handle, origin),
            transform_box,
            tracked,
            labels,
            applied: None,
            turned: None,
            step,
        }
    }

    /// The pointer that transforms the Selection.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// Ends the transform where it is; the step is what undoes it.
    pub fn finish(self) -> Step {
        self.step
    }

    /// Puts every Element back as it was before the gesture.
    pub fn cancel(self, scene: &mut Scene, entropy: &mut Entropy) {
        self.step.revert(scene, entropy);
    }

    /// Follows the pointer to `to`, keeping the ratio of the sides or
    /// snapping the rotation if `shift` is held. Returns whether anything
    /// changed.
    pub fn drag_to(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        to: Point,
        shift: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let at = (to.0 - self.offset.0, to.1 - self.offset.1);
        if self.applied == Some(at) {
            return false;
        }
        self.applied = Some(at);
        match self.handle {
            Handle::Rotation => self.rotate(scene, entropy, at, shift),
            _ => self.resize(scene, entropy, at, shift, measurer),
        }
    }

    /// Whether a font may become this small (Excalidraw's `MIN_FONT_SIZE`).
    fn font_fits(font: f64) -> bool {
        font >= MIN_FONT_SIZE
    }

    /// Whether the Selection keeps the ratio of its sides of itself: images
    /// do, and Shift frees them; everything else needs Shift for it.
    fn keeps_ratio(&self, shift: bool) -> bool {
        let images = self
            .tracked
            .iter()
            .any(|t| t.element().is_some_and(|e| e.kind() == ElementType::Image));
        if images { !shift } else { shift }
    }

    // --- Rotation -----------------------------------------------------------

    /// Turns the Selection about the middle of its box so that it points
    /// where the pointer does from there.
    fn rotate(&mut self, scene: &mut Scene, entropy: &mut Entropy, at: Point, shift: bool) -> bool {
        let angle = self.rotation(at, shift);
        if Some(angle) == self.turned {
            return false;
        }
        self.turned = Some(angle);
        if self.tracked.len() == 1 {
            self.rotate_single(scene, entropy, angle);
        } else {
            let centre = self.transform_box.frame().centre();
            self.rotate_many(scene, entropy, centre, angle);
        }
        true
    }

    /// The angle the Selection is to point at: where the pointer is from the
    /// middle of the box, counted from straight above it; snapped to
    /// fifteenths of a turn if `shift` is held.
    fn rotation(&self, at: Point, shift: bool) -> f64 {
        let (cx, cy) = self.transform_box.frame().centre();
        let mut angle = TWO_PI * 1.25 + (at.1 - cy).atan2(at.0 - cx);
        if shift {
            angle += SHIFT_LOCKING_ANGLE / 2.0;
            angle -= angle % SHIFT_LOCKING_ANGLE;
        }
        normalized(angle)
    }

    /// Turns the one Element of the Selection: it keeps where it is and
    /// takes the angle.
    fn rotate_single(&mut self, scene: &mut Scene, entropy: &mut Entropy, angle: f64) {
        let Some(tracked) = self.tracked.first() else {
            return;
        };
        scene.update(
            tracked.handle,
            entropy.integer(),
            [("angle", Value::Number(angle))],
        );
        self.refit_label(
            scene,
            entropy,
            tracked.handle,
            None,
            Some(angle),
            false,
            None,
        );
    }

    /// Turns every Element of the Selection about `centre`: each keeps its
    /// own angle on top of the turn, and moves with its middle.
    fn rotate_many(&mut self, scene: &mut Scene, entropy: &mut Entropy, centre: Point, angle: f64) {
        let (sin, cos) = angle.sin_cos();
        let mut moves = Vec::new();
        for tracked in &self.tracked {
            let Some(orig) = tracked.element() else {
                continue;
            };
            let (cx, cy) = Frame::of(&orig).centre();
            let (dx, dy) = (cx - centre.0, cy - centre.1);
            moves.push((
                tracked.handle,
                orig.x() + dx * cos - dy * sin - dx,
                orig.y() + dx * sin + dy * cos - dy,
                normalized(orig.angle() + angle),
            ));
        }
        for (handle, x, y, new_angle) in moves {
            scene.update(
                handle,
                entropy.integer(),
                [
                    ("x", Value::Number(x)),
                    ("y", Value::Number(y)),
                    ("angle", Value::Number(new_angle)),
                ],
            );
            self.refit_label(scene, entropy, handle, None, Some(new_angle), false, None);
        }
    }

    // --- Resizing -----------------------------------------------------------

    /// Scales the Selection as its box does when `handle` is dragged.
    fn resize(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        shift: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let aspect = self.keeps_ratio(shift);
        if self.tracked.len() == 1 {
            self.resize_single(scene, entropy, at, aspect, measurer)
        } else {
            self.resize_many(scene, entropy, at, aspect, measurer)
        }
    }

    /// Scales the one Element of the Selection from the pointer: the sides
    /// the handle is on follow it, the opposite corner or side stays.
    fn resize_single(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        aspect: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let Some(tracked) = self.tracked.first() else {
            return false;
        };
        let handle = tracked.handle;
        // The Element as it began, out of the gesture: the views below outlive
        // the borrow of it.
        let snapshot = tracked.snapshot.clone();
        let Some(orig) = Element::view(handle, &snapshot) else {
            return false;
        };
        let (shown_width, shown_height, shown_font_size) = scene
            .element(handle)
            .map(|e| (e.width(), e.height(), e.font_size()))
            .unwrap_or((orig.width(), orig.height(), orig.font_size()));
        // The pointer in the Element's own, unturned frame: resizing keeps
        // the Element's angle and works in the sides it had before it.
        let (x, y) = unturned(at, Frame::of(&orig).centre(), orig.angle());
        let mut next_width = if self.handle.is_east() {
            x - orig.x()
        } else if self.handle.is_west() {
            orig.x() + orig.width() - x
        } else {
            orig.width()
        };
        let mut next_height = if self.handle.is_south() {
            y - orig.y()
        } else if self.handle.is_north() {
            orig.y() + orig.height() - y
        } else {
            orig.height()
        };
        // Keeping the ratio of the sides: from a corner both sides take the
        // scale of the longer one; from a side the other side follows the
        // one that is dragged.
        if aspect {
            let width_ratio = next_width.abs() / orig.width();
            let height_ratio = next_height.abs() / orig.height();
            if self.handle.is_corner() {
                let ratio = width_ratio.max(height_ratio);
                next_width = orig.width() * ratio * next_width.signum();
                next_height = orig.height() * ratio * next_height.signum();
            } else {
                next_height *= width_ratio;
                next_width *= height_ratio;
            }
        }
        if orig.kind() == ElementType::Text {
            return self.resize_text(
                scene,
                entropy,
                &orig,
                shown_width,
                shown_height,
                shown_font_size,
                next_width,
                next_height,
                measurer,
            );
        }
        self.apply_single(
            scene,
            entropy,
            orig,
            next_width,
            next_height,
            aspect,
            measurer,
        )
    }

    /// Lays a text out again at its new size: its font scales with its
    /// height, and its width rewraps its lines (Excalidraw's
    /// `resizeSingleTextElement`). A corner keeps the ratio of the sides, as
    /// keeping it does.
    #[allow(clippy::too_many_arguments)]
    fn resize_text(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        orig: &Element,
        shown_width: f64,
        shown_height: f64,
        shown_font_size: f64,
        next_width: f64,
        next_height: f64,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let Some(measurer) = measurer else {
            return false;
        };
        let pointer_width = next_width;
        let corner = self.handle.is_corner();
        let next_height = if corner {
            next_height.max(next_width / orig.width() * orig.height())
        } else {
            next_height
        };
        // From the top or bottom, the width scales with the height.
        let next_width = shown_width * (next_height / shown_height);
        let font = shown_font_size * (next_height / shown_height);
        if !Self::font_fits(font) {
            return false;
        }
        if corner || self.handle.is_north() || self.handle.is_south() {
            let origin = resized_origin(
                (orig.x(), orig.y()),
                orig.width(),
                orig.height(),
                next_width,
                next_height,
                orig.angle(),
                self.handle,
                false,
            );
            scene.update(
                orig.handle(),
                entropy.integer(),
                [
                    ("x", Value::Number(origin.0)),
                    ("y", Value::Number(origin.1)),
                    ("fontSize", Value::Number(font)),
                    ("width", Value::Number(next_width)),
                    ("height", Value::Number(next_height)),
                ],
            );
            return true;
        }
        // From the sides: the lines wrap again to the new width, and the
        // height is the height the lines take.
        let width = pointer_width.max(text::min_text_width(shown_font_size, measurer));
        let lines = {
            let Some(current) = scene.snapshot(orig.id()) else {
                return false;
            };
            let Some(current) = Element::view(orig.handle(), &current) else {
                return false;
            };
            text::set_in_lines(&current, width.abs(), measurer)
        };
        let origin = resized_origin(
            (orig.x(), orig.y()),
            orig.width(),
            orig.height(),
            width,
            lines.height,
            orig.angle(),
            self.handle,
            false,
        );
        scene.update(
            orig.handle(),
            entropy.integer(),
            [
                ("x", Value::Number(origin.0)),
                ("y", Value::Number(origin.1)),
                ("width", Value::Number(width.abs())),
                ("height", Value::Number(lines.height)),
                ("text", Value::String(lines.text)),
                ("autoResize", Value::Bool(false)),
            ],
        );
        true
    }

    /// Scales the one Element of the Selection, its kind's rules applied:
    /// points rescale, an image mirrors, its label follows.
    #[allow(clippy::too_many_arguments)]
    fn apply_single(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        orig: Element<'_>,
        mut next_width: f64,
        mut next_height: f64,
        aspect: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        // The least size and the label's font use sizes as they are; the
        // signs are kept for the geometry below, where crossing the opposite
        // edge mirrors the Element.
        let flip_x = next_width < 0.0;
        let flip_y = next_height < 0.0;
        next_width = next_width.abs();
        next_height = next_height.abs();

        let mut label_font = None;
        if let Some(label) = self.label_of(orig.handle()) {
            if aspect {
                let font = label.font_size()
                    * (text::label_max_width(orig.kind(), next_width)
                        / text::label_max_width(orig.kind(), orig.width()));
                if !Self::font_fits(font) {
                    return false;
                }
                label_font = Some(font);
            } else {
                // The label's font is what it was when the gesture began;
                // only the lines rewrap.
                label_font = Some(label.font_size());
                if let Some(measurer) = measurer {
                    let (min_width, min_height) = text::min_container_size(&label, measurer);
                    next_width = next_width.max(min_width);
                    next_height = next_height.max(min_height);
                }
            }
        }

        let signed_width = if flip_x { -next_width } else { next_width };
        let signed_height = if flip_y { -next_height } else { next_height };
        let rescaled = rescale_points(&orig.points(), signed_width, signed_height, true);
        // A line or arrow alone has no handles (see `TransformBox::of`), so
        // only a freedraw stroke resizes alone: its points keep the least
        // coordinate they had, as Excalidraw's rescaling normalizes them.
        let previous = if linear(orig.kind()) {
            Frame::of(&orig).min
        } else {
            (orig.x(), orig.y())
        };
        let mut origin = resized_origin(
            previous,
            orig.width(),
            orig.height(),
            signed_width,
            signed_height,
            orig.angle(),
            self.handle,
            aspect,
        );
        // Mirroring puts the origin on the far side of the Element.
        if flip_x {
            origin.0 += signed_width;
        }
        if flip_y {
            origin.1 += signed_height;
        }
        if signed_width == 0.0 || signed_height == 0.0 {
            return false;
        }

        let mut members = vec![
            ("x", Value::Number(origin.0)),
            ("y", Value::Number(origin.1)),
            ("width", Value::Number(next_width)),
            ("height", Value::Number(next_height)),
        ];
        if linear(orig.kind()) {
            members.push(("points", points_value(&rescaled)));
        }
        if orig.kind() == ElementType::Image {
            let (scale_x, scale_y) = orig.image_scale();
            members.push((
                "scale",
                Value::Array(vec![
                    Value::Number(scale_x * signed_width.signum()),
                    Value::Number(scale_y * signed_height.signum()),
                ]),
            ));
        }
        scene.update(orig.handle(), entropy.integer(), members);
        // The label's font goes on before the lines wrap to the container it
        // sits in now.
        let rewrap = aspect || !(self.handle.is_north() || self.handle.is_south());
        self.refit_label(
            scene,
            entropy,
            orig.handle(),
            label_font,
            None,
            rewrap,
            measurer,
        );
        true
    }

    /// Scales every Element of the Selection as its box does: each from the
    /// corner or side opposite the handle, mirroring when the pointer
    /// crosses the anchor (Excalidraw's `resizeMultipleElements`).
    fn resize_many(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        aspect: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let Some(bounds) = self.common_bounds() else {
            return false;
        };
        let (width, height) = (bounds.width(), bounds.height());
        if width == 0.0 || height == 0.0 {
            return false;
        }
        let anchor = anchor_of(self.handle, bounds);
        let (flip_x, flip_y) = flips_of(self.handle, at, anchor);
        let mut next_width = if self.handle.is_east() || self.handle.is_west() {
            (at.0 - anchor.0).abs()
        } else {
            width
        };
        let mut next_height = if self.handle.is_north() || self.handle.is_south() {
            (at.1 - anchor.1).abs()
        } else {
            height
        };
        if next_width == 0.0 || next_height == 0.0 {
            return false;
        }
        if aspect {
            let scale = ((at.0 - anchor.0).abs() / width).max((at.1 - anchor.1).abs() / height);
            next_width = width * scale * (at.0 - anchor.0).signum();
            next_height = height * scale * (at.1 - anchor.1).signum();
        }
        if next_width == 0.0 || next_height == 0.0 {
            return false;
        }
        let mut scale_x = if self.handle.is_east() || self.handle.is_west() {
            next_width.abs() / width
        } else {
            1.0
        };
        let mut scale_y = if self.handle.is_north() || self.handle.is_south() {
            next_height.abs() / height
        } else {
            1.0
        };
        // Elements that are turned, or texts, keep the ratio of their sides
        // so that their angle and their font do not skew.
        let scale = if self.handle.is_corner() {
            (next_width.abs() / width).max(next_height.abs() / height)
        } else if self.handle.is_east() || self.handle.is_west() {
            scale_x
        } else {
            scale_y
        };
        let keeps = aspect
            || self.tracked.iter().any(|t| {
                t.element()
                    .is_some_and(|e| e.angle() != 0.0 || e.kind() == ElementType::Text)
            });
        if keeps {
            scale_x = scale;
            scale_y = scale;
        }

        let mut moves = Vec::new();
        let mut label_fonts = Vec::new();
        for tracked in &self.tracked {
            let Some(orig) = tracked.element() else {
                continue;
            };
            let (scaled_width, scaled_height) = (orig.width() * scale_x, orig.height() * scale_y);
            let angle = normalized(orig.angle() * flip_sign(flip_x) * flip_sign(flip_y));
            let points = linear(orig.kind());
            let (offset_x, offset_y) = (orig.x() - anchor.0, orig.y() - anchor.1);
            // Mirroring a made-of-points Element mirrors its points, so its
            // position does not shift over; any other does.
            let shift_x = if flip_x && !points { scaled_width } else { 0.0 };
            let shift_y = if flip_y && !points {
                scaled_height
            } else {
                0.0
            };
            let x = anchor.0 + flip_sign(flip_x) * (offset_x * scale_x + shift_x);
            let y = anchor.1 + flip_sign(flip_y) * (offset_y * scale_y + shift_y);
            let mut font = None;
            if orig.kind() == ElementType::Text {
                font = Some(orig.font_size() * scale_x);
                if !Self::font_fits(font.unwrap()) {
                    continue;
                }
            }
            let mut label_font = None;
            if let Some(label) = self.label_of(tracked.handle) {
                if keeps {
                    label_font = Some(label.font_size() * scale);
                    if !Self::font_fits(label_font.unwrap()) {
                        continue;
                    }
                } else {
                    label_font = Some(label.font_size());
                }
            }
            let mut members = vec![
                ("x", Value::Number(x)),
                ("y", Value::Number(y)),
                ("width", Value::Number(scaled_width)),
                ("height", Value::Number(scaled_height)),
                ("angle", Value::Number(angle)),
            ];
            if points {
                members.push((
                    "points",
                    points_value(&rescale_points(
                        &orig.points(),
                        scaled_width * flip_sign(flip_x),
                        scaled_height * flip_sign(flip_y),
                        false,
                    )),
                ));
            }
            if let Some(font) = font {
                members.push(("fontSize", Value::Number(font)));
            }
            if orig.kind() == ElementType::Image {
                let (scale_x, scale_y) = orig.image_scale();
                members.push((
                    "scale",
                    Value::Array(vec![
                        Value::Number(scale_x * flip_sign(flip_x)),
                        Value::Number(scale_y * flip_sign(flip_y)),
                    ]),
                ));
            }
            moves.push((tracked.handle, members));
            label_fonts.push((tracked.handle, label_font, angle));
        }
        for (handle, members) in moves {
            scene.update(handle, entropy.integer(), members);
        }
        for (container, label_font, angle) in label_fonts {
            self.refit_label(
                scene,
                entropy,
                container,
                label_font,
                Some(angle),
                true,
                measurer,
            );
        }
        true
    }

    /// The box around everything the gesture transforms, as it began, with
    /// the labels of lines and arrows included (Excalidraw's
    /// `getCommonBoundingBox`).
    fn common_bounds(&self) -> Option<Frame> {
        let mut bounds: Option<Frame> = None;
        let include = |frame: Frame, bounds: &mut Option<Frame>| {
            *bounds = Some(match *bounds {
                None => frame,
                Some(around) => around.union(frame),
            });
        };
        for tracked in &self.tracked {
            let orig = tracked.element()?;
            include(Frame::shown(&orig), &mut bounds);
            if orig.kind().is_linear() {
                for label in &self.labels {
                    if label.container != tracked.handle {
                        continue;
                    }
                    if let Some(label) = label.element() {
                        include(Frame::shown(&label), &mut bounds);
                    }
                }
            }
        }
        bounds
    }

    /// The label of the container with `handle`, as it began.
    fn label_of(&self, container: u32) -> Option<Element<'_>> {
        let label = self.labels.iter().find(|l| l.container == container)?;
        label.element()
    }

    /// Puts the label of the container with `handle` where it sits in the
    /// container as it is now. With `font` the label's font becomes it
    /// first; with `angle` the label turns with its container, which lines
    /// and arrows except against; with `rewrap` the lines wrap to the
    /// container again. An arrow's label keeps where the file has it.
    #[allow(clippy::too_many_arguments)]
    fn refit_label(
        &self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        container: u32,
        font: Option<f64>,
        angle: Option<f64>,
        rewrap: bool,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        let Some(label) = self.labels.iter().find(|l| l.container == container) else {
            return;
        };
        if let Some(font) = font {
            scene.update(
                label.handle,
                entropy.integer(),
                [("fontSize", Value::Number(font))],
            );
        }
        let fit = {
            let Some(container_element) = scene.element(container) else {
                return;
            };
            let is_arrow = container_element.kind() == ElementType::Arrow;
            let keeps_angle = !container_element.kind().is_linear();
            let Some(label_id) = label.id().and_then(|id| scene.snapshot(id)) else {
                return;
            };
            let Some(container_id) = scene.snapshot(container_element.id()) else {
                return;
            };
            let Some(label_element) = Element::view(label.handle, &label_id) else {
                return;
            };
            let Some(container_element) = Element::view(container, &container_id) else {
                return;
            };
            let wrap = if rewrap { measurer } else { None };
            (
                text::fit_label(&label_element, &container_element, wrap),
                is_arrow,
                keeps_angle,
            )
        };
        let (fit, is_arrow, keeps_angle) = fit;
        let mut members = Vec::new();
        if !is_arrow {
            members.push(("x", Value::Number(fit.x)));
            members.push(("y", Value::Number(fit.y)));
        }
        members.push(("width", Value::Number(fit.width)));
        members.push(("height", Value::Number(fit.height)));
        if let Some(text) = fit.text {
            members.push(("text", Value::String(text)));
        }
        if let (Some(angle), true) = (angle, keeps_angle) {
            members.push(("angle", Value::Number(angle)));
        }
        scene.update(label.handle, entropy.integer(), members);
    }
}

/// The handle of the tracked Element whose `id` is `id`.
fn container_of(tracked: &[Tracked], id: &str) -> u32 {
    tracked
        .iter()
        .find(|t| t.element().is_some_and(|e| e.id() == id))
        .map(|t| t.handle)
        .unwrap_or_default()
}

/// Whether a kind of Element is made of points.
fn linear(kind: ElementType) -> bool {
    kind.is_linear() || kind == ElementType::Freedraw
}

/// Turns `at` back by `angle` about `centre`: into the frame the Element had
/// before it was turned.
fn unturned(at: Point, centre: Point, angle: f64) -> Point {
    turned(at, centre, -angle)
}

/// Turns `at` by `angle` about `centre`, clockwise.
fn turned(at: Point, centre: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (at.0 - centre.0, at.1 - centre.1);
    (
        centre.0 + dx * cos - dy * sin,
        centre.1 + dx * sin + dy * cos,
    )
}

/// The angle in `0..2π` (Excalidraw's `normalizeRadians`).
fn normalized(angle: f64) -> f64 {
    angle.rem_euclid(TWO_PI)
}

/// `-1` for a mirrored axis, `1` otherwise.
fn flip_sign(flip: bool) -> f64 {
    if flip { -1.0 } else { 1.0 }
}

/// The part of the box of `bounds` that stays where it is when `handle` is
/// dragged (Excalidraw's anchor map).
fn anchor_of(handle: Handle, bounds: Frame) -> Point {
    let (width, height) = (bounds.width(), bounds.height());
    match handle {
        Handle::NE => (bounds.min.0, bounds.max.1),
        Handle::SE => (bounds.min.0, bounds.min.1),
        Handle::SW => (bounds.max.0, bounds.min.1),
        Handle::NW => (bounds.max.0, bounds.max.1),
        Handle::E => (bounds.min.0, bounds.min.1 + height / 2.0),
        Handle::W => (bounds.max.0, bounds.min.1 + height / 2.0),
        Handle::N => (bounds.min.0 + width / 2.0, bounds.max.1),
        Handle::S => (bounds.min.0 + width / 2.0, bounds.min.1),
        Handle::Rotation => (0.0, 0.0),
    }
}

/// Whether the pointer at `at`, dragging `handle` of a box of `anchor`,
/// mirrors the Selection across each axis.
fn flips_of(handle: Handle, at: Point, anchor: Point) -> (bool, bool) {
    match handle {
        Handle::NE => (at.0 < anchor.0, at.1 > anchor.1),
        Handle::SE => (at.0 < anchor.0, at.1 < anchor.1),
        Handle::SW => (at.0 > anchor.0, at.1 < anchor.1),
        Handle::NW => (at.0 > anchor.0, at.1 > anchor.1),
        Handle::E => (at.0 < anchor.0, false),
        Handle::W => (at.0 > anchor.0, false),
        Handle::N => (false, at.1 > anchor.1),
        Handle::S => (false, at.1 < anchor.1),
        Handle::Rotation => (false, false),
    }
}

/// Where the top left of an Element of `previous` size at `previous` goes
/// when it is resized to `next` by `handle`, which keeps the ratio of the
/// sides if `aspect` does (Excalidraw's `getResizedOrigin`).
#[allow(clippy::too_many_arguments)]
fn resized_origin(
    previous: Point,
    previous_width: f64,
    previous_height: f64,
    next_width: f64,
    next_height: f64,
    angle: f64,
    handle: Handle,
    aspect: bool,
) -> Point {
    /// The part of the Element that stays where it is.
    enum Anchor {
        TopLeft,
        TopRight,
        BottomLeft,
        BottomRight,
        West,
        East,
        North,
        South,
    }
    let anchor = match handle {
        Handle::N if aspect => Anchor::South,
        Handle::E if aspect => Anchor::West,
        Handle::S if aspect => Anchor::North,
        Handle::W if aspect => Anchor::East,
        Handle::NE => Anchor::BottomLeft,
        Handle::NW if aspect => Anchor::BottomRight,
        Handle::SE => Anchor::TopLeft,
        Handle::SW => Anchor::TopRight,
        Handle::N | Handle::NW | Handle::W => Anchor::BottomRight,
        _ => Anchor::TopLeft,
    };
    let (sin, cos) = angle.sin_cos();
    let (dw, dh) = (
        (previous_width - next_width) / 2.0,
        (previous_height - next_height) / 2.0,
    );
    match anchor {
        Anchor::TopLeft => (
            previous.0 + dw - dw * cos - dh * sin,
            previous.1 + dh - dw * sin - dh * cos,
        ),
        Anchor::TopRight => (
            previous.0 + dw * (cos + 1.0) + dh * sin,
            previous.1 + dh - dw * sin - dh * cos,
        ),
        Anchor::BottomLeft => (
            previous.0 + dw * (1.0 - cos) - dh * sin,
            previous.1 + dh * (cos + 1.0) - dw * sin,
        ),
        Anchor::BottomRight => (
            previous.0 + dw * (cos + 1.0) - dh * sin,
            previous.1 + dh * (cos + 1.0) - dw * sin,
        ),
        // The west side of the Element stays: the middle of the side the
        // handle is on, mirrored about in its own frame.
        Anchor::West => (
            previous.0 + dw * (1.0 - cos),
            previous.1 - dw * sin + (previous_height - next_height) / 2.0,
        ),
        Anchor::East => (
            previous.0 + dw * (cos + 1.0),
            previous.1 + dw * sin + (previous_height - next_height) / 2.0,
        ),
        Anchor::North => (
            previous.0 + dw + dh * sin,
            previous.1 + (next_height - previous_height) / 2.0 * (cos - 1.0),
        ),
        Anchor::South => (
            previous.0 + dw - dh * sin,
            previous.1 + (previous_height - next_height) / 2.0 * (cos + 1.0),
        ),
    }
}

/// Scales the points to `width` by `height`, mirroring them when a size is
/// negative; with `normalize` the least coordinate of each dimension stays
/// (Excalidraw's `rescalePoints`).
fn rescale_points(points: &[Point], width: f64, height: f64, normalize: bool) -> Vec<Point> {
    rescale_dimension(
        rescale_dimension(points.to_vec(), 0, width, normalize),
        1,
        height,
        normalize,
    )
}

/// Scales the dimension `0` (x) or `1` (y) of `points` to `new_size`.
fn rescale_dimension(
    points: Vec<Point>,
    dimension: usize,
    new_size: f64,
    normalize: bool,
) -> Vec<Point> {
    let coordinate = |&(x, y): &Point| if dimension == 0 { x } else { y };
    let least = points.iter().map(coordinate).fold(f64::INFINITY, f64::min);
    if !least.is_finite() {
        return points;
    }
    let greatest = points
        .iter()
        .map(coordinate)
        .fold(f64::NEG_INFINITY, f64::max);
    let scale = if greatest == least {
        1.0
    } else {
        new_size / (greatest - least)
    };
    let mut scaled = points
        .into_iter()
        .map(|(x, y)| {
            if dimension == 0 {
                (x * scale, y)
            } else {
                (x, y * scale)
            }
        })
        .collect::<Vec<_>>();
    if !normalize || scaled.len() == 2 {
        return scaled;
    }
    // The least coordinate stays where it was.
    let next_least = scaled.iter().map(coordinate).fold(f64::INFINITY, f64::min);
    let shift = least - next_least;
    for point in &mut scaled {
        if dimension == 0 {
            point.0 += shift;
        } else {
            point.1 += shift;
        }
    }
    scaled
}

/// The points as they are saved.
fn points_value(points: &[Point]) -> Value {
    Value::Array(
        points
            .iter()
            .map(|&(x, y)| Value::Array(vec![Value::Number(x), Value::Number(y)]))
            .collect(),
    )
}
