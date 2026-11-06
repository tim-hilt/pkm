//! Pan and zoom (ADR-0006), tested at the Editor seam: raw input goes in, the
//! viewport comes out. Pointer sequences are written as recorded event lines:
//!
//! ```text
//! <down|move|up|cancel> <pointer id> <finger|pencil|mouse> <x> <y>
//! ```
//!
//! Expected viewports are worked out by hand in the comments, not computed
//! with the formulas of the code under test.

use excalidraw_native_core::{
    Editor, MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent,
};

const EPSILON: f64 = 1e-9;

/// Feeds a recorded pointer sequence to the editor, 10 ms per event.
fn replay(editor: &mut Editor, recording: &str) {
    for (n, line) in recording
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [phase, id, kind, x, y] = f[..] else {
            panic!("bad recorded event: {line}")
        };
        editor.pointer(PointerEvent {
            id: id.parse().unwrap(),
            phase: match phase {
                "down" => PointerPhase::Down,
                "move" => PointerPhase::Move,
                "up" => PointerPhase::Up,
                "cancel" => PointerPhase::Cancel,
                _ => panic!("unknown phase in: {line}"),
            },
            x: x.parse().unwrap(),
            y: y.parse().unwrap(),
            kind: match kind {
                "finger" => PointerKind::Finger,
                "pencil" => PointerKind::Pencil,
                "mouse" => PointerKind::Mouse,
                _ => panic!("unknown kind in: {line}"),
            },
            button: excalidraw_native_core::PointerButton::Primary,
            pressure: 0.5,
            timestamp: n as f64 * 0.01,
            modifiers: Modifiers::default(),
        });
    }
}

fn scroll(editor: &mut Editor, dx: f64, dy: f64, modifiers: Modifiers) {
    editor.scroll(ScrollEvent {
        x: 100.0,
        y: 100.0,
        dx,
        dy,
        modifiers,
    });
}

fn origin(editor: &Editor) -> (f64, f64) {
    editor.viewport().origin()
}

fn zoom(editor: &Editor) -> f64 {
    editor.viewport().zoom()
}

#[track_caller]
fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < EPSILON && (actual.1 - expected.1).abs() < EPSILON,
        "{actual:?} != {expected:?}"
    );
}

const CTRL: Modifiers = Modifiers { ctrl: true, ..NONE };
const META: Modifiers = Modifiers { meta: true, ..NONE };
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..NONE
};
const CTRL_SHIFT: Modifiers = Modifiers {
    ctrl: true,
    shift: true,
    ..NONE
};
const NONE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: false,
};

#[test]
fn a_new_editor_shows_the_scene_origin_unzoomed() {
    let editor = Editor::new();

    assert_eq!(origin(&editor), (0.0, 0.0));
    assert_eq!(zoom(&editor), 1.0);
}

// --- Mac scroll wheel and trackpad -------------------------------------

#[test]
fn scrolling_moves_the_content_with_the_fingers() {
    let mut editor = Editor::new();

    // Content moves 10 right, 20 down; so the view looks 10 left, 20 up.
    scroll(&mut editor, 10.0, 20.0, NONE);

    assert_close(origin(&editor), (-10.0, -20.0));
    assert_eq!(zoom(&editor), 1.0);
}

#[test]
fn scrolling_covers_less_of_the_scene_when_zoomed_in() {
    let mut editor = Editor::new();
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 1.0,
    }); // zoom 2

    scroll(&mut editor, 10.0, 20.0, NONE);

    assert_close(origin(&editor), (-5.0, -10.0));
}

#[test]
fn shift_scrolls_horizontally_and_ctrl_shift_vertically() {
    let mut editor = Editor::new();

    // Mac turns shift+wheel into a horizontal delta; either axis counts.
    scroll(&mut editor, 0.0, 30.0, SHIFT);
    assert_close(origin(&editor), (-30.0, 0.0));
    scroll(&mut editor, 7.0, 0.0, SHIFT);
    assert_close(origin(&editor), (-37.0, 0.0));
    scroll(&mut editor, 0.0, 10.0, CTRL_SHIFT);
    assert_close(origin(&editor), (-37.0, -10.0));
}

#[test]
fn ctrl_or_cmd_scroll_zooms_in_excalidraws_steps() {
    for modifiers in [CTRL, META] {
        let mut editor = Editor::new();

        scroll(&mut editor, 0.0, 5.0, modifiers); // content down: zoom in
        assert!((zoom(&editor) - 1.05).abs() < EPSILON);

        let mut editor = Editor::new();
        scroll(&mut editor, 0.0, -5.0, modifiers);
        assert!((zoom(&editor) - 0.95).abs() < EPSILON);
    }
}

#[test]
fn one_scroll_event_zooms_by_at_most_a_tenth() {
    let mut editor = Editor::new();

    scroll(&mut editor, 0.0, -500.0, CTRL);

    assert!((zoom(&editor) - 0.9).abs() < EPSILON);
}

#[test]
fn zooming_in_is_faster_when_already_zoomed_in() {
    let mut editor = Editor::new();
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 9.0,
    }); // zoom 10

    // Excalidraw: a tick of 20 counts as 10/100 = 0.1, plus the extra
    // log10(10) * 1 = 1 for zooming in beyond 100%: 10 + 0.1 + 1 = 11.1.
    scroll(&mut editor, 0.0, 20.0, CTRL);

    assert!((zoom(&editor) - 11.1).abs() < EPSILON);
}

#[test]
fn a_zoom_scroll_keeps_the_scene_under_the_pointer_in_place() {
    let mut editor = Editor::new();
    scroll(&mut editor, 0.0, 5.0, CTRL); // zoom 1.05 about (100, 100)

    // Scene point (100, 100) was under the pointer and must still be.
    // origin = 100 - 100 / 1.05
    let expected = 100.0 - 100.0 / 1.05;
    assert_close(origin(&editor), (expected, expected));
}

#[test]
fn horizontal_only_scroll_with_ctrl_pans_instead_of_zooming() {
    let mut editor = Editor::new();

    scroll(&mut editor, 12.0, 0.0, CTRL);

    assert_eq!(zoom(&editor), 1.0);
    assert_close(origin(&editor), (-12.0, 0.0));
}

#[test]
fn trackpad_pinch_zooms_around_the_pinch_centre() {
    let mut editor = Editor::new();

    editor.magnify(MagnifyEvent {
        x: 100.0,
        y: 50.0,
        magnification: 0.5,
    });

    // Zoom 1.5; scene point (100, 50) stays put: 100 - 100/1.5, 50 - 50/1.5.
    assert!((zoom(&editor) - 1.5).abs() < EPSILON);
    assert_close(origin(&editor), (100.0 / 3.0, 50.0 / 3.0));
}

#[test]
fn every_zoom_is_rounded_to_six_decimals_like_excalidraws() {
    let mut editor = Editor::new();
    editor.magnify(MagnifyEvent {
        x: 0.0,
        y: 0.0,
        magnification: 0.123_456_789,
    });
    assert_eq!(zoom(&editor), 1.123_457);

    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    scroll(&mut editor, 0.0, 0.123_456_789, ctrl);
    // 1.123457 + 0.123456789 / 100 + log10(1.123457) * 0.123456789 / 20.
    assert_eq!(zoom(&editor), 1.125004);
}

// --- Zoom limits -------------------------------------------------------

#[test]
fn zoom_stops_at_excalidraws_limits() {
    let mut editor = Editor::new();
    for _ in 0..100 {
        editor.magnify(MagnifyEvent {
            x: 0.0,
            y: 0.0,
            magnification: -0.5,
        });
    }
    assert_eq!(zoom(&editor), 0.1);

    for _ in 0..100 {
        editor.magnify(MagnifyEvent {
            x: 0.0,
            y: 0.0,
            magnification: 1.0,
        });
    }
    assert_eq!(zoom(&editor), 30.0);
}

#[test]
fn zooming_at_a_limit_leaves_the_viewport_alone() {
    let mut editor = Editor::new();
    for _ in 0..100 {
        scroll(&mut editor, 0.0, -50.0, CTRL);
    }
    let at_limit = editor.viewport();

    scroll(&mut editor, 0.0, -50.0, CTRL);

    assert_eq!(editor.viewport(), at_limit);
}

// --- Two-finger touch --------------------------------------------------

#[test]
fn one_finger_does_not_navigate() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 100 100
         move 1 finger 160 130
         up 1 finger 160 130",
    );

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn two_fingers_moving_together_pan() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 130 110
         move 2 finger 230 110",
    );

    // Content follows the fingers 30 right and 10 down.
    assert_close(origin(&editor), (-30.0, -10.0));
    assert_eq!(zoom(&editor), 1.0);
}

#[test]
fn spreading_two_fingers_zooms_around_their_centre() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 50 100
         move 2 finger 250 100",
    );

    // Distance 100 -> 200: zoom 2. The centre (150, 100) did not move, so the
    // scene point under it, (150, 100), stays: origin = 150 - 150/2, 100 - 100/2.
    assert_eq!(zoom(&editor), 2.0);
    assert_close(origin(&editor), (75.0, 50.0));
}

#[test]
fn pinch_and_pan_combine() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 150 150
         move 2 finger 250 150",
    );
    // Pure pan by (50, 50) first.
    assert_close(origin(&editor), (-50.0, -50.0));

    replay(
        &mut editor,
        "move 1 finger 100 150
         move 2 finger 300 150",
    );
    // Distance 100 -> 200 about the centre (200, 150). The gesture grabbed the
    // scene point (150, 100) and keeps it under the centre:
    // origin = 150 - 200/2, 100 - 150/2.
    assert_eq!(zoom(&editor), 2.0);
    assert_close(origin(&editor), (50.0, 25.0));
}

#[test]
fn pinch_zoom_is_clamped_and_does_not_accumulate_overshoot() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 0 100
         down 2 finger 10 100
         move 2 finger 2000 100",
    );
    assert_eq!(zoom(&editor), 30.0, "10 -> 2000 is far beyond the limit");

    // Coming back to 3x the start distance zooms to 3, not to 30/200.
    replay(&mut editor, "move 2 finger 30 100");
    assert_eq!(zoom(&editor), 3.0);
}

#[test]
fn lifting_one_finger_ends_the_gesture() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 2 finger 220 100
         up 1 finger 100 100",
    );
    let after_gesture = editor.viewport();

    replay(&mut editor, "move 2 finger 400 300");

    assert_eq!(editor.viewport(), after_gesture);
}

#[test]
fn a_new_gesture_continues_from_the_current_viewport() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 110 100
         move 2 finger 210 100
         up 1 finger 110 100
         up 2 finger 210 100
         down 3 finger 100 100
         down 4 finger 200 100
         move 3 finger 120 100
         move 4 finger 220 100",
    );

    assert_close(origin(&editor), (-30.0, 0.0));
}

// Three fingers are the system's undo and redo gestures (swipe, double-tap):
// they must not navigate.

#[test]
fn three_fingers_do_not_navigate() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         move 1 finger 0 100
         move 2 finger 100 100
         move 3 finger 200 100
         up 1 finger 0 100
         up 2 finger 100 100
         up 3 finger 200 100",
    );

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn a_third_finger_takes_back_what_the_first_two_moved() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 50 120
         move 2 finger 250 120",
    );
    assert_ne!(editor.viewport(), Editor::new().viewport());

    replay(&mut editor, "down 3 finger 300 100");

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn after_three_fingers_none_navigate_until_all_have_lifted() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         up 3 finger 300 100
         move 1 finger 110 100
         move 2 finger 210 100
         up 1 finger 110 100
         move 2 finger 400 300",
    );
    assert_eq!(editor.viewport(), Editor::new().viewport());

    replay(
        &mut editor,
        "up 2 finger 400 300
         down 4 finger 100 100
         down 5 finger 200 100
         move 4 finger 110 100
         move 5 finger 210 100",
    );
    assert_close(origin(&editor), (-10.0, 0.0));
}

#[test]
fn a_third_finger_ends_a_hand_tool_drag() {
    let mut editor = Editor::new();
    editor.set_tool(excalidraw_native_core::Tool::Hand);
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         up 3 finger 300 100
         up 2 finger 200 100
         move 1 finger 150 150",
    );

    assert_eq!(editor.viewport(), Editor::new().viewport());
}

#[test]
fn a_cancelled_touch_ends_the_gesture() {
    let mut editor = Editor::new();
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         cancel 2 finger 200 100",
    );
    let before = editor.viewport();

    replay(&mut editor, "move 1 finger 300 300");

    assert_eq!(editor.viewport(), before);
}

#[test]
fn only_fingers_navigate() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 pencil 200 100
         move 2 pencil 400 100
         move 1 finger 0 0
         up 2 pencil 400 100",
    );
    assert_eq!(
        editor.viewport(),
        Editor::new().viewport(),
        "finger + pencil"
    );

    replay(
        &mut editor,
        "down 3 mouse 10 10
         move 3 mouse 50 50
         up 3 mouse 50 50",
    );
    assert_eq!(editor.viewport(), Editor::new().viewport(), "mouse drag");
}

#[test]
fn a_lost_finger_does_not_poison_later_gestures() {
    let mut editor = Editor::new();
    // Finger 1 never reports its end (e.g. the app lost the touch).
    replay(&mut editor, "down 1 finger 0 0");
    replay(&mut editor, "down 1 finger 100 100");
    replay(&mut editor, "down 2 finger 200 100");
    replay(
        &mut editor,
        "move 1 finger 110 100
         move 2 finger 210 100",
    );

    assert_close(origin(&editor), (-10.0, 0.0));
}

// --- Geometry does not depend on the viewport ---------------------------

#[test]
fn navigating_leaves_the_scenes_geometry_untouched() {
    let mut editor = Editor::new();
    editor
        .load(
            br#"{"type":"excalidraw","elements":[
                {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40}]}"#,
        )
        .unwrap();
    let before = editor.geometry();

    scroll(&mut editor, 40.0, 50.0, NONE);
    scroll(&mut editor, 0.0, 5.0, CTRL);
    replay(
        &mut editor,
        "down 1 finger 100 100
         down 2 finger 200 100
         move 2 finger 300 100",
    );
    let after = editor.geometry();

    assert_ne!(editor.viewport(), Editor::new().viewport());
    assert_eq!(before.parts(), after.parts());
    assert_eq!(before.verbs(), after.verbs());
    assert_eq!(before.coords(), after.coords());
}
