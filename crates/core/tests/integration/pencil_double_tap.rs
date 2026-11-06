//! Pencil double-tap (ADR-0006), tested at the Editor seam: the client forwards
//! the double-tap together with what the user set for it in the system
//! settings, and the core changes the Tool. Tool, Tool lock and saved Drawing
//! are read back.

use excalidraw_native_core::{Editor, PencilDoubleTap, Tool};

use crate::common;
use common::{editor_with, replay, saved_elements};

/// A filled rectangle at (100, 100), 100 x 50.
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

// --- Switch to the eraser ---------------------------------------------------------

#[test]
fn a_double_tap_switches_to_the_eraser_and_a_second_one_back() {
    for tool in [Tool::Selection, Tool::Freedraw, Tool::Rectangle, Tool::Hand] {
        let mut editor = Editor::new();
        editor.set_tool(tool);

        editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
        assert_eq!(editor.tool(), Tool::Eraser, "{tool:?}");

        editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
        assert_eq!(editor.tool(), tool, "{tool:?}: back");
    }
}

#[test]
fn the_eraser_after_a_double_tap_erases_with_the_pencil() {
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_tool(Tool::Freedraw);

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    replay(
        &mut editor,
        "1.0 down 1 pencil 150 125\n1.1 up 1 pencil 150 125",
    );

    let erased = saved_elements(&editor)
        .iter()
        .any(|e| e.get("isDeleted").and_then(|v| v.as_bool()) == Some(true));
    assert!(erased);
}

#[test]
fn a_double_tap_back_from_the_eraser_goes_to_the_tool_the_user_had_chosen_last() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Freedraw);
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

    // The user picks the eraser's way out themselves, then another Tool.
    editor.set_tool(Tool::Ellipse);
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    assert_eq!(editor.tool(), Tool::Eraser);
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    assert_eq!(editor.tool(), Tool::Ellipse);
}

#[test]
fn the_eraser_chosen_in_the_toolbar_goes_back_to_the_tool_before_it() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Line);
    editor.set_tool(Tool::Eraser);

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

    assert_eq!(editor.tool(), Tool::Line);
}

#[test]
fn a_double_tap_on_the_eraser_with_nothing_before_it_goes_to_selection() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Eraser);

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

    assert_eq!(editor.tool(), Tool::Selection);
}

// --- Switch to the previous Tool ------------------------------------------------------

#[test]
fn switch_previous_toggles_between_the_current_and_the_previous_tool() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Freedraw);
    editor.set_tool(Tool::Rectangle);

    editor.pencil_double_tap(PencilDoubleTap::SwitchPrevious);
    assert_eq!(editor.tool(), Tool::Freedraw);

    editor.pencil_double_tap(PencilDoubleTap::SwitchPrevious);
    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn switch_previous_counts_the_return_to_selection_after_drawing_a_shape() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    replay(
        &mut editor,
        "1.0 down 1 pencil 300 300\n1.1 move 1 pencil 350 330\n1.2 up 1 pencil 400 360",
    );
    assert_eq!(editor.tool(), Tool::Selection);

    // Back to the Tool the shape was drawn with.
    editor.pencil_double_tap(PencilDoubleTap::SwitchPrevious);
    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn switch_previous_does_nothing_when_there_is_no_previous_tool() {
    let mut editor = Editor::new();

    editor.pencil_double_tap(PencilDoubleTap::SwitchPrevious);

    assert_eq!(editor.tool(), Tool::Selection);
}

// --- Respecting the system setting ----------------------------------------------------------

#[test]
fn ignore_leaves_the_tool_alone() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Freedraw);

    editor.pencil_double_tap(PencilDoubleTap::Ignore);

    assert_eq!(editor.tool(), Tool::Freedraw);
}

#[test]
fn ignore_does_not_disturb_a_later_switch_back() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Freedraw);
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

    editor.pencil_double_tap(PencilDoubleTap::Ignore);
    assert_eq!(editor.tool(), Tool::Eraser);

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    assert_eq!(editor.tool(), Tool::Freedraw);
}

// --- Interplay -----------------------------------------------------------------------------------

#[test]
fn the_tool_lock_is_left_as_it_is() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    editor.set_tool_lock(true);

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

    assert!(editor.tool_lock());
    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn a_double_tap_in_the_middle_of_a_stroke_ends_it_cleanly() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Freedraw);
    replay(
        &mut editor,
        "1.0 down 1 pencil 300 300\n1.1 move 1 pencil 340 320",
    );

    editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);
    replay(&mut editor, "1.2 up 1 pencil 380 340");

    assert_eq!(editor.tool(), Tool::Eraser);
    assert!(saved_elements(&editor).is_empty());
}

#[test]
fn it_works_with_pencil_only_drawing_on_and_off() {
    for on in [true, false] {
        let mut editor = Editor::new();
        editor.set_pencil_only_drawing(on);

        editor.pencil_double_tap(PencilDoubleTap::SwitchEraser);

        assert_eq!(editor.tool(), Tool::Eraser, "pencil-only {on}");
        assert_eq!(editor.pencil_only_drawing(), on);
    }
}
