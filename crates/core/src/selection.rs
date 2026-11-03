//! The Selection: the set of Elements currently selected for editing.

/// The ids of the selected Elements, in the order they were selected. Ids
/// rather than handles, which change when Elements are added or removed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    ids: Vec<String>,
}

impl Selection {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|selected| selected == id)
    }

    /// Selects `id` alone. Returns whether the Selection changed.
    pub fn select_only(&mut self, id: &str) -> bool {
        if self.ids == [id] {
            return false;
        }
        self.ids = vec![id.to_owned()];
        true
    }

    /// Adds `id` to the Selection unless it is in it. Returns whether the
    /// Selection changed.
    pub fn add(&mut self, id: &str) -> bool {
        let absent = !self.contains(id);
        if absent {
            self.ids.push(id.to_owned());
        }
        absent
    }

    /// Takes `id` out of the Selection if it is in it, else adds it.
    pub fn toggle(&mut self, id: &str) {
        if !self.add(id) {
            self.ids.retain(|selected| selected != id);
        }
    }

    /// Selects exactly `ids`, once each, in the order given. Returns whether
    /// the Selection changed.
    pub(crate) fn select(&mut self, ids: impl IntoIterator<Item = String>) -> bool {
        let mut selected: Vec<String> = Vec::new();
        for id in ids {
            if !selected.contains(&id) {
                selected.push(id);
            }
        }
        if self.ids == selected {
            return false;
        }
        self.ids = selected;
        true
    }

    /// Keeps only the selected Elements whose id satisfies `keep`.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.ids.retain(|id| keep(id));
    }

    /// Selects nothing. Returns whether the Selection changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.ids.is_empty();
        self.ids.clear();
        changed
    }
}
