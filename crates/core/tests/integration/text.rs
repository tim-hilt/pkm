//! Text Elements (ADR-0008), tested through `Editor::geometry`: the core
//! describes text as positioned lines and never lays out glyphs. Core tests
//! use a fake `TextMeasurer` with fixed character widths.

use crate::common;

use std::sync::Arc;

use common::{FixedWidth, editor_with_element, editor_with_fixture};
use workingtitle_core::{Editor, Geometry, TextAlign, TextLine, TextMeasurer};

fn measured(element: &str) -> Editor {
    let mut editor = editor_with_element(element);
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor
}

/// The lines of `geometry` as `(text, x, y)`.
fn lines(geometry: &Geometry) -> Vec<(&str, f32, f32)> {
    geometry
        .text_lines()
        .iter()
        .map(|line| (geometry.line_text(line), line.x, line.y))
        .collect()
}

fn only_line(geometry: &Geometry) -> TextLine {
    assert_eq!(geometry.text_lines().len(), 1, "{:?}", lines(geometry));
    geometry.text_lines()[0]
}

const HELLO: &str = r##""id":"t","type":"text","x":100,"y":50,"width":50,"height":25,
    "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
    "textAlign":"left","verticalAlign":"top","strokeColor":"#e03131","opacity":50"##;

// --- Unbound text -----------------------------------------------------------

#[test]
fn a_text_element_is_described_as_a_line_of_text_not_as_paths() {
    let geometry = editor_with_element(HELLO).geometry();

    assert!(geometry.parts().is_empty(), "no box around text");
    let line = only_line(&geometry);
    assert_eq!(geometry.line_text(&line), "Hello");
    assert_eq!(line.font_size, 20.0);
    assert_eq!(line.font_family, 5);
    assert_eq!(line.align, TextAlign::Left as u32);
    assert_eq!(line.color, 0xe03131ff);
    assert_eq!(line.opacity, 0.5);
}

#[test]
fn the_line_sits_on_the_baseline_of_its_box() {
    // Excalifont: ascender 886 and descender -374 per 1000 units, so 17.72
    // above and 7.48 below the baseline at 20 pt. A line height of 25 leaves
    // (25 - 17.72 - 7.48) / 2 = -0.1 around them: the baseline is 17.62
    // below the top.
    let geometry = editor_with_element(HELLO).geometry();

    let line = only_line(&geometry);
    assert_eq!(line.x, 100.0);
    assert!((line.y - (50.0 + 17.62)).abs() < 1e-4, "{}", line.y);
}

#[test]
fn every_line_of_a_text_is_positioned_one_line_height_below_the_last() {
    let geometry = editor_with_element(
        r#""id":"t","type":"text","x":0,"y":0,"width":60,"height":75,
        "text":"one\ntwo\r\nthree","fontSize":20,"lineHeight":1.25"#,
    )
    .geometry();

    let ys: Vec<f32> = geometry.text_lines().iter().map(|l| l.y).collect();
    let texts: Vec<&str> = lines(&geometry).iter().map(|l| l.0).collect();
    assert_eq!(texts, ["one", "two", "three"]);
    assert!((ys[1] - ys[0] - 25.0).abs() < 1e-4);
    assert!((ys[2] - ys[1] - 25.0).abs() < 1e-4);
}

#[test]
fn alignment_anchors_the_lines_at_the_left_middle_or_right_of_the_box() {
    let anchor = |align: &str| {
        let geometry = editor_with_element(&format!(
            r#""id":"t","type":"text","x":100,"y":0,"width":80,"height":25,
            "text":"Hi","fontSize":20,"textAlign":"{align}""#
        ))
        .geometry();
        let line = only_line(&geometry);
        (line.x, line.align)
    };

    assert_eq!(anchor("left"), (100.0, TextAlign::Left as u32));
    assert_eq!(anchor("center"), (140.0, TextAlign::Center as u32));
    assert_eq!(anchor("right"), (180.0, TextAlign::Right as u32));
}

#[test]
fn text_is_drawn_in_z_order_between_the_paths_around_it() {
    let mut editor = Editor::new();
    editor
        .load(
            br#"{"type":"excalidraw","elements":[
            {"id":"a","type":"rectangle","width":10,"height":10},
            {"id":"t","type":"text","text":"x","width":10,"height":25},
            {"id":"b","type":"rectangle","width":10,"height":10}]}"#,
        )
        .unwrap();
    let geometry = editor.geometry();

    let line = only_line(&geometry);
    let before = geometry.parts()[..line.part_index as usize].to_vec();
    assert!(!before.is_empty() && before.iter().all(|p| p.element == 0));
    assert!(
        geometry.parts()[line.part_index as usize..]
            .iter()
            .all(|p| p.element == 2)
    );
    assert_eq!(line.element, 1);
}

#[test]
fn a_rotated_text_turns_about_the_middle_of_its_box() {
    // The box is 40 wide and 25 high around (120, 12.5); turned a quarter
    // clockwise, its left edge is at the top.
    let geometry = editor_with_element(
        r#""id":"t","type":"text","x":100,"y":0,"width":40,"height":25,
        "text":"Hi","fontSize":20,"lineHeight":1.25,"angle":1.5707963267948966"#,
    )
    .geometry();

    let line = only_line(&geometry);
    assert!((line.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    // The unrotated anchor (100, 17.62) is 20 left of and 5.12 below the
    // middle; a quarter clockwise puts it 5.12 left of and 20 above it.
    assert!((line.x - (120.0 - 5.12)).abs() < 1e-3, "{}", line.x);
    assert!((line.y - (12.5 - 20.0)).abs() < 1e-3, "{}", line.y);
}

#[test]
fn deleted_and_empty_texts_draw_nothing() {
    for element in [
        r#""id":"t","type":"text","text":"gone","isDeleted":true"#,
        r#""id":"t","type":"text","text":"""#,
    ] {
        assert!(
            editor_with_element(element)
                .geometry()
                .text_lines()
                .is_empty()
        );
    }
}

#[test]
fn other_font_families_are_drawn_as_excalifont_but_keep_their_family() {
    let editor = editor_with_element(
        r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":23,
        "text":"Hi","fontSize":20,"fontFamily":2,"lineHeight":1.15"#,
    );
    let line = only_line(&editor.geometry());
    assert_eq!(line.font_family, 2);
    assert!(
        String::from_utf8(editor.save())
            .unwrap()
            .contains(r#""fontFamily": 2"#)
    );
}

// --- Wrapping and labels ----------------------------------------------------

/// A rectangle `id` of the given width and height with the label `text`
/// (typed as `original`, saved as `text`).
fn labelled(container: &str, text: &str, original: &str, label: &str) -> Editor {
    let mut editor = Editor::new();
    let drawing = format!(
        r#"{{"type":"excalidraw","elements":[
        {{"id":"box","type":"rectangle",{container},"boundElements":[{{"type":"text","id":"t"}}]}},
        {{"id":"t","type":"text","containerId":"box","fontSize":20,"lineHeight":1.25,
          "text":"{text}","originalText":"{original}",{label}}}]}}"#
    );
    editor.load(drawing.as_bytes()).unwrap();
    editor.set_text_measurer(Arc::new(FixedWidth));
    editor
}

fn texts(geometry: &Geometry) -> Vec<&str> {
    lines(geometry).iter().map(|l| l.0).collect()
}

#[test]
fn a_label_wraps_to_the_width_of_its_container_less_padding() {
    // 100 wide less 5 padding on each side leaves 90: nine characters of 10.
    let editor = labelled(
        r#""x":0,"y":0,"width":100,"height":200"#,
        "aaaa bbbb cccc",
        "aaaa bbbb cccc",
        r#""x":5,"y":5,"width":90,"height":25,"textAlign":"left","verticalAlign":"top""#,
    );

    assert_eq!(texts(&editor.geometry()), ["aaaa bbbb", "cccc"]);
}

#[test]
fn a_label_wraps_again_when_the_container_is_not_the_size_it_was_wrapped_for() {
    // Saved for a wide container, now 60 wide: six characters fit.
    let editor = labelled(
        r#""x":0,"y":0,"width":70,"height":200"#,
        "aaaa bbbb cccc",
        "aaaa bbbb cccc",
        r#""x":5,"y":5,"width":130,"height":25,"textAlign":"left","verticalAlign":"top""#,
    );

    assert_eq!(texts(&editor.geometry()), ["aaaa", "bbbb", "cccc"]);
}

#[test]
fn without_a_measurer_the_lines_are_the_ones_saved_in_the_file() {
    let mut editor = labelled(
        r#""x":0,"y":0,"width":100,"height":200"#,
        "aaaa bbbb\\ncccc",
        "aaaa bbbb cccc",
        r#""x":5,"y":5,"width":90,"height":50,"textAlign":"left","verticalAlign":"top""#,
    );
    let mut unmeasured = Editor::new();
    unmeasured.load(&editor.save()).unwrap();
    assert_eq!(texts(&unmeasured.geometry()), ["aaaa bbbb", "cccc"]);

    // The measurer is a choice of the client; replacing it changes the lines.
    struct Narrow;
    impl TextMeasurer for Narrow {
        fn line_width(&self, text: &str, _: f64) -> f64 {
            text.chars().count() as f64 * 20.0
        }
    }
    editor.set_text_measurer(Arc::new(Narrow));
    assert_eq!(texts(&editor.geometry()), ["aaaa", "bbbb", "cccc"]);
}

#[test]
fn a_label_is_placed_in_its_container_by_its_alignment() {
    // Container 200 x 100 at (10, 20); the label is one line of "ab", 20 wide
    // and 25 high, in the padded area (15, 25) to (205, 115).
    let place = |align: &str, vertical: &str| {
        let editor = labelled(
            r#""x":10,"y":20,"width":200,"height":100"#,
            "ab",
            "ab",
            &format!(
                r#""x":0,"y":0,"width":20,"height":25,"textAlign":"{align}","verticalAlign":"{vertical}""#
            ),
        );
        let geometry = editor.geometry();
        let line = only_line(&geometry);
        // The baseline is 17.62 below the top of the line.
        (line.x, line.y - 17.62)
    };
    let near = |(x, y): (f32, f32), (ex, ey): (f32, f32)| {
        assert!(
            (x - ex).abs() < 1e-3 && (y - ey).abs() < 1e-3,
            "{x}, {y} != {ex}, {ey}"
        );
    };

    near(place("left", "top"), (15.0, 25.0));
    // The anchor of a line is its start, middle or end, by alignment.
    near(place("center", "middle"), (15.0 + 95.0, 25.0 + 45.0 - 12.5));
    near(place("right", "bottom"), (15.0 + 190.0, 25.0 + 90.0 - 25.0));
}

#[test]
fn a_label_in_an_ellipse_or_diamond_uses_the_rectangle_inscribed_in_it() {
    let width = |kind: &str| {
        let mut editor = Editor::new();
        let drawing = format!(
            r#"{{"type":"excalidraw","elements":[
            {{"id":"box","type":"{kind}","x":0,"y":0,"width":200,"height":200}},
            {{"id":"t","type":"text","containerId":"box","fontSize":20,"text":"x",
              "originalText":"aaa bbb ccc ddd eee","x":0,"y":0,"width":10,"height":25}}]}}"#
        );
        editor.load(drawing.as_bytes()).unwrap();
        editor.set_text_measurer(Arc::new(FixedWidth));
        // Characters of the longest line.
        texts(&editor.geometry())
            .iter()
            .map(|l| l.len())
            .max()
            .unwrap()
    };

    // Ellipse: round(200 / 2 * sqrt 2) - 10 = 131 -> 13 characters, so
    // "aaa bbb ccc" fits and "aaa bbb ccc ddd" does not.
    assert_eq!(width("ellipse"), 11);
    // Diamond: round(200 / 2) - 10 = 90 -> 9 characters: "aaa bbb".
    assert_eq!(width("diamond"), 7);
}

#[test]
fn text_bound_to_an_arrow_stays_where_the_file_has_it() {
    let mut editor = Editor::new();
    editor
        .load(
            br#"{"type":"excalidraw","elements":[
            {"id":"a","type":"arrow","x":0,"y":0,"width":200,"height":0,"points":[[0,0],[200,0]],
             "boundElements":[{"type":"text","id":"t"}]},
            {"id":"t","type":"text","containerId":"a","fontSize":20,"text":"on the arrow",
             "originalText":"on the arrow","x":40,"y":-12,"width":120,"height":25}]}"#,
        )
        .unwrap();
    editor.set_text_measurer(Arc::new(FixedWidth));
    let geometry = editor.geometry();

    assert_eq!(texts(&geometry), ["on the arrow"]);
    assert_eq!(geometry.text_lines()[0].x, 40.0);
}

#[test]
fn a_text_with_a_width_of_its_own_wraps_to_it() {
    let editor = measured(
        r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"autoResize":false,
        "text":"x","originalText":"one two three","fontSize":20,"lineHeight":1.25"#,
    );

    assert_eq!(texts(&editor.geometry()), ["one", "two", "three"]);
}

#[test]
fn words_wrap_after_hyphens_and_break_when_longer_than_the_line() {
    let wrapped = |text: &str, width: u32| {
        let editor = measured(&format!(
            r#""id":"t","type":"text","x":0,"y":0,"width":{width},"height":25,"autoResize":false,
            "text":"x","originalText":"{text}","fontSize":20,"lineHeight":1.25"#
        ));
        texts(&editor.geometry()).join("|")
    };

    assert_eq!(wrapped("well-known state-of", 100), "well-known|state-of");
    assert_eq!(wrapped("well-known", 60), "well-|known");
    assert_eq!(wrapped("abcdefghijkl", 50), "abcde|fghij|kl");
    // Spaces at a break are dropped; at the very end of the text those that
    // still fit stay.
    assert_eq!(wrapped("ab  cd", 40), "ab|cd");
    assert_eq!(wrapped("ab  ", 30), "ab ");
    // Hard breaks stay.
    assert_eq!(wrapped("ab\\n\\ncd", 100), "ab||cd");
}

#[test]
fn cjk_text_wraps_between_any_two_characters() {
    let editor = measured(
        r#""id":"t","type":"text","x":0,"y":0,"width":50,"height":25,"autoResize":false,
        "text":"x","originalText":"日本語のテキスト","fontSize":20,"lineHeight":1.25"#,
    );

    assert_eq!(texts(&editor.geometry()), ["日本語のテ", "キスト"]);
}

#[test]
fn text_wraps_exactly_like_excalidraw_wraps_it() {
    // `text.excalidraw` was made by Excalidraw's own code with the same fixed
    // character widths: the file's lines and positions are what it laid out.
    let unmeasured = editor_with_fixture("text").geometry();
    let mut measuring = editor_with_fixture("text");
    measuring.set_text_measurer(Arc::new(FixedWidth));
    let measured = measuring.geometry();

    let (expected, actual) = (lines(&unmeasured), lines(&measured));
    assert!(expected.len() > 60, "{} lines", expected.len());
    assert_eq!(actual.len(), expected.len());
    for (e, a) in expected.iter().zip(&actual) {
        assert_eq!(a.0, e.0);
        assert!(
            (a.1 - e.1).abs() < 1e-3 && (a.2 - e.2).abs() < 1e-3,
            "{a:?} != {e:?}"
        );
    }
    for (e, a) in unmeasured.text_lines().iter().zip(measured.text_lines()) {
        assert_eq!((e.element, e.align, e.angle), (a.element, a.align, a.angle));
    }
}
