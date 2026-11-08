# Project Goal

- Native Clone of Excalidraw (source available at ../../excalidraw/excalidraw)
- Highly optimized for Touch devices
- Project Target devices; can also be used for simulation
    - Macbook Air M1; MacOS 27+
    - iPhone 15 Pro; iOS 27+
    - iPad Pro 4th Gen, 12.9in; iPadOS 26+
- Web client for environments, where a native application can not be installed

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

## Web app

The web app lives in `web/` (Bun for packages, bundling and the dev server; Vitest; oxlint and oxfmt; no UI framework) and runs the core as wasm from `crates/web` (ADR-0014). `make web-wasm` builds the wasm into `web/generated` (cargo with the `web` profile, then `wasm-bindgen` at the version in `Cargo.lock`, installed on demand); `make check` runs `oxfmt --check`, `oxlint`, `tsc`, the wasm build and `vitest run` (Node, against the real wasm) first.

- `make web-dev` serves http://localhost:3000.
- `make screenshot DEVICE=web [OUT=path] [WIDTH=] [HEIGHT=]` serves the Gallery and captures a PNG with Playwright's Chromium (WebGPU on). Run `cd web && bunx playwright-core install chromium` once.
- `make web-test-browser` runs the renderer tests in Chromium with WebGPU (reference images in `web/references`, pixel probes), outside `make check`; `make web-record-references` re-records the references. See `docs/web-renderer-reference-images.md`.
- `make web-fmt` formats the web app (`make fmt` does too).
- Validate the running app with the Chrome DevTools MCP: start `make web-dev`, open `http://localhost:3000/` (WebGPU needs a secure context; `localhost` is one), resize the page to phone and desktop widths, take screenshots and read the console for errors.

## Installing on a device

`scripts/install.sh "<device-name>"` builds Release and installs over `devicectl` (or into `/Applications` for `mac`). Apple registers an App ID on the first device build under a new bundle ID and allows only 10 per team every 7 days; when that quota is exhausted the script installs under `com.timhilt.workingtitle`, the app's pre-rebrand ID that is already registered, and says so. `BUNDLE_ID=<id>` picks the ID explicitly.

## Sideloading

`make ipa` builds an unsigned `apple/build/ExcalidrawNative.ipa` for an iPhone or iPad. SideStore, AltStore and similar tools re-sign it on the device; no provisioning profile is embedded.
