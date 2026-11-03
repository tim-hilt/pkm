//! Recognising taps and drags from raw pointer events (ADR-0006).
//!
//! A tap is a press that stays where it went down; a press that does not is
//! a drag, which the recognizer announces the moment the pointer leaves the
//! tap's slop and then leaves to whoever acts on it. How still counts as still
//! depends on the pointer: a finger is bigger and shakier than a mouse or
//! Pencil. Fingers also tap *quickly*: a finger resting on the Canvas is
//! more likely a hand being put down or the start of something else than a
//! tap, which is why only a [`Tap::is_short`] one is allowed to clear the
//! Selection.

use crate::input::{PointerEvent, PointerKind, PointerPhase};

/// How far a finger may wander from where it landed, in view points, and
/// still be tapping.
const FINGER_SLOP: f64 = 10.0;
/// The same for a mouse or Pencil, which are steady.
const PRECISE_SLOP: f64 = 4.0;
/// The longest a finger may stay down for a tap to be short, in seconds.
const SHORT_TAP: f64 = 0.3;

/// What a pointer just turned out to be doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Recognized {
    /// The pointer lifted without having moved away.
    Tap(Tap),
    /// The pointer moved away from where it went down, which is the origin.
    /// Announced once per press; what follows is the pointer's own events.
    DragBegan(Origin),
}

/// Where and with what a press went down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Origin {
    /// In view coordinates.
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
}

/// A completed tap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tap {
    /// Where the pointer went down, in view coordinates.
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
    short: bool,
}

impl Tap {
    /// Whether the tap was quick enough to count as a deliberate tap on
    /// empty Canvas. Only fingers are held to it; a click with a mouse or
    /// Pencil is never accidental in that way.
    pub fn is_short(&self) -> bool {
        self.short || self.kind != PointerKind::Finger
    }
}

/// A pointer that is down and has not moved away yet.
#[derive(Debug, Clone, Copy)]
struct Press {
    pointer: u64,
    x: f64,
    y: f64,
    kind: PointerKind,
    at: f64,
    moved: bool,
}

/// Follows one pointer at a time from down to up.
#[derive(Debug, Default)]
pub(crate) struct TapRecognizer {
    press: Option<Press>,
}

impl TapRecognizer {
    /// Forgets the press in progress: whatever it was going to be, it is
    /// not a tap any more (another finger landed, the Tool changed).
    pub fn cancel(&mut self) {
        self.press = None;
    }

    /// Follows `event`. Returns what the pointer just turned out to be
    /// doing, if anything.
    pub fn track(&mut self, event: PointerEvent) -> Option<Recognized> {
        match event.phase {
            PointerPhase::Down => {
                // A second pointer joining a press never makes a tap of its
                // own; the first keeps its press, unless this is that very
                // pointer again, whose end was never reported.
                if self.press.is_none_or(|p| p.pointer == event.id) {
                    self.press = Some(Press {
                        pointer: event.id,
                        x: event.x,
                        y: event.y,
                        kind: event.kind,
                        at: event.timestamp,
                        moved: false,
                    });
                }
                None
            }
            PointerPhase::Move => {
                if let Some(press) = self.press.as_mut().filter(|p| p.pointer == event.id) {
                    let slop = match press.kind {
                        PointerKind::Finger => FINGER_SLOP,
                        PointerKind::Pencil | PointerKind::Mouse => PRECISE_SLOP,
                    };
                    let away = (event.x - press.x).hypot(event.y - press.y) > slop;
                    if away && !press.moved {
                        press.moved = true;
                        return Some(Recognized::DragBegan(Origin {
                            x: press.x,
                            y: press.y,
                            kind: press.kind,
                        }));
                    }
                }
                None
            }
            PointerPhase::Up => {
                let press = self.press.filter(|p| p.pointer == event.id)?;
                self.press = None;
                (!press.moved).then_some(Recognized::Tap(Tap {
                    x: press.x,
                    y: press.y,
                    kind: press.kind,
                    short: event.timestamp - press.at <= SHORT_TAP,
                }))
            }
            PointerPhase::Cancel => {
                if self.press.is_some_and(|p| p.pointer == event.id) {
                    self.press = None;
                }
                None
            }
        }
    }
}
