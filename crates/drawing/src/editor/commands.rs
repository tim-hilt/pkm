use super::*;

impl Editor {
    /// Duplicates the Selection (the `duplicate` command): each Element gets
    /// a copy 10 units right and down of it, as on excalidraw.com, and the
    /// copies become the Selection. One undo step. Does nothing with nothing
    /// selected, or while a pointer is drawing or moving. See
    /// [`crate::duplicate`].
    pub fn duplicate(&mut self) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        let copies = duplicate::duplicate(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        );
        if copies.is_empty() {
            return;
        }
        self.selection = copies;
        self.sync_editing();
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed();
    }

    /// The text to put on the clipboard for copying the Selection: Excalidraw's
    /// clipboard JSON (`excalidraw/clipboard`), with the labels of selected
    /// containers and the files of selected images, which excalidraw.com
    /// pastes. `None` with nothing selected. See [`crate::clipboard`].
    pub fn copy_selection(&self) -> Option<String> {
        clipboard::copy(&self.scene, &self.selection)
    }

    /// The id of the file of the selected image, if the Selection is exactly
    /// one image that has a file; its bytes are [`Editor::file_bytes`]. What
    /// clients offer "Copy Image" for.
    pub fn selected_image(&self) -> Option<String> {
        let mut chosen = self.selection.covered(&self.scene);
        let image = chosen.next().filter(|e| e.kind() == ElementType::Image)?;
        if chosen.next().is_some() {
            return None;
        }
        image.file_id().map(str::to_owned)
    }

    /// Copies the Selection (see [`Editor::copy_selection`]) and deletes it
    /// (see [`Editor::delete_selection`]), which is one undo step. `None`,
    /// and nothing deleted, with nothing selected or while a pointer is
    /// drawing or moving.
    pub fn cut_selection(&mut self) -> Option<String> {
        self.finish_placing();
        if self.is_gesturing() {
            return None;
        }
        let copied = self.copy_selection()?;
        self.delete_selection();
        Some(copied)
    }

    /// Pastes `text` in the middle of the view. See [`Editor::paste_at`].
    pub fn paste(&mut self, text: &str) -> bool {
        let (width, height) = self.navigation.view_size();
        self.paste_at(text, width / 2.0, height / 2.0)
    }

    /// Pastes `text`, if it is Excalidraw's clipboard JSON (from
    /// [`Editor::copy_selection`] or excalidraw.com): its Elements are added
    /// with new ids, with the middle of them at the point `(x, y)` of the
    /// view. They become the Selection and the Tool returns to selection.
    /// One undo step. Returns whether anything was pasted: `false` for any
    /// other text, for the client to treat as it likes, and while a pointer
    /// is drawing or moving. See [`crate::clipboard`].
    pub fn paste_at(&mut self, text: &str, x: f64, y: f64) -> bool {
        self.finish_placing();
        if self.is_gesturing() {
            return false;
        }
        let centre = self.navigation.viewport().scene_at(x, y);
        let mut step = Step::begin(&self.selection);
        let Some(ids) =
            clipboard::paste(&mut self.scene, &mut self.entropy, &mut step, text, centre)
        else {
            return false;
        };
        self.set_tool(Tool::Selection);
        self.editing_group = None;
        self.selection.select(ids);
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed();
        true
    }

    /// Puts the `images` (pasted or dropped; see [`crate::insert_image`]) in
    /// the middle of the view. See [`Editor::insert_images_at`].
    pub fn insert_images(&mut self, images: Vec<ImageFile>) {
        let (width, height) = self.navigation.view_size();
        self.insert_images_at(images, width / 2.0, height / 2.0);
    }

    /// Puts the `images` as Elements around the point `(x, y)` of the view
    /// (where they were dropped): their files are stored in the Drawing,
    /// they become the Selection and the Tool returns to selection. One undo
    /// step. Images that have no pixels or bytes are skipped. Does nothing
    /// while a pointer is drawing or moving.
    pub fn insert_images_at(&mut self, images: Vec<ImageFile>, x: f64, y: f64) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let viewport = self.navigation.viewport();
        let placement = Placement {
            view_height: self.navigation.view_size().1,
            zoom: viewport.zoom(),
            centre: viewport.scene_at(x, y),
        };
        let mut step = Step::begin(&self.selection);
        let ids = insert_image::insert(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.style,
            &placement,
            &images,
        );
        if ids.is_empty() {
            return;
        }
        self.set_tool(Tool::Selection);
        self.editing_group = None;
        self.selection.select(ids);
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed();
    }

    /// Deletes the Selection (the `delete` command), and with it the labels
    /// of its containers; arrows bound to what is deleted are unbound. With
    /// points of a line or arrow selected, deletes those points instead (see
    /// [`Editor::selected_points`]). One undo step, which selects what was
    /// deleted again. Does nothing with nothing selected, or while a pointer
    /// is drawing or moving. See [`crate::delete`].
    pub fn delete_selection(&mut self) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        if !self.points.indices().is_empty() {
            self.delete_selected_points();
            return;
        }
        let mut step = Step::begin(&self.selection);
        delete::delete(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
        );
        self.selection.clear();
        self.sync_editing();
        self.sync_points();
        self.history.record(step, &self.scene, &self.selection);
        self.overlay_revision += 1;
        self.changed();
    }

    /// Moves the Selection in z-order (the `bring forward`, `send backward`,
    /// `bring to front` and `send to back` commands), as Excalidraw does: see
    /// [`crate::z_order`]. One undo step. Does nothing with nothing selected,
    /// while a pointer is drawing or moving, or if the Selection is already
    /// as far as it goes.
    pub fn move_in_z_order(&mut self, how: ZOrder) {
        self.arrange(|scene, entropy, step, selection, editing| {
            z_order::move_selection(scene, entropy, step, selection, editing, how)
        });
    }

    /// Lines up the Selection's groups and Elements (the `align` commands),
    /// as Excalidraw does: see [`crate::align`]. One undo step. Does nothing
    /// unless [`Editor::can_align`], or while a pointer is drawing or moving.
    pub fn align(&mut self, how: Alignment) {
        self.arrange(|scene, entropy, step, selection, editing| {
            align::align(scene, entropy, step, selection, editing, how)
        });
    }

    /// Spaces the Selection's groups and Elements evenly (the `distribute`
    /// commands), as Excalidraw does: see [`crate::align`]. One undo step.
    /// Does nothing unless [`Editor::can_distribute`], or while a pointer is
    /// drawing or moving.
    pub fn distribute(&mut self, how: Distribution) {
        self.arrange(|scene, entropy, step, selection, editing| {
            align::distribute(scene, entropy, step, selection, editing, how)
        });
    }

    /// Whether aligning would do anything: two or more groups or Elements are
    /// selected (or one group of them). The Properties panel offers Align
    /// exactly then (see [`crate::Sections::align`]).
    pub fn can_align(&self) -> bool {
        self.arranged_units() > 1
    }

    /// Whether distributing would do anything: three or more are selected.
    pub fn can_distribute(&self) -> bool {
        self.arranged_units() > 2
    }

    /// How many groups or Elements align and distribute move as wholes.
    pub(super) fn arranged_units(&self) -> usize {
        align::unit_count(&self.scene, &self.selection, self.editing_group.as_deref())
    }

    /// Moves the Selection with `how` as one undo step, unless a pointer is
    /// drawing or moving or `how` moves nothing.
    fn arrange(
        &mut self,
        how: impl FnOnce(&mut Scene, &mut Entropy, &mut Step, &Selection, Option<&str>) -> bool,
    ) {
        self.finish_placing();
        if self.is_gesturing() || self.selection.is_empty() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        let editing = self.editing_group.as_deref();
        if !how(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            editing,
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        // Moved, or, after a z-order move, the handles of the Elements now
        // stand for others.
        self.changed();
    }

    /// Puts the selected Elements into one new group (the `group` command),
    /// as Excalidraw does: see [`crate::group`]. Tapping any of them then
    /// selects them all. One undo step. Does nothing unless two or more
    /// Elements are selected and not all of them in a group already (see
    /// [`Editor::can_group`]), or while a pointer is drawing or moving.
    pub fn group(&mut self) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        if !group::group(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        // The Elements moved up: the handles of some now stand for others.
        self.changed();
    }

    /// Whether the group command would do anything: two or more Elements are
    /// selected and no group holds all of them already. The Element menu and
    /// the Canvas's accessibility custom actions offer Group exactly then.
    pub fn can_group(&self) -> bool {
        group::can_group(&self.scene, &self.selection, self.editing_group.as_deref())
    }

    /// Whether the ungroup command would do anything: a group is selected.
    /// The Element menu and the Canvas's accessibility custom actions offer
    /// Ungroup exactly then.
    pub fn can_ungroup(&self) -> bool {
        group::can_ungroup(&self.scene, &self.selection, self.editing_group.as_deref())
    }

    /// Dissolves the groups that are selected (the `ungroup` command): the
    /// Elements stay selected, now without them. Groups inside and outside
    /// those stay. One undo step. Does nothing unless a group is selected
    /// (see [`Editor::can_ungroup`]), or while a pointer is drawing or
    /// moving.
    pub fn ungroup(&mut self) {
        self.finish_placing();
        if self.is_gesturing() {
            return;
        }
        let mut step = Step::begin(&self.selection);
        if !group::ungroup(
            &mut self.scene,
            &mut self.entropy,
            &mut step,
            &self.selection,
            self.editing_group.as_deref(),
        ) {
            return;
        }
        self.history.record(step, &self.scene, &self.selection);
        self.changed();
    }

    /// Finishes the line being placed point by point, as tapping its last
    /// point does, and tells whether there was one.
    fn finish_line_being_placed(&mut self) -> bool {
        let placing = self.linear.as_ref().is_some_and(LinearDrawing::is_placing);
        if placing {
            self.finish_placing();
        }
        placing
    }

    /// Carries out `command`, e.g. from a keyboard shortcut (see
    /// [`crate::shortcuts`]), exactly as the matching method does.
    pub fn perform(&mut self, command: Command) {
        match command {
            Command::SetTool(tool) => self.set_tool(tool),
            Command::ToggleToolLock => self.set_tool_lock(!self.tool_lock),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Duplicate => self.duplicate(),
            Command::Delete => self.delete_selection(),
            Command::Group => self.group(),
            Command::Ungroup => self.ungroup(),
            Command::MoveInZOrder(how) => self.move_in_z_order(how),
            Command::Align(how) => self.align(how),
            Command::Distribute(how) => self.distribute(how),
            Command::SelectAll => self.select_all(),
            Command::ZoomIn => self.zoom_in(),
            Command::ZoomOut => self.zoom_out(),
            Command::ResetZoom => self.reset_zoom(),
            Command::ZoomToFit => self.zoom_to_fit(),
            Command::FinishLine => {
                self.finish_line_being_placed();
            }
            Command::FinishLineOrEditText => {
                if !self.finish_line_being_placed() {
                    self.edit_selected_text();
                }
            }
        }
    }

    /// Selects every shown, unlocked Element of the Scene (the `select all`
    /// command).
    pub fn select_all(&mut self) {
        self.commit_text();
        let ids: Vec<String> = self
            .shown()
            .filter(|e| !e.is_locked())
            .map(|e| e.id().to_owned())
            .collect();
        if self.selection.select(ids) {
            self.overlay_revision += 1;
        }
        self.editing_group = None;
        self.sync_points();
    }
}
