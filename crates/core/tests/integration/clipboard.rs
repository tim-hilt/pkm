//! Copy and paste with excalidraw.com (ADR-0003), tested through `Editor`:
//! copying the Selection gives Excalidraw's clipboard JSON
//! (`excalidraw/clipboard`) as text, and pasting such text, from this app or
//! from excalidraw.com, inserts its Elements as new ones around a point.
//!
//! `fixtures/excalidraw-clipboard.json` is what excalidraw.com wrote on
//! copying a labelled rectangle (in a group with its label), an ellipse and
//! an arrow bound to both, with `serializeAsClipboardJSON`.

use workingtitle_core::json::{Value, parse};
use workingtitle_core::{Editor, ImageFile};

use crate::common;
use common::{editor_with, saved_elements};

const FROM_EXCALIDRAW: &str = include_str!("../fixtures/excalidraw-clipboard.json");

const RECTANGLE: &str = r##"{"id":"rect","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "backgroundColor":"#ffc9c9","groupIds":[],"boundElements":[{"id":"arrow","type":"arrow"}],"seed":5}"##;
const ELLIPSE: &str = r##"{"id":"ell","type":"ellipse","x":300,"y":100,"width":100,"height":50,
    "groupIds":[],"boundElements":[{"id":"arrow","type":"arrow"}],"seed":6}"##;
const ARROW: &str = r##"{"id":"arrow","type":"arrow","x":200,"y":125,"width":100,"height":0,
    "points":[[0,0],[100,0]],"groupIds":[],
    "startBinding":{"elementId":"rect","focus":0,"gap":1},
    "endBinding":{"elementId":"ell","focus":0,"gap":1}}"##;

/// An editor whose view is 800 x 600 points, at 100 % with the origin of the
/// Scene in the top left corner.
fn blank() -> Editor {
    let mut editor = Editor::new();
    editor.set_view_size(800.0, 600.0);
    editor
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

fn text<'a>(element: &'a Value, key: &str) -> Option<&'a str> {
    element.get(key).and_then(Value::as_str)
}

fn id(element: &Value) -> &str {
    text(element, "id").unwrap()
}

fn ids(editor: &Editor) -> Vec<String> {
    saved_elements(editor)
        .iter()
        .map(|e| id(e).to_owned())
        .collect()
}

fn by_type<'a>(elements: &'a [Value], kind: &str) -> &'a Value {
    elements
        .iter()
        .find(|e| text(e, "type") == Some(kind))
        .unwrap()
}

/// The three Elements of a rectangle bound by an arrow to an ellipse, selected.
fn drawn() -> Editor {
    let mut editor = editor_with(&[RECTANGLE, ELLIPSE, ARROW]);
    editor.set_view_size(800.0, 600.0);
    editor.select_all();
    editor
}

fn clipboard(text: &str) -> Value {
    parse(text).unwrap()
}

// --- Copy ---------------------------------------------------------------------

#[test]
fn copying_nothing_gives_nothing() {
    let editor = editor_with(&[RECTANGLE]);

    assert_eq!(editor.copy_selection(), None);
}

#[test]
fn copy_gives_excalidraw_clipboard_json_of_the_selected_elements() {
    let editor = drawn();

    let copied = clipboard(&editor.copy_selection().unwrap());

    assert_eq!(text(&copied, "type"), Some("excalidraw/clipboard"));
    let elements = copied.get("elements").and_then(Value::as_array).unwrap();
    let copied_ids: Vec<&str> = elements.iter().map(id).collect();
    assert_eq!(copied_ids, ["rect", "ell", "arrow"]);
    assert_eq!(number(&elements[0], "x"), 100.0);
    assert_eq!(text(&elements[0], "backgroundColor"), Some("#ffc9c9"));
    // Excalidraw always has `files`, even if empty.
    assert_eq!(
        copied
            .get("files")
            .and_then(Value::as_object)
            .map(|f| f.len()),
        Some(0)
    );
}

#[test]
fn copy_leaves_out_what_is_not_selected_and_what_is_deleted() {
    // Only the ellipse is selected, by tapping it.
    let mut editor = editor_with(&[
        RECTANGLE,
        r#"{"id":"gone","type":"ellipse","x":0,"y":0,"width":10,"height":10,"isDeleted":true}"#,
        ELLIPSE,
    ]);
    common::tap(&mut editor, "mouse", (350.0, 100.0), 0.0);
    assert_eq!(editor.selection(), ["ell"]);

    let copied = clipboard(&editor.copy_selection().unwrap());

    let elements = copied.get("elements").and_then(Value::as_array).unwrap();
    assert_eq!(elements.iter().map(id).collect::<Vec<_>>(), ["ell"]);
}

#[test]
fn copy_takes_the_files_of_copied_images_along() {
    let mut editor = blank();
    editor.insert_images(vec![
        ImageFile {
            file_id: "f1".into(),
            mime_type: "image/png".into(),
            bytes: b"foobar".to_vec(),
            width: 10,
            height: 10,
        },
        ImageFile {
            file_id: "f2".into(),
            mime_type: "image/png".into(),
            bytes: b"other".to_vec(),
            width: 10,
            height: 10,
        },
    ]);
    // Only the first image is selected.
    let first = saved_elements(&editor)[0].clone();
    common::tap(
        &mut editor,
        "mouse",
        (number(&first, "x") + 1.0, number(&first, "y") + 1.0),
        0.0,
    );
    assert_eq!(editor.selection().len(), 1);

    let copied = clipboard(&editor.copy_selection().unwrap());

    let files = copied.get("files").and_then(Value::as_object).unwrap();
    assert_eq!(files.len(), 1);
    let file = files.get("f1").unwrap();
    assert_eq!(text(file, "mimeType"), Some("image/png"));
    assert_eq!(
        text(file, "dataURL"),
        Some("data:image/png;base64,Zm9vYmFy")
    );
}

#[test]
fn copy_does_not_change_the_scene_or_the_selection() {
    let editor = drawn();
    let before = editor.save();

    editor.copy_selection();

    assert_eq!(editor.save(), before);
    assert_eq!(editor.selection().len(), 3);
}

// --- Paste ---------------------------------------------------------------------

#[test]
fn pasting_what_was_copied_adds_new_elements_centred_in_the_view() {
    let copied = drawn().copy_selection().unwrap();
    let mut editor = blank();

    assert!(editor.paste(&copied));

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 3);
    for element in &elements {
        assert!(!["rect", "ell", "arrow"].contains(&id(element)));
    }
    // The three span 100..400 x 100..150: their middle goes to the middle of
    // the view, (400, 300).
    let rectangle = by_type(&elements, "rectangle");
    assert_eq!(
        (number(rectangle, "x"), number(rectangle, "y")),
        (250.0, 275.0)
    );
    let ellipse = by_type(&elements, "ellipse");
    assert_eq!((number(ellipse, "x"), number(ellipse, "y")), (450.0, 275.0));
    assert_eq!(text(rectangle, "backgroundColor"), Some("#ffc9c9"));
}

#[test]
fn paste_at_centres_the_elements_on_a_point_of_the_view() {
    let copied = drawn().copy_selection().unwrap();
    let mut editor = blank();
    editor.set_viewport((1000.0, 0.0), 2.0);

    assert!(editor.paste_at(&copied, 100.0, 50.0));

    // View point (100, 50) is Scene point (1050, 25).
    let elements = saved_elements(&editor);
    let rectangle = by_type(&elements, "rectangle");
    assert_eq!(
        (number(rectangle, "x"), number(rectangle, "y")),
        (900.0, 0.0)
    );
}

#[test]
fn pasted_elements_become_the_selection_without_their_labels() {
    let mut editor = blank();

    editor.paste(FROM_EXCALIDRAW);

    let elements = saved_elements(&editor);
    let mut selected: Vec<&str> = editor.selection().iter().map(String::as_str).collect();
    let mut expected: Vec<&str> = elements
        .iter()
        .filter(|e| text(e, "type") != Some("text"))
        .map(id)
        .collect();
    selected.sort_unstable();
    expected.sort_unstable();
    assert_eq!(selected, expected);
    assert_eq!(selected.len(), 3);
}

#[test]
fn pasting_from_excalidraw_com_gives_new_ids_and_keeps_bindings_among_the_elements() {
    let mut editor = blank();

    assert!(editor.paste(FROM_EXCALIDRAW));

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 4);
    let originals = clipboard(FROM_EXCALIDRAW);
    for original in originals.get("elements").and_then(Value::as_array).unwrap() {
        assert!(elements.iter().all(|e| id(e) != id(original)));
    }
    let rectangle = by_type(&elements, "rectangle");
    let ellipse = by_type(&elements, "ellipse");
    let arrow = by_type(&elements, "arrow");
    let label = by_type(&elements, "text");
    assert_eq!(text(label, "containerId"), Some(id(rectangle)));
    let bound: Vec<&str> = rectangle
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(id)
        .collect();
    assert_eq!(bound, [id(label), id(arrow)]);
    let start = arrow.get("startBinding").unwrap();
    let end = arrow.get("endBinding").unwrap();
    assert_eq!(text(start, "elementId"), Some(id(rectangle)));
    assert_eq!(text(end, "elementId"), Some(id(ellipse)));
    // Binding members Excalidraw has and the core does not know stay.
    assert_eq!(text(start, "mode"), Some("orbit"));
    assert!(start.get("fixedPoint").is_some());
}

#[test]
fn pasted_groups_are_new_groups_that_hold_the_same_elements() {
    let mut editor = blank();

    editor.paste(FROM_EXCALIDRAW);

    let elements = saved_elements(&editor);
    let group_of = |e: &Value| -> Vec<String> {
        e.get("groupIds")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|g| g.as_str().unwrap().to_owned())
            .collect()
    };
    let rectangle = group_of(by_type(&elements, "rectangle"));
    let label = group_of(by_type(&elements, "text"));
    assert_eq!(rectangle.len(), 1);
    assert_eq!(rectangle, label);
    let original = clipboard(FROM_EXCALIDRAW);
    let original_group = original
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find_map(|e| {
            e.get("groupIds")
                .and_then(Value::as_array)?
                .first()
                .cloned()
        })
        .unwrap();
    assert_ne!(Some(rectangle[0].as_str()), original_group.as_str());
    assert!(group_of(by_type(&elements, "ellipse")).is_empty());
}

#[test]
fn pasted_elements_are_drawn_differently_from_their_originals() {
    let copied = drawn().copy_selection().unwrap();
    let mut editor = blank();

    editor.paste(&copied);

    let rectangle = &saved_elements(&editor)[0];
    assert_ne!(number(rectangle, "seed"), 5.0);
}

#[test]
fn pasting_twice_makes_two_independent_sets() {
    let mut editor = blank();

    editor.paste(FROM_EXCALIDRAW);
    editor.paste(FROM_EXCALIDRAW);

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 8);
    let mut all = ids(&editor);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 8);
    let (first, second) = (&elements[..4], &elements[4..]);
    let label_of = |set: &[Value]| text(by_type(set, "text"), "containerId").map(str::to_owned);
    assert_eq!(
        label_of(first).as_deref(),
        Some(id(by_type(first, "rectangle")))
    );
    assert_eq!(
        label_of(second).as_deref(),
        Some(id(by_type(second, "rectangle")))
    );
    assert_ne!(label_of(first), label_of(second));
}

#[test]
fn pasted_elements_go_on_top_in_their_order() {
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_view_size(800.0, 600.0);

    editor.paste(FROM_EXCALIDRAW);

    let elements = saved_elements(&editor);
    let types: Vec<&str> = elements.iter().map(|e| text(e, "type").unwrap()).collect();
    assert_eq!(
        types,
        ["rectangle", "rectangle", "text", "ellipse", "arrow"]
    );
}

#[test]
fn a_reference_to_an_element_that_was_not_copied_is_dropped() {
    // Only the arrow is copied, not what it is bound to.
    let mut source = editor_with(&[RECTANGLE, ELLIPSE, ARROW]);
    common::tap(&mut source, "mouse", (250.0, 125.0), 0.0);
    assert_eq!(source.selection(), ["arrow"]);
    let copied = source.copy_selection().unwrap();
    let mut editor = blank();

    editor.paste(&copied);

    let arrow = &saved_elements(&editor)[0];
    assert_eq!(arrow.get("startBinding"), Some(&Value::Null));
    assert_eq!(arrow.get("endBinding"), Some(&Value::Null));
}

#[test]
fn pasting_images_stores_their_files_in_the_drawing() {
    let image = r#"{"id":"img","type":"image","x":0,"y":0,"width":40,"height":20,
        "fileId":"f1","status":"saved","scale":[1,1]}"#;
    let payload = format!(
        r#"{{"type":"excalidraw/clipboard","elements":[{image}],"files":{{
            "f1":{{"mimeType":"image/png","id":"f1","dataURL":"data:image/png;base64,Zm9vYmFy","created":1,"lastRetrieved":1}},
            "unused":{{"mimeType":"image/png","id":"unused","dataURL":"data:image/png;base64,YQ==","created":1}}}}}}"#
    );
    let mut editor = blank();

    assert!(editor.paste(&payload));

    let element = &saved_elements(&editor)[0];
    assert_eq!(text(element, "fileId"), Some("f1"));
    assert_eq!(editor.file_bytes("f1"), Some(b"foobar".to_vec()));
    assert_eq!(editor.file_bytes("unused"), None);
    let geometry = editor.geometry();
    assert_eq!(geometry.images().len(), 1);
}

#[test]
fn text_that_is_not_excalidraw_clipboard_json_is_not_pasted() {
    let mut editor = editor_with(&[RECTANGLE]);
    let before = editor.save();

    for not_clipboard in [
        "hello",
        "",
        "{\"type\":\"excalidraw/clipboard\"}",
        "{\"type\":\"other\",\"elements\":[]}",
        "[1,2,3]",
        "{\"type\":\"excalidraw/clipboard\",\"elements\":[]}",
        // Element skeletons of Excalidraw's API, which the core does not
        // turn into Elements (`convertToExcalidrawElements`).
        r#"{"type":"excalidraw-api/clipboard","elements":[
            {"id":"s","type":"rectangle","x":0,"y":0,"label":{"text":"lost"}}]}"#,
    ] {
        assert!(!editor.paste(not_clipboard), "{not_clipboard}");
    }

    assert_eq!(editor.save(), before);
    assert!(!editor.can_undo());
}

#[test]
fn a_whole_excalidraw_file_can_be_pasted_as_well() {
    let file = r#"{"type":"excalidraw","version":2,"elements":[
        {"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":100}],"appState":{}}"#;
    let mut editor = blank();

    assert!(editor.paste(file));

    assert_eq!(saved_elements(&editor).len(), 1);
}

#[test]
fn deleted_elements_of_the_payload_are_left_out() {
    let payload = r#"{"type":"excalidraw/clipboard","elements":[
        {"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":100},
        {"id":"b","type":"rectangle","x":500,"y":0,"width":100,"height":100,"isDeleted":true}]}"#;
    let mut editor = blank();

    editor.paste(payload);

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    // Centred on its own, not with the deleted one.
    assert_eq!(number(&elements[0], "x"), 350.0);
}

#[test]
fn elements_of_types_the_core_does_not_know_are_kept() {
    let payload = r#"{"type":"excalidraw/clipboard","elements":[
        {"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":100},
        {"id":"e","type":"embeddable","x":0,"y":0,"width":100,"height":100,"link":"https://example.com"}]}"#;
    let mut editor = blank();

    editor.paste(payload);

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 2);
    let embed = by_type(&elements, "embeddable");
    assert_ne!(id(embed), "e");
    assert_eq!(text(embed, "link"), Some("https://example.com"));
    // Only what the core knows can be selected.
    assert_eq!(editor.selection().len(), 1);
}

#[test]
fn missing_fields_of_pasted_elements_get_excalidraws_defaults() {
    let payload = r#"{"type":"excalidraw/clipboard","elements":[
        {"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":100}]}"#;
    let mut editor = blank();

    editor.paste(payload);

    let rectangle = &saved_elements(&editor)[0];
    assert_eq!(text(rectangle, "strokeColor"), Some("#1e1e1e"));
    assert_eq!(number(rectangle, "opacity"), 100.0);
    assert!(rectangle.get("groupIds").is_some());
}

#[test]
fn an_element_that_is_in_a_frame_that_was_not_copied_is_pasted_without_one() {
    let payload = r#"{"type":"excalidraw/clipboard","elements":[
        {"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":100,"frameId":"frame"}]}"#;
    let mut editor = blank();

    editor.paste(payload);

    assert_eq!(
        saved_elements(&editor)[0].get("frameId"),
        Some(&Value::Null)
    );
}

#[test]
fn pasting_goes_back_to_the_selection_tool() {
    let mut editor = blank();
    editor.set_tool(workingtitle_core::Tool::Rectangle);

    editor.paste(FROM_EXCALIDRAW);

    assert_eq!(editor.tool(), workingtitle_core::Tool::Selection);
}

// --- Undo ----------------------------------------------------------------------

#[test]
fn a_paste_is_one_undo_step() {
    let mut editor = editor_with(&[RECTANGLE]);
    editor.set_view_size(800.0, 600.0);
    editor.select_all();

    editor.paste(FROM_EXCALIDRAW);
    assert_eq!(editor.selection().len(), 3);
    editor.undo();

    let remaining: Vec<Value> = saved_elements(&editor)
        .into_iter()
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .collect();
    assert_eq!(remaining.len(), 1);
    assert_eq!(id(&remaining[0]), "rect");
    assert_eq!(editor.selection(), ["rect"]);
    assert!(!editor.can_undo());

    editor.redo();
    assert_eq!(editor.selection().len(), 3);
}

// --- Cut -----------------------------------------------------------------------

#[test]
fn cut_gives_the_clipboard_json_and_deletes_the_selection() {
    let mut editor = drawn();
    let expected = editor.copy_selection();

    let cut = editor.cut_selection();

    assert_eq!(cut, expected);
    assert!(editor.selection().is_empty());
    assert!(
        saved_elements(&editor)
            .iter()
            .all(|e| e.get("isDeleted").and_then(Value::as_bool) == Some(true))
    );
}

#[test]
fn cut_with_nothing_selected_does_nothing() {
    let mut editor = editor_with(&[RECTANGLE]);

    assert_eq!(editor.cut_selection(), None);

    assert!(!editor.can_undo());
}

#[test]
fn cut_is_undoable_and_can_be_pasted_back() {
    let mut editor = drawn();
    let cut = editor.cut_selection().unwrap();

    editor.undo();
    assert_eq!(editor.selection().len(), 3);
    editor.redo();
    assert!(editor.paste(&cut));

    let shown: Vec<Value> = saved_elements(&editor)
        .into_iter()
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .collect();
    assert_eq!(shown.len(), 3);
    // Pasted where the view's middle is.
    assert_eq!(number(by_type(&shown, "rectangle"), "x"), 250.0);
}
