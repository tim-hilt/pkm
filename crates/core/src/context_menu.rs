//! The Context menu: what a long-press means (ADR-0006).
//!
//! Clients detect the hold (a finger kept down inside a small slop, or a
//! secondary click) and show the system's menu; the core decides which menu
//! it is and what the press selects, so that every platform agrees. It
//! replaces the Action Bar's group and ungroup flags.

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
