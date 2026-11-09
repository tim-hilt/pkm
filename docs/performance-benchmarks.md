# Performance benchmarks

The spec's performance targets are measured, not blocking. This page lists the
Benchmark Scenes, how to measure each target, and the latest results.

## Benchmark Scenes

`benchmarks/scene-1000.excalidraw` and `benchmarks/scene-5000.excalidraw` are
1,000 and 5,000 Elements on a 100-column grid, cycling rectangle, ellipse,
diamond, line, arrow, freedraw (24 points) and text, with hachure,
cross-hatch and solid fills and both roughness 0 and 1. They're generated
deterministically; regenerate them with `node benchmarks/generate.mjs` (Node is
needed only for this, not for `cargo test`).

`cargo test` checks that both Scenes load with their Element counts
(`crates/drawing/tests/integration/benchmarks.rs`).

## Procedure

Run everything from the repo root, on a Release build.

| Target | How to measure |
| --- | --- |
| Open 1,000 Elements < 200 ms | `make bench`: loads `scene-1000` and builds its geometry through the `Editor`, best of 10. This is the core's share. For the end-to-end time on a device, open the Scene from the Gallery while recording with Instruments' *Time Profiler* and measure from the tap to the first Canvas frame. |
| Download < 10 MB | `make ipa`, then check the size of `apple/build/pkm.ipa` (an unsigned iOS Release build, zipped). App Store thinning only makes it smaller. |
| < 150 MB memory at 5,000 Elements | Paste the JSON of `scene-5000` into a new Drawing on the device, then pan, zoom and draw for 30 s while recording with Instruments' *Allocations* (or Xcode's memory gauge). Record the peak *Persistent* footprint. As a quick core-only check, run `/usr/bin/time -l` on the release `benchmarks` test binary and read *maximum resident set size*. |
| 120 fps pan/zoom/draw at 5,000 Elements | On iPhone 15 Pro and iPad Pro 4th gen (both ProMotion), paste the JSON of `scene-5000` into a new Drawing, then pan, pinch-zoom and draw freedraw strokes for 30 s each while recording with Instruments' *Animation Hitches* template. Target: frame time ≤ 8.3 ms and no hitches. Simulators don't count. `make bench` also measures the core's share of one input event in `scene-5000` (a pinch step with one Element selected, and a mouse box-select step that grows the Selection to thousands of Elements), including the queries the client makes after each event; it fails above 8.3 ms. |
| No work while idle | Open a Drawing in a Release build and watch the process for 10 s with nothing touched: `top -pid <pid> -stats cpu,idlew` shows 0 % CPU and 0 idle wake-ups (no render loop). `CanvasViewTests` checks the same on frame counts. |

## Results

Recorded 2026-11-03 on a MacBook Air M1 (core and size), rechecked
2026-11-05 in review R14 (open: 4.9 ms; `.ipa`: 1.2 MB, 3.8 MB `.app`) and
review R15 (input events, Mac app memory and idle). The device runs haven't
been recorded yet, as they need the physical devices and Instruments.

R15 on the Mac, Release build, `scene-5000` opened in a 900×450 pt window
(Retina): footprint (`footprint <pid>`) 110 MB, of which ~37 MB are the
Canvas's GPU buffers; 0.0 % CPU and 0 idle wake-ups while idle. Before R15 the
same was 135–148 MB: the 4× multisampled colour and the stencil buffer were
kept in memory (20 bytes per device pixel, ~110 MB for a full-screen iPad
Pro); they now live only in tile memory (`CanvasRenderer`, memoryless). A
box-select step that selects most of `scene-5000` took 34 ms in the core before
R15 (`Selection::contains` was a linear search, called per Element), 4.4 ms
after.

| Target | Result | Status |
| --- | --- | --- |
| Open 1,000 Elements < 200 ms | 4.4 ms (core: load + geometry, best of 10) | met (core); end-to-end on device not yet measured |
| Download < 10 MB | 1.2 MB `.ipa` (3.8 MB uncompressed `.app`), iOS Release | met |
| < 150 MB memory at 5,000 Elements | 35 MB peak RSS (core only, both Scenes loaded with geometry); 110 MB footprint of the Mac app with `scene-5000` open | met (core, Mac); app on device not yet measured |
| 120 fps pan/zoom/draw at 5,000 Elements | core's share of an input event: pinch step 2.2 ms, box-select step 4.4 ms (`make bench`); `geometry()` after a one-Element change 0.9 ms mean, 1.4 ms slowest (was 8.8 ms before the per-Element cache, issue 56) and, with all 5,000 selected, a move step 3.6 ms, a resize step 6 ms and a rotate step 5 ms, their first step (setting the gesture up) 7, 14 and 11 ms, undo 6-8 ms (issue 63) | pending device run; known misses filed |
| No work while idle | 0.0 % CPU, 0 idle wake-ups (Mac, `scene-5000` open) | met |

The first step of a resize or rotate of a large Selection (which keeps what
each Element was, for the gesture and its undo) still takes more than a frame,
and `geometry()` regenerates every moved Element at each step (issue 56 caches
only unchanged ones). File a ticket under `.scratch/` for any other target a later run
misses.
