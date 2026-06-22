//! SQL query executor with a hard 100-row stream-stop cap.
//!
//! Implements `execute` and `get_page` on `QueryEngine` (the struct from Plan 01).
//!
//! Key invariant: `execute` NEVER calls `.collect()` on the query DataFrame.
//! Instead it uses `execute_stream()` and breaks the drain loop once the
//! accumulated row count reaches 100 (PITFALLS.md §Pitfall 1, GRID-03).
//!
//! Results are cached as `Vec<RecordBatch>` on `QueryEngine.result_batch_cache`
//! (separate from the `result_cache: Option<Vec<serde_json::Value>>` reserved
//! in Plan 01). `get_page` slices from the cached rows.
//!
//! All Arrow types come via `datafusion::arrow::*` re-exports — no direct
//! `arrow` crate dependency (PITFALLS.md §Pitfall 3).

use std::sync::Arc;

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::compute::{cast, concat_batches};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use futures::StreamExt;

use crate::engine::QueryEngine;
use crate::ipc::{PageResponse, RunQueryResponse};
use crate::ipc::serializer::record_batches_to_ipc;

/// Maximum number of rows the executor will retain from any single query execution.
/// The backend stops pulling from the stream once this cap is reached (GRID-03).
const ROW_CAP: usize = 100;

impl QueryEngine {
    /// Executes `sql` against the registered `data` table with a hard 100-row stream-stop cap.
    ///
    /// Returns `(RunQueryResponse, Arrow IPC bytes)` on success.
    ///
    /// # Behavior
    ///
    /// 1. Builds a `DataFrame` via `SessionContext::sql`.
    /// 2. Streams batches via `execute_stream()` — never `collect()` (PITFALLS.md §Pitfall 1).
    /// 3. Accumulates rows until `ROW_CAP` is hit; slices the final batch to fit exactly.
    /// 4. Serializes retained batches to Arrow IPC bytes via `record_batches_to_ipc`.
    /// 5. Caches the retained batches for `get_page`.
    ///
    /// `result_cache` (the `Vec<serde_json::Value>` field from Plan 01) is cleared on each call.
    pub async fn execute(&mut self, sql: &str) -> Result<(RunQueryResponse, Vec<u8>), String> {
        // Clear caches from any prior execution.
        self.result_cache = None;
        self.result_batch_cache = None;
        self.last_query_meta = None;

        let df = self
            .ctx()
            .sql(sql)
            .await
            .map_err(|e| format!("SQL planning error: {}", e))?;

        let mut stream = df
            .execute_stream()
            .await
            .map_err(|e| format!("Failed to start query stream: {}", e))?;

        let mut retained_batches: Vec<RecordBatch> = Vec::new();
        let mut total_rows: usize = 0;
        let mut capped = false;

        // Drain batches from the stream, stopping as soon as we have ROW_CAP rows.
        // This is the key invariant: the full result set is NEVER materialized (PITFALLS.md §Pitfall 1).
        'drain: while let Some(batch_result) = stream.next().await {
            let batch = batch_result.map_err(|e| format!("Stream error: {}", e))?;

            let batch_rows = batch.num_rows();
            if batch_rows == 0 {
                continue;
            }

            let remaining_capacity = ROW_CAP.saturating_sub(total_rows);

            if batch_rows <= remaining_capacity {
                // Entire batch fits within the cap.
                total_rows += batch_rows;
                retained_batches.push(batch);
            } else {
                // Slice the batch to fill exactly up to the cap.
                let sliced = batch.slice(0, remaining_capacity);
                total_rows += remaining_capacity;
                retained_batches.push(sliced);
                capped = true;
                break 'drain;
            }

            if total_rows >= ROW_CAP {
                capped = true;
                break 'drain;
            }
        }

        // Belt-and-suspenders: downcast any view-typed columns (Utf8View, BinaryView) to
        // their non-view equivalents.  context.rs sets schema_force_view_types=false so
        // DataFusion does not upgrade plain Utf8→Utf8View on its own, but files that
        // store view types natively bypass that guard.  apache-arrow JS v21 cannot decode
        // Arrow type id 24 (Utf8View), so both the IPC stream and the page cache must
        // be view-type-free.  This call is a no-op (fast path) when no view types exist.
        let retained_batches = normalize_view_types(retained_batches)?;

        // Serialize the retained batches to Arrow IPC bytes for the binary IPC channel.
        let ipc_bytes = if retained_batches.is_empty() {
            Vec::new()
        } else {
            record_batches_to_ipc(&retained_batches)
                .map_err(|e| format!("IPC serialization error: {}", e))?
        };

        // Cache the retained batches for `get_page` calls.
        self.result_batch_cache = Some(retained_batches);

        let response = RunQueryResponse {
            total_rows,
            capped,
        };

        // Cache the metadata so `get_last_result_meta` can retrieve the authoritative `capped` flag.
        self.last_query_meta = Some(response.clone());

        Ok((response, ipc_bytes))
    }

    /// Returns a page of rows from the cached query result.
    ///
    /// The cache holds at most `ROW_CAP` rows from the last `execute` call.
    /// Rows are materialised from the cached `RecordBatch`es into `serde_json::Value`
    /// objects for JSON transfer (the page endpoint is for metadata/fallback, not bulk).
    pub fn get_page(&self, offset: usize, size: usize) -> Result<PageResponse, String> {
        let batches = self
            .result_batch_cache
            .as_ref()
            .ok_or_else(|| "No query results cached. Call run_query first.".to_string())?;

        if batches.is_empty() {
            return Ok(PageResponse {
                rows: Vec::new(),
                offset,
                has_more: false,
            });
        }

        // Concatenate all cached batches into a single batch for slicing.
        let schema = batches[0].schema();
        let combined = concat_batches(&schema, batches.iter())
            .map_err(|e| format!("Failed to concatenate cached batches: {}", e))?;

        let total = combined.num_rows();
        let actual_offset = offset.min(total);
        let actual_size = size.min(total.saturating_sub(actual_offset));
        let has_more = actual_offset + actual_size < total;

        // Slice the combined batch and convert to JSON row objects.
        let page_batch = combined.slice(actual_offset, actual_size);
        let rows = record_batch_to_json_rows(&page_batch)?;

        Ok(PageResponse {
            rows,
            offset: actual_offset,
            has_more,
        })
    }
}

/// Downcasts any Arrow view-typed columns in `batches` to their non-view equivalents.
///
/// # Why this exists
///
/// DataFusion 54 may produce result columns typed as `Utf8View` / `BinaryView` (Arrow
/// type id 24) when the source Parquet file stores them natively as view types.
/// The `schema_force_view_types = false` setting in `context.rs` stops DataFusion from
/// *upgrading* plain `Utf8` columns to `Utf8View`, but it cannot prevent the reader from
/// preserving the native view type that is already declared in the file's Arrow schema.
///
/// The frontend uses apache-arrow JS v21, which has **no** `Utf8View` support and throws
/// `"Unrecognized type: undefined (24)"` on any IPC stream that contains a view-typed
/// column, rendering an empty grid.  This function is the belt-and-suspenders layer that
/// guarantees neither the IPC bytes nor the `get_page` cache ever carry a view type.
///
/// # Mapping
///
/// | Source type      | Target type  |
/// |------------------|--------------|
/// | `Utf8View`       | `Utf8`       |
/// | `BinaryView`     | `Binary`     |
/// | everything else  | unchanged    |
///
/// # Fast path
///
/// If no column in `batches[0].schema()` is a view type the input `Vec` is returned
/// unchanged — no allocation, no casting.
///
/// # Errors
///
/// Returns `Err(String)` if Arrow's `cast` kernel fails or if `RecordBatch::try_new`
/// rejects the rebuilt batch.  Never panics.
pub fn normalize_view_types(batches: Vec<RecordBatch>) -> Result<Vec<RecordBatch>, String> {
    if batches.is_empty() {
        return Ok(Vec::new());
    }

    let orig_schema = batches[0].schema();

    // Build the list of target field types, mapping view types to non-view equivalents.
    let mut any_changed = false;
    let new_fields: Vec<Field> = orig_schema
        .fields()
        .iter()
        .map(|f| {
            let target_dt = match f.data_type() {
                DataType::Utf8View => {
                    any_changed = true;
                    DataType::Utf8
                }
                DataType::BinaryView => {
                    any_changed = true;
                    DataType::Binary
                }
                other => other.clone(),
            };
            Field::new(f.name(), target_dt, f.is_nullable())
        })
        .collect();

    // Fast path: no view-typed columns found.
    if !any_changed {
        return Ok(batches);
    }

    let new_schema: SchemaRef = Arc::new(Schema::new(new_fields));

    // Rebuild each batch, casting only the columns whose type changed.
    batches
        .into_iter()
        .map(|batch| {
            let new_columns: Vec<_> = orig_schema
                .fields()
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let col = batch.column(i);
                    let target_dt = new_schema.field(i).data_type();
                    if target_dt != f.data_type() {
                        cast(col.as_ref(), target_dt)
                            .map_err(|e| format!("cast error on column '{}': {}", f.name(), e))
                    } else {
                        // No cast needed — clone the Arc cheaply.
                        Ok(Arc::clone(col))
                    }
                })
                .collect::<Result<_, String>>()?;

            RecordBatch::try_new(Arc::clone(&new_schema), new_columns)
                .map_err(|e| format!("RecordBatch rebuild error: {}", e))
        })
        .collect()
}

/// Converts a `RecordBatch` into a `Vec<serde_json::Value>` row objects.
///
/// Uses DataFusion's Arrow JSON writer re-export (PITFALLS.md §Pitfall 3).
fn record_batch_to_json_rows(
    batch: &RecordBatch,
) -> Result<Vec<serde_json::Value>, String> {
    use datafusion::arrow::json::ArrayWriter;

    if batch.num_rows() == 0 {
        return Ok(Vec::new());
    }

    let mut buf = Vec::new();
    {
        let mut writer = ArrayWriter::new(&mut buf);
        writer
            .write(batch)
            .map_err(|e| format!("JSON write error: {}", e))?;
        writer
            .finish()
            .map_err(|e| format!("JSON finish error: {}", e))?;
    }

    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&buf).map_err(|e| format!("JSON parse error: {}", e))?;

    Ok(rows)
}
