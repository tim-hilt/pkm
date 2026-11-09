//! Text inside shapes (ADR-0006), tested at the Editor seam: taps and
//! commands in, the editor the client shows over the Canvas, the saved
//! Drawing and the geometry out. The only test double is a `TextMeasurer`
//! with fixed character widths (half the font size).
//!
//! Expected values are worked out by hand from Excalidraw's rules for bound
//! text (`textElement.ts`): at font size 20 a character is 10 wide and a line
//! 25 high; the text keeps 5 units of padding inside its container.

use std::sync::Arc;

use excalidraw_native_core::json::Value;
use excalidraw_native_core::{Editor, FontSize, Property, TextAlign, TextEditor, Tool};

use crate::common;
use common::{FixedWidth, editor_with, saved_elements, tap};

/// An editor whose Scene is the given Elements, with a measurer.
fn scene(elements: &[&str]) -> Editor {
    let mut editor = editor_with(elements);
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor
}

/// A shape of `kind` at (100, 100), 200 x 100, with its middle at (200, 150).
fn shape(kind: &str) -> String {
    narrow(kind, 200.0)
}

/// A shape of `kind` at (100, 100), `width` x 100.
fn narrow(kind: &str, width: f64) -> String {
    format!(
        r##"{{"id":"s","type":"{kind}","x":100,"y":100,"width":{width},"height":100,
        "backgroundColor":"transparent","roundness":null,"groupIds":["g"],"version":3,"versionNonce":7}}"##
    )
}

fn scene_of(kind: &str) -> Editor {
    scene(&[&shape(kind)])
}

const MIDDLE: (f64, f64) = (200.0, 150.0);

fn double_tap(editor: &mut Editor, at: (f64, f64)) {
    double_tap_at(editor, at, 10.0);
}

fn double_tap_at(editor: &mut Editor, at: (f64, f64), time: f64) {
    tap(editor, "finger", at, time);
    tap(editor, "finger", at, time + 0.2);
}

fn open(editor: &Editor) -> TextEditor {
    editor.text_editor().expect("a text editor is open")
}

/// The Element `id` as saved.
fn saved(editor: &Editor, id: &str) -> Value {
    saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no element {id}"))
}

/// The one text of the Scene that is not deleted.
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
    element
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("no number {key}"))
}

fn string<'a>(element: &'a Value, key: &str) -> &'a str {
    element
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no string {key}"))
}

// --- Creating --------------------------------------------------------------------

#[test]
fn a_double_tap_on_a_shape_opens_the_editor_in_it() {
    for kind in ["rectangle", "diamond", "ellipse"] {
        let mut editor = scene_of(kind);

        double_tap(&mut editor, MIDDLE);

        let shown = open(&editor);
        assert_eq!(shown.text, "", "{kind}");
        assert_eq!(shown.align, TextAlign::Center, "{kind}");
        assert!(shown.fixed_width, "{kind}");
    }
}

#[test]
fn the_editor_of_a_label_is_the_width_the_text_wraps_to_centred_in_the_shape() {
    // The width of a rectangle less 5 on both sides; the first line is
    // centred vertically in the same way.
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    let shown = open(&editor);
    assert_eq!((shown.x, shown.width), (105.0, 190.0));
    assert_eq!((shown.y, shown.height), (137.5, 25.0));

    // The largest rectangle inside an ellipse is 2 / sqrt(2) = 1.414 times
    // the radius: round(200 / 2 * 1.4142) = 141, less padding 131.
    let mut editor = scene_of("ellipse");
    double_tap(&mut editor, MIDDLE);
    let shown = open(&editor);
    assert_eq!(shown.width, 131.0);
    // It starts 5 in from the left of that rectangle, which is
    // 100 * (1 - 1 / sqrt(2)) = 29.29 in from the left of the ellipse.
    assert!((shown.x - 134.2893).abs() < 1e-3, "{}", shown.x);

    // And inside a diamond half the width: 100 less padding.
    let mut editor = scene_of("diamond");
    double_tap(&mut editor, MIDDLE);
    let shown = open(&editor);
    assert_eq!(shown.width, 90.0);
    assert_eq!(shown.x, 155.0);
}

#[test]
fn committing_binds_a_label_to_the_shape() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    editor.commit_text();

    assert!(editor.text_editor().is_none());
    let label = label(&editor);
    assert_eq!(string(&label, "containerId"), "s");
    assert_eq!(string(&label, "text"), "Hello");
    assert_eq!(string(&label, "originalText"), "Hello");
    assert_eq!(string(&label, "textAlign"), "center");
    assert_eq!(string(&label, "verticalAlign"), "middle");
    assert_eq!(number(&label, "fontSize"), 20.0);
    // 50 x 25 in the middle of the shape.
    assert_eq!(
        (number(&label, "width"), number(&label, "height")),
        (50.0, 25.0)
    );
    assert_eq!((number(&label, "x"), number(&label, "y")), (175.0, 137.5));
    // The label is in the group of the shape.
    let groups = label.get("groupIds").and_then(Value::as_array).unwrap();
    assert_eq!(groups, [Value::String("g".into())]);

    // The shape lists it.
    let bound = saved(&editor, "s");
    let bound = bound
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(bound.len(), 1);
    assert_eq!(string(&bound[0], "type"), "text");
    assert_eq!(string(&bound[0], "id"), string(&label, "id"));
}

#[test]
fn a_drawing_saved_while_a_new_label_is_edited_has_it_bound_both_ways() {
    // As Excalidraw lists it when the editing begins: autosave may write the
    // Drawing at any time, and leaving the app may end with that save.
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hel");

    let id = string(&label(&editor), "id").to_owned();
    let shape = saved(&editor, "s");
    let bound = shape
        .get("boundElements")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(bound.len(), 1);
    assert_eq!(string(&bound[0], "id"), id);

    // Cancelling, or leaving it empty, takes it off again.
    editor.cancel_text();
    let shape = saved(&editor, "s");
    let before = saved(&scene_of("rectangle"), "s");
    assert_eq!(shape.get("boundElements"), before.get("boundElements"));
}

#[test]
fn the_label_is_right_above_its_shape_and_the_shape_is_selected() {
    let mut editor = scene(&[
        &shape("rectangle"),
        r##"{"id":"o","type":"rectangle","x":500,"y":500,"width":10,"height":10}"##,
    ]);
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    editor.commit_text();

    let ids: Vec<String> = saved_elements(&editor)
        .iter()
        .map(|e| string(e, "id").to_owned())
        .collect();
    assert_eq!(ids[0], "s");
    assert_eq!(ids[2], "o");
    assert_eq!(editor.selection(), ["s"]);
}

#[test]
fn the_label_is_drawn_and_not_while_it_is_edited() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    assert_eq!(editor.geometry().text_lines().len(), 0);

    editor.commit_text();
    assert_eq!(editor.geometry().text_lines().len(), 1);
}

#[test]
fn creating_a_label_is_one_undo_step() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    editor.set_editing_text("Hello world");
    editor.commit_text();
    assert!(editor.can_undo());

    editor.undo();

    assert!(!editor.can_undo());
    assert_eq!(editor.geometry().text_lines().len(), 0);
    let shape = saved(&editor, "s");
    let bound = shape.get("boundElements");
    assert!(
        bound.is_none()
            || bound
                .and_then(Value::as_array)
                .is_some_and(|b| b.is_empty()),
        "{bound:?}"
    );

    editor.redo();
    assert_eq!(string(&label(&editor), "text"), "Hello world");
    assert_eq!(editor.geometry().text_lines().len(), 1);
}

#[test]
fn an_empty_label_is_not_kept() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("  ");
    editor.commit_text();

    assert_eq!(saved_elements(&editor).len(), 1);
    assert!(!editor.can_undo());
    assert_eq!(editor.selection(), ["s"]);
}

#[test]
fn cancelling_keeps_nothing_of_the_label() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    editor.cancel_text();

    assert_eq!(saved_elements(&editor).len(), 1);
    assert!(!editor.can_undo());
}

#[test]
fn a_tap_elsewhere_commits_the_label() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");

    tap(&mut editor, "finger", (600.0, 600.0), 20.0);

    assert!(editor.text_editor().is_none());
    assert_eq!(string(&label(&editor), "text"), "Hello");
}

#[test]
fn a_double_tap_on_the_empty_inside_of_a_hollow_shape_edits_too() {
    // The stroke is far from the tap: the inside of a transparent shape
    // counts as the shape for text.
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, (150.0, 130.0));
    assert!(editor.text_editor().is_some());
}

#[test]
fn a_double_tap_beside_the_shape_makes_a_free_text_not_a_label() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, (600.0, 600.0));
    editor.set_editing_text("Hi");
    editor.commit_text();
    assert_eq!(label(&editor).get("containerId"), Some(&Value::Null));
}

#[test]
fn the_tool_stays_the_selection_tool() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    assert_eq!(editor.tool(), Tool::Selection);
}

// --- Wrapping and alignment -------------------------------------------------------

/// The lines of the one label as saved.
fn lines(editor: &Editor) -> Vec<String> {
    string(&label(editor), "text")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// An editor with `typed` typed into a label of a shape of `kind`, `width`
/// wide.
fn labelled(kind: &str, width: f64, typed: &str) -> Editor {
    let mut editor = scene(&[&narrow(kind, width)]);
    double_tap(&mut editor, (100.0 + width / 2.0, 150.0));
    editor.set_editing_text(typed);
    editor.commit_text();
    editor
}

#[test]
fn a_label_wraps_to_the_width_of_the_shape_less_padding() {
    // 100 wide, 90 inside: nine characters of 10.
    let editor = labelled("rectangle", 100.0, "hello world foo");
    assert_eq!(lines(&editor), ["hello", "world foo"]);
    let label = label(&editor);
    assert_eq!(
        (number(&label, "width"), number(&label, "height")),
        (90.0, 50.0)
    );
    // Its original text is what was typed.
    assert_eq!(string(&label, "originalText"), "hello world foo");
}

#[test]
fn a_label_in_an_ellipse_or_diamond_wraps_to_the_rectangle_inside_it() {
    // 131 inside the ellipse of width 200: thirteen characters.
    let editor = labelled("ellipse", 200.0, "hello wonder ful");
    assert_eq!(lines(&editor), ["hello wonder", "ful"]);
    // 90 inside the diamond: nine.
    let editor = labelled("diamond", 200.0, "hello wonder ful");
    assert_eq!(lines(&editor), ["hello", "wonder", "ful"]);
    // And 190 in the rectangle: all of it.
    let editor = labelled("rectangle", 200.0, "hello wonder ful");
    assert_eq!(lines(&editor), ["hello wonder ful"]);
}

#[test]
fn a_label_is_centred_in_the_shape_as_it_grows() {
    let editor = labelled("rectangle", 100.0, "hello world foo");
    let label = label(&editor);
    // 90 x 50 in the 90 x 90 inside of the shape.
    assert_eq!((number(&label, "x"), number(&label, "y")), (105.0, 125.0));
}

#[test]
fn the_editor_follows_the_label_as_it_is_typed() {
    let mut editor = scene(&[&narrow("rectangle", 100.0)]);
    double_tap(&mut editor, (150.0, 150.0));
    editor.set_editing_text("hello world foo");

    let shown = open(&editor);
    assert_eq!(shown.text, "hello world foo");
    // The text view is as wide as the lines may be and as high as they are.
    assert_eq!((shown.x, shown.width), (105.0, 90.0));
    assert_eq!((shown.y, shown.height), (125.0, 50.0));
}

#[test]
fn the_editor_of_a_label_follows_the_viewport() {
    let mut editor = scene_of("rectangle");
    editor.set_view_size(800.0, 600.0);
    editor.zoom_in();
    let zoom = editor.viewport().zoom();
    let (ox, oy) = editor.viewport().origin();
    double_tap(
        &mut editor,
        ((MIDDLE.0 - ox) * zoom, (MIDDLE.1 - oy) * zoom),
    );
    let shown = open(&editor);
    assert!((shown.width - 190.0 * zoom).abs() < 1e-9);
    assert!((shown.font_size - 20.0 * zoom).abs() < 1e-9);
}

#[test]
fn a_left_aligned_label_starts_at_the_left_of_the_shape() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");
    editor.set_property(Property::TextAlign(TextAlign::Left));

    // The editor is the full width, the text is aligned in it.
    let shown = open(&editor);
    assert_eq!(shown.align, TextAlign::Left);
    assert_eq!((shown.x, shown.width), (105.0, 190.0));
    editor.commit_text();
    assert_eq!(number(&label(&editor), "x"), 105.0);
}

#[test]
fn a_label_can_be_aligned_and_resized_while_it_is_edited() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello");

    editor.set_property(Property::TextAlign(TextAlign::Right));
    editor.set_property(Property::FontSize(FontSize::Large));
    editor.commit_text();

    let label = label(&editor);
    assert_eq!(string(&label, "textAlign"), "right");
    assert_eq!(number(&label, "fontSize"), 28.0);
    // Five characters of 14, right at the inside of the shape.
    assert_eq!(
        (number(&label, "width"), number(&label, "height")),
        (70.0, 35.0)
    );
    assert_eq!((number(&label, "x"), number(&label, "y")), (225.0, 132.5));
}

// --- The shape grows with the text ---------------------------------------------------

fn height_of(editor: &Editor, id: &str) -> f64 {
    number(&saved(editor, id), "height")
}

#[test]
fn the_shape_grows_to_hold_more_lines_than_fit() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);

    // Three lines are 75 high, more than the 90 the shape has inside; five
    // are 125.
    editor.set_editing_text("a\nb\nc");
    assert_eq!(height_of(&editor, "s"), 100.0);
    editor.set_editing_text("a\nb\nc\nd\ne");
    // 125 + 2 * 5 padding.
    assert_eq!(height_of(&editor, "s"), 135.0);
    assert_eq!(number(&saved(&editor, "s"), "y"), 100.0);
    // And the text stays in the middle of it.
    assert_eq!(open(&editor).y, 100.0 + 5.0 + (125.0 - 125.0) / 2.0);
}

#[test]
fn the_shape_shrinks_again_as_text_is_taken_away_but_not_below_where_it_began() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");
    assert_eq!(height_of(&editor, "s"), 135.0);

    editor.set_editing_text("a\nb\nc\nd");
    assert_eq!(height_of(&editor, "s"), 110.0);
    editor.set_editing_text("a");
    assert_eq!(height_of(&editor, "s"), 100.0);
}

#[test]
fn an_ellipse_and_a_diamond_grow_by_the_rectangle_that_fits_in_them() {
    // Ellipse: round((125 + 10) / sqrt(2) * 2) = 191.
    let mut editor = scene_of("ellipse");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");
    assert_eq!(height_of(&editor, "s"), 191.0);

    // Diamond: twice the 125 + 10.
    let mut editor = scene_of("diamond");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");
    assert_eq!(height_of(&editor, "s"), 270.0);
}

#[test]
fn growing_and_labelling_are_one_undo_step_and_cancelling_gives_the_height_back() {
    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");
    editor.cancel_text();
    assert_eq!(height_of(&editor, "s"), 100.0);
    assert_eq!(saved_elements(&editor).len(), 1);

    let mut editor = scene_of("rectangle");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");
    editor.commit_text();
    assert_eq!(height_of(&editor, "s"), 135.0);
    editor.undo();
    assert_eq!(height_of(&editor, "s"), 100.0);
    editor.redo();
    assert_eq!(height_of(&editor, "s"), 135.0);
}

#[test]
fn arrows_bound_to_the_shape_follow_it_as_it_grows() {
    let mut editor = scene(&[
        &shape("rectangle").replace(
            r#""version":3"#,
            r#""boundElements":[{"type":"arrow","id":"a"}],"version":3"#,
        ),
        r##"{"id":"a","type":"arrow","x":200,"y":300,"width":0,"height":94,
            "points":[[0,0],[0,-94]],"startBinding":null,
            "endBinding":{"elementId":"s","focus":0,"gap":5,"fixedPoint":[0.5,1],"mode":"orbit"},
            "version":3,"versionNonce":7}"##,
    ]);
    let before = saved(&editor, "a");
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("a\nb\nc\nd\ne");

    // The bottom of the shape moved 35 down, and its arrow end with it.
    let after = saved(&editor, "a");
    assert_ne!(before, after);
    let end = |arrow: &Value| {
        let points = common::saved_points(arrow);
        let last = points[points.len() - 1];
        (number(arrow, "x") + last.0, number(arrow, "y") + last.1)
    };
    assert!(
        (end(&after).1 - end(&before).1 - 35.0).abs() < 1e-6,
        "{before:?} {after:?}"
    );

    editor.commit_text();
    editor.undo();
    assert_eq!(saved(&editor, "a").get("points"), before.get("points"));
}

// --- Styling a selected shape with a label ----------------------------------------------
//
// As on excalidraw.com (`changeProperty(..., includeBoundText)`), the
// properties panel reaches the label of a selected shape: its colour,
// opacity, font size and alignment.

#[test]
fn stroke_colour_and_opacity_of_a_selected_shape_reach_its_label() {
    let mut editor = labelled("rectangle", 200.0, "Hello");
    assert_eq!(editor.selection(), ["s"]);

    editor.set_property(Property::StrokeColor("#e03131".into()));
    editor.set_property(Property::Opacity(40.0));

    let label = label(&editor);
    assert_eq!(string(&label, "strokeColor"), "#e03131");
    assert_eq!(number(&label, "opacity"), 40.0);
    assert_eq!(string(&saved(&editor, "s"), "strokeColor"), "#e03131");

    editor.undo();
    assert_eq!(number(&self::label(&editor), "opacity"), 100.0);
}

#[test]
fn the_panel_offers_the_text_options_of_the_label_of_a_selected_shape() {
    let editor = labelled("rectangle", 200.0, "Hello");
    let shown = editor.properties();
    assert!(shown.sections.text);
    assert_eq!(shown.font_size, Some(FontSize::Medium));
    assert_eq!(shown.text_align, Some(TextAlign::Center));
}

#[test]
fn a_larger_font_grows_the_selected_shape_to_hold_its_label_in_one_undo_step() {
    let mut editor = labelled("rectangle", 200.0, "a\nb\nc");
    assert_eq!(height_of(&editor, "s"), 100.0);

    editor.set_property(Property::FontSize(FontSize::ExtraLarge));

    // Three lines of 36 * 1.25, and 5 of padding above and below.
    let label = label(&editor);
    assert_eq!(number(&label, "fontSize"), 36.0);
    assert_eq!(number(&label, "height"), 135.0);
    assert_eq!(height_of(&editor, "s"), 145.0);
    assert_eq!(number(&label, "y"), 105.0);

    editor.undo();
    assert_eq!(height_of(&editor, "s"), 100.0);
    assert_eq!(number(&self::label(&editor), "fontSize"), 20.0);
}

#[test]
fn aligning_the_label_of_a_selected_shape_moves_it_within_the_shape() {
    let mut editor = labelled("rectangle", 200.0, "Hello");
    editor.set_property(Property::TextAlign(TextAlign::Left));
    let label = label(&editor);
    assert_eq!(string(&label, "textAlign"), "left");
    assert_eq!(number(&label, "x"), 105.0);
}

// --- Editing a label that is there ------------------------------------------------------

/// A rectangle at (100, 100), 200 x 100, with the label `hi`, as a file from
/// excalidraw.com has it.
const FILE: [&str; 2] = [
    r##"{"id":"s","type":"rectangle","x":100,"y":100,"width":200,"height":100,
        "angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
        "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],
        "frameId":null,"index":"a0","roundness":{"type":3},"seed":1,"version":4,
        "versionNonce":2,"isDeleted":false,"boundElements":[{"type":"text","id":"t"}],
        "updated":1,"link":null,"locked":false}"##,
    r##"{"id":"t","type":"text","x":175,"y":137.5,"width":50,"height":25,"angle":0,
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
        "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],
        "frameId":null,"index":"a1","roundness":null,"seed":3,"version":2,"versionNonce":4,
        "isDeleted":false,"boundElements":null,"updated":1,"link":null,"locked":false,
        "text":"Hello","fontSize":20,"fontFamily":5,"textAlign":"center",
        "verticalAlign":"middle","containerId":"s","originalText":"Hello","autoResize":true,
        "lineHeight":1.25}"##,
];

#[test]
fn a_double_tap_on_a_shape_with_a_label_edits_the_label() {
    let mut editor = scene(&FILE);

    double_tap(&mut editor, MIDDLE);

    assert_eq!(open(&editor).text, "Hello");
    // Not a second label.
    editor.commit_text();
    assert_eq!(saved_elements(&editor).len(), 2);
}

#[test]
fn a_label_that_is_not_changed_leaves_the_file_as_it_was() {
    let mut editor = scene(&FILE);
    let before = editor.save();

    double_tap(&mut editor, MIDDLE);
    editor.commit_text();

    assert_eq!(editor.save(), before);
    assert!(!editor.can_undo());
}

#[test]
fn a_label_from_a_file_is_edited_and_saved_as_excalidraw_saves_it() {
    let mut editor = scene(&FILE);
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hello world, how are you doing");
    editor.commit_text();

    let label = saved(&editor, "t");
    // 190 inside: nineteen characters, and "Hello world, how are" has twenty.
    assert_eq!(string(&label, "text"), "Hello world, how\nare you doing");
    assert_eq!(
        string(&label, "originalText"),
        "Hello world, how are you doing"
    );
    assert_eq!(string(&label, "containerId"), "s");
    assert_eq!(number(&label, "seed"), 3.0);
    // The shape still lists it once.
    let shape = saved(&editor, "s");
    assert_eq!(
        shape
            .get("boundElements")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        1
    );
    // Reloading gives the same lines.
    let mut again = scene(&[]);
    again.load(&editor.save()).unwrap();
    assert_eq!(again.save(), editor.save());
}

#[test]
fn emptying_a_label_deletes_it_and_the_shape_stops_listing_it() {
    let mut editor = scene(&FILE);
    double_tap(&mut editor, MIDDLE);
    editor.set_editing_text("");
    editor.commit_text();

    assert_eq!(
        saved(&editor, "t").get("isDeleted"),
        Some(&Value::Bool(true))
    );
    let shape = saved(&editor, "s");
    assert_eq!(
        shape
            .get("boundElements")
            .and_then(Value::as_array)
            .map(<[Value]>::len),
        Some(0)
    );
    assert_eq!(editor.geometry().text_lines().len(), 0);

    editor.undo();
    assert_eq!(string(&label(&editor), "text"), "Hello");
    assert_eq!(
        saved(&editor, "s")
            .get("boundElements")
            .and_then(Value::as_array)
            .map(<[Value]>::len),
        Some(1)
    );
}

// --- Enter ------------------------------------------------------------------------------

#[test]
fn enter_on_a_selected_shape_edits_its_label() {
    let mut editor = scene_of("ellipse");
    tap(&mut editor, "mouse", (100.0, 150.0), 0.0);
    assert_eq!(editor.selection(), ["s"]);

    editor.edit_selected_text();

    assert_eq!(open(&editor).text, "");
    editor.set_editing_text("Hi");
    editor.commit_text();
    assert_eq!(string(&label(&editor), "containerId"), "s");
    assert_eq!(editor.selection(), ["s"]);
}

#[test]
fn enter_on_a_selected_shape_with_a_label_edits_that_label() {
    let mut editor = scene(&FILE);
    tap(&mut editor, "mouse", (101.0, 150.0), 0.0);
    assert_eq!(editor.selection(), ["s"]);

    editor.edit_selected_text();

    assert_eq!(open(&editor).text, "Hello");
}

#[test]
fn enter_on_a_selected_text_edits_it() {
    let mut editor = scene(&[
        r##"{"id":"t","type":"text","x":100,"y":50,"width":50,"height":25,"fontSize":20,
        "fontFamily":5,"lineHeight":1.25,"textAlign":"left","verticalAlign":"top",
        "text":"Hello","originalText":"Hello","containerId":null,"autoResize":true}"##,
    ]);
    tap(&mut editor, "mouse", (120.0, 60.0), 0.0);

    editor.edit_selected_text();

    assert_eq!(open(&editor).text, "Hello");
}

#[test]
fn enter_does_nothing_without_exactly_one_shape_or_text_selected() {
    let mut editor = scene(&[
        &shape("rectangle"),
        r##"{"id":"o","type":"rectangle","x":400,"y":100,"width":50,"height":50,
        "backgroundColor":"#ffc9c9","fillStyle":"solid"}"##,
        r##"{"id":"l","type":"line","x":500,"y":100,"width":50,"height":50,"points":[[0,0],[50,50]]}"##,
    ]);
    editor.edit_selected_text();
    assert!(editor.text_editor().is_none());

    editor.select_all();
    editor.edit_selected_text();
    assert!(editor.text_editor().is_none());

    tap(&mut editor, "mouse", (500.0, 100.0), 5.0);
    editor.edit_selected_text();
    assert!(editor.text_editor().is_none());
}

// --- Touch --------------------------------------------------------------------------------

#[test]
fn a_double_tap_goes_into_a_selected_group_before_it_edits_a_label() {
    let mut editor = scene(&[
        &shape("rectangle"),
        r##"{"id":"o","type":"rectangle","x":400,"y":100,"width":50,"height":50,
        "backgroundColor":"#ffc9c9","fillStyle":"solid","groupIds":["g"]}"##,
    ]);
    let edge = (150.0, 100.0);
    tap(&mut editor, "finger", edge, 0.0);
    assert_eq!(editor.selection().len(), 2);

    double_tap_at(&mut editor, edge, 5.0);
    assert!(editor.text_editor().is_none());
    assert_eq!(editor.selection(), ["s"]);

    // A double-tap on the Element that is alone now edits it.
    double_tap_at(&mut editor, edge, 10.0);
    assert!(editor.text_editor().is_some());
}

#[test]
fn a_single_tap_on_a_shape_only_selects_it() {
    let mut editor = scene_of("rectangle");
    tap(&mut editor, "finger", MIDDLE, 0.0);
    assert!(editor.text_editor().is_none());
}

#[test]
fn without_a_text_measurer_no_label_is_edited() {
    let mut editor = editor_with(&[&shape("rectangle")]);
    double_tap(&mut editor, MIDDLE);
    assert!(editor.text_editor().is_none());
}

// --- Resizing the shape ----------------------------------------------------------------------

#[test]
fn resizing_the_shape_wraps_the_label_to_it_again() {
    let mut editor = labelled("rectangle", 200.0, "hello world foo bar");
    assert_eq!(lines(&editor), ["hello world foo bar"]);
    tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
    assert_eq!(editor.selection(), ["s"]);

    // The south east handle is 4 outside the corner, and keeps that distance
    // to it: the shape is 150 wide, 140 inside, fourteen characters.
    common::replay(
        &mut editor,
        "30 down 1 mouse 304 204
         30.1 move 1 mouse 280 204
         30.2 move 1 mouse 254 204
         30.3 up 1 mouse 254 204",
    );

    assert_eq!(number(&saved(&editor, "s"), "width"), 150.0);
    assert_eq!(lines(&editor), ["hello world", "foo bar"]);
    let label = label(&editor);
    // Two lines of the longest, 11 characters, in the middle of the 100 high
    // shape and of its 140 inside.
    assert_eq!(
        (number(&label, "width"), number(&label, "height")),
        (110.0, 50.0)
    );
    assert_eq!((number(&label, "x"), number(&label, "y")), (120.0, 125.0));
    // What was typed is kept for wrapping it again.
    assert_eq!(string(&label, "originalText"), "hello world foo bar");

    // And wider again, it is one line.
    common::replay(
        &mut editor,
        "40 down 1 mouse 254 204
         40.1 move 1 mouse 280 204
         40.2 move 1 mouse 304 204
         40.3 up 1 mouse 304 204",
    );
    assert_eq!(lines(&editor), ["hello world foo bar"]);
}

#[test]
fn a_shape_made_too_narrow_for_its_label_grows_down_to_hold_the_lines() {
    let mut editor = labelled("rectangle", 200.0, "aaaa bbbb cccc dddd");
    tap(&mut editor, "mouse", (150.0, 100.0), 20.0);

    // 60 wide, 50 inside: a word a line, four lines of 25 are 100 high, more
    // than the 90 inside the shape (`handleBindTextResize`). The top stays.
    common::replay(
        &mut editor,
        "30 down 1 mouse 304 204
         30.1 move 1 mouse 200 204
         30.2 move 1 mouse 164 204
         30.3 up 1 mouse 164 204",
    );

    let shape = saved(&editor, "s");
    assert_eq!(number(&shape, "width"), 60.0);
    assert_eq!(
        (number(&shape, "y"), number(&shape, "height")),
        (100.0, 110.0)
    );
    assert_eq!(lines(&editor), ["aaaa", "bbbb", "cccc", "dddd"]);
    assert_eq!(number(&label(&editor), "y"), 105.0);
}

#[test]
fn a_shape_made_too_low_for_its_label_keeps_the_height_of_the_lines() {
    // Two lines of 25 in a shape 100 x 100.
    let resized = |handle: (f64, f64), to: (f64, f64)| {
        let mut editor = labelled("rectangle", 100.0, "hello world foo");
        tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
        common::replay(
            &mut editor,
            &format!(
                "30 down 1 mouse {} {}
                 30.1 move 1 mouse {} {}
                 30.2 up 1 mouse {} {}",
                handle.0, handle.1, to.0, to.1, to.0, to.1
            ),
        );
        let shape = saved(&editor, "s");
        (number(&shape, "y"), number(&shape, "height"))
    };

    // Pulled up by the bottom to 40 high, it is 50 + 2 * 5 padding high, from
    // its top.
    assert_eq!(resized((150.0, 204.0), (150.0, 144.0)), (100.0, 60.0));
    // Pushed down by the top, from its bottom.
    assert_eq!(resized((150.0, 96.0), (150.0, 156.0)), (140.0, 60.0));
}

#[test]
fn a_label_is_centred_vertically_whatever_the_height_of_the_shape() {
    let mut editor = labelled("rectangle", 200.0, "Hi");
    let label = label(&editor);
    assert_eq!(string(&label, "verticalAlign"), "middle");
    // 25 high in the 90 inside of a shape that is 100 high.
    assert_eq!(number(&label, "y"), 137.5);

    tap(&mut editor, "mouse", (150.0, 100.0), 20.0);
    // Dragging the south handle down 100 makes the shape 200 high.
    common::replay(
        &mut editor,
        "30 down 1 mouse 200 204
         30.1 move 1 mouse 200 254
         30.2 move 1 mouse 200 304
         30.3 up 1 mouse 200 304",
    );
    assert_eq!(number(&saved(&editor, "s"), "height"), 200.0);
    assert_eq!(number(&common::saved_elements(&editor)[1], "y"), 187.5);
}

// --- Excalidraw's own labels ----------------------------------------------------------------------

#[test]
fn editing_lays_a_label_out_as_excalidraw_did_in_its_own_fixture() {
    // `text.excalidraw` was made by Excalidraw's own code with the same fixed
    // character widths: every label in it sits as Excalidraw laid it out, in
    // rectangles, ellipses and diamonds, turned or not, at every alignment.
    let mut editor = common::editor_with_fixture("text");
    editor.set_text_measurer(Arc::new(FixedWidth));
    let originals = saved_elements(&editor);
    let labels: Vec<&Value> = originals
        .iter()
        .filter(|e| e.get("containerId").and_then(Value::as_str).is_some())
        .collect();
    assert!(labels.len() >= 10, "{} labels", labels.len());

    for original in labels {
        let container = string(original, "containerId");
        let c = saved(&editor, container);
        // Double-tap the middle of the container: it is the label's.
        let middle = (
            number(&c, "x") + number(&c, "width") / 2.0,
            number(&c, "y") + number(&c, "height") / 2.0,
        );
        let mut fresh = common::editor_with_fixture("text");
        fresh.set_text_measurer(Arc::new(FixedWidth));
        double_tap(&mut fresh, middle);
        let typed = string(original, "originalText").to_owned();
        fresh.set_editing_text("changed");
        fresh.set_editing_text(&typed);
        fresh.commit_text();

        let now = saved(&fresh, string(original, "id"));
        let id = string(original, "id");
        assert_eq!(string(&now, "text"), string(original, "text"), "{id}");
        for key in ["x", "y", "width", "height"] {
            let (a, b) = (number(&now, key), number(original, key));
            assert!((a - b).abs() < 1e-3, "{id}.{key}: {a} != {b}");
        }
        let c_now = saved(&fresh, container);
        assert_eq!(
            number(&c_now, "height"),
            number(&c, "height"),
            "{container}"
        );
    }
}

// --- The text Tool on shapes ---------------------------------------------------------

fn text_tap(editor: &mut Editor, at: (f64, f64)) {
    editor.set_tool(Tool::Text);
    tap(editor, "finger", at, 50.0);
}

fn live_texts(editor: &Editor) -> usize {
    saved_elements(editor)
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .count()
}

#[test]
fn a_text_tool_tap_near_the_middle_of_a_shape_makes_its_label() {
    for kind in ["rectangle", "diamond", "ellipse"] {
        let mut editor = scene_of(kind);
        text_tap(&mut editor, (MIDDLE.0 + 20.0, MIDDLE.1 - 20.0));
        let shown = open(&editor);
        assert!(shown.fixed_width, "{kind}");
        assert_eq!(shown.align, TextAlign::Center, "{kind}");
        editor.set_editing_text("Hi");
        editor.commit_text();
        assert_eq!(string(&label(&editor), "containerId"), "s", "{kind}");
    }
}

#[test]
fn a_text_tool_tap_on_a_label_edits_it() {
    let mut editor = labelled("rectangle", 200.0, "Hi");
    text_tap(&mut editor, MIDDLE);
    assert_eq!(open(&editor).text, "Hi");
    editor.set_editing_text("Hello");
    editor.commit_text();
    assert_eq!(live_texts(&editor), 1);
    assert_eq!(string(&label(&editor), "text"), "Hello");
}

#[test]
fn a_text_tool_tap_in_a_shape_but_away_from_the_middle_makes_a_free_text() {
    let mut editor = scene_of("rectangle");
    text_tap(&mut editor, (110.0, 110.0));
    assert!(!open(&editor).fixed_width);
    editor.set_editing_text("Free");
    editor.commit_text();
    assert!(
        label(&editor)
            .get("containerId")
            .is_none_or(|v| v.as_str().is_none())
    );
}

#[test]
fn a_text_tool_tap_on_a_labelled_shape_away_from_its_label_makes_a_free_text() {
    let mut editor = labelled("rectangle", 200.0, "Hi");
    text_tap(&mut editor, (110.0, 110.0));
    assert_eq!(open(&editor).text, "");
    assert!(!open(&editor).fixed_width);
}

#[test]
fn a_text_tool_tap_on_empty_canvas_makes_a_free_text() {
    let mut editor = scene_of("rectangle");
    text_tap(&mut editor, (600.0, 600.0));
    assert!(!open(&editor).fixed_width);
}

#[test]
fn the_middle_snap_is_30_view_points_whatever_the_zoom() {
    // Whether a text-Tool tap `dx` view points right of the middle, zoomed
    // in, makes a label.
    let makes_label = |dx: f64| {
        let mut editor = scene_of("rectangle");
        for _ in 0..10 {
            editor.zoom_in();
        }
        let view = editor.viewport();
        let (ox, oy) = view.origin();
        let middle = ((MIDDLE.0 - ox) * view.zoom(), (MIDDLE.1 - oy) * view.zoom());
        text_tap(&mut editor, (middle.0 + dx, middle.1));
        open(&editor).fixed_width
    };
    assert!(makes_label(25.0));
    assert!(!makes_label(35.0));
}

#[test]
fn text_tool_labels_undo_cancel_and_vanish_when_empty_as_double_tap_ones_do() {
    // Undo.
    let mut editor = scene_of("rectangle");
    text_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hi");
    editor.commit_text();
    editor.undo();
    assert_eq!(live_texts(&editor), 0);

    // Cancel.
    let mut editor = scene_of("rectangle");
    text_tap(&mut editor, MIDDLE);
    editor.set_editing_text("Hi");
    editor.cancel_text();
    assert_eq!(live_texts(&editor), 0);

    // Emptying a label deletes it.
    let mut editor = labelled("rectangle", 200.0, "Hi");
    text_tap(&mut editor, MIDDLE);
    editor.set_editing_text("  ");
    editor.commit_text();
    assert_eq!(live_texts(&editor), 0);
}
