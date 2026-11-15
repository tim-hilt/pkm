//! The Context menu: what a long-press means (ADR-0006).
//!
//! Clients detect the hold (a finger kept down inside a small slop, or a
//! secondary click) and show the system's menu; the core decides which menu
//! it is and what the press selects, so that every platform agrees. Its
//! Element menu carries whether group and ungroup would do anything.

/// The menu a long-press asks the client to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenu {
    /// The press is on an Element, or a handle of the Selection, and the
    /// Selection is what the menu acts on.
    Element {
        /// Whether grouping the Selection would do anything; clients enable
        /// the group entry exactly then.
        can_group: bool,
        /// Whether the Selection holds a group to dissolve; clients enable
        /// the ungroup entry exactly then.
        can_ungroup: bool,
    },
    /// The press is on empty Canvas; the Selection is left as it was.
    Canvas,
}

#[cfg(test)]
mod tests {
    //! The long-press (ADR-0006), tested at the Editor seam: the press point in,
    //! which menu to show and the Selection out. Clients detect the hold and show
    //! the menu; the core decides what the press means (see `context_menu.rs`).
    //!
    //! Coordinates are worked out by hand in the comments.

    use crate::{ContextMenu, Editor, PointerKind, Tool, ZOrder};

    use crate::test_support as common;
    use common::{editor_with, replay, saved_elements, tap};

    /// A filled rectangle `id` from `(x, y)`, 40 wide and 20 high, in `extra`
    /// (members such as `"groupIds":["g"]`) as well.
    fn rectangle(id: &str, x: f64, y: f64, extra: &str) -> String {
        format!(
            r##"{{"id":"{id}","type":"rectangle","x":{x},"y":{y},"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,{extra}
        "seed":11,"version":3,"versionNonce":7}}"##
        )
    }

    /// `a` at 100..140 by 100..120 and `b` at 300..340 by 200..220.
    fn two() -> Editor {
        editor_with(&[
            &rectangle("a", 100.0, 100.0, ""),
            &rectangle("b", 300.0, 200.0, ""),
        ])
    }

    /// Holds a finger at `at`: it goes down there and the client reports the
    /// hold. The finger stays down.
    fn hold(editor: &mut Editor, at: (f64, f64)) -> Option<ContextMenu> {
        replay(editor, &format!("1.0 down 1 finger {} {}", at.0, at.1));
        editor.long_press(at.0, at.1, PointerKind::Finger)
    }

    fn element_menu(can_group: bool, can_ungroup: bool) -> Option<ContextMenu> {
        Some(ContextMenu::Element {
            can_group,
            can_ungroup,
        })
    }

    // --- What the press means ------------------------------------------------------

    #[test]
    fn pressing_an_unselected_element_selects_it_and_offers_the_element_menu() {
        let mut editor = two();

        assert_eq!(
            hold(&mut editor, (120.0, 110.0)),
            element_menu(false, false)
        );

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn pressing_an_unselected_element_replaces_the_selection() {
        let mut editor = two();
        tap(&mut editor, "finger", (320.0, 210.0), 0.0);
        assert_eq!(editor.selection(), ["b"]);

        hold(&mut editor, (120.0, 110.0));

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn pressing_an_element_of_a_group_selects_the_outermost_group() {
        let mut editor = editor_with(&[
            &rectangle("a", 100.0, 100.0, r#""groupIds":["inner","outer"],"#),
            &rectangle("b", 300.0, 200.0, r#""groupIds":["inner","outer"],"#),
            &rectangle("c", 500.0, 200.0, r#""groupIds":["outer"],"#),
        ]);

        // Everything selected is one group: it can be taken apart, not formed.
        assert_eq!(hold(&mut editor, (120.0, 110.0)), element_menu(false, true));

        assert_eq!(editor.selection(), ["a", "b", "c"]);
    }

    #[test]
    fn pressing_a_selected_element_keeps_the_whole_selection() {
        let mut editor = two();
        editor.select_all();

        // Two Elements, none grouped: grouping applies.
        assert_eq!(hold(&mut editor, (120.0, 110.0)), element_menu(true, false));

        assert_eq!(editor.selection(), ["a", "b"]);
    }

    #[test]
    fn pressing_a_transform_handle_keeps_the_selection_and_offers_the_element_menu() {
        let mut editor = two();
        tap(&mut editor, "finger", (120.0, 110.0), 0.0);

        // The rotation handle is centred 16 above the middle of the top side,
        // which is 4 above the box: (120, 80), well clear of the rectangle.
        assert_eq!(hold(&mut editor, (120.0, 80.0)), element_menu(false, false));

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn pressing_a_point_handle_keeps_the_selection_and_offers_the_element_menu() {
        let mut editor = editor_with(&[
            r##"{"id":"l","type":"line","x":50,"y":50,"width":100,"height":0,
            "points":[[0,0],[100,0]],"roughness":0,"roundness":null,"strokeWidth":2}"##,
            &rectangle("b", 300.0, 300.0, ""),
        ]);
        tap(&mut editor, "finger", (100.0, 50.0), 0.0);
        assert_eq!(editor.selection(), ["l"]);

        // The end point at (150, 50) has a 44-point handle: 14 points to its
        // side is on it, but off the line.
        assert_eq!(hold(&mut editor, (164.0, 50.0)), element_menu(false, false));

        assert_eq!(editor.selection(), ["l"]);
    }

    #[test]
    fn pressing_empty_canvas_keeps_the_selection_and_offers_the_canvas_menu() {
        let mut editor = two();
        tap(&mut editor, "finger", (120.0, 110.0), 0.0);

        assert_eq!(hold(&mut editor, (600.0, 600.0)), Some(ContextMenu::Canvas));

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn pressing_a_gap_inside_the_selections_box_counts_as_empty_canvas() {
        let mut editor = two();
        editor.select_all();

        // The box spans 100..340 by 100..220; (220, 160) is inside, on nothing.
        assert_eq!(hold(&mut editor, (220.0, 160.0)), Some(ContextMenu::Canvas));

        assert_eq!(editor.selection(), ["a", "b"]);
    }

    #[test]
    fn a_press_does_not_move_or_change_the_drawing_and_lifting_afterwards_changes_nothing() {
        let mut editor = two();
        let saved = saved_elements(&editor);
        let revision = editor.revision();

        hold(&mut editor, (120.0, 110.0));
        replay(&mut editor, "2.0 up 1 finger 120 110");

        assert_eq!(editor.selection(), ["a"]);
        assert_eq!(saved_elements(&editor), saved);
        assert_eq!(editor.revision(), revision);
        assert!(!editor.can_undo());
    }

    #[test]
    fn lifting_after_a_press_on_a_handle_does_not_select_a_point() {
        let mut editor = editor_with(&[
            r##"{"id":"l","type":"line","x":50,"y":50,"width":100,"height":0,
            "points":[[0,0],[100,0]],"roughness":0,"roundness":null,"strokeWidth":2}"##,
        ]);
        tap(&mut editor, "finger", (100.0, 50.0), 0.0);
        let overlay = editor.overlay_revision();

        hold(&mut editor, (150.0, 50.0));
        replay(&mut editor, "2.0 up 1 finger 150 50");

        assert!(editor.selected_points().is_empty());
        assert_eq!(editor.overlay_revision(), overlay);
    }

    #[test]
    fn a_secondary_click_of_a_mouse_opens_the_menu_too() {
        let mut editor = two();
        replay(&mut editor, "1.0 down 1 mouse 120 110 right");

        assert_eq!(
            editor.long_press(120.0, 110.0, PointerKind::Mouse),
            element_menu(false, false)
        );
        assert_eq!(editor.selection(), ["a"]);
    }

    // --- When it is ignored --------------------------------------------------------

    #[test]
    fn other_tools_ignore_the_press() {
        let mut editor = two();
        editor.set_tool(Tool::Rectangle);

        assert_eq!(hold(&mut editor, (120.0, 110.0)), None);

        assert!(editor.selection().is_empty());
    }

    #[test]
    fn the_pencil_never_opens_it() {
        let mut editor = two();
        replay(&mut editor, "1.0 down 1 pencil 120 110");

        assert_eq!(editor.long_press(120.0, 110.0, PointerKind::Pencil), None);

        assert!(editor.selection().is_empty());
    }

    #[test]
    fn a_press_while_two_fingers_are_down_is_ignored() {
        let mut editor = two();
        tap(&mut editor, "finger", (320.0, 210.0), 0.0);
        replay(
            &mut editor,
            "1.0 down 1 finger 120 110
         1.1 down 2 finger 220 110",
        );

        assert_eq!(editor.long_press(120.0, 110.0, PointerKind::Finger), None);

        assert_eq!(editor.selection(), ["b"]);
    }

    #[test]
    fn a_press_while_a_pointer_is_drawing_is_ignored() {
        let mut editor = two();
        editor.set_tool(Tool::Rectangle);
        replay(
            &mut editor,
            "1.0 down 1 mouse 500 500
         1.1 move 1 mouse 550 550",
        );

        assert_eq!(editor.long_press(120.0, 110.0, PointerKind::Finger), None);
    }

    #[test]
    fn a_press_while_a_pointer_is_moving_the_selection_is_ignored() {
        let mut editor = two();
        tap(&mut editor, "mouse", (120.0, 110.0), 0.0);
        replay(
            &mut editor,
            "1.0 down 1 mouse 120 110
         1.1 move 1 mouse 140 130",
        );

        assert_eq!(editor.long_press(320.0, 210.0, PointerKind::Finger), None);

        assert_eq!(editor.selection(), ["a"]);
    }

    #[test]
    fn a_press_while_a_pointer_is_transforming_is_ignored() {
        let mut editor = two();
        tap(&mut editor, "mouse", (120.0, 110.0), 0.0);
        // Drags the bottom right handle, centred on (144, 124).
        replay(
            &mut editor,
            "1.0 down 1 mouse 144 124
         1.1 move 1 mouse 160 140",
        );

        assert_eq!(editor.long_press(320.0, 210.0, PointerKind::Finger), None);

        assert_eq!(editor.selection(), ["a"]);
    }

    // --- Group and ungroup ---------------------------------------------------------

    #[test]
    fn group_and_ungroup_apply_as_the_selection_allows() {
        let mut editor = two();
        editor.select_all();
        assert_eq!(hold(&mut editor, (120.0, 110.0)), element_menu(true, false));
        replay(&mut editor, "1.1 up 1 finger 120 110");

        editor.group();

        assert_eq!(hold(&mut editor, (120.0, 110.0)), element_menu(false, true));
    }

    // --- The commands the menu runs ------------------------------------------------

    #[test]
    fn the_menus_commands_act_on_what_the_press_selected() {
        let mut editor = two();
        hold(&mut editor, (120.0, 110.0));

        editor.move_in_z_order(ZOrder::ToFront);
        let ids: Vec<_> = saved_elements(&editor)
            .iter()
            .map(|e| e.get("id").and_then(|v| v.as_str()).unwrap().to_owned())
            .collect();
        assert_eq!(ids, ["b", "a"]);

        editor.delete_selection();
        assert_eq!(editor.element_count(), 1);
    }
}
