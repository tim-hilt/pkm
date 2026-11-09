//! What Excalidraw draws for an image Element (`renderElement.ts` at the
//! pinned commit), as geometry: which file, where, cropped how.
//!
//! The core only describes the image. Decoding the file is the client's job
//! (ADR-0003, ADR-0008), and so is the placeholder it shows while it has no
//! pixels for the file.

use crate::geometry::{Geometry, Image};
use crate::scene::Element;
use crate::shape::corner_radius;

/// Adds the image `element` to `geometry`.
pub(crate) fn draw(element: &Element, geometry: &mut Geometry) {
    let (scale_x, scale_y) = element.image_scale();
    let (u0, v0, u1, v1) = element.image_crop().unwrap_or((0.0, 0.0, 1.0, 1.0));
    let (width, height) = (element.width(), element.height());
    let corner_radius = element
        .roundness()
        .map_or(0.0, |roundness| corner_radius(width.min(height), roundness));
    // Positions in the record are filled in by the geometry.
    geometry.image(
        element.file_id().unwrap_or_default(),
        Image {
            element: element.handle(),
            version: element.version(),
            part_index: 0,
            file_id_start: 0,
            file_id_len: 0,
            status: element.image_status() as u32,
            opacity: element.opacity() as f32,
            x: element.x() as f32,
            y: element.y() as f32,
            width: width as f32,
            height: height as f32,
            angle: element.angle() as f32,
            scale_x: scale_x as f32,
            scale_y: scale_y as f32,
            u0: u0 as f32,
            v0: v0 as f32,
            u1: u1 as f32,
            v1: v1 as f32,
            corner_radius: corner_radius as f32,
        },
    );
}
