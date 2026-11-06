//! Drawing rectangles, diamonds and ellipses (ADR-0006), tested at the Editor
//! seam with recorded pointer sequences: pointer events in, saved JSON, Tool
//! and geometry out. Coordinates in the comments are worked out by hand.

use workingtitle_core::json::{Value, parse};
use workingtitle_core::{Editor, Modifiers, PointerEvent, PointerKind, PointerPhase, Tool};

use PointerKind::{Finger, Mouse, Pencil};
use PointerPhase::{Cancel, Down, Move, Up};

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
};

fn send(
    editor: &mut Editor,
    (id, kind): (u64, PointerKind),
    phase: PointerPhase,
    (x, y): (f64, f64),
    modifiers: Modifiers,
) {
    editor.pointer(PointerEvent {
        id,
        phase,
        x,
        y,
        kind,
        button: workingtitle_core::PointerButton::Primary,
        pressure: 0.5,
        timestamp: 0.0,
        modifiers,
    });
}

/// A mouse drag from `from` to `to`.
fn drag(editor: &mut Editor, from: (f64, f64), to: (f64, f64)) {
    drag_with(editor, from, to, Modifiers::default());
}

fn drag_with(editor: &mut Editor, from: (f64, f64), to: (f64, f64), modifiers: Modifiers) {
    let mouse = (0, Mouse);
    send(editor, mouse, Down, from, Modifiers::default());
    send(editor, mouse, Move, to, modifiers);
    send(editor, mouse, Up, to, modifiers);
}

fn editor_with(tool: Tool) -> Editor {
    let mut editor = Editor::new();
    editor.set_tool(tool);
    editor
}

/// The `elements` of the saved Drawing.
fn elements(editor: &Editor) -> Vec<Value> {
    let saved = String::from_utf8(editor.save()).unwrap();
    parse(&saved)
        .unwrap()
        .get("elements")
        .and_then(Value::as_array)
        .unwrap()
        .to_vec()
}

fn number(element: &Value, key: &str) -> f64 {
    element.get(key).and_then(Value::as_f64).unwrap()
}

fn text<'a>(element: &'a Value, key: &str) -> &'a str {
    element.get(key).and_then(Value::as_str).unwrap()
}

/// `(x, y, width, height)`.
fn frame(element: &Value) -> (f64, f64, f64, f64) {
    (
        number(element, "x"),
        number(element, "y"),
        number(element, "width"),
        number(element, "height"),
    )
}

// --- Creating the Element ----------------------------------------------------

#[test]
fn dragging_with_the_rectangle_tool_creates_a_rectangle() {
    let mut editor = editor_with(Tool::Rectangle);

    drag(&mut editor, (10.0, 20.0), (110.0, 70.0));

    let shapes = elements(&editor);
    assert_eq!(shapes.len(), 1);
    assert_eq!(text(&shapes[0], "type"), "rectangle");
    assert_eq!(frame(&shapes[0]), (10.0, 20.0, 100.0, 50.0));
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn the_diamond_and_ellipse_tools_create_those_shapes() {
    let mut editor = editor_with(Tool::Diamond);
    drag(&mut editor, (0.0, 0.0), (40.0, 40.0));
    editor.set_tool(Tool::Ellipse);
    drag(&mut editor, (50.0, 0.0), (90.0, 30.0));

    let types: Vec<_> = elements(&editor)
        .iter()
        .map(|e| text(e, "type").to_owned())
        .collect();
    assert_eq!(types, ["diamond", "ellipse"]);
}

#[test]
fn dragging_up_and_left_still_gives_a_positive_size() {
    let mut editor = editor_with(Tool::Rectangle);

    drag(&mut editor, (110.0, 70.0), (10.0, 20.0));

    assert_eq!(frame(&elements(&editor)[0]), (10.0, 20.0, 100.0, 50.0));
}

#[test]
fn a_finger_and_a_pencil_draw_like_a_mouse() {
    let mut editor = editor_with(Tool::Rectangle);
    for (id, kind) in [(1, Finger), (2, Pencil)] {
        let pointer = (id, kind);
        send(&mut editor, pointer, Down, (0.0, 0.0), Modifiers::default());
        send(
            &mut editor,
            pointer,
            Move,
            (30.0, 40.0),
            Modifiers::default(),
        );
        send(&mut editor, pointer, Up, (30.0, 40.0), Modifiers::default());
        editor.set_tool(Tool::Rectangle);
    }

    assert_eq!(elements(&editor).len(), 2);
}

#[test]
fn positions_are_in_scene_coordinates_whatever_the_viewport() {
    let mut editor = editor_with(Tool::Rectangle);
    editor.set_view_size(400.0, 200.0);
    // Ten zoom steps make 200 % around the view's middle (200, 100), so the
    // Scene point at the view's top-left corner is (200 - 200 / 2, 100 - 100 / 2).
    for _ in 0..10 {
        editor.zoom_in();
    }
    assert_eq!(editor.viewport().zoom(), 2.0);
    assert_eq!(editor.viewport().origin(), (100.0, 50.0));

    drag(&mut editor, (40.0, 20.0), (140.0, 120.0));

    // View point (40, 20) is Scene point (100 + 40 / 2, 50 + 20 / 2).
    assert_eq!(frame(&elements(&editor)[0]), (120.0, 60.0, 50.0, 50.0));
}

#[test]
fn new_elements_go_on_top_of_the_existing_ones() {
    let mut editor = Editor::new();
    editor
        .load(
            br#"{"type":"excalidraw","version":2,"elements":[
                {"id":"old","type":"rectangle","x":0,"y":0,"width":5,"height":5}]}"#,
        )
        .unwrap();
    editor.set_tool(Tool::Ellipse);

    drag(&mut editor, (10.0, 10.0), (50.0, 50.0));

    let shapes = elements(&editor);
    assert_eq!(text(&shapes[0], "id"), "old");
    assert_eq!(text(&shapes[1], "type"), "ellipse");
}

// --- Style -------------------------------------------------------------------

#[test]
fn a_new_shape_has_excalidraws_default_style() {
    let mut editor = editor_with(Tool::Rectangle);
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    let shape = &elements(&editor)[0];

    assert_eq!(text(shape, "strokeColor"), "#1e1e1e");
    assert_eq!(text(shape, "backgroundColor"), "transparent");
    assert_eq!(text(shape, "fillStyle"), "solid");
    assert_eq!(number(shape, "strokeWidth"), 2.0);
    assert_eq!(text(shape, "strokeStyle"), "solid");
    assert_eq!(number(shape, "roughness"), 1.0);
    assert_eq!(number(shape, "opacity"), 100.0);
    assert_eq!(number(shape, "angle"), 0.0);
    assert_eq!(shape.get("isDeleted").and_then(Value::as_bool), Some(false));
}

#[test]
fn corners_are_rounded_as_excalidraw_does_for_each_shape() {
    let mut editor = Editor::new();
    for tool in [Tool::Rectangle, Tool::Diamond, Tool::Ellipse] {
        editor.set_tool(tool);
        drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    }

    let types: Vec<_> = elements(&editor)
        .iter()
        .map(|e| number(e.get("roundness").unwrap(), "type"))
        .collect();
    // Adaptive radius for rectangles, proportional for the others.
    assert_eq!(types, [3.0, 2.0, 2.0]);
}

// --- Identity ----------------------------------------------------------------

#[test]
fn every_shape_gets_a_new_random_seed_and_id() {
    let mut editor = Editor::new();
    for _ in 0..20 {
        editor.set_tool(Tool::Rectangle);
        drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    }

    let shapes = elements(&editor);
    let mut seeds: Vec<_> = shapes.iter().map(|e| number(e, "seed")).collect();
    // Excalidraw's `randomInteger`: an integer below 2^31.
    assert!(
        seeds
            .iter()
            .all(|s| s.fract() == 0.0 && (0.0..2_147_483_648.0).contains(s))
    );
    seeds.sort_by(f64::total_cmp);
    seeds.dedup();
    assert_eq!(seeds.len(), 20, "seeds repeat");

    let mut ids: Vec<_> = shapes.iter().map(|e| text(e, "id").to_owned()).collect();
    assert!(ids.iter().all(|id| id.len() == 21));
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 20, "ids repeat");
}

// --- Shift constrains proportions ---------------------------------------------

#[test]
fn shift_makes_a_square_as_big_as_the_longer_side() {
    let mut editor = editor_with(Tool::Rectangle);

    drag_with(&mut editor, (10.0, 10.0), (110.0, 40.0), SHIFT);

    assert_eq!(frame(&elements(&editor)[0]), (10.0, 10.0, 100.0, 100.0));
}

#[test]
fn shift_follows_the_taller_side_and_the_drag_direction() {
    let mut editor = editor_with(Tool::Ellipse);

    // 30 right and 80 up: the square has side 80, is anchored at the start
    // on its bottom left corner.
    drag_with(&mut editor, (0.0, 100.0), (30.0, 20.0), SHIFT);

    assert_eq!(frame(&elements(&editor)[0]), (0.0, 20.0, 80.0, 80.0));
}

#[test]
fn shift_squares_towards_the_top_left_too() {
    let mut editor = editor_with(Tool::Diamond);

    drag_with(&mut editor, (100.0, 100.0), (60.0, 90.0), SHIFT);

    assert_eq!(frame(&elements(&editor)[0]), (60.0, 60.0, 40.0, 40.0));
}

#[test]
fn shift_can_be_pressed_and_released_during_the_drag() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);

    send(&mut editor, mouse, Down, (0.0, 0.0), Modifiers::default());
    send(&mut editor, mouse, Move, (100.0, 50.0), SHIFT);
    send(
        &mut editor,
        mouse,
        Move,
        (100.0, 50.0),
        Modifiers::default(),
    );
    send(&mut editor, mouse, Up, (100.0, 50.0), Modifiers::default());

    assert_eq!(frame(&elements(&editor)[0]), (0.0, 0.0, 100.0, 50.0));
}

// --- The Tool after drawing ----------------------------------------------------

#[test]
fn the_tool_returns_to_selection_after_drawing() {
    let mut editor = editor_with(Tool::Rectangle);

    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));

    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn the_tool_stays_while_it_is_locked_so_shapes_can_follow_each_other() {
    let mut editor = editor_with(Tool::Diamond);
    editor.set_tool_lock(true);

    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    assert_eq!(editor.tool(), Tool::Diamond);
    drag(&mut editor, (60.0, 0.0), (90.0, 50.0));

    assert_eq!(editor.tool(), Tool::Diamond);
    assert_eq!(elements(&editor).len(), 2);
}

#[test]
fn the_tool_stays_while_the_pointer_is_still_down() {
    let mut editor = editor_with(Tool::Rectangle);

    send(
        &mut editor,
        (0, Mouse),
        Down,
        (0.0, 0.0),
        Modifiers::default(),
    );
    send(
        &mut editor,
        (0, Mouse),
        Move,
        (50.0, 50.0),
        Modifiers::default(),
    );

    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn the_tool_lock_is_off_until_switched_on_and_can_be_switched_off() {
    let mut editor = Editor::new();
    assert!(!editor.tool_lock());

    editor.set_tool_lock(true);
    assert!(editor.tool_lock());

    editor.set_tool_lock(false);
    assert!(!editor.tool_lock());
}

#[test]
fn locking_keeps_the_tool_and_unlocking_returns_to_selection() {
    let mut editor = editor_with(Tool::Ellipse);

    editor.set_tool_lock(true);
    assert_eq!(editor.tool(), Tool::Ellipse);

    // As on excalidraw.com: the lock is what kept the Tool.
    editor.set_tool_lock(false);
    assert_eq!(editor.tool(), Tool::Selection);
}

#[test]
fn unlocking_while_already_unlocked_leaves_the_tool_alone() {
    let mut editor = editor_with(Tool::Rectangle);

    editor.set_tool_lock(false);

    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn the_lock_survives_choosing_another_tool() {
    let mut editor = Editor::new();
    editor.set_tool_lock(true);

    editor.set_tool(Tool::Rectangle);
    editor.set_tool(Tool::Hand);

    assert!(editor.tool_lock());
}

// --- Gestures that draw nothing -------------------------------------------------

#[test]
fn a_tap_draws_nothing_and_keeps_the_tool() {
    let mut editor = editor_with(Tool::Rectangle);

    drag(&mut editor, (30.0, 30.0), (30.0, 30.0));

    assert!(elements(&editor).is_empty());
    assert_eq!(editor.element_count(), 0);
    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn a_cancelled_pointer_removes_the_shape_and_keeps_the_tool() {
    let mut editor = editor_with(Tool::Rectangle);
    let finger = (1, Finger);

    send(&mut editor, finger, Down, (0.0, 0.0), Modifiers::default());
    send(
        &mut editor,
        finger,
        Move,
        (50.0, 50.0),
        Modifiers::default(),
    );
    send(
        &mut editor,
        finger,
        Cancel,
        (50.0, 50.0),
        Modifiers::default(),
    );

    assert!(elements(&editor).is_empty());
    assert_eq!(editor.tool(), Tool::Rectangle);
}

#[test]
fn a_second_finger_turns_the_drawing_into_a_pinch_and_removes_the_shape() {
    let mut editor = editor_with(Tool::Rectangle);
    let none = Modifiers::default();

    send(&mut editor, (1, Finger), Down, (100.0, 100.0), none);
    send(&mut editor, (1, Finger), Move, (120.0, 120.0), none);
    send(&mut editor, (2, Finger), Down, (200.0, 100.0), none);
    send(&mut editor, (2, Finger), Move, (300.0, 100.0), none);
    send(&mut editor, (1, Finger), Move, (0.0, 100.0), none);
    send(&mut editor, (2, Finger), Up, (300.0, 100.0), none);
    send(&mut editor, (1, Finger), Up, (0.0, 100.0), none);

    assert!(elements(&editor).is_empty());
    assert_eq!(editor.tool(), Tool::Rectangle);
    assert!(editor.viewport().zoom() > 1.0, "the two fingers pinch");
}

#[test]
fn a_finger_landing_while_two_are_down_does_not_start_a_shape() {
    let mut editor = editor_with(Tool::Rectangle);
    let none = Modifiers::default();
    send(&mut editor, (1, Finger), Down, (0.0, 0.0), none);
    send(&mut editor, (2, Finger), Down, (50.0, 0.0), none);

    send(&mut editor, (3, Finger), Down, (100.0, 0.0), none);
    send(&mut editor, (3, Finger), Move, (150.0, 50.0), none);
    send(&mut editor, (3, Finger), Up, (150.0, 50.0), none);

    assert!(elements(&editor).is_empty());
}

#[test]
fn choosing_another_tool_mid_drag_drops_the_shape() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), Modifiers::default());
    send(&mut editor, mouse, Move, (50.0, 50.0), Modifiers::default());

    editor.set_tool(Tool::Hand);
    send(&mut editor, mouse, Move, (80.0, 80.0), Modifiers::default());
    send(&mut editor, mouse, Up, (80.0, 80.0), Modifiers::default());

    assert!(elements(&editor).is_empty());
}

#[test]
fn other_pointers_do_not_disturb_the_drawing() {
    let mut editor = editor_with(Tool::Rectangle);
    let none = Modifiers::default();

    send(&mut editor, (0, Mouse), Down, (0.0, 0.0), none);
    send(&mut editor, (7, Pencil), Move, (500.0, 500.0), none);
    send(&mut editor, (0, Mouse), Move, (40.0, 30.0), none);
    send(&mut editor, (7, Pencil), Up, (500.0, 500.0), none);
    send(&mut editor, (0, Mouse), Up, (40.0, 30.0), none);

    assert_eq!(frame(&elements(&editor)[0]), (0.0, 0.0, 40.0, 30.0));
}

#[test]
fn the_selection_and_hand_tools_draw_nothing() {
    let mut editor = Editor::new();
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    editor.set_tool(Tool::Hand);
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));

    assert!(elements(&editor).is_empty());
}

#[test]
fn loading_a_drawing_mid_drag_drops_the_shape_being_drawn() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), Modifiers::default());
    send(&mut editor, mouse, Move, (50.0, 50.0), Modifiers::default());

    editor.load(&Editor::new().save()).unwrap();
    send(&mut editor, mouse, Move, (80.0, 80.0), Modifiers::default());
    send(&mut editor, mouse, Up, (80.0, 80.0), Modifiers::default());

    assert!(elements(&editor).is_empty());
}

// --- What clients and excalidraw.com see ------------------------------------------

#[test]
fn the_saved_shape_is_complete_and_reads_back_unchanged() {
    let mut editor = editor_with(Tool::Rectangle);
    drag(&mut editor, (10.0, 20.0), (110.0, 70.0));

    let saved = editor.save();
    let mut reloaded = Editor::new();
    reloaded.load(&saved).unwrap();

    // Loading adds the defaults of whatever the element lacks; nothing was.
    assert_eq!(reloaded.save(), saved);
}

#[test]
fn the_saved_shape_has_the_members_and_order_of_excalidraws_own() {
    let mut editor = editor_with(Tool::Diamond);
    drag(&mut editor, (10.0, 20.0), (110.0, 70.0));
    let element = &elements(&editor)[0];

    let keys: Vec<_> = element
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        keys,
        [
            "id",
            "type",
            "x",
            "y",
            "width",
            "height",
            "angle",
            "strokeColor",
            "backgroundColor",
            "fillStyle",
            "strokeWidth",
            "strokeStyle",
            "roughness",
            "opacity",
            "groupIds",
            "frameId",
            "index",
            "roundness",
            "seed",
            "version",
            "versionNonce",
            "isDeleted",
            "boundElements",
            "updated",
            "created",
            "link",
            "locked"
        ]
    );
    assert!(
        number(element, "updated") > 1.7e12,
        "milliseconds since 1970"
    );
    assert!(number(element, "created") > 1.7e12);
}

#[test]
fn the_version_moves_on_with_every_change_so_clients_redraw() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);
    let mut versions = Vec::new();

    send(&mut editor, mouse, Down, (0.0, 0.0), Modifiers::default());
    for to in [(10.0, 10.0), (20.0, 20.0)] {
        send(&mut editor, mouse, Move, to, Modifiers::default());
        versions.push(number(&elements(&editor)[0], "version"));
    }

    assert!(versions[0] >= 1.0);
    assert!(versions[1] > versions[0]);
}

#[test]
fn the_shape_is_drawn_while_it_is_dragged() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);

    send(&mut editor, mouse, Down, (5.0, 5.0), Modifiers::default());
    // Nothing to see yet, but nothing may go wrong asking.
    let _ = editor.geometry();
    send(
        &mut editor,
        mouse,
        Move,
        (105.0, 55.0),
        Modifiers::default(),
    );

    let geometry = editor.geometry();
    assert!(!geometry.parts().is_empty());
    assert!(geometry.parts().iter().all(|part| part.element == 0));
}

#[test]
fn the_revision_tells_clients_when_to_ask_for_geometry_again() {
    let mut editor = editor_with(Tool::Rectangle);
    let mouse = (0, Mouse);
    let mut revisions = vec![editor.revision()];

    send(&mut editor, mouse, Down, (0.0, 0.0), Modifiers::default());
    revisions.push(editor.revision());
    send(&mut editor, mouse, Move, (10.0, 10.0), Modifiers::default());
    revisions.push(editor.revision());
    send(&mut editor, mouse, Move, (20.0, 20.0), Modifiers::default());
    revisions.push(editor.revision());
    for pair in revisions.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }

    // Input that changes nothing leaves it alone.
    let settled = editor.revision();
    send(&mut editor, mouse, Up, (20.0, 20.0), Modifiers::default());
    editor.set_tool(Tool::Hand);
    send(
        &mut editor,
        (3, Finger),
        Down,
        (0.0, 0.0),
        Modifiers::default(),
    );
    send(
        &mut editor,
        (3, Finger),
        Move,
        (5.0, 5.0),
        Modifiers::default(),
    );
    assert_eq!(editor.revision(), settled);
}

#[test]
fn a_dropped_shape_keeps_the_generation_because_its_handle_is_never_reused() {
    let mut editor = editor_with(Tool::Rectangle);
    let finger = (1, Finger);
    send(&mut editor, finger, Down, (0.0, 0.0), Modifiers::default());
    let while_drawing = editor.geometry().generation();

    send(
        &mut editor,
        finger,
        Cancel,
        (0.0, 0.0),
        Modifiers::default(),
    );

    // The element was removed; handles are never reused, so no cache of the
    // handle can stand for the next element.
    assert_eq!(editor.geometry().generation(), while_drawing);
}

// --- The Selection after drawing -----------------------------------------------

/// The ids of the saved Elements, bottom first.
fn ids(editor: &Editor) -> Vec<String> {
    elements(editor)
        .iter()
        .map(|e| text(e, "id").to_owned())
        .collect()
}

#[test]
fn the_drawn_shape_becomes_the_selection_so_it_can_be_adjusted_right_away() {
    let mut editor = editor_with(Tool::Rectangle);

    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));

    assert_eq!(editor.selection(), ids(&editor));
}

#[test]
fn drawing_selects_the_drawn_shape_alone() {
    let mut editor = editor_with(Tool::Rectangle);
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    let first = ids(&editor)[0].clone();
    assert_eq!(editor.selection(), std::slice::from_ref(&first));

    editor.set_tool(Tool::Ellipse);
    assert!(editor.selection().is_empty());
    drag(&mut editor, (100.0, 0.0), (150.0, 50.0));

    let second = ids(&editor)[1].clone();
    assert_eq!(editor.selection(), [second]);
    // Choosing the Tool cleared the Selection, so undoing the shape goes
    // back to an empty one.
    editor.undo();
    assert!(editor.selection().is_empty());
}

#[test]
fn a_tap_or_a_cancelled_drawing_selects_nothing() {
    let mut editor = editor_with(Tool::Rectangle);
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    editor.set_tool(Tool::Rectangle);

    let mouse = (0, Mouse);
    let none = Modifiers::default();
    send(&mut editor, mouse, Down, (100.0, 0.0), none);
    send(&mut editor, mouse, Up, (100.0, 0.0), none);
    assert!(editor.selection().is_empty());

    send(&mut editor, mouse, Down, (100.0, 0.0), none);
    send(&mut editor, mouse, Move, (150.0, 50.0), none);
    send(&mut editor, mouse, Cancel, (150.0, 50.0), none);
    assert!(editor.selection().is_empty());
}

#[test]
fn with_the_tool_lock_on_a_drawn_shape_is_not_selected() {
    let mut editor = editor_with(Tool::Rectangle);
    drag(&mut editor, (0.0, 0.0), (50.0, 50.0));
    editor.set_tool_lock(true);
    editor.set_tool(Tool::Diamond);

    drag(&mut editor, (100.0, 0.0), (150.0, 50.0));
    drag(&mut editor, (200.0, 0.0), (250.0, 50.0));

    // As on excalidraw.com: the next shape follows right away, and a
    // Selection would only be in its way.
    assert!(editor.selection().is_empty());
    assert_eq!(editor.tool(), Tool::Diamond);
}
