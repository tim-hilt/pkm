//! Choosing a drawing Tool, or putting a pointer down with one, clears the
//! Selection and the group that was gone into (ADR-0006), as on
//! excalidraw.com's `App.setActiveTool` and `clearSelectionIfNotUsingSelection`.
//! Tested at the Editor seam with recorded sequences.

use pkm_drawing::{Command, Editor, PencilDoubleTap, PointerKind, Tool};

use crate::common;
use common::{editor_with, replay, tap};

fn rect(id: &str, x: f64, index: &str, groups: &str) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":"{index}",
        "groupIds":[{groups}],"seed":11,"version":3,"versionNonce":7}}"##
    )
}

/// Rectangles `a` and `b` in group `g`, and `c` alone, centred at x = 20,
/// 120 and 220 (y = 10), with `a` selected.
fn with_a_selected() -> Editor {
    let mut editor = editor_with(&[
        &rect("a", 0.0, "a0", "\"g\""),
        &rect("b", 100.0, "a1", "\"g\""),
        &rect("c", 200.0, "a2", ""),
    ]);
    tap(&mut editor, "mouse", (20.0, 10.0), 0.0);
    assert!(!editor.selection().is_empty());
    editor
}

const DRAWING_TOOLS: [Tool; 8] = [
    Tool::Hand,
    Tool::Rectangle,
    Tool::Diamond,
    Tool::Ellipse,
    Tool::Line,
    Tool::Arrow,
    Tool::Freedraw,
    Tool::Eraser,
];

#[test]
fn choosing_a_drawing_tool_clears_the_selection() {
    for tool in DRAWING_TOOLS.into_iter().chain([Tool::Text]) {
        let mut editor = with_a_selected();
        let revision = editor.overlay_revision();

        editor.set_tool(tool);

        assert!(editor.selection().is_empty(), "{tool:?}");
        // A long-press with a drawing Tool offers no Context menu.
        assert!(
            editor.long_press(20.0, 10.0, PointerKind::Mouse).is_none(),
            "{tool:?}"
        );
        assert!(editor.overlay_revision() > revision, "{tool:?}");
    }
}

#[test]
fn choosing_selection_keeps_the_selection() {
    let mut editor = with_a_selected();
    editor.set_tool(Tool::Selection);
    assert_eq!(editor.selection().len(), 2);
}

#[test]
fn a_shortcut_and_a_pencil_double_tap_clear_it_too() {
    let mut editor = with_a_selected();
    editor.perform(Command::SetTool(Tool::Rectangle));
    assert!(editor.selection().is_empty());

    let mut editor = with_a_selected();
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    assert_eq!(editor.tool(), Tool::Eraser);
    assert!(editor.selection().is_empty());
}

#[test]
fn choosing_a_drawing_tool_leaves_the_group_that_was_gone_into() {
    let mut editor = with_a_selected();
    // Double-tap on b goes into the group g: its elements select singly.
    tap(&mut editor, "finger", (120.0, 10.0), 5.0);
    tap(&mut editor, "finger", (120.0, 10.0), 5.2);
    tap(&mut editor, "mouse", (20.0, 10.0), 10.0);
    assert_eq!(editor.selection(), ["a"]);

    editor.set_tool(Tool::Freedraw);
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "mouse", (20.0, 10.0), 20.0);

    assert_eq!(editor.selection().len(), 2);
}

#[test]
fn drawing_then_returning_to_selection_selects_only_the_drawn_element() {
    let mut editor = with_a_selected();
    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "1 down 1 mouse 300 100\n1.1 move 1 mouse 350 150\n1.2 up 1 mouse 350 150",
    );

    assert_eq!(editor.tool(), Tool::Selection);
    assert_eq!(editor.selection().len(), 1);
    assert_ne!(editor.selection()[0], "a");
}

#[test]
fn undoing_the_first_element_drawn_after_a_tool_change_goes_back_to_an_empty_selection() {
    let mut editor = with_a_selected();
    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "1 down 1 mouse 300 100\n1.1 move 1 mouse 350 150\n1.2 up 1 mouse 350 150",
    );
    assert_eq!(editor.selection().len(), 1);

    editor.undo();

    assert!(editor.selection().is_empty());
}

#[test]
fn in_pencil_only_mode_a_finger_selects_and_the_selection_stays_until_the_pencil_touches_down() {
    let mut editor = editor_with(&[&rect("a", 0.0, "a0", ""), &rect("c", 200.0, "a2", "")]);
    tap(&mut editor, "pencil", (600.0, 600.0), 0.0);
    assert!(editor.pencil_only_drawing());
    editor.set_tool(Tool::Freedraw);

    tap(&mut editor, "finger", (20.0, 10.0), 1.0);
    assert_eq!(editor.selection(), ["a"]);

    // A finger drag pans and does not clear it.
    replay(
        &mut editor,
        "2 down 1 finger 400 400\n2.1 move 1 finger 420 410\n2.2 up 1 finger 420 410",
    );
    assert_eq!(editor.selection(), ["a"]);

    replay(
        &mut editor,
        "3 down 1 pencil 500 500\n3.1 move 1 pencil 520 520\n3.2 up 1 pencil 520 520",
    );
    assert!(editor.selection().is_empty());
}
