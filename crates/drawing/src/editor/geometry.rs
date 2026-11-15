use super::*;
use crate::geometry::Sizes;
use std::cell::OnceCell;
use std::collections::{HashMap, hash_map};

/// What `generate` makes, kept small; counted in `generated`.
fn generated_by(
    generate: impl FnOnce(&str) -> Geometry,
    background: &str,
    generated: &mut u64,
) -> Geometry {
    *generated += 1;
    let mut geometry = generate(background);
    geometry.shrink_to_fit();
    geometry
}

/// What was generated for the Elements of the open Scene, so that
/// [`Editor::geometry`], which runs after every change to the Scene,
/// regenerates only what changed instead of every Element.
///
/// An entry is valid for the key
/// `(generation, handle, version, background, dark, measurer)`:
///
/// * `generation` and `handle`: handles are only stable while the same
///   Drawing is open ([`Geometry::generation`]); a load or a switch of dark
///   mode empties the cache.
/// * `version`: changes whenever the Element's drawing does. For a text that
///   is the version it is drawn with, which also changes with its container.
/// * `background`: the colour of the Canvas, which fills outline arrowheads;
///   a change empties the cache.
/// * `dark`: whether colours are the dark display colours; a change empties
///   the cache. Besides a switch of dark mode, that is an [`Editor::export`]
///   (always light) while dark mode is on, which costs one regeneration of
///   each.
/// * `measurer`: what texts are laid out with; setting one empties the cache.
///
/// The eraser fade is not in the key: entries are cached unfaded and the fade
/// is applied when assembling.
///
/// Entries of Elements that are no longer shown are dropped by
/// [`Editor::geometry`], so the cache never outgrows the Scene.
#[derive(Debug, Default)]
pub(super) struct GeometryCache {
    generation: u64,
    background: String,
    dark: bool,
    /// By handle: the version, what was generated and the `epoch` of the last
    /// assembly that used it.
    elements: HashMap<u32, Entry>,
    epoch: u64,
    /// What the last assembly came to.
    sizes: Sizes,
    /// How many Elements were generated, for tests.
    generated: u64,
}

#[derive(Debug)]
struct Entry {
    version: u32,
    geometry: Geometry,
    epoch: u64,
}

impl GeometryCache {
    /// Empties the cache unless it is for `generation`, `background` and
    /// `dark`.
    fn begin(&mut self, generation: u64, background: &str, dark: bool) {
        if self.generation != generation || self.background != background || self.dark != dark {
            self.elements.clear();
            self.generation = generation;
            self.background = background.to_owned();
            self.dark = dark;
        }
        self.epoch += 1;
    }

    /// What was generated for the Element with `handle` at `version`; made by
    /// `generate` (given the background) if the cache has nothing current.
    fn get(
        &mut self,
        handle: u32,
        version: u32,
        generate: impl FnOnce(&str) -> Geometry,
    ) -> &Geometry {
        let Self {
            background,
            elements,
            epoch,
            generated,
            ..
        } = self;
        let entry = match elements.entry(handle) {
            hash_map::Entry::Occupied(occupied) => {
                let entry = occupied.into_mut();
                if entry.version != version {
                    entry.geometry = generated_by(generate, background, generated);
                    entry.version = version;
                }
                entry
            }
            hash_map::Entry::Vacant(vacant) => vacant.insert(Entry {
                version,
                geometry: generated_by(generate, background, generated),
                epoch: *epoch,
            }),
        };
        entry.epoch = *epoch;
        &entry.geometry
    }

    /// Drops what the last assembly did not use.
    fn prune(&mut self) {
        let epoch = self.epoch;
        self.elements.retain(|_, entry| entry.epoch == epoch);
    }

    /// Forgets everything, for a change that the key does not cover.
    pub(super) fn clear(&mut self) {
        self.elements.clear();
    }
}

impl Editor {
    /// Shows colours as Excalidraw's dark theme does (ADR-0010) in
    /// [`Editor::geometry`], [`Editor::overlay`], [`Editor::text_editor`] and
    /// [`Editor::canvas_color`]; off by default.
    /// Clients set it from the system appearance. It changes only what is
    /// shown: the Scene, [`Editor::revision`], the Selection, the history,
    /// [`Editor::properties`] and [`Editor::export`] stay as they are, so
    /// clients redraw after changing it. The geometry is regenerated once, and
    /// has another [`Geometry::generation`].
    pub fn set_dark_mode(&mut self, on: bool) {
        if self.dark_mode != on {
            self.dark_mode = on;
            self.overlay_revision += 1;
            // What clients cached per Element has the old colours.
            self.generation = next_generation();
        }
    }

    /// Whether colours are shown dark, see [`Editor::set_dark_mode`].
    pub fn dark_mode(&self) -> bool {
        self.dark_mode
    }

    /// The colour of the Canvas as `0xRRGGBBAA`: `viewBackgroundColor`, as
    /// displayed (so transformed in dark mode). Clients clear their frame to it.
    pub fn canvas_color(&self) -> u32 {
        let background = parse_color(self.scene.view_background_color());
        if self.dark_mode {
            dark_mode(background)
        } else {
            background
        }
    }

    /// What a PNG export of `scope` shows: its geometry and the Scene
    /// rectangle to rasterise, which is the Elements' bounds plus
    /// Excalidraw's export padding. A text being edited is in it as typed
    /// so far. `None` if there is nothing to export.
    pub fn export(&self, scope: ExportScope) -> Option<Export> {
        let included = |element: &Element| match scope {
            ExportScope::Scene => true,
            ExportScope::Selection => self.selection.covers(element),
        };
        let bounds = bounds_of(self.shown().filter(|e| included(e)))?;
        Some(Export {
            geometry: self.geometry_of(next_generation(), false, included),
            x: bounds.min_x - EXPORT_PADDING,
            y: bounds.min_y - EXPORT_PADDING,
            width: bounds.max_x - bounds.min_x + 2.0 * EXPORT_PADDING,
            height: bounds.max_y - bounds.min_y + 2.0 * EXPORT_PADDING,
        })
    }

    /// The geometry of the shown Elements for which `include` holds.
    pub(super) fn geometry_of(
        &self,
        generation: u64,
        dark: bool,
        include: impl Fn(&Element) -> bool,
    ) -> Geometry {
        let background = self.scene.view_background_color();
        let mut cache = self.geometry_cache.borrow_mut();
        cache.begin(self.generation, background, dark);
        let mut geometry = Geometry::with_capacity(generation, cache.sizes);
        let containers = Containers::new(&self.scene);
        // Found the first time an arrow has a label: most Scenes have none.
        let labels = OnceCell::new();
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
        // Elements are drawn in their own colours (outline arrowheads in the
        // Canvas colour as written) and transformed as a whole, once.
        let shown = |mut drawn: Geometry| {
            if dark {
                drawn.map_colors(dark_mode);
            }
            drawn
        };
        for element in self.shown().filter(|e| include(e)) {
            let mark = geometry.mark();
            let handle = element.handle();
            let drawn = match element.kind() {
                ElementType::Text => {
                    let container = containers.of(&element);
                    let version = text::version(&element, container.as_ref());
                    cache.get(handle, version, |_| {
                        let mut drawn = Geometry::new();
                        text::draw(&element, &containers, measurer, &mut drawn);
                        shown(drawn)
                    })
                }
                ElementType::Image => cache.get(handle, element.version(), |_| {
                    let mut drawn = Geometry::new();
                    image::draw(&element, &mut drawn);
                    drawn
                }),
                ElementType::Arrow if element.has_bound_text() => {
                    let label = labels
                        .get_or_init(|| text::labels(&self.scene))
                        .get(element.id());
                    // The arrow looks different when its label does.
                    let version = label.map_or(element.version(), |label| {
                        text::version(label, Some(&element))
                    });
                    cache.get(handle, version, |background| {
                        let mut drawn = Geometry::new();
                        shape::draw(&element, background, &mut drawn);
                        if let Some(hole) = label
                            .and_then(|label| text::arrow_label_hole(label, &element, measurer))
                        {
                            drawn.cut(hole, version);
                        }
                        shown(drawn)
                    })
                }
                _ => cache.get(handle, element.version(), |background| {
                    let mut drawn = Geometry::new();
                    shape::draw(&element, background, &mut drawn);
                    shown(drawn)
                }),
            };
            geometry.extend(drawn);
            if self
                .erasing
                .as_ref()
                .is_some_and(|erasing| erasing.is_pending(element.id()))
            {
                geometry.fade(mark, READY_TO_ERASE_OPACITY);
            }
        }
        cache.sizes = geometry.sizes();
        geometry
    }

    /// Drops cached Elements of Elements that [`Editor::geometry`] just left out.
    pub(super) fn prune_geometry_cache(&self) {
        self.geometry_cache.borrow_mut().prune();
    }

    /// How many Elements were generated (not served from the cache) so far.
    /// For tests: unchanged Elements must not be generated again.
    #[doc(hidden)]
    pub fn elements_generated(&self) -> u64 {
        self.geometry_cache.borrow().generated
    }
}

/// The smallest rectangle containing `elements` as they are shown (see
/// [`Frame::shown`]), or `None` if there are none.
pub(super) fn bounds_of<'a>(elements: impl Iterator<Item = Element<'a>>) -> Option<Bounds> {
    elements
        .map(|element| Frame::shown(&element))
        .reduce(Frame::union)
        .map(|frame| Bounds {
            min_x: frame.min.0,
            min_y: frame.min.1,
            max_x: frame.max.0,
            max_y: frame.max.1,
        })
}

#[cfg(test)]
mod tests {
    //! `Editor::geometry` regenerates only Elements whose drawing may have
    //! changed (issue 56). Tested at that seam: after every kind of change the
    //! output equals that of a fresh `Editor` loaded with the same Scene, and
    //! unchanged Elements are not generated again.

    use crate::{Editor, Geometry, Tool, ZOrder};

    use crate::test_support::{editor_with, replay, tap};

    fn rect(id: &str, x: f64) -> String {
        format!(
            r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"index":null,
        "seed":11,"version":3,"versionNonce":7}}"##
        )
    }

    fn arrow(id: &str) -> String {
        format!(
            r##"{{"id":"{id}","type":"arrow","x":0,"y":100,"width":80,"height":0,
        "points":[[0,0],[80,0]],"startArrowhead":"circle_outline","endArrowhead":"triangle_outline",
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,"index":null,
        "seed":5,"version":2,"versionNonce":3}}"##
        )
    }

    fn three() -> Editor {
        editor_with(&[&rect("a", 0.0), &rect("b", 100.0), &arrow("c")])
    }

    fn drawing(editor: &Editor, background: &str) -> Vec<u8> {
        let saved = String::from_utf8(editor.save()).unwrap();
        // The Scene's own background is what a fresh editor must share.
        saved
            .replacen("\"#ffffff\"", &format!("\"{background}\""), 1)
            .into_bytes()
    }

    fn same(a: &Geometry, b: &Geometry) -> bool {
        a.parts() == b.parts()
            && a.verbs() == b.verbs()
            && a.coords() == b.coords()
            && a.text_lines() == b.text_lines()
            && a.text() == b.text()
            && a.images() == b.images()
            && a.file_ids() == b.file_ids()
    }

    /// Asserts that `editor` draws what a fresh editor with its Scene draws.
    fn assert_matches_fresh(editor: &Editor) {
        let mut fresh = Editor::new();
        fresh.load(&editor.save()).unwrap();
        assert!(same(&editor.geometry(), &fresh.geometry()));
    }

    #[test]
    fn repeating_geometry_generates_nothing() {
        let editor = three();
        editor.geometry();
        let generated = editor.elements_generated();
        assert_eq!(generated, 3);

        editor.geometry();

        assert_eq!(editor.elements_generated(), generated);
    }

    #[test]
    fn moving_one_element_regenerates_only_it() {
        let mut editor = three();
        editor.geometry();
        let before = editor.elements_generated();

        tap(&mut editor, "mouse", (120.0, 10.0), 0.0);
        replay(
            &mut editor,
            "1.0 down 1 mouse 120 10\n1.1 move 1 mouse 150 40\n1.2 move 1 mouse 160 50\n1.3 up 1 mouse 160 50",
        );
        editor.geometry();

        // One regeneration per distinct version the Element went through.
        assert!(editor.elements_generated() - before <= 3);
        assert_matches_fresh(&editor);
    }

    #[test]
    fn edits_deletes_and_undo_match_a_fresh_editor() {
        let mut editor = three();
        editor.geometry();

        editor.set_tool(Tool::Rectangle);
        replay(
            &mut editor,
            "1.0 down 1 mouse 300 300\n1.1 move 1 mouse 340 330\n1.2 up 1 mouse 340 330",
        );
        assert_matches_fresh(&editor);

        editor.delete_selection();
        assert_matches_fresh(&editor);
        editor.undo();
        assert_matches_fresh(&editor);
        editor.redo();
        assert_matches_fresh(&editor);
        editor.undo();
        editor.undo();
        assert_matches_fresh(&editor);

        editor.select_all();
        editor.move_in_z_order(ZOrder::ToBack);
        assert_matches_fresh(&editor);
    }

    #[test]
    fn deleted_elements_leave_the_cache_and_come_back() {
        let mut editor = three();
        editor.geometry();
        tap(&mut editor, "mouse", (120.0, 10.0), 0.0);
        editor.delete_selection();
        editor.geometry();
        let generated = editor.elements_generated();

        editor.undo();
        let geometry = editor.geometry();

        assert_eq!(geometry.parts().iter().map(|p| p.element).max(), Some(2));
        assert!(editor.elements_generated() > generated);
        assert_matches_fresh(&editor);
    }

    #[test]
    fn the_eraser_preview_fades_without_regenerating() {
        let mut editor = three();
        editor.geometry();
        let generated = editor.elements_generated();
        editor.set_tool(Tool::Eraser);

        replay(
            &mut editor,
            "0.0 down 1 mouse 20 -10\n0.1 move 1 mouse 20 30",
        );
        let preview = editor.geometry();
        assert!(preview.parts().iter().any(|p| p.opacity < 1.0));
        assert_eq!(editor.elements_generated(), generated);

        replay(&mut editor, "0.2 up 1 mouse 20 30");
        assert_matches_fresh(&editor);
        // Cancelling brings the Elements back unfaded.
    }

    #[test]
    fn loading_another_background_regenerates_what_it_changes() {
        let mut editor = three();
        editor.geometry();
        let bytes = drawing(&editor, "#000000");
        editor.load(&bytes).unwrap();

        let mut fresh = Editor::new();
        fresh.load(&bytes).unwrap();
        assert!(same(&editor.geometry(), &fresh.geometry()));
    }

    #[test]
    fn texts_and_images_are_cached_and_match_a_fresh_editor() {
        let text = r##"{"id":"t","type":"text","x":10,"y":200,"width":60,"height":50,"text":"two\nlines",
        "originalText":"two\nlines","fontSize":20,"fontFamily":5,"lineHeight":1.25,
        "textAlign":"left","verticalAlign":"top","strokeColor":"#1e1e1e","version":4,"versionNonce":1}"##;
        let image = r##"{"id":"i","type":"image","x":200,"y":200,"width":50,"height":40,"fileId":"f1",
        "status":"saved","scale":[1,1],"version":2,"versionNonce":1}"##;
        let editor = editor_with(&[&rect("a", 0.0), text, image, &rect("b", 100.0)]);
        editor.geometry();
        let generated = editor.elements_generated();
        assert_eq!(generated, 4);

        assert!(same(&editor.geometry(), &editor.geometry()));
        assert_eq!(editor.elements_generated(), generated);
        assert_matches_fresh(&editor);
        assert_eq!(editor.geometry().text_lines().len(), 2);
        assert_eq!(editor.geometry().images().len(), 1);
    }
}
