//! The Action Bar (ADR-0006), tested at the Editor seam: pointer events and
//! commands in, where the bar is shown and the saved Drawing out. Excalidraw
//! has no such bar (it has a context menu, which this app does not), but
//! duplicating and deleting do what they do there.
//!
//! Expected values (anchors, offsets) are worked out by hand in the comments,
//! not computed with the formulas of the code under test.

use workingtitle_core::json::Value;
use workingtitle_core::{Editor, MagnifyEvent, Modifiers, ScrollEvent};

use crate::common;
use common::{editor_with, replay, saved_elements, tap};

/// A filled rectangle `id` from `(x, y)`, 40 wide and 20 high.
fn rectangle(id: &str, x: f64, y: f64) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":{y},"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,
        "seed":11,"version":3,"versionNonce":7}}"##
    )
}

/// A rectangle `a` at (100, 100), the only Element, selected.
fn selected() -> Editor {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    tap(&mut editor, "mouse", (120.0, 110.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

fn element(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no Element {id}"))
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

fn is_deleted(editor: &Editor, id: &str) -> bool {
    element(editor, id)
        .get("isDeleted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The ids of the Elements that are not deleted, bottom first.
fn shown_ids(editor: &Editor) -> Vec<String> {
    saved_elements(editor)
        .iter()
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .map(|e| e.get("id").and_then(Value::as_str).unwrap().to_owned())
        .collect()
}

fn anchor(editor: &Editor) -> Option<(f64, f64)> {
    editor.action_bar().map(|bar| (bar.x, bar.y))
}

// --- Visibility ---------------------------------------------------------------

#[test]
fn the_bar_is_shown_exactly_while_something_is_selected() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    assert_eq!(anchor(&editor), None);

    tap(&mut editor, "finger", (120.0, 110.0), 0.0);
    assert!(anchor(&editor).is_some());

    // A deliberate tap on empty Canvas clears the Selection.
    tap(&mut editor, "finger", (400.0, 400.0), 1.0);
    assert_eq!(anchor(&editor), None);
}

#[test]
fn the_bar_follows_the_selection_through_select_all_and_undo() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    editor.select_all();
    assert!(anchor(&editor).is_some());

    editor.duplicate();
    editor.undo();
    // Undo selects what was selected before the duplicate: the original.
    assert!(anchor(&editor).is_some());
}

#[test]
fn the_bar_is_hidden_while_the_selection_is_dragged_and_back_where_it_ends() {
    let mut editor = selected();
    replay(
        &mut editor,
        "1.0 down 1 mouse 120 110
         1.1 move 1 mouse 140 130",
    );
    assert_eq!(anchor(&editor), None);

    replay(&mut editor, "1.2 up 1 mouse 140 130");
    // Moved by (20, 20): the box is 120..160 by 120..140, so its bottom
    // middle is (140, 140), and the bar sits 32 below.
    assert_eq!(anchor(&editor), Some((140.0, 172.0)));
}

#[test]
fn the_bar_is_hidden_while_a_box_is_dragged_and_shown_for_what_it_selected() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    replay(
        &mut editor,
        "1.0 down 1 mouse 50 50
         1.1 move 1 mouse 100 100
         1.2 move 1 mouse 200 200",
    );
    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(anchor(&editor), None);

    replay(&mut editor, "1.3 up 1 mouse 200 200");
    assert!(anchor(&editor).is_some());
}

#[test]
fn holding_a_finger_down_keeps_the_bar_and_the_selection() {
    let mut editor = selected();
    let before = anchor(&editor);

    // A long press on empty Canvas is not a deliberate tap: it clears
    // nothing, and nothing else (no menu) comes of it.
    replay(
        &mut editor,
        "1.0 down 1 finger 400 400
         2.5 up 1 finger 400 400",
    );

    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(anchor(&editor), before);
}

// --- Anchor -------------------------------------------------------------------

#[test]
fn the_anchor_is_below_the_middle_of_the_selection() {
    // The rectangle spans 100..140 by 100..120: bottom middle (120, 120); the
    // bar keeps 32 view points from it, clear of the handles.
    assert_eq!(anchor(&selected()), Some((120.0, 152.0)));
}

#[test]
fn the_anchor_of_several_elements_is_below_the_middle_of_all_of_them() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    editor.select_all();

    // Together 100..340 by 100..220: bottom middle (220, 220).
    assert_eq!(anchor(&editor), Some((220.0, 252.0)));
}

#[test]
fn the_anchor_is_in_view_coordinates_and_follows_panning() {
    let mut editor = selected();

    // The Canvas content moves right 10 and down 20 with the fingers.
    editor.scroll(ScrollEvent {
        x: 0.0,
        y: 0.0,
        dx: 10.0,
        dy: 20.0,
        modifiers: Modifiers::default(),
    });

    assert_eq!(anchor(&editor), Some((130.0, 172.0)));
}

#[test]
fn the_anchor_follows_zooming_but_keeps_its_distance_from_the_selection() {
    let mut editor = selected();
    editor.set_view_size(400.0, 200.0);

    // One zoom step is 110 % around the middle of the view (200, 100). The
    // bottom middle of the selection, (120, 120), is then at
    // 200 + (120 - 200) * 1.1 = 112 and 100 + (120 - 100) * 1.1 = 122.
    editor.zoom_in();

    let (x, y) = anchor(&editor).unwrap();
    assert!((x - 112.0).abs() < 1e-9, "x = {x}");
    assert!((y - (122.0 + 32.0)).abs() < 1e-9, "y = {y}");
}

#[test]
fn pinching_keeps_the_bar() {
    let mut editor = selected();
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 0.5,
    });

    assert!(anchor(&editor).is_some());
}

// --- Duplicate ----------------------------------------------------------------

#[test]
fn duplicating_offsets_the_copy_by_ten_and_selects_it() {
    let mut editor = selected();

    editor.duplicate();

    let ids = shown_ids(&editor);
    assert_eq!(ids.len(), 2);
    let copy_id = &ids[1];
    assert_eq!(ids[0], "a");
    assert_ne!(copy_id, "a");
    assert_eq!(editor.selection(), [copy_id.as_str()]);

    let (original, copy) = (element(&editor, "a"), element(&editor, copy_id));
    assert_eq!(
        (number(&original, "x"), number(&original, "y")),
        (100.0, 100.0)
    );
    assert_eq!((number(&copy, "x"), number(&copy, "y")), (110.0, 110.0));
    for key in ["width", "height", "strokeWidth"] {
        assert_eq!(copy.get(key), original.get(key), "{key}");
    }
    assert_eq!(copy.get("backgroundColor"), original.get("backgroundColor"));
    // It is hand-drawn differently, as Excalidraw's copies are.
    assert_ne!(copy.get("seed"), original.get("seed"));
    assert_eq!(copy.get("isDeleted").and_then(Value::as_bool), Some(false));
}

#[test]
fn the_bar_moves_to_the_copy() {
    let mut editor = selected();

    editor.duplicate();

    // The copy spans 110..150 by 110..130.
    assert_eq!(anchor(&editor), Some((130.0, 162.0)));
}

#[test]
fn duplicating_several_elements_keeps_them_together_and_selects_all_copies() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    editor.select_all();

    editor.duplicate();

    let ids = shown_ids(&editor);
    // Each copy is right above its original.
    assert_eq!(ids.len(), 4);
    assert_eq!((ids[0].as_str(), ids[2].as_str()), ("a", "b"));
    assert_eq!(editor.selection(), [ids[1].as_str(), ids[3].as_str()]);
    let (a, b) = (element(&editor, &ids[1]), element(&editor, &ids[3]));
    assert_eq!((number(&a, "x"), number(&a, "y")), (110.0, 110.0));
    assert_eq!((number(&b, "x"), number(&b, "y")), (310.0, 210.0));
}

#[test]
fn only_the_selection_is_duplicated() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    tap(&mut editor, "mouse", (120.0, 110.0), 0.0);

    editor.duplicate();

    assert_eq!(shown_ids(&editor).len(), 3);
    assert_eq!(number(&element(&editor, "b"), "x"), 300.0);
}

#[test]
fn a_copy_of_a_container_takes_a_copy_of_its_label_with_it() {
    let mut editor = editor_with(&[
        r##"{"id":"box","type":"rectangle","x":0,"y":0,"width":100,"height":50,
            "boundElements":[{"type":"text","id":"label"}]}"##,
        r##"{"id":"label","type":"text","x":10,"y":10,"width":30,"height":20,
            "text":"hi","originalText":"hi","containerId":"box"}"##,
    ]);
    tap(&mut editor, "mouse", (0.0, 25.0), 0.0);
    assert_eq!(editor.selection(), ["box"]);

    editor.duplicate();

    let ids = shown_ids(&editor);
    assert_eq!(ids.len(), 4);
    let (box_copy, label_copy) = (element(&editor, &ids[2]), element(&editor, &ids[3]));
    // The copies refer to each other, not to the originals.
    let bound = box_copy
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(bound.len(), 1);
    assert_eq!(
        bound[0].get("id").and_then(Value::as_str),
        Some(ids[3].as_str())
    );
    assert_eq!(
        label_copy.get("containerId").and_then(Value::as_str),
        Some(ids[2].as_str())
    );
    // The label moves with its container; only the container is selected.
    assert_eq!(number(&label_copy, "x"), 20.0);
    assert_eq!(editor.selection(), [ids[2].as_str()]);
}

#[test]
fn a_copy_of_an_arrow_is_bound_to_the_copy_of_its_shape_and_not_to_the_original() {
    let mut editor = editor_with(&[
        r##"{"id":"box","type":"rectangle","x":0,"y":0,"width":100,"height":50,
            "backgroundColor":"#ffc9c9",
            "boundElements":[{"type":"arrow","id":"arrow"}]}"##,
        r##"{"id":"arrow","type":"arrow","x":200,"y":25,"width":-100,"height":0,
            "points":[[0,0],[-100,0]],
            "endBinding":{"elementId":"box","mode":"orbit","fixedPoint":[1,0.5]},
            "startBinding":null}"##,
    ]);
    editor.select_all();
    editor.duplicate();

    let ids = shown_ids(&editor);
    let (box_copy, arrow_copy) = (element(&editor, &ids[1]), element(&editor, &ids[3]));
    let end = arrow_copy.get("endBinding").unwrap();
    assert_eq!(
        end.get("elementId").and_then(Value::as_str),
        Some(ids[1].as_str())
    );
    let bound = box_copy
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(
        bound[0].get("id").and_then(Value::as_str),
        Some(ids[3].as_str())
    );

    // An arrow copied without its shape is not bound at all.
    let mut editor = editor_with(&[
        r##"{"id":"box","type":"rectangle","x":0,"y":0,"width":100,"height":50,
            "backgroundColor":"#ffc9c9",
            "boundElements":[{"type":"arrow","id":"arrow"}]}"##,
        r##"{"id":"arrow","type":"arrow","x":200,"y":25,"width":-100,"height":0,
            "points":[[0,0],[-100,0]],
            "endBinding":{"elementId":"box","mode":"orbit","fixedPoint":[1,0.5]},
            "startBinding":null}"##,
    ]);
    tap(&mut editor, "mouse", (150.0, 25.0), 0.0);
    assert_eq!(editor.selection(), ["arrow"]);
    editor.duplicate();
    let ids = shown_ids(&editor);
    assert_eq!(
        element(&editor, &ids[2]).get("endBinding"),
        Some(&Value::Null)
    );
    // The shape still has only its own arrow.
    let bound = element(&editor, "box");
    let bound = bound
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(bound.len(), 1);
}

#[test]
fn copies_of_grouped_elements_form_a_new_group() {
    let mut editor = editor_with(&[
        r##"{"id":"a","type":"rectangle","x":0,"y":0,"width":40,"height":20,
            "backgroundColor":"#ffc9c9","groupIds":["g"]}"##,
        r##"{"id":"b","type":"rectangle","x":100,"y":0,"width":40,"height":20,
            "backgroundColor":"#ffc9c9","groupIds":["g"]}"##,
    ]);
    editor.select_all();

    editor.duplicate();

    let ids = shown_ids(&editor);
    // The group stays together: the copies follow the last original.
    assert_eq!((ids[0].as_str(), ids[1].as_str()), ("a", "b"));
    let group_of = |id: &str| element(&editor, id).get("groupIds").cloned().unwrap();
    assert_eq!(group_of(&ids[2]), group_of(&ids[3]));
    assert_ne!(group_of(&ids[2]), group_of("a"));
}

#[test]
fn duplicating_is_one_undo_step_that_takes_the_copies_away_and_selects_the_originals() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    editor.select_all();
    editor.duplicate();
    assert_eq!(shown_ids(&editor).len(), 4);
    let copies = editor.selection().to_vec();

    editor.undo();
    assert_eq!(shown_ids(&editor), ["a", "b"]);
    assert_eq!(editor.selection(), ["a", "b"]);

    editor.redo();
    assert_eq!(shown_ids(&editor).len(), 4);
    assert_eq!(editor.selection(), copies);
}

#[test]
fn duplicating_twice_offsets_from_the_copy() {
    let mut editor = selected();

    editor.duplicate();
    editor.duplicate();

    let ids = shown_ids(&editor);
    assert_eq!(ids.len(), 3);
    let last = element(&editor, &ids[2]);
    assert_eq!((number(&last, "x"), number(&last, "y")), (120.0, 120.0));
}

#[test]
fn duplicating_with_nothing_selected_does_nothing() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    let revision = editor.revision();

    editor.duplicate();

    assert_eq!(editor.revision(), revision);
    assert!(!editor.can_undo());
    assert_eq!(shown_ids(&editor), ["a"]);
}

#[test]
fn duplicating_while_the_selection_is_dragged_does_nothing() {
    let mut editor = selected();
    replay(
        &mut editor,
        "1.0 down 1 mouse 120 110
         1.1 move 1 mouse 140 130",
    );

    editor.duplicate();

    assert_eq!(shown_ids(&editor), ["a"]);
}

// --- Delete -------------------------------------------------------------------

#[test]
fn deleting_removes_the_selection_and_hides_the_bar() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    tap(&mut editor, "mouse", (120.0, 110.0), 0.0);

    editor.delete_selection();

    assert_eq!(shown_ids(&editor), ["b"]);
    assert!(is_deleted(&editor, "a"));
    assert!(editor.selection().is_empty());
    assert_eq!(anchor(&editor), None);
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn deleting_several_elements_is_one_undo_step_that_brings_them_back_selected() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0), &rectangle("b", 300.0, 200.0)]);
    editor.select_all();
    editor.delete_selection();
    assert_eq!(editor.element_count(), 0);

    editor.undo();
    assert_eq!(shown_ids(&editor), ["a", "b"]);
    assert_eq!(editor.selection(), ["a", "b"]);
    assert!(anchor(&editor).is_some());

    editor.redo();
    assert_eq!(editor.element_count(), 0);
    assert!(editor.selection().is_empty());
}

#[test]
fn deleting_a_container_deletes_its_label() {
    let mut editor = editor_with(&[
        r##"{"id":"box","type":"rectangle","x":0,"y":0,"width":100,"height":50,
            "boundElements":[{"type":"text","id":"label"}]}"##,
        r##"{"id":"label","type":"text","x":10,"y":10,"width":30,"height":20,
            "text":"hi","originalText":"hi","containerId":"box"}"##,
    ]);
    tap(&mut editor, "mouse", (0.0, 25.0), 0.0);
    assert_eq!(editor.selection(), ["box"]);

    editor.delete_selection();

    assert_eq!(editor.element_count(), 0);
}

#[test]
fn deleting_a_shape_unbinds_the_arrows_that_were_bound_to_it() {
    let mut editor = editor_with(&[
        r##"{"id":"box","type":"rectangle","x":0,"y":0,"width":100,"height":50,
            "backgroundColor":"#ffc9c9",
            "boundElements":[{"type":"arrow","id":"arrow"}]}"##,
        r##"{"id":"arrow","type":"arrow","x":200,"y":25,"width":-100,"height":0,
            "points":[[0,0],[-100,0]],
            "endBinding":{"elementId":"box","mode":"orbit","fixedPoint":[1,0.5]},
            "startBinding":null}"##,
    ]);
    tap(&mut editor, "mouse", (50.0, 25.0), 0.0);
    assert_eq!(editor.selection(), ["box"]);

    editor.delete_selection();

    assert_eq!(shown_ids(&editor), ["arrow"]);
    assert_eq!(
        element(&editor, "arrow").get("endBinding"),
        Some(&Value::Null)
    );

    editor.undo();
    assert_eq!(shown_ids(&editor), ["box", "arrow"]);
    assert!(
        element(&editor, "arrow")
            .get("endBinding")
            .is_some_and(|b| *b != Value::Null)
    );
}

#[test]
fn deleting_with_nothing_selected_does_nothing() {
    let mut editor = editor_with(&[&rectangle("a", 100.0, 100.0)]);
    let revision = editor.revision();

    editor.delete_selection();

    assert_eq!(editor.revision(), revision);
    assert!(!editor.can_undo());
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn deleting_while_the_selection_is_dragged_does_nothing() {
    let mut editor = selected();
    replay(
        &mut editor,
        "1.0 down 1 mouse 120 110
         1.1 move 1 mouse 140 130",
    );

    editor.delete_selection();

    assert_eq!(editor.element_count(), 1);
}
