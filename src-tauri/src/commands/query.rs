//! Query execution Tauri commands — `run_query` and `get_page`.
//!
//! # IPC design (CONTEXT.md D-06, STACK.md §IPC Serialization Strategy)
//!
//! `run_query(sql)` returns a SINGLE framed `tauri::ipc::Response` carrying both the
//! result metadata and the row bytes:
//!
//! ```text
//! [0..4)              u32 little-endian meta_len
//! [4..4+meta_len)     meta JSON (UTF-8) — a serialized `RunQueryResponse`
//! [4+meta_len..)      Arrow IPC stream bytes (empty for a zero-row result)
//! ```
//!
//! The frame is built by `crate::ipc::frame::frame_meta_and_bytes` and decoded by
//! `decodeQueryFrame` in `src/lib/tauri.ts`. Bulk row data still travels as binary and
//! the metadata segment stays sized by column count, so the original channel-separation
//! intent (CONTEXT.md: "JSON only for sub-10KB metadata") is preserved.
//!
//! This replaces an earlier two-command pattern — `run_query` for bytes, then
//! `get_last_result_meta` for metadata. The engine mutex was released between those two
//! calls, so an interleaved `open_file` (which clears `last_query_meta` via
//! `register_source`) turned a successful query into "No query results cached", and an
//! interleaved `run_query` paired one execution's bytes with another's metadata. Framing
//! removes that desynchronisation window: both halves come from the same
//! `engine.execute` return value (WR-03).
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
/// # D-06 clarification: `Statement::Query` alone is NOT sufficient (WR-04)
///
/// `SELECT * INTO new_table FROM data` parses as a `Statement::Query` whose `Select`
/// body carries `into: Some(..)` — the `INTO` never surfaces as its own statement
/// variant. A bare `Statement::Query(_) => {}` arm therefore waved it through, and
/// DataFusion plans that statement as `CreateMemoryTable`, creating a session-scoped
/// table under a contract that promises read-only operations. Source files were never
/// at risk (the memory table is in-session only and the `SessionContext` is rebuilt on
/// every file open), but the documented guarantee was false. Source: review finding
/// WR-04.
///
/// The guard now validates the query RECURSIVELY: the query body, every CTE body,
/// every set-operation operand, and every FROM-clause relation (derived-table
/// subqueries, nested joins, and the inputs of PIVOT/UNPIVOT/MATCH_RECOGNIZE) are
/// walked, and `into: Some(..)` is rejected wherever that walk reaches. Every
/// `SetExpr` and `TableFactor` variant is handled explicitly so a future grammar
/// addition cannot silently reopen the hole.
///
/// Known limit of layer 1 (review finding WR-01): expression-level subqueries
/// (`Expr::Subquery` in the projection or `WHERE`) are NOT walked here. An `INTO`
/// hidden there is stopped by the plan-level `SQLOptions` backstop in
/// `engine/executor.rs`, which visits the whole plan tree including subqueries.
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
                    // Allowed: SELECT / WITH...SELECT — but only once the body and
                    // every CTE body are proven free of `INTO` and embedded DML (WR-04).
                    SqlStmt::Query(q) => {
                        if !query_is_read_only(q) {
                            return Err(
                                "Only read-only SELECT queries are allowed. \
                                 SELECT ... INTO creates a table and is blocked."
                                    .to_string(),
                            );
                        }
                    }
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

/// Returns `true` when `q` — its body AND every CTE body it declares — performs no
/// write-shaped operation.
///
/// A `WITH` clause is checked separately from the body because a CTE body is never
/// reached by walking `q.body`: `with c as (select * into t from data) select * from c`
/// has a perfectly innocent outer body and hides the `INTO` in `with.cte_tables` (WR-04).
fn query_is_read_only(q: &datafusion::sql::sqlparser::ast::Query) -> bool {
    if let Some(with) = &q.with
        && !with
            .cte_tables
            .iter()
            .all(|cte| query_is_read_only(&cte.query))
    {
        return false;
    }

    set_expr_is_read_only(&q.body)
}

/// Returns `true` when `expr` performs no write-shaped operation at any depth.
///
/// Every `SetExpr` variant is matched explicitly — there is deliberately no catch-all
/// `_ => true` arm. A new variant added by a future sqlparser release must break the
/// build rather than default to "allowed", because defaulting to allowed is exactly how
/// `SELECT ... INTO` slipped through in the first place (WR-04).
fn set_expr_is_read_only(expr: &datafusion::sql::sqlparser::ast::SetExpr) -> bool {
    use datafusion::sql::sqlparser::ast::SetExpr;

    match expr {
        // The `INTO` that DataFusion plans as `CreateMemoryTable` lives here — and it
        // can also hide inside a FROM-clause relation, e.g. a derived table:
        // `select * from (select * into t from data) sub` (review finding WR-01).
        SetExpr::Select(select) => {
            select.into.is_none()
                && select.from.iter().all(table_with_joins_is_read_only)
        }

        // A parenthesised subquery may carry its own WITH clause and body.
        SetExpr::Query(inner) => query_is_read_only(inner),

        // UNION / EXCEPT / INTERSECT: the INTO may hide in either operand.
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_read_only(left) && set_expr_is_read_only(right)
        }

        // Literal rows and a bare `TABLE t` read nothing into a new table.
        SetExpr::Values(_) | SetExpr::Table(_) => true,

        // sqlparser can nest a DML statement inside a query body. These arms are not
        // reachable through the current DataFusion grammar, but they exist so a grammar
        // change cannot reopen the hole silently.
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => false,
    }
}

/// Returns `true` when a FROM-clause entry — its base relation AND every joined
/// relation — performs no write-shaped operation (review finding WR-01).
fn table_with_joins_is_read_only(twj: &datafusion::sql::sqlparser::ast::TableWithJoins) -> bool {
    table_factor_is_read_only(&twj.relation)
        && twj.joins.iter().all(|j| table_factor_is_read_only(&j.relation))
}

/// Returns `true` when a single FROM-clause relation performs no write-shaped
/// operation at any depth.
///
/// A derived table carries a full `Query` — `select * from (select * into t from
/// data) sub` hides its `INTO` there — so this walk is what makes the layer-1
/// guarantee ("rejected wherever the walk reaches") true for FROM clauses (WR-01).
///
/// Every `TableFactor` variant is matched explicitly — no catch-all — for the same
/// reason as `set_expr_is_read_only`: a new variant added by a future sqlparser
/// release must break the build rather than default to "allowed".
fn table_factor_is_read_only(tf: &datafusion::sql::sqlparser::ast::TableFactor) -> bool {
    use datafusion::sql::sqlparser::ast::TableFactor;

    match tf {
        // A derived table is a parenthesised subquery — recurse into the full Query
        // (body + its own CTEs) exactly like a top-level query (WR-01).
        TableFactor::Derived { subquery, .. } => query_is_read_only(subquery),

        // A parenthesised join tree nests a whole FROM entry: recurse into the base
        // relation and every join operand.
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => table_with_joins_is_read_only(table_with_joins),

        // Operators that wrap another table factor: unwrap and recurse into the input.
        TableFactor::Pivot { table, .. }
        | TableFactor::Unpivot { table, .. }
        | TableFactor::MatchRecognize { table, .. } => table_factor_is_read_only(table),

        // Leaf relations: plain table references and table-valued functions. These
        // carry identifiers/expressions, not query bodies, so there is no `Query` to
        // recurse into here. (Scalar subqueries inside their argument EXPRESSIONS are
        // the documented layer-1 limit — covered by the SQLOptions plan backstop.)
        TableFactor::Table { .. }
        | TableFactor::TableFunction { .. }
        | TableFactor::Function { .. }
        | TableFactor::UNNEST { .. }
        | TableFactor::JsonTable { .. }
        | TableFactor::OpenJsonTable { .. }
        | TableFactor::XmlTable { .. }
        | TableFactor::SemanticView { .. } => true,
    }
}

/// Executes `sql` against the registered `data` table and returns one framed response
/// carrying BOTH the result metadata and the Arrow IPC row bytes.
///
/// The 100-row stream-stop cap is enforced by `QueryEngine::execute` (GRID-03).
/// The frame layout is documented on this module and on
/// `crate::ipc::frame::frame_meta_and_bytes`; the frontend decodes it with
/// `decodeQueryFrame` (STACK.md §IPC Serialization Strategy).
///
/// # Atomicity (WR-03)
///
/// The metadata and the bytes are the two halves of a single `engine.execute` return
/// value and are serialized together, so they can never describe different executions.
/// A second round-trip for the metadata could be — and was — desynchronised by any
/// command that ran while the engine mutex was free.
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

    let (meta, ipc_bytes) = {
        let mut engine = state.engine.lock().await;
        engine.execute(&sql).await?
        // The metadata is also cached on engine.last_query_meta for the secondary
        // get_last_result_meta accessor, but this path no longer depends on that cache.
    };
    // Lock released: framing touches nothing shared, and `meta` is already paired with
    // `ipc_bytes` by value, so nothing that happens next can desynchronise them.

    let frame = crate::ipc::frame::frame_meta_and_bytes(&meta, &ipc_bytes)?;

    Ok(tauri::ipc::Response::new(frame))
}

/// Returns the metadata from the most recent `run_query` call
/// (`total_rows`, `capped`, and the query RESULT schema).
///
/// # Retained as a secondary accessor only (WR-03)
///
/// This command is NO LONGER part of the `run_query` path: `run_query` now returns the
/// same metadata inline in its framed response, so the frontend never makes this call
/// during a query. It remains registered as a diagnostic read of the cached
/// `last_query_meta` — and as such it is inherently a snapshot of whatever query ran
/// last, which is exactly why the query path stopped relying on it.
///
/// Returns an error if no query has been executed since the last file open
/// (`register_source` clears the cache).
///
/// The result schema belongs on the metadata channel because it is sized by column
/// count, not row count (D-PH1-01).
///
/// The cached struct is cloned wholesale rather than rebuilt field-by-field: a manual rebuild
/// silently drops any newly added field (it dropped `schema` before this change) and is a
/// recurring maintenance hazard.
#[tauri::command]
pub async fn get_last_result_meta(
    state: tauri::State<'_, AppState>,
) -> Result<RunQueryResponse, String> {
    let engine = state.engine.lock().await;
    let meta = engine
        .last_query_meta
        .as_ref()
        .ok_or_else(|| "No query results cached. Call run_query first.".to_string())?;

    Ok(meta.clone())
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
