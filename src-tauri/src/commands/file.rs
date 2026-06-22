//! `open_file` Tauri command — opens a Parquet file and registers it as SQL table `data`.
//!
//! Normalises the incoming path before constructing `LocalFileSource` to handle both
//! long paths (PITFALLS.md §Pitfall 6) and non-ASCII folder names (§Pitfall 6, encoding).
//! Never panics — all errors are propagated as `Err(String)` (PITFALLS.md §Pitfall 9).

use std::path::PathBuf;

use serde::Deserialize;

use crate::ipc::{FileMetadata, OpenFileResponse};
use crate::state::AppState;
use crate::storage::{LocalFileSource, RemoteS3Source};

/// Connection parameters for a remote MinIO/S3 Parquet object.
///
/// # Credential Safety
///
/// `Debug` is deliberately NOT derived on this struct to prevent `secret_access_key`
/// from appearing in log output or error messages (RESEARCH §Pitfall 3, T-05-01).
/// Never use `format!("{:?}", conn)` on this struct.
#[derive(Deserialize)]
pub struct RemoteConnection {
    pub endpoint: String,
    pub bucket: String,
    pub object_key: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Normalises a filesystem path for safe use with `LocalFileSource` and `ListingTableUrl`.
///
/// On Windows, `std::fs::canonicalize` already returns the `\\?\`-prefixed verbatim
/// extended-length form (e.g. `\\?\D:\foo\bar.parquet`). `ListingTableUrl::parse` (and
/// `Url::from_file_path` underneath it) cannot handle the verbatim prefix, so we strip
/// `\\?\` (and the UNC variant `\\?\UNC\`) after canonicalisation and return a plain
/// absolute path.  The plain absolute path still passes `path.exists()` and still lets
/// DataFusion's `LocalFileSystem` open files in deep directory trees, because the Windows
/// I/O subsystem accepts both forms interchangeably for read-only access.
///
/// On non-Windows platforms the path is canonicalised but otherwise returned unchanged.
pub fn normalise_path(raw: &str) -> Result<PathBuf, String> {
    let canonical = std::path::Path::new(raw)
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize path '{}': {}", raw, e))?;

    // On Windows, canonicalize() yields a verbatim extended-length path such as:
    //   \\?\D:\some\path\file.parquet        (local)
    //   \\?\UNC\server\share\file.parquet    (UNC)
    //
    // Url::from_file_path (called inside ListingTableUrl::parse) cannot handle the
    // verbatim prefix, so we strip \\?\ (or \\?\UNC\) and return a plain absolute path.
    // Windows accepts both forms interchangeably for read-only file access.
    //
    // On non-Windows platforms the canonical path is returned unchanged.
    #[cfg(target_os = "windows")]
    let normalised = {
        let s = canonical.to_string_lossy();
        let plain = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            // Reconstruct as a normal UNC path: \\server\share\...
            format!(r"\\{}", rest)
        } else if let Some(rest) = s.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            s.into_owned()
        };
        PathBuf::from(plain)
    };

    #[cfg(not(target_os = "windows"))]
    let normalised = canonical;

    Ok(normalised)
}

/// Opens a Parquet file at `path`, registers it as SQL table `data` in DataFusion,
/// and returns the inferred Arrow schema.
///
/// The path is canonicalised and, on Windows, the `\\?\` verbatim prefix returned by
/// `canonicalize()` is stripped so that `ListingTableUrl::parse` can convert the path
/// to a `file://` URL (PITFALLS.md §Pitfall 6).
#[tauri::command]
pub async fn open_file(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<OpenFileResponse, String> {
    let normalised = normalise_path(&path)?;

    let source = LocalFileSource::new(normalised)?;

    let mut engine = state.engine.lock().await;
    engine.register_source(&source).await?;

    let schema = engine.get_schema()?;
    Ok(OpenFileResponse { schema })
}

/// Returns Parquet footer metadata for the currently registered file.
///
/// Reads row-group statistics cached on `QueryEngine` during `register_source`.
/// Returns `Err(String)` if no file is registered (PITFALLS.md §Pitfall 9 — no unwrap).
///
/// Arrow/Parquet types accessed via `datafusion::parquet::*` — no direct parquet crate
/// dependency (PITFALLS.md §Pitfall 3).
#[tauri::command]
pub async fn get_file_metadata(
    state: tauri::State<'_, AppState>,
) -> Result<FileMetadata, String> {
    let engine = state.engine.lock().await;
    engine.get_file_metadata()
}

/// Opens a remote Parquet object at an S3/MinIO endpoint, registers it as SQL table `data`,
/// and returns the inferred Arrow schema.
///
/// Mirrors `open_file` exactly — constructs a `RemoteS3Source` instead of `LocalFileSource`,
/// then delegates to the unchanged `engine.register_source`. The engine is completely
/// storage-agnostic (SPEC constraint: `engine/context.rs` must NOT be modified).
///
/// All errors propagate as `Err(String)` prefixed `"Failed to open remote object: <cause>"`.
/// Credentials are never logged or included in error messages (T-05-01).
#[tauri::command]
pub async fn open_remote_file(
    conn: RemoteConnection,
    state: tauri::State<'_, AppState>,
) -> Result<OpenFileResponse, String> {
    // Unpack fields individually — never format!("{:?}", conn) which would expose secret_access_key
    let source = RemoteS3Source::new(
        &conn.endpoint,
        &conn.bucket,
        &conn.object_key,
        &conn.access_key_id,
        &conn.secret_access_key,
    )?;

    let mut engine = state.engine.lock().await;
    // Wrap the engine's storage-agnostic errors (e.g. "Failed to infer table config: …")
    // with the remote prefix so every remote-open failure reads
    // "Failed to open remote object: <cause>" per REMOTE-02 / D-06. The engine layer is
    // storage-agnostic (engine/context.rs must NOT be modified), so the remote framing
    // lives here at the command boundary. The raw object_store cause carries no
    // credential (T-05-01).
    engine
        .register_source(&source)
        .await
        .map_err(|e| format!("Failed to open remote object: {}", e))?;

    let schema = engine
        .get_schema()
        .map_err(|e| format!("Failed to open remote object: {}", e))?;
    Ok(OpenFileResponse { schema })
}
