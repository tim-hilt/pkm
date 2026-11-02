//! The Tool: the active mode that determines what the next input does.

/// Tools join as the features that need them land.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    /// Selects Elements (the default). Needs no input handling yet.
    #[default]
    Selection,
    /// Pans the Canvas with a single pointer drag, whatever the pointer.
    Hand,
}
