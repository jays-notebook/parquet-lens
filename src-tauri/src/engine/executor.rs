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
use datafusion::arrow::compute::{can_cast_types, cast, concat_batches};
use datafusion::arrow::datatypes::{DataType, Field, FieldRef, Schema, SchemaRef};
use datafusion::execution::context::SQLOptions;
use futures::StreamExt;

use crate::engine::QueryEngine;
use crate::ipc::{PageResponse, RunQueryResponse, SchemaField};
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
    /// 1. Builds a `DataFrame` via `SessionContext::sql_with_options` with DDL, DML and
    ///    statements all disallowed — the plan-level backstop behind the AST guard in
    ///    `commands/query.rs` (WR-04).
    /// 2. Streams batches via `execute_stream()` — never `collect()` (PITFALLS.md §Pitfall 1).
    /// 3. Captures the RESULT schema from the stream, before draining it, so the frontend
    ///    can build grid columns from the result rather than the opened file (D-PH1-01).
    /// 4. Accumulates rows until `ROW_CAP` is hit; slices the final batch to fit exactly.
    /// 5. Serializes retained batches to Arrow IPC bytes via `record_batches_to_ipc`.
    /// 6. Caches the retained batches for `get_page`.
    ///
    /// # The `capped` flag
    ///
    /// `capped` means **"the result was truncated"** — not "the result reached the cap".
    /// A complete result of exactly `ROW_CAP` rows (e.g. `select * from data limit 100`,
    /// the app's own default query) reports `capped: false`, because the backend discarded
    /// nothing. It is set only when rows were provably dropped: either the final batch had
    /// to be sliced, or a bounded peek found another non-empty batch waiting in the stream
    /// (WR-06).
    ///
    /// `result_cache` (the `Vec<serde_json::Value>` field from Plan 01) is cleared on each call.
    pub async fn execute(&mut self, sql: &str) -> Result<(RunQueryResponse, Vec<u8>), String> {
        // Clear caches from any prior execution.
        self.result_cache = None;
        self.result_batch_cache = None;
        self.last_query_meta = None;

        // Defence in depth for WR-04. `sql_with_options` runs `verify_plan` over the
        // built `LogicalPlan` and rejects `Ddl`, `Dml`, `Copy` and `Statement` nodes, so a
        // write-shaped plan — `CreateMemoryTable` from `SELECT ... INTO`, or any future
        // write-shaped construct — is refused at PLAN level even if the AST guard in
        // `commands/query.rs` is ever bypassed, including on a direct IPC call that reaches
        // the engine another way. All three flags default to `true`, so each one must be
        // set explicitly; omitting any of them silently restores the hole.
        let opts = SQLOptions::new()
            .with_allow_ddl(false)
            .with_allow_dml(false)
            .with_allow_statements(false);

        let df = self
            .ctx()
            .sql_with_options(sql, opts)
            .await
            .map_err(|e| format!("SQL planning error: {}", e))?;

        let mut stream = df
            .execute_stream()
            .await
            .map_err(|e| format!("Failed to start query stream: {}", e))?;

        // Capture the RESULT schema before the 'drain loop consumes the stream.
        //
        // This is taken PRE-normalization on purpose: `normalize_view_types` below
        // downcasts Utf8View→Utf8 (and BinaryView→Binary) purely so apache-arrow JS v21
        // can decode the IPC bytes, while `get_schema` reports the file's UN-normalized
        // types. Labelling from the normalized schema would make a natively-view-typed
        // column read `Utf8View` in the sidebar but `Utf8` in the grid (D-PH1-02).
        //
        // Reading the schema from the stream (rather than the retained batches) also means
        // a zero-row result still reports its full column list.
        let result_schema = stream.schema();
        let schema: Vec<SchemaField> = result_schema
            .fields()
            .iter()
            .map(|field| SchemaField {
                name: field.name().clone(),
                // MUST stay character-for-character identical to the expression in
                // `context.rs::get_schema` — otherwise `select *` grid headers would
                // diverge from the sidebar's file-schema labels (D-PH1-01).
                arrow_type: format!("{:?}", field.data_type()),
                nullable: field.is_nullable(),
            })
            .collect();

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
                // We filled to exactly ROW_CAP without discarding anything. Whether the
                // result is TRUNCATED depends on whether the stream still has rows, so
                // peek before claiming it (WR-06). Zero-row batches are skipped because a
                // stream may legitimately emit trailing empty batches before ending —
                // those carry no rows and therefore prove no truncation.
                //
                // This is not a materialization violation (PITFALLS.md §Pitfall 1 holds):
                // the peek pulls at most the already-scheduled next batches, stops at the
                // first non-empty one, and DROPS every batch it pulls. Nothing peeked is
                // ever pushed into `retained_batches`, so `total_rows` stays exactly
                // ROW_CAP and the full result is still never accumulated.
                let mut more = false;
                while let Some(peeked_result) = stream.next().await {
                    let peeked = match peeked_result {
                        Ok(batch) => batch,
                        Err(_) => {
                            // WR-02 (phase 02 review): rows past the cap would have been
                            // DROPPED regardless, so an error reading them cannot invalidate
                            // the 100 rows already retained — the slice branch above never
                            // even observes errors past the cap, and the two paths must not
                            // succeed or fail differently depending only on how the source
                            // chunked its batches. Report capped=true conservatively: we
                            // cannot prove the result was complete.
                            more = true;
                            break;
                        }
                    };
                    if peeked.num_rows() > 0 {
                        more = true;
                        break;
                    }
                }

                capped = more;
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
            schema,
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
/// The rewrite applies at **every nesting depth**, not only at the top level (WR-05).
/// A `List(Utf8View)` child blanks the grid exactly like a top-level `Utf8View` column
/// does, so a top-level-only match would leave the stated guarantee false.
///
/// | Source type      | Target type  |
/// |------------------|--------------|
/// | `Utf8View`       | `Utf8`       |
/// | `BinaryView`     | `Binary`     |
/// | everything else  | unchanged    |
///
/// Container types are recursed into and rebuilt around their rewritten children:
/// `List`, `LargeList`, `ListView`, `LargeListView`, `FixedSizeList` (size preserved),
/// `Struct` (unchanged children kept as-is), `Map` (`sorted` flag preserved),
/// `Dictionary` (key type preserved, value type rewritten) and `RunEndEncoded`.
///
/// # Metadata
///
/// Field-level and schema-level Arrow metadata survive the rewrite (IN-01): fields are
/// rebuilt with `Field::with_data_type` (which keeps name, nullability AND metadata) and
/// the schema with `Schema::new_with_metadata`.  Previously a rewrite triggered by one
/// view column silently stripped metadata from every other column too.
///
/// # Fast path
///
/// If no column in `batches[0].schema()` contains a view type at any depth the input
/// `Vec` is returned unchanged — no allocation, no casting.
///
/// # Errors
///
/// Returns `Err(String)` if the Arrow cast kernel does not support a required cast
/// (checked up-front with `can_cast_types`, naming the column and both types), if the
/// `cast` kernel itself fails, or if `RecordBatch::try_new` rejects the rebuilt batch.
/// Failing loudly is deliberate: silently shipping a view-typed column produces
/// `"Unrecognized type: undefined (24)"` in apache-arrow JS v21 and an empty grid with
/// no explanation.  Never panics.
pub fn normalize_view_types(batches: Vec<RecordBatch>) -> Result<Vec<RecordBatch>, String> {
    if batches.is_empty() {
        return Ok(Vec::new());
    }

    let orig_schema = batches[0].schema();

    // Build the target fields, rewriting view types at any depth.
    // `view_free_field` returns None when the field is already view-free.
    let mut any_changed = false;
    let new_fields: Vec<Field> = orig_schema
        .fields()
        .iter()
        .map(|f| match view_free_field(f) {
            Some(rewritten) => {
                any_changed = true;
                rewritten
            }
            None => f.as_ref().clone(),
        })
        .collect();

    // Fast path: no view-typed columns found at any depth.
    if !any_changed {
        return Ok(batches);
    }

    // new_with_metadata (not Schema::new) so schema-level key/value pairs survive (IN-01).
    let new_schema: SchemaRef = Arc::new(Schema::new_with_metadata(
        new_fields,
        orig_schema.metadata().clone(),
    ));

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
                        // Guard before casting: an unsupported nested cast must surface as
                        // a named error, never as undecodable IPC bytes on the wire.
                        if !can_cast_types(f.data_type(), target_dt) {
                            return Err(format!(
                                "cannot normalize view type on column '{}': no Arrow cast from {:?} to {:?}",
                                f.name(),
                                f.data_type(),
                                target_dt
                            ));
                        }
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

/// Returns the view-free rewrite of `dt`, or `None` when `dt` already contains no
/// view type at any depth.
///
/// `None` (rather than an unconditional clone) is what powers the fast path in
/// `normalize_view_types`: an unchanged type costs no allocation and no cast.
fn view_free_data_type(dt: &DataType) -> Option<DataType> {
    match dt {
        DataType::Utf8View => Some(DataType::Utf8),
        DataType::BinaryView => Some(DataType::Binary),

        // Single-child list containers: rebuild the same variant around the rewritten child.
        DataType::List(f) => view_free_field(f).map(|nf| DataType::List(Arc::new(nf))),
        DataType::LargeList(f) => view_free_field(f).map(|nf| DataType::LargeList(Arc::new(nf))),
        DataType::ListView(f) => view_free_field(f).map(|nf| DataType::ListView(Arc::new(nf))),
        DataType::LargeListView(f) => {
            view_free_field(f).map(|nf| DataType::LargeListView(Arc::new(nf)))
        }
        // Fixed size must be carried over verbatim.
        DataType::FixedSizeList(f, size) => {
            view_free_field(f).map(|nf| DataType::FixedSizeList(Arc::new(nf), *size))
        }

        // Struct: rebuild only if at least one child changed; unchanged children are
        // reused by Arc clone so their metadata is untouched.
        DataType::Struct(fields) => {
            let mut any_changed = false;
            let new_fields: Vec<FieldRef> = fields
                .iter()
                .map(|f| match view_free_field(f) {
                    Some(rewritten) => {
                        any_changed = true;
                        Arc::new(rewritten)
                    }
                    None => Arc::clone(f),
                })
                .collect();

            if any_changed {
                Some(DataType::Struct(new_fields.into()))
            } else {
                None
            }
        }

        // Map: the `sorted` flag is part of the type and must be preserved.
        DataType::Map(entries, sorted) => {
            view_free_field(entries).map(|nf| DataType::Map(Arc::new(nf), *sorted))
        }

        // Dictionary: only the value type can be a view type; keep the key type as-is.
        DataType::Dictionary(key, value) => view_free_data_type(value)
            .map(|nv| DataType::Dictionary(key.clone(), Box::new(nv))),

        DataType::RunEndEncoded(run_ends, values) => view_free_field(values)
            .map(|nf| DataType::RunEndEncoded(Arc::clone(run_ends), Arc::new(nf))),

        _ => None,
    }
}

/// Returns the view-free rewrite of `f`, or `None` when its type is already view-free.
///
/// Uses `Field::with_data_type` specifically: it preserves the field's name, nullability
/// AND metadata.  The previous `Field::new(f.name(), dt, f.is_nullable())` construction
/// silently dropped field metadata (IN-01).
fn view_free_field(f: &Field) -> Option<Field> {
    view_free_data_type(f.data_type()).map(|new_dt| f.clone().with_data_type(new_dt))
}

/// Converts a `RecordBatch` into a `Vec<serde_json::Value>` row objects.
///
/// Uses DataFusion's Arrow JSON writer re-export (PITFALLS.md §Pitfall 3).
///
/// # Duplicate column names (phase 02 review, WR-03)
///
/// `ArrayWriter` keys cells by the raw column name, so a result with duplicate
/// names — `select a, b as a from data` — produced JSON objects with duplicate
/// keys, and `serde_json` kept only the LAST occurrence: one column's values
/// silently replaced the other's. Column names are therefore disambiguated
/// positionally BEFORE serialization, with the same `__N`-suffix scheme the
/// frontend applies to the Arrow IPC channel (`dedupeFieldKeys` in
/// `src/lib/tauri.ts`, D-PH1-03), so both row channels expose duplicate columns
/// identically instead of dropping data.
fn record_batch_to_json_rows(
    batch: &RecordBatch,
) -> Result<Vec<serde_json::Value>, String> {
    use datafusion::arrow::json::ArrayWriter;

    if batch.num_rows() == 0 {
        return Ok(Vec::new());
    }

    // Rebuild the batch under deduped column names when (and only when) the
    // result schema contains duplicates (WR-03). Types, nullability, metadata
    // and the column arrays themselves are untouched — only names change.
    let schema = batch.schema();
    let names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let deduped = dedupe_field_names(&names);

    let renamed_batch: RecordBatch;
    let write_batch: &RecordBatch = if deduped == names {
        batch
    } else {
        let new_fields: Vec<Field> = schema
            .fields()
            .iter()
            .zip(deduped.iter())
            .map(|(f, name)| f.as_ref().clone().with_name(name))
            .collect();
        let new_schema = Arc::new(Schema::new_with_metadata(
            new_fields,
            schema.metadata().clone(),
        ));
        renamed_batch = RecordBatch::try_new(new_schema, batch.columns().to_vec())
            .map_err(|e| format!("Failed to rebuild page batch with deduped names: {}", e))?;
        &renamed_batch
    };

    let mut buf = Vec::new();
    {
        let mut writer = ArrayWriter::new(&mut buf);
        writer
            .write(write_batch)
            .map_err(|e| format!("JSON write error: {}", e))?;
        writer
            .finish()
            .map_err(|e| format!("JSON finish error: {}", e))?;
    }

    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&buf).map_err(|e| format!("JSON parse error: {}", e))?;

    Ok(rows)
}

/// Returns positionally deduped column names: the first occurrence of a name is
/// kept verbatim, later occurrences get `__2`, `__3`, ... suffixes, probing
/// forward when a source column literally carries an already-taken probed name.
///
/// MUST stay behaviourally identical to `dedupeFieldKeys` in `src/lib/tauri.ts`
/// (D-PH1-03): the frontend applies this scheme to the Arrow IPC channel, and the
/// `get_page` JSON channel must expose the same keys for the same result (WR-03).
fn dedupe_field_names(names: &[String]) -> Vec<String> {
    use std::collections::{HashMap, HashSet};

    // Occurrence counter per source name, and every key already handed out.
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();

    names
        .iter()
        .map(|name| {
            let mut count = seen.get(name.as_str()).copied().unwrap_or(0) + 1;
            let mut key = if count == 1 {
                name.clone()
            } else {
                format!("{name}__{count}")
            };
            // Probe forward: a source column may literally be named `{name}__{count}`.
            while used.contains(&key) {
                count += 1;
                key = format!("{name}__{count}");
            }
            seen.insert(name.as_str(), count);
            used.insert(key.clone());
            key
        })
        .collect()
}
