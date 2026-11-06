//! Selecting and deselecting Elements (ADR-0006), tested at the Editor seam
//! with recorded pointer sequences: pointer events in, Selection and overlay
//! out. Pointer sequences are recorded event lines:
//!
//! ```text
//! <time in s> <down|move|up|cancel> <pointer id> <finger|pencil|mouse> <x> <y>
//! ```
//!
//! Expected values (distances, corners) are worked out by hand in the
//! comments, not computed with the formulas of the code under test.

use workingtitle_core::{Editor, Modifiers, PartKind, Tool, Verb};

use crate::common;
use common::{editor_with, replay, tap};

/// A rectangle from (100, 100) to (200, 150) that is `filled` or hollow.
fn rectangle(id: &str, filled: bool) -> String {
    let background = if filled { "#ffc9c9" } else { "transparent" };
    format!(
        r#"{{"id":"{id}","type":"rectangle","x":100,"y":100,"width":100,"height":50,
        "strokeWidth":2,"backgroundColor":"{background}","roundness":null}}"#
    )
}

#[test]
fn tapping_a_filled_rectangle_selects_it() {
    let mut editor = editor_with(&[&rectangle("a", true)]);

    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    assert_eq!(editor.selection(), ["a"]);
}

// --- Stroke versus interior ----------------------------------------------------

#[test]
fn a_hollow_rectangle_is_selected_by_its_stroke_not_its_interior() {
    let mut editor = editor_with(&[&rectangle("a", false)]);

    // The middle of the rectangle is 25 from the nearest edge.
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert!(editor.selection().is_empty());

    // On the left edge.
    tap(&mut editor, "mouse", (100.0, 125.0), 1.0);
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_mouse_hits_within_about_seven_units_of_a_stroke() {
    // Excalidraw's threshold: 0.85 * 8 / zoom = 6.8 at 100 %. The left edge is
    // at x = 100.
    for (x, hit) in [(93.5, true), (92.5, false)] {
        let mut editor = editor_with(&[&rectangle("a", false)]);

        tap(&mut editor, "mouse", (x, 125.0), 0.0);

        assert_eq!(!editor.selection().is_empty(), hit, "tap at x = {x}");
    }
}

#[test]
fn rounded_corners_are_followed() {
    let rounded = |roundness: &str| {
        format!(
            r#"{{"id":"a","type":"rectangle","x":0,"y":0,"width":200,"height":200,
            "strokeWidth":2,"backgroundColor":"transparent","roundness":{roundness}}}"#
        )
    };
    // With the adaptive radius of 32 the outline cuts the corner: its closest
    // point to (0, 0) is 32 * (sqrt(2) - 1) = 13.3 away, out of reach.
    let mut editor = editor_with(&[&rounded(r#"{"type":3}"#)]);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    assert!(editor.selection().is_empty());

    let mut editor = editor_with(&[&rounded("null")]);
    tap(&mut editor, "mouse", (0.0, 0.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
}

// --- Finger tolerance ----------------------------------------------------------

#[test]
fn a_finger_reaches_further_than_a_mouse_or_pencil() {
    // 15 units left of the stroke at x = 100: past the 6.8 of a mouse, within
    // the 22 of a finger.
    for (kind, selected) in [("finger", true), ("mouse", false), ("pencil", false)] {
        let mut editor = editor_with(&[&rectangle("a", false)]);

        tap(&mut editor, kind, (85.0, 125.0), 0.0);

        assert_eq!(!editor.selection().is_empty(), selected, "{kind}");
    }
}

#[test]
fn a_fingers_reach_is_the_same_on_screen_at_any_zoom() {
    let mut editor = editor_with(&[&rectangle("a", false)]);
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);

    // 15 Scene units from the stroke are 30 points now: out of the 22 reach.
    tap(&mut editor, "finger", (170.0, 250.0), 0.0);
    assert!(editor.selection().is_empty());
    // 20 points are 10 Scene units from it.
    tap(&mut editor, "finger", (180.0, 250.0), 1.0);
    assert_eq!(editor.selection(), ["a"]);
}

// --- Deselecting ---------------------------------------------------------------

/// An editor with a filled rectangle selected.
fn editor_with_selection() -> Editor {
    let mut editor = editor_with(&[&rectangle("a", true)]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["a"]);
    editor
}

#[test]
fn a_short_stationary_tap_on_empty_canvas_clears_the_selection() {
    for kind in ["finger", "pencil", "mouse"] {
        let mut editor = editor_with_selection();

        tap(&mut editor, kind, (500.0, 500.0), 1.0);

        assert!(editor.selection().is_empty(), "{kind}");
    }
}

#[test]
fn a_finger_that_stays_down_too_long_does_not_clear_the_selection() {
    let mut editor = editor_with_selection();

    // 500 ms: a press, not a tap.
    replay(
        &mut editor,
        "1.0 down 1 finger 500 500
         1.5 up 1 finger 500 500",
    );

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_finger_that_moves_does_not_clear_the_selection() {
    let mut editor = editor_with_selection();

    // 20 points away from where it landed.
    replay(
        &mut editor,
        "1.00 down 1 finger 500 500
         1.05 move 1 finger 520 500
         1.10 up 1 finger 520 500",
    );
    assert_eq!(editor.selection(), ["a"]);

    // Moving away and back is still moving.
    replay(
        &mut editor,
        "2.00 down 1 finger 500 500
         2.05 move 1 finger 520 500
         2.10 move 1 finger 500 500
         2.15 up 1 finger 500 500",
    );
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_finger_may_wobble_a_little_during_a_tap() {
    let mut editor = editor_with_selection();

    replay(
        &mut editor,
        "1.00 down 1 finger 500 500
         1.05 move 1 finger 505 503
         1.10 up 1 finger 505 503",
    );

    assert!(editor.selection().is_empty());
}

#[test]
fn a_mouse_click_clears_however_long_it_is_held_but_a_drag_does_not() {
    let mut editor = editor_with_selection();
    replay(
        &mut editor,
        "1 down 0 mouse 500 500
         3 up 0 mouse 500 500",
    );
    assert!(editor.selection().is_empty());

    let mut editor = editor_with_selection();
    replay(
        &mut editor,
        "1.0 down 0 mouse 500 500
         1.1 move 0 mouse 520 500
         1.2 up 0 mouse 520 500",
    );
    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_cancelled_touch_never_clears_the_selection() {
    let mut editor = editor_with_selection();

    replay(
        &mut editor,
        "1.00 down 1 finger 500 500
         1.05 cancel 1 finger 500 500",
    );

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn two_fingers_never_change_the_selection() {
    let mut editor = editor_with_selection();

    // A second finger lands, then both lift quickly and without moving.
    replay(
        &mut editor,
        "1.00 down 1 finger 500 500
         1.02 down 2 finger 560 500
         1.05 up 2 finger 560 500
         1.06 up 1 finger 500 500",
    );
    assert_eq!(editor.selection(), ["a"]);

    // Nor does the first finger select the Element it landed on.
    let mut editor = editor_with(&[&rectangle("a", true)]);
    replay(
        &mut editor,
        "1.00 down 1 finger 150 125
         1.02 down 2 finger 560 500
         1.05 up 2 finger 560 500
         1.06 up 1 finger 150 125",
    );
    assert!(editor.selection().is_empty());
}

#[test]
fn a_finger_left_after_a_pinch_does_not_tap_when_it_lifts() {
    let mut editor = editor_with_selection();

    replay(
        &mut editor,
        "1.0 down 1 finger 500 500
         1.1 down 2 finger 560 500
         1.2 move 2 finger 600 500
         1.3 up 1 finger 500 500
         1.4 up 2 finger 600 500",
    );

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn a_press_on_an_element_selects_it_however_long_it_lasts() {
    let mut editor = editor_with(&[&rectangle("a", true)]);

    replay(
        &mut editor,
        "1.0 down 1 finger 150 125
         2.0 up 1 finger 150 125",
    );

    assert_eq!(editor.selection(), ["a"]);
}

#[test]
fn only_the_selection_tool_selects() {
    for tool in [Tool::Hand, Tool::Rectangle] {
        // Choosing the Tool already cleared the Selection.
        let mut editor = editor_with_selection();
        editor.set_tool(tool);
        assert!(editor.selection().is_empty(), "{tool:?} clears it");

        let mut editor = editor_with(&[&rectangle("a", true)]);
        editor.set_tool(tool);
        tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
        assert!(editor.selection().is_empty(), "{tool:?} selects nothing");
    }
}

#[test]
fn a_tool_change_during_a_press_makes_it_no_tap() {
    let mut editor = editor_with_selection();

    replay(&mut editor, "1.0 down 1 finger 500 500");
    editor.set_tool(Tool::Hand);
    editor.set_tool(Tool::Selection);
    replay(&mut editor, "1.1 up 1 finger 500 500");

    // The tap on empty Canvas would have cleared it; the Tool change did,
    // and the lift selected nothing.
    assert!(editor.selection().is_empty());
}

#[test]
fn tapping_another_element_selects_only_that_one() {
    let other = r##"{"id":"b","type":"rectangle","x":300,"y":100,"width":50,"height":50,
        "backgroundColor":"#a5d8ff"}"##;
    let mut editor = editor_with(&[&rectangle("a", true), other]);

    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    tap(&mut editor, "mouse", (325.0, 125.0), 1.0);

    assert_eq!(editor.selection(), ["b"]);
}

#[test]
fn loading_a_drawing_clears_the_selection() {
    let mut editor = editor_with_selection();

    editor.load(&Editor::new().save()).unwrap();

    assert!(editor.selection().is_empty());
}

// --- Other shapes ----------------------------------------------------------------

/// An Element of `kind` with `members` (JSON members after the id and type).
fn element(id: &str, kind: &str, members: &str) -> String {
    format!(r#"{{"id":"{id}","type":"{kind}","strokeWidth":2,{members}}}"#)
}

const FILLED: &str = r##""backgroundColor":"#ffc9c9""##;
const HOLLOW: &str = r#""backgroundColor":"transparent""#;

/// Whether a mouse tap at each of `taps` selects the only Element of the
/// Scene, as `(point, expected)`.
#[track_caller]
fn assert_hits(element: &str, taps: &[((f64, f64), bool)]) {
    for &(point, expected) in taps {
        let mut editor = editor_with(&[element]);
        tap(&mut editor, "mouse", point, 0.0);
        assert_eq!(
            !editor.selection().is_empty(),
            expected,
            "tap at {point:?} on {element}"
        );
    }
}

#[test]
fn an_ellipse_is_hit_along_its_curve_not_its_corners() {
    // Centre (100, 50), radii 100 and 50.
    let shape = |fill| {
        element(
            "a",
            "ellipse",
            &format!(r#""x":0,"y":0,"width":200,"height":100,{fill}"#),
        )
    };

    // Filled: anywhere inside, but not in the corners of its box, which are
    // about 22 from the curve.
    assert_hits(
        &shape(FILLED),
        &[
            ((100.0, 50.0), true),
            ((5.0, 5.0), false),
            ((3.0, 50.0), true),
        ],
    );
    // Hollow: on the curve only (the leftmost and topmost points).
    assert_hits(
        &shape(HOLLOW),
        &[
            ((100.0, 50.0), false),
            ((0.0, 50.0), true),
            ((100.0, 0.0), true),
        ],
    );
}

#[test]
fn a_diamond_is_hit_along_its_edges_not_its_corners() {
    // Corners at the middle of each side of the 100 x 100 box.
    let shape = |fill| {
        element(
            "a",
            "diamond",
            &format!(r#""x":0,"y":0,"width":100,"height":100,{fill}"#),
        )
    };

    // (10, 10) is 20 from the edge x + y = 50.
    assert_hits(
        &shape(FILLED),
        &[
            ((50.0, 50.0), true),
            ((10.0, 10.0), false),
            ((30.0, 30.0), true),
        ],
    );
    // The middle of the upper left edge, and the middle of the diamond.
    assert_hits(
        &shape(HOLLOW),
        &[((25.0, 25.0), true), ((50.0, 50.0), false)],
    );
}

#[test]
fn a_line_is_hit_along_its_length() {
    // From (50, 50) to (150, 50).
    let line = element(
        "a",
        "line",
        r#""x":50,"y":50,"width":100,"height":0,"points":[[0,0],[100,0]],"roundness":null"#,
    );

    assert_hits(
        &line,
        &[
            ((100.0, 50.0), true),
            ((100.0, 56.0), true),
            ((100.0, 58.0), false),
            // Past the end: 5 and 10 from it.
            ((155.0, 50.0), true),
            ((160.0, 50.0), false),
        ],
    );
}

#[test]
fn a_curved_line_runs_through_its_points() {
    let line = element(
        "a",
        "line",
        r#""x":0,"y":0,"width":200,"height":100,"points":[[0,0],[100,100],[200,0]],
        "roundness":{"type":2}"#,
    );

    assert_hits(&line, &[((100.0, 100.0), true), ((100.0, 30.0), false)]);
}

#[test]
fn only_a_closed_line_with_a_background_is_hit_inside() {
    let square = |last: &str, fill| {
        element(
            "a",
            "line",
            &format!(
                r#""x":0,"y":0,"width":100,"height":100,{fill},
                "points":[[0,0],[100,0],[100,100],[0,100],{last}]"#
            ),
        )
    };

    assert_hits(&square("[0,0]", FILLED), &[((50.0, 50.0), true)]);
    assert_hits(&square("[0,0]", HOLLOW), &[((50.0, 50.0), false)]);
    // Not coming back to the start: nothing is enclosed.
    assert_hits(&square("[0,50]", FILLED), &[((50.0, 50.0), false)]);
}

#[test]
fn an_arrow_is_never_hit_inside() {
    let arrow = element(
        "a",
        "arrow",
        &format!(
            r#""x":0,"y":0,"width":100,"height":100,{FILLED},
            "points":[[0,0],[100,0],[100,100],[0,100],[0,0]],"roundness":null"#
        ),
    );

    assert_hits(&arrow, &[((50.0, 50.0), false), ((50.0, 0.0), true)]);
}

#[test]
fn a_freedraw_stroke_is_hit_on_its_ink() {
    let stroke = |points: &str, fill| {
        element(
            "a",
            "freedraw",
            &format!(
                r#""x":0,"y":0,"width":100,"height":100,{fill},"points":{points},
                "pressures":[],"simulatePressure":true"#
            ),
        )
    };
    let open = "[[0,0],[50,0],[100,0]]";
    assert_hits(
        &stroke(open, HOLLOW),
        &[((50.0, 0.0), true), ((50.0, 20.0), false)],
    );

    // Scribbled over itself into a loop, with a background: also inside.
    let square = "[[0,0],[100,0],[100,100],[0,100],[0,0]]";
    assert_hits(&stroke(square, FILLED), &[((50.0, 50.0), true)]);
    assert_hits(&stroke(square, HOLLOW), &[((50.0, 50.0), false)]);
}

#[test]
fn a_stroke_of_constant_width_is_hit_on_its_own_ink() {
    // Ink 2.8 (1.4 times the stroke width) each side of the line.
    let stroke = element(
        "a",
        "freedraw",
        r#""x":0,"y":0,"width":100,"height":0,"backgroundColor":"transparent",
        "points":[[0,0],[50,0],[100,0]],"pressures":[],"simulatePressure":false,
        "strokeOptions":{"variability":"constant","streamline":0.5}"#,
    );

    assert_hits(&stroke, &[((50.0, 2.0), true), ((50.0, 20.0), false)]);
}

#[test]
fn text_and_images_are_hit_anywhere_in_their_box() {
    let text = element(
        "a",
        "text",
        r#""x":0,"y":0,"width":100,"height":25,"text":"hello","originalText":"hello",
        "fontSize":20,"backgroundColor":"transparent""#,
    );
    assert_hits(&text, &[((50.0, 12.0), true), ((50.0, 40.0), false)]);

    let image = element(
        "a",
        "image",
        r#""x":0,"y":0,"width":100,"height":80,"fileId":"f","backgroundColor":"transparent""#,
    );
    assert_hits(&image, &[((50.0, 40.0), true), ((50.0, 100.0), false)]);
}

#[test]
fn a_label_belongs_to_its_container() {
    let container = element(
        "box",
        "rectangle",
        &format!(
            r#""x":0,"y":0,"width":200,"height":100,{HOLLOW},"roundness":null,
            "boundElements":[{{"type":"text","id":"label"}}]"#
        ),
    );
    let label = element(
        "label",
        "text",
        r#""x":50,"y":40,"width":100,"height":25,"text":"hi","originalText":"hi","fontSize":20,
        "containerId":"box""#,
    );
    let mut editor = editor_with(&[&container, &label]);

    // On the label, and in the container around it: the container.
    tap(&mut editor, "mouse", (100.0, 50.0), 0.0);
    assert_eq!(editor.selection(), ["box"]);
    editor.set_tool(Tool::Selection);
    tap(&mut editor, "mouse", (500.0, 500.0), 1.0);
    tap(&mut editor, "mouse", (20.0, 20.0), 2.0);
    assert_eq!(editor.selection(), ["box"]);
}

#[test]
fn tapping_an_arrows_label_selects_the_arrow() {
    // A straight arrow from (0, 0) to (200, 0), its label (60 by 50) sitting
    // on its middle: from (70, -25) to (130, 25).
    let arrow = element(
        "arrow",
        "arrow",
        r#""x":0,"y":0,"width":200,"height":0,"points":[[0,0],[200,0]],"roundness":null,
        "boundElements":[{"type":"text","id":"label"}]"#,
    );
    let label = element(
        "label",
        "text",
        r#""x":70,"y":-25,"width":60,"height":50,"text":"hi","originalText":"hi","fontSize":40,
        "containerId":"arrow""#,
    );

    // 20 units from the arrow, out of a mouse's reach of it, but on the
    // label (Excalidraw's `hitElementBoundText`).
    for (point, hit) in [((80.0, 20.0), true), ((60.0, 20.0), false)] {
        let mut editor = editor_with(&[&arrow, &label]);
        tap(&mut editor, "mouse", point, 0.0);
        let expected: &[&str] = if hit { &["arrow"] } else { &[] };
        assert_eq!(editor.selection(), expected, "tap at {point:?}");
    }
}

#[test]
fn locked_and_deleted_elements_are_not_hit() {
    for members in [r#""locked":true"#, r#""isDeleted":true"#] {
        assert_hits(
            &element(
                "a",
                "rectangle",
                &format!(r#""x":0,"y":0,"width":100,"height":100,{FILLED},{members}"#),
            ),
            &[((50.0, 50.0), false)],
        );
    }
}

// --- Turned and stacked Elements ------------------------------------------------

#[test]
fn a_turned_element_is_hit_where_it_is() {
    // 100 x 20 around (150, 110), a quarter turn clockwise: it covers
    // x 140..160, y 60..160, so the former top edge is the line x = 160.
    let turned = element(
        "a",
        "rectangle",
        &format!(
            r#""x":100,"y":100,"width":100,"height":20,"angle":{},{HOLLOW},"roundness":null"#,
            std::f64::consts::FRAC_PI_2
        ),
    );

    assert_hits(
        &turned,
        &[
            ((160.0, 70.0), true),
            ((150.0, 62.0), true),
            // Where the unturned top edge was.
            ((120.0, 100.0), false),
            // In the middle of the turned one, 10 from both long edges.
            ((150.0, 100.0), false),
        ],
    );
}

#[test]
fn the_topmost_element_wins_unless_it_is_only_barely_hit() {
    let behind = element(
        "behind",
        "rectangle",
        &format!(r#""x":0,"y":0,"width":300,"height":300,{FILLED},"roundness":null"#),
    );
    // A hollow rectangle on it, with its left edge at x = 100.
    let front = element(
        "front",
        "rectangle",
        &format!(r#""x":100,"y":100,"width":100,"height":100,{HOLLOW},"roundness":null"#),
    );
    for (x, selected) in [(101.0, "front"), (95.0, "behind"), (150.0, "behind")] {
        let mut editor = editor_with(&[&behind, &front]);

        tap(&mut editor, "mouse", (x, 150.0), 0.0);

        // At 95, `front` is within the 6.8 reach, but when something else is
        // hit too the reach halves to 3.4.
        assert_eq!(editor.selection(), [selected], "tap at x = {x}");
    }
}

#[test]
fn the_inside_of_a_selected_hollow_element_keeps_it_selected() {
    let mut editor = editor_with(&[&rectangle("a", false)]);
    tap(&mut editor, "mouse", (100.0, 125.0), 0.0);

    // Its stroke is 25 away, but it is selected: its box is hit.
    tap(&mut editor, "mouse", (150.0, 125.0), 1.0);

    assert_eq!(editor.selection(), ["a"]);
    // Outside the box (and out of reach) deselects as always.
    tap(&mut editor, "mouse", (500.0, 125.0), 2.0);
    assert!(editor.selection().is_empty());
}

// --- Selection frame overlay -------------------------------------------------------

/// Excalidraw's selection colour, opaque.
const SELECTION_COLOR: u32 = 0x6965_dbff;

/// The corners of the one frame the overlay draws: a closed chain of four
/// lines, as `(x, y)`, and the width of its stroke. The transform handles
/// (see `resize_rotate.rs`) are parts of their own, named for no Element.
#[track_caller]
fn frame_of(editor: &Editor) -> (Vec<(f32, f32)>, f32) {
    let overlay = editor.overlay();
    let frames: Vec<_> = overlay
        .parts()
        .iter()
        .filter(|p| p.element != u32::MAX)
        .collect();
    assert_eq!(frames.len(), 1, "one frame");
    let part = *frames[0];
    assert_eq!(part.kind, PartKind::Stroke as u32);
    assert_eq!(part.color, SELECTION_COLOR);
    assert_eq!(part.opacity, 1.0);
    assert_eq!((part.dash_on, part.dash_off), (0.0, 0.0), "solid");
    let ops = common::ops(&overlay, &part);
    let verbs: Vec<_> = ops.iter().map(|(verb, _)| *verb).collect();
    assert_eq!(
        verbs,
        [Verb::Move, Verb::Line, Verb::Line, Verb::Line, Verb::Line]
    );
    let corners: Vec<_> = ops.iter().map(|(_, c)| (c[0], c[1])).collect();
    assert_eq!(corners[0], corners[4], "closed");
    (corners[..4].to_vec(), part.width)
}

#[track_caller]
fn assert_corners(actual: &[(f32, f32)], expected: [(f32, f32); 4]) {
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a.0 - e.0).abs() < 1e-3 && (a.1 - e.1).abs() < 1e-3,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn nothing_is_drawn_over_the_canvas_without_a_selection() {
    let editor = editor_with(&[&rectangle("a", true)]);

    assert!(editor.overlay().parts().is_empty());
}

#[test]
fn a_selected_element_gets_a_frame_around_it() {
    let mut editor = editor_with(&[&rectangle("a", true)]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);

    // The rectangle's box is 100..200 by 100..150; the frame keeps 4 units
    // away from it, and is one unit wide.
    let (corners, width) = frame_of(&editor);
    assert_corners(
        &corners,
        [(96.0, 96.0), (204.0, 96.0), (204.0, 154.0), (96.0, 154.0)],
    );
    assert_eq!(width, 1.0);

    tap(&mut editor, "mouse", (500.0, 500.0), 1.0);
    assert!(editor.overlay().parts().is_empty());
}

#[test]
fn the_frame_stays_one_point_wide_and_away_at_any_zoom() {
    let mut editor = editor_with(&[&rectangle("a", true)]);
    tap(&mut editor, "mouse", (150.0, 125.0), 0.0);
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);

    let (corners, width) = frame_of(&editor);
    assert_corners(
        &corners,
        [(98.0, 98.0), (202.0, 98.0), (202.0, 152.0), (98.0, 152.0)],
    );
    assert_eq!(width, 0.5);
}

#[test]
fn the_frame_turns_with_the_element() {
    // 100 x 20 at (100, 100), a quarter turn clockwise about (150, 110).
    let turned = element(
        "a",
        "rectangle",
        &format!(
            r#""x":100,"y":100,"width":100,"height":20,"angle":{},{FILLED},"roundness":null"#,
            std::f64::consts::FRAC_PI_2
        ),
    );
    let mut editor = editor_with(&[&turned]);
    tap(&mut editor, "mouse", (150.0, 110.0), 0.0);

    // The padded corners (96, 96), (204, 96), (204, 124), (96, 124) are 54
    // and 14 from the centre; a quarter turn takes (dx, dy) to (-dy, dx).
    let (corners, _) = frame_of(&editor);
    assert_corners(
        &corners,
        [(164.0, 56.0), (164.0, 164.0), (136.0, 164.0), (136.0, 56.0)],
    );
}

#[test]
fn the_frame_of_an_image_touches_it() {
    let image = element(
        "a",
        "image",
        r#""x":10,"y":20,"width":100,"height":80,"fileId":"f","backgroundColor":"transparent""#,
    );
    let mut editor = editor_with(&[&image]);
    tap(&mut editor, "mouse", (50.0, 50.0), 0.0);

    let (corners, _) = frame_of(&editor);
    assert_corners(
        &corners,
        [(10.0, 20.0), (110.0, 20.0), (110.0, 100.0), (10.0, 100.0)],
    );
}

#[test]
fn the_frame_of_a_line_surrounds_its_points() {
    let line = element(
        "a",
        "line",
        r#""x":50,"y":50,"width":100,"height":40,"points":[[0,0],[100,40]],"roughness":0,
        "roundness":null"#,
    );
    let mut editor = editor_with(&[&line]);
    tap(&mut editor, "mouse", (100.0, 70.0), 0.0);

    let (corners, _) = frame_of(&editor);
    assert_corners(
        &corners,
        [(46.0, 46.0), (154.0, 46.0), (154.0, 94.0), (46.0, 94.0)],
    );
}

#[test]
fn the_overlay_revision_tells_when_the_overlay_may_have_changed() {
    let mut editor = editor_with(&[&rectangle("a", true)]);
    let before = editor.overlay_revision();

    // A tap on empty Canvas that changes nothing.
    tap(&mut editor, "mouse", (500.0, 500.0), 0.0);
    assert_eq!(editor.overlay_revision(), before);

    tap(&mut editor, "mouse", (150.0, 125.0), 1.0);
    let selected = editor.overlay_revision();
    assert_ne!(selected, before);

    // Tapping what is selected already changes nothing either.
    tap(&mut editor, "mouse", (150.0, 125.0), 2.0);
    assert_eq!(editor.overlay_revision(), selected);

    // The frame's width and distance depend on zoom, its place does not.
    replay(
        &mut editor,
        "3 down 1 finger 10 10\n3.1 down 2 finger 50 10\n3.2 move 2 finger 150 10\n3.3 up 2 finger 150 10\n3.4 up 1 finger 10 10",
    );
    assert_ne!(editor.viewport().zoom(), 1.0);
    assert_ne!(editor.overlay_revision(), selected);
    let zoomed = editor.overlay_revision();

    editor.scroll(workingtitle_core::ScrollEvent {
        x: 0.0,
        y: 0.0,
        dx: 30.0,
        dy: 0.0,
        modifiers: Modifiers::default(),
    });
    assert_eq!(editor.overlay_revision(), zoomed, "panning moves no frame");
}

#[test]
fn the_overlay_does_not_change_while_something_unselected_is_drawn() {
    let mut editor = Editor::new();
    let before = editor.overlay_revision();
    editor.set_tool(Tool::Rectangle);

    replay(&mut editor, "0 down 0 mouse 10 10\n0.1 move 0 mouse 60 60");

    assert_eq!(editor.element_count(), 1);
    assert_eq!(editor.overlay_revision(), before);

    // Lifting selects the new shape, which then has a frame.
    replay(&mut editor, "0.2 up 0 mouse 60 60");
    assert_ne!(editor.overlay_revision(), before);
    assert!(!editor.overlay().parts().is_empty());
}

#[test]
fn loading_a_drawing_takes_the_frame_away() {
    let mut editor = editor_with_selection();
    let before = editor.overlay_revision();

    editor.load(&Editor::new().save()).unwrap();

    assert_ne!(editor.overlay_revision(), before);
    assert!(editor.overlay().parts().is_empty());
}
