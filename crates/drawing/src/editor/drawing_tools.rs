use super::*;

impl Editor {
    /// Follows the pointer with the shape being drawn, or starts one.
    pub(super) fn draw_shape(&mut self, event: PointerEvent) {
        // Two fingers navigate; they never draw, and a drawing one of them
        // began turns into the gesture.
        if self.navigation.suspends_tool() {
            self.drop_shape();
            return;
        }
        let drawing_pointer = self.shape.as_ref().map(ShapeGesture::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        match event.phase {
            PointerPhase::Down if drawing_pointer.is_none() => {
                if let Some(kind) = self.tool.shape() {
                    self.shape = Some(ShapeGesture::begin(
                        Step::begin(&self.selection),
                        &mut self.scene,
                        &mut self.entropy,
                        &self.style,
                        kind,
                        event.id,
                        at,
                    ));
                    self.changed();
                    self.dimensions_changed();
                }
            }
            PointerPhase::Move if drawing_pointer == Some(event.id) => {
                let square = event.modifiers.shift;
                let moved = self.shape.as_mut().is_some_and(|shape| {
                    shape.drag_to(&mut self.scene, &mut self.entropy, at, square)
                });
                if moved {
                    self.changed();
                    // The Dimensions follow the pointer.
                    self.dimensions_changed();
                }
            }
            PointerPhase::Up if drawing_pointer == Some(event.id) => {
                let Some(shape) = self.shape.take() else {
                    return;
                };
                // The Dimensions end with the gesture.
                self.dimensions_changed();
                let id = shape.id().to_owned();
                match shape.finish(&mut self.scene) {
                    Outcome::Drawn(step) => {
                        // As on excalidraw.com the new shape is selected, to
                        // be adjusted right away, and the Tool returns to
                        // selection; with the Tool lock neither, so that the
                        // next shape can follow.
                        let selected = (!self.tool_lock).then_some(id);
                        if self.selection.select(selected) {
                            self.overlay_revision += 1;
                        }
                        self.history.record(step, &self.scene, &self.selection);
                        if !self.tool_lock {
                            self.assign_tool(Tool::Selection);
                        }
                    }
                    Outcome::Dropped => self.changed(),
                }
            }
            PointerPhase::Cancel if drawing_pointer == Some(event.id) => self.drop_shape(),
            _ => {}
        }
    }

    /// Follows the pointer with the line or arrow being drawn, or starts one
    /// (see [`crate::draw_linear`]).
    pub(super) fn draw_linear(&mut self, event: PointerEvent) {
        let zoom = self.navigation.viewport().zoom();
        // Two fingers navigate: they place no points, and a line that one of
        // them began is gone. A line that is waiting for its next point
        // waits on.
        if self.navigation.suspends_tool() {
            if let Some(linear) = self.linear.as_mut() {
                let update = linear.interrupt(&mut self.scene, &mut self.entropy);
                self.linear_updated(update);
            }
            return;
        }
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        if let Some(linear) = self.linear.as_mut() {
            let update = linear.follow(&mut self.scene, &mut self.entropy, event, at, zoom);
            self.linear_updated(update);
        } else if event.phase == PointerPhase::Down
            && let Some(kind) = self.tool.linear()
        {
            self.linear = Some(LinearDrawing::begin(
                Step::begin(&self.selection),
                &mut self.scene,
                &mut self.entropy,
                &self.style,
                kind,
                event.id,
                event.kind,
                at,
                zoom,
            ));
            self.changed();
        }
    }

    /// Does what the line being drawn needs after a step of a pointer.
    pub(super) fn linear_updated(&mut self, update: Update) {
        match update {
            Update::Unchanged => {}
            Update::Changed => self.changed(),
            Update::Finished => self.finish_linear(),
            Update::Abandoned => self.drop_linear(),
        }
    }

    /// Finishes the line being drawn, if any, with the points it has: it
    /// stays in the Scene and is the latest thing that can be undone. Nothing
    /// is left of a line without two points. Returns whether there is a line.
    pub(super) fn end_linear(&mut self) -> bool {
        let Some(linear) = self.linear.take() else {
            return false;
        };
        let zoom = self.navigation.viewport().zoom();
        let id = linear.id().to_owned();
        match linear.finish(&mut self.scene, &mut self.entropy, zoom) {
            Outcome::Drawn(step) => {
                // Selected like a new shape, to adjust its ends and style.
                if !self.tool_lock {
                    self.selection.select(Some(id));
                }
                self.history.record(step, &self.scene, &self.selection);
                self.changed();
                true
            }
            Outcome::Dropped => {
                self.changed();
                false
            }
        }
    }

    /// Finishes the line being drawn, and with it the Tool's work: it returns
    /// to selection unless locked.
    pub(super) fn finish_linear(&mut self) {
        if self.end_linear() && !self.tool_lock {
            self.assign_tool(Tool::Selection);
        }
    }

    /// Finishes the text being edited, and the line being drawn if it is
    /// waiting for further points: what commands on the Scene do first.
    pub(super) fn finish_placing(&mut self) {
        self.commit_text();
        if self.linear.as_ref().is_some_and(LinearDrawing::is_placing) {
            self.finish_linear();
            self.sync_highlight();
        }
    }

    /// Takes the line being drawn, if any, out of the Scene again.
    pub(super) fn drop_linear(&mut self) {
        if let Some(linear) = self.linear.take() {
            linear.cancel(&mut self.scene, &mut self.entropy);
            self.changed();
        }
    }

    /// Follows the pointer with the stroke being drawn, or starts one (see
    /// [`crate::draw_freedraw`]). The Tool returns to selection after the
    /// stroke unless the Tool lock is on.
    pub(super) fn draw_stroke(&mut self, event: PointerEvent) {
        // Two fingers navigate, and end the stroke being drawn, whichever
        // pointer draws it (also the Pencil): a short one is dropped as the
        // mark of a finger landing, a longer one is kept as drawn.
        if self.navigation.suspends_tool() {
            if let Some(stroke) = self.stroke.take() {
                if stroke.is_spike() {
                    stroke.cancel(&mut self.scene);
                    self.changed();
                } else {
                    self.history
                        .record(stroke.finish(), &self.scene, &self.selection);
                }
            }
            return;
        }
        let drawing_pointer = self.stroke.as_ref().map(StrokeDrawing::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        match event.phase {
            PointerPhase::Down if drawing_pointer.is_none() && self.tool == Tool::Freedraw => {
                self.stroke = Some(StrokeDrawing::begin(
                    Step::begin(&self.selection),
                    &mut self.scene,
                    &mut self.entropy,
                    &self.style,
                    event,
                    at,
                ));
                self.changed();
            }
            PointerPhase::Move if drawing_pointer == Some(event.id) => {
                let moved = self.stroke.as_mut().is_some_and(|stroke| {
                    stroke.extend(&mut self.scene, &mut self.entropy, event, at)
                });
                if moved {
                    self.changed();
                }
            }
            PointerPhase::Up if drawing_pointer == Some(event.id) => {
                if let Some(stroke) = self.stroke.take() {
                    let step = stroke.lift(&mut self.scene, &mut self.entropy, event, at);
                    self.history.record(step, &self.scene, &self.selection);
                    if !self.tool_lock {
                        self.assign_tool(Tool::Selection);
                    }
                    self.changed();
                }
            }
            PointerPhase::Cancel if drawing_pointer == Some(event.id) => self.drop_stroke(),
            _ => {}
        }
    }

    /// Follows the pointer with the erase, or starts one (see
    /// [`crate::erase`]). The Tool returns to selection after the erase unless
    /// the Tool lock is on.
    pub(super) fn erase(&mut self, event: PointerEvent) {
        // Two fingers navigate, and an erase one of them began is called off:
        // nothing is deleted by a finger that was only landing.
        if self.navigation.suspends_tool() {
            self.drop_erase();
            return;
        }
        let erasing_pointer = self.erasing.as_ref().map(Erasing::pointer);
        let at = self.navigation.viewport().scene_at(event.x, event.y);
        let zoom = self.navigation.viewport().zoom();
        match event.phase {
            PointerPhase::Down if erasing_pointer.is_none() && self.tool == Tool::Eraser => {
                self.erasing = Some(Erasing::begin(Step::begin(&self.selection), event, at));
            }
            PointerPhase::Move if erasing_pointer == Some(event.id) => {
                let previewing = self
                    .erasing
                    .as_mut()
                    .is_some_and(|erasing| erasing.drag_to(&self.scene, at, zoom));
                if previewing {
                    self.changed();
                }
            }
            PointerPhase::Up if erasing_pointer == Some(event.id) => {
                if let Some(erasing) = self.erasing.take() {
                    let step = erasing.finish(&mut self.scene, &mut self.entropy, at, zoom);
                    // What was erased cannot stay selected.
                    let remaining = history::without_gone(self.selection.clone(), &self.scene);
                    self.show_selection(remaining);
                    self.history.record(step, &self.scene, &self.selection);
                    if !self.tool_lock {
                        self.assign_tool(Tool::Selection);
                    }
                    self.changed();
                }
            }
            PointerPhase::Cancel if erasing_pointer == Some(event.id) => self.drop_erase(),
            _ => {}
        }
    }

    /// Calls off the erase in progress, if any: what was previewed as about
    /// to be erased is shown as it is again.
    pub(super) fn drop_erase(&mut self) {
        if self.erasing.take().is_some() {
            self.changed();
        }
    }

    /// Takes the stroke being drawn, if any, out of the Scene again.
    pub(super) fn drop_stroke(&mut self) {
        if let Some(stroke) = self.stroke.take() {
            stroke.cancel(&mut self.scene);
            self.changed();
        }
    }

    /// Takes the shape being drawn, if any, out of the Scene again.
    pub(super) fn drop_shape(&mut self) {
        if let Some(shape) = self.shape.take() {
            shape.cancel(&mut self.scene);
            self.changed();
            self.dimensions_changed();
        }
    }
}
