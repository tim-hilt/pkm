//! Duplicating the Selection, as Excalidraw's `duplicateSelection` does
//! (`duplicate.ts` at the pinned commit).
//!
//! Every selected Element gets a copy, 10 units right and down of it, with a
//! new id and a new seed (so it is hand-drawn differently), right above its
//! original in z-order; the copies of a group stay together after the group's
//! last Element and form a group of their own (inside the group that was gone
//! into, if any: the copies stay in it and the groups outside it). Labels go with their
//! containers, in the same order (labels cannot be selected on their own). The copies refer to each
//! other, not to the originals: what a copy of a container lists as bound,
//! what a label is contained by and what an arrow's ends are bound to are
//! the copies, and a reference to something that was not copied is dropped.

use std::collections::HashMap;

use crate::history::Step;
use crate::scene::Scene;
use crate::selection::Selection;
use pkm_base::clock::now_ms;
use pkm_base::json::{Object, Value};
use pkm_base::random::Entropy;

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
    editing: Option<&str>,
) -> Selection {
    // In z-order, so that the copies are made and stacked in it.
    let originals: Vec<String> = selection
        .covered(scene)
        .map(|e| e.id().to_owned())
        .collect();
    let mut copying = Copying::new(&originals, entropy, editing);
    // Where the next copy goes: right above the last one of its place.
    let mut above: HashMap<String, String> = HashMap::new();

    for id in &originals {
        let Some(mut copy) = scene.snapshot(id) else {
            continue;
        };
        let copy_id = copying.make_copy(&mut copy, id, entropy, (OFFSET, OFFSET));
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
            .filter_map(|id| copying.copy_of(id).map(str::to_owned)),
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
        scene.elements().rev().find(|e| e.is_in_group(group))
    } else {
        let container = element.container_id().unwrap_or(id);
        scene
            .elements()
            .rev()
            .find(|e| e.id() == container || e.container_id() == Some(container))
    };
    last.map_or_else(|| id.to_owned(), |e| e.id().to_owned())
}

/// The copies being made of a set of Elements: which Element is a copy of
/// which, and the new groups. Shared with pasting (see [`crate::clipboard`]),
/// which copies Elements that are not in the Scene.
pub(crate) struct Copying<'a> {
    /// The id of the copy of each original, by the original's id.
    ids: HashMap<String, String>,
    /// The new group for each group of the originals that is copied.
    groups: HashMap<String, String>,
    /// The group that was gone into, whose groups and those outside it stay
    /// as they are.
    editing: Option<&'a str>,
    now: f64,
}

impl<'a> Copying<'a> {
    /// A copy with a new id for each of the Elements with `originals` as ids.
    pub fn new(originals: &[String], entropy: &mut Entropy, editing: Option<&'a str>) -> Self {
        Self {
            ids: originals
                .iter()
                .map(|id| (id.clone(), entropy.id()))
                .collect(),
            groups: HashMap::new(),
            editing,
            now: now_ms(),
        }
    }

    /// The id of the copy of the original `id`, if it is copied.
    pub fn copy_of(&self, id: &str) -> Option<&str> {
        self.ids.get(id).map(String::as_str)
    }

    /// Turns `element`, the original `original` or a snapshot of it, into
    /// its copy, `offset` away from where it was. Returns the id of the copy.
    pub fn make_copy(
        &mut self,
        element: &mut Object,
        original: &str,
        entropy: &mut Entropy,
        offset: (f64, f64),
    ) -> String {
        let id = self.copy_of(original).unwrap_or_default().to_owned();
        let number = |element: &Object, key: &str| element.get(key).and_then(Value::as_f64);
        let moved = [
            ("x", number(element, "x").unwrap_or(0.0) + offset.0),
            ("y", number(element, "y").unwrap_or(0.0) + offset.1),
            ("version", number(element, "version").unwrap_or(0.0) + 1.0),
            ("versionNonce", entropy.integer()),
            ("seed", entropy.integer()),
            ("updated", self.now),
            ("created", self.now),
        ];
        for (key, value) in moved {
            element.insert(key.into(), Value::Number(value));
        }
        element.insert("id".into(), Value::String(id.clone()));
        // The index of an Element is its place among others. Like any new
        // Element the copy has none until it is moved in z-order; until then
        // its place in the file is its order, which excalidraw.com turns
        // into an index when it opens the file (`syncInvalidIndices`).
        element.insert("index".into(), Value::Null);

        self.renew_groups(element, entropy);
        self.relink(element);
        id
    }

    fn renew_groups(&mut self, element: &mut Object, entropy: &mut Entropy) {
        let Some(Value::Array(ids)) = element.get_mut("groupIds") else {
            return;
        };
        // Only the groups inside the one that was gone into are copied.
        let editing = self.editing;
        let inside = ids
            .iter()
            .position(|group| editing.is_some() && group.as_str() == editing)
            .unwrap_or(ids.len());
        for group in ids.iter_mut().take(inside) {
            if let Some(old) = group.as_str().map(str::to_owned) {
                let new = self.groups.entry(old).or_insert_with(|| entropy.id());
                *group = Value::String(new.clone());
            }
        }
    }

    /// Makes `element` refer to the copies of the Elements it refers to, and
    /// to nothing that was not copied.
    fn relink(&self, element: &mut Object) {
        let copy_of = |value: Option<&Value>| {
            value
                .and_then(Value::as_str)
                .and_then(|old| self.copy_of(old))
                .map(str::to_owned)
        };
        if let Some(Value::Array(bound)) = element.get_mut("boundElements") {
            bound.retain_mut(|entry| {
                let copy = copy_of(entry.get("id"));
                match (copy, entry.as_object_mut()) {
                    (Some(copy), Some(entry)) => {
                        entry.insert("id".into(), Value::String(copy));
                        true
                    }
                    _ => false,
                }
            });
        }
        if let Some(container) = element.get("containerId").cloned() {
            let copy = copy_of(Some(&container));
            element.insert(
                "containerId".into(),
                copy.map_or(Value::Null, Value::String),
            );
        }
        for end in ["startBinding", "endBinding"] {
            let Some(Value::Object(binding)) = element.get_mut(end) else {
                continue;
            };
            match copy_of(binding.get("elementId")) {
                Some(copy) => binding.insert("elementId".into(), Value::String(copy)),
                None => element.insert(end.into(), Value::Null),
            }
        }
    }
}
