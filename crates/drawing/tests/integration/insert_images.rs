//! Pasting and dropping images (ADR-0003), tested through `Editor`: the
//! client has already decoded and downscaled the image and hands the core
//! its bytes with the natural size; the core makes an image Element, stores
//! the bytes in the Drawing's `files` as Excalidraw does and selects it, as
//! one undo step.

use pkm_base::json::{Value, parse};
use pkm_drawing::{Editor, ImageFile, MagnifyEvent, Tool};

use crate::common;
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
    editor.scroll(pkm_drawing::ScrollEvent {
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
