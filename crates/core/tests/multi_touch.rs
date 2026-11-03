//! Two-finger gestures never disturb Selection or Tool (ADR-0006), tested at
//! the Editor seam with recorded pointer sequences (see `select.rs` for their
//! format): pointer events in, Selection, Tool, saved Drawing, undo history
//! and viewport out.
//!
//! Every test starts some single-finger action, lands a second finger in the
//! middle of it, and then checks that what the action had begun is gone
//! without a trace while the two fingers navigated.

use workingtitle_core::{Editor, Tool};

mod common;
use common::{editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50. Its transform handles are
/// centred on (96, 96), (204, 96), (204, 154) and (96, 154), and the rotation
/// handle at (150, 80).
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// A second rectangle far to the right, at (400, 100).
const OTHER: &str = r##"{"id":"b","type":"rectangle","x":400,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":9}"##;

/// An editor with the rectangle `a` selected.
fn selected() -> Editor {
    let mut editor = editor_with(&[RECTANGLE, OTHER]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

/// Everything a two-finger gesture must leave as it found it.
#[derive(Debug, PartialEq)]
struct Untouched {
    /// The saved Drawing, but for the versions of its Elements: cancelling an
    /// edit puts the Element back but never reuses a version, so that what
    /// clients cached for the abandoned edit is never mistaken for it.
    drawing: String,
    selection: Vec<String>,
    tool: Tool,
    can_undo: bool,
    can_redo: bool,
}

impl Untouched {
    fn of(editor: &Editor) -> Self {
        Self {
            drawing: String::from_utf8(editor.save())
                .unwrap()
                .lines()
                .filter(|line| !line.trim_start().starts_with("\"version"))
                .collect::<Vec<_>>()
                .join("\n"),
            selection: editor.selection().to_vec(),
            tool: editor.tool(),
            can_undo: editor.can_undo(),
            can_redo: editor.can_redo(),
        }
    }
}

/// How the two fingers end: the one that landed second lifts first, or the
/// other way round.
const LIFT_SECOND_FIRST: &str = "2.00 up 2 finger 560 500\n2.05 up 1 finger {first}";
const LIFT_FIRST_FIRST: &str = "2.00 up 1 finger {first}\n2.05 up 2 finger 560 500";

/// Starts the single-finger action `begin` (events of finger 1, which end
/// with it down at `first`), lands a second finger and pinches it out, and
/// lifts both in either order. The Drawing, Selection, Tool and history must
/// be as before, and the viewport must have followed the pinch.
fn assert_pinch_disturbs_nothing(begin: &str, first: &str) {
    for lift in [LIFT_SECOND_FIRST, LIFT_FIRST_FIRST] {
        let mut editor = selected();
        let before = Untouched::of(&editor);
        let zoom = editor.viewport().zoom();

        replay(&mut editor, begin);
        replay(
            &mut editor,
            "1.50 down 2 finger 520 500
             1.60 move 2 finger 560 500
             1.70 move 2 finger 600 500",
        );
        replay(&mut editor, &lift.replace("{first}", first));

        assert_eq!(Untouched::of(&editor), before, "{lift}");
        assert!(editor.viewport().zoom() > zoom, "the fingers pinched");
    }
}

#[test]
fn a_second_finger_mid_move_leaves_no_partial_move() {
    assert_pinch_disturbs_nothing(
        "1.00 down 1 finger 150 125
         1.10 move 1 finger 200 125
         1.20 move 1 finger 250 140",
        "250 140",
    );
}

#[test]
fn a_second_finger_mid_move_of_an_unselected_element_leaves_it_unselected() {
    assert_pinch_disturbs_nothing(
        "1.00 down 1 finger 450 125
         1.10 move 1 finger 480 125
         1.20 move 1 finger 500 140",
        "500 140",
    );
}

#[test]
fn a_second_finger_mid_resize_leaves_no_partial_resize() {
    assert_pinch_disturbs_nothing(
        "1.00 down 1 finger 206 156
         1.10 move 1 finger 230 180
         1.20 move 1 finger 256 206",
        "256 206",
    );
}

#[test]
fn a_second_finger_mid_rotation_leaves_no_partial_rotation() {
    assert_pinch_disturbs_nothing(
        "1.00 down 1 finger 150 80
         1.10 move 1 finger 200 90
         1.20 move 1 finger 260 125",
        "260 125",
    );
}

#[test]
fn a_second_finger_mid_box_select_restores_the_selection() {
    assert_pinch_disturbs_nothing(
        "1.00 down 1 finger 350 50
         1.10 move 1 finger 450 100
         1.20 move 1 finger 520 200",
        "520 200",
    );
}

#[test]
fn a_second_finger_mid_draw_leaves_no_partial_element() {
    for tool in [Tool::Rectangle, Tool::Diamond, Tool::Ellipse] {
        let mut editor = selected();
        editor.set_tool(tool);
        let before = Untouched::of(&editor);

        replay(
            &mut editor,
            "1.00 down 1 finger 300 300
             1.10 move 1 finger 320 320
             1.20 move 1 finger 340 340
             1.50 down 2 finger 520 500
             1.60 move 2 finger 600 500
             2.00 up 2 finger 600 500
             2.05 up 1 finger 340 340",
        );

        assert_eq!(Untouched::of(&editor), before, "{tool:?}");
        assert_eq!(editor.element_count(), 2, "{tool:?}");
    }
}

#[test]
fn a_pinch_in_the_middle_of_a_hand_drag_keeps_the_tool() {
    let mut editor = selected();
    editor.set_tool(Tool::Hand);

    replay(
        &mut editor,
        "1.00 down 1 finger 300 300
         1.10 move 1 finger 320 300
         1.50 down 2 finger 520 500
         1.60 move 2 finger 600 500
         2.00 up 2 finger 600 500
         2.05 up 1 finger 320 300",
    );

    assert_eq!(editor.tool(), Tool::Hand);
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_pinch_keeps_a_locked_shape_tool() {
    let mut editor = selected();
    editor.set_tool_lock(true);
    editor.set_tool(Tool::Ellipse);

    replay(
        &mut editor,
        "1.00 down 1 finger 300 300
         1.10 down 2 finger 400 300
         1.20 move 2 finger 500 300
         1.30 up 2 finger 500 300
         1.40 up 1 finger 300 300",
    );

    assert_eq!(editor.tool(), Tool::Ellipse);
    assert!(editor.tool_lock());
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn the_finger_left_after_a_pinch_never_acts_on_what_it_touches() {
    // Each finger lands on something it could act on, then the other lifts,
    // and the one left drags over it.
    let drags = [
        ("on a handle", "206 156", "300 220"),
        ("on the rotation handle", "150 80", "260 125"),
        ("on the selected element", "150 125", "250 140"),
        ("on an unselected element", "450 125", "480 160"),
        ("on empty canvas", "300 300", "420 220"),
    ];
    for (what, from, to) in drags {
        let mut editor = selected();
        let before = Untouched::of(&editor);

        replay(
            &mut editor,
            &format!(
                "1.00 down 1 finger {from}
                 1.10 down 2 finger 700 700
                 1.20 move 2 finger 720 700
                 1.30 up 2 finger 720 700
                 1.40 move 1 finger {to}
                 1.50 move 1 finger {from}
                 1.60 up 1 finger {from}"
            ),
        );

        assert_eq!(Untouched::of(&editor), before, "{what}");
    }
}

#[test]
fn lifting_after_a_pinch_never_counts_as_a_tap() {
    // A finger lands on empty canvas, a second one on an element; both lift
    // at once without moving, however briefly they were down.
    let mut editor = selected();
    let before = Untouched::of(&editor);

    replay(
        &mut editor,
        "1.00 down 1 finger 300 300
         1.01 down 2 finger 450 125
         1.02 up 2 finger 450 125
         1.03 up 1 finger 300 300",
    );
    assert_eq!(Untouched::of(&editor), before);

    // The other way round: first on an element, then on empty canvas.
    replay(
        &mut editor,
        "2.00 down 1 finger 450 125
         2.01 down 2 finger 300 300
         2.02 up 1 finger 450 125
         2.03 up 2 finger 300 300",
    );
    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn a_third_finger_never_taps_either() {
    let mut editor = selected();
    let before = Untouched::of(&editor);

    replay(
        &mut editor,
        "1.00 down 1 finger 300 300
         1.01 down 2 finger 400 300
         1.02 down 3 finger 450 125
         1.03 up 3 finger 450 125
         1.04 up 2 finger 400 300
         1.05 up 1 finger 300 300",
    );

    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn editing_works_normally_after_a_pinch() {
    let mut editor = selected();
    replay(
        &mut editor,
        "1.00 down 1 finger 300 300
         1.10 down 2 finger 400 300
         1.20 move 2 finger 500 300
         1.30 up 2 finger 500 300
         1.40 up 1 finger 300 300",
    );
    let zoom = editor.viewport().zoom();

    // Tapping empty canvas clears the Selection again, and does not navigate.
    // The pinch doubled the zoom around (400, 300): the corner is far from
    // both rectangles.
    tap(&mut editor, "finger", (790.0, 590.0), 5.0);
    assert!(editor.selection().is_empty());
    assert_eq!(editor.viewport().zoom(), zoom);
}
