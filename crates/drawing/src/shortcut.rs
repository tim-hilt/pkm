//! Keyboard shortcuts: Excalidraw's keys (macOS variants) bound to semantic
//! Commands. Clients turn the table into platform shortcuts (Mac menus, the
//! iPad keyboard shortcut overlay) and hand the Command of a pressed one to
//! [`crate::Editor::perform`]. Copy, cut and paste are not in it: they go
//! through the platform's own Edit commands and pasteboard.

use crate::align::{Alignment, Distribution};
use crate::input::Modifiers;
use crate::tool::Tool;
use crate::z_order::ZOrder;

/// Something the user can ask the Editor to do, whatever the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SetTool(Tool),
    ToggleToolLock,
    Undo,
    Redo,
    Duplicate,
    Delete,
    Group,
    Ungroup,
    MoveInZOrder(ZOrder),
    Align(Alignment),
    Distribute(Distribution),
    SelectAll,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ZoomToFit,
    /// Escape: finishes the line or arrow being placed; otherwise nothing.
    FinishLine,
    /// Enter: finishes the line or arrow being placed; otherwise edits the
    /// text of the Selection.
    FinishLineOrEditText,
}

/// A key that performs a Command. `key` is the character the key types
/// without Shift (`"r"`, `"1"`, `"["`), or `"backspace"`, `"delete"`, `"escape"`,
/// `"enter"` (Return and keypad Enter), `"arrowleft"`, `"arrowright"`,
/// `"arrowup"`, `"arrowdown"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub key: String,
    pub modifiers: Modifiers,
    pub command: Command,
}

const NONE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: false,
};
const CMD: Modifiers = Modifiers { meta: true, ..NONE };
const CMD_SHIFT: Modifiers = Modifiers { shift: true, ..CMD };
const CMD_ALT: Modifiers = Modifiers { alt: true, ..CMD };
const ALT: Modifiers = Modifiers { alt: true, ..NONE };
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..NONE
};

const TABLE: &[(&str, Modifiers, Command)] = &[
    ("v", NONE, Command::SetTool(Tool::Selection)),
    ("1", NONE, Command::SetTool(Tool::Selection)),
    ("h", NONE, Command::SetTool(Tool::Hand)),
    ("r", NONE, Command::SetTool(Tool::Rectangle)),
    ("2", NONE, Command::SetTool(Tool::Rectangle)),
    ("d", NONE, Command::SetTool(Tool::Diamond)),
    ("3", NONE, Command::SetTool(Tool::Diamond)),
    ("o", NONE, Command::SetTool(Tool::Ellipse)),
    ("4", NONE, Command::SetTool(Tool::Ellipse)),
    ("a", NONE, Command::SetTool(Tool::Arrow)),
    ("5", NONE, Command::SetTool(Tool::Arrow)),
    ("l", NONE, Command::SetTool(Tool::Line)),
    ("6", NONE, Command::SetTool(Tool::Line)),
    ("p", NONE, Command::SetTool(Tool::Freedraw)),
    ("x", NONE, Command::SetTool(Tool::Freedraw)),
    ("7", NONE, Command::SetTool(Tool::Freedraw)),
    ("t", NONE, Command::SetTool(Tool::Text)),
    ("8", NONE, Command::SetTool(Tool::Text)),
    ("e", NONE, Command::SetTool(Tool::Eraser)),
    ("0", NONE, Command::SetTool(Tool::Eraser)),
    ("q", NONE, Command::ToggleToolLock),
    ("z", CMD, Command::Undo),
    ("z", CMD_SHIFT, Command::Redo),
    ("y", CMD, Command::Redo),
    ("d", CMD, Command::Duplicate),
    ("backspace", NONE, Command::Delete),
    ("delete", NONE, Command::Delete),
    ("escape", NONE, Command::FinishLine),
    ("enter", NONE, Command::FinishLineOrEditText),
    ("g", CMD, Command::Group),
    ("g", CMD_SHIFT, Command::Ungroup),
    ("]", CMD, Command::MoveInZOrder(ZOrder::Forward)),
    ("[", CMD, Command::MoveInZOrder(ZOrder::Backward)),
    ("]", CMD_ALT, Command::MoveInZOrder(ZOrder::ToFront)),
    ("[", CMD_ALT, Command::MoveInZOrder(ZOrder::ToBack)),
    ("arrowleft", CMD_SHIFT, Command::Align(Alignment::Left)),
    ("arrowright", CMD_SHIFT, Command::Align(Alignment::Right)),
    ("arrowup", CMD_SHIFT, Command::Align(Alignment::Top)),
    ("arrowdown", CMD_SHIFT, Command::Align(Alignment::Bottom)),
    ("h", ALT, Command::Distribute(Distribution::Horizontally)),
    ("v", ALT, Command::Distribute(Distribution::Vertically)),
    ("a", CMD, Command::SelectAll),
    ("=", CMD, Command::ZoomIn),
    ("=", CMD_SHIFT, Command::ZoomIn),
    ("+", CMD, Command::ZoomIn),
    ("-", CMD, Command::ZoomOut),
    ("0", CMD, Command::ResetZoom),
    ("1", SHIFT, Command::ZoomToFit),
];

/// Every keyboard shortcut, in menu order; a Command may have several.
pub fn shortcuts() -> Vec<Shortcut> {
    TABLE
        .iter()
        .map(|&(key, modifiers, command)| Shortcut {
            key: key.to_owned(),
            modifiers,
            command,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Keyboard shortcuts (ADR-0006), tested at the Editor seam: the table of
    //! Excalidraw's shortcuts (`shortcuts()`) maps keys to semantic Commands,
    //! which `Editor::perform` carries out. Expected keys come from Excalidraw's
    //! `Tools.tsx` and action `keyTest`s (macOS variants).

    use crate::test_support as common;

    use crate::{
        Alignment, Command, ContextMenu, Distribution, Editor, Modifiers, PointerKind, Tool,
        ZOrder, shortcuts,
    };
    use common::editor_with;

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };
    const CMD: Modifiers = Modifiers { meta: true, ..NONE };
    const CMD_SHIFT: Modifiers = Modifiers {
        meta: true,
        shift: true,
        ..NONE
    };
    const CMD_ALT: Modifiers = Modifiers {
        meta: true,
        alt: true,
        ..NONE
    };
    const ALT: Modifiers = Modifiers { alt: true, ..NONE };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..NONE
    };

    /// The Command the table binds to `key` with `modifiers`, if any.
    fn command_for(key: &str, modifiers: Modifiers) -> Option<Command> {
        shortcuts()
            .into_iter()
            .find(|s| s.key == key && s.modifiers == modifiers)
            .map(|s| s.command)
    }

    fn press(editor: &mut Editor, key: &str, modifiers: Modifiers) {
        let command =
            command_for(key, modifiers).unwrap_or_else(|| panic!("no shortcut for {key}"));
        editor.perform(command);
    }

    fn rect(id: &str, x: f64) -> String {
        format!(r#"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":10,"height":10}}"#)
    }

    #[test]
    fn letter_keys_select_tools() {
        let cases = [
            ("v", Tool::Selection),
            ("h", Tool::Hand),
            ("r", Tool::Rectangle),
            ("d", Tool::Diamond),
            ("o", Tool::Ellipse),
            ("a", Tool::Arrow),
            ("l", Tool::Line),
            ("p", Tool::Freedraw),
            ("x", Tool::Freedraw),
            ("t", Tool::Text),
            ("e", Tool::Eraser),
        ];
        for (key, tool) in cases {
            let mut editor = Editor::new();
            editor.set_tool(Tool::Hand);
            if tool == Tool::Hand {
                editor.set_tool(Tool::Selection);
            }
            press(&mut editor, key, NONE);
            assert_eq!(editor.tool(), tool, "key {key}");
        }
    }

    #[test]
    fn number_keys_select_tools() {
        let cases = [
            ("1", Tool::Selection),
            ("2", Tool::Rectangle),
            ("3", Tool::Diamond),
            ("4", Tool::Ellipse),
            ("5", Tool::Arrow),
            ("6", Tool::Line),
            ("7", Tool::Freedraw),
            ("8", Tool::Text),
            ("0", Tool::Eraser),
        ];
        for (key, tool) in cases {
            let mut editor = Editor::new();
            editor.set_tool(Tool::Hand);
            press(&mut editor, key, NONE);
            assert_eq!(editor.tool(), tool, "key {key}");
        }
    }

    #[test]
    fn q_toggles_the_tool_lock() {
        let mut editor = Editor::new();
        press(&mut editor, "q", NONE);
        assert!(editor.tool_lock());
        press(&mut editor, "q", NONE);
        assert!(!editor.tool_lock());
    }

    #[test]
    fn cmd_z_undoes_and_cmd_shift_z_redoes() {
        let mut editor = editor_with(&[&rect("a", 0.0)]);
        editor.select_all();
        press(&mut editor, "backspace", NONE);
        assert_eq!(editor.element_count(), 0);
        press(&mut editor, "z", CMD);
        assert_eq!(editor.element_count(), 1);
        press(&mut editor, "z", CMD_SHIFT);
        assert_eq!(editor.element_count(), 0);
        press(&mut editor, "z", CMD);
        press(&mut editor, "y", CMD);
        assert_eq!(editor.element_count(), 0);
    }

    #[test]
    fn delete_key_deletes_the_selection() {
        let mut editor = editor_with(&[&rect("a", 0.0)]);
        press(&mut editor, "a", CMD);
        press(&mut editor, "delete", NONE);
        assert_eq!(editor.element_count(), 0);
    }

    #[test]
    fn cmd_a_selects_all_and_cmd_d_duplicates() {
        let mut editor = editor_with(&[&rect("a", 0.0), &rect("b", 20.0)]);
        press(&mut editor, "a", CMD);
        assert_eq!(editor.selection().len(), 2);
        press(&mut editor, "d", CMD);
        assert_eq!(editor.element_count(), 4);
    }

    /// The long-press's answer for the Selection: whether ungrouping would do
    /// anything. Rectangle `a` is 0..10 by 0..10.
    fn can_ungroup(editor: &mut Editor) -> bool {
        match editor.long_press(5.0, 5.0, PointerKind::Finger) {
            Some(ContextMenu::Element { can_ungroup, .. }) => can_ungroup,
            Some(ContextMenu::Canvas) | None => panic!("a long-press on a selected Element"),
        }
    }

    #[test]
    fn cmd_g_groups_and_cmd_shift_g_ungroups() {
        let mut editor = editor_with(&[&rect("a", 0.0), &rect("b", 20.0)]);
        press(&mut editor, "a", CMD);
        press(&mut editor, "g", CMD);
        assert!(can_ungroup(&mut editor));
        press(&mut editor, "g", CMD_SHIFT);
        assert!(!can_ungroup(&mut editor));
    }

    #[test]
    fn arrows_align_and_alt_letters_distribute() {
        let cases = [
            ("arrowleft", CMD_SHIFT, Command::Align(Alignment::Left)),
            ("arrowright", CMD_SHIFT, Command::Align(Alignment::Right)),
            ("arrowup", CMD_SHIFT, Command::Align(Alignment::Top)),
            ("arrowdown", CMD_SHIFT, Command::Align(Alignment::Bottom)),
            ("h", ALT, Command::Distribute(Distribution::Horizontally)),
            ("v", ALT, Command::Distribute(Distribution::Vertically)),
        ];
        for (key, modifiers, command) in cases {
            assert_eq!(command_for(key, modifiers), Some(command), "{key}");
        }

        let mut editor = editor_with(&[&rect("a", 0.0), &rect("b", 30.0)]);
        press(&mut editor, "a", CMD);
        press(&mut editor, "arrowright", CMD_SHIFT);
        assert!(editor.can_undo());
    }

    #[test]
    fn brackets_move_in_z_order() {
        assert_eq!(
            command_for("]", CMD),
            Some(Command::MoveInZOrder(ZOrder::Forward))
        );
        assert_eq!(
            command_for("[", CMD),
            Some(Command::MoveInZOrder(ZOrder::Backward))
        );
        assert_eq!(
            command_for("]", CMD_ALT),
            Some(Command::MoveInZOrder(ZOrder::ToFront))
        );
        assert_eq!(
            command_for("[", CMD_ALT),
            Some(Command::MoveInZOrder(ZOrder::ToBack))
        );

        // a below b; bringing a to the front puts it on top.
        let mut editor = editor_with(&[&rect("a", 0.0), &rect("b", 0.0)]);
        common::tap(&mut editor, "mouse", (1.0, 5.0), 0.0);
        let top = editor.selection()[0].clone();
        press(&mut editor, "[", CMD_ALT);
        let saved = common::saved_elements(&editor);
        assert_eq!(common::text(&saved[0], "id"), top);
    }

    #[test]
    fn zoom_shortcuts() {
        let mut editor = Editor::new();
        editor.set_view_size(100.0, 100.0);
        press(&mut editor, "=", CMD);
        assert!((editor.viewport().zoom() - 1.1).abs() < 1e-9);
        press(&mut editor, "-", CMD);
        press(&mut editor, "-", CMD);
        assert!((editor.viewport().zoom() - 0.9).abs() < 1e-9);
        press(&mut editor, "0", CMD);
        assert!((editor.viewport().zoom() - 1.0).abs() < 1e-9);
        assert_eq!(command_for("1", SHIFT), Some(Command::ZoomToFit));
    }

    #[test]
    fn cmd_plus_zooms_in_whatever_the_layout() {
        // Excalidraw ignores Shift for zoom in, so ⌘+ on a US keyboard (⌘⇧=)
        // works; on layouts with a key of its own for "+" (German) that key does.
        assert_eq!(command_for("=", CMD_SHIFT), Some(Command::ZoomIn));
        assert_eq!(command_for("+", CMD), Some(Command::ZoomIn));
    }

    #[test]
    fn every_key_and_modifier_combination_is_bound_once() {
        let table = shortcuts();
        for (i, a) in table.iter().enumerate() {
            for b in &table[i + 1..] {
                assert!(
                    !(a.key == b.key && a.modifiers == b.modifiers),
                    "{} bound twice",
                    a.key
                );
            }
        }
    }

    // --- Escape and Enter finish a line being placed -------------------------------

    /// Three taps with `tool`: two points placed, the third being placed.
    fn place_points(editor: &mut Editor, tool: Tool) {
        editor.set_tool(tool);
        common::tap(editor, "mouse", (0.0, 0.0), 0.0);
        common::tap(editor, "mouse", (50.0, 0.0), 1.0);
        common::tap(editor, "mouse", (50.0, 50.0), 2.0);
    }

    #[test]
    fn escape_and_enter_finish_a_line_and_an_arrow_with_the_points_so_far() {
        for key in ["escape", "enter"] {
            for (tool, kind) in [(Tool::Line, "line"), (Tool::Arrow, "arrow")] {
                let mut editor = Editor::new();
                place_points(&mut editor, tool);

                press(&mut editor, key, NONE);

                let elements = common::saved_elements(&editor);
                assert_eq!(elements.len(), 1, "{key} {kind}");
                assert_eq!(
                    elements[0]
                        .get("type")
                        .and_then(pkm_base::json::Value::as_str),
                    Some(kind)
                );
                assert_eq!(
                    common::saved_points(&elements[0]),
                    [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)],
                    "{key} {kind}"
                );
                assert_eq!(editor.tool(), Tool::Selection, "{key} {kind}");
            }
        }
    }

    #[test]
    fn finishing_with_a_key_is_one_undo_step_and_respects_the_tool_lock() {
        let mut editor = Editor::new();
        editor.set_tool_lock(true);
        place_points(&mut editor, Tool::Line);

        press(&mut editor, "escape", NONE);
        assert_eq!(editor.tool(), Tool::Line, "locked Tool stays");

        editor.undo();
        assert_eq!(editor.element_count(), 0);
        assert!(!editor.can_undo());
    }

    #[test]
    fn escape_with_nothing_being_placed_changes_nothing() {
        let mut editor = editor_with(&[&rect("a", 0.0)]);
        editor.select_all();
        editor.set_tool(Tool::Rectangle);
        let before = editor.save();

        press(&mut editor, "escape", NONE);

        assert_eq!(editor.save(), before);
        assert_eq!(editor.tool(), Tool::Rectangle);
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn enter_with_nothing_being_placed_edits_the_selected_text() {
        let mut editor = editor_with(&[&rect("a", 0.0)]);
        editor.set_text_measurer(std::sync::Arc::new(common::FixedWidth));
        common::tap(&mut editor, "mouse", (5.0, 5.0), 0.0);
        assert_eq!(editor.selection(), ["a"]);

        press(&mut editor, "enter", NONE);

        assert!(editor.text_editor().is_some());
    }
}
