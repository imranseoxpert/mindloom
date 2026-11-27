# Mindloom Rust port — decisions

All accepted by the user on 2026-11-27 ("all yes, go on").

## ADR-001: Pure Rust app, no web view
Status: accepted (2026-11-27)
Context: Mindloom 1.x is Electron (Node main process + HTML/JS/SVG renderer, ~5,000 lines). The user asked for the app to be built entirely in Rust.
Decision: Rewrite every part in Rust, including the interface. No Tauri, no web view, no JavaScript at run time.
Consequences: The UI is rebuilt, not translated. The 226 MB Electron bundle becomes a native binary.

## ADR-002: egui (eframe) for the interface
Status: accepted (2026-11-27)
Context: The app is a free-form drawing canvas plus panels, dialogs and text editing, on macOS and Windows.
Decision: eframe/egui. The canvas is painted with epaint shapes from the ported layout.
Consequences: Clean but not OS-native look; stickers render as egui's single-colour emoji font.

## ADR-003: Full feature parity with Mindloom 1.1.0
Status: accepted (2026-11-27)
Decision: Port every feature of 1.1.0: 9 structures, 16 themes, shapes, lines, backgrounds, markers, notes, links, images, relationships, boundaries, floating topics, connected dragging, focus, presentation, search, outline, mini map, templates, autosave/recovery, all import/export formats, multi-provider AI (11 providers + custom).

## ADR-004: `.mindloom` file format unchanged
Status: accepted (2026-11-27)
Decision: Read and write the same JSON as 1.x (field names, `offset` keyed by layout, floating `x`/`y`). Files open in 1.x, 2.x and the Notes app.

## ADR-005: Equivalence bar for the engine
Status: accepted (2026-11-27)
Context: Chromium canvas and the Rust font stack measure text differently.
Decision: The engine takes text measurement as an input. With the recording's fixed measure (UTF-16 length × size × 0.55), layout geometry must match the JS engine within 1e-6 px and text exports (Markdown, OPML, FreeMind, text, SVG structure) must match byte for byte. Recorded cases live in `rust/cases/`.
Consequences: Real on-screen sizes differ slightly from 1.x; behaviour does not.

## ADR-006: Bundle Inter as the default font
Status: accepted (2026-11-27)
Decision: Ship Inter (SIL OFL) for the "System" font choice so maps look the same on Mac and Windows; the other font choices use installed system fonts found by name.

## ADR-007: Windows builds on GitHub Actions
Status: accepted (2026-11-27)
Context: The gh token already has `workflow` scope.
Decision: A workflow builds the Windows .exe (and the macOS universal .dmg) on GitHub-hosted runners for `v*` tags.

## ADR-008: Release as Mindloom 2.0.0; keep the JavaScript
Status: accepted (2026-11-27)
Decision: The Rust app ships as 2.0.0 from `rust/`. The JS sources stay: the Notes app's mind maps are built from `src/renderer/core`.

## ADR-009: Check-ins
Status: accepted (2026-11-27)
Decision: Stop and report after (1) the engine matches the recordings, (2) the interface works, (3) before publishing 2.0.0.

## ADR-010: Engine recorded with a custom harness
Status: accepted (2026-11-27)
Context: `rust-anything analyze` found 0 bridgeable exports: the JS engine registers itself on a global (`window.ML`), so `bridge`/`record` cannot drive it.
Decision: `rust/record.js` runs the JS engine under Node on generated documents and writes `rust/cases/*.json`; the Rust crate's tests replay them.
