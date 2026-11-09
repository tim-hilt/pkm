//! The Selection: the set of Elements currently selected for editing.

use std::collections::HashSet;

use crate::scene::{Element, Scene};

/// The ids of the selected Elements, in the order they were selected. Ids
/// rather than handles, which change when Elements are added or removed.
///
/// Every operation is linear in the ids it is given, not in the Selection,
/// so that a Scene-wide filter by [`Selection::contains`] stays linear in a
/// select-all of thousands of Elements.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    ids: Vec<String>,
    /// The same ids, for looking them up.
    index: HashSet<String>,
}

impl Selection {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains(id)
    }

    /// Whether commands on the Selection act on `element`: it is selected,
    /// or it is the label of a selected container (labels cannot be selected
    /// on their own).
    pub fn covers(&self, element: &Element) -> bool {
        self.contains(element.id()) || element.container_id().is_some_and(|c| self.contains(c))
    }

    /// The shown Elements of `scene` that commands on the Selection act on
    /// (see [`Selection::covers`]), in z-order.
    pub fn covered<'a>(&'a self, scene: &'a Scene) -> impl Iterator<Item = Element<'a>> {
        scene
            .elements()
            .filter(move |e| !e.is_deleted() && self.covers(e))
    }

    /// Adds `id` to the Selection unless it is in it. Returns whether the
    /// Selection changed.
    pub fn add(&mut self, id: &str) -> bool {
        let absent = self.index.insert(id.to_owned());
        if absent {
            self.ids.push(id.to_owned());
        }
        absent
    }

    /// Takes `ids` out of the Selection if they all are in it, else adds
    /// them all (in the order given). Returns whether the Selection changed.
    pub fn toggle_all(&mut self, ids: &[String]) -> bool {
        if ids.is_empty() {
            return false;
        }
        if ids.iter().all(|id| self.contains(id)) {
            let toggled: HashSet<&str> = ids.iter().map(String::as_str).collect();
            self.retain(|selected| !toggled.contains(selected));
            return true;
        }
        let before = self.ids.len();
        for id in ids {
            self.add(id);
        }
        self.ids.len() != before
    }

    /// Selects exactly `ids`, once each, in the order given. Returns whether
    /// the Selection changed.
    pub(crate) fn select(&mut self, ids: impl IntoIterator<Item = String>) -> bool {
        let mut selected = Selection::default();
        for id in ids {
            selected.add(&id);
        }
        if *self == selected {
            return false;
        }
        *self = selected;
        true
    }

    /// Keeps only the selected Elements whose id satisfies `keep`.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.ids.retain(|id| keep(id));
        self.index.retain(|id| keep(id));
    }

    /// Selects nothing. Returns whether the Selection changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.ids.is_empty();
        self.ids.clear();
        self.index.clear();
        changed
    }
}
