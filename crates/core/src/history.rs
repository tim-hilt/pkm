//! The undo history of the open Drawing (ADR-0006).
//!
//! The history lives as long as the Scene is open and is never saved: a
//! loaded Drawing starts with none, as on excalidraw.com.
//!
//! Every user action that changes the Scene (drawing a shape, a whole drag
//! of the Selection) is one [`Step`]: a gesture opens it before its first
//! change, names the Elements it is about to touch, and hands it back when
//! it ends. [`History::record`] turns it into an entry that remembers those
//! Elements as they were and as they became, along with the Selection at
//! both ends, so that undo and redo are plain swaps and need no knowledge
//! of what the action was. Only the Elements an action touched are kept, not
//! the Scene.
//!
//! Like on excalidraw.com an Element is never removed from the Scene by
//! undo: undoing its creation marks it deleted, redoing brings it back, and
//! each swap makes a new version of the Element. Unlike there, changing the
//! Selection alone is not an undo step: undo and redo are for the Drawing,
//! and the Selection comes along with them, restored to what it was before
//! the action when undoing and to what it was after when redoing.

use std::collections::HashSet;

use crate::json::Object;
use crate::random::Entropy;
use crate::scene::Scene;
use crate::selection::Selection;

/// Members that change with every revision of an Element without being
/// part of what the user did.
const VERSIONING: [&str; 2] = ["version", "versionNonce"];

/// An action that is in progress: the Elements it touches and the Selection
/// before it began.
#[derive(Debug)]
pub(crate) struct Step {
    selection: Selection,
    /// Each touched Element's id with how it was before, `None` if it did not
    /// exist.
    before: Vec<(String, Option<Object>)>,
}

impl Step {
    /// Opens a step for an action that begins with `selection` selected.
    pub fn begin(selection: &Selection) -> Self {
        Self {
            selection: selection.clone(),
            before: Vec::new(),
        }
    }

    /// The Selection before the action began.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Notes that the action is about to change the Element `id` of the
    /// Scene, which is as it was before.
    pub fn touch(&mut self, scene: &Scene, id: &str) {
        if !self.is_tracking(id) {
            self.before.push((id.to_owned(), scene.snapshot(id)));
        }
    }

    /// Notes that the action created the Element `id`.
    pub fn create(&mut self, id: &str) {
        if !self.is_tracking(id) {
            self.before.push((id.to_owned(), None));
        }
    }

    /// Puts every Element the action touched back as it was before it, as
    /// new versions; for an action that is abandoned.
    pub fn revert(self, scene: &mut Scene, entropy: &mut Entropy) {
        for (id, before) in self.before {
            match before {
                Some(before) => scene.restore(before, entropy.integer()),
                None => scene.mark_deleted(&id, entropy.integer()),
            }
        }
    }

    fn is_tracking(&self, id: &str) -> bool {
        self.before.iter().any(|(tracked, _)| tracked == id)
    }
}

/// What a step changed, in a form that can be applied both ways.
#[derive(Debug)]
struct Entry {
    changes: Vec<Change>,
    /// The Selection before the action and after it.
    selection: (Selection, Selection),
}

#[derive(Debug)]
struct Change {
    id: String,
    before: Option<Object>,
    after: Object,
}

impl Entry {
    /// What `step` did to the Scene, as it is now. `None` if it did nothing:
    /// Elements that ended up as they were (a move dragged back to where it
    /// began) are not changes.
    fn of(step: Step, scene: &Scene, selection: &Selection) -> Option<Self> {
        let changes: Vec<Change> = step
            .before
            .into_iter()
            .filter_map(|(id, before)| {
                let after = scene.snapshot(&id)?;
                if before.as_ref().is_some_and(|b| same_content(b, &after)) {
                    return None;
                }
                Some(Change { id, before, after })
            })
            .collect();
        (!changes.is_empty()).then(|| Self {
            changes,
            selection: (step.selection, selection.clone()),
        })
    }
}

/// Whether two versions of an Element are the same apart from their version.
fn same_content(a: &Object, b: &Object) -> bool {
    let content = |o: &Object| o.iter().filter(|(k, _)| !VERSIONING.contains(k)).count();
    content(a) == content(b)
        && a.iter()
            .all(|(key, value)| VERSIONING.contains(&key) || b.get(key) == Some(value))
}

/// What can be undone and redone.
#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Forgets everything; the Scene it was about is gone.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Makes the finished `step` the latest thing that can be undone, if it
    /// changed anything, and forgets what could be redone: that was another
    /// future.
    pub fn record(&mut self, step: Step, scene: &Scene, selection: &Selection) {
        if let Some(entry) = Entry::of(step, scene, selection) {
            self.undo.push(entry);
            self.redo.clear();
        }
    }

    /// Takes back the latest step. Returns the Selection from before it, or
    /// `None` if there was nothing to undo.
    pub fn undo(&mut self, scene: &mut Scene, entropy: &mut Entropy) -> Option<Selection> {
        let entry = self.undo.pop()?;
        for change in &entry.changes {
            match &change.before {
                Some(before) => scene.restore(before.clone(), entropy.integer()),
                None => scene.mark_deleted(&change.id, entropy.integer()),
            }
        }
        let selection = entry.selection.0.clone();
        self.redo.push(entry);
        Some(selection)
    }

    /// Does again the step that was last undone. Returns the Selection from
    /// after it, or `None` if there was nothing to redo.
    pub fn redo(&mut self, scene: &mut Scene, entropy: &mut Entropy) -> Option<Selection> {
        let entry = self.redo.pop()?;
        for change in &entry.changes {
            scene.restore(change.after.clone(), entropy.integer());
        }
        let selection = entry.selection.1.clone();
        self.undo.push(entry);
        Some(selection)
    }
}

/// The Selection without the Elements that are gone from the Scene: undoing
/// a creation leaves nothing to select.
pub(crate) fn without_gone(mut selection: Selection, scene: &Scene) -> Selection {
    if !selection.is_empty() {
        let shown: HashSet<&str> = scene
            .elements()
            .filter(|e| !e.is_deleted())
            .map(|e| e.id())
            .collect();
        selection.retain(|id| shown.contains(id));
    }
    selection
}
