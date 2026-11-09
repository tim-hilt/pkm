//! Pencil double-tap (ADR-0006): the Apple Pencil's tap gesture switches the
//! Tool, as the user set it up for all apps in the system settings.
//!
//! The client forwards the double-tap with that setting (UIKit's
//! `UIPencilInteraction.preferredTapAction`), and maps the settings that are
//! not about Tools (a palette, a shortcut, ...) to [`PencilDoubleTap::Ignore`]:
//! the app shows none of them.

use crate::tool::Tool;

/// What the user wants a Pencil double-tap to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PencilDoubleTap {
    /// Toggles between the current Tool and the eraser.
    SwitchEraser,
    /// Toggles between the current Tool and the one used before it.
    SwitchPrevious,
    /// Does nothing.
    Ignore,
}

impl PencilDoubleTap {
    /// The Tool to switch to, if any. `previous` is the Tool used before
    /// `current`; switching Tools makes `current` the previous one, so the
    /// next double-tap goes back.
    pub(crate) fn target(self, current: Tool, previous: Tool) -> Option<Tool> {
        match self {
            PencilDoubleTap::SwitchEraser if current == Tool::Eraser => Some(previous),
            PencilDoubleTap::SwitchEraser => Some(Tool::Eraser),
            PencilDoubleTap::SwitchPrevious => Some(previous),
            PencilDoubleTap::Ignore => None,
        }
    }
}
