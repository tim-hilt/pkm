//! Navigation: turns raw input into viewport changes (ADR-0006).
//!
//! * Mac scroll wheel / trackpad: scroll pans; with ctrl or cmd it zooms like
//!   excalidraw.com. A trackpad pinch zooms.
//! * Touch: two fingers pan and pinch-zoom around their centre. The gesture
//!   is recognised here from raw finger events, so every client behaves the
//!   same.
//! * Hand Tool: one pointer of any kind (finger, Pencil, mouse) drags the
//!   Canvas. A second finger turns it into the two-finger gesture above.
//!   Without the hand Tool, Pencil and mouse never navigate. Fingers drag the
//!   Canvas like the hand Tool does in Pencil-only drawing.
//! * Mac mouse: with the space bar held, or with the wheel button, or with the
//!   right button once it has travelled more than [`SECONDARY_SLOP`] points,
//!   a drag pans whatever the Tool, and takes the pointer from it. Scrolling
//!   while the wheel button pans zooms. A right button released before that
//!   was a right-click, which does nothing. Fingers, Pencil and mouse alike
//!   pan with space.
//! * Zoom commands (in, out, reset, fit) act on the view as a whole, so they
//!   need to know its size.

use crate::input::{
    MagnifyEvent, PointerButton, PointerEvent, PointerKind, PointerPhase, ScrollEvent,
};
use crate::viewport::Viewport;

/// One zoom step of Excalidraw (`ZOOM_STEP`); a wheel tick moves the zoom by
/// at most this much.
const MAX_WHEEL_ZOOM_STEP: f64 = 0.1;
/// The zoom change of one zoom in / zoom out command (`ZOOM_STEP`).
const ZOOM_STEP: f64 = 0.1;
/// How far (view points) the right button must travel to pan rather than
/// click (Excalidraw's `AppPan`).
const SECONDARY_SLOP: f64 = 5.0;
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
    /// Set while a pointer is dragging the Canvas.
    drag: Option<Drag>,
    /// Whether the pan key (space) is held.
    pan_key: bool,
    /// Width and height of the Canvas in view points; zero until a client
    /// reports it.
    view_size: (f64, f64),
}

/// A rectangle in Scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// What started a drag of the Canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragSource {
    /// The hand Tool, or a finger in Pencil-only drawing.
    Hand,
    /// The pan key was held.
    PanKey,
    Wheel,
    Secondary,
}

/// A drag of the Canvas: the pointer driving it and where it was last seen.
#[derive(Debug, Clone, Copy)]
struct Drag {
    id: u64,
    x: f64,
    y: f64,
    source: DragSource,
    /// A secondary-button press that has not travelled [`SECONDARY_SLOP`]
    /// yet, and may still be a click. `x` and `y` are where it began.
    pending: bool,
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

    /// `hand_tool` tells whether this pointer drags the Canvas: the hand Tool
    /// is active, or it is a finger in Pencil-only drawing.
    pub fn pointer(&mut self, event: PointerEvent, hand_tool: bool) {
        if event.kind == PointerKind::Finger {
            self.track_finger(event);
        }
        // Two fingers belong to the pinch, which takes a drag over.
        if self.pinch.is_some() {
            self.drag = None;
        }
        self.follow_drag(event, hand_tool);
    }

    /// Width and height of the Canvas in view points; zero until a client
    /// reports it.
    pub fn view_size(&self) -> (f64, f64) {
        self.view_size
    }

    pub fn set_view_size(&mut self, width: f64, height: f64) {
        self.view_size = (width, height);
    }

    pub fn set_viewport(&mut self, origin: (f64, f64), zoom: f64) {
        if origin.0.is_finite() && origin.1.is_finite() && zoom.is_finite() {
            self.viewport.pin(origin, (0.0, 0.0), zoom);
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_about_centre(self.viewport.zoom() + ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_about_centre(self.viewport.zoom() - ZOOM_STEP);
    }

    pub fn reset_zoom(&mut self) {
        self.zoom_about_centre(1.0);
    }

    /// Centres `bounds` in the view, zoomed out far enough for them to fit
    /// but never in past 100 % (Excalidraw's "zoom to fit"). Does nothing
    /// until the view has a size.
    pub fn fit(&mut self, bounds: Bounds) {
        let (width, height) = self.view_size;
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        // A degenerate extent divides to infinity, which `min` leaves to 1.
        let zoom = (width / (bounds.max_x - bounds.min_x))
            .min(height / (bounds.max_y - bounds.min_y))
            .min(1.0);
        let centre = (
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
        );
        self.viewport.pin(centre, (width / 2.0, height / 2.0), zoom);
    }

    /// Whether two or more fingers are down: they pan and zoom, and no other
    /// input may act on what they touch.
    pub fn is_pinching(&self) -> bool {
        self.pinch.is_some()
    }

    /// Whether a drag of space, the wheel button or the right button is in
    /// progress: it pans whatever the Tool, which then gets no input.
    pub fn is_pan_overriding_tool(&self) -> bool {
        self.drag.is_some_and(|d| d.source != DragSource::Hand)
    }

    /// Whether the Canvas is really being navigated by pinching or by a pan
    /// that overrides the Tool: a right-button press that has not yet
    /// travelled [`SECONDARY_SLOP`] is only a click so far, as the long-press
    /// of a secondary click needs it to be.
    pub fn is_navigating(&self) -> bool {
        self.is_pinching()
            || self
                .drag
                .is_some_and(|d| d.source != DragSource::Hand && !d.pending)
    }

    /// Whether the Tool's gestures are suspended: pinching or overridden
    /// panning.
    pub fn suspends_tool(&self) -> bool {
        self.is_pinching() || self.is_pan_overriding_tool()
    }

    /// Whether a pointer is dragging the Canvas: a right-button press that
    /// has not travelled [`SECONDARY_SLOP`] yet does not count.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some_and(|d| !d.pending)
    }

    /// Whether the pan key (space) is held.
    pub fn pan_key(&self) -> bool {
        self.pan_key
    }

    /// The client reports the pan key (space) down or up. It only matters
    /// for drags that begin while it is held.
    pub fn set_pan_key(&mut self, held: bool) {
        self.pan_key = held;
    }

    /// Ends a hand Tool drag, e.g. because another Tool was chosen.
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    fn follow_drag(&mut self, event: PointerEvent, hand_tool: bool) {
        match event.phase {
            PointerPhase::Down => {
                let others = self.fingers.iter().filter(|f| f.id != event.id).count();
                // Only a pointer that is alone on the Canvas can start a drag.
                if self.drag.is_none() && self.pinch.is_none() && others == 0 {
                    self.drag = self.drag_source(event, hand_tool).map(|source| Drag {
                        id: event.id,
                        x: event.x,
                        y: event.y,
                        source,
                        pending: source == DragSource::Secondary,
                    });
                }
            }
            PointerPhase::Move => {
                if let Some(drag) = self.drag.as_mut().filter(|d| d.id == event.id) {
                    if drag.pending {
                        if (event.x - drag.x).hypot(event.y - drag.y) <= SECONDARY_SLOP {
                            return;
                        }
                        drag.pending = false;
                    } else {
                        self.viewport.scroll_by(drag.x - event.x, drag.y - event.y);
                    }
                    drag.x = event.x;
                    drag.y = event.y;
                }
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                if self.drag.is_some_and(|d| d.id == event.id) {
                    self.drag = None;
                }
            }
        }
    }

    /// What makes `event`, a press, drag the Canvas, if anything. Decided at
    /// the press: the pan key or the Tool changing later does not matter.
    fn drag_source(&self, event: PointerEvent, hand_tool: bool) -> Option<DragSource> {
        match event.button {
            PointerButton::Middle => Some(DragSource::Wheel),
            PointerButton::Secondary => Some(DragSource::Secondary),
            PointerButton::Primary if self.pan_key => Some(DragSource::PanKey),
            PointerButton::Primary => hand_tool.then_some(DragSource::Hand),
        }
    }

    fn track_finger(&mut self, event: PointerEvent) {
        match event.phase {
            PointerPhase::Down => {
                // A finger whose end was never reported starts over.
                self.fingers.retain(|f| f.id != event.id);
                self.fingers.push(Finger {
                    id: event.id,
                    x: event.x,
                    y: event.y,
                });
                // Re-anchor: the finger may have been part of the pinch.
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
        // Scrolling with the wheel button held zooms, whatever the modifiers.
        if self.drag.is_some_and(|d| d.source == DragSource::Wheel) {
            if dy != 0.0 {
                self.zoom_by_wheel(event.x, event.y, dy);
            }
            return;
        }
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

    /// Zooms the way the zoom commands do: keeping the middle of the view in
    /// place.
    fn zoom_about_centre(&mut self, zoom: f64) {
        let (width, height) = self.view_size;
        self.viewport.zoom_at(width / 2.0, height / 2.0, zoom);
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
