//! Arrows bound to shapes (ADR-0006), as Excalidraw does at the pinned commit
//! (`binding.ts`), with the data it saves.
//!
//! An end of an arrow is bound to a shape when the arrow's `startBinding` or
//! `endBinding` names it, and the shape lists the arrow in its
//! `boundElements`. The binding says where on the shape the arrow aims: its
//! `fixedPoint`, as fractions of the shape's width and height, so that it
//! follows when the shape moves, is resized or turned. The `mode` says how the
//! end sits on the shape:
//!
//! * `orbit`: the arrow stops short of the shape, at a gap of 5 plus half the
//!   shape's stroke width from its outline, on the line towards the arrow's
//!   other end. Arrows bind this way when drawn.
//! * `inside`: the arrow ends exactly at the fixed point. Only read from
//!   files.
//!
//! Ends are bound when a pointer puts them near a shape ([`bind_end`]) and
//! unbound when it puts them elsewhere or when the arrow moves without the
//! shape; shapes that change take the arrows bound to them along
//! ([`Attachments`]). Everything that changes the Scene does so as part of a
//! [`Step`], so that undoing a gesture also undoes its bindings.
//!
//! Not done: elbow arrows, which Excalidraw routes (their bindings are kept as
//! they are), the dwell that switches a drawn arrow to `inside`, and bindings
//! of older files that have no `fixedPoint`: those stay in the file and the
//! arrow does not follow.

use std::collections::HashSet;

use crate::edit_points;
use crate::frame::Frame;
use crate::history::Step;
use crate::input::PointerKind;
use crate::json::{Object, Value};
use crate::outline::Outline;
use crate::path::Point;
use crate::random::Entropy;
use crate::scene::{Element, ElementType, Scene};

/// Excalidraw's `BASE_BINDING_GAP`; the stroke of the shape adds to it.
const BASE_GAP: f64 = 5.0;
/// Excalidraw's `BASE_ARROW_MIN_LENGTH`: an arrow between two shapes shorter
/// than this ends at the aimed points instead of stopping short.
const MIN_ARROW_LENGTH: f64 = 10.0;
/// Excalidraw's base binding distance, in Scene units at 100 %.
const BASE_REACH: f64 = 15.0;
/// Half of the 44 points Apple recommends for touch targets: how near a
/// finger must be to a shape to bind to it, in view points.
const FINGER_REACH: f64 = 22.0;
/// Excalidraw's overlap rule for nested shapes: a smaller shape on top of the
/// closest one wins if it is mostly inside it.
const NESTED_OVERLAP: f64 = 0.25;
const NESTED_SIZE: f64 = 0.75;

/// The ends of an arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Start,
    End,
}

impl End {
    const BOTH: [End; 2] = [End::Start, End::End];

    fn key(self) -> &'static str {
        match self {
            End::Start => "startBinding",
            End::End => "endBinding",
        }
    }
}

/// How far in the Scene a pointer of `kind` may be from a shape and still
/// bind to it, in a view at `zoom` (Excalidraw's `maxBindingDistance_simple`;
/// a finger reaches as far as it does for everything else).
pub(crate) fn reach(kind: PointerKind, zoom: f64) -> f64 {
    let zoom_value = if zoom < 1.0 { zoom } else { 1.0 };
    let excalidraw = (BASE_REACH / (zoom_value * 1.5)).clamp(BASE_REACH, BASE_REACH * 2.0);
    match kind {
        PointerKind::Finger => excalidraw.max(FINGER_REACH / zoom),
        PointerKind::Pencil | PointerKind::Mouse => excalidraw,
    }
}

/// How far an arrow stops short of a shape: Excalidraw's `getBindingGap`.
fn gap_of(shape: &Element) -> f64 {
    BASE_GAP + shape.stroke_width() / 2.0
}

/// An end's binding as the file has it.
struct Bound<'a> {
    shape: &'a str,
    inside: bool,
    /// `None` for bindings of older files.
    fixed_point: Option<Point>,
}

fn bound<'a>(arrow: &Element<'a>, end: End) -> Option<Bound<'a>> {
    let binding = arrow.member(end.key())?.as_object()?;
    let pair = |value: &Value| {
        let [x, y] = value.as_array()? else {
            return None;
        };
        let (x, y) = (x.as_f64()?, y.as_f64()?);
        (x.is_finite() && y.is_finite()).then_some((x, y))
    };
    Some(Bound {
        shape: binding.get("elementId")?.as_str()?,
        inside: binding.get("mode").and_then(Value::as_str) == Some("inside"),
        fixed_point: binding.get("fixedPoint").and_then(pair),
    })
}

// --- Finding the shape ----------------------------------------------------

/// The shape a pointer at `at` would bind an arrow end to, if any: the
/// bindable shapes it is inside or within `reach` of, topmost first, up to
/// the first opaque one that has it inside, of which the one whose outline is
/// nearest (Excalidraw's `getHoveredElementForBinding`).
pub(crate) fn hovered(scene: &Scene, at: Point, reach: f64) -> Option<String> {
    struct Candidate<'a> {
        element: Element<'a>,
        /// Negative inside.
        distance: f64,
        area: f64,
        frame: Frame,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let bindable = scene
        .elements()
        .rev()
        .filter(|e| !e.is_deleted() && !e.is_locked())
        .filter_map(|e| Some((Outline::of(&e)?, e)));
    for (outline, element) in bindable {
        let distance = outline.distance(at);
        if distance >= reach {
            continue;
        }
        let opaque = distance <= 0.0 && is_opaque(&element);
        candidates.push(Candidate {
            element,
            distance,
            area: outline.area().max(0.00001),
            frame: outline.frame(),
        });
        if opaque {
            break;
        }
    }
    let nearest = candidates
        .iter()
        .min_by(|a, b| a.distance.abs().total_cmp(&b.distance.abs()))?;
    let nested = (nearest.distance <= 0.0)
        .then(|| {
            candidates.iter().find(|c| {
                c.element.id() != nearest.element.id()
                    && c.distance <= 0.0
                    && overlap(&c.frame, &nearest.frame) > NESTED_OVERLAP * c.area
                    && c.area / nearest.area < NESTED_SIZE
            })
        })
        .flatten();
    Some(nested.unwrap_or(nearest).element.id().to_owned())
}

/// The area `a` and `b` share.
fn overlap(a: &Frame, b: &Frame) -> f64 {
    let width = (a.max.0.min(b.max.0) - a.min.0.max(b.min.0)).max(0.0);
    let height = (a.max.1.min(b.max.1) - a.min.1.max(b.min.1)).max(0.0);
    width * height
}

/// Whether the shape hides what is behind it from binding: images and shapes
/// with a background (Excalidraw's `isOpaqueForBinding`).
fn is_opaque(element: &Element) -> bool {
    match element.kind() {
        ElementType::Image => true,
        ElementType::Text => false,
        _ => element.background_color() != "transparent",
    }
}

/// The polygon the highlight of the shape `id` follows, in the Scene, if it
/// can be bound to: its outline at the gap an arrow keeps from it.
pub(crate) fn highlight(scene: &Scene, id: &str) -> Option<Vec<Point>> {
    let shape = scene.element_by_id(id)?;
    Some(Outline::of(&shape)?.contour(gap_of(&shape)))
}

// --- Binding and unbinding ----------------------------------------------------

/// Binds `end` of the arrow `arrow_id` to the shape a pointer at `at` is on
/// (see [`hovered`]), aimed at where the pointer is, or unbinds it if there is
/// none. Returns the shape it is bound to, if any, which is the one to
/// highlight.
pub(crate) fn bind_end(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    arrow_id: &str,
    end: End,
    at: Point,
    reach: f64,
) -> Option<String> {
    let Some(shape) = hovered(scene, at, reach) else {
        unbind(scene, entropy, step, arrow_id, end);
        return None;
    };
    bind(scene, entropy, step, arrow_id, end, &shape, at);
    Some(shape)
}

/// Binds `end` of the arrow `arrow_id` to the shape `shape_id`, orbiting it
/// and aimed at `at`.
fn bind(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    arrow_id: &str,
    end: End,
    shape_id: &str,
    at: Point,
) {
    let Some(arrow) = scene.element_by_id(arrow_id) else {
        return;
    };
    let Some(outline) = scene.element_by_id(shape_id).and_then(|s| Outline::of(&s)) else {
        return;
    };
    let (rx, ry) = outline.fixed_point(at);
    let mut binding = Object::new();
    binding.insert("elementId".into(), Value::String(shape_id.to_owned()));
    binding.insert("mode".into(), Value::String("orbit".into()));
    binding.insert(
        "fixedPoint".into(),
        Value::Array(vec![Value::Number(rx), Value::Number(ry)]),
    );
    let binding = Value::Object(binding);
    let previous = bound(&arrow, end).map(|b| b.shape.to_owned());
    if arrow.member(end.key()) != Some(&binding) {
        let handle = arrow.handle();
        step.touch(scene, arrow_id);
        scene.update(handle, entropy.integer(), [(end.key(), binding)]);
    }
    sync_bound(scene, entropy, step, shape_id, arrow_id);
    if let Some(previous) = previous.filter(|p| p != shape_id) {
        sync_bound(scene, entropy, step, &previous, arrow_id);
    }
}

/// Unbinds `end` of the arrow `arrow_id`, if it is bound.
pub(crate) fn unbind(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    arrow_id: &str,
    end: End,
) {
    let Some(arrow) = scene.element_by_id(arrow_id) else {
        return;
    };
    if matches!(arrow.member(end.key()), None | Some(Value::Null)) {
        return;
    }
    let shape = bound(&arrow, end).map(|b| b.shape.to_owned());
    let handle = arrow.handle();
    step.touch(scene, arrow_id);
    scene.update(handle, entropy.integer(), [(end.key(), Value::Null)]);
    if let Some(shape) = shape {
        sync_bound(scene, entropy, step, &shape, arrow_id);
    }
}

/// Unbinds both ends of the arrow `arrow_id`.
pub(crate) fn unbind_all(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    arrow_id: &str,
) {
    for end in End::BOTH {
        unbind(scene, entropy, step, arrow_id, end);
    }
}

/// Unbinds every arrow end that is bound to the shape `shape_id`, for a shape
/// that goes away.
pub(crate) fn unbind_from(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    shape_id: &str,
) {
    let arrows: Vec<String> = scene
        .element_by_id(shape_id)
        .and_then(|shape| shape.member("boundElements")?.as_array())
        .unwrap_or_default()
        .iter()
        .filter_map(|b| Some(b.get("id")?.as_str()?.to_owned()))
        .collect();
    for id in arrows {
        for end in End::BOTH {
            let bound_here = scene
                .element_by_id(&id)
                .and_then(|arrow| Some(bound(&arrow, end)?.shape == shape_id))
                .unwrap_or(false);
            if bound_here {
                unbind(scene, entropy, step, &id, end);
            }
        }
    }
}

/// Makes the `boundElements` of the shape `shape_id` list the arrow
/// `arrow_id` exactly if one of the arrow's ends is bound to the shape.
fn sync_bound(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    shape_id: &str,
    arrow_id: &str,
) {
    let wanted = scene.element_by_id(arrow_id).is_some_and(|arrow| {
        !arrow.is_deleted()
            && End::BOTH
                .iter()
                .any(|&end| bound(&arrow, end).is_some_and(|b| b.shape == shape_id))
    });
    let Some(shape) = scene.element_by_id(shape_id) else {
        return;
    };
    let mut list: Vec<Value> = shape
        .member("boundElements")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .to_vec();
    let is_arrow = |entry: &Value| entry.get("id").and_then(Value::as_str) == Some(arrow_id);
    let listed = list.iter().any(is_arrow);
    if listed == wanted {
        return;
    }
    if wanted {
        let mut entry = Object::new();
        entry.insert("id".into(), Value::String(arrow_id.to_owned()));
        entry.insert("type".into(), Value::String("arrow".into()));
        list.push(Value::Object(entry));
    } else {
        list.retain(|entry| !is_arrow(entry));
    }
    let handle = shape.handle();
    step.touch(scene, shape_id);
    scene.update(
        handle,
        entropy.integer(),
        [("boundElements", Value::Array(list))],
    );
}

// --- Following the shapes -----------------------------------------------------

/// Where an end of an arrow is bound: the shape and the point on it the arrow
/// aims at.
struct Anchor {
    outline: Outline,
    gap: f64,
    focus: Point,
    inside: bool,
}

fn anchor(scene: &Scene, arrow: &Element, end: End) -> Option<Anchor> {
    let binding = bound(arrow, end)?;
    let shape = scene.element_by_id(binding.shape)?;
    if shape.is_deleted() {
        return None;
    }
    let outline = Outline::of(&shape)?;
    Some(Anchor {
        focus: outline.at_fixed_point(binding.fixed_point?),
        gap: gap_of(&shape),
        outline,
        inside: binding.inside,
    })
}

/// Puts the bound ends of the arrow `arrow_id` where its bindings say, after
/// the shapes they are bound to changed (Excalidraw's `updateBoundArrow`).
/// Other points stay where they are shown.
pub(crate) fn follow(scene: &mut Scene, entropy: &mut Entropy, arrow_id: &str) {
    let Some(arrow) = scene.element_by_id(arrow_id) else {
        return;
    };
    if arrow.is_deleted() || arrow.kind() != ElementType::Arrow || arrow.is_elbow_arrow() {
        return;
    }
    let shown = edit_points::shown_points(&arrow);
    let last = shown.len().saturating_sub(1);
    // An arrow that has only just begun has no length to place ends by.
    if last == 0 || arrow.points().last() == Some(&(0.0, 0.0)) {
        return;
    }
    let anchors = End::BOTH.map(|end| anchor(scene, &arrow, end));
    let heads = (
        arrow.start_arrowhead().is_some(),
        arrow.end_arrowhead().is_some(),
    );
    let mut placed = shown.clone();
    for (end, index, adjacent) in [(End::Start, 0, shown[1]), (End::End, last, shown[last - 1])] {
        let (mine, other) = match end {
            End::Start => (&anchors[0], &anchors[1]),
            End::End => (&anchors[1], &anchors[0]),
        };
        let Some(mine) = mine else { continue };
        let has_head = if end == End::Start { heads.0 } else { heads.1 };
        placed[index] = end_position(
            mine,
            other.as_ref(),
            adjacent,
            shown.len() == 2,
            (has_head, heads.0 || heads.1),
        );
    }
    if placed != shown {
        edit_points::reshape(scene, entropy, arrow.handle(), &placed);
    }
}

/// Where the end bound at `mine` of an arrow is shown, given the anchor of
/// its other end (if bound), the point next to it on the arrow and whether
/// the arrow has only two points (Excalidraw's `updateBoundPoint`).
/// `heads` tells whether this end has an arrowhead, and whether either has.
fn end_position(
    mine: &Anchor,
    other: Option<&Anchor>,
    adjacent: Point,
    two_points: bool,
    (has_head, any_head): (bool, bool),
) -> Point {
    if mine.inside {
        return mine.focus;
    }
    let aimed_at = match other {
        Some(other) if two_points => other.focus,
        _ => adjacent,
    };
    // Of the crossings of the line to the other end with the outline, the
    // one nearest to the other end.
    let outline_point = mine.outline.crossings(mine.focus, aimed_at, mine.gap).pop();
    let on_outline = outline_point.unwrap_or(mine.focus);
    // An arrow without arrowheads, or with one here, touches what it aims
    // at; only an arrow with one at the other end stops short.
    let resolved = if !any_head || has_head {
        mine.focus
    } else {
        on_outline
    };
    let Some(other) = other else {
        return if too_short(adjacent, on_outline) {
            mine.focus
        } else {
            on_outline
        };
    };
    // Stopping short must not turn the arrow around: if its end would be
    // inside the shape at the other end, it aims there instead.
    if other.outline.area() < mine.outline.area() * 2.0
        && outline_point.is_some_and(|p| other.outline.distance(p) <= other.gap)
    {
        return resolved;
    }
    let other_outline = other
        .outline
        .crossings(mine.focus, aimed_at, other.gap)
        .into_iter()
        .next();
    let other_target = other_outline.unwrap_or(other.focus);
    if too_short(other_target, on_outline) {
        resolved
    } else {
        on_outline
    }
}

fn too_short(a: Point, b: Point) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) <= MIN_ARROW_LENGTH
}

/// The arrows that follow shapes that change, and the arrows that change with
/// them, for a gesture that moves, resizes or turns some Elements.
#[derive(Debug)]
pub(crate) struct Attachments {
    /// The ids of the arrows among the Elements that change, which are bound
    /// to something.
    arrows: Vec<String>,
    /// The ids of the Elements that change.
    changing: HashSet<String>,
    /// The arrows bound to them that do not change themselves.
    followers: Vec<String>,
}

impl Attachments {
    /// Notes the arrows that follow the Elements `changing`, and touches them
    /// for `step`, which they are a part of.
    pub fn begin(step: &mut Step, scene: &Scene, changing: Vec<String>) -> Self {
        let changing: HashSet<String> = changing.into_iter().collect();
        let mut arrows = Vec::new();
        let mut followers: Vec<String> = Vec::new();
        for element in scene.elements().filter(|e| changing.contains(e.id())) {
            if element.kind() == ElementType::Arrow {
                if End::BOTH.iter().any(|&end| bound(&element, end).is_some()) {
                    arrows.push(element.id().to_owned());
                }
                continue;
            }
            let listed = element
                .member("boundElements")
                .and_then(Value::as_array)
                .unwrap_or_default();
            for id in listed.iter().filter_map(|b| b.get("id")?.as_str()) {
                let follows = !changing.contains(id)
                    && !followers.iter().any(|f| f == id)
                    && scene
                        .element_by_id(id)
                        .is_some_and(|a| a.kind() == ElementType::Arrow && !a.is_deleted());
                if follows {
                    step.touch(scene, id);
                    followers.push(id.to_owned());
                }
            }
        }
        Self {
            arrows,
            changing,
            followers,
        }
    }

    /// Brings the arrows up to date after the Elements changed: arrows that
    /// changed without the shape they are bound to are unbound from it, and
    /// the arrows bound to shapes that changed follow them.
    pub fn update(&self, scene: &mut Scene, entropy: &mut Entropy, step: &mut Step) {
        for id in &self.arrows {
            for end in End::BOTH {
                let stray = scene
                    .element_by_id(id)
                    .and_then(|arrow| Some(!self.changing.contains(bound(&arrow, end)?.shape)))
                    .unwrap_or(false);
                if stray {
                    unbind(scene, entropy, step, id, end);
                }
            }
        }
        for id in &self.followers {
            follow(scene, entropy, id);
        }
    }
}
