//! parquet-lens — Tauri v2 desktop application library entry point.
//!
//! Wires together the plugin stack, managed state, and IPC command handlers.
//! All business logic lives in the submodules (engine, storage, commands, ipc).

pub mod commands;
pub mod engine;
pub mod ipc;
pub mod state;
pub mod storage;

use engine::QueryEngine;
use state::AppState;

/// Entry point called from `main.rs`.
///
/// Registers plugins before `invoke_handler` per Tauri v2 convention.
/// `.expect()` here is acceptable — startup failure is unrecoverable.
/// Never calls `tokio::runtime::Runtime::new()` — Tauri owns the runtime (PITFALLS.md §Pitfall 9).
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            engine: tokio::sync::Mutex::new(QueryEngine::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::file::open_file,
            commands::file::get_file_metadata,
            commands::file::open_remote_file,
            commands::query::run_query,
            commands::query::get_last_result_meta,
            commands::query::get_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
