//! Platform-independent core of WorkingTitle (ADR-0001).
//!
//! Clients drive everything through [`Editor`].

pub mod json;

/// The core's public interface: owns the open Scene and everything that edits
/// it. For now only an empty Scene exists; Tools, Selection, history and
/// viewport join as the features that need them land.
#[derive(Debug, Default)]
pub struct Editor {}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of Elements in the Scene.
    pub fn element_count(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_editor_has_an_empty_scene() {
        assert_eq!(Editor::new().element_count(), 0);
    }
}
