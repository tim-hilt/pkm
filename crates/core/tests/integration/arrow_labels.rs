//! Labels on arrows (ADR-0006), tested at the Editor seam like those of
//! shapes (`bound_text.rs`). The only test double is a `TextMeasurer` with
//! fixed character widths (half the font size).
//!
//! Expected values are worked out by hand from Excalidraw's rules
//! (`getBoundTextMaxWidth`, `LinearElementEditor.getBoundTextElementCenter`):
//! at font size 20 a character is 10 wide and a line 25 high; the lines of an
//! arrow's label are at most 0.7 of its width wide, but no less than 11 font
//! sizes (220); the label is centred on the middle of the arrow's path.

use std::sync::Arc;

use workingtitle_core::json::Value;
use workingtitle_core::{Editor, TextAlign, TextEditor};

use crate::common;
use common::{FixedWidth, editor_with, saved_elements, tap};

fn scene(elements: &[&str]) -> Editor {
    let mut editor = editor_with(elements);
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor
}

/// An arrow at (100, 100) through `points`, `width` wide, as saved by
/// Excalidraw (`roundness` as given).
fn arrow_through(kind: &str, width: f64, points: &str, roundness: &str) -> String {
    format!(
        r##"{{"id":"a","type":"{kind}","x":100,"y":100,"width":{width},"height":0,"points":{points},
        "roundness":{roundness},"groupIds":[],"version":3,"versionNonce":7}}"##
    )
}

/// A straight arrow from (100, 100) to (400, 100); its middle is (250, 100).
fn arrow() -> String {
    arrow_through("arrow", 300.0, "[[0,0],[300,0]]", "null")
}

const ON_ARROW: (f64, f64) = (150.0, 100.0);

fn double_tap(editor: &mut Editor, at: (f64, f64)) {
    tap(editor, "finger", at, 10.0);
    tap(editor, "finger", at, 10.2);
}

fn open(editor: &Editor) -> TextEditor {
    editor.text_editor().expect("a text editor is open")
}

fn saved(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn label(editor: &Editor) -> Value {
    let labels: Vec<Value> = saved_elements(editor)
        .into_iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .collect();
    assert_eq!(labels.len(), 1, "{labels:?}");
    labels[0].clone()
}

fn number(element: &Value, key: &str) -> f64 {
    common::num(element, key)
}

/// The middle of the box of the open editor.
fn middle(shown: &TextEditor) -> (f64, f64) {
    (shown.x + shown.width / 2.0, shown.y + shown.height / 2.0)
}

// --- Opening ----------------------------------------------------------------------

#[test]
fn a_double_tap_on_an_arrow_opens_a_new_label_at_the_middle_of_its_path() {
    let mut editor = scene(&[&arrow()]);

    double_tap(&mut editor, ON_ARROW);

    let shown = open(&editor);
    assert_eq!(shown.text, "");
    assert_eq!(shown.align, TextAlign::Center);
    assert!(shown.fixed_width);
    // 0.7 * 300 = 210 is less than 11 font sizes, so 220 wide; one line high,
    // centred on (250, 100).
    assert_eq!((shown.x, shown.width), (140.0, 220.0));
    assert_eq!((shown.y, shown.height), (87.5, 25.0));
    assert_eq!(shown.angle, 0.0);
}

#[test]
fn a_long_arrow_gives_its_label_seven_tenths_of_its_width() {
    let mut editor = scene(&[&arrow_through("arrow", 600.0, "[[0,0],[600,0]]", "null")]);

    double_tap(&mut editor, ON_ARROW);

    assert_eq!(open(&editor).width, 420.0);
}

#[test]
fn enter_with_an_arrow_selected_opens_its_label() {
    let mut editor = scene(&[&arrow()]);
    tap(&mut editor, "finger", ON_ARROW, 10.0);
    assert_eq!(editor.selection(), ["a"]);

    editor.edit_selected_text();

    assert_eq!(middle(&open(&editor)), (250.0, 100.0));
}

#[test]
fn a_label_that_is_there_is_edited() {
    let mut editor = scene(&[
        r##"{"id":"a","type":"arrow","x":100,"y":100,"width":300,"height":0,"points":[[0,0],[300,0]],
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
        r##"{"id":"t","type":"text","containerId":"a","fontSize":20,"text":"from a file",
            "originalText":"from a file","x":0,"y":0,"width":110,"height":25,"lineHeight":1.25,
            "textAlign":"center","verticalAlign":"middle","version":3,"versionNonce":7}"##,
    ]);

    double_tap(&mut editor, ON_ARROW);

    let shown = open(&editor);
    assert_eq!(shown.text, "from a file");
    assert_eq!(middle(&shown), (250.0, 100.0));
}

#[test]
fn a_line_takes_no_label() {
    let mut editor = scene(&[&arrow_through("line", 300.0, "[[0,0],[300,0]]", "null")]);

    double_tap(&mut editor, ON_ARROW);

    assert!(editor.text_editor().is_none());
}

// --- Where the label sits -----------------------------------------------------------

#[test]
fn the_label_of_an_arrow_with_an_odd_number_of_points_is_on_the_middle_one() {
    let bent = arrow_through("arrow", 200.0, "[[0,0],[100,100],[200,0]]", "null");
    let mut editor = scene(&[&bent]);

    double_tap(&mut editor, (150.0, 150.0));

    // The point (100, 100) of the arrow, which is at (100, 100).
    assert_eq!(middle(&open(&editor)), (200.0, 200.0));
}

#[test]
fn the_label_of_an_arrow_with_an_even_number_of_points_is_in_the_middle_of_its_middle_segment() {
    let elbow = arrow_through(
        "arrow",
        200.0,
        "[[0,0],[100,0],[100,100],[200,100]]",
        "null",
    );
    let mut editor = scene(&[&elbow]);

    double_tap(&mut editor, (150.0, 100.0));

    // Halfway between (100, 0) and (100, 100) of the arrow.
    assert_eq!(middle(&open(&editor)), (200.0, 150.0));
}

#[test]
fn the_label_of_a_rounded_arrow_is_halfway_along_its_curve() {
    // Symmetric about x = 250, so the middle of its length is there; the
    // inner points are at y = 0 and the curve between them bulges above.
    let rounded = arrow_through(
        "arrow",
        300.0,
        "[[0,0],[100,-100],[200,-100],[300,0]]",
        r#"{"type":2}"#,
    );
    let mut editor = scene(&[&rounded]);
    tap(&mut editor, "finger", (100.0, 100.0), 10.0);
    editor.edit_selected_text();

    let (x, y) = middle(&open(&editor));
    assert!((x - 250.0).abs() < 1e-6, "{x}");
    assert!(y < 0.0 && y > -50.0, "{y}");
}

#[test]
fn a_turned_arrow_has_its_label_turned_about_its_middle_but_upright() {
    // The arrow runs (100, 100) to (400, 100), and a quarter turn about its
    // middle (250, 100) makes it run up from (250, 250) to (250, -50). The
    // label stays upright.
    let turned = arrow().replace(
        r#""roundness""#,
        r#""roughness":0,"angle":1.5707963267948966,"roundness""#,
    );
    let mut editor = scene(&[&turned]);
    tap(&mut editor, "finger", (250.0, 0.0), 10.0);
    assert_eq!(editor.selection(), ["a"]);
    editor.edit_selected_text();

    let shown = open(&editor);
    let (x, y) = middle(&shown);
    assert!(
        (x - 250.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6,
        "{x} {y}"
    );
    assert_eq!(shown.angle, 0.0);
    assert_eq!(shown.angle, 0.0);
}

// --- Typing -----------------------------------------------------------------------

#[test]
fn the_label_wraps_to_the_arrow_label_width_and_the_arrow_does_not_grow() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);

    // 22 characters make a line of 220: "aaaaaaaaaa bbbbbbbbbb" fits (21),
    // "cccc" does not.
    editor.set_editing_text("aaaaaaaaaa bbbbbbbbbb cccc");
    editor.commit_text();

    let text = label(&editor);
    assert_eq!(common::text(&text, "text"), "aaaaaaaaaa bbbbbbbbbb\ncccc");
    assert_eq!(number(&text, "width"), 210.0);
    assert_eq!(number(&text, "height"), 50.0);
    // Centred on (250, 100).
    assert_eq!(number(&text, "x"), 145.0);
    assert_eq!(number(&text, "y"), 75.0);
    assert_eq!(number(&text, "angle"), 0.0);
    let arrow = saved(&editor, "a");
    assert_eq!(
        (number(&arrow, "width"), number(&arrow, "height")),
        (300.0, 0.0)
    );
}

#[test]
fn committing_binds_the_label_to_the_arrow_and_selects_it() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("Hello");
    editor.commit_text();

    let id = common::text(&label(&editor), "id").to_owned();
    assert_eq!(common::text(&label(&editor), "containerId"), "a");
    let bound = saved(&editor, "a");
    let bound = bound
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(common::text(&bound[0], "id"), id);
    assert_eq!(editor.selection(), ["a"]);
    assert_eq!(editor.geometry().text_lines().len(), 1);
}

#[test]
fn the_label_moves_with_the_arrow() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("Hello");
    editor.commit_text();
    let before = editor.geometry().text_lines()[0].x;

    common::replay(
        &mut editor,
        "20 down 1 finger 150 100\n20.1 move 1 finger 170 140\n20.2 move 1 finger 200 160\n20.3 up 1 finger 200 160",
    );

    assert_eq!(editor.geometry().text_lines()[0].x, before + 50.0);
    assert_eq!(number(&label(&editor), "x"), 250.0 - 25.0 + 50.0);
}

#[test]
fn the_drawn_label_follows_a_changed_path() {
    // An arrow from the file whose label was saved where the arrow used to
    // be: it is drawn at the middle of the path as it is now.
    let editor = scene(&[
        r##"{"id":"a","type":"arrow","x":100,"y":100,"width":300,"height":0,"points":[[0,0],[300,0]],
            "boundElements":[{"type":"text","id":"t"}],"version":3,"versionNonce":7}"##,
        r##"{"id":"t","type":"text","containerId":"a","fontSize":20,"text":"x",
            "originalText":"x","x":0,"y":0,"width":10,"height":25,"lineHeight":1.25,
            "textAlign":"center","verticalAlign":"middle","version":3,"versionNonce":7}"##,
    ]);

    // The anchor of a centred line is the middle of the label.
    assert_eq!(editor.geometry().text_lines()[0].x, 250.0);
}

// --- Undo, cancel, empty ------------------------------------------------------------

#[test]
fn creating_an_arrow_label_is_one_undo_step() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("Hello");
    editor.set_editing_text("Hello world");
    editor.commit_text();
    assert!(editor.can_undo());

    editor.undo();

    assert_eq!(editor.geometry().text_lines().len(), 0);
    let bound = saved(&editor, "a");
    let bound = bound.get("boundElements");
    assert!(
        bound.is_none()
            || bound
                .and_then(Value::as_array)
                .is_some_and(<[Value]>::is_empty),
        "{bound:?}"
    );

    editor.redo();
    assert_eq!(common::text(&label(&editor), "text"), "Hello world");
}

#[test]
fn cancelling_a_new_arrow_label_leaves_the_arrow_as_it_was() {
    let mut editor = scene(&[&arrow()]);
    let before = saved(&editor, "a");
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("Hel");

    editor.cancel_text();

    assert_eq!(saved_elements(&editor).len(), 1);
    assert_eq!(
        saved(&editor, "a").get("boundElements"),
        before.get("boundElements")
    );
}

#[test]
fn an_empty_arrow_label_is_not_kept() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("  ");
    editor.commit_text();

    assert_eq!(saved_elements(&editor).len(), 1);
    assert!(!editor.can_undo());
}

#[test]
fn emptying_an_arrow_label_deletes_it_and_undo_brings_it_back() {
    let mut editor = scene(&[&arrow()]);
    double_tap(&mut editor, ON_ARROW);
    editor.set_editing_text("Hello");
    editor.commit_text();

    editor.edit_selected_text();
    assert_eq!(open(&editor).text, "Hello");
    editor.set_editing_text("");
    editor.commit_text();

    assert_eq!(editor.geometry().text_lines().len(), 0);
    editor.undo();
    assert_eq!(common::text(&label(&editor), "text"), "Hello");
}
