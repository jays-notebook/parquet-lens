//! Shared IPC types and serialization utilities that cross the Tauri boundary.
//!
//! These structs are the single source of truth. The TypeScript counterparts in
//! `src/lib/tauri.ts` must stay in sync (ARCHITECTURE.md §SchemaField Type).
//!
//! Arrow-native type names (`Utf8`, `Int64`) are passed as-is from DataFusion;
//! display formatting is the frontend's responsibility (CONTEXT.md v1 decision).

pub mod serializer;

use serde::{Deserialize, Serialize};

/// Describes a single column in the registered `data` table's Arrow schema.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    /// Arrow-native type name, e.g. "Int64", "Utf8", "Float64".
    pub arrow_type: String,
    pub nullable: bool,
}

/// Returned by `open_file` after the file is registered as table `data`.
#[derive(Debug, Serialize)]
pub struct OpenFileResponse {
    pub schema: Vec<SchemaField>,
}

/// Returned by `run_query` after SQL execution (Plan 02).
#[derive(Debug, Clone, Serialize)]
pub struct RunQueryResponse {
    pub total_rows: usize,
    /// `true` when the backend 100-row cap was hit (CONTEXT.md D-04).
    pub capped: bool,
}

/// A single page of result rows returned by `get_page` (Plan 02).
#[derive(Debug, Serialize)]
pub struct PageResponse {
    pub rows: Vec<serde_json::Value>,
    pub offset: usize,
    pub has_more: bool,
}

/// Per-row-group statistics extracted from the Parquet footer (META-03).
///
/// Rust → frontend only (no `Deserialize` — serde-as-is snake_case keys).
#[derive(Debug, Clone, Serialize)]
pub struct RowGroupInfo {
    pub num_rows: i64,
    pub total_byte_size: i64,
    /// Compression codec name, e.g. "SNAPPY", "ZSTD", "UNCOMPRESSED".
    pub compression: String,
}

/// File-level metadata from the Parquet footer (META-02 + META-03).
///
/// Rust → frontend only (no `Deserialize` — serde-as-is snake_case keys).
#[derive(Debug, Clone, Serialize)]
pub struct FileMetadata {
    /// Total row count summed from all row groups (META-02).
    pub total_rows: i64,
    pub row_groups: Vec<RowGroupInfo>,
}
