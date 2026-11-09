//! Drawing with the freedraw Tool (ADR-0004, ADR-0006), tested at the Editor
//! seam with recorded pointer sequences: pointer events in, saved JSON, Tool
//! and geometry out. Expected values are worked out by hand from Excalidraw's
//! `App.tsx` (pointer down, move and up of a freedraw element).

use pkm_core::json::Value;
use pkm_core::{Editor, Modifiers, PointerEvent, PointerKind, PointerPhase, Tool};

use PointerKind::{Finger, Mouse, Pencil};
use PointerPhase::{Cancel, Down, Move, Up};

use crate::common;
use common::{num, saved_elements, saved_points, text};

fn editor_with(tool: Tool) -> Editor {
    let mut editor = Editor::new();
    editor.set_tool(tool);
    editor
}

/// One step of pointer `id` of `kind` at `(x, y)` with `pressure`.
fn send(
    editor: &mut Editor,
    (id, kind): (u64, PointerKind),
    phase: PointerPhase,
    (x, y): (f64, f64),
    pressure: f32,
) {
    editor.pointer(PointerEvent {
        id,
        phase,
        x,
        y,
        kind,
        button: pkm_core::PointerButton::Primary,
        pressure,
        timestamp: 0.0,
        modifiers: Modifiers::default(),
    });
}

/// A stroke of `pointer` along `path` (the first point is where it goes down,
/// the last where it lifts), each point with the pressure alongside.
fn stroke(editor: &mut Editor, pointer: (u64, PointerKind), path: &[((f64, f64), f32)]) {
    let (&(first, p), rest) = path.split_first().unwrap();
    send(editor, pointer, Down, first, p);
    let (&(last, p), middle) = rest.split_last().unwrap();
    for &(at, pressure) in middle {
        send(editor, pointer, Move, at, pressure);
    }
    send(editor, pointer, Up, last, p);
}

/// A stroke of `pointer` along `path` with the pressure `0.5` that devices
/// without pressure report.
fn flat_stroke(editor: &mut Editor, pointer: (u64, PointerKind), path: &[(f64, f64)]) {
    let path: Vec<_> = path.iter().map(|&at| (at, 0.5)).collect();
    stroke(editor, pointer, &path);
}

fn pressures(element: &Value) -> Vec<f64> {
    element
        .get("pressures")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|p| p.as_f64().unwrap())
        .collect()
}

fn flag(element: &Value, key: &str) -> bool {
    element.get(key).and_then(Value::as_bool).unwrap()
}

fn streamline(element: &Value) -> f64 {
    num(element.get("strokeOptions").unwrap(), "streamline")
}

// --- What a stroke becomes -------------------------------------------------

#[test]
fn dragging_with_the_freedraw_tool_draws_a_stroke_through_the_points() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(
        &mut editor,
        (0, Mouse),
        &[(10.0, 20.0), (15.0, 30.0), (25.0, 25.0), (40.0, 5.0)],
    );

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    let stroke = &elements[0];
    assert_eq!(text(stroke, "type"), "freedraw");
    assert_eq!((num(stroke, "x"), num(stroke, "y")), (10.0, 20.0));
    // Points are relative to where the pointer went down; the point where it
    // lifts is the last.
    assert_eq!(
        saved_points(stroke),
        [(0.0, 0.0), (5.0, 10.0), (15.0, 5.0), (30.0, -15.0)]
    );
    // The size is that of the points, the first included: x from 0 to 30,
    // y from -15 to 10.
    assert_eq!((num(stroke, "width"), num(stroke, "height")), (30.0, 25.0));
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn a_stroke_has_the_style_excalidraw_gives_it() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);

    let stroke = &saved_elements(&editor)[0];
    assert_eq!(text(stroke, "strokeColor"), "#1e1e1e");
    assert_eq!(text(stroke, "backgroundColor"), "transparent");
    // Excalidraw draws freedraw half as wide as other Elements for the same
    // stroke width setting: "medium" is 2 for them, 1 for it.
    assert_eq!(num(stroke, "strokeWidth"), 1.0);
    assert_eq!(num(stroke, "opacity"), 100.0);
    assert!(stroke.get("roundness").is_some_and(|r| *r == Value::Null));
    assert_eq!(
        stroke
            .get("strokeOptions")
            .and_then(|o| o.get("variability"))
            .and_then(Value::as_str),
        Some("variable")
    );
    assert!(!flag(stroke, "isDeleted"));
}

#[test]
fn a_stroke_has_the_members_and_order_of_excalidraws_own() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);

    let saved = String::from_utf8(editor.save()).unwrap();
    let saved = &saved[saved.find("\"elements\"").unwrap()..];
    let keys = [
        "\"id\"",
        "\"type\"",
        "\"x\"",
        "\"locked\"",
        "\"points\"",
        "\"pressures\"",
        "\"simulatePressure\"",
        "\"strokeOptions\"",
    ];
    let positions: Vec<usize> = keys.iter().map(|k| saved.find(k).expect(k)).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{saved}");
}

#[test]
fn every_stroke_gets_its_own_seed_and_id() {
    let mut editor = editor_with(Tool::Freedraw);
    editor.set_tool_lock(true);

    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);
    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);

    let elements = saved_elements(&editor);
    assert_ne!(text(&elements[0], "id"), text(&elements[1], "id"));
    assert_ne!(num(&elements[0], "seed"), num(&elements[1], "seed"));
}

#[test]
fn positions_are_in_scene_coordinates_whatever_the_viewport() {
    let mut editor = editor_with(Tool::Freedraw);
    // Pan by two fingers, away from where the stroke is drawn.
    send(&mut editor, (7, Finger), Down, (100.0, 100.0), 0.5);
    send(&mut editor, (8, Finger), Down, (200.0, 100.0), 0.5);
    send(&mut editor, (7, Finger), Move, (150.0, 130.0), 0.5);
    send(&mut editor, (8, Finger), Move, (250.0, 130.0), 0.5);
    send(&mut editor, (7, Finger), Up, (150.0, 130.0), 0.5);
    send(&mut editor, (8, Finger), Up, (250.0, 130.0), 0.5);
    let origin = editor.viewport().origin();
    assert_eq!(origin, (-50.0, -30.0));

    flat_stroke(&mut editor, (0, Mouse), &[(10.0, 10.0), (20.0, 20.0)]);

    let stroke = &saved_elements(&editor)[0];
    assert_eq!((num(stroke, "x"), num(stroke, "y")), (-40.0, -20.0));
    assert_eq!(saved_points(stroke), [(0.0, 0.0), (10.0, 10.0)]);
}

#[test]
fn a_move_that_stays_where_the_last_one_was_adds_no_point() {
    let mut editor = editor_with(Tool::Freedraw);
    let mouse = (0, Mouse);

    send(&mut editor, mouse, Down, (0.0, 0.0), 0.5);
    send(&mut editor, mouse, Move, (5.0, 5.0), 0.5);
    send(&mut editor, mouse, Move, (5.0, 5.0), 0.5);
    send(&mut editor, mouse, Move, (9.0, 5.0), 0.5);
    send(&mut editor, mouse, Up, (9.0, 5.0), 0.5);

    // The point where the pointer lifts is always added, as Excalidraw does.
    assert_eq!(
        saved_points(&saved_elements(&editor)[0]),
        [(0.0, 0.0), (5.0, 5.0), (9.0, 5.0), (9.0, 5.0)]
    );
}

#[test]
fn a_tap_draws_a_dot() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(&mut editor, (0, Mouse), &[(30.0, 40.0), (30.0, 40.0)]);

    // Excalidraw nudges the end of a dot by 0.0001 so that it is not
    // "invisibly small".
    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    assert_eq!(saved_points(&elements[0]), [(0.0, 0.0), (0.0001, 0.0001)]);
    assert!(!editor.geometry().parts().is_empty(), "a dot is drawn");
}

// --- Pressure --------------------------------------------------------------

#[test]
fn a_pencil_stroke_keeps_the_pressure_of_every_point() {
    let mut editor = editor_with(Tool::Freedraw);

    stroke(
        &mut editor,
        (0, Pencil),
        &[
            ((0.0, 0.0), 0.1),
            ((10.0, 0.0), 0.4),
            ((20.0, 0.0), 0.9),
            ((30.0, 0.0), 0.3),
        ],
    );

    let stroke = &saved_elements(&editor)[0];
    assert!(!flag(stroke, "simulatePressure"));
    // One for each point: the one where the Pencil lifts has its own.
    assert_eq!(pressures(stroke), [0.1, 0.4, 0.9, 0.3]);
    assert_eq!(saved_points(stroke).len(), 4);
}

#[test]
fn a_pencil_stroke_does_not_take_a_pressure_of_one_half_for_missing() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(&mut editor, (0, Pencil), &[(0.0, 0.0), (10.0, 0.0)]);

    let stroke = &saved_elements(&editor)[0];
    assert!(!flag(stroke, "simulatePressure"));
    assert_eq!(pressures(stroke), [0.5, 0.5]);
}

#[test]
fn the_pressure_of_a_pencil_changes_how_thick_the_stroke_is() {
    let outline_of = |pressure: f32| {
        let mut editor = editor_with(Tool::Freedraw);
        let path: Vec<_> = (0..20)
            .map(|i| ((f64::from(i) * 10.0, 0.0), pressure))
            .collect();
        stroke(&mut editor, (0, Pencil), &path);
        let geometry = editor.geometry();
        let [part] = geometry.parts() else {
            panic!("a stroke is one part")
        };
        let ys: Vec<f32> = common::ops(&geometry, part)
            .into_iter()
            .flat_map(|(_, operands)| operands.chunks(2).map(|c| c[1]).collect::<Vec<_>>())
            .collect();
        let (min, max) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &y| (lo.min(y), hi.max(y)));
        max - min
    };

    let (light, firm) = (outline_of(0.1), outline_of(0.9));

    assert!(light > 0.0);
    assert!(firm > light * 2.0, "{firm} vs {light}");
}

#[test]
fn finger_and_mouse_strokes_simulate_pressure_whatever_the_device_reports() {
    for (kind, pressure) in [(Finger, 0.5), (Finger, 0.9), (Mouse, 0.5), (Mouse, 0.0)] {
        let mut editor = editor_with(Tool::Freedraw);

        stroke(
            &mut editor,
            (1, kind),
            &[
                ((0.0, 0.0), pressure),
                ((10.0, 5.0), pressure),
                ((20.0, 0.0), pressure),
            ],
        );

        let stroke = &saved_elements(&editor)[0];
        assert!(flag(stroke, "simulatePressure"), "{kind:?} {pressure}");
        assert!(pressures(stroke).is_empty(), "{kind:?} {pressure}");
    }
}

#[test]
fn the_pencil_is_smoothed_less_than_the_mouse() {
    // Excalidraw's `DEFAULT_STROKE_STREAMLINE_PRECISE` and
    // `DEFAULT_STROKE_STREAMLINE`.
    for (kind, expected) in [(Mouse, 0.5), (Pencil, 0.2), (Finger, 0.2)] {
        let mut editor = editor_with(Tool::Freedraw);

        flat_stroke(&mut editor, (1, kind), &[(0.0, 0.0), (10.0, 5.0)]);

        assert_eq!(
            streamline(&saved_elements(&editor)[0]),
            expected,
            "{kind:?}"
        );
    }
}

// --- The Tool --------------------------------------------------------------

#[test]
fn the_tool_returns_to_selection_after_a_stroke() {
    let mut editor = editor_with(Tool::Freedraw);

    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);

    assert_eq!(editor.tool(), Tool::Selection);
    assert_eq!(editor.element_count(), 1);
}

#[test]
fn the_tool_stays_after_each_stroke_while_the_tool_lock_is_on() {
    let mut editor = editor_with(Tool::Freedraw);
    editor.set_tool_lock(true);

    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (10.0, 10.0)]);
    assert_eq!(editor.tool(), Tool::Freedraw);
    flat_stroke(&mut editor, (0, Pencil), &[(50.0, 0.0), (60.0, 10.0)]);

    assert_eq!(editor.tool(), Tool::Freedraw);
    assert_eq!(saved_elements(&editor).len(), 2);
    assert_eq!(editor.element_count(), 2);
}

#[test]
fn a_stroke_clears_the_selection() {
    let mut editor = Editor::new();
    editor.set_tool(Tool::Rectangle);
    send(&mut editor, (0, Mouse), Down, (0.0, 0.0), 0.5);
    send(&mut editor, (0, Mouse), Move, (50.0, 50.0), 0.5);
    send(&mut editor, (0, Mouse), Up, (50.0, 50.0), 0.5);
    send(&mut editor, (0, Mouse), Down, (50.0, 0.0), 0.5);
    send(&mut editor, (0, Mouse), Up, (50.0, 0.0), 0.5);
    editor.select_all();
    assert_eq!(editor.selection().len(), 1);

    editor.set_tool(Tool::Freedraw);
    flat_stroke(&mut editor, (0, Mouse), &[(100.0, 100.0), (120.0, 130.0)]);

    assert!(editor.selection().is_empty());
}

#[test]
fn choosing_another_tool_mid_stroke_drops_the_stroke() {
    let mut editor = editor_with(Tool::Freedraw);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), 0.5);
    send(&mut editor, mouse, Move, (10.0, 10.0), 0.5);

    editor.set_tool(Tool::Rectangle);
    send(&mut editor, mouse, Move, (20.0, 20.0), 0.5);
    send(&mut editor, mouse, Up, (20.0, 20.0), 0.5);

    assert!(editor.save().windows(9).all(|w| w != b"freedraw\""));
    assert_eq!(editor.element_count(), 0);
}

#[test]
fn other_tools_do_not_draw_strokes() {
    for tool in [Tool::Selection, Tool::Hand, Tool::Rectangle] {
        let mut editor = editor_with(tool);

        flat_stroke(
            &mut editor,
            (0, Mouse),
            &[(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)],
        );

        assert!(
            saved_elements(&editor)
                .iter()
                .all(|e| text(e, "type") != "freedraw"),
            "{tool:?}"
        );
    }
}

// --- Interruptions ---------------------------------------------------------

#[test]
fn a_cancelled_pointer_removes_the_stroke_and_keeps_the_tool() {
    let mut editor = editor_with(Tool::Freedraw);
    let finger = (1, Finger);
    send(&mut editor, finger, Down, (0.0, 0.0), 0.5);
    send(&mut editor, finger, Move, (10.0, 10.0), 0.5);
    let while_drawing = editor.geometry().generation();

    send(&mut editor, finger, Cancel, (10.0, 10.0), 0.5);

    assert_eq!(editor.element_count(), 0);
    assert_eq!(editor.tool(), Tool::Freedraw);
    // Handles are never reused, so caches of the removed one stay valid.
    assert_eq!(editor.geometry().generation(), while_drawing);
}

#[test]
fn a_second_finger_early_in_a_stroke_drops_it_and_pinches() {
    let mut editor = editor_with(Tool::Freedraw);
    send(&mut editor, (1, Finger), Down, (100.0, 100.0), 0.5);
    send(&mut editor, (1, Finger), Move, (105.0, 100.0), 0.5);
    send(&mut editor, (1, Finger), Move, (110.0, 100.0), 0.5);

    send(&mut editor, (2, Finger), Down, (200.0, 100.0), 0.5);
    send(&mut editor, (2, Finger), Move, (300.0, 100.0), 0.5);
    send(&mut editor, (1, Finger), Move, (0.0, 100.0), 0.5);
    send(&mut editor, (2, Finger), Up, (300.0, 100.0), 0.5);
    send(&mut editor, (1, Finger), Up, (0.0, 100.0), 0.5);

    assert_eq!(editor.element_count(), 0);
    assert_eq!(editor.tool(), Tool::Freedraw);
    assert!(editor.viewport().zoom() > 1.0, "the two fingers pinch");
}

#[test]
fn a_second_finger_late_in_a_stroke_ends_it_where_it_is() {
    let mut editor = editor_with(Tool::Freedraw);
    send(&mut editor, (1, Finger), Down, (0.0, 0.0), 0.5);
    for i in 1..=12 {
        send(
            &mut editor,
            (1, Finger),
            Move,
            (f64::from(i) * 5.0, 0.0),
            0.5,
        );
    }

    send(&mut editor, (2, Finger), Down, (200.0, 100.0), 0.5);
    send(&mut editor, (1, Finger), Move, (80.0, 0.0), 0.5);
    send(&mut editor, (1, Finger), Up, (80.0, 0.0), 0.5);
    send(&mut editor, (2, Finger), Up, (200.0, 100.0), 0.5);

    // Excalidraw keeps a stroke of ten points or more; the pinch added none.
    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    assert_eq!(saved_points(&elements[0]).len(), 13);
    assert_eq!(editor.tool(), Tool::Freedraw);
    assert!(editor.can_undo());
}

#[test]
fn a_finger_landing_while_two_are_down_does_not_start_a_stroke() {
    let mut editor = editor_with(Tool::Freedraw);
    send(&mut editor, (1, Finger), Down, (0.0, 0.0), 0.5);
    send(&mut editor, (2, Finger), Down, (50.0, 0.0), 0.5);

    send(&mut editor, (3, Finger), Down, (100.0, 0.0), 0.5);
    send(&mut editor, (3, Finger), Move, (150.0, 50.0), 0.5);
    send(&mut editor, (3, Finger), Up, (150.0, 50.0), 0.5);

    assert_eq!(editor.element_count(), 0);
}

#[test]
fn other_pointers_do_not_disturb_the_stroke() {
    let mut editor = editor_with(Tool::Freedraw);
    send(&mut editor, (1, Pencil), Down, (0.0, 0.0), 0.3);
    send(&mut editor, (1, Pencil), Move, (10.0, 0.0), 0.3);

    // A mouse button cannot go down while the Pencil is; if it does, it is
    // ignored.
    send(&mut editor, (2, Mouse), Down, (50.0, 50.0), 0.5);
    send(&mut editor, (2, Mouse), Move, (60.0, 60.0), 0.5);
    send(&mut editor, (2, Mouse), Up, (60.0, 60.0), 0.5);
    send(&mut editor, (1, Pencil), Up, (20.0, 0.0), 0.3);

    let elements = saved_elements(&editor);
    assert_eq!(elements.len(), 1);
    assert_eq!(
        saved_points(&elements[0]),
        [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)]
    );
}

#[test]
fn loading_a_drawing_mid_stroke_drops_the_stroke() {
    let mut editor = editor_with(Tool::Freedraw);
    send(&mut editor, (0, Mouse), Down, (0.0, 0.0), 0.5);
    send(&mut editor, (0, Mouse), Move, (10.0, 10.0), 0.5);

    editor
        .load(br#"{"type":"excalidraw","version":2,"elements":[]}"#)
        .unwrap();
    send(&mut editor, (0, Mouse), Move, (20.0, 20.0), 0.5);
    send(&mut editor, (0, Mouse), Up, (20.0, 20.0), 0.5);

    assert_eq!(editor.element_count(), 0);
}

// --- Undo ------------------------------------------------------------------

#[test]
fn a_stroke_is_one_step_of_undo_and_redo() {
    let mut editor = editor_with(Tool::Freedraw);
    editor.set_tool_lock(true);
    flat_stroke(
        &mut editor,
        (0, Mouse),
        &[(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)],
    );
    flat_stroke(
        &mut editor,
        (0, Mouse),
        &[(50.0, 0.0), (55.0, 5.0), (60.0, 0.0)],
    );

    editor.undo();
    assert_eq!(editor.element_count(), 1);
    editor.undo();
    assert_eq!(editor.element_count(), 0);
    assert!(!editor.can_undo());

    editor.redo();
    editor.redo();
    assert_eq!(editor.element_count(), 2);
    assert_eq!(editor.tool(), Tool::Freedraw);
    // The redone strokes are complete.
    let elements = saved_elements(&editor);
    assert_eq!(saved_points(&elements[0]).len(), 3);
    assert_eq!(saved_points(&elements[1]).len(), 3);
}

#[test]
fn undo_does_nothing_while_a_stroke_is_being_drawn() {
    let mut editor = editor_with(Tool::Freedraw);
    editor.set_tool_lock(true);
    flat_stroke(&mut editor, (0, Mouse), &[(0.0, 0.0), (5.0, 5.0)]);
    send(&mut editor, (0, Mouse), Down, (50.0, 0.0), 0.5);
    send(&mut editor, (0, Mouse), Move, (55.0, 5.0), 0.5);

    editor.undo();

    assert_eq!(editor.element_count(), 2);
}

// --- Drawing while the pointer is down -------------------------------------

#[test]
fn the_stroke_is_drawn_as_it_grows() {
    let mut editor = editor_with(Tool::Freedraw);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), 0.5);
    let mut reach = Vec::new();

    for i in 1..=5 {
        send(&mut editor, mouse, Move, (f64::from(i) * 20.0, 10.0), 0.5);
        let geometry = editor.geometry();
        assert_eq!(geometry.parts().len(), 1);
        let xs = geometry.coords().chunks(2).map(|c| c[0]);
        reach.push(xs.fold(f32::MIN, f32::max));
    }

    // The stroke reaches the pointer, which moves on with every step.
    assert!(
        reach.windows(2).all(|pair| pair[1] > pair[0]),
        "every move extends the outline: {reach:?}"
    );
}

#[test]
fn every_point_tells_clients_to_draw_again_without_reassigning_handles() {
    let mut editor = editor_with(Tool::Freedraw);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), 0.5);
    let generation = editor.geometry().generation();
    let mut revisions = vec![editor.revision()];
    let mut versions = vec![num(&saved_elements(&editor)[0], "version")];

    for i in 1..=3 {
        send(&mut editor, mouse, Move, (f64::from(i) * 10.0, 0.0), 0.5);
        revisions.push(editor.revision());
        versions.push(num(&saved_elements(&editor)[0], "version"));
    }

    assert!(revisions.windows(2).all(|w| w[0] != w[1]));
    assert!(versions.windows(2).all(|w| w[0] < w[1]));
    // Clients may keep what they cached for other Elements.
    assert_eq!(editor.geometry().generation(), generation);
}

#[test]
fn a_long_stroke_stays_cheap_to_extend() {
    let mut editor = editor_with(Tool::Freedraw);
    let mouse = (0, Mouse);
    send(&mut editor, mouse, Down, (0.0, 0.0), 0.5);

    for i in 1..=20_000 {
        let x = f64::from(i);
        send(&mut editor, mouse, Move, (x, (x / 7.0).sin() * 50.0), 0.5);
    }
    send(&mut editor, mouse, Up, (20_000.0, 0.0), 0.5);

    assert_eq!(saved_points(&saved_elements(&editor)[0]).len(), 20_002);
}
