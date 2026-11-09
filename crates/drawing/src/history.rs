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

use std::collections::{HashMap, HashSet};

use crate::scene::{Members, Scene};
use crate::selection::Selection;
use pkm_base::json::Object;
use pkm_base::random::Entropy;

/// Members that change with every revision of an Element without being
/// part of what the user did.
const VERSIONING: [&str; 2] = ["version", "versionNonce"];

/// How an Element was or became, as far as an action touched it.
#[derive(Debug)]
enum State {
    /// The Element did not exist.
    Absent,
    /// All of the Element.
    Element(Object),
    /// Only the members the action changes: much cheaper to keep for a
    /// Selection of thousands of Elements that all move.
    Members(Members),
}

impl State {
    /// Makes the Element `id` of the Scene as this, as a new version of it.
    fn apply(&self, scene: &mut Scene, id: &str, nonce: f64) {
        match self {
            State::Absent => scene.mark_deleted(id, nonce),
            State::Element(element) => scene.put(element.clone(), nonce),
            State::Members(members) => scene.restore_members(id, members, nonce),
        }
    }
}

/// An action that is in progress: the Elements it touches and the Selection
/// before it began.
#[derive(Debug)]
pub(crate) struct Step {
    selection: Selection,
    /// Each touched Element's id with how it was before.
    before: Vec<(String, State)>,
    /// The ids of all Elements in z-order before the action, if it reorders
    /// them.
    order: Option<Vec<String>>,
    /// The place in `before` of each id in it, to find one in constant time.
    tracked: HashMap<String, usize>,
}

impl Step {
    /// Opens a step for an action that begins with `selection` selected.
    pub fn begin(selection: &Selection) -> Self {
        Self {
            selection: selection.clone(),
            before: Vec::new(),
            order: None,
            tracked: HashMap::new(),
        }
    }

    /// The Selection before the action began.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Notes that the action is about to change the Element `id` of the
    /// Scene, which is as it was before.
    pub fn touch(&mut self, scene: &Scene, id: &str) {
        match self.tracked.get(id) {
            None => {
                let before = scene.snapshot(id).map_or(State::Absent, State::Element);
                self.track(id, before);
            }
            Some(&at) => {
                // Only the members noted so far have changed: the others are
                // still as they were, so the whole Element can be made up.
                if let State::Members(members) = &self.before[at].1
                    && let Some(mut element) = scene.snapshot(id)
                {
                    for (key, value) in members {
                        match value {
                            Some(value) => element.insert((*key).into(), value.clone()),
                            None => element.remove(key),
                        }
                    }
                    self.before[at].1 = State::Element(element);
                }
            }
        }
    }

    /// Like [`Step::touch`], for an action that changes nothing of the Element
    /// `id` but its `keys` (and its version): only those are kept, which is
    /// much less to copy than the Element.
    pub fn touch_members(&mut self, scene: &Scene, id: &str, keys: &[&'static str]) {
        match self.tracked.get(id) {
            None => {
                let before = scene
                    .members(id, keys)
                    .map_or(State::Absent, State::Members);
                self.track(id, before);
            }
            Some(&at) => {
                if let State::Members(members) = &mut self.before[at].1 {
                    for &key in keys {
                        if members.iter().all(|(m, _)| *m != key) {
                            members.extend(scene.members(id, &[key]).into_iter().flatten());
                        }
                    }
                }
            }
        }
    }

    /// Notes that the action is about to change the z-order of the Elements
    /// of the Scene, which is as it was before.
    pub fn touch_order(&mut self, scene: &Scene) {
        self.order.get_or_insert_with(|| scene.order());
    }

    /// Notes that the action created the Element `id`.
    pub fn create(&mut self, id: &str) {
        if !self.tracked.contains_key(id) {
            self.track(id, State::Absent);
        }
    }

    /// Puts every Element the action touched back as it was before it, as
    /// new versions; for an action that is abandoned.
    pub fn revert(self, scene: &mut Scene, entropy: &mut Entropy) {
        if let Some(order) = &self.order {
            scene.arrange(order);
        }
        for (id, before) in &self.before {
            before.apply(scene, id, entropy.integer());
        }
    }

    /// Starts tracking `id`, which was `before`. A map rather than a search of
    /// `before`: a Selection of thousands of Elements touches each of them
    /// once.
    fn track(&mut self, id: &str, before: State) {
        self.tracked.insert(id.to_owned(), self.before.len());
        self.before.push((id.to_owned(), before));
    }
}

/// What a step changed, in a form that can be applied both ways.
#[derive(Debug)]
struct Entry {
    changes: Vec<Change>,
    /// The Elements whose z-order changed, in the places they took before
    /// the action and after it.
    order: Option<(Vec<String>, Vec<String>)>,
    /// The Selection before the action and after it.
    selection: (Selection, Selection),
}

#[derive(Debug)]
struct Change {
    id: String,
    before: State,
    after: State,
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
                let after = match &before {
                    State::Members(members) => {
                        let keys: Vec<&'static str> = members.iter().map(|(k, _)| *k).collect();
                        State::Members(scene.members(&id, &keys)?)
                    }
                    _ => State::Element(scene.snapshot(&id)?),
                };
                if same_content(&before, &after) {
                    return None;
                }
                Some(Change { id, before, after })
            })
            .collect();
        let order = step
            .order
            .and_then(|before| reordered(before, scene.order()));
        (!changes.is_empty() || order.is_some()).then(|| Self {
            changes,
            order,
            selection: (step.selection, selection.clone()),
        })
    }
}

/// The ids from the first to the last place that holds another Element in
/// `after` than in `before`, in both orders; `None` if they are the same
/// order. Elements made or removed in between are not part of it.
fn reordered(before: Vec<String>, after: Vec<String>) -> Option<(Vec<String>, Vec<String>)> {
    let kept: HashSet<&str> = after.iter().map(String::as_str).collect();
    let before: Vec<String> = before
        .into_iter()
        .filter(|id| kept.contains(id.as_str()))
        .collect();
    let known: HashSet<&str> = before.iter().map(String::as_str).collect();
    let after: Vec<String> = after
        .into_iter()
        .filter(|id| known.contains(id.as_str()))
        .collect();
    let first = before.iter().zip(&after).position(|(b, a)| b != a)?;
    let last = before.iter().zip(&after).rposition(|(b, a)| b != a)?;
    Some((before[first..=last].to_vec(), after[first..=last].to_vec()))
}

/// Whether an Element as it was and as it became are the same apart from
/// their version.
fn same_content(before: &State, after: &State) -> bool {
    match (before, after) {
        (State::Element(a), State::Element(b)) => {
            let content = |o: &Object| o.iter().filter(|(k, _)| !VERSIONING.contains(k)).count();
            content(a) == content(b)
                && a.iter()
                    .all(|(key, value)| VERSIONING.contains(&key) || b.get(key) == Some(value))
        }
        (State::Members(a), State::Members(b)) => a == b,
        _ => false,
    }
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
            change.before.apply(scene, &change.id, entropy.integer());
        }
        if let Some((before, _)) = &entry.order {
            scene.arrange(before);
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
            change.after.apply(scene, &change.id, entropy.integer());
        }
        if let Some((_, after)) = &entry.order {
            scene.arrange(after);
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
