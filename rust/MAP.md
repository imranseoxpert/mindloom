# Mindloom Rust port — map

Goal: Mindloom as a pure Rust desktop app (macOS, Windows), released as 2.0.0. Decisions: [DECISIONS.md](DECISIONS.md) (ADR-001…010). Terms: [GLOSSARY.md](GLOSSARY.md).

| Unit | Crate / module | State |
| --- | --- | --- |
| Engine: themes, model, templates | `rust/core` (`themes.rs`, `model.rs`, `templates.rs`) | done — gate passes (2026-11-27) |
| Engine: layout + connector paths | `rust/core/layout.rs` | done — gate passes (2026-11-27) |
| Engine: SVG renderer | `rust/core/render.rs` | done — gate passes (2026-11-27) |
| Engine: import/export (MD, OPML, FreeMind, text, XMind) | `rust/core/io.rs` | done — gate passes (2026-11-27) |
| Interface: canvas, editing, drag, panels, dialogs | `rust/app` (`canvas.rs`, `stage.rs`, `state.rs`, `ui.rs`) | done — self test 18/18 (2026-11-27) |
| Shell: files, recent, autosave, menus, PNG/PDF export | `rust/app` (`files.rs`, `main.rs`) | done (2026-11-27) |
| AI assistant (BYOK, 11 providers) | `rust/app/ai.rs` | done — not run against a live API yet |
| Packaging + CI (.dmg, .exe) | `rust/packaging/`, `.github/workflows/rust-release.yml` | done — CI green on macOS and Windows, self test 18/18 on both (2026-11-27) |

Checkpoints (ADR-009): 1 engine matches (reached 2026-11-27) · 2 interface works (reached 2026-11-27) · 3 before 2.0.0.

Out of scope: the browser build of 1.x; `scripts/` (replaced by Rust tooling).
