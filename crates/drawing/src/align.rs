//! Aligning and distributing the Selection, as Excalidraw's `actionAlign` and
//! `actionDistribute` do (`align.ts`, `distribute.ts` and
//! `getSelectedElementsByGroup` in `groups.ts` at the pinned commit).
//!
//! Both move *units* by the boxes they are shown in (see [`Frame::shown`]):
//! each selected group, or Element, with the labels of its containers. With
//! one group selected, its next groups down and its own single Elements are
//! the units, so that its parts line up. Arrows bound to what moves follow it
//! (see [`crate::binding`]).

use crate::binding::Attachments;
use crate::frame::Frame;
use crate::history::Step;
use crate::scene::{Element, ElementType, Scene};
use crate::selection::Selection;
use pkm_base::random::Entropy;

/// Which edges or middles of the units are lined up with the Selection's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    /// The middles across (Excalidraw's `alignHorizontallyCentered`).
    CenterHorizontally,
    Right,
    Top,
    /// The middles down (`alignVerticallyCentered`).
    CenterVertically,
    Bottom,
}

/// Along which axis the gaps between the units are made equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    Horizontally,
    Vertically,
}

/// Moves of less than this are none: what is aligned already stays.
const EPSILON: f64 = 1e-9;

/// A group or Element that moves as a whole.
struct Unit {
    /// Its Elements, labels included.
    ids: Vec<String>,
    frame: Frame,
}

/// The units of the Selection, in z-order (see the module).
fn units(scene: &Scene, selection: &Selection, editing: Option<&str>) -> Vec<Unit> {
    let selected: Vec<Element> = scene
        .elements()
        .filter(|e| !e.is_deleted() && !e.is_locked() && selection.contains(e.id()))
        .filter(|e| !is_label(e))
        .collect();
    // The group selecting each Element selects, as `group::unit_group`.
    let outer = |e: &Element<'_>| -> Option<String> {
        e.group_ids()
            .take_while(|group| Some(*group) != editing)
            .last()
            .map(str::to_owned)
    };
    let first = selected.first().and_then(outer);
    let one_group = first.is_some() && selected.iter().all(|e| outer(e) == first);
    let key = |e: &Element<'_>| -> String {
        let Some(group) = outer(e) else {
            return e.id().to_owned();
        };
        if !one_group {
            return group;
        }
        // The group right inside the selected one, if any.
        let ids: Vec<&str> = e.group_ids().collect();
        match ids.iter().position(|g| *g == group) {
            Some(at) if at > 0 => ids[at - 1].to_owned(),
            _ => e.id().to_owned(),
        }
    };

    let mut keys: Vec<String> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    for element in &selected {
        let key = key(element);
        let at = keys.iter().position(|k| *k == key).unwrap_or_else(|| {
            keys.push(key);
            units.push(Unit {
                ids: Vec::new(),
                frame: Frame::shown(element),
            });
            units.len() - 1
        });
        let unit = &mut units[at];
        unit.ids.push(element.id().to_owned());
        unit.frame = unit.frame.union(Frame::shown(element));
    }
    // The labels go with their containers.
    for label in scene.elements().filter(|e| !e.is_deleted() && is_label(e)) {
        let container = label.container_id().unwrap_or_default();
        if let Some(unit) = units
            .iter_mut()
            .find(|u| u.ids.iter().any(|id| id == container))
        {
            unit.ids.push(label.id().to_owned());
            unit.frame = unit.frame.union(Frame::shown(&label));
        }
    }
    units
}

fn is_label(element: &Element) -> bool {
    element.kind() == ElementType::Text && element.container_id().is_some()
}

/// Whether aligning would do anything with this Selection: it has two units.
pub(crate) fn can_align(scene: &Scene, selection: &Selection, editing: Option<&str>) -> bool {
    units(scene, selection, editing).len() > 1
}

/// Whether distributing would: it has three units.
pub(crate) fn can_distribute(scene: &Scene, selection: &Selection, editing: Option<&str>) -> bool {
    units(scene, selection, editing).len() > 2
}

/// Lines up the units of `selection` with the edge or middle of all of them,
/// as part of `step`. Returns whether anything moved.
pub(crate) fn align(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
    how: Alignment,
) -> bool {
    let units = units(scene, selection, editing);
    if units.len() < 2 {
        return false;
    }
    let bounds = all(&units);
    let middle = |f: &Frame| f.centre();
    let moves: Vec<(f64, f64)> = units
        .iter()
        .map(|u| {
            let f = &u.frame;
            match how {
                Alignment::Left => (bounds.min.0 - f.min.0, 0.0),
                Alignment::CenterHorizontally => (middle(&bounds).0 - middle(f).0, 0.0),
                Alignment::Right => (bounds.max.0 - f.max.0, 0.0),
                Alignment::Top => (0.0, bounds.min.1 - f.min.1),
                Alignment::CenterVertically => (0.0, middle(&bounds).1 - middle(f).1),
                Alignment::Bottom => (0.0, bounds.max.1 - f.max.1),
            }
        })
        .collect();
    shift(scene, entropy, step, &units, &moves)
}

/// Spaces the units of `selection` evenly between the outermost ones, as part
/// of `step`: with equal gaps, or, where they would overlap, with equally
/// spaced middles. Returns whether anything moved.
pub(crate) fn distribute(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
    how: Distribution,
) -> bool {
    let mut units = units(scene, selection, editing);
    if units.len() < 3 {
        return false;
    }
    // Start, end and middle of a frame along the axis.
    let span = |f: &Frame| match how {
        Distribution::Horizontally => (f.min.0, f.max.0),
        Distribution::Vertically => (f.min.1, f.max.1),
    };
    let mid = |f: &Frame| {
        let (start, end) = span(f);
        (start + end) / 2.0
    };
    let (start, end) = span(&all(&units));
    units.sort_by(|a, b| mid(&a.frame).total_cmp(&mid(&b.frame)));
    let sizes: f64 = units
        .iter()
        .map(|u| span(&u.frame).1 - span(&u.frame).0)
        .sum();
    let gap = (end - start - sizes) / (units.len() - 1) as f64;

    let along: Vec<f64> = if gap < 0.0 {
        // Overlapping: the middles are spaced between those of the units
        // that hold the ends, which stay.
        let first = units.iter().position(|u| span(&u.frame).0 == start);
        let last = units.iter().position(|u| span(&u.frame).1 == end);
        let (Some(first), Some(last)) = (first, last) else {
            return false;
        };
        let (from, to) = (mid(&units[first].frame), mid(&units[last].frame));
        let step = (to - from) / (units.len() - 1) as f64;
        let mut at = from;
        units
            .iter()
            .enumerate()
            .map(|(index, u)| {
                if index == first || index == last {
                    return 0.0;
                }
                at += step;
                at - mid(&u.frame)
            })
            .collect()
    } else {
        let mut at = start;
        units
            .iter()
            .map(|u| {
                let (s, e) = span(&u.frame);
                let by = at - s;
                at += e - s + gap;
                by
            })
            .collect()
    };
    let moves: Vec<(f64, f64)> = along
        .into_iter()
        .map(|by| match how {
            Distribution::Horizontally => (by, 0.0),
            Distribution::Vertically => (0.0, by),
        })
        .collect();
    shift(scene, entropy, step, &units, &moves)
}

/// The box around all the units.
fn all(units: &[Unit]) -> Frame {
    units
        .iter()
        .map(|u| u.frame)
        .reduce(Frame::union)
        .expect("at least one unit")
}

/// Moves each unit by its move, as part of `step`, and the arrows bound to
/// them along. Returns whether anything moved.
fn shift(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    units: &[Unit],
    moves: &[(f64, f64)],
) -> bool {
    let moving: Vec<(&Unit, (f64, f64))> = units
        .iter()
        .zip(moves.iter().copied())
        .filter(|(_, (dx, dy))| dx.abs() > EPSILON || dy.abs() > EPSILON)
        .collect();
    if moving.is_empty() {
        return false;
    }
    let ids: Vec<String> = moving.iter().flat_map(|(u, _)| u.ids.clone()).collect();
    for id in &ids {
        step.touch_members(scene, id, &["x", "y"]);
    }
    let attachments = Attachments::begin(step, scene, ids);
    for (unit, (dx, dy)) in moving {
        for id in &unit.ids {
            let Some(element) = scene.element_by_id(id) else {
                continue;
            };
            let (handle, at) = (element.handle(), (element.x() + dx, element.y() + dy));
            scene.set_position(handle, at, entropy.integer());
        }
    }
    attachments.update(scene, entropy, step);
    true
}
