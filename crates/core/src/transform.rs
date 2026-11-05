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
//! While a pointer resizes, Shift (on a keyboard) keeps the ratio of the
//! sides; a Selection with an image keeps it anyway and Shift frees it. Alt
//! resizes about the middle of the box instead of the opposite corner or
//! side. While a pointer rotates, Shift snaps the rotation to steps of 15°.
//!
//! Arrows bound to the shapes that are transformed follow them, and arrows
//! that are transformed without the shape they are bound to are unbound from
//! it (see [`crate::binding`]).
//!
//! A line or arrow alone has no transform handles: it is edited at its
//! points instead (see [`crate::edit_points`]). An arrow routed at right angles is
//! transformed like any other line here, points and all, where Excalidraw
//! re-routes it; the core keeps its points as the file has them.

use std::collections::HashMap;

use crate::binding::Attachments;
use crate::frame::Frame;
use crate::history::Step;
use crate::input::Modifiers;
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
/// (Excalidraw's `originalElements`), with its label. Resizing follows the
/// pointer from the Elements as they began, not as the last step left them,
/// so dragging back undoes the steps before.
#[derive(Debug)]
struct Tracked {
    handle: u32,
    snapshot: Object,
    label: Option<Label>,
}

impl Tracked {
    /// The Element as it began.
    fn element(&self) -> Option<Element<'_>> {
        Element::view(self.handle, &self.snapshot)
    }

    /// The label as it began.
    fn label(&self) -> Option<Element<'_>> {
        self.label.as_ref()?.element()
    }

    /// Puts the label, if there is one, where it sits in the Element as it
    /// is now; see [`Label::refit`].
    fn refit_label(
        &self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        font: Option<f64>,
        angle: Option<f64>,
        wrap: Option<&dyn TextMeasurer>,
    ) {
        if let Some(label) = &self.label {
            label.refit(scene, entropy, self.handle, font, angle, wrap);
        }
    }
}

/// A label that follows the container it sits in, as it was when the
/// gesture began.
#[derive(Debug)]
struct Label {
    handle: u32,
    snapshot: Object,
}

impl Label {
    fn element(&self) -> Option<Element<'_>> {
        Element::view(self.handle, &self.snapshot)
    }

    /// Puts the label where it sits in the `container` as it is now. With
    /// `font` the label's font becomes it first; with `angle` the label
    /// turns with its container, which lines and arrows except; with `wrap`
    /// its lines wrap to the container again. An arrow's label keeps where
    /// the file has it.
    fn refit(
        &self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        container: u32,
        font: Option<f64>,
        angle: Option<f64>,
        wrap: Option<&dyn TextMeasurer>,
    ) {
        if let Some(font) = font {
            scene.update(
                self.handle,
                entropy.integer(),
                [("fontSize", Value::Number(font))],
            );
        }
        let (fit, kind) = {
            let (Some(label), Some(container)) =
                (scene.element(self.handle), scene.element(container))
            else {
                return;
            };
            (text::fit_label(&label, &container, wrap), container.kind())
        };
        let mut members = Vec::new();
        if kind != ElementType::Arrow {
            members.push(("x", Value::Number(fit.x)));
            members.push(("y", Value::Number(fit.y)));
        }
        members.push(("width", Value::Number(fit.width)));
        members.push(("height", Value::Number(fit.height)));
        if let Some(text) = fit.text {
            members.push(("text", Value::String(text)));
        }
        if let (Some(angle), false) = (angle, kind.is_linear()) {
            members.push(("angle", Value::Number(angle)));
        }
        scene.update(self.handle, entropy.integer(), members);
    }
}

/// How the pointer resizes: by which handle, and what the keys held ask
/// for.
#[derive(Debug, Clone, Copy)]
struct Resize {
    handle: Handle,
    /// The sides keep their ratio (Shift; images the other way round).
    keep_ratio: bool,
    /// The middle of the box stays and the side opposite the handle moves
    /// as far the other way (Alt).
    from_centre: bool,
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
    /// The pointer and keys of the last step, to tell a pointer that moved
    /// from one that is where it was.
    applied: Option<(Point, Modifiers)>,
    /// The turn of the last rotation, to tell one that changed the angle
    /// from one that snapped to the same.
    turned: Option<f64>,
    attachments: Attachments,
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
        let targets: Vec<Element> = crate::transform_handles::targets(scene, selection).collect();
        let index: HashMap<&str, usize> = targets
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id(), i))
            .collect();
        let mut labels: Vec<Option<Label>> = targets.iter().map(|_| None).collect();
        for element in scene.elements().filter(|e| !e.is_deleted()) {
            let Some(&i) = element.container_id().and_then(|id| index.get(id)) else {
                continue;
            };
            if let Some(snapshot) = scene.snapshot(element.id()) {
                step.touch(scene, element.id());
                labels[i] = Some(Label {
                    handle: element.handle(),
                    snapshot,
                });
            }
        }
        let mut tracked = Vec::with_capacity(targets.len());
        let mut ids = Vec::with_capacity(targets.len());
        for (element, label) in targets.iter().zip(labels) {
            if let Some(snapshot) = scene.snapshot(element.id()) {
                step.touch(scene, element.id());
                ids.push(element.id().to_owned());
                tracked.push(Tracked {
                    handle: element.handle(),
                    snapshot,
                    label,
                });
            }
        }
        Self {
            pointer,
            handle,
            offset: transform_box.grab_offset(handle, origin),
            transform_box,
            tracked,
            applied: None,
            turned: None,
            attachments: Attachments::begin(&mut step, scene, ids),
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

    /// Follows the pointer to `to`. Shift keeps the ratio of the sides or
    /// snaps the rotation; Alt resizes about the middle of the box. Returns
    /// whether anything changed.
    pub fn drag_to(
        &mut self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        to: Point,
        keys: Modifiers,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let at = (to.0 - self.offset.0, to.1 - self.offset.1);
        if self.applied == Some((at, keys)) {
            return false;
        }
        self.applied = Some((at, keys));
        let changed = if self.handle == Handle::Rotation {
            self.rotate(scene, entropy, at, keys.shift)
        } else {
            let resize = Resize {
                handle: self.handle,
                keep_ratio: self.keeps_ratio(keys.shift),
                from_centre: keys.alt,
            };
            match self.tracked.as_slice() {
                [single] => resize_single(single, scene, entropy, at, resize, measurer),
                _ => self.resize_many(scene, entropy, at, resize, measurer),
            }
        };
        if changed {
            self.attachments.update(scene, entropy, &mut self.step);
        }
        changed
    }

    /// Whether the Selection keeps the ratio of its sides: Selections with an
    /// image do of themselves and Shift frees them; any other needs Shift.
    fn keeps_ratio(&self, shift: bool) -> bool {
        let images = self
            .tracked
            .iter()
            .any(|t| t.element().is_some_and(|e| e.kind() == ElementType::Image));
        images != shift
    }

    // --- Rotation -----------------------------------------------------------

    /// Turns the Selection about the middle of its box so that it points
    /// where the pointer does from there: snapped to steps of 15° if `shift`
    /// is held. One Element keeps where it is and takes the angle;
    /// several each keep their own angle on top of the turn, and move with
    /// their middles. Returns whether the turn changed.
    fn rotate(&mut self, scene: &mut Scene, entropy: &mut Entropy, at: Point, shift: bool) -> bool {
        let centre = self.transform_box.frame().centre();
        let mut turn = TWO_PI * 1.25 + (at.1 - centre.1).atan2(at.0 - centre.0);
        if shift {
            turn += SHIFT_LOCKING_ANGLE / 2.0;
            turn -= turn % SHIFT_LOCKING_ANGLE;
        }
        let turn = normalized(turn);
        if self.turned.replace(turn) == Some(turn) {
            return false;
        }
        if let [single] = self.tracked.as_slice() {
            scene.update(
                single.handle,
                entropy.integer(),
                [("angle", Value::Number(turn))],
            );
            single.refit_label(scene, entropy, None, Some(turn), None);
            return true;
        }
        for tracked in &self.tracked {
            let Some(orig) = tracked.element() else {
                continue;
            };
            let middle = Frame::of(&orig).centre();
            let (x, y) = turned(middle, centre, turn);
            let angle = normalized(orig.angle() + turn);
            let members = [
                ("x", Value::Number(orig.x() + x - middle.0)),
                ("y", Value::Number(orig.y() + y - middle.1)),
                ("angle", Value::Number(angle)),
            ];
            scene.update(tracked.handle, entropy.integer(), members);
            tracked.refit_label(scene, entropy, None, Some(angle), None);
        }
        true
    }

    // --- Resizing several Elements -----------------------------------------

    /// Scales every Element of the Selection as its box does: each from the
    /// corner or side opposite the handle (or the middle, from the centre),
    /// mirroring when the pointer crosses it (Excalidraw's
    /// `resizeMultipleElements`). Nothing changes if a font would get
    /// smaller than the least.
    fn resize_many(
        &self,
        scene: &mut Scene,
        entropy: &mut Entropy,
        at: Point,
        resize: Resize,
        measurer: Option<&dyn TextMeasurer>,
    ) -> bool {
        let Some(bounds) = self.common_bounds() else {
            return false;
        };
        let (width, height) = (bounds.width(), bounds.height());
        if width == 0.0 || height == 0.0 {
            return false;
        }
        let handle = resize.handle;
        let (anchor, factor) = if resize.from_centre {
            (bounds.centre(), 2.0)
        } else {
            (anchor_of(handle, bounds), 1.0)
        };
        let (flip_x, flip_y) = flips_of(handle, at, anchor);
        let (reach_x, reach_y) = (
            (at.0 - anchor.0).abs() * factor,
            (at.1 - anchor.1).abs() * factor,
        );
        let across = handle.is_east() || handle.is_west();
        let down = handle.is_north() || handle.is_south();
        // Without the ratio kept, an axis the handle is not on keeps its
        // size; with it, both take the scale of the pointer's farther reach.
        let (next_width, next_height) = if resize.keep_ratio {
            let scale = (reach_x / width).max(reach_y / height);
            (width * scale, height * scale)
        } else {
            (
                if across { reach_x } else { width },
                if down { reach_y } else { height },
            )
        };
        if next_width == 0.0 || next_height == 0.0 {
            return false;
        }
        let scale_x = if across { next_width / width } else { 1.0 };
        let scale_y = if down { next_height / height } else { 1.0 };
        let scale = if handle.is_corner() {
            (next_width / width).max(next_height / height)
        } else if across {
            scale_x
        } else {
            scale_y
        };
        // Turned Elements, texts and groups keep the ratio of their sides,
        // so that angles, fonts and groups do not skew.
        let keeps = resize.keep_ratio
            || self.tracked.iter().any(|t| {
                t.element().is_some_and(|e| {
                    e.angle() != 0.0
                        || e.kind() == ElementType::Text
                        || e.group_ids().next().is_some()
                })
            });
        let (scale_x, scale_y) = if keeps {
            (scale, scale)
        } else {
            (scale_x, scale_y)
        };

        // Worked out in full before anything changes: a font that would get
        // too small leaves everything as it is.
        let mut updates = Vec::with_capacity(self.tracked.len());
        for tracked in &self.tracked {
            let Some(orig) = tracked.element() else {
                continue;
            };
            let (scaled_width, scaled_height) = (orig.width() * scale_x, orig.height() * scale_y);
            let angle = normalized(orig.angle() * flip_sign(flip_x) * flip_sign(flip_y));
            let points = made_of_points(orig.kind());
            // Mirroring a made-of-points Element mirrors its points, so its
            // position does not shift over; any other does.
            let shift_x = if flip_x && !points { scaled_width } else { 0.0 };
            let shift_y = if flip_y && !points {
                scaled_height
            } else {
                0.0
            };
            let x = anchor.0 + flip_sign(flip_x) * ((orig.x() - anchor.0) * scale_x + shift_x);
            let y = anchor.1 + flip_sign(flip_y) * ((orig.y() - anchor.1) * scale_y + shift_y);
            let mut members = vec![
                ("x", Value::Number(x)),
                ("y", Value::Number(y)),
                ("width", Value::Number(scaled_width)),
                ("height", Value::Number(scaled_height)),
                ("angle", Value::Number(angle)),
            ];
            if points {
                let width = scaled_width * flip_sign(flip_x);
                let height = scaled_height * flip_sign(flip_y);
                let rescaled = rescale_points(&orig.points(), width, height, false);
                members.push(("points", points_value(&rescaled)));
            }
            if orig.kind() == ElementType::Text {
                let font = orig.font_size() * scale_x;
                if font < MIN_FONT_SIZE {
                    return false;
                }
                members.push(("fontSize", Value::Number(font)));
            }
            if orig.kind() == ElementType::Image {
                let (image_x, image_y) = orig.image_scale();
                let mirrored = [image_x * flip_sign(flip_x), image_y * flip_sign(flip_y)];
                members.push(("scale", Value::Array(mirrored.map(Value::Number).to_vec())));
            }
            let label_font = match tracked.label() {
                Some(label) if keeps => Some(label.font_size() * scale),
                Some(label) => Some(label.font_size()),
                None => None,
            };
            if label_font.is_some_and(|font| font < MIN_FONT_SIZE) {
                return false;
            }
            updates.push((tracked, members, label_font, angle));
        }
        for (tracked, members, label_font, angle) in updates {
            scene.update(tracked.handle, entropy.integer(), members);
            tracked.refit_label(scene, entropy, label_font, Some(angle), measurer);
        }
        true
    }

    /// The box around everything the gesture transforms, as it began, with
    /// the labels of lines and arrows included (Excalidraw's
    /// `getCommonBoundingBox`).
    fn common_bounds(&self) -> Option<Frame> {
        let mut frames = Vec::with_capacity(self.tracked.len());
        for tracked in &self.tracked {
            let orig = tracked.element()?;
            frames.push(Frame::shown(&orig));
            if let Some(label) = tracked.label().filter(|_| orig.kind().is_linear()) {
                frames.push(Frame::shown(&label));
            }
        }
        frames.into_iter().reduce(Frame::union)
    }
}

// --- Resizing one Element ---------------------------------------------------

/// Scales the one Element of the Selection from the pointer: the sides the
/// handle is on follow it, and the opposite corner or side stays (or, from
/// the centre, the middle). Turned Elements are resized in their own,
/// unturned frame and keep their angle (Excalidraw's
/// `getNextSingleWidthAndHeightFromPointer` and `resizeSingleElement`).
fn resize_single(
    tracked: &Tracked,
    scene: &mut Scene,
    entropy: &mut Entropy,
    at: Point,
    resize: Resize,
    measurer: Option<&dyn TextMeasurer>,
) -> bool {
    let Some(orig) = tracked.element() else {
        return false;
    };
    let handle = resize.handle;
    // The box the Element is drawn in: for a freedraw stroke that of its
    // points, which may reach left of or above its first one.
    let bounds = Frame::of(&orig);
    let (x, y) = unturned(at, bounds.centre(), orig.angle());
    let mut next_width = if handle.is_east() {
        x - bounds.min.0
    } else if handle.is_west() {
        bounds.max.0 - x
    } else {
        orig.width()
    };
    let mut next_height = if handle.is_south() {
        y - bounds.min.1
    } else if handle.is_north() {
        bounds.max.1 - y
    } else {
        orig.height()
    };
    if resize.from_centre {
        next_width = 2.0 * next_width - orig.width();
        next_height = 2.0 * next_height - orig.height();
    }
    // Keeping the ratio of the sides: from a corner both sides take the
    // scale of the longer one; from a side the other side follows the one
    // that is dragged.
    if resize.keep_ratio {
        let width_ratio = next_width.abs() / orig.width();
        let height_ratio = next_height.abs() / orig.height();
        if handle.is_corner() {
            let ratio = width_ratio.max(height_ratio);
            next_width = orig.width() * ratio * next_width.signum();
            next_height = orig.height() * ratio * next_height.signum();
        } else {
            next_height *= width_ratio;
            next_width *= height_ratio;
        }
    }
    if orig.kind() == ElementType::Text {
        return resize_text(
            scene,
            entropy,
            &orig,
            (next_width, next_height),
            resize,
            measurer,
        );
    }

    // The least size and the label's font use sizes as they are; the signs
    // are kept for the geometry below, where crossing the opposite edge
    // mirrors the Element.
    let (flip_x, flip_y) = (next_width < 0.0, next_height < 0.0);
    next_width = next_width.abs();
    next_height = next_height.abs();
    // Only the handles in the middle of the top and bottom sides leave the
    // width, which a label's lines wrap to, as it is (`handleBindTextResize`).
    let rewrap = resize.keep_ratio || !matches!(handle, Handle::N | Handle::S);
    let wrap = measurer.filter(|_| rewrap);
    let mut label_font = None;
    if let Some(label) = tracked.label() {
        let font = if resize.keep_ratio {
            let font = label.font_size()
                * (text::label_max_width(orig.kind(), next_width)
                    / text::label_max_width(orig.kind(), orig.width()));
            if font < MIN_FONT_SIZE {
                return false;
            }
            font
        } else {
            // The label's font is what it was when the gesture began; only
            // the lines rewrap.
            if let Some(measurer) = measurer {
                let (min_width, min_height) = text::min_container_size(&label, measurer);
                next_width = next_width.max(min_width);
                next_height = next_height.max(min_height);
            }
            label.font_size()
        };
        // Never lower than its lines: whichever side the pointer pulls, the
        // shape is as high as its label needs, from the side the anchor
        // keeps (`handleBindTextResize`).
        if !orig.kind().is_linear() {
            next_height =
                text::height_to_hold(&label, orig.kind(), (next_width, next_height), font, wrap);
        }
        label_font = Some(font);
    }
    let signed_width = next_width * flip_sign(flip_x);
    let signed_height = next_height * flip_sign(flip_y);
    if signed_width == 0.0 || signed_height == 0.0 {
        return false;
    }
    // A line or arrow alone has no handles (see `TransformBox::of`), so of
    // the Elements made of points only a freedraw stroke resizes alone: its
    // points keep their least coordinates, and so its first point is where
    // the box's corner goes, as for any other Element.
    let mut origin = resized_origin(
        (orig.x(), orig.y()),
        (orig.width(), orig.height()),
        (signed_width, signed_height),
        orig.angle(),
        Anchor::of(handle, resize.keep_ratio, resize.from_centre),
    );
    // Mirroring puts the origin on the far side of the Element.
    if flip_x {
        origin.0 += signed_width;
    }
    if flip_y {
        origin.1 += signed_height;
    }

    let mut members = vec![
        ("x", Value::Number(origin.0)),
        ("y", Value::Number(origin.1)),
        ("width", Value::Number(next_width)),
        ("height", Value::Number(next_height)),
    ];
    if made_of_points(orig.kind()) {
        let rescaled = rescale_points(&orig.points(), signed_width, signed_height, true);
        members.push(("points", points_value(&rescaled)));
    }
    if orig.kind() == ElementType::Image {
        let (image_x, image_y) = orig.image_scale();
        let mirrored = [image_x * flip_sign(flip_x), image_y * flip_sign(flip_y)];
        members.push(("scale", Value::Array(mirrored.map(Value::Number).to_vec())));
    }
    scene.update(orig.handle(), entropy.integer(), members);
    // The label's font goes on before the lines wrap to the container it
    // sits in now.
    tracked.refit_label(scene, entropy, label_font, None, wrap);
    true
}

/// Lays a text out again at the size `next` the pointer asks for: from a
/// corner, the top or the bottom its font scales with its height (a corner
/// keeping the ratio of the sides, as keeping it does); from the left or
/// right its lines wrap again to the width (Excalidraw's
/// `resizeSingleTextElement`).
fn resize_text(
    scene: &mut Scene,
    entropy: &mut Entropy,
    orig: &Element,
    (next_width, next_height): (f64, f64),
    resize: Resize,
    measurer: Option<&dyn TextMeasurer>,
) -> bool {
    let Some(measurer) = measurer else {
        return false;
    };
    // Texts ignore Shift: they keep their ratio from every handle but the
    // left and right ones, which rewrap instead.
    let anchor = Anchor::of(resize.handle, false, resize.from_centre);
    let previous = ((orig.x(), orig.y()), (orig.width(), orig.height()));
    let handle = resize.handle;
    if handle.is_north() || handle.is_south() {
        let Some((shown_width, shown_height, shown_font)) = scene
            .element(orig.handle())
            .map(|e| (e.width(), e.height(), e.font_size()))
        else {
            return false;
        };
        let next_height = if handle.is_corner() {
            next_height.max(next_width / orig.width() * orig.height())
        } else {
            next_height
        };
        let width = shown_width * (next_height / shown_height);
        let font = shown_font * (next_height / shown_height);
        if font < MIN_FONT_SIZE {
            return false;
        }
        let origin = resized_origin(
            previous.0,
            previous.1,
            (width, next_height),
            orig.angle(),
            anchor,
        );
        scene.update(
            orig.handle(),
            entropy.integer(),
            [
                ("x", Value::Number(origin.0)),
                ("y", Value::Number(origin.1)),
                ("fontSize", Value::Number(font)),
                ("width", Value::Number(width)),
                ("height", Value::Number(next_height)),
            ],
        );
        return true;
    }
    let lines = {
        let Some(current) = scene.element(orig.handle()) else {
            return false;
        };
        let width = next_width.max(text::min_text_width(current.font_size(), measurer));
        (width, text::set_in_lines(&current, width.abs(), measurer))
    };
    let (width, lines) = lines;
    let origin = resized_origin(
        previous.0,
        previous.1,
        (width, lines.height),
        orig.angle(),
        anchor,
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

/// Whether a kind of Element is made of points.
fn made_of_points(kind: ElementType) -> bool {
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

/// The part of an Element that stays where it is while it is resized
/// (Excalidraw's `getResizeAnchor`).
#[derive(Debug, Clone, Copy)]
enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    West,
    East,
    North,
    South,
    Centre,
}

impl Anchor {
    /// The anchor of a resize by `handle`: the opposite corner, or, keeping
    /// the ratio of the sides, the opposite side for a side handle, or the
    /// middle from the centre.
    fn of(handle: Handle, keep_ratio: bool, from_centre: bool) -> Self {
        match handle {
            _ if from_centre => Anchor::Centre,
            Handle::N if keep_ratio => Anchor::South,
            Handle::E if keep_ratio => Anchor::West,
            Handle::S if keep_ratio => Anchor::North,
            Handle::W if keep_ratio => Anchor::East,
            Handle::NE => Anchor::BottomLeft,
            Handle::SW => Anchor::TopRight,
            Handle::N | Handle::NW | Handle::W => Anchor::BottomRight,
            _ => Anchor::TopLeft,
        }
    }
}

/// Where the top left of an Element at `previous`, of `previous_size` and
/// turned by `angle`, goes when it is resized to `next_size` so that its
/// `anchor` stays where it is on the screen (Excalidraw's
/// `getResizedOrigin`). The Element turns about its middle, so a corner
/// stays only if the middle moves by the turned half of the change.
fn resized_origin(
    previous: Point,
    (previous_width, previous_height): (f64, f64),
    (next_width, next_height): (f64, f64),
    angle: f64,
    anchor: Anchor,
) -> Point {
    // Where the anchor is from the middle, in halves of the change of size
    // (in the Element's own frame): the middle moves by that much, turned.
    let (along_x, along_y) = match anchor {
        Anchor::TopLeft => (-1.0, -1.0),
        Anchor::TopRight => (1.0, -1.0),
        Anchor::BottomLeft => (-1.0, 1.0),
        Anchor::BottomRight => (1.0, 1.0),
        Anchor::West => (-1.0, 0.0),
        Anchor::East => (1.0, 0.0),
        Anchor::North => (0.0, -1.0),
        Anchor::South => (0.0, 1.0),
        Anchor::Centre => (0.0, 0.0),
    };
    let (dw, dh) = (
        (previous_width - next_width) / 2.0,
        (previous_height - next_height) / 2.0,
    );
    let (sin, cos) = angle.sin_cos();
    let (mx, my) = (along_x * dw, along_y * dh);
    (
        previous.0 + dw + mx * cos - my * sin,
        previous.1 + dh + mx * sin + my * cos,
    )
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
