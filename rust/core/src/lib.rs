//! Mindloom's engine: the `.mindloom` document model, the nine layouts, the SVG renderer,
//! and import/export of text formats. A port of the 1.x JavaScript core, checked against
//! recordings of it (see `rust/record.js` and `tests/replay.rs`).

pub mod data;
pub mod io;
pub mod js;
pub mod layout;
pub mod model;
pub mod render;
pub mod templates;
pub mod themes;
