//! Tauri IPC command layer.
//!
//! Each file corresponds to one IPC surface area. Commands are thin controllers:
//! they validate input, delegate to the engine layer, and return `Result<T, String>`.

pub mod file;
pub mod query;
