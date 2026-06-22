//! Local filesystem implementation of `DataSource`.
//!
//! `LocalFileSource` wraps a validated file path and implements the `DataSource` trait
//! using `object_store::local::LocalFileSystem`. The v2 remote implementation follows
//! the same pattern with a different `ObjectStore` impl.
//!
//! The path arriving here is a plain absolute path produced by `commands::file::normalise_path`,
//! which canonicalises the path and strips any Windows `\\?\` verbatim prefix so that
//! `ListingTableUrl::parse` (via `Url::from_file_path`) can convert it to a `file://` URL
//! (PITFALLS.md §Pitfall 6).

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::execution::object_store::ObjectStoreUrl;
use datafusion::datasource::listing::ListingTableUrl;
use object_store::local::LocalFileSystem;
use object_store::ObjectStore;

use super::DataSource;

/// A Parquet file on the local filesystem, ready to be registered with DataFusion.
#[derive(Debug)]
pub struct LocalFileSource {
    path: PathBuf,
}

impl LocalFileSource {
    /// Creates a new `LocalFileSource` from a validated path.
    ///
    /// Returns `Err` if the path does not exist — no panic on missing files.
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("File not found: {}", path.display()));
        }
        Ok(Self { path })
    }
}

impl DataSource for LocalFileSource {
    fn url(&self) -> ObjectStoreUrl {
        ObjectStoreUrl::local_filesystem()
    }

    fn object_store(&self) -> Arc<dyn ObjectStore> {
        Arc::new(LocalFileSystem::new())
    }

    fn table_path(&self) -> Result<ListingTableUrl, String> {
        // Use the verbatim-prefixed path that was already applied in the command layer.
        // ListingTableUrl::parse keeps the URL-shaped path convention consistent for v2 remote
        // (PITFALLS.md §Pitfall 10).
        ListingTableUrl::parse(self.path.to_string_lossy().as_ref())
            .map_err(|e| format!("Failed to parse path as ListingTableUrl: {}", e))
    }
}
