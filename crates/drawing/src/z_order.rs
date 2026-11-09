//! Z-order: bringing the Selection forward or backward by one place, or to
//! the front or the back of the Scene, as Excalidraw's `actionZindex` does
//! (`zindex.ts` and `fractionalIndex.ts` at the pinned commit).
//!
//! The order of the Elements is the order of the file. A group, a frame and
//! a container with its label are passed as a whole, and deleted Elements
//! between selected ones move along. The Elements that moved get fractional
//! indices that fit their new place (see [`crate::index_sync`]), so that a
//! file reordered here is ordered the same on excalidraw.com, which sorts by
//! them.
//!
//! Grouping also moves Elements: [`gather`] brings them together.

mod stack;

use std::collections::HashSet;

use crate::history::Step;
use crate::index_sync;
use crate::scene::Scene;
use crate::selection::Selection;
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;
use stack::{Direction, Layer, Stack};

/// How the Selection is moved in z-order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrder {
    /// One place towards the front (Excalidraw's `bringForward`).
    Forward,
    /// One place towards the back (`sendBackward`).
    Backward,
    /// Above everything (`bringToFront`).
    ToFront,
    /// Below everything (`sendToBack`).
    ToBack,
}

/// Moves the Elements of `selection` (and the labels of its containers) in
/// z-order as part of `step`, within `editing_group` if one was gone into.
/// Returns whether anything moved.
pub(crate) fn move_selection(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing_group: Option<&str>,
    how: ZOrder,
) -> bool {
    let layers: Vec<Layer> = scene.raw_elements().map(layer_of).collect();
    let wanted: Vec<bool> = layers
        .iter()
        .map(|layer| {
            !layer.deleted
                && (selection.contains(&layer.id)
                    || layer
                        .container
                        .as_deref()
                        .is_some_and(|c| selection.contains(c)))
        })
        .collect();
    let mut stack = Stack::new(&layers, editing_group);
    match how {
        ZOrder::Forward => stack.shift_by_one(Direction::Front, &wanted),
        ZOrder::Backward => stack.shift_by_one(Direction::Back, &wanted),
        ZOrder::ToFront => stack.shift_to_end(Direction::Front, &wanted),
        ZOrder::ToBack => stack.shift_to_end(Direction::Back, &wanted),
    }
    let Some(outcome) = stack.into_outcome() else {
        return false;
    };

    apply(scene, entropy, step, &outcome.order, outcome.indices);
    true
}

/// Brings the Elements at the positions `together` next to each other, at
/// the place of the highest of them and in the order they have, as grouping
/// does (`actionGroup`): the Elements in between that are not among them
/// go below. The moved Elements get indices that fit their new places.
pub(crate) fn gather(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    together: &HashSet<usize>,
) {
    let Some(&top) = together.iter().max() else {
        return;
    };
    let count = scene.raw_elements().count();
    let order: Vec<usize> = (0..top)
        .filter(|place| !together.contains(place))
        .chain((0..=top).filter(|place| together.contains(place)))
        .chain(top + 1..count)
        .collect();
    let indices: Vec<Option<String>> = scene.raw_elements().map(|e| text(e, "index")).collect();
    let by_place: Vec<Option<&str>> = order.iter().map(|&at| indices[at].as_deref()).collect();
    let moved: Vec<bool> = order.iter().map(|at| together.contains(at)).collect();
    let new_indices = index_sync::sync_moved(&by_place, &moved)
        .into_iter()
        .map(|(place, index)| (order[place], index))
        .collect();
    apply(scene, entropy, step, &order, new_indices);
}

/// Puts the Elements in `order` (the positions they have now, bottom first)
/// and gives the Elements at the positions in `indices` their new index, as
/// part of `step`.
fn apply(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    order: &[usize],
    indices: Vec<(usize, String)>,
) {
    if indices.is_empty() && order.iter().enumerate().all(|(place, &at)| place == at) {
        return;
    }
    step.touch_order(scene);
    // By position, before the Elements are moved.
    for (position, index) in indices {
        let Some(handle) = scene.handle_at(position) else {
            continue;
        };
        if let Some(id) = scene.element_id(handle).map(str::to_owned) {
            step.touch(scene, &id);
        }
        scene.update(handle, entropy.integer(), [("index", Value::String(index))]);
    }
    scene.permute(order);
}

/// The string member `key` of `element`, unless it is missing or empty.
fn text(element: &Object, key: &str) -> Option<String> {
    element
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn layer_of(element: &Object) -> Layer {
    let text = |key: &str| text(element, key);
    let type_name = element.get("type").and_then(Value::as_str);
    Layer {
        id: text("id").unwrap_or_default(),
        deleted: element.get("isDeleted").and_then(Value::as_bool) == Some(true),
        groups: element
            .get("groupIds")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        frame: text("frameId"),
        is_frame: matches!(type_name, Some("frame" | "magicframe")),
        container: text("containerId"),
        // The first thing bound to it that is not an arrow: its label.
        label: element
            .get("boundElements")
            .and_then(Value::as_array)
            .and_then(|bound| {
                bound
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) != Some("arrow"))
            })
            .and_then(|b| b.get("id")?.as_str().map(str::to_owned)),
        index: text("index"),
    }
}
