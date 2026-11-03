//! Groups: Elements that are selected, moved and erased together, as in
//! Excalidraw (`groups.ts`, `actionGroup.tsx` and `App.selection.ts` at the
//! pinned commit).
//!
//! An Element lists the ids of the groups it is in, innermost first, in
//! `groupIds`; nothing else is saved of a group, so a group is whatever
//! Elements share an id. A group of fewer than two shown Elements is none.
//!
//! Tapping an Element selects the *outermost* group it is in, which is
//! what [`expand`] does to anything that is selected. Going into a group
//! (the *editing group*, Excalidraw's `editingGroupId`) lets the Elements
//! of the next group down be selected on their own, and the Elements that
//! are in no further group individually. The editing group is a state of
//! the editor and not saved; it is passed in as `editing` where it matters.
//!
//! Which groups are selected is not kept anywhere: it follows from the
//! Selection (see [`selected`]), as the Selection is always made of whole
//! groups apart from within the editing group.

use std::collections::{HashMap, HashSet};

use crate::history::Step;
use crate::hit_test::is_selectable;
use crate::json::Value;
use crate::random::Entropy;
use crate::scene::{Element, Scene};
use crate::selection::Selection;
use crate::z_order;

/// The group that selecting `element` selects: the outermost one it is in,
/// not counting the editing group and those outside it.
fn unit_group<'a>(element: &Element<'a>, editing: Option<&str>) -> Option<&'a str> {
    element
        .group_ids()
        .take_while(|group| Some(*group) != editing)
        .last()
}

/// The groups that selecting the Elements `ids` selects, each with the
/// Elements (selectable ones) in it; groups with fewer than two of them are
/// left out.
fn groups_of<'a>(
    scene: &'a Scene,
    ids: &HashSet<&str>,
    editing: Option<&str>,
) -> Vec<(&'a str, Vec<&'a str>)> {
    let wanted: HashSet<&str> = scene
        .elements()
        .filter(|e| ids.contains(e.id()))
        .filter_map(|e| unit_group(&e, editing))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    // In the order the groups first appear, which is the Scene's.
    let mut members: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut found: Vec<&str> = Vec::new();
    for element in scene.elements().filter(is_selectable) {
        if let Some(group) = element.group_ids().find(|g| wanted.contains(g)) {
            let list = members.entry(group).or_default();
            if list.is_empty() {
                found.push(group);
            }
            list.push(element.id());
        }
    }
    found
        .into_iter()
        .map(|group| (group, members.remove(group).unwrap_or_default()))
        .filter(|(_, members)| members.len() >= 2)
        .collect()
}

/// `ids` together with the other Elements of the groups they select: what
/// tapping them, or boxing them, selects. `ids` come first, in their order,
/// then the others, in the Scene's.
pub(crate) fn expand(scene: &Scene, ids: &[String], editing: Option<&str>) -> Vec<String> {
    let given: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut expanded = ids.to_vec();
    for (_, members) in groups_of(scene, &given, editing) {
        expanded.extend(
            members
                .into_iter()
                .filter(|id| !given.contains(id))
                .map(str::to_owned),
        );
    }
    expanded
}

/// The groups that are selected: those whose Elements are all in the
/// Selection, as far as the Elements of the Selection select groups (not
/// counting the editing group and those outside it).
fn selected<'a>(scene: &'a Scene, selection: &Selection, editing: Option<&str>) -> Vec<&'a str> {
    let ids: HashSet<&str> = selection.ids().iter().map(String::as_str).collect();
    groups_of(scene, &ids, editing)
        .into_iter()
        .filter(|(_, members)| members.iter().all(|id| ids.contains(id)))
        .map(|(group, _)| group)
        .collect()
}

/// The group that a double-tap on the Element `hit` goes into: the selected
/// group it is in, if any.
pub(crate) fn entered(
    scene: &Scene,
    selection: &Selection,
    hit: &str,
    editing: Option<&str>,
) -> Option<String> {
    let group = unit_group(&scene.element_by_id(hit)?, editing)?;
    selected(scene, selection, editing)
        .contains(&group)
        .then(|| group.to_owned())
}

/// Whether the group command can do anything: two or more Elements are
/// selected and no group holds all of them already.
pub(crate) fn can_group(scene: &Scene, selection: &Selection, editing: Option<&str>) -> bool {
    let chosen: Vec<Element> = scene
        .elements()
        .filter(|e| !e.is_deleted() && selection.contains(e.id()))
        .collect();
    let Some(first) = chosen.first() else {
        return false;
    };
    // The groups of the first, as far as they are in reach, are the only
    // ones that can hold them all.
    let holding_all = first
        .group_ids()
        .take_while(|group| Some(*group) != editing)
        .any(|group| chosen.iter().all(|e| e.is_in_group(group)));
    chosen.len() >= 2 && !holding_all
}

/// Whether the ungroup command can do anything: a group is selected.
pub(crate) fn can_ungroup(scene: &Scene, selection: &Selection, editing: Option<&str>) -> bool {
    !selected(scene, selection, editing).is_empty()
}

/// Puts the Elements of `selection` (and the labels of its containers) into
/// one new group, as part of `step`, as `actionGroup` does: the group is
/// inside the editing group, if there is one, and holds the groups the
/// Elements are in already. Its Elements move up to the place of the
/// highest of them. Returns whether anything was done.
pub(crate) fn group(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
) -> bool {
    if !can_group(scene, selection, editing) {
        return false;
    }
    let members: Vec<(usize, String, Vec<String>)> = scene
        .elements()
        .filter(|e| !e.is_deleted() && selection.covers(e))
        .map(|e| {
            let groups = e.group_ids().map(str::to_owned).collect();
            (e.handle() as usize, e.id().to_owned(), groups)
        })
        .collect();
    let new_group = entropy.id();
    for (handle, id, mut groups) in members.iter().cloned() {
        // Just inside the editing group, or outermost.
        let position = editing
            .and_then(|editing| groups.iter().position(|g| g == editing))
            .unwrap_or(groups.len());
        groups.insert(position, new_group.clone());
        step.touch(scene, &id);
        scene.update(
            handle as u32,
            entropy.integer(),
            [(
                "groupIds",
                Value::Array(groups.into_iter().map(Value::String).collect()),
            )],
        );
    }
    let together: HashSet<usize> = members.iter().map(|(handle, ..)| *handle).collect();
    z_order::gather(scene, entropy, step, &together);
    true
}

/// Dissolves the groups that are selected, as part of `step`, as
/// `actionUngroup` does: the Elements keep the groups outside and inside
/// the ones that are gone. Returns whether anything was done.
pub(crate) fn ungroup(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    selection: &Selection,
    editing: Option<&str>,
) -> bool {
    let dissolved: HashSet<String> = selected(scene, selection, editing)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if dissolved.is_empty() {
        return false;
    }
    // Deleted Elements too, so that undoing their deletion does not bring
    // back a group.
    let members: Vec<(u32, String, Vec<String>)> = scene
        .elements()
        .filter(|e| e.group_ids().any(|g| dissolved.contains(g)))
        .map(|e| {
            let remaining = e
                .group_ids()
                .filter(|g| !dissolved.contains(*g))
                .map(str::to_owned)
                .collect();
            (e.handle(), e.id().to_owned(), remaining)
        })
        .collect();
    for (handle, id, remaining) in members {
        step.touch(scene, &id);
        scene.update(
            handle,
            entropy.integer(),
            [(
                "groupIds",
                Value::Array(remaining.into_iter().map(Value::String).collect()),
            )],
        );
    }
    true
}
