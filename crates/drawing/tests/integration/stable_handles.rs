//! Element handles are stable (see `Geometry::generation`), tested at the
//! `Editor::geometry` seam: whatever is added, removed, reordered or brought
//! back, the Elements that stay keep their handle and version and the
//! generation stays; loading a Drawing changes it.

use std::collections::BTreeMap;

use pkm_drawing::{Editor, Tool, ZOrder};

use crate::common::{editor_with, replay, tap};

fn rect(id: &str, x: f64) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":null,
        "seed":11,"version":3,"versionNonce":7}}"##
    )
}

fn three() -> Editor {
    editor_with(&[&rect("a", 0.0), &rect("b", 100.0), &rect("c", 200.0)])
}

/// Version of every handle that is drawn.
fn drawn(editor: &Editor) -> BTreeMap<u32, u32> {
    let mut drawn = BTreeMap::new();
    for part in editor.geometry().parts() {
        drawn.insert(part.element, part.version);
    }
    drawn
}

fn generation(editor: &Editor) -> u64 {
    editor.geometry().generation()
}

fn select_b(editor: &mut Editor) {
    tap(editor, "mouse", (120.0, 10.0), 0.0);
}

fn draw_rectangle_at_top(editor: &mut Editor) {
    editor.set_tool(Tool::Rectangle);
    replay(
        editor,
        "1.0 down 1 mouse 300 300 \n1.1 move 1 mouse 340 330\n1.2 up 1 mouse 340 330",
    );
}

/// Asserts that `after` still draws everything of `before` as it was.
fn assert_kept(before: &BTreeMap<u32, u32>, after: &BTreeMap<u32, u32>) {
    for (handle, version) in before {
        assert_eq!(after.get(handle), Some(version), "handle {handle}");
    }
}

#[test]
fn drawing_adds_a_handle_and_leaves_the_others() {
    let mut editor = three();
    let (before, gen_before) = (drawn(&editor), generation(&editor));

    draw_rectangle_at_top(&mut editor);

    let after = drawn(&editor);
    assert_eq!(generation(&editor), gen_before);
    assert_kept(&before, &after);
    assert_eq!(after.len(), before.len() + 1);
}

#[test]
fn deleting_undoing_and_redoing_keep_every_other_handle() {
    let mut editor = three();
    let before = drawn(&editor);
    let gen_before = generation(&editor);
    select_b(&mut editor);

    editor.delete_selection();
    let deleted = drawn(&editor);
    assert_eq!(deleted.len(), 2);
    assert_kept(&deleted, &before);

    editor.undo();
    // The Element comes back with its old handle and version.
    assert_eq!(
        drawn(&editor).keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    assert_kept(&deleted, &drawn(&editor));

    editor.redo();
    assert_eq!(drawn(&editor), deleted);
    assert_eq!(generation(&editor), gen_before);
}

#[test]
fn undoing_a_drawing_and_redoing_it_gives_it_the_same_handle() {
    let mut editor = three();
    draw_rectangle_at_top(&mut editor);
    let drawn_with = drawn(&editor);
    let gen_before = generation(&editor);

    editor.undo();
    assert_eq!(drawn(&editor).len(), 3);
    editor.redo();

    assert_eq!(
        drawn(&editor).keys().collect::<Vec<_>>(),
        drawn_with.keys().collect::<Vec<_>>()
    );
    assert_eq!(generation(&editor), gen_before);
}

#[test]
fn pasting_and_duplicating_leave_the_handles_of_the_others() {
    let mut editor = three();
    select_b(&mut editor);
    let before = drawn(&editor);
    let gen_before = generation(&editor);
    let copied = editor.copy_selection().unwrap();

    editor.duplicate();
    editor.paste(&copied);

    let after = drawn(&editor);
    assert_eq!(after.len(), 5);
    assert_kept(&before, &after);
    assert_eq!(generation(&editor), gen_before);
}

#[test]
fn z_order_changes_leave_the_handles() {
    let mut editor = three();
    select_b(&mut editor);
    let before = drawn(&editor);
    let gen_before = generation(&editor);

    editor.move_in_z_order(ZOrder::ToBack);

    // The moved Element gets a new index, so a new version.
    assert_eq!(
        drawn(&editor).keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    assert_eq!(generation(&editor), gen_before);
}

#[test]
fn loading_a_drawing_changes_the_generation() {
    let mut editor = three();
    let before = generation(&editor);
    let saved = editor.save();

    editor.load(&saved).unwrap();

    assert_ne!(generation(&editor), before);
}
