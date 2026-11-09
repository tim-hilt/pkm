//! The eraser Tool (ADR-0004, ADR-0006), tested at the Editor seam with
//! recorded pointer sequences: pointer events in, saved Drawing, geometry, Tool
//! and undo state out. Expected behaviour is worked out by hand from
//! Excalidraw's `eraser/index.ts` (which Elements a step of the eraser path
//! touches) and `App.tsx` (previewing at 20 % opacity, erasing on release).

use pkm_core::json::Value;
use pkm_core::{Editor, Geometry, Tool};

use crate::common;
use common::{editor_with, replay, saved_elements, text};

/// A hollow rectangle from (100, 100) to (200, 150).
const HOLLOW: &str = r##"{"id":"hollow","type":"rectangle","x":100,"y":100,"width":100,
    "height":50,"strokeWidth":2,"backgroundColor":"transparent","roundness":null,
    "version":3,"versionNonce":7}"##;
/// A filled rectangle from (300, 100) to (400, 150).
const FILLED: &str = r##"{"id":"filled","type":"rectangle","x":300,"y":100,"width":100,
    "height":50,"strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,
    "version":3,"versionNonce":7}"##;

/// An editor with the eraser Tool and the Tool lock on, so that the Tool
/// stays for the several erases some tests make.
fn erasing(elements: &[&str]) -> Editor {
    let mut editor = editor_with(elements);
    editor.set_tool(Tool::Eraser);
    editor.set_tool_lock(true);
    editor
}

fn is_deleted(editor: &Editor, id: &str) -> bool {
    saved_elements(editor)
        .iter()
        .find(|e| text(e, "id") == id)
        .and_then(|e| e.get("isDeleted").and_then(Value::as_bool))
        .unwrap()
}

/// The opacity the Element with `handle` is drawn with, if it is drawn.
fn drawn_opacity(geometry: &Geometry, handle: u32) -> Option<f32> {
    geometry
        .parts()
        .iter()
        .find(|p| p.element == handle)
        .map(|p| p.opacity)
}

// --- Erasing what the path touches --------------------------------------------

#[test]
fn an_element_the_path_crosses_is_deleted_on_release() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 up 1 mouse 150 170",
    );

    assert!(is_deleted(&editor, "hollow"));
    assert_eq!(editor.element_count(), 0);
}

#[test]
fn an_element_the_path_does_not_touch_stays() {
    let mut editor = erasing(&[HOLLOW, FILLED]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 up 1 mouse 150 170",
    );

    assert!(is_deleted(&editor, "hollow"));
    assert!(!is_deleted(&editor, "filled"));
}

#[test]
fn an_element_touched_early_is_erased_even_if_the_path_moves_on() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 120
         0.2 move 1 mouse 500 500
         0.3 up 1 mouse 500 500",
    );

    assert!(is_deleted(&editor, "hollow"));
}

#[test]
fn a_fast_step_that_jumps_over_an_outline_still_touches_it() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 20
         0.1 move 1 mouse 150 300
         0.2 up 1 mouse 150 300",
    );

    assert!(is_deleted(&editor, "hollow"));
}

#[test]
fn a_path_inside_a_hollow_shape_touches_nothing() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 120 120
         0.1 move 1 mouse 180 130
         0.2 up 1 mouse 180 130",
    );

    assert!(!is_deleted(&editor, "hollow"));
}

#[test]
fn a_path_ending_inside_a_filled_shape_touches_it() {
    let mut editor = erasing(&[FILLED]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 350 120
         0.1 move 1 mouse 360 125
         0.2 up 1 mouse 360 125",
    );

    assert!(is_deleted(&editor, "filled"));
}

// --- Preview ------------------------------------------------------------------

#[test]
fn touched_elements_are_previewed_faded_and_deleted_only_on_release() {
    let mut editor = erasing(&[HOLLOW, FILLED]);
    let before = editor.geometry();
    let version = before
        .parts()
        .iter()
        .find(|p| p.element == 0)
        .unwrap()
        .version;
    assert_eq!(drawn_opacity(&before, 0), Some(1.0));

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170",
    );

    let preview = editor.geometry();
    // Excalidraw draws what is about to be erased at 20 % opacity.
    assert_eq!(drawn_opacity(&preview, 0), Some(0.2));
    assert_eq!(drawn_opacity(&preview, 1), Some(1.0));
    // Clients cache what they draw per Element and version, so a faded
    // Element is another version than the same Element at full opacity.
    let faded = preview.parts().iter().find(|p| p.element == 0).unwrap();
    assert_ne!(faded.version, version);
    assert!(!is_deleted(&editor, "hollow"));

    replay(&mut editor, "0.2 up 1 mouse 150 170");

    assert_eq!(drawn_opacity(&editor.geometry(), 0), None);
}

#[test]
fn previewing_changes_the_revision_so_that_clients_draw_it() {
    let mut editor = erasing(&[HOLLOW]);
    replay(&mut editor, "0.0 down 1 mouse 150 80");
    let revision = editor.revision();

    replay(&mut editor, "0.1 move 1 mouse 150 120");

    assert_ne!(editor.revision(), revision);
}

#[test]
fn a_cancelled_erase_deletes_nothing_and_shows_the_elements_again() {
    let mut editor = erasing(&[HOLLOW]);
    let version = editor.geometry().parts()[0].version;

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 120
         0.2 cancel 1 mouse 150 120",
    );

    assert!(!is_deleted(&editor, "hollow"));
    assert!(!editor.can_undo());
    let geometry = editor.geometry();
    assert_eq!(drawn_opacity(&geometry, 0), Some(1.0));
    assert_eq!(geometry.parts()[0].version, version);
}

// --- Tool and history -------------------------------------------------------------

#[test]
fn the_tool_returns_to_selection_after_erasing() {
    let mut editor = editor_with(&[HOLLOW]);
    editor.set_tool(Tool::Eraser);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 up 1 mouse 150 170",
    );

    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn the_tool_stays_active_after_erasing_while_the_tool_lock_is_on() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 up 1 mouse 150 170",
    );

    assert_eq!(editor.tool(), Tool::Eraser);
}

#[test]
fn one_erase_is_one_undo_step() {
    let mut editor = erasing(&[HOLLOW, FILLED]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 move 1 mouse 350 170
         0.3 move 1 mouse 350 80
         0.4 up 1 mouse 350 80",
    );
    assert!(is_deleted(&editor, "hollow") && is_deleted(&editor, "filled"));

    editor.undo();
    assert!(!is_deleted(&editor, "hollow") && !is_deleted(&editor, "filled"));
    assert!(!editor.can_undo());

    editor.redo();
    assert!(is_deleted(&editor, "hollow") && is_deleted(&editor, "filled"));
}

#[test]
fn an_erase_that_touches_nothing_is_no_undo_step() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 500 500
         0.1 move 1 mouse 600 600
         0.2 up 1 mouse 600 600",
    );

    assert!(!editor.can_undo());
}

// --- Tapping ------------------------------------------------------------------------

#[test]
fn a_tap_erases_every_element_under_it() {
    // A filled rectangle with another filled one on top of it.
    let on_top = FILLED.replace("filled", "on_top");
    let mut editor = erasing(&[FILLED, &on_top, HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 350 125
         0.05 up 1 mouse 350 125",
    );

    assert!(is_deleted(&editor, "filled") && is_deleted(&editor, "on_top"));
    assert!(!is_deleted(&editor, "hollow"));
    assert_eq!(editor.tool(), Tool::Eraser);
}

#[test]
fn a_tap_on_empty_canvas_erases_nothing() {
    let mut editor = erasing(&[HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 500 500
         0.05 up 1 mouse 500 500",
    );

    assert!(!is_deleted(&editor, "hollow"));
    assert!(!editor.can_undo());
}

#[test]
fn a_finger_tap_reaches_further_than_a_mouse_click() {
    // 15 units outside the outline: out of reach for a mouse, in reach of the
    // 22 points of a finger.
    let mut editor = erasing(&[HOLLOW]);
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 85\n0.05 up 1 mouse 150 85",
    );
    assert!(!is_deleted(&editor, "hollow"));

    replay(
        &mut editor,
        "1.0 down 2 finger 150 85\n1.05 up 2 finger 150 85",
    );
    assert!(is_deleted(&editor, "hollow"));
}

// --- What kind of Element is touched how ---------------------------------------------

#[test]
fn a_line_is_touched_by_passing_near_its_path() {
    let line = r#"{"id":"line","type":"line","x":100,"y":100,"width":100,"height":0,
        "points":[[0,0],[100,0]],"strokeWidth":2,"roundness":null}"#;
    let mut editor = erasing(&[line]);

    // The tolerance is the stroke width, 2.
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 90
         0.1 move 1 mouse 150 95
         0.2 up 1 mouse 150 95",
    );
    assert!(!is_deleted(&editor, "line"));

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 90
         1.1 move 1 mouse 150 101
         1.2 up 1 mouse 150 101",
    );
    assert!(is_deleted(&editor, "line"));
}

#[test]
fn a_freedraw_stroke_is_touched_by_passing_near_its_ink() {
    let stroke = r#"{"id":"stroke","type":"freedraw","x":100,"y":100,"width":100,"height":0,
        "points":[[0,0],[50,0],[100,0]],"pressures":[],"simulatePressure":true,
        "strokeWidth":2}"#;
    let mut editor = erasing(&[stroke]);

    // Far beyond the ink and the 2.25 tolerance.
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 90
         0.2 up 1 mouse 150 90",
    );
    assert!(!is_deleted(&editor, "stroke"));

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 80
         1.1 move 1 mouse 150 102
         1.2 up 1 mouse 150 102",
    );
    assert!(is_deleted(&editor, "stroke"));
}

#[test]
fn a_turned_shape_is_touched_where_it_is_shown() {
    // A thin bar from (100, 100) to (200, 110), turned a quarter about its
    // middle (150, 105): it is shown from (145, 55) to (155, 155).
    let bar = r#"{"id":"bar","type":"rectangle","x":100,"y":100,"width":100,"height":10,
        "angle":1.5707963267948966,"strokeWidth":2,"roundness":null}"#;
    let mut editor = erasing(&[bar]);

    // Crosses where the bar was before it was turned, but not where it is.
    replay(
        &mut editor,
        "0.0 down 1 mouse 105 80
         0.1 move 1 mouse 105 130
         0.2 up 1 mouse 105 130",
    );
    assert!(!is_deleted(&editor, "bar"));

    replay(
        &mut editor,
        "1.0 down 1 mouse 120 80
         1.1 move 1 mouse 170 80
         1.2 up 1 mouse 170 80",
    );
    assert!(is_deleted(&editor, "bar"));
}

#[test]
fn locked_elements_are_not_erased() {
    let locked = HOLLOW.replace(r#""version":3"#, r#""locked":true,"version":3"#);
    let mut editor = erasing(&[&locked]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 170
         0.2 up 1 mouse 150 170",
    );
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 100\n1.05 up 1 mouse 150 100",
    );

    assert!(!is_deleted(&editor, "hollow"));
}

// --- What goes with an Element ---------------------------------------------------------

const CONTAINER: &str = r##"{"id":"c","type":"rectangle","x":100,"y":100,"width":100,"height":50,
    "strokeWidth":2,"roundness":null,"boundElements":[{"id":"t","type":"text"}]}"##;
const LABEL: &str = r##"{"id":"t","type":"text","x":130,"y":115,"width":40,"height":20,
    "text":"hi","originalText":"hi","containerId":"c","fontSize":16,"fontFamily":5}"##;

#[test]
fn erasing_a_container_erases_its_label() {
    let mut editor = erasing(&[CONTAINER, LABEL]);

    // Across the container's left edge only.
    replay(
        &mut editor,
        "0.0 down 1 mouse 80 125
         0.1 move 1 mouse 110 125
         0.2 up 1 mouse 110 125",
    );

    assert!(is_deleted(&editor, "c") && is_deleted(&editor, "t"));
}

#[test]
fn erasing_a_label_erases_its_container() {
    let mut editor = erasing(&[CONTAINER, LABEL]);

    // Across the label's box only, ending inside it.
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 100
         0.1 move 1 mouse 150 125
         0.2 up 1 mouse 150 125",
    );

    assert!(is_deleted(&editor, "t") && is_deleted(&editor, "c"));
}

#[test]
fn erasing_an_element_erases_its_whole_outermost_group() {
    let grouped = |id: &str, x: u32| {
        format!(
            r#"{{"id":"{id}","type":"rectangle","x":{x},"y":100,"width":50,"height":50,
            "strokeWidth":2,"roundness":null,"groupIds":["inner","outer"]}}"#
        )
    };
    let left = grouped("left", 100);
    let right = grouped("right", 400);
    let mut editor = erasing(&[&left, &right, HOLLOW]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 80 125
         0.1 move 1 mouse 110 125
         0.2 up 1 mouse 110 125",
    );

    assert!(is_deleted(&editor, "left") && is_deleted(&editor, "right"));
    editor.undo();
    assert!(!is_deleted(&editor, "left") && !is_deleted(&editor, "right"));
}

#[test]
fn a_group_is_previewed_as_a_whole() {
    let grouped = |id: &str, x: u32| {
        format!(
            r#"{{"id":"{id}","type":"rectangle","x":{x},"y":100,"width":50,"height":50,
            "strokeWidth":2,"roundness":null,"groupIds":["g"]}}"#
        )
    };
    let mut editor = erasing(&[&grouped("left", 100), &grouped("right", 400)]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 80 125
         0.1 move 1 mouse 110 125",
    );

    let geometry = editor.geometry();
    assert_eq!(drawn_opacity(&geometry, 0), Some(0.2));
    assert_eq!(drawn_opacity(&geometry, 1), Some(0.2));
}

#[test]
fn erasing_a_shape_unbinds_the_arrows_bound_to_it_and_undo_binds_them_again() {
    let shape = r#"{"id":"box","type":"rectangle","x":100,"y":0,"width":100,"height":100,
        "strokeWidth":2,"roundness":null,"boundElements":[{"id":"arrow","type":"arrow"}]}"#;
    let arrow = r#"{"id":"arrow","type":"arrow","x":0,"y":40,"width":94,"height":0,
        "points":[[0,0],[94,0]],"strokeWidth":2,"roundness":{"type":2},
        "startBinding":null,"endBinding":{"elementId":"box","mode":"orbit","fixedPoint":[0.2,0.4]},
        "startArrowhead":null,"endArrowhead":"arrow","elbowed":false}"#;
    let mut editor = erasing(&[shape, arrow]);
    let member = |editor: &Editor, id: &str, key: &str| {
        saved_elements(editor)
            .into_iter()
            .find(|e| text(e, "id") == id)
            .unwrap()
            .get(key)
            .cloned()
            .unwrap()
    };

    // Across the shape's top edge, far from the arrow.
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 -20
         0.1 move 1 mouse 150 20
         0.2 up 1 mouse 150 20",
    );

    assert!(is_deleted(&editor, "box"));
    assert!(!is_deleted(&editor, "arrow"));
    assert_eq!(member(&editor, "arrow", "endBinding"), Value::Null);

    editor.undo();

    assert!(!is_deleted(&editor, "box"));
    let restored = member(&editor, "arrow", "endBinding");
    assert_eq!(text(&restored, "elementId"), "box");
}

#[test]
fn erasing_an_arrow_takes_it_out_of_the_boundelements_of_its_shape() {
    let shape = r#"{"id":"box","type":"rectangle","x":100,"y":0,"width":100,"height":100,
        "strokeWidth":2,"roundness":null,"boundElements":[{"id":"arrow","type":"arrow"}]}"#;
    let arrow = r#"{"id":"arrow","type":"arrow","x":0,"y":40,"width":94,"height":0,
        "points":[[0,0],[94,0]],"strokeWidth":2,"roundness":{"type":2},
        "startBinding":null,"endBinding":{"elementId":"box","mode":"orbit","fixedPoint":[0.2,0.4]},
        "startArrowhead":null,"endArrowhead":"arrow","elbowed":false}"#;
    let mut editor = erasing(&[shape, arrow]);

    replay(
        &mut editor,
        "0.0 down 1 mouse 20 20
         0.1 move 1 mouse 20 60
         0.2 up 1 mouse 20 60",
    );

    assert!(is_deleted(&editor, "arrow"));
    assert!(!is_deleted(&editor, "box"));
    let shape = saved_elements(&editor)
        .into_iter()
        .find(|e| text(e, "id") == "box")
        .unwrap();
    assert_eq!(
        shape
            .get("boundElements")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        0
    );
}

// --- Selection, other Tools and gestures ------------------------------------------------

#[test]
fn choosing_the_eraser_clears_the_selection_and_undo_of_an_erase_keeps_it_empty() {
    let mut editor = editor_with(&[HOLLOW, FILLED]);
    common::tap(&mut editor, "mouse", (100.0, 125.0), 0.0);
    assert_eq!(editor.selection(), ["hollow"]);
    editor.set_tool(Tool::Eraser);
    assert!(editor.selection().is_empty());

    replay(
        &mut editor,
        "1.0 down 1 mouse 150 80
         1.1 move 1 mouse 150 170
         1.2 up 1 mouse 150 170",
    );
    assert!(editor.selection().is_empty());

    editor.undo();
    assert!(editor.selection().is_empty());
}

#[test]
fn changing_the_tool_during_an_erase_calls_it_off() {
    let mut editor = erasing(&[HOLLOW]);
    replay(
        &mut editor,
        "0.0 down 1 mouse 150 80
         0.1 move 1 mouse 150 120",
    );

    editor.set_tool(Tool::Selection);
    replay(&mut editor, "0.2 up 1 mouse 150 120");

    assert!(!is_deleted(&editor, "hollow"));
    assert_eq!(drawn_opacity(&editor.geometry(), 0), Some(1.0));
}

#[test]
fn a_second_finger_calls_off_an_erase_instead_of_deleting() {
    let mut editor = erasing(&[HOLLOW]);
    replay(
        &mut editor,
        "0.0 down 1 finger 150 80
         0.1 move 1 finger 150 120
         0.2 down 2 finger 300 300
         0.3 move 1 finger 150 170
         0.4 up 1 finger 150 170
         0.5 up 2 finger 300 300",
    );

    assert!(!is_deleted(&editor, "hollow"));
    assert!(!editor.can_undo());
    assert_eq!(editor.tool(), Tool::Eraser);
}

#[test]
fn undo_does_nothing_while_the_pointer_is_erasing() {
    let mut editor = erasing(&[HOLLOW, FILLED]);
    replay(
        &mut editor,
        "0.0 down 1 mouse 350 125\n0.05 up 1 mouse 350 125",
    );
    assert!(is_deleted(&editor, "filled"));
    replay(
        &mut editor,
        "1.0 down 1 mouse 150 80
         1.1 move 1 mouse 150 120",
    );

    editor.undo();

    assert!(is_deleted(&editor, "filled"));
}

#[test]
fn a_tool_lock_does_not_matter_to_the_eraser() {
    let mut editor = erasing(&[HOLLOW]);
    editor.set_tool_lock(true);

    replay(
        &mut editor,
        "0.0 down 1 mouse 150 100\n0.05 up 1 mouse 150 100",
    );

    assert_eq!(editor.tool(), Tool::Eraser);
}
