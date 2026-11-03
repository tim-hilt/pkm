//! Benchmark Scenes (`benchmarks/`) through the `Editor` seam.
//!
//! The always-on test keeps the Scenes loadable; the ignored one measures the
//! core's share of the "open 1,000 Elements in < 200 ms" target. Run it with
//! `cargo test --release -p workingtitle-core --test benchmarks -- --ignored --nocapture`
//! (procedure: docs/performance-benchmarks.md).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use workingtitle_core::Editor;

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
