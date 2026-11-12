//! Aligning and distributing the Selection (ADR-0006), tested at the Editor
//! seam: Select All in, commands, the saved Drawing out.
//!
//! Expected positions follow Excalidraw's `align.ts`, `distribute.ts` and
//! `getSelectedElementsByGroup` (`groups.ts`) at the pinned commit.

use pkm_base::json::Value;
use pkm_drawing::{Alignment, Distribution, Editor};

use crate::common;
use common::{editor_with, num, saved_elements};

/// A rectangle `id` at `(x, y)`, `w` wide and `h` high, in `groups` (JSON
/// array contents, innermost first).
fn rect(id: &str, (x, y): (f64, f64), (w, h): (f64, f64), groups: &str) -> String {
    format!(
        r#"{{"id":"{id}","type":"rectangle","x":{x},"y":{y},"width":{w},"height":{h},
        "groupIds":[{groups}],"seed":1,"version":3,"versionNonce":7}}"#
    )
}

/// `a` (0, 0; 40 × 20), `b` (100, 50; 60 × 30) and `c` (200, 10; 20 × 40),
/// all selected: the Selection spans 0 to 220 across and 0 to 80 down.
fn three(groups: [&str; 3]) -> Editor {
    let [a, b, c] = groups;
    let mut editor = editor_with(&[
        &rect("a", (0.0, 0.0), (40.0, 20.0), a),
        &rect("b", (100.0, 50.0), (60.0, 30.0), b),
        &rect("c", (200.0, 10.0), (20.0, 40.0), c),
    ]);
    editor.select_all();
    editor
}

fn position(editor: &Editor, id: &str) -> (f64, f64) {
    let element = saved_elements(editor)
        .into_iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no element {id}"));
    (num(&element, "x"), num(&element, "y"))
}

fn positions(editor: &Editor) -> [(f64, f64); 3] {
    ["a", "b", "c"].map(|id| position(editor, id))
}

#[test]
fn aligning_lines_up_the_edges_or_middles_of_the_selection() {
    let cases = [
        (Alignment::Left, [(0.0, 0.0), (0.0, 50.0), (0.0, 10.0)]),
        (
            Alignment::CenterHorizontally,
            [(90.0, 0.0), (80.0, 50.0), (100.0, 10.0)],
        ),
        (
            Alignment::Right,
            [(180.0, 0.0), (160.0, 50.0), (200.0, 10.0)],
        ),
        (Alignment::Top, [(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)]),
        (
            Alignment::CenterVertically,
            [(0.0, 30.0), (100.0, 25.0), (200.0, 20.0)],
        ),
        (
            Alignment::Bottom,
            [(0.0, 60.0), (100.0, 50.0), (200.0, 40.0)],
        ),
    ];
    for (how, expected) in cases {
        let mut editor = three(["", "", ""]);
        editor.align(how);
        assert_eq!(positions(&editor), expected, "{how:?}");
    }
}

#[test]
fn distributing_makes_the_gaps_between_them_equal() {
    // 220 wide, 120 of it shapes: two gaps of 50.
    let mut editor = three(["", "", ""]);
    editor.distribute(Distribution::Horizontally);
    assert_eq!(
        positions(&editor),
        [(0.0, 0.0), (90.0, 50.0), (200.0, 10.0)]
    );

    // 80 high, 90 of it shapes: they overlap, so the middles are spaced
    // instead, between those of the topmost and the bottommost.
    let mut editor = three(["", "", ""]);
    editor.distribute(Distribution::Vertically);
    // a's middle is at 10, b's at 65; c's goes halfway between, to 37.5.
    assert_eq!(
        positions(&editor),
        [(0.0, 0.0), (100.0, 50.0), (200.0, 17.5)]
    );
}

#[test]
fn a_selected_group_moves_as_a_whole() {
    let mut editor = three([r#""g""#, r#""g""#, ""]);
    editor.align(Alignment::Right);
    // The group (0 to 160) moves 60 to the right; c is there already.
    assert_eq!(
        positions(&editor),
        [(60.0, 0.0), (160.0, 50.0), (200.0, 10.0)]
    );
}

#[test]
fn with_one_group_selected_its_parts_are_aligned() {
    let mut editor = three([r#""inner","outer""#, r#""inner","outer""#, r#""outer""#]);
    editor.align(Alignment::Top);
    // The inner group (top at 0) stays; c, on its own in the outer one, moves.
    assert_eq!(
        positions(&editor),
        [(0.0, 0.0), (100.0, 50.0), (200.0, 0.0)]
    );
}

#[test]
fn a_label_moves_with_its_container() {
    let label = r#"{"id":"t","type":"text","x":110,"y":55,"width":40,"height":20,
        "text":"Hi","originalText":"Hi","fontSize":20,"containerId":"b","seed":2}"#;
    let b = rect("b", (100.0, 50.0), (60.0, 30.0), "").replace(
        r#""groupIds""#,
        r#""boundElements":[{"type":"text","id":"t"}],"groupIds""#,
    );
    let mut editor = editor_with(&[&rect("a", (0.0, 0.0), (40.0, 20.0), ""), &b, label]);
    editor.select_all();
    let before = position(&editor, "t");
    editor.align(Alignment::Top);
    assert_eq!(position(&editor, "b"), (100.0, 0.0));
    assert_eq!(position(&editor, "t"), (before.0, before.1 - 50.0));
}

#[test]
fn aligning_is_one_undo_step() {
    let mut editor = three(["", "", ""]);
    editor.align(Alignment::Left);
    editor.undo();
    assert_eq!(
        positions(&editor),
        [(0.0, 0.0), (100.0, 50.0), (200.0, 10.0)]
    );
    assert!(!editor.can_undo());
}

#[test]
fn what_is_aligned_already_records_no_step() {
    let mut editor = three(["", "", ""]);
    editor.align(Alignment::Left);
    editor.align(Alignment::Left);
    editor.undo();
    assert!(!editor.can_undo());
}

#[test]
fn aligning_takes_two_units_and_distributing_three() {
    let editor = three([r#""g""#, r#""g""#, ""]);
    assert!(editor.can_align());
    assert!(!editor.can_distribute());
    let sections = editor.properties().sections;
    assert!(sections.align && !sections.distribute);

    let editor = three(["", "", ""]);
    assert!(editor.can_distribute());
    assert!(editor.properties().sections.distribute);

    let mut editor = three([r#""g""#, r#""g""#, r#""g""#]);
    editor.distribute(Distribution::Horizontally);
    assert!(
        editor.can_distribute(),
        "one group: its Elements are the units"
    );
}

#[test]
fn a_single_element_cannot_be_aligned() {
    let mut editor = editor_with(&[&rect("a", (0.0, 0.0), (40.0, 20.0), "")]);
    editor.select_all();
    assert!(!editor.can_align());
    assert!(!editor.properties().sections.align);
    editor.align(Alignment::Right);
    assert!(!editor.can_undo());
}
