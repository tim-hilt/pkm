//! The hand Tool (ADR-0006), tested at the Editor seam: with it active, one
//! pointer of any kind drags the Canvas around. Expected viewports are worked
//! out by hand in the comments.

use pkm_core::{Editor, MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, Tool};

use PointerKind::{Finger, Mouse, Pencil};
use PointerPhase::{Cancel, Down, Move, Up};

fn send(editor: &mut Editor, id: u64, kind: PointerKind, phase: PointerPhase, x: f64, y: f64) {
    editor.pointer(PointerEvent {
        id,
        phase,
        x,
        y,
        kind,
        button: pkm_core::PointerButton::Primary,
        pressure: 0.5,
        timestamp: 0.0,
        modifiers: Modifiers::default(),
    });
}

fn hand_editor() -> Editor {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Hand);
    editor
}

fn origin(editor: &Editor) -> (f64, f64) {
    editor.viewport().origin()
}

#[test]
fn a_new_editor_starts_with_the_selection_tool() {
    assert_eq!(Editor::new().tool(), Tool::Selection);
}

#[test]
fn the_tool_can_be_switched() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Hand);
    assert_eq!(editor.tool(), Tool::Hand);
    editor.set_tool(Tool::Selection);
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn a_one_finger_drag_pans_so_the_content_follows_the_finger() {
    let mut editor = hand_editor();

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 130.0, 110.0);

    // The content moved 30 right and 10 down, so the Scene point at the
    // Canvas's top-left corner is 30 left and 10 above the origin.
    assert_eq!(origin(&editor), (-30.0, -10.0));
    assert_eq!(editor.viewport().zoom(), 1.0);
}

#[test]
fn a_drag_keeps_following_across_many_moves() {
    let mut editor = hand_editor();

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 110.0, 100.0);
    send(&mut editor, 1, Finger, Move, 90.0, 120.0);
    send(&mut editor, 1, Finger, Move, 100.0, 100.0);

    // Back where the finger started: nothing moved overall.
    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn a_mouse_drag_pans() {
    let mut editor = hand_editor();

    send(&mut editor, 0, Mouse, Down, 50.0, 50.0);
    send(&mut editor, 0, Mouse, Move, 20.0, 80.0);
    send(&mut editor, 0, Mouse, Up, 20.0, 80.0);

    assert_eq!(origin(&editor), (30.0, -30.0));
}

#[test]
fn a_pencil_drag_pans() {
    let mut editor = hand_editor();

    send(&mut editor, 1, Pencil, Down, 50.0, 50.0);
    send(&mut editor, 1, Pencil, Move, 60.0, 50.0);

    assert_eq!(origin(&editor), (-10.0, 0.0));
}

#[test]
fn a_drag_pans_less_of_the_scene_when_zoomed_in() {
    let mut editor = hand_editor();
    // Zoom 2 around the top-left corner leaves the origin at (0, 0).
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 1.0,
    });

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 140.0, 100.0);

    // 40 view points are 20 Scene units at zoom 2.
    assert_eq!(origin(&editor), (-20.0, 0.0));
}

#[test]
fn without_the_hand_tool_a_single_pointer_does_not_pan() {
    let mut editor = Editor::new();

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 150.0, 100.0);
    send(&mut editor, 1, Finger, Up, 150.0, 100.0);
    send(&mut editor, 0, Mouse, Down, 100.0, 100.0);
    send(&mut editor, 0, Mouse, Move, 150.0, 100.0);

    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn moving_without_pressing_does_not_pan() {
    let mut editor = hand_editor();

    send(&mut editor, 0, Mouse, Move, 150.0, 100.0);
    send(&mut editor, 0, Mouse, Move, 200.0, 100.0);

    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn a_released_pointer_stops_panning_and_the_next_drag_starts_where_it_lands() {
    let mut editor = hand_editor();

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 110.0, 100.0);
    send(&mut editor, 1, Finger, Up, 110.0, 100.0);
    send(&mut editor, 1, Finger, Move, 500.0, 500.0);
    assert_eq!(origin(&editor), (-10.0, 0.0));

    send(&mut editor, 2, Finger, Down, 300.0, 300.0);
    send(&mut editor, 2, Finger, Move, 310.0, 300.0);
    assert_eq!(origin(&editor), (-20.0, 0.0));
}

#[test]
fn a_cancelled_touch_ends_the_drag() {
    let mut editor = hand_editor();

    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Cancel, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 150.0, 100.0);

    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn a_second_finger_turns_the_drag_into_a_two_finger_gesture() {
    let mut editor = hand_editor();
    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 120.0, 100.0);
    assert_eq!(origin(&editor), (-20.0, 0.0));

    // Landing the second finger must not move the view ...
    send(&mut editor, 2, Finger, Down, 200.0, 100.0);
    assert_eq!(origin(&editor), (-20.0, 0.0));

    // ... and both moving together by 10 pans by 10 once, not twice.
    send(&mut editor, 1, Finger, Move, 130.0, 100.0);
    send(&mut editor, 2, Finger, Move, 210.0, 100.0);
    assert_eq!(origin(&editor), (-30.0, 0.0));
}

#[test]
fn the_finger_left_after_a_two_finger_gesture_does_not_pan() {
    let mut editor = hand_editor();
    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 2, Finger, Down, 200.0, 100.0);
    send(&mut editor, 2, Finger, Up, 200.0, 100.0);

    send(&mut editor, 1, Finger, Move, 150.0, 100.0);

    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn a_finger_landing_during_a_mouse_drag_does_not_hijack_it() {
    let mut editor = hand_editor();
    send(&mut editor, 0, Mouse, Down, 100.0, 100.0);
    send(&mut editor, 5, Finger, Down, 300.0, 300.0);

    send(&mut editor, 0, Mouse, Move, 90.0, 100.0);
    send(&mut editor, 5, Finger, Move, 400.0, 400.0);

    assert_eq!(origin(&editor), (10.0, 0.0));
}

#[test]
fn switching_away_from_the_hand_tool_ends_a_drag_in_progress() {
    let mut editor = hand_editor();
    send(&mut editor, 1, Finger, Down, 100.0, 100.0);
    send(&mut editor, 1, Finger, Move, 110.0, 100.0);

    editor.set_tool(Tool::Selection);
    send(&mut editor, 1, Finger, Move, 150.0, 100.0);
    // Switching back must not resume the old drag either.
    editor.set_tool(Tool::Hand);
    send(&mut editor, 1, Finger, Move, 200.0, 100.0);

    assert_eq!(origin(&editor), (-10.0, 0.0));
}
