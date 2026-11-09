//! Space+drag, wheel-button drag and right-button drag pan the Canvas with
//! any Tool (ADR-0006), tested at the Editor seam with recorded pointer
//! sequences (see `select.rs` for their format, `common::replay` for the
//! `right` / `middle` button flags and the `space` lines): pointer events in,
//! viewport, Selection, Tool, saved Drawing and undo history out.
//!
//! A drag of (50, 25) moves the content by that much, so the view's origin
//! goes to (-50, -25).

use std::sync::Arc;

use pkm_core::{Editor, Modifiers, ScrollEvent, Tool};

use crate::common;
use common::{FixedWidth, editor_with, replay, tap};

/// A filled rectangle at (100, 100), 100 x 50.
const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

/// An editor with the rectangle `a` selected and a measurer, for text.
fn selected() -> Editor {
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_text_measurer(Arc::new(FixedWidth));
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

/// What a pan must leave as it found it.
#[derive(Debug, PartialEq)]
struct Untouched {
    /// The saved Drawing but for versions, which cancelled edits bump.
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

fn origin(editor: &Editor) -> (f64, f64) {
    editor.viewport().origin()
}

/// A drag of pointer 1 from (150, 125) to (200, 150) in `kind` with `flags`.
fn drag(kind: &str, flags: &str) -> String {
    format!(
        "1.00 down 1 {kind} 150 125 {flags}
         1.10 move 1 {kind} 175 140 {flags}
         1.20 move 1 {kind} 200 150 {flags}
         1.30 up 1 {kind} 200 150 {flags}"
    )
}

// --- Space ----------------------------------------------------------------

#[test]
fn space_and_drag_pans_with_every_tool_and_changes_nothing_else() {
    let tools = [
        Tool::Selection,
        Tool::Rectangle,
        Tool::Diamond,
        Tool::Freedraw,
        Tool::Eraser,
        Tool::Arrow,
    ];
    for tool in tools {
        let mut editor = selected();
        editor.set_tool(tool);
        let before = Untouched::of(&editor);

        replay(&mut editor, "space down");
        replay(&mut editor, &drag("mouse", ""));
        replay(&mut editor, "space up");

        assert_eq!(origin(&editor), (-50.0, -25.0), "{tool:?}");
        assert_eq!(Untouched::of(&editor), before, "{tool:?}");
    }
}

#[test]
fn space_and_drag_on_the_selection_pans_instead_of_moving_it() {
    let mut editor = selected();
    let before = Untouched::of(&editor);

    replay(&mut editor, "space down");
    replay(&mut editor, &drag("mouse", ""));

    assert_eq!(origin(&editor), (-50.0, -25.0));
    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn space_and_drag_pans_while_a_line_waits_for_its_next_point() {
    let mut editor = selected();
    editor.set_tool(Tool::Line);
    tap(&mut editor, "mouse", (300.0, 300.0), 0.5);
    let before = Untouched::of(&editor);

    replay(&mut editor, "space down");
    replay(&mut editor, &drag("mouse", ""));

    assert_eq!(origin(&editor), (-50.0, -25.0));
    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn space_and_drag_pans_with_a_finger_and_with_a_pencil() {
    for kind in ["finger", "pencil"] {
        let mut editor = selected();
        editor.set_tool(Tool::Rectangle);
        let before = Untouched::of(&editor);

        replay(&mut editor, "space down");
        replay(&mut editor, &drag(kind, ""));

        assert_eq!(origin(&editor), (-50.0, -25.0), "{kind}");
        assert_eq!(Untouched::of(&editor), before, "{kind}");
    }
}

#[test]
fn space_and_drag_pans_with_a_pencil_in_pencil_only_drawing() {
    let mut editor = selected();
    editor.set_pencil_only_drawing(true);
    editor.set_tool(Tool::Rectangle);
    let before = Untouched::of(&editor);

    replay(&mut editor, "space down");
    replay(&mut editor, &drag("pencil", ""));

    assert_eq!(origin(&editor), (-50.0, -25.0));
    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn a_drag_without_space_still_draws() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);

    replay(&mut editor, "space down");
    replay(&mut editor, "space up");
    replay(&mut editor, &drag("mouse", ""));

    assert_eq!(origin(&editor), (0.0, 0.0));
    assert_eq!(editor.element_count(), 2);
}

#[test]
fn pressing_space_mid_drag_does_not_affect_that_drag() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);

    replay(
        &mut editor,
        "1.00 down 1 mouse 300 300
         1.10 move 1 mouse 350 330
         space down
         1.20 move 1 mouse 400 350
         1.30 up 1 mouse 400 350
         space up",
    );

    assert_eq!(origin(&editor), (0.0, 0.0));
    assert_eq!(editor.element_count(), 2, "the rectangle was drawn");
}

#[test]
fn releasing_space_mid_drag_does_not_affect_that_drag() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);

    replay(
        &mut editor,
        "space down
         1.00 down 1 mouse 150 125
         space up
         1.10 move 1 mouse 175 140
         1.20 move 1 mouse 200 150
         1.30 up 1 mouse 200 150",
    );

    assert_eq!(origin(&editor), (-50.0, -25.0));
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn a_second_finger_during_a_space_drag_turns_it_into_the_two_finger_gesture() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);
    let before = Untouched::of(&editor);

    replay(
        &mut editor,
        "space down
         1.00 down 1 finger 500 500
         1.10 move 1 finger 510 500
         1.50 down 2 finger 520 500
         1.60 move 2 finger 560 500
         1.70 move 2 finger 600 500
         2.00 up 2 finger 600 500
         2.05 up 1 finger 510 500",
    );

    assert!(editor.viewport().zoom() > 1.0, "the fingers pinched");
    assert_eq!(Untouched::of(&editor), before);
}

// --- Wheel button ---------------------------------------------------------

#[test]
fn a_wheel_button_drag_pans_with_any_tool_and_changes_nothing_else() {
    for tool in [
        Tool::Selection,
        Tool::Rectangle,
        Tool::Freedraw,
        Tool::Eraser,
    ] {
        let mut editor = selected();
        editor.set_tool(tool);
        let before = Untouched::of(&editor);

        replay(&mut editor, &drag("mouse", "middle"));

        assert_eq!(origin(&editor), (-50.0, -25.0), "{tool:?}");
        assert_eq!(Untouched::of(&editor), before, "{tool:?}");
    }
}

#[test]
fn a_wheel_button_press_cancels_a_drawing_that_the_primary_button_began() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);
    let before = Untouched::of(&editor);

    replay(
        &mut editor,
        "1.00 down 1 mouse 300 300
         1.10 move 1 mouse 350 330
         1.20 down 1 mouse 350 330 middle
         1.30 move 1 mouse 360 330 middle
         1.40 up 1 mouse 360 330 middle
         1.50 up 1 mouse 360 330",
    );

    assert_eq!(Untouched::of(&editor), before);
    assert_eq!(origin(&editor), (-10.0, 0.0));
}

#[test]
fn scrolling_during_a_wheel_button_pan_zooms_around_the_pointer() {
    let mut editor = selected();
    replay(&mut editor, "1.00 down 1 mouse 150 125 middle");
    let scene_under_pointer = |e: &Editor| {
        let (ox, oy) = origin(e);
        let zoom = e.viewport().zoom();
        (ox + 150.0 / zoom, oy + 125.0 / zoom)
    };
    let before = scene_under_pointer(&editor);

    // No modifier: this would pan without the button.
    editor.scroll(ScrollEvent {
        x: 150.0,
        y: 125.0,
        dx: 0.0,
        dy: 20.0,
        modifiers: Modifiers::default(),
    });

    assert!(editor.viewport().zoom() > 1.0);
    let after = scene_under_pointer(&editor);
    assert!((after.0 - before.0).abs() < 1e-9 && (after.1 - before.1).abs() < 1e-9);
}

#[test]
fn scrolling_without_a_wheel_button_pan_still_pans() {
    let mut editor = Editor::new();
    replay(&mut editor, "1.00 down 1 mouse 150 125 right");

    editor.scroll(ScrollEvent {
        x: 150.0,
        y: 125.0,
        dx: 0.0,
        dy: 20.0,
        modifiers: Modifiers::default(),
    });

    assert_eq!(editor.viewport().zoom(), 1.0);
    assert_eq!(origin(&editor), (0.0, -20.0));
}

// --- Right button ---------------------------------------------------------

#[test]
fn a_right_button_drag_pans_from_where_it_passes_five_points() {
    let mut editor = selected();
    editor.set_tool(Tool::Rectangle);
    let before = Untouched::of(&editor);

    replay(
        &mut editor,
        "1.00 down 1 mouse 150 125 right
         1.10 move 1 mouse 154 125 right
         1.20 move 1 mouse 160 125 right
         1.30 move 1 mouse 180 135 right
         1.40 up 1 mouse 180 135 right",
    );

    // Panning follows the pointer from (160, 125) on.
    assert_eq!(origin(&editor), (-20.0, -10.0));
    assert_eq!(Untouched::of(&editor), before);
}

#[test]
fn a_right_button_drag_of_less_than_five_points_does_not_pan() {
    let mut editor = Editor::new();

    replay(
        &mut editor,
        "1.00 down 1 mouse 150 125 right
         1.10 move 1 mouse 153 128 right
         1.20 up 1 mouse 153 128 right",
    );

    assert_eq!(origin(&editor), (0.0, 0.0));
}

#[test]
fn a_right_click_leaves_scene_selection_and_tool_alone() {
    for at in ["150 125", "600 600"] {
        let mut editor = selected();
        let before = Untouched::of(&editor);

        replay(
            &mut editor,
            &format!(
                "1.00 down 1 mouse {at} right
                 1.05 up 1 mouse {at} right"
            ),
        );

        assert_eq!(Untouched::of(&editor), before, "{at}");
        assert_eq!(origin(&editor), (0.0, 0.0));
    }
}

#[test]
fn a_right_click_is_no_part_of_a_double_tap() {
    let mut editor = selected();
    editor.set_tool(Tool::Selection);

    // Were the right-click a tap, the left tap after it would be its second
    // and edit the rectangle's label.
    replay(
        &mut editor,
        "5.00 down 1 mouse 150 125 right
         5.05 up 1 mouse 150 125 right",
    );
    tap(&mut editor, "mouse", (150.0, 125.0), 5.2);

    assert!(editor.text_editor().is_none());
}

#[test]
fn a_right_click_does_not_end_the_wait_for_a_double_tap() {
    let mut editor = selected();
    tap(&mut editor, "mouse", (150.0, 125.0), 5.0);

    replay(
        &mut editor,
        "5.10 down 1 mouse 600 600 right
         5.15 up 1 mouse 600 600 right",
    );
    tap(&mut editor, "mouse", (150.0, 125.0), 5.2);

    assert!(editor.text_editor().is_some(), "the second tap doubled");
}

// --- Text view ------------------------------------------------------------

#[test]
fn wheel_button_and_right_button_drags_pan_and_keep_the_text_view_open() {
    // The right button pans from where it passed five points, (175, 140).
    for (flags, expected) in [("middle", (-50.0, -25.0)), ("right", (-25.0, -10.0))] {
        let mut editor = Editor::new();
        editor.set_text_measurer(Arc::new(FixedWidth));
        editor.set_tool(Tool::Text);
        tap(&mut editor, "mouse", (300.0, 300.0), 1.0);
        assert!(editor.text_editor().is_some());

        replay(&mut editor, &drag("mouse", flags));

        assert_eq!(origin(&editor), expected, "{flags}");
        assert!(editor.text_editor().is_some(), "{flags}");
    }
}
