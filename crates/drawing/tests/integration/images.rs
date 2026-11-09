//! Image Elements (ADR-0003, ADR-0008), tested through `Editor`: the core
//! describes an image by file id, placement, crop and scale, hands out the
//! bytes of the file untouched, and never decodes them; clients do that.

use crate::common;

use common::editor_with_element;
use pkm_base::json::parse;
use pkm_drawing::{Editor, Geometry, Image, ImageStatus, PartKind};

const PHOTO: &str = r#""id":"i","type":"image","x":100,"y":50,"width":200,"height":100,
    "fileId":"f1","status":"saved","scale":[1,1],"crop":null,"opacity":50"#;

fn only_image(geometry: &Geometry) -> Image {
    assert_eq!(geometry.images().len(), 1);
    geometry.images()[0]
}

// --- What is drawn ----------------------------------------------------------

#[test]
fn an_image_is_described_by_file_and_placement_not_by_paths() {
    let geometry = editor_with_element(PHOTO).geometry();

    assert!(geometry.parts().is_empty(), "no box around an image");
    let image = only_image(&geometry);
    assert_eq!(geometry.image_file_id(&image), "f1");
    assert_eq!(image.status, ImageStatus::Saved as u32);
    assert_eq!((image.x, image.y), (100.0, 50.0));
    assert_eq!((image.width, image.height), (200.0, 100.0));
    assert_eq!(image.angle, 0.0);
    assert_eq!(image.opacity, 0.5);
}

#[test]
fn an_uncropped_image_shows_the_whole_file() {
    let image = only_image(&editor_with_element(PHOTO).geometry());

    assert_eq!(
        (image.u0, image.v0, image.u1, image.v1),
        (0.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn a_crop_selects_a_part_of_the_file_in_fractions_of_its_size() {
    // Crop rectangles are in pixels of the natural image, 400 x 200 here.
    let geometry = editor_with_element(
        r#""id":"i","type":"image","x":0,"y":0,"width":100,"height":50,"fileId":"f1",
        "crop":{"x":100,"y":50,"width":200,"height":100,"naturalWidth":400,"naturalHeight":200}"#,
    )
    .geometry();

    let image = only_image(&geometry);
    assert_eq!(
        (image.u0, image.v0, image.u1, image.v1),
        (0.25, 0.25, 0.75, 0.75)
    );
}

#[test]
fn a_crop_of_a_zero_sized_file_is_ignored_rather_than_dividing_by_zero() {
    let image = only_image(
        &editor_with_element(
            r#""id":"i","type":"image","width":10,"height":10,"fileId":"f1",
            "crop":{"x":0,"y":0,"width":5,"height":5,"naturalWidth":0,"naturalHeight":0}"#,
        )
        .geometry(),
    );

    assert_eq!(
        (image.u0, image.v0, image.u1, image.v1),
        (0.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn flipped_images_carry_their_scale_and_rotation() {
    let image = only_image(
        &editor_with_element(
            r#""id":"i","type":"image","width":10,"height":10,"fileId":"f1",
            "scale":[-1,1],"angle":0.5"#,
        )
        .geometry(),
    );

    assert_eq!((image.scale_x, image.scale_y), (-1.0, 1.0));
    assert_eq!(image.angle, 0.5);
}

#[test]
fn rounded_images_say_how_much_to_round_their_corners() {
    let rounded = |roundness: &str| {
        only_image(
            &editor_with_element(&format!(
                r#""id":"i","type":"image","width":200,"height":100,"fileId":"f1",
                "roundness":{roundness}"#
            ))
            .geometry(),
        )
        .corner_radius
    };

    assert_eq!(rounded(r#"{"type":2}"#), 25.0);
    assert_eq!(rounded(r#"{"type":3}"#), 25.0);
    assert_eq!(rounded("null"), 0.0);
    assert_eq!(
        only_image(&editor_with_element(PHOTO).geometry()).corner_radius,
        0.0
    );
}

#[test]
fn an_image_without_a_file_id_or_a_status_is_still_described() {
    let geometry =
        editor_with_element(r#""id":"i","type":"image","width":10,"height":10,"fileId":null"#)
            .geometry();

    let image = only_image(&geometry);
    assert_eq!(geometry.image_file_id(&image), "");
    assert_eq!(image.status, ImageStatus::Pending as u32);
}

#[test]
fn the_status_of_a_failed_image_is_passed_on() {
    let image = only_image(
        &editor_with_element(
            r#""id":"i","type":"image","width":10,"height":10,"fileId":"f1","status":"error""#,
        )
        .geometry(),
    );

    assert_eq!(image.status, ImageStatus::Error as u32);
}

#[test]
fn deleted_images_are_not_drawn() {
    let geometry = editor_with_element(&format!(r#"{PHOTO},"isDeleted":true"#)).geometry();

    assert!(geometry.images().is_empty());
}

#[test]
fn an_image_changes_version_with_its_element() {
    let version = |v: u32| {
        only_image(&editor_with_element(&format!(r#"{PHOTO},"version":{v}"#)).geometry()).version
    };

    assert_eq!(version(3), 3);
    assert_ne!(version(3), version(4));
}

#[test]
fn images_are_drawn_in_z_order_with_the_paths_around_them() {
    let mut editor = Editor::new();
    editor
        .load(
            br#"{"type":"excalidraw","version":2,"elements":[
            {"id":"a","type":"rectangle","width":10,"height":10},
            {"id":"i","type":"image","width":10,"height":10,"fileId":"f1"},
            {"id":"b","type":"rectangle","x":50,"width":10,"height":10}]}"#,
        )
        .unwrap();
    let geometry = editor.geometry();

    let image = only_image(&geometry);
    let before = geometry.parts()[..image.part_index as usize].iter();
    let after = geometry.parts()[image.part_index as usize..].iter();
    assert!(before.len() > 0 && after.len() > 0);
    assert!(
        before
            .chain(after)
            .all(|p| p.kind == PartKind::Stroke as u32)
    );
    assert!(
        geometry.parts()[..image.part_index as usize]
            .iter()
            .all(|p| p.element == 0)
    );
    assert!(
        geometry.parts()[image.part_index as usize..]
            .iter()
            .all(|p| p.element == 2)
    );
    assert_eq!(image.element, 1);
}

// --- Files ------------------------------------------------------------------

const DRAWING: &str = r#"{
  "type": "excalidraw",
  "version": 2,
  "elements": [
    {"id":"i","type":"image","width":10,"height":10,"fileId":"f1"}
  ],
  "appState": {},
  "files": {
    "f1": {
      "mimeType": "image/png",
      "id": "f1",
      "dataURL": "data:image/png;base64,iVBORw0KGgo=",
      "created": 1700000000000,
      "lastRetrieved": 1700000000001
    },
    "f2": {"mimeType":"image/jpeg","id":"f2","dataURL":"data:image/jpeg;base64,/9j/4A==","created":1}
  }
}"#;

fn loaded(drawing: &str) -> Editor {
    let mut editor = Editor::new();
    editor.load(drawing.as_bytes()).unwrap();
    editor
}

#[test]
fn the_bytes_of_a_file_are_handed_out_as_stored_without_looking_at_them() {
    let editor = loaded(DRAWING);

    assert_eq!(
        editor.file_bytes("f1"),
        Some(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
    );
    assert_eq!(editor.file_bytes("f2"), Some(vec![0xff, 0xd8, 0xff, 0xe0]));
}

#[test]
fn bytes_that_are_no_image_at_all_come_out_unchanged() {
    // The core never decodes images: whatever the data URL holds is the answer.
    let editor = loaded(
        r#"{"type":"excalidraw","elements":[],
        "files":{"x":{"dataURL":"data:image/png;base64,bm90IGFuIGltYWdl"}}}"#,
    );

    assert_eq!(editor.file_bytes("x"), Some(b"not an image".to_vec()));
}

#[test]
fn a_file_that_is_missing_or_unreadable_has_no_bytes() {
    let editor = loaded(
        r#"{"type":"excalidraw","elements":[],"files":{
        "nodata":{"id":"nodata"},
        "number":{"dataURL":5},
        "plain":{"dataURL":"data:text/plain,hello"},
        "broken":{"dataURL":"data:image/png;base64,@@@@"}}}"#,
    );

    for id in ["absent", "nodata", "number", "plain", "broken"] {
        assert_eq!(editor.file_bytes(id), None, "{id}");
    }
    assert_eq!(Editor::new().file_bytes("f1"), None);
}

#[test]
fn files_are_saved_back_exactly_as_loaded() {
    let saved = String::from_utf8(loaded(DRAWING).save()).unwrap();

    let files = |text: &str| parse(text).unwrap().get("files").cloned().unwrap();
    assert_eq!(files(&saved), files(DRAWING));
}

#[test]
fn files_of_an_unreferenced_image_are_kept_too() {
    let editor = loaded(DRAWING);
    // f2 is used by no Element; it still survives open-and-save.
    assert!(
        String::from_utf8(editor.save())
            .unwrap()
            .contains("/9j/4A==")
    );
}
