//! **StageSync** — the desktop application, as a library.
//!
//! The binary in `main.rs` is a thin shell around this library. Keeping the
//! application logic in a library target makes the parts that do not need a window —
//! the file list bookkeeping, the export step and the painters — reachable from
//! integration tests.

pub mod app;
pub mod chart;
pub mod jobs;
pub mod state;
pub mod timeline;
