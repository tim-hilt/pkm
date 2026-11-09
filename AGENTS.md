# Project Goal

Personal Knowledge Management and organization application that combines the following functionality

- Tasks
    - Prior Art: Todoist
- Notes
    - Prior Art: Obsidian
- Links
    - Prior Art: raindrop.io
- Sketches
    - Prior Art: Excalidraw, Miro
- Books
    - Prior Art: Nutrient PDF Reader

# Target Environment

- Native target devices; can also be used for simulation
    - Macbook Air M1; MacOS 27
    - iPhone 15 Pro; iOS 27
    - iPad Pro 4th Gen, 12.9in; iPadOS 26
- Web client for environments, where a native application can not be installed

# Coding Guidelines

- Stay Lightweight! Focus especially on
    - High Performance
    - Low memory usage
    - Minimal app size
- Don't add bloat
- Use test driven development (red, green, refactor) wherever applicable
- Use native UI functionality wherever possible
- Keep documentation very lean and precise. No walls of text in this project
- Apply the boy scouts rule: Leave the code cleaner than you found it

## Rust

- Write tests inside the source code files

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

