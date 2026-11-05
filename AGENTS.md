# Project Goal

- Native Clone of Excalidraw (source available at ../../excalidraw/excalidraw)
- Highly optimized for Touch devices
- Project Target devices; can also be used for simulation
    - Macbook Air M1; MacOS 27+
    - iPhone 15 Pro; iOS 27+
    - iPad Pro 4th Gen, 12.9in; iPadOS 26+

# Why?

Excalidraw is not optimally usable in the browser on a smartphone or tablet. The context menu shows more often than is practical, selections get deleted if taps or not optimally timed or placed.

# Coding Guidelines

- Stay Lightweight! Focus especially on
    - High Performance
    - Low memory usage
    - Minimal app size
- Use test driven development (red, green, refactor) wherever applicable


## Agent skills

### Issue tracker

Issues and specs are local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.

## Checking

`make check` formats, lints, then runs the Rust tests, failing at the cheapest step; run it before committing. Rust integration tests are modules of `crates/core/tests/integration/main.rs`.

## Seeing the app

`make screenshot DEVICE=mac|ipad|iphone [OUT=path]` builds the app, launches it with the sample Drawing open (launch argument `-UITestingOpenSampleDrawing`, temporary store) and writes a PNG (default `/tmp/wt-<device>.png`) that you can read to check layout by eye.

## Sideloading

`make ipa` builds an unsigned `apple/build/WorkingTitle.ipa` for an iPhone or iPad. SideStore, AltStore and similar tools re-sign it on the device; no provisioning profile is embedded.
