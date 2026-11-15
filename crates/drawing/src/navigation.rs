//! Navigation: turns raw input into viewport changes (ADR-0006).
//!
//! * Mac scroll wheel / trackpad: scroll pans; with ctrl or cmd it zooms like
//!   excalidraw.com. A trackpad pinch zooms.
//! * Touch: two fingers pan and pinch-zoom around their centre. The gesture
//!   is recognised here from raw finger events, so every client behaves the
//!   same. Three fingers are the system's undo and redo gestures (swipe,
//!   double-tap): the third one takes back what the first two had moved, and
//!   no finger navigates again until all have lifted.
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
    /// Set from a third finger landing until every finger has lifted: the
    /// fingers are making a system gesture, not navigating.
    three_fingers: bool,
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
    /// The viewport before the gesture, for a third finger to go back to.
    start_viewport: Viewport,
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
        if self.is_pinching() {
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

    /// Whether two or more fingers are down, or fingers are left of three:
    /// two pan and zoom, three are a system gesture, and no other input may
    /// act on what they touch.
    pub fn is_pinching(&self) -> bool {
        self.pinch.is_some() || self.three_fingers
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
                if self.drag.is_none() && !self.is_pinching() && others == 0 {
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
                if self.fingers.len() >= 3 && !self.three_fingers {
                    self.three_fingers = true;
                    if let Some(pinch) = self.pinch.take() {
                        self.viewport = pinch.start_viewport;
                    }
                }
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
                if self.fingers.is_empty() {
                    self.three_fingers = false;
                }
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

    /// Makes the first two fingers down the pinch, if that is not already so
    /// and no three-finger gesture is under way.
    /// A new pair starts from the viewport as it is now, so the view never
    /// jumps when fingers come and go.
    fn retarget_pinch(&mut self) {
        let (&[a, b, ..], false) = (self.fingers.as_slice(), self.three_fingers) else {
            // Fewer than two fingers, or a system gesture: nothing navigates.
            self.pinch = None;
            return;
        };
        if self.pinch.is_some_and(|p| p.ids == [a.id, b.id]) {
            return;
        }
        let centre = centre(a, b);
        self.pinch = Some(Pinch {
            ids: [a.id, b.id],
            start_viewport: self.viewport,
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

#[cfg(test)]
mod tests {
    mod mouse_pan {
        //! Space+drag, wheel-button drag and right-button drag pan the Canvas with
        //! any Tool (ADR-0006), tested at the Editor seam with recorded pointer
        //! sequences (see `select.rs` for their format, `common::replay` for the
        //! `right` / `middle` button flags and the `space` lines): pointer events in,
        //! viewport, Selection, Tool, saved Drawing and undo history out.
        //!
        //! A drag of (50, 25) moves the content by that much, so the view's origin
        //! goes to (-50, -25).

        use std::sync::Arc;

        use crate::{Editor, Modifiers, ScrollEvent, Tool};

        use crate::test_support as common;
        use common::{FixedWidth, editor_with, replay, tap};

        /// A filled rectangle at (100, 100), 100 x 50.
        const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

        /// An editor with the rectangle `a` selected and a measurer, for text.
        fn selected() -> Editor {
            let mut editor = editor_with(&[RECTANGLE]);
            editor.set_text_measurer(Arc::new(FixedWidth));
            tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
            assert_eq!(editor.selection(), ["a"]);
            editor
        }

        /// What a pan must leave as it found it.
        #[derive(Debug, PartialEq)]
        struct Untouched {
            /// The saved Drawing but for versions, which cancelled edits bump.
            drawing: String,
            selection: Vec<String>,
            tool: Tool,
            can_undo: bool,
            can_redo: bool,
        }

        impl Untouched {
            fn of(editor: &Editor) -> Self {
                Self {
                    drawing: String::from_utf8(editor.save())
                        .unwrap()
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("\"version"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    selection: editor.selection().to_vec(),
                    tool: editor.tool(),
                    can_undo: editor.can_undo(),
                    can_redo: editor.can_redo(),
                }
            }
        }

        fn origin(editor: &Editor) -> (f64, f64) {
            editor.viewport().origin()
        }

        /// A drag of pointer 1 from (150, 125) to (200, 150) in `kind` with `flags`.
        fn drag(kind: &str, flags: &str) -> String {
            format!(
                "1.00 down 1 {kind} 150 125 {flags}
         1.10 move 1 {kind} 175 140 {flags}
         1.20 move 1 {kind} 200 150 {flags}
         1.30 up 1 {kind} 200 150 {flags}"
            )
        }

        // --- Space ----------------------------------------------------------------

        #[test]
        fn space_and_drag_pans_with_every_tool_and_changes_nothing_else() {
            let tools = [
                Tool::Selection,
                Tool::Rectangle,
                Tool::Diamond,
                Tool::Freedraw,
                Tool::Eraser,
                Tool::Arrow,
            ];
            for tool in tools {
                let mut editor = selected();
                editor.set_tool(tool);
                let before = Untouched::of(&editor);

                replay(&mut editor, "space down");
                replay(&mut editor, &drag("mouse", ""));
                replay(&mut editor, "space up");

                assert_eq!(origin(&editor), (-50.0, -25.0), "{tool:?}");
                assert_eq!(Untouched::of(&editor), before, "{tool:?}");
            }
        }

        #[test]
        fn space_and_drag_on_the_selection_pans_instead_of_moving_it() {
            let mut editor = selected();
            let before = Untouched::of(&editor);

            replay(&mut editor, "space down");
            replay(&mut editor, &drag("mouse", ""));

            assert_eq!(origin(&editor), (-50.0, -25.0));
            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn space_and_drag_pans_while_a_line_waits_for_its_next_point() {
            let mut editor = selected();
            editor.set_tool(Tool::Line);
            tap(&mut editor, "mouse", (300.0, 300.0), 0.5);
            let before = Untouched::of(&editor);

            replay(&mut editor, "space down");
            replay(&mut editor, &drag("mouse", ""));

            assert_eq!(origin(&editor), (-50.0, -25.0));
            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn space_and_drag_pans_with_a_finger_and_with_a_pencil() {
            for kind in ["finger", "pencil"] {
                let mut editor = selected();
                editor.set_tool(Tool::Rectangle);
                let before = Untouched::of(&editor);

                replay(&mut editor, "space down");
                replay(&mut editor, &drag(kind, ""));

                assert_eq!(origin(&editor), (-50.0, -25.0), "{kind}");
                assert_eq!(Untouched::of(&editor), before, "{kind}");
            }
        }

        #[test]
        fn space_and_drag_pans_with_a_pencil_in_pencil_only_drawing() {
            let mut editor = selected();
            editor.set_pencil_only_drawing(true);
            editor.set_tool(Tool::Rectangle);
            let before = Untouched::of(&editor);

            replay(&mut editor, "space down");
            replay(&mut editor, &drag("pencil", ""));

            assert_eq!(origin(&editor), (-50.0, -25.0));
            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn a_drag_without_space_still_draws() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);

            replay(&mut editor, "space down");
            replay(&mut editor, "space up");
            replay(&mut editor, &drag("mouse", ""));

            assert_eq!(origin(&editor), (0.0, 0.0));
            assert_eq!(editor.element_count(), 2);
        }

        #[test]
        fn pressing_space_mid_drag_does_not_affect_that_drag() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);

            replay(
                &mut editor,
                "1.00 down 1 mouse 300 300
         1.10 move 1 mouse 350 330
         space down
         1.20 move 1 mouse 400 350
         1.30 up 1 mouse 400 350
         space up",
            );

            assert_eq!(origin(&editor), (0.0, 0.0));
            assert_eq!(editor.element_count(), 2, "the rectangle was drawn");
        }

        #[test]
        fn releasing_space_mid_drag_does_not_affect_that_drag() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);

            replay(
                &mut editor,
                "space down
         1.00 down 1 mouse 150 125
         space up
         1.10 move 1 mouse 175 140
         1.20 move 1 mouse 200 150
         1.30 up 1 mouse 200 150",
            );

            assert_eq!(origin(&editor), (-50.0, -25.0));
            assert_eq!(editor.element_count(), 1);
        }

        #[test]
        fn a_second_finger_during_a_space_drag_turns_it_into_the_two_finger_gesture() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);
            let before = Untouched::of(&editor);

            replay(
                &mut editor,
                "space down
         1.00 down 1 finger 500 500
         1.10 move 1 finger 510 500
         1.50 down 2 finger 520 500
         1.60 move 2 finger 560 500
         1.70 move 2 finger 600 500
         2.00 up 2 finger 600 500
         2.05 up 1 finger 510 500",
            );

            assert!(editor.viewport().zoom() > 1.0, "the fingers pinched");
            assert_eq!(Untouched::of(&editor), before);
        }

        // --- Wheel button ---------------------------------------------------------

        #[test]
        fn a_wheel_button_drag_pans_with_any_tool_and_changes_nothing_else() {
            for tool in [
                Tool::Selection,
                Tool::Rectangle,
                Tool::Freedraw,
                Tool::Eraser,
            ] {
                let mut editor = selected();
                editor.set_tool(tool);
                let before = Untouched::of(&editor);

                replay(&mut editor, &drag("mouse", "middle"));

                assert_eq!(origin(&editor), (-50.0, -25.0), "{tool:?}");
                assert_eq!(Untouched::of(&editor), before, "{tool:?}");
            }
        }

        #[test]
        fn a_wheel_button_press_cancels_a_drawing_that_the_primary_button_began() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);
            let before = Untouched::of(&editor);

            replay(
                &mut editor,
                "1.00 down 1 mouse 300 300
         1.10 move 1 mouse 350 330
         1.20 down 1 mouse 350 330 middle
         1.30 move 1 mouse 360 330 middle
         1.40 up 1 mouse 360 330 middle
         1.50 up 1 mouse 360 330",
            );

            assert_eq!(Untouched::of(&editor), before);
            assert_eq!(origin(&editor), (-10.0, 0.0));
        }

        #[test]
        fn scrolling_during_a_wheel_button_pan_zooms_around_the_pointer() {
            let mut editor = selected();
            replay(&mut editor, "1.00 down 1 mouse 150 125 middle");
            let scene_under_pointer = |e: &Editor| {
                let (ox, oy) = origin(e);
                let zoom = e.viewport().zoom();
                (ox + 150.0 / zoom, oy + 125.0 / zoom)
            };
            let before = scene_under_pointer(&editor);

            // No modifier: this would pan without the button.
            editor.scroll(ScrollEvent {
                x: 150.0,
                y: 125.0,
                dx: 0.0,
                dy: 20.0,
                modifiers: Modifiers::default(),
            });

            assert!(editor.viewport().zoom() > 1.0);
            let after = scene_under_pointer(&editor);
            assert!((after.0 - before.0).abs() < 1e-9 && (after.1 - before.1).abs() < 1e-9);
        }

        #[test]
        fn scrolling_without_a_wheel_button_pan_still_pans() {
            let mut editor = Editor::new();
            replay(&mut editor, "1.00 down 1 mouse 150 125 right");

            editor.scroll(ScrollEvent {
                x: 150.0,
                y: 125.0,
                dx: 0.0,
                dy: 20.0,
                modifiers: Modifiers::default(),
            });

            assert_eq!(editor.viewport().zoom(), 1.0);
            assert_eq!(origin(&editor), (0.0, -20.0));
        }

        // --- Right button ---------------------------------------------------------

        #[test]
        fn a_right_button_drag_pans_from_where_it_passes_five_points() {
            let mut editor = selected();
            editor.set_tool(Tool::Rectangle);
            let before = Untouched::of(&editor);

            replay(
                &mut editor,
                "1.00 down 1 mouse 150 125 right
         1.10 move 1 mouse 154 125 right
         1.20 move 1 mouse 160 125 right
         1.30 move 1 mouse 180 135 right
         1.40 up 1 mouse 180 135 right",
            );

            // Panning follows the pointer from (160, 125) on.
            assert_eq!(origin(&editor), (-20.0, -10.0));
            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn a_right_button_drag_of_less_than_five_points_does_not_pan() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "1.00 down 1 mouse 150 125 right
         1.10 move 1 mouse 153 128 right
         1.20 up 1 mouse 153 128 right",
            );

            assert_eq!(origin(&editor), (0.0, 0.0));
        }

        #[test]
        fn a_right_click_leaves_scene_selection_and_tool_alone() {
            for at in ["150 125", "600 600"] {
                let mut editor = selected();
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    &format!(
                        "1.00 down 1 mouse {at} right
                 1.05 up 1 mouse {at} right"
                    ),
                );

                assert_eq!(Untouched::of(&editor), before, "{at}");
                assert_eq!(origin(&editor), (0.0, 0.0));
            }
        }

        #[test]
        fn a_right_click_is_no_part_of_a_double_tap() {
            let mut editor = selected();
            editor.set_tool(Tool::Selection);

            // Were the right-click a tap, the left tap after it would be its second
            // and edit the rectangle's label.
            replay(
                &mut editor,
                "5.00 down 1 mouse 150 125 right
         5.05 up 1 mouse 150 125 right",
            );
            tap(&mut editor, "mouse", (150.0, 125.0), 5.2);

            assert!(editor.text_editor().is_none());
        }

        #[test]
        fn a_right_click_does_not_end_the_wait_for_a_double_tap() {
            let mut editor = selected();
            tap(&mut editor, "mouse", (150.0, 125.0), 5.0);

            replay(
                &mut editor,
                "5.10 down 1 mouse 600 600 right
         5.15 up 1 mouse 600 600 right",
            );
            tap(&mut editor, "mouse", (150.0, 125.0), 5.2);

            assert!(editor.text_editor().is_some(), "the second tap doubled");
        }

        // --- Text view ------------------------------------------------------------

        #[test]
        fn wheel_button_and_right_button_drags_pan_and_keep_the_text_view_open() {
            // The right button pans from where it passed five points, (175, 140).
            for (flags, expected) in [("middle", (-50.0, -25.0)), ("right", (-25.0, -10.0))] {
                let mut editor = Editor::new();
                editor.set_text_measurer(Arc::new(FixedWidth));
                editor.set_tool(Tool::Text);
                tap(&mut editor, "mouse", (300.0, 300.0), 1.0);
                assert!(editor.text_editor().is_some());

                replay(&mut editor, &drag("mouse", flags));

                assert_eq!(origin(&editor), expected, "{flags}");
                assert!(editor.text_editor().is_some(), "{flags}");
            }
        }
    }

    mod multi_touch {
        //! Two-finger gestures never disturb Selection or Tool (ADR-0006), tested at
        //! the Editor seam with recorded pointer sequences (see `select.rs` for their
        //! format): pointer events in, Selection, Tool, saved Drawing, undo history
        //! and viewport out.
        //!
        //! Every test starts some single-finger action, lands a second finger in the
        //! middle of it, and then checks that what the action had begun is gone
        //! without a trace while the two fingers navigated.

        use std::sync::Arc;

        use crate::{Editor, Tool};

        use crate::test_support as common;
        use common::{FixedWidth, editor_with, replay, tap};

        /// A filled rectangle at (100, 100), 100 x 50. Its transform handles are
        /// centred on (96, 96), (204, 96), (204, 154) and (96, 154), and the rotation
        /// handle at (150, 80).
        const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}"##;

        /// A second rectangle far to the right, at (400, 100).
        const OTHER: &str = r##"{"id":"b","type":"rectangle","x":400,"y":100,"width":100,"height":50,
    "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":9}"##;

        /// An editor with the rectangle `a` selected.
        fn selected() -> Editor {
            let mut editor = editor_with(&[RECTANGLE, OTHER]);
            tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
            assert_eq!(editor.selection(), ["a"]);
            editor
        }

        /// Everything a two-finger gesture must leave as it found it.
        #[derive(Debug, PartialEq)]
        struct Untouched {
            /// The saved Drawing, but for the versions of its Elements: cancelling an
            /// edit puts the Element back but never reuses a version, so that what
            /// clients cached for the abandoned edit is never mistaken for it.
            drawing: String,
            selection: Vec<String>,
            tool: Tool,
            can_undo: bool,
            can_redo: bool,
        }

        impl Untouched {
            fn of(editor: &Editor) -> Self {
                Self {
                    drawing: String::from_utf8(editor.save())
                        .unwrap()
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("\"version"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    selection: editor.selection().to_vec(),
                    tool: editor.tool(),
                    can_undo: editor.can_undo(),
                    can_redo: editor.can_redo(),
                }
            }
        }

        /// How the two fingers end: the one that landed second lifts first, or the
        /// other way round.
        const LIFT_SECOND_FIRST: &str = "2.00 up 2 finger 560 500\n2.05 up 1 finger {first}";
        const LIFT_FIRST_FIRST: &str = "2.00 up 1 finger {first}\n2.05 up 2 finger 560 500";

        /// Starts the single-finger action `begin` (events of finger 1, which end
        /// with it down at `first`), lands a second finger and pinches it out, and
        /// lifts both in either order. The Drawing, Selection, Tool and history must
        /// be as before, and the viewport must have followed the pinch.
        fn assert_pinch_disturbs_nothing(begin: &str, first: &str) {
            for lift in [LIFT_SECOND_FIRST, LIFT_FIRST_FIRST] {
                let mut editor = selected();
                let before = Untouched::of(&editor);
                let zoom = editor.viewport().zoom();

                replay(&mut editor, begin);
                replay(
                    &mut editor,
                    "1.50 down 2 finger 520 500
             1.60 move 2 finger 560 500
             1.70 move 2 finger 600 500",
                );
                replay(&mut editor, &lift.replace("{first}", first));

                assert_eq!(Untouched::of(&editor), before, "{lift}");
                assert!(editor.viewport().zoom() > zoom, "the fingers pinched");
            }
        }

        #[test]
        fn a_second_finger_mid_move_leaves_no_partial_move() {
            assert_pinch_disturbs_nothing(
                "1.00 down 1 finger 150 125
         1.10 move 1 finger 200 125
         1.20 move 1 finger 250 140",
                "250 140",
            );
        }

        #[test]
        fn a_second_finger_mid_move_of_an_unselected_element_leaves_it_unselected() {
            assert_pinch_disturbs_nothing(
                "1.00 down 1 finger 450 125
         1.10 move 1 finger 480 125
         1.20 move 1 finger 500 140",
                "500 140",
            );
        }

        #[test]
        fn a_second_finger_mid_resize_leaves_no_partial_resize() {
            assert_pinch_disturbs_nothing(
                "1.00 down 1 finger 206 156
         1.10 move 1 finger 230 180
         1.20 move 1 finger 256 206",
                "256 206",
            );
        }

        #[test]
        fn a_second_finger_mid_rotation_leaves_no_partial_rotation() {
            assert_pinch_disturbs_nothing(
                "1.00 down 1 finger 150 80
         1.10 move 1 finger 200 90
         1.20 move 1 finger 260 125",
                "260 125",
            );
        }

        #[test]
        fn a_second_finger_mid_box_select_restores_the_selection() {
            assert_pinch_disturbs_nothing(
                "1.00 down 1 finger 350 50
         1.10 move 1 finger 450 100
         1.20 move 1 finger 520 200",
                "520 200",
            );
        }

        #[test]
        fn a_second_finger_mid_draw_leaves_no_partial_element() {
            for tool in [Tool::Rectangle, Tool::Diamond, Tool::Ellipse] {
                let mut editor = selected();
                editor.set_tool(tool);
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    "1.00 down 1 finger 300 300
             1.10 move 1 finger 320 320
             1.20 move 1 finger 340 340
             1.50 down 2 finger 520 500
             1.60 move 2 finger 600 500
             2.00 up 2 finger 600 500
             2.05 up 1 finger 340 340",
                );

                assert_eq!(Untouched::of(&editor), before, "{tool:?}");
                assert_eq!(editor.element_count(), 2, "{tool:?}");
            }
        }

        #[test]
        fn a_second_finger_mid_line_leaves_no_partial_line() {
            for tool in [Tool::Line, Tool::Arrow] {
                let mut editor = selected();
                editor.set_tool(tool);
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    "1.00 down 1 finger 300 300
             1.10 move 1 finger 340 320
             1.20 move 1 finger 380 340
             1.50 down 2 finger 520 500
             1.60 move 2 finger 600 500
             2.00 up 2 finger 600 500
             2.05 up 1 finger 380 340",
                );

                assert_eq!(Untouched::of(&editor), before, "{tool:?}");
                assert_eq!(editor.element_count(), 2, "{tool:?}");
            }
        }

        /// A line from (100, 300) to (300, 300), with point handles on its ends.
        const LINE: &str = r#"{"id":"l","type":"line","x":100,"y":300,"width":200,"height":0,
    "points":[[0,0],[200,0]],"roughness":0,"roundness":null,"strokeWidth":2,
    "version":3,"versionNonce":11}"#;

        #[test]
        fn a_second_finger_mid_point_drag_leaves_the_line_as_it_was() {
            // The end point, and the middle, whose drag adds a point.
            for from in ["300 300", "200 300"] {
                let mut editor = editor_with(&[LINE]);
                tap(&mut editor, "mouse", (150.0, 300.0), 0.0);
                assert_eq!(editor.selection(), ["l"]);
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    &format!(
                        "1.00 down 1 finger {from}
                 1.10 move 1 finger 330 330
                 1.20 move 1 finger 360 360
                 1.50 down 2 finger 520 500
                 1.60 move 2 finger 600 500
                 2.00 up 2 finger 600 500
                 2.05 up 1 finger 360 360"
                    ),
                );

                assert_eq!(Untouched::of(&editor), before, "{from}");
                assert!(editor.selected_points().is_empty(), "{from}");
            }
        }

        #[test]
        fn a_second_finger_mid_erase_erases_nothing() {
            let mut editor = selected();
            editor.set_tool(Tool::Eraser);
            let before = Untouched::of(&editor);

            // The first finger crosses both rectangles before the second lands.
            replay(
                &mut editor,
                "1.00 down 1 finger 50 125
         1.10 move 1 finger 250 125
         1.20 move 1 finger 450 125
         1.50 down 2 finger 520 500
         1.60 move 2 finger 600 500
         2.00 up 2 finger 600 500
         2.05 up 1 finger 450 125",
            );

            assert_eq!(Untouched::of(&editor), before);
            assert_eq!(editor.element_count(), 2);
        }

        /// [`selected`] with the text Tool, able to edit text.
        fn with_text_tool() -> Editor {
            let mut editor = selected();
            editor.set_text_measurer(Arc::new(FixedWidth));
            editor.set_tool(Tool::Text);
            editor
        }

        #[test]
        fn a_tap_with_the_text_tool_opens_a_text() {
            // What the next test relies on to mean something.
            let mut editor = with_text_tool();
            tap(&mut editor, "finger", (300.0, 300.0), 1.0);
            assert!(editor.text_editor().is_some());
        }

        #[test]
        fn a_second_finger_during_a_text_tap_opens_no_text() {
            for lift in [LIFT_SECOND_FIRST, LIFT_FIRST_FIRST] {
                let mut editor = with_text_tool();
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    &format!(
                        "1.00 down 1 finger 300 300
                 1.01 down 2 finger 520 500
                 {}",
                        lift.replace("{first}", "300 300")
                    ),
                );

                assert!(editor.text_editor().is_none(), "{lift}");
                assert_eq!(Untouched::of(&editor), before, "{lift}");
            }
        }

        #[test]
        fn a_pinch_leaves_the_text_being_edited_open() {
            let mut editor = with_text_tool();
            tap(&mut editor, "finger", (300.0, 300.0), 0.5);
            editor.set_editing_text("hello");
            assert!(editor.text_editor().is_some());
            let selection = editor.selection().to_vec();

            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.10 down 2 finger 400 300
         1.20 move 2 finger 500 300
         1.30 up 2 finger 500 300
         1.40 up 1 finger 300 300",
            );

            let shown = editor.text_editor().expect("still editing");
            assert_eq!(shown.text, "hello");
            assert_eq!(editor.selection(), selection);
        }

        #[test]
        fn a_pinch_in_the_middle_of_a_hand_drag_keeps_the_tool() {
            let mut editor = selected();
            editor.set_tool(Tool::Hand);

            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.10 move 1 finger 320 300
         1.50 down 2 finger 520 500
         1.60 move 2 finger 600 500
         2.00 up 2 finger 600 500
         2.05 up 1 finger 320 300",
            );

            assert_eq!(editor.tool(), Tool::Hand);
            assert!(editor.selection().is_empty());
        }

        #[test]
        fn a_pinch_keeps_a_locked_shape_tool() {
            let mut editor = selected();
            editor.set_tool_lock(true);
            editor.set_tool(Tool::Ellipse);

            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.10 down 2 finger 400 300
         1.20 move 2 finger 500 300
         1.30 up 2 finger 500 300
         1.40 up 1 finger 300 300",
            );

            assert_eq!(editor.tool(), Tool::Ellipse);
            assert!(editor.tool_lock());
            assert!(editor.selection().is_empty());
        }

        #[test]
        fn the_finger_left_after_a_pinch_never_acts_on_what_it_touches() {
            // Each finger lands on something it could act on, then the other lifts,
            // and the one left drags over it.
            let drags = [
                ("on a handle", "206 156", "300 220"),
                ("on the rotation handle", "150 80", "260 125"),
                ("on the selected element", "150 125", "250 140"),
                ("on an unselected element", "450 125", "480 160"),
                ("on empty canvas", "300 300", "420 220"),
            ];
            for (what, from, to) in drags {
                let mut editor = selected();
                let before = Untouched::of(&editor);

                replay(
                    &mut editor,
                    &format!(
                        "1.00 down 1 finger {from}
                 1.10 down 2 finger 700 700
                 1.20 move 2 finger 720 700
                 1.30 up 2 finger 720 700
                 1.40 move 1 finger {to}
                 1.50 move 1 finger {from}
                 1.60 up 1 finger {from}"
                    ),
                );

                assert_eq!(Untouched::of(&editor), before, "{what}");
            }
        }

        #[test]
        fn lifting_after_a_pinch_never_counts_as_a_tap() {
            // A finger lands on empty canvas, a second one on an element; both lift
            // at once without moving, however briefly they were down.
            let mut editor = selected();
            let before = Untouched::of(&editor);

            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.01 down 2 finger 450 125
         1.02 up 2 finger 450 125
         1.03 up 1 finger 300 300",
            );
            assert_eq!(Untouched::of(&editor), before);

            // The other way round: first on an element, then on empty canvas.
            replay(
                &mut editor,
                "2.00 down 1 finger 450 125
         2.01 down 2 finger 300 300
         2.02 up 1 finger 450 125
         2.03 up 2 finger 300 300",
            );
            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn a_third_finger_never_taps_either() {
            let mut editor = selected();
            let before = Untouched::of(&editor);

            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.01 down 2 finger 400 300
         1.02 down 3 finger 450 125
         1.03 up 3 finger 450 125
         1.04 up 2 finger 400 300
         1.05 up 1 finger 300 300",
            );

            assert_eq!(Untouched::of(&editor), before);
        }

        #[test]
        fn editing_works_normally_after_a_pinch() {
            let mut editor = selected();
            replay(
                &mut editor,
                "1.00 down 1 finger 300 300
         1.10 down 2 finger 400 300
         1.20 move 2 finger 500 300
         1.30 up 2 finger 500 300
         1.40 up 1 finger 300 300",
            );
            let zoom = editor.viewport().zoom();

            // Tapping empty canvas clears the Selection again, and does not navigate.
            // The pinch doubled the zoom around (400, 300): the corner is far from
            // both rectangles.
            tap(&mut editor, "finger", (790.0, 590.0), 5.0);
            assert!(editor.selection().is_empty());
            assert_eq!(editor.viewport().zoom(), zoom);
        }
    }

    mod navigation {
        //! Pan and zoom (ADR-0006), tested at the Editor seam: raw input goes in, the
        //! viewport comes out. Pointer sequences are written as recorded event lines:
        //!
        //! ```text
        //! <down|move|up|cancel> <pointer id> <finger|pencil|mouse> <x> <y>
        //! ```
        //!
        //! Expected viewports are worked out by hand in the comments, not computed
        //! with the formulas of the code under test.

        use crate::{
            Editor, MagnifyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, ScrollEvent,
        };

        const EPSILON: f64 = 1e-9;

        /// Feeds a recorded pointer sequence to the editor, 10 ms per event.
        fn replay(editor: &mut Editor, recording: &str) {
            for (n, line) in recording
                .lines()
                .filter(|l| !l.trim().is_empty())
                .enumerate()
            {
                let f: Vec<&str> = line.split_whitespace().collect();
                let [phase, id, kind, x, y] = f[..] else {
                    panic!("bad recorded event: {line}")
                };
                editor.pointer(PointerEvent {
                    id: id.parse().unwrap(),
                    phase: match phase {
                        "down" => PointerPhase::Down,
                        "move" => PointerPhase::Move,
                        "up" => PointerPhase::Up,
                        "cancel" => PointerPhase::Cancel,
                        _ => panic!("unknown phase in: {line}"),
                    },
                    x: x.parse().unwrap(),
                    y: y.parse().unwrap(),
                    kind: match kind {
                        "finger" => PointerKind::Finger,
                        "pencil" => PointerKind::Pencil,
                        "mouse" => PointerKind::Mouse,
                        _ => panic!("unknown kind in: {line}"),
                    },
                    button: crate::PointerButton::Primary,
                    pressure: 0.5,
                    timestamp: n as f64 * 0.01,
                    modifiers: Modifiers::default(),
                });
            }
        }

        fn scroll(editor: &mut Editor, dx: f64, dy: f64, modifiers: Modifiers) {
            editor.scroll(ScrollEvent {
                x: 100.0,
                y: 100.0,
                dx,
                dy,
                modifiers,
            });
        }

        fn origin(editor: &Editor) -> (f64, f64) {
            editor.viewport().origin()
        }

        fn zoom(editor: &Editor) -> f64 {
            editor.viewport().zoom()
        }

        #[track_caller]
        fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
            assert!(
                (actual.0 - expected.0).abs() < EPSILON && (actual.1 - expected.1).abs() < EPSILON,
                "{actual:?} != {expected:?}"
            );
        }

        const CTRL: Modifiers = Modifiers { ctrl: true, ..NONE };
        const META: Modifiers = Modifiers { meta: true, ..NONE };
        const SHIFT: Modifiers = Modifiers {
            shift: true,
            ..NONE
        };
        const CTRL_SHIFT: Modifiers = Modifiers {
            ctrl: true,
            shift: true,
            ..NONE
        };
        const NONE: Modifiers = Modifiers {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        };

        #[test]
        fn a_new_editor_shows_the_scene_origin_unzoomed() {
            let editor = Editor::new();

            assert_eq!(origin(&editor), (0.0, 0.0));
            assert_eq!(zoom(&editor), 1.0);
        }

        // --- Mac scroll wheel and trackpad -------------------------------------

        #[test]
        fn scrolling_moves_the_content_with_the_fingers() {
            let mut editor = Editor::new();

            // Content moves 10 right, 20 down; so the view looks 10 left, 20 up.
            scroll(&mut editor, 10.0, 20.0, NONE);

            assert_close(origin(&editor), (-10.0, -20.0));
            assert_eq!(zoom(&editor), 1.0);
        }

        #[test]
        fn scrolling_covers_less_of_the_scene_when_zoomed_in() {
            let mut editor = Editor::new();
            editor.magnify(MagnifyEvent {
                x: 0.0,
                y: 0.0,
                magnification: 1.0,
            }); // zoom 2

            scroll(&mut editor, 10.0, 20.0, NONE);

            assert_close(origin(&editor), (-5.0, -10.0));
        }

        #[test]
        fn shift_scrolls_horizontally_and_ctrl_shift_vertically() {
            let mut editor = Editor::new();

            // Mac turns shift+wheel into a horizontal delta; either axis counts.
            scroll(&mut editor, 0.0, 30.0, SHIFT);
            assert_close(origin(&editor), (-30.0, 0.0));
            scroll(&mut editor, 7.0, 0.0, SHIFT);
            assert_close(origin(&editor), (-37.0, 0.0));
            scroll(&mut editor, 0.0, 10.0, CTRL_SHIFT);
            assert_close(origin(&editor), (-37.0, -10.0));
        }

        #[test]
        fn ctrl_or_cmd_scroll_zooms_in_excalidraws_steps() {
            for modifiers in [CTRL, META] {
                let mut editor = Editor::new();

                scroll(&mut editor, 0.0, 5.0, modifiers); // content down: zoom in
                assert!((zoom(&editor) - 1.05).abs() < EPSILON);

                let mut editor = Editor::new();
                scroll(&mut editor, 0.0, -5.0, modifiers);
                assert!((zoom(&editor) - 0.95).abs() < EPSILON);
            }
        }

        #[test]
        fn one_scroll_event_zooms_by_at_most_a_tenth() {
            let mut editor = Editor::new();

            scroll(&mut editor, 0.0, -500.0, CTRL);

            assert!((zoom(&editor) - 0.9).abs() < EPSILON);
        }

        #[test]
        fn zooming_in_is_faster_when_already_zoomed_in() {
            let mut editor = Editor::new();
            editor.magnify(MagnifyEvent {
                x: 0.0,
                y: 0.0,
                magnification: 9.0,
            }); // zoom 10

            // Excalidraw: a tick of 20 counts as 10/100 = 0.1, plus the extra
            // log10(10) * 1 = 1 for zooming in beyond 100%: 10 + 0.1 + 1 = 11.1.
            scroll(&mut editor, 0.0, 20.0, CTRL);

            assert!((zoom(&editor) - 11.1).abs() < EPSILON);
        }

        #[test]
        fn a_zoom_scroll_keeps_the_scene_under_the_pointer_in_place() {
            let mut editor = Editor::new();
            scroll(&mut editor, 0.0, 5.0, CTRL); // zoom 1.05 about (100, 100)

            // Scene point (100, 100) was under the pointer and must still be.
            // origin = 100 - 100 / 1.05
            let expected = 100.0 - 100.0 / 1.05;
            assert_close(origin(&editor), (expected, expected));
        }

        #[test]
        fn horizontal_only_scroll_with_ctrl_pans_instead_of_zooming() {
            let mut editor = Editor::new();

            scroll(&mut editor, 12.0, 0.0, CTRL);

            assert_eq!(zoom(&editor), 1.0);
            assert_close(origin(&editor), (-12.0, 0.0));
        }

        #[test]
        fn trackpad_pinch_zooms_around_the_pinch_centre() {
            let mut editor = Editor::new();

            editor.magnify(MagnifyEvent {
                x: 100.0,
                y: 50.0,
                magnification: 0.5,
            });

            // Zoom 1.5; scene point (100, 50) stays put: 100 - 100/1.5, 50 - 50/1.5.
            assert!((zoom(&editor) - 1.5).abs() < EPSILON);
            assert_close(origin(&editor), (100.0 / 3.0, 50.0 / 3.0));
        }

        #[test]
        fn every_zoom_is_rounded_to_six_decimals_like_excalidraws() {
            let mut editor = Editor::new();
            editor.magnify(MagnifyEvent {
                x: 0.0,
                y: 0.0,
                magnification: 0.123_456_789,
            });
            assert_eq!(zoom(&editor), 1.123_457);

            let ctrl = Modifiers {
                ctrl: true,
                ..Modifiers::default()
            };
            scroll(&mut editor, 0.0, 0.123_456_789, ctrl);
            // 1.123457 + 0.123456789 / 100 + log10(1.123457) * 0.123456789 / 20.
            assert_eq!(zoom(&editor), 1.125004);
        }

        // --- Zoom limits -------------------------------------------------------

        #[test]
        fn zoom_stops_at_excalidraws_limits() {
            let mut editor = Editor::new();
            for _ in 0..100 {
                editor.magnify(MagnifyEvent {
                    x: 0.0,
                    y: 0.0,
                    magnification: -0.5,
                });
            }
            assert_eq!(zoom(&editor), 0.1);

            for _ in 0..100 {
                editor.magnify(MagnifyEvent {
                    x: 0.0,
                    y: 0.0,
                    magnification: 1.0,
                });
            }
            assert_eq!(zoom(&editor), 30.0);
        }

        #[test]
        fn zooming_at_a_limit_leaves_the_viewport_alone() {
            let mut editor = Editor::new();
            for _ in 0..100 {
                scroll(&mut editor, 0.0, -50.0, CTRL);
            }
            let at_limit = editor.viewport();

            scroll(&mut editor, 0.0, -50.0, CTRL);

            assert_eq!(editor.viewport(), at_limit);
        }

        // --- Two-finger touch --------------------------------------------------

        #[test]
        fn one_finger_does_not_navigate() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 100 100
         move 1 finger 160 130
         up 1 finger 160 130",
            );

            assert_eq!(editor.viewport(), Editor::new().viewport());
        }

        #[test]
        fn two_fingers_moving_together_pan() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 130 110
         move 2 finger 230 110",
            );

            // Content follows the fingers 30 right and 10 down.
            assert_close(origin(&editor), (-30.0, -10.0));
            assert_eq!(zoom(&editor), 1.0);
        }

        #[test]
        fn spreading_two_fingers_zooms_around_their_centre() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 50 100
         move 2 finger 250 100",
            );

            // Distance 100 -> 200: zoom 2. The centre (150, 100) did not move, so the
            // scene point under it, (150, 100), stays: origin = 150 - 150/2, 100 - 100/2.
            assert_eq!(zoom(&editor), 2.0);
            assert_close(origin(&editor), (75.0, 50.0));
        }

        #[test]
        fn pinch_and_pan_combine() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 150 150
         move 2 finger 250 150",
            );
            // Pure pan by (50, 50) first.
            assert_close(origin(&editor), (-50.0, -50.0));

            replay(
                &mut editor,
                "move 1 finger 100 150
         move 2 finger 300 150",
            );
            // Distance 100 -> 200 about the centre (200, 150). The gesture grabbed the
            // scene point (150, 100) and keeps it under the centre:
            // origin = 150 - 200/2, 100 - 150/2.
            assert_eq!(zoom(&editor), 2.0);
            assert_close(origin(&editor), (50.0, 25.0));
        }

        #[test]
        fn pinch_zoom_is_clamped_and_does_not_accumulate_overshoot() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 0 100
         down 2 finger 10 100
         move 2 finger 2000 100",
            );
            assert_eq!(zoom(&editor), 30.0, "10 -> 2000 is far beyond the limit");

            // Coming back to 3x the start distance zooms to 3, not to 30/200.
            replay(&mut editor, "move 2 finger 30 100");
            assert_eq!(zoom(&editor), 3.0);
        }

        #[test]
        fn lifting_one_finger_ends_the_gesture() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 2 finger 220 100
         up 1 finger 100 100",
            );
            let after_gesture = editor.viewport();

            replay(&mut editor, "move 2 finger 400 300");

            assert_eq!(editor.viewport(), after_gesture);
        }

        #[test]
        fn a_new_gesture_continues_from_the_current_viewport() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 110 100
         move 2 finger 210 100
         up 1 finger 110 100
         up 2 finger 210 100
         down 3 finger 100 100
         down 4 finger 200 100
         move 3 finger 120 100
         move 4 finger 220 100",
            );

            assert_close(origin(&editor), (-30.0, 0.0));
        }

        // Three fingers are the system's undo and redo gestures (swipe, double-tap):
        // they must not navigate.

        #[test]
        fn three_fingers_do_not_navigate() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         move 1 finger 0 100
         move 2 finger 100 100
         move 3 finger 200 100
         up 1 finger 0 100
         up 2 finger 100 100
         up 3 finger 200 100",
            );

            assert_eq!(editor.viewport(), Editor::new().viewport());
        }

        #[test]
        fn a_third_finger_takes_back_what_the_first_two_moved() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 1 finger 50 120
         move 2 finger 250 120",
            );
            assert_ne!(editor.viewport(), Editor::new().viewport());

            replay(&mut editor, "down 3 finger 300 100");

            assert_eq!(editor.viewport(), Editor::new().viewport());
        }

        #[test]
        fn after_three_fingers_none_navigate_until_all_have_lifted() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         up 3 finger 300 100
         move 1 finger 110 100
         move 2 finger 210 100
         up 1 finger 110 100
         move 2 finger 400 300",
            );
            assert_eq!(editor.viewport(), Editor::new().viewport());

            replay(
                &mut editor,
                "up 2 finger 400 300
         down 4 finger 100 100
         down 5 finger 200 100
         move 4 finger 110 100
         move 5 finger 210 100",
            );
            assert_close(origin(&editor), (-10.0, 0.0));
        }

        #[test]
        fn a_third_finger_ends_a_hand_tool_drag() {
            let mut editor = Editor::new();
            editor.set_tool(crate::Tool::Hand);
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         down 3 finger 300 100
         up 3 finger 300 100
         up 2 finger 200 100
         move 1 finger 150 150",
            );

            assert_eq!(editor.viewport(), Editor::new().viewport());
        }

        #[test]
        fn a_cancelled_touch_ends_the_gesture() {
            let mut editor = Editor::new();
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         cancel 2 finger 200 100",
            );
            let before = editor.viewport();

            replay(&mut editor, "move 1 finger 300 300");

            assert_eq!(editor.viewport(), before);
        }

        #[test]
        fn only_fingers_navigate() {
            let mut editor = Editor::new();

            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 pencil 200 100
         move 2 pencil 400 100
         move 1 finger 0 0
         up 2 pencil 400 100",
            );
            assert_eq!(
                editor.viewport(),
                Editor::new().viewport(),
                "finger + pencil"
            );

            replay(
                &mut editor,
                "down 3 mouse 10 10
         move 3 mouse 50 50
         up 3 mouse 50 50",
            );
            assert_eq!(editor.viewport(), Editor::new().viewport(), "mouse drag");
        }

        #[test]
        fn a_lost_finger_does_not_poison_later_gestures() {
            let mut editor = Editor::new();
            // Finger 1 never reports its end (e.g. the app lost the touch).
            replay(&mut editor, "down 1 finger 0 0");
            replay(&mut editor, "down 1 finger 100 100");
            replay(&mut editor, "down 2 finger 200 100");
            replay(
                &mut editor,
                "move 1 finger 110 100
         move 2 finger 210 100",
            );

            assert_close(origin(&editor), (-10.0, 0.0));
        }

        // --- Geometry does not depend on the viewport ---------------------------

        #[test]
        fn navigating_leaves_the_scenes_geometry_untouched() {
            let mut editor = Editor::new();
            editor
                .load(
                    br#"{"type":"excalidraw","elements":[
                {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40}]}"#,
                )
                .unwrap();
            let before = editor.geometry();

            scroll(&mut editor, 40.0, 50.0, NONE);
            scroll(&mut editor, 0.0, 5.0, CTRL);
            replay(
                &mut editor,
                "down 1 finger 100 100
         down 2 finger 200 100
         move 2 finger 300 100",
            );
            let after = editor.geometry();

            assert_ne!(editor.viewport(), Editor::new().viewport());
            assert_eq!(before.parts(), after.parts());
            assert_eq!(before.verbs(), after.verbs());
            assert_eq!(before.coords(), after.coords());
        }
    }
}
