use super::*;

impl Editor {
    /// The text view to show over the Canvas, if a text is being edited, with
    /// where it is, what it says and how it looks (see
    /// [`crate::text_editing`]). A tap with the text Tool opens it at the tap
    /// (or on the text that is there), and so does a double-tap (a
    /// double-click) on a text; a double-tap on a rectangle, diamond or
    /// ellipse opens it on the label of the shape, as does
    /// [`Editor::edit_selected_text`]. It closes when the text is committed
    /// or cancelled, or by a tap elsewhere, which commits. The position is in view
    /// coordinates: clients read it with the viewport after every input and
    /// command. Needs a [`Editor::set_text_measurer`]: without one no text
    /// can be edited.
    pub fn text_editor(&self) -> Option<TextEditor> {
        self.text
            .as_ref()?
            .editor(&self.scene, self.navigation.viewport())
    }

    /// Tells the core what the text view says now: the text is laid out
    /// again, and the editor follows it. Clients call it on every change of
    /// the text view. Does nothing unless a text is being edited.
    pub fn set_editing_text(&mut self, text: &str) {
        let (Some(editing), Some(measurer)) = (self.text.as_mut(), self.measurer.as_ref()) else {
            return;
        };
        if editing.set_text(&mut self.scene, &mut self.entropy, text, &*measurer.0) {
            self.changed();
        }
    }

    /// Ends the editing of the text and keeps it: a text that is new, or has
    /// been changed, is one undo step, and becomes the Selection (for a
    /// label, its shape does). A text with
    /// nothing in it but blanks is not kept: one that is new is never made,
    /// one that was there is deleted. Does nothing unless a text is being
    /// edited.
    pub fn commit_text(&mut self) {
        let Some(editing) = self.text.take() else {
            return;
        };
        let id = editing.id().to_owned();
        let container = editing.container_id().map(str::to_owned);
        let step = editing.finish(&mut self.scene, &mut self.entropy);
        let kept = self
            .scene
            .element_by_id(&id)
            .is_some_and(|element| !element.is_deleted());
        // The shape of a label is selected after it, kept or not; a text of
        // its own only if it is kept. With the Tool lock the text Tool waits
        // for the next text.
        let selected = container.or_else(|| kept.then_some(id));
        if let Some(selected) = selected.filter(|_| !self.tool_lock) {
            self.selection.select([selected]);
        }
        if let Some(step) = step {
            self.history.record(step, &self.scene, &self.selection);
        }
        self.overlay_revision += 1;
        self.changed();
    }

    /// Ends the editing of the text and keeps it as it was before: a text
    /// that is new is never made. Does nothing unless a text is being edited.
    pub fn cancel_text(&mut self) {
        if let Some(editing) = self.text.take() {
            editing.cancel(&mut self.scene, &mut self.entropy);
            self.overlay_revision += 1;
            self.changed();
        }
    }

    /// Opens the text view on the text `existing` if there is one, else on a
    /// new text at `at` in the Scene.
    pub(super) fn begin_text(&mut self, existing: Option<String>, at: Point) {
        let Some(measurer) = self.measurer.as_ref() else {
            return;
        };
        if let Some(id) = existing.filter(|id| self.scene.element_by_id(id).is_some()) {
            return self.begin_existing_text(&id);
        }
        let editing = TextEditing::begin_new(
            Step::begin(&self.selection),
            &mut self.scene,
            &mut self.entropy,
            &self.style,
            at,
            &*measurer.0,
        );
        self.open_text(editing);
    }

    /// Opens the text view on the text of its own `id`.
    pub(super) fn begin_existing_text(&mut self, id: &str) {
        if self.measurer.is_none() {
            return;
        }
        let Some(element) = self.scene.element_by_id(id) else {
            return;
        };
        let step = Step::begin(&self.selection);
        let editing = TextEditing::begin_existing(step, &self.scene, &element, None);
        self.open_text(editing);
    }

    /// Opens the text view on the label of the shape `container_id`, or on a
    /// new one if it has none. Does nothing if the shape cannot hold text.
    pub(super) fn begin_label(&mut self, container_id: &str) {
        let Some(measurer) = self.measurer.as_ref() else {
            return;
        };
        let holds_text = self
            .scene
            .element_by_id(container_id)
            .is_some_and(|e| text::takes_label(e.kind()) && !e.is_deleted());
        if !holds_text {
            return;
        }
        let step = Step::begin(&self.selection);
        let editing = match text::label_of(&self.scene, container_id) {
            Some(label) => {
                let container = self.scene.element_by_id(container_id);
                Some(TextEditing::begin_existing(
                    step,
                    &self.scene,
                    &label,
                    container.as_ref(),
                ))
            }
            None => TextEditing::begin_label(
                step,
                &mut self.scene,
                &mut self.entropy,
                &self.style,
                container_id,
                &*measurer.0,
            ),
        };
        if let Some(editing) = editing {
            self.open_text(editing);
        }
    }

    /// Shows the text view for `editing`. The Selection is empty while a
    /// text is edited, and the Tool returns to selection unless it is
    /// locked.
    pub(super) fn open_text(&mut self, editing: TextEditing) {
        self.text = Some(editing);
        self.selection.clear();
        self.editing_group = None;
        self.sync_points();
        if !self.tool_lock {
            self.assign_tool(Tool::Selection);
        }
        self.overlay_revision += 1;
        self.changed();
    }

    /// The command to edit the text of the Selection (Enter on a Mac): the
    /// text of a selected text, or the label of a selected rectangle,
    /// diamond or ellipse, which is made if it has none. Does nothing for any
    /// other Selection, and while a text is edited.
    pub fn edit_selected_text(&mut self) {
        if self.text.is_some() || self.is_gesturing() {
            return;
        }
        let [id] = self.selection.ids() else {
            return;
        };
        let id = id.clone();
        if self.is_free_text(&id) {
            self.begin_existing_text(&id);
        } else {
            self.begin_label(&id);
        }
    }

    /// Whether the Element `id` is a text of its own: one that is not the
    /// label of a container, which is edited with it.
    pub(super) fn is_free_text(&self, id: &str) -> bool {
        self.scene
            .element_by_id(id)
            .is_some_and(|e| e.kind() == ElementType::Text && e.container_id().is_none())
    }

    /// Follows the pointer for a tap that opens the text view (with the text
    /// Tool, on a text if there is one at the tap) or, if it is open, closes
    /// it, keeping the text.
    pub(super) fn follow_text_tap(&mut self, event: PointerEvent) {
        let Some(Recognized::Tap(tap)) = self.taps.track(event) else {
            return;
        };
        if self.text.is_some() {
            self.commit_text();
            return;
        }
        let at = self.navigation.viewport().scene_at(tap.x, tap.y);
        let existing = self
            .hit(tap.x, tap.y, tap.kind)
            .filter(|id| self.is_free_text(id));
        self.begin_text(existing, at);
    }
}
