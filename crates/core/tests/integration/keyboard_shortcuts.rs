//! Keyboard shortcuts (ADR-0006), tested at the Editor seam: the table of
//! Excalidraw's shortcuts (`shortcuts()`) maps keys to semantic Commands,
//! which `Editor::perform` carries out. Expected keys come from Excalidraw's
//! `Tools.tsx` and action `keyTest`s (macOS variants).

use crate::common;

use common::editor_with;
use workingtitle_core::{Command, Editor, Modifiers, Tool, ZOrder, shortcuts};

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
    let command = command_for(key, modifiers).unwrap_or_else(|| panic!("no shortcut for {key}"));
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

#[test]
fn cmd_g_groups_and_cmd_shift_g_ungroups() {
    let mut editor = editor_with(&[&rect("a", 0.0), &rect("b", 20.0)]);
    press(&mut editor, "a", CMD);
    press(&mut editor, "g", CMD);
    assert!(editor.action_bar().unwrap().can_ungroup);
    press(&mut editor, "g", CMD_SHIFT);
    assert!(!editor.action_bar().unwrap().can_ungroup);
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
