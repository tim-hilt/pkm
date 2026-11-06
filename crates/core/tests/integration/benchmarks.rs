//! Benchmark Scenes (`benchmarks/`) through the `Editor` seam.
//!
//! The always-on test keeps the Scenes loadable; the ignored ones measure the
//! core's share of the "open 1,000 Elements in < 200 ms" target and of a
//! frame at 120 fps with 5,000 Elements. Run them with `make bench`
//! (procedure: docs/performance-benchmarks.md).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use workingtitle_core::{Editor, Modifiers, PointerEvent, PointerKind, PointerPhase};

fn benchmark_scene(count: u64) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks")
        .join(format!("scene-{count}.excalidraw"));
    fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Loads the Scene and builds its geometry, as opening a Drawing does.
fn open(bytes: &[u8]) -> Editor {
    let mut editor = Editor::new();
    editor.load(bytes).expect("Benchmark Scene should load");
    editor.geometry();
    editor
}

#[test]
fn benchmark_scenes_load_with_their_element_counts() {
    for count in [1000, 5000] {
        let editor = open(&benchmark_scene(count));
        assert_eq!(editor.element_count(), count);
    }
}

#[test]
#[ignore = "measurement; run in release, see docs/performance-benchmarks.md"]
fn opening_1000_elements_takes_under_200ms() {
    let bytes = benchmark_scene(1000);
    let runs = 10;
    let mut best = Duration::MAX;
    for _ in 0..runs {
        let start = Instant::now();
        open(&bytes);
        best = best.min(start.elapsed());
    }
    println!("open scene-1000 (core, best of {runs}): {best:?}");
    assert!(best < Duration::from_millis(200), "took {best:?}");
}

/// What a client asks the editor after every input event (`CanvasMetalView`
/// and `CanvasController` do exactly this): the core's whole share of one
/// event, apart from `geometry()` when the Scene changed (issue 56).
fn client_refresh(editor: &Editor, overlay_revision: &mut u64) {
    let _ = (editor.revision(), editor.viewport(), editor.text_editor());
    if editor.overlay_revision() != *overlay_revision {
        *overlay_revision = editor.overlay_revision();
        editor.overlay();
    }
    let _ = (
        editor.tool(),
        editor.tool_lock(),
        editor.pencil_only_drawing(),
    );
    let _ = (editor.can_undo(), editor.can_redo(), editor.action_bar());
    if editor.shows_properties() {
        editor.properties();
    }
}

fn pointer(
    id: u64,
    phase: PointerPhase,
    (x, y): (f64, f64),
    kind: PointerKind,
    t: f64,
) -> PointerEvent {
    PointerEvent {
        id,
        phase,
        x,
        y,
        kind,
        button: workingtitle_core::PointerButton::Primary,
        pressure: 0.5,
        timestamp: t,
        modifiers: Modifiers::default(),
    }
}

/// The slowest of `steps` input events (each one `event(step)`, then the
/// client's refresh) in `scene-5000`, zoomed to fit in an iPad Pro sized view.
fn slowest_step(
    prepare: impl Fn(&mut Editor),
    steps: usize,
    event: impl Fn(&mut Editor, usize),
) -> Duration {
    let mut editor = open(&benchmark_scene(5000));
    editor.set_view_size(1024.0, 1366.0);
    editor.zoom_to_fit();
    prepare(&mut editor);
    let mut overlay_revision = u64::MAX;
    let mut slowest = Duration::ZERO;
    for step in 0..steps {
        let start = Instant::now();
        event(&mut editor, step);
        client_refresh(&editor, &mut overlay_revision);
        slowest = slowest.max(start.elapsed());
    }
    slowest
}

#[test]
#[ignore = "measurement; run in release, see docs/performance-benchmarks.md"]
fn input_events_in_5000_elements_take_under_a_frame() {
    use PointerKind::{Finger, Mouse};
    use PointerPhase::{Down, Move, Up};
    let frame = Duration::from_micros(8_333);

    // A pinch with one Element selected: the Selection's overlay and Action
    // Bar follow every step.
    let pinch = slowest_step(
        |editor| {
            editor.pointer(pointer(1, Down, (500.0, 600.0), Mouse, 0.0));
            editor.pointer(pointer(1, Up, (500.0, 600.0), Mouse, 0.05));
            editor.pointer(pointer(1, Down, (400.0, 600.0), Finger, 1.0));
            editor.pointer(pointer(2, Down, (600.0, 600.0), Finger, 1.0));
        },
        200,
        |editor, step| {
            let spread = 100.0 + step as f64;
            let t = 1.0 + step as f64 / 120.0;
            editor.pointer(pointer(1, Move, (500.0 - spread, 600.0), Finger, t));
            editor.pointer(pointer(2, Move, (500.0 + spread, 600.0), Finger, t));
        },
    );
    println!("pinch step, scene-5000 (core + client queries, slowest of 200): {pinch:?}");

    // A mouse box-select across the whole Scene: the Selection grows to all
    // 5,000 Elements on the way.
    let box_select = slowest_step(
        |editor| editor.pointer(pointer(1, Down, (1.0, 1.0), Mouse, 0.0)),
        100,
        |editor, step| {
            let corner = 10.0 + step as f64 * 13.6;
            editor.pointer(pointer(
                1,
                Move,
                (corner * 0.75, corner),
                Mouse,
                step as f64 / 120.0,
            ));
        },
    );
    println!("box-select step, scene-5000 (core + client queries, slowest of 100): {box_select:?}");

    assert!(pinch < frame, "pinch step took {pinch:?}");
    assert!(box_select < frame, "box-select step took {box_select:?}");
}

/// `Editor::geometry()` after a one-Element change in `scene-5000`: what a
/// client asks for after every pointer move while dragging (issue 56).
#[test]
#[ignore = "measurement; run in release, see docs/performance-benchmarks.md"]
fn geometry_after_a_one_element_change_in_5000_elements_takes_well_under_a_frame() {
    use PointerKind::Mouse;
    use PointerPhase::{Down, Move, Up};
    let mut editor = open(&benchmark_scene(5000));
    editor.set_view_size(1024.0, 1366.0);
    editor.zoom_to_fit();
    editor.pointer(pointer(1, Down, (500.0, 600.0), Mouse, 0.0));
    editor.pointer(pointer(1, Up, (500.0, 600.0), Mouse, 0.05));
    editor.pointer(pointer(1, Down, (500.0, 600.0), Mouse, 1.0));

    let steps = 100;
    let mut slowest = Duration::ZERO;
    let mut total = Duration::ZERO;
    for step in 0..steps {
        let t = 1.0 + step as f64 / 120.0;
        editor.pointer(pointer(1, Move, (500.0 + step as f64, 600.0), Mouse, t));
        let start = Instant::now();
        let geometry = editor.geometry();
        let took = start.elapsed();
        assert!(!geometry.parts().is_empty());
        slowest = slowest.max(took);
        total += took;
    }
    println!(
        "geometry() after a drag step, scene-5000 (mean {:?}, slowest of {steps}): {slowest:?}",
        total / steps
    );
    assert!(slowest < Duration::from_millis(4), "took {slowest:?}");
}

/// The scene bounds of every Element as saved: `(min x, min y, max x, max y)`.
fn scene_bounds(editor: &Editor) -> (f64, f64, f64, f64) {
    use workingtitle_core::json::{Value, parse};
    let drawing = parse(&String::from_utf8(editor.save()).unwrap()).unwrap();
    let number = |e: &Value, key: &str| e.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for e in drawing.get("elements").and_then(Value::as_array).unwrap() {
        let (x, y) = (number(e, "x"), number(e, "y"));
        let (w, h) = (number(e, "width"), number(e, "height"));
        bounds = (
            bounds.0.min(x).min(x + w),
            bounds.1.min(y).min(y + h),
            bounds.2.max(x).max(x + w),
            bounds.3.max(y).max(y + h),
        );
    }
    bounds
}

/// Where the Selection's south-east and rotation handles are in the view:
/// the corner and the middle of the top side, 4 and 20 points out.
fn handle_positions(editor: &Editor) -> ((f64, f64), (f64, f64)) {
    let (min_x, min_y, max_x, max_y) = scene_bounds(editor);
    let viewport = editor.viewport();
    let (origin, zoom) = (viewport.origin(), viewport.zoom());
    let view = |x: f64, y: f64| ((x - origin.0) * zoom, (y - origin.1) * zoom);
    let corner = view(max_x, max_y);
    let top = view((min_x + max_x) / 2.0, min_y);
    ((corner.0 + 4.0, corner.1 + 4.0), (top.0, top.1 - 20.0))
}

/// How long the steps of a gesture and its undo take, in the core and with
/// the client's queries after each.
struct GestureTimes {
    /// The step that turns the press into a drag: it also sets the gesture
    /// up (what each Element was, for the transform and the undo), once.
    start: Duration,
    /// The slowest of the steps after it.
    steady: Duration,
    undo: Duration,
}

/// Select-all, then a gesture of `steps` pointer steps from the view point
/// that `grab` picks, each moving the pointer by `travel(step)` from there.
fn select_all_gesture(
    grab: impl Fn(&Editor) -> (f64, f64),
    travel: impl Fn(usize) -> (f64, f64),
    steps: usize,
) -> GestureTimes {
    use PointerKind::Mouse;
    use PointerPhase::{Down, Move, Up};
    let mut editor = open(&benchmark_scene(5000));
    editor.set_view_size(1024.0, 1366.0);
    editor.zoom_to_fit();
    editor.select_all();
    let mut overlay_revision = u64::MAX;
    client_refresh(&editor, &mut overlay_revision);
    let before = editor.save();
    let bounds = scene_bounds(&editor);
    let (x, y) = grab(&editor);
    editor.pointer(pointer(1, Down, (x, y), Mouse, 0.0));
    let mut timed = Vec::new();
    let mut started = None;
    for step in 0..steps {
        let (dx, dy) = travel(step);
        let revision = editor.revision();
        let start = Instant::now();
        editor.pointer(pointer(
            1,
            Move,
            (x + dx, y + dy),
            Mouse,
            (step + 1) as f64 / 120.0,
        ));
        client_refresh(&editor, &mut overlay_revision);
        timed.push(start.elapsed());
        if started.is_none() && editor.revision() != revision {
            started = Some(step);
        }
    }
    let started = started.expect("the gesture never began");
    let (dx, dy) = travel(steps - 1);
    editor.pointer(pointer(1, Up, (x + dx, y + dy), Mouse, 1.0));
    assert_ne!(editor.save(), before, "the gesture changed nothing");
    assert!(editor.can_undo());
    let start = Instant::now();
    editor.undo();
    client_refresh(&editor, &mut overlay_revision);
    let undo = start.elapsed();
    assert_eq!(
        scene_bounds(&editor),
        bounds,
        "undo did not restore the Scene"
    );
    GestureTimes {
        start: timed[started],
        steady: timed[started + 1..]
            .iter()
            .copied()
            .max()
            .unwrap_or_default(),
        undo,
    }
}

#[test]
#[ignore = "measurement; run in release, see docs/performance-benchmarks.md"]
fn gestures_on_a_selection_of_5000_elements_take_under_a_frame_per_step() {
    let frame = Duration::from_micros(8_333);

    // A press on the corner of the Selection's box that no Element covers
    // is as good as one on an Element: inside the box, a drag moves.
    let moved = select_all_gesture(
        |editor| {
            let (min_x, min_y, max_x, max_y) = scene_bounds(editor);
            let viewport = editor.viewport();
            let (origin, zoom) = (viewport.origin(), viewport.zoom());
            (
                ((min_x + max_x) / 2.0 - origin.0) * zoom,
                ((min_y + max_y) / 2.0 - origin.1) * zoom,
            )
        },
        |step| (step as f64 + 1.0, step as f64 + 1.0),
        6,
    );
    let resized = select_all_gesture(
        |editor| handle_positions(editor).0,
        |step| (-(step as f64) - 1.0, -(step as f64) - 1.0),
        6,
    );
    let rotated = select_all_gesture(
        |editor| handle_positions(editor).1,
        |step| (step as f64 * 2.0 + 5.0, 0.0),
        6,
    );

    for (name, times) in [("move", &moved), ("resize", &resized), ("rotate", &rotated)] {
        println!(
            "select-all {name}, scene-5000: start {:?}, later steps (slowest) {:?}, undo {:?}",
            times.start, times.steady, times.undo
        );
    }
    for (name, times) in [("move", moved), ("resize", resized), ("rotate", rotated)] {
        // Setting a gesture up takes at most two frames, once.
        assert!(
            times.start < frame * 2,
            "{name} start took {:?}",
            times.start
        );
        assert!(times.steady < frame, "{name} step took {:?}", times.steady);
        assert!(
            times.undo < Duration::from_millis(100),
            "{name} undo took {:?}",
            times.undo
        );
    }
}
