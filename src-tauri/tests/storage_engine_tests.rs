//! Behavior tests for the storage seam + query engine (Plan 01, Task 2).
// Tests legitimately use unwrap/expect for test assertions — this is idiomatic in Rust test code.
#![allow(clippy::unwrap_used)]
//!
//! Tests follow the TDD RED→GREEN→REFACTOR cycle defined in the plan.
//!
//! Test 1 (storage seam): `LocalFileSource::new` on non-existent path returns `Err` (not panic);
//!   on an existing path returns `Ok` and `table_path()` yields a parseable `ListingTableUrl`.
//! Test 2 (engine registration): given a real sample `.parquet` file,
//!   `QueryEngine::register_source(&LocalFileSource)` succeeds and a subsequent
//!   `ctx.sql("select * from data limit 1")` resolves the `data` table without error.
//! Test 3 (schema): after `register_source`, `QueryEngine::get_schema()` returns a non-empty
//!   `Vec<SchemaField>` whose `arrow_type` strings are Arrow-native (e.g. "Int64", "Utf8").
//! Test 4 (path regression): `normalise_path` applied to the fixture's plain path must produce
//!   a path accepted by `LocalFileSource::new` and must complete register+get_schema end-to-end.
//!   This is the regression test that would have caught the double `\\?\` prefix bug.

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::{Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use parquet_lens_lib::commands::file::normalise_path;
use parquet_lens_lib::engine::QueryEngine;
use parquet_lens_lib::storage::{DataSource, LocalFileSource};

/// Path to the test fixture Parquet file.
const FIXTURE_PATH: &str = "tests/fixtures/sample.parquet";

/// Creates (or verifies) the test fixture file.
///
/// Generates a minimal 3-column, 5-row Parquet file using DataFusion's arrow
/// re-exports. Returns the absolute path to the fixture.
fn ensure_fixture() -> PathBuf {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_PATH);

    if !fixture_path.exists() {
        // Create the fixture using arrow + parquet (via datafusion re-exports).
        create_fixture(&fixture_path);
    }

    fixture_path
}

/// Writes a minimal Parquet fixture: 3 columns (id: Int64, name: Utf8, value: Int64), 5 rows.
fn create_fixture(path: &PathBuf) {
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

    let batch =
        RecordBatch::try_new(schema.clone(), vec![ids, names, values]).unwrap();

    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

// ---------------------------------------------------------------------------
// Test 1: LocalFileSource::new — Err on missing path, Ok on existing path
// ---------------------------------------------------------------------------

#[test]
fn test_local_file_source_missing_path_returns_err() {
    let path = PathBuf::from("/nonexistent/path/that/does/not/exist.parquet");
    let result = LocalFileSource::new(path);
    assert!(
        result.is_err(),
        "Expected Err for non-existent path, got Ok"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("File not found"),
        "Error message should contain 'File not found', got: {}",
        err
    );
}

#[test]
fn test_local_file_source_existing_path_returns_ok() {
    let fixture_path = ensure_fixture();
    let result = LocalFileSource::new(fixture_path.clone());
    assert!(
        result.is_ok(),
        "Expected Ok for existing path '{}', got Err: {:?}",
        fixture_path.display(),
        result.err()
    );
}

#[test]
fn test_local_file_source_table_path_parseable() {
    let fixture_path = ensure_fixture();
    let source = LocalFileSource::new(fixture_path).expect("fixture must exist");
    // table_path() returns a Result<ListingTableUrl, String> — Ok means parseable.
    let table_path_result = source.table_path();
    assert!(
        table_path_result.is_ok(),
        "table_path() should return Ok for a valid path, got: {:?}",
        table_path_result.err()
    );
}

// ---------------------------------------------------------------------------
// Test 2: QueryEngine::register_source — table `data` registered, SQL resolves
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_register_source_and_sql_resolves_data_table() {
    let fixture_path = ensure_fixture();
    let source = LocalFileSource::new(fixture_path).expect("fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source should succeed");

    // Verify the `data` table is accessible via SQL.
    let df = engine
        .ctx()
        .sql("SELECT * FROM data LIMIT 1")
        .await
        .expect("SQL should resolve table 'data' without error");

    let batches = df.collect().await.expect("collect should succeed");
    assert!(
        !batches.is_empty(),
        "Expected at least one batch from 'SELECT * FROM data LIMIT 1'"
    );
}

// ---------------------------------------------------------------------------
// Test 3: QueryEngine::get_schema — non-empty Vec<SchemaField>, Arrow-native types
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_schema_returns_non_empty_arrow_native_types() {
    let fixture_path = ensure_fixture();
    let source = LocalFileSource::new(fixture_path).expect("fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source should succeed");

    let schema = engine.get_schema().expect("get_schema should succeed");

    assert!(
        !schema.is_empty(),
        "Expected non-empty schema, got empty Vec"
    );

    // Verify arrow_type strings look like Arrow-native names (not empty).
    for field in &schema {
        assert!(
            !field.arrow_type.is_empty(),
            "arrow_type for field '{}' should not be empty",
            field.name
        );
        assert!(
            !field.name.is_empty(),
            "field name should not be empty"
        );
    }

    // The fixture has columns: id (Int64), name (Utf8), value (Int64).
    // Verify at least one of them is present with the expected Arrow type.
    let id_field = schema.iter().find(|f| f.name == "id");
    assert!(id_field.is_some(), "Schema should contain field 'id'");
    let id_field = id_field.unwrap();
    assert!(
        id_field.arrow_type.contains("Int64"),
        "Field 'id' should have Arrow type containing 'Int64', got: {}",
        id_field.arrow_type
    );
}

// ---------------------------------------------------------------------------
// Test 4: End-to-end path regression — normalise_path → LocalFileSource → get_schema
//
// This test exercises the EXACT same transform that `open_file` uses:
//   normalise_path(raw_string) → LocalFileSource::new → register_source → get_schema
//
// On Windows, `normalise_path` canonicalises the path (which yields a `\\?\`-prefixed
// verbatim path) and then strips the `\\?\` prefix.  The resulting plain absolute path
// must be accepted by `LocalFileSource::new` (exists check) AND by DataFusion's
// `ListingTableUrl::parse` inside `register_source`.
//
// This is the regression test that would have caught the double `\\?\` prefix bug
// (where the old code applied `format!("\\\\?\\{}", canonical)` on top of a path
// that `canonicalize()` had already prefixed with `\\?\`).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Test 5 (D-04/D-05 regression guard): failed re-open preserves the previous table
//
// A failed `register_source` call (invalid/non-Parquet path) must leave `self.ctx`,
// `self.cached_schema`, and `self.cached_file_metadata` untouched. The previous `data`
// table must remain fully queryable after the failed second registration.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_normalise_path_end_to_end_open() {
    let fixture_path = ensure_fixture();

    // Simulate what the Tauri command receives: a plain string path from the OS dialog.
    let raw = fixture_path.to_string_lossy().into_owned();

    // Apply the same transform the command uses.
    let normalised = normalise_path(&raw)
        .expect("normalise_path should succeed on the fixture path");

    // On Windows the verbatim \\?\ prefix must have been stripped — the resulting
    // path must NOT start with \\?\.
    let normalised_str = normalised.to_string_lossy();
    assert!(
        !normalised_str.starts_with(r"\\?\"),
        "normalise_path must strip the \\?\\ verbatim prefix; got: {}",
        normalised_str
    );

    // The normalised path must exist (basic sanity check before passing to LocalFileSource).
    assert!(
        normalised.exists(),
        "normalised path must still point to an existing file: {}",
        normalised_str
    );

    // Full chain: LocalFileSource → register_source → get_schema.
    let source = LocalFileSource::new(normalised)
        .expect("LocalFileSource::new must accept the normalised path");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("register_source must succeed with a normalised path");

    let schema = engine
        .get_schema()
        .expect("get_schema must succeed after registration");

    assert!(
        !schema.is_empty(),
        "schema must be non-empty — the full open chain must work end-to-end"
    );
}

#[tokio::test]
async fn failed_reopen_preserves_previous_table() {
    // Step 1: Register a valid Parquet file successfully.
    let fixture_path = ensure_fixture();
    let source = LocalFileSource::new(fixture_path).expect("fixture must exist");

    let mut engine = QueryEngine::new();
    engine
        .register_source(&source)
        .await
        .expect("first register_source must succeed");

    // Capture the first file's schema field count so we can verify it survives.
    let first_schema = engine.get_schema().expect("get_schema must succeed after first registration");
    assert!(!first_schema.is_empty(), "first schema must be non-empty");
    let first_field_count = first_schema.len();

    // Verify file metadata is also cached after successful registration.
    engine.get_file_metadata().expect("get_file_metadata must succeed after first registration");

    // Step 2: Attempt to register an invalid (non-existent) path — this must return Err.
    // LocalFileSource::new checks file existence, so we build the source manually by using a
    // path that exists as a file but is not a valid Parquet (we create an empty temp file).
    let temp_dir = std::env::temp_dir();
    let invalid_path = temp_dir.join("not_a_parquet_file.txt");
    std::fs::write(&invalid_path, b"this is not parquet content").expect("write temp file");

    let invalid_source = LocalFileSource::new(invalid_path.clone())
        .expect("LocalFileSource::new must accept the temp file path");
    let second_result = engine.register_source(&invalid_source).await;

    // Clean up the temp file.
    let _ = std::fs::remove_file(&invalid_path);

    // The second registration must fail — the file is not a valid Parquet file.
    assert!(
        second_result.is_err(),
        "register_source on a non-Parquet file must return Err"
    );
    let err_msg = second_result.unwrap_err();
    assert!(!err_msg.is_empty(), "error message must be non-empty");

    // Step 3 (D-04/D-05 guard): the first file must still be registered after the failed open.
    // get_schema must return Ok (NOT the "No file registered" error), with the same field count.
    let surviving_schema = engine
        .get_schema()
        .expect("get_schema must return Ok after failed re-open — previous table must survive (D-04)");
    assert_eq!(
        surviving_schema.len(),
        first_field_count,
        "schema field count after failed re-open must match the first file's field count"
    );

    // get_file_metadata must also return Ok (cached metadata from the first file survived).
    engine
        .get_file_metadata()
        .expect("get_file_metadata must return Ok after failed re-open — previous metadata must survive (D-05)");
}
