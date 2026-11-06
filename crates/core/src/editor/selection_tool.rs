//! The selection Tool's pointer handling: taps, groups, and the
//! `begin_*` / `follow_*` / `drop_*` of each of its gestures.
//!
//! The four `follow_*` functions (move, transform, point drag, box select)
//! deliberately stay explicit instead of sharing one "unchanged / changed /
//! finished / cancelled" form: each borrows a different gesture field and
//! calls a different `drag_to` signature (shift, modifiers and measurer, or
//! none), and box select records no history Step but a Selection change. A
//! shared helper would need closures over disjoint `self` fields and would
//! read longer than the ~20 lines each takes now.

use super::*;

impl Editor {
    /// Selects the Element a tap hits, or clears the Selection if the tap
    /// was a deliberate one on empty Canvas (see [`crate::tap`]); moves the
    /// Selection with a drag that began on an Element, and box-selects with
    /// one that began on empty Canvas.
    ///
    /// Only the selection Tool taps and drags, and never with two fingers
    /// down: they navigate, and must not disturb the Selection, nor leave a
    /// move half done. A finger in Pencil-only drawing taps like the
    /// selection Tool does whatever the Tool is, but its drags are the
    /// pan's, see [`crate::pencil_only`].
    pub(super) fn select(&mut self, event: PointerEvent) {
        let finger = self.pencil_only.restricts(event.kind);
        let tool = if finger { Tool::Selection } else { self.tool };
        let tapping = matches!(tool, Tool::Selection | Tool::Text);
        // A pan of space or a mouse button took the pointer: what it
        // interrupted is gone, but the pan is no tap, not even a right-click,
        // and leaves the taps before it as they were.
        if tapping && !self.navigation.is_pinching() && self.navigation.is_pan_overriding_tool() {
            self.taps.abandon_press();
            self.drop_move();
            self.drop_transform();
            self.drop_point_drag();
            self.drop_box_select();
            return;
        }
        if !tapping || self.navigation.is_pinching() {
            self.taps.cancel();
            self.drop_move();
            self.drop_transform();
            self.drop_point_drag();
            self.drop_box_select();
            return;
        }
        if tool == Tool::Text || self.text.is_some() {
            self.follow_text_tap(event);
            return;
        }
        self.follow_move(event);
        self.follow_transform(event);
        self.follow_point_drag(event);
        self.follow_box_select(event);
        match self.taps.track(event) {
            Some(Recognized::Tap(tap)) => self.tap(tap, event.modifiers.shift),
            Some(Recognized::DragBegan(origin)) if !finger => self.begin_drag(origin, event),
            Some(Recognized::DragBegan(_)) | None => {}
        }
    }

    /// With `shift`, a tap on an Element adds it to the Selection or, if it
    /// is in it, takes it out, and a tap on empty Canvas leaves the Selection
    /// alone.
    pub(super) fn tap(&mut self, tap: Tap, shift: bool) {
        // Before the handles: they are as big as a small text, which would
        // otherwise never be edited by a finger.
        if tap.double && !shift && self.edit_text_tapped(tap) {
            return;
        }
        // A tap on a handle is a grab that never dragged: it neither selects
        // nor clears the Selection. On a point handle it selects the point.
        if let Some((handles, grab)) = self.grabbed_point(tap.x, tap.y, tap.kind) {
            if let Grab::Point(index) = grab {
                if shift {
                    self.points.toggle(handles.id(), index);
                } else {
                    self.points.select_only(handles.id(), index);
                }
                self.overlay_revision += 1;
            }
            return;
        }
        // Anywhere else, as in Excalidraw's linear editor, a tap unselects
        // the points, so that `delete` takes the whole line again.
        if !shift && self.points.clear() {
            self.overlay_revision += 1;
        }
        if self.grabbed_handle(tap.x, tap.y, tap.kind).is_some() {
            return;
        }
        let hit = self.hit(tap.x, tap.y, tap.kind);
        let before = self.selection.clone();
        match hit {
            // A double-tap on an Element of a selected group goes into it.
            Some(id) if tap.double && !shift && self.enter_group(&id) => {}
            Some(id) if shift => {
                // Tapping outside the group that was gone into leaves it.
                self.editing_group = self.editing_around(&id);
                let unit = self.unit_of(&id);
                self.selection.toggle_all(&unit);
            }
            Some(id) => {
                self.select_unit(&id, false);
            }
            None if tap.is_short() && !shift => {
                self.selection.clear();
                self.editing_group = None;
            }
            None => {}
        }
        if self.selection != before {
            self.overlay_revision += 1;
        }
    }

    /// Opens the text view on what a double-tap hit, unless the double-tap
    /// goes into a group instead: on a text of its own, or on the label of a
    /// shape (made if it has none; the inside of a hollow shape counts as the
    /// shape). Returns whether it did.
    pub(super) fn edit_text_tapped(&mut self, tap: Tap) -> bool {
        let hit = self.hit(tap.x, tap.y, tap.kind);
        let target = match hit {
            Some(id) => Some(id),
            None => {
                let at = self.navigation.viewport().scene_at(tap.x, tap.y);
                self.label_container_at(at)
            }
        };
        let Some(id) = target else {
            return false;
        };
        let editable = self.is_free_text(&id)
            || self
                .scene
                .element_by_id(&id)
                .is_some_and(|e| text::takes_label(e.kind()));
        let goes_into_group = group::entered(
            &self.scene,
            &self.selection,
            &id,
            self.editing_group.as_deref(),
        )
        .is_some();
        if !editable || goes_into_group {
            return false;
        }
        if self.is_free_text(&id) {
            self.begin_existing_text(&id);
        } else {
            self.begin_label(&id);
        }
        self.text.is_some()
    }

    /// The topmost shape that can hold a label with `at` (in the Scene) in
    /// its box, hollow or not.
    pub(super) fn label_container_at(&self, at: Point) -> Option<String> {
        self.scene
            .elements()
            .rev()
            .filter(hit_test::is_selectable)
            .filter(|e| text::takes_label(e.kind()))
            .find(|e| Frame::of(e).contains_turned(at, e.angle(), 0.0))
            .map(|e| e.id().to_owned())
    }

    /// The group that was gone into, if it is one the Element `id` is in:
    /// selecting anything else leaves it.
    pub(super) fn editing_around(&self, id: &str) -> Option<String> {
        let editing = self.editing_group.as_deref()?;
        let element = self.scene.element_by_id(id)?;
        element.is_in_group(editing).then(|| editing.to_owned())
    }

    /// What selecting the Element `id` selects: it, and its group (see
    /// [`crate::group`]).
    pub(super) fn unit_of(&self, id: &str) -> Vec<String> {
        group::expand(&self.scene, &[id.to_owned()], self.editing_group.as_deref())
    }

    /// Selects what selecting the Element `id` selects, in place of the
    /// Selection or, if `additive`, in addition to it. Selecting outside the
    /// group that was gone into leaves it. Returns whether the Selection
    /// changed.
    pub(super) fn select_unit(&mut self, id: &str, additive: bool) -> bool {
        self.editing_group = self.editing_around(id);
        let unit = self.unit_of(id);
        if additive {
            let kept = self.selection.ids().iter().cloned();
            self.selection.select(kept.chain(unit).collect::<Vec<_>>())
        } else {
            self.selection.select(unit)
        }
    }

    /// Goes into the selected group the Element `id` is in, if there is one,
    /// and selects the Element (or its group inside the group that was gone
    /// into). Returns whether it did.
    pub(super) fn enter_group(&mut self, id: &str) -> bool {
        let Some(entered) = group::entered(
            &self.scene,
            &self.selection,
            id,
            self.editing_group.as_deref(),
        ) else {
            return false;
        };
        self.editing_group = Some(entered);
        let unit = self.unit_of(id);
        self.selection.select(unit);
        true
    }

    /// Leaves the group that was gone into unless every selected Element is
    /// still in it.
    pub(super) fn sync_editing(&mut self) {
        let Some(editing) = self.editing_group.as_deref() else {
            return;
        };
        let inside = !self.selection.is_empty()
            && self.selection.ids().iter().all(|id| {
                self.scene
                    .element_by_id(id)
                    .is_some_and(|e| e.is_in_group(editing))
            });
        if !inside {
            self.editing_group = None;
        }
    }

    /// The id of the topmost Element at the view point `(x, y)`, if any.
    pub(super) fn hit(&self, x: f64, y: f64, kind: PointerKind) -> Option<String> {
        let viewport = self.navigation.viewport();
        let reach = Reach::new(kind, viewport.zoom());
        let point = viewport.scene_at(x, y);
        hit_test::topmost(&self.scene, &self.selection, point, reach)
            .map(|element| element.id().to_owned())
    }

    /// The transform handle a pointer of `kind` that went down at the view
    /// point `(x, y)` takes hold of, if any.
    pub(super) fn grabbed_handle(
        &self,
        x: f64,
        y: f64,
        kind: PointerKind,
    ) -> Option<(TransformBox, Handle)> {
        let viewport = self.navigation.viewport();
        let transform_box = TransformBox::of(&self.scene, &self.selection)?;
        let handle = transform_box.grab(viewport.scene_at(x, y), kind, viewport.zoom())?;
        Some((transform_box, handle))
    }

    /// The handles of the points of the selected line or arrow, if there is
    /// one, for a pointer of `kind`.
    pub(super) fn point_handles(&self, kind: PointerKind) -> Option<PointHandles> {
        let zoom = self.navigation.viewport().zoom();
        PointHandles::of(&self.scene, &self.selection, kind, zoom)
    }

    /// The point handles, and which of them, a pointer of `kind` that went
    /// down at the view point `(x, y)` takes hold of, if any.
    pub(super) fn grabbed_point(
        &self,
        x: f64,
        y: f64,
        kind: PointerKind,
    ) -> Option<(PointHandles, Grab)> {
        let viewport = self.navigation.viewport();
        let handles = self.point_handles(kind)?;
        let grab = handles.grab(viewport.scene_at(x, y), kind, viewport.zoom())?;
        Some((handles, grab))
    }

    /// Starts moving points of the selected line or arrow if the drag began
    /// on a handle of one: the point is selected, if it is not yet, and moves
    /// with the other selected points; the middle of a segment gets a point
    /// that moves (see [`PointHandles::drag`]).
    pub(super) fn begin_point_drag(&mut self, origin: Origin, event: PointerEvent) {
        let Some((handles, grab)) = self.grabbed_point(origin.x, origin.y, origin.kind) else {
            return;
        };
        self.overlay_revision += 1;
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        self.point_dragging = Some(handles.drag(
            Step::begin(&self.selection),
            &self.scene,
            event.id,
            start,
            &mut self.points,
            grab,
            event.modifiers.shift,
            binding::reach(origin.kind, self.navigation.viewport().zoom()),
        ));
        self.follow_point_drag(event);
    }

    /// Takes the points along with the pointer that moves them.
    pub(super) fn follow_point_drag(&mut self, event: PointerEvent) {
        let Some(dragging) = self
            .point_dragging
            .as_mut()
            .filter(|d| d.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if dragging.drag_to(&mut self.scene, &mut self.entropy, to) {
                    self.changed();
                }
            }
            PointerPhase::Up => {
                if let Some(dragging) = self.point_dragging.take() {
                    self.history
                        .record(dragging.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_point_drag(),
        }
    }

    /// Puts the points being dragged, if any, back where they were, and
    /// selects again the points that were selected before the drag.
    pub(super) fn drop_point_drag(&mut self) {
        if let Some(dragging) = self.point_dragging.take() {
            self.points = dragging.cancel(&mut self.scene, &mut self.entropy);
            self.overlay_revision += 1;
            self.changed();
        }
    }

    /// The indices of the points of the selected line or arrow that are
    /// selected: tapping a point selects it, shift adds to them, tapping
    /// anywhere else unselects them, and dragging one moves them all. Empty
    /// unless a line or arrow is the only Element selected.
    pub fn selected_points(&self) -> &[usize] {
        self.points.indices()
    }

    /// Deletes the selected points of the selected line or arrow, for
    /// [`Editor::delete_selection`]. A line or arrow keeps at least two
    /// points: if fewer would be left, it is deleted with its label. The
    /// point before the first one deleted is selected next.
    pub(super) fn delete_selected_points(&mut self) {
        let Some(handles) = self.point_handles(self.last_pointer) else {
            return;
        };
        let deleted = self.points.indices().to_vec();
        let Some(&first) = deleted.iter().min() else {
            return;
        };
        let mut step = Step::begin(&self.selection);
        step.touch(&self.scene, handles.id());
        let remaining = handles.without(&deleted);
        let last = handles.points().len() - 1;
        if remaining.len() < 2 {
            delete::delete(
                &mut self.scene,
                &mut self.entropy,
                &mut step,
                &self.selection,
            );
            self.selection.clear();
            self.points.clear();
        } else {
            // The point next to an end that is deleted becomes the end, and
            // is not bound to anything.
            for (index, end) in [(0, End::Start), (last, End::End)] {
                if deleted.contains(&index) {
                    binding::unbind(
                        &mut self.scene,
                        &mut self.entropy,
                        &mut step,
                        handles.id(),
                        end,
                    );
                }
            }
            edit_points::reshape(
                &mut self.scene,
                &mut self.entropy,
                handles.handle(),
                &remaining,
            );
            binding::follow(&mut self.scene, &mut self.entropy, handles.id());
            self.points
                .select_only(handles.id(), first.saturating_sub(1));
        }
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed();
    }

    /// Makes the highlight the shape an arrow end is over, if one is being put
    /// somewhere.
    pub(super) fn sync_highlight(&mut self) {
        let current = self
            .linear
            .as_ref()
            .and_then(LinearDrawing::highlight)
            .or_else(|| self.point_dragging.as_ref().and_then(PointDrag::highlight))
            .map(str::to_owned);
        if current != self.highlight {
            self.highlight = current;
            self.overlay_revision += 1;
        }
    }

    /// Forgets the selected points that the Selection no longer has.
    pub(super) fn sync_points(&mut self) {
        if self.points.retain_existing(&self.scene, &self.selection) {
            self.overlay_revision += 1;
        }
    }

    /// Starts resizing or rotating the Selection if the drag began on a
    /// transform handle of it.
    pub(super) fn begin_transform(&mut self, origin: Origin, event: PointerEvent) {
        let Some((transform_box, handle)) = self.grabbed_handle(origin.x, origin.y, origin.kind)
        else {
            return;
        };
        let viewport = self.navigation.viewport();
        let start = viewport.scene_at(origin.x, origin.y);
        self.transforming = Some(TransformGesture::begin(
            Step::begin(&self.selection),
            &self.scene,
            &self.selection,
            event.id,
            transform_box,
            handle,
            start,
        ));
        self.follow_transform(event);
    }

    /// Takes the Selection along with the handle its pointer holds.
    pub(super) fn follow_transform(&mut self, event: PointerEvent) {
        let Some(transforming) = self
            .transforming
            .as_mut()
            .filter(|t| t.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if transforming.drag_to(
                    &mut self.scene,
                    &mut self.entropy,
                    to,
                    event.modifiers,
                    self.measurer.as_ref().map(|m| m.0.as_ref()),
                ) {
                    self.changed();
                }
            }
            PointerPhase::Up => {
                if let Some(transforming) = self.transforming.take() {
                    self.history
                        .record(transforming.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_transform(),
        }
    }

    /// Puts the Selection being transformed, if any, back as it was.
    pub(super) fn drop_transform(&mut self) {
        if let Some(transforming) = self.transforming.take() {
            transforming.cancel(&mut self.scene, &mut self.entropy);
            self.changed();
        }
    }

    /// Starts what a drag of the selection Tool is for, by where it began: on
    /// a transform handle it resizes or rotates the Selection, in the box
    /// around a Selection of several Elements or on an Element it moves, and
    /// on empty Canvas it box-selects. `event` is the step that turned the
    /// press into a drag.
    pub(super) fn begin_drag(&mut self, origin: Origin, event: PointerEvent) {
        self.begin_transform(origin, event);
        if self.transforming.is_some() {
            return;
        }
        self.begin_point_drag(origin, event);
        if self.point_dragging.is_some() {
            return;
        }
        if self.in_selection_box(origin) {
            return self.begin_move(None, origin, event);
        }
        match self.hit(origin.x, origin.y, origin.kind) {
            Some(id) => self.begin_move(Some(&id), origin, event),
            None => self.begin_box_select(origin, event),
        }
    }

    /// Whether a press at `origin` is in the box around a Selection of
    /// several Elements, where a drag moves them all, whatever is under it
    /// (Excalidraw's `isHittingCommonBoundingBoxOfSelectedElements`). A
    /// single Element's box is part of hitting it, see
    /// [`hit_test::topmost`].
    pub(super) fn in_selection_box(&self, origin: Origin) -> bool {
        if self.selection.ids().len() < 2 {
            return false;
        }
        let viewport = self.navigation.viewport();
        TransformBox::of(&self.scene, &self.selection).is_some_and(|transform_box| {
            transform_box.encloses(viewport.scene_at(origin.x, origin.y), viewport.zoom())
        })
    }

    /// Starts moving the Selection by a drag that began on the Element `on`,
    /// if any: on the Selection itself, or else on an Element that becomes
    /// the Selection, or with shift joins it.
    pub(super) fn begin_move(&mut self, on: Option<&str>, origin: Origin, event: PointerEvent) {
        // Undoing the move also undoes the selecting it began with.
        let step = Step::begin(&self.selection);
        self.drag_editing = self.editing_group.clone();
        let changed = match on {
            Some(id) if !self.selection.contains(id) => self.select_unit(id, event.modifiers.shift),
            _ => false,
        };
        if changed {
            self.overlay_revision += 1;
        }
        let viewport = self.navigation.viewport();
        let start = viewport.scene_at(origin.x, origin.y);
        self.moving = Some(MoveGesture::begin(
            step,
            &self.scene,
            &self.selection,
            event.id,
            start,
        ));
        self.follow_move(event);
    }

    /// Takes the Selection along with the pointer that moves it.
    pub(super) fn follow_move(&mut self, event: PointerEvent) {
        let Some(moving) = self.moving.as_mut().filter(|m| m.pointer() == event.id) else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                let lock_axis = event.modifiers.shift;
                if moving.drag_to(&mut self.scene, &mut self.entropy, to, lock_axis) {
                    self.changed();
                }
            }
            PointerPhase::Up => {
                if let Some(moving) = self.moving.take() {
                    self.history
                        .record(moving.finish(), &self.scene, &self.selection);
                }
            }
            PointerPhase::Cancel => self.drop_move(),
        }
    }

    /// Puts the Selection being moved, if any, back where it was, and selects
    /// again what was selected before the drag began.
    pub(super) fn drop_move(&mut self) {
        if let Some(moving) = self.moving.take() {
            let selection = moving.cancel(&mut self.scene, &mut self.entropy);
            self.show_selection(selection);
            self.editing_group = self.drag_editing.take();
            self.changed();
        }
    }

    /// Starts a box at where the drag began on empty Canvas; with shift it
    /// adds to the Selection instead of replacing it.
    pub(super) fn begin_box_select(&mut self, origin: Origin, event: PointerEvent) {
        let start = self.navigation.viewport().scene_at(origin.x, origin.y);
        self.drag_editing = self.editing_group.clone();
        self.box_selecting = Some(BoxSelect::begin(
            event.id,
            start,
            &self.selection,
            self.editing_group.as_deref(),
            event.modifiers.shift,
        ));
        self.follow_box_select(event);
    }

    /// Stretches the box to the pointer that draws it, and selects what it
    /// encloses.
    pub(super) fn follow_box_select(&mut self, event: PointerEvent) {
        let Some(boxing) = self
            .box_selecting
            .as_mut()
            .filter(|b| b.pointer() == event.id)
        else {
            return;
        };
        match event.phase {
            PointerPhase::Down => {}
            PointerPhase::Move => {
                let to = self.navigation.viewport().scene_at(event.x, event.y);
                if boxing.drag_to(to) {
                    let selection = boxing.selection(&self.scene);
                    self.show_selection(selection);
                    self.overlay_revision += 1;
                }
            }
            PointerPhase::Up => {
                self.box_selecting = None;
                self.overlay_revision += 1;
            }
            PointerPhase::Cancel => self.drop_box_select(),
        }
    }

    /// Takes the box being dragged, if any, away, and the Selection back to
    /// what it was before it.
    pub(super) fn drop_box_select(&mut self) {
        if let Some(boxing) = self.box_selecting.take() {
            self.show_selection(boxing.cancel());
            self.editing_group = self.drag_editing.take();
            self.overlay_revision += 1;
        }
    }

    pub(super) fn show_selection(&mut self, selection: Selection) {
        if self.selection != selection {
            self.selection = selection;
            self.overlay_revision += 1;
        }
    }
}
