//! Behavior tests for the query execution vertical slice (Plan 02, Task 1).
//!
//! Tests legitimately use unwrap/expect for assertions — idiomatic in Rust test code.
#![allow(clippy::unwrap_used)]
//!
//! Test 1 (cap): against a >100-row fixture, `execute("select * from data")` returns
//!   `total_rows == 100, capped == true`.
//! Test 2 (under cap): against a <100-row fixture, `execute("select * from data")` returns
//!   `total_rows == 5, capped == false`.
//! Test 3 (limit interaction): `execute("select * from data limit 1000")` still returns
//!   `total_rows == 100, capped == true` — the app cap is independent of the query LIMIT.
//! Test 4 (no full materialization): executor uses `execute_stream()` with a row-count break;
//!   verified via source inspection in the acceptance criteria (no `.collect()` in executor.rs).
//! Test 5 (IPC round-trip): `record_batches_to_ipc(&batches)` produces a non-empty `Vec<u8>`
//!   that begins with the Arrow IPC magic bytes (decodable by an Arrow IPC reader).

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::{Array, Float64Array, Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;

use parquet_lens_lib::engine::QueryEngine;
use parquet_lens_lib::ipc::serializer::record_batches_to_ipc;
use parquet_lens_lib::storage::LocalFileSource;

/// Path to the large test fixture (>100 rows).
const LARGE_FIXTURE_PATH: &str = "tests/fixtures/large_sample.parquet";
/// Path to the small test fixture (<100 rows).
const SMALL_FIXTURE_PATH: &str = "tests/fixtures/sample.parquet";

/// Creates a large Parquet fixture with 150 rows (3 columns: id Int64, label Utf8, score Float64).
fn ensure_large_fixture() -> PathBuf {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(LARGE_FIXTURE_PATH);

    if !fixture_path.exists() {
        create_large_fixture(&fixture_path);
    }

    fixture_path
}

/// Writes a 150-row Parquet fixture for cap-related tests.
fn create_large_fixture(path: &PathBuf) {
    use datafusion::parquet::arrow::ArrowWriter;
    use std::fs::File;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("label", DataType::Utf8, true),
        Field::new("score", DataType::Float64, true),
    ]));

    const ROW_COUNT: usize = 150;

    let ids: Vec<i64> = (1..=ROW_COUNT as i64).collect();
    let labels: Vec<Option<&str>> = (0..ROW_COUNT)
        .map(|i| {
            if i % 20 == 0 {
                None // Introduce some NULLs to test null handling
            } else {
                Some("row")
            }
        })
        .collect();
    let scores: Vec<Option<f64>> = (0..ROW_COUNT)
        .map(|i| {
            if i % 15 == 0 {
                None
            } else {
                Some(i as f64 * 1.5)
            }
        })
        .collect();

    let id_arr = Arc::new(Int64Array::from(ids));
    let label_arr = Arc::new(StringArray::from(labels));
    let score_arr = Arc::new(Float64Array::from(scores));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![id_arr, label_arr, score_arr],
    )
    .unwrap();

    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

/// Returns the path to the small (5-row) fixture from Plan 01.
fn small_fixture_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SMALL_FIXTURE_PATH);
    // The small fixture is created by Plan 01's storage_engine_tests.rs ensure_fixture().
    // If it doesn't exist yet, create it here too.
    if !path.exists() {
        create_small_fixture(&path);
    }
    path
}

fn create_small_fixture(path: &PathBuf) {
    use datafusion::parquet::arrow::ArrowWriter;
    use std::fs::File;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("value", DataType::Int64, true),
    ]));

    let ids = Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5]));
    let names = Arc::new(StringArray::from(vec![
        Some("alice"),
        Some("bob"),
        None,
        Some("dave"),
        Some("eve"),
    ]));
    let values = Arc::new(Int64Array::from(vec![
        Some(100),
        Some(200),
        None,
        Some(400),
        Some(500),
    ]));

    let batch = RecordBatch::try_new(schema.clone(), vec![ids, names, values]).unwrap();

    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

// ---------------------------------------------------------------------------
// Test 1: 100-row cap against a >100-row fixture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_execute_caps_at_100_rows() {
    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, ipc_bytes) = engine
        .execute("select * from data")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.total_rows, 100,
        "execute() must cap at 100 rows, got {}",
        meta.total_rows
    );
    assert!(
        meta.capped,
        "capped must be true when 150-row fixture is queried without LIMIT"
    );
    assert!(
        !ipc_bytes.is_empty(),
        "IPC bytes must be non-empty for a 100-row result"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Under-cap result — small fixture with 5 rows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_execute_under_cap_returns_all_rows() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select * from data")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.total_rows, 5,
        "execute() must return all 5 rows from a 5-row fixture, got {}",
        meta.total_rows
    );
    assert!(
        !meta.capped,
        "capped must be false when result is under the 100-row cap"
    );
}

// ---------------------------------------------------------------------------
// Test 3: LIMIT in query does NOT override the backend cap
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_execute_limit_1000_still_caps_at_100() {
    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    // The user-specified LIMIT 1000 is larger than the 100-row cap;
    // the backend must still return at most 100 rows (GRID-03).
    let (meta, _ipc_bytes) = engine
        .execute("select * from data limit 1000")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.total_rows, 100,
        "Backend cap must override user LIMIT 1000; expected 100 rows, got {}",
        meta.total_rows
    );
    assert!(
        meta.capped,
        "capped must be true when backend cap is hit regardless of query LIMIT"
    );
}

// ---------------------------------------------------------------------------
// Test 5 (IPC round-trip): record_batches_to_ipc produces valid Arrow IPC bytes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_record_batches_to_ipc_produces_valid_arrow_ipc() {
    use datafusion::arrow::ipc::reader::StreamReader;
    use std::io::Cursor;

    // Build a small in-memory RecordBatch.
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", DataType::Int64, false),
        Field::new("y", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])),
            Arc::new(StringArray::from(vec![Some("a"), None, Some("c")])),
        ],
    )
    .unwrap();

    // Serialize to Arrow IPC bytes.
    let ipc_bytes = record_batches_to_ipc(std::slice::from_ref(&batch))
        .expect("record_batches_to_ipc must succeed");

    assert!(
        !ipc_bytes.is_empty(),
        "IPC bytes must be non-empty for a non-empty RecordBatch"
    );

    // Verify the bytes begin with the Arrow IPC stream magic ("ARROW1\0\0").
    // The Arrow IPC stream format always starts with the continuation/magic marker.
    // The exact magic is embedded in the schema message; the first 4 bytes are the
    // continuation marker [0xFF, 0xFF, 0xFF, 0xFF] in the modern IPC format.
    assert!(
        ipc_bytes.len() >= 8,
        "IPC bytes must be at least 8 bytes long, got {}",
        ipc_bytes.len()
    );

    // Decode the IPC bytes back to a RecordBatch using the Arrow IPC StreamReader.
    let cursor = Cursor::new(ipc_bytes);
    let mut reader = StreamReader::try_new(cursor, None)
        .expect("StreamReader must parse valid Arrow IPC bytes");

    let decoded_batch = reader
        .next()
        .expect("StreamReader must yield at least one batch")
        .expect("Batch read must succeed");

    assert_eq!(
        decoded_batch.num_rows(),
        batch.num_rows(),
        "Decoded batch must have the same row count as the original"
    );
    assert_eq!(
        decoded_batch.num_columns(),
        batch.num_columns(),
        "Decoded batch must have the same column count as the original"
    );
}

// ---------------------------------------------------------------------------
// Regression: executed result schema must contain NO Utf8View / BinaryView
// (guards apache-arrow JS IPC compatibility — DF54 defaults schema_force_view_types=true)
// ---------------------------------------------------------------------------

/// Asserts that QueryEngine never returns Utf8View or BinaryView columns.
///
/// DataFusion 54 defaults `schema_force_view_types = true`, which causes the
/// Parquet reader to upgrade `Utf8`/`Binary` columns to `Utf8View`/`BinaryView`.
/// The apache-arrow JS IPC decoder (v21) does not recognise Arrow type id 24
/// (`Utf8View`) and throws `"Unrecognized type: undefined (24)"`, leaving the
/// results grid empty. QueryEngine::new() disables that option so this test
/// must pass: no view types in the result schema.
#[tokio::test]
async fn test_execute_schema_has_no_view_types() {
    use datafusion::arrow::datatypes::DataType;

    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (_meta, _ipc_bytes) = engine
        .execute("select * from data")
        .await
        .expect("execute must succeed");

    // Inspect every field in the result batch schema.
    let batches = engine
        .result_batch_cache
        .as_ref()
        .expect("result_batch_cache must be populated after execute");

    assert!(
        !batches.is_empty(),
        "at least one batch must be returned for the 150-row fixture"
    );

    let schema = batches[0].schema();
    for field in schema.fields() {
        let dt = field.data_type();
        assert_ne!(
            dt,
            &DataType::Utf8View,
            "field '{}' must not be Utf8View — apache-arrow JS cannot decode it",
            field.name()
        );
        assert_ne!(
            dt,
            &DataType::BinaryView,
            "field '{}' must not be BinaryView — apache-arrow JS cannot decode it",
            field.name()
        );
        // Also guard large-string variants.
        assert_ne!(
            dt,
            &DataType::LargeUtf8,
            "field '{}' is LargeUtf8 — prefer Utf8 for JS compatibility",
            field.name()
        );
    }
}

// ---------------------------------------------------------------------------
// Task 1 (Phase 3): get_file_metadata returns total_rows and row_groups
// ---------------------------------------------------------------------------

/// Asserts that `get_file_metadata` returns the total row count from the Parquet footer
/// and at least one row group with a non-empty compression string.
///
/// Behavior assertions (03-01-PLAN.md §behavior):
///   - total_rows == sum of row-group num_rows (== 150 for the large fixture)
///   - row_groups is non-empty, each entry has a non-empty compression string
///   - calling before register_source returns Err
#[tokio::test]
async fn get_file_metadata_returns_total_rows_and_row_groups() {
    // Setup: register the large fixture (150 rows).
    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();

    // Before registration: must return Err.
    let before = engine.get_file_metadata();
    assert!(
        before.is_err(),
        "get_file_metadata before register_source must return Err, got Ok"
    );

    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let meta = engine.get_file_metadata().expect("get_file_metadata must succeed after register_source");

    // The large fixture has exactly 150 rows.
    assert_eq!(
        meta.total_rows, 150,
        "total_rows must equal the fixture row count (150), got {}",
        meta.total_rows
    );

    // The fixture is written as a single batch => at least one row group.
    assert!(
        !meta.row_groups.is_empty(),
        "row_groups must be non-empty for a valid Parquet file"
    );

    // Every row group must report a non-empty compression string.
    for (i, rg) in meta.row_groups.iter().enumerate() {
        assert!(
            !rg.compression.is_empty(),
            "row_group[{}] compression must be non-empty",
            i
        );
        assert!(
            rg.num_rows > 0,
            "row_group[{}] num_rows must be > 0, got {}",
            i, rg.num_rows
        );
    }

    // total_rows must equal sum of per-group num_rows.
    let sum: i64 = meta.row_groups.iter().map(|rg| rg.num_rows).sum();
    assert_eq!(
        meta.total_rows, sum,
        "total_rows ({}) must equal sum of row_group num_rows ({})",
        meta.total_rows, sum
    );
}

// ---------------------------------------------------------------------------
// Task 1 (quick-na5): normalize_view_types downcasts Utf8View → Utf8
// ---------------------------------------------------------------------------

/// Regression test: normalize_view_types must downcast view-typed columns to
/// their non-view equivalents so the apache-arrow JS IPC decoder (v21) can
/// parse the result without throwing "Unrecognized type: undefined (24)".
///
/// Verifies:
///   - Utf8View column is downcast to Utf8 (schema + values preserved)
///   - Int64 passthrough column is unchanged
///   - A batch with no view types passes through unchanged (no error)
#[test]
fn normalize_view_types_downcasts_utf8view_to_utf8() {
    use datafusion::arrow::array::StringViewArray;
    use datafusion::arrow::datatypes::DataType;

    // Build a RecordBatch with a Utf8View column and an Int64 passthrough column.
    let schema = Arc::new(Schema::new(vec![
        Field::new("name", DataType::Utf8View, true),
        Field::new("id", DataType::Int64, false),
    ]));

    let name_arr: Arc<dyn datafusion::arrow::array::Array> = Arc::new(
        StringViewArray::from(vec![Some("alice"), Some("bob"), None]),
    );
    let id_arr: Arc<dyn datafusion::arrow::array::Array> =
        Arc::new(Int64Array::from(vec![1i64, 2, 3]));

    let batch = RecordBatch::try_new(schema, vec![name_arr, id_arr]).unwrap();

    // Call normalize_view_types — must succeed.
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    assert_eq!(result.len(), 1, "must return one batch");
    let out = &result[0];

    // Field 0: Utf8View must be downcast to Utf8.
    assert_eq!(
        out.schema().field(0).data_type(),
        &DataType::Utf8,
        "Utf8View must be normalized to Utf8"
    );

    // Field 1: Int64 must be unchanged.
    assert_eq!(
        out.schema().field(1).data_type(),
        &DataType::Int64,
        "Int64 must pass through unchanged"
    );

    // Values must be preserved after downcast.
    use datafusion::arrow::array::StringArray;
    let name_col = out
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 0 must be a StringArray after normalization");
    assert_eq!(name_col.value(0), "alice");
    assert_eq!(name_col.value(1), "bob");
    assert!(name_col.is_null(2), "null must survive the downcast");
}

/// Fast-path: a batch with no view-typed columns must pass through unchanged.
#[test]
fn normalize_view_types_passthrough_non_view_batch() {
    use datafusion::arrow::datatypes::DataType;

    let schema = Arc::new(Schema::new(vec![
        Field::new("x", DataType::Int64, false),
        Field::new("y", DataType::Utf8, true),
    ]));

    let x_arr: Arc<dyn datafusion::arrow::array::Array> =
        Arc::new(Int64Array::from(vec![10i64, 20]));
    let y_arr: Arc<dyn datafusion::arrow::array::Array> = Arc::new(
        datafusion::arrow::array::StringArray::from(vec![Some("foo"), Some("bar")]),
    );

    let batch = RecordBatch::try_new(schema.clone(), vec![x_arr, y_arr]).unwrap();
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].schema().field(0).data_type(),
        &DataType::Int64
    );
    assert_eq!(
        result[0].schema().field(1).data_type(),
        &DataType::Utf8
    );
}

// ---------------------------------------------------------------------------
// Bonus: get_page returns correct slice from cached result
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_page_slices_from_cached_result() {
    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    // Execute to populate the cache.
    engine
        .execute("select * from data")
        .await
        .expect("execute must succeed");

    // Fetch page 0 (first 10 rows).
    let page = engine
        .get_page(0, 10)
        .expect("get_page must succeed after execute");

    assert_eq!(page.rows.len(), 10, "First page must contain 10 rows");
    assert_eq!(page.offset, 0);
    assert!(page.has_more, "has_more must be true (100 total, only 10 fetched)");

    // Fetch page starting at offset 95 (last 5 rows of the 100-row cap).
    let last_page = engine
        .get_page(95, 10)
        .expect("get_page at offset 95 must succeed");

    assert_eq!(
        last_page.rows.len(),
        5,
        "Last page must contain 5 rows (95..100), got {}",
        last_page.rows.len()
    );
    assert!(!last_page.has_more, "has_more must be false at the end of the 100-row cache");
}
