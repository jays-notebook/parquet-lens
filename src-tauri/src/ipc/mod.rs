//! Shared IPC types and serialization utilities that cross the Tauri boundary.
//!
//! These structs are the single source of truth. The TypeScript counterparts in
//! `src/lib/tauri.ts` must stay in sync (ARCHITECTURE.md §SchemaField Type).
//!
//! Arrow-native type names (`Utf8`, `Int64`) are passed as-is from DataFusion;
//! display formatting is the frontend's responsibility (CONTEXT.md v1 decision).
//!
//! Result-column Arrow type names are supplied by the backend for BOTH the file
//! schema (`OpenFileResponse.schema`, built by `engine::context::get_schema`) and
//! the query result schema (`RunQueryResponse.schema`, built by
//! `engine::executor::execute`). Both use the identical
//! `format!("{:?}", data_type)` expression, so the sidebar and the grid can never
//! disagree about how a type is spelled (D-PH1-01).

pub mod serializer;

use serde::{Deserialize, Serialize};

/// Describes a single column in an Arrow schema — either the registered `data`
/// table's file schema or a query result's own schema.
///
/// `Clone` is required because `RunQueryResponse` derives `Clone` and now owns a
/// `Vec<SchemaField>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Schema of the QUERY RESULT — NOT the opened file's schema.
    ///
    /// This describes the columns the executed statement actually produces, so
    /// aggregates, aliases and computed expressions render under their own
    /// headers instead of the file's columns (RESULT-01/03/04).
    ///
    /// Its `arrow_type` strings are produced by the same
    /// `format!("{:?}", data_type)` expression as `context.rs::get_schema`, so
    /// file-schema and result-schema labels are identical by construction for
    /// every Arrow type — including nested, dictionary, decimal, temporal and
    /// view types (D-PH1-01).
    ///
    /// Travels on the JSON metadata channel because it is metadata sized by
    /// column count; bulk row data stays on the binary Arrow IPC channel.
    pub schema: Vec<SchemaField>,
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
