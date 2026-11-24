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

use std::cell::OnceCell;
use std::collections::HashMap;

use crate::binding::Attachments;
use crate::frame::Frame;
use crate::history::Step;
use crate::input::Modifiers;
use crate::path::Point;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use crate::text::{self, TextMeasurer};
use crate::transform_handles::{Handle, TransformBox};
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;

/// Excalidraw's `SHIFT_LOCKING_ANGLE`: the angle a snapped rotation is a
/// multiple of.
pub(crate) const SHIFT_LOCKING_ANGLE: f64 = std::f64::consts::PI / 12.0;
/// Excalidraw's `MIN_FONT_SIZE`: a resize that would make a font smaller
/// than this does not happen.
const MIN_FONT_SIZE: f64 = 1.0;
const TWO_PI: f64 = std::f64::consts::TAU;

/// The members a transform may change of an Element or its label (the only
/// ones its undo step keeps).
const TRANSFORMED: [&str; 10] = [
    "x",
    "y",
    "width",
    "height",
    "angle",
    "points",
    "fontSize",
    "scale",
    "text",
    "autoResize",
];

/// The members that rotating may change of an Element or its label: the
/// points and the font stay as they are, which spares copying them.
const ROTATED: [&str; 6] = ["x", "y", "width", "height", "angle", "text"];

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
    /// What is the same at every step of a resize, worked out from `tracked`
    /// at the first: the Elements as they began do not change, and there may
    /// be thousands.
    resizing: OnceCell<Resizing>,
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
        let changes: &[&'static str] = if handle == Handle::Rotation {
            &ROTATED
        } else {
            &TRANSFORMED
        };
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
                step.touch_members(scene, element.id(), changes);
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
                step.touch_members(scene, element.id(), changes);
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
            resizing: OnceCell::new(),
            attachments: Attachments::begin(&mut step, scene, ids),
            step,
        }
    }

    /// The handles of the Elements being transformed.
    pub fn handles(&self) -> Vec<u32> {
        self.tracked.iter().map(|t| t.handle).collect()
    }

    /// The pointer that transforms the Selection.
    pub fn pointer(&self) -> u64 {
        self.pointer
    }

    /// The handle the pointer holds.
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Whether the handle resizes, rather than rotates.
    pub fn is_resize(&self) -> bool {
        self.handle != Handle::Rotation
    }

    /// How the box was turned when the gesture began.
    pub fn angle(&self) -> f64 {
        self.transform_box.angle()
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

    fn resizing(&self) -> &Resizing {
        self.resizing.get_or_init(|| Resizing::of(&self.tracked))
    }

    /// Whether the Selection keeps the ratio of its sides: Selections with an
    /// image do of themselves and Shift frees them; any other needs Shift.
    fn keeps_ratio(&self, shift: bool) -> bool {
        self.resizing().has_image != shift
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
        let Some(bounds) = self.resizing().common_bounds else {
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
        let keeps = resize.keep_ratio || self.resizing().keeps_ratio_anyway;
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
}

/// What a resize works out once from the Elements as they began.
#[derive(Debug)]
struct Resizing {
    /// The box around everything.
    common_bounds: Option<Frame>,
    /// Whether the Selection keeps the ratio of its sides whatever the keys
    /// held: turned Elements, texts and groups do, so that angles, fonts and
    /// groups do not skew.
    keeps_ratio_anyway: bool,
    /// Whether an image is in the Selection.
    has_image: bool,
}

impl Resizing {
    fn of(tracked: &[Tracked]) -> Self {
        Self {
            common_bounds: common_bounds(tracked),
            keeps_ratio_anyway: tracked.iter().any(|t| {
                t.element().is_some_and(|e| {
                    e.angle() != 0.0
                        || e.kind() == ElementType::Text
                        || e.group_ids().next().is_some()
                })
            }),
            has_image: tracked
                .iter()
                .any(|t| t.element().is_some_and(|e| e.kind() == ElementType::Image)),
        }
    }
}

/// The box around everything the gesture transforms, as it began, with the
/// labels of lines and arrows included (Excalidraw's `getCommonBoundingBox`).
fn common_bounds(tracked: &[Tracked]) -> Option<Frame> {
    let mut frames = Vec::with_capacity(tracked.len());
    for tracked in tracked {
        let orig = tracked.element()?;
        frames.push(Frame::shown(&orig));
        if let Some(label) = tracked.label().filter(|_| orig.kind().is_linear()) {
            frames.push(Frame::shown(&label));
        }
    }
    frames.into_iter().reduce(Frame::union)
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

#[cfg(test)]
mod tests {
    //! Resizing and rotating the Selection by its transform handles (ADR-0006),
    //! tested at the Editor seam with recorded pointer sequences (see `select.rs`
    //! for their format): pointer events in, saved Drawing and overlay out.
    //! Expected values are worked out by hand in the comments, from Excalidraw's
    //! rules for resizing and rotating (`resizeElements.ts`).

    use crate::{Editor, PartKind};
    use pkm_base::json::{Value, parse};

    use crate::test_support as common;
    use common::{FixedWidth, editor_with, replay, tap};

    /// A filled rectangle at (100, 100), 100 x 50. Its selection frame is 4
    /// units out, so its handles are centred on (96, 96), (204, 96), (204, 154)
    /// and (96, 154), and the rotation handle 16 above the middle of the top
    /// side, at (150, 80).
    const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    /// A member of the Element `id` as saved.
    fn saved(editor: &Editor, id: &str, key: &str) -> f64 {
        let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
        let element = drawing
            .get("elements")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap();
        element.get(key).and_then(Value::as_f64).unwrap()
    }

    /// `(x, y, width, height)` of the Element `id`.
    fn frame(editor: &Editor, id: &str) -> (f64, f64, f64, f64) {
        (
            saved(editor, id, "x"),
            saved(editor, id, "y"),
            saved(editor, id, "width"),
            saved(editor, id, "height"),
        )
    }

    /// Presses at `from`, drags to `to` and lifts there.
    fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        replay(
            editor,
            &format!(
                "1.0 down 1 {kind} {} {}
             1.1 move 1 {kind} {} {}
             1.2 move 1 {kind} {} {}
             1.3 up 1 {kind} {} {}",
                from.0, from.1, mid.0, mid.1, to.0, to.1, to.0, to.1
            ),
        );
    }

    /// An editor with the rectangle selected.
    fn selected_rectangle() -> Editor {
        let mut editor = editor_with(&[RECTANGLE]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        assert_eq!(editor.selection(), ["a"]);
        editor
    }

    #[test]
    fn dragging_the_south_east_handle_resizes_the_selected_element() {
        let mut editor = selected_rectangle();

        // The pointer takes hold 4 units beyond the corner and keeps that
        // distance, so the corner ends at (250, 180).
        drag(&mut editor, "mouse", (204.0, 154.0), (254.0, 184.0));

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 80.0));
        assert_eq!(editor.selection(), ["a"]);
    }

    /// Presses at `from`, drags to `to` and lifts there, with `shift` held.
    fn held_drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64), shift: bool) {
        let hold = if shift { " shift" } else { "" };
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        replay(
            editor,
            &format!(
                "2.0 down 1 {kind} {} {}{hold}
             2.1 move 1 {kind} {} {}{hold}
             2.2 move 1 {kind} {} {}{hold}
             2.3 up 1 {kind} {} {}{hold}",
                from.0, from.1, mid.0, mid.1, to.0, to.1, to.0, to.1
            ),
        );
    }

    /// The centre of each handle of [`RECTANGLE`] for a mouse at zoom 1: just
    /// beyond the selection frame, which keeps 4 units from the box, plus
    /// Excalidraw's 2 of spacing; the rotation handle 16 further above.
    const NE: (f64, f64) = (206.0, 94.0);
    const SE: (f64, f64) = (206.0, 156.0);
    const W: (f64, f64) = (94.0, 125.0);
    const S: (f64, f64) = (150.0, 156.0);
    const E: (f64, f64) = (206.0, 125.0);
    const ROTATION: (f64, f64) = (150.0, 78.0);

    #[test]
    fn dragging_the_north_east_handle_keeps_the_opposite_corner() {
        let mut editor = selected_rectangle();

        // The pointer takes hold 6 beyond the corner and keeps that distance,
        // so the dragged corner ends at (210, 70) and the box grows left and
        // down of it.
        held_drag(&mut editor, "mouse", NE, (216.0, 64.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 70.0, 110.0, 80.0));
    }

    #[test]
    fn dragging_the_west_handle_keeps_the_opposite_side() {
        let mut editor = selected_rectangle();

        held_drag(&mut editor, "mouse", W, (74.0, 145.0), false);

        assert_eq!(frame(&editor, "a"), (80.0, 100.0, 120.0, 50.0));
    }

    #[test]
    fn dragging_a_side_handle_across_the_box_mirrors_the_element() {
        let mut editor = selected_rectangle();

        // The south side is dragged up past the north one: the Element is
        // mirrored, and the dragged side ends where the pointer is.
        held_drag(&mut editor, "mouse", S, (150.0, 46.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 40.0, 100.0, 60.0));
    }

    #[test]
    fn shift_keeps_the_ratio_of_the_sides_from_a_corner() {
        let mut editor = selected_rectangle();

        // The pointer asks for 150 x 60; the wider side wins, so both sides
        // grow by half again.
        held_drag(&mut editor, "mouse", SE, (256.0, 166.0), true);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 75.0));
    }

    #[test]
    fn shift_keeps_the_ratio_of_the_sides_from_a_side() {
        let mut editor = selected_rectangle();

        // The east side is dragged out by half; the height follows it, and the
        // Element stays centred on the side it keeps.
        held_drag(&mut editor, "mouse", E, (256.0, 145.0), true);

        assert_eq!(frame(&editor, "a"), (100.0, 87.5, 150.0, 75.0));
    }

    #[test]
    fn undoing_puts_the_resized_element_back() {
        let mut editor = selected_rectangle();

        held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);
        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));

        editor.undo();
        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn cancelling_a_resize_puts_the_element_back() {
        let mut editor = selected_rectangle();

        replay(
            &mut editor,
            "3.0 down 1 mouse 206 156
         3.1 move 1 mouse 256 206
         3.2 cancel 1 mouse 256 206",
        );

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn a_tap_on_a_handle_leaves_the_selection_alone() {
        let mut editor = selected_rectangle();

        // A press on the rotation handle that never drags is neither a
        // selection nor a clearing of it.
        tap(&mut editor, "mouse", ROTATION, 4.0);

        assert_eq!(editor.selection(), ["a"]);
    }

    /// The `key` of the Element `id` as saved, within a tolerance.
    fn nearly(editor: &Editor, id: &str, key: &str, expected: f64) {
        let actual = saved(editor, id, key);
        assert!(
            (actual - expected).abs() < 1e-9,
            "{key} is {actual}, expected {expected}"
        );
    }

    #[test]
    fn dragging_the_rotation_handle_turns_the_element() {
        let mut editor = selected_rectangle();

        // The pointer ends level with the middle of the box, to its right: a
        // quarter turn clockwise, and the box stays where it is.
        held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

        nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn shift_snaps_the_rotation_to_steps_of_15_degrees() {
        let mut editor = selected_rectangle();

        // The pointer is 140° clockwise from straight above the middle
        // (150, 125); snapped, the turn becomes the nearest step of 15°, 135°.
        let (sin, cos) = 140f64.to_radians().sin_cos();
        let to = (150.0 + 100.0 * sin, 125.0 - 100.0 * cos);
        held_drag(&mut editor, "mouse", ROTATION, to, true);

        nearly(&editor, "a", "angle", 3.0 * std::f64::consts::FRAC_PI_4);
    }

    /// A filled rectangle at (300, 150), 100 x 100.
    const BIGGER_RECTANGLE: &str = r##"{"id":"b","type":"rectangle","x":300,"y":150,"width":100,"height":100,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    /// An editor with both rectangles selected.
    fn selected_rectangles() -> Editor {
        let mut editor = editor_with(&[RECTANGLE, BIGGER_RECTANGLE]);
        editor.select_all();
        assert_eq!(editor.selection(), ["a", "b"]);
        editor
    }

    #[test]
    fn dragging_a_handle_resizes_the_selection_as_a_unit() {
        let mut editor = selected_rectangles();

        // The box of the Selection is 100..400 by 100..250. The pointer asks
        // for 450 x 300, so everything is scaled by 1.5 across and 2 down from
        // the corner opposite the handle.
        held_drag(&mut editor, "mouse", (406.0, 256.0), (556.0, 406.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
        assert_eq!(frame(&editor, "b"), (400.0, 200.0, 150.0, 200.0));
    }

    #[test]
    fn dragging_a_side_handle_stretches_the_selection_across_only() {
        let mut editor = selected_rectangles();

        held_drag(&mut editor, "mouse", (406.0, 175.0), (556.0, 175.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 50.0));
        assert_eq!(frame(&editor, "b"), (400.0, 150.0, 150.0, 100.0));
    }

    #[test]
    fn dragging_a_handle_across_the_selection_mirrors_it() {
        let mut editor = selected_rectangles();

        // The west handle is dragged 600 to the right, past the east side of
        // the box: what was left of it is now right of it, at the same size.
        held_drag(&mut editor, "mouse", (94.0, 175.0), (694.0, 175.0), false);

        assert_eq!(frame(&editor, "a"), (600.0, 100.0, 100.0, 50.0));
        assert_eq!(frame(&editor, "b"), (400.0, 150.0, 100.0, 100.0));
    }

    #[test]
    fn dragging_the_rotation_handle_turns_the_selection_as_a_unit() {
        let mut editor = selected_rectangles();

        // The pointer ends level with the middle of the box, to its right: a
        // quarter turn about the middle of the box, each Element keeping its
        // own angle on top of it.
        held_drag(&mut editor, "mouse", (250.0, 78.0), (450.0, 175.0), false);

        nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
        assert_eq!(frame(&editor, "a"), (250.0, 50.0, 100.0, 50.0));
        nearly(&editor, "b", "angle", std::f64::consts::FRAC_PI_2);
        assert_eq!(frame(&editor, "b"), (175.0, 225.0, 100.0, 100.0));
    }

    // --- Each kind of Element follows its own rules -----------------------------

    /// The points of the Element `id` as saved.
    fn saved_points(editor: &Editor, id: &str) -> Vec<(f64, f64)> {
        let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
        let element = drawing
            .get("elements")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap();
        element
            .get("points")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|p| {
                let p = p.as_array().unwrap();
                (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())
            })
            .collect()
    }

    /// A freedraw stroke from (100, 100) to (200, 150).
    const STROKE: &str = r##"{"id":"a","type":"freedraw","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"pressures":[0.5,0.5],"version":3,"versionNonce":7}"##;

    #[test]
    fn resizing_a_freedraw_stroke_rescales_its_points() {
        let mut editor = editor_with(&[STROKE]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

        held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
        assert_eq!(saved_points(&editor, "a"), [(0.0, 0.0), (150.0, 100.0)]);
    }

    /// A text of one line of `text` in the size `font_size`, at (100, 100),
    /// wide enough that the handles on its sides are offered.
    fn text_element(text: &str, font_size: f64) -> String {
        let chars = text.chars().count() as f64;
        format!(
            r#"{{"id":"a","type":"text","x":100,"y":100,"width":{},"height":{},
        "fontSize":{font_size},"lineHeight":1.25,"text":"{text}","originalText":"{text}","version":3,"versionNonce":7}}"#,
            chars * font_size / 2.0,
            font_size * 1.25
        )
    }

    fn selected_text(text: &str, font_size: f64) -> Editor {
        let mut editor = editor_with(&[&text_element(text, font_size)]);
        editor.set_text_measurer(std::sync::Arc::new(FixedWidth));
        tap(&mut editor, "mouse", (115.0, 112.0), 0.0);
        assert_eq!(editor.selection(), ["a"]);
        editor
    }

    #[test]
    fn resizing_a_text_from_a_corner_scales_its_font() {
        let mut editor = selected_text("abc", 20.0);

        // The pointer asks for 80 x 75; a corner keeps the ratio of the sides,
        // so the text becomes 90 wide and three times as tall and its font
        // three times as big.
        held_drag(&mut editor, "mouse", (136.0, 131.0), (186.0, 181.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 90.0, 75.0));
        nearly(&editor, "a", "fontSize", 60.0);
    }

    #[test]
    fn resizing_a_text_from_a_side_wraps_its_lines_again() {
        let mut editor = selected_text("ab cd", 40.0);

        // The east side is dragged in to 50 from the left edge: the two words
        // no longer fit on one line, and the text takes two lines of the same
        // font.
        held_drag(&mut editor, "mouse", (206.0, 125.0), (156.0, 125.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 50.0, 100.0));
        nearly(&editor, "a", "fontSize", 40.0);
        let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
        let text = drawing.get("elements").and_then(Value::as_array).unwrap()[0]
            .get("text")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned();
        assert_eq!(text, "ab\ncd");
    }

    /// An image at (100, 100), 100 x 50.
    const IMAGE: &str = r##"{"id":"a","type":"image","x":100,"y":100,"width":100,"height":50,
    "fileId":"f","version":3,"versionNonce":7}"##;

    fn selected_image() -> Editor {
        let mut editor = editor_with(&[IMAGE]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        assert_eq!(editor.selection(), ["a"]);
        editor
    }

    #[test]
    fn an_image_keeps_the_ratio_of_its_sides_of_itself() {
        let mut editor = selected_image();

        // The pointer asks for 150 x 60; the image keeps its sides' ratio
        // without Shift, so both grow by half.
        held_drag(&mut editor, "mouse", SE, (256.0, 166.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 75.0));
    }

    #[test]
    fn shift_frees_an_image_to_lose_the_ratio_of_its_sides() {
        let mut editor = selected_image();

        held_drag(&mut editor, "mouse", SE, (256.0, 166.0), true);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 60.0));
    }

    #[test]
    fn mirroring_an_image_mirrors_which_way_it_faces() {
        let mut editor = selected_image();

        // The corner is dragged past the opposite one: the image is mirrored,
        // and its `scale` says so, as excalidraw.com reads it.
        held_drag(&mut editor, "mouse", SE, (46.0, 46.0), true);

        assert_eq!(frame(&editor, "a"), (40.0, 40.0, 60.0, 60.0));
        let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
        let scale = drawing.get("elements").and_then(Value::as_array).unwrap()[0]
            .get("scale")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|f| f.as_f64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(scale, [-1.0, -1.0]);
    }

    /// A rectangle at (100, 100), 100 x 50, with the label `hi` in its middle.
    fn labelled_rectangle() -> Editor {
        let mut editor = editor_with(&[
            r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
            r##"{"id":"t","type":"text","containerId":"a","x":105,"y":112.5,"width":20,"height":25,
            "fontSize":20,"lineHeight":1.25,"textAlign":"center","verticalAlign":"middle",
            "text":"hi","originalText":"hi","version":3,"versionNonce":7}"##,
        ]);
        editor.set_text_measurer(std::sync::Arc::new(FixedWidth));
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        assert_eq!(editor.selection(), ["a"]);
        editor
    }

    #[test]
    fn the_label_of_a_resized_container_sits_in_it_again() {
        let mut editor = labelled_rectangle();

        held_drag(&mut editor, "mouse", SE, (256.0, 206.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
        // The label keeps its font and is centred in the container it sits in,
        // as Excalidraw's own tests expect of a resized container.
        let (width, height) = (saved(&editor, "t", "width"), saved(&editor, "t", "height"));
        assert_eq!((width, height), (20.0, 25.0));
        nearly(&editor, "t", "x", 165.0);
        nearly(&editor, "t", "y", 137.5);
        nearly(&editor, "t", "fontSize", 20.0);
    }

    #[test]
    fn the_label_of_a_turned_container_turns_with_it() {
        let mut editor = labelled_rectangle();

        held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

        nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
        nearly(&editor, "t", "angle", std::f64::consts::FRAC_PI_2);
        // The label stays in the middle of the container.
        nearly(&editor, "t", "x", 140.0);
        nearly(&editor, "t", "y", 112.5);
    }

    /// An arrow from (100, 100) to (200, 150).
    const ARROW: &str = r##"{"id":"a","type":"arrow","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"version":3,"versionNonce":7}"##;

    #[test]
    fn a_line_or_arrow_alone_has_handles_on_its_points_instead_of_transform_handles() {
        let mut editor = editor_with(&[ARROW]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

        // A lone line or arrow is edited at its points (see `edit_points.rs`): a
        // handle on each of its two points and one in the middle of its segment,
        // but no eight to resize and one to rotate.
        let overlay = editor.overlay();
        let handles = overlay
            .parts()
            .iter()
            .filter(|p| p.element == u32::MAX && p.kind == PartKind::Fill as u32)
            .count();
        assert_eq!(handles, 3);
    }

    #[test]
    fn a_finger_takes_hold_of_a_handle_from_farther_away() {
        let mut editor = selected_rectangle();

        // Apple's 44 points of grip for a finger reach 22 beyond the handle's
        // centre; a mouse's 8 points would not take hold here at all.
        held_drag(&mut editor, "finger", (226.0, 176.0), (266.0, 216.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 140.0, 90.0));
    }

    /// The sides of the squares the overlay draws for the handles.
    fn handle_sizes(editor: &Editor) -> Vec<f64> {
        let overlay = editor.overlay();
        overlay
            .parts()
            .iter()
            .filter(|p| p.element == u32::MAX && p.kind == PartKind::Fill as u32)
            .map(|p| {
                let coords: Vec<_> = common::ops(&overlay, p)
                    .iter()
                    .flat_map(|(_, c)| c.iter().copied())
                    .collect();
                let (mut least_x, mut greatest_x) = (f32::INFINITY, f32::NEG_INFINITY);
                let (mut least_y, mut greatest_y) = (f32::INFINITY, f32::NEG_INFINITY);
                for pair in coords.chunks(2) {
                    least_x = least_x.min(pair[0]);
                    greatest_x = greatest_x.max(pair[0]);
                    least_y = least_y.min(pair[1]);
                    greatest_y = greatest_y.max(pair[1]);
                }
                (greatest_x - least_x).max(greatest_y - least_y) as f64
            })
            .collect()
    }

    #[test]
    fn the_handles_are_larger_for_a_finger_than_for_a_mouse() {
        let mut editor = editor_with(&[RECTANGLE]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        let mouse_sizes = handle_sizes(&editor);
        assert!(
            mouse_sizes.iter().all(|size| (size - 8.0).abs() < 1e-3),
            "{mouse_sizes:?}"
        );

        tap(&mut editor, "finger", (150.0, 125.0), 1.0);
        let finger_sizes = handle_sizes(&editor);
        assert!(
            finger_sizes.iter().all(|size| (size - 14.0).abs() < 1e-3),
            "{finger_sizes:?}"
        );
    }

    /// A text of five characters, 100 wide and one line high, at (300, 150).
    const SELECTED_TEXT_IN_MULTIPLE: &str = r##"{"id":"b","type":"text","x":300,"y":150,"width":100,"height":25,
    "fontSize":20,"lineHeight":1.25,"text":"abcde","originalText":"abcde","version":3,"versionNonce":7}"##;

    #[test]
    fn the_selection_of_a_text_keeps_the_ratio_of_the_sides() {
        let mut editor = editor_with(&[RECTANGLE, SELECTED_TEXT_IN_MULTIPLE]);
        editor.select_all();

        // The box of the Selection is 100..400 by 100..175. The pointer asks
        // for 450 x 150; a text in the Selection keeps the ratio of the sides,
        // so both take the taller scale, of 2.
        held_drag(&mut editor, "mouse", (406.0, 181.0), (556.0, 256.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 200.0, 100.0));
        assert_eq!(frame(&editor, "b"), (500.0, 200.0, 200.0, 50.0));
        nearly(&editor, "b", "fontSize", 40.0);
    }

    #[test]
    fn resizing_a_turned_element_follows_the_pointer_in_its_own_frame() {
        // The rectangle of the first test, a quarter turn clockwise about its
        // middle: its south-east handle sits at (119, 181), below and left of
        // its middle on the screen.
        let turned = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "angle":1.5707963267948966,"strokeWidth":2,"backgroundColor":"#ffc9c9",
        "roundness":null,"version":3,"versionNonce":7}"##;
        let mut editor = editor_with(&[turned]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

        // The handle is dragged down the screen, which is across in the
        // Element's own frame: the box grows across by the 30 the pointer
        // travelled, and the Element keeps its turn.
        held_drag(&mut editor, "mouse", (119.0, 181.0), (119.0, 211.0), false);

        assert_eq!(frame(&editor, "a"), (85.0, 115.0, 130.0, 50.0));
        nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn a_diamond_and_an_ellipse_resize_like_a_rectangle() {
        let mut editor = editor_with(&[
            r##"{"id":"a","type":"diamond","x":100,"y":100,"width":100,"height":50,
            "version":3,"versionNonce":7}"##,
            r##"{"id":"b","type":"ellipse","x":300,"y":100,"width":100,"height":50,
            "version":3,"versionNonce":7}"##,
        ]);
        editor.select_all();

        // The box of the Selection is 100..400 by 100..150; the pointer asks
        // for 450 x 150, so everything is scaled by 1.5 across and 3 down.
        held_drag(&mut editor, "mouse", (406.0, 156.0), (556.0, 256.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 150.0));
        assert_eq!(frame(&editor, "b"), (400.0, 100.0, 150.0, 150.0));
    }

    /// An arrow from (100, 100) to (200, 150), selected with the big rectangle.
    const ARROW_IN_MULTIPLE: &str = r##"{"id":"a","type":"arrow","x":100,"y":100,"width":100,"height":50,
    "points":[[0,0],[100,50]],"version":3,"versionNonce":7}"##;

    #[test]
    fn resizing_the_selection_rescales_the_points_of_lines_and_arrows() {
        let mut editor = editor_with(&[ARROW_IN_MULTIPLE, BIGGER_RECTANGLE]);
        editor.select_all();

        // The box of the Selection is 100..400 by 100..250; the pointer asks
        // for 450 x 300. The arrow's points scale with it.
        held_drag(&mut editor, "mouse", (406.0, 256.0), (556.0, 406.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
        assert_eq!(saved_points(&editor, "a"), [(0.0, 0.0), (150.0, 100.0)]);
        assert_eq!(frame(&editor, "b"), (400.0, 200.0, 150.0, 200.0));
    }

    // --- Turned Elements, strokes, groups, the least font, from the centre -------

    /// `offset` from `centre`, turned by `angle` (clockwise on the screen).
    fn turned_from(centre: (f64, f64), angle: f64, offset: (f64, f64)) -> (f64, f64) {
        let (sin, cos) = angle.sin_cos();
        (
            centre.0 + offset.0 * cos - offset.1 * sin,
            centre.1 + offset.0 * sin + offset.1 * cos,
        )
    }

    #[test]
    fn resizing_a_turned_element_from_any_corner_keeps_the_opposite_corner() {
        // The rectangle of the first test, turned by half a radian about its
        // middle (150, 125).
        let angle = 0.5;
        let turned = format!(
            r##"{{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "angle":{angle},"strokeWidth":2,"backgroundColor":"#ffc9c9",
        "roundness":null,"version":3,"versionNonce":7}}"##
        );
        for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let mut editor = editor_with(&[&turned]);
            tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
            let opposite = turned_from((150.0, 125.0), angle, (-sx * 50.0, -sy * 25.0));

            // The handle is 6 beyond the corner in the Element's own frame, and
            // is dragged 15 out across and 10 out down in that frame.
            let handle = turned_from((150.0, 125.0), angle, (sx * 56.0, sy * 31.0));
            let delta = turned_from((0.0, 0.0), angle, (sx * 15.0, sy * 10.0));
            let to = (handle.0 + delta.0, handle.1 + delta.1);
            held_drag(&mut editor, "mouse", handle, to, false);

            let (x, y, width, height) = frame(&editor, "a");
            assert!(
                (width - 115.0).abs() < 1e-9 && (height - 60.0).abs() < 1e-9,
                "handle ({sx}, {sy}): {width} x {height}"
            );
            let centre = (x + width / 2.0, y + height / 2.0);
            let kept = turned_from(centre, angle, (-sx * width / 2.0, -sy * height / 2.0));
            assert!(
                (kept.0 - opposite.0).abs() < 1e-9 && (kept.1 - opposite.1).abs() < 1e-9,
                "handle ({sx}, {sy}): the opposite corner moved from {opposite:?} to {kept:?}"
            );
        }
    }

    #[test]
    fn resizing_a_stroke_that_reaches_back_past_its_start_keeps_the_opposite_corner() {
        // A stroke from (100, 100) back to (50, 80) and on to (150, 130): its
        // box is 50..150 by 80..130, left of and above its first point.
        let mut editor = editor_with(&[
            r##"{"id":"a","type":"freedraw","x":100,"y":100,"width":100,"height":50,
        "points":[[0,0],[-50,-20],[50,30]],"pressures":[0.5,0.5,0.5],"version":3,"versionNonce":7}"##,
        ]);
        editor.select_all();

        // The south-east corner goes from (150, 130) to (200, 180); the north-west
        // one stays at (50, 80). The points scale by 1.5 across and 2 down and
        // keep their least coordinates, so the first point is still (100, 100).
        held_drag(&mut editor, "mouse", (156.0, 136.0), (206.0, 186.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 150.0, 100.0));
        assert_eq!(
            saved_points(&editor, "a"),
            [(25.0, 20.0), (-50.0, -20.0), (100.0, 80.0)]
        );
    }

    #[test]
    fn a_group_keeps_the_ratio_of_its_sides() {
        let grouped = |element: &str| element.replacen("\"x\"", "\"groupIds\":[\"g\"],\"x\"", 1);
        let mut editor = editor_with(&[&grouped(RECTANGLE), &grouped(BIGGER_RECTANGLE)]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        assert_eq!(editor.selection(), ["a", "b"]);

        // The box is 100..400 by 100..250; the east side is dragged out by half
        // again, and a group grows down by as much as across (Excalidraw's
        // `isInGroup`), about the middle of the west side.
        held_drag(&mut editor, "mouse", (406.0, 175.0), (556.0, 175.0), false);

        assert_eq!(frame(&editor, "a"), (100.0, 62.5, 150.0, 75.0));
        assert_eq!(frame(&editor, "b"), (400.0, 137.5, 150.0, 150.0));
    }

    #[test]
    fn a_selection_whose_text_would_get_too_small_a_font_is_not_resized() {
        // A text of a 2-unit font, 5 x 2.5 at (300, 150), selected with the
        // rectangle: the box is 100..305 by 100..152.5.
        let mut editor = editor_with(&[
            RECTANGLE,
            r##"{"id":"b","type":"text","x":300,"y":150,"width":5,"height":2.5,
        "fontSize":2,"lineHeight":1.25,"text":"abcde","originalText":"abcde","version":3,"versionNonce":7}"##,
        ]);
        editor.select_all();

        // Shrunk to two fifths, the text's font would be 0.8, under Excalidraw's
        // least of 1: nothing is resized, the rectangle neither.
        replay(
            &mut editor,
            "4.0 down 1 mouse 311 158.5
         4.1 move 1 mouse 188 127
         4.2 up 1 mouse 188 127",
        );

        assert_eq!(frame(&editor, "a"), (100.0, 100.0, 100.0, 50.0));
        nearly(&editor, "b", "fontSize", 2.0);
        assert!(!editor.can_undo());
    }

    #[test]
    fn alt_resizes_an_element_about_its_middle() {
        let mut editor = selected_rectangle();

        // The corner is dragged out by 25 x 15; with Alt the opposite corner
        // moves out as far, so the Element grows by twice that about its middle.
        replay(
            &mut editor,
            "5.0 down 1 mouse 206 156 alt
         5.1 move 1 mouse 231 171 alt
         5.2 up 1 mouse 231 171 alt",
        );

        assert_eq!(frame(&editor, "a"), (75.0, 85.0, 150.0, 80.0));
    }

    #[test]
    fn alt_resizes_the_selection_about_its_middle() {
        let mut editor = selected_rectangles();

        // The box is 100..400 by 100..250, its middle (250, 175). The corner is
        // dragged to (475, 287.5): with Alt, twice as far from the middle, so
        // everything is scaled by 1.5 about the middle.
        replay(
            &mut editor,
            "5.0 down 1 mouse 406 256 alt
         5.1 move 1 mouse 481 293.5 alt
         5.2 up 1 mouse 481 293.5 alt",
        );

        assert_eq!(frame(&editor, "a"), (25.0, 62.5, 150.0, 75.0));
        assert_eq!(frame(&editor, "b"), (325.0, 137.5, 150.0, 150.0));
    }

    #[test]
    fn a_rotation_is_one_undo_step() {
        let mut editor = selected_rectangle();
        held_drag(&mut editor, "mouse", ROTATION, (250.0, 125.0), false);

        editor.undo();
        nearly(&editor, "a", "angle", 0.0);
        assert_eq!(editor.selection(), ["a"]);

        editor.redo();
        nearly(&editor, "a", "angle", std::f64::consts::FRAC_PI_2);
        assert!(!editor.can_redo());
    }
}
