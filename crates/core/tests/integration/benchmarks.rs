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
