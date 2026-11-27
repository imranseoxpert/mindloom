# Handoff — checkpoint 1 (engine), 2026-11-27

**Done:** `rust/core` (crate `mindloom-core`) is a full port of `src/renderer/core`: document model and
operations, undo history, themes/style resolution, the nine layouts with hand-placed offsets, connector
and relationship paths, the SVG renderer and export, and Markdown / OPML / text / FreeMind / XMind
import and export.

**Proof:** `cargo test --release` replays every recording in `rust/cases/` (from `node rust/record.js`):

| Suite | Cases | Match |
| --- | --- | --- |
| layout + scene + SVG export (+ focus, ignore-offset layouts) | 368 | 368 |
| style resolution | 13,824 | 13,824 |
| text wrapping | 384 | 384 |
| text formats out + back in | 200 | 200 |
| import detection, hand-written files | 16 | 16 |
| model operations | 400 | 400 |
| normalize | 5 | 5 |

Numbers within 1e-6, strings (SVG, Markdown, OPML, text, FreeMind) byte for byte. A one-character change
to the renderer or a 0.02% change to the measure fails the suite (checked). `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` are clean.

**Known differences, by design:**
- JSON parse error messages are serde's, not V8's (both still fail).
- `&#xD800;`-style entities for lone surrogates become U+FFFD (Rust strings cannot hold them).
- XMind import is not in the recordings (the JS reads it asynchronously); unit tests cover JSON and XML
  variants and a damaged file.
- `.mindloom` files keep their JSON shape; key order inside a node can differ from 1.x.

**Next:** `rust/app` with eframe/egui (canvas painted from `layout::Layout`), then the shell, AI and CI.

# Handoff — checkpoint 2 (interface), 2026-11-27

**Done:** `rust/app` (binary `mindloom`) is the desktop app: eframe/egui 0.36, canvas painted natively from the
engine's layout (anti-aliased shapes, connectors, text in the measured fonts, colour emoji markers rendered by
resvg), connected dragging with home-slot snap, side switching, re-parent/reorder drops, marquee, pan/zoom,
relationship reshaping, inline editing, every 1.x command and shortcut, inspector (Topic/Map/Markers/Notes) with
live previews, outline, minimap, search, focus, presentation, templates, context menus, menus and toolbar,
open/save/import (incl. XMind), autosave with crash recovery, recent files, PNG/SVG/PDF/text exports, copy as
image, image insert/paste/drop, and the BYOK AI assistant (keys in the system keychain).

**Proof:** `MINDLOOM_SELFTEST=<dir> MINDLOOM_CONFIG_DIR=<tmp> target/release/mindloom` drives the real interface
with mouse and keyboard events and writes screenshots plus `report.txt` (18/18 pass, including a 1,453-topic map
laid out in 1.2 ms). `cargo test` (engine replay + app unit tests), `cargo clippy -D warnings` and `cargo fmt` are
clean. Screenshots: `docs/screenshots/`.

**Not yet verified / known gaps:**
- Windows has not been built or run yet (next step: CI).
- The AI assistant has not made a live call (needs a key; costs money).
- Menus are in the window (egui), not the native macOS menu bar.
- Opening a file by double-clicking it in Finder while the app runs is not wired up yet (comes with packaging and
  file associations); opening from the command line and dropping files on the window work.
- New Window starts a second process.

**Next:** packaging and CI (.dmg universal, .exe installer), then checkpoint 3 before publishing 2.0.0.

# Handoff — packaging, 2026-11-27 (before checkpoint 3)

**Done:** `rust/packaging/macos/build.sh` makes a universal (arm64 + x86_64), ad-hoc signed `Mindloom.app` and
`Mindloom-2.0.0-mac-universal.dmg` (30 MB) with document types for .mindloom (owner) and .xmind/.mm/.opml.
Files double-clicked in Finder open, at launch and while running (`app/src/mac.rs`; verified with `open`).
`rust/packaging/windows/mindloom.iss` makes `Mindloom-2.0.0-win-x64.exe` (12 MB, per-user install, file
associations, Start menu entry, uninstaller); CI also makes `Mindloom-2.0.0-win-x64-portable.zip`. The exe carries
the icon and version info (`app/build.rs`). HTTPS uses the operating system's TLS (no C crypto library).

`.github/workflows/rust-release.yml` runs on pushes to `rust-2*` branches, by hand, and on `v2*` tags: fmt,
clippy, tests, the builds, and the self test on both macOS and Windows runners (18/18 on both, screenshots kept
as artifacts). A `v2*` tag also creates a *draft* release with the installers.

**Still not verified:** the installers have only run on GitHub runners and this Mac (the Windows installer itself
has not been run interactively); the AI assistant has made no live call. Neither installer is signed with a paid
certificate (same as 1.x).

**To publish 2.0.0:** merge `rust-2.0` into `main`, tag `v2.0.0`, review the draft release, publish it.
