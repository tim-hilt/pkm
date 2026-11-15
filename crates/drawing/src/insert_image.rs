//! Pasting and dropping images, as Excalidraw's `insertImages` does
//! (`App.tsx` at the pinned commit).
//!
//! The client decodes and downscales (ADR-0003: the core never looks at the
//! pixels) and hands over the bytes with the image's natural size. Each image
//! becomes an image Element showing a file of the Drawing's `files`, at its
//! natural size unless that is taller than half of what the view shows. Several
//! images are laid out in a grid, as a whole centred on the point they are
//! put at.

use crate::draw_shape::new_element;
use crate::history::Step;
use crate::scene::Scene;
use crate::style::Style;
use pkm_base::clock::now_ms;
use pkm_base::json::Value;
use pkm_base::random::Entropy;

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

#[cfg(test)]
mod tests {
    //! Pasting and dropping images (ADR-0003), tested through `Editor`: the
    //! client has already decoded and downscaled the image and hands the core
    //! its bytes with the natural size; the core makes an image Element, stores
    //! the bytes in the Drawing's `files` as Excalidraw does and selects it, as
    //! one undo step.

    use crate::{Editor, ImageFile, MagnifyEvent, Tool};
    use pkm_base::json::{Value, parse};

    use crate::test_support as common;
    use common::saved_elements;

    fn image(file_id: &str, (width, height): (u32, u32)) -> ImageFile {
        ImageFile {
            file_id: file_id.into(),
            mime_type: "image/png".into(),
            bytes: b"foobar".to_vec(),
            width,
            height,
        }
    }

    /// An editor whose view is 800 x 600 points, at 100 % with the origin of the
    /// Scene in the top left corner.
    fn editor() -> Editor {
        let mut editor = Editor::new();
        editor.set_view_size(800.0, 600.0);
        editor
    }

    fn number(element: &Value, key: &str) -> f64 {
        element.get(key).and_then(Value::as_f64).unwrap()
    }

    /// The `files` of the saved Drawing.
    fn saved_files(editor: &Editor) -> Value {
        let saved = String::from_utf8(editor.save()).unwrap();
        parse(&saved).unwrap().get("files").cloned().unwrap()
    }

    /// `(x, y, width, height)` of an Element.
    fn frame(element: &Value) -> (f64, f64, f64, f64) {
        (
            number(element, "x"),
            number(element, "y"),
            number(element, "width"),
            number(element, "height"),
        )
    }

    // --- The Element ------------------------------------------------------------

    #[test]
    fn an_inserted_image_is_an_image_element_for_the_file_centred_in_the_view() {
        let mut editor = editor();

        editor.insert_images(vec![image("f1", (200, 100))]);

        let elements = saved_elements(&editor);
        assert_eq!(elements.len(), 1);
        let element = &elements[0];
        assert_eq!(element.get("type").and_then(Value::as_str), Some("image"));
        assert_eq!(element.get("fileId").and_then(Value::as_str), Some("f1"));
        assert_eq!(element.get("status").and_then(Value::as_str), Some("saved"));
        assert_eq!(frame(element), (300.0, 250.0, 200.0, 100.0));
        let geometry = editor.geometry();
        assert_eq!(geometry.images().len(), 1);
        assert_eq!(geometry.image_file_id(&geometry.images()[0]), "f1");
    }

    #[test]
    fn a_large_image_is_shown_at_half_the_height_of_the_view_keeping_its_ratio() {
        let mut editor = editor();

        editor.insert_images(vec![image("f1", (1000, 500))]);

        // Excalidraw: at most half the view height (and at most the view height
        // less 120 points) in Scene units.
        assert_eq!(
            frame(&saved_elements(&editor)[0]),
            (100.0, 150.0, 600.0, 300.0)
        );
    }

    #[test]
    fn the_size_limit_is_in_scene_units_so_a_zoomed_in_view_gets_a_smaller_image() {
        let mut editor = editor();
        editor.magnify(MagnifyEvent {
            x: 400.0,
            y: 300.0,
            magnification: 1.0,
        });
        let zoom = editor.viewport().zoom();
        assert!(zoom > 1.5, "the view is zoomed in");

        editor.insert_images(vec![image("f1", (1000, 1000))]);

        let (_, _, width, height) = frame(&saved_elements(&editor)[0]);
        assert!((height - 300.0 / zoom).abs() < 1e-9);
        assert_eq!(width, height);
    }

    #[test]
    fn an_image_can_be_put_at_a_point_of_the_view() {
        let mut editor = editor();

        editor.insert_images_at(vec![image("f1", (200, 100))], 100.0, 80.0);

        assert_eq!(
            frame(&saved_elements(&editor)[0]),
            (0.0, 30.0, 200.0, 100.0)
        );
    }

    #[test]
    fn the_point_is_in_the_view_so_it_follows_panning() {
        let mut editor = editor();
        editor.scroll(crate::ScrollEvent {
            x: 0.0,
            y: 0.0,
            dx: -50.0,
            dy: -20.0,
            modifiers: Default::default(),
        });
        let (origin_x, origin_y) = editor.viewport().origin();
        assert_ne!((origin_x, origin_y), (0.0, 0.0), "the view is panned");

        editor.insert_images_at(vec![image("f1", (200, 100))], 100.0, 80.0);

        let (x, y, _, _) = frame(&saved_elements(&editor)[0]);
        assert_eq!((x, y), (origin_x, origin_y + 30.0));
    }

    #[test]
    fn several_images_are_laid_out_in_a_grid_around_the_point_without_overlapping() {
        let mut editor = editor();

        editor.insert_images(vec![
            image("a", (100, 100)),
            image("b", (100, 100)),
            image("c", (100, 100)),
        ]);

        // Two columns, 50 points apart; each row and the whole grid are centred on
        // (400, 300).
        let frames: Vec<_> = saved_elements(&editor).iter().map(frame).collect();
        assert_eq!(
            frames,
            [
                (275.0, 175.0, 100.0, 100.0),
                (425.0, 175.0, 100.0, 100.0),
                (350.0, 325.0, 100.0, 100.0),
            ]
        );
    }

    // --- The file ---------------------------------------------------------------

    #[test]
    fn the_bytes_are_stored_in_the_files_map_as_a_data_url_like_excalidraw_does() {
        let mut editor = editor();

        editor.insert_images(vec![image("f1", (200, 100))]);

        let files = saved_files(&editor);
        let file = files.get("f1").expect("a file for the id");
        assert_eq!(
            file.get("mimeType").and_then(Value::as_str),
            Some("image/png")
        );
        assert_eq!(file.get("id").and_then(Value::as_str), Some("f1"));
        assert_eq!(
            file.get("dataURL").and_then(Value::as_str),
            Some("data:image/png;base64,Zm9vYmFy")
        );
        assert!(number(file, "created") > 1.0e12, "milliseconds since 1970");
        assert_eq!(editor.file_bytes("f1"), Some(b"foobar".to_vec()));
    }

    #[test]
    fn the_same_image_twice_makes_two_elements_but_stores_the_file_once() {
        let mut editor = editor();
        editor.insert_images(vec![image("f1", (200, 100))]);
        let mut again = image("f1", (200, 100));
        again.bytes = b"other".to_vec();

        editor.insert_images(vec![again]);

        assert_eq!(saved_elements(&editor).len(), 2);
        assert_eq!(editor.file_bytes("f1"), Some(b"foobar".to_vec()));
    }

    // --- Selection, Tool and undo -----------------------------------------------

    #[test]
    fn the_inserted_images_are_selected_and_the_selection_tool_is_active() {
        let mut editor = editor();
        editor.set_tool(Tool::Rectangle);

        editor.insert_images(vec![image("a", (10, 10)), image("b", (10, 10))]);

        let ids: Vec<String> = saved_elements(&editor)
            .iter()
            .map(|e| e.get("id").and_then(Value::as_str).unwrap().to_owned())
            .collect();
        assert_eq!(editor.selection(), ids);
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn inserting_is_one_undo_step_and_redo_brings_the_images_back() {
        let mut editor = editor();
        editor.insert_images(vec![image("a", (10, 10)), image("b", (10, 10))]);
        assert_eq!(editor.element_count(), 2);

        editor.undo();
        assert_eq!(editor.element_count(), 0);
        assert!(editor.selection().is_empty());
        assert!(!editor.can_undo());

        editor.redo();
        assert_eq!(editor.element_count(), 2);
        assert_eq!(editor.selection().len(), 2);
    }

    #[test]
    fn an_image_without_pixels_or_bytes_is_not_inserted() {
        let mut editor = editor();
        let mut empty = image("e", (10, 10));
        empty.bytes.clear();

        editor.insert_images(vec![image("z", (0, 10)), image("y", (10, 0)), empty]);

        assert_eq!(editor.element_count(), 0);
        assert!(!editor.can_undo());
        assert_eq!(saved_files(&editor), parse("{}").unwrap());
    }

    #[test]
    fn a_valid_image_among_invalid_ones_is_inserted_alone_at_the_centre() {
        let mut editor = editor();

        editor.insert_images(vec![image("z", (0, 10)), image("a", (200, 100))]);

        assert_eq!(
            frame(&saved_elements(&editor)[0]),
            (300.0, 250.0, 200.0, 100.0)
        );
    }

    #[test]
    fn a_saved_drawing_with_an_inserted_image_opens_with_the_image() {
        let mut editor = editor();
        editor.insert_images(vec![image("f1", (200, 100))]);

        let mut reopened = Editor::new();
        reopened.load(&editor.save()).unwrap();

        assert_eq!(reopened.file_bytes("f1"), Some(b"foobar".to_vec()));
        assert_eq!(reopened.geometry().images().len(), 1);
    }
}
