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
/// Path to the exactly-at-the-cap fixture (exactly ROW_CAP == 100 rows).
const EXACT_100_FIXTURE_PATH: &str = "tests/fixtures/exact_100.parquet";

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

/// Creates a Parquet fixture holding EXACTLY 100 rows — the same count as the
/// executor's `ROW_CAP`. Used to prove that a *complete* result which happens to
/// land exactly on the cap is not reported as truncated (WR-06).
///
/// Same three columns as the large fixture (id Int64 non-null, label Utf8
/// nullable, score Float64 nullable) so the two are interchangeable in queries.
fn ensure_exact_100_fixture() -> PathBuf {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(EXACT_100_FIXTURE_PATH);

    if !fixture_path.exists() {
        create_exact_100_fixture(&fixture_path);
    }

    fixture_path
}

/// Writes a 100-row Parquet fixture for cap-accuracy tests.
fn create_exact_100_fixture(path: &PathBuf) {
    use datafusion::parquet::arrow::ArrowWriter;
    use std::fs::File;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("label", DataType::Utf8, true),
        Field::new("score", DataType::Float64, true),
    ]));

    const ROW_COUNT: usize = 100;

    let ids: Vec<i64> = (1..=ROW_COUNT as i64).collect();
    let labels: Vec<Option<&str>> = (0..ROW_COUNT)
        .map(|i| if i % 20 == 0 { None } else { Some("row") })
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

    let batch = RecordBatch::try_new(schema.clone(), vec![id_arr, label_arr, score_arr]).unwrap();

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
// WR-06: `capped` means TRUNCATED, not "reached the cap"
// ---------------------------------------------------------------------------

/// A file whose entire content is exactly `ROW_CAP` rows produces a COMPLETE
/// result — nothing was discarded, so `capped` must be false even though
/// `total_rows` equals the cap (WR-06).
#[tokio::test]
async fn test_execute_exactly_100_rows_is_not_capped() {
    let fixture_path = ensure_exact_100_fixture();
    let source = LocalFileSource::new(fixture_path).expect("exact-100 fixture must exist");

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
        meta.total_rows, 100,
        "the exact-100 fixture must return all 100 of its rows, got {}",
        meta.total_rows
    );
    assert!(
        !meta.capped,
        "a COMPLETE result that lands exactly on the 100-row cap was not truncated — \
         `capped` must stay false; it reports truncation, not cap-reached (WR-06)"
    );
}

/// `limit 100` on a 150-row file is the app's own default query. The result is
/// complete with respect to the query, so nothing was truncated by the backend
/// cap and `capped` must be false (WR-06).
#[tokio::test]
async fn test_execute_limit_100_on_larger_file_is_not_capped() {
    let fixture_path = ensure_large_fixture();
    let source = LocalFileSource::new(fixture_path).expect("large fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select * from data limit 100")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.total_rows, 100,
        "`limit 100` must return exactly 100 rows, got {}",
        meta.total_rows
    );
    assert!(
        !meta.capped,
        "`select * from data limit 100` yields a COMPLETE 100-row result — the backend \
         discarded nothing, so `capped` must be false; a complete result at the cap is \
         not truncated (WR-06)"
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
// WR-05 / IN-01: view types must be normalized at EVERY nesting depth, and the
// rewrite must preserve Arrow field- and schema-level metadata.
// ---------------------------------------------------------------------------

/// Recursively reports whether `dt` contains `Utf8View` or `BinaryView` at ANY depth.
///
/// apache-arrow JS v21 throws `"Unrecognized type: undefined (24)"` on a view type
/// wherever it appears — a `List(Utf8View)` child blanks the grid exactly like a
/// top-level `Utf8View` column does. Assertions therefore have to walk the whole tree,
/// not just the top level (WR-05).
fn contains_view_type(dt: &DataType) -> bool {
    match dt {
        DataType::Utf8View | DataType::BinaryView => true,
        DataType::List(f)
        | DataType::LargeList(f)
        | DataType::ListView(f)
        | DataType::LargeListView(f)
        | DataType::FixedSizeList(f, _)
        | DataType::Map(f, _) => contains_view_type(f.data_type()),
        DataType::Struct(fields) => fields.iter().any(|f| contains_view_type(f.data_type())),
        DataType::Dictionary(key, value) => {
            contains_view_type(key) || contains_view_type(value)
        }
        DataType::RunEndEncoded(run_ends, values) => {
            contains_view_type(run_ends.data_type()) || contains_view_type(values.data_type())
        }
        _ => false,
    }
}

/// A `Struct` column with a `Utf8View` child must come back with a `Utf8` child,
/// values intact (WR-05).
#[test]
fn normalize_view_types_downcasts_struct_child_utf8view() {
    use datafusion::arrow::array::{Array, StringArray, StringViewArray, StructArray};
    use datafusion::arrow::datatypes::Fields;

    let inner_fields: Fields = vec![
        Field::new("name", DataType::Utf8View, true),
        Field::new("id", DataType::Int64, false),
    ]
    .into();

    let name_child: Arc<dyn Array> =
        Arc::new(StringViewArray::from(vec![Some("alice"), Some("bob"), None]));
    let id_child: Arc<dyn Array> = Arc::new(Int64Array::from(vec![1i64, 2, 3]));

    let struct_arr: Arc<dyn Array> = Arc::new(StructArray::new(
        inner_fields.clone(),
        vec![name_child, id_child],
        None,
    ));

    let schema = Arc::new(Schema::new(vec![Field::new(
        "person",
        DataType::Struct(inner_fields),
        false,
    )]));

    let batch = RecordBatch::try_new(schema, vec![struct_arr]).unwrap();
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    assert_eq!(result.len(), 1, "must return one batch");
    let out_field_type = result[0].schema().field(0).data_type().clone();

    assert!(
        !contains_view_type(&out_field_type),
        "a Utf8View nested inside a Struct must be normalized away; got {:?}",
        out_field_type
    );

    // The struct's string child must round-trip as a plain Utf8 array.
    let out_struct = result[0]
        .column(0)
        .as_any()
        .downcast_ref::<StructArray>()
        .expect("column 0 must still be a StructArray");
    let name_col = out_struct
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("struct child 0 must be a StringArray after normalization");
    assert_eq!(name_col.value(0), "alice");
    assert_eq!(name_col.value(1), "bob");
    assert!(name_col.is_null(2), "null must survive the nested downcast");
}

/// `List(Utf8View)` must normalize to `List(Utf8)` with offsets and values intact (WR-05).
#[test]
fn normalize_view_types_downcasts_list_of_utf8view() {
    use datafusion::arrow::array::{Array, ListArray, StringArray, StringViewArray};
    use datafusion::arrow::buffer::OffsetBuffer;

    let item_field = Arc::new(Field::new("item", DataType::Utf8View, true));

    let values: Arc<dyn Array> = Arc::new(StringViewArray::from(vec![
        Some("a"),
        Some("b"),
        Some("c"),
        Some("d"),
    ]));
    // Two lists: ["a","b"] and ["c","d"].
    let offsets = OffsetBuffer::new(vec![0i32, 2, 4].into());

    let list_arr: Arc<dyn Array> = Arc::new(ListArray::new(
        Arc::clone(&item_field),
        offsets,
        values,
        None,
    ));

    let schema = Arc::new(Schema::new(vec![Field::new(
        "tags",
        DataType::List(Arc::clone(&item_field)),
        false,
    )]));

    let batch = RecordBatch::try_new(schema, vec![list_arr]).unwrap();
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    let out_field_type = result[0].schema().field(0).data_type().clone();
    assert!(
        !contains_view_type(&out_field_type),
        "a Utf8View nested inside a List must be normalized away; got {:?}",
        out_field_type
    );

    let out_list = result[0]
        .column(0)
        .as_any()
        .downcast_ref::<ListArray>()
        .expect("column 0 must still be a ListArray");
    assert_eq!(out_list.len(), 2, "list offsets must be preserved");

    let flat = out_list
        .values()
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("list values must be a StringArray after normalization");
    let flat_values: Vec<&str> = (0..flat.len()).map(|i| flat.value(i)).collect();
    assert_eq!(
        flat_values,
        vec!["a", "b", "c", "d"],
        "flattened list values must be unchanged"
    );
}

/// `Dictionary(Int32, Utf8View)` must normalize its VALUE type while keeping the
/// key type (WR-05).
#[test]
fn normalize_view_types_downcasts_dictionary_value_utf8view() {
    use datafusion::arrow::array::{Array, DictionaryArray, Int32Array, StringViewArray};

    let keys = Int32Array::from(vec![0i32, 1, 0]);
    let values: Arc<dyn Array> = Arc::new(StringViewArray::from(vec![Some("x"), Some("y")]));
    let dict: Arc<dyn Array> = Arc::new(
        DictionaryArray::<datafusion::arrow::datatypes::Int32Type>::try_new(
            keys,
            Arc::clone(&values),
        )
        .unwrap(),
    );

    let schema = Arc::new(Schema::new(vec![Field::new(
        "code",
        DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8View)),
        false,
    )]));

    let batch = RecordBatch::try_new(schema, vec![dict]).unwrap();
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    let out_field_type = result[0].schema().field(0).data_type().clone();
    assert!(
        !contains_view_type(&out_field_type),
        "a Utf8View dictionary VALUE type must be normalized away; got {:?}",
        out_field_type
    );
    assert_eq!(
        out_field_type,
        DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8)),
        "the dictionary key type must be preserved and only the value type rewritten"
    );
}

/// IN-01: the rewrite path must not silently drop Arrow metadata. A rewrite
/// triggered by ONE view-typed column must leave every other field's metadata and
/// the schema-level metadata intact.
#[test]
fn normalize_view_types_preserves_field_and_schema_metadata() {
    use datafusion::arrow::array::{Array, StringViewArray};
    use std::collections::HashMap;

    let mut schema_meta = HashMap::new();
    schema_meta.insert("file".to_string(), "fixture".to_string());

    let mut field_meta = HashMap::new();
    field_meta.insert("unit".to_string(), "count".to_string());

    let id_field = Field::new("id", DataType::Int64, false).with_metadata(field_meta.clone());
    // The view-typed field is what forces the rewrite path to run at all.
    let name_field = Field::new("name", DataType::Utf8View, true);

    let schema = Arc::new(Schema::new_with_metadata(
        vec![id_field, name_field],
        schema_meta.clone(),
    ));

    let id_arr: Arc<dyn Array> = Arc::new(Int64Array::from(vec![1i64, 2]));
    let name_arr: Arc<dyn Array> =
        Arc::new(StringViewArray::from(vec![Some("alice"), Some("bob")]));

    let batch = RecordBatch::try_new(schema, vec![id_arr, name_arr]).unwrap();
    let result =
        parquet_lens_lib::engine::executor::normalize_view_types(vec![batch]).unwrap();

    let out_schema = result[0].schema();

    assert_eq!(
        out_schema.metadata(),
        &schema_meta,
        "schema-level metadata must survive the view-type rewrite (IN-01)"
    );
    assert_eq!(
        out_schema.field(0).metadata(),
        &field_meta,
        "field-level metadata on an UNCHANGED column must survive a rewrite \
         triggered by a different column (IN-01)"
    );
    assert_eq!(
        out_schema.field(1).data_type(),
        &DataType::Utf8,
        "the view-typed column must still be normalized"
    );
}

// ---------------------------------------------------------------------------
// Phase 1 Plan 01 Task 1: RunQueryResponse carries the QUERY RESULT schema
// ---------------------------------------------------------------------------

/// ROADMAP Success Criterion 4 (automated form): for `select * from data` the
/// result schema reported by `execute()` must be field-for-field identical to the
/// file schema reported by `get_schema()`.
///
/// Both labels are produced by the SAME `format!("{:?}", field.data_type())`
/// expression (`context.rs::get_schema` and `executor.rs::execute`), so parity is
/// true by construction for every Arrow type — including nested, dictionary,
/// decimal, temporal and view types. This test pins that contract.
#[tokio::test]
async fn test_execute_result_schema_matches_file_schema_for_select_star() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let file_schema = engine.get_schema().expect("get_schema must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select * from data")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.schema.len(),
        file_schema.len(),
        "ROADMAP Success Criterion 4: `select *` result schema must have the same \
         column count as the file schema ({} result vs {} file)",
        meta.schema.len(),
        file_schema.len()
    );

    for (i, (result_field, file_field)) in meta.schema.iter().zip(file_schema.iter()).enumerate() {
        assert_eq!(
            result_field.name, file_field.name,
            "ROADMAP Success Criterion 4: column {} name must match the file schema",
            i
        );
        assert_eq!(
            result_field.arrow_type, file_field.arrow_type,
            "ROADMAP Success Criterion 4: column {} ('{}') arrow_type must be \
             byte-identical to the file schema label — both must come from the same \
             format!(\"{{:?}}\", data_type) expression",
            i, file_field.name
        );
        assert_eq!(
            result_field.nullable, file_field.nullable,
            "ROADMAP Success Criterion 4: column {} ('{}') nullable must match the file schema",
            i, file_field.name
        );
    }
}

/// RESULT-01/RESULT-03: an aggregate result reports its OWN column, not the file's.
#[tokio::test]
async fn test_execute_result_schema_for_count_star() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select count(*) from data")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.schema.len(),
        1,
        "count(*) must produce exactly one result column, got {}",
        meta.schema.len()
    );
    assert_eq!(
        meta.schema[0].name, "count(*)",
        "count(*) result column must carry DataFusion's raw output name"
    );
    assert_eq!(
        meta.schema[0].arrow_type, "Int64",
        "count(*) result column must be labelled Int64, not the source column's type"
    );
}

/// RESULT-03: an aliased projection reports the alias, not the source column name.
#[tokio::test]
async fn test_execute_result_schema_for_alias() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select id as label from data")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.schema.len(),
        1,
        "an aliased single-column projection must produce one result column, got {}",
        meta.schema.len()
    );
    assert_eq!(
        meta.schema[0].name, "label",
        "the result column must carry the alias `label`, not the source column name"
    );
}

/// The schema comes from the executed stream, not from the retained batches —
/// so a zero-row result still knows its columns and the grid can render headers.
#[tokio::test]
async fn test_execute_result_schema_present_for_empty_result() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select * from data where id < 0")
        .await
        .expect("execute must succeed");

    assert_eq!(
        meta.total_rows, 0,
        "an always-false predicate must return zero rows, got {}",
        meta.total_rows
    );
    assert!(
        !meta.schema.is_empty(),
        "a zero-row result must still carry a fully populated result schema"
    );
}

// ---------------------------------------------------------------------------
// WR-04 layer 2: plan-level backstop (SQLOptions) inside the executor
//
// These tests call `execute` DIRECTLY, deliberately bypassing `guard_select_only`.
// That is the whole point: layer 1 lives in commands/query.rs, and this layer must
// hold on its own for anything that reaches the engine another way.
// ---------------------------------------------------------------------------

/// `SELECT ... INTO` must be refused at PLAN level and must leave no session table
/// behind (WR-04).
///
/// The second assertion is the decisive one: DataFusion plans `SELECT ... INTO` as
/// `CreateMemoryTable`, so without the `SQLOptions` backstop the statement executes
/// and a subsequent `select * from t` SUCCEEDS against the table it created.
#[tokio::test]
async fn test_execute_rejects_select_into_at_plan_level() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let result = engine.execute("select * into t from data").await;

    assert!(
        result.is_err(),
        "SELECT ... INTO plans as CreateMemoryTable and must be rejected at plan level (WR-04)"
    );
    let msg = result.err().unwrap_or_default();
    assert!(
        msg.contains("SQL planning error"),
        "the rejection must surface through the existing planning-error wrapper so the \
         frontend error surface is unchanged; got '{msg}'"
    );

    // Decisive: no session table may have been created by the rejected statement.
    let follow_up = engine.execute("select * from t").await;
    assert!(
        follow_up.is_err(),
        "the rejected SELECT ... INTO must NOT have created a session table `t` — \
         a successful `select * from t` proves the DDL side effect happened (WR-04)"
    );
}

/// The `SQLOptions` backstop must not narrow what ordinary read-only SQL can do:
/// aggregates still plan, execute and report their own result schema.
#[tokio::test]
async fn test_execute_select_still_works_with_sql_options() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("select count(*) from data")
        .await
        .expect("an aggregate SELECT must still execute under sql_with_options");

    assert_eq!(
        meta.total_rows, 1,
        "count(*) must return exactly one row, got {}",
        meta.total_rows
    );
    assert_eq!(
        meta.schema.len(),
        1,
        "count(*) must report exactly one result-schema field, got {}",
        meta.schema.len()
    );
    assert_eq!(
        meta.schema[0].arrow_type, "Int64",
        "the count(*) result column must still be labelled Int64 under sql_with_options"
    );
}

/// A CTE is a read-only construct and must survive the backstop unchanged.
#[tokio::test]
async fn test_execute_cte_still_works_with_sql_options() {
    let fixture_path = small_fixture_path();
    let source = LocalFileSource::new(fixture_path).expect("small fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed");

    let (meta, _ipc_bytes) = engine
        .execute("with c as (select * from data) select count(*) from c")
        .await
        .expect("a read-only CTE must still execute under sql_with_options");

    assert_eq!(
        meta.total_rows, 1,
        "the CTE aggregate must return exactly one row, got {}",
        meta.total_rows
    );
}

// ---------------------------------------------------------------------------
// WR-02 (phase 02 review): a stream error during the CAP PEEK must not fail an
// already-complete 100-row result.
// ---------------------------------------------------------------------------

/// A `PartitionStream` that yields one exactly-at-the-cap batch and then a stream
/// error — the precise shape where the executor's peek loop observes an error in
/// a batch it would have DROPPED anyway.
#[derive(Debug)]
struct ErrAfterCapStream {
    schema: datafusion::arrow::datatypes::SchemaRef,
}

impl datafusion::physical_plan::streaming::PartitionStream for ErrAfterCapStream {
    fn schema(&self) -> &datafusion::arrow::datatypes::SchemaRef {
        &self.schema
    }

    fn execute(
        &self,
        _ctx: Arc<datafusion::execution::TaskContext>,
    ) -> datafusion::execution::SendableRecordBatchStream {
        use datafusion::physical_plan::stream::RecordBatchStreamAdapter;

        let ids: Arc<dyn Array> = Arc::new(Int64Array::from((1..=100i64).collect::<Vec<_>>()));
        let batch = RecordBatch::try_new(Arc::clone(&self.schema), vec![ids])
            .expect("cap-sized batch must build");

        let items = vec![
            Ok(batch),
            Err(datafusion::error::DataFusionError::Execution(
                "simulated corruption past the cap".to_string(),
            )),
        ];
        Box::pin(RecordBatchStreamAdapter::new(
            Arc::clone(&self.schema),
            futures::stream::iter(items),
        ))
    }
}

/// The slice branch never observes errors past the cap (it breaks immediately),
/// so the peek branch must not fail on them either — the same data with the same
/// corruption past row 100 must not succeed or fail depending only on how the
/// source chunked its batches. The peek error is treated as end-of-stream and
/// `capped` is reported `true` conservatively (WR-02).
#[tokio::test]
async fn test_execute_peek_error_past_cap_does_not_fail_result() {
    use datafusion::catalog::streaming::StreamingTable;

    let schema: datafusion::arrow::datatypes::SchemaRef =
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

    let table = StreamingTable::try_new(
        Arc::clone(&schema),
        vec![Arc::new(ErrAfterCapStream {
            schema: Arc::clone(&schema),
        })],
    )
    .expect("StreamingTable::try_new must succeed");

    let mut engine = QueryEngine::new();
    engine
        .ctx()
        .register_table("err_data", Arc::new(table))
        .expect("register_table must succeed");

    let (meta, ipc_bytes) = engine.execute("select * from err_data").await.expect(
        "an error in a batch past the cap must not discard the 100 already-retained \
         rows (WR-02)",
    );

    assert_eq!(
        meta.total_rows, 100,
        "all 100 retained rows must survive the peek error, got {}",
        meta.total_rows
    );
    assert!(
        meta.capped,
        "capped must be reported true conservatively when the peek hits an error — \
         completeness cannot be proven (WR-02)"
    );
    assert!(
        !ipc_bytes.is_empty(),
        "IPC bytes must be non-empty for the retained 100-row result"
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
