//! Navigation: turns raw input into viewport changes (ADR-0006).
//!
//! * Mac scroll wheel / trackpad: scroll pans; with ctrl or cmd it zooms like
//!   excalidraw.com. A trackpad pinch zooms.
//! * Touch: two fingers pan and pinch-zoom around their centre. The gesture
//!   is recognised here from raw finger events, so every client behaves the
//!   same. Pencil and mouse never navigate (the hand Tool and Pencil-only
//!   drawing mode will add their own panning).

use crate::input::{MagnifyEvent, PointerEvent, PointerKind, PointerPhase, ScrollEvent};
use crate::viewport::Viewport;

/// One zoom step of Excalidraw (`ZOOM_STEP`); a wheel tick moves the zoom by
/// at most this much.
const MAX_WHEEL_ZOOM_STEP: f64 = 0.1;
/// Wheel deltas of this size and larger zoom at full amplification.
const FULL_WHEEL_DELTA: f64 = 20.0;

/// Owns the [`Viewport`] and everything needed to change it from input.
#[derive(Debug, Default)]
pub(crate) struct Navigation {
    viewport: Viewport,
    /// Fingers currently down, in the order they landed.
    fingers: Vec<Finger>,
    /// Set while the first two of `fingers` are navigating.
    pinch: Option<Pinch>,
}

#[derive(Debug, Clone, Copy)]
struct Finger {
    id: u64,
    x: f64,
    y: f64,
}

/// A two-finger gesture, relative to the moment it started. Measuring from
/// the start rather than from the previous event keeps zoom limits from
/// accumulating: pinching beyond a limit and back follows the fingers again
/// right away.
#[derive(Debug, Clone, Copy)]
struct Pinch {
    ids: [u64; 2],
    /// The Scene point at the fingers' centre when the gesture started; it
    /// stays under the centre for the whole gesture.
    scene_at_centre: (f64, f64),
    start_zoom: f64,
    start_distance: f64,
}

impl Navigation {
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn pointer(&mut self, event: PointerEvent) {
        if event.kind != PointerKind::Finger {
            return;
        }
        match event.phase {
            PointerPhase::Down => {
                // A finger whose end was never reported starts over.
                self.fingers.retain(|f| f.id != event.id);
                self.fingers.push(Finger {
                    id: event.id,
                    x: event.x,
                    y: event.y,
                });
                self.pinch = None;
                self.retarget_pinch();
            }
            PointerPhase::Move => {
                if let Some(finger) = self.fingers.iter_mut().find(|f| f.id == event.id) {
                    finger.x = event.x;
                    finger.y = event.y;
                    self.follow_pinch();
                }
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.fingers.retain(|f| f.id != event.id);
                self.retarget_pinch();
            }
        }
    }

    pub fn scroll(&mut self, event: ScrollEvent) {
        let zoom_modifier = event.modifiers.ctrl || event.modifiers.meta;
        let (dx, dy) = (event.dx, event.dy);
        if event.modifiers.shift {
            // Shift pans horizontally, ctrl/cmd+shift vertically. Macs turn
            // shift+wheel into a horizontal delta, so either axis counts.
            let delta = if dy != 0.0 { dy } else { dx };
            if zoom_modifier {
                self.viewport.scroll_by(0.0, -delta);
            } else {
                self.viewport.scroll_by(-delta, 0.0);
            }
        } else if zoom_modifier && dy != 0.0 {
            self.zoom_by_wheel(event.x, event.y, dy);
        } else {
            self.viewport.scroll_by(-dx, -dy);
        }
    }

    pub fn magnify(&mut self, event: MagnifyEvent) {
        let zoom = self.viewport.zoom() * (1.0 + event.magnification);
        self.viewport.zoom_at(event.x, event.y, zoom);
    }

    /// Zooms like Excalidraw's wheel handler: small steps, larger ones the
    /// further in you are. `dy > 0` (content moving down) zooms in.
    fn zoom_by_wheel(&mut self, x: f64, y: f64, dy: f64) {
        // Excalidraw's formula is in browser wheel deltas, which point the
        // other way: positive zooms out.
        let wheel = -dy;
        let zoom = self.viewport.zoom();
        let step = wheel.clamp(-MAX_WHEEL_ZOOM_STEP * 100.0, MAX_WHEEL_ZOOM_STEP * 100.0);
        // Beyond 100 % each tick gets a boost, scaled down for the small
        // deltas of a trackpad.
        let boost =
            zoom.max(1.0).log10() * -wheel.signum() * (wheel.abs() / FULL_WHEEL_DELTA).min(1.0);
        self.viewport.zoom_at(x, y, zoom - step / 100.0 + boost);
    }

    /// Makes the first two fingers down the pinch, if that is not already so.
    /// A new pair starts from the viewport as it is now, so the view never
    /// jumps when fingers come and go.
    fn retarget_pinch(&mut self) {
        let &[a, b, ..] = self.fingers.as_slice() else {
            self.pinch = None;
            return;
        };
        if self.pinch.is_some_and(|p| p.ids == [a.id, b.id]) {
            return;
        }
        let centre = centre(a, b);
        self.pinch = Some(Pinch {
            ids: [a.id, b.id],
            scene_at_centre: self.viewport.scene_at(centre.0, centre.1),
            start_zoom: self.viewport.zoom(),
            start_distance: distance(a, b),
        });
    }

    /// Applies the pinch to the fingers' current positions.
    fn follow_pinch(&mut self) {
        let Some(pinch) = self.pinch else { return };
        let (Some(a), Some(b)) = (self.finger(pinch.ids[0]), self.finger(pinch.ids[1])) else {
            return;
        };
        // Two fingers started on the same spot have no scale to compare with.
        let scale = if pinch.start_distance > 0.0 {
            distance(a, b) / pinch.start_distance
        } else {
            1.0
        };
        self.viewport.pin(
            pinch.scene_at_centre,
            centre(a, b),
            pinch.start_zoom * scale,
        );
    }

    fn finger(&self, id: u64) -> Option<Finger> {
        self.fingers.iter().find(|f| f.id == id).copied()
    }
}

fn centre(a: Finger, b: Finger) -> (f64, f64) {
    ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn distance(a: Finger, b: Finger) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
