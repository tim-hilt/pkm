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
- Strive for zero dependencies
- A test failing without your change is a finding: fix it, or file an issue in `.scratch/` before going on
- Commit messages are short and to the point, so that they can appear directly in a changelog. They drive the release version and notes (`cliff.toml`); prefix them with
    - `feature: ` for a new capability
    - `feature!: ` for a breaking change
    - `fix: ` for fixing wrong behavior
    - `chore: ` for everything else

## Rust

- Write tests inside the source code files

## Agent skills

### Issue tracker

Issues and specs are local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Multi-context: `GLOSSARY-MAP.md` points at one `GLOSSARY.md` per Context; system-wide ADRs in `docs/adr/`, Drawings ADRs in `crates/drawing/docs/adr/`. See `docs/agents/domain.md`.

## Searching

Search with `rg` or `git grep`: they skip the gitignored `node_modules` folders, whose minified lines flood a `grep -r`.

## Checking

`make check` formats, lints, then runs the Rust tests, failing at the cheapest step; run it before committing.

## Seeing the app

`make screenshot DEVICE=mac|ipad|iphone [OUT=path]` builds the app, launches it with the sample Drawing open (launch argument `-UITestingOpenSampleDrawing`, temporary store) and writes a PNG (default `/tmp/wt-<device>.png`) that you can read to check layout by eye. `SCREEN=tasks` shows the sample Tasks instead, `SCREEN=task` the sheet of one with a Subtask.

