use super::*;

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
        let mut geometry = Geometry::with_generation(generation);
        let background = self.scene.view_background_color();
        let containers = Containers::new(&self.scene);
        let measurer = self.measurer.as_ref().map(|m| &*m.0);
        for element in self.shown().filter(|e| include(e)) {
            let mark = geometry.mark();
            if element.kind() == ElementType::Text {
                text::draw(&element, &containers, measurer, &mut geometry);
            } else if element.kind() == ElementType::Image {
                image::draw(&element, &mut geometry);
            } else {
                shape::draw(&element, background, &mut geometry);
            }
            if self
                .erasing
                .as_ref()
                .is_some_and(|erasing| erasing.is_pending(element.id()))
            {
                geometry.fade(mark, READY_TO_ERASE_OPACITY);
            }
        }
        geometry
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
