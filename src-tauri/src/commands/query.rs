//! Query execution Tauri commands — `run_query` and `get_page`.
//!
//! # IPC design (CONTEXT.md D-06, STACK.md §IPC Serialization Strategy)
//!
//! `run_query` uses a two-command pattern to keep row data on the binary channel
//! and metadata on the JSON channel (CONTEXT.md: "JSON only for sub-10KB metadata"):
//!
//!   1. `run_query(sql)` → `tauri::ipc::Response` (Arrow IPC bytes as ArrayBuffer).
//!      The frontend uses `tableFromIPC(arrayBuffer)` to decode rows.
//!   2. `get_last_result_meta()` → `RunQueryResponse` (JSON: total_rows, capped).
//!      Called immediately after `run_query` to get the 100-row-cap metadata.
//!
//! `get_page(offset, size)` → `PageResponse` (JSON row slice from the cached result).
//!
//! All commands return `Result<_, String>`; no `.unwrap()` / `.expect()` (PITFALLS.md §Pitfall 9).

use crate::ipc::{PageResponse, RunQueryResponse};
use crate::state::AppState;

/// Returns `Ok(())` if `sql` is a plain SELECT or WITH...SELECT (CTE) statement.
///
/// Returns `Err(String)` for any other statement type (DML, DDL, COPY, EXPLAIN) or
/// for SQL that cannot be parsed.
///
/// # Design (D-06 / D-07 / D-08)
///
/// D-06: Only `Statement::Query` is allowed (covers SELECT and WITH ... SELECT).
///       EXPLAIN, DDL, DML, and COPY are all explicitly rejected.
/// D-07: Called at the very top of `run_query`, before `engine.lock()`, so the
///       guard runs in the backend and cannot be bypassed via direct IPC.
/// D-08: Uses `DFParser::parse_sql` (AST-based), not a string-prefix or regex check,
///       so mixed case, leading comments, and multi-statement injections cannot evade it.
///
/// # Note on sqlparser access
///
/// `sqlparser` is a transitive dependency of DataFusion 54. We access it exclusively
/// through `datafusion::sql::parser` — no direct `sqlparser` entry in Cargo.toml
/// (per PITFALLS.md §Pitfall 3 — avoid duplicate crate versions).
pub fn guard_select_only(sql: &str) -> Result<(), String> {
    use datafusion::sql::parser::DFParser;

    let statements = DFParser::parse_sql(sql)
        .map_err(|e| format!("SQL parse error: {e}"))?;

    if statements.is_empty() {
        return Err("No SQL statement provided. Please enter a SELECT query.".to_string());
    }

    for stmt in &statements {
        use datafusion::sql::parser::Statement as DFStmt;
        // `sqlparser` is re-exported by `datafusion_sql` as `pub use sqlparser`,
        // which is surfaced at `datafusion::sql::sqlparser` (no direct dep needed — PITFALLS §3).
        use datafusion::sql::sqlparser::ast::Statement as SqlStmt;

        match stmt {
            DFStmt::Statement(inner) => {
                // The inner box holds a `sqlparser::ast::Statement`.
                // `Statement::Query` covers both plain SELECT and WITH...SELECT (CTE).
                // Every other variant (Insert, Update, Delete, CreateTable, Drop,
                // AlterTable, Truncate, etc.) is rejected here.
                match inner.as_ref() {
                    SqlStmt::Query(_) => {} // allowed: SELECT / WITH...SELECT
                    _ => {
                        return Err(
                            "Only SELECT queries are allowed. \
                             INSERT, UPDATE, DELETE, DDL, and COPY are blocked."
                                .to_string(),
                        );
                    }
                }
            }
            // DFParser produces additional non-SQL variants for DataFusion-specific
            // extensions: CopyTo, CreateExternalTable, Explain (DFParser level), etc.
            // All of these are rejected — they are not read-only SELECT operations.
            _ => {
                return Err(
                    "Only SELECT queries are allowed. \
                     INSERT, UPDATE, DELETE, DDL, and COPY are blocked."
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

/// Executes `sql` against the registered `data` table and returns Arrow IPC bytes.
///
/// The 100-row stream-stop cap is enforced by `QueryEngine::execute` (GRID-03).
/// Row data is returned as an Arrow IPC binary stream via `tauri::ipc::Response`
/// so the frontend can decode with `tableFromIPC(arrayBuffer)` (STACK.md §IPC Serialization Strategy).
///
/// Call `get_last_result_meta()` immediately after to retrieve `total_rows` and `capped`.
///
/// # Security (D-07 / D-08)
///
/// `guard_select_only` is called first, before acquiring the engine lock, to reject
/// non-SELECT statements at the backend level (QUERY-03 / T-02-01 / T-02-02).
#[tauri::command]
pub async fn run_query(
    sql: String,
    state: tauri::State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    // D-07/D-08: Reject non-SELECT statements before execution.
    // AST-based check via DFParser — not a string prefix. Cannot be bypassed via IPC.
    guard_select_only(&sql)?;

    let mut engine = state.engine.lock().await;
    let (_meta, ipc_bytes) = engine.execute(&sql).await?;
    // Metadata is cached on engine.last_query_meta; retrievable via get_last_result_meta.

    Ok(tauri::ipc::Response::new(ipc_bytes))
}

/// Returns the metadata from the most recent `run_query` call (total_rows, capped).
///
/// Must be called after `run_query`. Returns an error if no query has been executed.
/// This command exists so row data (binary IPC) and metadata (JSON) travel on separate channels
/// (CONTEXT.md: JSON only for sub-10KB metadata, binary IPC for row data).
#[tauri::command]
pub async fn get_last_result_meta(
    state: tauri::State<'_, AppState>,
) -> Result<RunQueryResponse, String> {
    let engine = state.engine.lock().await;
    let meta = engine
        .last_query_meta
        .as_ref()
        .ok_or_else(|| "No query results cached. Call run_query first.".to_string())?;

    Ok(RunQueryResponse {
        total_rows: meta.total_rows,
        capped: meta.capped,
    })
}

/// Returns a page of rows from the cached result of the most recent `run_query`.
///
/// Returns `Err` if no query has been executed yet.
#[tauri::command]
pub async fn get_page(
    offset: usize,
    size: usize,
    state: tauri::State<'_, AppState>,
) -> Result<PageResponse, String> {
    let engine = state.engine.lock().await;
    engine.get_page(offset, size)
}
