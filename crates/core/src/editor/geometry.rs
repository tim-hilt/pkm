use super::*;
use crate::geometry::Sizes;
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
/// `(generation, handle, version, background, measurer)`:
///
/// * `generation` and `handle`: handles are only stable while the same
///   Drawing is open ([`Geometry::generation`]); a load empties the cache.
/// * `version`: changes whenever the Element's drawing does. For a text that
///   is the version it is drawn with, which also changes with its container.
/// * `background`: the colour of the Canvas, which fills outline arrowheads;
///   a change empties the cache.
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
    /// Empties the cache unless it is for `generation` and `background`.
    fn begin(&mut self, generation: u64, background: &str) {
        if self.generation != generation || self.background != background {
            self.elements.clear();
            self.generation = generation;
            self.background = background.to_owned();
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
            geometry: self.geometry_of(next_generation(), included),
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
        include: impl Fn(&Element) -> bool,
    ) -> Geometry {
        let background = self.scene.view_background_color();
        let mut cache = self.geometry_cache.borrow_mut();
        cache.begin(self.generation, background);
        let mut geometry = Geometry::with_capacity(generation, cache.sizes);
        let containers = Containers::new(&self.scene);
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
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
                        drawn
                    })
                }
                ElementType::Image => cache.get(handle, element.version(), |_| {
                    let mut drawn = Geometry::new();
                    image::draw(&element, &mut drawn);
                    drawn
                }),
                _ => cache.get(handle, element.version(), |background| {
                    let mut drawn = Geometry::new();
                    shape::draw(&element, background, &mut drawn);
                    drawn
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
