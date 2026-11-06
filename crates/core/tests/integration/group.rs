//! Grouping and ungrouping (ADR-0006), tested at the Editor seam: taps and
//! commands in, the Selection, the long-press's group and ungroup flags and
//! the saved Drawing out.
//!
//! Expected values follow Excalidraw's `actionGroup` and `groups.ts`: an
//! Element lists its groups innermost first, a tap selects the outermost
//! group, and a double-tap goes into the group (`editingGroupId`), where the
//! Elements of the next group down are the ones that are selected. The
//! expected fractional indices are worked out with its `fractional-indexing`.

use excalidraw_native_core::json::Value;
use excalidraw_native_core::{ContextMenu, Editor, PartKind, PointerKind, ZOrder};

use crate::common;
use common::{editor_with, replay, saved_elements, tap};

/// A filled rectangle `id`, 40 wide and 20 high, at `(x, 0)`, with `index`
/// and `groups` (JSON array contents, innermost first).
fn rect(id: &str, x: f64, index: &str, groups: &str) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":"{index}",
        "groupIds":[{groups}],"seed":11,"version":3,"versionNonce":7}}"##
    )
}

/// The point on rectangle `id` that taps hit, which are 100 apart: `a` at
/// 0, `b` at 100, `c` at 200, `d` at 300.
fn centre(id: &str) -> (f64, f64) {
    let x = match id {
        "a" => 0.0,
        "b" => 100.0,
        "c" => 200.0,
        "d" => 300.0,
        _ => panic!("no such rectangle: {id}"),
    };
    (x + 20.0, 10.0)
}

/// Rectangles `a`, `b`, `c` and `d` with indices `a0` to `a3`, in the groups
/// given: `groups[0]` for `a` and so on.
fn four(groups: [&str; 4]) -> Editor {
    let [a, b, c, d] = groups;
    editor_with(&[
        &rect("a", 0.0, "a0", a),
        &rect("b", 100.0, "a1", b),
        &rect("c", 200.0, "a2", c),
        &rect("d", 300.0, "a3", d),
    ])
}

fn select(editor: &mut Editor, id: &str, t: f64) {
    tap(editor, "mouse", centre(id), t);
}

/// The long-press's answer for `id`, which must be selected: whether grouping
/// and ungrouping the Selection would do anything.
fn group_flags(editor: &mut Editor, id: &str) -> (bool, bool) {
    let (x, y) = centre(id);
    match editor.long_press(x, y, PointerKind::Finger) {
        Some(ContextMenu::Element {
            can_group,
            can_ungroup,
        }) => (can_group, can_ungroup),
        Some(ContextMenu::Canvas) | None => {
            panic!("a long-press on an Element offers the Element menu")
        }
    }
}

fn can_group(editor: &mut Editor, id: &str) -> bool {
    group_flags(editor, id).0
}

fn can_ungroup(editor: &mut Editor, id: &str) -> bool {
    group_flags(editor, id).1
}

#[test]
fn group_availability_is_readable_without_a_press() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "b"]);
    assert!(editor.can_group());
    assert!(!editor.can_ungroup());

    editor.group();
    assert!(!editor.can_group());
    assert!(editor.can_ungroup());

    // A lone Element is neither a group nor something to group.
    select(&mut editor, "c", 10.0);
    assert!(!editor.can_group());
    assert!(!editor.can_ungroup());
}

fn select_with_shift(editor: &mut Editor, id: &str, t: f64) {
    let (x, y) = centre(id);
    replay(
        editor,
        &format!(
            "{t} down 1 mouse {x} {y} shift\n{} up 1 mouse {x} {y} shift",
            t + 0.05
        ),
    );
}

/// Selects `ids`, in that order: the first by a tap, the rest with Shift.
/// Everything is selected as a unit, so for Elements that are grouped this
/// selects their whole groups.
fn select_all_of(editor: &mut Editor, ids: &[&str]) {
    for (i, id) in ids.iter().enumerate() {
        match i {
            0 => select(editor, id, 0.0),
            _ => select_with_shift(editor, id, i as f64),
        }
    }
}

fn element(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no Element {id}"))
}

/// The groups of Element `id`, innermost first.
fn groups_of(editor: &Editor, id: &str) -> Vec<String> {
    element(editor, id)
        .get("groupIds")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap().to_owned())
        .collect()
}

fn order(editor: &Editor) -> Vec<String> {
    saved_elements(editor)
        .iter()
        .map(|e| e.get("id").and_then(Value::as_str).unwrap().to_owned())
        .collect()
}

fn index_of(editor: &Editor, id: &str) -> String {
    element(editor, id)
        .get("index")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// The Elements that are selected, sorted: the order they were selected in
/// is not what these tests are about.
fn selected(editor: &Editor) -> Vec<String> {
    let mut ids = editor.selection().to_vec();
    ids.sort();
    ids
}

// --- Group -------------------------------------------------------------------

#[test]
fn grouping_puts_the_selected_elements_into_one_new_group() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "b"]);

    editor.group();

    let group = groups_of(&editor, "a");
    assert_eq!(group.len(), 1);
    assert_eq!(groups_of(&editor, "b"), group);
    assert!(groups_of(&editor, "c").is_empty());
    assert!(groups_of(&editor, "d").is_empty());
    // The Selection is the group.
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn grouped_elements_move_to_the_place_of_the_highest_of_them_and_get_indices_to_fit() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "c"]);

    editor.group();

    // `a` goes up to just below `c`, keeping the order within the group.
    assert_eq!(order(&editor), ["b", "a", "c", "d"]);
    // Only the Element that moved gets a new index: between `b` and `c`.
    assert_eq!(index_of(&editor, "b"), "a1");
    assert_eq!(index_of(&editor, "a"), "a1V");
    assert_eq!(index_of(&editor, "c"), "a2");
    assert_eq!(index_of(&editor, "d"), "a3");
}

// A z-order command still works on the Scene afterwards; see `z_order.rs`.
#[test]
fn a_group_is_moved_in_z_order_as_a_whole() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "b"]);
    editor.group();

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(order(&editor), ["c", "d", "a", "b"]);
}

#[test]
fn grouping_needs_two_elements_and_is_not_done_again_for_a_whole_group() {
    let mut editor = four(["", "", "", ""]);
    select(&mut editor, "a", 0.0);
    editor.group();
    assert!(!editor.can_undo());
    assert!(groups_of(&editor, "a").is_empty());

    select_with_shift(&mut editor, "b", 1.0);
    editor.group();
    let revision = editor.revision();
    let group = groups_of(&editor, "a");

    // The group is selected as a whole: nothing more to group.
    editor.group();
    assert_eq!(groups_of(&editor, "a"), group);
    assert_eq!(editor.revision(), revision);
}

#[test]
fn grouping_a_group_with_another_element_nests_it() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "b"]);
    editor.group();
    let inner = groups_of(&editor, "a");

    select_with_shift(&mut editor, "c", 5.0);
    editor.group();

    // Innermost first: the old group, then the new one.
    let a = groups_of(&editor, "a");
    assert_eq!(a.len(), 2);
    assert_eq!(a[0], inner[0]);
    assert_eq!(groups_of(&editor, "b"), a);
    assert_eq!(groups_of(&editor, "c"), vec![a[1].clone()]);
    assert_eq!(selected(&editor), ["a", "b", "c"]);
}

#[test]
fn grouping_takes_the_label_of_a_container_along() {
    let mut editor = editor_with(&[
        r#"{"id":"box","type":"rectangle","x":0,"y":0,"width":40,"height":20,"index":"a0",
            "groupIds":[],"boundElements":[{"id":"label","type":"text"}]}"#,
        r#"{"id":"label","type":"text","x":5,"y":5,"width":10,"height":10,"index":"a1",
            "groupIds":[],"containerId":"box","text":"hi","originalText":"hi"}"#,
        &rect("other", 100.0, "a2", ""),
    ]);
    tap(&mut editor, "mouse", (0.0, 10.0), 0.0);
    select_with_shift(&mut editor, "b", 1.0);
    assert_eq!(selected(&editor), ["box", "other"]);

    editor.group();

    let group = groups_of(&editor, "box");
    assert_eq!(group.len(), 1);
    assert_eq!(groups_of(&editor, "label"), group);
}

// --- Select ------------------------------------------------------------------

#[test]
fn tapping_a_grouped_element_selects_the_whole_group() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);

    select(&mut editor, "b", 0.0);

    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn tapping_a_nested_element_selects_the_outermost_group() {
    // `a` and `b` are in `inner`, which is in `outer` along with `c`.
    let mut editor = four([
        "\"inner\",\"outer\"",
        "\"inner\",\"outer\"",
        "\"outer\"",
        "",
    ]);

    select(&mut editor, "a", 0.0);

    assert_eq!(selected(&editor), ["a", "b", "c"]);
}

#[test]
fn a_group_of_deleted_elements_and_one_other_is_no_group() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, "a0", "\"g\""),
        &rect("b", 100.0, "a1", "\"g\"").replace("\"seed\"", "\"isDeleted\":true,\"seed\""),
    ]);

    select(&mut editor, "a", 0.0);

    assert_eq!(selected(&editor), ["a"]);
    assert!(!can_ungroup(&mut editor, "a"));
}

#[test]
fn shift_tapping_a_grouped_element_adds_or_takes_away_the_whole_group() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "c", 0.0);

    select_with_shift(&mut editor, "a", 1.0);
    assert_eq!(selected(&editor), ["a", "b", "c"]);

    select_with_shift(&mut editor, "b", 2.0);
    assert_eq!(selected(&editor), ["c"]);
}

#[test]
fn dragging_a_grouped_element_moves_the_whole_group() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 120 10
         0.1 move 1 mouse 130 40
         0.2 up 1 mouse 130 40",
    );

    assert_eq!(selected(&editor), ["a", "b"]);
    for id in ["a", "b"] {
        let y = element(&editor, id).get("y").and_then(Value::as_f64);
        assert_eq!(y, Some(30.0), "{id}");
    }
    let y = element(&editor, "c").get("y").and_then(Value::as_f64);
    assert_eq!(y, Some(0.0));
}

#[test]
fn a_box_selects_the_whole_group_of_what_it_encloses() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);

    // Around `a` only, which is 0..40 wide.
    replay(
        &mut editor,
        "0.0 down 1 mouse -20 -20
         0.1 move 1 mouse 20 5
         0.2 move 1 mouse 60 40
         0.3 up 1 mouse 60 40",
    );

    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn select_all_selects_every_element_of_every_group() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);

    editor.select_all();

    assert_eq!(selected(&editor), ["a", "b", "c", "d"]);
}

// --- Ungroup -----------------------------------------------------------------

#[test]
fn ungrouping_dissolves_the_selected_group_and_keeps_the_elements_selected() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    assert!(can_ungroup(&mut editor, "a"));

    editor.ungroup();

    assert!(groups_of(&editor, "a").is_empty());
    assert!(groups_of(&editor, "b").is_empty());
    assert_eq!(selected(&editor), ["a", "b"]);
    assert!(!can_ungroup(&mut editor, "a"));
    // Tapping selects what is tapped alone from here on.
    select(&mut editor, "a", 1.0);
    assert_eq!(selected(&editor), ["a"]);
}

#[test]
fn ungrouping_a_nested_group_dissolves_the_outer_one_and_keeps_the_inner_one() {
    let mut editor = four([
        "\"inner\",\"outer\"",
        "\"inner\",\"outer\"",
        "\"outer\"",
        "",
    ]);
    select(&mut editor, "c", 0.0);

    editor.ungroup();

    assert_eq!(groups_of(&editor, "a"), ["inner"]);
    assert_eq!(groups_of(&editor, "b"), ["inner"]);
    assert!(groups_of(&editor, "c").is_empty());
    // The Selection is as it was: the Elements of the groups that are left
    // are all selected.
    assert_eq!(selected(&editor), ["a", "b", "c"]);
}

#[test]
fn ungrouping_without_a_selected_group_does_nothing() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "b"]);
    let revision = editor.revision();

    editor.ungroup();

    assert_eq!(editor.revision(), revision);
    assert!(!editor.can_undo());
}

#[test]
fn ungrouping_leaves_other_groups_alone() {
    let mut editor = four(["\"g\"", "\"g\"", "\"h\"", "\"h\""]);
    select(&mut editor, "a", 0.0);

    editor.ungroup();

    assert_eq!(groups_of(&editor, "c"), ["h"]);
    assert_eq!(groups_of(&editor, "d"), ["h"]);
}

// --- Undo --------------------------------------------------------------------

#[test]
fn grouping_is_one_undo_step_that_takes_back_the_groups_and_the_order() {
    let mut editor = four(["", "", "", ""]);
    select_all_of(&mut editor, &["a", "c"]);
    editor.group();

    editor.undo();

    assert_eq!(order(&editor), ["a", "b", "c", "d"]);
    assert!(groups_of(&editor, "a").is_empty());
    assert!(groups_of(&editor, "c").is_empty());
    assert_eq!(index_of(&editor, "a"), "a0");
    assert_eq!(selected(&editor), ["a", "c"]);
    assert!(!editor.can_undo());

    editor.redo();

    assert_eq!(order(&editor), ["b", "a", "c", "d"]);
    assert_eq!(groups_of(&editor, "a").len(), 1);
    assert_eq!(index_of(&editor, "a"), "a1V");
    assert_eq!(selected(&editor), ["a", "c"]);
}

#[test]
fn ungrouping_is_one_undo_step_that_brings_the_group_back() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    editor.ungroup();

    editor.undo();

    assert_eq!(groups_of(&editor, "a"), ["g"]);
    assert_eq!(groups_of(&editor, "b"), ["g"]);
    select(&mut editor, "b", 1.0);
    assert_eq!(selected(&editor), ["a", "b"]);

    editor.redo();

    assert!(groups_of(&editor, "a").is_empty());
}

// --- Going into a group -------------------------------------------------------

/// A double-tap on `id` with a finger: two taps 0.2 s apart.
fn double_tap(editor: &mut Editor, id: &str, t: f64) {
    tap(editor, "finger", centre(id), t);
    tap(editor, "finger", centre(id), t + 0.2);
}

#[test]
fn a_double_tap_goes_into_the_group_and_selects_the_element_alone() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);

    double_tap(&mut editor, "b", 5.0);

    assert_eq!(selected(&editor), ["b"]);
    // Inside the group its elements are selected one by one.
    select(&mut editor, "a", 10.0);
    assert_eq!(selected(&editor), ["a"]);
    // Neither is a group, so there is nothing to ungroup.
    assert!(!can_ungroup(&mut editor, "a"));
    select_with_shift(&mut editor, "b", 11.0);
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn a_double_tap_on_an_element_that_is_not_in_a_selected_group_goes_nowhere() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);

    // The first tap of the double-tap selects the group, the second goes in.
    double_tap(&mut editor, "a", 0.0);
    assert_eq!(selected(&editor), ["a"]);

    // An ungrouped element has no group to go into.
    double_tap(&mut editor, "c", 5.0);
    assert_eq!(selected(&editor), ["c"]);
}

#[test]
fn two_taps_too_slow_or_too_far_apart_are_no_double_tap() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);

    tap(&mut editor, "finger", centre("a"), 5.0);
    tap(&mut editor, "finger", centre("a"), 6.0);
    assert_eq!(selected(&editor), ["a", "b"]);

    tap(&mut editor, "finger", centre("a"), 10.0);
    tap(&mut editor, "finger", (centre("a").0 + 40.0, 10.0), 10.2);
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn tapping_outside_the_group_or_on_empty_canvas_goes_out_of_it() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    double_tap(&mut editor, "a", 5.0);
    assert_eq!(selected(&editor), ["a"]);

    // Out by tapping another Element: it is selected, and then so is any
    // group it is in as a whole.
    select(&mut editor, "c", 10.0);
    select(&mut editor, "a", 11.0);
    assert_eq!(selected(&editor), ["a", "b"]);

    // Out by tapping empty Canvas.
    double_tap(&mut editor, "a", 15.0);
    assert_eq!(selected(&editor), ["a"]);
    tap(&mut editor, "mouse", (700.0, 300.0), 20.0);
    assert!(editor.selection().is_empty());
    select(&mut editor, "a", 21.0);
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn a_double_tap_goes_into_a_nested_group_one_level_at_a_time() {
    let mut editor = four([
        "\"inner\",\"outer\"",
        "\"inner\",\"outer\"",
        "\"outer\"",
        "",
    ]);
    select(&mut editor, "a", 0.0);
    assert_eq!(selected(&editor), ["a", "b", "c"]);

    // Into `outer`: its next groups down are selected as wholes.
    double_tap(&mut editor, "a", 5.0);
    assert_eq!(selected(&editor), ["a", "b"]);
    select(&mut editor, "c", 10.0);
    assert_eq!(selected(&editor), ["c"]);
    select(&mut editor, "b", 11.0);
    assert_eq!(selected(&editor), ["a", "b"]);
    assert!(can_ungroup(&mut editor, "a"));

    // Into `inner`: now the Elements are on their own.
    double_tap(&mut editor, "b", 15.0);
    assert_eq!(selected(&editor), ["b"]);
    select(&mut editor, "a", 20.0);
    assert_eq!(selected(&editor), ["a"]);
}

#[test]
fn grouping_inside_a_group_makes_the_new_group_part_of_it() {
    let mut editor = four(["\"g\"", "\"g\"", "\"g\"", ""]);
    select(&mut editor, "a", 0.0);
    double_tap(&mut editor, "a", 5.0);
    select_with_shift(&mut editor, "b", 10.0);
    assert_eq!(selected(&editor), ["a", "b"]);
    assert!(can_group(&mut editor, "a"));

    editor.group();

    // Just inside `g`.
    let a = groups_of(&editor, "a");
    assert_eq!(a.len(), 2);
    assert_eq!(a[1], "g");
    assert_eq!(groups_of(&editor, "b"), a);
    assert_eq!(groups_of(&editor, "c"), ["g"]);
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn duplicating_inside_a_group_puts_the_copy_into_it() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    double_tap(&mut editor, "a", 5.0);

    editor.duplicate();

    let copy = editor.selection()[0].clone();
    assert_eq!(groups_of(&editor, &copy), ["g"]);
    // Still inside the group, whose elements are still taken one by one.
    select(&mut editor, "b", 10.0);
    assert_eq!(selected(&editor), ["b"]);
}

#[test]
fn undoing_leaves_the_group_when_the_selection_is_no_longer_inside_it() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "c", 0.0);
    editor.duplicate();
    select(&mut editor, "a", 5.0);
    double_tap(&mut editor, "a", 10.0);
    assert_eq!(selected(&editor), ["a"]);

    // Undo selects the duplicated `c`, which is outside the group.
    editor.undo();
    assert_eq!(selected(&editor), ["c"]);
    select(&mut editor, "a", 15.0);
    assert_eq!(selected(&editor), ["a", "b"]);
}

#[test]
fn a_box_called_off_by_another_finger_keeps_the_group_that_was_gone_into() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    double_tap(&mut editor, "a", 5.0);

    replay(
        &mut editor,
        "10.0 down 1 mouse 150 -50
         10.1 move 1 mouse 350 80
         10.2 cancel 1 mouse 350 80",
    );

    assert_eq!(selected(&editor), ["a"]);
    select(&mut editor, "b", 15.0);
    assert_eq!(selected(&editor), ["b"]);
}

// --- Group frames on the overlay -----------------------------------------------

/// The overlay's parts that belong to an Element's frame (solid) and to a
/// group's (dashed), as the chains of corners they trace. Handles have no
/// Element and are left out.
type Corners = Vec<(f32, f32)>;

fn frames(editor: &Editor) -> (Vec<Corners>, Vec<Corners>) {
    let overlay = editor.overlay();
    let (mut solid, mut dashed) = (Vec::new(), Vec::new());
    for part in overlay.parts() {
        if part.kind != PartKind::Stroke as u32 {
            continue;
        }
        let corners: Vec<_> = common::ops(&overlay, part)
            .iter()
            .map(|(_, c)| (c[0], c[1]))
            .collect();
        if part.dash_on > 0.0 {
            // Excalidraw's `groupSelectionColor` (black), 8 on and 4 off, one wide.
            assert_eq!(part.color, 0x0000_00ff);
            assert_eq!((part.dash_on, part.dash_off, part.width), (8.0, 4.0, 1.0));
            dashed.push(corners);
        } else if part.element != u32::MAX {
            solid.push(corners);
        }
    }
    (solid, dashed)
}

/// The box a frame traces, as `(min, max)`.
fn bounds_of(corners: &[(f32, f32)]) -> ((f32, f32), (f32, f32)) {
    let xs = corners.iter().map(|c| c.0);
    let ys = corners.iter().map(|c| c.1);
    (
        (
            xs.clone().fold(f32::MAX, f32::min),
            ys.clone().fold(f32::MAX, f32::min),
        ),
        (xs.fold(f32::MIN, f32::max), ys.fold(f32::MIN, f32::max)),
    )
}

#[test]
fn a_selected_group_gets_one_dashed_frame_around_all_its_elements() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);

    let (solid, dashed) = frames(&editor);

    // Padded by 4 on every side; the elements have no frames of their own.
    assert!(solid.is_empty());
    assert_eq!(dashed.len(), 1);
    assert_eq!(bounds_of(&dashed[0]), ((-4.0, -4.0), (144.0, 24.0)));
}

#[test]
fn nested_groups_are_framed_as_the_outermost_one_that_is_selected() {
    let mut editor = four(["\"i\",\"o\"", "\"i\",\"o\"", "\"o\"", ""]);
    select(&mut editor, "a", 0.0);

    let (solid, dashed) = frames(&editor);

    assert!(solid.is_empty());
    assert_eq!(dashed.len(), 1);
    assert_eq!(bounds_of(&dashed[0]), ((-4.0, -4.0), (244.0, 24.0)));
}

#[test]
fn the_group_that_was_gone_into_keeps_its_frame_around_the_selected_element() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select(&mut editor, "a", 0.0);
    double_tap(&mut editor, "b", 5.0);

    let (solid, dashed) = frames(&editor);

    assert_eq!(selected(&editor), ["b"]);
    assert_eq!(dashed.len(), 1);
    assert_eq!(bounds_of(&dashed[0]), ((-4.0, -4.0), (144.0, 24.0)));
    assert_eq!(solid.len(), 1);
    assert_eq!(bounds_of(&solid[0]), ((96.0, -4.0), (144.0, 24.0)));
}

#[test]
fn a_group_and_a_single_element_selected_together_are_framed_each_its_own_way() {
    let mut editor = four(["\"g\"", "\"g\"", "", ""]);
    select_all_of(&mut editor, &["a", "c"]);

    let (solid, dashed) = frames(&editor);

    assert_eq!(dashed.len(), 1);
    assert_eq!(bounds_of(&dashed[0]), ((-4.0, -4.0), (144.0, 24.0)));
    assert_eq!(solid.len(), 1);
    assert_eq!(bounds_of(&solid[0]), ((196.0, -4.0), (244.0, 24.0)));
}
