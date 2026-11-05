//! Keyboard shortcuts: Excalidraw's keys (macOS variants) bound to semantic
//! Commands. Clients turn the table into platform shortcuts (Mac menus, the
//! iPad keyboard shortcut overlay) and hand the Command of a pressed one to
//! [`crate::Editor::perform`]. Copy, cut and paste are not in it: they go
//! through the platform's own Edit commands and pasteboard.

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
/// `"enter"` (Return and keypad Enter).
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
