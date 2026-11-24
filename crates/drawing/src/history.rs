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
//!
//! A step can also carry the Drawing's Unit (`appState.pkmUnit`, see
//! [`Scene::unit`]) from before and after, which undo and redo swap like
//! Elements.

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
    /// The Unit before the action, if it changes it.
    unit: Option<f64>,
}

impl Step {
    /// Opens a step for an action that begins with `selection` selected.
    pub fn begin(selection: &Selection) -> Self {
        Self {
            selection: selection.clone(),
            before: Vec::new(),
            order: None,
            tracked: HashMap::new(),
            unit: None,
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

    /// Notes that the action is about to change the Unit of the Scene, which
    /// is as it was before.
    pub fn touch_unit(&mut self, scene: &Scene) {
        self.unit.get_or_insert_with(|| scene.unit());
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
    /// The Unit before the action and after it, if it changed it.
    unit: Option<(f64, f64)>,
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
        let unit = step
            .unit
            .map(|before| (before, scene.unit()))
            .filter(|(before, after)| before != after);
        (!changes.is_empty() || order.is_some() || unit.is_some()).then(|| Self {
            changes,
            order,
            unit,
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
        if let Some((before, _)) = entry.unit {
            scene.set_unit(before);
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
        if let Some((_, after)) = entry.unit {
            scene.set_unit(after);
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

#[cfg(test)]
mod tests {
    //! Undo and redo (ADR-0006), tested at the Editor seam with recorded pointer
    //! sequences: pointer events and commands in, saved Drawing, Selection and
    //! undo/redo availability out.

    use crate::{Editor, Tool};
    use pkm_base::json::{Value, parse};

    use crate::test_support as common;
    use common::{editor_with, replay, tap};

    /// A filled rectangle at (100, 100), 100 x 50.
    const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    /// The `elements` of the saved Drawing.
    fn saved(editor: &Editor) -> Vec<Value> {
        let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
        drawing
            .get("elements")
            .and_then(Value::as_array)
            .unwrap()
            .to_vec()
    }

    fn number(element: &Value, key: &str) -> f64 {
        element.get(key).and_then(Value::as_f64).unwrap()
    }

    /// The position of the Element `id`.
    fn position(editor: &Editor, id: &str) -> (f64, f64) {
        let elements = saved(editor);
        let element = elements
            .iter()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
            .unwrap();
        (number(element, "x"), number(element, "y"))
    }

    /// Draws a rectangle from (10, 10) to (60, 40) and returns to selection.
    fn draw_rectangle(editor: &mut Editor) {
        editor.set_tool(Tool::Rectangle);
        replay(
            editor,
            "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 up 1 mouse 60 40",
        );
    }

    /// Drags the Element at `from` by `(dx, dy)` with the mouse.
    fn drag(editor: &mut Editor, from: (f64, f64), (dx, dy): (f64, f64)) {
        let (x, y) = from;
        replay(
            editor,
            &format!(
                "1.0 down 1 mouse {x} {y}
             1.1 move 1 mouse {} {}
             1.2 up 1 mouse {} {}",
                x + dx,
                y + dy,
                x + dx,
                y + dy
            ),
        );
    }

    #[test]
    fn undoing_a_drawn_shape_takes_it_away_and_redoing_brings_it_back() {
        let mut editor = Editor::new();
        draw_rectangle(&mut editor);
        assert_eq!(editor.element_count(), 1);
        let drawn = saved(&editor);

        editor.undo();
        assert_eq!(editor.element_count(), 0);

        editor.redo();
        assert_eq!(editor.element_count(), 1);
        let redrawn = saved(&editor);
        assert_eq!(redrawn.len(), 1);
        for key in ["id", "x", "y", "width", "height", "seed"] {
            assert_eq!(redrawn[0].get(key), drawn[0].get(key), "{key}");
        }
    }

    #[test]
    fn undoing_a_move_puts_the_element_back_and_redoing_moves_it_again() {
        let mut editor = editor_with(&[RECTANGLE]);
        // The whole drag is one step: it passes through (120, 90) on the way.
        replay(
            &mut editor,
            "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 170 115
         1.2 move 1 mouse 180 105
         1.3 up 1 mouse 180 105",
        );
        assert_eq!(position(&editor, "a"), (130.0, 80.0));

        editor.undo();
        assert_eq!(position(&editor, "a"), (100.0, 100.0));

        editor.redo();
        assert_eq!(position(&editor, "a"), (130.0, 80.0));
    }

    #[test]
    fn steps_are_undone_latest_first_and_redone_in_the_order_they_were_done() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        drag(&mut editor, (180.0, 125.0), (0.0, 40.0));
        assert_eq!(position(&editor, "a"), (130.0, 140.0));

        editor.undo();
        assert_eq!(position(&editor, "a"), (130.0, 100.0));
        editor.undo();
        assert_eq!(position(&editor, "a"), (100.0, 100.0));
        editor.redo();
        assert_eq!(position(&editor, "a"), (130.0, 100.0));
        editor.redo();
        assert_eq!(position(&editor, "a"), (130.0, 140.0));
    }

    #[test]
    fn undo_and_redo_are_available_exactly_when_there_is_something_to_do() {
        let mut editor = Editor::new();
        assert!(!editor.can_undo() && !editor.can_redo());

        draw_rectangle(&mut editor);
        assert!(editor.can_undo() && !editor.can_redo());

        editor.undo();
        assert!(!editor.can_undo() && editor.can_redo());

        editor.redo();
        assert!(editor.can_undo() && !editor.can_redo());
    }

    #[test]
    fn undo_and_redo_with_nothing_to_do_change_nothing() {
        let mut editor = editor_with(&[RECTANGLE]);
        let (saved_before, revision) = (editor.save(), editor.revision());

        editor.undo();
        editor.redo();

        assert_eq!(editor.save(), saved_before);
        assert_eq!(editor.revision(), revision);
    }

    #[test]
    fn doing_something_new_after_undo_forgets_what_could_be_redone() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        editor.undo();
        assert!(editor.can_redo());

        drag(&mut editor, (150.0, 125.0), (0.0, 40.0));

        assert!(!editor.can_redo());
        assert_eq!(position(&editor, "a"), (100.0, 140.0));
    }

    #[test]
    fn selecting_alone_is_not_a_step_and_keeps_what_can_be_redone() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        editor.undo();

        tap(&mut editor, "mouse", (350.0, 350.0), 5.0);
        tap(&mut editor, "mouse", (150.0, 125.0), 6.0);

        assert!(editor.can_redo());
    }

    #[test]
    fn a_move_dragged_back_to_where_it_began_is_not_a_step() {
        let mut editor = editor_with(&[RECTANGLE]);

        replay(
            &mut editor,
            "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 190 125
         1.2 move 1 mouse 150 125
         1.3 up 1 mouse 150 125",
        );

        assert!(!editor.can_undo());
    }

    #[test]
    fn a_cancelled_drawing_or_move_is_not_a_step() {
        let mut editor = Editor::new();
        editor.set_tool(Tool::Rectangle);
        replay(
            &mut editor,
            "0.0 down 1 mouse 10 10
         0.1 move 1 mouse 60 40
         0.2 cancel 1 mouse 60 40",
        );
        assert!(!editor.can_undo());

        let mut editor = editor_with(&[RECTANGLE]);
        replay(
            &mut editor,
            "1.0 down 1 mouse 150 125
         1.1 move 1 mouse 190 125
         1.2 cancel 1 mouse 190 125",
        );
        assert!(!editor.can_undo());
    }

    #[test]
    fn a_tap_with_a_shape_tool_draws_nothing_and_is_not_a_step() {
        let mut editor = Editor::new();
        editor.set_tool(Tool::Ellipse);

        tap(&mut editor, "mouse", (50.0, 50.0), 0.0);

        assert!(!editor.can_undo());
    }

    #[test]
    fn undo_and_redo_make_new_versions_of_the_elements_and_redraw() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        let version = |editor: &Editor| number(&saved(editor)[0], "version");
        let (moved, revision) = (version(&editor), editor.revision());

        editor.undo();
        assert!(version(&editor) > moved, "clients cache by version");
        assert_ne!(editor.revision(), revision);

        let (undone, revision) = (version(&editor), editor.revision());
        editor.redo();
        assert!(version(&editor) > undone);
        assert_ne!(editor.revision(), revision);
    }

    #[test]
    fn undoing_a_creation_keeps_the_element_in_the_drawing_as_deleted() {
        let mut editor = Editor::new();
        draw_rectangle(&mut editor);

        editor.undo();

        let elements = saved(&editor);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].get("isDeleted"), Some(&Value::Bool(true)));
        assert_eq!(editor.element_count(), 0);
    }

    #[test]
    fn undoing_a_move_of_an_unselected_element_clears_the_selection_it_began_with() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        assert_eq!(editor.selection(), ["a"]);

        editor.undo();
        assert!(editor.selection().is_empty());

        editor.redo();
        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn undoing_a_move_of_a_selected_element_keeps_the_selection() {
        let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,
        "backgroundColor":"#ffc9c9","roundness":null}"##;
        let mut editor = editor_with(&[RECTANGLE, other]);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        drag(&mut editor, (150.0, 125.0), (0.0, 60.0));

        editor.undo();

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn undoing_the_creation_of_a_selected_element_leaves_nothing_selected() {
        let mut editor = Editor::new();
        draw_rectangle(&mut editor);
        // Select the new rectangle with a tap on its edge, then move it.
        tap(&mut editor, "mouse", (10.0, 25.0), 2.0);
        assert_eq!(editor.selection().len(), 1);
        drag(&mut editor, (10.0, 25.0), (50.0, 0.0));

        editor.undo();
        assert_eq!(editor.selection().len(), 1, "undoing the move");
        editor.undo();

        assert!(editor.selection().is_empty(), "the rectangle is gone");
    }

    #[test]
    fn undoing_changes_what_clients_redraw_the_selection_frame_on() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        let overlay = editor.overlay_revision();

        editor.undo();

        assert_ne!(editor.overlay_revision(), overlay);
    }

    #[test]
    fn nothing_is_undone_while_a_pointer_is_drawing_or_moving() {
        let mut editor = editor_with(&[RECTANGLE]);
        drag(&mut editor, (150.0, 125.0), (30.0, 0.0));
        replay(
            &mut editor,
            "2.0 down 1 mouse 180 125
         2.1 move 1 mouse 200 125",
        );

        editor.undo();

        // The first move left it at (130, 100); this one is 20 further.
        assert_eq!(position(&editor, "a"), (150.0, 100.0), "the move goes on");
        replay(&mut editor, "2.2 up 1 mouse 200 125");
        assert_eq!(position(&editor, "a"), (150.0, 100.0));
        editor.undo();
        assert_eq!(position(&editor, "a"), (130.0, 100.0));
    }

    #[test]
    fn the_history_is_not_saved_and_loading_a_drawing_starts_without_one() {
        let mut editor = Editor::new();
        draw_rectangle(&mut editor);
        editor.undo();
        draw_rectangle(&mut editor);
        editor.undo();
        let saved_bytes = editor.save();
        assert!(editor.can_redo());

        // The saved file knows nothing of undo or redo ...
        let text = String::from_utf8(saved_bytes.clone()).unwrap();
        assert!(!text.to_lowercase().contains("history"));
        assert!(!text.to_lowercase().contains("undo"));

        // ... so a new session starts with neither ...
        let mut reopened = Editor::new();
        reopened.load(&saved_bytes).unwrap();
        assert!(!reopened.can_undo() && !reopened.can_redo());
        reopened.undo();
        assert_eq!(reopened.save(), saved_bytes);

        // ... and so does the editor that loads another drawing.
        editor.load(&saved_bytes).unwrap();
        assert!(!editor.can_undo() && !editor.can_redo());
    }

    #[test]
    fn drawing_several_shapes_is_undone_one_shape_at_a_time() {
        let mut editor = Editor::new();
        editor.set_tool(Tool::Rectangle);
        editor.set_tool_lock(true);
        for y in [0.0, 100.0, 200.0] {
            replay(
                &mut editor,
                &format!(
                    "0.0 down 1 mouse 0 {y}
                 0.1 move 1 mouse 50 {}
                 0.2 up 1 mouse 50 {}",
                    y + 40.0,
                    y + 40.0
                ),
            );
        }
        assert_eq!(editor.element_count(), 3);

        editor.undo();
        assert_eq!(editor.element_count(), 2);
        editor.undo();
        editor.undo();
        assert_eq!(editor.element_count(), 0);
        assert!(!editor.can_undo());
        editor.redo();
        editor.redo();
        assert_eq!(editor.element_count(), 2);
    }

    #[test]
    fn undo_leaves_the_tool_and_the_viewport_alone() {
        let mut editor = Editor::new();
        draw_rectangle(&mut editor);
        editor.set_tool(Tool::Hand);
        editor.zoom_in();
        let zoom = editor.viewport().zoom();

        editor.undo();

        assert_eq!(editor.tool(), Tool::Hand);
        assert_eq!(editor.viewport().zoom(), zoom);
    }
}
