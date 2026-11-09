//! Creating and editing text (ADR-0006), tested at the Editor seam: taps and
//! commands in, the editor the client shows over the Canvas, the saved
//! Drawing and the geometry out. The only test double is a `TextMeasurer`
//! with fixed character widths (half the font size).
//!
//! Expected values are worked out by hand: at font size 20 a character is 10
//! wide and a line 25 high (line height 1.25), at 28 they are 14 and 35.

use std::sync::Arc;

use pkm_core::json::Value;
use pkm_core::scene::LineStyle;
use pkm_core::{Editor, FontSize, Property, TextAlign, TextEditor, Tool};

use crate::common;
use common::{FixedWidth, editor_with, replay, saved_elements, tap};

fn measured(mut editor: Editor) -> Editor {
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor
}

fn empty() -> Editor {
    measured(Editor::new())
}

/// An editor with the text Tool active.
fn with_text_tool() -> Editor {
    let mut editor = empty();
    editor.set_tool(Tool::Text);
    editor
}

/// The editor after a tap at `(x, y)` with the text Tool, and `text` typed.
fn typing(text: &str, (x, y): (f64, f64)) -> Editor {
    let mut editor = with_text_tool();
    tap(&mut editor, "finger", (x, y), 1.0);
    editor.set_editing_text(text);
    editor
}

fn open(editor: &Editor) -> TextEditor {
    editor.text_editor().expect("a text editor is open")
}

fn texts(editor: &Editor) -> Vec<Value> {
    saved_elements(editor)
        .into_iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
        .filter(|e| e.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .collect()
}

fn only_text(editor: &Editor) -> Value {
    let texts = texts(editor);
    assert_eq!(texts.len(), 1, "{texts:?}");
    texts[0].clone()
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

fn string<'a>(element: &'a Value, key: &str) -> &'a str {
    element.get(key).and_then(Value::as_str).unwrap()
}

/// The lines of the Scene's geometry as drawn.
fn drawn(editor: &Editor) -> Vec<String> {
    let geometry = editor.geometry();
    geometry
        .text_lines()
        .iter()
        .map(|line| geometry.line_text(line).to_owned())
        .collect()
}

// --- Creating --------------------------------------------------------------------

#[test]
fn tapping_with_the_text_tool_opens_the_editor_at_the_tap() {
    let mut editor = with_text_tool();

    tap(&mut editor, "finger", (100.0, 200.0), 1.0);

    let shown = open(&editor);
    assert_eq!(shown.text, "");
    // The first line is centred on the tap: 12.5 above it.
    assert_eq!((shown.x, shown.y), (100.0, 187.5));
    assert_eq!(shown.font_size, 20.0);
    assert_eq!(shown.align, TextAlign::Left);
}

#[test]
fn the_tool_returns_to_selection_once_the_editor_is_open() {
    let mut editor = with_text_tool();
    tap(&mut editor, "finger", (100.0, 200.0), 1.0);
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn a_tool_lock_keeps_the_text_tool() {
    let mut editor = with_text_tool();
    editor.set_tool_lock(true);
    tap(&mut editor, "finger", (100.0, 200.0), 1.0);
    assert_eq!(editor.tool(), Tool::Text);
    assert!(editor.text_editor().is_some());
}

#[test]
fn committing_creates_the_text_element() {
    let mut editor = typing("Hello", (100.0, 200.0));

    editor.commit_text();

    assert!(editor.text_editor().is_none());
    let text = only_text(&editor);
    assert_eq!(string(&text, "text"), "Hello");
    assert_eq!(string(&text, "originalText"), "Hello");
    assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 187.5));
    assert_eq!(
        (number(&text, "width"), number(&text, "height")),
        (50.0, 25.0)
    );
    assert_eq!(number(&text, "fontSize"), 20.0);
    assert_eq!(number(&text, "fontFamily"), 5.0);
    assert_eq!(number(&text, "lineHeight"), 1.25);
    assert_eq!(string(&text, "textAlign"), "left");
    assert_eq!(string(&text, "verticalAlign"), "top");
    assert_eq!(string(&text, "strokeColor"), "#1e1e1e");
    assert_eq!(text.get("containerId"), Some(&Value::Null));
    assert_eq!(text.get("autoResize"), Some(&Value::Bool(true)));
}

#[test]
fn the_new_text_is_selected() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.commit_text();
    assert_eq!(editor.selection().len(), 1);
}

#[test]
fn empty_text_cancels() {
    let mut editor = typing("", (100.0, 200.0));
    editor.commit_text();
    assert!(editor.text_editor().is_none());
    assert!(texts(&editor).is_empty());
    assert!(!editor.can_undo());
}

#[test]
fn text_of_only_spaces_and_line_breaks_cancels() {
    let mut editor = typing("  \n ", (100.0, 200.0));
    editor.commit_text();
    assert!(texts(&editor).is_empty());
}

#[test]
fn cancelling_discards_what_was_typed() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.cancel_text();
    assert!(editor.text_editor().is_none());
    assert!(texts(&editor).is_empty());
}

#[test]
fn the_editor_follows_the_text_as_it_is_typed() {
    let mut editor = typing("Hello\nWorld!", (100.0, 200.0));

    let shown = open(&editor);
    assert_eq!(shown.text, "Hello\nWorld!");
    assert_eq!((shown.width, shown.height), (60.0, 50.0));
    assert_eq!((shown.x, shown.y), (100.0, 187.5));

    editor.set_editing_text("Hi");
    let shown = open(&editor);
    assert_eq!((shown.width, shown.height), (20.0, 25.0));
}

#[test]
fn the_text_is_not_drawn_while_the_editor_shows_it() {
    let mut editor = typing("Hello", (100.0, 200.0));
    assert_eq!(drawn(&editor), Vec::<String>::new());

    editor.commit_text();
    assert_eq!(drawn(&editor), ["Hello"]);
}

#[test]
fn creating_text_is_one_undo_step() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.set_editing_text("Hello world");
    editor.commit_text();
    assert!(editor.can_undo());

    editor.undo();
    assert!(texts(&editor).is_empty());
    assert!(editor.selection().is_empty());
    assert!(!editor.can_undo());

    editor.redo();
    assert_eq!(string(&only_text(&editor), "text"), "Hello world");
}

#[test]
fn a_tap_elsewhere_commits_the_text() {
    let mut editor = typing("Hello", (100.0, 200.0));

    tap(&mut editor, "finger", (400.0, 400.0), 2.0);

    assert!(editor.text_editor().is_none());
    assert_eq!(string(&only_text(&editor), "text"), "Hello");
}

#[test]
fn two_fingers_panning_do_not_commit_the_text() {
    let mut editor = typing("Hello", (100.0, 200.0));

    replay(
        &mut editor,
        "2.0 down 1 finger 300 300
         2.0 down 2 finger 340 300
         2.1 move 1 finger 300 340
         2.1 move 2 finger 340 340
         2.2 up 1 finger 300 340
         2.2 up 2 finger 340 340",
    );

    assert!(editor.text_editor().is_some());
}

#[test]
fn changing_the_tool_commits_the_text() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.set_tool(Tool::Rectangle);
    assert!(editor.text_editor().is_none());
    assert_eq!(string(&only_text(&editor), "text"), "Hello");
}

#[test]
fn commands_commit_the_text_first() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.undo();
    assert!(editor.text_editor().is_none());
    // The commit was the latest change: undoing took it back.
    assert!(texts(&editor).is_empty());
}

#[test]
fn a_tap_with_the_text_tool_on_text_edits_it() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor.commit_text();
    editor.set_tool(Tool::Text);

    tap(&mut editor, "finger", (120.0, 200.0), 5.0);

    assert_eq!(open(&editor).text, "Hello");
    assert_eq!(texts(&editor).len(), 1);
}

#[test]
fn no_editor_opens_without_a_measurer() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Text);
    tap(&mut editor, "finger", (100.0, 200.0), 1.0);
    assert!(editor.text_editor().is_none());
}

// --- Editing ---------------------------------------------------------------------

const HELLO: &str = r##"{"id":"t","type":"text","x":100,"y":50,"width":50,"height":25,
    "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
    "textAlign":"left","verticalAlign":"top","strokeColor":"#e03131","opacity":100,
    "containerId":null,"autoResize":true,"version":3,"versionNonce":7}"##;

fn with_hello() -> Editor {
    measured(editor_with(&[HELLO]))
}

/// Double-taps the text at `(x, y)`.
fn double_tap(editor: &mut Editor, (x, y): (f64, f64)) {
    tap(editor, "finger", (x, y), 10.0);
    tap(editor, "finger", (x, y), 10.2);
}

#[test]
fn a_double_tap_on_text_opens_the_editor_on_it() {
    let mut editor = with_hello();

    double_tap(&mut editor, (120.0, 60.0));

    let shown = open(&editor);
    assert_eq!(shown.text, "Hello");
    assert_eq!((shown.x, shown.y), (100.0, 50.0));
    assert_eq!((shown.width, shown.height), (50.0, 25.0));
}

#[test]
fn a_double_tap_on_empty_canvas_opens_the_editor_at_the_tap() {
    let mut editor = empty();

    double_tap(&mut editor, (100.0, 200.0));

    let shown = open(&editor);
    assert_eq!(shown.text, "");
    assert_eq!((shown.x, shown.y), (100.0, 187.5));
    assert_eq!(editor.tool(), Tool::Selection);

    editor.set_editing_text("Hello");
    editor.commit_text();
    assert_eq!(string(&only_text(&editor), "text"), "Hello");
    assert_eq!(editor.selection().len(), 1);
}

#[test]
fn a_double_click_with_a_mouse_on_empty_canvas_makes_a_text() {
    let mut editor = empty();
    tap(&mut editor, "mouse", (100.0, 200.0), 10.0);
    tap(&mut editor, "mouse", (100.0, 200.0), 10.1);
    assert!(editor.text_editor().is_some());
}

#[test]
fn a_single_tap_on_empty_canvas_makes_no_text() {
    let mut editor = empty();
    tap(&mut editor, "finger", (100.0, 200.0), 10.0);
    assert!(editor.text_editor().is_none());
}

#[test]
fn a_double_tap_on_empty_canvas_makes_one_text_not_two() {
    let mut editor = empty();
    double_tap(&mut editor, (100.0, 200.0));
    editor.set_editing_text("A");
    editor.commit_text();
    // A third tap soon after starts a new double-tap; it makes nothing alone.
    tap(&mut editor, "finger", (400.0, 400.0), 10.4);
    assert!(editor.text_editor().is_none());
    assert_eq!(texts(&editor).len(), 1);
}

const LINE: &str = r#"{"id":"l","type":"line","x":100,"y":300,"width":100,"height":0,
    "points":[[0,0],[100,0]],"strokeWidth":2,"version":3,"versionNonce":7}"#;

const STROKE: &str = r#"{"id":"f","type":"freedraw","x":100,"y":300,"width":100,"height":0,
    "points":[[0,0],[50,0],[100,0]],"pressures":[],"simulatePressure":true,
    "strokeWidth":2,"version":3,"versionNonce":7}"#;

#[test]
fn a_double_tap_on_a_stroke_makes_a_text_at_the_tap() {
    let mut editor = measured(editor_with(&[STROKE]));

    double_tap(&mut editor, (150.0, 300.0));

    let shown = open(&editor);
    assert_eq!((shown.x, shown.y), (150.0, 287.5));
    editor.set_editing_text("Hi");
    editor.commit_text();
    assert_eq!(string(&only_text(&editor), "text"), "Hi");
}

#[test]
fn a_double_tap_on_the_only_selected_line_makes_no_text() {
    // Excalidraw edits the points of that line instead.
    let mut editor = measured(editor_with(&[LINE]));
    double_tap(&mut editor, (150.0, 300.0));
    assert!(editor.text_editor().is_none());
}

#[test]
fn a_double_tap_on_an_image_makes_no_text() {
    // Excalidraw crops it instead.
    let image = r#"{"id":"i","type":"image","x":100,"y":300,"width":100,"height":50,
        "fileId":"f1","status":"saved","scale":[1,1],"version":3,"versionNonce":7}"#;
    let mut editor = measured(editor_with(&[image]));
    double_tap(&mut editor, (150.0, 325.0));
    assert!(editor.text_editor().is_none());
}

#[test]
fn a_single_tap_on_text_only_selects_it() {
    let mut editor = with_hello();
    tap(&mut editor, "finger", (120.0, 60.0), 10.0);
    assert!(editor.text_editor().is_none());
    assert_eq!(editor.selection(), ["t"]);
}

#[test]
fn a_double_click_with_a_mouse_edits_too() {
    let mut editor = with_hello();
    tap(&mut editor, "mouse", (120.0, 60.0), 10.0);
    tap(&mut editor, "mouse", (120.0, 60.0), 10.1);
    assert_eq!(open(&editor).text, "Hello");
}

#[test]
fn the_edited_text_is_not_drawn_under_the_editor() {
    let mut editor = with_hello();
    assert_eq!(drawn(&editor), ["Hello"]);

    double_tap(&mut editor, (120.0, 60.0));
    assert_eq!(drawn(&editor), Vec::<String>::new());

    editor.commit_text();
    assert_eq!(drawn(&editor), ["Hello"]);
}

#[test]
fn committing_saves_the_new_text_and_its_size() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));

    editor.set_editing_text("Hello world\nagain");
    editor.commit_text();

    let text = only_text(&editor);
    assert_eq!(string(&text, "text"), "Hello world\nagain");
    assert_eq!(string(&text, "originalText"), "Hello world\nagain");
    assert_eq!(
        (number(&text, "width"), number(&text, "height")),
        (110.0, 50.0)
    );
    assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 50.0));
    assert_eq!(editor.selection(), ["t"]);
}

#[test]
fn editing_is_one_undo_step() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));
    editor.set_editing_text("Hi");
    editor.set_editing_text("Hi there");
    editor.commit_text();

    editor.undo();
    let text = only_text(&editor);
    assert_eq!(string(&text, "text"), "Hello");
    assert_eq!(number(&text, "width"), 50.0);

    editor.redo();
    assert_eq!(string(&only_text(&editor), "text"), "Hi there");
}

#[test]
fn committing_without_a_change_is_no_undo_step() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));
    editor.commit_text();
    assert!(!editor.can_undo());
}

#[test]
fn cancelling_keeps_the_text_as_it_was() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));
    editor.set_editing_text("Something else");
    editor.cancel_text();
    assert_eq!(string(&only_text(&editor), "text"), "Hello");
    assert!(!editor.can_undo());
}

#[test]
fn emptying_a_text_deletes_it_and_undo_brings_it_back() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));
    editor.set_editing_text("");
    editor.commit_text();
    assert!(texts(&editor).is_empty());
    assert!(editor.selection().is_empty());

    editor.undo();
    assert_eq!(string(&only_text(&editor), "text"), "Hello");
}

#[test]
fn a_double_tap_into_a_selected_group_still_goes_into_the_group() {
    let second = HELLO
        .replace(r#""id":"t""#, r#""id":"u""#)
        .replace("100,\"y\"", "300,\"y\"");
    let first = HELLO.replace(r#""type":"text""#, r#""type":"text","groupIds":["g"]"#);
    let second = second.replace(r#""type":"text""#, r#""type":"text","groupIds":["g"]"#);
    let mut editor = measured(editor_with(&[&first, &second]));

    double_tap(&mut editor, (120.0, 60.0));

    assert!(editor.text_editor().is_none());
}

#[test]
fn text_of_a_fixed_width_wraps_to_it_while_it_is_edited() {
    let fixed = HELLO
        .replace(r#""width":50"#, r#""width":60"#)
        .replace(r#""autoResize":true"#, r#""autoResize":false"#);
    let mut editor = measured(editor_with(&[&fixed]));
    double_tap(&mut editor, (120.0, 60.0));
    assert!(open(&editor).fixed_width);

    editor.set_editing_text("Hello there");

    let shown = open(&editor);
    assert_eq!((shown.width, shown.height), (60.0, 50.0));
    assert_eq!(shown.text, "Hello there");
    editor.commit_text();
    let text = only_text(&editor);
    assert_eq!(string(&text, "text"), "Hello\nthere");
    assert_eq!(string(&text, "originalText"), "Hello there");
}

#[test]
fn the_text_tool_creates_a_text_where_another_element_is() {
    let rectangle = r##"{"id":"r","type":"rectangle","x":50,"y":50,"width":200,"height":100,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,
        "version":3,"versionNonce":7}"##;
    let mut editor = measured(editor_with(&[rectangle]));
    editor.set_tool(Tool::Text);

    tap(&mut editor, "finger", (150.0, 100.0), 1.0);
    editor.set_editing_text("Label");
    editor.commit_text();

    assert_eq!(string(&only_text(&editor), "text"), "Label");
    assert_eq!(saved_elements(&editor).len(), 2);
}

#[test]
fn with_the_tool_lock_the_text_tool_stays_for_the_next_text() {
    let mut editor = with_text_tool();
    editor.set_tool_lock(true);
    tap(&mut editor, "finger", (100.0, 200.0), 1.0);
    editor.set_editing_text("One");
    editor.commit_text();
    assert_eq!(editor.tool(), Tool::Text);
    assert!(editor.selection().is_empty());

    tap(&mut editor, "finger", (100.0, 300.0), 2.0);
    editor.set_editing_text("Two");
    editor.commit_text();
    assert_eq!(texts(&editor).len(), 2);
}

#[test]
fn loading_a_drawing_closes_the_editor() {
    let mut editor = typing("Hello", (100.0, 200.0));
    editor
        .load(br#"{"type":"excalidraw","version":2,"elements":[]}"#)
        .unwrap();
    assert!(editor.text_editor().is_none());
}

// --- Where the editor is ----------------------------------------------------------

#[test]
fn the_editor_follows_panning_and_zooming() {
    let mut editor = with_hello();
    editor.set_view_size(800.0, 600.0);
    double_tap(&mut editor, (120.0, 60.0));
    assert_eq!(open(&editor).zoom, 1.0);

    editor.zoom_in();
    // Zooming acts around the middle of the view, (400, 300): the point
    // (100, 50) of the Scene moves away from it with the zoom of 1.1.
    let shown = open(&editor);
    assert_eq!(shown.zoom, 1.1);
    assert!(
        (shown.x - (400.0 + (100.0 - 400.0) * 1.1)).abs() < 1e-9,
        "{}",
        shown.x
    );
    assert!(
        (shown.y - (300.0 + (50.0 - 300.0) * 1.1)).abs() < 1e-9,
        "{}",
        shown.y
    );
    assert!((shown.font_size - 22.0).abs() < 1e-9);
    assert!((shown.width - 55.0).abs() < 1e-9);
    assert!((shown.height - 27.5).abs() < 1e-9);
}

#[test]
fn the_editor_has_the_style_of_the_text() {
    let mut editor = with_hello();
    double_tap(&mut editor, (120.0, 60.0));
    let shown = open(&editor);
    assert_eq!(shown.color, 0xe03131ff);
    assert_eq!(shown.opacity, 1.0);
    assert_eq!(shown.font_family, 5);
    assert_eq!(shown.angle, 0.0);
}

#[test]
fn a_turned_text_is_edited_turned() {
    let turned = HELLO.replace(r#""text":"Hello""#, r#""angle":0.5,"text":"Hello""#);
    let mut editor = measured(editor_with(&[&turned]));
    double_tap(&mut editor, (125.0, 62.5));
    assert_eq!(open(&editor).angle, 0.5);
}

// --- Alignment keeps the text where it is anchored -----------------------------------

#[test]
fn centred_text_grows_to_both_sides() {
    let mut editor = with_text_tool();
    editor.set_property(Property::TextAlign(TextAlign::Center));
    tap(&mut editor, "finger", (200.0, 100.0), 1.0);

    editor.set_editing_text("Hello");
    editor.commit_text();

    // A space is 10 wide, so the empty text is centred on the tap with its
    // left edge at 195; five characters are 50 wide: 175.
    let text = only_text(&editor);
    assert_eq!(string(&text, "textAlign"), "center");
    assert_eq!((number(&text, "x"), number(&text, "width")), (175.0, 50.0));
}

#[test]
fn right_aligned_text_grows_to_the_left() {
    let mut editor = with_text_tool();
    editor.set_property(Property::TextAlign(TextAlign::Right));
    tap(&mut editor, "finger", (200.0, 100.0), 1.0);

    editor.set_editing_text("Hello");
    editor.commit_text();

    let text = only_text(&editor);
    assert_eq!((number(&text, "x"), number(&text, "width")), (150.0, 50.0));
}

// --- Font size and alignment ----------------------------------------------------------

#[test]
fn the_font_size_of_the_next_text_is_the_chosen_one() {
    let mut editor = with_text_tool();
    editor.set_property(Property::FontSize(FontSize::Large));
    tap(&mut editor, "finger", (100.0, 200.0), 1.0);
    assert_eq!(open(&editor).font_size, 28.0);

    editor.set_editing_text("Hi");
    editor.commit_text();

    let text = only_text(&editor);
    assert_eq!(number(&text, "fontSize"), 28.0);
    assert_eq!(
        (number(&text, "width"), number(&text, "height")),
        (28.0, 35.0)
    );
}

#[test]
fn the_panel_shows_the_font_size_and_alignment_of_the_next_text() {
    let mut editor = empty();
    assert_eq!(editor.properties().font_size, Some(FontSize::Medium));
    assert_eq!(editor.properties().text_align, Some(TextAlign::Left));
    editor.set_property(Property::FontSize(FontSize::ExtraLarge));
    editor.set_property(Property::TextAlign(TextAlign::Right));
    assert_eq!(editor.properties().font_size, Some(FontSize::ExtraLarge));
    assert_eq!(editor.properties().text_align, Some(TextAlign::Right));
}

#[test]
fn the_panel_offers_font_size_and_alignment_for_texts_only() {
    let rectangle = r##"{"id":"r","type":"rectangle","x":300,"y":300,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,
        "version":3,"versionNonce":7}"##;
    let mut editor = measured(editor_with(&[HELLO, rectangle]));
    // Nothing selected: for the next Element, which is a text with the text Tool.
    assert!(!editor.properties().sections.text);
    editor.set_tool(Tool::Text);
    assert!(editor.properties().sections.text);
    editor.set_tool(Tool::Selection);

    tap(&mut editor, "finger", (320.0, 300.0), 1.0);
    assert_eq!(editor.selection(), ["r"]);
    assert!(!editor.properties().sections.text);

    editor.select_all();
    assert!(editor.properties().sections.text);
}

#[test]
fn the_panel_offers_font_size_and_alignment_while_a_text_is_edited() {
    let editor = typing("Hi", (100.0, 200.0));
    assert!(editor.properties().sections.text);
}

#[test]
fn the_panel_shows_the_font_size_and_alignment_of_the_selected_text() {
    let mut editor = with_hello();
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);
    assert_eq!(editor.properties().font_size, Some(FontSize::Medium));
    assert_eq!(editor.properties().text_align, Some(TextAlign::Left));
}

#[test]
fn a_font_size_that_is_none_of_the_options_shows_none_chosen() {
    let odd = HELLO.replace(r#""fontSize":20"#, r#""fontSize":21"#);
    let mut editor = measured(editor_with(&[&odd]));
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);
    assert_eq!(editor.properties().font_size, None);
}

#[test]
fn the_font_size_applies_to_the_selected_text_and_resizes_it() {
    let mut editor = with_hello();
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);

    editor.set_property(Property::FontSize(FontSize::Large));

    let text = only_text(&editor);
    assert_eq!(number(&text, "fontSize"), 28.0);
    // Five characters of 14, one line of 35; anchored at the left and top.
    assert_eq!(
        (number(&text, "width"), number(&text, "height")),
        (70.0, 35.0)
    );
    assert_eq!((number(&text, "x"), number(&text, "y")), (100.0, 50.0));
    assert_eq!(editor.properties().font_size, Some(FontSize::Large));
}

#[test]
fn the_font_size_of_a_centred_text_grows_about_its_middle() {
    let centred = HELLO.replace(r#""textAlign":"left""#, r#""textAlign":"center""#);
    let mut editor = measured(editor_with(&[&centred]));
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);

    editor.set_property(Property::FontSize(FontSize::Large));

    // 50 wide at 100 becomes 70 wide: 10 further left.
    assert_eq!(number(&only_text(&editor), "x"), 90.0);
}

#[test]
fn the_font_size_is_one_undo_step() {
    let mut editor = with_hello();
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);
    editor.set_property(Property::FontSize(FontSize::Large));

    editor.undo();

    let text = only_text(&editor);
    assert_eq!(number(&text, "fontSize"), 20.0);
    assert_eq!(number(&text, "width"), 50.0);
}

#[test]
fn the_alignment_applies_to_the_selected_text() {
    let mut editor = with_hello();
    tap(&mut editor, "finger", (120.0, 60.0), 1.0);

    editor.set_property(Property::TextAlign(TextAlign::Right));

    assert_eq!(string(&only_text(&editor), "textAlign"), "right");
    assert_eq!(editor.properties().text_align, Some(TextAlign::Right));
    let geometry = editor.geometry();
    assert_eq!(geometry.text_lines()[0].align, TextAlign::Right as u32);
}

#[test]
fn font_size_and_alignment_leave_other_elements_alone() {
    let rectangle = r##"{"id":"r","type":"rectangle","x":300,"y":300,"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"transparent","roundness":null,
        "version":3,"versionNonce":7}"##;
    let mut editor = measured(editor_with(&[HELLO, rectangle]));
    editor.select_all();
    assert_eq!(editor.properties().font_size, Some(FontSize::Medium));

    editor.set_property(Property::FontSize(FontSize::Small));

    let rectangle = saved_elements(&editor)
        .into_iter()
        .find(|e| string(e, "id") == "r")
        .unwrap();
    assert_eq!(rectangle.get("fontSize"), None);
    assert_eq!(number(&only_text(&editor), "fontSize"), 16.0);
}

#[test]
fn the_font_size_applies_to_the_text_being_edited() {
    let mut editor = typing("Hi", (100.0, 200.0));

    editor.set_property(Property::FontSize(FontSize::ExtraLarge));

    let shown = open(&editor);
    assert_eq!(shown.font_size, 36.0);
    assert_eq!((shown.width, shown.height), (36.0, 45.0));
    // The editor stays open: the panel is a change of the text, not an end.
    editor.commit_text();
    assert_eq!(number(&only_text(&editor), "fontSize"), 36.0);
    // And the next text has the size too.
    assert_eq!(editor.properties().font_size, Some(FontSize::ExtraLarge));
}

#[test]
fn the_alignment_applies_to_the_text_being_edited() {
    let mut editor = typing("Hi", (100.0, 200.0));
    editor.set_property(Property::TextAlign(TextAlign::Center));
    assert_eq!(open(&editor).align, TextAlign::Center);
    editor.commit_text();
    assert_eq!(string(&only_text(&editor), "textAlign"), "center");
}

#[test]
fn other_properties_apply_to_the_text_being_edited_too() {
    let mut editor = typing("Hi", (100.0, 200.0));
    editor.set_property(Property::StrokeColor("#1971c2".into()));
    assert_eq!(open(&editor).color, 0x1971c2ff);
    editor.commit_text();
    assert_eq!(string(&only_text(&editor), "strokeColor"), "#1971c2");
}

#[test]
fn properties_that_text_does_not_have_leave_it_alone() {
    let mut editor = typing("Hi", (100.0, 200.0));
    editor.set_property(Property::StrokeStyle(LineStyle::Dashed));
    editor.commit_text();
    assert_eq!(string(&only_text(&editor), "strokeStyle"), "solid");
}
