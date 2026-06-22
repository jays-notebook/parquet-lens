//! DataFusion `SessionContext` lifecycle management.
//!
//! `QueryEngine` owns one `SessionContext` per opened file. The context is recreated
//! when `register_source` is called for a new file — never recreated per query
//! (ARCHITECTURE.md Anti-Pattern 2).
//!
//! All Arrow types are imported via `datafusion::arrow::*` — never from a direct
//! `arrow` crate dependency (PITFALLS.md §Pitfall 3).
//! All Parquet types are imported via `datafusion::parquet::*` — never from a direct
//! `parquet` crate dependency (PITFALLS.md §Pitfall 3).

use std::sync::Arc;

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::listing::{ListingTable, ListingTableConfig, ListingTableConfigExt};
use datafusion::parquet::arrow::async_reader::{AsyncFileReader, ParquetObjectReader};
use datafusion::parquet::file::metadata::ParquetMetaData;
use object_store::ObjectStoreExt;
use datafusion::prelude::{SessionConfig, SessionContext};

use crate::ipc::{FileMetadata, RowGroupInfo, RunQueryResponse, SchemaField};
use crate::storage::DataSource;

/// Owns the DataFusion `SessionContext` and optional result caches.
///
/// Created once per opened file; invalidated (reset) when a new file is opened.
pub struct QueryEngine {
    ctx: SessionContext,
    /// Cached Arrow schema from the last successful `register_source` call.
    /// Populated synchronously during registration so `get_schema` can be called without async.
    cached_schema: Option<SchemaRef>,
    /// Cached JSON result rows — reserved from Plan 01 layout; cleared on each `execute` call.
    pub result_cache: Option<Vec<serde_json::Value>>,
    /// Cached RecordBatches from the last `execute` call (at most 100 rows).
    /// Used by `get_page` for slicing without re-executing the query.
    pub result_batch_cache: Option<Vec<RecordBatch>>,
    /// Metadata from the last `execute` call — `total_rows` and the authoritative `capped` flag.
    /// Stored so `get_last_result_meta` can return it without re-inspecting the batch cache.
    pub last_query_meta: Option<RunQueryResponse>,
    /// Cached Parquet footer metadata — populated during `register_source` (Phase 3).
    ///
    /// Contains the total row count (`total_rows`) and per-row-group stats (`row_groups`).
    /// Available immediately after registration without re-reading the file.
    pub cached_file_metadata: Option<FileMetadata>,
}

impl QueryEngine {
    /// Creates a new `QueryEngine` with a fresh `SessionContext`.
    ///
    /// Disables `schema_force_view_types` so Parquet string columns are read as
    /// `Utf8` / `Binary` instead of `Utf8View` / `BinaryView`. The apache-arrow JS
    /// IPC decoder used on the frontend does not recognise view types (Arrow type id 24),
    /// so leaving the default `true` would cause `tableFromIPC` to throw
    /// `"Unrecognized type: undefined (24)"` and leave the results grid empty.
    pub fn new() -> Self {
        let mut config = SessionConfig::new();
        config.options_mut().execution.parquet.schema_force_view_types = false;
        Self {
            ctx: SessionContext::new_with_config(config),
            cached_schema: None,
            result_cache: None,
            result_batch_cache: None,
            last_query_meta: None,
            cached_file_metadata: None,
        }
    }

    /// Registers a data source as SQL table `data` in the `SessionContext`.
    ///
    /// Uses a build-then-swap (atomic) pattern: all fallible work (schema inference,
    /// table registration, footer read) is performed against a local `new_ctx`. Only after
    /// every step succeeds are `self.ctx`, `self.cached_schema`, and `self.cached_file_metadata`
    /// atomically replaced. A failed registration leaves the previous `data` table intact and
    /// fully queryable (D-04/D-05).
    ///
    /// The table name `"data"` is the well-known contract shared with the frontend (FILE-03).
    ///
    /// After successful registration, reads the Parquet footer via the object-store seam
    /// to populate `cached_file_metadata` (total row count + per-row-group stats).
    /// Uses `datafusion::parquet::*` re-exports — never a direct `parquet` crate dependency
    /// (PITFALLS.md §Pitfall 3). Does NOT open a local file handle directly.
    pub async fn register_source(&mut self, source: &dyn DataSource) -> Result<(), String> {
        // Build a fresh SessionContext in a local variable — do NOT touch self.ctx yet.
        // Preserve the same view-type config: schema_force_view_types = false (see new()).
        // Any ? early-return below leaves self.ctx / self.cached_schema / self.cached_file_metadata
        // untouched so the previous `data` table remains fully queryable (D-04/D-05).
        let mut new_session_config = SessionConfig::new();
        new_session_config
            .options_mut()
            .execution
            .parquet
            .schema_force_view_types = false;
        let new_ctx = SessionContext::new_with_config(new_session_config);

        // Register the object store against the local context (not self.ctx).
        // `register_object_store` takes `&Url`; `ObjectStoreUrl::as_ref()` gives us that.
        let object_store = source.object_store();
        new_ctx.register_object_store(source.url().as_ref(), Arc::clone(&object_store));

        // Infer format and schema from the file at table_path (reads only Parquet footer metadata).
        // ListingTableConfigExt::infer calls infer_options + infer_schema in sequence.
        let table_path = source.table_path()?;
        let config = ListingTableConfig::new(table_path.clone())
            .infer(&new_ctx.state())
            .await
            .map_err(|e| format!("Failed to infer table config: {}", e))?;

        // Extract the inferred schema into a local variable — do NOT assign self.cached_schema yet.
        let schema = config
            .file_schema
            .clone()
            .ok_or_else(|| "Schema inference produced no schema".to_string())?;

        // Build the ListingTable and register it as table `data` against the local context (FILE-03).
        let table = ListingTable::try_new(config)
            .map_err(|e| format!("Failed to create listing table: {}", e))?;

        new_ctx
            .register_table("data", Arc::new(table))
            .map_err(|e| format!("Failed to register table 'data': {}", e))?;

        // Read the Parquet footer via the object-store seam to populate file metadata cache.
        // This accesses row-group stats without opening a local file handle directly, keeping
        // the DataSource abstraction intact for future remote sources (CONTEXT.md §Integration).
        //
        // `table_path.prefix()` returns the `object_store::path::Path` that the object store
        // understands. We stat the object first to get its content-length (required by
        // `ParquetObjectReader::with_file_size` for efficient footer reads).
        let path = table_path.prefix().clone();
        let object_meta = object_store
            .head(&path)
            .await
            .map_err(|e| format!("Failed to stat parquet file: {}", e))?;

        // Build a ParquetObjectReader positioned at the file URL.
        // `with_file_size` lets the reader seek directly to the footer without an extra HEAD.
        let mut reader = ParquetObjectReader::new(Arc::clone(&object_store), object_meta.location)
            .with_file_size(object_meta.size);

        // Read only the footer (row-group metadata). `get_metadata` is defined on `AsyncFileReader`.
        // The returned `Arc<ParquetMetaData>` is cheap to clone and never reads row data.
        // Pass `None` for `ArrowReaderOptions` — we only need row-group metadata, not schema hints.
        let parquet_meta = reader
            .get_metadata(None)
            .await
            .map_err(|e| format!("Failed to read parquet footer metadata: {}", e))?;

        // Build the file metadata into a local variable — last fallible step before the swap.
        let file_metadata = build_file_metadata(parquet_meta.as_ref());

        // SWAP POINT — every fallible step above has succeeded.
        // This is the single mutation block: replace self.* atomically so the previous table
        // is either fully replaced (success path) or fully preserved (any prior ? early-return).
        self.ctx = new_ctx;
        self.result_cache = None;
        self.result_batch_cache = None;
        self.last_query_meta = None;
        self.cached_schema = Some(schema);
        self.cached_file_metadata = Some(file_metadata);

        Ok(())
    }

    /// Returns the Arrow schema of the registered `data` table as IPC-ready `SchemaField`s.
    ///
    /// Uses the schema cached during `register_source` — no SQL execution or async required.
    pub fn get_schema(&self) -> Result<Vec<SchemaField>, String> {
        let schema = self
            .cached_schema
            .as_ref()
            .ok_or_else(|| "No file registered. Call register_source first.".to_string())?;

        let fields = schema
            .fields()
            .iter()
            .map(|field| SchemaField {
                name: field.name().clone(),
                // Arrow-native type name; display formatting is the frontend's responsibility.
                arrow_type: format!("{:?}", field.data_type()),
                nullable: field.is_nullable(),
            })
            .collect();

        Ok(fields)
    }

    /// Returns cached Parquet footer metadata for the currently registered file.
    ///
    /// Uses the metadata cached during `register_source` — no async required.
    /// Returns `Err` if no file has been registered (mirrors `get_schema`).
    pub fn get_file_metadata(&self) -> Result<FileMetadata, String> {
        self.cached_file_metadata
            .clone()
            .ok_or_else(|| "No file registered. Call register_source first.".to_string())
    }

    /// Provides read access to the underlying `SessionContext` for query execution (Plan 02).
    pub fn ctx(&self) -> &SessionContext {
        &self.ctx
    }
}

/// Builds a `FileMetadata` from the raw `ParquetMetaData` footer.
///
/// Maps each row group to a `RowGroupInfo` and sums `num_rows` into `total_rows`.
/// Guards against zero-column row groups (no `.unwrap()` / index panic — PITFALLS §9).
fn build_file_metadata(parquet_meta: &ParquetMetaData) -> FileMetadata {
    let row_groups: Vec<RowGroupInfo> = parquet_meta
        .row_groups()
        .iter()
        .map(|rg| {
            // Guard: use compression from column 0 if available; fall back to "UNKNOWN".
            let compression = if rg.num_columns() > 0 {
                format!("{:?}", rg.column(0).compression())
            } else {
                "UNKNOWN".to_string()
            };
            RowGroupInfo {
                num_rows: rg.num_rows(),
                total_byte_size: rg.total_byte_size(),
                compression,
            }
        })
        .collect();

    let total_rows: i64 = row_groups.iter().map(|rg| rg.num_rows).sum();

    FileMetadata {
        total_rows,
        row_groups,
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}
