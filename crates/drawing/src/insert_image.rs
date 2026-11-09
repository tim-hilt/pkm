//! Pasting and dropping images, as Excalidraw's `insertImages` does
//! (`App.tsx` at the pinned commit).
//!
//! The client decodes and downscales (ADR-0003: the core never looks at the
//! pixels) and hands over the bytes with the image's natural size. Each image
//! becomes an image Element showing a file of the Drawing's `files`, at its
//! natural size unless that is taller than half of what the view shows. Several
//! images are laid out in a grid, as a whole centred on the point they are
//! put at.

use crate::clock::now_ms;
use crate::draw_shape::new_element;
use crate::history::Step;
use crate::json::Value;
use crate::random::Entropy;
use crate::scene::Scene;
use crate::style::Style;

/// Excalidraw's padding between images of a grid, in view points.
const GRID_PADDING: f64 = 50.0;

/// An image as a client hands it over: the bytes of the file as they are to
/// be stored, and the size of the picture they show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    /// What the file is called in the Drawing's `files`: the same for the same
    /// content, so that an image pasted twice is stored once. Excalidraw uses
    /// the SHA-1 of the file as the client got it, before downscaling.
    pub file_id: String,
    /// The type of the bytes, e.g. `image/png`.
    pub mime_type: String,
    pub bytes: Vec<u8>,
    /// Size of the picture in pixels.
    pub width: u32,
    pub height: u32,
}

impl ImageFile {
    /// Whether there is anything to show.
    fn is_usable(&self) -> bool {
        self.width > 0 && self.height > 0 && !self.bytes.is_empty() && !self.file_id.is_empty()
    }
}

/// What the Scene is looked at through, which sizes and places the images.
pub(crate) struct Placement {
    /// Height of the view in points, zero if it is not known.
    pub view_height: f64,
    pub zoom: f64,
    /// The Scene point the images are centred on.
    pub centre: (f64, f64),
}

/// Adds the usable images of `images` to the Scene as part of `step`. Returns
/// the ids of the new Elements, in the order of the images.
pub(crate) fn insert(
    scene: &mut Scene,
    entropy: &mut Entropy,
    step: &mut Step,
    style: &Style,
    placement: &Placement,
    images: &[ImageFile],
) -> Vec<String> {
    let usable: Vec<&ImageFile> = images.iter().filter(|image| image.is_usable()).collect();
    let sizes: Vec<(f64, f64)> = usable
        .iter()
        .map(|image| shown_size(image, placement))
        .collect();
    let origins = grid(&sizes, placement.centre, GRID_PADDING / placement.zoom);
    let now = now_ms();

    let mut ids = Vec::new();
    for ((image, (width, height)), origin) in usable.iter().zip(sizes).zip(origins) {
        scene.add_file(&image.file_id, &image.mime_type, &image.bytes, now);
        let mut element = new_element("image", None, entropy, style, origin);
        let text = |s: &str| Value::String(s.to_owned());
        for (key, value) in [
            ("width", Value::Number(width)),
            ("height", Value::Number(height)),
            // Images keep their colours; Excalidraw may one day let SVGs take
            // the stroke colour.
            ("strokeColor", text("transparent")),
            ("status", text("saved")),
            ("fileId", text(&image.file_id)),
            (
                "scale",
                Value::Array(vec![Value::Number(1.0), Value::Number(1.0)]),
            ),
            ("crop", Value::Null),
        ] {
            element.insert(key.into(), value);
        }
        let id = element
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        step.create(&id);
        scene.push(element);
        ids.push(id);
    }
    ids
}

/// How big the image is shown (`getImageNaturalDimensions`): its natural
/// height, but at most the view's height less 120 points (160 at least) and
/// half the view's height in Scene units; the width keeps the ratio. Without
/// a view size there is no limit.
fn shown_size(image: &ImageFile, placement: &Placement) -> (f64, f64) {
    let (width, height) = (f64::from(image.width), f64::from(image.height));
    if placement.view_height <= 0.0 {
        return (width, height);
    }
    let tallest = (placement.view_height - 120.0)
        .max(160.0)
        .min((placement.view_height * 0.5).floor() / placement.zoom);
    let shown_height = height.min(tallest);
    (shown_height * (width / height), shown_height)
}

/// The top left corners of rectangles of `sizes` laid out in rows of
/// `ceil(sqrt(n))`, `padding` apart, each row centred horizontally and the
/// whole grid vertically on `centre` (`positionElementsOnGrid`).
fn grid(sizes: &[(f64, f64)], centre: (f64, f64), padding: f64) -> Vec<(f64, f64)> {
    let columns = (sizes.len() as f64).sqrt().ceil().max(1.0) as usize;
    let rows: Vec<&[(f64, f64)]> = sizes.chunks(columns).collect();
    let row_width = |row: &[(f64, f64)]| {
        row.iter().map(|(width, _)| width).sum::<f64>() + padding * (row.len() as f64 - 1.0)
    };
    let row_height = |row: &[(f64, f64)]| row.iter().map(|&(_, height)| height).fold(0.0, f64::max);
    let total_height =
        rows.iter().map(|row| row_height(row)).sum::<f64>() + padding * (rows.len() as f64 - 1.0);

    let mut origins = Vec::with_capacity(sizes.len());
    let mut y = centre.1 - total_height / 2.0;
    for row in rows {
        let mut x = centre.0 - row_width(row) / 2.0;
        for &(width, _) in row {
            origins.push((x, y));
            x += width + padding;
        }
        y += row_height(row) + padding;
    }
    origins
}
