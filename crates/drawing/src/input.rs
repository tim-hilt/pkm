//! Raw input as clients forward it (ADR-0006). Clients translate nothing
//! beyond units and coordinate systems; what an input means is for the core
//! to decide.

/// Modifier keys held while an event happened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Command on Apple platforms, the Windows key elsewhere.
    pub meta: bool,
}

/// What produced a pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Finger,
    Pencil,
    Mouse,
}

/// Which button of a mouse is down. Fingers and the Pencil always send
/// `Primary`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PointerButton {
    #[default]
    Primary,
    /// The right button.
    Secondary,
    /// The wheel button.
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    /// The system took the pointer away (palm rejection, an alert, ...);
    /// treated like `Up` for tracking but never as a completed action.
    Cancel,
}

/// One step in the life of a pointer: a finger, a Pencil or a mouse button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// Identifies the pointer from its `Down` to its `Up` or `Cancel`; may be
    /// reused afterwards.
    pub id: u64,
    pub phase: PointerPhase,
    /// Position in view coordinates: points from the Canvas's top-left
    /// corner, y down.
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
    /// The button that is down; see [`PointerButton`].
    pub button: PointerButton,
    /// `0.0..=1.0`; `0.5` where the device does not report pressure.
    pub pressure: f32,
    /// Seconds on any monotonic clock.
    pub timestamp: f64,
    pub modifiers: Modifiers,
}

/// A scroll wheel or two-finger trackpad scroll (Mac).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollEvent {
    /// Pointer position in view coordinates; zooming is anchored there.
    pub x: f64,
    pub y: f64,
    /// How far the system wants the content to move, in view points: positive
    /// `dx` to the right, positive `dy` down. This is the direction the
    /// user's fingers moved, with "natural scrolling" already applied by the
    /// system. Wheel ticks measured in lines must be converted to points.
    pub dx: f64,
    pub dy: f64,
    pub modifiers: Modifiers,
}

/// A trackpad pinch (Mac), delivered as increments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagnifyEvent {
    /// Pinch centre in view coordinates.
    pub x: f64,
    pub y: f64,
    /// Relative change of zoom since the previous event: `0.1` zooms in by
    /// 10 %, `-0.1` out by 10 %.
    pub magnification: f64,
}
