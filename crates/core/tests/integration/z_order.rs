//! Z-order (ADR-0006), tested at the Editor seam: taps select, commands move
//! the Selection, and the saved Drawing shows the order and the fractional
//! `index` of every Element, which is what excalidraw.com sorts by.
//!
//! Expected orders follow Excalidraw's `zindex.ts` and expected keys its
//! `fractional-indexing` (worked out with the vendored implementation).

use excalidraw_native_core::json::Value;
use excalidraw_native_core::{Editor, ZOrder};

use crate::common;
use common::{editor_with, replay, saved_elements, tap};

/// A filled rectangle `id` with `index` (if any), 40 wide, 20 high, at
/// `x` (and y 0), with `extra` members (JSON, with a leading comma).
fn rect(id: &str, x: f64, index: Option<&str>, extra: &str) -> String {
    let index = index.map_or("null".to_owned(), |i| format!("\"{i}\""));
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":{index},
        "seed":11,"version":3,"versionNonce":7{extra}}}"##
    )
}

/// Rectangles `a`, `b` and `c`, bottom first, side by side with indices
/// `a0`, `a1` and `a2`, as excalidraw.com writes them.
fn three() -> Editor {
    editor_with(&[
        &rect("a", 0.0, Some("a0"), ""),
        &rect("b", 100.0, Some("a1"), ""),
        &rect("c", 200.0, Some("a2"), ""),
    ])
}

/// The centre of the rectangle at `x`.
fn centre(x: f64) -> (f64, f64) {
    (x + 20.0, 10.0)
}

fn select(editor: &mut Editor, x: f64, t: f64) {
    tap(editor, "mouse", centre(x), t);
}

/// Adds the rectangle at `x` to the Selection with a Shift-tap.
fn add_to_selection(editor: &mut Editor, x: f64, t: f64) {
    let (px, py) = centre(x);
    replay(
        editor,
        &format!(
            "{t} down 1 mouse {px} {py} shift\n{} up 1 mouse {px} {py} shift",
            t + 0.05
        ),
    );
}

fn ids(editor: &Editor) -> Vec<String> {
    saved_elements(editor)
        .iter()
        .map(|e| e.get("id").and_then(Value::as_str).unwrap().to_owned())
        .collect()
}

fn indices(editor: &Editor) -> Vec<Option<String>> {
    saved_elements(editor)
        .iter()
        .map(|e| e.get("index").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// What excalidraw.com needs to read the file in the order it is in: every
/// Element has an index, and they ascend.
fn assert_indices_ascend(editor: &Editor) {
    let indices = indices(editor);
    assert!(indices.iter().all(Option::is_some), "{indices:?}");
    assert!(indices.windows(2).all(|w| w[0] < w[1]), "{indices:?}");
}

fn index_of(editor: &Editor, id: &str) -> String {
    let position = ids(editor).iter().position(|i| i == id).unwrap();
    indices(editor)[position].clone().unwrap()
}

// --- The four operations ------------------------------------------------------

#[test]
fn bring_forward_passes_the_next_element() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::Forward);

    assert_eq!(ids(&editor), ["b", "a", "c"]);
    // Between b (a1) and c (a2).
    assert_eq!(
        indices(&editor),
        [Some("a1".into()), Some("a1V".into()), Some("a2".into())]
    );
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn send_backward_passes_the_previous_element() {
    let mut editor = three();
    select(&mut editor, 200.0, 0.0);

    editor.move_in_z_order(ZOrder::Backward);

    assert_eq!(ids(&editor), ["a", "c", "b"]);
    // Between a (a0) and b (a1).
    assert_eq!(index_of(&editor, "c"), "a0V");
}

#[test]
fn bring_to_front_goes_above_everything() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(ids(&editor), ["b", "c", "a"]);
    assert_eq!(index_of(&editor, "a"), "a3");
}

#[test]
fn send_to_back_goes_below_everything() {
    let mut editor = three();
    select(&mut editor, 200.0, 0.0);

    editor.move_in_z_order(ZOrder::ToBack);

    assert_eq!(ids(&editor), ["c", "a", "b"]);
    assert_eq!(index_of(&editor, "c"), "Zz");
}

#[test]
fn only_the_moved_elements_change() {
    let mut editor = three();
    let before = saved_elements(&editor);
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::ToFront);

    let after = saved_elements(&editor);
    // b and c are as they were, a is a new version of itself.
    assert_eq!(after[0], before[1]);
    assert_eq!(after[1], before[2]);
    assert_eq!(after[2].get("version").and_then(Value::as_f64), Some(4.0));
    assert_ne!(after[2].get("versionNonce"), before[0].get("versionNonce"));
}

#[test]
fn moving_past_the_end_does_nothing() {
    let mut editor = three();
    let before = editor.save();
    select(&mut editor, 200.0, 0.0);
    select(&mut editor, 200.0, 1.0);

    editor.move_in_z_order(ZOrder::Forward);
    editor.move_in_z_order(ZOrder::ToFront);
    select(&mut editor, 0.0, 2.0);
    editor.move_in_z_order(ZOrder::Backward);
    editor.move_in_z_order(ZOrder::ToBack);

    assert_eq!(editor.save(), before);
    assert!(!editor.can_undo());
}

#[test]
fn nothing_moves_without_a_selection() {
    let mut editor = three();
    let before = editor.save();

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(editor.save(), before);
}

#[test]
fn several_selected_elements_keep_their_order_among_themselves() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    add_to_selection(&mut editor, 200.0, 1.0);
    assert_eq!(editor.selection().len(), 2);

    editor.move_in_z_order(ZOrder::ToBack);
    assert_eq!(ids(&editor), ["a", "c", "b"]);
    assert_indices_ascend(&editor);

    // c is already at the front: only a moves past b.
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    add_to_selection(&mut editor, 200.0, 1.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["b", "a", "c"]);
    assert_indices_ascend(&editor);
}

#[test]
fn separated_selected_elements_all_move_to_the_front_together() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    add_to_selection(&mut editor, 100.0, 1.0);

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(ids(&editor), ["c", "a", "b"]);
    assert_eq!(
        indices(&editor),
        [Some("a2".into()), Some("a3".into()), Some("a4".into())]
    );
}

// --- What is passed as a unit -------------------------------------------------

#[test]
fn deleted_elements_between_selected_ones_move_along() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, Some("a0"), ""),
        &rect("gone", 100.0, Some("a1"), r#","isDeleted":true"#),
        &rect("b", 200.0, Some("a2"), ""),
        &rect("c", 300.0, Some("a3"), ""),
    ]);
    select(&mut editor, 0.0, 0.0);
    add_to_selection(&mut editor, 200.0, 1.0);

    editor.move_in_z_order(ZOrder::Forward);

    // `gone` is between a and b, so it is moved with them.
    assert_eq!(ids(&editor), ["c", "a", "gone", "b"]);
    assert_indices_ascend(&editor);
}

#[test]
fn deleted_elements_are_not_passed_as_a_place() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, Some("a0"), ""),
        &rect("gone", 100.0, Some("a1"), r#","isDeleted":true"#),
        &rect("b", 200.0, Some("a2"), ""),
    ]);
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::Forward);

    // One step is past the next shown element, deleted ones in between too.
    assert_eq!(ids(&editor), ["gone", "b", "a"]);
}

#[test]
fn a_group_is_passed_as_a_whole() {
    let editor_with_group = || {
        editor_with(&[
            &rect("a", 0.0, Some("a0"), ""),
            &rect("g1", 100.0, Some("a1"), r#","groupIds":["g"]"#),
            &rect("g2", 200.0, Some("a2"), r#","groupIds":["g"]"#),
            &rect("z", 300.0, Some("a3"), ""),
        ])
    };

    let mut editor = editor_with_group();
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["g1", "g2", "a", "z"]);
    assert_indices_ascend(&editor);

    let mut editor = editor_with_group();
    select(&mut editor, 300.0, 0.0);
    editor.move_in_z_order(ZOrder::Backward);
    assert_eq!(ids(&editor), ["a", "z", "g1", "g2"]);
    assert_indices_ascend(&editor);
}

#[test]
fn a_label_is_passed_with_its_container() {
    let label = |x: f64| {
        format!(
            r#"{{"id":"t","type":"text","x":{x},"y":0,"width":10,"height":10,"text":"hi",
            "originalText":"hi","fontSize":20,"fontFamily":5,"textAlign":"center",
            "verticalAlign":"middle","containerId":"c","index":"a2","version":1,"versionNonce":1}}"#
        )
    };
    let editor_with_label = || {
        editor_with(&[
            &rect("x", 0.0, Some("a0"), ""),
            &rect(
                "c",
                100.0,
                Some("a1"),
                r#","boundElements":[{"id":"t","type":"text"}]"#,
            ),
            &label(110.0),
            &rect("y", 300.0, Some("a3"), ""),
        ])
    };

    // Moving the container moves its label along, above it.
    let mut editor = editor_with_label();
    select(&mut editor, 100.0, 0.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["x", "y", "c", "t"]);
    assert_indices_ascend(&editor);

    // Passing the container passes the label too.
    let mut editor = editor_with_label();
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["c", "t", "x", "y"]);
}

#[test]
fn elements_of_unknown_types_are_passed_like_any_other() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, Some("a0"), ""),
        r#"{"id":"u","type":"embeddable","x":500,"y":500,"width":10,"height":10,"index":"a1"}"#,
        &rect("b", 200.0, Some("a2"), ""),
    ]);
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::Forward);

    assert_eq!(ids(&editor), ["u", "a", "b"]);
    assert_eq!(index_of(&editor, "a"), "a1V");
}

#[test]
fn a_frame_child_stays_inside_its_frame() {
    let frame = r#"{"id":"f","type":"frame","x":-10,"y":-10,"width":300,"height":50,
        "name":null,"index":"a2"}"#;
    let editor_in_frame = || {
        editor_with(&[
            &rect("c1", 0.0, Some("a0"), r#","frameId":"f""#),
            &rect("c2", 100.0, Some("a1"), r#","frameId":"f""#),
            frame,
        ])
    };

    let mut editor = editor_in_frame();
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["c2", "c1", "f"]);

    // As in Excalidraw's `shiftElementsToEnd`, the frame itself counts as
    // the end of the frame's range, so a child that goes to the front ends up
    // after a frame that is above its siblings.
    let mut editor = editor_in_frame();
    select(&mut editor, 100.0, 0.0);
    editor.move_in_z_order(ZOrder::ToFront);
    assert_eq!(ids(&editor), ["c1", "f", "c2"]);
}

// --- Indices ------------------------------------------------------------------

#[test]
fn elements_without_an_index_are_given_one_so_that_the_file_stays_ordered() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, None, ""),
        &rect("b", 100.0, None, ""),
        &rect("c", 200.0, None, ""),
    ]);
    select(&mut editor, 0.0, 0.0);

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(ids(&editor), ["b", "c", "a"]);
    assert_eq!(
        indices(&editor),
        [Some("a0".into()), Some("a1".into()), Some("a2".into())]
    );
}

#[test]
fn a_file_of_excalidraw_com_is_written_back_ordered_by_its_indices() {
    let mut editor = editor_with(&[
        &rect("a", 0.0, Some("a0"), ""),
        &rect("b", 100.0, Some("a0V"), ""),
        &rect("c", 200.0, Some("a1"), ""),
        &rect("d", 300.0, Some("a1V"), ""),
    ]);

    // Sorting by index is what excalidraw.com does when it opens the file.
    let sorted = |editor: &Editor| {
        let mut elements: Vec<_> = ids(editor).into_iter().zip(indices(editor)).collect();
        elements.sort_by(|x, y| x.1.cmp(&y.1));
        elements.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
    };
    let commands = [
        (0.0, ZOrder::Forward),
        (300.0, ZOrder::Backward),
        (100.0, ZOrder::ToBack),
        (200.0, ZOrder::ToFront),
        (0.0, ZOrder::Backward),
        (300.0, ZOrder::ToFront),
    ];
    for (n, (x, how)) in commands.into_iter().enumerate() {
        select(&mut editor, x, n as f64 * 2.0);
        editor.move_in_z_order(how);
        assert_eq!(
            sorted(&editor),
            ids(&editor),
            "after {how:?} of the one at {x}"
        );
        assert_indices_ascend(&editor);
    }
}

// --- Undo ---------------------------------------------------------------------

#[test]
fn undo_and_redo_restore_the_order_and_the_indices() {
    let mut editor = three();
    let original = saved_elements(&editor);
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::ToFront);
    let moved = saved_elements(&editor);
    assert_eq!(ids(&editor), ["b", "c", "a"]);

    editor.undo();
    assert_eq!(ids(&editor), ["a", "b", "c"]);
    assert_eq!(
        indices(&editor),
        [Some("a0".into()), Some("a1".into()), Some("a2".into())]
    );
    assert_eq!(editor.selection(), ["a"]);

    editor.redo();
    assert_eq!(ids(&editor), ["b", "c", "a"]);
    assert_eq!(indices(&editor), indices_of(&moved));
    assert_ne!(original.len(), 0);
}

fn indices_of(elements: &[Value]) -> Vec<Option<String>> {
    elements
        .iter()
        .map(|e| e.get("index").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

#[test]
fn each_command_is_one_undo_step() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::Forward);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["b", "c", "a"]);

    editor.undo();
    assert_eq!(ids(&editor), ["b", "a", "c"]);
    editor.undo();
    assert_eq!(ids(&editor), ["a", "b", "c"]);
    assert!(!editor.can_undo());
}

#[test]
fn undo_puts_elements_back_that_were_drawn_since() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    editor.move_in_z_order(ZOrder::ToFront);
    // Another Element is made on top (a copy of a), then undone again.
    editor.duplicate();
    assert_eq!(ids(&editor).len(), 4);

    editor.undo();
    editor.undo();

    assert_eq!(&ids(&editor)[..3], ["a", "b", "c"]);
}

// --- Clients ------------------------------------------------------------------

#[test]
fn the_geometry_generation_and_handles_stay_when_elements_change_places() {
    let mut editor = three();
    select(&mut editor, 0.0, 0.0);
    let before = editor.geometry().generation();
    let handles = |editor: &Editor| -> Vec<u32> {
        let mut handles: Vec<u32> = editor
            .geometry()
            .parts()
            .iter()
            .map(|p| p.element)
            .collect();
        handles.dedup();
        handles
    };
    let drawn_before = handles(&editor);

    editor.move_in_z_order(ZOrder::ToFront);

    assert_eq!(editor.geometry().generation(), before);
    // `a` is drawn last now, with the handle it had at the bottom.
    let drawn = handles(&editor);
    assert_eq!(drawn[..drawn.len() - 1], drawn_before[1..]);
    assert_eq!(drawn[drawn.len() - 1], drawn_before[0]);
}

// --- With a group gone into (Excalidraw's `editingGroupId`) ---------------------

/// `o`, then `a`, `b`, `c` (the group `g`, with `a` and `b` also in the
/// inner group `i` if `inner`), then `z`; `g` gone into, `pick` selected.
fn inside_group(inner: bool, pick: f64) -> Editor {
    let ab = if inner {
        r#","groupIds":["i","g"]"#
    } else {
        r#","groupIds":["g"]"#
    };
    let mut editor = editor_with(&[
        &rect("o", 0.0, Some("a0"), ""),
        &rect("a", 100.0, Some("a1"), ab),
        &rect("b", 200.0, Some("a2"), ab),
        &rect("c", 300.0, Some("a3"), r#","groupIds":["g"]"#),
        &rect("z", 400.0, Some("a4"), ""),
    ]);
    select(&mut editor, 300.0, 0.0);
    tap(&mut editor, "finger", centre(300.0), 5.0);
    tap(&mut editor, "finger", centre(300.0), 5.2);
    select(&mut editor, pick, 10.0);
    editor
}

#[test]
fn inside_a_group_send_to_back_stays_in_the_group() {
    let mut editor = inside_group(false, 300.0);
    editor.move_in_z_order(ZOrder::ToBack);
    assert_eq!(ids(&editor), ["o", "c", "a", "b", "z"]);
    assert_indices_ascend(&editor);
}

#[test]
fn inside_a_group_bring_to_front_stays_in_the_group() {
    let mut editor = inside_group(false, 100.0);
    editor.move_in_z_order(ZOrder::ToFront);
    assert_eq!(ids(&editor), ["o", "b", "c", "a", "z"]);
    assert_indices_ascend(&editor);
}

#[test]
fn inside_a_group_one_place_passes_the_next_member_only() {
    let mut editor = inside_group(false, 100.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["o", "b", "a", "c", "z"]);
    editor.move_in_z_order(ZOrder::Backward);
    assert_eq!(ids(&editor), ["o", "a", "b", "c", "z"]);
    assert_indices_ascend(&editor);
}

#[test]
fn inside_a_group_a_member_does_not_leave_it() {
    let mut editor = inside_group(false, 100.0);
    editor.move_in_z_order(ZOrder::Backward);
    assert_eq!(ids(&editor), ["o", "a", "b", "c", "z"]);
    let mut editor = inside_group(false, 300.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["o", "a", "b", "c", "z"]);
}

#[test]
fn inside_a_group_an_inner_group_moves_as_a_whole() {
    let mut editor = inside_group(true, 100.0);
    editor.move_in_z_order(ZOrder::Forward);
    assert_eq!(ids(&editor), ["o", "c", "a", "b", "z"]);
    assert_indices_ascend(&editor);

    // And is passed as a whole.
    let mut editor = inside_group(true, 300.0);
    editor.move_in_z_order(ZOrder::Backward);
    assert_eq!(ids(&editor), ["o", "c", "a", "b", "z"]);
    assert_indices_ascend(&editor);
}

#[test]
fn inside_a_group_undo_restores_the_order_and_the_indices() {
    let mut editor = inside_group(false, 300.0);
    let (before_ids, before_indices) = (ids(&editor), indices(&editor));
    editor.move_in_z_order(ZOrder::ToBack);
    editor.undo();
    assert_eq!(ids(&editor), before_ids);
    assert_eq!(indices(&editor), before_indices);
}

#[test]
fn inside_a_group_one_place_passes_the_next_member_whatever_lies_between() {
    // `x` is no member of `g` but lies between two of them (Excalidraw
    // moves to the closest member regardless, `getTargetIndex`).
    let mut editor = editor_with(&[
        &rect("o", 0.0, Some("a0"), ""),
        &rect("a", 100.0, Some("a1"), r#","groupIds":["g"]"#),
        &rect("x", 200.0, Some("a2"), ""),
        &rect("c", 300.0, Some("a3"), r#","groupIds":["g"]"#),
        &rect("z", 400.0, Some("a4"), ""),
    ]);
    select(&mut editor, 300.0, 0.0);
    tap(&mut editor, "finger", centre(300.0), 5.0);
    tap(&mut editor, "finger", centre(300.0), 5.2);
    select(&mut editor, 100.0, 10.0);

    editor.move_in_z_order(ZOrder::Forward);

    assert_eq!(ids(&editor), ["o", "x", "c", "a", "z"]);
    assert_indices_ascend(&editor);
}
