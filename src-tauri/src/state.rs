//! Application state managed by Tauri.
//!
//! `AppState` holds the single `QueryEngine` instance behind a `tokio::sync::Mutex`.
//! Tauri wraps state in `Arc` internally — no manual `Arc` wrapper here.

use crate::engine::QueryEngine;

/// Top-level application state injected into every Tauri command via `State<AppState>`.
///
/// Uses `tokio::sync::Mutex` (NOT `std::sync::Mutex`) because the guard must be held
/// across `.await` points inside async commands (ARCHITECTURE.md Anti-Pattern 4).
pub struct AppState {
    pub engine: tokio::sync::Mutex<QueryEngine>,
}
