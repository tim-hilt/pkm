//! Pencil-only drawing mode (ADR-0006): on an iPad with an Apple Pencil, the
//! Pencil uses the current Tool while fingers only select, pan and zoom, so
//! that a palm or finger never leaves a stray mark.
//!
//! The mode turns itself on the first time a Pencil is used, unless the
//! user has already chosen. It is a setting, so clients keep it between
//! launches: store the value whenever [`PencilOnly::is_on`] changes, and
//! restore it with [`PencilOnly::choose`] if one is stored.

use crate::input::PointerKind;

#[derive(Debug, Default)]
pub(crate) struct PencilOnly {
    on: bool,
    /// Whether the user (or a stored setting) decided, which a Pencil being
    /// used must no longer override.
    chosen: bool,
}

impl PencilOnly {
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The user's decision, or one restored from storage; final from now on.
    pub fn choose(&mut self, on: bool) {
        self.on = on;
        self.chosen = true;
    }

    /// Notes which kind of pointer is in use; the first Pencil turns the mode
    /// on if nobody has chosen yet.
    pub fn observe(&mut self, kind: PointerKind) {
        if kind == PointerKind::Pencil && !self.chosen {
            self.choose(true);
        }
    }

    /// Whether pointers of `kind` are held to selecting, panning and zooming:
    /// fingers are, in the mode. The Pencil and the mouse are not.
    pub fn restricts(&self, kind: PointerKind) -> bool {
        self.on && kind == PointerKind::Finger
    }
}

#[cfg(test)]
mod tests {
    //! Pencil-only drawing mode (ADR-0006), tested at the Editor seam with
    //! recorded pointer sequences (see `select.rs` for their format): pointer
    //! events in, Selection, Tool, saved Drawing, viewport and text view out.
    //!
    //! In the mode the Pencil uses the current Tool while fingers only select,
    //! pan and zoom. Expected values are worked out by hand in the comments.

    use std::sync::Arc;

    use crate::{Editor, Tool};

    use crate::test_support as common;
    use common::{FixedWidth, editor_with, replay, saved_elements, tap};

    /// A filled rectangle at (100, 100), 100 x 50.
    const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

    /// An editor on an empty Canvas that has seen a Pencil, so the mode is on.
    fn after_first_pencil() -> Editor {
        let mut editor = editor_with(&[RECTANGLE]);
        tap(&mut editor, "pencil", (600.0, 600.0), 0.0);
        assert!(editor.pencil_only_drawing());
        editor
    }

    /// A drag of pointer 1 of `kind` from `from` to `to`, in two steps.
    fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
        replay(
            editor,
            &format!(
                "1.00 down 1 {kind} {} {}\n1.05 move 1 {kind} {} {}\n1.10 move 1 {kind} {} {}\n1.15 up 1 {kind} {} {}",
                from.0,
                from.1,
                (from.0 + to.0) / 2.0,
                (from.1 + to.1) / 2.0,
                to.0,
                to.1,
                to.0,
                to.1
            ),
        );
    }

    // --- Turning on and off ---------------------------------------------------------

    #[test]
    fn it_is_off_until_a_pencil_is_used() {
        let mut editor = editor_with(&[RECTANGLE]);
        assert!(!editor.pencil_only_drawing());

        tap(&mut editor, "finger", (150.0, 125.0), 0.0);
        tap(&mut editor, "mouse", (150.0, 125.0), 1.0);

        assert!(!editor.pencil_only_drawing());
    }

    #[test]
    fn the_first_pencil_input_turns_it_on_and_it_stays_on() {
        let mut editor = editor_with(&[RECTANGLE]);

        replay(&mut editor, "0 down 1 pencil 600 600");
        assert!(editor.pencil_only_drawing());

        replay(&mut editor, "0.1 up 1 pencil 600 600");
        tap(&mut editor, "finger", (150.0, 125.0), 1.0);
        assert!(editor.pencil_only_drawing());
    }

    #[test]
    fn switching_it_off_in_settings_is_final_even_if_a_pencil_is_used_after() {
        let mut editor = after_first_pencil();

        editor.set_pencil_only_drawing(false);
        tap(&mut editor, "pencil", (600.0, 600.0), 1.0);

        assert!(!editor.pencil_only_drawing());
    }

    #[test]
    fn a_choice_made_before_any_pencil_is_used_is_kept() {
        // What a client does at launch with a setting stored earlier.
        let mut off = Editor::new();
        off.set_pencil_only_drawing(false);
        tap(&mut off, "pencil", (10.0, 10.0), 0.0);
        assert!(!off.pencil_only_drawing());

        let mut on = Editor::new();
        on.set_pencil_only_drawing(true);
        assert!(on.pencil_only_drawing());
    }

    // --- Fingers never create Elements ------------------------------------------------

    #[test]
    fn a_finger_never_draws_a_shape_but_a_pencil_does() {
        for tool in [Tool::Rectangle, Tool::Diamond, Tool::Ellipse] {
            let mut editor = after_first_pencil();
            editor.set_tool(tool);

            drag(&mut editor, "finger", (300.0, 300.0), (400.0, 360.0));
            assert_eq!(saved_elements(&editor).len(), 1, "{tool:?}: finger");
            assert_eq!(editor.tool(), tool);

            drag(&mut editor, "pencil", (300.0, 300.0), (400.0, 360.0));
            assert_eq!(saved_elements(&editor).len(), 2, "{tool:?}: pencil");
        }
    }

    #[test]
    fn a_finger_never_draws_a_stroke_but_a_pencil_does() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Freedraw);
        editor.set_tool_lock(true);

        drag(&mut editor, "finger", (300.0, 300.0), (400.0, 360.0));
        assert_eq!(saved_elements(&editor).len(), 1);

        drag(&mut editor, "pencil", (300.0, 300.0), (400.0, 360.0));
        assert_eq!(saved_elements(&editor).len(), 2);
        assert_eq!(editor.tool(), Tool::Freedraw);
    }

    #[test]
    fn fingers_never_place_the_points_of_a_line_the_pencil_began() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Line);

        // A Pencil tap places the first point and leaves the line waiting.
        tap(&mut editor, "pencil", (300.0, 300.0), 1.0);
        // A finger tap far away would place a second one.
        tap(&mut editor, "finger", (400.0, 360.0), 2.0);
        editor.set_tool(Tool::Selection);

        // Only one point was placed: nothing is left of the line.
        assert_eq!(saved_elements(&editor).len(), 1);
    }

    #[test]
    fn a_finger_never_erases_but_a_pencil_does() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Eraser);

        tap(&mut editor, "finger", (150.0, 125.0), 1.0);
        assert!(!is_deleted(&editor, "a"));

        tap(&mut editor, "pencil", (150.0, 125.0), 2.0);
        assert!(is_deleted(&editor, "a"));
    }

    fn is_deleted(editor: &Editor, id: &str) -> bool {
        saved_elements(editor).iter().any(|e| {
            e.get("id").and_then(|v| v.as_str()) == Some(id)
                && e.get("isDeleted").and_then(|v| v.as_bool()) == Some(true)
        })
    }

    #[test]
    fn a_finger_never_opens_the_text_view_but_a_pencil_does() {
        let mut editor = after_first_pencil();
        editor.set_text_measurer(Arc::new(FixedWidth));
        editor.set_tool(Tool::Text);

        tap(&mut editor, "finger", (300.0, 300.0), 1.0);
        assert!(editor.text_editor().is_none());

        tap(&mut editor, "pencil", (300.0, 300.0), 2.0);
        assert!(editor.text_editor().is_some());
    }

    // --- What fingers do ----------------------------------------------------------------

    #[test]
    fn a_finger_tap_selects_whatever_the_tool_and_leaves_the_tool_alone() {
        for tool in [
            Tool::Selection,
            Tool::Rectangle,
            Tool::Freedraw,
            Tool::Eraser,
        ] {
            let mut editor = after_first_pencil();
            editor.set_tool(tool);

            tap(&mut editor, "finger", (150.0, 125.0), 1.0);

            assert_eq!(editor.selection(), ["a"], "{tool:?}");
            assert_eq!(editor.tool(), tool);
            assert_eq!(saved_elements(&editor).len(), 1, "{tool:?}");
        }
    }

    #[test]
    fn a_one_finger_drag_pans_on_empty_canvas_and_does_not_box_select() {
        let mut editor = after_first_pencil();

        // The content follows the finger by 40 right and 30 down.
        drag(&mut editor, "finger", (300.0, 300.0), (340.0, 330.0));

        assert_eq!(editor.viewport().origin(), (-40.0, -30.0));
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn a_one_finger_drag_pans_even_from_an_element_and_does_not_move_it() {
        let mut editor = after_first_pencil();
        tap(&mut editor, "finger", (150.0, 125.0), 1.0);
        assert_eq!(editor.selection(), ["a"]);

        drag(&mut editor, "finger", (150.0, 125.0), (190.0, 155.0));

        assert_eq!(editor.viewport().origin(), (-40.0, -30.0));
        assert_eq!(editor.selection(), ["a"]);
        assert!(!editor.can_undo(), "nothing was moved");
    }

    #[test]
    fn a_one_finger_drag_pans_with_a_drawing_tool_too() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Rectangle);

        drag(&mut editor, "finger", (300.0, 300.0), (340.0, 330.0));

        assert_eq!(editor.viewport().origin(), (-40.0, -30.0));
        assert_eq!(saved_elements(&editor).len(), 1);
    }

    #[test]
    fn two_fingers_still_pinch_zoom() {
        let mut editor = after_first_pencil();

        replay(
            &mut editor,
            "1.0 down 1 finger 100 100
         1.0 down 2 finger 200 100
         1.1 move 2 finger 300 100",
        );

        // The fingers went from 100 to 200 apart.
        assert_eq!(editor.viewport().zoom(), 2.0);
    }

    // --- What the Pencil does ---------------------------------------------------------------

    #[test]
    fn a_pencil_drag_on_empty_canvas_box_selects_and_does_not_pan() {
        let mut editor = after_first_pencil();

        // The box from (50, 50) to (250, 200) encloses the rectangle.
        drag(&mut editor, "pencil", (50.0, 50.0), (250.0, 200.0));

        assert_eq!(editor.selection(), ["a"]);
        assert_eq!(editor.viewport().origin(), (0.0, 0.0));
    }

    #[test]
    fn a_pencil_drag_on_an_element_moves_it() {
        let mut editor = after_first_pencil();

        drag(&mut editor, "pencil", (150.0, 125.0), (170.0, 125.0));

        assert_eq!(editor.selection(), ["a"]);
        assert!(editor.can_undo());
        assert_eq!(editor.viewport().origin(), (0.0, 0.0));
    }

    #[test]
    fn a_finger_resting_while_the_pencil_draws_leaves_the_stroke_alone() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Freedraw);

        replay(
            &mut editor,
            "1.00 down 1 pencil 300 300
         1.05 down 2 finger 500 500
         1.10 move 1 pencil 340 320
         1.15 up 2 finger 500 500
         1.20 up 1 pencil 380 340",
        );

        // Only the Pencil's stroke was added.
        assert_eq!(saved_elements(&editor).len(), 2);
    }

    // --- Without the mode -----------------------------------------------------------------------

    #[test]
    fn without_the_mode_fingers_draw_and_box_select_like_on_iphone() {
        let mut editor = editor_with(&[RECTANGLE]);
        editor.set_tool(Tool::Rectangle);
        drag(&mut editor, "finger", (300.0, 300.0), (400.0, 360.0));
        assert_eq!(saved_elements(&editor).len(), 2);

        drag(&mut editor, "finger", (50.0, 50.0), (250.0, 200.0));
        assert_eq!(editor.selection(), ["a"]);
        assert_eq!(editor.viewport().origin(), (0.0, 0.0));
    }

    #[test]
    fn switching_the_mode_off_gives_fingers_back_their_tools() {
        let mut editor = after_first_pencil();
        editor.set_pencil_only_drawing(false);
        editor.set_tool(Tool::Rectangle);

        drag(&mut editor, "finger", (300.0, 300.0), (400.0, 360.0));

        assert_eq!(saved_elements(&editor).len(), 2);
    }

    #[test]
    fn the_mouse_is_not_restricted_by_the_mode() {
        let mut editor = after_first_pencil();
        editor.set_tool(Tool::Rectangle);

        drag(&mut editor, "mouse", (300.0, 300.0), (400.0, 360.0));

        assert_eq!(saved_elements(&editor).len(), 2);
    }
}
