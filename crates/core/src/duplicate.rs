//! Duplicating the Selection, as Excalidraw's `duplicateSelection` does
//! (`duplicate.ts` at the pinned commit).
//!
//! Every selected Element gets a copy, 10 units right and down of it, with a
//! new id and a new seed (so it is hand-drawn differently), right above its
//! original in z-order; the copies of a group stay together after the group's
//! last Element and form a group of their own. Labels go with their
//! containers, in the same order (labels cannot be selected on their own). The copies refer to each
//! other, not to the originals: what a copy of a container lists as bound,
//! what a label is contained by and what an arrow's ends are bound to are
//! the copies, and a reference to something that was not copied is dropped.

use std::collections::HashMap;

use crate::draw_shape::now_ms;
use crate::history::Step;
use crate::json::{Object, Value};
use crate::random::Entropy;
use crate::scene::Scene;
use crate::selection::Selection;

/// Excalidraw's `DEFAULT_GRID_SIZE / 2`: how far a copy is from its original.
const OFFSET: f64 = 10.0;

/// Duplicates the Elements of `selection` as part of `step`. Returns the
/// Selection of the copies of what was selected (not of the labels that were
/// copied only because they belong to it).
pub(crate) fn duplicate(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
) -> Selection {
    // In z-order, so that the copies are made and stacked in it.
    let originals: Vec<String> = scene
        .elements()
        .filter(|e| {
            !e.is_deleted()
                && (selection.contains(e.id())
                    || e.container_id().is_some_and(|c| selection.contains(c)))
        })
        .map(|e| e.id().to_owned())
        .collect();
    let copy_ids: HashMap<&str, String> = originals
        .iter()
        .map(|id| (id.as_str(), entropy.id()))
        .collect();
    let mut groups = HashMap::new();
    // Where the next copy goes: right above the last one of its place.
    let mut above: HashMap<String, String> = HashMap::new();
    let now = now_ms();

    for id in &originals {
        let Some(mut copy) = scene.snapshot(id) else {
            continue;
        };
        let copy_id = copy_ids[id.as_str()].clone();
        make_copy(&mut copy, &copy_id, &copy_ids, &mut groups, entropy, now);
        let place = place_of(scene, id);
        let anchor = above.get(&place).unwrap_or(&place).clone();
        step.create(&copy_id);
        scene.insert_after(&anchor, copy);
        above.insert(place, copy_id);
    }

    let mut copies = Selection::default();
    copies.select(
        selection
            .ids()
            .iter()
            .filter_map(|id| copy_ids.get(id.as_str()).cloned()),
    );
    copies
}

/// The Element that copies of the Element `id` are put above, so that what
/// belongs together stays together: for an Element of a group the last
/// Element of the group, and for a container or label the last of them.
fn place_of(scene: &Scene, id: &str) -> String {
    let Some(element) = scene.element_by_id(id) else {
        return id.to_owned();
    };
    let last = if let Some(group) = element.outermost_group() {
        scene.elements().filter(|e| e.is_in_group(group)).last()
    } else {
        let container = element.container_id().unwrap_or(id);
        scene
            .elements()
            .filter(|e| e.id() == container || e.container_id() == Some(container))
            .last()
    };
    last.map_or_else(|| id.to_owned(), |e| e.id().to_owned())
}

/// Turns `element`, a snapshot of an original, into its copy `id`.
fn make_copy(
    element: &mut Object,
    id: &str,
    copy_ids: &HashMap<&str, String>,
    groups: &mut HashMap<String, String>,
    entropy: &mut Entropy,
    now: f64,
) {
    let number = |element: &Object, key: &str| element.get(key).and_then(Value::as_f64);
    let moved = [
        ("x", number(element, "x").unwrap_or(0.0) + OFFSET),
        ("y", number(element, "y").unwrap_or(0.0) + OFFSET),
        ("version", number(element, "version").unwrap_or(0.0) + 1.0),
        ("versionNonce", entropy.integer()),
        ("seed", entropy.integer()),
        ("updated", now),
        ("created", now),
    ];
    for (key, value) in moved {
        element.insert(key.into(), Value::Number(value));
    }
    element.insert("id".into(), Value::String(id.to_owned()));
    // The index of an Element is its place among others; the copy's is
    // made when the Drawing is next opened, as for any new Element.
    element.insert("index".into(), Value::Null);

    if let Some(Value::Array(ids)) = element.get_mut("groupIds") {
        for group in ids.iter_mut() {
            if let Some(old) = group.as_str().map(str::to_owned) {
                let new = groups.entry(old).or_insert_with(|| entropy.id());
                *group = Value::String(new.clone());
            }
        }
    }
    if let Some(Value::Array(bound)) = element.get_mut("boundElements") {
        bound.retain_mut(|entry| {
            let copy = entry
                .get("id")
                .and_then(Value::as_str)
                .and_then(|old| copy_ids.get(old));
            match (copy, entry.as_object_mut()) {
                (Some(copy), Some(entry)) => {
                    entry.insert("id".into(), Value::String(copy.clone()));
                    true
                }
                _ => false,
            }
        });
    }
    if let Some(Value::String(container)) = element.get("containerId").cloned() {
        let copy = copy_ids.get(container.as_str()).cloned();
        element.insert(
            "containerId".into(),
            copy.map_or(Value::Null, Value::String),
        );
    }
    for end in ["startBinding", "endBinding"] {
        let Some(Value::Object(binding)) = element.get_mut(end) else {
            continue;
        };
        let copy = binding
            .get("elementId")
            .and_then(Value::as_str)
            .and_then(|old| copy_ids.get(old))
            .cloned();
        match copy {
            Some(copy) => binding.insert("elementId".into(), Value::String(copy)),
            None => element.insert(end.into(), Value::Null),
        }
    }
}
