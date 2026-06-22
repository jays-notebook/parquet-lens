//! Behavior tests for the SELECT-only guard (Plan 02-01, Task 1).
//!
//! # Design notes (D-06 / D-07 / D-08)
//!
//! D-06: SELECT and WITH...SELECT (CTEs) are the only allowed statement forms.
//!       EXPLAIN, DDL, DML, and COPY are blocked even if they wrap a SELECT.
//! D-07: The guard runs in the Rust backend *before* `engine.execute`, so it cannot
//!       be bypassed by crafting an IPC call directly.
//! D-08: The guard uses `DFParser::parse_sql` (AST-based), not a string-prefix check,
//!       so comments, whitespace, mixed case, and multi-statement injections cannot evade it.
//!
//! Tests legitimately use unwrap/expect for assertions — idiomatic in Rust test code.
#![allow(clippy::unwrap_used)]

use parquet_lens_lib::commands::query::guard_select_only;

// ---------------------------------------------------------------------------
// Allowed: SELECT and WITH (CTE)
// ---------------------------------------------------------------------------

#[test]
fn test_allow_simple_select() {
    // Basic lowercase SELECT must be allowed.
    assert!(
        guard_select_only("select * from data").is_ok(),
        "plain SELECT must be allowed"
    );
}

#[test]
fn test_allow_uppercase_select() {
    assert!(
        guard_select_only("SELECT id, name FROM data WHERE id = 1").is_ok(),
        "uppercase SELECT must be allowed"
    );
}

#[test]
fn test_allow_select_with_limit() {
    assert!(
        guard_select_only("SELECT * FROM data LIMIT 100").is_ok(),
        "SELECT with LIMIT must be allowed"
    );
}

#[test]
fn test_allow_cte_with_select() {
    // WITH ... SELECT (CTE) is a read-only pattern — must be allowed (D-06).
    let sql = "WITH t AS (SELECT 1) SELECT * FROM t";
    assert!(
        guard_select_only(sql).is_ok(),
        "CTE (WITH...SELECT) must be allowed (D-06)"
    );
}

#[test]
fn test_allow_leading_comment_then_select() {
    // Comments and leading whitespace must not confuse the AST parser (D-08).
    let sql = "-- comment\n  SELECT 1";
    assert!(
        guard_select_only(sql).is_ok(),
        "leading comment + whitespace before SELECT must be allowed (D-08)"
    );
}

#[test]
fn test_allow_block_comment_then_select() {
    let sql = "/* block comment */ SELECT 42";
    assert!(
        guard_select_only(sql).is_ok(),
        "block comment before SELECT must be allowed (D-08)"
    );
}

// ---------------------------------------------------------------------------
// Blocked: DML
// ---------------------------------------------------------------------------

#[test]
fn test_reject_insert() {
    let result = guard_select_only("INSERT INTO data VALUES (1)");
    assert!(result.is_err(), "INSERT must be rejected");
    // The error message must mention read-only / SELECT restriction.
    let msg = result.unwrap_err();
    assert!(
        msg.to_lowercase().contains("select") || msg.to_lowercase().contains("allowed"),
        "rejection message must reference SELECT or allowed: got '{msg}'"
    );
}

#[test]
fn test_reject_update() {
    let result = guard_select_only("UPDATE data SET x = 1");
    assert!(result.is_err(), "UPDATE must be rejected");
}

#[test]
fn test_reject_delete() {
    let result = guard_select_only("DELETE FROM data WHERE id = 1");
    assert!(result.is_err(), "DELETE must be rejected");
}

// ---------------------------------------------------------------------------
// Blocked: DDL
// ---------------------------------------------------------------------------

#[test]
fn test_reject_create_table() {
    let result = guard_select_only("CREATE TABLE t (x INT)");
    assert!(result.is_err(), "CREATE TABLE must be rejected");
}

#[test]
fn test_reject_drop_table() {
    let result = guard_select_only("DROP TABLE data");
    assert!(result.is_err(), "DROP TABLE must be rejected");
}

#[test]
fn test_reject_alter_table() {
    let result = guard_select_only("ALTER TABLE data ADD COLUMN y INT");
    assert!(result.is_err(), "ALTER TABLE must be rejected");
}

// ---------------------------------------------------------------------------
// Blocked: COPY
// ---------------------------------------------------------------------------

#[test]
fn test_reject_copy() {
    // DataFusion's DFParser may represent COPY as a DFParser-specific variant
    // (CreateExternalTable / CopyTo); either way it must be rejected.
    let result = guard_select_only("COPY data TO 'x.csv'");
    assert!(result.is_err(), "COPY must be rejected");
}

// ---------------------------------------------------------------------------
// Blocked: EXPLAIN (D-06 — EXPLAIN wraps a query but is not itself a query)
// ---------------------------------------------------------------------------

#[test]
fn test_reject_explain() {
    let result = guard_select_only("EXPLAIN SELECT * FROM data");
    assert!(result.is_err(), "EXPLAIN must be rejected (D-06)");
}

#[test]
fn test_reject_explain_verbose() {
    let result = guard_select_only("EXPLAIN VERBOSE SELECT 1");
    assert!(result.is_err(), "EXPLAIN VERBOSE must be rejected (D-06)");
}

// ---------------------------------------------------------------------------
// Blocked: invalid / unparseable SQL
// ---------------------------------------------------------------------------

#[test]
fn test_reject_invalid_sql() {
    // Garbage input must return Err (parse error), not panic.
    let result = guard_select_only("not valid sql ;;");
    assert!(result.is_err(), "invalid SQL must return Err, not panic");
}

#[test]
fn test_reject_empty_string() {
    // An empty query must not panic; it should return an error.
    let result = guard_select_only("");
    assert!(result.is_err(), "empty SQL must return Err");
}

#[test]
fn test_reject_semicolons_only() {
    let result = guard_select_only(";;;");
    assert!(result.is_err(), "bare semicolons must return Err");
}
