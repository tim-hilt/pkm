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
(`crates/core/tests/benchmarks.rs`).

## Procedure

Run everything from the repo root, on a Release build.

| Target | How to measure |
| --- | --- |
| Open 1,000 Elements < 200 ms | `make bench`: loads `scene-1000` and builds its geometry through the `Editor`, best of 10. This is the core's share. For the end-to-end time on a device, open the Scene from the Gallery while recording with Instruments' *Time Profiler* and measure from the tap to the first Canvas frame. |
| Download < 10 MB | `xcodebuild -scheme WorkingTitle -configuration Release -destination 'generic/platform=iOS' -derivedDataPath /tmp/wt-rel CODE_SIGNING_ALLOWED=NO build`, then zip `Payload/WorkingTitle.app` into an `.ipa` and check the size. App Store thinning only makes it smaller. |
| < 150 MB memory at 5,000 Elements | Import `scene-5000` on the device, then pan, zoom and draw for 30 s while recording with Instruments' *Allocations* (or Xcode's memory gauge). Record the peak *Persistent* footprint. As a quick core-only check, run `/usr/bin/time -l` on the release `benchmarks` test binary and read *maximum resident set size*. |
| 120 fps pan/zoom/draw at 5,000 Elements | On iPhone 15 Pro and iPad Pro 4th gen (both ProMotion), import `scene-5000`, then pan, pinch-zoom and draw freedraw strokes for 30 s each while recording with Instruments' *Animation Hitches* template. Target: frame time ≤ 8.3 ms and no hitches. Simulators don't count. |

## Results

Recorded 2026-11-03 on a MacBook Air M1 (core and size). The device runs
haven't been recorded yet, as they need the physical devices and Instruments.

| Target | Result | Status |
| --- | --- | --- |
| Open 1,000 Elements < 200 ms | 4.4 ms (core: load + geometry, best of 10) | met (core); end-to-end on device not yet measured |
| Download < 10 MB | 1.1 MB `.ipa` (3.6 MB uncompressed `.app`), iOS Release | met |
| < 150 MB memory at 5,000 Elements | 35 MB peak RSS (core only, both Scenes loaded with geometry) | met (core); app on device not yet measured |
| 120 fps pan/zoom/draw at 5,000 Elements | not yet measured | pending device run |

No measured result misses a target, so no regression tickets were filed. File
one under `.scratch/` for any target a later run misses.
