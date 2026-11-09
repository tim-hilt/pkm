//! Box-select and multi-select (ADR-0006), tested at the Editor seam with
//! recorded pointer sequences (see `select.rs` for their format): pointer
//! events in, Selection and overlay out.
//!
//! Expected values (which Elements a box encloses, where its rectangle is)
//! are worked out by hand in the comments, not computed with the formulas of
//! the code under test.

use pkm_drawing::{Editor, PartKind, Tool, Verb};

use crate::common;
use common::{editor_with, replay, tap};

/// A filled rectangle `id` from `(x, y)`, 40 wide and 20 high, with a stroke
/// of 2 (so that it reaches 1 unit beyond that).
fn rectangle(id: &str, x: f64, y: f64) -> String {
    format!(
        r##"{{"id":"{id}","type":"rectangle","x":{x},"y":{y},"width":40,"height":20,
        "strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null}}"##
    )
}

/// Three rectangles in a row, 100 units apart: a at x = 100, b at 200 and c
/// at 300, all at y = 100.
fn row() -> Editor {
    editor_with(&[
        &rectangle("a", 100.0, 100.0),
        &rectangle("b", 200.0, 100.0),
        &rectangle("c", 300.0, 100.0),
    ])
}

/// A drag of `kind` from `from` to `to`, in steps, ending at `to`.
fn drag(editor: &mut Editor, kind: &str, from: (f64, f64), to: (f64, f64)) {
    replay(editor, &drag_events(kind, from, to, true));
}

/// The events of [`drag`]; without the final `up` if `lift` is false.
fn drag_events(kind: &str, from: (f64, f64), to: (f64, f64), lift: bool) -> String {
    let mut events = format!(
        "1.00 down 1 {kind} {} {}\n1.05 move 1 {kind} {} {}\n",
        from.0,
        from.1,
        (from.0 + to.0) / 2.0,
        (from.1 + to.1) / 2.0
    );
    events += &format!("1.10 move 1 {kind} {} {}\n", to.0, to.1);
    if lift {
        events += &format!("1.15 up 1 {kind} {} {}", to.0, to.1);
    }
    events
}

#[test]
fn dragging_on_empty_canvas_selects_the_elements_the_box_encloses() {
    for kind in ["mouse", "finger", "pencil"] {
        let mut editor = row();

        // From (50, 50) to (270, 150): encloses a (99..141 by 99..121) and b
        // (199..241 by 99..121), but only reaches into c, which starts at 299.
        drag(&mut editor, kind, (50.0, 50.0), (270.0, 150.0));

        assert_eq!(editor.selection(), ["a", "b"], "{kind}");
    }
}

#[test]
fn a_box_must_enclose_an_element_whole_touching_is_not_enough() {
    let mut editor = row();

    // Up to x = 120, the box cuts a in two.
    drag(&mut editor, "mouse", (50.0, 50.0), (120.0, 150.0));

    assert!(editor.selection().is_empty());
}

#[test]
fn the_stroke_counts_as_part_of_an_element() {
    // a's box is 100..140 by 100..120 and its stroke of 2 reaches one unit
    // beyond: it is drawn from 99 to 141 by 99 to 121.
    let mut editor = row();
    drag(&mut editor, "mouse", (50.0, 50.0), (140.5, 150.0));
    assert!(editor.selection().is_empty(), "stops inside the stroke");

    drag(&mut editor, "mouse", (50.0, 50.0), (141.0, 150.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_box_can_be_dragged_in_any_direction() {
    for (from, to) in [
        ((270.0, 150.0), (50.0, 50.0)),
        ((50.0, 150.0), (270.0, 50.0)),
        ((270.0, 50.0), (50.0, 150.0)),
    ] {
        let mut editor = row();

        drag(&mut editor, "mouse", from, to);

        assert_eq!(editor.selection(), ["a", "b"], "from {from:?}");
    }
}

#[test]
fn a_turned_element_is_enclosed_as_it_is_shown() {
    // 100 wide and 20 high at the origin, turned a quarter about its middle
    // (50, 10): it stands 20 wide and 100 high, at 40..60 by -40..60.
    let turned = r##"{"id":"t","type":"rectangle","x":0,"y":0,"width":100,"height":20,
        "angle":1.5707963267948966,"strokeWidth":0,"backgroundColor":"#ffc9c9","roundness":null}"##;
    let mut editor = editor_with(&[turned]);

    // Wide enough for it lying down, but not for it standing.
    drag(&mut editor, "mouse", (-10.0, -10.0), (110.0, 30.0));
    assert!(editor.selection().is_empty());

    drag(&mut editor, "mouse", (30.0, -50.0), (70.0, 70.0));
    assert_eq!(editor.selection(), ["t"]);
}

#[test]
fn box_select_works_in_scene_coordinates_when_zoomed() {
    let mut editor = row();
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);
    // At 200 % the middle of a, Scene (120, 110), is at view (250, 220), and
    // a is drawn over view 208..292 by 198..242.
    tap(&mut editor, "mouse", (250.0, 220.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    tap(&mut editor, "mouse", (700.0, 700.0), 1.0);
    assert!(editor.selection().is_empty());

    // Cuts a at its right: 270 points are not past 292.
    drag(&mut editor, "mouse", (160.0, 160.0), (270.0, 280.0));
    assert!(editor.selection().is_empty());

    drag(&mut editor, "mouse", (160.0, 160.0), (320.0, 280.0));
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn locked_deleted_and_label_elements_are_not_boxed() {
    let locked = r##"{"id":"l","type":"rectangle","x":100,"y":100,"width":40,"height":20,
        "locked":true,"backgroundColor":"#ffc9c9"}"##;
    let deleted = r##"{"id":"d","type":"rectangle","x":100,"y":100,"width":40,"height":20,
        "isDeleted":true,"backgroundColor":"#ffc9c9"}"##;
    let label = r##"{"id":"t","type":"text","x":105,"y":105,"width":30,"height":10,
        "text":"hi","originalText":"hi","containerId":"c","fontSize":10,"fontFamily":5}"##;
    let container = r##"{"id":"c","type":"rectangle","x":100,"y":100,"width":40,"height":20,
        "backgroundColor":"#ffc9c9","boundElements":[{"id":"t","type":"text"}]}"##;
    let mut editor = editor_with(&[locked, deleted, container, label]);

    drag(&mut editor, "mouse", (50.0, 50.0), (270.0, 150.0));

    assert_eq!(editor.selection(), ["c"]);
}

#[test]
fn an_arrow_is_boxed_with_its_label() {
    // From (100, 100) to (200, 100), its label from (130, 80) to (170, 120).
    let arrow = r#"{"id":"a","type":"arrow","x":100,"y":100,"width":100,"height":0,
        "strokeWidth":2,"points":[[0,0],[100,0]],"roundness":null,
        "boundElements":[{"id":"t","type":"text"}]}"#;
    let label = r#"{"id":"t","type":"text","x":130,"y":80,"width":40,"height":40,
        "text":"hi","originalText":"hi","containerId":"a","fontSize":30,"fontFamily":5}"#;

    // Around the arrow and its stroke, but through the label.
    let mut editor = editor_with(&[arrow, label]);
    drag(&mut editor, "mouse", (90.0, 90.0), (210.0, 110.0));
    assert!(editor.selection().is_empty());

    // Around both (Excalidraw's `elementsOverlappingBBox`).
    let mut editor = editor_with(&[arrow, label]);
    drag(&mut editor, "mouse", (90.0, 70.0), (210.0, 130.0));
    assert_eq!(editor.selection(), ["a"]);
}

// --- Replacing, keeping and adding ---------------------------------------------

/// An editor with the row's `a` selected.
fn row_with_a_selected() -> Editor {
    let mut editor = row();
    tap(&mut editor, "mouse", (120.0, 110.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn a_box_replaces_the_selection() {
    let mut editor = row_with_a_selected();

    // Encloses b and c only.
    drag(&mut editor, "mouse", (180.0, 50.0), (360.0, 150.0));

    assert_eq!(editor.selection(), ["b", "c"]);
}

#[test]
fn a_box_that_encloses_nothing_leaves_the_selection_alone() {
    // Else a sloppy touch dragging on empty Canvas would throw it away.
    for kind in ["finger", "mouse"] {
        let mut editor = row_with_a_selected();

        drag(&mut editor, kind, (150.0, 300.0), (350.0, 400.0));

        assert_eq!(editor.selection(), ["a"], "{kind}");
    }
}

#[test]
fn shift_adds_the_box_to_the_selection() {
    let mut editor = row_with_a_selected();

    // Encloses b only, with shift held from the start.
    replay(
        &mut editor,
        "1.00 down 1 mouse 180 50 shift
         1.05 move 1 mouse 200 100 shift
         1.10 move 1 mouse 260 150 shift
         1.15 up 1 mouse 260 150 shift",
    );

    assert_eq!(editor.selection(), ["a", "b"]);
}

#[test]
fn the_selection_follows_the_box_while_it_drags() {
    let mut editor = row();

    replay(
        &mut editor,
        &drag_events("mouse", (50.0, 50.0), (150.0, 150.0), false),
    );
    assert_eq!(editor.selection(), ["a"], "grown over a");

    replay(&mut editor, "1.20 move 1 mouse 270 150");
    assert_eq!(editor.selection(), ["a", "b"], "grown over b");

    replay(&mut editor, "1.30 move 1 mouse 150 150");
    assert_eq!(editor.selection(), ["a"], "shrunk back");

    replay(&mut editor, "1.40 up 1 mouse 150 150");
    assert_eq!(editor.selection(), ["a"], "lifting keeps it");
}

#[test]
fn box_selecting_is_no_undo_step() {
    let mut editor = row();

    drag(&mut editor, "mouse", (50.0, 50.0), (270.0, 150.0));

    assert!(!editor.can_undo());
}

// --- Cancelling ----------------------------------------------------------------

#[test]
fn a_cancelled_drag_puts_the_selection_back() {
    let mut editor = row_with_a_selected();

    replay(
        &mut editor,
        &drag_events("finger", (180.0, 50.0), (360.0, 150.0), false),
    );
    assert_eq!(editor.selection(), ["b", "c"]);
    replay(&mut editor, "1.20 cancel 1 finger 360 150");

    assert_eq!(editor.selection(), ["a"]);
    assert!(!shows_box(&editor));
}

#[test]
fn a_second_finger_cancels_the_box_and_navigates_instead() {
    let mut editor = row_with_a_selected();

    replay(
        &mut editor,
        "1.00 down 1 finger 180 50
         1.10 move 1 finger 360 150
         1.15 down 2 finger 400 200
         1.20 move 2 finger 420 200
         1.30 up 2 finger 420 200
         1.35 move 1 finger 300 150
         1.40 up 1 finger 300 150",
    );

    assert_eq!(editor.selection(), ["a"]);
    assert!(!shows_box(&editor));
}

#[test]
fn a_tool_change_ends_the_box() {
    let mut editor = row_with_a_selected();

    replay(
        &mut editor,
        &drag_events("mouse", (180.0, 50.0), (360.0, 150.0), false),
    );
    editor.set_tool(Tool::Hand);

    // Choosing the hand Tool cleared the Selection, and the box selected
    // nothing.
    assert!(editor.selection().is_empty());
}

#[test]
fn only_the_selection_tool_box_selects() {
    // The hand Tool pans, and selects nothing at all.
    let mut editor = row();
    editor.set_tool(Tool::Hand);
    drag(&mut editor, "mouse", (50.0, 50.0), (270.0, 150.0));
    assert!(editor.selection().is_empty());

    // The rectangle Tool draws, and selects what it drew, not what the drag
    // enclosed.
    let mut editor = row();
    editor.set_tool(Tool::Rectangle);
    drag(&mut editor, "mouse", (50.0, 50.0), (270.0, 150.0));
    let drawn = editor.selection().to_vec();
    assert_eq!(drawn.len(), 1);
    assert!(!["a", "b"].contains(&drawn[0].as_str()));
}

// --- The rectangle over the Canvas -----------------------------------------------

/// A closed chain of corners, as `(x, y)`.
type Corners = Vec<(f32, f32)>;

/// Whether the overlay shows a box-select rectangle (the handles of a
/// Selection name no Element either, but are white).
fn shows_box(editor: &Editor) -> bool {
    editor
        .overlay()
        .parts()
        .iter()
        .any(|p| p.color == 0x0000_c8ff)
}

/// The fill and the stroke of the box-select rectangle: the parts of the
/// overlay that are not of an Element; the box hides the transform handles.
fn box_parts(editor: &Editor) -> (Corners, Corners, f32) {
    let overlay = editor.overlay();
    let parts: Vec<_> = overlay
        .parts()
        .iter()
        .filter(|p| p.element == u32::MAX)
        .collect();
    assert_eq!(parts.len(), 2, "a fill and a stroke");
    let corners = |part: &pkm_drawing::Part| {
        let ops = common::ops(&overlay, part);
        let verbs: Vec<_> = ops.iter().map(|(verb, _)| *verb).collect();
        assert_eq!(
            verbs,
            [Verb::Move, Verb::Line, Verb::Line, Verb::Line, Verb::Line]
        );
        ops.iter().map(|(_, c)| (c[0], c[1])).collect::<Vec<_>>()
    };
    let (fill, stroke) = (parts[0], parts[1]);
    assert_eq!(fill.kind, PartKind::Fill as u32);
    // Excalidraw's rgba(0, 0, 200, 0.04).
    assert_eq!(fill.color, 0x0000_c8ff);
    assert_eq!(fill.opacity, 0.04);
    assert_eq!(stroke.kind, PartKind::Stroke as u32);
    assert_eq!(stroke.color, 0x6965_dbff, "the selection colour");
    (corners(fill), corners(stroke), stroke.width)
}

#[test]
fn the_box_is_drawn_while_dragging_and_gone_after() {
    let mut editor = row();

    replay(
        &mut editor,
        &drag_events("mouse", (50.0, 60.0), (150.0, 160.0), false),
    );
    let (fill, stroke, width) = box_parts(&editor);
    let expected = [
        (50.0, 60.0),
        (150.0, 60.0),
        (150.0, 160.0),
        (50.0, 160.0),
        (50.0, 60.0),
    ];
    assert_eq!(fill, expected);
    assert_eq!(stroke, expected);
    assert_eq!(width, 1.0);

    replay(&mut editor, "1.20 up 1 mouse 150 160");
    assert!(!shows_box(&editor));
}

#[test]
fn the_box_is_drawn_the_same_whichever_way_it_is_dragged() {
    let mut editor = row();

    replay(
        &mut editor,
        &drag_events("mouse", (150.0, 160.0), (50.0, 60.0), false),
    );

    let (fill, ..) = box_parts(&editor);
    assert_eq!(
        fill,
        [
            (50.0, 60.0),
            (150.0, 60.0),
            (150.0, 160.0),
            (50.0, 160.0),
            (50.0, 60.0)
        ]
    );
}

#[test]
fn the_box_stays_one_point_wide_at_any_zoom() {
    let mut editor = row();
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);

    replay(
        &mut editor,
        &drag_events("mouse", (50.0, 60.0), (150.0, 160.0), false),
    );

    let (.., width) = box_parts(&editor);
    assert_eq!(width, 0.5, "one view point is half a Scene unit at 200 %");
}

#[test]
fn the_overlay_revision_changes_as_the_box_does() {
    let mut editor = row();
    replay(
        &mut editor,
        "1.00 down 1 mouse 50 50\n1.05 move 1 mouse 60 60",
    );
    let mut seen = editor.overlay_revision();

    for event in ["1.10 move 1 mouse 70 70", "1.15 up 1 mouse 70 70"] {
        replay(&mut editor, event);
        assert_ne!(editor.overlay_revision(), seen, "{event}");
        seen = editor.overlay_revision();
    }
}

#[test]
fn the_selected_elements_are_framed_as_the_box_selects_them() {
    let mut editor = row();

    replay(
        &mut editor,
        &drag_events("mouse", (50.0, 50.0), (270.0, 150.0), false),
    );

    // A frame around each of a and b, and the box's fill and stroke.
    assert_eq!(editor.overlay().parts().len(), 4);
}

// --- Shift-click -----------------------------------------------------------------

fn shift_click(editor: &mut Editor, (x, y): (f64, f64), t: f64) {
    replay(
        editor,
        &format!(
            "{t} down 1 mouse {x} {y} shift\n{} up 1 mouse {x} {y} shift",
            t + 0.05
        ),
    );
}

#[test]
fn shift_click_adds_an_element_to_the_selection() {
    let mut editor = row_with_a_selected();

    shift_click(&mut editor, (320.0, 110.0), 1.0);
    assert_eq!(editor.selection(), ["a", "c"]);

    shift_click(&mut editor, (220.0, 110.0), 2.0);
    assert_eq!(editor.selection(), ["a", "c", "b"]);
}

#[test]
fn shift_click_removes_a_selected_element() {
    let mut editor = row_with_a_selected();
    shift_click(&mut editor, (220.0, 110.0), 1.0);
    assert_eq!(editor.selection(), ["a", "b"]);

    shift_click(&mut editor, (120.0, 110.0), 2.0);

    assert_eq!(editor.selection(), ["b"]);
}

#[test]
fn shift_click_on_empty_canvas_keeps_the_selection() {
    let mut editor = row_with_a_selected();

    shift_click(&mut editor, (500.0, 500.0), 1.0);

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_plain_click_on_a_member_of_the_selection_selects_only_it() {
    let mut editor = row_with_a_selected();
    shift_click(&mut editor, (220.0, 110.0), 1.0);

    tap(&mut editor, "mouse", (220.0, 110.0), 2.0);

    assert_eq!(editor.selection(), ["b"]);
}

#[test]
fn shift_click_changes_the_overlay_revision() {
    let mut editor = row_with_a_selected();
    let before = editor.overlay_revision();

    shift_click(&mut editor, (320.0, 110.0), 1.0);

    assert_ne!(editor.overlay_revision(), before);
}

// --- Moving what was selected ----------------------------------------------------

/// The x of the Element `id` in the saved Drawing.
fn x_of(editor: &Editor, id: &str) -> f64 {
    let saved = String::from_utf8(editor.save()).unwrap();
    let drawing = pkm_base::json::parse(&saved).unwrap();
    drawing
        .get("elements")
        .and_then(|e| e.as_array())
        .unwrap()
        .iter()
        .find(|e| e.get("id").and_then(|i| i.as_str()) == Some(id))
        .and_then(|e| e.get("x"))
        .and_then(|x| x.as_f64())
        .unwrap()
}

#[test]
fn dragging_a_member_of_a_box_selection_moves_all_of_it() {
    let mut editor = row();
    drag(&mut editor, "mouse", (50.0, 50.0), (270.0, 150.0));
    assert_eq!(editor.selection(), ["a", "b"]);

    drag(&mut editor, "mouse", (220.0, 110.0), (240.0, 110.0));

    assert_eq!(editor.selection(), ["a", "b"]);
    assert_eq!((x_of(&editor, "a"), x_of(&editor, "b")), (120.0, 220.0));
    assert_eq!(x_of(&editor, "c"), 300.0);
}

#[test]
fn shift_dragging_an_element_adds_it_to_the_selection_and_moves_all() {
    let mut editor = row_with_a_selected();

    replay(
        &mut editor,
        "1.00 down 1 mouse 320 110 shift
         1.05 move 1 mouse 330 110 shift
         1.10 move 1 mouse 340 110 shift
         1.15 up 1 mouse 340 110 shift",
    );

    assert_eq!(editor.selection(), ["a", "c"]);
    assert_eq!((x_of(&editor, "a"), x_of(&editor, "c")), (120.0, 320.0));
}
