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

mod stack;

use crate::history::Step;
use crate::json::{Object, Value};
use crate::random::Entropy;
use crate::scene::Scene;
use crate::selection::Selection;
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
/// z-order as part of `step`. Returns whether anything moved.
pub(crate) fn move_selection(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
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
    let mut stack = Stack::new(&layers);
    match how {
        ZOrder::Forward => stack.shift_by_one(Direction::Front, &wanted),
        ZOrder::Backward => stack.shift_by_one(Direction::Back, &wanted),
        ZOrder::ToFront => stack.shift_to_end(Direction::Front, &wanted),
        ZOrder::ToBack => stack.shift_to_end(Direction::Back, &wanted),
    }
    let Some(outcome) = stack.into_outcome() else {
        return false;
    };

    step.touch_order(scene);
    // By position, which is the handle, before the Elements are moved.
    for (handle, index) in outcome.indices {
        let handle = handle as u32;
        if let Some(id) = scene.element_id(handle).map(str::to_owned) {
            step.touch(scene, &id);
        }
        scene.update(handle, entropy.integer(), [("index", Value::String(index))]);
    }
    scene.permute(&outcome.order);
    true
}

fn layer_of(element: &Object) -> Layer {
    let text = |key: &str| {
        element
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
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
