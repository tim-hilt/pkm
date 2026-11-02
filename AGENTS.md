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
