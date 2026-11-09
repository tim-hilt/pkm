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
